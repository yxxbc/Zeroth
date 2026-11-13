# dummy 后端与它的场景注册表

`DummyAi` 是内置的离线后端。没有配置任何 provider 时跑的就是它，而 —— 更要紧的是
 —— 它是 `scripts/smoke.sh` 唯一能确定性地驱动的后端。所以每个本来需要一个真模型
的 UI 功能在这里都有一个**脚本化回合**：一个并行 tool 批次、一个 subagent 组、一
张流式 markdown 表格、四次权限往返、`/compact` 的摘要。

这就让 dummy 成了个奇怪的东西：一个 demo 后端，同时又是半个程序的测试台。每落地
一个功能它就长出一个新回合，而它以前住在 `src/stream.rs` 里，和每个*真*后端都说
的线协议待在一起。

## 以前错在哪

dummy 是那个 2,459 行文件中间的 1,100 行，而那个文件还定义着 `StreamEvent`、
`ReplySource` 和 `CancelToken`。读那个接缝意味着要滚过一堆罐头 `ping` 输出；加一
个 demo 意味着编辑那个定义线格式的文件。

选择更糟。播哪个 demo 由**两处手写的 `if`/`else` 链**决定：

```rust
// in DummyAi::spawn — the gated demos
if let Some(gate) = permissions.filter(|_| prompt.to_lowercase().contains("permission")) {
    if prompt.to_lowercase().contains("auto")            { dummy_auto_permission_turn(…) }
    else if prompt.to_lowercase().contains("parallel")   { dummy_parallel_permission_turn(…) }
    else if prompt.to_lowercase().contains("staggered")  { dummy_staggered_permission_turn(…) }
    else                                                 { dummy_permission_turn(…) }
    return;
}

// in turn_events — the scripted ones
if prompt.starts_with(COMPACT_PROMPT_MARKER) { … return }
if prompt.to_lowercase().contains("table")   { … return }
if lower.contains("agents") && !lower.contains("agents.md") { … return }
…
if prompt.to_lowercase().contains("parallel") { … } else { … }
```

九个 demo，九个 cue，没有表。没有任何东西说明哪些 cue 被取走了，也没有任何东西抓
住一个被更早的 cue **遮蔽**的 cue —— `"staggered permission demo"` 也提到
`"permission"`，所以把这四个分支的顺序排错，会静默地让一个 demo 退休。smoke 套件
照样会通过：它只是在测一个和它以为的不一样的回合。

## 现在的形状

`src/stream/` 是一个按领域分模块的目录（见
[`module-layout.md`](module-layout.md)），而 dummy 是它里面一棵自成一体的子树：

| 模块 | 装着什么 |
|--------|-------|
| `dummy/mod.rs` | `DummyAi` —— `ReplySource` 实现、`turn_events`，以及播放节奏（`with_startup_delay` / `with_chunk_delay`，也就是 `ZEROTH_STARTUP_DELAY_MS` / `ZEROTH_CHUNK_DELAY_MS` 设的两个旋钮）。 |
| `dummy/scenario.rs` | **那个注册表**：`Cue`、`Scenario`、`SCENARIOS`、`select`。 |
| `dummy/script.rs` | 罐头回复、它们用来收尾的 `handoff!()` 句子，以及流式/输出原语（`chunks`、`tokens` —— 慢流速压力 demo 据以流出的 token 尺寸切分（`docs/ui/slow-stream.md`）、`dummy_response`、`reply_parts`、`image_ack`；file cell 的正文直接来自 `llm::tools::write_report`/`update_report`）。 |
| `dummy/turns.rs` | **纯的**脚本化回合 —— 每个场景一个 `Cue -> Vec<StreamEvent>`。 |
| `dummy/gated.rs` | 会*问*的回合，阻塞在权限闸门上。 |
| `dummy/agent.rs` | 在 agent 通道上流出**被启动 subagent 自己那一轮**的两个回合，好让 agent 会话视图也能离线驱动（`docs/agents/agent-view-streaming.md`）—— 一个流出一张表，一个自己的并行 `bash` 批次在共享的权限闸门上*发问*。 |

dummy 之上的全都是真后端实现的那个接缝：`event.rs`（整个线格式）、`source.rs`（
`ReplySource`）、`cancel.rs`（`CancelToken`）、`stall.rs`（卡死的后端的替身）。

## 注册表

一张有序表，就是 `app::COMMANDS` 给斜杠命令用的那个形状：

```rust
pub(in crate::stream) const SCENARIOS: &[Scenario] = &[
    // Auto mode's classifier deciding a `bash` batch instead of the user.
    Scenario {
        selects: |cue| cue.mentions("permission") && cue.mentions("auto"),
        play: Play::Gated(gated::auto_permission_turn),
    },
    …
    // The default turn: think, then read/edit/run zeroth's calling card.
    Scenario {
        selects: |_| true,          // the catch-all — keep it last
        play: Play::Script(turns::tools_turn),
    },
];
```

三个类型撑着它：

- **`Cue`** —— 一个场景所读的回合输入：prompt、它的小写形式，以及随行了多少张
  Ctrl+V 图片。每回合构建一次，于是一次注册表遍历不必为试的每个条目重新小写化
  prompt。`cue.mentions("table")` 大小写不敏感（这些 cue 是*用户*打的）；
  `cue.starts_with(marker)` 精确匹配，只用于 `/compact` 这类生成的请求，那里模糊
  匹配会劫持一个关于压缩的真问题。
- **`Play`** —— 一个场景怎么产生事件。`Script(fn(&Cue) -> Vec<StreamEvent>)` 是
  一个纯回合，由 `DummyAi` 带着那些让流式可见的延迟回放。`Gated(fn(&Stage))` 直
  接在通道上流出，并在调用之间阻塞在权限闸门上，和真后端的 tool 线程完全一样。
- **`Stage`** —— 一个 gated demo 流出时所经的东西：闸门、通道、取消 flag。

`select(cue, gate_attached)` 按顺序走这张表，而且是**全的** —— 最后一条匹配一
切，所以没有 prompt 会掉出表尾。一个 gated 场景只在确实挂上闸门时才被提供；没有
闸门它会永远阻塞在一个没人能给的答案上，所以 prompt 落到一个脚本上。这就是
`turn_events` ——它没有闸门 —— 能回答*任何* prompt 的原因。

两个入口现在都经由这同一张表派发：

```rust
// the pure twin, for tests and offline callers
pub fn turn_events(prompt: &str, image_count: usize) -> Vec<StreamEvent> {
    let cue = Cue::new(prompt, image_count);
    match scenario::select(&cue, false).play { … }
}

// the ReplySource impl
match (scenario::select(&cue, permissions.is_some()).play, &permissions) {
    (Play::Gated(play), Some(gate)) => play(&Stage { gate, tx: &tx, cancel: &cancel }),
    (Play::Script(script), _)       => replay(script(&cue), &tx, &cancel),
    …
}
```

## 加一个场景

1. 写这个回合 —— `turns.rs` 里一个 `fn(&Cue) -> Vec<StreamEvent>`，或者
   `gated.rs` 里一个 `fn(&Stage)`。
2. 在 `SCENARIOS` 里加一条带它 cue 的条目。
3. 把它那个示例 prompt 加到 `src/stream/tests/scenario.rs` 的 `EXAMPLES` 里。

第 3 步不是可选的，而这就是重点。`EXAMPLES` 是一份并行的 prompt 列表，按顺序每个
条目一个，而套件断言：

- **长度一致** —— 一个没有示例的新场景会让构建的测试失败，于是没人能不加说明它由
  什么触发就加一个 demo；
- **每个示例选中它自己的条目** —— 上面的遮蔽变成一个点名两个场景的测试失败，而不
  是一个悄悄从不播放的 demo；
- **名字唯一** —— 也就是文档和 smoke prompt 所用的键；
- **选择是全的** —— 包括对空 prompt；
- **gated demo 需要闸门** —— 没有闸门时落到脚本；
- **每个面向用户的脚本都确认挂上的图片**，因为 dummy 没有视觉，而图片通道唯一的
  可见证据就是那个开头的 chunk（`docs/input/image-paste.md`）。`/compact` 的请求
  是那个唯一的豁免：循环构建它，从不给它挂图片。
- **每个面向用户的脚本都以那句交接收尾** —— 也就是下面的 `/login` → `/model` 句
  子。同样的豁免，同样的理由。
- **`turn_events` 与注册表一致** —— 于是第二份派发拷贝无法再长回来。

`Scenario::name` 只存在于 `#[cfg(test)]` 下。运行时没有任何东西读它 —— 选择需要
cue 和回合 —— 所以它在发出去的二进制里会是死重，但正是它让一次失败能说出一个
prompt 挑中了*哪个*场景。`ui::TranscriptCache` 的那些计数器用的是同一个把戏。

## 各个场景

| 名字 | Cue | 它演示什么 |
|------|-----|----------------------|
| `agent-permission` | "subagent" + "permission" | 一个**前台** subagent，它自己的并行 `bash` 批次发问：在它的 session 视图里升起的 prompt 是关于*它自己*的调用的，而不是 lead 的 `● Agent(…)` cell（`docs/agents/agent-view-streaming.md`） |
| `permission-auto` | "permission" + "auto" | auto 模式的 classifier 代用户裁决一个 `bash` 批次（`docs/state/permissions.md`） |
| `permission-parallel` | "permission" + "parallel" | 两个 gated `bash` 调用 —— 背靠背的 prompt，中间没有停顿 |
| `permission-staggered` | "permission" + "staggered" | 一个屏幕高的 `write` prompt 被回答成一个单行的：模态区域最难的那次收缩 |
| `permission-write` | "permission" | 单次 `Write` 批准：选项、Tab 的修订、暂存的草稿 |
| `compact` | 摘要标记 | `/compact` 的纯文本交接摘要（`docs/architecture/compact.md`） |
| `agent-follow-up` | "follow-up" + "agent" | lead 用配套工具**干活** —— 一次后台启动、一次 `agentoutput` 等待（它先流出 agent 在它调用之前说的话，然后是那次调用，`docs/agents/agent-progress.md`）、一次 `agentsend` 续上已完成 agent 的对话、第二次等待、`agentlist` —— 每个调用都经由**真**执行器打到真注册表，所以每个 cell 都是活的，两次等待都沉降为 observed，不欠通知（`docs/agents/agent-tools.md`、`smoke.sh` Phase 130） |
| `agent-stream` | "subagent" | 一个后台 subagent 流出**它自己的 session** —— 一个思考阶段然后一张成形的表 —— 唯一一个驱动 agent 会话视图条带的 demo（`docs/agents/agent-view-streaming.md`） |
| `table` | "table" | 一张带宽 emoji 的流式 GFM 表格（`docs/ui/table-streaming.md`） |
| `markdown` | "markdown"，不是 "agents.md" | 慢流速压力巡游：一段长纯文本回复里每一个 markdown 元素，以 **token 尺寸**的碎片流出（`tokens`）—— 也就是 `smoke.sh` Phase 121 以每秒几个 token 流出的那份文档（`docs/ui/slow-stream.md`） |
| `agents` | "agents"，不是 "agents.md" | 一个两 subagent 的组，前台或后台（`docs/agents/agent-tool.md`） |
| `parallel-batch` | "parallel" | 三个并行的 `Bash(ping …)` 调用和它们的 `⎿ Waiting…` cell（`docs/tools/parallel-tools.md`） |
| `scratchpad` | "scratchpad" | agent 在它的 **session scratchpad** 里起草一个小游戏 —— 一个以 token 尺寸碎片生成的 `write`，然后一个单行 `edit` —— 两者都折进折叠组：一行 `Making 1 scratchpad edit +N…` 在文件路径上随流式计数，最后沉降为 `Made 2 scratchpad edits +26 -1`（`docs/ui/tool-groups.md` *Scratchpad edits*、`smoke.sh` Phase 147）。它写进这个 session 自己的 scratchpad（在边界处交给 `DummyAi`），否则写进一个带 scratchpad 布局的路径 |
| `files` | "diff"/"edit"/"write"，不是 "agents.md" | 同一个文件的一次 `Write` 然后一次 `Edit`：带编号的 file cell 和它的绿/红 diff hunk（`docs/tools/tools.md`） |
| `download` | "pacman"/"download" | pacman 的并行数据库刷新，按终端看到的样子：一次 `bash` 调用，它的四个进度条按 pacman 自己的宽度绘制（所以每个都折成两行），一帧一帧地原地重绘，两个小的在第一秒内结束并**坐在**两个仍在下载的**下面** —— 一个钉在输出末尾的窗口会搞错的那个形状，所以运行中 cell 的窗口跟随还在动的那些条（`docs/ui/tool-streaming.md` *The window follows what is still moving*、`smoke.sh` Phase 130） |
| `interactive` | "interactive" | 经由终端 session 驱动的安装向导：一次 `bash` 启动停在一个 prompt 上、一个 `bashsend` 回答遇上下一个问题、最后一个答案结束程序 —— 每轮一个调用，每个结果都是真的 `pty::report::report`，所以这些 cell 穿着它们暗色的 `Waiting for input · session …` 行（`docs/tools/interactive-shell.md`、`smoke.sh` Phase 124）。只认整词：`tty` 藏在 "pretty" 里，`repl` 藏在 "reply" 里 |
| `explore` | "explore" | 在改任何东西之前先摸清路：一次 `ls -la`、一个想法，然后一轮并行的 `grep`、一个 `Read` 和一次 `head`，最后跑一个脚本 —— 每次读都是一个**折叠组**，它的圆点在全部三轮和它们之间的那个想法上闪过去，由那次脚本运行收尾，后者保住自己的 cell（`docs/ui/tool-groups.md`、`smoke.sh` Phase 146） |
| `tools` | 任何东西 | 默认回合：思考，然后读 `about.py`，`Edit` 补上它忘掉的署名，再跑它 |

cue 的顺序就是注册表的顺序，所以更窄的 cue 坐在也会匹配它的更宽的 cue 之上 ——
`agent-stream` 坐在 `table` 之上，因为那张表是它的 subagent 流出的东西，于是同时
点名两者的 prompt 指的是前者；`agent-permission` 出于同样的理由坐在整个权限块之
上（它的 cue 是 "permission" 加一个词）；而 `scratchpad` 坐在 `files` 之上，因为
一个关于 scratchpad 的 prompt 很可能说 "write"。有两个 cue 带的是守卫而不是顺
序：`agents` 和 `files` 都排除 `agents.md`，因为 `/init` 提交一条点名它的罐头
prompt 并说「do not over**write**」—— 没有这个守卫，那个回合会被一个脚本化的
fizzbuzz 回答。

`files` 和 `scratchpad` 两个 demo 的 `write` 是**生成**出来的，就像真模型生成一
个那样 —— 这一点其他任何脚本化调用都没有：它真实的 JSON 参数在这个调用的槽位下被
切成 token 尺寸的碎片（`stream::tokens`），每一块暂停一个 chunk 延迟，像回复文本
里的一个词 ——于是文件离线地流进它的 cell，尾部窗口沿着它下移，内容闭合时翻回头部
（`docs/ui/write-streaming.md`、`smoke.sh` Phase 138）。其他脚本化调用保留它们那
几块罐头碎片，只是让计数跳一下。

一个场景也可能需要一个这个 session 未必挂上的**句柄**，`select` 在它缺失时跳过
它，让 prompt 落到一个脚本上：权限那几个 demo 需要闸门，ask demo 需要 ask 闸门，
而 `agent-stream` 需要 subagent 注册表（`Play::Gated`/`Asked`/`Agent`）。
`agent-permission` 两个都要：它是一个 `Play::Agent`，所以注册表给它的选择设闸，
而 `AgentStage` 把权限闸门作为一个 `Option` 携带 —— 没挂上时（权限关着）它的
subagent 不发问地跑它的调用，真的 approve 接缝在那里也是这么做的。

## dummy 到底说什么

dummy 是第一次运行会遇到的东西 —— 没配 key 就是没有模型，而不管用户是不是想 demo
它，那个 session 就是 dummy。所以这些回复是干活的，不是填空的：

- **它们承认自己是什么。** 每条回复都一开头就说这些字是罐头的、这些 tool call 是
  脚本化的。
- **它们叙述自己下面的 cell。** 每个场景有它自己两段式的文本，所以 `parallel`
  demo 谈的是 `⎿ Waiting…`，而 `files` demo 谈的是 diff 的着色。默认回合轮换
  其中三个（`dummy_response`，仍然按 `prompt.chars().count() %` 表的长度取
  键，所以它保持确定、可测）。
- **它们以那句交接收尾。** 每条面向用户的回复都以一个共享句子收尾 —— `script.rs`
  里的 `handoff!()` 宏：

> 离真家伙还差两条命令：`/login` 让你登录 —— 一份订阅、一枚 API key 或一个本地模
> 型服务器 —— 然后 `/model` 挑要跑的模型。

`stream::tests::scenario::every_user_facing_script_hands_the_user_off_to_a_real_model`
走一遍注册表，让任何不这么做的场景失败，而 `smoke.sh` 无论发了什么 prompt 都落在
这同一个句子上（`SETTLED_REPLY`）。

一条回复写成**由空行分开的两部分**（`reply_parts`）：这个回合干活之前的文本，
和之后的文本。这不是装饰 —— 一次 tool call 把在它之前的文本定型为自己的历史消息
（`App::flush_streaming_segment`，不变量 4），所以任何别处的分割都会把一段话
或一个列表切到两条消息里，渲染成两个坏掉的块。分隔符骑在第一部分上，所以两者
拼起来仍然是 `dummy_response` 的整条回复。

## 脚本化的 tool 用*真* tool 输出解析

一个渲染得和真 agent 不一样的 demo，是在 demo 错的东西。所以每个脚本化调用都以真
执行器会产出的输出解析，由执行器自己的渲染器构建（`llm::tools` 是纯的，所以这不
花成本）：

| 调用 | 输出 | 这买到了什么 |
|------|--------|----------------|
| `Read` | `tools::format_read` | `ui::file_cell_lines` 解析的 `{n:>W} {text}` 栏 —— 在 `Read N lines` 头下面的一个带编号、带语法高亮（用活动主题的代码主题，`docs/ui/theme.md`）的 cell，而不是纯文本偷看 |
| `Write` | `tools::write_report` —— `Wrote {N} lines to {path}` + 带编号的内容 | 带编号的新文件正文（执行器自己的 `describe_change` 核心，与 gated 权限 demo 共用） |
| `Edit` | `tools::update_report` —— `Updated {path} (+A -D)` + 带编号的 diff hunk | 只有被碰到的那个 hunk，`+` 行在绿色着色上、`-` 行在红色上 |
| `Bash` | `Exit code: N` + 正文 | 模型在它的上下文里读的那个框，也是 `ui::command_display_output` 从 cell 里丢掉的，成功失败都一样，所以一个脚本化的失败是在命令打印的东西上盖一个红色 cell —— 和活的那个一模一样 |

对等一直延伸到 *模型*本会读到的东西，不只是 cell：`Write` 和 `Edit` 经由活执行器
发出的同一个**两段文本** `ToolAnswered` 解析（`ScriptedCall::end`），带编号的正
文在 cell 上、`tools::write_ack`/`edit_ack` 的一行作为结果，而每个脚本化的
`ToolStart` 都带着真调用会带的逐字参数（`ScriptedCall::start`）。所以这个 demo
的 Ctrl+D 显示与活的一样的重放形状 —— 一个内容骑在它自己调用上的 `write`，以及一
个只有一行的结果（`docs/tools/tools.md`、`docs/architecture/context.md`）。

## 默认回合是一个故事，不是一个样品册

回答*任何东西*的那个回合以前是两个不相关的调用 —— 读 `src/main.rs`，然后 ping 一
个解析不了的主机。两个 cell 都渲染了，两个都没意义：这个 demo 读起来像一个控件样
品册。

现在它是同一份文件上三步的一件差事 —— 而这份文件选得让这件差事有个*重点*。
`about.py` 是 zeroth 的名片，它最上面就带着 `CREATOR = "linuztx"` 和
`HOME = "https://github.com/linuztx"`，然后打印一张两者都没提的卡片：

1. **`Read(about.py)`** —— 这个文件，带编号、带高亮。两个没用上的常量就坐在可见
   的偷看里，所以读者在 demo 修它之前就看到那处疏漏。这个文件刻意比 cell 十行的
   偷看多出几行，好让已提交的 cell 带上 `… +N lines (ctrl+o to expand)` 那条尾
   巴，而 transcript 有东西可展开，那是内联 cell 不显示的。那是 **Verbose
   output** 画的 cell；默认情况下一次读反而折成折叠的
   `Read 1 file (ctrl+o to expand)` 行（`docs/ui/tool-groups.md`），它教的是同一
   个键，而叙述两个样子都没点，所以它在两种样子下都成立。
2. **`Edit(about.py)`** —— 改写一行，把那些常量接进卡片打印的东西。它上下文里的
   一个 `+`/`-` 对：仍然能展示两种着色的最小 diff。
3. **`Bash(python3 about.py)`** —— 那次运行，它的输出*就是*署名：

   ```
   zeroth — an autonomous AI agent that lives in your terminal
     created by linuztx · https://github.com/linuztx
   ```

最后一个 cell 是中间那个落地了的证明，而这个 demo 已经说出你正在看的这东西是谁写
的。

那个顺序就是回复所叙述的，也是
`stream::tests::turns::the_default_turn_is_one_errand_in_three_steps` 所钉住的：
三次调用、同一个路径、按那个顺序，命令点名的就是这次编辑改的那个文件。整个回合一
路绿 ——一个无缘无故失败的调用会把故事搅浑，所以那个红色 cell 住在 `parallel` 批
次里，它的第三个 ping 解析不了主机。

`bash` 的框还有值得理清的另一半：执行器**在命令运行时就流出原始输出行**，只在解
析时才把结果框起来。`ScriptedCall` 照做 —— `output` 是它作为 `ToolOutput` 流出的
正文，`result()` 是框好的 `ToolEnd` —— 所以活的尾部永远不会显示一行真后端不会显
示的 `Exit code:`。

## 什么没变

事件的*形状*。每个场景发出的仍然是它原来那个序列：开头一个 `ToolBatch`，一次一个
运行中的调用，`bash` cell 上有 `ToolOutput` 尾巴而 file 那些没有，图片确认在除压
缩摘要之外的一切前面。

共享的活是搬了而不是复制了。四个 gated demo 以前各自带着同一段 40 行批准正文的一
份拷贝 —— 建请求、查允许列表、升起它、阻塞在闸门、映射五个决定、发 `ToolStart`
然后 `ToolEnd` 或 `ToolRejected`。现在它只在 `Stage` 上有一份（`ask`、`judge`、
`resolve`、`close`、`announce`、`request`），而每个 demo 读起来就是它自己的一份
调用脚本。
