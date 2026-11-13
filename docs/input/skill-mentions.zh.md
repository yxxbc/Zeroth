# `$` 技能提及 —— 输入框下方的一个 skill picker

日期：2026-08-12

## 目标

在 composer 里敲 **`$`**，会在输入框下方打开一个 **skill picker** ——
openai/codex 的 `$` 技能提及弹窗，用这套 TUI 的 band 样式呈现：敲 `$`，边敲边
对已发现的 skill 做模糊过滤，↑/↓ 选择，**Tab/Enter 插入这条提及**（`$name` ——
记号留着，再加一个分隔空格），Esc 关掉。这条提及可以待在**消息的任何位置**
（`Hello world $dataviz …chart this`），而提交一条带提及的消息，会让模型在回答
之前**加载那个 skill 的手册**，除非那些指令已经在它的上下文里：

```text
────────────────────────────────────────────────────────────────────────
❯ $
────────────────────────────────────────────────────────────────────────
  dataviz        Generate or edit charts for websites, games, a…
→ skill-creator  Create or update a skill
```

## 参照实现怎么做（来自 `/tmp/codex`、`/tmp/claude-code-source-code`）

- **codex 的触发**（`tui/src/bottom_pane/chat_composer/completion_target.rs`）：
  `$` 必须开启一个以空白分隔的 token，且光标在它里面 —— `foo$bar` 和 `US$5`
  从不触发。提及名字延续于 `[A-Za-z0-9_-]`（`is_mention_name_char`），在第一个
  别的字节处结束，所以 `$dataviz,` 仍然查询 `dataviz`。一个裸 `$` 列出全部。
- **codex 对 shell 的排除**（`dollar_query_kind`）：真正是 shell 语法的东西从
  不打开弹窗 —— 位置参数（`$1`）、`$-`/`$_`，以及那些众所周知的大写环境变量
  （`is_common_env_var`：`PATH`、`HOME`、……）。带小写的查询则总是搜索（skill
  名是小写的；弹窗的模糊匹配折叠大小写）。
- **codex 的弹窗**（`skill_popup.rs`）：每个匹配一行 —— 名字，然后是一段暗色的
  描述，放在按最宽可见名字对齐的列里 —— 最多 8 行，↑/↓ 移动，Tab/Enter 插入
  `$name` 加一个尾随空格（复用已有的后续空格 ——
  `advance_past_completion_separator`），Esc 关掉并记住这个 token，使它不再重
  开。
- **codex 的提交**（`core/src/session/turn.rs`、`skills/src/selection.rs`、
  `ext/skills/src/fragments.rs`）：文本**原样**送到模型；回合开始时 codex 重新
  扫描其中的 `$token`，用已发现的 skill 解析出无歧义的名字，读每个
  `SKILL.md`，并为每条提及注入一条 `<skill>…</skill>` 用户消息。Codex 没有
  skill *tool*，所以急切注入是它加载 skill 的唯一办法。
- **Claude Code**：根本没有 `$` 提及 —— skill 通过 `Skill` tool 加载（渐进披
  露：上下文里一份带预算的清单，tool 按需取正文），或者一个 `/skill-name` 斜杠
  命令。

## 映射到这套代码库

这个 picker 是输入框下方的**第四条 live-region band**，与 `@` 文件 picker
（`docs/input/file-search.md`）完全平行 —— 但它的匹配**同步**地从已经发现的
`SkillRegistry` 推导出来，所以和文件 picker 不同，它没有异步往返：是调色板的
按需推导形状（`app::COMMANDS`）加上文件 picker 的 token 触发。

**提交一条提及走的是 `Skill` tool，不是 codex 的急切注入。** 这套 TUI 已经有
Claude Code 那个带两段文本切分的 `skill` tool（`docs/tools/skills.md`）：可见的
`● Skill(name)` cell、在 `ToolCall::context_output` 上的正文、Ctrl+D 的回放、
rollout 的往返，以及一个 subagent 自己的副本，全都来自它。所以提及在消息里保持
纯文本，而**指引**让模型去调那个 tool：

- `skill` tool 的简短描述声明了 `$<name>` 的提及规则，并说每个 skill 只在它的
  指令还留在上下文里时加载一次。它骑在提供这个 tool 的每个请求上，所以
  `<system-reminder>` 那份清单保持是裸名册，把它的预算只花在名字和描述上；
- 活体验证过：`live_dollar_mention_loads_the_mentioned_skill`
  （`tests/live_openrouter.rs`）证明一个真模型会为一句裸的
  `Use $mixology — name a cocktail` 调用 `skill("mixology")`，并照加载来的正文
  去做。

与 codex 刻意的分歧：他们的注入是确定的，但不可见（没有 cell，没有*是否*需要它
的记录）；tool 这条路把这次加载作为绿色的 `● Skill(name)` cell 展示给用户，把正
文留在 transcript 之外，并要求模型复用一份已经在上下文里的手册。如果模型还是又
调了一次，执行器只返回一条简短的「已经加载过」成功提醒，不重读磁盘、不重复正文
（`docs/tools/skills.md`）。一个没匹配上的 `$name` 在两种设计里都是空操作 —— 字
面文本抵达模型。

它也让提及在**历史里保持字面**，这正是让 `/resume` 或 Esc-Esc 回溯能诚实回放的
原因：消息仍然读作 `$mixology`，模型重新决定。之前加载过的正文通过它们普通的
tool 结果回放；去重跟随仍留在上下文里的正文，所以回溯过某次加载之后，那个 skill
可以再次加载。

tool 这条路换掉的是确定性 —— 它取决于模型是否遵守描述里的指引 —— 所以这一点是
量出来的，不是假设的。两个 live 提及测试都跨五个模型跑过
（`openai/gpt-4o-mini`、`anthropic/claude-haiku-4.5`、`qwen/qwen3.6-flash`、
`deepseek/deepseek-v3.2`、`openai/gpt-oss-120b`），经 `ZEROTH_LIVE_MODEL`，十次
运行全都加载了被提及的 skill —— 包括
`live_an_embedded_mention_still_loads_the_skill`，它的 prompt 是那个难案例：名
字被丢在句子中间，没有祈使句，没有「use」，任何地方都没出现 *skill* 这个词。那
正是区分一个真机制和一个只在明确措辞上奏效的 prompt 伎俩的案例。

如果真有人看到某个模型忽略了提及，答案**不是**采纳急切注入（那会往上下文里建第
二条路，还得重新挣得它自己的 cell、历史项和 resume 处理）。答案是预先播种那次调
用：循环在回合开始时解析这条提及，自己合成 `skill` 调用，执行器经同一条路解析它
—— 同样的 cell、同样的记录，没有往返，没有合规风险。它之所以不是默认，是因为它
把决定权从模型手里拿走，而一条提及不总是请求：「这次就别管 `$dataviz` 了」是一
句 codex 的急切注入读不懂、而这个设计读得懂的话。

### 纯核心（`skills.rs` —— 在 skill 模型的其余部分旁边）

- `SKILL_MENTION_PREFIX`（`'$'`）、`COMMON_ENV_VARS`（codex 那张表）。
- `mention_token(text, cursor) -> Option<MentionToken>` —— 上面那套 codex 语法；
  `MentionToken { range, query }` 是 `$` band 的 `AtToken`
  （`range` 从 `$` 跨到名字序列，绝不含尾随标点）。
- `shell_flavored_query(query) -> bool` —— 弹窗那一侧的排除，留成一个单独的谓
  词，好让扫描器保持为一次诚实的解析。
- `SkillMatch { name, description, score, indices }` +
  `rank_skills(query, &[SkillMetadata])` —— 对每个**名字**做
  `file_search::fuzzy_match`，分数最高的在前，同分靠稳定排序保持发现（优先）
  顺序；空查询列出全部。没有上限 —— band 用 `centered_window` 开窗，也就是
  `/` 调色板的那个窗口。

### 应用状态（`app/skill_picker.rs`）

- `App::skill_picker: Option<SkillPicker { selected, query }>` —— 调色板的形
  状：只存高亮，匹配按需推导。
- `App::skills`（私有）+ `App::set_skills` —— 边界在每次清单渲染旁边注入注册表
  的**已启用**快照（`tui::models::Session::sync_skill_listing`），好让 picker、
  清单和 tool 集合永远一致；为空则让 `$` 是一个普通的美元符号。
- `refresh_skill_picker(had_mention)` —— `refresh_file_search` 的逻辑套在
  `mention_token` 上：在 None→Some 的转换时打开（Esc 关掉在这条提及内是粘性
  的），查询变了就重置高亮，提及消失就关闭。在 `!` shell 模式下被抑制（那里
  `$VAR` 是真的 shell 变量），`skills` 为空时也被抑制。
- `skill_band_active()` —— 开着**并且**光标还在一条可用的提及里：行是从活的
  token 推导出来的，所以光标一移出提及，band 就藏起来并交还 ↑/↓，不必等某次编
  辑去关状态。
- `skill_matches()` / `highlighted_skill_match()` / `move_skill_selection` /
  `accept_skill_selection` —— 接受把 token 换成 `$name`（记号留着 —— 它正是
  在提交文本里标记这条提及的东西）加一个分隔空格，**复用**已有的后续空格而不是
  加倍，光标放在分隔符之后。

### 按键分发（`App::on_key_conversation`）

`skill_open = skill_band_active()`，与文件 picker 的那几个分支完全平行，并由构
造与它们互斥（一个 token 恰好以一个记号开头）：Esc 关掉（粘性），↑/↓ 移动，有
高亮匹配时 **Tab/Enter 接受** —— 否则它们落空，所以一个没匹配上的 `$query` 仍
然排队/提交。composer 的每一次编辑（Char、Backspace、Delete、粘贴、附加图片）都
会快照 `had_mention`，并在调色板/shell/文件的刷新旁边重新推导 picker；每一条消
耗 composer 的路径（提交、排队、Ctrl+C 清空、`/clear`、替换 composer 的那些
picker、Ctrl+R 搜索、一次回溯）都在 `file_search` 旁边把它关掉。

### 渲染（`ui/menu.rs`）

这个 band 共用调色板的槽位：`band_rows += skill_menu_rows`。

- `skill_menu_rows(app, width)` —— `skill_menu_lines` 构造出来的行数，即调色板
  的 `menu_rows` 规则，好让预留的行和画出来的行在每个宽度上都一致：关着时 0
  行（光标移出提及也算），落空时一行占位（`No matching skills`，缩进到名字那一
  列），否则就是匹配项，上限 `SKILL_MENU_MAX_ROWS`（5；更长的列表滚动），下面
  再放计数器。
- `skill_menu_lines(app, width)` —— 每个匹配一行，开窗
  （`centered_window`，`/` 调色板的那个窗口）：↓ 把高亮往下走到五行中间，然后
  列表在它下面滚动，所以它上面*和*下面的 skill 都留在视野里，只在列表到头的地
  方才贴边滑（最前几行锚在顶部，最后几行与尾巴齐平）。它以前用的那个贴边
  `menu_window` 要等高亮走到最底一行才滚，然后把它按在那儿，把更远的一切藏起
  来。调色板的 `→` 标记落在选中的那行上（其余缩进），名字里被查询匹配到的字节
  **加粗**（`file_menu_highlight` 在偏移 0 处复用），然后 —— 越过一个按最宽可见
  名字 + `FILE_MENU_GAP` 定宽的名字列 —— 是 skill 自己的描述，按宽度用 `…`
  切。选中的行整行亮青色，其余暗色（调色板的惯例；`ui/theme.rs` 里的
  `SKILL_MENU_*`）。行下面是调色板那个暗色的 `(n/total)` 计数器
  （`menu_counter_line`），挂在名字下方：哪一个匹配被高亮，在查询匹配到的多少
  个之中 —— 七个 skill 上的一个裸 `$` 显示 `(1/7)`，`$alp` 收窄到一个之后显示
  `(1/1)`。一个窄到放不下整个计数器的终端宁可不显示它，也不切掉一个数字，而落
  空时没什么可数。

### 离线 dummy 与 demo

`skills` 那个场景也答应 `$dataviz` 提及（`cue.mentions("$dataviz")` 就在
`"skill"` 旁边 —— `stream/dummy/scenario.rs`），所以离线的首次运行会为一条提及
演出真的 `● Skill(dataviz)` 加载，和活模型一模一样，而 smoke 套件能在没有网络的
情况下驱动整个往返。

## 测试

- `skills`（单元）：token 语法（文本中间、终结符、边界规则、裸 `$`）、shell 风
  味谓词、排名（按名字模糊、同分稳定、空查询 = 发现顺序）。
- `app`（单元）：开/关/粘性 Esc/shell 模式抑制/无 skill 时惰性；选择的移动 +
  环绕 + 重置；Tab/Enter 接受（记号留着、空格复用、光标落座）对落空时的落穿；与
  调色板和文件 picker 的互斥；每一条消耗路径都把它关掉。
- `ui`（单元）：行数（关着/占位/上限五行）、每个宽度下行数与画出的行相等、对齐
  的名字/描述列、`…` 切、匹配字节加粗、选中颜色、`(n/total)` 计数器（跟随高
  亮、数查询匹配到的、宽度放不下时省去）、居中窗口（上面有位置时 `→` 在中间那
  行，两端时锚在顶部、与尾巴齐平）、`band_rows` 的计入、光标移出提及时的隐藏。
- `stream`（单元）：`$dataviz` 这个 cue 选中 skills demo。
- `scripts/smoke.sh` Phase 79：对着一个种了七个 skill 的目录 —— 敲 `$`，band
  列出前五个并带描述，压在 `(1/7)` 上；↑ 环绕到最后一个，窗口跟着它，压在
  `(7/7)` 上；↓ 环绕回第一个，再往下三步把 `delta-skill` 放到中间那行，
  `alpha-skill` 滚出去，压在 `(4/7)` 上；过滤收窄到 `(1/1)`；Tab 把这条提及补
  全进 composer；提交一条提及演出 skill demo（`● Skill(dataviz)` cell，正文一
  点都不内联）。
- `tests/live_openrouter.rs`（`--ignored`），提及契约的两半 —— 两者都跨上面点名
  的五个模型验证过：`live_dollar_mention_loads_the_mentioned_skill`（一句明确的
  `Use $mixology — …`）和 `live_an_embedded_mention_still_loads_the_skill`
  （名字丢在句子中间，别的什么都没在要求加载）。
