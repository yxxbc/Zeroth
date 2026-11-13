# 状态 spinner 的样式与 `/spinner` picker

实时状态行以一个 **spinner** 开头 —— 就是 shimmer 动词之前那个（些）动画字形
（`⣤⣀⣀⣀⣀⣀⣀⣀ Working… (3s · ↓ 1.2k tokens · esc to interrupt)`，
`docs/ui/status-indicator.md`）。它过去只有一种动画，comet。**`/spinner`** 命令
—— 第九个替换 composer 的内联 picker，`/mascot` picker 的孪生 —— 在九种之间挑
选，**实时**预览它们，并把选择跨 session 持久化 —— **按工作目录**，即
`/mascot` picker 的规则（`docs/state/per-directory-state.md`）。**默认是
`gravity`**，那个沿点字轨道跳动的球。comet 仍是目录的第一行，而 `(●•·   )` 在
这些文档里凡是画示意图的地方仍代表 comet —— 它只是不再是没有任何已存选择的
session 打开时的那一个。

## 目录

| 名字 | 外观 | 节奏 |
| --------- | -------------------------------------------------------- | ------- |
| `comet` | `(●•·   )` —— 暗色墙之间的 Larson 扫描扫过 | 80 ms |
| `gravity` | `⣤⣀⣀⣀⣀⣀⣀⣀` —— 一个球沿点字轨道跳动、撞两侧墙反弹，accent → link（**默认**） | 2.4 s 一趟，0.6 s 一跳 |
| `wave` | 八个点字单元格 —— 一道波沿轨道滚下并从墙上反射，浸在横幅的 wash 里 | 3.4 s 去与回 |
| `sparkle` | `· ✢ ✳ ✶ ✻ ✽ ✻ ✶ ✳ ✢` —— 一点火花绽成一颗星，accent → link | 120 ms |
| `dots` | `⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏` —— 经典点字 spinner | 80 ms |
| `blocks` | `▙ ▛ ▜ ▟` —— mascot 们的象限字形在转，accent → link | 150 ms |
| `pulse` | `●` —— 一个点在呼吸，dim → white，节奏同运行中 tool 的圆点 | 1 s 一次呼吸 |
| `bars` | `▁ ▂ ▃ ▄ ▅ ▆ ▇ █ ▇ …` —— 一个电平表，随高度变亮 | 60 ms |
| `line` | `\| / - \` —— 经典 ASCII spinner，任何字体都行 | 100 ms |

目录的**身份** —— 名字、顺序、描述，以及 `spinner.json` 读回时经的
`from_name` 往返 —— 是纯的 `app::Spinner` 枚举（`Default` = `Gravity`）。列表
的*顺序*与默认值是两回事 —— picker 把高亮坐在**当前活动**的样式上，无论它在哪
一行，所以目录可以始终以 comet 开头，而一个 session 以球开场。它的**外观**
—— 每种样式的帧、节奏与颜色规则 —— 是样式，所以它住在 `ui/theme.rs` 里，与
其他每一个样式决定并列（`SPINNER_*_FRAMES` / `_INTERVAL`、轨道几何、pulse 与
bars 的颜色端点），而 `ui::status::spinner_spans(spinner, elapsed)` 把两者映
射起来。每种样式守两条规则，由 `ui/tests/status.rs` 钉住：一种样式的每一帧都
**等宽**（这样它后面的动词绝不抖动 —— comet 自己的规则，
`docs/ui/status-indicator.md`），而且每个字形都是**单宽**（一个宽字形会把动词
与它后面的指标剪切掉 —— `docs/ui/table-streaming.md` *Wide glyphs*）。

### 两条点字轨道

`gravity` 与 `wave` 没有帧表。它们各自画在一条**点字轨道**
（`ui::status::Track`）上：八个单元格 —— comet 的占位，所以三种宽样式共用一种
宽度 —— 每格 2 × 4 个点，在单个文本行里是十六个点列乘四个点行。正是这个分辨
率让一个球肉眼可见地*跳*、一道波肉眼可见地*滚*，而字形表只能一格一格地走。一个
单元格就是一个字形，因而是一种颜色：在有色东西落到它上面之前是 dim
（`status_detail_color()`）。这对样式是某个弹跳指示器实验脚本里两段点字动画的移
植 —— 它的画布、它的 `tri` 乒乓与它的 `arc` 跳跃 —— 为固定的八格轨道与本
crate 的调色盘重新调过。

- **`gravity`** 沿最下面那排点铺一条地板，并让一个 2 × 2 点的球在地板上跳。水平
  方向上球以恒定速度**乒乓**、在每面墙处硬反转（`ui::wrap::ping_pong`，每
  `SPINNER_GRAVITY_SWEEP` 一趟往返）；垂直方向上它走一条抛物线
  （`ui::wrap::hop`，起跳与落地为 0，最高点 1，每 `SPINNER_GRAVITY_HOP` 一
  跳）。两个周期是 2.4 s 与 0.6 s —— **一趟四次跳** —— 所以球正好在碰到每面墙
  时落地，而脚本的 2.6 s / 0.66 s 会让反弹相对墙漂移。静止时球的底排与地板共
  用一行，所以读起来是落地而不是悬着；在最高点时它整块坐在最上面两排点里。球按
  它在轨道上的位置穿横幅渐变 —— 左墙青色，右墙蓝色 —— 它所在的单元格取它的
  色。
- **`wave`** 在每个点列上放一个点，落在一条波长为整条轨道的正弦上
  （`SPINNER_WAVE_LENGTH`，所以永远有一个峰与一个谷在视野里），量化到四排点。
  它的*相位*是乒乓的：每 `SPINNER_WAVE_SWEEP` 前进 `SPINNER_WAVE_TRAVEL`
  （3）个波长再原样回来，于是波向右滚、从墙上反射、再滚回来。因为行程是整数个
  波长，反转时的帧*就是*它出发时的帧 —— 没有接缝。每个单元格按它在轨道上的位置
  穿横幅渐变：mascot 自己的 wash，在滚动。

两条曲线都用**整毫秒**计算（`ping_pong` 把 elapsed 折到*朝向*远墙的那一程上；
`hop` 先乘整数再除），所以回程逐比特重走去程：`T − d` 时的帧等于 `T + d` 时的
帧，这才是让反射无缝而不只是近似无缝的原因。`ping_pong` 就是 comet 的 Larson
扫过的连续值形式。

### 颜色

每种样式都以动词之前那个分隔空格结束它的 span，所以无论样式多宽，动词的
shimmer 都从同样的距离开始；comet 保留它逐单元格的八个 span（白色粗体头、中灰
`•`、其余 dim），两条轨道每格一个 span（不加粗 —— 点字点已经很密了，合成的粗体
会把它们糊在一起），而其他每种样式都是**一个字形一个 span**，加粗，由
`glyph_color` 上色：

- `sparkle`、`blocks`、`gravity` 与 `wave` 走**横幅的渐变**
  （`header_gradient_start()` → `header_gradient_end()`，主题的 accent →
  link，`docs/ui/header.md`、`docs/ui/theme.md`）—— 火花按它的绽放层级
  （`·` 处 accent，`✽` 处 link，然后沿淡出回来），方块按它的转角，球按它的位
  置，波按每格在轨道上的位置 —— 于是主题的 accent 骑在状态行上；
- `pulse` 呼吸一条余弦钟形（`ui::wrap::breath`），从调色盘的 `pulse_dim`
  （`tool_pulse_dim()`）一路升到白 —— 即运行中 tool 的圆点在开始闪烁之前曾有的
  那个起伏（`docs/ui/tool-pulse.md`），留在这里是因为状态行的头部必须被*读
  到*，而一个单元格只需要被注意到 —— 它与那个圆点共用周期，所以呼吸的状态行与
  闪烁的 tool 单元格步调一致；
- `bars` 随高度变亮，从 tool pulse 的亮灰在 `▁` 处到白在 `█` 处；
- `dots` 与 `line` 穿 comet 头的白。

## `/spinner` picker

在一个放得下列表、一个间隔与示例状态行的终端上 —— 对这份目录是 72 列 —— 预览
坐在列表**旁边**，即 `/theme` picker 的布局（`docs/ui/theme.md`）：

```
────────────────────────────────────────────────────────────────────────────────
  Select spinner
  The status line's animation — saved for this directory

  ❯

    comet     (●•·   )      Gravity
  → gravity   ⣤⣀⣀⣀⣀⣀⣀⣀ ✓    A ball hopping along the track, wall to wall
    wave      ⠔⠉⠉⠑⠤⣀⣀⡠
    sparkle   ·             ⣤⣀⣀⣀⣀⣀⣀⣀ Working… (0s · esc to interrupt)
    dots      ⠋
    (2/9)

  ↑/↓ to navigate · Type to search · Enter to choose · Esc to cancel
────────────────────────────────────────────────────────────────────────────────
```

在更窄的终端上，同一个预览叠在列表下方：

```
────────────────────────────────────────────────────────────
  Select spinner
  The status line's animation — saved for this directory

  ❯

    comet     (●•·   )
  → gravity   ⣤⣀⣀⣀⣀⣀⣀⣀ ✓
    wave      ⠔⠉⠉⠑⠤⣀⣀⡠
    sparkle   ·
    dots      ⠋
    (2/9)

  Gravity
  A ball hopping along the track, wall to wall

  ⣤⣀⣀⣀⣀⣀⣀⣀ Working… (0s · esc to interrupt)

  ↑/↓ to navigate · Type to search · Enter to choose · Esc …
────────────────────────────────────────────────────────────
```

刻意做成 `/theme` picker 的布局 —— picker 家族的页面
（`docs/ui/view-flow.md` *A picker's page*）：一个 `Select spinner` 标题压在
一行暗色说明上，说明这个选择是为本目录保存的
（`docs/state/per-directory-state.md`），同样的内联外框与 `❯` 打字即搜索（匹配
名字*与*描述，所以 `braille` 能找到 `dots`），各行带着家族的 `→`、选中项在
`❯` 之下是粗体名字、其余每一行缩进在它下面，高亮样式周围一个五行窗口压在
`(n/total)` 计数器之上 —— 在那里 `↑`/`↓` 会让行来了又去，并随选择走过九种样式
而移动外框 —— `/theme` 那种说明压在预览之上（样式的名字作为粗体标题压在它那行
暗色的描述之上），以及一段由导航打头的提示 —— 外加一个 spinner picker 存在的
理由：这一页是**实时的**。

- **每一行都穿自己的 spinner。** 名字列按**整个目录**里最宽的名字加上
  `SPINNER_MENU_GAP` 定宽 —— 绝不是只按列出的那些行，所以一次搜索既不移动
  spinner 也不移动旁边的预览 —— 之后每行经状态行用的同一个 `spinner_spans` 画出
  它样式的帧，于是九种样式一眼可比，全都同时在转。session 当前的样式在它的
  spinner 之后带 `/model` picker 那个绿色 `✓`，而打开时高亮就坐在它上面。
- **高亮的样式作为一整条状态行预览** —— 一个示例 `TurnStatus`（第一轮的：第一个
  状态动词、还没有 token，动词像真实回合那样每 30 秒换一个 ——
  `docs/ui/status-indicator.md`）经 `ui::styled_status_line`，即带的状
  态行用的*同一个*渲染器，所以 picker 显示的东西与回合显示的东西绝不会不一致。
  它的 elapsed 是从**打开那一刻**起量的帧时钟（`SpinnerPicker::opened_at`、
  `App::spinner_preview_elapsed`），所以页面出现时这一行读作 `0s` 并随用户浏览
  往上数 —— 是一个刚刚开始的回合，而不是一个自启动以来一直在走的时钟 —— 而
  spinner 的运动与动词的 shimmer 正是从同一个值取相位，与真实回合完全一样。
- **宽终端上在列表旁边，窄终端上在它下方。** 一旦终端有空间放下列表那一列
  （目录能画出的最宽一行，24 列，因为 comet 与两条轨道都是八格宽），再加上
  `SPINNER_PREVIEW_GAP`（4，即 `/theme` 的间隔），然后是
  `SPINNER_PREVIEW_MIN_COLS`（44，即 `/theme` 示例的宽度：放得下示例行并让它
  的时钟有分钟位），预览就坐在列表旁边。那是 72 列，所以标准的 80 列终端拿得到
  它，预览列 52 列。每条描述在那里都短到能整段读完：它们头一次移到列表旁边时
  有三条在 80 列下被截断，所以被缩短了
  （`a_standard_terminal_shows_every_caption_whole`）。在那之下，预览隔一个间
  隔叠在列表下方，缩进两列。只有一个断点，示例在断点处整段读得出。列表列是目录
  的常量，所以 ↑/↓、搜索、走一格都不会把预览横着挪动。布局是
  `ui::model_view::PreviewLayout`，`/theme` 与 `/mascot` 摆它们预览用的同一个。
- **没有回合在跑它也在动。** picker 打开时 `App::wants_animation_frames` 为真，
  于是绘制 tick 像活跃回合或 ↓ 管理器带那样重新挂上 32 ms 的时钟链（codex 的
  状态小部件节奏），而它用来动画的帧时钟正是边界每次绘制都已注入的那个
  （`App::set_pulse`，`docs/ui/tool-pulse.md`）。picker 关闭后的第一次绘制，这
  条链自己就停了。
- **页面在动画期间绝不移动。** 各样式宽度不同（comet 与两条轨道是八格，其余一
  格），但每种样式正好一行，所以选择与时钟都不改变页高 —— `/settings` 那条"始
  终发出"的规则，由
  `the_page_height_is_stable_across_selections_and_ticks` 钉住。并排摆时这一
  页十五行，叠起来的是二十行。

一条**什么都匹配不到**的搜索会塌成它的要点 —— 占位符
（`No matching spinners`）、一个空行间隔、提示 —— 因为没有列表、没有说明、也
没有预览可显示；即 `/mascot` picker 的规则，源自 `/model` picker 的
`model_has_detail`。

按键：↑/↓ 移动，**在两端回绕**（共享的 `wrap_step` 语法）；
PageUp/PageDown/Home/End 跳转（夹取）；可打印键过滤（Backspace 弹出，一次按键
把高亮重新落到第一个匹配上）；**Enter 或空格选中并关闭**；Esc 先清查询、再关
闭；Ctrl+C 关闭。打开期间 picker 拥有每一个键（在 `App::on_key` 顶部路由，早于
composer 的全局 Ctrl+C/Ctrl+O），粘贴被吞掉（谁粘的什么都不可能是样式名），而
运行中单元格那句 `(ctrl+b to run in background)` 提示在它打开时被清空
（`App::background_hint_elapsed`，每个替换 composer 的 picker 都遵循的规则 ——
`/mascot` 与它一起加入这个名单）。它像 `/mascot` 一样**回合中途**也能用：它只
替换 composer，而流式带在它上方保留自己的行 —— 对这个 picker 来说这正是重点，
因为带的那个状态行就是切换所改变的东西，就在下一帧。

在矮到放不下这一页（列表旁边十五行，叠起来二十行）的终端上，picker 改构家族的
**紧凑**页面（`docs/ui/view-flow.md` *A page that sheds rows before it
flows*）：
实时预览让位，说明与状态行一起，三种样式坐在预留的溢出槽之间，每种仍穿自己的
实时 spinner —— 页头之下还放得下时就在其下，然后连页头也不要 —— 这样搜索行、
高亮样式与提示都留在屏幕上，而把整页底部锚定会把它们流进 scrollback。字形列按
整个目录定宽，而不按视野里的那三个，所以窗口滚动时它不动。在更矮的终端上，紧凑
页面一次一行地卸掉剩下的东西，而绝不卸掉它的搜索行
（`docs/ui/view-flow.md` *Below the compact page*）—— `↑`/`↓` 那一对、横线、
行，一路减到高亮样式、提示 —— 这样页面在每个高度上都放得下、从不流动；若它真
的流了，就会在一点上与它的家族不同，见下。

### 流动签名看选中项，不看行

`docs/ui/view-flow.md` 那个逐视图的选择：一页只在按键时变化，就签它的**行**，
于是任何编辑都会重新流动；一页在两次按键之间**走格**，就绝不能这么签，否则每一
帧都会把整屏 purge 重建三十次。↓ 管理器的详情页曾是这样一个页面；`/spinner`
picker 是第二个。所以 `ui::view_flow` 用 picker 的 `(selected, query)` 把它签成
`FlowSign::Frozen`：若它的页面会流动，一个推进 spinner 的帧就会让签名保持不动
（它那流动的顶部冻在 scrollback 里，反正 scrollback 对其他一切就是这个作用），
而一次移动高亮或编辑搜索的按键会重新签名，标准的 purge 重建便会重新流动这个页
面。既然它的阶梯一直减到搜索行，页面在每个终端上都放得下，问题也就不出现 ——
`a_tick_never_churns_the_page` 钉住一个 tick 在每个高度上都保持页高、什么都不
流动。

## 应用一个选择

`Action::SelectSpinner(spinner)` —— 纯的那一半已经移动了 `App::spinner` 并关掉
了 picker；边界（`tui::spinner::Session::select_spinner`）：

1. **持久化** `{config_home}/spinner.json` —— 既作为**本工作目录**自己的选择，
   也作为任何地方做过的最后一次选择，对文件做一次读-改-写
   （`tui::config::save_look`，像那里每次写入一样尽力而为）：纯格式是
   `app::SpinnerFile`，即两种 look 共用的 `app::LookFile` 的 `spinner.json`
   实例 —— `config.json` 的逐目录规则，与 `docs/ui/mascot.md` *Per directory*
   里 `spinner` 键下展示的文件形状相同 —— 并且
2. 弹出确认的 `Spinner: {name}` toast。

与 `/mascot` 切换不同，这**不需要 purge 重建**：状态行只存在于 live 区域 —— 它
逐帧的 spinner 与 shimmer 颜色从不进 scrollback
（`docs/ui/status-indicator.md`）—— 所以没有任何已提交的东西需要重绘。带的状态
行每帧按 `App::spinner()` 构建（`ui::live` 把它传给 `styled_status_line`，主回合
与 subagent session 视图合成出来的状态都一样），所以运行中的回合从下一帧起就穿
上新样式，下一回合从它的第一帧起。

启动时 `tui::bootstrap` 在第一帧之前从 `spinner.json` 播种 `App::spinner`
—— 本目录自己的条目，或者第一次在此启动时，任何地方做过的最后一次选择，并当场
**钉**成这个目录自己的（`tui::config::adopt_look`）；文件缺失或损坏则保留默认
gravity，绝不成为启动失败 —— 于是第一轮的状态行就已经穿着它。

## 设计说明

- **为什么是 picker 而不是一行 `/settings`？** 一行 `/settings` 循环的是一个你
  在下一轮跑起来之前看不到的值；状态行是 TUI 里唯一只在有东西运行时才存在的东
  西。一个行与预览都在动的 picker，是选择一种动画唯一诚实的方式。
- **为什么是 `/spinner` 而不是 `/status`？** `/status` 读起来像"给我看看这个
  session 的状态"（Claude Code 给那个名字的命令）。spinner 恰恰是被选的那部分。
- **为什么帧在 `ui/theme.rs` 里而不是在枚举上，像 `Mascot::art()` 那样？**
  mascot 的 art 是横幅着色的*内容*；spinner 的帧、节奏与颜色规则就是它全部的
  外观 —— 纯样式，本 crate 把它收在一个文件里，好让一次换肤只碰一处。枚举对字
  形一无所知，`spinner_spans` 是唯一的映射。
- **为什么两条轨道是画出来的而不是列表？** 它们的运动跑在两个独立的周期上（球
  的行程与它的跳跃；波的扫过与它的波长），所以一张帧表会写到几百条，却仍把运动
  量化到表的步长。从 `elapsed` 画，每帧只花几十次点写入，并保住其他每个
  spinner 都有的那份纯度：相位来自边界时钟，别无其他。
- **为什么一趟四次跳？** 这样球正好在到达每面墙时落地。脚本的 2.6 s / 0.66 s
  会让反弹相对墙漂移，读起来像随机；一个整数比读起来像节奏。
- **为什么一格样式只用一个 span？** comet 逐单元格的 span 是为了让每格携带自己
  的淡出步。一个字形样式没有可横跨的东西去淡出，而测试里的 `/spinner` 行与
  `VERB_START` 算术也因此读起来更简单。
- **为什么不顺手把 `● Thinking…` 表头或 tool 圆点也改样式？** 它们不是
  spinner：表头借了 tool 单元格的圆点，因为它*表示*同一件事
  （`docs/ui/thinking-stream.md`），而两者都守着本 crate 对 `●` 的唯一含义。
  `/spinner` 改的正是状态行开头的那段动画，在画它的那两个界面上都是。

## API

- `app::Spinner` —— 目录（`ALL`、`name`、`description`、`from_name`），
  `Default` = `Gravity`。
- `app::SpinnerPicker` / `SpinnerRow` —— 打开的 picker 的状态（含
  `opened_at`，预览时钟的原点）与推导出的一行；`App::spinner()`、
  `set_spinner`、`open_spinner_picker`、`close_spinner_picker`、
  `spinner_rows`、`highlighted_spinner`、`spinner_preview_elapsed`、
  `on_key_spinner_picker`。
- `app::SpinnerFile` —— `spinner.json` 格式：`app::Spinner` 之上的
  `app::LookFile`（`Look::KEY = "spinner"`），另一个键下同一套 `MascotFile`
  API（`docs/ui/mascot.md`）。
- `ui::styled_status_line(status, verb, spinner, width)` —— 给定样式的状态行；
  `status_line` / `status_line_with_verb` 是它默认（`gravity`）的情形。
- `ui::spinner_view` —— `spinner_view_lines`（页面构建器；它的长度就是预留高
  度，`docs/ui/view-flow.md`）、`spinner_picker_height`、
  `render_spinner_picker`；`ui::model_view::PreviewLayout`（在
  `SPINNER_PREVIEW_GAP` 与 `SPINNER_PREVIEW_MIN_COLS` 之上）把预览摆在列表旁
  边或下方。
- `ui::status::Track`（私有）—— 两条轨道所画的八格点字画布；
  `ui::wrap::{breath, ping_pong, hop}` —— 那些运动曲线（`pulse`；两条轨道）。
- `tui::spinner::Session::select_spinner`、`tui::config::{spinner_json_path,
  adopt_look, save_look}`。

## 测试

- `app/tests/spinner.rs` —— 目录（九种样式，comet 在前；gravity 是枚举与新
  `App` 的默认；互不相同的裸词名字、`from_name` 往返）、逐目录持久化格式
  （`spinner` 键下的往返、最后且被钉住规则、拒绝 mascot 键的宽松解析）、
  `/spinner` 命令打开 picker（列在 `/mascot` 旁边）、打开期间的动画帧请求、从
  打开起计的预览时钟、被清空的 Ctrl+B 提示，以及整套按键语法（过滤、回绕的
  ↑/↓ 对照夹取的跳转键、Enter/空格选中、Esc/Ctrl+C、拥有每个键、回合中途使用
  而不动那一轮）。
- `ui/tests/status.rs` —— 每种样式的帧单宽且定宽，默认行在每个相位都与 gravity
  轨道逐字节相同（并以球在地板上开场），每种样式的第一帧与它那一个分隔空格、
  单 span 规则、sparkle 的绽放与渐变、pulse 的呼吸、bars 的变亮、blocks 的渐
  变、经典步进；gravity 的球在墙处、四分之一跳与最高点上的精确帧，每一帧每一格
  之下的地板，暗色地板之上的渐变；wave 每格两个点、它的运动、它无缝的反转与时间
  对称的爬行、它的渐变 wash；以及 `render_live` 穿 session 的样式。
- `ui/tests/spinner_view.rs` —— 带框页面（横线、标题压在副标题上、搜索、家族
  `→` 下的五行、计数器、带说明的预览、提示）在宽与窄两个宽度下，标记与缩进、
  每一行的实时字形（含两条轨道）、宽终端上预览与列表同一列（80 列终端也算）而
  窄终端上隔一个间隔叠在下方、唯一断点处示例整段可读、↑/↓ 与搜索期间预览列与字
  形列不动、说明的粗体标题压在它的暗色描述上且每条说明在 80 列下完整、行与预览
  随时钟走格、预览跟随选中项、跨选择*与* tick 的稳定页高、活动的 ✓ 与它的座位、
  无命中时的塌缩、无叠起来的空行、宽度安全、高度契约、流动资格，以及 tick 稳定
  / 按键敏感的流动签名；紧凑页面与它之下的阶梯，一路到搜索行。
- `scripts/smoke.sh` Phase 139 —— 12 行终端上的紧凑页面，以及 7 行时的搜索行。
- `scripts/smoke.sh` Phase 110 —— 真实终端里端到端的 picker：从 palette 打开，
  停在默认 gravity 上（全新配置 home 上它的 ✓），90 列下预览在列表旁边，实时行
  （地板上的球、wave 的点字单元格）与轨道在预览，↑ 把预览移到 comet 上，在**没
  有回合运行**时那张预览在两次截取之间转了一格，一次过滤后的 Enter 换样式并弹出
  toast、写入 `spinner.json`，紧接着的下一轮状态行在流前停顿中就以新样式开场，以
  及**对同一个配置 home 的第二个进程启动时就带着它**。
- `scripts/smoke.sh` Phase 114 —— 逐目录规则的端到端，与 `/mascot` 并列
  （`docs/ui/mascot.md` *Tests*）：在一个目录里选的样式是它自己的、也是最后一
  次；一个新目录把那次钉住；那里的选择绝不移动第一个目录的；第三个目录取新的最
  后一次。
