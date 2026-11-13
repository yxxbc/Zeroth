# 状态指示器（codex / Claude-Code 风格）

回合进行期间有一条实时状态行，另外还有一条常驻的“done”摘要 —— 仿照
openai/codex 与 Claude Code 里那个 spinner 行而来。

```
> Hi                                                        (committed)

● Happy to help! …                                          (streaming preview — existing)

⣤⣀⣀⣀⣀⣀⣀⣀ Working… (1s · ↓ 100 tokens · esc to interrupt)    (live status — the ball hops, verb shimmers)
                                                            (blank gap so the status clears the box)
─────────────────────────────────────                       (input box)
❯ ▏
─────────────────────────────────────
```

结束时，状态行被一条提交进 scrollback 的暗淡摘要取代，摘要以**状态行刚才
所戴动词的过去式**开头 —— `Working…` 收尾成 `Worked for …`：

```
> Hi
● Happy to help! …                 (the finished reply)
Worked for 20s                     (NEW: committed turn summary)

> Thanks                           (the next turn — "Worked for 20s" stays put)
```

文档其余部分把那个单元格称作“`Done for Ns` 摘要” —— 那是动词成对之前的旧
名字。它如今说的是该回合自己那个状态动词的过去式（见*动词：轮换与摘要*）；
`Done` 只作为旧 rollout 记录下来的动词留存。

## 显示什么，什么时候显示

实时行是
`(●•·   ) {verb}… ({elapsed}[ · {arrow} {n} tokens][ · retrying {a}/{max} | · offline for {d}][ · Thinking for {m}] · esc to interrupt)`
—— 其中 `{elapsed}` 和 `{m}` 由 `ui::format_elapsed` **人化**（见下面的
*耗时*），所以短回合读作 `3s`，长回合读作 `1m 30s` / `1h 5m`：

| 阶段                 | 状态行                                                                        |
|----------------------|-------------------------------------------------------------------------------|
| 流开始前的停顿       | `(●•·   ) Working… (1s · ↑ 7 tokens · esc to interrupt)`                       |
| 流式输出文本         | `(•●    ) Working… (3s · ↓ 100 tokens · esc to interrupt)`                     |
| 流式输出 + 思考      | `(·•●   ) Working… (3s · ↓ 150 tokens · Thinking for 0s · esc to interrupt)`   |
| 生成一个 tool call   | `( ·•●  ) Working… (3s · ↓ 170 tokens · esc to interrupt)`（该 call 流式生成时计数在跳） |
| 一个 tool 结果之后   | `( ·•●  ) Working… (4s · ↑ 200 tokens · esc to interrupt)`                     |
| 重试一次失败         | `( ·•●  ) Working… (5s · ↑ 42 tokens · retrying 2/3 · esc to interrupt)`       |
| 等网络               | `( ·•●  ) Waiting for internet… (2m 3s · ↑ 42 tokens · offline for 1m 10s · esc to interrupt)` —— 动词与从句为琥珀色，压在一行 `⎿ ((·)) No connection to {host} — trying again · N attempts` 之上（`docs/providers/offline.md`） |
| 结束（已提交）       | `Worked for 20s` —— 该行最后所戴动词的过去式                                    |
| 被打断（Esc）        | 红色的 `Conversation interrupted` notice，随后是同一份摘要，压在状态行最后显示的计数之上 —— `Worked for 8s · ≈24.3k tokens`，`≈` 标出没有任何 usage 帧报告过的那一份（见 `docs/input/interrupt.md`） |

回合以一段**流开始前的停顿**开场（backend 在第一个 chunk 之前要等
`STARTUP_DELAY`，3 秒 —— 所以任何文字出现之前，指示器就已经在肉眼可见地
*working*）：刚发出的**用户消息在开头就以 `↑` 箭头计入统计**（上传的输入，
与折回来的 tool 结果同类），于是停顿里显示 `↑ N tokens`，计时器也在走。第一个
流式 chunk 把箭头翻成 `↓`。这段停顿是 `DummyAi` 的（`stream::STARTUP_DELAY`，
可用 `ZEROTH_STARTUP_DELAY_MS` 覆盖 —— smoke 测试跑得短，一个 phase 那么长）；
真实 backend 自己的延迟扮演同一个角色。

- **spinner** —— 行首是 session 的**spinner 样式**（`docs/ui/spinner.md`）；默认
  是 `gravity`，一颗沿盲文轨道跳动的球。本节描述的是**彗星**，那是还没有目录
  之前状态行所戴的样式，也是下面 `SPINNER_*` 帧以及每一张 `(●•·   )` 草图
  仍然指的形状 —— 一次 Larson 扫描：一个白色粗体的头，在暗淡的括号墙之间拖着
  一条两格渐隐的尾，每 80 ms 一帧（`(●•·   )` → `(•●    )` → … →
  `(   ·•●)`（右墙）→ `(    ●•)`（弹回时尾从身后甩过去）→ 再穿回
  `(●•·   )`（左墙，紧贴 `(` —— 最左那格被用上，没浪费）；**十**帧全部定宽，
  所以它后面没有任何东西会抖）。与微光一样，`ui::spinner_spans` 是纯的 —— 帧
  序号从边界喂进来的 `elapsed` 推导出来。
- **verb** —— 一个 whimsical 的词（`Working`、`Cooking`……），来自
  `STATUS_VERBS`，其中每一项是一个动词的两种形式
  （`StatusVerb { working, done }` —— `Working`/`Worked`、`Brewing`/`Brewed`）。回合的行开在上一次行所戴动词之后
  的那一项上，并**每 `VERB_ROTATION`（30 秒）走到下一项**；已提交的摘要以状态
  行最后所戴那个动词的过去式开头。确定性的 —— 没有 RNG，可以像
  `dummy_response` 那样测。见下面的*动词：轮换与摘要*。动词歇在**页面标题的
  盛装**里 —— `/mcp` 与 `/skills` 喜庆栏那条粗体渐变，铺满动词自己的宽度 ——
  一道明亮的**微光波**从它身上扫过（见下）。
- **elapsed** —— 从回合提交算起的时间，由纯函数 `ui::format_elapsed(secs)`
  **人化**：不足一分钟是裸秒数（`0s`、`45s` —— 实时秒数一直在跳，好让跑着的
  计时器永不像冻住了），不足一小时是 `{m}m {s}s`（`1m 30s`、`59m 59s`），过了
  一小时是 `{h}h {m}m`（`1h 0m`、`1h 5m`、`25h 0m` —— `h` 无上限地长）。它是
  **双单位合并**形式，与 `session::relative_age` 那个单单位的静态年龄标签
  （`2m`、`1h`）不同；同一个 helper 也格式化 `Thinking for {m}` 从句和
  `{done verb} for {n}` 摘要，所以三者读起来一样。即使没有事件到达，值也在
  前进（回合活跃时绘制分支会重新挂上动画帧 —— 见微光那一节）。
- **tokens** —— 整个回合的单一累计统计（**用户的输入**消息、回复文本、
  **reasoning delta**、**模型正在生成的那个 tool call**（它流式到来的
  `name`/`arguments` 片段 —— 与 reasoning 一样计数，好让统计在*模型产生这个
  call 期间*就在跳，在它运行之前），以及 tool 输出），由 app 侧一个与词汇表
  无关、与 provider 无关的估算计数（计数接缝是 `app::count_tokens` →
  `tokens::estimate` —— 一遍扫过文本，像任何 byte-pair tokenizer 那样给词、
  数字、缩进和标点定价，在代码、英文、JSON 与 shell 输出上与常见编码器
  相差几个百分点，且没有任何东西需要加载或常驻；见
  `docs/architecture/token-estimate.md`）。每一份输入/chunk/tool 输出在到达时
  就被估算并**加**进统计（每个来源各带 `↑`/`↓`）；跨 chunk 边界的一片可能把
  实时总数推高一点点，超出一份整缓冲估算 —— 对一个状态估算来说没问题，而且
  它保持 O(text)、非阻塞。回合中途**从不重置**。只有当它为 0 时才省略 ——
  既然输入现在开头就计，那就只是 `count_user_input` 跑之前的第一帧。
  **真实 backend 的轮末 usage 帧把统计吸附到 provider 自己的记账上**
  （`StreamEvent::Usage` → `App::apply_usage` —— 估算从没见过 system prompt
  和重发的上下文，所以这次吸附通常把它往上跳），下一轮的估算再叠在上面；
  大总数人化渲染（`ui::format_token_count` —— `8.1k`），已提交的摘要追加
  `· {n} tokens ({c} cached)`。见 `docs/providers/prompt-caching.md`。
- **arrow** —— `↑` 表示**上传的** token（回合开始时用户的输入，以及折回来的
  tool 结果），回复（或它的 reasoning）在流时是 `↓`。于是一个回合以 `↑` 开场
  （流前停顿里那份已计数的输入），第一个 chunk 一到翻成 `↓`，每个 tool 之后
  又回到 `↑`；两种情况下计数都在继续长。
- **retrying {a}/{max}** —— 以**琥珀色**显示（`status_retry_color()`，它与那个
  占同一槽位的 offline 从句是仅有的两个用这套警示色的地方），*只在一次失败的
  请求正在被重试时*：任何内容流出之前连接/发送失败（或者回来一个瞬时的
  `429`/`5xx`），或者回复中途连接丢了、本轮从头开始、它的半成品被撤回 ——
  也就是说真实 backend 正在重连（`llm::retry`，见 `docs/providers/llm.md`、
  `docs/providers/offline.md`）。`App::set_retry` 由
  `StreamEvent::Retrying` 设置它；下一个流式 chunk 清掉它（请求恢复了）。
  dummy 从不重试。
- **Thinking for {m}** —— *只在正在思考时*显示（`{m}` 与 elapsed 一样由
  `format_elapsed` 人化）；思考一结束就撤下。它穿调色板的**紫色**
  （`status_thinking_color()`），在这块屏幕上紫色意味着 *reasoning*：页脚的
  思考模式也穿它（`docs/ui/footer.md`），于是一份努力和它换来的思考读作同一个
  事实。每个主题都把它与暗淡的度量项、琥珀色的重试从句区分开
  （`the_thinking_clause_wears_the_thinking_colour`）。
- **esc to interrupt** —— 收尾从句，状态行在显示时它始终在：codex 给 Esc
  打断留的可发现性提示（`docs/input/interrupt.md`）。被打断的回合提交红色的
  `Conversation interrupted` notice，并在其下提交下面那份摘要，用的是状态行
  当时显示的耗时与计数 —— 否则 Esc 会抹掉的那两个数字
  （`docs/input/interrupt.md` 的*被打断的回合留下的回执*）。backend 出错则只在
  红色错误上收尾。

在状态行之下，回合一旦跑满**三秒**，就挂上 Claude Code 那条暗淡的
**spinner tip** ——
`  ⎿  Tip: Press ctrl+o to see the whole transcript and every tool's output`
—— 一个值得知道的按键或命令，每个回合取目录里的下一个，长回合里每三分钟换
一条；它随回合生灭，从不提交，让位给 task 清单，并可在 `/settings` →
**Show tips** 里关掉（`docs/ui/tips.md`）。

## 宽度：状态行被夹取，摘要换行

回合的两端在窄终端上退化方式不同，各自按它*是什么*来。**状态行被夹取**到
宽度，以暗淡的 `…` 收尾（`ui::clamp_spans`，经 `status_line`/
`status_line_with_verb` 穿下去）：它按设计就是一条动画条带行 —— `STATUS_ROWS`
是固定的，它的 spinner/微光 span 带着每帧的颜色，那些颜色绝不能进 scrollback
—— 所以尾部那些从句（琥珀色的重试警告、思考从句、esc 提示）是可见地让位，
而不是在缓冲区边缘被无提示地裁掉。**已提交的摘要换行**（`summary_lines`
按宽度做词换行）：它那些 token/缓存/shell 从句是回合在永久 scrollback 里的
回执，多几行不花什么 —— 旧的那份单行渲染写在 usage 帧的 token 从句还没把它
撑破之前。

## 不纯的部分住在哪里（边界，不是纯核心）

时间是不纯的，所以 —— 与 `docs/ui/timestamps.md` 完全一样 —— 它留在
`main.rs` 里：

- 循环持有 `turn_start` / `thinking_start` 两个 `Instant`；
- 回合活跃时，绘制分支会**重新挂上**下一个动画帧（`schedule_frame_in(32ms)`
  —— codex 的状态组件节奏），于是即使是没有事件的停顿（一次 tool 运行、一段
  思考），计时器也在走、微光也在扫；这条链从 Submit 按键那一帧播种，并在回合
  结束后第一次绘制时自行停下；
- 每次绘制之前，循环把算好的 `elapsed` / `thinking` `Duration` 通过
  `App::set_status_times` 写到实时状态上。

纯的 `App` 只持有*不是*时间的东西：动词的行走（状态行戴哪个动词是光标与注入
的 `elapsed` 的纯函数）、它下面 tip 的行走（显示哪条 tip 由 tip 光标在同一个
注入时钟上抽出 —— `docs/ui/tips.md`）、token 统计，以及 `↓`/`↑` 箭头。
`ui::status_line(&TurnStatus)` 是一个纯格式化器，它读那个 struct（带着边界
给的时长）—— 用显式值做单元测试。

## 状态（App）

- `TokenArrow { Down, Up }`。
- `RetryInfo { attempt, max }` —— 一个实时重试指示（见
  `docs/providers/llm.md`）。
- `OfflineInfo { host, attempts, began }` —— 一次实时断线等待
  （`docs/providers/offline.md`）：`began` 是中断开始时回合时钟的读数，所以
  `offline for Ns` 就是用注入的 `elapsed` 做一次减法。
- `TurnStatus { verb, done_verb, rotates_from, tokens, arrow, elapsed, thinking,
  shell, retry, offline }`（`verb`/`done_verb` 是同一个 `StatusVerb` 的两种
  形式，一起移动；`rotates_from: Option<usize>` 是该行开场所处的 `STATUS_VERBS`
  索引，仅当它的动词会轮换时存在，固定动词时为 `None`。`elapsed: Duration` /
  `thinking: Option<Duration>` 每帧由边界写入；`thinking` 只在思考时为 `Some`。
  用 `Duration` 而不是整秒，好让一个值同时驱动显示的秒数、微光的亚秒相位和
  动词轮换。`retry: Option<RetryInfo>` 只在一次失败请求正在被重试时为 `Some`）。
- `App.status: Option<TurnStatus>` —— 从 `begin_stream` 到回合结束为 `Some`
  （`turn_active()` == `status.is_some()`）。
- `App.verb_cursor: usize` —— 下一个回合的行开场所处的 `STATUS_VERBS` 索引：
  某个行所戴最后一个动词的下一个。
- `begin_stream` 创建状态（开在光标所指的动词上并推进它）；
  `set_status_times(elapsed, thinking)` 写入边界的时间并随之轮换动词对，同时
  推进光标；`count_user_input(text)` 紧接着加上用户消息的 token（`↑`），于是
  流前停顿显示的是已上传的输入计数；`push_chunk` 加 token（`↓`，并清掉任何
  `retry`）；`push_thinking` 加 token（`↓`，不碰回复缓冲 —— reasoning 文本去
  思考流自己的缓冲区，`docs/ui/thinking-stream.md` —— 并清掉任何 `retry`）；
  `push_tool_call_progress` 以同样的方式加 token（`↓`，不碰缓冲），对应模型
  *生成*一个 tool call 的过程 —— 由 `StreamEvent::ToolCallDelta` 驱动，即真实
  backend 在本轮批次之前浮出的那些流式 `name`/`arguments` 片段，每一片按它的
  `ToolCallFragment::tally_text` 计（经由 `App::push_tool_call_fragment`，它还
  在 `write` 流式到来时预览它，`docs/ui/write-streaming.md`）；
  `set_retry(a, max)` 设置琥珀色的 `retrying a/max` 从句（来自
  `StreamEvent::Retrying`）；`set_offline(host, n)` 设置 `Waiting for
  internet…` 动词、`offline for` 从句以及状态行下面那一行（来自
  `StreamEvent::Offline`），而内容、一次 tool 轮或一次重试会清掉这两者
  （`TurnStatus::recovered`）；`end_tool` 加 token（`↑`）；`fail_stream`
  清掉状态（一个错误就是摘要 —— 没有“Done”行）；`interrupt_turn(secs)` 也
  清掉它，但先构建出边界记录在 `Conversation interrupted` notice 之下的那份
  回执（`docs/input/interrupt.md`）；`end_turn(secs)` 记录摘要并清掉状态。

## “Done for Ns”摘要

一个新的、有序的历史条目，所以它能扛过 resize，也会列在 Ctrl+O 的 transcript
里（在那里没有时间戳 —— 只有用户消息显示时间戳）：

- `TurnSummary { verb, secs, timestamp }`、`HistoryItem::Summary(TurnSummary)`
  —— `verb` 是状态的 `done_verb`，即该行最后所戴动词的过去式。它的 token 字段
  就是回执（`docs/providers/prompt-caching.md`）：`tokens` 是 usage 帧报告的
  部分，`cached`/`cache_write` 是它们的缓存两半，`estimated` 是估算里没有帧
  报告过的那一份 —— 只有被打断的回合它才非零，那时总数读作 `≈`
  （`docs/input/interrupt.md`）。
- `ui::summary_lines` 渲染一行暗淡、无项目符号的 `"{verb} for {elapsed}"`
  （秒数由 `format_elapsed` 人化 —— `Worked for 20s`、`Generated for 1m 30s`）。
- `App::end_turn(elapsed_secs)`（循环在 `StreamDone` 时调用）推入它，并返回给
  循环去提交进 scrollback。在 `StreamDone` 上，循环先把 viewport 重新坐回空闲
  盒子高度（条带没了）—— 与最后一行回复已经用的那套“在底部冲一遍”的舞步相同。
- 一次保留其输出的 **Esc 打断**也在红色 notice 之下以同样的东西收尾：
  `App::interrupt_turn(elapsed_secs)` 用同一个 `App::turn_summary` 构建它（两端
  一个构建器，只在这里被传入估算的那一份），循环通过同一个
  `Session::commit_turn_summary` 记录并提交它（`docs/input/interrupt.md` 的
  *被打断的回合留下的回执*）。

## 条带几何

盒子之上那条带有两个独立的槽位，每个是一行内容加一段尾随间隙。**回合运行
期间状态行及其间隙是被预留的**，*只有 `!` shell 回合例外* —— 它隐藏 spinner
状态，改在 `⎿ Running… (Ns)` 预览里显示自己的耗时（`ui::strip_has_status`；
见 `docs/input/shell-command.md`）。**预览及其间隙只在有东西可预览时才预留**，
而预览的*高度*是动态的（`ui::preview_rows`）：缓冲非空的回复或正在跑的 `!`
shell 命令预览**一**行，而正在跑的 backend tool 预览它**整个折叠单元格**（换行
后的 `● name(args)` 头*加上*它那行 `⎿ Running…` —— `N` 行，于是一条长命令
不会运行到一半被裁掉；见 `docs/tools/tools.md`）。流前停顿期间（以及第一个
chunk 之前的任何时刻）没有预览，所以那条带就只有状态 + 间隙，**不预留空预览
行** —— codex 也是这样（它的底部面板只在状态可见时预留一条空白的*分隔*行，
从不预留空的内容/预览行；`bottom_pane/mod.rs`）：

```
strip = (preview_rows > 0 ? preview (N) + gap (1) : 0) + (has_status ? status (1) + hang (H) + gap (1) : 0)
      = preview(1) + gap + status + gap   streaming a reply
      = preview(N) + gap + status + gap   running a backend tool (N = header + `⎿ Running…`)
      = status + gap only                 during the pre-stream pause
      = preview(1) + gap only             during a `!` shell run (status hidden)
      = 0                                 idle
```

`H` 是**挂在状态行上的**行数（`ui::hang_rows`）：task 清单
（`docs/tools/task-tools.md`），否则 —— 一旦回合跑了三秒 —— 是 spinner tip
（`docs/ui/tips.md`），再否则没有。两者共用一个槽位、从不同时出现，所以
`live_height`/`live_layout`/`input_box` 把它们之和当作一个 `hang_rows`
参数收下。

`render_live` 把 `status_line` 画在预览槽位之下（`preview_rows + gap`，没有
预览时为 0，于是状态就是那条带的最上一行），*仅当* `has_status`；它下面那行
`STATUS_GAP_ROWS` 保持空白。`live_height`/`live_layout`/`input_box` 收一个
`has_status: bool` 和一个 `preview_rows: u16`（预览内容行数），由持有 `App`
的调用方（`render_live`、`cursor_position`、`main.rs`）用
`strip_has_status`/`preview_rows` 喂进去。

上面那个 `N` 是**做预算的**，不只是量出来的。预览是该区域里唯一能让路的行数
—— 状态行、盒子、band 和页脚各自由这一帧无法与之谈判的状态决定 —— 所以
`ui::preview_budget` 是所有这些都付清之后终端还剩的行，而
`ui::fitted_preview_rows`（把内容的请求夹取到它）才是上面那些调用方实际传入
的东西。没有它时，一个高预览加一个打开的 band 会要求比终端拥有的更多行，而
夹取就从不该拿走的地方 —— composer —— 拿走，这就是被报告过的输入框消失
（`docs/ui/table-streaming.md` 的*预览槽位是做预算的*）。

### 条带活得比 composer 久

只有那两个**模态框** —— tool 权限 prompt（`docs/state/permissions.md`）与
`AskUserQuestion` prompt（`docs/tools/ask.md`）—— 连同 composer 一起替换掉
条带，而且这有道理：回合正*阻塞*在那个答复上，spinner 没什么可报告的。其他
每一个内联视图只替换**composer 本身**，并把条带留在自己上方 —— `/model`
picker、`/login` 流程、`/settings` 菜单（`docs/providers/llm.md`、
`docs/state/settings.md`）以及 ↓ 后台管理器 band
（`docs/tools/background.md`）。它们共用一套几何：`ui::layout` 的
`strip_above_rows` 预留这些行（同一个 `strip_rows` 之和加上排队消息和 toast），
`view_split` 把区域分给条带与视图自己的框 —— 那个框是一个钉在底部的
`Length`，所以太矮的终端会挤条带，绝不挤用户在往里打字的那个视图 —— 而
`render_strip_above` 画的东西与 composer 路径画的完全相同。任何*开着回合旁边*
打开的视图都不许把它藏起来：那正是这四处（先是 band，然后是各个 picker）被
报告过的 bug。

## 动词：轮换与摘要

一个动词贯穿整个回合，对十秒钟的答复还行，对五分钟的 agentic 回合就单调了，
而摘要又从自己的一张表里取动词，于是一条说 `Brewing…` 的行可能落定成
`Wrapped up for 3m 2s`。现在**一张表把每个动词与它自己的过去式配对**
（`STATUS_VERBS`，由 `every_status_verb_pairs_with_its_own_past_tense` 钉住），
状态行随回合行走这张表，摘要则点名该行所戴的动词：

```
⣤⣀⣀⣀⣀⣀⣀⣀ Working… (12s · ↓ 850 tokens · esc to interrupt)
⣤⣀⣀⣀⣀⣀⣀⣀ Generating… (41s · ↓ 3.2k tokens · esc to interrupt)     (30 s in: the next verb)
Generated for 52s                                                    (the verb it wore last)
```

- **每 30 秒**（`VERB_ROTATION`）。一个快答复保留单个动词，于是它的摘要点名的
  就是用户看过的那一个；一个长的 agentic 回合每半分钟换一个新词，足以说明它
  还在走，又不会把视线从上面流出的回复上拉走 —— 动词是这条行上最亮的文字。
  而 30 秒是微光扫过次数的整数倍（每次 2 秒），所以换词总落在一次扫过的开头、
  此时光带仍在文字之外 —— 词在安静的行上变化，绝不在波峰上变
  （`every_verb_swap_happens_with_the_shimmer_band_off_the_word`，它在表上
  往下每一次换词处检查出的动词的最后一帧、换词那一帧和下一帧）。
- **摘要点名用户看到的那个动词。** 轮换发生在 `App::set_status_times` 里，
  它一起移动 `verb` 和 `done_verb`，而边界每次绘制调用它一次，别处不调 ——
  所以它设的动词*就是*屏幕上的动词。摘要读 `done_verb`，而它的秒数来自边界
  自己的回合末时钟：一个在轮换之后几毫秒结束、没有任何一帧画过的回合，仍然
  落定在最后一帧所戴的动词上
  （`the_summary_names_the_verb_the_last_frame_drew`）。
- **行走跨回合继续。** `App::verb_cursor` 始终是某个行所戴最后一个动词的下一个
  —— `begin_stream` 和每一次轮换都推进它 —— 所以下一个回合开在一个新动词上，
  绝不是上面那份摘要刚点名的那个，无论回合以哪种方式结束（打断和出错也算，
  因为光标随行移动，而不是在某个回合末站点移动）。
- **固定动词保持固定。** `!` shell 的 `Running…`（它的状态行反正被隐藏）与
  `/compact` 的 `Compacting…` 命名的是一个操作而不是一种心情：它们的状态带
  `rotates_from: None`，所以时钟从不推动它们，它们也把行走留在最后一个 AI
  回合留下的地方。
- **进行中的 task** 用它的 `activeForm` 装扮状态行（`docs/tools/task-tools.md`）
  —— 一个 app 推导不出过去式的短语 —— 所以摘要保留回合自己的动词，那个动词在
  底下照常轮换，task 一完成就回到行上。
- **agent session 视图**在 agent 自己的时钟上行走同一张表
  （`ui::agent_view_status` 读 `AgentRun::status_verb`）。它的轮换搭的是每帧的
  runtime 注入（`App::set_agent_runtime` → `AgentRun::rotate_verb`），绝不是
  边界在每个 agent 事件落地时施加的冻结（`App::freeze_agent_runtime`，它钉住
  一次落定所记录的准确耗时）：只有被画出的帧才移动动词，所以 agent 的
  `{verb} for Ns` 摘要点名的就是它视图显示过的动词。一段 chat 续写开在最后
  一个动词的下一个，与一个新的主回合一样。
- **`/spinner` picker 的样例行**是一个第一回合的行，所以它开在第一个动词上，
  并在用户浏览时以同样的方式行走这张表（`docs/ui/spinner.md`）。
- **旧 rollout 按记录的样子读出。** 成对之前写下的摘要（`Done`、`Finished`、
  `Completed`、`Wrapped up`、`Ready`）仍然按它们各自的静态量加载
  （`session::LEGACY_DONE_VERBS`）；这个构建完全不认识的动词加载为 `Done`。

## 微光波（移植自 openai/codex）

动词（`Working…`）渲染成**每个字符一个粗体 span**，颜色来自
`ui::shimmer_spans` —— 对 codex `tui/src/shimmer.rs` 的忠实移植：

- 一条升余弦亮度带（`t = ½(1 + cos(π·dist/5))`，半宽 5 个字符）每 **2 秒**
  扫过文本一次，两侧各有 10 个字符的文字外留白，好让它从两端滑进滑出；
- 每个字符按其**静息色**朝主题的 `text` 色混合 `t · 0.9`。codex 让动词歇在
  灰色（`#888888`）上，这条行一直穿到动词改用页面标题的盛装为止：现在每个
  字符歇在 `status_verb_color(col, total)` 上 —— 即该字符在动词自身宽度内的
  显示列上取那条喜庆栏渐变（`header_gradient_at`，mascot 和每个页面标题穿的
  同一道洗色），与 `page_title_spans` 用的同一个键控，所以在波峰之间（扫过的
  大部分时间）动词逐字符地就是 `/mcp` 或 `/skills` 标题的样子，在任何一个
  生效主题里
  （`the_status_verb_rests_in_the_page_titles_dress_in_every_theme`）。
  波峰仍然把每个字符朝文字色抬起，所以这道波读起来像掠过彩色文字的一抹反光。
  一个 task 的 activeForm 也是动词，也穿同样的盛装；offline 动词保留它警示的
  琥珀色（`docs/providers/offline.md`），靠颜色与普通动词区分；
- codex 在渲染器内部读一个进程启动时钟；我们的移植保持**纯**，从边界喂进来的
  `TurnStatus::elapsed`（亚秒分辨率）推导相位，所以测试能在准确相位上钉住这道
  波；
- codex 靠从它的组件每 32 ms 重新挂一帧来做动画；我们的循环在
  `App::turn_active()` 期间从绘制分支做同样的事（取代了早先的 1 秒 ticker ——
  秒数如今也从这些帧前进）。

`SHIMMER_*` 常量（基色/高亮色、扫过、留白、带宽、最大混合量）与其他样式常量
一起住在 `ui/theme.rs`。

**另一个调用方。** 思考流那条实时的 `● Thinking…` 头穿同一道波
（`docs/ui/thinking-stream.md`），扫的相位对着帧脉冲而不是回合的 elapsed。它走
`ui::shimmer_spans_from`，那个函数收的是**静息**色 —— `shimmer_spans` 就是它
配上 codex 的灰底。那个头传的是一个近白的下限，因为静息时发灰对度量项合适，
对头不合适。不管走哪条，这些 span 都是**只活一帧的**：每一个都带着从某一帧采样
的颜色，把它们提交进 scrollback 会把这道扫过的波永远冻在半途。

## 彗星 spinner

状态行开头的 `(●•·   )` 是一颗**彗星** —— 一次 Larson 扫描（codex 没有对应的
东西 —— 它的 shimmer 是一个静止的 `•`）：**十**帧定宽（`SPINNER_FRAMES`），
明亮的头（`●`）每 `SPINNER_INTERVAL`（80 ms）走一格，向右走到右墙
（`(   ·•●)`），再穿回左墙（`(●•·   )`，紧贴 `(`），每 800 ms 循环一次。一条
两格的尾在头后面渐隐 —— `•` 中灰、`·` 落到暗淡的细节灰 —— 每次弹回时在它
身后甩过去（`(   ·•●)` → `(    ●•)`）；被头覆盖的尾格藏在它下面。
`ui::spinner_spans` 把每一帧样式化成每格一个 span —— 白色**粗体**的头、中灰的
`•`、其余一切（淡淡的 `·`、墙、空轨道）暗淡，分隔空格挂在右墙上 —— 并且与
微光一样纯粹从 `TurnStatus::elapsed` 推导帧序号；同一个 32 ms 的绘制重挂驱动
它。定宽的帧意味着 spinner 之后的动词在彗星移动时永不位移。

**彗星是九种样式之一，而且不再是默认。** `/spinner`
（`docs/ui/spinner.md`）在其中挑 —— 盲文轨道的 `gravity` 球（**默认**）与
`wave`，然后是 `sparkle`、`dots`、`blocks`、`pulse`、`bars`、`line` —— 从一个
行与预览都实时动画的 picker 里挑，并持久化在 `spinner.json`。条带通过
`ui::styled_status_line(status, verb, spinner, width)` 为 `App::spinner()` 构建
它的状态行（主回合与 agent session 视图一样）；`status_line` /
`status_line_with_verb` 是它的**默认**分支，也正是没有保存过选择的 session
开场时用的。每种样式都保持彗星的两条规则 —— 各帧定宽、字形单宽 —— 而单格样式
也把它那一个 span 收在同一个分隔空格上，所以无论 spinner 多宽，动词的微光都
从 spinner 之后一列开始。

## 流开始前的停顿（dummy backend）

`DummyAi` 在回放它的第一个事件之前等 `STARTUP_DELAY`（3 秒），于是状态指示器
在任何回复文字之前就肉眼可见地在工作 —— 这段停顿的意义就是把它秀出来。这个
等待是一个可打断的 `nap`（期间按 Esc 会立刻收割线程、什么都不流）。延时可配
（`DummyAi::with_startup_delay`）；`main.rs` 读 `ZEROTH_STARTUP_DELAY_MS`
（好让 smoke 测试跑短、一个 phase 那么长），默认 `STARTUP_DELAY`。它的孪生常量
`ZEROTH_CHUNK_DELAY_MS`（`DummyAi::with_chunk_delay`）设置每一个流式片段之后的
停顿 —— 默认 `CHUNK_DELAY`，45 ms —— 离线 backend 正是靠它模仿一个每秒流几个
token 的模型（`docs/ui/slow-stream.md`）。真实 backend 自己的首 token 延迟扮演
同一个角色。停顿期间条带**只有状态 + 间隙** —— 不预留预览行（回复缓冲为空且
没有 tool 在跑时 `preview_rows` 为 0），所以状态正好坐在已提交用户消息之下一
个空行处，上方没有多出来的空行（那条“不要保留一行”的修复；codex 也不预留空
预览行 —— 见*条带几何*）。

## dummy backend 里的思考

`StreamEvent::ThinkingStart` / `ThinkingEnd` 由 `DummyAi` 在第一段文字之后发出
（于是 demo 会显示 `↓ tokens · Thinking for Ns`），其间 `DUMMY_THINKING` 以
`StreamEvent::ThinkingChunk` 逐词流出 —— 每个事件一个 `THINK_CHUNK_DELAY` 停顿
—— 好让计时器看得见，且 token 统计在整个阶段持续跳动（真实 API 的 reasoning
delta；文本通过 `App::push_thinking` 计，当一个阶段开着时它还喂思考流的实时块
—— `docs/ui/thinking-stream.md`）。循环把这一对映射成
`thinking_start = Some(now)` / `None`；思考的*秒数*只通过 `set_status_times`
到达状态。

## dummy backend 里的 tool call 生成

同一套 demo 手法也覆盖模型*生成*一个 tool call。每个脚本批次之前，`DummyAi`
流出几片 `StreamEvent::ToolCallDelta`（`DUMMY_READ_CALL` / `DUMMY_BASH_CALL` ——
先是 `name`，然后是 JSON `arguments` 片段，各自落在它那个 call 的槽位下）：命名
那片停一个 `THINK_CHUNK_DELAY`，每个参数片停一个 chunk 延迟，于是 call 被产生
期间 token 统计肉眼可见地跳 —— 与 reasoning 一样。循环通过
`App::push_tool_call_fragment` 计它们；这些 call 都不预览任何东西。文件改动
demo 的 `write` 是例外：它按 token 大小的片段从真实参数生成，于是文件流进它的
单元格（`docs/ui/write-streaming.md`）。真实 backend 从它流式到来的
`tool_calls` delta（`openai::Delta::tool_calls`）直接浮出同样的事件，在本轮
批次之前。

## 测试

- `app`：每个状态动词与它自己的过去式配对且只出现一次；摘要是该行所戴动词的
  过去式；动词每 30 秒走一步并绕表回卷；摘要点名最后一帧画过的动词，即便越过
  了一次没有任何帧画过的轮换；下一回合开在最后展示过的动词之后一个，被打断的
  也算；固定动词的回合（`!` shell、`/compact`）从不轮换也不推动行走；agent
  session 视图的动词随 agent 的 runtime 轮换，它的摘要点名其视图戴过的动词，
  一个事件的 runtime 冻结从不移动它，chat 续写则开在下一个动词上；
  `count_user_input` 以 `↑` 加上输入 token（空闲时是空操作），第一个 chunk 把
  箭头翻回 `↓`；token 累积（`↓`）并穿过一个 tool（`↑`，不重置）；思考 chunk
  长统计（`↓`）而不碰回复缓冲；`end_turn` 记录摘要并清掉状态；`fail_stream`
  清掉状态；`interrupt_turn` 交回一份回执，压在状态行显示过的计数之上，帧的
  token 与估算的其余部分分开（`docs/input/interrupt.md`）；`set_status_times`
  写入边界时长。
- `ui`：`format_elapsed` 的分桶（不足一分钟裸秒数、不足一小时 `{m}m {s}s`、
  过了则是 `{h}h {m}m`），以及 `status_line`/`summary_lines` 通过它人化长
  elapsed / thinking / done 时间；每个阶段的 `status_line`（0 时没有 token；
  `↓`/`↑`；`Thinking for`）；offline 动词压过 task 动词并歇在警示琥珀上，
  `offline for` 从句与状态行下面那一行（`docs/providers/offline.md`）；
  流前停顿上 `preview_rows` 为 0，于是 `live_height` 不预留预览行，
  `render_live` 把状态画成条带最上一行（带着 `↑` token，上方没有预留空行）；
  `live_layout` 为每一种 `(streaming, has_preview)` 组合铺排四个区域；spinner
  的彗星头是白色粗体，带一条在暗淡墙之间单调渐隐的尾，每过一个间隔走一帧，在
  右墙处反向（尾在其身后甩过去），并在一个完整周期后循环；动词的逐字符粗体
  span 在每个主题里都歇在页面标题的渐变里，度量项暗淡、思考从句紫色；波的波峰
  把它那个字符朝高亮抬，带外字符停在静息，波峰随 `elapsed` 前进而移动；每一次
  换词都发生在光带离开单词时 —— 出的动词最后一帧、换词帧和下一帧，在表上往下
  每一次换词处（轮换是扫过次数的整数倍）；`/spinner` 的预览像一个第一回合那样
  行走动词；`summary_lines` 是一行暗淡的文字；条带在盒子上方叠预览 / 间隙 /
  状态 / 间隙；`conversation` / `transcript` 渲染一个 `Summary`。
- `stream`：`turn_events` 发出配对的 `ThinkingStart`/`ThinkingEnd`，
  `ThinkingChunk` 严格在配对之内；chunk 仍能重建回复；`DummyAi` 在第一个 chunk
  之前等启动延迟（测试里用短延迟），等待期间取消则什么都不流。
- `session`：每个状态动词的过去式往返一致；旧式摘要动词（`Finished`、
  `Wrapped up`……）按记录加载；不认识的加载为 `Done`。
- `src/tui/`（smoke）：实时行在流式输出时显示 `tokens`，且它与盒子之间隔一行
  空白；回合落定后有一条已提交的摘要。一个等*某个*回合落定的 phase 匹配
  `scripts/smoke/lib.sh` 的 `SUMMARY_RE` —— 表里每一个过去式 —— 或者数摘要
  （`wait_summaries`/`count_summaries`），不管它们戴什么动词，因为超过 30 秒的
  回合结束在比它开场更靠后的动词上；只有那些在同一屏上分辨回合的 phase 才点名
  某个回合自己的动词（`SUMMARY_TURN1`…`3`）。
  `the_smoke_scripts_summary_patterns_follow_the_verb_table` 把它们全部钉到
  `STATUS_VERBS`（`docs/ops/smoke.md`）。Phase 20（更长的启动延迟）：停顿中途
  状态显示 `↑ N tokens` 而没有回复文字，随后回复以 `↓` 箭头流出。

## ……以及在一个装不下它的终端上

超出预览槽位之后，`live_layout` 会饿死条带本身，过去状态行干脆就不画了 ——
spinner、耗时和 token 统计随之消失。现在它会冻进终端真实的 scrollback 里，与
区域显示不下的其他每一条条带行一样（`docs/ui/strip-flow.md`）。
