# task tool —— 一份实时清单，而不是一堆 tool cell

Claude Code 的结构化任务列表（`TaskCreate` / `TaskGet` / `TaskList` /
`TaskUpdate`），移植到内联 TUI：模型把多步工作规划成任务，一边做一边把它们标成
`in_progress`/`completed`，而用户在状态行下看着一份**实时清单**，而不是一串
tool call 圆点。

```
(  ●•· ) Setting up project structure… (25s · ↓ 1.2k tokens · esc to interrupt)
  ⎿  ◼ Set up project structure
     ◻ Write core logic › blocked by #1
     ◻ Add tests › blocked by #2
```

## 模型拿到什么

四个函数 tool，只在边界附上了一个 [`TaskRegistry`] 时才提供
（`LlmBackend::with_tasks` —— 与 `with_ask` 同一个模式：没有注册表就没人持有这份
列表，那些 tool 也就不在那儿可供调用）。subagent 永远拿不到 —— 由 lead agent
规划、subagent 执行 —— 一次性的 `/compact` backend 也拿不到。wire 名与其他每个
tool 一样小写（`taskcreate`，……），显示名是 CamelCase（`TaskCreate`），而正是
这一点让上下文重放的小写回退能还原出 wire 名。

这些 schema 就是 Claude Code 的、**减去 `owner` 与 `metadata`**：`owner` 是一个
多 agent 集群的概念（agent 按名字认领任务），而这个会话恰好只有一个 agent 在驱动
这份列表；`metadata` 则是一坨在这个 TUI 里没有任何东西消费的任意数据。删掉它们
而不是接受并忽略，让 schema 保持诚实 —— 模型能发什么，这个特性就做什么。

- `taskcreate` —— `subject`（必需）、`description`（必需）、`activeForm`
  （可选；任务进行中时 spinner 所穿的现在进行时标签）。结果：
  `Task #3 created successfully: {subject}`。依赖是 `taskupdate` 的活，本不该
  出现在这个 schema 里，但一个真实的模型（在 gpt-4o-mini 上见过）还是会把它们
  折进 create 里，用的可能是它刚在那儿读到的 `addBlocks`/`addBlockedBy` 写法，
  也可能是裸的 `blocks`/`blockedBy` —— 所以解析器**四种都认**，而不是悄悄丢掉
  它们，并按 `taskupdate` 的方式把边接上。这些在任务创建之前就校验过，所以依赖
  一个不存在的任务会让整个调用失败（`Task #9 not found`，什么都不创建），而不是
  留下一个半接上的行。
- `taskget` —— `taskId`。结果：`Task #3: {subject}` / `Status: …` /
  `Description: …`（有链接时再加 `Blocked by: #2` / `Blocks: #4`）。
- `tasklist` —— 没有参数。结果：每个任务一行 `#3 [pending] {subject}`，
  `[blocked by #2]` 只点出**未完成**的阻塞者；空时是 `No tasks found`。
- `taskupdate` —— `taskId` 加上 `subject`/`description`/`activeForm`/
  `status`（`pending`/`in_progress`/`completed`/`deleted`）/`addBlocks`/
  `addBlockedBy` 中的任意几项。结果：`Updated task #3 {changed fields}`；未知
  id：`Task #9 not found`。`deleted` 永久删除该任务，并把它从其他每个任务的依赖
  列表里刷掉。

上面所有这些文本都是 Claude Code 自己的结果字符串，所以在它们上面训练过的模型
如鱼得水。任务 id 从一个**高水位线**往上数，而这条线在删除之后仍然保留
（`TaskStore::next_id`）：任务 #1–3 都被删掉之后，下一次 create 是 #4，证明旧的
那些是真的没了，而不是重新编号。

## 藏起来的 cell，看得见的清单

Claude Code 对这些 tool 调用**什么都不渲染**（它的 `renderToolUseMessage()`
返回 `null`）—— 实时清单*就是*反馈，否则一个五任务的计划会提交十几个纯噪音的
`● TaskUpdate` cell。我们也这么做，但在每个要紧的地方都留下记录：

- **内联 scrollback：什么都没有。** 一次 task 调用不提交 cell，而它之前该回合的
  叙述文本仍作为自己的 `●` 消息定稿（那套 `ToolStart` 冲刷动作），所以每一轮的
  散文都读作自己的一条圆点 —— Claude Code 的精确节奏。
- **条：清单。** 一个回合在活动时（`strip_has_status`）且列表非空时，这些行直接
  渲染在**状态行下方**、它的尾部空隙之上：tool 装订边里的
  `⎿ {glyph} {subject}`。
  `◻` 待办（暗色）、`◼` 进行中（字形用系统青色，subject 亮）、`✔` 已完成（绿色
  字形、暗色带删除线的 subject），还有一个暗色的 `› blocked by #1, #2` 后缀，
  点出一个被阻塞任务的**未完成**阻塞者（已完成或已删除的阻塞者在下次渲染时
  掉出）。
  subject 像页脚那样截断到一行（省略号）；超过 [`TASK_MAX_ROWS`] 之后，列表按
  进行中 → 未被阻塞的待办 → 被阻塞 → 已完成的优先级排列，其余折进一行暗色的
  `… +N pending, M done` 尾行。
- **静止时：独立块。** 一个还有活没干完的计划不会在回合结束时消失 —— 同样的行
  坐在 composer 上方、Claude Code 那条暗色计数行之下，所以用户阅读和打字时，剩下
  的东西都在视野里：

  ```
    1 tasks (0 done, 1 open)
    ◻ Review demo output with user
  ```

  同样的字形、同样的截断、同样的折叠（`task_rows_lines` 同时构建两者）；只有前缀
  不同 —— `⎿` 装订边挂在 spinner 上，而静止时没有 spinner，所以这些行采用
  composer 自己的两空格内缩。计数行在任何任务在跑时多一个 `N in progress` 分句，
  而它的三个数字**划分**整份列表 —— `open` 只是那些还没开始的任务，也就是
  Claude Code 在那里印的 `pendingCount`，所以一个运行中的任务只被报告一次，而
  不是既算进行中*又*算待办（三个任务的列表上是
  `3 tasks (1 done, 1 in progress, 1 open)`，绝不是 `2 open`）。
- **一个完成的计划会退场。** 每个任务都完成后，这份列表就完成了它的使命：完成它
  的那个回合继续显示全绿的行（那份回报），而那个回合结束时它被**整份丢掉**
  （`App::retire_finished_tasks` → `TaskStore::retire_if_finished`，由循环的
  `dispatch_after_turn` 调用，它还会重新同步共享注册表，好让模型下一次
  `tasklist` 与之一致）。所以它是*没了*，而不是藏起来：没有静止块，之后的回合
  什么都没有，而且 —— 这才是要紧的原因 —— 下一次 `taskcreate` 开启一个真正的新
  计划，而不是往三个旧的勾上再添一行。**id 的高水位线保留下来**，所以那个新计划
  从 `#4` 开始：与删除给出的连续性证明相同。一次回退（`/resume`、Esc-Esc 回溯）
  对它恢复的快照套用这条规则，因为回退也落在回合之间。
- **spinner 穿着活动任务。** 只要某个任务是 `in_progress`，状态动词就是
  **第一个**
  这样的任务的 `activeForm`（回落到它的 `subject`），而不是回合那个古怪的动词
  —— `✻ Setting up project structure…` —— 也就是 Claude Code 的 spinner 规则
  （`currentTodo.activeForm ?? currentTodo.subject ?? randomVerb`）。在渲染时
  派生
  （`App::task_verb` → `ui::status_line_with_verb`），从不存储，所以任务一完成，
  动词在回合中途就弹回去。
- **Ctrl+O：完整记录。** transcript 视图把每次 task 调用列成一个普通 tool cell
  （`● TaskCreate(Set up project structure)` 压在它的 `⎿` 结果装订边之上）——
  调试面不藏任何东西。
- **Ctrl+D / 模型的上下文：原生重放，逐字。** 每条记录重放成一对 provider 原生的
  `tool_calls` + `tool` 结果（`context::context_messages`）—— 在 provider 自己的
  call id 下（`TaskCallRecord::call_id`，按模型的顺序取自该轮的公告），并且与该
  次调用一起落在该轮那一条 `tool_calls` 消息里
  （`docs/providers/prompt-caching.md`）—— 携带**模型发来的原始参数** ——
  `TaskCallRecord::arguments`，正是为了让重放能这么做，才把它保存在头部摘要
  旁边。摘要按设计是有损的（`#1 → completed` 是头部，不是载荷），重放*那个*
  会让后来的回合在一条结果行之上读到 `taskupdate {}`，它自己的 subject、
  description 和依赖接线都从对话里没了。在字段存在之前写下的记录重放为 `{}`，
  与它过去的样子完全一样。

## 它怎么流动

一个新事件承载整个解析 —— 一次 task 调用是瞬时的、不需要权限、不流任何输出，
所以 `ToolBatch`/`ToolStart`/`ToolEnd` 三件套只会存在然后被再过滤掉：

```
StreamEvent::TaskCall { name, args, arguments, output, ok, tasks }
```

`args` 是 Ctrl+O cell 显示的一行头部摘要；`arguments` 是模型发来的原始
JSON，记录
为上下文重放而保留它（见上）。

`llm::agent::run_agent` 路由 task 调用的方式与路由 `agent` 调用一样：它们被排除
在 `ToolBatch` 公告之外（一整轮只有 task 调用时什么都不公告），被权限接缝跳过，
经普通的 `execute` 闭包执行 —— 它的结果通过 `ToolOutcome::tasks` 携带调用后的
快照 —— 并按模型调用顺序各自发出一个 `TaskCall`，结果文本像其他任何调用一样作为
`tool` 消息反馈回去。被 **Max tool calls** 上限拒绝的 task 调用以那段上限
文本作答
并且什么都不发（被拒绝的 `agent` 调用那条规则：没有 cell，只有答案）。

执行器那一半是 `llm::task::run_task_tool`：解析参数，把操作应用到共享的
[`TaskRegistry`]（`Arc<Mutex<TaskStore>>`，后台注册表的兄弟），然后交回分裂的
结果。所有语义 —— id 分配、字段差异更新、依赖接线、删除清理、每一个结果字符串
—— 都住在纯的 [`crate::tasks`] 模块里，不用终端就能单元测试。

循环的 `TaskCall` 分支镜像 `ToolBatch`：冲刷已流出的片段（每一轮的文本成为自己的
圆点），在安全的边界落定被扣住的后台完成，然后 `App::record_task_call` —— 它追加
那条隐藏的 `HistoryItem::TaskCall` 记录，把快照存到 `App::tasks` 供条使用，并把
结果文本计入 token 计数（`↑`，那个上传回来的箭头），与一个可见的 tool 结果完全
一样。

## 记录与三次回退

`HistoryItem::TaskCall(TaskCallRecord)` 保存
`{name, args, output, ok, timestamp, tasks}` —— **调用后的快照搭上每一条记录**，
正是这一点让三次历史回退全都精确，而不必重放参数：

- **`/resume`** —— rollout 序列化每条记录（`session::ItemRecord::TaskCall`；
  老构建跳过未知类型，前向兼容契约），而一次加载从*最后*一条记录的快照重建列表
  （`tasks::latest_snapshot`）—— 包括高水位的 `next_id`，所以恢复的会话继续往下
  数。
- **Esc-Esc 回溯** —— 回退到更早的用户消息会截断历史；列表重置到**截断之前**的
  最后一份快照（对话在那里实际拥有的状态）。
- **`/clear`** —— 连同其他一切清空列表。

每一种情况下边界都把共享注册表同步到 app 的快照（`tui` 的
`sync_task_registry`），好让模型下一次 `tasklist` 与条一致。

## 离线演示

dummy 把一个 `todo`/`task` 的 prompt 演成一段脚本化的生命周期
（`stream/dummy/turns.rs::tasks_turn`）：回合函数驱动一个**真实的** `TaskStore`
并发出真正的输出与快照 —— 三次 create、依赖接线、`in_progress` → `completed`
—— 所以离线清单、spinner 覆写和 Ctrl+O cell 与真实执行器所产生的逐字节相同，而
它像其他每个场景一样以共享的 `handoff!()` 句子收尾。它以**还有活没干完**结束
（#1 完成、#2 运行中、#3 待办），正是这一点让静止块和跨回合列表可以在离线驱动
—— 而 `smoke.sh` Phase 69 断言的正是这个。

另一个结尾有自己的场景：一个还说了**finish** 的 todo prompt
（`tasks_finished_turn`）把两个任务走到全 ✔ 然后就停在那儿，于是退场也能被驱动
—— 闭包显示到它自己那个回合结束，静止时什么都不显示，而下一回合的 spinner 干净
地起来（`smoke.sh` Phase 70，被报告的那个过期列表 bug）。把它拆成两个而不是扩展
第一个演示，能让各自的结尾都不含糊：一个脚本只能演示其中一种。

[`TaskRegistry`]: ../src/tasks.rs
[`crate::tasks`]: ../src/tasks.rs
[`TASK_MAX_ROWS`]: ../src/ui/theme.rs
