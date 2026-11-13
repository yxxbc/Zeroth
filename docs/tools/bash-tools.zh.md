# bash 系列 tool —— 每条命令都跑在真终端里，一个动作一个 tool

模型通过五个 tool 运行命令。每条命令都跑在**bash** 之下一个**真终端**里，每个
tool 只做一件事：

| tool | 参数 | 做什么 | 是否询问权限 |
| --- | --- | --- | --- |
| `bash` | `command`、`wait?`、`description?` | 在它自己的终端里跑一条命令；命令退出、停下来要求输入，或 `wait` 秒过去时返回 | 是 —— 按命令询问 |
| `bashsend` | `session_id`、`input` | 往一个运行中的命令里打字，返回它的答复 | 是 —— 按 session 输入询问（单独一个 `<C-c>` 不算） |
| `bashwait` | `session_id`、`wait?` | 等新输出、等退出，或等一个问题；`wait: 0` 只查一下 | 从不 |
| `bashkill` | `session_id` | 停掉一个命令以及它启动的一切 | 从不 |
| `bashlist` | — | 正在运行的 session，以及哪些在等输入 | 从不 |

这几个同伴的 wire 名各是一个小写单词 —— `bashsend`、`bashwait`、`bashkill`、
`bashlist` —— 这是 task 系列 tool 的惯例（`taskcreate`、`tasklist`）：单元格
显示 `BashSend`，而显示名小写化之后就是 wire 名，所以上下文重放不需要为它们
准备一张表。

它取代了 `bash` 的 `tty`、`timeout`、`run_in_background` 三个 flag，以及那个
四个动作挤在一个 tool 里的 `bash_session`（`docs/tools/interactive-shell.md`
有那一版设计的历史，以及两者共用的 pty 引擎）。

## 为什么

旧形状对模型提出了什么要求，这是通过真实执行器（`examples/pty_drive.rs`）量
出来的：

- **命令跑起来之前就得选定一种模式。** `tty` × `run_in_background` 组合出四种
  stdin、超时与返回规则各不相同的行方，而 `tools::tty_hint` 的存在就是为了挽救
  一个选错了的模型。
- **`timeout` 以毫秒计，却意味着三件事。** 普通调用上的杀死死线、带 `tty` 时的
  等待预算、后台里被忽略 —— 而一个本意是秒的模型会丢掉它的命令：
  `"timeout": 30` 在 30 毫秒之后杀掉了 `sleep 1; echo finished`。
- **`bash_session` 是四种动作挤在一种调用形状里** —— 打字、等待、杀死、打完再
  杀 —— 靠哪些可选字段被设置来区分。它的实时模型失败大多恰恰来自这一点：
  `stop` 被读成*停止等待*、最后一条答复带着 `kill` 发出、`"input": ""` 本意是
  回车，每一个都用一个别名或一句备注来打补丁。
- **`bash` 跑的是 `sh -c`**，而在 Debian、Ubuntu、Kali 和这个项目自己的 Docker
  镜像上那是 dash：`[[ … ]]`、`source`、`{1..3}` 和 `pipefail` 都以
  `sh: 1: [[: not found` 失败。
- **长输出被悄悄截断。** 普通调用留前 64 KB，在行中截断，什么都不告诉模型
  （那个 `…` 只属于单元格）；`tty` 调用只留最后 2000 行。无论如何都丢了一头。

## 契约

### `bash`

```json
{"command": "cargo test", "wait": 300, "description": "Run the test suite"}
```

命令以 `bash -c {command}` 运行 —— 在没有装 `bash` 的地方是 `sh -c`
（`subprocess::tool_shell`）—— 跑在**它自己的伪终端**里（120×40，
`docs/tools/interactive-shell.md` 的*终端*），工作目录不变。调用在下面第一件
事情发生时返回：

- **命令退出** —— 输出之上是 `Exit code: N`，正是普通调用一直有的那个框；
- **它在等输入** —— 它打印的东西之上是
  `Running (session {id}, waiting for input)`（在密码提示处是
  `waiting for a password — only bashsend can type it`），命令仍在运行；
- **`wait` 秒过去** —— 目前输出之上是 `Running (session {id})`。**命令没有被
  停下**：它作为 session 继续跑，模型用 `bashsend`/`bashwait`/`bashkill` 接着
  操作它，命令退出时会被告知。`wait` 以秒计（默认 120，上限 600），一个模型只
  会用一种单位写的数。

`wait: 0` 把命令**放到后台**启动：调用立刻返回 session id 和它日志的路径，退出
时跟进一条完成 notice。以单个 `&` 收尾的命令按同样方式处理 —— `&` 被去掉 ——
因为那正是模型要的，而一个 shell 随后就退出的命令里的 `&` 否则会停掉它启动的
东西。

模型以前写过的任何东西都不会被拒绝。执行器仍然读旧的写法 —— `timeout` /
`timeout_ms`（毫秒）、`run_in_background: true`（当作 `wait: 0`）、`tty`（如今
每条命令都有终端）—— 所以 hook 的 `updatedInput`、旧夹具或模型的习惯都还能用；
只是 schema 不再提供它们。

### `bashsend`、`bashwait`、`bashkill`、`bashlist`

```json
{"session_id": "b7x2k9m1q", "input": "import math; print(math.pi)<Enter>"}
{"session_id": "b7x2k9m1q", "wait": 600}
{"session_id": "b7x2k9m1q"}
{}
```

- `bashsend` 打出 `input`（`docs/tools/interactive-shell.md` 的按键记法 ——
  `<Enter>`、`<C-c>`、`<Up>`，一个换行就是按回车），并在程序再次等待、退出，
  或它的固定预算（[`SEND_WAIT`]，10 秒）过去之后返回答复。它从不杀死。提交
  密码之后，仅凭换行或终端控制序列不会让旧提示变成又一次输入请求：需要可见的
  答复文字，或者探针看到的一次终端读，而一个可见地被重新打印的重试提示也算。
- `bashwait` 什么都不打，最多等 `wait` 秒（默认 120）等退出或一个问题 —— 沉默
  不会结束它，所以一次调用能等完一次长构建。`wait: 0` 立刻返回新的内容。一个
  已经在等模型看得见的按键的程序 —— 一个停在提示符前的 REPL，没打印新东西 ——
  立刻结束等待，而不是把它耗到预算尽头。Linux 上一个被识别出的 Node REPL 会
  把已提交的一行保持在它自己的回显和普通的换行结尾日志之外，于是顶层 `await`
  期间的 raw 终端模式不会被误认成它的下一个提示符。这个范围包含受支持的
  Node/V8 runtime flag 以及 `ts-node`/`tsx` 启动器符号链接。一个沉默的异步
  stdin 监听者在不打印问题时仍可能报 `Running`；可见的嵌套问题和内核观察到的读
  仍然可被检出。完整范围与限制在 `docs/tools/interactive-shell.md`。
- `bashkill` 先请命令停下 —— `SIGINT`，然后 `SIGTERM` —— 并在 [`KILL_GRACE`]
  （2 秒）之后对剩下的一切发 `SIGKILL`，session 的整棵进程树都在内：只用
  `SIGKILL` 停下的 `docker compose up` 会把它的容器留着继续跑。每个信号都到达
  该 session 里的每一个进程 —— 按 session 的 id 找，在 macOS 上（它的 `pkill`
  没有 `-s`）则按 session 的终端找（`docs/tools/interactive-shell.md`）。
- `bashlist` 点名每一个正在运行的 session —— 它的 id、它的命令、它跑了多久，
  以及它是否在等输入 —— 于是一个因为 `/compact` 或 `/resume` 丢了 id 的模型
  不用猜就能找回它。停在密码提示处的 session 按报告自己的从句列出
  （`waiting for a password — only bashsend can type it`），所以从列表得知该
  提示的模型，听到的话与一份报告会告诉它的一样。

每个结果都是同一套小语法：`Exit code: N`、`Running (session …)`（带或不带
`waiting for input`）、`Stopped (session …)`，以及模型上一次看之后的输出。

### 每次退出一条 notice

一个 session 可能**在模型正往它写调用的时候结束**：`bash` 在一个 `sudo` 密码
提示处返回，然后命令自己就结束了（凭据已缓存、指纹），而模型那段带密码的
`bashsend` 几秒之后才到。该次退出的 `[background] … completed` 备注是在那一轮
请求发出之后才投出的，所以模型没有读到它 —— 然而那个调用却答复
`No running session … its final output was already reported`，备注在它之后才
到。模型往虚空里打了字，被告知了一件不真实的事，并且晚了一轮才知道真相。

所以一次**没有任何调用报告过的**退出（监视器在没人等的时候把它结掉 ——
`Finish::Now` —— 或者等待中的那个调用报告的是别的东西）会连同 session 的输出
一起留在注册表里，直到它的备注被读到（`background::UnreadExit`）。在这个窗口里
点名该 session 的同伴调用会**认领**它（`BackgroundRegistry::claim_exit`）并自行
报告这次退出 —— 那一眼把退出覆盖在内，`Exit code: N` 之下是模型上次看之后命令
打印的东西，与一次亲眼看到退出的调用报告得一模一样 —— 外加一条只给模型看的
备注，说明那个调用变成什么样了（`pty::report::EXITED_BEFORE_INPUT_NOTE`：什么
都没打进去；`EXITED_BEFORE_KILL_NOTE`：没有可停的；用户按 `x` 结束它时是
`USER_STOPPED_NOTE`）。本来会说同样内容的备注于是不再欠着，靠的是 agents 那套
*每个答复一条 notice* 的规则在同一把锁下：

- 认领先到 —— 循环的 `post_exit_notice` 发现退出已被认领，什么都不投，也不
  记录 notice 单元格；
- 备注已投但未读 —— 认领把它从公示板上取回，注册表记住它的编号，于是循环丢掉
  它为它持有的那个单元格（`take_retracted`）；
- 备注已被读 —— 没有可认领的，调用被告知 session 已经结束、怎么结束的、以及
  它的最终输出已经报告过，而这又成了真的。

**只有备注自己的读者才能认领它。** 公示板是 lead 的：一个 subagent 的调用点名
lead 的 session —— 不管它是怎么知道那个 id 的 —— 绝不能把 lead 的备注取回去，
否则 lead 永远听不到自己的命令是怎么结束的。所以一条备注正躺在公示板上的退出，
归 lead 认领；而一条还没投出的，归它的启动者（`UnreadExit::claimable_by`）。

一次没有退出可认领的调用，不会在注册表明明知道更多的时候被告知该 session
“has exited (its final output was already reported) or never existed”。它记住
自己最新的那些结束 —— 代号、是不是它自己的 kill 干的、有没有一份报告覆盖过、
以及那条备注是谁的（`background::EndedSession`，最近 64 条）—— 并说出这个
session 是怎样结束的，以及那对本次调用意味着什么：输出已经报告过，如果那条
备注本该由调用方读，或者一份报告覆盖了这次退出；否则是
`It is not this conversation's session: its exit is reported to the main
conversation`（或 `to agent {id}`），什么都没打进去、也没停掉什么。模型自己的
`bashkill` 升级后的停止读作 `was stopped`，绝不是 `by the user`。

**subagent** 启动的 shell 报到那个 agent 自己的队列而不是公示板，那里会发生
同样的竞态 —— 一个 subagent 的 `bashsend` 打进它自己那个在它写调用时已经结束的
session。所以被路由的备注随身带着这次退出的认领细节：循环在 agent 注册表的锁下
一步完成路由，向公示板询问它是否仍然欠着
（`AgentRegistry::route_shell_note` → `BackgroundRegistry::take_shell_notice`，
agent 自己的 notice 已经在用的“先 agents 后公示板”的锁序），而 subagent 的调用
**从它自己的队列**认领它（`AgentRegistry::claim_routed_exit`，经 `claim_exit`
的 `caller` 到达）。只有启动它的那个 agent 可以：别人把那条备注取回去，会让启动
这条命令的 agent 永远听不到它是怎么结束的。被认领的退出什么都不路由；agent 的
循环已经读过的备注会被报告，调用也会被告知这一点。被路由的备注按公示板自己的
计数编号（`Route::Routed(seq)`），所以当 agent 自己的调用把它取回时，循环为它
持有的单元格随之而去，与公示板备注的行为完全一致。

同一条规则覆盖**等待输入**的备注：同伴调用的一眼就把 session 正在问的东西展示
给模型，于是它把那一条未被读过的 `… is waiting for input` 备注取回
（`retract_shell_notices`，在那一眼之后）—— lead 的从公示板上，subagent 的从它
自己的队列里（`AgentRegistry::withdraw_routed_notes`），而且只取调用方自己的：
一个 subagent 看了 lead 的 session，是它看见了提示，lead 没看见。循环在那一眼
之后才投的备注干脆不投（`SessionIo::prompt_seen`）。否则这条备注会在模型的答复
之后到达，读起来像第二个问题。

`smoke.sh` Phase 131 端到端重放那个被报告的竞态：一个 stub provider，它的
`bash` 调用在 `Password: ` 提示处返回，然后花六秒写那条 `bashsend`，而命令
在此期间自己结束了 —— 调用必须读回 `Exit code: 0` 和 `Nothing was typed`，
并且不得有任何 `[background]` 备注到达 wire 或屏幕。

旧的 `bash_session` tool 仍然会被**执行**（一个被续上的对话可能持有对它的调用，
模型也可能记得它），但不再提供，而它记下的调用在重放时按如今做同样事情的 tool
来呈现：一次 kill 作 `bashkill`、打进去的输入作 `bashsend`、其他一切作
`bashwait`（`context::record_calls`），于是请求携带的每个调用都点名一个该请求
提供的 tool。

## 默认就跑在终端里需要付出什么

让每条命令都跑在终端里，正是 `docs/tools/interactive-shell.md` 拿来与这一版
比较的另一套设计，当初因三个理由被否：输出应该是*数据*、程序对终端有不同的
格式化、以及模拟一个终端是有开销的。只翻转默认值还会弄坏安静的命令。翻转之后
跟着来的东西：

1. **安静的命令不等于在等待的命令**（`pty::settle`）。终端启动过去在沉默 2 秒
   （`LINE_QUIET`）之后返回，不管命令在做什么：`sleep 4; echo done` 在 2.02 秒
   时回来 `Running`，然后无人看着地退出 —— 每个安静的 `curl`、链接步骤或
   `terraform plan` 都会带来一条完成 notice，还可能带来一次自动的后续回合。现在
   只有在核无法看见程序在干活时，沉默才结束一次启动或一次输入：探针看到**全都
   在干活**的一棵树（`Probe::Idle`）会跑到退出或它的 `wait`；一棵在等描述符的树
   —— 等终端（`Probe::Polling`），或者谁也不等（`Probe::Elsewhere`：一个网络
   客户端、一个空闲服务器，正如它的 `poll`、`select` 或 epoll 集所说）—— 或者
   探针看不见的一棵（`sudo`、macOS），必须沉默 [`LAUNCH_QUIET`]（10 秒）才让
   启动返回 —— 除非一个不等任何终端的进程有一个后代在干活，那也让它的等待算作
   干活：一个编译器沉默运行的构建（cargo 轮询 rustc 的管道）会跑到退出或它的
   `wait`，而不是回来 `Running`，而一个停在提示符前的 shell 或 REPL、身后有个
   任务在干活，仍然被看成停在提示符前。提示符仍然在半秒内结束调用，与以前一样
   —— 但一棵只在别处等待的树，或者一个把终端留在行模式下的备用屏显示
   （`gh run watch`），都不是提示符，对它的等待会陪着它耗过去
   （`docs/tools/interactive-shell.md`）。一个被隐藏的光标仍然可以标记一个问题或
   一个菜单。它的位置只在有显示的证据时被否决：一次在实时输出末尾之上的重写，
   排除被擦掉的菜单尾部，或者在光标处的一个数字百分比，并伴随一条进度条或传输
   提示。Pacman 最初的 0% 帧和被重写的仓库状态行满足这些检查；只出现在问题里的
   一个百分比不满足。同样的显示也会让一次安静的启动或 `bashsend` 等到退出、
   另一个输入信号或它的等待预算，而不是在通常的沉默截止处返回。于是一个陈旧的
   进度显示可能占住整个预算；它是屏幕上的证据，不是工作仍在继续的证明。内核
   观察到的读、密码提示、读键的程序和交互式界面保留它们各自的输入信号。
2. **结束后的输出是数据**（`pty::session` 的数据视图）。终端的 transcript 是人
   读的东西 —— tab 展开到制表位、行尾空格被削掉 —— 所以 `printf 'a\tb   \n'`
   回来是 `a       b`。同一条流还会按普通调用一直折叠管道的方式再折叠一次
   （`pty::fold`），而一条**在自己的调用内退出**、没有画过屏幕的命令 —— 没有光标
   寻址、没有回退到自己的行上、没有备用屏 —— 报告的就是那次折叠：tab、行尾空格
   都在，正是管道返回的东西。
3. **长输出两头都留**（`pty::fold::HeadTail`）。保留开头 [`HEAD_BYTES`] 和结尾
   [`TAIL_BYTES`]，截断处用单独一行说明 ——
   `[… 12345 lines omitted — the full output is in
   /tmp/zeroth-1000/{session}/tasks/{id}.output]` —— 点名每个 session 本来就在
   写的日志（`crate::background`），模型可以 `read` 它或搜索它。日志保留每一行：
   以前一次读进一个 chunk 的突发输出会从中间丢行，丢在 transcript 那个 2000 行
   的保留上限上，因为换行是在取该 chunk 已完成的那些行之前施加的，所以
   `SessionIo::absorb` 每次喂它一千行。没有日志的管道兜底会说明它丢了多少。
4. **停下来发问的后台 session 会说一声。** 以前只在退出时投 notice；一个问
   `Port 3000 is in use. Use another? (Y/n)` 的 dev server 会在无人看见的情况下
   干等。现在，当一个宣告过的 session 没人在等、并且在模型上次看之后它打印出的
   提示符处安静地坐了 3 秒时，监视器会投一条 notice（`BgEvent::Waiting`）——
   报告方式与一次退出相同：一个琥珀色的
   `● Background command "…" is waiting for input` 单元格、一条点名该 session
   的备注、模型启动时还带一次空闲的后续回合。只有形状像提示符的一行才算，探针
   随后会核实那不是一个在干活的程序，也不是一个只在等若干非终端文件的程序 ——
   一个一边逐键占着终端一边等管道的显示什么也没问 —— 而一个已被告知过的
   session 在模型看过它之前不会再被告知，于是一个一直发问的程序无法一轮接一轮地
   开启回合。
5. **session 上限计的是活过它那次调用的东西。** 16 个 session 的上限过去把每个
   终端都算上，包括还在它自己调用里的命令，于是并发 subagent 的普通命令也可能
   撞上它。
6. **是 bash，不是 sh。** 每个进程从 `PATH` 解析一次；一个非登录、非交互的
   shell，所以预先设置的 `ENV` 仍然能到达每条命令。
7. **在没有终端可用时用管道。** 在 Unix 之外，或者 `setsid` 和 helper 都无法给
   命令一个控制终端时，命令像以前那样跑在管道上 —— 同样的框、同样的数据视图 ——
   而过期的 `wait` 把它交给后台注册表而不是杀死它；完全没有注册表时（一个嵌入
   方、一个测试）就在它的 `wait` 处被杀掉，这是对它唯一能做的事。

留下的取舍是终端本身的取舍，而结果每次都说明发生了什么，所以没有一条是无声的：

- **终端格式化。** `ls` 打列、`ps` 在 120 列处截断、颜色被剥掉、进度条显示它的
  最终状态。`| cat` 给程序一个管道去格式化。
- **读 stdin 的程序会等它。** 没有文件的 `grep needle` 在半秒内返回
  `Running (session …, waiting for input)` 而不是 `Exit code: 1`；`< /dev/null`
  给它文件结尾。在 Linux 上这也覆盖通过 `splice` 读的现代 GNU `cat`，只要内核的
  等待通道确认了一次终端读（`docs/tools/interactive-shell.md`）。
- **`sudo` 会问密码**而不是失败：那个框说
  `waiting for a password — only bashsend can type it`，并且模型被告知绝不要输入
  一个没人给过它的密码。这个从句点名那条唯一的入口，因为用户没有入口：过去只
  被告知打进去的输入是被隐藏的，于是模型请用户“在终端提示里输入它”，一个他到
  不了的终端 —— 现在它在对话里要密码。
- **输出是流式的。** 一个只在终端上行缓冲的程序（Python、大多数 CLI）会边跑边
  显示输出。
- **留下的任务不总被点名。** 命令退出时 `server & curl …` 会停掉 `server`，而
  监视器发现那个任务还在命令的进程组里时，报告会说一声
  （`tools::REAPED_NOTE`）。在 Linux 上，一个只含已退出、未收割线程的组不算作
  运行中的工作；一个活着的 worker 即使主线程已退出仍然算，而读不到的细节保持
  不确定。终端在同一瞬间把那个组挂断（该任务在它的前台进程组里），所以一个不
  忽略挂断的任务在监视器看过去时可能已经没了 —— 在一台 init 立刻收割孤儿的机器
  上，一个普普通通的 `sleep 30 &` 在 20 次运行里有 4 次没被点名。它无论如何都
  被停下了；缺的只是那条备注。在管道上没有任何东西挂断这个任务，所以那里的备注
  是准确的。

模拟的开销在一条琐碎命令上什么也没量出来：`echo hi` 经执行器走两条路都是
0.00 秒。

## 接线

- **权限**（`llm::approval::permission_request`）：`bash` 按命令询问，`bashsend`
  按 session 输入询问（继承启动命令的允许清单规则，
  `docs/tools/interactive-shell.md` 的*权限*）；`bashwait`、`bashkill` 和
  `bashlist` 从不询问 —— 过去是一个 tool 里的特例，如今是 tool 的名字。保存的
  规则按命令文本作键，而不是 tool 名，所以每条规则都能存活。
- **auto 模式的分类器**审查 `bash` 命令和 `bashsend` 的输入，即那两个会询问的
  调用。
- **Hooks**（`hooks::claude_code_alias`）：每个 tool 也响应它单元格的显示名，
  作为第二个精确名字 —— `Bash`、`BashSend`、`BashWait`、`BashKill`、`BashList`
  （旧 tool 则是 `BashSession`）。
- **Subagent 的 tool 列表**（`subagents::AgentTools::allows`）：`Bash` 授予整个
  家族，而每个同伴也响应它的显示拼法（`BashSend`），小写化之后就是它的 wire 名。
- **单元格。** `● Bash(npm run dev)`、`● BashSend(python3 ← print(1)⏎)`、
  `● BashWait(cargo build)`、`● BashKill(npm run dev)` —— 一个 session 从调用被
  宣告的那一刻起就用它的命令点名（`tools::summarize_call_naming`）。运行中单元格
  的时钟点名这次等待：`(22s · wait 2m)`。

[`SEND_WAIT`]: ../src/llm/tools.rs
[`KILL_GRACE`]: ../src/background.rs
[`LAUNCH_QUIET`]: ../src/pty/settle.rs
[`HEAD_BYTES`]: ../src/pty/fold.rs
[`TAIL_BYTES`]: ../src/pty/fold.rs
