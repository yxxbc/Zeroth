# 流式 GFM 表格（整体缓冲，实时预览）

GFM 管道表格（`docs/ui/markdown.md`）在做成现在这样之前走过两版设计：

1. **整体缓冲，成形期间不可见。** 这个块只在闭合时才提交进 scrollback（后面的
   一行可能把某一列撑宽，所以网格不是前缀稳定的 —— CLAUDE.md 不变量 2）。正确
   是正确，但带上的预览只有一行，所以一张长表格就是盯着它最后一行发呆，直到整
   个网格在闭合那一刻蹦出来。
2. **列宽在第一行数据处锁死，逐行流式输出。** 每一行到达即提交，按表头 + 第一
   行数据锁出的宽度换行。这让表格*流式*了，可这个锁定是对还没到达的行的猜测：
   只要后面某行比第一行宽，它的单元格就碎成细条。被报上来的问题就是那个典型失
   败 —— 一张键/值天气表，表头是**空的**（`| | |`），第一行是窄的
   `| **Time** | 03:00 (CEST, GMT+2) |`：宽度锁在 [4, 19]，后面每一行都碎开
   （`Temp`/`erat`/`ure`、`10.1 km/h from NNE`/`(28°)`）。任何早期锁定在一般
   情况下都不可能对，而因为批处理与流式共用一个渲染器，resize/Ctrl+O 重绘会把
   同一个坏网格也画出来。

本设计保住 (1) 的正确性与 (2) 的流式手感，正是 Claude Code 的做法：**表格只在其
宽度定稿时才渲染，但用户全程都在 live 区域里看着它成形。**

## 思路：缓冲这个块，预览整张成形中的表

- **渲染**：一张表格的源码行被缓冲（`TableState::Buffering`），而这个块在闭合
  时**整体**渲染 —— 闭合条件是一行非表格行、一个打断它的代码围栏，或消息结束
  （`flush`）。列宽来自表头**以及每一行数据**（`table_column_widths` →
  `allocate_column_widths`，见下文"把列放下"），所以网格永远装得下它真实的内
  容；单元格按分配到的宽度按词换行（行变高，绝不出现 `…`）。网格还是记录的判
  断（见下文）也基于同样的完整知识。
- **网格样式**：Claude Code 的完整网格 —— 每一行数据都被框住，相邻两行之间有一
  条 `├──┼──┤` 横线（不只是表头下面），所以每个单元格读起来都是自己的一个框。
  两条摆放规则补完观感（在被报上来的截图里都看得到）：
  - **表头居中。** 当分隔行没有声明对齐时，表头单元格在它的列里居中 ——
    `ui::header_align` 把 `Alignment::None` 映射成 `Center` —— 于是居中的标签压
    在左对齐的数据上方，读起来是列的标题而不是第一行。*显式声明*的
    `:--`/`:-:`/`--:` 对表头同样优先：改的是默认，不是明示的意图。
  - **短的单元格垂直居中。** `table_row_lines` 把每个单元格的行摆在行内
    `(height - lines) / 2` 处，所以旁边一个换成三行的邻居时，一行的单元格落在
    **中间**那行，两者读作一条记录。向下取整意味着一行的单元格在有两行的行里停
    在顶部，与 `pad_cell_line` 对水平居中的取整一致。
- **流式**：块开着时它什么都不提交进 scrollback（scrollback 不可变；已提交的行
  不能再被撑宽）。取而代之的是流式**带预览整张成形中的表** ——
  `StreamRender::preview` 返回批处理渲染的*未提交尾部*，而在表格期间那就是用目
  前已见的行渲染出的整个块（包括末尾那截不完整的 `| cell…` 行）。带是每帧重绘
  的 live 区域，所以网格肉眼可见地逐行生长、列随更宽的单元格到达而重新适配
  —— 正是 Claude Code 的体验。闭合时，完成的块（与最后一次预览一模一样）一次
  性提交，带随之清空：视觉上表格就"就地冻住"了。

预览在这里第一次从"最后一行渲染出的行"（一行）推广成"批处理渲染的未提交尾部"
（n 行），为的就是成形中的表格 —— 而如今这是**每一种**构件的规则，而不是表格的
特例：带显示的正是 `commit` 扣下的那些行，无论是什么扣下的（见下文*推广了什么*
以及 `docs/ui/markdown.md` 里的 *Scrollback and the strip share one
frontier*）。
一个运行中 tool 的单元格早就在预览多行了，所以带的机制本就为此准备好了。

## 把列放下（`allocate_column_widths`）

给定每一列的 `natural` 宽度（它最宽的单元格）与 `word` 宽度（它最宽的单个不可
断词 —— `natural_word_widths`，分词方式与 `wrap_inline` 要断它的方式完全一
致），按顺序三种情形：

1. **自然网格放得下。** 就用它。表格保持与内容一样窄，而不是撑满终端 ——
   Claude Code 的观感。
2. **它溢出了，但每一列的按词安全下限都放得下。** 每列落到 `min(natural,
   word)` —— 它只需要在*空格*处换行所需的宽度 —— 然后按每列**未满足的需求**
   （`natural - floor`）比例分配余量，用最大余数法，让各列正好填满预算。
3. **连下限都溢出。** 某个 token 必须硬断，所以把**最宽的**那一列一次降一个显示
   列（并列时先降最左，下限 `TABLE_MIN_COL` —— `level_widest_columns`）：一个
   短单元格（`facebook.com`、一个 `Packets` 表头）尽可能久地保住它的自然宽度。
   这里若按比例收缩，会同时饿死*每一*列并在单元格中间打断短词（早先的一个
   bug）。

情形 2 才是让表格读起来像 Claude Code 的东西，它取代了降最宽作为*通用*适配。
被报上来的截图就是原因：`| Check | Result |`，Check 列的自然宽度被一个离群值
（`cargo clippy --all-targets -- -D warnings`，40 列）定住，而它其他的单元格
都 ≤ 17，Result 则装着一个约 104 列的值。降最宽给出 [35, 36] —— Check 白白在每
一行上浪费 18 列，而 Result 被饿成四行换行。按词安全的下限是 [13, 18]，所以余
量按 27:86 分配，网格得出 [23, 48]：Result 拿到空间，与 Claude Code 对同一张表
的布局一致。它也比降最宽更诚实地修好了 ping 汇总那个形状 —— 一个装着不可断的
33 列 IPv6 的列，那就是它的下限，所以溢出如今来自一个能在空格处让位的列，而不是
把那个地址从 token 中间打碎。

## 宽字形：把表格切开的那个 emoji

一个 ✅ 是**两**个终端列。网格里每一处宽度早已这样量它（`cols()` →
`unicode-width`），所以渲染出的行是对的 —— 可屏幕上的一行 emoji 还是每个
emoji 宽出一列，把它的右边框挤歪，而在填满宽度的表格上还会让边框换行到自己的
一行。这就是被报上来的"emoji 把表格切开"。

原因在下一层，在写入边界。ratatui 的 `Buffer` 把一个宽字形存在**单个** cell
里，并把它视觉上覆盖的那些 cell 重置成空白的**影子**（`Buffer::set_stringn`）；
它自己的 `Buffer::diff` 随后*跳过*这些影子 —— 后端只打印一次字形，终端自己推
进剩下的列。但 `term.rs` 手写的整 cell 绘制把 cell 直接交给 `Backend::draw`，
**不**经过 `diff`，于是它们把影子当成一个空格打印出来：两个字形的列花了三列。

`term::visible_cells` 就是那个缺失的跳过。它遍历缓冲区，丢掉每个宽 cell 后面
的 `cell_width - 1` 个跟随 cell，用 ratatui 自己的 `CellWidth` 来量 —— 与
`set_stringn` 预留它们时用的是同一个度量，所以预留与跳过永远不会不一致 —— 并夹
取到行内，好让影子绝不越过分行边界。有了这次跳过，后端的位置检查会在空隙上发一
个绝对的 `MoveTo`，于是下一个字形无论终端实际把那个簇画得多宽，都落在它真正的
列上。

**三处**绘制都经过它，而这个数量正是重点：

| 绘制 | 它画什么 |
| --- | --- |
| `draw_lines` | `insert_before` 的 scrollback 提交，以及 `reflow` |
| `blit` | 一次整块 live 区域重绘（成形表格的带） |
| `draw_overlay` | 备用屏幕 —— Ctrl+O transcript、`/resume`、Ctrl+D |

只修前两处，撕裂在 Ctrl+O transcript 里依然活着 —— 而那正是你为了完整读一张表
所打开的那个视图：在 80 列上实测，它的 emoji 行出来是 29，而边框是 28。所以
`smoke.sh` Phase 41 既查内联窗格**也**查 overlay。（增量的
`prev.diff(buf)` 路径一直是正确的，这也是 live 区域有时看着对、有时不对的原
因。）

一处有针对性的例外，移植自 `Buffer::diff` 的 VS16 变通：终端对 emoji **呈现序
列**（`…U+FE0F`）的宽度看法不一，所以在把它画窄的终端上，被跳过的影子可能让字
形旁边陈旧的屏幕内容继续可见。对带 VS16 的宽 cell，影子 cell 会**先**发 —— 擦
掉字形可能盖不住的东西 —— 然后才发字形本身，最后发，好让它在那把它画宽的终端
上整块落下。这是保险，而不是针对此处任何可复现问题的修复：经光标位置实测，
tmux 3.6b 把 ⚠️ 画成两列，与 `unicode-width` 一致。

这从来就不是表格特有的 —— 散文里每个 emoji 与 CJK 字符也都曾宽画了一列
（`✅` 之后那个加倍的空格）。表格只是让它可见，因为网格有一条能被撕裂的右边。

### 极限：终端量法不同的簇

这次跳过只在我们和终端对簇的宽度看法一致之处让两者同步。它们大体一致 —— 对着
tmux 3.6b 实测，`✅ ❌ ⚠️ 🇵🇭 世 👍🏽 1️⃣` 都是两列，正是 `cols()` 报的数。
**ZWJ 序列**是例外：`👨‍👩‍👧‍👦` 对 `unicode-width` 是 2 列，对 tmux 是
4 列，所以一行含它的表格行仍会漂。应用里没有任何东西能修这个 —— ZWJ 序列的
宽度不可移植，Claude Code 也有同样的极限 —— 这就是为什么
`cols_measures_emoji_clusters_as_two_columns` 钉的是我们*确实*能控的政策，而不
假装解决了它。

### 为什么这在构造上前缀稳定

当渲染器处在一张打开的表格里（`AssistantRenderer::in_table`，任何表格状态），
或当末尾那一行是表头候选（`markdown::is_table_header_candidate` —— 一个**前导
管道符**）时，`StreamRender::commit` 扣下末尾那一行。既然各缓冲状态**一行都不
发**，`frozen` 在表格积累期间就根本不长 —— 没有任何已提交的表格行需要失效。整
个块在闭合（或 `finish`）时落进 `frozen`，之后像任何已定的行一样提交。批处理
（`assistant_lines`）与流式驱动的都是同一个 `AssistantRenderer`，所以
scrollback、带、resize 重绘与 Ctrl+O transcript 在构造上就一致。

那个前导管道符才是让候选性*在流式期间可判定*的东西：在 GFM 的可选前导管道
（无表头）形式下，任何正在生长的散文行都可能在后一个 `|` 流入的瞬间变成表头
—— 加上下一行的分隔行，批处理渲染会把提交器已经冻进 scrollback 的那些散文行重
画成网格（一个模糊测试抓到的分歧）。以 `|` 开头的表头则在这一行的第一个字符处
就决定了；罕见的无表头表格作为散文渲染，批处理与流式完全相同。已确认块之内的数
据行仍用宽松的 `is_table_row`，所以下面那段硬换行尾部的重接不受影响。

## 状态机（`ui::AssistantRenderer` / `TableState`）

```
None ──(a |-led header candidate)──▶ PendingHeader(header)
PendingHeader ──(matching delimiter)──▶ Buffering{header, aligns, rows: []}
             ──(not a delimiter)──────▶ render the header as prose, reprocess the line
Buffering    ──(data row)─────────────▶ rows.push(line)            // emits nothing
             ──(non-table line)───────▶ emit table_block_rows(header, aligns, rows),
                                        reprocess the line
```

`flush()`（消息结束 / 一个打断它的代码围栏）用同样的方式关掉一张打开的表格：
`PendingHeader` → 它实际上是的那行散文，`Buffering` → 整个块（只有表头的表格渲
染成开门横线 + 底部边框）。

`table_block_rows(header, aligns, rows, width)` 就是那个表格渲染器 —— 批处理路
径与闭合/flush/预览路径都只经它来发出表格：先算所有行的宽度，然后要么是制表符
网格（`table_open_rows` + 每行 `table_row_lines`，相邻行之间一条 `├──┼──┤`
横线，+ 底部边框），要么是键/值记录。

### 硬换行的行会重接

一个回显终端换行后源码的模型可能带着**行中**的换行，于是一行会作为一行以管道符
开头的内容加上一个碎片到达 ——
`| google.com | … | 86.8 / 89.8 /` ␤ `96.4 ms |`（单元格中间换行，第一段没有
终止符）或 `| facebook.com | … | 0% |` ␤ `15.0 / 16.4 / 17.9 ms |`（在单元格
边界处换行，第一段以 `|` 终止但偏短）。严格的 GFM 把每一行都读成一行，于是每个
碎片都铸造出一行只有一列的幽灵行（被报上来的 `│ 96.4 ms │ │ │ …` —— GitHub
渲染这份源码也一样是坏的）。但在前导管道表格里，一行真正的数据是*以* `|` 开头
的：所以 `Buffering` 会**重接**一行带管道符却不以 `|` 开头的行到它前一个已缓冲
的行上（`join_wrapped_table_row`，以空格分隔）—— 前提是合并后的行仍符合分隔行
的列数。会超出 `ncols` 的碎片是一行真正的（样式混合的）行 —— 一个完整
`| a | b |` 之后的 `c | d` 按 GFM 仍是它自己的行 —— 而在**无前导管道**的表
格，任何行都不像碎片（它的行合理地没有 `|`），那里什么都不重接。只有已确认的
数据行才重接：一个硬换行的*表头*仍通不过分隔行确认，于是作为它很可能本来就是
的散文渲染（修复 `PendingHeader` 会改变带着管道符的普通散文怎么渲染）。重接发
生在缓冲区里，在任何渲染之前，所以宽度、记录判断、批处理、带上的预览与 Ctrl+O
transcript 看到的都是修好的那一行 —— 而且既然 `Buffering` 什么都不发，前缀稳定
性不受影响（差分语料里就有一张硬换行的表；一个碎片中途的前缀只是把这个碎片作为
散文预览，直到它的管道符到达，与该前缀的批处理渲染完全一致）。

## 记录兜底（键/值转置）

当连换行后的网格都挤到没法扫读 —— 某一列**既**窄（`< TABLE_SCANNABLE_COL`，
12）**又**装着一个在它分配宽度下会换成 `≥ TABLE_RECORDS_MIN_LINES`（3）行的
单元格 —— 这个块改为渲染成竖排的**键/值记录**（Claude Code 那种紧凑的
`label: value` 形式，codex 的 `table_key_value.rs`）：每行数据变成一条记录
—— 每列一个**加粗**标签、`: `、随后的值并以对齐在它下面的续行换行（连这个都放
不下时则标签叠在值之上）—— 记录之间用一条暗色的 `─` 分隔线隔开，上限
`TABLE_RECORD_SEPARATOR_WIDTH`（40）。这个判断
（`table_should_use_records`）如今检查**每一**已缓冲的行，而不只是第一行
—— 完整知识，与宽度一样。单列表格是一个列表，绝不是记录。任何东西都不会被
`…` 掉。

## 多行预览（`StreamRender::preview`）

`preview(text, width, max_rows)` 返回带的预览**行**：**每一行未提交的行** ——
`frozen` 中 `committed` 之后的部分（例如表前散文与表格之间那个被扣住的空行），
然后把末尾那行喂给一个 clone。那正是批处理渲染的尾部，所以 `committed ++
preview == assistant_lines(prefix)` —— 整条回复在每一刻都可见，分摊在
scrollback 与带之间。

一张表格不再需要自己的分支，只需要它的 **flush**：当它开着时（渲染器进入末尾那
行时的状态，或 clone 喂完它之后的状态），clone 的 `flush()` 渲染迄今的块，连
闭合边框一起，而既然其中还没有任何东西被提交，它就和其他一切一样落在未提交范围
之内。**空的**末尾行*不*喂给 clone（紧接换行之后的 chunk 边界会把块提前关
掉）；flush 两种情形下都渲染已缓冲的行。

推广的是另一个方向：这原先只是表格的*特例*，其他每种构件都保留一个与提交前沿独
立挑选的单行预览 —— 当一个被扣下的源码行换行超过一行时，那种做法就让行从屏幕上
消失；而当一个被扣下的行渲染成零行时又会重复一行（见
`docs/ui/markdown.md` 里的 *Scrollback and the strip share one frontier*）。
末尾的空白行会被剪掉（它们也是 `commit` 扣下的段落分隔），围栏里除外，那里空白
行是内容；一条渲染出零行、什么都没提交的回复仍然预览那个圆点位，与
`assistant_lines` 一致。

结果被夹到**最后** `max_rows` 行，所以比屏幕更高的尾部 —— 一张大表、一个非常
长的被扣代码行 —— 会跟随它的前沿（最新的行保持可见；表格的顶边框滚出带外，在
块整体提交时重新出现）。当前沿是干净的时，预览是**空的**，带根本不预留预览行。

### 闭合冲刷不能搁空输入框（空白带 bug）

块闭合的那一刻，一帧之内发生两件事：整个网格（常常几十行）落进待提交的
scrollback 队列，而带从高高的成形表格预览塌成一行。待发冲刷
（`term::write_above_chunk`）在它写的行下方预留**被跟踪的** viewport 高度
—— 若那里还算着高高的带，冲刷就会把 viewport 放到高出一个带的高度那么高的地
方，而缩小又会擦掉空出来的行，于是在框与屏幕底部之间留下一条空白带（被报上来的
bug；回合结束时的 `set_view_height` 重钉只覆盖回合结束那次冲刷）。所以只要还有
待发的行，`term::paint_live` 就会在 `flush_pending` *之前*把被跟踪的高度同步
成这一帧的高度 —— 冲刷于是正好预留塌下去的区域，框紧贴底部。没有待发行时，高度
变化仍走 `ui::repin`，它缩小所擦的正是就地缩小所空出的行。由 `smoke.sh`
Phase 41 守护（dummy 的 `"table"` 演示回复流出一张 10 行网格，后面还有散文）。

### 几何：带必须预留预览所画的东西

`ui::preview_rows` 决定带的预览槽大小，而 `live_height` /
`cursor_position` / `render_live_with_preview` 都消费它 —— 但只有边界的
`StreamRender` 知道成形表格的高度。边界每帧算一次预览
（`tui::view::Session::stream_preview_lines`，以
`ui::stream_preview_max_rows(app, screen.width, screen.height)` 为上限），并
经 `App::set_stream_preview_rows` 把它行数注入纯状态 —— 即
`set_status_times` / `set_clock` 那套边界注入模式。回复流式输出期间
`preview_rows` 报的就是那个数（没有多行东西被注入时为 1，好让单元测试与渲染兜
底保留旧的单行行为），而带的 `debug_assert` 仍然钉住"画出的行 == 预留的行"。

### 预览槽有预算

那个上限过去是一个**固定额度** —— 屏幕减去预览的间隔、状态行及其间隔、一个最小
的框和一个页脚，再加两行余量。它只对一种帧成立：一场屏幕上别无他物的流式回合。
live 区域可能欠下的其他任何行 —— `/` palette、`?` 快捷键带、`@` 文件 picker、
`$` skill picker、排队消息、toast、任务 checklist、agent 名单、一个换行了的
composer 草稿 —— 对它都是不可见的，所以成形表格总是索取区域的全部余量，别的东
西只能从某处让出来。

它让出来的地方就是 composer。`live_height` 把它的求和夹到终端，而
`live_layout` 的约束求解器按约束顺序花掉被夹取的区域：先是带的 `Length`，最后
是框的 `Min(0)`。在 24 行的终端上于表格中途按 `/`，框被分到**零行** ——
textarea 从屏幕上消失，只有在回合结束、带塌下去之后才重新出现（被报上来的
bug；`smoke.sh` Phase 99 用三条带各驱动一遍）。这一点也跟表格无关：九个
`⎿ Waiting…` 单元格的并行批次在一张小屏上预览得比屏幕还高，而表格影子都没有，
却用同样的方式把框走出了屏幕。

预览是区域里**唯一有弹性的行** —— 它持有的其他一切由这一帧无法与之协商的状态
决定 —— 所以让步的是它：

- `ui::preview_budget(app, width, height)` 就是在付清其他每一行之后区域剩下的
  东西，再减去这个槽自己的尾部间隔。`height` 是区域可以占据的行数：边界处是屏
  幕，渲染内部是区域自己的 `area.height`，两者在构造上一致（未被夹取的区域正好
  等于它各部分之和，所以预算回上来就是内容的完整索取；被夹取的那个就是终端）。
- `ui::stream_preview_max_rows` **就是**那个预算，所以
  `StreamRender::preview` 绝不渲染带预留不了的行。
- `ui::fitted_preview_rows` 是夹到那个预算的 `preview_rows` ——
  `live_height`、`live_layout`、`input_box`、`cursor_position` 与带的绘制都用
  它来定尺寸的同一个数，于是一个被挤压的区域裁的是预览而不是 composer。
  `preview_lines` 把构建出的行裁到同一个数：对回复的前沿从前面裁（最新的行才
  是仍在到达的那些 —— `StreamRender::preview` 自己的跟随尾部规则），对一队实时
  cell 或一个思考块则从末尾裁（运行中的调用与 `● Thinking…` 表头才是说明正在发
  生什么的那些行）。
- `live_layout` 是手工切分而不是求解的，并且**最先**留出框的
  `LIVE_MIN_HEIGHT`：无论那些外框要求什么，只有三行的区域也留得住一个你看得
  见、打得进的 composer。它是预算之下的兜底 —— 光那条带就能撑爆一个很小的终端
  —— 而当一切都放得下时，它落点与求解器完全一致（每个槽按顺序拿走它要的；没有
  人认领的每一行都让框长高）。

同样的顺序早就在管**替换 composer** 的那些视图（`/model`、`/settings`、↓ 管理器
带）：`view_split` 钉住它们的框并挤压带。`render_strip_above` 如今把预览装进那次
切分实际留下的行里，所以一张高高的成形表格不再把它自己带上的状态行从底部裁掉。

## 取舍（有意接受）

- 一张表格的行只在闭合时才到达 **scrollback**。它们从不深藏不露 —— 带到它到达
  的那一刻就显示每一行，布局正确 —— 但终端 scrollback（以及任何读它的东西，例
  如 tmux 的 copy mode）看到的是整块一次出现。这是不可变 scrollback + 正确宽度
  的代价，也是 Claude Code 的行为。
- 成形期间，网格的列可能随着更宽的行到达而肉眼可见地重新适配（带每帧重渲）。这
  正是重点：*已提交*的网格是终局的、正确的。
- 比 `stream_preview_max_rows` 更高的表格在成形期间只预览它最新的行（跟随尾
  部）。已提交的块永远是完整的。
- 因此在一张表格中途打开一条带，会在屏幕上*缩短*成形中的网格 —— 那条带的行从
  预览的预算里出。这正是这个修复有意做的取舍：composer 与用户刚打开的带，比一
  个几秒后就会整体提交的块的行更值钱，而预览是唯一一个能让位而不损失任何东西的
  行计数。

## 测试

- `assistant_table_sizes_columns_from_all_rows` —— 被报上来的 bug：那张天气表
  （空表头、窄的第一行、更宽的后续行）在宽度 80 下把
  `Temperature` / `10.1 km/h from NNE (28°)` 渲染在单行上。
- `assistant_table_joins_hard_wrapped_rows` —— 幽灵行 bug：两种换行风味都有的
  ping 表重接它的行（三行数据、完整的 RTT 单元格、没有 `│ 96.4 ms` 行）；
  `wrapped_row_join_accepts_fragments_and_rejects_real_rows` 钉住重接/拒绝规则
  （前导管道碎片重接，已声明的行与 `ncols` 溢出不接，无管道风格永不重接）。
- `stream_render_withholds_a_table_until_it_closes` —— 一张打开的表格什么都不
  提交；`finish` 冲刷整个块，与批处理一致。
- `table_commits_whole_and_previews_while_forming` —— 流式中途：scrollback 里
  没有任何表格行，预览显示成形中的网格（边框 + 迄今的行），而
  `committed ++ preview` 等于每个前缀的批处理渲染；闭合时整个网格提交。
- `table_preview_caps_to_its_newest_rows` —— `max_rows` 上限保住尾部。
- 预算：`the_preview_budget_pays_the_band_before_the_preview`（每条带的行都
  从它里面出，而边界的上限**就是**那个预算）、
  `a_forming_table_under_the_palette_keeps_the_composer`（被报上来的几何 ——
  框、带与带三处都落位，区域填满终端）、
  `a_tall_tool_queue_yields_to_the_composer_too`（同一类，没有表格）、
  `live_layout_never_evicts_the_box`（兜底，从区域下限往上的每一个高度），以及
  绘制层面上的 `the_palette_over_a_streaming_table_keeps_the_composer_on_screen`
  + `every_band_and_terminal_size_keeps_the_composer` —— 三种预览种类 × 五种
  带状态 × 每个终端高度的扫描，也顺带跑过带那条"画出等于预留"的
  `debug_assert`。
- `stream_render_matches_batch_render_on_every_prefix` —— 差分护栏，被加强：
  如今断言预览在每个前缀/宽度下都是批处理渲染的**后缀**（而且在表格打开期间，
  `committed + preview` 是*整个*批处理渲染）。
- `preview_never_shows_a_committed_row_while_streaming` —— 性质不变，如今覆盖
  每一行预览。
- 适配：`allocate_column_widths_spends_the_surplus_where_the_content_is`（截
  图的形状 → [23, 48]，而不是拉平的 [35, 36]）、
  `allocate_column_widths_wraps_words_before_shattering_a_token`（ping 表保住
  它完整的 IPv6）、
  `allocate_column_widths_levels_the_widest_when_no_floor_fits`（情形 3 的兜
  底）、`allocate_column_widths_keeps_a_grid_that_fits_natural`，以及端到端的
  `assistant_table_gives_the_content_heavy_column_the_room`。
- 宽字形，`ui` 那一侧：`emoji_table_rows_all_end_at_the_same_column`（一张
  emoji/CJK 表的每一行网格在五个宽度下都等宽）、
  `table_rows_with_mixed_emoji_clusters_render_the_same_width`（一张混合了
  VS16、ZWJ 与肤色簇的网格），以及
  `cols_measures_emoji_clusters_as_two_columns` —— 跨七种簇钉住两列政策的依赖
  回归守卫，因为所有列宽算术都压在它上面。
- 宽字形，`term` 那一侧：六个测试钉住 `visible_cells` ——
  `visible_cells_skips_the_cells_shadowed_by_a_wide_glyph`、
  `..._skips_the_shadow_of_every_wide_cluster_kind`（CJK、一个 ZWJ 家庭、一
  对半角片假名浊点）、`visible_cells_emits_every_cell_of_a_narrow_row`、
  `back_to_back_wide_glyphs_each_skip_one_shadow`、
  `visible_cells_scrubs_then_redraws_a_vs16_shadow`（发出的*顺序*），以及
  `visible_cells_shadow_stays_in_its_row_and_the_origin_offsets`（行夹取 +
  `blit` 的区域偏移）。
- 预算的 live 那一半是 `live_streamed_table_never_squeezes_the_composer`
  （tests/live_openrouter.rs，`--ignored`）：一个真实模型流出的表格按 provider
  自己的 chunk 边界重放，在四种窗格尺寸下经 `render_live_with_preview` 绘制，
  分别配 `/` palette、`?` 带、以及不带 —— composer 在每个前缀下都保住两条横线
  与它的提示行，而带在它下方保持打开。对着三个 Venice 模型验证，它们的分块相差
  一个数量级（14 个 chunk 到 293 个）。
- 预算的边界那一半是 `smoke.sh` Phase 99，它把被报上来的 bug 端到端驱动一遍：
  dummy 的表格演示流进一个 51x24 的窗格，在块比区域的余量更高之后依次打开三条
  带。对着未修复的二进制，三者都报 `no composer row`（以及 `0 of 2` 条横线）。
- 边界那一半是 `smoke.sh` Phase 41：dummy 的演示表格带着 ✅/❌ 状态单元格，而
  这一阶段断言内联窗格**与** Ctrl+O overlay 里每一行网格的显示宽度都相同 ——
  第二个检查才是抓住"修了 `draw_lines`/`blit` 却没修 `draw_overlay`"的那个。
  对着未修复的发射器，内联检查报 `[1 65 69 71 72 78]`（撕裂的边框换行到自己那
  一行），overlay 检查报 `[65 69 71 72 78]`。
- 摆放：`table_header_cells_center_by_default`、
  `table_header_keeps_an_explicitly_declared_alignment`（`:---` 列保持左对
  齐）、`table_row_centers_a_short_cell_against_a_wrapped_one`（那一行落在三行
  的中间），以及端到端的
  `assistant_table_centers_headers_and_short_cells_like_claude_code` —— 截图上
  的那张表，断言 `✅ pass (exit 0)` 与 `--all-targets` 同行、`cargo test` 与它
  Result 单元格的中间行同行。
- 其他纯辅助函数保留它们的测试（`table_cells_wrap_*`、记录判断/渲染器、
  `table_content_rows_*`）。

## 没变的

包在代码围栏里的表格仍是逐字代码（一个 `CodeStart` 会先冲刷任何打开的表格）。
行/分隔行的解析仍是 `markdown::is_table_row` / `table_delimiter`（一个块*只在*
以 `|` 开头的表头上打开，`is_table_header_candidate` —— 见前缀稳定性那一节）；
一个没有管道符或列数不符的"表格"仍是散文。单元格仍做行内解析，列宽仍按*渲染后*
的宽度定（`docs/ui/markdown.md`）。经 OpenRouter 对着真实模型做过端到端验证：
问题里的天气表作为一张成形的实时网格流入，并以适配每一行的列提交；围栏里的那张
渲染成代码。报告里的 emoji 表用同样的方式重驱了一遍 —— 修复前它的 emoji 行量出
29 个终端列，而边框是 28，修复后每行都量出 28，而一张 20 行的 ✅/❌ 表格在带里
预览时网格完整。
