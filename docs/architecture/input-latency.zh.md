# 输入延迟 —— 那个等着帧地板的按键

日期：2026-11-12

状态（2026-11-14）：**成因 1——全局帧地板——已修复。**
`frame::URGENT_FRAME_INTERVAL`（2 ms）现在夹取按键帧，而
`Session::schedule_for_key` 在粘贴爆发检查之后，只把编辑 composer 的键标为
urgent。成因 2 和 3（O(draft) 的换行和每帧的行求和）仍未解决。下面的测量留作记录：
当时错在哪，以及是怎么找到的。

## 问题

一个竞品自己的基准（`miyu`，15 个 session + 在它旁边跑一个 16 线程编译，取中位数）
把一次按键放在这里的 **7.36 ms**，而对表里最快的 TUI 是 **0.39 ms**。这份代码里没有
什么慢到这个程度：一个空闲的 composer 渲染整个 live 区域只要 **41 µs**。所以另外
7.3 ms 去哪了，而在一台有负载的机器上是什么让这个数字变得*更糟*？

两个各自独立的答案，而它们需要分开，因为其中之一是一个调度决定，另一个是算法性的。

## 测了什么

一个一次性的 `examples/` 探测（已删除；数字就是记录），release 构建、120 列的帧、
macOS、在与开发这个 repo 同一台八核机器上。一次「按键」= `TextArea::insert_char` +
一次完整的 `ui::render_live` 进一个 `Buffer`。

| composer 里的草稿 | 平均 | 最好 |
| --- | --- | --- |
| 空 | 64 µs | 40 µs |
| 300 B 的句子 | 80 µs | 78 µs |
| 4 KB 段落 | 254 µs | 237 µs |
| 20 KB 粘贴的代码 | 398 µs | 332 µs |
| 64 KB 单行 | **1.63 ms** | 1.38 ms |

以及底下的各块（`wrap_rows` 是 composer 在每次编辑时做的缓存填充，因为
`forget_geometry` 会丢缓存）：

| 文本 | `wrapped_rows` | `display_rows` + `cursor_row_col` |
| --- | --- | --- |
| 84 B | 3.4 µs | 3.0 µs |
| 3.8 KB | 5.4 µs | 6.6 µs |
| 19 KB，640 行 | 26 µs | 67 µs |
| 64 KB，一行 | 193 µs | 214 µs |

从数字得出的结论：**一次普通按键花掉的 CPU 远低于 0.1 ms。** 基准里的 7.36 ms 不是
这份代码在计算。

## 成因 1 —— 帧地板是全局的，所以按键排在动画后面（已修复）

在这次测量的时候，`frame.rs::FrameRateLimiter::clamp_deadline` 在**最后一帧发出**
之后、无论谁要求的，都从不比 `MIN_FRAME_INTERVAL`（8.333 ms，120 fps）更早地发出
一帧。来自按键的 `schedule_frame()` 和来自状态 spinner 的
`schedule_frame_in(32 ms)` 走同一条通道，而限速器分不出它们。

于是在一个空闲应用上，一次按键立刻画（64 µs，实测过），但**只要有动画在跑**——
一段流式答复、状态微光（`STATUS_FRAME_INTERVAL` 32 ms）、↓ manager 条带、一个开着
的 `/spinner` 页——一次按键能画的最早时刻就是 `last frame + 8.33 ms`。7.36 ms 的中位数
恰好落在那个窗口里，而在一台有 15 个 session 在流式传输的机器上，那是常态而不是
尾部。

`docs/architecture/async-rewrite.md` 已经说这是刻意选的：那个地板是从
openai/codex 移植来求架构上一致的，而它的 *Why* 注记观察到「对这个应用而言，打字
延迟上的收益是边际的（本来就 ~1 ms）」。在空闲时那是真的，在一个回合流式传输时是
假的。

### miyu 改为怎么做

`miyu` 的 TUI（`proto/tui-demo`、`src/pacing.rs`）只有一个函数，而全部差别都在
它里面：

```rust
pub fn deadline(drawn_at, beat, now, burst, urgent) -> Instant {
    let paced = drawn_at + beat;              // beat = 33 ms
    if urgent { paced.min(now + burst) }      // burst = 2 ms
    else { paced }
}
```

`pacing::urgent` 对按键/重复键和粘贴为真，对流式文本、鼠标移动和尺寸变化为假。于
一次按键**从现在起 2 ms 后**到期，永不晚于下一个节拍；那个先 join 的循环还会在一条
后来的消息其实是按键时，*把一个已经算好的截止时间往前拉*。没有单独的限速器可夹取
——节拍是一个目标，不是一个地板。

这就是 miyu 测得亚毫秒的原因：同样 termbox 级别的工作，不同的调度策略。我们没有丢
掉 7 ms 的渲染；我们把它调度掉了。

### 这个修复花了什么（2026-11-14 实现）

实现的形状就是这里所提议的那个。`FrameRequest` 携带 `urgent`，
`FrameRequester::schedule_frame_urgent()` 坐在两个既有调用旁边，而
`FrameRateLimiter::clamp_request` 为它选 `URGENT_FRAME_INTERVAL`（2 ms）；
`soonest` 本来就保住最早的待处理截止时间，所以调度器本身只需把这个请求传过去。
`Session::schedule_for_key` **先**查 `PasteBurst`，再查纯的
`app::edits_composer` helper（普通字符、Backspace、Delete、Enter），然后才选择
urgent 调用。`clamp_deadline` 保留普通的 120 fps 契约及其锁定测试；新测试覆盖
urgent 地板、空闲、迟到的请求和往前拉。

风险，按它们会咬人的顺序——三条都满足了：

- **一个按住的键变成 500 Hz 的重绘。** 终端按操作系统键速发重复；2 ms 的 urgent
  地板把它封在 ~500 fps。`PasteBurst`（`BURST_CHAR_INTERVAL` 8 ms、
  `BURST_MIN_CHARS` 3）在一段连发被检出之后就要求放宽的帧，于是一个按住的键在三个
  字符之后退回节拍——但爆发检测器必须在选择 urgent *之前*被查，否则一次粘贴会淹没
  终端。
- **测试钉住了当前契约。** `frame.rs` 的单元测试
  （`clamp_pushes_a_too_soon_request_to_the_frame_floor`）断言确切的夹取规则；
  `docs/design.md` 把 120 fps 地板描述成粗粒度的合并器。两者都需要把新规则写出来，
  而不是悄悄改掉。
- **慢终端。** 120 fps 有一部分存在就是为了让一股快速文本流不把 pty 打满；一条也
  重绘*流式条带*的 urgent 路径会毁掉那一点。urgent 调用必须只能从一次按键到达。

## 成因 2 —— composer 的换行是 O(draft)，且每帧重算 4–5 次

与调度无关，一个长草稿让每一帧花掉真实时间。`TextArea` 在每次编辑时丢掉它的
`wrap_cache`（`forget_geometry`），而一帧随后要好几次：

- `ui::layout::live_height` → `input.row_count`（1 次换行）
- `render_live` → `input_box`：`display_rows` + `wrapped_rows` +
  `cursor_row_col`（又 3 次，每次要么重建缓存，要么克隆一个 `Vec<Range>`）
- 边界的 `ui::cursor_position` → `cursor_row_col` + `row_count`（又 2 次）

`wrapped_rows` 还会在每次调用时**克隆整个 `Vec<Range<usize>>`**，当调用方只是
读的时候那是纯粹的开销。

64 KB 草稿，一帧：这里 1.63 ms。在一台更慢的机器上三四倍于此，就是一台有负载的
笔记本会显示的 6 ms——而它是草稿的*内容*、而非调度，决定延迟的*唯一*路径。

三个修法，最便宜的在前：

1. **借用而不是克隆。** 从缓存返回 `Rc<[Range<usize>]>`（或者一个生命周期绑在
   `RefCell` 借用上的 `&[..]`）；渲染路径从不需要一份拥有的拷贝。
2. **每帧只算一次换行。** `render_live` 已经构造了一个带 `rows`/`row_ranges` 的
   `InputBox`；`live_height` 和 `cursor_position` 可以读它，而不是重新推导。这与
   live 区域行计数已经在用的「一个事实来源」相同（`ui::LiveRows`）。
3. **增量重换行。** 一次编辑碰一个逻辑行；只有那一行的行会变（前缀稳定，像
   `StreamRender` 的冻结行那样）。从被编辑的那行往后重算并拼接，而不是整个草稿。

composer 的文本也没有上限：`paste.rs` 的 `LARGE_PASTE_CHAR_THRESHOLD`（1000 字符）
只作用于 **bracketed paste**。一个召回或程序设的草稿没有限制——`ui::layout` 自己的
`non_preview_rows` 文档承认「一个召回的几兆字节粘贴会换行成几万行」。miyu 把任何超过
10 行 / 800 字符的东西在*编辑器里*折成一个 `[已粘贴 14 行]` 块，所以它的 composer
文本在结构上是小的。在召回时折叠（`app/input_history.rs::recall_input`）会在这里
堵上同一个洞。

## 成因 3 —— 每帧的行求和被重算两到三次

不是按键路径，但它在负载下会叠加。`live_rows`、`non_preview_rows` 和 `live_height`
各自重新推导同样的八个计数，而 `non_preview_rows` 自己的文档记下了代价：

> 实测：普通帧上 53 ns，而在一个调色板开着压在一个四条消息的队列上的帧上是 13 µs
> （`queued_lines` 渲染每条消息）。

13 µs 乘每帧两次，是 700 µs 帧预算的 4%，而 `queued_lines` 已经被 memo 了——剩下
的代价是那个*求和*，从 `preview_budget` 调一次、又从 `live_height` 调一次。一个每帧
构造一次并往下传的 `LiveRows`（正如 `ui::strip_paint_rows` 已经在做的）就把它消掉。

## 不是问题的是什么

- **渲染路径本身。** 整个 live 区域 41 µs，按键 + 一帧 64 µs。单元格差分
  （`term::paint_frame` 的 `prev.diff`）和同步更新已经在做它们该做的。
- **各个 memo。** `QueuedMemo`、`StreamRender::preview_memo` 和 `TranscriptCache`
  的冻结前缀各自都已经把一项实测代价砍掉一个数量级（每五个空闲秒 75→13 个 CPU
  tick；Ctrl+O 273 ms → 14 ms）。
- **I/O。** `src/app/` 或 `src/ui/` 里、测试之外，任何地方都没有出现
  `Command::new`、`fs::read` 或 `metadata`；没有每帧的东西碰磁盘。

## 做什么，按顺序

1. **urgent 帧 —— 已完成（2026-11-14）。** 唯一移动基准所测量的那个数字的改动，
   因为它是一个调度常量而不是一项渲染代价。`docs/design.md` 的帧调度器那一段和
   `docs/architecture/async-rewrite.md` 的 *Why* 注记现在都说地板不再统一，而
   `frame.rs` 的锁定测试覆盖两个地板。
2. **借用换行缓存**（成因 2，第 1 项）。一个文件，行为不变，旁边已经有 ~200 个单元
   测试。
3. **每帧一次换行**（成因 2，第 2 项）——需要行计数也需要的那套「一个事实来源」
   管道（成因 3），所以两者一起做。
4. **折叠长召回**（成因 2，那个上限）。既是工程决定也是产品决定：召回一条 40 KB 的
   消息，是整个显示，还是显示一个块？
5. **一份帧时间日志。** miyu 有 `MIYU_TUI_FRAME_LOG`，每帧写
   `elapsed prep total`，外加在 `slow_frame_ms` 之上按小节的分解；它自己的检查就是
   靠它找到该修什么的。我们没有等价物（没有 `ZEROTH_*` 的帧旋钮），这正是本文
   档的数字来自一个不得不写、然后又不得不扔掉的探测的原因。

## 复现

那个探测没了；如果数字需要复核，就重建它。它是 `examples/ink_probe.rs`，约 90 行：
构造一个 `App`、从一个字符串设 `input`、对每个草稿尺寸计时 `insert_char` +
`ui::render_live` 进一个 `Buffer`。`wrap_rows` 那张表来自直接计时
`TextArea::wrapped_rows` / `display_rows` / `cursor_row_col`。

`cargo run --release --example ink_probe` —— release profile 要紧：debug 构建在这里
慢 10–30 倍，什么都测不到真的。上面的表早于 urgent 帧的修复；修复后的测量需要同一个
探测在一个流式回合期间被驱动（或者第 5 项里的帧时间日志），因为那 8.33 ms 的等待是
调度，不是渲染代价。
