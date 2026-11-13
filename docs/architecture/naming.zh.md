# 命名 —— 收尾 Zeroth 的改名

日期：2026-11-14

## 目标

Zeroth 是 Alter Zero 的一个 fork。改名已经走到了用户最先看见的那部分 ——
`Cargo.toml` 的包名与二进制名、[`crate::APP_NAME`](../../src/lib.rs)、
`~/.zeroth/` 这个配置 home、`prompts/zeroth.md`，以及那个一次性把三份身份文件
从旧 home 拷出来的 `migrate_from_alter_zero()` —— 然后就停住了。留下的是第二套
身份，而且是过期的：73 个 `ALTER_ZERO_*` 环境变量、session 的临时根目录
`{tmp}/alter-zero-{uid}`、`.alter-zero-paste-*` 这个中转文件前缀、
`alter-zero/{version}` 这个 User-Agent，以及一大串仍然写着旧名字的文档、
smoke 断言和测试 fixture。有几个 smoke 阶段断言的是字面的 `Alter Zero` 横幅，
而 [`APP_NAME`](../../src/lib.rs) 早就不再绘制它了，所以它们本来就在以正确的原因
失败。

这次改动一刀把改名做完：程序用来指*自己*的每一个名字都变成 `Zeroth` / `zeroth` /
`ZEROTH_*`。它刻意是一次**没有兼容垫片的破坏性变更** —— 旧拼写在运行时不会被
读取、起别名或迁移。

## 决定：一刀切完，不留别名

另一种做法是双读层（优先 `ZEROTH_*`，仍然认 `ALTER_ZERO_*`）。它被否掉了：
那会让代码里永远留着两个名字，让将来每一个覆盖项都含义不明，还会让权限与
hooks 的载荷一直带着一个没人分得清是否已废弃的字段。一个版本停在 `0.0.0`、
自己没发布过任何 release 的 fork，没有值得付这个代价的装机量。

`src/tui/config.rs` 里那个一次性的 home 迁移**不**在这次改名之内，保留原样：
它是从*上一个应用*的目录做的一次数据导入，不是本应用配置 home 的别名。它只读
`~/.alter-zero/`，从不写它，而旧应用照样能靠着它工作。

## 映射

| 改前 | 改后 | 适用处 |
| --- | --- | --- |
| `ALTER_ZERO_*` | `ZEROTH_*` | 全部 73 个覆盖变量、hook 载荷字段、文档、smoke 阶段 |
| `alter-zero-{uid}` | `zeroth-{uid}` | `scratchpad::session_root`、`is_session_scratchpad`、prompt 与文档布局 |
| `.alter-zero-paste-*` | `.zeroth-paste-*` | `clipboard.rs` 的中转文件前缀 |
| `alter-zero/{version}` | `zeroth/{version}` | 遥测的 User-Agent（`TELEMETRY.md`） |
| `alter-zero`（命令、路径、临时目录） | `zeroth` | CLI 示例、smoke fixture、测试路径、文档 |
| `Alter Zero`（作为应用名） | `Zeroth` | 横幅、help、回调与 banner 相关文档，以及 smoke 标记 |
| `.alter-zero/`（作为当前的配置根） | `.zeroth/` | 描述*当前* home 的文档与 fixture |
| `git clone https://github.com/linuztx/alter-zero.git` | `yxxbc/zeroth` | `CONTRIBUTING.md` —— 现在正在开发的仓库 |
| `docs/ops/update.md` 里的上游 release URL | `yxxbc/zeroth` | 更新通道；`DEFAULT_REPO_URL` 已经读 `CARGO_PKG_REPOSITORY` |

一个 `tests/naming.rs` 契约测试钉住了新的拼写。它在这一刀之前是失败的
（生产代码仍然返回 `alter-zero-{uid}`），也是整场清扫的闸门。

## 刻意*不*改名的东西

这些名字指的是**上游项目或历史事实**，不是本应用。改写它们会造出坏掉的链接、
错误的归属和一份错的法律声明：

- `NOTICE` —— Apache-2.0 的衍生作品归属声明。
- `README.md` 的 Fork 说明与 `CHANGELOG.md` 里的“inherited from Alter Zero”，
  以及 changelog 末尾那两条历史链接定义
  `github.com/linuztx/alter-zero/compare/…` 与 `releases/tag/v0.1.0`。
- `docs/architecture/multithread-core.md` 里的上游 issue 链接，以及
  `docs/ui/links.md` 和 dummy 后端文档里作者的 `github.com/linuztx` 主页。
- `src/update.rs` 里的 `NPM_PACKAGE = "@linuztx/alter-zero"`：那是更早的 release
  *实际发布时用的*名字。`is_package_manager_install` 与 update 命令都要靠它
  认出一个 npm 装出来的安装。等 Zeroth 自己的 npm 包存在了，它就会改。
- `migrate_from_alter_zero()` 及它读取的 `~/.alter-zero/` **源**路径、
  `prompts/skills/README.md` 里关于来自那个 home 的无授权个人 skill 的说明、
  `scripts/release/notes.sh` 里历史的 `**Alter Zero Initial Release**` 标题，
  以及 `docs/research/harness-security-threats.zh.md` 里记录下来的 rollout
  路径。这些都是对过去的描述。
- `src/update.rs` 的 `is_cargo_build_dir` fixture *不是*历史的：它们描述的是
  本仓库的构建树，随其他一切一起改名。

## 破坏性变更

- 每一个 `ALTER_ZERO_*` 覆盖项都失效。设了它的脚本、hook 和 CI 必须换成
  `ZEROTH_*` 的拼写。`docs/architecture/environment.md` 与 `docs/cli/cli.md`
  载有新的名字。
- 续上的对话，它的 scratchpad 会挪到新的 session 根目录。更早的 session 在
  `alter-zero-{uid}` 下写过的文件不再豁免写权限 prompt，也不再显示为
  scratchpad 编辑。
- 遥测的 User-Agent 变了；任何以旧字符串为键的允许清单都必须更新。
- 剪贴板的中转文件换了新前缀；一个遗留的 `.alter-zero-paste-*` 文件只是再也
  不会被新名字的清扫回收。

## 验证

`cargo test --test naming` 钉住这份契约；带 pty/background/exec 跳过项的
`cargo test --lib --` 覆盖改名后的 fixture；`cargo fmt --check`、
`cargo clippy --all-targets -- -D warnings` 与 `cargo doc --no-deps --lib` 是
CI 闸门。`python3 scripts/docs/zh_parity.py` 让双语文档保持同步，
`bash scripts/smoke/selftest.sh` 检查改名后的 smoke 辅助脚本。在 macOS 上那
38 个只跑 Linux 的测试根本不会编译，所以 Linux 闸门仍必须在 CI 里跑。
