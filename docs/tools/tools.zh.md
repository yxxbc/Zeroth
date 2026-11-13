# LLM tool calling —— `bash` 及其同伴 / `read` / `write` / `edit`

真实 backend 现在能**调用 tool**：模型请求跑一条 shell 命令、读一个文件、写一个
文件，或改一个文件；backend 在本地执行它，像 dummy 的脚本化 tool 一样把这次 call
流式送进 TUI，把结果回灌给模型，然后循环，直到模型用纯文本作答。这让本应用从一个
聊天客户端变成一个小型编码 agent —— 而这个能力正是 `docs/providers/llm.md` 的
“Known limitations” 列为未来工作的那一条（“No streaming tool-call support from
the model yet”）。

本文覆盖线上格式、agentic 循环、执行器和渲染。它与 `docs/providers/llm.md`（后端
接缝）和 `docs/input/shell-command.md`（`!` 本地 shell，与本主题无关 —— 那是
*用户* 在跑命令，这里是*模型*）互补。

## 为什么用 Chat Completions 的 function-calling（而不是 codex 的 Responses/apply_patch）

openai/codex 用的是 **Responses API** 加一个自由格式的 `apply_patch` **grammar**
tool。两者在这里都不合适：zeroth 讲的是 **Chat Completions** API
（`/chat/completions`），为的是广泛覆盖 OpenAI 兼容与 OpenRouter 的各家；而一个
Lark grammar 的自定义 tool 只在支持它的少数模型上可用。

所以我们用**标准的 Chat Completions function-calling** —— 每个 OpenAI 兼容
provider 都实现的那一种形状 —— 加上四个模仿 Claude Code 工具集的
**JSON-schema function tool**（名字用用户要求的那几个）。Codex 仍然影响着设计：
它的 tool *描述*、它的执行 *语义*（shell 默认 10 秒超时、按字节封顶且中间截断/头
部截断的输出、`Exit code:` 加输出的框法），以及它的 TUI *diff 单元格*
（`• Edited path (+N −M)`，绿色为增、红色为删）都被移植了过来。

## 核心 tool

八个 tool 都以
`{"type":"function","function":{"name","description","parameters":<schema>}}` 条
目的形式，连同 `tool_choice:"auto"`，声明在请求的 `tools` 数组里交给模型。纯定义
与 JSON schema 住在 [`llm::tools`](../../src/llm/tools.rs)（`tool_specs()`），并
有针对形状的单元测试。

| tool | params | 执行什么 |
| --- | --- | --- |
| `bash` | `command`（必填）、`wait`（可选，**秒** —— 默认 120，上限 600；`0` 表示放到后台启动）、`description`（可选） | 在一个**自己的伪终端**里跑 `bash -c command`（没装 `bash` 的地方用 `sh`）；命令退出（输出之上加 `Exit code: N`，以数据形式给出 —— `docs/tools/bash-tools.md`）、停下来等输入，或 `wait` 到期时返回 —— 仍在运行的命令**不会被停掉**：它报出自己的 session id 并继续跑。拿不到终端的地方，它在一条没有控制终端的管道上跑（`crate::subprocess`）。旧的 `timeout`/`timeout_ms`（毫秒）、`run_in_background` 与 `tty` 仍然能被解析 |
| `bashsend` | `session_id`（必填）、`input`（必填 —— 文本，换行即回车，`<Enter>`/`<C-c>`/`<Up>`… 是按键） | 向一条运行中的命令输入并取回它的回答，限固定的 10 秒（`docs/tools/bash-tools.md`） |
| `bashwait` | `session_id`（必填）、`wait`（可选，秒 —— 默认 120） | 等新输出、退出或一个提问；`wait: 0` 只做检查 |
| `bashkill` | `session_id`（必填） | 停掉一条命令以及它启动的一切 —— `SIGINT`、`SIGTERM`，然后 `SIGKILL` |
| `bashlist` | — | 正在运行的 session：id、命令、运行时长、每个是否在等输入 |
| `read` | `path`（必填 —— 绝对路径，和另外两个一样）、`offset`（可选，1 起的行号）、`limit`（可选，默认 2000 行） | 读文件：文本返回带行号的行（动态宽度的行号槽）；**图片**（png/jpg/jpeg/gif/webp）以视觉方式附上，好让模型看见它（`offset`/`limit` 被忽略 —— 见下文“图片读取”） |
| `write` | `path`（必填 —— schema 要求**绝对**路径）、`content`（必填） | 创建父目录、写文件；新文件在带编号的内容之上**显示** `Wrote {N} lines to {path}`，覆盖已有文件则显示相对旧内容的带编号 diff 片段 —— 头部的路径按 cwd 相对显示（`tools::display_path`，`../` 爬出 cwd 之外），`● Write({path})` 头部则按 TUI 自己的规则（下文“路径显示”）—— 而*模型*读到的是一行确认 |
| `edit` | `path`（必填 —— 绝对路径，同 `write`）、`old_string`（必填）、`new_string`（必填）、`replace_all`（可选） | 精确字符串替换；`old_string` 不存在，或没有 `replace_all` 而不唯一时报错；在带编号的 diff 片段之上**显示** `Updated {path} (+A -D)`，路径像 `write` 那样按 cwd 相对显示 —— 模型同样只读到一行确认 |

`read`/`write`/`edit` 是三个独立的 JSON tool，而不是一个 `apply_patch` grammar：
它们在任何 function-calling 模型上都能用，而 `edit` 那种精确的 `old_string`→
`new_string` 契约（Claude Code 的）对任意模型来说，比模糊上下文的补丁匹配更简单
也更安全。

其他 tool 按条件挂载，各自靠边界把所需状态交给 backend 来启用：`agent`
（`docs/agents/agent-tool.md`）、`askuserquestion`（`docs/tools/ask.md`）、四个
task tool（`docs/tools/task-tools.md`），以及 `skill`
（`docs/tools/skills.md`）—— 只有至少发现一个 `SKILL.md` 时才提供，因为一个都没
有时它除了 “unknown skill” 什么也答不了。

## 线上格式（Chat Completions）

### 请求

客户端带 tool 时，`OpenAiClient::build_payload` 会多出：

```json
"tools": [ {"type":"function","function":{"name":"bash", "...": "..."}}, ... ],
"tool_choice": "auto"
```

### 助手的 tool-call 回合

以 `choices[].delta.tool_calls[]` 流式回来，每个 delta 带一个 `index`（累积所依
据的键）、在第一个片段上带 `id` + `function.name`，以及若干 `function.arguments`
字符串片段，它们**拼接**成最终的 JSON 参数对象。`finish_reason:"tool_calls"`（或
者简单说：累积到了任何 tool call）意味着模型想跑它们。纯的
[`ToolCallAccumulator`](../../src/llm/openai.rs) 把这些带索引的 delta 折叠成
`Vec<ToolCallRequest { id, name, arguments }>` —— 针对碎片化的流做了单元测试。

### 把结果回灌

backend 每轮往它正在运行的请求列表里追加两条消息：

1. 它刚收到的**助手**消息，带着 `tool_calls`（以及任何文本），
2. 每次 call 一条 **`role:"tool"`** 消息：
   `{"role":"tool","tool_call_id":<id>, "content":<output>}`。

`ChatMessage` 多了两个可选的、`skip_serializing_if` 保护着的字段（`tool_calls`、
`tool_call_id`），所以普通的 system/user/assistant 消息仍与以前逐字节一致地序列
化。

## agentic 循环（`llm::agent::run_agent`）

本功能的心脏是一个**纯的、泛型的驱动** —— 像 `llm::retry` 的 `run_stream` —— 所
以整个循环可以用假件、不碰网络地单元测试：

```
run_agent(tx, cancel, max_iterations, round, execute, pending_notices):
  loop:
    if cancel: return                       # silent — the UI owns the interrupt notice
    for note in pending_notices():          # background completions since the last
      messages.push(user(note))             # round — a killed shell is known to the
                                            # model within the turn (docs/tools/background.md)
    match round(&messages):                 # one streaming request (with its own retry)
      Complete(blank) | CutOff(text):       # no answer, or one cut off at the output
        if unfinished == MAX_CONTINUATIONS: # cap: keep what it said, ask for the rest
          tx.send(Error(gave_up)); return   # in a new user message, two times in a
        unfinished += 1                     # row at most (docs/architecture/unfinished-replies.md)
        messages.push(assistant(text)) if text
        messages.push(user(continue_note)); continue
      Complete            => tx.send(StreamDone); return
      Cancelled           => return
      Failed(e)           => tx.send(Error(e)); return
      ToolCalls{assistant, calls}:
        if iters == max_iterations: tx.send(Error("tool-call limit reached")); return
        messages.push(assistant)
        for call in calls:
          tx.send(ToolStart{ name: display_name(call), args: summary(call) })
          out = execute(&call, cancel)       # runs the tool locally
          tx.send(ToolEnd{ out.output, out.ok, out.truncated })
          messages.push(tool_result(call.id, out.output))
          if cancel: return
        iters += 1
```

- `round` 是真正干活的流式器：它构建载荷（带 tool），流式输出一个回复 —— 像今天
  一样发出 `Chunk`/`Thinking*`，另外为每个流出的 `tool_calls` 片段发一个
  `ToolCallDelta(fragment)`，落在它所属那次 call 的槽位下，所以模型*生成*这次
  call 时状态行的 token 计数在跳（按推理的方式计数，
  `docs/ui/status-indicator.md`），而一次 `write` 会在内容到达时预览
  （`docs/ui/write-streaming.md`）—— 累积 tool call，并在内部应用
  **已有的按请求重试**（`llm::retry`），返回一个 `RoundOutcome`。重试没有变，仍
  然是按请求的。round 报告什么，是从流的 stop reason 读出来的
  （`round_outcome`）：在输出上限处停住的回复返回 `CutOff`，被上下文窗口或内容过
  滤器停住的返回 `Failed`，并指明原因
  （`docs/architecture/unfinished-replies.md`）。
- **什么都不回答的 round 不会结束回合。** 一个空回复 —— 没有文本、没有 tool
  call —— 或一个 `CutOff`，都会在一条新的 user 消息里用续写提示来回应，并被记成
  一条无单元格的注记，transcript 与之后每一份 context 都带着它，然后同一个回合再
  跑一个 round。任何进展都能把预算挣回来；连续第三个未完成的 round 会以一条说明
  此事的错误结束回合（`docs/architecture/unfinished-replies.md`）。
- `execute` 是真正的执行器（见下文）。测试里两者都是普通闭包，生产环境里是真正的
  流式器加真正的执行器。
- `max_tool_calls`（`MAX_TOOL_ITERATIONS`，20 —— `/settings` 的
  **Max tool calls** 行，`0` 表示不限）给失控的 tool 循环封顶。它数的是
  **call**，不是 round：一个 round 可以请求整批并行的 call
  （`docs/tools/parallel-tools.md`），所以按 round 计数会让一个 round 把上限超额
  花掉好几倍。预算只够一部分的那个 round 会被 **夹取** —— 装得下的 call 按模型自
  己的顺序跑，其余的以红色收尾并带上
  `Not run: this turn had already spent its tool-call limit.`（这样每个
  `tool_call` 仍然有一条匹配的 `tool` 结果），然后回合以一条错误结束，该错误点出
  这个上限并指向 `/settings` —— 这条错误模型也会读到，作为一条 `[error] …` 的
  user 条目，所以每次 call 自己的结果里不再重复这条建议。
- `pending_notices` 接的是后台注册表那块完成通知板（`LlmBackend` 把它接上；没有
  注册表时为空）—— 每条注记是一条 user 角色消息，追加在上一轮 tool 结果之后，位
  置正是已沉降历史条目为后续回合重放它的地方（`docs/tools/background.md`）。
- **取消**在 round 之间、每个 tool 之后、以及 tool 内部都会被检查 —— 所以 Esc 能
  及时收尾，与普通流的契约相同。

### 为什么 TUI 几乎不用改

事件循环**早就**把 `ToolStart`/`ToolEnd` 与 `Chunk` 交织在一起 —— dummy 从第一天
起就是这么脚本化的（`stream::turn_events`），而
`tui::stream::Session::on_stream_event` 也早就冲刷文本片段、显示 tool 在运行、以
绿/红提交它、并把它记进 `history`。一个驱动这些同样事件的真实模型几乎
**不需要新的事件循环代码** —— 唯一新增的是 `StreamEvent::ToolBatch` 分支，它提前
登记**一批并行**的 call（于是那些还没跑的 call 显示 `⎿ Waiting…`；见
`docs/tools/parallel-tools.md`），之后同样的逐 call `ToolStart`/`ToolEnd` 流程原
封不动地跑。中断、Ctrl+O transcript、resize 重绘，以及 `context_messages` 的重放
（已完成的 tool → 下一回合原生的 `assistant tool_calls` + `tool` 结果对，与本回
合内的协议对齐 —— 见 `docs/architecture/context.md`）全都照旧工作。

## 执行器（`llm::exec::RealToolExecutor`）

边界代码（文件与进程 I/O），由手工/冒烟验证，把 `llm::tools` 里的纯核心包起来：

- **`bash`** —— 在一个自己的伪终端里 `bash -c`
  （`background::BackgroundRegistry::launch_tty`、`docs/tools/bash-tools.md`），
  这次 call 在 session 上等，直到命令退出、停下来发问、或它的 `wait` 到期；拿不
  到终端的地方，则走 [`crate::subprocess::spawn_detached_shell`] 上的管道（与终
  端分离 —— 见下文），stdin 为 `/dev/null`，stdout+stderr 在读取线程上排空（话多
  的命令堵不死一条写满的管道）。无论哪种方式，已结束命令的输出都保留前 16 KiB 与
  后 48 KiB （`pty::fold::HeadTail`，切割处点出该 session 的日志），到期的
  `wait` 会把命令交给后台注册表而不是杀掉它，取消则会停掉它。输出按 codex 的做法
  为模型框好： `Exit code: N` 加输出；非零退出把单元格收成红色，而显示层无论退出
  码是多少都丢掉那行框 —— 退出码是给模型读的，永远不是给单元格的
  （`docs/ui/tool-streaming.md`）。
- **`read`** —— 读文件；文本文件应用 `offset`/`limit`，格式化成带编号的行
  （`format_read`，纯的），对结果按字节封顶；图片文件走下文的图片分支。
- **`write`** —— 创建父目录、写入，并**显示** `describe_change`：一个全新的文件
  是在 **带编号的内容**之上的 `Wrote {N} lines to <path>` 头部
  （`tools::write_report`—— `{n:>W} {text}` 行，编号与 `read` 的一致，好让模型能
  引用它们去 `edit`）；覆盖写则按 edit 的方式报告（`tools::update_report`）。头
  部的路径是紧凑的 cwd 相对显示形式（`tools::display_path` —— cwd 下就是
  `src/main.rs`，cwd 之外是一段 `../../README.md` 的爬升），而 `● Write({path})`
  头部逐字记录模型自己的（按 schema 是绝对的）参数，并按 TUI 的路径显示规则在渲
  染时*显示*它（下文“路径显示”）。 **模型**读到的是下面那行确认。
- **`edit`** —— 纯的 `apply_edit` 引擎做精确替换；执行器把它写回，并在
  **带编号的 diff 片段**之上报告 `Updated <path> (+A -D)`
  （`render_numbered_diff` —— 只画每段改动加上两侧各 `DIFF_CONTEXT_LINES`（3）行
  上下文，相接的段合并，相隔远的片段之间用一行 `⋮` 隔开；行形如
  `{n:>W} {sign}{text}` —— 上下文/新增行按*新*文件编号，删除行按*旧*文件编号，即
  codex 的 `diff_render` 编号法）。两个正文都以 `DIFF_MAX_LINES` 封顶，并留一条
  `… N more lines` 尾巴 —— 而且同样只在单元格上。

### 模型读到的结果只有一行

`write`/`edit` 走的是 ask 与 skill 工具已经在用的**双文本分离**
（`ToolOutcome::context` → `StreamEvent::ToolAnswered` →
`ToolCall::context_output`）：上面那段带编号的正文是*单元格*，而进入对话的是

```
File created successfully at: /tmp/name.txt (file state is current in your context — no need to read it back)
File overwritten successfully at: /tmp/name.txt (…)
The file /tmp/note.txt has been updated successfully. (…)

Replaced 3 occurrences.
```

（`tools::write_ack` / `tools::edit_ack`，基于模型自己的路径参数 —— 它送来的那个
绝对路径，而不是单元格头部那种 cwd 相对显示形式。对已存在文件的 `write` 说
*overwritten*，而不是借用 `edit` 的 “updated”：文件本来就在那里，这是只有执行器
知道的事实，而一个以为自己在新建文件的模型应该听到它。计数那段只在 `replace_all`
触及不止一处时出现 —— 那是 diff 带着而参数里没有的唯一一条事实，因为唯一匹配永远
恰好是一处。什么都没改变的 write 保持单文本：`No changes to {path}` 已经是全部真
相，也没有正文可以替模型省下。**失败**同样保持单文本 —— 错误消息*就是* 模型恢复
时必须读的东西。）

省下的量是真的，因为改动没有被丢掉，只是被**搬走**了：现在每次 call 都记录模型逐
字的参数，而派生 context 在助手那条 `tool_calls` 条目上重放它们
（`docs/architecture/context.md`）。在此之前，一次 `write` 重放成
`write({"path": …})`，内容则每个回合都作为带行号的结果回来 —— 大约是内容自身
token 的 1.3 倍，就为了模型刚写下的那份副本。现在内容以模型送出时的形状随 call
走一次， provider 的 prompt 缓存就能把它留住
（`docs/providers/prompt-caching.md`）。

结尾那句话是关于这次重放的断言，所以两个文件 tool 的 schema 用同一句话说明同一件
事——
*“Returns a one-line confirmation: the content you sent stays in the conversation, so do not read the file back to check it”* ——
因为一个看不出结果为什么变短的模型，否则就会去发一次用于校验的 `read`，把文件再
上传一遍，把省下的全赔回去。

每个失败都以**非 ok 的 `ToolOutcome`** 返回，带一条人或模型可读的消息（绝不
panic）—— 模型把错误字符串当作 tool 结果看到并自行恢复，和 codex 的
`RespondToModel` 一样。

### 没有控制终端 —— sudo 提示的修法（`crate::subprocess`）

这是命令在拿不到自己终端时所获得的环境 —— `!` shell、`bash` call 的管道兜底
（`docs/tools/bash-tools.md`）—— 也是命令在终端里运行之前每个模型命令所获得的环
境。

`stdin(Stdio::null())` **并不能**让命令变成非交互：密码提示（`sudo`、`ssh`、git
的 credential helper）会打开 **`/dev/tty`** —— 控制终端，它是通过进程 *session*
继承来的，不是通过任何 fd —— 把提示直接写在 live 区域上，然后阻塞在读键盘上，而
那把键盘归事件循环所有。所以每个 shell 运行器（本执行器、
`BackgroundRegistry::launch`、`!` shell）都经由
`subprocess::spawn_detached_shell` 启动，它把命令跑在一个
**没有控制终端的新 session** 里 —— `/dev/tty` 打开失败，`sudo` 在几毫秒内报错
（`sudo: a terminal is required to read the password`），把单元格收成红色并带上
一条可操作的消息，而不是挂住，与 Claude Code 完全一样。这套机制（一个 `setsid`
二进制 → 助手重执行 → 受 crate 的 `forbid(unsafe)` 塑形的附加层级链）、保留下来
的 `pgid == child.id()` 终止契约，以及完整的测试矩阵，都在
**`docs/tools/tty-detach.md`** 里。

`bash` call 的终端是刻意相反的一端，而且仍然绝不是用户的终端：命令得到一个
**自己的伪终端**作为控制终端（同一条层级链，只是 TTY 形态），于是密码提示到达的
是一张*模型*能读的屏幕，并通过 `bashsend` 作答 —— 或者，当它没被交给那个秘密时，
就去问用户（`docs/tools/interactive-shell.md`、`docs/tools/bash-tools.md`）。

## 图片读取（对 png/jpg/jpeg/gif/webp 用 `read`）

单元格还会在自身下方**画出**这张图，而 `/settings` 的 **Auto-resize images** 决
定超大的图在上传前是否被缩小 —— 两者都在 `docs/ui/images.md`。

`read` 不是只读文本：扩展名为图片的路径会**以视觉方式**回来，Claude-Code 风格，
好让模型能看截图、下载的图，或它刚生成的图表。

- **检测按扩展名**（`tools::is_image_path` —— png/jpg/jpeg/gif/webp，即每个支持
  视觉的 OpenAI 兼容端点都接受的四种格式；纯的，有单元测试）。执行器随后
  **嗅探实际的字节**（`image::ImageReader::with_guessed_format`，剪贴板模块的那
  套做法），这样即使扩展名撒谎，`data:` URL 的 MIME 也与内容相符；而任何实际上不
  属于这四种的东西都会失败为一条模型能读到的可恢复错误。图片忽略 `offset`/
  `limit`。
- **tool 结果仍是一小段文本** —— 一行简洁的事实
  `Read image ({format}, {W}x{H}, {size})`（`tools::format_read_image`，例如
  `Read image (PNG, 512x512, 17 KB)`）：路径不出现在里面，因为 `● Read({path})`
  头部已经显示了参数，而后面的 `[image]` 注记为模型点名了它。单元格显示的是这一
  行， session 录制记录的是这一行，token 计数数的也是这一行；像素从不进入
  `output`。
- **像素走 `ToolOutcome::image`**（一个 base64 的 `data:` URL，在执行器边界编
  码）， `run_agent` 把它们作为一条后续的 **user 角色 parts 消息**附上 ——
  `[image] …` 注记（`tools::image_attachment_note`）加一个 `image_url` part ——
  位置在那一轮的 tool 结果*之后*。为什么不放进 `role:"tool"` 消息里？Chat
  Completions 的 tool 内容在 OpenAI 与大多数兼容实现上只接受文本 part —— 那里放
  `image_url` part 就是一个 400。带图片 part 的 user 消息是这个代码库为 Ctrl+V
  粘贴早就发过的标准视觉形状，而回合中途注入 user 角色消息也是既定的模式（后台通
  知就是这么做的）。结果仍 **连续**排在附件之前：严格的 provider 要求每个
  `tool_call_id` 都紧接着助手消息被回答。
- **后续回合仍能看见它**：`context_messages` 从存下的记录里识别出一次图片读取
  （`name == "Read"` 加 `Read image ` 输出标记 —— 文本读取总是以带编号的行号槽、
  `(file …` 或一条 `could not read …` 错误开头，所以这个标记不会撞车），并带着路
  径作为 `images` 附件重放同一条注记。`build_messages` 在每次请求时接上 session
  对这个附件唯一的一份共享编码 —— `read` 已经把它的载荷交给了
  `images::remember_attachment`，所以之后没有任何回合再读一次文件或再 base64 一
  次（`docs/architecture/memory.md`）；一个此后被删掉的文件会退化成已有的
  `[image unavailable: …]` 文本注记，而不是让回合失败。
- **尺寸上限**：`READ_IMAGE_MAX_BYTES`（原始 3.75 MB，这样 base64 形式仍低于最严
  格的主流 provider 每图 5 MB 的限制 —— Claude Code 自己的上限）。超大的文件以一
  条可恢复的 “downscale or convert it with a bash command first” 失败。
- **渲染**：单元格把事实行显示为普通的 `⎿` 输出块 —— `ui::parse_file_cell` 要求
  带编号的行号槽，否则退回旧式渲染；而终端反正也显示不了像素（与 `[Image #N]` 粘
  贴占位符同一立场）。

### 视觉检测 —— 在纯文本模型上优雅降级

把图片附给一个看不见的模型不是软失败：OpenRouter 会**把整个请求 404 掉**（“No
endpoints found that support image input”），所以没有一道闸门的话，在例如
`openai/gpt-oss-120b` 上一次图片读取（或一次 Ctrl+V 粘贴）就会把回合弄成红色。这
道闸门复用了 Ctrl+T 思考循环确立的 `/v1/models` 能力探测模式
（`docs/providers/reasoning.md`）：

- **检测**（`models::vision_support_of` → `ModelEntry::vision`，和推理嗅探一样逐
  条记录）：OpenRouter 的 `architecture.input_modalities` 数组（里面是否含
  `"image"`），退回更早的合并式 `architecture.modality` 字符串，按它的**输入**一
  侧判定（`"text+image->text"` 看得见；`"text->image"` —— 一个生成器 —— 看不
  见）； Venice 的 `model_spec.capabilities.supportsVision`；一个光秃秃的顶层
  `input_modalities`（ChatGPT backend 的，以及像 OmniRoute 这类网关的，后者那个
  由布尔值组成的扁平 `capabilities` 对象从不会被当作 Anthropic 的
  `{"supported": bool}` 树来读 —— `docs/providers/claude.md`），否则取那个网关扁
  平的 `capabilities.vision`（`docs/providers/user-providers.md`）。什么都不说的
  记录（一个光秃秃的 OpenAI 风格列表）给出 `None` = 未知 → 乐观地附上，与旧行为
  完全一致。
- **流程**：选中条目的 `vision` 随 `Action::SelectModel` 进入重建后的 backend
  （`Selection::vision` → `ModelConfig::vision` → `LlmBackend`），与思考那块一起
  持久化在 `config.json` 里（`Settings::vision` —— 旧文件只是重新探测一次），而
  启动时的能力探测（现在是思考**和**视觉）为按环境变量选中的模型播种它。
- **`Some(false)` 时的效果**：`read` tool 会在*碰文件之前*以一条可恢复的错误拒绝
  图片路径（“the current model does not support image input — … ask the user to
  switch … with /model”），所以模型能在回合内自行调整；而 `build_messages_for`
  会把每个附件 —— 一次粘贴，或一张重放的读取图片 —— 换成一条
  `[image omitted: {path} — …]` 文本注记，于是过去的图片只是降级，而不是毒化之后
  每一次请求。Ctrl+V 粘贴还会在边界处弹一个红色 toast
  （`docs/input/image-paste.md`）。已实测：
  `live_non_vision_model_gracefully_declines_an_image_read` /
  `live_non_vision_model_survives_a_pasted_image`（模型回答 “I can't view
  images”，而不是让回合死掉）以及两个 `live_*_models_report_vision_support` 列
  表。

## 启用 / 停用

对真实 backend 而言 tool **默认开启**，对 dummy 则从不开启（dummy 的固定 tool 不
受影响）。用 `ZEROTH_TOOLS` 开关（`0`/`false`/`no` 关闭）。启用它们
**不给 system prompt 加任何散文** —— schema 承载了全部能力说明，所以拼出来的
prompt 仍然只是 persona（[`prompts/zeroth.md`](../../prompts/zeroth.md)）→ 运行
时的**环境上下文**（[`prompts/environment.md`](../../prompts/environment.md)，
`docs/architecture/environment.md`），别无他物。（这里过去会追加一段
`prompts/tools.md` 注记；它在每次请求上把那些 token 又花一遍，去说 schema 已经说
了的话。）两个文件都被 `include_str!` 进 `llm::backend`，所以措辞住在可维护的
markdown 里（换个 persona 只要把那个常量指向另一个 `prompts/*.md`）。

执行器**在应用的工作目录里、没有沙箱地**跑命令、写文件 —— 与 `!` shell 相同的信
任模型。未来的版本可以加审批 prompt（codex 的沙箱/审批流程）；v1 信任操作者，而
那个环境变量开关就是关掉它的闸刀。

### 手工实测

`examples/tool_smoke.rs` 不经过 TUI 就能对真实 provider 驱动整个循环 —— 用来看一
看 ToolStart/ToolEnd/作答流程很方便：

```bash
OPENROUTER_API_KEY=sk-... cargo run --example tool_smoke -- \
  openai/gpt-4o-mini "Create hello.py, read it back, then edit its message."
```

（它像应用一样从 `SSL_CERT_FILE`/`ZEROTH_CA_FILE` 读代理 CA。）

## 渲染（codex 的 `diff_render`，落在 `⎿` 沟槽里）

`bash` 单元格以 `!` shell 单元格的形状渲染 —— 彩色的 `● Bash(cmd)` 头部压在一段
**多行 `⎿` 输出速览**之上 —— 但 shell 单元格把整段输出就地显示
（`docs/input/shell-command.md`：那是用户为了读它才跑了那条命令），而 `bash` 单
元格的读者是模型，所以它的头部按 Claude Code 的方式**折叠**：前 `TOOL_FOLD_ROWS`
（3）个折后行，然后是 `… +N lines (ctrl+o to expand)`，数的是展开会多出的显示行
（恰好四行的输出整段显示 —— 藏起一行去提示会正好赔掉它藏起的那一行；
`docs/ui/long-lines.md`）。它那行 `Exit code: N` 的框（在 `tool.output` 里为模型
/ context 重放保留）在显示时被剥掉，所以单元格读起来就像真正的命令输出；而
**一行本身是 JSON 文档时会被重排显示**，用两空格缩进（`ui::exec_display_lines`，
Claude Code 的 tool 结果也一样这么做），于是一次 API 的 `curl` 读出的是 `{` /
`"batchcomplete": "",` / `"query": {`，而不是一堵大括号墙 —— 记录本身仍逐字节不
变。**它运行时单元格会流式输出并跟随尾部** —— 头部、最后几行，以及一个
`+N lines (22s · wait 1m 50s)` 页脚，点出命令自己的时钟与这次 call 所处的等待
（没有隐藏内容时是一行光秃秃的 `(10s · wait 10m)`，还没有任何输出时是
`⎿ Running… (10s · wait 2m)`）—— 见 `docs/ui/tool-streaming.md`。

**整个单元格读起来像一条普通回复 —— Claude Code 那种显眼的样子。** `(...)` 头部
的整个主体 —— 命令文本、它外面的 `(`/`)`，**以及**截断用的 `…` —— 都是
**粗体 + 助手文字的白色**（`tool_args_color()`），而 `⎿` 沟槽下的**输出**是同一
种白色（`tool_output_color()`），于是一条 `bash` 命令和它的输出和普通消息一样清
楚，而不是从前那种压暗的灰。只有结构性部件保持暗色（[`tool_dim_color()`]）：`⎿`
拐角字形、`Running…`/`Waiting…`/`(no output)` 占位符，以及 `… +N lines` /
`+N lines (Ns · wait …)` 提示。`●` 圆点保留它的生命周期颜色（运行时闪灰 ——
`docs/ui/tool-pulse.md` —— 成功鲜绿 · 失败红）。这在**每个** tool 上都一致 ——
`bash`/`read`/`write`/`edit` 以及任何未来的 tool —— 因为头部走的是共享的
`tool_header_lines`，而命令/shell 输出走的是共享的 `output_row`。（而 `read`/
`write`/`edit` 的带编号单元格改为对正文做语法高亮 —— 本来就很鲜亮 —— 见下文。）

**长头部换行，绝不裁切**（`ui::tool_header_lines`）。一条长命令 ——
`● Bash(curl -s "wttr.in/…" 2>/dev/null || echo "…")` —— 过去会跑出终端右缘，最
后一列之后的东西全丢。现在 `(args)` 会**换行**到续行上，每一行都缩进到
**开头的 `(` 底下**对齐（按 `● Bash` 的宽度，Claude-Code 风格 —— 续行正落在括号
下面，而不是偏出一列），于是整条命令读起来干净：

```
● Bash(curl -s "wttr.in/Warsaw?format=%C+%t+%w+%h" 2>/dev/null
      || echo "wttr.in unavailable, trying alternative...")
  ⎿  Partly cloudy +19°C ↓8km/h 83%
```

这个换行是**Claude Code 的，逐行一致** —— Ink 的 `wrap-ansi` 在它的 `hard` 模式
下：词整个往下移，但一个在**任何**一行都放不下的 token —— 一条长 URL —— 从它当前
所在的位置断开，填满它所在的那一行，除非把它挪到下一行严格更省行数（打平时就往下
移，于是一个从填充里毫无所得的 token 仍然自己开一行）。旧的换行先把每个超宽的
token 往下挪，把它前面那行留短，这正是 72 列下被报告的那个差异（我们现在填满同样
的行，并早一行停下，停在下文那个两行上限处）：

```
Before:
● Bash(curl -s --max-time 15
      "https://en.wikipedia.org/w/api.php?action=query&prop=extracts&exi
      ntro&explaintext&format=json&titles=World%20Chess%20Championship…)

Claude Code:
● Bash(curl -s --max-time 15 "https://en.wikipedia.org/w/api.php?action=
      query&prop=extracts&exintro&explaintext&format=json&titles=World%2
      0Chess%20Championship%202026"…)

Now:
● Bash(curl -s --max-time 15 "https://en.wikipedia.org/w/api.php?action=
      query&prop=extracts&exintro&explaintext&format=json&titles=World…)
```

（`ui::wrap` 的 `fills_from_here`，在 `WrapMode::Output` 扫描里 —— 所以头部下方
的输出行也以同样方式填充，Ctrl+O 视图同样如此。）

**命令自己的空格能挺过头部。** 一次 `bash` call 记录下来的摘要
（`tools::summarize_call`）是命令**逐字**（去首尾空白），而头部用与它下方输出行
相同的保留空白的按词换行来折它（`ui::wrap_output_hanging`，`wrap_output` 的悬挂
缩进形式）：引号里的一串空格仍是一串 —— `echo "a b"` 绝不会读成 `echo "a b"` ——
而一个 **换行独占一行**，收尾的 `)` 骑在最后一行上，因为换行是语句边界，而旧的一
行压平把 `cd foo\nls` 显示成了另一条命令 `cd foo ls`（heredoc 的正文被并进了它的
开头行）。制表符按代码块的方式展开显示。问过同一条命令的权限 prompt 早就这么显示
它了，所以它变成的单元格与之一致：

```
● Bash(cat > hello.py <<'EOF'
      print("hi")
      EOF)
```

只有头部保留它：那些单行的界面 —— agent roster 的 `{Name}: {args}` 活动行，以及
auto 模式 classifier 的 `Name(args)` 动作列表 —— 自己把摘要压平
（`tools::flatten_one_line`）。`(` 骑在**名字**那一行上，而不是第一个参数词上，
于是一个在名字旁边剩余宽度里放不下的参数**整个**溢出到续行 ——
`● Deepwiki - ask_question (MCP)(` 压在 `repoName: "…"` 之上 —— 而不是被硬断成两
半（`(repoName` / `: "…"`，四十列终端从前会呈现的形状；`docs/tools/mcp.md`）。

**极长的头部按 Claude Code 的方式就地切**（`ui::tool::header_cut`，移植自它的
`renderToolUseMessage`）：命令的前 `TOOL_HEADER_MAX_LINES`（2）行，且至多
`TOOL_HEADER_MAX_COLS`（160）个显示列，去尾部空白，然后 `…)`
（`TOOL_HEADER_ELLIPSIS`）。这个标记贴到最后一个保留下来的词上（切出的文本去过尾
部空白，所以 `word …)` —— 在一个*找不到的*词之后切 —— 绝不会出现）。在一个能用两
行装下这 160 列的终端上，也就是大约九十列及以上，这就是全部规则：对**文本**设预
算，于是同一条命令在每个这样的宽度下都显示同样的字符，而一段多行脚本显示它的前两
条语句。在 100 列下：

```
● Bash(for i in {1..5}; do echo "=== Iteration $i ===" && echo "Current time: $(date)" && echo
      "System uptime: $(uptime)" && echo "Memory usage: $(free -h | grep Mem)"…)
```

**折叠的头部绝不吃第三行**（`ui::tool::cap_header_rows`，`TOOL_HEADER_MAX_ROWS`
= 2）。`● Bash(` 这个开头、续行缩进、以及换行参差的行程末尾都在花列，所以在大约
九十列以下，那 160 就折到了第三行 —— 而那一行装的是切口的结尾，常常是某个 token
的一部分，出现在一个输出止于四行的单元格里。现在头部停在它第二行的末尾。报告如
下， 87 列：

```
Before:
● Bash(cd /home/linuztx/.zeroth/skills/web-search && PARALLEL_SESSION_ID=$(python3
      -c "import secrets;print(secrets.token_hex(16))") && python3
      scripts/parallel_se…)

Now:
● Bash(cd /home/linuztx/.zeroth/skills/web-search && PARALLEL_SESSION_ID=$(python3
      -c "import secrets;print(secrets.token_hex(16))") && python3…)
```

这个上限作用在折后的行上，所以它保留的行正是整条命令折出来的那些，而最后一行以
`…)` 结尾 —— 在最满到边缘时，它让出标记所需的那两列，这可能切断一个词（上文
`titles=World%2` → `titles=World…)`）。它数的是**参数**的行：一个溢出头部那种只
有名字的第一行（四十列下的 `● Deepwiki - ask_question (MCP)(`）不算在那两行里，
所以溢出仍然不会赔掉它为容纳参数而付出的代价。九十列以下，决定切口落在哪里的是宽
度，而不是文本预算。这代价很小：每次 resize 都会从历史重折这些单元格，所以屏幕永
远与它自己的宽度相符，而整条命令离一个 Ctrl+O 只差一下（在权限 prompt 上则只要有
人在问它）。

**提交的 heredoc 折叠进它的消息里**（`ui::tool::collapse_heredoc`，Claude Code
的规则，移植自它的正则）：`git commit -m "$(cat <<'EOF'\n{body}\nEOF\n)"` 是带正
文的消息到达 shell 的方式，而 `cat` 这套脚手架对命令本身什么也没说，所以头部显示
`git commit -m "{body}"` —— 再配上两行切口，就是
`● Bash(git commit -m "Add the fold…)`。开头必须落在命令的第一行，正文一直延伸到
第一个 `EOF` 行加上 `)"` 收尾，而收尾之后的东西必须留在它自己那行上；任何别的形
状都逐字显示。只是显示，就地单元格与 Ctrl+O 里都一样 —— 记录、权限 prompt 和
context 重放都保留命令逐字节不变。

换行与对齐是就地速览（`tool_lines`）与 Ctrl+O transcript（`tool_full_lines`）共
享的，所以一次 resize/重排会以同样方式重折到新宽度 —— 但只有就地速览（和实时预
览）会切命令（`collapsed = true`）；Ctrl+O 视图传 `false`，显示**整条**未截断的
命令。

**运行中的 backend tool 会预览它整个单元格。** 模型的 tool 运行期间，流式条的预
览槽显示*完整的*实时单元格 —— 折好的头部**加上**它的输出。在任何输出之前，`bash`
单元格显示 `⎿ Running… (10s · wait 2m)` —— 命令自己的时钟紧挨着这次 call 所处的
等待；一旦有输出流出它就**跟随尾部** —— 最后 `TOOL_PEEK_ROWS` 行再加上
`+N lines (22s · wait 1m 50s)` 页脚，没有隐藏内容时则是那行光秃秃的时钟
（`ui::running_command_lines`；见 `docs/ui/tool-streaming.md`）—— 所以运行状态可
见，而一条长命令在运行途中也不会被裁掉。预览槽的高度由 `ui::preview_rows` 决定
（空闲 0· 流式回复 / `!` shell 运行 1 · 运行中的 backend tool 单元格 N）；状态行
（spinner + token 计数 + `esc to interrupt`）留在它下面。在 `ToolEnd` 时，流式条
的单元格被就地换成已提交的 scrollback 单元格（头部 + 头部速览）。

一个输出是上文带编号格式的 `read`/`write`/`edit` 单元格渲染成
**codex/Claude-Code 的文件单元格**（`ui::file_cell_lines`）—— `⎿` 拐角占两列宽
（` ⎿ …`，拐角内容在第 5 列），带编号的正文再**往里一列**
（`ui::file_body_indent`），与 Claude Code 一致：

```
● Read(index.html)
  ⎿  Read 254 lines
        1 <!DOCTYPE html>
        2 <html lang="en">
        …
     … +244 lines (ctrl+o to expand)

● Write(index.html)
  ⎿  Wrote 254 lines to index.html
        1 <!DOCTYPE html>
        2 <html lang="en">
        …
     … +244 lines (ctrl+o to expand)

● Edit(index.html)
  ⎿  Updated index.html (+2 -2)
        5     <meta name="viewport" …>
        6 -   <title>Portfolio</title>
        6 +   <title>Bruce Rivero</title>
        7     <link rel="stylesheet" …>
          ⋮
       16 -   <span>Portfolio</span>
       16 +   <span>Bruce Rivero</span>
```

- **白色**的摘要头（`tool_output_color()`，所以它和输出一样显眼 —— 不是暗的）坐
  在 `⎿` 拐角行上：`Read {N} lines`、`Wrote {N} lines to {path}`，或
  `Updated {path} (+A -D)`，其中 `(+A -D)` 的计数按绿/红着色（codex 的头部计
  数 —— `file_summary_spans`）。`read` 单元格的输出里没有头部，所以
  `ui::parse_file_cell` 合成一行 `Read {N} lines`；改名前的
  `Created {path} ({N} lines)` 头部（旧 rollout 仍带着它）以同样的无符号方式解析
  出来。比终端更长的头部会在拐角下方**按词换行**（`ui::summary_head_lines` 套
  `wrap_inline_hanging` —— 路径是一个词，所以超长路径硬断），计数的着色随换行一
  起走，而不是在边缘被裁掉。
- 正文行重新样式化输出自带的行号槽文本：右对齐的**行号**暗色，`+`/`-` **符号**
  绿/红（`read`/`wrote` 的正文没有符号列），内容按路径扩展名做**语法高亮**
  （`highlight::Highlighter`，围栏代码那套配色 —— 扩展名取自单元格的 `args`，而
  词法状态在每个 `⋮` 间隔处重置，像 codex 的逐片段高亮）。因为 `format_read` 现
  在发出的行号槽与 write/edit 正文**相同**（`{n:>W} {text}`，动态宽度数字，一个
  空格—— 不是旧的定宽 `cat -n` 制表符），三者解析与渲染得完全一致。
- **新增的行**躺在暗绿的背景底色上，**删除的行**（文本压暗）躺在暗红的底色上 ——
  codex 暗色主题的 `#213A2B`/`#4A221D` 底色 —— 两者都像用户消息块那样铺满整宽。
  上下文行不带底色。`⋮` 间隔行与 `…` 注记行保持暗色。
- 一对 `-`/`+` 若是对某一行的**修改**而非整行替换，**真正不同的那些字符**会被提
  亮到更亮的底色并加粗（`ui::inline_diff`，`docs/ui/inline-diff.md`）。于是柔和
  的整行底色回答*这一行变了吗？*，而那个标记回答*变在哪里？*，不用把两行对着读：

  ```
    ⎿  Updated hello.txt (+1 -1)
        1 -Bruce River█          ← only the `a`, bright red, bold, undimmed
        1 +Bruce River█          ← only the `o`, bright green, bold
  ```

  规则是**只有变了的东西才着色**：`Bruce River` 两侧相同，所以它保持普通的整行底
  色。仅仅靠近改动并不被标记 —— 两处改动之间一段没变的文本绝不会被桥接，而一个变
  成 `10` 的 `1` 只标记新增的那个 `0`，`1` 保持原样，因为它没被碰过。这个标记逃
  出删除行的 `DIM`（把眼睛该找到的那一样东西压暗，等于毁掉标记它的意义）并在换行
  后仍然存在。它的墨色对标记保持 3:1 的对比（`INLINE_DIFF_MARK_MIN_CONTRAST`）。
  注释的灰色可能与标记的亮度相当，在 Dracula 里是 1.0:1，所以一个在那里读不出来
  的语法颜色会被提亮到主题的文字色，直到读得出来。一对差异太大、不像是修改的，就
  保持平铺 —— 被替换的行没有“变了什么”可指，否则会回来时斑斑点点全是两段文本碰巧
  共有的字母。它由解析出的正文派生，所以在它存在之前录下的 rollout 上也会亮起
  来，而权限 prompt 的预览也得到它。
- 长行**换行**（`code_content_rows`），续行缩进到内容列之下，保留颜色与底色。在
  Ctrl+O 与权限 prompt 的预览里，一行显示它折出的每一行；就地速览只切那唯一一行
  超出第十行的（`docs/ui/long-lines.md` §7、§8）。
- **就地速览**至多 `FILE_PEEK_LINES`（10）行。它还有行可用时从顶部开始铺正文行，
  剩余行装得下就整行显示，需要更多行的就在第十行切掉，并用一个暗色 `…` 在其底色
  上收尾那一行 —— 然后是 `… +N lines (ctrl+o to expand)` 提示，这里数的是
  **文件行**（它从未开始的那些），即行号槽与展开所显示的单位；当被切的是最后一行
  时，就是不带计数的 `… (ctrl+o to expand)`；Ctrl+O transcript 显示全部。

### 路径显示

`● Read/Write/Edit({path})` **头部**按 Claude Code 的方式显示路径，依据是它
**在哪里**（相对于 session）（`tools::header_path`，拐角行 `display_path` 的同门
兄弟，共用同一个词法内核，由 session 策略 `app::PathDisplay` 携带；那个示例是在
`~/Codes/tests` 里启动的）。它下面的拐角头部是执行器自己的记录 —— `display_path`
的 cwd 相对形式，cwd 之外是一段 `../` 爬升 —— 永不重写。两者回答不同的问题：头部
说 *哪个文件*，拐角行说从这里怎么到它：

```
● Write(hello.py)                      ← /home/linuztx/Codes/tests/hello.py, under the cwd
  ⎿  Wrote 1 line to hello.py
● Write(hello/hello.py)                ← …/Codes/tests/hello/hello.py, still under it
  ⎿  Wrote 1 line to hello/hello.py
● Write(~/hello.py)                    ← /home/linuztx/hello.py: outside the cwd, under home
  ⎿  Wrote 1 line to ../../hello.py
● Edit(/tmp/notes.txt)                 ← outside home: absolute
  ⎿  Updated ../../../../tmp/notes.txt (+1 -1)
```

- **在 cwd 之下 → 相对**（`hello.py`、`src/app.rs`；cwd 自己就是 `.`）。
  **不在其下但在 home 之下 → 相对 `~`**，即使是近邻：`~/Codes/other/x.py` 说的是
  文件在哪里，而一段 `../other/x.py` 的爬升只说了它有多远。
  **其他任何地方 → 绝对**，按词法规范化（`/tmp/./a/../x.py` → `/tmp/x.py`）。包
  含关系按路径分量判定（`/home/user2` 不在 `/home/user` 之下；另一个用户的 home
  不是 `~`），`.`/`..` 折叠，相对参数先对 cwd 解析，而符号链接从不被查询。schema
  要求模型给绝对路径，所以这就是以前每个头部把大部分行都花在它上面的那个东西。
- **它是渲染时的规则，而且只是渲染时的规则。** 头部的记录 —— `ToolCall::args`，
  那段逐字的摘要 —— 不被改动；`ui::tool_header_lines` 通过 session 的策略缩短被
  画出的那一行（`ui::tool::shown_args`，唯一一处说“文件 tool 的摘要就是一个路径”
  的地方）。于是派生 context（Ctrl+D）、rollout、auto 模式 classifier 的
  `Name(args)` 动作日志与权限规则全都保留绝对路径 —— 一个回读自己历史的模型看到
  的正是它送出去的东西 —— 而就地单元格、实时条的运行单元格、Ctrl+O transcript、
  subagent 会话视图、权限 prompt 的目标行（`docs/state/permissions.md`）、实时
  agent 树的 `Write: {path}` 活动行（`AgentRun::activity_shown`，基于记录下来的
  `last_call`），以及 Ctrl+O agent 展开里嵌套的 `Write({path})` 头部
  （`app::file_tool_header`，即写下它们的 `tool_header_text` 的逆；
  `docs/agents/agent-tool.md`）全都显示短形式。因为记录是被缩短的*来源*，一个
  **从另一个目录恢复的** session 读到的头部是相对于它*现在*所在之处的 —— 一个存
  了短形式的 rollout 在移动之后会点出一个不同的文件。
- **拐角头部是执行器的，不是渲染器的。** `Wrote {N} lines to {path}` /
  `Updated {path} (+A -D)` 带的是执行器写这个单元格时选定的路径
  （`describe_change` → 对进程 cwd 做 `tools::display_path` —— 其下是
  `hello.py`，其上是 `../../hello.py`），而文件单元格照原样画那段文本，旧
  rollout 与遗留的 `Created {path} ({N} lines)` 头部也不例外。所以两行可能用两种
  方式点名同一个文件（`~/hello.py` 压在 `../../hello.py` 上）：头部说文件在哪
  里，头部之下那行说执行器是怎么到达它的。
- **只有文件 tool 的摘要是路径。** `tools::is_file_tool`（`Read`/`Write`/
  `Edit`，按显示名）把着这道门：一条 `Bash` 命令内嵌的是 shell 将要解析的路径，
  `Agent` 的摘要是散文，MCP call 的摘要是 JSON，而 `!` shell 单元格根本没有头
  部 —— 那些都逐字回显记录。
- **而显示出来的路径是到该文件的链接。** 头部的路径带着一个 OSC 8 超链接，其目标
  是文件的绝对 `file://` URI（`PathDisplay::file_url` → `links::file_url`：相对
  参数先对 cwd 解析，`.`/`..` 折叠，百分号编码），无论那一行画的是哪种短形式，所
  以点 `hello.py`、`~/hello.py` 或 `/tmp/notes.txt` 都会打开那个文件 —— 标记在换
  行之前打上，所以一个被断到多行的长路径从任何片段都能整段打开。只是路径：`(`/
  `)` 与它下面的拐角头部（`Wrote 1 line to hello.py`）都不带。见
  `docs/ui/links.md` 的 *The file tool header*、`smoke.sh` 的 Phase 113。
- **注入，从不读取。** 这个策略是进程 cwd 加 `$HOME`，像时钟一样在 bootstrap 时
  交给 `App::set_path_display` 一次，所以纯渲染器从不碰环境；
  `PathDisplay::VERBATIM`—— 单元测试的默认值，也是 cwd 读不出来时的兜底 —— 什么
  都不缩短，这正是让其他每个渲染测试仍在描述它从来描述的那些行的原因。`~` 那一半
  是页脚自己的 `display_cwd` 规则。

所有样式都集中在 `ui/theme.rs` 的 `TOOL_DIFF_*`/`FILE_PEEK_LINES`/
`INLINE_DIFF_*` 常量里。**解析不出**带编号格式的输出 —— 在这个格式存在之前录下的
rollout、一段错误正文，或一个像 `(file is empty)` / offset-past-end 这样的
`read` 占位符 —— 退回旧式渲染（`write`/`edit` 按首字符 `+`/`-` 着色，或 `read`
的普通暗色速览），所以旧 session 与边缘情况仍然渲染得合理。

## 已知限制（v1）

- 没有沙箱 / 审批 prompt —— 执行器信任操作者（由环境变量开关）。
- `edit` 要求一个唯一的 `old_string`（或 `replace_all`）；它不做 codex 的
  `apply_patch` 那种模糊上下文匹配。
- 一条助手回合里的并行 tool call 会提前宣告（一个 `StreamEvent::ToolBatch`），所
  以每个运行中的 call 都活生生地显示 —— 可能有几个同时在跑，各自跟随自己的输
  出 —— 而还没跑的显示 `⎿ Waiting…`，各自在解决时提交
  （`docs/tools/parallel-tools.md`；`CLAUDE.md` 里的不变量 4）。一批可证明独立的
  call —— 全是 `bash`，或全是点名不同路径的文件 call，绝不混用 shell 与文件
  tool—— 在一个有界的 worker 池上并发执行；其他每一批都保持串行。
- token 计数仍是应用侧的估算（协议的 `usage` 仍然没有被透出）。
- `write` 重放的参数是**不封顶的**，而它们取代的带编号结果被切在
  `DIFF_MAX_LINES`（400）。所以写一个 2 000 行的文件会把全部 2 000 行带进之后每
  个回合，而不是 400 行加一条 `… N more lines` 尾巴。那是模型自己产出的线上形
  状 —— 截断一次 tool call 的*参数*会向模型误报这段对话，而旧的上限悄悄缩短了模
  型对自己文件的记忆 —— 但对一个满是巨大写入的 session 来说，这是一笔实实在在的
  开销，要靠 auto-compaction（`docs/architecture/compact.md`）、而不是这条路径来
  吸收。
