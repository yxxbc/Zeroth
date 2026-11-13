# Headless 模式 — `alter-zero -p`

`alter-zero -p "prompt"` 在**没有屏幕**的情况下运行一个 prompt：没有 inline
viewport、没有 raw 模式、没有光标查询、没有按键读取，什么都不问。答复写到
stdout，任何出错的东西写到 stderr —— 加上 `--verbose` 时运行的每一步也写过去
—— 进程退出时带一个脚本可以据以分支的状态码。它是所有不是"终端前的人"的东西
的入口 —— cron 任务、CI 步骤、n8n 节点、基准测试 harness、流水线里的某一阶段。

```
$ alter-zero -p "Reply with exactly: OK"
OK
$ git diff origin/main | alter-zero -p "Review this diff; list real bugs only"
$ alter-zero -p --output-format json "Summarize the failing tests" | jq -r .result
$ alter-zero -p -c "Run the suite again and compare with last night"
$ alter-zero -p --permission-mode master --max-turns 40 -C ~/proj < task.md
```

## 它为什么一直缺席

agent 循环原先活在 `InlineViewport::init` 之后，而它会在其他任何东西运行之前
开启 raw 模式并询问终端光标在哪里（DSR）。stdin 是管道时立刻失败（`Device not
configured`）；是一个从不回应查询的 pty 时超时；是一个 `0x0` 的 pty 时渲染空帧。
每个调用方都得先造一个终端模拟器 —— 回应 `ESC[6n`、设置窗口尺寸 —— 然后才能
发一个 prompt。crossterm 的 `EventStream` 自身也有同样的毛病：构造一个就会立刻
打开控制终端，所以哪怕一个从不读按键的 session 也不能在没有 tty 的情况下存在。

交互式启动仍然需要那个终端，但它不再以错误的 Debug 形式死去
（`Error: Os { code: 6, … }`）：当 `InlineViewport::init` 失败时，`main` 会打印
原因并点出 print 模式（`cli::no_terminal_error`），然后退出 1。它从不自行切换到
`-p` —— 一次没人能批准任何东西的运行是调用方的选择。`init` 那时已经把它设置过
的模式回滚了。

```text
error: could not start the interface: No such device or address (os error 6)
It needs a terminal; to run without one (cron, CI, a script), use print mode:
alter-zero -p "your prompt"
```

## 参考实现的做法（调研结论）

- **Claude Code** —— `claude -p "query"`：同一个 session，打印出来而不是显示出
  来；`--output-format text|json|stream-json`；`--max-turns` 限制 agent 化的
  turn 数（模型回复数），结束时给出一个 `result`，其 `subtype` 为
  `error_max_turns`；`--permission-mode`；`-c`/`--resume` 继续一段对话；管道
  进来的 stdin 并入 prompt。`json` 结果对象（`type`、`subtype`、`is_error`、
  `result`、`session_id`、`num_turns`、`duration_ms`、`usage`、
  `permission_denials`）正是脚本们已经在解析的东西。
- **codex** —— `codex exec`（读了它的源码，`codex-rs/exec`）：stdout **只**携带
  最终消息（或者，带 `--json` 时是 JSONL 事件），其他一切都去 stderr —— 这条
  规则由它的 crate 以 `deny(clippy::print_stdout)` 强制执行。批准被强制为
  *never*：任何会发问的东西都被拒绝，并告知模型。JSONL 流是条目形状的 ——
  `thread.started`、带按进程计数的 `item_N` id 的 `item.started` /
  `item.completed`、`turn.completed {usage}`、`turn.failed {error}` —— 并且
  不携带增量：一条 agent 消息完整地出现一次。位置参数 prompt 配非 tty 的 stdin
  时，会把输入追加为 `{prompt}\n\n<stdin>\n{stdin}\n</stdin>`；没有位置参数时
  就把 stdin 当作 prompt 读取；`-` 显式点名 stdin。这个移植避开了它的两个陷阱：
  stdin 读取会一直阻塞到 EOF，所以**一个开着却永不关闭的管道 —— n8n 的命令
  节点、`docker run -i`、不带 `-n` 的 `ssh` —— 会让运行永远挂住**；而一个被
  打断的 turn 结束时根本没有终结性的 JSON 事件。

## 形状：同一个 session，不同的外壳

headless 运行的是 **TUI 所运行的那个同一个 `Session`** —— 同样的 bootstrap、
同样的 `start_turn`、同样的 stream 折叠、同样的 notice 落定与后续 turn、同样的
hooks、记录器、压缩与 subagent —— 只是跑在一个什么都不绘制的 viewport 上
（`InlineViewport::headless`，一个固定尺寸的 sink，其提交在入队时即被丢弃），
并且**完全没有 `EventStream`**。它替换掉的是循环里属于终端的那一半：
`tui::headless` 在回复、后台、subagent 与 MCP 通道外加进程信号上做 select，
并把 session 记录下来的东西变成输出。

另一种做法 —— 一个直接调用 `App` 和 backend 的精简 runner —— 会重新实现 TUI
的事件折叠：notice 在哪里落定、一个 agent 组在其成员事件排空后如何解析、一个
失败的 turn 记录什么、一次重试的尝试如何撤回。这些规则中的每一条都是有人已经
付过代价的 bug，而它们的第二份副本就是每条规则卷土重来的第二个地方。"CLI 路径
绝不会偏离键入的路径"（`docs/cli/cli.md`）正是 `submit_startup_prompt` 所依据
的规则；headless 以构造方式保住了它。代价是把渲染工作做进一个没人读的 sink
—— 提交在被绘制之前就被丢弃，Ctrl+O 的 transcript 也从不预热。

## 调用方式

```
alter-zero -p [OPTIONS] [PROMPT]
```

| Flag | Meaning |
| --- | --- |
| `-p`, `--print` | Run headless: answer, print, exit. |
| `--output-format <FORMAT>` | `text` (default), `json` or `stream-json`. Needs `-p`. |
| `--max-turns <N>` | Stop after **N model replies** of the lead agent, counted over the whole run. Needs `-p`. |
| `--verbose` | Log each step of the run to stderr. Needs `-p`. |
| `--permission-mode <MODE>` | `manual`, `edit`, `auto` or `master` for this run. Both modes. |
| `-C`, `--cwd <DIR>` | Run as if started in `DIR`. Both modes. |
| `-c`, `--continue` / `-r`, `--resume <ID>` | Send the prompt into that conversation (as without `-p`). |

语法是 `cli::parse` 的（`docs/cli/cli.md`）：`-p` 是一个开关，prompt 仍是那个
唯一的位置参数，所以 `alter-zero -p "…"` 读起来与 Claude Code 的一样。用法错误
以 `2` 退出并给出 clap 形状的尾部信息：`--output-format`、`--max-turns` 或
`--verbose` 没有配 `-p`、未知的格式或模式、`--max-turns 0`、某个 flag 给两次，
以及 `-p` 与裸 `--resume` 同给 —— 选择器是一个屏幕，所以 headless 运行必须点名
它的 session。

`--permission-mode` 和 `-C` 在 TUI 里也能用：前者就是 `ALTER_ZERO_PERMISSION_MODE`
本就提供的运行种子值（它优先于该变量和项目已保存的模式，且从不被保存）；后者是
在任何东西读取工作目录之前的一次 `chdir` —— 这也是为什么它先于 `--continue`
在*那里*寻找最新对话。

`--max-turns` 计的是 Claude Code 所计的东西 —— **模型回复**（lead agent 发出的
请求），而不是 alter-zero 的 turn（一个 turn 是一整轮交流，无论它的 tool 循环
取了多少次回复）。每个请求从一份预算里取一个，这份预算由 prompt 的 turn 和该次
运行开启的每一个后续 turn 共享；subagent 的请求和压缩的请求不计入。预算用尽时
下一个请求绝不发出：turn 以 `Stopped after N model replies — the run's
--max-turns limit` 结束，运行退出 `1`，结果的 `subtype` 是 `error_max_turns`。
库那一半是 `llm::agent::RoundBudget`，通过 `LlmBackend::with_round_budget` 挂上
（像各注册表一样，每次重建都重新挂上），并在 round 闭包里检查，那里
`RoundOutcome::Limit` 结束 turn 的方式与 tool-call 上限完全相同 —— 包括它的
`StopFailure` hook。

### prompt：参数、stdin，或两者

| Argument | stdin | Prompt |
| --- | --- | --- |
| given | a terminal | the argument |
| given | a file (`< task.md`) | the argument, then the file as a `<stdin>` block |
| given | a pipe (a shell's `\|`) | the argument, then the pipe as a `<stdin>` block, read to its end however slow its writer |
| given | a socket (what Node starts commands with — n8n) | the argument, then the socket as a `<stdin>` block — **if it says anything within 3 s** |
| none, or `-` | not a terminal | stdin, read to its end |
| `-` | a terminal | what is typed there, until Ctrl+D — `cat`'s rule |
| none | a terminal | usage error: nothing to answer |

`<stdin>` 块是 codex 的，因为被告知*什么*来自管道的模型会把 diff 当作 diff 来
读：`{prompt}\n\n<stdin>\n{input}\n</stdin>`。空白输入什么也不加。

**管道**绝不被截断。shell 的 `|` 在其写入方结束时结束，所以等它不会损失任何
东西 —— `./slow-tests.sh 2>&1 | alter-zero -p "summarize the failures"` 会把整份
日志送过去，无论测试先安静了多久 —— 而一个几秒后就把它截断的运行会拿光秃秃的
指令作答并以 0 退出，这是最糟糕的失败方式。代价是：如果父进程握着一个管道
开着却从不往里写（Python 的 `Popen(stdin=PIPE)` 从不关闭），运行会等它，并在
`STDIN_NOTE_AFTER`（10 秒）之后在 stderr 上说它仍在读取 stdin、以及
`< /dev/null` 可以停掉它。

**socket** —— 既不是终端、也不是文件、也不是管道的任何东西 —— 与一个参数同时
出现时则改用 3 秒的守卫（`STDIN_WAIT`，Claude Code 的规则）。Node 的
`spawn`/`exec` 会给它们启动的命令一个 socket 作为 stdin，而 n8n 的 Execute
Command 节点正是这样启动命令并且从不关闭它，否则会让每次运行永远挂住（codex
的挂起）：`poll` 最多等 3 秒让 socket 变为可读 —— 数据*或* EOF —— 沉默这么久的
socket 就不去管它，并在 stderr 上留一条说明。一旦它说了什么，就把它读到结尾。
普通文件从不等待；而没有参数时 stdin *就是* prompt，所以读取会如所要求的那样
一直等到它结束。

输入直接从描述符读取（绝不经过 std 的缓冲，好让 `poll` 看见后面还没来的一切），
最多 `STDIN_MAX_BYTES`（8 MiB —— 超出任何上下文窗口；错误信息会说把它存成文件
并改为点名那个文件）。它按 UTF-8 解码并丢弃 BOM；不是 UTF-8 的字节读作
`U+FFFD` 并在 stderr 上留一条说明：一份有一个杂散字节的日志仍然值得读。

## 运行做了什么

1. **在 runtime 之前**：`cli::parse`、`chdir`（`-C`）、`--continue`/`--resume`
   解析完成（快速失败，退出 1）、prompt 读取并合并。
2. **Bootstrap**，headless：与 TUI 相同的组装，减去屏幕。没有 `EventStream`；
   启动审查从不打开（见 *Trust*）；没有 telemetry ping、更新检查或 herdr 报告；
   没有任何东西进入 ↑ 输入历史；`--continue`/`--resume` 加载**不会把工作树恢复**
   到该对话的最后一个 checkpoint（见 *Resume*）。错误 toast —— 一个无法解析的
   `SKILL.md`、某个 provider 文件的问题 —— 写成 stderr 上的一行
   `alter-zero: …`；信息 toast 只在 `--verbose` 下写出。
3. **没有模型就不运行**：当 provider、模型和 key 都解析不出来时，TUI 回退到
   离线 demo。headless 运行则拒绝（退出 1，并点名 `/login` 和
   `ALTER_ZERO_PROVIDER`/`ALTER_ZERO_MODEL` 变量）—— 一个 CI 任务在罐头 demo
   答复上"成功"，是配置错误所能造成的最严重的失败。`ALTER_ZERO_DUMMY=1` 仍然
   强制使用 demo，测试正是这样离线运行的。
4. **MCP 服务器先安定下来**：TUI 在后台连接服务器，一个 turn 就直接在没有连上
   的那些的情况下运行；headless 运行几秒就结束，所以它会等到没有服务器处于
   `Pending`（每个都受其自身启动超时的约束）才发出第一个请求。
5. **prompt 被提交**，走与 `[PROMPT]` 快捷方式相同的路径 —— 先对 secrets 做
   脱敏，然后 `start_turn`。一个 `--continue` 续上的、已超出其窗口的对话会先
   压缩：即 TUI 的空闲边界自动压缩（`docs/architecture/compact.md`），在 prompt
   之前运行一次，之后只在运行等待其 subagent 时才运行 —— 绝不在最后一个答复
   之后，那里只会白花一个请求。
6. **循环运行到 session 空闲**：没有 turn 在进行、没有排队的东西、没有 subagent
   在跑。一个后台 agent 的完成会开启 TUI 会开启的那个后续 turn，所以"启动
   critic agent，写一份报告"确实会作报告。这个等待是有界的：一旦只剩后台 agent，
   运行最多等待 `ALTER_ZERO_HEADLESS_AGENT_WAIT_SECS`（默认 600，`0` 表示不等），
   然后停掉仍在工作的那些，在 stderr 上点名它们，并以它最后的回复结束 ——
   一个永不完成的 agent 不能永远占住一个 cron 任务。已经到手的 notice 绝不被
   截断：它的后续 turn 就要来了。后台 **shell** 不等 —— 用 `&` 启动的 dev
   server 永不退出 —— 所以那时仍在运行的会被停掉并在 stderr 上点名；system
   prompt 会把这一点告诉模型（见下）。
7. **关闭**是 TUI 的那一套：backend 被取消，shell 和 agent 被杀掉，`SessionEnd`
   被触发（原因 `other`），MCP 子进程被回收，rollout 被冲刷。然后打印结果并
   退出进程。

### 模型被告知了什么

system prompt 获得第四个块，`prompts/headless.md`，由
`backend::augment_with_headless` 追加在 scratchpad 之后：

```
## Headless

This run is non-interactive no one can answer questions or approve actions
A call that needs approval is refused work around it or report what you needed
The run ends when your turn ends background commands still running are stopped
Your final message is the run's output
```

`AskUserQuestion` 完全不提供（ask gate 从不挂上），所以模型无法等待一个不会
到来的答复。

### 权限：什么都不问

gate 是同一个，在某个 prompt 之前应答的每一样东西也是 —— 项目的允许清单
（`permissions.json`）、scratchpad 豁免、只读快速路径、`PermissionRequest`
hook、auto 模式的分类器。变化的是最后一步：headless 的 gate 是**无人值守的**
（`PermissionGate::set_unattended`），而 `approve_call` 本该抛出提示的地方改为
拒绝 —— 没有请求、没有等待、没有 `Notification` hook，因为没有人被展示任何
东西。模型读到的是 `permission::UNATTENDED_RESULT`：该调用未运行，没有人能批准
它，绕开它继续，或者说出需要什么。单元格显示 `Not run: needs approval
(headless run)`。subagent 共享这个 gate，所以它们的调用也以同样的方式被拒绝。

**失败的安全检查**被区别对待。当 auto 模式的分类器在得出裁决之前出错 ——
网络、限流、一个它读不了的回复 —— TUI 回退到发问；headless 运行做不到，但
一次失败不是一次拒绝，而告诉模型"不要重试"会让一次短暂的故障悄悄让运行丢掉
它的工作。所以模型读到的是 `permission::CHECK_FAILED_RESULT`：检查因错误而
失败，没有别人能批准该调用，它可以重试**这一个确切的调用一次**，如果检查再次
失败则不得再试。gate 会记住每个检查失败的调用
（`PermissionGate::first_check_failure`，以调用的种类和目标为键），而对同一个
调用的第二次失败回答 `CHECK_FAILED_AGAIN_RESULT` —— 又失败了，不要重试 ——
不再留下邀请，所以一个一直失败的检查无论模型怎么做都无法把它困在循环里。
单元格显示 `Not run: the safety check failed (headless run)` 和
`… failed again …`；每一次尝试在 `permission_denials` 里都是一次拒绝。

模式决定有多少东西能走到那一步。`master` 运行一切；`auto`（默认，除非项目
保存了别的）运行编辑、只读命令以及分类器允许的任何东西；`edit` 拒绝没人加进
允许清单的命令；`manual` 拒绝每一处改动。交互式保存的规则（"`cargo *` 不要再
问"）会被遵守，所以一个项目可以在 TUI 里准备一次，然后无人值守地运行。

每一次拒绝都被收集（`PermissionGate::take_unattended_refusals`）：结果的
`permission_denials` 列出它们，文本日志用本该允许它们的那个 flag 点名它们。
一次拒绝不会让运行失败 —— 模型被告知了并作了答；想让运行因某次拒绝而失败的
脚本去读那个列表。

### Trust

启动审查（`docs/state/config-review.md`）是一个屏幕，所以它从不打开：一个没有
人信任过的 hook 或项目 MCP 服务器**保持关闭**，与审查被拒绝之后完全一样，而
stderr 会说明有什么在等待、以及在该目录里交互式地运行 `alter-zero` 会询问它。
headless 运行从不自行信任任何东西。

### Resume

`-c` 和 `--resume <ID>` 像 TUI 那样加载对话、它的模型和它的 rollout，而 prompt
就是它的下一个 turn。有两处不同。工作树**不会**恢复到该对话的最后一个
checkpoint：在流水线里，树是流水线把它弄成的样子（一次 `git pull`、一次构建），
在 cron 任务下把它回退到昨晚的状态会悄悄撤销那些改动。而无可续之处仍是错误
（退出 1），与不加 `-p` 时一样 —— 脚本化的 `-c` 绝不能悄悄开一条新线索；cron
任务的第一次运行要省略 `-c`，或用 `--resume` 点名它的 session。

每次 headless 运行都像任何 session 一样记录它的 rollout，所以输出里的
`session_id` —— 或者 `--continue` —— 能再次接上它，无论 headless 还是 TUI。

## 输出

**stdout** 携带答复，别无他物（codex 的规则）：`text` 下是最终答复加一个换行；
`json` 下是一个结果对象；`stream-json` 下每行一个事件，结果在最后。

**stderr** 除非有需要说的东西否则是安静的 —— Claude Code 的姿态，也正是 cron
任务想要的，因为 cron 会把任务写出的任何东西邮寄出去。一次顺利的运行在那里
什么也不写。它总会说的：一次运行为什么失败（`alter-zero: HTTP 401: …`、
`interrupted by SIGTERM`、`--max-turns` 上限）、没人能批准的调用、它停掉的后台
工作、它等待过或放着不管的 stdin、它正在等过去的一次故障、为审查而暂扣的
hooks 和 MCP 服务器、一条错误 toast。**`--verbose`** 在每一种格式下都追加下面
这份运行的步骤日志：头部、每个 tool call 及其结果、叙述、一次重试、一条信息
toast、结尾的页脚。

**答复**是运行的最后一个回复：在本次运行新增的历史里，位于最后一件"非回复"
（一个 tool call、一个 notice、一个 hook 的附注、prompt）之后的那些 assistant
消息，以空行连接 —— 所以一个 Stop hook 的续写用自己的回复作答，而一个 tool
call 之前的叙述绝不会被误当作结果。

### `text`

```
$ alter-zero -p --verbose "Run the tests and fix what fails"
alter-zero 0.18.0 · gpt-5.5 · ~/proj · auto · session 18a9f2c33d41e5b6-1a2b   ← stderr
● Bash(cargo test)                                                           ← stderr
  ⎿ exit 101 · 240 lines
I'll fix the empty-input case first.                                         ← stderr
● Read(src/parse.rs)
  ⎿ read 120 lines
● Edit(src/parse.rs)
  ⎿ Updated src/parse.rs (+3 -1)
● Bash(cargo test)
  ⎿ exit 0 · 238 lines
Done in 1m 12s · 5 model replies · 48k tokens (31k cached)                   ← stderr
Fixed the empty-input panic in parse.rs; all 238 tests pass.                 ← stdout
```

不带 `--verbose` 时，同样的运行只把最后一行写到 stdout。日志是纯文本 —— 没有
颜色、没有光标移动 —— 所以在 CI 日志、cron 邮件和终端里读起来一样。运行从它
继续下去的回复属于叙述，进入日志；只有答复进入 stdout。

### `json`

```json
{"type":"result","subtype":"success","is_error":false,"result":"Fixed the …",
 "session_id":"18a9f2c33d41e5b6-1a2b","model":"gpt-5.5","num_turns":5,
 "duration_ms":72113,"usage":{"input_tokens":46100,"output_tokens":2100,
 "cached_input_tokens":31000,"cache_write_input_tokens":0},
 "permission_denials":[]}
```

`subtype` 是 `success`、`error_max_turns`、`error_during_execution`（turn 失败
—— provider、一个阻塞了 prompt 的 hook）或 `error_interrupted`（一个信号）；
`is_error` 除第一个之外都是真，`error` 在有消息时携带该消息。`num_turns` 是
预算计下的模型回复数。`usage` 是 provider 自己的记账，含 subagent。

每个 `permission_denials` 条目都是 Claude Code 的：模型调用它时的那个 tool、
该调用的 id 及其 input，再加上当发起者不是 lead 时的 subagent 类型 ——
`{"tool_name":"bash","tool_use_id":"call_7","tool_input":{"command":"rm -rf build"},"agent":"explore"}`。
gate 在拒绝该调用的那一刻就记录它（`permission::UnattendedRefusal`），所以
input 正是模型发出的东西 —— 它的占位符，绝不是某个 secret 的值。

一个**从未启动**的运行 —— 没有配置模型、无可续之处、未知的 `--resume` id、
读不了的 stdin、一个无法 bootstrap 的 session —— 在 `json` 和 `stream-json` 下
仍然打印一个结果（`error_during_execution`，`session_id` 和 `model` 为 null，
`num_turns` 为 0），所以一个管道接到 `jq .result` 的脚本遇到的是错误对象而不是
虚无。

### `stream-json`

codex 的条目流，由结果收尾：

```
{"type":"session.started","session_id":"…","model":"gpt-5.5","cwd":"/home/u/proj","permission_mode":"auto","resumed":false}
{"type":"item.completed","item":{"id":"item_0","type":"user_message","text":"Run the tests and fix what fails"}}
{"type":"item.started","item":{"id":"item_1","type":"tool_call","name":"bash","summary":"cargo test","arguments":{"command":"cargo test"},"status":"in_progress"}}
{"type":"item.completed","item":{"id":"item_1","type":"tool_call","name":"bash","summary":"cargo test","arguments":{"command":"cargo test"},"status":"failed","output":"Exit code: 101\n…"}}
{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"I'll fix the empty-input case first."}}
…
{"type":"turn.completed","duration_ms":71000,"usage":{"total_tokens":48200,"cached_input_tokens":31000,"cache_write_input_tokens":0}}
{"type":"result","subtype":"success",…}
```

- **条目**是运行记录下来的历史，按其顺序，id 为 `item_N`、按进程计数：
  `user_message`、`agent_message`、`reasoning`（仅当 thinking 显示开启时）、
  `tool_call`（每一个 tool —— `bash`、`write`、`mcp__…`、task 类的 tool ——
  带模型自己的 `arguments`、头部的 `summary`、**模型**读到的 `output`，以及
  一个批准 `note`；`status` 是 `completed`、`failed`、`declined`（它从未运行）
  或 `background`；它的 `name` 是模型所调用的那个 —— `bash`、
  `mcp__github__create_issue` —— 绝不是屏幕上的 `Bash` 标签）、`subagents`
  （一个已启动的组：每个 agent 的 id、类型、状态与结果）、`background_notice`
  （其 `shell_id`）、`agent_notice`（其 `agent_id`）、`compaction`、`hook` 和
  `notice`。没有条目自带一个名为 `id` 的字段：信封的 `id` 被扁平化地放在条目
  字段旁边，若再有第二个就是重复键 —— codex 的 `web_search` 条目恰好带着这个
  bug。
- 一个 tool call 在开始时**进入 started**，在解析时**进入 completed**，同一个
  id；其他一切都完整地出现一次 —— 没有增量，codex 的规则。
- **绝不打印 session 之后会收回的东西**：一次丢失连接所作废的重试回复会从历史
  中撤回（`docs/providers/offline.md`），所以超过 `App::settled_history_len`
  的条目 —— 那种重试仍可能撤回的东西 —— 会等这次尝试结束。
- 一个 turn 以 `turn.completed`（其摘要的时长与 token）或 `turn.failed` 结束；
  被中断的运行仍以一个 `result` 结束 —— 这正是 codex 缺失的终结事件。

## 退出状态

| Status | When |
| --- | --- |
| `0` | The run finished and its last turn completed. |
| `1` | The last turn failed (provider error, a blocked prompt), `--max-turns` ran out, no model is configured, the prompt could not be read, nothing to continue / unknown session — each said on stderr, and in the JSON formats as the result. |
| `2` | Usage error. |
| `130` / `143` | Interrupted by `SIGINT` / `SIGTERM`. |

信号是被处理的，而不只是被忍受：它取消 turn 并执行上面的关闭 —— shell 活在
自己的 session 里（`setsid`），MCP 服务器是子进程，所以在默认动作上死去会把
它们变成孤儿 —— 然后打印结果（`error_interrupted`，一个仍未关闭的 tool call
在其之前以 `failed` 完成）并以 `128 + signal` 退出。这些 handler 在 bootstrap
启动任何服务器之前就已装上，而关闭是有界的（`SessionEnd` 有它的两秒），所以
没有东西会活得比运行更久。

## 一次跑多个 job

`-p --jobs FILE` 把一个 JSONL 提示词文件当作相互独立的 session，在一个进程里
跑在有界的工作线程池上 —— 多线程内核的批量客户端（`docs/cli/batch.md`、
`docs/architecture/multithread-core.md`）。每个 job 就是这里描述的同一个
headless 运行；批量层加上准入（`--concurrency`）、逐 job 的输出捕获
（`--out-dir`），以及 stdout 上每个 job 一行 JSON 结果。

## 有意不在 headless 模式里的东西

- **没有 telemetry ping，也没有更新检查。** ping 的规则是它的 notice 在任何
  发送之前就展示出来（`TELEMETRY.md`）；没有屏幕的运行什么都不展示，而 CI
  机器正是回连最不受欢迎的地方。
- **没有输入历史。** 自动化的 prompt 不是用户键入的东西；TUI 里的 ↑ 不应走过
  一个 cron 任务的 prompt。
- **目前没有 `--output-schema`、`-o FILE`、`--ephemeral` 或 `--model`**：
  stdout 重定向已经涵盖了 `-o`，而 `ALTER_ZERO_PROVIDER`/`ALTER_ZERO_MODEL`
  已经为一次运行钉住了模型。

## 测试

- `cli`：每个 flag 的各种拼法、带 `=` 附着的值、仅 `-p` 的 flag（`--verbose`
  在其中）在没有 `-p` 时被拒、未知值会列出可选项、`--max-turns 0`、某个 flag
  给两次、`-p` 与裸选择器同给、help 页面被钉住。
- `headless`：各格式标签；按种类区分的 stdin 规则（管道读到结尾、与参数同给
  的 socket 在 3 秒处截断）以及有损解码；`combine_prompt` 的表；答复提取
  （叙述、续写、失败的 turn、续上的历史里更早的回复从不计入）；结果与退出
  状态；每个条目的映射与事件行、tool 按模型调用的名字命名；安静的默认行为与
  `--verbose` 在每种格式下的日志；toast 规则；Claude Code 字段里的一次拒绝；
  一个从未启动的运行；agent 等待以及它停掉的那些 agent。
- `permission` / `llm::approval`：无人值守的 gate 在本该发问的地方拒绝 ——
  在允许清单、scratchpad、只读快速路径、一个 hook 和分类器都发过言之后 ——
  不发请求，并以模型做出该调用时的样子记录这次拒绝。
- `stream`（离线 demo）：无人值守的 gate 拒绝 demo 中受门控的调用，lead 的和
  它 subagent 的，都不抛起请求。
- `llm::agent` / `llm::backend`：`RoundOutcome::Limit` 以错误结束 turn，没有
  `StreamDone`；预算跨 turn 计数，并在超过其上限时拒绝请求。
- `tests/round_budget.rs`：一个真实的 `LlmBackend` 对着一个回环替身 —— 预算
  为一次回复时绝不发出第二个请求、覆盖该 turn 的预算会把每次回复都计入、而
  预算横跨该次运行的每一个 turn。
- `tests/headless.rs`，真实二进制，**任何地方都没有终端**（每个流都是管道）：
  离线 demo 的答复在 stdout、stderr 上什么都没有，而 `--verbose` 的日志在那
  同一个 stdout 旁边，两个流上都没有转义；`json` 和 `stream-json` 逐行解析；
  来自 stdin 的 prompt、一个参数加一个 `<stdin>` 块、一个沉默超过 3 秒的管道
  被等待并使用其数据、一个开着的 socket 在等待之后被放着不管；无内容可答以及
  仅 `-p` 的 flag 没有 `-p` 作为用法错误；`-C`；`-c` 继续在它之前的那次运行；
  没有配置模型被拒，而无可续之处或未知 id 时仍给出 JSON 结果；运行途中收到
  SIGTERM 以 143 退出并给出 `error_interrupted` 结果。对着一个本地替身
  provider（几行 `std::net`），其模型请求一次 `bash` 调用：`manual` 拒绝它、
  告知模型，并以 Claude Code 的字段列出这次拒绝；`master` 运行它；
  `--max-turns 1` 在第二次回复之前停下；system prompt 携带 headless 块，而
  请求里没有 `askuserquestion`；一个后台 subagent 被等待，其完成答复了本次
  运行，而一个永不完成的在等待耗尽后被停掉；一个留着的运行中命令被停掉并被
  点名。
- `scripts/smoke.sh` Phase 153：从 tmux 窗格运行 `alter-zero -p --verbose` ——
  日志和答复在终端上，其模式不变（`stty -g`），原始字节流中没有任何 TUI 的
  转义（没有 DSR、没有备用屏、没有隐藏光标、没有括号粘贴、没有同步更新）；
  以及带 `--permission-mode master -C <dir>` 启动的 TUI 在其页脚点名这两者。
