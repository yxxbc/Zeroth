# 登录：订阅、API key，以及 GitHub Copilot

`/login` 以前只问一个问题 —— *你粘贴的是哪个 provider 的 key？* —— 因为它认识
的每个 provider 都收一枚粘贴的 key。GitHub Copilot 不收：它是一个你要**登录**的
**订阅**，走 GitHub 的设备流程，回来的是用户从没见过、更不会去敲的 token。所以
这个流程长出了一个先问那个问题的根步骤，以及它背后的另一半。

```
/login
  ├─ Sign in with an account ──► GitHub Copilot ──► device code ──► (approved)
  │                         ├──► ChatGPT Codex ──► browser | device code ──► (approved)   docs/providers/chatgpt.md
  │                         └──► Anthropic Console ──► browser ──► (approved)              docs/providers/claude.md
  ├─ Sign in with an API key ──► Agent Zero API / Venice / OpenRouter / … ──► paste the key
  └─ Sign in with a local model ──► LM Studio / Ollama / … ──► an address, tested       docs/providers/local-models.md
```

两半落到**同一个地方**：`~/.zeroth/.env` 里 provider 的 `api_key_env` 之下的一个
机密。这就是这次分叉便宜的全部原因 —— `/model`、能力探测、那些 ✓、启动时的 key
解析和下一次启动全都照旧工作，不需要教它们第二套机制。

## 页面

六步，一个 `KeyOnboarding`（`src/app/login.rs`）—— 第六步，即一个双向订阅摆在它
页面之前的那次登录方式选择，是 `docs/providers/chatgpt.md` 的 —— 全都共用
`/model` picker 的外框。**每个标题都穿着这些页面唯一的那身标题装束** —— 粗体，
浸在活动主题的横幅渐变里（`ui::header::page_title_line`、`docs/ui/theme.md`）——
于是这个流程的标题和其他每个页面的标题读成一家人，并且像横幅那样跟着 `/theme`。

### `KeyStep::Method` —— 根

```
────────────────────────────────────────────────────────────────────────────
  Select login method

  → Sign in with an account
    Sign in with an API key
    Sign in with a local model

  ↑↓ navigate  enter select  escape/ctrl+c cancel
────────────────────────────────────────────────────────────────────────────
```

用话说出的问题压在它的三个答案之上 —— `Select login method`，也就是登录方式选择
自己的措辞（`Select ChatGPT Codex login method:`，`docs/providers/chatgpt.md`）
，好让这个流程的两个问题读起来一样 —— 而行的 `→` 在外框的内缩处，像每个 picker一
样。**没有 `❯` 过滤**：三行是一个问题，不是一个要搜索的列表，所以打字和粘贴都什
么也不做，光标藏起来（方式选择的规则，`ui::cursor_visible`），也没有 `(1/3)`计数
器，因为那说不出行本身没说的事。用「Account」而不是「subscription」：它打开的那
个列表登入的是账号，而一个 Anthropic Console 账号按 token 计费，根本没有套餐。
Esc 关闭：这里是根。在一个短得放不下该页面的终端上，它的空隙和分隔线先于标题和行
让位（`model_view::compact_page`，标题坐在搜索行的位置上 ——
`docs/ui/view-flow.md`），而藏起来的光标的座位跟着标题（`ui::layout` 的
`title_row`，页面的第一个文本行），所以顶部分隔线一走，它就绝不会落到某个答案上
。

### `KeyStep::Subscription`

```
────────────────────────────────────────────────────────────────────────────
  ❯

  → GitHub Copilot · ✔ configured
    ChatGPT Codex · ◯ unconfigured
    (1/2)

  ↑↓ navigate  enter sign in  esc back
────────────────────────────────────────────────────────────────────────────
```

行的形状是 provider 列表的那个 —— 名字和它的**配置状态**，以同样的方式解析，因为
一个订阅的 token 住在同一个存储里；它的 `✔` 是绿的，像 `/model` picker 的 ✓，而
它旁边的一切都是暗的，因为记号才是眼睛要找的东西，而那个词只是一个朴素的事实。
别的什么都没有：两个列表以前都拖着一截暗色尾巴（这里是文件的 `description`，key
行上是 `[ENV_VAR]`），这让两者读起来像两种形状，还把名字彼此推远。`description`
仍然引导输入即搜索，现在则在 key 那一步介绍一个*要 key 的* provider
（`docs/providers/llm.md`）；它只是不该由列表的一行来承担。

也没有标题（2026-09-03 更新）。这个页面以前以一个青色标题开头，重复打开它的那一
行（当时是 `Use a subscription`），那说不出列表下面那行提示没说的事，还把每一行
往下推了两行。现在三个列表都直接开在它们的过滤器上；只有根用话说出它的问题。

### `KeyStep::Device`

```
────────────────────────────────────────────────────────────────────────
  Sign in to GitHub Copilot

  Visit https://github.com/login/device            (dim — the code is the
  and enter this one-time code                      bright thing on the page)

     ╭─────────────╮
     │  C363-262E  │
     ╰─────────────╯

  Waiting for approval… · expires in 14:11

  c copy code  esc cancel
────────────────────────────────────────────────────────────────────────
```

……而在码还在被请求的时候，恰好是这样 —— 不为还不存在的内容留着空行：

```
────────────────────────────────────────────────────────────────────────
  Sign in to GitHub Copilot

  Requesting a code…

  c copy code  esc cancel
────────────────────────────────────────────────────────────────────────
```

- **不启动浏览器。** URL 是用户自己去打开的文本。一个往外调 `xdg-open` 的 TUI 是
  在猜一个它未必在其上的桌面（一个 SSH session、一个容器、一台无头机器），而那种
  失败方式 —— 一个游荡的进程，或者一个在*错误的机器*上打开的浏览器 —— 比一个用户
  反正能在任何现代终端里点开的 URL 更糟。
- URL 和它下面那句话是**暗色**的。这个页面上该让眼睛落上去的唯一东西是框里的码；
  一个加了强调色的 URL 会和它抢，而 URL 是一条指令，不是一次选择。
- 框**按码的大小**定尺寸，所以一个发放更长码的 provider 也能得到一个贴合的框。
- 页面是**用恰好一个空行连接起来的块搭的，空块什么也不贡献**。四者中有两个只在
  GitHub 回答之后才存在 —— 在那之前没有 URL 可点名、没有码可装框 —— 而无论如何为
  它们留行，会在标题下面留一道空白带，而页面正在做它自己说它在做的那唯一一件事。
  所以等码的时候，就是两条分隔线之间的标题、空隙、状态、空隙、提示，一个都不带填
  充（`no_device_page_ever_stacks_two_blank_rows`，把列表页那条规则用在内容真的
  来来去去的页面上，见 `docs/ui/view-flow.md` *The frame hugs its content*）。
- `c` 经由 `/copy` 自己的剪贴板路径拷贝这个码（arboard 加 OSC 52 兜底，
  `docs/ui/copy.md`）；只有确认语不同，因为「Copied last message」在这里会是句谎
  话。
- 倒计时是一次**边界读时钟**，每次绘制注入（`App::set_device_remaining`），和
  状态行的已过时间完全一样 —— 纯核心从不读时钟。
- 它**每秒重绘一次，在自己的链上**（`DEVICE_TICK_INTERVAL`），刻意*不*用状态动画
  那条 32 ms 的链。一个 `mm:ss` 倒计时每秒才变一次；32 ms 的链把它重绘了三十倍
  那么频繁，而每一帧都带着光标 `Hide` 和一次重新落座，那在一个带光标拖尾动画的
  终端（kitty 之流）上会被渲染成页面上一层永久性的微光。用 `tmux pipe-pane` 在
  一个打开的 device 页面上量三秒：**90 个隐藏光标的转义 → 3 个**，2520 字节 →
  84。
- 这里**光标是藏起来的**（`ui::cursor_visible`）：这个页面是一个等待，不是一个
  输入框。它停在外框的第一个内容行（`DEVICE_CURSOR_ROW`），因为一个藏起来的光标
  仍然*落座*在某处，而拖尾是朝座位动的 —— 码框是页面上唯一不许有任何东西画在
  上面的地方，而它的行会随着页面从等待形状长成完整形状而移动。
- 一次失败**替换那行等待，用红色，页面仍然留着** —— 关掉页面会把解释一起带走。
  只有 Esc 能把它关掉，退回订阅列表。

### `KeyStep::Provider` / `KeyStep::Key`

除了每行现在带的 `✔ configured` / `◯ unconfigured` 状态、多一行点名该步骤各键的
暗色提示、Esc 现在退*回*方式根而不是直接关闭，以及 key 那一步自己的描述与 key页
面块（`docs/providers/llm.md`）之外，没有变化。列表恰好显示那些要粘贴 key 的
provider —— **Agent Zero API**、**Anthropic**、**OpenRouter**、**Ollama Cloud**
、**Ollama**、**Venice** —— 订阅 provider 被过滤掉，因为给一个根本没有 key 的流
程提供一个 key 输入框是死胡同。

## provider 文件写了什么

一个键决定一个 provider 落在哪个列表里：

```toml
[providers.github_copilot]
name = "GitHub Copilot"
auth = "github_copilot"                        # ← a sign-in, not a pasted key
description = "Sign in with your GitHub account"
api_key_env = "GITHUB_COPILOT_TOKEN"

[providers.github_copilot.extra_headers]
Editor-Version = "vscode/1.104.3"
Editor-Plugin-Version = "copilot-chat/0.26.7"
Copilot-Integration-Id = "vscode-chat"
User-Agent = "GitHubCopilotChat/0.26.7"
Openai-Intent = "conversation-panel"

[providers.github_copilot.kwargs]
api_base = "https://api.githubcopilot.com"
```

`AuthScheme`（`src/llm/config.rs`）默认是 `api_key`，而且**对不认识的值退回它**
（`#[serde(other)]`）：一个照更新版本写的 provider 文件必须降级，而不是让整个解
析失败。

## 两层 token

把它们混为一谈是每个第三方 Copilot 客户端里的经典 bug，所以
`src/llm/copilot.rs` 按名字把它们分开：

| | 它是什么 | 寿命 | 住在哪里 |
| --- | --- | --- | --- |
| **OAuth token**（`ghu_…`） | 设备流程铸造出来的东西 | 长期 | `.env`，和其他任何 provider 的机密一样 |
| **Copilot bearer**（`tid=…:mac`） | API 真正接受的东西 | 约 30 分钟 | 只在内存里，从不写入 |

`copilot::authorize` 把第一个换第二个，并按 OAuth token 缓存在一个进程全局 map
里—— 一个回合的各轮、`/model` 的抓取、每个 subagent 的线程要的都是同一个 bearer
，而逐请求重新交换会在一个 GitHub 限流的端点上每次花掉一次网络往返。

并发的缓存未命中共享一道交换闸门，并在等待之后重查缓存，于是并行的 subagent 不会
都去交换同一枚 OAuth token。一次新登录的 `copilot::forget` 不在这道闸门后面等 ——
TUI 循环调用它，而一次进行中的交换会持有闸门直到 GitHub 答完为止：它立刻丢掉缓存
的 bearer 并把 token 记在案，于是那次进行中的交换丢弃自己铸造的 bearer，而不是把
它缓存到登录已经替换掉的凭据之下。

**缓存的寿命取自 `refresh_in`，不是 `expires_at`。** 一个时钟走快的用户会得到一
个已经在过去的 `expires_at`，按它取就*每个请求*都重新交换一次。`refresh_in` 是一
个时长，对它免疫；再扣掉一分钟作为时钟漂移和飞行中延迟的余量。

### 接缝

`auth::request_auth(cfg)` 对任何配置回答 *(bearer, base 覆盖, headers)*：

- **`AuthScheme::ApiKey`** → 存下的 key，没有覆盖，而且**完全没有 I/O**。每个现有
  provider 的路径与以前逐字节相同。
- **`AuthScheme::GithubCopilot`** → 交换来的 bearer 和该账号自己的主机。一个没有
  存 token 的配置解析成一个空的 `RequestAuth`，而不是去调外部：`/model` picker
  会给从没登录过的 provider 建配置，在那里尝试交换会让抓取挂在一个只可能失败的请
  求上。

它以前住在这个模块里、返回一个 `(bearer, base)` 对，直到第二个订阅到来：一次
ChatGPT 请求需要一个**头部**，携带它是代表哪个账号发的，那是一对儿表达不了的，于
是接缝移到 `src/llm/auth.rs` 并加宽了一个字段（`docs/providers/chatgpt.md`）。
Copilot 自己那一支没变 —— 它的头部列表就是空的。

`openai::stream_chat` 和 `models::fetch_models` 都经过它。base 覆盖很重要：一个
Business/Enterprise 席位由 `api.business.githubcopilot.com` 服务，那是 token 响
应在 `endpoints.api` 里点名的，而 provider 文件无从知道。

## 请求身份

静态的那一半骑在 `providers.toml` 的 `extra_headers` 上。有三个头部是**逐请求**
的，住在 `openai::copilot_request_headers` 里：

- **`X-Initiator`** 是*与计费相关的*，不是装饰：GitHub 对用户发起的一轮按高级请
  求收费，对 agent 自己的 tool 往返不收。规则是最后一条消息的角色 —— 由用户自己
  的文本收尾的一轮是他的；收尾在一条 tool 结果上（或收尾在 assistant 自己的文本
  上，一次延续就是这样）的是 agent 的。把每一轮都标成 `user`，会为一个问题向用户
  收好几次费。
- **`Copilot-Vision-Request: true`**，只要一轮带着图片部件，否则 API 会回答
  `400 missing required Copilot-Vision-Request header for vision requests`。
- **`X-Request-Id`** —— 一个请求出毛病时 GitHub 支持会要的东西。按 session 的缓存
  键同时兼任它。

**载荷**以同样的方式修剪：没有 `prompt_cache_key`。Copilot 的代理对一个它不认识
的请求形状回答 `model_not_supported` —— 一个怪罪*模型*的错误 —— 而它的缓存在服务
端，所以亲缘键在那里买不到值得冒这个险的东西。同一个值改骑 `X-Request-Id`。

`Editor-Version` 和 `Copilot-Integration-Id` 是错起来**很大声**的那两个：少了第
一个是 `400 missing Editor-Version header for IDE auth`，而一个服务器不认识的
integration id 是 `400 The requested model is not supported` —— 读起来像模型的问
题，其实不是。`vscode-chat` 这个 id 与设备流程所用的 VS Code OAuth client id 配
对；两个在服务端是一起校验的。

## 推理、视觉与上下文窗口

Copilot 的 `/models` 发布比这里任何其他 provider 都丰富的元数据，`llm::models`
的三处逐记录嗅探各长了一个分支 —— 与 OpenRouter 和 Venice 早就共用的同一种形状嗅
探，所以一个朴素的 OpenAI 风格列表对它们三个都不受影响。

| | 字段 | 注记 |
| --- | --- | --- |
| context | `capabilities.limits.max_prompt_tokens` | 退回 `max_context_window_tokens` |
| vision | `capabilities.supports.vision` | |
| reasoning | `capabilities.supports.reasoning_effort` | 一个**数组** —— 阶梯本身 |

有三件事值得直说：

- **prompt 上限就是窗口。** Copilot 把两者分开 —— gpt-4o 的窗口是 128000 个
  token，但只接受 63997 个 token 的 prompt —— 而页脚量表和自动压缩的存在是为了让
  一个回合待在 API *强制*的那个限内。
- **`supports` 从不发 `false`。** 一个不支持的能力就是干脆不在，所以一旦对象存在，
  答案就是「不在」，而不是「未知」。
- **`reasoning_effort` 就是 Ctrl+T 的阶梯，直接从 wire 上取。** 其他每个 provider
  拿到的都是硬编码的 low/medium/high 猜法；这里模型自己点名它接受什么
  （`["minimal","low","medium","high"]`、`["low","medium","high","max"]`、……），
  于是循环提供的恰好是 API 会接受的。一个只宣传思考*预算*的模型（Anthropic 的
  形状）是一个没有阶梯的开/关推理器。

在 wire 上，模式是一个**顶层的 `reasoning_effort` 字符串**，不是这里其他每个
provider 接受的 `reasoning` 对象 —— 而且是*替代*它，绝不并排：那个对象在这个端点
上是个未知字段，而 Copilot 的代理对未知请求形状回答同样误导人的
`model_not_supported`400。`Off` 通过省略这个参数来表达，Copilot 自己的 opt-in 推
理反正也是这么做的。

### 列表省掉了什么

`entry_of` 跳过两类记录，因为提供任何一类都会把某个每回合都失败的模型放进 picker
：

- **embeddings / completion** 模型（`capabilities.type`），它们根本没有聊天表面；
- 只由 `/responses` 服务的模型（`supported_endpoints`）—— 当前 GPT-5 推理家族的大
  部分 —— 它们对一次 `/chat/completions` 请求回答 400。

一个缺席的端点列表意味着 chat completions（旧的 GPT 记录就省略它），而一条记录只
有在它的 `capabilities` 对象有 Copilot 自己的形状时才算 Copilot 的 —— 一个
`type`字符串，或者一个 `supports` 或 `limits` 对象（
`models::copilot_capabilities`）——所以其他每个 provider 的列表原样通过。网关的那
种才是要紧的情形：OmniRoute 的记录带一个扁平布尔值的 `capabilities` 对象，旁边还
有一个 `supported_endpoints`，点的是**上游**的 API（对它发现的一个 Codex 模型就
是 `["responses"]`），而它照样用 chat completions 服务这些模型中的每一个。被读成
Copilot 的，那个列表会把每一个这样的模型都从 picker 上拿掉。picker 合理地会比 VS
Code 的模型 picker 显示一个更短的列表。

## 它不工作时

Copilot 失败的方式比一枚粘贴的 key 多，而从外面看它们大多很像 —— 所以有两样东西
纯粹是为了把它们分辨开而存在的。

**批准不是访问权。** GitHub 的设备流程认证的是*用户*；这个用户能不能调 Copilot，
只有token 交换才能定。一个没有订阅的账号 —— 或者一个组织 SSO 还没授权的账号 ——
登录得完美，然后在第一个请求上失败。所以 worker 把交换**作为登录的一部分**跑，趁
那个能解释它的页面还在，而不是报一句 `Signed in ✓`，再让 `/model` 一分钟之后神秘
地失败。

**一次失败会说要做什么，并引述 GitHub。** `copilot::exchange_advice` 把交换的回
答变成一句话，而那个**顺序**就是设计：那里的一个 403 覆盖十几种不同的状态，所以
只要 GitHub自己给了说明，它的话就优先，只有它什么都不说的地方，我们才从状态码推
断。

| 回答里带着 | 用户被告知什么 |
| --- | --- |
| 以 `API rate limit exceeded` 开头的 `message` | GitHub 的 REST 速率限制用完了 —— **不是**权限问题，尽管它是作为一个 403 来的 |
| `error_details.notification_id: subscription_ended` | 去 `github.com/settings/copilot` 续订 |
| `…: enterprise_managed_user_account` | 一个 EMU 账号：管理员必须授予席位 |
| `…: go_http_client` / `programmatic_token_generation` | 客户端被拒了 —— token 请求得太频繁，或来自一个 GitHub 不认识的客户端 |
| 任何 `error_details.message`（加 `url`） | **GitHub 自己的话和链接**，原样 |
| `can_signup_for_limited: true` | 这个账号有资格拿 Copilot Free 但还没接受 —— 去 `github.com/settings/copilot` 启用它 |
| 一个点名 SAML/SSO 的 403 | 去 `github.com/settings/tokens` 为那个组织授权这枚 token |
| 一个光秃秃的 403 | 没有可用的 Copilot 权限 |
| `401` | OAuth token 死了 —— 重新跑 `/login` |
| `404` | token 是由错误的 OAuth app 铸造的 —— **不是**订阅问题，也是这个流程里最常被误诊的失败 |

推断出来的那些句子后面跟着 `(GitHub said: …)` —— 它自己的 `message`，或者当回答
不是JSON 时的原始正文，Copilot 的 403 有时就不是（一个光秃秃的 `forbidden`，或者
`403 Unauthorized: not authorized to use this Copilot feature`）。为那些丢掉证据
，会恰好在建议最不可能贴切的地方丢掉它。

**而一次成功会点名席位。** `ExchangedToken::plan_note` 读 `sku` ——
`Copilot Free`、`Copilot for Students`、`Copilot Business`、`Copilot Enterprise`
—— 加上一个免费席位剩余的 `limited_user_quotas.chat`，于是确认读作
`Signed in to GitHub Copilot (Copilot Free — 42 chat requests left this month)`
。套餐是一次登录否则会留下的那个悬而未决的问题，而一个计量席位的额度，说在前面比
在回合中途撞上一个 402 更好。一个缺席或无法识别的 `sku` 什么都不说 —— 它在
GitHub 自己的校验器里是可选的，所以它的缺席绝不能变成一句关于错误套餐的自信断言
。

**Copilot Free 能用。** 它是一个 *SKU*，不是一条单独的认证路径：同一个设备流程、
同一次交换服务它，而 `sku` 回来是 `free_limited_copilot`。不同的是模型集合（服务
端过滤，且在持续变动 —— picker 去发现它而不是硬编码）和那个计量器。

`/model` picker 会显示它。它的计数后缀把一个失败的 provider 收缩成
`GitHub Copilot unavailable`，那只点名 provider，别的什么也没说；原因现在渲染在
**列表下面**（`model_error_lines`），红色并换行，夹在 `MODEL_ERROR_MAX_ROWS` 以
内、切口有标记 —— 否则一个用 HTML 页面作答的 provider 会把模型列表挤出框外。

## 边界处的流程

`spawn_copilot_login`（`src/tui/workers.rs`）是循环的第四个 worker，也是唯一一个
跑**几分钟**而不是几毫秒的：它轮询到用户批准、码过期，或它的 `CancelToken` 触发
。它在自己那条 `select!` 分支上报告两条消息 —— 先是码，然后是判定 —— 而
`tui::login` 处理它们：

- 一个**码**上页面并启动倒计时；
- 一次**成功**通过粘贴 key 用的同一个 `.env` 写入器持久化 OAuth token，关闭流程，
  并弹出 toast `Signed in to GitHub Copilot — run /model to use it`；
- 一次**失败**让页面留着，穿着那个原因。

Esc、Ctrl+C 和退出都会回收这个 worker。一次发现自己的页面已不在的轮询什么都不投
递—— 没有东西可以挂 token，而猜一个 provider 会把 key 挂错。

轮询的节奏是 GitHub 自己的 `interval`，以 200 ms 的小睡来睡，好让一次 Esc 被尊重
得和回合中途差不多快，而不是在一个五秒块的末尾。`slow_down` 会拉长它（GitHub 点
名了新下限就用它，否则用 RFC 的五秒惩罚）；`expired_token` 和 `access_denied` 是
终局。过期时间两种拼法都匹配 —— GitHub 自己的文档在这上面自相矛盾，只匹配一种会
把一次例行的过期变成「意外响应」。

每次轮询都回答 **HTTP 200**，错也不错：判定在正文的 `error` 字段里，所以只查状态
码会把一次拒绝读成成功。这也是一个读不了的正文是终局而不是「继续等」的原因 —— 后
者会循环到码过期。

## 环境

没有新的东西。Copilot provider 是一个普通的 `providers.toml` 条目，所以
`ZEROTH_PROVIDER=github_copilot` 选中它，`ZEROTH_MODEL` 挑模型，而真实环
境里的 `GITHUB_COPILOT_TOKEN` 胜过 `.env` 存储，和每个其他 provider 的 key 完全
一样 ——这也是一台已经有 Copilot 登录的机器（一台 CI 机器、一份 dotfiles 导出）可
以完全跳过`/login` 的方式。

`ZEROTH_GITHUB_API_BASE` 把这次登录所做的两次 GitHub REST API 调用 —— token
交换，以及 `/usage` 页面的额度读取（`copilot_internal/user`，
`docs/providers/usage.md`）——指向另一台主机：默认 `https://api.github.com`，套件
里则是 `smoke.sh` Phase 145 的本地 stub。设备流程本身留在 `github.com` 上。
