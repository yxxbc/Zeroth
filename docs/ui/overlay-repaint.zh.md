# overlay 重绘 —— 在回合进行时从 Ctrl+O / Ctrl+D 里拷文字

## 这个 bug

在 agent 正在生成时打开 Ctrl+O transcript（或 Ctrl+D 上下文视图），试着用鼠标
选一段文字，选区在你自己的光标下就死了。你得等回合结束，才能从全屏视图里拷出
任何东西 —— 而恰恰那是你最不想等的时候，因为你想留下的那条回复正是还在流式的
那一条。

*选区*本身哪儿都没坏。终端在它下面的单元格被重写的那一刻就丢掉鼠标选区 ——
xterm、VTE、kitty、iTerm2 和 tmux 都是这个行为，而且这是正确的行为：高亮的那
段文字不再是那里存在的文字了。overlay 只是不停地把屏幕上的每一个单元格一遍又
一遍地重写，不管有没有东西变了。

## 它为什么重写了一切

两件彼此独立的事在推它。

**`draw_overlay` 从不做 diff。** inline 的 live 区域一直在做 diff：
`paint_frame` 把上一帧留在 `InlineViewport::prev` 里，只发出
`prev.diff(buf)`。alternate-screen 那条路径没有这样的基线 —— 有 overlay 时
`prev` 被刻意设为 `None`，因为它描述的是一块不同的屏幕 —— 所以每一个 overlay
帧都新建一个 `Buffer`，把它**每一个单元格**推过 `draw_cells`。在一个 100×30
的窗格里那就是 3 000 个单元格，SGR 序列和所有东西，每帧如此。

**帧不停地到。** 两个来源，都不看视图：

- 每个回复事件都以 `schedule_frame()` 结尾（`src/tui/stream.rs`），只受
  120 fps 的 `MIN_FRAME_INTERVAL` 夹取 —— 所以一个快速的文本流会把帧推高到
  120 Hz；
- 只要有回合在进行，draw tick 就重新武装一个 32 ms 的动画帧
  （`src/tui/view.rs`），于是哪怕穿过一段没有事件的停顿（比如一次 tool 运行），
  也有约 31 Hz 在流。

那条链条是为 **inline** live 区域存在的 —— spinner 的 shimmer、已过时间的计数
器、脉动的 tool 圆点、后台 shell 走动的运行时长、agent 名单的计数器和 linger
清扫。这些在 overlay 之下没有一个在屏幕上。所以应用在以每秒约 31 到 120 次的
频率重绘整屏，去动画化没人看得见的东西，每次还把用户的选区一起带走。

Ctrl+D 上下文页面是它最纯粹的形式：`ContextSig` 完全不含流式状态，所以它的
*内容*在整个回合里可证明是不变的。它是同一块屏幕，在一个长回合里被重新序列化
了数千次。

## 修复

两处改动，都不大，就在上面点名的两个地方。

### 1. overlay 做 diff，未变的帧什么都不写

**单这一处改动就是修复。** §2 里的一切是另一笔节省；如果你只读一节，读这一节。

`term::overlay_paint` —— 纯的、有单元测试的 —— 把已经在 alternate screen 上的
那一帧和正在画的那一帧变成一份裁决：

- `Unchanged` —— 什么都没动；
- `Diff(updates)` —— 只有这些单元格不同；
- `Full` —— 屏幕上没有可比较的帧，重画每一个单元格。

`draw_overlay` 在 `Unchanged` 上**还没碰 backend 之前**就返回：没有单元格，
也没有 `BeginSynchronizedUpdate` / `Hide` / 光标移动这几种转义。一页静止的
页面，在线上就是沉默，而这正是重点 —— 一个在上面握着选区的终端看不到任何写入。

基线住在它自己的字段里，`InlineViewport::overlay_prev`。它不能共用 `prev`：
两者描述的是不同区域的不同的屏幕，而 overlay 的进入/退出路径按设计清掉
`prev`。只要 alternate screen 不再是它记录的那块屏幕，它就被丢弃：

- `enter_overlay` —— 排队的 `Clear(All)` 把屏幕清空；
- `exit_overlay` —— 屏幕没了，而且重新进入反正也会清掉它；
- `resized` —— 模拟器在它脚下把它重排了；
- 一次写到一半失败的帧 —— 屏幕随后处在一个没有 buffer 能描述的状态，所以整屏
  重画，而不是对一个虚构物做 diff。

`resized` 的丢弃与 `overlay_paint` 的面积检查并不重复，而且它是刻意无条件的
（无论 `changed` 与否）。一串 resize 可以合并成单个帧 —— `on_resize` 请求一个
帧，但调度器的下限是 8 ms —— 于是净尺寸可能落回基线正好记录的那块区域，而终
端在中间曾把 alternate screen 裁剪又长回来。对着那个基线做 diff，会让腾空的
行一直陈旧到 overlay 被关掉再打开。丢弃它只在一次 resize 上付一次整屏重绘，而
那次重绘反正要发生。

`Buffer::diff` 自己跳过宽字形影子单元格，这与 `visible_cells` 在整屏重画路径
上强制的是同一条规则，所以 diff 路径继承了撕表格那处修复，而不是把它重新打开
（`docs/ui/table-streaming.md` *Wide glyphs*）。

### 2. overlay 之下时钟链条停下 —— 一份 CPU 节省，不是修复的一部分

要说清这是什么、不是什么。§1 就位之后，在未变的页面上重新武装的帧本来就什么
都不写，所以**这根本没换来额外的可复制性**。它换来的是工作量：没有它，应用仍
然每秒 31 次地为一块没人看的屏幕*构建*那个帧 —— 一次整屏 `Buffer` 分配、渲染
闭包、一次走过每个单元格的 diff，在 Ctrl+D 分类器页面上还有一个 backend 互斥
锁和一次全新的 `String` 渲染 —— 只为每次都得出 `Unchanged`。

`App::wants_animation_frames` —— 纯的、有单元测试的 —— 是 draw tick 的重新
武装条件，而只要 `View::is_overlay()`，它现在就是 `false`。

overlay 显示的东西没有一样会变陈旧，因为 overlay 显示的东西没有一样是时间驱动
的。这不是当前代码的偶然，它就写在代码里：`tool_full_body` 开头是
`let pulse: Option<Duration> = None;`，上面一条注释解释说在 transcript 里做动
画「would either not move or cost a full-tail re-render every 32 ms」，
`TranscriptSig` 不含时钟字段，而*确实*接收已过时间的那两个渲染器 ——
`shell_running_line` 和 `running_command_lines` —— 只从 `src/ui/live.rs`（
inline 条带）调用。内容变化仍然以事件到来，而每个事件来源（`stream`、`agent`、
`background`、`mcp`、各 worker、每一次按键）都以自己的 `schedule_frame()`
结尾。链条在返回之后的第一次绘制时重新播种，因为 `after_key` →
`schedule_for_key` 正是在关掉 overlay 的那一次按键上触发的。

诚实地说什么*确实*会暂停：agent 名单的运行时长计数器及其 linger 清扫
（`tick_agent_roster`），只有 draw tick 驱动它们。两者都由绝对的 `Instant`
推导并重新计算而非累加 —— 运行时长是 `started.elapsed()`，清扫把存下的截止时
间与 `now` 比较 —— 所以返回后的第一帧产出的正是它本来会有的值。它们还在页脚
里，在 overlay 之下看不见。什么都没丢，只是在看不见时被推迟了。（toast 过期
*不*受影响：`expire_toast` 独立于这个谓词调度自己的帧。）

真正的代价是一处耦合：如果哪天给 overlay 渲染的某样东西加了时钟，它会在这里
冻住而不是走动。transcript 缓存在一个无时钟的签名之后，所以那里的改动得先打
败 `TranscriptSig`，会被注意到；但 `agent_transcript_lines` 是每次绘制新建的，
没有这样的绊线。任何一个往 overlay 主体里加实时计数器的人都得重新审视这个谓词。

## 它量出了什么

`tmux pipe-pane` 捕获应用写进它窗格的每一个字节。在 100×30 的窗格里驱动一个
确定性的 30 秒回合（`!sleep 30`），在它中间量一个三秒的窗口：

| 视图 | 之前 | 之后 |
| --- | ---: | ---: |
| inline 对话（对照） | 4 533 B | 4 533 B |
| Ctrl+D 上下文视图 | 316 217 B | **0 B** |
| Ctrl+O transcript | 389 019 B | **0 B** |
| Ctrl+O 往上滚（Home） | 380 277 B | **0 B** |

inline 对照组未变，这是必须的 —— 它的状态行确实在动，那 4.5 KB 是 shimmer 和
计时器在干它们该干的事。

同样的测量对着一个**真实 backend** —— OpenRouter 上的 `openai/gpt-4o-mini`，
驱动真实的 agentic tool 循环，带一次 `bash sleep 25` 调用，断言那个 tool 在
窗口结束时仍在运行：

| 视图 | 之前 | 之后 |
| --- | ---: | ---: |
| inline 对话（对照） | 20 562 B | 20 700 B |
| Ctrl+O transcript | 550 970 B | **0 B** |
| Ctrl+D 上下文视图 | 420 198 B | **0 B** |

大约每秒 138 KB 的屏幕重写，落在一页没变过的页面上，回合持续多久就写多久。

## 这给了你什么，没给什么

**静止的页面，零写入。** Ctrl+D 上下文视图在回合流式期间不变（`ContextSig` 只
按已提交的历史取键），所以它现在整个回合完全静止：随便选、随便拷。

**在 Ctrl+O 里往回滚时，只有动了的东西被重写。** 按 ↑ / PageUp / Home 会脱离
跟随尾部（`App::tool_follow`），`settle_tool_scroll` 随后保持住偏移，于是窗口
不再追着前端跑。到达终端的只有 diff，别无其他。滚到一个比屏幕还长的
transcript 的顶部 —— 前端在屏幕之外 entirely —— 那是**零字节**，量出来的。
滚到某个位置，还有一部分正在增长的文字可见，那就是每秒几百字节，且*恰好是那些
单元格*：在一段短 transcript 的顶部于流式途中捕获，整整 2 秒的流量就是回复
自己的词一个个落到它们所属的那一行上（`ESC[11;7Hmodel`、
`ESC[11;13Hthat`、…）。屏幕上其他每一行都不动，所以除了正在增长的那一行，
任何地方的选区都能活下来。

**在 Ctrl+O 里钉在底部时，主体仍然在动。** 跟随尾部做的正是它该做的：新内容到
来，窗口滚动以保持前端在视野里。一个选区覆盖的文字从它下面滚出去，无论在这个
pager 还是任何 pager 里都不可能存活，而悄悄冻结跟随会是更糟的交换 —— 你会在
没被告知的情况下读一页陈旧的页面。即便在这里 diff 也有帮助，因为只延长最后一
行的 chunk 只重写那一行；是新的*一行*才会移动窗口、赔上选区。往上滚去回读
—— 页脚的 `↑/↓ to scroll` 那行本来就在邀请你 —— 页面就静止了。

## Phase 94 检查什么

两种不同的失败，谁也不包含谁：

- **它真的安静下来了吗？** `tmux pipe-pane` 在由 30 秒 `!sleep` 保持活跃的回合
  中捕获应用在整整两秒里写进窗格的每一个字节。Ctrl+O 与 Ctrl+D 必须写出恰好
  零字节 —— 而 inline 对照组先被断言写出*某些东西*，所以零永远不可能来自一个
  坏掉的表计。然后检查时钟链条在返回之后重新播种，因为一条留断的链条会让
  inline 计时器在这个回合剩下的时间里冻住。
- **它画对单元格了吗？** 字节对正确性什么也没说。所以整个回合在 overlay 之下
  流式（数百个 diff 帧），视图滚离尾部再滚回来，resize 离开再直接回来，然后
  overlay 被关掉再打开 —— `enter_overlay` 清掉 alternate screen，所以那一帧
  是同一内容在同一位置的一次纯整屏重画。两块屏幕必须完全相同。一个丢掉更新的
  diff，或者一个在 resize 之后留下陈旧单元格的 diff，会在这里失败，而且只在
  这里失败。

## 这触及的不变量

不变量 4（`CLAUDE.md`）实质未变，实践上略强了一点：overlay 之下的提交仍然只是
*排队* —— `insert_before` 不做 I/O，`draw_overlay` 从不冲刷队列 —— 而返回时的
普通绘制仍然把 live 区域之上的整份积压冲刷出去。overlay 现在只是拒绝写它不需要
写的单元格，也就是 alternate screen 终于拿到了 `paint_frame` 在 inline 里一直
遵循的那条规则。

## 它住在哪儿

| 部件 | 文件 | 性质 |
| --- | --- | --- |
| `overlay_paint` → `OverlayPaint` | `src/term.rs` | **纯的** |
| `overlay_prev` 基线、`draw_overlay`、`resized` | `src/term.rs` | 边界 |
| `View::is_overlay` | `src/app/types.rs` | **纯的** |
| `App::wants_animation_frames` | `src/app/views.rs` | **纯的** |
| draw tick 的重新武装 | `src/tui/view.rs` | 边界 |
| 字节级证明 + 陈旧单元格检查 | `scripts/smoke.sh` Phase 94 | smoke |
