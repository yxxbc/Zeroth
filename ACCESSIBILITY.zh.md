# 无障碍

Zeroth 是一个终端应用。这里的「无障碍」指：读屏器读到什么、低视力或光敏感用户
能关掉什么、单色或浅色终端下能不能用，以及当界面成为障碍而不只是难看时该找谁。

这一页就是报告障碍的地方。它也是 GitHub 在仓库概况的 **Accessibility** 页签里
展示的那一页。

## 报告一个障碍

开一个标题为 `accessibility: <the barrier>` 的 issue（把尖括号里的内容换成你自己的描述）；如果 issue 本身也是障碍，可以写
邮件给 <xynrin@icloud.com>。请说明你用的终端和 OS、界面哪一部分挡住了你、你期望
它怎样。

针对下面任何一条的报告 —— 读屏器把对话读成一长串、主题对比度不够、某个动画关
不掉 —— 都是 `CONTRIBUTING.md` 意义上的 bug；即使你说不出怎么修，也值得开一个
issue。

## 界面对环境的假设

- **一个 80 列起的终端。** 低于 80 列时，live 区域会先减行再流入
  （[`docs/ui/view-flow.md`](docs/ui/view-flow.md)），但设计目标是标准终端。
  没有 GUI，也没有浏览器界面。
- **带 Unicode 回退的等宽字体。** 会用到盲文（默认 spinner 的轨道）、制表符
  （diff 与侧栏）和少量装饰字形（品牌标记）。缺字形的字体会显示一个占位方框，
  别的不会坏；**`line`** spinner 风格就是为这种情况准备的纯 ASCII。
- **终端内的颜色。** 没有颜色对话也可读 —— 一个工具的成败由它的动词和 `⎿` 行
  承担，不只靠 bullet —— 但主题是颜色优先的
  （[`docs/ui/theme.md`](docs/ui/theme.md)）。

## 可以关掉的东西

每一个动画、每一个颜色决定都有开关，而且关掉它们都不改变对话本身的内容。

| 障碍 | 开关 |
| --- | --- |
| 动效 —— spinner、shimmer、运行中 bullet 的脉冲 | `/spinner` → **`line`**（静止的 ASCII 条），按目录保存在 `spinner.json` |
| 流式的思维链 | `/settings` → **Hide thinking**；`ZEROTH_SHOW_THINKING=0` |
| 对话里的图片 | `/settings` → **Show images** 关；`ZEROTH_IMAGES=0` |
| 横幅的图形吉祥物 | `/mascot`；终端画不了图时会用文字画 |
| 侧栏那一列（是第二种布局，不是颜色选择） | `/settings` → **Sidebar** 关，这是默认值；`ZEROTH_SIDEBAR=0` |
| 状态行上挂的提示 | `/settings` → **Show tips** 关 |
| 颜色，追求最大对比 | `/theme` → **`ansi`** —— 终端自己的十六色，对比度由终端设置决定，不由我们 |
| 浅色终端 | `/theme` → **`latte`**（Catppuccin 的浅色口味） |
| 超链接 | `ZEROTH_HYPERLINKS=0` —— 完全不发 OSC 8 序列 |
| 鼠标捕获 | inline 模式从不开；只有全屏页面打开时（`docs/ui/full-screen.md`），关闭时释放 |

用 `/settings` 选的开关按目录持久化
（[`docs/state/per-directory-state.md`](docs/state/per-directory-state.md)）；
`/mascot` 和 `/spinner` 也这样持久化，`/theme` 则是用户唯一的选择。

## 读屏器能拿到什么

终端暴露它的屏幕缓冲区，而应用画的就是一块普通的缓冲区。两个后果：

- **对话就是从上到下的文本。** 工具单元格、diff 和表格都只是缓冲区里的字符，
  与散文一样。线性读缓冲区的读屏器读到的顺序与视力用户读到的顺序相同。
- **某个视图占据屏幕时，帧是重画的，不是滚动的。** Ctrl+O 的完整记录、各种
  picker 和全屏模式都画在备用屏上；挂在主屏上的读屏器要等页面关闭才看得到。
  跟随备用屏的读屏器则能正常读到。

inline 默认模式部分就是为此存在的：完成的回合被**提交进终端自己的 scrollback**
（[`docs/architecture/async-rewrite.md`](docs/architecture/async-rewrite.md)），
所以对话就是普通的终端历史，读屏器、`tmux capture-pane` 或复制粘贴都能拿到。
`--inline`（或 `ZEROTH_TUI=inline`）可以强制这个模式。

**Ctrl+O** 的完整记录是那份完整的档案 —— 每个工具的完整输出、每一段思考、每一
次压缩 —— 不用鼠标就能到，`q` 或 Esc 离开
（[`docs/ui/full-screen.md`](docs/ui/full-screen.md)、
[`docs/ui/overlay-repaint.md`](docs/ui/overlay-repaint.md)）。

## 已知的障碍

这些是真实存在、尚未修复的：

- **没有「减少动效」检测。** 应用不读平台的减弱动效设置；动效靠选 `line`
  spinner 关掉，而不是自动的。状态动词上的 shimmer 即使选了 `line` spinner 也
  还在跑 —— 它还没有自己的开关。
- **没有按键重映射。** 快捷键是固定的
  （[`docs/ui/shortcuts.md`](docs/ui/shortcuts.md)，空 composer 里按 `?` 查看）。
  终端层面重映射可行，但应用本身不提供。
- **颜色是一个通道，不是唯一一个。** 页脚右边缘的权限模式随着放开的范围从暗色升
  到红色，但那个词本身始终在；看不见颜色的用户必须读那个词。上下文量表同理。
- **live 区域是一行不断重画的区域。** 读屏器读状态行时可能每帧重读一次。用
  `--inline` 工作并等回合结束可以避开；没有「只播报一次」的模式。
- **没有声音或触觉反馈。** 设计上就没有，也没有计划。

如果上面某一条正是你无法使用 Zeroth 的原因，请开 issue 说出来 —— 被点名的障碍
比我们猜的更有价值。

## 这一页由什么保证

没有机械检查保证这一页。删掉上表里的某个开关、或让某个动画无法跳过，是同一次
提交里改这个文件的事。
