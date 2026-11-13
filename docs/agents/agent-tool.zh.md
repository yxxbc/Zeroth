# `Agent` tool —— Claude Code 风格的 subagent

模型现在能**启动 subagent** 了：它们自治地做侧对话，在自己的上下文上跑自己的
agent 循环（`bash`/`read`/`write`/`edit`），在 TUI 里实时汇报进度，并把最终
响应作为 tool 结果交回主 agent。它就是 Claude Code 的 `Agent`（Task）tool，
适配到这个内联 TUI —— 包括 Claude Code 没有的那些部分：用户可以**进入某个
subagent 自己的内联会话，与它对话**。

```
❯ Call two agents to fetch weather and time in Manila and Warsaw

● I'll spawn two agents...

● 2 background agents launched (↓ to manage · ctrl+o to expand)
   ├ Fetch current weather and time in Warsaw
   └ Fetch current weather and time in Manila

( ●•·  ) Working… (4s · ↓ 120 tokens · esc to interrupt)

──────────────────────────────────────────────────────────────
❯
──────────────────────────────────────────────────────────────
  kimi-k3 medium · ~/repo · 1.8k/1M (0.2%)

  ● main
  ◯ general-purpose  Fetch current weather and time in… 48s · ↓ 16.5k tokens
  ◯ general-purpose  Fetch current weather and time in… 48s · ↓ 15.9k tokens
```

## 这个 tool

声明在 `bash`/`read`/`write`/`edit` 旁边（`llm::tools::agent_spec`），只给主
backend —— subagent 永远拿不到 `agent` tool，所以 agent 无法嵌套：

| 参数 | | |
| --- | --- | --- |
| `description` | 必需 | 一个短的（3-5 个词）任务标签 —— 树里的行 / 页脚列表显示它 |
| `prompt` | 必需 | 交给 agent 去完成的完整任务 |
| `subagent_type` | 可选 | 要启动哪个 **agent 定义** —— `general-purpose`（默认）和 `explore` 以 `agents/*.md` 文件的形式随附，一个项目或用户可以再加更多；可用的类型及其 tool 会在 `<system-reminder>` 的清单里列名（`docs/agents/subagents.md`）。未知类型会被解析成一条列出真实类型的可恢复错误 |
| `run_in_background` | 可选 | **默认 `true`** —— 调用立刻以启动回执返回（agent 由它的 description 和 id 指认，配套 tool 用这两个把人接回来 —— `docs/agents/agent-tools.md`）；`false` 会阻塞该回合直到 agent 结束并返回它的最终响应，以点名该 agent、`agentsend` 继续的那一行收尾 |

（参考 schema 里的 `isolation` 参数有意不实现 —— 在这个 TUI 的范围之外。它的
`model` 在这里，但作为*定义*的属性而不是调用的属性：一个类型跑在哪个模型上，
是关于那个类型的常设选择，不是每次启动都要重决的事。）

subagent 的对话从零开始：先是它那个类型的 system prompt —— 定义自己有正文
时就用定义自己的正文，否则用会话的 persona/environment prompt —— 加上一段
subagent 说明（`prompts/subagent.md` —— “你的最终消息会返回给调用方”），然后
是作为第一条用户消息的 `prompt`。它的 **tool 集合**与**模型**来自同一个定义
（`docs/agents/subagents.md`）。它在自己的线程上、带着自己的 executor 和自己的
cancel token 运行 `llm::agent::run_agent`。executor 还携带**共享的后台注册表**，
所以 subagent 的 `bash` 能像主回合的那样在后台跑（`wait: 0`，或者一个刚好过去
的 `wait` —— `docs/tools/bash-tools.md`）—— 那个 shell 进入同一个列表（在每个
会话视图里都叠进页脚的 `· N shells` 计数），并通过
`BgOrigin { agent_id, agent_type }` 归属到它的启动者：↓ manager 的详情页多出
一个 `From: {type} agent` 字段，完成 notice 读作 `· from the {type} agent`，
上下文说明写 `launched by the {type} agent`。只有 **Ctrl+B 闩锁**保持主会话专用
—— subagent 的 executor 既不清理也不消费它，所以这次交接永远属于主回合的前台
命令。一次完成先路由到它的启动者：边界把这条 notice 排到启动 agent 的 steer
接缝上（`AgentRegistry::queue_input` —— 在它下一个回合边界被听到，
`docs/input/queue.md`）而且**别的什么也不做**，`StreamEvent::Steered` 的回显
才是把它记到那个 agent 的 transcript 上的东西；如果启动者已经落定，这条 notice
就落到共享面板上（主回合 / 自动后续路径），所以总有人听到结果。

## 协议

两层，镜像后台 shell（`docs/tools/background.md`）：

- **回复通道**承载父回合内部的*组生命周期*：
  - `StreamEvent::AgentBatch { background, agents: Vec<AgentSpec> }` —— 该轮的
    `agent` 调用被提前公布（id、description、类型、prompt）。循环据此播种名单
    （`App::agent_started`）和实时组 cell。
  - `StreamEvent::AgentGroupDone { group: AgentGroupResult }` —— 该组已解析
    （所有前台 agent 结束，或后台启动被回执，或 Ctrl+B 把剩下的挪到后台）。
    携带最终的逐 agent 快照，所以已提交的 cell 永远不会与 agent 通道抢跑。
    循环记录 `HistoryItem::AgentGroup` 并提交 cell。
- **agent 通道**（`agents::AgentEvent`，一个专门的 `select!` 源 —— agent 比回合
  活得久）按 id 标记地承载每个 subagent 自己的流：
  `AgentEvent::Stream { id, event }`，对应 subagent 的 `run_agent` 发出的每个
  `StreamEvent`（chunk、tool 批次、usage、done/error）。循环把它们折进名单条目
  （`App::apply_agent_event` → `agents::AgentRun::apply`），后者累积该 agent 的
  **自己的 transcript**（`Vec<HistoryItem>`）、实时 tool 队列、token 计数、
  tool 使用次数和状态 —— 也就是页脚列表、Ctrl+O 各 cell 和 agent 会话视图所
  渲染的一切。

`llm::agent::run_agent` 多了一个闭包，
`run_agents(&[ToolCallRequest]) -> Vec<(call_id, output)>`：一轮的调用被划分
（`agent` 与其余）；agent 调用交给这个闭包（它并发启动它们、发出上面那些事件、
等待前台完成 —— 轮询父级 cancel 和注册表的 Ctrl+B 闩锁 —— 并返回每个调用的
tool 结果）；其余照旧执行；结果按原来的调用顺序追加。这个闭包在单元测试里是一
个空实现的假货，在生产里是 `llm::backend` 的真实启动器。

## 注册表（`agents::AgentRegistry` —— 边界，和 `background` 一样）

一个循环与 backend 共享的可克隆句柄：分配 Claude Code 风格的 id（`a` + 8 个
base36 字符），保存每个运行中 subagent 的 `CancelToken` + 完成标志 + 最终消息
列表，并拥有逐 agent 的**待处理输入队列**（对话功能）。`kill(id)` 取消 token
并把完成标记为 `killed`，好让父级的等待循环立刻解析（不必等线程察觉）；
`kill_all()` 在退出时清扫，`clear()` 在 `/clear` 时清扫。已完成的前台结果从
共享槽里读出。每个槽还保存 lead 的配套 tool 需要的东西 —— agent 的任务、它
那些调用的一行式、它的运行时 —— 而一个已落定的槽比它的名单行活得久，最新的
16 个保持可恢复（`docs/agents/agent-tools.md`）。一次运行的终结事件在结果被
记录之后、持锁经 `AgentRegistry::settle` 发出 —— 正是这一点让一个等待中的
`agentoutput` 能认领结果，从而没有 notice 再重复它。

后台完成 notice 搭**既有的** notice 面板（
`BackgroundRegistry::post_agent_notice`，`from_model: true`，带上 agent 标记），
所以进行中的 agent 在同一回合内就能听到某个 subagent 结束，而
`dispatch_after_turn` 的自动后续回合以零新增管道覆盖空闲的完成。TUI 的 cell 是
新的 `HistoryItem::AgentNotice` ——
`● Agent "{description}" finished · {elapsed}`（绿色 —— 运行时由
`format_elapsed` 人性化，过一分钟就是 `6m 2s`）/ `was stopped by user` /
`failed`（红色）—— 与面板 notice 的编号一同持有，并在 lead 读到它的地方落定，
完全与 shell notice 一样（`StreamEvent::NoticeDelivered` 上的
`App::take_delivered_agent_notice`，否则在回合末尾紧挨着
`settle_bg_completions` —— `docs/tools/background.md` *Where a notice lands*）。
还有一条走同一条路径却不报告一个结局：`was resumed by user`（青色），它是 lead
的话，说明用户的消息又启动了一个已落定的 agent
（`docs/agents/agent-tools.md` *A resume is news when it starts*）。

## App 状态

- `App::agents: Vec<agents::AgentRun>` —— 名单。由 `AgentBatch`（创建）和
  agent 通道（更新）喂养。一个结束的 agent 会带着彩色的 `◯`（绿色完成 / 红色
  stopped/failed）**滞留**一会儿，然后由边界清扫（计时在 `main.rs` 里，跟 toast
  一样）—— 用户正待在那个 agent 的会话里时则推迟。被清扫的条目会**退场**而不是
  消失 —— 最新的 16 个连同它们的 transcript 一起留着，所以 lead 用 `agentsend`
  恢复的 agent 会整份回到名单里（`docs/agents/agent-tools.md`）。多久是条目
  自己的 `AgentRun::linger()`：自然结束时是 `AGENT_LINGER`（30s）—— 一个几秒
  就被清扫的行可能在用户读到之前就消失了，而且（过期是从 agent *自己的*落定
  起算的）甚至可能在它的组 cell 提交之前；（用户的 `x` 停下它之后
  （`AgentRun::stopped_by_user`）是 `AGENT_STOPPED_LINGER`（也是 30s），因为
  在按键下消失的行不会留下任何关于「停下了什么」的证据。每一处起算都读那个
  方法 —— 落定事件、组解析（`or_insert`，所以一个*在*停止*之后*解析的组不能
  重启或缩短它的倒计时）、每帧的重新起算 —— 所以没有任何路径能把一次停止降级
  成短滞留。第二次 `x` 会在两个截止时间之前**清除**该行（`AgentStop::Cleared`
  → `hidden`，条目本身由下一次清扫收集，好让一个实时组的解析仍能快照它）——
  而清除那个**会话视图**正开着的 agent 也会关掉那个视图：视图是从清扫即将丢弃
  的条目渲染的，一份留在已经列不出它的名单之上的 transcript（并且不标记任何
  会话在视图中）才是那个破损的中间态。
- `App::agent_group: Option<AgentGroupLive>` —— 实时组 cell（id + 后台标志），
  父回合运行时显示在预览条里。Esc 打断 / `fail_stream` 在本地解析它（状态 →
  Interrupted），因为通道切换会丢掉 backend 自己的 `AgentGroupDone`。
- `App::agent_selection: Option<usize>` —— ↓ 名单导航：`0` 是 `● main` 行，
  `i+1` 是第 i 个 agent。空闲 composer 上的 ↓ 在有 shell 可点亮时**先踩到
  shell 指示器**（既有的 `background_focus`），第二次 ↓ 进入名单；没有 shell 时
  ↓ 直接进名单。↑ 走同一条路回去：从 `● main` 行出发，有 shell 在跑时落在
  shell 指示器上（在那里再按 ↑ 回到 composer），否则直接退出 —— 所以 ↑/↓ 对称
  地往返 composer ⇄ 指示器 ⇄ 名单。↑/↓ 移动，`Enter` 查看（main = 离开视图 /
  关闭），`x` **停止**选中的 agent —— 它开着和排队中的权限 prompt 也随之离屏，
  因为一个回答将无人接收（`docs/state/permissions.md`）—— 而在一个已经落定的
  行上则**清除**它（见下），Esc 关闭，其他任何键在清除之后穿透（
  `background_focus` 的契约）。选择处于活动状态时页脚那行换成提示（main 上是
  `↑/↓ to select · Enter to view`，运行中的 agent 上是
  `Enter to view · x to stop`，已落定后是 `x to clear`）。
- `App::agent_selection_memory: Option<String>` —— **最后选过**的那一行，所以
  名单是一个游标，而不是每次从顶部重开的菜单：↓ 落在记住的那个 agent 上
  （`App::agent_selection_start`，两个入口都查它 —— composer 的 ↓ 和 shell
  指示器的第二次 ↓），而不是每次都从 `● main` 走过去。这段行走的两半都会写它：
  ↑/↓ 的步子，以及 `open_agent_view`（进入一个会话**就是**一次选择，那正是
  被报告的流程 —— Enter 去读某个 agent，↓ 直接回到它的那一行）。它保留的是
  **id** 而不是下标，所以底下的名单无论长了还是短了，都还能在同一个 agent 上
  恢复，而记住的 agent 若不在了就回落到 `● main`。把 `❯` 走到 `● main` 上就是
  忘掉一个的方式（`remember_agent_selection(0)` 清除它）—— 一次显式移到 `main`
  不能被记忆覆盖，这也是开着的会话视图*不*算第二个回落的原因：它在打开时已经
  设过记忆了。
- `App::agent_view: Option<String>` —— **agent 会话视图**：整个内联屏幕显示
  那个 agent 自己的对话（横幅 + 它的 transcript，像 `/clear` 一样用 Purge
  重建），输入框的上边框带着该 agent 的 description 作为右对齐标签，而那一个
  标签同时是三样东西。它**嵌在那条边框里**：文本之后边框继续画一个边界格
  （`── {description} ─`，`AGENT_VIEW_RULE_TAIL` 那个字形），所以标签读起来是
  框的一部分，而不是挂在右端外面。它**作为芯片点亮**，用活跃主题的强调色配它
  的 on-accent 墨色（`agent_view_label_bg` / `agent_view_label_fg`，也就是
  ↓ 聚焦时页脚芯片的装束，`docs/ui/theme.md`），标签自己的两侧留白在填充里面、
  边框尾巴在填充外面，所以那唯一一行说明 composer 喂给*哪个*对话的内容一眼可读，
  而不是一条暗色边框里嵌着的暗字（用户要求的填充；
  `the_composer_label_is_a_lit_chip_in_the_themes_accent` 在目录里的每个主题下
  渲染它）。而且它**被裁到那条边框的一半**（`agent_view_rule_label`，
  `width / AGENT_VIEW_LABEL_DIVISOR`，把两侧留白和尾巴字形都算进去），
  description 其余部分由 `TOOL_HEADER_ELLIPSIS` 截断：一段 `description`
  是模型自己的句子，
  可以长到终端那么宽，而 ratatui 会把一个过宽的**右**对齐标题从它的**左**端滑
  出去，所以一个未裁剪的标签既吃掉了整个框，*又*丢掉了它正在显示的那段文本的
  开头 —— 是 `─── An agent tasked with confirming it… ─`，
  而不是
  `h confirming its status and acknowledging the requested description length ─`。
  一条既放不下这个标记、旁边又放不下一个真实字符的边框干脆不带标签（孤零零的
  ` … ─` 什么也没说明，而框比提示更值钱）—— 不过一段*正好装得下*的
  description 在那个宽度上照旧显示，因为需要空间的是那个标记，而什么也没被裁掉。
  另外 composer **与 agent 对话**，而它就是**主会话的回合中途队列往下一层**
  （`docs/input/queue.md`）：agent 运行时按 Enter 把草稿停在它自己的队列上 ——
  在框上方显示为同样内缩的 `  ❯ …` 行，`AgentRun::queued` —— 它的循环在下一个
  回合边界取走它（`run_agent` 的 `pending_inputs` 接缝），在那里 agent 通道上的
  `StreamEvent::Steered` 把它变成**那个 agent 的 transcript 上**的一条真实用户
  消息，位置在它前面那段已流出的片段定稿之后。而当 agent 空闲时，会在它存储的
  消息列表之上派生一个续接回合，把草稿作为它最新的用户回合带上，transcript
  立刻记录它 —— 一次空闲提交。**由注册表决定是哪一种**，不是名单：
  `spawn_agent_chat` 返回一个 `AgentChatDelivery`
  （`Queued`/`Started`/`Declined`），因为比注册表晚一个事件的名单状态要么把待
  处理行永远搁在那儿，要么把消息记两遍。一次落定时还有消息没被读的运行会把它们
  交回（`Session::reclaim_agent_chat`）：一个**自然结束**的 agent 把它们直接作为
  续接拿回来，而失败或被 `x` 停下的 agent 什么也不留 —— 没有什么可续的，重启
  一个用户刚杀掉的 agent 恰恰与那个键的意思相反。**Tab 是另一半**，它同样是主
  会话的那个键往下一层：它为那个 agent 排一个**后续回合**
  （`AgentRun::followups`、`App::queue_agent_draft`），渲染在被 steer 的行下方
  并与之空行分隔，等那个 agent 的循环落定后由
  `Session::dispatch_agent_followups` 作为它自己的对话续接启动 —— 每次落定一个
  条目，按提交顺序，走 Enter 空闲路径所用的同一个 `agent_chat` 接缝。它在交出
  一个之前会问 `ReplySource::agent_ready_for_turn`（问注册表，从不问名单），
  所以一个后续回合无法被折成落定窗口的那次续接去当 steer；被停下的 agent 会
  连同被 steer 的行一起丢掉它们；而那里的 **Alt+Up** 是主会话越过该 agent 自己
  那些集合的两步 —— 先它的最后一个后续回合，然后（经
  `ReplySource::reclaim_agent_input` → `AgentRegistry::take_last_input`，它的
  回合边界读过之后就是 `None`）那条还在等的消息 —— 永远够不到主会话的积压。
  Tab 过去读的是**lead 的**流、推的是**lead 的**队列，所以打进一个 subagent 的
  消息会作为主对话的一个后续回合运行（`docs/input/queue.md`）。Esc（空
  composer）回到主会话（Purge 重建主 transcript，含流式中途的片段）。视图开着
  时，主回合的提交被抑制，完全与 Ctrl+O overlay 一样（不变量 4）；被查看 agent
  的事件经一个专门的 `StreamRender` 增量提交 —— **而它的条预演的正是同一个
  渲染的前沿**，所以一张正在成形的表格或一个围栏代码块在那里流式出现的方式与
  在主视图里完全一样，包括它那个实时 `● Thinking…` 块
  （`docs/agents/agent-view-streaming.md`）—— 而它的回合**以主会话回合的方式
  结束**：落定会在 agent 自己的 transcript 上记录一条暗色的
  `Generated for 59s · 6.1k tokens (2.8k cached)` 摘要（它的动词是视图那行所穿
  动词的过去式，按 agent 自己的时钟轮换 —— `docs/ui/status-indicator.md`）
  （`AgentRun::apply` 的 StreamDone 分支压入 `HistoryItem::Summary`，它的
  `tokens`/`cached` 是**该回合**计费的用量 —— `AgentRun::turn_usage_*`，由
  `reopen()` 重置，好让每次对话续接有自己的回执而名单计数保持累积；运行时在
  落定事件之前的那一刻被冻结在它的实时值上，所以 `secs` 是该回合的耗时），视图
  在 StreamDone 提交它（`Session::commit_agent_view_event`），每次重建和 Ctrl+O
  的 agent transcript 都从历史渲染它，派生上下文像对待任何摘要一样把它当装饰
  跳过，而一次失败的运行什么都不记（与主会话对齐 —— 红色 notice 就是那条记录）。
  被打断的运行也什么都不记，尽管主会话被打断的回合现在会在它的 notice 下提交一
  份回执（`docs/input/interrupt.md`）：被停止 agent 的红色名单行改为在
  `AGENT_STOPPED_LINGER` 期间把它的耗时和计数留在视图中。composer 在视图里
  保持它**完整的功能**：`/` 面板（命令作用于主会话，到处如此）、`?` 快捷键条、
  Ctrl+R 历史搜索，以及 `@` 文件 picker 都能用，而名单仍在它们下方；**Ctrl+O
  显示被查看 agent 自己的 transcript**（一次全新的有界构建 ——
  `ui::agent_transcript_lines` —— 主缓存不动），**Ctrl+D 显示它派生的上下文**
  （`ui::context_body` 按被查看的 agent 分支：正文从该 agent 的
  transcript 派生，
  `SYSTEM` 条目显示一份 subagent *实际被发送*的 prompt —— 主 prompt + subagent
  说明，由边界作为 `App::agent_system_prompt` 注入的
  `ReplySource::agent_system_prompt()` —— 并且没有 AGENTS.md 段，因为 subagent
  对话开始时没有它；`docs/architecture/context.md`），**而那里的 Tab 显示它自己
  的 classifier 上下文** —— 给它的启动 prompt 加上它已经做过的调用，也就是它
  自己的 auto-mode 判定所对照复核的那一块，从它的注册表槽里读，而不是从
  lead 的 backend 读（`AgentRegistry::classifier_context`，
  `docs/state/permissions.md`）；
  只有 `!` shell 模式仍然缺席 —— 开头的感叹号在这里是字面的聊天文本。**页脚
  的模型段与量表段也是被查看 agent 的**
  （`docs/agents/agent-context-gauge.md`）：它自己的上下文大小 ——
  `AgentRun::context_used`，最近一帧的 `input` 加上它之后加入的内容的估算，按主
  量表的规则（`docs/architecture/token-estimate.md`），落定时没见到帧就用它
  transcript 上的估算 —— 对比它跑的那个模型的窗口，以及当它的定义钉了一个模型
  时那个模型的名字（`App::context_gauge` / `App::viewed_agent_model`，由
  `ReplySource::agent_model` 和 `ModelSession::agent_context_window` 经
  `sync_agent_view_context` 喂养）；lead 的那一对（只有它驱动自动压缩）随主视图
  回来。overlay 返回和尺寸变化都会重绘 agent 视图
  （`tui::view::Session::repaint_active_view`）。

## 渲染（`ui`）

- **实时组 cell**（`live_agent_group_lines`）：树上方一行闪烁灰色的
  `● Running {n} agents… (ctrl+o to expand)` ——
  `   ├ {description} · {n} tool uses · {tokens} tokens`，每个 agent 带一行
  `   │ ⎿  {activity}` 状态行 —— 再加上延迟出现的
  `(ctrl+b to run in background)` 提示（仅前台）。活动是**黏性的**：第一个事件
  之前是 `Initializing…`，之后是最新 tool 的 `{Name}: {detail}` —— **每个调用
  一套语法**（`agents::activity_line`）：`bash` 调用的模型给出的 `description`
  （`Bash: Fetching current weather…`），否则是该调用参数的摘要
  （`Write: game.py`、`Read: ~/notes/today.md` —— 文件 tool 的路径按与它 cell
  头相同的相对 / `~` 相对 / 绝对规则显示：运行把最新的调用原样留着
  （`AgentRun::last_call`），渲染器为那一行向 `activity_shown` 要套用了会话规则
  的结果，`docs/tools/tools.md` *Path display*；绝不用 cell 头的
  `Write({args})`，那对括号在暗色裁剪过的一行式上只会显得杂乱），而 MCP 调用
  写成**大写服务器名压在 tool 名之上**（`Deepwiki: ask_question` —— 完整的
  `deepwiki - ask_question (MCP)` 显示名对这一行来说太绕口）；冒号后无话可说的
  调用只显示光秃秃的名字 —— 在调用之间保持（绝不掉回 `Working…`），好让 agent
  在就一个结果推理时那一行仍保有上下文（`StreamEvent::ToolStart` 携带
  `detail`；`AgentRun::last_call`）。agent **说的话**也是它的活动：它写的时候，
  那一行是 `Agent: {update}` —— 它文本的最新一段压成一行 —— 文本定稿之后它保持
  到下一个调用开始，两者中较新的那个胜出（`AgentRun::last_update`，
  `docs/agents/agent-progress.md`）。一个**单独的** agent 渲染成 tool cell 的
  形状而不是单行树：`● Agent({description})` 压在同一行 `⎿ {activity}` 之上
  —— `⎿ Initializing…`，然后 `⎿ Bash: Fetch public repos for linuztx` /
  `⎿ Bash: curl -s https://…` /
  `⎿ Agent: The profile lists 42 public repos; checking the events now.`。
  条的 `preview_rows`/`preview_lines` 像划定并绘制 tool 队列那样划定并绘制它。

  每一行活动 —— 树里的和单独 cell 的都一样 —— 都是**一行暗色、按宽度裁剪**
  （`agent_activity_row` 套 `clip_cols`，切口标 `…`；一旦某个 agent 失败或被
  停止就经 `agent_status_color` 变红）。单独 agent 的*运行中*调用过去只在这里
  破掉了那个形状：它白色的 `Bash(…)` 头折成好几行，压在一条暗色
  `Running…` 行上面，读起来像主回合自己那个运行中的 cell，还会让条在每帧都在
  跳的计数器下长高。圆点的闪烁灰色已经说明了调用在跑，所以那一行只说*什么*在跑，
  别的不说。
- **会话视图的条**（`agent_view_preview_lines`）：主条的各分支套在被查看 agent
  自己的状态上，顺序相同 —— 它的实时 tool cell（批次队列空行分隔，运行中的
  `bash` 经共享的 `ui::live::live_call_lines` **尾随它流出的输出**），否则是它
  开着的思考块（`live_reasoning_lines`），否则是它回复的**未提交前沿**，边界用
  它提交时同一个 `agent_render` 构建它并注入它的高度
  （`App::set_stream_preview_rows`，由 `agent_preview_rows` 读回）。见
  `docs/agents/agent-view-streaming.md` ——
  过去只预演一个成批渲染的行，正是「流式在 subagent TUI 里消失」那个 bug。
- **已提交的 cell**：`● {n} background agents launched (↓ to manage ·
  ctrl+o to expand)` 压着只有 description 的树行（绿色）；`● {n} agents
  finished (ctrl+o to expand)` 压着带计数的树行，每个 agent 带 `⎿ Done` /
  `⎿ Interrupted` / `⎿ Failed`（全部完成时绿色，否则红色）。一个**单独的**
  agent 提交为 `● Agent({description})` 压着
  `⎿ Done ({n} tool uses · {tokens} tokens · {elapsed})`（运行时人性化）（或者
  红色的 `⎿ Interrupted`/`⎿ Failed`）以及一行暗色 `(ctrl+o to
  expand)` —— 一次单独的后台启动保持
  `⎿ Running in the background (↓ to manage · ctrl+o to expand)`，其中 ctrl+o
  那半在教：启动 cell 展开成 transcript 里该 agent 的 prompt/tools/response。
- **Ctrl+O**：每个 `AgentGroup` 条目展开成自己的 cell ——
  `● Agent({description})` / `⎿ Prompt:`（缩进块）/ agent 跑过的嵌套 tool 头
  （`Bash(curl …)`，文件 tool 的路径按各 cell 的相对 / `~` 相对 / 绝对规则显示
  —— 对记录的 `Write(/home/u/repo/notes.md)` 显示
  `Write(notes.md)`：`app::tool_header_text` 写那个一行式，
  `app::file_tool_header` 是它的逆，所以 cell 把路径读回来时不用 ui 重新解析
  语法；`docs/tools/tools.md` *Path display*）/ `⎿ Response:`（最终文本）/
  `⎿ Done ({n} tool uses · {tokens} tokens · {elapsed})` 或
  `⎿ Interrupted`。**实时尾部**以同样的方式走 `App::agent_group` + 名单
  （活动为 `Running…`），而 `TranscriptSig` 对名单的代际做指纹，所以一个流式中
  的 agent 会让缓存失效。
- **页脚名单**（`agent_list_lines`）：只要还有 agent，live 区域就在页脚下方多出
  若干行 —— 一个空行间隔、main 行，然后每个 agent 一行
  `  {type}  {description trimmed}… {elapsed} · ↓ {tokens} tokens`（暗色类型、
  实时计数器、人性化的耗时）。**实心 `●` + 高亮粗体行标记视图中的那个会话**：
  正常情况下是 `● main` 压在暗色 `◯` 的 agent 行之上，而在某个 agent 的会话
  视图里，那个 agent 的行接过 `●` + 高亮，main 降级为暗色 `◯`（用户报告的那个
  修复 —— 高亮过去会一直黏在 `main` 上）。`❯ ` 标记只属于活动的 ↑/↓ 选择，
  并在 Enter/Esc 把按键交回 composer 时随它离开。已结束 agent 的圆点在 30s
  滞留窗口里变绿/变红 —— 用户的 `x` 才是让它变红的东西，而那一行留在那里（带
  提示里的 `x to clear`）直到第二次 `x` 或 30s 清扫。这些行是第五个
  `live_layout` 区域，所以 `live_height`、光标座位和各个 overlay 都不受影响。

## 不变量说明

- subagent 事件走自己的通道，**绝不**直接提交到 scrollback —— 名单是状态；
  cell 只在组回复通道的边界上提交（或者在 agent 会话视图里，经那个视图自己的
  渲染）。不变量 1-4 原样成立。
- 父级的 tool 结果在后续回合经 `context.rs` 的一个分支重放：
  `HistoryItem::AgentGroup` 为每个条目派生一个 `agent` 调用 —— 在 provider 自己
  的 id（即启动所应答的那个）下，带模型逐字的 arguments，两者都随启动的
  `AgentSpec` 携带并记录在条目上（为更老的 rollout 服务时是重建出来的）—— 折进
  该轮那一条 `tool_calls` 消息，与该轮其他调用并排，位置是模型给这次启动的位置
  （组的记录落在普通 cell 之前，位置恢复 wire 的顺序），再加每个 agent 一条
  `tool` 结果（`docs/providers/prompt-caching.md`）；`AgentNotice` 派生带方括号
  的 user 角色说明，携带最终响应。
- `session.rs` 往返两个新条目（`agent_group` / `agent_notice` 记录）；老构建
  跳过它们（前向兼容契约）。名单本身是短暂的，像后台 shell 一样。
- 会话视图**提交的是折叠所记录的东西**，不是到达的事件：
  `Session::commit_agent_tail` 比较 `AgentRun::apply` 前后的 transcript 长度，
  提交新记录下的 `Tool`/`Summary`。按事件做键，正是 `write`/`edit` 搬到
  `ToolAnswered` 上时掉下去的原因（`docs/agents/agent-view-streaming.md`）；本地
  的 `x` 停止根本没有任何事件，它也走同一个 helper。
- subagent 的**思考阶段**作为 `HistoryItem::Reasoning` 落定到它自己的 transcript
  上（`AgentRun::begin_reasoning`/`finish_reasoning`，该阶段的耗时由边界从
  `Session::agent_thinking_clocks` 注入），所以会话视图、它的 Ctrl+O 展开和名单
  的 token 吸附都表现得与主会话一样 —— 由同一个 `/settings` 的 **Hide thinking**
  旋钮设闸（`docs/agents/agent-view-streaming.md`）。
- `/clear` 和退出都会 `kill_all()` 所有 agent；一次 Esc 打断只杀掉在途的
  **前台**组（后台 agent 继续跑，像后台 shell 一样）。
