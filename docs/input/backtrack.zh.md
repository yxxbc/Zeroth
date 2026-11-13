# Esc Esc —— 编辑上一条消息（回溯）

codex 的**回溯**特性（`tui/src/app_backtrack.rs`）的移植：从一个空闲、空的
composer 按两次 **Esc**，会退回到上一条 user 消息，在 transcript overlay 里高亮
显示它，并且在 Enter 时把对话回卷到那条消息之前，把它的文本放回 composer 供编辑
和重发。

## 手势

```
idle, empty composer, a previous user message exists:

  Esc            → primed: the footer slot shows "esc again to edit previous message"
  Esc (again)    → the Ctrl+O transcript overlay opens as a *preview*, the last
                   user message highlighted (reversed video)
  Esc / ←        → step the highlight to the next-older user message (stops at the oldest)
  →              → step back toward the newest (stops at the newest)
  Enter          → confirm: drop the highlighted message and everything after it
                   from the conversation, close the overlay, repaint the truncated
                   conversation, and prefill the composer with the message's text
  q / Ctrl+O     → cancel: close the overlay, conversation untouched
```

预置是忠于 codex 的：**没有超时**（它一直持续到第二次 Esc 或任何其他键），而
**任何其他键都会取消预置** —— 下一次按键只是做它平常的事。overlay 预览也可以从
一个已经打开的 Ctrl+O 视图进入：在那里按 Esc（空闲、且有目标）会就地开始预览，
而不是关闭视图 —— codex 的 Ctrl+T → Esc 路径。`q` 与 Ctrl+O 仍然能从预览关闭
overlay（那就是取消它），而每一个滚动键（↑/↓、PgUp/PgDn、Home/End）在预览时
继续有效。

## Esc 现在意味着什么（优先级阶梯）

Esc 保留它既有的每一份工作，顺序也照旧 —— 回溯插在「中断」与「退出」之间：

1. 一个打开的 `?` 快捷键 band / slash palette / `@` 文件 picker → 关掉它
2. 空的 `!` shell 模式 composer → 退出 shell 模式
3. 一个在飞的回合 → 中断它（`docs/input/interrupt.md`）
4. **已预置 → 打开回溯预览；未预置、空 composer、且有一条上一条 user 消息 →
   预置**
5. 一份打出来的草稿 → 什么都不做（codex 的 composer 只在空时对 Esc 行动；
   Ctrl+C 才是清空 composer 的）
6. 否则 —— 空 composer、没有什么可回溯 → 退出

与 codex 唯一刻意的偏离：codex 从不因 Esc 退出，而我们空闲时的 Esc 历史上会。
Esc 仍然**在一个空 composer、没有什么可回溯时**退出 —— 一个新的 session，或者
刚 `/clear` 之后 —— 所以启动屏的行为活了下来；一旦存在 user 消息，空闲的 Esc 就
是「编辑上一条消息」，有草稿时它什么都不做，而退出是 **Ctrl+C**（空 composer）或
`/quit`，两者都早有文档。`?` 快捷键 band 反映当前为真的那一个
（`esc to quit` ↔ `esc esc to edit previous`）。

## 状态

`App::backtrack`（一个 `Backtrack` struct，全是纯状态）：

- `primed: bool` —— 第一次 Esc 武装了这个手势（codex 的 `primed`）。
- `selected: Option<usize>` —— overlay 预览活动期间是 `Some(i)`：对话**user
  消息**里的一个索引（`Role::User` 的历史消息，最旧 = 0 —— codex 的
  `nth_user_message`）。`Role::Shell` 的 exec 头部是用户*敲*的，但不是用户的
  *prompt*；像 codex 一样，步进会跳过它们（如果截断点在它们之前，它们仍会被
  丢掉）。
- `scroll_pending: bool` —— 在预览打开或步进时置位；下一次 overlay 绘制把高亮
  滚进视野（codex 的 `scroll_chunk_into_view`），然后清掉它，这样之后手动滚动
  不会被顶。

`toggle_tool_view`（任何 Ctrl+O / q / overlay-Esc 关闭）以及
`clear_conversation`（`/clear`）重置整个 struct —— codex 在 overlay 关闭与
Ctrl+L 清空时也以同样的方式重置它的 `BacktrackState`。

## 确认

`App::confirm_backtrack`（预览时按 Enter）：

1. 在 `history` 里找到被选中 user 消息的位置，并**在那里截断历史** —— 被选中的
   消息以及它之后的一切（之后的回复、tool call、摘要、shell 单元格）被丢弃；它
   之前的一切留下（codex 的 `trim_transcript_cells_to_nth_user`）；
2. 重置回溯状态并回到 `View::Conversation`；
3. 经由与 ↑ 历史召回相同的路径把消息的文本预填进 composer（光标在末尾，
   palette/shell 模式重新推导）。

那个按键分支返回 `Action::ConfirmBacktrack`（一个专门的动作，不是
`ToggleToolView` —— 循环还会把这个代码重置到那个时点的 checkpoint，
`docs/state/checkpoint.md`），然后跑 `exit_overlay` 加一次
**`ReflowClear::Purge`** 的 `repaint_conversation`，它**从被截断的历史**重建内联
视图（不变量 3 的重绘 —— 截断只是更短的尾部；空结果像 `/clear` 一样重绘）。overlay
立着的时候不发生任何 scrollback 提交（不变量 4），所以没有陈旧的东西要撤回 ——
那次重绘*就是*回卷。已提交输入的 ↑ 历史不受影响（codex 的跨 session 输入历史同样
在 fork 之后存活）。

**为什么要 purge 重建，而不是普通的 overlay 返回。** 回溯*缩短*历史，所以 —— 像
`/resume`（它替换历史）和每一次 resize 一样 —— 重绘必须在重建之前**purge
scrollback 并清空整个屏幕**（不变量 3）。普通的返回只会把排队的提交 flush 到已有
内容之下：它会让被丢掉的那段交流滞留在屏幕上、也滞留在终端自己的 scrollback 里，
只有下一次 resize 才清掉 —— 就是那个「回卷之后它还在那儿」的重复 bug。purge 让
回卷后的对话成为全部记录，屏幕与 scrollback 都是如此。（这是与 codex 唯一刻意的
偏离，它 fork 之后的 scrollback 保留被丢掉的单元格；它在这里造成的重复不值得为了
对齐而保留。）

没有 backend session 可以 fork：一个 `ReplySource` 每回合拿到一个 prompt
（`docs/design.md`），所以截断 `App::history` *就是*整个回滚 —— codex 的
`Op::ThreadRollback` 往返在这里没有对应物。一个真正有状态的 backend 会把它自己的
回卷挂进同一个确认。

预填也恢复附件（随 `docs/architecture/context.md` 更新）：一个
`[Pasted Content N chars]` 占位符在提交时就已经展开，所以完整文本会回来，而
`[Image #N]` 占位符回来时是**有底的** —— 消息现在记录它的临时 PNG 路径，所以确认
时把它们重新绑定到恢复的占位符上（中断-撤销的那套舞蹈：先丢弃任何支撑被覆盖草稿
的配对，而被丢掉的*更晚*的那些 user 消息所孤立的附件排队等临时文件删除），即
codex 的 retained-local-images 重新挂上。

## 渲染

- **高亮** —— `ui::transcript_lines` 用 `Modifier::REVERSED` 补在正常的暗色 user
  样式之上来渲染被选中 user 消息的那些行（codex 的
  `user_message_style().reversed()`）；它的时间戳行保持正常。选区的行范围来自同
  一次遍历（`transcript_selection`），所以高亮与滚动目标不会彼此漂移。
- **滚进视野** —— 纯函数 `ui::backtrack_scroll` 决定新的 `tool_scroll`（只在
  `scroll_pending` 时）：向上滚到刚好显示高亮的顶部，或向下滚到刚好显示它的底部，
  否则不动。`tui::view::Session::draw_tool_view` 在 settle/clamp 之前应用它。
- **页脚提示** —— 已预置期间，页脚槽位（搜索行 / shell 提示的槽位，
  `docs/ui/footer.md`）显示 `esc again to edit previous message` —— `esc` 键像
  搜索提示那样青色加粗，标签暗色（codex 的 `esc_backtrack_hint` 页脚）。它占下
  session 页脚的槽位，并且即使没有注入 session info 也显示，像搜索行一样。
- **overlay 提示** —— 预览期间，overlay 的第二行按键提示换成回溯提示
  （`esc/← to edit prev · → to edit next · enter to edit message · q to
  cancel`），即 codex 高亮分页器的页脚。在预览之前，同一行已经如实说出 Esc 会做
  什么：只要空闲的 Esc 会*开始*预览，它就读作 `q/ctrl+o to quit   esc to
  edit prev`（`TOOL_VIEW_HINT_QUIT_EDIT`），而只有当 Esc 真的关闭 overlay ——
  没有目标、一个运行中的回合、一个 agent session 视图、一个等着用户的模态框 ——
  时才立着经典的 `q/esc/ctrl+o to quit`。提示与按键分支都在同一个共享谓词
  `App::overlay_esc_backtracks` 上把关，所以它们永远不会不一致（旧的
  `q/esc/ctrl+o` 行承诺了一个 Esc 并不会做的退出 —— 就是那个被报告的「我按 Esc
  想退出，却得到编辑上一条消息」）。
- **快捷键 band** —— `esc` 条目现在是三向的、随情境而变：回合运行时
  `esc to interrupt`（一如既往），空闲且有目标时 `esc esc to edit
  previous`，空闲且没有目标时 `esc to quit`。

所有新的样式都住在 `ui/theme.rs` 的 consts 块里（`BACKTRACK_*`、
`TOOL_VIEW_HINT_BACKTRACK`），遵循约定。

## 边界情形

- **composer 非空**：Esc 从不预置（codex 要求 `composer_is_empty`）—— 也从不
  退出：有打出来的草稿时空闲的 Esc 是空操作，codex 的风格，所以它不能把打了一半
  的活扔掉。
- **回合中途**：`turn_active()` 时 Esc 仍然中断 —— 预置要求空闲（codex 的
  `!is_task_running`）。中断落地之后，下一次 Esc 预置：从一个流式回合出发的
  Esc-Esc-Esc 是 中断 → 预置 → 预览，正是 codex 的链。排队积压意味着有活动回合，
  所以预览永远不可能与待派发同时存在。
- **没有 user 消息**（新的、被 `/clear` 过的，或只有错误/系统通知）：没有目标 ——
  Esc 像从前一样退出，而 overlay 里的 Esc 只是关闭 overlay（codex 会贴出
  "No previous message to edit."；我们干脆不进入这个模式）。
- **有一个模态框开着**：一个 tool 权限 prompt 或一个 `AskUserQuestion` 问题可以被
  Ctrl+O/Ctrl+D 在其*之上*打开（`docs/state/permissions.md`），而一个**后台
  agent** 可以在没有回合运行时抬起其中任何一个 —— 所以单凭「空闲」会让 overlay
  的 Esc 预置出一个预览，而它的 Enter 会截断历史、预填的正是模态框已经暂存的那个
  composer，同时还有一个 tool 线程停在 gate 上。所以 `overlay_esc_backtracks` 还
  要求 `!App::modal_open()` —— 同一个 `ui::region_is_modal` 也读的谓词 —— 而提示
  行一如既往地跟随它。
- **`/clear` 与 Ctrl+O 重置**：两者都擦掉回溯状态；除了通过显式地 Esc 重新进入，
  没有任何东西能在一次视图切换后存活。
- **最旧/最新边界**：步进在两端饱和（codex 以同样的方式夹取）。

## 测试

单元测试住在它们所锁定的代码旁边（`app::tests`、`ui::tests`）：预置/取消预置与
优先级阶梯、预览的打开/步进/夹取、确认的截断 + 预填 + 视图翻转、取消路径
（`q`、Ctrl+O）、`/clear` 重置、overlay-Esc 进入、反色高亮 + 选区范围、滚进视野
的决定、页脚提示行与它的 `footer_rows` 槽位，以及三向的快捷键条目。
`scripts/smoke.sh` 的回溯阶段端到端驱动真实二进制：两次交流、Esc-Esc、向更旧步进
一次、Enter，然后断言 composer 持有第一条消息、第二次交流离开了重绘后的屏幕，而
重发的回合仍然流式跑到它的摘要（`Pondered for …`，即第三个回合的）。
