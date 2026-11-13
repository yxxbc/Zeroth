# /resume —— 接续一个存下的 session（rollout 文件 + session picker）

openai/codex 的 `/resume` 的移植：每段对话在发生时就被记录进一个 JSONL 的
「rollout」文件，而 `/resume` 斜杠命令打开一个全屏 picker 列出那些存下的
session —— `❯ {age}  {first user message}` 行，输入即搜索 —— 它的 Enter 把选中的
那段对话载回 app（内联重绘，和一次 resize 重绘它完全一样），并把之后的回合追加到
*同一个*文件。Codex 源码：`codex-rs/rollout` crate（`recorder.rs`、`list.rs`、
`policy.rs` —— 文件格式、懒创建 recorder、日期目录游走 + 头扫描列表）、
`tui/src/resume_picker.rs`（备用屏幕 picker）、`tui/src/session_resume.rs` 和
`tui/src/app/session_lifecycle.rs`（换掉活动 session 的流程），以及
`tui/src/slash_command.rs` 里的 `SlashCommand::Resume` 条目。

## codex 怎么做（发现）

记录（`rollout` crate）：

- session 住在 `~/.codex/sessions/YYYY/MM/DD/` 下（session 开始的本地日期），
  每个 session 一个文件：`rollout-YYYY-MM-DDThh-mm-ss-{uuid}.jsonl`
  （`recorder.rs::
  precompute_log_file_info`；时间里的 `:` 换成 `-`，好让文件名
  对文件系统安全）。
- 每一行都是 `{"timestamp": <UTC write time>, "type": <kind>, "payload": {…}}`。
  一个文件的**第一**行永远是 `type: "session_meta"`（`payload`：线程 `id`、
  session 开始 `timestamp`、`cwd`、`originator`、`cli_version`、模型/provider 字
  段……）；其余是对话的条目，按文件顺序 —— 身份住在 meta 行 + 文件名里，不在逐行
  id 里。
- **只有完成的条目落盘，绝不落流式增量**（`policy.rs`：用户/agent 消息、
  tool call + 输出落盘；`*Delta`、begin 事件、纯 UI 噪音丢掉）。
- 文件创建是**延迟**的：路径先算好，但直到第一个可记录条目之前什么都不碰磁盘，所
  以空 session 不留文件（
  `recorder_tests.rs::recorder_materializes_on_flush_with_pending_items`）。恢复
  时以追加模式打开已有文件 —— 一个文件跨多次恢复累积整个线程。
- 读回时跳过解析不了的行（前向兼容），并对一个没有任何记录的文件报错。

列表（`list.rs`）：

- 从新到旧游走 `sessions/`（年/月/日目录按数字降序，文件名按内嵌时间戳
  降序），有一个硬扫描上限。picker 里的默认排序是 **Updated** —— 文件 mtime，
  最新在前。
- 每个文件做一次**头扫描**（最多 10 行，找用户消息时扩到约 210 行），取出 meta
  行和 **第一条用户消息**，后者兼作 picker 预览。一个 session 只有**两者都找到**
  才符合条件 —— 用户从没打过字的 session 不列出。

picker（`resume_picker.rs`）—— 一个备用屏幕 overlay：

- 版式：一个标题（`Resume a previous session`）、一个搜索行（`Type to search`
  占位符）、session 行，以及一个页脚 —— 一条 `─` 分隔线，上面右对齐的
  `{selected+1} / {total}` 进度标签，下面几行暗色按键提示。密集行各占一行：
  `❯ {relative-age padded to 12}{preview}` —— 标记在选中行上，年龄暗色，预览以
  `...` 截断。
- 年龄是单单位的相对时间 —— `now`、`{N}s ago`、`{N}m ago`、`{N}h ago`、
  `{N}d ago`（`format_relative_time`）—— 对着一个**在 picker 打开时冻结**的参考
  时间算，所以列表不会自己跳。
- 键：↑/↓ 移动（两端环绕），PageUp/PageDown 按一个视口跳（夹取），Home/End 跳到
  两端，Enter 恢复选中的，而**任何普通可打印字符都是搜索输入**（大小写不敏感的子
  串过滤；选择重置到第一条匹配），Backspace 弹掉查询。**Esc 先清查询**；只有在空
  查询上按 Esc 才取消。从一个正在跑的 session 里，取消（以及 Ctrl+C）只是关掉
  picker —— 活着的对话不受影响。
- 列表区域的空状态：`No sessions yet` / `No results for your search`。

流程（`slash_dispatch.rs`、`session_lifecycle.rs`）：

- `/resume` 是 `SlashCommand::Resume`，描述是「resume a saved chat」。在一个真实
  agent 回合正在跑时它被**拒绝**（一个错误 cell；
  `slash_command_blocked_by_active_task`）—— picker 永远不必与一个流式回合赛跑。
- 选中时后端加载 rollout，**只有成功**时 UI 才切换到恢复的对话；失败以一个错误
  cell 呈现，当前 session 毫发无损地继续。恢复的线程继续追加到它已有的文件；被放
  弃的那个 session 的文件之后仍然可恢复。

## 我们建的东西

同一个功能，按这个代码库的尺寸裁剪：一个纯的 `session` 模块（文件格式 + 列表/解
析逻辑 + picker 的纯状态）、`main.rs` 里 I/O 边界上的一个小 recorder，以及第三个
用 Ctrl+O overlay 机制渲染的 `View`。

### session 文件（src/session.rs，纯）

- 文件：`{root}/YYYY/MM/DD/rollout-YYYY-MM-DDThh-mm-ss-{id}.jsonl`，其中
  `{root}` 是配置目录下的 `sessions/` —— 默认 `~/.zeroth/sessions`，而在一个被移
  动过的 `ZEROTH_CONFIG_DIR` 里也跟着走，所以状态目录整体搬迁（它以前无论如何都
  退回 `$HOME`，smoke 套件那些一次性的 session 就是这样进到开发者自己的 picker
  里的）——而 `ZEROTH_SESSIONS_DIR` 直接覆盖它（`ZEROTH_STARTUP_DELAY_MS`
  那套 ——smoke 套件把它指向一个临时目录）。`{id}` 是自纪元起的纳秒 + pid 的十六
  进制 ——不靠 uuid 依赖也足够唯一，而且从不解析回来（我们按*路径*恢复）。路径推
  导是纯的 `session::rollout_rel_path(date, time, id)`；时钟/pid 留在边界上。id
  在文件存在 **之前**就选好了 —— 第一段对话用的是这个 session 的启动 id（临时树
  的那个，`docs/tools/scratchpad.md`），而 `/clear` 铸造下一个 —— 因为一个 hook
  载荷从它的第一个事件起就点名这段对话，早于那次延迟创建（`docs/tools/hooks.md`
  ）。
- 行是 codex 的形状：`{"timestamp": <UTC millis Z>, "type": …, "payload": …}`。
  第 1 行是 `session_meta`（`{id, timestamp, cwd, model, originator:
  "zeroth", version}`）；然后每个完成的 [`HistoryItem`] 在落进
  `App::history` 时占一行：
  - `"message"` → `{role: "user"|"assistant"|"system"|"error"|"shell", text,
    timestamp}`（条目本来就带的显示时间戳），
  - `"tool"` → `{name, args, ok: bool, output, timestamp, shell, truncated}`（正
    在跑的工具从不在 `history` 里，所以只存在完成的状态），外加这次调用怎么跑的
    —— `exit_code`、`duration_ms`，以及失败时的 `error_kind` —— 各自没有值时省略
    （`docs/state/rollout-records.md`），
  - `"summary"` → `{verb, secs, timestamp}`（`verb` 在加载时映射回它的静态字符串
    —— 一个 `STATUS_VERBS` 的过去式，或者更旧构建记录过的一个动词（`Finished`、
    `Wrapped up`、……）—— 退回 `"Done"`：`TurnSummary.verb` 是 `&'static str`），
  - `"reasoning"` → `{text, timestamp, secs, tokens}` —— 一个已沉降的思考阶段，
    所以恢复的 session 保住它的 `Thought for …` cell *和*它们 Ctrl+O 展开所展示
    的思维链（`docs/ui/thinking-stream.md`）；那些计数是 `serde(default)` 的，所
    以一条没有它们的记录以 0 加载。流式增量、状态行和 token 计数从不记录 ——
    codex 的持久化策略。（一个*已沉降*的思考阶段是普通历史条目，确实会得到它的行
    ；原始增量不会。）三条**旁路**记录搭同一个文件走，但不是历史条目：
    `"checkpoint"`（`docs/state/checkpoint.md`）、`"model"` —— session 自己的模
    型选择（`docs/state/session-model.md`：`llm::settings::ModelSelection`，也就
    是 `config.json` 条目的形状，紧跟在 meta 行之后写一次，之后每次 `/model` 选
    择、Ctrl+T 循环和探测回答时再写；最新的胜出，`session::parse_model`），一次
    恢复正是在它上面跑完剩下的对话 —— `session_meta` 行的 `model` 只点名文件创建
    时所用的模型，从不被读回 —— 以及 `"usage"`，每个模型请求一条，带 provider 自
    己的计数，写在请求结束的地方（`docs/state/rollout-records.md`：
    `session::usage_line`，由 `session::history_lines` 摆到条目之间，由
    `session::parse_usage` 读回，好让一次重写把每一条留在它原来所在的位置）。
- 序列化是 **模块本地的记录类型**（`SessionMeta`、一个带标签的行 enum）上的
  `serde`/`serde_json`，与应用类型互相映射，于是磁盘格式与 `app/` 解耦，而应用类
  型保持无 serde。解析跳过畸形/未知的行（前向兼容）；`parse_session` 产出 meta +
  items，而对一个没有有效 meta 行的文件产出 `None`。
- 列表辅助函数：`preview_of`（第一条非空 `user` —— 或 `shell`，见差异 —— 消息的
  文本，空白压平，shell 带 `! ` 前缀）和 `relative_age(secs)`（codex 的 `now`/
  `Ns ago`/`Nm ago`/`Nh ago`/`Nd ago` 分档）。
  `SessionSummary { path, updated_secs, created_secs, cwd, preview }` 是 picker
  持有的东西：两个排序键都是扫描时冻结的「几秒之前」值（显示的年龄是活动键那个值
  的 `relative_age`），外加 meta 行记录的 cwd，供 `Cwd` 过滤用。

### 记录（main.rs，I/O 边界）

一个 `SessionRecorder` 拥有根目录、活动文件路径 + meta，以及一个 `recorded` 水位
（scrollback 的 `committed` 模式，用在磁盘上）：

- `sync(&app.history)` 每次事件循环迭代跑一次（循环退出后再跑一次）：
  `history.len() > recorded` 追加新条目 —— 在第一次追加时懒创建日期目录 + 文件并
  先写 meta 行，所以空 session 从不碰磁盘（codex 的延迟创建）—— 而
  `history.len() < recorded` **重写**文件（meta + 每个条目）：唯一的截断路径是
  Esc-Esc 回溯倒回和 `/clear`，而 `/clear` 先把 recorder 重置到一个全新的（延迟
  的）session 文件，也就是 codex 的 `/new`。历史只有追加或截断两种，所以水位比较
  是可靠的。chunk 从不碰 `history`，所以流式不花 I/O。
- 记录失败被忽略（`let _ =`）—— TUI 绝不能因为磁盘满而死掉；codex 同样记日志然后
  继续。
- 恢复时，recorder **收养**加载的文件（路径、它解析出的 meta、
  `recorded = items.len()`、它最新的 `model` 记录），并从那里往后追加 —— 同文件
  累积，codex 的 `Resume{path}` 模式。这个 session 的选择骑在 recorder 上、紧挨
  着 meta（`set_model`，一个 `selection_written` 水位 —— checkpoint 那套）：选择
  变了就在下一次 `sync` 时作为一条新的 `model` 行冲刷出去，文件的第一次写入把它
  紧跟在 meta 之后带上，而一次重写会重新发出当前那条（
  `docs/state/session-model.md`）。
- picker 的扫描也住在这里：降序遍历 `{root}` 的年/月/日目录收集候选（受
  `RESUME_WALK_CAP` 约束），在封顶那个昂贵部分**之前**按 mtime 降序排（codex 的
  Updated 排序）—— 只有最新的 `RESUME_SCAN_CAP` 个文件会被读头部（
  `RESUME_HEAD_LINES` 行，在一个宽松的 `RESUME_HEAD_BYTES` 上限之内，所以一条很
  长的粘贴首消息仍然能给出它的预览），保留既有 meta 行又有预览的文件（
  `session::parse_session` + `session::preview_of`），排除 recorder 的活动文件，
  并把 `Vec<SessionSummary>`（年龄在这里算，在打开时冻结）交给纯核心。

### 状态 + 键（app/resume.rs）

- `View::ResumePicker` —— 第三个视图。`App` 上的
  `ResumePicker { sessions, selected, query, cwd, filter, sort, focus }` 状态（
  打开时是 `Some`）；行按需推导（`ResumePicker::matches()` —— 先 `Cwd`/`All` 过
  滤，再在预览上做大小写不敏感的子串匹配（codex 客户端的 `Row::matches_query`）
  ，再按活动排序键的顺序），像 palette 的 `matching_commands` 一样。
  **Filter/Sort 工具条**是 codex 的：`ResumeFilter { Cwd (default), All }` 把每
  个 session 记录的 cwd 与 picker 自己的比对（逐字 —— 两边都是 recorder 的
  `Path::display` 格式化结果），`ResumeSort { Updated (default), Created }` 挑键
  ，而 `ResumeControl { Filter (initial), Sort }` 是 Tab 轮转、←/→ 作用的焦点；
  任何切换都会重新落座选择（codex 也这样重启它的列表）。
- `COMMANDS` 多了 `resume`（「Resume a saved chat」）→ `CommandEffect::Resume`。
  在一个回合活动时跑它返回 `Action::Toast(RESUME_BUSY_NOTICE)` —— 框上方一个瞬时
  toast（codex 在任务中途拒绝；它会换掉整段对话，与流赛跑），*不是*一条已提交消
  息（2026-07-08 更新；曾经是 `Action::ErrorNotice`，见 `docs/ui/toast.md`）。空
  闲时它返回 `Action::OpenResumePicker` —— *循环*扫描磁盘并调用
  `App::open_resume_picker(sessions)`（设好视图，重置回溯的预备状态，像
  `toggle_tool_view` 一样）。
- `on_key_resume_picker`：↑/↓ 环绕移动（`wrap_step`）；PageUp/PageDown 按
  `RESUME_PAGE`（10 —— 工具视图那一页）；Home/End；**Tab/BackTab 换工具条焦点，←
  /→ 切换被聚焦控件的值**（Filter Cwd↔All、Sort Updated↔Created —— 重新落座选择
  ）；普通字符追加到 `query` 并重置 `selected`；Backspace 弹掉；Esc 先清非空的查
  询，否则关闭（`Action::CloseResumePicker`）；Enter 为选中的过滤后行返回
  `Action::ResumeSession(path)`（空列表上是 `None`）；Ctrl+C 也关闭（codex 从
  session 里打开的那个 picker），覆盖全局退出；Ctrl+O 被吞掉（两个视图共用备用屏
  幕）。一次 bracketed paste 经由 `paste_into_resume_search` 并进查询（codex 的
  `normalize_pasted_search_query` —— 连续的空白压成单个空格，往非空追加前加一个
  分隔空格，纯空白的粘贴忽略）。
- `App::load_session(items)` —— `/clear` 重置的形状（把队列排进丢弃的图片、丢掉
  流式/tool/状态 —— 防御性的；一个回合不可能在活动）—— 但把 `items` 装成新的
  `history` 并回到 `View::Conversation`。`input_history`、composer 草稿和附件都
  存活，像 `/clear` 一样。

### 循环（main.rs）

- `Action::OpenResumePicker` → 扫描，`app.open_resume_picker(sessions)`，
  `term.enter_overlay()`，立刻绘制（Ctrl+O 那套不闪黑屏的模式）。
- `Action::CloseResumePicker` → `term.exit_overlay()` + 普通的 overlay 返回
  （`overlay_return_repaint` —— 冲刷排队的，重绘 live 区域）。
- `Action::ResumeSession(path)` → 在边界读取 + `session::parse_session`。成功：
  `app.load_session(items)`，recorder 收养文件，**session 的模型被恢复**自文件最
  新的 `model` 记录（`Session::restore_session_model` → `ModelSession::restore`
  ，一次 `/model` 切换减去 `config.json` 写入 —— 页脚、Ctrl+T 的种子和量表都跟上
  ；一个环境变量钉住保留钉住的模型，一个这台机器没有 key 的 provider 在当前模型
  上保留，配一个红色 `Can't resume on …` toast；`docs/state/session-model.md`）
  ，然后 `exit_overlay` + 一次 **purge** 的 `repaint_conversation`（像 `/clear`
  ）—— 加载的 session *替换*整段对话，所以重建用它的完整历史填满 scrollback；一
  次普通返回会把之前的聊天留在它上面，而恢复的 session 早先的回合一个都不会提交
  。失败：以普通返回关掉 picker，然后
  `commit_error_notice("Failed to load session: …")` —— 当前对话毫发无损地继续（
  codex）。
- `Action::Toast(text)` → `present_toast`（一个瞬时的信息 toast；它替换掉的
  `Action::ErrorNotice` 那个红色提交的 `Action::Notice` 孪生体于 2026-07-08 移除
  —— 见 `docs/ui/toast.md`）。一次失败的 session *加载*仍然提交一条红色的
  `commit_error_notice`（那是一个值得留下的真错误，不是一个临时性的拒绝）。
- Quit 分支的 overlay 展开从 `view == ToolOutput` 泛化成 `view != Conversation`
  ；绘制分支多一个
  `View::ResumePicker => term.draw_overlay(ui::render_resume_picker)` 支。
  scrollback 提交本来就以 `view == Conversation` 设闸，所以 picker 竖起时没有任
  何东西能写备用屏幕（不变量 4 成立 —— 反正也没有回合在跑）。

### 渲染（ui/resume_view.rs）

备用屏幕上的 `render_resume_picker`，外壳与 Ctrl+O transcript 分页器一致，样式在
`RESUME_*` 常量里：

```
/ R E S U M E / R E S U M E / R E S U M E …          ← slash-tiled title row
                                                      ← blank
  Type to search     Filter: [Cwd] All   Sort: [Updated] Created
                     └ the toolbar, right-aligned on the search line ┘
                                                      ← blank
  ❯ 5m ago       fix the wrap bug in ui.rs            ← rows: marker+age+preview
    2h ago       ! cargo test                           (selected row on a
    3d ago       hello there                            full-width bg tint)
  …                                                   ← `No sessions yet` /
                                                        `No results for your search`
─────────────────────────────────────── 1/3 ─        ← separator + right count
  ↑/↓ select · enter resume · esc cancel · tab + ←/→ filter/sort
```

搜索行左边是暗色的 `Type to search` 占位符（或 `Search: {query}`）；它的右边缘带
codex 的 **Filter/Sort 工具条**（`resume_toolbar_spans`）：暗色的 `Filter:`/
`Sort:` 标签，活动值加方括号（`[Cwd]` —— 它的控件持有 Tab 焦点时是
`resume_focus_color()` 的洋红，否则朴素），非活动值暗色。当完整的 Tab 对放不下搜
索文本旁边时，工具条压缩成活动值（`Filter:[Cwd]   Sort:[Updated]`，codex 的紧凑
形式），而在最窄的屏幕上它干脆不显示。

行是 codex 的密集密度：`❯ ` 标记（未选中时是空格）、**活动排序键**的年龄补到
`RESUME_AGE_WIDTH`（12 —— codex 每行只显示那个时间戳）、预览省略号截断到宽度（
`cols()` 算术）。选中行在调色板的选中色上亮起，**底色是占满整行的
`resume_selected_bg()`**（按列补到边缘 —— 用户消息块那套，codex 的背景混色）；其
余暗色。行窗口让选择**居中**（`ui::centered_window`，`/model` 列表的规则 —— 由
`selected` 和正文高度推导，不存滚动偏移）：在一个比屏幕高的列表上，高亮骑在中间
那一行，于是它上面*和*下面的 session 都在视野里，每次 ↑/↓ 都把下一个滚进来，只在
那一侧的列表到头时才滑到一个边缘（最前几行锚在顶部，最后几行贴着尾部）。以前用来
开窗的是贴边的 `menu_window`（那时的 palette 窗口，以及居中之前的 `@` 和 `$`
picker 的），它把高亮钉在它跨过的那个边缘 —— 从最后一行按 ↓ 只滚进一个新的
session、把它之后的全藏起来，而 ↑ 让高亮沿着已经在屏幕上的行往回走，根本不滚。

## 与 codex 的已知差异

- **`!` shell session 也列出**：codex 的资格要求是一条*用户消息*；我们的
  `Role::Shell` 头在什么意义上都是用户输入，所以它算数（预览成 `! {command}`）。
  一个只跑过 shell 命令的 session 仍然值得恢复。
- **活动 session 从列表里排除**，而不是 codex 在选中时的「Already viewing」守卫
  —— codex 的 picker 是基于线程、带 id/名字的；我们的是基于文件的，而你正在写的
  那个文件，在一列「要*回去*」的东西里是噪音。
- **没有分页、密度切换或 transcript 预览**：扫描有上限，打开时整批加载（一个本地
  目录，不是一个 app server），永远是密集行。输入即搜索和 Filter/Sort 工具条从加
  载的东西上重新推导。codex 的 Ctrl+T 全文预览在恢复之后已经以 Ctrl+O 的形式存在
  于这里。（工具条本身 —— `Filter: [Cwd] All`、`Sort: [Updated] Created`、Tab 焦
  点 + ←/→ —— **是**移植了的；codex 另外把密度选择持久化进 config，我们没有那个
  。）
- **`Cwd` 过滤逐字比较记录的 cwd 字符串**（两边都是 recorder 的 `Path::display`
  格式化结果）—— codex 在比较之前归一化路径；一个通过符号链接路径记录的 session
  ，在这里对不上它规范的孪生体。
- **`/resume <id>` 的内联参数不在范围内** —— 但 CLI 的孪生体现在有了（
  `docs/cli/cli.md`，2026-07-31；在这之前二进制不收参数）：`--continue` 重开这个
  cwd 里记录的最新 session，`--resume {id}` 按 id 重开（文件名的 `{id}` 段，现在
  可经 `session::rollout_file_id` 定位），裸 `--resume` 直接启动进这个 picker，
  而退出一个记录过任何东西的 session 会打印 `--resume {id}` 提示。
- **没有 cwd prompt**：codex 提供 chdir 到该 session 记录的 cwd。我们的把 `cwd`
  记在 meta 里（前向兼容）但就地恢复 —— dummy 后端没有任何依赖 cwd 的行为需要保
  护。
- **条目里的时间戳是显示字符串**（`hh:mm AM/PM`，可能是空的）—— 它们逐字
  往返；只有行/meta 的时间戳是机器可读的 UTC。codex 处处记 RFC3339。
- **一次截断重写会归一化文件。** 恢复之后的一次回溯倒回会从*解析出来的*历史重写
  文件，于是未来某个构建写、这个构建跳过的那些行（前向兼容的读法）会被那次重写丢
  掉。codex 的文件是只追加的（回滚作为事件记录）；我们的截断即重写让文件保持为
  `App::history` 的一面镜子。（一个恢复的文件，如果最后一行丢了换行 —— 一次撕裂
  的写 —— 会在第一次追加时被修好，而不是被粘上去。）
- **并发实例**：两个 zeroth 进程从不共用文件（id 里嵌了 pid），但从两个实例
  恢复 *同一个*存下的 session 会让追加无保护地交错 —— codex 有 state-db 仲裁；这
  里是记录在案的局限。

## 测试

- `session`：meta/条目行经 `parse_session` 往返（多行 + 引号 + unicode 文本，每
  种角色，ok/failed/truncated 工具，summary 动词恢复成它 `STATUS_VERBS` 的过去式
  或旧的静态值 —— 未知动词退回 `Done`）；畸形与未知行被跳过；一个没有 meta 行的
  文件（或空文件）解析成 `None`；`preview_of` 找到第一条非空 user/shell 消息（压
  平空白，shell 带 `! `，纯空白消息跳过），没有时返回 `None`；`relative_age` 的
  各档（`now`、`59s ago`、`1m`、`59m`、`1h`、`23h`、`1d`）；`rollout_rel_path`
  补零并把 `:` 换成 `-`。
- `app`：`/resume` 的 palette 条目空闲时跑成 `OpenResumePicker`、回合中途跑成
  `Toast(RESUME_BUSY_NOTICE)`（2026-07-08；曾经是 `ErrorNotice`）；
  `open_resume_picker` 以 codex 的默认值（filter `Cwd`、sort `Updated`、focus
  `Filter`）进入视图（并重置一个已预备的回溯）；picker 的键 —— ↑/↓ 环绕，
  PageUp/PageDown/Home/End，输入即过滤会收窄并重新落座选择，Backspace 弹掉，一次
  粘贴压平后并进查询，Esc 先清后关，Ctrl+C 关闭，Ctrl+O 被吞掉，Enter 以选中的（
  过滤后）路径产出 `ResumeSession`、空列表上产出 `None`；`Cwd` 过滤藏起其他目录
  ，直到 → 切到 `All`；Tab 移动工具条焦点，←/→ 切换被聚焦的控件（`Created` 按开
  始时间戳重排）；`load_session` 装入历史、回到会话视图，并把排队的/流式残留清进
  丢弃的图片。
- `ui`：`render_resume_picker` —— 标题行、搜索占位符 vs `Search:` 回显、右对齐的
  工具条（方括号跟着切换走，窄时是紧凑形式，更窄时不显示）、标记 + 暗色年龄 + 预
  览行，选中项亮在**占满整行的底色**上，年龄列跟着活动排序键，右对齐的
  `{n}/{total}` 计数，两个空状态，窄宽截断，选择在一个矮窗口里保持可见，以及在列
  表中部时保持**居中**、上下两边的行都在视野里（`/model` 的规则）。
- `scripts/smoke.sh` Phase 31（I/O 边界）：把 `ZEROTH_SESSIONS_DIR` 指向一个
  临时目录 —— 一个回合写下一个 rollout 文件（meta + user + assistant 行）；第二
  次启动的 `/resume` picker 列出它（预览可见），Enter 把旧对话内联重绘，一个后续
  回合 **追加到同一个文件**（没有第二个文件），而*那之后*的重启显示两个回合；
  `/clear` 随后开一个新文件。Phase 118 驱动恢复的模型那一侧 —— 一段对话回在它文
  件所记录的模型上，而不是目录的当前条目上（`docs/state/session-model.md`）。
