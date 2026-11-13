# herdr 支持 —— 窗格的状态，由 agent 自己报告

**在 [herdr](https://herdr.dev) 的窗格里运行 zeroth，侧边栏就会如实说出它：
一个回合在跑时是 working；一旦有权限 prompt 或问题在等你 —— 或者一个回合
已经失败 —— 就是 blocked；空闲时是 idle，回合结束后、在你去看之前是「已
完成」，而 herdr 重启之后又回到原处。** 没有什么要安装或配置：herdr 给窗格
里的每个进程设三个变量，应用一旦发现它们，就自己向 herdr 的本地 socket
报告。

herdr 是一个面向编码 agent 的终端复用器。它靠进程和屏幕认出自己自带规则的
那些 agent；对 zeroth 它没有规则，而第三方 agent 加不了屏幕规则（herdr 的
manifest 覆盖以它已知的 agent 为键）。它给出的替代方案是自我报告 —— herdr
的 *Add Herdr support* 页面 —— 而一个自我报告的 agent 是窗格唯一的权威：
不抓屏幕、不要 hooks 文件、不用 `herdr integration install`。纯的那一半与
socket worker 是 `src/herdr.rs`；边界是 `src/tui/herdr.rs`。下面的一切都
对照过 herdr 0.9.3 的源码（最新发行版，2026-09-29），不只是它的文档，而
决定行为的那几部分都在真实的 herdr 上跑过。

## 侧边栏显示什么，什么时候显示

状态是**从应用推导出来的，从不作为事件跟踪**：`herdr::activity(&App)` 在每个
循环底部、在这一轮迭代干完一切之后，读取 session 正在做什么，再由
`herdr::Tracker` 把它变成告诉 herdr 的东西 —— 先匹配者胜：

| herdr 状态 | 何时 |
|---|---|
| `blocked` | 有权限 prompt 或 `AskUserQuestion` 模态框打开着（`App::modal_open`）—— 无论是谁发起的，包括后台 subagent —— 或者启动时对 hooks 与项目 MCP 服务器的审查在等一个答复（[config-review.md](../state/config-review.md)）：在它得到答复之前，它问的东西一律不运行。 |
| `working` | 有回合在进行 —— 一次回复、一条 `!` 命令、一次 `/compact` 或 `/init`。 |
| `blocked` | 上一个回合因 backend 错误失败了 —— 直到有回合重新开始，或者对话被替换、被回退（`/clear`、`/resume`、一次回溯）。 |
| `working` | 有 subagent 在干活 —— 在运行，或者已经安定但为它排着一条消息或一次 Tab 后续。 |
| `idle` | 其他情况。 |

herdr 自己推导出第四个状态：**done** 是用户还没看过的 idle。从 working 或
blocked 走到 idle 是一次*完成* —— 窗格不在视野里时那声「finished」的 toast
与提示音 —— 而进入 blocked 是「needs attention」那一声。这两条通知塑造了
每一条规则：

- **串联的回合之间没有 idle。** 因为状态是在循环底部读的，那时回合结束已经
  分发了接下来的一切 —— 一条排队的消息、一个后台 notice 的后续回合、一次
  自动压缩 —— 所以一串回合只报出一个 `working` 和一个 `idle`，绝不会在中间
  报出假的「finished」（herdr 自己的报告者靠 250 ms 去抖、Pi 靠
  `agent_settled` 做的正是这件事）。
- **在干活的 subagent 就是活。** lead 的回合可以在 agent 还在忙时结束，而它们
  的结果会自己开启下一个回合；在那里报 idle 等于工作还在继续时宣布「finished」，
  真的结束时再宣布一次。一个已经安定但为它排着东西的 agent 也算 —— 它的循环
  会取起的消息，或者边界在下一个 tick 交出去的 Tab 后续（`herdr::agent_busy`）
  —— 否则它安定和下一次运行之间那一瞬就是它自己的「finished」。这一点刻意
  不同于 herdr 为自己新的 Grok 与 agy 支持所定的规则，那里把 prompt 处还在跑
  的任何东西都算作 idle —— 因为那里的后台 *monitor* 可以永远跑下去，把
  `herdr agent wait` 挂住一小时。subagent 总会结束；后台 **shell** 可能不会
  （dev server 会一直跑到被停掉），所以 shell 从不计入，这也是 herdr 自己为
  Claude 定的规则。
- **失败的回合保持 blocked。** 对一个死在限流、被拒绝的请求或过期 key 上、活
  只干了一半的回合，idle 就是一次完成 —— herdr 的「finished」。blocked 是
  herdr 的「needs attention」，而这正是事实：用户有个决定要做（重试、换模型、
  重新登录），消息说明了发生了什么 —— `Turn failed: HTTP 429: …`。这个保持
  由用户走过去而解除：任何回合的开始（一次重试、一条排队的消息、一次后续），
  或者它结束所在的那段对话离开屏幕 —— tracker 记下失败回合的历史世代
  （`App::history_generation`，`/clear`、`/resume` 或一次回溯会移动它），一旦
  它变了就永久放下这个保持。打开的模态框仍然压过它，它也压过旁边还在干活的
  subagent。只有 lead 的回合能保持：subagent 的失败是 lead 该去读的。

一条 blocked 报告用一行说明它在等什么 —— `Bash command: cargo test`、
`Create file: src/hello.py`（路径就是 prompt 自己的目标行所显示的样子）、
`Tool use: Deepwiki - ask_question`、`Question: Which database? (+2 more)`、
`Trust hooks: Stop, PreToolUse · bash`、`Enable MCP servers: github`、
`Turn failed: …` —— 每一段连续的空白和控制字符都压成一个空格，所以命令行里的
终端转义永远到不了 herdr，再在 80 个字符处截断。herdr 0.9.3 存下这条消息，
但哪里都不显示它；发它是因为文档这么要求，而且只花几个字节。

## session，以及重启之后回来

每条报告都带着把 session 带回来的那条命令：

```
zeroth --resume 18dbf0f64f371a36-4473
```

一旦对话有了 rollout 文件（第一条消息）就是这条；在那之前以及 `/clear` 之后
则是光秃秃的 `zeroth` —— 也就是这个窗格当初那个全新的 session。herdr 按窗格
保存最新的一条这样的命令，并在其服务器的**冷重启**之后
（`resume_agents_on_restore`，默认开），把它敲进恢复出来的窗格自己的目录里的
那个 shell。除非有一次 release，herdr 握着的命令不会被清掉，所以一条不带
命令的报告会把上一个 session 的留在那里：一次 `/clear` 之后重启 herdr，会把
用户刚清掉的对话又带回来。光秃秃的命令替换掉它。

第一个词是应用被调用时的名字（`argv[0]` 的 basename），并且只有当 `PATH` 上
有同名可执行文件时才给出，因为 herdr 就是在那个 shell 里运行它 —— 而 herdr
对带 resume 命令的窗格不重放 scrollback，所以一个 shell 找不到的名字会让窗格
同时丢掉它的历史和它的 session。按路径跑起来的开发构建则完全不上报命令。
herdr 会校验这条命令（`[A-Za-z0-9_.-]` 的第一个词不以 `-` 打头，任何位置都
没有单引号或控制字符，≤ 64 个词，≤ 8 KiB），并且**把无效的命令连同随它一起
送来的状态一起**拒掉，所以 `herdr::resume_argv` 先套用同样的规则：违反其中
一条的 session id 就不带上，宁可给出全新启动，也不丢掉整条报告。这条命令需要
herdr ≥ 0.9.2；更老的 herdr 会忽略这个不认识的键。

`/resume` 立刻报告新命令（报告变了，所以即使状态没变也要发），而一次
`--resume`/`--continue` 启动会在它第一条报告里就带上它。不发
`agent_session_id`：herdr 只从它自己的集成里保存这个。

## 线上协议

herdr 的 API socket 是位于 `$HERDR_SOCKET_PATH` 的 `AF_UNIX` 流式 socket，
讲的是**每条连接一个请求**：一行以 `\n` 结尾的紧凑 JSON，回来一行回复，然后
服务器关掉连接。永远不结束的那一行被丢弃，不予回答。

```json
{"id":"zeroth:1791290217491819","method":"pane.report_agent","params":{"pane_id":"w1:p3","source":"zeroth","agent":"zeroth","state":"working","seq":1791290217491819,"resume_argv":["zeroth","--resume","18dbf0f64f371a36-4473"]}}
{"id":"zeroth:1791290262462611","method":"pane.release_agent","params":{"pane_id":"w1:p3","source":"zeroth","agent":"zeroth","seq":1791290262462611}}
```

回复是 `{"id":…,"result":{"type":"ok"}}` 或 `{"id":…,"error":{"code":…,
"message":…}}`（`pane_not_found`、`invalid_resume_argv`、…）。`source` 和
`agent` 都是 `zeroth`：herdr 自己的 source 带 `herdr:` 前缀，而一个 herdr 认
得的 agent 名（或它的某个别名）会被改写成那个 agent 并按它的规则评判。应用不
认识的键就不写，绝不以 `null` 发送；herdr 忽略它不认识的键，所以更新的字段永
远弄不坏更老的 herdr。`herdr::send` 只写给这个用户自己拥有的 socket —— 检查
的是路径自己的条目，绝不穿过符号链接，因为被检查的东西必须就是被连接的东西
—— 因为一条报告可能带上 prompt 索要的那条命令。反正 herdr 的 socket 本来也
只属于它的主人。

**`seq` 就是全部的排序契约。** herdr 按窗格、按 source 保存它接受过的最高
`seq`，把任何不超过它的请求丢弃 —— 仍然回 `ok` —— 在这个窗格的**整个生命
周期**内都如此，跨越 agent 退出并在其中重新启动。所以 `herdr::next_seq` 跟着
以微秒计的时钟（herdr 自己的插件报告者所用的单位；它的 shell hooks 用纳秒，
这没关系，因为标记是按 source 的），时钟没往前走或倒退时就取上一个值加一；
重新启动之所以压过它之前的那个进程，是因为时钟走过去了，而单位绝不能变得更粗，
否则旧的标记会压过之后的每一个请求。id 是 `zeroth:{seq}`，天然唯一。

于是每个回合就是一串请求 —— `working`，也许 `blocked` 再 `working`，`idle`
—— 而一次退出是一个 `release`，它告诉 herdr 忘掉这个 agent、它的状态和它的
resume 命令，并重新读屏幕。release 是退出所做的最后一件事（`/quit`、空
composer 上的 Ctrl+C），在 `SessionEnd` hooks 与 MCP 服务器的拆除之后，所以
只要这些还有一样在跑，窗格就还是这个 agent 的 —— 而丢掉 reporter 也会发出它，
所以一个因错误而退出的循环仍然把窗格交还。它从不在 `/clear` 或 `/resume` 时
发出，那里下一条报告会指名新 session 来代替。一个没发它就死掉的进程 —— 被杀，
或者因为终端关闭而挂断（没有 handler：挂断的默认动作就是结束进程）—— 会在
窗格里的 shell 重新独自占住前台之后约一秒内被 herdr ≥ 0.9.2 发现。

## 绝不挡在 agent 的路上

herdr 自己的建议是「don't let Herdr slow your agent down」，代码是照着它写的：

- **循环从不碰 socket。** `Session::sync_herdr` 在每个循环底部运行（在记录器
  同步之后，所以第一条消息的报告已经指名它创建的那个文件），读
  `herdr::activity`，并把 `Tracker::update` 给出的那条报告投递出去 —— 只在
  状态、它的消息或 session 变了时：常见路径上只有一次比较，别的什么都没有。
  herdr 之外根本没有 reporter：没有线程，没有 I/O。
- **一个 worker，一次一个请求，只要最新的报告。** `herdr::Reporter` 拥有一个
  分离线程，排空单个槽位：worker 还没发出去的报告会被更新的替换掉，绝不排在
  它后面排队；请求一次只发一个 —— herdr 在自己的线程上服务每条连接，所以两个
  同时在飞可能被乱序应用。worker 拥有 `seq`，每次发送都重新打戳。
- **有界、安静地失败 —— 并修复。** 每个请求的写与回复都以 500 ms 为界；被拒绝
  或失败的发送在 1 s、2 s、4 s … 之后重试，直到 30 s（`Timing::resend_after`），
  而已经送达的在什么都不变时每 30 s 重发一次。herdr 持有自我报告的状态且不会
  过期，所以没有重发的话，一条因超时而丢掉的报告 —— 或者因 herdr 的在线升级
  （`herdr update --handoff`，它会把 herdr 自己六个集成的状态带过去，别人的
  一律丢掉）而丢掉的 —— 会一直错到下一次变化：一个空闲的 session，或者一个在
  等 prompt 的 session，会从侧边栏消失，直到它下次被用到。重发一个没变的状态
  不会打扰任何人（herdr 在抛出任何东西之前先比较状态）；herdr 不在时每半分钟
  付一次连接失败。
- **退出最多等 400 ms。** `release` 替换掉任何尚未发出的报告，之后不再报告任
  何东西，而退出最多等这么久让 worker 把它写出去，然后无论如何都退出进程。
  对着一个没人监听的 socket，连接立刻失败，退出根本不会被拖慢。

## 嵌套的 agent 留在窗格之外

herdr 窗格里的每个进程都会继承它的变量。一个由 session *运行*的 agent ——
`bash` 调用里的 `claude -p` 或 `codex exec`、一个向别的模型要评审的 hook、
一个自己就是 agent 的 MCP 服务器（`codex mcp-server`）、另一个 zeroth ——
会在这些变量下也向这个窗格报告，而 herdr 某个官方集成这么做不是一场关于状态
的争夺，而是它的终结：它的 session 报告会让那个 agent 成为窗格的 session 主人，
此后只要窗格还活着（包括重新启动），herdr 就会丢掉 zeroth 发来的每一条报告
—— 仍然回 `ok`（herdr 的 `current_session_owner_conflicts`；用 Claude 与
Codex 集成对真实 herdr 复现过）。第三方 source 抢不回窗格：herdr 只从自己的
集成里保存 session 身份。

所以 **session 启动的任何东西都不会拿到 `HERDR_PANE_ID`**：模型的终端 session
（`pty::spawn` 的 `FOREIGN_TERMINAL_VARS`，紧挨着 `TMUX_PANE` 和
`WEZTERM_PANE`，原因相同 —— 它指名的是 TUI 画在其中的那个窗格，不是 session
自己的终端），每一个走管道的子进程 —— `!` shell、各 hooks、一个没有终端可用的
`bash` 调用（`subprocess::command_in`）—— 以及 MCP stdio 服务器
（`StdioTransport`，那里服务器自己的配置若指名了这个变量，仍然以配置为准）。
每个 herdr 集成都需要一个窗格 id，所以它们全都站下，嵌套的 zeroth 也一样，
它的 `herdr::pane` 也需要一个。

为什么摘掉窗格 id 而不是 `HERDR_ENV`（每个集成也都要求它等于 `1`）？因为
herdr 自己的工具会读那一个：herdr 的 agent skill 开头就是
`test "${HERDR_ENV:-}" = 1`，并告诉 agent 在它失败时说它不在 herdr 里然后停下
来；而 herdr 看到它时会拒绝嵌套启动。没有窗格 id，herdr 的 CLI 在每个子进程里
仍然可用 —— `HERDR_ENV`、`HERDR_SOCKET_PATH` 和 `HERDR_BIN_PATH` 都留着，所以
模型可以列出窗格、切分一个、在那里启动一个 agent —— 它丢掉的是 `--current`，
即一个不带目标的命令转而发往获得焦点的窗格。这正是 herdr 给自己 popup 的环境，
也是除窗格占用者之外一个进程在 herdr 之下运行的另一个地方（它源码里的
`without_pane_identity`）；它同时也让 `herdr pane run --current` 不会敲进 TUI
自己。

## 窗格里的图片

与状态报告分开，也不需要任何设置：herdr 窗格把 zeroth 的图片（粘贴的截图、
`read` 读到的图片）画成 kitty graphics。herdr 的模拟器是 libghostty，它讲
kitty graphics 协议，也包括 unicode 占位符，但窗格的环境里一样都没提 ——
`TERM=xterm-256color`、`TERM_PROGRAM=herdr`，外层终端的 `KITTY_WINDOW_ID`
被剥掉 —— 所以只靠环境探测，在那里就落到半块字符上了。在环境什么都不说的
地方，zeroth 在启动时的光标查询里问终端，而 herdr 回答 `OK` 并自称
`libghostty`（`docs/ui/images.md` *Asking the terminal*）。herdr 的
`terminal.kitty_graphics` 设置自 0.9.0 起默认开着；关掉它，窗格的终端就没有
kitty graphics 可答，什么也不说，图片也就还是半块字符。这时 herdr 会在它自己
的外层终端上画那张图，所以一个没有 kitty graphics 的外层终端显示什么由 herdr
决定；在 herdr 配置里写 `terminal.kitty_graphics = false`，或者在窗格里设
`ZEROTH_IMAGE_PROTOCOL=halfblocks`，会让那里保持半块字符。

## 关掉它

`ZEROTH_HERDR=0`（`false`/`no`/`off`，常见的语法）为一次运行关掉它：不启动
reporter，什么都不发，herdr 把这个窗格当作一个不认识的程序。没有 `/settings`
里的一行 —— herdr 之外这个功能什么都不做，而在其中报告正是在那里运行的意义。

## 测试

`src/herdr.rs` 的单元测试把 `herdr::activity` 钉在真实的 `App` 状态上（一个
回合、一条 `!` 命令、一次压缩、每个模态框及其消息、一个在跑/已安定/挂着后续
的 subagent、历史世代），钉住 tracker 的状态机（排序、失败保持及其两种结束
方式、什么都不变时什么都不发、resume 命令跟随对话）、`HERDR_ENV` 恰好为 `1`
时的窗格探测与关闭开关、逐字段的请求形状、`seq`、重发时间表，以及 herdr 的
resume 命令规则。worker 对着真实的 Unix socket 测试：在递增的 `seq` 下按序
发出报告且 release 在最后、慢发送之后的一串突发收拢成最新那条、release 超过
一条在等的报告、一次丢弃触发一次 release、release 等待不超过被告知的时长、
投递在死掉的 herdr 上从不阻塞、一条被拒的报告在第一次重试内再发、一条已送达
的报告走在 keepalive 上 —— 每一项都做过变异检查。`send` 也对着真实 socket
测试：回复被读回、拒绝被暴露出来、一个沉默的服务器要花掉它的超时、缺失的
socket 立刻失败、指向活 socket 的符号链接被拒、普通文件绝不被写入。
`pty::spawn`、`subprocess` 和 MCP 传输各自钉住 `HERDR_PANE_ID` 的摘除。
`smoke.sh` Phase 132 驱动真实二进制对上一个 stub socket，它记录收到的内容：
启动时的 `idle` 以 `zeroth` 作为 resume 命令、一个回合的
`idle working blocked working idle` 与 prompt 的消息、从第一条消息起每条报告
都带 `zeroth --resume {id}`、严格递增的 `seq`、一条 `!` 命令只看到 herdr 的
标记与 socket 而看不到窗格 id、release 作为退出时的最后一个请求、一次
`--resume` 重新启动在它第一条报告里带上命令并压过旧的 release、
`ZEROTH_HERDR=0` 时什么都没有、死掉的 socket 让一个回合和一次退出都不付代价
—— 以及，通过真实 backend 对上一个拒绝每个请求的 stub provider，一个失败的
回合报告 `blocked` 带 `Turn failed: …`，并保持到一次 `/clear`，其报告用光
秃秃的 `zeroth` 回到 `idle`。fixture 会清掉每一个 `HERDR_*` 变量，所以从
herdr 窗格里跑的测试套件绝不会报告给真的那个。

## 限制

- **仅限 Unix。** herdr 的 Windows socket 是一个命名管道，这个集成不讲它；
  在那里不会启动 reporter。
- **herdr popup** 没有窗格 id，所以从它那里什么都不报告。
- **blocked 消息不被显示**，herdr 0.9.3 里如此（见上）。
- **`--current` 在模型的 shell 里失败**（见 *Nested agents*）：herdr 的 CLI
  在那里表现得就像在 herdr popup 里一样。
