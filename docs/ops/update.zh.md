# 更新提示 —— 每天一次的发布检查，以及 `zeroth update`

**每天一个请求，有新发布时横幅下面一张卡片，以及一条装上它的命令。** 用一行安装
器（`docs/ops/release.md`）装上的二进制背后没有包管理器告诉它有发布出货了，所以
应用自己去问仓库 ——
`HEAD https://github.com/yxxbc/zeroth/releases/latest`，每个 UTC 日一次，
在第一帧之后 —— 从它落到的那个重定向上读出 tag，而当那个 tag 比
`CARGO_PKG_VERSION` 新时，在遥测卡片的框里**每天一次**这么说：

```
  ╭─ Update available ───────────────────────────────────────────────────────╮
  │  Zeroth v0.2.0 is out — you are running v0.1.0.                      │
  │  What's new https://github.com/yxxbc/zeroth/releases/tag/v0.2.0    │
  │  Update run zeroth update, then restart.                             │
  │                                                                          │
  │  Turn off /settings → Update check or ZEROTH_UPDATE_CHECK=0          │
  ╰──────────────────────────────────────────────────────────────────────────╯
```

然后 `zeroth update` 做安装：同一次检查，以及 —— 当有更新的发布存在时 —— 把
一行的 `install.sh` 取到临时文件，并在运行中二进制所在的那个目录上跑它，钉在检查
刚解析出的版本上。纯的那一半是 `src/update.rs`；边界是 `src/tui/update.rs`（那次
检查、那张卡片、那个文件）和 `src/tui/update_cli.rs`（那个子命令）；设计是
`tui::telemetry` 的，一条规则一条规则地照搬，所以两个碰网络的每日一次的功能读起
来一个样子。

面向用户的陈述 —— 这个请求携带什么、怎么关掉它 —— 是根目录 `TELEMETRY.md` 的
*Update check* 一节，它承诺是离开一台机器的所有东西的完整清单，所以也必须点名这
个请求。改一个，就重读另一个。

## 发出去什么

每个安装每个 UTC 日一个 `HEAD` 请求，发往 `{repo}/releases/latest`，携带：

| 头部 | 值 |
|---|---|
| `User-Agent` | `zeroth/{version}` —— 没有它 GitHub 会拒绝请求 |

这就是整个请求：**没有请求体，没有安装 id，没有查询串**。它就是浏览器打开
releases 页面时发的那个请求，少了页面。回来的是一个 `302` 到
`/releases/tag/vX.Y.Z`；客户端跟着它走，`update::version_from_release_url` 从它
落到的那个 URL 上读出 tag（`response.url()` —— 查询串和 fragment 去掉，`v` 去
掉，剩下的必须解析成一个版本）。正文从不读。

有三件事它刻意不是：

- **不是 REST API。** `api.github.com/repos/…/releases/latest` 回答一份 JSON 文
  档，里面每个 asset 和它的上传者，按地址限流每小时 60 个未认证请求 —— 一个共享
  办公室中午就超了 —— 而正文还得为一个字符串去解析。重定向在它的 `Location` 头部
  里带着同一个字符串，而且不会那样被限流。它还保住了内存姿态：我们只从一个正文里
  读一个字段，就不留它的 `Value` 树（`docs/architecture/memory.md`）。
- **不是一次 ping。** 没有任何东西标识这个安装，这个项目也不记录任何东西 ——
  GitHub 看见这条连接，就像它看见一次页面加载，仅此而已。这也是 `DO_NOT_TRACK`
  **不**覆盖它的原因：那个约定是关于分析的，而这个什么都没测量。它有自己的一套开
  关（见下），因为一个用户没有输入的、发往 github.com 的请求，仍然是一个用户可能
  不想要的请求。
- **不是一次下载。** 检查不取任何二进制；只有 `zeroth update` 会，而且只在被
  要求时。

## 什么时候，以及它花什么

- **启动时，第一帧之后**（`Session::bootstrap` 在 `start_telemetry` 之后调用
  `start_update_check`，两者都在 `paint_first_frame` 后面），在一个 detached 的
  worker 上（`workers::spawn_update_check`）。应用从不等它。
- **任何开启新 UTC 日的回合开始时**（`Session::update_day_check`，就在
  `telemetry_day_check` 旁边）：一个跨过午夜还开着的 session 在新的一天也会检
  查。
- **每天一次，不管请求发生了什么。** 那次尝试的日子在*发起时*就记进
  `update.json`（`record_check(today, None)`），不是在答案到达时，所以一个死掉的
  网络、一个还没有发布的仓库、或者一个拒绝的代理，每个安装每天只花一个请求，而用
  户什么也看不到 ——他们也没有什么可做的。session 另外记住它试过的那一天（
  `Session::update_attempted`），所以同一天的回合开始只花一次日期读取和一次字符
  串比较。
- **worker 报告，循环写入。** worker 在它自己的 `select!` 通道（`update_rx`）上
  把它找到的版本送出去，`Session::on_update_result` 记录它 —— `update.json` 有一
  个写入线程，`/settings` 的切换永远赛不过它。每次写入都是
  `config::update_update_file` 的读-改-写，所以一个在用户关掉检查时被记录的结
  果，不可能把他们的 `true` 复活。

## 那张卡片

`ui::update_notice_lines(width, current, latest, repo, command)` 在遥测卡片的框
里渲染 `update::notice` 的文本 —— 两者都经由
`ui::notice_card_lines(heading, body, width)`，那个圆角的 `DEVICE_BOX_*` 框，标
题用头部强调色，正文按框换行，`**bold**` 标签和强调色代码片段还是 `ui::inline`
早就在给回复的那些 —— 所以横幅下面那两张卡片是一个设计。正文说四件事就停：发布与
运行中的版本、发布页（`update::release_page_url`，在会画它们的终端里是一个真链接
 —— `docs/ui/links.md`）、装上它的命令，以及关闭开关。那条命令是
`zeroth update`，除非某个 JavaScript 包管理器装了这个二进制：那就用那个管理
器自己的 —— `npm install -g @linuztx/alter-zero@latest` 之流，
`update::update_command` 作用在 `update::package_manager(current_exe)` 上 —— 因
为 `zeroth update` 拒绝这样的安装。

有三条规则决定它*何时*显示：

- **每天一次。** `update::notice_due(file, current, today)` 只在文件的 `latest`
  比这个构建新**并且** `notice_day` 不是今天时，才回答要宣告的那个版本。一个已经
  显示过卡片的启动明天会再显示，不是每次启动都显示 —— 一条你下一分钟无法行动的提
  示，是你不再读的那种。
- **第一帧之后，绝不在回复内部。** 一个在回合流式输出时落地的结果会与模型正在产
  出的文本交错，所以它在 `Session::update_notice_pending` 里等着，而
  `Session::flush_pending_update_notice` —— 从 `on_update_result` 和循环底部（
  `after_iteration`）跑 —— 在第一个空闲时刻提交它：没有回合在飞、没有东西在流
  式、允许提交（不变量 4，agent 视图的规则）。提交之前它重读文件、重问
  `notice_due`：日子可能已经翻了，或者另一个启动在结果排队之后已经显示过卡片。
- **是外壳，绝不是 `history`。** 卡片像横幅和遥测通知一样骑 `insert_before`；一
  次 purge 重建（一次 resize、`/clear`）不会重新发出它。它是一条通知，不是一条消
  息。

一个已经知道有更新发布的启动（文件里昨天那次检查的 `latest`）会在发起今天的请求
之前先从文件里宣告它，所以卡片不等网络 —— 而一个*领先于*最新发布的构建（一个
checkout）看不到卡片，因为 `is_newer` 是一次比较，不是一个差值。

## 版本

`update::parse_version` 读 `MAJOR.MINOR.PATCH[-pre][+build]`，带不带 tag 的 `v`
都行，其他一切拒绝（一个 `nightly` tag、`0.1`、一个前导零），好让一个最新发布不
是版本的仓库被比较成*不是更新*，而不是*更新* —— 垃圾永远不算一次更新。`Version`
的 `Ord` 是 semver 的：数字字段按数字比（`0.10.0 > 0.9.0`，字符串比较会搞错），
预发布比它的正式发布**更旧**（`0.2.0-rc.1 < 0.2.0`，于是正式发布取代它自己的候
选），预发布标识符按点切分、两边都是数字时按数字比，构建元数据忽略。

## `update.json`

`telemetry.json` 旁边的**按用户**文件（`docs/state/per-directory-state.md` —— 一
个你在一个目录里关掉、另一个目录里没关的检查会是个意外，Telemetry 的规则）：

```json
{
  "enabled": true,
  "last_check_day": "2026-09-12",
  "latest": "0.2.0",
  "notice_day": "2026-09-12"
}
```

| 字段 | 它是什么 |
|---|---|
| `enabled` | `/settings` 的 **Update check** 行的值；手动 `false` 与循环切换它是一样的 |
| `last_check_day` | 最后一个*尝试过*请求的 UTC 日 —— 在发起时设定 |
| `latest` | 上一次成功检查找到的最新发布，好让一次启动不必等网络就能宣告它 |
| `notice_day` | 卡片最后一次显示的日子 —— 提示本身每天一次的那个界 |

删掉这个文件会重置全部四个：检查是开着的，下次启动会再问，而一个更新的发布会再被
宣告一次。

## 关掉它

| 怎么关 | 范围 |
|---|---|
| **`/settings` → Update check** | 永久的，对你的用户，在每个目录 |
| **`ZEROTH_UPDATE_CHECK=0`** | 那一次运行（`0`/`false`/`no`/`off`；`=1` 为一次运行打开它） |
| **没有配置目录** | 什么都不跑 —— 没有地方记住那一天，而每次启动都检查一次就不是每天一次 |

这个变量**撤掉 `/settings` 那一行**，而不是给它播种 —— `false (unavailable)`，拒
绝循环切换 —— Telemetry 的规则（`config::update_check_forbidden_by_env` →
`SettingAvailability::update_check`）：一个按键就能撤销的退出，不算退出。
`ZEROTH_UPDATE_URL` 把这次检查（以及 `zeroth update`）指向另一个仓库根
 ——一个 fork，或者 smoke 套件的替身服务器 —— 而 `smoke.sh` 的基础环境设
`ZEROTH_UPDATE_CHECK=0`，好让其他任何 phase 都不会够到 github.com。

## `zeroth update`

这个子命令在 `mcp` 所在的同一个 TUI 之前边界上路由（`cli::parse` →
`Cli::Update`、`tui::startup::resolve_cli` → `tui::update_cli::run`，
`docs/cli/cli.md`）：第一个参数是 `update`，不带任何东西 ——
`zeroth update now` 是一个用法错误，不是一次猜测。它在 runtime 和终端存在之
前、在 cooked 模式下跑，边跑边打印，而当二进制已经是最新发布或安装器跑完时以 `0`
退出，在它无法检查、取回或安装时以 `1` 退出。

```
$ zeroth update
Checking https://github.com/yxxbc/zeroth for a newer release…
zeroth v0.2.0 is out — this is v0.1.0. Installing it over /home/me/.local/bin.
  …the installer's own output…
```

它做什么，按顺序：

1. **拒绝一个构建目录。** `update::is_cargo_build_dir` 认得
   `…/target/debug/zeroth`、`…/target/release/zeroth` 和
   `…/target/dist/zeroth` —— 发布就是在那个 profile 里构建的，
   `docs/ops/binary-size.md` ——（以及 cargo 那个按目标的
   `target/{triple}/{profile}/` 形状，`scripts/release.sh build` 留下的）并以
   `update the checkout instead: git pull && cargo build --release` 停下。用一个
   发布二进制覆盖 checkout 的构建产物，会被下一次 `cargo build` *撤销*，而且悄无
   声息。**还有一个包管理器的安装**，出于同样的理由：`update::package_manager`
   从二进制自己在 `node_modules` 下的路径上读出 npm、pnpm、Bun 或 Yarn，而子命令
   以那个管理器的命令停下 ——
   `npm installed {exe} — update it with npm instead: npm install -g @linuztx/alter-zero@latest`
    —— 因为一个被在管理器背后替换掉的二进制，会被它的下一次安装再替换一次。两者
   都以 `1` 退出。
2. **同一次检查** —— `update::fetch_latest`，上面那个 `HEAD` —— 以及
   `is_newer`。已经是最新的就如此打印并以 `0` 退出。
3. **取回安装器** —— 从仓库 `main` 分支的 `install.sh`（
   `update::installer_url`：一个 github.com 仓库是
   `raw.githubusercontent.com/{slug}/main/ install.sh`，其他任何根是
   `{repo}/install.sh`，这正是让替身服务器能服务它的东西）—— **到一个临时文件
   **，绝不是一条管道。`sh` 读一条空管道会以 `0` 退出；一次失败的取回必须响亮地
   失败。`update::looks_like_installer` 在跑任何东西之前检查形状（`#!/bin/sh`、
   `main "$@"` 结尾、`ZEROTH_INSTALL_DIR` 变量），所以一个以 `200` 服务的
   HTML 错误页被拒绝为*没有服务安装器*。
4. **跑它**，带三个安装器本来就尊重的变量（`docs/ops/release.md`）：
   `ZEROTH_INSTALL_DIR` = 运行中二进制自己的目录，好让更新落在提出它的那个二
   进制上（如果它不住在 `~/.local/bin` 就不落在那里）；`ZEROTH_VERSION` =
   `v{latest}`，检查解析出的那个 tag，好让两半不可能对哪个发布最新有分歧；
   `ZEROTH_INSTALL_BASE_URL` = 检查读过的同一个仓库。安装器的校验和验证、平
   台选择与 `PATH` 建议都没变 —— 这个子命令不加第二条安装路径。

这个子命令忽略 `ZEROTH_UPDATE_CHECK`：那个变量让*自动*检查安静，而一条显式的
命令是用户自己的请求。

在新二进制上的下一次启动也会再发当天的遥测 ping，带着新版本 —— 那个 ping 每天一
次规则的唯一例外（`TELEMETRY.md` *When it is sent*）—— 好让 dashboard 的
Versions 面板当天就把这个安装移动过去，而不是等到 UTC 午夜。它是同一个安装 id，
所以它算作新版本上的同一个用户，绝不算作一个新安装。

## `/settings` 那一行

**Update check** 坐在 **Telemetry** 上面（`SettingKey::UpdateCheck`），第十六
行：`true`/`false`，按用户（`SessionSettings::update_check` 是
`#[serde(skip)]`，`copy_value` 从不移动它，`apply_setting` 跳过 `settings.json`
写入），在 bootstrap 时先由 `update.json` 然后由环境播种，没有配置目录或在那个变
量之下时是 `false (unavailable)`。把它循环打开会立刻走今天的这一步（
`Session::apply_update_setting` → `update_tick`）：文件已经知道的一个更新的发布
会被宣告，而当没有任何请求跑过时，当天的请求被发出去。把它循环关掉不再发任何东
西；一个已经在飞的结果仍然记录它的日子。

## 测试

- **单元**（`update::tests`）：版本语法与排序、从重定向 URL 上读 tag（查询串、一
  个 `nightly` tag、裸的 `/releases` 页面）、检查与提示各自的每天一次规则、
  github.com 仓库和其他任何根的 URL、环境谓词、提示文本、构建目录识别、对每个管
  理器真实安装布局的包管理器识别以及各自更新所用的命令，还有安装器形状检查。
  `ui::tests::header` 钉住卡片的框和正文；`settings::tests` 钉住那一行的位置、循
  环、可用性以及它在 `settings.json` 里的缺席；`cli::tests` 和
  `tests/cli_help.rs` 钉住 `update` 的路由、带参数时的用法错误，以及帮助页的行。
- **Smoke**（`scripts/smoke.sh 116`）：`scripts/release/release_server.py` ——
  github.com 发布页面的替身，发布工具早就在用它 —— 多了一个可选的第三参数，要在
  `/install.sh` 服务的安装器。这个 phase 用 `package_dist` 从 debug 二进制打一个
  假的 `v9.9.9` 发布，把 `ZEROTH_UPDATE_URL` 指向那个服务器，并驱动：一个全
  新的 home 显示卡片并记录全部四个字段；同一天的重启不显示卡片；`/settings` 那一
  行循环进 `update.json`、从不进 `settings.json`；`ZEROTH_UPDATE_CHECK=0` 撤
  掉那一行且不写文件；一个最新发布*就是*运行中版本的服务器记录 `latest` 而没有
  `notice_day`；从二进制的一份拷贝对着那个服务器跑 `zeroth update`（
  `is the newest release`，退出 `0`）、从 `target/debug` 跑（被拒，退出 `1`），
  以及对着 `v9.9.9` —— 那份拷贝被服务出去的归档里的二进制替换掉（一个新的
  inode）并且仍然回答 `--version`，因为那个假发布就是这个构建。
