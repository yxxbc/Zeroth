# agent 会话视图量的是被查看 agent 的上下文

日期：2026-09-05

## 报告

> subagent 的 TUI 应该显示当前 agent 真实的上下文计数 —— 现在的实现显示的是主
> agent 的上下文 token 数，不是 subagent 的

```
─────────────────────────────────────────────── Look up linuztx GitHub profile ─
❯
────────────────────────────────────────────────────────────────────────────────
  kimi-k3 medium · ~ · 23.7k/1M (2.4%)                                      auto

  ◯ main
  ● general-purpose  Look up linuztx GitHub profile 39s · ↓ 64.9k tokens
```

在一个 subagent 的会话视图里，页脚读作 `23.7k/1M` —— **lead 的**上下文 —— 而它上
面的名单行说这个 agent 自己用了 64.9k 个 token。

## 实际发生的是什么

`ui::footer_line` 用 `App::context_window()` 和 `App::context_used()` 构建它的量
表 —— 主 session 的那一对，也就是 `apply_usage` 向 lead 的用量帧对齐、自动压缩触
发器读取的那一对（`docs/architecture/compact.md`）。它没有任何地方知道屏幕上是哪
段对话。agent 会话视图（`docs/agents/agent-tool.md`）把 transcript、条带、队列
行、Ctrl+O 和 Ctrl+D 换成被查看 agent 的 ——
`docs/agents/agent-view-streaming.md` 讲的正是其中每一个曾经跟不上的故事 —— 但页
脚的模型段和量表段仍然在描述 lead。

而 agent 的上下文大小根本没被记在任何可供显示的地方。`AgentRun` 保留的是**计费
tally**（`tokens`：每个用量帧的 `input + output` 求和 —— 名单上的
`↓ 64.9k tokens`）和逐回合的回执，但*上下文*是另一个数字：每一帧的 `input` 本来
就带着整段重发的对话，所以帧的和是这个 agent 花掉的东西，不是它持有的东西，而随
着轮次增多两者分岔得越来越大。

## 设计

三小块，每一块都是主 session 自己的规则往下放一层：

- **`AgentRun::context_used`**（`agents.rs`）—— agent 的上下文大小，按主量表的精
  确规则（`App::apply_usage`、`docs/architecture/token-estimate.md`）：最后一个
  用量帧的 `input`，逐帧**替换**而不是累加，加上之后并进来那些东西的校准估计，在
  每次沉降时重读（agent 自己的 `tokens::ContextMeter`）。一个沉降时没看到任何帧
  的回合（离线 dummy 在 agent 通道上一个都不脚本化；provider 可能省略用量）站在
  这份估计上，估计基于 agent 自己推导出的 transcript —— `App::take_turn_summary`
  的规则 —— 通过那一个共享的计数函数 `app::estimate_messages_tokens`，所以 lead
  的估计和 agent 的估计永远不会对一条消息花多少钱有分歧。两者都还没落地之前是
  零，主量表也是从这里起步的。这份估计刻意不含 agent 的 system prompt 和简报（这
  次运行两者都不知道；视图的 Ctrl+D 显示它们）——它只是替代一个活 provider 每轮都
  会发的那个帧。
- **`App::context_gauge()`** —— 页脚的那一个问题，**屏幕上**那段对话的
  `(used, window)`：在一个会话视图里是被查看 agent 的 `context_used` 对
  `App::agent_context_window`，否则是主的那一对，而当那个窗口未知时是 `None`（没
  有量表）。`context_used`/`context_window` 仍然是自动压缩触发器的输入：压缩是关
  于 lead 对话的事实，不管屏幕上是什么。
- **谁的窗口，谁的模型。** 一个定义可以钉 `model:`（
  `docs/agents/subagents.md`），而一个被钉住的模型有一个这个 session 从没学到的
  窗口 —— 列表只为*被选中的*模型读。所以 `ReplySource::agent_model(agent_type)`
  呈现那个被钉住的模型（`None` = 继承；`LlmBackend` 从启动切换客户端所用的同一个
  定义上读它），而边界的 `sync_agent_view_context` —— 它本来就在进入视图时和每个
  `sync_backend_info` 旁边跑，现在也在能力探测之后跑 —— 注入
  `App::set_agent_model` 和 `App::set_agent_context_window` 两者（
  `ModelSession::agent_context_window(inherits)`：一个继承类型的窗口就是 session
  的窗口，被钉住的则只有 `ZEROTH_CONTEXT_WINDOW` 覆盖值）。页脚随后点名被钉
  住的模型而不是 session 的 —— 不带 session 的思考模式，那东西和它所替换的模型一
  起在启动时被丢掉了 —— 而一个继承类型保住 session 的那一对。一个被钉住、又没有
  覆盖值的类型不显示量表，正如主页脚对一个 provider 从没上报过窗口的模型也不显
  示：一个诚实的空白胜在 agent 的计数下面放 lead 的分母。

## 测试

- `agents`：用量帧把 `context_used` 安在帧的 `input` 上，而后来的帧替换它，同时
  `tokens` 继续累加；一个看到过帧的 `StreamDone` 保住帧的数字加上回复的估计；一
  次没看到帧的沉降从 transcript 经由 `estimate_messages_tokens` 估计。
- `app`：`context_gauge` 跟着被查看的 agent，关闭时回到 lead 那一对；一个窗口未
  知的被查看 agent 没有量表；`viewed_agent_model` 只在视图内部作答。
- `ui`：`footer_line` 在一个 agent 会话视图里显示 `64.9k/1M (6.5%)`，绝不显示
  lead 的 `23.7k`；一个被钉住的模型被点名时不带 session 的思考模式，而一个继承类
  型保住 `kimi-k3 medium`。
- `llm::backend`：`agent_model` 是定义里钉住的模型，对继承的内置项和未知类型是
  `None`。
- `scripts/smoke.sh` Phase 111：在 `ZEROTH_CONTEXT_WINDOW` 下，agent-stream
  demo 的会话视图打开在 `0/100k (0.0%)` —— agent 真实的零，绝不是 lead 的计数 ——
  在 agent 沉降之后读到它自己 transcript 的一个非零估计，而主页脚的量表（一个不
  同的数字）回到主视图里。
