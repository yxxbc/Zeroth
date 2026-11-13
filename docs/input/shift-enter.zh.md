# Shift+Enter 插入换行 —— 设计

日期：2026-06-15

## 目标

在 composer 里按下 **Shift+Enter**，会在光标处插入一个换行（输入框长高）而不是
提交 —— 就像 openai/codex 让你编排一条多行消息那样 —— 而一个普通的 Enter 仍然
提交。这个功能还必须在报不出「带修饰键的 Enter」的终端上优雅降级。

## codex 怎么做（移植自 `/tmp/codex/codex-rs/tui`）

- **它打开 keyboard enhancement。** `tui.rs::set_modes` 调
  `keyboard_modes::enable_keyboard_enhancement`，后者用 crossterm 的
  `PushKeyboardEnhancementFlags(
  DISAMBIGUATE_ESCAPE_CODES | REPORT_EVENT_TYPES | REPORT_ALTERNATE_KEYS)` 推
  入 kitty 键盘协议标志。这是承重的那一块：没有它，终端把 Shift+Enter 发成一个
  裸的 `\r` —— **与 Enter 逐字节相同** —— 所以分辨不出来。有了
  `DISAMBIGUATE_ESCAPE_CODES`，Shift+Enter 以 `CSI 13;2u` 抵达，crossterm 把它
  解析成 `KeyEvent{ code: Enter, modifiers: SHIFT }`。
- **它在退出时弹出这些标志**（`restore_keyboard_enhancement_stack`），并在进程
  退出时做一次更强的重置，好让父 shell 从不继承增强的按键报告。
- **它把三个键绑到「插入换行」**（`keymap.rs`、`editor.insert_newline`）：
  `Ctrl+J`、`Shift+Enter` 和 `Alt+Enter`。`Ctrl+J` 是**通用的兜底** —— 在 raw
  模式里每个终端都交付它，不需要任何增强。
- 它有一个环境逃生口（`CODEX_TUI_DISABLE_KEYBOARD_ENHANCEMENT`），并在 WSL 下的
  VS Code 里自动关掉增强，那里这个协议会出毛病。
- 它刻意避开 crossterm 那个阻塞的 `supports_keyboard_enhancement()` 探测（在从
  不回答的终端上最多 2 秒），改用自己那 100ms 的。

## 为什么这*大部分*已经在这儿了

`App::on_key_conversation` 早就把 `Enter + (ALT | SHIFT)` 映射到
`self.input.insert_newline()`，还有通过的单元测试 —— 但它在**多数终端上是死代
码**，因为我们从未打开 keyboard enhancement，所以终端在 Enter 上根本报不出
SHIFT 修饰键。缺的两块是终端侧的启用，和 `Ctrl+J` 这个兜底。

## 映射到这套代码库

### 按键处理（`App::on_key_conversation`）—— 纯的，有单元测试

三个键插入换行；一个普通的 Enter 提交：

- `Enter` 带 `ALT` **或** `SHIFT` → `insert_newline()`（早已存在）。
- `Ctrl+J` → `insert_newline()`（**新**）。在 raw 模式里字节 `0x0A` 在每个终端
  上都解析成 `Char('j') + CONTROL`（crossterm 的 `parse.rs`：`\n` 只在**不**处于
  raw 模式时才映射到 Enter；否则它落进 `0x01..=0x1A` → `Ctrl+<letter>` 那条分
  支）。所以 `Ctrl+J` 在**没有** keyboard enhancement 时也能当换行键 —— 那个可靠
  的兜底。增强打开时它经 `CSI 106;5u` 抵达为同样的 `Char('j') + CONTROL`，所以
  一个分支覆盖两条路。
- `Enter`（无修饰键）→ 提交 / 排队 / 运行 shell，和以前完全一样。

`Alt+Enter` 在多数终端上早就好用（它发 `ESC` + `\r` →
`KeyEvent{Enter, ALT}`）；`Ctrl+J` 覆盖其余的。这个分支就坐在
`on_key_conversation` 里既有的 Enter 分支旁边，所以所有插入换行的逻辑读起来是在
一起的。

### 终端侧启用（`term.rs::init` / `restore` / panic hook）—— I/O 边界

`InlineViewport::init` 在同步的光标查询**之后**、在 `EventStream` 被创建**之
前**推入 kitty 标志：

```rust
PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
```

两条不变量让这件事安全：

1. **不变量 1（单一 stdin 读取者）。** 一次 *push* 是发了不管 —— 终端对它**不
   回任何东西** —— 所以它不加第二个 stdin 读取者，也偷不走光标位置（DSR）的回
   复。（它也是在 `get_cursor_position` 之后发出的，所以光标查询跑在它原本那个
   干净的环境里。）
2. 我们**只推 `DISAMBIGUATE_ESCAPE_CODES`** —— 唯一能把 Shift+Enter 与 Enter 区
   分开来的标志。我们不需要 codex 的 `REPORT_EVENT_TYPES`（按键释放事件；循环本
   来就过滤到 `KeyEventKind::Press`）或 `REPORT_ALTERNATE_KEYS`（基础布局键）。

`restore()` 弹出这个标志（仍在 raw 模式下），panic hook 也弹它，所以无论干净退出
还是崩溃，都不把增强的按键报告留给 shell。两者都只在 `init` 真的推过时才弹
（`keyboard_enhanced` 字段 / 捕获下来的 bool），保持 push/pop 栈平衡。

我们刻意**跳过** crossterm 的 `supports_keyboard_enhancement()` 探测 —— 它在从
不回答的终端上阻塞最多 2 秒（这正是 codex 自己写一个的原因）。一次无条件的 push
会被不支持这个协议的终端直接忽略，所以付那笔延迟什么都换不来。

### 逃生口（`ZEROTH_DISABLE_KEYBOARD_ENHANCEMENT`）

一个真值（`1`/`true`/`yes`，不区分大小写）让增强保持**关闭**，为那个罕见的、
kitty 协议会出毛病的终端（codex 的
`CODEX_TUI_DISABLE_KEYBOARD_ENHANCEMENT`）。解析是
`term.rs` 里一个纯的 `keyboard_enhancement_disabled(Option<&str>) -> bool` 辅助
函数，所以即使周围的终端 I/O 只有 smoke 覆盖，它也有单元测试。逃生口设上时
`Ctrl+J` 和 `Alt+Enter` 仍然插入换行 —— 只有 Shift+Enter 变哑（没有这个协议它报
不出来）。

## 测试

- `app`（单元）：`Ctrl+J` 插入换行而不提交，包括流式输出中途；既有的
  `shift_enter_inserts_a_newline_too` /
  `alt_enter_inserts_a_newline_instead_of_submitting` /
  `plain_enter_submits_a_multi_line_message_intact` 测试仍然成立。
- `term`（单元）：`keyboard_enhancement_disabled` 默认关 / 对无法识别的值也关，
  而对那组真值开。
- `src/tui/`（smoke）：Phase 2 早就在 keyboard enhancement 现在打开的情况下驱动
  `Alt+Enter`（`M-Enter`）把输入框撑高（所以它同时守着「启用这个协议没有弄坏普
  通按键交付」—— 普通 Enter、Esc、Ctrl+C 在每个 phase 里都仍然有效）。Phase 23
  通过真实的 terminal+crossterm 路径驱动 `Ctrl+J`：输入框长高，把两行都显示出
  来。
