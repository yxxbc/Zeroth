# agent session 视图像主视图一样流式输出

日期：2026-08-26

## 报告

> subagent tui 里面的流式输出 —— 它想流式输出的时候就消失了。我不确定是不是
> 因为它在流一个表格。

是的。而且，来自第二份报告：

> agent 用并行 tool call 时结果不显示 —— 你需要刷新 TUI。它成功创建了文件，
> 但没有更新内联的 TUI。

两个 bug，一个形状：**agent session 视图保留了自己的一份「主视图做什么」的
副本，而那份副本落后了。**（在那个视图里 `/copy` 也会复制错对话 —— 而 footer
的 context 量表一直描述着 lead 的对话，直到
`docs/agents/agent-context-gauge.md`。）

## 实际发生的是什么

agent session 视图（`docs/agents/agent-tool.md`）有自己的流式条带，由
`ui::agent::agent_view_preview_lines` 构建。对于一段流式的回复，它做的是：

```rust
message_lines(Role::Assistant, text, width).pop()   // the LAST row, batch-rendered
```

那**不是** agent 的提交所留下的边界。
`tui::agent::commit_agent_view_event` 的 `Chunk` 分支经由
`Session::agent_render` —— 一个 `ui::StreamRender`，与主 session 用的是同一个
增量渲染器 —— 提交，而 `StreamRender::commit` 是**按源码行**扣留的：一个未闭合
的表格、一行围栏代码、一个不完整围栏 / 标题 / 列表标记、一个未闭合的行内标记、
一个成形中的 URL（CLAUDE.md 不变量 2）。

那两半本应守住的契约是
`committed ++ preview == the whole reply at every instant`
（`StreamRender::preview` 的文档）。主条带守住了它，因为
`Session::stream_preview_lines` 是通过提交所用的那同一个渲染来预览的。agent
视图破坏了它：`commit` 扣留了整个块，而条带显示了它的一行。其余一切在屏幕上
**哪儿都没有**。

对 `anthropic/claude-haiku-4.5` 实机捕获（100×30 tmux，一个在 subagent 自己的
session 视图里向它要的 8 行 markdown 表格）：

```
                        agent view (before)                main view
  ┌───────────────────────────────────────────┬──────────────────────────────┐
  │  └────────────┴──────┴──────────┴───────┘ │ ┌──────────┬──────┬────────┐ │
  │                                           │ │ Language │ Year │ Typing │ │
  │  (●•·   ) Working… (2s · ↓ 2.2k tokens)   │ ├──────────┼──────┼────────┤ │
  │                                           │ │ Python   │ 1991 │ Dynamic│ │
  │                                           │ │ Java…                      │
  └───────────────────────────────────────────┴──────────────────────────────┘
```

一行 —— 表格的*底边框* —— 而在主视图里，整个成形中的网格随着更宽的单元格到达
而重排各列地流式输出（`docs/ui/table-streaming.md`）。对用户来说，那就是
「流式输出消失了」。

同一个视图里还有三处偏离，全是同一个根因（主条带逻辑更薄的一份第二副本）：

1. 一条运行中的 `bash` 调用显示朴素的 `⎿ Running…` 一瞥，而不是拖着它流式的
   输出（`running_command_lines`，`docs/ui/tool-streaming.md`）。
2. **思考阶段不可见**：`AgentRun::apply` 把 `ThinkingStart`/`ThinkingEnd`
   列在它的空操作里，只对 `ThinkingChunk` 做*计数*，所以一个推理中的 subagent
   显示一个光秃秃的 spinner，而主视图显示实时的 `● Thinking…` 块，之后是落定的
   `Thought for 1s · 206 tokens (ctrl+o to expand)` 单元格
   （`docs/ui/thinking-stream.md`）。
3. 条带的状态行从不说 `Thinking for Ns`（`agent_view_status` 把
   `thinking: None` 写死）。

还有 `/copy`：`App::last_assistant_text` 无条件遍历 `App::history`，所以在
一个 agent session 视图里它复制的是**主**对话的最后一条 assistant 消息 —— 实机
验证：屏幕上是 agent 的表格，剪贴板里是 lead 的
`Background agent "table demo" completed…` 摘要。

## 修法

### 1. 一个边界，两个视图

`Session::stream_preview_lines` 变成懂得视图的。开着 agent session 视图时，它
通过 `self.agent_render`（它提交所用的那个渲染）预览**被查看 agent 的**缓冲；
否则一如从前，通过 `self.render` 预览主缓冲。两种情况它都用
`App::set_stream_preview_rows` 注入行数，而 `ui::preview_rows` 的 agent 分支
为一个流式 agent 报告的就是那个数，与主分支完全一样 —— 所以预留的行与绘制的行
按构造仍然一致（条带的 `debug_assert`）。

`agent_view_preview_lines` 接受构建好的预览
（`stream_preview: Option<&[Line]>`）并使用它；旧的批量渲染留作 `None` 的
fallback，纯单元测试和任何没有渲染的调用方用的就是它 —— 与主分支的
`preview_lines` 早已采用的那个形状*相同*。

`repaint_agent_view` 在 `live_region_height()` **之前**计算预览（那个计数喂给
高度）并把它传给渲染闭包；它过去传 `None`，所以视图的每一次 purge 重建
—— 一次 resize、一次 overlay 返回、一次 flow 退出 —— 都会把多行预览塌成那一帧
的一行。

### 2. 一个渲染器供实时 tool 单元格

`ui::live::preview_tool_lines` 里逐调用的正文搬进一个共享的
`live_call_lines(tool, elapsed, pulse, width)`，而
`agent_view_preview_lines` 调用它。subagent 运行中的 `bash` 现在拖着它的输出，
带着同样的 `+N lines (Ns · timeout …)` 页脚 —— 以及在任何输出之前同样的
`⎿ Running… (Ns · timeout …)` 行 —— 而它的 `!`-shell / 普通情形保持相同。两个
界面上 elapsed 都是**命令自己的**：主条带读 `App::command_elapsed`
（`docs/ui/tool-streaming.md`），而 agent 视图读
`AgentRun::command_elapsed` —— 每帧由 `Session::agent_command_clocks` 从边界
注入，即思考时钟的孪生体（条目在 agent 的 `ToolStart` 时插入，
`clear_agent_command_elapsed` 然后在每次绘制前对每个运行中的调用
`set_agent_command_elapsed` —— 所以一条已解决命令的 `(Ns)` 会掉落而不是冻住）。
一处刻意不同：思考时钟在它唯一的落定辅助函数处被*移除*，而一条命令会在四个事件
和两个本地落定上解决，所以命令表改为**在名单 tick 处**从运行自己的队列里修剪
—— 一个前台调用不是 `Running` 的 agent 会失去它的条目 —— 而没有任何解决路径
需要携带一个要忘记的移除。它过去是 agent 整个 `run.runtime`（也就是
`agent_view_status` 所显示的），那会把状态行的数字放在一个刚刚开始的调用下面
—— 正是主条带在回合 elapsed 上犯过的同一个 bug
（`docs/ui/tool-streaming.md`，*Whose clock the footer shows*）。

延迟出现的 `(ctrl+b to run in background)` 提示刻意**不**共享：
`App::background_hint_elapsed` 是按**主**回合的命令时钟来决定它的，而 agent
视图里的 Ctrl+B 移动的是整个组，不是那次调用。按另一个回合的时钟去宣传它，就是
撒一个谎。

### 3. agent 把思考说出来

`AgentRun` 得到主 session 的那三件东西，拆分方式相同：

- `reasoning: Option<String>` —— 打开阶段的那个缓冲。由
  `AgentRun::begin_reasoning`（经由 `App::begin_agent_reasoning`）打开，边界
  **只在 `/settings` 的 **Hide thinking** 关着时**调用它 —— 所以显示关掉时永远
  不会打开任何缓冲，`ThinkingChunk` 只做计数，与从前完全一样（即
  `ZEROTH_SHOW_THINKING` 的姿态，`docs/ui/thinking-stream.md`）。
- `round_reasoning: Vec<usize>` —— 这一轮的单元格落在哪里，好让 agent 自己的
  `Usage` 帧把它们的 token 估算吸附到 provider 的 `reasoning_tokens`。那个按权重
  分配的辅助函数现在是共享的纯函数 `app::reasoning::snap_reasoning_tokens`，由
  `App` 与 `AgentRun` 一样地调用，而不是各抄一份。
- `thinking: Option<Duration>` —— 打开阶段的 elapsed，每帧由
  `Session::agent_thinking_clocks` 从边界注入（`set_status_times` 的模式），
  所以 `agent_view_status` 显示 `Thinking for Ns`，而没有任何时钟进入纯核心。

`Session::settle_agent_reasoning` 是那个唯一的落定辅助函数 ——
`Session::settle_reasoning` 的兄弟 —— 从一个阶段可以结束的三个地方调用：它自己
的 `ThinkingEnd`、agent 的 `Error`，以及一次用户停止。它把
`HistoryItem::Reasoning` 记录在**agent 的** transcript 上，并且在那个 agent 的
session 视图开着时提交折叠后的单元格（提交之前先重新就位）。`ThinkingStart`
先冲刷 agent 已流出的分段 —— 交叠之前先 flush —— 所以标题坐在落定单元格将要
占据的同一个空白间隔符之下。

下游的一切都免费：`ui::conversation_lines`（视图的重建）与
`ui::transcript_lines`（它的 Ctrl+O）早就会渲染 `HistoryItem::Reasoning`，而
`context::context_messages` 早已跳过它。

### 3b. 条带其余的对齐

还有两处较小的偏离随之一起走，都是「agent 视图有自己更薄的一份副本」：

- **`retrying {n}/{max}`。** `AgentRun::apply` 把 `StreamEvent::Retrying` 扔在
  了地上，而 `agent_view_status` 写死 `retry: None`，所以一个正在重连的
  subagent 显示一个光秃秃的 spinner，而主回合会说出它在做什么
  （`docs/providers/llm.md`）。`AgentRun::retry` 现在携带它，由下一个流式
  chunk 清掉，正是 `App::push_chunk` 清掉主视图那个的方式。一次重试也作废那次
  失败的尝试，这里与主视图一样：`AgentRun::apply` 撤回它所流出的内容，而一个
  打开着的 session 视图会在重建时不再有它已提交的那些行
  （`docs/providers/offline.md` *A connection lost mid-reply*）。
- **agent 的 Ctrl+O 尾部。** `agent_transcript_lines` 渲染已落定的
  `HistoryItem::Reasoning` 单元格，却不渲染*打开中的*阶段；主尾部把它整个显示
  （`reasoning_live_full_lines` —— 分页器没有行预算，不像条带的窗式块）。现在
  它也会了。

### 3c. 视图提交的是折叠*记录*的东西，而不是到达的事件

第二份报告，同样的形状：一个 subagent 创建文件，而那些单元格从不出现 —— 直到
你 resize 终端，然后它们一下全出现。

`commit_agent_view_event` 列出了那些能解决一次调用的事件：

```rust
StreamEvent::ToolEnd { .. } | StreamEvent::ToolRejected { .. } | StreamEvent::ToolBackgrounded { .. }
```

`157e032`（"write/edit: record the model's arguments, hand it a one-line
result"）把 `write`/`edit` 挪到了**两段文本拆分**上 ——
`ToolOutcome::context` → `StreamEvent::ToolAnswered`，于是带编号的正文留在
单元格上，而模型读到一句确认。它更新了 `src/tui/stream.rs`，即主视图的提交
路径。它没有碰 `src/tui/agent.rs`。所以从那次提交起，subagent 的
`write`/`edit`（以及一次载入的 `skill`）落在它的 transcript 上就到此为止；
一次 resize 从同一份历史 purge 重建视图，于是所有缺失的单元格一起出现。一批
并行的 write 让它无法被忽略 —— 这就是它被报告的方式。

修法不是「把 `ToolAnswered` 加进那个清单」。手维护的事件清单正是漂移的东西，
而这个视图已经落后主视图一次（§1）。`Session::commit_agent_tail` 改为以
**`AgentRun::apply` 追加到 transcript 上的东西**为键：边界在折叠之前快照
`run.history.len()`，之后提交它之后记录的每一个条目 —— 一个 `Tool` 经由
`ui::tool_commit_lines` 作用于到它为止的历史（它仍然持有一个 MCP 运行的成员，
直到该运行结束），一个 `Summary` 经由 `ui::summary_lines`。那就是一次重建所
回答的同一个问题，这正是两者现在一致的原因；未来一个解决事件完全不需要在这里
做任何改动。是*每一个*条目，不是最新的一个：一次落定会同时解决一整批并行调用
—— 运行中的调用，以及每一个仍 `⎿ Waiting…` 的兄弟
（`docs/input/interrupt.md`）—— 只读最后一个会把在它前面的单元格留在屏幕之外。

到达 transcript 的其他一切都有自己的提交者，不能落两次 —— 一个冲刷出的文本
分段属于 `agent_render`，一个落定的思考属于 `settle_agent_reasoning` —— 所以
它们穿透到底。一条 hook 注记只有携带 report 时才在这里提交：agent 的
`● Ran 1 SubagentStop hook` 单元格（`docs/tools/hooks.md` *The hook-run
cell*）；没有 report 的一条内联不可见。而那条红色错误通知是一次失败确实*不*
记录的唯一东西（运行只保留它的 `error` 字段），所以它仍然显式提交，在失败所
解决的那个单元格之后：在这次改动之前，`Error` 分支提交了通知，却悄悄丢掉了那个
单元格。

同一个辅助函数也收束**本地的**落定，它们完全不携带事件：名单的 `x` 通过
`AgentRun::interrupt` 解决 agent 的实时批次 —— 运行中的调用以及排在它后面的每
一个兄弟 —— 而它的 `⎿ Interrupted by user` 单元格有一模一样的问题。

### 4. 视图之上的 prompt 问的是*这个*对话

第三份报告，再次是同样的形状：

> 在 subagent TUI 里面它显示
>
> ```
> ● Agent(Run ls -la via subagent)
>   ⎿  Working…
> ```
>
> 它应该显示 `● Bash(ls -la)` / `⎿ Waiting…` —— 与主 agent TUI 一样。而
> subagent 的并行 tool call 应该显示每一个等待中的 tool。

一个 permission prompt 会替换整个 live 区域，所以在一个 agent session 视图里，
那个 agent 的流所剩下*唯一*的东西就是 prompt 自己的 context 单元格 —— 而
`ui::permission_view::context_chunks` 是这个视图里最后一个从未被改成懂得视图的
界面。它读 `App::agent_group()` 和 `App::tool_queue()`：**lead 的**实时单元格
和**主**回合的队列，画在一个显示另一个对话的屏幕上。对一个孤零零的前台 agent
来说，那就是上面那个 `● Agent({description})` / `⎿ Working…` 单元格；对一个
后台 agent（它的组早已解决）来说，那就*什么都没有*，所以 prompt 光秃秃地打开，
而本该是 agent 自己的 `● Bash(ls -la)` / `⎿ Waiting…`。两者是同一个 bug。

context 现在来自被查看 agent 自己的 `tool_queue`，走主分支所用的那同一次
`tool_lines` 遍历（`queue_chunks`，两者共享）—— 所以 subagent 的并行批显示被
询问的那次调用压住每一个 `⎿ Waiting…` 兄弟，空行分隔，与主视图显示主回合的
一模一样。`context_is_stable`（flow 的资格判定，`docs/ui/view-flow.md`）读同
一个队列。

要说出是*哪一个*对话在问，需要一个 id 而不是类型：两个 agent 可以共用
`general-purpose`，而请求的 `agent` 字段只是标题所点名的类型。
`PermissionRequest` 得到 `agent_id`，由 `tui::agent::Session::on_agent_event`
打上 —— 路由该事件的那个边界是唯一知道它的地方。一个 id 不是当前打开视图的请求
（主回合自己的调用、某个兄弟 agent 的）是在另一个屏幕上竖起它的单元格的，所以
prompt 打开时完全没有 context，而不是去借这个 agent 无关的批次。

### 5. 冲刷点是折叠的答案，不是一个清单

`on_agent_event` 在折叠消费缓冲之前，先把 agent 已流出的文本收尾 ——
`agent_render.finish`、间隔符、`reset`。哪些事件是这些，曾经是一个**手维护的
清单**，正是 §3 刚刚为提交分支退役的那种东西，而它已经漂移了：
`StreamEvent::HookNote` 在 `AgentRun::apply` 里冲刷缓冲（一个 `SubagentStop`
block 的继续反馈，`docs/tools/hooks.md`），却在这里缺席。于是回复被扣留的尾部
到达了 agent 的 transcript 却从未到达它的屏幕，*而且* `agent_render` 还持有着
一个已经不存在的缓冲的前缀 —— 而那次继续的下一个 `Chunk` 又对着它提交。

现在这个清单是折叠自己的，就放在定义它的折叠旁边：边界在折叠之前问
`AgentRun::will_flush_segment` —— 静态的那些边界（一个私有的
`flushes_segment`，好让边界无法只问那一半）加上唯一只有运行状态能回答的那个，
即打开 `write` 实时预览的那一块（`docs/ui/write-streaming.md`）。而
`agents::tests::every_flush_point_is_declared` 会走遍**每一个**事件变体 ——
包括那个开场的块 —— 以同样的方式问这个谓词，并断言它与折叠对缓冲的看法一致。
一个新的变体混不过去。（`ThinkingStart` 单独处理：它也冲刷，但只在显示打开时，
而那道闸门是边界的。）

**本地**落定需要同样的待遇。`AgentRun::interrupt` 收尾三件东西 —— 打开中的
思考、那一串流式文本、运行中的调用 —— 没有任何事件携带它们中的任何一个，而边界
只跑了 `commit_agent_tail`，它提交已记录的尾部，让其余条目穿透到*它们的*提交者。
那些提交者并没有运行。于是已落定 agent 的思考，和它部分回复被扣留的尾部，都从
视图的 scrollback 里丢掉了，而 `agent_render` 仍然持有着一个
`flush_segment` 早已取走的缓冲的前缀 —— 下一次聊天继续的 `Chunk` 又对着它提交。

**有三扇门通向那次落定**，它们现在都走
`Session::begin_local_agent_settle`（`settle_agent_reasoning`，然后是共享的
`flush_agent_segment`，然后是它的 `commit_agent_tail` 所需的 transcript 长度），
之后 `interrupt` 自己的各个落定都是空操作：

1. 名单的 `x` —— `Session::stop_agent`；
2. 一个 backend `Error`，它的 `App::fail_stream` 调用
   `App::resolve_live_agent_group`，并中断该回合实时**前台**组里每一个尚未收尾的
   成员；
3. Esc 中断，它的 `App::interrupt_turn` 做同样的事。

(2) 与 (3) 共享 `Session::begin_turn_agent_settle` /
`finish_turn_agent_settle`，它们只在被查看的 agent 确实是那个组的一个成员
**且有一个回合在飞**时才干活 —— 否则两个 app 方法都提前返回，而一次背后没有
落定的折叠前冲刷会在缓冲仍持有它的文本时重置渲染，在下一个 chunk 时把整段回复
再提交一遍。

### 6. 折叠从 `App` 那里漂走的还有三个地方

subagent 的 `tool_queue` 是 `App` 批次机制的第二份实现
（`docs/tools/parallel-tools.md`），而那份副本在三处小地方落后了，每一处都用
主路径自己的规则加一个单元测试修好：

- **一次批次宣告是这一轮的全部队列。** `App::start_tool_batch` 会*替换*
  `tool_queue`；折叠则追加，所以更早一轮留下的任何东西都会坐在最前面，抢走这一
  轮的解决，把每个单元格都错了一位。
- **每一次解决都记录 `truncated`。** 主边界在 `ToolEnd`、`ToolRejected` *和*
  `ToolAnswered` 上标记它；折叠只理会第一个，所以 subagent 一次被封顶的
  `write`/`skill` 结果在 Ctrl+O 里悄悄声称自己是完整的（没有暗色的 `…` 标记）。
- **一次交给后台的调用要记上它的确认。** `resolve_front_tool` 把每次解决的
  面向模型文本折进 `↑` 的计数；折叠的 `ToolBackgrounded` 分支什么都没记。
- **一个组的解决会落定它成员的实时调用。** 一个前台成员若自己的终结事件从未
  到达 —— 一个被杀掉的循环不带它返回，离线 dummy 也一个都不脚本 —— 过去由
  `App::finish_agent_group` 用一个光秃秃的 `agent.status = …` 赋值落定，于是
  一个*已完成的* agent 继续拥有实时单元格：它的 session 视图预览着一个永远无法
  解决的 `⎿ Running…`。它现在经由共享的 `AgentRun::settle_from_group` 落定
  （部分思考、`resolve_tools`、状态），与其他每一次落定的方式相同。

### 7. `/copy` 复制屏幕所显示的

开着 agent session 视图时，`App::last_assistant_text` 读**被查看 agent 的**
transcript，否则读 `App::history` —— 也就是 `viewed_agent()` 的那条分支，而
`ui::context_lines`、`ui::agent_transcript_lines` 与 Ctrl+D 的 classifier 页
早就在走它。剪贴板 I/O、toast 和空的情形都不动。

## 测试

- `ui`（单元）：被查看 agent 的条带预览注入的边界，而 `preview_rows` 预留的
  正是它；`commit ++ preview` 重建出一个成形中表格的整个块；一个运行中 agent
  的 `bash` 单元格拖着它的输出；实时的 `● Thinking…` 块被预览，且
  `agent_view_status` 携带该阶段的 elapsed。
- `ui`（单元）：在一个 agent session 视图里，prompt 的 context 是那个 agent
  自己的调用（`● Bash(ls -la)` / `⎿ Waiting…`，绝不是 lead 的 `Agent(` 单元格），
  一批并行调用显示每一个等待中的兄弟、空行分隔，而来自另一个对话的请求完全不留
  context —— 同时**主**视图自己的 prompt 仍然保留 lead 的树，同一条规则的另一
  半。一个溢出的页面保留 agent 的静态单元格、丢掉它*运行中*的那个，所以
  `context_is_stable` 的新分支在要紧的地方被覆盖（`docs/ui/view-flow.md`）。
  视图的*条带*预览同一批，而 `preview_rows` 预留的正是它绘制的那些行。
- `agents`（单元）：**每一次**解决 —— `ToolEnd`、`ToolAnswered`、
  `ToolRejected`、`ToolBackgrounded` —— 都把它的单元格留作 transcript 的最后
  一个条目，而一个 backend `Error` 把它杀掉的那次调用留在那里（`commit_agent_tail`
  据以取键的不变量；旧的清单才是漂移的东西）。`will_flush_segment` 在折叠之前
  被问，对每一个事件变体都与折叠一致，包括 `write` 开场的那一块
  （`every_flush_point_is_declared`）；一次批次宣告替换队列；三种解决都记录
  `truncated`；一次交给后台的调用记上它的确认。
- `app`（单元）：一个前台组的解决会落定它成员的实时调用
  （`a_foreground_groups_resolution_settles_its_members_live_calls`）—— 运行中
  的调用被解决到 transcript 上，队列被清空。
- `tests/live_openrouter.rs`（live，`#[ignore]`）：同一份单边界契约跑在**真实
  模型**的流式回复上 —— 九种宽度下的敌意 markdown，包括退化的窄宽度，以及一份
  CJK+emoji 语料在奇宽度下（一个双列字形跨在换行点上）—— 跟踪 provider 自己
  的 chunk 边界，好让一次失败点名那个把渲染弄坏的分割。这是生成器编不出来的
  语料，在每一个字符前缀上核对。`apply` 只在阶段打开期间缓冲 `ThinkingChunk`，
  打开一个会先冲刷它之前的文本，落定在 `Error` 与 `StreamDone` 两种情形下都把
  `HistoryItem::Reasoning` 记录在部分回复**之前**，该轮的 `Usage` 吸附计数，一次
  中断保留部分思考，而一个 `Retrying` 会一直显示到下一个 chunk 清掉它。
- `ui`（压力测试，`stream_stress.rs`）：agent 视图的边界在 40 份生成的敌意
  文档 × 3 种宽度 × **每一个字符前缀**上守住
  `committed ++ strip == the reply so far`，经真实的 `preview_lines` 分发驱动
  —— 主 harness 的契约，跑在那个曾保留它第二份副本的视图里。行比较器携带文本、
  两种颜色、修饰符**以及 `underline_color`**，那是 OSC 8 的链接载体
  （`docs/ui/links.md`）而不是装饰：没有它，链接驻留上的批式对流式分歧会无声
  通过。
- `stream`（单元）：`agent-stream` 条目有一个选中*它*的示例 prompt（注册表的
  可达性测试），并且在没有注册表挂上时被跳过，落到它所压过的 `table` demo。
- `app`（单元）：`last_assistant_text` 跟随被查看的 agent，而从视图里执行的
  `/copy` 返回那段文本。
- `scripts/smoke.sh` **Phase 98**：离线的 `agent-permission` 场景
  （`src/stream/dummy/agent.rs`，cue "subagent" + "permission"）。三个细节让它
  能复现，而三者都是真 backend 的形状：启动是**前台**的（一个后台组会立刻解决，
  于是没有任何实时 lead 单元格存活下来盖住东西 —— 那会让下面那个否定断言变得
  空洞），各次调用在任何一次运行之前作为**并行批**宣告，而请求在 `ToolStart`
  **之前**就升起，即 approve 接缝自己的顺序，这也是被询问的那次调用真的读作
  `⎿ Waiting…` 的原因。这个阶段断言 lead 的 `● Agent(…)` 单元格*确实*在主视图
  里，↓ ↓ Enter 的走查到达了 session 视图（这样一个竞态会点名它的原因而失败，
  而不是作为一个内容不匹配），那里的 prompt 在 `● Bash(pwd)` 之上显示 agent
  自己的 `● Bash(ls -la)`，带两个 `⎿ Waiting…` 行且**没有** `● Agent(`
  单元格，以及 Esc 回到主视图时 lead 的单元格仍在屏幕上。
- `scripts/smoke.sh` **Phase 95**：驱动真实二进制。dummy 新增一个
  `agent-stream` 场景（`src/stream/dummy/agent.rs`，cue "subagent"），它启动
  一个后台 subagent，并从那个 agent 自己的线程在 subagent 通道上播放*它*的回合
  —— 一个思考阶段，然后是表格 —— 这是 agent session 视图第一次能在没有网络的
  情况下被驱动 —— 一个思考阶段，一批**两个 `write` 调用的并行批**（两段文本
  拆分，那个被报告的案例），然后是表格。这个阶段把名单走查进那个 session，并
  断言实时的 `● Thinking…` 块出现，成形中的**网格**（不只是它的收尾边框）在流
  中途就在条带里，两个 write 单元格**无需 resize** 就到达 scrollback，表格恰好
  提交一次，而落定的 `Thought for …` 单元格落在 agent 的 transcript 上。
