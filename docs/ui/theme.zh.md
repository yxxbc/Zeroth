# 配色主题与 `/theme` picker

TUI 画的每一种颜色 —— picker 选中时用的青色、工具 bullet 的绿色、`⎿` 装订线的暗
色、用户气泡的底色、diff 的色调、横幅的渐变，以及代码块和 file cell 里的语法颜
色 —— 都来自同一个**主题**。**`/theme`** 命令 —— 第十个替换 composer的内
联 picker，版式照 codex 的主题 picker —— 在十一个主题之间选择，在列表旁边用真实
的 cell 预览它们，并把选择跨 session 持久化。默认是**Catppuccin Mocha**：代码块
自 syntect 移植以来一直穿着它（`docs/ui/markdown.md`），如今外框也配上了，而不再
是给 Catppuccin 的代码套一个One Dark 的青色边框。

## 目录

| 名字 | 标题 | 家族 | 代码主题（two-face） |
| --- | --- | --- | --- |
| `mocha` | Catppuccin Mocha | 最深的那一档 —— **默认** | `Catppuccin Mocha` |
| `macchiato` | Catppuccin Macchiato | 中等偏暗的一档 | `Catppuccin Macchiato` |
| `frappe` | Catppuccin Frappé | 暗色几档里最浅的 | `Catppuccin Frappe` |
| `latte` | Catppuccin Latte | **浅色**档，给浅色终端用 | `Catppuccin Latte` |
| `onedark` | One Dark | 这个 TUI 原来的外框，一个值都不差 | `TwoDark`（Atom 的） |
| `dracula` | Dracula | 暗底上鲜明的紫、粉与青 | `Dracula` |
| `nord` | Nord | 极地感，偏蓝而安静 | `Nord` |
| `gruvbox` | Gruvbox Dark | 复古 groove，温暖而土质 | `gruvbox-dark` |
| `solarized` | Solarized Dark | Ethan Schoonover 的精确调色板 | `Solarized (dark)` |
| `monokai` | Monokai | 经典的编辑器调色板 | `Monokai Extended` |
| `ansi` | Terminal | 终端自己的 16 色 ANSI | `ansi` |

每一条都是**一个设计系统**：它的外框调色板和它的代码主题来自同一个家族，所以一段
回复的代码和它周围的边框是相称的。这份一致性正是目录是一份固定清单、而不是一个自
由格式文件的原因：用户自己写的调色板得配上一个相称的 syntect 主题才值得存在，
而 two-face bundle 已经为上面每个家族都带了一个。

目录的**身份** —— 名字、标题、描述、顺序、`theme.json` 读回时经过的
`from_name` 往返 —— 是纯的 `app::Theme` enum（`Default` = `Mocha`），也就是
`app::Spinner` 那条规则。它的**外观** —— 每个主题的调色板 —— 属于样式，所以它住
在 `ui` 里，和其余每一个样式决策一起（`src/ui/palette.rs`），而 syntect的那一半
在 `highlight::CodeTheme`，一个目录条目一个变体，按位置配对
（`every_theme_pairs_its_chrome_with_its_own_code_theme` 钉住这个配对）。

## 调色板：二十个角色

一个主题就是一个 `Palette` —— 一张按**角色**排列的颜色表。外框画的每一种颜色要么
是其中之一，要么由其中之一派生：

| 角色 | 谁在穿它 |
| --- | --- |
| `text` | 回复正文、composer 的 prompt、工具的名字/参数/输出、shimmer 的峰、comet 的头 |
| `text_muted` | 未选中的模型 id、设置项的值、comet 的中段尾巴、思考横幅 shimmer 的底 |
| `dim` | `⎿` 的角与占位符、页脚的分隔符与还有空间时的量表、提示、计数器、等待中或运行中的 bullet（它闪烁用的那一种灰）、时间戳 |
| `border` | composer 框的边框，以及每个带边框视图的边框 |
| `user_fg` / `user_bg` | `❯ …` 用户气泡（以及 `!` 命令的横幅）—— 有意压低 |
| `selection_bg` | `/resume` picker 选中的那一行 |
| `accent` | 每个 picker 选中时用的颜色、system bullet、行内代码、横幅提示、权限 prompt 的标题、Ctrl+R 的查询、↓ 聚焦的页脚 chip 的填充、agent session 视图的 composer 标签 chip、页脚的模型名 —— 以及渐变的近端（横幅、每个页面标题、状态动词） |
| `on_accent` | `accent` 填充之上的墨色（页脚 chip、当前的提问 chip、agent session 视图的 composer 标签） |
| `link` | 链接的 URL、有序列表的标记、上下文视图的用户标签、页脚的 `edit` 模式 —— 以及渐变的远端（横幅、每个页面标题、状态动词） |
| `success` | 已完成的工具 bullet、diff 的 `+`、活动模型的 ✓、已完成的任务、顺利的后台通知、页脚与横幅里的路径、回复的引用块（codex 的绿） |
| `error` | 错误 bullet、失败的工具、diff 的 `-`、`!` shell 模式、页脚的 `master` 模式，以及越过自动压缩阈值的上下文量表 —— 还有混了三分之一往 `dim` 去的错误 toast |
| `warning` | `retrying n/m` 子句、上下文视图的 system 标签、ask 复核里未回答的警告、越过 75% 的上下文量表（页脚的和 `/context` 的）、页脚的 `auto` 模式 |
| `purple` | 上下文视图的 tool 标签、`/resume` 工具栏的焦点 —— 以及*推理*：状态行的 `Thinking for` 子句、页脚的思考模式与速度档 |
| `diff_add_bg` / `diff_del_bg` | 新增 / 删除的带编号行的底色 |
| `diff_add_mark_bg` / `diff_del_mark_bg` | 真正改动的那几个字符之下更亮的标记（`docs/ui/inline-diff.md`）；改动段的墨色被压在它们之上的 3:1，代码主题的颜色读不出来时朝 `text` 抬 |
| `pulse_dim` | `pulse` spinner 风格一呼一吸的谷底 |
| `code` | 代码块和 file cell 用来上色的 `highlight::CodeTheme` |

**派生**是 `ui/theme.rs` 里的访问器函数（`tool_ok_color()` 是 `success`，
`menu_selected_color()` 是 `accent` —— `agent_view_label_bg()` 也是，那个
agent session 视图的 composer chip，它的 `agent_view_label_fg()` 是`on_accent`，
`header_gradient_start()`/`header_gradient_end()` 是 `accent`→ `link`，
而 `header_gradient_at(col, total)` 是它们之间的漫染，mascot、每个页面标题和状态
动词的静息色（`status_verb_color()`）都读它，
`status_thinking_color()` 是 `purple`，`footer_model_color()` 是 `accent`、
`footer_cwd_color()` 是 `success`，`shimmer_highlight()` 是 `text`，
`tool_pulse_bright()` 是 `dim`，`toast_error_color()` 是 `error` 混三分之一往
`dim` 去（`docs/ui/toast.md`），等等）：角色是 `theme.rs` 一直就在命名的东西，调
色板是每个主题用来画它们的东西，所以一个新主题就是一张二十个值的表，别的什么都没
有。（曾经是二十一个，直到状态动词离开 codex 的灰、改穿页面标题的渐变，
`docs/ui/status-indicator.md`：它的 `shimmer_base` 角色再没有东西要画。）
Catppuccin 那几档用该档自己的角色（`text`/`subtext1` 作墨色，`overlay1`
/ `overlay2` 给暗色和边框，`surface0`/`surface1` 作底色，`sky` 作 accent，
`blue` 作 link，`mauve` 作 purple），色调则用 Catppuccin 自己的
DiffAdd/DiffDelete 规则 —— 色相混 18% 进 `base` 做成一行，混 42% 做成一个标记
（Solarized 的 `base03` 是那么深的蓝，所以混 25%/50%）。`onedark` 是 TUI 发布时
带的外框，它曾经硬编码的每一个 RGB，只换了代码那一半 —— Atom 的 One Dark而不是代
码块底下原本穿着的 Catppuccin Mocha —— 这样喜欢旧样子的人留得住它
（`one_dark_is_the_original_look_value_for_value`）。这是调色板的承诺，而不是它
后来每一处用法：状态动词的渐变和页脚的颜色，在原外框留白的地方读的是同一批值
（`docs/ui/footer.md`）。

有两个条目行为不同，它们的描述也说明了，因为 picker 的描述行正是用户搞明白一个主
题为什么在他终端上看着不对的地方：

- **`latte`** 是给**浅色**终端的：它的墨色深，底色和色调浅，每一次混合的
  “亮”端都是那个*更深*的颜色，因为在浅底上突出的正是它
  （`the_light_theme_inverts_the_inks_and_the_tints`）。在深色终端上它读起来发
  白 —— 终端的背景不归我们画。
- **`ansi`** **完全不点名 RGB** —— Claude Code 的“只用 ANSI 颜色”模式：
  `Cyan`、`Green`、`Red`、`Yellow`、`Blue`、`Magenta`、`DarkGray`、`Gray`、
  `Reset`，以及 bat 的 `ansi` syntect 主题，它的 scope 解析成调色板*索引*
  （`highlight::convert_syntect_color` 解码 bat 的 alpha 通道编码，那是一条自移
  植以来就一直休眠的路径）。于是 TUI 跟着终端配置的任何调色板走—— 这是唯一一个能
  让 Solarized 终端用户的 TUI 悄悄变成 Solarized 的主题。十六种颜色表达不了的东
  西就不表达：diff 行保留终端底色（只有标记上色，两者都在亮黑上，所以一行的符号
  说明了是哪种），渐变和 shimmer 在两端之间**跳变**而不是混合，
  而 `pulse` spinner 一动不动，它一呼一吸的两端就是同一种亮黑 ——运行中
  的 bullet 仍用那种亮黑闪烁，和它在哪儿都一样，闪烁不需要第二种深浅
  （`docs/ui/tool-pulse.md`）。

这种跳变是那些混合唯一需要的泛化：`ui::wrap::lerp_color` 和 `blend_color`接
受 `Color` 而不是 RGB 三元组，两端都是 `Rgb` 时混合，否则取更近的那一端
（`the_gradient_and_the_blends_follow_the_palette`）。每一个使用者 —— mascot渐
变、sparkle/blocks/gravity/wave 几种 spinner、tool pulse、状态 shimmer ——形状都
不变。错误 toast 柔和下来的红也照同一规则，所以在 `ansi` 之下它保留终端自己
的 `Red`（`docs/ui/toast.md`）。

## 环境中的活动主题

`ui/theme.rs` 过去把每种颜色都存成 `const`；一个能在运行期变的主题做不到。现在颜
色是**访问器函数**（`tool_ok_color()` 就在原来 `TOOL_OK_COLOR` 常量的位置 —— 同
样的名字，小写化，所以每个调用点和每条文档注释都保住了含义），每个都读**活动主题
**的调色板。活动主题是**线程局部**的（`ui::palette`），既不是参数，也不是进程全
局：

- **不是参数**，因为调色板在大约四百个调用点被读，其中大多数在拿不到 `&App`
  的构建器里（`message_lines`、`tool_lines`、`wrap` 的辅助函数、file cell、行
  内 diff）。把一个 `&Palette` 穿进所有这些地方，等于为一个每 session 只变两次的
  值重写 `ui` 里的每一个签名。渲染器在要紧的那个意义上保持纯粹 —— 同样的输入，同
  样的行 —— 而调色板是边界注入一次的输入（`ui::activate_theme`），和时钟、路径显
  示策略被注入的方式一样，而不是由它们穿进来。
- **不是进程全局**，因为测试跑在并行线程上，全局会让一个测试的主题给另一个
  测试的断言重新上色。每个线程的 cell 从默认值开始；一个测试用
  `ui::with_theme(theme, || …)` 在另一个主题下渲染，它划定范围并在退出的路上恢复
  上一个主题 —— panic 时也一样，靠一个 drop guard，所以范围内失败的断言没法给它
  之后的测试重新上色（`with_theme_restores_even_when_the_closure_panics`）。页脚
  那个排队的 memo出于同样的理由已经是按线程保存状态的。

事件循环在一个线程上绘制，所以一个 cell 就是应用状态的全部；`tui::bootstrap`在横
幅构建之前激活保存的主题，而 `tui::theme::Session::select_theme` 在用新主题重绘
的那次重建之前激活新主题。纯的 `App::theme` 记录同一个选择 —— picker
的 ✓ 和 open 时的座位都读它 —— 而两者在边界处保持一致。

**每一个缓存已渲染行的东西都以活动主题为键**，因为一行的颜色是烤进它里面的：Ctrl
+O 的 `TranscriptCache` 的冻结前缀及其签名、Ctrl+D 的 `ContextCache` 的签名，以
及页脚那个排队行的 memo，都带上 `active_theme()`，所以一次切换会在它们下一次读取
时重建，而不是用旧调色板供行（那三个 `…_when_the_theme_changes`测试）。
`StreamRender` 不需要键：每次 purge 重建都会重置它，并用新调色板重新提交进行中
的 partial（不变量 3），而一个 `highlight::Highlighter` 保留它打开时用
的 `CodeTheme`（`a_highlighter_keeps_the_theme_it_opened_with`），这样主题切换时
渲染到一半的块在那次重建把它整块重渲之前仍是一种配色。

### 高亮器

`highlight::Highlighter::new(lang, code)` 显式接受 `CodeTheme` —— `highlight`位
于 `ui` 之下，不读任何环境状态；`ui` 在它的五个构造点传入 `palette().code`，
`plain_style(code)` 也是如此。two-face bundle 整份装在一个 `LazyLock` 里（它
的 `themes.bin` 有 64 KB —— 比一个解析好的 grammar 还小），而每个主题的
`syntect::Highlighter` 在第一次使用时构建进每个 `CodeTheme` 的 `OnceLock`，在进
程的整个生命期里从 bundle 借出它的主题，这正是让一个 `Highlighter` 携带的
`HighlightState` 能持有一个指向它打开时所对高亮器的 `'static` 引用的原因。一
次 `/theme` 浏览会碰到好几个；每个只花一次约 20 KB 主题的解析，once。

## `/theme` picker

在有空间放下列表、一个空隙和未换行的样例的终端上 —— 这套目录需要 71 列 ——预览坐
在列表**旁边**：

```
────────────────────────────────────────────────────────────────────────────────
  Select theme
  The interface's colours — applies in every directory

  ❯

  → mocha       ●●●●● ✓    Catppuccin Mocha
    macchiato   ●●●●●      The darkest flavour, and the default
    frappe      ●●●●●
    latte       ●●●●●      ❯ Rename the greeting in greet.py
    onedark     ●●●●●      ● Edit(greet.py)
    (1/11)                   ⎿  Updated greet.py (+1 -1)
                                 1  def greet(name):
                                 2 -    print("Hello, world")
                                 2 +    print(f"Hello, {name}")
                           ● Done — greet.py greets by name now:

                             greet("Zeroth")  # Hello, Zeroth

  ↑/↓ to navigate · Type to search · Enter to choose · Esc to cancel
────────────────────────────────────────────────────────────────────────────────
```

在更窄的终端上，同一个预览叠在列表之下（这里滚到了目录末尾，窗口跟着选择走）：

```
──────────────────────────────────────────────────────────────
  Select theme
  The interface's colours — applies in every directory

  ❯

    nord        ●●●●●
    gruvbox     ●●●●●
    solarized   ●●●●●
    monokai     ●●●●●
  → ansi        ●●●●●
    (11/11)

  Terminal
  The terminal's own 16 ANSI colours, so the TUI follows its …

  ❯ Rename the greeting in greet.py
  ● Edit(greet.py)
    ⎿  Updated greet.py (+1 -1)
        1  def greet(name):
        2 -    print("Hello, world")
        2 +    print(f"Hello, {name}")
  ● Done — greet.py greets by name now:

    greet("Zeroth")  # Hello, Zeroth

  ↑/↓ to navigate · Type to search · Enter to choose · Esc to…
──────────────────────────────────────────────────────────────
```

版式是 codex 的主题 picker（`codex-rs/tui/src/theme_picker.rs` —— 列表在左，高亮
主题的样例在右，终端窄时叠起来），穿这个 TUI 的衣服：picker 家族的那个页面
（`docs/ui/view-flow.md` 的*一个 picker 的页面*）—— 一个 `Select theme` 标题，下
面一行暗色说明主题在每个目录都生效（每个用户一个 `theme.json`，而 `/mascot`
和 `/spinner` 是按目录保存的），内联框和 `❯` 打字即搜（匹配名字、**标题**和描
述，所以 `catppuccin` 找到那四档，`follows` 找到终端主题）压在列表之上，还有一条
暗色提示，和 `/mcp` 列表的一样，由导航引导。然后是一个主题 picker 之所以存在的那
些东西：

- **列表就是这个家族的列表。** 选中的行是那个缩进、`accent` 的 `→`
  （`MODEL_MARKER`，坐在搜索行的 `❯` 之下）和加粗的名字；其他每一行保留标记宽度
  的空白，所以名字在它底下一一对齐。它曾经是 `/mcp` 列表的那个 `❯`——在一个搜索行
  已经是 `❯` 的页面上再来第二个。十行的窗口（`THEME_MENU_MAX_ROWS`，其他带预览
  的 picker 只显示五行）把选择保持在中间行（`centered_window`），而它下面家族
  的 `(n/total)` 计数器说明它站在哪儿，而在过去，
  是 `↑ 1 more above` / `↓ 1 more below` 这一行点名它藏起来的那一个主题。十行加
  上计数器，高度大约等于旁边那个预览，所以显示几乎整个目录不花并排页面的任何高度
  （21 行）；叠起来时，预览从计数器下面开始，那使那一页变成33 行。所以列表那一列
  在每个选择下都是同一个高度，框在 ↑/↓ 走过它时纹丝不动—— `/settings` 的总是输出
  规则（`the_page_height_is_stable_across_selections`）。
- **每一行都穿自己的色块。** 名字列之后（按**整个目录**里最宽的名字定尺寸，
  再加 `THEME_MENU_GAP`，绝不是只按可见行来定，所以一次搜索或一次滚动既不移动色
  块、也不移动旁边的预览），每一行带五个 `●`，用的是*那个主题的* accent、link、
  success、warning 和 error，直接从它自己的调色板读出，绝不是活动主题的，所以整
  个目录一眼可比，就像 `/spinner` 各行上活着的 spinner 一样。session 当前的主题
  带 `/model` picker 那个绿色的 ✓，而把它打开时高亮就停在它上面。
- **高亮的主题用真实的 cell 预览。** 预览是一段三个 cell 的样例对话 —— 一个
  用户气泡、一个 `Edit` diff cell、一段带行内代码和一行围栏代码的助手回复 ——
  由 `message_lines` 和 `tool_lines` 构建，也就是对话*自己的*构建器，高亮的主题
  在它们周围被划为活动（`with_theme`）。所以一眼就看得见气泡的底色与墨色、
  success bullet、代码主题（diff 的上下文行和回复的代码行都带语法高亮）、diff 色
  调和行内 diff 标记，以及行内代码上的 accent —— 而 picker 展示的和Enter 产出的
  永远不会互相矛盾，因为它们就是同样的行。diff cell 的输出是执行器自己
  的 `llm::tools::update_report` 字符串，所以它解析和上色的方式与一次活的
  `edit` 完全一样。范围在它周围那一页构建之前就结束：框、各行、说明和提示保留**
  活动**主题，只有样例穿高亮的那个
  （`the_preview_wears_the_highlighted_theme_and_the_frame_the_active_one`）。
- **一行说明点名样例穿的主题。** 它是 codex 的表头 —— 一个加粗标题压着一行暗色
  副标题 —— 压在预览之上：主题的标题（`Catppuccin Mocha`），它下面是描述的其余部
  分，去掉标题与那个破折号、首字母大写
  （`The darkest flavour, and the default`）。两行而不是旧的单
  行 `Title — description`，因为那一行在标准 80 列终端的预览列里，十一个主题里有
  五个被 `…` 截断，默认主题也在其中；拆开之后，除了终端主题那条长副标题，每条说
  明在 80 列下都读得完整，在 100 列下全都完整
  （`a_standard_terminal_shows_every_caption_whole`）。每一行仍是被 `…`截断而不
  是换行，所以预览对每个主题都是一个形状。
- **宽终端上在列表旁边，窄终端上在它下面。** 一旦终端有空间放下列表列（目录
  能画出的最宽行，23 列）、`THEME_PREVIEW_GAP`（4 —— codex 的侧边空隙加上它的面
  板内缩）和 `THEME_PREVIEW_MIN_COLS`（44 —— codex 的`WIDE_PREVIEW_MIN_WIDTH`，
  样例不换行所需的空间），预览就坐在列表旁边：71 列，所以标准 80 列终端就能拿
  到。在那之下，预览隔一个空隙叠在列表下面，内缩两列。只有一个断点，而样例在它之
  后每个宽度里都放得下不换行，所以并排页面无论怎么布局都是同一个高度
  （`the_layout_switches_once_and_beside_the_list_the_sample_never_wraps`）。列
  表列是目录的一个常量，所以 ↑/↓ 和搜索都不让预览左右移动
  （`the_preview_column_holds_still_as_the_list_moves_and_filters`），而列表旁边
  时计数器住在列表列里，所以预览的行也从不上下移动。在列表旁边，预览是更高的那一
  列，所以那个短窗口在那里不花页面的高度。
- **气泡的底色留在它自己那一列。** 在对话里，用户气泡的深色底是一个*行*样式，
  ratatui 会把它抹到这一行上的每一个 span 上 —— 摆在列表旁边，它就会把共享那一行
  的主题行一起涂上。所以预览的每一行在就位之前，都先把自己的行样式折到自己
  的 span 上（`fold_line_style`），而合成后的那一行不带自己的行样式
  （`the_bubble_ground_stays_in_the_preview_column`）。
- **一套版式，三个 picker。** `/mascot`（`docs/ui/mascot.md`）和 `/spinner`
  （`docs/ui/spinner.md`）的 picker 用同样的方式摆它们的预览，经过
  `ui::model_view` 里同样的那些部件：`picker_header`（标题压着它的副标题）、
  `marked_row_head`（家族那一行的缩进、标记和名字）、`list_counter_line`（窗口之
  下的计数器）、`caption_lines`（加粗标题压着暗色副标题）和`PreviewLayout`（列表
  列、空隙和预览最窄的列，决定并排还是叠放；它的 `body`折叠每一行预览的行样
  式）。每个 picker 只提供自己的行、自己的预览和自己的三个宽度。

样例对每个主题是同一个形状，所以页面高度从不取决于选择 —— 而页面是**静止**的（上
面没有任何东西在跳），所以和 `/spinner` picker 不同，它不要动画帧，它的
scrollback 流是按它的**行**签名的，像 `/mascot` picker 一样：任何一个按键都让它
重排（`docs/ui/view-flow.md`）。并排时页面是二十一行，叠放时是二十八行，所以它不
用流动就能装进一个 24 行的终端 —— 框的两条边都不补空白，标题紧跟在上边之下，提示
紧贴在下边之上（`docs/ui/view-flow.md` 的*框贴着它的内容*）。

### 一个放不下这一页的终端

比整页更短的终端拿到一个**紧凑**页 —— codex 的短面板，它的
`selection_picker_layout.rs` 把规则写在了一行里：*“可选的空隙和预览先让位”*：

```
──────────────────────────────────────────────────────────
  Select theme
  The interface's colours — applies in every directory

  ❯
  ↑ 4 more above
    onedark     ●●●●●
  → dracula     ●●●●●
    nord        ●●●●●
  ↓ 4 more below
  ↑/↓ to navigate · Type to search · Enter to choose · Es…
──────────────────────────────────────────────────────────
```

放得下时的表头，然后是搜索行、`COMPACT_LIST_ROWS`（3，整个家族的紧凑窗口）行列
表 —— codex 的最小结果行数 —— 和提示留下；预览、它的说明和计数器让位，再短一行则
是表头跟着让位。在这之前，页面在短终端上像每个带边框视图一样贴着底部，那会把预览
和提示留在屏幕上，而把搜索行和主题名流转进 scrollback：无论picker 被挤到哪儿，屏
幕上都不剩什么可以挑的（`a_short_terminal_keeps_the_theme_choices_on_screen`）。
这里那些溢出的行是codex 预留的箭头槽 —— `↑` 就在搜索行之下，`↓` 就在提示之上，那
一侧没有东西被藏起来时每个都是空白 —— 因为十一主题上的三行窗口在列表中间会**两侧
**都藏行,而忽来忽去的行会随 ↑/↓ 移动框。预留之后，页面在每个选择下都是同一个高度
（`the_compact_page_holds_still_across_selections`），而空着的槽正是页面本来就想
要的空隙。

这个选择在页面构建器手里：`theme_view_lines` 拿到页面可用的行数，放得下就构建整
页，放不下就构建紧凑页（`the_whole_page_returns_once_it_fits`）。那些行是终端
的 —— 一个替换 composer 的视图保住它的行，而它上面的流式条带让位
（`view_split`）—— 而这一页的每个读者要么传那个高度，要么传已经为它预留的区域，
那个区域至少是所选页面的高度、最多是终端的高度，所以预留的高度、绘制、光标座位和
流动永远构建同一页。

在更短的终端上，紧凑页一次一行地舍掉剩下的东西，而从不舍它的搜索行
（`docs/ui/view-flow.md` 的*紧凑页之下*）：`↑`/`↓` 那一对、上边框、一直到高亮行
的那些行、下边框和提示。若改成贴底，紧凑页会把它的搜索行流转进scrollback —— 光标
夹在一个主题行上 —— 然后高亮主题也流走。现在七行就读得完，两条边框都还在：

```
──────────────────────────────────────────────────────────
  ❯
    onedark     ●●●●●
  → dracula     ●●●●●
    nord        ●●●●●
  ↑/↓ to navigate · Type to search · Enter to choose · Es…
──────────────────────────────────────────────────────────
```

于是页面在每个高度都放得下，它没有任何一部分流动，而光标坐在搜索行上，是在构建出
的页面里找到的，而不是钉在第 1 行。

一次**什么都没有**匹配到的搜索收拢到它的要点 —— 占位符（`No matching themes`）、
一个空行空隙、提示 —— 这是 `/mascot` picker 的规则，来自 `/model` picker 的
`model_has_detail` —— 并以同样的方式舍去，把打进去的内容留在占位符之上。

按键：↑/↓ 移动，**到两端会绕回**（共用的 `wrap_step` 语法）；
PageUp/PageDown/Home/End 跳转（夹取）；可打印键过滤（Backspace 弹出，一次击键把
高亮重新落到第一个匹配上）；**Enter 或 Space 选择并关闭**（所以空格永不到达搜
索 —— 每个查询一个词）；Esc 先清查询、然后关闭；Ctrl+C 关闭。picker 打开时拥有每
一个键（在 `App::on_key` 顶部路由，早于 composer 全局的Ctrl+C/Ctrl+O),粘贴被吞掉
（谁粘贴的什么东西都不是主题名），而运行中 cell 的
`(ctrl+b to run in background)` 提示在它开着时被清空
（`App::background_hint_elapsed`，每个替换 composer 的 picker 都遵循这条规则）。
它像 `/mascot` 一样能**在回合中途**工作：它只替换 composer，流式条带在它上面保住
自己的行，而一次切换从不碰运行中的回合 —— purge 重建用新颜色重新提交进行中
的 partial。在放不下这一页的终端上，紧凑页接手（见上）。

## 应用一次选择

`Action::SelectTheme(theme)` —— 纯的那一侧已经把 `App::theme` 移好、把 picker关
掉了；边界（`tui::theme::Session::select_theme`）：

1. **持久化** `{config_home}/theme.json`（`{"theme": "mocha"}` —— 纯格式是
   `app::theme_file_json`/`parse_theme_file`；I/O 在`tui::config::save_theme` 里
   尽力而为，`save_spinner` 的姿态），
2. **激活**调色板（`ui::activate_theme`），让每个渲染器从此刻起读到它，
3. 弹出一个确认用的 `Theme: {name}` toast，并且
4. **purge 重建**（`repaint_active_view`）：每个已提交行的颜色都烤进了
   scrollback，所以给对话重新上色的唯一办法就是从历史做标准重建 —— 横幅、每
   个 cell、进行中的 partial —— 在刚变成环境的那套调色板之下，toast 骑在重建后的
   帧上。transcript 和上下文的缓存在它们下一次读取时重建（以主题为键，见上）。切
   换立刻可见，和一次 resize 的重绘完全一样；`smoke.sh` 的Phase 111 从原
   始 SGR 字节读出横幅的 `/login` 提示，切换前后各读一次，看着Mocha 的天蓝变
   成 Dracula 的青。

启动时 `tui::bootstrap` 从 `theme.json` 播种 `App::theme` 并在**横幅构建之前**激
活它（缺失或损坏的文件保留默认值 —— 绝不是启动失败），所以第一帧就已经穿着保存的
主题。

## 设计说明

- **为什么默认是 Catppuccin Mocha，而不是旧样子？** 代码块自 syntect 移植以来
  一直是 Catppuccin Mocha，而周围的外框仍是 One Dark 的青色 —— 一帧里两套调色
  板。一个默认值自己就不协调的主题系统，会是对它自己的糟糕广告。旧外框以
  `onedark` 活下来，如今配着 One Dark 的代码，所以那个选择只有一行之遥。
- **为什么是固定目录而不是一个调色板文件？** 一致性，见上：一个没有相称
  syntect 主题的调色板，会把外框涂成一种样子、代码涂成另一种，而那正是这个功能存
  在要终结的东西。这十一个覆盖了 two-face bundle 带的那些家族；加一个就是一
  个 `Theme` 变体、一张 `Palette` 表和一次 `CodeTheme` 映射。
- **为什么是访问器函数而不是一个 `Palette` 参数？** 拿不到 `&App` 的构建器里
  有四百个调用点；见*环境中的活动主题*。那些 `const` 的名字变成了同名函数，所以
  没有一个调用点改变含义，只改变了拼写。
- **为什么是线程局部而不是 `static`？** 测试隔离；见上。应用在一个线程上绘制，
  所以什么也没丢。
- **为什么预览渲染真实 cell 而不是色块？** 色块说明颜色*是什么*；一个 cell
  说明它们放在一起*看起来如何* —— 行内代码上的 accent 挨着暗色的装订线、语法颜色
  之下的 diff 色调。而通过真实构建器渲染是 picker 家族的规则：`/mascot` 的预览是
  横幅自己的构建器，`/spinner` 的预览是状态行的。
- **为什么是 codex 的版式，而不是 codex 的每一个细节？** 列表旁边的预览是
  codex 的，因为列表下面的预览会让页面和两者一样高 —— 二十八行，超过一个24 行终
  端能显示的，于是页面顶部流转进了 scrollback —— 而并排时两者在二十一行里一眼读
  完。其余的部分留在这个 TUI 自己这边。`→` 和 `(n/total)` 计数器是picker 家族
  的，而不是 codex 的全宽填充上的 `›` 和裸箭头，所以一个主题列表看起来像应用的其
  他列表。列表列按目录定尺寸，而不是拿走 codex 的一半宽度：codex 带底色的面板把
  半个屏幕花在一个只有四分之一宽的列表上，而它的分栏在每个窄于 94 列的终端上都会
  把预览叠起来。色块和 ✓ 留下，因为 codex 的名字不带颜色，而我们的能一眼比较调色
  板。没有标题：`/theme` 保留家族的 `❯` 过滤框，那个框没有标题，而预览之上的说明
  是 codex 的表头挪到了它点名所展示内容的地方。而 ↑/↓ 不会实时切换主题：codex 每
  次击键都给整个屏幕换主题并在 Esc 时恢复，但这里一次切换是整段对话的 purge 重
  建，所以预览穿高亮的主题，由 Enter来应用它。
- **为什么 ANSI 主题在目录里？** 它是唯一一个跟随*终端*主题的主题，而对一个
  已经给终端上了主题的用户来说，那是唯一要紧的主题。代价是让混合在一个非 RGB的端
  点上跳变，那只是几行代码。
- **为什么 spinner 的帧和 mascot 的画不按主题上色？** 它们是字形，不是颜色；
  主题给它们上色（渐变和 pulse 来自调色板），而 `/mascot` 和 `/spinner` 仍然是给
  字形的 picker。
- **为什么页面标题穿横幅的渐变？** 每一个有标题的、替换 composer 的页面 ——
  `/hooks`、`/mcp`、`/skills`、启动复核、`/export`、↓ 管理器、`/login` 的带标题
  页面、`/donate`、`/secrets` —— 用同一身衣服做标题，加粗，accent → link的漫染横
  跨标题自己的宽度（`ui::header::page_title_line`，压在`page_title_spans` 上，
  `/donate` 在它的爱心旁边用的是同一个）。纯 accent 是页面们共享的一种颜色；渐变
  是横幅自己的样子，所以一个页面读起来像是它打开的那个应用的一部分，并像横幅那样
  跟随 `/theme`。`/skills` 是唯一一个带标题的`❯` 过滤 picker（它的表头数
  着 skill，`docs/tools/skills.md`）；其他以 `❯`过滤打开的（`/model`、
  `/settings`、`/theme`、`/mascot`、`/spinner`）没有标题可穿，而全屏视图
  （`/resume`、Ctrl+O、Ctrl+D）保留 pager 的斜线铺砌表头。

## API

- `app::Theme` —— 目录（`ALL`、`name`、`title`、`description`、`is_light`、
  `from_name`），`Default` = `Mocha`。
- `app::ThemePicker` / `ThemeRow` —— 打开的 picker 的状态和一条派生出的行；
  `App::theme()`、`set_theme`、`open_theme_picker`、`close_theme_picker`、
  `theme_rows`、`highlighted_theme`、`on_key_theme_picker`。
- `app::theme_file_json` / `parse_theme_file` —— `theme.json` 格式。
- `highlight::CodeTheme` —— syntect 那一侧（`ALL`、`name`）；
  `Highlighter::new(lang, code)`、`highlight(lines, lang, code)`、
  `plain_style(code)`。
- `ui::activate_theme` / `active_theme` / `with_theme` —— 环境中的活动主题；
  `ui::palette::Palette` 和 `palette_of(theme)`（crate 内），以及`ui::theme` 里
  的那些访问器函数。
- `ui::wrap::{lerp_color, blend_color}` —— 基于 `Color` 的那些混合。
- `ui::theme_view` —— `theme_picker_height`、`render_theme_picker`，以及
  crate 内的页面构建器 `theme_view_lines(app, width, height)`（它的长度就是预留
  的高度，`docs/ui/view-flow.md`），它把预览摆在列表旁边或下面
  （`model_view::PreviewLayout`，压在 `THEME_PREVIEW_GAP` 和
  `THEME_PREVIEW_MIN_COLS` 上，`/mascot` 和 `/spinner` 共用这套版式），并在整页
  比 `height` 更高时构建紧凑页（`model_view::compact_page` ——
  `COMPACT_LIST_ROWS` 行夹在溢出槽之间，以及它下面一直到搜索行的那些梯级）
  （`first_page_that_fits`）。
- `tui::theme::Session::select_theme`、`tui::config::{theme_json_path,
  load_theme, save_theme}`。

## 测试

- `app/tests/theme.rs` —— 目录（十一个主题，mocha 在最前且是默认，各不相同的
  裸词名，标题开启描述，浅色与终端主题说明自己是那样，`from_name` 往返），持久化
  格式，`/theme` 命令打开 picker（排在 `/mascot` 之前），不请求动画，Ctrl+B 提示
  被清空，以及整套按键语法（对名字/标题/描述的过滤，绕回的 ↑/↓对夹取的跳转键，
  Enter/Space 选择，Esc/Ctrl+C，拥有每个键，回合中途使用且不碰回合）。
- `ui/tests/palette.rs` —— 每个线程上的默认值，`with_theme` 的范围及其 panic
  安全性，`activate_theme`，外框/代码的配对，每个主题不同的语义色相，One Dark一
  个值都不差，ANSI 主题不点名任何 RGB 及其跳变的混合，浅色主题反过来的墨色和色
  调，渐变/混合的派生，每个主题里错误 toast 柔化的红，以及一个穿活动主题的渲染出
  的 cell。
- `ui/tests/theme_view.rs` —— 带边框的页面（边框、标题压在副标题上、搜索、
  家族 `→` 之下的十行、计数器、预览、提示）在宽和窄两个宽度上，标记与缩进，计数
  器说明窗口站在哪儿，预览在某个宽度上挨着列表（包括一个 80 列终端）而在窄终端上
  隔一个空隙叠在它下面，唯一的那个断点之后不换行，预览列经过 ↑/↓ 和搜索纹丝不
  动，说明的加粗标题压着它的暗色副标题、且每条说明在 100 列下完整，预览的真
  实 cell，每一行自己的色块，预览处在高亮主题而框处在活动主题（bullet、代码关键
  字、diff 色调、气泡底色），气泡底色被留在预览列，稳定的高度，并排页面更矮，活
  动的 ✓ 和它的座位，无匹配的收拢，没有叠放的空白，宽度的安全，高度的契约；在短
  终端上紧凑页（codex 的形状，主题选择留在屏幕上，窗口把选择保持在自己的中间，每
  个选择下同一个高度，放得下时整页回来，宽度与空行的安全，光标在它的搜索行上）；
  在比紧凑页更短的终端上那些梯级（搜索行、高亮主题和提示从三十行一路留到一行，七
  行读作搜索行、三个主题、提示、空隙、边框，光标在每个高度都在搜索行上，无匹配时
  打进去的内容被保留），任何高度下都不流动，渲染填满预留的行，以及一个比紧凑页更
  矮的区域把一级梯级完整画出。
- `ui/tests/transcript.rs`、`context_view.rs`、`footer.rs` —— 三个缓存在主题
  改变时重建。
- `highlight` —— 每个代码主题都能解析，两个主题在一个关键字上不一致，ANSI
  主题的调色板颜色，一个高亮器保留它的主题。
- `scripts/smoke.sh` Phase 111 —— picker 在真实终端里端到端：默认 accent 下的
  横幅提示（原始 SGR），调色板条目，打开页面带着它的标题、`(1/11)` 计数器之上五
  个带色块的行、以及它在 90 列下**挨着列表**的真实 cell 预览，↓ 移动标记和说明，
  一次**调整到 64 列把预览叠在**列表之下、再回到 90 列又把它摆回旁边，一次**调整
  到 14 行**显示紧凑页（标题、搜索行、三行主题、`↓ 8 more below`、提示、没有预
  览），一次过滤后的 Enter 切换主题、带一个toast 和一次 `theme.json` 写入，**已
  经画出的横幅被重建重新上色**，以及**第二个进程对着同一个 config home 以保存的
  主题启动**。
