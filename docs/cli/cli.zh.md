# CLI — `--help`、`[PROMPT]` 快捷方式、`--continue`、`--resume [id]` 与退出提示

Claude Code 的 session CLI，移植到 `/resume` 的 rollout 文件之上
（`docs/state/resume.md`）：`zeroth --continue` 重新打开当前目录记录的最新
对话，`zeroth --resume {id}` 可以在任何地方重新打开某个指定的 session，
而单写一个 `zeroth --resume` 会把 `/resume` 选择器作为第一个屏幕打开 ——
并且在退出一个记录过内容的 session 时，会打印出把它带回来的那条可复制粘贴的
命令。在此之上，**一个带引号的消息就是一个 `[PROMPT]`** ——
`zeroth "fix the failing test"` 直接启动进入这个 turn，而
`zeroth --resume {id} "and now the docs"` 把它送进重新打开的对话 —— 并且
`--help` 是应用自身配色下的一个有标题、分节的一页。（同一个 pre-TUI 边界还
应答 `mcp` 子命令族 —— `zeroth mcp add/add-json/remove/get/list`，
`docs/cli/mcp-cli.md` —— 以及 `zeroth update`，它通过一行安装脚本把最新
release 安装到这个二进制上，`docs/ops/update.md`；每一个都由其第一个参数在下面
的 flag 语法生效之前路由。）**`-p` 在无屏幕的情况下运行同一个 session** ——
一个 prompt 进去，答复在 stdout 上，退出状态出来 —— 在页面上有它自己的小节；
运行本身见 `docs/cli/headless.md`，本页只涵盖它的语法。

```
$ zeroth --help
Zeroth

Starts an interactive session by default; with -p, answers once and exits.

Usage: zeroth [OPTIONS] [PROMPT]
       zeroth -p [OPTIONS] [PROMPT]
       zeroth mcp <COMMAND>
       zeroth update

Commands:
  mcp                            Manage MCP servers in the user config file —
                                 see zeroth mcp --help
  update                         Install the newest release over this binary,
                                 if one is out (the checksum-verified installer)

Arguments:
  [PROMPT]                       First message of a new conversation, or the
                                 next of the one --continue/--resume reopens

Options:
  -c, --continue                 Continue the most recent conversation recorded
                                 in this directory
  -r, --resume [ID]              Resume a conversation by the id the exit hint
                                 prints — or pick it from a list, given no ID
  -C, --cwd <DIR>                Run in DIR instead of the current directory
      --permission-mode <MODE>   manual, edit, auto or master, for this run only
      --fullscreen               Run the terminal UI on the alternate screen
                                 (experimental; ZEROTH_TUI=full does the same)
  -h, --help                     Print help
  -V, --version                  Print version

Print mode:
  -p, --print                    Answer PROMPT (or stdin) with no screen: the
                                 answer on stdout, problems on stderr, then exit
      --output-format <FORMAT>   text (the answer), json (one result object) or
                                 stream-json (one event per line)
      --max-turns <N>            Stop after N model replies
      --verbose                  Log each step of the run to stderr
```

```
────────────────────────────────────────────────────────────────────────────
❯
────────────────────────────────────────────────────────────────────────────
  dummy_model_name · ~/repo

Resume this session with:
zeroth --resume 18a9f2c33d41e5b6-1a2b
```

## Claude Code 的做法（调研结论）

- `claude --continue` / `-c` —— 非交互：加载**当前目录下**最近的对话并接着
  进行。若无可续之处，它会报错退出（"No conversation found to continue"）而不是
  从头开始，这样脚本里的 `-c` 就不会悄悄丢掉线索。
- `claude --resume [id]` / `-r` —— 给定 id（Claude 用的是 UUID）则恢复那个
  session；**裸写**则打开交互式 session 选择器。未知 id 是错误，而不是新开
  session。
- `claude "prompt"` —— 一个可选的位置参数，即那个 prompt：session 启动，这条
  消息就是它的第一个 turn。`claude -c "prompt"` 和 `claude -r {id} "prompt"`
  把它送进重新打开的对话。第二个位置参数是 `error: too many arguments`，绝不
  拼接；而 `--resume` 会贪婪地把下一个 token 当作自己的 id，所以
  `claude --resume "prompt"` 会把那个 "id" 拿去查找然后失败。
- `claude --help` —— commander 的页面：一行 `Usage:`、一行描述，然后是
  `Arguments:` / `Options:` / `Commands:` 几节，描述对齐到同一列。
- 两个 session flag 都只关乎启动；session 内的行为不受影响。

## 我们构建了什么

同样的这些表面，按本代码库的规模裁量：一个纯 `cli` 模块（参数语法、help
*页面* 以及提示/用法字符串），两个纯 `session` 辅助函数（文件名 → id 的映射
和当前目录内最新的挑选），以及 `src/tui/startup.rs` 里的一层薄边界，它在终端
被触碰之前把 flag 解析成 rollout *路径* —— 并把 prompt 解析成第一个 turn。

### Session id

id 就是记录器本就生成的那个（`docs/state/resume.md`）：
`rollout-YYYY-MM-DDThh-mm-ss-{id}.jsonl` 里的 `{id}` 段，也记录在文件的
`session_meta` 行上 —— 自 epoch 起的纳秒数 + 十六进制的 pid。磁盘格式没有任何
变化；id 只是变得可寻址了：

- `session::rollout_file_id(file_name)` 从 rollout 文件名中把它提取回来（对
  外来名字返回 `None` —— 会检查时间戳的形状，所以一个杂散的
  `rollout-notes.jsonl` 不会产生垃圾 id）。
- `--resume {arg}` 按顺序匹配：一个存在的 rollout **路径**（比较方便 ——
  sessions 目录里就是普通文件）、一个精确的**文件名**、一个精确的 **id**，
  然后是一个**唯一的 id 前缀**（两处匹配即前缀有歧义 —— 报错且不列出任何东西，
  绝不猜测）。扫描复用 `/resume` 列表的新到旧日期目录遍历，无需读取文件头。

### `[PROMPT]` 快捷方式

`zeroth "…"` 就是没有 composer 的 Enter：该消息被记录进 ↑ 输入历史（它
*确实*被提交过），在横幅下方作为 `❯` 气泡提交，turn 便从它开始 —— 边界处的
`Session::submit_startup_prompt`（`src/tui/turn.rs`，与每一个其他开启 turn 的
方式并列）先运行 `App::input_history.record`，然后跑 `Submit` 键分支所调用的
同一个 `start_turn`，因此 CLI 路径绝不会偏离键入的路径。它在 session 指令被
应用、第一帧被排定**之后**运行，所以带 `--continue`/`--resume {id}` 时，加载
进来的 transcript 位于新气泡之上，而 turn 追加到被采纳的 rollout 文件里；在
全新启动时，横幅、气泡和 streaming 条一起落在第一个同步帧里，正如快速按一下
Enter 会绘制出的样子。

语法（`cli::parse`）：

- **一个位置参数，可以出现在 flag 之间的任何位置。** `zeroth "p" -c` 与
  `zeroth -c "p"` 是同一次调用。第二个位置参数是一个点名它的用法错误 ——
  `unexpected argument: bug (quote the prompt so the shell passes it as one argument)`
  —— 而不是拼接：否则 `zeroth write a -c program` 会把 `-c` 读成
  `--continue`，而一个悄悄少了一个词的 prompt 比一个必须加引号的 prompt 更糟。
  Claude Code 在这里也报错。
- **`--` 结束 flag**；其后必须恰好是那一个 prompt，因此以短横线开头的消息也
  可达（`zeroth -- "-flags first, then prose"`）。
- **空白 prompt 是用法错误**（`the prompt is empty`）：空字符串正是变量未设置
  时 shell 递过来的东西，而 Enter 同样拒绝空草稿。
- **prompt 是一条消息，逐字对待。** 开头的 `!` 或 `/` 作为文本送给模型，而
  不是当作 shell 命令或斜杠命令执行 —— composer 的 `!`/`/` 语法是按键语法，
  而脚本有 `zeroth mcp …` 来满足它本来可能想用 `/mcp` 的那件事。
- **`--resume` 仍然把下一个非短横线参数当作自己的 id**，所以
  `zeroth --resume "fix the bug"` 会把 `fix the bug` 当作 id 解析，并以
  `No session found for id: fix the bug` 失败（Claude Code 的行为；查找的提示
  会点名选择器）。要把 prompt 送进一个重新打开的 session，就点名那个 session：
  `--resume {id} "prompt"`，或者对这里最新的那个用 `--continue "prompt"`。
- **与裸选择器同给的 prompt 会被拒绝** —— `zeroth "p" --resume` 得到的是
  `--resume needs a session id to send a prompt into (or use --continue)`。选择器
  是交互式的且可被关闭，一个停在它后面的 prompt 要么在按 Esc 时开启一段新对话，
  要么不得不被丢弃：一个 flag 带来两种意外结局，所以语法一开始就说不。
- **`--fullscreen`** 选用实验性的、自己拥有一块屏幕的 TUI
  （`docs/ui/full-screen.md`）：对话被画在备用屏上，而不是提交进 scrollback。
  `ZEROTH_TUI=full` 从环境里做同一件事，而与 `-p` 同用时这个 flag 被拒绝 ——
  一次 print 运行没有屏幕可供它拥有。

### `--help` 页面

一个渲染器，两个页面：`cli::help(HelpPage::Main, style)` 和
`cli::help(HelpPage::Mcp, style)` 各自构建一个 `HelpDoc` —— 一个标题、一段描述、
`Usage:` 各行，以及各节的行 —— 并按 clap 与 cargo 的方式渲染，这样任何熟悉
Rust CLI 的读者都知道该往哪里看：

- **标题是 `Zeroth`** —— `crate::APP_NAME`，应用名字唯一所在之处 ——
  在第一行，穿着 *heading* 样式：页面点出产品名，而它下面的 `zeroth` 点出
  二进制名。`mcp` 页面以其命令 `zeroth mcp` 为题，样式相同。
- **描述**是它下面那行，描述的是*调用方式*，而不是产品：`Starts an interactive
  session by default; with -p, answers once and exits.` 两个参考实现都同样写这
  一行 —— codex 的 *"If no subcommand is specified, options will be forwarded to
  the interactive CLI"*、Claude Code 的 *"starts an interactive session by
  default, use -p/--print for non-interactive output"* —— 因为打开 help 页面是
  为了弄清怎么运行某个东西，而不是为了被推销它；应用*是什么*属于 README。
  只有一行，因为它是最先被读到的东西，而那里放一段话就等于一段话被跳过；并且
  像其他一切一样在 80 列以内（`main_help_fits_eighty_columns` 钉住它）。
- **`Usage:`** 是内联的，正如 clap 的打印方式 —— `Usage: zeroth [OPTIONS]
  [PROMPT]` —— 之后每一个形式各占一行，缩进对齐到第一个（七个空格）。命令词
  （`zeroth`、`mcp`）是*字面量*；方括号和尖括号里的部分是*占位符*。
- **各节** —— `Commands:`、`Arguments:`、`Options:` —— 每节是一个标题，下面是
  `  {literal}{placeholder}   {description}` 的行：两列缩进、该节最宽的字面量
  单元格、再三个列，然后是描述；多行描述在同列处续写。渲染时不换行 —— 每段
  描述都在源码里预先折好，所以页面在磁盘上读起来与打印出来完全一致，测试也能
  钉住它。
- **样式。** `HelpStyle::Plain` 完全不发转义序列 —— 测试所比对的字符串，也是
  管道所收到的内容。`HelpStyle::Ansi` 把标题和各节标题打扮成**粗体青色**
  （`ESC[1;36m` —— 应用在终端自身调色板里的强调色，横幅的青色和选择器的高亮
  色，因此这一页属于打印它的那个应用），字面量用**粗体**，占位符不着样式；
  填充按纯文本来度量，所以两种样式下各列都对齐。用法错误的 `error:` 是粗体
  红色（clap 的样式）。两种渲染仅靠转义序列不同 ——
  `ansi_help_strips_back_to_the_plain_page` 钉住这一点。
- **策略属于边界。** `cli::colour_enabled(no_color, term)` 是那条纯规则 ——
  除非 `NO_COLOR` 被设为非空（https://no-color.org）或 `TERM` 为 `dumb`，
  否则着色 —— 而 `startup.rs` 把它与正在写入的流上的 `std::io::IsTerminal`
  相组合：`--help` 按 stdout 决定样式，用法错误按 stderr。管道、日志文件和 CI
  任务无需询问即得到纯文本；终端得到颜色。

**用法错误**是 clap 的形状，而不是过去整页倾倒：

```
error: unrecognized argument: --frob

Usage: zeroth [OPTIONS] [PROMPT]
       zeroth -p [OPTIONS] [PROMPT]
       zeroth mcp <COMMAND>
       zeroth update

For more information, try '--help'.
```

`cli::usage_error(page, message, style)` 渲染它 —— `error:` 开头、该页面自己的
`Usage:` 块、指向 `--help` 的提示 —— 而 `mcp` 页面的语法错误在其六行 usage 上
取同样的形状。退出码 `2`，输出到 stderr，与之前一致。

### 纯 `cli` 模块（src/cli.rs）

- `Cli` —— 解析后的调用：`Session(SessionArgs)`（一次 session 运行 —— 如何
  开始，外加可选的 prompt）、`Help`、`Version`、`Mcp(McpCli)`、`Update`
  （`update` 子命令 —— `tui::update_cli::run` 干活，`docs/ops/update.md`）。
  `SessionArgs { start, prompt, cwd, permission_mode, print }`，其中
  `SessionStart::{Fresh, Continue, Resume(Option<String>)}`，`cwd` 是
  `-C`/`--cwd` 的目录，`permission_mode` 是 `--permission-mode` 的种子值，
  `print: Option<PrintOptions { output_format, max_turns }>` —— headless 运行时
  为 `Some`（`docs/cli/headless.md`），TUI 下为 `None`。
- `parse(args)` —— 作用于 `std::env::args().skip(1)`：第一个参数为 `mcp` 或
  `update` 时路由到该子命令（`update` 不接受任何东西 —— 尾随参数是用法错误，
  绝不猜测）；否则是 `--continue`/`-c`、`--resume [id]`/`-r [id]`（也支持
  `--resume=id`）、`--help`/`-h`、`--version`/`-V`、一个位置参数 `[PROMPT]`、
  位于其前的 `--`。其他任何情况 —— 未知 flag、第二个位置参数、
  `--continue --resume` 同给、prompt 与裸选择器同给 —— 都是 `Err(message)`；
  边界把 `usage_error` 打印到 stderr 并退出 `2`，而*解析*失败（无可续之处、
  未知 id）退出 `1`。`--resume` 的值可以是后一个参数，也可以带 `=` 附着；下一
  个参数以 `-` 开头意味着"裸 `--resume`，然后是那个 flag"（所以
  `--resume --help` 不会把 `--help` 当作 id 吃掉）。
- headless 的 flag（`docs/cli/headless.md`）：`-p`/`--print` 是一个开关，因此
  prompt 仍是那个唯一的位置参数（`zeroth -p "…"`，或者当 stdin 携带它时
  一个也不给）；`--output-format text|json|stream-json`、`--max-turns <N>`
  （大于 0 的计数）和 `--verbose` 需要 `-p`，没有它会被拒绝；
  `--permission-mode manual|edit|auto|master` 和 `-C`/`--cwd <DIR>` 在两种运行
  中都能用。每个取值 flag 都像 `--resume` 那样读取自己的值（分开给或带 `=`
  附着，绝不把以 `-` 开头的下一个参数当值），未知值会被拒绝并列出可选项，
  任何 flag 给两次都会被拒绝，而 `-p` 与裸 `--resume` 同给会被拒绝 —— 选择器
  是一个屏幕，因此 headless 运行必须点名它的 session。在边界处，`-C` 是在任何
  东西读取工作目录之前的一次 `chdir`，这也是为什么 `--continue` 随后会在*那里*
  寻找最新的对话。
- `help(page, style)` / `usage_error(page, message, style)` —— 上面那两个页面，
  任一样式；`colour_enabled(no_color, term)` 是颜色规则。
- `resume_hint(bin, id)` —— 与上面所示完全一致的退出提示（两行，无缩进）。
  `bin_name(arg0)` 从用户实际调用它的方式推导出打印的程序名（改名或软链接的
  安装会回显被运行的那个名字），否则回退到 `zeroth`。

### 边界（src/tui/startup.rs、main.rs）

- **参数解析在 `main()` 中运行，位于 detached-exec hook *之后*。** 该 hook 必须
  保持为第一条语句（不变量 1 —— 辅助进程重新 exec 时从不解析 TUI flag；它的
  argv 是 `__zeroth-detached-exec {cmd}`），而 CLI 的工作不触碰任何终端状态，
  所以 `--help`/错误打印到普通的 cooked 模式 stdio，并在 tokio runtime 启动
  之前退出。
- **一个无法启动的界面会说该改用什么。** 当 `InlineViewport::init` 失败时 ——
  没有终端可开（cron、CI、管道），或终端从不回应光标查询 —— `tui_main` 会把
  `cli::no_terminal_error` 打印到 stderr（先是原因，然后是 `{bin} -p "your
  prompt"`）并退出 1，而过去它是以 `io::Error` 的 Debug 形式结束。
  `init` 在返回错误之前已经回滚了它设置过的模式。
- **一次在开始之前就失败的启动只说明一次。** 工作目录、要重新打开的 session，
  以及 headless 运行的 prompt，都由同一个 `prepare` 步骤解析，其失败以 `Halt`
  返回 —— 一个用法错误（打印 usage 块，退出 2）或一个带消息的失败（退出 1）
  —— 然后由 `resolve_cli` 打印它：对 TUI 是文本，对 headless 运行则经由
  `headless::not_started`，这样 `--output-format json` 仍能把结果对象交给脚本
  （`docs/cli/headless.md`）。
- **flag 在 `InlineViewport::init` 之前解析成一个 `Startup`** ——
  `Startup { session: Option<StartupSession>, prompt: Option<String> }`，
  快速失败，不会闪一下 TUI：
  - `--continue`：`list_sessions(root, None)`（即 `/resume` 的扫描 —— 含资格
    判定，所以一个你从未键入过的 session 不会被续上），再用
    `session::latest_for_cwd` 对照 `cwd.display()` 过滤 —— 即记录器写入的、
    也是选择器 `Cwd` 过滤器比较的那个逐字字符串。无匹配 → stderr + 退出 1。
  - `--resume {id}`：上面那个查找器。无匹配 → stderr + 退出 1。
  - 选中的文件**在这里**被读取并解析；不可读/外来的文件是同样的快速失败错误。
    `StartupSession::Load` 把解析好的 `(path, text, meta, items)` 带入 `run()`，
    因此 TUI 内的路径不会再失败。
  - 裸 `--resume` → `StartupSession::Picker`。
  - prompt 原样随行；语法早已拒绝了唯一没有合理含义的组合（`Picker` + prompt）。
- **`run(term, startup)` → `Session::bootstrap` 在第一帧之前应用指令：**
  - `Load`：镜像选择器的 `ResumeSession` 分支 —— 把 cwd 恢复到该 session 的
    最后一个 checkpoint（先做备份快照，未知 commit 则无操作；
    `docs/state/checkpoint.md`）、`app.load_session(items)`、记录器 `adopt`
    （同一文件累积，含断裂尾部修复）、**从文件里最新的 `model` 记录恢复该
    session 的模型**（`docs/state/session-model.md` —— 目录 `config.json` 中的
    条目只为*新* session 播种；`ZEROTH_MODEL`/`ZEROTH_PROVIDER` 的钉定
    在这次运行中仍优先于那条记录）—— 然后在边界处把已加载的对话提交到
    scrollback：标题横幅、一个空行，以及通过正常的 `insert_before` 管线执行的
    `ui::repaint_lines(history, width, RESIZE_REFLOW_MAX_ROWS)`。启动时**不
    `Purge`**：全新启动绝不能擦掉用户的终端 scrollback（选择器在 session 中途
    的 purge 是为了丢掉*上一个对话的*行；启动时并没有），而 `insert_before` 正是
    标题横幅本来就落下的方式。真正移动了文件的恢复会弹出通常的 toast。
  - `Picker`：逐字照搬 `OpenResumePicker` 分支（扫描 → 打开 →
    `enter_overlay` → 绘制），位于循环之前。这条路径上标题横幅*不*预先提交
    —— 每个 overlay 返回都会经 `ui::banner_tail` 重新发出它，因此关闭时会重绘
    它（直接从选择器退出也走同一个返回）。
  - 然后是有的话就提交 prompt：即前述的 `submit_startup_prompt` ——
    `bootstrap` 做的最后一件事，在第一帧被排定之后。
- **退出提示：** `run()` 返回 `Option<String>` —— 记录器的活动 session id，
  仅当 `app.history` 非空时（codex 的延迟创建意味着空 session 没有文件、没有
  id，什么也不打印；一个 `/clear` 之后闲置的 session 同理）。`tui_main` 在
  `term.restore()` **之后**打印它，好让这两行在正常终端流中落在方框下方，经由
  `cli::resume_hint` 并带上被调用的 bin 名。出错结束的运行则显示它的错误
  （没有提示）。

## 与 Claude Code 的已知差异

- **id 是记录器的十六进制时间戳，不是 UUID** —— 同样的长度量级、同样的复制
  粘贴手感，无需新依赖；旧的 rollout 文件继续可用，因为没有任何东西会重新生成
  id。唯一前缀匹配是 Claude 不提供的便利；歧义是错误，所以它不会误触。
- **`--continue` 按 meta 行的逐字 cwd 字符串限定在当前目录**（选择器的 `Cwd`
  过滤规则，`docs/state/resume.md`）—— 一个经由软链接路径记录的 session 不会
  匹配它的规范路径孪生。
- **只要退出时有历史就打印提示**（Claude 会对某些退出加以限制）；它是恢复
  之后的普通 stdout，因此自然滚走，绝不进入 transcript、记录器或模型的上下文。
- **`[PROMPT]` 兼作 `-p` 的输入**，正如 Claude Code 的那样 —— 而 stdin 按
  codex 的方式与它合并，放进一个带标记的 `<stdin>` 块
  （`docs/cli/headless.md`）。`--fork-session` 和 `--session-id` 不在范围内。
- **help 页面有标题且有颜色。** commander 的页面以 `Usage:` 开头；我们的以产品
  名和描述开头，即 cargo 的顺序，并在 commander 是素色的地方使用应用的青色。

## 测试

- `cli`：`parse` —— 裸写/`-c`/`--continue`、带 id 与不带 id 的 `--resume`
  （分开给、带 `=` 附着、`-r`）、后面的 flag 不被当作 id 吃掉、
  help/version、未知 flag / `-c -r` 冲突作为错误；prompt —— 单独给、在 flag
  之前和之后、在 `--` 之后（以短横线开头的）、与 `-c` 和 `-r {id}` 同给；
  第二个位置参数作为携带引号提示的错误；空白 prompt 被拒；prompt 与裸选择器
  同给被拒；`--resume "x y"` 仍是一个 id。help —— 纯文本主页逐行钉住、
  每一行都在 80 列以内且无行尾空白、`Zeroth` 标题在最前、`mcp` 页面的
  标题和六行 usage；ANSI 渲染剥掉转义后与纯文本版相等、标题/各节标题携带
  heading 转义而 `zeroth` 携带粗体转义；`usage_error` 的三段式形状；
  `colour_enabled` 的三条规则；`resume_hint` 与打印形状一致；`bin_name` 对
  路径取 basename，并在 `None`/空时回退。
- `session`：`rollout_file_id` 与 `rollout_rel_path` 内嵌的 id 往返一致（含带
  短横线的 id），拒绝外来/无时间戳的名字；`latest_for_cwd` 只在 cwd 匹配的行中
  挑出最新的 `updated_secs`，否则 `None`。
- `tests/cli_help.rs`（真实二进制，`CARGO_BIN_EXE_zeroth`，离线）：管道化的
  `--help` 与 `help(Main, Plain)` 逐字节相同 —— tty 之外没有转义 ——
  而 `mcp --help` 等于 `Mcp` 页面；`--version` 打印 `zeroth {version}`；
  语法错误以 `error:` 形状打印到 stderr、stdout 为空，退出 2；两个位置参数会
  点名第二个并给出引号提示；prompt 与裸选择器同给被拒。
- `scripts/smoke.sh` Phase 61（I/O 边界）：退出一个记录过 turn 的 session 会
  打印 `Resume this session with:` + 一行 `--resume {id}`，其中的 id 点出那条
  rollout 文件；`--continue` 直接重启进入旧对话（无选择器）并追加到同一文件；
  `--resume {id}` 按 id 做同样的事；裸 `--resume` 启动进入选择器；退出一个
  *空* session 不打印提示；在不含会话的新 cwd 里 `--continue` 快速失败，退出 1
  且无 TUI。**Phase 108**：`zeroth "hello there"` 直接启动进入这个 turn ——
  `❯` 气泡、streaming 的回复和 `Done for`，无需按任何键 —— 并且它的退出会
  打印提示；`--resume {id} "again please"` 重新加载 transcript，并把 prompt
  作为同一 rollout 文件里的下一个 turn 运行；`-c "…"` 同理；窗格 tty 上的
  `--help` 以 `Zeroth` 开头、带有 `[PROMPT]`、并且是着色的（原始窗格中含
  粗体青色转义），而同一页面经管道则不含任何转义；用法错误打印 clap 形状的
  尾部信息并退出 2。
- `scripts/live_smoke.sh` Phase L4：对一个真实 provider，prompt 快捷方式的
  turn 会 streaming 并落定，而 `--resume {id} "…"` 会继续它。
