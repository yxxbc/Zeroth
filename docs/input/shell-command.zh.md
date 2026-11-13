# 用 `!` 跑 shell 命令 —— 从 composer 里跑一条本地命令

日期：2026-06-13

## 目标

**输入 `!command` 再按回车，就在本地跑那条命令** —— composer 里的一个 shell
逃生口，就像 openai/codex 的 `!` shell 模式那样。命令和它的输出像一次 tool call
一样落进对话（成功绿色、失败红色，在 Ctrl+O 视图里可展开），而一条长命令可以用
Esc 打断。

## codex 是怎么做的（读自 `/tmp/codex/codex-rs`）

- **触发**：把 `!` 作为第一个字符输入，会翻转 `DraftState::is_bash_mode`，并把
  这枚 `!` 从 textarea 里*吸收*掉（`chat_composer.rs:3349`，
  `sync_bash_mode_from_text`）。页脚提示变成**浅红色**的 `Shell mode`
  （`shell_mode_footer_line`，`chat_composer.rs:3152`）。在已经空了的 composer
  上按 Esc 退出该模式。
- **提交**：回车剥掉 `!`，跑剩下的部分（`input_submission.rs:24`，
  `submit_queued_shell_prompt`）。一个空的 `!` 反而贴出一条帮助 notice
  （`Prefix a command with ! to run it locally`）。
- **执行**（`core/src/tasks/user_shell.rs`）：用户的登录 shell（`-lc`）、
  **没有 sandbox**、1 小时超时，异步 spawn 所以模型的 turn 不被阻塞；可以有多
  个并发跑。输出经 `ExecCommandBegin`/`ExecCommandEnd` 流回来。
- **渲染**（`exec_cell/render.rs:375`）：一个 `You ran` exec cell，命令做语法
  高亮，stdout+stderr 聚合，最多 50 行，显示 exit code。
- **打断**：Esc/Ctrl+C 中止 —— exit code `-1`，`command aborted by user`。
- **上下文 / 历史**：输出被注入*模型*下一回合的上下文
  （`user_shell.rs:399`），完整的 `!command` 文本则记进输入历史，供 ↑ 召回
  （`slash_commands.rs:214`）。
- **turn 中**：允许 —— 模型正在流式输出时，Tab 会把一个 `!command` 排队，turn
  结束后才派发它（作为一项辅助任务）。

## 映射到这套代码

我们没有真模型，所以「把输出注入模型的上下文」无从谈起 —— 一个 `!command` 是
**纯本地的**。其余一切都干净地映射到既有的 turn + tool 机制上，所以一次 shell
运行复用流式 strip、`⎿` 窥视/展开 cell、resize 重绘和队列 —— 几乎没有新的渲染。
它在两处可见的渲染上与一次 AI turn **不同**，两处都符合用户的规格：

- spinner 的**状态行被隐藏** —— 没有 `Running… (Ns · esc to interrupt)` 那一
  行。它的 elapsed 改为骑在预览上，作为 `⎿ Running… (Ns)`（shell turn 的
  `strip_has_status` 为 false；见*运行中的预览*）。
- 一次 Esc 打断**不**提交 `Conversation interrupted` notice —— 落定的
  `⎿ Interrupted by user` cell 就是全部记录（见*打断*）。

### 被吸收的前缀 —— `App::shell_mode`（codex 的 `is_bash_mode`）

在空 composer 里把 `!` 作为第一个字符输入，它会被**吸收**：那枚感叹号活在
`App::shell_mode` 这个 flag 里，从不进入 textarea，而是作为 composer 的 prompt
渲染回来 —— 框里读作 `! pwd`，不是 `❯ !pwd`（`SHELL_BULLET`，红色）。
`sync_shell_mode`（codex 的 `sync_bash_mode_from_text`）在每次 edit/recall
之后运行，所以任何拿到前导 `!` 的文本 —— 敲进去的、召回出来的，或者从 Ctrl+R
搜索里接受的 —— 都以同样的方式被吸收。在**空的** shell composer 上按 Backspace
或 Esc 会删掉那枚被吸收的感叹号（退出该模式；Esc 那条分支坐落在 palette-dismiss
的优先级里，在 interrupt/quit 之前）。模式开着时，palette 从不打开（`/` 在这里
是个路径字符），`?` 是打出一个问号而不是切换快捷带。一次 Ctrl+R 搜索会*挂起*
该模式（`HistorySearch::snapshot_shell` —— 预览显示原始条目），取消时用快照恢复
它。`shell_query`（镜像 `command_query`）是前导 `!` 判定唯一待着的地方。

### 提交路由 —— `Action::RunShell(String)`

在 `on_key_conversation` 的回车分支里，处于 shell 模式时：

- 空草稿或只有空白 → `Action::Notice(SHELL_EMPTY_NOTICE)`，留在模式里（codex
  的空感叹号帮助）。
- turn 中 → **作为一个独立的 `QueuedTurn::Shell` 条目排队**（`queue_shell`），
  轮到它时在本地运行 —— codex 那个带 action 标签的 `RunShell` 派发
  （`submit_queued_shell_prompt`）—— 退出模式并记录完整的 `!command` 供 ↑
  召回。**绝不合并**进一个文本批次；见 `docs/input/queue.md`。
- 空闲 → `Action::RunShell(draft.trim())`，把完整的 `!command` 记进
  `input_history`（codex 记录整段文本；召回时会把感叹号重新吸收）。

### 跑起来 —— exec cell（`App::begin_shell` 加边界 runner）

被提交的结果是一个 Claude-Code 风格的 **exec cell** —— `! command` 深色横幅，
然后是 `⎿` 块里的**全部**输出（第一行在拐角下面，其余对齐在它下方），
**从不折叠**：用户跑这条命令就是为了读它的输出，所以每一显示行都内联出现，空行
也算在内，没有任何东西躲在 `… +N lines (ctrl+o to expand)` 提示后面 —— 那个折叠
是后端 `bash` cell 独有的（`docs/ui/tool-streaming.md`），那里的读者是模型，窥视
只是客气。比终端宽的一行**按词换行、保留空格**（`wrap_output`，与 Ctrl+O 视图所
用的同一个 wrapper —— 像 `sudo` 报错这样的散文在词处断开，而放得下的 `ls -l` 列
保持逐字节不变），而不是在边缘裁掉，所以没有任何输出文字消失；而一行若是 JSON
文档，会像 `bash` cell 那样为显示重新塑形（`exec_display_lines`）。这些行正是
Ctrl+O 视图所绘的那些（`result_full_block`，`tool_full_body` 行管线的内联
孪生），所以两个视图逐行一致，读一条 `!` 命令的输出**从不需要** Ctrl+O ——
它仍然在，作为整段对话之上的滚动分页器：

```
! ls                           ← Role::Shell header: dark user-style line
  ⎿  index.html                ← first output line, under the ⎿ corner
     script.js                 ← continuation lines aligned beneath it
     styles.css

! tree .
  ⎿  .
     ├── index.html
     ├── script.js
     ├── styles.css            ← every line, however many — no fold, no hint
     └── vendor
         └── lib.js
```

约束是**内存里的上限**（`SHELL_OUTPUT_MAX_BYTES`，见下），不是行预算：一条
`! seq 1 50000` 把保留下来的头部 —— 成千上万行 —— 提交进终端自己的 scrollback，
终端的滚动就在那里读它，并在上限切断它的地方以一个 `…` 标记收尾。

`begin_shell(command)`（纯函数）把 turn 设置好，好让既有的路径恰好产出这些：

- 事先记录 **`Role::Shell` 横幅消息**（这样运行中的一次 resize 能重绘它）——
  `message_lines` 把它渲染得像一条用户消息（深色全宽带），带红色 `! ` bullet；
- `streaming = Some(String::new())` —— 一个**空**缓冲：`is_streaming()` 为真
  （strip 显示，运行中按回车会排队），但 `finish_stream` 不记录任何幽灵消息；
- 一个带 `shell` 标志的 `TurnStatus`：这个标志让 `end_turn` 跳过 `Ran for Ns`
  摘要（**cell 就是它自己的记录**），*并且*让 `strip_has_status` 为 false，所以
  spinner 的状态**行不渲染** —— 状态对象仍然存活（它带着边界供给的 `elapsed`，
  并让 turn 机制继续跑），但 `SHELL_VERB`/`↑ tokens`/`esc to interrupt`
  从不显示；
  （没有 `● name(args)` —— 上面那条 Shell 消息就是横幅）。运行期间 strip 的预览
  行是 `  ⎿ Running… (Ns)` —— `render_live` 的 `shell_running_line(elapsed)`，
  也就是那个被隐藏的状态本会携带的 elapsed —— 紧贴在被提交的横幅下面；到
  `ToolEnd` 时，被提交的 `⎿` 块（每一行，对齐，不折叠）取代它（`result_row`
  负责拐角与续行的对齐）。`conversation_lines` 跳过 Shell 消息后面的空分隔行，
  好让重绘时 cell 保持紧贴。

**Ctrl+O 视图**渲染同一个 shell cell 时也**无横幅**（`tool_full_lines` 对 shell
tool 跳过 `tool_header`），所以 overlay 里看到的是 `! command` 深色横幅（那条
`Role::Shell` 消息）和作为不设上限的 `⎿` 块的**全部**输出 —— 从不是后端 tool
得到的那个 `● command` bullet。

`tui::turn::Session::run_shell`（I/O 边界，像 `start_turn` 一样）：
`begin_shell`，提交横幅行时**不带**尾随空行，然后在一个后台线程上 spawn
`spawn_shell_command` —— 它像 `DummyAi` 那样复用 `StreamEvent` 通道和
`CancelToken`，跑 `sh -c {command}`，piped 的 stdout/stderr 在 reader 线程上排空
（不会因管道缓冲而死锁），然后发 `ToolEnd { output, ok }`，再发
`StreamDone`。子进程来自
`subprocess::spawn_detached_shell`，用的是注册表的 detach helper，所以它**脱离
控制终端**运行（通过 `setsid` 二进制拥有自己的 session，否则退到 `setsid()`
helper 重新 exec —— `crate::subprocess`，也就是模型的 `bash` tool 所用的同一个
spawn；完整设计见 `docs/tools/tty-detach.md`）：一条会在 `/dev/tty` 上发问的
命令（`! sudo …`）直接在 cell 里报错 ——
`sudo: a terminal is required to read the password` —— 而不是把提示打在 TUI 上，
并在事件循环所拥有的键盘上阻塞（smoke Phase 44；交互式提示在这里从来就没法
用 —— raw 模式会让提示和 crossterm 抢每一次按键 —— 所以带着真实错误快速失败严格
更好）。**Esc** 走既有的 `Action::Interrupt`：取消杀掉子进程（runner 立刻返回，
*不* join 它的 reader 线程 —— 像 `sleep` 这样的被重新托付的孙进程可以在 `sh`
死后很久还握着管道），而 `interrupt_turn` 把 cell 落定为
`⎿ Interrupted by user`，**不**提交 `Conversation interrupted` notice（req 2 ——
那个 cell 就是记录；对 shell turn，`interrupt_turn` 返回
`Kept { notice: None }`，所以 `commit_turn_failure` 只写那个 tool）。输出是先
stdout 再 stderr 的拼接；非零退出会追加 `[exit status: N]` 并把 cell
落定为红色。

### 输出太大 —— 在内存里封顶（峰值内存的修复）

像 `! tree ~/` 这样的命令可以吐出几十 MB。把那一整坨读进一个 `String`（也就是
`read_to_string` 所做的事）会让**峰值** RSS 按输出大小暴涨：一个约 10MB 的
`tree ~/` 把进程从约 5MB 带到约 12MB（通过 `/proc/self/status` 的 `VmHWM` 测得，
线性增长 —— 30MB 输出约 31MB 峰值）。这**不是**泄漏 —— RSS 会回到基线，重复运行
也不累积 —— 但它是一个真实的、可以避免的峰值（足够大的命令可以把一台受限的机器
OOM 掉）。更早那版「把完整输出存到文件」的办法没有帮助：它仍然先把一切读进内存，
*然后*才写文件。

所以 runner **在读取的同时给它保留的内容封顶**（`tui::shell::append_capped`，
codex 的 `read_capped`/`append_capped` 模式）。每个管道 reader 最多保留
`SHELL_OUTPUT_MAX_BYTES`（100KB），并**把剩下的排空** —— 所以子进程永远不会因
管道写满而阻塞（那正是要开两个 reader 线程的原因）—— 同时跟踪是否丢掉了东西。
合并后的 stdout+stderr 再按同一上限封一次顶（在字符边界上切）。峰值 RSS 现在与
输出大小无关，是平的（一个 `VmHWM` 复现：7/10/30MB 的输出都约 2.5MB，之前是
9/12/31MB）。代价是一条被截断命令*保留*的输出最多 100KB（对比旧的 2KB 预览）
—— 但它只活在 `App.history` 里，而峰值才是要紧的。

runner 发 `ToolEnd { output: head, ok, truncated }`（丢掉了字节时 `truncated` 为
true；普通的后端 tool 总是发 `false`）。循环在 `end_tool` 之前调用
`App::set_tool_truncated()`，于是记下来的 `ToolCall.truncated` 让这个 cell ——
内联那个和展开的（Ctrl+O）那个一样 —— 在最后一行保留内容后面追加一个暗淡的
`…` 标记：

```
! tree ~/
  ⎿  /home/me/
     ├── Codes
     ├── Downloads
     ├── Documents
     …                                   ← (thousands of retained rows)
     └── zzz/last-retained-line
     …                                   ← TOOL_TRUNCATED_MARKER: the cap cut here
```

`ui::tool_full_lines` 和 `ui::tool_lines` 在 `tool.truncated` 时都会追加
`ui::TOOL_TRUNCATED_MARKER`（`…`）：内联 cell 显示全部保留的输出，而没有任何提示
说后面还有，所以这个标记是唯一告诉读者上限在此切断的东西。被丢掉的字节是
**不可恢复的**：与一个可分页的文件不同，这里没有什么可以展开；那个 `…` 只说明
「上限切在这里」。

保留的字节按终端本会显示的样子**折叠**过（`pty::fold`，
`docs/tools/interactive-shell.md`）：一个用 `\r` 重画的进度条是它最终状态的
一行，颜色转义消失，所以这个 cell —— 以及会重放它的模型上下文 —— 读到的是
文本，而不是画出它的每一帧；制表符和行尾空格留着。非 UTF-8 字节读作 `U+FFFD`
（折叠之前 `String::from_utf8_lossy` 给出的就是它），所以非 UTF-8 输出从不报错
（`read_to_string` 会），而一个切在多字节字符中间的上限只产出一个 `U+FFFD`。

### 页脚 —— `Shell mode` 那一行（`ui/footer.rs`）

和 Ctrl+R 搜索行一样，shell 模式提示占用的是**页脚槽位**：`app.shell_mode` 开着
时 `footer_rows` 返回 1（即使没有 session 信息也一样），而 `render_live` 在那里
画 `  Shell mode`（红色 `shell_mode_color()`、`FOOTER_INDENT`），而不是
`{model} · {cwd}` 那一行。光标留在输入框里（不像搜索，搜索拥有页脚的光标）。
shell 模式下快捷带不可能开着（palette 被抑制，而快捷带需要一个非 shell 的空
composer），所以这个槽位总是空的。

`?` 快捷带增加了 `! for shell command` 一条。

## 测试

- `app`：第一个字符就敲 `!` 会被吸收进模式（在文本中间它只是个字符；一次制造出
  前导 `!` 的编辑也会被吸收）；在空的 shell composer 上按 Backspace/Esc 退出模式
  （Esc 绝不会从它那里退出程序）；palette 和 `?` 带在该模式下被抑制；Ctrl+C 记录
  重新加前缀的草稿；空闲回车 → `RunShell(trimmed)` 并记录 `!cmd`（召回会重新进入
  模式；一次普通召回清掉它；一次 Ctrl+R 搜索挂起并恢复它，接受一条 `!entry` 则
  重新进入模式）；一个空感叹号 → 帮助 `Notice`，留在模式里；**turn 中回车会排队
  一个独立的 `QueuedTurn::Shell` 条目**（本地运行，绝不合并 —— 两条 `!` 命令是
  两个条目，之后一次文本回车开启一个新批次，在它上面 Alt+Up 重新进入 shell
  模式；见 `docs/input/queue.md`）；`begin_shell` 记录 `Role::Shell` 横幅、
  给状态和 tool 打上标志，然后 `end_turn` 返回无摘要；打断把命令落定为失败并返回
  `Kept { notice: None }` —— 没有 `Conversation interrupted` 消息（req 2）。
- `ui`：shell turn 的 `strip_has_status` 为 false，所以 `render_live` 隐藏
  状态行，改把 `⎿ Running… (Ns)`（那个 elapsed）作为预览显示，strip 只有
  预览加间隙（没有 `esc to interrupt`，req 3）；`message_lines(Role::Shell…)`
  是带红色 `! ` bullet 的深色用户风格行，按宽度补齐；shell tool 渲染成
  无横幅的（`tool_lines`）—— 内联时是**每一**行的 `⎿` 块（续行对齐在拐角下面），
  从不折叠，也从不暗示 Ctrl+O，运行期间是 `⎿ Running…`（`tool_lines`；实时
  预览加上 elapsed）；Ctrl+O 的 `tool_full_lines` 同样无横幅（没有 `● ls` bullet），
  在 `⎿` 下面显示不设上限的
  保留输出，按词换行并保留空白（`wrap_output` —— `ls -l`/`tree` 的对齐得以幸存，
  散文在词处断开；`wrap_text` 留给 assistant 的 markdown —— `! command` 横幅本身
  和用户的气泡也用 `wrap_output` 换行，见 `docs/input/textarea.md`）；一次被截断
  的输出（`tool.truncated` 置位）在最后一行保留内容之后追加一行暗淡的 `…`
  （`TOOL_TRUNCATED_MARKER`），内联与展开视图一样，而完整无缺的输出什么也不
  追加；
  `conversation_lines` 和 `transcript_lines` 让 cell 保持紧贴（Shell 横幅后面
  没有分隔行）；shell 模式把 composer prompt 换成红色的 `! `；该模式下没有
  session 信息时 `footer_rows` 为 1；页脚槽位显示红色的 `Shell mode`，顶掉
  `{model} · {cwd}`；光标留在框里；快捷带列出 `!`；对空参数的 backend tool，
  `tool_header` 仍然省略 `()`。
- `app`：`set_tool_truncated` 给运行中的 tool 打标记，且该标记活过 `end_tool`；
  没有 tool 在跑时它是空操作。
- `src/tui/`（smoke，Phase 19）：敲 `!echo …` 会显示 `! echo …` 这个 prompt 和
  `Shell mode` 页脚（绝不是 `❯ !echo`）；这次运行提交 exec cell（`! echo …` 横幅
  加 `⎿` 输出，没有 `●` 横幅，没有 `Ran for` 摘要）；一条失败的命令报
  `[exit status: N]`；一条运行中的 `!sleep 9` 显示 `⎿ Running… (Ns)` 预览，且
  **没有** `esc to interrupt` 状态（req 3），Esc 把它落定为
  `⎿ Interrupted by user`，且**没有** `Conversation interrupted` notice
  （req 2）。
  **Phase 22**：一条输出超过 100KB 的 `!` 命令（`seq 1 50000`）把它保留的整个
  头部内联提交 —— 屏幕上是一片数字行，没有 `+N lines (ctrl+o to expand)` 提示
  —— 以 `…` 截断标记收尾，**不**写任何 `/tmp/zeroth-shell-*.txt` 文件（输出在内
  存里封顶，从不落盘），而 Ctrl+O 视图以同一个 `…` 结束。

## 已知限制（v1）

- ~~**turn 中的 `!command` 不运行**~~ —— **已解决**（与 codex 对齐）：turn 正在
  流式输出时提交的 `!command` 现在会作为一个独立的 `QueuedTurn::Shell` 条目
  排队，轮到它时在本地运行（绝不合并进文本批次）。见
  `docs/input/queue.md`。空闲派发走的是同一条 `run_shell` 路径。
- **超过 `SHELL_OUTPUT_MAX_BYTES`（100KB）的输出会被丢弃**，不保存 —— 只保留头部
  （带一个 `…` 标记）。这个上限约束着峰值内存；代价是尾部不可恢复。需要更多就调
  高那个 const（或者加上头尾双段保留，codex 的 `HeadTailBuffer`）。
- **命令在设计上就是非交互的**：没有 stdin，也没有控制终端
  （`crate::subprocess`），所以任何会发问的东西（`sudo`、`ssh`）都带着它自己的
  「no terminal」错误快速失败，而不是挂住。密码流程需要非交互形式（`sudo -n`、
  一个 askpass helper、ssh 密钥）。
- stdout 和 stderr 是**拼接**的，不是真正交错的。
- ~~打断 notice 用的是共享的 `Conversation interrupted…` 文本。~~ ——
  **已解决**：shell 打断现在不提交任何 notice，落定的
  `⎿ Interrupted by user` cell 就是记录（req 2；见 `docs/input/interrupt.md`）。
- 没有超时 —— 一条挂住的命令会一直跑到 Esc（codex 封顶 1 小时）。
