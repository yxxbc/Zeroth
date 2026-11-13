# 输入延迟 —— 那个等着帧地板的按键

日期：2026-11-12

状态（2026-11-14）：**成因 1、2、3 都已修复。** 全局帧地板现在为编辑 composer 的
按键留了一条 2 ms 的紧急通道；换行缓存改为共享（`Arc`），不再每帧
克隆四到六次；`live_rows` 一次就推导出它那八个行计数。仍未完成：增量重换行、
对召回草稿的上限（成因 4）以及一份帧时间日志（第 5 项）。下面的测量留作
记录：当时错在哪，以及是怎么找到的。

## 问题

一个竞品自己的基准（`miyu`，15 个 session + 在它旁边跑一个 16 线程编译，
取中位数）把一次按键放在这里的 **7.36 ms**，而对表里最快的 TUI 是
**0.39 ms**。这份代码里没有什么慢到这个程度：一个空闲的 composer 渲染整个 live
区域只要 **41 µs**。所以另外 7.3 ms 去哪了，而在一台有负载的机器上是什么让这个
数字变得*更糟*？

两个各自独立的答案，而它们需要分开，因为其中之一是一个调度决定，
另一个是算法性的。

## 测了什么

一个一次性的 `examples/` 探测（已删除；数字就是记录），release 构建、
120 列的帧、macOS、在与开发这个 repo 同一台八核机器上。一次「按键」=
`TextArea::insert_char` + 一次完整的 `ui::render_live` 进一个 `Buffer`。

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

从数字得出的结论：**一次普通按键花掉的 CPU 远低于 0.1 ms。**
基准里的 7.36 ms 不是这份代码在计算。

## 成因 1 —— 帧地板是全局的，所以按键排在动画后面（已修复）

在这次测量的时候，`frame.rs::FrameRateLimiter::clamp_deadline`
在**最后一帧发出**之后、无论谁要求的，都从不比 `MIN_FRAME_INTERVAL`
（8.333 ms，120 fps）更早地发出
一帧。来自按键的 `schedule_frame()` 和来自状态 spinner 的
`schedule_frame_in(32 ms)` 走同一条通道，而限速器分不出它们。

于是在一个空闲应用上，一次按键立刻画（64 µs，实测过），但**只要有动画在跑**
——一段流式答复、状态微光（`STATUS_FRAME_INTERVAL` 32 ms）、↓ manager 条带、
一个开着的 `/spinner` 页——一次按键能画的最早时刻就是
`last frame + 8.33 ms`。7.36 ms 的中位数恰好落在那个窗口里，而在一台有 15 个
session 在流式传输的机器上，那是常态而不是尾部。

`docs/architecture/async-rewrite.md` 已经说这是刻意选的：那个地板是从
openai/codex 移植来求架构上一致的，而它的 *Why* 注记观察到「对这个应用而言，
打字延迟上的收益是边际的（本来就 ~1 ms）」。在空闲时那是真的，
在一个回合流式传输时是假的。

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

`pacing::urgent` 对按键/重复键和粘贴为真，对流式文本、鼠标移动和尺寸变化为假。
于是一次按键**从现在起 2 ms 后**到期，永不晚于下一个节拍；那个先 join 的
循环还会在一条后来的消息其实是按键时，*把一个已经算好的截止时间往前拉*。
没有单独的限速器可夹取
——节拍是一个目标，不是一个地板。

这就是 miyu 测得亚毫秒的原因：同样 termbox 级别的工作，不同的调度策略。
我们没有丢掉 7 ms 的渲染；我们把它调度掉了。

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
  `BURST_MIN_CHARS` 3）在一段连发被检出之后就要求放宽的帧，于是一个按住的
  键在三个字符之后退回节拍——但爆发检测器必须在选择 urgent *之前*被查，否则一次
  粘贴会淹没终端。
- **测试钉住了当前契约。** `frame.rs` 的单元测试
  （`clamp_pushes_a_too_soon_request_to_the_frame_floor`）断言确切的
  夹取规则；`docs/design.md` 把 120 fps 地板描述成粗粒度的合并器。两者都需要
  把新规则写出来，而不是悄悄改掉。
- **慢终端。** 120 fps 有一部分存在就是为了让一股快速文本流不把 pty 打满；
  一条也重绘*流式条带*的 urgent 路径会毁掉那一点。urgent 调用必须只能从一次
  按键到达。

## 成因 2 —— composer 的换行是 O(draft)，且每帧复制 4–5 次（部分修复）

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

三个修法，最便宜的在前（第 1、2 项已于 2026-11-14 完成）：

1. **借用而不是克隆。** 从缓存返回 `Rc<[Range<usize>]>`（或者一个生命周期绑在
   `RefCell` 借用上的 `&[..]`）；渲染路径从不需要一份拥有的拷贝。**已完成：**
   `WrapCache.rows` 是一个 `Arc<[Range<usize>]>`（`Arc` 让 `TextArea` 仍然是
   `Send`），`wrapped_rows_shared()` 交出一个 O(1) 的克隆，而 `wrapped_rows()`
   为测试保住那份拥有的 `Vec`。`display_rows`、`row_count`、`cursor_row_col`、
   `cached_rows` 和 `InputBox.row_ranges` 全都读这个共享句柄，所以一帧复制
   那些区间**零**次。这个行为由
   `textarea::tests::wrapped_rows_shared_reuses_one_cache_instead_of_copying_it`
   锁住。
2. **每帧只算一次换行。** `render_live` 已经构造了一个带 `rows`/`row_ranges` 的
   `InputBox`；`live_height` 和 `cursor_position` 可以读它，而不是重新推导。
   **以更便宜的形式完成：** 现在每个调用方都撞同一个共享缓存，于是重新推导的
   代价是一次 `Arc` 克隆，而不是一次 O(draft) 的拷贝。把一个 `InputBox` 一路接进
   `live_height` 和 `cursor_position` 能连这一下也去掉，但需要跨边界改签名，
   换来的只有几百纳秒。
3. **增量重换行——仍未做。** 一次编辑碰一个逻辑行；只有那一行的行会变（前缀稳定，
   像 `StreamRender` 的冻结行那样）。从被编辑的那行往后重算并拼接，而不是整个
   草稿。这是剩下唯一一处 O(draft) 的代价（换行这一趟本身）。

composer 的文本也没有上限：`paste.rs` 的 `LARGE_PASTE_CHAR_THRESHOLD`
（1000 字符）只作用于 **bracketed paste**。一个召回或程序设的草稿没有限制
——`ui::layout` 自己的 `non_preview_rows` 文档承认「一个召回的几兆字节粘贴会
换行成几万行」。miyu 把任何超过 10 行 / 800 字符的东西在*编辑器里*折成一个
`[已粘贴 14 行]` 块，所以它的 composer 文本在结构上是小的。在召回时折叠
（`app/input_history.rs::recall_input`）会在这里堵上同一个洞。

## 成因 3 —— 每帧的行求和被重算两到三次（已修复）

不是按键路径，但它在负载下会叠加。`live_rows`、`non_preview_rows` 和
`live_height` 各自重新推导同样的八个计数，而 `non_preview_rows` 自己的文档
记下了代价：

> 实测：普通帧上 53 ns，而在一个调色板开着压在一个四条消息的队列上的帧上是 13 µs
> （`queued_lines` 渲染每条消息）。

13 µs 乘每帧两次，是 700 µs 帧预算的 4%，而 `queued_lines` 已经被 memo 了
——剩下的代价是那个*求和*，从 `preview_budget` 调一次、又从 `live_height`
调一次。一个每帧构造一次并往下传的 `LiveRows`（正如 `ui::strip_paint_rows`
已经在做的）就把它消掉。

**已修复（2026-11-14）：** `ui::layout::RowCounts` 一次推导出那七个计数，
`live_rows` 和 `fitted_preview_rows` 都从这一套里拟合预览；`strip_other_rows`
和 `RowCounts::non_preview` 共用条带求和的一种写法，所以谁也漂不了。
`live_rows` 里的第二次推导没了，由
`ui::tests::layout::live_rows_shares_one_set_of_counts_with_the_public_fit`
钉住。把一个 `LiveRows` 穿过 `strip_paint_rows` / `view_flow` / 边界传下去
也能去掉剩下的那次跨函数推导，但那是一次签名改动，而且还没有量到收益。

## 不是问题的是什么

- **渲染路径本身。** 整个 live 区域 41 µs，按键 + 一帧 64 µs。单元格差分
  （`term::paint_frame` 的 `prev.diff`）和同步更新已经在做它们该做的。
- **各个 memo。** `QueuedMemo`、`StreamRender::preview_memo` 和
  `TranscriptCache` 的冻结前缀各自都已经把一项实测代价砍掉一个数量级
  （每五个空闲秒 75→13 个 CPU tick；Ctrl+O 273 ms → 14 ms）。
- **I/O。** `src/app/` 或 `src/ui/` 里、测试之外，任何地方都没有出现
  `Command::new`、`fs::read` 或 `metadata`；没有每帧的东西碰磁盘。

## 做什么，按顺序

1. **urgent 帧——已完成（2026-11-14）。** 唯一移动基准所测量的那个数字的改动，
   因为它是一个调度常量而不是一项渲染代价。`docs/design.md` 的帧调度器那一段和
   `docs/architecture/async-rewrite.md` 的 *Why* 注记现在都说地板不再统一，而
   `frame.rs` 的锁定测试覆盖两个地板。
2. **借用换行缓存——已完成（2026-11-14）。** `Arc<[Range<usize>]>` +
   `wrapped_rows_shared`；渲染路径的每个读者都用它，由两个新的 `textarea::tests`
   锁住。
3. **每帧一套行计数——已完成（2026-11-14）。** `RowCounts` 同时喂 `live_rows` 和
   公开的拟合；`ui::tests::layout` 钉住二者的等价。
4. **折叠长召回**（成因 2，那个上限）。既是产品决定也是工程决定：召回一条 40 KB
   的消息，是整个显示，还是显示一个块？
5. **一份帧时间日志。** miyu 有 `MIYU_TUI_FRAME_LOG`，每帧写
   `elapsed prep total`，外加在 `slow_frame_ms` 之上按小节的分解；它自己的
   检查就是靠它找到该修什么的。我们没有等价物（没有 `ZEROTH_*` 的帧旋钮），
   这正是本文档的数字来自一个不得不写、然后又不得不扔掉的探测的原因。

## 复现

那个探测没了；如果数字需要复核，就重建它。它是 `examples/ink_probe.rs`，
约 90 行：构造一个 `App`、从一个字符串设 `input`、对每个草稿尺寸计时
`insert_char` + `ui::render_live` 进一个 `Buffer`。`wrap_rows` 那张表来自
直接计时 `TextArea::wrapped_rows` / `display_rows` / `cursor_row_col`。

`cargo run --release --example ink_probe` —— release profile 要紧：debug
构建在这里慢 10–30 倍，什么都测不到真的。上面的表早于 urgent 帧的修复；
修复后的测量需要同一个探测在一个流式回合期间被驱动（或者第 5 项里的帧时间
日志），因为那 8.33 ms 的等待是调度，不是渲染代价。
