# 把 Venice 当作 provider

Venice.ai 的模型——私有的、不审查的、开放权重的和前沿的——有两条路可到，
共用一个 wire（Chat Completions，默认的 `wire_api`）：

| provider id | 在哪里对话 | 从哪里列举 | 凭据 |
| --- | --- | --- | --- |
| `a0_venice`（**Agent Zero API**） | `https://api.agent-zero.ai/venice/v1`——Agent Zero 在 Venice 前面的代理 | `https://api.venice.ai/api/v1`——Venice 本身，`sk-a0-…` 的 key 对它也有效 | 一个 Agent Zero 的 key（`A0_VENICE_API_KEY`），对 A0T token 持有人带一份免费日配额 |
| `venice`（**Venice**） | `https://api.venice.ai/api/v1`——Venice 本身 | 同一个 base：它在哪列举就在哪对话 | 你自己 Venice 账户的 key（`VENICE_API_KEY`，在 [venice.ai/settings/api](https://venice.ai/settings/api) 生成） |

第二个 provider 存在，是因为两个 key 不可互换：代理收 Agent Zero 的 key，而一个
Venice 账户持有人手里是一个代理不接受的 Venice key。两个 provider *共用*的，是请求
构造器据以取键的一切——这正是直接那个只是一个 `providers.toml` 块、别的什么都没有
的原因。Agent Zero 那一行是应用的第一批 provider 之一；本文档就是这一对的共享形状被
写下来的地方，因为代理自己的行为此前只是从遇到它的那个特性的侧面被描述过
（`docs/providers/reasoning.md`、`docs/providers/prompt-caching.md`、
`docs/providers/llm.md`）。

## 一个 provider 文件说什么

```toml
[providers.venice]
name = "Venice"
description = "Venice.ai's private, uncensored models straight from Venice's own API — …"
api_key_url = "https://venice.ai/settings/api"

[providers.venice.kwargs]
api_base = "https://api.venice.ai/api/v1"

[providers.venice.kwargs.venice_parameters]
include_venice_system_prompt = false
```

块里没有一样东西是文件的新东西（`docs/providers/llm.md` 记录了每个键）。让它成为
Venice 的是 `venice_parameters` 表，而让它*直接*的是那个 base：

- **`api_base` 也是列举用的 base。** 代理需要一个单独的 `api_model_base`，因为
  Agent Zero 不提供 `/models`；直接的 provider 在同一个 host 上列举和对话，所以
  文件不点名第二个 base，而 `Provider::models_base` 退回对话 base。
- **`api_key_env` 是默认值** `VENICE_API_KEY`——即该 id 自己的
  `<ID_UPPERCASE>_API_KEY`，也正是 Venice 自己的示例所用的那个变量——所以
  `/login` 的保存和一个导出的变量无需文件说明就一致。
- **`venice_parameters.include_venice_system_prompt = false`**，经代理也一样：
  Venice 不被告知就会在开头加一段它自己的 system prompt，而这里人格就是整个
  system prompt。
- **不钉 `disable_thinking`。** 那张表的*存在*就是 payload 构造器所据以取键的
  Venice 家族标记：有一个思考模式活跃时，它把 `disable_thinking` 同步成
  `mode == Off`，即 Venice 在 `reasoning.enabled` 被忽略的地方所尊重的那个开关
  （`docs/providers/reasoning.md`）。一个测试钉住两个 provider 的 `extra_body()`
  相等，于是这个开关永远不会为一个同步而不为另一个同步。
- **没有 `extra_headers`。** 代理也不发；除了那个 key，没有身份要向 Venice 声明。

## 一个请求，两个目的地

`the_shipped_venice_provider_sends_the_proxys_request_to_venice_itself`
（`src/llm/openai.rs`）像边界那样经附带文件解析两个 provider，并断言两个请求 body
**逐字节相同**——只有 `OpenAiClient::endpoint` 不同。因此下面的一切对两者都成立：

- **思考模式**作为 `reasoning` 对象*以及*同步过的
  `venice_parameters.disable_thinking` 搭上去（`docs/providers/reasoning.md`）。
  Venice 今天同时记录 `reasoning_effort` 和一个 `reasoning` 对象；而
  `disable_thinking` 的同步是经代理的线上测试所显示真正能停下混合推理者的东西，
  所以它留着。
- **prompt 缓存是隐式的**（`docs/providers/prompt-caching.md`）：没有我们自己的
  `cache_control` 断点——Venice 为它光秃秃的 `claude-*` id 自己注入 Anthropic 的
  标记，而 `cache::needs_cache_breakpoints` 不碰那些 id。session 的
  `prompt_cache_key` 搭在 body 上（Venice 把它记作它的缓存亲和提示），而
  OpenRouter 的 `session_id` **不**搭——Venice 拒绝未知的 body 键
  （`Unrecognized key(s)`），所以那个钉住仍只在 OpenRouter 的 base 上生效。
- **`stream_options.include_usage`** 被尊重；最后一个 usage 帧的缓存细节按 Venice
  所用的两种拼法都读（`prompt_tokens_details.cached_tokens` 以及顶层的
  `cache_read_input_tokens` / `cache_creation_input_tokens` 别名——
  `drain_stream_reads_the_venice_style_usage_aliases`）。
- **目录**是 base 上的 `GET /models`，代理所发的同一个调用，而 picker 和页脚所要的
  每个能力都来自每条记录的 `model_spec`：`availableContextTokens` 作为上下文窗口、
  `capabilities.supportsVision`，以及 `capabilities.supportsReasoning`（+
  `supportsReasoningEffort` 对应 `low/medium/high` 档；没有它，模型是一个只有
  开/关的推理者）——`docs/providers/reasoning.md`、`docs/tools/tools.md` 的
  「Vision detection」、`docs/architecture/compact.md`。

因为代理从 Venice 自己的 base 列举，两个 key 都配好之后 `/model` 会把**同样的 id
显示两次**——分别打上 `[a0_venice]` 和 `[venice]` 的标签——而 ✓ 标记活动的
`(provider, id)` 对，于是在用的那一行永不含糊。

## `/login` 与在它上面启动

`/login` → **Sign in with an API key** → **Venice** → key 步用文件的描述介绍这个
provider 并链接 key 页面，Enter 把 key 存成 `~/.zeroth/.env` 里的
`VENICE_API_KEY`。Venice 发两种 key；一个 **Inference Only** key 就是应用所需的
全部（Admin 那种存在是为了以编程方式管理 key）。从环境上，
`ZEROTH_PROVIDER=venice` 加上导出的 `VENICE_API_KEY` 直接就在它上面启动
（`ZEROTH_MODEL` 点名模型，否则由 `/model` 挑一个）。

其他每个 Venice 旋钮都搭同一张表，且不需要附带文件的副本：用户自己的
`~/.zeroth/providers.toml` 里一个块会逐键地补丁附带的那个
（`docs/providers/user-providers.md`），于是 `enable_web_search = "auto"`、
`strip_thinking_response = true`，或
`[providers.venice.kwargs.venice_parameters]` 下的一个 `character_slug`，会落在
附带的 `include_venice_system_prompt = false` 旁边，而那张表原样往返进 body——
模式的 `disable_thinking` 同步保留其他键。

## 测试

- **单元** —— `the_builtin_file_ships_venice_direct_beside_the_agent_zero_proxy`
  和 `a_venice_selection_resolves_to_venice_itself_and_needs_its_key`
  （`src/llm/config.rs`）把块的每个字段对着代理的钉住，而上面那个 payload 的孪生
  测试钉住请求。
- **Smoke** —— Phase 103（`scripts/smoke/phases/103-loginfork.sh`）走
  `/login` 的 API-key 列表，要求 **Venice** 行在 **Agent Zero API** 旁边，两者在
  套件清洗过的 key 存储下都读 `◯ unconfigured`。
- **Live** —— 选择性加入，`cargo test` 从不跑：

  ```sh
  VENICE_API_KEY=… cargo test --test live_caching -- --ignored --nocapture live_venice_direct
  ZEROTH_LIVE_PROVIDER=venice VENICE_API_KEY=… ZEROTH_LIVE_MODEL=llama-3.3-70b scripts/live_smoke.sh
  ```

  `live_venice_direct_second_turn_reads_the_prefix_from_cache` 是把代理的缓存测试
  指向直接 provider。**它没有从这里跑过**——加这个 provider 时没有 Venice key——
  所以直接 base 是靠构造验证的（同一个请求，代理的测试在 Venice 自己的缓存上证明过，
  `docs/providers/prompt-caching.md` 的表），以及靠 Venice 公布的 API 参考，而不是
  靠它自己的一个回合。一拿到 key，第一个要跑的就是这个测试。

## 已知限制

- 列举不发 `type` 查询，正是代理的做法；Venice 的 `/models` 在那个参数后面还提供
  图片、音频和 embedding 记录，而如果它的默认值哪天变宽，picker 会为两个 provider
  一样地显示它们——每条记录的过滤器（`models::is_callable`）就是 `type` 检查该放的
  地方。
- Venice 从账户自己的余额里计 API 用量；应用不读余额也不读配额，于是一个用尽的账户
  作为请求的 HTTP 错误出现在回合的红色通知里，像任何 provider 一样。
