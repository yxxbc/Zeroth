# rollout 为每个调用和每个请求记录什么

每一个对话都录制进一个 rollout 文件（`docs/state/resume.md`）。那个文件足以
*恢复*一个 session，却不足以*分析*一个。一行 tool 说出这次调用是否成功、打印了
什么，别的没有了。退出码只以 `output` 开头的 `Exit code: N` 这段文本的形式存在，
一段很长或被截断的输出会让它难以找到。时长和失败的原因完全没有记录。token 用量
是 `summary` 行上每个回合一个合计，所以单个请求、或单个 tool 步骤的代价算不出来。
一个把各次运行与另一个 harness 比较的基准正好撞上了这一点：它必须用正则从文本里
抠出 `Exit code`，而它按请求统计的 token 数在这里没有任何东西可以对照。

rollout 现在多携带两类事实：

- 每一行 `tool` 记录**这次调用是怎么跑的**：`exit_code`、`duration_ms`，失败
  时还有 `error_kind`；
- 每一个模型请求留下一行 **`usage`**，带 provider 自己的计数，写在请求结束的
  位置。

两者都是新增。既有的字段不动。

## codex 是怎么做的（调查结果）

rollout 格式是 codex 的移植（`docs/state/resume.md`），所以 codex 在这里也是
范本（对照 `openai/codex` 的 `codex-rs` 核对过）：

- **每个模型响应一条 usage 记录。** 每一个完成的响应持久化一行
  `token_usage_record`（`history/src/lib.rs`，由 `core/src/session/mod.rs` 的
  `record_observed_response_completed` 写入）。它的 `usage` 就是那个响应的
  `TokenUsage`：`input_tokens`（含缓存读取）、`cached_input_tokens`、
  `cache_write_input_tokens`、`output_tokens`、`reasoning_output_tokens`。
  无论历史模式是什么，这条记录总会被持久化。它还携带回合与线程的运行合计，
  以及该响应的 id。`TokenCount` 是一个独立的、累计的 UI 快照，不是按请求的
  账本。
- **命令 item 保留退出码与时长。** `CommandExecutionItem`
  （`protocol/src/items.rs`）有 `exit_code: Option<i32>` 和 `duration`，用
  spawn 与等待前后各一个 `Instant` 计时（`core/src/exec.rs`）。内部时长序列化
  成 serde 的 `{secs, nanos}`，但每一个公开 schema（app server、analytics、
  OpenTelemetry 的 `codex.tool_result`）都把它转成整数 `duration_ms`。
- **失败种类是一套分析词汇。** 持久记录只有一个状态
  （`completed`/`failed`/`declined`）。分析事件分类失败
  （`ToolItemFailureKind`：`tool_error`、`approval_denied`……）与终结状态
  （`ToolItemTerminalStatus`：`completed`、`failed`、`rejected`、
  `interrupted`）。

作为对照，Claude Code 的 transcript 把 API 的 `usage` 存在 assistant 行上。
一个响应按每个 content 块一行写出，所以读的人必须按 request id 去重。它的退出码
与时长只活在它的遥测里。

我们从 codex 取的是：每个请求一条记录而不是每个块一条、codex 的各个计数、如每
一个公开 codex schema 那样的整数 `duration_ms`，以及一套建立在 codex 名字之上
的封闭失败词汇，写进持久记录，好让分析读得到。

## `tool` 行

三个字段加入 `tool` 记录（`session::ToolRecord`）。每一个都在**没有值时被省略**，
也就是该记录之后每个字段都遵循的那条规则，所以一行没有它们的记录读起来与从前完全
一样：

```json
{"timestamp":"…","type":"tool","payload":{"name":"Bash","args":"cargo test","ok":false,
 "exit_code":101,"duration_ms":48213,"error_kind":"tool_error",
 "output":"Exit code: 101\n…","timestamp":"03:21 PM","shell":false,"truncated":false, …}}
```

- **`exit_code`** 是命令的退出状态，在这次调用期间它**退出**时记录：一次
  `bash` 调用、一次 session 结束了的 `bashsend`/`bashwait`/`bashkill`，以及一条
  `!` shell 命令。没有的时候它缺席：这次调用不是命令、命令还在跑（作为 session
  交回去，或移到后台）、它被信号杀掉，或者它被时间上限停掉。`Exit code: N` 那一
  帧一如既往留在 `output` 里。
- **`duration_ms`** 是这次调用**跑了**多久，以整毫秒计。它从调用开始运行那一刻
  起算 —— 在任何 permission prompt 和任何 `PreToolUse` hook 之后 —— 到它的结果
  为止（`run_agent` 给 executor 计时，正如 codex 给 spawn 与等待计时）。一条
  `!` 命令由它的 runner 计时。一个从未运行过的调用没有它：在 prompt 处被拒绝
  的、被 hook block 的、超过调用上限的，或者一个在开始之前就被 Esc 结束的批次
  兄弟，或者一个点名了未存储 secret 的调用。被 Esc 或 backend 错误提前切断的运行
  中调用，记录它跑了多久：循环在那一刻读调用自己的时钟（它实时单元格所计的那个
  时钟），而不是在最后一帧读，所以一次不绘制任何东西的 headless 运行也能记录它。
- **`error_kind`** 说出一次失败的调用（`ok: false`）为什么失败。它从不出现在成功
  的调用上：

| `error_kind` | 这次调用…… | 由谁设置 |
| --- | --- | --- |
| `tool_error` | 在 tool 内部失败：它跑了并失败（一个非零退出，由 `exit_code` 点名；一个缺失的文件；一次什么都没匹配到的 edit；一个 MCP 错误），或者它无法按写的那样运行（tool 读不了的参数、一个点名了未存储任何东西的 secret 占位符） | 任何失败结果的默认（`run_agent`、`!` runner）；占位符由 approval 接缝设置（`docs/state/secrets.md`） |
| `timed_out` | 被时间上限停掉：一条没有地方在它的 `wait` 之后继续跑的命令，或一个没人回答的问题 | executor、`llm::ask` |
| `interrupted` | 被 Esc 提前切断，无论是在跑还是在等它那一轮 | `App::interrupt_turn` |
| `rejected` | 被用户拒绝：permission prompt 上按 No（或 Tab 修改后的 no，或 Ctrl+E），或一个被拒答的问题 | approval 接缝、`llm::ask` |
| `classifier_denied` | 被 auto 模式的 classifier 拒绝 | approval 接缝 |
| `hook_blocked` | 被一个 hook 拒绝：一个 `PreToolUse` block 或一个 `PermissionRequest` deny | `run_agent`、approval 接缝 |
| `approval_required` | 需要一个没人能给的批准：一次 headless（`-p`）运行不问任何人 | approval 接缝 |
| `tool_call_limit` | 被 `/settings` 的 **Max tool calls** 上限拒绝 | `run_agent` |
| `backend_error` | 在该回合的请求失败时正在飞 | `App::fail_stream` |

线上名在 codex 有对应时就是 codex 的（`tool_error`、`timed_out`、
`interrupted`、`rejected`）。其余的命名只有这个应用才有的东西：auto 模式的
classifier、hook、headless 运行与调用上限。

### 这些事实怎么走

事实在它们被知晓的地方产生，并从那里作为数据携带：

- executor 的 `ToolOutcome` 携带 `exit_code` 与任何具体的 `error_kind`
  （`timed_out`）。它在每一处学到退出码的地方设置它：管道 runner、命令退出时的
  终端 runner、一次看到该退出的 session 调用，以及一次事后声称的退出的报告。
- `run_agent` 给 executor 计时并构建这次调用的 **`ToolRun`**
  （`stream::ToolRun`：三个事实在一起）。一个没有具体种类的失败结果变成
  `tool_error`。它自己决定的那些拒绝（hook block、调用上限）以及 approval 接缝
  交给它的那些（`Approval::Reject` 现在点名它的 `kind`）携带它们的 kind 和没有
  时长。
- 每一个解决调用的（`ToolEnd`、`ToolAnswered`、`ToolRejected`、
  `ToolBackgrounded`）事件都有一个 `run` 字段，所以一条新的解决路径必须在能编译
  之前说出它的事实。循环在解决之前把它应用到运行中的调用上
  （`App::set_tool_run`，`set_tool_truncated` 的模式），而记录下来的
  `ToolCall::run` 就是 `session` 所写的东西。
- 应用自己解决的那些调用 —— 一批被 Esc 或 backend 错误结束的 —— 在那里打戳
  （`App::fail_live_queue`）。

## `usage` 行

provider 报告了用量的每一个模型请求一行，计数与 `StreamEvent::Usage` 携带的完全
一致：

```json
{"timestamp":"…","type":"usage","payload":{"input":18240,"output":312,"cached":17920,"cache_write":0,"reasoning":128}}
```

- **`input`** 是这个请求计费的每一个 prompt token，**包含缓存读取与缓存写入**。
  每一条线上格式都归一化成这个，即 OpenAI 的记账形状，而 Anthropic 线上格式把它
  未缓存的那部分与两个缓存计数相加来达到它（`docs/providers/claude.md`）。
- **`cached`** 是 `input` 中由 provider 的 prompt 缓存服务的那部分，而
  **`cache_write`** 是写进去的那部分（`docs/providers/prompt-caching.md`）。
- **`output`** 是每一个补全 token，**包含 `reasoning`**：花在思考上的那部分。
- **`agent`** 只在 subagent 的请求上出现。它用该 agent 的 `agent_group` 条目所
  携带的同一个 `id` 点名它，那个条目的 `tokens` 是这个 agent 各请求的合计。对话
  自己的请求 —— 包括 `/compact` 的摘要化 —— 不带 `agent`。
- **`compact`** 只在写出一次压缩摘要的那个请求上以 `true` 出现（`/compact` 或
  自动压缩，`docs/architecture/compact.md`）。那个请求是对话自己的，而它的代价
  正是分析最常想单独拎出来的那一个：它读整个 context 来替换它。

五个计数总是全写，包括零，所以读的人不需要任何默认值。它们的含义就是每回合
`summary` 的含义：一个已完成回合的 `tokens` 是它所产出的、对话自己的 `usage`
行的 `input + output` 之和，而它的 `cached`/`cache_write` 是那些行相应项之和。
subagent 的行不在那个和里（它们属于它的组条目），而一次压缩没有 `summary`：它的
`compact` 行之后跟的是 `compaction` 记录。

### 这一行坐在哪里

一行 `usage` 写在**它的请求结束的地方**，在迄今记录的各个条目之中。在一个 tool
轮里，这意味着该请求的那一行在它所要求的各次 tool 调用之前，而下一个请求的那一行
在它们之后：

```text
message (user)
usage            ← request 1: the reply that asked for the tools
message (assistant)
tool             ← the calls request 1 made
tool
usage            ← request 2: it read their results
message (assistant)
summary
```

所以一行 `usage` 与下一行之间的那些调用，就是第一个请求所要求的调用，而下一行的
`input` 因它们的结果而增长。那就是分析想要的按步骤代价，按文件顺序读出来，就像
codex 自己的读者读它们的文件那样。模型在文本之前流出的思考落在它落定的位置，那
可能在它那个请求的 `usage` 行之前。只有那些 tool 调用被保证坐在它们自己的请求与
下一个请求之间。

### 记录它

一行 `usage` 是一个**sidecar**，像 `checkpoint` 和 `model` 一样
（`docs/state/resume.md`）。它不是历史条目：transcript、模型所读的 context、
`/resume` 的历史和 Ctrl+O 视图都不受影响，而一个在此之前构建的版本会跳过这个
未知的记录类型。

recorder（`tui::recorder::SessionRecorder`）像持有 checkpoint 那样持有每个请求
的用量。`record_usage` 在请求完成时把它按历史长度记下
（`session::RequestUsage::after`），下一次 `sync` 写出它，而这些行按那个位置摆放
（`session::history_lines`）。这个位置正是让文件在它被重写的两种方式下都保持诚实
的东西：

- **一次重写**（一次回溯、一次中断的撤销、一次重试撤回的尝试、一个被 block 的
  prompt）从历史重新序列化整个文件。每一条位置仍然存在的 usage 记录都被放回它原
  来的位置（`session::retain_usage`）。在被切掉的地方之后，对话自己的请求随它们
  所做的那些回合一起走，正如那些回合自己的行那样。subagent 的请求留下，移到剩下
  内容的末尾：无论它周围的对话变成了什么，它都被计了费，而一个后台 agent 的请求
  可能在一个随后被切掉的回合里完成（一次被撤销的提交、一次被撤回的尝试），而该
  agent 还在继续跑。
- **一次恢复**接收这个文件。`session::parse_usage` 把那些 usage 行读回来，每条
  放在文件里位于它之前的那些历史条目之后，所以之后一次重写会把它们留在原地。
  载荷不持有位置：文件的顺序就是它的记录。

循环在对话自己的各个请求的 `Usage` 事件到达时记录它们（`tui::stream`），
subagent 的那些由它自己的到达时记录（`tui::agent`）。每次循环迭代折叠一个回复
事件，而 recorder 在每一次的底部同步，所以那一行就落在它之前记录的那些条目之后。

### 不记录什么

- **一个 provider 没发用量的请求。** 这一行是 provider 的报告，绝不是估算，所以
  一个省略了 usage 帧的 provider（或离线 demo）不留任何行。codex 的记录同样尽力
  而为。
- **auto 模式的 classifier 检查。** 它们是对这个 session 的 provider 的请求，但
  classifier 什么都不报回循环，所以它们今天不在任何统计里：不在回合的、不在
  `/context` 的，也不在 rollout 的。
- **subagent 自己的 tool 调用。** rollout 把 subagent 保留为它的 `agent_group`
  条目，从不保留它的 transcript（`docs/agents/agent-tool.md`），所以它那些调用的
  退出码与时长不在文件里。它那些请求的用量在。

## 兼容性

- **旧文件读起来一如从前。** 每个新字段都是 `serde(default)` 的，而这个构建不认
  识的 `error_kind` 读作缺席，而不是让那一行失败，与未知角色和状态所遵循的规则
  相同。
- **旧构建读新文件。** 未知字段被忽略、未知记录类型被跳过（`docs/state/resume.md`
  里的向前兼容契约）。一个重写该文件的旧构建（比如在一次回溯之后）会丢掉它从
  不知道的 `usage` 行，正如这样一次重写会丢掉它跳过的任何行。
- **summary** 什么都没有变：它的每回合合计保持原样，所以读它们的人不受影响。

## 读一个 rollout

从一个文件里得到按种类分的失败率，以及每个请求的代价：

```bash
jq -r 'select(.type=="tool" and .payload.ok==false) | .payload.error_kind' rollout.jsonl | sort | uniq -c
```

```bash
jq -c 'select(.type=="usage") | .payload' rollout.jsonl
```

## 测试

- `session` 自己的测试钉住格式：三个 tool 字段（写出、缺席时省略、往返、未知的
  `error_kind` 读作缺席）、`usage` 行的形状（`agent` 与 `compact` 只在它们说出
  点什么时出现）、位置的合并（`history_lines`）、一次重写保留什么
  （`retain_usage`）、读回（`parse_usage`），以及 `parse_session` 跳过这一行。
- `llm::agent` 的测试用一个假 executor 驱动 `run_agent`：解决事件上结果的退出码
  与一段实测时长、`tool_error` 作为默认种类、每次拒绝的种类且没有时长，以及一次
  交给后台的调用计到交出的时刻、没有退出码。
- `llm::exec` 的测试跑真实命令来验退出码（管道上和终端里的 `exit 3`、一次看到该
  退出的 session 等待与 kill、管道 runner 上的一次超时），`llm::approval` 与
  `llm::ask` 钉住每次拒绝的种类，`agents` 钉住 subagent 折叠，而 `app::tests`
  钉住中断与 backend 错误的戳。
- `tests/headless.rs` 用真实的二进制对着替身 provider 运行，并读它写出的
  rollout：每个请求一行 `usage`，按顺序围在 `tool` 行两侧，那一行的
  `exit_code`、`duration_ms` 与 `error_kind`，回合的 `summary` 是这些行的合计
  —— 以及，在一次 `--continue` 之后，第一个回合的行在原位、第二个回合的在其后。
  它还读一个 subagent 在其 id 之下的各个请求、一次压缩被标 `compact` 的请求，
  以及一次 headless 拒绝记录为 `approval_required`、没有退出码也没有时间。
- `smoke.sh` Phase 19 从一个真实 session 读 `!` 命令的记录：每一次的退出码与
  时间、`exit 3` 上是 `tool_error`，以及一次被 Esc 的 `sleep 9` 记录为
  `interrupted`，带着它跑过的一秒多、没有退出码。
