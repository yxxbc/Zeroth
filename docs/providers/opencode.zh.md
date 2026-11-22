# 把 OpenCode Go 当作 provider

OpenCode Go 是 OpenCode 自己的订阅——每月 $10，换一套精选的开源编码模型，由官方
CLI 所走的同一座网关提供服务。Zeroth 把它做成一个 provider，进入方式与 OpenCode
自己的 `/connect` 一致：**粘贴一个 API key**，不是 OAuth 登录。

| provider id | 怎么认证 | 存什么 | `/model` 列什么 |
| --- | --- | --- | --- |
| `opencode_go` | 从 [Go 页面](https://opencode.ai/console/go)拿到的 API key | 那个 key，逐字 | Go 自己的 `/models`：DeepSeek、Kimi、GLM、Qwen、MiniMax 以及套餐里的其余模型 |

key 从 `OPENCODE_API_KEY` 读取——官方 CLI 用的也是这个变量——所以导出的 key 不用
`/login` 就能用，而在应用里保存的那一个会落到 `~/.zeroth/.env` 的同名下。免费的
OpenCode 端点（`/inference/openai/v1`）是另一套后端，不属于这个 provider。

## Go 要求的两个 header

只有当请求表明了自己的身份，Go 才表现得像任何别的 OpenAI 兼容端点。它的
[文档](https://opencode.ai/v2/docs/console/go/#where-can-i-use-it)要求每个 harness：

1. 发送典型的编码 agent 流量；
2. 用 `User-Agent` 报出自己的名字（`my-coding-agent/1.0`），而不是一个通用的 SDK
   或 HTTP 库字符串；
3. 在每个对话里用一个稳定的 id 放在 `x-opencode-session` 里，"这样我们才能优化
   路由和 prompt 缓存"。

两者都是承重的，而且弄错的失败方式不是回得慢，而是根本没有回复——已实测：

- 通用的 `User-Agent` 会被边缘回以 **`403 error code: 1010`**；
- 有名字的 `User-Agent` 但没有 `x-opencode-session` 的请求会被回以 **`400 MissingSessionID`**：
  *"Request is missing x-opencode-session and cannot be routed efficiently."*

Zeroth 在 `src/llm/opencode.rs` 里把两者都加上，对任何 `/inference/go/` 下的 base
生效，并且聊天请求和 `/models` 列表都会带：

- `User-Agent: zeroth/<version>`——用 crate 自己的版本号，名字在每次发版后都是
  诚实的。provider 块里自己写的 `User-Agent` 优先，所以客户端身份仍由用户决定。
- `x-opencode-session: <per-session cache key>`——与请求体里的 `prompt_cache_key` 是
  同一个 key（`tui::config::session_cache_key`），进程活多久就稳定多久，因而对话
  的每一轮都稳定。没有 key 就不发这个 header，绝不发一个空值。

**这就是速度的来源。** 在 Go 前面放一个本地路由器（Magpie 之类）是多一跳，而其中
会改写或遮罩 prompt 的那种每请求都在做真实的工作；直连走的正是 OpenCode 自己客户端
走的那条路。session header 还是 Go 把对话放到它 prompt 缓存所在地的依据——与每个
别的 provider 都发 `prompt_cache_key` 是同一个道理
（`docs/providers/prompt-caching.md`）。

## 一个 provider 文件说什么

```toml
[providers.opencode_go]
name = "OpenCode Go"
api_key_env = "OPENCODE_API_KEY"                              # ← the official CLI's variable
api_key_url = "https://opencode.ai/console/go"                # ← where `/login` says keys are made
api_model_base = "https://opencode.ai/inference/go/v1"        # ← the richer catalog
reasoning_param = "reasoning_effort"                          # ← Go reads OpenAI's top-level string
description = "OpenCode's $10/month subscription …"

[providers.opencode_go.kwargs]
api_base = "https://opencode.ai/inference/go/openai/v1"       # ← Chat Completions
```

聊天 base 就是 OpenCode 自己目录为 Go 的 `chat-completions` API 命名的那个。**列表**
是另一条路径：Go 从 `https://opencode.ai/inference/go/v1/models` 发布每个模型的窗口、
模态和档位，所以 `api_model_base` 点名它，而不是回退到聊天 base。上面那两个 header
是**路径**的属性，不是块的属性，所以把 base 换到同一路径下的代理上的用户仍然会带上
它们。

`reasoning_param = "reasoning_effort"` 是官方客户端所发的：OpenAI 兼容 SDK 把模型的
档位写成顶层字符串，而不是 OpenRouter 的 `{"reasoning": …}` 对象。因此 Ctrl+T 循环
列表报告的档位（DeepSeek V4.1 Flash 是 `low → high → max`），Off 则省略该字段。

## 每个模型能做什么

列表是 models.dev 的记录形状，`llm::models` 按那个形状读取：

| 记录字段 | 它给出什么 |
| --- | --- |
| `limit.context` | 页脚对照的窗口，以及自动压缩的 90% |
| `modalities.input` | 图片输入，以 MIME 类型拼写（`image/jpeg`）；只有 `text` 就是不能看图 |
| `capabilities.reasoning.supported` + `effort` | Ctrl+T 的档位，正是模型接受的级别 |

什么都没说的记录得到旧行为（没有表、不强制降级、没有档位），而不是一个猜测。
免费与付费的划分是 Go 要改的：换套餐或让一个模型退役会改变列表，picker 跟着它走，
没有一份会过时的随包副本。

## 限额

Go 的付费模型共用一个额度，按滚动的 5 小时、每周和每月三个窗口计量。到达限额时网关
回 `429` 和一个 `GoUsageLimitError`；Zeroth 显示服务端自己的话，且不重试它（重试
补不回窗口）。建议是 Go 自己的：换一个免费模型，或者在 Go 页面打开 Extra Usage。
`/usage` 读的是 Zeroth 拿这个 key 去问的同一个量表——`GET
https://opencode.ai/inference/go/v1/usage`，每个窗口一条，状态说用尽时那一行写
`limit reached`（`docs/providers/usage.md`）。每个窗口用了多少也在
[Go 页面](https://opencode.ai/console/go#limits)；一次被拒的读取会说明是 key 本身
被拒，还是它背后的 workspace 没有生效中的订阅。
