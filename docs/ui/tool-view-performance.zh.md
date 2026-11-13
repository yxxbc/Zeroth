# Ctrl+O 性能：增量式 transcript 缓存与原子切换

## 问题

在一个**恢复回来的大会话**上打开 Ctrl+O 的 transcript overlay，又慢又难看。加载
一份真实的 146 KB rollout（一个会话，它的 `write` tool 带过 15–20 KB 的 HTML
文件）然后按 Ctrl+O：

- overlay 花了约 270 ms 才出现（在 190×45 的 tmux 里测的；慢机器上更糟）——
  `ui::transcript_build` 走遍*整个* `history`，并把每个 tool cell 都**展开**
  渲染，那意味着用 syntect 为每个写出的文件做语法高亮。五个 `Write` cell 就占了
  其中约 180 ms（带内嵌 CSS/JS 的 HTML 是 syntect 最慢的语法）。这个构建**每次
  打开**都重跑一遍，因为 overlay 一关缓存就被释放了。
- 这个构建跑在终端**已经切换之后**：`enter_overlay` 先冲刷 cursor-hide +
  `EnterAlternateScreen` + clear，然后 `draw_tool_view` 才开始那次 O(history)
  的构建。用户在整段构建期间盯着一块**空白的 alternate screen** —— 而在 kitty
  里，缓冲区切换会移动光标，所以 kitty 的光标拖尾动画在 overlay 出现之前明显
  地划过空屏。

## 修法，两半

### 1. 切换是原子的（`term::enter_overlay` 只入队，绝不冲刷）

`enter_overlay` 现在只在 backend 上**入队** `Hide` + `EnterAlternateScreen` +
`Clear`，然后不冲刷就返回。每个调用方紧接着绘制 overlay（`draw_tool_view` /
`draw_context_view` / `draw_resume_picker`），而 `draw_overlay` 的冲刷把这次
切换与已完整绘制的第一帧作为**一次写**交付：终端从内联的实时帧直接跳到画好的
overlay。不再有任何被冲刷出来的空白 alternate screen 状态 —— 没有东西可供光标
拖尾动画划过，没有黑屏一闪 —— 而如果第一帧碰巧构建得慢，那次停顿发生在*任何*
终端写之前，屏幕上内联视图（composer、页脚、光标）仍然完好。

光标 `Hide` 排在入队的切换之前，所以任何被冲刷的状态都不会在跳转中途露出光标
（`draw_overlay` 在它的同步更新里重申这次隐藏；内联重排在 `exit_overlay` 之后
把它重新放到 prompt 上并重新显示 —— term.rs 的模块文档描述了这套纪律）。

不过，光隐藏在每个终端上都不够：有些终端不管光标可见与否都会给它的**移动**做
动画，而一次全屏的单元格绘制过去会把（隐藏的）光标留在最后一个格子落下的地方
—— 空白的右下角 —— 于是打开 Ctrl+O / Ctrl+D 就把动画从 composer 的 `❯` 划到
了不知何处（被报告的那个 bug）。因此 `draw_overlay` 每帧结尾都经纯的
`ui::overlay_cursor_seat` 把光标**安置**在该帧最后的文本上 —— 就在收尾的
`q/esc/… to quit` 提示之后 —— 与内联 `paint_frame` 经 `ui::cursor_position`
把它放在 prompt 上、以及 `cursor_visible` 的「隐藏但仍安置」规则让菜单的座位
保持合理，做法完全一致。座位是帧的属性而不是滚动的属性，所以翻页 transcript
从不移动它 —— 进去跳一次（跳到提示上），出来跳一次（跳回 prompt 上）。

用真实会话在 tmux 里做了 A/B 验证（旧 → 新）：overlay 可见时间 273 ms → 14 ms
（那是抓取轮询的下限），而在按键后立刻抓的一帧显示，旧构建是空屏、新构建已经是
画好的 overlay。

### 2. transcript 构建是增量的（`ui::TranscriptCache`）

签名守卫的全量重建被替换成一种**冻结前缀**的增量构建（`StreamRender` 的模式，
用在 overlay 上）：

- **逐条目渲染。** `transcript_item_lines` 渲染一个历史条目 —— 消息加时间戳戳，
  或者 tool 的展开 cell，或者一条摘要/后台 notice —— 加上它尾部的间隔（在一个
  shell 头之后跳过，因为它的 tool cell 紧贴其上），并报告回溯预览可能反转的那段
  用户消息行跨度。`transcript_build` 本身现在只是一个薄的「新建缓存、刷新一次」
  包装，所以缓存路径与从零渲染共享同一条代码路径，永远不会漂移。
- **冻结前缀。** 缓存的头 `frozen_rows` 行装着横幅装饰加上每一个已提交条目，
  每个恰好渲染一次。可靠性来自 `App::history_generation`：已提交条目不可变，
  而历史只会增长，且每一次非追加的改动 —— `/clear`、一次 `/resume` 加载、一次
  回溯截断、一次打断撤销的弹出 —— 都会推进这个代际。缓存按
  `(generation, width, session cwd)` 钉住它的前缀；那个易变的 `TranscriptSig`
  也携带代际，因为一次弹出再压入可能让历史落在*同样的长度*上却是不同的内容
  （`transcript_cache_rebuilds_when_history_is_replaced_at_the_same_length`
  这个测试）。
- **刷新 = 截断 + 追加。** 一次刷新把易变的尾部（进行中的回复、开着的思考阶段
  —— `docs/ui/thinking-stream.md` —— 实时 tool 队列、排队中的积压）从末端切掉，
  把任何新提交的条目追加到前缀，然后只重建尾部。一个流出的 chunk 只重渲染尾部；
  一次滚动（签名不变）什么都不渲染。
- **回溯高亮是一次样式差异。** Esc-Esc 预览那些 REVERSED 的行就住在冻结前缀
  *内部*；移动选择会把旧的那一段反反转、把新的一段就地反转（按条目的行偏移
  跟踪），与从零构建所打出的样式逐字节相同 —— 绝不是一次重渲染。
- **关闭之后仍保留。** overlay 退出不再释放缓存：正是冻结前缀让下一次打开是瞬时
  的。内存占用是当前对话的那份紧凑渲染后 transcript（与 `history` 本身同一个
  顺序）；一次代际推进就整份丢掉。

### 紧凑的行保住了热缓存

保留展开的 ratatui 行，会让每一个高亮的 token 各占一次文本分配外加一个完整的
`Style`。一个代码密集的会话即使 overlay 关着也会留住几千个这样的对象。
`ui::cached_lines::CachedLines` 现在把每个渲染条目的文本打包进一块共享的 UTF-8
slab，并把重复的样式内部化。span 长度和样式下标用无损的 base-128 `usize` 编码；
行偏移允许直接访问任何 viewport 而不必扫描前面的行。空 span、行对齐、行样式以及
不可见的图片/链接载体都精确往返。回溯选择只改那一行的样式下标。

行与 span 各自计数它们对内部样式的引用。移除活的尾部或改变选择，会释放未使用的
尾部样式条目，所以反复刷新不会累积独一无二的链接/图片载体。未使用的中间条目保留
它们的 ID，而后来的样式仍在被使用；缓冲区容量保留以便复用。这为每个内部样式
增加一个引用计数，而不是每个 span 一个。

`TranscriptCache::render` 只展开可见窗口，从 slab 借用它的文本。打开、滚动和再次
打开都沿用同一个已预热的前缀，不需要任何高亮。公开的 `lines` 访问器仍然服务那些
明确需要一份完整自有 transcript 的调用方；交互式 pager 从不把它实体化，并在绘制
时丢掉更早的那个兼容视图。一次新的提交或一次缓存失效也会丢掉那份自有视图。

按需重建 transcript 能省下 overlay 关闭时的存储，但会把上面说的慢首开带回来。
保留紧凑的已渲染行保住了那个性能保证。渲染单个历史条目仍然使用临时的展开行，
所以一个巨大的条目仍会影响分配器的高水位内存；这次改动主要去掉的是跨条目的累积。

### 3. 缓存在边界处预热（`TranscriptCache::warm`）

渲染一个条目的代价与在哪儿渲染无关 —— 所以就在用户感觉不到的地方做。循环底部
（在 `recorder.sync` 旁边）每次迭代都调用 `transcript.warm(&app, width)`：

- 没有提交任何东西 → 几次整数比较；
- 一个回合提交了条目 → 每个新条目一次语法高亮，摊到产生它们的那个回合里；
- 一次 `/resume` 加载换掉了历史（一次代际推进）→ 整个加载的会话渲染一次，就在
  加载本身已经很重的那一刻（同一次迭代还会 purge 重绘内联视图），146 KB 的会话
  约 240 ms。

所以 Ctrl+O 那一次按键本身永远是 O(live tail) —— 在测过的那个会话上预热之后约
1 µs —— 而 overlay 永远不会冷着打开。

**宽度是唯一一处有意的例外**：`warm` 会跳过纯粹因宽度引起的不匹配。尺寸变化事件
是成批到达的（一次拖动会送来几十个），而每个事件一次 O(history) 的全量重渲染会
把循环冻住；于是新宽度下的第一次 overlay 绘制承担那一次重建 —— 躲在仍然画着的
内联屏幕后面，靠的是第 1 半。代际变化会在当前宽度下重新预热，所以陈旧宽度的状态
也不可能活过下一次真实的改动。

## 什么守着它

- `app::tests::history_generation_bumps_on_every_non_append_mutation` ——
  冻结前缀的可靠性不变量。
- `ui::tests::transcript_cache_appends_new_items_without_rerendering_frozen_ones`
  以及 `…_streams_a_reply_without_rerendering_history`、
  `…_backtrack_selection_restyles_without_rerendering`、
  `…_rebuilds_when_history_is_replaced_at_the_same_length`、
  `…_warm_prebuilds_so_the_open_renders_nothing` —— 增量行为，每个都断言与一次
  从零构建逐字节相等（`item_renders` / `builds` 是证明复用的测试专用计数器）。
- 既有的那些 transcript 测试（交错、活尾部、排队积压、占位符、时间戳戳、选择）
  全部走同一条代码路径。
- `scripts/smoke.sh` Phase 48 —— 在 tmux 里恢复一个代码密集的会话，Ctrl+O，
  断言展开的 transcript 迅速出现且内联视图完好地回来（它是两个边界的半边：入队
  的切换与循环底部的预热）。

## 本文*不*覆盖的：绘制

上面一切都是关于 transcript 的**行**建得有多快。之后它们会怎样 —— 那些 cell 里
究竟有多少真正到达终端 —— 是另一个机制，而有很长一段时间它把这份节省里的大半
又毁掉了：`draw_overlay` 每一帧都把 alternate screen 的每个 cell 重新序列化
一遍，
所以一次没渲染任何新东西的缓存命中仍然写了满满一屏。比白干活更糟，那次写毁掉了
用户的鼠标选择，使得有回合在流式输出时 overlay 的文本无法复制。

现在 overlay 会与屏幕上已有的那一帧做 diff，而未变的帧什么都不写。两个机制组合
起来恰恰如你所愿：一次缓存命中考出逐字节相同的缓冲区，diff 随即把它判为
`Unchanged`，到达终端时就是沉默。见 `docs/ui/overlay-repaint.md`。
