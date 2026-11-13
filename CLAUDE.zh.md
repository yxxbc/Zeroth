# CLAUDE.md

给本仓库里 Claude Code 的指引。

**[`AGENTS.md`](AGENTS.md) 管你跑什么、什么时候跑** —— 第一次编辑之前先读它。
它把一次改动映射到能失败的最小检查，列出 macOS 上不同的地方，并跑
`scripts/preflight.sh --smoke`。本文件是架构地图：代码怎么运转，以及那些一旦搞错
就会静默失效的不变量。

## 这是什么

Zeroth 是一个内联终端编码 agent：一个 Rust 二进制（edition 2024），在 crossterm
之上有一个自制的内联 viewport、一个 tokio `select!` 循环，以及一个跨五种 wire
format（Chat Completions、Responses、Anthropic Messages、Ollama native、Cloud
Code）的 agentic tool 循环。它以交互方式或 headless（`-p`）运行。设计理由见
`docs/design.md`。模块地图见 `docs/architecture/module-layout.md`。每个领域在
`docs/` 下都有一页。

## 命令

```bash
cargo run                                   # the TUI
cargo test --lib <module>::                 # the loop after an edit
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                  # once, before calling it done
cargo doc --no-deps --lib
cargo build && scripts/smoke.sh <id>        # the terminal boundary
scripts/smoke.sh --list                     # phases, tags, durations
scripts/preflight.sh --smoke                # is this machine fit to run the gate
```

每次改动的测试矩阵、每条命令的代价，以及 macOS 的基线都在
[`AGENTS.md`](AGENTS.md) 里。`target/` 是一个增量缓存（在这台机器上是指向 mbx
存储的一个符号链接）：绝不要 `rm -rf` 它；`cargo clean -p zeroth` 能修好一个陈旧
的链接。

## 架构一览

- **`src/lib.rs`** —— 库，即可测试的核心。纯的、有单元测试的逻辑住在 `app`、
  `stream`、`ui`、`textarea`、`file_search`、`session`、`history`、`context`，
  外加 `frame`/`paste`/`subprocess` 的纯核心。
- **`src/main.rs`** —— 一层薄壳：detached-exec hook、CLI 解析、viewport、循环。
- **`src/tui/`** —— 驱动异步循环的二进制私有树（`event_loop`、`turn`、`stream`、
  `agent`、`background`、`permission`、`ask`、`view`、`commit`、`models`、……），
  `Session` 在 `mod.rs` 里。每个 handler 都是它所属领域模块里的一个
  `impl Session` 块。
- **`src/app/`、`src/ui/`、`src/stream/`** 是按领域划分的模块目录，按名字重新
  导出 —— 从不用 glob —— 所以 `tests/api_surface.rs` 能锁住公开接口。
- **后端接缝**是 `stream::ReplySource`；`llm::LlmBackend` 实现它，并在
  `llm::exec::ToolExecutor` 之上跑 tool 循环（`llm::agent::run_agent`）。离线
  demo 是 `stream::dummy`（`DummyAi`），一个场景注册表。
- **Headless** 的 `-p` 在 `InlineViewport::headless` 之上复用同一个 `Session`
  （`docs/cli/headless.md`）。
- **边界与纯：** `src/tui/`、`term.rs`、剪贴板读取、session 记录器和输入历史
  存储都是 I/O；smoke 覆盖它们，而不是单元测试。把逻辑挡在边界之外，放进它所调用
  的纯 helper 里。

## 四条不变量

破一条，一整类 bug 就都回来了。下面点名的测试和 smoke 阶段就是报警的金丝雀。

1. **一个 stdin 读取者，且在 init 的光标查询之后创建。** `InlineViewport::init`
   在 crossterm 的 `EventStream` 存在之前，同步地做那一次 DSR 读取。任何其他对
   终端的问题都搭同一次读取（`term::query`），而在它之前敲的键会被重放
   （`take_typeahead`）。永远不要加第二个读取者。detached-exec hook
   （`subprocess::run_detached_exec_if_requested`）必须一直是 `main()` 的**第一
   条语句**，在 runtime 或任何终端 I/O 之前。
2. **贪心按词换行是前缀稳定的。** 文本被追加时，`ui::wrap_text` 只改动最后一行
   换行后的内容。`ui::StreamRender` 靠这一点，在回复增长时把已完成的行提交进
   scrollback，扣住正在增长的最后一行、尾随空白，以及整条未闭合的代码行。
   没有那三个锁定测试（`incremental_commits_reconstruct_the_whole_reply`、
   `stream_render_is_prefix_stable_over_every_prefix`、
   `streamed_code_never_recolours_a_committed_row`），不要改换行算法，也不要改
   markdown/高亮渲染器。
3. **viewport 以内容为锚，resize 双向重排。** 那个框在原地向下生长；`term::draw`
   只在必要时滚动，而 `ui::repin` 决定收缩。绝不要把它钉在
   `screen.height - height` 上。一个内联*模态*（权限提示）可以占满屏幕高：它长进
   真实的 scrollback，而它关闭后的第一次绘制会做 purge-rebuild
   （`modal_scrolled`、`ui::modal_needs_rebuild`；smoke 的
   Phase 58/60/62/63）。一条流式回复结束时，在最终提交*之前*把 viewport
   复位到空闲高度，否则框会上移。每一次完整重建
   （resize、`/clear`、rewind、`/resume`）都走 `term::reflow` —— 清掉 scrollback
   和屏幕、重写历史、在一个同步帧里画出这块区域。一次普通的 overlay 返回**不是**
   重建：overlay 开着时提交只是排队，返回时才把它们刷出去。
   **这条不变量描述的是内联默认。** 那个可选启用、拥有屏幕的模式（`--fullscreen`
   / `ZEROTH_TUI=full`，`docs/ui/full-screen.md`）从历史画出整帧到备用屏；它的
   `reflow` 和 `insert_before` 走全屏分支，要守住的是它自己的滚动契约。
   不要把两者混在一起。
4. **tool call 与文本交错。** 一个 `ToolStart` 把它之前的 assistant 文本收尾成一
   条独立的历史消息。一个并行的批次会事先宣告（`ToolBatch` →
   `App::tool_queue`）；运行中的调用实时显示，而它的兄弟显示 `⎿ Waiting…`，
   执行仍然是顺序的。Ctrl+O 是完整 transcript，由 `ui::TranscriptCache` 增量
   搭成；它开着时循环仍在提交 —— 提交只是*排队*（`insert_before` 不做 I/O，
   只有内联的 draw/reflow/restore 才刷出去）。从 overlay 退出程序必须先
   `exit_overlay`，再 `overlay_return_repaint`，然后才能 break。一个打开的
   agent session 视图是唯一会推迟提交的视图（`commits_allowed`）。

## 数据流

`select!` 跑在五个来源上：输入、回复事件、绘制 tick、`@` 搜索的结果，以及完成
的 Ctrl+V 剪贴板读取。每一次状态变化都排一帧；调度器把普通请求合并到 120 fps
（`MIN_FRAME_INTERVAL`），但一个编辑 composer 的键会拿到自己 2 ms 的紧急下限
（`URGENT_FRAME_INTERVAL`，在 paste-burst 检查之后），这样打字不会排在一个动画
tick 后面；而 turn 活跃时，绘制分支会重新武装一个 32 ms 的动画帧。

`Submit(text)` → `tui::turn::Session::start_turn`：记录、提交、spawn 一个
`ReplySource`。**turn 中按回车是给它掌舵**：后端在每个 round 边界排空共享的
`SteerQueue`，而 `StreamEvent::Steered` 把这条消息落成一个真正的用户气泡。
**Tab 会排队**一个独立的后续 turn。`!command` 作为它自己的 shell turn 在本地运行
（有 turn 在跑时就排队）。

后端交错发出 `Chunk`、`ThinkingStart`/`ThinkingChunk`/`ThinkingEnd`、
`ToolCallDelta`、`ToolStart`/`ToolOutput`/`ToolEnd`、`Usage`，然后是
`StreamDone` 或 `Error`。turn 中按 Esc 会取消，并且要么保留部分内容（把每一个
在途调用都落定为失败，带红色 notice 和 receipt），要么在什么都没有流式输出过时撤
销这次提交。每个 turn 结束都会派发**一个**排队条目。turn 中的 `/clear` 是一次
击杀：取消、收割、排空、擦掉、重绘。

## 约定

- **样式集中**在 `ui/theme.rs`：字形和几何是 const，颜色是读取
  `ui/palette.rs` 里当前主题 `Palette` 的访问函数。`palette.rs` 之外不许有
  `Color::Rgb`；一个新颜色就是每个主题表里的一个新 palette 角色
  （`docs/ui/theme.md`）。
- **所有宽度运算都走 `cols()`**，绝不用 `chars().count()`。每一次把 cell 交给
  后端的绘制都走 `term::visible_cells` —— 三条路径，漏掉一条就会在那一个视图里
  复活宽字形 bug（`docs/ui/table-streaming.md`）。
- **输入是一个 `textarea::TextArea`**，绝不是 `String`。把编辑路径都导经它；用
  `ui::layout::text_field_width` 来定字段尺寸，好让 caret 始终占着一个 cell
  （`docs/input/textarea.md`）。
- **绝不要为一个只从里面读几个字段的 body 建一棵 `Value` 树**，也绝不要每 turn
  分配任何图片大小的东西。给网络读取设界；把一张 PNG 按它被适配后的尺寸解码
  （`images::fitted`）；流式发送请求体（`llm::body::streamed_request`，
  `docs/architecture/memory.md`）。
- **图片在 `ui` 里预留、在边界上绘制**：行构建器调用 `images::place` 并发出带
  标记的空行；`ImageStore::stamp` 在全部四条绘制路径里运行
  （`docs/ui/images.md`）。
- **一个带框的内联视图紧贴它的分隔线**，而在矮终端上，它在流入之前先减行
  （`ui::view_flow`，`docs/ui/view-flow.md`）。
- **一个生命周期 hook 事件就是 `llm::hooks::HookSink` 上一个带默认实现的方法，
  加一个调用点。** 任何 hook 会阻塞在上面的东西，都必须轮询该 turn 的
  `CancelToken`（`docs/tools/hooks.md`）。
- **tool schema 要短，要具体。** 路径参数说「the absolute path to ……」然后就
  停；每一个词都会随每一次请求旅行。
- **agent 自己的名字是一个常量**：`zeroth::APP_NAME`。移植来的句子保留它们源
  tool 的名字，除非你有意改掉。
- **README 是前门，不是手册。** 机制写进 `docs/`，改动写进 `CHANGELOG.md`，隐私
  声明写进 `TELEMETRY.md`。
- **工作方式：TDD。** 没看着一个测试先失败，就不写产品代码。`src/main.rs` +
  `src/tui/` 是例外 —— 用 app 和 smoke 来验证。对非平凡的功能：探索、发问、提出
  2–3 个设计、达成一致、写进 `docs/`，行为变化时更新 `docs/design.md`。

## 细节都住在哪里

`docs/` 是分了类的。先看 `docs/design.md` 和
`docs/architecture/module-layout.md`，然后是那个领域的那一页：

- `docs/architecture/` —— 异步循环、compact、上下文/用量计数、内存、token 估算、
  dummy 后端、herdr，外加实测的输入延迟与并发笔记。
- `docs/input/` —— textarea、历史/搜索/持久化、队列、粘贴、图片粘贴、文件搜索、
  shell 命令、backtrack、打断。
- `docs/ui/` —— 主题、布局、图片、tool cell/流式/分组、视图流、各个 picker、
  overlay、页脚/横幅/状态。
- `docs/tools/` —— tool、bash/交互式 shell、后台、hook、MCP、ask、task、skill、
  scratchpad、tty detach。
- `docs/providers/` —— 每一种 wire、登录、缓存、离线、用量、推理、fast 模式、
  本地模型。
- `docs/state/` —— resume、rollout、checkpoint、权限、设置、按目录状态、
  信任/配置审查。
- `docs/ops/` —— 发布、smoke、更新、二进制体积、构建时间。
- `docs/agents/`、`docs/cli/` —— subagent/agent tool，以及 CLI/headless/MCP。
