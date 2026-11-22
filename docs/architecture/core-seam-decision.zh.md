# 内核接缝 —— 推迟 host 重构，以及让它保持便宜的规则

日期：2026-11-22

状态：**已接受**。这是一页决策，不是计划：重构被推迟，下面的边界规则在排期前就是评审标准。

## 决定

现在不建 codex 那种“会话内核 + 多个前端”的架构。保留已有的形状：

```
main.rs ──owns──▶ InlineViewport
                      ▲
                      │ &mut
                 tui::Session ──▶ App (pure) · ui (pure)
```

纯库仍然是决定行为、承载测试的地方；TUI 仍然是唯一边界；`Host` 仍然是 `-p --jobs` 已经共享的进程级事实。只有下面的触发条件之一成立时才重新审视拆分。在那之前，*让它保持便宜的规则* 就是评审要检查的内容。

## 这里的“内核”指什么

core/host 拆分把对话移出终端：

- **一个会话内核**，拥有 `App`、回合循环、rollout、权限与 MCP，类型里没有终端；
- **前端经由 transport** —— TUI、headless 运行，以及将来的 server 或 attach 客户端；
- **一个进程里 N 个对话**，带会话切换器或窗格；会话可以创建和丢弃，不必每次都重新 bootstrap。

这就是 [`concurrency.md`](concurrency.md) 里的 *Option D*，而证明线程形状可行的实测在 [`multithread-core.md`](multithread-core.md)。它**不是**一个回合内的并行 tool call —— 那是同一页的 *Option A*，是另一个决定。

第一个客户端已经存在：`-p --jobs` 在一个进程里跑 N 个独立会话（[`batch.md`](../cli/batch.md)）。没跟上的客户端是 TUI。

## 已经存在的东西

- **`Host`**（`src/tui/host.rs`）：只读的进程级事实 —— config home、首次运行种子、skill 与 agent 遍历、settings、secrets、hooks、provider 表。`-p --jobs` 解析一次，把 `Option<&Host>` 交给每个 bootstrap；单次 TUI 或 headless 运行自己解析同样的事实。
- **批量的形状**：每个会话一个 OS 线程，各自带 current-thread runtime，跑在有界的 `--concurrency` 上（默认 4）。64 个 job、64 并发实测：38.5 MB，stderr 为空。
- **`InlineViewport::headless()`**：会话已经可以在没有屏幕的情况下运行。
- **`ReplySource` / `LlmBackend`**：后端接缝与 UI 无关。
- **按会话的 rollout**：一个对话就是 sessions 树下的一个文件，`/resume` 加载它不需要进程级全局表。
- **纯逻辑表面被钉住**：`tests/api_surface.rs` 与 `app`/`ui` 的单元测试让“搬行为”变成“搬测试”，而不是重写。

## 重构必须改什么

按成本从高到低大致是：

1. **`App` 是一个对话。** `history`、私有的 `streaming`、`queued` 和 composer 的 `input` 都直接挂在它上面。N 个对话要么每个对话一个 `App`（外加容器），要么把这几块收进 `App` 拥有的 `Conversation`。所有渲染和决策都读这些字段之一。
2. **`Session<'t>` 借用屏幕的生命周期。** 终端的生命周期就是会话的生命周期 —— 这一点仍然成立，是第 3 阶段的事。[`full-screen.md`](../ui/full-screen.md) 里设计的 `Screen` 接缝**已经落地**（`core-host-split.md` 第 2 阶段）：`Session` 持有一个 `Screen`（inline / 全屏 / 无头），TUI 问它 —— `is_full`、`accepts_pointer`、`draw_region`、`draw_frame` —— 而不是在每个调用点读 viewport 的 `full` 标志。
3. **全屏视图状态在 `Session` 上。** tail 缓存、滚动锚点、选择、hover、命中图、展开集合和置顶输入都是存在唯一 app 旁边的每窗格状态。窗格模型必须能由 `App` 加一个尺寸重建，就像内联 viewport 现在这样。
4. **`Session` 是 `!Send`。** 文档记录的原因是 `syntect` 的 `onig` 原始指针跨 await。每会话一线程绕开了它；daemon 或共享 async runtime 得先改高亮器。
5. **进程级假设是分散的。** `palette::ACTIVE` 是 `thread_local`，`OVERLAY_ACTIVE` 是 `static` 原子，而信号、MCP manager 和 provider client 都是每会话一份。批量能成立，是因为每个会话拥有自己的线程；多对话 TUI 会共享它们。
6. **`bootstrap.rs` 和 `models.rs` 仍然假设一个会话。** `Host` 搬走了只读的那一半；可变的那一半仍在每次 bootstrap 时组装。

这些今天都不算错。它是单会话类型的代价，只付一次。

## 为什么推迟

- **没有产品功能被卡住。** 同时跑几个任务就用几个进程，或者 `-p --jobs`；两者今天都能用。
- **价格是真实的。** `concurrency.md` 把 *Option D* 定价为 very high：它会碰到 `App`、`Session`、事件循环，以及承重的取消与权限不变量。
- **接缝在正确的位置。** 纯 `App`/`ui` 对着 I/O 边界是难的那一半，而它已经在了。重构是沿着接缝搬，不是重写模型层。
- **只要规则守住，债不会复利。** 昂贵的失败模式不是“我们没重构”，而是“我们一边等一边加了第二个事实来源”。

## 让它保持便宜的规则

在重构排期之前，这些就是评审标准：

1. **把状态放在该在的地方。** 每个对话的产品状态放 `App`（纯的、有单元测试）。进程级事实走 `Host`。每屏幕缓存属于视图层，并且必须能由 `App` 加一个尺寸重建 —— 缓存永远不能是产品所需信息的唯一副本。
2. **不新增全局。** 不为会话状态新增 `static` 或 `thread_local`。`palette::ACTIVE` 和 `OVERLAY_ACTIVE` 是已知的债；新代码把值作为参数传，或经 `Host` 拿。
3. **跨线程只传消息，不共享可变状态。** 沿用 `StreamEvent` 加 channel 的形状。不要 `Arc<Mutex<App>>`，也不要让线程直接读写 `Session` 字段。
4. **取 `&App`，而不是会话。** `app` 或 `ui` 里的新纯逻辑只拿它需要的数据。一个要穿过 `Session` 的 helper 没法服务第二个对话。
5. **不要半截搭接缝。** 不要为一个调用方加 `Screen` 枚举，也不要加第二个 viewport。它落地时，两个 viewport 都实现它，`full` 分支在同一次改动里移到它后面，并由 smoke 阶段证明两边都行。

## 什么时候排期

下面任何一条成立，就足以开工：

- 产品功能需要**一个 TUI 里同时有两个对话** —— 子代理视图之外的切换器或窗格；
- 想要 **host 或 daemon**：attach/detach、多个客户端，或者会话活得比终端久；
- 批量需要剩下两项 `Host` 工作 —— 共享的 **MCP 连接池**和一个**进程级信号处理器**；
- 一个版本里有两个以上 bug 都能追到“假设只有一个 `App`”或“假设只有一个 `Session`”；
- 为了 headless 一致性，需要终端之外的前端。

没有触发条件就是答案：继续推迟。

## 真要做时的路径

1. **`Screen` 接缝 —— 已完成**（`core-host-split.md` 第 2 阶段）。`Session` 不再点名 `InlineViewport`；内联、全屏和 headless 三条路径是一个 enum（`commit`/`draw_region`/`draw_frame`/`draw_overlay`/`restore` 在它后面）。循环不再在 `fullscreen()` 上分支；剩下的 viewport 调用经由接缝走到第 3 阶段给它们各自的方法。
2. **每对话一个视图对象。** 全屏缓存从 `Session` 移到每个对话一个的值里，由 `App` 加一个尺寸重建。完成的标准是不碰事件循环也能构造第二个视图。
3. **拆 `App`。** `history`、`streaming`、`queued` 和 `input` 移进 `Conversation`；`App` 拥有一个或多个。完成的标准是 `-p --jobs` 能复用同一类型，TUI 能持有 N 个。
4. **TUI 客户端。** 一个会话切换器和它背后的 transport（#28）。完成的标准是第二个对话在跑回合时，第一个仍在正常绘制 —— 也就是批量已经实测过的线程形状。

第 1、2 步在当前的**单会话** TUI 上就能回本：更少的 `full` 分支，以及一个不依赖终端就能测试的视图。

## 这页不决定什么

- **内联还是全屏作为默认** —— 归 [`full-screen.md`](../ui/full-screen.md) 管。
- **一个回合内的并行 tool call** —— [`concurrency.md`](concurrency.md) 里的 *Option A*，连同它的取消与顺序成本。
- **MCP 连接池共享与进程级信号处理器** —— 可以在不拆会话的情况下单独落地的 Host 步骤。
- **单进程还是 attach 到 daemon** —— 以后的产品决定，不是前置条件。

## 在哪里追踪

清单在 [`concurrency.md`](concurrency.md)；实测与批量客户端在 [`multithread-core.md`](multithread-core.md) 和 [`batch.md`](../cli/batch.md)；模块边界在 [`module-layout.md`](module-layout.md)。代码锚点是 `src/tui/mod.rs`（`Session`）、`src/tui/host.rs`（`Host`）、`src/tui/bootstrap.rs`（`Option<&Host>` 入口）和 `src/term.rs`（`InlineViewport`）。

新增全局、或只把产品状态放在 `Session` 上的 pull request，应该在描述里说明理由；那句话就是这页想一直保持可见的早期警报。
