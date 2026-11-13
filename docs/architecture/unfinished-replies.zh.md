# 回合绝不以一条不是回复的东西结束

agent 循环（`llm::agent::run_agent`，`docs/tools/tools.md`）以前用一条规则判定
回合结束：**一轮没要求任何 tool 的，就是模型的回答。** 这条规则让回合结束在什么
都没回答的轮次上：

```
● I'll look at the current model first, so I know what's wrong with it.

Called Brave-devtools 2 times (ctrl+o to expand)

Thought for 3m 42s · 4.1k tokens (ctrl+o to expand)

Conjured for 4m 33s · 95.3k tokens (60k cached · 3k written)

❯ continue
```

两轮 tool，然后只思考了一轮 —— 回合就在它的摘要上关掉了，什么都没说，直到用户
敲了 `continue`。现在，一个结束得**没完成**的轮次会由循环按用户会用的方式回应，
在同一个回合里，而一个仍然什么也没拿到的回合会用红字说出来，而不是看上去像做
完了。

## 什么让一轮结束却没有完成

五件事，每一件都是 provider 文档记载的一种停止：

- **空回复。** 模型以没有文本也没有 tool call 的方式结束它的回合。Anthropic 为
  Claude 记载了它 —— 「exactly 2–3 tokens with no content」、
  `stop_reason: "end_turn"`、「particularly after tool results」—— 而其他模型
  也在 tool 轮之后被报告过同样的情况。流本身没有任何问题；模型就是停下了。
- **输出上限，耗在思考上。** 推理与回复共用一个输出预算，所以一段很长的思考可以
  在回复写出一个字之前就把它花光。provider 说了这件事 —— Chat Completions 与
  Ollama 上的 `finish_reason: "length"`、Anthropic 上的
  `stop_reason: "max_tokens"`、Responses API 上一个
  `incomplete_details.reason` 为 `max_output_tokens` 的 `response.incomplete`
  —— 但循环从来没读过它。屏幕上那段思考可以看起来很小，而这件事正在发生：
  Anthropic 的 `display: "summarized"`（`docs/providers/claude.md`）流式的是
  思考的摘要，不是思考本身；而一个套用自己偏低默认输出上限的 chat provider
  （应用在那里不发 `max_tokens`，`docs/providers/prompt-caching.md`）在一次
  不大的思考之后就撞上它。
- **输出上限，在回复中途。** 同样的停止，但已经显示了一部分回复：回合结束在半句
  话上，看起来像个答案。
- **上下文窗口。** Anthropic 的 `model_context_window_exceeded`：回复在写完之前
  就填满了模型的整个上下文。
- **内容过滤器。** `finish_reason: "content_filter"`（以及 Responses API 的
  `content_filter` 理由）：provider 省掉了回复的其余部分。

## 在线上格式说话的地方分类，在循环运行的地方决定

**线上格式的那一半**是一个问题，每种拼写对应一个答案。
`StreamOutcome::stop_reason`（`src/llm/openai.rs`）把每一种线上格式的 finish
reason 折叠成 crate 自己的 `StopReason` —— `Finished`、`OutputCap`、
`ContextWindow`、`ContentFilter` ——
大小写不敏感，所以一个把 provider 的 `MAX_TOKENS` 原样透出来的 shim 读起来也
一样。Responses 的排空现在把 `incomplete_details.reason` 作为 finish reason
带上（以前只保留光秃秃的状态 `incomplete`，它说明了一个响应没完成，却没说
为什么）。完全没有理由 —— 一个从不发送它的 shim —— 就是完成了。

`llm::backend::round_outcome` 随后把一条结束的流映射成循环据以行动的东西，纯的，
所以每个分支都是一个单元测试：

| 流 | 那一轮 |
| --- | --- |
| 要求了 tool | `ToolCalls` —— 无论停止理由是什么，一次已完成的调用都会运行 |
| 填满了上下文窗口 | `Failed`：对话也没有余量再做续写了，而消息指向 `/compact` |
| 被内容过滤器停下 | `Failed`，并指名那个过滤器：再问一次会遇到同一个过滤器 |
| 撞上输出上限 | `CutOff { text }`，保留到目前为止的回复 |
| 其他 | `Complete { text }` —— 空或不空 |

一个**空的** `Complete` 是否可以结束回合，刻意不是线上格式的问题：它在每个
provider 上都是同一个事实，而循环正是回合其他结束规则早已居住的地方。

**循环的那一半**（`run_agent`）把两种轮次视为未完成 —— 文本为空或全是空白的
`Complete`，以及任何 `CutOff` —— 并各用一条**写进一条新用户消息的续写 prompt**
来回应：

- `Empty reply`：*[system] Your last response was empty: it had no reply
  and no tool call. Continue with the task — no apology, no recap. If the
  task is already done, reply with a short summary of what you did.*
- `Reply cut off`：*[system] Your last response was cut off at the output
  token limit. Continue directly from where it stopped — no apology, no
  recap. Keep your reasoning short, and break the remaining work into
  smaller steps.*

`[system]` 标签就是推导出来的上下文给应用所说的每一条 notice 加的那个
（`docs/architecture/context.md`）：prompt 骑在一条用户消息上，不打标签的话模型
会把应用的话读成用户的话。

被截断之前说过的内容作为一条 assistant 消息保留，所以续写从它停下的地方读起。
一条**空的**回复从不会被送回去：Anthropic 的停止理由指引说得很明确 ——
「Sending the empty response back won't help」，而办法是「add a continuation
prompt in a new user message rather than retrying with the empty response」
—— 何况在那种线上格式上，一条空的 assistant 消息本来就会被拒。空回复的 prompt
给模型留了正确收尾的路子（*reply with a short summary*），因为一个因为做完了
而停下的模型需要说出来，而不是编出更多活。

这条 prompt 用 `Stop` hook 的续写已经在用的那个事件记录，`StreamEvent::HookNote`
（`docs/tools/hooks.md`），标签就是上面那个。这一个选择就是整个集成，因为那个
事件所做的一切正是续写所需要的：

- 循环先冲刷它之前流式出来的文本，所以续写作为它自己的消息流出（不变量 4）；
- 它记录无单元格的 `HistoryItem::HookNote`，所以 **Ctrl+O transcript 会显示回合
  为什么继续下去** —— prompt 之上的 `● Empty reply` —— 而 inline 视图仍然和一次
  hook 续写之后一样干净；
- `context::context_messages` 把它作为模型读到的用户消息重放，所以下一个回合的
  上下文 —— 以及保留的线上格式前缀（`docs/providers/prompt-caching.md`）——
  与发出的请求一致；
- rollout 让它往返，所以 `/resume` 能恢复它；
- subagent 的折叠和它的 `RunRecorder` 已经处理它
  （`docs/agents/agent-progress.md`），而 `/compact` 回合的注记从不进入对话的
  记录，所以一个回来是空的摘要只是晚一轮到达。

## 有界，用尽时大声说出

`MAX_CONTINUATIONS`（2）数的是**连续的**未完成轮次 —— 总共三次尝试，正是
Anthropic 自己的续写示例所用的界（`max_attempts=3`）。一个取得进展的轮次 ——
一次 tool call，或者一条 `Stop` hook 送回来要求更多的真答案 —— 把预算挣回来，
所以一个时不时卡住的长任务从不会因此被截断，而一个完全不回复的模型最多花掉三个
请求，而不是一个无尽的循环。Esc 随时停下它，就像它停下一切；一个正好在未完成
轮次回来时被取消的回合，不会再要求任何东西。

连续的第三个未完成轮次会**以失败的方式**结束回合 —— 通过
`StreamEvent::Error` 的一条红色 notice，`StopFailure` 先触发
（`docs/tools/hooks.md`）：从不回来的回复是 `unknown`，一直被打断的回复是
`max_output_tokens`（Claude Code 给它的错误类型）。每条 notice 都带着它自己的
出路，因为用户看到的是一行光秃秃的红字：

- *The model stopped without replying, even after being asked to continue 2
  times. Send a message to try again, or switch models with /model.*
- *The model's reply kept getting cut off at its output token limit, even
  after 2 continuations. Ask for the work in smaller steps, or lower the
  thinking effort with Ctrl+T.*

模型下一回合也会读到它，作为每条红色 notice 都推导出的那条 `[error] …` 条目
（`docs/architecture/context.md`）。

`Stop` 与 `SubagentStop` 只在真正的回答上触发 —— 从不在空轮次上，也从不在一个
预算用尽的回合上 —— 所以 hook 看到的正是它被写来检查的那条回复。

## 它看起来是什么样

在 inline 里，空轮次的 `Thought for …` 单元格停在它安定下来的地方，下一轮在它底
下流式 —— 它自己的思考、它的回答，然后是回合的摘要。续写本身不往 scrollback 里
加任何东西：像 hook 的续写一样，它是 transcript 该显示的（Ctrl+O），也是上下文
该携带的（Ctrl+D）。

## 为什么不用别的办法

- **重发同一个请求。** 空回复就是那个请求产出的东西；Anthropic 的指引把这么做
  点名为错误，而一份花在思考上的上限会被再花一次。
- **提高输出上限。** 应用不保留逐模型的输出上限 —— 一个模型的条目存的是它的上
  下文窗口，一个不同的数字（`docs/providers/claude.md`）—— OpenAI 的推理模型
  拒绝 `max_tokens`，要的是 `max_completion_tokens`，而 OpenRouter 在运行一个
  请求之前就按账户额度把整个上限预留掉（`docs/providers/prompt-caching.md`）。
  一次续写在每一种线上格式上都按原样可用。
- **立刻显示一个错误。** 用户接下来的动作几乎总是 `continue` —— 所以循环就替他
  做了，并把错误留到它帮不上忙的时候。

## 已知限制

- **一次续写就是一条新消息。** 一条断在半句的回复再续写，读起来是两行要点，第二
  行从第一行停下的地方接着来。subagent 的回答同样是它的最后一条消息，这也是
  `SubagentStop` 续写本来就遵循的规则：截断之前的部分在它的记录里作为一次进展更
  新（`docs/agents/agent-progress.md`）。
- **Chat Completions 上撞上上限的 tool call** 保留它被截断的参数并运行，解析失败
  —— 模型读到那个错误再试一次，和以前一样。
- **预防不在这里。** Anthropic 列出的空回复第一成因，是紧跟在 tool 结果之后的一
  个用户文本块，它教会模型它的回合在那里结束。一个 tool 返回的图片（`read` 读到
  一张图、一次 MCP 截图）今天就正好骑在那个形状上：`[image] …` 注记和它的图片
  作为一条用户消息跟在结果之后，Anthropic 的线上格式会把它并进 `tool_result`
  消息（`docs/tools/tools.md` *Image reads*）。把图片搬进结果本身（那里
  Anthropic 接受它），会在那种线上格式上消除这个模式；那是一次横跨消息模型、四
  种线上格式和上下文重放的独立改动。

## 测试

- `llm::agent::tests` —— 循环：一条空回复、一条全空白的、一条被截断的有文本和没
  文本、`Stop` 只听得到回答、每种放弃及其 `StopFailure` 类型、进展把预算挣回来、
  未完成轮次下的一次 Esc、各 prompt 的 `[system]` 标签，以及各错误的措辞。
- `llm::backend::tests` —— 每个停止理由的 `round_outcome`，以及 `stream_round`
  对一个在自己的输出上限处停下的回环替身。
- `llm::openai::tests` / `llm::responses::tests` —— 每种线上格式对一次停止的
  拼法，以及 Responses API 的 `incomplete_details.reason`。
- `tests/wire_history.rs` —— 通过一个真实 backend 端到端：空回复之后的请求带着
  prompt 而不带任何回复内容，被截断之后的请求带着已说的内容然后是 prompt，而下一
  个人类回合把两者按 provider 看到的样子发出去 —— 截断那一种在一个重建过的
  backend 上，仅凭记录。
- `tests/live_unfinished.rs` —— 通过一次 Console 登录的 Anthropic Messages 线上
  格式，`#[ignore]`（轮换的 refresh token 会写回
  `ZEROTH_LIVE_TOKEN_STORE`）：一个 40 token 的上限让一次从 1 数到 30 的计数在
  中途停下并带 `stop_reason: "max_tokens"`，而续写在同一个回合里把它做完（写成
  时，第一轮停在 20，第二轮从 21 走到 `DONE`）；一条空回复被续写时发去的请求
  —— 问题和 prompt 并排 —— 得到了回答。
- `scripts/smoke/phases/146-emptyreply.sh` —— 真实二进制对着各种 stub provider：
  有的什么都不回、有的一边思考一边撞上上限、有的从不回复：恢复出来的答案、
  `Thought for` 单元格、Ctrl+O 与 Ctrl+D 里的 prompt、请求次数，以及指名
  `/model` 的那条红色 notice。
