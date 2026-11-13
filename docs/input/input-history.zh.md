# ↑/↓ 输入历史 —— shell 风格的已提交消息召回

> **Ctrl+P/Ctrl+N** 是 ↑/↓ 的终端孪生体（`docs/input/textarea.md`）：同样的 band
> 导航、同样的召回闸门、同样的光标回退 —— 只是 Ctrl+N 从不走页脚（shell 指示器 /
> agent 名单仍然是 ↓ 自己的）。

日期：2026-06-11

## 目标

在 composer 里按 **↑ 召回最后一条已提交的消息**（再按更新更旧的），↓ 往最新的方向
退回一步然后清空 —— 像 openai/codex（以及一个 shell）召回输入那样 —— 同时不破坏
方向键既有的、在多行草稿内部移动光标的职责。

## codex 是怎么做的（移植自 `/tmp/codex/codex-rs/tui`）

`bottom_pane/chat_composer_history.rs`（`ChatComposerHistory`）+
`chat_composer.rs`：

- **条目**是这个 session 提交过的文本，**最新的在末尾**；空提交被忽略，一个**与最
  新的完全相同的条目被折叠**（`record_local_submission`）。记录总是**退出浏览**。
- **闸门**（`should_handle_navigation(text, cursor)`）：只有当 composer 是
  **空的**，或者文本**恰好等于上一次召回的条目**（`last_history_text`）*并且*
  光标坐在文本的最开头或最末尾时，↑/↓ 才浏览历史。其他任何情形 —— 一份打出来的
  草稿、一次被编辑过的召回、光标被移到里面 —— 都落到普通的光标移动，所以召回
  绝不会毁掉进行中的工作。
- **导航**（`navigate_up`/`navigate_down`）：一个 `Option<index>` 光标只在浏览期间
  是 `Some`。从空闲按 ↑ 从最新的开始；在最旧的那一条它返回 `None`，而那个键落到
  光标移动。↓ 越过最新的返回一个**空条目** —— 「清空 composer 并停止浏览」；不浏览
  时按 ↓ 就是普通的光标移动。
- **一次召回替换整份草稿并把光标放到末尾**
  （`apply_history_entry` → `move_cursor_to_history_entry_end`），这正是让*下一次*
  按键仍满足闸门的原因。
- **弹窗优先**：弹窗打开时，↑/↓ 驱动弹窗而不是历史（它们的匹配分支排在前面）。
- **Ctrl+C 的清空会记录被清掉的草稿**进同一份历史（`clear_for_ctrl_c`，由
  `clear_for_ctrl_c_records_cleared_draft` 钉住），所以 ↑ 能把一份被误清的草稿
  找回来。
- Codex 还跨 session 持久化历史（一次异步的 `LookupMessageHistoryEntry` 抓取），并
  提供 Ctrl+R 增量搜索。这份文档移植的是 session 内的 `local_history` 机器；Ctrl+R
  搜索后来在它之上移植（`docs/input/history-search.md`），而**跨 session 持久化也
  是后来加的** —— `entries` 现在在启动时从一个磁盘上的历史文件播种，并在每次提交
  时增长它，所以 ↑/↓ 召回与 Ctrl+R 都跨 session
  （`docs/input/history-persistence.md`）。
  Codex 派发的 slash 命令也会被记录；我们只记录已提交的消息和 Ctrl+C 清掉的草稿
  （YAGNI —— 一次召回的 `/token` 基本只会重新打开 palette）。Ctrl+C 清掉的草稿在
  这个 session 内可召回，但**不被持久化**（codex 把清掉的草稿只留在
  `local_history` 里 —— 见 `docs/input/history-persistence.md`）。

## 到本代码库上的映射

### 纯状态（`app::InputHistory`，`App` 上的一个字段）

同步核心的直接移植，在 `app/input_history.rs` 里，与其余状态类型并列：

```rust
pub struct InputHistory {
    entries: Vec<String>,        // submitted texts, newest at the END
    cursor: Option<usize>,       // Some(index) only while browsing
    last_recall: Option<String>, // what navigation last wrote into the composer
}
```

`record(text)`（退出浏览；忽略空白；折叠相邻重复）、
`should_navigate(text, cursor)`（上面的闸门）、`up() -> Option<String>`、
`down() -> Option<String>`
（越过最新的 `Some("")` = 清空）。全是纯的，通过 `App::on_key` 做单元测试。

`App.input_history` **挺过 `/clear`** —— `/clear` 擦的是对话
（`App.history`），不是 composer 的召回，与 codex 的净体验一致（它的历史甚至跨
session）。

### 按键处理（`App::on_key_conversation`）

既有的 ↑/↓ 分支在光标回退*之前*先尝试一次召回；palette 的菜单打开分支留在两者之上
（codex 的「弹窗优先」）：

```rust
KeyCode::Up => {
    if self.should_browse_history()
        && let Some(text) = self.input_history.up()
    {
        self.recall_input(&text);
        return Action::None;
    }
    self.input.move_up();
    Action::None
}
```

（↓ 与 `down()` 对称。）`recall_input` 就是 `set_text` —— 它已经把光标放到末尾，
即 codex 的召回落点 —— 加上 `refresh_command_menu`，所以召回一个光秃秃的 `/token`
会像敲下它一样重新打开 palette（而*下一次* ↑ 在那之前就是 palette 导航，直到它被
关掉 —— 与 codex 的 slash 弹窗同样的动态）。

### 记录点

- **Enter（提交）**：`Action::Submit` 的分支在把取走的文本返回之前记录它。
- **Ctrl+C 清空 composer**：记录被清掉的草稿（codex 的 `clear_for_ctrl_c`），所以
  ↑ 能撤销这次清空。这堵上了 `docs/design.md` 在清空这一步最初被移植时所记下的
  缺口。

`main.rs`、`ui/` 或协议里什么都不变：召回是纯 `App` 状态，而框本来就把 textarea
持有的内容渲染出来。

## 测试

- `app`：↑ 召回最新的提交（光标在末尾）并步进到更旧的，在最旧处夹住；↓ 步向更新的
  并**在越过最新的之后清空**；不浏览时 ↓ 什么都不做；打出来的草稿永不被毁（↑
  落到光标移动）；编辑一次召回会让方向键回到光标移动（而在 Home/End 边缘重新启用
  召回）；提交把浏览重置到最新；相邻的重复提交被折叠；空白文本永不被记录；Ctrl+C
  清掉的草稿可召回；召回一个 `/token` 重新打开 palette；`/clear` 保留召回历史；
  没有历史时方向键的行为与从前完全一样（既有的光标移动测试保持绿色）。
- `src/tui/`（smoke，Phase 10）：一个回合结束之后，↑ 把已发送的消息再次显示在
  输入框里，↓ 清掉它，而 ↑ + Enter 重新提交它 —— 出现第二份已提交副本和第二个
  回合摘要。
