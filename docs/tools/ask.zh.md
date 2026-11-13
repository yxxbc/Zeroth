# `AskUserQuestion` tool —— 在回合中途问用户

Claude Code 的 `AskUserQuestion`：模型向用户问 1–4 个多选题，并**阻塞到用户
作答**，然后把答案作为 tool 结果读回 —— 或者，当用户十分钟都没碰它时，读到用户
不在，并继续干活（见下面 *The timeout*）。UI 是一个内联模态框 —— 权限 prompt
的兄弟 —— 带一条问题 tab 的芯片条、编号选项、多选复选框、一行自动添加的自由文本
“Other” 行、一个可选的带逐题备注的并排预览面板，以及（对多问题的调用）一个收尾
的复核并提交页。

```
────────────────────────────────────────────────────────────────────────────
 ←  ☒ Coffee style  ☒ Demo topics  ✔ Submit  →

 What's your favorite way to drink coffee?

 ❯ 1. Black ✔
      No milk, no sugar — just coffee
   2. Latte
      Espresso with steamed milk
   3. Cold brew
      Slow-steeped, served cold
   4. Type something.
   5. Chat about this

 Enter to select · Tab/Arrow keys to navigate · Esc to cancel
────────────────────────────────────────────────────────────────────────────
```

## 各个部件

这个特性完全按 tool 权限的方式来拆（`docs/state/permissions.md`）—— 那个接缝已经
被验证过，而这里是同一个形状：一个必须停在那儿等用户决定的 backend 线程。

- **`ask`（纯的部分 + 闸门）** —— 词汇与握手：
  - `AskQuestion`/`AskOption` 从 tool call 的 JSON 解析出来
    （`parse_questions`，校验 schema 的 1–4 个问题、每个 2–4 个选项）；
  - `AskAnswer`/`AskDecision` —— 用户产出的东西：`Submitted(answers)`、
    `Declined` 或 `Chat`（那行 “Chat about this”）—— 或者用户不在时，
    `TimedOut { answers, after }`（见下面 *The timeout*）；
  - 每种结局的两段文本：**cell 显示**
    （`answered_display` —— `User answered Zeroth's questions:`，这个
    headline 通过唯一的 `zeroth::APP_NAME` 点名*这个* agent，压在 `· Q → A`
    各行之上；`declined_display`/`chat_display` —— headline 压在
    `· Q (opt / opt / …)` 各行之上）与**面向模型的结果**
    （`answered_result` —— schema 的 `{"answers": {question: labels}}` JSON，
    外加携带备注/预览的 `annotations`；`declined_result`/`chat_result` ——
    停下等待的指示；`timed_out_display`/`timed_out_result` —— 继续干活那一版）；
  - `AskGate` —— `PermissionGate` 的 `Arc<Mutex<…>> + Condvar` 兄弟：
    `next_id` → `resolve(id, AskDecision)` → `wait(id, cancelled)`；
  - `AskTimer` —— 问题超时的空闲时钟，用注入的时刻保持纯（见下面
    *The timeout*）。

- **`llm` 那一半** —— `tools::ask_spec()`（函数定义，wire 名
  `askuserquestion`，只在 `LlmBackend::with_ask` 附上一个闸门时才提供
  —— 没有闸门的嵌入方永远不会暴露一个无法作答的 tool；subagent 永远拿不到
  它），以及 `llm::ask::ask_user` —— `approve_call` 的孪生体，backend
  的执行闭包把
  这个调用路由到它：解析、发 `StreamEvent::AskUser(request)`、**阻塞在闸门上**，
  然后把决定映射到一个 `ToolOutcome`，它新增的 `context` 字段把面向模型的结果
  与显示出来的 cell 文本并排放着。`run_agent` 把这个分裂的结果变成对应的事件：
  提交是 `ToolAnswered { display, result }`（绿色 —— `ToolRejected` 的孪生），
  拒绝/对话是 `ToolRejected`，所以记录下来的调用两份文本都留着
  （`ToolCall::context_output`），而派生的上下文重放的正是模型读到的那份
  （`context::context_messages` —— 免费，因为它本来就读
  `ToolCall::context_text()`；rollout 以同样的方式往返它）。

- **`app::AskPrompt`** —— 模态框状态，`PermissionPrompt` 的形状：打开时它拥有
  每一个键，**除了 Ctrl+O 和 Ctrl+D**，它把 composer 草稿暂存起来（问答的文本
  输入复用 `App::input`，像 Tab 的补正字段那样），并让跨模态的到达排队 ——
  一个问题开着时落下的权限请求等它轮到自己，反过来也一样
  （`open_next_pending`）。导航：←/→/Tab/Shift+Tab 在问题 tab 之间移动（以及
  Submit tab），↑/↓ 在两端回绕地移动各行，数字键跳选即激活，Enter
  选择/切换/激活，`n` 在一个带预览的问题上打开备注字段，Esc **拒绝**（整个调用
  解析为 declined —— 回合继续；模型被告知停下等待）。两个只读 overlay 是权限
  prompt 那个例外的兄弟（`docs/state/permissions.md`）：`on_key_ask` 先跑
  `App::on_key_overlay_toggle`，所以 Ctrl+O（transcript）和 Ctrl+D（原始上下文）
  能在一个开着的问题之上打开 —— 包括输入字段，两者都不是编辑键 —— 而回来时那个
  问题还在。一个*关于*这段对话的问题，绝不能锁住那两个展示它的视图。overlay 的
  空闲 Esc 也设了闸：一个待处理的模态框不是回溯目标
  （`App::overlay_esc_backtracks`）。单选的回答自动前进到下一个 tab；单独的
  一个问题
  则立刻解析；多选问题通过它自己那行不带编号的 `Submit` 确认。各输入字段
  （Other 行、备注行）是真正的 composer 字段：**Shift+Enter / Ctrl+J** 插入换行
  （换行后的行就地渲染，续行与文本对齐，而接受的答案保留那些换行），而一次
  **bracketed paste** 落下的方式与在 composer 里完全一样 —— 过了阈值就折叠成
  紧凑的 `[Pasted Content N chars]` 占位符（Backspace 整份删掉它），输入框退出
  时再拼回真实文本（`paste::expand_pastes_consuming` —— 只消费属于这个输入框
  自己的那些对，所以一个坐在暂存 composer 草稿里的占位符，等那份草稿最终被发送
  时仍然会展开）。Submit 页在提交会不完整时，以一行琥珀色的
  `⚠ You have not answered all questions` 警告开场，并且**只复核已作答的问题**
  （`● question` 压在绿色的 `→ answer` 上；未作答的那个被省略 —— 它的 ☐ 芯片和
  那行警告已经说明了），而且只有至少一个问题被作答时才提交 —— 一个都没答时按
  `Submit answers` 会跳到第一个未作答的问题。

- **`ui::ask_view`** —— 渲染器，`permission_view` 的兄弟：一个构造器
  （`ask_lines`）产出每一行（边框 → 芯片条 → 问题 → 选项 → 提示 → 边框），而
  `ask_height` 按夹取到终端的行数预留它们（`ui::region_is_modal` 覆盖这个
  prompt，所以关闭时像权限 prompt 那样 purge 重建）。终端短到放不下整页时，
  构造器拿出它可能拥有的行，构建第一个装得下的页面
  （`docs/ui/view-flow.md` *A page that sheds rows before it flows*），先甩掉
  选择所不需要的东西。在一个普通问题上，各选项的 description 先各自裁到一行，
  然后整个去掉；接着各行以选中的那行居中以三个为窗口，由 `↑ N more above` /
  `↓ N more below` 的槽位代表页面的缺口；最后才轮到芯片条 —— codex 的顺序，
  tab 留得最久 —— 所以问题、高亮行和提示都留在屏幕上（在 12 行里，dummy 的
  coffee 问题仍留着它的芯片条、问题、五行中的三行和提示）。一个带预览的问题先
  丢掉它那行暗色的 `Notes: press n to add notes`（按键提示仍提供 `n`；已输入的
  备注和打开的字段在每一页都留着），再把面板裁到选项列的高度，然后才丢掉芯片条。
  复核页保持它唯一那个形状，它的选择本来就在底部。比上述最后一种还高的页面，
  就像其余的一样是一个带框视图：绘制**底部锚定** —— 选项、提示和收尾边框都留在
  屏幕上 —— 而被跳过的顶部（芯片条、问题、前几个选项）**流入终端自己的真实
  scrollback**，由终端自身的滚动去读它。退役的那种「从顶部丢」的夹取，把那些行
  放进*任何*缓冲区之外，于是在一个小终端上读起来像是「模态框把文本藏了」：
  页面从一个选项中途打开，问题不见了。改变页面的按键（一次 tab 移动、一个回答、
  那个输入字段）会重新签署流程，边界随之 purge 重建 —— picker 搜索行的规则 ——
  而在已绘制的尾部里用普通 ↑/↓ 在行间移动，会保留已流出的顶部。芯片条把已作答的
  问题标 `☒`、未作答标 `☐`、Submit tab 标 `✔`，并把**当前**芯片点成青色选择
  背景。带选项预览的问题并排渲染：选项在左，聚焦选项的预览在右侧一个带框面板里，
  `Notes: …` 行在它下面。硬件光标在选项菜单上隐藏
  （`ui::cursor_visible`，权限那条规则），**而它的座位跟着高亮的 `❯` 行、落在
  选项文本那一列** —— 权限 prompt 的座位，由构造器连同各行一起记录，好让两者
  永不漂移，这也正是终端的光标动画（kitty 的拖尾之类）落脚的地方：正在被选的
  那个选项，在每一页（列表、预览、复核）上都一样，绝不在底部边框的远端 —— 而
  光标在 Other/备注文本字段上会回来（`ask_cursor`，共用构造器的几何；一个座位
  若其行已被底部锚定流入 scrollback，则回落到该区域的远角，也就是各菜单那条
  无标记的规则）。

- **已解析的 cell** —— `ui::tool` 对 ask tool 做特判：第一行输出
  （“User answered Zeroth's questions:”）成为 `●` 头（提交为绿色，拒绝/对话
  为红色），而 `· Q → A` 各行渲染在 `⎿` 装订边里，并换行 —— 也就是参考实现的
  已提交 transcript，用这个 agent 的名字。调用运行期间站着的是通用的
  `● AskUserQuestion(…)` 头（只在 Ctrl+O 里可见 —— 模态框盖住了 live 区域）。

- **循环** —— `Session.ask: AskGate`，始终附上（bootstrap → `ModelSession` →
  每一次 backend 构建，`DummyAi` 也包括在内；提问不是权限，不跟
  `ZEROTH_PERMISSIONS`）。`StreamEvent::AskUser` 打开 prompt；
  `Action::ResolveAsk`
  把决定投到闸门上；循环底部把被放弃的请求释放为 `Declined`（权限那次释放的
  孪生），而 `/clear` 清空面板。问题的**空闲时钟**（`ask::AskTimer`，由
  `tui::ask` 里的绘制 tick 读取）在它旁边跑，当超时时间内没人碰键时投出
  `TimedOut`。

- **dummy 演示** —— 一个 `Play::Asked` 场景（提示词：“ask” + “question”）离线
  驱动整个往返：三个问题 —— 单选、多选，以及一个带预览+备注的代码风格选择 ——
  通过真实的闸门解析，以共享的交接句子收尾（一次超时的运行以「在没有用户的情况下
  继续」收尾 —— `smoke.sh` Phase 128）。

## 流程

```
model calls askuserquestion
  └─ execute closure → llm::ask::ask_user
       ├─ parse_questions ── error → ToolOutcome::error (model retries)
       ├─ tx.send(AskUser(request)) ─────────► loop: App::open_ask (modal up)
       │                                      + the next draw starts its idle clock
       └─ gate.wait(id) … blocked …          user answers / declines / chats
            ▲                                 └─ Action::ResolveAsk
            ├──────── gate.resolve(id, decision) ┘
            │                                 …or no key for the timeout
            │                                 └─ App::expire_asks (draw tick)
            └──────── gate.resolve(id, TimedOut) ┘
       decision → ToolOutcome { output: display, context: result, ok }
  └─ run_agent → ToolAnswered / ToolRejected → the committed cell
       └─ result appended as the tool message → the model reads the answers
```

Esc/`/clear`/退出通过回合的 `CancelToken` 收割那个被阻塞的线程（闸门的 `wait`
轮询它）；一个没有答案就被丢下的请求在循环底部被解析为 `Declined`，所以没有
任何线程会永远停着。

## 超时 —— 当没人作答

一个问题会阻塞回合，而用户可能已经走开了。在这之前，向一个空房间提的问题会让
agent 一直停到用户回来，无论那要多久 —— 一个午休期间还留着的长达一小时的任务，
就把那一小时花在等它的第一个问题上。现在，一个问题对**空闲**用户等**十分钟**
（`ask::DEFAULT_ASK_TIMEOUT`），然后*未作答*地解析：tool 返回，模型读到用户不可
用，回合继续往下走。

- **是空闲，不是流逝。** 只要还有问题待处理 —— 打开的模态框，或者排在权限 prompt
  后面的那个 —— 时钟就跑，而**每一次按键或粘贴都让它重新开始**，无论那个键落在
  哪儿：一个选项、一个输入字段、在模态框之上打开的 Ctrl+O transcript。一个在读
  选项、在打答案、或者在滚动对话以便决定的用户，永远不会把它跑光；十分钟没被碰
  过，就意味着用户不在。
- **模型读到什么**（`ask::timed_out_result`），很短，因为它要搭上之后每一个
  请求：

  > The user did not answer within 10m and is not available. Continue working
  > with your best judgment and state your assumptions. Do not ask again until
  > the user sends a message.

  它与拒绝的「停下等待」正相反：拒绝是用户*在场*说不，超时是用户*缺席*。
  「state your assumptions」是用户回来时读到的东西；「do not ask again
  until the user sends a message」让下一个问题不至于又在同一个空房间上卡掉
  十分钟 —— 作用域限于下一条消息，所以它永远不会活得比用户的归来更久。仅此
  而已：怎么选是模型自己的判断，而每多一个分句都要搭上之后每一个请求。
- **部分的回答会保留。** 一个先选了三个问题里的第一个然后离开的用户，给出的是
  真实答案，所以超时会把它交付：`TimedOut { answers, .. }` 携带与一次 Submit
  相同的已作答集合（一个输入字段里未被接受的文本不算回答，而它的粘贴对也随之
  消失），而结果变成*用户回答了部分问题，然后 10m 没回应……*，最后是答案 JSON。
  一个还在排队 —— 从未显示过 —— 的问题，以没有答案解析。
- **cell** 是拒绝的形状，红色：`User did not answer within 10m`
  （有东西被作答时是 `User did not finish answering within 10m`），压在每份答案
  一行 `· Q → A` 与每个被留空的问题一行 `· Q (A / B)` 之上。
- **倒计时。** 时钟跑的时候，模态框的收尾边框把它右对齐地带上 ——
  `── Timeout: 9:41 ─` —— 作为一个点亮的芯片，也就是 agent 会话视图标签的装束：
  主题的强调色配它的 on-accent 墨色，留白在填充里面、之后一个裸边框格，所以它
  一眼可读，而不是一条暗边框里的暗字；在最后一分钟里芯片点亮警示琥珀色。之所以
  放在边框上，是因为那是底部锚定的页面永远会绘制的那一行：一个在那里跳动的标签
  永远不会重新签署流程（`docs/ui/view-flow.md`），也永远不让页面多花一行。计数
  向上取整，所以一个新鲜的时钟读作 `10:00`，最后一秒读作 `0:01`，绝不会在一个
  仍然开着的问题上读作 `0:00`；一条装不下它的窄边框保持朴素。
- **为什么是十分钟。** 长到足够一个在场但很忙的用户 —— 从另一个窗口回来、在读
  预览、去倒了杯咖啡 —— 因为任何键都会让它重新开始；过了这个点，用户就是不在，
  而再多的每一分钟都是 agent 在为一个没有人的房间空转。二十分钟把那个空闲时间
  翻倍，对常见情况也仍然接不住一场会或一顿午饭，而且一个 provider 的 prompt
  缓存到十分钟时通常已经过期了，所以更长的等待什么也买不回来。
- **改它。** `/settings` 的 **Ask timeout** 行循环 `5m` / `10m` / `20m` /
  `30m` / `1h` / `never`，按目录（`docs/state/settings.md`）；
  `ZEROTH_ASK_TIMEOUT_SECS` 以秒为一次运行播种它，`0` 意为永不 —— 任何值都行，
  smoke 套件正是这样把一次等待在八秒内坐过去（Phase 128）。

时钟的规则是纯的 —— `ask::AskTimer`，用注入的时刻读取，并像这里其他一切一样有
单元测试 —— 只有时间本身住在边界上（`tui::ask`），那是 toast 截止时间的模式。
计时器保留的是**等待从何时开始**，绝不是截止点：从不把一个等待加到一个
`Instant` 上，所以一个超过时钟能达到的任何时刻的
`ZEROTH_ASK_TIMEOUT_SECS`，读出来会是一个极长的倒计时，而不是溢出。每次读取
（`AskTimer::tick`）拿到是否有问题在等（`App::has_pending_asks`）、开着那个的
id（另一个打开就重新开始这次等待，所以每个问题都在屏幕上得到完整的等待）以及
那一行的等待，并回答 `Idle`、`Running(left)` 或 `Expired(wait)`；
`AskTimer::touch` 是每一次按键或粘贴（`Session::note_user_activity`），它会把
一个跑着的等待重新开始，而在没有东西等待时不会起算一个。绘制 tick 在绘制
**之前**读它（`Session::tick_ask_clock`）。过期时，`App::expire_asks` 关闭
模态框 —— 把 composer 草稿交回 —— 并丢掉每一个排队的问题，而每个决定都从循环
自己的线程、像一次回答那样精确地投上闸门，所以按键与过期永远不会抢跑；被停住的
tool 线程在 `ask_user` 的 `TimedOut` 分支里醒来，同一帧把模态框画成关闭，而排在
它后面的一个权限 prompt 随之打开。运行时，开着的模态框的剩余时间被注入
（`App::set_ask_remaining`），并且保留一帧预约在一秒之外 —— 不晚于过期时间，
上限一小时，在 overlay 之下则全部如此，那里什么都不在屏幕上、状态链也不再重新
预约 —— 所以过期根本不需要任何输入就能触发。一次落在 Ctrl+O transcript 之下的
超时会在它底下关闭模态框，而 transcript 随着回合继续而跟随。

tool 自己的 description 把这件事留给结果：用户「也可能拒绝或要求聊一聊；结果会
告诉你接下来怎么走」。它过去承诺每一个未作答的结果都意味着停下等待，而超时的
继续干活结果与此矛盾 —— 现在它也不点超时这件事，因为结果说明了超时发生时该做
什么，而 description 要搭上每一个请求。
