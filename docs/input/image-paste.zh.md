# Ctrl+V 粘贴图片

> 粘贴进来的截图也会在对话里**画**出来，就在它自己的用户气泡下面 —— 见
> `docs/ui/images.md`，那里还讲了 `/settings` 的 **Auto-resize images** 行，
> 它在一次大粘贴被上传之前先把它缩小。

按 **Ctrl+V**（或 **Ctrl+Alt+V**）会在一个后台 worker 上从系统剪贴板读出一张
图片，存进这个 session 的**粘贴目录** ——
`{config_home}/image-cache/{session}/N.{ext}`，按粘贴顺序编号（剪贴板里已经是
编码好的图片、或粘进来的是一个可接受格式的文件，就逐字节拷贝；否则是 PNG）——
并在 composer 里放下一个紧凑的 `[Image #N]` 占位符。macOS 上要用
**Ctrl+V，不是 Cmd+V**：那个组合键根本到不了程序（见下文 *Cmd+V
(macOS)*）。真实文件路径记在旁边，在这个回合发送时随消息文本一起交给后端 ——
而且会**告诉**模型：在 wire 上占位符读作
`[Image #1: /home/me/.zeroth/image-cache/773c1c6cb321/1.png]`，于是模型
知道它正在看的这张图存到了哪里，可以再 `read` 一次，或者把路径交给某个工具
（见下文 *粘贴存在哪里*）。这是 openai/codex 剪贴板图片粘贴
（`tui/src/clipboard_paste.rs` +
`bottom_pane/chat_composer/attachment_state.rs`）
的一次聚焦移植。它直接建在 `docs/input/paste.md` 里已有的长文本粘贴占位符机制
之上 —— 图片占位符是*同一个*想法，只是载荷是 `PathBuf` 而不是 `String`。

> 注意：这**不是**一次 bracketed-paste 事件。普通文本粘贴以 `Event::Paste`
> 到达（见 `docs/input/paste.md`）；*图片*永远不会 —— 终端不走那条路流传图片
> 字节。Codex（以及这个移植）改为绑定一个显式的 `Ctrl+V` 键，通过 `arboard`
> **主动读取系统剪贴板**。两条路径互不相干，尽管都叫「粘贴」。

## 容器与 SSH：没有显示器的 session

Ctrl+V 读的是**进程能够够到的**那个桌面的剪贴板，而一条终端连接什么也不带：
`docker exec -it` 和 `ssh` 转发的是按键，从来不是显示服务器的选择区。这样的
session 不点任何显示器 —— 既没有 `DISPLAY` 也没有 `WAYLAND_DISPLAY` —— 而在
Linux 上，那里的粘贴会在**探测任何东西之前**就被拒绝（`require_display`，
下面的第 0 步）：

```
Failed to paste image: no desktop clipboard in this session (neither DISPLAY
nor WAYLAND_DISPLAY is set). Save the image file where this session can reach
it, then ask Zeroth to read its path.
```

以前它会一路落到 `arboard`，而它探测 X server 可能卡上几秒，然后报告它撞到的
任何东西 —— 对某个从没听说过 X11 的人讲一段关于 X11 的话。这次拒绝点名两个
变量，因为转发其中一个*就是*解决办法；结尾给出绕行的路，因为它哪儿都管用：
把图片放到这个 session 读得到的地方，然后按路径要它。那个认图片的 `read` 工具
随后把它上传给视觉模型并内联画出来，和一次粘贴完全一样。（在 composer 里敲
进去或用 `@` 选出来的路径是文本，不是附件 —— 真正挂上去的是那次 `read`。）

当终端转发了显示服务器 —— Wayland socket 和/或带 cookie 的 X11 —— 变量就设好
了，下面那条普通路径原样运行。一个点了名却够不到的显示器（重新登录后变陈旧的
转发 socket）保留 `arboard` 自己的原因，并加上同样的建议。

## 触发

`App::on_key`（只在会话视图 —— Ctrl+O overlay 没有 composer）把 `Ctrl+V`、
`Alt+V` 和 `Ctrl+Alt+V` 映射到一个新的 `Action::PasteImage`（codex 绑同一组
—— Ctrl+Alt+V 是对 WSL 友好的别名，那里裸 Ctrl+V 会被终端吃掉）。这是一个
纯决定：`on_key` **不**做任何 I/O，所以真正的剪贴板读取发生在 `main.rs` 的
边界上。

## 读剪贴板（I/O 边界）

`crate::clipboard::read_clipboard_image(dir) -> Result<PathBuf, String>` ——
`dir` 是 session 的粘贴目录 —— 是剪贴板 I/O 的边界（和 `term.rs` 一样）：
codex 的 `paste_image_as_png` + `paste_image_to_temp_png` 的移植，**跑在一个
worker 线程上**（见下文 *异步投递*）：

在 Linux 上它先问这个 session 到底有没有点显示器 ——
`require_display(DISPLAY, WAYLAND_DISPLAY)`，也就是上面那次拒绝。它**对这两个
值而言是纯的**，这不是讲究：edition 2024 让修改进程环境成为 `unsafe`，而这个
crate 禁止 `unsafe`，所以一条自己去读环境的规则没法做单元测试。设了却为空的
变量什么也没点；不是 UTF-8 的值仍然是一个显示器，接受还是拒绝是后端的事。它
不连接任何东西，也不猜 socket 路径 —— 配置好的显示器完全留给下面的步骤。

0. **owner 自己编码好的字节，流式取（Linux）** —— 在 arboard 还没被构造之前，
   因为每次构造都是一次 X11 连接加一个在结尾又要拆掉的服务线程：截图工具把
   图片*以 PNG 形式*放到剪贴板上 —— `image/png` 这个 X11 target、`image/png`
   这个 Wayland MIME —— 而那也是 arboard 向 Linux owner 索要、随后解码成 RGBA
   的唯一形状。`clipboard::linux::stream_image_into` 直接要它，并在字节到达时
   就把它们拷进粘贴目录：一个 Wayland offer 是一条管道（`wl-clipboard-rs`，
   arboard 自己的 Wayland 后端就建在它上面）；一个 X11 selection 是一次
   `ConvertSelection`，回答按 1 MiB 的 property 分片抓取，owner 怎么发就怎么收
   `INCR` 段（GTK 与 Qt 传超过几百 KB 的东西就是这么传的）。先要 `image/png`，
   之后是后端接受的其他格式 —— `image/jpeg`、`gif`、`webp`，各自落在自己的
   扩展名下 —— 于是只拿得出一张照片的 owner 也能被逐字节服务。不解码、不编码，
   不管图片多重手上也就一兆左右；流的长度上限是 `CLIPBOARD_IMAGE_MAX_BYTES`
   （256 MiB），超过它的流是唯一**不**往下落的拒绝 —— 把它交给一条会解码的
   路径，正是这个上限存在要避免的成本。一个不提供任何编码目标的 owner、一个
   读不到的 server、或一次卡住的传输，都会落到下面的步骤，所以最坏情况与原来
   完全一样。`docs/architecture/memory.md` 里有数字：这一步替换掉的往返是每次
   粘贴约 24 MB 的尖峰，而且 —— 经由 glibc 动态的 `mmap` 阈值 —— 是之后的
   图片*卡住*的原因。
1. `arboard::Clipboard::new()` —— 失败（一个点了名却够不到的显示器、没有剪贴板
   server）返回 `Err("clipboard unavailable: …")`，完整保留 arboard 的原因
   （`clipboard_unavailable`），在 Linux 上还给出与无显示器那次拒绝相同的绕行
   办法。
2. **先是文件**：如果剪贴板里是文件列表（例如从图形文件管理器里拷来的文件），
   `image_from_files` 取第一个可用的条目 —— 一个已经是可接受格式的文件
   （`png`/`jpg`/`jpeg`/`gif`/`webp`，用 `image::image_dimensions` 按头部校验，
   它只读头部）被**逐字节拷贝**进粘贴目录，扩展名保留：不解码、不重编码，基本上
   是瞬间完成。其他任何能解码的文件（用 `with_guessed_format` 做内容嗅探，所以
   标错扩展名的图片照样能用）被转码成 PNG。这一半只碰文件系统，所以与剪贴板那
   一半不同，它**是**可以在无头环境里做单元测试的。
3. **原始图片兜底**：否则 `clipboard.get_image()` 给出原始 RGBA（macOS 或
   Windows 上的截图，那里操作系统直接交出像素 —— 以及第 0 步什么也没找到时的
   Linux）；重建一个 `image::RgbaImage`，并把它 PNG 编码**直接写进它的文件**，
   经由 `BufWriter` —— 而不是写进一个不断增长的内存 vector 再拷到磁盘。
4. **macOS：AppKit 自己的强制转换**（`clipboard/macos.rs`），在第 3 步拒绝时。
   见下文 *那个 16 位 TIFF（macOS）*。它是兜底而不是第一步，因为它救回的图正是
   arboard *读不了*的那一张：`public.png` 的拷贝和 8 位 TIFF 在第 3 步就已经
   成功，因为 `NSPasteboard` 会把 PNG owner 的图按需重新编码成 TIFF。只有 16 位
   浮点那个形状才会走到这里。
5. 无论哪条路，字节都作为目录里的下一个编号落地 —— `1.png`、`2.jpg`、…… ——
   返回的是那个路径：永远是**我们自己的拷贝**，绝不是用户的原件（丢弃清理删的
   就是这个返回值）；后端按路径读文件（TUI 从不把它 base64 化 —— 与 codex
   一致）。下文 *粘贴存在哪里* 讲这个目录和编号。

一次什么图也没得到的粘贴会被 [`NO_IMAGE_ON_CLIPBOARD`](../../src/clipboard.rs)
拒绝，它点名两种看起来像图片却不是的东西 —— 一次*文件*拷贝、一个网页的选择区
—— 以及一个没有显示器的 Linux session 早就拿到的绕行办法：把图片存下来，然后
按路径要它。

## 那个 16 位 TIFF（macOS）

一张拷进剪贴板的截图（⌃⇧⌘4 —— 只用 ⇧⌘4 是存成文件）以 **`public.tiff`** 落在
pasteboard 上，而 `NSImage.tiffRepresentation` 默认写的是**每通道 16 位**
（`bitsPerSample = 16,16,16,16`、`sampleFormat = 3`，在这台机器上用 `tiffinfo`
量出来的）。arboard 的 macOS 后端向 pasteboard 要那个 TIFF，交给 `image`
crate，而它的 TIFF 解码器拒绝这个形状
（`Unhandled TIFF sample format 3 for 16 bits`）—— 于是在最寻常的一张 macOS
截图上，`get_image()` 回答的是 `Err(ConversionFailure)`。第 3 步把每个错误都
变成了 `None`，而一次粘贴对着一个装着完好图片的剪贴板报告了
**`no image on the clipboard`**。这是对照 codex 报的，它有同形状的 bug
（[openai/codex#17050](https://github.com/openai/codex/issues/17050)）。

`clipboard::macos::coerce_into` 请 **AppKit 自己**把同一张图以 PNG 交出来 ——
就是往 Preview 里 ⌘V 时发生的那次强制转换：

```
the clipboard as «class PNGf»     (via /usr/bin/osascript -e, path in argv)
```

`-1700`（「没法把……变成期望的类型」）是对一个不装图片的剪贴板的回答 —— 一次
文本拷贝、一次文件拷贝（arboard 的 `file_list()` 先处理那些）—— 它读作「不是
这条路」，而不是一次失败。其他任何东西都往上抛。目标是粘贴目录里的一个隐藏
**暂存文件**，先用 `set eof fp to 0` 写，好让一张更短的图永远不继承更长文件的
尾巴。那个文件自己从不占编号：粘贴会保留它的一份去掉元数据的拷贝（见下），
而暂存文件不管粘贴怎么结束都被删掉。

### 元数据是被剥掉的，不是带上的

**这次强制转换会连同像素一起把 pasteboard 的元数据拷过来。** 这是量出来的，
不是假设：一个带着 `ImageDescription`、`Artist` 和 `Software` 标签的 TIFF
放进去，AppKit 的 `«class PNGf»` 交回来的 PNG 里有

- 一个 **`eXIf`** chunk，装着同样的那些 TIFF 标签，以及
- 一个 **`iTXt`** chunk，装着一个 XMP 包，把那些标签*又*点了一遍 ——
  `dc:description`、`dc:creator`、`xmp:CreatorTool`。

对一张照片，同一条路会带上相机、镜头，以及（若开着定位服务）**GPS 坐标**。
对一张窗口截图，它可能带上窗口标题和截它的那个应用。而一张粘贴的图片不是只发
一次：`attachment_data_url` 把那个确切的文件 base64 进这个对话的**之后每一次
请求**（`docs/architecture/context.md`），于是一个在第一回合粘进去的地理位置
标签，在第二十回合还会到达 provider。

所以粘贴从不保留 AppKit 的那个文件。`clipboard::store_png_without_metadata`
把它拷进粘贴目录，**一个 chunk 一个 chunk 地拷，只留画图的那五个** ——
`IHDR`、`PLTE`、`tRNS`、`IDAT`、`IEND`（`strip_png_metadata`）—— 其他每个
chunk 都不读就跳过：`eXIf`、`iTXt`、`tEXt`、`zTXt`、`iCCP` 色彩配置（它的
描述可能点出显示器），以及这段代码从没听说过的任何 chunk。它是白名单而不是
一份「要删哪些」的清单，所以将来某个 AppKit 发明出来的 chunk 是被丢掉，而不是
被留下；而 PNG 的 chunk 不嵌套 —— 每一个，包括元数据，都是顶层的兄弟 —— 所以
元数据无处可藏。

留下的 chunk 是**逐字节**拷的，CRC 和一切，于是像素带着 AppKit 选的位深和压缩
原样到达，而图片从不被解码：不管多重手上也就一两个缓冲区，而一次解码再编码会
抱住整个位图 —— 一张 16 位截图每像素 8 字节，Retina 上就是几十兆 —— 只为了把
大部分再写出去（`docs/architecture/memory.md` 讲了这在这里为什么重要）。这次
拷贝走的是其他每次粘贴都走的那个 `store_image`，所以它同样被
`CLIPBOARD_IMAGE_MAX_BYTES` 夹住、同样做头部校验、同样编号。唯一放弃的是色彩
配置：一张广色域截图的颜色被当成 sRGB 来读 —— 原始 RGBA 那条路（第 3 步）一直
如此，因为它没有配置可留。

把范围说清楚，免得这条规则被误当成越界 —— 或者被当成比它实际做的更多：

- 它适用于 **AppKit 转换创建的那份拷贝**，那是我们的。`read` 工具的路径是
  *用户自己的文件*：原样发出去（或被 `/settings` → **Auto-resize images**
  缩小，那会重新编码，于是也丢掉元数据 —— 一个副作用，不是承诺）。
- 在 Finder 里拷来的**文件**是逐字节拷的（`image_from_files`），元数据一并带着。
  那是用户把他自己按名字拥有的文件交给我们；之后再被发出去的是粘贴目录里那份
  拷贝。
- 原始 RGBA 兜底（第 3 步）只从像素构建它的 PNG，所以它什么也不带。
- **Linux** 的流（第 0 步）*不*剥：它逐字节拷贝 owner 的 `image/png`、`jpeg`、
  `gif` 或 `webp` 字节 —— 那正是流式取它的意义 —— 所以 owner 写进去的任何东西
  都一起走，PNG 的文本 chunk 或 JPEG 的 EXIF 都是。`tests/clipboard_linux.rs`
  专门服务一个带文本 chunk 的 PNG，正因为要求它原样到达。

这条规则在 `clipboard.rs` 里做了无头单元测试，也就是 Linux 那道闸门跑的：
用的 PNG 带着上面每一个 chunk，剥过的拷贝必须与同一张图在不带它们的情况下编码
出来的结果逐字节相同，16 位和调色板图片都在内，而任何短于完整 PNG 的东西都被
拒绝，目录里什么也不留。`tests/clipboard_macos.rs` 然后在真实 pasteboard 上
端到端检查它：一个带着 `ImageDescription`、`Artist` 和 `Software` 的 16 位浮点
TIFF —— arboard 拒绝的那个形状，所以只有这个兜底能粘它 —— 必须能粘，而存下来
的文件里这三个标签一个都没有、没有 `eXIf`、没有 XMP 包，并且那张 8×8 的图
完好。一个对照组先跑 AppKit 对同一个 pasteboard 自己的转换，并要求那些标签
*在*：没有它，测试就分不清「剥掉了」和「一次转换本来就没带」。

**为什么用子进程而不直接用 AppKit**：那次转换需要
`NSBitmapImageRep::representationUsingType:properties:`，而 `objc2-app-kit`
把它标成 `unsafe`，这个 crate 又设了 `unsafe_code = "forbid"`（`Cargo.toml`）。
`osascript` 是一个第一方的 macOS 二进制，本来就会说 AppKit，于是 crate 保持
无 unsafe，也不多一个依赖。在这台机器上量到的成本：一张 200×120 的图约 150 ms，
且只在**失败**路径上，已经不在事件循环里、在 worker 线程上。Claude Code 的
`imagePaste.ts` 走同一条路。如果这个成本哪天真的碍事，替代品是一个小的、经过
审计的 `objc2` wrapper —— 而不是放宽 `unsafe_code`。

**`tests/clipboard_macos.rs`**（默认 ignored，和 Linux 那个一样）拿真实
pasteboard 把两半都钉住：一个由测试自己造的 16 位浮点 TIFF —— 绝不用位深由
机器显示器决定的那种 —— 粘成 `1.png` 且不留暂存文件，而一个文本剪贴板以上面
那条消息拒绝。在一台 Mac 上用
`cargo test --test clipboard_macos -- --ignored --test-threads=1` 跑它。CI 在
`macos-latest` 上跑（*clipboard (macOS pasteboard)* 那个 job），在那里一个够不
到 pasteboard 的测试会**失败**而不是跳过（`CI` 已设），所以绿的 job 意味着每次
粘贴真的跑了；同一个 job 也跑 `clipboard::` 的单元测试，因为 `macos.rs` 自己
那些在 Linux 那道闸门上从来编译不过。

## macOS 上粘贴跑不了的地方

Linux 在一个不点显示器的 session 里拒绝粘贴（`require_display`，见上）。macOS
没有 `DISPLAY`，它的闸门是另一道 —— 在构造 arboard 或 `osascript` **之前**就
问，出于同样的理由：

**窗口服务器。** `launchctl managername` 对已登录的桌面回答 `Aqua`，对守护进程
或 `ssh` 登录回答 `Background`，对锁屏回答 `LoginWindow`。只有第一个有
pasteboard：其余情况下 `NSPasteboard.generalPasteboard` 回答 NULL。
`macos::require_gui_session` 问它，并以

```
Failed to paste image: no desktop clipboard in this session (it is not a GUI
login, so `NSPasteboard` has nothing to read). Save the image file where this
session can reach it, then ask Zeroth to read its path
```

拒绝。规则本身是纯的 —— `is_desktop_session(name)` 与
`judge_session(Option<&str>)` —— 所以不动真实 session 就能单元测试。**一个问
不出来的主机放行粘贴**：`launchctl` 在每个 macOS 上都在 `/bin` 里，而跑不动它
绝不能拒绝一次本来会成功的粘贴；读取随后会以它撞到的任何东西作答。

**TCC 自动化。** 即使在 GUI session 里，macOS 也把「一个应用读另一个应用的
pasteboard」关在 *Automation* 之后，在系统设置 → 隐私与安全性 → 自动化里。一个
从未被授予过这项权限的终端第一次粘贴会以 `-1743`
（`errAEEventNotPermitted`）失败，而那个授权提示只在 GUI session 里出现 ——
于是 `ssh` 登录（或者一个提示被关掉的 session）拿到的是一句无从下手的原始拒绝。
`macos::refusal_for` 把它翻译成：

```
Failed to paste image: macOS has not allowed this terminal to read the
clipboard. Allow it in System Settings → Privacy & Security → Automation, then
try again — or save the image file where this session can reach it and ask
Zeroth to read its path
```

其他每个 `osascript` 失败都保留 AppleScript 自己的措辞，它比这段代码能编出来
的任何话都更具体。`-1700`（「没法把……变成期望的类型」）*不是*失败：它是一个
不装图片的剪贴板的正常回答，会落到「没有图片」那次拒绝。

## Cmd+V（macOS）

**Cmd+V 不管用，改代码也管不了。** 那个组合键属于终端模拟器自己的 Paste 菜单，
它在程序看到按键之前就把它吃掉了，然后 —— 如果剪贴板里没有文本 —— 什么都不发。
Kitty 的键盘协议*可以*报告 `SUPER`，但没有终端把 Cmd+V 映射到它上面。绑定是
**Ctrl+V**（或 Ctrl+Alt+V），和其他每个平台一样。

真正绑上的，是给那些确实转发 `SUPER` 的终端（一个 kitty 协议的 Super 映射，或
一个把 Cmd 翻成 Alt 的终端）用的：同一个分支里的 `KeyModifiers::SUPER` —— 于是
这个组合键解析成 `Action::PasteImage`，而不是落进字符分支敲出一个多余的 `v`。
字符分支出于同样的理由排除 `SUPER`。

## 粘贴存在哪里

**目录**是 `clipboard::paste_store_dir(config_home, session)` =
`{config_home}/image-cache/{session}` —— 默认是
`~/.zeroth/image-cache/773c1c6cb321/`，在 `ZEROTH_CONFIG_DIR` 下随配置目录的
其余部分一起移动；完全没有配置目录时（没有 `HOME`，没有覆盖项）同样的布局落在
系统临时目录下（`tui::config::paste_store_dir`）。session id 就是 scratchpad
树和 hook 载荷携带的那个（`docs/tools/scratchpad.md`）。放在配置目录下而不是
`/tmp`，是因为一段对话携带的图片会**在之后每次请求里重发**、并且**在 `/resume`
时重画**（`docs/architecture/context.md`、`docs/ui/images.md`），而 OS 的临时
清理器以前会把它们从这两者脚下拿走。目录在第一次粘贴时创建 —— 从不粘贴的
session 从不创建它 —— 由 worker 创建，因为那和第 0 步的读取一样是文件 I/O。

**名字**是目录的下一个编号：`clipboard::next_image_number` 是现有最高编号名字
加一，空目录里是 `1`，而只有词干是数字的名字才算 —— 于是粘贴按粘贴顺序读作
`1.png`、`2.png`、`3.jpg`，而一次粘贴永远拿不到目录里已有的名字。

**每个路径都先暂存。** 字节先写进目录*里*的一个隐藏
`.zeroth-paste-*.{ext}` 文件（流式的、拷贝的和 reader 那几条路走
`clipboard::store_image`，PNG 编码器直接写进一个）：放在里面，最后的移动才是
同文件系统的 rename；隐藏起来，半张图就不会出现在列表里，它那个非数字的词干也
不进计数。头部解析之后，`keep_numbered` 用一个排他创建**预定**下一个编号，再把
暂存文件 rename 到它上面。两次 Ctrl+V 是两个 worker，可能扫到同一个「下一个」
编号，但排他创建赢不了两次，所以输的那个直接拿再下一个 —— 没有任何一次粘贴会
被另一次覆盖。一次被拒绝的粘贴（垃圾字节、字节上限、编码错误）丢掉它的暂存
文件，目录保持原样。

**模型会被告知路径。** `paste::annotate_image_placeholders`（纯函数，
`paste.rs`）跑在 `context::context_messages` 里，也就是在推导模型所读的那段
对话时，把消息文本里的每个 `[Image #N]` 变成 `[Image #N: {path}]`，按文本顺序
把占位符与消息记录下来的路径配对（也就是 `paste::distribute_images` 存它们的
顺序）—— 按消息配对，在相邻用户条目合并*之前*，于是每份草稿的 `[Image #1]`
点的是它自己的那张图。一个没有路径的占位符保持原形；一个没有占位符认领的路径
（被编辑掉的那种）仍然会在结尾一行 `[attached image: {path}]` 里被点名，因为
不管怎样这张图都搭着这次请求走 —— 文件读不到时后端那句
`[image unavailable: {path}]` 跟在它后面。

它跑在*那里*而不是请求构建器里，是因为 `context_messages` 就是 Ctrl+D 渲染的
东西，而那个视图的契约是它显示发出去的内容（每个两段文本 tool call 都遵循的
规则）。再低一层去标注，放在 `llm::backend::chat_message` 里，会让调试视图显示
一个 wire 从未携带的裸 `[Image #1]`。一个地方，两个读者都满意。记录下来的消息、
transcript 和 composer 仍然保留裸占位符（与 codex 一致）—— 只有推导出的上下文
点名文件，在把同一张图作为附件列出的那行暗色 `image:` 附件行上。一次在粘贴
还写进 `/tmp` 的年代记录下来的 rollout 也以同样的方式标注：路径就是消息记录的
那个。

这个目录本身刻意**不**写进 system prompt。写在那里，会在每个 session 的每个
回合花掉 token 去回答一个只有真会粘贴的 session 才会问的问题 —— 而那个 session
反正会在图片自己的占位符上、在那个要紧的回合里被告知答案。

**恢复的 session 找得到它的图片。** 一次粘贴没有事实行可以据以保留自己的行
（`docs/ui/images.md`，*Where the pixel size comes from*），所以在
`/resume`、`--continue` 或 `--resume` 加载被布局之前，
`tui::view::recheck_pictures` 会重新读每张粘贴图片的头部
（`images::recheck_pictures`），图片就照原样画出来 —— 这也是这个目录住在那里的
原因。一张文件终究不见了的粘贴会在那里被标成缺失，也不占行
（`docs/ui/images.md`，*A picture whose file is gone*）。

**这个存储是有界的，按大小而不是按时间。** 一个永不遗忘的持久目录会无限长大，
所以启动时 `tui::config::sweep_image_cache` 对 `image-cache` 下每个 session 目录
求和（一次 `read_dir` 加每个文件一次 `stat` —— 一个字节都不读，所以几百个
session 也就几毫秒），并删掉 `clipboard::evictable_paste_dirs` 点名的东西：每个
**空**目录，也就是一次在创建目录之后失败的粘贴留下的、删了不损失任何东西的
那种，然后一次一个地删**最旧**的目录，直到存储能装进 `IMAGE_CACHE_MAX_BYTES`
（512 MiB；`ZEROTH_IMAGE_CACHE_MAX_BYTES` 可覆盖，`0` 表示不设上限）。当前
session 自己的目录永远不是候选。

按**大小**而不是按时间，也是它与那个「删掉两周前一切」的兄弟分支分道扬镳的
地方：rollout 是无限期保留的，所以一段对话只要用户留着就一直是可恢复的，而按
日历丢掉它的图片会弄坏一段还完好的对话，而磁盘还有富余。在这里，什么都没发生
直到存储真的很大，而之后成本落在最久没人打开过的那些对话上。
`rm -rf ~/.zeroth/image-cache/{session}` 可以手动退役一个；无论哪条路，它的消息
随后都带着 `[image unavailable: …]` 注记。

## 异步投递（粘贴为什么冻不住 UI）

Codex 在它的按键处理里同步调用 `paste_image_to_temp_png()` —— 对一个 release
二进制没问题，那里一张 1080p 截图 PNG 编码约 25 ms。在我们的循环里内联跑，它
会在编码期间卡住**一切** —— 彗星 spinner、shimmer、计时器、按键 —— 因为一个
线程驱动整个 `select!`。所以 `Action::PasteImage` 分支只**spawn**
（`tui::workers::spawn_image_paste`）：一个短命线程做剪贴板读取 + 解码 + 编码，
把 `Result` 送回循环的**第五条 `select!` 通道**，它的分支挂上图片
（`App::attach_image`）或呈现那条红色通知 —— 像每次提交一样按视图设闸（在
Ctrl+O overlay 下只记录通知；不变量 4）。worker 只发 —— 从不当 stdin reader
（不变量 1）—— 并且像 shell 管道的 reader 一样被 detached：退出时还在跑的
落伍者把一个临时文件写完，无害。两次 Ctrl+V 会 spawn 两个 worker，按完成顺序挂
上两个占位符 —— 与 codex 的同步处理产生的完全一样，只是不冻。

两条构建配置的注记让**编码本身**在 dev 运行里也很快（发出去的 release 二进制
一直都很快 —— 那也是 codex 感觉瞬间的原因）：依赖用 `opt-level = 3` 编译，我们的
crate 用 `opt-level = 1`（PNG 编码器对 writer 泛型，所以它单态化*进这个 crate*，
否则就会以未优化的形态运行）。在一张 1920×1080 RGBA 上量到：旧 dev 设置约
1.6 s → 现在约 60 ms（release 里约 25 ms）。Linux 上常见情形根本不再编码 ——
owner 的 PNG 被拷贝 —— 所以一次粘贴的成本就是一次文件拷贝的成本，时间上内存上
都是。

在 `Ok(path)` 上循环调用 `App::attach_image(path)`；在 `Err(msg)` 上它提交一条
红色 `Failed to paste image: {msg}` 通知（codex 的 `new_error_event`），用与
斜杠命令 `Notice` 相同的流中刷段顺序，好让通知在有回复正在流式输出时插到正确的
位置。

**文件生命周期**：一个*被丢弃而从未提交*的附件 —— 一次原子的占位符
Backspace/Delete、一份被 Ctrl+C 清掉的草稿、一个被 `/clear` 的队列 —— 把它的
路径放进 `App::take_discarded_images`，循环在每个按键事件之后排空它，
`remove_file` 掉那张成了孤儿的图片（纯核心记录丢弃，文件 I/O 留在边界上）。
*已提交*的图片刻意留在粘贴目录里：后端按路径读它们 —— 而且在每次历史重发时
再读 —— `/resume` 会画它们，模型被告知路径，所以与 codex 的临时文件不同，它们
**不是**交给 OS 临时清理器的那种（*粘贴存在哪里*）。

`Cargo.toml` 多了 `arboard`（带 `wayland-data-control`）、`image`
（`jpeg,png,gif,webp`，默认特性关掉）和 `tempfile` —— 外加只在 Linux 上、为
流式的 `image/png` 读取直接够到的 `x11rb` 和 `wl-clipboard-rs`：arboard 自己
就建在这两个后端上，版本也是它钉的那些，所以没有新东西被编译。这些 crate 在
Linux 上是纯 Rust，所以它们在 CI 里不需要系统包就能构建。**v1 不管**（之后可
加）：codex 的 WSL PowerShell 兜底和它的 Android `cfg` 桩。

## 模型（纯的，有单元测试）

```rust
// App — parallel to `pasted: Vec<(String, String)>` (text pastes)
pub images: Vec<(String, PathBuf)>,   // (placeholder, saved-picture path), insertion order
```

`App::attach_image(path)`（纯的 —— 用一个假的 `PathBuf` 测，正是 codex 单元
测试它的方式，不需要真实剪贴板）：

1. `placeholder = paste::next_image_placeholder(&self.images)` → `[Image #N]`，
   其中 `N` 是 `max existing #k + 1`（没有则是 1）。编号按**已有最大值**而
   不是数量，所以删掉一张图再挂一张永远不会复用标签 —— 占位符必须保持唯一，因为
   （不像 codex 的 element-ID 模型）我们**按字符串**匹配它们来删除
   （`docs/input/paste.md` 的「刻意不在这里」）。删除之后编号出现空档无害。
2. 在光标处 `self.input.insert_str(&placeholder)`，然后把 `(placeholder, path)`
   推到 `self.images` 上。
3. 重新推导 `/` palette / shell 模式 / `@` picker 状态（每次 composer 编辑都
   跑的那三样），好让紧挨着 `@token` 的插入行为正确。

### 占位符字符串

`paste::next_image_placeholder` 对应 `next_paste_placeholder`：基础形式
`[Image #N]`。它扫已存在的占位符找最大的 `#k`，返回 `#(k+1)`，从 `#1` 起。
（`paste.rs` 把两个占位符构建器放在一起。）

### 原子删除

一次 Backspace/Delete 删掉整个 `[Image #N]` 并丢掉它那条 `images` 条目，与文本
占位符完全相同。`paste::placeholder_to_delete` 和它的辅助函数
`longest_placeholder_at` 从 `&[(String, String)]` 泛化成 `&[(String, T)]`
（它们只读占位符字符串 `.0`），于是同一套光标规则逻辑服务两种载荷。
`App::delete_placeholder(backward)` 先试文本粘贴，再试图片；第一个命中者胜出。

### 发送时

与文本占位符不同 —— 后者被 `take_input` **展开**回它的真实文本 ——
`[Image #N]` 占位符**留在**消息文本里（与 codex 一致：发出/记录的文本字面上是
`"[Image #1] describe this"`）。*路径*走一条单独的带类型通道。在
`Action::Submit` 边界上，循环排空
`App::take_submission_images() -> Vec<PathBuf>`（清空 `self.images`，像 codex
的 `take_recent_submission_images` 一样），并把它穿进 `start_turn`。

因为占位符活在草稿文本里，一份只有图片的草稿从不是「空」的（`[Image #N]` 不是
空白），所以 Enter 会提交它；删掉占位符也就丢掉了图片，所以不存在「有图无文」
的状态。

## 协议（扩展后的后端接缝）

`ReplySource::spawn` 多了一个 `images: Vec<PathBuf>` 参数 —— codex 的
`UserInput::LocalImage { path }` 作为带类型的旁路通道，与文本 `prompt` 分开。
这就是那个写明的「换上一个真 AI」接缝：一个真正的视觉后端读每个路径并把它挂到
自己的请求上。`start_turn` 把 `app.take_submission_images()` 传下去；
`count_input_images`（`count_user_input` 的图片计数兄弟，后者决定文本的大小）
按每个挂上的图片抬高 `↑` token 的计数，好让状态行反映它们。

`DummyAi` 没有视觉，所以它**确认**图片：`turn_events` 接收图片数，非零时在回复
前面加上一小段 `Looking at your N image(s). ` chunk —— 这是这条通道端到端确实
带上了附件的可见证据。**真的 `LlmBackend` 现在有视觉了**：记录下来的用户消息
保留它的附件路径，而每次请求把它们作为 base64 `data:` URL 嵌进 OpenAI 的多模态
parts 形式里 —— 过去回合的图片随对话上下文重发，所以针对早前某张图追问也行 ——
用的是**这个 session 对每张图的那一次编码**（`images::attachment_data_url`，
在第一次发送它的那个回合构建，之后按引用共享；
`docs/architecture/memory.md`）。见 `docs/architecture/context.md`。回合中途的
消息**队列随批次一起携带附件**：`queue_draft` 把 `(placeholder, path)` 对暂存
进 `QueuedTurn::Messages` 条目（一次并入批次的 Enter 也按挂上顺序并入它
的图片），
队列冲刷把路径通过与空闲提交相同的带类型通道派发出去，而 Alt+Up 拉回会把它们重新
挂到 composer 上，于是恢复的草稿里的占位符又有东西支撑了 —— 一次回合中途的
Enter 绝不静默丢掉一个附件。

**一个已知不支持视觉的模型让粘贴降级，而不是死在上面。** 活动模型的图片输入
支持是从它的 `/v1/models` 记录里探测的（`ModelEntry::vision` —— OpenRouter 的
`architecture.input_modalities`、Venice 的 `supportsVision`；见
`docs/tools/tools.md` 的「Vision detection」），并随 `ModelConfig::vision` 进入
每个重建的后端。当它是 `Some(false)` 时，照样发 parts 数组会让整个请求失败
（OpenRouter 回 404「No endpoints found that support image input」），所以
`build_messages_for` 把每个附件换成一则文字注记
`[image omitted: {path} — the current model does not support image input]` ——
模型知道曾经有过一张图，并告诉用户它看不见 —— 而粘贴本身在附件落地的瞬间弹一条
红色 toast（`{model} does not support image input`），于是用户在发出之前就知道。
支持情况未知（`None` —— 记录里没说的 provider）保持今天这种乐观的挂载。

## 渲染

占位符在 composer 里和在已提交的用户消息里（那块暗色 `❯ …`）都是**纯文本**，
和文本粘贴占位符一模一样 —— 没有特殊样式，没有内联终端图片协议
（sixel/kitty）。Codex 同样把 `[Image #N]` 渲染成文本标记。`?` 快捷键条带多了
一条 `ctrl+v for image paste`。

## 测试

- `clipboard.rs`：读取中关于文件的那一半 —— `accepted_image_extension`（逐字节
  拷贝的白名单）和 `image_from_files`（一个真 PNG 逐字节拷进粘贴目录成
  `1.png`；一个带图片扩展名的垃圾文件被拒绝且什么也不存；一个不在白名单里的
  扩展名仍能通过内容嗅探解码并转码成 PNG）—— 只碰文件系统，所以能无头测；只有
  剪贴板那一半仍留在边界上不做测试。还有目录本身：`paste_store_dir`（配置目录
  下的 session 文件夹）、`next_image_number`（现有最高编号名字加一，非数字名字
  忽略）、跨每条路径按顺序编号且不留暂存文件、目录里已有的图从不被覆盖、目录在
  首次使用时创建。
- `paste.rs`：`next_image_placeholder`（基础 `#1`、`#k+1` 消歧、删除后的空档）、
  在 `(String, PathBuf)` 列表上泛化后的 `placeholder_to_delete`，以及
  `annotate_image_placeholders`（每个占位符按文本顺序拿到它的路径，不带图片的
  文本不受影响，没人认领的路径在结尾一行被点名，一个不是占位符的方括号原样
  不动）。
- `context.rs`：一条用户消息的占位符点名它存下的文件，而一个合并的批次先按
  每份草稿自己的图片分别标注。
- `llm/backend.rs`：请求构建器把（已标注的）文本和图片拆成 parts 且不碰文本；
  一个读不到的附件仍然被注记。
- `clipboard.rs`：`evictable_paste_dirs` —— 上限之内的存储不管多旧都不淘汰，
  超上限的存储丢最旧的直到装得下，空目录总是被删，当前 session 的目录永远不删，
  而上限为 `0` 表示不设限。
- `app/composer.rs`：`attach_image`（在光标处插入 `[Image #N]`、记录那一对）、
  往返（挂上再 Enter → `Action::Submit("[Image #1] …")` *并且*
  `take_submission_images()` 给出那个路径）、原子删除（一次 Backspace 清掉整个
  `[Image #N]` 并丢掉路径），以及 Ctrl+V → `Action::PasteImage`。
- `stream/dummy/turns.rs`：当 cue 带图片时，每个面向用户的脚本 `opening` 都带
  上那句确认，而 0 张时不产出任何东西；`DummyAi::spawn` 带上新参数。
- `scripts/smoke.sh`：一个在**没有点名显示器**时按 Ctrl+V 的 phase（无头 CI 的
  现实，以及每个容器和 SSH 登录）断言那条红色 `Failed to paste image` 通知说清
  了为什么（`no desktop clipboard`，点名 `WAYLAND_DISPLAY`）以及什么办法管用
  （`read its path`），并且 app 还活着。顺利路径的 composer 那一半（路径 → 占位
  符）由 `attach_image` 的单元测试覆盖 —— codex 也是这么测的。
- `clipboard.rs` 在不碰任何环境的情况下单元测试显示器规则：每一种未设/为空的
  搭配都以点名两个变量加读路径建议的方式拒绝；任一显示器 —— 包括非 UTF-8 的
  一个 —— 都保住原生路径；一个够不到的剪贴板保住它的原因；而两句话都钉在
  `APP_NAME` 上。
- `clipboard.rs` 也无头单元测试流式核心：`CappedWriter`（上限之内逐字节，超过
  则 `InvalidData`，正好等于上限允许）和 `stream_encoded_image_into_capped`
  （一个编码好的 PNG 逐字节作为目录的下一个编号落地；垃圾、空目标或超上限的流
  被拒绝并让目录保持为空），外加流式的 `encode_png_into` 精确往返像素。
  `tests/image_paste_memory.rs` 给每个阶段可以为常驻集花多少设了闸
  （`docs/architecture/memory.md`）。
- `clipboard.rs` 无头单元测试 macOS 转换的元数据剥离：`strip_png_metadata` 恰好
  保留 `IHDR`/`PLTE`/`tRNS`/`IDAT`/`IEND` —— 剥过的拷贝与同一张图在不带它的
  `iCCP`、`eXIf`、XMP、`tEXt`、`zTXt` 和私有 chunk 的情况下编码出来的结果逐
  字节相同，16 位和调色板图片都在内，`IEND` 之后的字节丢掉 —— 而任何短于完整
  PNG 的东西都被拒绝；`store_png_without_metadata` 给剥过的拷贝编号、拒绝垃圾
  和超上限的拷贝，两条路都不留任何东西。
- `tests/clipboard_linux.rs`（默认 ignored —— 它要一个 X server）：读取本身，
  对着一个真实的。`tests/support/x11_owner.rs` 是一个最小的 selection owner，
  像截图工具那样服务一个编码载荷 —— 整块放在一个 property 里，或者按 `INCR`
  分段 —— 而测试要求存下的文件 —— 第一种形状是 `1.png`，第二种是 `2.png` ——
  逐字节装着被服务出去的那些字节。被服务出去的 PNG 用一个非默认压缩级别编码并
  带一个文本 chunk，所以任何解码再编码都复现不了它：字节相等就是这些字节被流式
  取过的证明。一个只拿得出 JPEG（并拒绝 `image/png`）的 owner 必须把它的字节
  逐字节落成 `1.jpg`，而一个根本没有图的 owner 必须以
  `NO_IMAGE_ON_CLIPBOARD` 失败并让目录保持为空。CI 在 `Xvfb` 下跑它
  （*clipboard (X11)* 那个 job）；手动跑：

  ```
  Xvfb :99 & DISPLAY=:99 cargo test --test clipboard_linux -- --ignored
  ```
- `tests/clipboard_macos.rs`（默认 ignored —— 它要一个 GUI session）：macOS 的
  读取，对着真实 pasteboard。一个手工造的 16 位浮点 TIFF 经由 AppKit 兜底粘成
  功，它的标签永不进入存下的文件（对照一个证明 AppKit 的转换仍然带着它们的
  对照组），而一个文本剪贴板以 `NO_IMAGE_ON_CLIPBOARD` 拒绝。
  `clipboard/macos.rs` 单元测试它纯的那几半：session 规则（`judge_session`、
  `is_desktop_session`）、各次拒绝的措辞，以及脚本的形状（路径在 `argv` 里、
  `set eof fp to 0`、`try` 平衡）。两个都在 CI 的 *clipboard (macOS
  pasteboard)* job 里跑。

## 测量

`docs/architecture/memory.md` 里有前后对比的数字；这里是它们当初是怎么取的，
好让它们能被再取一遍。一切都跑在 `Xvfb` 上，正是这一点让一台无头的机器能驱动
真实的剪贴板路径：

```
Xvfb :99 -screen 0 1280x800x24 &
cargo build && cargo build --example clipboard_owner
scripts/paste_mem.sh target/debug/zeroth 1920x1080 3 halfblocks 1
```

`scripts/turn_mem.sh` 是它给粘贴*之后*那些回合用的兄弟 —— 一次粘贴、一次发送，
然后 N 次每次重发这张图的文本追问，每一步之后同样的采样（`LIVE=1` 从
`ZEROTH_PROVIDER`/`ZEROTH_MODEL` 驱动一个真 provider）；它的数字是
`docs/architecture/memory.md` 里*每个回合都重发了图片*那几张表，而
`examples/image_turn_probe.rs` 单独重放请求构建，不联网，用于每阶段成本。

`scripts/paste_mem.sh` 在 tmux 里驱动真二进制：`clipboard_owner` —— 集成测试
用的同一个 selection owner —— 以 256 KB 的 `INCR` 分段服务一张截图形状的图
（一个带逐像素噪声的渐变，所以它压缩起来像真的），那也是它服务一张 arboard 自己
的 owner 服务不了的 4K 图的方式；脚本按要求的次数按 Ctrl+V，可选地在回合里发送
每张图，并在每一步之后打印进程的 `VmRSS` 和 `VmHWM`（读自
`/proc/<pid>/status`）。`docs/architecture/memory.md` 的表格正是它的输出：五次
粘贴，以及三轮「粘贴并发送」，一半在 half-block 下、一半在 kitty 下。要测没有
显示服务器时的每阶段成本，`tests/image_paste_memory.rs` 在一个进程里量流式拷贝、
适配后的解码和载荷的收缩。

## 这里刻意*没有*的东西（范围）

- **WSL PowerShell 兜底**和**Android** 的 `cfg` 桩。
- **composer 里的内联图片显示** —— 那里的标记是文本；图本身画在已发送气泡下面
  （`docs/ui/images.md`）。
- composer 不阻止光标*走进* `[Image #N]`（与文本占位符同样的限制 ——
  `docs/input/paste.md`）；原子的 Backspace/Delete 覆盖常见情形。
- **占位符编号按草稿重新开始**，所以两条分别排队的消息可以各自带一个
  `[Image #1]`；一次把两者合并的批次在 Alt+Up 时恢复出两个以同一字符串为键的
  对。一次原子 Backspace 删掉某一次出现时只丢掉*它自己*那一对 —— 文本顺序里的
  出现与列表条目按顺序配对（`App::delete_placeholder` 的序数匹配）—— 所以另一次
  出现仍然有支撑。
- 一份被 **Ctrl+C 清掉**的草稿丢掉它的附件（记录下来的 ↑ 召回文本把如今没有
  支撑的占位符保留为纯文本 —— codex 反正把标记渲染成已发送消息里的文本）；一份
  **shell 模式**的草稿从不带图片（`!` 命令本地跑；空闲和排队两条路都丢掉它们）。
