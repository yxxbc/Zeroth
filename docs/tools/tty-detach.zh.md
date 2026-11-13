# 把子进程从控制终端上摘下来

这个应用运行的每一条 shell 命令 —— 模型的 `bash` tool
（[`llm::exec::run_bash`](../../src/llm/exec.rs)、
`docs/tools/tools.md`）、用户的 `!` shell
（[`tui::shell::spawn_shell_command`](../../src/tui/shell.rs)、
`docs/input/shell-command.md`），以及后台任务
（[`background::BackgroundRegistry::launch`](../../src/background.rs)、
`docs/tools/background.md`）—— 都通过一个共享的边界辅助函数
[`subprocess::spawn_detached_shell`](../../src/subprocess.rs) 启动，它在一个
**没有控制终端的新 session** 里运行命令。本文说明为什么这样做，以及 crate 的
`forbid(unsafe)` 如何塑造了这个机制。

## 这个 bug：`sudo`（及其同类）劫持 TUI 并挂住

通过 `bash` tool 运行 `sudo pacman -S …`（或任何需要密码的东西）会把

```
[sudo] password for linuztx:
```

直接写进真实终端 —— **把 live TUI 搅乱** —— 然后挂住，直到每条命令的超时
（默认 30 s）触发。tool 报「ran too long」，而终端被留在损坏状态。

根因是一个 Unix 的微妙之处。我们已经在用 `stdin(Stdio::null())` 启动，所以从
**stdin** 读密码的程序立刻得到 EOF。但 `sudo`（以及 `ssh`、`git` 的凭据
prompt、`passwd`、…）刻意**不**从 stdin 读密码 —— 它们直接打开进程的**控制
终端** `/dev/tty`，正是为了让脚本没法通过管道把密码喂给它们。重定向 stdin 对此
毫无作用。因为我们的子进程继承了 TUI 的控制终端 —— 通过进程 *session*，而不是
通过任何文件描述符 —— `sudo` 打开 `/dev/tty`，那正是 TUI 以 raw 模式画图的
那个终端，在那里打印它的 prompt，然后阻塞着等一次击键，而那次击键已经被 TUI 的
输入循环截走了。死锁，直到超时。

## 修复：新 session，让 `/dev/tty` 打不开

一个进程的 `/dev/tty` 是**它所属 session** 的控制终端。一个领着 *没有* 控制
终端的 session 的进程没有 `/dev/tty`，所以打开它立刻失败：

```
sh: cannot open /dev/tty: No such device or address
```

所以我们把每条命令放在一个全新的 session 里运行。`sudo` 这时够不到任何终端，会
**快速**失败，stderr 上是一条被捕获、模型可读的错误，而不是挂住并在 TUI 上乱涂：

```
sudo: a terminal is required to read the password; either use the -S option
to read from standard input or configure an askpass helper
```

退出 1，毫秒级，输出像任何其他命令一样被捕获 —— 从不碰真实终端。（随后单元格
在红色圆点下显示换行后的消息，而 `Exit code: 1` 那一段只给模型 ——
`docs/ui/tool-streaming.md`。）

## 为什么不用 `pre_exec(setsid)` —— 以及取代它的层级链

在进程内创建一个新 session，意味着在 `fork` 与 `exec` 之间于子进程里调用
`setsid(2)` —— 也就是 `CommandExt::pre_exec`，一个 `unsafe fn`。这个 crate
**`forbid` 了 `unsafe`**（`Cargo.toml` `[lints]`），而 `forbid` 不能被局部
覆盖。所以 [`subprocess::tiers`](../../src/subprocess.rs) 按顺序尝试，**只在**
`NotFound` 这类 spawn 错误上往下落（任何其他错误都是真的，直接暴露）：

1. **`setsid sh -c "<command>"`** —— util-linux 的 `setsid(1)` 二进制调用
   `setsid(2)` 并把 `sh` `exec` 进新 session。简单，Linux 上的日常层级，而且
   独立于我们自己的二进制（一次 `cargo build` 在 session 中途替换掉正在运行的
   TUI 的文件也弄不坏它）。
2. **helper 重执行** —— `{current_exe} __zeroth-detached-exec "<command>"`：
   我们自己的二进制，它的 `main()` 把
   `subprocess::run_detached_exec_if_requested()` 作为**第一条语句**运行。
   helper 模式下它调用安全的 `rustix::process::setsid()` 包装，并**就地**
   `exec` `sh -c "<command>"`（同一个 pid），从不返回。正是这个让 macOS 和精简
   过的镜像 —— 那里没有 `setsid` 二进制 —— 不至于退化回那个附着的 bug。它是
   按进程选入的：helper 路径在启动时解析一次，并穿在 `BackgroundRegistry` 上
   （`with_detach_helper` → `detach_helper()`），每个 runner 本来就共享它。一个
   `cargo test` 二进制有 libtest 的 `main`，不是我们的 —— 重执行它会跑测试套件
   而不是那条命令 —— 所以测试改为安装 cargo 在它们旁边构建的 `zeroth` 二进制
   （*Testing*）。
3. **附着的 `sh -c` + `process_group(0)`** —— 修复之前的行为，两条 detach 路线
   都不存在时的最后手段（仍然完全可以被杀掉，但共享控制终端）。

非 Unix（Windows）没有 `/dev/tty`，也没有 session 可以逃出去；它只用朴素的那条
路径。

### 链条的 TTY 形式

一次 `bash` 调用（`docs/tools/bash-tools.md`、
`docs/tools/interactive-shell.md`）需要的是 detach 的**反面** —— 一个控制
终端 —— 但是它自己的伪终端，绝不是用户的。那是 `setsid(2)` **加上**新
session 上的 `TIOCSCTTY`，仍然没有 `pre_exec`，所以同一条链长出了一种
TTY 形式（[`subprocess::tty_tiers`](../../src/subprocess.rs) 和
`tty_command_for`）：`setsid -c bash -c "<command>"`（util-linux 的
`--ctty` 让 stdin —— pty 的从端 —— 在 `setsid` 之后成为控制终端），然后是
它自己哨兵下的 helper，`__zeroth-detached-tty-exec`，它在
`run_detached_exec_if_requested` 里的分支在 `setsid()` 之后、`exec` shell 之前
加上安全的 `ioctl_tiocsctty(stdin)` —— 当它不是 `sh` 时由一个可选的第三个
参数指名（`subprocess::tool_shell`：装了 bash 就用 `bash`）。**没有附着
层级**：一个附着的 shell 的 `/dev/tty` 就是用户的终端 —— 正是上面那个 bug ——
所以在两条路线都没有的机器上，命令改在一条管道上运行，由上面那条朴素链
detach。两个层级都保持 `child.id() == pgid == sid`，也就是下面的不变量；
`tests/detached_exec.rs` 证明 helper 层级的 `/dev/tty` 到达的是 session 自己的
终端。

## 承重的不变量：`child.id() == pgid`

三个启动点在超时 / Esc / 一次 registry 停止时都杀掉**整个进程组** ——
`kill -KILL -<pgid>` —— 而每个点都把 `pgid` 算作 `child.id()`，依赖子进程领着
自己的组（`pgid == pid`）。修复之前的代码用 `process_group(0)` 保证这一点。两个
detach 层级都**免费**保住它，但只有用对了才行：

- `setsid(2)` 让调用者成为新 session **以及**进程组的首领（`pgid == pid`），
  随后包装程序 `exec` `sh` —— 同一个 pid。所以 Rust 握着的那个 `Child` *就是*
  组首领：`child.id() == pgid`。✓
- **包装程序在启动时绝不能是进程组首领**，否则 `setsid(2)` 会 `EPERM` ——
  `setsid(1)` 这时会 `fork`（一个新的、我们不跟踪的 pid），而 helper 会以附着
  方式运行。一个刚被 `Command::spawn` 出来的子进程从不是组首领，**除非我们主动
  要求**，所以 [`subprocess::command_for`](../../src/subprocess.rs) 刻意**不**在
  detach 层级上设 `process_group(0)` —— 只有附着层级才认领自己的组。

因为这个不变量在每个层级上都同样成立，那几个杀组的辅助函数
（`exec::kill_process_group`、`main::kill_shell_group`、
`background::kill_group`）以及 Ctrl+B 收养（`docs/tools/background.md`）
**都没有变**。退出状态也不变地
传递：两个 detach 层级都 `exec`（不 fork），所以 `child.wait()` 给出命令的真实
状态（`exit 7` → `Some(7)`），而模型读到的 `Exit code: N` 框架正依赖它。

## 什么*不*变

普通命令（`ls`、`grep`、`cargo`、`git status`）从不碰 `/dev/tty`，所以它们的
行为与以前完全一样 —— 同样的输出，同样的退出码。`isatty(0/1/2)` 也没变：stdin
本来就是 `/dev/null`，stdout/stderr 本来就是管道（从不是 tty），所以那些按
「我在不在终端上？」变行为的命令，之前和之后看到的都是非 tty 的 fd。新 session
只去掉了 `/dev/tty` 访问，别的什么都没有。Stdio 集中在 spawner 里
（`command_for`），所以三个 runner 拿到的是完全相同的契约。

## 测试

- **纯的** —— `subprocess::tiers` 排序这条链（`SetsidBinary` →
  安装后就 `HelperReexec` → `Attached`）；`command_for` 每个层级的 argv，包括
  `main()` 那个 hook 所解析的 helper 协议 `{exe} __zeroth-detached-exec
  {command}`。
- **边界（真实 `sh`）** —— 一条被启动的命令报告自己是**会话首领**（`/proc` 的
  session 字段 == `$$`，这只有在 detach 时才成立 —— pgrp 字段即使附着也等于
  `$$`，所以只有 session 能区分）；输出加一个非零退出穿过各层传播；杀组仍然回收
  一个放到后台的孙进程。执行器与 registry 能熬过一条**陈旧的 helper 路径**
  （链条往下落）。`tests/detached_exec.rs` 驱动**真实构建出来的二进制**
  （`CARGO_BIN_EXE`）：执行器端到端，以及直接驱动 helper 重执行层级 —— 那条
  代表 macOS 的路径。
- **库自己的测试**在没有 `setsid` 二进制的地方也通过 helper 启动它们的 session
  与 detach 命令：各 registry、session 的启动以及 detach 用的 fixture 都安装
  cargo 在测试二进制旁边构建的 `zeroth` 二进制
  （`subprocess::test_detach_helper` —— `target/debug/zeroth` 对应
  `target/debug/deps/zeroth-…`）。Linux 上 `setsid` 先来，helper 只在它后面；
  在 macOS 上（它不自带 `setsid`），helper 就是每个终端测试所跑的层级，正如应用
  在那里一样 —— CI 的 macOS job 检查 `PATH` 上没有 `setsid`。`cargo test` 在跑
  任何东西之前都会先构建二进制；只跑 `cargo test --lib` 不会，所以在 Mac 上一个
  全新的检出需要先 `cargo build`，而改动 helper 本身则需要重新构建。
- **Smoke** —— `scripts/smoke.sh` Phase 44 在一个真实的 tmux pty 里驱动一条向
  `/dev/tty` 写字的 `!`-shell 命令，并断言它在单元格里快速失败
  （`No such device or address`），且没有任何东西打到 TUI 上。
- **Live** —— `tests/live_openrouter.rs` 的
  `live_sudo_style_tty_prompt_fails_fast`：一个真实模型发出会要 tty 的 `bash`
  调用，经由生产链条在毫秒级解析为失败。
