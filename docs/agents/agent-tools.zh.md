# agent tools —— 每个 agent 都有 id，而 lead 可以操作它

`agent` 启动一个 subagent（`docs/agents/agent-tool.md`）。在此之前，那是
lead 模型对一个 agent 唯一能做的事：一个后台 agent 是一次发了不管的启动，
它的最终响应作为一条通知到达，而已完成的 agent 就没了。bash 家族展示了更好的
形状 —— `bash` 启动一条命令，四个同伴来操作它留下的那个 session
（`docs/tools/bash-tools.md`）—— 而 agent tools 就是那个形状，往上提了一层：

| tool | 参数 | 做什么 | 询问权限 |
| --- | --- | --- | --- |
| `agent` | `description`、`prompt`、`subagent_type?`、`run_in_background?` | 启动一个 agent；结果点出它的 id | 从不 |
| `agentsend` | `agent_id`、`message` | 运行中的 agent 在当前步骤之后读到这条消息；已完成的 agent 用它**恢复自己的对话**，在后台 | 从不 |
| `agentoutput` | `agent_id`、`wait?` | agent 的状态、它已做的 tool call，以及它完成后的最终响应；`wait` 秒会阻塞到它完成 | 从不 |
| `agentkill` | `agent_id` | 停掉一个运行中的 agent —— 它的对话被保留，`agentsend` 可以恢复它 | 从不 |
| `agentlist` | — | 这个 session 的每个 agent：id、类型、任务、状态、运行时长、tool 使用数 | 从不 |

这些同伴的线上名各是一个小写单词，bash 同伴们的约定：单元格显示
`AgentSend`，而显示名转成小写就是线上名，所以 context 重放不需要为它们准备
任何映射表。它们是**lead 的** tool：subagent 永远不会被提供它们，正如它永远
不会被提供 `agent` —— agent 不会嵌套，而让一个 agent 去操纵它的兄弟，是这个
TUI 不接的协调问题。

## Id

每个 agent 一直都有 id —— 注册表里的 `a` 加 8 个 base36 字符（`a7k2m9x4q`）
—— 但没有任何面向模型的东西携带它，因为也没有任何面向模型的东西把它要回去
（启动文本自己的注释把一行 `agentId:` 称作「一个没有消费者的 token」）。现在
有四个 tool 消费它，所以每一个引出 agent 的结果都会点出它：

- 一次**后台启动** —— `Async agent "Fetch GitHub profile" launched as
  a7k2m9x4q and working in the background. …`，它同时也劝住轮询（等是给
  现在就需要结果的情形）；
- 一个**前台结果** —— agent 的最终响应，然后是
  `[agent a7k2m9x4q — agentsend continues this conversation]`；
- 一条**完成通知** —— `[background agent] Agent "Fetch GitHub
  profile" (a7k2m9x4q) completed in 35s.`；
- **Ctrl+O 的 agent 单元格** —— `● Agent(Fetch GitHub profile) · a7k2m9x4q`，
  好让用户把一个 `AgentSend` 单元格对上它所够到的那个 agent。

一条已记录条目的 `output` 是不可变的 —— 它是模型读到的内容 —— 所以这次改动
之前的一次启动在重放时不带 id，与它被发出时一样。完成通知不一样：它的注记在
每次重放时从记录渲染，所以一条旧的会在那里得到它的 id（prompt 缓存会 miss
一次，在携带它的第一个恢复后的回合上）。

## 契约

### `agentoutput`

```json
{"agent_id": "a7k2m9x4q"}
{"agent_id": "a7k2m9x4q", "wait": 300}
```

agent 一直在做什么，以**摘要**形式 —— 每次 tool call 一行 `Name(args)`，与
Ctrl+O 的 agent 单元格所列出的是同一批单行，加上它一路上说的话，每一条
**进展更新**都是它在说的位置的一条 `Agent: …` 条目
（`docs/agents/agent-progress.md`）—— 之上是一行说明它此刻站在哪里的框架行：

```text
Running (agent a7k2m9x4q · Fetch GitHub profile · 42s · 3 tool uses)
Agent: I'll start with the profile, then the repositories.
Bash(curl -s https://api.github.com/users/linuztx)
Bash(curl -s "https://api.github.com/users/linuztx/repos?per_page=100&sort=updated")
Agent: 42 public repos, most of them Rust.
  Checking the recent events next.
Bash(curl -s https://api.github.com/users/linuztx/events/public)
```

一旦它落定，框架行说明是怎么落定的，它答了什么跟在后面：

```text
Done (agent a7k2m9x4q · Fetch GitHub profile · 1m 5s · 3 tool uses)
Bash(curl -s https://api.github.com/users/linuztx)
Bash(curl -s https://api.github.com/users/linuztx/repos)
— message received —

Final response:
…
```

`Failed (…)` 带上它的错误，`Stopped (…)` 说明是谁停的 —— 以及 `agentsend`
能否恢复它。只列出最新的 30 次调用，每次截成一行；更早的被计数
（`… 12 earlier tool calls`），而 agent 在其中说过的话随它们一起走。tool 的
输出刻意缺席：那是 agent 的 context，不是 lead 的，而 subagent 的意义就在于
lead 永远不为它们付钱。它自己的话则不然：一条更新是它对发现了什么、接下来要
做什么的陈述，所以它整个列出 —— 最多 2,000 个字符，第一行放在 `Agent: ` 之后，
其余缩进在下面。

框架行的数字覆盖 agent 的**整个生命** —— 它的运行时和 tool 使用数，跨启动与每
一次继续累加 —— 而它读过的每一条消息都在它到达处被标出：lead 自己的
`agentsend` 和一条路由给它的后台 shell 完成是 `— message received —`，而用户
在它的 session 里敲下的一条是 `— message from the user: {text} —`，带引号，
因为 lead 从未见过它（见下文 *用户自己的消息*）。两者都出自一次真实运行：
框架行过去给出当次运行的运行时，旁边是生命周期的 tool 计数，于是一个模型把
`9s · 5 tool uses` 读成它的 follow-up 做了五次调用 —— 它一次也没做，而第五次
调用之后的那个标记现在就说明了这一点。

`wait`（秒，默认 0，最多 600）阻塞到 agent 落定或时间过去。一个等待中的单元格
会**流式输出** agent 在等待期间启动的每次调用、给出的每条更新、读到的每条消息，
各一行，按报告的次序，所以用户能从 lead 自己的 transcript 里看着 agent 干活，
而 Ctrl+B 提前结束等待（agent 继续跑）。每次轮询只拷回新增的内容
（`docs/agents/agent-progress.md` *The wait stream*）。每一份关于仍在工作的 agent
的报告 —— 一次查看，或一次过期的等待 —— 都以一条注记收尾，说它仍在运行、以及
怎么等它：一个没用 `wait` 只看了一眼的模型，曾告诉用户那个 agent 已经停了。

### `agentsend`

```json
{"agent_id": "a7k2m9x4q", "message": "Also list the repos with the most stars."}
```

- **运行中** → 消息加入 agent 的队列，在下一个轮边界到达它的循环 —— 与用户在
  agent 的 session 视图里敲下的消息所乘的是同一个接缝（`docs/input/queue.md`）。
  完成通知到来时会同时回答两者。
- **已落定**（完成、失败，或被 lead 停掉）→ agent 存下来的对话**恢复**，以这条
  消息作为它最新的回合，在后台进行；它下一次的完成会像一次后台启动那样被通知。
  这就是用户所要的 follow-up：一个已完成的 agent 保留它的 context，所以第二个
  问题只花一条消息，而不是让一个新 agent 从头把一切再推一遍。
- **被用户停掉** → 拒绝。名单里的 `x` 说了停，而模型不得撤销它；启动一个新的
  agent 仍然可以。

它立刻返回。要阻塞等答案，就接一个 `agentoutput` + `wait` —— 两次调用可以搭在
同一条消息上，它们按顺序运行。

### `agentkill`

停掉一个运行中的 agent：它的循环被取消的方式与名单里 `x` 取消它完全一样，正在
跑的调用解决为 `Interrupted by user`，而结果是那份 `Stopped (…)` 报告。与用户的
停止不同，它**让对话仍可恢复** —— kill-then-send 就是 lead 在不等待当前步骤的
情况下把一个跑偏的 agent 拨回来的方式。一个已经落定的 agent 会被如实报告；没有
任何东西被停两次。

### `agentlist`

```text
3 agents:
- a7k2m9x4q (general-purpose) Fetch GitHub profile — running 42s · 3 tool uses
- a1b2c3d4e (explore) Search the repo — done in 1m 5s · 12 tool uses
- a9z8y7x6w (general-purpose) Fix the tests — stopped by the user after 30s · 2 tool uses
```

运行中的 agent 加上保留下来的已落定者，按启动顺序 —— 好让一个因 `/compact`
丢了 id 的 lead 不必瞎猜就能再找到它。

## 用户自己的消息

用户也可以跟一个 agent 说话 —— 在它的 session 视图里按 Enter，会把一条消息排进
它正在跑的循环，或开始一个已完成 agent 的继续（`docs/input/queue.md`）。那些
消息不是 lead 写的，而它过去无从分辨：agent 的下一个答案作为一条普通的完成通知
回来，读起来像 lead 自己对话里的一句回复，而一次与一个前台组早已解决的 agent 的
对话则根本到不了 lead。所以用户的消息在 lead 从 agent 那里听到的每一处都会被
标明来源：

- **完成通知**在它的结果与答案之间点出它们 ——

  ```text
  [background agent] Agent "Fetch GitHub profile" (a7k2m9x4q) completed in 35s.
  The user messaged this agent directly in its session view — these came from the user, not from you:
  - also list the repos with the most stars
  Final response:
  …
  ```

- 一个**前台结果**在答案与 `agentsend` 那一行之间携带同样的一行，用于用户
  在 lead 等待期间发出的一条消息；
- **`agentoutput` 报告**在每条消息到达处引用它；
- 而一条让**已落定**的 agent 重新开工的消息，在它开工的那一刻就告知 lead
  （见下文 *一次恢复在开始时就是新闻*），而不只是在它新一轮结束的那条通知里。

一条通知只点名**它自己那次运行**读过的消息：每一次继续都从零开始
（`AgentRun::user_messages`，由 `reopen` 清空；注册表的 `user_messages(id)`，
从该次运行的起点开始计）。而一次由用户启动或操纵的运行，**即使是一个前台
agent，一旦它的组已解决，也就欠着一条通知** —— 那个组的结果再也无法携带它 ——
而一个仍在等待的组的成员由那个组报告，绝不重复。

名单不靠新事件就能区分它们：只有用户敲下的消息会等在 `AgentRun::queued` 上
（lead 的 `agentsend` 和一条路由来的 shell 注记是不经通知就到达的），所以一次
清掉排队行的 `Steered` 投递就是用户的，而用户启动的一次继续由
`App::agent_chat` 记录为他的。在注册表那一侧，用户的门是
`queue_user_input` / `note_user_message`，就在 lead 的 `queue_input` 旁边。
通知的清单保存在 rollout 里（`user_messages`，为空时省略），所以一次恢复的
session 会重放出同样的通知。

### 一次恢复在开始时就是新闻

一条把**已落定** agent 带回来的消息 —— 一个已完成、失败或被停掉的 —— 过去只
经由那次运行的完成通知到达 lead，而在那之前，lead 一直相信它上次听到的。被报告
的案例：一个前台 agent 在搭前端时撞上一个错误，lead 读到 `[agent failed: …]`
就把它放在一边继续干活，而用户在该 agent 的 session 视图里给它发了消息 —— 于是
它重新拾起自己的任务，而对此一无所知的 lead 可以自由地重做同样的工作，或者启动
一个替代者。所以启动这次运行的那条消息在**它启动时**就被宣告：

```text
[background agent] Agent "Build the frontend" (a7k2m9x4q) is running again: the user resumed it after it failed. It continues its own conversation, and you will be notified with its final response when it finishes. Leave its task to it — don't redo the work or launch another agent for it; if you need its result to go on, agentoutput with `wait` blocks until it finishes.
The user messaged this agent directly in its session view — these came from the user, not from you:
- the API key is set now, carry on
```

它是一个 `resumed` 被置位的 `AgentNotice` —— shell 通知里 `waiting` 的孪生体，
一条不报告结束的通知：`status` 是它之前那次运行是怎么结束的（lead 最后听到的），
`user_messages` 是恢复它的那条消息，而它的单元格在系统通知的青色里读作
`● Agent "Build the frontend" was resumed by user`。它走完成通知的那条路 ——
上板，它的单元格一直留到 lead 在一个轮边界读到它，lead 空闲时则立刻记录 ——
有两处不同。它不是以模型启动注记的身份贴出的
（`BackgroundRegistry::post_agent_resume_notice`），所以它自己**不开启任何
follow-up 回合**：此刻还没有什么可答的；那次运行的完成通知，因为用户的消息而
仍然欠着，带来那个回合，而它的 context 那时两者都带上。并且它与 agent 的
完成在标签上分开（`PendingNotice::resumed`，在 `agent` 旁边），因为它因另一个
事件而过期（见下文）。它对已记录的组条目毫无改变
（`App::settle_agent_completion` 跳过它）：Ctrl+O 的单元格保留 lead 读到的那个
结果，直到那次运行自己报告为止。

这条注记只在 lead 听到那次运行如何结束之前算新闻，而 lead 自己的两次调用可能
先告诉它，那时注记还在等轮边界：一份对已落定运行的 `agentoutput` 报告，以及
`agentkill`，它的结果无论哪种情形都说运行结束了 —— 停止的报告，之后不再有通知；
或者对一个早已结束的运行的 `not running`，而它的完成通知仍会随后到来。在这两者
任一之后读到注记，它都会说 agent 正在跑、并且会有通知随后到来。所以两者都会
把它未读收回，它扣着的单元格一同丢弃
（`BackgroundRegistry::retract_agent_resume_notice`）：
`AgentRegistry::report_settled`，无论那次运行的完成是怎么站住的
（*每个答案一条通知*），以及 `AgentRegistry::stop` —— lead 的停止，无论是它
真的停掉了运行还是发现它已结束 —— 都在同一个加锁步骤里。一份在用户把它重新
启动**之前**读到那次运行的报告是例外：它收回它读到的那次完成，而把注记留下，
因为那注记说的是 lead 还没听说过的一次运行。注册表在它自己的锁下区分两者
（这次运行还是结束的吗？），而循环也在那把锁下贴出注记
（`AgentRegistry::post_resume_notice`，只在它所宣告的那次运行还在进行时），
所以一次报告、一次停止和这次贴出永远不会相交。一次在它的注记来得及贴出之前
就结束的运行不欠任何通知：它的完成通知同样点名用户的消息，而 lead 先做的那次
停止的报告会给它们打上标记（`— message from the user: … —`）。

用户消息到达一个已落定 agent 的两种方式都算：从它的 session 视图开始的聊天
继续 —— Enter，或一个在它落定时发出的 Tab follow-up（`App::agent_chat`）——
以及一条在它写最后回复时排入队列、由它自己的线程继续的消息（那个重开一次已落定
运行的 `Steered`，由 `App::apply_agent_event` 折叠）。lead 自己的
`agentsend` 不欠任何通知，因为它知道它；对一个 lead 仍在等待的前台组的成员也
不欠，因为那个组的结果携带了恢复后的运行并点名了那条消息；对一条发给仍在工作的
agent 的消息也不欠，因为 lead 从不曾相信它已经结束 —— 它的完成通知会点名那条
消息。

要让以上任何一条成立，名单必须已经看到上一次运行的结束，而注册表可能领先它
一步：一个 Tab follow-up 在绘制 tick 里发出，它可能先于 agent 通道的分支折叠
落定而运行，而一个 `agentoutput` 等待者扣住的结束只有在新运行开始时才被释放
（`begin_continuation`）。一个仍然读作运行中的条目会不重开就接下这条消息，在
旧的结束到达时于新运行之下落定，并且丢掉随后那次运行的每一个事件 —— 它的答案
连同它的通知。所以当注册表回答 `Started` 时，`Session::agent_chat` 先排空
agent 通道，旧运行的结束就在那里等着。

`smoke.sh` Phase 141 通过真 backend 对着一个 stub provider 驱动这个被报告的
案例：subagent 的请求被拒绝，lead 继续它的回合，用户从该失败 agent 的 session
视图给它发消息让它回去干活，而 lead 的下一个请求就带着那条注记 —— 就在它在用户
说话时所做那次调用的结果之后 —— 而单元格落在那次调用的下面。Phase 142 跑同样
的案例，但 lead 扣着的那个回合对用户刚救回来的 agent 应答 `agentkill`：它的下
一个请求带着 kill 的报告 —— 用户的消息在其中被标出 —— 而没有注记，也没有恢复
单元格被提交。Phase 130 覆盖空闲的 lead：用户在离线 demo 的 agent 完成后给它
发消息，而恢复单元格落在答案的前面。

## 保留

一个已落定 agent 的注册表槽位过去随它的名单行一起被丢掉，在它完成 30 秒之后
（`AGENT_LINGER`）。一个 follow-up 需要对话活得比那一行长，所以两者解耦了：

- **名单行**仍然照旧滞留，并照旧被清扫；
- **注册表槽位** —— 它存下的消息、它的进展记录 —— 为 [`AGENT_RETAINED_MAX`]
  （16）个最近落定的 agent 保留，有新 agent 注册时淘汰最旧的；
- 被清扫的**名单条目**移进 `App` 里一个同样大小的退役列表，所以一个 lead 恢复
  的 agent 回到名单时*带着*它的 transcript，它的 session 视图显示整段对话。从
  那个列表里被淘汰的，只凭它的 prompt 回来。

`/clear` 把这一切都丢掉 —— 那些槽位、那些退役条目 —— 因为知道那些 id 的对话
已经没了。重启之后什么都不留：一个来自已恢复 session 的 id，会用确实存在的那些
agent 的列表来回答。

## 每个答案一条通知

一个后台 agent 的完成会贴出一条注记，lead 在它下一个轮边界听到。一个已经报告了
同一结果的 `agentoutput` 会让那条注记变成重复，所以注册表跑的是 bash session 的
等待者协议（`BackgroundRegistry::finalize`）：

- subagent 的转发器**扣住**这次运行的终结事件（`StreamDone`/`Error`），在结果
  被记录之后交给 `AgentRegistry::settle`；
- 没有人在等时，`settle` 立刻送出它 —— **在注册表锁下**，所以一个看到 agent
  已完成的启动者（前台组的等待循环）知道事件已经在通道上了，这正是让那个组的
  解决能先排空它的原因（`docs/agents/agent-tool.md`）；
- 有一个 `agentoutput` 在等时，事件被扣住；等待者的 `end_wait` 读取最终状态并
  **在一个加锁步骤里**释放事件，在那个状态已落定时标记 `observed` —— 而一次被
  观察的落定不贴任何注记、也不提交任何通知单元格（`AgentOutput` 单元格就是记录）。

用户的 `x` 原子地问同一个问题（`kill` 返回是否会有等待者报告这次停止），所以
一次被用户停止结束的等待不会被同时通知。

一次**没有任何等待看到的**落定，以未被观察的状态发出去：落在 `agentoutput` 把
agent 读成运行中与注册它的等待之间那一瞬的那一次，或者落在 lead 根本没看之前的
那一次。任何把已落定结果交给 lead 的 `agentoutput` 随后都会告诉注册表
（`AgentRegistry::report_settled`），而事件循环用它对**每一次**未被观察的落定
做账（`AgentRegistry::post_notice`，无论是否欠一条注记），两者都在注册表锁下，
所以两者永不相交：

- 报告先到：这次运行所欠的注记被消耗掉（槽位保留它欠注记的那些运行，
  `notices_owed`），循环什么都不贴，并且以没有通知单元格的方式落定组条目 ——
  即被观察的路径；与此同时贴出的一次运行结束注记（用户的停止）也被收回；
- 注记已贴出但 lead 还没取走：报告把它从板上收回
  （`BackgroundRegistry::retract_agent_notice`，板上的注记带着它们的 agent 和
  运行标签），随它扣着的通知单元格在循环于回合末尾落定它时被丢弃
  （`BackgroundRegistry::take_retracted`）。板按每条注记的**编号**记住收回，
  而不是按 agent：循环会一直扣着单元格直到 lead 读到它的注记
  （`docs/tools/background.md` *Where a notice lands*），所以一个恢复后的
  agent 的下一条注记可以在一条被收回的注记仍被扣着时贴出，而按 agent 保存的
  标记无法区分两者；
- lead 已经取走了它：报告就是一次重读，而单元格记录它所读到的注记。

无论哪一种，一条仍在说用户启动了那次运行的注记也会随之作废
（*一次恢复在开始时就是新闻*）。

### 一份报告覆盖一次运行

agent 的每一次运行都有一个编号 —— 启动是 0，每一次继续加一
（`AgentSnapshot::run`）—— 而一次运行结束的注记带着那次运行的标签：每次落定都
点名它（`AgentEvent::Settled::run`），离线 demo 的脚本化运行以真实转发器的方式
落定，而用户的停止点名它停下的那次运行（`Stop::run`）。绝不能用循环贴注记时的
当前运行：到那时用户的消息可能已经开始了下一次，它的报告就会把这条注记当作自己
的收回，而不是把它交回去。一份报告只为它读到的那次运行做账。这一点要紧，因为
用户的消息可以在一次运行结束的瞬间开始下一次：一条在 agent 写最后回复时排入队列
的消息，由 agent 自己的线程在落定之后立刻继续（`docs/input/queue.md`），所以
一个在等它的 `agentoutput` 会看到 agent 又开工了、报告之后的那次运行，而第一次
运行的结束，被这次重启释放出来，作为一条普通通知发出。一份把该 agent 所有未读
注记都收回的报告，会丢掉那第一个答案 —— 对 lead 所给任务的答案 —— 而只收回自己
那次运行的，会把第一个答案排到更新的那个之后，一个更旧的 `completed` 被最后读
到。所以报告把更早的答案交回去：板上每一个更早运行仍然未读的完成都从板上被取
下，它的单元格被丢弃
（`BackgroundRegistry::take_earlier_agent_notices`），携带在报告的末尾，最旧的
在前，之上是一行说明它先发生：

```text
[Before this run it had ended an earlier one, whose notice had not reached you:]
[background agent] Agent "Build the frontend" (a7k2m9x4q) completed in 2m 5s.
Final response:
…
```

一次更早的运行，若循环还没贴出它的注记 —— 循环为整整一次运行卡住了 —— 则仍然
欠着，并作为一条通知到达 lead。`smoke.sh` Phase 143 通过真 backend 对着一个
stub provider 驱动这个案例：lead 在等一个后台 agent，用户的消息在 agent 的最终
回复被扣住时排入队列，而等待的结果携带两个答案，新的在前。

留下的边缘：一个在回合中途边界就提交了的通知单元格，在报告收回它未被取走的注记
之前就已经进了历史，所以之后几个回合的 context 会把答案携带两遍。以及已记录的
组条目 —— 一条被收回的注记在它的单元格于回合末尾落定时仍会更新它 —— 在一次更晚
运行的报告更新过它之后，可能在 Ctrl+O 里显示出更早那次运行的结果 —— 只是显示；
context 重放的是线上格式所携带的内容。

## 实机验证

对着 Venice（`z-ai-glm-5-3`，经由 Agent Zero 代理）与 Ollama Cloud
（`gpt-oss:120b`），在 tmux 下的真实二进制里：一次后台启动、一次
`agentoutput` 等待，它流式输出了 agent 的调用并在其后报告了它的答案而没有
任何通知；一次 `agentsend`，它以 context 恢复了已完成的 agent，并在其上第二次
等待；`agentlist`；一次循环中途的 `agentkill`，随后一次 `agentsend` 把同一个
agent 拨回去，它的答案随后作为一条通知到达，开启了空闲的 follow-up 回合；以及
一条排进运行中 agent 的消息，在它当前步骤之后被读到，并在它的 session 视图里作为
用户气泡显示。

其中两次运行保留为 `tests/live_subagents.rs` 里 `#[ignore]` 的 live 测试
（`A0_VENICE_API_KEY=… cargo test --test live_subagents -- --ignored`）：一个
已完成的前台 agent 按 id 恢复，并用 `agentoutput` 等它，它的新答案在它自己的
定义之下；以及一个后台 agent 被检视、被 lead 停掉、再被列出，注册表同意这次
停止是可恢复的。

离线时，dummy 的 `agent-follow-up` demo 无需网络就播放其中第一个 —— 它的同伴
通过真实 executor 调用，它 agent 的运行由真实转发器的 `RunRecorder` 记录并经
由真实注册表落定，所以它的第一次等待就在调用之前流式输出 agent 所说的话 ——
而 `smoke.sh` Phase 130 在真实二进制里驱动它
（`docs/architecture/dummy-backend.md`）。

## 接线

- **权限**：同伴们一个都不问 —— 启动从来不问，而发给 agent 的一条消息本身不
  运行任何东西；agent 随后运行的任何东西都以它自己的调用去面对 gate
  （`docs/state/permissions.md`）。
- **secrets**：`agentsend` 的消息交给另一个模型，所以它的占位符永不展开
  （`crate::secrets::expands_placeholders`）；每份报告出去时都像任何 tool 的
  一样被脱敏。
- **hook**（`hooks::claude_code_alias`）：每个同伴都以自己的显示名作为第二个
  精确名字应答 —— `AgentSend`、`AgentOutput`、`AgentKill`、`AgentList`。
- **单元格**：`● AgentSend(Fetch GitHub profile ← Also list…)`、
  `● AgentOutput(Fetch GitHub profile)`、`● AgentKill(Fetch GitHub
  profile)` —— agent 从调用被宣告的那一刻起就用它的 description 点名
  （`tools::summarize_call_naming`，bash session 的查找也应答 agent id）。
  一个等待中的 `AgentOutput` 渲染得像一条运行中的命令：流式出来的行在
  `(12s · wait 5m)` 时钟行与 `(ctrl+b to stop waiting)` 之下拖尾。
- **名单**：恢复后的 agent 以一个新的运行时钟重新出现（或就在那里重开，如果它
  还在滞留）；被 lead 停掉的那个变红，没有用户的 `x to clear` 行，并滞留自然的
  30 秒。

## 布局

| | |
| --- | --- |
| `src/llm/tools.rs` | 那些 spec、参数类型、显示名与头部摘要 |
| `src/agents.rs` | 注册表（身份、进展、保留、等待者协议）、转发器用来保存记录的运行记录器，以及纯粹的报告格式化 |
| `src/llm/agent_tools.rs` | executor —— 注册表之上的各个同伴，一次通过注入的启动器 spawn 的继续 |
| `src/llm/backend.rs` | 转发器的扣留、启动文本里的 id、路由 |
| `src/app/agent.rs`、`src/tui/agent.rs` | 名单那一侧：被观察的落定、lead 的停止、恢复与复活 |

[`AGENT_RETAINED_MAX`]: ../src/agents.rs
