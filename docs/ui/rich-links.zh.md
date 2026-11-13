# 富链接 —— 可点击的路径、预览卡片与屏幕归属

**状态：路径、点击展开、预览卡片（文本 + 封面）已实现。** assistant 散文与行内
代码会把形似文件路径的 token 链到两个 viewport 里的 `file://` URI
（[`links.md`](links.md) *File paths*）；拥有屏幕的那个模式另外加上点击展开、
composer 的点击定位光标，以及对独占一行的一个 URL 显示**预览卡片**：站点、标题、
描述，以及 —— 当页面有 `og:image` 且终端能画图时 —— 左侧一张封面缩略图（按需经
`zeroth::link_card` 取回，缓存在 `~/.zeroth/cache/link-cards.json` +
`cache/link-cards/` 下，`ZEROTH_LINK_CARDS=0` 可禁用）。B 那套只在 overlay 里
兜底的方案，以及编辑器交接，仍留待后续阶段
（[`full-screen.md`](full-screen.md)）。

## 需求

积压清单要三样当前链接支持给不出的东西：

1. 回复里一个链接的**预览卡片** —— 标题、站点、描述、封面；
2. **路径可点击** —— 散文里的文件路径、一个 `pwd`、一个 URL，一视同仁；
3. **单个事件可点击展开** —— 一个 tool 单元格或一条消息，而不是整份 Ctrl+O
   transcript。

URL 已经能经 OSC 8 点击：终端把一段被标记的连续文本变成一个链接，点它是终端自己
的动作，既不需要捕获鼠标，也不需要 app 参与。文件 tool 的**头部**以同样的方式
链到它的文件。缺的是散文与输出里的路径检测、卡片，以及任何需要 app 知道指针在
哪里的东西。

## 今天有什么

- **链接是载体，不是命中测试。** `links::find_urls` 在渲染器里标出 URL 片段，
  `links::linked(style, url)` 把一个 interned id 盖进 `Style::underline_color`，
  而每一条写单元格的路径（`term::draw_cells`）都给被标记的连续文本套上 `OSC 8`。
  可见文本与样式不变；点击归终端。overlay（`Ctrl+O`）走同一个发射器，所以链接在
  那里也能用。
- **文件路径只在渲染器拿到了策略的地方才被链接。**
  `ui::tool::tool_header_lines` 用 `PathDisplay::file_url` 链一个
  `● Read/Write/Edit({path})` 头部。assistant 散文没有 `PathDisplay`，所以写在
  回复里的一个路径（`src/app/tools.rs:657`）只是纯文本。tool 输出完全不被链接
  （[`links.md`](links.md) *Limitations*）。
- **对话是 inline 的。** viewport 是内容锚定的，每一行完成的行都被提交进终端的
  scrollback（不变量 3）。已提交的行无法重绘、重新度量，也无法归到某个鼠标坐标
  上：它们归终端所有。只有**live 区域**和**备用屏 overlay**（Ctrl+O、各全屏页）
  是我们的。
- **鼠标事件被忽略。** `Session::on_terminal_event` 处理按键、resize 与粘贴；
  鼠标捕获从不开启（`crossterm` 的 `EnableMouseCapture` 在代码里哪都不出现）。
  这是有意的：它让终端原生的选择、复制与链接悬停在对话之上继续可用。

## 参考实现：miyu 的全屏 TUI

本地的 Miyu 树把上一版终端头留在 `proto/tui-demo`（`tui-demo/src/…`），本页正是
借它的模型。它是一个**全屏** app，而正是这一点让它那些交互成为可能：

- **归属。** 备用屏、raw 模式、`EnableMouseCapture`、括号粘贴、焦点上报、kitty
  键盘增强。它自己滚动、自己选择、自己复制；`Ctrl+Z` 还原终端、挂起，在 `fg` 时
  重新进入；打开 `$EDITOR` 时让出终端、跑编辑器、再重新进入，画面留在原处。
- **片段自带它的目标。** Markdown 解析产出
  `Piece { text, style, link: Option<String> }`；`links.rs` 找出裸的
  `http(s)://` 与 `file://` URL，而一行 `title (url)` 会把整个标题重新链上。
  渲染出的一行还带 `links: Vec<(from_col, to_col, url)>` 和
  `target: Option<Target>`（这一行所属的可展开条目）。
- **命中测试归主体。** `BodyView::mouse` 把一个屏幕点映射到一行：`Moved` 设置
  `hover` / `hover_link`（被悬停的链接加下划线），在链接上左键 `Up` 返回
  `BodyAction::Open(url)`，在条目上 `Up` 返回 `Toggle(target)` 并锚定该行，于是
  展开向下生长而不会移动被点击的那一行，拖拽选择并复制，滚轮滚动。
- **卡片。** 一行里所有非空白单元格都属于同一个链接时，它变成一张**链接卡片**：
  左侧封面图（至多三分之一宽，高 `link_cover_rows`），然后是一个图标和链接装束
  的标题、暗色的站点与作者、暗色的描述，以及带时长的视频页脚。每一个被绘制的
  单元格都是那个链接。数据来自核心（`link.preview`）：OpenGraph/meta 的标题、
  站点、描述、封面与图标；图片是按内容寻址的 blob，以 512 KiB 分块读回并缓存到
  磁盘，所以卡片能活过一次重启。取回是按 URL、按需进行的。
- **路径。** Miyu **不**自动检测散文里的裸路径。Markdown 链接目标与 `file://`
  URL 走 `local::resolve`（剥掉 `file://`、展开 `~`、相对路径按 cwd 解析）；点一
  个本地**文本**文件会打开终端编辑器，其他一切交给系统打开器（`open` /
  `xdg-open` / `start`）。裸 URL 是它唯一会链接的文本。

结论是：悬停卡片与点击展开不是链接解析的特性；它们是拥有屏幕的后果。文本里的一个
模式很便宜；绘制时刻的一张单元格到目标的映射表，才是需要屏幕的那个东西。

## 设计空间

| | Inline，终端原生 | overlay 范围的鼠标 | 可选开启的全屏 |
|---|---|---|---|
| 可点击的 URL | 已有 | 已有 | 已有 |
| 散文里的可点击路径 | 有，OSC 8 | 有，OSC 8 | 有 |
| 预览卡片 | 无 | 在 app 拥有的页面里 | inline 在回复里 |
| 悬停下划线 | 无 | 只在页面里 | 到处 |
| 点击单个事件展开 | 无 | 在 transcript 页里 | 到处 |
| 原生文本选择 | 不受影响 | 页面之外不受影响 | 丢失；必须重新实现 |
| 屏幕归属 | 无 | 一个页面打开期间 | 始终 |
| 符合 `CLAUDE.md` 不变量 3 | 是 | 是（overlay 本就存在） | 刻意替换它 |
| 屏幕归属 | 无 | 一个页面打开期间 | 始终 |

**建议：先 A，然后 C。B 是可选的过渡。** 所有者已经决定拥有屏幕是有意义的 ——
没有它，这之后每一个可供性都得委托给终端 —— 所以 C 是目标，不是应急。A 仍是正确
的第一片（它在渲染器层，两种模式里都能落地）；B 的命中图与鼠标分发正是 C 需要的
同一份工作，所以只有 C 拖得足够久、想要一个临时替代时，B 才值得做。C 有自己的
页面：[`full-screen.md`](full-screen.md)。

## A. inline 视图里的可点击路径

### 检测

在 `find_urls` 旁边新增一个纯检测器：

```rust
/// Byte ranges of path-like tokens in one prose line.
pub fn find_paths(text: &str) -> Vec<Range<usize>>;
```

一个 token 是路径，当且仅当以下全部成立：

- 它是按空白切分的一个词，且尚未属于某个 URL；
- 它以 `/`、`./`、`../` 或 `~/` 开头，**或者**它含 `/` 且它的最后一段含一个 `.`
  （`src/app/tools.rs`、`docs/ui/links.md`）；
- 它至少有一个非空段，且没有 scheme（`://`）；
- flag（`--path=…`）、glob（`src/*.rs`）以及最后一段没有点的 `word/word` 散文
  保持原样。

尾部标点按 URL 规则修剪（`.,;:!?'"` 与不配对的收尾符）。可选的 `:line[:col]`
后缀从目标里剥掉、留在可见文本里；它是否变成一个片段（`#L657`）是下面的开放
问题。

### 目标

目标是 `PathDisplay::file_url` —— 与 tool 头部已经链接的同一个绝对
`file://` URI，百分号编码，`~` 与相对路径按 session 的 cwd 解析。没有 cwd
（逐字策略、一个单元测试）就意味着不链接，这正是头部现有的规则。`file://` 是
唯一被新增的 scheme；一个路径绝不会变成 `http://`，而 `javascript:`/`mailto:`
文本不是路径。

### 接在哪里

- **assistant 散文。** `ui::inline::inline_spans` 是唯一的漏斗；路径片段与 URL
  片段完全一样地拿到链接装束和载体。渲染器需要 session 的 `PathDisplay`，而它
  今天没有：把 `Option<&PathDisplay>` 穿进 `ui::assistant::AssistantRenderer` 与
  `ui::message::message_lines`（公开的 `message_lines` 保持签名并传 `None`，于是
  测试与 API 面锁定都稳定；`message_lines_with_paths` 是 TUI 的入口）。
- **tool 输出。** `ui::tool` 本来就有 `PathDisplay`；在 peek/展开的输出行里也
  标出路径（`ls`、编译器错误）。输出的字节与列不变 —— 变的只有载体和被标记片段
  上的链接装束。
- **用户/shell 文本**保持字面，与 URL 一样。
- **tool 头部**不变；它们本就链接，而角上的行保持朴素。

### 前缀稳定性

回复流式输出时，正在生成的那一行还会长大：`src/ap` 可以变成 `src/app.rs`。URL 那
套机制已经用 `links::has_forming_url` 回答了这一点 —— 当一个目标仍可能改变时，
流提交会扣住正在长的那一行。路径检测需要同一个谓词（`has_forming_path`）：一个
贴着行尾的、或以分隔符结尾的形似路径的 token，会扣住这一行。于是已提交的行绝不
改变它的目标。被空白或标点封住的路径像被封住的 URL 一样逐段提交。

### 测试与 smoke

- `links.rs`：检测器的正例与反例、尾部标点、`:line` 的处理、百分号编码、
  无 cwd、无 scheme。
- `ui/tests/inline.rs`：散文、代码、标题、列表、引用与表格里一个路径的每一种换行
  之下的载体；可见文本逐字节相同。
- `ui/tests/tool.rs`：peek 与展开输出行同样如此。
- Smoke：给 dummy 回复加一个 `src/app.rs:42`；一个原始流阶段（Phase 87 的形状）
  断言一个 `OSC 8` 开启，携带解析后的 `file://…/src/app.rs`，而窗格显示的文本
  不变。

### 限制

- 不检查存在性：检测发生在渲染器里，它是纯的，绝不能碰文件系统。一个被链接却
  不存在的路径，是用户终端的问题（它会说出来），而一个裸的 `README` 保持朴素。
- 没有 scheme 的 `www.…` 与裸文件名保持朴素，与 URL 一样。
- OSC 8 仍然是终端能力；既有的 `ZEROTH_HYPERLINKS` 闸门同时覆盖两类。

## B. overlay 范围的鼠标与 `/links` 页面

备用屏 overlay 已经存在（`term::enter_overlay` / `draw_overlay` /
`exit_overlay`），已经通过链接发射器拿到它们的单元格，也已经有一个可滚动的行
模型（`ui::transcript::TranscriptCache`）。这个设计让**页面**在打开期间对鼠标
有感知，而对话保持原样。

### 鼠标捕获的生命周期

`EnableMouseCapture` / `DisableMouseCapture` 由 `term::enter_overlay` /
`exit_overlay` 写，就在 `EnterAlternateScreen` / `LeaveAlternateScreen` 旁边，
并且写在每一条已经在发 `LeaveAlternateScreen` 的还原/panic 路径上。于是捕获只在
一个 app 拥有的页面在屏上时才开；对话保住原生选择、复制与终端链接悬停。本来就
在代码里的 `Ctrl+O`/页面退出（`exit_overlay`、`overlay_return_repaint`）是唯一
可能弄错的地方，所以它也是唯一该测的地方。

### 命中图

transcript 缓存本来就构建一个 `Vec<Line>`，并知道每个历史条目占哪段行范围。在
overlay 打开期间，另建一份平行的 `Vec<RowHit>`（关闭时丢弃，绝不在 inline 路径
里构建）：

```rust
struct RowHit {
    /// The expandable thing this row belongs to, if any.
    target: Option<usize>,          // history item index
    /// Byte/column ranges whose cells carry a link carrier.
    links: Vec<(u16, u16, String)>, // decoded from the spans' carriers
}
```

链接来自行里各 span 样式上已有的载体（由编码它们的同一个 `links` 模块解码），
所以 transcript 里画出的一个链接可点击，且没有第二个事实来源。目标来自缓存已经
在跟的条目范围；一个 tool 单元格的一行和它输出的一行都映射到同一个条目。

### 交互

- `Moved` → 设置 `hover: Option<(row, link)>`，把被悬停的链接片段重绘为带下划线
  （overlay 反正每帧都重画；下划线只是一次样式补丁，不改文本）。
- 在链接上左键 `Up` → `open_link(url)`；这就是 A 的同一套打开器策略（OSC 8 让
  终端去做；这里我们自己做，因为鼠标归我们）：`http(s)` 走系统打开器，`file://`
  走操作系统（编辑器交接是另一个问题），错误显示为一行提示。
- 在有目标的行上左键 `Up` → 切换该条目的展开集合；保住被点击那一行的屏幕位置
  （overlay 本来就存了一个滚动偏移，所以锚定 `offset + row`，让新增的行向下生
  长）。
- 滚轮 → 滚动页面，与键盘已经能做的一样。
- 拖拽 → **捕获开着时原生选择就没了**。B1 阶段不碰选择，并记录 Shift+拖拽（多数
  终端用 Shift 绕过捕获）；B2 若确有需要，就在行的纯文本上实现拖拽选择加复制
  （Miyu 的 `selected_text` 是参考），用 `Ctrl+C`/`c`，与 UI 其余部分一致。

`Ctrl+O` 保持它的键盘行为不变；鼠标是一层附加。

### `/links` 页面与卡片

一个新的 overlay 页面（`/links`，或 `Ctrl+O` 的兄弟页）列出**当前对话里的目标**
—— A 的检测器找到的 URL 与路径，加上文件 tool 的头部 —— 去重，最新的回复在前。
它是卡片天然的家：

- 选中一个条目按 `Enter` 打开它；按 `p`（或在预览列上按 `Enter`）取回预览；按
  `r` 重试一个失败的。
- 取回的卡片按参考实现渲染：左侧封面图（至多三分之一宽，固定行高），链接装束的
  图标加标题、暗色的站点 · 作者、暗色的描述、页脚里的视频时长。没有封面或终端
  不能显示图片时就是纯文本。既有的 `images::fitted` / `ImageStore::stamp` 流水线
  本来就在 `draw_overlay` 里跑，所以一张封面与一张粘贴的图片是同一条图片路径。
- 因为页面拥有屏幕，一张迟到的卡片可以改变页面的高度；页面的滚动偏移被调整，
  以把选中的那一行留在视野里。这正是 inline 提交做不到的事。

### 取回策略

预览一个 URL 是代表用户发出的网络请求，所以它**显式触发，绝不预取**：

- 只限 `http(s)`；`file://`、其他 scheme，以及不是 URL 的文本永不被取回。URL
  本身就是不可信的模型内容。
- 从 worker 线程发（剪贴板/git-diff/checkpoint 那套模式），不是从事件循环：
  `GET`、5 s 超时、1 MiB 主体上限、≤ 3 次重定向、无 cookie、无 auth 头、一个
  通用 user agent。只收 `text/html`。
- 解析 `<title>`、`og:title`、`og:description`、`og:site_name`、`og:image`、
  `og:type`、`og:video:duration`；回退到 `description`。什么都没有的页面是一张
  纯文本卡片，而不是失败。
- 拒绝私有/回环/链路本地地址（模型可以写出任何 URL；一次预览绝不能变成对用户
  LAN 的探测）。重定向按同样的方式重新检查。
- 缓存在 `~/.zeroth/cache/link-cards/` 下：一个按 URL（哈希）索引的 JSON 索引
  加若干按内容寻址的图片 blob，带一个体积预算和一个年龄上限（既有的图片缓存剪枝
  模式）。卡片能活过重启；一次失败的取回被记住一段短 cooldown，而不是永远。
- 取回失败时给一条可见的一行提示，以及一个 `NO_PREVIEW`/设置开关，供想让这个
  特性关掉的用户使用 entirely。

### 测试与 smoke

- 一个纯的命中图测试：一段含一条消息、一个 tool 单元格和一个链接的 transcript；
  在边界处、换行之后、展开之后做行 → 目标/链接的查找。
- 一个 app/overlay 的点击分发测试：在某一行上一个合成的 `MouseEvent` 恰好执行
  一次 `Open`/`Toggle`，且不触发任何键盘动作。
- 卡片解析与策略是对 fixture HTML 的单元测试（标题/OG/相对图片 URL、非 HTML、
  超限主体、重定向到私有地址）。
- Smoke：进入 Ctrl+O 在原始流上写出 `\x1b[?1000h`（退出写出 `\x1b[?1000l`）；
  一个在已知行上的 `tmux send-keys -M` 点击打开预期的目标（一个替身打开器记录
  它的 argv，Phase 113 的模式）。如果 tmux 这条路被证明不稳定，pty harness 直接
  写入 SGR 鼠标序列。

## C. 可选开启的全屏（目标）

全屏模式有它自己的设计页：[`full-screen.md`](full-screen.md)。简言之：一个拥有
屏幕的 viewport 套在同一个 `App` 上，`ui::transcript` 作为框的 builder，它自己的
滚动模型、鼠标捕获与一张每帧的命中图、选择/复制，以及备用屏生命周期。屏幕归自己
之后，卡片可以 inline 画在回复里，一次点击可以切换任何条目；悬停下划线到处都能
用。它是目标，因为它移除了未来每一种交互对终端的「手心向上」依赖，并且用一份属于
自己的滚动契约替换不变量 3。它先以可选开启的模式落地，只在达到对等之后才成为
默认。

## 分期

1. **A —— 路径 —— 已完成。** 检测器（`find_paths`）、经 `PathDisplay` 的
   `file://` 目标、散文/代码标记，以及 `has_forming_path` 的流扣留；单元测试在
   `links.rs` 与 inline 渲染器里。没有鼠标，没有新状态。它在两种 viewport 模式里
   都落地了。
2. **C —— 拥有屏幕的模式 —— 骨架、鼠标、滚动与选择已完成。** 见
   [`full-screen.md`](full-screen.md)。那一页对本页仍未完成的：点击展开与预览
   卡片。
3. **全屏模式里的富链接 —— 卡片已完成（文本 + 封面）。** assistant 回复里独占
   一行的 URL 变成 `● site — title` 加一行暗色描述，并在取到 `og:image` 且终端
   能画图时加一张封面缩略图（≤ 1/3 宽、≤ 6 行、保持宽高比）；图片存储在绘制
   时刻从卡片缓存载入它。取回是每个 URL 一个 worker 线程（最多八个在飞），页面
   5 s/512 KiB/`text/html`，封面 2 MiB/`image/*`；主机在**解析之后**被拒绝
   （`host_is_private`：字符串规则加上该名字答出的每一个地址，于是 `nip.io`、
   IPv6 映射形式与十六进制写法都被抓住），重定向按同样的方式重新检查。卡片跨
   重启缓存（封面在 32 MiB、最旧先出的预算之下）；`/links` 页面仍然保留。
4. **B —— 只在需要时。** overlay 范围的鼠标与一个 `/links` 页面仍是 inline 模式
   下有用的临时替代，它们的命中图是 C 的子集，但它们不是目标。

## 开放问题

- **到底要不要取回。** 一个预览值得发出任何对外请求吗？默认关闭、只在显式按键时
  取回，是建议的答案；一个从不取回的构建保留一个纯文本页面。
- **`path:line`。** 只链到文件，还是链到 `#L657`？片段被某些编辑器尊重、被另一些
  忽略；只到文件是安全的默认值，而「在那一行打开」是 app 层打开器（编辑器交接）
  的功能。
- **编辑器交接。** Miyu 通过让出终端在 `$EDITOR` 里打开本地文本文件。zeroth 今天
  没有编辑器交接；打开系统默认程序是更小的第一步。
- **页面里的选择。** 自己做选择/复制，还是 Shift+拖拽了事？

## 非目标

- 重写已提交的 scrollback。它做不到，这里没有任何设计假装可以。
- 展开回复里的每一个 URL。卡片只在 app 拥有屏幕的地方画，且只在用户要求之后画。
- 在渲染时刻取回 URL。渲染器是纯的，而一次预览是一个显式动作。
