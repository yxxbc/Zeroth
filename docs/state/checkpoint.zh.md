# Checkpoint —— 重置的是代码，不只是 transcript

zeroth 是一个 coding agent：它的 `write`/`edit`/`bash` 工具会改动工作目录里的
文件。但两个「回到过去」的手势向来只倒回*对话* —— **`/resume`** picker
（`docs/state/resume.md`）重新载入一份旧 transcript，**Esc-Esc 回溯**
（`docs/input/backtrack.md`）把历史截断到更早的一条消息 —— 而磁盘上的代码还留在
最近一次工作放下的地方。于是恢复上周的 session，看到的是上周的消息压在今天的
文件之上：transcript 与代码对不上。

**Checkpoint** 补上这道缝。每个回合都把整个工作目录快照进一个隔离的 git 存储，
以对话长度为键。当你倒回对话 —— 恢复一个保存的 session，或回溯到更早的一条消息
—— 文件会被恢复成它们在那个时点的样子。这是 Claude Code 的 "checkpointing" 与
codex 的 `ThreadRollback`，按本代码库「纯逻辑 / 边界 I/O」的分法改造过。

## 你得到什么

- **回溯重置代码。** Esc-Esc 到一条早先的用户消息并按 Enter：历史截断到那条
  消息，*并且*工作目录被恢复到它最初发出时的状态（上一个回合的末尾，或对第一条
  消息来说是原始树）。一个
  `Reset files to the checkpoint for that message` toast 予以确认。
- **`/resume` 重置代码。** 在 `/resume` picker 里选一个保存的 session：它的
  transcript 被载入，*并且*工作目录被恢复到那个 session 的**最后**一个
  checkpoint —— 哪怕文件此后已经分叉。一个
  `Restored files to this session's checkpoint` toast 予以确认。
- **你真正的 git 从不被碰。** 快照住在 `~/.zeroth/checkpoints` 下的一个私有对象
  存储里，带自己的 `GIT_DIR`、索引、配置与分支。你的 `.git`、暂存区、分支和提交
  从不被读或写。一次恢复的清理也绝不删除你的 `.git` 或 vendored 目录。
- **没有东西真正丢掉。** 恢复对当前工作树是破坏性的，所以当前状态被**先**快照进
  存储 —— 可通过存储的 `git reflog` 找回。

## 模型

一个 **checkpoint** 是 `{ after: usize, commit: String }`
（`checkpoint::Checkpoint`）：在 `after` 条已完成的 [`HistoryItem`] 之后，工作
目录长成隔离存储里那个 commit `commit` 的样子。checkpoint 记录两次：

- 一次在 **session 开始**（`after == 0`）—— 原始树，这样回溯到第一条消息时有
  东西可恢复；
- 一次在**每个回合末尾**（`after == history.len()`）—— 在任何排队中的后续回合
  又开始改动之前捕获，所以它就是刚结束那个回合的代码状态。这正是之后回溯到
  *下一条*消息、或一次 resume 所想要的东西。

两种倒回都汇聚到一个纯的决定 `checkpoint::restore_target`：

```
restore_target(checkpoints, target_len) = the commit of the checkpoint with the
                                           greatest `after <= target_len`
```

- **回溯**到历史位置 `P` 的一条用户消息，先把历史截断到 `P`，再恢复
  `restore_target(cps, P)` —— 上一个回合末尾拍下的那张快照（它的
  `after == P`），对第一条消息来说则是 `after == 0` 的原始快照。
- **`/resume`** 恢复 `restore_target(cps, usize::MAX)` —— 最后一张快照，即保存的
  那段对话结束时的代码。

一次截断还会丢掉描述被倒掉的那段未来的 checkpoint
（`checkpoint::retain_surviving` —— 保留 `after <= len`），与 rollout 文件的截断
重写步调一致。

### 为什么以 `after` 为键，而不是以条目为键

以历史*长度*为键、而不是以某个具体条目为键，让截断的算术变得平凡（一个 `<=`
过滤），并让一个函数同时服务两种倒回：resume 不过是 `target_len = usize::MAX`
的回溯。它也意味着某个回合被倒掉之后，那个 checkpoint 就被丢弃、永不复活 ——
resume 的 rollout 文件对它的 transcript 条目本就守着同一条不变量
（`docs/state/resume.md`）。

### 回合末尾的快照跑在 worker 线程上

回合末尾的快照是对整个工作目录做 `git add -A` —— O(bytes)，在一个普通项目上
以百毫秒计，在冷的或大的存储上以秒计。它原先跑在事件循环线程上，于是每个回合
末尾都把 UI 冻住那么久。现在它跑在自己的线程上：

- `dispatch_after_turn` 启动快照（`Session::start_checkpoint_snapshot`）并完成
  它那些与 checkpoint 无关的活 —— 关闭 tool 组、落定 notice、退役任务、取走
  完成板、清掉回合句柄。
- **回合之后的派发** —— 下一个排队的回合，或关于一次模型从未听说的完成的自动
  回合 —— 等结果落地，而不是立即运行，于是没有任何东西能在刚结束的状态被捕获
  之前改动那棵树。在快照进行中落地的直接提交（`Submit`、一个 `!command`）以
  同样的方式排队（`App::queue_messages` / `App::queue_shell_command`）；结果
  一到它就开始。
- 结果（`CheckpointDone`）经一条事件循环会 select 的通道回来。只有它仍属于这段
  对话时才会被记录：同样的 `after`、与启动时相同的**倒回 epoch**，以及不短于
  它的历史。`/clear`、`/resume` 和一次回溯都加入待定的 worker —— 它的 git 不能
  与恢复的 git 在同一个私有存储上抢跑 —— 并推高 epoch、忘掉待定的派发
  （`Session::invalidate_checkpoints`），于是一张来自已不存在的对话的快照被
  丢弃，而不是被记录。
- 退出 TUI，以及一次在中断下结束的 headless 运行，最多等五秒拿一个待定的结果
  并记录它 —— 绝不派发那个被推迟的回合。一次普通的 headless 运行把待定的快照
  当作「不空闲」，所以它会记录它，然后才退出。

**session 开始**的那张快照（`after == 0`）仍在第一帧之前同步运行：它的成本守卫
（`cwd_scope` 加快照预算）已经给它划了界，而回溯到第一条消息必须能在链上找到它。

计时不用调试器就能量：`ZEROTH_TIMING=<path>`（或 `=1` 表示
`<tmp>/zeroth-timing.log`）会追加 `turn-end`、`checkpoint.snapshot` 与
`checkpoint.dispatch-wait` 三类行。TUI 写文件而不是写 stderr，于是一行日志绝无
可能弄坏 inline viewport。

## 两层

按这个仓库惯常的分法拆开 —— 一个**纯核心**（单元测试）和一个**边界**（git I/O，
集成/smoke 测试），两者都在 [`src/checkpoint.rs`](../../src/checkpoint.rs)。

### 纯核心（`checkpoint`）

- `Checkpoint`、`restore_target`、`retain_surviving` —— 上面那张映射。
- `store_git_dir(root, cwd)` —— 按 cwd 划分的存储路径，以 cwd 为键、把每个非
  字母数字字符换成短横线（`/home/user/proj` → `-home-user-proj`），于是在同一个
  目录里跑的所有 session 共用一个对象存储：上周记录的 checkpoint 今天仍可恢复。
- `CHECKPOINT_EXCLUDES` / `exclude_file_contents` —— `info/exclude` 黑名单。
  项目*自己的* `.gitignore` 已经把大多数构建/vendor 目录挡在 `git add -A` 之外
  （影子存储像任何 git 命令一样读它们）；这是给没有 `.gitignore` 的目录兜底，
  而最重要的，是给用户真正的 `.git` 兜底 —— 一次恢复的 `git clean` 绝不许删掉
  它。
- `enabled_by_env` —— `ZEROTH_CHECKPOINTS` 闸门（`ZEROTH_TOOLS` 那套模式：
  `0`/`false`/`no`/`off` 为关）。
- `cwd_scope(cwd, env)` / `CheckpointRefusal` / `CheckpointEnv` —— 下面那个
  **项目范围守卫**。
- `SnapshotBudget` / `SnapshotCost` / `parse_size_limit` —— 下面那个**成本
  上限**。

## 这个目录值得快照吗？

session 开始的快照跑在**第一帧画出之前**，处在 raw 模式里 —— 那里 Ctrl+C 只是一
个没人读的按键事件 —— 而 `git add -A` 是 O(bytes)，因为它要给每个暂存的文件算
哈希。在本仓库的硬件上实测：

| 工作目录 | 到第一帧 | 一次启动后的存储 |
| --- | --- | --- |
| 一个普通项目 | 86 ms | 180 kB |
| 本仓库（5.3 MB） | 368 ms | 3.2 MB |
| 一棵 235 MB / 6 000 文件的树 | **9 754 ms** | **260 MB** |

所以有两条守卫，在那一切运行之前就决定一个目录到底该不该快照。两条都是纯的；
边界把它们的度量对象注进去。

### 分类那一半 —— `cwd_scope`

有些目录不是项目，而成本只是其中一半的理由：在它们里面做一次恢复的
`git reset --hard` + `git clean -fd`，删掉的是从来不属于这个 session 的文件。
以下各有一条 `CheckpointRefusal`，一律拒绝：

- **`FilesystemRoot`** —— `/`、某个盘符根，或一个空的、无从知晓的 cwd。
- **`HomeDirectory`** —— 家目录本身或它的某个祖先（`/home`）：用户的整棵树
  （最早那个「zeroth 在 `~` 里挂住」的 bug）。
- **`StateDirectory`** —— zeroth 自己的 `~/.zeroth` **以及它底下的一切**：那里
  装着各份 rollout、输入历史，以及每个项目的 checkpoint 存储，所以在那里做一次
  恢复的 `git clean -fd` 会删掉其他 session 的记录。一个只是*包含*本 session
  存储的 cwd **不**被拒绝 —— 见[自我排除那一节](#存储从不快照它自己)，那才是
  对这个方向的更好回答。
- **`SystemDirectory`** —— 一个 `SYSTEM_TREES` 条目（`/proc`、`/sys`、`/dev`、
  `/run`）**或它底下的任何东西**，因为这些根本不是普通文件系统，而 `/dev/shm`
  与 `/run/user/N` 里装着其他*活着的*进程的文件；或一个 `SYSTEM_ROOTS` 条目
  （`/usr`、`/etc`、`/root`、`/bin`、`/boot`、`/lib*`、`/sbin`，以及 macOS 的
  `/System`、`/Library`、`/Applications`）**本身、仅它自己** —— 整个 `/usr` 不是
  谁的项目，但人们真的会在 `/usr/local/src` 里放工作、给 `/etc/nginx` 做版本
  管理，而拒绝一个合法项目是那个更糟的默认值。
- **`SharedDirectory`** —— 一个 `SHARED_PARENTS` 条目（`/tmp`、`/var/tmp`、
  `/var`、`/opt`、`/srv`、`/mnt`、`/media`、`/home`、`/net`、`/export`、
  `/Users`、`/Volumes`，以及 macOS 的 `/private/tmp`、`/private/var`）或
  `$TMPDIR`，且**只限目录本身**：`/tmp` 装着这台机器上每个程序的临时文件，而
  `/tmp/my-project` 是一个普通项目，照旧得到 checkpoint（smoke 套件的
  `mktemp -d` 工作目录就靠这一点）。`$TMPDIR` 要注入，因为 macOS 把它放在一个
  不可预测的 `/var/folders/xx/yyy/T`。

匹配自始至终按路径分量进行，所以 `/home/user` 的兄弟目录 `/home/username` 照样
得到 checkpoint，而尾随斜杠永远不影响结果；一个注入的空路径算作未知，而不是算作
一切的前缀。

### 存储从不快照它自己

那 2 GB `~/.zeroth` 的另一半，而且是*bug* 而非范围问题的那一半。当 checkpoints
根落在工作树**之内**时，每一次 `git add -A` 都会把上一批快照的对象文件重新暂存
一遍，于是被跟踪的集合一回合接一回合地复利增长 —— 在一个 40 MB 的状态目录上
实测：四次快照之后跟踪文件数 59 → 130 → 269 → 528，存储 41 MB → 165 MB。

`CHECKPOINT_EXCLUDES` 覆盖不了它：那些是*名字*模式，而根是
`ZEROTH_CHECKPOINTS_DIR` 指向的任意路径。所以只要根按前缀剥离后落在工作树之内，
`store_exclude_line(work_tree, root)` 就多派生一行 `info/exclude` —— 用前导 `/`
**锚定**，让它匹配工作树根上的那个目录，而不是项目深处某个同名的目录；四个
gitignore 元字符被转义，于是一个字面路径保持字面（放在 `ck[1]` 里的存储必须排除
`ck[1]`，而不是 `ck1`）。`CheckpointStore::new` 计算它一次，`write_excludes`
把它追加进去。

这就是为什么一个包含存储的 cwd *不*被拒绝：为了一个配置选择把整个特性拿掉，
比简单地不暂存那个存储更糟。这也是一层纵深防御 —— 即使某条范围规则哪天被漏掉，
存储也无法再自我复利。

### 通用那一半 —— `SnapshotBudget` 与飞行前探测

没有任何黑名单能点名那个*本身就是*项目的巨大目录。于是凡是活过 `cwd_scope` 的
都要被度量：`CheckpointStore::probe` 用
`git ls-files --others --exclude-standard -z` 问出 `git add -A` 将会新暂存的
那些路径，并把它们的 `stat` 尺寸加起来。

- 它**遵守 `.gitignore`** 与存储的 `info/exclude`，于是一个大头被 gitignore 掉
  的仓库绝不因为快照根本不会携带的重量而被拒。这也是它去问 git 而不是自己走树
  的原因。
- 它**比它所预测的那件事便宜约 800 倍** —— 在一棵实测 470 MB 的树上，26 ms
  对 20 354 ms —— 因为 git 只走目录、只 stat，从不读内容。
- 它**受三个方向约束**（`max_files`、`max_bytes`、`max_time`），任意一项触线就
  立刻杀掉子进程，于是一棵病态的树花掉的是预算，而不是整趟遍历。
- 在一个**热**存储上它自然只报告新东西 —— 那正是这次快照将要哈希的东西，于是
  一个很大但已被捕获过的树能跨 session 保住它的 checkpoint。

越过预算之后，存储在这个 session 里被退役（`CheckpointStore::disable` —— 退役
的是能力，不只是那个开启标志，所以 `/settings` 报告该行为不可用，而不是提供一个
会重新装上一次实测过的卡顿的开关）。

默认值是 **20 000 个文件**和 **256 MiB**，探测期限 500 ms。字节上限刻意不按
时间来标定 —— 哈希吞吐在不同磁盘之间能差 ~20 倍。它说的是项目是什么：一棵持有
超过这么多非忽略内容的工作树，是数据目录、媒体目录或临时目录，而每回合的整树
快照对其中之一都是错的工具，无论磁盘多快。副作用是在普通硬件上冷快照约一秒、
在最慢的硬件上要几秒 —— 这是不拒绝真项目所付的价，而拒绝才是两种失败里更糟的
那种。

*时间*不够有它自己的结论（`ProbeOutcome::OutOfTime` →
`CheckpointRefusal::TooSlow`），绝不并入「太大」：一次超时的探测对树有多大
一无所知，把它勉强数到的少数文件当作理由报出来，那是撒谎。

两个上限都可覆盖，各自接受一个纯数字或一个 `k`/`m`/`g` 后缀，并且各自把 **`0`
当作不限**（`/settings` 的 **Max tool calls** 那个约定）：

```
ZEROTH_CHECKPOINT_MAX_FILES=50000
ZEROTH_CHECKPOINT_MAX_BYTES=2g
ZEROTH_CHECKPOINT_MAX_BYTES=0     # no byte ceiling
```

### 把它说出来

每一次拒绝都在第一帧弹一个单行 toast `Checkpoints off — {reason}`
（`CheckpointRefusal` 的 `Display`），而 `/settings` 把 **Checkpoints** 显示为
`false (unavailable)`。当用户此前已经把 checkpoint 关掉时，这个 toast 被抑制
—— 一次拒绝只有在它拿走了什么东西时才是新闻。悄悄自我禁用正是这件事难定位的
原因：「zeroth 在 `/tmp` 里启动要好几秒」和「checkpoint 在这里什么都不做」本是
同一个事实的两面。

拒绝 toast 有一个兄弟，属于那张**确实**跑了的快照：飞行前**公告**。session 开始
的快照在第一帧画出之前哈希整个 cwd，于在一棵又大又冷的树上，终端会冻住好几秒，
而没有任何东西说明为什么。`seed_checkpoints` 现在接住探测的 `SnapshotCost`
（它以前直接喂进 `.refusal()` 就把数字扔了），并在树装得下预算时提交纯的
`checkpoint::snapshot_notice` 那一行 ——
`Snapshotting 326 files (7.9 MB) for checkpoints…`，用 `human_bytes` 和拒绝
信息自己那套复数规则，好让这一对读起来像一家 —— 经
`ui::startup_notice_lines`（横幅的缩进与暗色 meta 颜色）写出，并在
`snapshot("session start")` 阻塞之前**强制画一帧**（`draw_conversation`）：
`insert_before` 只是入队，而循环的第一次绘制 tick 在快照的另一侧。因为它提交在
`paint_first_frame` 的横幅之前，这一行落在 scrollback 里**横幅之上**。它是装饰
（从不进 `history`，从不会被 purge 重建重新发出 —— 一条一次性的启动事实，所以
它在第一次 resize 或 `/clear` 时消失，刻意不同于横幅的 `banner_tail`），并且
以探测的 `files > 0` 为条件：一个没有新东西的热存储 —— 每一次普通重启 —— 探测
为零，保持安静，不多花一帧（`probe_shrinks_once_the_store_is_warm` 正是让那条
判据诚实的东西）。回合末尾的快照不公告：热存储只哈希变了的东西，而点名它会多
付一次每回合的探测。

### 边界（`CheckpointStore`）

一个隔离的 git 对象存储。每条 git 命令都带着一个私有 `GIT_DIR`（在 checkpoints
根之下）、以 cwd 作为它的**分离工作树**、一个存储本地的索引
（`GIT_INDEX_FILE`），以及完全与用户系统/全局 git 隔离的配置
（`GIT_CONFIG_NOSYSTEM`，一个指向不存在的存储本地文件的
`GIT_CONFIG_GLOBAL`），无提示，所有 stdio 都分离。

- **`probe(budget)`** —— 下一次快照要花多少，不花这笔钱就量出来（见上）。在
  被禁用的存储上是惰性的，在跑不了 git 的任何失败上则从宽：探测绝不因自己的
  不确定而关掉这个特性。
- **`disable()`** —— 在这个 session 里退役存储（探测的裁决）。与
  `set_enabled(false)` 不同，它丢掉的是**能力**，所以 `/settings` 报告该行
  不可用。
- **`init`** —— `git init --initial-branch=checkpoints`（固定分支，于是无论
  用户的 `init.defaultBranch` 是什么，`HEAD` 对第一次提交都有效）、写
  `info/exclude`，
  并设置存储本地配置：一个身份（好让 `commit` 无需用户的身份就能工作）、
  `commit.gpgsign=false` + `core.hooksPath=/dev/null`（绝不在 hook 或签名上挂住
  或 fork）、以及 `gc.auto=0`（让每一张快照都能跨过恢复所做的那次 reset 保持
  可达）。幂等。
- **`snapshot(msg) -> Option<String>`** —— `git add -A`（遵守工作树的
  `.gitignore` 与我们的 `info/exclude`），然后 `git commit --allow-empty
  --no-verify`（允许空提交，于是一个什么都没变的回合仍映射到一个可恢复的独立
  SHA），返回 `HEAD`。在禁用/失败时返回 `None` —— TUI 绝不因为一个 checkpoint
  而死。
- **`restore(commit) -> io::Result<bool>`** —— 先验证该 commit 在这个存储里存在
  （`cat-file -e`；一个记录在*另一个* cwd 里的 session 是未知的 → 恢复是一个
  优雅的 `Ok(false)`），然后 `git reset --hard <commit>`（把被跟踪的文件带回
  快照）加 `git clean -fd`（删掉此后新建的文件，遵守排除规则好让 `.git`/vendor
  目录活下来）。成功时 `Ok(true)`。

> 一个测试锁住的、不显然的 git 细节：`git()` 会把命令的 stdout 预设为 null
> （对 `status()` 那些众多调用者是对的），而 `Command::output` 会**保留**显式的
> stdout 设置 —— 所以 `head()` 必须重新把 stdout 接成管道，否则 SHA 被悄悄吞掉，
> 每张快照都返回 `None`。

## 接线（`main.rs`，即边界）

- **存储 + 原始快照**在 `SessionRecorder` 旁边创建（`tui::bootstrap`），由
  `checkpoint::enabled_by_env(ZEROTH_CHECKPOINTS)` **且**
  `checkpoint::cwd_scope(cwd, env)` —— 就是上面那条分类守卫，喂给它 `$HOME`、
  `config::config_home()`、`config::checkpoints_root()` 与 `config::tmp_dir()`
  —— **且**存在一个 `git` 二进制（最后才探测，于是一个被拒的 cwd 连 git 都不会
  spawn）三者把关。根：`ZEROTH_CHECKPOINTS_DIR`，否则 `~/.zeroth/checkpoints`
  （`ZEROTH_SESSIONS_DIR` 那套模式；smoke 测试把它指向一个临时目录）。
- **飞行前探测**随后在 `seed_checkpoints` 里跑，位于 `init` 与原始快照之间 ——
  那是第一帧之前、这个特性仍能决定不运行的最后一个时刻。超预算时它调用
  `disable()`、重新同步 `SettingAvailability`、toast 出原因，不做任何快照。
- **回合末尾的快照**搭乘 `dispatch_after_turn` —— 每个回合末尾都汇流经过的那个
  唯一收窄点（`StreamDone`/`Error`、Esc 中断、一次空闲的后台完成）—— 经
  `checkpoint_turn_end`，在下一个排队的回合派发*之前*。于是哪怕背靠背排队的
  回合，各自也得到自己的 checkpoint。
- **记录。** `SessionRecorder` 长出了一个 `checkpoints` 内存镜像加一个
  `checkpoints_written` 水位（`recorded` 的孪生）。
  `record_checkpoint` 把一张快照留在内存里；下一次 `sync` 把它和条目行交错冲刷
  出去 —— 但单独一个 checkpoint **绝不物化出一个文件**，于是一个空 session
  （只有启动快照）仍然不留 rollout 文件（codex 的延迟创建）。一次截断重写会
  重新发出幸存者；`adopt`（resume）接管文件自己的 checkpoint；`start_new`
  （`/clear`）清掉它们，并由一张新的原始快照重新播种这条链。
- **恢复的触发点。** `ResumeSession` 分支解析文件里的 checkpoint
  （`session::parse_checkpoints`）、备份当前树，并在 purge 重绘之前恢复最后
  一个。
  新的 `ConfirmBacktrack` action（回溯预览的 Enter 分支现在返回它，而不是
  `ToggleToolView`）在**purge 重建**的重绘之前恢复
  `restore_target(recorder.checkpoints(), history.len())`（回溯让历史变小，所以
  像 `/resume` 一样，就地覆写会把被丢掉的往返留在 scrollback 里发馊；
  `docs/input/backtrack.md`）；随后循环底部的 `recorder.sync` 重写文件，丢掉
  那些被倒掉的 checkpoint 行。

## rollout 那一行（`session`）

checkpoint 搭的是与 transcript **同一个** JSONL 文件，表现为一行
`{"type":"checkpoint","payload":{"commit","after"}}`
（`session::checkpoint_line`）。它们**对 transcript 的解析不可见**
（`parse_session` 跳过它们，于是一段恢复的对话在有 checkpoint 与没有时逐字节
相同），由专门的 sidecar `parse_checkpoints` 读取。旧版本跳过这个未知的记录
类型 —— 这就是向前兼容的约定。

## 启用 / 禁用

**关着，直到某个目录把它打开**：`/settings` 的 **Checkpoints** 旋钮是按工作
目录的（`docs/state/per-directory-state.md`），默认为 `false` —— session 开始
的快照是在第一帧之前对整个 cwd 做 `git add -A`，这种事不该在你没为它选择过的
树里跑。在某个目录里把旋钮拨开，就在那里记下 `"checkpoints": true`；
`ZEROTH_CHECKPOINTS=1` 为一次运行打开它而不保存任何东西（`=0`，或
`false`/`no`/`off`，则是关）。两种方式下，它都只在存在 `git` 二进制**且 cwd
是一个值得快照的项目**（上面那条守卫的两半）时才运行；缺 git 或没有可写的根都
会悄悄禁用它。

两条守卫都是**无条件的** —— 显式的 `ZEROTH_CHECKPOINTS=1` 也覆盖不了任何一条，
因为它们回答的是「这次运行能不能不弄坏 session」，而不是偏好。有旋钮的是成本
上限：抬高或去掉 `ZEROTH_CHECKPOINT_MAX_FILES` /
`ZEROTH_CHECKPOINT_MAX_BYTES`，就可以给一棵超过默认预算的树做 checkpoint。对
分类性拒绝来说，答案是在一个项目目录里运行。

被禁用的存储让每个操作都是惰性的空操作，于是 `/resume` 与回溯的表现与这个特性
出现之前完全一样 —— 只有 transcript。

> **跑测试套件时：** `scripts/smoke.sh` 在*本仓库的工作目录里*驱动真实二进制，
> 而一次恢复的 `git clean` 会删掉没被快照捕获的文件。所以套件对每个 repo-cwd
> 阶段都用 `ZEROTH_CHECKPOINTS=0` 跑，只在那些跑在一次性的临时目录里的阶段
> （Phase 46–47、49、71）启用 checkpoint。你加做 resume/backtrack 的阶段时，
> 请保持这层把关。

## 已知限制（v1）

- **整 cwd 快照。** 一个 checkpoint 捕获整个工作目录（减去排除项），所以一次
  恢复会回退那个时点以来的*所有*改动，包括你手工编辑过的文件 —— 这就是这个
  特性得名于之的那个「git reset」语义。恢复前备份 + `git reflog` 是安全网。
  外科手术式的、只恢复 agent 动过的东西，是未来的工作。
- **同步 git —— 每个目录一次慢的首次快照。** session 开始的快照在第一帧之前的
  循环线程上跑，回合末尾的快照则跑在空闲边界上。只有在一个目录里的**第一次**
  运行付出真实时间；热存储的 `git add -A` 只 stat、不哈希。在本仓库的硬件上实测
  （一台虚拟机，它的 git 以约 23 MB/s 哈希，约是一个 SSD 的十分之一）：

  | 工作树 | 启动 1 | 启动 2 | 启动 3 |
  | --- | --- | --- | --- |
  | 本仓库（5.3 MB） | 368 ms | ~80 ms | ~80 ms |
  | 一个 201 MB 的项目 | 10 134 ms | 71 ms | 81 ms |

  所以字节上限才是决定那一次启动能坏到什么程度的旋钮，而它不能按时间来标定：
  256 MiB 在这台机器上要十秒，在 SSD 上不到一秒。慢存储上请调低
  `ZEROTH_CHECKPOINT_MAX_BYTES`。唯一能*消除*这次停顿、而不只是给它划界的办法，
  是把冷快照挪到 worker 线程（Ctrl+V 图片流水线的形状）—— 那仍是未来的工作，
  而且要小心：后台的 `git add -A` 绝不能与一次恢复的 `git reset --hard` 重叠。
  *病态*情形由 `cwd_scope` 与探测直接拒绝、而不是为它停顿，那些拒绝不依赖这个
  上限。
- **不剪枝。** `gc.auto=0` 让每张快照都能跨过一次恢复所制造的那次分叉保持可
  恢复，于是存储缓慢而无界地长大。保留策略是未来的工作。
- **按 cwd 划范围。** checkpoint 以工作目录为键；恢复一段记录在*另一个*目录里的
  session 找不到匹配的 commit，于是不去动代码（一个优雅的空操作，而非错误）。
- **没有直接浏览/恢复 checkpoint 的 UI** —— 它们只由 resume/回溯手势驱动。恢复
  前的备份只能通过存储的 `git reflog` 触达。

## 测试

- `checkpoint`（纯）：`restore_target` 挑出倒回点或其之前最新的那张快照（
  `usize::MAX` = 最后一张）、在并列时取更近的那张、没有合格者时为 `None`；
  `retain_surviving` 丢掉截断点之后的快照；`store_git_dir` 像 tasks 目录那样把
  cwd 里的字符换成短横线；`exclude_file_contents` 始终护住 `.git`；
  `enabled_by_env` 认得那些表示关闭的词；`cwd_scope` 拒绝家目录（含尾随斜杠的
  `$HOME`）、它的祖先、文件系统根、两个方向的状态目录、每一个系统树*及其子树*、
  以及每个共享父目录（外加 `$TMPDIR`）*仅其本身* —— 同时接受项目目录，包括一个
  `/home/username` 兄弟目录（按分量匹配，不是前缀）、`/tmp/scratch`，以及一个
  在 `$TMPDIR` 下的 `mktemp -d`；注入的空路径算作未知；一个同时装着存储的共享
  父目录报告为共享。`SnapshotBudget::exceeded` 对任一上限报警，并把 `0` 当作
  不限；`parse_size_limit` 读纯数字与 `k`/`m`/`g` 后缀、保留 `0`、对垃圾输入
  回退；每个 `CheckpointRefusal` 都渲染成一行。
- `session`（纯）：一行 `checkpoint` 经 `checkpoint_line` /
  `parse_checkpoints` 按文件顺序往返一致，而 checkpoint 行对 `parse_session`
  不可见。
- `tests/checkpoint_store.rs`（真 git）：在编辑/新建/删除之后，恢复一张更早的
  快照把树一模一样地还回来（编辑被回退、新文件被删、删除被撤销）；恢复绝不删除
  用户的 `.git` 或 vendored 目录；未知 commit 是一个优雅的空操作；被禁用的存储
  是惰性的。探测方面：它数出快照将会暂存的文件及其字节，对 `.gitignore` 掉的
  与 `info/exclude` 掉的树视而不见，越过文件上限就提前停下，能抓住少数几个单靠
  文件上限会放行的巨无霸文件，在存储变热之后收缩到只剩新文件，被禁用时惰性。
- `scripts/smoke.sh` Phase 46–47（真实二进制，在临时 cwd 里）：一次 Esc-Esc
  回溯把一个被 `!` 改过的工作文件还原到它的原始 checkpoint；一次 `/resume` 把
  一个工作文件还原到保存的 session 的最后一个 checkpoint，哪怕它在两次启动之间
  已在磁盘上分叉。Phase 49：以 cwd == `$HOME` 启动（哪怕带着显式的
  `ZEROTH_CHECKPOINTS=1`）时存储从不被创建，而在同一个 home 下的一个项目目录
  仍然得到快照。Phase 71：在 `/tmp` 或状态目录里启动从不创建存储，并 toast 出
  原因；一棵超过被强行调小的 `ZEROTH_CHECKPOINT_MAX_BYTES` 的树 toast
  「too big to snapshot per turn」，并让存储里没有 commit；同一个目录在默认预算
  下做快照且无 toast。

[`HistoryItem`]: ../src/app/types.rs
