# 按目录的状态 —— `/model`、`/settings`、`/mascot` 和 `/spinner` 记住目录

你选的模型和你设的旋钮是关于一个**项目**的事实，不是关于你的：需要前沿模型的那
个 repo 旁边是跑一个便宜本地模型的临时目录，想要 formatter hook 的那个项目旁边
是你从没审过的 clone。在此之前两个文件都是对整用户的一个 blob——一个终端里的
`/model` 切换改变了其他每个目录起步时的样子，而为一棵大树关掉 checkpoints 就把
它们处处关掉。`config.json` 和 `settings.json` 现在按工作目录为键，正如
`permissions.json` 和 `skills.json` 本来就是这样——`mascot.json` 和
`spinner.json` 这两个 **look** 也是（`docs/ui/mascot.md`、
`docs/ui/spinner.md`）：一个项目所穿的横幅和它跑的模型一样是它自己的，而一个终端
里选的 `/mascot` 以前会在其他每个项目下次启动时重绘它们的横幅。

## 规则

这里每个文件都保留一个**以该 cwd 绝对路径为键的 `projects` map**，以**读-改-写**
的方式读取和写入（重读文件、替换这个目录的条目、写回），于是两个目录里的两个
session 永不互相冲掉。每个文件的顶层保留它旧的形状，所以在这次改动之前写下的文件
仍然能加载——而两个原有的文件对「一个我从没在其中启动过的目录从什么起步？」的回答
不同，这是刻意的（两个 look 文件站 `config.json` 那边，条目对条目——见下文
*`mascot.json` 和 `spinner.json`*）：

| | `config.json`（`/model`） | `settings.json`（`/settings`） |
|---|---|---|
| 一个目录自己的条目 | `projects[cwd]` | `projects[cwd]` |
| 一个没有条目的目录从……起步 | 任何地方最后一次做出的选择——顶层字段 | **种子**——顶层键，它们也正是改动之前的文件 |
| 一个目录何时拿到它的条目 | **在它第一次启动时**：最后的选择当场被钉成它自己的 | **在它第一个被循环的旋钮时**：种子被复制进来，那一个键被改动 |
| 什么会移动顶层 | 每次 `/model` 切换（它就*是*最后的选择） | 应用做的事一概不动——手工编辑它来改变新目录起步的样子 |
| 环境（`ZEROTH_*`） | 为这次运行胜出，从不写入 | 为这次运行胜出，从不写入 |

这个不对称来自顶层*是什么*。模型的「最后选择」随每次切换移动，所以一个只是*跟随*
它的目录会在你在别处选了什么东西时在你脚下改变——正是这次要移除的那种耦合——这
就是一个新目录在第一次启动时**钉住**它的副本、此后独立的原因。设置的种子永不移动，
所以一个没有条目的目录无法漂移；它的条目可以等到它真的改了点什么，而在此之前文件
保持是一份纯 diff。

模型有第三个轴，别的文件没有：session（`docs/state/session-model.md`）。目录的条目
是一个*新* session 起步所依据的，仅此而已——每次对话的 rollout 记录它实际所跑的
模型（一条只追加的 `model` 记录，形状与条目一样是 `ModelSelection`，随文件写入，
并在每次 `/model` 选择、Ctrl+T 循环和探测答复时写入），而 `/resume`、`--resume`
和 `--continue` 把那个模型带回来，不碰条目。于是同一个目录里的两个实例各自保留、
各自恢复自己的模型；条目只在一次 `/model` 选择时移动。

「一个目录」就是进程 cwd，正如 `permissions.json` 为它取键的那样
（`Session::cwd`，不是 git 根）：`repo/` 和 `repo/src` 是两个条目。

## `config.json`

```json
{
  "provider": "openrouter",
  "model": "openai/gpt-4o-mini",
  "thinking": { "supported": false },
  "speed": {},
  "projects": {
    "/home/user/work/api": {
      "provider": "a0_venice",
      "model": "llama-3.3-70b",
      "vision": false,
      "context": 131072
    }
  }
}
```

`llm::settings::Settings` 是那个纯格式。顶层字段——那一对加上模型的 `thinking`
 blob、`vision`、`context` 和 `speed` blob（`docs/providers/reasoning.md`、
`docs/tools/tools.md`、`docs/architecture/compact.md`、
`docs/providers/fast-mode.md`——列出的速度档与 `/fast` 的选择，`{}` 是「已知不列
任何档」的那个标记）——是任何地方最后一次做出的选择；每个 `projects` 条目是一个
`ModelSelection`，同样的六个字段、那一对是必需的。五个纯操作，各有单元测试：

- `selection_for(dir)` —— 该目录的条目，否则最后的选择。
- `adopt(dir)` —— 当 `dir` 没有条目时，把最后的选择钉成它的条目；改变了文件时
  返回 `true`（边界只在此时写入）。
- `record(dir, sel)` —— 在 `dir` 里做出的一次 `/model` 选择：它的条目**以及**最后
  的选择。
- `record_capabilities(dir, sel)` —— Ctrl+T 或启动探测所学到的关于 `dir` 所记录的
  那个模型的东西：只在它点名同一对时替换条目（于是一个被 env 覆盖的选择永不写回），
  并且只在*它*也是那一对时，随之移动最后的选择——于是一个新目录不需要探测就能
  播种。
- `last()` / `project(dir)` —— 两半，读回来。

在边界处（`tui::config`、`tui::models`）：`ModelSession::resolve` 调
`adopt_selection`（加载 → `adopt` → 变了就写 → 条目），而下游一切——env 对已保存
的优先级、`selection_is_saved`、thinking/vision/context 的种子、
`persisted_selection`——读那个条目，与它以前读那个扁平文件完全一样。`switch_to`
经 `save_selection`（`record`）写，`persist` 经 `save_capabilities`
（`record_capabilities`）；两者都是读-改-写。`ModelSession::project` 是那个键。一个
恢复的对话随后经 `ModelSession::restore` 用它自己记录的模型覆盖条目——一次减去写入
的切换，所以条目保持意指「这里一个新 session 起步所依据的」
（`docs/state/session-model.md`）；`persisted_selection` 仍点名条目的那一对，正是
它让一个跑着另一个模型的 session 里的 Ctrl+T 不会把那个模型写到条目上。

## `settings.json`

```json
{
  "hide_thinking": true,
  "projects": {
    "/home/user/work/api": { "hide_thinking": true, "error_retry": 5 },
    "/home/user/work/huge-monorepo": { "hide_thinking": true, "checkpoints": true }
  }
}
```

`settings::SettingsFile` 是那个纯格式：`seed`（摊平的顶层键）压在 `projects` 上，
后者每个是一个 `SessionSettings`。一个条目是一个**整块**——相对*默认值*的一份
diff，像种子本身一样——绝不是种子之上的一层。diff 格式表达不了分层：一个停在默认
上的 `true` 和一个刻意选的 `true` 无法区分，所以一个种子说 `error_retry: 5` 的目录
会没有办法说「回到 3」。这也是一个条目**即使等于默认也保留**的原因——丢掉它会让
种子重新进来，而一个把某个旋钮循环回默认的目录，已经选定了那个默认。

- `settings_for(dir)` —— 条目，否则种子（可用性是边界随后注入的宿主事实）。
- `record_value(dir, key, live)` —— 取条目（否则种子），从活 blob 里只搬过
  **`key` 的值**（`SessionSettings::copy_value`），把它留作条目。

第二个操作是写入路径（`tui::config::save_setting`，来自
`Session::apply_setting`），而正是它让一个 `ZEROTH_*` 覆盖不粘住：活 blob
携带环境，但只有被循环的那个键跨过去。重读的文件现在是基础——session 不再保留自己
的 `saved_settings` 副本，因为文件里这个目录的条目*就是*那份副本，而重读它也保住了
同一目录里第二个 session 期间保存的东西。

## `mascot.json` 和 `spinner.json`

```json
{
  "mascot": "sprout",
  "projects": {
    "/home/user/work/api": { "mascot": "bloom" }
  }
}
```

`spinner.json` 在 `spinner` 键下是同样的形状。每个都完全是 `config.json` 的模型
——一个 look 是没人会手工编辑种子的偏好，所以一个新目录应当从*你上次选的那个*
起步，而随后必须独立于你接下来选的东西：顶层是任何地方最后一次做出的选择（也正
是在 look 按目录之前应用所写的那个单值文件，于是一个旧文件仍能加载并为每个目录
播种），一个首次在其中启动的目录在启动时**钉住**最后那个作为自己的条目，而在一个
目录里做出的选择是条目*和*最后。一个条目与顶层形状相同，这正是让一个解析器读两者
的原因。

纯格式是一个泛型，`app::LookFile<T>` 压在 `app::Look` trait 之上（`KEY`——JSON
键、`name`、`from_name`——由 `Mascot` 和 `Spinner` 实现），因为两个目录按设计是
孪生；`app::MascotFile` 和 `app::SpinnerFile` 是它的实例。它的操作镜像
`config.json` 的，各有单元测试：

- `choice_for(dir)` —— 该目录的条目，否则最后的选择。
- `project(dir)` —— 只取条目。
- `adopt(dir)` —— 当 `dir` 没有条目时，把最后那个钉成它的条目；改变了文件时返回
  `true`（边界只在此时写入）。
- `record(dir, look)` —— 在 `dir` 里做出的一次选择：它的条目**以及**最后。
- `parse` / `to_json` —— 进来时宽松（一个损坏的文件读作什么都没选，一个未知名字
  只花掉那一个值），出去时好看、选择键在前，于是一个没有条目的文件逐字节就是旧的
  单值文件。

在边界处（`tui::config`）：bootstrap 时的 `adopt_look::<T>(path, cwd)`（加载 →
`adopt` → 变了就写 → `choice_for`），在第一帧提交横幅之前；`save_look(path, cwd,
look)` 来自两个 picker 的 Enter（`tui::mascot::Session::select_mascot`、
`tui::spinner::Session::select_spinner`）——两者都是读-改-写，两者都尽力而为，一个
`None` 路径（没有 config home）禁用持久化，别的都不影响。

### Checkpoints 关，直到某个目录打开；hooks 开

两个默认值都随这次改动从 `true` 翻成 `false`。每一个都代表用户去跑*代码*——一个
`hooks.json` handler 围绕每次 tool call，一个整 cwd 的 `git add -A` 在第一帧之前
——而两者看起来都不是你希望在一个你并未为它选择过的目录里开着的东西：为一个项目
写的 hooks 文件在每个项目里触发，而为一棵你只是打开来读一个文件的树做 checkpoint
存储，启动时花掉几秒，还要在 `~/.zeroth` 下留一份树的拷贝。有了按目录的设置，
opt-**in** 就是想要它的那个目录里的一次 `/settings` 循环，记在该目录的条目里
（`"checkpoints": true`；diff 谓词随默认值一起翻转，于是文件仍只记录变了的东西）。

**Hooks 后来改回默认开。** 用户自己的 `hooks.json` 是他们自己的配置——Claude Code
不问就运行它（codex 改为在启动时请用户信任每个新增或改动的 hook），而一个在隐藏
开关被翻转之前什么都不做的 hooks 文件读起来像坏的——而 opt-in 所防的那个危险，一个
被克隆项目的 `hooks.json` 在它第一次 tool call 时运行，是启动复核的职责：在用户
信任它之前没有 hook 运行（`docs/state/config-review.md`）。一个不该运行它们的目录
opt **out**，记作 `"hooks": false`；hooks 字段的 diff 谓词随它的默认值翻了回去
（`is_true`），否则一个 opt-out 永远到不了文件，而下一次启动又会把 hook 打开。

**MCP 采取同样的姿态**（`docs/tools/mcp.md`）：默认开，每个服务器仍只在被启用之后
才连接——一个项目的要经启动复核——而一个一概不要它们的目录循环一次退出，记作
`"mcp": false`，这会让每个服务器自己的 `/mcp` 开关保持原样。`ZEROTH_MCP` 为
一次运行播种该行，正如 `ZEROTH_HOOKS` 播种 Hooks，且从不保存。

边界处的两个后果：

- **`ZEROTH_HOOKS` 播种 Hooks 行**（`apply_setting_overrides`，
  `ZEROTH_TOOLS` 的模式），而不是在它之上做 AND。旧闸门只能把 hooks *关*掉；
  一个种子为一次运行打开或关掉它们——`smoke.sh` 的 hooks phase 仍然像
  checkpoint phase 设 `ZEROTH_CHECKPOINTS=1` 那样设 `ZEROTH_HOOKS=1`，
  于是一个目录保存的 opt-out 绝不会跳过它们。像每个覆盖一样，它为这次运行胜出且
  从不保存。
- checkpoint 拒绝的 toast（`Checkpoints off — …`）默认是沉默的，因为它以前只在用户
  要求过 checkpoints 时才升起；一个把它们打开过的目录仍会听到为什么没拿到它们。

## 什么*不*按目录

- **`.env`** —— 一个 key 是用户的，不是项目的；`/login` 不变。
- **rollout 的 `model` 记录** —— 一个*对话*所跑的模型
  （`docs/state/session-model.md`）：`config.json` 的条目为一个新 session 播种，
  此后 rollout 携带 session 自己的，而一次恢复恢复它而不是条目。
- **`theme.json`** —— 颜色是用户的：一个主题是与用户所坐的终端相配的，不是与一个
  项目（`docs/ui/theme.md`）。
- **`permissions.json`**、**`skills.json`** —— 本来就按项目；不变。
- **`telemetry.json`** —— 匿名每日 ping 的开关和 install id（`TELEMETRY.md`）。
  一个只作用于你碰巧所在目录的退出会是个意外，所以 `/settings` 的 **Telemetry**
  行是唯一按*用户*持久化的旋钮：`SessionSettings::telemetry` 是
  `#[serde(skip)]`，`copy_value` 从不搬动它，而边界只从 `telemetry.json` 播种它、
  也只写回它。
- **`update.json`** —— 每天一次的更新检查的开关与记录（`docs/ops/update.md`）：
  同样的按*用户*规则，同样的理由，`SessionSettings::update_check` 与 `telemetry`
  并排是 `#[serde(skip)]`。
- **`tips.json`** —— spinner tip 的 **Show tips** 开关，以及目录的行走走到了哪里
  （`docs/ui/tips.md`）：也是按*用户*——一个在一个目录里关掉、在下一个目录里又打开
  的开关会读起来像设置不生效——而行走应当在下一个 session 开始的地方接着走——
  `SessionSettings::tips` 与其他两个并排是 `#[serde(skip)]`。
- **项目级的 `.zeroth/`** 层（`docs/state/project-config.md`）是另一个轴：项目
  *里面*的文件，在启动复核之后（`docs/state/config-review.md`）。这里的两个文件
  都住在用户的 config home 里，按目录为键，不需要复核——它们里面没有东西会执行。

## 测试

纯格式有单元测试（`src/llm/settings.rs`、`src/settings/tests.rs`：扁平文件的兼容、
钉住、读-改-写、整块规则、新默认；两个 look 见 `src/app/tests/mascot.rs` 和
`spinner.rs`：旧的单值文件作为最后、只钉一次、条目与最后的记录、无条目形状的逐
字节相同、宽松解析）。`scripts/smoke.sh` Phase 109 对着一个 config home 从两个目录
端到端驱动边界：第一个里循环的旋钮留在这里，第二个从默认起步（hooks 和 checkpoints
关），而一个预置的最后模型按目录钉住；Phase 114 以同样方式驱动两个 look——在第一个
目录里选的 mascot 和 spinner 是它自己的也是最后，第二个目录的首次启动钉住那个最后、
而它自己的选择永不移动第一个的，而第三个目录取新的最后；Phase 118 驱动 session 轴
——同一个目录里两个实例跑两个模型，各自恢复自己的，而一次新启动取那个条目
（`docs/state/session-model.md`）。线上 `/model` 的写入在提交里描述的 tmux 运行中
对着一个真实 provider 演练。
