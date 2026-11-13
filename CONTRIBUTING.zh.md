# 为 Zeroth 做贡献

谢谢你看过来。Zeroth 是一个用 Rust 写的终端 coding agent，这一页是你要改动它所
需的东西：怎么把构建跑起来、每个改动都要过的闸门、这套代码库建立时所依据的两条
实践，以及其余的东西写在哪儿。

设计上的取舍刻意不在这里。[`docs/`](docs/) 每个功能一页，讲它做什么*以及为什么
这样做*，而 `CLAUDE.md` 是架构地图。动手改之前，先读你要碰的那块地方的那一页
—— 这套代码里看起来随意的多数东西，都是有人已经付过代价的 bug。

## 帮得上忙的方式

| | |
| --- | --- |
| **报一个 bug** | 开一个 issue，带上你的终端、操作系统、`zeroth --version`，以及复现步骤。任何和画面有关的东西，一张截图或者一份 `tmux capture-pane` 转储，价值胜过一段文字。 |
| **修一个 bug** | 从能复现它的那个失败测试开始，然后是修复。小而显然的修复不需要先讨论。 |
| **加一个功能** | 先开 issue，在动手写之前把设计议定 —— 见 [先设计再实现](#先设计再实现)。 |
| **改进文档** | 一页 `docs/` 不再与代码相符，就是一个 bug。一条没说清用户会注意到什么的 `CHANGELOG.md` 条目也是。 |
| **加一个模型 provider** | 通常是在 [`providers.toml`](providers.toml) 里加一个 `[providers.<id>]` 块 —— 文件自己的头部注释记录了每一个键。只给自己用的话，同一个块放进 `~/.zeroth/providers.toml`，这里不用改（[`docs/providers/user-providers.md`](docs/providers/user-providers.md)）。带新认证形状或新线格式的 provider 是一个功能：先议定（[`docs/providers/llm.md`](docs/providers/llm.md)）。 |

## 把环境搭起来

你需要 [rustup](https://rustup.rs) 和一个 C 编译器。工具链钉在
`rust-toolchain.toml` 里，rustup 会替你装上 —— 不要去覆盖它，一个浮动的
`stable` 正是这个钉住要防的东西。

```bash
git clone https://github.com/yxxbc/zeroth.git
cd zeroth
cargo run
```

什么都没配置时，应用打开它的**离线 demo** —— 预设的回合，其 tool call 走的是
*真的*执行器，所以各个 cell 像现场的一样编号、高亮、失败时标红。多数 UI 工作是
对着它开发的，而 smoke 套件只用它
（[`docs/architecture/dummy-backend.md`](docs/architecture/dummy-backend.md)）。

如果检出构建得不像这一页说的那样，先跑 `scripts/preflight.sh --smoke`：它会检查
钉住的工具链、rustfmt/clippy、C 编译器、`mbx`、`tmux`、bash 4+ 和 `setsid`，并
为缺的那一个点出修法。在 macOS 上最容易咬人的两个是 `setsid`
（`brew install util-linux`，然后把 `/opt/homebrew/opt/util-linux/bin` 加到
`PATH` 上）和 bash 3.2（`brew install bash`）—— 见下面的 *macOS*。

### 用 `mbx` 构建

编译产物通过 [mbx](https://github.com/mr-boxington/mbx) 在各个检出之间共享，所
以第二次检出和切分支花的是秒，而冷构建花的是分钟。`mbx setup` 装一个 `cargo`
垫片；把它放在 `PATH` 最前面之后，裸的 `cargo …` 就走 mbx，别的什么都不变。清掉
构建是 `mbx clean`（在工作区根目录跑），它删掉托管的 target 和学到的增量状态，
不碰共享存储 —— 没必要 `rm -rf target/`，那是一个指向该存储的符号链接。

```bash
mbx setup              # install the cargo shim (once per machine)
mbx doctor             # shim, cache, toolchain, remote-cache status
mbx stats              # what the cache has saved
mbx clean              # the workspace's managed target — not a hand rm -rf
```

不用 mbx 一切也照样能用；只是构建是冷的。

### macOS

Linux 是参照平台（CI 在 `ubuntu-latest` 上跑闸门和 smoke 套件），但只要两个
Homebrew 包在 `PATH` 上，Mac 就是受支持的开发机：

```bash
brew install util-linux bash tmux
export PATH="/opt/homebrew/opt/util-linux/bin:$PATH"
```

- **`setsid`**（来自 `util-linux`）是让一个派生出来的命令拥有自己终端的那个东
  西。没有它，每个要起 tty 的测试都会以*这里没有办法给命令一个自己的终端*失
  败 —— 46 个测试，看起来像坏掉的分支，其实不是。在 macOS 上装了 `setsid` 仍有
  六个测试失败（那里的 `ps` 没有 `sid` 关键字，再加上五个 SIGTERM/粘贴时序的测
  试）；AGENTS.md 的 *macOS：那六个基线失败* 把它们连同确切的 `--skip` 参数一
  起列了出来。按绿色形式这样跑：

  ```bash
  PATH="/opt/homebrew/opt/util-linux/bin:$PATH" cargo test --lib
  ```

- **bash 4+** 是 `scripts/smoke.sh` 需要的（`declare -A`）。macOS 的
  `/bin/bash` 是 3.2，套件会以 `declare: -A: invalid option` 死掉；在读一个
  smoke 失败之前，先确认 `bash --version` 报的是 5.x。
- **有 35 个测试在 macOS 上根本不编译**（只有 Linux 的 `cfg` 分支：
  `llm/exec.rs`、`pty/probe.rs`、`clipboard.rs`、`subprocess.rs`、……），所以
  那里一次绿色的 `--lib` 跑，对 pty 探测、执行器和 Linux 剪贴板什么都不说明。那
  些需要 CI 的 Linux 跑，或者一个容器。

`scripts/preflight.sh --smoke` 一屏就把这一切都查了。

### macOS

<details>
<summary><strong>只有某些领域才需要的工具</strong></summary>

| 工具 | 用来做什么 |
| --- | --- |
| `tmux` | `scripts/smoke.sh` —— 终端 I/O 边界唯一的自动化覆盖 |
| `shellcheck` | `install.sh` 和 `scripts/release/*.sh`，CI 会 lint 它们 |
| `python3` | `scripts/build_timings.py`，smoke 套件的时钟/盲文辅助 |
| `Xvfb` | Linux 剪贴板的 X11 粘贴读取（`cargo test --test clipboard_linux -- --ignored`） |

</details>

## 闸门

四条命令。四条都干净，才能说一个改动做完了：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo doc --no-deps --lib
```

CI 跑的就是这四条，而 `Cargo.toml` 的 `[lints]` 表把同一副姿态烤进每次构建 ——
`unsafe_code = "forbid"`，`warnings` 和 `clippy::all` 都拒绝 —— 所以一个警告也
会让你本地的构建失败。

doc 构建进闸门有一个具体理由：因为 crate 拒绝警告，一个坏掉的文档内链接就是
**错误**，而一个公开条目的文档不可以链到私有条目。把条目在模块之间搬动、或者收
窄它的可见性，都会弄断指向它的链接。修法是：如果目标仍公开，就限定路径
（`` [`x`] `` → `` [`x`](App::x) ``），否则把链接降成普通的 code span
（`` [`x`] `` → `` `x` ``），让散文仍然点到它的名。

## 工作方式

两条实践塑造了这套代码库。它们是指令，不是建议。

### 测试驱动开发

**铁律：没有先失败的测试，就没有生产代码。** 按 红 → 绿 → 重构 工作：

1. **红** —— 写一个最小的测试，命名你想要的行为，跑它，并且**看它因为正确的
   理由失败**（功能缺失，而不是测试里的一个笔误）。一个你没见它失败过的测试什么
   都证明不了。
2. **绿** —— 写*最小的*代码让它通过。不要投机性的功能。
3. **重构** —— 在测试保持绿色的前提下清理。

如果你在测试之前写了生产代码，删掉它，从测试重新推导。修 bug 不是例外：一次修
复以复现该 bug 的测试开始，否则你手上没有证据说明原因就是你以为的那个。

唯一的例外是 `src/main.rs` 和 `src/tui/` —— 终端 I/O 边界，它没有单元测试。那
里的改动靠跑应用和 smoke 套件来验证。

### 先设计再实现

「加个 X」说的是*什么*，不是「跳过设计」。对一个功能或任何非平凡的改动：探索现
有代码，一次一个地问清你的疑问，提出两三种方案并给出推荐，先就设计达成一致。然
后把它记下来 —— 新功能写一页新的 `docs/`，行为有变就把 `docs/design.md` 更新。

琐碎的、自明的编辑，以及原因显而易见的 bug 修复，不需要这套仪式。那个失败测试
仍然需要。

## 代码是怎么布局的

一个**库**（`src/lib.rs`）装着逻辑；**`src/main.rs`** 是覆盖在 **`src/tui/`**
上的一层薄壳，`src/tui/` 是驱动异步（`tokio`）`select!` 循环的、二进制私有的
树。

这个划分是让 crate 可测的原因，也是要守住的规则：纯的、有单元测试的逻辑放在
`app/`、`ui/`、`stream/`、`textarea`、`session`、`history`、`context`，以及
`frame`/`paste`/`subprocess` 的纯核心；终端 I/O 关在 `src/tui/` 和 `term.rs`
里。**把逻辑挡在边界之外** —— 边界需要做决定时，那个决定属于它调用的纯辅助函
数（live region 的几何就是这么工作的：`term.rs` 作用在 `ui::live_height`、
`ui::repin`、`ui::cursor_position` 上）。

| 位置 | 内容 |
| --- | --- |
| `src/app/` | 对话状态与纯更新逻辑 —— 每个领域一个模块，`App` struct 在 `mod.rs` 里，好让每个子模块保留它的私有字段访问 |
| `src/ui/` | 每一个渲染器，加上集中的主题与调色板 |
| `src/tui/` | 事件循环与 I/O 边界 —— 每个领域一个 `impl Session` 块 |
| `src/stream/` | 后端接缝（`ReplySource`、`StreamEvent`）与离线 demo 后端 |
| `src/llm/` | 真后端：agent 的 tool 循环、四种线格式、auth、MCP |
| `tests/` | 集成测试，包括内存闸门和那些标了 `#[ignore]` 的 live 测试 |
| `scripts/smoke/phases/` | 每个 smoke phase 一个文件 |
| `prompts/` | system prompt、内置的 agent 定义与内置 skill |

[`docs/architecture/module-layout.md`](docs/architecture/module-layout.md)
是完整的地图。

### 四条不变量

`CLAUDE.md` 在 *运行时模型及其不变量* 一节里完整地写了它们。它们不是风格偏好
—— 破坏一条就会重新引入一类在 diff 里极难看见的 bug：

1. **只有一个 stdin 读取者**，在 init 的光标查询之后创建。
2. **贪婪的单词换行是前缀稳定的** —— 追加只改动最后一行换行的结果，这正是让
   直接流向 scrollback 安全的原因。
3. **viewport 是内容锚定的**（顶部固定），而一次 resize 会从历史向两个方向重
   排。
4. **tool call 与文本交错**，而在全屏 overlay 之下做的提交是*排队*而不是绘制。

碰循环、`term.rs` 或者换行/markdown 渲染器之前，先读它们。

## 测试

`cargo test` 跑单元测试（约 6,500 个，就写在它们覆盖的代码旁边的 `#[cfg(test)]`
模块里），加上 `tests/` 里没有标 `#[ignore]` 的集成测试。它离线、确定 —— 保持
它这样。

[`AGENTS.md`](AGENTS.md) 把一个改动映射到能失败的最小检查，并给出实测耗时 ——
每次编辑跑那个，最后跑一次全量。它还说了 Mac 为这套件需要什么（构建好的二进
制，终端测试通过它启动各自的 session），以及 Linux 与 macOS 之间的差异，好让一
次红的跑被读懂它到底是什么。

| 套件 | 怎么跑 | 什么时候跑 |
| --- | --- | --- |
| 一个模块 | `cargo test --lib app::tests::login` | 每次编辑之后（构建好之后亚秒级；一次编辑后重新链接几秒） |
| 所有单元测试 | `cargo test --lib` | 改动中途跑一次完整的单元通过，不重建集成测试的二进制 |
| 单元 + 集成 | `cargo test` | 一次，在你说改动做完之前 —— 它是闸门的测试步骤（会重建每一个集成二进制，约 2 分钟） |
| smoke 套件 | `cargo build && scripts/smoke.sh` | 你改了 `src/tui/`、`term.rs`，或者任何只有在屏幕上才看得见的东西 |
| smoke 套件自己的测试 | `bash scripts/smoke/selftest.sh` | 你改了 `scripts/smoke/lib.sh` 或 `scripts/smoke.sh` —— 各 phase 的读取器和选择器，一秒之内 |
| live provider 测试 | `OPENROUTER_API_KEY=… cargo test --test live_openrouter -- --ignored --nocapture` | 你改了 provider、线格式或者上下文装配 |
| 内存闸门 | `cargo test --test image_turn_memory`、`--test model_parse_memory`、`--test image_paste_memory` | 你改了任何按回合、按请求或按图片分配内存的东西 |
| 发布工具 | `scripts/release.sh selftest` 和 `scripts/release.sh check` | 你改了 `scripts/release/` 或 `install.sh` |
| 内置 skill 脚本 | `python3 -m unittest discover -s prompts/skills/web-search/scripts -p 'test_*.py'` | 你改了 `prompts/skills/web-search/scripts/` |

### smoke 套件

`scripts/smoke.sh` 在 tmux 里驱动真二进制，并**对它在屏幕上画出的东西做断言**。
它是 I/O 边界唯一的自动化覆盖，所以那里的改动，在某个 phase 说出来之前，就是没
被测过。

```bash
cargo build && scripts/smoke.sh               # every phase, in parallel
scripts/smoke.sh 55 permission                # an id, a range (50-60), a name substring
scripts/smoke.sh --list                       # the phases, their tags and last durations
scripts/smoke.sh --failed --serial            # re-run what failed, one at a time
bash scripts/smoke/phases/055-permission.sh   # one phase on its own, output live
```

加覆盖就是加一个文件 —— `scripts/smoke/phases/NNN-slug.sh`，source
`scripts/smoke/lib.sh` 并调用 `smoke_begin`。id、slug 和标题都来自这个文件，所
以哪儿都不用注册。[`docs/ops/smoke.md`](docs/ops/smoke.md) 有辅助函数的词汇
表、phase 的契约，以及让一个 phase 不至于不稳定的那些规则。要内化的那条是：
**轮询状态，绝不定时睡一觉再指望。**

套件自己的机械也是测试：`bash scripts/smoke/selftest.sh` 覆盖各 phase 的读取器
（`smoke_phase_title`、`smoke_phase_tags`、……）以及作用在样本 header 上的查询
选择器。那些读取器决定一次查询跑*哪些* phase，所以一个什么都不返回的读取器会跑
错套件而不是失败 —— 那正是一次 BSD-`sed` 不兼容在当时被发现之前干的事。CI 在
smoke 任务里用 GNU `sed` 跑它，在 macOS 上用 BSD `sed`。

每个 phase 有自己的 tmux 服务器、配置主目录、session 根、`$HOME` 和临时树，并
在第一次启动之前净化环境（每一个 `*_API_KEY`、每一个 `ZEROTH_*`、`OLLAMA_HOST`、
`NO_COLOR`、各显示变量）。正是这一点让并行跑 phase 是正确的 —— 也正是它让一次
跑不碰你真正的剪贴板、配置和 `/resume` 列表。

### live 测试

每一个会碰网络的测试都标了 `#[ignore]`，好让 `cargo test` 保持离线。它们从环境
里读凭据。**永远不要提交 key**，并注意 `tests/live_caching.rs` 之流会花掉真
token，花在你指向的那个账号上。

## 约定

评审里会冒出来的那些：

- **样式是集中的。** 字形和几何是 `src/ui/theme.rs` 里的 `const`；每一种颜色都
  是读过活动主题的 `Palette`（`src/ui/palette.rs`）的同名访问函数。一个新颜色就
  是 `Palette` 上的一个新角色，要在*每一张*主题表里填上，再加上它的访问函数。
  绝不在 `palette.rs` 之外写字面的 `Color::Rgb` —— 一个字面量就是一个无视主题
  的颜色。
- **所有宽度计算都走 `cols()`**（经 `unicode-width` 得到显示列），绝不用
  `chars().count()`。一个宽字形还占着空白的影子 cell，所以每一次把 cell 交给
  backend 的绘制都要走 `term::visible_cells`；漏掉一处，那个 bug 就只在那一个视
  图里活着。
- **输入行是一个 `textarea::TextArea`，不是 `String`。** 编辑走它的方法，读取用
  `.text()`。
- **绝不为了取几个字段就把一个响应体建成 `Value` 树，也绝不为了做一张小图就把
  一张图整个解码。** 常驻内存在这里是一个特性 —— 进程整天在终端里闲着，而 glibc
  不会把一棵被释放的树的页还给操作系统，所以一次尖峰的解析就是一次*常驻*的解
  析。[`docs/architecture/memory.md`](docs/architecture/memory.md) 有测量数据和
  修正它的形状；那些 `tests/*_memory.rs` 闸门把它们钉住。
- **应用的名字是一个常量**，`zeroth::APP_NAME`。从别的工具搬过来的措辞会带着
  *那个* 工具的产品名 —— 重读一句搬来的话，看它说的是谁的名字。
- **tool schema 的散文短、具体、直接。** 每个词都要骑在每一个提供这个 tool 的请
  求上。说出能力和它的尖角，把填充去掉。
- **README 是前门，不是手册。** 一个功能落地不配一段 README：机制进 `docs/`，用
  户会注意到的东西进 `CHANGELOG.md`。

## 提交

一次提交一个逻辑改动，配一条读者能据以行动的信息：

- **一行短的祈使句标题**，说清这个改动做什么 ——
  `Blink the running tool bullet instead of breathing it between two greys`，而
  不是 `fix bullet`。
- **正文解释原来错在哪儿、为什么这是修法**：症状、原因、改动做了什么、它怎么被
  覆盖。历史是「代码为什么长成这样」的记录；为一个一年后读它的人写。`git log`
  里满是例子。
- **不写署名尾注，不写工具或助手名，不写 session 链接。** pull request 的标题和
  正文同理。

作者和提交者应该是你。不要动版本号，也不要编辑已发布的 `CHANGELOG.md` 段落 ——
`scripts/release.sh prepare` 在切发布时两样都做，而 CI 会检查 `Cargo.toml`、
`Cargo.lock`、README 徽章和 changelog 是一致的。

## Pull request

从 `main` 开分支（历史里把话题分支叫 `update/<topic>`）。开 pull request 之
前：

- 闸门是干净的 —— 四条命令；
- 如果你碰了任何屏幕上显示得出来的东西，smoke 套件通过；
- 新行为有一个你先看它失败过的测试；
- 这个功能的 `docs/` 页面写了或更新了，行为变了的话 `docs/design.md` 也更新
  了；
- `CHANGELOG.md` 在 `## [Unreleased]` 下、在正确的小节（Added / Changed /
  Fixed / Removed）里有一条。GitHub 上一次发布的说明是从那个小节生成的，所以你
  写在那里的就是用户读到的 —— 描述他们会注意到什么，而不是你重命名了哪个函数；
- diff 是你的改动，别的什么都没有。不要顺手重新格式化。

CI 在 Linux 和 macOS 上跑闸门，在 tmux 下跑 smoke 套件，跑发布工具
（`shellcheck`、`selftest`，以及版本/changelog 的 `check`），还有内置 skill 脚
本的测试。一次 smoke 失败会把每个 phase 的日志作为 artifact 上传 —— 从那里开始
查，而不是猜。

描述里说清改了什么、为什么、你怎么测的。如果一条评审意见其实是设计问题，在
issue 里解决比在 diff 里便宜。

## 发布

切一次发布是维护者的事，而且完全脚本化：`scripts/release.sh
prepare X.Y.Z`、一次提交、一个附注 `vX.Y.Z` 标签、一次 push。标签 push 会跑
`.github/workflows/release.yml` —— 闸门、每个平台一次构建（在 `dist`
profile 下，全程序 LTO），见
[`docs/ops/binary-size.md`](docs/ops/binary-size.md)；然后是校验和、验证，以及
一个 GitHub release，其说明就是那个版本的 changelog 小节。
`workflow_dispatch` 把整条流水线排练一遍而什么都不发布，而每一步在本地也是同样
的方式跑（[`docs/ops/release.md`](docs/ops/release.md)）。

## 报告安全问题

请不要为漏洞开公开 issue。用仓库上 GitHub 的私有漏洞报告（**Security → Report
a vulnerability**），好让修复在问题被公开描述之前发出去。

一个会跑命令的 agent 按设计就有很大的攻击面。权限闸门
（[`docs/state/permissions.md`](docs/state/permissions.md)）和项目配置的信任闸
门（[`docs/state/project-config.md`](docs/state/project-config.md)）是决定那片
面的地方，所以对这两者中任何一个的报告都值得提，即使你不确定它可利用。

## 许可证

Zeroth 以 [Apache License 2.0](LICENSE) 发布。通过贡献，你同意你的贡献以它授
权（Apache-2.0 §5），并且你有权提交它。
