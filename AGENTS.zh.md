# AGENTS.md

给编码 agent 的说明（Claude Code、Codex、Copilot、Cursor，任何会读 `AGENTS.md`
的东西）。`CLAUDE.md` 是架构地图，在**代码怎么工作**上说了算；这个文件在**该跑
什么**上说了算。改动某块区域之前，先读 `docs/` 里对应的那一页 —— 看起来随意的
地方，通常都是有人已经付过代价的 bug。

## 跑能失败的最小检查

**不要**每次改完都跑 `cargo test`、`cargo clippy --all-targets` 或整套 smoke
套件。按你动到的东西挑那一行；CI 在每次 push 时跑完整的闸门。一次带过滤的 lib
运行花的是重新链接（约 9 秒），不是重新构建。

在一台新机器上读一次红的运行之前，`scripts/preflight.sh --smoke` 每个必需工具
打一行（钉住的工具链、rustfmt/clippy、`mbx`、`tmux`、bash 4+、`setsid`），并
指名缺的那个该怎么修。

| 动到了 | 跑 |
| --- | --- |
| `src/app/<area>.rs`（纯状态） | `cargo test --lib app::tests::<area>` |
| `src/ui/<area>.rs`（渲染器、主题） | `cargo test --lib ui::` + 若可见，一个 smoke 阶段 |
| `src/llm/<provider or wire>` | `cargo test --lib llm::`（只有用户要求时才跑 live 测试 —— 它们花 token） |
| `src/tui/`、`src/main.rs` | `cargo build && scripts/smoke.sh <id>` + `cargo test --bin zeroth` |
| `src/term.rs` | `cargo test --lib term::` + 画它的那些 smoke 阶段 |
| 公开条目被移动/改名、`mod.rs` 重导出 | `cargo test --test api_surface` + `cargo doc --no-deps --lib` |
| `src/cli.rs`、`--help`、参数解析 | `cargo test --lib cli::` + `cargo test --test cli_help` |
| `src/headless.rs`、`src/tui/headless.rs`、任何与 `-p` 有关的 | `cargo test --lib headless::` + `cargo test --test headless` + `scripts/smoke.sh 153` |
| `src/pty/`、`src/subprocess.rs` | `cargo test --lib pty::` 或 `subprocess::` + `cargo test --test detached_exec`（Mac：先 `cargo build`） |
| `src/mcp/`、`src/llm/mcp/`、`mcp` CLI | `cargo test --lib mcp::` + `cargo test --test mcp_cli` |
| 一条对内存敏感的路 | 对应的 `tests/*_memory.rs` |
| `providers.toml`、`src/llm/config.rs` | `cargo test --lib llm::config` |
| `scripts/smoke/lib.sh`、`scripts/smoke.sh` | `bash scripts/smoke/selftest.sh` |
| `scripts/smoke/phases/` | `cargo build && scripts/smoke.sh <ids>` |
| `scripts/`、`install.sh` | 那个子树的套件（CONTRIBUTING *Testing*），不是 `cargo test` |
| `prompts/`、`CHANGELOG.md` | 什么都不跑；散文没有闸门 |
| 旁边有 `.zh.md` 的一页 | `python3 scripts/docs/zh_parity.py [path]` —— 对照英文原文检查围栏块、code span、链接目标、标题层级 |
| `scripts/docs/` | `python3 -m unittest discover -s scripts/docs -p 'test_*.py'` |

过滤器是子串：一个模块路径选中它之下的一切，所以 `app::tests` 覆盖每个区域，
代价与一个文件相同。当几个区域没有共同前缀时，在 `--` 之后传好几个
（`cargo test --lib -- app::tests::login term::`）。不要在一次编辑里留下两次带
过滤的运行；把过滤器放宽。一次横跨若干行的改动跑它们的**并集** —— 不是因为它跨
了边界就跑整套。

当你想要一次宽的本地通过而不是一行时：`cargo test --lib --
--skip pty:: --skip background::tests:: --skip llm::exec:: --test-threads=16`
—— 8 核 Mac 上约 49 秒跑 6,072 个测试。这套件是等出来的（固定 sleep），所以更
多线程买不到多少，还会让计时测试变得不稳定；完整的 `cargo test` 是 CI 的。

## 在你宣布做完之前

CI 跑的闸门：`cargo fmt --check`、`cargo clippy --all-targets -- -D
warnings`、`cargo test`、`cargo doc --no-deps --lib`。在本地跑 fmt、clippy 和
doc；把完整的 `cargo test` 留给 CI，除非用户要求或 CI 不可用。`Cargo.toml`
deny 警告、`clippy::all` 与 `unsafe_code`，而一条坏掉的 intra-doc 链接就是
错误。

做 cherry-pick 或 rebase 时，先比较两棵树（`git diff <old> <new>
-- ':!docs' ':!CHANGELOG.md'`）；代码相同就不必重测。8 核 Mac 上的热态开销：
`cargo check --lib` 约 6 秒；一次带过滤的 lib 运行热态约 0.4 秒 / 一次编辑之
后约 9 秒；`cargo test` 约 2 分 20 秒；一个 smoke 阶段约 2–20 秒；release/dist
构建是分钟级，只在做发布二进制的任务时（`docs/ops/binary-size.md`）。`target/`
是一个约 10 GB 的增量缓存 —— 绝不 `rm -rf` 它。

## Linux 与 macOS

CI 在 Ubuntu 上跑闸门和 smoke，在 macOS 上跑闸门（不含 fmt）加上剪贴板测试和
smoke 的读取器。Linux 是参照；两者都应该是绿的。

- **macOS 没有 `setsid`。** 终端测试通过构建出来的 `zeroth` helper 启动它们的
  session。`cargo test` 会构建它；`cargo test --lib` 不会，所以先跑
  `cargo build` —— 否则约 46 个终端测试会以「no way to give the command a
  terminal of its own here」失败。改了 helper 之后要重建。
- **38 个测试只在 Linux 上跑**（`cfg(target_os = "linux")`）：`llm/exec` 18、
  `pty/probe` 8、`clipboard` 4、`subprocess` 4、`background` 2、`pty/spawn` 1、
  `llm/mcp/transport` 1。一次 macOS 全绿对 `/proc` 探测、流式剪贴板、`setsid`
  detach 或 Linux 执行器什么也没说。在提交里说明你跑的是哪个平台。
- **BSD 工具：** `pkill` 没有 `-s`（它的 `-t` 什么也匹配不到），`ps` 没有
  `sid`，`od` 会给它的列补位。不要依赖 GNU 的形式。
- **Smoke 需要 bash 4+**（macOS 的 `/bin/bash` 是 3.2）。动过读取器或选择器
  之后，跑 `bash scripts/smoke/selftest.sh`。
- **套件的脚本已移植到两个平台**（`scripts/smoke/lib.sh` 里的 `smoke_*` 辅助
  函数；`docs/ops/smoke.md` *Portability*）。一个直接调用 `stat -c`、`sed -i`
  或 `sha256sum` 的阶段就是个 bug —— 在 macOS 上它们会以空字符串失败，把一条
  断言变成假通过，而不是一次红的运行。
- **切分支之后一次陈旧的增量链接**（`Undefined symbols: … .llvm.…`）是 codegen
  问题：`cargo clean -p zeroth`，绝不 `rm -rf target/`。
- **网络/剪贴板/X11 测试是 `#[ignore]` 的。** 绝不要给一次例行运行加
  `--ignored` —— 它们需要 key、花真 token，还会覆盖剪贴板。

两个假警报：在一台全新的 Mac 上「我的改动弄坏了 46 个测试」（先构建二进制），
以及对 `pty`/`exec`/`subprocess`/`background`/`clipboard` 说「它在 macOS 上
通过」（那些代码有一半在那里从没编译过）。还有，别在 Mac 上相信 GNU 工具的
输出。

## 工作方式

来自 `CONTRIBUTING.md`：**TDD** —— 在你亲眼看到一个因正确原因失败的测试之前，
不写生产代码；例外是 `src/main.rs` + `src/tui/`，它们靠运行应用和 smoke 套件
来验证。**先设计再实现**一个功能或不平凡的改动：探索，一次问一个问题，提出
2–3 种做法，达成一致，写进 `docs/`，行为变化时更新 `docs/design.md`。**一次
提交一个逻辑改动**；不要 bump 版本，也不要改已发布的 changelog 小节。意图写在
`docs/`，用户可见的变化写在 `CHANGELOG.md`，README 是前门。

## 约定

- 样式：字形/几何作为 `const` 放在 `src/ui/theme.rs`，颜色作为 `Palette` 的
  accessor 放在 `src/ui/palette.rs` —— 别处绝不用 `Color::Rgb`；一个新颜色就是
  每张主题表里的一个新角色。
- 宽度走 `cols()`（显示列），每个单元格走 `term::visible_cells`；输入是一个
  `textarea::TextArea`。
- 只为读几个字段的主体，不建 `Value` 树；缩略图不做整图解
  （`docs/architecture/memory.md`、`tests/*_memory.rs`）。
- `unsafe_code = "forbid"`；终端与 pty 走安全的 `rustix` 包装，不用 `pre_exec`。
- 决定属于一个边界所调用的纯辅助函数 —— 把逻辑挡在 `tui/` 和 `main.rs` 之外。

## 不要

- 不要「为了检查」一个小改动就跑整套 `cargo test` 或 smoke 套件。
- 不要 `rm -rf target/`，不要改 `incremental`、`[profile.*]` 或
  `rust-toolchain.toml`；`cargo clean -p zeroth` 能修好陈旧的链接。
- 不要跑 release/publish 命令（`scripts/release.sh` 的
  prepare/build/download、push）—— 那些是用户的决定。动它的时候，
  `scripts/release.sh selftest`/`check` 是没问题的。
- 不要为了变绿而加 `#[ignore]` 或 `#[allow]`；修好原因，或者说你被卡住了。
