# 拥有屏幕的 TUI —— 全屏模式（设计）

**状态：phase 1（骨架）已实现。** `--fullscreen` 或 `ZEROTH_TUI=full` 把对话和
live 区域画进一帧备用屏画面。滚动、自己的选区和复制、鼠标与卡片是接下来的
phase；内联 viewport 仍是默认，直到下面的对齐清单全绿。

## 为什么

内联 viewport 是一个活在终端自己 scrollback 里的对话：每一行画完就归终端所有。
这正是它的特性 —— 你的 shell 历史和对话共用一个 scrollback —— 也正是它的天花板。
一行画完之后无法重画、无法重新度量，也无法归到某个鼠标坐标上。所以每一个需要
知道单元格*在哪里*的交互 —— 悬停、点击展开、链接下的预览卡、画出来的滚动条、
跟着链接走的选区 —— 要么得委托给终端（OSC 8 恰恰就是这样的委托），要么根本
无法存在。事后再加每一个，都意味着再向终端讨一次人情：屏幕不是我们的，所以设计
永远摊着手。

拥有屏幕就拆掉了这个天花板。app 知道自己画过的每一个单元格，于是它能对任何一个
做命中测试、重画任何一个，并让将来任何新能力只是一次普通的渲染决定。代价是实打
实的 —— 终端原生的 scrollback、选区和复制不再可用 —— 而这一页就是有意付这笔钱
的计划，且先放在一个开关模式里，直到它至少不亚于它替代的东西。

**决定：** 做一个拥有屏幕的全屏模式。内联 viewport 保持默认，直到下面的对齐清单
全绿，然后作为一次独立的产品决定再议默认值（以及内联是否保留为一个选项）。

## 保持不变的东西

- **`App` 是唯一事实来源。** `history`、live tool 队列、agents、各个 overlay、
  composer、队列、checkpoint：不变。全屏模式是同一份状态上的第二个
  **viewport**，不是第二个 app。
- **事件循环与 backend 接缝。** `StreamEvent`、agent 循环、`ReplySource`、权限、
  hooks、MCP、后台 shell：不变。
- **Headless。** `-p` 从不创建 viewport，不受影响。
- **不变量 1、2 和 4**（`CLAUDE.md`）：单一 stdin 读取者；行构建用前缀稳定的
  换行；tool call 与文字交错。即使整屏都重画，不变量 2 仍然要紧：行构建器和
  transcript 缓存依赖“已完成的一行在更多文字到来时永不改变”。
- **那些构建器。** `ui::transcript`、`ui::live`、`ui::header`、`ui::footer`、
  `ui::composer`、`ui::toast`、各个 picker、权限 prompt、
  `images::place`/`ImageStore::stamp`，以及 `term::draw_cells`（OSC 8、图片、
  同步更新）是原材料。全屏的那一帧是它们的新组合，不是把它们重写一遍。

## 改变的东西

不变量 3 —— 内容锚定的内联 viewport 以及它的双向 resize 重排 —— 是**内联模式
的**不变量。全屏模式用下面陈述的另一份契约替代它，两者绝不能混用。

### viewport 接缝

`InlineViewport` 多了一个兄弟，而调用点看到的是一个 `Screen` 值（一个 enum，
不是 `dyn` trait —— 绘制方法收一个闭包）：

```rust
enum Screen {
    Inline(InlineViewport),
    Full(FullViewport),
}

impl Screen {
    /// Show `lines` as part of the conversation. Inline: commit into
    /// scrollback (`insert_before`). Full: no-op — history already holds it.
    fn commit(&mut self, lines: Vec<Line<'static>>) -> io::Result<()>;
    /// Reseat before a commit (inline) / no-op.
    fn set_view_height(&mut self, height: u16) -> io::Result<()>;
    /// The caller composes a region: inline paints the live region at the
    /// bottom; full paints its whole frame and places that region in it.
    fn draw(&mut self, render: impl FnOnce(Rect, &mut Buffer)) -> io::Result<()>;
    /// An app-owned page: inline opens an alt-screen overlay; full paints the
    /// same page into its frame.
    fn draw_overlay(&mut self, render: impl FnOnce(Rect, &mut Buffer)) -> io::Result<()>;
    fn exit_overlay(&mut self) -> io::Result<()>;
    fn suspend(&mut self) -> io::Result<()>;
    fn resume(&mut self) -> io::Result<()>;
    fn restore(&mut self) -> io::Result<()>;
}
```

TUI 的处理函数本来就把自己的 scrollback 写入收束到少数几个 `Session` 方法上
（`commit_tool_cell`、`flush_segment`、`insert_before`、`reflow`、
`overlay_return_repaint`……）。在全屏模式下，这些提交方法变成空操作（`history`
里的记录就是那一帧的来源），而每一帧都从状态重建。这就让处理函数和事件契约保持
不动；模式切换发生在边界上，而不是散进每一个 `if fullscreen`。

### 那一帧

一帧是这些的纯函数：

```
App state × width × height × scroll offset × open overlay × theme
```

它自上而下构建：

1. 横幅（与内联同一个构建器，在配置了它时），
2. 对话：已完成的历史由 transcript 的构建器渲染，接着是 live 区域（流式文字、
   思考、tool 单元格/组、agent 树、排队消息、后台 notice），
3. 状态行与 tips，
4. composer 与页脚，
5. 任何 overlay 页面（权限 prompt、ask、各个 picker、`/usage`、Ctrl+O……），它
   替换那一片区域或整个帧，与它的内联对应物完全一样。

transcript 缓存（`ui::transcript::TranscriptCache`）已经按宽度渲染一次冻结的
历史、只追加尾部；这一帧复用它。live 区域的高度、光标位置与预览，就是内联条带
用的那些 `ui::live` 函数。因为帧是从历史重建的，`/resume`、一次回溯、`/clear`
和 resize 都只是重画一次 —— 没有 purge/rebuild 的舞步，没有 `insert_before`
队列，也没有 viewport 钉住。

### 滚动

对话比屏幕高，所以这个模式拥有一个滚动模型（参考实现 `BodyView`）：

- **默认跟随尾部**，且下限只向前移动：新输出把视图往下推，从不把它往上拽。
- **PageUp/PageDown、Home/End、滚轮**；任何提交消息或启动回合的按键都回到尾部；
  已经往上滚过的视图在新文字到来时原地不动。
- **底部提示**（既有的按钮样式），在视图不在尾部时显示，外加一个
  `N new lines` 标记。
- **resize 锚定**第一行可见的行（或选中行），而不是锚定原先的屏幕几何；一次重排
  从历史重新换行。
- **选区**锚定到帧里的 `(row, column)`，在重建之后重新解析。

### 鼠标与命中测试

整个全屏 session 期间 `EnableMouseCapture` 都开着（已经没有原生 scrollback 要
保护了）。每一帧都在旁边带一张**命中图**：

```rust
struct RowHit {
    target: Option<Target>,          // history item / tool cell / agent group
    links: Vec<(u16, u16, String)>,  // decoded from the spans' carriers
    button: Option<Action>,          // e.g. the bottom hint
}
```

- `Moved` → 悬停：给链接那一段加下划线，高亮可折叠的那一行；
- 左键 `Up` 落在链接上 → 打开它（OSC 8 的终端活儿现在是 app 的）；
- 左键 `Up` 落在一个目标上 → 折叠/展开那个条目，锚定被点的那一行，让展开向下
  生长；
- 滚轮/拖拽如上；双击选词（可选），三击选行；
- 键盘仍是主要的交互方式 —— 鼠标永远是附加的。

权限 prompt、ask 模态框和各个 picker 保持它们的键盘契约；在它们的选项行上做
点击目标是可选的后续一层。

### 选择与复制

丢掉原生选区是主要的代价。这个模式在帧上实现拖拽选择，`Ctrl+C`/`c` 经既有的
剪贴板模块复制；选区能在新输出到来时存活，按链接的完整目标标记链接，并复制
不带 gutter/缩进装饰的文字（`BodyView::selected_text` 的规则）。`Shift+drag` 在
实现了旁路的终端上仍可留给终端；app 不应依赖它。是否为 ssh 下的复制加上 OSC 52
作为兜底，是下面一个待定问题。

### 备用屏生命周期

- 进入：raw 模式（已有）、`EnterAlternateScreen`、鼠标捕获、括号粘贴（已有）、
  受支持时的键盘增强（可选），然后第一次整屏重画。
- 离开：反过来，在每一条退出路径上，`SIGINT`、panic（既有的 panic hook 已经会
  恢复内联屏幕）。
- `Ctrl+Z` / `SIGTSTP`：离开备用屏和 raw 模式，挂起，`fg` 时重新进入并重画。
  这正是 Miyu 的 `suspend`/`reenter` 路径，也是唯一一处重新进入时不能重新查询
  终端的地方（单一 stdin 读取者还活着；不变量 1）。
- 编辑器交接（可选，稍后）：离开 raw/捕获，运行 `$EDITOR`，重新进入，重画；
  终端不会被丢在 shell 上。

### 链接、路径与卡片

一旦屏幕归我们，富链接的设计（[`rich-links.md`](rich-links.md)）就变简单了：
裸路径经既有的 carrier 得到 `file://` 目标、悬停加下划线、点击打开；一张链接卡
可以**内联**画在回复里，因为迟到的元数据在法律上可以改变帧的高度（滚动锚点守住
阅读位置）。那一页上的抓取/缓存策略仍然适用。

## 骨架今天做了什么

- `--fullscreen`（或 `ZEROTH_TUI=full`）在通常的启动查询之后进入备用屏；每一条
  退出路径 —— 正常退出、出错、panic —— 都会离开它（该 session 用
  `OVERLAY_ACTIVE`，所以既有的 panic hook 和 `restore` 完成拆除）。
- `Session::draw_conversation` 组合一帧：横幅、已完成的历史、把在飞的回复当作
  一条 assistant 消息、然后是 live 区域，底部是 composer 与页脚。对话跟随它的
  尾部（最后 `screen − live` 行）；短对话从顶部开始。
- 这个模式下条带的流式预览被抑制（回复已经在对话里），尾部按
  `(agent view, history generation, width, streaming length, theme)` 缓存，于是
  一帧 120 fps 的动画不会重渲整个对话。
- **滚动。** PageUp/PageDown 移动一屏，Ctrl+Home/Ctrl+End 到顶/尾，滚轮移三行。
  往上滚把窗口锚在那一行尾部行（`full_top`）上，所以新输出不会移动它；到达底部
  就恢复跟随尾部。只有当边界把 app 标成全屏时这些键才被占用，所以内联 composer
  仍保有它们。当拥有窗口顶部的那条用户消息已经滚到它上方时，它的第一行被**钉**在
  一行预留行上（窗口之上最新的那条用户消息），于是问题与它的答案待在一起；当
  提示本身可见时不花任何行（`sticky_line`、`full_user_starts`）。
- **鼠标。** 捕获在整个 session 期间开着（退出时关掉）。最后画出的那些对话行
  就是命中图（`full_window`）：移到链接上给它那一段加下划线，无拖拽的左键点击经
  平台打开器（`open`/`xdg-open`/`cmd start`）打开它，左键拖拽选择并复制（app
  自己的选区，因为捕获拿走了终端的；`Shift+drag` 仍是终端的逃生口），且不
  要求按 Shift。滚轮滚动。点击的地方**没有链接**时**折叠/展开它下面那个可展开
  的单元格**（`full_items` → `conversation_lines_expanded`）：一个 tool call、
  thought、task call、agent 组或 hook 报告，在它折叠的内联形态与 Ctrl+O 形态
  之间切换，该条目的第一行被钉在它原来占的那一行（`full_anchor`）。在 composer
  的文字行里左键点击把光标放在那里（`ui::composer_cursor_at` +
  `TextArea::set_cursor`）。链接就是已经画出的那些 carrier —— OSC 8 的 URL、
  file tool 的头，以及 `links::find_paths` 现在标记的正文/代码文件路径 —— 所以
  一次点击经同一个平台打开器打开一个 URL 或一个文件。assistant 回复里**单独占
  一行**的 URL 会被抓取并替换成一张预览卡
  （站点 —— 标题、暗淡的描述，以及页面有封面图且终端能画图时的封面缩略图），
  缓存在 `~/.zeroth/cache/link-cards.json`（`ZEROTH_LINK_CARDS=0` 关闭它）；每个
  文字单元格都带着该链接。
- Ctrl+O、context 视图以及每个全屏页面都通过既有的 overlay 路径画在帧之上；
  离开一个页面就重画这一帧。resize 时从历史按新宽度重新换行。
- **还没有：** 精确到字符的选区（高亮是 span 级的）、在 live 区域的选项/picker
  上的指针点击（composer 的光标已经有了）、链接卡上的封面图、挂起（`Ctrl+Z`）
  /编辑器交接，以及超出 transcript 尾部之外的 agent 视图对齐。它们就是下面的
  phase。

## 模式选择与推广

- **先做成开关：** `--fullscreen`（名字待定），加上一个配置键
  （`ui.screen = "inline" | "full"`）和一个供测试用的环境覆盖
  （`ZEROTH_TUI=full`，先例是 Miyu 的 `MIYU_TUI=1`）。默认：`inline`。
- 翻转默认之前的**对齐清单**：流式文字、思考、tool 单元格/组、并行批次、
  agents/agent 视图、后台 shell、权限 + ask prompt、每一个 picker 与页面、
  `/clear`、`/resume`、回溯/backtrack、Ctrl+O（展开的 transcript）、图片、
  toast、状态/tips、页脚、队列/steer、checkpoint 通知、Ctrl+B、粘贴、resize、
  挂起、退出恢复、headless 不受影响。
- **Smoke：** 既有的 phase 在内联默认下保持全绿。一套全屏 phase 集用
  `ZEROTH_TUI=full` 跑这个二进制，并对备用屏的 `tmux capture-pane` 做断言；鼠标
  可以用 `tmux send-keys -M` 注入（或经 pty harness 发一个 SGR 序列）。键盘对齐
  拿到大部分断言；鼠标拿到单元级的命中图测试加少量端到端点击。
- **然后**翻转默认，为内联留一个弃用窗口（`--inline`），并（与 owner 一起）
  决定内联是永久选项，还是全屏模式成为唯一的交互前端。

## 风险与缓解

- **丢掉原生 scrollback。** 这个模式下对话随屏幕一起走。缓解：`/export` 和复制
  已经存在；帧自己的滚动模型覆盖了阅读；备用屏按设计在退出时恢复 shell 的屏幕。
- **选区/复制的质量。** 手写的选区永远不如终端的第一天。缓解：复制走系统剪贴板、
  链接复制完整目标，并按终端记录 `Shift+drag` 这个逃生口。
- **终端/tmux 的怪癖。** 备用屏、鼠标上报和图片协议各家不同。缓解：这个模式是
  开关式的，内联路径始终可用，而图片/OSC 8 的闸门是共用的。
- **性能。** 每次绘制重建整帧是 O(screen)，而 transcript 缓存已经把贵的部分
  做了。差分是按单元格的。预算是在一个大 session 上以 200×60 流式输出时
  60 fps；翻转默认之前先量。
- **多一个要测的前端。** smoke 矩阵翻倍。缓解：让帧的组合保持共用，并把功能集
  分阶段。

## 阶段

1. **骨架 —— 已完成。** `--fullscreen`：备用屏生命周期、由历史 + live 区域 +
   composer + 状态组成的帧、跟随尾部、仅键盘、退出/panic 恢复、resize 重排，
   以及端到端驱动它的 Phase 156。
2. **对齐 —— 下一步。** agents/agent 视图、prompt、picker、图片、toast、队列、
   backtrack、resume/clear、Ctrl+O、resize 锚点；对齐清单全绿。
3. **鼠标与滚动 —— 除卡片/选区打磨外已完成。** 捕获、命中图、悬停、点击链接、
   点击展开、滚轮、PageUp/PageDown/Ctrl+Home/End、拖拽选择/复制，以及
   composer 的点击定位光标都能用；这里仍开着的是：精确到字符的选区，以及
   picker/选项行上的鼠标。
4. **富链接 —— 文字卡已完成。** assistant 回复里单独一行的 URL 抓取之后变成一张
   预览卡（站点 —— 标题、暗淡的描述、有则加封面）；`/links` 页面仍然保留
   （`rich-links.md`）。
5. **默认值的决定。** 翻转默认，保留 `--inline`，评估移除。

## 待定问题

- **内联是永久的吗？** 两者都留是有实打实代价的（两个 viewport、两个渲染器、
  翻倍的 smoke 矩阵）。追求全屏的 owner 决定已经记录；移除是更晚的一个决定。
- **Ctrl+O 的角色。** 全屏下对话本身就是 transcript；Ctrl+O 可以留作“到处都
  展开”（当前的 overlay）、变成跳到最近一条回复的动作，或者被退休。在对齐阶段
  决定。
- **选区的语义。** 选词/选行、链接的完整目标复制、ssh 上用 OSC 52 作为复制兜底。
- **这个模式的名字与 flag。** `--fullscreen` 是描述性的；`--tui` 与 `--inline`
  搭配得不好；在第一个带上它的版本之前定下来。
- **退出时的 scrollback。** 这个模式应该把最后的屏幕留下（一次“最后一帧”的
  收成），还是像每个备用屏应用那样恢复 shell 的屏幕？恢复是默认；`/copy
  transcript` 是逃生口。

## 非目标

- 改变 agent 循环、tool 契约、session 格式或 headless。
- 在对齐完成之前把全屏模式做成默认。
- 重新实现终端的模拟器：滚动、选区和复制在这个模式下归我们，但粘贴文字的解读、
  括号粘贴和图片协议仍然归终端和既有的模块。
