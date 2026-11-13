# 启动时的横幅头

一块 Claude-Code 风格的**头**，在启动时提交到 scrollback：会话的
**mascot**（见 `docs/ui/mascot.md`）挨着产品名 + 版本、工作目录，以及一行命令
提示。它是用户看到的第一样东西，并随着对话变长自然地向上滚进历史 —— 与 Claude
Code 的欢迎横幅完全一样。

## 它显示什么

```
 ▙▄▙▄▟▄▟   Zeroth (v0.1.0)
▝▜▄███▄▛▘  ~/inline-tui
  ▘▘ ▝▝    / commands   @ files   ? shortcuts
```

- **Mascot** —— 选中 mascot 的图案（默认是 crest，用 `/mascot` 切换）。支持原生图形
  协议时显示透明 PNG；其他终端显示紧凑的块字符画。图案齐左绘制，
  并用一条左到右的**强调色 → 链接色渐变**上色（主题自己的两个强调色 —— 默认是
  Catppuccin Mocha 的 sky `#89DCEB` → blue `#89B4FA`，在 `/theme onedark` 下
  是 One Dark 的 `#56B6C2` → `#61AFEF`；`docs/ui/theme.md`），按显示列并以图案块
  的宽度为键。没有边框，所以图案有呼吸的余地，也不画任何框线（见下面
  *Why borderless*）。
- **标题** —— `Zeroth` **加粗**，然后 `(v{CARGO_PKG_VERSION})` 暗色。
- **cwd** —— 与页脚所用的同一个 `~` 相对显示字符串（`App::session.cwd`，由边界
  处的 `ui::display_cwd` 产生），颜色也是页脚的路径色 —— 主题的成功绿
  （`header_cwd_color()` 就是 `footer_cwd_color()`，`docs/ui/footer.md`），所以
  这条路径在屏幕的两端是同一种颜色。没有注入会话信息时跳过。
- **提示** —— 状态感知：一个已配置好的会话显示打字之外那三个入口 ——
  `/ commands`、`@ files`、`? shortcuts` —— 符号用青色强调色、名字暗色，三空格
  分隔（分隔符也是暗的）。一个处在离线演示上的会话（没有解析出 provider、模型和
  key）则改为显示那一个能改变它的动作：`Connect a model → /login`。

这些元数据行坐落在 `art width + 2` 处对齐的一列里，而在 mascot 比它高时，整块
**垂直居中**，平局向下取 —— 文本坐得低一些，而不是挂在图案顶端之外，所以这一对
读起来像一枚构图完整的徽章。目录里的图案统一都是三行，那恰好是元数据**带会话时**
的高度；而活着的那个情况是**无会话**的横幅，它的两行（标题 + 提示，没有 cwd）
坐在 1–2 行：

```
 ▙▄▙▄▟▄▟
▝▜▄███▄▛▘  Zeroth (v0.1.0)
  ▘▘ ▝▝    / commands   @ files   ? shortcuts
```

没有元数据邻居的图案行单独渲染（不带尾部填充），而超出图案的元数据行（今天目录里
没有这种）会保住它们那一列。每一行都按宽度夹取并带一个尾部 `…`，与
`footer_line` 完全一样。

## 自适应回落

横幅从不溢出：当终端放不下图案挨着完整标题
（`art width + gap + "Zeroth (v…)"`）时，`header_lines` 回落到一行**文字徽章**
—— 渐变的 `Zeroth` + 一个强调色的 `v…` —— 压在同样夹取过的 cwd/提示行上。两级
都带字面的 `Zeroth`，而 smoke 套件正是以它为键（Phase 45 那个与层级无关的
标记）。

## 它是装饰，不是历史

这个头和会话页脚一样是**纯粹的装饰** —— 它从不进入 `App::history`。这意味着它
从不会被发给模型（`context::context_messages` 只走历史），也从不写进 `/resume`
的 rollout 文件（`tui::recorder::SessionRecorder` 镜像历史）。它**确实**位于
Ctrl+O transcript 的顶端 —— `ui::transcript_build` 在走历史之前先前置
`header_lines` + 一个空行，所以 overlay 镜像内联的 scrollback，而后者以横幅开场
（空 transcript 的占位符以「横幅之外什么都没有」为键，而回溯的选择/滚动偏移来自
同一次遍历，所以它们保持一致）。`header_lines` 是行构建器，输入包括
`App::session`、`App::mascot`、`env!("CARGO_PKG_VERSION")` 和进程级图片策略。图片路径由
边界发布；构建器本身不做文件 I/O。时机由边界掌管。

### 熬过每一次重建

内联 TUI 的重排从 `history` 重建（不变量 3），这带来一个麻烦：每次重绘都从历史
重新生成屏幕，而这个头并不属于历史。所以它在 I/O 边界上被重新发出：

1. **启动时** —— `tui::bootstrap` 经
   `term.insert_before(ui::header_lines(&app, width))`（外加一个空行间隔）提交它
   一次，就在第一次绘制之前。它通过正常的无闪烁管道流入 scrollback。
2. **每次 purge 重建**（尺寸变化、`/clear`、一次历史回退、一次 `/mascot` 切换）
   —— purge 把 scrollback 整个丢掉了，所以 `repaint_conversation` 经纯的
   `ui::banner_tail(banner, tail)`（横幅 + 间隔 + 尾部）把横幅恢复到重建后的尾部
   之上：它在尺寸变化**之后**重新出现，在 `/clear` **之后**也是（一条全新开始的
   横幅，与 Claude Code 的 `/clear` 一致），而一次 `/mascot` 切换会立刻用新的
   mascot 重绘它（`docs/ui/mascot.md`）。

一次 **overlay 返回**（Ctrl+O / `/resume` / Ctrl+D）完全不需要重新发出：它从不
重写屏幕 —— overlay 开着时提交的一切都从 viewport 的待处理队列里冲出到**已有
内容之下** —— 所以启动时提交的横幅就那么活着，在屏幕上或在终端保留的 scrollback
里，永远不会重复（`smoke.sh` Phase 3 钉住不重复那一半，Phase 45 钉住落定的往返；
它所替代的那种「覆盖整个窗口」的返回，过去会在短对话上把它擦掉，而当一条回复在
overlay 下长过窗口时，会把窗口上方的一切从终端里悄悄丢掉 —— 那个 scrollback 的
洞现在由 Phase 81/82 钉住关上了）。

因为重绘窗口是**底部锚定**的（输入框钉在底部、溢出向上滚 —— 不变量 3），前置这个
头永远不会遮住之前屏幕上的内容：头只是占据对话上方的 scrollback。在长对话上它会
从顶部滚走，像其他一切一样被 `RESIZE_REFLOW_MAX_ROWS` 封顶。

## 为什么无边框

`smoke.sh` 的尺寸变化检查（Phase 17）按部件数输入框 —— `^─+$` 边框行 == 2、
裸的 `^❯ *$` 提示行 == 1、`dummy_model_name ·` 页脚 == 1。一个*带框*的头
（`╭──╮ … ╰──╯` 那种盒子）会多出只有 `─` 的边框行，把那两个计数撑大。无边框的
设计不画整宽边框、不画裸提示行、也不画模型名，所以它在那些结构计数器眼里不可见
—— 它只会在图案/元数据行上增加东西。头部的文本同样避开了其他阶段所键的那些
字符串（`for commands`、`dummy_model_name ·`、`Happy`、`⎿`、`Done for`、
`esc to interrupt`）。

## API（位于 `ui/header.rs`）

- `header_lines(app: &App, width: u16) -> Vec<Line<'static>>` —— 整个横幅，
  作为该会话 mascot 的 scrollback 行，尺寸为 `width`。原生图形终端通过图片 carrier
  预留 icon 区域；其他终端输出块字符画。不带尾部间隔；调用方加一个
  （`insert_before(msg); insert_before(blank)` 那个模式）。
- `header_lines_for(app, mascot, width)`（`pub(super)`）—— 同一个横幅但用一个
  显式给出的 mascot：`/mascot` picker 的实时预览，所以预览与横幅共用一个构造器
  （`docs/ui/mascot.md`）。
- `banner_tail(banner, tail) -> Vec<Line<'static>>` —— 横幅 + 一个空行间隔，
  压在 purge 重建的重绘尾部之上（见 *Surviving every rebuild*）。
- `HEADER_NAME` / `HEADER_HINT` / `HEADER_ART_GAP` —— 标题里那个词、提示的各个
  token，以及图案 → 元数据之间的间隙。
- `header_gradient_start()` / `header_gradient_end()` —— mascot 渐变的端点；
  `gradient_spans` 按显示列做线性插值，并把颜色相同的连续段合并成 span。
- 图案本身住在 `app::Mascot` 上（`docs/ui/mascot.md`）。

## 测试

- `ui/tests/header.rs` 的单元测试（`header_*`），以及 `ui/header.rs` 的 icon 布局测试：
  横幅把图案行排在对齐的元数据列
  旁边（逐字的图案、2 列的间隙、没有元数据的行不带尾部填充）；元数据块在一个更高
  的 mascot 旁边坐得更低（每个 mascot 的无会话横幅都把它那两行放在 1–2，图案独自
  在上方）；它跟随 `App::mascot`；标题是加粗的名字 + 暗色的版本，cwd 用页脚的
  路径色，提示用强调色；图案携带那条强调色 → 链接色渐变；极窄的宽度回落到文字
  徽章且绝不超出宽度；一个无会话的 `App` 仍渲染图案 + 版本（没有 cwd 行）；横幅
  在任何宽度、任何 mascot 下都不含任何 smoke 保留的字符串。
- `ui/tests/header.rs` 的单元测试（`banner_tail_*`）：横幅 + 间隔恢复到重建后的
  尾部之上，而一条很长的尾部永远不会把它裁掉。
- `ui/tests/header.rs` 的单元测试（`transcript_*`）：transcript 以横幅 + 一个
  空行开场，在走历史之前。
- `ui/tests/wrap.rs`：mascot 那一级在 smoke 套件 50 列的尺寸变化宽度下仍然放
  得下，
  而大范围扫过的任何宽度都不溢出。
- `scripts/smoke.sh` Phase 45：头在启动时显示（`Zeroth` 标记），熬过一次尺寸变化
  （80×24 → 50×24 → 80×24 之后仍在），在 `/clear` 之后重新显示，位于 Ctrl+O
  transcript 的顶端（空的以及与一段真实对话的），并且**熬过 Ctrl+O 的往返**；
  Phase 3 钉住不重复那一半（在一段长对话上、流式中途返回之后，屏幕 + scrollback
  里永远不超过一块横幅）；Phase 16/17 确认它不干扰 `/clear` 后的空白计数或尺寸
  变化的结构计数；Phase 86 驱动 `/mascot` 切换（`docs/ui/mascot.md`）。
