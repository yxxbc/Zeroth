# 内联图片

粘贴进来的截图，以及 `read` tool 读到的图片，在对话里被画成**真正的图像** —— 在
终端自己的 scrollback 里、在 live 区域里、在 Ctrl+O 的 transcript 里 —— 而不是只
用一行事实描述。终端讲得上某种图形协议（kitty、iTerm2、sixel）时，那些是真正的像
素；其余情况下是 unicode 半块，也就是普通的带色单元格，因此每个终端都成立，
scrollback 与 resize 也包括在内。

```
● Read(/home/me/shots/cat.png)
  ⎿  Read image (JPEG, 700x689, 63 KB)

<the picture, flush at the left margin>

● Downloaded ✅ — a kitten asleep on a MacBook charger.
```

上方一个空行、下方一个空行，块从**第 0 列**开始 —— 它刻意*不*缩进到单元格的 `⎿`
缩进槽里，因为一张图片应当拿到终端拥有的整个宽度。

## 形状

五个模块分属两层 —— 纯的与边界的 —— 按 crate 一贯的切法切开（`src/images/`），再
加上 `ui/image.rs` 管行本身：

| 模块 | 内容 |
| --- | --- |
| `images::geometry` | 纯的。一张图片占用的单元格足迹，以及标记为它预留的那些行的逐单元格载体。 |
| `images::registry` | 进程级的渲染策略（`/settings` 加上终端最终支持的东西）、placement 驻留表，以及边界在每个图片路径上最后一次发现的东西。 |
| `images::fitted` | 纯的。按它将被显示或发送的尺寸解码一个 PNG —— 逐行流过面积平均的缩小，所以整张图片从不被持有（`docs/architecture/memory.md`）。 |
| `images::payload` | 另一个边界：在图片被**上传**之前缩小它，并把结果为本 session 留在磁盘上，好让重新发送的附件从不被缩两次。 |
| `images::attachment` | 线上格式里的那份图片：它的 base64 `data:` URL，每个 session 只编码**一次** —— 从文件或 payload 的 sidecar 流式读出 —— 之后每个重新发送它的请求都按引用共享（`docs/architecture/memory.md`）。 |
| `images::store` | 绘制边界：终端能力、已编码的图片，以及把一块预留区变成一张图的那一趟。 |
| `ui::image` | 纯的。某个历史条目显示哪些图片，以及它们在它的单元格之下预留的带标记行。 |

正是这个切法让这个功能接起来很便宜。`ui` **预留行**；边界**往这些行里画**。一块
预留区是 `rows` 个普通的 `Line`，每个由 `cols` 个空格组成，每个单元格在它的
`underline_color` 里带一个标记 —— 于是一张图片走过 crate 已有的每一条路径（一次
scrollback 提交就是一个 `Vec<Line>`，从来都是），而边界的 `ImageStore::stamp` 在
每一处绘制经过的四个位置把标记变成图片。

### 载体

`links` 早就把 `Style::underline_color` 当作逐单元格的载体 —— 那是唯一能在
`Span` → `Cell` → 每条绘制路径里活下来的通道。图片骑在同一段 24 位空间里，是切分
而不是共用：**第 23 位置位**表示这是一个图片标记
（`images::geometry::IMAGE_CARRIER_FLAG`），`links::LINK_ID_MAX` 也相应降到
`0x7F_FFFF`，于是一个 URL 绝不会被解码成图片，反过来也一样。在标志位之下，一个标
记打包了 `(placement id, row index)` —— 15 位加 8 位 —— 而正是行索引让 stamp 不
必扫描就能找到一块的左上角。

`term::draw_cells` 在单元格发出去之前剥掉链接载体；图片 stamp 出于同样的理由清掉
自己的标记 —— 载体绝不能真的被画成下划线颜色。

### placement 驻留表

`images::place(path, px, avail_cols)` 把 `(path, cols, rows)` 驻留成一个小 id，
因为单元格里放得下的只有一个 id。既按路径、又按**尺寸**驻留，正是 resize 正确的
原因：更窄的终端产出一个不同的 placement id，于是边界编码一张新图片，而不是把旧
的那张按错误的尺寸重新摆放。

### 原地被重写的文件

键是路径与尺寸 —— 它**不是**的东西同样重要：文件的字节。agent 下载一张图片、显示
它、原地把它转成黑白、再读一次，而新的单元格预留了*同一个* placement —— 同样的路
径、同样的像素尺寸 —— 而 store 里以该 placement 为键的缓存仍然握着它从彩色文件编
码出来的那张图。拿到那条缓存，新单元格显示的是彩色的猫。这就是被报告的那个 bug，
而它唯一反常的细节由下面的*屏幕切换*解释：Ctrl+O 显示的是转换后的图，因为 kitty
按屏幕各存一份，而备用屏那时还没有 —— 任何在两个屏幕之间共用一个条目的协议，在那
里也会显示陈旧的图，这正是 Phase 107d 在半块下驱动的东西。

所以一次编码会记住它取材时的**文件状态** —— 大小与 mtime，也就是 payload 的
sidecar 与线上附件早已用作键的那个 `(len, mtime)`（*内存*）—— 而命中只在文件仍然
是那个状态时才算命中：`ImageStore::encode` 在每次绘制一块时 `stat` 一次路径，不
匹配就丢掉条目、重新编码，对 kitty 而言这是在新 image id 下的一次新上传，于是已
经躺在 scrollback 里的占位符仍然指着它们被绘制时的那个 image。这项检查是每次绘
制、每个可见块一次 `stat` —— 微秒级，对比它所替代的解码与上传 —— 而且它读的是文
件的状态，绝不是它的字节。此后 **消失**的文件不是一个变了的文件：已经编码好的图
片继续绘制，也不会每帧去重试。单元测试在没有终端的任何地方把一个半块 store 驱动
进一个 `Buffer` （`ImageStore::with_protocol` —— 红的，在一个不同的 mtime 上被改
写成蓝的，画出来是蓝的；被删掉，仍然画）；`smoke.sh` Phase 107d 对着真实二进制做
同样的事。但一个消失的文件只画到下一次 purge 重建为止，那次重建会随 scrollback
一起丢掉每一张已编码的图片（下面的*文件已消失的图片*）。

### 文件已消失的图片

行预留从不打开文件：一次图片读取的行来自它自己的事实行（下面的*像素尺寸从哪
来*），而续上的对话在文件早已消失之后很久仍然保留着那一行。agent 把截图存到
`/tmp` 下并读它，机器重启、`/tmp` 随之而去，而续上这个 session 时照样预留了图片
的那些行 —— `Read image` 单元格与回复之间一整屏的空行，里面已经没有东西可画。这
就是被报告的那个 bug。

所以边界在按历史铺开一段对话之前会再看一眼：`tui::view::recheck_pictures` 读一遍
历史所显示的每张图片的头（`ui::item_image_paths` 列出这些文件 —— 一次图片读取的
路径，加上每个 Ctrl+V 附件），把答案交给纯的决定 `images::recheck_pictures`。找
到的记下尺寸（`remember_size`）；找不到的标为缺失（`remember_missing`），而
`images::place` —— 每一次预留都要经过的那一个调用 —— 对缺失的路径不预留任何行。
上面那个单元格于是独自立着，与关掉图片时完全一样。

它在任何把内联对话整段按历史铺开的地方运行 —— 两处：

* **一次 `--resume`/`--continue` 加载**，它在启动时提交 transcript，不做重建
  （`apply_startup`）；
* **每一次 purge 重建** —— 一次 `/resume` 加载、一次 resize、`/settings` 的图片
  行、一次回溯、agent session 视图自己的重建。purge 会随 scrollback 丢掉每一张已
  编码的图片（*Resize*），所以一次 purge 之后，文件已消失的图片再也画不出来。在
  那里检查的代价是每张图片一次头读取，对比一次无论如何都要重绘整段历史的重建。

缺失的标记并非永久。之后任何一次看见都会清掉它：一次重新找到文件的复查（agent 又
截了一次图；一次回滚点恢复把它放了回来）、一个落地的 Ctrl+V 粘贴，以及 `read` 执
行器自身 —— 它在后端线程上、在自己的单元格被提交之前记下它读到的每张图片 —— 所以
一个发现 `/tmp/shot.png` 已消失的续上对话，仍然能把随后一次对新的
`/tmp/shot.png` 的 `read` 画出来。

它够不到的，是已经画出去的行。一张已经画出、其文件在会话中途被删掉的图片，会从它
的编码继续绘制，直到下一次 purge 重建 —— 也就是它的行消失的那一刻 —— 而 Ctrl+O
的 transcript 会保留它的缓存构建出的行，直到那份缓存下次重建（宽度改变、历史回
退）。单元测试用一个假探针驱动 registry、用标记为缺失的路径驱动行构建
器；`smoke.sh` Phase 107f 续上一个 rollout，其中一次 read 点名的文件并不存在 ——
经 `--resume`，再经 `/resume` picker —— 然后把文件写回去、再删一次，中间跨两次
resize。

### 像素尺寸从哪来

行预留是纯的，所以它打不开文件。两个来源，而且只有两个：

* **一次图片读取** —— `read` tool 自己的事实行已经带着它了（`Read image (JPEG,
  700x689, 63 KB)`），所以 `images::read_image_size` 把它解析回来。那也是在
  `/resume` 之后唯一活下来的记录：rollout 保留的是单元格的文字，不是文件的头。
* **一个 Ctrl+V 粘贴** —— 没有那样的行，所以边界在粘贴落地时读一次头
  （`images::remember_size`，来自 `tui::workers`）—— 并且为一个加载进来的对话所
  带的每个粘贴图片再读一次（`tui::view::recheck_pictures`，在一次
  `/resume`、`--continue` 或 `--resume` 加载被铺开之前），因为粘贴会被存在
  config home 下
  （`{config_home}/image-cache/{session}/N.png`，`docs/input/image-paste.md`）而
  且通常还在那里。头读不到的粘贴 —— 被谁删掉的图片 —— 干脆不画。

两种方式下尺寸都只是它的一半：只有文件还在、还有东西可画时，图片才预留行（上面
的*文件已消失的图片*）。

## 几何

`images::image_budget` 然后 `images::fit_cells`，两者都是纯的：

```
usable    = terminal width − IMAGE_GUTTER_COLS (2)
max_cols  = min(/settings Image width, usable)
max_rows  = ceil(max_cols × cell_w / cell_h)        ← the square pixel box
(cols, rows) = Fit(image px) into (max_cols, max_rows)
```

有两点值得说清。

**行数上限是被当成正方形像素框的宽度上限。** `max_cols` 列就是横向 `max_cols ×
cell_w` 个像素；同样数目的像素*纵向*就是 `max_rows` 行。没有它，一张 600×4000 的
竖屏截图会把整个宽度预算花在它的宽度上，然后再花四百行去匹配。有了它，一张高图被
它的高度限住，结果是窄的 —— 这正是参考实现针对这个 bug 的修法。

**`Image width` 是上限，不是目标。** 拟合用的是 `ratatui_image` 的
`Resize::Fit`：按比例，且**只缩不放**。一个 32×32 的图标仍然是几个单元格，而不是
被放大到填满 120 列 —— 在那里终端的重采样只会让它更糊。

`fit_cells` 刻意复现 `ratatui_image` 自己的算术，而不是另发明一套，因为预留与编
码器必须精确到单元格地一致 —— 差一行，就是每张图片下面的一条空
白。`images::tests::fit_cells_reproduces_the_encoders_own_arithmetic` 是一份对着
真正的 `Resize::Fit`、跨字体、图片尺寸与预算矩阵的差分测试。

## 探测终端

`images::Detection` 只决定一次，在 `InlineViewport::init` 里，先看环境、再看终端
—— 而且它从不新增一个 stdin 读取者。

`ratatui_image` 带了一个 `Picker::from_query_stdio`，直接去问终端，但这里用不
了。它在 stdin 上起一个读取线程、背后是两秒超时，而且*从不 join 它*，于是在一个
不回话的终端上，那个线程活得比查询更久，把用户的按键吃掉。做这件事的时候在 tmux
下实测过：启动卡两秒，然后每一个键都被吞。那是 CLAUDE.md 的不变量 1 —— 一个
stdin 读取者 —— 没有商量余地。

所以，像参考实现一样，先由环境决定：

* **单元格尺寸**来自 tty 自己的 `ws_xpixel`/`ws_ypixel`（经 `rustix` 的
  `TIOCGWINSZ`），那是一个 ioctl，不是一次来回。零表示「未知」，回退到 1:2 的单
  元格 —— 只有*比例*要紧，因为正是它决定一张图片的行数；
* **协议**来自 `ratatui_image` 自己的环境嗅探（iTerm2 家族，加上某个复用器的外层
  终端）再加上 kitty/ghostty —— 它们会在 `TERM` / `TERM_PROGRAM` /
  `KITTY_WINDOW_ID` 里自我声明 —— 而*不*在复用器之下（`TMUX`、`STY`、`ZELLIJ`，
  或一个 `tmux`/`screen` 的 `TERM`），那里恰恰是那个答案会过期的地方。

### 去问终端

环境说不出协议的地方，它就无从下手 —— 而这并不只发生在未知终端上。**herdr** 给每
个窗格 `TERM=xterm-256color` 和 `TERM_PROGRAM=herdr`，并剥掉外层终端的变量（其中
就有 `KITTY_WINDOW_ID`），而它自己的模拟器 —— libghostty —— 讲的是 kitty 图形协
议，unicode 占位符也包含在内。只靠环境探测，把那里的每张图片都画成了半块。

于是恰恰在这种情况下要去问终端，而且是在一次本来就存在的读里：`init` 在循环的
`EventStream` 存在之前，同步地、经 stdin 问一次光标位置。那次读就是任何别的问题
必须去的地方（`term::query`），而另外两个问题搭在同一次写入里，排在光标查询**之
前**：

```
ESC[22;0t  ESC_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA ESC\  ESC[23;0t  ESC[>q  ESC[6n
push title  the kitty graphics support query        pop title  XTVERSION  cursor
```

终端按顺序回答，所以光标报告永远是最后一个答案，读取在它的最后一个字节上停下。不
认识某个问题的终端什么也不说，那也就是它的答案，而且不花等待时间。herdr 0.9.3，
从一个真实窗格录下来的答案是：`ESC_Gi=31;OK ESC\`，然后 `ESCP>|libghostty
ESC\`，然后光标报告。

* **什么时候问**（`images::graphics_query_wanted`）：只在环境停在半块回退、且没
  有东西钉住协议时 —— 它点了名的时候从不，设了 `ZEROTH_IMAGE_PROTOCOL` 的时候从
  不（`halfblocks` 也算，那正是关掉这个问题的办法），`ZEROTH_IMAGES=0` 时从不，
  在复用器之下从不（它自己什么也不回答），在 `linux` 或 `dumb` 控制台上也从不 ——
  那会把它不认识的转义串打印出来。
* **标题的 push/pop** 是给 tmux 的。它会把一条它不认识的 APC 串当作窗格标题存起
  来，于是一个从环境里*藏起* tmux 的 session（不带 `-e TERM` 的 `docker exec`、
  从一个 `default-terminal` 为 `xterm-256color` 的 tmux 里发起的 `ssh`）会永远顶
  着 `Gi=31,s=1,…` 当标题。标题栈把它放回去；没有标题栈的终端两个都忽略。
* **一个答案意味着什么**（`images::protocol_after_reply`）：一个 kitty 的 `OK`
  把半块回退升级成 kitty，别的一概不动。一个错误答案，或沉默，就留在半块。
* **谁回答的**是唯一需要判断的一处。按 `ratatui_image` 的说法，WezTerm 与
  Konsole 对查询回答 `OK`，却不实现 unicode 占位符 —— 那是这个 crate 唯一发出的
  kitty placement —— 所以它自己的查询靠 `WEZTERM_EXECUTABLE` / `KONSOLE_VERSION`
  变量把两者都拉黑。但 herdr 的窗格里保留着那两个变量，而真正在回答的是它*自己
  的*模拟器，所以这里的黑名单从 XTVERSION 读**回答者**的名字（`WezTerm
  20240203-…`、`libghostty`），只在终端没有给出名字时才回退到变量。

这次读会把光标报告之前到达的一切都收进来，其中包括它等待时敲下的键 —— TUI 起来之
前打出的提前输入。这些被逐字节地放在一边，在循环的第一个 `select!` 之前经普通的
按键处理器重放（`term::query::typed_events`，crossterm 的旧式映射：文字、控制
键、Esc 与 Alt；像方向键这样的转义*序列*会被丢掉，而不是当作杂散字符重放）。光标
报告之后的东西在那里一个都不读，所以之后敲的一切完整地到达 `EventStream`。这次读
给每个 `read()` 十分之一秒（VMIN 0、VTIME 1 —— POSIX 自己的读超时，在一个 `poll`
不生效的 macOS tty 上也成立），并在两秒之后放弃 —— crossterm 旧的预算 —— 报的还
是那个 "cursor position could not be read" 错误。

仍然探测不出的，落到**半块**上。`ZEROTH_IMAGE_PROTOCOL`（`kitty` / `iterm2` /
`sixel` / `halfblocks`）与 `ZEROTH_IMAGE_CELL_SIZE`（`9x18`）覆盖二
者；`ZEROTH_IMAGES=0` 关掉整个功能。`smoke.sh` Phase 107e 自己扮演终端 —— 一个按
herdr、沉默终端或 WezTerm 的方式回答的 pty —— 因为 tmux 一点都回答不了。

### 在容器里

从环境探测有一个后果值得写下来：**环境必须是这个终端的环境。** 用 `docker exec`
起的进程看到的是镜像里通用的 `TERM=xterm-256color`，其余一概没有。会回答 kitty
查询的终端仍然能被找到 —— 问题与它的答案像任何别的字节一样穿过 `docker exec -it`
的 pty —— 但 iTerm2 家族只能由 `TERM_PROGRAM` 点名，而复用器规则需要 `TMUX`（或
tmux 设置的那个 `TERM`）。headless 的 Kali 容器所记录的启动命令按 session 转发本
节读的那些变量：

```sh
docker exec -it -e TERM -e COLORTERM -e TERM_PROGRAM -e KITTY_WINDOW_ID -e TMUX \
  zeroth-kali zeroth
```

裸的 `-e NAME` 从调用方自己的环境里取值。`TMUX` 在列表上是刻意的：正是它让上面那
条复用器规则在一个嵌套 session 里也依然诚实，于是 tmux 之下的 session 一概不问。
在问终端之前、于原始字节流上量过一次，续上一个带图片读取的 rollout：不带 flag
时，0 个 kitty 转义、35 个半块单元格；从 kitty 终端带上 flag，8 个转义承载 37 KB
的图片；在 tmux 里带上 flag，又是半块。其中第一种现在只对一个不回答 kitty 查询的
终端成立。

## 绘制

`ImageStore::stamp(buf)` 在**四个**位置运行，漏掉一个，那个视图里的 bug 就还活着
（`visible_cells` 规则）：

| 路径 | 画什么 |
| --- | --- |
| `write_above_chunk` | 一次 scrollback 提交 —— 图片被写进终端真实的 scrollback 一次，此后随文字一起滚动 |
| `paint_live` | live 区域 |
| `paint_reflow` | 一次 purge 重建的 live 区域（它的尾部走 `write_above`） |
| `draw_overlay` | Ctrl+O / Ctrl+D 的备用屏 —— 在 diff *之前* stamp，于是一个没变过的页面保持沉默（`docs/ui/overlay-repaint.md`） |

`ratatui_image` 的渲染方式是把整条图形转义塞进一个 `Cell` 的 symbol，并把被覆盖
的单元格标成 `CellDiffOption::Skip`（半块是普通单元格，一点都用不上这
些）。`Buffer::diff` 自己就尊重这两个标志，所以 diff 路径白拿 —— 但直接发射的那
条路径 `term::visible_cells` 得学会它们：

* 一个 `Skip` 单元格**绝不写**：那条转义已经画过那些列，在上面写一个空格会在图片
  里打出一个洞；
* 影子宽度算自 `Cell::cell_width()`，绝不是 `cell.symbol().cell_width()` —— 一个
  图片单元格的 symbol 是几百字节的转义、在屏幕上却恰好一列，而只有 `Cell` 的实现
  尊重那个说明这一点的 `ForcedWidth`。

`term::draw_cells` 还会在这一团东西**之后**结束它的单元格连跑。后端只会为那些与
前一个不相邻的单元格重新寻址，而一条转义团可以把光标留在任何地方 —— sixel 的摆放
会先把它的区域逐行清掉 —— 所以它之后的那个单元格开一次新的 `draw`，而那总是以一
次光标移动开头。

### 按拟合后的尺寸解码

`ImageStore::encode` 过去把文件整份解码，再把图片交给 `ratatui_image` 去缩 —— `4
× width × height` 字节，一瞬间；一张 1080p 截图 8 MB、4K 的 33 MB，为的是产出一
块约 3 MB 的东西。而且不只一瞬间：那个缓冲区恰好是 glibc 的动态 `mmap` 阈值学会
保留的尺寸，于是第一张之后的每张图片都把它留在身后
（`docs/architecture/memory.md`，*粘贴截图*）。

PNG —— 每个粘贴，以及 `read` 遇到的大多数截图 —— 现在改走
`images::fitted::decode_png_fitted`。`png` crate 一次吐一行，一个做面积平均的
`Downsampler` 把每一行折进它所属的目标行，于是被持有的永远只有*拟合后*的图片：峰
值是那一块加上一行，无论文件里装着什么。目标是 `fit_box` —— `Resize::Fit` 自己的
算术，像 `fit_cells` 一样被一份差分测试钉住 —— 所以编码器拿到的是一张已经拟合好
的图片，并照它原样构建协议。其他一切，以及隔行扫描的 PNG（它的行是乱序到达的），
都**整份**解码 —— 超过 `WHOLE_DECODE_MAX_PIXELS`（5000 万像素，那里整份解码正好
是这个设计要避免的尖峰）就拒绝 —— 再用 `thumbnail_exact` 投进框里，那是一个不需
要源图的 `f32` 工作副本的框式滤波，而编码器自己的 `resize` 会按源宽度每像素 16
字节地分配一份。流式解码本身在*时间*上而不是内存上有界
（`FIT_MAX_SOURCE_PIXELS`，2 亿像素：它从不持有它走过的行）。解码器是纯的、从任
何可 seek 的缓冲区读，所以 `images::tests` 拿它对着一个朴素的面积平均 oracle 在
源尺寸上（一次精确拷贝）、跨这个格式的每种颜色类型 —— 包括带 `tRNS` 的调色板 ——
校验，并要求它在读任何一行之前就拒绝一张隔行扫描的图、一张被截断的图和一个超界的
头。

### 只画进那些预留的单元格

`stamp` 把一块的范围**从载体上读回来** —— 它的可见行在哪里、有多宽、有多少行在帧
之上 —— 而不是从 placement 上取。由此有两件事，而它们都是第一版里的 bug。

一块只被画进那段预留区，别的什么都不碰，于是一张被帧截短的图片绝不会盖到它下面已
经画好的东西：pager 自己的 `─── 100% ───` 分隔线以及它的按键提示行。

而**头一行在帧之上的一块照样画**。这是常规情形，不是边角：Ctrl+O 打开时钉在*底
部*，所以任何比 pager 主体更高的图片都从窗口上方开始，它的第一条可见行带着非零的
行索引。只从头一行开始渲染，会在最常见的打开方式上画出一整屏「预留了却空着」的行
（`smoke.sh` Phase 107c 量到修之前 0 行、修之后 10 行）。行索引就是切片的偏移，
以一个负的 `SignedPosition` 交给 `ratatui_image` 的
`SlicedImage`/`SlicedProtocol`。

切片也正是让 **sixel 与 iTerm2** 至少能裁剪的东西 —— 两者都拒绝绘制一张比给它们
的区域更大的图片。`SlicedProtocol` 在渲染时剥掉 sixel 的带，并把一张 iTerm2 图片
切成每行一个协议；kitty 原生就跳过占位符行，而半块是普通单元格。

### 屏幕切换

kitty —— 以及实现了它协议的 Ghostty —— 给**每个屏幕缓冲各分配一个 image
store**：kitty 自己的源码里是 `main_grman` 与 `alt_grman`，而协议规范写得很清
楚：*"when switching from the main screen to the alternate screen buffer (1049
private mode) all images in the alternate screen must be cleared"*。因此在一个屏
幕上传输的图片，从另一个屏幕上寻址不到。

那是第一版的 bug。kitty 协议对每个已编码的协议对象恰好*一次*地传输一张图片 —— 并
创建它的 `U=1` 虚拟 placement —— 于是一个被带过这个跳转的协议，用指名着一张备用
屏的 store 从未听说过的图片的 unicode 占位符，画出了 Ctrl+O 的 transcript。查询
只是返回，而 `q=2` 压制回复，于是它**悄无声息地**失败了：预留了行，里面却什么都
没有。

所以**一次编码属于一个屏幕** —— 而且一旦做过就留着。缓存以 `(placement, screen)`
为键，`enter_overlay`/`exit_overlay` 调用 `ImageStore::enter_screen`，而它现在只
是跟随这次切换：每个屏幕对一张给定的图片**只上传一次，永远**，之后每次访问只重绘
占位符。

备用屏那份拷贝会活下来，这不是假设。1049 切换时的清除恰好放过这个协议所用的
placement —— kitty 的过滤器开头就是 `if (ref->is_virtual_ref) return false;` ——
而它背后的图片也不会被回收，因为那个虚拟引用*算*一个引用（`filter_refs` 只在
`!vt_size(&img->refs_by_internal_id)` 时才释放一张图片）。公开的规范从另一面说了
同样的事：虚拟 placement 绝不会被 `a`/`c`/`p`/`q`/`x`/`y`/`z` 这些删除类别碰到，
而那正是切换与我们的 `ESC [ 2 J` 都在用的。已在 kitty 0.28（占位符发布的那一版）
到当前版本、以及 Ghostty 1.1 起验证过。

这要紧，是因为一张图片在线上不便宜。`ratatui_image` 把 kitty 图片当作原始
**RGBA** 传输，于是一张 120×35 单元格的图片是约 3.4 MB 的像素、**约 4.5 MB 的
base64**。在一次 Ctrl+O 开关上量过：

| | 第一版 | 每屏一份缓存 | 加保留 |
| --- | --- | --- | --- |
| 打开 | 4.53 MB | 4.53 MB | 4.53 MB |
| 关闭 | 4.53 MB | 1.3 KB | 1.3 KB |
| 重新打开 | 4.53 MB | 4.53 MB | **19.6 KB** |

剩下那一次上传是诚实的那一次：你第一次打开它时，备用屏的 store 确实没有这张图
片。

`ZEROTH_IMAGE_RETRANSMIT=1` 走保守路线 —— 每次切换都重传 —— 给那些讲这个协议、却
不讲这一部分的终端。没有它，这样的终端在第一次 Ctrl+O 时能显示图片，第二次就只有
空白行。我们没法问它是哪一种：协议里没有这样的查询，而启动时的那次读（*去问终
端*）只能携带终端回答得了的问题。

只有 kitty 付这些账。Sixel、iTerm2 与半块不按屏幕保存任何东西（它们每次渲染都带
着整个载荷），所以它们共用主屏的那一条，只编码一次 —— 在一个本就该瞬间打开的视图
上，因为切换屏幕而重新编码是浪费（`docs/ui/tool-view-performance.md`）。

被弃用的 image id 会自己清理：kitty 的配额是按缓冲的，压力之下*"existing images
without placements will be preferentially deleted"*，而下次进入备用屏会直接清掉
它的 store。

`smoke.sh` Phase 107b 是那道护栏 —— 进**去**的路上一次传输、回来的路上一次都没
有、重新打开时也没有。它经 `pipe-pane` 读原始字节流而不是窗格的文字，因为窗格文
字恰恰是分不开这些情况的东西：一个 kitty 占位符*就是*一个普通单元格，所以无论图
片最终会不会出现，截下来的样子都一样；无论有没有兆字节跟着它走，也一样。

### Resize

每次 resize 都按历史 purge 重建对话（不变量 3），那会在新的宽度上重跑
`images::place`，于是把每张图片按新尺寸重新编码。purge 还会丢掉已编码的图片
（`ImageStore::invalidate`，来自 `clear_scrollback_and_screen`）：一个 kitty
placement 每个已编码的协议只传输一次像素，所以一个活得比 `ESC[3J` 更久的
placement 会摆出一张终端可能早就丢了的图片。

### 内存

一个 kitty placement 以 base64 RGBA 持有整张图片 —— 一张 120 列的截图在 10×20 的
单元格下是 1200×600 像素，约 2.9 MB 像素、约 3.8 MB base64 —— 而这个进程在用户的
终端里整天空转（`docs/architecture/memory.md`）。所以 store 按**字节**限额，尺寸
由 placement 自己的几何估算，而不是数条目：超过 `CACHE_MAX_BYTES`（24 MB）就丢掉
最久没被绘制的那张。再次遇到它只花一次重新编码，绝不会是一张错的图片。而填满一条
目的那次解码由块限额，不由文件限额（*按拟合后的尺寸解码*）：画一张 4K 截图的花
费，与画一张 1080p 的一样。

## `/settings` 的几行

| 行 | 默认 | 做什么 |
| --- | --- | --- |
| **Show images** | `true` | 到底要不要内联绘制图片。终端画不了图片时不可用 —— `false (unavailable)`。 |
| **Image width** | `120` | 以列计的宽度**上限**；在 60 / 80 / 120 之间轮转。 |
| **Auto-resize images** | `true` | 在一张大图**发给模型**之前把它缩小。与显示无关，所以两种情况下都保持可用。 |

轮转这两条显示行中的任何一条都会重新发布策略、丢掉已编码的图片并 purge 重建对
话，于是已提交的图片立刻改变尺寸（或消失）—— `/mascot` 切换的规则。它们像别的每
个旋钮一样以相对默认值的差异持久化在 `settings.json` 里
（`docs/state/settings.md`）。

**Auto-resize images** 是载荷那一行，而且它刻意是另一种东西：一张 1200 万像素的
手机照片是几兆字节的 base64，provider 要么直接拒收、要么全额计费，而模型读它并不
比读同一张 2000 像素的图片更好。于是那两条上传像素的路径 —— `read` tool 的图片分
支和一个 Ctrl+V 附件 —— 先让它们的字节走一遍 `images::payload`。JPEG 仍然是 JPEG
（一张照片重新编码成 PNG 会*变大*）；其他一切都变成 PNG。PNG 由显示所用的同一个
流式解码器缩小，直接拟合到 2000 像素的目标，所以缩小一张 4K 截图的代价是目标的约
9 MB，而不是图片的 33 MB；其他格式整份解码（超过 `WHOLE_DECODE_MAX_PIXELS` 就拒
绝）并 `thumbnail_exact` 进去，不经过 `f32`。

而且它只做**一次**。一个附件在之后的每个回合都随上下文重发，而那些回合过去都会把
原件重新解码、重新缩一遍 —— 对一张 4K 截图，只要图片还在上下文里，每个回合就是
20 MB 的尖峰。缩小后的字节现在**为本 session 留在磁盘上**（`images::payload` 在
`{tmp}/zeroth-{uid}/{session}/images/` 下的
sidecar，`docs/tools/scratchpad.md`），键是文件的路径、大小、mtime 与这个上限，
之后的回合就从这个小的文件应答请求 —— `cached_downscale`，在原件被打开之前就被查
询。正是这个键让陈旧的载荷不可能存在：变了的文件是另一条。关掉这一行则什么都不缓
存、不应答，与以前完全一样；`downscale_to`，那个不带缓存的核心，是单元测试所驱动
的，而缓存有它自己的测试（只写一次、读回来不经过解码、没有缓存目录时不写任何东
西、关掉这一行时不应答任何东西）。`read` tool 的图片走同一份缓存，所以模型读两次
的图片只缩一次。而**编码**也只做一次：线上格式携带的是作为 base64 `data:` URL 的
载荷，`images::attachment` 在第一个发送这张图片的回合把那个串建出来 —— 直接从
sidecar（或文件，当它本来就够小时）流进 session 保留的那一个串 —— 之后每个请求都
按引用拿到同一个串，键是路径，并对着文件的大小、mtime 以及这一行的设置校验（所以
翻转这一行会重建它）。以 `ATTACHMENT_CACHE_MAX_BYTES` 为界，并在每个回合开始时扫
到上下文仍然带着的那些图片，正是它让粘贴之后的那个回合不花图片尺寸的钱
（`docs/architecture/memory.md`）。**文件**本身没被碰过，这就是屏幕上的图片不受
影响的原因，也是一次自动缩放的读取其事实行先给出文件自己的尺寸、再点名发出的尺寸
的原因：

```
Read image (PNG, 4000x3000, 4.6 MB; sent resized to 2000x1500, 1.1 MB)
```

那个顺序两次承重：模型需要原始尺寸，才能把从图片上读出的一个坐标映射回文件；而
`images::read_image_size` 把**第一个** `WxH` 当作它即将从磁盘绘制的那张图片的尺
寸 —— 也就是原件。

关掉这一行，过大的图片像以前一样被拒（错误现在会指向这个设置）。缩小在源有超过
5000 万像素或 64 MB 字节时也会直接拒绝：解码要常驻 `4 × width × height` 字节，而
缩一张 12000×12000 的扫描件要花 576 MB 才发现这不是个好主意。

## 已知限制

* 在 **tmux/screen/zellij** 之下，协议的猜测刻意保守：除非外层终端是已知的
  iTerm2 家族，否则是半块，而且从不去问终端。对启用了 passthrough 的配
  置，`ZEROTH_IMAGE_PROTOCOL` 可以覆盖它。
* 一个对 kitty 查询回答 `OK`、却不绘制 unicode 占位符的终端，如果在既没有名字
  （XTVERSION）也没有环境说明它是谁的情况下被遇到，就会拿到它显示不了的 kitty 图
  形 —— WezTerm 与 Konsole 靠名字被抓住，未知的抓不
  住。`ZEROTH_IMAGE_PROTOCOL=halfblocks` 可以解决它。
* live 的**流式预览**不画图片：一次 `read` 在解析之前没有图片，而提交后的单元格
  还差一帧。

## 手动驱动它

`cargo run --example make_test_image -- shot.png 640 400` 写出一张带白色对角线的
渐变图 —— 比例不对或被裁掉一行，一眼就看得出来。然后让一个视觉模型去 `read` 它，
或者强制某个协议；粘贴那条路径用 `cargo run --example clipboard_owner -- 1920
1080`，它在 X11 剪贴板上提供一张截图形状的 PNG 供 Ctrl+V 取走
（`docs/input/image-paste.md`，*Measuring*）：

```
ZEROTH_IMAGE_PROTOCOL=halfblocks ZEROTH_IMAGE_CELL_SIZE=5x10 cargo run
```

`smoke.sh` Phase 107 就是照着做的：对着一个手工造的、带一次真实 PNG 图片 `read`
的 rollout，它断言图片的行数、它从第 0 列开始、单元格下面那个空行、Ctrl+O 画出同
一张图片，以及关掉 `/settings` 的 **Show images** 后 purge 重建不再有它。Phase
107b 读原始字节流，断言一次 kitty 传输落在**进入**备用屏的路上、回来时一个都没有
（见*屏幕切换*）。Phase 107c 驱动一张比 pager 更高的图片，断言钉在底部的打开方式
仍然画出它可见的那部分。Phase 107d 覆写一个续上的单元格所显示的图片 —— 同样的路
径、同样的尺寸，连字节长度都一样 —— 断言 transcript 对那个 placement 的下一次绘
制画出新的颜色、一点旧的都没有（*原地被重写的文件*）。Phase 107f 续上一个 read
的文件已消失的 rollout —— 经 `--resume`，然后 `/resume` picker 与它的 Ctrl+O
transcript —— 断言不为它保留任何行，而同一段对话里的一个粘贴仍然能画；然后它把文
件写回去再删掉，每次都跟一次 resize，断言图片先回来、然后随它的行一起消失（*文件
已消失的图片*）。
