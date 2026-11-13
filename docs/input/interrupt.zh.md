# Esc 打断进行中的回合 —— 设计

日期：2026-06-10

## 目标

**在回合生成时按 Esc** 立刻停下这个回合 —— 像 openai/codex 打断一个运行中的
任务那样 —— 而不是退出应用。半截的回复留在对话里，一条 notice 告诉用户发生了
什么，输入框马上可以打下一条消息。

有两处刻意与 codex 不同，调的是“没有值得留下的东西”时该怎么办：

- **还没有输出 → 撤销，不通知。** 如果 Esc 落在回合产出任何东西之前（没有半截
  回复、没有 tool、没有哪个已经开始流进自己单元格的 `write` ——
  `docs/ui/write-streaming.md`），而且它身后没有任何东西在等 —— 既没有排队的
  后续消息，也没有一条交给本回合却从未被它读过的消息
  （`docs/input/queue.md`）—— 那么这次提交被**回滚**而不是被打断：刚发出的
  用户消息回到 composer 里可编辑，并且**不**提交 `Conversation interrupted`
  notice。既然没有输出，我们就退回提交之前的状态，而不是在屏幕上留下一个多余
  的用户气泡 + notice。
- **`!` shell 的打断不提交 notice。** 被取消的 shell 命令已经把自己的单元格
  落定为 `⎿ Interrupted by user`；在它下面再来一行 `Conversation interrupted`
  就是重复，所以 shell 回合跳过 notice（它的单元格就是记录 —— 如同它也省掉的
  `Ran for Ns` 摘要）。

第三处不同走的是另一边，留住 codex 丢掉的东西：**留下的回合留住它的回执。**
在 notice 之下提交一个完成的回合收尾时用的那条暗淡摘要 ——
`Worked for 8s · ≈24.3k tokens` —— 于是停下一个回合不再抹掉那两个说明它付出了
什么的数字（见下面的*被打断的回合留下的回执*）。

## codex 是怎么做的（移植自 `/tmp/codex/codex-rs/tui`）

- 任务运行时**单次 Esc 立刻打断** —— 没有双击确认（`bottom_pane/mod.rs`：
  当 `is_task_running` 时 `interrupt_turn` 绑定触发）。有弹窗打开时，**关掉弹窗
  优先**于打断。
- 已流出的半截文字**保留**在 transcript 里；已经显示出来的东西一概不撤回。
- 提交一个**错误样式**的单元格：
  `"Conversation interrupted - tell the model what to do differently. …"`
  （`chatwidget/turn_runtime.rs::interrupted_turn_message`）。被打断的回合**没有**
  “done”摘要 —— notice 就是它的终态。
- 任务运行时实时状态行带着提示：`(0m 00s • Esc to interrupt)`
  （`status_indicator_widget.rs`）。
- 空闲时的 Esc 是 codex 的回溯（“编辑上一条消息”）—— 现在已移植
  （`docs/input/backtrack.md`）：空闲 Esc 只要存在上一条用户消息就武装 Esc-Esc
  手势，只有在无可回溯时才退出。打断有严格的优先级：`turn_active()` 时 Esc 永远
  打断，只有*下一个* Esc（此时已空闲）才武装 —— codex 那条“打断 → 预备 → 预览”
  的链。

## 映射到这份代码库

### 按键处理（`App::on_key_conversation`）

优先级顺序，照搬 codex 的“弹窗优先”规则：

1. `Esc` 且斜杠命令面板打开 → 关掉面板（不变）。
2. `Esc` 且回合活跃（`App::turn_active()`）→ **`Action::Interrupt`**（新变体）。
3. `Esc` 其他情况 → `Action::Quit`（不变）。

Ctrl+O overlay 里的 Esc 仍然只是回到对话（overlay 是只读查看器；打断从对话
发起）。Ctrl+C 在输入为空时仍然退出（接在 codex 的清空 composer 一步之后 ——
输入里有文字时第一次 Ctrl+C 只清草稿，见 `docs/design.md`）；流中途退出本来
就会取消并收割 backend。

### 纯状态（`App::interrupt_turn`）

`fail_stream` 的兄弟，返回一个 enum 告诉循环怎样收敛屏幕 —— codex 的“留下已
流出的”*或者*这份代码库的“什么都没流出，那就撤掉”：

```rust
pub enum InterruptedTurn {
    Undone,                          // no output + empty queue: rolled back
    Kept {
        partial: Option<String>,     // the kept partial reply, if any text streamed
        tools: Vec<ToolCall>,        // every call in flight — the running one and each
                                     // ⎿ Waiting… sibling — resolved Failed, in order
        notice: Option<&'static str>,// INTERRUPT_NOTICE, or None for a shell turn
        agents: Option<AgentGroup>,  // the live agent group, resolved interrupted
        summary: Option<TurnSummary>,// the receipt — built here, recorded by the
                                     // boundary; None for a shell or /compact turn
    },
}
```

`interrupt_turn(elapsed_secs)` —— 回合的墙钟来自边界，与 `end_turn` 的一样 ——
先取走半截缓冲，然后分支：

**撤销路径** ——
`partial.is_none() && current_tool.is_none() && queued.is_empty()
&& steered.is_empty() && !steered_this_turn`（什么都没产出，身后没东西在等，
中途也没读到任何消息）：

- **pending** 守卫：打断正是发送一条回合从未读过的消息的动作，所以在那次派发
  还在进行时把提交拉回来，会让人以为撤销什么都没做。
- **delivered** 守卫：回合*确实*读过的一条消息会留下一条用户消息作为历史尾部，
  哪怕回合已经提交了回复和 tool 单元格，所以只看尾部的判断会在投递与下一轮首个
  token 之间的空档里触发撤销 —— 把模型已经读过的消息拽回 composer，并且不在
  屏幕上的输出之上提交任何 notice（`docs/input/queue.md`）。

然后：

- `take_trailing_user_messages` 弹出末尾那串最长的 `Role::User` 消息（本回合
  自己的输入 —— 上一个回合总是以摘要、notice 或 tool 收尾，绝不是用户消息），
  并用 `\n` 把它们接起来（作为一个回合刷出的一批会记下好几条；codex 的 Alt+Up
  也是这样重新拼合）。
- `recall_input` 把那段文字放回 composer。
- 状态清空，**什么都不**记录 —— 没有半截，没有 tool（它只是被窥视、没被解析，
  所以 token 统计不动），没有 notice。
- 返回 `Undone`；循环重绘 scrollback，不要那条被回滚的消息。shell 回合永远不会
  走到这里 —— 它总有一个正在跑的 tool。

**保留路径** —— 有东西流出来了（或者有队列在等）：

- 把任何非空的半截文本作为普通的 `Role::Assistant` 历史消息保留（codex 保留
  半截输出）。
- **每一个在飞的 call** 都以 `ToolStatus::Failed` 解析，输出为
  `INTERRUPT_TOOL_OUTPUT`（`"Interrupted by user"`），走既有的 `end_tool` 路径
  （`App::fail_live_queue`）—— 正在跑的那个 call，在中途被取消（codex：被中止
  的 tool“可能已经部分执行”），**以及**它身后并行批次里的每一个 `⎿ Waiting…`
  兄弟，按批次的顺序（最前面那个本身可能还在等，等一个权限 prompt 或者 auto
  模式的分类器 —— approve 接缝跑在它的 `ToolStart` 之前）。每一个都被记录，所以
  每一个都在屏幕上和 Ctrl+O 视图里保住自己红色的单元格，而下一个请求推导出的
  上下文会重放整个这一轮 —— 一条 assistant 消息，按 provider id 和原样参数持有
  每一个 call，每个都以 `Interrupted by user` 作答（Claude Code 对一个被中止
  后没有结果的 tool use 的答复）。像以前那样丢掉兄弟 call，会把模型已经做过的
  调用从 Esc 之后的一切里抹掉：下一个回合只看到第一个 call。参见
  `docs/tools/parallel-tools.md` 的*打断一个批次* —— 以及它的*限制*一节讲的那些
  本轮从未宣告过的 call（在一个前台 agent 组之后、一个没走到的 task call），
  它们仍然不留记录。
- 把 notice 作为 `Role::Error` 消息记录 —— `INTERRUPT_NOTICE` =
  `"Conversation interrupted - tell the model what to do differently."`
  （codex 的措辞去掉它的 `/feedback` 广告）—— **除非**这是一个 `!` shell 回合
  （`status.shell`），它的 `⎿ Interrupted by user` 单元格就是它自己的记录，所以
  `notice` 为 `None`，不记录任何 `Role::Error` 消息。
- 基于实时状态构建本回合的**回执** —— 在上面那些 call 解析之前，所以它读到的是
  状态行当时显示的统计 —— 并作为 `summary` 交回而不记录它（下面的*被打断的
  回合留下的回执*）；shell 或 `/compact` 回合为 `None`。
- 清掉实时状态。

没有回合在飞时返回 `None`（什么都不记录）。

注意缓冲与 tool 的互斥：一次 `ToolStart`（或一个批次的 `ToolBatch`）先冲刷掉
流式片段，所以 tool 在飞时缓冲是空的 —— 实际上 `partial` 从不会与非空的 `tools`
同时为 `Some`，而历史顺序（半截、tool、agent 组、notice，然后是边界记录的回执）
不论如何都与流的顺序一致。

### 事件循环（`main.rs`，`Action::Interrupt` 分支）

1. **`abandon_inflight`** —— 先 `cancel.cancel()`，然后**脱离** backend 线程
   （绝不在循环上 `join()` 它）并**换上一个新的回复 channel**。这就是对
   *打断滞后*冻结（见下）的完整修复：旧代码在这里 `join` 那个线程，会把单线程
   循环阻塞到线程解开为止 —— 对 dummy 没问题（它每 ≤20 ms 轮询一次 token），
   但对一个停在阻塞读里的真实网络 backend 就不是（见*打断滞后的冻结*）。脱离
   让循环保持响应；
   换 channel 既是“线程停止发送”的保证，**也是**排空 —— 将死的线程仍可能发出的
   任何陈旧事件（一个最后的 chunk，或者一个否则会卡出一个幽灵“运行中”tool 的
   `ToolStart`）都发往它的旧 sender，而那个 receiver 我们刚丢掉，所以它永远
   到不了下一个回合（下一个回合在新 sender 上起）。脱离出来的 handle 停在
   `reaping` 里，完成时清扫（`is_finished()`，非阻塞）。
2. 回合的时钟在步骤 1 **之前**读，因为步骤 1 会停掉它 —— 回执要说出这个回合跑了
   多久。然后 `app.interrupt_turn(elapsed)` 并按结果分支：
   - **`Undone`** —— 提交被回滚了（消息回到 composer，从历史里丢掉）。
     `repaint_conversation(ReflowClear::Purge)` 从此刻已被截短的历史重建
     scrollback，于是用户气泡消失、composer 显示恢复的草稿。不提交，也不刷队列
     （空队列正是撤销的前提）。
   - **`Kept { partial, tools, notice, agents, summary }`** —— 像
     `StreamDone` 那样提交：先把 viewport 重新坐回空闲高度（流式条带已经没了
     —— 不变量 3），然后 `insert_before` 半截文本、每一个被取消的 call（折叠、
     红色、空行分隔）、
     agent 组，以及 notice（`commit_turn_failure`，各自带一个空行间隔）。这些
     call 是按数量从历史里读回来的（`ui::resolved_tools_commit_lines`）：notice
     已经记录在它们之后，所以 `ui::tool_commit_lines` —— 它提交刚解析的那一个
     call，即最后一个历史条目 —— 会在那里找到一条消息而什么都不提交，屏幕上就
     会出现 notice 而上方完全没有单元格。`notice` 对 shell 回合是 `None`，所以
     它那行 `⎿ Interrupted by user` 单元格单独站着。接着，回合期间被按住的
     后台完成消息落定，回执被记录（`App::record_turn_summary`）并提交在它们
     之下（`ui::summary_lines`）—— 与 `StreamDone` 的顺序相同，所以摘要为回合
     收尾。然后把队首那一批（`flush_next_queued`）刷到**新** channel 上 —— Esc
     立刻发送队列。

`Action::Interrupt` 只可能来自对话视图（overlay 的 Esc 是返回），所以这些提交
从不碰备用屏。

**回合中途的 `/clear` 复用步骤 1**（取消 + 脱离 + 换 channel —— 同一个“之后
不可能有陈旧事件到达”的保证，如今不带冻结），但跳过那些提交：
`App::clear_conversation` 抹掉历史、流式缓冲、运行中的 tool、状态，*以及排队的
积压*，不记录半截、不记录 notice、也不记录摘要 —— 用户要的是一块白板，不是一个
完成的回合。见 `docs/design.md`（`/clear` 那一段）与 `smoke.sh` Phase 16。
**回合中途退出**同样取消而不 join，于是终端恢复不会被一个卡住的 backend 线程
拖住 —— 进程紧接着就退出，操作系统回收它。

### 打断滞后的冻结（为什么脱离而不是 join）

回复 backend 在一个普通 OS 线程上流式输出，并且**协作式**停下 —— 它在 chunk
之间轮询 `CancelToken`。dummy 每 ≤20 ms 检查一次，所以 `cancel + join` 几乎立刻
返回。一个**真实** backend（`llm::LlmBackend`）*曾经*不同：在首 token 之前的
停顿里，线程停在一个*阻塞*的网络操作上 —— `reqwest` 的 `send()`（等响应头，
从不轮询 token）或者第一个 SSE `read()`（只在读*之间*轮询）—— 要到客户端每个
操作的超时（当时是 3 秒）之后才醒。于是 Esc 之后，线程要到它在飞的操作返回才能
看到取消 —— 网络答复（约 1–2 秒）或者那个 3 秒上限。（今天这些阻塞操作住在一个
更外层的*脱离的传输线程*上，流式线程从它接收时每 ~50 ms 轮询一次 token ——
`openai::drain_stream`，见 `docs/providers/llm.md` —— 所以即使正卡住，它也会
迅速确认一次取消。下面“脱离而不 join”的纪律仍然成立：它正是让*任何* backend
的最坏情况都远离循环的那条规矩。）

旧分支在**单线程**（`current_thread`）的 tokio 循环上对那个线程调
`handle.join()`，于是整个事件循环 —— 绘制 tick、状态动画、按键 —— 在那一整段
窗口里被阻塞：这就是被报告的“按 Esc → spinner 冻住 1–2 秒，然后解冻”的 bug。
由 `stream::StallAi`（一个忽略取消 `ZEROTH_STALL_MS` 毫秒的 backend，模拟卡住的
读）离线确定性地复现：卡住的 backend 在卡住之前什么都不流，所以 Esc 落在**撤销**
路径上 —— 用旧的 `join`，状态行会冻住约 `STALL_MS`；用 `abandon_inflight`，它在
一帧之内清空（消息也回到 composer）。`smoke.sh` Phase 32 断言这个 prompt 落定。

### 状态行的提示（`ui::status_line`）

细节从句获得 codex 的可发现性提示，作为它最后一段暗淡的片段（小写，符合这份
代码库的提示惯例 —— `(ctrl+o to expand)`、`esc return`）：

```
( ●    ) Working… (3s · ↓ 27 tokens · Thinking for 2s · esc to interrupt)
```

### 被打断的回合留下的回执

日期：2026-10-25

codex 在它的 notice 之下什么都不提交，这个移植原来也是：那行红色就是这个回合的
全部终态。但打断恰恰是回执最要紧的时刻。用户停下回合，是因为它跑得太久或者走偏
了，而说明这一点的两个数字 —— 状态行的耗时和它的 token 统计 —— 随那一行一起
消失。所以**保留**路径现在会在 notice 之下提交一个完成的回合收尾时用的那条暗淡
摘要：

```
● Here is how the parser handles a nested block. It first

● Conversation interrupted - tell the model what to do differently.

Worked for 8s · ≈24.3k tokens (8k cached)
```

- **动词与时间**遵循完成回合的规则（`docs/ui/status-indicator.md`）：该行最后
  所戴动词的过去式，以及边界自己的时钟，在 `abandon_inflight` 停掉它之前读出。
  两个回合末都用同一个 `App::turn_summary` 构建它们的摘要，所以两者只差各自交给
  它的那份估算占比，而循环通过 `Session::commit_turn_summary` 提交两者。
- **token** 是按 Esc 那一刻状态行的统计：已完成的那些轮的 provider usage 帧，
  加上 app 对 Esc 截短那一轮的估算。provider 在一轮结束时发该轮的 `usage` 帧，
  所以被抛弃的一轮从不上报，它的份额只能是估算。统计在打断解析在飞 call **之前**
  读出：它给那些 `Interrupted by user` 答复记的账，是*下一个*请求的上传，不是
  本回合的。
- **`≈` 标出任何估算的份额。** `docs/providers/prompt-caching.md` 承诺回执里的
  数字是 provider 自己的、绝不是猜的，而记录把两者分开以守住这一点：
  `TurnSummary::tokens` 仍然只有帧报告的部分，`TurnSummary::estimated` 是叠在
  上面的 app 估算。`ui::summary_lines` 把它们折成一个总数，并在估算非零时用
  `/context` 页面的 `≈`（`SUMMARY_ESTIMATE_MARK`）标记它。在一次单独的 tool
  call 期间按 Esc，且该轮的帧已经把统计吸附过，就什么都没有估算，读作裸数。
  流中途按 Esc，或者在离线 demo 上（完全不上报 usage），读作 `≈`。缓存与写入
  两半只来自帧，所以它们从不被标记。
- **估算偏低。** 统计从未计入被截短那一轮重发的上下文：system prompt、tool
  schema 以及到目前为止的对话，而这些正是一个完成的轮的帧在把统计吸附上去时加
  上的东西。回执说的是这个回合流出了什么，加上 provider 确认了什么。它不是账单；
  provider 的仪表盘才是。
- **没有回执**的情况：没有回合可交代时，或者回合保留自己的记录时（shell 与
  `/compact` 这两个例子，也正是完成回合的摘要会跳过的两个）：
  - **撤销**（什么都没流出，提交回到了 composer，屏幕回到发送之前）；
  - 一个 **`!` shell** 回合（它的 `⎿ Interrupted by user` 单元格就是它的记录，
    就像 `Ran for Ns` 一行从来也不是记录）；
  - 一个 **`/compact`** 回合（它的记录是 `● Context compacted` 单元格，而被
    打断的压缩永远挣不到它，且它固定的 `Compacting` 动词没有过去式可供摘要
    开头）。
- **顺序。** 历史与 scrollback 先得到半截、各 call、agent 组和 notice，然后是在
  这个回合末落定的任何后台完成消息，然后是回执。这就是 `StreamDone` 的顺序，
  其中摘要为回合收尾（`docs/tools/background.md`）。这也是为什么
  `interrupt_turn` 只*构建*回执，而由边界*记录*它
  （`App::record_turn_summary`），即 `take_turn_summary` 已经做过的那种切分。
- **什么会留存。** 回执是一个普通的 `HistoryItem::Summary`。resize 或模态框关闭
  会重建它，Ctrl+O 列出它，`/export` 写出它，rollout 保留它，`estimated` 在其他
  计数旁边、为零时省略。被续上的被打断回合保留它的 `≈`，而完成回合的记录序列化
  结果与以前完全一样。推导的上下文把摘要当作装饰跳过，所以下一个请求、以及它
  背后的 prompt 缓存，都不变。

backend 的 `Error` 仍只在它那行红色错误上收尾（`App::fail_stream`）。subagent
也不变：被停下的 agent 在它红色的名单行上为 `AGENT_STOPPED_LINGER` 保留它的耗时
与统计，它自己的 transcript 不记录回执（`docs/agents/agent-tool.md`）。

## 测试

- `app`：回合活跃时 Esc → `Action::Interrupt`；空闲 Esc 仍然退出；面板打开时 Esc
  仍然只关掉它（回合继续跑）；`interrupt_turn` —— **保留**路径 —— 保留半截并
  记录 notice（`Kept { notice: Some(_) }`），把运行中的 tool 解析为 Failed
  并带 `"Interrupted by user"`，清掉状态，用注入的时钟打戳，且空闲时是 `None`
  空操作。
- `app`（回执）：保留的回合交回一个完成回合收尾时用的摘要 —— 它的动词是最后一帧
  画过的那个，轮换过的也算，秒数来自边界，仍在运行的 shell 被计入
  （`an_interrupted_turn_hands_back_its_receipt`、
  `the_interrupted_receipt_names_the_verb_the_last_frame_drew`、
  `an_interrupted_turn_hands_on_its_last_verb_too`、
  `the_interrupted_receipt_counts_the_shells_still_running`）—— 只构建、不记录，
  好让边界能在它之上落定被按住的那些完成消息；记录之后历史读作半截、notice、
  回执（`the_interrupted_receipt_is_built_not_recorded`）；帧的 token 仍是
  `tokens`，被截短那轮的份额是 `estimated`，缓存两半只算帧的
  （`the_interrupted_receipt_keeps_reported_usage_apart_from_the_estimate`）；
  统计在各 call 解析之前读出，所以在它那轮的帧之后被打断的单独一个 call
  什么都不估算
  （`an_interrupt_with_nothing_streamed_since_the_frame_estimates_nothing`）；
  `!` shell 或 `/compact` 回合不保留任何回执
  （`an_interrupted_shell_turn_keeps_no_receipt`、
  `an_interrupted_compaction_keeps_no_receipt`）；完成的回合什么都不估算
  （`a_finished_turn_estimates_nothing`）。`ui`：总数把估算折进去，且只在有估算
  时戴 `≈`（`summary_lines_mark_an_estimated_share_with_approx`）。`session`：
  估算往返一致，而没有估算的摘要写出的仍是它一贯那一行。
- `app`（**撤销**路径，需求 1）：没有输出时打断（`Kept` 的反面，`Undone`）会把
  本回合的用户消息从历史里丢掉，把它们的文字（一批用 `\n` 重新拼合）恢复到
  composer，**不**记录 notice，并且只收回*当前*回合的消息；非空队列则退出这条路
  （回到保留路径，提交 notice）。
- `app`（shell 跳过 notice，需求 2）：打断一个 `!` shell 回合把命令解析为 Failed
  并带 `"Interrupted by user"`，返回 `notice: None` —— 没有 `Role::Error` 的
  `Conversation interrupted` 消息。
- `app`（一个并行批次）：批次中途 Esc 解析运行中的那个 call **以及**每一个
  `⎿ Waiting…` 兄弟，按顺序，各自记录在 notice 之前
  （`interrupt_mid_batch_resolves_the_running_call_and_every_waiting_sibling`）；
  整个批次都还在等队首 call 的权限 prompt 时 Esc 也一样
  （`interrupt_while_the_whole_batch_still_waits_resolves_every_call`），批次
  中途 backend 出错也一样
  （`fail_stream_mid_batch_resolves_every_call_in_the_batch`）；一个宣告过的轮
  会用模型自己的参数给每个等待中的 call 打戳，而被打断的一轮推导出的上下文是
  一条 assistant 消息，按 provider id 携带每一个 call，各以
  `Interrupted by user` 作答
  （`an_interrupted_round_replays_every_call_the_model_made`）。
- `ui`：`resolved_tools_commit_lines` 按顺序提交每个已解析的单元格，空行分隔，与
  重建一致，并冲刷掉那个被打断的 call 一直占着的 MCP 运行；`agents`：批次中途被
  停下（或失败）的 subagent 记录每一个 call；`llm::agent`：被取消留下未执行的
  call，在存储的消息列表里以 `Interrupted by user` 作答。
- `ui`：`status_line` 在每个阶段都以暗淡的 `esc to interrupt` 提示收尾。
- `stream`：`StallAi`（卡住 backend 的测试替身）在它的卡住期间忽略取消 ——
  `join()` 它的调用方要付满整段卡住 —— 而被放着跑时流出一个正常回复。
- `src/tui/`（smoke，Phase 8）：流中途 Esc（半截流出之后）把半截文字与
  `Conversation interrupted` notice 留在屏幕上，清掉状态行（没有
  `esc to interrupt`），在 notice 之下恰好提交一份回执 ——
  `Worked for Ns · ≈N tokens`，全部是估算的，因为 dummy 不上报 usage —— 而应用
  仍能正常完成下一个回合，其摘要是屏幕上的第二条。Phase 13 也等那第二份摘要，
  要求恰好两份，并检查第 1 回合的回执位于它的 notice 与 Esc 送出的那条排队消息
  之间。
- `src/tui/`（smoke，Phase 32 —— 打断滞后的回归护栏，需求 1）：backend 卡住 3 秒
  （`ZEROTH_STALL_MS`，忽略取消且什么都不流），Esc **撤销**这个回合 —— 状态行在
  **一帧之内**清空（断言 `< 1.5 s`，远低于卡住时长），`hello there` 回到
  composer 且**没有** `Conversation interrupted` notice —— 证明循环是脱离线程而
  不是 `join()` 它。实测：~3.0 秒（旧的，冻结）→ ~0.015 秒（修复后）。
- `src/tui/`（smoke，Phase 19 —— 需求 2/3）：运行中的 `!sleep 9` 显示
  `⎿ Running… (Ns)` 预览且**没有**状态行，Esc 把它解析成
  `⎿ Interrupted by user` 且**没有** `Conversation interrupted` notice。
- `src/tui/`（smoke，Phase 123 —— 一个并行批次）：在 dummy 那个三调用的
  `Bash(ping …)` 批次运行时按 Esc，提交全部三个红色
  `⎿ Interrupted by user` 单元格，按批次顺序，位于 notice 之上，回执在它下面；
  Ctrl+D 显示三个 call 全部作答（且没有回执 —— 摘要是装饰），而 resize 的 purge
  重建把每个单元格与回执各显示恰好一次；在批次的权限 prompt 上按 Esc（两个
  单元格仍是 `⎿ Waiting…`）解析两个 call，屏幕上与 Ctrl+D 里都一样，在同一份
  回执之下。修复之前，面板只显示 notice，Ctrl+D 只有第一个 call。
