# `/settings` —— session 设置菜单

在此之前，这个应用的每个旋钮都是一个你得在启动前就知道的环境变量：
`ZEROTH_SHOW_THINKING`、`ZEROTH_TOOLS`、`ZEROTH_CHECKPOINTS`、
`ZEROTH_TEMPERATURE`、`ZEROTH_PROJECT_DOC_MAX_BYTES`，外加一个硬编码的
`retry::MAX_RETRIES`、一个硬编码的 `agent::MAX_TOOL_ITERATIONS`，和一个常开的
自动压缩。`/settings` 让这份清单**可见且可在 session 中途修改**，形状沿用
`/model` 和 `/login` 已确立的那套：一个替换 composer 的内联 picker、输入即搜，
以及每次改动一次按键。

```
────────────────────────────────────────────────────────────────────────────
  ❯

→ Hide thinking           false
  Verbose output          false
  Show images             true
  Image width             120
  Auto-resize images      true
  Error retry             3
  Tools                   true
  Permission mode         auto
  ...
  Max tool calls          0
  (1/20)

  Hide the model's chain-of-thought…

  Type to search · Enter/Space to change · Esc to cancel
────────────────────────────────────────────────────────────────────────────
```

## 设置项

二十行，每一行都是运行中的 session 真正会读的一个旋钮。每个值都是**循环**的
——任何地方都没有自由文本字段，所以 Enter 和 Space 在每一行上意思相同，菜单也
从不需要编辑模式。

| 设置 | 取值 | 它改变什么 |
|---|---|---|
| **Hide thinking** | `false` / `true` | 模型的思维链是否在 live 的 `● Thinking…` 单元格里流出并收成一行 `Thought for …`（`docs/ui/thinking-stream.md`）。`true` 恢复「计数后丢弃」的行为。由 `ZEROTH_SHOW_THINKING` 播种。 |
| **Verbose output** | **`false`** / `true` | 每次 read、搜索、列举和 scratchpad 编辑是不是各留一个自己的单元格（`true`，tool 单元格与它们一直以来的样子完全一致），还是一串这样的合并成一行折叠行——运行中是 `● Reading 2 files, listing 1 directory…`，结束后是 dim 的 `Read 2 files, listed 1 directory (ctrl+o to expand)`，session 的 scratchpad 里一次 `write`/`edit` 则有自己的领头从句 `made 1 scratchpad edit +55`（`docs/ui/tool-groups.md`）。**默认关**，即 Claude Code 的非 verbose 视图；Ctrl+O 两边都把每个调用完整显示。循环它会 purge 重建，所以已提交的对话立刻改变形状。由 `ZEROTH_VERBOSE` 播种。 |
| **Show images** | `true` / `false` | 一张粘贴的截图和 `read` 工具的图片读取，是否在对话里画成图（`docs/ui/images.md`）。当终端画不出来时是 `false (unavailable)`。循环它会 purge 重建，所以已提交的图片立刻出现或消失。 |
| **Image width** | `60` / `80` / `120` | 一张内联图片的宽度**上限**，按列——夹取到终端宽度，从不用于把一张小图放大。循环它像 **Show images** 一样 purge 重建。 |
| **Auto-resize images** | `true` / `false` | 一张大图是否在**发给模型**之前缩到 2000×2000（`docs/ui/images.md`）。与显示无关，所以这一行在一个画不出图的终端上也仍可用。 |
| **Error retry** | `0` / `1` / `2` / `3` / `5` / `10` | 一个 provider **答得很糟**（`5xx`、一次 reset、一次停滞）的请求在错误浮现前重试多少次（`llm::retry`，`retrying n/N` 状态）。根本连不上的连接不计入：它是被**等待**的，每几秒一次，要多久就多久，在 `Waiting for internet…` 状态下（`docs/providers/offline.md`）。`0` 表示从不重试，那一份等待也包括在内。曾是固定的 `MAX_RETRIES = 3`。 |
| **Tools** | `true` / `false` | `bash`/`read`/`write`/`edit`/`agent` 是否提供给模型（`docs/tools/tools.md`）。由 `ZEROTH_TOOLS` 播种。关掉还会撤回 `<system-reminder>` 的两半——skills 清单和 agent 类型清单（`docs/agents/subagents.md`）：为一个请求从不携带的工具准备名单，是条死路。 |
| **Permission mode** | `manual` / `edit` / **`auto`** / `master` | Shift+Tab 循环的那同一个姿态（`docs/state/permissions.md`）——这一行是通往同一个状态的第二扇门，不是它的副本。**`auto`，直到某个项目保存了别的模式**，而它按项目存在 `permissions.json` 而不是 `settings.json` 里；一次运行时由 `ZEROTH_PERMISSION_MODE` 播种，从不保存。 |
| **Checkpoints** | **`false`** / `true` | 按回合的工作目录快照（`docs/state/checkpoint.md`）。**关，直到某个目录把它打开**（`docs/state/per-directory-state.md`）；由 `ZEROTH_CHECKPOINTS` 播种；当存储完全跑不了时（没有 git、没有 config home，或一个该特性拒绝的 cwd——见*不可用的设置*）被强制成 `false`，且不可更改。 |
| **Auto compact** | `true` / `false` | 循环是否在上下文窗口超过 90 % 之后自行跑摘要回合（`docs/architecture/compact.md`）。手动的 `/compact` 不受影响。 |
| **Project docs** | `true` / `false` | 项目的 `AGENTS.md` 文件是否每回合被重读进上下文的领头 user 条目（`docs/state/project-doc.md`）。由 `ZEROTH_PROJECT_DOC_MAX_BYTES=0` 播种。 |
| **Hooks** | **`true`** / `false` | 用户信任过的 lifecycle hook 是否运行——用户的 `~/.zeroth/hooks.json` 和一个项目的——围绕 tool call、回合和 session 边界（`docs/tools/hooks.md`）。**开，直到某个目录把它关掉**（`docs/state/per-directory-state.md`）；每个 hook 在启动复核里被信任之后才运行（`docs/state/config-review.md`）；由 `ZEROTH_HOOKS` 播种；**不可用**于没有解析出 hooks 文件、或其中没有可运行的东西时。 |
| **Skills** | `true` / `false` | `skill` 工具是否被提供，以及 `<system-reminder>` 的清单是否搭在上下文上（`docs/tools/skills.md`）。由 `ZEROTH_SKILLS` 播种；**不可用**于没有任何 `SKILL.md` 加载时——没有东西可打开。 |
| **MCP** | **`true`** / `false` | MCP 服务器是否连接，以及模型是否被提供它们的工具和 instructions（`docs/tools/mcp.md` 的 *MCP off for the session*）。**开，直到某个目录把它关掉**，Hooks 的姿态。关掉会停下每个服务器，而各自保留它在 `/mcp` 里的启用或禁用状态，于是把这一行重新打开，带回来的正是那些原本启用的服务器；`/mcp` 仍能打开并说明 MCP 已关。由 `ZEROTH_MCP` 播种；**不可用**于用户 `mcp.json` 或某个项目的里面一个服务器都没声明时。 |
| **Temperature** | `default` / `0.0` / `0.3` / `0.5` / `0.7` / `1.0` | 每个请求携带的采样温度；`default` 什么都不发，留给 provider。由 `ZEROTH_TEMPERATURE` 播种。 |
| **Max tool calls** | **`0`** / `5` / `10` / `20` / `50` / `100` | 一个回合在放弃之前可以跑多少个 tool **call**（`llm::agent::run_agent` 的上限）。**`0` 是无限制，也是默认**——见下文。 |
| **Ask timeout** | `5m` / **`10m`** / `20m` / `30m` / `1h` / `never` | 一个 `AskUserQuestion` 问题在一个**空闲**用户身上等多久才无答复地解决、让 agent 按自己的判断继续（`docs/tools/ask.md`）——任何按键或粘贴都重新开始计时，而模态的收尾规则把它倒数出来。`never` 停掉这个钟。由 `ZEROTH_ASK_TIMEOUT_SECS` 播种（秒，`0` = never），它的奇数取值仍整段读出（`1m 30s`）。在用到它的地方读：绘制 tick 每一次读空闲时钟时都读它，于是一个新值作用于一个已经在跑的等待，而 `never` 立刻停掉它。 |
| **Show tips** | `true` / `false` | 一个回合跑过三秒后，状态行下是否挂一行 dim 的 `⎿  Tip: …`——一个值得知道的按键或命令，每回合取目录里的下一个（`docs/ui/tips.md`；Claude Code 的 `spinnerTipsEnabled`）。**按用户，不按目录**：存在 `tips.json` 里、与行走的位置并排，从不进 `settings.json`。由 `ZEROTH_TIPS` 播种，它——不是隐私开关——从不撤回那一行。不需要宿主提供什么，所以它永不不可用。 |
| **Update check** | `true` / `false` | 应用是否每天一次问仓库有没有更新的 release，并在横幅下面说（`docs/ops/update.md`：对 `/releases/latest` 的一个 `HEAD`，由一个 tag 就是版本的 redirect 回答——没有 body、没有 install id、没有关于你的任何东西；卡片点名那个 release、`zeroth update` 和关闭开关，每天一次，绝不在一段答复中途）。**按用户，不按目录**：存在 `update.json`，从不进 `settings.json`。由 `ZEROTH_UPDATE_CHECK` 播种；**不可用**于没有 config home（没地方记住那一天）**以及只要该变量禁止它**——Telemetry 的规则，一个说不的环境就撤回那一行。命令行的 `zeroth update` 是用户自己的请求，无论如何都会跑。 |
| **Telemetry** | `true` / `false` | 匿名的每日用量 ping 是否发送（`TELEMETRY.md`：五个字段——一个载荷版本、一个随机 install id、应用版本、操作系统和架构——绝无 prompt、路径、模型或 key；收集端记下国家，绝不记地址）。**按用户，不按目录**：存在 `telemetry.json`，从不进 `settings.json`。由 `ZEROTH_TELEMETRY` 播种，被 `DO_NOT_TRACK=1` 压过；**不可用**于没有 config home（没地方留 install id）**以及只要任一变量禁止它**——各行之中唯独它，一个说不的环境是撤回这一行，而不只是播种，因为一个一次按键就能撤销的退出不算退出（`TELEMETRY.md`）。 |

### 上限计的是 call，不是轮

它必须如此。一轮可以请求一整批**并行批**（`docs/tools/parallel-tools.md`）——
让一个模型跑六条命令，它在一轮里回六个 `tool_calls`——于是按轮计数会让单独一轮
超支上限好几倍：`Max tool calls 5` 实际跑了 15，即被报上来的 bug。所以
`run_agent` 是**按 call** 花预算的。

一轮只能部分负担的预算会被**夹取**而不是整轮拒绝：能装下的那么多个 call 按模型
自己的顺序跑，其余的解决为红色单元格，读作
`Not run: this turn had already spent its
tool-call limit.`
整轮拒绝会更简单，但一个一开口就发出比整个上限还宽的批次的模型，那样就什么也
不做、只是报错。每个被拒的 call 仍被*回答*以同一段文本，于是存下的消息列表对
每个 `tool_call` 都保留一条 `tool` 结果——这正是严格的 provider 对下一个请求、
一次 `/resume` 或一次 agent 续跑所要求的。

随后回合以一条承载完整故事的红通知收尾，因为用户遇到它时那一行上没有别的东西
可以解释——上面没有单元格，下面没有提示：

```
● Stopped after 5 tool calls — this turn hit the "Max tool calls" limit before
  the model finished. Run /settings to raise it, or set it to 0 for no limit.
```

什么停下来了、**为什么**（一个用户设的上限——不是模型或 provider 的失败，而一句
光秃秃的 `stopped after 5 tool calls` 读起来就像后者），以及**怎么**改，点名那
一行的 label，好让它被找到。

**那条错误也是模型得知的方式。** 一条 `Role::Error` 通知推导成上下文里一条
`[error] …` **user** 条目（`crate::context`），所以下一回合的窗口已经读到：

```
user:
  [error] Stopped after 5 tool calls — this turn hit the "Max tool calls"
  limit before the model finished. Run /settings to raise it, or set it to 0
  for no limit.
```

这正是被拒调用自己的结果只留一句话的原因。把建议也放在那里会说同一件事两遍
——而一个被夹取的批次会一次拒绝好几个调用，那样就为一个事实花掉好几段屏幕
*和*好几段上下文窗口：

```
tool:
  Not run: this turn had already spent its tool-call limit.
```

建议在一处，事实在一处。下一回合被问「那为什么停了？」时，模型读到两者，用原因
和那个设置回答。

### 为什么 **Max tool calls** 默认无限制

库把 `agent::MAX_TOOL_ITERATIONS = 20` 留作对一个永远打转的模型的后备，对一个
没人在旁边看的嵌入方来说，那是正确的默认。对这个应用是错的默认：一个在任务
中途触发的上限会丢下一半的工作——文件写到一半、迁移做了一半——而用户就坐在那里
按 **Esc**，它会立刻停下回合并保住已经流出的所有东西。所以应用不带上限地发布
（`0`），而这些上限留给想要一道硬墙的人。`0` 用 dim 的「关」颜色渲染，像
`false` 一样：限流器是关着的。

### 不可用的设置

一个旋钮可以**不可用**，而不只是关掉：checkpoints 需要一个 `git` 二进制、一个
config home、一个该特性愿意快照的 cwd（`checkpoint::cwd_scope`——不是 `/tmp`、
`~`、`~/.zeroth`，也不是系统目录），以及一棵小到能装进每回合成本预算的工作树
（预检探测，当不满足时用 `CheckpointStore::disable` 让存储退役）。这些在 session
中途都不会变。这样的一行把它的值渲染成 dim 的 `false (unavailable)`，循环它会
弹一个解释性 toast，而它从不写进设置文件——于是一个在 `~` 里的 session 不会把
「checkpoints 关」持久化进之后每一个项目。
[`SessionSettings::availability`] 携带边界的裁决，像时钟一样在 bootstrap 时注入。

## 形状

`SettingsPicker` 是 `ModelPicker` 的兄弟：它作为 `Option<SettingsPicker>` 住在
`App` 里，**内联**渲染（替换 composer，绝不是 alternate screen），打开时拥有
每一个键。它的行按需从活状态推导——`App::setting_rows()`——而不是存下来，正如
`/model` picker 推导它的匹配，所以值那一列永远不会漂离 session 实际在做的事。

布局是 `/model` picker 的加上一行提示——picker 家族的那一页
（`docs/ui/view-flow.md` 的 *A picker's page*）：

```
──────────────────────────────────────────────────────────────────────────────────
  Settings
  Changes take effect at once and persist across sessions

  ❯

  → Hide thinking        false
    Verbose output       false
    Show images          true
    Image width          120
    Auto-resize images   true
    (1/20)

    Hide the model's chain-of-thought instead of streaming it above the composer

  Type to search · Enter/Space to change · Esc to cancel
──────────────────────────────────────────────────────────────────────────────────
```

标题压在一行 dim 的「改动会做什么」之上（`SETTINGS_TITLE`、
`SETTINGS_SUBTITLE`——每一行都立刻生效并被保存，多数按目录、三个按用户，所以这
行说的是对它们全体成立的东西），搜索行，五行带 `→` 的、`→` 坐在 `❯` 之下，以及
计数器和被高亮行的描述挂在 label 之下（`MODEL_DETAIL_INDENT`）。

一个什么都匹配不到的搜索没有计数、也没有可描述的，于是那些槽位收成一个空行
——`rule · header · gap · ❯ search · gap ·
No matching settings · gap · hint · rule`——而不是在帧中间画出四行空白（`/model`
picker 的 `model_has_detail` 规则，现在由 `/mascot`、`/skills` 和 `/login` 共用）。
两条框线都不贴空行（`docs/ui/view-flow.md` 的 *The frame hugs its content*）。

——十二行固定装饰围着一个封顶在 `SETTINGS_MENU_MAX_ROWS`（五；曾是十——计数器
说明还有多少个，而搜索能到达其中任何一个）的列表，由共享的 `centered_window`
开窗，所以选中项保持居中；整页是一个行构造器
（`settings_view_lines`——`/mcp` 家族的形状，它的高度就是构造出的行数，底部锚定
绘制，被跳过的顶部流进 scrollback，`docs/ui/view-flow.md`）。与 `/model` 不同，
装饰从不折叠：总至少有一个设置，而一个什么都匹配不到的搜索仍在同一个帧里显示
它的占位符。

### 一个装不下这一页的终端

整页是十七行或更多，而在一个更矮的终端上底部锚定它，会保住描述和提示，却让搜索
行和被高亮的行流进 scrollback——在十二行上，Enter 会改的那一行在屏外。所以
`settings_view_lines` 拿到页面可用的行数，并构造四个页面中第一个装得下的
（`first_page_that_fits`，家族规则见 `docs/ui/view-flow.md` 的 *A page that
sheds rows before it flows*）：

1. 整页；
2. 整页但描述截成一行（`Description::OneRow`）——在 80 列的终端上，
   好几段描述会换到两行，于是整页的高度随选中项移动，而一个处在边缘的终端单凭
   那个高度做决定时，会在 ↑/↓ 经过某一行时在整页和紧凑页之间来回翻；
3. 标题之下的紧凑页，十二行（`compact_page_with_header`）；
4. 同样的紧凑页去掉标题，即 `/theme` 和 `/skills` 的那一页，一行对一行：

```
──────────────────────────────────────────────────────────
  ❯
  ↑ 5 more above
    Tools                true
  → Permission mode      auto
    Checkpoints          false
  ↓ 12 more below
  Type to search · Enter/Space to change · Esc to cancel
──────────────────────────────────────────────────────────
```

描述和计数器让位，一个 `COMPACT_LIST_ROWS`（3）的窗口坐在保留的溢出槽位之间
（`model_compact_list`），没有东西被那样藏起来时各自为空，所以页面在每个选中项
上都是一个高度——两条框线都不贴空行（`docs/ui/view-flow.md` 的 *The frame hugs
its content*）。值那一列按所有列出的设置定宽，而不是按视野里的那些行，所以窗口
滚动时它保持不动——它以前在整页上也会移动，只要 ↑/↓ 把
`Auto-resize images` 滚进或滚出所显示的行——而在一个窄到放不下最宽 label 的
终端上，label 用 `…` 截断，好让值保住自己的座位。

在更矮的终端上，紧凑页一次一行甩掉剩下的东西，永不甩掉它的搜索行
（`docs/ui/view-flow.md` 的 *Below the compact page*）：`↑`/`↓` 那一对、顶框线、
一路到被高亮设置的行、底框线和提示。改为底部锚定时，紧凑页把搜索行流进了
scrollback，然后是被高亮的设置。这些梯级把搜索行往上挪，所以光标的座位在构造出
的页里找到它（`view_prompt_row`），而不是把它钉在第 1 行；一个什么都匹配不到的
搜索以同样方式甩掉，同时保留已输入的内容。

### 按键

| 键 | 效果 |
|---|---|
| ↑/↓、PageUp/PageDown、Home/End | 移动选中项（↑/↓ 在两端回绕——↓ 过最后一行落到第一行，从第一行按 ↑ 落到最后一行；跳转是夹取的） |
| **Enter** / **Space** | 循环被高亮设置的值 |
| 可打印字符 | 输入进搜索（Space *不*是其中之一——它是循环键） |
| Backspace | 弹出搜索 |
| Esc | 清空非空的搜索，否则关闭 |
| Ctrl+C | 关闭 |

Space 用于循环，正是搜索不能含空格的原因；这没关系——label 是按大小写不敏感的
子串对 label *和*描述匹配的，所以 `perm` 找到 `Permission mode`，`agents` 找到
`Project docs`。

## 一次改动之后发生什么

`App::cycle_selected_setting()` 推进那个纯值，并为循环返回一个
[`Action`]，那才是每个旋钮真正被*应用*的地方：

- **Permission mode** 返回既有的 `Action::SetPermissionMode`，于是它走那条把模式
  镜像到闸门上、清扫新模式现在覆盖的那些请求、并持久化本项目条目的唯一路径。
- 其余都返回 `Action::SettingChanged(key)`，而
  `tui::settings::Session::apply_setting` 干活：
  - **Tools**、**Error retry**、**Temperature** 和 **Max tool calls** 重建后端
    （`ModelSession::set_tools` / `set_max_retries` /
    `set_temperature` / `set_max_tool_calls`——即 `/model` 切换的那次重建，所以
    整份共享的附件集被重新挂上，而一个 subagent 继承同样的预算）。
  - **Checkpoints** 翻动 `CheckpointStore::set_enabled`，它只能把一个*有能力*的
    存储打开或关掉。
  - **Hooks** 也重建：sink 是每次后端构建时挂上的，所以翻动这一行真正地让下一
    回合停止（或开始）查询用户的命令，而不是留一个第二 flag 去漂移
    （`docs/tools/hooks.md`）。
  - **Project docs** 立刻重载（或丢掉）`App::user_instructions`，于是 Ctrl+D 在
    下一回合之前就显示了这个改动。
  - **Skills** 两半都做，因为该特性有两半：工具搭在后端构建上（所以这一行重建
    它），而清单搭在上下文上（所以这一行重新渲染 `App::skill_listing`）。关掉它
    把发现到的集合留在内存里，于是再打开不花一次重扫（`docs/tools/skills.md`）。
  - **MCP** 切换 session 的 MCP manager（`McpManager::set_active`、
    `Session::apply_mcp_setting`）。关掉丢弃每条连接——每个 stdio 服务器都被
    停掉——并立刻撤回服务器的工具和 instructions：下一回合的后端不带它们重建，
    reminder 重新渲染。打开则连接 `/mcp` 里启用的每个服务器，后端随着每个 connect
    落地而获得它的工具，在等着它的项目服务器之上打开复核的服务器页，并点名一个
    启动时保持安静的配置问题（`docs/tools/mcp.md`）。
  - **Verbose output** purge 重建对话，**Show images** 的规则：每一次已提交的
    read 和搜索改变形状——每次调用一个单元格，或一行折叠行——而重建从 history
    重绘它们。打开它会先放掉一个打开的组，于是它持有的那些 read 作为它们自己的
    单元格回来（`docs/ui/tool-groups.md`）。
  - **Hide thinking** 和 **Auto compact** 除那个纯 flag 之外什么都不需要：
    `tui::stream` 按阶段查 `App::settings().show_thinking()`，而
    `App::should_auto_compact` 以该 flag 为闸。
  - **Show tips** 写它自己的文件——`tips.json`，与行走的位置并排
    （`docs/ui/tips.md`）——别的什么都不需要：`App::tip` 每帧读这一行，所以状态行
    下的那一行在下一次绘制时就走（或来），回合中途也一样。
  - **Update check** 也写它自己的文件——`update.json`——而打开它立刻走今天那一
    步：文件已经知道的一个更新的 release 会被宣告，而当一次都还没跑过时，今天的
    请求被发出（`Session::apply_update_setting`，`docs/ops/update.md`）；关掉它
    就不再发任何东西。
  - **Telemetry** 写它自己的文件——`telemetry.json`，与 install id 并排——而不是
    这个目录 `settings.json` 里的条目，而打开它在今天还没发过时立刻发今天的 ping
    （`Session::apply_telemetry_setting`，`TELEMETRY.md`）；关掉它就不再发任何
    东西。

每次改动都以一个瞬时 toast 确认（`{label}: {value}`）并持久化。

像 `/model` 和 `/login` 一样，`/settings` **回合中途可用**——它只替换 composer，
而一次重建重绑*下一个*回合的后端，正在跑的那个在自己的线程上照常流出，未被动过。
「只替换 composer」是字面意思（2026-08-08 更新）：流式条带——运行中工具的 live
单元格、状态行、排队的消息、toast——保留它在菜单框之上的行，所以在回合中途打开
`/settings` 永远不会遮住它被在其旁边打开的那个回合。`settings_height` 在菜单自己
的 `settings_rows` 之上保留 `ui::layout` 的 `strip_above_rows`，而 `render_live`
经共享的 `view_split` 在它上面画条带。见 `docs/providers/llm.md`（那个被报上来
的 bug，先在 `docs/tools/background.md` 里为 ↓ manager 条带修好）。

## 持久化

`~/.zeroth/settings.json`，与 `config.json`（`/model` 的选择）和
`permissions.json`（按项目的规则）并排的自己的文件——每个拥有它的特性一个文件，
于是一次写入永远不会冲掉邻居的状态——而且像那两个一样，内部**按工作目录**为键
（`docs/state/per-directory-state.md`）：你在一个项目里设的旋钮就是那个项目的。
格式是纯的（[`settings::SettingsFile`]：`projects` 下每个目录一个
[`settings::SessionSettings`] 条目，每个字段可选且等于默认时跳过，压在文件的顶层
键之上——即一个没有条目的目录起步所依据的**种子**，它也是在一个设置按目录之前
写下的文件的全部内容，于是一个已保存的文件继续处处生效，直到某个目录改了点什么）。
读写是边界的（`tui::config::load_settings_file` / `save_setting`），像其他的一样
尽力而为：一个只读的 home 绝不能杀死 TUI。每个条目读起来是**相对默认值的一个
diff**——里面只有你真正改过的东西：

```json
{
  "projects": {
    "/home/user/work/api": {
      "hide_thinking": true,
      "error_retry": 5,
      "temperature": 0.7
    }
  }
}
```

一个条目是一个整体块，绝不是种子之上的一层（diff 格式分不清一个停在默认上的
`true` 和一个刻意选的 `true`），而且即使它等于默认也保留着——丢掉它会让种子
重新进来。

**Telemetry**、**Update check** 和 **Show tips** 是三个完全不在此文件里的行。
一个只作用于你碰巧所在目录的退出会是个意外，所以它们的值住在 `telemetry.json`、
`update.json` 和 `tips.json`（`TELEMETRY.md`、`docs/ops/update.md`、
`docs/ui/tips.md`）——每个字段在 `SessionSettings` 上是 `#[serde(skip)]`，而
`copy_value` 从不搬动它（`PermissionMode` 的模式）；`apply_setting` 把每个写进
自己的文件，它们一个都不会移进这个目录的条目。

启动时的优先级是应用其余部分遵循的同一条规则——**环境胜出**：一个显式设置的
`ZEROTH_*` 变量为那一个设置覆盖已保存的值
（`tui::config::apply_setting_overrides`——包括 `ZEROTH_HOOKS`，它播种
**Hooks** 行，正如 `ZEROTH_TOOLS` 播种 **Tools**，以及 `ZEROTH_MCP`，
它以同样方式播种 **MCP**）。既没设置也没保存的任何东西取它的默认。

而且，像 `ZEROTH_MODEL` 一样，环境**永不粘住**。一次保存是对文件本身的一次
读-改-写：重读该目录的条目（否则种子），**只有用户循环过的那个键**从活 blob 里
搬过去（`SessionSettings::copy_value` / `SettingsFile::record_value`），然后条目
被写回。改为写入合并后的活 blob，会悄悄持久化为一次运行所设的覆盖：在一个恰好
导出 `ZEROTH_TOOLS=0` 的 shell 里把 `Error retry` 调高，那个目录之后每个
session 都会以工具关闭启动。

### Hooks 和 checkpoints 默认关

**Checkpoints** 在第一帧之前跑一次整 cwd 的 `git add -A`，所以它们是**关**的，
直到某个目录把它们打开：为一个你只是打开来读一个文件的树做 checkpoint 存储，
启动时花掉几秒，还要在 `~/.zeroth` 下留一份树的拷贝。opt-in 就是想要它的那个
目录里循环一次，那里记作 `"checkpoints": true`。**Hooks** 是**开**的，每个
hook——用户的或项目的——在启动复核里被信任之后才运行
（`docs/state/config-review.md`）；一个目录循环一次退出，记作
`"hooks": false`。`ZEROTH_HOOKS` / `ZEROTH_CHECKPOINTS` 为一次运行播种
两者而不保存任何东西——`smoke.sh` 的 hooks 和 checkpoint phase 就是这样拿到它们
的。**MCP** 跟随 Hooks：开，每个服务器仍只在被启用之后连接（一个项目的要经启动
复核），而一个一个都不要的目录循环一次退出，记作 `"mcp": false`。

## 测试

纯模型（`src/settings.rs`）、picker 状态（`src/app/settings.rs`）和渲染器
（`src/ui/settings_view.rs`）都有单元测试。边界——后端重建、checkpoint 翻动、
文件写入——由 `scripts/smoke.sh` Phase 67 覆盖，它打开菜单、搜索、循环一个值，
并检查 toast 和被收起的 composer；Phase 109 驱动按目录的那一半（一个目录里循环
一个旋钮，下一个目录里是默认，两者对着同一个 config home）；Phase 139 在一个
12 行终端上打开它并走过它的紧凑页；Phase 146 在一个折叠的 read/搜索组上循环
**Verbose output**，并检查重建把它展开（`docs/ui/tool-groups.md`）；Phase 152
在一个脚本化服务器上把 **MCP** 关掉再打开，检查它停下、跨重启动保持关着、再次
连上，以及 `ZEROTH_MCP=0` 从不被保存（`docs/tools/mcp.md`）。
