# 把 Cline 当作 provider

两种方式够到 Cline 扩展和 CLI 背后的模型：`/login` →
**Sign in with an API key** → **Cline**，或者 `/login` →
**Sign in with an account** → **ClinePass**。

附带两个 provider——一个 API，两条入口，各自带着它自己付费的那份目录：

| provider id | 怎么认证 | 存什么 | `/model` 列什么 |
| --- | --- | --- | --- |
| `cline` | 在 [app.cline.bot/dashboard/account](https://app.cline.bot/dashboard/account) 建的 key | 那个 key，逐字 | 按用量计费的目录，从账户的**信用**里付 |
| `cline_pass` | Cline 账户登录（一个 WorkOS device code） | 一个 **refresh** token | ClinePass 套餐的 `cline-pass/…` 模型，从**订阅**里付 |

两者都说 **Chat Completions**，对着 `https://api.cline.bot/api/v1`，而两者都知道它们
每个模型能做什么——上下文窗口、是否读图、Ctrl+T 循环的思考档——从 Cline 自己的
目录里（见下文*每个模型能做什么*）。

这个登录**只提供一次**，名为 ClinePass。它以前提供两次，旁边一个 `cline_account`
行跑着完全相同的流程、为按用量计费的目录存着完全相同的 token。但在
*Sign in with an account* 下那一个订阅就是 ClinePass：按用量计费的模型计的是信用，
不是套餐，而一个 key 已经够得到它们。一个登录两行，招来的是登录两次，而第二行
正是路由 bug 藏身之处（下文*哪条流程在跑*）。

## 一个 provider 文件说什么

```toml
[providers.cline]
name = "Cline"
api_model_base = "https://api.cline.bot/api/v1/ai/cline"   # ← the catalog with each model's facts
description = "The models behind the Cline extension and CLI — …"
api_key_url = "https://app.cline.bot/dashboard/account"   # ← where `/login` says keys are made

[providers.cline.kwargs]
api_base = "https://api.cline.bot/api/v1"

[providers.cline_pass]
name = "ClinePass"
auth = "cline_account"                       # ← a Cline account sign-in, not a pasted key
description = "Cline's $9.99/month subscription: …"
api_key_env = "CLINE_ACCOUNT_REFRESH_TOKEN"  # ← the account's refresh token

[providers.cline_pass.kwargs]
api_base = "https://api.cline.bot/api/v1"
```

`auth = "cline_account"` 才是把这一行放进**订阅**列表、把 Enter 送到 device 页、并
列出套餐模型（下文 *ClinePass*）的东西；拼法上一个不匹配会*悄悄*退化成一个粘贴 key
的行，这正是 `llm::config` 用一个测试把它钉住的原因——`anthropic_console` 得到的
也是同一个姿态（`docs/providers/claude.md`）。这个方案名保留它的名字：它所描述的
是凭据，一个 Cline *账户*——即那唯一一次登录，也就是 ClinePass。块里没有一样东西
点名套餐的模型：那份清单是 Cline 要发布的，不是这个文件要抄的。

### 哪条流程在跑

`tui::workers::spawn_signin` 按这一行的**认证方案**、在一个穷尽的 match 里挑登录
——绝不是按它的 provider id。当它按 id 匹配、其余用一个 catch-all 兜底时，
ClinePass 那一行掉进了 GitHub Copilot 的 device flow，于是一次完成的登录会把
GitHub 的 token 存成 Cline 的 refresh token 并发给 Cline 的 refresh 端点。用户
自己的 `providers.toml` 可以把一行叫任何名字，也可以给两行同一个登录；方案才是
做决定的东西。

## 登录

Cline 自己的客户端通过 **WorkOS** 登录，即 app.cline.bot 背后的身份 provider，所以
这里是对 Cline 已发布客户端的移植（`sdk/packages/core/src/auth/cline.ts` 及它的
provider-auth 注册表）；`src/llm/cline.rs` 里每个常量都是从中读来的。流程是
**device code**——GitHub Copilot 的登录所用的同一个页面（`docs/providers/copilot.md`）：
一个圆角框里的 code 压在要去确认它的 URL 之上、`c` 复制它、一个从 code 自身寿命起算
的倒计时、不启动浏览器。

1. **code 请求** —— `POST {auth}/user_management/authorize/device`，form 编码，
   只带**公开的** WorkOS client id（`llm::cline::CLIENT_ID`），别的什么都没有。
   device-code 流程没有 client secret，所以那个 id 就是客户端所需的全部；响应点名
   轮询要出示的 `device_code`、页面显示的 `user_code`/`verification_uri`，以及
   过期时间/间隔（缺失时默认 WorkOS 自己的 300 s / 5 s，并按数字*或*字符串读，
   即 `docs/providers/chatgpt.md` 的 device code 已经延展过的那个宽容度）。
2. **轮询** —— `POST {auth}/user_management/authenticate`，带
   `grant_type=urn:ietf:params:oauth:grant-type:device_code` 和 code 请求发出的
   那一对，每个间隔一次。裁决读**body**：`authorization_pending` 继续等、
   `slow_down` 把间隔加一秒（Cline 自己对那个提示的读法）、
   `access_denied`/`expired_token`/`invalid_grant` 以服务器写下的那句话结束流程
   （当它写了一句时），而一个同时带两个 token 字符串的 body 就是批准，不管送达它的
   状态是什么。
3. **注册** —— WorkOS 那一对*不是* API 所要的。它在 `POST {api}/api/v1/auth/register`
   上被交换**一次**，换成 Cline 自己的 token：`data.refreshToken` 是 key 存储所
   留下的，而 `data.userInfo.email` 是确认语所点名的——`Signed in to
   ClinePass (you@example.com) — run /model to use it`。WorkOS 形状的东西一概
   不存：登录之后，再也不会与 WorkOS 那一对说话。

一次不带 refresh token 就完成注册的登录**在登录处**被拒绝——没有它，session 会在
第一次 access token 过期时死掉、除了再登录一次没有回头路——完全像
`chatgpt::exchange_code` 拒绝一组没有 refresh token 的 token。

## ClinePass

ClinePass 是 Cline 的**订阅**——每月 $9.99，换一份精选开源编码模型上 2–5 倍的用量。
Cline 自己的客户端要求你在订阅之后指向 **ClinePass** 而不是 **Cline**，而 API 按套餐
的 **`cline-pass/…` slug** 取这些模型——`cline-pass/glm-5.3`、
`cline-pass/deepseek-v4.1-flash`、`cline-pass/kimi-k3`、`cline-pass/qwen3.7-max`
以及这份套餐清单上的其余——而按用量计费的 id（`deepseek/deepseek-v4.1-flash` …）
改为计账户的**信用**。那个分野就是两行之间的分野：登录列出套餐的模型，key 列出按
用量计费的那些。

因为那些 id 是套餐自己的目录，公开的 `/models` 列表**不**携带它们。Cline 改为为它
自己的客户端发布套餐清单——
`GET https://api.cline.bot/api/v1/ai/cline/recommended-models`，公开的，一页四个
列表（`recommended`、`free`、`clinePass`、`clineCloud`），其中 `clinePass` 数组
就是套餐的（`sdk/packages/core/src/cloud/models.ts`、`loadCloudModels`）——而一次
`cline_account` 登录从那里读它：`llm::cline::fetch_clinepass_models` 以**不带任何
凭据**的方式 GET 它，正如 Cline 的客户端所做，`parse_clinepass_models` 按 Cline
自己的顺序保留清单（没有可用 id 的记录跳过，重复的 id 只留一次，一个 `data` 包着的
页读穿），而 `llm::models::fetch_models` 把行交给 `/model` picker 和启动探测，
打上 `[cline_pass]`。

这些 slug 是 Cline 要改的：它已经退役过一些，随这个 provider 附带的清单拷贝在合并
之前就陈旧了两个模型（`cline-pass/mimo-v2.6-flash` 和 `-pro`），而线上清单此后又
重排过。所以已发布的清单是**唯一**来源，正如其他每个 provider 的清单是它唯一的
来源：套餐新加的模型在它被列出的那天就在 picker 里，退役的随它一起走。一份拿不到的
清单——离线、被拒、一个这个解析不再读的页，或一个不点名任何模型的页——就是
provider 自己在 picker 里的注记（`⚠ ClinePass unavailable` 在别的 provider 的模型
旁边，或者当它是唯一那个时，理由本身为红字）：绝不是一份看起来是新的、其实不是的
附带拷贝，也绝不是一个没有行的 provider，那读起来像获取根本没发生（已登录 ChatGPT
清单的规则）。

清单由这一行的**认证方案**选，正如 ChatGPT 那份必需的 `client_version` 一样——它
属于凭据的后端——而不是由它自己的一个键、一个 provider id 或一个模型名选。id
（`cline_pass`）是用户自己的 `providers.toml` 对这一行的任意叫法，所以以它为键
正是*哪条流程在跑*所描述的那个路由 bug：一个被改名或被复制的块会悄悄从错的地方
列举。而一个模型名前缀（`cline-pass/…`）选不了它要被读出的那份清单，因为那份清单
还没到。

配额按滚动的 5 小时、周和月三个窗口衡量；仪表盘是
[app.cline.bot/dashboard/subscription?personal=true](https://app.cline.bot/dashboard/subscription?personal=true)。
一个窗口用尽时服务器会说是哪一个（*You have reached your 5-hour
Clinepass limit. Please try again later.*），而建议点名那个窗口并指向按用量计费的
模型，即 key 行所够得到的（`/login` → Sign in with an API key → Cline）。每个窗口
用了多少、何时重置，是 `/usage` 页的 ClinePass 小节——读自仪表盘自己的
`/api/v1/users/me/plan/usage-limits`（`docs/providers/usage.md`）。

**免费模型刻意不列在这里。** Cline 跑着轮换的 `cline-free/…` 推广，但它的文档明确
说免费模型的用量**不支持经 Cline API**——它们只在 Cline 扩展和 CLI 里能用——所以
这里的一行会提供端点所拒绝的模型。

## 每个模型能做什么

chat base 自己的 `/models`（`https://api.cline.bot/api/v1/models`）只点名每个模型的
id，别的什么都没有——没有窗口、没有模态、没有推理——所以从它列出的模型没有上下文
量表、没有自动压缩、没有 Ctrl+T 的档位，而且贴图是瞎的。Cline 的客户端改为从
`https://api.cline.bot/api/v1/ai/cline/models` 读它们的目录：同样的 461 个模型、
公开（一次已登录的请求与一次匿名的逐字节相同），按 **OpenRouter 的记录形状**，
而 `llm::models::parse_models` 本来就读那个：

| 记录字段 | 它给出什么 |
| --- | --- |
| `context_length` | 页脚量表所对照的窗口，以及自动压缩的 90% |
| `architecture.input_modalities` | 图片输入：一张图被附上，或对一个读不了图的模型降级成一条注记 |
| `supported_parameters` 里点名 `reasoning` / `reasoning_effort` | 一个推理模型：Ctrl+T 循环 `off → low → medium → high`，默认 `medium` |

**key 行**列那份目录：它的 `api_model_base` 是
`https://api.cline.bot/api/v1/ai/cline`，所以 `{api_model_base}/models` 就是目录
本身，而普通列举路径做其余的。

**ClinePass** 列它自己发布的模型，那些目录不携带——而且它们对自己什么都没说
（`id`、`name`、`description`、`tags`）。所以每一个都像 Cline 自己的客户端那样
（`normalizeClineRecommendedProviderModels`）从目录里借它的事实：按模型名字**去掉
它的实验室**，`cline-pass/glm-5.3` 即是 `z-ai/glm-5.3`，它的显示名
（`Z.ai: GLM 5.3`）也算在内。`llm::models::clinepass_entries` 做这个 join；目录
在已发布清单旁边被读出（`fetch_cline_catalog`，不带凭据，从生效 API host 上的
`llm::cline::catalog_url` 取，所以 Phase 137 的 stub 能提供它）。今天十四个套餐
模型里有十三个匹配上。一个不匹配的——今天的是 `cline-pass/qwen3.8-max`，或者一个
两个实验室都曾共用的光秃秃名字，那种不做猜测——得到 Cline 的客户端对这类模型的假定
（`CLINE_PASS_MODEL_DEFAULTS`）：**128K** 窗口、推理开、无图片输入。一份取不到的
目录只花掉这些事实，从不花掉行：那时每个套餐模型都取那些默认。

两者都用一个真实账户和 key 对着线上 API 验证过：这个客户端所发的 reasoning 对象
——`reasoning: {"effort": …}`，以及 Off 的 `{"enabled": false}`——在一个 ClinePass
模型上和在按用量计费的一个上同样被接受，一个 effort 把 `reasoning` 增量流进思考
块，而 Off 的答复不带它，而 usage 帧报告
`completion_tokens_details.reasoning_tokens`。

## wire 形式，以及两层 token

每个订阅都保留的同一个分野（`docs/providers/chatgpt.md`）：

1. 注册铸出的那个 **Cline refresh token**。长寿命，持久化在 `.env` key 存储里的
   `CLINE_ACCOUNT_REFRESH_TOKEN` 之下——与一个粘贴的 key 落在同一个存储里，所以
   key 解析、`/model` picker 的 ✓ 和下一次启动都不需要第二个机制。
2. **access token**——一个 API 真正接受的 WorkOS JWT，按需在
   `POST {api}/api/v1/auth/refresh` 铸出（JSON：`refreshToken` 加
   `grantType: "refresh_token"`），缓存在内存里，从不写入。

bearer 不是逐字的 access token：Cline 的 API 接受拼作 `workos:<jwt>` 的 OAuth
access token——一个**粘贴的仪表盘 key** 逐字直传，给它加前缀会 401——所以
`cline::bearer_for` 在凭据被铸出的地方（`Authorization: Bearer workos:<jwt>`）
加上那个前缀，而隔壁的粘贴 key provider 把它的 key 直接送过去。

**refresh token 会轮换。** Cline 的 register/refresh 响应在它轮换过时重发
`refreshToken`、没轮换时省略，所以 `docs/providers/chatgpt.md` 所描述的同一个修法
适用：`cline::persist_refresh` 经一条 `tui::models` 在启动时交给模块一次的路径，把
新值直接写回 `.env` 存储——只在存储仍持有本 session 自己的那条链时，绝不覆盖期间
更新的登录所存的 token——而一个内存里的轮换表让活着的 `ModelConfig`（它还持有
session 开始时的那个 token）继续刷新成功。一次新登录的 `cline::forget` 清掉缓存，
不必排在一个在飞的刷新后面。

token 的缓存寿命来自响应的 ISO-8601 `expiresAt`（当它能解析时），解析不了时来自
access token 自己的 `exp` 断言，两者都没有时取保守的一分钟——再减去五分钟偏移，
好让没有请求带着一个会在路上死掉的 token 出发。短寿命的 token 至多保留它宣称寿命
的一半；一个已经过期的截止时间什么都不缓存。

## 不工作的时候

`openai::explain` 改写 Cline 的 API 所记录的拒绝（`docs.cline.bot/api/errors`），
每一种都想要一句不同的措辞。关于 ClinePass **套餐**的那些拒绝是消息而非状态，按
Cline 的客户端匹配它们的顺序、按服务器自己的话匹配（`sdk/packages/llms/src/providers/builtins.ts`
里的 `handleClineResponseError`），而它们压过送达它们的那个状态——一个 Pass 窗口
用尽，绝不能读成「加信用」：

| 你看到的 | 它意味着 |
| --- | --- |
| `your Cline sign-in is no longer valid — run /login and sign in again.` | 一次请求上的 `401`，或 refresh 端点拒绝存下的 token。线上服务器用一个 **`400`** 回答一个死掉的 refresh token——`{"data":"","error":"failed to refresh token: invalid_grant","success":false}`——所以一次被拒的刷新按 Cline 自己的规则读（`isLikelyInvalidGrant`，移植为 `cline::is_likely_invalid_grant`）：任何状态上的一个点名 `invalid_grant`、`invalid_token` 或 `unauthorized` 的错误码，或者一个 `400`/`401`/`403`、其消息说 `invalid`、`expired`、`revoked` 或 `unauthorized` |
| `this Cline account is out of credits — add credits at app.cline.bot/dashboard, or switch provider with /model.` | 一个 `402`：账户的余额，不是这次请求 |
| `this Cline account isn't allowed to make that request — …` | 一个 `403`：凭据的够及范围 |
| `ClinePass is for personal Cline accounts, and this is an organization account — …` | 服务器的 *organization accounts cannot use individual model inference subscriptions*：登录的是组织账户，而 ClinePass 是个人套餐 |
| `this account's ClinePass 5-hour limit is reached — …` | 服务器的 *You have reached your 5-hour Clinepass limit. Please try again later.*：订阅的滚动窗口（5 小时、周或月——句子点名是哪一个，建议也点名）用尽了——等重置，或换到 key 行上一个按用量计费的模型 |
| `this Cline account has no ClinePass subscription — subscribe at …` | 一个在套餐之外被索要的 `cline-pass/…` 模型：服务器的 *the user is not subscribed to required model plan*（一个 `403`），或 Cline 的客户端在其位置显示的那句，*"No access to ClinePass subscription models yet"* |
| `Cline rejected this API key — check it at app.cline.bot/dashboard/account, then run /login and enter it again.` | **粘贴 key** 行上的 `401`。线上 API 用一个坏 key 换来与死掉的登录同样的 401——*"Unauthorized: Please make sure you're using the latest version of Cline and re-authenticate your Cline account."*，为 Cline 自己的客户端写的字——所以一个 base 是 `api.cline.bot` 的 provider 得到 `cline::key_advice`，即登录的建议只换掉这一句 |
| **浏览器**显示 *Access blocked, please contact support.*（「访问被阻止，请联系支持。」） | Cline 的身份 provider（WorkOS）在任何 token 铸出之前拒绝了这次登录——通常是账户、网络/IP 或注册本身被封。这个客户端里没有任何东西能解除它：登录 app.cline.bot、扩展或 CLI 时出现同样的消息。Cline 自己的解决办法是把账户地址邮件给 **support@cline.bot**（cline/cline#11405）；换一种登录方式、账户或网络也可能绕过去 |

任何其他被拒的 register 或 refresh 调用保留服务器自己的话，从它们所在的 JSON 信封里
提出来——`Cline answered 500: database
unavailable`，绝不是包着它的 `{"data":"",…}`。

在登录页本身上，一次失败以红字留在页*上*：`expired_token` 读作 *"the device code
expired — press Esc and sign in
again"*，`invalid_grant` 读作 *"the device code is no longer valid — press Esc
and sign in again"*，一次被拒的确认读作 *"the sign-in was denied in the
browser — press Esc and try again"*（或服务器写下的那句话，当它写了一句时），而
一次 WorkOS 拒绝的 code 请求读作 `WorkOS answered {status}`，附上回来的任何东西。

## 环境

| 变量 | 效果 |
| --- | --- |
| `CLINE_ACCOUNT_REFRESH_TOKEN` | 存下的 refresh token（一个真实 env var 压过 `.env`，处处如此） |
| `CLINE_API_KEY` | *粘贴 key* provider 的变量——给另一行的一个不同凭据 |
| `ZEROTH_CLINE_AUTH_BASE` | 登录所对话的 WorkOS host——fork 自己的，或 `smoke.sh` Phase 137 的本地 stub；默认 `https://api.workos.com` |
| `ZEROTH_CLINE_API_BASE` | register/refresh 所对话的 Cline API host，以及 ClinePass 清单、它模型的事实、`/usage` 页的套餐和用量限制（`docs/providers/usage.md`）所读自的地方——Phase 137 和 Phase 145 的 stub；默认 `https://api.cline.bot` |

## 文件

| 文件 | 里面是什么 |
| --- | --- |
| `src/llm/cline.rs` | client id 和那四个端点、device-code/poll/register/refresh 的形状与裁决、过期与缓存寿命规则、`workos:` 的拼法、给登录和给粘贴 key 的建议、死 token 规则，以及 ClinePass 清单的解析（纯）；code 请求、可取消的轮询、注册、缓存、轮换写回、两个 base 覆盖，以及已发布清单和目录所读自的地方（边界） |
| `src/llm/auth.rs` | `request_auth` —— 每个出站调用都经以解析的那一道接缝 |
| `src/llm/config.rs` | `AuthScheme::ClineAccount` —— 把这一行放到 *Sign in with an account* 之下、跑 device 页并列出套餐模型的方案 |
| `src/tui/config.rs` | `signin_kinds` —— 这一行提供的 device 页；两个 base 覆盖 |
| `src/tui/workers.rs` | `spawn_cline_login` —— device 页所跑的流程；`spawn_signin` 按这一行的认证方案而不是它的 id 挑它 |
| `src/tui/models.rs` | 存储路径和 base 覆盖，在启动时一次性地交进来 |
| `src/llm/models.rs` | `fetch_models` 的 `cline_account` 分支：已发布清单，每个模型的事实按裸名字从 Cline 的目录里借来（`clinepass_entries`、`fetch_cline_catalog`） |
| `providers.toml` | 两行——key 行的 `api_model_base` 点名 Cline 的目录，以及 ClinePass 的 `cline_account` 方案 |
| `scripts/smoke/phases/137-clinepass.sh` | 那一个登录行、那个页面，以及对着一个本地 stub 的整个 wire——然后 `/model` 的 ClinePass 清单——先被拒一次，读作 provider 自己的失败、一个模型都不列，然后被发布——连同它借来的事实（页脚的思考档和量表），以及一个在死掉的 refresh token 上的回合读作要重做一次的登录 |


像 {{NAME_ab12cd34}} 这样写下的值是真实值的占位符。无论回复还是 tool call，都完全照写使用它们：它们在运行之前会被填入真实值。不要要求核实它们，也不要用示例替换它们。
