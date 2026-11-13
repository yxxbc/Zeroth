# 思考流

推理模型的思维链曾经是**看不见的**：后端把它从流里剥出来
（`llm::thinking::ThinkingSplitter`），包进
`ThinkingStart`/`ThinkingChunk`/`ThinkingEnd`，而循环把这段文本扔掉 —— 只数它
的 token 让计数继续跳，屏幕上除了状态行里半句 `· Thinking for 12s` 之外什么都
不显示。

现在它**实时流式输出**，然后**折叠**：

```
● Thinking…
  ⎿  I need to look at the file first. The user asked for a modern
      landing page, so the structure should be: a hero, three feature
      cards, a footer. Let me check the repo before writing…
```

……而当这个阶段结束时，整块让位给一行提交下来的文字：

```
Thought for 1m 5s · 1.5k tokens (ctrl+o to expand)
```

两种形状，刻意如此：跑着的时候它**就是**一个 tool cell —— 一个 `●` 圆点压在
`⎿` 槽线上，因为正在发生的就是这件事（有东西在工作，输出在它下面）—— 而它是这
个功能里**唯一**会动的东西：圆点像任何运行中的 tool 一样闪烁，而 `Thinking…` 带
着状态行的微光扫过。等它落定，cell 的形状就整个消失：没有圆点，因为已经没有事
在发生了。留下的是一条关于这个回合的事实，用的是 `Done for 7s` 已经用过的无圆
点形状，`(ctrl+o to expand)` 提示说明那段思考去哪儿了 —— 和一个被截断的 tool 预
览做出的承诺一样。

## 分量花在哪儿

全部花在 live 块上，落定的那行一点都不给：

| 表面 | 渲染什么 | 为什么 |
| --- | --- | --- |
| **Strip**（live） | `● Thinking…` —— 闪烁的圆点、带微光的粗白标签 | 唯一还有事在发生的地方，而这里的东西永远不会被提交，所以动是免费的 |
| **Inline**（落定） | 一行暗色，`Done for Ns` 的同一套装扮 | 一段结束的思考是关于已完成工作的脚注 |
| **Ctrl+O**（展开） | 同一行暗色，去掉提示 | 没有提示要留位置 —— 这*就是*展开 |

落定的那行在两个表面上都是暗色，所以一段思考无论在哪儿遇到看起来都是同一个东
西，也所以它永远不去和正上方那条助手回复争。

全文从不丢失 —— 它在 **Ctrl+O transcript** 里像 tool call 的输出一样展开，记进
rollout，并在 `/resume` 时回来。

由 **`ZEROTH_SHOW_THINKING`** 控制：假值（`0`/`false`/`no`/`off`）精确恢复旧的
不可见行为（状态行里那半句和 token 计数都留着 —— 只有 live 块和提交的 cell
走）。

## 为什么它折叠，而不是滚进 scrollback

Scrollback 是**不可变的** —— `insert_before` 排队的行，终端会永远留着。一个把
推理流进 scrollback 的设计永远收不回来，所以 `Thought for …` 没法替换它。于是
推理块**只住在 live region** 里，住在那条流式 strip 的预览位（运行中的 tool 的
尾巴本来就住那儿），而**唯一**会被提交的东西是那一行 cell。这就是每个 tool call
都有的那个折叠内联 / 在 Ctrl+O 里展开的契约（CLAUDE.md 不变量 4），而且它不需
要在 `term.rs` 里加任何新机制。

## 各个部件

### live 块（strip 预览）

`ui::live_reasoning_lines` 渲染

- `● Thinking…` 这个 header —— tool cell 自己的 `TOOL_BULLET`，**闪烁**在共享
  的帧脉冲上（`docs/ui/tool-pulse.md`），和运行中的 tool 一模一样，旁边是一个
  带着状态行**微光**扫过的标签（`ui::status::shimmer_spans_from`，就是它下面那
  个 `Working…` 动词披的同一道波，取自同一个帧时钟）；以及
- 到目前为止推理的**尾巴**，在 `⎿` 槽线里
  （`TOOL_RESULT_PREFIX`，经由 `ui::file_cell::gutter_row_styled` —— 第一行带转
  角，其余对齐在它下面）：最后 `REASONING_PEEK_LINES`（三）个换行后的显示行，按
  单词换行且保留空格（`ui::wrap_output`，流式 `bash` 尾巴和 Ctrl+O 视图用的同
  一个包装器），暗色且**斜体** —— 这是把一段思考的 `⎿` 正文和一个 tool 的区分
  开来的唯一线索。

**三行**（2026-10-15 更新；窗口原来是五行）。数的是显示行，不是源行，所以一个
换到两行的行花掉两行：在 87 列 —— 报告里的那个屏幕 —— 上三行思考就把五行全填
满了，让这一块比输入框高出六行，每一行都在把对话往上推（不变量 3）。三行把模型
此刻正在写的东西留在视野里，和落定的 `bash` cell 在其折叠之上露出的正文高度一
样（`TOOL_FOLD_ROWS`）；整段思考离一次 Ctrl+O 只有一步。

空的源行被跳过：推理里满是段落分隔，把它们花在这个小窗口上会只显示出三分之一的
思考。从最新往回走源行，只对窗口显示的部分做换行，所以一次长思考每个动画帧花
O(window)，不是 O(reasoning)。

**微光的地板在这里被抬高了。** Codex 把它的动词停在灰色 `#888888` 上，所以它读
起来是*灰字上跑一道白波* —— 对一个度量是对的，对一个 header 是错的：在两个波峰
之间（每一趟扫过约三分之一的时间）它会和它下面的暗色正文无法区分。于是
`shimmer_spans_from` 接受静止颜色，而 `Thinking…` 传入近白的
`reasoning_shimmer_base()`（`#C8C8C8`）。静止时它是粗的近白；波把它提亮到
`#FFFFFF`，而不是成为唯一让它可见的东西。（状态行那个动词后来也离开了 codex 的
灰色，为了页面标题的渐变 —— `docs/ui/status-indicator.md` —— 所以 header 和动
词都不落在暗色上。）这是这个功能里唯一抓眼睛的地方 —— 刻意如此，因为它是唯一还
有事在发生的地方。

`ui::preview_rows` 从同一份状态量出同一块的大小（strip 的 `debug_assert` 把它
钉住），所以框和光标留在原位。

推理分支排在 agent/tool 分支**之后**、流式回复分支**之前**。实践中它们从不撞车
—— 推理先于它那一轮的文本和 tool call —— 但一个真的在执行中的 tool 会赢，因为
那才是用户在等的东西。

状态行保留它那半句 `· Thinking for 12s`：它是*计时器*，块才是*内容*。

**一个折叠的读/搜组打开时，既没有块也没有 cell。** 两次 read 之间的一段思考把
strip 让给那个组的 cell，它在 `ui::live::preview_lines` 和 `ui::preview_rows` 里
都压过推理分支，所以这个 cell 不会每有一段思考就长出一个块、下一次调用又缩回
去；strip 只说出状态行那半句，Ctrl+O 显示随流而来的文本。落定时，这段思考随组
一起保留，自己不提交任何东西：对话里只有组的行，而 Ctrl+O —— 或者 **Verbose
output** —— 显示这段思考（`docs/ui/tool-groups.md` *Thoughts*）。

### 提交下来的 cell

`ThinkingEnd` 关闭缓冲区并追加 `HistoryItem::Reasoning`：

```rust
pub struct Reasoning {
    pub text: String,       // the whole streamed chain-of-thought
    pub secs: u64,          // how long the phase ran (boundary clock)
    pub tokens: usize,      // the estimate, snapped to the provider's count
    pub timestamp: String,
}
```

`ui::reasoning_lines` 把它渲染成一行不换行、**无圆点**的暗色行 ——
`summary_lines` 的形状*和*颜色（`status_done_color()`），好让把一个回合夹在中间
的那一对读起来像一对：

```
Thought for 1m 5s · 1.5k tokens (ctrl+o to expand)
Done for 1m 12s · 2.8k tokens (64 cached)
```

`format_elapsed` 把秒数人性化（`5s`、`1m 5s`、`1h 2m`），
`format_token_count` 处理 token（`842`、`1.5k`、`1.2M`）—— 状态行和回合摘要用
的是同两个辅助函数，所以 TUI 里每一个时长和计数读起来都一样。token 数为零时藏
掉那半句；`EXPAND_HINT` 和被截断的 tool 预览结尾用的是同一个字符串。

**顺序是不变量 4 的事，不是白送的。** 这一项在思考结束的那一刻就追加 —— 那通常
发生在这一轮的回复文本存在之前，但模型可以在已经开始回答**之后**再推理（dummy
的默认回合正是这个形状）。所以这个阶段之前的那段助手文本要**冲掉**
（`App::flush_streaming_segment`，那套 `ToolStart` 舞步）：不冲的话，cell 会把
自己拼进那段还在流的段落里，而 `history` —— 思考刚刚被追加进去的地方 —— 就和
scrollback 不一致，一次 resize 重绘会把它们重新排序。

**冲掉发生在 `ThinkingStart`，趁 live 块立起来的时候**（2026-08-08 更新）——
不是在这个阶段结束时。块是一个像 tool 的 cell，而一个 cell 在**活着的时候**就
需要它上面那行空隔，不是等它落定之后：`● Thinking…` 这个 header 以前直接顶在
那段还在流的段落上（这就是报告的那个 bug），然后等阶段折叠、冲掉终于跑了，它又
往下跳一行。在开头就冲掉还放出了段落的**最后一行** —— `StreamRender::commit`
会扣住还在长的那一行，`finish` 不会 —— 所以模型刚写完的文本在它思考下一步做什
么的时候，在屏幕上就是完整的。闸门就是显示：一个被藏起来的阶段
（`show_thinking()` 为假）不显示块，所以它绝不能把回复切开，那条路径上什么都
不变。一个*被显示*的阶段如果最后没有文本，仍然留下那道切分 —— 但它的 header 在
屏幕上待过，所以这道断裂是诚实的，而真后端只在握有 reasoning delta 时才开一个
阶段（`llm::backend` 从 `if !delta.reasoning.is_empty()` 里面发
`ThinkingStart`）。`settle_reasoning` 保留自己的冲掉：在开头那次之后它是空操作，
而对阶段中途的 Esc/错误，仍然由它来跑。

`ui::reasoning_full_lines` 是 Ctrl+O 的展开 —— 同一行暗色，**去掉**
`(ctrl+o to expand)` 提示（这*就是*展开），压在 `⎿` 槽线里出现的全文之上，保留
模型自己的段落分隔。不过只是段落*之间*的那些分隔：推理常在空行上开头和收尾
（`<think>\n…\n\n</think>`），而这些边缘被丢掉。开头空一行会把 `⎿` 的转角挤离
思考的第一行，收尾空行会叠在 transcript 给每一项后的那一行空隔上 —— 一段思考和
它下面的 cell 之间三行空白，而别的任意两个 cell 之间只有一行。

**思考中途按 Ctrl+O** 也显示这个阶段，整段而不是开窗
（`ui::reasoning_live_full_lines` —— 同一个 `● Thinking…` header 压在同一个槽线
上）：分页器没有行预算，所以它像对待一次流式 `bash` call 的输出那样跟住前沿。
`TranscriptSig` 为此多了一个 `reasoning_len` 字段，所以 overlay 之下正在流的
一段思考会像流式 tool 一样精确地作废缓存住的尾巴
（`docs/ui/tool-view-performance.md`）。那里的圆点静止渲染 —— transcript 的签名
刻意不含时钟。

### token 计数

状态计数用应用侧的估算来数推理（`App::push_thinking` → `tokens::estimate`），
而 cell 就是按这个估算构造的，因为 `ThinkingEnd` 远在这一轮的 usage 帧之前就触
发了。那一帧到达时，应用把这个 cell **对齐**到 provider 的真实数字 —— 正是
`App::apply_usage` 早就在对 live 计数做的事
（`docs/providers/prompt-caching.md`）：

```json
"usage": {
  "completion_tokens": 74,
  "completion_tokens_details": { "reasoning_tokens": 23 }
}
```

`TokenUsage::reasoning` 携带它（在 `llm::openai::parse_sse_usage` 里解析），而
`App::apply_usage` 把它分摊到**本轮**记录下来的推理项上
（`App::round_reasoning`）—— 按它们的估算比例分，最后一个拿余数，好让总和精
确。常见情形是每轮一个阶段，那就只是「把它设成 `reasoning_tokens`」。

已经在 scrollback 里的那一行，保留它被提交时的估算（没有任何东西能重写一行
scrollback）；此后每一次渲染 —— Ctrl+O transcript、一次 resize 重绘、一次
`/resume` —— 显示的都是 provider 的数字。

一个不报告 `reasoning_tokens` 的后端（dummy、省略这个细节的 provider）就简单地
保留估算。

### 缓冲区在哪里关闭

| 事件 | 一个开着的缓冲区会怎样 |
| --- | --- |
| `ThinkingEnd` | 记录并提交 —— 正常路径 |
| Esc 中断，**保留** | 记录并提交，排在部分回复之前（codex 从不撤回已经流出去的东西） |
| Esc 中断，**撤销** | 连同这次提交的其余部分一起丢弃 |
| 后端 `Error` | 记录并提交，排在部分内容之前 |
| `/clear` | 丢弃 |
| 一个 `/compact` 回合 | 从不打开 —— 摘要那一回合按设计就是不可见的（`docs/architecture/compact.md`） |

`Session::settle_reasoning` 是那个唯一的边界辅助函数：它用思考时钟走过的时间关
闭缓冲区，提交 cell（像每一次提交一样按视图闸门），并清掉时钟。三个落定处都在
记录别的东西之前调用它，这正是让流顺序保持正确的原因。中断也会在停掉这个回合的
时钟之前调用它：`abandon_inflight` 连同其余的时钟一起清掉思考时钟，而在它之后
落定的一个阶段，不管模型思考了多久，都会读成 `Thought for 0s`，且正压在被中断
回合的回执上方（`docs/input/interrupt.md`，`smoke.sh` Phase 66）。

### 持久化与上下文

- **Rollout** —— `session::ItemRecord::Reasoning`，所以 `/resume` 恢复这些 cell
  和它们的文本。旧的 rollout（没有这种记录）照旧解析。
- **派生上下文** —— 一个推理项被 `context::context_messages` **跳过**。Chat
  Completions 没有地方放上一轮的原始思维链，而重发它只会白白烧掉上下文。因此
  Ctrl+D 里看不到它一丝痕迹，这正是模型实际收到什么的真相。
- **Checkpoint / 回溯** —— 没什么特别的：这一项就是一次普通的追加，所以按对话
  长度算的键和 Esc-Esc 截断都照它们一贯的方式工作。

## subagent 也出声思考

上面的一切说的都是*主* session。一个 **subagent** 现在在它自己的状态上跑同样的
形状（`docs/agents/agent-view-streaming.md`）：`AgentRun` 携带这个阶段的缓冲
区、它这一轮的 snap 目标，以及边界注入的已用时间；
`Session::settle_agent_reasoning` 是 `settle_reasoning` 的兄弟，在那个 agent 的
transcript 上记录 `HistoryItem::Reasoning`，并在它的 session 视图在屏幕上时提交
折叠的 cell。strip 的 live 块、`Thinking for Ns` 那半句、Ctrl+O 展开、
`reasoning_tokens` 的 snap，以及 `/settings` 的 **Hide thinking** 闸门，全都是
同样的代码路径。唯一没共享的那块是 `distribute` 的调用方：snap 是纯的
`app::reasoning::snap_reasoning_tokens`，`App` 和 `AgentRun` 都调它，而不是各自
留一份。

## 那个开关

`tui::config::show_thinking()` 用 `ZEROTH_PERMISSIONS` 的语法读
`ZEROTH_SHOW_THINKING` —— **除非**值是 `0`、`false`、`no` 或 `off`，否则开着
—— 而 `Session` 在 bootstrap 时存一次。闸门在边界上：关掉时 `Thinking*` 那几个
分支从不打开缓冲区，所以 `App` 没有东西可预览也没有东西可记录，下面每一个纯渲染
器就干脆永远见不到推理项。没有第二条代码路径要同步。

注意它**不**改变*向模型要什么* —— Ctrl+T 的思考模式
（`docs/providers/reasoning.md`）仍然骑在请求上。`ZEROTH_SHOW_THINKING=0` 把思
考藏起来；把 `Ctrl+T` 打到 `off` 让它不再发生。

## 测试

全程纯的、有单元测试：usage 解析（`llm::openai`）、缓冲区生命周期与 usage 的
snap（`app::tests::reasoning`）、各渲染器与 strip 的几何
（`ui::tests::reasoning`）、记录的往返（`session`），以及上下文的跳过
（`context`）。

边界的接线由 `scripts/smoke.sh` 的 **Phase 66** 覆盖，它在 tmux 里驱动真二进
制：dummy「思考」时 live 块出现，它被折叠的 cell 替代，思维链**不在**
scrollback 里，Ctrl+O 把它展开回来 —— 而在 `ZEROTH_SHOW_THINKING=0` 下同样的回
合两样都不显示，却仍然落定到它的 `Done for Ns` 摘要。

`tests/live_openrouter.rs` 里有两个 `#[ignore]` 的 live 测试钉住 provider 那一
端（用真 key 跑）：

- `live_usage_reports_the_reasoning_token_count` —— 一条真流的最后一帧真的携带
  `completion_tokens_details.reasoning_tokens`；
- `live_thought_cell_snaps_to_the_providers_reasoning_tokens` —— 把一条真流的
  事件重放进一个真的 `App`，断言记录下来的 cell 用它的 token 估算换来了
  provider 的计数（观察到的：164 token 的估算 snap 到报告的 89 —— 两套记账确实
  不同，而这正是宁可要 provider 那套的全部理由）。

## 已知限制（v1）

- **subagent** 不显示它们的推理。`agents::AgentRun::apply` 仍然把一个 subagent
  的 `ThinkingChunk` 计进它的 token 计数，仅此而已 —— 花名册的树行没有自己的
  时钟来给一个阶段计时，所以 `Thought for …` 那个 cell 没话可说。它们的推理被丢
  掉，一如既往。
- 每个**思考阶段**一个折叠 cell，不是每回合一个：一个在一轮里交错推理与文本的模
  型，每一阵提交一个 cell。这忠于实际发生的事，但一个话多的交错者会产出好几个
  短 cell。
- provider 的计数来得更晚时，scrollback 那一行保留 token 估算（见上）。只是已
  经印出来的那一行；每一次重渲染都被修正。
