# Textarea —— 一个 codex 风格的可编辑输入框

输入框过去是**只追加**的：`App.input` 是一个 `String`，只会在尾部变长（`push`）
或在尾部变短（`pop`）。光标永远在末尾；Left/Right/Up/Down/Home/End/Delete 什么
也不做。你没法回头改已输入内容中间的一个字符。

本文描述它的替代品：一次聚焦的移植 —— 把 openai/codex 的
`bottom_pane/textarea.rs` 编辑内核搬过来，得到一个能在*任意位置*插入/删除的真实
光标，加上 codex 的移动模型 —— 收成一个小 `textarea` 模块（`src/textarea.rs`），
并在其上再加**readline 那一套**（按词移动、kill 键、Ctrl+字母的光标键 —— 见下文
*终端快捷键*）。codex 的 vim 模式、原子化的 `@mention` 元素、emacs 的
kill-ring/yank、密码掩码、搜索高亮和可配置的 keymap 系统都**不在范围内**
（codex 特有的，没移植）。

## 模型

```rust
pub struct TextArea {
    text: String,                              // the raw UTF-8 draft
    cursor: usize,                             // byte offset; always a grapheme boundary
    wrap_cache: RefCell<Option<WrapCache>>,    // cached wrapped rows, keyed on width
    preferred_col: Option<usize>,              // remembered column for vertical motion
}
struct WrapCache { width: u16, rows: Vec<Range<usize>> }
```

这与 codex 的 `TextArea` 完全对应（同样那四个承重字段），减去
vim/元素/kill-buffer/keymap 的状态。

- **`text` + `cursor`** —— 光标是 `text` 里的一个字节偏移，保持在字素簇的边界
  上（所以 CJK、emoji-ZWJ 序列和组合附标都整体移动）。编辑是在*光标处*插入/
  删除，不是末尾。
- **`wrap_cache`** —— codex 的技巧 #5。把输入折成视觉行的计算只在宽度变化时
  重做；每次编辑都会清掉缓存（`wrap_cache.replace(None)`）。它按视觉行存字节
  区间，好把光标映射到一个 (row, col) 再映回来。缓存由渲染路径懒填充
  （`wrapped_rows(width)`），于是垂直移动不必知道终端宽度就能读它（见下）。
- **`preferred_col`** —— 当你在长度不同的行之间上下移动时，你*想要*的那一列
  会被记住，所以中间一行短行不会把光标永久拽到左边。任何水平移动或编辑都会
  清掉它。

## 换行：字节区间，而不是字符串

`ui::wrap_text`（给 assistant 的 markdown 用的）返回 `Vec<String>` 并把连续空白
压掉 —— 对散文没问题，对编辑器是错的，而且它丢掉了光标所需的字节偏移。
textarea 改为折成**字节区间**（`wrap_rows(text, width) -> Vec<Range<usize>>`）：

- 每个区间是一个视觉行的*显示*切片（`text[start..end]`），保留用户的确切字符
  （包括空格）。
- 贪心按词换行，形状与 `ui::wrap_text` 相同：显式的 `'\n'` 被尊重（一个空的
  逻辑行是一个空区间），软换行会吃掉断点处那串空格，而长于宽度的单词在字素
  边界上硬断。行首缩进与行尾的空格串*显示*出来（所以行尾空格之后的光标看得见），
  并且像单词一样参与换行，因此没有任何一行 —— 也没有任何光标列 —— 会宽过输入
  框。
- 相邻两行之间可能有字节*间隙* —— 软换行处吃掉的那些空白 —— 所以这些区间是
  显示区间，不是严格的铺砌。
- 一行可以填满 `width` 的每一列，而这样一行末尾的光标落在 `col == width`，
  超出它一格。那一格存在是因为**每个输入框都在自己的文本宽度之外多留一列给
  光标**（`ui::layout::text_field_width`、`CURSOR_COLUMN`）：composer 在
  `terminal − 2 (the prompt) − 1` 处折它的草稿，而 ask 模态框的条目与 Tab 的
  修订字段都用同一个辅助函数量自己的尺寸。所以一个本该落在输入框最后一列的
  单词改为折到下一行 —— Claude Code 的规则：

  ```
  ❯ ..................................................................
    who
  ```

  另一种做法 —— 让文本填满这一行，把光标安置在下面一个空行上（codex 的 `+1`
  哨兵，这个 textarea 过去还重新推导过它）—— 会把草稿紧贴终端边缘，光标独自
  坐在下面那一行，读起来像是用户从没打过的换行。

发出之后的草稿也保留它的空白：用户消息、`!` shell 的头部和各条 notice 都经
`ui::wrap_output` 渲染（按词边界、保留空白、显示时把 tab 展开），所以 `a    b`
和粘贴进来那一行的缩进，在气泡里和输入框里看起来一样。只有 assistant 的
markdown 会压掉连续空格，因为 markdown 就是这样。

codex 自己的 `wrap_ranges` 建立在 `textwrap` crate 上（Cow/指针运算，以及给
文本末尾光标的一个 `+1` 哨兵字节）。我们不依赖 `textwrap`，所以这是同一个贪心
算法的一次干净重推导，从零开始单元测试 —— 减去那个哨兵：上面那个光标列就是
它的替代品。

### 光标 ↔ (row, col)

- **光标 → (row, col)**（`cursor_row_col`）：行是*最后一个* `start <= cursor` 的
  折行；列是 `text[row.start .. min(cursor, row.end)]` 的显示宽度 —— 当折行把
  一行恰好填满时，行尾那里就是 `width` 本身，也就是上面那格预留列。在共享边界
  处偏好靠后的那一行，与 codex 的 `partition_point` 规则一致，所以正好坐在一个
  硬断单词块边界上的光标显示在下一行的开头；在软换行处，被吃掉的那个空格让光标
  仍留在自己这一行上。
- **(row, target_col) → 光标**：逐字素走过这一行，累加显示宽度，直到超过
  `target_col`。垂直移动用它。

## 移动

| 键 | 方法 | 行为 |
|-----|--------|-----------|
| ←/→（Ctrl+B/Ctrl+F） | `move_left`/`move_right` | 一个**字素**；清掉 `preferred_col`（Ctrl+B 只在没有可后台化的东西在跑时才算 —— 见下） |
| ↑/↓（Ctrl+P/Ctrl+N） | `move_up`/`move_down` | 跨**折行后的视觉行**，保留 `preferred_col`；折行缓存冷时回退到逻辑行移动 |
| Home/End（Ctrl+A/Ctrl+E） | `move_home`/`move_end` | 当前**逻辑**行的开头/结尾（`'\n'` 之间） |
| Alt+B/Alt+F、Ctrl/Alt+←/→ | `move_word_left`/`move_word_right` | 按**readline 的单词**（一串字母数字 —— `foo/bar.txt` 是三个落点）；左移到单词开头，右移到它结尾（Emacs 的 `forward-word`） |
| Backspace（Ctrl+H） | `delete_backward` | 删光标前一个字素 |
| Delete | `delete_forward` | 删光标处那个字素 |
| 字符 | `insert_char` | 在光标处插入 |
| Ctrl+J / Alt+Enter / Shift+Enter | `insert_newline` | 在光标处插入 `'\n'`（见 `docs/input/shift-enter.md`） |

垂直移动读折行缓存时**不带宽度参数** —— 与 codex 的 `move_cursor_up`/`down`
完全一样。缓存由最近一次渲染填充（那次渲染知道宽度）；在一次编辑与下一次渲染
之间缓存是冷的，移动就回退到逻辑行导航。正是这一点让 `App::on_key` 与宽度无关：
纯的状态机永远不必知道终端尺寸。

Up/Down 是*视觉*的（折行），而 Home/End 是*逻辑*的 —— 这是 codex 的行为，为了
保真而保留。

## 终端快捷键（readline 那一套）

composer 应答 shell 的编辑键。上面那张表已经列出了这些移动键的对：**Ctrl+A/
Ctrl+E**（行首/行尾）、**Ctrl+B/Ctrl+F**（一个字素）、**Ctrl+P/Ctrl+N**（上下
行）、**Alt+B/Alt+F** 与 **Ctrl/Alt+←/→**（按词）。kill 键删的是*区间*：

| 键 | 区间 | 备注 |
|-----|------|-------|
| Ctrl+W | `prev_unix_word_boundary()..cursor` | shell 的 unix-word-rubout：先退过空白，再吃掉整段非空白 —— `src/main.rs` 作为一个词删掉 |
| Alt+Backspace / Ctrl+Backspace | `prev_word_boundary()..cursor` | readline 的 backward-kill-word —— 遇到标点就停，所以同一个路径要杀三次 |
| Alt+D / Alt+Delete / Ctrl+Delete | `cursor..next_word_boundary()` | 向前删一个词 |
| Ctrl+U | `cursor_line_start()..cursor` | 删到逻辑行的开头 |
| Ctrl+K | `cursor..cursor_kill_end()` | 删到逻辑行的结尾 —— 就在结尾处它连 `'\n'` 一起吃掉（Emacs 的 join），所以它从不是一个死键 |

三处刻意的例外：

- **词的分类用 readline 的，不是 UAX#29 的。** Unicode 分词会把 `bar.txt` 并成
  一个词（它的字母间 `.` 规则）；终端肌肉记忆却期望那个点能止住一次 Alt+B /
  Alt+Backspace。这里的一个“词”是一串字母数字，逐字素走，所以 emoji/CJK 仍然
  整体移动。
- **textarea 暴露的是 kill *落点*，不是 kill 方法**（`prev_word_boundary`、
  `prev_unix_word_boundary`、`next_word_boundary`、`cursor_line_start`、
  `cursor_kill_end`）：composer 必须在删除之前，把一个区间扩到覆盖它碰到的任何
  粘贴占位符 `[Pasted Content N chars]` / `[Image #N]`（`App::kill_span` —— 一次
  kill 对占位符来说仍像 Backspace 一样原子，被吞掉的那一对丢弃，被杀掉的图片的
  临时文件交给边界；`docs/input/paste.md`、`docs/input/image-paste.md`）。之后
  每次 kill 都像 Backspace 一样重新推导 palette/`@`/`$` 条带与 shell 模式
  （`App::kill_and_refresh`）。
- **session 键先占住自己的组合。** Ctrl+B 在有一个可后台化的命令或前台 agent
  组在跑时是*移到后台*（`App::can_move_to_background`、
  `docs/tools/background.md`），其余时候是光标左移；↓ 保留它的页脚行走
  （shell 指示器、agent 名单），而 Ctrl+N 从不行走 —— 它纯粹是 ↓ 的编辑那一
  半。Ctrl+P/N 确实会驱动已打开的 palette/`@`/`$` 条带的选择，和方向键一样。

同一套键在权限 prompt 的 Tab 修订字段和 ask 模态框的自由文本条目里也能用
（`App::edit_amend` —— 那里 Ctrl+P/N 是普通光标键，而 Ctrl+B 永远是 ←，因为从
模态框内部没有任何可后台化的东西可操作）。Ctrl+R 的搜索行是它自己的键世界，
不变。

两个 session 级按键让了位：

- **Shift+Tab** 循环权限模式（原来是 Ctrl+A）—— Claude Code 自己用的就是这个
  键，`docs/state/permissions.md`；在一个 `write`/`edit` 权限 prompt 里它仍然
  *是*选项 2，而该选项的标签会标出 `(shift+tab)`。
- **Ctrl+T** 循环思考模式（原来是 Shift+Tab），`docs/providers/reasoning.md`。

（Ctrl+L 仍然刻意不绑定。）

## 集成

- **`App.input`** 是一个 `TextArea` 而不是 `String`。`on_key_conversation` 把
  编辑/导航键路由给它；`Action::Submit` 是 `self.input.take()`（返回文本并重置
  编辑器）。斜杠命令 palette 仍然完全像以前那样按
  `command_query(self.input.text())` 取键 —— 当 palette 打开时它拦截
  ↑/↓/Tab/Enter/Esc，否则那些键驱动光标。
- **`ui/live.rs` + `ui/layout.rs`** 从 textarea 渲染：`render_live`/
  `cursor_position` 向它要 `display_rows(width)` 和 `cursor_row_col(width)`，
  而输入框现在滚动以保持**光标**可见（codex 的 `effective_scroll`），不只是保持
  尾部。`live_height` 用 `input.row_count(width)` 量输入框的尺寸。

`main.rs` 与 `term.rs` 的形状不变：`live_height(&app.input, …)` 与
`cursor_position(area, app)` 保留签名（现在透过 `TextArea` 读）。

## 刻意*不*在这里的东西（YAGNI / 超出范围）

Vim 模式、原子化的 `@mention` 文本元素、emacs 的 **kill-ring + `Ctrl+Y` yank**
（kill 键只删；不会暂存任何东西供重新插入）、密码掩码、只用于渲染的搜索高亮，
以及运行时的 keymap 配置系统。每一个都是 codex 的功能，本应用里没有对应物；
将来再加是纯增量，不会改变上面的模型。
