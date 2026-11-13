# prompt 缓存与真实用量记账

两个特性一起落地，因为第二个正是你*看到*第一个在起作用的方式：

1. **prompt 缓存** —— 每个请求都被整形成这样的形状：provider 能从它自己的缓存
   里供给重复的 prompt 前缀（system prompt + 到目前为止的对话）：provider 隐式
   缓存的地方自动完成，不隐式缓存的地方靠显式的 `cache_control` 断点，并且带上
   每个 backend 真正据以路由的**按 backend 的亲和提示**。一个 agent 会话每一轮
   都会把整段不断增长的上下文重发一遍，所以这决定了是每轮都为整段对话重新付费，
   还是只为前缀付约 1/10（`claude-haiku-4.5` 上 8k token 的前缀，写一次约一
   美分，读回来是那个数的十分之一）。
2. **真实用量记账** —— 请求会索要 provider 最终的 `usage` 帧（chat wire 上是
   `stream_options: {"include_usage": true}`；另外三种 wire 不用索要也会上报），
   app 的 token 计数随即吸附到它上面，而不再相信 app 侧的估算 —— 那个估算从没
   见过 system prompt，没见过重发的上下文，也没见过缓存折扣。于是提交下来的回合
   摘要读作 `Done for 12s · 8.2k tokens (8k cached)`，或者在*预热*了缓存的
   那个回合上读作 `(8k written)` —— 这就是缓存生效的可见证据。

两者都按各厂商自己的指南来适配 provider —— OpenRouter 的
[prompt 缓存最佳实践]、Venice 的 [prompt 缓存功能指南]、Anthropic 的
[prompt 缓存]参考 —— 并且由 `tests/live_caching.rs` 逐 provider 在 wire 上
**验证过**（表格在文末）。

[prompt 缓存最佳实践]: https://openrouter.ai/docs/guides/best-practices/prompt-caching
[Venice 的 prompt 缓存功能指南]: https://docs.venice.ai/guides/features/prompt-caching
[prompt 缓存]: https://platform.claude.com/docs/en/build-with-claude/prompt-caching

## 真实的还是估算的？两者都是，而下面的表说明哪个是哪个

| 数字 | 它来自哪里 |
| --- | --- |
| 状态行的 `↓ N tokens`，位于两次 usage 帧**之间** | app 侧对已流式传出内容的估算（chunk、推理、tool 输出、正在生成的 tool call）—— 不依赖词表，在代码和英文上与常见 encoder 相差几个百分点以内（`docs/architecture/token-estimate.md`） |
| **每个回合边界**上的计数、`Done … · N tokens` 回执，以及它的 `(N cached · N written)` | provider 自己的 usage 帧 —— 从不估算；不上报 usage 的 provider（dummy、把 usage 丢掉的 shim）让回执保持光秃秃的，而不是去猜 |
| **被打断**回合的回执，`Worked for 8s · ≈24.3k tokens` | Esc 落下时状态行显示的那个计数：已结束各回合的帧，加上被 Esc 截断那个回合的估算 —— 那个回合永远不发自己的帧 —— 只要总量里包含这份估算就打上 `≈`，缓存那两半仍然是各帧自己的（`docs/input/interrupt.md`） |
| 页脚的上下文量表 | 每个回合都是真实的：最近一帧的 `input`，再加上它之后加入的内容的估算，都用 provider 的 token 计 —— 由各帧来校准那份估算（`docs/architecture/token-estimate.md`） |
| 历史被改动后（`/clear`、回溯、`/resume`、压缩）的量表 | 最近一帧的计数按校准过的变更估算移动，直到下一帧到来；在任何帧之前则只有估算 |
| `Thought for … · N tokens` 单元格 | 帧里带 `reasoning_tokens` 时吸附到它上面（OpenAI、OpenRouter、Responses wire）；不带时用估算（Anthropic 把思考折进 `output_tokens`，Ollama 什么也不报） |
| subagent 的名单计数与它自己的回执 | 对它们自己的帧套用同一条规则（`docs/agents/agent-context-gauge.md`） |

所以缓存相关的数字**永远是真实的**：app 从不估算缓存份额，回执上的
`(N cached)` 就是 provider 自己声称它从缓存里供给了多少。

## 两个缓存世界

OpenAI 兼容的 provider 分成两类：

- **隐式** —— OpenAI、DeepSeek、Grok、Gemini 2.5+、Moonshot、Z.ai 和
  **Venice** 自动缓存重复的前缀；请求不需要任何标记。Venice 甚至会为它服务的
  Claude 模型自己注入 Anthropic 的标记 —— 已在 live 上验证：一个对
  `claude-sonnet-4-6` 不带任何标记的请求在第 1 回合写了 7998 个 token，第 2
  回合把它们读了回来 —— 所以我们自己再加就可能翻倍、越过 Anthropic 的 4 个
  断点上限，因此 Venice 服务的裸 `claude-*` id 被有意留作隐式。
- **显式** —— Anthropic 与 Qwen（Alibaba）经由 OpenRouter 风格的聚合器访问
  时，只缓存到标了 `cache_control: {"type": "ephemeral"}` 的文本块为止的前
  缀。没有标记 → 没有缓存 → 每个 agent 回合都按全价为整段对话重新付费。

纯的 [`llm::cache`](../../src/llm/cache.rs) 模块拥有这个划分：
`needs_cache_breakpoints(model)` 对带命名空间的 `anthropic/…` 和 `qwen/…`
id **以及它们的 `~vendor/model-latest` 别名**为真 —— OpenRouter 的
`~anthropic/claude-haiku-latest` 在 Amazon Bedrock 上解析为
`anthropic/claude-haiku-4.5`（响应自己的 `model` 字段就是这么说的），而这条
路由不加标记就什么也不缓存；这个嗅探过去会漏掉那个 `~`，于是一个别名 id 每回合
都付全价 —— 而 `apply_cache_breakpoints(messages)` 标记的是这一轮里
`ChatMessage` 的**副本** —— 带类型地标记，打在文本 part 的 `cache_control`
字段上（没有 `skip_serializing_if`，所以其他任何 provider 的载荷都保持逐字节
不变）。它过去是改写序列化后的 JSON 树，那意味着每回合都要为整段对话构造一个
`Value` —— 每次都把一张粘贴图片的几兆 base64 复制一遍
（`docs/architecture/memory.md`）；而它现在标记的副本是浅的，图片按引用共享。

嗅探读的是一个模型 id，所以只有在 id 表达了某个聚合器的路由时，它才意味着什么。
一个**无 key** 的 provider（`auth = "optional_key"`）是用户自己跑的服务器，
无论它的 id 长什么样都永不被标记：LM Studio 把模型命名为 `publisher/model`
（`qwen/qwen3-8b` 恰好就是聚合器的那种拼法），vLLM 则提供 Hugging Face 仓库
id（`Qwen/Qwen3-8B`），大小写不敏感的嗅探会把它们读成同一个东西。一个不按
prompt 缓存计费的服务器，断点买不到任何东西，所以它的请求保持每个 OpenAI 兼容
服务器都能读的朴素形式（`docs/providers/local-models.md`）。这道闸门属于载荷
构造器（`OpenAiClient::chat_request`），而不属于嗅探：嗅探只回答一个关于模型
id 的问题，provider 才决定这个问题是否适用。

Anthropic **自己的** API（`wire_api = "anthropic"`，
`docs/providers/claude.md`）不需要模型 id 嗅探 —— 在那边缓存*永远*是显式的
—— 所以 `anthropic.rs` 在自己的形状上放同样的三个断点：最后一个 system 块、
最后一条消息的最后一个内容块，以及再往前那条用户消息的最后一个块。

## 断点位置（Anthropic 的 4 个中最多用 3 个）

```
[system ← ①] … [previous request's final message ← ③] [assistant+tool_calls] [tool ← ②]
```

1. **system 消息** —— 这个会话里每个请求都共享的巨大稳定前缀
   （persona + environment）。
2. **最后一条可缓存消息** —— 一个*移动*的断点，追踪对话前沿。回合中途，每个
   agent 轮次都以该轮的 tool 结果收尾，所以下一轮能从缓存里读到目前为止的
   一切，只有新结果是新鲜输入；跨回合时它落在最新的用户消息上。已在 live 上
   验证：OpenRouter 的 Anthropic 路由接受打在 `tool` 角色消息上的标记（一个
   Qwen 的 tool 轮次带上它也能跑完）。
3. **上一个请求的前沿** —— 最新一条 assistant 响应之前的那条最后可缓存消息，
   其中可能包含上一轮的一个 tool 结果。只钉住最后一条人类消息，会在新内容超出
   provider 的回看窗口时丢掉其间的 tool 上下文。在原生 Messages wire 上，
   tool 结果会变成 user 块，所以它之前那条用户消息已经标识了那个边界。Claude
   自己的 API 把连续的 tool-use 与 tool-result 串算作一个回看位置；光是一个大
   批次并不会耗尽那个 API 的窗口，但一段很长的文本/图片后缀会。

标记会把纯字符串内容转成单文本 part 的数组（这是唯一能携带 `cache_control`
的形状）；parts 数组（一条 vision 消息）则把标记打在它的**最后一个非空文本
part** 上 —— 图片 part 带不了标记。空的和纯空白的内容会被跳过（provider 会
拒绝空的已标记文本块）；前沿会往回走过它们。最小尺寸规则（依模型而定，1k–4k
token）是 provider 自己的：小对话上的标记直接被忽略，直到前缀长过那个门槛，
所以 app 侧没有什么需要设闸。

重复施加这个变换会替换掉它旧的标记，而不是累积。空白的 system/developer 文本
在原生 Messages wire 上会被省略 —— 在那里，一个空的已标记块会让请求被拒。

## 保持对话前缀稳定

新的用户回合发出的请求是从显示历史派生出来的，而它过去会在两个地方与 provider
已经缓存的那个请求不同，代价是整段后缀：派生过程会在那些把 id 原样透传的 wire
（Responses、Messages、Ollama）上铸造新的 `call_0`、`call_1`… id，并且把一个
并行批次拆成每个 call 一对 assistant/result。信息等价，前缀不同 —— 而缓存读取
就是一次前缀匹配。

现在有两层保住这个前缀。**durable** 的那一层是记录
（`docs/architecture/context.md`）：每个 tool 轮次先公布它的 id
（`StreamEvent::RoundCalls`，按模型的顺序），循环把它们盖到该轮的 cell 解析成
的记录上，旁边还有一个按轮次的批次号（`ToolCall::call_id`/`batch`、
`TaskCallRecord` 的那一对、agent 组里带着它的启动所应答的 `call_id` 和逐字
`arguments` 的那个条目，两者都随启动的 `AgentSpec` 携带）**以及每个 call 在
这一轮里的 `position`** —— 模型给它的下标，批次公告、task call 的轮次和启动的
id 各自都能从公布的列表里恢复出来 —— rollout 会把这一切往返一遍，而
`context::context_messages` 把共享一个批次的记录折回它们当初到达时的那一条
`tool_calls` 消息，**并重新按模型的顺序排序**：一个 subagent 组在该轮的普通
call 运行之前就已解析，所以 `[bash, agent, read]` 会记成
`[agent, bash, read]`，而如果没有 position，重放就会按那个顺序把 call 及其结果
发出去 —— 与 provider 缓存的那个前缀不同，从那一轮直到最后。普通调用、task
调用和 subagent 启动都一样，每一个结果都排在 call 之后、顺序同一，批次中途提交
的 notice 则被推迟到批次之后。所以派生出的请求*就是*缓存的那个，无论在
`/resume`、`/settings` 或 `/model` 重建 backend 之后，还是在一次重启之后；在
引入这些字段之前写下的 rollout，其记录会铸造一个避开已记录 id 的合成 id，所以
老会话的后续回合仍然能配对。**retained** 的那一层是上一个请求本身：活跃的
backend 保留它最后一次请求，并在重建后的历史具有相同文本、图片、有序 tool 名、
参数和结果时恢复那个精确的前缀 —— 这还能覆盖任何记录都覆盖不了的东西：一个
批次 call 之前只有空白的引导（Responses 与 Messages wire 会把模型的 `\n\n` 原样
回显成本轮的内容，而 app 不为它记录任何片段，过去把两者当作不同，恰恰让这两种
wire 永远匹配不上）以及相邻的同角色纯消息边界。会拦住 prompt 的
`UserPromptSubmit` hook（`docs/tools/hooks.md`）在查询 retained 前缀之前就被
检查，所以这次拦截不花它任何代价，下一个 prompt 依然匹配。任何不匹配都改用新
派生的历史，所以编辑、回退和压缩都无法复活过期的上下文。保留量上限 8 MiB
（图片共享它们既有的分配）；backend 重建或重启会丢弃它并回落到记录。
MCP tool 定义使用稳定的名字顺序，所以发现顺序不会改变一个 prompt。

`tests/wire_history.rs` 让 Chat Completions 与 Responses 两种 wire 对着一个
loopback 替身端到端跑起来 —— 一个真实回合，它的事件完全按循环折法折进 `App`，
下一回合的请求从 wire 上读回来时带着 provider 自己的 call id —— 并且单独证明
durable 那一半：在回合之间重建 backend，它 retained 的请求没了，仍然发出
provider 见过的那个批次，而一个被 hook 拦下的 prompt 也不会让前缀白花。
它那个 `#[ignore]` 的 live 孪生测试在真实缓存上度量这一半
（`live_openrouter_a_rebuilt_backend_reads_the_tool_round_back_from_cache`，
OpenRouter 的 Anthropic 路由）：一个真实的 `bash` 回合折进 `App`，然后一个新
backend 的第一个请求读回了它 9,561 个输入 token 中的 9,540 个 —— 那个 tool
轮次，在 provider 自己的 `toolu_…` id 下，仅凭记录还原（测量于 2026-09-21）。
它在 Responses wire 上的孪生测试
（`live_chatgpt_a_rebuilt_backend_reads_the_tool_round_back_from_cache`，一个
800 行的 `seq` 结果，好让这一轮跨过 OpenAI 的若干个 128-token 块）在
`codex-auto-review` 上读回了 9,067 中的 8,704 —— 越过了 system prompt、
穿过了 tool 结果，因为那一次运行模型没有发出空白引导；空白引导那种形状仍然只
属于 retained 请求。

## 缓存亲和：每个 backend 据以路由什么

一次缓存命中要求请求落在缓存*所在*的地方。边界为每个进程铸造一个 key
（`tui::config::session_cache_key` —— pid + 启动时间，本会话内构建的每个
backend 都是同一个 key），每个 backend 以它真正据以做 key 的形式拿到它：

- 请求体里的 **`prompt_cache_key`** —— OpenAI 的标准参数，已验证 OpenRouter
  和 Venice 都接受（后者把它写作文档里的缓存亲和提示），在 Responses wire 上也
  会发送。
- 请求体里的 **`session_id`** —— OpenRouter 自己的会话钉，**只**在 chat 的
  `api_base` 就是 OpenRouter 的（base URL 里有 `openrouter` 标记）时才发送：
  OpenRouter 会把一个模型路由到多个上游 provider，而写在 Amazon Bedrock 上的
  缓存从 Google Vertex 读不到。Venice 会拒绝未知的请求体键
  （`Unrecognized key(s): 'session_id'` —— 已在 live 上验证），所以这个钉
  保持设闸状态。
- 作为 header 的 **`session_id` + `conversation_id`** —— ChatGPT backend
  （`chatgpt.com/backend-api/codex`，`docs/providers/chatgpt.md`）。这一个
  **忽略请求体里的键**：在一个一模一样的 6.7k token 前缀上实测，app 过去发送
  的那种请求在第 2 回合从缓存读到 **0** 个 token（中间停顿 12 s，以排除写入
  慢），而同样的请求一旦带上 Codex CLI 的那两个身份 header 就读到 **6400**
  —— 而单加 `OpenAI-Beta: responses=experimental` 什么也没改变。所以
  `chatgpt::session_headers` 把 session key 放在这两个名字下，
  `openai::chatgpt_request_headers` 负责附上它们，闸门与 Copilot 的按请求
  header 一样，精确地按 `ChatGptCodex` 认证方案设：一个粘贴 key 的 provider
  永远看不到它没要的 header。
- **什么也不带** —— GitHub Copilot。它的代理对无法识别的请求体形状回以
  `model_not_supported`，而它的缓存是服务端的；这个 key 只是为了支持工作才搭
  `X-Request-Id` 的车（`docs/providers/copilot.md`）。

## 用量流水线

```
build_payload: stream_options.include_usage      (openai.rs — kwargs can override)
        ↓
final SSE frame: {"usage": {prompt_tokens, completion_tokens,
                            prompt_tokens_details: {cached_tokens, cache_write_tokens, …},
                            cache_read_input_tokens, …}}     (null on delta frames)
        ↓ parse_sse_usage → StreamOutcome::usage             (openai.rs, pure — all alias
        ↓                                                     spellings normalized)
stream_round → StreamEvent::Usage(TokenUsage)                (backend.rs — one per round)
        ↓
App::apply_usage: accumulate + SNAP status.tokens            (app/status.rs, pure)
        ↓
TurnSummary { tokens, cached, cache_write }
        → "Done for 12s · 8.2k tokens (8k cached · 1.2k written)"
                                                             (ui::summary_lines; persisted
                                                              by session.rs, serde-defaulted
                                                              so old rollouts still parse)
```

- [`TokenUsage`](../../src/stream/event.rs) 携带 `input`、`output`、`cached`
  （读取量，按 OpenAI 的记账形状是 `input` 的一个子集）、`cache_write` 和
  `reasoning`。每种 wire 的解析器都落到同一个 struct 里：
  - **Chat Completions**（`openai::parse_sse_usage`）把拼写动物园归一化：
    `prompt_tokens_details.cached_tokens`
    （OpenRouter/OpenAI/Venice）、顶层的 `cache_read_input_tokens` /
    `cache_creation_input_tokens` 别名（Anthropic 风格的 shim）、
    OpenRouter 的 `cache_write_tokens` 以及 Venice 嵌套的
    `cache_creation_input_tokens`。一个 `"usage": null` 的 delta 帧或一个
    全零块不算上报。
  - **Responses**（`responses::parse_usage`）：`input_tokens` /
    `output_tokens`，加上 `input_tokens_details.cached_tokens` 和
    `output_tokens_details.reasoning_tokens`。
  - **Messages**（`anthropic::MessageAccumulator::merge_usage`）：Anthropic
    把 `input_tokens` 上报为*未缓存的余量*，所以 `input` 是它与两个缓存数字
    之和；这些计数在 `message_start` 上落定，并在 `message_delta` 上
    **累加式地重复**（已在 live 上验证：
    `{"input_tokens":10,"cache_creation_input_tokens":7998,"cache_read_input_tokens":0,"output_tokens":5}`），
    而合并是**按计数器单调**的 —— 后来的帧只能报得更多，某个帧省略了某个计数器
    就保持原值 —— 所以一个只说了余量、不带缓存键的 delta（较早文档描述的形状）
    永远不能把缓存份额归零，也不能把整个 prompt 的 `input` 缩到几十个 token。
  - **Ollama**（`ollama.rs`）：最终帧的 `prompt_eval_count` /
    `eval_count`，即使命中缓存也是整个 prompt；这个 API 完全没有缓存记账，
    所以缓存份额诚实地是零。
- **吸附，不要累加**：app 侧的估算在帧与帧之间继续实时跳动（chunk、推理、
  tool 输出 —— 不变），但每轮的 usage 帧会把计数**替换**为累积的真实总量
  （`App::apply_usage`）。所以状态永远是「到目前为止计费的一切，加上当前轮次
  的实时估算」，而最终数字是 provider 自己的记账。计数会在回合边界*跳高* ——
  那是估算在得知它从未计入的 system prompt 和重发的上下文；这是真相，不是
  故障。
- 回执的括号里显示该回合各帧相加的两个缓存半边 —— `(8k cached)`、
  `(8k written)`，或者在一个读了旧前缀、写了新前缀的 agent 回合上是
  `(24k cached · 2k written)` —— 为零的那半边被省略，两者都不适用时没有括号。
  写入的那一半讲的正是*第一个*回合的故事：显式缓存的 provider 按 1.25× 计写入
  的钱，而一份只写 `8.2k tokens` 的回执看起来就像缓存什么也没做。
- 在很长的 agent 回合上计数会到六位数，所以状态行和摘要把它人性化
  （`ui::format_token_count` —— `8.1k`、`154.3k`、`1.2M`；一千以下原样，所以
  dummy 那些小估算渲染如初）。
- dummy 不发任何 `Usage` 事件：`smoke.sh` 的输出 —— 包括它的各个阶段所断言的
  那种朴素的 `Done for Ns` 摘要 —— 保持不变。

## 配置

没什么可配的。`stream_options` 是 provider 唯一可能噎住的那个字段，而
`providers.toml` 的 kwargs 是在基础载荷*之后*合并的，所以一个表现古怪的 shim
可以在文件里覆盖它（`[providers.x.kwargs] stream_options = …` —— 或者把它置为
null）。断点规则按模型 id 定，亲和 key 按 api_base 和认证方案定；新增一个
provider 块不需要任何缓存方面的知识。

## Live 验证

`tests/live_caching.rs` —— 每个 provider 一个 `#[ignore]` 的测试，每个都像
边界那样通过 `ProvidersFile::model_config` 构建自己的配置，并在一段加了盐的
约 5k token 的 system prompt（越过每个 provider 的最小可缓存尺寸）上驱动两个
一模一样的回合，打印两个 usage 帧：

```sh
A0_VENICE_API_KEY=sk-a0-…    cargo test --test live_caching -- --ignored --nocapture live_venice
VENICE_API_KEY=…             cargo test --test live_caching -- --ignored --nocapture live_venice_direct
OPENROUTER_API_KEY=sk-or-…   cargo test --test live_caching -- --ignored --nocapture live_openrouter
ANTHROPIC_API_KEY=sk-ant-…   cargo test --test live_caching -- --ignored --nocapture live_anthropic_api_key
OLLAMA_API_KEY=…             cargo test --test live_caching -- --ignored --nocapture live_ollama_cloud
GITHUB_COPILOT_TOKEN=ghu_…   cargo test --test live_caching -- --ignored --nocapture live_copilot
CHATGPT_CODEX_REFRESH_TOKEN=rt.1.… ZEROTH_LIVE_TOKEN_STORE=/tmp/live.env \
                             cargo test --test live_caching -- --ignored --nocapture live_chatgpt
ANTHROPIC_CONSOLE_REFRESH_TOKEN=… ZEROTH_LIVE_TOKEN_STORE=/tmp/live.env \
                             cargo test --test live_caching -- --ignored --nocapture live_anthropic_console
```

两个 refresh token 会**轮换**的登录，在没有 `ZEROTH_LIVE_TOKEN_STORE` 时拒绝
运行：铸造过程可能让传给它的那个 token 作废，而轮换后的 token 只会写回一个
存储路径 —— 这样跑过之后，该文件里 `*_REFRESH_TOKEN` 键下的值就是要留着的
那个。较早的 `tests/live_openrouter.rs` 缓存测试
（`live_usage_frame_reaches_the_app`、
`live_anthropic_prompt_cache_writes_then_reads`、
`live_qwen_accepts_cache_breakpoints_on_a_tool_round`、
`live_venice_reports_usage_and_hits_its_cache`）也照旧运行。

在真实 wire 上观测到的（2026-09-06），每个回合一个请求：

| provider · 模型 | 第 1 回合 | 第 2 回合 |
| --- | --- | --- |
| Venice（a0 代理）· `openai-gpt-4o-mini-2024-07-18` | `input: 6749, cached: 0` | `cached: 6656` |
| Venice · `claude-sonnet-4-6`，**不带我们的任何标记** | `cache_write: 7998` | `cached: 7998` |
| OpenRouter · `~anthropic/claude-haiku-latest`（→ Bedrock 上的 `claude-haiku-4.5`） | `cache_write: 8004` | `cached: 8004` |
| Anthropic · `claude-haiku-4-5`，粘贴的 key 打在 `x-api-key` 上 | `cache_write: 8004` | `cached: 8004`（`input: 8007`，整个 prompt） |
| ChatGPT · `gpt-5.4-mini`，Responses wire | `cached: 0` | 带 session header 时 `cached: 6400` —— 不带时 `0` |
| Ollama Cloud · `gpt-oss:20b` | `input: 6813` | `input: 6813`，wire 上没有缓存字段 |
| OpenRouter · `qwen/qwen3.6-flash`，断点在 tool 结果上 | 该轮跑完 | — |

有两行无法从验证沙箱里观测，在此标注为**未验证**：GitHub Copilot（沙箱的
GitHub 代理只允许仓库范围的端点，所以在
`api.github.com/copilot_internal/v2/token` 上的 token 交换在任何请求到达
Copilot 之前就被拒了）与 Anthropic Console
登录（手上的 refresh token 已经作废 —— `/login` 会铸造一个新的；它随后说的
那个 wire 与粘贴 key 的一样，上面已验证，只是多一个 OAuth beta header）。

一个 OpenRouter 的怪癖值得知道：它在跑一个请求之前，会按模型的**整个输出上限**
在账户额度里预留，余额不够覆盖就是一个 402 并点出那个数字
（`You requested up to 64000 tokens, but can only afford 41732`）。app 不发
`max_tokens`，所以一个几乎空的 OpenRouter 余额可以干脆拒掉一个大输出模型；
live 测试通过配置的 `extra_body` 设 `max_tokens: 32` 来服务它的一词答复。

## 压力审计（2026-09-20）

确定性套件对每个订阅型 provider 演练 32 个并发授权调用方、64 次连续的 refresh
轮换、32 个并发凭据存储写入者、64 个人类回合（每个带 16 个并行 tool call），
以及批次大小为 1、19、20、21 和 64 的 32 个回合上的断点放置。它还覆盖了大段
文本/图片的后续输入、重复的标记准备、hook 消息合并、一个批次 call 之前只有
空白的引导、被打断回合的 ID 碰撞、变动的历史、保留内存的上限、一个本地时钟读
作已过期的 `exp` 的 bearer，以及一个符号链接的 key store。
`tests/wire_history.rs` 补上端到端那条腿：在 Chat Completions wire 和
Responses wire 上对着一个 loopback 替身跑一个真实 backend 回合，下一回合的
请求带着 provider 自己的 call id —— 来自 retained 请求，以及在回合之间重建
backend 后仅凭记录 —— 并且一个被 hook 拦下的 prompt 不花任何前缀。认证那一半
还覆盖了一个 `forget` 落在 refresh 在途时，以及一次轮换与一次更新的登录争抢
key store。完整的本地闸门通过了：4,037 个测试、格式化、警告即错误的 Clippy，
以及文档构建。

live 测试使用生产环境的 provider 配置和合成的 prompt。同请求基线：OpenRouter
读回了 8,007 个输入 token 中的 8,004 个，ChatGPT 的 `codex-auto-review` 读回
了 6,748 中的 5,888，而 Venice 在某个下午读回 6,749 中的 6,528、在另一个下午
一个都没读回 —— 无论背靠背还是相隔八秒 —— 那是 provider 的缓存，不是请求，
因为两次的请求逐字节相同。

`live_*_growing_conversation_keeps_cache` 这些测试发六个不断增长的回合并打印
每一个 usage 帧。各自断言什么，取决于 provider 的缓存（测试里的
`CacheReads`）：显式断点是确定性的，所以每个热回合都必须读回它的前缀；隐式
缓存是尽力而为且异步的，所以一次运行里如果没有任何热回合读回，就拿它的最后一
个请求原样重发一次来核对 —— 一个 provider 如果读回了*那个*、却一个增长中的
回合都没读回，那是请求把前缀挪走了，两个都不读则说明它今天没在读。它们默认
被忽略，因为 provider 的可用性和缓存落点不在本地测试的掌控之内。观测到：

| Provider/模型 | 五个热回合 | 结果 |
| --- | --- | --- |
| OpenRouter / `~anthropic/claude-haiku-latest` | 每个回合都复用 40,360 个输入 token 中的 40,250 个（99.7%） | 通过，确定性 |
| ChatGPT / `codex-auto-review` | 5 个里 5 个读回 5,888 个 token（86.3%）；更早一次运行 5 个里 4 个，有一个回合什么也没读到 | 通过，尽力而为 |
| Venice 代理 / `openai-gpt-4o-mini-2024-07-18` | 两次运行 5 个里 1 个读回（6,656 个 token），另外两次 5 个里 0 个 —— 其中一次回合之间停顿八秒 —— 同请求基线也一并落空 | 是 provider，不是请求 |

那个恒定的 5,888 是 OpenAI 的记账方式，不是前缀卡住了：它按整个 128-token 块
计数，而一个回合加入的 token 少于一个块。另外
`live_openrouter_marks_the_previous_tool_result_and_reads_it_back` 把新的
断点 ③ 打在对话中段的一个 tool 结果上：OpenRouter 的 Anthropic 路由接受它，
tool 轮次上写入 10,151 个 token，并在下一回合整段读回。没有为了掩盖这些结果
而添加任何编造的缓存计数或生产环境延时；一个 provider 在同请求上的命中并不
证明它在整个增长中的会话里都可靠命中，某一次落空也不构成对请求的指控。

## 已知局限

- 按模型的**最小可缓存尺寸**与 TTL（5 分钟 – 1 小时）是 provider 的；一个短于
  阈值的会话什么也不缓存，而且是静默的。
- `needs_cache_breakpoints` 是一份**前缀清单**（`anthropic/`、`qwen/`，带不带
  OpenRouter 的 `~` 别名标记都算），不是一次能力探测。这些列表确实带有信号
  —— OpenRouter 的 `pricing.input_cache_write`、Venice 的
  `pricing.cache_write` —— 但两者都没说*标记*是否是必需的，所以清单保留；
  将来一个显式缓存的厂商意味着再多一个前缀（而一个*忽略* `cache_control` 的
  provider 只是忽略它而已）。Gemini 2.5+ 有意留作隐式：它的自动缓存不收写入
  的钱，而显式的 `cache_control` 会收。
- 一个显式缓存的 provider 前面的**无 key 代理**（不带 master key、路由
  `anthropic/*` 的 LiteLLM）从这里拿不到标记，因为无 key 被读作用户自己跑的
  服务器。这个代理可以放它自己的标记（LiteLLM 的
  `cache_control_injection_points`），或者把 provider 声明为
  `auth = "api_key"` 配一个占位 key —— 一个什么都不检查的代理会接受它。
- **GitHub Copilot** 从这里无法验证（见上）。它的代理可能为它服务的 Claude
  模型要它自己的 `copilot_cache_control` 标记；在没有可达的交换之前，那只能
  是个假设，不是一次改动。
- **Ollama** 不上报任何缓存记账，所以即使服务器复用了它的 KV 缓存，回执也
  从不显示缓存份额。
- **状态行**显示吸附后的总量，但不显示缓存份额；缓存的那些半边只在提交的
  `Done …` 摘要里露面（以及 Ctrl+O 的 transcript，那里摘要会重新渲染）。让
  实时行的分句集合保持稳定，是为了不为了一个反正只在回合边界才落定的数字，去
  重新教 `docs/ui/status-indicator.md` 的几何。
- OpenRouter 的按请求 `cost` 字段会被解析并越过，但不外露 —— 计数仍然用
  token（Venice 用另一种单位计费）。
- **durable 前缀**重放的是记录能说的东西，而一旦 retained 请求没了（一次
  重建、一次 `/resume`、一次重启 —— 运行中的 backend 的 retained 请求在这
  之前覆盖两者，代价是从那一轮起重新读一遍对话，绝不是一次畸形请求），仍有两
  种回合形状与 wire 不同：被**Max tool calls** 上限拒绝的 task 或 agent 调用
  完全不留下记录（它从未有过 cell），而一个被 `PreToolUse` hook 拦住的启动会
  以一个合成 id 下的孤立 cell 解析在批次之前；另外同一轮里的多次图片读取会重
  放成一条合并的 user 消息，而 live 循环每次读取发一条。
