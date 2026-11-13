# 交互式 shell —— bash 工具背后的终端引擎

> **工具面已经变了**（`docs/tools/bash-tools.md`）。现在每一条 `bash` 命令都跑在
> 终端里 —— 没有 `tty` flag —— 而且是跑在 `bash` 而不是 `sh` 之下，
> `bash_session` 的四个动作变成了四个独立的工具：`bashsend` 打字、`bashwait`
> 等待、`bashkill` 停止、`bashlist` 列举。本文保留第一种形状的设计历史，并描述
> 它们至今都还跑在其上的那台引擎（`pty/`）：把*一次 `tty` 调用*读作任意一次
> `bash` 调用，把*`bash_session`*读作执行那个动作的伴随工具。
> `bash_session` 本身仍会被执行 —— 为了一段持有对它的调用的恢复会话 —— 但不再
> 被提供。

`bash` 工具过去是**只执行**的：命令在 `/dev/null` 上拿着 stdin、没有终端地跑，
调用则一直等到它退出。这是对的默认值 —— 编码 agent 运行的大多数命令要的正是
这个 —— 但有一大类有用的程序需要更多东西：

- **prompt —— `npm init`、`apt install`（`[Y/n]`）、`git add -p`、`ssh` 的
  主机密钥提问、脚本里的 `read -p`、prompt；
- **REPL** —— `python3`、`node`、`psql`、`sqlite3`、`gdb`、`pdb`、`irb`，
  一个在命令之间留住自己的 `cd` 和 virtualenv 的 shell；
- **全屏程序** —— `vim`、`less`、`htop`、`top`、`git rebase -i` 的编辑器、
  脚手架工具的上下键菜单；
- **只在终端上才正常的程序** —— 行缓冲的进度、颜色、改变工具行为的 `isatty()`
  检查，或者干脆拒绝运行。

`bash` 新增一个 `tty` flag，把命令放进一个真正的**伪终端**里运行，并在它还在
运行时返回；另有一个伴随工具 **`bash_session`**，向它打字、等它、读它、结束它。

## 模型看到什么

### `bash` —— 新增一个参数

```json
{"command": "python3 -q", "tty": true}
```

`tty: true` 把命令放进一个伪终端（120 列 × 40 行），作为新 session 的控制终端
运行。命令**退出**、**停下来等待输入**，或者它的 `timeout` 到点 —— 三者中哪个
先发生，调用就立刻返回。已退出的命令报告的东西和一次普通 `bash` 调用完全一样
（输出之后跟一个 `Exit code: N`）。仍在运行的命令则在具名其 session 的一行框架
之下，报告到目前为止的输出：

```
Running (session b7x2k9m1q, waiting for input)
>>>
```

`tty: false`（默认）保持不变：stdin 接在 `/dev/null` 上，没有终端，调用等到
退出，并在 `timeout` 时杀掉命令。非 TTY 有意保持为默认 —— 终端会改变程序格式化
输出的方式（颜色、进度条、`\r` 重绘），而一条批处理命令从中得不到任何好处。
`bash` 的描述用一条 bullet 说明何时该用它：prompt（`[Y/n]`、密码）、REPL、
全屏程序 —— *以及当一条命令说它需要一个终端时，带上它重试* —— 存在非交互
flag（`-y`、`--yes`）时优先用它。

`tty` 还能与 `run_in_background` 组合：命令在终端里启动，调用立刻带着它的
session id 返回，完成通知在它退出时随后到达。现在每一次后台启动都会具名它的
session id，所以 `bash_session` 能把其中任何一个接回来。

### `bash_session` —— 伴随工具

```json
{"session_id": "b7x2k9m1q", "input": "import math; print(math.pi)<Enter>"}
```

| 参数 | 含义 |
| --- | --- |
| `session_id` | 必填 —— 某次 `bash` 调用报告过的 id |
| `input` | 要打的内容；一个换行按下 Enter，按键写在尖括号里：`<Enter>`、`<Tab>`、`<Esc>`、`<BS>`、`<Up>`/`<Down>`/`<Left>`/`<Right>`、`<PageUp>`、`<C-c>`、`<C-d>`、`<C-z>`、`<M-x>`、`<F1>`…… 省略它就只是等待更多输出。 |
| `timeout` | 等待命令响应的最长毫秒数（默认 10000，最大 600000） |
| `kill` | `true` 结束这条命令以及它启动的一切 —— 只用于放弃它 |

结果**只有模型上次看之后打印出来的输出**，处在同一个框架之下：运行中是
`Running (session …)`，退出之后是 `Exit code: N`（此后 session 就没了），被杀
之后是 `Stopped (session …)`。在密码 prompt 上框架会说明 ——
`Running (session …, waiting for a password — only bashsend can type it)`
—— 这也告诉模型没有别人能回答它（下文的*要密码的 prompt 自己会说明*）。一个全屏
程序 —— 跑在终端备用屏上的那种 —— 返回的是它的**当前屏幕**而不是
transcript，因为一串按光标寻址的重绘作为文本毫无意义：

```
Running (session b7x2k9m1q, waiting for input)
Screen (40x120, cursor at line 3, column 14):
inserted line
line one
line two
~
                                                    1,13          All
```

屏幕上只靠颜色表达的东西 —— 菜单里被选中的那一项、有焦点的按钮 —— 会在下面
点名，因为它的文字说不出来（下文的*一个全屏程序长什么样*）：

```
Running (session b3vqd0sq1, waiting for input)
Screen (40x120, cursor at line 17, column 54):
                                        ┌──────────────┤ Fruit ├───────────────┐
                                        │ Pick a fruit                         │
                                        │                                      │
                                        │            apple  Apple              │
                                        │            banana Banana             │
                                        │            cherry Cherry             │
                                        …
[Highlighted: line 17 "banana Banana"]
```

`bash_session` 对注册表持有的每一个 shell 都有效，不只是 TTY 的：一条
`run_in_background` 命令（或者用户用 Ctrl+B 移到后台的命令）也能以同样的方式被
等待和结束。只有 TTY session 接受打字 —— 后台命令的 stdin 是 `/dev/null` ——
但单独一个 `<C-c>` 仍能中断一个，作为发给它进程组的 `SIGINT`。它的输出走管道，
管道上没有终端把每个 `\n` 变成 `\r\n`（TTY 的 `onlcr`），所以它的 transcript
把换行当作新的一行（`Transcript::for_pipe`，VT 的 new-line 模式）。若按终端的
换行来读 —— 那会保留列号 —— 一台后台服务器日志的每一行都起于上一行结束的地方，
而一旦超过一行 8192 列的上限，最新的那些行就被整行丢掉。

模型读到的描述简短而命令式（`src/llm/tools.rs`，由一个测试钉在 900 字符以内）：
按键记法，**每次调用回答一个 prompt，以 `<Enter>` 收尾，然后读它接下来问什么**，
**在全屏程序里每个键立刻生效** —— `q`、`<Down>` 或 `<F7>` 单独发，而不是
`q<Enter>`，按键条上的 `7Mkdir` 读作 `<F7>` —— 等待不需要 input，`kill` 只用于
放弃一条命令（*自己跑完的会自己退出*），而它没有被告知的密码或密钥永不被猜 ——
它去问用户。`input` 的示例把 Enter 写成按键（`"y<Enter>"`），从不写成 `\n`
—— 见*活模型教给设计的东西*。

## 为什么是这个形状

**两个工具，不是一个。** `bash` 始终只意味着「跑一条命令」；一切*对一条正在
运行的命令*做的事都走 `bash_session`。一个被重载的 `bash`（OpenHands 的
`is_input` flag）会把 agent 串行化到一台终端上，并让「这段文字是一条命令还是
按键？」成为模型每次调用都得回答的问题。两个工具、它们之间只共享一个 id，也让
并发免费：好几个 session、subagent 的 session，以及后台 shell 全都共存。

**让出点由替模型决定。** Codex 的 `exec_command`/`write_stdin` 在一个由模型选定
的固定 `yield_time_ms` 之后返回，而模型选得很糟：太短就轮询成环，太长则打进
REPL 的每一次击键都要等满时钟。这里调用在命令**安定**（settle）时返回
（`pty::settle`，纯函数且有单元测试）：

- 它**退出了** —— 输出排干之后立刻返回；
- 内核说它的某个线程**阻塞在读取它的终端上**，或者终端**正关着回显读一行**
  —— 那是密码 prompt，由终端自己说出来，而内核看不到那里 —— 且它已经安静了
  `PROMPT_QUIET`（0.5 s）：既不需要 prompt，也不需要调用自己的任何输出，所以
  一个裸 `read x` 或一个 `cat` 也会让调用返回，prompt 出现之后才开始的等待
  同样如此（下文的*内核的话*与*要密码的 prompt 自己会说明*）；
- 它打印了东西，然后为 `PROMPT_QUIET`（0.5 s）安静下来，而终端**在等按键**
  —— 一个把光标留在行中间的 prompt（`>>> `、`Password: `、`[Y/n] `）、一个处在
  备用屏且终端已离开行模式的全屏程序（下文的*一个显示*不算这类），或者程序
  **逐键**读终端（菜单、编辑器、readline prompt），这正是 `waiting for input`
  所说的；
- 它**在画一个永不安静的屏幕**，并且自调用最后一次按键以来已经过了
  `SCREEN_BUSY`（2 s）—— 对一个之后才切过去、迟迟才画出的屏幕，则从它的第一帧
  算起（下文的*尚未画出的屏幕*）—— 那是一个吃按键的全屏程序，或者一个在终端
  逐键读取时用光标寻址重绘主屏的程序（`top`），重绘时钟、仪表或动画的频率快过
  每半秒一次。`watch -n 0.1` 曾把一次启动拖满它两分钟的 timeout，因为它始终在
  吃按键、从不安静；到那时屏幕上的东西就是答案（纯等待仍然继续等 —— 是模型
  要求看着它变 —— 而一条进度条在寻址屏幕的批处理命令，apt 的滚动区，不读按键，
  照旧被等到底）；
- 自调用的输入以来它已经沉默了 `LINE_QUIET`（2 s）—— 一条用整行作答然后安静
  下来的命令 —— 或者，在调用提交了密码之后、或在一个切过去还没画上的屏幕之上，
  沉默 `CHECK_QUIET`（10 s）直到程序回答它或画出来，前提是进程探测与进度显示
  检查都仍不指示有工作在跑；
- 调用的 `timeout` 到了。

有五种东西看起来像 prompt，其实不是（`SessionIo` 的 `awaiting_keys`）：

- **一个忙碌的程序。** 编译器跑着时留着的 `Compiling foo... `、`sleep` 之前的
  `Working... `、卡在进度条中段的下载：光标停在文字之后，和 prompt 上一样。
  当内核看到 session 的每个进程都在干活 —— 运行、睡眠、读管道、等子进程 ——
  那么无论屏幕显示什么，命令都是忙的，下面那些都不再问了；当它看到唯一的那些
  等待都不在终端上时也是如此 —— 事件循环的 socket 或定时器，一个 Go、Node 或
  asyncio 程序等网络时留着的 `Fetching... `（下文的*内核的话*）；
- **一个显示。** `gh run watch` 切到备用屏，每隔几秒重绘一次状态，之间就睡着，
  一个键都不读 —— 而安静的备用屏曾是最可靠的 prompt：一次对它做的等待在 3 秒后
  以 `waiting for input` 结束，模型信了，于是杀掉了一条正在按它要求干活的命令。
  每个在备用屏上吃按键的程序都会先把终端的**行模式**（canonical 模式）关掉 ——
  vim、less、nano、htop、btop、mc、whiptail、dialog、fzf、ranger、tig、ncdu、
  top、watch、各个 REPL，一个一个量过 —— 因为行模式下任何单个按键都到不了
  程序。所以备用屏只有在线模式关闭时才算作一个吃按键的屏幕（或者一个替身后面的
  程序把终端握在 raw 模式的 relay，或者模式还没读到）；留在行模式里的显示按主屏
  对待 —— 光标在那儿仍算 prompt —— 并像任何边干活边打印的命令一样被等到底
  （`IoState::drawing_screen`、`SessionIo::set_line_mode`）；
- **隐藏光标下的进度。** pacman 在同步软件包数据库时隐藏光标（`ESC[?25l`），
  之后又恢复（`ESC[?25h`）。它最初那个 0% 的条可以安静地摆着，光标位置就在文字
  之后，此时还没有任何重绘建立起动画。在 `sudo` 之下，探测看不到 root 的进程，
  那个位置过去会让 `bashsend` 在刚提交完密码之后就报告 `waiting for input`。
  单单隐藏光标证明不了什么：prompt 库在画问题和菜单时也会隐藏它，包括在屏幕是
  唯一证据的 relay 后面。只有当隐藏的光标跟在带进度条或传输提示的数字百分比
之后，或者程序往上跑、重写它实时输出末尾之上的旧输出时，那个位置才被否决。百分比
是在
  光标处检查的，所以留在它右边的旧进度文字遮不住一个新问题。菜单仅仅把光标移回
  自己的问题不算重写，菜单收起时被显式擦掉的那些行也不会把实时输出延伸到下一个
  问题之下；但一个新鲜的尾随换行仍会。同一项检查也阻止沉默结束一次启动或一次
  输入调用：它一直等到退出、另一个输入信号或调用的预算。内核观察到的读、密码
  prompt、逐键读取的程序，以及交互式备用屏，都保留各自独立的输入信号，可以结束
  那次等待；
- **一条动画中的行。** 在 `\r` 之后重绘的进度条、spinner、计数器、追加到
  `Downloading…` 后面的点 —— 每一个都把光标留在正是一个 prompt 会留的位置，
  并且随下载一起停顿时停。transcript 把输出切成**突发**（burst，`BURST_SPAN`，
  100 ms —— 一帧的各次写入落在同一个突发里），并按行统计那些改动了前一个突发
  留在它上面的文字（覆盖、擦除或延长）的突发数；重绘成同样的文字不算改动。自
  程序最后一次被输入以来被两个突发改动过的行是**动画中的**
  （`Transcript::cursor_line_animated`），它上面的光标不是 prompt，无论终端还
  说了什么。打字会重置这个计数 —— 菜单每按一键移动一次高亮，而那次重绘是对按键
  的回答，不是动画 —— 一条新行同样会：进度条之后打印出的问题立刻就是 prompt；
- **relay 的 raw 模式。** `sudo`（它把命令放在自己的终端里跑并做转接，自 sudo
  1.9.14 起是默认行为）、`ssh` 和 `docker run -it` 都会在自己的命令运行期间把
  session 的终端握在 raw 模式。逐键读取的程序会关掉 canonical 模式，但保留
  **输出处理**（`OPOST`）—— readline、Node、`prompt_toolkit`、`stty -icanon`、
  `read -n1` 都是如此 —— 而 relay 连那个也一并关掉，直接透传在远端已经处理过的
  输出。所以只有 `OPOST` 仍打开时，一个终端才算被逐键读取
  （`LineMode::reads_keys`）；在 relay 后面，程序只按它的屏幕来判断。

一次**没有 input** 的 `bash_session` 调用是一次*等待*，沉默规则不适用于它：
一个在行与行之间停顿的构建仍在干活，所以等待只在退出、prompt 或它自己的
timeout 时返回。正因如此，模型才能在一次调用里等一条长命令，而不是去轮询它。
等待对 prompt 的要求也更高：等待期间出现的 prompt 必须保持安静
`WAIT_PROMPT_QUIET`（3 s）而不是 0.5 —— 模型在等，是因为它认为命令正忙，而一条
忙碌命令在干活时留着的行（`Reading package lists... `）看起来就像一个提问。真正
的提问仍会在几秒内结束等待。

**尚未画出的屏幕**（`Screen::undrawn`、`Observation::undrawn`）。btop 1.4 切到
备用屏，然后探测它的 GPU 好几秒之后才画出任何东西，而一次把空白当成答案的启动
在 2 秒后就把那个空白返回了 —— 一个睡着的程序不是 prompt，但它是一个「从不停止
绘制」的全屏程序。一个切过去时上面什么都没有的屏幕，如今没有什么可回答的，也
不是一个正在被绘制的屏幕：它的沉默不会算作一条安静的行，最长到 `CHECK_QUIET`，
调用会带着第一帧返回。之后忙碌屏幕的看守从那一帧算起（`IoState::drawn_at`），
而不是从调用的最后一次按键算起：从按键算的话，一帧分四次写入的第一小块就让调用
安定了，模型只读到屏幕的四分之一。

**等待会把模型没看见的东西算进来**（`IoState::printed_since`），而不只是等待
期间到达的东西。一条安静干活、越过 `LINE_QUIET` 的命令把它的调用以 `Running`
返回，而它的问题常常在模型决定下一步做什么的那几秒里冒出来：等模型的等待开始时，
问题已经在屏幕上了，也不再打印任何东西。只盯着自己那点输出的等待，在探测看不到
程序在读的地方（`sudo apt install` 的 `[Y/n]`、一个在 `poll` 里等待的 REPL、
Linux 之外的任何东西）就会一直在那儿坐满整个 timeout —— 如果模型要的是十分钟，
那就是十分钟。所以 session 记住模型上次看的位置（`SessionIo::look`），而等待把
自那以后打印的一切都当作新的：模型在做决定时被问的问题，和它在等待时被问的问题
一样新，并按同一条 `WAIT_PROMPT_QUIET` 规则结束等待 —— 问题摆够那么久就立刻
结束。模型已经看见的不算：在一次上一次报告已经显示过的 prompt 上开始的等待，
是模型选择等待，那它就等。等待在安定了比这个调用更早的输出之前，仍会自己找
`PROMPT_QUIET`，这正是探测能发言的原因 —— monitor 在等待开始后几毫秒内就探测，
而它看到正在干活的进程树，无论最后一行长什么样都是忙的。

**session 处在什么状态，是在出报告时读的。** `waiting for input` 不只是调用
那次等待碰巧怎么结束的：一次什么都没看到新东西的轮询在它的 timeout 上结束，
可程序并没有因此不在它的 prompt 上，所以框架在组报告时从终端重算
（`SessionIo::waiting`），用与等待相同的规则 —— 看到新输出的纯等待要求
`WAIT_PROMPT_QUIET`，什么都没看到的那个则发现程序一直坐在原地。而一次什么都没
找到新东西的查看会点名终端仍停在那里的那一行 —— `(no new output — still at:
Full name:)` —— 于是在各次轮询之间失去线索的模型会被告知它正被问什么。安静是
自最新的输入或输出算起的，等待与探测里都是如此。有节奏的按键可能超出
`bashsend` 的预算：它的结果、`bashlist` 和一次零预算的 `bashwait` 不能在这些
按键仍在投递时把旧 prompt 误当成另一个问题。它们的最后一次投递会重新开始那段
安静区间。

**Node REPL 里提交的一行不是它的下一个 prompt。** 顶层 `await` 运行期间，Node
把它的终端留在逐键模式，而它的事件循环仍在盯着终端。这两个信号都不能证明它已经
处理完那一行。在 Linux 上，session 从终端前台进程组的命令行认出受支持的 Node
REPL 调用，然后把在 prompt 上提交的一个完整行扣住，等它的答复。分几次调用打
进去、
用 Enter 收尾的文字仍属于那一行；多行提交，或者从第零列开始的 `.editor` 行，
不会开始新的扣留。这一行自己的回显、Enter 的空白、终端控制序列，以及普通的以
换行结尾的日志，都让它保持待定。有一新行匹配记住的 prompt 前缀（包括自定义
prompt）、Node 的 `... ` / `| ` 续行，或者它的编辑器入口横幅之后跟着第零列，
就解除扣留。备用屏上的可见文字，或在绝对光标寻址之后的可见文字，同样算答复。
在那之前，光有 raw 模式不会让它变成一个新问题：`bashsend` 可以返回 `Running`，
`bashwait` 可以等那个回复。提前敲入（typeahead）或被 REPL 消费掉的一次 Ctrl+C
本身不解除这一行；探测看到的真实终端读取仍是权威的，而光标形状的嵌套提问仍
可以要求输入。

识别接受一组明确的 Node/V8 运行时选项，包括 `--max-old-space-size`、
`--max-semi-space-size`、`--initial-old-space-size` 和 `--stack-size`，其十进制
操作数用 `=` 紧接在后面。以空格分隔的数字操作数不合格。布尔选项 `--expose-gc`、
`--trace-gc`、`--jitless` 及其 `--no-…` 形式也支持。原生选项名里接受下划线；
它们的操作数保持原样。未知的选项、`--run` 和 `--test` 这样的应用模式，以及脚本
调用，仍然拒绝识别。求值需要显式的交互模式。对 `ts-node` 和 `tsx`，已知的包
入口路径和以前一样被直接识别。其他启动器脚本操作数会经符号链接解析到这样一个
入口点；相对别名属于运行进程的工作目录。单凭一个启动器名字不够。解析失败会拒绝
一个别名，但不会禁用对已知入口路径的直接识别。选项操作数、预加载名和被求值的
JavaScript 都不作为脚本路径解析。

这是对前台 REPL 的识别，不是对 JavaScript 工作的通用跟踪。无法识别的参数、
取不到的进程信息、把 REPL 藏起来的 relay，以及没有 Linux 进程信息的主机，都
沿用普通规则。停在行中间的输出仍可能像一个嵌套提问，而形状与某个已识别 prompt
完全一致的文字，可能在一个异步表达式还在跑时就解除扣留。反过来，一个没有可见
提问的异步 stdin 监听器可以一直 `Running`：同一个 Node 进程在等 stdin 时和在
一个永不 resolve 的 promise 上等待时，被量到的终端描述符、epoll 关注项和内核
等待状态完全相同。只要任意一行日志就能解除扣留，那在有日志的 await 期间就会把
假 prompt 带回来。一个可见的嵌套提问，或探测看到的真实终端读取，仍提供独立的
证据。

**按键是记法，不是转义码。** 模型在 JSON 里发出 `\u0003` 并不可靠，但对
Vim/tmux 的按键记法很熟，所以 `input` 认得 `<Enter>`、`<C-c>`、`<M-x>`、
`<F5>` 之类（大小写不敏感；`<lt>` 是字面的 `<`），以及按在任何键上的修饰键，
顺序和拼法都随意 —— `<C-Left>`、`<S-Up>`、`<M-F7>`、`<Ctrl+Shift+End>`、
`<M-Enter>` —— 按 xterm 的方式发送（`ESC [ 1 ; 5 D`，修饰键是按键自身序列的
一个参数；Alt 下的单字节键前面加一个 ESC）。在有这样的键之前，一个模型把
`<M-F7>` 写进了 `mc`，`mc` 的命令行就收到了那六个字符。尖括号里任何不是按键名
的东西都按原样打出，所以 `a<b` 和 `<div>` 都能存活。在 TTY 上，换行以 `\r` 发送
—— 终端为 Enter 发送的那个字节，cooked 模式会把它变回 `\n`，而 raw 模式的程序
（菜单、编辑器）正是这么期待的。方向键遵循程序的 cursor-key 模式（DECCKM 下是
`ESC O A`，否则是 `ESC [ A`），由模拟的屏幕读出。单独一个 `<Esc>` 之后会先短暂
停顿，然后再发剩下的输入，这样 Vim 就不会把 `<Esc>:` 读成 Alt+`:`。

**每个键都在程序读走上一个之后才发**（`pty::keys::encode`、`Pace`、
`background::type_keys`）。人的按键从不一起到达，而有些程序把一次读取返回到的
东西当成一次按键：`btop` 忽略了同时写进去的 `<Down><Down>` —— 六个字节，命名
不了它认得的任何键 —— 并且把打进它过滤框的 `bash` 整坨丢掉，而一个字母一个
字母地它就过滤了；`top` 把 `Mq` 当成什么键都不是，既不排序也不退出。所以每一个
具名按键都是自己的一次写入，而打到**逐键**读终端的程序（canonical 模式关闭的
菜单、`top`、行编辑器，或者一个全屏程序）的文字，只要不超过 `TYPED_TEXT_MAX`
（64）个字符，就一个字符一个字符地发。每次写入都等程序**读走**上一个：写者去问
终端的输入队列（`pty::spawn::InputQueue` —— 在从端上做 `FIONREAD`，在一次零超时
poll 之后，那次 poll 等 Linux 的行规程收下一个从 worker 线程交过去的写入；没有
它，计数几乎每次都会漏掉一个还在路上的键），最多等 `KEY_READ_WAIT`（1 s）一次
写入 —— 这么久什么都没读的程序不是在读，剩下的就按 typeahead 发，间隔
`KEY_PAUSE`（20 ms）。过去用一个固定的 20 ms 间隔来代表这次读取，于是每个键要花
50 ms 的程序只能收到十分之一的键。在队列无法替程序作答的地方，每次写入还额外等
`KEY_PAUSE`：一个 **relay**（sudo 自己的终端、ssh、`docker exec -it`、
`script`）在按键落地的那一刻就读走它、交给看不见的程序，只按队列来配速会把
`sudo btop` 的过滤词一口气发完 —— 一个字母都没到。relay 由它握着终端的模式来
认定，即关了输出处理的 raw（`LineMode::relays`），每发一个键之前重新问一次，
因为刚被打进 shell 的那条 `sudo` 可能在调用中途启动一个。粘贴、给按整行读取的
程序发的文字，以及更长的文字（打进编辑器的代码，任何吃下这么多内容的程序都把它
当输入而不是按键）仍是一次写入；字符永不被拆开，`\r\n` 仍是一个 Enter。打字要
多久取决于程序，所以调用记录下它最多可能花的时间（`typing_bound`），写者会在
最后一个键出去时说明（`SessionIo::typed` —— 是每个调用的键都出完时；若有调用
在它的键还在发时就结束了的话）：在那之前，调用除了退出或它的 timeout 之外不会
安定，并从那时起才计算程序的安静（`Observation::typing`）—— 一个回答了第一个键
就安静下来的菜单，还没有回答其余的。探测也要等最后一个键 —— 一个夹在两个键
之间的程序正阻塞着读下一个，这时去问内核，它会在还没拿到最后一个键时就说它在
等输入。

**代码是粘贴进去的，不是打进去的**（`pty::keys::encode`）。编辑器或 REPL 的
自动缩进会给换行之后打进去的每一行加上它自己的缩进，所以打进 Vim 的一个 Python
函数会变成一级级台阶 —— 每一行缩进的是它自己的空格*加上*上一行的 —— 文件于是
解析不了（*活模型教给设计的东西*）。人是靠粘贴把代码弄进去的，而能区分粘贴文字
与按键的程序会通过打开**括号粘贴模式**（bracketed-paste，2004）来说明这一点：
Vim、nano、bash 与 zsh 的行编辑器、Python 3.13 的 REPL、IPython。所以当程序开着
那个模式、而文字跑到了**缩进的**行上时，它就按终端投递粘贴的方式到达：第一行
照常打字 —— 这样文字前面的命令，Vim 的 `i`，仍是一条命令 —— 之后的各行作为
一次粘贴，夹在 `ESC [ 200 ~` 与 `ESC [ 201 ~` 之间，而结束整段文字的换行在粘贴
之后作为 Enter 键发出，这样一个 shell 或 REPL 仍会运行交给它的东西。被 `<Enter>`
键分开的行也算行 —— 线上是同一个字节，而模型写代码也是这么写的。没有缩进行的
文字走打字 —— Vim 的 `:%s/a/b/g` 然后 `:wq`、对 prompt 的回答、给 shell 的
命令，都是要生效的按键，而在 normal 模式下粘贴会把它们插进去。

粘贴只发给一个会**插入**它所收到内容的程序（`keys::Reader`）。shell 会把打给
它的每一行都执行掉，而一行启动一个程序的命令会把它之后的那些行留给那个程序去
读：`python3` 和一段给它的循环，在一次调用里发给一个交互式 bash，就作为一次粘贴
整个去了 bash —— python3 启动时没东西可读，而那段循环作为 shell 命令跑掉了。
所以 session 会指出终端的**前台程序** —— 它前台进程组最底下的那个进程
（`spawn::foreground_program`、`probe::group_program`，Linux 的 `/proc`）；光有
进程组的组长不够，因为 `sh -c 'vim x.py'` 把 vim 留在 shell 的组里，在那个不
exec 它的 shell 之下 —— 并对 shell 像人一样逐行打字。一个 relay（sudo、ssh、
`docker`），或者没有进程表可问时，可能把任何东西放在远端：只有处在备用屏上时，
文字才在那里作为粘贴发出，因为那里是编辑器负责插入的地方。

**终端发给程序的码不是按键**（`pty::keys::
strip_output_codes`）。一个模型用
`\u000f<Enter>\u001b[?25h` 在 nano 里保存文件 —— Ctrl+O、Enter，以及*显示
光标* —— 而 nano 把那个 ESC 当成按键，把 `25h` 打进了文件里。颜色
（`ESC [ … m`）、模式切换（`ESC [ ? 25 h`）和擦除（`ESC [ 2 J`、`ESC [ K`）
从不来自键盘，所以它们被排除在打出去的内容之外，而模型会被告知是哪几个、以及
按键该怎么写（`[Not typed: \e[?25h — …]`）。终端确实会发的那些通行：方向键、
功能键，以及鼠标报告 —— 后者的 `<` 把它们标记为输入。

**同一条字节流的两个视图。** 程序写出的每一个字节都喂给两个解析器
（`pty::screen` 与 `pty::transcript`）：

- 一个**屏幕** —— `vt100` crate 的模拟器，尺寸与 pty 一致，没有 scrollback
  （每个 session 约 150 KB）：一个人会看到的东西。它就是全屏程序返回的东西、
  ↓ 管理器展示的东西，也是输入所依赖的终端状态（cursor-key 模式、括号粘贴）
  所在之处。它还会**回答终端查询**：光标位置报告（`ESC [ 6 n`）、设备属性、
  颜色查询 —— `vim` 和 `prompt_toolkit` 的 REPL 这类程序会问，而有些会等很久，
  等一个任何管道都不会发来的答复。`vt100` 漏掉的东西在它前面补上，由一个前置
  处理用自己的解析器跟着这条流，把其余每一个字节原样交过去（下文的*一个全屏
  程序长什么样*）；
- 一份 **transcript** —— 这条流作为文本行，由我们自己的 `vte::Perform` 构建：
  回车和退格覆盖（一个进度条塌缩到它的最终状态），行内擦除与列移动生效，颜色
  和其余所有转义都消失。它就是普通程序返回的东西：模型拿到的是它上次看之后的
  输出，而不是其中 40 行的窗口。

一次查看投递的是自上次查看以来**每一行新的、或者文字变了的**行，按顺序，别的
什么都没有：每一行都（以哈希的方式）记住模型上次拿到的文字。模型回答过的 prompt
会带着它的回答回显回来 —— 先是 `>>> import math; print(math.pi)`，然后 `3.14…`，
然后是新的 `>>> ` —— 因为回显回答改动了那一行；在原地重绘的进度条回来一次，以
它此刻的样子，即使它位于光标所在行之上（pacman 会往上移动去重绘一个条），而它
周围那些已完成的、没变的条不会重复。于是这些报告以及由它们构建的 cell 都是
追加的 —— 每次调用加上新的东西，上下文从不两次带上同样一行没变的文字。全屏程序
的屏幕显示在主屏在它接管之前打印的那些东西之下（`git commit` 在编辑器之前的
提示），所以切换时什么都没丢。一次投递在超过 `SESSION_OUTPUT_MAX_BYTES`
（32 KB）时保留**尾部**，因为最新的那些行和 prompt 才是交互步骤要说的东西，
并说明它略去了多少行。

**一个全屏程序长什么样**（`pty::screen`）。用这些工具驱动 `htop`、`mc`、
`dialog`、`whiptail` 和 `nano`，暴露出屏幕视图错了三处，每一处都在字节进来的
地方修好，屏幕和 transcript 一视同仁 —— 而 `btop` 和一次对 tmux 的差分检查
（见下文）找出了其余的问题：

- **同字符的连串消失了。** ncurses 把一串重复的字符 —— 缩进、两列之间的空隙、
  一条横线 —— 写成字符加 REP（`CSI n b`，*把它重复 n 次*），而 `vt100` 没实现
  它：这一串变成一个字符，行的其余部分往左滑过来盖上。`htop` 在一行里读成
  `1 root 20`，下一行读成 `67 root        20`，两端还留着 `--m024 --m` 这样的
  残渣；`dialog` 的框只有三个字符宽；`nano` 的快捷键条在保存之后丢了空隙 ——
  而文件的缩进，一串和别的没什么两样的空格，也暴露在同一个问题下。前置处理把
  那个字符重新写一遍，要几次写几次，上限是一屏。它还实现**插入模式**
  （`CSI 4 h`），用模拟器自己的字符插入，照顾那些用它而不是 ICH 的程序。
- **框变成了字母。** 在没有 UTF-8 locale 的时候 —— 任何不设 `LANG` 的容器、CI
  任务或 Docker 镜像 —— ncurses 和 S-Lang 用 DEC Special Graphics 字符集
  （`ESC ( 0`，或者 G1 加 SO/SI）画线：字母 `lqqk`、`x  x`、`mqqj`。
  `whiptail` 读出来是 `lqqqu Fruit tqqqk`；`mc` 的面板是一列列 `x`。
  `pty::charset` 跟着这些指派走，把框显示成框 —— `┌──┤ Fruit ├──┐`、`│`、
  `└──┘`。
- **菜单的选择看不见。** 一个 curses 程序只靠颜色来显示哪一项被选中、哪个按钮
  有焦点：按下 `<Down>` 之后，`whiptail` 的三项读起来和之前一模一样，而 `htop`
  把光标停在它的功能键条上，于是连光标都不说 `F9` 会杀掉哪个进程。现在屏幕会在
  这些行下面点名它**高亮**的东西 —— `[Highlighted: line 17 "banana Banana"]`。
  一处高亮是一段同一个背景（或反显）的连串，它要与上一行**和**下一行都不同 ——
  按整段以多数来判定，所以一个压住表头开头的 tab 不会把表头截短 —— 而且它得
  点名某个东西：里面有一个字母或数字。菜单所在的框或底色不是高亮（它周围的行
  共享它）；相接的、或只隔着一点短空白的连串算一个；一行里超过三段彼此分开的
  就是图例 —— nano 的 `^G Help  ^O Write Out`、一根功能键条 —— 直接略去；最多
  点名八处。`htop` 点名它的表头和选中的进程，`mc` 点名选中的文件和活动面板，
  `dialog` 点名有焦点的项和按钮，`less` 点名它的搜索匹配，`nano` 点名它的标题
  和状态行。
- **换了名字的光标移动哪儿也没去。** `btop` 用 HVP（`CSI r ; c f`）放置每一个
  cell，而 `vt100` 不认识它：整个屏幕堆在了左上角。前置处理把 `vt100` 认识的
  移动用它们另外的名字写出来 —— HVP 写成 CUP，HPA（`` CSI n ` ``）写成 CHA，
  HPR（`CSI n a`）写成 CUF，VPR（`CSI n e`）写成 CUD，IND/NEL（`ESC D`、
  `ESC E`）写成一个换行和一个新行，SCOSC/SCORC（`CSI s`、`CSI u`）写成
  DECSC/DECRC —— 而模式 1047/1048，即备用屏和保存的光标，被一次一个地切换，
  分别当作 47 和 DECSC/DECRC。
- **关了自动换行的行照样换行了。** DECAWM 关闭时（`CSI ? 7 l`），一行到边距
  就停：跑过去的内容覆盖最后一列，光标留在那里。`vt100` 不管这些照样换行；
  `ranger` 就是这么写屏幕右下那一格的，于是一条比屏幕宽的状态行跑到了下一行。
  在正确字节取决于光标的地方，前置处理把自己到目前为止的内容交给模拟器，再问
  光标在哪（`Out::cursor`）：没有空位的窄字符占下最后一列，宽字符被丢弃（tmux
  的规则），光标则被设回最后一列，而不是留在那里等着换行。
- **tab 忽略了程序设的停靠点。** `vt100` 只每八列留一个 tab 停靠点，别的没有：
  HTS（`ESC H`）和 TBC（`CSI g`、`CSI 3 g`）什么都没做，CBT 和 CHT
  （`CSI n Z`、`CSI n I`）—— 往回和往前的 tab —— 也没有。前置处理在程序改动
  它们之后保留这些停靠点，并用同一个问法把每个 tab 移到它指定的那个。
- **窗口标题变成了文字。** `ESC k … ESC \` 给一个 screen/tmux 窗口命名；
  `ranger` 不管什么终端都发它，而 `vt100` 把标题打印在了光标处。它被丢弃，
  一直丢到结束它的 BEL 或 ESC。

前置处理自己的解析器也得知道自己在哪儿：它只在序列与序列之间把纯文本直传，而
结束一个链接或标题的 ST（`ESC \`）在它的 ESC 处就结束了字符串，此时 `\` 还没
到 —— 那个 `\` 被当成文本，于是解析器留在了转义序列里面，链接之后的 `│Disk`
（编译器和 systemd 会打印它们）变成了一个换行加 `isk`。

这些是靠一次**差分检查**（`scripts/pty_oracle.sh`、`examples/pty_oracle.rs`）
找出来的：一个程序在 session 的伪终端里带着一串按键运行，它写出的每一个字节都被
记录下来并通过 session 的模拟器显示，同样的字节再回放进一个 120×40 的 tmux
窗格 —— 一个成熟的模拟器 —— 然后对比两个屏幕和光标。在 `btop`、`htop`、`top`、
`nano`、`vim`、`nvim`、`less`、`mc`、`dialog`、`whiptail`、`fzf`、`ncdu`、
`tig`、`ranger`、`micro`、`nnn`、`w3m`、`vifm`、`calcurse`、`glances`、
`watch`、`man`、Python、Node 与 Ruby 的 REPL、`gdb` 以及 tmux 自己上面，两个
屏幕如今逐格一致。仍不一致的地方，要么是 tmux 偏离了 xterm（它忽略 CHT、HPR 和
VPR，除了 ASCII 不重复任何字符），要么那个序列只有终端测试套件才会发（DECALN
满屏的 `E`、DECSCA 的受保护字符）。

光标是按一个人会看到它的样子来点名的：写到最后一列之后，模拟器把它握在边缘之外
一格、等着换行，而屏幕却说 120 列里的 `column 121`。它的**那一行也被引出来**，
放在标题里，光标处标 `‸`（`Snapshot::cursor_text`，前面截到四十个字符、后面
二十个）：`cursor at line 38, column 31` 要求模型往下数三十八行，而它并不会数；
nano 的保存 prompt —— 文件名已经填好了 —— 曾让一个模型把名字又打了一遍，
`sysinfo.shsysinfo.sh`，然后在 `Save file under DIFFERENT NAME?` 上打转。
`— "File Name to Write: sysinfo.sh‸"` 说明名字已经在那儿了。一个**隐藏的**
光标（`ESC [ ? 25 l` —— htop、mc、whiptail）报告为 `cursor hidden`，而不是
程序把它停在哪儿。

**画在主屏上的屏幕仍是屏幕**（`IoState::screen_view_now`）。`top` 和 `dialog`
用绝对寻址在主屏上绘制，那些行跟不上，所以看到它的查看会展示屏幕；但 `dialog`
一旦画好，就只靠列移动和颜色来移动焦点，而下一次查看 —— 自上次以来没有寻址 ——
退回到了行：一次把焦点移到 Cancel 的 Tab 之后，得到的是 `(no new output — still
at: <  OK  >    <Cancel>)`。现在屏幕视图会粘住：绝对寻址打开它，原地编辑的输出
—— 一个退格、一次相对或列移动、插入或删除一个字符
（`Transcript::edited_in_place`）—— 保留它，而普通行的输出关掉它，于是 `clear`
之后的一条普通命令又按行来读。

**内核的话**（`pty::probe`，Linux）。屏幕只能给出一个形状；内核知道。
`/proc/PID/task/TID/syscall`
点名一个睡眠线程阻塞在哪个系统调用里，而对一次 read 来说，它的第一个参数就是
文件描述符，`/proc/PID/fd/N` 会把它解析成路径。于是 monitor 遍历 session 的
进程树（`/proc/…/children`），给每个线程分类：阻塞在**读 session 终端**的
`read` 上 —— 那个终端的 `/dev/pts/N`，或者 `ssh` 和 `git` 读密码用的
`/dev/tty` —— 就是**在读**。现代 GNU `cat` 也可能阻塞在 `splice` 里，把终端
输入拷进一个内部管道。探测检查它的输入描述符和等待通道
（`/proc/PID/task/TID/wchan`）：`wait_woken` 或 `n_tty_read` 标识出终端读取，
而 `pipe_wait_writable` 意味着输出管道满了、程序正在干活。隐藏的或无法识别的
等待通道仍是不确定的。输入不是终端的 splice 是干活，即使它的输出是终端。在
`poll`、`select` 或 epoll 里等待的线程会被问它在等什么；其余一切都算**在干活**。
`poll` 的 `pollfd` 数组和 `select` 的读集合就住在程序自己的内存里，地址由调用的
参数给出，而 `/proc/PID/task/TID/mem` 是在读系统调用那一行所用的同一项权限检查
之下（attach 模式的 `ptrace_may_access`）读它们的 —— 所以凡是探测看到一次等待
的地方，它就看到那等待是在等什么：在二十多个程序上、以 root 和以非特权用户量过，
从没有一次只有其一没有其二。每一个为输入而等待的描述符都经 `/proc/PID/fd/N`
解析：session 的终端，或者另一台终端的从端（`/dev/pts/M`，relay 在那里跑它的
程序），是**终端**；终端的主端（`/dev/ptmx`、`/dev/pts/ptmx`）是一个 relay 的
另一端；epoll 实例会被跟进去；别的任何东西 —— 管道、socket、eventfd —— 都是
**别处**。一次 **epoll** 等待通过它实例的关注列表
（`/proc/PID/fdinfo/N`）说同样的事，那个列表点名它盯的每一个文件 —— 按 inode
和文件系统，这在描述符号被关闭或复用时仍成立（在一个子进程与父进程共享的集合里
见过），从内核的设备编码转换成 `stat` 的；位于 session 终端所在 `devpts` 上的
文件是另一台终端，而用主端的名字就能打开的那个是主端。所以对终端的等待**可能**
是一个 prompt（一个 REPL、`vim`、`top`、bash 的 readline）；同时等终端和主端的
是一个 **relay**（`script`、`sudo` 自己的终端 —— 光一个主端是对它身后那个程序
的等待，和管道一样）；一个都不在终端上的在**别处**等；而探测读不到的
—— 一个已经不再映射的地址、一个之后被关掉的描述符、超过 `WAIT_LIST_MAX`
（1024）个描述符 —— 仍留作「可能」。每个 Go、Node、libuv、asyncio 和 tokio
程序就是这样闲着的：`gh run watch` 在两次重绘之间是六个线程停在 futex 里、一个
停在运行时自己 eventfd 上的 `epoll_pwait` 里。在没有描述符的 `poll` 或 `select`
是一次睡眠。而一次不可能有 prompt 在里面的等待，在一个有后代正在干活的进程里
就是**干活** —— 那个后代的 CPU 自上次探测以来增长了，从 `/proc/PID/stat` 读出，
由 monitor 在两次探测之间留着（`probe::Work`，以 pid 和启动时间为键，所以一个
被新进程占用的 pid 是新进程）：rustc 沉默地编译它最后一个 crate 时，`cargo
build` 在轮询 rustc 的 stdout 和 stderr，而模型给了十分钟的一次启动在第十秒就
回来了 `Running`；npm 的事件循环等 webpack 的管道、make 等它的 jobserver 管道，
也是同样的情形。relay 的等待也是干活，因为它把按键交给的那个程序有自己的终端、
它的 prompt 在那里被看到：`script` 罩着一次沉默的构建时能把它熬过去。对终端的
等待从来不算，无论子进程在做什么：bash 在它的 prompt 上、身后跑着一个后台任务，
一个 REPL 罩着它启动的子进程，一个 raw 按键读取器罩着它的 worker —— 把这些算作
干活，就会把它们的 prompt 一直扣到调用的整个 `wait`。终端的模式本来分不出这两
种情况：Python 3.13 的 REPL、crossterm 和 Go 的 `x/term` 都像 relay 一样关掉
输出处理，所以由描述符来定。上次探测没看到的进程是之后才启动的 —— 工作已经开始，
在它的第一个 tick 之前就被计入：构建末尾，rustc 轮询链接器的管道，而那个才几
毫秒大的链接器一个字节都还没用，如果只统计它已经用掉的，在那一次探测里就找不到
任何在干活的进程，于是一次沉默了十秒的构建在链接处被交回。但如果完全没有读到，
那就什么都没有新的。只有处在那干活进程*之上*的进程才算 —— 一个自己线程在计算的
程序，或者一个待在忙碌兄弟旁边的程序，都不等它们的东西 —— 而一个活着但闲着的
子进程证明不了什么。Node 的 REPL 在一个干活的子进程上 `await`，仍在那个内核区分
之外：它把终端留在自己的等待里，raw，和在它的 prompt 上完全一样。上面说的
提交行扣留，为已识别的前台 REPL 补充了探测。任何地方有一个读就是
`Probe::Reading`，无论子进程做什么；一棵全在干活的树是 `Probe::Idle`；一棵有
对终端等待的树，或者探测读不到的那种，是 `Probe::Polling`，而等待全在别处的
是 `Probe::Elsewhere` —— 它什么都不问（不问 prompt，不问密码），却也不像
`Idle` 那样在干活，因为一次网络等待会持续到服务器闲着为止，而它的沉默在
`LAUNCH_QUIET` 之后仍会结束一次启动：`Building... ` 罩着一个只会睡觉的子进程
留着，那时回来的是 `Running`，而一次它读不到的 poll 曾让它在半秒后就变成一个
提问。探测不得检视的进程，或者根本没有 `/proc`，是 `Probe::Unknown`，它把
决定权留给屏幕规则，`Polling` 也是如此。从描述符读到的答案按读到算，尽管在
查看它们的时候线程可能已经离开了等待：下一次探测会看到它去了哪儿。去怀疑一次
已经离开的等待 —— 探测曾经重读调用，因为它变了而退回*可能* —— 会让一个忙碌的
事件循环、每秒成百次带着新 timeout 进出等待，一次又一次变成可能的 prompt：在
`Downloading... ` 后面的一次沉默下载，九次运行里有三次被读成在等输入。这些文件
只对用户自己的进程可读，所以 `sudo` 和它跑的一切都让探测瞎掉：正是上面那条
relay 规则已经生效的地方。monitor 只探测一次调用正在等的那台终端，且要在它安静
`PROBE_QUIET`（0.2 s）之后，最多每 `PROBE_INTERVAL`（0.2 s）一次 —— 就那么几次
文件读取 —— 而任何输出或输入都会让结论立刻失效。monitor 在读进程树之前记下
session 的活动 epoch，只在那个 epoch 没变时才接受结果：读取期间排队或投递的输出
或输入，不会被它更早的观察盖掉。（移植自与之做过比较的那个备选设计，下文的
*与 terminal-session 设计的比较*。）

**要密码的 prompt 自己会说明**（`LineMode::hides_input`）。要密码的程序会把终端
的
回显关掉并读一整行 —— `sudo`、`ssh`、`git` 的凭据 prompt、`getpass`、
`read -s` —— 而那个模式属于终端，经它的主端读出，而探测恰恰需要程序自己的
`/proc` 文件。所以它正好在探测瞎掉的地方成立：`sudo` 是以 root 跑的。这份瞎曾经
让一个模型等了十分钟。密码错了之后，`sudo` 检查一遍，说 `Sorry, try again.`，
再问一次 —— 那是在打密码的那次调用于 `LINE_QUIET` 上返回之后几秒 —— 而模型的下
一次调用，一次等待，没看到属于自己的输出可以安定，于是坐满了整个 timeout。现在
一次关着回显的读，在终端安静 `PROMPT_QUIET`（0.5 s）之后就结束任何调用，和内核
的读一样，而框架会点名它：`Running (session …, waiting for a password — only
bashsend can type it)`。这个子句过去写作 `typed input is hidden`，它告诉了模型
为什么它的回答不会回显，却没告诉它谁能给这个回答：一个手里没有密码的模型曾让用户
"enter it in the terminal prompt"，而那个终端用户够不着，因为这个 session 是
agent 一个人的。现在它点名那唯一的入口，于是模型没被告知的密码，就是它要在聊天
里问的那个（`docs/tools/bash-tools.md`）；旧子句仍能被解析，所以录下来的 session
保得住它的状态行。两道守卫让这个信号保持诚实（`IoState::password_prompt`）。
一个回答 —— 结束这一行的按键、一个 Enter 或一个信号键
（`keys::reaches_line_reader`）—— 会到达一个仍在它模式里的程序，所以这个模式
只有在程序用输出作了答复之后才算数 —— 提交密码之后的可见文字，除非探测看到一次
终端读取 —— 这也正是 `sudo` 的重试成为一个新 prompt 的原因；没带 Enter 打进去的
文字根本没到达它，prompt 仍在，报告里会附上缺那个 Enter 的提示。而当探测看到整棵
树都在干活时，这个模式什么都不问：一个关掉回显以便在工作时吞掉 typeahead 的脚本
是忙的。monitor 在每块输出之后以及每次空闲轮询时读这个模式
（`refresh_line_mode`），所以 prompt 打印之后才切换的模式会在 20 ms 内被看到。
它只负责检测：打进去的东西就按打进去的样子显示（*与 terminal-session 设计的
比较*）。

**交出去的密码在同一次调用里得到回答。** sudo 一读到密码就断开这一行，然后花上
几秒校验它 —— pam 在拒绝时的延迟，网络登录还更长 —— 才吐一个字。沉默规则曾把
调用在校验中途以 `(no new output)` 返回，于是每次尝试都要模型再发一次调用才知道
判决，而一个把沉默当成「是」的模型会在 sudo 还在校验时打下一个回答 —— 那时回显
已经打开了，打进了接下来出现的随便哪个 prompt。所以一次其按键提交了一行
（`keys::submits_line`，一个 Enter）的调用，在密码 prompt 上会等程序的
**答复**：
两个屏幕中任一处的可见文字（`Transcript::answered` —— 单单一个换行、画不出东西
的空格和转义都不算答复）、一个新的 prompt，或者退出。拒绝回来的是
`Sorry, try again.` 罩着下一个密码 prompt；放行的密码则带着命令的头几个字回来。
待答守卫（`IoState::answer_pending`）由安定、屏幕启发式和密码模式检测共用：一个
换行或纯 ANSI 的输出，不得让回显仍为关闭、或旧 prompt 仍可见，被当成又要一个
密码。探测真正看到的终端读取仍是权威的，而一次可见地再次打印同样 prompt 文字的重
试仍算一个答复。沉默规则给它 `CHECK_QUIET`（10 s）而不是 `LINE_QUIET` —— 给任何
真实的校验留出余量，而一条被放行之后就沉默跑下去的命令仍会返回 `Running`，而不是
把调用扣到它的 timeout。
## 让运行中的 cell 流式更新

一次等待中的调用会把它将要报告的内容边构建边流出去，而 cell 像终端那样展示它：
执行器给出 `ToolProgress::Screen { settled, live }`，线上是
`StreamEvent::ToolScreen`，而 `App::push_tool_screen` —— `settled` 文字永久追
加，
`live` 行则**替换**上次发出的 `live` 行。一个进度条是一行在原地变化，绝不是每帧
一行；TUI 和模型的上下文都只读它一次。

对一次 session 来说，这个划分来自 transcript（`Transcript::take_stream`）：距末
尾
一个屏幕高度之内的行（`ROWS`，40）仍可能被重绘 —— pacman 会往上移动重绘一个条
—— 所以它们作为 `live` 流出；已经滚出可及范围的行是最终的，作为 `settled` 流出
一次。流出的是程序**自模型上次看以来画出的**每一行，以它此刻的样子 —— 它此刻正在
画的东西，绝不是模型已经拿到、而程序没再动过的那些行。这和下一次查看所携带的并不
完全一样：一次查看报告的是*变了的*东西，所以一行被画回它被交出去时的文字，会被
排除在外。过去流出完全照抄查看的行为，而 npm 的 spinner 就是它搞坏的东西。对一次
`npm install` 的 `bashwait`，它的启动已经把 `⠏` 这一帧交给了模型，于是每次整个
循环（`⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`，每 80 ms 一帧）转回到 `⠏` 时，那一行 spinner 就被丢掉，
于是每 800 ms 一次，cell 空掉、退回 `⎿ Running…` 一帧、并把那个框上下各挪一行。
等待最多每 `STREAM_INTERVAL`（50 ms）发一次，只在有东西变了时发，`settled` 每次
最多 `STREAM_MAX_BYTES`（64 KB），并在安定之时再发一次，于是 cell 结束的地方
正是报告开始的地方。cell 那个四行的窗口，结束在**仍在移动**的最低活跃行，而不是
输出的末尾 —— pacman 已经完成的条位于仍在下载的那些之下，全都在可及范围内 ——
见 `docs/ui/tool-streaming.md` 的*窗口跟着仍在移动的东西走*。

**一次更新是在两帧之间采样的，绝不在帧内**（`pty::pace`）。程序会分块重绘一行
—— npm 的 spinner 把一次列移动（`ESC [1G`）、一次擦除（`ESC [0K`）和那个字形
写成三次写入 —— 而读取方可以把它们拆开。一次在擦除之后采样的更新会显示空行，而
采样器*锁在了*那个时刻：唤醒它的那次读取是一帧的第一块，于是一帧又一帧，更新都
恰好落在那儿，spinner 几乎到不了屏幕上（喂给它十四帧、每帧的各块相隔一毫秒落地，
cell 收到的却是空行和一个字形）。现在一次到期的更新会等到输出静止
`FRAME_SETTLE`（10 ms）—— 一帧的各块相隔微秒落地，帧与帧之间是几十毫秒 ——
等待中的调用睡的时间不超过那段停顿还剩下的（`Pace::hold_left`），于是采样落进
停顿里，而不是落在下一次轮询上；从不暂停的输出，则在一次更新已经等了 `MAX_HOLD`
（100 ms）之后照样采样。管道上的一条普通命令按同一规则配速（`PipeOutput`），那里
的采样过去也同样落在擦除那一块上。

**普通命令以同样的方式折叠**（`pty::fold`）。很多程序在管道上也像在终端上一样画
—— curl 的计量表、tqdm、ffmpeg 和 rsync 不管有没有人看都用 `\r` 重绘，
`--color=always` 给词套上转义 —— 而保持原始的话，模型会读到一条进度条画过的每一
帧，拼在一行上。`Fold` 一次一行地重放这条流：`\r` 回到行首，退格和光标列转义
（`CSI G`/`C`/`D`）沿行移动，全都是覆盖；行内擦除生效；其他所有转义消失。和
transcript 不同，它是为模型可能复制回去的数据而构建的：tab 仍是 tab，尾随空格
仍在，一行保持完整到输出的上限而不是终端的宽度。一次普通 `bash` 调用把它的已结束
行作为 `settled`、仍在下笔的那一行作为 `live` 流出，配速与 session 一样
（`PipeOutput`、`PIPE_STREAM_INTERVAL`）；它的结果是折叠后的文本。后台 shell 的
事件流 —— ↓ 管理器的视图和模型读的完成提示 —— 由它的 monitor 折叠，而它的
`.output` 文件保留写出的每一个字节；一次 Ctrl+B 移交会把前台 runner 读到的内容
原样重放进同一个折叠里，于是进行中的那一行作为一行收尾。用户的 `!` 命令也折叠它
们的输出（`docs/input/shell-command.md`）。

## 活模型教给设计的东西

这个工具是拿真模型调出来的（`examples/session_probe.rs` 用真实的 system prompt
驱动真实的 agent 循环，并打印每一个面向模型的结果）：OpenRouter 的
`gpt-4.1-mini`、`qwen3-coder`、`gemini-3.6-flash`、`kimi-k2.6`、
`deepseek-v4-flash` 和 `glm-5.3-flash`，Ollama Cloud 的 `gpt-oss:120b`，以及
Venice 的 `qwen3-coder-480b`，跑六个场景：一个拒绝管道的注册向导、一个 raw 模式
的上下键菜单、一个 `getpass` 密码 prompt（请求里带密码和不带密码两种）、`vim`、
一个跨多个步骤保持打开的 `python3` REPL，以及一个在后台启动、用 `curl` 检查、
经 `bash_session` 结束的 web 服务器。第一轮里，注册向导在八个模型中的两个上跑通
了；经过下面这些改动，八个里有七个跑通了（第八个是会发出乱码工具名的模型），而
每个模型都找到了保险库的密码、在 `vim` 里改了文件（或用它非交互的 `-c`，那是更
好的工具），并结束了它的服务器。每一处改动都对应一个在线上看到的失败：

- **一个模型对「必须在交互式终端里运行」放弃了。** `bash` 的描述说，一条命令要求
  终端时就带上 `tty` 重试；而一次失败的普通 `bash` 调用，如果它的输出表明了这一
  点
  （`not a tty`、`inappropriate ioctl for device`、`/dev/tty`，等等 ——
  `tools::tty_hint`），就会把一段指向 `tty` 的指引只追加到**模型的**文本里
  （`ToolOutcome::context`）；cell 保留命令自己的原话。
- **`stop` 被读成「别等了」。** 一个模型在半途结束了向导。这个参数改叫 `kill`
  （`stop` 仍能解析），而它的 bullet 说明它只用于放弃一条命令。
- **最后一个回答带着 `kill` 发出。** 一个模型把它认为是最终回答的东西配上了
  `kill: true`；程序又问了一个问题，而那次 kill 把 session 扔掉了。现在带 `kill`
  的输入会先打字、等待，并且**不会杀掉一个用「继续问」作了回答的程序** —— 调用
  报告它仍在等待，而模型被告知为什么什么都没被杀
  （`pty::report::NOT_KILLED_NOTE`）。
  被输入结束掉的程序会报告它自己的退出。
- **转义了两次。** 一个模型在它的 JSON 里写了 `"Ada Lovelace\\n"` —— 打进去的是
  一个反斜杠加一个 `n`，没有 Enter。自身不含控制字符、并且**以转义字符结尾**的
  输入（`\n`、`\r`、`\t`、`\e`、`\x03`、`\u0003`）会被去掉一层转义
  （`pty::keys::parse_input`）；别处的反斜杠 —— `print("a\nb")`、`C:\\` —— 按原
  样
  打出。描述里的示例也把 Enter 写成 `<Enter>`，因为描述里出现一个 `\n` 正是那个
  模型照抄的东西。
- **打了但从未提交。** 一个模型打了它的回答却没按 Enter，看到它在屏幕上回显，就
  以为已经给出去了。当输入以打字文字结尾、而这一行仍开着时，模型会被告知它尚未
  提交、并且每个回答都应以 `<Enter>` 收尾（`UNSUBMITTED_NOTE`）。有两个信号说明
  这一行还开着：终端处在 canonical 模式（程序按整行读取 —— 用 `tcgetattr` 从 pty
  读出，`pty::spawn::line_mode`），或者 —— 对一个**逐键**读取的**行编辑器**，
  比如 Python 的 readline REPL，那里第一个信号是沉默的 —— 打进去的文字回显在
  光标所在行上、光标紧随其后（`SessionIo::holds_typed`；备用屏上从不如此，因为
  那里的编辑器会把它收到的内容插进去）。逐键读取的菜单已经拿到了按键，什么都不
  回显，也不会得到提示。在第二个信号之前，一个模型对一行它从未提交的文字，轮询了
  一个 REPL 三次「等输出冲出」；有它之后，它立刻就按了 Enter。
- **轮询一个正在等它的程序。** 程序停在 `Full name:` 上时，一个模型轮询了十次，
  每次报告都只说 `Running` —— 等待状态是在出报告时重算的，而什么都没新的时候，
  prompt 会被重新读给模型（见*session 处在什么状态*）。
- **一个从不说自己在等的菜单。** 上下键菜单把光标留在一条新行的开头，于是光标规
  则
  从不把它算作 prompt，每次启动都要等满 2 s 的行沉默回退。现在 monitor 随输出到
  达
  读取终端的行模式：逐键读取的程序在安静下来时就是在等待（一个把终端握在 raw 模
  式
  的 relay 不是 —— 见下面那次 pacman 运行）。
- **`sudo pacman -Syy` 返回了十一次。** 来自真实使用：打完 sudo 密码之后，一个
  模型用十分钟的 `timeout` 等下载，而每次等待都在一两秒内回来，框着
  `waiting for input`，每次都重复同一条已经完成的 `multilib … 100%` 条。两个原
  因。
  sudo 用 raw 终端做 relay，那被读成一个在等按键的程序，于是下载中任何停顿都安定
  成 prompt；而一次查看会把自光标所在行起的每一行重发一遍，于是 pacman 的那些条
  —— 光标停在仍在移动的那条上 —— 一次又一次地回来。现在 relay 的 raw 模式靠它的
  输出处理区分开来，动画中的行不是 prompt，纯等待给期间出现的 prompt 更长的时
  间，
  而一次查看只携带变了的那些行：同一次运行是一次等待，结束在仍在移动的那条上方的
  `Exit code: 0`（`llm::exec` 的
  `a_relayed_progress_display_is_waited_out_and_reported_once` 重放了它）。
- **一次把所有东西都发了。** 一个模型把四个回答放在一次 input 里发。这能行 ——
  终端会缓冲 typeahead —— 但回显落在每个 prompt 之前，transcript 读起来是乱的，
  而一个会冲刷自己输入的密码 prompt（`getpass` 就会）会丢掉提前打进去的东西。
  描述要求每次调用只回答一个 prompt。
- **打进 Vim 的代码变成了一级级台阶。** 被要求在 Vim 里写一个 Python 文件时，
  Ollama Cloud 的 `gpt-oss:120b` 和 Venice 的 `qwen3-coder-480b` 都按下 `i` 打了
  那个函数：Vim 的自动缩进把每一行的缩进加到了上一行之上，保存下来的文件抛
  `IndentationError`，两个都没脱身 —— 一个重打了三次然后退出，另一个一直重写到
  撞上 tool call 上限。谁也没想到 `:set paste`。现在缩进的文字在处于括号粘贴模式
  的
  程序里作为粘贴到达（*代码是粘贴进去的，不是打进去的*），同一个文件出来时和发出
  去
  时一样：之后 GLM-5.2 用六次调用写出了它，`qwen3-coder-480b` 也把它完成了。在后
  来的
  一次运行里，`gpt-oss:120b` 用 `<Enter>` 键而不是换行来分行 —— 同样的字节，同样
  的
  台阶 —— 所以那些对粘贴而言也计入换行。
  DeepSeek-V4-Pro 用它那种方式分行，然后用三次调用写出了文件，`gpt-oss:120b` 用
  了
  一次。
- **按键被当成 HTML 发出去。** 一次运行里，Venice 的 `qwen3-coder-480b` 把工具参
  数
  里的每一个 `<` 和 `>` 都做了 HTML 转义：`&lt;Esc&gt;` 作为十一个字母打进
  了 Vim，
  插入模式再没结束，而 `:wq` 被写进了文件里而不是保存 —— 两次，之后那个模型放弃
  了
  Vim 改用 `write`。同一个模型在其他运行里发出过真正的 `<Enter>`。含有被转义
  **按键名**（`&lt;Esc&gt;`、`&lt;Enter&gt;`）的输入现在按 HTML 转义读取
  （`pty::keys::html_escaped`）：整条去掉一层实体，并告知模型，因为它其他调用里
  的
  转义文字是原样打进去的。单单 `&lt;div&gt;` 按原样打字：HTML 正是一个往编辑器里
  打字的作者可能想表达的东西。
- **「显示光标」被打进了文件。** `gpt-oss:120b` 在 nano 里保存时发了
  `\u000f<Enter>\u001b[?25h`：保存成功了，而 `25h` 作为最后一行落进了文件，模型
  直到脚本跑不起来才发现。现在输出码会从输入里去掉并附上说明（*终端发给程序的码
  不是按键*）。
- **手动驱动它**（`examples/pty_drive.rs`，一串 tool call、循环里没有模型）找到
  了
  没有任何模型报告过的问题，因为没有模型看得出它是错的：屏幕视图缺失的 REP、画线
  和高亮（*一个全屏程序长什么样*），以及往 `top` 里打的一个键 —— `1`，用来看每
  CPU 视图 —— 得到的却是*打了但从未提交*，因为 `top` 的最后一行碰巧以 `1` 结尾。
  一个持有打进去文字的行编辑器会把光标就留在它后面，所以现在提示要的就是这个；
  而 `top` 把光标停在屏幕边缘。
- **btop，三个毛病。** 手动驱动时，`btop` 没有 UTF-8 locale 就不启动，接着把整个
  屏幕画在了左上角（HVP），再接着忽略 `<Down><Down>` 和一次写入打进去的过滤词 ——
  它把每次读取当成一次按键。加上 locale、那些别名和有节奏的按键（*每个键都在程序
  读走上一个之后才发*）之后，同一个脚本能移动选择并过滤。之后再驱动它的模型正确
  读了它的屏幕，却仍像对行 prompt 那样驱动它：`gpt-oss:120b` 发 `M<Enter>`、
  `s<Enter>` 和 `→<Enter>`（把箭头当成一个字符），直到调用用完，而 `kimi-k2.6`
  用 `<Left><Enter>` 排序 —— btop 的 Enter 是打开一个进程。`qwen3-coder-480b` 把
  mc 的按键条 `7Mkdir` 读成数字 `7`，mc 把它打进了命令行，然后它老实报告目录没
  建成。现在描述说明全屏程序是每个键立刻生效，而且按键条上的数字是功能键：之后
  `gpt-oss:120b` 按键裸发，`kimi-k2.6` 用几次 `<Right>` 按内存排了 btop、过滤了
  它并退出，还用四次调用做完了 `mc` 那个任务 —— `<F7>`、`made-in-mc<Enter>`、
  `<F10>`、`ls`。Venice 上那个 `qwen3-coder-480b`，也就是忽略下文未提交行提示的
  那个部署，仍在打 `7` 和 `10`。

第二轮实机运行，用 Venice 和 Ollama Cloud 的模型驱动 nano、top、htop 和各种菜单
式安装器 —— 还留了一个模型去查它在 `top` 里启动了什么东西 —— 找出了更多工具拒绝
或打错的失误：

- **数字写成浮点。** `grok-4.5` 把每个数字都写成浮点（`"timeout": 5000.0`），
  schema 的 `number` 允许它，而严格整数会拒绝 —— 在同一个错误上连着二十次调用。
  现在计数接受任何 JSON 数字，或者字符串里的一个数字，小数会取整；flag 接受
  `"true"`/`"false"`（`tools::lenient`）。别的一律带着原因拒绝。
- **从帮助条上抄来的键。** `glm-5.3` 读了 nano 的 `^O Write Out` 就发 `"^O"`，
  nano 把它当成两个字符插了进去。只由脱字符名字组成的输入 —— `^O`、`^X^C`、
  单独的 `^C` —— 就是那些键（`keys::parse_input`）：作为文本，它永远不是本意。
- **没有括号的按键名。** `qwen3-coder-480b` 用 `"F3"`、`"Esc"`、`"F10"` 驱动
  `htop` —— 打进去的是一个 `F` 和一个 `3`，搜索从未打开 —— 十九次之后它才断定
  那个进程不存在。对全屏程序来说，光一个按键名只可能意味着那个键，所以那里它作为
  一个键发出（`keys::bare_key`）—— 但只限没人会当单词打的那些名字：`F1`–`F12`、
  `Esc`、翻页键，以及 `Ctrl+`/`Alt+`/`Shift+` 组合。在 Ruby 里结束一个块
  的 `end`、
  一个 `return`、vim 的 `f3` 移动，在编辑器里仍是文本，而在行 prompt 上每个名字
  都按打字处理。
- **一个 Enter 写了两遍。** `qwen3-coder-480b` 把它的脚本打成
  `"#!/bin/bash<Enter>\necho …<Enter>\n"` —— 那个换行只是它 JSON 的排版，不是
  第二个 Enter —— 于是 nano 保存出来是双倍行距。当一次输入里的每个换行都在一个
  `<Enter>` 之后紧接着开启正文、且各一个时，它们就是排版、直接丢掉
  （`keys::parse_input`）；文字之后的换行是它自己的一行，所以
  `"def f():\n    return 1\n<Enter>"` 仍用一个空行结束一个 Python 块。
- **空的 `input` 被当成 Enter。** 一个模型发了 `"input": ""` 想接受 btop 的过滤
  词，
  读回一个和任何等待一样的报告，然后当成那个键已经进去了继续往下 —— 它接下来的
  按键进了过滤词而不是去了 btop。`""` 仍是什么都不打（等待就是等待），但模型会被
  这样告知，并且告知 Enter 是 `<Enter>`（`EMPTY_INPUT_NOTE`）。
- **一个 id 和 input 连在了一起。** 一次调用到达时是 `"session_id":
  "b…\nparameter=input>\n<Up>"` —— 模型的 `input` 在它的 tool call 模板与
  provider 的解析器之间某个地方漏进了 id —— 而回答是：没有这样的 session 在跑，
  同一份回复里却又把它列为在跑；模型信了前半句，重新启动了 btop。一个不是任何
  session、却以单词形式恰好含有某一个运行中 session 的 id 的 id，现在会以那个
  session 作答、说明它仍在运行、以及该如何调用它。什么都不打 —— 从乱码字段里猜
  出来的键可能是错的那些。
- **用 `&` 启动的进程。** 被告知在普通调用里用 `&` 启动的脚本已经停了 —— 调用在
  退出时会回收它的进程组 —— `kimi-k2-6` 和 `nemotron-3-super` 都改用
  `run_in_background` 重试并保留那个 `&`：任务的 shell 立刻退出，任务的回收又把
  脚本停了一次。现在后台启动会丢掉一个单独的尾随 `&`
  （`tools::without_trailing_ampersand` —— 绝不是 `&&`、转义的 `\&` 或 `>&`），
  而一次留下进程在跑的普通调用会说明它被停掉了，并指向不带 `&` 的
  `run_in_background`（`tools::REAPED_NOTE`、`subprocess::group_outlives`）。在
  Linux 上，信号 0 还会找到未被回收的僵尸，所以这个警告会检查 `/proc` 里活着的
  线程：只有一个已退出的辅助进程不算有什么工作被停掉，而主线程已退出、worker 仍
  存活则仍算。读不到的进程或任务状态保守地保留这个警告。无论如何，那个进程组都
  会被清理掉。

有些失败留在模型那边，工具该说的都已经说了。一个 REPL 要一个空行才结束一个
Python 块，而一个发了 `def` 却没带空行的模型会发现它的下一行被吞在 `...` prompt
里 —— 报告展示了那个 prompt，更强的模型会读它。一个较弱的部署（Venice 上的
`qwen3-coder-480b`）连着三次忽略未提交行的提示，把三个回答打进了一行，然后确认
了错误的账户；工具不会替模型按 Enter，因为 `input` 是按原样打进去的，而分两次
调用完成一行是合法的。同一个模型经 OpenRouter 则从一开始就用 `<Enter>` 回答每个
prompt。

有一个失败不是我们的：经 OpenRouter 的 `kimi-k2.6` 由好几个上游提供服务，其中
一个（Inceptron）会间歇性地把 `input` 参数从模型的调用里丢掉 —— 它自己的推理说
「让我打 Ada Lovelace」，而调用到达时只是一个裸等待 —— 于是一次 session 看起来
卡在它的 prompt 上，直到另一个上游来处理一个请求。在应用之外用一个原始请求复现过
（`provider.order` 对每个上游分别钉住）。

## 终端（`pty::spawn`）

这个 crate 禁止 `unsafe`，所以伪终端是用 `rustix` 的安全封装搭出来的（`pty`
feature：`openpt`、`grantpt`、`unlockpt`、`ptsname` —— Linux 用 `TIOCGPTPEER`
打开对端）。窗口尺寸在子进程启动之前用 `tcsetwinsize` 设好，主端是
close-on-exec 的，而我们对从端的那些副本，在子进程握有自己的那一刻就被丢弃，于是
最后一个进程放手时主端读到 `EIO`（流的末尾）。主端也是 session 读程序**行模式**
的地方（`line_mode` —— `ICANON`、`ECHO`、`OPOST`）：在 Linux 和 BSD
上这一对共享一条行规程，所以不必保留从端的句柄。

子进程必须把从端变成自己的**控制终端**，否则一个 `/dev/tty` 的 prompt（`sudo`、
`ssh`）会根本找不到终端。那需要在子进程里、`exec` 之前调用 `setsid(2)` +
`TIOCSCTTY` —— 也就是这个 crate 用不了的 `pre_exec` hook —— 所以由
`docs/tools/tty-detach.md` 为 detach 搭的那条同一套分级链来做
（`subprocess::tty_tiers`）：

1. **`setsid -c bash -c {command}`** —— util-linux 的（以及 BusyBox 的）`-c`/
   `--ctty` 在 `setsid` 之后把 stdin 变成控制终端；
2. **helper 重新 exec** —— `{exe} __zeroth-detached-tty-exec {command} [bash]`：
   `main()` 的第一条语句调用 `rustix::process::setsid()` 和
   `ioctl_tiocsctty(stdin)`，然后把它被指名的那个 shell 就地 `exec` 掉（macOS
   和精简镜像，那里没有 `setsid` 二进制）；
3. **没有第三级。** detach 链的最后手段 —— 一个附着着的 shell —— 会把密码 prompt
   交给*用户的*终端，那正是这条链存在要防止的 bug，所以在两级都不成的
   地方，命令改在管道上跑（`background::TtyLaunchError::NoTerminal`、
   `docs/tools/bash-tools.md`）。

shell 是 `bash` —— 绝对 `PATH` 条目里的第一个，每个进程解析一次
（`subprocess::tool_shell`）—— 只有在没装 `bash` 的地方才用 `sh`：`sh` 在
Debian、Ubuntu 和 Kali 上是 dash，那里的 `[[ … ]]`、`source` 和 `{1..3}` 都会
失败。

两级都保住每一次 kill 所依赖的那个不变量 —— `child.id()` 是 session leader、
它的进程组和它的 session。结束一个 session 会杀掉 session 里剩下的每一个进程，
然后再杀那个组，这样一个 shell 自己的后台任务也一并走掉。`pkill -s` 在 `pkill`
能点名 session 的地方找到 session 的进程；macOS 的 `pkill` 不能（Apple 编译它时
没带 `-s`），所以在那里改由 session 的终端来找：`ps -t` 把它们列出来（macOS 自己
的 `pkill -t` 至少到 macOS 26 上都匹配不到任何东西，尽管 `ps -t` 能列出那台终端
的进程）。一台终端只在 leader 活着的时候点名它的 session —— 所以先 session、后组
（`background::signal_session`），而 monitor 的 kill token 连 leader 也一并结
束，
只有在两者都完成之后才触发（`BackgroundRegistry::kill`）。

环境是父进程的加上 `TERM=xterm-256color`（模拟器实现的那台终端）以及
`PAGER=cat`/`GIT_PAGER=cat`/`MANPAGER=cat`/`SYSTEMD_PAGER=cat`，这样 `git log`
或 `man` 会打印而不是打开一个没人要求的 pager —— 模型*当作* pager 跑的程序
（`less file`）不受影响。描述 TUI 自己那台终端的变量（`TMUX`、`TERM_PROGRAM`、
`KITTY_WINDOW_ID`、`COLUMNS`，等等）会被移除，因为它们会把程序引向 session 的
模拟器没有的特性 —— `HERDR_PANE_ID` 也一并移除，因为它点名 TUI 所画在的那个
herdr pane：一个嵌套 agent 的 herdr 集成在它之下报告，会把自己变成那个 pane 的
owner，此后 herdr 会丢掉 zeroth 发的每一份报告，而 `herdr … --current` 会往
TUI 自己里打字（`docs/architecture/herdr.md`；每一个在管道上跑的子进程和 MCP
服务器也都不带它）。`HERDR_ENV` 和 herdr 的 socket 保留，所以模型仍能驱动 herdr
—— 它的 agent skill 先检查 `HERDR_ENV`。

**环境没点名时给一个 UTF-8 locale**（`spawn::utf8_locale`）。一个容器、一个 CI
任务或一个 Docker 镜像常常不设 `LANG`，于是 session 里的每个程序都跑在 C locale
下：ncurses 退回 DEC 画线字符集（反正读出来是一个框，见*一个全屏程序长什么
样*），打印 UTF-8 的程序打出问号，而 `btop` 根本不启动（`ERROR: No UTF-8 locale
detected!`）。模拟器不管主机怎么说都是 UTF-8，所以当决定字符集的那个变量 ——
`LC_ALL`，否则 `LC_CTYPE`，否则 `LANG`，即第一个被设的 —— 没有点名一个 UTF-8
locale 时，session 就设一个：`C.UTF-8`（macOS 上用 `en_US.UTF-8`，它没有
`C.UTF-8`），如果被设的是 `LC_ALL` 就设在 `LC_ALL` 里，因为它压过其他；否则设在
`LC_CTYPE`（什么都没设时则设在 `LANG`）—— 于是消息、日期和排序的语言保持用户
设定的样子。

## 注册表（`background.rs`）

一个 TTY session 就是一个**后台 shell** —— 那个已经拥有每个活得比它的 tool call
更久的进程的注册表（`docs/tools/background.md`）。这样免费得到 ↓ 管理器、页脚
计数、`/clear` 与退出时的 `kill_all`、subagent 归属和完成通知；注册表得到的则
是：

- **input** —— 每个 TTY 任务有一个由 channel 喂的写线程（程序不读时 pty 写入会
  阻塞，而任务状态被锁住时什么都不能阻塞）；monitor 的查询回复走同一个 channel；
- **两个视图** —— monitor 在同一把锁之下把每个 chunk 喂给 session 的屏幕和
  transcript（`pty::session::SessionIo`），刷新行模式，而一个等待中的调用在它
  旁边的 condvar 上阻塞；
- **announcement** —— 由前台调用启动的 TTY 任务静默注册，只有在它活得比调用更久
  时
  才被**宣告**（`BgEvent::Started`）。一条在自己的调用内部就跑完的命令从不出现在
  页脚或管理器里，从不发通知，除了它的 cell 之外不花任何代价 —— 也不计入 16 个
  session 的上限，那个上限数的是被宣告过的 session；
- **被观察到的退出** —— 当模型*看见*了退出（结果框着 `Exit code: N` 或
  `Stopped`）时，`Exited` 事件就说明这一点，不发完成通知：模型已经拿到结果，之后
  再来一条 `[background] … completed` 就是噪音，更糟的话，是一个自动的后续回合。
  谁报告退出是一个小小的握手（`SessionIo::finish`/`end_wait`）：一次调用在对
  session 动手*之前*登记为等待者，所以由它自己的输入或 kill 引起的退出归它报告
  —— 而一次没有调用看见的退出，在它的提示被读走之前一直可被认领，所以紧接着
  到达的调用也会报告它（`docs/tools/bash-tools.md` 的*每次退出一条通知*）；
- **给 UI 的屏幕** —— 对 TTY shell，monitor 发 `BgEvent::Screen`（节流到每秒
  十次）而不是行输出，而 ↓ 管理器的详情页展示渲染后的屏幕：它停在上面的那个
  prompt、菜单画出来的样子；
- **一个上限** —— `MAX_TTY_SESSIONS`（16）个运行中的 session；再多一个就会被拒，
  并附上运行中那些的列表，而不是让被遗忘的 REPL 在无人看见处堆起来；
- **一个没人看见的问题** —— 一个没有调用在等、并且停在自模型上次看以来它打印的
  某个 prompt 上的 session，在安静 `WAITING_NOTICE_QUIET`（3 s）之后发一次
  `BgEvent::Waiting`，此后不再发，直到模型去看：那个琥珀色的
  `● Background command "…" is waiting for input` cell，以及一条为 `bashsend`
  点名该 session 的提示（`docs/tools/bash-tools.md`）。只有形状像 prompt 的行才
  会
  为它去探测，所以一台闲着的服务器的日志永远不会。它的安静区间用与报告和探测相同
  的最新输入或输出时钟：排队的按键可以回答那个问题，而区间只在它们最终投递之后
  才开始。

一个 TTY session 的中间输出文件（`{tasks}/{id}.output`）装的是清理过的
transcript，而不是原始转义流，所以 `read` 它也是有用的。

## 权限

启动一条 TTY 命令和任何一次 `bash` 调用一样，问同样的问题。**往 session 里打字
也问** —— 否则批准一次 `bash` 就等于批准之后打进它里的每一条命令。这个请求是它
自己的一种 kind，`PermissionKind::Session`：标题 `Session input`，正文是一行上的
输入（`print(2 + 2)⏎`），它下面一行暗色的 `into python3 · session b7x2k9m1q`，
以及选项 2 `Yes, and don't ask again for this session` —— 在 gate 上为该 session
的生命期记住，绝不写进 `permissions.json`。有三件事让它在实践中保持安静：

- 一个其**启动命令**被某条常设 allowlist 规则覆盖的 session 继承那条规则 ——
  `python3 *` 已经允许了 `python3 -c "anything"`，所以往 `python3` 里打字并不
  增加它没有授予的东西；
- `master` 模式从不问，而**auto 模式的 classifier**像审查命令那样审查 session
  输入，并被告知这些按键去的那个程序（`prompts/classifier.md`：往一个 shell 里
  打 `rm -rf ~⏎` *就是*在运行它）；
- 一次**等待**、一次 **kill**，以及单独一个 **`<C-c>`** 从不问：它们只观察或结束
  已经被批准的东西。

生命周期 hook 像看任何其他工具一样看这些伴随调用；一个 matcher 可以用它的 wire
名（`bashsend`）或者它 cell 显示的名字（`BashSend`）点名一个 —— 而那个遗留工具
则叫 `bash_session` 或 `BashSession`。

## 各个 cell

- 一次 `bash` 调用是一个普通的 `● Bash(cmd)` cell。当它返回时仍在运行时，报告里
  的框架行 —— 模型的那一行 —— 被剥掉，程序的输出显示出来，而 cell 在一个新鲜的
  暗色角上收尾：`⎿ Waiting for input · session b7x2k9m1q`（或者在关着回显读取的
  prompt 上是 `Waiting for a password · …`，`Still running · …`，或者 kill 之后
  的 `Stopped · …`）。已退出的命令读起来和普通 `bash` cell 完全一样。
- 一次伴随调用是 `● BashSend(python3 ← import math⏎)` —— 先是 session 运行的那条
  命令（一行，截到 60 字符），然后是那一行上的输入（Enter 写成 `⏎`，按键写成
  `<Down>`）—— 或者 `● BashWait(python3)`、`● BashKill(python3)`，罩着同一个输出
  预览和状态行（`bashlist` 不点名任何 session：`● BashList()`）。这条命令从调用
  被
  宣告的那一刻就被点名：循环去问注册表这个 session 在跑什么
  （`summarize_call_naming`），也就是权限 prompt 为它那行
  `into {command} · session {id}` 所做的查询，所以那个 prompt 之上的横幅点名的是
  按键要去的程序，而一次被用户拒绝的调用 —— 它根本到不了执行器 —— 也保得住它。
  一个展示给人的 id 什么都不说。执行器把同一个横幅作为调用精炼后的标题发出
  （`ToolProgress::Title` → `StreamEvent::ToolTitle`）；只有一个无人应答的
  session 才用它的 id 点名（`bnope ← y⏎`）。模型额外被告知的东西（关于未提交行或
  未执行的 kill 的提示）只在它的上下文里；Ctrl+D 显示它。
- 两者都是命令 cell（`COMMAND_TOOL_NAMES`）：预览在 `TOOL_FOLD_ROWS` 处折叠，
  Ctrl+O 显示全部，而一次运行中的调用在它的时钟行之下实时跟随 transcript ——
  一次 `bashsend` 调用的时钟点名它自己的预算（`wait 10s`），一次 `bashwait` 调用
  的则点名它要求的等待，而不是 `bash` 的两分钟。
- 在一条运行中的 `bash` 启动上按 **Ctrl+B** 会把 session 交给后台（它继续跑，
  模型被告知它的 id）；在一次 `bashsend` 或 `bashwait` 调用上，它结束的是*等待*
  —— session 早就在后台了 —— 而模型被告知用户已经走开。cell 的提示说明是哪种：
  启动之下是 `(ctrl+b to run in background)`，等待之下是
  `(ctrl+b to stop waiting)`，而 `bashkill` 之下什么都没有，那个键到不了。
- ↓ 管理器的详情页展示一个 TTY shell 的当前屏幕。

## 离线 demo 与测试

dummy 的 `interactive` 场景（触发词：`interactive` —— 整个词，因为 `tty` 藏在
"pretty" 里、`repl` 藏在 "reply" 里）播一个安装向导：一次 `bash` 启动停在
`Project name:`，每次 `bashsend` 回答都被下一个问题接住，而最后一个回答结束
程序 —— 每回合一次调用，每个结果都是真实的 `pty::report::report`，所以离线的
cell 就是线上的那些。`scripts/smoke.sh` 的 Phase 124 在真实二进制里驱动它，并钉
住
只有屏幕才显示的东西：暗色的状态行、各个横幅，以及任何框架行或 `Exit code: 0`
都不会到达 cell。`cargo build` 之后跟着 `scripts/smoke.sh` 会跑每一个 phase，
覆盖 `docs/ops/smoke.md` 里描述的完整 TUI 集成覆盖。

纯的那几块 —— 按键记法、transcript、屏幕、安定策略、各个框架 —— 都有单元测试；
pty 的 spawn、注册表的 TTY 任务和执行器则对着真实进程测试（`sh`、`stty`，有
`python3` 时就加上它）；`tests/detached_exec.rs` 证明 helper 那一级给了 session
它自己的控制终端。屏幕和 session 的测试覆盖隐藏的进度光标、隐藏的菜单，以及旧行
被
擦掉之后的后续问题，还有程序接受输入的独立证据。`tests/bash_relay_prompts.rs`
通过一个真实的 `script` relay 演练那些转换，包括一段比输入安静截断更长的同步停
顿，
它必须在同一次 `bashsend` 内完成。探测的
`node_runtime_options_keep_the_repl_without_hiding_application_modes` 钉住选项的
拼写、操作数的处理和应用程序模式的拒绝；
`a_path_compiler_repl_is_resolved_in_its_process_working_directory` 通过真实的
进程元数据检查绝对和相对的启动器符号链接。执行器的
`a_node_repl_await_is_waited_for_not_taken_for_a_prompt` 检查一个真的 Node await
——它先记日志再作答，然后接受另一行。session 的测试还覆盖密码校验期间的换行、
空格和纯 ANSI 输出，回显开着和关着都测，还有一次可见地重复的重试和一次被探测到的
读。执行器的 `a_password_submission_waits_past_a_hidden_progress_cursor` 用一个
拒绝被检视的 Linux 进程来复现瞎掉的探测，然后一次密码读取和一段暂停的第一帧进
度，
无需 sudo 或改动软件包。`examples/session_probe.rs` 是那个实机 harness；
`examples/pty_drive.rs` 用一串调用、循环里没有模型地驱动同一批工具（`$S` 代表
上一份报告点名的那个 session），并打印模型会读到的东西 —— `nano`、`top`、
`htop`、`mc`、`dialog` 和 `whiptail` 就是这样检查的。

## 与 terminal-session 设计的比较

这个曾与为同一件事做的另一个设计做过比较 —— 一个 codex 风格的、在编号 session
之上跑的 `write_stdin`，命令本身默认在终端里跑（分支
`update/optimistic-planck-fiq8pr`）。它有三个想法更好，已经移植过来：把运行中的
cell 作为 settled 文字加 live 行流式发出（*让运行中的 cell 流式更新*）、点名
session 命令的横幅（*各个 cell*），以及 `/proc` 探测（*内核的话*）。普通命令折叠
它们的 `\r` 帧这一步走得比它更远：它是把每一次 `bash` 调用都放在终端里跑来获得
这个，而这里普通调用保留它的管道 —— 没有终端模拟，`isatty` 的答案不变，输出仍是
数据 —— 改在它上面折叠。有一个想法试过又放弃了：掩掉在关着回显读取的 prompt 上
打进去的内容。实际用起来，它掩掉了并非密码的普通输入，恰好藏住了横幅和权限
prompt 本来就要展示的东西（横幅在执行器跑起来之前没有终端可问，所以它默认掩
掉），
于是打进去的按键就按打进去的样子显示。它检测到的东西则留着，用来说不必藏任何东西
就能说的话：一次等待在密码 prompt 上结束，而框架会点名一个（*prompt自己会
说明*）。

这个设计有三个选择被保留而不采用它的做法：

- **变了的行，而不是一个前缀。** 它把每份报告与上一份按公共前缀比较，并把第一个
  差异之后的一切全部重发；一条在已完成行之上重绘的进度条（pacman）也因此重发了
  那些行。这里每一行都记住模型拿到过什么，所以只有新的或变了的行才发出去。
- **一台真终端。** 它给程序 `TERM=dumb`，那让程序变简单，却破坏了终端存在的意义
  —— `vim`、`less`、`top`、上下键菜单。这里程序拿到 `xterm-256color` 和一个真正
  的屏幕（`vt100`），连同查询回复等等。
- **`tty` 是要来的。** 普通调用留在它的管道上，所以 `git`、`ls` 以及每一个会为
  终端改变输出的工具都表现得像在脚本里，而模型只为需要终端的程序才去要终端。
  这一条后来被**推翻**了（`docs/tools/bash-tools.md`）：一个模型必须在命令运行
  之前就选好的模式，正是模型最容易搞错的 flag，而让终端成为那个设计默认值的那些
  东西 —— 一条已完成的命令按数据读回，以及不把沉默当成提问 —— 如今都在引擎里。

## 限制

- **只支持 Unix** —— 伪终端是 Unix 的设施；别处命令在管道上跑，和过去每一条命令
  一样（`docs/tools/bash-tools.md`）。
- **探测无法决定时的 prompt 启发式** —— 在 Linux 之外、对一个以另一个用户身份
  运行的进程（`sudo` 和它跑的东西），以及在对终端本身的等待之后（一个 REPL 的、
  一个 relay 的 —— 看得见，但从内核看一个提问和一次停顿长得一样），只有屏幕来
  决定，密码 prompt 除外（终端的模式点名那一个）。在那里，一个打印出它的问题、
  一个换行、然后在 canonical 模式里等待的程序，看起来就像输出行之间的一个 ——
  在备用屏上也是如此，那里的行模式程序按主屏来判：它会在 `LINE_QUIET` 上安定并
  报告 `Running` 而不是 `waiting for input`（输出里仍显示那个问题）；而一条留下
  一行开着的忙碌命令（`Reading package lists... `），在一次启动或一次输入调用
  那里可以在 0.5 s 安静之后被读成 prompt，在等待那里则是 3 s 之后。单单隐藏光标
  不会改变这一点：一个隐藏的提问和一条暂停的状态行可能看起来一模一样。一个真正
  的问题，如果重写了仍在实时的下方行之上更早的输出，或者看起来像百分比进度条，
  在没有独立输入信号可用时也可能匹配上那个显示守卫。那个守卫还会把安静的启动和
  输入调用扣到它们普通沉默截断之外。一个陈旧的进度显示可以扣住整个被请求的启动
  预算，或者 `bashsend` 的 10 秒；屏幕无法证明工作仍在发生。实机的 relay 测试
  仍漏掉 Clack 的 prompt 和 `prompts` 库里的初始 select，当它们的光标停在零列
  时；
  之后那些光标在输入行上的 `prompts` 问题则能被识别。
- **一个并不在等待的读者** —— 一个键监听线程坐在 `read` 里、而另一个线程在干活的
  程序，在探测看来就是在等待。
- **Node REPL 里一个沉默的异步读者** —— 一个提交了的表达式先日志了整行，然后在
  没有 prompt 的情况下等 stdin，或者用一个空的 `readline.question`，可能报告
  `Running`。一个 promise 仅仅挂着的时候，它的事件循环盯着同一台终端。可见的
  嵌套提问和内核观察到的读仍可检出；单凭换行日志无法安全地分辨这两种情况。
- **在输入之前挤掉 Node prompt 的输出** —— `node -i -e` 的一条异步 bootstrap
  日志、一条 `--trace-gc` 消息，或者初始 `> ` 之后的一条 TSX 警告，都可能在第一
  次 `bashsend` 之前把光标留在零列。提交行扣留要求一个可见的非空 prompt，所以它
  不在那里开始，之后的某次 await 仍可能看起来像一个输入请求，直到 REPL 重绘它的
  prompt。
- **cwd 变更之后相对编译器启动器别名** —— 别名是对着运行进程的当前 cwd 解析的。
  一个调用了 `process.chdir()` 的 `ts-node` 或 `tsx` REPL，可以让它的相对别名在
  下一次输入时解析不了。已知的编译器入口路径仍被直接识别，不依赖那次解析。
- **没有可读等待通道的 splice** —— 一次终端到管道的转移，可能在等输入，也可能在
  等管道容量。在内核把等待通道藏起来、或者点名了一个探测不认识的地方，这个区分
  仍是不确定的，由屏幕启发式来决定。
- **一条在密码之后沉默跑下去的命令** 会把提交它的那次调用扣住 `CHECK_QUIET`
  （10 s）而不是 `LINE_QUIET`（2 s），之后才返回 `Running`。
- **终端显示不出的密码 prompt —— 一个逐键读取、为每个键回显 `*` 的密码 prompt
  （`sudo` 的 `pwfeedback`）读作 `waiting for input`；一个在 relay 后面被问的
  （`ssh` 里的 `sudo`、一个 `docker run -it` 的 shell）坐在 relay 远端的终端上，
  由屏幕来判；一个在读之前什么都不打印的对探测来说就是一次普通的读；而一个以
  另一个用户身份运行、为了吞掉 typeahead 关掉回显然后安静下来的程序，读起来像是
  在要密码 —— 那个本会否决它的探测在那里是瞎的。
- **普通命令的折叠是一次一行** —— 行与行之间的光标移动（`docker pull` 用光标上移
  画的多行进度）在管道上什么都不是，会被丢掉；这类程序在输出不是终端时会打印普通
  的行。如今只有在没有终端可以给的命令才在管道上跑。
- **画在动画之上的提问** —— 一个替换 spinner 自身那一行的 prompt 继承了它的重绘
  计数，被读成动画中的：一次启动或输入调用会在 `LINE_QUIET` 上返回 `Running`，
  而一次等待会把它熬到 timeout。打印在新行上的一次立刻就是 prompt。
- **在 relay 后面** —— 在 `sudo`、`ssh` 或 `docker run -it` 之下，终端的 raw
  模式什么都不说，所以那里一个在等按键、光标停在行首的程序不会被识别为在等待
  （光标之前有 prompt 文字的那个可以，即使光标是隐藏的）。
- **relay 后面的显示** —— `ssh host gh run watch`，或者在一个做 relay 的 `sudo`
  之下的 `gh run watch`：relay 把终端握在 raw 模式，所以行模式对她身后的显示什么
  都不说，而 relay 自己对终端的那次等待（等按键传过去）是探测无法与 prompt 区分
  的。这样的显示在安静下来之后仍读作 `waiting for input`。
- **一台被盯着却从未被读取的终端** —— 一个启动时它的终端描述符已经是非阻塞的 Go
  程序，会把那台终端永久注册进它运行时的 epoll 实例里，读不读都一样，于是它的
  等待仍是可能落在终端上的那种，由屏幕来决定，和以前一样。session 打开终端时是
  阻塞的，所以这需要一个在同一 session 里更早、把它留成非阻塞的程序。
- **从不暂停的输出里的重绘** —— 运行中的 cell 是在帧与帧之间 10 ms 的停顿里采样
  的（`pty::pace`）；从不留下这样一段停顿的输出，画得比那更快的帧，或者一条盖过
  spinner 的日志洪水，会在一次更新已经为它等了 100 ms 之后采一次样，可能短暂地
  显示一帧画到一半的样子。
- **Typeahead** —— 一次输入里的好几个回答会像终端那样一起投递；一个在 prompt
  之前冲刷掉待处理输入的程序会丢掉它们。描述把模型引向每次调用一个回答，而不是
  逐行给输入配速。
- **relay 后面的按键间隔 20 ms** —— 输入队列只说明 relay 读到了一个键，不说明它
  远端的程序读了，所以那里每个键要慢过 `KEY_PAUSE` 的程序仍可能一次读取看到两
  个。
- **读者只在 Linux 上被点名** —— 别处没有进程表能说明一个 shell 是否在读终端，
  而缩进的文字只有在备用屏上才作为粘贴到达处于括号粘贴模式的程序，所以那里的
  IPython 或 Python 3.13 的 REPL 拿到的是打字进去的代码，并被自动缩进。在一次
  调用里先启动一个程序、再往它里打字的文字，在 shell 上逐行进行；第一行之后的
  那些作为打进去的按键发送，所以这样启动的一个 Python 3.13 REPL 会给一个它在
  单独一次调用里本会作为粘贴收下的块加上自动缩进。
- **高亮是一种启发式** —— 选择是靠它的背景与上下两行都不同来找到的，所以跨两行
  或更多行的一个（Vim 的可视选择）不会被点名，和旁边那行同色的一个也不会；只靠
  粗体或下划线显示的选择不是背景，也不会被点名。
- **粘贴是给缩进文字的** —— 处于括号粘贴模式的程序，只有在文字跑到缩进行上时才
  作为粘贴收到它，所以一个不缩进的块仍会撞上它的自动缩进 —— 一个智能缩进器
  （Vim 的 `cindent`）能给一行没带缩进的加上缩进 —— 而第一行是打字发的，所以在
  一个自动缩进的换行之后打进去时，它仍可能被缩进两次。在 normal 模式下发给 Vim
  的缩进行是被插入，而不是作为命令执行 —— 若走打字，它们会被当成一串 normal
  模式的命令执行。
