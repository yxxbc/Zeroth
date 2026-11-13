# `/hooks` 菜单 —— hook 浏览器

`/hooks` 在每一个配置过的生命周期 hook 之上打开一个内联浏览器 —— 用户的
`hooks.json` 和某个项目的（`docs/tools/hooks.md`）—— Claude Code 的 `/hooks` 菜
单，整套移植自它的 `HooksConfigMenu`（那套 `SelectEventMode` →
`SelectMatcherMode` → `SelectHookMode` → `ViewHookMode` 状态机）。它不离开
session 就回答「现在有哪些守卫是上了膛的？」；要加或改一个 hook，用户去编辑
`hooks.json`（菜单自己在脸上这么说），因为在菜单里复制一个设置文件编辑器，正是让
参考实现变成只读的那种维护负担。它唯一改变的是**信任**（
`docs/state/config-review.md`）：一个 hook 只在用户信任它之后才跑，而这个菜单就
是启动审查之后改它的地方。

（codex 也长了自己的 `/hooks` 浏览器 —— `hooks_browser_view.rs`，一个两页的事件
表 → handler 视图，为它的插件来源带信任审查列。我们的改跟 Claude Code 的四层形
状，带编号的 `❯` 行 / `Hook details` 页正是这个功能被要求时的样子，而信任放在行
上和详情页上，而不是放在列里。）

## 它住在哪

**第四个替换 composer 的内联 picker**，排在 `/model`、`/login` 和 `/settings` 旁
边：它在 `/model` picker 的分隔线之间、取代 composer 渲染，把流式条留在自己上方
（`ui::layout` 的 `strip_above_rows` / `view_split` —— 在回合中途打开它从不藏起
正在跑的回合），打开时拥有每一个键，关闭后回到 composer。像 ↓ 后台管理器条带一
样，它**没有文本输入**，也没有输入即搜索：导航就是全部语法。所以菜单竖起时外框**
不显示硬件光标** —— 权限 prompt 的规则（`ui::cursor_visible`）：一个菜单没有东西
可供光标指，而一个带光标动画的终端（kitty）会在你挑的任何座位上闪。*座位*仍然跟
着被高亮的 `❯` 行（`ui::layout` 的标记扫描），所以一个无视这个隐藏的终端 —— 以及
菜单关闭时光标的回归 —— 会从某个有意义的地方开始。一个没有 `❯` 的页面 —— hook 详
情、某个事件的空状态 ——改坐在它的收尾提示之后，全屏 overlay 的规则（
`overlay_cursor_seat`）：绝不坐在底部分隔线上，那种页面的远角正是这种页面过去停
放座位的地方（`docs/ui/view-flow.md`）。

因为浏览不碰任何正在跑的东西 —— 一个信任变化只重绑*下一次*后端构建 —— `/hooks`
在回合中途工作（`/model` 的规则），而关闭纯是塌缩 —— 循环没有任何东西要回收。

## 四层

一台小小的栈机器（`app::HooksLevel`），Esc 一次弹一帧：

1. **Events** —— `Hooks` 压在 `{N} hook(s) configured` 上，当任何 hook 不受信任
   时一条琥珀色注记（
   `2 hooks are not trusted and do not run — open one and press Enter to trust it`）、
   那行信息（
   `ℹ Open a hook to trust it, or to stop trusting it. To add or modify hooks, edit hooks.json directly or ask zeroth. See docs/tools/hooks.md`），
   以及**十四**个 [`HookEvent`] 作为带编号的行 ——`{n}.  {Event} ({count})`，计数
   用选中强调色（零时省掉，Claude Code 的形状），而事件的一行摘要在对齐的描述列
   里。当文件里装着可跑的 hook 而 session 把它们关着（`/settings`、
   `ZEROTH_HOOKS=0`）时，一条偏红的注记这么说 —— 参考实现的「受策略限制」槽
   位。
2. **Matchers** —— `{Event} - Matchers` 压在事件的多行描述上（stdin 载荷 + 退出
   码语义，**按这个运行器的实现**），然后每个配置过的 matcher 组一行 ——
   `{n}. [User] {matcher}`，描述列里是 `{k} hook(s)`。共用同一个 matcher 字符串
   的组合并成一行（参考实现的分组）；缺席/空 matcher 显示 `(all)`。
3. **Hooks** —— `{Event} - Matcher: {matcher}`（对一个无 matcher 的事件就是
   `{Event}`）、同样的那个事件描述，然后每个 handler 一行 ——
   `{n}. [{type}] {command}`（handler 的 `statusMessage` 在设了时代替命令，
   Claude Code 的 `getHookDisplayText`）、一个等待信任的 hook 后面跟着琥珀色的
   `⚠ not trusted`，描述列里是 `User Settings`。
4. **Hook details** —— `Hook details` 压在一个对齐的字段块上（`Event:` /
   `Matcher:`（只在事件匹配某样东西时）/ `Type:` /
   `Source:  User settings ({hooks.json path})` / `Trust:` —— `✔ trusted`，或者
   `⚠ not trusted — it does not run` —— 对一个能跑的 hook）、`Command:` 标签压在
   一个圆角暗色边框的框上，框里装着**真实**命令，逐词换行（`ui::wrap_output`，保
   留空格 —— `statusMessage` 在这里绝不代替），设了时还有一行
   `Status message:`，以及收尾那句去编辑 `hooks.json` 的指示。**Enter 切换这个
   hook 的信任**（`Action::SetHookTrust` —— 页面立刻更新，循环把它记进
   `trust.json` 并重绑这个 session 所跑的 hook），所以提示读作
   `Enter to trust · Esc to go back` 或
   `Enter to stop trusting · Esc to go back`；一个从不跑的 handler 没有信任，提
   示就保持 `Esc to go back`。一个详情页的换行命令框长过了终端时，它**从底部锚定
   **渲染，跳过的顶部**流进真实 scrollback**，于是整页仍然可读、提示/分隔线仍在
   屏幕上（`docs/ui/view-flow.md`；`smoke.sh` Phase 85）。

第 1–3 层共用选择语法：↑/↓ 移动（两端环绕）、Home/End 跳、**数字 1–9 跳转并激活
**它们那行（`AskUserQuestion` 模态框的规则）、Enter 下降、Esc 上升（在 Events 处
则关闭）、Ctrl+C 直接关闭（picker 家族的规则 —— 从不退出）。超过
`HOOKS_MENU_MAX_ROWS`（5，参考实现的 `visibleOptionCount`）的行在**以选择为中心
的窗口**里滚动（`centered_window`，`/model` 列表那个），窗口边缘行的标记 cell 里
是暗色 `↑`/`↓` 溢出标记 —— 编号保持 **绝对**，所以第二个 mock 的 `↑ 2. … ↓ 6.`
窗口读起来和 Claude Code 画的一模一样。

钻进一个什么都没配置的事件仍然会下降 —— 列表被参考实现的那两行暗色文本替换（
`No hooks configured for this event.` /
`To add hooks, edit hooks.json directly or ask zeroth.`）—— 所以描述文本（*
什么*会触发，以及怎么触发）对每个事件都够得到，有 hook 没有都一样。

## Matcher 是派发器的真相，不是参考实现的拷贝

一个事件恰好在我们的派发为它传一个匹配查询时显示 matcher 层（
`hooks::event_has_matchers`，镜像 `llm/hooks.rs`）：tool 事件匹配**tool 名**，
`SubagentStart`/`SubagentStop` 匹配**agent 类型**，`SessionStart` 匹配**来源**，
`SessionEnd` 匹配**原因**，`PreCompact`/`PostCompact` 匹配**触发方式**，
`StopFailure` 匹配**错误类型**，`Notification` 匹配**通知类型**。`Stop` 和
`UserPromptSubmit` 什么都不匹配 —— 它们的组不管有没有 matcher 都跑 —— 所以在它们
上面按 Enter 直接跳到第 3 层（事件的所有 handler，摊平），而它们的详情页不显示
`Matcher:` 行：一个引擎会忽略的 matcher 绝不能被呈现得像它做了过滤。

那些逐事件的摘要/描述字符串（`hooks::event_summary` / `event_description`）同样
是 **这个运行器**的语义 —— 退出 `2` 阻塞并以 stderr 作为理由，退出 `0` + stdout
JSON 是一个裁决，其他退出不阻塞，引擎忽略 block 的地方就忽略（`SessionStart`、
`SubagentStart`），结果被丢弃的地方就发完不管（`PostCompact`、`SessionEnd`、
`StopFailure`、`Notification`）—— 不是参考实现的，它的退出码意味不同的东西。

## 纯/边界的切分

- **`hooks::overview`**（纯）：`HooksOverview::from_file` 把一个解析好的 [
  `HooksFile`] 消化成显示树 —— 按注册表顺序的全部十四个事件，每个带它的 matcher
  组（按 matcher 字符串合并，保留配置顺序）和每个 handler（任何 `type`，包括被跳
  过的种类：它们是*配置过的*，那正是浏览器所报告的），解析成
  `{kind, content, status_message}`，外加对一个能跑的 hook，它的 `trust_key`（它
  的 `hooks.json` 和 `trust::HookIdentity`）—— 而 `with_trust` 回答每一个的
  `trusted`。文件点名而我们不建模的事件从不显示（参考实现也这样遍历它自己的注册
  表）。
- **`app::hooks_menu`**（纯状态）：打开着的菜单 —— overview 快照、`Source:` 路径
  字符串、启用 flag，以及层栈 —— 外加 `on_key_hooks`。打开是
  `App::open_hooks_menu(overview, source, enabled)`，边界注入的接缝。
- **`ui::hooks_view`**（纯渲染）：`hooks_view_lines` 一行一行地构建整个带框正文
  （↓ 管理器条带的模式 —— 高度作为 `lines.len()` 落出来，所以多行描述和换行命令
  框不需要固定槽位布局），`hooks_menu_height` 经由 `view_height` 把它安在条带下
  面，`render_hooks_menu` 绘制它。样式是 `ui/theme.rs` 里的 `HOOKS_*`，复用
  picker 家族的强调色；每一层的标题穿这些页面唯一的那身标题装束 —— 粗体，浸在活
  动主题的横幅渐变里（`ui::header::page_title_line`、`docs/ui/theme.md`），像
  `/mcp` 管理器的：两个菜单都是深入几层的走查，而那个大标题正是回答「我在哪儿
  ？」的那一行。
- **`tui`**（边界）：`Action::OpenHooksMenu` 分支从每个配置过的 hook 推导
  overview —— 用户的文件和项目的，按 bootstrap 时读的样子 —— 带上 `trust.json`
  记录下来的信任（运行器恰好跑受信任的那一份，所以浏览器和运行器永远不可能不一
  致），解析显示路径（`llm::hooks::hooks_file_path` → `ui::display_cwd` 的 `~`
  缩短），读活着的启用 flag，并把这三样都交给 `App`。`/resume` picker 那套：纯命
  令返回意图，循环供给数据。

一个缺席或空的 `hooks.json` 仍然打开这个菜单 —— `0 hooks configured`，每个事件都
是零 —— 而一个畸形的照空浏览，启动时那条 toast（`docs/tools/hooks.md`）携带着真
正的错误。

## 这刻意不是什么

除了信任之外不编辑，不重新加载（overview 在打开时快照，正如运行器在启动时快照那
些文件），也没有新的 `StreamEvent`/`HistoryItem` —— 这个菜单是配置之上的 UI，绝
不是对话。
