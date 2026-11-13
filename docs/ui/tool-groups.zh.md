# 折叠的读取/搜索组 —— 非 verbose 的 tool 视图

agent 在一个回合里，往往要花上大半天四处摸索，然后才开始动手改东西：一次列目录、
一次搜索、一个文件、又一个文件。若按每次调用一个单元格来画，这些探索会用没人会读
的输出填满屏幕 —— 一段目录列表上的 `● Bash(ls -la)`、十行带编号的内容上的 `●
Read(src/app.rs)`、一堆匹配结果上的 `● Bash(grep -rn …)` —— 并把真正重要的那部分
（改动与回复）挤出顶端。Claude Code 的非 verbose 视图把这样一串调用折成一行，这
里就是那个做法，照搬过来 —— 外加 agent 写进自己 scratchpad 的那些用完就扔的文
件，与读取一起折进去（下面的*scratchpad 编辑*）。

```
● I'll get my bearings first.

● Searching for 1 pattern, reading 2 files, listing 1 directory… (ctrl+o to expand)
  ⎿  $ grep -rn 'card()' .
```

这是它还在跑的样子；而一旦来了一个不是读取的东西：

```
● I'll get my bearings first.

Searched for 1 pattern, read 2 files, listed 1 directory (ctrl+o to expand)

● Bash(python3 about.py)
  ⎿  zeroth — an autonomous AI agent that lives in your terminal
```

每一次调用仍然完整地被记录：**Ctrl+O** 的 transcript 像以前一样把每一个都展开，
派生的上下文（Ctrl+D）与 rollout 未被触碰，一次 `/resume` 从同样的记录重绘出同一
行。组只是 *对话*显示它们的方式。

## Verbose 输出

`/settings` 的 **Verbose output** 行（`docs/state/settings.md`）在两个视图之间
选，它 **默认关闭** —— 也就是折叠视图，Claude Code 的默认也一样。打开则完全不分
组：每次调用都保留它一直有的那个单元格，一个字节不差，所以这一行是回到旧设计的
路，而不是第三种设计。轮转它会 purge 重建对话，于是已经躺在 scrollback 里的东西
立刻变样；`ZEROTH_VERBOSE` 为一次运行播种它（smoke 套件导出 `1`，好让它的各个
phase 保留它们所断言的逐调用单元格）。

在一个回合**中途关掉**时，这个回合已经以各自单元格显示过的那些读取会变成**打开着
的** 组（`App::hold_trailing_group`），于是重建把它们画在条带里，而下一次读取也
加入它们：否则 scrollback 会为切换之前的读取留一行、为之后的读取再留一行，而之后
每一次重建（它把历史读成一整串）画的都是一行。回合之间不保留任何东西 —— 那里一次
重建自己就能为已结束的一串画出那一行 —— 一串结束之后也不保留（回复文字、另一种调
用、一次 subagent 启动 —— 尽管模型还在往 scratchpad 里写的 `write` 会让它继续下
去，见*scratchpad 编辑*）。**打开**时，打开着的组放手，重建把它的调用画成单元
格。

## 两种样子

**实时的**，在流式条带里，组还开着的时候：

```
● Reading 1 file, listing 1 directory… (ctrl+o to expand)
  ⎿  $ ls -la
```

- 圆点是正在运行的那个 tool 的**闪烁灰色**（`docs/ui/tool-pulse.md`），而且只要
  组还开着它就一直在闪 —— **调用之间也一样**。一次已经读完的读取并不结束这个组：
  模型的下一次调用很可能还是读取，而一个在轮次之间停下闪烁、随后又开始闪的单元
  格，只会说明工作已经停了，而它并没有。它在组关闭时停下（见下文）：另一次调用、
  文字、一个错误、回合的结束。
- 各个分句按参考实现的顺序 —— 组改了什么，然后它读了什么：scratchpad 编辑，然后
  搜索、读取与列目录 —— 每个动词用组所处的时态（`Making`、`searching
  for`、`reading`、`listing`），第一个大写，计数**加粗**，名词在超过一个时用复数
  （`1 pattern`、`2 patterns`；`1 directory`、`2 directories`），行以一个 `…` 收
  尾。
- `⎿` 行说明组正在做什么，**一次一个调用的值，按调用运行的顺序**（下面的*提示
  行*），而在轮次之间它继续说着组最后做过的事。一次读取或一次 scratchpad 编辑点
  名它的文件（按 session 的路径显示规则，`docs/tools/tools.md` *Path
  display*），行太窄时就 **换行**到拐角之下 —— 一个路径的文件名在它的末尾，截断
  会把它切掉 —— 需要几行就用几行；命令则是它被敲进去时的 `$ {command}` —— 每一行
  有文字的都单独占一行，间距保留 —— 同样**换行**，首行之后的行挂在 `$ ` 之后的文
  字之下。这一行上的任何东西都绝不用 `…` 截断。
- 暗色的 `(ctrl+o to expand)` 是窄终端最先丢掉的东西；再往外，各分句用 `…` 截
  断。

### 提示行

一个并行的批次在毫秒内就完成了 —— 三次读取加一次列目录，在下一帧绘制出来之前就做
完了 —— 于是一个只点名最新那次调用的行只显示了批次的**最后一个**值，其他的根本没
上过屏幕。参考实现有同样的问题，只是隔了一层：它让每条提示显示
`MIN_HINT_DISPLAY_MS` （700 毫秒），但那是当作*节流*用的（它的
`useMinDisplayTime`），在那段保持期内到达的每个值都被丢掉，保持结束才显示最新的
那个 —— 所以它的批次也是从第一个值直接跳到最后一个。

这里那一行是一个**队列**。值是组内的调用按它们运行的顺序 —— 先是被保留的，然后是
正在跑的那个 —— 而这一行一次一个地走过它们，每个都保持 [`GROUP_HINT_HOLD`]（参考
实现的 700 毫秒）之后再被下一个替换，无论一帧来得多晚都不跳过任何一个。同一轮里
的一次读取加一次列目录会读作

```
● Reading 1 file, listing 1 directory… (ctrl+o to expand)
  ⎿  README.md
```

保持 700 毫秒，然后是

```
● Reading 1 file, listing 1 directory… (ctrl+o to expand)
  ⎿  $ ls -la
```

而各分句从头就在数整轮的量。一个还在批次里等轮次的调用绝不会抢在它前面的调用之前
被显示（在有任何东西跑起来之前，由批次的第一调用顶替），一次一回合的组则在每次调
用跑的当刻就显示它的值（模型写它下一次调用要花好几秒，比保持期更长），而最后一个
值会留在行上直到组关闭。队列最多落后现实「每个比一个保持期更快的调用一档」—— 对
一个大批次来说就是几秒钟，在模型写下一次调用的同时播放完。它刻意不设上限：一个会
让最老的等待值被丢掉的上限，会跳过大批次**最开始**的那些调用 —— 恰恰是这个队列存
在的意义所在的那些值 —— 而且滞后反正随组结束而结束，因为关闭的组会重置这一行。

这个状态是纯的：`App::group_hint` —— 哪一个调用在行上、从何时起 —— 由边界在
[`App::advance_group_hint`] 里按帧时钟推进（也就是 `set_pulse` 取的那个读数，在
回合的 32 毫秒动画链运行时于每次绘制之前注入），并在每次组关闭时重置，于是下一个
组从它自己的第一个调用开始。权限 prompt 的静止帧只在被保留的调用之间步进，因为它
所询问的那个调用在那里有自己的单元格。

一条两秒之后还在跑的命令（`TOOL_GROUP_CLOCK_DELAY`，参考实现的阈值）会在行的末尾
戴上它自己的**时钟** —— `$ grep -rn todo . (12s)`，打印出东西之后则是 `(12s · 34
lines)` —— 于是一次漫长的搜索读起来是活着的，而不是冻住的。时钟保持完整：放得下
就骑在命令的最后一行上，放不下就移到命令之下自己的那一行。时钟是这条命令自己的
（`App::command_elapsed`），也只骑在它自己的值上：行上还在显示更早的调用时，它一
点时钟都不显示。一条跑得足够久、值得 Ctrl+B 的命令也保留那条提示，挂在组的单元格
上。

**已沉降的**，在 scrollback 里，组关闭之后：

```
Read 1 file, listed 1 directory (ctrl+o to expand)
```

没有圆点，齐着边距，通体暗色 —— 已沉降念头的那套装扮
（`docs/ui/thinking-stream.md`，`Thought for 4s · 1.2k tokens (ctrl+o to
expand)`），因为剩下的是一条关于这个回合的事实，而不是正在发生的事。计数保持加
粗。

## 哪些调用会加入一个组

**成员**是组收下的调用（[`app::is_group_member`]）：

- 对一个文件的 `Read` —— 任何文件，但**图
  片**（`png`、`jpg`、`jpeg`、`gif`、`webp`）除外，它的单元格是画在它下面的那张
  图（`docs/ui/images.md`）：折进组的行里它什么都画不出来，所以一次图片读取永远
  保留自己的单元格；
- 一条只做读取的 `Bash` 命令，由 [`read_only::read_kind`] 判定 —— 一次**列目录**
  （`ls`、`tree`、`du`）、一次**搜
  索**（`grep`、`rg`、`find`、`fd`、`which`、……）或一次**读
  取**（`cat`、`head`、`tail`、`wc`、`jq`、`sed -n '1,40p'`、`awk '{print
  $1}'`、……）。一条管道或一个列表取它最强的种类 —— 出现列目录就是列目录，否则是
  搜索，否则是读取，参考实现的优先级 —— 所以 `ls | grep x` 是列目录、`cat f |
  grep x` 是搜索；`echo`、`printf`、`true` 与 `cd` 搭着走，自己没有种类，所以
  `cd src && ls` 是一次列目录；
- 对某个 session **scratchpad** 里一个文件的 `Write` 或 `Edit` —— 但只在它**成
  功** 之后（下面的*scratchpad 编辑*）。

而一次读取只在它**跑起来了**之后才加入，无论它在运行中是成功还是失败 —— 一次什么
也没匹配到的搜索以 1 退出、一次对缺失文件的读取失败；两种情况下模型都读到了那个
错误，而 Ctrl+O 保留它。一个从未运行或从未结束的调用保留自己的单元格，因为它的单
元格就是*为什么*的记录：在 prompt 处被拒或被 classifier 拒、被 hook 挡住、被 Esc
或一个后端错误切断、被 tool-call 上限拒绝，或者被移到后台（`wait:
0`、`run_in_background`，或对一次长搜索按 Ctrl+B）。用户跑的 `!` 命令也一样，还
有 MCP 调用 —— 它有自己的折叠行（`docs/tools/mcp.md`）。

成员自己的附加物随它一起进 Ctrl+O：一条严格检查担保不了的命令所挣到的 `⎿ Allowed
by auto mode classifier` 注记（`grep -c TODO *.rs` 是一次搜索，但它的 glob 把它
送去了 classifier）、hook 的 `⎿` 注记，以及完整输出。

`read_kind` 对它称之为读取的东西刻意**宽松**，只对一件事严格：折叠行绝不能把一次
写总结成一次读。glob 或变量没问题（`grep -c TODO *.rs` 仍然是搜索），而一次重定
向到文件、一个 `$(…)` 替换、一个后台的 `&`、一个子 shell，以及那些让读取程序去写
或去跑东西的 flag（`find -delete`/`-exec`、`sort -o`、`tree -o`/`-R`、`rg
--pre`/ `--hostname-bin`、`fd -x`、`ag`/`ack`/`bat --pager`、`file -C`、`less
-o`、`bat cache`、`uniq IN OUT`、带 `system`/`getline`/`>`/`|` 的 `awk`、超出
`-n …p` 与一次 stdin `s///` 的 `sed`）都让它完全不算读取。短 flag 在捆绑里或带着
值粘在一起时（`tree -aR`、`fd -xrm`）都算数，长 flag 则按 GNU getopt 允许的任意
缩写算数（`sort --out=…`）：只比对整个词，正是那些拼法能溜过去的地方。一条命令是
否*可证明地*只读 —— auto 模式是否可以为它跳过 classifier —— 是另一个更严格的问
题，有它自己的函数 [`read_only::is_read_only`]（`docs/state/permissions.md`
*Read-only commands skip the classifier*）。两者只共用 tokenizer，别的什么都不共
用。

### 一个组数什么

- **搜索** —— 每条搜索命令，各算一个 "pattern"。
- **读取** —— 每个 `Read` **路径算一次**，无论模型重读了多少次，加上每条读取命
  令，各算一个 "file"（`cat a b` 算一个：这些词并不说明一条命令打开几个文件，参
  考实现也是这么数的）。
- **列目录** —— 每条列目录命令，各算一个 "directory"。
- **scratchpad 编辑** —— 每次 scratchpad 的 `write` 与 `edit`，各算一次 "edit"，
  外加它们加过与删过的行数。

## scratchpad 编辑

session 的 scratchpad（`docs/tools/scratchpad.md`）是 agent 放那些不属于用户工作
的东西的地方 —— 它试着写的一个脚本、一条笔记、一个中间文件。那里的改动属于探路，
而不是对项目的改动，所以较新的参考实现也把它折进组里，作为一个自己的分句，这里也
一样：

```
● Making 1 scratchpad edit +25… (ctrl+o to expand)
  ⎿  /tmp/zeroth-1000/18cea7cc0aee22c0-5d77f/scratchpad/guess_the_numbe
     r.py
```

这是模型正在写文件的样子；组关闭之后则是：

```
Made 2 scratchpad edits +26 -1 (ctrl+o to expand)
```

- 分句是 `Making`/`made` **N** 个 `scratchpad edit(s)`，然后是这些编辑加过与删过
  的行数 `+A -D`，**暗色** —— 在实时行上是 `(ctrl+o to expand)` 提示的颜色，沉降
  之后是这行自己的暗色，绝不像计数那样加粗 —— 没有变化的那一侧省略（一个新文件读
  作 `+25`，绝不是 `+25 -0`）。不用文件单元格那个 `Updated … (+A -D)` 拐角所戴的
  diff 颜色：这一行是摘要，而绿与红属于 Ctrl+O 展开的那些单元格。
- 行数来自每次编辑自己的单元格头（`llm::tools::write_report`/`update_report`）：
  新文件是 `Wrote {N} lines to …`，覆写或编辑是 `Updated … (+A -D)`，旧 rollout
  则是遗留的 `Created … ({N} lines)` —— 于是这一行说的正是 Ctrl+O 展开的那些单元
  格所说的话，而一次 `/resume` 只凭记录就能把它重绘出来。`No changes to …` 是一
  次没有点名任何行的编辑。
- **一个还在生成的 `write` 已经是组的一部分了。** 它的草稿
  （`docs/ui/write-streaming.md`）不流式输出自己的单元格：路径一出来、并且落在
  scratchpad 里，分句就开始数它，它的 `+N` 随行的到达而增长，提示行点名它的文
  件。排在它批次后面等待时，它数的是它所携带的内容（被采纳的草稿，否则就是调用的
  原样参数）；而 `edit` 只有在跑过之后才点名它的行数，因为只有跑过才报告 diff。

**什么算 scratchpad**（`PathDisplay::in_scratchpad`）：本 session 自己的目录，无
论在 `ZEROTH_SCRATCHPAD_DIR` 把它放到了哪里 —— 那是边界与 cwd、`$HOME` 一起注入
的策略 —— 以及**任何** session 的，按它的布
局，`{tmp}/zeroth-{uid}/{session}/scratchpad/…`
（`scratchpad::is_session_scratchpad`），因为在一个新进程里续上的对话有自己的
scratchpad，而它的记录点名的是更早那个进程写进去的那个
（`docs/tools/scratchpad.md` *Known limitations*），它的行在续上之后必须像之前一
样重绘。两项检查都是词法的，与 permission gate 的规则一致：绝对路径，任何地方都
不许有 `..`。布局检查**只用于显示** —— permission gate 的豁免仍然只问本 session
自己的 scratchpad，不问别的。

**只有成功的编辑才加入。** 失败的、或从未运行的 —— 在 hook 强制的 prompt 处被
拒、被 hook 挡住、被 Esc 或后端错误切断 —— 什么都没改，所以它保留自己那个红色的
单元格，说明原因，而「做了 N 次编辑」依然为真。这比读取更严格，读取跑过之后无论
怎么结束都会加入：一次失败的读取仍然告诉了模型一些东西，一次失败的编辑什么也没
做。

## 组的生命周期

组是**历史**的一个属性，像并行的 MCP 运行一样（`docs/tools/mcp.md`）：记录仍然是
普通的 `HistoryItem::Tool`，而组就是任何一段极大的成员连续 —— 沿途放行那些自己没
有内联单元格的条目（一次 task 调用、一条没有报告的 hook 注记）以及它们之间的**念
头** —— 一条带着 hooks 单元格的 hook 注记是一个单元格，它会结束这个组
（`docs/tools/hooks.md` *The hook-run cell*）（[`app::group_run`]）。正是这一点
让每一次重建 —— 一次 resize、一次 overlay 返回、一次 `/resume` —— 都能从同样的记
录推导出同一行（`ui::conversation_lines`），没有组 id 需要记录，也没有组 id 会漂
移。

scrollback 还没有接手的那个，是**打开着的**组：`App::tool_group`，它打开时的历史
索引，在它的第一个成员解析的那一刻设定。它开着的时候，它持有的东西一个都到不了
scrollback —— 条带替它们说话 —— 而每次重建都把它排除在外，就像排除一个仍在飞行的
MCP 运行一样（`ui::committed_history`）。每一次解析都由一个调用安放，
[`App::place_resolved_tool`]：成员被**保留**（若没有打开的组，就打开一个），别的
一切都**关闭**打开着的组并排在自己之前，于是组的行提交在那个关闭它的调用之上。

### 什么会关闭一个组

任何不属于它的东西，在它即将上屏的那一刻 —— 边界在每一个之前关闭组
（`tui::commit::close_tool_group`，没有打开的组时是个空操作）：

- **说了些什么的回复文字** —— 那一行落在回复第一行之上；空白什么也不关闭，因为它
  什么也没记录；
- **另一种调用**：它的批次公告、它的开始、或它的解析，哪个先到算哪个 —— 由读取领
  头的一个并行批次加入组，由别的任何东西领头的则关闭它；
- 一次 **subagent 启动**、一个**开始流式输出**到自己的单元格里的 `write` —— 也就
  是路径已经出来、且落在每个 scratchpad 之外的；在它的路径出来之前，什么都说明不
  了它往哪写，所以它的单元格在打开着的组**之下**流式输出，组悬在它上面，而落在
  scratchpad 里的 write 则加入组 ——、一条**被引导的消息**落地、一条
  **notice**（一条错误 notice、后台 shell 或某个 agent 的完成）；
- 一个关于非读取调用的**权限 prompt****，好让那一行提交在问题之上；而关于一次*读
  取*的 prompt（manual 模式询问 `ls`）让组保持打开，并在 prompt 的上下文里、把它
  静止地显示在那个被询问调用的单元格之上；
- 一个**后端错误**、一次 **Esc 中断**，以及**回合的结束** —— 每个回合结束时都有
  一道兜底（`dispatch_after_turn`），于是没有任何一条路径能让一个组把它的读取扣
  在 scrollback 外面。

### 念头

一个念头不关闭组，它也不内联显示：组的行就是对话对那一串调用的全部呈现，它的念头
也包括在内，而行上的 `(ctrl+o to expand)` 说明它们在哪里。一个在组打开时落地的念
头沉降进组内部的历史（`tui::stream` 的 `settle_reasoning` 在组打开时把它留住），
而当组关闭时它的行独自提交（`ui::tool_group_lines`）—— 两个读取之间的念头与最后
一个读取之后的念头同样如此，在实时提交上和在每次重建上都是。Ctrl+O 永远
verbose，把每个念头显示在它在调用之间的位置上，而 **Verbose output** 又给每个念
头它自己的 `Thought for …` 单元格。参考实现则把这样的念头**推迟**到组的行之后，
这里也曾这样做过：每个念头一行，往往是 `Thought for 0s · 7 tokens`，把组存在的意
义 —— 省下的那些行 —— 又还了回去。

它流式输出的时候，也留在条带之外：打开着的组之下不会开一个 `● Thinking…` 块，组
单独占着那个位置（在 `ui::live::preview_lines` 与 `ui::preview_rows` 里组都压过
这个块，于是画出的行与预留的行一致）。那里一个块会在每个念头上把条带撑出一个空
隙、多出几行，又在下次调用时还回去，盒子在两个读取之间来回跳；而组的行与它的提示
行则纹丝不动。状态行照样说模型在思考（`Thinking for 6s`），Ctrl+O 也照样把念头按
它流式输出的样子显示出来。没有组打开时的念头 —— 第一个读取之前，或在一个没有任何
读取的回合里 —— 保留它的块与它的行（`docs/ui/thinking-stream.md`）。

## 什么情况下不适用

- **Ctrl+O** 永远 verbose：transcript 就是发生过的事，每次调用都完整。
- **agent 的 session 视图**为每次调用保留一个单元格：它的提交以它自己那次运行产
  生的记录为键
  （`Session::commit_agent_tail`，`docs/agents/agent-view-streaming.md`），而在
  那里保留一个组，就得在第二份历史上再挂一份打开组的状态。主视图的折叠行是 lead
  自己的那些调用。
- 用户的 **`!` 命令**永远是自己那个 exec 单元格
  （`docs/input/shell-command.md`）。

## 限制

- 一次在某个组刚因那条回复的头几个字而关闭之后、**回复中途**丢失的连接会让这次尝
  试作废（`docs/providers/offline.md`）：回复没了，已关闭的组保持关闭，而如果重
  启后那一轮又读了，就会在第一个旁边开出第二个组。于是 scrollback 显示两行，而下
  一次重建显示一行 —— 无害，且足够罕见，不值得为它带一条重开路径。

## 测试

纯的那几半有单元测试：成员资格、连续段、计数、安放与提示行的队列（每个值按顺序、
各保持一段、一个都不跳过、新组从头开始）在 `app/tests/tool_group.rs`，scratchpad
编辑的成员资格、行数与流式草稿在旁边；两种样子、路径显示、窄宽度下的拟合、换行的
路径与命令、行上放不下时移到命令之下的时钟、`+A -D` 的计数、一个并行批次先第一个
值再下一个值、打开组内的一个念头把条带留给单元格、把对话留给组的行，以及重建，都
在 `ui/tests/tool_group.rs`；`read_kind` 与 `is_read_only` 对着参考实现自己的用
例外加对抗性的（重定向的写法、unicode 空格、`find` 的分组、续行）在
`read_only/tests.rs`；scratchpad 的布局在 `scratchpad.rs`。

离线 dummy 的 **`explore`** 场景（一个点名 "explore" 的 prompt）在没有网络的情况
下驱动一个完整的组：一次列目录、轮次之间的一个念头、一次并行搜索与两次读取、第二
个念头 —— 活模型在它下一次调用之前的停顿，提示行在其中把这一轮的值播放完 —— 然后
是一次关闭它脚本运行，每个调用都以真实执行器的输出解析。它的 **`scratchpad`** 场
景（一个点名 "scratchpad" 的 prompt）把一个写小游戏的 `write` 按活模型生成它的方
式流式输出进这个 session 的 scratchpad，然后 `edit` 它，两者都经真实的
`write_report`/ `update_report` 解析。

`scripts/smoke.sh` Phase 146 对着真实二进制播放第一个 —— 模型思考时只有那个实时
单元格（状态行在数念头，单元格下面没有块）、分句在增长、并行轮次的三个值依次占据
提示行、关闭它的那个调用之上是唯一一条沉降行且没有逐读取的单元格、它所保留的两个
念头都没有 `Thought for` 行、Ctrl+O 展开每次调用与两个念头、一次 resize 重建画出
同一行，以及 **Verbose output** 把它展开。Phase 147 播放第二个 —— 实时的 `Making
1 scratchpad edit +N…` 行在 write 流式输出的同时在文件路径上数字往上走、任何地方
都没有 `Write(` 单元格、一条沉降的 `Made 2 scratchpad edits +26 -1`，以及 Ctrl+O
把两次调用都展开。其他每个 phase 都用 `ZEROTH_VERBOSE=1` 跑。
