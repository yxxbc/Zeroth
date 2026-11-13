# 把 Google Antigravity 当作 provider

用一个 Google 账号登录，使用 Google Antigravity 所提供的模型 —— Gemini，以及
Google 经由它路由的 Claude 和 GPT-OSS 模型 —— 计在那个账号的 Antigravity
配额上：`/login` → **Sign in with an account** → **Google Antigravity**。不粘贴
任何东西；浏览器自己回到这台机器。

这是第五种登录（在 GitHub Copilot、ChatGPT Codex、Anthropic Console 与
ClinePass 之后），它保持它们的形状：`.env` 存储里一个长期 token，每个请求铸造
成一个短期 bearer，`/login` 那个分叉的浏览器页面，`/model` picker 的 ✓，
`/usage` 页面。它新增的是**第五种线上格式** —— Google 的 Cloud Code Assist
API，也就是 Antigravity 自己所对话的那个 backend —— 以及一件其他登录都没有的
事实：每个请求都点名该账号的 **Cloud Code 项目**，它在 bearer 旁边被查一次。

## 先读这一段：一条非官方路线

Google 没有发布任何 API 供第三方客户端够到 Antigravity 的配额。这个 provider
用 **Antigravity 自己的公开桌面 OAuth client**（每个社区集成都在用的同一个
client id 和 secret，从应用里提取出来的）登录，并用 **agy CLI 的
`User-Agent`** 向 backend 表明身份，因为 backend 是按那个 header 来开放它的
模型目录的 —— 这与 `docs/providers/chatgpt.md` 对 Codex 的 client id 和
`originator` 所采取的姿态相同。它是非官方的，Google 可能限制使用它的账号：
用得最多的社区插件警告说 *"Google may restrict accounts using third-party
clients"*，它的疑难解答页报告了对被标记账号的限流与影子封禁。`/login` 的
那一行在它的 description 里说明了这一点，而登录与否的决定属于用户。

它与 `docs/providers/claude.md` 所划的那条线哪里不同：在那里，厂商的使用政策
直接禁止第三方登录，服务器强制执行它，而绕过去意味着发送一份客户端被规定的身份
prompt（"You are Claude Code…"）。这里不需要这样的 prompt —— 三份参考实现中有
两份不发，第三份只是「为了对齐」另一个代理才发 —— 所以**一份都不发**：这个应用
所发的 system instruction 就是用户自己的 system prompt，别无其他，
`llm::cloud_code::tests::no_client_identity_is_ever_injected_into_the_system_instruction`
钉住它，正如 `docs/providers/claude.md` 的测试为 Messages API 所做的那样。

## provider 文件里写什么

```toml
[providers.antigravity]
name = "Google Antigravity"
auth = "google_antigravity"          # ← a Google sign-in, not a pasted key
wire_api = "cloud_code"              # ← Cloud Code Assist, not chat completions
description = "…"
api_key_env = "ANTIGRAVITY_REFRESH_TOKEN"   # ← the Google refresh token

[providers.antigravity.kwargs]
api_base = "https://daily-cloudcode-pa.googleapis.com"
```

`auth` 与 `wire_api` 是两个问题，与其他各处一样：认证方案决定 `/login` 的流程
与凭据，线上格式决定请求的形状。两种拼写都由 `llm::config` 里的测试钉住，因为
不匹配会**无声地**降级成一条粘贴 key 的 chat-completions 行。

base 是 **daily** 主机，也就是每一个当前的消费级 Antigravity 客户端发送生成、
项目初始化与目录请求所用的那个。主机不能混用：生产主机
（`cloudcode-pa.googleapis.com`）服务的是 Gemini CLI 那个独立的配额池，它的
权益与目录都不同，而从一个回退到另一个正是 404 连串的开始。

## 登录

`llm::antigravity` —— Google 为安装型应用准备的 OAuth 2.0 PKCE 流程，跑在共享的
loopback 回调上（`llm::loopback`），在 `/login` 的 worker 上运行，用的是
ChatGPT 与 Console 所用的同一个浏览器页面（`SigninKind::BrowserLink`）。

| | |
| --- | --- |
| authorize | `https://accounts.google.com/o/oauth2/v2/auth` |
| token | `https://oauth2.googleapis.com/token` |
| client | Antigravity 的桌面 client：id `1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com`，以及它那个（公开的）secret |
| scopes | `cloud-platform`、`userinfo.email`、`userinfo.profile`、`cclog`、`experimentsandconfigs` |
| redirect | `http://localhost:{port}/oauth-callback`，端口先试 `51121`，否则用任意空闲的 |
| extra | `access_type=offline` 与 `prompt=consent` —— 没有它们 Google 不会发 refresh token |

- **`state` 是随机的，而校验器从不离开这台机器。** 有一份参考实现把 PKCE 的
  verifier *塞进* `state`，那就等于公开了 challenge 存在正是为了保住的秘密；
  这里 `state` 是它自己的一个值，而共享的 loopback 会拒绝一个没有回显它的回调。
- **什么端口都行。** 一个桌面 OAuth client 接受任何 loopback 端口，所以
  `51121`（Antigravity 自己的）只是第一选择；端口被占就退回一个临时端口，而不是
  让登录失败。
- **这次交换是表单编码的，并携带 client secret** —— Google 的安装型应用流程即使
  有 PKCE 也要它。
- 被存下来的是 **refresh token**，存在 `ANTIGRAVITY_REFRESH_TOKEN` 之下。Google
  会保持一个桌面 client 的 refresh token 稳定，但一次返回了新 token 的刷新会
  被原样写回，正是 `docs/providers/claude.md` 的轮换做法（只在存储仍然持有本
  session 自己那条链时），而一次什么都不返回的刷新保留旧的。
- 一个没有 refresh token 的 token 响应会在登录时被拒绝：否则这个 session 在
  第一次过期时就会死掉，除了再登录一次别无退路。
- 确认信息点名**账号**（`userinfo` 的 email）与它的套餐，也就是一个有多个 Google
  账号的用户刚刚在两者之间做出的选择。

### 两层 token，以及项目

`antigravity::authorize(refresh)` 就是 auth 接缝每个请求所调用的：

1. **bearer** —— 从 refresh token 在 `https://oauth2.googleapis.com/token` 铸造，
   在内存里按它的 `expires_in` 减五分钟缓存，以 refresh token 为键；一次缓存
   miss 会排在一道闸门之后串行处理，所以一个回合的各轮与 `/model` 的抓取永远
   不会刷新两次。
2. **项目** —— `POST {base}/v1internal:loadCodeAssist`，带
   `{"metadata": {"ideType": "ANTIGRAVITY"}}`（`platform`/`pluginType` 值在那里
   会被拒绝），读 `cloudaicompanionProject`（一个字符串，或一个带 `id` 的对象）。
   一个 backend 从未设置过的账号什么都不点名：它会被**引导上线**
   （onboard）—— `POST {base}/v1internal:onboardUser`，用默认允许的层级（没有
   标记时是 `free-tier`）和 snake_case 的 metadata，重发直到它回答 `done`，次数
   有界 —— 而一个仍然什么都不点名的账号退回共享的消费级项目
   `aicode-consumers`。`ANTIGRAVITY_PROJECT_ID` 被导出时，高于以上一切。项目在
   bearer 旁边缓存，活过整个进程。

引导上线**只在登录时**跑，在它那个可取消的 worker 上：它可能要问好几次、相隔
几秒，而一个请求 —— 它的线程在轮询 Esc —— 绝不能等它。一个在别处设置过的账号上
的请求，问一次 `loadCodeAssist`，什么都不点名时就用那个 fallback。

`antigravity::forget` 被 `/login` 边界用一次登录刚刚存下的 token 调用（这样一条
陈旧的 bearer 永远无法替它作答），它丢弃的是 **bearer**，绝不是账号的设置：项目
是关于账号的一个事实，不是会过期的凭据 —— 而丢掉它，就把这次登录刚刚引导上线
所换来的那个项目扔了，这正是 `smoke.sh` Phase 148 第一次运行时抓到的。

`RequestAuth` 把项目带到线上，作为 `RequestAuth::project` —— 一个只有这个认证
方案会填的字段，因为只有这个 API 在它的正文里点名一个项目 —— 就在 bearer 和
`User-Agent` header 旁边。

## 线上格式

`llm::cloud_code` 在这个 crate 的 Chat Completions 通货与 Cloud Code Assist
之间双向翻译，与 `llm::anthropic` 和 `llm::responses` 守的是同一份契约：
`OpenAiClient::stream_chat` 之上的任何东西都不知道存在第五种线上格式。

### 信封

`POST {base}/v1internal:streamGenerateContent?alt=sse`：

```json
{
  "project": "<the account's project>",
  "model": "<the runtime model id>",
  "request": { "contents": [], "systemInstruction": {}, "tools": [], "generationConfig": {}, "sessionId": "…" },
  "requestType": "agent",
  "userAgent": "antigravity",
  "requestId": "agent-<uuid>"
}
```

Header：bearer、`Content-Type: application/json`，以及 agy CLI 的
`User-Agent` —— `antigravity/cli/{version} (aidev_client; os_type={os};
arch={arch}; cl={changelist}; auth_method=consumer)`，在 auth 接缝处按机器构建。
backend 对任何其他 agent 字符串只提供一个缩减的目录，并用 404 回答那些隐藏模型，
所以这个 header 是承重的；`ZEROTH_ANTIGRAVITY_USER_AGENT` 在 Google 移动
那道闸门时整体替换它。没有 `X-Goog-Api-Client`、`Client-Metadata` 或
`x-goog-user-project`：最后一个会让请求在一个其项目缺少该 API 的账号上变成
403。`sessionId` 是这个 session 的缓存亲和键
（`docs/providers/prompt-caching.md`）。

### 请求

- **`contents`**：角色为 `user` 和 `model`。system prompt 被提到
  `systemInstruction` 里（`{"role": "user", "parts": [...]}`，与每一份参考实现
  发送的方式一样）；一个 tool 结果是一个 `functionResponse` part
  （`{"name", "response": {"output": …}}`），放在一个**user** content 里；相邻
  的同角色 content 被合并，所以一批并行调用的结果是同一个 user content，而一个
  `read` 在它们之后附上的图片，作为 `inlineData` 与它们并列随行。图片是从
  session 共享的 `data:` URL 里拆出来的 `inlineData {mimeType, data}` —— 借用，
  从不拷贝（`docs/architecture/memory.md`）。
- **backend 强制的形状规则**：一个请求不能以 model 轮结尾（以它结尾的会得到一条
  `Continue.` 的 user 轮），而没有 parts 的 content 被丢弃。
- **call id** 只对 Claude 和 GPT-OSS 模型通过 `functionCall`/`functionResponse`
  随行，它们需要它来把一个结果与它的调用配对；Gemini 按顺序和名字配对，所以
  一个都不发。Gemini 自己不点名任何 id，所以流会给每次调用一个随机的 `call_…`
  id —— 随机而不是计数，因为 id 会被记录并在 `/resume` 之际重放。
- **tool schema**，按家族分。Gemini 把标准 JSON Schema 作为
  `parametersJsonSchema` 接收，前提是本地 `$ref` 都已被内联，且元关键词
  （`$schema`、`$id`、`$defs`、`definitions`、`$comment`、`$anchor`）已经去掉
  —— backend 要求自包含的 schema。Claude 与 GPT-OSS 走 Google 的自定义 tool
  桥，它接受旧的 `parameters` 字段，并对 `type`、`description`、`properties`、
  `required`、`items` 和 `enum` 之外的任何关键词回答 `400 Unknown name`，所以
  只有这六个被保留（一张 allowlist，这样 JSON Schema 明天新增的关键词无法让一个
  请求坏掉）：一个类型联合塌缩成它第一个非 null 类型，一个 `enum` 只有每个值都是
  字符串时才保留，而一个写成 `anyOf` 的属性变成 `{}`。
- **思考**，按家族分 —— `generationConfig.thinkingConfig`，只在选择携带思考模式
  时才发（模型的记录说它会推理）：
  - Gemini：`{"includeThoughts": true}`。**级别**是运行时 id 的后缀（见下文
    *目录*），所以旁边不发任何预算或级别 —— 最新的 Gemini 拒绝 1000 以下的预算，
    而 id 已经选好的级别不需要再说一遍。
  - Claude（`…-thinking` 的 id）：`{"includeThoughts": true, "thinkingBudget":
    10000}`，Messages 线上格式自己的预算，配上 `maxOutputTokens` 64000，
    必须高于它。Claude 从不得到 `temperature`。
  - GPT-OSS：`{"includeThoughts": true, "thinkingBudget": 8192}`。
- **`maxOutputTokens`**：Gemini 是 65535（当前 Gemini 模型中最小的那个上限，
  所以没有任何模型被要求得超过它允许的 —— 超过一个上限就是 400），Claude 是
  64000，GPT-OSS 是 32768。

### 思考签名

Gemini 3 及之后会为每个 tool 轮的推理签名，并**拒绝**一个其 tool call 回来时
没有签名的后续请求（`Function call is missing a thought_signature`）。这个 crate
刻意在派生的 context 里不保留任何推理（`docs/ui/thinking-stream.md`），所以这些
签名住在它**旁边**：`cloud_code` 维护一个按 call id 索引、按字节有界的进程级
memo，从每一个结束的轮填充（`StreamAccumulator::finish` →
`RoundEnd::signatures`），而请求构建器在它里面查每一次调用：

- 每个 model content 的**第一个** `functionCall` 携带它的签名 —— memo 里有就是
  真的那个，否则就是 Google 有文档的 `skip_thought_signature_validator` 哨兵
  （一个来自另一个 provider 的轮，或来自一次 `/resume` 之前）；它之后的并行调用
  一个都不带。一个以重放的 Claude 思考开场的回合（见下文）由那个思考自己的签名
  担保，绝不*同时*再给它那个哨兵 —— 那是没有任何参考实现会发的配对；
- 对 **Claude**，它的思考是 Anthropic 自己的、必须开启它所启动的那个 tool 循环，
  该轮的思考文本和签名也被 memo 下来，作为开头的
  `{"thought": true, "text", "thoughtSignature"}` part 重放。一个 Claude 的
  tool 循环，若 memo 没有它开场的那一轮（一次循环中途的 `/resume`），那一轮就
  **不带**思考发出，而不是带上一个服务器会拒绝的重建块 —— 那份用合成的
  `[Tool execution completed.]` / `[Continue]` 轮把一切糊过去的参考实现，是在替
  双方说话，而这里不这么做。

### 流

`StreamAccumulator` 折叠这些 SSE 行：每个 `data:` 行是一个被 `{"response": …}`
包住的 `GenerateContentResponse`（没包住的也一样读）。一个 `functionCall` part
是**整块**到达的，参数是一个对象，并立刻在它的槽位下浮出
（`docs/ui/write-streaming.md`）；`thought: true` 的 part 是推理；`finishReason`
映射成这个 crate 的用词（`STOP`、`MAX_TOKENS` → `length`、安全那一族 →
`content_filter`、其他任何东西小写化 —— `MALFORMED_FUNCTION_CALL` 结束一个什么
都没回答的轮，agent 循环会请模型从那里继续，
`docs/architecture/unfinished-replies.md`）。usage 是 `usageMetadata`：
`promptTokenCount`（包含缓存的那一份）作为输入，回复**和**思考作为输出。两种
失败形状是带内的：一帧 `{"error": …}`，以及一个**在**各帧**之后**写出的、完全
没有 `data:` 框架的原始多行 JSON 错误正文。一个没有 finish reason 就结束的流是
被截断的，并作为这样的失败 —— 落在有界重试上，那会作废这次尝试。

## 目录

`POST {base}/v1internal:fetchAvailableModels`，带 `{"project": …}`，回答一张以
运行时模型 id（要发的那个 id，逐字）为键的**映射**，带 `displayName`、
`maxTokens`（上下文窗口）、`maxOutputTokens`、`supportsImages`、
`supportsThinking`、`quotaInfo` 和 `agentModelSorts`，即 backend 自己精选的
列表。`/model` 列出：

- `agentModelSorts` 点名过的那些 id（它点名时；backend 就是这样退役一条仍会作答
  但已失效的路由），否则就是每一个 id；
- 减去 router 槽位（`chat_…`、`tab_…`）、`-tiered` 路由器、图片模型、内部的
  （`isInternal`、`API_PROVIDER_INTERNAL`）与被禁用的；
- 并把**级别兄弟归成一组**：`gemini-3.8-flash-low`、`-medium` 和 `-high` 是
  同一行，即 `gemini-3.8-flash`，它的 Ctrl+T 阶梯正是目录所列的那些后缀，而它的
  请求发出 `{model}-{level}`。没有兄弟的级别后缀留在它自己的 id 里
  （`gpt-oss-120b-medium`、`claude-opus-5-5-high`）。只归并精确的阶梯后缀，正是
  让重建保持精确的原因：所发出的 id 永远是目录列出过的某一个。
- 一行的窗口是它各个变体中最小的那个，只要有一个变体说不行，它的图片输入就是
  `false`，而它在记录说 `supportsThinking` 时推理 —— 对 Claude 只在一个
  `-thinking` 的 id 上，那是 backend 接受思考预算的唯一一条 Claude 路由。

## `/usage`

同一个 `fetchAvailableModels` 的答案携带每个模型的 `quotaInfo`
（`remainingFraction` 0–1，`resetTime` 为 RFC 3339）：每个 `/model` 行一条量表，
按家族分组（先是 Gemini，然后 Claude，然后 GPT-OSS），已用份额是该行**最稀缺**
变体的 `1 - remainingFraction`，并带上那个变体的重置时间（一个没有分数的
`quotaInfo` 就是已耗尽）。套餐是 `loadCodeAssist` 的 `paidTier` 名字，否则是它的
`currentTier` —— `currentTier` 即使在一个付费账号上也读作 `free-tier` —— 而
账号是 `userinfo` 的 email。这些池在模型之间共享，所以页面会说明这一点。没有
仪表盘链接：Google 没有发布任何 Antigravity 配额页面。

## 当它不工作时

`antigravity::auth_advice` 会改写那些用户能据此采取行动的拒绝 —— 在 chat 流上
通过 `OpenAiClient::explain`，在其他每一次调用上（`loadCodeAssist`、目录）通过
`antigravity::explained`，所以没有一个会以原始 JSON 到达屏幕：

| 状态 | 服务器说了什么 | 用户读到什么 |
| --- | --- | --- |
| 400 | `invalid_grant`（刷新） | 这次登录已不再有效 —— 再 `/login` 一次 |
| 401 | — | 同上 |
| 403 | `details[].metadata.validation_url` | Google 要求验证这个账号 —— 那个 `accounts.google.com` 链接，且只接受这样的链接 |
| 403 | 其他任何东西 | 这个账号无法使用 Antigravity，附服务器的理由 |
| 404 | `Requested entity was not found` | 这个模型（已）不可用于这个账号 —— 换一个 |
| 429 | 带重置时间的 `RESOURCE_EXHAUSTED` | 这个模型的配额用完了，以及何时重置 |
| 429 | 限流的措辞 | 一次瞬时限流（有界重试已经试过了） |
| 503 | `No capacity available` / `MODEL_CAPACITY_EXHAUSTED` | Google 此刻对这个模型没有容量 |

### 浏览器打开之前

登录开始时跑两项检查，两者都取自
[`pingdotgg/t3code`](https://github.com/pingdotgg/t3code) 的 Antigravity
provider，它驱动官方 CLI，而这个 crate 重新实现了那套流程。这两项检查无论哪种
方式都是它设计里值得拥有的那部分。

**可达性。** `antigravity::probe_reach` 向 authorize 主机发一个 `HEAD`，30 秒
期限（`REACH_GOOGLE_AFTER`）。任何 HTTP 状态都算够到了 —— 一个 405 仍然证明这
条路由能用 —— 只有传输失败才是失败，在打开一个否则会挂十分钟的浏览器页面之前
报告：

> Antigravity has not reached Google yet. Check this machine's network
> connection — broken IPv6 is a common cause.

IPv6 那句是 t3code 的，它挣到了自己的位置：一条断掉的 IPv6 路由会让每一个
Google 连接卡住，而 IPv4 一切正常，这是用户猜不到的唯一一种原因。这个探针从不
阻塞登录 —— 一个跑不起来的探针，不是登不进去的证据。

**环境里的凭据。** `antigravity::AMBIENT_GOOGLE_KEYS` 点名那些会让一个 Google
client 去读*另一份*凭据的变量：`GOOGLE_APPLICATION_CREDENTIALS`、
`GOOGLE_CLOUD_PROJECT`、`GOOGLE_GENAI_USE_VERTEXAI`、`GEMINI_API_KEY`，还有
五个。这项检查**大小写不敏感**，因为 Windows 就是这样对待环境变量名的，而任何
被设置了的都会在一条 toast 里点名：

> GOOGLE_APPLICATION_CREDENTIALS set in this shell — the sign-in ignores it
> and uses your account instead

登录本身早已忽略它们：这个 crate 从不读一份环境里的 Google 凭据，所以那个变量
对我们从来不是正确性问题 —— 只是当用户期待它被使用时的一份无声的意外。t3code
从它交给 CLI 的环境里剥掉同一组变量，理由是更硬的那一个：CLI *会*读它们。

## 这个不做的事

- **没有账号轮换，没有配额池回退。** 一次登录一个账号，一个池（Antigravity
  的）；社区插件的多账号轮换、以及它们遇到 429 就切到 Gemini CLI 的池，是这个
  客户端不代它的用户做主的政策。
- **没有 Google Search grounding，也没有图片生成** —— 两者都是独立的 RPC，有
  自己的模型，不是 chat。
- **没有 headless 粘贴。** 浏览器必须够到 `localhost`；在一台远程机器上，把登录
  页面点名的端口转发出去（`ssh -L 51121:127.0.0.1:51121 …`）。
- **没有交错的 Claude 思考**（`anthropic-beta: interleaved-thinking-…`）：
  Claude 在每个回合的开头思考。

## 环境

| 变量 | 作用 |
| --- | --- |
| `ANTIGRAVITY_REFRESH_TOKEN` | 存下来的凭据（`.env`，或导出的） |
| `ANTIGRAVITY_PROJECT_ID` | 要计费的那个 Cloud Code 项目，高于自动发现 |
| `ZEROTH_ANTIGRAVITY_USER_AGENT` | 替换 agy CLI 的 `User-Agent` |
| `ZEROTH_GOOGLE_OAUTH_BASE` | token 与 userinfo 调用去哪里（smoke 套件的 stub） |

## 文件

- `src/llm/antigravity.rs` —— 登录、token 缓存、项目、目录与配额解析，以及那些
  建议。
- `src/llm/cloud_code.rs` —— 线上格式的翻译与流。
- `src/llm/auth.rs` —— `RequestAuth::project`。
- `src/tui/workers.rs` —— 登录 worker。
