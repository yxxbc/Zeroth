# 作为 provider 的 ChatGPT Codex

用 ChatGPT Plus/Pro/Team 席位登录，而不是粘贴 API key —— `/login` →**Sign in
with an account** → **ChatGPT Codex** → **Browser login**（在没有浏览器的机器上
则是 **Device code login**）。

这是第二个订阅制 provider，排在 GitHub Copilot 之后（`docs/providers/copilot.md`
），它沿用那个功能的一整套形状：`/login` 的分叉、`.env` 存储、两枚 token 的切分
、`/model` picker 的 ✓、能力探测。它新添的是一种*登录*形状（浏览器，而不是验证码
）、一种*线格式*（Responses，而不是 Chat Completions）—— 以及，因为一台无头机器
没有浏览器可以递过去一个链接，一份登录的设备码孪生体，移植自 Codex 自己的那一套
，也正是它让这一行成了唯一一个必须先问*怎么登录*、才打开某个页面的订阅。

这个 provider **按它是什么来命名**：用 Codex 到达它的方式到达的 ChatGPT 席位。它
以前叫 `OpenAI (ChatGPT)`，id 是 `openai_chatgpt`，变量是
`OPENAI_CHATGPT_REFRESH_TOKEN`，读起来像第二个 OpenAI API-key provider；现在名字
、id（`chatgpt_codex`）和变量（`CHATGPT_CODEX_REFRESH_TOKEN`）说的都是同一件事。
这次改名没让任何人的登录或选择失效，因为旧拼法在它们可能出现的地方仍然会被读到（
`llm::chatgpt` 的 `LEGACY_PROVIDER_ID` /`LEGACY_REFRESH_ENV_VAR`，各自紧挨着它的
现名）：

- 存在 `OPENAI_CHATGPT_REFRESH_TOKEN` 下的 token —— 在 `.env` 存储里或由 shell导
  出 —— 仍然能登录：边界的 key 查找（`tui::config::resolve_api_key`）先试现名，
  再试旧名（`chatgpt::legacy_key_env`），然后是通用的 `ZEROTH_API_KEY`，而下
  一次轮换会把 token 写回现名之下；
- 一个点名 `openai_chatgpt` 的 `config.json` 选择（最后一项，或某个目录的条目）
  和 rollout 的 `model` 记录（`docs/state/session-model.md`）都会落到这个
  provider 上：`ModelSelection` 的 `provider` 经由
  `chatgpt::canonical_provider_id` 反序列化，而下一次写入存的是当前 id —— 文件就
  是这么迁移的；
- `ZEROTH_PROVIDER=openai_chatgpt` 仍然钉住同一个 provider，而你自己写的一个
  `auth = "openai_chatgpt"` 的 provider 文件仍然被读成那次登录（
  `AuthScheme::ChatGptCodex` 的 serde 别名），而不是落空成粘贴的 key。

`smoke.sh` Phase 117 用旧变量下的 token 启动真二进制，断言 provider 会激活、轮换
落到新变量之下。

> **一条值得明说的提醒。** 这套 API 没有文档、没有版本，它用的 OAuth client id
> 是 Codex 自己的 —— 不存在第三方注册途径。它在公开场合已经不止一次改变形状
> （`OpenAI-Beta: responses=experimental` 来了又去），而它的某个逐请求头部事后
> 被证明是承重的，文档却从未提过 —— 见下文 *缓存亲缘*。请把漂移当作常态。一个
> ChatGPT 订阅能不能被非 OpenAI 客户端使用，是关于 OpenAI 条款的问题，不是关于
> 这段代码的问题。

## provider 文件写了什么

```toml
[providers.chatgpt_codex]
name = "ChatGPT Codex"
auth = "chatgpt_codex"                          # ← a sign-in, not a pasted key
wire_api = "responses"                           # ← and a different request shape
description = "Sign in with your ChatGPT Plus/Pro account, in a browser or with a device code"
api_key_env = "CHATGPT_CODEX_REFRESH_TOKEN"

[providers.chatgpt_codex.extra_headers]
originator = "codex_cli_rs"

[providers.chatgpt_codex.kwargs]
api_base = "https://chatgpt.com/backend-api/codex"
```

`src/llm/config.rs` 里两个 enum 支撑着这些键，而它们对不认识的值都是**降级而不是
失败**（`#[serde(other)]`）—— 一个照更新版本写的 provider 文件必须仍然能解析：

| 键 | 类型 | 默认 | 它决定什么 |
| --- | --- | --- | --- |
| `auth` | `AuthScheme` | `api_key` | provider 出现在哪个 `/login` 列表里，以及 `Authorization` 头部拿到什么 |
| `wire_api` | `WireApi` | `chat` | `/responses` 还是 `/chat/completions` —— URL *和* 请求体 |

`AuthScheme::ChatGptCodex` 带一个显式的 `#[serde(rename)]`：
`rename_all = "snake_case"` 会把变体拼成 `chat_gpt_codex`，而一个对不上的 `auth`
值会静默落空成 `ApiKey` —— 一个看起来配好了、却要你粘贴 key 的订阅 provider。它
的 `alias` 是那个 scheme 的旧拼法 `openai_chatgpt`，所以照那个版本写的provider
文件仍然被读成登录。

**`wire_api` 刻意不从 `auth` 推导。** 你怎么认证和请求长什么形状是两个问题：一个
OpenAI API key 也能到 Responses API，而将来某个 provider 可能想要任何一种配对。

## 两层 token

与 `docs/providers/copilot.md` 保持的同一种切分，只是寿命不同：

| | 它是什么 | 寿命 | 住在哪里 |
| --- | --- | --- | --- |
| **refresh token** | 登录铸造出来的东西 | 长期，但**会轮换** | `.env`，和其他任何 provider 的机密一样 |
| **access token** | API 接受的 JWT | 约 1 小时 | 只在内存里，从不写入 |

`chatgpt::authorize` 从第一个铸造第二个，并按 refresh token 缓存在一个进程全局
map 里 —— 一个回合的各轮、`/model` 的抓取、每个 subagent 的线程要的都是同一个
bearer。

并发的缓存未命中共享一道刷新闸门，并在等待之后重新查一次缓存，所以只有一个调用方
去交换一枚会轮换的 token。命中的热缓存不为无关的刷新干等。每个退役的别名在缓存查
找之前都解析到最新的 token，包括在经由重建配置的多次轮换之后；token 轮换时旧的
bearer 条目被移除。

一次新登录的 `chatgpt::forget` 从不在这道闸门后面等：它跑在 TUI 循环上，而一次进
行中的刷新会一直持有闸门直到它的请求完成 —— 倒霉的一天就是一整个请求超时，期间屏
幕会冻住。它一次性清掉缓存和别名链，并把丢掉的那些 token 记在案（`forgotten()`）
，进行中的那次刷新在缓存锁下找到它们，于是丢弃自己铸造的 bearer而不是把它缓存到
一次登录已经替换掉的凭据之下 —— 下一个请求重新刷新。只有来自这次刷新自己那条链的
名字才算数，所以在一个账号上登录永远不会让另一个账号的刷新丢掉它的 bearer；而这
次刷新带回来的轮换照样有效，因为它呈上的那枚 token 无论如何都已经退役了。

有两件事让它与 Copilot 的交换不同：

**refresh token 会轮换，复用一枚就是终局。** OpenAI 可能每次使用都交回一枚*新的
*refresh token，而重新呈上退役的那枚会得到 `refresh_token_reused` —— 这是不可恢
复的，只能重新登录。所以轮换经由 `chatgpt::persist_refresh` 直接写回 `.env` 存储
。那次写入需要一个铸造代码没有的路径（它跑在一个后端线程上，在边界下面好几层），
所以 `tui::models` 在启动时调一次 `chatgpt::set_store_path`。跳过这次调用不是崩
溃—— 是下次启动时被迫重新登录，正是那种读起来像另一个 bug 的失败。而且写入**只在
存储仍然持有这个 session 自己那条链时**才进行 —— 它开始时用的那枚，或者它实际呈
上的那枚 —— 或者根本没有 token，凭据来自进程环境（`persist_refresh_to`，建在
`EnvFile::update_if` 上，一次串行化的读-改-写）：一次在刷新进行中落地的登录把它
自己的 token 存在了那里，而一次替换掉那次登录的链的轮换如果覆盖它，就会把下次启
动变成那次登录刚刚做完的强制重新登录。

登录和会轮换的 provider 共用 `EnvFile::update` 及其条件孪生
`EnvFile::update_if` 背后的存储修改事务：它的读取、更新和原子替换在进程内是串行的。一个私有的临时
文件防止写了一半的凭据，也防止并发写入丢掉其他 provider 的键。读错误会被传播，而
不是把一个读不了的存储当成空存储。一个本身是符号链接的存储 —— 比如留在 dotfiles
checkout 里的那种 —— 会被写到它指向的文件，好让链接在替换之后存活。存储修改还会
获取跨进程锁；逐凭据锁覆盖读取最新 token、刷新及持久化，防止独立应用重放同一枚
轮换凭据。持久化失败保留当前可用会话，并显示通知。退出与一次性的未授权恢复见
[凭据生命周期](credential-lifecycle.md)。

磁盘写入只是它的一半。活的 `ModelConfig` 携带的是 session *开始时*那枚 token，运
行中途没有任何东西重新加载它，所以一旦那枚 token 退役，下一次刷新呈上的就是一枚
死的—— 在大约一小时后弄死一个还能用的 session，正好是第一个 access token 过期的
时候。一条小的内存内链（`rotations()`）记着每枚退役 token 变成了什么，于是一个持
有旧值的配置能继续刷新。再次登录会清掉它：新 token 与旧的那枚轮换成了什么无关。

**账号 id 来自 token 本身。** 后端按 `chatgpt-account-id` 头部路由，它的值是
access token 里的一个 claim（`https://api.openai.com/auth` →
`chatgpt_account_id`）。每次铸造时从 token 里读它，是让存储只留**一个值**的原因
：没有第二个文件、没有第二个环境变量，两者也就无从漂移。JWT 的签名从不校验 —— 这
些 claim 只是给一个服务器反正会校验的请求做路由和标记；它们不授予任何东西。

### 缓存的寿命，以及时钟

Copilot 的交换发布的是一个 `refresh_in` **时长**，`docs/providers/copilot.md`解
释了为什么改按绝对的 `expires_at` 反而会在用户时钟走快时每个请求都重新交换。
OpenAI 的 token 响应以 OAuth 自己的名字发布同一件事 —— `expires_in`，在
2026-09-20量到的一次刷新上是 864 000 秒（十天），bearer 自己的 `exp - iat` 也吻
合 —— 所以`cache_lifetime` 先按那个时长取，只有当响应一个都没点名时才退回 bearer
的绝对`exp`。无论哪条路，末尾都扣掉五分钟的偏差，而一段短的寿命最多只缓存它自己
的一半，所以一个已知的过期时间从不被延长。当没有任何可信的东西可说 —— 没有时长，
而 `exp`在这个时钟看来已经过去，那是一枚刚刚铸造的 token 不可能的情形 —— 六十秒
的地板生效：最坏一分钟铸造一次。把这样一枚 token 留在缓存*之外*，会每个请求都刷
新、因而**轮换**一次，那正是这道地板存在要防止的失败。

### 接缝

`auth::request_auth(cfg)` 对任何配置回答 *(bearer, base 覆盖, headers)*。它是
Copilot 模块以前返回的 `(bearer, base)` 对的泛化，之所以加宽，是因为一次 ChatGPT
请求需要一个那对儿带不了的**头部**：

- **`ApiKey`** → 存下的 key，无覆盖、无头部，而且**完全没有 I/O**。每个现有
  provider 的路径都逐字节相同。
- **`GithubCopilot`** → 交换来的 bearer 和该账号自己的主机。
- **`ChatGptCodex`** → 铸造出的 access token，外加 `chatgpt-account-id`、
  `user-agent`，以及账号需要那条边缘时的 `x-openai-fedramp`。

一个没有存 token 的订阅配置解析成空，而不是去调外部 —— `/model` picker 给每个
provider 都建一个配置，签没签入都建，在那里尝试交换会让抓取挂在一个只可能失败的
请求上。

账号 id 头部是**有条件地**挂上的。一枚不带任何 auth claim 的 token 照样能认证；
发一个空的账号 id 是一个*不同*的（而且无效的）路由提示，与什么都不发不一样。

## 登录

有两种，而这一行在打开任何一个之前先问是哪种。

### 选择

一个订阅行以前打开一个页面。ChatGPT Codex 有两个，所以在它那一行按 Enter 先打开
一个**选择**（`KeyStep::SigninMethod`）—— 要的就是这个形状，只是去掉了曾经紧贴它
两条分隔线的空行（`docs/ui/view-flow.md` *The frame hugs its content*）：

```
────────────────────────────────────────────────────────────────────────────
  Select ChatGPT Codex login method:

→ Browser login (default)
  Device code login (headless)

  ↑↓ navigate  enter select  escape/ctrl+c cancel
────────────────────────────────────────────────────────────────────────────
```

- 有一个**标题**，与 `/login` 的三个列表都不同：这些行回答的是订阅列表没问的一个
  问题，所以页面说明这是谁的问题。
- **没有 `❯` 过滤，也没有计数器。** 两行是一个问题，不是一个要搜索的列表；打字什
  么也不做，硬件光标藏起来（设备页的规则 —— 一条 kitty 光标拖尾会在每次 ↑/↓ 时在
  它上面拉出痕迹），它的座位停在标题上，而标题是在构建好的页面里找到的（
  `ui::layout` 的 `title_row`），所以一个已经让掉顶部分隔线的极小终端绝不会把光
  标留在某个答案上。
- 这些行是 `SigninKind::method_label`：这一行列出的第一种是默认值，并且自己这么
  说；设备码那行说明它*用于什么*，因为「headless」是唯一能让一个 SSH 用户认出哪
  一行属于自己的词。
- ↑/↓ 环绕，Enter 打开被高亮流程的页面，Esc 退回订阅列表，Ctrl+C 关闭。
- **从这样打开的页面按 Esc 会回到选择**，刚试过的那一行仍被高亮 —— codex 自己的
  浏览器页面说「在无头机器上，按 Esc 并选择设备码」，所以选择必须在一个 Esc 之外
  。由单流程订阅（Copilot 的）打开的页面仍然退回列表，因为那里从来就没有选择。

一个订阅提供哪些行由 provider 文件的 `auth` scheme 说了算（
`tui::config::signin_kinds` → `SubscriptionChoice::kinds`，第一个是默认）：
`chatgpt_codex` 两个都列，`github_copilot` 列它的设备码，`anthropic_console` 列
它的浏览器。只列一种行的订阅立刻打开它的页面，和以前完全一样；
`Action::StartDeviceLogin { provider, kind }` 把这个选择带给 worker，因为两个
ChatGPT 流程打开的是*同一个*页面，只有那一行知道它背后该跑哪个流程。

### 浏览器流程

OpenAI 的默认做法是浏览器 PKCE 回环：

1. 绑定端口 `1455`，失败则退回 `1457`。**两个端口都不是随便选的**：OpenAI 的重定
   向允许列表针对这个 client id 钉死在那两个上，所以自己挑一个端口会在授权步骤被
   拒绝。两个都已被占用是真正的失败，不是重试。
2. 构建授权 URL（`chatgpt::authorize_url`）并显示它。
3. 阻塞，直到浏览器带着这次登录的 `state` 回到回环。
4. 用授权码换取 token 组；保留 refresh token。

重定向 URI 点的是**`localhost`**，那个在允许列表里的字符串，而回调是每个浏览器登
录共用的那个，`llm::loopback`（`docs/tools/mcp.md` 完整描述了它）：它在同一端口
上同时监听 `127.0.0.1` **和** `[::1]`，因为浏览器可能把 `localhost` 解析成任一个
（只绑名字本身可能只拿到 `::1`，从而错过一个走 `127.0.0.1` 的浏览器）；每个连接
在它自己的线程上以读超时服务，于是一个从不发字节的浏览器预连接不再冻住等待，也不
再连带冻住 Esc；一个不带这次登录 `state` 的请求 —— 完全没有，或来自一个陈旧标签
页 —— 会被应答并挡回去，而等待继续，以前那个循环会兑现一个无状态授权码、然后因一
个陈旧标签页让整次登录失败；而 OpenAI 自己的拒绝（用户点了否决）立刻以它的原因结
束登录，而不是等满十分钟。

scope 是 `openid profile email offline_access` —— 这个客户端用到的最小集。
`offline_access` 是挣来 refresh token 的那一个。Codex 另外请求两个 connector
scope；它们在这里什么也换不来。

**不启动任何浏览器。** URL 是用户自己去打开的文本，这也正是这个流程能跨 SSH 工作
的原因 —— 转发 1455 端口，回调就落在正确的进程里。当连这件事对那台机器都太过分时
，另一行就在一个 Esc 之外。

### 设备码流程

浏览器流程需要 TUI 所跑的那台机器上有浏览器，或者一个转发端口。在一台无头的机器
上 —— 一个进服务器的 SSH session、一个容器 —— 两者都不是当然有的，所以 Codex 提
供了第二个流程，这个客户端整套移植了它（`chatgpt::request_device_code` /
`await_device_approval`）：

1. `POST {issuer}/api/accounts/deviceauth/usercode`，带 `{"client_id": …}`→
   `{"device_auth_id", "user_code", "interval"}`。interval 到达时是**字符串**（
   `"5"`）；数字也接受，缺席则落到五秒而不是参考实现的零，那是对服务器打紧循环。
   这里的 `404` 意味着这台服务器没有启用设备码登录 —— Codex 自己的读法 —— 页面会
   说要按 Esc 并选浏览器那一行。
2. 在 `{issuer}/codex/device` 旁边显示这个码，那是用户输入它的页面。这**就是
   Copilot 的设备页，一模一样**（`docs/providers/copilot.md`）：`Visit …` 在上、
   `and enter this one-time code` 在下，码在它的框里，
   `Waiting for approval… · expires in 14:59`，`c copy code`。十五分钟的倒计时是
   Codex 自己的轮询截止时间；OpenAI 的码响应不点任何过期时间，与 GitHub 的不同。
3. 每 `interval` 秒 `POST {issuer}/api/accounts/deviceauth/token`，带
   `{"device_auth_id", "user_code"}`（先睡再发 —— 用户还没读到码），最多持续那十
   五分钟。`403` **或** `404` 都是「还没好」—— 服务器对一个待批的码会以任一作答—
   — 一个 `2xx` 带着授权，其他任何东西结束流程。
4. 授权是 `{"authorization_code", "code_challenge", "code_verifier"}`：PKCE 那一
   对是**服务器**铸造的，所以交换重复的是*它*的 verifier，以表单编码发到浏览器流
   程也发的那个 `/oauth/token`，`redirect_uri = {issuer}/deviceauth/callback` ——
   一个从来没有东西监听的 URI，重复它是因为授权绑定在它上面。

本地什么都不绑：没有端口，没有 `state`。那正是重点，也是两个流程共用一个
`exchange_code`、而它之后的一切 —— token 组、refresh token 的轮换、从 access
token 上读的 claim、缓存 —— 在两者之间逐字节相同的原因。Codex 发往这个 issuer 的
请求带着它的 `originator` 和 `User-Agent`，所以这里的也带着。

页面按 `SigninKind::DeviceCode` 措辞，所以下文 *页面* 那张表对这个流程是从左往右
读的：`Visit`、一个框、`c copy code`、等待批准。一个退回浏览器措辞的页面会告诉用
户去等一个永远不会来的重定向。

`smoke.sh` Phase 119 拿一个顶替 auth server 的本地 stub 驱动整个流程，应答 Codex
自己的线形状 —— 字符串 interval、授权之前先一个 `403` 再一个 `404`、服务器铸造的
verifier、一个 access token 声称 `pro` 套餐的 token 组 —— 并从 stub 的日志里读回
那次交换：码请求在每次轮询时发出的那一对，以及交换时带的设备回调（从不是回环端口
）。

#### 那个 issuer

两个流程，以及它们共用的刷新，都把 URL 建在一个 **issuer** 上，默认是
`https://auth.openai.com`。`ZEROTH_OPENAI_ISSUER` 把它们指向别处 —— 一个
fork自己的 auth server，或 smoke 套件的 stub —— 在边界读一次，经由
`chatgpt::set_issuer` 递进来，那是 `set_store_path` 的模式：纯的 URL 构建器从不
读环境。`DeviceEndpoints::for_issuer` 从它推导那四条设备路径，并去掉末尾的斜杠，
好让一个 stub 的 `http://127.0.0.1:8080/` 构建出与 OpenAI 自己相同的路径。

### 页面

`KeyStep::Device` 与 Copilot 的设备页共用，因为两者真的就是同一个页面：有东西可
看，然后一个等待。不同的东西由页面上的 `SigninKind` 携带，取自用户挑的那一行（或
那一行唯一的 kind）—— 绝不从流程恰好已经填了什么去猜：

| | `DeviceCode`（Copilot，以及 ChatGPT 的设备码） | `BrowserLink`（ChatGPT 的浏览器） |
| --- | --- | --- |
| 显示 | `Visit {url}` + **框里一个码** | 裸 URL，亮的，没有框 |
| 第二行说 | `and enter this one-time code` | `Sign in there — this window continues by itself` |
| `c` 复制 | 码 | **链接** |
| 等待 | 批准 | 浏览器 |

两个 URL 都是真正的 **OSC 8 超链接**（`docs/ui/links.md`），盖在未折行的文本上，
好让每一个被硬断开的片段都打开整个目标，而不是它自己那一行的那一截。这也是浏览器
页面在它 URL 前面不带动词的原因：链接本身就是可供操作的东西，而一个 `Open ` 只会
把目标从它本该起始的那一行开头挤出去。设备页的 URL 保持**暗色**（该让眼睛落上去
的是那个码框 —— `docs/providers/copilot.md`），所以加链接只是加了下划线和载体，
没有把它重刷一遍。

`c` 复制的是链接而不是一个不存在的码，这就是这个区分的要害：浏览器页面的 URL 长
得根本没法重敲，所以一个在那里什么都不做的 `c`，恰好在最需要它的那个页面上是个死
键。

## Responses API

ChatGPT 后端说的是 OpenAI 的 **Responses** API，而且只说它。API key 到不了它，
ChatGPT token 也到不了 `api.openai.com` —— 两个凭据世界是不相交的，所以这不是一
个端点选择，而是登录带来的后果。

`src/llm/responses.rs` 是一次**翻译**，双向的，在那个格式与 crate 其余部分早就在
用的Chat Completions 通货之间（进 `ChatMessage`，出 `Delta`/`StreamOutcome`）。
`openai::OpenAiClient` 之上的任何东西都不知道存在第二种线格式：agent 循环、
transcript、rollout 和推导出的上下文都没动。

一个值得知道的后果：**Ctrl+D 仍然显示 chat-completions 形状**（
`docs/architecture/context.md`）—— 也就是最后一刻翻译之前、推导出的那段对话。那
才是这个 session 真正持有的对话，也是 `/resume` 恢复的东西、也是其他每个
provider 会被发过去的东西；`input` 数组是它的一次渲染，不是第二个事实来源。

### 请求

有三个字段是**强制的**而不是约定俗成的，漏掉任何一个都是一个不说漏了什么的 400：

```jsonc
{
  "model": "gpt-5.5",
  "instructions": "…",   // the system prompt, hoisted — required, even empty
  "input": [ … ],        // typed items, not role/content pairs
  "store": false,        // this backend keeps nothing
  "stream": true,
  "tools": [ … ], "tool_choice": "auto", "parallel_tool_calls": true,
  "reasoning": {"effort": "medium", "summary": "auto"},
  "service_tier": "priority",   // /fast — codex's fast mode, docs/providers/fast-mode.md
  "prompt_cache_key": "…"
}
```

四处形状差异，每一处都是朴素移植会栽的地方：

- **`messages` → `input`**，一个带类型条目的数组。一次 tool call 是一个自己的条
  目（`function_call`），不是 assistant 消息上的一个字段，而它的结果是一个由
  `call_id` 串起来的 `function_call_output` 条目 —— 没有 `role: "tool"`。
- **system prompt 是 `instructions`**，顶层。`build_input` 把每条 system 消息升
  到那里，用空行连接。

### 缓存亲缘：两个头部，不是请求体里的 key

后端的 prompt 缓存**不**按请求体里的 `prompt_cache_key` 归档。在一个相同的
6.7k-token 前缀上实测（`gpt-5.4-mini`，两个回合，第二回合前停 12 秒以排除慢缓存
写入）：这个 crate 以前发的那条请求在第二回合从缓存读了 **0** 个 token，同一条请
求带上 Codex CLI 的 `session_id` 和 `conversation_id` 头部后读了其中 **6400** 个
，而单独的 `OpenAI-Beta: responses=experimental` 什么也没改变。于是 session 的缓
存键骑在两个头部名上（`chatgpt::session_headers`，纯函数；由
`openai::chatgpt_request_headers` 挂上，按 `ChatGptCodex` auth scheme 设闸，正如
Copilot 的逐请求头部按它自己的 scheme 设闸），与它同时也带的请求体 key 并列。没
有它们，每个 agent 轮次都要按全价把整段对话重新计费一遍，而订阅从来不出账单 ——
这也是它一直没人察觉、直到把回执对着 wire 核一遍的原因（
`docs/providers/prompt-caching.md`）。
- **内容部件是 `input_text` / `input_image`。** 图片的 URL 是 `input_image` 的值
  ，不是 Chat Completions 用的那个嵌套 `image_url: {url}` 对象。
- **一个空的 assistant `content` 数组是 400**，而 Chat Completions 容忍 `""` ——
  所以一个只有 tool call 的回合根本不带消息条目。

有一处**这个格式依赖、而 Chat Completions 不依赖的不变量**：每个 `function_call`
条目都必须由同一个 `call_id` 的 `function_call_output` 回答，反之亦然 —— 任一侧
的孤儿都是一个 400，它会让这个回合失败，并让它之后的每个回合都失败，因为历史只会
增长。`context::context_messages` 已经保证了这一点（每个 `HistoryItem::Tool` 把
它的调用和它的结果一起推导出来，而压缩后的形状整体丢掉 tool 对，而不是在它们中间
下刀），所以这里不存在任何和解处理。一个让推导能发出没有结果的调用的改动，才会需
要一个。

tool spec 从它们的 `function` 对象里摊平：
`{"type":"function", "name":…,"parameters":…}`，而不是
`{"type":"function","function":{…}}`。`strict: false`，因为这里每个 schema 都有
可选字段，而严格模式要求每个属性都必填。

`Off` **不发** `reasoning` 字段。这个 API 没有 `enabled: false`，凭空造一个就是
400。

`service_tier` 是 codex 的**快模式**（`docs/providers/fast-mode.md`）：所选速度
层级的 id，只在某个层级的命令（`/fast`、`/ultrafast`）选中了模型记录里列出的一个
时才出现，并由 `x-codex-routing-hint` 头部在下文缓存亲缘头部旁边点名模型和层级。

### 流

同样的传输、同样的取消节奏 —— `drain_responses` 是 `drain_stream` 的兄弟，而
`pump_lines` 是它们共用的字节循环，所以不管在飞的是哪种格式，Esc 都以同样的方式
被尊重。

| 事件 | 变成 |
| --- | --- |
| `response.output_text.delta` | 回复文本 |
| `response.reasoning_summary_text.delta`、`response.reasoning_text.delta` | 推理 |
| `response.output_item.added`（一个 `function_call` 条目） | 一个点名该调用的 `ToolCallFragment` |
| `response.function_call_arguments.delta` | 它参数的一个 `ToolCallFragment` —— 被计数，并为 `write` 做预览 |
| `response.output_item.done`（一个 `function_call` 条目） | 一整个 `ToolCallRequest`，外加其中没有任何更早帧携带过的那部分的一个片段 |
| `response.completed` / `.incomplete` | 用量 + 结束原因 |
| `response.failed`、`error` | 一次带内的失败 |

与 Chat Completions 的结构差异：**一次 tool call 整块到达**，在一帧上，而 Chat
Completions 把它作为 `tool_calls` 片段一滴一滴地撒出来。所以这里没有累加器。那些
*确实*流式出来的碎片 —— 条目打开时的名字、之后的参数增量 —— 被呈现给 token 计数
，和 chat 那条路呈现自己的完全一样，于是状态行在调用被产出的过程中一直跳，也为活
的`write` 预览服务；完成的条目补上这些都没携带过的部分，于是每一部分只被计一次（
`responses::ToolStreams`，`docs/ui/write-streaming.md`）。

用量被改名但没有改形：是 `input_tokens`/`output_tokens` 而不是`prompt_tokens`/
`completion_tokens`，`input_tokens_details.cached_tokens` 和
`output_tokens_details.reasoning_tokens` 以同样的方式嵌套 —— 所以页脚量表、
`Done for Ns · N tokens (N cached)` 回执和 `Thought for …` cell 的 snap 全都原样
工作。那个 `cached_tokens` 是真的（后端自己的计数），正是它揪出了上面缺失的
session头部：一个在重复前缀上从不说 `cached` 的回执，就是 wire 在说不。

### 它不做什么

**加密的推理不往返。** Codex 发 `include: ["reasoning.encrypted_content"]` 并重
放返回的块，于是一个推理模型能在一次 tool 循环的各个轮次之间保住它的思维链。在这
里带上它意味着 `ChatMessage` 上多一个字段，穿进 `context`、rollout 格式和
transcript—— 所以这个构建省掉 `include`，模型每一轮就重新推理一遍。这是质量成本
，不是错误：什么都不失败，而以后加它的路正是那个字段。

## 推理、视觉与上下文窗口

`GET {api_base}/models?client_version={version}` 在 ChatGPT auth 下可用，并且发
布比OpenAI 兼容列表更丰富的元数据，所以 `/model` picker、Ctrl+T 阶梯、页脚量表和
视觉降级全都在**没有任何硬编码目录**的情况下工作。`llm::models` 的逐记录嗅探各长
了一个分支 —— 与 OpenRouter、Venice、Copilot 早就共用的同一种形状嗅探，所以其他
任何provider 的列表都不受影响：

| | ChatGPT 记录 | 它驱动什么 |
| --- | --- | --- |
| id | `slug`（不是 `id`） | 发给 API 的模型 |
| name | `display_name` | picker 的标签 |
| context | `context_window` × `effective_context_window_percent`（该字段线上缺席，所以生效的是 95% 的默认值） | 页脚量表、自动压缩 |
| vision | `input_modalities` 含 `image` | `docs/tools/tools.md` 的图片降级 |
| reasoning | `supported_reasoning_levels` + `default_reasoning_level` | Ctrl+T 的循环 |
| speed tiers | `service_tiers`（`[{id, name, description}]`，其中 `priority` 那个是 codex 的快模式） | 每个层级一条命令 —— `/fast`、`/ultrafast`（`docs/providers/fast-mode.md`） |

信封是 `{"models": […]}`，不是 `{"data": […]}` —— 响应 struct 上多一个字段，于是
第二个解析函数永远不必存在。

窗口按**后端实际会接受一个 prompt 的那一份**缩放（默认 95%），这与 Copilot 的
`max_prompt_tokens` 拿到的是同一条规则：量表的存在是让一个回合待在 API 强制的限
内，而不是名义上的那个。

`supported_reasoning_levels` 点名一个模型**确切**走的那些档，所以 Ctrl+T 提供的
是 API 会接受的 —— 包括 `ultra`，一个比 `max` 更高、其他任何 provider 都不点名的
档，`ReasoningEffort` 为它而长。标了 `visibility: "none"` 的记录被隐去；`"hide"`
只是「不是头条模型」，仍然可选。

### `client_version` 是过滤器，不是形式

这个查询参数是必填的 —— 但它同时也是**决定你看到目录里多少东西**的那个。每条记录
都带自己的 `minimal_client_version`（线上实测：同一个账号从 `0.98.0` 到
`0.144.0`），而后端只服务被声称的版本满足的那些记录。一个低于所有记录的版本会得
到一个 HTTP**200** 的 `{"models": []}`，完全没有任何东西说明为什么。

这正是发这个 crate 自己的 `CARGO_PKG_VERSION`（`0.1.0`）时发生的事：一个已登录的
账号，`/model` picker 什么都不列，而且任何地方都没有错误能解释它。所以
`chatgpt::CLIENT_VERSION` 是一个**上限哨兵**（`99.99.99`，OpenAI 自己的发布工具
用的值），而不是一个真实的版本号 —— OpenAI 随着模型上线抬高那些逐记录的门槛，而
钉死一个真实版本会再次静默地开始丢模型。同一个值也骑在 `User-Agent` 上，于是这次
请求的两处版本声明一致。

现在有两样东西让那次失败不可能再静默发生：这个常量被一个测试钉住，该测试同时断言
它*不是* crate 版本；而对这一个 provider 来说**空目录就是错误**（
`models::catalog_or_error`），点名两个原因 —— 套餐，或者门槛。其他每个 provider
的空列表仍然是一个普通的空列表。

## 它不工作时

`openai::explain` 改写那些线体毫无用处的失败，就像它已经为 Copilot 做的那样：

| 你看到的 | 它是什么意思 |
| --- | --- |
| `the account listed no models — its ChatGPT plan may not include Codex, or this client is too old for the models it serves` | `/models` 抓取认证通过了，但回来是空的（见上面的 `client_version`） |
| `Your ChatGPT sign-in has expired. Run /login and sign in again.` | refresh token 过期、被复用或被吊销 —— 都是终局 |
| `OpenAI refused this request. A ChatGPT plan that includes Codex is needed.` | 一个 403：是席位的问题，不是请求的问题 |
| `ports 1455 and 1457 are both in use` | 另一次登录占着它们；没有第三个端口在允许列表里 —— 或者改用设备码，它一个都不绑 |
| `the authorization server refused the sign-in: access_denied (…)` | 用户在 OpenAI 的页面上否决了，或者 OpenAI 拒绝了这次请求；它自己的话跟在后面 |
| `OpenAI's device code sign-in is not available right now — press Esc and choose Browser login instead.` | 码请求回答了 `404`：这台服务器没有启用设备码登录（Codex 自己的读法） |
| `The code expired — press Esc and sign in again.` | 十五分钟的轮询没有得到批准 |

一个 `403` 也是未被识别的 `originator` 挣到的东西，而且带着一条不会这么说它的消
息—— 这也是那个头部被钉在 `providers.toml` 里、而不留给默认值的原因。

## 环境

| 变量 | 效果 |
| --- | --- |
| `CHATGPT_CODEX_REFRESH_TOKEN` | 存下的 refresh token（一个真实的环境变量胜过 `.env`，处处如此） |
| `OPENAI_CHATGPT_REFRESH_TOKEN` | 改名之前这个变量的名字 —— 当前那个未设时仍然读，从不会写 |
| `ZEROTH_ENV_FILE` | 迁移轮换写回的存储 |
| `ZEROTH_OPENAI_ISSUER` | 两次登录（以及刷新）所谈的 auth server —— 一个 fork 的，或者 `smoke.sh` Phase 119 的本地 stub；默认 `https://auth.openai.com` |

席位的速率限制与额度 —— `/usage` 显示的东西 —— 读自后端的根而不是 chat base：
`GET …/backend-api/wham/usage`，Codex 自己的读法，用同一个铸造的 bearer 和账号头
部（`docs/providers/usage.md`）。

## 文件

| 文件 | 里面有什么 |
| --- | --- |
| `src/llm/chatgpt.rs` | claim 解析，两个流程的 URL/请求体，设备流程的码/轮询/授权形状与判定，新鲜度规则（纯）；回环监听器、码请求与批准轮询、共用的码交换、缓存、轮换写回与 issuer 覆盖（边界） |
| `src/llm/auth.rs` | `request_auth` —— 每个出站调用都经由它解析的那一道接缝 |
| `src/llm/responses.rs` | Responses 线格式，双向（纯） |
| `src/llm/service_tier.rs` | 一条记录列出的速度层级、每个层级的命令名，以及它的命令所跑的切换（纯）—— `docs/providers/fast-mode.md` |
| `src/llm/openai.rs` | `request_url`/`request_payload`（线分支）、`drain_responses`、`pump_lines` |
| `src/llm/models.rs` | `{"models": …}` 信封和那三处记录嗅探 |
| `src/app/login.rs` | `SigninKind` 及其方法行、`SubscriptionChoice::kinds`、`KeyStep::SigninMethod` 选择及其按键、`DeviceLogin::copy_target` |
| `src/ui/login_view.rs` | 选择页和浏览器页的措辞 |
| `src/tui/config.rs` | `signin_kinds` —— 一个 scheme 提供哪些页面；`openai_issuer` —— 那个覆盖 |
| `src/tui/workers.rs` | `spawn_signin` —— 一行 auth scheme 加一次选择跑哪个流程 |
| `scripts/smoke/phases/119-chatgptdevice.sh` | 那个选择与设备流程，对着 auth server 的本地 stub 驱动 |
