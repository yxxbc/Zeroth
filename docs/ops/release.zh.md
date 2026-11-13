# 发布 —— tag、workflow、资产、发布说明

日期：2026-09-12

## 目标

切一个发布是一小段命令，tag 推送做完其余的：项目的闸门、为每个受支持平台做一次
release 构建、校验和、发布说明，以及承载它们的 GitHub release。版本号从不被
敲两遍，workflow 跑的每一步都是开发者能在笔记本上跑的命令，而整条流水线可以
彩排——构建、验证、说明——什么都不发布。在这之前既没有 CI 也没有打包好的
release；第一个 release 通过这条流水线切出，而工具链正是让它能被有信心地
切出来、而不是靠手工的原因。

## 形状

```
scripts/release.sh prepare X.Y.Z     # bump + roll the changelog (a commit's worth of edits)
git commit … && git tag -a vX.Y.Z -m "zeroth vX.Y.Z" && git push origin HEAD vX.Y.Z
        │
        ▼  .github/workflows/release.yml
   check ──► gate ──► smoke ──► build ×4 ──► publish
```

1. **`check`** —— tag 点名 Cargo.toml 的版本，而 Cargo.lock、README 的版本
   badge 和 `CHANGELOG.md` 都与它一致；然后是工具链自己的 `selftest` 和
   `shellcheck`。每一处不一致都在 job 失败之前报出来，于是一次运行就点名全部。
2. **`gate`** —— `CLAUDE.md` 里的 pre-commit 闸门，未改动：`cargo fmt
   --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、
   `cargo doc --no-deps --lib`。
3. **`smoke`** —— 在 tmux 里对真实二进制跑终端 I/O 边界
   （`docs/ops/smoke.md`），形状与 `ci.yml` 的 job 相同：每核一个 worker，
   然后并行那一趟失败的阶段串行重跑一次，才判红。单元闸门看不见终端路径，
   而一个 tag 是最不该第一次看到它出错的地方。
4. **`build`** —— 每个 target 一个 job（下面的矩阵），各自跑
   `scripts/release.sh build TARGET` 再对它的输出跑
   `scripts/release.sh verify dist`，并把 `dist/` 作为一个 workflow artifact
   上传。
5. **`publish`** —— 下载每一个 `dist/`，*一起*验证它们，把说明
   （`scripts/release.sh notes`）渲染进 job summary，然后要么发布
   （`scripts/release.sh publish`），要么在彩排时打印它本该运行的确切 `gh`
   命令。

每个 job 都持有 `contents: read`。只有 `publish` 被授予 `contents:
write`，而且只有 `GH_TOKEN` 那一步用它，所以任何彩排——包括一个 pull
request 的——都不持有任何能发布的东西。

### 一个 release 携带什么

| 资产 | 平台 | 在哪里构建 |
| --- | --- | --- |
| `zeroth-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` | Linux x86_64（glibc ≥ 2.35） | `ubuntu-22.04`，原生 |
| `zeroth-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` | Linux arm64（glibc ≥ 2.35） | `ubuntu-22.04`，用 `gcc-aarch64-linux-gnu` 交叉编译 |
| `zeroth-vX.Y.Z-x86_64-apple-darwin.tar.gz` | macOS Intel | `macos-latest`（arm64），由 Apple 的 clang 交叉编译 |
| `zeroth-vX.Y.Z-aarch64-apple-darwin.tar.gz` | macOS Apple silicon | `macos-latest`，原生 |

每个归档解出一个同名的主干目录，里面装着 `zeroth` 二进制（模式 755）、
`LICENSE`、`README.md` 和本次 release 的 `CHANGELOG.md`——别的什么都没有，而
`verify` 精确地强制这份清单。每个归档旁边坐着 `{archive}.sha256`，一行
`sha256sum -c` 形式（`<hex>  <filename>`），而 `SHA256SUMS` 收集每一条这样的
行，好让用户用一条命令校验任何下载。资产按完整的 Rust target triple 命名，而
不是更友好的 `linux-arm64`：triple 对 libc 无歧义，它是 `rustup target add` 和
`cargo build --target` 所接受的东西，而 `verify` 直接从名字里把 target 解析
回来，以决定 `file(1)` 必须怎么描述里面的二进制。

这个二进制就是 Cargo.toml 为打包 release 所记录的那个——
`cargo build --profile dist --locked --target T`。`--locked` 拒绝一个
Cargo.lock 已陈旧的构建（Cargo.toml 改了却没改它的 lock 条目），而 `dist` 是
在单个 codegen unit 上做 fat LTO、没有增量缓存的 release profile：在 x86_64
Linux 上，是 18.6 MB 而不是 25.6 MB 的二进制，加上更快的渲染路径，代价是构建
多花约两分钟（`docs/ops/binary-size.md`）。`release` profile 本身仍是开发者的
快速增量重建（`docs/ops/build-time.md`）。二进制**不被 strip**：profile 已经
丢掉了 debuginfo，而留下的符号表正是把用户的 `RUST_BACKTRACE=1` 报告变成函数名
的东西。LTO 之后它花掉二进制的 ~1.8 MB 和它 ~8.6 MB 归档的 ~0.4 MB；构建行上
的 `--config profile.dist.strip=true` 就是那个旋钮，如果哪天这更要紧的话。

### 说明

一个 release 的说明就是 `CHANGELOG.md` 的 `## [X.Y.Z]` 小节，加上指向上一个
release 的 compare 链接（第一个是 `…/commits/vX.Y.Z`）——别的什么都没有。
changelog 是描述一个 release 的唯一地方——`notes` 只读它，从不改写——所以写
在那里面的条目就是用户读到的条目，而 `check` 拒绝一个小节缺失或为空的 tag。
GitHub 自动生成的「what's changed」提交清单被考虑过而没用：提交主题是写给
reviewer 的，changelog 条目是写给用户的，两者很少读起来一样。

**一个小节可以带标题。** 它的第一行可以是一行**粗体**——`**Zeroth
Initial Release**`——给这个 release 命名，而不是列出它的某一项改动。这不需要
任何机制：小节按写下的样子渲染，所以那一行就以它本来就是的粗体文字出现在说明
里，压在 `### Added` 各节之上而不压过它们。标题（heading）试过又被拿掉了——
`##` 渲染得比它所引出的小节更大，于是那个名字朝它所该引出的一堆改动喊。

release 本身只由它的 **tag** 命名（`vX.Y.Z`），这是有意的。在那里追加一个
标题——`vX.Y.Z — Zeroth Initial Release`——在 releases 列表里读起来是噪声，
那里每一行已经是版本号，而视线在扫数字；那句话属于有人打开这个 release 去读的
地方。两种都行：一个小节直接从 `### Added` 开始的，得到同样直接从那里开始的
说明，而每个 `prepare` 出来的小节起初都是那样。

**贡献者是 GitHub 来渲染的，不是我们。** release 页面已经在 body 和资产之间
显示它们，作为从 release 自己的提交里读出的头像。一行
`**Contributors** name, name` 曾被写在 body 下面，在第一个 release 上发布过，然后被
拿掉：那又是那块东西，以纯文本，直接压在真品上面，而文字在「一眼告诉你是谁」
上输给头像。它的 `git log` 正是 publish job 的 `fetch-depth: 0` 的用途，所以
checkout 随它一起回到了默认的浅克隆。

说明以前携带更多——一张 **Assets** 表，列着每个归档、它的平台和 SHA-256，然后
是一行安装器压在手工验证并安装的片段之上——两者都拿掉了，因为 release 页面
已经把那些问题回答得更好：GitHub 就在说明下面列出上传的资产及它们的大小，
`SHA256SUMS` 是其中之一，而安装器是 README 的第一条命令。在这个页面上它们读
起来是同样的事实两遍，把真正变了的东西推到一张哈希表下面。新的东西才是说明
唯一存在的理由。

**而且只有归档和 `SHA256SUMS` 被上传。** 每个资产的 `.sha256` 文件仍写进
`dist/`——`verify` 读每一个——但把它们发布出去，会在资产列表里把四行校验和文件
排在构建本身之前。一个 `sha256sum -c` 文件覆盖每个归档，所以 `install.sh` 读它
刚下载的那个资产那一行，并且仍然拒绝安装任何它验证不了的东西。

## 安装器

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh | sh
```

仓库根目录的 `install.sh` 就是 README 开头的那一行。顶部的 `REPO` 是
`yxxbc/zeroth`，而它安装的二进制以那个 slug 命名——`zeroth`——因为 release
归档是以 Cargo.toml 的 `name` 命名的。`ZEROTH_REPO` 覆盖这个 slug
（selftest 的 fixture release 是 `example/zeroth`，它校验的哈希就是那个
release 的）。
它把 `uname` 映射成一个 target triple（拒绝 Windows、musl，以及早于 2.35 底线
的 glibc，并给出一条指向从源码构建的消息），从 github.com 的
`/releases/latest` 重定向读到最新的 tag——curl 的最终 URL，或者在没有 curl 时
wget 打印的最后一个 `Location:` header；从不用 API，它按地址限速，而且对
`ZEROTH_INSTALL_BASE_URL` 一无所知（`ZEROTH_VERSION=vX.Y.Z` 或
`--version` 改为钉住一个）——下载归档**和**这个 release 的 `SHA256SUMS`，在
拿不到校验和文件、或文件里没有匹配值时拒绝继续，解压，把二进制复制进
`~/.local/bin`（`ZEROTH_INSTALL_DIR` 或 `--dir` 指别处），与任何旧的一份
并排，然后移到位上，好让一个正在跑的 `zeroth` 保留它映射的文件、而新的那份
整个出现，跑一下已安装二进制的 `--version` 作为最后一项检查，并以接下来该做
什么收尾——当目录不在 PATH 上时给用户自己 shell 的 PATH 行，以及如何卸载。它
从不提示：在 `curl | sh` 之下，它的 stdin *就是*这个脚本。

它是 POSIX `sh`，不是 bash——没有数组、没有 `local`、没有 `[[`，也没有光秃秃的
`$name` 挨着多字节字符——因为管道目标是任何 `sh`（macOS 上 POSIX 模式的
bash 3.2、Debian 上的 dash、容器里的 busybox），而整个文件都是函数，最后一行
只有一个 `main "$@"`，于是一个下载中途断掉的连接交给 `sh` 的是一个不完整的
文件，它什么都不定义、什么都不运行。输出穿的是应用自己的装扮：横幅渐变里的
`crest` mascot（`COLORTERM` 说明可以用时用 truecolor，否则用终端的青色）、
`→`/`✔`/`✘` 步骤字形，而在管道里、`NO_COLOR` 下、或 UTF-8 locale 之外，这些
一概没有——没有颜色，ASCII 字形。

## mascot 与 `--preview`

横幅是应用的默认 mascot `crest`（`src/app/mascot.rs`），画成紧挨名字的三行
单宽块字形。在终端上它会动：色调沿着横幅的青色 → 蓝色渐变逐行走下，冠部摇摆，
眼睛每秒眨一次。它在各步骤之前播放约一秒半，然后在**Download** 行变成单行
spinner 直到归档落地，并报告到目前为止的字节数。管道、`TERM=dumb`、`NO_COLOR`
或非 UTF-8 locale 得到一次应用名和一次静态画面，以及纯 fetch 而不是 spinner
——输出其余部分遵循的同一规则。

`--preview`（也拼作 `--proview`）只播放 mascot 十秒——`--preview N` 和
`--preview=N` 取任意整数秒——然后退出：不查平台、不联网、什么都不安装，所以从
一个 checkout 里、在 SSH 之上、或为了看横幅在某个主题里长什么样，跑它都安全。
帧是数出来的，不是计时的，所以一个慢终端跑得久而不是丢帧；在 `sleep` 不接受
小数的地方（一个严格的 POSIX shell），一帧持续一秒而不是十分之一秒。走管道时，
画面画一次，运行说明它需要终端才能动。

这个脚本从 `main` 提供，而不是附在每个 release 上，因为那样 URL 永不改变：
资产的名称携带版本，而安装器自己解析版本。这也意味着对安装器的一个修复不用
发一个 release 就能到用户手里。它的离线测试替身是
`scripts/release/release_server.py`，一个 github.com 发布页面的替身——那个重
定向、tag 页面、指向某个目录的下载 URL——selftest 把
`ZEROTH_INSTALL_BASE_URL` 指向它，于是一条不联网、却对着真实归档跑遍安装器
每条路径的测试：最新版和一个钉住的 release、在自身上重装、一个没有本机资产的
release、一个被篡改的校验和（什么都不安装，而且输出说明这一点）、`--help`、
一个未知 flag，以及一次字节级的扫描，找 bash 3.2 唯一会读错的那个形状——一个
光秃秃的 `$name` 挨着一个非 ASCII 字节：`$dim` 挨着 `·`，向 macOS 的 `/bin/sh`
要一个叫 `dim` 的变量加上 `·` 的第一个字节，而 `set -u` 在那里中止了安装。

应用从内部驱动同一个脚本：`zeroth update`（`docs/ops/update.md`）从同一个
重定向读最新的 tag，当它比正在跑的二进制新时，把 `install.sh` 取到一个临时
文件，并以 `ZEROTH_INSTALL_DIR` 设为二进制自己的目录、`ZEROTH_VERSION`
设成它刚读到的 tag、`ZEROTH_INSTALL_BASE_URL` 设成同一个仓库来运行它——
所以应用内更新*就是*填了三个变量的一行安装，没有第二条安装路径要维护正确。
因此 `release_server.py` 也在 `/install.sh`（它的可选第三个参数）提供安装器，
这正是 `smoke.sh` Phase 116 能端到端地对一个打包好的假 release 跑那条命令的
原因。

## 脚本

`scripts/release.sh` 是 `scripts/release/` 下各步骤之上的一个入口——smoke 套件
的「runner 套在 phase 之上」形状：每个步骤是自己的文件、source 进
`scripts/release/lib.sh`，workflow 是同样那些命令的薄调用者，而 `RELEASE_ROOT`
把所有读者指向一棵树，这就是 selftest 让它们对 fixture 跑的方式。

| 命令 | 它做什么 |
| --- | --- |
| `version` | 打印 Cargo.toml 的 `[package]` 版本。 |
| `check [TAG]` | 下面的规则 1–6；没给 TAG 时读 `GITHUB_REF_TYPE`/`GITHUB_REF_NAME`。 |
| `build [TARGET] [--dist DIR]` | 需要时 `rustup target add`、Linux GNU target 的交叉工具链变量、文档里那个 `cargo build`，然后把 `package_dist` 做进 `dist/`。 |
| `verify [DIST] [--version X]` | 每个归档的名字、校验和、布局、可执行位、`file` 的格式/CPU，以及——当这台机器能跑它时——`--version`；`SHA256SUMS` 的一致性；没有多余文件。 |
| `notes VERSION [DIST]` | 上面那段 markdown，到 stdout。 |
| `publish VERSION [DIST] [--notes FILE] [--dry-run]` | `verify`，组装 `SHA256SUMS`，然后 `gh release create` **作为 draft**，带说明和每个资产，然后 `gh release edit --draft=false`（`--latest`，除非是 pre-release）。 |
| `download VERSION [DIST] [--from URL]` | 把一个已发布的 release 拉回空的 `dist/`：它的 `SHA256SUMS`，然后它列出的每个资产，用 curl 或 wget（不用 `gh`），每个旁边放着 `build` 会留下的那份 `.sha256`，然后 `verify`。`--from` 点名另一个仓库根。 |
| `prepare VERSION` | 版本 bump 与 changelog 滚动，然后 `check`，然后要运行的 commit/tag/push。 |
| `selftest` | 203 条由 fixture 驱动的断言，覆盖以上全部。 |

`check` 的规则：

1. Cargo.toml 的 `[package]` 版本是 semver（`X.Y.Z`，可选 `-pre` 和
   `+build`）；带连字符的版本是 **pre-release**，`publish` 会标记它，从不把它
   标成 latest。
2. Cargo.lock 记录这个 crate 的同一个版本。
3. README 的版本 badge 一致——URL 标签（shields.io 把 `-` 转义成 `--`，所以
   `0.2.0-rc.1` 以 `0.2.0--rc.1` 搭过去）和 alt 文字都一致。
4. `CHANGELOG.md` 有 `## [X.Y.Z] - YYYY-MM-DD`，body 非空，并有一条
   `[X.Y.Z]: url` 链接引用。
5. `CHANGELOG.md` 保留 `## [Unreleased]` 和它的链接。
6. 带 tag 时：它是 `vX.Y.Z`，且 `[Unreleased]` 是**空的**——留在那里的一条条目
   是为这次 release 写的，却从未滚进去。

CI 每次推送都跑不带 tag 的 `check`，于是这棵树时刻受规则 1–5 约束：Cargo.toml
里的版本永远是最新已发布的那一个或即将发布的那一个，绝不是在一个周期开始时
bump 出来的「下一个版本」，而一个 bump 了版本却没有 changelog 小节的 pull
request 会在 CI 失败，而不是在最终那个 tag 上失败。

`verify` 的 CPU 检查正是让交叉编译可以放心自动化的东西。一次悄悄退回宿主二进制
（一个陈旧的 `target/` 产物、一个设错的 linker）的交叉构建，会上传一个完美但
里面装着错误机器码的归档；在解出的二进制上跑 `file -b` 必须点名该 target 的
目标格式和 CPU（`ELF 64-bit` + `aarch64`、`Mach-O 64-bit` + `x86_64`……），而
selftest 通过把一个宿主二进制以另一个 target 的名字打包，证明了这项检查会触发。

`publish` 的顺序正是让一次失败的上传无害的东西。release 以一次 `gh` 调用创建成
**draft**，带说明和每个资产，所以一切都在那儿之前什么也看不见，然后才翻成
已发布。对某个 tag 已发布的 release **永不**被碰——这里的 release 是不可变的；
修复是一个新版本——而一个残留的 draft（一次传到一半死掉的运行）被删掉重建。
`--verify-tag` 拒绝凭空造 tag：tag 推送才是触发器，而一个仓库里没有 tag 的
release，是「无物的 release」。

`prepare` 原子地重写四个文件（经临时文件和 `mv`，不用 `sed -i`，GNU 和 BSD 对
它的参数意见不一）：Cargo.toml 的 `[package]` 版本，别的什么都不改；Cargo.lock
里这个 crate 的条目（正是 `cargo update --workspace` 会写的东西）；README badge
的标签和 alt 文字；以及 `CHANGELOG.md`，其中 `## [Unreleased]` 空着留在一个新的
`## [X.Y.Z] - <today>` 标题之上，那标题接过它的 body，而链接块得到该版本的
compare 链接（第一个 release 则是 tag 链接），`[Unreleased]` 重新指向它之后。
它拒绝一个不是 semver 的、就是当前版本的、已经有小节或有本地 tag 的、或者会让
一个空的 `[Unreleased]` 发布的版本——那样说明也会是空的。

## 彩排

**在 GitHub 上，在第一个 tag 之前**：手动跑 Release workflow（Actions →
Release → *Run workflow*，`publish` 关掉）。它做 tag 推送所做的一切——`check`、
闸门、四次构建、对每个和对全部的 `verify`——把说明渲染进 job summary，把每个
`dist/` 和说明作为 artifact 保留七天，并打印它本该运行的 `gh` 命令。这是推荐的
第一次运行：它演练一台笔记本做不到的那些平台（两个 macOS 构建、runner 上的
arm64 交叉构建），而不押任何东西。

**本地**（同样的命令，宿主的平台）：

```bash
scripts/release.sh check                       # the tree agrees with itself
scripts/release.sh selftest                    # the tooling agrees with its fixtures
scripts/release.sh build                       # dist/zeroth-vX.Y.Z-<host>.tar.gz (+ .sha256)
scripts/release.sh verify dist                 # …checksum, layout, CPU, `zeroth --version`
scripts/release.sh notes X.Y.Z dist            # the notes the workflow would publish
scripts/release.sh publish X.Y.Z dist --dry-run
```

一台装有交叉工具链的 Linux 机器也构建 arm64 资产
（`apt-get install gcc-aarch64-linux-gnu libc6-dev-arm64-cross`，然后
`scripts/release.sh build aarch64-unknown-linux-gnu`；`verify` 检查它的 CPU 并
跳过运行它）。macOS 资产在 Mac 上构建，两个 CPU 都能从任一台出。

**手工发布**，如果自动运行在构建之后失败了：带 `publish` 打开在那个 tag 上跑
workflow，或者在本地用已登录的 `gh` 跑 `scripts/release.sh
publish X.Y.Z dist`（dry run 显示确切的调用）。

## 切一个 release

```bash
scripts/release.sh prepare 0.2.0               # edits Cargo.toml, Cargo.lock, README.md, CHANGELOG.md; runs check
git add Cargo.toml Cargo.lock README.md CHANGELOG.md
git commit -m "Release v0.2.0"
git tag -a v0.2.0 -m "zeroth v0.2.0"
git push origin HEAD v0.2.0
```

第一个 release 不能是 `0.1.0`：继承下来的 changelog 已经占着那个小节，
`0.1.1` 到 `0.18.0` 也一样，而 `prepare` 拒绝一个已经有小节的版本。选
`0.19.0` 接在上游最后一个 release 后面延续这条累积的线，或者选 `1.0.0`
在它之上开始 Zeroth 自己的线——然后 `scripts/release.sh prepare <version>`
一步把 `[Unreleased]` 滚进新小节，并更新 Cargo.toml、Cargo.lock 和 README
badge。小节的日期是它被写下的那天；如果不同，把它改成打 tag 那天。打 tag
之前先跑一次彩排。

当一个版本在进展中时，改动落在 `## [Unreleased]` 之下、Keep a Changelog 的各
组里（`Added`、`Changed`、`Deprecated`、`Removed`、`Fixed`、`Security`），
写给读发布说明的用户，而不是读提交的 reviewer。

## CI

`.github/workflows/ci.yml` 在每次推送到 `main` 和每个 pull request 上运行：
**gate**（那四条命令）、**smoke**（在真实二进制之上跑 tmux 套件）、
**release tooling**（对脚本和 `install.sh` 跑 `shellcheck`、`selftest`、
`check`），以及 **内置 skill 脚本**（`web-search` skill 所播种的那些脚本的离线
`unittest`）。smoke job 每核跑一个 worker，而不是套件默认的两倍：在四核机器上
一次 `-j 8`
运行在这里让 phase 58 和 61 失败——权限 prompt 在它的等待里「从未出现」——而
两者单独都通过。于是这个 job 在判红之前，把并行那一趟失败的东西各串行重跑一次，
并在第一趟失败时把两趟的日志都留作 artifact，于是一个 flake 留下证据而不是一个
绿方块。release workflow 重跑闸门、smoke 套件和工具链 job，而不是信任一次
可能没在那个打了 tag 的提交上跑过的 CI——tag 可以从一个 CI 从未见过的分支
推出去。smoke 套件两边都有，因为它是终端 I/O 边界唯一的自动化覆盖
（`docs/ops/smoke.md`），而 release 是最不该第一次看到终端路径出错的地方。

## 决定

**手写 GitHub Actions 加 `gh` CLI，而不是 cargo-dist 或某个 release
action。** `cargo-dist` 生成一个几百行的 workflow，自带安装器和自带的
changelog 观念，并在 2025 换了维护者；那些 release action
（`softprops/action-gh-release` 之流）为唯一那一步需要写权限的东西引入第二个
信任根。`gh` 在每个 runner 上都预装，并用 job 自己的 token 登录，所以发布是两条
谁都能读、能跑的命令，而决定*发布什么*的逻辑住在本地可跑的 shell 里。代价是
cargo-dist 本会生成的安装器——列在 follow-ups 里。

**脚本是事实来源；workflow 是一个调用者。** 每个 `run:` 都是一条
`scripts/release.sh` 命令（smoke 套件的安排），所以没有只能在 YAML 里、又没有
办法跑的逻辑，而 fixture selftest 覆盖的是逻辑，而不是一次 workflow 运行。

**四个 target，glibc，不要 musl，不要 Windows。** 这个 crate 是 unix 形状的
——`rustix` 的 termios 和 `setsid`、tty 脱钩、`/proc` 读取——从未为 Windows
构建过；一个测试覆盖不到的 target 不是要发布的 target。一个静态的 musl Linux
构建会在任何发行版上跑，但 musl 的分配器在分配密集的渲染路径上明显更慢，而且
`docs/architecture/memory.md` 里每个常驻内存数字都是对着 glibc 的分配器测的；
发布 musl 意味着先重测一遍。Linux 资产在 `ubuntu-22.04` 而不是 `-latest` 上
构建，以把它们的 glibc 底线钉在 2.35：在一个更新的镜像上构建的 release，会在
每个更老的发行版上悄悄停止运行。arm64 用发行版的 `gcc-aarch64-linux-gnu` 交叉
编译，而不是在 arm64 runner 上构建，因为它不需要特殊的 runner 标签，而 `cc`
crate（oniguruma、`ring`）在每种方案上都按 triple 的前缀找到交叉编译器。

**可复现的归档。** `package_dist` 给每个暂存文件盖上二进制自己的 mtime（它的
构建时间）、不带构建者的 uid/gid 归档、按名字顺序、并把 gzip 的时间戳去掉，于是
同一个二进制的两次打包，无论何时跑，都逐字节相同。mtime 那一行是学来的：
`cp` 给每份拷贝盖上*此刻*，所以只有当两次打包落在同一秒内检查才通过，而
selftest 现在在它们之间睡一秒，免得那问题再回来。二进制本身不声称跨机器可
复现。

**版本在四个地方被读，是因为它被写在四个地方。** 一个单一来源会更好；在生成
README 之外，次好的事情是一项在任何副本漂移时失败的检查，以及一个把四者都改
掉的 `prepare`，好让没有人需要记住。

**Action 钉在 major tag 上，由 Dependabot 移动。** `actions/checkout`、
`actions/upload-artifact`、`actions/download-artifact` 和
`Swatinem/rust-cache` 按 major 版本引用，即生态的常规：一个 major tag 把 minor
和 patch 的修复浮动给我们，所以只有一次 *major* bump 才是一个决定，而
`.github/dependabot.yml` 每月提议那些——每个 action 的 bump 在**一个 pull
request** 里（该文件的 `actions` 分组），而在 release 满一周之前一个都不提
（它的 `cooldown`）。

两个设置都出自第一次运行。文件一到达默认分支，它就开了三个 pull request——
checkout 5 → 7、upload-artifact 4 → 7、download-artifact 4 → 8——每个 action
一个，那是默认行为：不是配置错误，而是一个在这里有锋利边缘的默认。四个里的
两个是一**对**：release 的 build job 各自 `upload-artifact` 一个平台的 dist，
而 publish job 把它们全 `download-artifact` 回来，而 artifact 的 major 在这道
接缝上以前就破坏过兼容性（v3 的上传按设计 v4 的下载读不了）。分成两个 pull
request 提议，一对 major 一起动的组合没有绿色的通过之路——每一半对着另一半的旧
major 都是红的——而出路就是关掉那三个的手工 bump。分组之后，这一对在一个 pull
request 里移动，而那个 pull request 在它落地的地方被演练：`release.yml` 在**任何
触及它的 pull request** 上作为一次彩排运行，触及 release 脚本或 `install.sh` 也
一样。它的 check job 从 tag 推出 `publish=false`，所以整条流水线——闸门、四次
平台构建、artifact 交接、`verify`、说明、`publish --dry-run`——都跑，却什么也不
发布。没有那个触发，那两条线只在 tag 上跑，而一个 pull request 跑的每一项检查都
来自 `ci.yml`，于是一半移动的配对会绿着合并；现在它是一个红检查，而不是一个坏
掉的 release，一个新的 major 里被改名的输入也一样。代价是一个触及那些路径的
pull request 上两个 macOS runner，而那些按构造就罕见。cooldown 是供应链那一半：
一个被重打 tag 或被撤下的 action，或一个一到就坏的 major，会在它的头几天显现，
而对着每月一次的检查，这一周最多花掉一个周期。

第一次运行也教了一个习惯。**手工关掉 Dependabot 的某个 pull request 就是忽略
那个 release**——按它在那三个上的原话，「OK, I won't notify you again about
this release, but will get in touch when a new version is available」——直到
main 带上那个版本，或那个 pull request 被重新打开。要接受一次 bump，就合并那
个 pull request；要手工 bump，就让那个 pull request 开着，Dependabot 在它下次
检查（每月一次，或在仓库 Dependabot 页面 Insights → Dependency graph 下的
*Check for updates*）时把它作为不再需要而关掉。

读目标 major 的 `action.yml` 仍然是那次 review：这个仓库所依赖的输入是下载侧的
`pattern` 和 `merge-multiple`、上传侧的 `if-no-files-found` 和
`retention-days`，以及 `archive` 默认为 true，于是一个上传的 artifact 仍是下载
所解的那个 zip。`grep -rn 'uses:' .github/workflows/*.yml` 列出每一个钉住处。
把每一个钉到一个 commit SHA（版本写在尾注释里）是一个想要对重打 tag 免疫的
仓库的进一步加固步骤；它让每个 action 每次 release 都要 bump。

**工具链只来自 `rust-toolchain.toml`。** workflow 跑
`rustup show active-toolchain || rustup toolchain install`，它从该文件安装钉住
的 channel 及其 `rustfmt`/`clippy` 组件（旧 rustup 上用第一种形式，现在的用第二
种），于是一次工具链 bump 就是它本来就已是的那一处编辑。

## 验证

- `scripts/release.sh selftest` 在一个临时目录下的 fixture 树上驱动每条规则
  ——一个 manifest，其依赖各自带 `version =` 行，还有一个叫 `version` 的 crate；
  一个有两个 release 的 changelog；在一棵一致的树上跑 `check`，再试每一种破坏
  它的方式；对一个真实（很小、C 写的）二进制跑 `package_dist` + `verify`，好让
  `file` 和 `--version` 检查真跑起来；CPU 检查在一个被贴错标签的二进制上触发；
  带资产和不带资产的 `notes`；`prepare` 把一个 fixture 往前滚，以及一次首发；
  在 `PATH` 里没有 `gh` 时的 `publish --dry-run`；端到端地对着替身 release
  服务器（见上）跑 `install.sh`；用 curl 和只用 wget 从那个服务器 `download`
  回来。它需要 bash、awk、sed、tar、gzip、`file`、一个 C 编译器和 python3，跑
  完不到半分钟；CI 和 release workflow 都跑它。
- `shellcheck -x` 扫过脚本和 `install.sh`（它按 POSIX `sh` 检查，标出任何
  bashism），在 CI 里和 release 的 `check` job 里。
- 脚本避开 bash 4（`${var,,}`、关联数组、`mapfile`）、`sed -i`、`readlink -f`
  和只有 gawk 才有的 awk，因为一个 macOS runner 可能交给它们 bash 3.2、BSD
  sed 和 BWK awk。
- 这套工具链的第一次本地彩排用 2 分 15 秒构建了 x86_64 Linux 资产（一个 22 MB
  二进制、一个 9.0 MB 归档），`verify` 跑了打包二进制的 `--version`，arm64 资产
  在同一个脚本下交叉编译并在 qemu 下运行了，而 `install.sh` 从替身服务器安装了
  x86_64 归档，校验和已验证，只花一行。

## 后续工作

没做，想做的时按这个顺序值得做：一个指向 macOS 资产的 Homebrew tap formula；
给校验和签名（从 workflow 做 Sigstore `cosign` 的无密钥方式，或 `minisign`），
好让下载者能验证来源而不只是完整性；把 crate 发布到 crates.io（从 `publish`
job 跑 `cargo publish` 需要一个 token secret）；等分配器测过之后做一个 musl
Linux 资产；把 action 钉到 SHA。
