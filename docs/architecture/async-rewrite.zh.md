# 异步运行时重写（codex 风格）

事件循环建在 **tokio** 上，用 `select!` 在五个源上选择（输入、回复、绘制 tick、
`@` 文件搜索结果 —— `docs/input/file-search.md` —— 以及已完成的 Ctrl+V 剪贴板
读取 —— `docs/input/image-paste.md`），镜像 openai/codex 驱动其 TUI 的方式。这
替换掉过去的同步 `event::poll` + drain 循环。*渲染*路径（`term.rs`：diff +
同步更新）没有变 —— 变的只是输入/流式/重绘被调度的方式。

## 为什么

codex 靠这些拿到一个非常灵敏的 TUI：cell diffing、同步帧、一个合并重绘请求的
120 fps 帧调度器、一个粘贴爆发检测器，以及输入与绘制之间的轮转公平。前两样这个
代码库早就有了；这次改动移植其余的。

> 注：对*打字延迟*而言，这个 app 的收益是边际的（本来就在约 1 ms，且由终端 I/O
> 主导）。这次重写是为了在架构上与 codex 对齐，是有意这么选的。
>
> **后来实测，只有一半是真的**（`docs/architecture/input-latency.md`，
> 2026-11-12）：那个 ~1 ms 的数字只在 app 空闲时成立。`MIN_FRAME_INTERVAL` 是
> *每个*请求方的地板，所以只要有东西在动 —— 一条流式回复、状态的微光、一条开着
> 的带子 —— 一次按键最早的一帧就是 `last frame + 8.33 ms`，而一次竞品的基准在
> 负载下测得每次按键 7.36 ms，它自己的是 0.39 ms。地板合并得很正确；它只是分不清
> 一次按键和一个动画 tick。
>
> **2026-11-14 修好：** 请求现在携带紧迫性。一次编辑 composer 的按键按
> `URGENT_FRAME_INTERVAL`（2 ms）夹取，而 `soonest` 把待定的截止时间往前拉；
> 粘贴爆发的检查仍然先跑。普通请求保持 120 fps 地板。那份文档里其余的实测
> （composer 的 O(draft) 换行、逐帧的行和）仍然是未完成的工作。

## 各个部件

- **`frame.rs`** —— `FrameRateLimiter`（`MIN_FRAME_INTERVAL = 8.333 ms`，即普通
  请求的 120 fps；`URGENT_FRAME_INTERVAL = 2 ms`，给编辑 composer 的按键）配一个
  纯的 `clamp_request`（绝不比该请求的地板所定的时间更早发出，且要晚于上一帧；
  `clamp_deadline` 保持普通契约）和 `soonest`（把一个新请求折进待定的截止时间，
  保留最早的那个 —— 于是一次紧迫的按键会把一个动画帧往前拉）。一个薄的异步
  `run_scheduler` 任务在 mpsc 通道上接收 `FrameRequest`，合并并限速它们，睡到
  到期，然后发出一个绘制 tick。`FrameRequester` 是可克隆的发送句柄
  （`schedule_frame`、`schedule_frame_urgent`、`schedule_frame_in`）。纯的部分有
  单元测试；那个异步任务由 `smoke.sh` 覆盖。

- **`paste.rs`** —— `PasteBurst`，一个纯的状态机：在 `BURST_CHAR_INTERVAL`
  （8 ms）之内彼此到达的字符，一旦攒够 `BURST_MIN_CHARS`（3）个，就算一次
  *爆发*。循环用它把一次粘贴/快速打字串合并成**一个**已调度的帧，而不是每个字符
  一帧。用注入的 `Instant` 单元测试。

- **`stream/`** —— `StreamEvent` 现在走一条 `tokio::sync::mpsc::Unbounded`
  通道。backend 仍然跑在一个普通的 `std::thread` 上，那个线程*只发送*（tokio 的
  unbounded `send` 是同步的，可以在运行时之外调用），从而保住「backend 从不读
  stdin」。

- **`main.rs` + `src/tui/`** —— `#[tokio::main(flavor = "current_thread")]`。
  （同步的）viewport 初始化查询完光标之后，一个 `EventStream` 成为唯一的 stdin
  读取者。循环住在 `tui::event_loop::run` 里，每个分支一次 handler 调用，作用在
  持有循环状态的 `Session` 上（`docs/architecture/module-layout.md`）：

  ```text
  tokio::select! {
    Some(Ok(ev)) = events.next()      => on_key / resize → schedule_frame
    Some(se)     = reply_rx.recv()    => stream event   → (commit) + schedule_frame
    Some(())     = draw_rx.recv()     => render current view
    Some(res)    = file_rx.recv()     => @ file-search results → set_file_matches
  }
  ```

  （第四个分支随 `@` 文件 picker 一起到来 —— 它的后台 worker 在一条循环可以
  `select!` 的 tokio 通道上答复排名匹配；见 `docs/input/file-search.md`。）

  每次状态变化之后的 `schedule_frame()` 请求一次重绘；一次编辑 composer 的按键
  则改用 `schedule_frame_urgent()`（在粘贴爆发检查之后），它 2 ms 的地板让它待在
  动画链的前面。调度器发出一个合并后的 tick 来驱动真正的绘制。`insert_before`
  把它的行入队，`set_view_height` 就地调整被跟踪的几何；随后 tick 在一个同步帧里
  写出那些排队的行*并*重绘 live 区域（那个无闪烁修复 —— `docs/ui/flicker.md`）。

## 不变量 1，重述

过去的「只有主线程读 stdin」变成：**`EventStream` 是唯一的 stdin 读取者，它在
初始化时的光标位置查询*之后*创建；回复 backend 只在它的通道上发送。** 理由没变
—— 第二个 stdin 读取者会偷走光标位置（DSR）的回复。`init()` 在 `EventStream`
存在之前、同步地查询一次光标；`insert_before` 自己跟踪 viewport 的行，从不查询。

## 公平性

`tokio::select!` 每次迭代都以随机顺序轮询它的各分支，所以输入和绘制 tick 谁也
饿不死谁 —— 这就是 codex 用它显式的轮转 `poll_draw_first` 开关得到的效果。
