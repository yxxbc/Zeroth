# 后台 shell —— `wait: 0`、一次等到头的 `wait`、Ctrl+B，以及 ↓ 管理器

> 自 `docs/tools/bash-tools.md` 起，一次 `bash` 调用改用 `wait: 0`（或结尾的
> `&`）来请求后台，而不再设 `run_in_background`（后者仍然能解析）；一条在
> `wait` 到点时仍在运行的命令也会到这里来，而不是被杀掉，而配套工具
> （`bashsend`/`bashwait`/`bashkill`/`bashlist`）接手了 `bash_session` 以前
> 干的活。下面的机制没有变化。

Claude Code 风格的后台命令执行：模型可以启动一条 `bash` 命令，让它在对话继续
的同时一直跑；用户可以用 **Ctrl+B** 把一条正在跑的命令移到后台（没有可移后台的
命令在跑时，Ctrl+B 就是终端的「光标左移」—— `docs/input/textarea.md`）；而一个
**↓ 管理器条带**（从空的 composer 到达 —— ↓ 点亮页脚的 shell 指示符，Enter
打开条带）列出正在跑的 shell，实时流出一条选中 shell 的输出，并且能停掉一个。
一个后台 shell 结束时，反馈是**即时的** —— 在回合中途，它面向模型的注记被贴到
注册表的公告板上（在飞的 agent 每轮开头取走公告板，所以模型刚 `kill` 掉的
shell —— 或者用户刚用 `x` 停掉的 —— 会在*同一个回合内*被它知道，就在做这事的
那条 tool 结果之后），而通知 cell 提交在**模型读到它的地方** —— 也就是 agent
宣告它取走那条注记的轮次边界（`StreamEvent::NoticeDelivered`），不是随后来到的
第一个 tool 边界，也不是回合遥远的结尾；而在空闲时，通知立刻提交，并且 —— 对
一个模型启动、没有 agent 读过它注记的 shell —— 模型会在一个新的回合里自动被告知
结果。

## 各个部件

### 协议

- `llm::tools::ToolOutcome` 多一个 `background: Option<String>`（task id）。
  `bash` 参数多 `run_in_background: Option<bool>` 和 `description:
  Option<String>`；tool schema 两个都宣传。
- 一次被移入后台的调用以
  `StreamEvent::ToolBackgrounded {id, output}` 解析，**而不是** `ToolEnd`
  （`llm::agent` 按 `outcome.background` 挑）。`output` 是*面向模型*的文本 ——
  也就是 **session id**、临时输出文件路径和完成承诺。这个 id 以前是省掉的，因为
  没有任何面向模型的東西接过它；现在配套工具接了 —— `bashwait` 等一条后台命令，
  `bashsend` 用 `<C-c>` 打断它，`bashkill` 结束它（`docs/tools/bash-tools.md`）
  —— 所以启动文本点名它。cell 从不显示这段文本 —— 它渲染固定的
  `⎿ Running in the background (↓ to manage)` 行。
- 这段文本按**是谁**把调用移入后台而不同。一次 `run_in_background` 启动拿到的是
  朴素的确认（`exec::background_launch_text` —— 模型自己要求的，所以临时路径 +
  完成承诺就够了）。一次 **Ctrl+B 移交**拿到的是
  `exec::background_handoff_text`：同样的事实，但以 `The user moved this command
  to the background …` 打头，并以一句「不要重跑、不要轮询」的引导收尾 —— 模型
  要求的是*前台*运行，而在没被告知用户把它移走后，它会期待完整输出，一轮又一轮
  地重读那个临时文件等它。记录下来的 `tool.output` 就是这段同样的文本，所以
  `context::context_messages` 会把这段解释重放进之后每个回合的上下文。两者都以
  **模型**身上会发生什么收尾 —— *you will be notified with the final output
  when it finishes* —— 而不是以 harness 自己的重新调用来收尾：「你会被重新调用」
  点的是一个模型对它无能为力的机制，读起来像一场要提防的中断，而不是一个要等的
  结果。`agent` 工具的启动文本（`backend::agent_launch_text`）用同样的话承诺它，
  于是一套词汇覆盖两种移入后台的方式。（`bash` 的描述仍然把启动事实隔在外 ——
  它说明这个 flag 做什么，并点出用到它的一个场景，一个 dev server 或一整套测试
  —— 而移交文本自己带着指令，在它们要紧的那一刻。）

- `ToolStatus::Backgrounded` 是解析后的状态：绿色头部圆点，固定行作为它的正文，
  内联、在预览里、在 Ctrl+O transcript 里都是。推导上下文里的 wire/tool 结果内容
  仍然是 `tool.output`（`context::context_messages` 不需要特例）。
- **subagent 也能移后台**（`docs/agents/agent-tool.md`）：subagent 的执行器携带
  同一个注册表，通过 `launch_from` 启动，带一个
  `BgOrigin { agent_id, agent_type }`，好让这个 shell 处处都被归属 —— 详情页的
  `From: {type} agent` 字段、通知的 `· from the {type} agent` 后缀、上下文注记的
  `launched by the {type} agent` 从句（持久化在通知记录上；缺席时省略，好让旧
  rollout 能解析）。**Ctrl+B 的闩锁仍然只属于主 agent**：subagent 的 `bash` 既不
  清除也不消费它。一个 subagent 启动的 shell，其完成注记先路由给它的启动者
  （`AgentRegistry::route_shell_note` 投到那个 agent 的 steer 接缝上，
  `docs/input/queue.md` —— 带着这次退出的认领细节，好让那个 agent 自己对已结束
  session 的调用能在那里认领它，`docs/tools/bash-tools.md` *One notice per
  exit*）；一个已经沉降的启动者听不到它，于是注记落到共享公告板上 —— 也就是正常
  的主回合 / 后续回合路径。边界做的事**全部**就是排队：
  `StreamEvent::Steered` 回显在 agent 的循环真的取走它时，把这条注记记录到该
  agent 的 transcript 上。同时也提前记录，会让它在那里出现两次，永久地 —— 在
  历史里、在 rollout 里、在每次重建里。

### 注册表（`src/background.rs` —— 边界，和 `term` 一样）

`BackgroundRegistry` 是一个可克隆的句柄，由事件循环、执行器（`llm::exec`）和
`!` shell 运行器共享：

- task id 是 **Claude-Code 风格**的 —— 一个 `b` 前缀 + 8 个小写 base36 字符
  （`bvyo7tkbe`），每次启动从一个 splitmix64 混合过的熵种子（纳秒 ⊕ pid ⊕ 一个
  启动计数器 —— `tui::host::session_id` 那套不用 `rand` 的模式）里滚出来，并在
  与在跑集合冲突时重滚。纯的 `task_id(seed)` 钉住格式。
- 临时文件住在 session 自己的临时树里（`docs/tools/scratchpad.md`）—— 纯的
  `scratchpad::tasks_dir(session_root)`：
  `{tmp}/zeroth-{uid}/{session}/tasks/{id}.output`（例如
  `/tmp/zeroth-0/18f…-4e2/tasks/bvyo7tkbe.output`）—— 一个稳定的按用户划分的
  根（Claude Code 的 `claude-{uid}` 模式）加一个按 session 划分的目录，把并发实
  例隔开，而 `tasks` 这一层把这类文件与旁边的 agent `scratchpad/` 分开。长度刻意
  短：模型会从每一段启动文本里把这些路径读回去，而 session id 已经把项目分开了，
  再加一段 cwd 只会是纯粹的长度。边界注入 uid（`tui::host::process_uid` —— 经
  rustix 的安全 `geteuid` 取有效 uid；一个`forbid(unsafe)` 的 crate 里没有
  `libc`）和 session id。

- `launch(command, description, from_model)` 在它自己的进程组里 spawn `sh -c`，
  **与控制终端脱离**（`subprocess::spawn_detached_shell` 加注册表的 detach 辅助
  函数 —— 一个 `/dev/tty` 密码提示会快速失败，而不是把任务楔住；注册表还把这个
  辅助函数*带*给执行器和 `!` 运行器，`docs/tools/tools.md`），外加一个**监视
  线程**，它按到达顺序合并 stdout/stderr，把完整的行作为 `BgEvent::Output` 流出
  —— **折叠**成终端显示它们的样子（`pty::fold`：一个 `\r` 进度条到达一次、是
  完成的，颜色转义永远不出现，于是管理器和完成注记读到的是文本；那行没写完的在
  退出时到达）—— 把每个字节按原样 tee 到 `{tasks_dir}/{id}.output`（好让模型能
  `read` 临时输出），并在子进程死亡时发 `BgEvent::Exited {code, killed}`。
  `BgEvent::Started {id, command, description, from_model}` 提前发上去。
- `adopt(child, chunk_rx, prior, …)` 就是那次 **Ctrl+B 转交**：一次前台 `bash`
  （或 `!` shell）运行把它的子进程 + 管道通道 + 已读到的输出 —— 读到的那些字节
  —— 中途交给一个新的监视线程，它把 `prior` 当作第一个 chunk 并进同一个折叠，
  于是一条被从行中间打断的进度条最后仍合并成一行。
- `request_background()` / `take_background_request()` 是**全局**的 Ctrl+B 闩锁
  （一个 agent 组的等待循环、`!` shell）；`request_background_for(id)` /
  `take_background_request_for(id)` 是**定向**形式。当一个命令 cell 正在跑时，
  TUI 抬起的是定向那个：`App::background_target` 点名第一个正在跑的 `Bash`/
  session 等待 cell，于是一个并行批次把那次调用移入后台，而它兄弟们的运行器不去
  动闩锁。每个运行器在一条新命令开始时仍然会清掉任何陈旧请求，所以一次错过某条
  命令的按键不可能把下一条移入后台。全局抬起的请求也会被定向运行器取走。
- `kill(id)` / `kill_all()` 直接向进程**组**发 `SIGKILL`（同步地 —— 退出不能
  依赖监视线程的调度），在此之前先把任务标记为 killed，好让监视器报告
  `killed: true`。
- **完成公告板**（`PendingNotice { seq, context, from_model, agent, shell }`、
  `post_exit_notice`/`post_waiting_notice`/`post_agent_notice`/
  `take_pending_notices`）：事件循环在处理 `Exited` 事件的那一刻，把结束的
  shell 的面向模型注记（`BgCompletion::context_text` —— 与沉降后的通知在未来
  上下文里重放的是同一段文本）贴上去。每条注记在贴上去时**编号**（`seq`），并
  打上它报告的那个 shell（或 agent）的标签。两个消费者竞争，各自恰好取走一条：
  - **在飞的 agent** —— `llm::agent::run_agent` 在每一轮开头取走公告板（在它的
    取消检查之后，于是一个被放弃的回合偷不走注记），并把每条注记作为用户角色
    消息追加在上一轮 tool 结果之后，于是模型就在它自己跑的那次 `kill` 的 tool
    结果之后、在同一个回合内，听到「was terminated by a signal」—— 并且用
    `StreamEvent::NoticeDelivered { seq }` **宣告每次取走**，那是 `Steered` 回显
    的孪生体，循环正是在那里记录这条注记被扣住的 cell（见下文 *通知落在哪里*）；
  - **回合边界的派发**（`tui::turn::Session::dispatch_after_turn`）—— 回合结束
    时仍留在公告板上的注记，从没被任何模型听过：模型启动的那种（且队列为空）会
    启动自动的后续回合；一个已经读过该注记的 agent 不再欠一个后续回合，所以一次
    刻意的回合中途 kill 不再多触发一个多余的回合。
  `/clear` 会连同 `kill_all()` 一起丢掉公告板（`clear_notices` —— 被清掉的对话
  不欠任何幽灵后续回合）。
- **一次报告可以把一条注记拿回来**，在任何模型读到它之前。一个点名刚**退出**的
  session 的配套调用 —— 它在模型写这次调用时结束了 —— 自己报告这次退出并认领它的
  注记（`claim_exit`）；一个**看**正在跑的 session 的调用，会把该 session 的
  `… is waiting for input` 注记收回（`retract_shell_notices`）—— 各自只拿调用方
  自己的那条：lead 的从公告板上，subagent 的从它的队列里，后者被路由的注记从同一
  个计数里编号；一个报告 agent 回答的 `agentoutput` 会收回那个 agent 的完成注记
  （`retract_agent_notice`）。注册表记住每个被收回的编号（`take_retracted`），而
  循环丢掉它为其中一个扣住的 cell —— 按编号，所以一次收回只可能让它拿回的那条注
  记付出代价（`docs/tools/bash-tools.md` *One notice per exit*、
  `docs/agents/agent-tools.md` *One notice per answer*）。

事件走一条专门的 tokio 通道（`BgEvent`）—— 第六个 `select!` 源 —— 因为后台 shell
活过回合：回复通道在每次打断/`/clear` 时被换掉，而这些事件必须活过那次更换。

### 纯状态（`app`）

- `App::background: Vec<BackgroundShell>` —— 正在跑的 shell（退出的 shell 离开
  列表；它的注记就是记录）。每个都保有一份尾部截断的输出缓冲区，供详情视图和完成
  通知用；`runtime` 每帧由边界注入（`set_status_times` 那套 —— 时钟住在
  `main.rs`）。
- `bg_started` / `bg_output` / `bg_exited` 应用 `BgEvent`。`bg_exited` 返回一个
  `BgCompletion`；循环立刻把它的 `context_text()` 贴到注册表公告板上，并把这个
  完成连同注记的编号一起**扣住**（`pending_bg: VecDeque<Held<BgCompletion>>`、
  `defer_bg_completion`），直到模型读到它 —— 见*通知落在哪里*。
  `BgCompletion::context_text` 与沉降后的通知的 `context_text` 逐字节相同，所以
  agent 注入的那条注记与之后上下文重放的那条永远不会分岔。
- 在**回合结束**时，没有被任何 agent 读过的扣住完成会沉降在 `Done for Ns` 摘要
  *上面* —— 在它们从未影响过的最终回复之后，那是下个回合的上下文携带它们的地方
  —— 在历史和 scrollback 里都是（不变量 3）。`StreamDone` 把旧的
  `App::end_turn` 拆成 `take_turn_summary`（把状态清到空闲高度、**构建**摘要）和
  `record_turn_summary`（推入它），在两者之间沉降扣住的完成：重新落座、提交最终
  回复、沉降，然后记录 + 提交摘要。`TurnSummary::shells` 的计数不受影响 —— 一个
  结束的 shell 早在 `bg_exited` 时就离开了 `App::background`，在计数被快照之前。
- 沉降一个完成：记录 `HistoryItem::Background(BackgroundNotice)`（绿色/红色的一
  行`●`；输出尾部随条目给模型，从不渲染）。**自动回合**现在由边界派发负责：在回
  合结束时，公告板上一条没被取走的**模型启动**注记（agent 从没听过它）且没有排队
  的回合在派发，就启动一个。通知是一个历史条目，所以当排队的用户批次取而代之被派
  发时，通知就搭那个回合的推导上下文一起走（不多一次请求）。用户启动（`!` +
  Ctrl+B）的 shell 只提交通知 —— 若有一个回合在飞，模型在回合中途经由公告板得知
  ，否则在下一个回合。

### 通知落在哪里

一条通知被记录在对话里**模型读到它的地方** —— 任何别的地方都会让 transcript 声称
一件没有发生的事：

- **回合中途**，在 agent 的宣告处：`run_agent` 对它在每轮开头取走的每条注记发
  `StreamEvent::NoticeDelivered { seq }`，循环用那个编号沉降扣住的 cell（
  `App::take_delivered_bg_completion` / `take_delivered_agent_notice`、
  `tui::background::Session::settle_delivered_notice`）—— 在上一轮 tool 结果之后
  、下一轮流出的任何东西之前，且流式缓冲区为空（不变量 2/3）。
- **在回合结束时** —— `StreamDone`（在其摘要之上）、一次后端错误、任一种 Esc
  结局 —— 每个仍被扣住的 cell 都沉降：没有 agent 读过那些注记，所以下个回合的
  上下文是模型遇见它们的地方（自动后续回合，或用户的下一条消息）。一条被
  注册表报告为已收回（`take_retracted`）的扣住注记则被丢掉。
- **空闲时**，立刻（没有回合要读它）。

**tool 边界不是沉降点。** 它们以前是：每次解析和段冲刷都沉降所有扣住的完成，理论
是通知 cell 在流式缓冲区为空时落到哪里都安全。它在那里确实落地安全 —— 但落在错的
位置。一个在模型*正在生成*一次调用时退出的 shell，它的注记在该轮请求发出之后才被
贴上去，于是模型只在下一轮开头遇见它，*在那次调用的结果之后*；而在那次调用的
`ToolStart` 处沉降，却把它记在了调用*之前*。transcript、Ctrl+D、rollout 和之后每
个回合的上下文于是都说模型读过「completed (exit code 0)」，却还是往已结束的
session里调了 `bashsend` —— 这就是被报告的那个 bug，读起来像注记「被注入了却从未
发送」。（上下文推导早就重新排过它能看见的那一种情形 —— 一条提交在同一个回合两次
调用*之间*的通知 —— `context::derive_round` 的延迟注记，它留下来是为了在这次改动
之前记录的rollout；一条在一个回合*之前*沉降的通知，与模型真的在那里读到的一条无
法区分，所以只有在读的那一刻记录才修得掉它。）Steered 消息从来没有这个 bug：
`Steered` 一直都是一次回显。离线 dummy 在一个脚本化 tool 轮次解析时读一次公告板
—— 它宣告的那一批的最后一次调用，那是它对「一轮开头」的替身（
`DummyAi::with_background`，就在它回合中途队列旁边；一次没有批次宣告的调用自成一
轮，而同一批两次调用之间从不是边界，因为真实循环只在该轮最后一个结果之后才读）——
并以同样的方式宣告每条注记，所以那个 demo—— 以及 `smoke.sh` Phase 43 —— 经由这同
一条路径沉降一条回合中途的通知。`smoke.sh` Phase 131 拿真后端对一个慢吞吞的 stub
provider 驱动被报告的那个情形：一个在 stub 写一条无关调用时完成的任务，必须在该
调用结果*之后*到达 wire、Ctrl+D和下个回合的请求（未修的构建三项全挂）。

- `TurnSummary::shells` 在 `end_turn` 快照正在跑的计数，渲染成
  `Done for Ns · N shells still running`（不持久化 —— 恢复的 session 的 shell 都
  没了）。
- `/clear` 杀掉一切（对话重置是硬重启）；退出杀掉一切（没有孤儿 ping）。

### 页脚的 shell 指示符，以及 ↓ 聚焦它（`App::background_focus`）

页脚正在跑的计数（`{model} · {cwd} · {n} shell(s)`）同时也是**入口**，Claude
Code 风格：↓ 不直接跳进管理器，它**先踩到指示符上** —— 那一段亮成调色板的青色（
`ui` 的 `footer_focus_bg()`/`footer_focus_fg()`）并等待，而页脚其余每一段都保住
自己的文本*和*它的暗色样式：

```
────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────
  kimi-k3 medium · ~/Codes/rust/zeroth · 1.8k/1M (0.2%) · 1 shell
                                                              ▔▔▔▔▔▔▔ cyan
```

- `App::background_focus` 是那个纯 flag（`App::background_focused()` 就是
`ui::footer_line` 据以绘制的东西）。它由空闲
  composer 上的 ↓ 设定，由 `background_focusable` 设闸：composer 为空、没有
  palette/文件 picker/搜索/shell 模式、先试过历史召回 —— **而且确实有 shell 在跑
  **。高亮落在页脚的计数*上*，所以你只能聚焦一个在屏幕上的指示符：没有 shell 在
  跑时↓ 保住它历史召回/光标的本义，管理器也就没有隐藏的键绑定。（这取代了旧的
  `had_background`「这个 session 跑过」这道闸，后者随 `background_ever()` 一起没
  了。）
- 亮着时，`App::on_key_background_focus` 在所有东西之前认领几个键（在 `on_key`顶
  部路由，紧接条带之后，所以它也胜过全局的 Ctrl+C/Ctrl+O/Ctrl+D）：一个**裸
  Enter** 打开管理器条带，**Esc** / **↑** / **Ctrl+C** 撤销高亮，第二次 **↓**保
  住它 —— 或者，在列着 agent 名单时，越过指示符进入名单选择（
  `docs/agents/agent-tool.md`；从名单的 `● main` 行按 ↑ 以同样的方式退回指示符，
  所以 ↑/↓ 对称地走 composer ⇄ 指示符 ⇄ 名单）。其他每个键都清掉高亮再干它正常的
  活 —— codex 的「活动后重置」、`?` 条带的规则 —— 于是打字既撤销它又打出字来，换
  行键（Alt/Shift+Enter、Ctrl+J —— `docs/input/shift-enter.md`）仍然插入换行而不
  是打开条带，而一个 Ctrl+O/Ctrl+D overlay 永远不可能被退回到一个陈旧的亮指示符
  上。
- 高亮被任何移除指示符的事情丢掉：**最后一个 shell 退出**（`bg_exited`）、
  打开条带（`open_background_view`），以及 `/clear`。

### ↓ 管理器条带（`App::background_view` + `ui`）

一个**内联**条带，替换 composer，和 `/model` picker 完全一样（绝不是备用屏幕
overlay）：`Option<BackgroundView>`，带 `List {selected}` 和 `Details {id}` 两种
模式，打开时拥有每一个键。

它**只**替换 composer —— 从不替换流式条。在回合中途打开管理器，会让正在跑的工具
的活 cell（包括它流出的尾部）、spinner 状态行、排队消息和 toast 都留在条带*上方*
的各自行里（一个用户报告：条带以前占掉整个区域，于是管理器恰好把它旁边的那个前台
命令藏了起来）。`ui::background_view_height` 在条带自己的行数之上为条保留行，而
`render_live_with_preview` 的条带分支绘制与 composer 路径同一个条（共用的
`render_strip` 辅助函数），条带钉在底部 —— 在短得放不下两者的终端上，条带保住它
的完整高度，先被挤压的是条。**那三个内联 picker**（`/model`、`/login`、
`/settings`）有同样的 bug，现在共用这套确切的几何（2026-08-08 更新）：预留是
`ui::layout` 的 `strip_above_rows`，切分是 `view_split`，绘制是
`render_strip_above` —— 四个视图全都走它们，所以谁也不会漂（
`docs/providers/llm.md`）。tool cell 仍然在解析时提交到区域下面（条带打开时提交
仍然允许 —— 除非它的页面正在流动，那里它们像在任何别的流动视图下一样暂停，而流出
重建会重新生成它们），于是「运行中 cell → 已提交 cell」的转换读起来和它在
composer 上时完全一样。条带拥有每个键的一个后果：运行中 cell 延迟出现的
`(ctrl+b to run in background)` 提示在条带打开时被抑制 ——
`App::background_hint_elapsed` 读作 `None`，那条权限 prompt的规则 —— 因为 Ctrl+B
从条带内部到不了运行器。

- **在亮着的页脚指示符上按 Enter** 打开列表 —— 唯一的进法，所以条带永远不会没有
  正在跑的 shell 在它背后就出现。它离开的方式也一样：当**最后一个**列出的 shell*
  在条带打开时*退出（`App::bg_exited`），条带关闭、composer 回来，因为一个没东西
  可管的管理器是死胡同 —— 它拥有的每个键（`↑/↓`、`Enter`、`x`）都无从作用，页面
  的唯一剩余用途就是被关掉。渲染器保留一个 `No tasks currently running` 页面作为
  防御性回退，用于一个背后没有 shell 却打开的条带（`App::open_background_view`
  是pub 的；app 自己的入口闸 `background_focusable` 到不了它）。
- 两个页面都以一个标题打开，穿着每个可走菜单给它的层级打头的那身装束 —— palette
  强调色（默认青色）、粗体，正如 `/hooks` 和 `/mcp` 那样 —— 因为那是回答「我在哪
  儿？」的那一行。
- 列表：`Background` 标题、`{n} active shells`、`❯` 标记的可选行
  （`{command} (running)`）、提示
  `↑/↓ to select · Enter to view · x to stop · Esc to close`。
- 详情：`Shell details`、`Status:/Runtime:/Command:` 字段 —— Runtime 人性化（
  `format_elapsed`：`2m 3s`，绝不是光秃秃的 `123s`）、一个 subagent 启动该 shell
  时的 `From: {type} agent` 字段，以及 Command 在值列下**逐词换行**跨越多行（
  `wrap_output`，保留空格），这样一条长命令行绝不会被截掉（
  `background_view_height` 因此拿真实宽度 —— 条带的高度现在依赖宽度）—— 一个
  `Output:` 框（圆角 `╭─╮` 边框）尾随实时输出的最后几行、`Showing N lines`，提示
  `← to go back · Esc/Enter/Space to close · x to stop`。shell 继续流进框里（bg
  通道调度帧；条带打开时绘制 tick 约 30fps 重新武装，好让 `Runtime` 跳动）。
- **一个比终端高的页面会把它的顶部流进真实 scrollback**（`docs/ui/view-flow.md`
  ）。条带像每个带框视图一样从底部锚定，所以它可交互的尾部—— 提示、收尾分隔线 ——
  永远在屏幕上；锚点跳过的那几行以前被丢进**完全没有缓冲**的地方，在一个 24 行终
  端上这吃掉了详情页的顶部分隔线，让对话直接扎进一个无头的框（被报告的 bug：*「
  我往上滚的时候它把行文本删掉了」*）。它们现在提交在 live 区域之上，于是终端自
  己的滚动能读到完整页面。详情页是 TUI 里唯一一个在两次按键*之间*就会变的页面 ——
  `Runtime` 跳动、输出框填充 —— 所以它的流动按 shell 的 id 签名（`ui::view_flow`
  的 `FlowSign::Frozen`）而不是按它的行：流出的顶部**冻在**scrollback 里它被提交
  的位置，而不是按条带自己约 30 fps 重新签名、purge 重建整个屏幕。一次 resize、
  `←` 回到列表，或换一个 shell 会重新签名它，重建也就重新流出页面。条带*画*什么
  完全没有变 —— 锚点、字段、框和提示都完全和以前一样；修的只是被跳过的行去哪里。
  列表页在按键时变，所以它像菜单那样按行签名。
- `x` 返回 `Action::KillBackground(id)`；循环通过注册表杀掉，随之而来的 `Exited`
  事件移除那一行（详情回落到列表；停掉最后一个 shell 会关闭条带 —— 见上）。
- 页脚多一个正在跑的计数：`{model} · {cwd} · {n} shell(s)` —— 它同时兼任条带的
  入口（见上）。

### Ctrl+B

正在跑的 `bash` cell 的实时预览追加一行暗色 `(ctrl+b to run in background)`（只
在实时里 —— 从不提交）；正在跑的 `!` shell cell 拿到同样一行。正在跑的`bashwait`
或 `bashsend` cell 则拿到 `(ctrl+b to stop waiting)`：它的 session 本来就在后台
跑，所以那里这个键是结束等待，命令继续跑。`bashkill` 一行都不拿，因为这个键对一
次 kill 不做任何事（`ui::tool::ctrl_b_hint` 挑是哪一个）。这个提示是**延迟**出现
的，Claude Code 风格：只在命令跑了 `ui::TOOL_BACKGROUND_HINT_DELAY`（3s）之后才
出现，于是一条很快就结束的命令永远不会闪它（快命令用不上它）。闸门是命令**自己的
**已过时间 —— `App::background_hint_elapsed`，每帧由边界从
`StatusClocks::command_start` 注入（`set_status_times` 那套）：一次模型 `bash`调
用的时钟从它的 `ToolStart` 起算，一次 `!` shell 运行的从 `run_shell` 起算，在命
令解析和每个回合开始时清零。回合已过时间不行 —— 一个模型工具可能在回合深处才开始
。同一个时钟也是运行中 cell **显示**的东西 —— `bash` cell 的
`+N lines (Ns · timeout …)` 时钟行和 `!` shell 的 `⎿ Running… (Ns)` 行经由朴素的
`App::command_elapsed` 读它，也就是 `set_command_elapsed` 设的那个值；
`background_hint_elapsed` 是那个值按提示唯一职责*遮罩*过的：条带和每个替换
composer的 picker 把它清空（它们吞掉这个键），同时把运行中 cell 留在自己上方的屏
幕上，那里它的页脚必须继续计数（`docs/ui/tool-streaming.md`，*Whose clock the
footer shows*）。**Ctrl+B 本身全程有效**（`can_move_to_background` 不设闸）；只
有可发现性提示在等，所以在一 条还很新的命令上按它照样把它移入后台。当屏幕上有正
在跑的命令时，Ctrl+B返回 `Action::MoveToBackground`；循环设上注册表 flag；运行器
（执行器的 `run_bash`或 `spawn_shell_command`）在轮询中途消费它、`adopt` 子进程
，并把这次调用解析成后台 —— 模型 bash 走 `ToolBackgrounded`（agent 循环以**移交
文本**作为 tool 结果继续—— 用户移走的引子盖在启动事实之上，见上文协议），`!`
shell 走它正常的`ToolEnd`+`StreamDone`，cell 解析成后台行。

`?` 快捷键条带也列出这个键，写成 `ctrl+b to background`
（`docs/ui/shortcuts.md`）：延迟提示只在一 条已经跑了一阵的命令上点名它，而条带
才是查键的地方。

### 交互式 session

一次 `bash` 调用跑在一个伪终端里，是这同一个注册表的一个任务（`launch_tty`、
`docs/tools/interactive-shell.md`）—— 页脚计数、↓ 管理器、`kill_all` 和 subagent
归属全都成立 —— 但有五处不同。它只在活过它的启动调用时才被**宣告**（
`BgEvent::Started`），所以一条在自己的调用内结束的命令从不在这里出现。它的监视器
发 `BgEvent::Screen`（模拟屏幕，有节流）而不是行输出，详情页把它显示在尾部本该在
的位置。一个模型*看见*过的退出 —— 一条框成 `Exit code: N` 或 `Stopped`的配套结果
—— 以 `Exited { observed: true }` 到达，它不贴完成通知、也不启动后续回合。最多
`MAX_TTY_SESSIONS`（16）个宣告过的 session 同时运行。而一个没人等、停下来**请求
输入**的 session 以 `BgEvent::Waiting` 到达，循环报告它的方式与报告一次退出相同
—— 为模型贴的注记（点名那个 session，好让 `bashsend` 能回答它）、在下一个安全边
界沉降的琥珀色 `● Background command "…" is waiting for input` cell、为模型启动
而开启的空闲后续回合 —— 而 shell 保住它的行（`docs/tools/bash-tools.md`）。结束
一个（`bashkill`）先发 `SIGINT`、再发`SIGTERM`，然后杀掉它 session 和进程组里剩
下的所有东西 —— session 按 id 找，或者在 `pkill` 没有 `-s` 的地方（macOS）按它的
终端找（`docs/tools/interactive-shell.md`）。一个**没有调用在等**就结束的
session ——它的注记没人读 —— 在被读之前一直可认领：一个点名它的配套调用随后自己报
告这次退出，那条注记也就不欠了（`docs/tools/bash-tools.md` *One notice per exit*
）。

### 持久化（`session`）

- `ToolRecord` 多一个 `backgrounded: bool`（false 时省略 —— 旧文件原样解析），
  映射到 `ToolStatus::Backgrounded`。
- 一种新的 `background` 行记录往返 `BackgroundNotice`。旧构建跳过未知的记录类型
  （前向兼容契约）。

## 不变量注记

- 后台 shell 是**与回合无关的**：Esc 打断、`fail_stream` 和通道更换从不碰它们
  （它们的事件走自己的通道）。
- 完成通知只提交在模型读到它们的地方 —— 一轮开头 agent 的 `NoticeDelivered` ——或
  者在回合结束时，从不在一段流式文本内部：一段流式回复里的外来 cell 会打乱已提交
  行的顺序（不变量 2/3）。提交只发生在会话视图里（不变量 4）；历史总是记录它们，
  所以 overlay 返回/resize 会重绘它们。
- 公告板和 `pending_bg` 为两个读者扣住同样的完成 —— 公告板喂**模型**，
  `pending_bg`喂 **TUI/历史** —— 而它们再也不可能在顺序上不一致：公告板的取走在
  回复通道上按序宣告，cell 跟着宣告走。（它们以前是两条在互不相关时刻排空的并行
  队列；一条注记可能在它的 cell 提交一轮之后才到达模型，而那个 cell 随后就坐在一
  个模型没带它做出的调用前面。）每条注记仍然恰好落地一次：被一个 agent 读过、在
  回合结束时沉降，或者被一次报告拿回去。
- 条带是内联状态，不是 `View` —— 四个备用屏幕视图和 resize 重绘全都和以前一模一
  样。它顶掉 composer 但不顶掉流式条：运行中工具的预览、状态行、队列和 toast 都
  用composer 路径同一批辅助函数渲染在它上面，所以条的几何（`preview_rows`、
  `strip_has_status`）对两者仍然是唯一的事实来源。
- 页脚高亮**不是一个模式**：它只认领裸 Enter/Esc/↑/↓/Ctrl+C，放其他每个键通过
  （在清掉自己之后），所以没有哪个按键会被困在它上面，也没有哪个既有键会失去本义
  —— 换行键在内。它也不改任何几何 —— 它绘制的那一行页脚是 `ui::footer_rows` 早就
  预留的，所以 `live_height`、光标座位和 scrollback 提交都不受影响。
