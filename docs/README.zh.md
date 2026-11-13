# 文档

一个设计决策一页。`AGENTS.md` 说改代码前要跑什么；`CLAUDE.md` 给的是架构地图和四条运行时不变量；本索引说的是每个机制住在哪。新功能在这里占一页并列进下面的清单——**一页与代码不再相符，就是一个 bug**。

> **关于译本。** 英文原文是本目录的唯一事实来源，路径不变（`docs/cli/headless.md`）。中文译本旁挂在同目录、同名的 `.zh.md` 上（`docs/cli/headless.zh.md`），因此代码里那五千多处 `docs/…md` 引用、`tests/api_surface.rs` 的路径断言、以及 `cargo doc` 的内部链接都无需改动。
>
> 译本**落后于原文时以原文为准**。只改英文而忘了同步译本是允许的（原文永远正确）；反过来不行。译文里保留英文术语（tool call、fallback、cooldown、seal 等），因为它们与代码里的标识符一一对应，译成中文反而对不上。
>
> **怎么保证译本没跑偏。** 散文本身靠译者；结构不靠。`python3 scripts/docs/zh_parity.py`
> 逐对比较三样：**围栏代码块连同其 info string 逐字节相同**（命令、help 输出、JSON
> 载荷一旦漂移比不译更糟——有人会照抄）、**行内 `code span` 一个不少**（它们点名标识符、
> flag 与文件）、**链接目标集相同**（路径是共用的）。标题的数量与层级也在其列：读得懂
> 译本的人读不懂原文，所以「结构对不上」这类 bug 只有机器看得见。译文里的纯锚点
> （`](#标题)`）随标题一起翻译，因此只要求它在本文档内解析得到，不与英文文本比对；
> 散文的长度与换行随译者。改动任何有译本的页面后跑一次，改动矩阵见 `AGENTS.md`。
>
> 目前除 `docs/research/` 的三篇研究文档外，全站都有译本：`docs/` 其余分类、`guide/`
> 与仓库顶层。那三篇（`harness-network-security`、`harness-security-threats`、
> `miyu-path-jail`）**本来就是用中文写的**，没有英文原文，所以也没有 `.zh.md` ——
> 在它们旁边放一份译文只会是一份字节复制，结构对账必然通过，看起来被维护着，实则
> 什么也没做。`scripts/docs/zh_parity.py` 会把这种复制报为失败，也不会把这类页面
> 列进「待译」。

## 从这里开始

- [`design.md`](design.md)：内联会话 TUI —— 设计

## 架构

这套 crate 是怎么拼起来的：模块布局、异步循环、上下文、token 与内存规则。

- [`architecture/async-rewrite.md`](architecture/async-rewrite.md)：异步运行时重写（codex 风格）
- [`architecture/compact.md`](architecture/compact.md)：`/compact` —— 把对话摘要化以腾出上下文
- [`architecture/concurrency.md`](architecture/concurrency.md)：并发 —— 回合在哪里阻塞，什么在同时跑
- [`architecture/context-usage.md`](architecture/context-usage.md)：`/context` —— 什么在占用上下文窗口
- [`architecture/context.md`](architecture/context.md)：对话上下文、视觉附件与 Ctrl+D 调试视图
- [`architecture/dummy-backend.md`](architecture/dummy-backend.md)：dummy 后端及其场景注册表
- [`architecture/environment.md`](architecture/environment.md)：系统提示里的环境上下文 —— 设计
- [`architecture/herdr.md`](architecture/herdr.md)：herdr 支持 —— 由 agent 自己上报的窗格状态
- [`architecture/memory.md`](architecture/memory.md)：常驻内存：应用持有什么，`/model` 曾经贵在哪
- [`architecture/module-layout.md`](architecture/module-layout.md)：模块布局：`app/`、`ui/`、`tui/` 与 `stream/`
- [`architecture/multithread-core.md`](architecture/multithread-core.md)：多线程核心 —— 一个进程里的 N 个会话，实测
- [`architecture/naming.md`](architecture/naming.md)：命名 —— 完成 Zeroth 改名
- [`architecture/token-estimate.md`](architecture/token-estimate.md)：token 估算，以及 provider 的修正
- [`architecture/unfinished-replies.md`](architecture/unfinished-replies.md)：回合绝不终结在一个并不完整的回复上

## UI

屏幕上有什么：live region、流式渲染器、主题、全屏页与 transcript 翻页器。

- [`ui/copy.md`](ui/copy.md)：`/copy` —— 把最后一条回复复制到剪贴板
- [`ui/diff.md`](ui/diff.md)：`/diff` —— 审阅项目的 Git 改动
- [`ui/donate.md`](ui/donate.md)：`/donate` —— 加密货币捐赠页
- [`ui/command-tabs.md`](ui/command-tabs.md)：命令页 —— tab，以及如何让它们各自的状态分开（设计）
- [`ui/export.md`](ui/export.md)：`/export` —— 把对话导出为纯文本
- [`ui/flicker.md`](ui/flicker.md)：无闪烁渲染 —— scrollback 提交并入绘制帧
- [`ui/footer.md`](ui/footer.md)：输入框下方的会话上下文页脚
- [`ui/full-screen.md`](ui/full-screen.md)：自己拥有一块屏幕的 TUI —— 全屏模式（设计）
- [`ui/header.md`](ui/header.md)：启动横幅
- [`ui/images.md`](ui/images.md)：内联图片
- [`ui/inline-diff.md`](ui/inline-diff.md)：`edit`/`write` 单元格里的字符级差分标记
- [`ui/links.md`](ui/links.md)：可点击链接 —— 为换行 URL 生成 OSC 8 超链接
- [`ui/long-lines.md`](ui/long-lines.md)：极长的输出行 —— 有界行数、诚实的计数
- [`ui/markdown.md`](ui/markdown.md)：助手侧的 markdown 渲染
- [`ui/mascot.md`](ui/mascot.md)：横幅吉祥物与 `/mascot` 选择器
- [`ui/overlay-repaint.md`](ui/overlay-repaint.md)：覆盖层重绘 —— 在回合运行期间从 Ctrl+O / Ctrl+D 里抄文字
- [`ui/rich-links.md`](ui/rich-links.md)：富链接 —— 可点击的路径、预览卡片与屏幕归属（设计）
- [`ui/shortcuts.md`](ui/shortcuts.md)：`?` 快捷键条 —— 输入框下方的键盘快捷键帮助
- [`ui/slow-stream.md`](ui/slow-stream.md)：慢速流 —— 每秒几个 token 穿过内联管线
- [`ui/spinner.md`](ui/spinner.md)：状态 spinner 样式与 `/spinner` 选择器
- [`ui/status-indicator.md`](ui/status-indicator.md)：状态指示器（codex / Claude Code 风格）
- [`ui/strip-flow.md`](ui/strip-flow.md)：strip 流动 —— 流式条冻结而不是消失
- [`ui/table-streaming.md`](ui/table-streaming.md)：流式 GFM 表格（整体缓冲，实时预览）
- [`ui/theme.md`](ui/theme.md)：配色主题与 `/theme` 选择器
- [`ui/thinking-stream.md`](ui/thinking-stream.md)：思考流
- [`ui/timestamps.md`](ui/timestamps.md)：Ctrl+O transcript 里的时间戳 —— 设计
- [`ui/tips.md`](ui/tips.md)：Spinner 提示 —— 回合运行期间状态行下方的一句提醒
- [`ui/toast.md`](ui/toast.md)：瞬时 toast
- [`ui/tool-groups.md`](ui/tool-groups.md)：折叠的读/搜组 —— 非 verbose 的工具视图
- [`ui/tool-pulse.md`](ui/tool-pulse.md)：运行中圆点的闪烁
- [`ui/tool-streaming.md`](ui/tool-streaming.md)：实时流式输出 `bash` 的输出 —— 运行中的尾巴 + 折叠的窥视行
- [`ui/tool-view-performance.md`](ui/tool-view-performance.md)：Ctrl+O 的性能：增量 transcript 缓存与原子切换
- [`ui/view-flow.md`](ui/view-flow.md)：视图流动 —— 过长的内联视图内容流入 scrollback
- [`ui/write-streaming.md`](ui/write-streaming.md)：模型写文件时流式显示这个 `Write`

## Input

输入框，以及敲进去的一切：textarea、历史、选择器、队列、shell 命令与中断。

- [`input/backtrack.md`](input/backtrack.md)：Esc Esc —— 编辑上一条消息（backtrack）
- [`input/file-search.md`](input/file-search.md)：`@` 文件路径提及 —— 输入框下方的文件选择器
- [`input/history-persistence.md`](input/history-persistence.md)：跨会话输入历史 —— 把 ↑/↓ 与 Ctrl+R 的历史落盘
- [`input/history-search.md`](input/history-search.md)：Ctrl+R 历史搜索 —— 对已提交输入的反向增量搜索
- [`input/image-paste.md`](input/image-paste.md)：Ctrl+V 粘贴图片
- [`input/input-history.md`](input/input-history.md)：↑/↓ 输入历史 —— shell 风格地召回已提交消息
- [`input/interrupt.md`](input/interrupt.md)：Esc 中断进行中的回合 —— 设计
- [`input/paste.md`](input/paste.md)：粘贴内容的占位符
- [`input/queue.md`](input/queue.md)：回合运行期间给消息排队
- [`input/shell-command.md`](input/shell-command.md)：`!` 执行 shell 命令 —— 从输入框跑一条本地命令
- [`input/shift-enter.md`](input/shift-enter.md)：Shift+Enter 插入换行 —— 设计
- [`input/skill-mentions.md`](input/skill-mentions.md)：`$` skill 提及 —— 输入框下方的 skill 选择器
- [`input/textarea.md`](input/textarea.md)：Textarea —— codex 风格的可编辑输入区

## 工具

模型能做什么：工具循环、bash 及其终端引擎、MCP、hooks、skills、tasks、后台 shell 与 scratchpad。

- [`tools/ask.md`](tools/ask.md)：`AskUserQuestion` 工具 —— 在回合中途向用户提问
- [`tools/background.md`](tools/background.md)：后台 shell —— `wait: 0`、一次会超时的 `wait`、Ctrl+B 与 ↓ 管理器
- [`tools/bash-tools.md`](tools/bash-tools.md)：bash 工具 —— 每条命令一个真实终端，每个动作一个工具
- [`tools/hooks-menu.md`](tools/hooks-menu.md)：`/hooks` 菜单 —— hooks 浏览器
- [`tools/hooks.md`](tools/hooks.md)：生命周期 hooks
- [`tools/interactive-shell.md`](tools/interactive-shell.md)：交互式 shell —— bash 工具背后的终端引擎
- [`tools/mcp.md`](tools/mcp.md)：MCP 服务器 —— `/mcp` 管理器与 `mcp__server__tool` 工具
- [`tools/parallel-tools.md`](tools/parallel-tools.md)：并行工具调用 —— 可见批处理 + `⎿ Waiting…`
- [`tools/scratchpad.md`](tools/scratchpad.md)：会话 scratchpad —— 设计
- [`tools/skills.md`](tools/skills.md)：`Skill` 工具
- [`tools/task-tools.md`](tools/task-tools.md)：task 工具 —— 用实时清单取代工具单元格
- [`tools/tools.md`](tools/tools.md)：LLM 工具调用 —— `bash` 及其伙伴 / `read` / `write` / `edit`
- [`tools/web-search.md`](tools/web-search.md)：`web_search` 与 `web_fetch` 工具 —— 来源、fallback、cooldown、地址闸
- [`tools/tty-detach.md`](tools/tty-detach.md)：把子进程从控制终端上摘下

## 代理

子代理定义、运行与进度，以及 agent 会话视图。

- [`agents/agent-context-gauge.md`](agents/agent-context-gauge.md)：agent 会话视图给所看代理的上下文安上仪表
- [`agents/agent-progress.md`](agents/agent-progress.md)：子代理的进度更新 —— 在它自己的行上，也在 `agentoutput` 里
- [`agents/agent-tool.md`](agents/agent-tool.md)：`Agent` 工具 —— Claude Code 式子代理
- [`agents/agent-tools.md`](agents/agent-tools.md)：agent 工具 —— 每个代理都有一个 id，主代理可以操纵它
- [`agents/agent-view-streaming.md`](agents/agent-view-streaming.md)：agent 会话视图像主视图一样流式输出
- [`agents/subagents.md`](agents/subagents.md)：子代理定义 —— `agents/*.md`

## Providers

登录、线上格式、模型目录、prompt 缓存、推理强度与订阅用量。

- [`providers/antigravity.md`](providers/antigravity.md)：Google Antigravity 作为 provider
- [`providers/callback-page.md`](providers/callback-page.md)：浏览器登录回调页
- [`providers/chatgpt.md`](providers/chatgpt.md)：ChatGPT Codex 作为 provider
- [`providers/claude.md`](providers/claude.md)：Anthropic 作为 provider
- [`providers/cline.md`](providers/cline.md)：Cline 作为 provider
- [`providers/copilot.md`](providers/copilot.md)：登录：订阅、API key 与 GitHub Copilot
- [`providers/fast-mode.md`](providers/fast-mode.md)：`/fast`、`/ultrafast`、…… —— codex 的速度档位做成命令
- [`providers/llm.md`](providers/llm.md)：真实的 LLM 后端与 `/model` 选择器
- [`providers/local-models.md`](providers/local-models.md)：本地模型 —— `/login` → **Sign in with a local model**
- [`providers/offline.md`](providers/offline.md)：等待一个断掉的连接
- [`providers/ollama.md`](providers/ollama.md)：Ollama 作为 provider
- [`providers/prompt-caching.md`](providers/prompt-caching.md)：Prompt 缓存与真实用量核算
- [`providers/reasoning.md`](providers/reasoning.md)：Ctrl+T 思考模式（reasoning effort）
- [`providers/usage.md`](providers/usage.md)：`/usage` —— 你的订阅还剩多少
- [`providers/user-providers.md`](providers/user-providers.md)：自己的 provider —— `~/.zeroth/providers.toml`
- [`providers/venice.md`](providers/venice.md)：Venice 作为 provider

## State

什么会持久化、什么是受信的：权限、设置、checkpoint、rollout、会话、凭据与项目配置。

- [`state/checkpoint.md`](state/checkpoint.md)：Checkpoint —— 重置的是代码，不只是 transcript
- [`state/config-review.md`](state/config-review.md)：hooks 与项目 MCP 服务器运行前的审查
- [`state/init.md`](state/init.md)：`/init` —— 生成 AGENTS.md 贡献指南（codex 的 `/init`）
- [`state/per-directory-state.md`](state/per-directory-state.md)：按目录记的状态 —— `/model`、`/settings`、`/mascot`、`/spinner` 记住目录
- [`state/permissions.md`](state/permissions.md)：工具权限请求
- [`state/project-config.md`](state/project-config.md)：项目级 `.zeroth` 配置 —— hooks、MCP 服务器、skills、agents
- [`state/project-doc.md`](state/project-doc.md)：上下文窗口里的 AGENTS.md —— codex 的项目文档（设计）
- [`state/resume.md`](state/resume.md)：`/resume` —— 接上一个已保存的会话（rollout 文件 + 会话选择器）
- [`state/rollout-records.md`](state/rollout-records.md)：rollout 对每次调用与请求记录了些什么
- [`state/secrets.md`](state/secrets.md)：`/secrets` —— agent 使用但从来看不到的凭据
- [`state/session-model.md`](state/session-model.md)：按会话记的模型 —— 一段对话记得它跑在哪个模型上
- [`state/settings.md`](state/settings.md)：`/settings` —— 会话设置菜单

## CLI

TUI 之前的命令行界面：参数、`[PROMPT]` 快捷形式、headless 模式与 `mcp` 子命令族。

- [`cli/cli.md`](cli/cli.md)：CLI —— `--help`、`[PROMPT]` 快捷形式、`--continue`、`--resume [id]` 与退出提示
- [`cli/headless.md`](cli/headless.md)：Headless 模式 —— `zeroth -p`
- [`cli/batch.md`](cli/batch.md)：批处理模式 —— 一个进程里跑多个 headless 会话
- [`cli/mcp-cli.md`](cli/mcp-cli.md)：CLI 安装 MCP —— `zeroth mcp add/remove/list/get/add-json`

## Operations

把它发出去：发布、更新、冒烟套件、构建时间与二进制体积。

- [`ops/binary-size.md`](ops/binary-size.md)：二进制体积 —— 发布版实际发出的那个二进制
- [`ops/build-time.md`](ops/build-time.md)：构建时间 —— `cargo build --release`
- [`ops/release.md`](ops/release.md)：发布 —— tag、workflow、产物与发布说明
- [`ops/smoke.md`](ops/smoke.md)：冒烟套件 —— `scripts/smoke.sh`
- [`ops/update.md`](ops/update.md)：更新提示 —— 每天一次的版本检查与 `zeroth update`

## Credits

- [`credits.md`](credits.md)：技术致谢
