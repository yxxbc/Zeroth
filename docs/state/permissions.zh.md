# 工具权限请求

Claude Code 在改动任何东西之前都会先问：一次 `write`、一次 `edit` 或一条 `bash`
命令都会中止当前回合，并在 composer 原来的位置放上一个内联的 approval
prompt。这里做的正是同一件事，移植到了内联 TUI —— [`crate::permission`] 中的纯
请求/决策词汇、`app/permission.rs` 中的模态 prompt 状态、`ui/permission_view.rs`
中的渲染，以及 [`crate::permission::PermissionGate`] 中的跨线程握手。

哪些调用会停下来发问，取决于 session 的**权限模式**（见下文*权限模式*）。一个
session 以 **auto** 启动：文件改动会执行，只做读取的命令会执行，其他任何命令则
交给 auto 模式的 classifier，它无法判断时就退回发问。下面的 prompt 就是
**manual** 模式对每一次 `write`、`edit` 和 `bash` 调用所弹出的东西，也是其他模式
在遇到自己覆盖不到的调用时退回的东西。

```
● Write(hello.py)
  ⎿  Waiting…

────────────────────────────────────────────────────────────────────────
 Create file
 hello.py
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
  1 #!/usr/bin/env python3
  2
  3 def main():
  4     print("hello")
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
 Do you want to create hello.py?
 ❯ 1. Yes
   2. Yes, allow all edits during this session (shift+tab)
   3. No

 Esc to cancel · Tab to amend
────────────────────────────────────────────────────────────────────────
```

## 形状

五种，一种版式，对应 `permission::PermissionKind`：

| kind      | title           | body                              | question                              |
| --------- | --------------- | --------------------------------- | ------------------------------------- |
| `Write`   | `Create file`   | the numbered new contents         | `Do you want to create {file}?`       |
| `Edit`    | `Edit file`     | the numbered diff hunks           | `Do you want to make this edit to {file}?` |
| `Bash`    | `Bash command`  | the indented command + description | `Do you want to proceed?`             |
| `Mcp`     | `Tool use`      | the indented `{server} - {tool}({args}) (MCP)` + the server's description | `Do you want to proceed?` |
| `Session` | `Session input` | the input on one line (`print(2 + 2)⏎`) + a dim `into {command} · session {id}` | `Do you want to send this input?` |

`Mcp` 那一行就是换了个目标的 `Bash` 行：它命名的是一个调用而不是一条命令，所以
沿用同样的形状（`docs/tools/mcp.md`）。`Session` 行也是如此：输入到一条仍在运行
的命令里的内容（`bashsend`，`docs/tools/bash-tools.md`）和一条命令一样会执行
—— 输入到 `python3` 的一行就是 Python —— 所以它以同样的方式发问。什么都不输入
的调用从不会发问：`bashwait` 和 `bashlist` 只做读取，`bashkill` 结束的是一条
模型自己启动的命令（↓ 管理器里的停止同样不发问），而单独一个 `<C-c>` 只是中断。

目标**已经存在**的 `write` 属于 `Edit` —— 它展示 diff，而不是整个文件，和最终
那个 `Updated {path} (+A -D)` 单元格完全一致。标题下面的文件行也按那个单元格的
方式命名文件 —— 在 cwd 之下用相对路径，在 home 之下用 `~` 相对路径，其他地方用
绝对路径（`docs/tools/tools.md` *Path display*，经由 `App::path_display`）；请求
自己的 `target` 仍是模型给的逐字路径，那才是规则引擎和模型看到的东西。

正文和已完成的 `write`/`edit` 单元格所渲染的是同一种带编号、带语法高亮、按 `+`/`-`
着色的块（`ui/file_cell.rs` 的 `numbered_body_lines`，两者共用）—— 是整个文件/diff，
不是一瞥：prompt 的意义就在于你读到的是你正在批准的东西，**全部**都读到。
prompt 是一个带边框的视图，和菜单、picker 一样（`docs/ui/view-flow.md`）：比终端
更高的一页会贴着底部绘制 —— 问题、选项和提示行收在这一页的末尾，所以它们永远在
屏幕上 —— 而被跳过的顶部则流入终端真实的 scrollback，在那里由终端自己的滚动读完整
个 diff。被废止的上限曾把正文中段藏在一个 `… +N lines` 尾巴后面；如今那个尾巴只在
超过 `PERMISSION_BODY_MAX_ROWS` 安全上限时才保留（每次绘制 tick 都会重建并重新
高亮这一页，所以一个病态的多兆字节 write 绝不能把每一帧变成一个无界的构建）。

由**子代理**（`docs/agents/agent-tool.md`）发起的请求会在标题里说明：
`Create file · from the general-purpose agent`。

**被停止的 agent，它的请求也随之离场。** 停止一个 agent —— roster 里的 `x`、lead
的 `agentkill` —— 会取消它的线程，线程随即放弃在 gate 上的等待；它请求要做的事情
永远不会发生。过去它的请求仍会留在屏幕上，变成一个答复送不到任何人的 prompt（在
herdr pane 里还会变成一个已经没人在问的问题的 `blocked` 状态）。现在
`App::withdraw_agent_permissions` 会把它们撤下：它排队的请求被丢弃，它打开的
prompt 关闭、下一个排队的 prompt 在原位打开，每个 id 都会像任何被放弃的请求一样在
gate 上释放。别的发起者的请求 —— lead 的、兄弟 agent 的 —— 原地不动。

**gate 也知道一个 prompt 何时真正到达屏幕。** 请求在线程把它发出去时被*发起*，
但只有在循环把它打开时才被*显示*，而一个排在别人后面的请求可能在打开之前就已了结
（它现在落入的某条规则、它的 agent 被停止、一次 Esc）。线程登记好当它的 prompt
被显示时应当发生什么（`PermissionGate::on_shown`，在请求发出之前），循环底部把
打开的 prompt 标记为已显示一次（`PermissionStore::mark_shown`，由它来执行），
而线程在请求结束时把它丢弃（`forget_shown`）。今天这就是 `Notification` hook 的
时机（`docs/tools/hooks.md`）；该任务跑在循环自己的线程上，所以它只把活计交给
别处去做。

## 屏幕上留下什么

一个 prompt 是*关于某个东西*的问题，所以这个模态从不遮住引出它的东西。它上方那些
活跃单元格会留下来：被询问的那次调用、排在它后面同批的兄弟调用，以及 —— 对于子
代理的请求 —— 本轮整棵活跃 agent 树（`● Running 3 agents…` 及其行）。活跃区域的
其他一切都让位：状态行（没有东西在跑；回合正卡在你这里）、composer、bands 和
footer。

那些单元格来自**屏幕上的对话**，这正是它们值得显示的原因。在主视图里，就是活跃的
agent 组加上 `App::tool_queue`，如上所述。在**子代理的 session 视图**里，则是该
agent 自己的队列、别无其他（`App::viewed_agent`）—— 与它的 strip 所预览的同一次
遍历，所以 prompt 看起来与被它取代的那个 strip 一模一样。这个分歧曾是一个被报告
的 bug，manual 模式在每条命令上都撞上了它：当一个*前台*子代理在跑时，lead 的活跃
`● Agent(Run ls -la via subagent)` / `⎿ Working…` 单元格会在该 agent 工作期间
一直挂着，于是站在这个 agent 的 session 里时，它的每次 `bash` 请求都开在 lead 的
单元格之上 —— 一个属于用户早已离开的屏幕的单元格 —— 而 agent 自己的
`● Bash(ls -la)` / `⎿ Waiting…` 及其同批兄弟却不见踪影。`context_chunks` 和
`context_is_stable` 都走被查看 agent 的分支，因为 flow 测试必须读到 context 实际
渲染的单元格：只要有前台子代理在跑，lead 的组就*永远*是活跃的，若以它来判断静态性，
就会把 agent 视图自己的静态单元格从每一个装不下的页面里一并丢掉。边界的其余部分
早已遵循这条规则 —— `ui::preview_lines`、`ui::preview_rows`、`queued_lines`、
`task_lines`、`context_lines`、`agent_transcript_lines` 和
`App::last_assistant_text` 都会在 `viewed_agent()` 上切换；prompt 是最后一个
没跟上的
（`docs/agents/agent-view-streaming.md`，`scripts/smoke.sh` Phase 98）。

是哪个 agent 发问是可知道的，因为请求本身带着它：`PermissionRequest::agent` 是
标题里那个*类型*，而 `PermissionRequest::agent_id` 是**哪一个**运行实例，由分发该
事件的边界（`tui::agent::Session::on_agent_event`）打上 —— 这是唯一知道它的地方，
因为两个 agent 可以共用一个类型。id 不是当前打开视图的请求 —— 主回合自己的调用，
或兄弟 agent 的 —— 它的单元格是在另一个屏幕上竖起来的，所以这个 prompt 打开时
完全没有 context（那种空闲形状），而不是去借被查看 agent 的无关批次。

每个排队的调用都渲染它平常那个折叠单元格 —— Claude Code 的样子。prompt 底下那个
显示的，正是它同批兄弟所显示的暗色 `⎿ Waiting…`：它确实*在*等待 —— approve 接缝
跑在它的 `ToolStart` **之前**，所以什么都还没开始 —— 而把它赤裸地渲染出来（旧的
只剩表头的特例）只会让一个两调用的批次读成一个等待中的调用加一个谜。真正在执行中
的调用（主回合自己的工具，位于*子代理的*请求之下）则保留它的运行行，只是静止绘制
—— prompt 是一帧静物（`docs/ui/tool-pulse.md`）。prompt 打开期间
`App::background_hint_elapsed` 读作 `None`，这样就丢掉了那个延迟出现的
`(ctrl+b to run in background)` 提示：prompt 独占了所有按键，所以那个提示等于在
宣传一个会被它吞掉的绑定。

但 context 也不能把 prompt 本身挤出去。模型一个大的并行批次（一轮里十五个 edit）
会排出一屏的 `⎿ Waiting…` 单元格，这在正文预算被计算之前就吃掉了整个终端：prompt
只显示了标题、问题和选项，**完全没有内容** —— 用户看不见自己正在批准的是哪次
edit —— 而兄弟一多，连选项都跑出了屏幕底部。所以这些单元格要算预算：
`permission_lines` 留出它固定的行数*以及*正文的下限（正文本身够短时用它的高度，
否则用 `PERMISSION_MIN_BODY_ROWS` —— 内联一瞥的大小），context 按队列顺序保留
完整单元格直到装不下，多出来的折叠成一行暗色的 `… +N more waiting`；天生比下限
还短的正文只预留它需要的高度，把余下的还给兄弟。第一个块 —— 发问的 agent 树，
否则就是被询问的那个调用本身 —— 永远不会被丢掉，这样 prompt 始终是关于屏幕上
某个东西的问题。

被询问的那个调用是从请求的 **call id** 读出来的（
`PermissionRequest::call_id` —— 与它的批次摘要和 `ToolStart` 携带的是同一个
id），而不是假定它就是队列头。一个并行批次按模型的顺序在任何一个调用开始
*之前*逐个询问每个调用的权限，所以第五个问题到达时，队列里已经躺着五个
`⎿ Waiting…` 单元格 —— prompt 把第五个提到它 context 的最前面，更早的单元格
排在它后面（`asked_first`，`docs/tools/parallel-tools.md`）。没有任何 wire 调用
发起的请求（一个脚本化的回合、一个续上的 prompt）保持旧的队列顺序。

当整页**流动**时（正文比终端高），context 随之流动 —— 前提是它*静态*。排队的
`⎿ Waiting…` 单元格就是静态的：approve 接缝跑在它的 `ToolStart` 之前，所以答案
悬而未决期间什么都不可能变，它**完整且不折叠**地进入 scrollback（已经没有一屏要
争了，把兄弟藏在摘要行后面纯属白白丢掉）。而**活跃的 agent 组**或**运行中的调用**
则不是 —— 树的圆点随帧脉冲闪烁，它的 `{n} tool uses · {tokens} tokens` 计数在
递增，运行中调用流式的输出在长出它的一瞥 —— 而流走的行在 scrollback 里是冻结的，
所以会变动的内容要么在那里变陈旧，要么每个 tick 都重新触发流动、引发一次 purge
重建（`context_is_stable`，`docs/ui/view-flow.md`）。那样的 context 让位，只有
静态的框流动。把*静态*单元格也一并丢掉曾是一次回归：在矮终端上 `● Write(…)`
表头从屏幕**和** scrollback 里都消失了，剩下的恰恰是本节存在的目的所要防止的那种
凭空冒出来的框。

## 它滚动、并归还的那个屏幕

prompt 是唯一能高到占满整个终端的内联视图 —— 正文是*完整*显示的 —— 而它和其他
每个区域一样增长（不变式 3）：有空位时就地向下长，之后每需要再多一行，就把一行
聊天从顶部**滚动**进终端的 scrollback。这在两种意义上都把对话留在了用户预期的
地方。在屏幕上，最新的消息就在问题正上方 —— 刚发出的消息、刚完成的单元格 ——
Claude Code 的那幅画面。而在屏幕之外，它们在**真正的 scrollback** 里：prompt
等待期间用户可以往上滚，重读任何内容。（之前的设计则是把对话*盖住* —— 区域向上
长并压住它，刻意不把任何东西推进 scrollback，这样关闭时可以在原处重绘被盖住的
那一段。代价只有在实际使用中才显现：被盖住的那几行活在**任何 buffer 之外** ——
既不在屏幕上，也不在 scrollback 里 —— 于是在 prompt 期间往上滚什么也找不到。它
读起来像是"提问期间终端的滚动被禁用了"，在 kitty 里最糟，直到答案或 resize 把
那些行带回来。）

滚动是单向的移动，而需要小心处理的正是这一点。当 prompt 关闭、区域回落成三行的
composer 时，没有任何东西能填回它腾出的行 —— 一次朴素的收缩会把框搁浅在屏幕
中间，下面是一片空行。所以边界会记下 prompt 打开期间发生的每一次单向移动 ——
`term` 的 `modal_scrolled` 标志，由 `ui::region_is_modal` 在这类移动发生的两处
置位：

- **一次绘制**：prompt 自身的增长触发了滚动，或一次提交在它下面落地
  （`term::paint_live`），以及
- **一次重建**：prompt 打开期间跑了一次 `reflow`（`term::reflow`）—— 一次
  prompt 中途 resize 的 purge，或一次 overlay 返回而它的 prompt 是在其下打开的
  （请求到达时 Ctrl+O / Ctrl+D 处于开启，所以这次返回的 reflow 就是该 prompt 的
  第一次内联绘制）。

prompt 关闭后的第一次绘制会消费这条记录（`InlineViewport::take_modal_scrolled`），
进行一次 **purge 重建**：scrollback 和屏幕一起从历史重建，框齐整地落到底部，
什么都不会丢失或重复 —— 与每次 resize 得到的是同一次重绘，在一帧内同步完成。
从未滚动过的 prompt（一个早期 session 的 composer，下方还有空行）不会留下记录，
它的朴素收缩只是清空本来就已经空闲的行。由 `smoke.sh` 的 Phase 58、60 和 62 守护。

有一个值得知道的后果：**prompt 挂着时提交照常流入**。区域和其他区域一样坐在底部，
所以一个批次背靠背的 prompt 之间，某个单元格的完成只是滚进仍未关闭的下一个 prompt
上方 —— 立刻可见、且只可见一次（`tui::commit::Session::commits_allowed` 的暂缓
清单上只保留 alternate-screen overlay 和 agent session 视图）。这样一次提交引起的
滚动正是记录所追踪的单向移动之一，所以最终的关闭仍能干净地 purge 重建
（`smoke.sh` Phase 59）。

关闭并不是单向滚动唯一咬人的时刻：区域也可能**在 prompt 仍然打开时收缩**。背靠背
的 prompt 高度不同 —— 一个受正文上限约束的 prompt 填满终端，而一个单行文件的
prompt 只有十几行；回答高的那个会把它的单元格提交出活跃区域、并打开矮的那个，
往往就在同一个帧间隙里（子代理那个顶端带树的 prompt 与主回合的 prompt 交替出现，
高度同样会变）。就地绘制时，这一帧会把*打开着的* prompt 搁浅在一片空行之上，只要
它还在发问就一直如此 —— flush 的滚动计划把区域安置在已提交行之下，并且只预留新的、
更矮的高度（其尾部清屏会把下面的一切清空），而分开成帧的排序则会重新钉住并收缩进
同一片空行，甚至不设置那条记录（没有 flush，也没有滚动）。所以循环的绘制 tick
在绘制*之前*就做决定（`ui::modal_needs_rebuild`，由
`tui::view::Session::modal_rebuild_due` 提供输入）：当区域处于模态时，如果最后一次
**已绘制**的帧触到了屏幕底部（`InlineViewport::painted_bottom` —— 被追踪的 `view`
高度会在两次绘制之间为 flush 计划重新同步，所以它派不上用场），而本帧的计划 ——
`view_top + pending rows + new height` —— 会让它坐得低于那个底部，那么这一帧就用
与关闭时相同的 purge 重建来应对，把打开的 prompt 重新齐整安置，同时已了结的单元格
在它上方可见。重建在打开的 prompt 之下进行，所以 `reflow` 会重新布置那条记录，
最终的关闭仍会 purge；一个浮在底部之上的 prompt（对话很短 —— 从未滚动过）保持它
的朴素收缩，清空本来就空闲的行。由 `smoke.sh` Phase 63 守护（dummy 的
"staggered permission demo"：一个满屏高的 `write`，其回答落进一个单行的 prompt）。

## 选项

每个 prompt 都给三个选项，用 ↑/↓ + Enter 选择（步进会绕回 —— ↓ 越过 **No** 会
回到 **Yes**），或者直接按 `1`/`2`/`3`：

1. **Yes** —— 只批准这一次调用。
2. 对 `write`/`edit` 是 **Yes, allow all edits during this session (shift+tab)** ——
   这*就是*切换到 [edit 模式](#权限模式manual--edit--auto--master)，而 Shift+Tab
   （模式切换键）会直接选中它；
   对 `bash` 是 **Yes, and don't ask again for: {rule}** —— 规则就是它将被存储的
   样子：前缀规则用 `python3 *`（星号意为"这个程序，任意参数"，即 Claude Code 的
   `Bash(prefix:*)`），只有精确匹配才安全时则是整条命令。没有字母快捷键：旧的
   `(a)` 绑定与一条长期有效的批准只差一次手误，所以记住那一行要用数字或 ↑/↓ +
   Enter 来选，和 Claude Code 一样；
   对 MCP 调用是 **Yes, and don't ask again for {server} - {tool} commands in
   {project}** —— 工具按用户认识它的名字来称呼，项目是规则为其保留的那个
   （`App::project_dir`），而存储下来的规则仍是精确的 `mcp__…` 线上名
   （`docs/tools/mcp.md`）；
   对 session 输入是 **Yes, and don't ask again for this session** —— 用 session
   id 记住，只要 session 在跑就一直留在 gate 上，从不写入 `permissions.json`
   （一个 id 对下一个进程毫无意义）。
3. **No** —— 拒绝；模型被告知停下并等待。

长选项会**折行**而不是截断（`option_rows`，命令正文的 `wrap_output`）：仅精确匹配
的规则就是整条命令，而旧的一行 `…` 截断恰好把正在被批准的文本藏起来了。续行在
编号之后与标签对齐，只有第一行带 `❯`，选中时整块发亮 —— 而光标静止时的落点仍落在
被高亮选项的第一行，因为落点和渲染器共用每项的高度（`option_heights`，由
`ui::cursor_position` 读取）。一个*病态*的标签（几 KB 长的精确命令）会在
`PERMISSION_OPTION_MAX_ROWS` 处封顶，带上那个熟悉的 `…` —— 区域会被钳制到终端内
并自上而下绘制，所以无界的折行会把 `3. No` 和提示行挤出屏幕底部。

此外，在提示行上：

- **Esc** —— 取消：拒绝*并*中断本回合（普通的 Esc 中断路径，
  `docs/input/interrupt.md`）。
- **Tab** —— 修改：选项列表被 composer 替换，这样你可以输入模型应该改成做什么。
  Enter 会拒绝，**并**把这段反馈附在工具结果上、*同时*记录在单元格上；Esc 回到
  选项。见 [修改反馈的价值是什么](#amend-反馈值多少)。
- **ctrl+e**（仅 `bash`）—— 解释：拒绝，并附带一条指示，要求它解释这条命令做什么
  然后重新询问，而不是执行它。

### "don't ask again" 记住了什么

`permission::command_scope` 把一条命令化归为 allowlist 所存储的规则：

- 命令按 `|`、`||`、`&&`、`;`、`&` 和换行切分成段，**切法跟 `bash -c` 一样**，
  每一段都化归为它的**前缀**（见下文*按 bash 的方式切分*）：程序词 ——
  `python3 script.py` → `python3`，`ls -la` → `ls`，`mkdir build` → `mkdir` ——
  文件/参数是*参数*，不是规则的一部分。（旧的前缀会保留非 flag 的第二个词元，
  所以批准了 `python3 script.py` 永远覆盖不到 `python3 other.py`：每个新脚本都要
  重新问一遍，这让"don't ask again"几乎没用 —— 这次重做修的正是这个缺陷。）
- 精选的**子命令工具**（`git`、`cargo`、`npm`、`docker` 等）保留它们的动词 ——
  `git status --short` → `git status`，`npm run build` → `npm run` —— 因为
  `git *` 会覆盖 `git push --force`；动词让规则与所批准的动作一样窄。动词位置上
  的 flag 或路径则退回程序本身（`git -C /tmp status` → `git`）。
- 后续命令只有在它的**每一个**段前缀都在 allowlist 中时才会被自动批准，所以把
  `python3` 加入 allowlist 绝不会悄悄放行一个 `; rm -rf /` 尾巴。
- 批准时会存储每一段的前缀（所以完全相同的命令不会再问），而选项的**标签**只用
  `*` 指明最后一个 —— 用户读作动作的那条命令，与 Claude Code 一致
  （`echo "" | python3 script.py` 会提供 `python3 *`）。
- 带重定向或替换的段（`>`, `<`, `` ` ``, `$(`）无法用前缀概括 —— 前缀不会提到
  真正要紧的那部分 —— 所以作用域降级为 `Exact`：标签和存储的规则都是**整条命令**
  （没有星号），只有逐字节相同的命令才会被自动批准。扫描是引号感知的：
  `echo "a > b"` 不重定向任何东西，仍是一个前缀作用域（旧的裸 `contains('>')` 毫无
  理由地把它降级了），而 `"$(whoami)"` 在双引号内仍然做替换、仍然降级；只有单引号
  才能拆除一次替换。
- **没有前缀能诚实概括**的段以同样方式降级：前置的环境赋值（`FOO=1 python3 …`
  —— `PATH=…` 可以改变程序*是什么*）、其"参数"本身就是一条命令的命令包装器
  （`sudo`、`sh -c`、`xargs`、`env`、`timeout` 等 —— 一个 `sudo` 前缀会把 sudo
  能携带的一切都加入 allowlist），或者一个领着段却不是其程序的词：bash 的保留词
  （`if`、`then`、`do`、`while`、`for`、`!`、`{` 等）、`.`（它 source 一个文件）
  以及 `builtin`。为它们存储的规则会覆盖在它后面跑的任何东西：批准 `if [ -f x ];
  then make; fi` 过去会存下 `then`，于是 `if true; then rm -rf ~; fi` 随后就会
  不经询问地跑起来。子 shell 的括号同样会降级作用域，因为 `(git` 不是 `git`，
  它的动词会丢掉。

用选项 2 批准还会**清扫已经等着的请求**。并行 agent 在任何一个被回答之前就已各自
发起了请求 —— 每个线程在第一个 prompt 被画出来之前就查过规则了 —— 所以没有清扫
的话，跑同一条命令的三个 agent 会在你说过不要之后再问三次。循环记住这个作用域，
然后释放每一条现在被新规则覆盖的打开/排队请求（`App::drain_covered_permissions`，
用 gate 自己的 `allows`）；没被覆盖的仍然要问。

session 输入也受它所在 session 的**启动命令**所适用的规则覆盖：`python3 *` 已经
信任 `python3` 带任何参数 —— 包括 `-c "anything"` —— 所以往一个 `python3` session
里输入东西，并没有增加规则未曾授予的任何权限。

没有内置的安全命令清单。在 manual 模式下，每一条 `bash`、`write` 和 `edit` 都要
问，直到规则另有说法 —— 这是 Claude Code 的默认姿态，也是唯一不会在"什么安全"
上出错的姿态。`read` 从不受门控。

#### 按 bash 的方式切分

规则是逐段匹配的，所以切分器读作一段、而 bash 却当作**两段**执行的段，就会让规则
覆盖一条没人批准过的命令。切分器过去只懂引号，有三种写法从它手里溜了过去：一个
**转义引号**（`echo \"; rm -rf ~; echo \"` —— 对 bash 来说 `\"` 是个字面字符，
所以它后面的 `;` 是真的）、一个**注释**（`echo #'` 换行 `rm -rf ~` —— 注释里的
`'` 什么也没开），以及 **ANSI-C 引号**（`echo $'\'' ; rm -rf ~` —— 在 `$'…'`
里面，`\'` 并不闭合）。每一种都被读成单独一个 `echo` 段，于是一条已保存的
`echo *` 规则会在任何模式下不经 prompt 就跑起 `rm -rf ~`；最后一种还藏起了一个
重定向（`echo $'\'' > /etc/passwd`）。`permission::command_segments` 现在按
`bash -c` 在切分方面的做法来读一条命令：单引号、双引号和 `$'…'` 以及各自接受的
转义，引号之外的反斜杠转义下一个字符（一个续行被整段移除），而一个 `#` 开启一个
词、把该行剩下的内容注释掉。永不闭合的引号是 bash 拒绝的一行，所以它的作用域是
精确命令。

## 权限模式（manual · edit · auto · master）

session 有一个**权限姿态**，`permission::PermissionMode`，齐整地钉在 footer 的
**右边缘**（`{model} · {cwd}      auto` —— 它自己的区域，从左侧链条的截断预算中
预留出来，所以长 cwd 的 `…` 裁切永远吃不到它；`docs/ui/footer.md`），并且在
composer 里用 **Shift+Tab** **循环**切换，每按一次在自主性上走一级。标签的颜色也
随之升温（`footer_mode_color`）：`manual` 和行内其余部分一样暗，`edit` 蓝，`auto`
琥珀，`master` 红 —— Claude Code 的 bypass-permissions 装扮，让那个什么都不问的
姿态成为整行最响的东西：

- **manual** —— 每个 `write`/`edit` 和每条 `bash` 命令都问，如上所述。最谨慎的
  姿态，也是读不出模式时退化到的那个。
- **edit** —— Claude Code 的 "auto-accept edits on"：`write`/`edit` 不问直接跑，
  `bash` 命令仍然要问（直到被加入 allowlist）。
- **auto**（默认）—— Claude Code 的 auto 模式：`write`/`edit` 像 edit 模式那样
  跑；只做读取的 `bash` 命令不经复核就跑（下文*只读命令跳过 classifier*）；其他
  任何命令 —— 或一次 MCP 工具调用（`docs/tools/mcp.md`）—— allowlist 尚未覆盖
  的，都由 **auto 模式 classifier** 复核 —— 一个代替用户做的静默 LLM 安全检查
  （下一节）。auto 模式下不会打开 prompt，除非 classifier 自己失败。
- **master** —— Claude Code 的 bypass-permissions 姿态：一切不问就跑。没有
  prompt，没有 classifier；用户把安全带解开了。

循环会绕回（`master` → `manual`），所以一个键走完整条梯子。session 以 auto 启动，
所以第一次按落在 master，第二次落在 manual。
`PermissionRules::allows` 编码了既有的覆盖范围：文件改动被 manual 之上的每一种模式
覆盖；命令或 MCP 调用只被 master 或 allowlist 覆盖 —— auto 模式的 classifier 是
**approve 接缝中的一次逐调用咨询**，从不是一条长期规则，这正是 classifier 失败能
退回 prompt 的原因。

`write`/`edit` prompt 上的选项 2 **就是**切换到 edit 模式 —— 该模式正是旧的
"allow all edits during this session" 标志，只是变得可见且可逆 —— 所以选择它
（或在 prompt 上按 Shift+Tab）会批准待决的改动、翻转 footer 的那一段，并弹出确认
toast（`Mode: edit — file edits run without asking (shift+tab to switch back)`）。
在 **bash** prompt 上，Shift+Tab 只步进姿态 —— 步进到一个覆盖了当前打开请求的模式
（master 覆盖一切）时，会经由普通的清扫让它了结；否则 prompt 继续打开 —— 而在
composer 里，它无论空闲还是回合中都有效（gate 的规则是共享状态；下一次 `approve`
咨询就会服从新姿态）。**循环回到 manual 会让文件改动重新发问**：模式是 gate 所咨询
的唯一真相来源，不是一个单向的闩锁。每一条切换路径都落到
`Action::SetPermissionMode`（选项 2 则落到 `ResolvePermission` 分支），循环在那里
把模式镜像到 gate 上、**清扫**新模式下已被覆盖的排队请求（并行 agent 的文件改动
原本等在打开的 prompt 后面，此刻一并批准 —— 上面说的选项 2 清扫；切到 master 会
清扫每一个排队的 prompt）、持久化它（见下文），并 toast。权限被禁用时就没有模式：
footer 那段被隐藏，Shift+Tab 改成弹出一个 `Tool permissions are disabled` toast，
而不是默默什么都不做。

### 一个 session 从哪里开始

一个新 session 的模式取自以下第一项：

1. 命令行上的 **`--permission-mode <MODE>`** —— 下面那个变量的 flag 形式，优先级
   更高（`docs/cli/cli.md`）。解析会直接拒绝未知的模式，并点名那四个。
2. **`ZEROTH_PERMISSION_MODE`** —— `manual`、`edit`、`auto` 或 `master`，
   大小写不限 —— 仅对这次启动有效。这两个种子都优先于项目保存的模式，并且**从不
   被保存**：只有用户自己选定的模式才会写进 `permissions.json`（见下文）。空值
   视为未设置；其他任何值都以 manual 启动本次运行，一个读不出的已保存标签同样
   遵循这条规则（`permission::mode_by_env`）。
3. **这个项目在 `permissions.json` 里保存的模式**。
4. **auto**。

`PermissionsFile::starting_mode` 就是这个顺序，纯粹且有单元测试；
`PermissionStore::open` 接收 flag 的种子、读取该变量和文件，并在第一帧之前用答案
初始化 gate，而 footer 的那一段读取 gate。

auto 是默认值，因为 manual 会为每一次文件改动、每一条命令中止回合，所以任何规模
的任务都变成一连串的批准。auto 让文件改动跑起来 —— footer 写着模式，Shift+Tab 只
差一个键 —— 并把每条 allowlist 未覆盖的命令送到 classifier 面前，后者在得不出结论
时随时退回普通的 prompt。代价是每一次被复核的调用都向 session 自己的 provider 发
一次 classifier 请求。smoke 套件固定设置 `ZEROTH_PERMISSION_MODE=manual`，
好让它的 prompt 相关阶段仍能遇到它们的 prompt（`docs/ops/smoke.md`）；Phase 64
不带它启动，以覆盖默认值。

## 自动模式：classifier

在自动模式下，一条本来会弹出 prompt 的 `bash` 命令 —— 或者一次 MCP tool
call（`docs/tools/mcp.md`），或者输入到交互式 session 里的按键
（`docs/tools/interactive-shell.md`：classifier 会被告知这些按键发往哪个
程序，因为往 shell 里敲 `rm -rf ~⏎` *就是* 在运行它）—— 改为交给 **auto
mode classifier**：Claude Code 的自动模式审查器，在 session 自己的 provider
上重建（`llm::classifier::SafetyClassifier`；参考实现把它的 MCP 调用也
喂给同一个审查器，`mcpToolInputToAutoClassifierInput`）。approve 接缝
（`llm::approval::approve_call`）在 allowlist 检查和 prompt 之间询问它：

```
approve(bash/MCP call) ── gate.allows()? ── yes ─────────────────► runs (no note)
                           └ no · mode == auto
                              only reads? ── yes ──────────────────► runs (no note)
                              └ no
                              classifier ── allow ──► runs, cell notes the classifier
                                          ── deny ───► rejected with the reason
                                          ── error ──► the ordinary prompt (fallback)
```

### 只读命令跳过 classifier

一条**可证明只读**的 `bash` 命令 —— `ls -la`、`grep -rn foo src`、
`cat README.md | head -20`、`git log --oneline -5`、`rg --files` —— 在自动
模式下根本不会发出 classifier 请求，因而也没有 `Allowed by auto mode
classifier` 那条 note（`llm::approval` 的 fast path，位于 scratchpad 检查
之前、`PreToolUse` 强制 ask 之下）。为了一个从不会有疑问的答案，每次调用都
问一次模型 `ls` 是否安全，要付一次往返的代价；而 agent 在改动任何东西之前
所做的探索，正是一连串这样的调用；Claude Code 对同一组命令也跳过它的审查器
（`checkReadOnlyConstraints`），所以这既是参考实现的行为，也是更省的那个。

“可证明”指的是纯粹的 [`read_only::is_read_only`](../../src/read_only/mod.rs)
检查，Claude Code 自身的移植：命令按 shell 的方式被切分成 token
（`read_only::lex` —— 引号、转义、操作符、重定向、`$`/反引号展开、glob、
花括号展开、注释），在 `&&` / `||` / `;` / `|` 处分割，并且**每一段**都必须
是 allowlist 上的程序，其所有 flag 也都通过该程序自己的表
（`read_only::flags`、`read_only::git`）。它遇到第一处疑问就拒绝，而不是更
用力地解析：

- 任何它无法求值的展开 —— `$HOME`、`$(…)`、反引号、glob、`{a,b}` ——
  因为那时命令*所说*的已不是实际运行的东西；
- 除无害者（`2>&1`、`> /dev/null`、`< /dev/null`）以外的任何重定向 ——
  `ls > out.txt` 是在写；
- 按程序而定的写或执行 flag：`find -delete` / `-exec` / `-fprint`、
  `sort -o`、`tree -o`、`rg --pre`、`fd -x`、超出 `-n …p` 与一次 stdin
  `s///` 的 `sed`、带 `system`、`getline`、`|` 或 `>` 的 `awk`、超出只读
  动词（`status`、`diff`、`log`、`show`、`blame`、不带目标的 `branch`、
  `config --get`……）的 `git` 动词；
- 已知程序上的未知 flag —— 这些表永远是 allowlist，绝不是 denylist，所以
  某个程序下个版本新增一个写 flag 也混不进去；
- `git` 处在**布局成 bare 仓库**的目录里（`objects/` 或 `refs/` 旁边有个
  `HEAD` 文件，没有 `.git`）：那里 git 会读该目录自己的 `config`，其中的
  `core.fsmonitor` 或某个 hook 可以执行任意代码 —— 参考实现针对同一攻击的
  防护。

它**只属于自动模式**：`manual` 和 `edit` 仍然对每条命令都会询问（allowlist
对一条读取命令的答复与从前完全一样，用户保存的 rule 依旧覆盖它），而
`master` 本来就什么都不问。一个回答 `ask` 的 `PreToolUse` hook 仍然会问
—— 强制 ask 意味着要有人来答，见 `docs/tools/hooks.md`。同一个检查也决定
折叠后的 tool 视图把哪些调用归成一组，是列表、搜索还是读取
（`read_only::read_kind`、`docs/ui/tool-groups.md`），而两者刻意只隔一个
函数：分组想知道一次调用是*哪一类*读取，并容忍那些不影响显示的东西
（`grep -c foo *.rs`、一个 glob），而 fast path 必须确信没有写、也没有
执行。

这个请求是**一次静默的 completion** —— 没有事件到达 UI，所以被询问的那个
cell 只是保留其批次公告给出的 `⎿ Waiting…` 行，直到裁决定下。它唯一的一条
user 消息有两部分。`## Task context` 块是 session 的近况 —— 也就是 **task
context**（`classifier::ClassifierContext`）：近期的用户请求，逐行引用
（每行前置 `> `，这样一条自身带 markdown 的请求会清楚地保持为被引用的
材料，而不会变成结构），加上 agent 已采取动作的一行一条，各自使用
transcript cell 自己的 `Name(args)` 词汇（`Read(/path)`、`Bash(cargo test)`、
`deepwiki - ask_question (MCP)({…})` —— 与 cell 用的是同一对
`display_name`/`summarize_call`，所以 classifier 和用户用同样的话读这一
轮），被拒绝的调用标为 `— denied, not run`（agent 重试某条被拒命令的变体
时，应该被*看见*在这么做）。

两半都是**在会话之上的滚动窗口**，而不是一轮。对其中任何一半来说，轮边界
都是错误的复位点：解释一条命令的请求常常在两轮之前 —— “把项目搭起来” →
… → 一条作用在构建产物上的 `rm -rf` —— 而一个命令被拒、一轮之后又重试其
变体的 agent，也应仍被看见在这么做。所以每条新的 user 消息是*压入*窗口而
不是清空它，只由上限来界定这个块：最新的 `CONTEXT_MAX_REQUESTS`（10）条
请求和 `CONTEXT_MAX_ACTIONS`（20）个动作，每行截断，在窗口截断处带计数
的 `(+N … omitted)` 标记。因此一次裁决在第五十轮的代价与第一轮相同。
然后，在它自己的 `## Action to review` 标题之下，使正在*裁决*的东西绝不会
与已经发生的事混在一起：cwd 和请求本身 —— 对命令而言：命令与模型自述的
`description`（标明是主张，不是证据）；对 MCP 调用而言：名为
`{server} - {tool}` 的 tool、server 自己对它的描述，以及用 cell 的
`key: "value"` 形式给出的参数（`classifier_request_prompt` 的 MCP 分支
—— 光有 wire name 会掩盖远程调用的风险究竟在哪）。上下文让裁决**具备任务
感知** —— `cargo build` 刚失败之后的 `rm -rf build/` 读起来与凭空冒出的
`rm -rf` 不同 —— 又不会重新打开旧的被投毒 transcript 漏洞，因为这个块是
**有界且惰性**的：每一部分都截断（每条请求一个 `CONTEXT_REQUEST_MAX_CHARS`
摘录，每个动作行 `CONTEXT_ACTION_MAX_CHARS` 并以 `…` 收尾，还有上面那两个
窗口），tool 的*输出*从不搭车（那是被投毒的仓库最省力的游说通道），并且
system prompt 把整个块钉死为信息而非指令 —— 其中任何内容都无法授权一个
动作（live 套件证明，一个*哀求*放行的上下文什么也改变不了）。这份日志放在
**backend** 上，每个 session 一份：每次 spawn 把它的 user 消息压入窗口
（subagent 保有自己的那份，由其启动 prompt 或续聊 chat 播种 ——
`classifier::latest_user_text`，在 skill 提醒和 hook 说明压入更多 user 角色
消息之前读取），execute/launch 闭包记录每个已执行调用和 agent 启动，approve
闭包记录拒绝。它的 system prompt（`prompts/classifier.md`，每个 prompt 都用
的 `include_str!` 接缝）以参考实现的严格输出约定收尾 —— 回复必须以
`<block>yes</block><reason>…</reason>` 或 `<block>no</block>` 开头 —— 这在
任意 OpenAI 兼容模型上都比 JSON 存活得好得多；解析
（`classifier::parse_verdict`）仍容忍爱闲聊的模型的前言，而任何没有可识别
裁决的回复都是**错误，绝不是 allow**。该调用运行在 session 的 `ModelConfig`
的一份无 tools 克隆上，且 thinking 被丢弃（裁决不需要推理预算）；
`ZEROTH_CLASSIFIER_MODEL` 可在同一 provider 上换成一个更便宜的 model
id —— Claude Code 的 small-fast-model 槽位。取消走与每个请求相同的
`CancelToken` 轮询，而一次*因为*该轮被拆掉才失败的 classify，处理方式与被
收割的 gate 等待相同 —— 不会有 prompt 被拉进一个正在死去的轮。

一次**被允许**的调用运行起来与用户批准的调用完全一样，额外多一条来源说明：
`run_agent` 在 `ToolStart` 之后立刻发出 `StreamEvent::ToolNote`
（`Approval::AllowNoted` 变体），循环把它留在正在运行的调用上
（`App::set_tool_note` → `ToolCall::approval_note`），调用一解决，cell 就
追加一条新的暗色 `⎿` 行 —— 这是 transcript 对“没有人批准过它”的记录：

```
● Bash(ls -la)
  ⎿  total 40
     drwxr-xr-x 1 user user  256 Aug  4 17:14 .
     …
     … +12 lines (ctrl+o to expand)
  ⎿  Allowed by auto mode classifier
```

这条 note 在折叠 cell 和 Ctrl+O transcript 中都一样渲染（`ui::tool` 在每个
分支的正文之后追加它，只针对已解决的状态 —— 正在运行的 cell 保持其实时
外观），随 rollout 记录为 `ToolRecord::approval_note`（不存在时省略，所以
旧文件仍可解析），因而能挺过一次 `/resume`；subagent 被允许的调用也通过
同一个 `ToolNote` 事件把它带到自己的 transcript 上（`AgentRun::apply`）。
有一个 cell 会跳过它的内联显示：**安静的已解决 MCP cell** —— 它全部的内联
存在就是一行暗色的 `Called {server}`，而在自动模式下每次 server 调用都带
note 解决，这一行会把每个 cell 翻倍成噪声（一次并行运行的聚合行本来也从不
携带它）；Ctrl+O transcript、rollout 以及失败的 MCP 调用那条响亮的 cell
仍保留这条记录（`docs/tools/mcp.md`）。

一次**被拒绝**的调用绝不运行：该接缝返回 `Approval::Reject`，以
`Denied by auto mode classifier`（第二行是 `Reason: {…}`，与 amend 反馈的
形状相同）作为红色 cell，并给模型一条更长的结果 —— 改编自 Claude Code 的
自动模式拒绝文本，措辞不偏向命令或 MCP 调用中的任何一方，因为它对两者都作
答 —— 告诉模型这次调用未执行、其他工作可以继续、换一种更安全的做法没问题、
不得绕过拒绝的本意，而若某能力确实必需就停下来问用户（用户可以切到 manual
模式批准、自己动手，或按 Shift+Tab）。两段文本都像任何拒绝一样随被记录的
调用一起走（`context_output`），所以后续轮次重放的正是模型当时被告知的内容。

一次 classifier **失败** —— 网络问题、无法解析的回复 —— 回退到普通 prompt：
问用户是安全姿态，也是离线仍能奏效的姿态。**dummy backend** 从不走 HTTP，
所以它的自动模式演示（一个名称里带 “auto” + “permission” 的 prompt）改为
询问确定性的离线启发式（`permission::auto_verdict` —— 一张小小的只读前缀
表）：脚本里的 `ls -la` 走只读 fast path 且不带 note 运行，脚本里的
`pytest -q` 被启发式允许并带着 note 运行，脚本里的 `rm -rf /tmp/scratch`
被拒绝，而同样的演示在 manual 模式下对三者都弹出 prompt —— 正因如此，
`smoke.sh` 无需 provider 就能驱动整个特性（Phase 64）。
live OpenRouter 套件（`tests/live_openrouter.rs`）覆盖真实情况：命令*和*
MCP 调用的两个方向裁决 —— 带 task context 与裸的两种 —— 外加两个要紧的
上下文性质（一个哀求放行的上下文什么也抬不起来，一个 `— denied, not run`
动作的重试变体仍被封锁），以及一整轮自动模式，其事件显示
`ToolStart → ToolNote → ToolEnd`，全程不见 `Permission` ——
`tests/live_mcp.rs` 用一个真实的 server tool 端到端地归类收尾
（`live_auto_mode_classifies_an_mcp_call_instead_of_prompting`）。

## 看清 classifier 看到的东西 —— Ctrl+D、Tab

task context 决定一条命令是否不问就运行，所以它是裁决中用户否则读不到的
那一项输入：请求是静默的，cell 只显示结果。它只隔一个键，作为 **Ctrl+D
视图的第二页** —— Tab 在两者间切换：

```
/ C L A S S I F I E R / / / / / / / / / / / / / / / / / / / / / / / / / / /
Auto mode — the classifier reads this before each command or MCP call.

 SYSTEM  #1 · classifier prompt · abridged
  ⎿  You are the safety classifier for an autonomous AI coding agent in auto
     mode: it runs shell commands and MCP tool calls without asking the user,
     and you decide — one action at a time — whether the action runs or is
     blocked.
     … +6 lines

     ## Shell commands
     Allow ordinary, project-scoped development work: reading, searching and
     listing files; `git` work in the working directory; building, testing, lin…
     … +8 lines

     ## MCP tool calls
     For a tool exposed by an MCP server — named `server - tool`, with the
     server's description and the call's arguments:
     … +8 lines

     ## Output Format
     If the action should be blocked:
     <block>yes</block><reason>one short sentence</reason>
     … +4 lines

 USER  #2 · task context
  ⎿  ## Task context
     User requests (oldest first; the last is the current task):
     > set up the project
     > now clean up the build output

     Recent actions (oldest first):
     - Read(/home/user/proj/Makefile)
     - Bash(make)
     - Bash(sudo rm -rf /var/log) — denied, not run

     ## Action to review
     (the command or MCP call being judged — filled in when a verdict is asked)

──────────────────────────────────────────────────────────────────── 100% ─
 ↑/↓ to scroll   ←/→ prev/next message   pgup/pgdn to page   home/end to jump
 q/esc/ctrl+d to quit   tab for llm context
```

把两者放在同一个键下正是要点：两者都回答*这一轮实际发送了什么* —— 一个
是模型自己的上下文窗口，另一个是其审查器的 —— 所以它们共用外壳
（`ui::render_context_view` 绘制其中任一；只有标题、滚动偏移和 Tab 提示的
方向不同），只在正文上不同，正文由 `ui::classifier_body` /
`ui::context_body` 构建 —— 出自同一批 entry 构建函数，所以每条消息都以同样
的角色 chip 开在同样的 `⎿` 装订线之上，而 ←/→ 在这里于两者之间步进，就像
在其姊妹页的几十条消息之间一样（`docs/architecture/context.md`）。这一页
遵循这些规则：

- **评分标准，节略显示。** 那个块只是审查者所见的一半 —— 另一半是它据以
  裁决的 system prompt（`prompts/classifier.md`），看不到它的读者能知道一条
  命令*被*拦了，却不知道是凭什么。所以这一页像其姊妹页那样绘制请求：prompt
  作为琥珀色 `SYSTEM` entry —— 其头部写作 `classifier prompt · abridged`，
  而姊妹页那里估算的是 entry 的大小，因为节略 prompt 的大小并不是被发送的
  东西 —— 然后把那个块作为 `USER` entry，标题为 `task context`，最后由
  `## Action to review` 标题收尾，其下是消息中唯一只在请求裁决时才知晓的
  那部分的暗色占位符（标题是一个常量
  `classifier::ACTION_TO_REVIEW_HEADER`，与 `classifier_prompt` 共用，所以
  这一页和请求不会漂移）。prompt 被**节略到其结构**而不是整份显示 ——
  3.6 KB 的评分标准有六十行，会把这一页存在的理由（实时块）挤出屏幕：
  `ui::classifier_view::abridge_prompt` 完整保留每个标题（各节*就是*结构），
  每节的开头段落保留到 `CLASSIFIER_PROMPT_PEEK_COLS`（240 —— 80 列终端的
  三行；整行能放下就整行，溢出的行截断并以 `…` 收尾），把该节其余部分折进
  一条暗色的、带计数的 `… +N lines` 行 —— 即 tool cell 的惯用法去掉它的
  `(ctrl+o to expand)`，因为并没有可展开的地方 —— 所以不会有东西悄无声息地
  消失（`the_real_prompt_abridges_to_its_four_sections` 钉住：真实 prompt 的
  每一行内容要么被显示，要么被计数）。它进入这一页的方式与主 prompt 相同：
  `ReplySource::classifier_system_prompt`（`LlmBackend` 给出
  `SafetyClassifier::classify` 实际发送的那份；dummy 的默认值为 `None`，
  因为它没有 classifier）在启动时以及 `/model` 切换时同步进
  `App::classifier_system_prompt`（`sync_backend_info` —— 每个 backend 一个
  常量，无需每次绘制都拉取）。subagent 的这一页在它自己的块之上显示同样的
  prompt，因为每个 classifier 发送的都是同一份。角色标签、折叠标记和占位符
  随正文一起换行；极窄的屏幕会去掉缩进，以免它吃掉可用的文本宽度。

- **这是谁的窗口。** 在 agent session 视图内，这一页显示**那个 agent 的**
  上下文，而不是 lead 的 —— 它自己的启动 prompt 作为请求，它自己执行的调用
  作为动作。subagent 的裁决是依据它自己的运行来审查的（`spawn_subagent_run`
  用它被启动时的 prompt 播种一个上下文，并为它发出的每次调用记录一行），
  所以在那里显示 lead 的日志会描述那个屏幕上没人做过的决定。这份上下文放在
  agent 的 **registry slot** 上，在 `AgentRegistry::register` 中随其 id 一起
  铸造，并通过 `AgentRegistry::classifier_context` 读回 —— 它以前是 agent
  线程里的一个普通局部变量，这恰恰是外部读不到它、这一页因而落到
  `ReplySource::classifier_context`（lead 的那份）上的原因。一次 chat 续聊
  压入的是同一组滚动窗口，因为那是同一个 agent 的会话；registry 不再持有的
  agent 会渲染空占位符，而不是退回 lead。另一个 Ctrl+D 页一向就是这么切换
  的（`ui::context_lines` 的第一个分支以 `viewed_agent` 为键）；这一页现在
  与之对齐。由 `agents::tests::each_agent_gets_its_own_classifier_context`
  以及真实链路上的端到端测试
  `live_a_subagents_classifier_context_is_its_own_not_the_leads` 覆盖。

- **模式说明。** 日志在*每一种*模式下都会被记录 —— 边界按每次调用而非每次
  裁决喂它 —— 但只有自动模式会查阅它。一个什么都不说的页面，在用户做决定的
  那些模式下会被读成“classifier 在决定这件事”，所以块上方那一行说明当前是
  哪种情况：`Auto mode — …`、`Recorded every turn; consulted only in auto
  mode (shift+tab to switch)`，或 `Tool permissions are disabled — no
  classifier runs`（根本没有 gate）。
- **Tab 在 permission prompt 之上也能到达它。** Ctrl+D 本来就能逃出模态
  （连同 Ctrl+O，两个只读视图），而视图一旦立起，它自己的 handler 就拥有
  按键 —— 模态的路由只在会话视图里运行 —— 所以 prompt 的 Tab（它的 amend
  字段）和这一页的翻转从不冲突。这一点在这里最要紧：自动模式下的 prompt
  意味着 classifier *失败*了，或者该调用是它从不查看的文件改动，而“它当时
  知道些什么？”正是被问的问题。它是只读的，所以被阻塞的 tool 线程继续等待，
  回来时 prompt 仍然开着。
- **每一页各自保留滚动位置**，所以翻过去比较再翻回来会落在原处
  （`App::debug_page_scroll` 把当前那一页交给分页器的各个分支），而页面本身
  跨多次打开保持 —— Ctrl+D 回到你上次读的那一页，标题会说明是哪一页。
- **classifier 页是实时读取的。** 它的块在每次绘制时从 backend 拉取
  （`ReplySource::classifier_context` → `App::set_classifier_context`，即
  system-prompt 注入的模式）而不是缓存，因为它随轮次增长，并随会话推进滚动
  其窗口 —— 一个开着的页面会随着动作落地而尾部跟随。它不需要
  `ContextCache` 姊妹：这个块在构造上就被限制为
  `CONTEXT_MAX_REQUESTS` + `CONTEXT_MAX_ACTIONS` 条短行，这正是这些上限的
  意义。

那次实时读取正是 `LlmBackend` 把 `ClassifierContext` 放在
`Arc<Mutex<…>>` 后面而不是每次 spawn 一个局部变量的原因：它必须比一轮活
得久（两半都是在会话之上的窗口），`spawn` 把每条 user 消息压入它，tool
闭包追加，边界把它读出来。dummy backend 不保留日志 —— 它的离线自动模式
演示依据纯 `permission::auto_verdict` 启发式作答，也不注入任何 prompt ——
所以这一页在那里显示其暗色的 `No classifier context yet` 占位符，位于同样的
模式说明之下，且其上方没有 `SYSTEM` entry：这一页绝不凭空造出一份 backend
并未发送的评分标准。

**subagent** 保有自己的上下文（由其启动 prompt 播种），查看该 agent 时这一
页显示的就是它。它自己的裁决读自己的日志，处在与 lead 相同的 classifier
system prompt 之下。

## scratchpad 豁免

还有一件事在 prompt 之前就解决了：一次位于**session 自己的 scratchpad
目录内**的 `write`/`edit`（`docs/tools/scratchpad.md`）。system prompt 把
每个临时文件都送到那里，而该目录在用户的项目之外，所以 `approve_call` 站在
常设 allowlist 旁边询问 `PermissionGate::scratchpad_covers` —— 在
`PermissionRequest` hook、classifier 和 prompt 之前，因为它回答的是与它们
相同的问题。

它很窄：只限那两个 file tool（一条点名 scratchpad 路径的 `bash` 命令仍然会
问 —— 它接着会动什么，是它自己的事），路径必须严格在内且经过词法检查，而
强制 ask 仍然会问。并且它是可见的，走的正是 classifier 裁决所用的那条通道：

```
● Write(/tmp/zeroth-1000/18ce…-5d77f/scratchpad/plan.md)
  ⎿  Wrote 12 lines to …/scratchpad/plan.md
  ⎿  Allowed in the session scratchpad
```

## 工作区路径监狱

`auto` 与 `edit` 模式的“文件改动别再问了”说的是**项目**，不是用户的家目录。
一个 `write`/`edit` 的目标若落在 session 的工作区（启动时的 cwd）与 scratchpad
之外，即便在这两个模式下也不被常设规则覆盖：`PermissionGate::allows` 返回
false，调用落到 prompt（或者，在 headless 运行里落到无人应答的拒绝）。`master`
是明示的“什么都行”姿态，绕过这道监狱。

这道检查是 `file_change_inside`：目标里带有任何词法上的 `..` 直接拒绝；一个已
存在的路径，或一个尚不存在的路径的父目录，会被规范化，所以项目下的一个符号
链接无法把写入指向 `~/.ssh` 或 `.git/hooks`；相对路径对每一个根分别解析。没有
设置工作区的 gate（一个单元测试，一个从未注入过工作区的调用方）保持过去那种
不设监狱的姿态。executor 会再查一遍：`write`/`edit` 拒绝符号链接目标（包括悬空
的），而不是跟随它，因为模型的路径在 gate 处被检查、而写入发生在之后 —— 在两者
之间放下的一个链接绝不能把写入递到外面去（`run_write`/`run_edit`）。这只是
write/edit 的监狱 —— `bash` 仍然能碰到它命令点名的任何东西，这正是 classifier
与各道读取闸存在的原因。

## 按项目持久化（`~/.zeroth/permissions.json`）

常设批准以前只活一个 session —— 退出就每条 rule 都没了，这与“别再问了”这
句话不符。它们现在持久化在 config home 里（`ZEROTH_CONFIG_DIR`，否则
`~/.zeroth`），**以项目目录为键**，所以在一个项目里授予的 rule 绝不会渗到
另一个项目 —— 也就是 Claude Code 的按项目设置：

```json
{
  "projects": {
    "/home/user/Codes/rust/project/zeroth": {
      "allow_commands": ["git status *", "python3 *"],
      "mode": "edit"
    }
  }
}
```

`allow_commands` 中以 ` *` 结尾的行是前缀 rule；其他任何内容都是精确命令。
`mode` 是保存的姿态，只在它与默认值不同时才写入：缺失 = `auto`，而
`manual`、`edit` 和 `master` 则明确写出，未知的标签降级为 `manual` —— 手工
编辑的文件只会要求更多，绝不会更少。manual 还是默认值时，`mode` 键曾对
`manual` 省略，所以早于默认值变更的条目没有这个键，现在读作 auto，与一个
从未保存过的项目完全一样；Shift+Tab 切回 manual，条目就会写明。

纯粹的格式/解析是 `permission::PermissionsFile`/`ProjectPermissions`
（`Settings` 模式）；`tui::config` 和 `tui::permission` 掌管文件 I/O：启动时
的加载在第一帧之前播种 gate（`seed_commands`，然后以 `starting_mode` 的答案
调用 `set_mode`），而每次变更都**重新读取文件，只更新本项目条目中变了的部分，
再写回** —— 一次模式变更（Shift+Tab、`/settings` 行、文件 prompt 上的选项
2）只改 `mode`（`record_mode`），一条命令或 MCP rule（其 prompt 上的选项 2）
只改 `allow_commands`（`record_commands`）。这个拆分正是把
`ZEROTH_PERMISSION_MODE` 的播种挡在文件之外的原因 —— gate 的实时模式
携带着播种值，否则在其下保存的一条 rule 会把播种值写到项目选定的模式上
（`settings.json` 的规则，`docs/state/per-directory-state.md`）—— 同时也让
同一目录里第二个实例保存的 rule 不再覆盖第一个实例期间选定的模式。
读-改-写，所以其他目录里的实例从不互相覆盖；与 `config.json` 一样尽力而为，
写失败绝不会害死 TUI。在同一目录里重启应用 —— 或者 `--continue` /
`--resume` 一个 session —— footer 会以保存的模式起来，保存的 allowlist 也
在生效中。

## 握手

tool 跑在 backend 线程上；prompt 活在 event loop 里。这次往返靠一个
`PermissionGate` —— `background::BackgroundRegistry` 和
`agents::AgentRegistry` 的 `Arc<Mutex<…>> + Condvar` 姊妹：

```
run_agent (backend thread)          event loop                     user
  approve(call) ─┐
                 ├─ gate.allows()? ── yes ──────────────────────────► (runs)
                 └─ no
                    tx.send(StreamEvent::Permission(request)) ──► App::open_permission
                    gate.wait(id, cancelled) …blocked…            (composer draft stashed)
                                                    ◄── Action::ResolvePermission ── 1/2/3
                    ◄─────────────── gate.resolve(id, decision)
   Allow  → ToolStart … ToolEnd
   Reject → ToolStart + red ToolRejected{display, result}
```

`wait` 以很短的超时轮询其 condvar，并在该轮的 `CancelToken` 触发的那一刻
放弃，所以 Esc / `/clear` / 退出都会及时收割一个被阻塞的 tool 线程，而不是
把它卡死。

`run_agent` 为此多了一个接缝 —— `approve: FnMut(&ToolCallRequest) ->
Approval` —— 在每条普通调用的 `ToolStart` **之前**立即调用，所以 prompt
出现时什么都没运行，也什么都没被宣告为正在运行。拒绝仍然发出 Start/End 这
一对 —— `ToolStart` 然后是代替 `ToolEnd` 的 `StreamEvent::ToolRejected` ——
所以这次调用作为红色 cell 落进历史和 transcript，而*模型*读到更长的“停下
来等”文本。显示字符串与 tool 结果正是 `Approval::Reject` 的两个不同字段，
好让 cell 可以简短、指示可以完整，而 `ToolRejected` **两者都**携带，所以
被记录的调用也把两者都留下（见下文）。

subagent 的调用走同一个接缝 —— `spawn_subagent_run` 把 agent 的类型作为
请求的 `agent` 传入，事件走 agent 通道的转发器，于是 `on_agent_event` 打开
同样的 prompt。

## amend 反馈值多少

Tab 的全部意义在于说明*改做什么*，所以那句话必须比它被敲进去的那一轮活得
更久。有两个地方记录它：

- **cell** —— `permission::denied_display` 把它放在第二行，于是 transcript
  显示被要求的是什么，而不是一句光秃秃的拒绝：

  ```
  ● Write(hello.py)
    ⎿  User rejected write to hello.py
       Instructions: use pathlib instead
  ```

- **派生上下文** —— 面向模型的 `result` 随被记录的调用作为
  `ToolCall::context_output` 一起走，而 `context::context_messages` 重放的
  是*它*作为 `tool` 结果（`ToolCall::context_text()`），而不是 cell 文本。
  于是 Ctrl+D 显示模型实际被告知的内容，之后每一轮都继续携带它，`/resume`
  一个 session 也能恢复它（`session::ToolRecord` 的 `context_output`，不存在
  时省略，所以更旧的 rollout 仍可解析）。

没有第二条，这条指示只到达模型一轮 —— live `run_agent` 的消息列表有它，但下
一轮会从历史重建上下文，而历史只保留了 `User rejected write to hello.py`。
同样的拆分覆盖选项 3（纯 `No`）和 Ctrl+E 的改为解释，它们的面向模型文本
此前也在以同样的方式被丢弃。subagent 被拒的调用在自己 transcript 上保留两段
文本，所以它的 Ctrl+D 视图和任何续跑都能看到它读到了什么。

token 计数也计入 `context_text()`：下一次请求上传的是那段长指示，而不是一行
的 cell。

## prompt 是模态的，草稿会存活

`on_key` 里最先检查的是 `App::permission`，先于 Ctrl+R 搜索和内联 picker：
请求打开期间它拥有除两个只读覆盖层之外的所有按键
（[见下文](#除了那两个只看的键)）。`render_live` 用整个
live 区域替换成它，连流式条一起 —— 这一轮在等你，没有什么可动画的。

打开时搜走 composer（`TextArea` 文本 + 光标，以及 `!` shell 模式标志）并清空
它，同时丢掉所有其他挂在 composer 上的东西 —— palette、`@`/`$` picker、`?`
带、打开的 Ctrl+R 搜索、已预备的 backtrack，以及 footer 的 ↓ 选中项
（`background_focus`、`agent_selection`）。footer 那几项要紧的理由与其他项
相同：prompt 会盖在 footer 上，所以一个仍亮着的选中项是用户既看不见也清不掉
的 —— 模态路由在其按键 handler 之上 —— 而且它会在 prompt 关闭时*待发*地回来，
下一次 Enter 会打开 background manager 带，而不是做用户的本意。关闭时恢复草
稿，所以一条在你打字中途到来的请求不会吃掉你正在输入的东西 —— 而 Tab 的
amend 字段起始为空，因为它*就是*同一个 textarea。一个请求开着时到来的第二个
请求会排队（`App::pending_permissions`），并在第一个解决后立即打开；其间它
的 backend 线程只是继续阻塞。

`/clear`、一次中断和一次退出都会丢掉 prompt 和队列；被阻塞的线程注意到自己
的 cancel token 后返回。

### ……除了那两个只看的键

**Ctrl+O**（transcript 分页器）和 **Ctrl+D**（原始 LLM 上下文）是“拥有一
切按键”的例外，而它们成为例外，理由与这条规则的其余部分存在相同。prompt
询问的是已经出现在 transcript 上的工作，而回答它往往需要读到问题上方那个
cell 更远的地方 —— 模型说它在做什么、上一个 tool 返回了什么、它工作的那份
上下文里实际包含什么。吞掉这两个键，会让 prompt 成为这个 session 里唯一无法
检视会话的时刻，而那恰恰是最需要检视的时刻。所以
`on_key_permission` 先运行 `App::on_key_overlay_toggle` 并返回它的结论 ——
一个 helper，从 `on_key` 的全局分支中提取出来，放进 `app/views.rs`，就放在
它包装的那两个 toggle 旁边，这样全局绑定和模态的就不会漂移 —— 之后才吞掉
其余按键。其他每个 Ctrl+key 仍被吸收：这个 prompt 绝不能能被误答。

transcript 显示待处理调用的方式，与 prompt 上方那条带一样 ——
`● Write(hello.py)` 配其暗色 `⎿ Waiting…` —— 因为它读的是同一个
`tool_queue`，所以你打开它是为了看的东西就在最上面。

其他一切不变。两个视图都是只读的 —— 它们只滚动，仅此而已 —— 所以 tool 线程
继续阻塞在 gate 上，队列继续排队，搜走的草稿继续被搜走，回来时 prompt 仍然
开着。这两个键从**Tab 的 amend 字段**里也能用（两者都不是编辑键），输入的
反馈在往返中存活。而且因为这两个视图在 alternate screen 上，模态区域的路由
自己就管好了：`on_key` 的各个守卫都是 `view == View::Conversation`，所以
overlay 立起时，它自己的键位表就掌管键盘。

两个边界细节让这次往返在终端上成为 no-op，而两者本来就已经为“请求在 overlay
立起时到来”那种情况存在：

- 返回用的是普通的 `Session::overlay_return_repaint` —— 在 overlay 之下提交
  的一切都躺在视口的待处理队列里，并在 live 区域之上冲刷，而下一次绘制滴答
  的流检查（`view_flow_stale`）用惯常的 purge 重建重新确立一个屏高的 prompt
  的流。屏幕**和**回滚在 `smoke.sh` Phase 90 驱动的三种几何下都逐字节恢复
  —— 一个浮动的区域、一个在屏幕底部冲刷的区域、一个页面向上流入回滚的冲刷。
- overlay 空闲时的 **Esc** 绝不能预备 backtrack 预览
  （`App::overlay_esc_backtracks` 新增了 `!App::modal_open()` 子句）。一个
  *后台 agent* 可以在没有轮次运行时拉起 prompt，所以光有“空闲”会提供一个
  回退，它截断历史并预填 prompt 已经搜走的那个 composer —— 而此时还有 tool
  线程停在 gate 上。有东西阻塞在用户身上时，没有任何东西可回退；关闭时的
  提示行相应地写作 `q/esc/ctrl+o to quit`，因为提示和按键共享同一个谓词。

`App::modal_open` **就是**那个谓词 —— 一个 permission prompt 或一个 ask 模态
开着 —— 而 `ui::region_is_modal` 现在也是它的调用方，所以键路由、区域的重
钉与其关闭时的 purge，以及 backtrack 守卫读的都是同一个定义。再手写一份副本
正是共用定义要防止的那种漂移。

`AskUserQuestion` 模态遵循同样的规则 —— 同一个 helper，同一个理由
（`docs/tools/ask.md`）。内联 **picker**（`/model`、`/settings`、`/hooks`、
↓ 管理器……）刻意不遵循：它们也拥有一切按键，但 Esc 只是关掉它们，所以那两
个视图本来就只隔一次按键。模态是唯一不回答就无法离开的区域 —— prompt 上的
Esc 取消这一轮，问题上的 Esc 拒绝这次调用 —— 这正是它们两个绝不能把会话锁
在外面的原因。

### 光标消失

选项立起期间，画面**完全不显示硬件光标**（`ui::cursor_visible`）。选项列表
是菜单，不是文本字段：没有东西可供光标指，而终端光标是屏幕上唯一*自己*会动
的东西 —— 带光标拖尾动画的终端（kitty）会画出它的每一次跳跃，所以一个
prompt 打开就在屏幕上甩出一道痕，每次 ↑/↓ 又甩一道。没有可显示的东西，就什么
都不显示。

这对边界毫无代价：每一帧本来都以 `Hide` 开头（所以任何一次重绘都不会被抓到
拖着光标跑），而 `term.rs` 只是跳过结尾的 `Show` —— 光标在选项存在的整段时
间里都留在外面。Tab 的 amend 字段**是**要打字的，所以光标随它回来，prompt 关
闭后 composer 的也一样。

`ui::cursor_position` 无论如何仍为它定位 —— 落在高亮选项上，越过 `❯` 标记
一列，随 ↑/↓ 跟踪 —— 所以它的回归从有意义的一行开始，而不是上次滚动留下的
任意位置（而一个忽略隐藏指令的终端看起来也仍然正确）。两个座位，选项行和
amend 字段，都从区域的下边缘往上 `PERMISSION_TAIL_ROWS` 处找到（间隙、提示、
分隔线 —— 没有空行紧贴收尾分隔线，`docs/ui/view-flow.md` 的 *The frame hugs
its content*），所以光标从不需要重新推导正文。尾部块收束页面，所以底部锚点
在每种正文尺寸下都让它紧贴收尾分隔线 —— 一个流动的页面和一个恰好放下的页面
在定位光标上结果相同。

## 没人可问：headless 运行

headless 运行（`zeroth -p`，`docs/cli/headless.md`）没有屏幕可放 prompt，
所以它的 gate 是**无人值守**的（`PermissionGate::set_unattended`）。在 prompt
之前作答的一切仍然作答 —— allowlist、scratchpad 豁免、只读 fast path、一个
`PermissionRequest` hook、classifier —— 只有最后一步变了：在 `approve_call`
本会发出 `StreamEvent::Permission` 并等待的地方，它改为拒绝这次调用
（`PermissionGate::refuses_unattended`）。没有请求存在，没有东西等待，也没有
`Notification` hook 触发，因为没人被展示任何东西。cell 读作
`UNATTENDED_DISPLAY`（`Not run: needs approval (headless run)`）；模型读到
`UNATTENDED_RESULT` —— 调用未运行、没人能批准它、不要重试或绕过、并说明所需
要什么 —— 这是 classifier 拒绝的姿态去掉它的“停下来问”，因为那里没人可问。
强制 ask（`PreToolUse` → `ask`）也止于那里：它意味着“由人来决定”。一个
**失败**了的 classifier —— 在任何裁决之前的错误 —— 在那里也同样被拒，但会被
区分开来：模型读到 `CHECK_FAILED_RESULT`（检查失败，没有其他人能批准它，把
这次完全相同的调用重试一次，再次失败就停下），而同一调用上的第二次失败读到
`CHECK_FAILED_AGAIN_RESULT`（不要重试），gate 记住每个失败的调用
（`PermissionGate::first_check_failure`），所以这份邀请每次调用只发出一次，
失败的检查绝不会循环。gate 保留每一次拒绝，直到这次运行把它们取走
（`take_unattended_refusals`）用于它的 `permission_denials` 和结尾的 stderr
一行。subagent 共用这个 gate，所以它们的调用也以同样的方式被拒。

所以模式决定一次 headless 运行：`master` 什么都不拦，`auto`（除非项目保存了
另一模式）放行文件改动、只读命令和 classifier 允许的命令，`edit` 拒绝命令中
不在 allowlist 的部分，`manual` 拒绝每一次改动。在 TUI 里保存的 rule 会被尊
重，这正是把一个项目先交互式地准备一次、然后无人值守地运行的做法。

## 关掉它

`ZEROTH_PERMISSIONS=0`（或 `false`/`no`/`off`）让 session 起来时根本不挂
gate，每个 tool 都像这个特性之前那样运行。`LlmBackend` 只在装了 gate 时才询问，
所以直接构建 backend 的嵌入方（以及 live 集成测试）不受影响。没有 gate 也就
没有模式：footer 右边缘那一段消失，Shift+Tab 用一个 toast 说明自己，而不是假
装切换什么，而 `ZEROTH_PERMISSION_MODE` 也无处可播种。

## 测试

- `permission.rs` —— 纯词汇：标题、问题、选项标签（`{prefix} *` 的显示，精
  确命令原样），`command_scope` 的分段/加前缀/降级（子命令工具、包装器、env
  赋值、保留字、引号感知的重定向，以及 bash 对转义引号、注释和 `$'…'` 所做
  的切分，各自对照一条保存的 rule），auto 默认值（模式、规则和一个新 gate 都
  从那里开始），模式的标签/解析往返与四步 Shift+Tab 循环，规则的允许 + 记住
  （模式对文件改动的两个方向门控；auto 覆盖文件但从不覆盖命令；master 覆盖
  一切），classifier 词汇（允许的 note、拒绝的显示/结果文本、离线
  `auto_verdict` 启发式的允许/拒绝之分），`PermissionsFile` 往返（文档化的
  形状，包含 manual/master 标签，保留其他项目，垃圾 → 空文件，模式缺失 →
  auto，未知标签 → manual，auto 省略而 manual 写入），两条局部记录
  （`record_commands` 保留已保存的模式，`record_mode` 保留 allowlist，所以
  一个播种的模式绝不会落进文件），`ZEROTH_PERMISSION_MODE` 语法
  （`mode_by_env`：未设置与空白不播种，标签任意大小写，其他一律 manual）与
  `starting_mode` 的优先级（播种值 > 保存的模式 > auto），以及 gate 的阻塞往
  返（真线程）+ 共享模式 + 播种命令。
- `llm/approval.rs` —— 带脚本化 classifier 的自动模式接缝：一个允许的裁决
  返回 `AllowNoted`（无 prompt），拒绝则给出 classifier 的文本，文件和
  allowlist 中的命令从不询问它，manual/edit 模式从不询问它，master 静默放行
  一切，classifier 失败回退到 prompt，被取消轮次上的失败不弹 prompt 直接拒绝，
  而没挂 classifier 的自动模式仍然会问。`llm/classifier.rs` —— user prompt 的
  构建与 `<block>` 约定的解析（容忍噪声/大小写/空白；无裁决 → 错误，绝不
  allow）。`llm/agent.rs` —— 带 note 的批准发出
  `ToolStart → ToolNote → ToolEnd` 并仍然运行该调用。
- `ui/tests/classifier_view.rs` —— Ctrl+D classifier 页：每种模式的模式说明，
  块逐字作为其 `USER` entry，system prompt 在其上方节略显示
  （`abridge_prompt` 保留标题、截断的开头段落并计数其余部分 —— 真实 prompt
  节略为其四个小节，每一行内容要么显示要么计数），`## Action to review` 槽位
  收束该消息，未注入 prompt 时没有 prompt 小节，且每一行都适配窄宽度。
- `tests/live_classifier.rs` —— 可选的 Venice 校验：把显示的 prompt 对照运行
  时的 prompt，随后对无害输出、递归删家目录和上传密钥给出合成的允许/拦截裁决。
  这些只是分类请求；不执行任何命令。环境里有 `A0_VENICE_API_KEY` 时，运行
  `cargo test --test live_classifier -- --ignored --nocapture`。
- `app/tests` —— 打开时搜走、关闭时恢复草稿，键位表
  （↑/↓/1/2/3/Tab/Esc/ctrl+e —— 裸的 `a` 现在什么都不做；Shift+Tab 在文件
  prompt 上采取记住选项，在 bash prompt 上和从 composer 都双向切换模式，权限
  被禁用时则说明自己），amend 字段，队列 —— 外加整个 amend 往返（真 gate、
  真按键），断言被记录的调用和派生上下文携带的正是模型被告知的内容；以及
  prompt 放行的那两个键 —— Ctrl+O/Ctrl+D 在打开的 prompt 之上打开并关闭（选
  项*和* amend 字段，反馈完好），其他每个 Ctrl+key 仍被吞掉，且一个等待中的
  prompt 不是 backtrack 目标 —— 再加上打开时随 composer 一起带走的 footer
  选中项。
- `ui/tests/transcript.rs` —— 打开 prompt 之下 overlay 的**关闭提示行**：一个
  后台 agent 的请求（空闲，且有 backtrack 目标）仍读作
  `q/esc/ctrl+o to quit`，所以这一行承诺的和 Esc 所做的在构造上就一致。
- `ui/tests/footer.rs` —— footer 右边缘的模式段（贴住该行的边缘、暗色、窄宽
  度下左侧内容先截断；未注入模式时不存在）。
- `ui/tests/permission_view.rs` —— 一条很长的记住规则会换行而不是藏起尾部
  （每个 token 都存活，续行对齐且无标记），一条病态的规则在
  `PERMISSION_OPTION_MAX_ROWS` 处封顶并带 `…`（选项和提示仍在屏幕上），选中
  选项的每个换行行都点亮，且光标座位仍落在每个选项 `❯` 所在的那一行 —— 即使
  在换行块之后。
- `app/tests/tools.rs` —— `reject_tool` 保留两段文本并在面向模型的那段上计
  数；`session.rs` —— 拒绝经由 rollout 文件往返，而普通调用的行保持其旧形状。
- `ui/tests` —— 渲染出的 prompt：规则、着色标题、agent 后缀、编号/diff 正文、
  选中项上的青色 `❯`、提示行、其后保留的实时 cell（待处理调用及其兄弟之下
  都是 `⎿ Waiting…`，一个真正在运行的调用是 `⎿ Running…`，subagent 的则是整
  棵树），纯渲染恰好只画 prompt 的那些行，`permission_height` 等于所画行数
  按终端钳制后的值 —— 构建器在区域自身高度上是个不动点 —— 以及大批次规则：
  十五个排队的 edit 仍给正文留出它的行、选项仍在屏幕上（多余的兄弟折叠成
  `… +N more waiting`，被询问的调用存活在最上面，高度契约成立），同一批次下
  一个高正文整体流动、滴答的上下文被丢弃，小批次显示每个兄弟且没有摘要行，而
  超出 `PERMISSION_BODY_MAX_ROWS` 上限的正文仍以 `… +N lines` 尾部封顶。
- `ui/tests/layout.rs` —— `region_is_modal` 只是 prompt，别的都不是（边界读取
  该谓词来为关闭时的 purge 记下单向移动）；以及 `modal_needs_rebuild`，绘制
  滴答的整个重建决策：当一个钉住的打开 prompt 下一帧将落得短于屏幕底部时触发
  （无论有无待处理行，无论 note 是否设置），当帧仍落到位时保持（同高、增长、
  一个自身滚动又落回贴底的冲刷），跳过浮在底部之上的 prompt，而在关闭时与以
  前一样精确遵循那一次单向记录。
- `stream/dummy/gated.rs` —— dummy 的“parallel permission”轮：两个受门控的
  `Bash` 调用事先宣告，各自在开始前询问，下一个请求紧接上一个 cell 的解决而
  来、没有脚本化的停顿；以及“staggered permission”轮 —— 两个受门控的 `Write`
  其 prompt 高度相差悬殊（第一个的正文顶满任何正常终端，第二个只有一行），同
  样的无停顿时序，用于中途打开时的收缩；以及“auto permission”演示的三条路
  径 —— 自动模式对两个调用都不询问地分类（列表上是 note，删除上是拒绝），
  manual 模式对两者都询问，master 模式两者都静默运行。
- `app/tests/tools.rs` —— `set_tool_note` 随运行中的调用进入历史（并忽略
  Waiting 前沿 / 空闲队列）；`agents.rs` —— subagent 的 `ToolNote` 落在它自己
  的 transcript 上；`ui/tests/tool.rs` —— note 行在折叠 peek 之后和展开视图
  中都渲染，暗色，只在调用解决后出现（包括失败的运行、固定在自身行下的后台
  cell），而无 note 的 cell 与之前逐字节相同；`session.rs` —— note 经过
  rollout 往返，而无 note 的行保持其形状。
- `tests/live_openrouter.rs`（ignored；需要 `OPENROUTER_API_KEY`）—— 真实的
  classifier 允许 `ls -la` 并带理由拒绝 `sudo rm -rf /etc`，以及一整轮 live
  自动模式：`ToolStart → ToolNote → ToolEnd`，没有 `Permission` 事件，命令
  的输出出现在模型的回复里。
- `ui/tests/permission_view.rs` —— 选项在 amend 字段和 composer 显示光标时
  不显示光标；以及座位，钉在渲染出的行上：它落在携带 `❯` 标记的那一行，并
  随每次 ↓ 下移，在一个流动的 prompt 上（经由底部锚点的跳过映射）和一个恰好
  放下的 prompt 上都是如此。
- `smoke.sh` Phase 55 —— 在真实终端里对 dummy backend 的整个往返：输入了草
  稿、显示了 prompt、按 `2` 批准、草稿恢复 —— footer 右边缘的模式翻到
  `edit`，项目的条目落进 `permissions.json` 且带 `"mode": "edit"`（事后清理，
  好让后续权限阶段仍能得到它们的 prompt）—— 外加从终端读回的硬件光标
  （`#{cursor_flag}`）：在选项上隐藏，在 Tab 的 amend 字段里和之后在 composer
  里重新出现，并停在 `❯ 1. Yes` 行，按 ↓ 后低一行。
- `smoke.sh` Phase 56 —— Tab 的 amend 端到端：指示落在红色 cell 上，面向模型
  的拒绝（含反馈）显示在 Ctrl+D 上下文视图里，两段文本都没有泄漏到对方的位置。
- `smoke.sh` Phase 58 —— 真实终端里的滚动几何：一个屏高的 prompt 打开时，早
  先的回复在屏幕+回滚中恰好可达一次（被盖住的行以前不活在任何缓冲里 —— 那个
  “询问期间终端滚动被禁用”的 bug），而作答后盒子贴回底部、会话完整、每条消息
  恰好提交一次。
- `smoke.sh` Phase 59 —— 真实终端里的背靠背间隙：“parallel permission”批次
  的第一个 prompt 把刚发出的消息、上一轮，以及其上方两个 `⎿ Waiting…` cell 都
  显示为真实的行；第二个 prompt（落在与第一个 cell 提交相同的帧间隙里）显示那
  个已完成的 cell —— 在打开的 prompt 之上一次性提交 —— 以及那条消息；最终屏
  幕完整 —— 盒子贴底、消息在回滚+屏幕中恰好一次。
- `smoke.sh` Phase 60 —— prompt 打开时的一次 resize，然后是作答：prompt 挺过
  中途的 purge 重建，而关闭时自己的 purge 让盒子贴底落下，而不是浮在折叠
  prompt 腾出的那些行之上，每条消息恰好提交一次。
- `smoke.sh` Phase 62 —— 同样的关闭，经由 overlay 到达：请求到来时 Ctrl+O 已
  立起，返回时把打开的 prompt 摆在恢复后的屏幕之上（它的增长和排队提交的冲刷
  是记录下的单向移动），作答后仍让盒子贴底落下、消息恰好提交一次 —— 那个
  “newlines at the bottom, but only when Ctrl+O was opened first” 的 bug。
- `smoke.sh` Phase 63 —— 真实终端里的中途打开收缩：“staggered permission”批
  次的屏高第一个 prompt 被作答，而当一行的第二个 prompt 打开时，它的收尾分隔
  线是窗格的**最后一行** —— 仍然打开的 prompt 之下没有一条空行带（被报告的
  空换行 bug）—— 高的 `write` 的已解决 cell 在其上清晰可见、恰好提交一次，之
  后盒子贴回底部。
- `smoke.sh` Phase 64 —— 模式端到端，启动时不带套件的
  `ZEROTH_PERMISSION_MODE=manual`：新 session 的 footer 在任何按键之前读
  作 `auto`；“auto permission”演示由离线 classifier 裁决，不弹 prompt，也不
  往 `permissions.json` 写任何东西；一次 Shift+Tab 是 master（保存为
  `"master"`，演示静默运行），第二次是 manual，保存为 `"mode": "manual"` 并
  在重启后回到 footer；而一次播种为 `edit` 的运行以 edit 启动，同时在播种值之
  下批准的一条命令规则把 `ls *` 保存在未被动过的 `manual` 旁边。
- `smoke.sh` Phase 90 —— 真实终端里在一个**打开**的 prompt 之上用 Ctrl+O/
  Ctrl+D，三种几何，每个都断言其名字所指的座位（一个有余行可浮动的区域、一
  个在屏幕底部冲刷的区域，以及一个其页面把顶部流入回滚的冲刷），这样任何情况
  都不会悄悄退化成另一情况的副本：transcript 打开时被询问的调用仍在其上显示
  `⎿ Waiting…`，并提供 `q/esc/ctrl+o to quit`，**Esc** 离开它 —— 那个本来会
  预备回退的键 —— 回到同一个仍贴底的 prompt，Ctrl+D 以同样方式往返，两次往返
  都让屏幕*和*回滚逐字节一致，而 prompt 之后仍能解决。
- `tests/live_openrouter.rs` —— 针对真实 provider：重放的拒绝是一个可读的上
  下文形状，而模型在一轮之后仍遵循那些指示。
