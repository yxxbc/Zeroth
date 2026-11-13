# `?` 快捷键条 —— 输入框下方的键盘快捷键帮助

日期：2026-06-11

## 目标

在**空 composer 里按 `?`**，显示一份简短的键盘快捷键总览 —— 也就是
openai/codex 的 `?` 切换其页脚快捷键 overlay 的做法 —— 再按一次（或做任何别的
事）就把它藏起来。composer 里有文本时，`?` 只是一个字符。

这份总览自己适配终端：能放下多少列就排多少列、每个条目都保持完整，所以宽终端把
宽度花在列上而不是输入框下方的行上，一直到两列短写，再窄就一行一个条目；而在短
到放不下它的终端上，显示放得下的那些行，外加一条其余条目数的计数（见下面
*As many columns as the width holds* —— 这套自适应布局始于 2026-11-06）。

## codex 怎么做（从 `/tmp/codex/codex-rs/tui` 移植）

`bottom_pane/chat_composer.rs`（`handle_shortcut_overlay_key`）+
`bottom_pane/footer.rs`：

- 切换绑定到**纯 `?` 和 shift+`?` 两者**
  （`toggle_shortcuts_keys`），因为终端在 Shift+/ 是否上报 SHIFT 修饰键上各不
  相同。
- 它**只在 composer 为空时**（并且没有粘贴爆发在进行中）才切换 —— 「这样输入/
  粘贴 `?` 仍然插入文本，而不是打开帮助」。
- overlay 是一个页脚模式（`FooterMode::ShortcutOverlay`）：
  `toggle_shortcut_mode` 在第二次按下时把它翻回基础模式，而
  `reset_mode_after_activity` 在任何别的活动上把它关掉 —— overlay 只用于显示，
  从不模态：别的键照旧干自己的活。
- 内容（`shortcut_overlay_lines`）：每条绑定一个 `{key} for {thing}` /
  `{key} to {verb}` 条目（`/ for commands`、`shift+enter for newline`、
  `ctrl+c to quit`……），排成**两列对齐**，整块是暗色的，键名高亮。退出的那条是
  上下文相关的：任务在跑时读作**「to interrupt」**，否则读作「to quit」。
- 空 composer 的页脚用一条暗色的 `? for shortcuts` 提示为它做广告。

## 映射到这个代码库

### 状态（`App.shortcuts_open: bool`）

一个标志位 —— 我们的页脚恰好只有两个模式（关/开），所以 codex 的
`FooterMode` enum 塌缩成一个 bool。在 `App::on_key_conversation` 里：

- `?`（`Char('?')`，不带 CTRL/ALT —— 允许 SHIFT，也就是 codex 的双绑定）在
  **输入为空**时切换这个标志并吞掉该键。回合中途也能用（条在输入框下方；上方
  流式输出的条不受影响）。
- 输入非空时的 `?` 穿透到普通的字符分支。
- **条开着时的任何其他键先把它关掉**（codex 的 activity 之后重置），然后照常
  执行 —— 打字就是打字，↑ 就是回溯，`/` 打开面板。例外是**只做关闭的 Esc**
  （返回 `Action::None`）：我们空闲时的 Esc 是退出（在没有可回溯的东西时）或者
  武装 Esc-Esc 回溯（`docs/input/backtrack.md`），对一个正在读帮助的人做这两件
  事中的任何一件都是敌意的 —— 与命令面板早就有的「弹窗优先」优先级相同。

条与面板在构造上互斥：面板需要一个非空的 `/token`，条需要一个空的 composer，而
造出其中任何一个状态的按键都会关掉另一个。

### 渲染（`ui::shortcuts_rows` / `ui::shortcuts_lines`）

条复用了面板的槽位 —— 预留的**输入框下方**那些行（codex 的页脚也坐在它的
composer 下方）。`band_rows` 把
`menu_rows(app, width) + shortcuts_rows(app, width)` 与两个 picker 的行相加
（最多只有一项非零）；`render_live` 画出开着的那个，`cursor_position` 预留同样
那些行（条打开时光标不动），而边界的 `live_region_height` 把同一个和传给
`ui::live_height`。

每个条目都是 codex 的 `{key} for {thing}` / `{key} to {verb}`，键名青色、标签
暗色（`shortcuts_key_color()` / `shortcuts_text_color()`），而每一行都以空的
`MENU_INDENT` 内缩开场。条占据页脚的槽位，而页脚那行也按同样的两列内缩
（`  {model} · {cwd}`），所以帮助打开的位置，正是它所替代的那一行的开头 ——
codex 的 overlay 作为它页脚的其中一个模式，也穿着它页脚的内缩。它也是输入框下方
其他每条带子起始名字所在的列，仅次于面板和各个 picker 的 `→ ` 标记。

#### 宽度能放下多少列就排多少列

条过去是一个固定的两列网格，第二列在第 32 列。在 87 列 —— 这次重设计所针对的
那个终端 —— 那会让这一行的右三分之一空着，并在输入框下方用八行去放一张参考卡。
现在它**把更宽的终端花在列上，而不是行上**。

80 列，经典的默认值 —— 三列，六行：

```
  / for commands             shift+enter for newline   ctrl+o for transcript
  ! for shell command        ctrl+v for image paste    ctrl+d for llm context
  @ for file paths           ctrl+w/u/k to kill text   ctrl+t to cycle thinking
  $ for skills               tab to queue next turn    shift+tab for permissions
  ↑ for input history        alt+↑ to edit queue       esc to quit
  ctrl+r to search history   ctrl+b to background      ctrl+c to quit
```

120 列 —— 四列，五行：

```
  / for commands           ctrl+r to search history      alt+↑ to edit queue         ctrl+t to cycle thinking
  ! for shell command      shift+enter for newline       ctrl+b to background        shift+tab for permissions
  @ for file paths         ctrl+v for image paste        ctrl+o for transcript       esc to quit
  $ for skills             ctrl+w/u/k to kill text       ctrl+d for llm context      ctrl+c to quit
  ↑ for input history      tab to queue next turn
```

从 149 列起，六列三行。低于 53 列时完整措辞排不出两列，于是各标签丢掉它们的
`for`/`to`，条保持两列九行 —— 50 列：

```
  / commands                 tab queue next turn
  ! shell command            alt+↑ edit queue
  @ file paths               ctrl+b background
  $ skills                   ctrl+o transcript
  ↑ input history            ctrl+d llm context
  ctrl+r search history      ctrl+t cycle thinking
  shift+enter newline        shift+tab permissions
  ctrl+v image paste         esc quit
  ctrl+w/u/k kill text       ctrl+c quit
```

再低于 46 列 —— 连那个也放不下 —— 一行一个条目，又用完整措辞（无论哪种方式，
一列都要花同样多的行）：

```
  / for commands
  ! for shell command
  @ for file paths
  …
```

布局（`ShortcutsGrid::for_width`，`ui/menu.rs` 私有）求的是**列能整份塞进宽度的
最少行数**：

- **按列优先，像 `ls` 一样。** 条目按 `SHORTCUTS` 的顺序沿每一列往下排，所以
  列表的顺序就是一个列里装的内容，而列表也为此分了组：composer 的各个符号
  （`/ ! @ $`）与历史，然后编辑与运行中的回合，然后是各个视图、各个模式与出路。
  三列时这三段恰好就是三列；两列时 composer 的键在左、会话的键在右。除不尽的
  计数会让**前面**的列各多一个条目（18 个放进四列是 5、5、4、4），所以空缺聚到
  条的右下角，而不是留下一列只有两个。
- **列宽按各自内容定，并在每一种上下文状态下都量过。** 一列和它有史以来所显示的
  最宽文本一样宽 —— 包括 esc 那条的 `esc esc to edit previous` —— 所以条开着时
  开始一个回合（`esc to quit` → `esc to interrupt`）绝不会让它之后的各列滑动。
- **行数最少，但至少三行。** 行数从 `SHORTCUTS_MIN_ROWS`（3）往上试，每一行数
  配上做出它的最少列数（`ceil(18 / rows)`：3 行是 6 列、4 行是 5、5 行是 4、
  6 行是 3、9 行是 2、18 行是 1），第一个其内缩、列宽和 `SHORTCUTS_MIN_GUTTER`
  （2）的间隙能塞进宽度者胜。这个下限的存在，是因为横跨超宽 pane 的一条两行丝带
  读起来像一条待扫的工具栏，而不是一份待读的列表。
- **只在能换来一列的地方才用短写。** 完整措辞连两列都做不出来时，就用同样的
  计数、但把每个标签开头的 ` for` / ` to` 去掉再试（`terse_label` —— 每个标签都
  以其中一个开头，有一个测试拿这一点来要求列表）。键名青色、其余暗色时，
  `ctrl+r search history` 读起来仍然是键与动作，而它把窄 pane 的条保持在半高：
  51 列下一列十八行把一条流式回复的预览挤出了条（`smoke.sh` Phase 99），而
  两列九行给它留了位置。完整措辞在两列或更多列里放得下时，绝不选短写，所以在
  一个正常终端上改变尺寸永远不会让措辞来回翻；而一列时又用回完整措辞，除非只有
  短写能整份放下（23 到 26 列）。
- **布局不需要的宽度用来加宽间隙**，最多到 `SHORTCUTS_MAX_GUTTER`（6）：在
  80 列时三列相隔三格、条结束在第 80 列；在 87 列时相隔六格、结束在 86。超过
  上限之后，更宽的终端把条留在原处，而不是把它撒满屏幕。

这些断点来自各条目自己的宽度：两个短写列从 46 列起，两列从 53 列起，三列从 78、
四列从 98、五列从 125、六列从 149 起
（`the_band_spends_a_wider_terminal_on_columns_not_rows` 在每个区间里钉一个
宽度）。
一个条目落在哪里，就按那里的列宽算，所以一个长的新条目可以挪动它们，而那个测试
说明了该去哪里看。有两个标签随布局一起变了。
`shift+tab for permission mode`（29 列）变成 `shift+tab for permissions`（25），
正是这一点让三列从 78 列起就放得下 —— 在经典的 80 之内 —— 而不是只从 82 起。
而 `ctrl+o for tool output` 变成 `ctrl+o for transcript`：自从移植进 pager
之后，
Ctrl+O 打开的就是整段对话的 transcript（它的标题行写着
`T R A N S C R I P T`），条是最后一个还在点名旧 tool 输出视图的地方。

`shortcuts_rows` 是整条带的已构建行数 —— 面板那条「高度就是行数」的规则
（`docs/ui/view-flow.md`）—— 所以预留的行与绘制的行在每个宽度下都一致。条排在
流式条之前取走它的行（`live_layout` 只有 composer 的地板会被它先扣住），所以在
一个 51×24 的 pane 上、有回复在流式输出时，composer、它的边框和状态行都还在屏幕
上，而回复的预览让出条所需的那几行，直到条关闭（`smoke.sh` Phase 99）。

#### 短到放不下这条带的终端

条要多少行只由宽度决定，但终端可能短到给不出：在一个 40×10 的 pane 上，一列的
条要十八行，而夹取到十行的区域留给 composer 三行。绘制拿到的是它实际得到的带的
面积（`shortcuts_lines(…, width, height)`），而一个行数多过它的条，会显示放得下
的那些行、但去掉最后一行，并在最后一行上用暗色 `… +12 more` 数出被略去的条目
—— 过去这里的裁剪会把列表底部（包括其中的出路）一声不响地丢掉。要多少行从不取决
于高度，所以预留的行与绘制仍然一致；只有绘制知道自己被短给了。这个标记是权限
prompt 的 `… +N more waiting` 惯用法（`SHORTCUTS_MORE_MARKER`）：一份静态总览
没有可滚动的窗口，所以那一行是点名被藏起来的东西，而不是指向它。

#### 各个条目

`ui/theme.rs` 里的 `SHORTCUTS` 常量是唯一的事实来源 —— 18 个条目，按阅读顺序。
`@ for file paths`（`docs/input/file-search.md`，条过去唯一没点名的符号）与
`ctrl+b to background`（`docs/tools/background.md` —— 把运行中的命令，或者前台
agent，挪到后台）随自适应布局一起加入。`$` 技能提及符号
（`docs/input/skill-mentions.md`）作为普通条目收尾第一列开头的那些符号。Ctrl+D
视图的 classifier 页没有自己的条目：Tab 是在该视图自己的提示行上被发现的，
`docs/state/permissions.md`。

Esc 条目是三向上下文相关的（也就是 codex 的退出条目）：
回合在跑时是 `esc to interrupt`（`SHORTCUTS_INTERRUPT`），空闲且有一条可编辑的
上一条用户消息时是 `esc esc to edit previous`（`SHORTCUTS_BACKTRACK`，
`docs/input/backtrack.md`），只有在没有可回溯的东西时才是 `esc to quit`。

### 已知的差异

- **没有常驻的 `? for shortcuts` 提示**：codex 有一条始终在场的页脚行可以放它；
  我们有意识地让空闲的 live 区域保持最小（一个输入框，别的一概没有），所以可发现
  性要花掉一行我们不想花。一旦被发现，条本身就是文档。
- **没有粘贴爆发防护**：我们的 `paste::PasteBurst` 住在 I/O 边界上，只用来给重绘
  计时 —— 纯的 `App` 看不见它。一次以 `?` 开头的非 bracketed 粘贴打进空
  composer，会短暂地切换条（下一个粘贴的字符又把它关掉，那个 `?` 丢了）。罕见且
  自愈；不值得把边界状态漏进核心。
- 没有 `customize shortcuts with /keymap` 的尾行（我们没有 keymap），而各个条目
  是这个 app 的绑定，不是 codex 的。
- **列数随终端而变。** codex 的 overlay 在任何宽度下都是两列，Claude Code 的是
  三列；我们的一到六列，取宽度能整份放下的最少行数。

## 测试

- `app`：`?` 在空 composer 上开/关（带 shift 的也算），而在非空草稿上打出字符；
  任何其他键关闭条但仍执行其动作（打字、↑ 回溯、`/` 面板）；Esc 只关闭 —— 空闲
  时（不退出）与回合中途（不打断、条关闭、回合不受影响）；`?` 在 tool 视图里被
  忽略。
- `ui`：`shortcuts_rows` 关闭时为 0、打开时在每个宽度下都是已构建行数（80 列下
  六行）；每个区间里某个宽度下的列与行（40 列下一列 18 个，50 和 60 下 2×9，
  80 和 87 下 3×6，100 和 120 下 4×5，140 下 5×4，200 和 260 下 6×3）；从一列
  那个宽度起，没有哪一行比它的终端更宽，而更宽的终端永不多花一行；顺序是按列
  优先，且不整除的分配会让最后几列更短；各列在每一种 esc 上下文状态下都保持
  不动，
  并且至少保留两列的间隙；间隙随富余宽度加宽（三列断点处是 2，宽两列时是 3，
  87 列时是 6，在 200 处封顶）；从 20 到 260 的每个宽度下，从左到右沿列读下去，
  恰好把每条绑定读一遍，且顺序正确；低于 53 列时短写保持两列（从 46 列起，esc
  的替换也用短写，每个标签都以其省略的那个连接词开头），而低于 46 列时一行一个
  条目、顺序正确、用完整措辞，只在完整措辞无法整份放下时回落到短写；每一行都以
  空的 `MENU_INDENT` 内缩开场（页脚的宽度）；键名青色、标签暗色、间隙无样式；
  每条绑定只列一次；Esc 条目三向翻转；一条被高度切断的带以 `… +N more` 收尾、
  数出被略去的条目，在一列和三列里都如此；`render_live` 在 40×10 终端上画出被
  切断的带、在 40×30 上画出堆叠的带，把条画在输入框下方，而它打开时
  `cursor_position` 原地不动。
- `src/tui/`（smoke，Phase 11）：`?` 显示条 —— 80 列下三列、内缩两列 —— 再按
  `?` 把它藏起来，而打一份含 `?` 的草稿最终原样落在输入框里；120 列下条是四列
  五行；在一个 50 列的 pane 上它是两列短写的九行；在一个 40 列的 pane 上它一行
  一个条目、每个条目完整；而在一个 40×10 的 pane 上它以 `… +12 more` 那一行
  收尾，
  composer 仍在屏幕上。Phase 99 在 51×24、一张流式表格之下把它打开，并发现表格、
  状态行、composer 和条同屏共存。
