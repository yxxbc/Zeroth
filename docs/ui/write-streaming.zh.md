# 在模型写的时候就流式显示一个 `Write`

一次 `write` 调用的 `content` 通常是模型在一个回合里生成的最大的一件东西：一个
三百行的文件要流几十秒。在这次改动之前，流式期间它一个字都到不了屏幕。backend
为状态行的 token 统计数了那些 tool call 碎片（`docs/ui/status-indicator.md`），
别的什么都没做。`● Write(…)` 单元格只在整轮结束之后才出现，所以那半分钟里用户
看着 `↓ 4.1k tokens` 往上涨，却看不到在写什么。

现在模型一点名这个调用，单元格就立起来，而文件在模型写它的过程中出现在里面：

```
While the content streams:                    Once it has all arrived:

● Write(fizzbuzz.py)                          ● Write(fizzbuzz.py)
  ⎿  Writing 17 lines…                          ⎿  Writing 17 lines…
      8             print("Fizz")                   1 #!/usr/bin/env python3
      9         elif i % 5 == 0:                    2 """Print FizzBuzz for the first n numbers."""
     10             print("Buzz")                   3
     11         else:                               4
     12             print(i)                        5 def fizzbuzz(n: int) -> None:
     13                                             6     for i in range(1, n + 1):
     14                                             7         if i % 3 == 0:
     15 if __name__ == "__main__":                  8             print("Fizz")
     16     fizzbuzz(15)                            9         elif i % 5 == 0:
     17 # the line still arriving                  10             print("Buzz")
                                                    … +7 lines (ctrl+o to expand)
```

内容流式写入期间，单元格显示**最后**十行，所以最新的一行永远是底行，窗口随文件
增长向下移动。拐角行在行到达时计数。当内容的收尾引号到达，单元格改为显示**头**
十行，带 `… +N lines (ctrl+o to expand)` 提示。那就是已提交单元格所显示的同一
个一瞥，所以这个调用解决时单元格不会再变一次形状。只有拐角行（排队时
`Waiting…`，然后 `Wrote 17 lines to fizzbuzz.py`）和圆点的颜色会动。Ctrl+O
显示整个文件，仍在流式，与其他所有实时内容一样。

## 线上格式：一个碎片属于哪个调用

每一种线上格式都是分片地流一个调用的，而每一个本来就知道一个碎片属于哪个调用。
那个身份在接缝处被丢掉了：`Delta` 每帧携带一个不透明的
`tool_call: String`，即帧里每个调用拼接起来的名字和参数文本，而
`StreamEvent::ToolCallDelta(String)` 把那个字符串送到一个只能数它的循环。

现在碎片是一个类型，`stream::ToolCallFragment { index, name, arguments }`，
同时出现在 `Delta::tool_calls` 和 `StreamEvent::ToolCallDelta` 上。`index` 是
线上格式自己的调用槽位：

| 线上格式 | 槽位从哪来 | 名字从哪来 |
|---|---|---|
| Chat Completions | `ToolCallAccumulator::push` 把这个 delta 解析进去的那个条目（它已经把同 index 而不同 id 的调用拆开了） | 第一个 delta 的 `function.name` |
| Anthropic Messages | `tool_use` content 块的 index | `content_block_start` |
| Responses | 该 item 的 `output_index` —— 或者，在一个没有它的帧上，是仍开着的那个调用（帧开启一个新调用时就是新调用） | `response.output_item.added`，drain 现在会读它；名字过去只随已完成的 item 到达 |
| Ollama | 调用在这一轮里的位置 | 整个调用，一帧到达 |
| Cloud Code | 调用在这一轮里的位置 | 整个 `functionCall` part，一帧到达（`docs/providers/antigravity.md`） |

槽位是身份，不是位置：一个 Anthropic 的文本块也会占一个 index。token 统计不变，
因为 `ToolCallFragment::tally_text` 就是名字加参数文本，正是旧字符串所携带的。
Responses 的 drain 在 `responses::ToolStreams` 里记账：名字在
`output_item.added` 处只数一次，而 `output_item.done` 只补上没有更早的帧送过的
部分 —— 一个其开场它从未见过的调用的名字，以及一个没有任何参数以 delta 流出的
调用的参数，这正是为什么一个只在已完成 item 里交出调用参数的服务器，仍然能让
它的文件被预览、并且只被数一次。`output_index` 是服务器负责发的。一个流一次只
产出一个输出 item，所以一个没有它的帧属于仍开着的那个调用，或者在这个帧开启一个
新调用时、或在所有更早的调用都已完成时开始一个新调用 —— 绝不会被折进第一个调用
的预览，而一个缺失的 index 过去正是把它放在那里。

## 读取尚未到齐的参数

`app::ArgsScanner` 是某一个调用参数对象之上的增量解码器。它是一台字符状态机，
一边来一边吃（`feed`），并且把每一个**顶层字符串字段**保持解码到它已到达的程度：
`{key, value, closed}`。嵌套对象、数组和标量被跳过；转义被解析，包括一个被拆在
两个碎片之间的 `\uXXXX`，以及一个被拆在两个转义之间的代理对。一个孤立的代理
解码为 U+FFFD。两个性质在下游要紧：

- **值只会变长。** 流的每一个前缀都解码成整个值的一个前缀，无论怎么拆。下面的
  渲染缓存依赖这一点。
- **它从不需要再要一遍整段文本。** 一个 100 KB 的文件，在整个流上只花一遍对它
  字符的扫描，而按碎片重新解析累积的参数会是平方时间。

重复键的第一个字段是被读取的那个。不是对象的文本产不出字段，也就什么都不显示，
因为预览只是装饰，而这次调用本身仍会在回合结束时由 `serde_json` 解析。

`serde_json` 也是解码器在测试里的 oracle。每一个样本 —— 每一种转义、一个代理对、
字面的非 ASCII、另一个键之下的嵌套 `content`、标量、token 之间的空白 —— 都在
**每一个**字符边界处被拆成两半喂进去，并且必须解码出 serde 在整段文本里所读到的
那些字段，每个都已闭合；一个字符一个字符地喂时，每个字段必须只会朝 serde 的值
增长。一个只与自己一致的解码器，会通过一个拿它自己的整段文本答案来核对它的测试。

## 这一轮的草稿

`app::ToolDrafts` 持有正在生成的那一轮的各个调用，每个线上槽位一个
`ToolDraft`，按各自开启的顺序排列。`App` 为主 session 保留一个，而每个
`AgentRun` 保留自己的那个。只有 `write` 会被**预览**
（`ToolDraft::is_previewed`）。其他每一个调用都被跟踪，好让它的碎片落到正确的
位置，而它什么都不显示。一个 `Bash(…)` 头部为它命令生成所需的几百毫秒闪一下，
说不了什么，因为那一批片刻之后就会说。

一个草稿的生命周期就是这一轮的生命周期：

1. **`push`**：一个碎片折进它槽位的草稿，槽位是新的就打开一个。它回答这个碎片
   是否**打开了一个被预览的调用**，那正是边界冲刷流式文本分段的时刻。这是
   `tui/stream.rs` 的第一条规则，*交叠之前先 flush*，正如 `ThinkingStart` 所
   应用的那样。引入这个文件的那一段以它被扣留的最后一行提交进 scrollback，所以
   单元格坐在它下面，而不是旁边。`opens_preview` 不折叠地问同一个问题，供
   agent 视图使用，它的冲刷跑在它的折叠之前。
2. **`seal`**：这一轮的调用被宣告了（`RoundCalls`、`AgentBatch`、一个
   `TaskCall`），所以不会再有什么流进来。每个草稿都被收尾，而下一个碎片打开
   新的一轮，而不是继续上一轮用过的某个槽位。
3. **`adopt`**：批次宣告（`ToolBatch`，或者一个推入了新调用的孤立
   `ToolStart`）把这些预览接过去。排队的 `Write` 单元格与那些被预览的草稿**按
   顺序**配对，但只在两个数量一致时。数量不一致就什么都不配，而不是去猜，即
   `start_tool_batch` 早已对这一轮的 call id 所应用的规则。采纳会替换掉更早一次
   采纳所留下的东西，所以一个陈旧的草稿永远不会与后来某一批的单元格配成对。
   一个孤立的 `ToolStart` 只采纳到空队列上。推入时旁边还有活着的调用，按顺序
   配对可能把另一个调用的文件交给某个单元格，所以那些草稿被清空，什么都不预览。
4. **`resolve`**：一个排队的调用离开了队列（`App::resolve_front_tool`，
   subagent 折叠的各个解决）。一个 `Write` 交出最前面的已采纳草稿。已采纳的草稿
   按队列顺序、最前为先，所以第 k 个排队的 `Write` 总是与第 k 个已采纳的草稿
   配对。
5. **`clear`**：每一个回合边缘都把这一切丢掉（`begin_stream`、`begin_shell`、
   `finish_stream`、`fail_stream`、一次中断、一个被 block 的 prompt、一个
   `/compact` 回合的结束、`/clear`）。一个生成了一半的调用永远不被记录。它的
   JSON 是不完整的，模型从未把话问完，所以没有任何东西可供之后的请求重放。

草稿之外还有一条规则随它们一起搬了。Esc 中断会**撤销**一个什么都没显示过的回合，
把消息放回 composer（`docs/input/interrupt.md`），而一个已经开始流进它单元格的
文件是「显示了东西」。所以在一个被预览的 `write` 出现之后按 Esc，改为中断这个
回合，保留消息并提交通知。一个什么都不预览的调用，比如一条正在生成的 `bash`
命令，仍然保持撤销。

`revision` 在每一次变化时前进。Ctrl+O transcript 在它上面签名。

## 单元格

`ui::file_cell` 的草稿渲染器仅凭草稿构建这个单元格。它的几何是已提交单元格的
几何，所以两者不会漂移：

- **头部**是那个普通的 `● Write({path})` 行（`tool_header_lines`，所以路径显示
  规则和文件链接一起带上）。路径在它的字符串闭合之后才显示，因为一个头部为了一
  帧拼出 `/home/us` 会读起来像个 bug。生成期间，圆点用运行中的灰色闪烁
  （`docs/ui/tool-pulse.md`）。排队之后，它穿队列的颜色。
- **拐角行**说出这个调用走到哪了：生成期间是 `Writing {n} lines…`，行数随着行
  到达而增长（第一行之前是 `Writing…`），排队之后是 `Waiting…`/`Running…`。
- **正文**是已提交 `Wrote …` 单元格那套带编号、带语法高亮的装订线，由同一个
  `numbered_row_lines` 构建。它用同一个 `{n:>W}` 装订线，以及拐角之后同样的
  一列缩进。窗口是已提交一瞥的 `FILE_PEEK_LINES`（10）个**显示行**：
  - **尾部**，内容流式期间：最新的十行，未完成的最后一行在底部。是行不是列：
    一个还在到达的长行显示刚刚到达的文本，而顶行可能是一行已经开始滚出去的行的
    结尾，所以窗口永远不会长过十行，也不会截断任何一行
    （`docs/ui/long-lines.md` §7）；
  - **头部**，全部到达之后：已提交的一瞥 —— 十行之内从顶部开始的那些行，剩下的
    行装得下时每行整个显示，需要更多的那一行在第十行处带一个 `…` 被截断
    （`docs/ui/long-lines.md` §8）—— 之上是
    `… +N lines (ctrl+o to expand)` 提示，其中 N 数的是这一瞥从未开始的行；
  - **整份**，在 Ctrl+O 里：每一行。

窗口在内容收尾引号处、或在这一轮没有引号就 seal 时，从尾部翻到头部。它不等这个
调用被宣告。一个先于它的 `path` 流出来的 `content`，在路径还在到达时就显示它的
头部。

排队时，一个带有已采纳草稿的 `Write` 在 `Waiting…` 之下显示头部，而不是光秃秃的
`⎿ Waiting…` 行。这只在 live 条带里如此。permission prompt 的 context 单元格
仍然用单行单元格（`docs/state/permissions.md`），因为 prompt 已经把整个文件显示
出来了。

**高亮缓存。** 尾部是高亮正确的，这意味着高亮器必须已经见过它上面的每一行：一个
在第 3 行打开的 docstring 会给第 40 行上色。重新高亮整个文件每行要几十微秒，而
条带每帧要构建好几次，所以几百行就会把整帧花在 syntect 里。
`highlight::LineCache` 改为增量地高亮一段增长的文本：每个完整的行高亮一次，把
高亮器的状态带到它们最后一行之后，而未完成的那一行跑在那个状态的一个*克隆*上，
与 `StreamRender` 为一个代码块被扣留的那一行所用的同一个一瞥。它按文本长度缓存，
所以一帧之内的多次构建是免费的。草稿把它藏在一个渲染路径填充的 `RefCell` 后面，
即 textarea 的换行缓存模式，所以缓存的寿命恰好与草稿一样长。语言或代码主题变
了，或者路径在内容之后到达，就重建一次。

**缓存保留什么。** 每一行都被高亮，但条带的单元格永远只*显示*它最早或最新的那几
行：头部和尾部两次遍历各自从自己那一端最多取 `FILE_PEEK_LINES` 个源码行，因为
每一行至少占一行，而每次遍历在预算用完时就停，而不是去读一行它显示不了的行。
所以草稿保留两个缓存。**一瞥**缓存（`LineCache::windowed(FILE_PEEK_LINES)`，经
`ToolDraft::highlighted_peek`）只保存最早十行和最新十行的分段，加上未完成的那
一行。它仍然按顺序高亮每一行，因为语法的状态必须穿过中间，但让中间那些分段走掉。
**整份**缓存（`ToolDraft::highlighted_whole`）保留每一行，并且只在一个显示每一
行的视图索要时才被填充：Ctrl+O transcript 或 `/export`。它随草稿在回合结束时一
起走。

这要紧，是因为一行的分段约是它文本的五十倍：每个 token 一个 `String` 和一个样式。
保留每一行每行约花 0.93 KB，约是文件的 19 倍，只要这次调用还活着就一直如此，而
glibc 会保留一个进程用过的，所以一次 2,000 行的 write 让应用在之后的 session 里
大了约 2.3 MB。一瞥持有的是解码后的文件加约二十行：同一次 write 是 147 KB 而不是
1.86 MB。`ui::tests::file_cell` 的
`the_cells_rows_are_the_ctrl_o_views_rows` 把一瞥的行钉在整份视图的行上，样式也
算。`tests/write_preview_memory.rs` 把一次流式 5,000 行 write 的常驻增长卡在文件
的四份拷贝上。它量到的是一份拷贝，而逐行缓存的常驻代价约为二十七份
（`docs/architecture/memory.md`）。

## 它在条带里的位置

`preview_lines`/`preview_rows` 共享一个分支：预览排在一个实时 agent 组或 tool
队列之后，在一个思考块和回复的边界之前。它们在时间上与头两者互斥，因为一轮的
调用在它的批次存在之前就流式了，而 backend 在第一个碎片之前就关闭了一个思考阶段。
几个被预览的 `write` 空行分隔地堆叠，像一批一样。这些单元格算作**实时单元格**
（`strip_preview_is_live_cells`），所以一条对小终端来说太高的条带会把它的头部流
进 scrollback，而不是裁掉它（`docs/ui/strip-flow.md`）。`strip_flow_key` 在每个
预览的路径以及它是否已完成上签名，绝不在它的行上签。所以一行流过的文本不会再签
一次名，而从尾部到头部的那次翻转 —— 它替换每一行 —— 则会。

**一次写进 scratchpad 的 write 在这里没有单元格**，当 **Verbose output** 关着
时（默认如此）：一旦它的路径出来并且在某个 session 的 scratchpad 之内
（`App::is_group_draft`），这个草稿就是那个打开的折叠组的一部分，而它的
`Making 1 scratchpad edit +N…` 行在行到达时计数，它的 `⎿` 行点名它写的文件
（`docs/ui/tool-groups.md` *Scratchpad edits*）。那个分支是一次遍历，
`ui::live::drafting_lines`，预留的行和绘制的行共享它：先是打开的组，然后是不属于
它的每个草稿一个单元格。一个路径还没到达的草稿就是其中之一 —— 还没有什么说出它
写到哪里 —— 所以它的单元格流在组之下，组在它之上保持打开；只有一条落在所有
scratchpad 之外的路径才关闭这个组（`App::draft_ends_group`，由边界在每个碎片之后
询问）。一旦这一批采纳了这些草稿，一个被组计数的 scratchpad `Write` 把它被
采纳的草稿一起带走，所以组之外第 k 个被预览的单元格仍与它自己的那个配对。

## subagent session 视图

一个 subagent 的 `write` 在它的 session 视图里流式的方式与主视图完全一样
（`docs/agents/agent-view-streaming.md`）。`AgentRun` 用同样的生命周期保留自己
的 `ToolDrafts`，而 `agent_view_preview_lines`/`agent_preview_rows` 通过同一个
构建器渲染这些预览。视图的分段冲刷跑在折叠**之前**，所以它问
`AgentRun::will_flush_segment`，即折叠自己的答案，而它现在包括一个打开被预览调用
的碎片 —— 而 `every_flush_point_is_declared` 对每个事件中的一个以同样的方式问
它，包括那个开场的碎片。这里手维护一张清单，曾经把一次 `SubagentStop` 继续的
尾部丢掉过一次（`docs/agents/agent-view-streaming.md`）。

## 离线

dummy 的文件改动 demo（一个提到 *write*、*edit* 或 *diff* 的 prompt）以活模型的
方式流式输出它的 `write`：这次调用真实的 JSON 参数，按回复自己的节奏
（`ZEROTH_CHUNK_DELAY_MS`）切成 token 大小的碎片（`stream::tokens`）。
`smoke.sh` 的 Phase 138 在 tmux 里驱动它，然后用**真实的** backend 去驱动一个
stub provider，后者每次六个字符地流一个 30 行的 `write`，在它移动时采样这个
单元格。超过十行之后，窗口的底行必须继续沿文件向下移动，而第 1 行绝不能出现。
内容闭合之后，单元格必须显示第 1 到 10 行，而已提交的单元格必须在
`… +20 lines` 之上保留它们。Phase 138b 以同样的方式流一个长段落行并把文件读
回来：流式单元格必须显示它超过三行、且没有任何一行以 `…` 结尾，而已提交的
`Write` 与 `Read` 单元格都必须显示它的最后一个词（`docs/ui/long-lines.md` §7）。
Phase 138c 流一个第四行有几千列长的样式表并把它读回来：单元格的任何样本都不得
超过十行正文，而后提交的 `Write` 单元格与 `Read` 单元格必须各自在它们的第十行处
用 `…` 截断那一行，之上是 `… +6 lines`（`docs/ui/long-lines.md` §8）。

## 它不做什么

- **`edit`** 不流任何预览。它的 `new_string` 会以同样的方式预览，但一次 edit 所
  解决成的单元格是一个 diff，而一个不断增长的替换内容被当作新文件显示，看起来
  会像另一种单元格。
- **一次覆写**仍然解决为 `Updated {path} (+A -D)` 的 diff。草稿无法知道文件是否
  存在，所以它把内容当作新文件预览，而执行器报告时 diff 替换掉它。
- **一个这一轮从未完成的调用**（一次 Esc、一个错误、一个被截断的请求）不被记录，
  一如从前。它的单元格随这个回合的其他实时行一起消失。
