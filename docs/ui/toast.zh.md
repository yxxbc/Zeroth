# 瞬时 toast

一个 **toast** 是一行、自己清掉的状态消息，显示在输入框正上方 —— 已提交的
`Role::System` / `Role::Error` scrollback 消息的短暂对应物。它为那些用户应该*看
到* 但绝不想留下的确认和软拒绝而存在：跑 `/copy`、切模型、或者被告知某条命令在回
合中途被禁用。应用不会在对话里铸出一个永久的圆点，而是「就地显示一下」，几秒之后
把它淡出。

```
❯ Hi

(  ●•· ) Working… (1s · ↑ 1 tokens · esc to interrupt)

  Copied last message to clipboard          ← the toast (dim), auto-clears ~4s
───────────────────────────────────────────────────────────────
❯
───────────────────────────────────────────────────────────────
```

## 什么走 toast

| 触发 | toast | 种类 |
| --- | --- | --- |
| `/copy` 成功 | `Copied last message to clipboard` | info |
| 还没有回复时 `/copy` | `No agent response to copy` | info |
| `/copy` 的剪贴板写入失败 | `Copy failed: {reason}` | error |
| 对话为空时 `/export` | `Nothing to export` | info |
| `/model` 切换成功 | `Switched model to {id}` | info |
| 没 key 时 `/model` 切换 | `Can't switch to {id}: run /login to set {ENV}` | error |
| `/login` key 已保存 | `Saved {ENV} — run /model to use {provider}` | info |
| `/login` 写入失败 | `Couldn't write {path}: {e}` | error |
| 回合中途跑 `/resume` | `/resume is disabled while a task is in progress` | info |
| 回合中途跑 `/init` | `/init is disabled while a task is in progress` | info |
| 回合中途跑 `/compact` | `/compact is disabled while a task is in progress` | info |
| 上下文为空时 `/compact` | `Nothing to compact` | info |

最初那几行（`/copy`、`/model`、`/login`、`/resume`，以及已退役的 `/help`）在这个
设计之前都是作为 scrollback 消息提交的；后来的 `/init` 和 `/compact` 那两行则生
来就是 toast。不管哪种，它们都以 toast 呈现，在 `history` 里不留痕迹（所以它们永
远不出现在 Ctrl+O transcript 或一次 `/resume` rollout 里）。空闲时的 `/help` 是
唯一的例外：它的多行命令列表作为一条 `Role::System` 消息提交，因为那是可以滚着看
的内容。`/help` 此后已移除 —— palette 列出每条命令和它做什么，带一个 `(n/total)`
计数 —— 所以现在没有任何斜杠命令会提交消息了。

## `/model` 和 `/login` 现在能在回合中途工作

`/resume` 仍然在一个回合活动时阻塞（它会换掉整段对话 —— 与流赛跑）。但 `/model`
和 `/login` 只是用它们的内联 picker 替换 composer；它们从不碰正在跑的回合。所以
它们现在 **不管回合状态都打开**（旧的 `MODEL_BUSY_NOTICE` / `LOGIN_BUSY_NOTICE`
拒绝没了）。两个后果：

- **正在跑的回合不受影响。** 它在自己的后台线程上流进自己的通道。选一个模型会重
  建 `main.rs` 的 `Box<dyn ReplySource>`，那只改变*下一个*回合所用的后端 —— 在飞
  的那个线程继续跑。
- **picker 打开时状态行仍然可见**（2026-08-08 更新）。「只替换 composer」是字面
  的：`/model` 和 `/login` 的 picker（以及 `/settings`）在它们自己的外框上方为流
  式条保留行，所以运行中工具的活 cell、spinner、计时器 —— 以及任何排队消息和
  toast 本身 ——在你浏览时都留在原位。它们以前占据整个 live 区域（对它们
  `render_live` 提前返回），那恰好在 picker 打开的那段时间里把指示符清空：这就是
  被报告的那个 bug，修法与 ↓ 管理器条带同一个 bug 的修法一样（
  `docs/providers/llm.md`、`docs/tools/background.md`、
  `docs/ui/status-indicator.md`）。

因为一次回合中途的模型切换或 key 保存绝不能把流式回复切在 scrollback 里，它们的
确认是 **toast**，不是已提交通知 —— `/copy` 是同样的理由。

## 纯状态（`app/mod.rs`）

时间是脏的，留在库外，所以 `App` 只持有 toast 说*什么*，从不持有它*什么时候*死：

```rust
pub enum ToastKind { Info, Error }
pub struct Toast { pub text: String, pub kind: ToastKind }
// App { … toast: Option<Toast> }
```

- `App::show_toast(text, kind)` —— 设定（覆盖当前 toast）。它**不**往 `history`
  里推：一个 toast 是 UI，不是对话。
- `App::clear_toast()` —— 丢掉它。
- `App::toast()` —— 当前 toast，给渲染器。

`clear_conversation`（`/clear`）也丢掉 toast —— 一块被擦干净的板子不显示任何残
留。

斜杠命令派发为 palette 的软拒绝返回 `Action::Toast(String)` —— 回合中途那些忙碌
通知（`/resume`、`/init`、`/compact`），以及那些还没找到可做之事的命令：
`/compact` 的 `Nothing to compact`、`/export` 的 `Nothing to export` 和 `/copy`
的 `No agent response to copy`。边界把每个 `Action::Toast` 显示成一个 info
toast，所以一个纯核心的拒绝永远不可能以红色出来。其他每个 toast 都由边界直接升起
（它本来就拥有那些确认所用的剪贴板 / 后端 / `.env` I/O）。

## 到期（`main.rs`，时间戳模式）

和状态时钟完全一样 —— 边界留住脏的那部分，纯核心只看到结果。`main.rs` 持有一个
`toast_deadline: Option<Instant>`。显示一个 toast（`present_toast`）会设
`App::show_toast`，把 deadline 设成 `now + TOAST_TTL`（**4 s**），并请帧调度器在
到期时刻绘制一次：

```rust
app.show_toast(text, kind);
toast_deadline = Some(Instant::now() + TOAST_TTL);
frame.schedule_frame_in(TOAST_TTL);
```

合并的绘制 tick 在到期时清掉它，并在它还活着时**重新武装** —— 之后某次按键的帧可
能消费掉那个挂着的 deadline（调度器只保留最早的一个），所以不重新武装的话，这次
清除可能被丢掉：

```rust
if let Some(dl) = toast_deadline {
    let now = Instant::now();
    if now >= dl { app.clear_toast(); toast_deadline = None; }
    else { frame.schedule_frame_in(dl - now); }   // keep a frame pending for the clear
}
```

这对应紧挨着它那句
`if app.turn_active() { schedule_frame_in(STATUS_FRAME_INTERVAL) }` 的重新武装。
一个 toast 到期不需要任何回合处于活动状态 —— 排好的帧自己会触发。

## 布局（`ui/footer.rs`）

toast 是一行（在宽度处用 `…` 截断，像页脚一样）钉在**条带的底部，紧贴框的顶部分
隔线之上** —— 有回合在流式时在状态/空隙和任何排队消息下面，空闲时就在框的正上
方。它由 `toast_rows(app)`（0 或 1）保留，并与 `queued_rows` 并列穿进
`live_height` / `live_layout` / `input_box` / `render_live_with_preview` /
`cursor_position` —— 也就是条带早就在长的那「框上方的行」。因为它住在框*上方*，
显示/清除它就和流式条与队列早已做过的那样，从顶部让区域长/缩（`ui::repin` 重新锚
定，框不跳）。

样式集中在 `ui/theme.rs` 里：`TOAST_INDENT`（与页脚/队列共用的两空格内缩）、info
用 `toast_color()`（暗色，`tool_dim_color()`）、失败用 `toast_error_color()` ——
见下。

## Info 还是 error

一个 toast 是安静的装饰：info 那种穿页脚的暗色，而 error 那种几乎一样安静，却仍
然一眼说出*失败*。

- **Info**：确认，以及什么都没失败的软拒绝 —— 一条在回合中途被禁用或在这个
  session 里不可用的命令，或者一条还没东西可做的命令
  （`No agent response to copy`、`Nothing to export`、`Nothing to compact`）。
- **Error**：用户要求的事情失败了，或者不会按他们以为的方式工作 —— 一次剪贴板或
  文件写入、一次 key 保存、一次登录、一次被拒的模型切换或恢复、一条 agent 收不了
  的消息、一张为看不见它的模型粘进来的图片，或者一个解析不了的配置文件（
  `hooks.json`、`trust.json`、`mcp.json`、一个 `SKILL.md`、一个 agent 文件）。
  `docs/tools/hooks.md` 和 `docs/state/project-config.md` 刻意让配置那几种很响：
  一个静默加载失败的守卫，读起来像一个正在生效的守卫。

error 的墨色是**主题自己的红，往暗色那边混三分之一**（`TOAST_ERROR_DIM_MIX`，
0.35，经由 `lerp_color`）。它保住红的色相 —— 红通道在每个 RGB 主题里仍然领先 ——
在大概三分之二的饱和度上，所以一次失败的 `/copy` 仍然看起来和成功的不一样，而
toast 又不会成为屏幕上最响的一行。在默认的 Mocha 主题上 `#F38BA8` 变成
`#CA89A4`。`ansi` 主题的红是一个没有分量可混的终端命名色，所以它保留终端自己的
`Red`（正常强度那个，不是 `LightRed`）。完整的红留给错误圆点、失败和被拒绝的
tool cell，以及其他已提交的失败，它们比 toast 的四秒活得久。`/skills` picker 的
session 关闭注记借的是同一种墨色。

把 error 那种一路调暗曾被考虑过并被拒绝。一个 toast 四秒就没了，所以它的色相是
`Copied last message to clipboard` 与 `Copy failed: …` 之间唯一的一眼可辨之处，
而启动时那些配置解析 toast 会和它们周围的暗色装饰混在一起 —— 也就是那两份文档所
针对的静默失败。
