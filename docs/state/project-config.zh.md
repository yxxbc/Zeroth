# 项目级 `.zeroth` 配置 —— hooks、MCP 服务器、skills、agents

一个项目现在可以在它根目录下的 `.zeroth` 目录里携带自己的 Zeroth 配置 ——
`/home/user/project/.zeroth/` —— 于是不同的项目加载不同的 hooks、MCP 服务器和
skills。目录名是 `.zeroth`（crate 的名字、config home 的名字，也是 skills 遍历
本来就在用的名字），不是另一种拼法。项目的 `.zeroth/` —— 上游应用的那个目录 ——
**不被**读取，于是两个应用绝不共享项目配置。

**项目根**是 cwd 最近的、带 `.git` 条目的祖先或自身
（`project_doc::find_project_root` —— 即 `AGENTS.md`/skills/MCP 的上溯，所以在
`repo/src` 里启动仍然找得到 `repo/.zeroth`），在一个不是 repo 的目录里回退到 cwd
自身 —— **home 目录除外，它永远不是项目**（`trust::is_project_root`，
`checkpoint::cwd_scope` 的那种直觉）：
在 `~` 里启动时，这个回退根会让 `{root}/.zeroth` 变成用户自己的 config home，而
这一层就会把用户的 `hooks.json`/`mcp.json` 当作“项目配置”重新发现，把用户自己的
文件读两遍。在 `~` 里这一层干脆是关的，而那些文件仍然作为它们本来的用户层加载。
`tui::trust` 里一个按文件的身份守卫在其他任何地方抓住同样的碰撞（一个指向项目
内部的 `ZEROTH_CONFIG_DIR`）：一个*就是*用户层文件的路径，绝不再按项目配置读
一遍。

项目层读的东西：

| 文件                            | 内容                                        | 合并规则                                     |
| ------------------------------- | ------------------------------------------- | -------------------------------------------- |
| `{root}/.zeroth/skills/`    | 项目 skills（已发布，不变）                 | 首根胜出的遍历（`docs/tools/skills.md`）     |
| `{root}/.zeroth/agents/`    | 项目 subagent 类型（在闸门之外，与 skills 一样） | 首根胜出的遍历（`docs/agents/subagents.md`） |
| `{root}/.zeroth/hooks.json` | 项目生命周期 hooks —— **新增**              | 在用户文件之后**取并集**（见下）             |
| `{root}/.zeroth/mcp.json`   | 项目 MCP 服务器 —— **新增**                 | 首名胜出，压过 `.mcp.json`                   |
| `{root}/.mcp.json`              | Claude-Code 兼容的 MCP 服务器（既有）       | 首名胜出，排在 `.zeroth/mcp.json` 之后       |

不在这里面的：`providers.toml`。一个 checkout 里的 provider 块可以把一把存下的
key 指向 checkout 自己挑选的主机，而且与 hook 不同，它会在任何东西被审查之前就
这么做 —— 启动时的能力探测会未经询问列出活跃 provider 的模型 —— 所以 provider
文件只从用户的 config home 读（`docs/providers/user-providers.md`）。这个应用
过去会从工作目录读一个；现在不会了。

## 里面的东西在用户看过之前都不运行

一份签入的 `hooks.json` 会在生命周期事件上运行 shell 命令，而一条签入的 MCP 条目
会在 session 开始时启动一个进程或打开一个连接：**克隆一个仓库不能因此获得代码
执行权。** 所以每个项目 hook 和每个项目 MCP 服务器都在等**启动审查**
（`docs/state/config-review.md`）：只要有一个是新的或改过的，它就在第一帧打开，
逐字列出每一个，并把答案按 hook、按服务器记进 `{config_home}/trust.json`。一个
在那里没有被启用的项目服务器仍然会出现在 `/mcp` —— 审查开着时是
`⚠ awaiting review`，被拒之后是 `◯ disabled` —— 因为一个你看不见的服务器就是一个
你无法判断的服务器；那里的 Enable 是回去的路。

这次审查取代了一个 `/trust` 菜单，那个菜单按内容哈希把项目的配置文件作为整体
批准：每个 hook 和每个服务器一次答案、只有用户打开那个菜单时才问，而且对用户
自己的 `hooks.json` 视而不见 —— 而审查现在把它也覆盖了。

- **Skills 仍在审查之外**：一份 `SKILL.md` 是惰性的 markdown，直到模型显式加载
  它，而它正文里的任何命令仍然和其他每个 tool call 一样要过权限闸门。（它那一行
  描述确实搭上上下文列表 —— 这是自 `docs/tools/skills.md` 以来的既有行为。）
- **Subagent 定义同样在审查之外**，理由相同（`docs/agents/subagents.md`）：一个
  `agents/*.md` 是加载时不运行任何东西的 markdown 和元数据，它的 `tools:` 只能
  *收窄*一个被启动的 agent 能触及的范围，它的 `model:` 点名的是该 session 自己
  provider 上的一个模型，而那个 agent 做的每一次 `write`/`edit`/`bash` 仍然要过
  权限闸门。一个项目文件能做的，是在一个 subagent 的 system prompt 里塞话 —— 而
  一份签入的 `AGENTS.md` 或 `SKILL.md` 本来就能这么做，所以给这一个上锁等于给
  一扇开着的门装锁。

## 合并语义

**Hooks：各层取并集。** 合并后的文件是用户文件，加上项目文件按事件追加的 matcher
组（`HooksFile::merged`）—— 纯增量，`docs/tools/hooks.md` 的预先承诺：从加载器多
来一条，不用发明任何优先级规则 —— 而且每一层只贡献在它里面被信任的 hooks
（`trust::trusted_only`）。既有的 `select()` 规则已经回答了每一次碰撞：重复项按
命令去重（先到者胜 —— 用户的副本）、任何 block 都胜、`deny` > `ask` > `allow`、
上下文相连接。一个格式错误的项目文件是一个点名它的红色 toast，而项目的 hooks
保持关闭；用户自己的 hooks 仍然加载（每一层各自失败）。

**MCP：首名胜出**，在 `[{root}/.zeroth/mcp.json, {root}/.mcp.json,
{config_home}/mcp.json]` 之间 —— 即既有的“项目遮蔽用户”优先级，只是应用专属文件
排在兼容文件之前（skills 的惯例：`.zeroth` 压过 `.claude`）—— 针对的是审查
**启用**了的那些服务器；一个还在等答案、或被拒的项目服务器列在用户的之后，于是
绝不可能把一个同名的用户服务器挤出 session（`mcp::merge_project_scopes`）。两个
项目文件都是 `McpScope::Project`，所以 `/mcp` 的分组不变；每个条目的
`Config location:` 行点名它实际所在的文件。

## 生命周期与旋钮

项目 hooks/MCP 文件保持它们各子系统**在 bootstrap 只读一次**的寿命；在审查、
`/hooks` 或 `/mcp` 里做的决定作用于当时读进来的那一份。对话中途的修改需要重启，
与用户级的 `hooks.json`/`mcp.json` 完全一样，并在下次启动时被审查。Skills 保持
它们每回合的重扫。

- `ZEROTH_PROJECT_CONFIG` —— 默认开；`0`/`false`/`no`/`off` 关掉整个项目层：
  不发现任何项目 hooks/MCP，于是审查里也没有任何项目的东西（`ZEROTH_MCP` 的
  姿态）。
  **这是 smoke 套件的封闭性开关** —— `scripts/smoke.sh` 在基础环境里把它设为
  `0`（套件跑在 cwd = 一个真实 checkout 之下；一个带着 `.zeroth/hooks.json` 的
  开发者 repo 否则会在每个 phase 里被审查 —— skills Phase 36 的教训），而审查
  那个 phase 在一个 `mktemp -d` 工作目录里把它重新打开。
- `ZEROTH_HOOKS_FILE` 仍然只替换**用户**文件，项目发现照旧运行 ——
  `ZEROTH_MCP_FILE` 的惯例，如今两个子系统共用；项目层自己的总开关是
  `ZEROTH_PROJECT_CONFIG=0`。
- `trust.json` 像 `permissions.json` 一样挂在 config home 下 —— 它自己没有覆盖
  变量；`ZEROTH_CONFIG_DIR` 会移动它。

## 模块划分

纯的（单元测试，TDD）：

- `src/trust.rs` —— 项目文件路径构建器（`project_hooks_file`、
  `project_mcp_files`）、根规则（`is_project_root`、`hooks_project_dir`），以及
  审查的存储（`docs/state/config-review.md`）。
- `hooks::HooksFile::merged` —— 并集。
- `mcp::merge_project_scopes` —— 认可感知的优先级。

边界（smoke 验证）：

- `tui/config.rs` —— `project_config_enabled()`、`trust_json_path()`、
  `load_trust_file`（失败即关闭 + 出声）。
- `tui/trust.rs` —— 在 bootstrap 读项目层（`load_project_layer`），以及审查围绕
  它做的一切。
- `tui/mcp.rs` —— `load_mcp_sources` 读第二个项目文件，并把审查压住的那批服务器
  喂给管理器。

## 有意不做的事

- **没有项目级的 `permissions.json`/`settings.json`。** 那些是偏好文件，不是
  能力；今天它们里面没有什么值得那份信任面。
- **没有对 hooks+MCP 的文件监视 / 每回合重扫。** 它们保持既定的只读一次寿命；
  skills 保持它们的重扫。一个在对话中途出现的项目层，离一次重启之遥，与其他每次
  hooks/MCP 编辑一样。
