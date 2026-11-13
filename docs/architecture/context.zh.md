# 对话上下文、视觉附件与 Ctrl+D 调试视图

三个特性共用一个数据模型，所以它们共用一篇文档：

1. **按 session 的 LLM 对话上下文**——真后端现在每个回合都发送*整段*对话，
   于是模型永不丢失上下文。
2. **真正的视觉图片粘贴**——一张 Ctrl+V 粘贴的图片以嵌入的 base64 `data:` URL
   （OpenAI 的多模态 parts 形式）到达模型，而不是一句文本道歉。
3. **Ctrl+D 上下文调试视图**——一个全屏 overlay（Ctrl+O pager 的兄弟），显示
   原始上下文窗口：正是发给模型的东西——图片占位符原样，每个 tool call/结果都
   作为它自己的原生 `assistant` 请求 + `tool` 结果条目。

> 有一个 provider 是「正是发给模型的东西」的例外：在一次 ChatGPT 登录下，请求
> 在最后一刻被翻译成 **Responses** 格式（`docs/providers/chatgpt.md`），所以
> Ctrl+D 显示的是那一步*之前*推导出的对话。它仍是这个 session 所持有的对话、
> 也是 `/resume` 所恢复的东西——`input` 数组是它的一种渲染，不是第二个事实
> 来源。那次翻译还依赖这条推导的一个性质：一个 tool call 和它的结果总是成对
> 发出，绝不出现一个有而另一个没有。

## 上下文是*推导*出来的，不是存下来的

其余一切都由此而来的设计决定：对话**没有第二个存储**。纯的
[`context`](../../src/context.rs) 模块按需从 `App::history` 推导上下文窗口——
`context_messages(&app.history) -> Vec<ContextMessage>`，或者在 session 的
`<system-reminder>` 领头时用
`context_messages_full(instructions, listings, &app.history)`——那一个块
（`reminder::reminder_message`，`docs/state/project-doc.md`）是在*推导时*由
`App` 持有的两个小节拼成的：项目的 AGENTS.md 说明（`App::user_instructions`、
`project_doc::instructions_section`），然后是它能加载的 skills、它能启动的
subagent 类型，以及它的工具能以占位符使用的 secrets（`App::listings`、
`subagents::listing_sections` + `secrets::secret_section`、
`docs/tools/skills.md`、`docs/agents/subagents.md`、`docs/state/secrets.md`）；
`context_messages_with(instructions,
&app.history)` 是只有 instructions 的情形，即不启用工具的 `/compact` 回合所发
的，而 `context_messages_behind(fragment, &app.history)` 把一个已经包好的块原样
接上——一个被查看的 subagent 的 briefing。那两个空检查刻意*不带*那个块去推导，
于是一个单独的常驻指南仍然是「没有可压缩的东西」。之所以推导，是因为 `history`
已经是 TUI 在每个要紧处都维护正确的那个唯一事实来源：

- `/clear` 抹掉它（循环同时杀掉在飞的 backend）；
- Esc-Esc 的 backtrack 回退截断它；
- 一次 Esc 中断的*撤销*把提交从里面弹出来；
- 一次 `/resume` 加载用解析出的 rollout 文件替换它。

reminder 坐在 history 的**前面**而不在里面，它的小节顺序固定（先 instructions，
再 skills，再 agent 类型，再 secrets）：每个小节都按回合重新渲染，所以一个挪了
位置的小节会让它后面的 prompt 缓存失效，而把块留在 `history` 之外，意味着一次
backtrack 不能回退过它，记录器也不能把它存两遍。它在上下文推导的地方组装，而
不是组装好存起来，因为它的两个输入在不同时刻变化——instructions 在每个回合
开始时和 **Project docs** 开关上，listings 在每次重扫和 `/model` 切换时——在
任一处组装好的块，在另一处就已经陈旧。

一个存下来的上下文必须镜像上述每一个转换，并且在第一个漏掉的转换上漂移。推导
出来的上下文*无法*漂移：下一个回合的请求按 history 当时所说的重建，而 Ctrl+D
视图渲染同一处推导，所以你看到的就是发出去的。

每个 session 的上下文通过 `/resume` 的 rollout 文件持久化（记录器已经按 session
把 `history` 镜像到磁盘——`docs/state/resume.md`）：把一个 session 加载回来就
一步恢复了它的 history，因而也恢复了它的上下文。

### 映射（`context::context_messages`）

| 历史条目 | 上下文条目 |
| --- | --- |
| `Message(User)` | `user`，文本逐字（含占位符）+ 它的图片路径 |
| `Message(Assistant)` | `assistant`，文本逐字 |
| `Tool`（backend） | 一条 assistant `tool_calls` 条目（原生 `{id, name, arguments}`——模型**逐字**的参数），折进前面的 assistant 段，然后一条 `tool` 角色结果，携带 `ToolCall::context_text()`——即**面向模型的**文本 |
| `Tool`（`!` shell） | `user`，一段 `$ {command}\n{output}` transcript（这是用户本地跑的） |
| `Message(Shell)` | 跳过——它上面的 tool 单元格已携带命令与输出 |
| `Message(Error)` | `user`，`[error] {text}`（中断与后端失败） |
| `Message(System)` | `user`，`[system] {text}`（斜杠命令通知） |
| `Summary` | 跳过——`Done for Ns` 是 TUI 装饰，不是对话 |
| `Reasoning` | 跳过——Chat Completions 没有地方放上一轮的原始思维链，重发它只会白白烧上下文（`docs/ui/thinking-stream.md`） |

模型的一个 tool call 映射成 **provider 原生**的 Chat Completions 形状——live
agent 循环在一个回合内本就流出的那个精确协议（`docs/tools/tools.md`），现在跨
回合重放：一条带 `tool_calls` 数组的 `assistant` 消息，紧接着每个调用一条
`tool` 角色消息。调用 **id 是 provider 自己的**，而一轮的调用**按它们到达时的
同一条消息**重放：agent 循环先宣告每一轮 tool 的全部 id
（`StreamEvent::RoundCalls`，按模型的顺序，每个带模型逐字的参数——一个从未到达
自己 `ToolStart` 的调用、一个被 Esc 解决的 `⎿ Waiting…` 兄弟，就用这个记录，
`docs/input/interrupt.md`），循环把它们盖到该轮单元格解决成的记录上——
`ToolCall::call_id`、`TaskCallRecord::call_id`、一个 agent 组条目的 `call_id`
及其逐字 `arguments`——在一个按轮的 `batch` 编号和每个调用在该轮里的 `position`
之下，rollout 把这一切往返，而推导把共享同一 batch 的记录折进一个 `tool_calls`
数组，**按模型的顺序**（一个 subagent 组在该轮普通调用跑之前就解决了，所以它的
记录无论模型按什么顺序发起调用都先落地；position 把它放回去）：一个并行批次、
它的 task 调用和它的 subagent 启动在同一条 assistant 消息上，每个结果按同样顺序
排在它后面，一次图片读取的附件注记排在结果之后，而一条在批次中途提交的通知
（一个后台 shell 的完成、一个 subagent 的注记）被推迟到批次之后，因为一个
user 角色消息不能坐在一个调用和它的结果之间。于是一个后续回合推导出的请求，就是
provider 缓存过的那个——在 `/resume` 之后、backend 重建（一次 `/settings` 或
`/model` 切换）之后、以及重启之后都如此，这正是让 prompt 缓存在这些之间保持温热
的原因（`docs/providers/prompt-caching.md`）。一条不带 id 的记录——一个在字段
之前写下的 rollout、`!` shell、一个孤立的脚本化调用——拿到一个合成 `call_N`，
避开每一个记录过的 id，于是配对在请求内保持一致；它自成一轮。重放的
`arguments` 是模型自己的，**逐字**：除了一行抬头摘要（`● Write(a.py)`），每个
调用都记录它当初所用的原始 JSON（`ToolCall::arguments`，搭在
`StreamEvent::ToolStart` 上，经 rollout 往返），而 `reconstruct_arguments` 在它
能解析成对象时返回它。

那以前是从摘要*重建*——`bash` 用 `{"command": …}`、文件工具用
`{"path": …}`——这以一种要紧的方式有损：一次 `write` 重放成
`write({"path": "a.py"})`，模型看着自己创建一个没有内容的文件，而整段内容却搭在
tool **结果**上作为一个带编号的 body 每个回合重新上传。记录参数让重放无损
（一次 `write` 的 `content`、一次 `edit` 的两个字符串和 `replace_all`、一次
`bash` 调用的 `timeout`），也正是让文件工具的结果能塌成一行
（`docs/tools/tools.md`）的原因：改动现在在*调用*上，而不是在结果里。重建作为
fallback 留给不带参数的记录——每一个在该字段之前写下的 rollout、`!` shell、一个
手写脚本的事件——以及偶尔损坏的记录，因为会校验的 provider 拒绝不是 JSON
对象的参数。

一次**图片 `read`**（从存下的记录里认出：`name == "Read"` + `Read image ` 输出
标记）还会重放 live 循环所附的后续 user 注记——`llm::tools::image_attachment_note`
压在作为 `images` 附件的路径上——好让后续回合继续*看见*这张图片，从 session
那唯一一份共享编码里供给，像一次 Ctrl+V 粘贴一样（`docs/tools/tools.md`、
`docs/architecture/memory.md`）。

### 模型读到的结果，而不是它看到的单元格

一个工具的结果是 `ToolCall::context_text()`：调用记录过 `context_output` 就用
它，否则用它显示的 `output`。对每个普通调用两者相同——但一次**权限拒绝**
（`docs/state/permissions.md`）和一次 `write`/`edit`（`docs/tools/tools.md`）
都刻意各自解析出两段文本。红色单元格读作

```
⎿ User rejected write to hello.py
  Instructions: use pathlib instead
```

而模型被交给的是完整的 stop-and-wait 说明，末尾附上同样的反馈。重放单元格的
文本，会交给后续回合一个与 live 那一轮所发*不同*的工具结果——把用户的说明从
对话里整个丢掉，而那是它们被给出的一个回合之后。两段都存，重放才是诚实的：
Ctrl+D 显示的、下一个请求所携带的，都正是模型被告知的。`write`/`edit` 的分裂
出于相反的理由——不是为了留住什么，而是为了丢掉什么：单元格保留带编号的内容或
diff 块，而模型读到

```
File created successfully at: /tmp/a.py (file state is current in your context — no need to read it back)
```

它之所以诚实，只因为它上面的参数逐字重放。（`Backgrounded` 的分裂走另一边：
`output` 持有面向模型的启动文本，而*单元格*那一行是由状态合成的，所以
`context_output` 在那里保持 `None`。）

相邻同角色的**纯文本**条目仍然**合并**（文本用空行连接，附件串联），好让消息
批次和通知串塌成一条；带 tool call 的 assistant 消息和 tool 结果消息从不是合并
对象（一个结果必须坐在它们之间）。把 TUI 通知映射成 *user* 角色的方括号注记
而不是对话中途的 `system` 消息，是刻意的 wire 兼容性（严格的 provider 拒绝
非领头的 system 消息）；原生的 tool 往返被任何支持 function calling 的 provider
接受，而那恰恰是会发出 tool call 的那批。一条 `!` shell 命令以一条 `user`
`$ command` transcript 回来——它是*用户的*本地动作，不是模型的 tool call，所以
它没有 assistant `tool_calls` 条目去配对一条原生 `tool` 消息。

## 接缝

`ReplySource::spawn` 增加一个 `context: Vec<ContextMessage>` 参数：

```rust
fn spawn(&self, prompt: String, images: Vec<PathBuf>,
         context: Vec<ContextMessage>, tx: …, cancel: …) -> JoinHandle<()>;
fn system_prompt(&self) -> Option<String> { None }  // for the Ctrl+D view
```

`tui::turn::Session::start_turn` 先把本回合的 user 消息记进 history（一如既往），
然后推导上下文——所以它的**最后一条是当前这条 user 消息**，文本和附件都在——
并把它交给后端。`prompt`/`images` 仍为需要它们的后端而传：`DummyAi` 完全忽略
上下文（罐头式答复；smoke.sh 未被动过），而 `LlmBackend` 只从上下文构造它的
请求（只在上下文万一为空时才退回光秃秃的 `prompt`，于是一个请求永远不会没有
user）。

`llm::backend::build_messages(system_prompt, prompt, context, encode_image)`
是纯的组装：可选的 system prompt，然后每个上下文条目一条 wire 消息。它对
`encode_image: Fn(&Path) -> Option<String>`——那个读文件的接缝——泛型，所以它
用一个假编码器、不碰磁盘就能单元测试。

## 视觉：图片在 wire 上

`app::Message` 增加 `images: Vec<PathBuf>`——一条 user 消息*拥有*它的 Ctrl+V
附件（`[Image #N]` 占位符留在记录下的文本里；路径陪在旁边——而**这条推导**给
每个占位符注上它的路径，`[Image #N: {path}]`，好让模型知道图片存在哪里，也让
Ctrl+D 显示的文本与 wire 所携带的相同：
`paste::annotate_image_placeholders`，在合并之前逐消息跑，于是一个批次的每份
草稿各自点名自己的图片——`docs/input/image-paste.md`）。提交路径把整个
`(placeholder, path)` 对暂存起来，并按文本出现顺序把每个路径记到文本携带它占位
符的那条消息上（`paste::distribute_images`——于是一个合并批次里重复的
`[Image #1]` 各自解析到它们自己草稿的路径，而草稿内的重排也忠实记录）。session
文件往返这些路径（消息记录上一个新的可选 `images` 字段——为空时省略，所以旧
文件和无图片的行保持旧形状；写成有损字符串，于是一个非 UTF-8 路径不会让记录器
panic）。中断撤销**和** Esc-Esc 的 backtrack 回退都把附件还回 composer：按每条
消息的记录路径重新拉链其占位符出现处
（`paste::image_placeholder_occurrences`），先丢掉任何支撑被覆盖草稿的对，并且
——对一次 backtrack——把被丢掉的*后续* user 消息所遗留的附件排进删除队列。

`ChatMessage.content` 现在 是 `MessageContent::Text(String) |
Parts(Vec<ContentPart>)`（serde-`untagged`，于是无图片消息保持经典的
`"content": "…"` 字符串，每个 OpenAI 兼容端点都接受）。一条带图片的上下文消息
变成 parts 数组：一个 `text` part，然后每个附件一个 `image_url` part，每个都是
`data:image/{png,jpeg,gif,webp};base64,…` URL（`image_data_url`，复用
`clipboard` 那个有测试的 RFC 4648 编码器；MIME 按扩展名，默认 PNG——剪贴板写入
器只产出那几种格式）。编码发生在**后端线程**上，从不在事件循环上。一个文件已经
消失的附件（有人清空的粘贴目录——粘贴现在住在 config home 下，所以操作系统的
临时清理器不再拿走它们）被*注在文本里*（`[image unavailable: {path}]`），而不是
悄悄丢掉。

因为过去的 user 消息保留它们的路径，早先回合的图片**在每个后续请求上重发**
——模型可以在三个回合之后被追问一张图片（codex 以同样方式重发）。这也是提交过的
粘贴被刻意留在 session 的粘贴目录里的原因（`docs/input/image-paste.md`）。

## Ctrl+D 上下文调试视图

第三个 alternate-screen overlay，`View::ContextDebug`，带着 Ctrl+O pager 的装饰：
斜杠铺开的 `/ C O N T E X T` 标题行、一个带 vi 风格 `~` 填充的可滚动 body、那条
携带滚动百分比（并在左端带上窗口摘要）的 `─` 分隔线，以及两行 dim 按键提示
（`↑/↓`、`←/→` 在消息之间、pgup/pgdn、home/end；q/esc/ctrl+d 关闭）。它从对话里
用 **Ctrl+D** 打开——回合中途也行，像 Ctrl+O——并在 Ctrl+O 无效的地方无效
（这些 overlay 共用 alt screen，从不叠加：Ctrl+D 在 pager 或 `/resume` picker
下什么都不做，Ctrl+O 在 Ctrl+D 下也什么都不做）。滚动是它自己的状态
（`App::debug_scroll`/`debug_follow`，像 pager 的一样每次绘制沉降），所以在
overlay 之间切换永远不会冲掉 transcript 的位置。

body（`ui::context_body`，只有它的行是 `ui::context_lines`）是原始上下文窗口，
每条请求携带的消息一个条目：

```
/ C O N T E X T / / / / / / / / / / / / / / / / / / / / / / / / / / / / / / / /
 SYSTEM  #1 · ≈1.8k tokens            (amber chip — the backend's prompt)
  ⎿  # System prompt
     You are Zeroth, an autonomous agent harness …
     ## Environment · Date … OS … User … CWD …           (docs/architecture/environment.md)

 USER  #2 · ≈40 tokens · 1 image      (blue chip — context_user_color(), the one
                                       blue the running bullet left behind)
  ⎿  [Image #1: /home/me/.zeroth/image-cache/773c1c6cb321/1.png] what's in
     this picture?
     image: /home/me/.zeroth/image-cache/773c1c6cb321/1.png  (dim attachment)

 ASSISTANT  #3 · ≈25 tokens · 1 tool call                       (green chip)
  ⎿  Let me look at the file.
     → read({"path":"src/main.rs"})  call_1      (the arrow and name purple,
                                                  bold; the call's id dim)

 TOOL  #4 · read · call_1 · ≈310 tokens   (purple chip — names the call it answers)
  ⎿  1 fn main() { … }

─ 4 messages · ≈2.2k tokens ─────────────────────────────────────────── 100% ─
 ↑/↓ to scroll   ←/→ prev/next message   pgup/pgdn to page   home/end to jump
 q/esc/ctrl+d to quit   tab for classifier context
```

——先是 system prompt（在边界处经 `App::set_system_prompt` 从
`ReplySource::system_prompt()` 注入，在启动时和每次 `/model` 切换时；dummy 没有
——真后端的是人格，然后是日期/os/用户/cwd 的运行时**环境上下文**：人格 →
环境，`docs/architecture/environment.md`），然后是每一条推导出的上下文消息。每个
条目有两部分：

- **抬头——一个角色 chip 和它的事实。** wire 角色大写（`SYSTEM`、`USER`、
  `ASSISTANT`、`TOOL`），填着该角色的色相、用主题的 on-accent 墨色，粗体——
  即 agent label 与 ask 倒计时的装扮（`context_chip_fg`）。它取代的小写 `user:`
  标签读起来像它们下面文本的一个词，而在一页原始文本上，抬头才是视线搜寻的
  那一行。chip 旁边，dim：条目在请求里的 `#n` 位次、**一个结果所回答的那个
  调用**——它的工具，经 `tool_call_id` 从它上面的调用里解析出来，然后是那个 id，
  即调用自己的行所收尾的东西，于是一个并行批次的结果用眼就能与它们的调用配
  上（一个光秃秃的结果以前对它是哪个调用只字不提）——它大小的一个**token
  估算**，像 `/context` 页的一样标 `≈`，以及它携带多少个 tool call 或多少张
  图片。估算用的是页脚量表对单条消息的同一条规则（`app::message_tokens`，
  `estimate_messages_tokens` 对它求和）：文本、每张图一个固定计费、每个调用的
  名字与参数——于是各条目和底边线的总数加起来，就是量表离线所读到的数。抬头
  永远是一行，在宽度处用 `…` 截断。
- **body，挂在 tool 单元格的 `⎿` 栏上**（`TOOL_RESULT_PREFIX`，经
  `file_cell::gutter_row_spans`）：第一行是那个角，其余每行对齐在它下面。文本
  **按词换行并保留自己的间距**（`wrap_output`，绝不用 markdown 渲染器——重点
  就是未格式化的 wire 内容；它取代的那种词中硬断把 `scripted` 变成 `a s` /
  `cripted`），tab 展开，尾部空行丢掉（一个模型在 tool call 前的 `…\n\n` 会在
  文本和它所引出的调用之间堆出空行），而 `<system-reminder>` 标记被压暗，好让
  注入的块读起来与它领头的消息分开。assistant 条目的原生 tool call 随后以
  `→ name(arguments)` 行出现——箭头和名字紫色粗体，参数用 body 的墨色，在
  `→ name` 引领之下悬挂换行（tool 抬头的规则；一个窄到放不下引领的终端，会
  把整个调用逐字换行），以调用的 dim id 收尾，放在它最后一行旁边，或者当两者
  放不下时自占一行——而附件路径在下面 dim，逐字换行，因为一条路径没有值得断行
  的空格。一个什么都没有的条目说 `(empty)`：一个空结果也上了 wire。在栏的宽度
  再加上一个宽字形的位置之下，栏把自己的列还回来。

图片占位符、原始 tool-call JSON 和 tool 结果只在这里、且仅在这里原样出现。

**找到你的路。** 一个窗口是几十条消息、几千行，所以这个视图带三种辅助，全由
`DebugBody::headers` 喂——每个条目抬头所在的那一行，在行旁边构造：

- **←/→ 在消息之间步进**，把上一条或下一条打开在 body 顶部。按键只记录方向
  （`App::debug_jump`，一个 `EntryJump`）：相邻条目从哪里开始是构造出的那一页的
  事实——它的宽度、它的换行——所以绘制时对着它即将绘出的那些行解析
  （`App::resolve_debug_jump`，在滚动沉降之前），绝不是对着上一帧的。从一个
  条目内部按 ← 先打开该条目自己的开头，像段落移动那样；过了最后一条 → 是末尾，
  在第一条之前 ← 是顶部；窗口的顶行在尾随（tail-follow）时是页面当前的末尾，
  于是一个自上次绘制以来长大的页面从它新的底部开始步进；而一次落在最后
  满屏之内的步进会重新接上尾随，像 End 一样。Tab 和一次关闭会丢掉仍待处理的
  一步，所以它绝不会落在它未被按下时的那个页上。它们正是 transcript pager 的
  backtrack 用于上一条和下一条消息的键，在这里意思相同。
- **窗口从中打开的那个条目被钉住**在顶行之上，当它自己的抬头被卷出视野时
  （`pinned_header`）：深入一段长 system prompt 或 tool 结果时，顶行仍然说明
  这是谁的文字。不在抬头行本身上（它已经在顶了），不在只剩该条目的空行会坐在
  钉之下时（一个抬头叠在下一个上），也不在比 `CONTEXT_PIN_MIN_ROWS` 更短的
  body 里，那里钉会吃掉它所点名的文字。
- **底边线统计窗口**：`5 messages · ≈1.2k tokens` 嵌进它的左端
  （`transcript::labeled_separator`），百分比保留自己的位置——当那条线放不下
  两者时整个让位。

视图升起时，循环继续排空流事件（它们的提交在 viewport 上排队，不变量 4），而
尾随让最新的条目留在视野里；关闭时把排队的提交 flush 掉并重绘 live 区域，完全
像一次 Ctrl+O 返回。`?` 快捷键带新增 `ctrl+d for llm context`。

这个视图显示的是*已完成*条目的上下文：一个在飞的半截答复住在流式缓冲区里，
当它那一段落进 history 时才进入窗口（以及下一个请求）。

### 窗口被缓存（`ui::ContextCache`）

推导窗口（对整个 history 跑 `context_messages_full`）并换行每个条目是
**O(对话)**——而回合运行期间视图在每一动画帧都重绘（循环的 32 ms 状态重置），
外加每次滚动键一次。逐帧重建，一个大的上下文——一个被加载 skill 的整个渲染
body 都搭在窗口上——会把事件循环顶满并饿死滚动键：被报上来的「Ctrl+D 冻结
TUI」bug，最糟的正是用户打开视图去读一个长 skill 注入了什么的时候。所以循环
拥有一个 `ui::ContextCache`（`TranscriptCache` 的小兄弟，
`docs/
tool-view-performance.md`）：`draw_context_view` 向它要构造好的页（滚动夹取，
`tool_view_max_scroll_for`——pager 的公式，共用，而 ←/→ 步进也对着渲染所绘的
同一次构造解析），而缓存**只在它的签名变化时**重建——history 的 generation +
长度、宽度、被查看的 agent（id + 它自己的 transcript 长度），以及领头片段的
长度（system/agent prompt、`AGENTS.md` 的 instructions 小节、listing 各小节、
一个被查看 agent 的 briefing——它们本来只在一次回合开始的 history 追加旁边变化，
而长度能抓到直接编辑：一个 `/settings` 开关去掉 instructions、一次 `/model`
切换换掉 prompt）。一次滚动键或一次状态 tick 是缓存命中（开窗代价 O(viewport)）；
一个流出的 chunk 根本不使它失效（窗口只显示已完成的条目）。不需要增量前缀——
与 transcript 不同，窗口只在条目边界变化，从不逐 chunk。

抬头的 token 估算让每段文本都过一遍估算器，而在长对话上这是构造里最贵的部分
——所以缓存在重建之间保留一个 `context_usage::CountMemo`（`/context` 页自己的
那个），关闭时也保留，因为它是计数而永不是文本：为一条新消息重建时只对那条消息
分词，其他每个计数都从 memo 供给
（`context_cache_counts_only_what_is_new`）。

在一个 **agent session view** 里（`docs/agents/agent-tool.md`），同一个 overlay
改为调试*被查看 agent 的*上下文：body 经同一套映射从那个 agent 自己的
transcript 推导，而 `SYSTEM` 条目显示的是一个 subagent **实际被发到的** prompt
——主 prompt 附上 subagent 注记（`prompts/subagent.md`），作为
`ReplySource::agent_system_prompt()` 浮出并在主的那份旁边注入
（`App::set_agent_system_prompt`）——没有 AGENTS.md 小节，因为 subagent 的对话
是全新开始的、不带它（`llm::backend` 的 `run_agent_calls`），并且带着 agent
自己的 briefing——单独包好的 skills 小节，正是启动时所发的样子——逐字放在它的
transcript 前面（`context_messages_behind`、`docs/agents/subagents.md`）。于是
那条注记的存在，恰好在用户找它的地方可验证。而标题说明这是谁的窗口——
`C O N T E X T · explore agent · Fetch the
weather`，agent 按它名单行的说法被点名——因为页上别的东西都分不清 lead 的窗口
和一个 agent 的。`overlay_header` 按显示列而不是字符来截这个标题（一段描述是
模型写下的任何东西，中日韩文在内，而在 `width` 个字符处截断可能宽达这一行的两
倍），并让它的斜杠铺开以一个空白开头，好让铺开永不把一个 `/` 粘在一个以另一
奇偶结尾的标题上。

## 顺手修好：默认 system prompt 曾经是死的

`build_backend` 把 `ZEROTH_SYSTEM_PROMPT` 的环境读取原样直传，于是在该
变量未设置时，真后端拿到的是**没有** system prompt——`DEFAULT_SYSTEM_PROMPT`
不可达，与 `docs/providers/llm.md`（「Override with…」）相矛盾。现在是：未设置
→ 默认；设置了 → 覆盖值；设成空 → 完全没有 system prompt（`with_system_prompt`
丢掉空白）。运行时环境上下文（日期/os/用户/cwd）会折到任何**非空**基础之上
——默认的或覆盖的——所以「空 → 无 prompt」的契约仍然成立
（`docs/architecture/environment.md`）。

## 已知限制（v1）

- 状态行的 `↑` token 估算仍只计新回合的输入，不计重发的历史（这个计数一直是
  估算）。
- 没有上下文窗口**上限**：一个很长的 session 会发送它的整段历史，直到撞上
  provider 的限制（provider 的错误像任何别的错误一样在带内浮现）。裁剪/摘要是
  未来的工作。
- session 文件记录附件的*路径*，不是字节：在操作系统临时清理器跑过之后恢复，
  发送的是 `[image unavailable]` 注记而不是图片。
- undo/backtrack 的附件重新配键，是把一条消息的占位符出现处拉到它的记录路径上
  拉链；同一消息里一个*手打*的占位符（从未由附件支撑）会让那个配对错位——这是
  字符串键方案的已知边缘（`docs/input/paste.md`）。
- ~~一次重放的 tool call 的 `arguments` 是从 history 的一行摘要重建的~~——
  **已修复**：每个调用记录模型逐字的参数并重放它们（见上）。重建只作为在该字段
  之前写下的 rollout 的 fallback 存活，那里这条限制仍照字面成立。
- 重放原生 `tool_calls`/`tool` 消息假设 provider 支持 function calling（即会
  发出 tool call 的那批 provider）。用 `ZEROTH_TOOLS=0` 恢复一个开了工具的
  session，会把 tool 消息重放给一个不声明任何工具的请求——一个严格的 provider
  可能拒绝的狭窄边缘。
