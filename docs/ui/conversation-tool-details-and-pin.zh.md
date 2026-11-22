# 对话内的工具详情与置顶的用户输入

拥有屏幕的模式（`--fullscreen` 或 `ZEROTH_TUI=full`，见 [`full-screen`](full-screen.md)）
把对话本身变成可以直接操作的地方。工具调用就在原地展开，不用另开一页；回复滚得
再远，它在回答的那个问题也始终留在顶部一行。这两件事在内联 viewport 里都做不到，
而这正是这个模式存在的理由：内联模式里画完的行归终端所有，既无法切换，也无法做
命中测试。内联默认模式保留折叠的单元格和 Ctrl+O。

## 工具调用在对话里展开

后端工具调用一开始是折叠的：`● name(args)` 标题按生命周期着色，下面是 `⎿` 输出。
普通输出折叠时显示三个屏幕行（`TOOL_FOLD_ROWS`），再加一行
`… +N lines (ctrl+o to expand)`；如果刚好只有四行，直接显示全部并不多占空间。
`!` shell 命令的输出不折叠（[`src/ui/tool.rs`](../../src/ui/tool.rs)，`tool_cell_body`）。

在可展开条目的任意一行上左键单击，这个单元格就会换成 Ctrl+O transcript 为它画的
形态 —— 用的就是同一个构建函数
[`ui::transcript::expanded_item_lines`](../../src/ui/transcript.rs)，由
[`conversation_lines_carded`](../../src/ui/conversation.rs) 调用，而全屏画面正是由它
构建的（[`src/tui/view.rs`](../../src/tui/view.rs)，`full_expanded` 上的 `expanded`
判定）。在展开后的任意一行再点一下就折回去。只有普通对话接受点击：打开的 overlay、
权限提示和 `AskUserQuestion` 提问先拿走鼠标。

「可展开」由 [`ui::is_expandable`](../../src/ui/conversation.rs) 定义：工具调用、
task 调用、思考、压缩标记、agent 组、hook 报告。消息、通知和回合摘要本来就在屏幕上
完整显示，没有第二种形态可换；一串被折叠成组的 `read`/`search` 调用
（[`tool-groups`](tool-groups.md)）只是一行，下面没有可点的条目。

展开在标题下面加行，折叠把这些行收回。点击记录条目的起始行和首行所在的屏幕行
（`full_anchor`，[`src/tui/actions.rs`](../../src/tui/actions.rs)，`toggle_full_item`）。
下一帧尝试留住首行的位置，但以重建后的对话范围为限（[`src/tui/view.rs`](../../src/tui/view.rs)）。
只有算出的位置在底部之上才会锚定（`full_top`）；若落在新的底部，就继续跟随尾部，
展开后高过窗口的单元格可能把自己的标题卷出顶部。在尾部点击也不保证一定跟随：展开
增加的行可能让锚点落在新底部之上。可靠地留住点开的条目是工作窗口的目标，不是当前
切换动作的保证。

详情不是另一个滚动区。它就是普通的对话行，所以跟着对话一起滚（PageUp/PageDown、
Ctrl+Home/Ctrl+End、滚轮 —— `scroll_full`），拖选和复制覆盖它，条目的每一行都可点，
不需要去找小小的「收起」按钮。

### 展开形态显示什么

展开的单元格和折叠的是同一条记录，只是完整：

- `bash` 调用是它的标题加折叠时藏起来的所有行，结尾是 `⎿` 行上记录的执行事实 ——
  退出码、错误类型、耗时（[`src/ui/tool.rs`](../../src/ui/tool.rs)，`tool_full_lines`
  与 `command_receipt_lines`）。执行器写进输出的 `Exit code: N` 行在两种形态下都不
  显示，整行是 JSON 文档的行在两种形态下都会重排成便于阅读的样子；
- 输出为编号文件格式的 `read`、`write`、`edit` 显示不设上限的编号块 —— 行号、hunk
  之间的 `⋮` 空档、`+`/`-` 底色以及字符级标记
  （[`src/ui/file_cell.rs`](../../src/ui/file_cell.rs)）—— 折叠时正文最多显示十个屏幕行
  （`FILE_PEEK_LINES`）；`… +N lines (ctrl+o to expand)` 提示数的是尚未开始显示的源文件行，
  不是折行后的屏幕行；
- 能识别的搜索按文件分组，带行号栏、一行统计（匹配数与文件数，或 `No matches`），
  命令单独成段；识别不了的输出保留 `Output` 标签和原始行
  （[`src/ui/search.rs`](../../src/ui/search.rs)，`search_lines` 的 `full` 分支）；
- 思考显示完整的推理链，压缩显示摘要正文，agent 组每个子 agent 一个单元格，hook
  报告逐个列出跑过的 hook。

有三种状态必须直说，因为它们的长度本身说明不了。没有产出任何内容的调用显示
`(no output)`，还在排队的显示 `Waiting…`，运行中而尚无输出的显示 `Running…`。
被截断的输出用暗色的 `…` 标记，包括达到上限的 `read`、`web_fetch` 或 `web_search`
（[`TOOL_TRUNCATED_MARKER`](../../src/ui/theme.rs)）：上限之后已经没有可展开的内容，
这条标记是唯一能说明「看到的输出并不是全部输出」的东西。能识别的搜索如果保留的
输出被截断，就在计数后面加 `retained results`，而不冒充总数
（[`search-details`](search-details.md)）。失败的调用，展开后是红点下面的错误正文；
展开的单元格画的是存下来的记录，所以 `/resume` 载回的对话展开出来也是一样的。

主对话根据 `history` 建立点击目标（`full_items`），再单独加上流式回复
（[`src/tui/view.rs`](../../src/tui/view.rs)）。`app.tool_queue` 不参与绘制这个对话窗口，
所以其中没有可点的实时调用。运行中的单元格仍由 live strip 绘制：命令有标题、最新的几行
输出和一行 `+N lines (Ns · wait …)` 时钟（[`src/ui/tool.rs`](../../src/ui/tool.rs)，
`running_command_lines`）。展开渲染并不要求记录已经结束：Ctrl+O 带上实时工具队列时，
等待中和运行中的记录也能显示。恢复 rollout 只还原已结算的工具记录
（[`src/session.rs`](../../src/session.rs)），不还原队列。实时输出更新 strip，不往对话
本身加行。带图片的工具单元格在两种形态下都把图挂在下面（[`src/ui/image.rs`](../../src/ui/image.rs)）。

### 与 Ctrl+O 的关系

Ctrl+O 依然把整段对话作为翻页器开到备用屏上：每个调用按顺序展开，最后是 live 尾部，
`q` 和 `ctrl+o` 退出；`esc` 只有在不打开回溯预览、也不在预览中向前回溯时才退出（[`src/app/views.rs`](../../src/app/views.rs)，
`on_key_overlay_toggle`；[`src/ui/transcript.rs`](../../src/ui/transcript.rs)，
`render_tool_view`）。它从来不是查看单次调用的途径 —— 它没有选中项 —— 它保持原来的
职责：通读、滚动、从里面复制。拥有屏幕只是多了一个更小的动作：在原地打开一个单元格，
周围的对话照样滚；它没有从 Ctrl+O 那里拿走任何东西。折叠单元格上的
`(ctrl+o to expand)` 提示仍然指向装着所有调用的翻页器；默认值翻转之后二者保留哪个，
留待设计页决定。

## 用户输入置顶跟随

当你读的这条回复，它回答的问题已经滚出画面顶部时，那个问题就留在对话区顶部的一行上：
整条消息的摘要，下面的对话保持自己的滚动位置。半路才接上一个长回答的读者，仍然看得出
问的是什么，不必用回答的行去找答案。

**这一行是摘要，不是问题的第一行。** 输入的全文折叠成一行 —— 每个换行和每段连续空白都
变成单个空格 —— 所以真正在提问的那些字才留了下来。长问题往往以信息量最小的部分开头
（`look at this:`），而那恰恰是「复制第一行」会显示出来的东西。摘要超出画面宽度时按字素
边界截断，末尾加 `…`
（[`src/ui/conversation.rs`](../../src/ui/conversation.rs)，`pinned_prompt_line`）。

**整条消息走完之后才出现。** 候选是「窗口顶部所在的那个回合」—— 起点在顶部之上的最新一条
用户消息 —— 并且只有它的**末行**也越过顶部时才置顶（`pinned_prompt`）。这条消息还有任何
一行在画面上，就不置顶：把它的开头一行复制到它自己的续行正上方，读起来像同一句话变成了
两段。这就是这条规则按「回合」而不是按「行」判断的地方，也是单行输入表现与从前完全一致
的原因 —— 它一次就整条离开画面。按窗口位置而不是按会话最新消息来选，意味着向上翻旧回合
时，置顶行也跟着往回走。

置顶要占一行，所以它从不盖住正文。有空间时，画面先预留这一行，再重算窗口起点；如果在
跟随尾部，顶部可能再少显示一行。对话区只有一行高时不置顶，以免藏起唯一可见的一行。
如果新内容让尾部窗口越过另一条输入，置顶也会随之改变。

**它是一个控件。** 这条横条代表窗口上方的一条输入，所以指针移到它上面时字会亮起来 ——
用强调色，和选择器里被选中的行同一个提示 —— 左键单击则把那条输入滚回视野：它的首行成为
窗口顶部，下一帧这条横条就退场，因为消息又可见了
（[`src/tui/actions.rs`](../../src/tui/actions.rs)，`on_pin_row`、
`scroll_to_pinned_prompt`）。滚动和其他锚点一样被夹紧
（[`src/ui/layout.rs`](../../src/ui/layout.rs)，`scroll_anchor`），所以落在最后一屏时会
重新跟随尾部，和在那里按 PageDown 完全一样。这条横条本身仍不在对话的命中图
（`full_window`）里：点它是回到输入原文，绝不会选中文字或打开链接。

置顶在对话区顶部、横幅之下；如果会话短到横幅加对话一屏放得下，就不会出现。在 agent 的
会话视图里，画面画的是该 agent 自己的 history，同一套规则改用该 agent 自己的输入，而不是
主对话的输入。

## 覆盖范围

画面和鼠标属于边界层，靠实际运行应用来验证。纯逻辑部分各有测试：对话遍历的展开分支
和 `is_expandable` 集合
（[`src/ui/tests/transcript.rs`](../../src/ui/tests/transcript.rs)），展开单元格对每种
工具的正文在 Ctrl+O 的测试里
（[`src/ui/tests/tool.rs`](../../src/ui/tests/tool.rs)、
[`src/ui/tests/search.rs`](../../src/ui/tests/search.rs)），置顶横条的判定规则与摘要行在
[`src/ui/tests/pinned_prompt.rs`](../../src/ui/tests/pinned_prompt.rs)，它与每个滚动键
共用的滚动锚点在 [`src/ui/tests/layout.rs`](../../src/ui/tests/layout.rs)。这个模式在
tmux 下的端到端运行是 smoke 阶段 156（[`docs/ops/smoke.md`](../ops/smoke.md)）——
它还会单击横条并确认输入原文回到了画面上。
