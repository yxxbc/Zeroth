# smoke 套件 —— `scripts/smoke.sh`

`scripts/smoke.sh` 在一个真实终端（tmux）里驱动真实二进制，并**断言**它画出了
什么。它是终端 I/O 边界 —— `src/main.rs`、`src/tui/`、`term.rs` —— 唯一的自动
覆盖，所以它会轮询自己期望的状态，并在不匹配时以非零退出，从而能卡住一次提交或
一个 CI 任务。本页是这套件*结构*的地图；每个阶段检查什么写在阶段文件自己的头部
注释里，而 fixture 所声明的环境规则在 `docs/design.md`（“The smoke suite
defines its own environment”）。

## 布局

```
scripts/smoke.sh              the runner: discovery, scheduling, logs, summary
scripts/smoke/lib.sh          the fixture + vocabulary every phase sources
scripts/smoke/selftest.sh     the readers' and the selector's own tests
scripts/smoke/phases/         one file per phase: NNN-slug.sh
  001-first-turn.sh
  005-bottom.sh
  …
  107b-imagetransmit.sh
  115-telemetry.sh
  116-update.sh
  117-fastmode.sh
  …
  121-slowstream.sh
```

一个**阶段**就是一个自足的场景：一个文件，它 source 那个库，调用 `smoke_begin`，
通过自己的 tmux server 驱动 app，并一边走一边断言。runner 的职责只是找到这些
文件、跑它们 —— 在一池并行 worker 上 —— 然后报告。一个阶段的任何东西都不住在
它文件以外的地方，这正是这套件可扩展的原因：加覆盖就是加一个文件，去掉覆盖就是
删一个文件。

这套件过去是一个 12,000 行的单一脚本，把它的 115 个阶段一个接一个跑，所有阶段
共享一个 config home 和一个 tmux server，维护一个 130 行的清理函数、手工点名每个
session 和临时目录，并在一个 1,200 行的断言块里检查头 44 个阶段 —— 那个断言块
在它所读的驱动下方两千行处。拆分是机械的（每条断言都逐字保留下来）；结构性的
变化就是下面这些。

## 运行它

```bash
cargo build && scripts/smoke.sh          # everything, in parallel
scripts/smoke.sh -j 2                    # two workers
scripts/smoke.sh --serial                # one phase at a time, in phase order
scripts/smoke.sh 55 66 107b              # only these phases
scripts/smoke.sh 50-60 permission        # a range; a slug/title substring
scripts/smoke.sh --skip 115              # everything but
scripts/smoke.sh --failed                # re-run what failed last time
scripts/smoke.sh --list                  # phases, tags, last durations
scripts/smoke.sh -v                      # every phase's whole log
scripts/smoke.sh target/release/zeroth
bash scripts/smoke/phases/055-permission.sh   # ONE phase, output live
bash scripts/smoke/selftest.sh                # the readers + selector, no app, no tmux
scripts/preflight.sh --smoke                  # tmux, bash 4+, setsid — before reading a red run
```

`--list` 和每一次选择都走 `lib.sh` 里的阶段读取器（`smoke_phase_id`、
`smoke_phase_slug`、`smoke_phase_title`、`smoke_phase_tags`）以及建在它们之上的
选择器。它们正是这套件自己的弱点：一个返回空白的读取器不会失败，它选出错误的
阶段 —— `title` 和 `tags` 读取器里一处 BSD-`sed` 不兼容，让每个查询都只按文件名
slug 匹配，并把 `serial` 从那个唯一需要它的阶段上拿掉了。
`scripts/smoke/selftest.sh` 在一秒内就着 fixture 的头部把它们钉住。CI 在 GNU
`sed`（接受读取器旧写法的那一个）下先跑它再跑各阶段，然后又在一个 macOS 任务里
在 BSD `sed`（当初拒绝它的那一个）下跑一次。动了 `lib.sh` 或 `smoke.sh` 就跑它。

runner 在每阶段结束时打印一行 —— `ok`/`FAIL`、计数器、id、slug 和耗时 —— 失败
时再在那下面打出该阶段的 `FAIL:` 各行以及它日志的路径。每个阶段的完整日志
（该阶段抓到的每个 pane，放在一个 `==== …` 头之下，正是老脚本内联打印的内容）
落到 `target/smoke/logs/NNN-slug.log`；`target/smoke/times` 记住每个阶段的耗时，
好让下一次运行先排最长的，而 `target/smoke/failed` 供 `--failed` 使用。只有每个
阶段都通过时，退出状态才是 0。

单独跑一个阶段时，它把同样的日志写到终端并自己清理；`SMOKE_KEEP=1`（或 runner
上的 `-k`）保留它的临时树供事后检查。`SMOKE_STARTUP_MS`、`SMOKE_JOBS`、
`SMOKE_TIMEOUT`、`SMOKE_OUT` 和 `SMOKE_BIN` 是这些旋钮的环境变量拼法。

## 隔离：按阶段，而不是按套件

`smoke_begin` 在一个属于它自己的临时树里（`$SMOKE_TMP`，退出时一次 `rm -rf`）
给每个阶段：

- **它自己的 tmux server**，在自己的 socket 上 —— 阶段里每一处 `tmux …` 调用都
  是一个 shell 函数，路由到 `-S $SMOKE_TMP/tmux.sock`，所以两个阶段永远看不到
  彼此的 session 或粘贴缓冲，套件的 `set-clipboard on` 也永远到不了开发者自己的
  server；
- **它自己的 config home**（`$SMOKE_CFG`）、skills 根与 agents 根，照旧拼进
  `$CFG_ENV` / `$CFG_ENV_NOHIST` / `$APP` —— 所以某个阶段持久化的常驻权限规则、
  `/model` 选择或 `/settings` 值对其他每个阶段都不可见（老套件得在权限阶段之间
  手工 `rm -f` 那个共享的 `permissions.json`）；
- **它自己的 sessions 根**（`ZEROTH_SESSIONS_DIR=$SMOKE_TMP/sessions`），即使
  app 现在把 rollout 放在 config home 之下也照样拼出来：有很长一段时间，无论
  config 目录是什么，它们都回落到 `$HOME/.zeroth/sessions`，于是每次运行都在
  开发者自己的 `/resume` picker 里留下几百个 dummy backend 的会话。
  `scripts/smoke.sh --sweep-leaked` 列出老运行留在那儿的东西（那些离线 backend
  的模型名会把它们暴露出来）；`--yes` 删掉它们；
- **`$HOME` 也挪进树里**，所以被测二进制 —— 或者 tmux、git、阶段自己那段环境
  字符串说的任何东西 —— 没有任何路径能到达开发者的家目录：`~/.zeroth`、
  `~/.claude/skills`、`~/.tmux.conf` 和 `~/.gitconfig` 都解析到一个随阶段一起
  消失的空目录（checkpoint 存储自带 git 身份）。要测 HOME 相对行为的阶段自己设
  一个；
- **一个不读任何配置文件的 tmux server**（`-f /dev/null`）：开发者自己的
  `default-terminal` 或状态行设置会改变 app 画出的东西；
- **`$TMPDIR` 指进树里**，所以一个裸的 `mktemp -d` —— 以及 app 自己的
  scratchpad —— 都落在里面；
- 净化过的环境（`smoke_sanitize_env`：每个 `*_API_KEY`、每个 `ZEROTH_*`、
  `OLLAMA_HOST`、`NO_COLOR`、各个显示变量一律 unset；telemetry、更新检查、
  spinner tip 和项目的 `AGENTS.md` 导出为关；权限模式导出为 `manual`，因为一个
  session 从 auto 开始，而那里 dummy 的写入从不发问，可权限那些阶段讲的正是
  prompt —— Phase 64 把它 unset 以覆盖默认值；还有**Verbose output** 导出为开，
  因为默认情况下一次运行里的读、搜、列会折成一行折叠行，而各阶段断言的是逐 call
  的 cell —— Phase 146 和 147 把它 unset 以覆盖默认值，
  `docs/ui/tool-groups.md`），在阶段第一次启动之前跑，所以单独跑一个阶段与由套件
  跑一样密闭。

正是这种隔离让并行 runner *正确*而不只是快：各阶段本来就是独立会话上的独立场景，
但它们共享一个 server 和一个 config home，所以在每个阶段都自带一份之前，「同时
跑它们」不是一个安全的变换。

## 词汇表（`scripts/smoke/lib.sh`）

| helper | 它做什么 |
|---|---|
| `launch [-c DIR] [-w REGEX \| -n] SESSION COLS ROWS [CMD]` | 按要求尺寸 `tmux new-session`（CMD 默认 `$APP`），然后**等 composer prompt 而不是睡 0.4s 靠运气**。`-w` 等另一个模式，`-n` 完全不等（一个 shell pane、一次 `--help` 运行）。 |
| `submit SESSION TEXT` | 打出文本，停 `SMOKE_TYPE_SETTLE`（0.2s —— 紧跟在按键爆发之后的 Enter 会被读成粘贴的一部分），按 Enter |
| `keys SESSION KEY…` / `type_text SESSION TEXT` | 用键名 / 字面文本 `send-keys` |
| `pane SESSION [capture opts]` | `capture-pane -p`；`-S -N` 够到 scrollback，`-e` 保留 SGR 转义 |
| `wait_for SECS SESSION [capture opts --] [grep opts] PATTERN` | 轮询直到 pane 匹配；超时返回 1 |
| `wait_pane SECS SESSION … PATTERN` | 同样的等待，但**打印最后一次抓取**，好让阶段能对满足它的那一帧断言 —— 并记录它 |
| `wait_settled SECS SESSION … PATTERN` | 匹配**且**相隔 0.2s 的两次取样相同：回复之后已落定的布局，而不是流式中途的一帧 |
| `wait_summaries SECS SESSION COUNT [capture opts]` / `count_summaries CONTENT` | “第 COUNT 个回合已落定”：屏幕上已提交的回合摘要，**不管它们穿着哪个动词**（`SUMMARY_RE`，见下） |
| `poll SECS COMMAND ARGS…` | 复合条件用的通用形式 |
| `wait_file` / `wait_gone` | 一个匹配的文件；一个 app 已退出的 session |
| `has CONTENT … PATTERN` / `lacks` / `pane_has SESSION …` | 那些等待所依据的谓词 |
| `expect_has CONTENT [grep opts] PATTERN MESSAGE` / `expect_lacks` / `expect_eq` / `expect_ne` / `expect_file_has` | 每条一个断言：没命中就 `fail MESSAGE` |
| `fail MESSAGE` | 记录一次失败的检查 —— stderr 上的 `FAIL: Phase N — MESSAGE` —— 然后继续走：一个阶段是一个场景，而后面的检查通常更能说明坏在哪里 |
| `note TITLE` / `dump TITLE CONTENT` | 日志里一个 `==== Phase N: TITLE ====` 段标记（罩在一个抓到的 pane 之上） |
| `work_dir [NAME]` | 阶段那个隔离 HOME 下的一个短工作目录，好让页脚把 cwd 渲染成 `~/{NAME}`。断言页脚**尾部**（`· N shells` 计数、上下文量表）的阶段必须在其中启动，并用 `$APP_ABS`/`$BIN_ABS` |
| `smoke_on_exit CMD…` | 在收尾时运行 —— 一个要杀掉的桩服务器 |
| `smoke_skip REASON` | 这个平台无法演练该阶段所测的东西，阶段就把这话说出来而不是让一次检查失败。退出 77；runner 把它算作 `skip`，与 pass 分开计，所以一次绿的 macOS 运行从不会悄悄意味着「有一半阶段什么都没做」。唯一值得动用它的理由：一个 fixture 就是**原生剪贴板的缺席** |
| `smoke_abs_path PATH` | 变绝对但**保持原本给出的名字**（不走符号链接）—— 给二进制用，`zeroth update` 靠它路径里的 `target/…/debug` 区分出「这是源码目录里跑起来的」还是「装好的」 |
| `smoke_real_path PATH` | 变绝对并解析掉每个符号链接（含叶子）—— 给工作目录、`$HOME`、临时根用，app 会解析它们然后再比较 |
| `smoke_file_size FILE` / `smoke_file_mode FILE` | 一个文件的字节大小 / 权限位（`stat -c%s`/`stat -c %a` 是 GNU 的，`stat -f%z`/`stat -f %Lp` 是 BSD 的） |
| `smoke_sed_in_place SCRIPT FILE` | 就地改写文件而不用 `sed -i` —— 它的备份后缀参数上 GNU 与 BSD 各执一词 |
| `smoke_now_ms` | 以毫秒为单位的 epoch 时间，一个整数 —— GNU 的 `date +%s%N` / `+%s%3N` 在 BSD `date` 下会字面印出一个 `N`（或者什么都不印），所以每条计时断言都用它 |
| `smoke_braille_cells [STRING]` | STRING 或 stdin 里 U+2800–U+28FF 单元格的个数 —— 显式解码 UTF-8 字节，因为 BSD/toybox grep 会拒绝一个字节范围的方括号表达式，而 GNU 专有的 PCRE `\x{…}` 不可移植 |
| `count_bare_prompts` / `count_rules` / `count_footers` / `count_msg_lines` | 各尺寸变化阶段共用的「屏幕上恰好一个输入框」计数器 |

### 可移植性

这套件在 CI 的 Linux 上跑，也在开发者的 macOS 上跑，而那些有差异的工具在要紧
的地方失败得**静悄悄**：Mac 上的 `stat -c%s` 印出 `stat: illegal option -- c`
和一个空字符串，于是 `$(stat -c%s f) + 1` 变成 `1`，一个阶段就把整个文件当成
它的后半段 —— 一个假通过，而不是一次红运行。所以阶段问的是上面那些 `smoke_*`
helper，而不是直接问工具，而 `scripts/smoke/selftest.sh` 在它跑的任何平台上，
都拿一个有已知答案的 fixture 去断言它们每一个。由此得出的规则：

- **阶段里的哈希是 `cksum` 或 app 自己的输出**，不是 `sha256sum`。阶段只需要
  「同样的字节」（120 的 review 什么都没写回）时，POSIX `cksum` 就够了；需要
  「只有真实值才可能产生这个」（126 的占位符被展开）时，`cksum` 也够，因为比
  的是阶段自己算出来的哈希。
- **`grep` 的选项保持 POSIX。** macOS 的 BSD `grep` 没有 `-P`；断言用的是
  `-E`、`-F`、`-q` 和 `-c`。
- **从 `wc` 读回来的计数要过 `tr -d ' '`。** BSD 的 `wc` 会给数字补空格
  （`"       0"`），GNU 的不补，所以一个裸的 `[ "$(… | wc -l)" = 0 ]` 在 Mac
  上位假、在 CI 上位真。
- **墙钟差值走 `smoke_now_ms`。** `date +%s%N`、`+%s%3N` 和 `+%s.%N` 是 GNU
  专有的：BSD `date` 会印出一个字面的 `N`，于是一次算术展开要么报错，要么悄悄
  把差值变成 0。Phase 6、32、48、133 和 134 现在都量整数毫秒，包括 134 里那条
  必须自己写时间戳的 hook 命令。braille 单元格计数出于同样的理由走
  `smoke_braille_cells` —— `LC_ALL=C grep -oE $'\xe2[\xa0-\xa3][\x80-\xbf]'`
  对 BSD/toybox grep 是一个非法的字节序列。两个 helper 都由 `selftest.sh` 钉住。
- **app 会解析的路径走 `smoke_real_path`**，而它被*递交*的路径（那个二进制）走
  `smoke_abs_path`。在 macOS 上 `$TMPDIR` 是 `/private/var/...` 后面的
  `/var/...`；用 mbx 时 `target/` 是一个指向构建缓存的符号链接，解析它会交给
  `zeroth update` 一个在任何 `target/` 之外的路径（Phase 116）。
- **平台 libc 的错误字符串要宽松匹配。** ENXIO 在 Linux 上是
  `No such device or address`，在 macOS 上是 `Device not configured`
  （Phase 44）；断言那次失败，而不是它的某一种拼法。

一个回合的摘要穿着它结束时所处状态动词的过去式（`Worked for 3s`、
`Generated for 1m 5s` —— `docs/ui/status-indicator.md`），而一个跑过 30s 的
回合，结束时的动词比它开场时更靠后。所以等待*某个*回合落定的阶段匹配
`SUMMARY_RE`，也就是库里点名每个过去式的那个模式（`-E "^$SUMMARY_RE"`，或者用
`wait_summaries` 来数它们），而只有必须在一屏上区分各回合的阶段才点名某个回合
自己的动词 —— `SUMMARY_TURN1`…`3`，因为一次启动里的各个回合按顺序以 `Worked`、
`Generated`、`Pondered`… 结束，同时每一个都还在 30s 以内。`app::tests::turn`
把那个模式、它在各个独立脚本里的副本，以及逐回合的名字，都钉到动词表上，所以
增、删、改序一个动词，失败的是 `cargo test` 而不是一次 smoke 运行。

一行斜杠面板行以选择的 `→ ` 标记或其余每一行都穿的两个空格内缩开场，所以阶段
靠 `PALETTE_ROW` 找到它（`-E "${PALETTE_ROW}fast"` —— `/fast` 那一行，高亮与否
都算），而不是靠一个裸的 `^/` —— 现在没有任何行以它开场，而那样会让一个否定检查
无论面板显示什么都通过。

每个超时都是**上限**：条件一旦成立，等待立刻返回，所以这套件在快机器上以 app 的
速度跑，在慢机器上也照样通过。老脚本给自己立的规矩仍然成立 —— *轮询状态，绝不
固定睡眠赌运气* —— 而剩下的固定睡眠都是 fixture 的一部分（打字与 Enter 之间的
0.2s；一个否定断言需要保持打开的时间窗；一段本身就是被测对象的停顿）。

## 写一个阶段

```bash
#!/usr/bin/env bash
# Phase 116 — the /foo page opens, lists its rows and closes on Esc
# smoke: tags=serial            ← only if the phase measures time
#
# Why this exists, what it drives, what it proves. The header comment is the
# phase's documentation; the runner's --list shows the first line.

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

S116="${S}_foo"
launch "$S116" 100 30
submit "$S116" "/foo"
page="$(wait_pane 3 "$S116" -F "Foo rows")"
dump "the /foo page" "$page"
expect_has "$page" -F "❯ 1. First row" "the page did not list its first row"
expect_lacks "$page" -F "dummy_model_name" "the page did not displace the footer"
keys "$S116" Escape
wait_for 3 "$S116" -F "dummy_model_name ·" || fail "Esc did not close the page"
```

- 把文件命名为 `NNN-slug.sh`，用下一个空闲编号；id、slug 和标题都来自文件名，
  所以没有任何地方需要注册。字母后缀（`107b`）是给扩展其邻居的阶段用的。
- `S` 是会话名前缀；把每个 session 命名为 `${S}_something`，并给阶段自己的
  尺寸 —— 框是该场景需要的样子。
- 抓取你要断言的那些 pane 并 `dump` 它们：失败时日志就是证据，而老脚本那个
  打印每次抓取的习惯，正是让它的失败能仅凭一条 CI 日志诊断的原因。
- 阶段创建的一切都落在 `$SMOKE_TMP` 下（`mktemp -d` 自己会这么做）；harness
  会移除树并杀掉 server，所以阶段不需要任何清理代码 —— 老套件那些逐阶段的
  `kill-session` 行是无害的，一个阶段重启某个 session 名时可以留着。
- 需要在临时 cwd 里跑 app 的阶段给 `launch` 传 `-c "$dir"` 并用
  `$BIN_ABS`/`$APP_ABS`；要重新打开 checkpoint 的阶段必须只在这样的 cwd 里做
  （docs/state/checkpoint.md —— 一次恢复会跑 `git clean`）。
- **读页脚尾部的阶段要在 `work_dir` 里启动。** 页脚把 cwd 排在 `· N shells`
  计数和上下文量表前面，并从右侧截断这一行，而由于每个阶段的 HOME 都是它自己的
  临时树，仓库 cwd 不再缩写成 `~/…`：它渲染成绝对路径，于是那些断言会随着开发者
  恰好把仓库克隆到多深而通过或失败。一个 43 个字符的 checkout 路径在 80 列下就已
  经把 `· 1 shell` 切成了 `· 1 sh…`，让 Phase 42 失败，并让 Phase 43 等同一段
  文本时静默超时。而 `~/work` 无论仓库在哪儿都是六列。
- **在等一个回合结束之前，先等它开始。** 一个在状态行*消失*时 break 的落定循环，
  在负载高的机器上会在第一帧画出之前取样、立刻 break，与回合抢跑。Phase 58 到
  63 每个都以 `wait_for 20 … "esc to interrupt"` 开场，就是这个原因；Phase 62
  在一次完整并行运行里正是这样失败的，单独跑时却通过。
- 断言是一次**度量**的阶段 —— 一个延迟、一个时间窗内的字节数 —— 给自己打
  `# smoke: tags=serial`，好让 runner 把它留到并行池排空之后单独跑。

## 速度

有两件事让这套件比它所替代的巨石脚本更快，而两件都不改变一个阶段检查什么：

1. **并行。** 各阶段现在在构造上就是独立的，所以 runner 把它们装到 `-j` 个
   worker 上 —— 默认是 CPU 数的两倍，上限八个 —— 已知最长的先跑。这套件是等待
   受限的，不是 CPU 受限的：一个每 chunk 45ms 流式输出的 app 和一个每 100ms 取样
   一次的 shell 循环几乎不占什么，所以墙钟时间大约按 worker 数除下去，直到最长的
   那些阶段（每个约 40s）成为界。
2. **等状态而不是等固定时间。** 每次启动都等 prompt 而不是睡 0.4s，每次轮询都
   按 100ms 取样 —— 这更多是稳健性上的收获而不是速度上的（慢机器不再与一个固定
   睡眠抢跑），而更快的 fixture 暴露出的两个对取样敏感的阶段，现在会先等它们随后
   要断言的那一行（31 接受同秒 picker 显示的 `now` 年龄；105 在读取详情页尾部
   之前先等它）。

在一台 4 核机器上驱动 debug 二进制实测，同样是那 115 个阶段：

| 运行 | 墙钟时间 |
|---|---|
| 老的单体脚本 | 21m16s |
| `scripts/smoke.sh --serial`（一次一个阶段） | 21m04s |
| `scripts/smoke.sh -j 4` | 5m16s |
| `scripts/smoke.sh -j 8`（这里的默认值） | 2m38s |

串行那个数字才是诚实的那个：一个阶段的时间是 dummy 自己的流式输出，加上属于它
fixture 的那些等待，所以轮询式启动省下的大约就是逐阶段 server 的开销，而装箱
才是全部的收益。一个度量时间的阶段（一个延迟、一个时间窗内的字节数）可以给自己
打 `# smoke: tags=serial`，在池排空后单独跑 —— 在四核的 `-j 8` 下没有阶段需要
它，但门在那儿，留给将来需要的那个。

## runner 能抓到而阶段自己抓不到的

一个阶段在一次失败的检查之后继续走（后面的检查说得更多），所以它的退出状态是
失败检查的条数，而不是最后一条命令的。阶段唯一无法自己报告的失败是 bash 的中止
—— `set -u` 下一个未绑定的变量、一个语法错误 —— 它在其余检查跑起来之前就结束了
脚本；bash 会为它们打印 `file: line N: …`，而 runner 只要在日志里看到那一行，
就把该阶段标为失败，无论退出状态说了什么。
