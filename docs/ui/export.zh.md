# `/export` —— 把对话导出为纯文本

日期：2026-09-22

## 目标

一条 **`/export`** 斜杠命令，把**整段对话作为纯文本**交给用户 —— 每一条消息、
每次 tool call 的完整输出，一如 Ctrl+O transcript 所显示的那样 —— 要么**复制
到剪贴板**，要么**保存成文件**放到工作目录里，文件名是
`conversation-YYYY-MM-DD-HHMMSS.txt`。它是 `/copy` 的兄弟（`/copy` 是最后一条
回复；`/export` 是全部），而作为一个页面，它又是 `/donate` 页面的兄弟：一个只读
的、替换 composer 的 picker，**没有文本输入**，两行就是两个答案，硬件光标藏起
来，打开时拥有每个键，Esc 让 composer 回来 —— 选中之后也回来，因为选中正是这个
页面存在的全部意义。

## 页面

```
────────────────────────────────────────────────────────────────────────
  Export conversation

  The whole transcript as plain text — every message and every tool
  call's full output, as Ctrl+O shows it.

  ❯ 1. Copy to clipboard
    2. Save to file

  Copies the transcript to the system clipboard.

  ↑↓ navigate  enter select  esc close
────────────────────────────────────────────────────────────────────────
```

外框是 picker 家族的 —— 同样的规则、同样的两列内缩、同样的暗色提示行 —— 而框里
坐的东西遵循它所借用的那两个兄弟：

- **标题**穿着这些页面唯一的那套标题装束 —— 粗体，刷上当前主题的横幅渐变
  （`ui::header::page_title_line`，`docs/ui/theme.md`）：一个问题之上的一个
  标题。
- 它下面的**简介**说这次导出*是*什么 —— 一个在两行之间徘徊的用户，在挑任何一行
  之前唯一想知道的那件事。两行暗色换行文本，从不截断：页面的高度就是它自己的行
  数（`docs/ui/view-flow.md`），所以一个窄终端花掉一行，而不是一个词。
- **那两行就是那两个答案**，`/donate` 页面的行形状：`❯` 标记在被高亮的那个上，
  一个绝对 `{n}.` 编号，然后是标签 —— 后面什么都没有。每一行*做什么*不是骑在行
  上的从句（这些行保持一眼可读的宽度），而是**列表下面的描述**，也就是
  `/settings` 菜单的形状：只属于被高亮的那一行。剪贴板那一行说文本去哪里；文件
  那一行点名文件的形状和它落进的目录 ——
  `Writes conversation-YYYY-MM-DD-HHMMSS.txt into ~/zeroth.` —— 也就是页脚所显示
  的那个 session cwd，好让用户在文件存在之前就知道该去哪里找（边界注入之前是
  `the working directory`；纯核心从不读取环境）。
- **选中由颜色显示**：被高亮那行的标记、编号和标签以强调色亮起（标签加粗），另一
  行保持柔和的墨色。这是 palette 的规则 —— 整行亮起来，没有第二个 caret。
- **块与块之间一个空行，绝不用两个。** 页面由恰好一个间隙连接起来的块搭成
  （`/login` 页面的规则），而有一个测试走遍每个宽度，断言绝没有两个空行会叠在
  一起。

这个页面是**静止的** —— 上面没有任何东西在滴答 —— 所以它的 scrollback 流是按行
签署的，像 `/donate` 页面那样；在一个矮终端上，它像每个带框视图那样底部锚定，并
把顶部流进 scrollback（`docs/ui/view-flow.md`）。

## 按键

页面打开时拥有**每一个**键（在 `App::on_key` 的顶端路由，早于 composer 的全局
Ctrl+C/Ctrl+O），用的是 `/donate` 的语法：

| 键           | 做什么                                                     |
| ------------ | ---------------------------------------------------------- |
| ↑ / ↓        | 移动高亮，在两端**环绕**（`wrap_step`）                    |
| Home / End   | 跳到第一行 / 最后一行                                      |
| `1` / `2`    | 跳到那一行**并选中它**（ask 弹窗的规则）                   |
| Enter        | 选中被高亮的那一行                                         |
| Esc / Ctrl+C | 关闭 —— composer 回来                                      |

超过行数的数字不点名任何东西，被忽略；其他一切都被吞掉 —— 一个可打印键从不落到
下面 composer 的草稿里，粘贴也一样（`/settings` 的吞掉规则）。**一次选中就关闭
页面。** 这是这个页面与 `/donate` 分道扬镳的唯一一处：在那里复制会保持页面
打开，因为另一个地址只差一个键；而在这里，同一个页面上没有别的东西可拿，
一个在 `Saved the conversation to …` toast 之上还开着的页面，读起来会像一个
还在等答案的页面。这个页面像每个 picker 一样**在 turn 中**也能用 —— 它只替换
composer，流式 strip 仍在它上面保持自己的行，而导出的内容随即带上实时尾部，与
Ctrl+O 显示的完全一致 —— 而且，像每个 picker 一样，打开时它会把运行中 cell 的
`(ctrl+b to run in background)` 提示抹掉（`App::background_hint_elapsed`），因为
它会吞掉那个提示所宣传的 Ctrl+B。

这个页面上任何地方都不输入文本，所以硬件光标是**隐藏**的（`ui::cursor_visible`
—— 权限提示的规则），而它的*座位*仍然跟着被高亮的 `❯` 行（`menu_marker_seat`），
所以页面关闭时光标回来时是从一个说得通的地方开始走的。

## 没有东西可导出

在一个空对话上（屏幕上的 transcript 里没有记录任何条目）敲 `/export`，得到一个
`Nothing to export` toast（`EXPORT_EMPTY_NOTICE`，即 `/compact` 的
`Nothing to compact` 规则），不打开任何页面：一次用户无需留着的柔和拒绝，绝不是
一个空文件。只有实时尾部从不作数，因为一个 turn 在任何东西流式输出之前就记录了
它的用户消息，所以空历史*就是*空对话。

## 导出的是什么（`ui::export_text`）

**屏幕上的 transcript，作为文本 —— 从上到下。** Ctrl+O 页面自己的那些行，用同一
趟 `TranscriptCache` 遍历搭出来，所以导出和分页器永远不会互相矛盾，从页面开始的
地方开始：终端打开时的那个**启动横幅** —— `Zeroth (v…)` 旁边的 mascot、cwd、
`/ commands   @ files   ? shortcuts` 提示 —— 然后一个空行，然后是对
话。横幅是刻意
保留的：文件是终端当时显示了什么的记录，而一份以会话开始时的样子开头的
transcript，本身就说明了是哪个 tool 写的、哪个版本、在哪里跑的，不必为它另发明一
个页眉。在它下面，每一条消息、每次 tool call 的**完整**输出（内联视图里的
`… +N lines` 折叠是一份屏幕预算；文件没有预算）、每个 `Thought for …` cell 的整
条思维链、压缩标记的摘要、hook 注记、作为普通 cell 的 task 调用 —— 严格按它们
发生的顺序 —— 以及，在 turn 中时，实时尾部：进行中的回复、打开的思考块、运行中的
tool 和它的那些 `⎿ Waiting…` 兄弟、排队的积压。（分页器的
`Nothing here yet.` 占位永远到不了：命令在页面打开之前就拒绝了空对话。）

**在 subagent 的 session 视图里，它是那个 agent 的 transcript**，不是 lead 的
—— `/copy` 的规则（`docs/ui/copy.md`，`docs/agents/agent-view-streaming.md`）：
导出的是屏幕正在显示的那段对话。

**按终端的宽度。** 这些行按边界交进来的宽度渲染（`term.screen().width`），所以
用户看到的就是他得到的：一个表格、一个代码块、一个换行过的段落，都原样落进文件，
与它们在屏幕上排布的一模一样。（一个固定宽度会让文件成为另一种渲染，而不同于用户
决定保留它时正在看的那一种。）

**纯文本。** 样式被丢弃 —— 一个 `Line` 就是它各 span 文本的拼接，所以横幅的渐变
mascot 落成它光秃秃的块状字形 —— 而每一行都丢掉 cell 在屏幕上带的填充：用户的
气泡会补齐到全宽，时间戳是右对齐的，横幅的行在文本之外还带着填充，一张图片预留
若干行空白（`docs/ui/images.md`），而在一个文本文件里那些填充是一个 diff 会显示
出来的噪声，所以每一行都做了 `trim_end`。尾随的空行去掉，文本以恰好一个换行
收尾。
其他都不碰：一个右对齐的时间戳保留它*前面*的空格，一个代码块保留它的缩进，一个
表格保留它的边框。OSC 8 链接载体和图片标记只骑在 `Style` 上，随它一起消失
（`docs/ui/links.md`）。

## 文件（`app::export_file_name`）

`conversation-YYYY-MM-DD-HHMMSS.txt` —— 日期用 ISO，时间是一串连续数字，每个字段
都零填充，好让这些名字按写下的顺序排序 —— 放在 session 所运行的**工作目录**里
（页脚里的 cwd，也就是权限规则所用的项目键）。时间戳用的是**本地**时钟，
rollout 自己的规则（`session::rollout_rel_path`）；纯函数把日期和时间当作值来
收，
由边界读 `chrono::Local::now()`（`tui::export`），也就是 `set_clock` 模式。

文件用 `create_new` 创建，所以一次导出**绝不覆盖**已经在那里存在的文件：一秒之内
的两次导出 —— 一次按两下回车 —— 会先拿到朴素的名字，然后
`conversation-…-2.txt`、`-3`、……（函数的 `dup` 参数：`0` 是朴素名字，`1` 是同
一个名字以 `-2` 收尾），只有连每个名字都拒绝的目录才是错误。toast 点名实际写下的
文件 —— `Saved the conversation to conversation-2026-09-22-090720.txt` —— 所以用
户不必猜时钟读到了哪一秒就能找到它。

## 剪贴板

剪贴板那一行走的是 `/copy` 自己的路径（`clipboard::copy_to_clipboard`，
`docs/ui/copy.md`）：arboard，对 headless、SSH 或 tmux session 有 OSC 52 兜底，
原生 lease 在整个 app 生命期内持有。只有确认文字不同 ——
`Copied the conversation to clipboard`，措辞对应实际被复制的东西，与 `/donate`
页面那句 `Copied the BTC address to clipboard` 是兄弟 —— 或者一句红色的
`Copy failed: {reason}`。一次失败的文件写入是红色的
`Export failed: {reason}`。从不是 scrollback 里的一个 bullet
（`docs/ui/toast.md`）。

## API

- `app::ExportTarget`（`Clipboard` / `File`、`ALL`、`label`）—— 那两行；
  `app::ExportPicker` —— 打开页面的状态（被高亮的那一行）；
  `app::EXPORT_EMPTY_NOTICE`；`app::export_file_name(date, time, dup)`。
- `App::export_available`、`open_export_picker`、`close_export_picker`、
  `highlighted_export_target`、`on_key_export_picker`。
- `Action::OpenExportPicker` / `CloseExportPicker` / `Export(ExportTarget)`。
- `ui::export_view` —— `export_view_lines`（页面构建器；它的长度就是预留高度）、
  `export_picker_height`、`render_export_picker`，以及 `export_text`（在
  `ui::transcript_lines` / `ui::agent_transcript_lines` 之上的纯文本渲染，
  也就是分页器自己的构建器）。
- `tui::export::Session::export_conversation` —— 写剪贴板或写文件，以及 toast。

## 测试

- `app/tests/export.rs` —— palette 那一行（紧跟在 `/copy` 之后，描述在那儿）、
  `/export` 命令打开页面（空闲时、turn 中、在 agent 视图里），以及用 toast 拒绝
  空对话；打开时被放弃的快捷带；整套按键语法（↑/↓ 环绕、Home/End、Enter 和一个
  数字既选中**又关闭**、超出行数的数字被忽略、Esc/Ctrl+C 关闭、拥有每个键、
  Ctrl+B 提示的时钟被抑制、没有动画），以及文件名（形状、零填充、`-2`/`-3` 后
  缀）。
- `ui/tests/export_view.rs` —— 带框的页面（分隔线、标题、简介、各个行、跟随高亮
  的描述以及它点名文件的形状和目录 —— 或者在 session 信息注入之前的那套兜底说法
  —— 还有提示行），每一行被整体读作它的标签，选中点亮它那一行，宽度安全，无叠加
  空行的规则，高度契约，流入 scrollback 的资格，隐藏的光标坐在 `❯` 上；以及导出
  文本 —— transcript 的行逐字、按序，以启动横幅开头（先是标题行，然后 cwd、提示
  行、一个空行，再是对话）、没有行尾空白、一个收尾换行、turn 中的实时尾部、按
  要求的宽度换行，以及在某个 agent 的 session 视图里，被查看 agent 的（以它自己
  的横幅开头的）transcript。
- `scripts/smoke.sh` Phase 122 —— 在一个真实终端里、一个一次性 cwd 中端到端跑
  这个页面：空对话上的 `/export` 弹出 toast 且什么也不写；一条落定的回复之后页面
  打开，两行和描述都在，↓ 移动 `❯` 并换掉描述，Enter 把
  `conversation-YYYY-MM-DD-HHMMSS.txt` 写进 cwd（toast 点名它，文件以横幅的标题
  行开头，并在消息和回复之上带着它的提示行，没有行尾空白，一个收尾换行），然后
  composer 回来；第二次 `/export` 在剪贴板那一行按 Enter，同样的文本通过 OSC 52
  兜底落进 tmux 的粘贴缓冲；Esc 关闭，什么也不写。
