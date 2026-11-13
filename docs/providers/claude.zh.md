# 把 Anthropic 当作 provider

直接跟 Claude 说话 —— `/login` → **Sign in with an API key** → **Anthropic**，或者
`/login` → **Sign in with an account** → **Anthropic Console**。

自带两个 provider，它们是*同一个 API* 的两种到达方式：

| provider id | 你怎么认证 | 存下的是什么 |
| --- | --- | --- |
| `anthropic` | 从 Console 粘过来的 key | key 本身，原样 |
| `anthropic_console` | 浏览器登录（OAuth PKCE） | 一个**刷新** token |

两者都讲 **Messages** API（`wire_api = "anthropic"`），也就是 Chat Completions
和 Responses 之后的第三种 wire format（`docs/providers/chatgpt.md`）。

## 这不是什么：Claude Pro/Max 登录

Anthropic 有两个 OAuth 客户端，把它们混为一谈正是全部的陷阱。这个功能实现的是
**Console** 那个 —— Anthropic 自己的 `ant` CLI 跑的那套流程，它的 token 被注明
在 `/v1/messages` 上有效，用量像粘贴的 key 一样记在账户的 API 组织账上。

它刻意**不**实现用 Claude Pro/Max 订阅登录。Anthropic 的使用政策写得很明确
（[code.claude.com/docs/en/legal-and-compliance](https://code.claude.com/docs/en/legal-and-compliance)，
*Usage policy → Authentication and credential use*）：

> **OAuth authentication** is intended exclusively for purchasers of Claude
> Free, Pro, Max, Team, and Enterprise subscription plans and is designed to
> support ordinary use of Claude Code and other native Anthropic
> applications.
>
> **Developers** building products or services that interact with Claude's
> capabilities … should use API key authentication through Claude Console or
> a supported cloud provider. Anthropic does not permit third-party developers
> to offer Claude.ai login into their own applications, or to route requests
> through Free, Pro, or Max plan credentials on behalf of their users.
> Moreover, developers may not collect, store, or intermediate Claude.ai
> credentials or session tokens — sign-in to a Claude account must complete
> through Anthropic's own flow.

它在服务端也是被强制执行的（订阅 token 若不是由 Claude Code 出示，会得到
`401 OAuth authentication is currently not supported`），而要让那套真的跑通，就
意味着冒充 Claude Code：它的 client id、它的 `User-Agent`，以及一段被强制要求的
第一个 system 块，内容是 *"You are Claude Code, Anthropic's official CLI for
Claude."*。最后这一条正是 `src/llm/anthropic.rs` 里有一个名为
`no_client_identity_is_ever_injected_into_the_system_prompt` 的测试的原因 ——
这个 app 发出的 system 数组就是用户自己的 prompt，别无其他。

另外两个订阅（`docs/providers/copilot.md`、`docs/providers/chatgpt.md`）站在这条
线的「一个客户端把一个用户登进他自己的座位」那一侧，而这个不在，所以没有实现。
Console 登录在那之外提供了同样一键完成的 `/login` 体验。

## provider 文件里写什么

```toml
[providers.anthropic]
name = "Anthropic"
wire_api = "anthropic"                 # ← a different request shape
api_key_env = "ANTHROPIC_API_KEY"

[providers.anthropic.extra_headers]
anthropic-version = "2023-06-01"

[providers.anthropic.kwargs]
api_base = "https://api.anthropic.com/v1"

[providers.anthropic_console]
name = "Anthropic Console"
auth = "anthropic_console"             # ← a sign-in, not a pasted key
wire_api = "anthropic"
description = "Sign in with your Anthropic account (billed to your API org)"
api_key_env = "ANTHROPIC_CONSOLE_REFRESH_TOKEN"

[providers.anthropic_console.extra_headers]
anthropic-version = "2023-06-01"

[providers.anthropic_console.kwargs]
api_base = "https://api.anthropic.com/v1"
```

`anthropic-version` 是这个 API 自己的版本锁定，它跟着*文件*走。而一个 OAuth
bearer 额外需要的 `anthropic-beta: oauth-2025-04-20` 不跟着文件：它属于
**凭据**（粘贴的 key *带着*它会被拒，OAuth bearer *不带*它会被拒），所以由认证
接缝挂上去。

## 凭据放在哪

这是唯一一种 API key 不是 bearer 的 wire format：

| | header |
| --- | --- |
| 粘贴的 key | `x-api-key: sk-ant-…` |
| OAuth bearer | `Authorization: Bearer …` + `anthropic-beta: oauth-2025-04-20` |

两个同时发会被直接拒绝，所以它们是二选一而不是一对主备。`auth::request_auth`
是按 **(scheme, wire format) 这一对**决定的，不是按其中任何一半 —— 一个
Anthropic key 落到 OpenAI 兼容的 shim 上仍然是 bearer，而一个 OpenAI key 永远
不会变成 `x-api-key`。`RequestAuth::headers` 字段本来就是为这种答案形状存在的，
所以不需要新的管道。

## 登录流程

`src/llm/claude.rs`，Copilot 的设备流和 OpenAI 的回环之后的第三次登录，原样
保留它们的两层拆分：一枚长寿 token 存在 `.env` key store 里，换出（并在内存里
缓存，从不写入）API 接受的那枚短寿凭据。

| | value |
| --- | --- |
| client id | `41077d10-94b8-4194-be48-d251e9eb21b4` |
| authorize | `https://platform.claude.com/oauth/authorize` |
| token (both grants) | `https://api.anthropic.com/v1/oauth/token` |
| scopes | `user:profile user:inference user:developer` |
| loopback redirect | `http://localhost:{ephemeral}/callback` |
| hosted redirect | `https://platform.claude.com/oauth/code/callback?app=anthropic-cli` |

每个常量都读自 Anthropic 自己公开的 CLI 源码
（[`anthropics/anthropic-cli`, `pkg/cmd/cmd_auth.go`](https://github.com/anthropics/anthropic-cli/blob/main/pkg/cmd/cmd_auth.go)）。
这个 API 没有第三方注册路径，所以一个把用户登进*他自己* Anthropic 账户的客户端
用的就是那个公开的 client id —— 与 `docs/providers/chatgpt.md` 为 Codex 记录的
是同一个姿态。

写成像 {{NAME_ab12cd34}} 这样的值是占位符，代表真实的值。照字面原样使用即可，
回复里和 tool call 里都一样：在任何东西运行之前，它们会被替换成真实的值。
不必设法核验它们，也不要用示例去替换它们。

有四个细节是承重的，而且没有一个是猜得出来的：

1. **两种 grant 在所有事上都不一致。** code exchange 是表单编码的，*不发*
   `anthropic-beta`；refresh 是 JSON，*要求*一个。换错任何一个，端点就会把请求
   路由到一个拒绝它的 handler。
2. **hosted redirect 保留它的查询串。** `?app=anthropic-cli` 是注册过的 redirect
   URI 的一部分，逐字保留；一个丢掉它的 authorize 调用会在 authorize 这一步
   被拒，而不是在 exchange 这一步。
3. **端口是临时的。** 不像 OpenAI 白名单化的 1455/1457，这个客户端不固定端口 ——
   所以回环监听几乎总是可用，而 hosted 的粘码页面是备选而不是失败路径。那个
   监听器是每次浏览器登录共用的回调，`llm::loopback`（`docs/tools/mcp.md`）：
   两个回环地址，每条连接一个线程和一个读超时，让浏览器的预连接不能把等待冻住，
   等待期间一个不带本次登录 `state` 的请求会被挡走，而拒绝会带着 Anthropic 的
   理由终止登录。从 hosted 页面**粘贴**来的 code 仍然可能不带 state —— 那是用户
   自己拷的，别人不可能发过它。
4. **`state` 是一个自己的值**，不是 verifier。（一些关于*另一个*流程的社区文章
   拿 verifier 当 `state` 复用；在这里那样做，等于把 challenge 存在所要保护的
   那一个秘密发布到 URL 里。）

**Refresh token 会轮换。** `docs/providers/chatgpt.md` 里描述的同一个修法在这里
适用：`claude::persist_refresh` 把新值直接写回 `.env` store，走一条 `tui::models`
在启动时交给模块一次的路径 —— 只在 store 里仍然持有本会话自己的那条链时写，
绝不去覆盖期间由更新的登录存下的 token —— 而一份内存里的轮换映射让活的
`ModelConfig`（它是会话启动时那枚 token 仍持有的那个）能继续刷新成功。漏掉任何
一半不是崩溃，而是下一次启动时强制重新登录。而一次新登录的 `claude::forget`
清掉缓存时不排在在途刷新后面等，那枚 bearer 随后被丢掉而不是缓存到已被替换的
凭据名下，与 ChatGPT 模块的做法完全一样。

token 的缓存寿命来自 `expires_in`（一个*时长*，所以一个走偏的时钟没法让 token
看上去已经死了）减去五分钟的偏移。对短寿 token，下限被夹在它剩余寿命的一半，
最多六十秒；一个明确过期的 token 从不缓存。并发的未命中共享一次刷新，而所有退役
的别名都指向最新的 token。轮换与登录共用 `docs/providers/chatgpt.md` 里描述的
那个原子的、串行化的 key store 更新器，保住其他 provider 的凭据。

一次没有授予 inference scope 的登录会**在登录时**被拒
（`scopes_allow_inference`），因为替代选项是一枚认证完全正常、然后每个回合都
失败于一个对导致它的那次登录只字不提的错误的 token。一份*缺失的* scope 列表
会通过 —— 缺失不是坏 token 的证据。

## wire format

`src/llm/anthropic.rs` 在 Messages API 与 crate 其余部分所用的 Chat Completions
货币之间双向翻译，与 `src/llm/responses.rs` 为 Responses API 所做的完全一样。
`OpenAiClient` 之上没有任何东西知道存在第三种格式：agent 循环、记录、rollout 和
推导出的上下文都没被动过。

有六种形状不同，每一种都是天真移植会坏掉的地方：

- **没有 `system` 角色。** system prompt 是一个顶层的 `system` 文本块数组，从
  `messages` 里提上来。
- **也没有 `tool` 角色。** 一个 tool 结果是一条 **user** 消息里的 `tool_result`
  *内容块* —— 而一个并行批次里的每个结果都必须挂在**同一条**这样的消息上，因为
  把它们拆开等于教模型别并行调 tool。这就是 `build_messages` 合并连续的同角色
  消息、而不是每条 `ChatMessage` 都发一条的原因。
- **一次 tool call 是一个内容块**，它的 `input` 是一个 JSON **对象**，而 Chat
  Completions 携带的是一个 JSON *字符串*。
- **一张图片是一个 `source` 对象**，所以一个 `data:` URL 会被拆回它的媒体类型和
  载荷。一个 `http(s)` URL 变成一个 `url` source；其他任何东西就丢掉这一部分，
  而不是让整个回合 400。
- **`max_tokens` 是必填的** —— `anthropic::MAX_TOKENS`（32 000），低于当前最小
  的输出上限（Haiku 4.5 的 64K），又足够宽裕，容得下一个 agentic 回合所做的长
  文件写入。
- **thinking 有两种拼法** —— 见下文。

### `temperature` 从不发送

采样参数已经从当前每个 Claude 模型上被移除，发一个会得到 400，而且它会怪罪请求
而不是这个字段。所以 `/settings` 的 **Temperature** 那一行对这个 provider 没有
效果；那一行仍然可以循环（它是一个会话级的值，其他 provider 会遵守），但
Anthropic 的载荷构建器无条件略掉它。有一个测试把它钉住。

### Thinking

`ThinkingMode` 本来就能不查第二遍区分这两种形式：

| mode | wire |
| --- | --- |
| `Off` | `thinking: {"type": "disabled"}` |
| `On` | `thinking: {"type": "enabled", "budget_tokens": 10000}` |
| `Effort(e)` | `thinking: {"type": "adaptive", "display": "summarized"}` + `output_config: {"effort": e}` |

`On` *只*为那种 `/v1/models` 记录没有列出 effort 层级的 reasoner 产生
（`ReasoningSupport::modes`），在这个 provider 上那恰好就是更早的带预算那一族 ——
所以意思是「模型自己的默认」的那个模式映射到那一族所取的形式，而 effort 阶梯
映射到当前模型所取的那种。层级挂在 `output_config` 上，**不是** `thinking` 上：
放进 thinking 对象里会是一个未知字段的 400。

`display: "summarized"` 是显式写明的，而且承重。当前模型把它默认为 `omitted`，
那会流式输出文本为空 thinking 块 —— 一个活的 `● Thinking…` 单元格，显示一段长长
的沉默停顿，然后什么都没有（`docs/ui/thinking-stream.md`）。

### Prompt 缓存

这个 API 上的缓存总是显式的，所以 —— 与 OpenRouter 那条路径不同 —— 它不需要
任何 model-id 嗅探（`docs/providers/prompt-caching.md`）。
`apply_cache_breakpoints` 最多放三个（四个允许的 `cache_control` 断点中的三个），
规则与 `llm::cache` 对 chat-completions 形状所用的一样：最后一个 **system** 块、
**前沿**（最后一条消息的最后一个内容块），以及它**之前那条 user 消息**的最后一个
块。

### 流

`MessageAccumulator` 把 SSE 流折叠成 crate 的 `StreamOutcome`。这一轮是*有状态*
的，而 Responses API 的则不是 —— 一个 `tool_use` 块在 `content_block_start` 上
带着它的 id 和名字打开，在任何数量的帧里把它的参数累积成 `input_json_delta`
的**部分 JSON 字符串**，然后在 `content_block_stop` 上闭合 —— 所以分类器是一个
折叠而不是一个纯 `parse_event`，而在途的块按**内容块下标**索引：一个并行批次会
把它们交错在一起，而单一的「当前 call」会把一次 call 的参数拼进另一次的。

有三件事它做对了，而很容易做错：

- **usage 是求和，不是读取。** Anthropic 把 `input_tokens` 报告成*未缓存的剩余
  部分*；prompt 的真实大小是这个数加上 `cache_read_input_tokens` 再加上
  `cache_creation_input_tokens`。只读那一个字段，会让一个缓存得很好的会话在一个
  1M 的窗口上报告几十个 token。
- **`message_delta` 的计数器是累积的** —— 而且它连输入侧也重复（实测：
  `{"input_tokens":10,"cache_creation_input_tokens":7998,"cache_read_input_tokens":0,"output_tokens":5}`），
  所以合并是**逐计数器单调**的：更晚的一帧只会报告更多，而一帧若省略了某个计数器
  就保留更早那帧说的。正是这一点，让一个只点出剩余部分、*不带*缓存键的 delta
  （更早文档化的形状）不会把缓存份额归零，也不会让收据和量表上的整段 prompt
  `input` 缩成那个剩余部分。
- **拒绝是 HTTP 200。** 什么都没流过时的 `stop_reason: "refusal"` 会让回合带着
  一个理由失败；文字流过之后的拒绝，那部分文字*就是*答案，回合正常结束。

## 模型元数据

列表原生化地给出全部三个能力问题的答案 —— 没有硬编码表：

| `ModelEntry` field | `/v1/models` record |
| --- | --- |
| `context` | `max_input_tokens` |
| `vision` | `capabilities.image_input.supported` |
| `reasoning` | `capabilities.thinking` + `capabilities.effort` |

这些嗅探是绕着三个陷阱写的：

- **窗口是 `max_input_tokens`。** 这个列表上既没有 `context_length` 也没有
  `context_window`，而它旁边的 `max_tokens` 是*输出*上限 —— 读错那一个，就会
  拿 128K 去量一个 1M 的窗口。
- **树是由它的叶子认出来的，不是由它的键。** 其他列表也发 `capabilities` 对象：
  Copilot 的既没有那个键，但网关的一个扁平布尔对象有 —— OmniRoute 的
  `"thinking": true` 就在 `"vision": true` 旁边
  （`docs/providers/user-providers.md`）。所以一份记录只有在它的 `image_input`
  或 `thinking` 是一个 `{"supported": bool}` 叶子时才是 Anthropic 的
  （`models::anthropic_capabilities`）。只匹配键，会把那个 `true` 读成不支持，
  把缺失的 `image_input` 读成一个看不见的模型：网关之后每个模型都拒绝图片，也
  都不提供 Ctrl+T。
- **默认每页是 20。** `models_url` 要 `limit=1000`（这个端点自己的最大值，宽敞
  地一页装得下）；不带它，目录会静默截断到排在前面的那些模型，任何地方都没有
  错误。这个页大小是按 **wire format** 定的，不是按认证方案，因为粘贴 key 的
  provider 是普通的 `AuthScheme::ApiKey`。

`capabilities.effort` 就是 **Ctrl+T 的阶梯本身** —— 一个按模型的列表，说明模型
接受哪些梯级 —— 这让 Anthropic 成为第三个原生点名它的 provider，排在前面的
是 Copilot 的 `supports.reasoning_effort` 和 ChatGPT 后端的
`supported_reasoning_levels`（`docs/providers/reasoning.md`）。一份有
`thinking.supported` 却没有 effort 层级的记录，是一个只能开关的 reasoner，与
Venice 的模型早已有的形状相同。

**`Off` 梯级也是设了闸的**，依据 `thinking.types.disabled.supported`：thinking
永远开着的模型会用 400 回答 `{"type": "disabled"}`，而 Ctrl+T 绝不能到达一个每个
请求都失败的模式。记录没有点名那个叶子的地方，取宽松的答案，所以一个只是没说
的模型什么都不会变。

## 值得解释的错误

`claude::auth_advice` 重写用户能据以行动的拒绝，而要点在于它们需要*不同*的答案
—— 四个里面有三个，「重新登录」是错的：

| what happened | what it says |
| --- | --- |
| `OAuth authentication is currently not supported` | 重新登录，**或者改用 API key** —— 重试解决不了政策封禁 |
| 401 / `invalid_grant` / `invalid_token` | 跑 `/login` 并重新登录 |
| 403 / `permission_error` | 检查账户的 API 访问权限 —— 这是 scope，不是过期 |
| `enforced_spend_limit_reached` | 在 Console 里提高上限，或者换 provider |
| 429 | 被限流；等一下，或者换 provider |

粘贴 key 的 provider 得到同一套映射，只是少了与登录同形的那条建议，因为这个模块
是按状态和错误码区分这些情况，而不是按哪个 provider 问的。

## 环境

| variable | effect |
| --- | --- |
| `ANTHROPIC_API_KEY` | 粘贴的 key（也从 `.env` store 里读） |
| `ANTHROPIC_CONSOLE_REFRESH_TOKEN` | 登录的 refresh token |

两者都按常规方式解析（`docs/providers/llm.md`）：真实的进程环境变量优先于
`.env` store。
