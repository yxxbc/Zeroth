# Ctrl+R 历史搜索 —— 对已提交输入的反向增量搜索

日期：2026-06-12

## 目标

在 composer 里按下 **Ctrl+R，会打开一个 shell 风格的反向增量搜索**，作用于
↑/↓ 那份输入历史（`docs/input/input-history.md`）：敲字即过滤，最新的匹配在
composer 里预览，再按 Ctrl+R 步进到更旧的匹配，Enter 把预览接受成一份可编辑的草
稿，Esc 精确恢复搜索之前的东西。这是 openai/codex 的 Ctrl+R 搜索的移植 ——
`docs/input/input-history.md` 在移植 ↑/↓ 时明确没做的那一块。

## codex 怎么做（移植自 `/tmp/codex/codex-rs/tui`）

`bottom_pane/chat_composer/history_search.rs`（会话 + UI 生命周期）和
`bottom_pane/chat_composer_history.rs::search`（遍历）：

- **一个会话**（`HistorySearchSession`）持有**原始草稿快照**、归页脚所有的
  **查询**，和一个**状态** ∈ Idle / Searching / Match / NoMatch。Ctrl+R 以空查
  询、*无预览*打开它 ——「打开 Ctrl+R 从不自己预览最新那条」；它还会先关掉任何
  打开的弹窗，并在快照之前冲掉一批悬着的粘贴。
- **搜索活着时每一个键都在**正常的 composer 处理之前**被吃掉**
  （`handle_history_search_key`）：普通字符**追加到查询**并从最新那条重新开始搜
  索；Backspace/Ctrl+H 弹掉一个字符；Ctrl+U 清空查询；**Ctrl+R / ↑ 步进更旧**，
  **Ctrl+S / ↓ 步进更新**；**Enter 只在 Match 时接受**（会话关闭，预览的文本留
  成一份普通的可编辑草稿，光标在末尾）；**Esc 和 Ctrl+C 取消**，恢复快照的草稿
  *包括它的光标*；**一次 bracketed 粘贴也追加到查询**（readline 的
  paste-into-isearch —— `App::on_paste` 以搜索开着为条件）：它绝不编辑底下那份
  注定要被换掉的预览，绝不翻转 shell 模式、也不打开 `@` picker，而控制字符被压
  平成空格，好让那一行查询活得下来；别的任何东西都被吞掉。
- **匹配**是一个**不区分大小写的子串**测试（`to_lowercase()
  .contains`），按**最新 → 最旧**遍历，每会话**跳过重复文本**
  （`seen_texts`）。步过任一端返回 `AtBoundary`，它*保持*当前匹配 —— 绝不在列
  表尽头闪一下「no match」。一个什么都没匹配上的查询显示 **NoMatch**，把原始草
  稿还给 composer，但搜索仍然开着等更多输入。
- **页脚那一行**（它的提示行）渲染这次搜索：
  `reverse-i-search: ` 暗色 + 查询**青色**；Match 时追加
  `enter accept · esc cancel`（键名青色**加粗**，标签暗色）；NoMatch 时追加
  `  no match`，**红色**。**硬件光标坐在查询末尾**，就在那行页脚里
  （`history_search_cursor_pos`），不在 textarea 里 —— 这正是 shell
  reverse-i-search 的手感。
- **预览的匹配**在 textarea 里把每一处不区分大小写的查询出现都高亮成
  `REVERSED | BOLD`（`history_search_highlight_ranges` →
  `case_insensitive_match_ranges`，一个字符折叠映射，好让比如 `İ` 正确折叠），
  直到匹配被接受。
- 接受会把 codex 那个共享的历史光标落在匹配上（`search_match` 设
  `history_cursor`/`last_history_text`），所以**接受之后紧接着按 ↑，会步进到比被
  接受那条更旧的条目**。
- codex 的搜索还横跨它**持久的跨 session 历史**，用异步取条目（Searching 状态
  / `Pending` 结果）。我们让搜索在 `InputHistory::entries` 之上完全同步 —— 但
  `entries` 现在是**启动时从一个磁盘上的历史文件种出来的**
  （`docs/input/history-persistence.md`），所以搜索也横跨过去的 session，而不需
  要 codex 那套异步 pending 机器：整个文件一次加载，而不是按偏移去取。

## 映射到这套代码库

### 纯状态（`app::HistorySearch`，`App` 上的一个字段）

```rust
pub struct HistorySearch {
    snapshot: TextArea,     // the draft (text + cursor) from before search
    pub query: String,      // the footer-owned query
    pub state: SearchState, // Idle | Match { selected } | NoMatch
}
```

`TextArea` 派生了 `Clone`，所以快照/恢复就是 codex 的 `ComposerDraft` 快照
—— 取消恢复文本*和*精确的光标。`Match.selected` 索引那个推导出来的、最新在前的
去重匹配列表；逐击键推导（像调色板的 `matching_commands`）取代了 codex 缓存的
`unique_matches` —— 搜索开着时条目不可能变（搜索吃掉每一个键，所以没有任何东西
能提交/排队/记录）。

`InputHistory` 长出三个方法：`search(query) -> Vec<usize>`（含该查询的条目索引，
不区分大小写，最新在前，每个重复文本只取最新的那处出现）、`entry(index)`，以及
`resume_at(index)` —— 接受时把 ↑/↓ 浏览落在被接受的那条上，于是 ↑ 从它继续往
旧走（codex 的共享光标）。

### 按键处理

`App::on_key` 在 `history_search.is_some()`（会话视图）时把**每一个**键都路由给
`on_key_search` —— 在全局的 Ctrl+C/Ctrl+O 分支*之前*，因为搜索重新定义了这两
者：**Ctrl+C 取消搜索**（恢复草稿；它既不清空也不退出 —— codex），而 **Ctrl+O
先取消再打开 overlay**（全局开关照常工作；搜索状态从不漏进 overlay）。
**Esc 即使回合中途也取消搜索** —— 调色板关掉那一先例；它从不抵达中断。Idle/
NoMatch 上的 Enter 被吞掉（搜索保持开着），所以一次搜索永远不能提交或排队一条消
息。预览通过 `set_text` 写 composer 而不重新推导调色板（搜索拥有按键）；**接受
会重新推导**，像 ↑ 回溯那样，所以接受一个裸 `/token` 会重新打开调色板。

Ctrl+R 本身是 `on_key_conversation` 里的一个新分支：它关掉调色板、快照草稿、打
开会话（一个已经开着的 `?` 快捷键 band 会先关掉 —— 已有的任意键规则）。搜索开着
时，Ctrl+R 转而步进更旧。

### 渲染（`ui/footer.rs`）

搜索行占用**会话页脚的那个槽位** —— codex 把它渲染在自己的页脚提示行里：
`footer_rows` 在有搜索活着时返回 1（即使没有任何 session info 被注入 —— 和
`{model} · {cwd}` 那行不同，它必须永远显示），`render_live` 在那里画
`search_line` 而不是 `footer_line`，而 `cursor_position` 把硬件光标移到那一行查
询的末尾（夹在宽度之内，codex 的 `history_search_cursor_pos`）。搜索期间不可能有
band 开着（打开搜索会关掉调色板；band 的按键从不抵达 band），所以这个槽位由构造
而言就是空的。几何（`live_height`/`live_layout`）不动：搜索那一行*就是*页脚那一
行。

匹配高亮：`App::search_highlight_ranges` 移植 codex 的
`case_insensitive_match_ranges`（字符折叠映射，好让多字符的小写折叠保持字节区间
对齐）；`render_live` 在这些区间处把输入框的各行切成 span —— textarea 的
`wrapped_rows` 字节区间与 `display_rows` 对齐 —— 并把重叠部分样式化成
`REVERSED | BOLD`。

`?` 快捷键 band 多了一条 `ctrl+r to search history`。

`main.rs` 不变：页脚槽位、光标和 live 高度都流过既有的纯辅助函数。

## 测试

- `app`：Ctrl+R 空闲打开且没有预览；敲字构建查询并（不区分大小写地）预览最新的
  匹配；Ctrl+R/↑ 步进更旧并在最旧处夹住（匹配留着 —— 不闪）；Ctrl+S/↓ 步进更新
  并在最新处夹住；重复文本只匹配一次；Backspace 弹掉查询字符并重新开始（一个
  无匹配的查询能恢复）；Ctrl+U 清回 Idle 并恢复草稿；一个无匹配的查询恢复草稿但
  让搜索开着；Enter 只在 Match 时接受（搜索关闭，文本留着，什么都不提交/排队）
  并把 ↑ 浏览落在被接受的那条上；Idle/NoMatch 上的 Enter 被吞掉；Esc 和 Ctrl+C
  取消并恢复草稿文本*和光标*；回合中途的 Esc 取消搜索，而不取消回合；Ctrl+O 取
  消并打开 overlay；打开会关掉调色板，而敲字从不把它重新打开；接受一个裸
  `/token` 会重新打开调色板；没有历史时敲字显示 NoMatch；高亮区间不区分大小写，
  且只在预览匹配时暴露。
- `ui`：搜索时页脚槽位显示 `reverse-i-search: {query}`（顶掉会话页脚，即使没有
  session info 也在）；Match 追加接受/取消的提示，NoMatch 追加 `no match`；光标
  在页脚行查询的末尾；预览匹配的查询出现在输入框里渲染成 `REVERSED`；快捷键
  band 列出 `ctrl+r`。
- `src/tui/`（smoke，Phase 18）：驱动真二进制 —— Ctrl+R 显示搜索行，敲一个查询
  会在 composer 里预览匹配的历史条目，Ctrl+R 步进到更旧的匹配，Enter 接受它
  （搜索行消失、页脚回来、文本可编辑），一个乱敲的查询显示 `no match` 且草稿被
  恢复，而 Esc 关掉搜索而不退出应用。
