# 作为 provider 的 Ollama

跑本地模型 —— `/login` → **Sign in with a local model** → **Ollama** → Enter（生
效中的地址会被填出来，并在保存之前先测一遍 —— `docs/providers/local-models.md`）
，然后 `/model` 列出 `ollama pull` 拉下来的东西。

随二进制出厂两个 provider，共用一种**第四种线格式**（`wire_api = "ollama"`，排在
Chat Completions、Responses 和 Messages 之后 —— `docs/providers/chatgpt.md`、
`docs/providers/claude.md`）：

| provider id | 到哪儿 | 凭据 |
| --- | --- | --- |
| `ollama` | 你自己跑的服务器（默认 `http://127.0.0.1:11434`，除非 `OLLAMA_HOST` 另有说法） | 不需要 —— 一个存下的 `OLLAMA_HOST_API_KEY` 在代理要的时候会作为 bearer 骑上去 |
| `ollama_cloud` | `https://ollama.com` | 一把来自 ollama.com/settings/keys 的 API key |

一个用 `ollama signin` 登录过的本地服务器，也会把它的 `-cloud` 模型列在 `ollama`
provider 下；那些模型不在本地加载，下面的窗口规则把它们留在云端的最大值上。

本文里每一个线形状都是对着 Ollama 0.33.2（写作时的当前发布）**观察**来的，而不是
从落后于服务器的文档上抄的 —— 并与服务器自己的源码交叉核对过（`api/types.go`、
`server/routes.go`、`envconfig/config.go`）。

## 为什么用原生 API，而不是 Ollama 的 `/v1`

Ollama 也提供一个 OpenAI 兼容的 `/v1/chat/completions`，现有的 Chat Completions
线本来不需要任何新东西就能驱动它。没有用它，理由有一个、分两半：

**上下文窗口。** 在大多数机器上，Ollama 加载模型时用的上下文是 **4096 个token**
（自 0.15.5 起是按显存分层的默认值：23 GiB 以下 4K，到 47 GiB 是 32K，以上 256K
），超出它的 prompt 会被**静默**截断 —— `server/prompt.go` 从头丢消息，只保留
system 消息和最新那条，响应里没有任何字段，只有一行没人开着的 `Debug`日志。一个
agent 回合 —— 一段 5K token 的人格加 tool schema，然后第一次读文件—— 在第一轮就
会溢出它，而它丢掉的正好是那段对话。原生的 `/api/chat` 逐请求接受
`options.num_ctx`；`/v1` 根本没有 `options` 字段，而 gin 的默认 binder 会**不报
错**地丢掉一个未知的。Cline 以同样的方式得出了同样的结论，也正是为此从 `/v1` 搬
走了。

**元数据。** `/v1/models` 每个模型只回答四个字段（`id`、`object`、`created`、
`owned_by`）—— 没有窗口，没有能力。`/api/tags` 和 `/api/show` 回答了 picker 会问
的三个问题（见下）。

所以 `ollama` 是一种线格式，不是一个 base URL：`src/llm/ollama.rs` 在原生形状与
crate 其余部分所用的 Chat Completions 通货之间双向翻译，和 `responses.rs`、
`anthropic.rs` 完全一样。`OpenAiClient` 之上的任何东西都不知道存在第四种格式：
agent 循环、transcript、rollout 和推导出的上下文都没动。

## provider 文件写了什么

```toml
[providers.ollama]
name = "Ollama"
auth = "optional_key"          # ← no key needed; a stored one still rides
wire_api = "ollama"            # ← the native shape
api_key_env = "OLLAMA_HOST_API_KEY"   # ← this server's own key, not the cloud's
api_base_env = "OLLAMA_HOST"          # ← Ollama's own variable replaces the base

[providers.ollama.kwargs]
api_base = "http://127.0.0.1:11434"

[providers.ollama_cloud]
name = "Ollama Cloud"
wire_api = "ollama"
api_key_env = "OLLAMA_API_KEY"

[providers.ollama_cloud.kwargs]
api_base = "https://ollama.com"
```

有两个键对这个文件是新的：

- **`auth = "optional_key"`** —— `AuthScheme::OptionalKey`。其他每个 scheme 的请
  求没有凭据就发不出去，所以 `ModelConfig::
  is_usable` 拒绝一个无 key 的配置，边
  界退回 dummy。一个本地服务器根本不要凭据；这个 scheme 让 base 一个就够，而一个
  存下的 key仍然作为 `Authorization: Bearer` 骑上去，和必需时完全一样（本地服务
  器忽略这个头部；托管的一个需要它）。`auth::request_auth` 回答它时**没有 I/O**
  ，和 `ApiKey`一样。
- **`api_base_env`** —— 一个它的值**替换** `kwargs.api_base` 的环境变量。在边界
  以和 key 相同的方式解析（进程环境优先，然后 `.env` 存储）进
  `Selection::api_base`，而在这种线上 —— 还要过 Ollama 自己的语法。

### `OLLAMA_HOST`，按 Ollama 的语法

`ollama::host_url` 是 `envconfig.Host()` 的移植：没有 scheme 意味着 `http`，没有
端口意味着 11434 —— 但一个显式的 scheme 带来它*自己的*默认端口（`http://x` 是 80
号端口），裸的 `ollama.com` 是 https 上的云端，路径会保留（反向代理的子路径），
一个无效端口退回默认值，而一个未指定的绑定地址（`0.0.0.0`、`::`）变成客户端真能
连上的回环地址。空值是本地默认。文件自己的 base 走同一条路，所以一个完整 URL 出
来时原样不变。

## 配置好了意味着指过去了

`configured` —— `/login` 里的 ✓，以及 `/model` 会抓哪些 provider —— 在别处都意味
着「有一个凭据解析得出来」。一个什么都不需要的 provider 会永远算配置好了，于是每
次打开`/model` 都会去抓一台大多数用户不跑的服务器，并在已经加载的列表旁边用红字
画上`Ollama unavailable`。smoke 套件的 `/model` 各个 phase 期待的是什么都没配置
时的`run /login to add one` 提示，它们也会读到同样的拒绝。

所以一个按主机配置的 provider，在它**被指过去**时才算配置好：`OLLAMA_HOST` 解析
得出来（进程环境或 `.env`），或者一个 key 解析得出来，或者 `ZEROTH_PROVIDER`
点名它。那也就是 `/login` 那一行所写下的东西。

### 一个 provider，一个变量

这也是两个 provider 不能共用一个 `api_key_env` 的原因。它们曾经共用 —— 两个都读
`OLLAMA_API_KEY` —— 而一个 key 解析得出来是「被指过去」的一半，于是粘贴一把
**Ollama Cloud** 的 key 会在同一次按键里把**本地** provider 也配置好了：它那一行
在`/login` 里变成 `✔ configured`，下次打开 `/model` 会去抓
`http://127.0.0.1:11434`，和云端的列表并排，用红字画上它的拒绝，而云端的凭据则以
每次请求的 bearer 形式骑到 `OLLAMA_HOST` 点名的任何地方。共用一个变量就是共用一
份凭据存储，所以修法是分成两个：`ollama_cloud` 保留 `OLLAMA_API_KEY` —— 那是
Ollama给托管 API 的 key 起的名字，所以一个已经为 `ollama` CLI 导出的 key 只配置
那一行，别的什么也不配置 —— 而你自己跑的服务器读 `OLLAMA_HOST_API_KEY`，也就是给
`OLLAMA_HOST` 点名的那台主机的 key。
`no_two_shipped_providers_read_the_same_key_variable` 为整个文件钉住这条，不只是
这一对。

### `/login` 的服务器页

`/login` → **Sign in with a local model** → **Ollama** 打开本地服务器页（
`docs/providers/local-models.md`）—— 它以前是 API-key 列表里一个「主机字段」，敲
什么就存什么。标题问 `Enter your Ollama server URL`，敲进去的值**按敲的**显示（
盲敲的 URL 就是敲错的 URL），而空字段显示**生效中的地址**：设了 `OLLAMA_HOST` 时
是它的值，否则是默认值。**在保存任何东西之前先按 Enter 测一遍** —— 也就是
`/model` 读的同一个 `/api/tags` + `/api/show` 列表 —— 只有通过才把 `OLLAMA_HOST`
写进`.env`，从那以后 provider 才算配置好。失败会留在页面上、红字，按出错的原因措
辞（`Nothing is answering at 127.0.0.1:11434 — is Ollama's server running?`），
而一个在代理后面、回答 401 的服务器会要它想要的 bearer，和地址一起存成
`OLLAMA_HOST_API_KEY`。敲进去的主机按 Ollama 的语法原样保存（`myhost:11434`、
`0.0.0.0`、`https://host`）；归一化它的是配置，不是这个字段。

`ollama_cloud` 是一个普通的 key provider：它那一行在 **Sign in with an API key**
之下列出，要它的 key。

## 上下文窗口

页脚量表所对的数字 —— `{used}/{window} ({pct}%)`，自动压缩的触发器 —— **就是请求
向服务器要的那个数字**（`options.num_ctx`）。任何别的安排都是在对着一个服务器并
不持有的窗口做计量。`ModelConfig::context` 携带它；边界的 `config_for` 从量表读
的同一个 `context_window()` 里填它，所以两者不可能不一致；而启动时的能力探测 ——
它从列表里学到窗口 —— 只在这种线上重绑后端（`wire_sends_context`），好让下一个请
求就带上它。

### 规则

`ollama::context_window`，逐模型，按顺序：

1. Modelfile 自己的 `num_ctx`（`/api/show` → `parameters`）—— 用户对每个模型的选
   择，也是列表要显示每一个模型、而不能只信 `/api/tags` 的原因（Roo Code 是吃了
   苦头才学到这一点的：把发现到的窗口强加在用户的 `PARAMETER num_ctx 4096` 之上
   ，爆了他们的显存）；
2. 否则服务器的配置默认值 `OLLAMA_CONTEXT_LENGTH`，从**本**进程的环境读，作为
   `ollama serve` 启动时所用配置的镜像；
3. 否则 `DEFAULT_NUM_CTX_CAP`，**32 768** —— Ollama 自己的中间显存档，也是
   Cline 的默认值：一次裸装的 4096 对 agent 没法用，而模型自己的最大值
   （128K、256K）是把显存填满的那种；

**永远不超过模型能持有的量**（它的 `context_length`；服务器反正会在那里夹取，而
对着一个更大的数计量就是撒谎）。一个最大值未知的模型取前两条，或者什么都不要 ——
为它凭空造一个窗口，就是对着没人声明过的数字计量。云端以最大值服务一个模型，所以
直接的`ollama_cloud` 列表和本地服务器的 `-cloud` 代理都是**不设上限**的。

`ZEROTH_CONTEXT_WINDOW` 压过这一切，正如它压过每个 provider 报告上来的窗口 —
—而在这种线上它同时也是*发出去*的东西，所以它是唯一能把一个本地模型抬过上限的旋
钮。

两个值得知道的后果：

- **改 `num_ctx` 会重载模型。** 窗口是一个加载时的选项，所以一次切换要付一次加载
  延迟；之后在一个 session 里它就稳定了（这也是为什么窗口是每 session 固定的值，
  而不像 aider 那样按每个请求定尺寸 —— 一个逐轮改它的 agent 循环会逐轮重载）。
- **溢出仍然会截断。** 有量表和自动压缩让一个回合待在窗口的 90% 以内，这很少见，
  而Ollama 的 `truncate: false`（改成报错）是刻意不发的：那样一条超大 tool 输出
  就会让这一轮失败，而本可以修好它的压缩会发出同一段上下文，也失败。丢掉最旧的消
  息并继续，是更好的降级。

## 模型元数据

列表原生地回答全部三个能力问题 —— 没有硬编码的表格 —— 靠
**`/api/tags` 加每个模型一次 `/api/show`**：

| `ModelEntry` 字段 | 它来自哪里 |
| --- | --- |
| `context` | 上面的规则，作用在 `details.context_length`（tags，0.19+）/ `model_info["<arch>.context_length"]`（show）以及 Modelfile 的 `num_ctx`（只在 show 里） |
| `vision` | `capabilities` 含 `vision` |
| `reasoning` | `capabilities` 含 `thinking` |

`/api/tags` 自 0.9.1 起带 `capabilities`，自 0.19 起带窗口；一条 tags 记录是抓取
在某个模型的 show 失败时**退回**的东西，所以一个更旧的服务器仍然能按它说了什么列
出。一个根本没有 `models` 列表的 tags 正文，不是一台什么都没拉的 Ollama（那会回
答`{"models":[]}`），而是另一台服务器占了那个端口：LM Studio 对它不服务的每条路
径都回答 200 加一个错误对象，所以把它当成 Ollama 的端口敲进去会被当成那样拒绝（
`tags_records`、`docs/providers/local-models.md`）。能力列表是显式的，所以不在就
是答案 —— 一条有这个列表、但没有 `vision` 的记录看不见（`Some(false)`，后端会降
级附件），而一条根本没有列表的记录仍然未知，也就是其他每个 provider 拿到的乐观默
认值。一个有这个列表但没有 `completion` 的模型（一个 embedding 模型）没有聊天表
面，不予提供。show 的正文只解码成这条记录需要的那几个字段，并受限地读（
`MODELS_BODY_MAX_BYTES`）；它的 `tensors` 列表不读就跳过。

### 思考

能力里的 `thinking` 让一个**开/关推理器** —— qwen3、deepseek-r1：Ctrl+T 的循环是
`off ↔ on`，线是 `think: false` / `think: true`。唯一的例外是 **gpt-oss** 家族
（`details.family == "gptoss"`），它的思考是一个关不掉的**层级**：阶梯是
`low → medium → high`（`think: "low"` ……），没有 `Off` 那一档，因为那里的
`think: false` 会被忽略，而一个读作 `off` 的页脚会撒谎。crate 更宽的阶梯夹到
Ollama 的三档上，正如 Ollama 自己的 OpenAI 层夹 `reasoning_effort`：`minimal` 是
`low`，`high` 以上的一切都是 `high`。

线是围绕三个事实搭的，每个都在真机上验证过：

- **思考默认是开的。** 一个带思考、没有 `think` 字段的模型会思考；所以播种的模式
  是`On`，页脚说的就是服务器实际做的。那个会清掉模式的自动模式 classifier，拿到
  的也是模型的默认。
- **对一个不思考的模型发 `think: true` 是 400**（
  `"gemma3:270m" does not support thinking`）；对一个开关式推理器发一个层级字符
  串会被当成 `true`，而不是拒绝，于是一个陈旧的持久化模式不可能让每一回合都失败
  。
- **思考轨迹作为 `message.thinking` 流出**，一个它自己的字段，而累加器仍然在内容
  上跑 Chat Completions 线的 `ThinkingSplitter`：一个自定义的 GGUF 导入可能缺少
  填这个字段的解析器，转而吐出 `<think>` 标签。

## 线

### 请求

```json
{
  "model": "qwen3:8b",
  "messages": [
    {"role": "system", "content": "…"},
    {"role": "user", "content": "what is this?", "images": ["iVBOR…"]},
    {"role": "assistant", "content": "", "tool_calls": [
      {"id": "call_1", "function": {"name": "read", "arguments": {"path": "f"}}}]},
    {"role": "tool", "content": "L1", "tool_call_id": "call_1", "tool_name": "read"}
  ],
  "stream": true,
  "tools": [ …the Chat Completions specs, verbatim… ],
  "think": true,
  "options": {"num_ctx": 32768, "temperature": 0.5}
}
```

五处形状与 Chat Completions 不同，每一处都是朴素移植会栽的地方：

- **一次 tool call 的 `arguments` 是一个 JSON 对象**，双向都是。那里放一个字符串
  —— Chat Completions 的通货 —— 就是 400（
  `Value looks like object, but can't find closing '}' symbol`）；一个模型截断了
  的调用降级成 `{}`，而不是让请求失败。
- **一个 tool 结果点名它的工具。** `tool_name`（0.9.6+）在 `tool_call_id`（
  0.12.10+）旁边，从同一个列表里早先看到的 assistant 调用上读出来；一个调用从未
  被看到过的结果（一段旧 rollout）省略这个名字，而不是猜一个。两个字段都会被早于
  它们的服务器忽略。
- **图片是裸 base64**，放在消息的 `images` 数组里 —— 没有 `data:` 前缀，没有媒体
  类型。一个非 `data:` 的 URL 没有像素可发，会被丢掉，而不是让这一回合 400。
- **`tools` 原样通过** —— Ollama 的 schema *就是* Chat Completions 那个 ——
  只少了
  `tool_choice`，它不是这里的字段。
- **一个由文件配置的 `options` 表会合并在该 session 自己的键之下**，所以
  `providers.toml` 里的 `num_gpu` 会一起走，而不带一个陈旧的 `num_ctx` 在旁边盖
  掉量表；其他每个 kwarg 都像在每种线上一样在顶层合并。

`temperature` 骑在 `options` 里，也就是 `/settings` 的 **Temperature** 行放它的
地方；`stream: true` 明确写出来，尽管它是默认值。

### 流

NDJSON，一行一个对象 —— 这正是共用的 `pump_lines` 字节循环早就在发的东西，所以
Esc在四种线上被尊重得完全一样。`ChatAccumulator` 折叠它：

```
{"message":{"role":"assistant","content":"","thinking":"Okay"},"done":false}
{"message":{"role":"assistant","content":"Hi"},"done":false}
{"message":{"role":"assistant","content":"","tool_calls":[{"id":"call_v1gr5rr3",
  "function":{"index":0,"name":"get_weather","arguments":{"city":"Paris"}}}]},"done":false}
{"message":{"role":"assistant","content":""},"done":true,"done_reason":"stop",
  "prompt_eval_count":141,"eval_count":120, …}
```

- **一次 tool call 整块到达**在一帧上，从不作为参数片段 —— 它的对象参数被重新序
  列化成执行器解析的那个字符串，它的 id 保留（`call_` + 8 个字符，自 0.12.10 起
  由服务端铸造）或合成（`call_{n}`），而名字加参数在调用落地时被呈现给 token 计
  数。
- **`done_reason` 即使在一个 tool 回合里也是 `stop`。** crate 的 `finish_reason`
  一旦看到过调用就变成 `tool_calls` —— 那是 agent 循环读来决定再跑一轮的东西。
- **用量是最后一帧的 `prompt_eval_count` / `eval_count`。** 已在真机上验证，
  `prompt_eval_count` 在缓存命中时报告的是**整段** prompt（一个重复请求两次都是
  85），所以量表可以信它 —— 不像 Anthropic 那个不含缓存的余量
  （`docs/providers/claude.md`）。不存在缓存或推理的细分，所以那些留 0，而
  `Thought for …` cell 保住它的 token 估计。
- **一条带内的 `{"error": …}` 行**（一个在流中途失败的 200）让这一回合失败；一个
  非 2xx 状态从传输层作为普通 API 错误到达，并在下文解释。

`accept: application/x-ndjson` 点明请求期待的东西。

## 它不工作时

大多数 Ollama 失败是关于*机器*的，`openai::explain` 在这种线上把它们改写
（`ollama::advice`、`ollama::connection_advice`）成说要做什么的话：

| 发生了什么 | 它说什么 |
| --- | --- |
| 连接被拒绝 | `Could not reach Ollama at {base} — is `ollama serve` running? …` |
| 主机解析不了 | 检查 `OLLAMA_HOST` |
| 404 `model 'x' not found` | 用 `ollama pull x` 拉它，或用 /model 换一个 |
| 400 `… does not support tools` | 在 /settings 里关掉 Tools，或换一个支持的模型 |
| 400 `… does not support thinking` | 按 ctrl+t 关掉它 |
| 400 `Multimodal data provided, but model does not support …` | 附件被拒了；换一个视觉模型 |
| 401 / 403 | 设 `OLLAMA_API_KEY`（一把云端 key，或用 /login 粘贴一把），或者为自建服务器设 `OLLAMA_HOST_API_KEY` |
| 429 | 被限流了；等一会儿 |
| 502 | 代理后面的云端模型够不到 |

被拒绝的连接还会先重试三次（`llm::retry` 把传输失败当作瞬时的）—— 立刻重试，因为
没有东西在监听。

**冷加载很慢。** 模型进内存之前 Ollama 什么都不发，所以一个大模型的第一个请求可
能在它的第一个字节之前坐等几十秒；`NET_OP_TIMEOUT`（120 s）给它设界，而一次超过
它的加载会以重试的形式呈现 ——  与此同时加载在服务器上继续，所以重试通常能落上。
改`num_ctx`（一次 `/model` 切换、一个 `ZEROTH_CONTEXT_WINDOW`）再付一次加载
。

## 测试

纯模块对着从 0.33.2 抓下来的帧和正文做单元测试：主机语法、载荷、消息翻译、NDJSON
折叠、目录解析和窗口规则、那些建议。边界由 `tests/live_ollama.rs` 覆盖 ——
`#[ignore]` 的真机测试，对着一个持有三个小模型的服务器（`qwen3:1.7b` 测工具和思
考，`gemma3:270m` 测两个都没有的模型，`moondream` 测视觉），驱动目录、一个思考回
合、一个tool 回合、一个图片回合、窗口到达 `/api/ps`，以及那些被解释过的拒绝：

```sh
ollama pull qwen3:1.7b && ollama pull gemma3:270m && ollama pull moondream
cargo test --test live_ollama -- --ignored --nocapture
```

tool 回合在思考**开着**时跑：`qwen3:0.6b` 在这种线上能思考也能聊天，但对着四个真
实的tool schema 组不成调用（它吐出服务器解析器会丢掉的一个坏调用，或者把一坨原始
JSON当作内容），连 1.7b 那个也只有在推理一番之后才可靠地做到 —— 是模型的极限，不
是线的，在为 agent 工作挑本地模型时值得知道。

`scripts/smoke.sh` 在它的 `*_API_KEY` 清理旁边也 unset `OLLAMA_HOST` 和
`ZEROTH_PROVIDER`，好让开发者自己的 Ollama 指向从不使套件的 `/model` 各个
phase 去抓一台它不跑的服务器。

## 环境

| 变量 | 效果 |
| --- | --- |
| `OLLAMA_HOST` | `ollama` provider 的服务器在哪，按 Ollama 的语法；设它（或 `/login` 写它）正是让这个 provider 配置好的东西 |
| `OLLAMA_API_KEY` | `ollama_cloud` 的 key（ollama.com/settings/keys）—— 也仅仅是它自己的：它从不配置那个「你自己跑的服务器」provider |
| `OLLAMA_HOST_API_KEY` | 给 `OLLAMA_HOST` 点名的那台服务器的 bearer，当它前面的代理要一个时；可选，也是 `ollama` 读的东西，而不是云端的 key |
| `OLLAMA_CONTEXT_LENGTH` | 服务器的配置默认窗口，镜像进窗口规则 |
| `ZEROTH_CONTEXT_WINDOW` | 压过规则，并作为 `num_ctx` 发出 |
| `ZEROTH_PROVIDER=ollama` | 也算作指向它 |

全都以普通方式解析（`docs/providers/llm.md`）：一个真实的进程环境变量胜过 `.env`
存储。

## 已知限制（v1）

- **一个没有 `tools` 能力的模型需要关掉 Tools。** picker 会列出它（它能聊天，而
  一个纯视觉模型值得拥有），但一个开着工具的请求是 400 —— 有解释，且离一次
  `/settings`切换 —— 而不是让 tool 集合悄悄缩小去迁就模型。把这项能力读进后端的
  tool 集合是下一步。
- 窗口是每 session 一个值；还没有为它设的 `/settings` 行（
  `ZEROTH_CONTEXT_WINDOW`就是那个旋钮）。
- 列表是每个模型一次 `/api/show`，串行：一个有很多模型的云端目录要几秒才能落进
  picker（与此同时显示的是其他 provider 的列表）。
- 不做模型预热：一个 session 的第一个请求付加载。
- `ollama_cloud` 的 `num_ctx` 和本地的一样发出去；云端尊不尊重它没有文档，而那个
  条目的不设上限窗口无论如何让它就是模型的最大值。
