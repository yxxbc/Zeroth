# 输入框之下的 session context 页脚

openai/codex 的页脚**状态行**的移植：单独一行，钉在输入框下边框的正下方，给出
环境性的 session context ——

```
────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────
  dummy_model_name · ~/zeroth                      auto
```

—— backend 的模型名与当前工作目录，用 ` · ` 连接，以及 —— 钉在这一行的**右边缘**，
即它自己的区域 —— 这个 session 的**权限模式**（`auto`，直到某个项目另存了别的；
Shift+Tab 循环的那个姿态 —— `docs/state/permissions.md`；权限被禁用时隐藏）。关于
它在运行时模型里的位置，见 `CLAUDE.md`。

## codex 是怎么做的（调查结果）

Codex 的底部窗格在 composer 之下预留一行页脚（`bottom_pane/footer.rs`）。它通过
那一行复用了许多东西（退出提醒、`?` 快捷键 overlay、队列提示）；我们移植的那一块
是**被动状态行** —— "the configurable contextual row built from `/statusline`
items such as model, git branch, and context usage"：

- **默认内容**（`chatwidget.rs`）：
  `DEFAULT_STATUS_LINE_ITEMS = ["model-with-reasoning", "current-dir"]` —— 恰好
  是一个模型名和一个 cwd。
- **拼装**（`bottom_pane/status_line_style.rs::status_line_from_segments`）：各段
  用一个**暗色的 ` · ` 分隔符**连接；主题颜色关闭时每一段都是朴素的
  `Style::default().dim()`（打开时，各项有强调色 —— 模型青色、路径绿色）。
- **cwd 格式**（`status/helpers.rs::format_directory_display`）：相对家目录 ——
  家目录本身是 `~`，家目录之下是 `~/rel`，否则是绝对路径。
- **布局**：以两列缩进渲染（`FOOTER_INDENT_COLS = 2`，经由 `prefix_lines`），而
  拼好的行在溢出宽度时以省略号截断（`truncate_line_with_ellipsis_if_overflow`）。
- **让位**（`footer.rs::shows_passive_footer_line`）：状态行把这一行让给指导性的
  内容 —— 命令弹窗、`?` 快捷键 overlay、退出/Esc 提醒，以及（任务运行时带草稿时）
  的队列提示。

## 我们造的

实时区域里一个常开的 context 行，**在框之下、在 band 打开的位置之下** —— 从启动
起就在，空闲时*和*回合流式时都在 —— 它让给任何打开的 band：palette、`?` 快捷键
band，或 `@` 文件 picker（我们对应 codex 的弹窗与快捷键 overlay 的东西）。后来有
两个特性**直接占下这个槽位**而不是把它清空，即 codex 的页脚模式复用：Ctrl+R 的
`reverse-i-search: {query}` 行（`docs/input/history-search.md`）与 `!` shell 模式
的红色 `Shell mode` 提示（`docs/input/shell-command.md`）。

### 状态（`app/`、`stream/`）

- `SessionInfo { model: String, cwd: String }` 与 `App.session:
  Option<SessionInfo>` —— 可直接显示的字符串，**在 I/O 边界**一次性注入，经由
  `App::set_session_info(model, cwd)`（`set_clock` 的模式：`main.rs` 做环境读取，
  纯核心只看到字符串）。未设置 —— 单元测试的默认 —— 意味着没有页脚，所以没有
  session info 的几何测试不受影响。
- `stream::ReplySource::model_name() -> String` —— 页脚点名所插入的任何 backend，
  所以换成一个真实模型就免费更新它（一个真实 backend 返回它真实的模型 id）。
  `DummyAi` 报告 `"dummy_model_name"`。

### 几何与显示（`ui/footer.rs`、`ui/layout.rs`）

- `footer_rows(app, band_rows) -> u16` —— 只要打开着 Ctrl+R 搜索或 `!` shell
  模式开着就是 `1`（搜索行 / `Shell mode` 提示占着槽位，**即使没有 session
  info** —— 这两种状态下都不可能有 band 打开）；否则在设置了 session info
  **且**没有 band 打开时是 `1`（`band_rows == 0` —— palette、`?` 快捷键 band 和
  `@` 文件 picker 都会顶掉这一行），否则是 `0`。band 取代页脚这个交换只在这一处
  决定；`live_height` 加上它，[`render_live`]/`cursor_position` 传入它，所以预留
  与绘制不会漂移。
- `footer_line(app, width) -> Line` —— `FOOTER_INDENT`（两个空格，codex 的
  `FOOTER_INDENT_COLS`）加 `{model} · {cwd}`，每一段有自己的颜色，而 ` · `
  分隔符是暗色的（`footer_color()`/`FOOTER_SEPARATOR`；见下文 *颜色*）；内容在
  溢出宽度时以收尾的 `…` 截断（codex 的
  `truncate_line_with_ellipsis_if_overflow`），每一段被截断的部分保留它的颜色。
  之后有两段在适用时追加到同一条暗色 ` · ` 链上 —— **context 量表**
  `{used}/{window} ({pct}%)`，只要模型的上下文窗口已知
  （`1.3k/160k (0.8%)`，两个计数都经 `format_token_count`，份额保留一位小数
  —— `docs/architecture/compact.md`）—— 以及运行中的**后台 shell** 计数
  （`docs/tools/background.md`）—— 而有一段坐在链*之外*，钉在这一行的**右边缘**：
  **权限模式**（`auto`/`manual`/`edit`/`master` —— `App::permission_mode`，由边界
  注入，权限被禁用时为 `None`，那就不显示它；`docs/state/permissions.md`）。它
  的列数加上 `FOOTER_MODE_GAP` 的间隔会事先从链的预算里预留出来，所以 `…` 截断
  总是吃左边的内文，绝不会吃掉那唯一一段有安全含义的东西。一整行读作
  `deepseek-v3.2 medium · ~/Codes/tmp · 1.3k/160k (0.8%) · 1 shell     auto`；
  一个被选中的速度档位跟在思考模式之后 —— `gpt-5.5 medium fast ·
  ~/repo`、`gpt-5.6-sol ultrafast · ~/repo` —— 穿着模型列表给该档位所起的名字
  （`/fast`、`/ultrafast`，`docs/providers/fast-mode.md`），而 standard 什么都不
  显示。**在一个 subagent 的 session 视图里，模型段与量表段描述的是那个 agent**
  （`docs/agents/agent-context-gauge.md`）：量表是被查看 agent 自己的 context
  —— `AgentRun::context_used`，最后一帧 usage 的 `input` 加上对之后加入内容的
  估算，一次落定没见到帧时用 transcript 的估算
  （`docs/architecture/token-estimate.md`）—— 对着它所跑模型的窗口
  （`App::context_gauge`），而一个定义钉住另一个模型的类型用那个模型名代替 session
  的，无模式（`App::viewed_agent_model`）；lead 的那一对随主视图回来，而只有它
  驱动自动压缩。最后那一段是唯一**可聚焦**的一段：从空闲的 composer 按 ↓ 会用
  palette 的青色点亮它（`footer_focus_bg()`/`footer_focus_fg()`），而 Enter 打开
  ↓ manager band，Claude Code 的风格。只有那一段的样式变化 —— 这一行其余每一段
  保持自己的颜色 —— 而截断路径会把样式带过去，所以窄宽度切断点亮的段时不会丢掉
  高亮（见 `docs/tools/background.md`）。
- `display_cwd(path, home) -> String` —— codex `format_directory_display` 的相对
  化：家目录本身是 `~`，家目录之下是 `~/sub`，否则（或没有已知家目录时）是绝对
  路径。纯的 —— `main.rs` 传入 `env::current_dir()` 与 `$HOME`。
- `live_height`/`live_layout`/`input_box` 多一个 `footer_rows` 参数；布局在最底部
  长出第四块区域 `[strip, input, band, footer]`，所以框的顶部 —— 以及光标 ——
  无论页脚（或取代它的 band）显示与否都待在原处。

### 颜色

Codex 的状态行在主题颜色打开时 —— *cyan model, green path* —— 从活动的
`/theme` 调色板读取，所以这一行像横幅一样跟随每一个主题（`docs/ui/theme.md`）。
颜色标记这一行**点名**的东西；各个名字之间的标点保持暗色：

```
  [magpie]deepseek-v4.1-flash medium fast · ~/Codes/rust/zeroth · 0/475k (0.0%)       master
```

- **model** —— `footer_model_color()`，强调色：这个 session 所对话的东西是这一行
  的第一个事实；`[provider]` 那对括号是这一行的暗色（`footer_color()`），所以
  provider 为模型取景却不与它争。subagent 被钉住的模型也穿强调色（没有括号 ——
  钉住不携带 provider）。
- **思考模式与速度档位** —— `footer_qualifier_color()`，状态行那个思考紫
  （`status_thinking_color()`），Ctrl+T 与一个档位的命令所改变的东西同一种颜色；
  框之上的 `Thinking for Ns` 从句穿同一个色相，所以这个努力和它所买来的思考读作
  同一个事实（`docs/ui/status-indicator.md`）。
- **cwd** —— `footer_cwd_color()`，成功绿，codex 的路径颜色；横幅的 cwd 行也穿
  它（`header_cwd_color()`，`docs/ui/header.md`），所以路径在应用点名它的任何地方
  都是同一种颜色。
- **context 量表** —— `footer_gauge_color(used, window, threshold)`：`/context`
  页面的份额规则，共享而不是抄一份（`context_fill_color`，那一页的副标题也调它）：
  还有空间时暗色，从 75% 起琥珀，从那一页为屏幕上对话所读的自动压缩阈值起红色
  （`App::context_usage_threshold`，没有时 90%）。一个带颜色的量表就是在说压缩
  快来了。
- **权限模式** —— `footer_mode_color(mode)`，随这个模式允许 agent 不问就做什么
  而升温：`manual`，一切之前都问，保持暗色；`edit`（文件改动不问）取链接蓝；
  `auto`（classifier 批准命令）是警示琥珀；而 `master`，什么都不问，是错误红
  （Claude Code 的 bypass-permissions 装扮）。四种色相每个主题都画得彼此可分，
  所以姿态一眼可读，而最有风险的那个最响。备选方案先做过样稿，但败给了同一行自己
  的那些颜色：紫色 `edit`（Claude Code 的 accept-edits 颜色）重复了思考模式的紫，
  青色重复了模型的强调色，绿色重复了路径的。
- ` · ` 分隔符、`…` 截断以及未聚焦的 shell 计数保持 `footer_color()`。

### 接线（`main.rs`，I/O 边界）

backend 选定之后立刻，`run` 注入 session info：
`app.set_session_info(backend.model_name(), display_cwd(cwd, home))`，`cwd`/
`home` 从环境读取。`live_region_height` 加上 `ui::footer_rows`，所以每一个
viewport 高度决定（绘制、`StreamDone` 的重新就位、resize）都已经把这一行算进去。

## 与 codex 的已知分歧

- **不可配置。** Codex 的 `/statusline` 让用户选项（分支、context %、……）并关掉
  它的主题颜色；我们的是固定的默认一对 —— 模型 + cwd —— 永远用主题颜色（上文
  *颜色*），从活动的 `/theme` 调色板绘制。以后更多的项是 `footer_line` 里新的段。
- **更少的页脚模式。** Codex 复用同一行给退出提醒、Esc 提示和队列提示；我们的
  槽位恰好有四个占用者 —— context 行、Ctrl+R 搜索行、`!` shell 模式提示，以及
  预置回溯的 `esc again to edit previous message` 提示（`backtrack_hint_line`，
  `docs/input/backtrack.md`）—— 外加顶掉它的那些 band。特别是 context 行在回合
  流式时保持可见（codex 在 composer 运行中途有草稿时为队列提示把它藏起来 —— 我们
  没有这样的提示）。
- **朴素的右截断。** Codex 在某些界面上对长路径做居中截断；页脚行只是以收尾的
  `…` 截断（codex 的页脚对拼好的行也是这么做的）。

## 测试

- `stream`：`DummyAi` 报告它的模型名。
- `app`：session info 默认未设置；`set_session_info` 存下那些显示字符串。
- `ui`：`footer_rows` 在没有 session info 时是 0 / 有它时是 1 / 有 band 打开时
  是 0 —— 而在打开着 Ctrl+R 搜索或 shell 模式开着时总是 1，无论有没有 session
  info（见 `docs/input/history-search.md` / `docs/input/shell-command.md`，它们
  测试这个槽位的其他占用者）；`footer_line` 渲染 `  {model} · {cwd}`，模型用
  强调色、cwd 绿色、分隔符暗色，每个主题把这两者彼此画开、也与暗色画开；思考模式
  与一个速度档位穿思考色；量表从 75% 起变琥珀、从阈值起变红；右边缘的模式在
  `manual` 时暗色、`edit` 时蓝、`auto` 时琥珀、`master` 时红，在每个主题里四种
  颜色都彼此可分；一个 ↓ 聚焦的 shell 计数亮成青色，而其余每一段保持它的文本和
  自己的样式；它在窄宽度下以 `…` 截断；`display_cwd` 把家目录映射为 `~`、家目录
  之下映射为 `~/sub`、之外/未知的家目录映射为绝对路径；`live_height` 随页脚长出
  一行；`render_live` 把页脚画在最后一行（palette 打开时不画）；页脚显示时光标
  不动；在一个 agent session 视图里，量表是被查看 agent 自己的 context，而一个
  被钉住的模型被点名时不带 session 的思考模式
  （`docs/agents/agent-context-gauge.md`）；一个被选中的速度档位把它的名字穿在
  思考模式之后，而 standard 什么都不加（`docs/providers/fast-mode.md`）。
- `scripts/smoke.sh`：启动那一帧在框之下显示 `dummy_model_name · ~`；打开
  palette（`/`）把它藏起来；关掉又把它带回来；Phase 111 在一个强制的窗口下打开
  一个 subagent 的 session 视图，读到的是那个 agent 的量表，不是 lead 的。
