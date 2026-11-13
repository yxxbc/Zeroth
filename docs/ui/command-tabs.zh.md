# 命令页 —— 分页签，以及把各自的状态隔开

**状态：Commands + Skills 已实现。** `CommandMenu` 带一个 `active` 页签和每个
页签一个高亮（`src/app/commands.rs`）；页签栏与各行由
`ui::menu::command_menu_lines` 绘制（`src/ui/menu.rs`）。Agents 页签是下一阶段；
`/mcp` 保留它自己的 manager 页面。

## 已实现的样子：下划线页签

页签栏是一行朴素的标签 —— 没有框、没有背景药丸 —— 所以它和它下面的行属于同一条
扁平条带：

```
Commands  Skills
‾‾‾‾‾‾‾‾
→ /clear                   Clear the conversation
  /copy                    Copy the last response to the clipboard
  /export                  Copy or save the conversation as plain text
  (1/48)
```

活动标签是强调色、加粗并带下划线；非活动的那个是暗色。终端太窄放不下整条栏时
回退到只显示活动标签。**只有当存在第二个页签时这条栏才会被画出来** —— 一个没有
skill 的 session 保留它分页签之前那副一模一样的扁平 palette。

页面打开时，Tab 与 Shift+Tab 循环页签；Enter 运行活动页签里高亮的那一行。Tab
不再运行命令（Enter 一向如此），而普通 Tab 的后续/接受分支在页面一关就回来。

## 分页签之前

打一个裸 `/` 会在 composer 上方打开 palette。每个条目都来自同一个注册表 ——
`COMMANDS` 加上插在 `/model` 之后的活动模型速度档
（`App::commands`、`src/app/commands.rs`）—— 按 `/` 之后那个 token 的大小写
不敏感前缀过滤，由 `ui::menu::command_menu_lines` 画在输入框上方那条条带里。
`CommandMenu` 只持有一个数字 `selected`，指向**过滤后**的列表；Up/Down 移它，
Enter（或菜单打开时的 Tab）运行它，Esc 关闭，而一个尺寸守卫用一个 `(n/total)`
计数滚动窗口。

有三张不是斜杠命令的列表已经住在它旁边：

- **skills** —— `$name` 提及与 `Skill` tool
  （`src/skills.rs`、`SkillRegistry`），由 `$` picker 与 `/skills` 浏览；
- **agent 类型** —— `Agent` tool 的 `subagent_type` 名字（`src/subagents.rs`），
  由 `agent` tool 的描述与 `/skills` 的兄弟菜单展示；
- **MCP 服务器/工具** —— `/mcp`，带它自己的 manager 页面。

palette 对它们一无所知。一个想要“那个叫 `review` 的东西”的用户，得先知道它是
斜杠命令、skill、agent 还是 MCP 工具，才能打对那个符号。

## 诉求

一个带**页签**的命令页：`Commands`、`Skills`、`Agents`（以及之后的 `MCP`），
每个列出自己的来源，且状态**彼此隔离** —— 一个页签里的过滤器或高亮行永远不会
漏到另一个去，而 Enter 只可能运行活动页签里的条目。已有的符号保留它们的快捷
方式：`/` 打开在 Commands，`$` 打开在 Skills，`@` 仍然是文件 picker。

## 设计空间

### A. 一个 palette 里的页签（已实现）

```
Commands  Skills
‾‾‾‾‾‾‾‾
→ /clear    Clear the conversation
  /compact  Summarize the conversation
  /copy     Copy the last response
  (3/48)
```

- `CommandMenu` 带 `{ active: CommandTab, selections: [usize; 2] }`。composer 的
  文本仍然是那一个查询串；每个页签用这个查询过滤**自己的来源**，各自保留自己的
  高亮。切换页签会恢复该页签的选择，并夹取到它当前的匹配数。
- **palette 打开期间 Tab / Shift+Tab 循环页签。** 普通 Tab 的那几个分支（接受
  一个高亮的 `@` 文件 / `$` skill、排一个后续回合）在 palette 一关就回来；菜单
  自己的 Tab 分支只在 `command_menu.is_some()` 时胜出，所以没有任何既有的绑定
  改变。
- **Enter 运行活动页签里的条目。** 命令分派它们的 `CommandEffect`；一个 skill
  行把 `$name ` 插进 composer（由模型决定加载它，这是 `$` picker 自己的契约）。
  agent 行是阶段 2（只读；启动它是模型的 `agent` tool，不是命令）。
- **页签栏**显示时占一条条带行；`menu_rows` 仍然是已构建的行数，所以布局不需要
  任何特例。一个没有匹配的页签显示自己的占位行（`No matching skills`），而不是
  一条空条带。
- **隔离**是结构性的：每个页签拥有自己的选中下标；过滤后的列表按来源从共享的
  查询重新推导，绝不作为“另一个页签的第一个匹配”携带。

### B. 各自独立的页面（`/commands`、`/skills`、`/agents`）

三个全屏页面，没有页签。最容易做对（每个页面本来就有列表模型），但用户仍然
得知道分类、再运行另一条命令去看它；它没解决发现问题，只解决了隔离。它还会
重复已经存在的 `/skills` 与 `/mcp`。

### C. 只在分类非空时显示页签

A 把没有东西可展示的页签藏起来（没装 skill、没有 agent 类型）。在新装的机器上
和窄终端上更干净；页签栏在第一次装上某个第二分类有内容的软件时出现。状态模型
就是 A 的；只有栏的可见性不同。

**建议：在 A 之上做 C。** 一个页面，只为真正有条目的分类显示页签，逐页签隔离。
如果 palette 条带变得太高，B 留作后备。

## 实现地图

| 部件 | 文件 | 改动 |
| --- | --- | --- |
| 目录 | `src/app/commands.rs` | `CommandTab`、`PaletteItem`、`CommandMenu { active, selections }`；按页签的 `palette_matches`；`run_selected_command` 按 `active` 分派 |
| 来源 | `src/skills.rs`、`src/subagents.rs` | `App::skills`（`$` picker 的快照）现在也喂 Skills 页签；agent 类型等阶段 2 |
| 按键 | `src/app/keys.rs` | Tab/Shift+Tab 只在 `command_menu.is_some()` 时循环；Enter 接受；Up/Down 仍在活动页签内移动 |
| 渲染 | `src/ui/menu.rs` | 下划线栏（仅在有 2 个以上页签时）、活动页签的各行、逐页签占位、计数 |
| 布局 | `src/ui/menu.rs` | `menu_rows` 就是已构建的行数，所以页签行不需要单独的布局改动 |
| 测试 | `src/app/tests/keys.rs`、`src/ui/tests/menu.rs` | 逐页签的选择/过滤隔离；Tab 循环；Enter 碰不到非活动页签；只有一个页签时隐藏栏；窄栏折为活动标签 |

## 隔离规则（契约）

1. `selected` 是逐页签的，在每次过滤变化和切回时夹取到该页签当前的匹配数。
2. 查询是全局的（一个 textarea），但绝不产出跨页签的结果：一行只会从它自己的
   页签被绘制和运行。
3. 切换页签不运行、不分派、不改变任何条目；它只改变哪张列表被画出来。
4. 关闭 palette 不会丢弃历史里的任何东西；重新打开时从 Commands 开始
   （`CommandMenu::new`），也就是 `/` 打开的那个页签。
5. 一条在回合中途不可用的命令（`/clear`、`/resume`、`/compact` …）保留它既有的
   toast/拒绝路径 —— 页签对可用性毫无改变。

## 分期

1. **Commands + Skills 页签** —— 已实现：逐页签选择、Tab/Shift+Tab、skill 行
   插入 `$name `、只有 Commands 存在时隐藏栏，以及窄栏折为活动标签。
2. **Agents 页签**（只读），以及 `/skills` 快照的 agent 类型列表 —— 如果它值
   一个位置的话。
3. **MCP** —— 如果它值一个位置（它的 manager 页面已经存在）；否则留给 `/mcp`。
4. **逐页签的查询记忆** —— 只在用户要求时：那时切换页签会保存/恢复那个 token
   —— 这是我们在 v1 里刻意避开的一次 textarea 同步。

## 已定问题

- **在一个 skill 上按 Enter 会插入 `$name `**；它不调用 `Skill` tool。palette
  是纯的 `App`；直接加载是一个 `Action`，会绕过模型“只加载一次”的记账。插入是
  更小的契约，也与 `$` 提及一致。
- **Tab 循环；Enter 接受。** Tab 只在 palette 打开时是空闲的。
- **窄栏只显示活动标签** —— 不折行、不回退符号；这一行永不增长。

## 非目标

- 从 palette 运行 agent 或 MCP 工具（它们是模型的工具，不是用户的命令）。
- 第二个注册表：页签是既有来源之上的视图，不是一个新的目录。
- 改变 `$`/`@` 的行为；它们仍然是直接的符号，而 palette 页签是发现它们的路径。
