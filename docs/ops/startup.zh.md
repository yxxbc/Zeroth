# 启动、空闲开销与首个回合 —— 实测

日期：2026-11-27

一张对比表让 zeroth 在六个数字上同时落后于 `alter-zero` 和 `litemiyu`：启动到
首字节、启动到可打字、空闲 `phys_footprint`、空闲 RSS 与线程数，以及一个带工具
调用的回合的 CPU 与内存峰值。这一页说清每个数字到底是什么、修了什么、还剩什么。
这里每个数字都在开发机上实测（8 核 Apple silicon，macOS 27.2，release 构建），
用下面列出的探针 —— 不是估算。

## 原来错在哪

两处，都不是渲染器。

### 1. 首帧路径里的一个子进程 —— 已修

`checkpoint::git_available()` 靠运行 `git --version` 并等它来回答「这里能不能做
checkpoint」。实测：每次 spawn **5.0–6.1 ms**（五次），在 bootstrap 里，首帧之前。
macOS 上 `/usr/bin/git` 是 Xcode 命令行工具的 shim，第一次运行时还可能卡在一个
*安装对话框*上 —— 一间「应用一启动就卡住」报告的温室。

这个答案从来只需要一次 `PATH` 查找：`PATH` 上有没有一个叫 `git` 的可执行文件？
那就是 `subprocess::find_on_path` + `is_executable_file`，仓库里早就为 shell 探测
备好了。一个存在但坏掉的 `git` 仍然每次调用都响亮地失败 —— `snapshot()` 返回
`None`，回合照常继续，这正是这个特性原本就有的失败模式。

### 2. 一天一次请求配一个常驻的 HTTP 运行时 —— 已修

更新检查（`docs/ops/update.md`）走的是**缓存**的 provider HTTP 客户端。对
provider 流量是对的 —— 暖连接池省一次 TLS 握手 —— 对一天一次的请求是错的：这个
客户端会让 reqwest 的内部运行时线程和它的定时器活一整个会话。实测它是

- 这个会话的**第四个线程**（主线程、两个 crossterm 读线程、它），以及
- **唯一**的空闲 CPU 唤醒来源：进程自己的
  `ri_pkg_idle_wkups`/`ri_interrupt_wkups` 计数器上是 6–7 次/分，而一个普通休眠
  进程是 0。

`llm::ephemeral_client` 建同样的策略但不缓存；更新 worker 在请求之后丢掉它，线程
随之消失。

## 现在的数字

| | 之前 | 现在 |
| --- | --- | --- |
| 线程数，空闲 | 4 | **3** |
| 空闲唤醒（自己的计数器） | 6–7/分 | **0/分** |
| `phys_footprint`，空闲 | 4.6 MB | **4.55 MB** |
| RSS，空闲 | 16.4 MB | **16.4 MB** |
| 启动，进程 → 首字节 | 5.7 ms | **5.7 ms** |
| 启动，进程 → 可打字提示符 | 13.4 ms | **12.5 ms** |
| 一个 dummy 回合：`phys_footprint` 峰值 | 5.6 MB | **5.6 MB** |
| 一个 dummy 回合：CPU | 2–3 ms | **2–3 ms** |

这两处修好值约 2.4 ms 的启动时间（那个 git spawn，单独测 5–6 ms，其中落在
bootstrap 里的部分在 11 次运行里是 2.4 ms），以及空闲线程与唤醒那一整列。启动那两
行是 git 修复前后同一个进程内测量；pty 探针在上面还加了自己的一次 shell spawn。

## 其余数字是什么，为什么不会怎么动

**`phys_footprint` 4.55 MB / RSS 16.4 MB。** RSS 里是二进制链接的那些框架的只读
共享页（AppKit、CoreText、Security、CFNetwork —— 都是 `arboard` 和 reqwest 的
系统 TLS 配置带进来的）；`phys_footprint` 算的是进程真正弄脏的内存，是 4.55 MB。
一个带这两个依赖的 Rust TUI（`reqwest` 与 `arboard`）大约就从这里起步；没有什么
按会话的数据结构可以删。

**三个线程。** 主线程（tokio 的 current-thread 运行时）、crossterm 的
`EventStream` 读线程，以及它自带的 crossterm 内部事件读线程。休眠的会话没有别的；
对运行中二进制的线程栈做一次采样（`sample`），三个都停在 `kevent` 或信号量等待上。
要降到两个，就得用我们自己的实现替换 crossterm 的事件源 —— 为省一个线程重写不变量
1 的那个唯一读者，这一页把它记作一个被否决的取舍。

**一个回合 2–3 ms 的 CPU（用 dummy 后端）** 是界面：给回复折行、提交行、一帧。模型
往返是墙钟时间（dummy 脚本节奏里是 2 秒，否则是网络时间），不是 CPU。

**启动到「可打字」（进程内 12.5 ms）。** 其中约 1.4 ms 是参数解析与会话解析，约
6 ms 是终端对光标查询的回答（一次真实的往返 —— 视口在知道光标在哪之前无法给输入框
定位，不变量 1），其余是 bootstrap：web 规则解析（约 0.5 ms）、skill 与 agent 的
遍历（暖时约 1 ms）、后端构建，以及首帧（约 0.1 ms）。对比表里的 35.3 ms 包含
启动承载这个应用的 tmux 会话（实测：这台机器上仅 tmux 建会话就要 80 ms），所以两个
数字不是同一种测量。

## 怎么复现

那些探针都是一次性脚本，作为记录留在这里而不是提交进仓库（它们是
`proc_pid_rusage` 和 pty 的胶水，不属于产品）：

- **启动** —— 在 pty 里 spawn release 二进制，用一行光标位置回答它的 `ESC[6n`，
  计下第一个输出字节和第一个 `❯` 的时间，取至少七次运行的中位数。
- **空闲** —— 相隔一分钟读两次 `proc_pid_rusage(pid, RUSAGE_INFO_V4)`：
  `ri_pkg_idle_wkups`、`ri_interrupt_wkups`、`ri_phys_footprint`、
  `ri_resident_size`、user + system 时间。对照：一个普通的 `sleep 300`。
- **一个回合** —— 在 pty 里用
  `ZEROTH_STARTUP_DELAY_MS=100 ZEROTH_CHUNK_DELAY_MS=1` 驱动一个 dummy 回合，
  每 50 ms 采一次 `ri_phys_footprint`，并在前后对 CPU 计数器做差。

## 有意没改的

- **缓存的 provider 客户端。** 它对 provider 流量是正确形状，而它的线程只在真的
  用上后端之后才存在。
- **光标查询。** 它是不变量 1 的那一次 stdin 读取，也是每个终端 TUI 为给视口定位
  都要付的一次往返。
- **crossterm 的两个事件读线程。** 一个键盘配两个线程，是为一个安全、上游维护的
  读者付出的合理代价。
