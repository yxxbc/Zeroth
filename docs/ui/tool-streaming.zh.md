# 实时流式 `bash` 输出 —— 运行的尾部 + 折叠的一瞥

Claude-Code 把一条正在运行的 shell 命令渲染成一个**实时尾部**：带颜色的
`● Bash(cmd)` 表头、`⎿` 边栏下最近的输出行，以及一行随更多内容到达而走动的
`+N lines (Ns)` 页脚 —— 而命令结束时，它折叠成最前面几行加上
`… +N lines (ctrl+o to expand)`。在这次改动之前，后端的 `bash` tool 在运行时只
显示 `⎿ Running…`，完成之后只显示**一行**一瞥。本文讲的是把它做成流式。

```
Running (live, in the strip above the box):        Finished (committed to scrollback):

● Bash(ping -c 10 google.com)                       ● Bash(ping -c 10 google.com)
  ⎿  64 bytes from … icmp_seq=6 … time=68.3 ms         ⎿  PING google.com (142.250.205.238) …
     64 bytes from … icmp_seq=7 … time=247 ms             64 bytes from … icmp_seq=1 … time=32.1 ms
     64 bytes from … icmp_seq=8 … time=332 ms             64 bytes from … icmp_seq=2 … time=71.1 ms
     64 bytes from … icmp_seq=9 … time=144 ms             … +11 lines (ctrl+o to expand)
     +5 lines (9s · wait 2m)
     (ctrl+b to run in background)

● Bash(ping -c 10 facebook.com)                     (a parallel batch's not-yet-run
  ⎿  Waiting…                                        siblings stay `⎿ Waiting…` until
                                                      each one's turn — docs/tools/parallel-tools.md)
```

页脚里那句是**命令自己的时钟，配在它那次调用所属的等待旁边** ——
`(9s · wait 2m)` —— 无论输出是什么形状，它都挂在实时单元格上（见下文*时钟行一
直在*）。

两个状态是刻意不对称的 —— **运行中**你想要**尾部**（刚发生了什么）；**结束**
之后你想要**头部**再加一句展开提示。尾部显示最后 `TOOL_PEEK_ROWS`（4）个显示
行；头部在 `TOOL_FOLD_ROWS`（3）个显示行处**折叠**，提示行压在上面 ——
Claude Code 的折法，最多还是那四行、含提示 —— 所以单元格落定时绝不长高。（它
可能落定得*更矮*：完成的头部是输出的第一个**块**，所以一个空行就结束了它 ——
`docs/ui/long-lines.md`。运行中的尾部保留它的空行，因为那正是命令刚打印出来的
东西；而且它按打印的样子显示输出，而落定后的单元格会为显示重塑一行 JSON。）

## 协议 —— `ToolOutput` 与 `ToolScreen`

两个事件携带一个**运行中** tool 的实时输出，在它的 `ToolStart` 与 `ToolEnd`
之间：

```rust
/// A chunk of one running tool's output, appended as it is produced.
ToolOutput { call_id: Option<String>, chunk: String },
/// The running tool's output as it builds up: `settled` text appended for
/// good, `live` rows replacing the `live` rows sent last.
ToolScreen { call_id: Option<String>, settled: String, live: String },
```

`ToolOutput` 是只追加的形式 —— 离线 dummy 的脚本化命令用它来流式输出。
`ToolScreen` 才是真实执行器发的：一行仍在被绘制的内容（被 `\r` 重画的进度条、
一个正在接收回答的提示符）作为 `live` 传输，并**被**下一次更新**替换**，而不是
每帧堆一行；而一行已定稿的内容作为 `settled` 传输一次
（`docs/tools/interactive-shell.md` *Streaming the running cell*）。两者都点名
它们属于哪个调用（`call_id`，与它的 `ToolStart` 及批次摘要携带的是同一个
id）：并行批次里的 worker 同时在跑，所以**可能有几个单元格都是 `Running`**，各
自拖着自己的行 —— 一个 chunk 会被路由到它点名的那个单元格，绝不落到恰好排在前
面的那个（`docs/tools/parallel-tools.md`）。从不流式输出的后端只要不发它们就
行 —— `ToolStart`/`ToolEnd` 仍照原样工作。

## App 状态 —— `push_tool_output` / `push_tool_screen`

`App::push_tool_output_for(call_id, chunk)` 把 `chunk` 追加到那个调用的
`output`；`App::push_tool_screen_for(call_id, settled, live)` 截掉那个调用上一
次追加的 `live` 尾部（它在 `App::tool_live` 里的条目，以调用 id 为键），追加
`settled`，然后是新的 `live`（`app::apply_tool_screen`，`agents` 里的
subagent 折叠也共用它）。两者都**只在被点名的调用是 `Running` 时**动作（否则
什么都不做），并且都会 bump `App::tool_revision`，Ctrl+O transcript 缓存正是以
它为签名 —— 一次重绘可能让输出长度保持不变。两者都不计 token —— 总数只按权威
的 `ToolEnd` 输出在 `end_tool` 里计一次（它用最终带框的文本覆盖 `output`）。所
以实时尾部与最终单元格绝不重复计数，而即使某次更新被丢掉，最终单元格也总是精确
的。`call_id` 为 `None` 的孪生版本（`push_tool_output`、`push_tool_screen`）针
对队首，即串行路径（`!` shell、一个脚本化调用）。

## 执行器 —— `run_bash` 流出折叠后的输出

`ToolExecutor::execute` 接收一个 `on_output: &mut dyn FnMut(ToolProgress<'_>)`
接收器等；`run_agent` 把 `ToolProgress::Screen` 映射成 `ToolScreen`（把
`ToolProgress::Title` —— 一个同伴调用精修后的表头 —— 映射成 `ToolTitle`）。
`run_bash` 在读取线程上抽空两根管道（这样一条话多的命令不会因管道写满而死
锁）；每个读取者经一条 `mpsc` 通道发送原始字节 chunk，主轮询循环按到达顺序把它
们喂给一个 `PipeOutput`：一个**折叠**（`pty::fold`），按终端显示它的方式重放这
条流 —— `\r` 与退格覆盖、颜色转义消失、tab 与行尾空格保留 —— 每行结束就保留
一次，且在输出上限之内。它把已结束的行作为 `settled` 流出、把仍在绘制的行作为
`live` 流出，最多每 `PIPE_STREAM_INTERVAL`（50 ms，结尾再补一次），所以一阵进
度帧每个间隔只花一次重绘 —— 每次都在输出的两帧之间的停顿处采样
（`pty::pace`），于是一次分几段写入的重绘、一次擦除再写文本，绝不会以画了一半
的样子到达单元格（`docs/tools/interactive-shell.md` *Streaming the running
cell*）。解析器把跨 chunk 拆开的 UTF-8 字符或转义留到下一个 chunk，所以尾部绝
不显示一个孤零零的替换字形。`read`/`write`/`edit` 忽略这个接收器（没有什么可流
的）。

最终的 `ToolOutcome.output` 是 codex 的 `Exit code: N` 框，罩在被（上限内）
**折叠**的正文之上 —— 一条 `\r` 进度条只以它的最终状态到达模型一次 —— 而
`end_tool` 用它覆盖显示的 `output`。一次 Ctrl+B 移交把读到的字节原样交给后台注
册表，所以它的监视器自己折叠整条流（`docs/tools/background.md`）。

## 渲染（`ui/tool.rs`）

一个**命令式** tool（不是 `read`/`write`/`edit` 文件单元格的非 shell 后端
tool —— 实践上就是 `bash`）如今把它的输出渲染成一个多行的 `⎿` 块，像 `!`
shell 单元格一样：

- **已完成**（`tool_lines`）：输出显示行的头部（`exec_display_lines` —— 剥掉
  框、重塑一行 JSON、丢掉尾部空行）—— 它的第一个**块**（跳过开头的空行，其后的
  第一个空行结束这一瞥：三行的折法负担不起一行什么都不说的内容，而跳过那个空隙
  会把两段输出呈现成一段）—— 在 `TOOL_FOLD_ROWS` 个显示**行**处**折叠** —— 然
  后是 `… +N lines (ctrl+o to expand)`，经共享的 `result_peek_block`；恰好
  `TOOL_FOLD_ROWS + 1` 行的输出整段显示，因为藏住一行的提示反而要花掉它藏的那
  一行。每一行**按词换行、保留空格**（`wrap_output`，Ctrl+O 视图用的同一个包装
  器 —— 像 `sudo` 那样散文式的错误在词处断开，绝不在 "askpass" 中间断；放得下
  的 `ls -l` 列保持逐字节精确；比任何一行都宽的 token 填满它所在的那一行，
  Claude Code 的换法），而不是在终端边缘裁剪，所以长行的尾部不再消失 —— 而折
  叠正是给某一病态行（一个压缩过的 bundle、一个 2 KB 的 `curl` 响应体）设界的
  东西：它显示其中三行，提示说明余下的还在（`docs/ui/long-lines.md`）。
  `+N lines` 提示数的是它没显示的**显示行** —— 即按下 Ctrl+O 实际多出来的东
  西，用展开时同一个包装器数的 —— 而不是源码行，这就是为什么 1.8 KB 被藏起来的
  JSON 过去把自己报成 `+1 lines`。（传统的 diff 兜底一瞥反而**逐字**换行 ——
  代码，绝不在空格处重排 —— 每个换行后的行按它*源码*行的 `+`/`-` 标记着色，所
  以续行保持它的色。）
- **运行中**（`running_command_lines`，只画在 live 带的预览里，那里边界提供的
  `elapsed` 可用）：表头、输出的**最后** `TOOL_PEEK_ROWS` 个显示**行**，然后是
  **时钟行** —— 窗口上方有任何整行被藏起来时是
  `+{hidden} lines ({elapsed} · wait {limit})`，一行都没藏时是裸的
  `({elapsed} · wait {limit})`。还没有输出 → `⎿ Running… ({elapsed} · wait
  {limit})`，那句就挂在角标行上。`{elapsed}` 是**命令自己的**运行时长，绝不是
  这一轮的 —— 见下文*页脚显示谁的时钟* —— 而 `{limit}` 是这次调用所属的等待
  —— 见下文*时钟行一直在*。长行用同样的方式**按词换行**（`wrap_output`）而不是
  在终端边缘裁剪；窗口按换行后的行计，所以单个长行跟随它自己最新的行而不会把带
  撑过预算，而最新优先的遍历每个动画帧只换行窗口能显示的那些。页脚数的是窗口之
  上的*显示行* —— 完全在其上方的行，加上最旧那条显示行在它自己顶部丢掉的行
  —— 所以一条滚过去的 2 KB 行读出 `+18 lines` 而不是 `+1 lines`
  （`docs/ui/long-lines.md`；`WrapMode::rows` 只数不建行，所以整个保留的缓冲区
  每个动画帧都能量一遍）。（带仍按 `render_live` 绘制所依靠的同一次
  `preview_tool_lines` 遍历来定尺寸，所以计数与绘制在构造上一致。）
- **Ctrl+O**（`tool_full_lines`）：完整输出，不设上限 —— 也用 `wrap_output`，
  所以展开视图与内联一瞥渲染得一模一样。

### 页脚显示谁的时钟

`+N lines (Ns · wait …)` 页脚 —— 以及 `!` shell 那行
`⎿ Running… (Ns)` —— 从**命令**启动的那一刻算起，绝不是从这一轮开始算。
`preview_tool_lines` 过去把状态指示器的时钟（这一轮的 `elapsed`）递给了
`running_command_lines`，于是一次在一轮进行到一分钟后才开始的 `bash` 调用，会在
一个刚刚启动的单元格下面以 `+N lines (60s)` 开场，而页脚与旁边
`Working… (60s ·` 同步计数 —— 这就是被报上来的 bug。两个时钟，两个问题：状态行
回答*这一轮跑了多久*，页脚回答*这条命令跑了多久*。

命令时钟本来就存在。`StatusClocks::command_start` 在一次模型 `bash` 调用的
`ToolStart` 处、以及一次 `!` shell 的启动处（`run_shell`，它把它与这一轮的一起
开，所以 shell 行的数字不变）被设置，在每次解析时清除，并每帧作为
`App::set_command_elapsed` 注入 —— delayed 的 `(ctrl+b to run in background)`
提示从 `docs/tools/background.md` 起就一直在等它。带如今经
`App::command_elapsed` 读同一个注入，那个直白的 getter，回答的正是 setter 设下
的东西。提示的门是单独命名的 `App::background_hint_elapsed`：同一个值，在
Ctrl+B 被吞掉的地方（权限 prompt、↓ 管理器带、每个替换 composer 的 picker）掩
成 `None` —— 因为那些会把运行中的单元格留在自己上方，那里的页脚必须继续计数，
而一个它们吃掉的键的提示必须不显示。给被掩的那个按它唯一的用途命名，正是让读者
不挑错孪生项的东西：与 setter 同名的那个 getter 才是诚实的。

**agent session 视图**也有同样的 bug，用的是 agent 的整个运行时长
（`run.runtime`，它的状态行所显示的），并采取同样的形状：一个逐 agent 的时钟
`Session::agent_command_clocks`，在那个 agent 的 `ToolStart` 处插入，并在每次
绘制前作为 `AgentRun::command_elapsed` 注入 —— 思考时钟的孪生
（`docs/agents/agent-view-streaming.md`）。哪些条目仍然适用是在名单 tick 处从
运行自己的队列*推导*出来的 —— 一个没有运行中队首调用的 agent 就丢掉它的时钟
—— 而不是在四个解析事件与两次本地落定处逐个删除，所以将来任何解析路径都不必记
得它。主带不需要这样的修剪：它那一个 `command_start` 在边界自己的解析分支里就
清掉了。

### 时钟行一直在

页脚过去只在输出溢出尾部窗口时才存在：一个输出放得下的命令只显示它的行，而一
条什么都没打印的命令就一直坐在光秃秃的 `⎿ Running…` 上 —— 一个
`sleep 100`、一次静默输出的构建、一套测试的预热 —— 单元格上没有任何东西说它还
活着，任何地方也没有东西说它还要多久。状态行带的是*这一轮*的计时器，那是错的数
字（上面那个被报上来的"页脚在数回合"bug），而模型选的超时只有执行器知道。

实时单元格如今在**每一次运行中单元格所取的各种形状里**都显示命令的时钟，**就配
在它那次调用所属的等待旁边**：

```
● Bash(for i in $(seq 1 100); do echo $i; sleep 1; done)
  ⎿  19
     20
     21
     22
     +18 lines (22s · wait 1m 50s)             ← rows hidden above the window
     (ctrl+b to run in background)

● Bash(python3 -u -c "
      import time…)
  ⎿  hello world
     (10s · wait 10m)                          ← output that fits: the clause alone
     (ctrl+b to run in background)

● Bash(python3 -c "import time; time.sleep(100)")
  ⎿  Running… (10s · wait 2m)                 ← nothing printed yet
     (ctrl+b to run in background)
```

这个等待是模型自己的：`llm::tools::bash_wait_ms` 从调用逐字的
`ToolCall::arguments` 里读 `wait`（秒 —— 或旧的 `timeout`/`timeout_ms`，毫
秒），并套用 `BashArgs::wait_ms` 的规则 —— 调用没点名时两分钟，夹到十分钟上限
—— 所以单元格写明的正是执行器所套用的等待；一个同伴调用点的是它自己的
（`session_wait_ms`：`bashsend` 固定的 10 秒、`bashwait` 的 `wait`）。它以前
标的是 `timeout`，那时这个上限会杀掉命令；如今过了时限的 `wait` 会让它作为
session 继续运行（`docs/tools/bash-tools.md`），标签也就这么说了。一个没有参数
记录的调用（dummy 后端的脚本化调用）显示默认值，那正是这种调用会跑在下面的
值。每个动画帧从参数里读那一个字段是刻意的：一次 `bash` 调用的 `command` 可能
是一千字节的 heredoc，而一个带类型的单字段解析跳过它、不复制它，而解析整个
`BashArgs` 会为了什么都得不到的事情每秒分配三十次那条命令。

上限由 `app::format_timeout` 人性化，它是 `format_elapsed` 的兄弟，把零的部分
丢掉 —— `2m`、`10m`、`1m 50s`、不足一秒的余数时 `1.5s` —— 因为一个上限是一个
整数（`2m 0s` 把同一件事说了两遍），而 elapsed 保留它的秒数，因为它在动。`!`
shell 那行 `⎿ Running… (Ns)` 不变：一条 `!` 命令没有超时，所以没有什么可点
名。

这一行只存在于实时，和它收尾的尾部一样：`tool_lines`（scrollback 提交、权限
prompt 的上下文、冻住的 Ctrl+O transcript）在静止时仍把运行中的命令渲染成
`⎿ Running…`，因为那些界面没有可走的时钟。

### 窗口跟随仍在动的东西

运行窗口过去是输出的**最后**四个显示行，依据是每条命令都共享的那个假设：最新的
输出在末尾。一个就地重画的终端程序会打破它。pacman 并行下载它的数据库，每个文
件保留一根进度条，把光标上移重画仍在走的那些；一个小文件早早完成，它的 `100%`
行随后就坐在那些**仍在动**的条**下面**。那些行全都在终端的可达范围内，所以 pty
transcript 把它们全都作为 `live` 流出
（`docs/tools/interactive-shell.md`），而窗口显示的恰恰是已完成的条，运动却栖身
于它们上方的 `+N lines` 计数里 —— 这就是被报上来的"内联单元格只显示已完成的行；
我得按 Ctrl+O 才看得到条在动"，`sudo pacman -Syy` 配一个 `ILoveCandy` 的
pacman.conf，它那个嚼着的 `C`/`c` 就是始终到不了带上的动画：

```
● Bash(echo "<secret:ROOT_PASSWORD>" | sudo -S pacman -Syy)
  ⎿   multilib                    82.7 KiB  21.6 KiB/s 00:04      ← finished 46 s ago
     [----------------------------------------] 100%
      omarchy                     49.9 KiB  13.7 KiB/s 00:04      ← finished 46 s ago
     [----------------------------------------] 100%
     +5 lines (50s · wait 3m)      ← the heading, core and extra — the two still moving — hidden here
```

（每根条按 pacman 自己的宽度画，在终端的宽度下换行成两行，这就是为什么四行装下
两根条。）窗口如今**结束在仍在动的最低那一行**并向上填充；被藏起来的是已完成的那
些条：

```
● Bash(echo "<secret:ROOT_PASSWORD>" | sudo -S pacman -Syy)
  ⎿   core                       130.6 KiB  21.6 KiB/s 00:06
     [-----------------C  o  o  o  o  o  o  o]  45%
      extra                        8.2 MiB  21.6 KiB/s 00:48
     [----c  o  o  o  o  o  o  o  o  o  o  o]  12%
     +5 lines (50s · wait 3m)
```

"仍在动"的含义在纯核心判定，就在它本就保留的实时尾部旁边。
`App::push_tool_screen` 过去只跟踪一个数 —— 输出末尾有多少字节会被下一次更新替
换。`app::LiveTail` 保留那个数**以及每个实时行一个时间戳**：该行文本最后一次变
化时命令时钟的读数（`App::set_command_elapsed`，即单元格 `(Ns)` 早已显示的、
最多差一帧的逐绘制注入）。一次更新把每个新的实时行与它替换掉的那一行相较，只给
不同的那些打戳；`settled` 行会移动比较的位置，因为它们是旧块的头几行、已出了可
达范围。一行在**最新**变化之后 `app::LIVE_ACTIVE_SPAN`（2 秒）之内变化过，就算
**活跃**，而 `LiveTail::anchor` 把最低的活跃行命名为窗口结束处的那条显示行
—— 没有活跃行时就是最后一行。`running_command_lines` 取调用旁边的那个尾部，把
窗口结束在那条行上并向上填充 —— 而当锚点之上的行比窗口装得下的少时，向下用后面
的内容补，因为一个藏起它明明装得下的行的窗口等于白折；页脚数每一个被藏起来的
行，窗口下方的也一样，所以 `+N lines` 仍是 Ctrl+O 多出来的东西。结束在仍在动的
最低行而不是从它开始是刻意的：那条线以下的每一行按定义都是静止的，而它以上的行
才是唯一也可能在动的 —— pacman 的 `core` 在它的 `extra` 之上 —— 所以向上填充
才是把两根动的条都留在屏幕上的做法。这个定义带出两个结果：

- **每条普通命令都不变。** 新的一行既是最新的变化也是最低的实时行，所以锚点就是
  末尾，窗口就是它一贯的尾部；一条跑在管道上的命令把它那半行作为 `live` 流出，
  答案相同；而默认的 `LiveTail` —— 没有实时行 —— 也锚在末尾，这正是离线
  dummy 的 `ToolOutput` chunk 与每个从不流式输出屏幕的单元测试拿到的。
- **停顿时窗口不动。** 这个跨度从最新变化量起，不从时钟量起，所以一个十秒什么
  都没收到的下载会让窗口停在它的条上，而不是掉到已完成的行、等数据再来时又弹回
  去。

新调用从一个空尾部**和**一个清掉的时钟开始：`start_tool` 会重置注入的
`command_elapsed`（下一次绘制重新注入它），所以上一条命令的读数绝不会给下一条的
头几行打戳 —— 一个过期的时间戳会在它那行静止期间一直算最新的，于是每一次真正的
重画都会被读成太旧、不算在动。追加的文本（`push_tool_output`）会让实时行定稿，
所以一个混用两种形式的后端，绝不会让一次更新替换掉在它之后追加进来的 chunk。
subagent session 视图取同样的形状：`AgentRun` 保留它自己的 `LiveTail`，用自己
注入的命令时钟打戳，并在它的 `ToolStart` 处同样重置
（`docs/agents/agent-view-streaming.md`），而同一个 `live_call_lines` 渲染器服
务两条带，所以 subagent 的 `pacman` 读起来与主回合的一样。

### `Exit code: N` 框是模型的，绝不是单元格的

`tool.output` 保持带框（`Exit code: 0\n…`），因为
`context::context_messages` 在后续回合里把它逐字重放为面向模型的 `tool` 结果
—— agent 的上下文窗口（因而 Ctrl+D）才是 exit code 住的地方。**显示**
（`command_display_output`）则不管它说什么都丢掉那一行框 —— `Exit code: 0`、
`Exit code: 1`、`Exit code: killed by signal` —— 于是单元格读起来像真实的命令
输出，而不是框：

```
● Bash(false)
  ⎿  (no output)
```

说这次调用失败的是那个圆点（红色，`ToolStatus::Failed`），而读到 code 的模型才
是报告或据此行动的那个。这以前不一样：一次失败会把框改写成正文之上的一行
`Error: Exit code N`（或 `Error: killed by signal`），把模型的记账摆到屏幕上、
摆在命令打印的内容前面。只有当框那一行确实存在时才动，所以非 `bash` 的 tool、
`!` shell 单元格（它们的输出是原生的、从不带框，以它们自己的
`[exit status: N]` 收尾 —— `docs/input/shell-command.md`）以及旧的 rollout 都
不受影响。

## Ctrl+O overlay 里的实时（我们在流，Claude Code 不流）

Claude Code 的 transcript 翻页器只在 tool **结束**后才显示它的输出；它运行期
间单元格是一个静态 spinner。我们的更好一些 —— **Ctrl+O overlay 随 `bash` 流式
输出实时更新**，所以你可以把它弹开，在全屏视图里看一条长命令的输出流进来（跟随
到前沿）。

两件事让这成立，而除了一样之外它们本来都在：

- **overlay 挂着时循环仍持续抽取回复事件**（不变量 4），并在回合进行时每 ~30 fps
  重新挂一次绘制，所以 `ToolOutput` 增量到达 `App::push_tool_output`，overlay
  随之重绘 —— 不需要任何 overlay 专属的接线。
- **`TranscriptCache` 的签名**（`ui::TranscriptSig`）必须注意到运行中调用的输出
  在增长，否则缓存的 transcript 永不重建、overlay 就变成静态的。签名的 tool 队
  列项是队列长度加上**每个实时调用的状态与输出长度** —— 逐调用那部分是修复：一
  个运行中调用的长度与状态在它流式输出期间不变，但它的 `output.len()` 变，所以
  每个增量都让缓存失效、overlay 重新渲染（而且因为它默认**跟随尾部** ——
  `App::tool_follow` / `settle_tool_scroll`，打开时接上 —— 会把新输出滚进
  视野）。并行批次里好几个单元格同时在流，所以这一项覆盖它们全部，而不只是最前
  面的那个。单元测试
  `transcript_cache_rebuilds_as_a_running_bash_streams_output` 锁住它；smoke
  Phase 40 端到端证明它（一个运行中的 `Bash(ping)` 单元格流进 overlay，而它的
  兄弟仍显示 `⎿ Waiting…`）。

overlay 经 `tool_full_lines` 渲染运行中调用的输出（*整个*迄今流出的输出，不设
上限），与显示已完成 tool 的是同一次遍历 —— 所以运行中的单元格与已完成的读起来
一模一样，只是还在长。

## tool 运行之前：`write` 在生成时就在流

上面的一切流的都是一个调用**运行之后**的**输出**。一个 `write` 调用的**输入**
也在流：模型在回合结束之前要花好几秒生成它的 `content`，而文件一到就流进单元
格 —— 到达期间是最新的十行带编号的行，到齐之后是前十行加上 `… +N lines` 提示，
与已提交单元格自己的一瞥相同。那是另一条流（这一轮的 `ToolCallDelta` 片段，而
不是本节讲的 `ToolOutput`/`ToolScreen`），有它自己的设计：
`docs/ui/write-streaming.md`。

## 演示（dummy 后端）

`stream::turn_events` 在每个 `Bash` 调用的 `ToolStart` 与 `ToolEnd` 之间穿插几
个 `ToolOutput` chunk，所以 `cargo run`（以及 `scripts/smoke.sh`）无需 API key
就能显示实时尾部。已提交的 scrollback 不变 —— 最终单元格仍来自 `ToolEnd`。

## 对着真实模型验证

`examples/tool_smoke.rs` 打印每个事件，包括 `ToolOutput`：

```bash
OPENROUTER_API_KEY=sk-... ZEROTH_CA_FILE=/root/.ccr/ca-bundle.crt \
  cargo run --example tool_smoke -- openai/gpt-4o-mini \
  "Use bash to run: ping -c 10 google.com"
```

## 矮终端会冻住单元格而不是裁掉它

实时尾部是区域里有弹性的内容，所以一个放不下整个单元格的终端过去会把它的行
—— 先是 `+N lines (Ns · wait …)` 页脚，然后是输出行，然后是表头 —— 丢进无处
可放。带如今底部锚定，把它画不下的行提交进终端自己的 scrollback，冻住：单元格
**滚动**，最新的行与它的时钟行留在屏幕上，而它的头部仍可向上滚读到。见
`docs/ui/strip-flow.md`。
