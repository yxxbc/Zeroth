# /compact —— 把对话总结掉，腾出上下文

codex 的 `/compact`（它本地的「Memento」压缩，`codex-rs/core/src/compact.rs`）的
移植：这个命令跑一个特别的**总结回合** —— 整段对话加上一段固定的交接 prompt 发给
模型，流出的总结被捕获（从不渲染），从那以后模型的上下文窗口按 codex 的*压缩后历
史* 重建：最近的用户消息（按 token 预算）加上一条带 `SUMMARY_PREFIX` 标记的桥接
消息，携带那份总结。可见的 transcript 不受影响 —— 一个青色的
`● Context compacted` cell 标出那个位置，和 codex 的 info cell 完全一样。

```
❯ long conversation …
● …many turns of replies and tool calls…

(  ●•· ) Compacting… (2s · ↑ 8.1k tokens · esc to interrupt)   ← the turn

● Context compacted                                            ← the marker
───────────────────────────────────────────────────────────────
❯
───────────────────────────────────────────────────────────────
```

一个 `/compact` 回合是**不可 steer 的**（`docs/input/queue.md`）：它的请求是在被
总结的上下文之上的那段固定交接 prompt，不是一段对话，所以它跑的时候提交的草稿会
排成一个后续回合 —— 那本来就是它会成为的东西 —— 而不是被并进总结请求。

## 只追加的设计（历史为什么从不重写）

Codex 保留**两份**历史：TUI transcript 保持完整，而面向模型的 session 历史被换成
压缩后的形状（rollout 文件记录一切外加一个 `Compacted` 条目；恢复时从最新的这类
记录推导工作集）。这个 TUI 只有**一份** `App::history`，同时喂渲染器、recorder、
checkpoint 的键、Esc-Esc 回溯，*以及*逐回合的上下文推导（
`context::context_messages`）。物理上替换它会让可见的 transcript 丢失、逼出一次
丢掉压缩前那些回合的 rollout 重写，并让每个按长度取键的 checkpoint 失效。

所以压缩是**只追加**的：`/compact` 的回合以*追加*一个
`HistoryItem::Compaction { summary, timestamp }` 标记收尾，而替换发生在**推导时
** —— `context_messages` 找到*最后一个*标记，并在它之前的一切的位置上发出：

1. 标记之前的**用户打字消息**（只有 `Role::User` 文本 —— 图片、tool 记录、shell
   transcript 和 notice 都丢，与 codex 一致），在
   `COMPACT_USER_MESSAGE_MAX_TOKENS`（20 000）的预算下由新到旧走：整条消息放得下
   就留，第一条溢出的被**中间截断**到剩余预算（codex 的
   `truncate_middle_with_token_budget` —— 头 + `…{n} tokens truncated…` + 尾），
   然后选出来的这些再翻回时间顺序。token 用 codex 的 `bytes/4` 近似（
   `approx_token_count`），不是估计器的接缝 —— 这一遍在每个回合开始都跑，必须是
   O(len)；
2. **桥**：`{SUMMARY_PREFIX}\n{summary}` 作为一条用户消息（空总结变成
   `(no summary available)`，codex 的兜底 —— 一次真机压缩如今不再走到它：总结回
   合跑在 agent 循环上，它会再要一次返回空了的总结，而一次始终没拿到总结的压缩会
   像其他任何一次那样失败，把上下文留在原样（
   `docs/architecture/unfinished-replies.md`）；这个兜底仍然覆盖在那之前记录下来
   的标记）。前缀是 codex 逐字的 `prompts/compact_summary_prefix.md`（「Another
   language model started to solve this problem…」）；

然后正常推导标记*之后*的条目。更早的标记（第二次 `/compact`）坐在最后一个之前，
被跳过 —— 只有真正的 `Role::User` 文本被收集，所以先前一次压缩的总结在结构上就被
排除了（codex 需要一个 `is_summary_message` 前缀检查，因为它的总结是用户消息；我
们的住在标记里）。

下游一切都成立，零重映射：

- **recorder** 看到的是一次朴素的追加（不重写、不丢 transcript；标记是一条新的
  `compaction` rollout 行，`session.rs`）；
- **checkpoint** 保持有效 —— `after` 的计数只会增长；
- 回溯到一条压缩前的用户消息，会把标记截掉，上下文回到完整对话 —— codex 的
  「回滚到压缩之前」语义白拿；
- **`/resume`** 把标记解析回来，推导重新生效；
- **Ctrl+D** 自动显示压缩后的上下文（它现渲染 `context_messages`）；
- **Ctrl+O transcript** 显示标记 cell，下面带总结文本（暗色、缩进）—— 与内联
  cell 不同，后者只是那一行。

## 那个压缩回合

`/compact`（palette 描述：codex 的「summarize conversation to prevent hitting
the context limit」）在空闲且有一个非空的、可推导的上下文时派发
`Action::Compact`；在回合中途它被拒绝，配一条 `COMPACT_BUSY_NOTICE` toast（
`/resume` 那套 —— codex 在任务期间也禁用它），而没有东西可压缩时它 toast
`COMPACT_EMPTY_NOTICE`。

循环的 `Compact` 分支是 `start_background_turn` 的兄弟 —— 一个没有新用户气泡的回
合：

- `App::begin_compact()` 用**固定**动词打开状态（`COMPACT_VERB`「Compacting」，
  从不轮换；动词游标不动，所以走过各状态动词的那一遍不受影响），外加一个空的流式
  缓冲和一个空的 **`compact_buffer`** —— 那个 flag *和*累加器；
- 上下文照常推导，codex 逐字的总结 prompt（`prompts/compact_prompt.md`，「You
  are performing a CONTEXT CHECKPOINT COMPACTION…」）作为最后一条用户条目压上去
   —— 这是必要的，因为只要上下文非空，真后端就忽略裸的 `prompt` 参数。这段
  prompt 从不记进历史（与 codex 一致：它只活在请求里）；
- 请求跑在一个**一次性无 tool 后端**上 ——
  `LlmBackend::configure(cfg, system_prompt, /*tools=*/false)`，带同样的人格+环
  境 prompt、没有后台注记注入 —— 所以模型只能用文本作答（codex 发总结请求时也不
  带 tool）。没有配置真后端时改用 session 后端（dummy）；`stream::turn_events`
  为压缩 prompt 脚本化一份纯文本总结，好让这个流程能离线驱动（以及被 `smoke.sh`
  驱动）。

**「没有真后端」意味着这个 session 没在跟一个说话** —— 是那个被跟踪的
`ModelSession::real_backend`，不是「有没有配置解析得出来」。按一个可用配置设闸是
个 bug：`ModelConfig::is_usable` 只证明一个 *key* 解析出来了，所以一个配好却**没
选模型** 的 provider（于是 session 跑着 dummy，`active_model` =
`dummy_model_name`）仍然建出一个真的临时后端，为一个不存在的模型发
`POST /chat/completions` ——
`HTTP 400: dummy_model_name is not a valid model ID`，回合红着解析、没有标记
cell。`smoke.sh` Phase 65 离线钉住它，用一个 base 指向丢弃端口的 provider；
- 流出的 `Chunk` **改道**进 `compact_buffer`（`App::push_chunk` 检查那个
  flag）—— 流式缓冲保持为空，所以条带只显示状态行（没有预览行、没有 scrollback
  提交），而 token 计数仍然在跳；codex 同样从不渲染总结；
- 在 `StreamDone` 上，`App::finish_compact()` 取出缓冲、追加 `Compaction` 标记，
  并清掉状态且**没有** `Done for Ns` 摘要；循环提交 `● Context compacted` cell（
  `ui::compaction_lines`）并跑正常的 `dispatch_after_turn` —— 一个排队的批次会在
  刚压缩好的上下文上派发，而回合末的 checkpoint 按新的（增长过的）长度快照。

## 自动压缩 + 上下文量表

Codex 也会在接近上下文限制时**自动**压缩；我们也一样，用同样的阈值 —— **模型上下
文窗口的 90%**（`ModelInfo::auto_compact_token_limit`，
`(context_window * 9) / 10`）。

- **窗口**来自 provider 的 `/v1/models` 记录 —— `context_length`（OpenRouter 和
  大多数聚合器）或 `model_spec.availableContextTokens`（Venice）—— 解析进
  `ModelEntry::context`，骑一次 `/model` 选择和启动时的能力探测，和视觉 flag 一
  起持久化在 `config.json` 里，并可以（为一个不上报的 provider，以及为 dummy）由
  `ZEROTH_CONTEXT_WINDOW` 覆盖或提供。窗口未知 → 没有量表，没有自动压缩。
- **量表**在窗口已知时显示在页脚：`{model} · {cwd} · {used}/{window} ({pct}%)`（
  例如 `1.3k/160k (0.8%)` —— 两个计数都由摘要的 token 格式化器人性化，占比一位小
  数）。在占比旁边显示原始大小是与 codex 的分歧，它只印百分比：绝对数字才告诉你
  一次大粘贴或一条长 tool 输出刚刚花掉了多少空间。`used` 保持 provider 自己的计
  数（`docs/architecture/token-estimate.md`）：最后一次用量帧的 `input` —— 它对
  重发的上下文、tool schema 和框架的计数 —— 加上从那以来并进来的一切的校准估计（
  回复、一个 tool 的输出、下一条消息，在循环底部读取，好让它在下一个请求之前显
  示），而在一次让那个计数按变化的校准估计移动的变动之后：一次压缩（codex 的
  `recompute_token_usage`）、一次回溯、一次 `/resume`、一次 `/clear`（→ 0，直到
  下一条消息）。一个从没见过用量帧的 session（dummy）只读估计，tool schema 也算
  在里面，那也是第一个请求在它的帧落地之前读到的东西。
- **subagent 的 session 视图量的是那个 agent，不是 lead**（
  `docs/agents/agent-context-gauge.md`）：`App::context_gauge` —— 页脚的那一个问
  题 —— 用被查看 agent 自己的上下文（`AgentRun::context_used`，同一条规则，在一
  次沉降没看到帧时从其 transcript 估计）对它运行所用模型的窗口作答，而
  `context_used`/`context_window` 仍然是触发器的输入：自动压缩是关于 lead 对话的
  事实，不管屏幕上是什么。
- **一个空对话读作真零。** 一旦推导出的上下文为空，估计就返回 0 —— 与 `/compact`
  判 `Nothing to compact` 用的是*同一个*谓词，所以页脚和命令永远不会对有没有东西
  存在分歧。system prompt 和常驻的 AGENTS.md 指令确实骑在下一个请求上，而估计在
  对话存在之后才把它们计入，但它们是 session 常量：一个刚启动的 session 两者都带
  着却读 `0/1M`，所以把它们记到一个*被清空的* session 上会让同一个状态显示两个数
  字，而那个看起来像残留的（空屏幕下的 `139/1M`）读起来像一段并没有真的消失的对
  话。`/clear` 现在恰好落在一个新 session 起步的地方。
- **触发器**住在循环底部，每个回合结束和量表变化都落在那里：空闲、越过阈值、并且
  有一个非空的、可推导的上下文，循环就启动这个命令所跑的同一个总结回合 —— 标着
  `auto`，所以 cell 读作 `● Context compacted · 88k → 2.1k tokens · auto`。**每
  个用户回合一次尝试**（codex 的逐回合语义）：一个压缩回合以*任何*方式结束 —— 落
  地、被 Esc、或失败 ——都会把触发器挡到下一个真回合开始，所以一次不够的压缩永远
  不会循环，而一次被打断的也永远不会违背用户意愿重新开始。它跑的时候排队的消息在
  它的回合末派发到压缩后的上下文上，和手动命令完全一样。

标记 cell 的收缩从句（`· {before} → {after} tokens`）在手动压缩上也出现 ——
`before` 是压缩开始时量表的读数，`after` 是标记落地后量表的读数 —— 最后一个计数
按总结所替换掉的东西的估计移动，所以骑在每个请求上的 tool schema 和框架都留在里
面（`docs/architecture/token-estimate.md`）；两者都持久化在 rollout 里（旧文件解
析时把该从句隐去）。一个 ` · {elapsed}` 从句跟在收缩后面（在 ` · auto` 标签之
前）：总结回合跑了多久 —— 边界的回合时钟传进 `finish_compact`，由
`format_elapsed` 人性化（`36s`、`1m 36s`），作为 `secs` 记录在标记上并与量表计数
一起持久化（旧 rollout 上是 0/缺席时隐去它）。cell 例如读作
`● Context compacted · 2.1k → 507 tokens · 36s · auto`。

## 打断、错误、`/clear`

「只在最后替换」的规则是 codex 的：在总结完整流出之前，什么都不变。

- **压缩中途按 Esc** 走正常的打断路径：后端被取消，`App::interrupt_turn` 落在
  `Kept`（一个压缩回合没有尾部用户消息，所以 `Undone` 触发不了），也没有部分内容
  （流式缓冲是空的 —— `compact_buffer` 里的半份总结被丢掉），只记下红色
  `Conversation interrupted` 通知。不追加标记；旧上下文站住。随后一个排队的批次
  在 *未压缩*的上下文上派发 —— 这是有意的（压缩没有发生）。
- **一次后端错误**照旧骑 `App::fail_stream`：缓冲被丢掉，红色错误通知落地，没有
  标记。
- **压缩中途 `/clear`** 能工作，是因为压缩回合住在循环正常的 `inflight` 槽里：
  Clear 分支取消线程并换掉通道，而 `clear_conversation` 也丢掉
  `compact_buffer`。

## 文件

- `prompts/compact_prompt.md` / `prompts/compact_summary_prefix.md` —— codex 的
  prompt 字节，逐字（前缀文件**没有**尾随换行；桥是
  `prefix + "\n" + summary`）。
- `src/context.rs` —— prompt 常量、`approx_token_count`、预算那一遍（
  `compacted_user_texts`）、桥，以及认标记的 `context_messages`；全是纯的，都有
  单元测试。
- `src/app/` —— `HistoryItem::Compaction(Compaction)`（`types.rs`）、
  `begin_compact` / `finish_compact` / `push_chunk` 的改道、`/compact` 命令及其
  忙碌/空守卫、`Action::Compact`。
- `src/session.rs` —— `compaction` rollout 记录（往返；旧构建跳过未知行，既定的
  前向兼容契约）。
- `src/ui/message.rs` —— `compaction_lines`（内联 cell）和 transcript 分支
  （cell + 暗色总结正文）；一个 `conversation_lines` 分支，好让 resize 重绘它。
- `src/tui/` —— `Action::Compact` 分支（`actions.rs`）、`StreamDone` 的压缩分支
  （`stream.rs`）、`Session::start_compact_turn`（`turn.rs`，与 `bootstrap.rs`
  里循环底部的自动触发器共用）、`ModelSession::compact_backend` 及其
  `real_backend` 闸（`models.rs`），以及 `ZEROTH_CONTEXT_WINDOW` 覆盖（
  `config.rs`）与窗口的播种/持久化。
- `src/llm/models.rs` / `src/llm/settings.rs` —— `ModelEntry::context`
  （`context_length` 嗅探）及其 `config.json` 持久化。
- `src/stream/dummy/turns.rs` —— dummy 的纯文本压缩脚本（`compact` 场景；
  `docs/architecture/dummy-backend.md`）。

## 局限

- 在总结调用*期间*没有上下文窗口超限的恢复：codex 在那个 provider 错误上丢掉最老
  的历史条目并重试；我们的 `ReplySource` 接缝没有这样的分类，所以这个回合失败一
  次，带 provider 的错误通知（讽刺的是，这正是 `/compact` 存在要防的失败）。手工
  裁剪之后（`/clear`、回溯）重试这个命令。
- codex 压缩之后那句「Heads up: Long threads and multiple compactions…」警告
  cell 没有移植 —— 标记 cell 就是全部记录。
- 桥按 `bytes/4` 重新计 token，不是应用的 token 估计 —— 与 codex 为预算用的是同
  一个近似。
