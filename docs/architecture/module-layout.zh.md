# 模块布局：`app/`、`ui/`、`tui/` 与 `stream/`

`app.rs` 和 `ui.rs` 是所有东西都往里长的两个文件。等到拆分时，它们分别有
13,872 行和 17,665 行 —— 一个 4,038 行的 `impl App`，两个分别为 7,548 行和
9,688 行的测试模块，以及程序里所有渲染器肩并肩挤在一起。现在两者都是按领域
分模块的目录。

这次拆分是**纯粹的代码搬移**。每个条目都保留了它的函数体、它的文档注释，以及
它相对于邻居的位置；唯一的改动是模块头、import，以及放宽那些如今跨模块边界
访问的条目的可见性。公开 API 未变：每个 `mod.rs` 按名字重新导出它的各个领域，
所以每一条 `crate::app::X` 和 `ui::y(…)` 路径依然和以前完全一样地解析，
`term.rs` 则完全没有改动。

`main.rs` 是第三个这样的文件 —— 5,304 行，一个 2,058 行的 `run()` —— 现在它是
`src/main.rs`（77 行）加上 `src/tui/`，20 个按同样模式构建的领域模块。那次拆分
需要另外两次不需要的东西：循环的状态必须先变成 struct，代码才能搬移。参见
[`src/tui/` —— 终端外壳](#srctui--终端外壳)。

`stream.rs` 是第四个 —— 2,459 行，其中每个真实模型都实现的那个后端*接缝*
（seam）约 250 行，而离线 demo 后端有 1,100 行。参见
[`src/stream/` —— 后端接缝](#srcstream--后端接缝)。

## 各部分住在哪里

### `src/app/` —— 会话状态与纯更新逻辑

`mod.rs` 保留 `App` struct 本身。这是有意为之：一个 struct 的私有字段对定义它的
模块*及其所有后代*可见，所以把 `App` 留在 `mod.rs` 里，能让每个子模块 —— 以及
整棵测试树 —— 继续以它还是一个文件时的方式直接访问字段。没有任何字段需要为
这次拆分放宽可见性。

| 模块 | 内容 |
|--------|-------|
| `mod.rs` | `App` struct、它的核心消息/toast/时钟方法，以及 facade（`mod` + `pub use`）。 |
| `types.rs` | `Role`、`Message`、`HistoryItem`、`View`、`Toast`、`SessionInfo` —— 共享词汇。属于某个具体功能的类型则与它同住：`tools.rs` 里的 `ToolCall`、`compact.rs` 里的 `Compaction`、`turn.rs` 里的 `StreamError`/`InterruptedTurn`。 |
| `status.rs` | `TurnStatus`、`TurnSummary`、`TokenArrow`、`RetryInfo`，以及 spinner 行渲染的 token 统计。 |
| `action.rs` | `Action`：一次按键要求循环做什么。 |
| `keys.rs` | `on_key`（按 `View` 分发）与会话视图的按键映射。 |
| `composer.rs` | 粘贴占位符、Ctrl+V 图片附件、`!` shell 模式。 |
| `commands.rs` | `COMMANDS` 注册表、`/token` 过滤器，以及运行高亮的命令。 |
| `file_picker.rs` | `@` picker 的查询往返与路径插入。 |
| `input_history.rs` | `InputHistory`（↑/↓ 回溯）与 `HistorySearch`（Ctrl+R）。 |
| `queue.rs` | `QueuedTurn`、回合中途的队列，以及交给运行中回合的消息（`docs/input/queue.md`）。 |
| `tools.rs` | tool call 批次队列、一次 call 如何解析，以及 `PathDisplay` —— 文件工具路径*显示*时所用 cwd 相对 / `~` 相对 / 绝对的规则（`docs/tools/tools.md` *Path display*），在边界处注入。 |
| `drafts.rs` | 一轮中仍在**生成**的 call：`ArgsScanner`（到达即解码的流式 JSON 参数）、`ToolDraft` 与 `ToolDrafts` —— `write` 在其文件流入单元格时预览所依的生命周期（`docs/ui/write-streaming.md`）。 |
| `turn.rs` | 回合生命周期：begin/stream/interrupt/finish、状态统计、摘要。 |
| `compact.rs` | `/compact`、自动压缩（auto-compaction），以及上下文量表。 |
| `backtrack.rs` | Esc-Esc 回溯。 |
| `views.rs` | Ctrl+O 与 Ctrl+D overlay 的状态。 |
| `resume.rs` | `/resume` picker。 |
| `model_picker.rs` | 内联 `/model` picker。 |
| `login.rs` | 内联 `/login` onboarding。 |
| `look.rs` | 两种 look 共享的按目录持久化：`Look` trait 与 `LookFile<T>` —— `mascot.json`/`spinner.json` 格式、`config.json` 的“最后一项且被钉住”规则（`docs/state/per-directory-state.md`）。 |
| `mascot.rs` | 横幅 mascot 目录（带它的 `Look` 实现，`mascot` 键）、`/mascot` picker 的状态（`docs/ui/mascot.md`）。 |
| `spinner.rs` | 状态 spinner 样式目录（带它的 `Look` 实现，`spinner` 键）、`/spinner` picker 的状态（`docs/ui/spinner.md`）。 |
| `theme.rs` | 配色主题目录、`/theme` picker 的状态、`theme.json` 格式（`docs/ui/theme.md`）。 |
| `donate.rs` | 捐赠地址目录与只读 `/donate` 页面的状态（`docs/ui/donate.md`）。 |
| `export.rs` | `/export` 页面的状态、它的两个目标，以及 `conversation-YYYY-MM-DD-HHMMSS.txt` 文件名（`docs/ui/export.md`）。 |
| `context_usage.rs` | `/context` 页面的状态 —— 已计数的分解、它的签名、跟随最大类别（直到被移动）的高亮 —— 注入的 tool 集合、provider 的最后一帧，以及自启动以来的账本（`docs/architecture/context-usage.md`）。计数本身是顶层的 `crate::context_usage`。 |
| `secrets.rs` | `/secrets` 页面的状态 —— 列表、带掩码值的三字段表单 —— 以及它的 key 语法，外加对用户所输入内容的脱敏（`set_secret_redactor`、`redact_typed`）（`docs/state/secrets.md`）。 |
| `usage.rs` | `/usage` 页面的状态 —— 每个已登录订阅一节、每个结果所匹配的若干轮读取、跨多次打开保留的报告、分钟刷新的到期检查、注入的时钟与本地偏移、它的按键（`docs/providers/usage.md`）。报告本身是 `crate::llm::usage`。 |
| `permission.rs` | 内联 tool 权限 prompt：暂存的草稿、选项/修订用的按键映射（`docs/state/permissions.md`）。 |
| `ask.rs` | 内联 `AskUserQuestion` 模态框：tab/行状态、正在构建的答案、Other/notes 条目，以及针对权限 prompt 的队列（`docs/tools/ask.md`）。 |
| `background.rs` | 后台 shell 与 ↓ manager 条带。 |
| `agent.rs` | `Agent` tool 的名单（roster）、分组与通知。 |
| `reasoning.rs` | 思考流：实时推理缓冲区、它最终沉降进的 `Reasoning` 单元格，以及 provider 的推理 token 片段（`docs/ui/thinking-stream.md`）。 |
| `tool_group.rs` | 折叠的读取/搜索组：哪些 call 加入某个组（`group_kind`、`is_group_member`）、一个组在历史中延伸多远（`group_run`）、它统计什么，以及提交路径所持有的打开组（`place_resolved_tool`、`close_tool_group`）（`docs/ui/tool-groups.md`）。 |
| `tips.rs` | spinner tip 的行走：边界从 `tips.json` 播种的游标、`set_status_times` 运行的抽取，以及什么会隐藏该行（`docs/ui/tips.md`）。目录与文件格式是顶层的 `crate::tips`。 |

### `src/ui/` —— 纯渲染

| 模块 | 内容 |
|--------|-------|
| `mod.rs` | crate 的 import 与 facade（`mod` + `pub use`）。 |
| `theme.rs` | **每一个**样式常量与 live 区域几何常量。 |
| `wrap.rs` | `cols` 以及换行/截断的基础原语。 |
| `layout.rs` | `live_height`、`repin`（外加 `region_is_modal` —— `term.rs` 读取的谓词，用来记录权限 prompt 的单向滚动，供其关闭时的 purge 重建用，`docs/state/permissions.md`）、`cursor_position`/`cursor_visible`（硬件光标停在哪里，以及这一帧到底显不显示光标）、`input_box` —— `term.rs` 据以行动的几何策略。 |
| `assistant.rs` | markdown/代码渲染器（`AssistantRenderer`、`assistant_lines`）。 |
| `inline.rs` | 内联片段渲染：`**bold**`、`` `code` ``、链接，以及让片段跨行保持完整的换行。 |
| `table.rs` | GFM 表格尺寸、边框，以及窄终端下的记录式 fallback。 |
| `message.rs` | 渲染一条已提交消息，外加 `/compact` 标记单元格。 |
| `conversation.rs` | 整段历史的遍历，以及 purge 重建（resize、`/clear`、一次回退）所重绘的尾部。 |
| `tool.rs` | Tool 单元格：`● name(args)` 头部、`⎿` 输出块、运行中命令的实时尾部。 |
| `file_cell.rs` | `read`/`write`/`edit` 的带编号 diff 单元格。 |
| `tool_group.rs` | 折叠的读取/搜索组的两种样子 —— 带 `⎿` 提示的实时 `● Reading 2 files…` 单元格，以及已关闭组提交的沉降暗色行（`docs/ui/tool-groups.md`）。 |
| `inline_diff.rs` | 对 diff 单元格 `-`/`+` 对的字符级精修 —— 一行里究竟哪些字节变了（`docs/ui/inline-diff.md`）。 |
| `image.rs` | 图片在单元格下方预留的行 —— 内联图片的纯逻辑一半（`docs/ui/images.md`）。 |
| `status.rs` | 状态行（spinner、shimmer、统计）与 `Done for Ns` 摘要 —— 包括琥珀色的 `Waiting for internet…` 动词与 `offline for` 从句（`docs/providers/offline.md`）。 |
| `offline.rs` | 状态行下方的连接丢失行 —— 波纹、主机、尝试次数（`docs/providers/offline.md`）；`layout.rs` 的 `hang_rows` 把它与 checklist 的、tip 的一起求和。 |
| `tips.rs` | 状态行下方的 spinner tip 行 `⎿  Tip: …`（`docs/ui/tips.md`）；`layout.rs` 的 `hang_rows` 把它与 checklist 的一起求和。 |
| `agent.rs` | Subagent 树、单元格与页脚名单。 |
| `menu.rs` | palette / `@` picker / `?` 快捷键条带。 |
| `footer.rs` | 页脚行、toast、排队行，以及什么会顶掉页脚。 |
| `header.rs` | 启动横幅 —— 元数据列旁的渐变 mascot（`docs/ui/mascot.md`）。 |
| `mascot_view.rs` | 带实时横幅预览的内联 `/mascot` picker（`docs/ui/mascot.md`）。 |
| `spinner_view.rs` | 内联 `/spinner` picker —— 覆盖在示例状态行预览上的实时 spinner 行（`docs/ui/spinner.md`）。 |
| `palette.rs` | 按主题划分的颜色表，以及每个渲染器读取的环境当前活动主题（`docs/ui/theme.md`）。 |
| `theme_view.rs` | 内联 `/theme` picker —— picker 家族形状的色板行，旁边是（窄终端下则在其下方）高亮主题中真实单元格的预览（`docs/ui/theme.md`）。 |
| `donate_view.rs` | 只读 `/donate` 页面 —— 每个地址的带标签圆角框之上的渐变标题（`docs/ui/donate.md`）。 |
| `export_view.rs` | 只读 `/export` 页面 —— 高亮目标描述之上的两行目标 —— 以及 `export_text`，即纯文本形式的 transcript（`docs/ui/export.md`）。 |
| `context_usage_view.rs` | 只读 `/context` 页面 —— 窗口条、旁边或下方带高亮类别各部分的图例、provider 的用量行与缓存量表，以及它的腾挪阶梯（`docs/architecture/context-usage.md`）。 |
| `secrets_view.rs` | `/secrets` 页面 —— 以固定掩码的占位符呈现的列表、在 builder 中被掩码的表单 —— 以及表单的光标座位（`docs/state/secrets.md`）。 |
| `usage_view.rs` | 只读 `/usage` 页面 —— 每个订阅一节、页面量表统一的一种条长、`limit reached`、重置倒计时与本地时间、窄行会放弃什么、高亮订阅的链接、新鲜度，以及它的腾挪阶梯（`docs/providers/usage.md`）。 |
| `live.rs` | `render_live` —— 流式条、框、条带。 |
| `transcript.rs` | Ctrl+O overlay 与 `TranscriptCache`。 |
| `classifier_view.rs` | Ctrl+D 视图的 classifier 页面主体（`docs/state/permissions.md`）。 |
| `context_view.rs` | Ctrl+D 上下文调试 overlay。 |
| `resume_view.rs`、`model_view.rs`、`login_view.rs`、`background_view.rs` | 各个 picker 与 manager 条带。 |
| `permission_view.rs` | tool 权限模态框（`docs/state/permissions.md`）。 |
| `ask_view.rs` | `AskUserQuestion` 模态框：chip 条、选项页、预览面板、复核页（`docs/tools/ask.md`）。 |
| `reasoning.rs` | 思考流的单元格：实时 `● Thinking…` 块、折叠的 `Thought for …` 行，以及它的 Ctrl+O 展开（`docs/ui/thinking-stream.md`）。 |
| `stream_render.rs` | `StreamRender`，增量的“提交到 scrollback”渲染器。 |

### `src/tui/` —— 终端外壳

`main.rs` 是 I/O 边界，所以它是单元测试够不到的那个文件单位（除个别纯辅助函数
之外）—— 而这正是它长得最糟的原因。5,304 行里塞着十几个互不相干的边界关切
（CLI、provider 配置、会话录制、输入历史持久化、`/resume` 扫描、`!` shell 运行器、
三个 worker 线程、stream 折叠、agent 折叠、渲染），外加一个 2,058 行的 `run()`，
其 `select!` 的 input 分支带着约 40 条内联 `Action` 分支。

`main.rs` 现在只做四件事 —— detached-exec hook、CLI 解析、打开 viewport、运行
循环 —— 其余一切都住在这里：

| 模块 | 内容 |
|--------|-------|
| `mod.rs` | `Session` struct 与 `StatusClocks` —— 循环的状态。 |
| `event_loop.rs` | `run`：对十一个来源的 `select!` —— 每个分支一次 handler 调用。 |
| `actions.rs` | `Action` 分发 —— 每个按键结果一条分支 —— 外加 `Flow`、`/clear`、`/copy`、resize 与粘贴的路由。 |
| `turn.rs` | 回合生命周期：`start_turn`、`run_shell`、后台 follow-up、`/compact`、`dispatch_after_turn`、`abandon_inflight`。 |
| `stream.rs` | `on_stream_event`：把一个回复事件折叠进 `App` + scrollback。 |
| `agent.rs` | Subagent 事件、名单的时钟、linger 清扫、会话视图的各分支（`docs/agents/agent-tool.md`）。 |
| `background.rs` | 后台 shell 事件与 `settle_bg_completions`（`docs/tools/background.md`）。 |
| `permission.rs` | `PermissionStore` —— permission gate、它的文件、本项目的键 —— 以及 prompt 的答案（`docs/state/permissions.md`）。 |
| `ask.rs` | `AskUserQuestion` 空闲时钟的边界一半：每次按键、粘贴和绘制都把 `Instant::now()` 送进纯的 `ask::AskTimer`，以及使等待中的问题以未作答状态结束的到期（`docs/tools/ask.md`）。 |
| `view.rs` | 绘制：draw tick、各 overlay、各次重绘、`live_region_height`、注入的时钟。 |
| `commit.rs` | Scrollback 提交 —— invariant 4 唯一被强制执行的地方 —— 以及 toast。 |
| `models.rs` | `ModelSession`：backend 以及选择它的每一个旋钮，外加 `/model`、Ctrl+T 与探测的分支（`docs/providers/llm.md`）。 |
| `login.rs` | `/login` 流程的边界一半：打开它、运行某个订阅的登录 —— 按 provider 是设备码或浏览器 PKCE 流程 —— 并持久化它所铸造的东西（`docs/providers/copilot.md`、`docs/providers/chatgpt.md`）。 |
| `config.rs` | 读取环境：provider、key、按目录的 `/model` 选择与 `/settings` 旋钮（`docs/state/per-directory-state.md`）、权限规则、按用户的 `telemetry.json`（`TELEMETRY.md`）、`update.json`（`docs/ops/update.md`）与 `tips.json`（`docs/ui/tips.md`）、各条路径。 |
| `bootstrap.rs` | `Session::bootstrap` / `shutdown` / `after_iteration` —— 装配、拆除、循环底部的工作。 |
| `startup.rs` | `--continue`/`--resume`/`[PROMPT]` 的参数解析、`-C` 的 `chdir`、带样式的 `--help` / usage 错误打印，以及 `mcp` / `update` 子命令分发（`docs/cli/cli.md`）。 |
| `headless.rs` | `zeroth -p`：在 runtime 之前从参数和 stdin 读取的 prompt、无屏幕地运行同一个 `Session` 的循环、信号、各条流、退出状态（`docs/cli/headless.md`）。 |
| `recorder.rs` | `SessionRecorder`：把历史镜像到 rollout 文件（`docs/state/resume.md`）。 |
| `resume.rs` | 在磁盘上寻找已录制的 session，以及 `/resume` + 回溯的分支。 |
| `history_store.rs` | `InputHistoryStore`（`docs/input/history-persistence.md`）。 |
| `mascot.rs` | 应用一次 `/mascot` 选择：按目录写 `mascot.json` + 重绘横幅（`docs/ui/mascot.md`）。 |
| `spinner.rs` | 应用一次 `/spinner` 选择：按目录写 `spinner.json` + toast（`docs/ui/spinner.md`）。 |
| `theme.rs` | 应用一次 `/theme` 选择：写 `theme.json`、切换 palette、重新着色的重建（`docs/ui/theme.md`）。 |
| `donate.rs` | 应用一次 `/donate` 复制：写剪贴板 + toast（`docs/ui/donate.md`）。 |
| `export.rs` | 应用一次 `/export` 选择：写剪贴板，或按本地时钟写 `conversation-….txt` 文件，+ toast（`docs/ui/export.md`）。 |
| `context_usage.rs` | 让打开的 `/context` 页面保持实时：在打开时和每次循环底部把 backend 的 tool 集合（或被查看 agent 类型的）交出去，然后重新计数（`docs/architecture/context-usage.md`）。 |
| `secrets.rs` | `/secrets` 页面的保存、删除与复制：`secrets.json` 的读-改-写、共享 store、提醒同步（`docs/state/secrets.md`）。 |
| `usage.rs` | `/usage` 页面的读取：每个已登录订阅一个 worker、它们所回答的那一轮、其倒计时所依的墙钟、分钟刷新、空态 toast、`c` 写剪贴板（`docs/providers/usage.md`）。 |
| `telemetry.rs` | 每天一次的匿名用量 ping（更新当天再多一次）：install id 的铸造、横幅下方的一次性提示、分离式发送，以及对已送达日期与版本的记录（`TELEMETRY.md`）。 |
| `update.rs` | 每天一次的更新检查：对仓库最新 release 的分离式 `HEAD`、横幅下方的 `Update available` 卡片（有回合在流式输出时暂缓）、尝试与提示的日期记在 `update.json`（`docs/ops/update.md`）。 |
| `update_cli.rs` | `zeroth update` 子命令：同样的检查，然后把单行安装器取到临时文件，并在这个二进制自身所在目录上运行（`docs/ops/update.md`）。 |
| `herdr.rs` | 向它所运行的 herdr 窗格报告 session 的状态：循环底部同步进库里的 `herdr::Tracker` 与 `herdr::Reporter` worker、失败回合的保持、resume 命令中经 `PATH` 检查的第一个词、退出时的释放（`docs/architecture/herdr.md`）。 |
| `shell.rs` | `!` 命令运行器及其 drain/cap 单元测试（`docs/input/shell-command.md`）。 |
| `workers.rs` | 线程外的文件搜索 / 剪贴板 / 模型列表 / 设备登录 / usage / telemetry ping / update 检查任务。 |
| `host.rs` | 时钟、日期、OS 字符串、uid、id —— 原始的杂质。 |

### `src/stream/` —— 后端接缝

`stream.rs` 装着两件彼此毫无关系的东西：*每个* backend 都讲的协议 ——
`StreamEvent`、`ReplySource`、`CancelToken`，约 250 行 —— 以及 `DummyAi`，那个
1,100 行的离线 demo，每落地一个 UI 功能它的固定回复就长一截（它是
`scripts/smoke.sh` 唯一能驱动的 backend）。想读接缝，就得先滚过一段段固定的
`ping` 输出。

| 模块 | 内容 |
|--------|-------|
| `mod.rs` | facade（`mod` + `pub use`）。 |
| `event.rs` | `StreamEvent` 及其负载（`ToolCallSummary`、`AgentSpec`、`AgentCallDone`、`TokenUsage`）—— 整个线上格式。 |
| `source.rs` | `ReplySource`：事件循环所依赖的那一个 trait，外加 `AgentChatDelivery` —— 发进 subagent session 的消息是怎么落地的（`docs/input/queue.md`）。 |
| `cancel.rs` | `CancelToken`。 |
| `stall.rs` | `StallAi`，卡死后端的替身（`docs/input/interrupt.md`）。 |
| `dummy/mod.rs` | `DummyAi` —— `ReplySource` 实现、`turn_events`、回放节奏。 |
| `dummy/scenario.rs` | 场景注册表：一个 prompt 会选中哪个 demo。 |
| `dummy/script.rs` | 固定回复与流式原语。 |
| `dummy/turns.rs` | **纯**的脚本化回合，每个一个 `Cue -> Vec<StreamEvent>`。 |
| `dummy/gated.rs` | 在 permission gate 上阻塞的回合：各个权限 demo 与 `AskUserQuestion` 往返（`docs/tools/ask.md`）。 |
| `dummy/agent.rs` | 在 agent 通道上流式输出一个已启动 **subagent 自身回合**的回合 —— agent 会话视图的离线驱动（`docs/agents/agent-view-streaming.md`）。 |

dummy 是一个子树，而不是四个同级文件，因为它确实可分：它是唯一一个能删掉而
不动到接缝的 backend，而现在这是一次成目录，而不是一次切除。

与另外三次拆分不同，这一次并非*仅仅*是代码搬移。哪个 demo 会播放，过去由两个
不同文件里的两条手写 `if`/`else` 链决定，九个 cue 且没有表 —— 于是被更早的 cue
遮蔽的 cue 会悄悄让一个 demo 退役，而 smoke 套件在测试一个与它以为不同的回合
时仍会一直通过。现在是一张有序的 `SCENARIOS` 表，而且套件证明每一项仍然可达。
参见 [`dummy-backend.md`](dummy-backend.md)。

#### 为什么这次拆分必须先有 struct

`app.rs` 和 `ui.rs` 是条目的集合，所以搬移它们就够了。`run()` 是一个持有约 50 个
局部变量的函数，它的辅助函数每次要接九个参数（有五处 `#[allow(clippy::too_many_arguments)]`
标在最疼的地方）。在没有状态类型的情况下把*那*切成模块，只会放宽签名。

所以状态变成了 **`Session`**（40 个字段），每个 handler 都是它上面的一个方法。
这和 `app/mod.rs` 对 `App` 玩的把戏相同：一个 struct 的私有字段对定义它的模块
*及其所有后代*可见，于是每个领域模块持有一个 `impl Session` 块并直接访问状态。
没有任何字段是 public 的，也没有任何 accessor 的存在只是为了跨越模块边界。

三个内聚的组变成了它们自己的类型，因为它们的部件总是只能一起移动：

- **`ModelSession`** —— backend 加上 provider 表、key store、活动模型、它的
  推理模式、图片支持与上下文窗口。它拥有后台/subagent 注册表和 permission gate 的
  *克隆*，于是 `ModelSession::rebuild` 能重新接上整套；一次只接上部分的
  rebuild 过去会悄悄丢掉 `agent` tool。
- **`PermissionStore`** —— permission gate、它的规则所持久化到的文件，以及那个
  文件里本项目的键。每个改变规则的方法都在同一口气里持久化它，所以不存在授予了
  却忘记保存的可能。
- **`StatusClocks`** —— 早已存在；它新增了 `start_turn`/`end_turn`，让四条回合
  起始路径不会在各自重置哪些时钟上走偏。

#### 通道也住在它上面

九个通道的两端都是 —— 发送者*以及* `select!` 轮询的那些接收者。

这次拆分的第一版把接收者放进一个单独的 `Sources` struct，因为相信 `select!`
需要它们作为可以分别借用的不同位置。**那是错的**，而且它花了实实在在的管道：
回复接收者不得不被线程穿过 `on_terminal_event` → `on_action` →
`interrupt_turn`/`clear_conversation`，只为让那条链的底部能换掉通道 —— 恰恰是
这次重构要移除的手工穿线状态。

`select!` 会限定它的 future 作用域。它构建的九个 future 借用同一个 `&mut` 位置的
九个*不同字段*，这是允许的，而且它们都在胜出分支的函数体运行前被丢弃 —— 所以
那个函数体可以自由地拿 `&mut session`。一个会排空第二个通道的 handler 用同样的
方式做，因为 `recv`/`try_recv` 返回拥有所有权的值，调用之后不保留借用：

```rust
pub(crate) fn drain_agent_events(&mut self) {
    while let Ok(AgentEvent::Stream { id, event }) = self.agent_rx.try_recv() {
        self.on_agent_event(&id, event);   // &mut self, mid-drain
    }
}
```

所以接收者是字段，`Session::abandon_inflight` 就地换掉回复通道的两端，并且
有三个签名少了一个参数。

#### 现在的循环读起来是什么样

```rust
Some(event) = sources.reply_rx.recv() => session.on_reply_event(event, &mut sources.agent_rx),
Some(()) = sources.draw_rx.recv() => session.on_draw_tick()?,
Some(result) = sources.file_rx.recv() => session.on_file_matches(result),
```

现在加一个功能就是：在拥有它的领域模块里写一个方法，再在 `actions.rs` 的分发里
加一行，或在 `select!` 里加一个分支。

#### 行为不变

这是一次重构：同样的 1,807 个单元测试、同样的 7 个 shell drain/cap 测试（随
运行器一起搬进 `tui/shell.rs`）、同样的集成测试，以及一次完整的 `scripts/smoke.sh`
运行 —— 全部 64 个阶段 —— 通过。每条分支的函数体、顺序和注释都原样搬移；
循环底部序列、提交顺序、回合结束的分发顺序，都是同样的语句、同样的次序。

沿途顺手整理了四件事，它们全都是注释里的潜伏 bug 而非代码：`main.rs` 里有四条
文档注释被后来的插入孤立到了*下一个*函数上（`build_backend` 的落到了
`permissions_enabled`、`dispatch_after_turn` 的落到了 `checkpoint_turn_end`、
`overlay_return_clear` 的落到了 `repaint_active_view`、`spawn_file_search_worker`
的落到了 `spawn_image_paste`）。现在每条文档都与它所描述的条目同住。

#### 阅读功能文档

`docs/` 下的各功能文档在散文中把边界称作 `main.rs` —— “the boundary
(`main.rs`)”、“`main.rs` reads the env”等等。每一处*符号限定*的引用都已更新到
它的新路径（`tui::turn` 的 `Session::start_turn`、`tui::config::save_selection`
等等），所以按名字搜索会落到正确的模块；文档中单独写 `main.rs` 的地方，请读作
“边界”，而上表说明了哪个模块持有它。

### `src/pty/` —— 交互式终端 session

不是一次拆分：生来就是目录，因为一个交互式 session（`docs/tools/interactive-shell.md`）
是若干围绕单一边界的纯核。这些核心不携带进程，可单独单元测试；`spawn` 是 I/O。

| 模块 | 内容 |
|--------|-------|
| `mod.rs` | 模块列表。 |
| `keys.rs` | `input` 记法 —— `<Enter>`、`<C-c>`、`<Up>` —— 的解析、编码为终端字节、单行显示；双转义撤销。 |
| `transcript.rs` | 以文本行呈现的输出（`vte::Perform`）：普通程序所报告的内容。 |
| `screen.rs` | 以屏幕呈现的输出（`vt100`）：全屏程序所报告的内容，以及终端查询的回复。 |
| `settle.rs` | 一次等待中的 call 何时返回 —— 退出、prompt、行静止、超时。 |
| `pace.rs` | 运行中的单元格何时被发去下一次更新 —— 每个间隔，在输出两帧之间的停顿里。 |
| `report.rs` | 模型读到的内容：覆盖在输出或屏幕上的 `Running`/`Stopped` 帧，以及仅给模型的注记。 |
| `session.rs` | `SessionIo` —— 两个视图、等待、外观，以及 monitor 与各次 call 共享的退出握手。 |
| `spawn.rs` | 边界：伪终端、控制终端的各个层级、session 环境、行模式。 |

### `src/read_only/` —— 识别只读命令

同样生来就是目录：一个 tokenizer 之上的两个问题（`docs/ui/tool-groups.md`、
`docs/state/permissions.md`），每一部分都是纯的。

| 模块 | 内容 |
|--------|-------|
| `mod.rs` | `read_kind`（一个命令读什么 —— 折叠组的各子句）与 `is_read_only`（它是否可证明地只读 —— auto 模式的 classifier 跳过），以及程序集合、按程序的写检查。 |
| `lex.rs` | shell 词 tokenizer：引号、转义、操作符、重定向及其无害形式，以及一个命令的词无法担保的 `Hazards`。 |
| `flags.rs` | 按程序的 flag 白名单及其校验器（`grep`、`rg`、`fd`、`sort`、`ps`、`xargs`、`sed` 等）。 |
| `git.rs` | 只读的 `git` 动词及各自接受的 flag。 |

## 拆分必须保留的约定

**样式仍然集中管理。** `ui.rs` 的每个 `const` 都搬进了 `ui/theme.rs` —— 没有一个
被散落到使用它们的模块。要换主题或改尺寸，仍然在那里；其他模块写
`use super::theme::*`。

**所有宽度计算仍然走 `cols`。** 它现在住在 `ui/wrap.rs`，需要测量的模块显式
import 它。

## facade

每个 `mod.rs` 都**按名字**重新导出它的各个领域，而不是 `pub use self::m::*;`。
通配导出能让路径继续工作，却让公开面变得隐含：没有任何东西声明该模块导出什么，
而后来加在某个子模块上的 `pub` 会未经任何人决定就逃出 crate。
`tests/api_surface.rs` 锁住了结果 —— 它列出了拆分前的 `app.rs`/`ui.rs` 所导出的
全部 144 个条目，加上 `stream.rs` 的 18 个，所以忘掉一个重导出就编译不过。

给每个重导出命名，也暴露出通配导出一直在悄悄做的事：它以条目自身的可见性把
`pub(super)` 条目重新导出进了 `crate::app` / `crate::ui`，兄弟模块正是借此通过
`use super::*` 拿到它们。现在这些是显式 import。

`src/tui/` 没有 facade，也不需要：它属于**二进制** crate，所以二进制之外没有
东西能命名它，也就没有公开面需要锁。`main.rs` 恰好触及三条路径 ——
`tui::startup::resolve_cli`，然后 `tui::event_loop::run`，或在 `-p` 运行时是
`tui::headless::run`（`docs/cli/headless.md`）。

## 文档链接

`cargo doc --no-deps --lib` 是 gate 的一部分。crate 对警告 deny，这会把一条坏掉的
intra-doc 链接升级为错误 —— 而模块拆分按构造就会弄坏链接，因为一个裸的
`[`name`]` 在新家会按不同作用域解析，而一个 public 条目可能不许链接到 private
条目。两类问题都在这里修好了（拆分前 77 个错误，现在 0 个）：目标仍然 public
时限定路径，不是时就降级 `[`X`]` 为 `` `X` ``。

## 可见性

把一个文件拆成许多，会将文件内访问变成跨模块访问，于是其他领域伸过来拿的条目
从 private 放宽到 `pub(super)` —— `pub(in crate::app)` / `pub(in crate::ui)`。
没有原本 crate 私有的东西变成 public，也没有任何原本 public 的东西改变。条目若
只在它自己的模块内使用，就保持 private。

在 `src/tui/` 里，对等物是 `pub(crate)`，它伸不出二进制。**每一个** `Session`
字段都保持 private —— 上面那个后代可见性的把戏正是让它成立的原因，也正是为什么
没有任何 accessor 的存在只是为了跨越模块边界。

## 测试

测试随代码搬进了 `src/app/tests/` 和 `src/ui/tests/`，每个领域一个文件。由于这些
文件是 `crate::app` / `crate::ui` 的后代，它们对私有状态的触及范围与原先那个
单一扁平 `mod tests` 完全一致。

一个辅助函数只有在两个或更多领域文件使用它时才住在 `tests/mod.rs` —— 按传递
关系判定，因为一个辅助函数可能通过另一个辅助函数被触及。其他一切都被下推到它
唯一的调用者。

拆分没有改变测试集：同样的 1591 个单元测试、同样的名字，全绿，外加干净的
`cargo fmt --check`、`cargo clippy --all-targets -- -D warnings`、
`cargo doc --no-deps --lib`，以及一次完整的 `scripts/smoke.sh` 运行。

`src/tui/` 是 I/O 边界，所以它由 `scripts/smoke.sh` 而非单元测试来验证 —— 只有
一个例外，也是它一直以来的那一个：`!` 运行器的读取器泛型 drain/cap 对，其 7 个
测试随它搬进了 `tui/shell.rs`。`tui/` 拆分之后的套件是 1,807 个单元测试（库自
前两次拆分以来长大了）、每个二进制目标的那 7 个，以及集成测试，全绿，且全部 64
个 smoke 阶段通过。
