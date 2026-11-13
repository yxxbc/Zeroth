# 真正的 LLM 后端与 `/model` picker

应用自带一个罐头式的 [`DummyAi`](../../src/stream/dummy/mod.rs)，所以它能离线
运行，`smoke.sh` 也能保持确定性。本文讲的是从任何 OpenAI 兼容端点流式传输的
**真**后端、把 dummy 切回来的那个 flag，以及列出并切换模型的内联 **`/model`**
picker。

## 接缝

事件循环的一切都没变：后端仍然是一个
[`stream::ReplySource`](../../src/stream/source.rs)，它的
`spawn(prompt, images, context, tx, cancel)` 跑在一个普通的 OS 线程上，*只发*
`StreamEvent` 并轮询 `CancelToken`。`main.rs` 把后端持有成一个
**`Box<dyn ReplySource>`** 而不是具体的 `DummyAi`，于是 `/model` 能在运行时
把它换掉（`start_turn`/`flush_next_queued` 取 `&dyn ReplySource`）。`context`
是从历史推导出的整段对话——真模型所需的多轮记忆，图片附件以嵌入的 `data:` URL
发送——见 `docs/architecture/context.md`。

## `llm` 模块（真模型的 I/O 边界）

`src/llm/` 是唯一讲 HTTP 的地方。它的纯核心有单元测试；网络调用是边界代码
（像 `main.rs`/`term.rs` 一样），靠手工验证。

| 文件 | 角色 | 纯？ |
| --- | --- | --- |
| `llm/mod.rs` | `ChatMessage`、模块粘合、`LlmError`/`Result` | 大部分 |
| `llm/cache.rs` | prompt 缓存请求的塑造——哪些模型需要显式的 `cache_control` 断点，以及把它们放进去的 wire-JSON 改写（`docs/providers/prompt-caching.md`） | **纯** |
| `llm/config.rs` | `providers.toml` → `Provider`/`ProvidersConfig`，用户自己的文件覆盖在内置之上，其中每个问题都写成 toast 的措辞（`docs/providers/user-providers.md`）、`ModelConfig`、key/模型解析、把粘贴的 key 与订阅登录区分开的 `AuthScheme`（`docs/providers/copilot.md`） | **纯** |
| `llm/copilot.rs` | GitHub Copilot：device flow 的 wire 形状与轮询裁决，每次请求都经以解析的 OAuth→bearer 交换，以及它的 API 坚持的请求身份（`docs/providers/copilot.md`） | 分裂 |
| `llm/cline.rs` | Cline 的账户登录：WorkOS device flow 的形状与裁决、register/refresh 信封、`workos:` 的 bearer 拼法、新鲜度规则（纯）；流程的 HTTP 调用、access token 缓存与轮换写回（`docs/providers/cline.md`） | 分裂 |
| `llm/keystore.rs` | `EnvFile` —— `/login` 流程持久化 key 所用的 `.env` 读写器 | **纯** |
| `llm/settings.rs` | `Settings` —— 跨运行持久化 `/model` 选择的 `config.json` 读写器，**按工作目录**（`docs/state/per-directory-state.md`） | **纯** |
| `llm/thinking.rs` | `ThinkingSplitter` —— 把 `<think>`/`<reasoning>` 标签（以及原生的 `reasoning` 增量）从流里剥出来 | **纯** |
| `llm/reasoning.rs` | `ThinkingMode`/`ReasoningSupport` —— Ctrl+T 的思考模式循环 + 它的请求 body（`docs/providers/reasoning.md`） | **纯** |
| `llm/service_tier.rs` | `ServiceTier`/`SpeedState` —— codex 的 fast mode：一个 ChatGPT 模型列出的速度档位，每一档都是自己的 palette 命令（`/fast`、`/ultrafast`）来切换，并作为 `service_tier` 搭在请求上（`docs/providers/fast-mode.md`） | **纯** |
| `llm/openai.rs` | `OpenAiClient` —— 端点/payload 构造（纯）+ 阻塞式 SSE 流（边界）。请求是从消息按引用写出的带类型 `ChatRequest`；`build_payload` 是测试所读的那份 JSON 树视图 | 分裂 |
| `llm/body.rs` | 一个请求如何离开进程（`docs/architecture/memory.md`）：`BodySource`，即每种 wire 的持有式请求通过写出它带类型的、借用的形式来实现的 trait；`streamed_request`，在自己的线程上把它序列化进一条 64 KB 块的有界管道让 transport 抽走，`Content-Length` 来自一次计数遍历——于是一个带图片的 body 从不会整体驻留内存 | **纯** |
| `llm/models.rs` | `/v1/models` 响应 → `Vec<ModelEntry>`（解析纯；获取是边界；每个条目携带它模型的推理能力——`docs/providers/reasoning.md`；记录一次解码一条，落在借用的 `RawValue` 切片上，从不是整表的树——`docs/architecture/memory.md`；Ollama wire 的 `/api/tags` + `/api/show` 走查也在这里，Cline 登录的也是：ClinePass 计划发布的清单，每个模型的事实借用自 Cline 的目录——`docs/providers/cline.md`） | 分裂 |
| `llm/ollama.rs` | Ollama 的**原生** wire format（`wire_api = "ollama"`）：`OLLAMA_HOST` 的语法、`/api/chat` 的 body（`options.num_ctx`——它在 Ollama 的 `/v1` 之外存在的原因）、NDJSON 折叠、带上下文窗口规则的目录解析，以及带解释的拒绝（`docs/providers/ollama.md`） | **纯** |
| `llm/cloud_code.rs` | Google 的 **Cloud Code Assist** wire format（`wire_api = "cloud_code"`）：套在 Gemini `generateContent` body 外的 `{project, model, request}` 信封、按家族的工具 schema（Gemini 的 `parametersJsonSchema`，Claude/GPT-OSS 走桥的六个关键字）、按家族的思考，以及 SSE 折叠，以及整个进程范围的 memo：后续请求必须交回的 thought signature（以及 Claude 的思考）（`docs/providers/antigravity.md`） | 大部分 |
| `llm/antigravity.rs` | Google Antigravity 登录：PKCE 的 URL 与授权、agy CLI 的 `User-Agent`、`loadCodeAssist`/`onboardUser` 的形状、目录的筛选与分级分组、带解释的拒绝（纯）；流程、token 缓存、项目查找与目录获取（`docs/providers/antigravity.md`） | 分裂 |
| `llm/backend.rs` | `LlmBackend: ReplySource` —— 把 SSE 增量桥接成 `StreamEvent` | 边界 |

### 为什么用阻塞的 `reqwest`、一个 transport 线程，和一个手搓的 SSE 读取器

`ReplySource::spawn` 交给循环一个 `std::thread::JoinHandle`，与 `DummyAi` 完全
一样——没有嵌套的 tokio 运行时。每一个**阻塞的网络操作**——上传请求 body 并等
响应头的 `send()`，以及每一次 body `read()`——都跑在一个更外层的**脱离的
transport 线程**上（`openai::run_transport`），它通过一个 `std::sync::mpsc`
通道转发 body 块（或结束这条流的那次失败）；干净的 EOF 只是 drop 掉 sender。
流式线程排空那个通道（`openai::drain_stream`，对网络而言是纯的，通过喂通道来
做单元测试）：它在任意的块边界上重组 SSE `data:` 帧，并在通道安静时每 ~50 ms
轮询一次 `CancelToken`——与 dummy 的 `nap` 同一种协作式取消形状，但 Esc/退出
**即使网络正阻塞**也能及时被确认。取消时，排空立刻返回并 drop 掉它的 receiver；
transport 线程在下一次通道发送时退出（或当它当前操作超时时），脱离且有界——
从不 join。

客户端每操作的超时（`openai::NET_OP_TIMEOUT`，120 s——阻塞的 `reqwest` 把它
作用于整个 send/header 交换和每次 body 读取）纯粹是一个**停滞探测器**：对一次
视觉请求的多兆字节 base64 上传和一个忙碌 provider 缓慢的首个 header 都足够
宽裕，同时仍给真正死掉的连接设了界，它的超时变成一个普通的可重试失败。它之前
那个 3 s 的版本（`STREAM_OP_TIMEOUT`）还兼作取消唤醒，而这种耦合恰恰在「慢但
活着」的请求上失效：一张粘贴的图片，其上传装不进 3 s，就把整个重试预算花在撞
同一堵墙上（`retrying 1/3 … 3/3`，然后浮现超时），而一次缓慢的 header 交换会
在一个正常文本回合的几秒里闪出虚假的重试计数。

`reqwest` 遵循环境里的 `HTTPS_PROXY`；agent proxy 的自定义 CA 在运行时从
`SSL_CERT_FILE`（或 `ZEROTH_CA_FILE`）加载并作为一个额外的信任根加进去，
于是 `rustls` 让构建里没有系统 OpenSSL。

### 重试失败的请求（`llm::retry`）

一次网络抖动——DNS、TLS、代理、连接被断，或者一次暂时的 `429`/`5xx`——否则会把
原始的 `request failed: error sending request for url …` 直接抛给用户并杀掉
回合。后端现在在放弃前**最多重试
[`MAX_RETRIES`](../../src/llm/retry.rs)（3）次**，并在状态行里实时显示尝试
（`… · retrying 2/3 · …`，琥珀色）。

策略住在纯的、有单元测试的 [`llm::retry`](../../src/llm/retry.rs) 里：

- **`is_retryable`** —— transport 失败（`LlmError::Http`）和暂时性的服务器
  状态（`408`、`429`、`500`、`502`、`503`、`504`）会重试；客户端错误
  （`4xx` 的认证/未找到/坏请求）、解码失败和取消不会（重试它们只会以同样的
  方式失败）。
- **`retry_backoff`** —— 从 500 ms 起指数增长，每次重试翻倍、封顶 8 s，于是
  三次重试分别等 **500 ms → 1 s → 2 s**。可中断（`sleep_cancellable`），所以
  退避期间的 Esc/退出仍在 ~50 ms 内被回收。
- **`next_step`** —— 重试一次**尚未发出任何内容**（`emitted == false`）而预算
  还够（`attempt < max`）的可重试失败，以及一次已经发过内容的**transport**
  失败（`LlmError::Http`）——中途断掉的连接。在字节已经流出之后再重新开始一个
  请求会把它们**重复**，所以那次重试被宣告为一次作废式重试：每个
  `StreamEvent::Retrying` 的消费者都会撤回失败那次所流出的内容，然后才是下一次
  尝试的第一个字节（`docs/providers/offline.md` 的 *A connection lost
  mid-reply*）。其他任何中途失败的——一个带内错误帧、一次解码失败——都直接
  浮现而不是重试。
- **`run_stream`** —— `backend.rs::spawn` 包住它那一次流式尝试所用的通用驱动器：
  它把每次重试宣告为
  [`StreamEvent::Retrying { attempt, max }`](../../src/stream/event.rs)，退避，
  重跑这次尝试，然后发出终局的 `StreamDone` / `Error`（取消时则什么也不发）。
  它对尝试和 sleep 都是泛型的，所以整个循环用假实现、**不碰网络**做单元测试。

循环把 `Retrying` 变成 `App::withdraw_attempt`——收回失败那次流出的任何东西，
并对任何已经到达 scrollback 的部分做 purge 重建——以及 `App::set_retry`，它
设置 `TurnStatus::retry`；下一个流出的 `Chunk`/`ThinkingChunk` 会清掉它（请求
恢复了）。整个过程中回合保持存活。

**根本连不上的连接走另一套策略**（`docs/providers/offline.md`）。一次失败的
send 在那道仍持有带类型 `reqwest` 错误的接缝上被分类（`LlmError::transport`）：
`is_connect()`——DNS、TCP、TLS、连接超时、不可达的代理——变成
`LlmError::Unreachable { host, message }`，message 携带整条因果链，而
`next_step` 用 `RetryStep::AwaitConnection` 回答它：一个
`StreamEvent::Offline { host, attempts }` 宣告、一个从 1 s 起翻倍、封顶 5 s 的
`offline_backoff`，以及再一次尝试——**不计入** `max`，只要断网还在持续。循环把
它显示为琥珀色的 `Waiting for internet…` 动词，带一个 `offline for Ns` 从句，
行下有一行 `No connection to {host}`。`0` 的 `Error retry` 像其他一切一样立刻
浮现它；Ollama wire 对一个被拒的本地服务器保留它的 `ollama serve` 建议。

### 推理（`ThinkingSplitter`）

有些 provider 把推理作为原生的 `reasoning`/`reasoning_content` 增量流出；另一些
把它内联成 content 里的 `<think>…</think>`。`ThinkingSplitter::feed` 把两者都
归一成一个 `(response_delta, reasoning_delta)` 对。后端把一个非空的 reasoning
增量映射成 `ThinkingStart`（一次）+ `ThinkingChunk(text)` 流 + `ThinkingEnd`
（当响应文本恢复时），驱动既有的 `Thinking for Ns` 状态。

## 配置与 dummy 开关

`providers.toml`（repo 根的那份，作为内置嵌入）声明各 provider；块的形状见文件
自己的注释。其中两个键改变一个 provider 如何被*抵达*，而不是它叫什么，而两者在
这个构建不认识的取值上都会退到默认：`auth` 决定该 provider 出现在哪个 `/login`
列表里，以及 `Authorization` header 拿到什么（`docs/providers/copilot.md`、
`docs/providers/chatgpt.md`、`docs/providers/antigravity.md`、
`docs/providers/ollama.md`——它的 `optional_key` 就是「不需要 key 的服务器」的
方案），而 `wire_api` 决定请求取 Chat Completions、Responses、Messages、
native-Ollama 还是 Cloud Code 形状（`docs/providers/chatgpt.md`、
`docs/providers/claude.md`、`docs/providers/ollama.md`、
`docs/providers/antigravity.md`）。它们刻意独立——你怎么认证和请求取什么形状
是两个问题。第三个可选键 `api_base_env` 点名一个替换 base 的环境变量
（Ollama 的 `OLLAMA_HOST`）。表里是附带的那些 provider（`a0_venice`——即
Agent Zero/Venice 代理——以及它背后 Venice 自己的 API `venice`，
`docs/providers/venice.md`；`openrouter`；`github_copilot`；`chatgpt_codex`；
`anthropic` 与 `anthropic_console`；`cline` 与 `cline_pass`
（`docs/providers/cline.md`）；`antigravity`（`docs/providers/antigravity.md`）；
`ollama` 与 `ollama_cloud`；以及 `lmstudio`，`docs/providers/local-models.md`），
用户自己的 `~/.zeroth/providers.toml` **覆盖**在其上：一个新 id 的块添加一个
provider，一个点名附带 provider 的块只改它设置的那些键，而文件里的每个问题都是
一个 toast 而不是沉默（`docs/providers/user-providers.md`）。`ZEROTH_PROVIDERS_FILE`
改为点名一个**独立**存在的文件，旁边没有内置的。工作目录里的 `providers.toml`
从不被读：否则一个 checkout 就能把存下的 key 指向它自己挑的 host。

活动后端在启动时选定——先看环境，再看**持久化的选择**（`~/.zeroth/config.json`，
由 `/model` 写入——**本工作目录的**条目，或者对一个首次在其中启动的目录，取
任何地方最后一次做出的选择，`docs/state/per-directory-state.md`），再看文件的
默认——并可以被 `/model` 现场切换。一个**恢复**的对话随后带回它自己的 rollout
所记录的模型，目录的条目只是*新* session 的起点
（`docs/state/session-model.md`；下面的 env 钉住仍然压过它）：

| env var | 含义 | 默认 |
| --- | --- | --- |
| `ZEROTH_DUMMY` | truthy（`1`/`true`/`yes`）强制 dummy 后端 | 未设置 |
| `ZEROTH_PROVIDER` | 活动 provider id | 保存的选择，然后文件里的第一个 |
| `ZEROTH_MODEL` | 活动 model id | 保存的选择，然后未设置 → dummy |
| `ZEROTH_API_KEY` | 通用 API key（fallback） | 未设置 |
| `<PROVIDER>_API_KEY` | 按 provider 的 key，例如 `OPENROUTER_API_KEY` | 未设置 |
| `ZEROTH_CONFIG_DIR` | config home（存放 `.env` + `config.json`，以及用户自己的 `providers.toml`） | `~/.zeroth` |
| `ZEROTH_PROVIDERS_FILE` | 一个**独立**存在的 provider 文件——只有它的 provider，没有内置的（`docs/providers/user-providers.md`） | 内置 + `{config_home}/providers.toml` |
| `ZEROTH_ENV_FILE` | `/login` 读写的 `.env` key 存储（以及轮换后的 ChatGPT refresh token 写回的地方——`docs/providers/chatgpt.md`） | `{config_home}/.env` |
| `ZEROTH_OPENAI_ISSUER` | ChatGPT Codex 的浏览器与 device-code 登录所对话的 auth 服务器——fork 自己的，或 smoke 套件的本地 stub（`docs/providers/chatgpt.md`） | `https://auth.openai.com` |
| `ZEROTH_CLINE_AUTH_BASE` / `ZEROTH_CLINE_API_BASE` | Cline 账户登录所对话的两个 host——WorkOS 和 Cline 自己的 API——fork 自己的，或 smoke 套件的本地 stub（`docs/providers/cline.md`） | `https://api.workos.com` / `https://api.cline.bot` |
| `OLLAMA_HOST` / `OLLAMA_HOST_API_KEY` / `OLLAMA_CONTEXT_LENGTH` | `ollama` provider 的 host（指向它就是配置它）、那台服务器的可选 bearer，以及镜像的服务器默认窗口（`docs/providers/ollama.md`） | 未设置 |
| `OLLAMA_API_KEY` | `ollama_cloud` 唯一的 key——两个 Ollama provider 读两个变量，所以配了一个不等于配了另一个（`docs/providers/ollama.md`） | 未设置 |
| `LMSTUDIO_HOST` / `LM_API_TOKEN` | `lmstudio` provider 的地址（光秃秃的 `localhost:1234` 会补成它的 `/v1`；指向它就是配置它，而 `/login` → Sign in with a local model 在连接测试通过后写入它），以及 LM Studio 服务器在被设成要求一个时想要的 token（`docs/providers/local-models.md`） | 未设置 |
| `ZEROTH_TEMPERATURE` | 采样温度 | provider/省略 |
| `ZEROTH_TOOLS` | falsy（`0`/`false`/`no`/`off`）禁用 `bash`/`read`/`write`/`edit` 工具（见 `docs/tools/tools.md`） | 工具开 |
| `ZEROTH_SYSTEM_PROMPT` | 覆盖「Zeroth」人格；空则不发 system prompt。任何非空 prompt 仍会折上运行时环境上下文（日期/os/用户/cwd，`docs/architecture/environment.md`） | `prompts/zeroth.md` 里的人格 |
| `SSL_CERT_FILE` / `ZEROTH_CA_FILE` | 给代理用的额外 CA bundle | 未设置 |

**dummy 是 fallback，从不是意外。** 真后端只在 `ZEROTH_DUMMY` 未设置
**且**一个 provider、一个 model、一个 API key 都解析得出时激活——对一个 `auth`
说明不需要 key 的 provider（本地 Ollama，`docs/providers/ollama.md`），key 是
可选的。否则应用使用 `DummyAi`。`smoke.sh` 一个都不设，所以它永远拿到 dummy
——罐头式答复、`dummy_model_name` 页脚，以及它的断言所依赖的脚本化 tool call
都未被动过。

### config home：`~/.zeroth`

应用把它的按用户状态放在一个 **config home** 下——`ZEROTH_CONFIG_DIR`，否则
`~/.zeroth`（与 `providers.toml` 和 sessions 目录已在用的同一个 base），否则当
没有 `HOME` 时为 `None`（此时持久化禁用，退回到 `./.env`）。它装两个文件，都
尽力而为地写（失败被吞掉，好让它永远杀不死 TUI），在首次写入时创建：

- **`.env`** —— `/login` 流程写入的 API key 存储
  （`tui::config::env_file_path`，可用 `ZEROTH_ENV_FILE` 覆盖）。被 git 忽略，
  所以 key 永不被提交。
- **`config.json`** —— `/model` 的选择（`llm::settings::Settings`）：`projects`
  下每个工作目录一个条目，顶层放着任何地方最后一次做出的选择
  （`{ "provider", "model", … }`——即按目录之前那个文件自己的形状）。你首次在
  其中启动的目录会取那次最后选择并钉成自己的，于是这里做的切换留在这里
  （`docs/state/per-directory-state.md`）。

它们旁边还坐着一个应用只读从不写的文件：**`providers.toml`**，用户自己的
provider，覆盖在附带的之上，并在 `/login` 或 `/model` 打开时重读
（`docs/providers/user-providers.md`）。

fork 之后的第一次启动，如果 fork 之前的 `~/.alter-zero` home 存在而 `~/.zeroth`
不存在，应用会把 **只有** `config.json`、`providers.toml` 和 `.env` 复制进新
home 一次（`tui::config::migrate_from_alter_zero`）。Sessions、历史、图片缓存、
skills、agents、looks 和 trust 留在原地，旧目录永不被修改——上一个应用继续从中
工作。显式的 `ZEROTH_CONFIG_DIR` 会禁用
迁移，因为那条路径由用户自己管。

### key 从哪来：`.env` 持久化

因为 `std::env::set_var` 是 `unsafe`（本 crate `forbid` unsafe），应用从不修改
进程环境。而是把 `.env` 文件在启动时加载一次进一个内存里的
`llm::keystore::EnvFile` map。key 解析（`tui::config::resolve_api_key`）先看
**真实的进程环境**（dotenv 的优先级——一个导出的 `OPENROUTER_API_KEY` 仍然
胜出），再退到 `.env` map，所以两种方式设的 key 都找得到。

`EnvFile` 是一个小的、**纯的、有测试的** `.env` 读写器：`parse` 读 `KEY=VALUE`
对（注释、空行、`export ` 前缀和引号都处理），而 `upsert` **就地**重写一个
key——其他每一行、包括注释，都保留——这正是 `/login` 流程写回的东西。

### 持久化模型：`config.json`

`llm::settings::Settings` 是纯的、有测试的 `config.json` 读写器（顶层
`{ "provider", "model", … }`——任何地方最后一次做出的选择，所有字段可选，所以
一个旧的或不完整的文件仍能加载——外加一个 `projects` map，每个工作目录一个
`ModelSelection`）。启动时，**该目录的**已保存 provider/model 为活动选择播种
（env 变量仍然胜出）：它自己的条目，或者——在那里首次启动时——那次最后选择，
当场钉成该目录自己的（`tui::config::adopt_selection`）。一次成功的 `/model`
切换后，边界把选择写回成这个目录的条目*和*新的最后选择
（`tui::config::save_selection`，一次读-改-写）。于是一个目录里选定的模型，就是
之后每次**在那个目录里**运行的默认——而如果它的 key 仍解析得出，真后端在启动时
自动激活。`docs/state/per-directory-state.md` 有完整规则以及两个文件为何不同。
这个选择还被记录第二次，在 session 自己的 rollout 里
（`docs/state/session-model.md`），这正是 `/resume`、`--resume` 和 `--continue`
所恢复的：一个恢复的对话在它当时所用的模型上运行，不管那时目录的条目怎么说，
而同一个目录里的两个实例可以各自保留自己的。

## 内联 `/model` picker

与 `/resume`（一个全屏 alt-screen overlay）不同，`/model` 是**内联**的：它原地
长高底部 live 区域，用自己的搜索 prompt 替换 composer——正是用户要求的那个形状。
它是 `App::model_picker: Option<ModelPicker>`（不是 `View`），像 Ctrl+R 搜索一样
在 `on_key` 里被拥有，并在 `ui::render_live` / `ui::live_height` /
`ui::cursor_position` 里被特判。

布局（picker 家族的那一页，`docs/ui/view-flow.md` 的 *A picker's page*——顶
框线、标题压在 dim 副标题上、通透的 codex 风格正文、然后底框线，两条框线都不贴
空行：用户 mock 的形状，`docs/ui/view-flow.md` 的 *The frame hugs its content*）：

```
────────────────────────────────────────────────
  Select model                                           (title, the pages' dress)
  Applies to this session and new ones in this directory (what a pick changes, dim)

  ❯ cla                                                  (search prompt, cyan '❯')

  a0_venice                                              (provider heading, bold + dim)
    claude-opus-4-8-fast ✓                               (active: ✓)
  openrouter
  → anthropic/claude-3-haiku                             (selected: '→' under the '❯')
    anthropic/claude-3.5-haiku
    (3/12)   ·   loading more…                           (position/total + load status, dim)

    Model Name: Anthropic: Claude 3.5 Haiku              (friendly name, dim)
────────────────────────────────────────────────
```

窗口容纳七行——模型加上任何 provider 标题（`MODEL_MENU_MAX_ROWS`），选中项骑在
它的中间；计数器、`Model Name:` 行和失败 provider 的理由挂在 id 之下
（`MODEL_DETAIL_INDENT`）。副标题说明 Enter 对 session 和对目录做了什么
（`docs/state/session-model.md`、`docs/state/per-directory-state.md`）。

- 由 `/model` 打开（`Action::OpenModelPicker`）。**回合中途可用**（2026-07-08
  更新；它以前像 `/resume` 一样被拒绝）：picker 只替换 composer，从不替换正在
  跑的回合——那在自己的线程上流式传输——而一次切换只重绑*下一个*回合的后端，
  所以没有可竞态的东西。切换确认是一个瞬时 toast（`Switched model to {id}` /
  `Can't switch to {id}: …`），不是一条提交的消息——回合中途的切换不得把流式
  答复劈开。见 `docs/ui/toast.md`。
- **流式条带停在它上面**（2026-08-08 更新）。「替换 composer」是字面意思：
  picker 的带框 body 占用 composer 的行，加上条带的和页脚的，*别的什么都不占*。
  运行中工具的 live 单元格（包括它流出的尾部）、`● Thinking…` 块、spinner 状态
  行、排队的后续回合和 toast 都保留它们在它**上面**的行。它以前占掉整个 live
  区域，恰恰遮住了用户在其旁边打开 picker 的那个回合——被报上来的 bug：在 agent
  生成时按 `/model`，会把状态指示器清空到 picker 关闭。修法与 ↓ manager 条带的
  相同——它先有同样的 bug（`docs/tools/background.md`）：`ui::layout` 的
  `strip_above_rows` 在 picker 自己的 `model_picker_rows` 之上保留条带，
  `view_split` 在两者之间切分区域（picker 是一个钉在底部的 `Length`，所以一个
  装不下两者的终端先挤压条带），而 `render_live_with_preview` 画的是 composer
  路径所画的同一个 `render_strip_above`。光标座位来自同一次切分，所以它绝不会
  漂离 `❯` 搜索行。picker 拥有每一个键的一个后果是：它打开时，运行中单元格那
  个延迟的 `(ctrl+b to run in background)` 提示被抑制
  （`App::background_hint_elapsed` 读到 `None`——权限 prompt 的规则），因为在
  picker 里面 Ctrl+B 到不了 runner。`/login` 流程与 `/settings` 菜单是同一个
  view 家族，行为一致（`docs/state/settings.md`）。
- **必须有一个已配置的 provider。** 如果没有任何 provider 有可解析的 key（或者，
  对一个本地服务器，一个地址——`docs/providers/local-models.md`），边界跳过
  获取，列表区显示一句青色
  `No provider set up yet — run /login to add one`（`ModelLoad::NeedsLogin`），
  而不是提供用户用不了的模型。
- 否则边界**并行获取每一个已配置的 provider**——各一个 worker 线程（图片粘贴
  的模式），都共用一个 cancel token——并在它们落地时把结果合并进一个列表
  （先是 `App::begin_model_load`，然后每条结果经一条专用通道走
  `App::add_models` / `App::add_model_error`）。picker 在**第一个** provider
  回答之前显示 `Loading models…`，然后是列表——随着其余的到来而增长并重排
  （先按 provider 再按 id；完全相同的 `(provider, id)` 重复被丢掉）。当列表
  跨多个 provider 时它是**分组的**：每个 provider 的行之前有一个粗体 provider
  标题，而当那个标题在同一个窗口里时，这些行上的 `[provider]` 标签被去掉
  （`ModelPicker::rows`）——一个矮终端的窗口可能从一个组的中间开始，那些行则
  保留标签，而不是让 provider 变成猜测。单 provider 的列表——以及一个收窄到
  一个 provider 的搜索——没有标题并保留标签，于是常见情形不变。计数器和
  `Model Name:` 行仍然只数模型：高亮从不坐在标题上。高亮在每次合并时都骑在
  同一个模型上，或者在其 provider 落地后重新落到活动模型上（除非用户已经移动/
  过滤过）。**一个在部分列表里什么都匹配不到的搜索会一直说
  `Loading models…`**，直到每个 provider 都回答：一个还在到达的列表无法证明某个
  查询没有匹配，而在一个仍在获取的 provider 之上显示 `No matching models`，
  是一个几秒后就被下一次合并推翻的断言（在 ChatGPT provider 落地前输入
  `chatgpt`，读起来就是「那个模型不存在」）。只有在 `pending == 0` 时，占位符
  才能如实落定为 `No matching models`。
- **部分结果被浮现，不是致命的。** 当 provider 仍在获取时，计数器带着一个 dim 的
  `· loading more…`；一个获取失败的 provider 被记录（`ModelPicker::errors`）并
  在计数器旁注明为红色的 `· {provider} unavailable` /
  `· N providers unavailable`，成功的列表原地不动。只有当**每一个** provider
  都失败（一个模型都没有）时，picker 才进入 `ModelLoad::Error`，每个各显示一行
  红色的 `{provider}: {reason}`。
- **获取是有内存界的**（`docs/architecture/memory.md` 有完整故事和实测数字；
  `examples/mem_probe.rs` 测它，`tests/model_parse_memory.rs` 守它）：
  `parse_models` 把每条记录持成一个借用的 `RawValue` 切片，并一次只解码**一条**
  记录的树（`entry_of`）——一次把整个目录物化，会花掉 body 的 ~10 倍堆块，而
  glibc 从不归还，那就是「`/model` 之后 RSS 再也降不下来」那个 bug——GET 骑在
  **chat 流自己的缓存客户端**上（`openai::NET_OP_TIMEOUT`，于是 session 只跑
  一个客户端/池，而 picker 的获取预热了下一个回合复用的那条连接），body 读取是
  **有上限的**（`MODELS_BODY_MAX_BYTES`，8 MiB——`SHELL_OUTPUT_MAX_BYTES` 的
  姿态），于是一个坏掉的端点胀不爆 worker。逐记录跳过全程不变：一条畸形的聚合
  行被丢掉，从不致命。
- 输入即过滤（对 `id`、provider 和名字做大小写不敏感的子串）、`↑/↓` 在两端
  回绕、PgUp/PgDn/Home/End 跳转、`Enter` 选中 →
  `Action::SelectModel { provider, id }`、`Esc` 清空查询再关闭、`Ctrl+C` 关闭。
  **bracketed paste** 也会扩展过滤器（`App::paste_into_model_filter`，从循环的
  `Event::Paste` 分支路由）：一个 model id 从 provider 的仪表盘复制过来，远比
  手打常见。它以前被整个吞掉——为了不让它落进 picker 底下的 composer 草稿而被
  丢弃，结果那个键什么都没做；路由给过滤器，既让那份草稿不受动，*又*让粘贴有
  用。连续空白被压成单个空格（复制来的 id 会拖一个尾换行），全空白的粘贴是
  空操作，像 `/login` 的 provider 步和 `/resume` 的搜索——但它**紧贴**追加，
  不加空格分隔，因为模型过滤器匹配的是一个 id：粘贴一个半打 id 的尾部必须补全
  那个 token，而不是开始一个新词。✓ 标记活动的 `(provider, id)`——边界通过
  `App::set_active_provider` 传入活动 provider，所以跨 provider 共享的 id 只
  标记真正在用的那一行。列表**让高亮保持居中**（`ui::centered_window`）：在长
  列表上选中项骑在中间行，好让它上面*和*下面的模型都在视野里，只有在那一侧
  到头（接近顶部/底部）时才滑向边缘。`/login` 的 provider 列表和 `/resume` 的
  session picker 也以同样方式滚动。
- 选中时，循环为新 provider/model 重建后端、更新页脚
  （`App::set_session_info`），并**把选择持久化到 `config.json`**（于是它就是
  下次运行的默认——在这个工作目录里，`docs/state/per-directory-state.md`），
  然后收起 picker。被选中条目的**推理能力**搭在选择上
  （`Action::SelectModel` 的 `reasoning`），为 Ctrl+T 的思考模式循环播种——
  而持久化的设置在选择旁边携带思考状态。见 `docs/providers/reasoning.md`。

- picker 打开在**一个压在 dim 副标题上的标题**（2026-11-06 更新；自旧的
  「Showing models…」横幅被去掉以来它一直无标题）——`Select model` 压在「一次
  选择改变什么」之上——然后一个空行和 `❯` 搜索行；选中标记是 `→`，在框内缩进
  处、`❯` 之下。
- 计数器 + `Model Name:` 行**只在真正高亮着一个模型时**显示。一个**占位符**
  状态（`Loading models…`、一个红色错误、`/login` 提示，或 `No matching
  models`）没有计数器也没有名字，于是那些行收起来，占位符把页面收在底框线
  之前——盒子贴着占位符，而不是留下一片空行（`model_has_detail`，在
  `model_view_lines` 构造器里分支——picker 现在是一个内容驱动的带框 body，高度
  就是构造出的行数，底部锚定绘制，被跳过的顶部流进 scrollback，像每一个带框
  view 一样；`docs/ui/view-flow.md`）。`/login` 的 provider 步遵循同样的规则：
  没有匹配的 provider 时它的计数器不占行，于是一个空行把
  `No matching providers` 带到提示上。
- 在一个装不下这一页的终端上（列出七个模型时需要十七行），
  `model_view_lines(picker, width, height)` 改为构造家族的**紧凑**页
  （`docs/ui/view-flow.md` 的 *A page that sheds rows before it flows*）：保留
  的 `↑ N more above` / `↓ N more below` 槽位之间三个模型，计数器和失败
  provider 的行让位，标题在装得下时留着（十二行），装不下则少一行。picker 没有
  提示行，所以紧凑页保留着高亮模型的 `Model Name:` 行，放在它的兄弟们保留提示
  的位置——标题走后就是同样的九行形状：

  ```
  ────────────────────────────────────────────────
    ❯

    → stub/model-00 [stub]
      stub/model-01 [stub]
      stub/model-02 [stub]
    ↓ 9 more below
      Model Name: Stub: Model 0
  ────────────────────────────────────────────────
  ```

  在那里把整页底部锚定，会把搜索行和高亮的模型流进 scrollback。在更矮的终端
  上，紧凑页一次一行地甩掉剩下的东西，永不甩掉它的搜索行
  （`docs/ui/view-flow.md` 的 *Below the compact page*）——`↑`/`↓` 那一对、框线、
  一路到高亮模型的行、名字行——而一个占位符页（加载中、错误、无匹配）把它的
  占位符放在列表的位置并同样甩掉，同时保留已输入的内容；光标的座位在构造出的
  页里找到搜索行（`view_prompt_row`）。`smoke.sh` Phase 139 在一个 12 行终端和
  一个 7 行终端上，于一个 stub provider 的十二个模型之上打开 picker。

### 样式

所有 picker 样式都集中在 `ui/theme.rs` 的 `MODEL_*` 常量里：`MODEL_TITLE` /
`MODEL_SUBTITLE` 抬头、青色的 `❯` prompt 与选中强调（复用
`menu_selected_color()`）、dim 的 provider 标签 / 计数器 / 模型名、`→` 标记和
详情悬挂所在的 `MODEL_DETAIL_INDENT`、`✓` 活动标记、青色的
`MODEL_LOGIN_HINT`，以及 `MODEL_MENU_MAX_ROWS`。改主题在那里改。

## 内联 `/login` onboarding 流程

`/login` 收集一个 provider 的 API key 并存进 `.env`，好让它跨运行持久——就是
用户要的那个「onboarding」。像 `/model` 一样它是**内联**的
（`App::key_onboarding: Option<KeyOnboarding>`，不是 `View`），原地替换 composer；
与它不同的是，它是一个多步流程，根在 `Select login method` 标题下的三种入口
——**Sign in with an account**、**Sign in with an API key**、
**Sign in with a local model**——它们的列表按 `auth` 切分 provider 表（一次
登录、一个粘贴的 key、一个可选的 key）。API-key 那一半见下文；订阅那一半有
自己的条目，而本地那一半——一个服务器的地址，**保存之前先测试**——是
`docs/providers/local-models.md`。

**第 1 步 —— 选一个 provider**（一个无标题的可过滤列表；`KeyStep::Provider`）：

```
────────────────────────────────────────────────
  ❯ agent                                                (filter, cyan '❯')

  → Agent Zero API · ✔ configured                        (selected '→'; green ✔ = a key resolves)
    OpenRouter · ◯ unconfigured                          (no key yet; the word stays dim)
    (1/2)                                                (position/total, dim)

  Keys are saved to ~/.zeroth/.env                   (dim hint — the real path)
────────────────────────────────────────────────
```

**第 2 步 —— 输入 key**（掩码；`KeyStep::Key`）：

```
────────────────────────────────────────────────
  Enter your OpenRouter API key                          (periwinkle #96a0d5, names the provider)

  One key for hundreds of models from every major        (the file's `description`, wrapped)
  lab — OpenAI, Anthropic, Google, Meta and more —
  billed from a single balance.
  Create a key at https://openrouter.ai/…/keys           (the file's `api_key_url`, an OSC 8 link)

  ❯ ••••••••••••••••••••                                 (masked field, cyan '❯')

  Enter to save · Esc to go back                         (dim hint)
────────────────────────────────────────────────
```

- 由 `/login` 打开（`Action::OpenKeyOnboarding`）。像 `/model` 一样**回合中途
  可用**（2026-07-08 更新）——而且像它一样，*只*替换 composer，把流式条带留在
  自己上面（2026-08-08 更新；见 picker 自己的那条）：保存 key 从不碰运行中的
  回合。边界构造 provider 行，好让 ✓ 反映真实的 env / `.env` key 解析
  （`provider_choices`），并注入 provider 步提示所点名的那个 `~` 相对 `.env`
  路径。保存确认是一个瞬时 toast
  （`Saved {ENV} — run /model to use {provider}`），不是一条提交的消息。见
  `docs/ui/toast.md`。
- **四个列表步**（method、subscription、provider、local）共用一个语法：输入即
  过滤（id/名字子串）、`↑/↓` 在两端回绕、PgUp/PgDn/Home/End 跳转、`Enter`
  激活高亮行（在 provider 步，它的下标是钉住的，所以过滤器没法把它从你脚下重排
  走）、`Esc` 清空过滤器再*退回*——method 步是根，改为关闭——而 `Ctrl+C` 从
  任何地方关闭。每次步进都重置共用的过滤器和选中项，于是一个列表永远以干净的
  查询从顶部打开。
- **每一行都说它是否已配置**——`{name} · ✔ configured`（当 key 或 token 已经
  解析得出）、`{name} · ◯ unconfigured`（当都不行）（`login_row`、
  `LOGIN_CONFIGURED` / `LOGIN_UNCONFIGURED`）。两半都是刻意写出来的：这一行
  以前在已配置时带一个绿 ✓，而在未配置时*什么都没有*，于是一个登录列表被打开
  要回答的那个问题，靠一个标记的缺席来回答。**只有 `✔` 上色**——`/model`
  picker 的 ✓ 所穿的那个绿，因为标记才是视线沿着一列名字搜寻的东西——而它旁边
  那个词、`◯`，以及它们前面的 ` · ` 保持 dim：状态是关于一行的**事实**，而不是
  一个警报，把整条尾巴上色让一个事实列表读成一列警报。method 根的三行完全没有
  状态（`status: None`）——它们是那个问题，不是可以被配置的东西。
- **没有列表带标题**（2026-09-03 更新）。subscription 和 provider 步以前会把
  打开它们的那个 method 行重复成一个青色标题（当时是 `Use a subscription` /
  `Use an API key`）；它没说任何行和下面提示没说的话，却让每一行多花两行——而
  那片区域已经在一个终端里跟一个运行中的回合共用。所有列表现在一个形状——框线，
  然后紧贴其下的 `❯` 过滤器。**根改为用文字问出它的问题**（2026-11-06 更新）：
  `Select login method` 压在它的三行上，没有 `❯` 过滤器也没有 `(n/total)`
  计数器——三行是一个问题，不是一个要搜的列表，所以在那里打字什么都不做，
  光标隐藏，这是登录方式选择的那条规则（`docs/providers/copilot.md`）。
- **其余标题穿各页唯一那套标题装扮**——粗体，浸在活动主题的横幅渐变里
  （`ui::header::page_title_line`，`docs/ui/theme.md`）——登录页的
  `Sign in to {provider}` 和 key 步的 `Enter your {provider} API key`，好让
  这个流程的标题与别的每一页读成一体。
- **订阅那一半**（`KeyStep::Subscription` → `KeyStep::Device`，对一个提供两种
  入口的行中间还夹着 `KeyStep::SigninMethod`）是一次 provider 登录，而不是一个
  要粘贴的秘密——GitHub Copilot 的 device code（`docs/providers/copilot.md`）、
  Cline 的 WorkOS device code（`docs/providers/cline.md`）、ChatGPT Codex 的
  浏览器 PKCE **或**它给无头机器的 device code（`docs/providers/chatgpt.md`
  ——在那一行按 Enter 会先打开一个带标题的两行选择，
  `Browser login (default)` / `Device code login (headless)`，在任何页面之前），
  或者 Anthropic 的 Console PKCE（`docs/providers/claude.md`），由该行的
  `SubscriptionChoice::kinds`（第一个是默认）点名它能打开哪些 `SigninKind` 页。
  它们全都终结在同一个 `.env` 存储里，在该 provider 自己的 `api_key_env` 之下，
  而这正是这个 fork 便宜的原因：`/model`、✓ 标记、能力探测和下一次启动都不需要
  第二个机制。两个浏览器流程是*同一页*——一个链接加一段等待——所以它们共用
  `SigninKind::BrowserLink`，只在常量上不同；而 ChatGPT 的 device code 落在
  Copilot 和 Cline 的同一页上。
- **key 步**：可打印键和 Backspace 编辑 key，一次 **bracketed paste**
  （`App::paste_into_key_onboarding`）在剥掉空白/换行后追加它（API key 总是
  粘贴进来的），`Enter` 保存非空的 key（`Action::SaveApiKey { provider,
  env_var, key }`）并关闭，`Esc` *退回* provider 列表，`Ctrl+C` 关闭。字段被
  掩成 `•` 字形——明文 key 从不触屏。prompt 点名 provider，并在其名字已经以
  「API」结尾时避免重复一个「API」（`login_key_prompt`）。
- **key 步介绍它索要的那个 provider**（2026-09-03 加入）：标题和字段之间坐着
  provider 文件自己的 `description` 和一行 `Create a key at {api_key_url}`
  （一个本地服务器的页面改读 `Install it from …`，因为它没有 key 可创建——
  `docs/providers/local-models.md`）。两者都是 dim，那个 URL 是一个真正的
  OSC 8 超链接，像这个流程显示的每一个别的 URL 一样（`docs/ui/links.md`），
  而这块**换行而不是裁剪**：一段停在词中间的描述什么都解释不了，一个截断的链接
  什么都打不开，而页面的高度就是它自己的行数，所以一个续行只花它自己
  （`docs/ui/view-flow.md`）。一个文件一个字都没描述的 provider，得到的正是它
  以前的那一页——这块整体省略，绝不留下一条空白行（`provider_about_lines` 压在
  共享的 `login_page` 块合并器上）。`description` 还指引 provider 列表的输入
  即搜，正如它已经在指引订阅列表的，所以 `claude` 找得到 **Anthropic**——而在
  本地列表上，`local` 找得到 **Ollama**。
- **`❯` 行是被找到的，不是被数出来的。** key 字段以前坐在一个固定行上，而一段
  终端自身宽度的描述一搬进来，那就不再成立，所以 `cursor_position` 从绘制刚
  构造出的那一页里把该行读回来（`view_prompt_row`，`menu_marker_seat` 的规则），
  而不是用一个可能跟它漂移的 `LOGIN_KEY_INPUT_ROW`。
- 四个列表步（method 根、订阅、provider、本地服务器）共用一个页面构造器，而在
  一个装不下它的终端上，它们构造家族的**紧凑**页
  （`docs/ui/view-flow.md`）：过滤器、保留的溢出槽位之间的三行和 key 提示，
  计数器和 `Keys are saved to` 注记让位。provider 列表在内置目录下是十五行，
  在十二行上把它底部锚定，会把过滤器和被高亮的 provider 流进 scrollback。根，
  没有过滤器，以同样的梯级甩掉，标题坐在过滤器的座位上——先甩它的空行和框线，
  于是七行仍装得下标题和全部三个入口。在更矮的终端上，紧凑页一次一行甩掉剩下
  的东西，永不甩掉它的过滤行（`docs/ui/view-flow.md` 的
  *Below the compact page*），一个什么都匹配不到的过滤器把它的占位符放在列表的
  位置。key 步、登录页和本地服务器页保持它们那一个形状——它们的字段或它们的
  等待坐在底部，锚定已经把那里留在屏上。
- 保存时，循环把 key `EnvFile::upsert` 进 `~/.zeroth/.env`（先创建 config
  home）、刷新它的内存副本（好让下一次 `/model` 的获取/切换立刻解析到它），并
  提交一条 `Saved {ENV} to {path} — run /model to use {provider}` 系统通知。它
  **不会**自动切换模型：用户用 `/model` 挑一个，那时它就能找到这个 key。

### 样式

`/login` 流程复用 `/model` picker 的颜色（缩进、青色 `❯` prompt / 选中、dim
元信息、绿色的 ✔/✓、`→` 标记），外加 `ui/theme.rs` 里的 `LOGIN_*` 字符串与
几何常量——根的 `LOGIN_METHOD_TITLE`、行的 `LOGIN_STATUS_SEP` /
`LOGIN_CONFIGURED` / `LOGIN_UNCONFIGURED`、key 步的
`LOGIN_KEY_URL_PREFIX` / `LOGIN_HOST_URL_PREFIX`，以及在 key 步点名 provider
的浅紫 `LOGIN_KEY_PROMPT_COLOR`（`#96a0d5`）。改主题在那里改。

## 已知限制（v1）

- ~~token 计数仍是应用侧的估算；OpenAI 流式协议最后的 `usage` 块没有通过
  `StreamEvent` 浮现。~~ **已修复**：每个请求都索要
  `stream_options.include_usage`，最后的 usage 帧搭
  `StreamEvent::Usage`，而 live 计数跳到 provider 自己的账（应用侧的估算仍在
  帧与帧之间走动）。请求还为 **prompt 缓存**做了塑造——需要处有显式的
  `cache_control` 断点、处处有缓存亲和键——见 `docs/providers/prompt-caching.md`。
- session 中途切换模型不会重写已经记录在 `/resume` session-meta 里的 `model`
  字段（它点名的是文件开始时所处的模型）。
- **tool calling 现在已实现**（曾是未来工作）：真后端把 `bash`/`read`/`write`/
  `edit` 提供给模型，并在一个 agentic 循环里跑它们。见
  **`docs/tools/tools.md`**。assistant 文本和推理仍像以前一样流式传输；`!` 本地
  shell 和 dummy 的脚本化工具不受影响。（已完成的工具在后续回合以
  provider 原生的 `tool_calls`/`tool` 格式重放给模型——即 live 循环所流出的
  同一协议——见 `docs/architecture/context.md`。）
- picker 在打开时获取模型（没有缓存）；一个慢 provider 会显示
  `Loading models…` 直到响应落地。
- **一条完全沉默的连接最多要花 `NET_OP_TIMEOUT`（120 s）才失败。** 每操作超时
  是一个停滞探测器，尺寸是按「慢但活着」的情形定的（一次视觉请求的多兆字节
  上传、一个卡在 header 上或 token 之间的 provider），于是一条安静下来却*没有
  死*的连接，只有在它走完才被判失败。中断从不在它上面阻塞：Esc/退出在 ~50 ms
  内被确认（排空的取消轮询），回合立刻结束，而脱离的 transport 线程自己收尾
  ——在下一次通道发送时，或者若停在那条沉默的 socket 上，至多一个操作超时之后。
  在任何内容流出之前的失败照常重试（`llm::retry`；根本连不上的连接改为等待，
  `docs/providers/offline.md`）；内容之后的失败直接浮现，从不重试（重试会重复
  已流出的文本）。
- `429` 的 `Retry-After`（以及 OpenRouter 的速率限制重置 header）不被读取；
  一次被限速的重试改为等标准指数退避。
- `ThinkingSplitter` 的内联标签路径和一个 provider 的*原生* `reasoning` 字段是
  独立处理的；一个同时混合了内联 `<think>` 标签**和**原生 reasoning 增量的
  补全，可能把一个缓冲的标签片段顺序搞错。没有真的 OpenAI 兼容 provider 在一个
  响应里两者都用，所以没有现实的触发条件；`flush()` 仍保证文本永不丢失，只在
  那种不可能的情形下被重排。
