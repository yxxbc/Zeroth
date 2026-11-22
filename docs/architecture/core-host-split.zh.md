# core/host 拆分 —— 一份分阶段方案

日期：2026-11-23

状态：**进行中**。第 1 阶段（`ConversationView`）、第 2 阶段（`Screen` 接缝）和
第 3 阶段的状态搬移已完成（2026-11-23）：`Session` 持有 23 个字段，
`Conversation` 63 个，`ConversationView` 29 个。第 3 阶段按区域的方法搬移继续；
第 4 阶段还等它自己的产品触发条件。本方案所延伸（并且刻意**不**推翻）的决策是
[`core-seam-decision.md`](core-seam-decision.md)。

## 问题

「这个需要 core 架构吗？」答案取决于说的是哪个「core」，两种读法的答案不同：

- **一个纯库、终端在边界之外**，这个已经有、而且是被强制的：`src/app/` 和
  `src/ui/` 里没有任何文件系统、时钟或环境访问，整个 crate 是
  `src/lib.rs` 之上的一个二进制，而 `tests/api_surface.rs` 在编译期把公开表面
  钉住。单独拆一个 `zeroth-core` crate 买不到 `lib.rs` 这条线之外的任何隔离。
- **一个会话核心：它拥有一个对话，而类型里没有终端** —— 这样一个进程能跑几个对
  话，前端以后也能接上来 —— 这个还没有。`tui::Session` 是一个对话唯一的拥有者，
  它整个生命期都借着终端，而 `App` 是一个对话。

本方案讲的是第二种读法。它保留决策页的结论（完整拆分现在还配不上它的代价），并
把那几条「让它保持便宜」的规则变成一份可执行的施工顺序，其头几步在今天的单会话
TUI 上就能自己回本。

## 代价到底在哪

今天（2026-11-23）测得的数字：

| 事实 | 数字 | 为什么贵 |
| --- | --- | --- |
| `Session` 字段 | **113** | 一个 struct 同时装着对话、它的运行时、它的缓存和终端 |
| ……其中全屏视图状态（`full_*`） | **23** | 不能脱离终端重建，所以第二个窗格不可能存在 |
| `impl Session` 块 | **36** 个，分布在 **29** 个文件 | 第二个对话要第二份处理函数 |
| `Session<'t>` 持有 | `&'t mut InlineViewport` | 终端的生命期*就是*会话的生命期 |
| `src/tui/` 与 `src/ui/` 里的 `fullscreen()` 分支 | **16**（15 个生产、1 个测试） | 模式是每个调用点各读一次的 flag，不是一道接缝 |
| `App` 字段 | **81** | `history`、`streaming`、`queued`、`input` —— 已经是一个纯对话 |

这张表要紧的读法是：**`App` 不需要拆。** 它本来就是每个对话的模型，有单元测试、
没有 I/O。N 个对话需要的，是**每个**对话一个 `App`，再加上围在各自外面的一个运行
时和一个视图 —— 而今天运行时和视图被焊在 `Session` 里，和那唯一的终端并排。

进程级的那一半已经分出去了：`Host`（`src/tui/host.rs`）装着 `-p --jobs` 共享的
只读事实，而 `batch.rs` 已经证明，只要每个会话拥有自己的线程，一个进程跑 N 个会
话是成立的（`multithread-core.md`：64 并发下 38.5 MB）。TUI 正是用不上这个形状
的那个客户端。

## 目标形状

```
Host                 process facts: config home, provider table, secrets, hooks
 │
Workspace            the session table, the one terminal, the one runtime
 ├── Conversation 0   App · runtime (models, channels, stores, queues) · view
 ├── Conversation 1   …
 └── Screen           inline | full-screen | headless  (one small interface)
```

- **向下**：前端把按键、鼠标和 resize 变成 `Action`，调用某一个对话的处理函数。
- **向上**：一个对话在自己的通道上报 `StreamEvent`；workspace 把它们合并进循环，
  并带上对话的标记。
- **不能动的那个不变量**：一个 stdin 读取者（不变量 1）、纯的 `App`/`ui` 决策
  层，以及 `StreamEvent` 加通道的形状（绝不跨线程共享可变状态）。

## 各阶段

每个阶段是一个或几个 commit，能独立构建、独立发布。前一阶段在 CI 和 smoke 套件里
绿了，后一阶段才开始。

### 第 1 阶段 —— `ConversationView`（2026-11-23 完成）

**做什么**：把 23 个全屏视图字段 —— 顺带把与屏幕无关的渲染器（`render`、
`agent_render`、`transcript`、`context`）、帧的钉住/命中偏移和链接卡片预览 ——
从 `Session` 挪进 `src/tui/view.rs` 里的一个 `ConversationView` 值。帧的行从
`App` 加一个尺寸重建；指针、滚动锚点和展开集合是跟着视图走的交互状态，而链接卡片
预览是从进程缓存恢复的显示缓存（`link_cards::load`，不是产品状态）。

**为什么它自己就回本**：113 个字段里的 23 个变成一个值、一个拥有者；帧缓存不借终
端也能测；「同一块屏幕永远画出同一帧」有了测试 —— 一个全新的视图和一个只增量的
视图给出同一帧。这正是决策页的第 2 步，而且它不需要 `Screen` 接缝。

**文件**：

- `src/tui/mod.rs` —— 那个 struct 字段和 `Default` 构造；`Session` 多一个
  `view: ConversationView`。
- `src/tui/view.rs` —— 新类型、它的 `Default`，以及搬到它上面的帧构建器
  （`full_tail_lines`、`tailed_with_stream`、`draw_conversation`、`apply_hover`、
  `apply_selection`、`full_window`……），其中历史那一半抽成自由的
  `frame_history(app, view, width, screen_width, …)`，让它的缓存在没有终端时也能
  测。
- `src/tui/actions.rs`、`src/tui/commit.rs`、`src/tui/stream.rs` —— 调用点，
  机械替换（`self.full_x` → `self.view.full_x`）。
- `src/tui/view.rs` 的测试 —— 显式构造一个视图；新增
  `a_fresh_view_rebuilds_the_same_frame`。

**不变量**：不新增公开 API（TUI 是二进制私有的）；行为不变；视图缓存永远不是产品
需要的任何东西的唯一副本（帧的行从 `App` 加尺寸重建；链接卡片是 wire 上那些 URL
之上的显示缓存）。`src/ui` 不碰。

**怎么验证**：`cargo test --bin zeroth`（视图测试）、`AGENTS.md` 里的全量 lib 扫
一遍、`cargo build && scripts/smoke.sh 156 164` 外加一个内联阶段（例如 055）、
`cargo fmt --check` 和 clippy。

**怎么回滚**：一次 `git revert`；没有落盘格式变化。

### 第 2 阶段 —— `Screen` 接缝（2026-11-23 完成）

**做什么**：`Session` 不再点名 `InlineViewport`。一个小的 `Screen` enum（`Inline`、
`Full`、`Headless`）实现 `commit`、`draw_frame`、`draw_overlay`、`suspend`、
`resume`、`restore` 和 `live_rect`；`src/tui/` 里 15 个生产 `fullscreen()` 分支
挪到它后面。`App::fullscreen()` 保留为 `ui` 构建器读的**纯布局信号** —— 它是数
据，不是边界分支。

**为什么它自己就回本**：循环不再在每个调用点问「我在哪个模式」；全屏帧有了一个入
口；无头路径成为真正的第三个实现，而不是散落各处的 `headless` 字段检查。

**文件**：`src/tui/screen.rs`（新）、`src/term.rs`（viewport 的公开表面收窄到
enum 需要的东西）、`src/tui/mod.rs`、`src/tui/view.rs`、`src/tui/actions.rs`、
`src/tui/bootstrap.rs`、`src/tui/startup.rs`、`src/main.rs`（构造变体）。**实际落
地**：同一个 viewport 之上的一个 enum，`is_full`/`is_headless`/
`accepts_pointer` 加上 `draw_region`/`draw_frame` 分派；六个生产
`fullscreen()` 分支和 `Session::headless` 字段都没了。剩下的 viewport 调用仍经
`Deref` 到达 —— 第 3 阶段给它们方法。

**有风险的不变量**：一个 stdin 读取者，且在初始化光标查询之后创建（不变量 1）；
内联模式的双向 resize 重排（不变量 3）；overlay 重绘规则。每个都有 smoke 金丝雀
（覆盖 resize、Ctrl+O、权限提示的阶段），必须保持绿。

**怎么验证**：整套大关、两种模式的 `scripts/smoke.sh` 阶段集（156/164，以及内联
套件的 resize/overlay 阶段），外加 `cargo test --bin zeroth`。

**怎么回滚**：整块落地；加一个按模式选旧路径的开关，正是决策页禁止的「半截接
缝」。回归的话整条系列 revert。

### 第 3 阶段 —— `Conversation`（运行时）、`Workspace`（表）—— 状态部分 2026-11-23 落地

**做什么**：把 `Session` 里非终端、按对话的字段 —— `models`、九个通道、各注册表
与 store、各时钟、inflight/reaping 句柄、`cwd`、recorder —— 归进
`tui::conversation::Conversation`。`Session` 变成
`{ screen, view, conversation }` 之上的薄驱动，而一个 `Workspace` 拥有一个对话
加上那唯一终端和 runtime。**行为不变，也还没有第二个对话**：重点是形状容得下。

**为什么它自己就回本**：这一步之后 `Session` 上的每个字段，不是终端、就是视图、
就是对话 —— 那些分节注释变成了类型。新的处理函数可以接
`&mut Conversation`，不会顺手摸到终端。

**文件**：`src/tui/conversation.rs`（新）、`src/tui/mod.rs`、以及每个拥有
`impl Session` 块、且方法只碰对话状态的模块（29 个文件 —— 按区域分批搬，每批一
个 commit）、`src/tui/event_loop.rs`（不绘制的分支借对话，而不是借会话）。**实际
落地**：63 个按对话的字段（模型、通道、各注册表、store、时钟、inflight 句柄、
cwd）归 `Conversation`；`Session` 是屏幕、视图、本次运行的终端事实和进程级事实
—— 23 个字段。处理函数仍经由 `self.conversation` 到达，按区域的方法搬移随后。

**不变量**：借用拆分必须让每个 `select!` future 借**不同字段**（通道字段上的注
释就是契约）；任何处理函数都不能持有它原本从一个 struct 上拿到的两个可变借用。既
需要对话又需要屏幕的处理函数留在驱动上。

**怎么验证**：整套大关、全量 lib 扫描、完整 smoke 套件（这一阶段碰循环）、
`-p --jobs` 的批量测试。

**怎么回滚**：每个区域的搬动各自一个 commit，拆错就回滚一个区域，而不是整个阶段。

### 第 4 阶段 —— 一个 TUI 里的第二个对话（产品触发条件）

**做什么**：第一个真需要 N 个对话的功能。最小的有用形状是**进程内标签页**：
`Workspace` 持有 `Vec<Conversation>`（每个标签一个 `ConversationView`），各自带
通道；循环的 `select!` 经一个 `(id, event)` 通道合并且按对话分派；现有的会话栏
（本来就切换子 agent 视图）长出一行 `+` 和一个 `/sessions` 命令。守护进程加 attach
刻意**不**在这一阶段：它需要 transport，而标签页不需要。

**为什么没有触发条件就停在这里**：今天的任何产品都不需要一个窗口里两个对话；几个
终端或 `-p --jobs` 足以覆盖那个对比。决策页的触发条件仍然适用：只有当一个 TUI 里
两个活对话被真正要求时才做。

**怎么验证**：新 smoke 阶段开两个对话，各跑一个 dummy 回合，断言两边都继续绘制、
谁的队列都不会漏到另一边；在 `multithread-core.md` 里补一行两个活 `App` 加两个视
图的内存数。

**怎么回滚**：标签页可以先藏在一个命令后面发布，而 workspace 只持一个对话；去掉那
个命令就回到第 3 阶段。

### 第 5 阶段 —— Host 补完（与其余阶段独立）

**做什么**：剩下两项进程级工作 —— 一个共享的 **MCP 连接池**和一个**进程级信号
处理器** —— 挂到 `Host` 上。它们可以在上面任何阶段之前或之后落地，而
`core-seam-decision.md` 本来就把它们列为让批量受益的触发条件。

**怎么验证**：MCP 各阶段与信号测试；`-p --jobs` 的内存不变。

## 让它在第 3 阶段前保持便宜的规则

决策页的五条规则继续有效。本方案加上保持*这个顺序*诚实的检查：

1. **新的按对话产品状态放 `App`**（纯、有单元测试）。新的按屏幕缓存放进
   `ConversationView`。新的进程事实走 `Host`。`Session` 上不新增东西，除非那是唯
   一的终端或某个对话的运行时。
2. **不为会话状态新增 `static` 或 `thread_local`。** 两处已知的债
   （`palette::ACTIVE`、`OVERLAY_ACTIVE`）不再长大。
3. **`src/tui/` 里不新增 `self.fullscreen()` 分支。** 第 2 阶段清掉它们；新加一个
   就是回归。（`ui` 可以读 `App::fullscreen()` —— 那是纯布局信号 —— 但那里新增分
   支要有测试。）
4. **传消息，不共享可变状态。** `StreamEvent` 加通道，不要 `Arc<Mutex<App>>`。
5. **新缓存必须能从状态加一个尺寸构造出来**，并且测试要构造两次、比对（第 1 阶段
   的测试就是模板）。
6. **会话级行为落地时必须点名它的字段组** —— 视图、对话还是终端 —— 写进 struct
   的分节注释。那条注释就是让第 3 阶段变成机械工作的东西。

## 什么情况下重新决策

- **触发条件成立**：产品需要一个 TUI 里两个活对话，或一个守护进程/attach 客户
  端，或一个 release 里两个 bug 都追到「假设只有一个 `App`/`Session`」。第 4 阶段
  开始。
- **第 1 或第 2 阶段没有回本**：如果视图抽取看不出简化、也换不来测试价值，就停在
  那里、保住规则。决策仍是「推迟」。
- **批量需要的不止 Host 那两项**：如果 `-p --jobs` 想要第二个前端（机器可读的
  事件流），第 2 阶段的接缝就是修它的地方，而顺序可以换成第 3 与第 5 对调。

## 本方案不是什么

- 不是把 `App` 重写成几个 struct：它本来就是对话模型，在第 4 阶段之前搬字段什么
  也买不到。
- 不是一个回合内的并行 tool 调用 —— [`concurrency.md`](concurrency.md) 里的
  *Option A*，另一个决定。
- 不是内联/全屏的默认值 —— 那是 [`full-screen.md`](../ui/full-screen.md) 的。
- 不是一个 `zeroth-core` crate：`lib.rs` 这条线已经是 crate 边界。

## 在哪里追踪

决策与它的触发条件：[`core-seam-decision.md`](core-seam-decision.md)。清单与定
价：[`concurrency.md`](concurrency.md)。批量客户端与测量：
[`multithread-core.md`](multithread-core.md)、[`batch.md`](../cli/batch.md)。模块
边界：[`module-layout.md`](module-layout.md)。代码锚点：`src/tui/mod.rs`
（`Session`）、`src/tui/view.rs`（`FullTail` 与帧）、`src/tui/bootstrap.rs`
（`Option<&Host>` 入口）、`src/term.rs`（`InlineViewport`）。

一个加全局、把产品状态只放在 `Session` 上、或者新增 `fullscreen()` 分支的 PR，应
该在描述里说清为什么：那句话就是这一页存在是为了保持可见的早期警报。
