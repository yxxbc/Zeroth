# Spinner tip —— 回合运行时状态行下的一行提示

Claude Code 在它干活时把一行 dim 的 `⎿  Tip: …` 挂在 spinner 上：一个值得知道的
按键或命令，在用户正盯着屏幕、无事可做的那一个时刻奉上。这一行就是它。

```
❯ refactor the parser

⣤⣀⣀⣀⣀⣀⣀⣀ Working… (5s · ↓ 120 tokens · esc to interrupt)
  ⎿  Tip: Press ctrl+o to see the whole transcript and every tool's output

────────────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────────────
```

## 显示什么，何时显示

- **不是立刻。** 一个回合起初只有状态行；提示在回合跑过 [`TIP_DELAY`]（3 s）之后
  才出现。一个快速答案从不让它闪一下——它是为真正在等的人准备的。三秒正是 Ctrl+B
  提示已经在等的那「几秒」（`TOOL_BACKGROUND_HINT_DELAY`，
  `docs/tools/background.md`），于是条带上两个延迟提示保持同一个节奏。
- **每回合一个提示，之后同一个回合里每 [`TIP_ROTATION`]**（3 分钟）换一个新的
  ——从提示出现时算起。多数回合在第一个槽位之内就结束了，所以用户多数时候看到的
  是*下一回合的下一提示*；一个长的 agentic 回合每几分钟得到一个新的，慢到永不把
  视线从它上面流出的答复上拉走（动词已经每 30 s 换一次，
  `docs/ui/status-indicator.md`）。
- **它挂在状态行上**——正好在它下面，在工具输出所悬挂的那条 `⎿` 栏里，在状态槽位
  之内、它尾随的空隙之上。全程 dim，在一个窄到放不下它的终端上在栏下换行（在 80
  列下每条提示都装得进一行）。
- **只在 live。** 回合结束它就走，且从不被提交：没有 scrollback 行、没有历史条目、
  Ctrl+O 里没有、rollout 和上下文里也没有。
- **哪里不显示：**
  - **task 清单**立着时——计划占了那个槽位：它才是更值得挂在 spinner 上的东西
    （`docs/tools/task-tools.md`）；
  - 在一个 **`!` shell** 回合上，它没有状态行可挂
    （`docs/input/shell-command.md`）；
  - 在一个 **agent session view** 里面，它的状态行是 subagent 的
    （`docs/agents/agent-tool.md`）；
  - **Show tips** 关掉时（见下）。

  一个 `/compact` 回合是普通的状态行回合，会显示一个。

## 目录

`tips::TIPS`——每条一句话，按它们被走过的顺序。每个条目都点名应用真有的一样东西：
一个键（`ctrl+o`、`shift+tab`、`alt+↑`——`?` 带上的拼法）、一个斜杠命令，或一个
CLI 形式，而单元测试把目录扣在读者所依赖的规则上：

- 在 `⎿  Tip: ` 前缀下**80 列装一行**；
- **它点名的每个 `/command` 都在 `COMMANDS` 里**，于是一个被改名的命令不能让某条
  提示指向虚无；
- 没有重复、没有空的、没有句尾句号（Claude Code 的提示读起来像句子的片段，这些
  也是）。

顺序把键、命令和命令行交错开，好让邻居不同，并以那条通往所有其他提示的提示开头
（`?`，即快捷键）。确定性的——没有 RNG，动词表的规则。

## 这次行走

应用在目录里保留一个**游标**：下一个显示的提示从哪个下标开始。一个回合在它的槽位
第一次上屏的那一帧**画**一条提示——槽位即时钟处在回合已用时间（过了延迟之后）
的哪个 [`TIP_ROTATION`] 里——而这次绘制取游标处的提示并把游标往前移一格。之后的
槽位再画一次；同一槽位里的帧保留它画过的那条。

两个后果随之而来，都是刻意的：

- **游标只在一条提示被看见时才移动。** 一个在延迟之前就结束的回合什么都不画，于是
  下一个回合显示它本该显示的提示——一连串快速答案不会在没人看见的情况下烧穿目录。
  同样，一个耗在 task 清单之下、或 tips 关掉的回合——以及任何条带根本不在屏上的
  帧：一个权限 prompt 或一个 `AskUserQuestion` 模态替换了整个 live 区域，而 Ctrl+O、
  Ctrl+D 或 `/resume` 盖住它，所以这次绘制等对话回来
  （`App::modal_open`、`View::is_overlay`）。
- **一条提示在被藏起来时不变。** 它是为它的槽位画的，于是一个来了又走的清单，或者
  Show tips 关掉又打开，回来的还是同一条提示，而不是下一条。

这一切都跑在 `App::set_status_times` 里——边界时钟抵达状态的唯一地方
（`docs/ui/status-indicator.md` 的 *Where the impurity lives*）——所以纯核心保持
确定性，而测试用普通 `Duration` 驱动它。

`App` 的游标是一个 `Option`，**在边界播种之前是 `None`**（`App::seed_tips`）：
单元测试的默认值，完全像页脚的 session 信息，于是每个跑一个回合过三秒的既有测试
看到的条带与它一直看到的一样。

## `tips.json` —— 按用户

```json
{
  "enabled": true,
  "next": 7
}
```

`{config_home}/tips.json` 装着 **Show tips** 开关和游标。它是按*用户*的，在
`telemetry.json` 和 `update.json` 旁边（`docs/state/per-directory-state.md` 的
*What is not per directory*）：你要不要提示是你的事，不是项目的事——一个在一个
目录里关掉、在下一个目录里又打开的开关会读起来像设置不生效——而这次行走应当跨
session 和目录继续：下一个 session 从任何地方最后显示的那条之后开始，于是每条提示
在任何重复之前都会轮到一次——一个轮转，不保留每条的历史。

边界在 bootstrap 时读它（既为设置也为游标播种），并**在游标移动过的每次循环底部
把它写回**——在 `Session::after_iteration` 里别的磁盘镜像旁边，每隔一次迭代做一次
整数比较。每次写入都是对文件的一次读-改-写（`config::update_tips_file`，
`update.json` helper 的孪生），于是一个并发 session 的开关绝不会被这个 session 的
游标冲掉。缺失、读不了或损坏时，文件读作默认值——开，从第一条起——而一个越过一个
缩小过的目录末尾的游标，就简单地回绕。

## 关掉提示

- **`/settings` → Show tips** —— `true`/`false`，持久化到 `tips.json`（绝不是
  `settings.json`：`SessionSettings::tips` 是 `#[serde(skip)]`，`copy_value` 从不
  搬动它——Telemetry 行的模式）。回合中途切换，这一行在下一帧就走（或来）。
- **`ZEROTH_TIPS=0`** 为一次运行把这一行播种成关，从不保存——
  `ZEROTH_TOOLS` 的语法。不是隐私开关，所以与 `ZEROTH_TELEMETRY` 不同，
  它不撤回这一行：用户仍可为这个 session 把提示重新打开。
- smoke 套件在它的基础环境里导出 `ZEROTH_TIPS=0`
  （`scripts/smoke/lib.sh`），于是一个把回合按过三秒的 phase 看到的正是它所断言的
  条带；tips 那个 phase 把它们重新打开。

## 各部分住在哪

| 部件 | 位置 |
|---|---|
| 目录、[`TIP_DELAY`]、[`TIP_ROTATION`]、槽位算术、`tips.json` 格式 | `src/tips.rs`（纯） |
| 这次行走：游标、绘制、什么藏起这一行 | `src/app/tips.rs`（`App::set_status_times` 调它） |
| 这一行：`  ⎿  Tip: …`，在栏下换行 | `src/ui/tips.rs` |
| 它的高度：状态行的**悬挂行**——清单，否则提示——像清单那样穿过 `live_height`/`live_layout`/`cursor_position`/条带 | `ui::hang_rows`（`src/ui/layout.rs`） |
| 播种、持久化、`/settings` 的应用、`ZEROTH_TIPS` | `src/tui/bootstrap.rs`、`src/tui/settings.rs`、`src/tui/config.rs` |

清单和提示共用一个槽位，所以它们共用一个行数：`strip_rows`/`live_height`/
`live_layout`/`input_box` 在以前只取清单的地方改为取**悬挂行**，而每个调用方都交给
它们 `ui::hang_rows`——那一个求和，只拼写一次，于是区域所保留的行和 `strip_lines`
所画的行不可能不一致。条带的 scrollback 流动（`docs/ui/strip-flow.md`）不把提示算
进去：这一行坐在条带里靠下的位置，被挤压的条带会把它留在屏上，而一条提示的出现
只改变条带的高度——流动自己会重新算号。

## 测试

单元：`src/tips.rs`（延迟和轮转边界处的槽位算术、上面那些目录规则、宽松的
`tips.json` 解析及其默认值）、`src/app/tests/tips.rs`（延迟之前或未播种时没有提示、
绘制与游标、每槽一条、下一回合的下一提示、提前结束的回合把游标留住、每一种隐藏
情形——包括一个模态和一个 overlay——隐藏之后回来的是同一条提示）、
`src/settings/tests.rs`（这一行：label、默认、循环、从不进 `settings.json`）、
`src/ui/tests/tips.rs`（这一行的文字与 dim 装扮、换行、高度穿过 live 区域、条带、
以及一个 picker 在自己上面的条带）。

`scripts/smoke.sh` Phase 125 驱动那个二进制：`ZEROTH_TIPS=0` 让一个回合没有
提示且 `tips.json` 不被写；在一个全新的 config home 上，回合的头几秒没有提示，然后
三秒过去后目录的第一条直接出现在状态行下面、压在槽位的空隙之上，随回合一起消失、
从不进 scrollback；下一回合在下一条上，`tips.json` 记录了游标；一次重启动把这次
行走接着走；`/settings` → Show tips 把这一行关掉、把这一点持久化到 `tips.json`
（不是 `settings.json`）而重启动保持关；而 `ZEROTH_TIPS=1` 为一次运行重新打开
且什么都不保存。

[`TIP_DELAY`]: ../src/tips.rs
[`TIP_ROTATION`]: ../src/tips.rs
