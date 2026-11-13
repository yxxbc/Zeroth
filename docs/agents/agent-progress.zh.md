# subagent 的进展更新 —— 在它的行上，也在 `agentoutput` 里

日期：2026-10-22

## 请求

> 加上 "Agent: ..." 来显示 agent 的中间响应或进展更新
>
> ```
> ● Agent(Gather GitHub contexts for user linuztx)
>   ⎿  Agent: ... <-like Bash: ... or Edit: ... something like that
> ```
>
> 还有 agentoutput 工具，把 agent 的中间响应或进展更新也带进 TUI 和
> context，好让主 agent 有完整的 context 感知

## 缺的是什么

一个会说话的 subagent —— `Let me fetch the repositories next.`、
`The profile lists 42 public repos; checking the events now.` —— 把那些话
说给了除它自己的 session 视图之外谁也不听的空气。

- **活动行只点名调用。** 一个实时 agent 之下的那条粘滞行
  （`docs/agents/agent-tool.md`）由 `AgentRun::last_call` 构建，所以 agent 在写字
  时，它显示的是之前那次调用（`⎿ Bash: Fetch the profile`）或
  `⎿ Initializing…`，不管它此后说了多少。
- **`agentoutput` 把那些话扔在了地上。** subagent 的转发器
  （`llm::backend::spawn_subagent_run`）累积回复文本以知道这次运行的最终答案，
  而每一个 tool 事件都把它**清空**：对答案来说是对的，但它清掉的文本去了无人之
  处。于是一个轮询长跑 agent 的 lead 读到的是 `Bash(curl …)` 那些行，而 agent
  发现了什么、决定了什么，一个字都没有 —— 它唯一的补救办法是等最终响应，或者
  `agentsend` 一个问题再等它。

## 什么算一条进展更新

**一段后面还跟着更多工作的 agent 回复文本**：一个 tool 轮之前的文本、一条它在运
行中途读到的消息之前的文本、一个 stop hook 的继续之前的文本
（`docs/tools/hooks.md`）。恰好就是转发器过去清空文本的那些点，理由也恰好相同
—— 它们之前的东西不是答案。后面什么都不跟的那段文本**就是**答案，一如既往地报
告（`Final response:`），永远不是一条更新。一个思考阶段也不是：它的链式思考是模型
的干活过程，不是它说的话（`docs/ui/thinking-stream.md`）。

## 活动行：`Agent: {update}`

```
● Agent(Gather GitHub contexts for user linuztx)
  ⎿  Agent: The profile lists 42 public repos; checking the events now.
```

这一行那一条文法（`{Name}: {detail}`，`agents::activity_line`）多了一个主体：
agent 自己，写作 `agents::AGENT_UPDATE_LABEL`。它显示

- **最新的那一段** agent 正在说的话 —— 这一段最后一条空行之后的行，拼成一行、
  每一串空白一个空格，并按宽度像每条活动行那样裁剪。不是整段：一段话的开头在它
  说出*现在让我……*的时候已经过时了，而这一行要的是最新的意图。也不是最新的一
  行：一个列表或一个折行的句子会显示成一个碎片。在同一段之内这一行是**前缀稳定**
  的 —— 裁剪保留头部 —— 所以一段一旦比这一行更宽，它就不动了，而不是在用户眼皮
  底下逐字滚动；
- **在文本流式期间**，每帧从 `AgentRun::streaming` 读取；
- **在它定稿之后**，保存在 `AgentRun::last_update` 里 —— 比如穿过一批的
  permission prompt，在文本与调用之间 —— 直到**下一次调用开始**才清掉它。粘滞
  规则变成双向的：这一行显示 agent 做过的最新的一件事，一个调用或它说的一句话，
  并在两者之间留住它，而不是掉回 `Working…`。

**没有一帧会再读一遍整段文本。** 文本流式期间这一行每帧要构建好几次，而一段文本
可以是任何东西：一个几千行的表格或代码块、一个不含空格的 URL 或大块数据、一整页
空白。所以 `AgentRun` 在每个 chunk 到达时记下最新那一段从哪里开始、正在写的这一行
上有没有东西（`ParagraphStart`，chunk 之前的任何东西都不再读一遍，一个被拆在
chunk 之间的行 —— 含 CRLF 结尾 —— 落在整段文本读取会放它的地方），而这一行只读
那一段的头部：最多 `ACTIVITY_UPDATE_MAX_CHARS`（512）个字符，取自不超过它四倍的
长度，好为任何缩进或表格内边距留出余地，并让一块数据在它自己内部就停住、让一段
空白达不到它的结尾。第一个版本每次都从文本末尾重新找那一段，于是一个几千行的段落
每帧都被走一遍，一块没有空格的数据被整份拷贝。第二个版本仍然每帧都去裁正在写的
那一行来判断它是否开启了一段，于是一行以四兆空格结尾花掉一行 24 ms；现在它花几
微秒。

transcript 保留文本原样；这一行的一句话是从它派生的，所以关于这一行措辞的任何东西
都不会进入 `history`、session 视图或 rollout。这一行出现在每条活动行出现的地方
—— 实时组单元格，孤立的或树形的，在一个前台组运行时。一个后台 agent 的话则用下面
的 `agentoutput` 读到，或者在它的 session 视图里读到，那里一直就显示它们。

## 记录：`ProgressUpdate`

注册表槽位把这个 agent 的更新保存在它的调用和它读过的消息旁边
（`AgentSlot::updates`、`AgentSnapshot::updates`），每一条是一个
`ProgressUpdate { at, read, text }`：

- `at` —— 它说这话时已经做了多少次调用，`FollowUp` 的规则；
- `read` —— 到那时它已经读过多少条消息，它把这个更新放在同一个 `at` 的那些标记
  之间（见下文 *一个标记放在哪里*）；
- `text` —— 已去掉首尾空白，并**整个**保留到 `AGENT_REPORT_UPDATE_CHARS`
  （2,000 个字符）为止，在那里以 `…` 截断。一段空白不是更新，不被记录。

`AgentRegistry::record_update` 就是那扇门。转发器经由 `agents::RunRecorder`
到达它，后者现在拥有转发器过去内联做的那些记账 —— agent 上次做别的事以来的文本、
每次 `ToolStart` 处的调用记录，以及现在每一个在更多工作之前结束一段文本的点处的
更新 —— 所以那条规则是被单元测试的，而不是住在一个线程体里。转发器只留下结果和
扣住的终结事件（`docs/agents/agent-tools.md` *One notice per answer*）。一个
`StreamEvent::Retrying` 会丢掉迄今收集的文本：流出它的那次尝试作废了，它的请求在
回复下面的连接丢失之后被重新发送，所以那段文本既不是答案 —— 那会把这份残缺和取
代它的回复粘在一起交给 lead —— 也不是一条更新（`docs/providers/offline.md` *A
connection lost mid-reply*）。

## 报告

`agentoutput` 的报告（以及 `agentkill` 的，那是同一份报告）按它发生的顺序列出
agent 的时间线 —— 每一次调用、它读过的每一条消息、它给出的每一条更新：

```text
Running (agent a7k2m9x4q · Gather GitHub contexts for user linuztx · 42s · 2 tool uses)
Agent: I'll start with the profile, then the repositories.
Bash(curl -s https://api.github.com/users/linuztx)
Agent: The profile lists 42 public repos and 100 followers.
  Fetching the repositories next, newest first.
Bash(curl -s "https://api.github.com/users/linuztx/repos?per_page=100&sort=updated")

[Still running — you are notified when it finishes; agentoutput with `wait` waits for it.]
```

一条更新是 `Agent: ` 加上它的第一行，其余缩进两个空格在下面 —— 就是用户消息注记
的形状 —— 并丢掉它的空行，因为报告里的一个空行是分节。它**整个**列出，而一次调用
是一行截在 160 个字符处，这个差别正是重点：调用行概括的是一个动作，而它的输出
lead 刻意从不为它付钱；而一条更新是 agent 自己对发现了什么、正在做什么的陈述 ——
lead 最需要的内容，实践中也就几句话。2,000 字符的上限是对一条病态更新的界定，不
是预期的尺寸。

窗口不变：最新的 30 次调用被列出，更早的被计数，而发生在那些被藏起来的调用之间的
事 —— 在那里读到的消息、现在还有在那里给出的更新 —— 与被它们一起藏起来，而不是
显示在不属于它的位置。

## 等待的流

一个等待中的 `agentoutput` 流式输出 agent 在被观察期间做的事
（`docs/agents/agent-tools.md`），现在也包括它说的话：每条更新作为一行流出，
`Agent: {flattened}`，像调用那样截断（`report_call`），而它读到的每条消息作为
报告给它的那个标记。单元格的尾部是一个四行窗口，一条完整的更新放在那里会把与它
并列的调用挤出窗口；落定单元格的输出才是报告，更新整个显示。

等待每 50 ms 轮询一次，所以一次轮询花多少，在一个忙碌 agent 之上的长等待里很要紧。
一次轮询是 `AgentRegistry::progress(id, &mut cursor)`：在注册表的锁下它只读
`ProgressCursor` 之后的那些条目 —— 单元格已经显示了多少次调用、多少条消息、
多少条更新 —— 格式化它们，并把游标移过它们。单元格已经显示过的任何东西都不会被
再拷一遍，而第一个版本每次轮询都取一次 `snapshot`，一秒二十次地把这个 agent 的
整份历史连更新一起拷走。它读的尾部就是报告自己的尾部：后记录的条目永远不会排到
先记录的前面，因为用来给它定位的那些计数只可能增长过，所以游标之后的那些行就是
时间线最新的部分，按它的顺序，于是这个流与等待结束时的那份报告不可能不一致。

## 保持原样的东西

- **完成通知与前台结果**仍然只携带最终响应，它之前的什么都不带。一条通知是 lead
  在那一刻并没有要过的答案；更新是给主动去问的 lead 的。
- **Ctrl+O 的 agent 单元格**保留它的 prompt、调用头部和响应；agent 的 session
  视图才是读它整段对话的地方。
- **其他离线 demo**：它们脚本化的 agent 不说话，所以它们的报告读起来与从前一样
  （`smoke.sh` Phase 98）。follow-up demo 的 agent 会说（见下文 *测试*）。

## 一个标记放在哪里

agent 读到的一条消息，由它自己的循环在一个轮边界处从队列里取走
（`AgentRegistry::take_pending_inputs`），在 agent 的线程上。它的调用与更新由转发
器在转发这个流时记录，在另一个线程上 —— 而转发器可能落后于循环：一个微秒级就返回
的调用、一台有负载的机器。每条条目都以它被记录时读到的计数为键，所以在 drain 处
记录的标记可能是按转发器还没追上的计数定的键，于是报告把它排在了一次调用之前、
或者排在 agent 在它之前说过的话之前。这个竞态早于更新存在，更新只是给一个标记多
添了一件它可以超到前面去的东西。

所以 drain 只记下每个标记说的是什么 —— lead 的消息一个，用户的消息各一个带引号
—— 而标记放在流宣告那条消息的位置。循环为它取走的每条消息发一个
`StreamEvent::Steered`，就在取走它们之后，而 subagent 的流上没有别的东西会发它；
转发器的 `RunRecorder` 在转发一次 drain 的第一个宣告时，记下 agent 在那之前说了
什么，然后把这个 drain 的标记放在它已经转发过的那些调用之后
（`AgentRegistry::announce_read`）。调用、更新与标记都在同一个线程、按流的顺序被
记录。各个 drain 按顺序排队，因为循环可能在转发器到达第一个 drain 的宣告之前又
drain 一次，而每个都按自己的宣告摆放。一个没有任何宣告到达的 drain —— 两者之间被
切断的一次运行 —— 在这次运行落定或重新开始时被摆放，晚到而不是永远不到；而
`AgentRegistry::user_messages` 从 drain 起算用户的消息，因为一次停止可能在转发器
转发它们之前就被报告。

## 测试

- `agents`（单元）：文本流式期间这一行显示最新的那一段，文本定稿之后留住它，并
  让位给下一次调用；一段空白什么都不改变；一个已落定的 agent 仍然读作 `Done`；
  一个一万字的段落、一段不含空格的文本、一整页空白，都只被读到界限为止，而一行以
  四兆空格结尾的行在一毫秒之内构建完成（一个墙钟闸门）；那一段被放在整段文本读取
  会放它的地方，对每一个长度不超过六个字符、由字母、空格、CR 与换行构成的文本，
  在每一个 chunk 之后，在它可能被切的每一种方式下，以及在一条手写的、带 CRLF 结尾
  且词被拆在 chunk 之间的流里。报告按 `at`/`read` 把更新摆放在调用与标记之间，把
  一条多行更新缩进渲染、丢掉它的空行，并把它们藏在那些被藏起来的调用之中；
  `AgentRegistry::progress` 只把新增的东西交给一次等待，一行一个，按顺序，标记也
  在内。`record_update` 去掉首尾空白、在上限处截断、跳过一段空白。`RunRecorder`
  在每一个在更多工作之前结束一段文本的点处记录一条更新 —— 并且在一次
  `ToolStart` 记录它那次调用的前面 —— 并把答案的文本整个保留。一次转发器在它尚未
  记录的调用之后才转发的 drain，被标记在那次调用之后、agent 在它之前说过的话之后；
  两个在飞的 drain 各在自己的宣告处被标记；一个从未被告宣告的 drain 在运行结束时
  被标记，它的用户消息从 drain 起算。
- `ui`（单元）：孤立单元格与树形单元格把 `⎿  Agent: …` 渲染成暗色并裁剪。
- `llm::agent_tools`（单元）：报告把更新交给 lead，而一次等待会把 agent 在被观察
  期间给出的一条更新流出来，摆在它在调用之间的位置上，并带上它同时读到的某条消息
  的标记。
- `stream::dummy`（单元）与 `scripts/smoke.sh` **Phase 130**：离线 follow-up demo
  的 agent 在它的调用之前说出它要做什么，经由真实转发器所用的同一个
  `RunRecorder` 记录，所以它的第一次等待在调用之上流出
  `Agent: I'll read the top of the changelog first.`，而它的报告列出两者，无需
  网络。
- `scripts/smoke.sh` **Phase 144**：真实 backend 对着一个 stub provider。一个
  前台 agent 的回复在流中途被扣住，好让 lead 的实时单元格能被读到显示
  `⎿  Agent: …`；然后 lead 调用 `agentoutput`，而那份报告 —— 在 lead 的
  transcript 里，也在 stub 所收到的请求里 —— 把该 agent 的两条更新都携带着，摆在
  它们在各次调用之间的位置上。
