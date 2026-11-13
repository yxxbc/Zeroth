# 运行中圆点的闪烁

一个正在执行的 tool 不再显示蓝色的 `●`。它显示 permission prompt 覆盖在它所询问
的那个调用之上的同一种灰 —— 而在 live 区域里那个圆点**会闪**：亮半秒，暗半秒，
旁边头部文本永不移动。Claude Code 的运行中点，移植到内联 TUI。

```
● Bash(npm test)          ← grey, shown…
  ⎿  Running… (3s · timeout 2m)

  Bash(npm test)          ← …then hidden, the text holding its column
  ⎿  Running… (3s · timeout 2m)

● Bash(npm run lint)      ← flat grey: queued, not started
  ⎿  Waiting…

● Read(src/main.rs)       ← green: it resolved
  ⎿  Read 412 lines
```

## 为什么是灰的

旧的调色板为「在飞」花掉了一种颜色：运行时蓝，完成时绿或红。但「在飞」是那个本来
就已经一目了然的状态 —— 它就是屏幕底部的那个单元格，上面有个 spinner，下面有
`⎿ Running…` —— 而蓝色与真正携带信息的绿/红在争。

所以现在在飞的一切都是柔和的，只有**解决**才落成颜色。这与 permission prompt
早已遵循的规则是同一条：它所询问的那个调用渲染成一个光秃秃的灰色
`● Write(hello.py)` 头部，因为还什么都没发生。一个运行中的调用是同一类东西 ——
没有完成，没有被判定 —— 现在也以同样的方式阅读。

动作才是取代色相的东西。静止的灰点和闪烁的灰点很容易分辨，而闪烁说「还在跑」远比
任何颜色直接。

## 为什么是闪，不是呼吸

第一个版本是*呼吸*的：一个升余弦把圆点在两种灰之间缓动，暗 → 亮 → 暗，一秒一次。
它在动，但它动的方式是显示**第二种颜色** —— 一种只为它而存在的更深的灰 —— 而在
一个没有余量容纳中间色的终端调色板上，它是跳变而不是胀缩，或者（在 `ansi` 主题
下，它的两端是同一个亮黑）根本不动。Claude Code 的运行点不这么做：它只有一种
颜色，它在或不在。

所以圆点保持**一种灰**，靠*有无*来动画。在周期的隐藏那一半，`●` 被替换成它自己
宽度的空白，样式相同，所以头部文本从不移动一列，这一行的宽度与原来完全一样；一个
`Bash(for i in …)` 头部有没有它那个点都在同一个位置阅读。没有中间帧 —— 在每一半
之内圆点保持不动 —— 这正是让它读起来像闪烁而不是抖动的原因。

## 周期

`ui::tool::tool_pulse_visible` 是一个 `Duration` 的纯函数：

```
shown = (elapsed mod PERIOD) < PERIOD / 2
```

整毫秒，所以无论时钟怎么读，那个边沿落在同一帧上。`ui::tool::bullet_span(color,
blink)` 是头部那个 `●` 唯一来源：`blink: Some(phase)` 按那条规则渲染这个字形或
它的空白，`None` 让它静止渲染。这些常量与每一个其他样式决定一起住在
`ui/theme.rs`：

| 访问器 | 调色板角色（`onedark` 下的值） | 含义 |
| --- | --- | --- |
| `tool_running_color()` | `dim`（`#8A8A8A`） | 一个运行中圆点所穿的**唯一**一种颜色 —— 显示时用它，冻帧渲染也用它 |
| `TOOL_PULSE_PERIOD` | `1000 ms` | 一次完整的闪烁：前一半显示，后一半隐藏 |

这种灰来自活动主题的调色板（`docs/ui/theme.md`）；在 `ansi` 主题下它是终端的
亮黑，而圆点在其中闪烁的方式与它在别处完全一样 —— 一次闪烁不需要第二种色阶。

这个周期明显粗于循环的 32 ms 动画帧，也慢到足以读作脉动而不是抖动。

呼吸所混合的 `pulse_dim` 调色板角色与 `tool_pulse_dim()` /
`tool_pulse_bright()` 访问器还在，但只供 **`pulse` spinner 样式**用
（`docs/ui/spinner.md`）：状态行的 `●` 保持那个升余弦的胀缩，从那种暗到白，在
圆点的同一周期上，所以一条脉动的状态行与一个闪烁的单元格仍然同步移动。

## 相位从哪来

与本代码库里每一个其他时钟相同的地方：边界。`App::set_pulse(Duration)` 在每次
绘制之前从 `StatusClocks::loop_start.elapsed()` 写入，正如 `set_status_times` 与
`set_command_elapsed` —— 纯核心从不读时钟。

它是一个**相位，不是一次测量**：没有任何东西显示它，所以它的纪元是任意的。要紧的
是它在前进，而循环既有的动画重臂已经保证了这一点 —— 绘制分支在 `turn_active()`
（tool 只能在回合内运行）、后台 band 打开、或有任何 agent 活着时，把下一帧安排在
32 ms 之后。

刻意用一个时钟，而不是让每个圆点各自计时：

- 一个混合回合的各个 tool 单元格与它的 `● Running 3 agents…` 树同屏，闪烁不同步
  会看起来像坏了，而不是有意为之；
- 与回合的 `elapsed` 不同，它在回合之间继续走，所以一个**后台** agent 的实时
  单元格也在动。

所以 `StatusClocks` 不再有 `Default` —— `loop_start` 是一次真实的读数，隐式地给
它打戳会让一个随手写的 `default()` 在会话中途悄悄重启每一次闪烁。

## 哪些圆点会闪

每一个意为「运行中」的实时 `●`，都经由那一个 `bullet_span`：

- 一个运行中 tool 单元格的头部（`live_tool_lines`，以及流式 `bash` 单元格的
  `running_command_lines`）；
- 那个聚合的 `● Calling {server}…` MCP 单元格（`docs/tools/mcp.md`）；
- 实时的 `● Running {n} agents…` 树与一个孤立 agent 的 `● Agent(…)` 单元格
  （`docs/agents/agent-tool.md`），在主条带里和在一个 agent session 视图里都是；
- 一个打开着的推理阶段的 `● Thinking…` 头部（`docs/ui/thinking-stream.md`）。

只有 `Running` 会动。一个并行批里的 `⎿ Waiting…` 兄弟、permission prompt 正在
询问的那个调用（它在等*你*）、以及每一个已解决的单元格，每一帧都保着它们的 `●`
—— 它们每一帧的意思都一样，让它们动什么也说明不了。

## 哪些地方*不*闪

动画只属于 live 区域，而渲染器的划分就是这么说的：

| 渲染器 | 闪？ | 为什么 |
| --- | --- | --- |
| `live_tool_lines`（条带） | **闪** | 每帧重绘，从不提交 |
| `running_command_lines`（流式 `bash`） | **闪** | 同一条带 |
| `agent_view_preview_lines`（一个 agent session 视图） | **闪** | 同一条带，为被查看的 agent |
| `tool_lines`（scrollback 提交、resize 重绘） | 不闪 | 一行已提交的东西**永久**冻结 —— 它绝不能捕获隐藏的那一半 |
| `tool_full_lines`（Ctrl+O transcript） | 不闪 | 见下文 |
| permission prompt 的 context 行 | 不闪 | 没有东西在跑：调用在等*你*，而那个 prompt 是一帧静物 |

`tool_lines` 与 `live_tool_lines` 是同一个函数，相位是 `None`/`Some` —— 一个
函数体，所以两者不会漂移。一次 scrollback 提交绝不能冻住的，是**隐藏**的那一帧：
一个没有它圆点就被提交的单元格，会读成一个永久丢了点的头部，而那正是这个划分存在
所要防止的意外。一个未注入的时钟（相位 0，单元测试的默认）把圆点渲染成**显示**的
—— 周期的顶部。

**Ctrl+O transcript** 是有意静止的。它是一个分页器，覆在一个增量构建的缓存之上，
它的刷新在一个不含时钟的签名上短路（`docs/ui/tool-view-performance.md`）；在那里
做动画，要么根本不动，要么每 32 ms 付一次整条 live 尾部的重渲染，只为了一颗没人
为了看它闪而打开 overlay 的圆点。它的运行中圆点静止渲染 —— 仍是那种灰，仍与周围
的绿/红不同。

## 什么还留着蓝色

`context_user_color()`，Ctrl+D context 视图中 `USER` 角色 chip 的填充，过去是
运行色的别名。它是一个*角色*，不是一种运行状态，所以它保留链接蓝（调色板的
`link` 角色 —— One Dark 主题里是 `#61AFEF`）作为它自己的值，两者就此分道。

## 测试

- `ui/tests/tool.rs` —— 运行中圆点是 permission prompt 的那种灰，只有一种颜色；
  它在周期的顶部与四分之一处显示，在一半与四分之三处藏在它自己宽度的空白之后、
  头部文本保住它的列，并在一个周期之后循环；`Waiting`/`Ok`/`Failed` 无论时钟怎么
  说都不动；而 `tool_lines` —— 供给 scrollback 的那个渲染器 —— 总是显示它。
- `ui/tests/live.rs` —— 接线：`render_live` 在注入的相位上绘制条带的圆点，而把
  `App::set_pulse` 移半个周期会明显地把绘制的单元格变成空白，同时头部文本留在
  原处。
- `ui/tests/agent.rs` —— 实时 agent 组的圆点在同一个时钟上闪。
- `ui/tests/reasoning.rs` —— 实时 `● Thinking…` 的圆点闪，已提交的那个从不闪。
- `smoke.sh` Phase 57 —— 在一个真实终端里，一批的运行中单元格跨帧被采样，必须既
  抓到有圆点的、也抓到没有圆点的，任何帧都不得藏起超过一个圆点（`⎿ Waiting…`
  兄弟们保着它们的），每一个圆点的灰都是那唯一一种静止灰，而解决后的单元格落成
  绿色。
