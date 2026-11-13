# 等待一条断掉的连接

一个到不了 provider 的请求，过去失败的方式与所有其他请求一样：相隔半秒重试三次，
然后一行红色的 `request failed: error sending request for url (…)` 和一个死掉的
回合 —— 在火车上、在 Wi-Fi 边缘、在正在重连的 VPN 后面都是如此。现在，一条断掉
的连接会被**等待**：backend 每隔几秒重发一次请求，无论要多久，状态行说明它在做
什么，而网络一回来，回合就自己接着往下走。

```
❯ refactor the parser

⣤⣀⣀⣀⣀⣀⣀⣀ Waiting for internet… (2m 3s · ↑ 42 tokens · offline for 1m 10s · esc to interrupt)
  ⎿  ((·))  No connection to api.venice.ai — trying again · 14 attempts

────────────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────────────
```

动词和 `offline for` 那个分句是琥珀色，动词处在惯常的微光里；`((·))` 从它的圆点
向外扩散、每秒一次、随距离淡出；主机名是亮的，那一行其余部分是暗的。请求一旦通
过去，这一切全都消失，回复照常流式输出，仿佛什么都没发生过。

## 两类失败，两套策略

重试策略（`llm::retry`，`docs/providers/llm.md`）过去只认识一种失败：*可重试*
的那种，值得在错误露出之前多试几次。对一台**答复得很糟**的主机，那是对的策略
—— `503`、`429`、交换中途被重置的连接、停滞超过按操作超时的请求。每次尝试都要
花一个真实请求，预算之所以存在，是为了让某个恰好闹一会儿脾气的 provider 不会
被永远猛敲；超过预算，错误就是用户需要的信息。

而一台**完全到不了**的主机是另一回事。什么都没发出去，什么都没有在被猛敲 ——
一次失败的连接只花了一个 SYN 或一次 DNS 查找 —— 而错误也不是信息：用户早就知道
Wi-Fi 掉了。他们要的是网络回来时那个回合还在。所以那种失败既没有预算也没有期限：

- **在仍然持有带类型错误的接缝处分类。** 每一处 `reqwest` 发送失败都经过
  `LlmError::transport(&e)`（`src/llm/mod.rs`）：`e.is_connect()` —— 名字解析
  不了、TCP 连接被拒或超时、TLS 握手失败、它前面的代理到不了 —— 变成
  `LlmError::Unreachable { host, message }`，点名请求所针对的主机（URL 里写了
  端口时也包括端口：`127.0.0.1:11434`）；其他一切保持 `LlmError::Http`。这个
  接缝是流式传输（`openai::run_transport`）、`/models` 的抓取，以及三次登录交换
  （Copilot、ChatGPT、Claude），所以在一台离线机器上铸造 bearer 会像它所服务的
  那个请求一样等待。两个变体都携带错误的**整条原因链**：`reqwest` 自己的
  `Display` 停在 `error sending request for url (…)`，而用户能据以行动的那部分
  —— `Connection refused`、`failed to lookup address information` —— 在往下两层
  的 source 里。按 `is_connect` 而不是按匹配那串文本分类，正是让这条规则在传输层
  换了几个版本之后仍然诚实的原因；文本则留给用户，以及留给下面 Ollama 的建议。
- **……并在传输层不肯明说时，拿主机本身去核对。** chat 请求从一根管道里流出它的
  请求体（`docs/architecture/memory.md`），而第一次 smoke 运行就发现了这对错误
  做了什么：连接失败时，请求体的泵*先*失去了接收端，于是 `reqwest` 返回的是一个
  **body** 错误 —— `request or response body error … send failed because
  receiver is gone` —— 它的原因链里哪儿都没有连接错误，所以 `is_connect()` 为
  假，这次拒绝本该走有界重试。`openai::Reachability` 来回答它：对一次传输层不
  归咎于连接的发送失败，紧跟一次对请求**来源**的无请求体 `GET`，走同一个 client
  （同一个代理、同样的信任根与连接池）。一台能答复的主机（任何状态码）就是在
  那儿，
  失败保持有界重试；探测连不上的就是 `Unreachable`，消息用探测自己的原因。纯粹
  的连接失败（`/models` 的抓取、各次登录交换 —— 它们的请求体在内存里）从不探测。
  探测只在已经失败的发送之后跑，所以一条正常的连接不花任何代价；离线时它像请求
  一样快地失败。
- **由 driver 来等。** `retry::next_step` 读一对 `Attempts` ——
  花掉的有界重试数、连续失败的连接尝试数 —— 并以
  `RetryStep::AwaitConnection { attempt, wait, host }`
  回答一个 `Unreachable`：`run_attempts` 公布 `StreamEvent::Offline { host,
  attempts }`，睡 `offline_backoff(attempt)`（1 s、2 s、4 s，之后每次尝试都是
  5 s 的 `OFFLINE_BACKOFF_CAP` —— 之所以短，是因为网络不在时一次失败的连接不花
  什么代价，而回合应当在网络回来后几秒内就接上），然后再试。**这一切都不计入
  有界预算**：一百次失败的连接之后，答案仍然是等待，而在主机回来后紧跟在 `503`
  后面的 `retrying 1/3` 从它的第一个数字开始。一台能答复的主机（哪怕是报错）会
  重置中断次数，所以下一次中断从一数起。
- **与重试同样的两道闸门。** 只有什么都没送出去的请求才等待：一条在*回复中途*掉
  掉的连接是连上过的，它走有界重试，并且先把它的部分撤回（见下面 *A connection
  lost mid-reply*）。（在一个 agent 回合里，每一轮都是一个自己的请求：一个 tool
  轮次回来发现网络没了就等待，而回合从它所在的地方接着走。）另外 `/settings` 的
  **Error retry** 为 `0` 意味着*永不重试*，等待也包括在内：`could not reach
  {host}: …` 这个错误立刻浮出，用传输层的原话。
- **Esc 是出路**，任何等待都一样：睡眠是 `sleep_cancellable` 的 50 ms 切片，所以
  打断立刻落地，而取消是静默的。然后照常适用普通的打断规则
  （`docs/input/interrupt.md`）：什么都没流出、队列里也没有东西时，这次提交会被
  **撤销** —— 消息回到 composer，没有 notice —— 所以一个在离线时被放弃的回合，
  之后再重发不花什么代价。

## 屏幕上出现什么

屏上的一切都来自同一份纯状态，
`TurnStatus::offline: Option<OfflineInfo { host, attempts, began }>`
（`App::set_offline`，来自事件；subagent 用 `AgentRun::offline`、
`agents::AgentRun::apply`），并在主机一答复的那一刻清除 ——
`TurnStatus::recovered`，由每个内容分支（`push_chunk`、`push_thinking`、
`push_tool_call_progress`）、由各轮次的公告（`start_tool_batch`/`start_tool`：
那些把 tool call 整份送达的 wire 在它之前不流任何 delta）以及由 `set_retry`
（一次重试意味着主机答复了）调用。`retry` 与 `offline` 永远不会同时为 `Some`：
最新的公告就是状态。

- **动词。** `ui::styled_status_line` 在动词槽里穿上 `OFFLINE_VERB` ——
  `Waiting for internet` —— 压在回合自己的动词*之上*，也压在一个 task 的
  `activeForm` 之上：activeForm 说的是模型在做什么，而主机不在的时候，它除了
  等待什么也没在做。微光是惯常那个，只是静息色换成 `status_offline_color()`
  （主题的 `warning`，也就是重试分句的琥珀色），所以这个状态既能靠词、也能靠色
  分辨，而回合底下那个动词仍在持续轮换 —— 提交下来的摘要仍读作
  `Worked for 3m 2s`。
- **分句。** `offline for 45s`，在重试分句的槽位和颜色里。它是一次减法，不是第
  二个时钟：`OfflineInfo::began` 是第一条 `Offline` 公告落地时回合时钟的读数
  （`TurnStatus::elapsed`，每帧由绘制 tick 注入），
  `OfflineInfo::duration(elapsed)` 就是那是多久以前。后来的公告把 `attempts`
  往前推，并保留 `began`。边界学不到新东西，也不保存新的 `Instant`；一个 agent
  的 `began` 同样来自它自己每帧的 `runtime`。
- **那一行。** `ui::offline::offline_lines` 在 tool 输出挂靠的装订边里构造
  `  ⎿  ((·))  No connection to {host} — trying again · {n} attempts`：当下那
  一刻的涟漪帧（`OFFLINE_FRAMES`，六个等宽帧，彼此相隔一个
  `OFFLINE_FRAME_INTERVAL`，由与状态行同一个时钟相位驱动，所以两者同步动画），
  圆点是琥珀色，每一圈向外淡出 `OFFLINE_RING_FADE` 那么多、趋向暗色，然后是那句
  话，在它自己那一列之下按宽度换行，主机名是亮的。这句话刻意很短 —— 动词已经
  说了回合在等待 —— 所以它在 80 列里、挨着一个寻常主机名也能占一行；一个很长的
  主机名或一个窄终端要花行数，从不吃掉尾部。它经 `ui::hang_rows` 占据状态行下方
  的槽位 —— `live_height`/`live_layout`/`cursor_position` 据以定尺寸的那一个和，
  如今是清单的、tip 的和这一行的 —— 所以预留的行与 `strip_lines` 绘制的行在构造
  上就一致。这一行显示时 tip 让位（`App::tip`：网络中断旁边的一条快捷键提示是
  噪音，而请求通过之后同一条 tip 会不损地回来）；task 清单不让 —— 一份计划的
  行和网络的状态都值一行，而 `hang_rows` 把它们相加。
- **只存在于 live**，像 tip 和重试分句一样：什么都不提交，rollout 和 Ctrl+O 的
  transcript 永远看不到它，一次尺寸变化会从状态重建它。在一个 agent 会话视图里，
  条是那个 agent 的，所以 `App::offline` 在那里回答被查看 agent 的等待，在其他
  任何地方回答主回合的。

## 本地服务器不是什么

被 `127.0.0.1:11434` 拒绝的连接不是网络：那是 `ollama serve` 没在跑。Ollama
的 wire 早把这种拒绝变成了建议（`ollama::connection_advice`，
`docs/providers/ollama.md`），而在它上面永远等下去，会藏起那一句能修好它的
话。所以 `OpenAiClient::explain` 把一个 `Unreachable` 映射回 `Http(advice)`
—— 有界重试，建议几秒内浮出 —— **只**在 base 位于本机时
（`ollama::is_local_base`：一个回环地址或 `localhost`）。Ollama 的云服务，
以及别处的 `OLLAMA_HOST`，仍然保持等待。

其他每一个**无 key** 的 provider（`auth = "optional_key"`：LM Studio，一个
llama.cpp 或 vLLM 块）同样是用户自己跑的服务器，所以它在任何 wire 上都得到同一
个例外，措辞与 `/login` 的连接测试一样（`local::refused_advice`，
`docs/providers/local-models.md`）：`Nothing is answering at
127.0.0.1:1234 — is LM Studio's server running?`。同一个服务器在另一台机器上
仍然保持着等待，一个**要 key** 的 provider 也一样，即使在 `localhost` 上：
在那里，
一次拒绝是托管服务前面的一个转发代理，而它会自己回来。那一种显示
`Waiting for internet…` 压在 `No connection to localhost:8080` 之上，后者点明了
真正不在的是什么（Phase 129 的 `flaky` 桩就是其中之一）。

## 回复中途掉掉的连接

上面的等待针对的是一个从未送达的请求。一条连接也可能在回复*底下*死掉 —— Wi-Fi
换了、VPN 重连了、笔记本睡眠了、一个 NAT 忘了这条流 —— 而那过去会让回合死在这
一行上

```
request failed: request or response body error: error reading a body from connection: Connection timed out (os error 110)
```

`os error 110` 是内核的 `ETIMEDOUT`，在 Linux 上正是 `reqwest` 0.12 自己的
socket 默认值把一条静默路径变成的样子：`SO_KEEPALIVE` 在 15 s 没有字节之后开始
探测，而 30 s 的 `TCP_USER_TIMEOUT` 放弃那些无人应答的探测 —— 或者任何无人应答
的数据 —— 所以一条不再答复的路径在约半分钟内被宣告死亡。一个只是慢的 provider
在 TCP 层仍然会应答（它改为停滞到 120 s 的 `NET_OP_TIMEOUT`：
`operation timed out`），所以这个错误意味着路径本身没了。一次重置
（`os error 104`）、一条没有 `close_notify` 就关掉的 TLS 流，以及一个被截短的
chunked 请求体，都以同样的方式到达流：一次失败的 body 读取，`LlmError::Http`
（`LlmError::body_read`）。

有界重试过去一旦有任何东西流出就拒绝它们全部 —— 重发会把用户已经看到的东西重复
一遍 —— 所以回合带着部分内容和红色 notice 一起死掉，无论一个 agent 任务走到了
多远。而能活到遇见一次网络变化的那种流，恰恰都是长流：跑几分钟的推理阶段、一次
整份文件的 `write`。

所以内容已经流出之后的一次传输失败，如今与它之前的一次一样走有界重试，而重复的
问题从另一头解决：**重试在下次尝试的第一个字节到达之前，撤回失败尝试所流出的
东西。**

- **策略**（`retry::next_step`）。一个已经发出了内容的 `LlmError::Http`，在预算
  还够时是 `RetryStep::Retry` —— 同样那个 `retrying {n}/{max}`、同样的退避、
  同样的请求再发一次，模型重新作答。一条在回复中途过了 `NET_OP_TIMEOUT` 还没
  出声的流也算：两分钟没有字节，这条流与一条被重置的流一样没了。回复中途失败的
  其他东西都不重启：provider 自己在带内发来的错误（`LlmError::Api`，状态码 0）
  与一次解码失败照旧浮出，取消也一如既往地停下。每次重启都从第一个 token 重新
  生成那一轮，所以它要花那份有界预算 —— `Error retry` 限制一轮的重启次数，而 `0`
  仍然意味着永不。一个连不上的重启请求与任何 `Unreachable` 一样被等待，所以一次
  把网络一起带走的掉线会显示 `Waiting for internet…`，并在网络回来后接着走。
- **每次尝试按它自己的输出评判**（`retry::run_attempts`）。`emitted` 过去跨尝试
  累积 —— 一旦有字节流出过，就再也不重试了。一次被撤回的尝试没留下任何可重复的
  东西，所以这个标志是尝试自己的：重启尝试第一个字节之前的 `503` 照常重试。
- **`StreamEvent::Retrying` 作废失败的尝试。** 流上的每个消费者都在事件落地的
  那一刻丢掉那次尝试的输出 —— 有界重试在内容之前就一个事件（那时没什么可丢），
  内容之后的重启也用它：
  - `App::withdraw_attempt`（主回合）：流出的回复文本（流本身保持打开）、该次
    尝试打开的一个思考阶段 —— 丢掉，绝不落定成一个 `Thought for …` cell ——
    它正在生成的一个调用（`write` 预览）、到目前为止的 `/compact` 摘要，以及该次
    尝试记录的历史条目：一段在某个阶段或预览打开时定稿的文本，以及一个它落定的
    思考。一次尝试不会记别的，而撤回绝不会弹出任何其他种类的条目，所以一个错误
    的标记最多也只能拿回太少。`history_generation` 随之移动（Ctrl+O 缓存、
    rollout 的截断改写）。
  - `AgentRun::apply`（一个 subagent）：同样那一套，作用在 agent 自己的
    transcript 和条上。
  - `agents::RunRecorder`（subagent 的转发器）：它作为 agent 答案正在收集的
    文本，所以 lead 永远不会拿到与重新生成的回复粘在一起的部分内容。
- **尝试从哪里开始**是唯一必须知道的东西，而它是派生出来的，不是公告出来的：
  `App::track_attempt`（每个回复事件都调用它）与 `AgentRun::apply` 在自任何
  其他种类的事件以来的第一个尝试输出片段处（`StreamEvent::is_attempt_output`：
  一个 chunk、一个思考事件、一个调用片段）记下历史长度。每个回合边界都是这样的
  事件 —— 一个 usage 帧、该轮的调用、一次 tool 解析、一条被 steer 的消息 ——
  所以标记绝不会伸回更早那一轮的 cell 里。
- **backend 不去碰一个开着的阶段。** `stream_round` 过去在每次尝试之后关闭一个
  思考阶段，失败的也算；现在它只在一次完成的流之后关闭。失败尝试的阶段由消费者
  来落定（`Error` 分支和 Esc 会做，一如既往）或丢弃（重试），所以一条在思考中途
  掉掉的连接，绝不会先提交一个 `Thought for …` cell 再把它拿回去。
- **屏幕。** 条在下一帧丢掉所有曾处于 live 的东西 —— 前沿行、`● Thinking…` 块、
  `write` 预览。已经到达 scrollback 的行 —— 回复已提交的行
  （`StreamRender::has_committed`）、一个已定稿的片段、一个已落定的思考 —— 由
  每次历史回退所用的 purge 重建拿回（`repaint_conversation`；被查看 subagent 的
  `repaint_agent_view`）。在 alternate-screen overlay 下，重建等到返回时进行
  （`overlay_resized`），而在 agent 会话视图打开时，主对话根本不在屏幕上，所以
  离开视图会重建它。一条暗色 toast 说明文本为什么走了 ——
  `Connection lost mid-reply — starting it over` —— 而琥珀色的 `retrying 1/3`
  说明正在发生什么。这条 toast 只在文本走掉的那个视图上抬起：主回复的绝不在
  一个 agent 的会话视图之上抬起，因为在那里它会把责任归到 agent 头上。
- **重启期间的 Esc** 是一次普通的打断。当被撤回的尝试就是那个回合所产出的全部
  时，回合什么也不剩，撤销把这次提交交回 composer
  （`docs/input/interrupt.md`）。

## 保持不变的

- 有界重试：`is_retryable`、`retry_backoff`、`retrying n/N` 分句和
  `Error retry` 旋钮，对一台会答复的主机，行为与以前完全一样。
- 一次停滞：一个接受了连接、却在 header 上坐过 `NET_OP_TIMEOUT`（120 s）的
  provider 是一次有界的、计入数的重试 —— 它答复了，只是很慢。
- provider 在流*内部*报告的一次失败（`200` 上的一个 `error` 帧）、一个不解码的
  请求体，以及一条干净结束的流，都照旧浮出或照旧完成：只有一条在回复底下断掉的
  连接才会重启它。
- `/model` picker 的抓取不重试；它的失败现在读作 `could not reach {host}: …`，
  而不是 `request failed: error sending request for url (…)`。

## 测试

- `llm::tests`：一次被拒的回环连接分类为 `Unreachable`，点名
  `127.0.0.1:{port}` 且保留原因链（离线安全 —— 内核立刻拒绝它）；一个在任何
  socket 之前就失败的请求（不支持的 scheme）保持 `Http`；一次 body 读取的
  `io::Error` 点名它的原因链；`host_label` 只保留显式端口；`Display`。
- `llm::openai::tests`：一个发往被拒端口的**流式**请求是 `Unreachable`（那个
  smoke 发现，离线复现）；一台会答复探测的主机上的发送失败保持 `Http`；一台连
  不上的主机则是 `Unreachable`，用探测自己的原因；一次 `reqwest` 归咎于连接的
  失败不需要探测。
- `llm::retry::tests`：`next_step` 在每一种预算状态下都以 `AwaitConnection` 回答
  `Unreachable`，内容流出之后或关掉重试时为 `Proceed`；退避翻倍到上限；driver
  为每次失败的连接公布一个 `Offline` 并带累计数，不在它们身上花重试，在一次中断
  之后从 `1` 开始那有界重试，并在一次答复之后重新开始中断计数，在等待中途取消
  时静默停下，并在关掉重试时浮出错误。一次回复中途掉掉的连接（用户自己的
  `os error 110` 消息）在有界重试上重启、花掉预算并在关掉重试时停下，而内容之后
  的一个带内错误帧或一次解码失败会浮出；driver 把被丢的回复再发一次，按它自己的
  输出评判一次重启的尝试（重启之后的 `503` 会被重试），并在每次重启也都掉线之后
  浮出传输层的原话。
- `llm::backend::tests`：`stream_round` 对着一个回环服务器，它的第一个答案流出
  一个思考并在请求体中途被切断 —— 重启发生，第二个答案跑完这一轮，而没有
  `ThinkingEnd` 关闭失败尝试的阶段。
- `stream::tests::event`：哪些事件是尝试输出。
- `app::tests::turn`：`withdraw_attempt` 拿回流出的回复（保持流打开）、该尝试
  记录下的已定稿文本与已落定思考（一次非追加的代际推进）、一个开着的思考和一个
  `write` 预览、一份 `/compact` 摘要，以及在任何内容之前什么也没有；它绝不伸回
  越过一个回合边界 —— 哪怕是一个什么都没记的边界 —— 也绝不弹出一个尝试无法记录
  的条目；一个被撤回的思考不是该轮 usage 帧的吸附目标。`app::tests::agent`：
  `withdraw_agent_attempt` 汇报走了什么并标记名单。
- `agents::tests`：`AgentRun::apply(Retrying)` 撤回 agent 的部分内容、它已定稿的
  文本与思考、它开着的思考与时钟、它做了一半的调用、被撤回文本设下的
  `Agent: …` 更新，以及活动行所读的段落标记；`RunRecorder` 只把通过去的回复交给
  lead。`ui::tests::stream_render`：`StreamRender::has_committed`。
- `llm::openai::tests`：一个被拒的本地 Ollama 是建议，一个远端的是等待；
  `ollama::is_local_base`。本机上一个被拒的无 key 服务器是 `/login` 测试的那句
  话，而同一个服务器在别处、以及回环上一个要 key 的 provider，都是等待；
  `local::refused_advice`。
- `app::tests::turn`：`set_offline` 记录主机、计数和时钟读数；后来的公告保留
  起点；
  每个内容分支和两个回合公告都结束等待；`retry` 与 `offline` 从不一起出现；空闲
  时是空操作。`app::tests::tips`：tip 在等待下隐藏，并在之后不损地回来。
  `app::tests::agent` / `agents::tests`：一个会话视图报告被查看 agent 的等待，
  并按它的 runtime 打时间戳。
- `ui::tests::status`：动词、它的琥珀底色、分句、activeForm 失去槽位。
  `ui::tests::offline`：那一行的文本与单数计数、各帧等宽、涟漪的序列、循环与
  淡出、文本列之下的换行、主机名点亮、条预留的行恰好等于它绘制的行（以及区域在
  有内容时回落）、agent 视图那一行。
- `scripts/smoke.sh` 的 **Phase 129** 用真实二进制对着一个指向无人监听的回环
  端口的 provider 文件跑：回合显示 `Waiting for internet…`、`offline for` 分句
  在计数、那一行点名 `127.0.0.1:{port}` 且尝试数在跳动；然后在那个端口上起一个
  桩的 chat-completions 服务器，回复就流出来，回合像任何其他回合一样落定。第二
  半用 Esc 打断等待，并发现消息回到了 composer。想对着真实 provider 看它，就在
  `HTTPS_PROXY=http://127.0.0.1:1`（每次连接都被拒）下运行 —— 或者待在一个你能
  随时停掉的本地转发代理后面。
- `scripts/smoke.sh` 的 **Phase 145** 演练一次回复中途掉掉的连接：一个桩，它的
  第一个答案流出一个思考与两行已完成的文本，然后在请求体中途关闭 socket。回合
  显示 `retrying 1/3` 和那条 toast，而思考与那两行已从屏幕、scrollback 和
  rollout 上消失，第二个答案把整份回复流一遍。第二半在一个前台 subagent 的会话
  视图开着时丢掉它的回复：视图以同样的方式撤回它，而 lead 的下一个请求只带该
  agent 的整份回复作为它的答案。

## 各个部件住在哪里

| 部件 | 位置 |
| --- | --- |
| 分类、`LlmError::Unreachable`、`host_label`、原因链 | `src/llm/mod.rs`（`LlmError::transport`），应用于 `openai.rs`、`models.rs`、`copilot.rs`、`chatgpt.rs`、`claude.rs` |
| 策略：`Attempts`、`RetryStep::AwaitConnection`、`offline_backoff`、driver、对连接已断的回复的重启（`is_lost_connection`） | `src/llm/retry.rs` |
| 失败尝试上把开着的思考留给消费者 | `src/llm/backend.rs`（`stream_round`） |
| 一次重试作废什么：`StreamEvent::is_attempt_output` | `src/stream/event.rs` |
| 撤回：`App::track_attempt`、`App::withdraw_attempt`、`Withdrawn`、`CONNECTION_LOST_NOTICE` | `src/app/turn.rs`；`App::withdraw_agent_attempt` 在 `src/app/agent.rs` |
| subagent 的撤回，以及它的答案的撤回 | `src/agents.rs`（`AgentRun::withdraw_attempt`、`RunRecorder`） |
| 把被撤回的行从屏幕上拿回来 | `src/tui/stream.rs`（`retract_withdrawn_rows`）、`src/tui/agent.rs`；`StreamRender::has_committed` 在 `src/ui/stream_render.rs` |
| 本地服务器的例外 | `src/llm/ollama.rs`（`is_local_base`）、`src/llm/local.rs`（`refused_advice`）、`src/llm/openai.rs`（`explain`） |
| 事件 | `src/stream/event.rs`（`StreamEvent::Offline`） |
| 状态 | `src/app/status.rs`（`OfflineInfo`、`TurnStatus::offline`、`set_offline`、`offline`、`recovered`），`src/agents.rs`（`AgentRun::offline`） |
| 边界分支 | `src/tui/stream.rs` |
| 状态行 | `src/ui/status.rs`；常量和颜色在 `src/ui/theme.rs` |
| 那一行 | `src/ui/offline.rs`；由 `src/ui/layout.rs`（`hang_rows`）加进条，由 `src/ui/live.rs` 绘制 |
| tip 让位 | `src/app/tips.rs` |
