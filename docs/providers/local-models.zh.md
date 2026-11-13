# 本地模型 —— `/login` → **Sign in with a local model**

`/login` 有三个入口。**Sign in with an account** 登录一个账号，**Sign in with
an API key** 粘贴某个 provider 的 key，而 **Sign in with a local model** 连接一
个用户自己跑的模型服务器：Ollama、LM Studio，或者用户在自己的 `providers.toml`
里描述的无 key 服务器。本地服务器不需要 key。它需要的是一个**地址**，而要地址
的那一页会在**保存任何东西之前先测一遍连接**。

```
/login → Sign in with a local model → LM Studio → Enter        (the address in force)
                                       → type gpu-box:1234, Enter
```

这一页测试的方式，和 `/model` 之后的列模型完全一样
（`llm::models::fetch_models`）。测试通过就结束流程，并说出找到了什么：
`Connected to LM Studio — 3 models · run /model to pick one`。失败则留在页面上，
红字写在输入框下方，直到地址被编辑为止。要 token 的服务器就在同一页上向你要。

## provider 属于哪一张列表

三张列表按 `auth` 把 provider 表分开 —— `auth` 是 provider 文件里说明某个
provider 如何认证的那个键（`docs/providers/llm.md`）：

| `auth` | 列表 | 该行索要什么 |
| --- | --- | --- |
| `github_copilot`、`chatgpt_codex`、`anthropic_console`、`cline_account` | Sign in with an account | 一次登录（`docs/providers/copilot.md` 及其同族） |
| `api_key`（默认） | Sign in with an API key | 一把 key，掩码显示 |
| `optional_key` | Sign in with a local model | 一个地址，测试过才收 |

`optional_key` 原本的含义就是「一个不需要凭据的服务器，尽管存下的凭据仍会作为
bearer 骑上去」（`docs/providers/ollama.md`）。本地服务器正好是这个意思，所以
本地那张列表恰恰就是这些 provider。在此之前，本地 Ollama 坐在 API key 列表里
当一个「host 输入框」，敲进去什么就存什么。现在它住在本地列表上，而那个 host
输入框变成了下面的服务器页面。**Ollama Cloud** 仍留在 API key 之下，因为它是
靠粘贴 key 到达的托管服务。

## 列表

和另外两张用的是同一个列表页：一个 `❯` 过滤器，行按注入顺序排列（按 id 字母
序，所以 `LM Studio` 在 `Ollama` 之前），一个 `(n/total)` 计数器，
`↑↓ navigate  enter connect  esc back`。每一行写着 `✔
configured` 或 `◯ unconfigured`。

**Configured 的意思是「已经指向它」**（`tui::config::is_configured`，
`docs/providers/ollama.md` 为 Ollama 立下的那条规矩）：provider 的地址变量解析
得出（`OLLAMA_HOST`、`LMSTUDIO_HOST`），或者 key 解析得出，或者
`ZEROTH_PROVIDER` 点了它的名。一个由用户自己的配置块就地固定、没有地址变量的
服务器，仅凭那个块就算 configured（`Provider::configured_by_definition`、
`docs/providers/user-providers.md`）。configured 同时也是 `/model` 会去哪些
provider 拉取列表的依据。一个没被指向却被算成 configured 的服务器，会让每一个
并不跑它的用户，在每次打开 `/model` 时都看到红色的
`LM Studio unavailable`。

## 服务器页面

```
────────────────────────────────────────────────
  Enter your LM Studio server URL                    (title)

  Open models running in LM Studio on your own       (the file's description)
  machine — nothing leaves it, and no key is …
  Install it from https://lmstudio.ai/download       (api_key_url, an OSC 8 link)

  ❯ http://127.0.0.1:1234                            (dim: the address in force)

  Nothing is answering at 127.0.0.1:1234 — is        (red, after a failed test)
  LM Studio's server running?

  Enter to test and save · Esc to go back
────────────────────────────────────────────────
```

- **输入框照敲进来的样子显示地址。** 瞎敲的 URL 就是敲错的 URL，所以什么都不
  掩码。空着时，它显示**生效中的地址**，暗色：变量有值时显示变量的值（按存储
  的样子），否则显示文件自己的服务器。对 LM Studio 来说那是
  `http://127.0.0.1:1234` —— 和 LM Studio 自己的服务器页面显示的方式一样，不带
  配置会补回去的 `/v1`（`llm::local::server_address`）。空着按 Enter 就是测那个
  地址，所以复查一个能用的服务器只需一次按键。这也正是空着按 Enter 不再把远程
  的 `OLLAMA_HOST` 重置成默认值的原因：旧 host 输入框的默认值是文件里的地址，
  不管之前存的是什么。
- **Enter 就是测试。** `Action::TestLocalServer { provider, test }` 携带一个
  `ServerTest { host, token }`。页面在输入框下方显示 `Testing the connection…`，
  提示行显示 `Esc to stop the test`。等待期间输入框不接受按键，光标也藏起来。
  边界用被测地址顶替那个变量，构造出 provider 的配置
  （`ModelSession::local_test_config`），用每个请求都用的同一套语法把它补全，
  然后在 `/login` 的 worker 上跑 `fetch_models`
  （`tui::workers::spawn_server_test`）。完全不是 URL 的地址立刻就有答复，不发
  任何请求，并以服务器自己的地址作为示例（`llm::local::server_label`、
  `unusable_address`）。
- **结论**是纯的（`llm::local::verdict`），措辞是为这个页面写的：

  | 列模型的结果…… | 页面说 |
  | --- | --- |
  | 答了 N 个模型 | *（通过：保存、关闭、toast `Connected to {name} — N models …`）* |
  | 连不上：连接被拒绝 | `Nothing is answering at {host} — is {name}'s server running?` |
  | 连不上：名字解析不出来 | `Could not find {host} — check the address.` |
  | 连不上：证书不受信任 | `{host}'s certificate is not trusted — it may be self-signed. Point ZEROTH_CA_FILE at its CA certificate, then test again.` |
  | 连不上：对一个纯 http 服务器发了 https 请求 | `{host} does not speak https — try http:// instead.` |
  | 超时 | `{host} did not answer in time — check the address, and that nothing blocks the port.` |
  | 答了 401 或 403 | *（要 token：见下一节）* |
  | 答了 404，或者一个不是列表的响应体（一个 200，但里面没有模型列表 —— LM Studio 对任何它不服务的路径都这么答） | `{host} answered, but not as {name} — check the address and port.` |
  | 答了其他任何状态码 | `{name} at {host} answered HTTP {status}: {body}` |

  `{host}` 是 `host[:port]`，只有 URL 里写了端口时才显示端口。一个没什么可聊
  的服务器仍然算通过。地址是对的；toast 说
  `Connected to {name}, but it has no models to chat with yet`。
- **页面已经翻篇的那次测试，它的答复会被丢掉。** 页面记着在飞的是哪一次测试，
  而 `App::finish_server_test` 只在答复正是那一次测试时才采纳结论。Esc 停掉一
  次测试（`Action::CancelServerTest` 取消 worker，worker 此后什么都不发）。哪怕
  一个结论漏过了取消，它也落不到下一次测试上，也关不掉用户已经离开的页面。
- **Esc** 停掉正在跑的测试，否则退回列表，刚才离开的那一行仍然高亮。**Ctrl+C**
  从任何位置关闭整个流程，正在跑的测试也随之停掉。
- **一编辑就清掉失败。** 红色的理由是冲着被测过的那个地址说的。它之后的第一次
  按键就把它移除，而不是留着它去描述一个已经不在输入框里的地址。

### token 这一步

LM Studio 0.4+ 可以设成 *Require Authentication*（一个以
`Authorization: Bearer …` 发出的 API token），而躲在反向代理后面的 Ollama 也可
能想要一个 bearer。一次测试答了 **401 或 403**，意味着服务器在那儿，只是拒绝了
一个未认证的请求。那不算失败，所以页面问：

```
  Enter your LM Studio API token

  LM Studio at gpu-box:1234 wants an API token.

  ❯ ••••••••
```

token 像 key 一样掩码。Enter 在这一步**用服务器索要它的那个地址**去测。服务器
再次拒绝的 token 会说
`LM Studio refused that token — check it, then test again.`，并留在那里等修。
Esc 回到地址。通过则两个都保存：地址存在 provider 的 `api_base_env` 之下，
token 存在它的 `api_key_env` 之下。已经存下的 token（比如导出了
`LM_API_TOKEN`）会像跟随每个请求那样跟随第一次测试，所以接受它的服务器从不会
开口问。

### 保存了什么，保存在哪儿

存进粘贴的 key 落地的同一个 `.env` 存储（`docs/providers/llm.md`），所以
`/model`、那些 ✓ 标记和下一次启动读的都是它，不需要第二套机制，而且只在测试通
过之后才存：

| 服务器 | 地址 | token |
| --- | --- | --- |
| Ollama | `OLLAMA_HOST`（Ollama 自己的变量，按它自己的语法） | `OLLAMA_HOST_API_KEY` |
| LM Studio | `LMSTUDIO_HOST` | `LM_API_TOKEN`（LM Studio 自己对它的叫法） |
| 用户配置块固定的服务器 | *（不存：它的地址就是文件里的）* | 它的 `api_key_env` |

地址**按敲进来的样子**保存，和旧 host 输入框一样；补全由配置来做。运行中的
session 里没有任何东西被重建。下一次构建 backend（选一次 `/model`、改一次
`/settings`）会解析这个新地址，就像解析刚粘贴的 key 一样。

**shell 里导出的变量压过存储。** 解析先读进程环境
（`tui::config::resolve_env`），而一个应用没法 unset 自己的环境，所以当
`OLLAMA_HOST` 已导出（Ollama 用户中很常见）时，在这里敲的地址会通过测试、会被
保存，却永远不会被用上。输入框把这个导出的值当作生效中的地址显示出来，而被它压
过的那次保存得到的是
`Saved — unset OLLAMA_HOST in your environment, which overrides it`（红色，
`ServerSave::overridden_notice`），而不是确认。判断由
`ModelSession::exported_overrides` 做：host 按它所指的那个服务器比较，所以导出
的 `localhost:1234` 与保存的 `http://localhost:1234/v1` 是同一个地址，什么都
不说；token 则按敲进来的样子比较。保存照样发生，所以变量一旦 unset 它就生效。

对一个测试不过的地址，这里刻意**没有**「照样保存」。`/login` 只保存它亲眼见过
能用的东西。一个此刻正躺着的服务器，靠手工设置它的变量来指向
（`export LMSTUDIO_HOST=…`，或者 `~/.zeroth/.env` 里的一行）。

## 作为 provider 的 LM Studio

```toml
[providers.lmstudio]
name = "LM Studio"
auth = "optional_key"
api_key_env = "LM_API_TOKEN"
api_base_env = "LMSTUDIO_HOST"
api_model_base = "../api/v1"

[providers.lmstudio.kwargs]
api_base = "http://127.0.0.1:1234/v1"
```

- **线是 chat completions**，走 LM Studio 那个 OpenAI 兼容的 `/v1`。tool call、
  流式传输和 usage 帧都是它的原生形状：`stream_options.include_usage` 自
  LM Studio 0.3.18 起被遵守。它的 reasoning 以 `reasoning` /
  `reasoning_content` delta 抵达，`ThinkingSplitter` 本来就读这个，所以推理模型
  的思考和其他任何 provider 一样流进 `● Thinking…` 块。
- **列表用的是 LM Studio 的原生列表**，`GET /api/v1/models`（LM Studio 0.4+）。
  `/v1/models` 只给出 id，别的什么都没有。原生记录带着 picker 需要的东西，和其
  他每个 provider 一样逐条读取（`llm::models`、`is_lmstudio_record`：一个字符串
  `key` 旁边配一个 `loaded_instances` 数组）：
  - **id 是 `key`**（`qwen/qwen3-8b`），`display_name` 是显示名；
  - **只提供语言模型。** `type: "embedding"`（每次安装都会带一个）没有聊天界
    面，会让每个回合都失败；
  - **窗口是加载时的那个。** LM Studio 服务一个模型时用的上下文长度是它**被加
    载时**的上下文长度（`loaded_instances[].config.
    context_length`，常见为 4096），而不是它最多能吃的长度；加载了多个实例时，
    取最小的那个，因为一个请求可能落到其中任何一个上。尚未加载的模型由第一个
    请求按一个列表不会报告的已存设置加载，所以它的窗口是**未知的**：量表隐
    藏，自动压缩保持关闭 —— 而不是一直声称它的 `max_context_length` 直到服
    务器拒绝；
  - **图像输入**是 `capabilities.vision`，一个明确的 bool。
- **`api_model_base` 是相对的**，`../api/v1`，以 chat base 为基准解析，方式和
  浏览器解析一个链接一样（`config::resolve_relative_base`）。服务器被指到哪
  儿，它就跟到哪儿：`gpu-box:1234` 从 `http://gpu-box:1234/api/v1` 列模型，
  反向代理的 `https://proxy/lmstudio/v1` 则从
  `https://proxy/lmstudio/api/v1` 列。任何 provider 都可以写一个（`./x`、
  `../x`、`/x`）。一个 host 被覆盖过的 provider 上写的绝对 model base，仍然
  让位给 chat base，和它一贯的做法一样。
- **较老的 LM Studio**（0.4 之前）没有 `/api/v1`。它回答自己的原生列表的方式，
  和回答每一个它不服务的路径的方式一样：**200** 加上
  `{"error":"Unexpected endpoint or method. (GET /api/v1/models)"}`
  （「Returning 200 anyway」，它的日志这么说），从不给 404。
  这样的响应体不携带模型列表，而这不是一个空列表（`models::is_listing`、
  `not_a_listing`）：`fetch_models` 会像对待 404 一样，回退到 chat base 的
  `/models` —— 只要一个 chat-completions provider 的 model base 与它的 chat
  base 不同。这样列出来的行只带 id：没有窗口，图像输入未知，而且任何 embedding
  模型都会出现在里面。把它当成列表来读，那个错误对象就成了一个没有模型的服务器：
  测试通过、地址被保存，而 `/model` 什么都不显示。
- **什么路径都答的服务器，不是它自称的那一个。** 同一条规矩也适用于 Ollama 的
  `/api/tags`（`ollama::tags_records`）：把 LM Studio 的端口敲进 Ollama 的页
  面，会得到 200 和那个错误对象，并被拒为
  `127.0.0.1:1234 answered, but not as Ollama — check the address
  and port.`。以前它会当成一个没有模型的 Ollama 通过测试，并替掉保存着的
  `OLLAMA_HOST`。空列表（`{"models":[]}`，一个什么都没拉的 Ollama）仍然是一个
  列表。
- **比加载的上下文更长的 prompt，会告诉你该怎么加大它。** LM Studio 在流里就
  拒绝它，用的是一帧只含 JSON 字符串的帧
  （`data: "The number of tokens to keep from the initial prompt is
  greater than the context length."`），然后结束流。chat drain 会跳过一帧它读
  不成 delta 的东西，于是回合结束时既没有回复也没有错误。
  `openai::parse_sse_error` 现在会把一帧裸字符串、以及一个用字符串点名自己错误
  的 `{"error": "…"}`，都读成服务器的失败。`explain` 于是用
  `qwen/qwen3-8b is loaded with a context window too
  small for this conversation — raise its context length in LM Studio, or
  load it with lms load qwen/qwen3-8b --context-length 32768, then try
  again.` 来回应 LM Studio 的原话（`local::refusal_advice`：`greater than the
  context length`、`context
  length of only`）。带内失败没有 HTTP 状态码，所以它显示时不带 `HTTP 0:`
  前缀。
- **服务器在 session 中途要的 token，归 `/login` 管。** 一个无 key 服务器在
  `/login` 存下地址之后又被设成需要 token（或者它的 token 变了），会用 401/403
  拒绝，并答以 `LM Studio at 127.0.0.1:1234 wants
  an API token — run /login to enter it.`
- **停掉的服务器是建议，不是等待。** 连本机上一个无 key 服务器的连接被拒绝，是
  服务器没在跑，而不是网络断了。`OpenAiClient::explain` 用测试自己的那句话回应
  它（`local::refused_advice`）：`Nothing is answering at
  127.0.0.1:1234 — is LM Studio's server running?`，并像停掉的 Ollama 早已做
  的那样，在有界的秒级重试上浮出来。跑在另一台机器上的 LM Studio 仍保留离线等
  待（`docs/providers/offline.md`）。
- **没有 cache breakpoint。** `qwen/qwen3-8b` 的拼写方式正是 OpenRouter 用来
  命名显式缓存模型的写法，所以 `cache::needs_cache_breakpoints` 对它说是。载荷
  构造器只给接受 key 的 provider 打标记，所以无 key 服务器的消息保持纯字符串形
  式（`docs/providers/prompt-caching.md`）。

### 地址语法

`LMSTUDIO_HOST` 是按 LM Studio 展示它服务器的方式敲的，所以配置把一个裸 host
读成一台**服务器**（`llm::local::base_from_host`，作用于每一个设置了
`api_base_env` 的非 Ollama provider）：

| 敲进去的 | chat base |
| --- | --- |
| *（空）* | `http://127.0.0.1:1234/v1`（文件自己的） |
| `localhost:1234` | `http://localhost:1234/v1`（文件的 scheme 和路径） |
| `gpu-box` | `http://gpu-box:1234/v1`（还有它的端口） |
| `https://lms.example.com` | `https://lms.example.com/v1`（敲了 scheme 就保留它自己的默认端口） |
| `0.0.0.0:1234` | `http://127.0.0.1:1234/v1`（绑定地址不是客户端能到的地址） |
| `::1` / `[::1]:1234` | `http://[::1]:1234/v1` |
| `http://gpu-box:1234/v1` | 照敲的（有路径时路径本身就是 base） |
| `https://proxy.example/openai` | 照敲的 |

带路径的值按敲进来的样子取用，和以前完全一样。所以变量里放完整的 base（旧的
`/login` host 输入框保存的就是块里的 base）解析结果不变。Ollama 保留自己的语
法 `ollama::host_url`，也就是把 Ollama 的 `envconfig.Host()` 移植过来
（`docs/providers/ollama.md`）。

### Thinking

LM Studio 有自己的 reasoning 开关（`capabilities.reasoning`，例如
`allowed_options: ["off", "on"]`）。chat 线上的 `reasoning` 参数不是那个开关。
LM Studio 只在它的 `/v1/responses` 上记录了 `reasoning.effort`，所以一个
Ctrl+T 模式只会是一个页脚，声称一个服务器根本不理会的 effort。因此 LM Studio
的模型**没有** Ctrl+T 循环：它按 LM Studio 配置的方式思考，而它的思考照样流式
输出。

## 自签名证书

一台走 **https** 到达的本地服务器，常常拿着一份没有公共 CA 签过的证书：家庭实
验室里的反向代理、藏在自带内部 CA 的 Caddy 后面的 LM Studio。连接测试会在 TLS
握手上让这样的服务器失败，页面会说出来，并说清怎么绕过：

> `gpu-box:8443's certificate is not trusted — it may be self-signed. Point
> ZEROTH_CA_FILE at its CA certificate, then test again.`

`ZEROTH_CA_FILE`（或 `SSL_CERT_FILE`）给这个进程发出的每个请求追加信任根
（`llm::http_client`）。它在第一个 client 被构造时读取，所以要在启动前设好。

**将来：问一下要不要接受这张证书。** 页面可以给出这个选择，而不是只报告失败。
这里把它写下来而不是建出来，好让它的形状在任何代码存在之前就议定：

1. **展示被信任的是什么。** 在证书不受信任的失败上，页面去取那张叶子证书（一
   次关掉校验的 TLS 握手，读对端证书、什么都不发），并显示它的 subject、
   issuer、有效期和 **SHA-256 指纹**。然后它问
   `Trust this certificate for gpu-box:8443? ❯ 1. Trust it  2. Cancel`，和权
   限提示一样的编号选择形状。
2. **钉住，而不是关掉。** 信任会把 `{host:port → fingerprint}` 记进配置主目录
   下一个按用户的文件（`trusted_certs.json`，`trust.json` 的姿态：读—改—写，
   格式坏掉的文件以红色 toast 失败并关闭）。此后发往那个 host:port 的请求，只
   对着那一个指纹校验整条链，别的什么都不信，靠一个只用于被钉住 host 的自定义
   rustls `ServerCertVerifier`。校验从不被关闭。证书变了（重签，或者中间人），
   钉住就失败，于是再问一次，并同时显示两个指纹。
3. **范围。** 只限在 **Sign in with a local model** 下输入的地址，永远不包括托
   管 provider：一个云 API 递来不受信任的证书，是该拒绝的攻击，不是该回答的提
   示。
4. **为什么还没建。** 它需要第二条 TLS 校验路径进入那一个共享的 HTTP client
   （`llm::http_client` 按超时缓存 client），一次读取自己没校验过的证书的握手，
   以及一个新文件 —— 整个 session 都信任它。每一样都是与安全相关的代码，都该
   有自己的评审。`ZEROTH_CA_FILE` 在此期间覆盖了这个需求，而失败信息会点出它
   的名字。

## 升级

- **你自己写的一个 `[providers.lmstudio]` 块**（照 0.15.0 的
  `docs/providers/user-providers.md` 例子写的）现在会**补丁**出厂的 LM Studio，
  而不再新增一个 provider —— 一个点了出厂 id 的块只改它设的那些键。出厂块的
  `api_base_env` 让它靠「被指向」而不是靠那个块算 configured，所以连它一次：
  `/login` → Sign in with a local model → LM Studio → Enter 会测试你的块点的
  那个地址并保存下来。或者把块删掉，因为出厂的那个就是它描述的东西。
- **以前连过的本地 Ollama** 继续能用：它的 `OLLAMA_HOST` 照旧被读取。只是现在
  它出现在 Sign in with a local model 之下，而不是 Sign in with an API key 之
  下。

## 代码在哪里

- **`llm::local`**（纯）：`base_from_host` / `server_address`（地址语法与输入框
  给出的建议）、`server_label`、`Verdict` + `verdict`（页面的句子）、
  `unusable_address`（在任何请求之前就答复的笔误，以服务器自己的地址作示例）、
  `refused_advice`（session 中途停掉的服务器，用测试的原话）以及
  `refusal_advice`（比加载的上下文更长的 prompt、服务器索要的 token）。
- **`llm::openai`**：`explain` 把一个无 key 的回环拒绝和一个无 key 服务器的各
  种拒绝变成那些建议，`parse_sse_error` 把一帧裸字符串和一个字符串值的 `error`
  读成服务器的失败，而 `chat_request` 让无 key provider 的消息保持不打标记。
- **`llm::config`**（纯）：出厂的 `lmstudio` 块；一个非 Ollama 的
  `api_base_env` 值由 `base_from_host` 补全；相对的 `api_model_base`
  （`is_relative_base`、`resolve_relative_base`），在用户的块里也接受。
- **`llm::models`**：LM Studio 的原生记录（`is_lmstudio_record`、
  `lmstudio_loaded_context` —— 最小的已加载窗口，未加载时没有 —— 类型过滤器、
  `capabilities.vision`、无 reasoning）；一个没有模型列表的响应体
  与一个空列表被区分开来（`is_listing`、`not_a_listing`，`ollama::tags_records`
  也共用它们）；`fetch_models` 在 404 或这样的响应体上回退到 chat base 的列表。
- **`app::login`**（纯）：`LoginMethod::Local`、`KeyStep::Local` /
  `KeyStep::Server`、`LocalChoice`、`ServerPage`、`ServerTest`、`TokenAsk`、
  `ServerSave`（+ `confirmation`、`overridden_notice`）、
  `App::finish_server_test`、`KeyOnboarding::server_takes_typing`。旧的
  `KeyKind::Host` 字段随它服务的那些 host 配置行一起消失了。
- **`ui::login_view`**：本地列表（共享的列表页）与 `server_page_lines`。
  `ui::layout` 把光标放在敲进来的地址或 token 的掩码之后，并在测试进行或地址被
  固定时把它藏起来。
- **`tui::config::local_choices`** / `is_configured`，以及
  `provider_choices` 里的划分（只含 `auth == api_key`）；
  `ModelSession::local_test_config` / `exported_overrides`；
  `tui::workers::spawn_server_test`（`DeviceEvent::Tested`）；`tui::login` 的
  `test_local_server` / `finish_server_test`（保存、toast、帧）。

## 测试

语法、结论、列表解析与它的回退（对着一个回环 stub）、相对的列表 base，以及页面
整套按键语法，都有单元测试（`llm::local`、`llm::config`、`llm::models`、
`llm::ollama`、`llm::openai`、`app::tests::login`、`ui::tests::login_view`）。
`smoke.sh` 的 Phase 141 拿真二进制对着本地 stub 跑：一个列出一个语言模型和一个
embedding 模型的 LM Studio、一个要求 token 的 LM Studio、一个 0.4 之前的
LM Studio、一个把每个 prompt 都以超出加载上下文为由拒掉的 LM Studio，以及一个
Ollama。每个 LM Studio stub 对不服务的路径都像真的一样答：200 加一个错误对象，
从不给 404。它走过第三个根行、不含本地 Ollama 的 API key 列表、一个以红色显示
且哪儿都没存下的被拒地址、一次保存 `LMSTUDIO_HOST` 并让 `/model` 列出那个语言
模型（绝不列 embedding 那个）的通过、token 这一步（被要、被拒、然后存到地址旁
边）、Ollama 保存 `OLLAMA_HOST`、较老的 LM Studio 通过 `/v1/models` 列出，以
及 LM Studio 的端口在 Ollama 页面上被拒而保存着的 `OLLAMA_HOST` 仍在。接着，一
个跑在 LM Studio 模型上、而它的服务器已经停掉的 session，必须让回合落在
`Nothing is answering at …` 上，而不是 `Waiting for internet…`。在那个
`LMSTUDIO_HOST` 已导出的 session 里，在 `/login` 下保存一个能用的地址必须说
`Saved — unset
LMSTUDIO_HOST in your environment, which overrides it`，而不是确认。最后，一个
跑在溢出 stub 上的 session 必须让回合落在那条加大上下文长度的建议上，而不是一个
空回复。这些检查中的每一条，都见过在把修复拿掉之后失败。token 这一步的通过是
等 `.env` 的写入，而不是等它的 toast：上一次通过的确认读起来一模一样，而 toast
只活 4 秒，所以等 toast 可能在测试返回之前就返回了。这套件会 unset
`OLLAMA_HOST`、`LMSTUDIO_HOST` 和 `LM_API_TOKEN`（最后一个不以 `API_KEY` 结
尾，所以 key 擦除会漏掉它），这样开发者自己的服务器永远不会配置某个 phase。

## 已知限制

- **LM Studio 的窗口就是它的加载设置。** 与 Ollama 的 `options.num_ctx` 不同
  （`docs/providers/ollama.md`），chat-completions 线没法向 LM Studio 要一个上
  下文长度，所以一个按 4096 加载的模型，会在 agent 化回合的第一次读文件时溢出。
  模型已加载时量表显示那个加载的窗口，而拒绝信息会说要到 LM Studio 里加大它，
  或者用 `lms load <model> --context-length 32768`。通过 LM Studio 的
  `/api/v1/models/load` 按 session 的窗口加载选中的模型，是下一步。
- **尚未加载的模型没有量表**，直到它的窗口被读过加载态：一次 `/model` 选择，或
  下一次启动的探测，会在第一个请求把它加载起来之后读到它。
- **LM Studio 没有 Ctrl+T**（见上）。
- **没有「照样保存」** —— 对一个测试不过的地址（见上）。
- **自签名证书**通过 `ZEROTH_CA_FILE` 被信任；询问要不要接受一张，上面已经设计
  了，还没建。
