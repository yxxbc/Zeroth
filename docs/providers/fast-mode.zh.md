# `/fast`、`/ultrafast`、…… —— codex 的速度档，做成命令

> 这里讲的是 ChatGPT 模型回答得**多快**。它在回答之前*想*多少 —— 也就是 Ctrl+T
> 的推理强度 —— 是 `docs/providers/reasoning.md`，而整件事所骑的登录与线上格式
> 是 `docs/providers/chatgpt.md`。

codex 的 **fast mode** 是请求上的一个*服务档*：一个在 ChatGPT 列表里点名了
`priority` 档的模型，可以被要求优先处理，后端以快 1.5–2× 的速度伺候它，代价是方
案的用量上升。本文讲 TUI 怎么得知一个模型提供哪些档、切换它们的**逐档命令**（列
表点名几档就有几个命令，`/fast`、`/ultrafast`，记录怎么写就怎么来，绝不是硬编码
的一行）、这个选择显示在哪里（页脚，在思考模式之后）、它怎么骑在请求上，以及它怎
么持久化。

```
  ❯ ask something…
────────────────────────────────────────────────
  gpt-5.5 medium fast · ~/zeroth                (footer: {model} {mode} {tier} · {cwd})
```

当前模型的列表点名了多少个速度档，就有多少个自己的斜杠命令，以档命名，紧跟在
`/model` 之后列出。这个账号今天的目录列了一档 `Fast`，所以 palette 显示
`/fast`；一条也列了 `Ultrafast` 的记录会在它旁边显示 `/ultrafast`，而后端明天加
的档，在它被列出的那一天就是一条命令。运行某一档的命令就会选中它 —— 一条短暂的
`Speed: fast — 1.5x speed, increased usage` toast（`docs/ui/toast.md`），重复记
录自己那份代价说明，而下一个请求就带上这一档；再运行同一条命令会回答 `Speed:
standard`，页脚里那个词随之消失；另一档的命令直接切过去，中间不经过 standard。一
个一档都不列的模型 —— 今天除 ChatGPT 后端之外的每一个供应商，以及离线 dummy ——
不列任何档命令：在那里敲 `/fast` 匹配不到任何东西（`No matching commands`），而
Enter 像任何其他没匹配上一样被吞掉，而不是当作一条消息发给模型。对每个模型，默认
都是 **standard**；没人要，就一点都不按档的费率计费。

## codex 是怎么做的（调研结论）

从 `openai/codex`（`codex-rs`）里读出来的，也就是这次移植的参考实现：

- **线上格式。** `ResponsesApiRequest.service_tier: Option<String>` —— fast mode
  时是 `Some("priority")`，standard 时省略
  （`codex-api/src/common.rs`；`protocol::config_types::ServiceTier::Fast` 的
  `request_value() == "priority"`）。在它旁边，客户端对发往 ChatGPT 后端的每个请
  求都带一个 `x-codex-routing-hint: model={model};tier={tier}` 头 —— 没选档时只
  有 `model={model}`（`core/src/client.rs`，`build_routing_hint_header`）。
- **闸门。** 一个模型支持 fast mode，是指它的 `/models` 记录里 `service_tiers`
  数组带着一条 `id: "priority"` 的条目（`{"id": "priority", "name": "Fast",
  "description": "1.5x speed, increased usage"}`），或者 —— 遗留的标记 ——
  `additional_speed_tiers` 含 `"fast"`
  （`protocol/src/openai_models.rs`，`ModelPreset::supports_fast_mode`）。
  有些记录会列第二个档 `ultrafast`。`service_tier_for_request` 会丢掉模型没列的
  档，于是不支持的档永远到不了线上。
- **UI。** 每个列出的档变成一条以此命名的斜杠命令 —— `/fast`、`/ultrafast` —— 插
  在 `/model` 之后，只在当前模型列出它时显示
  （`tui/src/bottom_pane/slash_commands.rs`）。运行一条就是切换那一档
  （`chatwidget/service_tiers.rs`，`toggle_service_tier_from_ui`）：关着就打开，
  开着就回到显式的 `default`。session 横幅把 `fast` 以品红画在模型名之后
  （`history_cell/session.rs`），`/status` 按速度报告花费。有一个
  `toggle_fast_mode` 按键绑定，**没有默认键**。
- **持久化。** 选择被写进 `config.toml` 的 `service_tier = "fast"`（请求里的拼法
  `priority` 也接受），并对每个支持它的模型生效；不支持的模型就是跑 standard，配
  置的值原封不动。
- **子进程。** 一个被启动的 agent 只在它自己的模型列出那一档时才继承根进程的档
  （`agent/child_config.rs`）；Guardian 的审查请求永远发 none。

## 我们建的是什么

同一件事，架在这个 crate 已有的各处接缝上 —— 其中每一处 Ctrl+T 的思考模式都已经
建好了，所以 fast mode 在每一处都是第二个乘客，而不是一套自己的机制：

| | 思考模式（`docs/providers/reasoning.md`） | 速度档 |
| --- | --- | --- |
| 从哪探测 | `/models` 记录的 effort 阶梯 | 记录的 `service_tiers` |
| 纯状态 | `App::thinking` 上的 `ReasoningSupport` + `ThinkingMode` | `App::speed` 上的 `SpeedState` |
| 由谁切换 | Ctrl+T（`Action::SetThinking`） | 每个列出档自己的命令（`Action::SetSpeed`） |
| 怎么骑在请求上 | `reasoning: {effort}` | `service_tier: "priority"` |
| 显示在 | 模型名之后 | 思考模式之后 |
| 持久化到 | `config.json` 的 `thinking` blob | `config.json` 的 `speed` blob |
| 重新接上 | *下一个*回合的 backend | *下一个*回合的 backend |

### 探测：列表怎么说

`llm::models::service_tiers_of` 把一条记录读成 `ModelEntry` 上的
`Vec<ServiceTier>` —— id、名字与描述，按记录自己的顺序 —— 与推理、视觉、上下文窗
口那几个字段用的是同一套逐记录嗅探，只是这个分支只有 ChatGPT 后端的记录会走。没
有 `service_tiers` 列表时，遗留的 `additional_speed_tiers: ["fast"]` 标记仍然读
作 fast 档（`ServiceTier::fast()`，没有描述）。其他每个供应商的记录什么都不列，
读作一个**空**列表：列表就是它列出的东西，所以与视觉不同，这里没有「未知」要携带
—— 空列表的模型就是没有 fast mode，如此而已。

### 状态与切换

`llm::service_tier` 是纯核心：

- `ServiceTier { id, name, description }` —— codex 的 `ModelServiceTier`，逐字照
  搬。`is_fast()` 对线上 id `priority` 或名字 `fast` 为真；`label()` 是转小写的
  名字（`fast`、`ultrafast`），绝不是裸 id，因为对一个按了 `/fast` 的用户来说
  `priority` 什么也没说；`command_name()` 是那个标签作为**一个 palette token**
  —— codex 自己的规则是 `tier.name.to_lowercase()`，而一个裸的 `/token` 到空白就
  结束（`app::command_query`），所以某个目录将来可能带上的名字（`Super Fast`）会
  被 slug 化：除字母、数字、`-` 与 `_` 之外的都折成 `-`，连续的合并，两端修剪
  （`super-fast`），而一个什么都不剩的名字根本不成命令。
- `SpeedState { tiers, tier }` —— 列出的各档与选中那档的 id（`None` =
  standard）。`SpeedState::new(tiers, tier)` 对空列表是 `None`（没有东西可选），
  并丢掉列表不提供的 `tier` —— 一个模型不再列出的已保存选择降级为 standard，而不
  是骑上一个会被拒的请求，这是 codex 的 `service_tier_for_request` 规则。
  `toggle(id)` 就是某一档的命令所跑的东西 —— codex 的
  `toggle_service_tier_from_ui`：当 `id` 就是当前选择时切到 **standard**，否则切
  到 `id` 那一档；而一个模型没列的 `id` 什么都不改，按现状返回当前选择（是个全函
  数而不是 panic；palette 也只会提供列出来的档）。新的选择被返回，交给循环去重新
  接上、持久化并 toast。

于是 standard 的 session 上敲 `/fast` 就是 fast，再敲 `/fast` 就是 standard，而
在 fast 的 session 上敲 `/ultrafast` 立刻就是 ultrafast —— 每条命令是自己的开
关，而不是一个轮转上的一站，正是这一点让页脚无论命令以什么顺序被敲，都能诚实地说
明下一个请求带的是什么。

### 命令

静态注册表里没有 `/fast`。`App::commands()` 为这个 session 构建 palette 的各
行：`COMMANDS`（内置的，一个 `const`），并在**紧挨 `/model` 之后**插入
`App::speed` 里每一档一行 —— 也就是 codex 的 `commands_for_input` 插入它的
`SlashCommandItem::ServiceTier` 条目的地方，因为档是关于刚刚切过去的那个模型的事
实。每一行是 `SlashCommand::tier(&tier)`：由 `command_name()` 命名，由记录自己的
代价说明描述（`1.5x speed, increased usage` —— 速度的价钱就说在买它的地方，那是
用户看不见的那一半），记录没给描述时则是 `Toggle {label} mode`，并把这一档作为自
己的效果携带（`CommandEffect::ServiceTier(tier)`）—— 这是唯一一个没有注册表条目
携带的效果，因为这些行来自列表。一个名字留不下 token、或者与某条已列出命令重名的
档，不给行：内置的永远不被遮蔽，`/model` 是通往其他一切的门。这些行是按需构建而
不是存起来的，于是它们绝不会与页脚和下一个请求所读的状态打架；内置条目的克隆是一
次借来的指针拷贝（`SlashCommand` 的字符串是 `Cow`，正是这一点让一个类型既能服务
一张 `const` 表、又能服务一行逐 session 的行），所以一次按键的重建不花任何代价。

下游一切都读这个列表：`matching_commands(&commands, query)` 是 palette、↑/↓ 与
Enter 共用的过滤器，而 palette 的条带渲染一行档与渲染一条内置的完全一样
（`ui::command_menu_lines`），行下面的 `(n/total)` 计数器也把它当任何别的来数。
运行一条就是 `App::toggle_speed_tier`：纯状态立刻切换，循环被交给
`Action::SetSpeed(Option<ServiceTier>)`。它在**回合中途**的运作与 Ctrl+T 完全一
样 —— 纯状态现在就动，只有*下一个*回合的 backend 被重新接上（正在跑的回合在自己
的线程上流式输出，不受影响）。

循环的 `Session::set_speed` 做 Ctrl+T 的 `set_thinking` 所做的那三件
事：`ModelSession::rebind_speed` 带着档重建 backend 的 `ModelConfig`，`persist`
把这个选择写在模型选择旁边，而 toast 由循环弹出（于是边界的到期计时器会武装起
来，`docs/ui/toast.md`）。toast 重复后端自己对这一档的描述 —— `Speed: fast —
1.5x speed, increased usage` —— 因为速度的价钱值得说在买它的地方；记录什么都没描
述的档读作 `Speed: fast`，standard 读作 `Speed: standard`。

### 线上格式

`ModelConfig::service_tier`（来自 `Selection::service_tier`）携带选中那一档的
id；两条 OpenAI 线上格式都把它当顶层的 `service_tier` 字段发出去：

| 线上格式 | 请求 |
| --- | --- |
| Responses（`docs/providers/chatgpt.md`） | `"service_tier": "priority"`，与 `reasoning`、`store`、`prompt_cache_key` 并列 |
| Chat Completions | `"service_tier": "priority"`，与 `prompt_cache_key` 并列，在供应商 `extra_body` 合并之前 |
| standard | 完全没有这个字段 —— 后端自己的默认，绝不是一个它还得去认识的 `"default"` |

Chat Completions 这条线上格式之所以取这个字段，是因为 OpenAI 自己的 API 就有；既
然档只会被设成模型的*记录*所列出的那种，而只有 ChatGPT 后端的记录会列，别的供应
商永远见不到它。Anthropic 与 Ollama 的线上格式忽略这个配置字段 entirely。

在 ChatGPT 后端上，请求还带着 codex 的路由提示：`chatgpt::routing_hint(model,
tier)` 在每次请求上渲染 `model={model}`，选了档时再加 `;tier={tier}`，由
`openai::chatgpt_request_headers` 挂在 `session_id`/`conversation_id` 这几个缓存
亲和头旁边，门控方式也一样 —— 粘贴 key 的供应商永远见不到 codex 的头。

有三个请求无论 session 的档是什么，都刻意以 **standard** 跑：

- 定义里**钉住了另一个模型**的 subagent（`OpenAiClient::with_model` 连同思考模式
  与视觉判定一起丢掉档 —— 这三者都是为被替换掉的那个模型探测出来的，而钉住的模型
  没列的档，是一个后端可能拒掉的请求；codex 的 `apply_spawn_agent_service_tier`
  做同样的切除）。继承的 subagent 跑在 session 的配置上，因此也是 session 的速
  度。
- **auto 模式的 classifier**（`SafetyClassifier::new` 在思考模式旁边一并清掉
  它）：一个裁决只有几个 token，优先处理却要计方案用量，而
  `ZEROTH_CLASSIFIER_MODEL` 可能点名一个一档都不列的模型。codex 的 Guardian 请求
  出于同样的理由发 none。
- 别的没有了：`/compact` 的摘要像同一个模型的任何其他回合一样骑在档上。

### 状态从哪来

- **`/model` 切换** —— 选中的 `ModelEntry` 带着它的 `service_tiers`，随
  `Action::SelectModel` 一起走，于是一次成功的切换无需重新拉取就播种了档命令。当
  新模型列出同一档时，**当前选择被带过去**（codex 对任何支持它的模型都保留配置的
  档），否则回落到 standard。
- **启动，已保存的选择** —— `config.json` 在 `thinking` blob 旁边持久化一个
  `speed` blob（`llm::settings::SpeedSettings`：列出的各档与选中的 id），写在当
  前工作目录自己的条目里（`docs/state/per-directory-state.md`），于是启动时从文
  件播种 `App::set_speed`，而档骑在第一个请求上。**空** blob（`{}`）是一个已知不
  列任何档的模型的标记 —— `ThinkingSettings::unsupported` 那个思路 —— 于是启动从
  不为它重新探测。
- **启动，支持未知** —— 这个功能之前的文件根本没有 blob，而既有的能力探测
  （`docs/providers/reasoning.md`）会为它触发，就像为一个未知的思考或视觉状态触
  发一样：一次后台的 `/models` 拉取，从中读出当前模型的记录，各档被采纳并持久
  化。一个记录不再列出的已选档被丢掉，backend 重新接上，好让下一个请求不再带着
  它。与各处一样，被环境变量覆盖的选择永远不回写。
- **探测答上来之前，各档是第三种状态 —— 未知 —— 而 `persist` 就当它是第三种。**
  `ModelSession::speed_known` 说明各档是否曾经第一手学到过（保存的 blob、一次切
  换、那次探测），而 `SpeedSettings::recorded(known, state)` 在没学到时**完全不
  写 blob**：否则，一次在探测答上来之前那几秒里发生的 Ctrl+T 持久化，或者在一次
  失败的探测之后，就会写下空 blob —— 那是「已知不列任何档」的模型的标记 —— 而下
  次启动会把它读成永远不必再探测的理由，让那个目录里那个模型的 `/fast` 就此死
  掉，且没有任何东西说明原因。对一个并行实现的复核恰好在本代码上演示了这个序列，
  所以这条规则有它自己的纯测试。

`/clear` 与 `/resume` 从不碰这个状态（模型没变）；切到一个不列任何档的模型会清掉
它（`App::set_speed(None)`），于是页脚的档词变空，各档的行也从 palette 里拿掉。

## 活体验证

对着一个已登录的 ChatGPT 账号跑过
（`tests/live_caching.rs`，
`live_chatgpt_fast_mode_is_listed_and_a_priority_request_is_served`，那个被忽略
的活体测试）：该账号的七个模型 ——
`codex-auto-review`、`gpt-5.5`、`gpt-5.6-luna`、`gpt-5.6-sol`、
`gpt-5.6-terra`、`gpt-6-astra`、`gpt-reserve` —— 每一个都恰好列了一档
`Fast=priority`，描述为 `1.5x speed, increased usage`（`gpt-6-astra` 上是
`2x`）；那个账号没有任何记录列出 `ultrafast`。同一个
一个词的 prompt 在 standard 下和带 `service_tier: "priority"` 加路由提示时都得到
了回答，各自带一个用量帧（`input: 20, output: 5`）。一个样本说明不了速度上的任何
事，也没有去测；它证明的是这个字段与这个头会被接受并伺候。

2026-09-17 重跑过一次，在供应商改名之后（`docs/providers/chatgpt.md`）、并且逐档
命令已就位的情况下，用 `CHATGPT_CODEX_REFRESH_TOKEN`：同样七个模型，每个都恰好列
一档 —— `Fast=priority`、`1.5x speed, increased usage`（`gpt-6-astra` 上是
`2x`）—— 而 standard 与 priority 请求各自都带一个用量帧得到了回答。真实二进制在
tmux 里对着这个账号以 `ZEROTH_MODEL=gpt-5.5` 驱动，在 `/model` 之后紧接着列出了
`/fast`，带着 `1.5x speed, increased usage`；第一次运行时 toast 出 `Speed: fast
— 1.5x speed, increased usage`、页脚里是 `gpt-5.5 medium fast`，第二次是 `Speed:
standard`，并且把轮转后的 refresh token 写回到新变量名下。还没有任何活体记录列出
第二个档，所以那一行是由 `smoke.sh` Phase 117 那个由 stub 伺候的两档目录来演练
的，而不是活体。

## 测试

档的解析（一个列出的目录、遗留标记、一个没名字或没 id 的条目、一条什么都不列的记
录）、状态与它的切换（选中、同一条命令回到 standard、两档之间的直接切换、一个未
列出的 id 什么都不改、陈旧选择回落到 standard）、命令名（codex 的转小写名字、不
是单个 token 的名字的 slug、留不下任何东西的名字）、两份载荷与路由提示、钉住模型
与 classifier 的丢弃、设置的往返（blob、空标记、遗留文件、三态的 `recorded` 规
则）、palette（注册表里没有 `/fast`、每个列出的档在 `/model` 之后紧接一行并带记
录的描述、什么都没描述的档用泛用描述、不可用或会遮蔽的名字没有行、各档的行随状态
消失）、按键路径（`/ultrafast` 加 Enter → `SetSpeed`、同一条命令回到
standard、`/fast` 再 `/ultrafast` 直接切过去、回合中途、没匹配上的 `/fast` 被吞
掉而不是被发出）、条带对一行档的渲染，以及页脚戴上任何一档的名字，都在各自的模块
里有单元测试。边界的接线 —— 探测、持久化、backend 重新接上 —— 由活体测试和实际运
行应用来演练；`smoke.sh` Phase 117 在真实二进制里驱动两边：dummy 不显示任何档行
（`No matching commands`、Enter 被吞、页脚没有那个词、没有 `Speed:` toast），然
后是一个 ChatGPT Codex 模型，它的列表 —— 由本地 stub 顶替 token 铸造与 `/models`
目录 —— 点名两档，`/model` 之后是 `/fast` 与 `/ultrafast` 两行，戴着记录里的描
述，各自带着自己的 toast 与页脚词被切换，stub 日志里只有一次列表拉取。Phase 1 用
数行的方式断言 palette 的行数上限，而不是点名第八位是哪个命令，所以落在 `/login`
之前的档行不会弄坏任何东西。

## 已知限制（v1）

- 记录里的 `default_service_tier` 不被尊重（今天每条记录都发 `null`，而 codex 反
  正也让配置的选择压过它）；这里的默认永远是 standard。
- 档是按选择的，不是全局的：切到一个列出它的模型会把选择带过去，但一个全新的目录
  从任何地方最后一次所做的选择开始，与思考模式完全一样。
- 没有任何东西测量速度。toast 重复的是后端自己的说法以及它所要的方案用量；某个请
  求究竟是否更快，是后端的事。
