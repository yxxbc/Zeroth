# 并发 —— 回合在哪里阻塞，以及实际同时跑的是什么

日期：2026-11-12

状态：**测量与盘点**，不是提案。“什么可以变”的那些小节是附带代价的选项，供在
任何代码动起来之前做决定。写完这页之后其中两条已经被采纳：回合末的 checkpoint 现
在在自己的线程上快照（*选项 B*），urgent 帧路径也已落地
（`input-latency.md`）。剩下的是什么、每一笔的实测代价是多少，
见下面的 *渲染循环和输入共用一个线程*。

两条轴上的原型结果 —— 一个进程里的 N 个 session，以及一个并行的 tool 批次 ——
在 [`multithread-core.md`](multithread-core.md)。

## 抱怨是怎么来的

“竞品的基准测试有 15 个 session 同时干活、后台一个 16 线程的编译，而且仍然保持
响应。zeroth 只有一个 session。”那是可观测的那一半。值得写下来的那一半是
**这套设计故意串行化了什么、又没有串行化什么**，因为其中几个串行化点是承重的
（取消、权限、transcript 不变量），而只有一个是可以随便改的。

## 今天同时跑着的东西

| 工作 | 跑在 | 阻塞 TUI 吗？ |
| --- | --- | --- |
| 回合本身（`run_agent` 的轮循环） | 每回合一个 `std::thread::spawn`（`backend.rs::spawn`） | 不 |
| 每个请求的网络 I/O | 每请求一个**第二个**脱离线程（`openai.rs::run_transport`，`:548` 处的 `thread::spawn`） | 不 |
| `agent` tool 调用（subagent） | 各一个线程，从 `run_agent_calls` 启动 | **调用方的回合线程 —— 是**（见下） |
| 一个 `!command` | 一个线程（`tui/shell.rs::spawn_shell_command`）+ 2 个管道排空读取者 | 不 |
| 启动时的 MCP 连接 | worker 线程，并发 | 不 |
| 一次 MCP `call_tool` | 该调用的死线之下的 worker 线程 | 不 |
| `@` 文件搜索、Ctrl+V 剪贴板、`/model` 抓取、`/login`、`/usage` | 每个任务一个 worker 线程（`tui/workers.rs`） | 不 |
| 后台 shell、后台 agent | 自己的线程，报在自己的 channel 上 | 不 |
| checkpoint 快照 | 它自己的线程（`tui/turn.rs::start_checkpoint_snapshot`） | 不 |
| checkpoint **restore**（回溯、`/resume`） | 循环自己的线程 | **是**（见下） |
| session 文件追加（`recorder.sync`） | 循环自己的线程（事件循环底部） | **是**，但只追加、无 fsync |
| Ctrl+O transcript warm（`transcript.warm`） | 循环自己的线程（事件循环底部） | **是**，每回合一个新项，`/resume` 后是整段重载 |
| 帧调度、渲染、全部 `App` 状态 | 循环的线程（`current_thread` tokio runtime） | — |

所以形状是：**按设计一次一个回合，但一个回合不止一个线程。** 单个回合已经扇出
到一个传输线程、N 个 subagent 线程、N 个 tool 线程。它从不做的事是同时跑**两个
回合**，或者两个普通 tool call。

## 那些串行化点，以及每一个为什么存在

### 1. 一轮里的普通 tool call 是顺序的 —— 出于选择

`llm/agent.rs:725` 是 `for call in &rest { … execute(call, …) }`。批次是提前
*宣告*的（`StreamEvent::ToolBatch`），所以 UI 用 `⎿ Waiting…` 显示每个 call，但
执行是一次一个。`docs/tools/tools.md` 与 `docs/tools/parallel-tools.md` 都这么
说，理由在后一篇里：现有的单子进程超时、`kill_process_group` 和取消机制假定只有
**一个**子进程；交错的 stdout 需要多子进程收割；而渐进提交管线假定 call 按顺序
解析。

代价：一轮请求三条各 20 秒的命令要 60 秒。这是最显眼的“并发”缺口，也是补起来
最贵的（见*选项 A*）。

### 2. 一个 subagent 批次会阻塞它的父回合

`run_agent_calls`（`backend.rs:1586`）把每个前台 agent 作为线程启动，然后**在
循环里轮询**，`std::thread::sleep(AGENT_WAIT_POLL)` —— 30 毫秒 —— 直到它们全部
完成（`backend.rs:1700-1709`）。这个轮询线程就是回合的线程，所以一个前台
subagent 组运行时，lead 什么别的都做不了。这是有意的
（`docs/agents/agent-tool.md`：lead 的结果*就是*各 agent 的结果），但那个
30 毫秒的 sleep 也意味着父进程自己的取消和 Ctrl+B 交接是轮询出来的，不是等
出来的。

代价不在用户的延迟 —— agent 并行跑着，名单在跳 —— 而在 lead 的对话在这段时间里
是**空闲**的。一个只启动了一个 agent、本来可以顺便读个文件的模型做不到。

### 3. 一次一个回合，其余一切排队

`App::turn_active` 把住 `start_turn`；Enter 给运行中的回合 steer，Tab 把后续消息
排进队列（`docs/input/queue.md`）。这是一个产品决定，也正是那个基准真正指向的
东西：15 个 *session*，不是一个 session 里的 15 个回合。架构里没有任何东西禁止
一个进程里跑 N 个对话 —— `agents` 已经在用各自的 channel、渲染和队列维持 N 份
transcript。拦路的是 `App`：一个 `history`、一个 `streaming`、一个 `queued`、
一个 composer。

### 4. checkpoint 快照离开了循环；它的 restore 没有

这正是*选项 B* 当初为之而写的那次冻结，它已经修好了：
`tui/turn.rs::start_checkpoint_snapshot` 把快照 spawn 出去，循环继续绘制，结果
从 `checkpoint_rx` 取（`tui/event_loop.rs` 第 13 个分支）。在一棵 两万文件
的树上实测，快照本身 0.17 s —— 不再是谁的帧。

它的**镜子**没有跟着搬。`restore_to`（`tui/resume.rs`）内联地跑 `snapshot` +
`git reset --hard` + `git clean -fdq`，所以一次回溯或一次 `/resume` 仍然会冻住
循环：在一棵脏的两万文件树上实测 1.5 s，而慢盘上的 `git clean` 没有任何上界。
迟到快照本来就需要的那道 `checkpoint_epoch` 守卫，原样适用于迟到的 restore，
这正是把它搬走算是剩余工作里便宜那一半的原因
（*渲染循环和输入共用一个线程* 第 3 条）。

### 5. recorder 在循环的线程上追加，没有 fsync

`recorder.sync` 跑在循环底部（`bootstrap.rs:1141`）与关闭时。它是只追加、带水位
标记的，并且当历史没有变动时就跳过 —— 于是一个流式回合分文不付。但一个记录了历史
条目的回合会在循环的线程上对该条目做 `write_all`。没有 `sync_all`，所以这是一次
页缓存写入：通常几微秒，但一个写满的页缓存或一块慢盘会把它变成没人设过界的停顿。

### 6. 一个 HTTP 连接池，一个回合一次请求

`llm/mod.rs::cached_client` 构建一个 `reqwest::blocking::Client`（按
`op_timeout` 缓存），带 `pool_idle_timeout(30 s)`。一个回合一次只发一个请求，
所以
这个池恰好服务一个连接。没什么不对；它只是意味着要做并发就得把
`pool_max_idle_per_host` 从默认值抬起来（其实默认是无上限，所以没问题），而
**阻塞式**客户端的池并不是限制 —— 限制是那个一次一请求的回合循环。

## 这里的“并发”可能意味着什么，以及价钱

### 选项 A —— 让一个普通 tool 批次并发执行

触及：`llm/agent.rs` 的循环、`llm/exec.rs`（单子进程假设）、`llm/backend.rs`
（执行器的 `&mut dyn FnMut` sink 变成按 call）、`app/tools.rs`（按 call 的输出
缓冲）、`ui/tool.rs`（多个 live 单元格本来就能渲染 —— `ToolBatch` 做了那一半）。

决定它的风险：

- **取消。** `kill_process_group` 现在杀的是*那个*子进程。一个批次意味着 N 个
  子进程，各有自己的进程组，全都要同样的保证。`docs/tools/parallel-tools.md`
  正是把这一点列为没做的原因。
- **输出顺序。** `ToolOutput` 的 chunk 现在到达一个流、追加到队首 call 上。
  并发的
  call 需要在事件上按 call 打标，这会改变 `StreamEvent::ToolOutput` 的形状以及
  每一个消费者。
- **权限。** 两个 call 同时询问，是权限系统从不需要建模的一种队列（它显式地一次
  一个模态框 —— `app/permission.rs`、`docs/state/permissions.md`）。
- **transcript 不变量 4。** 渐进提交假定队首的 call 先提交。

价钱：高。收益：一个 3×20 秒的批次变成 20 秒。值得，但它是独立的一个项目，也是
最有可能弄坏取消的那个改动。

### 选项 B —— 把 checkpoint 快照移出循环（已完成）

触及：`checkpoint.rs`（让 `snapshot` 可 spawn）、`tui/turn.rs`（发出即忘，结果
到达时记下 SHA）、`recorder`（接受在回合结束之后才落地的 checkpoint）、
`docs/state/checkpoint.md`。

风险：checkpoint 是按历史长度记录的，而一个迟到的 checkpoint 可能落在一次回溯
之后 —— recorder 用 `after-items` 给 checkpoint 作键，所以迟到的结果在被接受之前
需要一次代次检查。`git add -A` 和用户在工作树里自己跑的 `git` 会打起来（本来就
可能，但目前的窗口是“回合末”，而不是“git 什么时候跑完”）。

价钱：低 —— 一个调用点加一个 channel。收益：在冷存储上，在每台机器上，去掉一次
每回合最长数秒的冻结。**这是这个文件里最便宜的一处并发修复。**

### 选项 C —— 让 lead 在前台 agent 运行时继续干活

触及：`run_agent_calls` 的轮询循环变成 channel 等待；lead 的循环需要一个“结果
未到”的状态，好让一轮的 tool 结果可以迟到。

价钱：中高，而且它与“tool 结果保持连续”那条 prompt 缓存规则冲突
（`docs/providers/prompt-caching.md`）。大概不值得：lead *要的*就是各 agent 的
答复，所以空闲在语义上是对的。

### 选项 D —— 一个进程里的 N 个 session

这正是那个基准真正测的东西，也正是 `agents` 已经证明可行的东西。触及：每一处
假定只有一个 `App` 的地方 —— `history`、`streaming`、`queued`、`composer`、
`recorder`、`checkpoints`、`session` 文件，再加一个 session 切换器。视图机制
（按 view 的流动、按 session 的 transcript 缓存）是容易的那一半；`App` 的拆分是
剩下的全部。

价钱：非常高。收益：那个能让“15 个 session 的对比”真的成其为对比的东西。注意
路线图本来就把 session 当作独立的**进程**（`docs/state/resume.md` 是每个 session
一个文件的存储），所以这是一次重新设计，不是一次重构。

## 最值得先做的那一件事

**选项 B —— 已完成。** 它确实小、确实去掉了一次被量出来的冻结、确实不需要新的
并发模型，而且对其他每一条不变量都是透明的。同样的论证现在指向下面
*渲染循环和输入共用一个线程* 末尾的三条：每条都是一个调用点，每条去掉一次实测出来
的阻塞，而不是改变一个回合的结构。

## 选之前该量什么

上面没有一条值得凭直觉决定。下面第一条已经被采纳 —— `ZEROTH_TIMING=<path>`
现在会写它（`tui/timing.rs`），也正是它给这次 checkpoint 迁移定了价。仍然缺的：

- **每个回合末花在 `recorder.sync` 上的时间** —— checkpoint 那一半已经量到了
  （`checkpoint.snapshot`、`checkpoint.dispatch-wait`）；recorder 自己的
  `write_all` 还没有一行。
- **一个 3-call 批次和一个 4-agent 批次下的线程数与堆叠的 `ps -M`** —— 好知道
  “每请求一个传输线程 + 每 agent 一个线程 + 每 tool 一个线程”的形状是否有一个
  值得抬高的天花板。
- **每轮一行墙钟**：请求发出 → 首个 token → 最后一个 token → tool 全部完成。基准
  自己的数字是覆盖整个场景的中位数；按轮的数字才是修复会推动的东西。

帧耗时日志（`docs/architecture/input-latency.md`，第 5 项）能同时服务两份文档；
同一个环境变量门控的写入者也能给一个回合末计时。下面的数字来自临时的 `examples/`
探针，而这正是那个日志存在的理由。

## 渲染循环和输入共用一个线程

2026-11-17 补写。这一节回答的是：“UI 单线程渲染，而这个线程还管输入 —— 这不危险
吗？”值得把那句话拆成两半，因为它们的答案不同：真正会咬人的风险，不是这个形状
暗示的那些。

### 成立的部分

渲染和输入处理确实跑在同一个线程 —— `main.rs` 里的 `current_thread` tokio
runtime。`draw_rx` 和 `events` 是同一个 `select!`（`tui/event_loop.rs`）的两个
分支，所以一帧和一次按键按构造就永远不会**并发**。这不是疏忽：正是它让帧的决策
简单。一个线程独占 `App`、viewport 的 diff（`prev`）、光标座位和 scrollback
队列，所以它们都不需要锁，一帧也永远看不到半应用的状态变更。

“这很危险”这个前提，通常夹带了两个在这里不成立的假设：

- **跑一帧的时候输入不会丢。** crossterm 的 `EventStream` 有自己的读取线程
  （`crossterm-0.29.0/src/event/stream.rs:44`），所以即使循环正忙，按键也会被从
  pty 读走；而 pty 自己会缓冲还没被读的部分。两个方向都实测过：没人读的时候，
  写进 1024 字节，读回 1024 字节 —— 一模一样 —— 再往后写入方只是阻塞，而不是被
  丢弃。所以慢帧付的是**延迟**，不是按键。
- **状态不共享。** 没有任何东西基于别的线程正在写的快照渲染，所以不存在要考虑
  的撕裂帧。比任何锁都便宜。

所以问题不是“这安不安全”—— 而是“这个线程的一次迭代能阻塞多久，用户在这期间
在干什么”。本节剩下的就是那个数字。

### 一次迭代要花多少（实测）

release 构建、120 列帧、macOS，跑在本机 `~/.zeroth/sessions` 里自己的
transcript 上（611–1309 项，每份 3.3–4.1 MB）—— 渲染器的最坏情况，而它是真实的。

| 工作 | 在循环的线程上 | 实测 |
| --- | --- | --- |
| 一次流式帧，1 KB 回复 | 是 | 0.05 ms |
| 一次流式帧，64 KB 回复 | 是 | 0.10 ms p50 |
| 一次流式帧，1 MB 回复 | 是 | 1.2 ms p50，2.3 ms p95 |
| 空闲帧，1066 项 history | 是 | **0.02 ms** |
| 会话重建（resize） | 是 | 24–36 ms |
| Ctrl+O transcript，新落地一个 tool 结果 | 是 | **1.4 ms** |
| Ctrl+O transcript，`/resume` 换进来 | 是 | **1.07 s → 0.11 s**（高亮设上限之后） |
| 回合末的 checkpoint 快照 | 不（自己的线程） | 两万文件的树上增量 0.17 s、冷跑 0.93 s |
| backtrack/resume 的 restore（`git reset --hard` + `git clean -fdq`） | 不（自己的线程） | 两万文件脏树上 1.5 s |

稳态不是问题，而这正是当初选这个形状的原因：流式帧便宜，因为 `StreamRender`
把已完成的行一次性提交进 scrollback，而 strip 只预览一行 —— 一帧的代价跟着**最后
一行**走，不跟回复走，所以 1 KB 和 64 KB 分不出来，1066 项的 history 一点都不花，
因为它根本没被重渲染。`render_live` 重渲染整条回复（1 MB 时 36 ms）只是单元测试的
兜底；循环永远传 `Some(preview)`。

下面两处冻结都在 2026-11-17 关掉了；上面的数字是修好之后的，
*该做什么，按顺序* 记下了每一处当初的代价。

1. **`/resume` 之后的 `transcript.warm` —— 1.07 s → 0.11 s。** Ctrl+O 的缓存冻结
   已渲染的前缀、每回合追加一项（1.4 ms —— 设计是对的），但 `/resume` 会 bump
   `history_generation`，冻结的 key 对不上，于是整段载入的会话在一次调用里重渲染。
   那一整秒全都是语法高亮：这个 session 的八个 markdown 文件单元格合计 986 ms 的
   上色，最贵的单个 526 ms。按字节设界（`CODE_HIGHLIGHT_MAX_BYTES`）就是修法 ——
   完整测量、以及为什么按时间的闸门不管用，见 `docs/ui/long-lines.md` §7b。warm
   本身必然仍是全量历史：只缓存一部分会让 Ctrl+O 打开时缺项，而 overlay 本来也无法
   在 warm 期间进入。
2. **checkpoint restore —— 移出了循环。** 它的镜子 `restore_to` 内联跑 `snapshot` +
   `git reset --hard` + `git clean -fdq`：脏的两万文件树上冻 1.5 s，spinner 和 Esc
   一起。现在它像 `start_checkpoint_snapshot` 一样 spawn，结果报在 `restore_rx`
   （`tui/event_loop.rs` 第 14 个分支），由 `on_restore_done` 弹 toast。token 校验
   和 `checkpoint_epoch` 是同一个形状：晚于更新一次请求才落地的 restore 会被丢弃。
   到那时调用方早已回退了历史，所以下游没有任何东西在等工作树先动。

其他**本来可能**成为冻结的东西，都已经在线程外：回复后端、传输、`@` 搜索、剪贴板、
模型列表、`/login`、`/usage`、MCP、link card、git diff，以及 checkpoint 快照
（`tui/workers.rs`、`tui/diff.rs`、`tui/turn.rs`）。

### 为什么加一个渲染线程不是解法

显而易见的动作 —— 第二个线程持有 viewport、帧走 channel —— 在 1.07 s 的冻结里只
买到 1.2 ms，代价是放弃上面两条性质：`App` 必须 `Send`（它按设计持有 `RefCell`
渲染缓存，`app/drafts.rs`），而且一帧从此要和请求它的那次状态变更赛跑。这些冻结
并不是那种意义上的**渲染**代价 —— 它们是一次 `O(history)` 的渲染加一次 `git`
调用，都可以在源头避开。帧耗时日志（[`input-latency.md`](input-latency.md) 第 5
条）是把这件事保持诚实的便宜办法。

### 该做什么，按顺序

三条都已完成（2026-11-17），而且都没有引入并发模型 —— 每条都是一个调用点，这才是
对那句抱怨的真正回答：线程形状没问题，剩下的是几笔测出来的一次性代价，而单线程
让它们容易找。

1. **给高亮器设界 —— 已完成，`CODE_HIGHLIGHT_MAX_BYTES`（2 KB）。** 并不是这份
   文档当初猜的「一个 `if`」：昂贵的行没法从文本预测，所以上限约束的是每个单元格的
   **总**工作量，而常数是扫出来的（`docs/ui/long-lines.md` §7b）。986 ms 的上色 →
   40 ms。
2. **把 `/resume` 的 warm 挪到绘制之后 —— 放弃，附理由。** 它没法有效地分片：缓存
   必须持有完整历史，否则 Ctrl+O 打开时会缺项，而 warm 在飞时 overlay 也进不去。
   第 1 条已经把它本想遮住的那笔代价去掉了。
3. **把 restore 挪到 worker 上 —— 已完成。** `restore_to` spawn，结果落在
   `restore_rx` 上，token 守卫替代了原来的内联顺序。

## 渲染循环与输入同线程 —— 怎么复现

探针是临时的 `examples/` 文件，已删除；上面的数字就是记录。照同样的方式重建：对着
真实 rollout 跑 `cargo run --release --example …`
（`~/.zeroth/sessions/YYYY/MM/DD/rollout-*.jsonl`，用 `session::parse_session`
解析成 items，用 `App::load_session` 载入）。帧的数字要用循环自己的形状 ——
`StreamRender::preview` 先一次，然后 `render_live_with_preview` —— 而不是
`render_live`，后者是单元测试的兜底。

## 复现这份盘点

- **顺序批次**：`llm/agent.rs:725`（`for call in &rest`）。
- **subagent 轮询**：`llm/backend.rs:1586`（`run_agent_calls`）以及 `:1708` 处
  的 `AGENT_WAIT_POLL` sleep（30 毫秒，`:1484`）。
- **一次一个回合**：`App::turn_active`；队列语义见 `docs/input/queue.md`。
- **移出循环的 checkpoint**：`tui/turn.rs::start_checkpoint_snapshot` →
  在 spawn 出来的线程上跑 `checkpoint.rs:897`，结果走 `checkpoint_rx`
  （`tui/event_loop.rs` 第 13 个分支）。默认开在 `checkpoint.rs:536`。它的镜子
  `restore` 仍然内联执行（`tui/resume.rs::restore_to`）。
- **循环上的 recorder**：`tui/bootstrap.rs:1141` → `recorder.rs:279`。
- **transcript warm**：`tui/bootstrap.rs:1295`（`self.transcript.warm`），
  现在由 `ui::file_cell::HighlightBudget` 设界；
  `ui/transcript.rs::ensure_frozen`；单项的单元格是
  `ui/tool.rs::tool_full_lines` → `ui/file_cell.rs::numbered_body_lines`，高亮器是
  `highlight::Highlighter::line`。
- **输入的活性**：`crossterm` 的 `EventStream` 读取线程，
  `crossterm-0.29.0/src/event/stream.rs:44`；帧地板见
  `docs/architecture/input-latency.md`。
- **每请求一个传输线程**：`llm/openai.rs:548`。
