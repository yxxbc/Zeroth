# 回合进行中时排队消息

这是对 openai/codex 的*排队的用户消息***及其 steering** 的移植：回复正在生成
时，提交的消息不必在键盘上等候 —— 它显示在框上方，并且**无需这一轮结束**就能
到达模型。两个键承载 codex 的两种意图：

- **Enter** 把消息交给**正在运行的**那一轮。它的 agent 循环在下一个**回合边
  界**接收它 —— 就在本回合的 tool 结果之后、下一个请求构建之前 —— 所以模型在
  同一轮*之内*就读到它，通常只差一个 tool call，而不是整整一轮。
- **Tab** 打开一个**新的回合批次** —— 它的消息作为*独立的后续回合*，在当前这轮
  之后、以及在已经排队的任何批次之后运行（codex 的 Tab-to-queue）。

所以有两个待处理集合，你按哪个键就选哪一个：当前这一轮即将读取的东西
（`App::steered`），以及排在它后面的后续回合（`App::queued`）。两者在框上方的
渲染方式相同 —— 待处理的消息就是待处理的消息 —— 而一轮**在**读到交给它的东西
**之前**结束，就会把它交回去，所以后续那条路径同时也是兜底，用户打过的任何东
西都不会丢。

**同一套机制在下一层也在跑 —— 而且是它的两半都跑。** subagent 的 session 视图
在那个 agent 自己的循环之上有这个一模一样的队列：**Enter** 把消息交给进行中的
那一次运行（它在框上方等待，并在*它*的下一个回合边界落进*它*的 transcript），
而 **Tab** 打开一个在该循环落定之后运行的后续回合。主会话与 subagent 共用那条
接缝（`SteerQueue`）、那个事件（`StreamEvent::Steered`）与那套渲染
（`ui::queued_lines`）；各自对自己循环从未读到的部分做回收，会话在它回合结束
时，agent 在它自己那次运行的线程里。见 `docs/agents/agent-tool.md`。

这套东西在运行时模型里的位置见 `CLAUDE.md`。

## codex 是怎么做的（调研结果）

队列在 `chatwidget/input_queue.rs::InputQueueState` 上，形如
`queued_user_messages: VecDeque<QueuedUserMessage>`（外加我们没有的 steer/slash
机制）。要紧的机制是：

- **Tab 是排队键**（`bottom_pane/chat_composer.rs` 约 3106 行）：composer 的
  `queue_keys` 是 `Tab`。回合进行中时，**Tab** 产出 `InputResult::Queued` →
  `queue_user_message_with_options`（推到 `queued_user_messages` 上），而
  **Enter**（`submit_keys`）产出 `InputResult::Submitted` →
  `submit_user_message`，那是*steer 正在运行的那一轮*而不是排队。空闲时，
  Tab 对草稿就像 Enter 一样提交（而对 `!` 感叹号草稿它会插入一个字面 tab）；
  只有在回合进行中且 composer 里有草稿时，页脚才显示 `Tab to queue message`
  （`footer.rs`）。
- **排队还是发送**（`input_flow.rs::queue_user_message_with_options`）：
  `if !is_session_configured() || is_user_turn_pending_or_running() {
  push_back } else { submit }`。
- **完成时抽取**（`input_flow.rs::maybe_send_next_queued_input`，由
  `turn_runtime.rs::on_task_complete` 调用）：从**队首**弹出，**每个完成只提交
  一条 `Plain` 消息**（它提交一条后就 `break`）—— 所以每条排队的消息都成为
  **它自己那个顺序回合**。（它的中断路径
  `merge_user_messages_with_history_record` 反而把积压合并成一个新回合。）
- **显示**（`bottom_pane/pending_input_preview.rs`）：一段暗色斜体的
  "Queued follow-up inputs"，每条以 `↳` 开头，并截断。
- **编辑/出队**（`chatwidget/interaction.rs` + `input_restore.rs`）：
  `edit_queued_message` 绑定（Alt+Up）把**最后一条**排队消息弹回 composer
  （`pop_back`），供编辑、重发或丢弃。
- **中断**（`interaction.rs` 约 115 行、
  `input_restore.rs::on_interrupted_turn`）：
  两个层次。*普通*排队消息在 Esc 时恢复回 composer。但一次 *steer* 走
  `submit_pending_steers_after_interrupt` 这条路：Esc 中断，并立刻把那次 steer
  作为一个新回合重新提交。

## 我们造的是什么

两个待处理集合，一套渲染，一条回收。

**Steering —— 正在运行的那一轮即将读取的东西。** `App::steered:
VecDeque<String>` 持有 Enter 交给在飞回合的那些消息。边界把它们逐个推到共享的
`steer::SteerQueue` 上 —— 一个由后端线程抽取的
`Arc<Mutex<Vec<String>>>` —— 而 `run_agent` 在**每个回合**开头取它，正是取后台
完成注记的那个位置，把每一条作为 user 角色消息追加在该回合 tool 结果之后。这次
取用以 `StreamEvent::Steered { text }` 宣告，而那个事件就是交接：循环丢掉待处理
行，把它之前流出的助手文本收尾（不变量 4），记录真正的用户消息，并计入该回合的
`↑` 总数。此后它就是对话的普通一部分 —— Ctrl+O 显示它，推导出的上下文携带
它，rollout 保留它，回溯可以回卷到它。

**后续回合 —— 经典的队列。** `App::queued: VecDeque<QueuedTurn>` 没变：带类型
的条目（codex 的带动作标记的 `QueuedInputAction`）按 FIFO 抽取，每个回合结束抽
一条。一个 `QueuedTurn::Messages { texts, images }` 是一个文本批次（发给模型，
它的 Ctrl+V 附件作为 `(placeholder, path)` 对随行 ——
`docs/input/image-paste.md`）；一个 `QueuedTurn::Shell(String)` 是一条独立的
`!` 命令（**在本地运行**）。这个变体就是派发的判别依据，所以提交顺序在混合条目
之间也得以保持。**Tab** 开一个新的 `Messages` 批次；**shell 模式的 `!` 草稿**
作为它自己的 `Shell` 条目排队，**绝不并入**。

**把两者接起来的正是那次回收。** 一轮可能还没走到下一个回合边界就结束了
—— 模型直接作答了。在*每个*回合结束处（`dispatch_after_turn`，所以
`StreamDone`、后端错误与两种 Esc 中断结局都走它）边界把共享队列取回，
`App::reclaim_steered` 把剩下的东西作为**一个批次**移到 `queued` 的**队首**：
它本就是提交给刚跑完的那一轮的，所以它排在一个本来就属于更晚的 Tab 后续回合之
前。`flush_next_queued` 随后把它作为下一轮派发，和旧队列一贯的做法完全一样。
这就是为什么什么都不丢，也是旧的批处理为什么作为兜底活了下来：三次 Enter 而这一
轮从未读到，就变成承载这三条的下一轮。

两种草稿搭不上回合边界，于是掉进后续队列，两者都在 `App::steer_draft` 里处理，
好让调用方只问一次：

- 一条 **shell 模式的 `!` 命令** —— 它在本地运行，按定义就是它自己那一轮；
- 一个带 **Ctrl+V 附件**的草稿 —— 边界注入是一条文本 `ChatMessage`，而图片需要
  一次真正的回合开始所打开的那条带类型通道
  （`docs/input/image-paste.md`）。

两种**回合**同样什么都不接收 —— `App::turn_steerable` 是
`is_streaming() && !is_compacting() && !status.shell` —— 它们的草稿就像以往一
样作为后续排队：一个 `!` **shell 回合**，因为那里没有东西在读对话；以及一个
**`/compact` 回合**，它的请求是针对被摘要的上下文那句固定交接 prompt，而不是一
场对话（把用户消息折进去会污染摘要，`docs/architecture/compact.md`）。

### 接缝（`steer.rs`、`llm/agent.rs`）

`steer::SteerQueue` 就是整个跨线程表面：`push`（循环）、`take`（回合边界）、
`take_last`（Alt+Up 的拉回 —— 一轮读过之后就返回 `None`，这是唯一诚实的答
案）、`is_empty`。它可 clone，每个 clone 都是同一个队列，所以每次后端重建
（`/model`、某个 `/settings` 旋钮）都重新挂上循环已经往里推的那一个 ——
`LlmBackend::with_steer`，与 `with_ask`/`with_tasks` 并列。

`run_agent` 的 `pending_notices` 闭包扩成 `pending_inputs`，返回带类型的
`PendingInput`：

- `PendingInput::Notice { seq, text }` —— 一个后台 shell 或 agent 结束了，或
  一个 session 停下来请求输入（`docs/tools/background.md`）。以
  `StreamEvent::NoticeDelivered { seq }` 宣告，于是循环把被扣住的注记单元格记
  在模型读到它的位置。（它过去是不可见的，循环在最先遇到的任何 tool 边界记录
  那个单元格 —— 于是模型正在写某个调用时落下的注记就被放到那个调用之前。）
- `PendingInput::User(String)` —— 用户排队的消息。以
  `StreamEvent::Steered` 宣告。

**注记在前，用户自己说的话收尾。** 一次完成是一个*结果*，属于它跟随的那些结果
之中；用户最新说的话必须是模型最后读到的东西。取用仍留在取消检查之后，所以一
个被放弃的回合既不会偷走欠给后续回合的注记，也不会偷走边界即将重新派发的消息。

### 状态（`app/queue.rs`）

- `App.steered: VecDeque<String>` —— 应用侧对共享队列的镜像：带上渲染的东西，
  以及在一轮未读它而结束时活下来的东西。
- `App.queued: VecDeque<QueuedTurn>` —— 后续回合。我们从不推入空条目，所以
  `is_empty()`/`len()` 数的就是条目，而 `queued[0]` 就是下一轮。
- `App::turn_steerable()` —— 现在提交的消息能进正在运行的那一轮吗？一次模型回
  合能；一个 `!` shell 回合不能。
- `App::steer_draft() -> Option<String>` —— Enter 的回合中途路径。吃掉
  composer，把文本记进 `input_history`（↑ 能像提交那样召回它），把它停在
  `steered` 里，并返回它交给 `Action::Steer` 推到共享队列上。上面那两种落空情
  形返回 `None`，它自己把它们路由给 `queue_draft`。
- `App::deliver_steered(text)` —— `StreamEvent::Steered` 那一半：冲刷流式段，
  丢掉待处理行，记录用户消息，计入 `↑` token。即使没有匹配的行也记录 —— 模型
  读到的，就是 transcript 欠用户的。
- `App::reclaim_steered()` —— 上面说的回合结束兜底。没有东西在等时什么都不做，
  所以永远不会凭空造出一个空批次。
- `App::steered_this_turn` —— 这一轮*读过*吗？Esc 中断的撤销从历史**尾部**判
  断：末尾是用户消息，说明这一轮仍是那次未被触碰的提交。一条已投递的消息会破坏
  这个读法 —— 即使在回合已经提交了回复和 tool 单元格之后，它仍在那里留着一条
  用户消息 —— 所以它让这一轮退出，与一条待处理消息并列。没有它，在投递与下一
  回合第一个 token 之间的空隙里按 Esc，会把模型已经读过的消息拉回 composer、从
  history 里删掉，并且不对屏幕上已有的输出提交任何通知
  （`docs/input/interrupt.md`）。
- `App::recall_steered(text)` / `App::recall_last_queued()` —— Alt+Up 的两半
  （见下文）。
- `App::queue_draft(new_batch: bool)` —— 后续那条路径。`new_batch` 决定语义：
  `true`（Tab）推入一个新的 `Messages` 批次，`false` 追加到最后一个（shell 回
  合里的 Enter 与被回收的批次就是这么走的）；空队列 —— 或队尾是一个 `Shell`
  条目 —— 两种情况都开一个新批次。
- `App::queue_shell()` —— 把 shell 模式草稿作为独立的 `QueuedTurn::Shell` 条
  目排队（codex 的 `submit_queued_shell_prompt`），退出该模式，为 ↑ 召回记录
  完整的 `!command`。**永远是新的条目。**
- `App::drain_next_batch()` / `drain_last_batch()` —— 弹出队首条目（循环的回合
  结束派发）／最后一个（Alt+Up）。
- **回合进行中按 Enter**（`on_key_conversation`）：`turn_steerable()` 时走
  `steer_draft()`，否则 `queue_draft(false)`。空闲 Enter 不变
  （`Action::Submit` / `Action::RunShell`）；裸 `/token` 保留 palette 那条路。
- **回合进行中按 Tab**（在 palette 的 Tab 分支之后，所以菜单仍优先）：
  `queue_draft(true)`。**空闲或空输入时 Tab 落空为一个 no-op。**
- **Alt+Up**（`KeyCode::Up` 配 `ALT`，composer **为空**）：**先看后续队列**
  —— 那才是刻意的积压，也是 Alt+Up 一直在编辑的那个 —— 把最后一个条目拉回
  composer（一个 `Messages` 批次以换行连接并把图片重新附上，一个 `Shell` 条目
  作为 `!command`，由 `recall_input` 重新吸收进 shell 模式）。那里空了之后，它
  经 `Action::ReclaimSteered` 去够正在运行的那一轮的消息：只有共享队列知道还有
  没有一条能被拿回来，所以边界去问它（`take_last`），并把任何答案交给
  `recall_steered`。以空 composer 为前提，所以它绝不覆盖草稿。

### 冲刷与回收（`tui/turn.rs`，I/O 边界）

`dispatch_after_turn` 在每个回合结束点都跑，所以它们不会走偏。它把共享队列取
回，调用 `reclaim_steered`，然后 `flush_next_queued`，后者弹出下一个条目并按种
类派发 —— 一个 `Messages` 批次经 `start_turn`（记录并提交每个用户气泡，打开
流，用换行连接的 prompt 启动后端），一个 `Shell` 条目经 `run_shell`。

- **`StreamDone` / `Error`**：回收，再冲刷 —— **Ctrl+O overlay 之下也一样**
  （codex 的队列在回合结束时抽取，不管它的 Ctrl+T 视图开着没有）。这不违反不
  变量 4：它记录历史并*排队*用户气泡；`term` 从不把待发行刷进备用屏幕，而
  overlay 返回时的绘制会把它们冲刷到 live 区域之上。
- **Esc 中断**：同一条路径（`Action::Interrupt` 分支最后落到
  `dispatch_after_turn`），所以被中断那一轮从未读到的消息会立刻作为下一轮发
  出 —— codex 的 `submit_pending_steers_after_interrupt`，这里是靠通则达成而
  不是特例。
- **`/clear`**：`App::clear_conversation` 把 `steered` 连同这一轮的其余部分一起
  丢掉，边界也把共享句柄抽空 —— 垂死的后端绝不能靠读取刚被抹掉那场对话里的消息
  来开启下一轮。

## subagent 的队列（`docs/agents/agent-tool.md`）

一个 agent session 视图就是同一幅画面下一层的样子，而且是刻意用同一份代码：

- `AgentRegistry` 本来就有逐 agent 的队列（`queue_input` /
  `take_pending_inputs`）；它如今喂给 `run_agent` 的 `pending_inputs` 接缝，
  所以 subagent 用 agent 通道上的 `StreamEvent::Steered` 宣告它的取用，与主会话
  在回复通道上的做法完全一样。
- `AgentRun::queued: Vec<String>` 是镜像，而 `AgentRun::apply` 的 `Steered`
  分支是投递（冲刷流式段，丢掉行，在**那个 agent 自己的** transcript 上
  `push_user_message`）。它是**已定局**的运行仍会折叠的唯一事件，会重新打开那
  个条目：正是下面那次续写投递了它，而少了这次重开，那次续写就会隐形地运行
  —— 模型在干活，transcript 什么都不显示，行也永远不清。被用户**停止**的运行
  保持关闭（`stopped_by_user`），它的 `interrupt()` 干脆丢掉这些行：一个被取消
  的循环什么都不会读，所以把它们留着就等于承诺一次不可能发生的投递。
- `ui::queued_lines` 在 agent 视图里渲染**被查看 agent 的**两个集合（它的
  steered 行作为一个不可分的块，排在它那些用空行分隔的 Tab 后续之上），在它之
  外则渲染主 session 的两个集合 —— 同样的行、同样的几何、同样的顺序。
- **做决定的是注册表，不是名单。** `ReplySource::spawn_agent_chat` 返回一个
  `AgentChatDelivery`：`Queued`（循环在跑 —— 把行停住，等回合边界）或
  `Started`（它空闲 —— 一次续写运行把消息作为它最新的用户回合携带，于是
  transcript 立刻记下它、气泡随之提交，等于下一层的一次空闲提交）。一个比注册
  表慢一个事件的名单状态，要么会把这一行永远搁浅，要么会把消息记两遍 —— 这正
  是那个按键分支不再做决定的原因。
- **运行自己的线程在 `finish` 处做回收**，**落定窗口**就在这里：一个槽位在它
  循环最后一次抽取与 `finish` 之间保持 `busy`，所以那期间打进的消息会被
  `queue_input` 接收，然后没人读 —— 循环已经结束，而名单的终态事件可能已经被
  折叠了。`AgentRegistry::has_pending_inputs` 是那道守卫，线程在退出前自己启动
  续写。反方向抢跑的消息会看到 `busy == false`，走 `spawn_agent_chat` 里的续写
  路径，所以两者之间没有任何东西被搁浅；一个**被杀掉**的 agent 什么都不续写。

  在源头而不是在边界回收是刻意的。在终态事件上做边界清扫根本看不到这个窗口
  —— 事件在 `finish` 落地*之前*就被折叠了 —— 而两者都跑会形成竞态：清扫的
  `take_pending_inputs` 可能在 `has_pending_inputs` 读到之前一瞬间把队列抽空，
  结果消息被重新排进一个再没人抽取的槽位。

这一侧修掉的 bug 是：视图在消息被敲下的瞬间就把它记下来，宣称 agent 读到了它
没读过的东西，而且完全不显示待处理状态。

### ……以及 Tab，它的后续回合

Enter 是移植过来的那一半；**Tab 没有**，而这个遗漏并非无害。
`on_key_conversation` 的 Tab 分支读 `App::is_streaming()` 并调
`App::queue_draft` —— **lead 的**流和**lead 的**积压 —— 自己没
有 `agent_view` 分支，而 Enter 分支是先有一个。于是在某个 subagent 的 session
里用 Tab 排队的消息，跑到了主 session 的队列上：它哪里都不显示（agent 视图只渲
染那个 agent 的行，别的一概不渲染），而当 *lead 的*回合结束时，它作为**主**对话
的后续回合运行了。注意到之前打了好几条，就排了好几个主对话的回合。

所以 Tab 在下一层有它自己的集合，正像 Enter 那样：

- `AgentRun::followups: VecDeque<String>` 是 `App::queued` 的孪生 —— 每个后
  续回合一个条目，永远是它自己的（Tab 的意思是"单独一轮"；这里没有 Enter 的追
  加，因为 Enter 是 steer；也没有 `Shell`/图片变体，因为两者都搭不上一次聊天
  续写）。
- `App::queue_agent_draft` 是 `queue_draft` 的孪生：吃掉 composer，为 ↑ 召回
  记录文本，把它停在**被查看的** agent 上。它的按键分支坐在主分支**之前**，而
  主分支如今排除这个视图，所以谁都够不到对方的队列。对一个**已定局**的 agent
  按 Tab，是保住草稿的 no-op —— 主 session 空闲 Tab 的规则，下一层。
- `App::take_agent_followup(id)` 是 `drain_next_batch` 的孪生，而
  `Session::dispatch_agent_followups` 是 `flush_next_queued` 的：每次落定一
  个条目，按提交顺序，各自经既有的 `agent_chat` 接缝启动自己的聊天续写 ——
  于是 transcript 记录、已提交的气泡、回合回执与 rollout 全都免费得来。
- `App::recall_agent_pending` 是 Alt+Up 两个分支合一：那个 agent 的最后一个后
  续（`recall_last_agent_followup`，`recall_last_queued` 的孪生），以及那里空
  了之后它循环尚未读到的消息 —— `Action::ReclaimAgentChat` →
  `AgentRegistry::take_last_input`（`SteerQueue::take_last` 的孪生，回合边界
  抽过之后返回 `None`）→ `App::recall_agent_chat`。它只接收**用户**打的那条
  消息：lead 用 `agentsend` 排的那条既没有行、也没有可回的 composer，所以接收
  它只会把它悄悄弄丢（`docs/agents/agent-tools.md`）。它绝不落向主 session 的
  积压：那属于用户没有在看的另一场对话。
- `AgentRun::interrupt` 把后续连同 steered 行一起丢掉：每一条都会续写 `x` 刚
  取消的那次运行。注册表在同一次停止（`kill`，以及 lead 的 `stop`）里丢掉那些
  行背后的消息 —— 留着的话，它们会不经宣告地搭上下一次续写，用户的消
  息就这样以它自己的身份到达 lead。

**派发问的是注册表，不是名单** —— `AgentChatDelivery` 已经陈述过的规则，理由
相同。`ReplySource::agent_ready_for_turn` → `AgentRegistry::ready_for_turn`
回答"已定局、未停止、可恢复"，而 `dispatch_agent_followups`（从名单 tick 跑，
在 linger 清扫之前）在它说 yes 之前什么都不交出去。这就是让后续成为它**自己**
那一轮的东西：在落定窗口之内交出去，它会被 `queue_input` 接收并折进 agent 自己
的线程为它最后一轮未读消息启动的那次续写 —— 投递是投递了，但是作为一次 steer，
那是另一个键的含义。

**subagent 启动的后台 shell 的完成注记走同一条接缝**
（`docs/tools/background.md`）：`tui/background.rs` 用 `queue_input` 把它排队，
到此为止。同时也把它记到 transcript 上 —— 即既走接缝*又*走 `App::agent_chat`
以前的做法 —— 会让它出现**两次**，一次是急切记录、一次是回显记录，而且永久
地：在 history、rollout、上下文重放与每次重建里。只允许一条路径在 agent 的
transcript 上记录 user 角色消息，而具体是哪一条取决于有没有回显要来：把消息装
在自己请求里的续写走 `agent_chat`，搭队列的一切走 `Steered` 回显。

### 显示（`ui/footer.rs`）

待处理消息渲染在**框上方、流式带里** —— 就堆在状态行间隔之下，在它与框的上横
线之间 —— 每行**缩进两列**（`QUEUED_INDENT`）。缩进之后，文本消息样式得**与已
发送的用户消息完全一致**（`❯ ` 圆点、深色背景、换行），**shell 命令**则像它变
成的那个 exec 单元格（红色的 `! ` `Role::Shell` 表头）。进**正在运行的**那一轮
的东西在前 —— 它是接下来会发生的事 —— 而**空行把每一轮与下一轮分开**，所以
Tab 打开的后续与独立的 `!` 命令读起来是分开的回合：

```
● Happy to help!…            ← streaming preview
( ●    ) Working… (…)         ← status line

  ❯ Hello                     ← handed to the RUNNING turn (Enter): two-space
  ❯ World                       inset, user-style; gone at its next round
                                boundary, where each becomes a real bubble
                              ← blank: a turn boundary
  ! ls -la                    ← a Shell entry (Enter in shell mode): red `! `,
                                runs locally as its own turn
────────────────────────────
❯                             ← the input box
────────────────────────────
```

空闲时这条带的高度收缩为 0，而两个集合只在回合进行时才非空，所以这些行天然住在
带里。`queued_rows` / `queued_lines`（贯穿 `live_height`/`live_layout`/
`input_box`）：

- `queued_lines(app, width)` —— 显示**谁的**消息取决于屏幕上哪场对话，两种情
  形行都相同。主视图先渲染 `App::steered`（一个以空行分隔的块：模型会在它下一
  个回合边界一起读它们），然后渲染每个 `queued` 条目 —— 一个 `Messages` 批次
  的消息经 `message_lines(Role::User, …)`，一个 `Shell` 条目经
  `message_lines(Role::Shell, …)`（红色的 `! ` 表头）。**agent session 视图**
  只渲染那个 agent 的 `queued`，别的都不渲染：视图就是 agent 的世界，主会话的
  行在返回时重新出现。每一行都以 `QUEUED_INDENT` 为前缀（`indent_queued_line`
  把缩进留在深色块之外）；**不设上限**（整份积压都显示）。
- `queued_rows(app, width)` —— `queued_lines(...).len()`，所以带预留的正是
  `render_live` 画出的东西（两者不会走偏）。`live_height` 的终端高度夹取仍会
  约束区域。

**两者都经过一次 memo**，而这是一项修复而不是装饰。每帧绘制会**触及这些行六七
次** —— `live_height` 与 `preview_budget` 的行数求和、`cursor_position` 的落
座、`render_live` 的布局，以及它的绘制 —— 而每次构建都会把*整份*积压按词换行
并配样式。回合进行时绘制 tick 每 32 ms 重新挂一次，所以一个不断增长的队列每秒
会把自上次按键以来并未变化的行重渲约两百次。在一个卡住、排了五十条约 530 字符
消息的回合上实测：**五秒空闲内 75 个 CPU tick，基线是空队列的 6 个 tick**
—— 一成半的核心耗在静止不动的屏幕上，这正是"一边有消息一边反复按 Tab，TUI 就
卡"的来由。`with_queued_lines` 每帧构建一次，从那个条目服务所有调用方：同一负
载下**13 个 tick**，即队列自身的代价从 69 降到 7。键是这次构建所读的一切的指
纹 —— 宽度、屏幕上哪场对话，以及那场对话两个待处理集合的确切文本 —— **刻意由
内容推导**：这些队列是若干模块与整棵测试树直接写写的普通字段，所以一个变更计数
器只要漏一次 bump，就会画出已经不在那儿了的行。`queued_builds` 计数器是测试钩
子，因为这里的性质是一种*代价*，而代价只有数出来才钉得住。

**Ctrl+O transcript 视图也显示积压**：`ui::transcript_lines` 在实时尾部之后追
加 `queued_lines` —— 同样的缩进行，读起来就是进行中回复下方的"待处理、尚未读
取" —— 所以打开 overlay 绝不会藏起一条待处理消息，而回合边界会在观看者眼前把
它变成一条真正的 transcript 用户条目。

`tab to queue next turn` 这条绑定列在 `?` 快捷键带里（旁边是
`alt+↑ to edit queue`，`docs/ui/shortcuts.md`）。

## 与 codex 的已知分歧

- **steer 落在回合边界，而不是立刻。** codex 注入它正在运行的任务；我们的搭
  `run_agent` 每个回合的 `pending_inputs` 取用，所以消息在本回合 tool 结果之后
  才被读到，而不是在请求中途。实践上那也只差一个 tool call，而这是 Chat
  Completions 请求能诚实地多出一条消息的唯一时点。
- **在那之前，被 steer 的消息明确显示为待处理。** codex 的 steer 立刻变成一条
  transcript 消息；我们的在框上方作为缩进的一行等着，等
  `StreamEvent::Steered` 说模型真的拿到它时才提交。宣称模型读到了它没读过的东
  西，正是这套设计要避免的 bug —— 也正是 subagent 视图以前干的事。
- **Enter 的批处理作为兜底活了下来，而不是规则。** 一轮从未读到的连续 Enter 被
  回收成**一个**下一轮（Claude-Code 的合并），而 codex 的队列让每条消息都成为
  自己的回合。一条确实读到它们的回合会按顺序把它们拿到为分开的用户消息，那是
  codex 的形状。
- **我们保留红色的中断通知。** codex 的 steer 路径显示一条温和的信息；我们保留
  标准的 `Conversation interrupted` 通知，然后发出这一轮从未读到的那些 —— 中断
  确实发生了。
- **斜杠命令不排队。** 裸 `/token` 经 palette 就地运行；只有纯文本才排队。
  （palette 自己的 Tab 切它的标签页、Enter 运行那一行 —— 两个菜单分支都优先于
  排队的 Tab。）
- **空闲 Tab 是 no-op。** codex 的空闲 Tab 像 Enter 一样提交（并在 `!` 感叹号
  草稿里插入一个 tab）；我们的什么都不做 —— Tab 只对正在运行的回合排队，那正是
  用户要的。在 agent session 视图里，"正在运行"指的是*那个 agent 的*循环，所以
  在已定局的 agent 旁边按 Tab 是同样的 no-op。
- **subagent 的后续是单条消息，不是一个批次。** `App::queued` 的条目可以装好几
  条（回合无法被 steer 时连续 Enter 会追加，一次回收会合并未读的那些）；agent
  的不行，因为下一层那两条路径都不存在 —— 那里的 Enter 永远 steer，而未被读的
  消息由运行自己的线程回收。所以 `AgentRun::followups` 是一个
  `VecDeque<String>`，每个 Tab 一个条目。
- **Alt+Up 把最后一个条目（像 codex 那样）恢复成可编辑草稿。** codex 的
  `edit_queued_message` 弹出最近排队的条目；我们弹出最近的**条目**
  （`pop_back`）—— 一个 `Messages` 批次以换行连接进 composer（一个批次可以装好
  几条，而 codex 的条目只有一条），一个 `Shell` 条目作为 `!command` 重新进入
  shell 模式 —— 更早的条目留在队列里。后续没有了之后，它去够正在运行那一轮自己
  的消息，而 codex 没有对应物：那里的 steer 一提交就没了，我们的则还能被拿回
  来，直到这一轮读到它。**在 agent session 视图里，就是对那个 agent 的两个集合
  走同样两步**，并到此为止 —— 主 session 的积压是另一场对话的。
- **回合中途的 `!` 命令如今与 codex 一致（本地运行，不当作文本排队）。** 回合
  流式输出时提交的 shell 命令作为它自己的 `Shell` 条目排队，轮到它时本地运行
  —— codex 的带动作标记的 `RunShell` 派发。这退掉了 v1 的限制
  （`docs/input/shell-command.md`）：那时回合中途的 `!command` 作为字面文本排
  队并被发给后端。每条 `!` 命令都是**独立**条目（绝不并入文本批次），所以用户
  "各自分开的队列"这个意图与 codex 的逐完成派发正好重合。
- **没有逐队列的编辑提示行。** codex 在它的队列列表下方显示一行暗色提示；我们
  不为它花掉带里的一行 —— 绑定住在 `?` 快捷键带里（`alt+↑ to edit queue`、
  `tab to queue next turn`）。
- **退出会丢掉两个集合。** 它们只存在于回合中途；那里的 Ctrl+C 会退出
  （composer 是空的），一切待处理的东西随 session 一起丢弃。

## 测试

- `steer`：推入的消息只被取用一次，按提交顺序；每个 clone 都是同一个队列；
  `take_last` 只回收一条尚未投递的消息。
- `llm::agent`：被 steer 的消息落在**下一回合的**上下文里，在该回合 tool 结果
  之后，并以 `StreamEvent::Steered` 宣告；同一回合里一条后台注记在它之前，且
  **不**被宣告（它不是用户气泡）。
- `app`（steering）：回合中途 Enter 返回 `Action::Steer` 并把文本停在
  `steered` 里而不是 `queued`；连续 Enter 保持提交顺序；文本为 ↑ 召回被记录；
  `deliver_steered` 丢掉那一行并记录用户消息，**并把它之前流出的回复收尾**
  （history 里助手在前、用户在后）；`!` shell 回合里的 Enter 仍排一个后续；带附
  件的草稿排队而不是 steer；回收在队列**队首**造出一个批次，排在 Tab 后续之
  前，空时什么都不造；Alt+Up 优先后续队列，只有之后才去问边界
  （`Action::ReclaimSteered`），由 `recall_steered` 把文本送回 composer；回合
  中途 `/clear` 会丢掉它们；一个 **`/compact` 回合什么都不接收**
  （那里 `turn_steerable` 为 false，所以草稿作为后续排队）；而一轮从未读到的
  消息**会挡住中断撤销** —— 中断正是发出它的原因，所以提交不能在同一时刻被拉
  回（`docs/input/interrupt.md`）。
- `app`（后续队列）：**回合中途 Tab 开一个新批次**（composer 被吃掉，第二个批
  次加入）且绝不 steer；批次一次抽一个回合（`drain_next_batch` 先给第一个，再给
  后续）；**空闲 Tab 与回合中途的空输入 Tab 都是 no-op**；空闲 Enter 仍提交；
  **Alt+Up 只把最后一个批次**以换行连接拉进 composer，更早的批次留在队列里
  （多消息批次会被连起来）；有草稿时 Alt+Up 是 no-op；回合中途 `/clear` 丢掉积
  压。
- `app`（回合中途 shell，`docs/input/shell-command.md`）：回合中途的
  `!command` 作为独立的 `QueuedTurn::Shell` 条目排队（不是文本），为 ↑ 召回记
  录 `!command` 并退出该模式；`Shell` 条目**绝不合并** —— 它之后的 Tab 文本开
  一个新 `Messages` 批次，两条 `!` 命令排成两个独立的 `Shell` 条目；**对
  `Shell` 条目按 Alt+Up 会重新进入 shell 模式**，命令留在 composer 里。
- `agents`（它的回收）：`take_last_input` 只交回回合边界尚未读到的消息，未知
  id 什么都不给。
- `agents`（subagent 自己的队列）：排进运行中 agent 的消息在它的行上等待，且
  在它的 transcript 上**什么都不**宣称；它的回合边界
  （`StreamEvent::Steered`）在那里把它变成一条用户消息，在它之前流出的段收尾
  之后；一个已定局的运行把未读消息交回一次；它的**后续**最旧优先抽取，而
  Alt+Up 取最新的，一次**停止**把它们连同 steered 行一起丢掉，而
  `AgentRegistry::ready_for_turn` 只对一个已定局、未停止、可恢复的 agent 为
  真（未知 id 什么都不取）。
- `app`（agent 视图的 Tab）：那里的 Tab 在**那个 agent** 上排一个后续，并让主
  session 的两个集合保持为空，同时 bump `agents_generation`；每个 Tab 按顺序开
  自己的条目；对**已定局** agent 按 Tab 是保住草稿的 no-op，且仍绝不碰主队列；
  `take_agent_followup` 每次落定抽一个；**Alt+Up 优先那个 agent 的后续队列**，
  只有之后才向边界要它未读的 steered 消息（`Action::ReclaimAgentChat`，它的答
  案由 `recall_agent_chat` 放回 composer 并从它的行里删掉），绝不落到主积压。
- `app`（agent 队列）：`queue_agent_chat` 停住那一行并 bump
  `agents_generation`（transcript 缓存会被通知）；回合边界把它落在**那个
  agent 的** transcript 上，绝不到主对话；`reclaim_agent_chat` 把未读的那些交
  回。
- `stream::dummy`：离线后端在**整个 tool 批次**解析之后（不在它两次调用之间，
  真实循环从不这么做，也不在回合结束时）取一条排队的消息并抽空队列；
  `spawn_agent_chat` 把消息排进运行中的 agent，没有挂注册表时则拒绝。
- `ui`（memo）：一次构建服务一整帧的六七个调用方，一条排队消息会重建，新宽度也
  会；而 memo **跟随 agent session 视图** —— 同样的队列按屏幕上是谁的 session
  渲染成不同的页面，返回时又服务主行。
- `ui`（队列）：`queued_rows` 空时为 0 / 数一个批次的消息 / 数一条长消息换行后
  的行 / **数批次之间的空行**；`queued_lines` 把每条消息样式得像用户消息，
  **用一行空行分隔回合**，把 `Shell` 条目渲染成红色 `! ` 的 `Role::Shell` 表
  头，把 steered 消息显示在后续**之前**，而在 **agent session 视图**里只显示
  那个 agent 的队列、不显示主 session 的任何一行 —— 它的 steered 行排在它那些
  用空行分隔的 Tab 后续之前，也就是主视图的形状；`live_height` 随队列增长；
  `render_live` 把它画在框上方；`transcript_lines` 在实时尾部之后以同样的缩进
  样式列出积压，**包括交给正在运行那一轮的消息**（缓存签名同时数两个集合，所以
  排一条消息会刷新一个否则会冻住的页面），而 `agent_transcript_lines` 对被查看
  agent 自己的队列也一样 —— Ctrl+O 视图从不藏起待处理消息。
- `scripts/smoke.sh` Phase 12（回合中途投递）：提交 `hello there`，在流式中途
  **用 Enter** 排 `world` 与 `again`（两行缩进行都显示），随后两者都在**状态
  行还挂着时**于第 0 列提交，这一轮带着它自己的摘要结束 —— 恰好**一条**摘要
  行：第二个回合**不得**运行。
- `scripts/smoke.sh` Phase 13（中断后发送）：提交 `hello`，排 `world`，按 Esc
  —— `Conversation interrupted` 提交，被中断那一轮的回执在它下面
  （`docs/input/interrupt.md`），而这一轮从未读到的内容立刻发出，在那份回执之
  下。
- `scripts/smoke.sh` Phase 14（Alt+Up 恢复）：**用 Enter** 排 `world`（进正在
  运行的那一轮）、**用 Tab** 排 `again`（一个后续），按 Alt+Up —— 只有 `again`
  作为草稿回到框里，而 `world` 保留它的缩进行。
- `scripts/smoke.sh` Phase 21（Tab 后续）：提交 `hello`，在流式中途**先**用
  Enter 排 `world` **再**用 Tab 排 `later`（一行空行把它们分开）；`world` 被
  正在运行的那一轮读到，而 `later` 在它之后作为**单独**一轮运行（第二条回合摘
  要）—— 即全用 Enter 的 Phase 12 从不产生的那一轮。
- `scripts/smoke.sh` Phase 24（回合中途 shell 队列）：提交 `hello there`，在流
  式中途排 `world`（Enter）与 `!echo smoke_queue_ok`（shell 模式下 Enter）
  —— 两者都缩进显示（`  ❯ world`、`  ! echo smoke_queue_ok`）；随后正在运行的
  那一轮读到 `world`，而这条命令作为它自己那一轮**在本地**运行，提交 exec 单元
  格（`! echo …` 表头 + `⎿ smoke_queue_ok`），绝不是一条 `❯ !echo …` 用户消
  息。
- `scripts/smoke.sh` Phase 29（overlay 之下的队列）：提交 `hello there`，在流
  式中途排 `world`，按 Ctrl+O —— 缩进的 `  ❯ world` 行在 transcript 视图里显
  示；这一轮*在 overlay 之下*于它的回合边界取用它（第 0 列的 `❯ world` 条目
  出现，这一轮一路跑到它的 `Done for` 摘要，overlay 仍开着），而 Ctrl+O 返回
  会把这一切冲刷进内联视图。
- `scripts/smoke.sh` Phase 101（subagent 的 **Tab** 队列）：在演示 subagent 工
  作时打开它的 session 并用 Tab 排两条消息 —— 两者都在框上方缩进等待，随后在它
  的循环落定后，各自按顺序在那个 agent 的 transcript 上作为自己的续写回合运
  行，而 Esc 返回在主对话里看不出它们的任何痕迹（那里正是它们过去跑的地方）。
- `scripts/smoke.sh` Phase 96（subagent 的队列）：在演示 subagent 工作时打开它
  的 session 并打一条消息 —— 它在框上方缩进等待，它的循环在自己的回合边界（它
  的 `write` 批次之后）取用它，并在 **agent 仍在运行**时于*那个 agent 的*
  transcript 上提交，而 Esc 返回在主对话里看不出它的任何痕迹。
