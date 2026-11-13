# 你自己的 provider —— `~/.zeroth/providers.toml`

应用自带的那些 provider 住在仓库的 `providers.toml` 里，被嵌进二进制。其他任何
讲 OpenAI 兼容 API 的东西 —— 一个托管服务像 Groq、DeepSeek、Mistral 或
Together，一个公司代理，或者你自己机器上的一个模型服务器像 llama.cpp 或 vLLM
—— 都在用户自己的一个文件里添加（Ollama 和 LM Studio 是内置的，
`docs/providers/local-models.md`）：`~/.zeroth/providers.toml`
（`{config_home}/providers.toml`，所以 `ZEROTH_CONFIG_DIR` 会让它随其余状态
一起搬走）。块的形状就是那个自带文件的，而这个文件是**铺在**内置 provider 之上
的：它向它们添加，也能改动其中一个，但绝不替换这一套。

它过去是替换的。第一个解析成功的文件 —— 先是 `ZEROTH_PROVIDERS_FILE`，然后
是工作目录里的 `providers.toml`，再然后是家目录里的那个 —— 就成了整张 provider
表，所以加一个 provider 意味着把整个自带文件抄一遍，而那份副本从此再也拿不到
修复或新 provider。一个解析失败的文件被一声不吭地跳过：应用悄悄跑在内置之上，
而用户写的那个 provider 干脆就不存在。这三件事都去掉了，替代它们的东西在下面。

## 几行就写完一个 provider

```toml
[providers.groq]
name = "Groq"
description = "Fast inference on open models."
api_key_url = "https://console.groq.com/keys"

[providers.groq.kwargs]
api_base = "https://api.groq.com/openai/v1"
```

- **`[providers.groq]`** 是 id。它为 `/model` 里这个 provider 的各行打上标签
  （`[groq]`），在 `config.json` 里和 `ZEROTH_PROVIDER=groq` 里用它命名它，
  并决定 key 从哪里读：`GROQ_API_KEY`，即 id 大写、后面接 `_API_KEY`，标点变成
  `_`。`api_key_env = "…"` 可以指定另一个变量。
- **`name`** 是 `/login` 和页脚所显示的标签。没有它的块用 id 来称呼，所以一个
  本地服务器可以只有一个 base 而没有别的。
- **`kwargs.api_base`** 是必需的 —— Chat Completions 的 base。请求发往
  `{api_base}/chat/completions`，模型列表从 `{api_base}/models` 读。它放在
  `kwargs` 之下 —— 那张表的其他键会被转发进请求正文 —— 因为自带文件一直是把
  它放在那里的。
- **`description`** 和 **`api_key_url`** 是可选的。`/login` 的 key 那一步显示
  前者、链接后者，所以一个没有 key 的用户知道去哪里造一个。

然后 `/login` → **Sign in with an API key** → **Groq** 会把 key 存进
`~/.zeroth/.env`（一个导出的 `GROQ_API_KEY` 也一样管用），而 `/model` 会把
Groq 的模型列在其他每一个已配置 provider 的旁边。**两者打开时都会重读这个
文件**，所以一个在应用运行期间加上的 provider —— 手写，或者让 agent 写出那个块
—— 不重启就在那里。重读不碰正在跑的 backend：它继续用构建它时的配置流式输出，
只有接下来读这张表的东西（那个页面、从中选一个、之后一次重建）才看得到改动
—— 而在编辑中途读到的一个文件，绝不会把正在跑的模型的 provider 拿走（见下文
*在应用运行时编辑它*）。

其他服务用同样的块，配上自己的 id、名字和 base：

| id | `kwargs.api_base` | key 从哪读 |
| --- | --- | --- |
| `openai` | `https://api.openai.com/v1` | `OPENAI_API_KEY` |
| `deepseek` | `https://api.deepseek.com/v1` | `DEEPSEEK_API_KEY` |
| `together` | `https://api.together.xyz/v1` | `TOGETHER_API_KEY` |
| `mistral` | `https://api.mistral.ai/v1` | `MISTRAL_API_KEY` |
| `fireworks` | `https://api.fireworks.ai/inference/v1` | `FIREWORKS_API_KEY` |
| `xai` | `https://api.x.ai/v1` | `XAI_API_KEY` |

## 这张表从哪里来

| | provider 表 |
| --- | --- |
| `ZEROTH_PROVIDERS_FILE` 点名一个文件 | 那个文件**独占**，一个内置都不用 —— 一张精选的表，或一个测试的 stub：smoke 套件把它指向一个 stub provider，好让别的什么都不被抓取 |
| 否则 | 内置 provider，铺上 `{config_home}/providers.toml`；没有这个文件才是常态 |

无论哪种方式，文件里的块都用同样的方式检查（见下文 *当文件错了*），而一个完全
用不上的文件 —— 读不了、不是 TOML —— 会让内置在启动时继续生效，而不是没有
provider，重读时则让 session 自己的 provider 继续生效。

**工作目录里的 `providers.toml` 永不被读取。** 工作目录常常是用户没写过的一份
检出，而那里的一个 provider 块可以把一把存下来的 key 指向它自选的某个主机：
`[providers.openrouter.kwargs] api_base = "https://…"` 会把用户的 OpenRouter
key 发到那里 —— 而且是在第一帧之前，因为启动时的能力探测不问一声就会列出活动
provider 的模型。会运行或够到东西的项目文件，正是启动审查存在的理由
（`docs/state/config-review.md`）；provider 文件属于用户，所以它只从用户的
config home 读取。（仓库自己的 `providers.toml` 是嵌入的内置的来源，所以从一份
检出运行时什么都不缺。）

## 一个块怎么落地

一个 id 是**新的**块就是一个新 provider。一个点名**自带** provider 的块会逐键地
给它的打补丁：表遇到表就合并，其他任何值 —— 一个字符串、一个数字、一个数组
—— 替换自带的那个。改一项设置不必重述其余的：

```toml
# Venice's web search, beside the shipped include_venice_system_prompt = false
[providers.venice.kwargs.venice_parameters]
enable_web_search = "auto"

# One more header on OpenRouter's requests; the attribution headers stay
[providers.openrouter.extra_headers]
X-Team = "research"

# OpenRouter's chat requests through a company proxy
[providers.openrouter.kwargs]
api_base = "https://llm-proxy.example.com/openrouter/v1"
```

最后那个只挪动 chat 请求：OpenRouter 自带的块点名了自己的 `api_model_base`，
它仍然决定模型列表从哪里来 —— 要连列表一起挪，把 `api_model_base` 也设上。

补丁做不到的是删除：一个自带的键、一个自带的 header、一个自带的 provider 都会
留下。一张少了其中一些的表，才是 `ZEROTH_PROVIDERS_FILE` 的用途。

## 一个块能设的每一个键

| 键 | 它做什么 |
| --- | --- |
| `name` | 标签；未设时是 id |
| `kwargs.api_base` | chat 的 base（必需，除非 `api_base_env` 点名一个提供它的变量） |
| `kwargs.*` | `kwargs` 之下其他任何东西都合并进每一个请求正文，嵌套表也算 |
| `description` | 一句话说明这个 provider 是什么 —— `/login` 的 key 那一步，以及它的边打边搜 |
| `api_key_url` | 它的 key 在哪里造，在那一步上链接 |
| `api_key_env` | key 从哪个变量读；默认 `<ID>_API_KEY` |
| `api_model_base` | 模型列表从哪里读，当它不是 chat base 时 —— 一个 URL，或一个**相对**于 chat base 的路径（`../api/v1`、`./x`、`/x`），按浏览器解析链接的方式解析，所以它跟着 chat base 去到 base 被指向的任何主机（LM Studio 的原生列表，`docs/providers/local-models.md`） |
| `api_base_env` | 一个其值替换 chat base 的变量（`OLLAMA_HOST` 的角色）—— 里面一个裸主机（`gpu-box:8080`）就是一个服务器，用块自己的 base 补全：它的 scheme、端口和 API 路径；带路径的值就是 base 本身 |
| `extra_headers` | 一张随每个请求发出的 header 表 |
| `auth` | 它怎么认证：默认是一个粘贴的 key，`optional_key` 用于不需要 key 的服务器，或某一种登录（`docs/providers/copilot.md`、`docs/providers/chatgpt.md`、`docs/providers/claude.md`、`docs/providers/cline.md`） |
| `wire_api` | 请求形状：默认 Chat Completions，或 `responses`、`anthropic`、`ollama` |
| `drop_params` | 服务器拒绝的请求字段，从每个请求里去掉（见下文） |
| `reasoning_param` | 一个 Chat Completions 服务器从中读取 Ctrl+T 思考级别的字段：默认是 OpenRouter 的 `reasoning` 对象，或 `reasoning_effort`（见下文） |

## 不要 key 的服务器

`auth = "optional_key"`：存下来的 key 作为 bearer 随行，没有也行。这样的
provider 是一个**本地服务器**，列在 `/login` → **Use a local model** 之下，
与自带的 Ollama 和 LM Studio 并列，而不是在 API key 之下，而它的页面在保存任何
东西之前先测连接（`docs/providers/local-models.md`）。一个 key 可选、且**不**
点名任何主机变量的 provider 仅由它的块配置 —— 指向它的文件就是配置，所以
`/model` 不用走一趟 `/login` 就列出它（`Provider::configured_by_definition`），
而它的页面只是测它：

```toml
[providers.llamacpp]
name = "llama.cpp"
auth = "optional_key"

[providers.llamacpp.kwargs]
api_base = "http://localhost:8080/v1"
```

| 服务器 | `kwargs.api_base` |
| --- | --- |
| llama.cpp（`llama-server`） | `http://localhost:8080/v1` |
| vLLM | `http://localhost:8000/v1` |

要让 `/login` 改为询问地址，就为它点名一个变量 ——
`api_base_env = "LLAMACPP_HOST"`。页面随后在块自己的服务器之上提供一个地址
字段，测试所输入的内容（一个裸的 `gpu-box:8080` 会被补全成块里的 `/v1`），
测试通过后存进那个变量，而一旦变量解析成功，这个 provider 就算已配置：Ollama
的规则（`docs/providers/ollama.md`）。没有任何自带的 provider 是仅由它的块配置
的 —— 有一个测试钉住它 —— 因为自带的 `ollama` 和 `lmstudio` 是给运行那些服务器的
用户的，否则每一次 `/model` 打开都会从大多数用户没有的那一个去抓取。一个用户
自己、但没在跑的服务器，至多让 `/model` 在它的计数器旁边显示
`· llama.cpp unavailable`。

LM Studio 本身不再需要块：它是内置的，带着它的原生列表和它的 `LM_API_TOKEN`
（`docs/providers/local-models.md`）。一个按本页早先的例子写下的
`[providers.lmstudio]` 块现在会**给自带的那个打补丁** —— 在 Sign in with a
local model 之下连它一次，或者删掉那个块。

## 严格服务器拒绝的字段

每个 Chat Completions 请求都携带 `stream_options`（索要 token 统计所吸附的那个
最终 usage 帧）和 `prompt_cache_key`（这个 session 的缓存亲和性，
`docs/providers/prompt-caching.md`）。自带的 provider 两个都接受，但一个严格的
服务器会用错误来回答它不认识的字段 —— 比如 Mistral，就用
`Extra inputs are not permitted` 拒绝不支持的参数 —— 而一个 kwarg 只能*替换*
一个字段，因为 TOML 没有 null。`drop_params` 把点名的字段从每个请求里去掉：

```toml
[providers.strict]
name = "Strict"
drop_params = ["prompt_cache_key", "stream_options"]

[providers.strict.kwargs]
api_base = "https://llm.example.com/v1"
```

这些字段在**最后**被去掉（`ModelConfig::drop_refused`），在 kwargs 和思考模式
都被合并进来之后，所以无论是什么把字段放进去的，它都留不下 —— 在每一种线上
格式上，对任何顶层字段都是如此，kwarg 加的也一样。`model` 和 `stream` 永不
被去掉，因为没有它们的正文根本不是一个该发出的请求，而列出其中任何一个都会被
说明。去掉 `stream_options` 可能赔上精确的 token 计数：一个随后不发 usage 帧的
服务器，会让统计停在应用自己的估算上。

## 服务器从哪个字段读思考级别

一个列表里点出思考阶梯的模型（`docs/providers/reasoning.md`），会在每个 Chat
Completions 请求上以 OpenRouter 的 `reasoning` 对象收到 Ctrl+T 级别 ——
`{"effort": "high"}`，或者 Off 时是 `{"enabled": false}` —— 每一个自带的
provider 都读它。一个改为读 OpenAI 自己那个顶层 `reasoning_effort` 字符串的
服务器，就把它说出来：

```toml
[providers.omniroute]
name = "OmniRoute"
api_key_env = "OMNIROUTE_API_KEY"
reasoning_param = "reasoning_effort"

[providers.omniroute.kwargs]
api_base = "http://localhost:20128/v1"
```

OmniRoute 就是需要它的那个案例。它通往 Claude 模型的路由（`cc/…`）只从
`reasoning_effort` 或一个 `thinking` 块读级别，忽略那个对象，所以没有这一行，
为其中一个选的级别什么都不改变；它的 Codex 路由两者都读。于是级别只作为那个
字符串随行，从不与对象并列：级别自己的名字，Off 时是 `"none"` —— 只在模型的
列表点名 `none` 时才提供 —— 而 On 时什么都不给，即模型自己的默认。只有 Chat
Completions 这条线上格式读这个键；其他每一条线都用自己 API 的方式拼写级别，而
GitHub Copilot 的请求无论块里怎么说都取那个字符串
（`docs/providers/copilot.md`）。这个键在 `drop_params` 点名它时像其他任何字段
一样被去掉。

## `/model` 从列表里读什么

picker 列出 `{api_model_base or api_base}/models`。服务器会用两种形状回答，都被
解析 —— OpenAI 的 `{"data": […]}` 信封，以及 Together AI 返回的裸数组 —— 而
每条记录的事实都逐记录读取，无论服务器用哪种拼写：上下文窗口从
`context_length`、vLLM 的 `max_model_len`、Mistral 的 `max_context_length`
—— LM Studio 的原生记录改为点名某个已加载模型被加载时的窗口，而未加载的则是
没有（`docs/providers/local-models.md`）—— 或者 `context_window`（按 95% 估算，
ChatGPT backend 的规则）；图片输入与推理则按 `docs/tools/tools.md` 和
`docs/providers/reasoning.md` 所列的形状读。一份没有任何记录陈述的事实保持未知 —— 页脚的量表隐藏、自动压缩保持
关闭（`ZEROTH_CONTEXT_WINDOW` 可以提供一个窗口），而图片被乐观地附上
（`docs/tools/tools.md`）。

一个把每种能力都压成布尔值的网关也用同样的方式读。OmniRoute 的记录在顶层
`input_modalities` 旁边携带
`"capabilities": {"vision": true, "thinking": true, "effort_tiers": ["low",
"medium", "high"], …}`。
图片输入来自 `input_modalities`，或者在网关不点名模态时来自
`capabilities.vision`。Ctrl+T 的级别是 `capabilities.effort_tiers`，无论旁边
是否立着一个 `thinking` 标志都会读，因为网关只为一个它的能力来源认识的模型写
那个标志；`thinking: false` 会推翻它留在那里的一份列表。网关把 `none` 也算在
级别里，所以 Off 只在列表点名它时才提供。一份不点名任何级别的列表，或者干脆
没有列表，就不提供 Ctrl+T：否则网关会把 Off 不加检查地当作 effort `none` 传给
一个从未列出它的模型。`capabilities.reasoning` 是网关为每个模型写的猜测，不被
读取。这些标志与 Anthropic 的 `capabilities` 树共享键名，但不共享它
`{supported: bool}` 的叶子，两者就是这样被区分开的
（`docs/providers/claude.md`）。那个对象也不是 Copilot 的，所以它旁边的
`supported_endpoints`（对网关发现的一个 Codex 模型是 `["responses"]`）从不把
一个模型从列表中拿掉：网关通过 chat completions 提供每一个
（`docs/providers/copilot.md`）。要让在它的某个 Claude 模型上选的级别到达模型，
它的块还需要 `reasoning_param = "reasoning_effort"`（见上文）。

一个完全没有 `/models` 的服务器什么都不列出，`/model` 会把它注为
`unavailable` —— 一个用 200 回答 `/models` 却不带模型列表（一个错误对象）的也
一样，而空列表不是这种情况；改为在启动时用 `ZEROTH_PROVIDER` 和
`ZEROTH_MODEL` 点名模型。

## 当文件错了

关于这个文件，任何问题都不会被默默跳过。每个问题都是一行，在启动时抬成一条红色
toast，并在 `/login` 或 `/model` **每一次**打开时再抬一次，显示第一行、其余计数
（`(+2 more)`），每一行都以 `~` 相对的文件开头
（`~/.zeroth/providers.toml: …`）。每一次都抬，因为 toast 很容易丢：启动时那条
可能立刻被下一条盖住（比如一个恢复 session 的 checkpoint 通知），而一个 provider
从某个页面上消失了，正是说清原因的时刻。一个问题只让所在的块付出代价：

| 什么 | 那一行 | 它变成什么 |
| --- | --- | --- |
| 一个解析失败的文件 | ``line 3, column 10: expected `.`, `=` — using the built-in providers`` | 整个文件被忽略；重读时那一行改为以 `— keeping the providers already loaded` 结尾，而 session 的表继续生效 |
| 一个读不了的文件 | `Permission denied (os error 13) — using the built-in providers` | 同上 |
| 一个不是表的 `providers` | `providers is not a table — using the built-in providers` | 同上 |
| 一个没写前缀的 provider | `[groq] is not read — a provider goes under [providers.groq]` | 从它那里什么都载入不了 |
| 一张换了个名字的 provider 表（`[provider.groq]`、codex 的 `[model_providers.groq]`） | `[provider] is not read — providers go under [providers.<id>]` | 同上 |
| 一个没人读的键 | `providers.groq.descripton is not a known key` | 这个键被忽略；provider 照常载入 |
| 一个其他工具拼写不同的键 | `providers.groq.base_url is not a known key — set api_base under [providers.groq.kwargs]`（`api_base` 同理；`env_key` → `api_key_env`；`headers`、`http_headers` → `extra_headers`；`model`、`models` → `choose a model with /model`） | 同上 |
| 一个写进文件的 key | `providers.groq.api_key is not a known key — keys are not read from this file: use /login` | 同上；那个值永不被回显 |
| 一个这个构建不认识的 `auth`、`wire_api` 或 `reasoning_param` | `providers.groq.wire_api = "chat_completions" is not known — using chat completions`（`reasoning_param` 的：`… — using the reasoning object`） | 用默认值 |
| 点名 `model` 或 `stream` 的 `drop_params` | `providers.groq.drop_params cannot drop "stream" — every request needs it` | 列表其余部分照常生效 |
| 一个类型错的值 | ``providers.groq: invalid type: string "x", expected a map, in `extra_headers` — skipped`` | 这个块被跳过 |
| 没有 chat base | `providers.groq has no api_base under [providers.groq.kwargs] — skipped` | 同上 |
| 一个不是 URL 的 base | `providers.groq.kwargs.api_base "api.groq.com/openai/v1" is not an http(s) URL — skipped` | 同上（一个 Ollama 线上的主机像 `gpu-box:11434` 是例外，一个相对的 `api_model_base` 像 `../api/v1` 也是） |
| 一个不是表的块 | `providers.groq is not a table — skipped` | 同上 |

一个给自带 provider 打了补丁的坏块，结尾是 `— kept the built-in` 而不是
`— skipped`：自带的 provider 保持自带的样子。一个属于 session 正在跑的 provider
的坏块，结尾是 `— kept the one in use`（见下文 *在应用运行时编辑它*）。

未知的 `auth` 与 `wire_api` 值仍然降级为默认，而不是让这个块失败 —— 那条让为
新构建写的文件仍能载入的规则（`docs/providers/copilot.md`）—— 未知的键也是
如此；变了的是：用户自己的一个笔误会被说出来。

**key 不进这个文件。** 一个 provider 文件是要被阅读、分享、在机器之间拷贝的，
而 key 不是：key 住在 `~/.zeroth/.env` 里（由 `/login` 写入）或环境里，而块里
的 `api_key` 会被报告，永不被读取。

## 在应用运行时编辑它

`/login` 和 `/model` 打开时会重读这个文件，所以它常常是在编辑中途被读的：存了
一半的保存，一个为了贴到别处而被剪掉的块，一个正在重敲的 base。一次重读绝不会
从正在跑的模型那里拿走任何东西：

- 一个**完全用不上**的文件 —— 读不了、不是 TOML、没有 provider 表 —— 让 session
  的 provider 保持原样：
  ``~/.zeroth/providers.toml: line 1, column 16: invalid table header, expected `.`, `]` — keeping the providers already loaded``。
- 一个能载入但不再给出**正在使用的 provider 的可用定义**的文件 —— 它的块被跳过，
  或者从文件里没了 —— 保留正在跑的模型所依据的那个定义，它那一行以
  `— kept the one in use` 结尾（块被移除时是
  `providers.acme is not in the file — kept the one in use`）。文件其余部分照常
  生效。
- 对正在使用的 provider 的一次可用编辑会生效，从下一次重建起。

这要紧，是因为正在跑的模型在每一次重建时都会重新解决它的 provider —— 一次
`/settings` 对 Temperature、Tools、Error retry 或 Max tool calls 的改动，一个
Ctrl+T 思考模式，一个 `/fast` 档位，能力探测的答案 —— 而一个从表里消失的
provider 会让那次重建一声不吭地跳过：设置显示为已应用，却永远到不了请求。保留
正在使用的定义，也是为什么一个在编辑中途弄坏的、对自带 provider 的补丁不会把
正在跑的模型打回自带的块：一个打出到 OpenRouter 上的代理继续携带它的请求，而
不是被绕开。只有正在使用的 provider 会被保留 —— 一个 session 没在它上面跑的
provider，在它的块消失时就离开 `/login`，与启动时一样 —— 所以切到另一个模型
之后，被保留的那个也会在下次重读时离开，而一次重启把决定权留给文件。

## 代码在哪里

- **`llm::config`**（纯）—— `ProvidersFile::with_user_layer` 与 `standalone`
  （合并、`merge_tables`、每一项检查、`layer_block`，以及一次重读所保留的
  `in_use` provider），每一个都对完全用不上的文件返回 `Err`，所以替代生效的
  是什么由调用方来说；`Provider::unread`，一个块设了而解析从未读过的键 —— 由
  一个 `#[serde(flatten)]` 字段保存，所以 struct 本身就是键的清单，自带文件的
  测试不需要手维护一张；`Provider::configured_by_definition`；
  `Provider::drop_params` 与 `ModelConfig::drop_refused`，每条线上格式的请求
  构建器最后调用它们；`Provider::reasoning_param`
  （`llm::ReasoningParam`），`openai::OpenAiClient::build_payload` 读它来通过
  `reasoning::reasoning_body` 或 `reasoning::reasoning_effort_value` 拼写思考
  模式。
- **`llm::models`**（纯）—— `parse_models` 读两种列表形状，而
  `context_window_of` 读窗口的各个拼写。
- **`tui::config::load_providers`**（边界）—— 读哪个文件，它的行以 `~` 相对的
  文件开头；`startup_providers` 退回内置，而
  `ModelSession::reload_providers` 在 `/login` 或 `/model` 每次打开时运行，
  退回 session 自己的表，并把正在使用的 provider 交进去；
  `Session::report_provider_issues` 抬起那条 toast。

## 测试

- **单元** —— `src/llm/config.rs`：
  `a_user_file_adds_its_providers_beside_the_builtins`、
  `a_user_block_naming_a_shipped_provider_patches_only_what_it_sets`、
  `a_standalone_file_is_the_whole_table`、上面那张表每一行一个测试、
  `a_provider_without_a_name_is_called_by_its_id`、重读的五个
  （`a_reread_keeps_the_provider_in_use_when_its_block_cannot_be_used`、
  `…_when_its_block_is_gone`、
  `a_reread_keeps_a_patched_builtin_in_use_rather_than_the_shipped_one`、
  `a_reread_takes_a_usable_edit_of_the_provider_in_use`、
  `a_reread_keeps_only_the_provider_in_use`），无 key 规则的那一对
  （`a_keyless_provider_naming_no_host_variable_is_configured_by_its_definition`、
  `no_shipped_provider_is_configured_by_its_definition`），`drop_params` 的
  三个和 `reasoning_param` 的两个
  （`a_provider_can_ask_for_the_thinking_level_as_reasoning_effort`、
  `an_unknown_reasoning_param_is_said_and_keeps_the_object`），线上格式自己的
  那个在 `openai.rs` 里
  （`a_reasoning_effort_provider_hears_the_level_as_a_top_level_string`），
  以及 `reasoning.rs` 里（`a_reasoning_effort_server_hears_off_as_none`）；
  `openai.rs`、`responses.rs`、`anthropic.rs` 与 `ollama.rs` 里每条线上格式
  一个 `drop_params` 测试；`src/llm/models.rs`：
  `a_listing_that_is_a_bare_array_reads_like_the_envelope` 与
  `the_window_spellings_self_hosted_servers_use_are_read`。
- **Smoke** —— Phase 140（`scripts/smoke/phases/140-userproviders.sh`）把一个
  provider 写进该阶段的 HOME 里的 `~/.zeroth/providers.toml`，并驱动真实二进制
  对着一个 stub，后者以裸数组列出它的模型、并记录每个请求的键与 temperature：
  `/login` 显示这个 provider 与每一个自带的并列且已配置，它的 key 那一步显示
  文件的 description 与 key 页面，`/model` 列出 stub 的模型，一个回合以 key 作为
  bearer、且不带 `drop_params` 点名的字段流式输出，会话中途加上的一个块在下次
  `/login` 时是一行，而一个位置放错的 `base_url` 是一条 toast，点名文件和修法
  —— 下次打开时再说一遍。在正在使用这个 provider 时，一个存了一半的文件保留
  每一个 provider，而一次 `/settings` 的 Temperature 改动仍然到达请求，正在使用
  的 provider 的坏块被保留、它的下一个请求仍然发往 stub，而修好之后的文件什么
  都不说。启动时一个解析失败的文件是一条 toast，点名它的行，内置继续顶替；设了
  `ZEROTH_CONFIG_DIR` 时，`~/.zeroth/providers.toml` 不被读取；而种在工作目录里
  的 `providers.toml` 永远不会成为一行。
