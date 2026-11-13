# 条带流动 —— 流式条带冻住而不是消失

## 问题

live 区域的流式条带 —— 一个运行中 tool 的单元格、那个 `● Thinking…` 块、一棵
实时的 agent 树、spinner 状态行、排队消息、toast —— 是该区域唯一**弹性**的
内容。它持有的其他一切，都由这一帧无法与之谈判的状态决定（composer 的那两条
规则和它折过的行、band、页脚、agent 名单），所以 `ui::preview_budget` 把剩下的
花在预览槽位上，而 `ui::fitted_preview_rows` 把内容夹取到它。那个夹取是承重的：
正是固定的额度曾经让一张正在成形的表格把 composer 挤出屏幕
（`docs/ui/table-streaming.md`）。

夹取对它付不起的那些行所做的，是**把它们扔掉**。`preview_lines` 从尾部裁掉一个
live 单元格，于是随着终端变矮，一个运行中的 `bash` 调用按这个顺序丢行：

| 行数 | 条带丢了什么 |
| --- | --- |
| 16 | 什么都不丢 —— 整个单元格 |
| 13 | `+16 lines (15s)` 页脚**以及** `(ctrl+b to run in background)` |
| 11 | 四行输出里的两行 |
| 9 | 整个 `⎿` 输出块 —— 只剩头 |
| 7 | 整个单元格 |
| 4 | **状态行**（`live_layout` 饿死条带本身） |

`+N lines` 那一行 —— 唯一一行说明输出正在被藏起来的东西 —— 是*第一个*牺牲品，
而它丢掉的一切哪儿都找不到：不在屏幕上、不在 scrollback 里，也不在解析后的
单元格里（那个单元格提交的是 `ctrl+o to expand` 之后四行的头部窥视）。被报告的
bug：

> 当终端窗口非常矮时，它慢慢地把流式文本删掉，而不是冻住顶部并让 bash tool
> 滚动

## 设计

与带框视图已经遵循的同一条规则（`docs/ui/view-flow.md`），用在条带上。两半：

1. **条带是一个行构建器，按底部锚定来画。** `ui::live::strip_lines` 按绘制
   顺序产出条带的每一行 —— 预览槽位及其间隙、挂着 task 清单的状态行及其间隙、
   排队消息、toast —— 恰好
   `strip_rows(has_status, preview_n, task_rows) + queued_rows + toast_rows`
   那么多，于是 `live_height`/`live_layout` 预留的行与画出的行按构造一致。
   `render_strip` 通过带框视图自己的 `render_framed_tail` 来画它，于是一个内容
   超出自己行数的条带保住它**最后**的那些行：最新的流式行、`+N lines` 页脚、
   ctrl+b 提示、状态行。

2. **头部流进真实的 scrollback，冻着。** `ui::view_flow` 为条带增加一个分支，
   于是锚点跳过的行被提交到 live 区域之上 —— 终端自己的滚动在那里读它们 ——
   而边界既有的机制做完剩下的：`Session::flowed_view` 持有那个签名，
   `Session::commits_allowed` 在它之下暂停提交，任何签名变化都用标准的 purge
   重建作答。

单元格**滚动**起来了，这正是当初所要的：它的顶部冻在区域之上，最新的行留在
屏幕上。

### 几何*不*变的东西

`preview_n` —— 区域**预留**的行数 —— 仍然是 `fitted_preview_rows`，所以
`live_height`、`live_layout`、`input_box` 与 `cursor_position` 不受影响，
composer 受的保护与以前完全一样。只有条带的**内容**按完整请求构建
（`strip_content_preview_rows`），而正是这一点让它比自己的矩形更高，并给锚点
提供了可跳过的行。在一个放得下这个回合的终端上，两者是同一份列表，什么都不流动。

### 冻住，而不是反复签名

条带是屏幕上最易变的东西：输出流进运行中的单元格、耗时与 token 统计在走、
小方块在闪 —— 全都按活跃回合重新挂上的那个 32 毫秒节奏。用它的行来给流动签名
会每秒 purge 重建整屏三十次，所以它取 ↓ manager 的 `FlowSign::Frozen`
（`docs/ui/view-flow.md`），键是 `ui::live::strip_flow_key`：屏幕上哪个对话、
状态行是否立着、每个排队 call 的名字/参数/状态、live agent 组的成员、是否开着
思考阶段，以及每个仍在生成的 `write` 的路径与它是否已完成
（`docs/ui/write-streaming.md`）。一个新 call、一次解析、新的一轮、回合结束，
或者一个流式 `write` 从尾部翻到头部，会重新签名；流出的一行不会。宽度与流出的
行数由调用方哈希，所以 resize 也会重新签名。

于是冻住的行会变旧：在 11 行时 scrollback 可能停在 `icmp_seq=9`，而屏幕显示
`icmp_seq=12`，`+N lines` 的计数覆盖了这段缺口。这就是 bug 报告要的那个取舍 ——
陈旧胜过缺席 —— 而且它是临时的：解析会 purge 那些冻住的行，并提交真正的单元格。

### 流式回复的前沿是唯一的例外

回复的预览是它**未提交的**前沿。`StreamRender` 每有一条完整的行落地就把它提交
进 scrollback（不变量 2），所以夹取在那里丢掉的并不是丢失的行 —— 它们是*即将*
被提交的行 —— 而一个每个 chunk 都在增长的前沿会每收到一个 chunk 就重新签名
流动、并 purge 重建屏幕。所以 `strip_content_preview_rows` 给前沿的是**预留**
行数，它对流动毫无贡献；只有 live 单元格（它们在解析之前不进入任何缓冲）流动。

### 范围：仅 composer 路径

条带在 composer 在屏幕上时流动。一个替换 composer 的视图（`/model`、
`/settings`、↓ band……）改用 `view_split` 分割区域，那个会钉住自己的框并挤压
条带，而在那里流动的是**视图的**页面 —— 一次只流动一个，所以 scrollback 绝不
可能交错两页。`ui::layout::strip_paint_rows` 刻意只是 composer 路径的那一段
`live_layout`，不多不少；让它学会整条 `live_region_height` 优先级链，会是一个
可能漂移的、某个和的第二种拼法。

## 代价，以及针对它的护栏

`view_flow_stale()` 在**每个绘制 tick** 都跑，所以条带分支绝不能每帧付一次
构建的代价。`ui::live::strip_content_rows` 是那条早退：它报告条带的高度而不
构建它（与区域已经预留的同一个 `strip_rows` 之和），而一个放得下的条带在
`strip_lines` 被调用之前就返回了。它最坏也只是高估 —— 对回复的前沿，预留数
可以超过兜底构建所产出的量 —— 所以“放得下”永远意味着“放得下”。

## 它依赖的不变量

- 不变量 3（内容锚定的 viewport，经 `term::reflow` 重建）：流动搭的是同一次
  purge 重建，而 `write_above` 本来就会把区域坐落在任意长的尾部之下。
- 不变量 4（`commits_allowed` 门控）：一个在活跃流动之下解析的单元格在历史里
  等着，流动退出时的重建重新生成它 —— 而这正是让冻住的预览行不会被真正提交的
  单元格重复一遍的原因。
- 预览预算（`docs/ui/table-streaming.md`）：不动。流动改变的是付不起的那些行
  *去哪*，从不改变区域预留多少行。
