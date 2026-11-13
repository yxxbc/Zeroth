# 多线程内核 —— session 并发，实测

日期：2026-11-12

状态：**测量、建议，以及第一个客户端**。接在
[`concurrency.md`](concurrency.md) 之后。基准测试 harness 是
`src/bin/sessionbench.rs`，藏在 `--features bench` 后面。第一个产品客户端是
`-p --jobs`（`docs/cli/batch.md`）；剩下的 Host 工作列在下面。

## 问题

[#24](https://github.com/linuztx/alter-zero/issues/24) 在问：每个 session 一个
进程是不是有意设的天花板。它拿来对比的基准测试同时跑 15 个 session，旁边还压
着一个 16 线程的编译，而且仍然保持响应。这个问题的最小单位是：**一个进程里 N
个 session**（[#27](https://github.com/linuztx/alter-zero/issues/27)），在
session host 和 TUI 客户端存在之前
（[#28](https://github.com/linuztx/alter-zero/issues/28)）。

所以“支持多线程”的意思是：几个独立的 session 同时跑在几个核上，各自有自己的
对话、rollout、permission gate 和 MCP manager，一个 session 的回合不阻塞另一个。
它**不**指一个回合内部的并行 tool call
（[`tools/parallel-tools.md`](../tools/parallel-tools.md)）—— 那是另一个独立的
串行化点。

## 测了什么

一个临时二进制在一个进程里跑 N 次**真实的** `tui::headless::run` —— 与 `-p`
二进制同样的 bootstrap、回合循环、记录器与关闭流程。它提供四种形状：

| 形状 | 怎么做 |
| --- | --- |
| `serial` | N 个 session 一个接一个，无并发 |
| `thread` | 每个 session 一个 `std::thread`，各自带自己的 current-thread tokio runtime |
| `pool` | W 个工作线程从一个 channel 消费 N 个 job |
| `async` | 一个多线程 tokio runtime；每个 session 是一个 `spawn_blocking` job（见 *为什么不是一个 task？*） |

负载：内置 dummy 后端，设了 `ZEROTH_STALL_MS`，于是每个 session 在首 token 之前
的停顿上花掉已知的 300–800 ms —— 一个网络受限回合的形状 —— 然后不靠网络也不靠
token 作答。指标：进程 RSS（`ps -o rss`）、线程数（`ps -M`）、墙钟，以及 stderr
（bootstrap 会报告竞态）。config home 是共享的，就像 host 会共享它那样。

复现：

```bash
cargo build --features bench --bin sessionbench
ZEROTH_DUMMY=1 ZEROTH_STALL_MS=300 ZEROTH_CONFIG_DIR=$(mktemp -d) \
  target/debug/sessionbench thread 15
```

## 结果

15 个 session，500 ms 停顿 —— 也就是那次对比自己的形状：

| 形状 | 墙钟 | RSS | 线程 | bootstrap stderr |
| --- | --- | --- | --- | --- |
| 每个 session 一个进程（16 个 session，800 ms） | 0.98 s | 汇总 232 MB | — | — |
| `thread` | 0.77 s | 21.7 MB | 46 | 138 B |
| `async`（`spawn_blocking`，4 个 worker） | 0.64 s | 19.8 MB | 50 | 0 |
| `pool`（8 个 worker） | 1.41 s | 17.7 MB | 25 | 0 |
| `pool`（4 个 worker） | 2.37 s | 16.5 MB | 13 | 0 |

伸缩，300 ms 停顿（fd 软限制提到 4096 —— 见 *第一堵墙*）：

| N | `thread` 墙钟 / RSS / 线程 | `async` 墙钟 / RSS / 线程 | `pool` W=4 墙钟 / RSS |
| --- | --- | --- | --- |
| 1 | 0.38 s / 13.9 MB / 4 | 0.52 s / 14.1 MB / 8 | 0.46 s / 14.0 MB |
| 8 | 0.46 s / 17.6 MB / 25 | 0.45 s / 17.7 MB / 29 | 0.84 s / 16.8 MB |
| 16 | 0.46 s / 20.1 MB / 49 | 0.52 s / 20.5 MB / 53 | 1.57 s / 17.0 MB |
| 32 | 0.61 s / 26.1 MB / 97 | 0.54 s / 26.3 MB / 101 | 2.99 s / 16.9 MB |
| 64 | 0.63 s / 37.6 MB / 193 | 0.62 s / 37.8 MB / 197 | — |
| 128 | 0.80 s / 60.5 MB / 385 | 0.68 s / 60.9 MB / 389 | — |
| 256 | 1.08 s / 101.1 MB / 720 | 1.15 s / 105.9 MB / 773 | — |

串行成本供对比：8 个 session 3.04 s，16 个 session 5.74 s，RSS 平在 15 MB。

## 数字说明了什么

**线程在这里很便宜；session 不是。** 一个 session 花约 0.4 MB RSS 和三个线程
（它 runtime 的 reactor/worker、回合线程、transport 或 dummy 的 worker）。15 个
session 装进 22 MB 与 46 个线程；256 个装进 101 MB 与 720 个线程。内存线性增长，
而墙钟直到调度器饱和之前都停在一个停顿左右。进程模型每个 session 汇总起来是
14.5 MB RSS（16 个就是 232 MB）；这个汇总高估了共享页 —— 每个进程的峰值内存
占用约 7 MB —— 但即便是独占的那份，也是进程内形状的几倍。

**每个 session 一个真 async task 这条路不可用。** `Session` 是 `!Send` —— 它
跨一个 await 持有 syntect/onig 的裸指针 —— 所以 `JoinSet::spawn` 拒绝它。能
编译的 async 形状是 tokio 的阻塞池（`spawn_blocking`），那就是每个 session 一个
线程，再加四个 runtime 线程、多约 1 MB RSS，吞吐却没涨。一个 async 内核是对
`Session` 与阻塞式 LLM 客户端的重写，不是一个 runtime 开关。

**有界池用天花板换一个上界。** 四个 worker 把并发封在四：15 个 job 花 2.37 s
而不是 0.77 s，RSS 是 16.5 MB 而不是 21.7 MB。八个 worker 补上了大部分差距
（1.41 s，17.7 MB）。那是准入控制该拧的旋钮，不是拿最高天花板该用的。

**第一堵墙是文件描述符，不是线程也不是内存。** 在默认的 256-fd 软限制下，64 个
session 创建不了它们的 tokio runtime（`Too many open files`，EMFILE）；15 个
没事。每个 session 的 current-thread runtime 占一个 kqueue/epoll 描述符，而
bootstrap 还要打开 config、skill、agent 与 rollout 文件。host 在接近 50 个
session 之前必须抬高这个限制（或共享一个 runtime）。

**共享 bootstrap 是第二堵墙。** 到 64 个以上 session 时，并发的 bootstrap 写
同一批 `agents/*.md` 与 `skills/*/SKILL.md` 文件，又读回写到一半的它们：
`missing YAML frontmatter delimited by ---` 在每一次 64-session 运行里都出现过，
而一次 8 个 session 的 async 运行也碰上了。这条写路径在并发下不是幂等的；必须
由一个 Host 在 session 启动之前完成这些 config/skill/bootstrap 写入。

## 代码里发现的约束

- `Session<'t>` 借用一个 viewport；池需要每个 session 一个 sink viewport
  （`InlineViewport::headless()` 已经存在，这个 bench 用的就是它）。
- `host::session_id()` 是纳秒 + pid —— 同一纳秒启动的两个 session 共用一个
  rollout 路径。
- `palette::ACTIVE` 是线程局部的；session 线程不能假设自己拿的是主线程的
  palette。
- `Signals::install()` 是每次运行一个；host 想要一个进程级 handler 和一次关闭
  广播，而不是 N 条信号流。
- MCP manager、provider 客户端和 skill/agent 根目录都由 `Session::bootstrap`
  （1,185 行）按 session 装配；共享的那一半属于 Host。
- TUI 循环是在一个 `Session` 上的一个 `current_thread` runtime。在一个进程里跑
  N 个 session 并不会给 TUI N 个窗格；那是 #28 的 transport/客户端工作，不是
  这个基准测试。

## 另一条轴：一个回合的 tool call

`examples/concurrency_probe.rs` 在 `K` 条独立的 `sleep 1` 命令上，以三种形状驱动
**真实的** `RealToolExecutor`（每个 call 一个 pty 和一个子进程）：`seq`（今天的
`llm/agent.rs` 循环）、`threads`（每个 call 一个线程）和一个有界 `pool`。这是
[#24](https://github.com/linuztx/alter-zero/issues/24) 的方案 A，不是 session
host。

8 个 call，各 1 s（外部 RSS/线程采样）：

| 形状 | 墙钟 | RSS | 线程 |
| --- | --- | --- | --- |
| `seq` | 8.91 s | 8.1 MB | 3 |
| `threads` | 1.23 s | 8.6 MB | 25 |
| `pool` W=4 | 2.34 s | 8.4 MB | 13 |
| `threads` 16 个 call | 1.26 s | 9.2 MB | 49 |
| `threads` 32 个 call | 1.31 s | 10.2 MB | 97 |
| `pool` W=8，32 个 call | 4.58 s | — | — |
| `pool` W=16，32 个 call | 2.33 s | — | — |

一个并发 call 花约 65 KB RSS 和三个线程；32 个 call 仍然装进 10 MB。取消已经能
跨它们工作：设 `PROBE_CANCEL_MS=300` 时，四个 `sleep 5` 的 call 在 0.45 s 内停住
并全部报告失败，thread 和 pool 两种形状都是如此 —— 共享的 `CancelToken` 能到
每一个子进程。这里的天花板是 pty/子进程限制和 executor 的单子进程假设，不是
线程成本：

- `llm/exec.rs` 的超时、`kill_process_group` 与取消路径都假设只有**一个**子
  进程；一个批次需要 N 个进程组和一次联合回收。
- `ToolOutput`/`ToolScreen` 过去是一条不带标签的流，追加到最前面的那个 call 上；
  它们现在带一个 `call_id`，由应用把每个 chunk 路由到它的 cell
  （`docs/tools/parallel-tools.md`），所以一个 executor 上的并发 call 是可表示的
  —— 但 executor 自身仍然假设每个 call 一个子进程，而 permission gate 一次只有
  一个模态。
- 渐进提交假设最前面的那个 call 先提交；有了按 call 的 id，剩下的一块工作是让
  一个已完成的 worker 在其兄弟仍在跑时就解析出来。

**如果目标是 tool executor**，就天花板而言，实测的赢家是每个 call 一个线程；
要可预测的内存和 UI 顺序，4–8 个 worker 的有界池外加按 call 的事件标签是更安全
的第一步（8 个 call：W=4 时 2.3 s，W=8 时 1.2 s，串行 8.9 s）。先上上限，再把
每个 call 一个线程的形状放在它后面。

## 建议

**每个 session 一个线程，加上有界准入，再加一个共享的 Host。** 就 session 这条
轴，具体是：

1. 一个 `Host` 拥有 config home、provider/model 目录、MCP 配置、skill/agent
   注册表、bootstrap 的那些写入、信号 handler 与 session id 来源。
2. 一个 `Session` 拥有 `App`、rollout、permission gate、自己的 MCP manager 和
   自己的 current-thread runtime，钉在一个 OS 线程上。
3. 准入是一份固定的线程预算（`--concurrency N`），不是无界的 `thread::spawn`；
   队列是显式的、可测量的。
4. 批量的 `-p`（JSONL job）是第一个客户端；TUI 要等 #28 的 transport 之后才
   成为第二个客户端。

为什么不用替代方案：`spawn_blocking` 多花了 runtime 线程和内存，却没换来吞吐；
一个真 async 内核先要 `Session: Send` 和一个 async transport；每 session 一个
进程仍然是隔离基线，但每个任务要付约 7 MB 的峰值占用（14.5 MB 汇总 RSS）和一
整次 bootstrap。

第一个 Host/Session 里程碑的验收：一个进程里 15 个 job、RSS 低于 100 MB、没有
bootstrap 警告、遵守默认 256-fd 限制，墙钟在最慢 job 的 1.5 倍以内。

## 第一个客户端，已实现

`-p --jobs`（`docs/cli/batch.md`）在一份固定的 worker 预算上，为每个 job 跑真实
的 headless session。它取的是**每个 session 一个线程 + 有界准入**的形状，不是
`spawn_blocking`：每个 worker 在自己的 OS 线程上拥有一个 current-thread
runtime，所以 `Session` 的 `!Send` 无关紧要 —— 它从不离开创建它的那个线程。
共享状态的墙都在它们最小的位置上修掉了：首次运行的种子写走一个临时文件加一次
原子重命名，而 session id 带一个进程内序列。

2026-11-13 用离线 dummy 后端和 `ZEROTH_STALL_MS=300` 实测（worker 自己的
bootstrap 也算在内，与上面的 harness 不同）：

| job | 并发 | fd 软限制 | 墙钟 | 最大 RSS | stderr |
| --- | --- | --- | --- | --- | --- |
| 8 | 4 | 默认（这里是 1048576） | 0.98 s | — | 空 |
| 15 | 8 | 默认 | 0.66 s | 18.6 MB | 空 |
| 15 | 15 | 256（`ulimit -n`） | 0.35 s | — | 空 |
| 64 | 64 | 4096（`ulimit -n`） | 0.43 s | 38.5 MB | 空 |

那次 64-session 运行并发地用了签入仓库的 `skills/`/`agents/` 种子 —— 正是上面
那个 harness 看到半写文件和 `missing YAML frontmatter` 的规模 —— 而它启动时
stderr 是空的。那是原子种子修复在起作用。

一次两个 worker、每个 job 五秒的运行里，第一秒就来了一个 `SIGINT`：进程以
`130` 退出，把两个在飞的 job 都报告为已中断，队列里的 job 一个也没启动，而
stderr 是空的。这就是验收的形状：没有 bootstrap 警告、远在 RSS 上限之内，而且
15 个并发 session 还在原来那个 256-fd 软限制之内。

## Host

批量现在**一次**把进程级的事实解析出来，放在 `tui::host::Host` 里，然后把
`&Host` 交给每个 session：

- 工作目录、home 和 config home；
- fork 之前的 home 迁移与首次运行的 skill/agent 种子（本来就已是原子的 ——
  Host 现在是唯一一处会在 session 启动之前跑它们的地方）；
- skill 与 subagent 的遍历，连同它们的解析错误，以及本项目 `skills.json` 里
  禁用的集合；
- 本目录的 `/settings` 条目，加上 telemetry/update/tips 文件与 `ZEROTH_*` 覆盖
  项，以及 tips 游标；
- secrets 文件、用户的 `hooks.json`（路径与解析），以及模型解析器的输入：
  provider 表、`.env`，还有已保存的选择（`adopt_selection` 现在每个进程钉一个
  目录，不是每个 session 一次）。

session 仍然把它需要的东西克隆进自己的注册表和 store；Host 自己在
`Host::resolve` 之后是只读的。单次 `-p` 运行或 TUI 传 `None`，在 bootstrap 里
解析同样的事实，所以行为不变。

仍然**没**做的：

- **MCP 配置与 manager** 还是每个 session 一份。解析后的源取决于这个 session 的
  trust 状态，而每个 manager 都持有活的连接；共享一个连接池是 Host 的下一步，
  而且它先需要一次测量才谈得上是设计。
- **信号**还是每个 session 一份（tokio 只广播一个进程 handler，而每次运行装
  自己那条流）。批量是对的，因为每个在飞的 session 都应答信号并停下；但一个
  进程级的关闭广播仍然更干净。
- **TUI 还不是客户端。** 它是覆盖一个 `Session` 的一个 `current_thread`
  runtime；N 个窗格需要 #28 的 transport 工作，不是这条批量路径。
- **provider 的 HTTP 客户端**仍然按 session 构建。共享它是一个 Host 关切，也是
  一次测量，不是一次猜测。
- Host 是一个**快照**。批量运行期间被编辑的配置文件，不会被已经启动的 session
  重新读取；新进程会看到它。对一次 headless 运行来说，bootstrap 时解析的一切
  本来就是这个行为。
