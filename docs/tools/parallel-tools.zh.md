# 并行 tool call —— 可见的一批 + `⎿ Waiting…`

当模型**在一个 assistant 回合里请求多个 tool call**（比如同时三条 `Bash` 命令），
TUI 会把**它们全部提前显示**：每个被 worker 接手的调用都实时渲染（它自己的闪烁
灰点压在它自己拖尾的输出之上 —— `docs/ui/tool-pulse.md`），可能同时有好几个，而
还没有 worker 够到的那些渲染成暗色的 `⎿ Waiting…` 单元格 —— 就是 Claude Code
的并行 tool 样子：

```
● Bash(ping -c 20 google.com)
  ⎿  64 bytes from … icmp_seq=6 ttl=117 time=68.3 ms
     64 bytes from … icmp_seq=7 ttl=117 time=247 ms
     64 bytes from … icmp_seq=8 ttl=117 time=332 ms
     64 bytes from … icmp_seq=9 ttl=117 time=144 ms
     +5 lines (9s)

● Bash(ping -c 20 facebook.com)
  ⎿  64 bytes from … icmp_seq=3 ttl=117 time=71.0 ms
     +2 lines (9s)

● Bash(ping -c 20 x.com)
  ⎿  Waiting…
```

每一个运行中的调用都会**拖着它自己的实时输出**（最后几行加上一个
`+N lines (Ns · timeout …)` 页脚，它的时钟就在它所处的超时旁边）随流更新 ——
见 `docs/ui/tool-streaming.md`；那些 `⎿ Waiting…` 兄弟不变，只有一个例外：一个
排队的 `Write`，它的文件在这一轮生成时已经流进了它的单元格 —— 它在
`Waiting…` 拐角之下保留那个文件最初的十行带编号行，所以这个单元格在解决时既不
塌缩也不长回去（`docs/ui/write-streaming.md`）。只在 live 条带里如此：permission
prompt 的 context 单元格保留单行的 `⎿ Waiting…`，因为 prompt 自己显示了那个
文件。

这解除了旧的限制（`docs/tools/tools.md`，在这次改动之前：*"Parallel
tool calls … are executed sequentially (the TUI shows one running tool at a
time)"*）。整批都是**可见**的，而每个调用在按模型顺序轮到它解决的那一刻提交进
scrollback。

**执行不再总是串行。** 一批其调用彼此独立的调用会跑在一个有界的 worker 池上
（见下文的 `run_agent_parallel`）：一轮三条各一秒的 `bash` 调用，在机器有空闲
worker 时大约一秒就完成。一批不能证明彼此独立的调用保持串行循环。闸门是
`llm::tools::batch_is_parallelizable`：

- 每个调用都必须是 `bash`/`read`/`write`/`edit` 调用（这些 tool 的 executor
  不持有任何共享的 session 状态）；
- 一批把 `bash` 与文件 tool 混在一起的调用是串行的 —— shell 可能读或写文件调用
  所点名的任何路径，而一轮 `Write(script.py)` + `Bash(python3 script.py)`
  依赖于写先落地；
- 两个点名同一路径的文件调用（已规范化，所以 `/repo/a`、`/repo/./a` 和一个
  符号链接的目录会撞上）是串行的 —— `write`/`edit` 是无锁的读-改-写，所以对
  同一个文件的并发编辑会悄悄丢掉一个；
- 解析不了的参数保持串行循环，在那里 tool 错误会确定性地被回答。

## 为什么这一批在运行之前就显示出来

显示早于 executor：可见的一批是为了让每个调用都提前宣布、然后按模型的顺序解决而
造的。那个顺序现在也是*记录*契约 —— 即使各任务以另一种顺序完成，单元格仍然按
宣布的顺序解决 —— 而**执行与 live 条带是重叠的**。每一个属于某个调用的事件都点
名它（`call_id`），所以来自第二个 worker 的一个 chunk 永远不会落到第一个调用的
单元格上。并发 executor 复用同一套 Waiting/Running/Ok/Failed 单元格状态；只有
`llm` 多了一个 worker 池，而 `app` 在它的顺序之外多了一条按 id 索引的路由。宣布
的顺序仍然是 `ToolCall::position` 重放进下一个请求的那个。

## 机制

### 协议（`stream/event.rs`）

一个新事件在第一个 `ToolStart` **之前**宣告这一批，为每个调用携带一个小小的命名
struct（`ToolCallSummary { name, args, call_id }` —— 与调用的 `ToolStart` 所携带
的是*同一批*字符串，所以一个 `⎿ Waiting…` 的头部与它开始运行后所显示的头部一致，
也与它之后的事件所携带的 id 一致）：

```rust
pub struct ToolCallSummary {
    pub name: String,
    pub args: String,
    /// The provider's id for the call; `None` for a scripted/dummy batch.
    pub call_id: Option<String>,
}

/// The model requested a batch of tool calls this round, announced up front so
/// the UI can show every call — the ones not yet executing as `⎿ Waiting…`.
ToolBatch(Vec<ToolCallSummary>),
```

每一个调用范围的事件都得到同一个 `call_id`：`ToolStart`、`ToolEnd`、
`ToolAnswered`、`ToolRejected`、`ToolNote`、`ToolOutput`、`ToolScreen`、
`ToolTitle`，以及 `ToolBackgrounded`（它保留自己的 `id` —— 后台注册表的任务 id
—— 就在旁边）。`None` 意思是「队列头部」，那正是串行路径（`!` shell、脚本化
测试），与从前完全一样。

`ToolStart`/`ToolEnd` 这一对的载荷其他部分不变。一个从不批处理的 backend
（`!` shell）干脆从不发 `ToolBatch` —— 一个孤零零的 `ToolStart` 仍然照常工作。

### App 状态（`app/tools.rs`）

`current_tool: Option<ToolCall>` 变成一个**队列**：`tool_queue:
VecDeque<ToolCall>`。每个调用要么是 `Waiting`（没有 worker 够到它）、`Running`
（它的 worker 宣告了它），要么已解决并消失。

- `ToolStatus` 多了一个 `Waiting` 变体（暗色，渲染为 `⎿ Waiting…`）。
- `start_tool_batch(&[ToolCallSummary])` 用 `Waiting` 单元格填满队列，每个都
  带着它 summary 的 `call_id`（以及，一轮宣布过它时，provider 的参数和位置）。
  每个调用还被打上这一批自己的 id（`ToolCall::batch`，一个普通计数器）—— 正是
  这个让后来的渲染器能区分一轮的并行调用与两轮的串行调用（MCP 运行的折叠，
  `docs/tools/mcp.md`）。孤立调用的那个是 `None`。
- `start_tool_for(call_id, name, args, arguments)` 把**那个** `Waiting` 单元格
  翻成 `Running`；一个没有任何单元格持有的 id 会推入一个全新的 `Running` 调用
  （一个孤立的调用，它的 start 点名了它自己的 id）。`start_tool(name,args,…)`
  是 `None` 的孪生体，并把串行路径保持得逐字节不变：翻转队头的 `Waiting`，否则
  推入。
- `end_tool_for(call_id, output, ok)` 移除那个 id 点名的单元格 —— 不一定是队头
  —— 把它收尾（Ok/Failed）、记录进 `history`，并把它返回去提交。
  `end_tool`/`reject_tool`/`answer_tool`/`background_tool` 保留它们的 `None`
  孪生体；`*_for` 变体接受 id。
- 那些实时输出的设置函数（`push_tool_output_for`、`push_tool_screen_for`、
  `set_tool_title_for`、`set_tool_note_for`、`set_tool_run_for`、
  `set_tool_truncated_for`）都按 id 寻址调用，而 `live_tail_for`
  （`App::live_tail_for`）返回**那个调用的**实时行 —— 两个并发的 `bash` 单元格
  各自跟着自己的进度重绘。
- `current_tool()` 仍然返回队头（旧的调用点继续能用），而 `tool_queue()` 把整批
  暴露出去供渲染。有好几个单元格都在 `Running` 时，队头只是模型的第一个调用，
  而不是「那个」运行中的调用。
- 在中断 / backend 错误时，队列里**每一个**调用都解决为 `Failed`
  （`Interrupted by user` / `Interrupted by a backend error`）—— 运行中的和
  `Waiting` 的单元格一样，按这一批的顺序（`App::fail_live_queue`）—— 而每一个
  都被记录并提交在通知之上。一个从未开始的调用仍然是模型做出的调用：它的单元格
  留在屏幕上，它的记录把它留在下一个请求的 context 里（见下文 *中断一批*）。
  撤销提交的快路径以**空**队列为键（`tool_queue.is_empty()`），即旧的
  `current_tool.is_none()`。一个已经解决过的调用的解决事件（Esc 先到了）是空
  操作。

### Backend（`llm::agent::run_agent`）

agent 循环早已提前拿到每一个调用（`RoundOutcome::ToolCalls{calls}`）。它在任何
执行之前发一个 `ToolBatch`（由 `calls` 构建），一如既往。

`run_agent_parallel`（backend 的入口）给循环一个 `ParallelTools` 接缝；
`run_agent` 传 `None`，为 subagent、测试以及每一批不合格的批次保持串行循环。对
一批合格的批次，循环的前置遍历仍然**按模型顺序串行**跑 `PreToolUse` hook 与
permission gate —— 在一个 hook 或用户做决定期间，什么都不运行，也没有任何东西
显示为运行中 —— 并在那个时刻把每个被允许的调用写进 classifier 的动作日志，所以
同一批里后到的裁决读到的是更早的调用，且按模型所问的顺序。只有在那之后 worker
才启动。

任务跑在 `min(worker count, jobs)` 个 scoped 线程上，由一个原子索引喂给它们。一
个 worker 在接手任务之前检查该回合的 `CancelToken`，所以一个在 Esc 之后串行循环
永远不会够到的调用不会被启动；解决遍历把它回答为 `Interrupted`，且不为一个从未
运行过的 tool 触发 `PostToolUseFailure` hook。**每个 worker 在它真正启动自己的
调用时才宣告它** —— 它的 `ToolStart` 和注记事先在循环线程上构建，因为
`session_command` 不是 `Sync` —— 所以每个正在执行的调用都显示 `Running`，而它的
`ToolProgress`（`ToolScreen`/`ToolTitle`）携带那个调用的 `call_id`。解决和
`PostToolUse` hook 仍然发生在循环线程上、按模型顺序、在 worker join 之后；这次
遍历只宣告没有任何 worker 够到的步骤（一次拒绝，或一个在派发之前就被 Esc 取消的
任务），所以没有注记被发两次，而每一个 `ToolStart` 在屏幕上恰好有一个
`ToolStart` 形状的事件。

backend 提供与串行循环相同的、经过 secrets 展开的 executor 和 classifier 日志，
只是通过 `Sync` 闭包：`ParallelTools::execute` 与 `ParallelTools::record`。

### 渲染（`ui/tool.rs`、`ui/live.rs`）

流式条带的**预览**槽渲染*整个*实时队列（每个调用的折叠单元格，空行分隔），而不
只是一个调用。`preview_rows` / `preview_lines` 从同一次队列遍历取尺寸和绘制，
好让框与光标的几何保持精确（由既有的 `debug_assert` 守护）。一个 `Waiting`
单元格是 `● name(args)` 加一行暗色的 `⎿ Waiting…`；**每个 `Running` 单元格都拖
着它自己的实时行**（`App::live_tail_for`），而 Ctrl+O transcript 的实时尾部也
遍历整个队列，所以一个等待中的调用永远不会被藏起来。`TranscriptCache` 的签名以
队列的长度和每个调用的 `(status, output length)` 为键，所以一次
Waiting→Running 翻转、第二个调用开始、或任一调用的输出变长，都会让缓存失效。

单个 `!` shell 的预览（它那行 `⎿ Running… (Ns)` elapsed 行）不变：shell 从不被
批处理（队列长度为 1，`shell` 标志）。

两个过去假定队列头部的界面现在问的是 **id**：

- **permission prompt** 把它的请求所点名的调用（`PermissionRequest::call_id`）
  提到 context 的最前面（`ui::permission_view::asked_first`），所以一批里的第五个
  问题会显示第五个单元格，哪怕前四个仍然排在它前面。
- **Ctrl+B** 点名第一个运行中的命令单元格（`App::background_target` →
  `BackgroundRegistry::request_background_for`），而只有那个调用的 runner 会接
  下这个闩锁；它的兄弟继续运行（`docs/tools/background.md`）。

## 中断一批

批处理中途的 Esc —— 或一个 backend 错误 —— 会同时结束模型所要求的每一个调用，
而它们每一个都保留自己的单元格，运行中的调用与 `⎿ Waiting…` 兄弟一样：

```
● Bash(ping google.com -c 10)
  ⎿  Interrupted by user

● Bash(ls -la)
  ⎿  Interrupted by user

● Conversation interrupted - tell the model what to do differently.
```

三件事让它站得住：

- **记录。** `App::fail_live_queue` 按这一批的顺序，通过 `ToolEnd` 走的同一条
  `end_tool` 路径，解决运行中（或等权限）的队头调用以及每一个 `Waiting` 兄弟。
  这些记录保留它们的批次 id、provider 调用 id 和位置，所以下一个请求的派生
  context（`context::context_messages`）重放整轮的方式与线上格式所携带的一致
  —— 一条 assistant 消息持有每个调用，各自被回答为 `Interrupted by user`
  （Claude Code 对一个被 abort 留下没有结果的 tool use 的自己的答案），然后是
  那条 `[error]` 通知。过去兄弟会被丢掉：模型的下一个回合看到第一个调用，其余
  什么都没有，于是它不再知道自己问过它们。
- **参数。** 一个从未到达自己 `ToolStart` 的调用只知道批次宣告所携带的头部摘要。
  在它之前的那次轮宣告（`StreamEvent::RoundCalls`）携带每个调用逐字的参数
  （`RoundCall::arguments`），打戳到它的 `Waiting` 单元格上，所以重放的是模型
  所做的调用 —— 一个 `bash` 调用的 `timeout`、一个 `write` 的 `content` ——
  而不是从摘要重建的一个。一个确实开始的调用改为取它 `ToolStart` 的参数，因为
  一个 `PreToolUse` hook 可能改写了要运行的东西
  （`docs/tools/hooks.md`）。离线 dummy 不宣告轮，所以它被中断的兄弟们通过摘要
  重建来重放，与每一条字段引入之前的记录一样。
- **提交。** 中断把它的红色通知记录在那些单元格**之后**，所以历史不再以一个
  单元格结尾，而 `ui::tool_commit_lines` —— 为刚刚解决的那一个调用而造 ——
  找不到可提交的东西。这过去会让那条通知独自留在屏幕上，一个单元格都没有，直到
  有什么从历史重建屏幕（关掉一个 permission prompt 就会，这也是第一个单元格
  有时会出现的原因）。`ui::resolved_tools_commit_lines` 改为提交最后 N 条 tool
  记录，每一条都恰如它自己解决时那样 —— 第一条会冲刷掉它所扣着的任何并行 MCP
  运行（`docs/tools/mcp.md`）—— 也就是一次重建从同一份历史里绘制的那些行。

一个 subagent 的一批以同样的方式落定（`AgentRun::resolve_tools`，在名单的 `x`
上、在一次把它那个组带下去的 Esc 上，或它自己的 backend 错误上）：每个调用都落
在它的 transcript 上，它的 session 视图把它们全部提交，而它存下的消息清单 ——
一次聊天继续所依据的那个 —— 也把一个被取消留下未执行的调用回答为
`Interrupted by user`，而它过去说的是 `[not executed]`，所以这次继续读到的正是
视图所显示的。

## demo（dummy backend）

`stream::turn_events` 是**按 prompt 门控**的，好让 demo 鲜明而不把每个回合的
scrollback 撑大（那会把内容顶出 smoke 套件据以断言的固定尺寸窗格）：

- 一个提到 **"parallel"** 的 prompt 跑那个鲜明的三调用 `Bash(ping …)` 批次
  —— 用户的例子（两个绿、一个红）—— 所以用 "parallel" prompt 跑
  `cargo run`，以及 `scripts/smoke.sh` 的专属阶段，都会显示全部三个单元格，运行
  中的那个实时跑在两个暗色 `⎿ Waiting…` 兄弟之上；
- 其他**任何** prompt 跑一个紧凑的两调用 `Read`+`Bash` 批次（`Bash` 在 `Read`
  运行时等待）—— 这个特性仍然每个回合都可见，且是**基线 scrollback 占用**，所以
  无关的 smoke 阶段保持它们的尺寸。

一个真实的 backend 经由 agent 循环所发的 `ToolBatch` 免费得到同样的显示，无论
模型实际请求多少个并行调用都能渲染。

## 在 subagent 内部

一个 subagent 跑的是**同一个** `llm::agent::run_agent`，所以它的各轮以同样的
方式宣告自己的批次，而转发器把每个事件逐字传给 agent 通道。
`AgentRun::apply` 的 `ToolBatch` 分支是 `App` 的孪生体 —— 每个被宣告的调用一个
`ToolStatus::Waiting` 单元格，替换整个队列（那次宣告就是这一轮的全部集合）——
所以 subagent 的并行批在那个 agent session 视图的全部四个界面上显示得与主回合
完全一样（`docs/agents/agent-tool.md`）：条带通过共享的
`ui::live::live_call_lines` 预览整个队列，Ctrl+O 展开每个单元格，每次解决提交进
agent 自己的 scrollback，而在那里抬起的一个 **permission prompt 通过主分支所用的
同一次 `queue_chunks` 遍历，把被询问的调用显示在每一个 `⎿ Waiting…` 兄弟之上**
（`docs/state/permissions.md`、`docs/agents/agent-view-streaming.md`）。

仍有两处刻意的不同，都是关于聚合而不是显示：subagent 的调用不携带批次 id，所以
一连串 MCP 单元格永远不会被折叠成一行 `Called {server} N times`
（`docs/tools/mcp.md`），而父级的 `● Running {n} agents…` 树为每个 agent 显示
一行粘滞的活动行，而不是每个调用一个单元格 —— 那棵树是名单，不是 transcript。

## 限制

- 一批的单元格只在这一批**全部**结束之后才解决，所以一个落在慢兄弟仍在运行时
  的 Esc，会把一个已经成功的兄弟记录成 `Interrupted by user`；它的副作用已经在
  磁盘上了。解决顺序是模型的，这是刻意的 —— 下一次迭代可以在每个 worker 的调用
  完成时就解决它，而不打乱记录，因为记录早已携带 `position`
  （`docs/providers/prompt-caching.md`）。
- 一个并行批仍然在第一个 `ToolStart` **之前**询问每个调用的权限，所以在用户做
  决定期间这些等待在前置遍历里是串行的，这一批得不到任何重叠。prompt 的
  *context* 是对的 —— 它按 id 领出被询问的调用（`docs/state/permissions.md`）
  —— 但批准本身无法并发（一个模态框，一次一个问题）。
- 对两个不同路径的 `write`/`edit` 仍可能经由一个规范化器看不见的**别名**路径
  竞争（一个在批处理期间创建的符号链接），而一批 `bash` 当然也能让它的各个调用
  互相依赖。这个闸门是安全护栏，不是依赖分析；模型被告知彼此独立的调用才是可以
  批处理的那种。
- 一个非常大的批次会把 live 区域向上撑；它像 live 区域的其余部分一样被夹到终端
  高度（于是框在内部滚动）。还没有按批次的上限 / 「+N more」折叠。
- 中断保留它能看见的调用，也就是实时队列。一轮中从未被宣告的调用仍然不留记录，
  所以下一个请求不携带它们：一个前台 agent 组之后的普通调用（那一批只有在组结束
  之后才宣告，`docs/agents/agent-tool.md`），以及一个循环还没够到的 task 调用
  （task 调用不渲染单元格，也从不会被宣告，`docs/tools/task-tools.md`）。
