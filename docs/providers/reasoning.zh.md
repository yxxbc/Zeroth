# Ctrl+T 思考模式（推理力度）

> 这一篇讲的是**向模型要求什么**。而*回来*的东西——思维链本身，实时流出并折叠成一个
> `Thought for 1m 5s · 1.5k tokens` 单元格——是
> `docs/ui/thinking-stream.md`。

能推理的模型（「思考」模型）接受一个**推理力度**——回答之前花多少思维链。本文档讲
TUI 如何按模型探测这个能力、切换模式的 **Ctrl+T** 循环、模式显示在哪里（页脚，模型名
旁边）、它如何搭在请求上，以及它如何持久化。

```
  ❯ ask something…
────────────────────────────────────────────────
  qwen3-5-35b-a3b medium · ~/zeroth            (footer: {model} {mode} · {cwd})
```

按 Ctrl+T 循环 `off → low → medium → high → off …`（模型提供什么就循环什么），弹一个
瞬时 `Thinking: high` toast（`docs/ui/toast.md`），而下一个请求携带新的力度。在一个
完全不能推理的模型上，Ctrl+T 改为弹 `{model} does not support thinking`——这个键
永不悄悄死掉。（这个循环曾坐在 Shift+Tab 上，直到那次终端快捷键的工作，
`docs/input/textarea.md`——现在 Shift+Tab 循环的是*权限*模式，
`docs/state/permissions.md`。）一个推理模型的默认是 **medium**（在提供它的地方）。

## 探测：`/v1/models` 说了什么

支持由纯的 `llm::models::reasoning_support_of` 按记录解析成
[`llm::reasoning::ReasoningSupport`]——`efforts`（可接受的档位，按规范顺序）、
`can_disable`、`default_effort`。下面每种形状都**按记录**嗅探，于是一个用其中任一种
的自定义 OpenAI 兼容 provider 不用配置就能用：

- **OpenRouter** —— 记录上的一个 `reasoning` 对象：
  `{"mandatory": bool, "supported_efforts": ["low", …] | null,
  "default_effort": "medium"}`。`supported_efforts` 为 null 或缺失，意味着「全都
  接受」（提供默认的 `low/medium/high` 档）；提供 `"none"` 映射成 Off 开关，而不是
  档位的一级；`mandatory: true` 去掉 Off（例如 kimi-k3，它唯一的模式是 `max`）。
  没有这个对象、但 `supported_parameters` 里有 `"reasoning"`/`"reasoning_effort"`
  的记录也算。
- **Venice / Agent Zero** —— `model_spec.capabilities.supportsReasoning`
  （+ `supportsReasoningEffort`）。有力支持时提供默认档位；没有时这个模型是一个
  **只有开/关的推理者**（`efforts` 为空）——它按自己的默认思考，而循环只是
  `off ↔ on`。
- **一个网关的扁平能力**（OmniRoute）——
  `capabilities.effort_tiers` 作为档位，无论旁边是否立着一个
  `capabilities.thinking` 布尔都读它：网关只为它的能力来源认识的模型写那个布尔，
  而对其余的只列出发现过程或它自己的运行时学习找到的档位。`thinking:
  false` 是一个答案，并压过网关留在它旁边的清单。这个网关把 `"none"` 算在它的档位
  里，所以 `Off` 只在清单点名它时才提供。一张不点名任何档位的清单，或者根本没有
  清单（网关内置的 Codex 行），是网关在说这个档位不可调，所以 Ctrl+T 在那里什么都
  不提供——连 `Off` 都不提供，因为网关会把 `Off` 当作力度 `none` 原样透传给上游、
  不做检查（`docs/providers/user-providers.md`）。

Copilot、ChatGPT backend 和 Anthropic 发布它们自己的档位，按它们页面所描述的读
（`docs/providers/copilot.md`、`docs/providers/chatgpt.md`、
`docs/providers/claude.md`）。一个不公布上述任何形状的模型没有思考：没有页脚模式，
而 Ctrl+T 会解释。

## 模式、循环与 wire

[`llm::reasoning::ThinkingMode`] 是 `Off`、`On`，或 `Effort(level)`，规范档位是
`minimal → low → medium → high → xhigh → max → ultra`
（`ReasoningEffort`；`ultra` 只有 ChatGPT backend 最新的模型点名
——`docs/providers/chatgpt.md`——而且像每一级一样，只在某个模型自己的记录列出它时才
提供）。`ReasoningSupport::modes()` 构造这个循环——`Off`（可禁用时）然后模型的各档
力度，或者对一个无力度的推理者给 `[Off, On]`——而 `next_mode` 步进它并回绕；一个
模型不再提供的陈旧持久化模式会步进到 `default_mode()`（medium → provider 报告的
默认 → 档位的中间 → `On`）。

在 wire 上（`openai::build_payload`，在 provider 的 `extra_body` 合并**之后**应用，
好让用户的选择压过文件里配的静态值）：

| 模式 | 请求 | 当 `reasoning_param = "reasoning_effort"` |
| --- | --- | --- |
| `Effort(e)` | `"reasoning": {"effort": "e"}` | `"reasoning_effort": "e"` |
| `Off` | `"reasoning": {"enabled": false}` | `"reasoning_effort": "none"` |
| `On` | 什么都不发——模型自己的默认 | 什么都不发 |

这个对象是 OpenRouter 的，也是每个附带的 chat-completions provider 所读的。一个改为
读 OpenAI 自己的顶层 `reasoning_effort` 字符串的服务器——OmniRoute 通往 Claude 模型
的路由，它忽略那个对象——由它 provider 块里的 `reasoning_param` 点名
（`llm::config::ReasoningParam`、`docs/providers/user-providers.md`），而力度随后
只作为那个字符串搭上去，绝不与对象并排。

**Venice 的怪癖：** 线上测试显示 Venice（经 Agent Zero 代理）*忽略*
`reasoning.enabled`——它的混合推理者继续思考。它尊重的开关是
`venice_parameters.disable_thinking`。所以当 payload 携带一个 `venice_parameters`
表时（Venice 家族的标记——内置的 `a0_venice` 和 `venice` provider 总有一个，为了
`include_venice_system_prompt`；`docs/providers/venice.md`），构造器把
`disable_thinking` 同步成 `mode == Off`，保留那个表的其他键；没有那个表的 provider
（OpenRouter）保持一份干净的 payload。`providers.toml` 不再静态地钉住
`disable_thinking = true`——模式拥有它（以前每个 Agent Zero 模型的思考都被强制
禁用）。

## 按键处理

Ctrl+T 在每个终端里都以 `Char('t')`+`CONTROL` 到达——没有协议分裂要搭桥
（Shift+Tab，旧的绑定，需要同时绑上 `BackTab` 和 `Tab`+`SHIFT`；那一对现在驱动
权限模式的循环，`docs/state/permissions.md`）。循环**回合中途**可用——像 `/model`
一样，它只重绑*下一个*回合的后端，从不绑正在流式传输的那个。当 `/model` picker、
`/login` 流程或 Ctrl+R 搜索拥有按键时，Ctrl+T 是惰性的；`/resume` picker 保留它
自己那套 Tab/BackTab 工具栏绑定。`?` 快捷键带列出
`ctrl+t to cycle thinking`（`docs/ui/shortcuts.md`）。

纯的 `App::cycle_thinking` 推进 `App::thinking`
（`ThinkingState { support, mode }`）并返回 `Action::SetThinking(mode)`；循环用带该
模式的 `ModelConfig` 重建后端、持久化，并显示 `Thinking: {mode}` toast（由循环显示，
好让边界的过期计时器武装起来——`docs/ui/toast.md`）。

## 支持状态从哪来

- **`/model` 切换** —— 被选中的 `ModelEntry` 已经携带 `reasoning`（从列出它的那次
  `/v1/models` 获取中解析），搭在 `Action::SelectModel` 上，于是一次成功的切换用
  模型自己的默认模式播种这个循环，不必重新获取。
- **启动，已保存的选择** —— `config.json` 在已保存的 provider/model 旁边持久化一个
  `thinking` blob（`llm::settings::ThinkingSettings`：模式 + efforts +
  `can_disable`，或者一个已知非推理者的 `supported: false` 标记）——即工作目录自己
  的条目（`docs/state/per-directory-state.md`）——所以启动直接从文件播种
  `App::set_thinking`。
- **启动，支持未知** —— 一个真后端的已保存设置不带 blob（一个由 env 选的模型，或
  这个特性以来的第一次运行）会起一次性的后台**探测**：在一个 worker 线程上为活动
  provider 做 `fetch_models`（一条专用通道——`/model` picker 的并行获取搞不混它），
  从结果里读出活动模型的记录。探测播种状态，重绑后端好让默认模式搭在下一个请求上，
  并持久化——但**只写到已记录的选上**：一个被 env 覆盖的模型永不写 `config.json`
  （env 总是胜出，从不粘住），所以那个组合下次运行就重新探测。一次失败的探测让支持
  静默地保持未知（后台的记录工作，不是用户动作）。同一个探测（以及同一个持久化 blob
  的兄弟 `vision` 字段）现在还从记录里读出模型的**图片输入**支持——
  `docs/tools/tools.md` 的「Vision detection」那道优雅的图片附接闸——在*任一*能力
  未知时触发。

`/clear` 和 `/resume` 从不碰这个状态（模型没变）；切换到一个不推理的模型会清掉它
（`App::set_thinking(None)`），从而清空页脚的模式。

## 测试

模式/支持/循环的逻辑、模型记录的解析（两种 provider 形状）、payload 映射（包括
Venice 的 `disable_thinking` 同步）、设置的往返（包括陈旧 blob 的降级）、Ctrl+T 的
语法（回合中途、picker 惰性、不支持时的 toast），以及页脚渲染，都在各自的模块里有
单元测试。边界的接线（探测、持久化、后端重绑）对着真实 provider 演练——
`tests/live_openrouter.rs` 里被 ignore 的 live 测试，外加手工的 tmux 运行
（`docs/providers/llm.md` 的测试注记）。

## 已知限制（v1）

- Venice 不列举按模型的力度清单（只有布尔），所以有能力力度的 Venice 模型拿到安全的
  `low/medium/high` 档；`minimal`/`xhigh`/`max` 在那里不提供，即便某个模型可能接受
  它们。OpenRouter 的模型用它们精确公布的清单。
- 探测只覆盖启动时的模型；一个在飞行中经 `ZEROTH_MODEL` 切到的模型（不可能
  ——env 只读一次）不需要更多。
- `ThinkingMode::On` 什么都不发，所以一个无力度的推理者的实际思考取决于模型自己的
  默认（Venice 的 Claude 模型默认就思考；`off` 是通过 `disable_thinking` 在那里
  可靠的杠杆）。
