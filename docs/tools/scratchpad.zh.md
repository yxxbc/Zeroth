# session scratchpad —— 设计

日期：2026-08-24

## 目标

给 agent 一个**属于它自己的临时目录**，并且告诉它这件事。这是 Claude Code 的
规则，整条移植过来：每一个中间结果、用完即弃的脚本、随手记的工作笔记都放进一个
session 私有的目录，而不是 `/tmp` —— 在那里它不会和别的 session 撞上，不会被误认
为是用户的成果，写它也不需要一次权限提示。

同一个动作也整理了原本就在那里的东西：后台 shell 的临时 `{id}.output` 文件挪到
`tasks/` 叶子下面，于是一个 session 根目录里坐着两个有名字的孩子，而不是一堆平铺
的文件。

## 布局

```text
{tmp}/zeroth-{uid}/{session}/
├── scratchpad/            the agent's temp files
├── tasks/                 {id}.output — background shells' interim output
└── images/                the downscaled copy of every picture sent to the model
```

`images/` 这个叶子是后端自己的工作空间，从不告诉模型：一张附上的图片在之后每个
turn 都会随上下文重发，而为它构建的载荷 —— 缩到 **Auto-resize images** 的上限 ——
就保存在这里，好让那些 turn 读一个很小的文件，而不是重新解码原图
（`docs/ui/images.md` 的「Memory」）。它在 bootstrap 时与 scratchpad 一同创建
（`tui::config::prepare_image_cache`），并通过 `images::set_payload_cache_dir`
公布；与 scratchpad 不同，它不受 `ZEROTH_SCRATCHPAD` 控制，因为它既没有 prompt
块，也没有权限豁免。

例如 `/tmp/zeroth-1000/18cea7cc0aee22c0-5d77f/scratchpad`。

- `{tmp}` 是 `std::env::temp_dir()` —— `TMPDIR` 会挪动整棵树。
- `zeroth-{uid}` 是 Claude Code 的 `claude-{uid}` 模式：每个用户一个稳定的根，
  这样共享的 `/tmp` 永远不会把两个用户的 session 混在一起。
- `{session}` 是 `tui::host::session_id()`（纳秒 + pid 的十六进制），让并发的
  实例不去碰对方的文件。

两个叶子都是*被模型*读的 —— scratchpad 从它的 system prompt 里读，一个 task 的
`{id}.output` 从每一段后台启动文本里读 —— 所以布局在唯一性允许的范围内保持得尽量
短。session id 已经把项目分开了，这就是没有 dashed-cwd 段的原因。

形状是纯的 `scratchpad` 模块（`session_root` / `scratchpad_dir` /
`tasks_dir`）；边界在 `tui::bootstrap` 里注入临时目录、uid 和 session id
（`set_session_info` 模式）。

**一个 session id。** `bootstrap` 只铸造它一次并共享：临时树*和*第一个对话的
rollout 文件都由它命名（`docs/state/resume.md`）。以前它被铸造两次 —— 而由于这个
id 是从纳秒派生的，两个答案并不相同。现在一个 hook 的 `session_id` 是**对话的**
id（`--resume` 接受的那个句柄，`docs/tools/hooks.md`），一次 `/clear` 或
`/resume` 会让它往前走，而临时树始终属于这个进程，所以载荷里也带上
`scratchpad_dir` —— Claude Code 为此准备的那个字段 —— 而这就是一个 hook 找到本
session 的 scratchpad 和 task 输出的方式。

## prompt 块

`prompts/scratchpad.md`，用人设那种简洁的笔调写，里面只有一个由边界填的
`{scratchpad}` 占位符：

```
## Scratchpad

Dir {scratchpad}
Always put temp files here never /tmp: intermediate data, scratch scripts, working notes
Session private and outside the project so file writes here need no approval
Use /tmp only when asked
```

它是装配好的 system prompt 的**第三**块：

```
persona        prompts/zeroth.md    (who you are)
environment    prompts/environment.md   (where/when you are, as which user)
scratchpad     prompts/scratchpad.md    (where your scratch goes)   ← this doc
```

`augment_with_scratchpad` 在 `augment_with_environment` 之后组装 —— 同样的规则，
所以这两个读起来是一对：空白的基础 prompt 原样返回（「空的
`ZEROTH_SYSTEM_PROMPT` → 没有 system 消息」这条契约，
`docs/architecture/context.md`），而整套装配在 Ctrl+D 的上下文调试视图里可见。
Subagent 随 prompt 的其余部分一起继承它（`docs/agents/agent-tool.md`），所以一个
侧边的 agent 在同一个目录里打草稿。

session 没有 scratchpad 时这一块**被省略** —— 要么功能关了，要么目录没能创建。
把一个模型指向一个不存在的路径，同时又在那里拒绝它的写入，比什么都不说更糟。

## 往那里写不需要批准

这一块声称往 scratchpad 里的文件写入不需要批准，所以它们绝不能触发一次批准。
`PermissionGate::scratchpad_covers` 回答这个问题，而
`llm::approval::approve_call` 在**常驻允许清单旁边**咨询它 —— 早于
`PermissionRequest` hook、auto 模式的 classifier 和那个提示，因为它回答的正是
它们所回答的问题：*这次运行能不询问就跑吗？*

它刻意很窄：

- **只有 `write` 和 `edit`。** 它们的 `target` *就是*被改动的东西。一条点名
  scratchpad 路径的 `bash` 命令仍然会问 —— 它接下来会碰到什么，是它自己的事
  —— 一次 MCP 调用也一样。
- **严格在内，按字面**（`scratchpad::contains`）：两个路径都必须是绝对的（相对
  target 会按 cwd 解析，而 cwd 是用户的项目），任何位置出现 `..` 都直接拒绝，而
  不是解析掉，匹配是按组件做的，所以 `{root}-elsewhere` 不在 `{root}` 里面。
- **强制询问仍然会问。** 一个 `PreToolUse` hook 的 `permissionDecision: "ask"`
  意味着*由人决定*，它覆盖这里的判断，就像它覆盖允许清单一样
  （`docs/tools/hooks.md`）。

它也是**可见的**：这次调用落定为 `Approval::AllowNoted` 加
`SCRATCHPAD_ALLOWED_NOTE`，所以 cell 上会挂一行暗淡的
`⎿ Allowed in the session scratchpad` —— classifier 注记的兄弟
（`docs/state/permissions.md`），记在这次调用上，在 Ctrl+O 里重放，并被一次
`/resume` 往返保存。

这个豁免正是让 scratchpad 写入在 **manual** 模式下也能过的原因。在 edit 和 auto
（默认）下，每一次文件改动本来就已经跑在页脚所点名的模式下，所以一次 scratchpad
写入像任何其他写入一样在那里先被覆盖，也就不会挂注记。

## 它在对话里长什么样

草稿是 agent 在摸索路径，不是对用户项目的改动，所以在 **Verbose output** 关着
时（默认），一次进入 scratchpad 的 `write` 或 `edit` 不保留自己的 cell。它作为
自己的一条从句加入那个折叠起来的读/搜组，并显示文件路径，模型写的时候：

```
● Making 1 scratchpad edit +25… (ctrl+o to expand)
  ⎿  /tmp/zeroth-1000/18cea7cc0aee22c0-5d77f/scratchpad/guess_the_number.py
```

组关闭后则是一行暗淡的 —
`Made 2 scratchpad edits +26 -1 (ctrl+o to expand)` —— 这些编辑新增和删除的行数
暗淡地排在这个计数旁边（`docs/ui/tool-groups.md` 的*Scratchpad edits*）。Ctrl+O
仍然会把每次调用完整展开，包括它的权限注记；一次失败或从未运行的编辑保留它的红色
cell。

## 配置

- `ZEROTH_SCRATCHPAD` —— 假值（`0`/`false`/`no`/`off`）会关掉整个功能：没有
  目录，没有 prompt 块，没有豁免。
- `ZEROTH_SCRATCHPAD_DIR` —— 用这个确切的目录代替
  `{session_root}/scratchpad`（`ZEROTH_SKILLS_DIR` 的约定）。
- `TMPDIR` 挪动整个 session 树，tasks 目录也在内 —— `smoke.sh` Phase 91 正是靠
  它拿到一棵自己能毫无歧义地找到的树。

## 测试

- 纯（`src/scratchpad.rs`）：根/叶的形状，以及 `contains` 对它必须拒绝的那些逃逸
  —— 相对路径、`..`、同前缀的兄弟目录、根本身、`tasks` 这个兄弟；
  `is_session_scratchpad` 认得出任何 session 的布局，并拒绝目录本身、一个兄弟叶
  子、缺失的 session 段、另一个 per-user 根、相对路径和 `..`。
- 纯（`llm::backend`）：`render_scratchpad` 不留下任何占位符；
  `augment_with_scratchpad` 追加在 environment 块之后（顺序是 persona <
  environment < scratchpad），并且不去动一个空白的基础 prompt。
- 纯（`permission`）：这个闸门覆盖里面的 `write`/`edit`，别的什么也不覆盖 ——
  不是 `bash`，不是 MCP，不是外面，不是相对路径，也不是借 `..` 穿过去。
- 纯（`llm::approval`）：一次 scratchpad 写入返回 `AllowNoted`，不发起请求；一次
  外部的写入仍然发起；一次强制询问仍然会问。（豁免那个测试跑在一个**预先取消**
  的 token 上：豁免生效时它什么也不改变 —— 它在闸门被咨询之前就落定了 —— 但一个
  回归会快速失败，而不是让整套测试卡在一个没人会回答的提示上。）
- live（`tests/live_openrouter.rs`，`--ignored`）：一个真模型从它的 prompt 里把
  scratchpad 路径读回来，并点名称临时文件就去那里；以及一次真正写进 scratchpad
  的 `write` 落到盘上，不发起 `StreamEvent::Permission`，注记挂在它的 cell 上。
- 边界：`scripts/smoke.sh` 驱动真实的二进制 —— 目录在 session 根下存在，而一条
  放到后台的 `!` 命令在它旁边 tee 进 `{session}/tasks/{id}.output`。

## 已知限制

- **scratchpad 里的一个符号链接可以指向外面。** 包含性测试是字面的 —— 不咨询文件
  系统，所以经由这样一个链接的 `write` 会被放行。创建那个链接需要一次 `bash`
  调用，而它会发问。
- **没有任何东西清扫旧的 session 目录。** 每个 session 都把它的 scratchpad 和
  task 输出留在身后，交给操作系统自己的 `/tmp` 清理，一如这些临时文件一直都是
  这么处理的 —— 包括一个没用过的 scratchpad，因为目录是事先创建的，好让 prompt
  能点名一个确实存在的路径。
- **一个被 `/resume` 的 session 会拿到一个新的 scratchpad。** 这个 id 是按*一次
  启动*而不是按对话生成的，所以一段恢复的 transcript 里更早的 tool 记录点名的
  还是上一个 session 的目录，而 prompt 点名的是新的那个。文件仍然在 transcript
  所显示的那个路径上；没有任何东西重写它们。折叠视图仍然会折叠那些更早的编辑 ——
  它靠布局（`scratchpad::is_session_scratchpad`）认得出**任何** session 的
  scratchpad —— 而权限豁免只覆盖新目录：布局检查只用于显示。
- scratchpad **不是** `/settings` 里的一行：它是一件启动期的事实（prompt 在启动
  时装配一次），所以环境变量是它唯一的开关。
