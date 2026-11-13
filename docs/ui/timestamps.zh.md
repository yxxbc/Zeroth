# Ctrl+O transcript 里的时间戳 —— 设计

日期：2026-06-09（2026-06-10 修订：仅用户消息、右下角、不要秒）

## 目标

为**用户消息**显示一个墙上时钟时间戳 —— **只在** Ctrl+O tool 输出 overlay
（全屏对话 transcript）之内。AI 回复、tool call，以及已提交的 `Done for Ns`
回合摘要都**不**显示时间戳（每一个仍然*记录*一个）。inline 对话保持原样：那里
没有时间戳。

格式：**12 小时制带 am/pm，不显示秒，不显示日期**，例如 `03:20 AM`
（`chrono` 的 `%I:%M %p`）。

位置：**用户消息的右下角** —— 消息下面一个空行，然后时间戳暗色地独占一行，紧贴
右边缘：

```
❯ thanks

                                                     03:20 AM

● let me check the file for you
  it wraps onto a second line here
● Read(src/main.rs)
  fn main() -> io::Result<()> {
  …
Done for 3s
```

## 为什么是这个形状

库（`app`、`ui`）是纯的、确定性单元测试的 —— 墙上时钟不能住在里面。所以：

1. **时钟在 I/O 边界处注入。** `App` 持有一个可选的
   `clock: Option<fn() -> String>`。`main.rs` 在启动时设置一次基于
   `chrono::Local` 的真实时钟（`App::set_clock`）；测试把它留成 `None`（→ 空
   时间戳）或设一个固定的桩。唯一的杂质（`chrono`）住在 `main.rs`，也就是已有的
   I/O 边界里。
2. **每个条目存一个预先格式化好的 `String`。** `Message` 和 `ToolCall` 携带一个
   `timestamp: String`，在它们被记进历史的那一刻打戳（`record_user_message`、
   `record_system_message`、`flush_streaming_segment`、`finish_stream`、
   `fail_stream`、`end_tool`）—— 也就是完成时间。`ui` 保持愚钝：它只渲染存下的
   那个字符串。
3. **时间戳只加在 `transcript_lines` 里**，而且只给 `Role::User` 消息
   （`ui::user_stamp_lines`：一个空行 + 右对齐到视图宽度的暗色时间戳）。共享的
   行构建器（`message_lines`、`tool_lines`、`tool_full_lines`、`summary_lines`）
   不动，所以 inline 视图（`render_live`、`conversation_lines`）永远显示不了
   时间戳。时间戳独占一行，就不需要预留右列 —— 每个条目都折进整个宽度。

## 范围

每一个记录下来的 `HistoryItem` 仍然会**打戳**（用户、assistant、tool、摘要，以及
system/error 各 notice）—— 数据是齐的 —— 但只有用户消息的时间戳被**显示**。
时间戳**只**出现在 Ctrl+O overlay 里；inline 对话和流式预览不变。

## 测试

- `app`：用固定的桩时钟，`record_user_message` / `end_tool` /
  `finish_stream` 用时钟的值给条目打戳；没有时钟时时间戳为空（所以已有的相等性
  测试不受影响）。
- `ui`：`transcript_lines` 把用户的时间戳单独放在消息下面一条右对齐的暗色行上
  （在一个空行之后），空时间戳时 entirely 省掉那行时间戳，在 assistant/tool/
  摘要条目上**不**显示时间戳，而 inline 的构建器（`conversation_lines`、
  `message_lines`）从不包含它。
- `src/tui/`（smoke）：overlay 显示一行右对齐的 `hh:mm AM/PM`，任何地方都没有
  带日期或带秒的时间戳。
