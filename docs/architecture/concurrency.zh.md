# 并发 —— 回合在哪里阻塞，以及实际同时跑的是什么

日期：2026-11-12

状态：**测量与盘点**，不是提案。这里没有任何东西被实现；“什么可以变”的那些
小节是附带代价的选项，供在任何代码动起来之前做决定。

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
| checkpoint 快照 | 循环自己的线程 | **是**（见下） |
| session 文件追加（`recorder.sync`） | 循环自己的线程（事件循环底部） | **是**，但只追加、无 fsync |
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

### 4. checkpoint 快照跑在事件循环上

`tui/turn.rs:402::checkpoint_turn_end` → `CheckpointStore::snapshot`
（`checkpoint.rs:821`）在**循环的线程上同步跑三个 `git` 子进程**，每个回合末
都跑：
`add -A`、`commit`、`rev-parse HEAD`。默认开启（未设 `ZEROTH_CHECKPOINTS` 即
开）。`docs/state/checkpoint.md` 记下了代价：*“在普通硬件上一次冷快照约一秒，在
最慢的机器上要几秒”*。循环所拥有的一切 —— 输入、帧、spinner、Esc —— 就冻那么久，
**每回合一次**。

三分预算（`20 000` 个文件 / `256 MiB` / `500 ms` 探测）约束的是*探测*，不是
快照。
`git add -A` 本身没有超时。

### 5. recorder 在循环的线程上追加，没有 fsync

`recorder.sync` 跑在循环底部（`bootstrap.rs:1134`）与关闭时。它是只追加、带水位
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

### 选项 B —— 把 checkpoint 快照移出循环

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

**选项 B。** 它小、它去掉一次被量出来的冻结、它不需要新的并发模型，而且它对其他
每一条不变量都是透明的。这一页上别的东西都改变一个回合的结构；只有它只改变一个
子进程*什么时候*运行。

## 选之前该量什么

上面没有一条值得凭直觉决定。这份代码库缺的是看见它的仪表：

- **每个回合末花在 `checkpoint_turn_end` 与 `recorder.sync` 上的时间** —— 各一行
  日志、由环境变量门控，就能定下选项 B 是否值那 ~50 行。
- **一个 3-call 批次和一个 4-agent 批次下的线程数与堆叠的 `ps -M`** —— 好知道
  “每请求一个传输线程 + 每 agent 一个线程 + 每 tool 一个线程”的形状是否有一个
  值得抬高的天花板。
- **每轮一行墙钟**：请求发出 → 首个 token → 最后一个 token → tool 全部完成。基准
  自己的数字是覆盖整个场景的中位数；按轮的数字才是修复会推动的东西。

帧耗时日志（`docs/architecture/input-latency.md`，第 5 项）能同时服务两份文档；
同一个环境变量门控的写入者也能给一个回合末计时。

## 复现这份盘点

- **顺序批次**：`llm/agent.rs:725`（`for call in &rest`）。
- **subagent 轮询**：`llm/backend.rs:1586`（`run_agent_calls`）以及 `:1708` 处
  的 `AGENT_WAIT_POLL` sleep（30 毫秒，`:1484`）。
- **一次一个回合**：`App::turn_active`；队列语义见 `docs/input/queue.md`。
- **循环上的 checkpoint**：`tui/turn.rs:345` → `:402` → `checkpoint.rs:821`；
  默认开在 `checkpoint.rs:536`。
- **循环上的 recorder**：`tui/bootstrap.rs:1134` → `recorder.rs:268`。
- **每请求一个传输线程**：`llm/openai.rs:548`。
