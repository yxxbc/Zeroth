# `/copy` —— 把最后一条回复复制到剪贴板

日期：2026-06-20

## 目标

一条 `/copy` 斜杠命令，把**最后一条 assistant 回复**复制到系统剪贴板 —— 移植自
openai/codex 的 `/copy`（「copy last response as markdown」）。结果以一个**瞬时
toast** 出现在框的上方（一条信息性的 `Copied last message to clipboard` 或
`No agent response to copy`；一个错误 `Copy failed: {e}`），它几秒后自行消失，
而不是在 scrollback 里提交一个 bullet。见 `docs/ui/toast.md`。

> **更新（2026-09-23）：** 没有可复制的回复时，`/copy` 现在从纯核心返回
> `Action::Toast(COPY_EMPTY_NOTICE)` —— 与 `/export` 的 `Nothing to export`、
> `/compact` 的 `Nothing to compact` 走同一条路 —— 所以它显示为信息性 toast，
> 不是错误。没有东西失败；只是暂时没有东西可复制。codex 的措辞保留了，但它的
> 错误类别没有。`Action::Copy` 现在自己携带文本（`Copy(String)`），因为空的那一
> 情况永远到不了边界。`Copy failed: {e}` 仍然是错误。

> **更新（2026-07-08）：** `/copy` 的确认从一条提交的 `Role::System` /
> `Role::Error` 消息移到了一个瞬时 toast（`main.rs` 里的 `present_toast`，
> `App::show_toast`）。下面的剪贴板 I/O 没有变；只有表面不同。
> `commit_system_notice` / `COPY_OK_NOTICE` / `COPY_EMPTY_NOTICE` 这些字符串
> 仍然存在（现在字符串喂给 toast；那个提交 helper 仍然服务于空 `!` shell 的
> 提示）。本文件余下的部分描述的是原本那套提交式 notice 的设计，作为背景。

## codex 是怎么做的（移植自 `/tmp/codex/codex-rs/tui`）

- `slash_command.rs`：一个 `Copy` 变体，描述是 `"copy last response as
  markdown"`，**没有行内参数**，在侧对话里和**任务进行中**都可用，只在 Android
  上隐藏。
- `chatwidget/interaction.rs::copy_last_agent_markdown_with` 复制
  `transcript.last_agent_markdown` —— 最近一条**已完成的** agent 消息。空/缺失
  → 错误 `No agent response to copy`；复制的闭包 `Ok` → 信息
  `Copied last message to clipboard`；`Err(e)` → `Copy failed: {e}`。
- `clipboard_copy.rs`：一条**感知环境的兜底链** —— 在 SSH 上它用 tmux/OSC 52
  （从不用原生剪贴板，那会写到*远端*机器上）；本地则依次试 **arboard** → WSL
  PowerShell → OSC 52/tmux。它在整个 app 生命期内持有一个 `ClipboardLease`
  （Linux 上就是活的 `arboard::Clipboard`），这样 X11/Wayland 的选中内容不会在
  设置的那一刻就被丢掉。
- 默认按键绑定是 **Ctrl+O**（可重映射）。

## 映射到这套代码

### 我们复制什么（`App::last_assistant_text`）

> **更新（2026-08-26）：** 它复制的是**屏幕正在显示的那段对话**。在一个 agent
> session 视图里，那就是*被查看 agent* 自己的 transcript，而不是
> `App::history` —— 也就是每个其他感知视图的读取者都走的那条
> `viewed_agent()` 分支。屏幕正显示一个 subagent 的内容时复制 lead 的最后一条
> 回答，就是被报的那个 bug（`docs/agents/agent-view-streaming.md`）。下面的一切
> 描述的仍然是主视图的情形，那里没有变化。

`App::history` 中**最后一条 `role == Role::Assistant` 的
`HistoryItem::Message`** 的文本。我们的 turn 模型会把 assistant 的散文围绕
tool call 拆开（`flush_streaming_segment` 把每个 `ToolStart` 之前的那段文本记成
它自己的一条消息），所以「最后一条回复」就是最后一条被记录下来的 assistant
*片段* —— 与 codex 的 `last_agent_markdown` 呼应，那个同样只是最新的 agent
消息。在途的
`streaming` 缓冲**不**复制：正如 codex 复制最后一份*已记录*的 markdown，我们只
复制已完成的历史。最后一条 assistant 消息为空或缺失 → `None`（无内容可复制的
那条路径）。纯的，有单元测试。

### 命令 + 派发（纯）

- `COMMANDS` 增加 `{ name: "copy", description: "Copy the last response to the
  clipboard", effect: CommandEffect::Copy }`。
- 一个新的 `Action::Copy(Option<String>)`：`Some(text)` = 「把这段写到剪贴板」，
  `None` = 「没有东西可复制」。`run_selected_command` 的 `Copy` 分支在像每条命令
  一样消费输入并关闭 palette 之后，返回
  `Action::Copy(self.last_assistant_text())`。*决定*（复制什么文本，或者什么都
  不复制）留在纯核心；循环只做 I/O 并提交那条 notice —— 与
  `Action::Notice`/`PasteImage` 同一种切分。
- **turn 中也能跑。** `on_key` 里 palette 的 `Enter` 分支（`menu_open`）匹配在
  队列/流式分支*之前*，所以 turn 中敲的 `/copy` 立刻派发而不是排队 —— codex 的
  「任务进行中可用」白送。页面开着时 Tab 在它的标签间循环；它也从不排队。

### 按键绑定 —— 刻意省略

codex 把 `/copy` 绑在 **Ctrl+O** 上，但这里 Ctrl+O 已经是**工具输出视图**了。
所以 `/copy` 只作为一条**斜杠命令**发布（codex 也支持那种形式）。没有键被重新
绑定。

### 剪贴板写入 —— I/O 边界（`clipboard.rs`）

`pub fn copy_to_clipboard(text: &str) -> Result<Option<ClipboardLease>,
String>`：

1. **先试 arboard**（`arboard_copy`）：`Clipboard::new()?.set_text(text)?`。在
   **Linux** 上它返回 `Ok(Some(ClipboardLease))`，持有那个活的 `Clipboard`，这样
   选中内容能存活（X11/Wayland 由拥有它的进程来服务它）；其他平台返回
   `Ok(None)`。
2. **OSC 52 兜底**（`osc52_copy`），只在 arboard 出错时（headless / 没有显示 /
   一个没有剪贴板的 SSH 服务器）：把文本做 base64，塞进
   `\x1b]52;c;{b64}\x07` 这条设置剪贴板的转义，写到 `/dev/tty`（退而求其次写
   stdout）。终端原生 —— 在 SSH 上和 tmux 里都能用（要求 `set-clipboard` ≠
   `off`）。像 codex 一样封顶在 `OSC52_MAX_BYTES`（100 000）。
3. 两个都失败 → `Err`，点名两处失败。

`main.rs` 把返回的 lease 放在一个循环生命期的
`clipboard_lease: Option<clipboard::ClipboardLease>` 里（每次 `/copy` 替换），
与 codex 的 `chatwidget.clipboard_lease` 完全一致。

**纯**的那些部分 —— `base64_encode` 和 `osc52_sequence` —— 在 `clipboard.rs` 里
有单元测试（就像 `frame` 的限流算术那样，而它的任务由 smoke 覆盖）；真正的
arboard/tty 写入仍然只在边界里、只由 smoke 覆盖，遵从这个模块「I/O 不写单元
测试」的规则。

### 反馈 —— 循环的 `Action::Copy` 分支（`main.rs`）

- `None` → `commit_error_notice("No agent response to copy")`（红色
  `Role::Error`）。
- `Some(text)` → `clipboard::copy_to_clipboard(&text)`：
  - `Ok(lease)` → `clipboard_lease = lease;` 然后
    `commit_system_notice("Copied last message to clipboard")`（青色
    `Role::System`，也就是 codex 的信息事件）。
  - `Err(e)` → `commit_error_notice(&format!("Copy failed: {e}"))`。

`commit_system_notice` 是一个新的边界 helper，镜像 `commit_error_notice`（先刷
掉当前流式片段，好让 notice 插在它*之后*，然后 `record_system_message` +
`insert_before`）；既有的 `Action::Notice` 分支被重构成走它。这两个固定字符串
作为 `pub const COPY_OK_NOTICE` / `COPY_EMPTY_NOTICE` 活在 `app/commands.rs`
里，与 codex 逐字一致。

## 与 codex 的已知差异

- **没有按键绑定** —— 这里 Ctrl+O 是工具输出视图，所以 `/copy` 只是一条斜杠
  命令。
- **不做 SSH/WSL 检测。** 我们总是先试 arboard；在一台*有*剪贴板服务器的远端主机
  上，arboard 会设置*远端*的剪贴板（codex 在 SSH 上会跳过 arboard）。常见的
  headless 情形（没有显示）会正确地落到 OSC 52。没有 WSL PowerShell 那条路。
- **裸 OSC 52，没有 tmux 透传包装。** 我们依赖 tmux 原生的 OSC 52 转发
  （`set-clipboard external`，现代默认值），而不是 codex 的 `\x1bPtmux;…` DCS
  包装 —— 更简单，而且这也正是 tmux 自己的缓冲所捕获的东西（所以它可以被 smoke
  测到）。要求在 tmux 内 `set-clipboard` ≠ `off`。
- **不压 arboard 的 stderr。** codex 用 libc 的 `dup2` 压掉 X11/NSLog 的噪声；
  这个 crate `forbid` 了 `unsafe`，所以我们跳过它。
- **没有 rewind 历史。** 我们没有对话回滚，所以 codex 那个「只有最近 32 条回复可
  供 /copy」的淘汰情形并不存在。

## 测试

- `app`（单元）：`last_assistant_text` 返回最后一条 assistant 消息，跳过更晚的
  user/system/tool 条目，在没有 assistant 消息时是 `None`（空消息也是）。经
  回车（以及 Tab）的 `/copy` 返回 `Action::Copy(text)`，消费输入，关闭 palette；
  没有 assistant 消息时它返回 `Action::Toast(COPY_EMPTY_NOTICE)`，在 agent 的
  session 视图里与主视图里一样。turn 中敲的 `/copy` 仍然派发（palette 胜过
  队列）。
- `clipboard`（单元，只测纯 helper）：`base64_encode` 对着已知向量（0/1/2 个
  尾随填充的情形以及空串）；`osc52_sequence` 把 base64 框成 `\x1b]52;c;…\x07`，
  并拒绝超过 `OSC52_MAX_BYTES` 的输入。
- `src/tui/`（smoke，新的 Phase 28）：在一个 `set-clipboard on` 的 tmux session
  里发一条消息，让 dummy 的回复收尾，敲 `/copy`，回车 —— 断言 (a) 面板上显示
  `Copied last message to clipboard`，(b) `tmux show-buffer` 里持有回复的尾部
  （OSC 52 兜底到了剪贴板，因为 headless 环境没有 arboard）。还没有回复时敲
  `/copy` 会以暗色的信息 toast 配色显示 `No agent response to copy`。
