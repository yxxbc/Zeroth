# `/context` —— 上下文窗口里装了什么

日期：2026-10-22

## 目标

一条 **`/context`** 斜杠命令，回答长会话里一再到来的两个问题：*上下文窗口有多
满*，以及*是什么在占它*。页脚的量表（`docs/ui/footer.md`）已经说了有多满；没有
任何东西说过是什么。一段 system prompt、二十几个 tool schema、一堆 MCP tool、
项目的 `AGENTS.md`、skills 清单，以及对话本身，每一次请求都带着，而在某个
provider 拒绝之前，它们都是看不见的。

这一页把窗口拆成这些部分，显示每部分的份额，让用户钻进任何一个部分，并报告
provider 对最后一次请求的说法 —— 进和出的 token，以及输入里有多少来自 prompt
缓存（`docs/providers/prompt-caching.md`）。

## 页面

```
──────────────────────────────────────────────────────────────────────────────
  Context usage
  gpt-5.5 · ≈36.4k of 200k tokens (18.2%)

  █████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░╱╱╱╱╱╱╱

    ■ System prompt     161   0.1%   Messages · 48 messages
    ■ System tools     3.2k   1.6%   Tool results   17.2k  ━━━━━━━━━━━━
    ■ MCP tools        1.1k   0.6%   Tool calls      6.1k  ━━━━
    ■ Project docs     1.6k   0.8%   Replies         4.3k  ━━━
    ■ Skills            433   0.2%   Your messages   2.2k  ━━
    ■ Agent types       122   0.1%
  ❯ ■ Messages        29.8k  14.9%
    □ Free space     143.6k  71.8%
    ╱ Auto-compact      20k  10.0%

  Last request   38.1k in · 1.2k out
  Cache hit      ████████████████████░░░░  83%  31.6k read
  Since launch   14 requests · 402k in · 9.8k out · 79% cached

  ↑↓ select  ctrl+d raw context  esc close
──────────────────────────────────────────────────────────────────────────────
```

它是一页替换 composer 的内联页面 —— 与 `/donate`、`/export` 是同一家族
（`docs/ui/donate.md`、`docs/ui/export.md`）：同一套规则，同样的两栏内缩，
高亮行上的 `❯`，横幅渐变标题，没有文本输入因而没有硬件光标（它的座位跟随
`❯`），打开期间独占每一个按键，Esc 之后 composer 回来。它**在回合中途**也能
用：流式的条带把它那些行留在页面之上，跟任何一页之上一样。

为什么是一页而不是往 scrollback 里发一份报告（Claude Code 的 `/context` 就是往
记录里打一份）：报告是一张快照，而这个问题通常正是在窗口*正在被填*的时候问的 ——
回合中途，tool 输出正在落地。这一页是活的（见下文），可以探索，而且在对话里什么
都不留；备用屏幕（Ctrl+O、Ctrl+D、`/diff`）对一页整个内容只占四分之一终端的东西
来说是更重的选择。

### 横幅

标题戴着各页面唯一的那种标题装扮（`ui::header::page_title_line`）。副标题说出
屏幕上这段对话的模型和窗口的填充度：`{model} · ≈{total} of {window} tokens
({pct}%)`，每个计数都过 `ui::format_token_count` —— 页脚和收据用的那个人类化
函数。那个 `≈` 是刻意的（见下文*估算与报告*）。份额过 75% 转琥珀色，过
自动压缩阈值转红色，整行只有这一处颜色，所以一个马上要压缩的窗口在被读到之前
就先说了出来。那条规则是一个函数 `ui::theme::context_fill_color`，页脚的量表也
调它（`docs/ui/footer.md`），所以两者在「窗口有多满」上永远不会打架。窗口未知时
这一行读作 `{model} · ≈{total} tokens · window unknown`，而页面上每个百分比都
变成占总量的份额。

### 窗口条

一行，宽度是页面宽度减去两侧内缩，代表整个窗口：每个类别的格用各自的颜色，按
请求携带它们的顺序排（system prompt、tools、提醒的各个小节、messages），然后是
自由空间（`░`，暗淡），然后是**自动压缩的预留** —— 会话永不填满的最后 10%，
因为到 90% 时循环自己就会压缩（`docs/architecture/compact.md`）—— 用 `╱` 画
斜线。

每一段的格数都是它**自己**在窗口里的份额，四舍五入到最近的格（`allot_cells`），
所以一个类别的条长就是它的 token 值多少，不管它前面有什么，并随着 token 增长
一格一格地长。只要有 token 的类别至少拿一格，所以那 161 个 token 的 system
prompt 是一道看得见的小缝而不是什么都没有 —— 而那一格出自**自由空间**，自由
空间是类别和预留剩下的部分，绝不从邻居的条里扣。预留也是它自己的份额，所以斜线
从条的 90% 处开始，精确到格。给小缝的那些格是从代价最小的地方付的：已用的格
可以比副标题的份额每道小缝多读到一格，而副标题那个数才是该读的数。只有当自由
空间降到它自己的最后一格 —— 离阈值还差几格时 —— 最长的那一段才把格还回去。
已经越过阈值的对话会填满斜线格；越过窗口的会填满整条。窗口未知时，条就是总量
自己的比例，四舍五入由最长的那一段承担。

窗口条过去是把每段的**累积**边界四舍五入的，于是小缝那格是从它后面的条里扣的。
对话排在每一道小缝之后，所以它替所有小缝付账：在一个 1M 的窗口上，前面有六个
小类别的对话，它的条从第一条消息到接近 88k token 都只占一格 —— 一条图例在数着
涨、而条却纹丝不动的条。

### 图例与详情

窗口里真有的每个类别各一行 —— 没有 token 的类别略过，只有 **Messages** 例外，
它永远在（一段空对话也是值得陈述的事实）—— 然后在窗口已知时加 **Free space** 和
**Auto-compact**（后者只在 `/settings` 的 **Auto compact** 开关打开时）。每行
是一个该类别颜色的小方块（`■`，自由空间用 `□`，预留用 `╱`）、标签、token 数
和份额，列都对齐 —— 小到印不出十分位的份额读作 `<0.1%`，而不是一个会说该类
免费的 `0.0%`。颜色是主题角色（`ui::theme::context_usage_color`）：system
prompt 和 tools 沿用 Ctrl+D 视图自己的那两个，messages 用强调色，project docs
用链接蓝，skills 用成功绿，secrets 用错误红，而没有自己角色的三个类别是混色 ——
MCP tools 在警告与错误的色相之间，agent types 在紫色与红色之间，MCP
instructions 在绿色与蓝色之间 —— 所以每一行在每个主题下都保持可区分。终端自带
的十六色（`ansi`）没有可混的分量，而两个具名颜色的混色会倒向它的一端 —— MCP
tools 和 agent types 都落到 secrets 的红上 —— 所以在那里它们各自取第一个色相的
明亮变体（亮黄、亮洋红、亮绿；`palette::bright`）。

高亮行的**详情**在终端够宽放得下两者时坐在图例旁边
（`CONTEXT_USAGE_SIDE_BY_SIDE_MIN_COLS`），否则在它下面：该类别自己的分解，从
最大的开始，每部分一个标签、它的 token 数和一根以最大部分为基准的条，尾部折成
一行 `… N more` 带着它们之和。高亮开在**最大**的类别上，所以屏幕上第一眼就是
「我的上下文被谁占了？」的答案 —— 并且**跟随**最大的那个，直到用户移动它，
因为页面在打开时就被计数，而边界在它之后马上交过来的 tool 集合（一个稍后连上的
MCP server，一段在回合中途长过 tool schema 的对话）可能改变哪个类别最大。一旦
用户挑了一行，重数会把高亮留在那一行上；只有当某一行消失时才回到最大的那个。

| 类别 | 它数的是什么 | 它的分项 |
| --- | --- | --- |
| System prompt | `App::system_prompt` —— persona → environment → scratchpad（`docs/architecture/environment.md`） | 每个 markdown 标题块一个（`System prompt`、`Environment`、`Scratchpad`） |
| System tools | 下一次请求提供的每个内置 tool schema | 每个 tool 一个，按 wire 名字 |
| MCP tools | 每个 `mcp__server__tool` schema（`docs/tools/mcp.md`） | 每个 server 一个，tool 数写在标签里 |
| Project docs | 提醒里的 `AGENTS.md` 小节（`docs/state/project-doc.md`） | 每个文件一个，按路径 |
| Skills | 提醒里的 skills 小节（`docs/tools/skills.md`） | 每个列出的 skill 一个 |
| Agent types | 提醒里的 subagent 类型小节（`docs/agents/subagents.md`） | 每个类型一个 |
| Secrets | 提醒里的 secrets 小节（`docs/state/secrets.md`）—— 只有名字和上下文，绝不含值 | 每个 secret 一个 |
| MCP instructions | 提醒的最后一个小节：已连上的 MCP server 自己的 instructions（`docs/tools/mcp.md`） | 每个 server 一个，按它的 `## {server}` 标题 |
| Messages | 推导出的对话（`context::context_messages`，`docs/architecture/context.md`） | 你的消息、shell 命令、app 通知、压缩摘要、回复、tool call、tool 结果、图片 |

提醒的外壳 —— 那些标签和它一行前言，几个 token —— 计入它的第一个小节，所以提醒
的各个小节加起来等于提醒。**Free space** 和 **Auto-compact** 没有分项；它们的
详情说的是对话离阈值和窗口还有多远，以及那里跑的是什么。

每一行的 token 都来自构造请求所用的那个推导，绝不是它的一份平行模型：tool
schema 就是客户端发出去的那份 JSON，提醒就是推导出的上下文开头那一块，而
messages 就是 `context::context_messages` —— Ctrl+D 显示、下一次请求携带的
东西。所以这一页无法描述一个模型拿不到的上下文。tool 是按客户端保留它们的
chat-completions 形状计数的；一个会转换它们的 wire（Anthropic、Responses 和
Ollama 的请求，见 `docs/providers/claude.md`、`docs/providers/chatgpt.md`、
`docs/providers/ollama.md`）用自己那套框架发出同样的名字、描述和参数，token
差个几个。

提醒分两处切分。它的 instructions 小节是会话存下来的那段文本
（`App::user_instructions`），在提醒携带它的地方整块找到：一个 `AGENTS.md` 是
用户自己的文本，可能引用下面任何一个标题，所以它内部的任何标题都不能切它。
它之后的那些清单是会话自己渲染的，顺序固定，每一个都按它的贡献者所写的标题找到
（`skills::SKILL_LISTING_HEADER`、`subagents::AGENT_LISTING_HEADER`、
`secrets::SECRET_LISTING_HEADER`、`mcp::INSTRUCTIONS_HEADER`），从末尾找，
也就是会话写出它的地方。MCP 小节装的是 server 自己选定的文本，所以它的渲染把
它们每一个标题都降两级：它里面没有任何东西能像小节标题或 `## {server}` 分项那样
起一行。subagent 的 briefing 不含 instructions，所以它没有 **Project docs** 行。

**Messages** 的分项是按用户角色文本的来源而不是角色来切的，因为随用户消息一起
走的很多东西并不是用户打的：推导在每段文本落地时记下它的来源
（`context::context_messages_with_origins` —— 同样的消息，每段用户角色文本配一个
`TextSegment`，指明它的 `TextOrigin`），所以 **Your messages** 只是被打进去的，
**Shell commands** 是 `!` 记录，**App notices** 是 `[error]`/`[system]` 通知、
后台与 agent 通知和 hook 备注，**Summary** 是最新一次压缩的桥接，而一次图片读取
的附件备注计入它所属的 **Tool results**。

### 用量与缓存

图例之下，是 **provider** 报告的东西 —— 真实数字，从不估算
（`docs/providers/prompt-caching.md` *Real or estimated*）：

- **Last request** —— 最新一帧 usage 的输入和输出 token。
- **Cache hit** —— 那次请求的输入里由 prompt 缓存提供的份额，画成成功绿的量表，
  旁边是读取和写入的计数（为零时各自略掉）。不报告缓存账目的 provider
  （Ollama）读作 `0%`。
- **Since launch** —— 本进程发出过的每一次请求，包含 subagent 的：它们的次数、
  输入与输出合计，以及所有输入里属于缓存读取的份额。

第一帧之前（离线 dummy 从不会发一帧）这一节是一行暗淡的说明。Last request 描述
的是它为之发出的那段对话，所以任何替换那段对话的东西 —— 一次 `/clear`、一次
`/resume`、一次 Esc-Esc 回退、一次压缩（它最新的一帧是摘要器自己的请求）—— 也
把它带走，那一行就一直这么说，直到下一帧。这个账本是本会话的花费，它会保留每一
帧。

份额是整数百分比，从不会声称它并不是的「全」或「全无」：99.6% 的命中读作
`>99%`，0.3% 的读作 `<1%`。

## 估算与报告

各个类别是用 app 的 token 估算来数的，那是每一个活的统计都要过的接缝
（`docs/architecture/token-estimate.md`）：不依赖词表，在代码和英文上与常见
编码器相差几个百分点，而对 provider 在自己那一侧多加的东西（chat 模板的框架、
一张图片的真实代价、它自己对 tool schema 的渲染）是盲的。一次请求的真实大小由
provider 说了算，而它每轮说一次 —— 并且每一次都在教估算：这个 provider 的
tokenizer 怎么数本会话的文本。页面上的每个计数都过那一课
（`Calibration::correct`，在页面的备忘之外施加，并且算进它的签名，所以新的一课
会让它重数），所以这一页说的是 provider 的 token，和页脚的量表一样。两者仍然
分得清谁是谁：分解和条是估算（那个 `≈`），usage 一节是 provider 的账。页脚的
量表是 provider 对最后一次请求的计数加上对之后新加入内容的估算
（`docs/architecture/compact.md`），所以两者可能相差那份估算看不见的额外开销 ——
而页面的总量，与量表不同，即使第一条消息之前也把 system prompt、tools 和提醒
算进去，因为那正是第一次请求要花的。

## 活页

这一页从不滴答 —— 它上面没有任何东西在动 —— 但它随对话变化：一轮 tool 落地，
一个 MCP server 连上，一次回合开始时重扫了 skill。所以边界在打开时以及打开期间
每个循环底部都刷新它（`tui::context_usage`、`Session::sync_context_usage`）：
tool 集合从后端拉（`ReplySource::tool_definitions`，一个共享句柄，后端随它的
tool 集合重建，所以拉取的代价是一次指针拷贝），而纯核心只在它的**签名**动时才
重数 —— `(history generation, history length)`、正在查看的 agent 及其记录长度，
以及 system prompt、提醒输入和 tool 集合的哈希 —— 所以它从不逐帧计数，只在变化
时数。而一次重数是从页面的**备忘**（`context_usage::CountMemo`）里读每段文本的
token：上一趟数过的每段文本都按长度和哈希记着，所以只有新的东西过估算器，而一趟
不再数的东西会被忘掉，这把备忘限制在两个窗口的条目之内。正是这一点让忙碌的一轮
在页面开着时仍然顺滑：在一段 288k token 的对话上实测，整窗计数要 45 ms，而一次
提交之后的重数不到 1 ms，而未加备忘的重数在每次提交时都要卡 55 ms。打开页面要
付一次整窗计数的钱。usage 一节每次绘制都从 `App` 活读。

页面在它那些行上签出自己的视图流（`docs/ui/view-flow.md`），像每个按自己节奏
而不是逐帧变化的页面一样 —— 尽管它从不流：每个终端拿到的都是放得下的页面
（见下文）。

## 在 agent 会话视图里

这一页描述的是屏幕上的那段对话，也就是 `/copy`、`/export`、Ctrl+D 和页脚量表
早已遵循的规则（`docs/agents/agent-view-streaming.md`、
`docs/agents/agent-context-gauge.md`）：在 subagent 的会话视图里，它就是那个
agent 的窗口 —— 它自己的 system prompt（`App::agent_system_prompt`）、它那个
类型被提供的 tool 集合（`ReplySource::agent_tool_definitions`，与它启动时构建的
是同一套白名单，由后端保留到那个类型或它的白名单变动为止，因为页面在每个循环
底部都会问）、它的 briefing（`App::agent_system_reminder`）和它的记录 —— 对着
它运行的模型的窗口，配着它自己的 last request，以及一个取代会话账本的
**This agent** 账本。subagent 从不自动压缩，所以它的条没有斜线预留。

## 按键

| 按键           | 作用                                                           |
| -------------- | -------------------------------------------------------------- |
| ↑ / ↓          | 移动高亮，到两端回绕                                            |
| Home / End     | 第一行 / 最后一行                                               |
| Ctrl+D         | 原始上下文窗口（`docs/architecture/context.md`）—— 页面在它后面继续开着 |
| Ctrl+O         | 记录，同样如此                                                  |
| Esc / Ctrl+C   | 关闭 —— composer 回来                                           |

两个只读 overlay 会放行，理由与 permission prompt 相同：它们展示的正是这一页在
量的东西，而且它们只会滚动。从页面打开的 overlay 是一种「看一眼」，所以它的
Esc 回到页面，绝不武装 Esc-Esc 回退（`App::overlay_esc_backtracks`）：在那里
开始的一次回退会落回一页仍开在它所唤回的草稿之上的页面。其他一切都被吞掉，
粘贴也包括在内。

## 终端太矮放不下这一页

页面会构建它那些页面里第一个放得进它可能有的行数的
（`model_view::first_page_that_fits`，`docs/ui/view-flow.md` *A page that sheds
rows before it flows*），按这个顺序舍弃：详情的尾部（先裁到三部分，然后 ——
它坐在图例下面时 —— 整个去掉，而坐在图例旁边的详情不花行数，留着）、**Since
launch** 那一行、缓存量表（它的份额改挂在 **Last request** 那一行上）、条和
副标题一起 —— 标题折成一行，带上窗口的填充度，因为窗口有多满比条和模型名字
都活得久 —— 以及 usage 那一行。再往上，图例本身被开窗：三行跟着高亮在保留的
`↑`/`↓` 槽位之间滚动，位于标题行之下、提示之上，这是紧凑页面的形状
（`model_view::compact_page`，`/theme` 与 `/skills` picker 自己的那套），终端
够宽时详情在窗口旁边。那把梯子依次舍弃溢出槽位、上边框、高亮行之上的那些行、
下边框和提示，只留下标题行 —— 所以没有终端会矮到放不下一页，也不会有东西流进
scrollback。

```
──────────────────────────────────────────────────────────
  Context usage · ≈36.4k of 200k tokens (18.2%)
  ↑ 5 more above
    ■ Agent types       122   0.1%
  ❯ ■ Messages        29.8k  14.9%
    □ Free space     143.6k  71.8%
  ↓ 1 more below
  ↑↓ select  ctrl+d raw context  esc close
──────────────────────────────────────────────────────────
```

## 接缝

```rust
fn tool_definitions(&self) -> Arc<[ToolDefinition]>;              // the next request's tools
fn agent_tool_definitions(&self, agent_type: &str) -> Arc<[ToolDefinition]>;
```

一个 `ToolDefinition` 是一个 wire 名字，加上完全按客户端发送时序列化的 schema。
两者的默认值都是空 —— dummy 什么也不提供给它。`LlmBackend` 在客户端的 `tools`
数组旁边留着自己的一套，由重建那个数组的同一个函数重建（`sync_tool_specs`），
所以两者无法漂移；subagent 那套由它启动时用的同一个函数构建
（`subagent_tool_specs`，就在 `spawn_subagent_run` 旁边），并与它据以构建的类型
和白名单放在一起 —— 那是每回合重扫会就地改写的唯一输入。skills 的判定和 MCP
集合是后端构建时的那一份 —— 其中任何一个变动时边界都会重建后端
（`refresh_skills`、`refresh_mcp`），那会起一套新的 —— 所以和 lead 自己的那套
一样，agent 的那套描述的是后端当下的状态。

## API

- `context_usage`（纯）：`ToolDefinition`、`UsageLedger`、`Category`、`Part`、
  `Slice`、`Breakdown`、`Sources`（它的 `instructions` 和 `origins` 就是切出
  提醒和 messages 的东西）、`breakdown`、`CountMemo`。
- `app`：`ContextUsagePage`、`ContextUsageRow`；`App::open_context_usage`、
  `close_context_usage`、`refresh_context_usage`、`set_tool_definitions`、
  `set_agent_tool_definitions`、`last_usage`、`usage_since_launch`；
  `CommandEffect::Context`、`Action::OpenContextUsage` /
  `CloseContextUsage`。`AgentRun` 增加 `last_usage` 和 `usage`。
- `ui::context_usage_view`：`context_usage_lines`（页面构建器；它的长度就是预留
  高度）、`context_usage_height`、`render_context_usage`。
- `context::context_messages_with_origins`、`TextOrigin`、`TextSegment` ——
  推导出的对话，每段用户角色文本的来源都标了出来，所以页面能把打进去的与 app
  说的话分开数。
- `tui::context_usage` —— `Session::sync_context_usage`，在打开时和循环底部跑的
  那次刷新。

## 测试

- `context_usage` —— 每个类别在一套 fixture 的 prompt、tool 集合、提醒和历史
  上的归属（计数器是注入的，所以一个 token 就是一个字节，每个和都能手算核对）；
  提醒的 instructions 不管怎么引用标题都按存下来的计，它的各清单按标题切开，
  它的外壳与第一个小节合算；system prompt 在它代码围栏之外的标题处切开；MCP
  tool 按 server 分组；messages 按种类切，它们的用户角色文本按来源切；账本的和
  与命中率；备忘在多次遍历之间每段文本只数一次、忘掉某一趟跳过的内容，并且从不
  改掉一个计数。
- `context` —— 每段用户角色文本的来源在它落地处标出，包含压缩的文本和它的桥接，
  在消息上与该 `context_messages` 完全一致。
- `llm::backend` —— subagent 的 tool 集合构建一次，直到它的白名单变动。
- `app/tests/context_usage.rs` —— 配色行，命令在空闲时、回合中途和 agent 视图里
  打开页面，最大的类别开着高亮并且高亮跟随它直到用户移动它，按键语法（回绕、
  Home/End、Esc/Ctrl+C、overlay 放行、独占每一个键），压在页面上的 overlay 绝不
  武装回退，重数在变化时发生而且只在那时，账本由 lead 与 agent 的帧喂入，last
  request 随被 `/clear`、`/resume`、回退或压缩替换掉的对话一起丢弃，以及用户的
  份额把不是他们打的东西也点名。
- `ui/tests/context_usage_view.rs` —— 带框的页面及其贴合的边框，条的分格（每段
  都是自己的份额，不管它前面有多少道小缝，自由空间为它们的格付账，对话的条一次
  长一格且从不缩回，每个类别都可见，斜线预留，越过阈值的窗口），图例的列和小
  方块，图例旁边与下面的详情，usage 各行与量表，不满整数的份额绝不读成全或全无，
  未知窗口，每个类别在每个主题下都是自己的颜色，梯子在每个高度上一直到开窗的
  图例和只剩标题行，宽度安全到两列，任何高度都不流，隐藏的光标在每一页都坐在
  `❯` 上。
- `scripts/smoke.sh` 第 144 阶段 —— 在真实终端里端到端跑这一页：对一个
  OpenAI 兼容的 stub，种下一个 skill、一个 `AGENTS.md` 和一个脚本化的 MCP
  server（每个类别都上图例，MCP server 的 tool 落到已经开着的页面上，↓ 走遍
  每份详情，Ctrl+D 压上页面再回来，两轮之后 provider 的进/出和 86% 的缓存命中，
  一段很长的第三个回答让对话的条在窗口条上长到它自己的份额 —— 它的强调色格是
  从原始 SGR 字节里读出来的），然后是回合中途的 dummy（页面之上的状态行，对话
  随着回合在它下面流式而重数）。
