# 无闪烁渲染 —— scrollback 提交加入绘制帧

日期：2026-06-11

## 症状

回复在流式输出时，live 区域（输入框 + 页脚，加上流式条带）会**闪**：每有一条
完成的行提交进 scrollback，盒子就消失几毫秒，然后在下一个绘制 tick 上重新
出现。修复前
一个流式回合的字节轨迹说明了原因：**14/14** 次 live 区域清空（`ESC[J`）都写在
任何同步更新块*之外*，每一次后面都跟着它自己的盒子重画，在同一次、更晚的 flush
里 —— 每提交一行就产生一个可呈现的无盒状态；在 60 Hz 的终端刷新下，那就是每隔
几秒一次肉眼可见的闪。

## 诊断 —— 三个窗口，flush 出的帧里缺了 live 区域

这个项目本来有两层防闪烁：`term::draw` 把 live 区域的重画括在一次同步更新里
（BSU/ESU，DEC 模式 2026），而保留的 `prev` 缓冲对静止帧做差量，于是一次按键只发
出几个单元格。两者都只覆盖*绘制*路径。缺口是：

1. **流式提交。** `insert_before` 立刻做它的终端 I/O，在任何同步更新之外：把屏幕
   往上滚、画出提交的行，然后**清空 live 区域**（从 viewport 顶部起
   `ClearType::AfterCursor` —— 盒子、队列行和页脚全空），每一步都 flush 了。盒子
   的重画随后要等下一个合并的绘制 tick（120 fps 上限下 0–8.3 毫秒，还要加上终端
   刷新）。每提交一行就闪一次；插入中途的状态（屏幕已滚、行还没画）可能撕裂；
   硬件光标 —— 从不隐藏，也在任何 BSU 之外 —— 在插入写入时肉眼可见地在屏幕上
   乱跳。
2. **重排**（resize、Ctrl+O 返回、`/clear`）：同样的形状 —— `reflow` 重写屏幕、
   留下清空了的 live 区域，盒子晚一个 tick 才重画。
3. **Ctrl+O 进入**：`enter_overlay` 清空备用屏，而第一次 overlay 绘制晚一个 tick
   才来 —— 一次黑闪。

## codex 是怎么做的（移植自 `/tmp/codex/codex-rs/tui`）

- `Tui::insert_history_lines` **不做任何终端 I/O**：它把行追加到
  `pending_history_lines` 并安排一帧（`tui.rs`）。
- `Tui::draw` 在**一次**同步更新里做*所有*事情
  （`stdout().sync_update(..)` —— crossterm 的 `SynchronizedUpdate`，同一对
  BSU/ESU）：重新钉住 viewport、`flush_pending_history_lines`（真正的 scrollback
  写入，经 `insert_history.rs`），然后是 live 区域的 `terminal.draw`。一个原子帧
  —— 终端从不展示“历史写了、盒子不在”的中间状态，而插入期间光标的旅行是不可见
  的（`insert_history_lines` 刻意做到“光标位置中性”）。
- 同步更新也让光标处理变得安全：写入前后什么都不需要隐藏/显示，因为任何中间状态
  都不会被呈现。

## 移植

`term::InlineViewport` 采纳 codex 的 pending-lines 模型；`main.rs` 里的公开调用
形状几乎不变。

- **`insert_before(lines)` 只排队。** 它把行追加到一个新的
  `InlineViewport::pending` 缓冲并返回 —— 现在不会失败（I/O 移进了帧里）。每个
  既有调用方本来就在它之后安排一帧。旧的函数体（ratatui 那套可移植的
  insert-before 滚动算法，配 tmux 安全的“先画后清”顺序）作为私有的 `write_above`
  保留下来。
- **`draw` 在它自己的帧里冲刷队列。** 既有的 BSU/ESU 括号里现在包含：
  `flush_pending`（把排队的行写在 viewport 之上 —— 该区域的行留着过期内容，将在
  原地被覆盖；见下面的*提交清空 —— 已退休*），然后重新钉住、渲染并重画 live
  区域，最后放置光标 —— 末尾一次 flush。scrollback 的增长与盒子的重画作为一次
  原子更新落地；不再有任何不带盒子的 flush 状态。没有待处理内容的帧仍走 `prev`
  差量的快路径（一次按键仍然只发出变了的单元格）。
- **`reflow` 也画 live 区域。** 它收渲染闭包 + `App`（与 `draw` 一样），并把整个
  重建 —— 清空尾部、把 viewport 坐到顶部、`write_above(tail)`、以及在最终
  viewport 处画 live 区域 —— 括在一次同步更新里。一次 resize 重画或 Ctrl+O 返回
  现在是一个包含盒子的原子帧。先丢掉过期的 `pending` 行：排队的一切都会从
  `history` 重新生成（调用方会重置 `committed`），所以把它们也冲出去会造成重复。
- **`restore` 冲刷残留。** 退出可能落在排队与下一个 tick 之间（比如一条 notice，
  然后立刻 Ctrl+C）；`restore` 在离开 raw 模式之前写出任何待处理的行，所以已提交
  的内容永不丢失。
- **Ctrl+O 进入时立刻绘制。** `ToggleToolView` 分支在 `enter_overlay` 之后马上画
  overlay，而不是等那个 tick，关掉了黑闪窗口。（退出本来后面就跟着
  `repaint_conversation`，它现在经 `reflow` 在同一帧里画出盒子。）
- `paint_frame` 排入 `Show` 而不是 ratatui 的 `show_cursor`（后者会 `execute!`
  —— 一次帧中额外的 flush），所以一帧只有一次写入。

## 光标可见性 —— kitty 光标拖尾的细化

日期：2026-07-09

同步更新阻止终端*呈现*撕裂或无盒的中间帧，但它**不能**阻止一个会动画光标移动的
终端（kitty 的 `cursor_trail`）拖出一道痕迹。那个动画由光标的*逻辑*位置驱动：
一帧先把光标归位到顶部（resize / `/clear` 时 `clear_scrollback_and_screen` 的
`ESC[H`）或者滚动它（scrollback 提交时 `write_above`），最后才把它重新坐到提示符
上，仍然把一个**显示中的**光标从提示符 → 顶部 → 再回来地移动了一遍，于是 kitty
从顶部往下画出一道拖尾，尽管每一个字节都骑在同一个 BSU/ESU 帧上。这就是“光标
动画从顶部开始，而不是从文字区域开始”的那个报告。

所以早先那句“写入前后什么都需要隐藏/显示”只对*撕裂*成立，对光标拖尾不成立。现在
这一帧还在**它整个身体期间隐藏光标**，只在最后那个提示符座位处显示它 —— codex 的
规则：光标只在它停下来的地方被*显示*：

- `draw` 与 `reflow` 在 `BeginSynchronizedUpdate` 之后立刻排入 `Hide`，先于任何
  `flush_pending` / `write_above` 滚动、`clear_scrollback_and_screen` 归位或
  单元格 blit。`paint_frame` 在定位光标之后排入对应的 `Show`，于是它只在提示符处
  重新出现，绝不在飞行途中出现。
- `enter_overlay` 在切备用屏**之前**隐藏（原来是之后），而 `draw_overlay` 再断言
  一次隐藏，于是切换 + 全屏绘制不会把光标拖进 overlay 里。overlay 从不重新显示
  它；Ctrl+O 返回的 `reflow` 把它重新坐回提示符。
- 在普通终端上隐藏不花什么：Hide + Show 落在同一次同步更新里，所以光标就是在
  提示符处重新出现，没有任何可见闪烁（与 ratatui 每次绘制所做的那套“先藏后显示
  在静息处”一样）。流中途“光标留在提示符行上可见”的行为不变 —— 帧与帧之间它
  是显示的、在提示符上，与以前完全一样。

### 为什么延迟是安全的（顺序）

- `set_view_height` 立刻改变*被跟踪的*高度，于是排队的行按最新高度冲刷 ——
  `StreamDone` 那个“在最终插入之前坐回空闲”的舞步仍然有效，而且现在作用于批里的
  每一行（严格比按过期的条带高度插入更少过度滚动）。
- 待处理的行只在对话视图里累积（overlay 按设计延迟提交 —— 不变量 4），而只有
  `draw`/`reflow`/`restore` 冲刷它们 —— 绝不是 `draw_overlay` —— 所以排队的行
  绝不可能被写进备用屏。
- 待处理的行永远是最新的内容，所以 `reflow` 的“从历史重新生成”覆盖了它们；在那里
  丢掉队列不损失任何东西。
- 每一条排队路径本来都会安排一帧；`restore` 是兜底。

## 提交清空 —— 已退休（2026-09-20）

`write_above` 从 ratatui 的 `insert_before` 里多留了一步：滚动与提交行之后，一个
`ESC[J` 把 live 区域清空，等着同一帧里随后那次重画。在同步更新之内它从不被呈现
—— Phase 15 的规则 —— 但一个不认模式 2026 的终端，在它刷新到点时会把已经解析的
东西原样渲染出来，而一个几 KB 的提交帧可能跨过一次 pty 读取，清空落在前半段：
一个没有盒子的已呈现帧，每提交一行一次。慢模型让这变成每秒一次闪
（`docs/ui/slow-stream.md` 量到过）。现在这个区域**原地**重画 —— blit 覆盖每一个
单元格 —— 而 `paint_frame` 只清掉上一个区域落在新区域下方的那些行，所以任何一帧
都不含无盒状态，无论有没有括号。Phase 121 把区域清空的次数钉在零。

## 与 codex 的已知差异

- codex 自己包装待处理的行（`HistoryLineWrapPolicy`，链接感知），并在支持的地方
  用边距区域滚动；我们的 `write_above` 保留我们本来就有的、更简单的 ratatui
  可移植滚动算法。
- codex *确实*在 resize 时重建 scrollback —— 它清掉屏幕 + scrollback 并重放
  transcript（`clear_terminal_for_resize_replay` + `reflow_transcript_now`），
  我们现在的 `ReflowClear::Purge` 模式就照它（见 `docs/design.md`）。codex 没有
  对应物的是**原子帧**那套处理：我们的 `reflow` 是自造的，把 purge + 重建包在
  一次同步更新里，而 codex 重排时不带这层框定。
- 没有 zellij 的特殊处理（codex 的 `InsertHistoryMode::ZellijRaw`）。

## 测试

`term.rs`/`main.rs` 是 I/O 边界（没有单元测试 —— 靠 `scripts/smoke.sh`）。关于
*怎么*测这里有一点要注意：`tmux capture-pane` 读的是 tmux 的**网格**，而网格随
字节解析而变 —— 同步更新保护的是终端*呈现*什么，不是一次解析中途的抓取采样到
什么。所以抓取采样无法把帧内无害的瞬态与真正 flush 出去的空帧区分开（修复后一个
锤击式探针仍能抓到约 0.4% 的无盒网格状态）。护栏改在**字节层**断言，那里这个性质
是精确的：

- **Phase 15** 录下整个流式回合的原始输出流（`tmux pipe-pane`）并走一遍：每一个
  `ESC[J` live 区域清空都必须位于 `ESC[?2026h` 与 `ESC[?2026l` 之间，于是一个
  认模式 2026 的终端绝不可能呈现那个无盒中间状态。两个方向都确定性 —— 修复前的
  构建录到 **14/14 次清空在任何同步块之外**；修复后录到 **0 次在外**（而且清空
  总数更少，因为一帧会合并它的多次提交）。录制在退出之前关闭：`restore` 的拆除
  清空本来就不带括号。
- **Phase 34** 录下一次 resize 前后的原始输出流（`tmux pipe-pane`，与 Phase 15
  一样）并走一遍：重排必须在它归位/清屏（`ESC[H` / `ESC[2J` / `ESC[3J`）**之前**
  发出一次光标 Hide（`ESC[?25l`），之后发出一次 Show（`ESC[?25h`）—— 光标拖尾
  细化的字节级证明。两个方向都确定性：修复前的构建在 resize 帧里录到 **0 次
  hide**（`scripts/cursor_hide_check.sh` 是独立的 Red→Green 探针），修复后录到
  第一次归位/清空之前的 Hide。
- 所有既有的 phase（流式底部钉住、resize、overlay 往返、打断、队列冲刷、
  `/clear`）端到端地检验这套延迟冲刷的顺序 —— 它们证明确实是排队的行按顺序落地，
  且盒子坐得正确。
