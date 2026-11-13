# 生命周期 hook

Claude Code 和 codex 都允许用户把自己的命令楔进 agent 的生命周期：每次
`edit` 之后跑一个 linter、拒绝一条匹配某个模式的 `bash` 命令、在 session
打开时给模型喂额外的 context。这里做的就是那个功能，移植到了本代码库。

```
● Bash(rm -rf build/)
  ⎿  Blocked by PreToolUse hook: no destructive deletes outside ./tmp
     Hook: ~/.zeroth/hooks/no-deletes.sh
     Config: ~/.zeroth/hooks.json
```

**`/hooks`** 命令在这份配置之上打开一个只读的内联浏览器 —— 哪些事件有
hook、在哪个 matcher 之下、跑的是什么 —— 见 `docs/tools/hooks-menu.md`。

## 契约

codex 有一个 `codex-rs/hooks` crate，其引擎类型的名字字面上就叫
`ClaudeHooksEngine`：它实现的是 **Claude Code 的** hook 线上契约，而不是
自创一套。我们瞄准的是同一份契约，所以为这两个工具中任何一个写的 hook
脚本，在这里都能原样工作。

配置是一个 `hooks.json`，形状是 事件 → matcher 组 → handler：

```json
{
  "description": "optional, ignored",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "bash|write",
        "hooks": [
          { "type": "command", "command": "./guard.sh", "timeout": 60 }
        ]
      }
    ]
  }
}
```

handler 以 **stdin 上的 JSON** 接收它的事件载荷，用 **stdout 上的 JSON**
作答。这两半的拼写方式不同，两份参考实现都对这种不对称达成一致，所以我们
照抄：

- **stdin（载荷）是 `snake_case`** —— `session_id`、`transcript_path`、
  `cwd`、`permission_mode`、`scratchpad_dir`、`hook_event_name`、
  `tool_name`、`tool_input`、`tool_use_id`、`agent_id`、`agent_type`。
  `session_id` 是**对话的** id —— rollout 文件的、`--resume` 所接受的
  句柄 —— 所以一个把它记下来的 hook 可以把它交回给
  `zeroth --resume`。它在文件存在之前就已确定（一个新对话从它的第一个
  事件起就有 id），在 `/clear` 时换掉，在 `/resume` 时变成被恢复文件的
  id；`transcript_path` 随它一起走。第一个对话的文件以 session 的启动 id
  命名，所以在一次 `/clear` 之前，这个 id 也是临时树的 id
  （`docs/tools/scratchpad.md`）—— 但 `scratchpad_dir` 才是 handler 在
  一次 `/clear` **之后**找到该 session 的 scratchpad 与任务输出的方式，
  这也正是 Claude Code 为它准备的字段。（`session_id` 过去是进程的 id，
  `/clear` 从不移动它，`--resume` 也接不了它。）
- **stdout（裁决）是 `camelCase`** —— `continue`、`stopReason`、
  `suppressOutput`、`systemMessage`、`decision`、`reason`、
  `hookSpecificOutput.{hookEventName,permissionDecision, permissionDecisionReason,updatedInput,additionalContext}`。

建模了十四个事件。每一个能返回什么：

| 事件                             | hook 可以返回                                          |
| ------------------------------- | ----------------------------------------------------- |
| `PreToolUse`                    | allow / deny / ask · block 加理由 · `additionalContext` · `updatedInput` |
| `PermissionRequest`             | `allow` / `deny` 加一条消息                              |
| `PostToolUse` / `PostToolUseFailure` | block 加理由 · `additionalContext`                |
| `UserPromptSubmit`              | block 加理由 · `additionalContext`                      |
| `Stop` / `SubagentStop`         | block 加理由（让 agent 继续跑）                          |
| `SessionStart` / `SubagentStart`| `additionalContext`                                    |
| `SessionEnd`                    | 什么都不返回 —— 输出被忽略（两份参考实现都如此）            |
| `StopFailure` / `Notification`  | 什么都不返回 —— 输出被忽略（Claude Code 的规则）           |
| `PreCompact`                    | `additionalContext` / stdout → 额外的 compact 指令 —— **不能 block**（见下文） |
| `PostCompact`                   | 什么都不返回 —— 只有信封                                 |

**退出码**（Claude Code 的语义，逐字照搬）：`0` —— 成功，stdout 以 `{`
开头时被读作裁决，否则当作纯文本。`2` —— **block**，而理由是 **stderr**，
不是 stdout；对以权限作答的 `PermissionRequest` 来说，block 就是
**deny**（它过去会漏到 classifier 和 prompt 上），而在一个没有什么可 block
的事件上 —— `SessionStart`、`SubagentStart`、`PreCompact`、`PostCompact`
—— 理由只展示给用户，落在 hook 运行单元格上（*hook 运行单元格*）。其他
任何非零值 —— 一个不 block 的错误，抛给用户，其余一概忽略。以 `{` 开头但
解析失败的 stdout 同样是不 block 的错误，绝不会悄悄放行。

**合并。** 当多个 handler 匹配同一个事件时：**任何**一个 block 都胜出，
理由取**第一个**，各 context 按 handler 顺序**拼接**，而对 `PreToolUse`
来说 `deny` 高于 `ask`，`ask` 高于 `allow`。

最初十一个之后加的那三个事件带上 Claude Code 自己的字段。
`PostToolUseFailure` 有 `tool_name`、`tool_input`、`tool_use_id`、失败
本身的文本作为 `error`，以及 `is_interrupt`（永远是 `false` —— 一次被中断
的调用在这里不会触发任何 post-tool 事件）。`StopFailure` 有 `error`
（种类，见下文）、`error_details` 和 `last_assistant_message`。
`Notification` 有 `notification_type`、`title` 和 `message`。

有两个载荷字段是**扩展**，因为这份契约没有为这个应用才有的事实留位置：
`tool_response` 是 `{"output": …, "success": …}`（我们的 tool 返回文本，
而不是结构化的结果，而一个稳定的形状胜过有时是字符串），以及
`SubagentStop` 携带 `success`。后者的存在是因为那个显而易见的捷径是错的：
`stop_hook_active` 的意思是*这次运行是被某个 `Stop` hook 的 block 启动的*
—— 那个循环守卫 —— 所以用它来报告一个失败的 agent，会在一个已有文档的
名字下把错误的事实交给 hook 作者。它在这里永远是 `false`，因为还没有什么
会从 hook 继续一个 agent。codex 以同样的方式扩展这些载荷，用的是
`turn_id`。

## matcher

两份参考实现共用同一份实现，我们也是（`hooks::matcher::matches`）：

- 缺失、空，或 `*` —— 匹配一切。
- 每个字符都在 `[A-Za-z0-9_|]` 里 —— **快路径**：按 `|` 切分、去掉空白，
  然后比较**完全相等**。不编译任何正则。注意这意味着 `bash` 刻意*不*匹配
  一个假想的 `bashoutput`。
- 其他任何东西 —— 当作正则编译。

最初的设计提议遇到真正的正则就警告并跳过，以避免引入 `regex` 依赖。结果
那点开销几乎不存在：`regex` 只是 `regex-automata` 引擎之上的一层薄包装，
而 `fancy-regex`（token 计数器的切分模式，后来已被一个免词表的估算取代
—— `docs/architecture/token-estimate.md`）早就把它拉进了每一次构建，
`regex` 现在是这个 crate 唯一的正则引擎。既然 `^Bash$` 是真实 Claude Code
配置里常见的 matcher，悄悄跳过它就是一个没有对冲收益的坑，所以我们改为
编译它。编译失败的 matcher 会被警告并跳过。

既然 `regex` 已归这个 crate 独享，它的 `perf` 特性就被去掉了
（`default-features = false`，`std` + `unicode`）：惰性 DFA 与单趟 DFA、
回溯器以及 `aho-corasick` 预过滤器加速的是在长文本里搜索，而一个 matcher
只会对一个 tool 名字试用。这个 crate 承诺有没有它们都得到同样的匹配，
`unicode` 让 `\w`、`\d` 和 `(?i)` 保持原样，而二进制小了 0.57 MB
（`docs/ops/binary-size.md`）。

**tool 名字对两种拼写都应答。** tool 事件的匹配查询用的是这个应用自己的
tool 名 —— `bash`、`read`、`write`、`edit`、`agent` —— 小写，模型看到的
样子，而每一个也以自己的 Claude Code 拼写作为**第二个精确名字**应答
（`Bash`、`Read`、`Write`、`Edit`，`agent` 则是 `Task` —— 参考实现自己的
遗留别名先例，`permissionRuleParser.ts`）。所以 `"matcher": "Bash"` ——
野外最常见的单一 hook 配置 —— 和 `^Bash$` 在这里都选中 `bash`，而
`bashoutput` 仍然两者都不匹配：别名是第二个精确名字，永远不是前缀或大小写
折叠规则。bash 的同伴们（`docs/tools/bash-tools.md`）没有可借的参考拼写，
所以每一个的第二个名字就是它的单元格所显示的那个 —— `bashsend` 应答
`BashSend`，`BashWait`、`BashKill`、`BashList` 同理，遗留的
`bash_session` 应答 `BashSession` —— 而 `Bash` 从不选中它们：它是一个
精确名字。agent 类型（`SubagentStart`/`SubagentStop`）以及其他非 tool 的
查询永远没有别名。

## 每个事件挂在哪里

| 事件                 | 位置                                                             |
| -------------------- | --------------------------------------------------------------- |
| `PreToolUse`         | `llm/agent.rs` —— 在 `approve` 之前；**包含 `agent` 启动** |
| `PermissionRequest`  | `llm/approval.rs` —— 在常设规则之后                              |
| `Notification`       | 由 `llm/approval.rs` 登记；它在循环打开 prompt 时运行，走通知车道 |
| `PostToolUse`        | `llm/agent.rs` —— 在 `execute` 返回时，**仅成功**                 |
| `PostToolUseFailure` | `llm/agent.rs` —— 在 `execute` 返回时，**仅失败**                 |
| `SubagentStart`      | `llm/backend.rs` —— 在 `spawn_subagent_run` 内部                  |
| `SubagentStop`       | `llm/agent.rs` —— `run_agent` 的 Complete 分支，经由带标签的 sink |
| `SessionStart`       | `tui` —— 对话打开时在后台触发；下一次 spawn 的顶部会等它          |
| `SessionEnd`         | `tui` —— `/clear`、`/resume` 与退出，在一个 2 s 的事件预算之下    |
| `UserPromptSubmit`   | `llm/backend.rs` —— spawn 的顶部，在 SessionStart 的等待之后      |
| `Stop`               | `llm/agent.rs` —— `run_agent` 的 Complete 分支                    |
| `StopFailure`        | `llm/agent.rs` —— 一个失败的轮、tool 调用上限，或一个始终没跑完的回复 |
| `PreCompact`         | compact spawn 的顶部，经由 `CompactHooks` 包装                    |
| `PostCompact`        | compact spawn 的完成，经由同一个包装                              |

十四个事件每一个都会触发。没有一个跑在 tokio 循环上：tool、回合与 compact
事件跑在 **backend 线程**上，`Notification` 跑在通知车道的 worker 上
（循环只把它交出去），而边界的 session 事件跑在自己的线程上
（`SessionStart`），或在一个硬预算之下（`SessionEnd`）—— 这就是下面那个
故事的后一半。

`PermissionRequest` 的位置正是 auto 模式 classifier 的位置 —— 在常设
allowlist 之后、用户之前 —— 因为它回答的是同一个问题：*这件事能不能不问
就跑？* 两个值得说明的后果：一条已被常设规则覆盖的调用永远到不了它（没有
人在问），而权限 gate 整个关掉时（`ZEROTH_PERMISSIONS=0`）任何调用也
到不了它，因为根本没有权限裁决要做。`PreToolUse` 在这两种情况下仍然触发
—— 它是关于调用的，不是关于询问的。

### 一个执行 context —— 一个自行消解的前提

最初的阶段划分假定那六个「循环路径」事件必须跑在 tokio 的 current-thread
循环上 —— 一个阻塞的子进程在那里会冻住 spinner 和键盘 —— 因而挂上它们就
需要一个本代码库没有的 pending-turn 状态机。查证参考实现杀掉了这个前提：

- **Claude Code 在查询循环内部触发它的 stop hook**，而不是在 REPL 层 ——
  一个 `Stop` block *就是*那个循环的下一次迭代（`query.ts:1282-1305`），
  在这里恰好就是 `run_agent`。
- **两份参考实现都不让 `SessionStart` 阻塞第一次绘制。** Claude Code 在
  渲染之前就把它踢起来而不等待，只阻塞*第一次 API 调用*
  （`main.tsx:3761-3765`）—— hook 确实在启动时跑了，在后台；codex 排入
  一个待处理来源，在下一回合顶部排空它（`turn.rs:239-241`）。我们现在
  跟随 Claude Code 的时机（见下文），而第一次绘制无论哪种方式都保持空闲
  —— checkpoint 探测的教训，两份参考实现在我们撞上它之前就已经遵守了。

所以每个事件都落在 **backend 线程**上 —— 阻塞在那里已经是正常且正确的
（permission gate 就停在那里的一个 `Condvar` 上，`llm::exec` 每 20 ms
轮询它的子进程）—— 而那个循环线程的问题从来就不存在：

- **`Stop`/`SubagentStop`** 在 `run_agent` 里于
  `RoundOutcome::Complete` 处触发，在 `StreamDone` 之前 —— 只对真实回答
  触发：空回复会先被继续，而一个放弃继续的回合改由 `StopFailure` 收尾
  （`docs/architecture/unfinished-replies.md`）。一次 block 会把到此为止的
  回复作为 assistant 消息追加、把反馈（`Stop hook feedback: {reason}`，
  其下是 hook 的 `Hook:` 与 `Config:` 行 —— 见下文*模型读到什么*）作为下
  一条 user 消息追加、翻转 `stop_hook_active`，然后**同一个回合再跑一轮**
  —— 所以状态行继续工作，队列与 token 统计不受影响，而
  `dispatch_after_turn` 的回合末尾 checkpoint 落在每一次继续*之后*：
  formatter hook 的写入在回合的快照之内，Esc-Esc 回溯能把它们恢复。按构造
  它从不为一个 `/compact` 回合触发（那个 backend 带着下面的包装），不为
  `!` shell 触发（没有 agent 循环），不为 backend 错误触发（那是
  `StopFailure` 的，见下文），也不为一次中断触发（在 abort 上 Claude Code
  在 stop hook 之前就返回了 —— Esc 总会打断一条继续链）。引擎从不拒绝再次
  block：`stop_hook_active` 是 hook 自己的守卫，参考实现的确切姿态。带标签
  的 sink 把 subagent 的完成路由到 `SubagentStop`（matcher = agent 类型），
  在 agent 自己的循环上有同样的 block-即-继续语义；一个被杀掉或出错的
  agent 什么都不触发。
- **`StopFailure`** 在一个回合以错误而不是回答收尾的地方触发：一个失败的
  轮（有界重试放弃之后）、**Max tool calls** 上限，或一个经过各次继续仍然
  没跑完的回复（`docs/architecture/unfinished-replies.md`）。它的
  `error` —— 也是 matcher —— 是从失败里读出的 Claude Code 错误类型
  （`llm::hooks::stop_failure_kind`）：`rate_limit`（429）、
  `overloaded`（529）、`authentication_failed`（401/403）、
  `billing_error`（402）、`model_not_found`（404）、
  `invalid_request`（其他任何 4xx）、`server_error`（5xx、一次传输失败、
  一个不可达的主机）、回复一直卡在输出上限被截断时的 `max_output_tokens`，
  其余的则是 `unknown` —— 包括那个上限，以及一个从未回复的模型。输出被
  忽略，与参考实现一致。一次中断什么都不触发（Esc 不是失败），subagent
  失败的轮也不触发：这个事件是 session 的回合结束。
- **`SessionStart`** 在**对话打开时在后台**触发 —— Claude Code 的时机：
  用户可以立刻打字，而第一个回合会等这些 hook，好让它们的 context 仍然
  落地。边界在第一次绘制入队之后立刻触发 `startup`（`--continue`/
  `--resume` 启动时为 `resume`；裸 `--resume` 要等它的 picker，其选中项
  才是真正开始的那个对话 —— 若被关掉，则它最终落入的那个新对话那时才开始，
  记为 `startup`），在 `/clear` 之后触发 `clear`，在 `/resume` 载入之后
  触发 `resume` —— 每一次都在 recorder 移动之后，所以载荷命名的是新的
  对话（matcher = source）。这次运行被保存在 `HookHandles::started` 里；
  下一次 spawn 的顶部等它 —— 轮询该回合的 `CancelToken`，所以 Esc 依旧
  及时，而一次被取消的等待会让这次运行留给下一个回合 —— 并取走它的
  context，那会作为 user 消息落在 prompt 之后并被记录（见下文
  `HookNote`）。同一时间只保存一次运行：后来的触发会替换并杀掉一次没有
  任何回合读取的运行，因为它的 context 属于那个已经结束的对话。边界没能
  触发的 source —— hook 关着，或者还没有真正的 backend（dummy 不跑任何
  hook）—— 保持排队，在第一个两者都具备的回合运行，即 codex 那个旧的
  drain。
- **`UserPromptSubmit`** 在那次等待之后、第一个请求之前触发。一次
  **block** 会送出 `StreamEvent::PromptBlocked` 而不是别的东西：循环把刚
  记录的那次提交从历史里滚出去（recorder 的收缩重写把它从 rollout 抹掉
  —— 参考实现的「从 context 中抹除」，所以被 secrets 过滤器查禁的内容
  什么都到不了后面的回合），把文本还给 composer，purge 重绘抹掉那份回显，
  并提交一条红色的一行通知，`Prompt blocked by UserPromptSubmit hook:
  {reason}`。`additionalContext` 即使在 block 时也注入 —— 两份参考实现的
  规则。循环自发的回合（后台完成的 follow-up）被标记为合成并跳过这个事件：
  它的 prompt 不是用户的。只有运行该回合 hook 的 sink 才会消费这个标记，
  所以每一个**用户**回合在 spawn 之前都会清掉它 —— 一个在 hook 关着时、或
  在离线 demo 上设下的标记，过去会活过它自己的回合，并让用户的下一个真实
  prompt 沉默。rollout 在该回合的线程存在之前也已同步好了（codex 的规则：
  文件在 hook 运行之前就存在），所以一个对话的第一次 `UserPromptSubmit`
  已经命中了它的 `transcript_path`，而不是与创建文件的循环底部同步竞争。
  backend 只在这个检查之后才查阅它保留的请求前缀，所以一次 block 不花费
  任何 prompt 缓存前缀（`docs/providers/prompt-caching.md`）。
- **`PreCompact`/`PostCompact`** 经由 `CompactHooks` 包装搭上摘要化那次
  spawn：它的「prompt」hook 是 PreCompact（context/stdout → 额外的摘要
  指令；**它不能 block —— 两份参考实现都是如此**：Claude Code 自己的应用内
  文档声称 exit 2 能 block 压缩，但 `blocked` 字段在任何 PreCompact 调用点
  都从未被读取，而 codex 也没有为它准备 exit-2 分支），它的完成是
  PostCompact。这个包装从不排空 session-source 队列，也从不继续一次摘要化；
  它的注记不进对话记录（它们是短暂的，就像 compact 的各块本身）。
- **`SessionEnd`** 在一个对话关闭时触发 —— `/clear`（`clear`）、一次
  `/resume` 切换离开（`resume`）、一次退出（`prompt_input_exit`）以及一次
  headless 运行的结束（`other`，`docs/cli/headless.md`），都是 Claude Code
  的理由 —— 在 recorder 移动**之前**，所以它命名的是那个结束的对话，且在
  一个 **2 s 的整事件预算**之下：Claude Code 把这个事件封顶在 1.5 s，
  codex 夹到 3 s，因为退出绝不能挂在一个 hook 上。一次退出是在回合和每个
  agent 都被取消**之后**才触发它，所以它们的任何 hook 都不可能落在 session
  结束之后。输出被忽略，与两份参考实现一致。任何没有被回合读过的
  `SessionStart` context 会先被丢弃（若其 hook 还在跑则杀掉）。它**只为
  一个起点已经触发的对话**触发，以保持这对事件的诚实：如果一个
  `SessionStart` source 还在排队 —— hook 关着、没有真正的 backend（一个
  只有 dummy 的 session，不跑任何 hook）、一次裸 `--resume` 启动还停在它
  的 picker 里 —— 这个对话对 hook 而言从未开始，所以不会有 `SessionEnd`
  触发，待处理的 source 也随之作废。一个记录 session 的 hook 绝不会只看到
  结束而看不到开始。会话中途的一次 `/login` 会让这对事件成立：下一个回合
  运行排队的 `startup`，而结束在退出时触发。
- **`PostToolUseFailure`** 是 `PostToolUse` 为一次跑过且**失败**的调用
  （`ok: false` —— 非零退出，或一次找不到待改文本的 edit）准备的孪生体，
  答案也一样：一次 block 是模型必须读的反馈，而 `additionalContext` 追加到
  结果之后。一次被拒绝或被 block 的调用从未运行，所以它两者都不触发，而
  一次交给后台的调用尚未完成，所以它也暂时两者都不触发。
- **`Notification`** 在 permission prompt **出现在屏幕上**的那一刻以
  `notification_type` 为 `permission_prompt` 触发 —— 那一刻一个人才真正
  在被询问，而这是 `PermissionRequest`（在 classifier 之前被咨询）说不出
  的。标题 `Permission needed`，消息 `Zeroth needs your permission to
  use {Tool}`，matcher = 那个类型。Claude Code 的其他通知类型（idle、
  auth）没有建模。

  *在屏幕上*不等于*被抬起*。一个请求可以在另一个 prompt 占着屏幕时到达并
  排在它后面，也可以在排着的时候无声地了结：一条覆盖它的「别再问了」、它
  的 agent 被停止、一次 Esc。所以抬起 prompt 的那个线程只在 permission
  gate 上**登记**这条通知（`PermissionGate::on_shown`，在请求发出之前，这样
  循环不可能没看见就打开它），循环在打开它的那次迭代底部把该 prompt 标记为
  已显示（`mark_shown` —— 那时开着的 prompt 就是下一帧要绘制的那个），而
  真正运行它的是这个。一个先了结的请求会丢弃它登记的东西
  （`forget_shown`）：没有一个看见的 prompt 不通知任何人。

  它只是观察，绝不能拖住 prompt，所以它不能在发问的那个线程上跑 —— 而跑到
  别处又没有顺序的话，一个慢的通知器会落在它所宣告的那次调用的
  `PostToolUse` 之后，或落在下一个 prompt 的通知之后，于是一个维护某种状态
  的 hook（比如多路复用器的「在等你」标记）就结束在错误的那一个上。所以
  通知跑在这个 session 的**通知车道**上（`NotificationLane`，挂在
  `HookHandles` 上，好让每个 sink 和带标签的 subagent sink 共用它）：一个
  worker 线程，一次一条通知，按它们 prompt 被显示的顺序；一个在排队期间
  回合已结束的任务什么都不运行，而一次 Esc 会收割一个挂住的。其他每一个
  有 handler 要跑的分发都先等已经显示的通知 —— 轮询它自己的
  `CancelToken` —— 而 `SessionEnd` 在它自己那 2 s 预算之内也这么做。prompt
  本身从不等待：gate 的等待在通知运行之前就已在进行，而在一个慢通知器
  运行期间给出的答案会立刻被采纳 —— 只有这次调用的下一个 hook 事件在等。

**一个阻塞的 hook 必须轮询 `CancelToken`。** 既有那种 `Condvar` 停车之所
以安全，是因为 `PermissionGate::wait` 接受一个取消谓词，而 `run_bash` 每
20 ms 重新检查一次取消。hook runner 以同样的 20 ms 节奏轮询 —— 载荷的写入
也算在内，在它自己的线程上，因为一个会填满管道缓冲的载荷喂给一个从不读
stdin 的 handler，过去会把一次内联写入停在 Esc 和超时两者之后 —— 并在取消
时杀掉进程组，所以 Esc 依旧及时。

**hook 自己退出，就结束了等它的输出。** 它的 stdout 和 stderr 在各自的
线程上读取，而一旦 hook 的进程退出，runner 给它们 `PIPE_DRAIN_GRACE`（1 s）
去送完最后几个字节 —— 而不是等到管道一直开着为止。一个启动后台进程的 hook
（`server &`，一个守护化的通知器）会把自己的管道交给那个进程，而等它们结束
就把事件 —— 以及那个回合，或那次 tool 调用 —— 扣到进程活多久就扣多久。
在宽限期里到达的才是 hook 的输出；那时仍在跑的读取者被留下来排空剩下的。

## 一个裁决怎么到达屏幕

这次审计最有价值的一条发现是：本代码库早已为每一个 hook 裁决准备好落点，
只是叫了别的名字。**没有新增任何 `StreamEvent` 变体**，也无需新增：

- **一次 block** 就是 `Approval::Reject { display, result, kind }` ——
  permission gate 自己的两段文本拒绝。它本来就渲染成一个红色单元格、
  本来就把更长的面向模型文本留在 `ToolCall::context_output` 上、本来就
  走完 rollout 的往返，而且本来就正确显示在 Ctrl+D 里。一个 hook block 的
  单元格会说出是哪个事件的 hook 拒绝的、是哪个 hook、以及它配置在哪个
  `hooks.json` 里；模型读到的也是这些，外加一行该怎么做（见下文
  *用户看到什么* 与 *模型读到什么*）。一个既 block 又有话要说的 hook
  **两处都会被听到**（`llm::agent` 的 `refuse_by_hook`，`agent` 启动也
  包含在内）：它的 `additionalContext` 跟在模型读到的拒绝之后，而它的
  注记 —— 一个兄弟 handler 的警告、一个 `systemMessage` —— 坐在红色单元格
  之下。这两者过去都随调用一起被丢掉。
- **一次 `PreToolUse` allow** 是 `Approval::AllowNoted { note }` ——
  auto 模式 classifier 那条既有的「这是在没你的情况下跑的」来源行。
- **`additionalContext`** 搭乘 `StreamEvent::ToolNote`，成为用户看到的
  那条暗色 `⎿` 行，并追加到**面向模型**的结果之后，在它的
  `{event} hook additional context:` 标签之下，经由既有的
  `ToolAnswered { display, result }` 拆分，所以单元格保持干净，而
  `context_output` 携带模型实际读到的内容。两者都被记录，两者都能挺过一次
  `/resume` —— 交给后台的调用也在内，它被记录的启动文本过去会在读到它的
  那个回合之后丢掉 context。

对话级的事件需要两处新增，因为既有的东西无法表达它们：

- **`StreamEvent::HookNote { label, text, report }`** 携带 hook 注入的
  对话文本 —— 一个 Stop block 的反馈、一个 SessionStart/
  UserPromptSubmit hook 的额外 context —— 并且在 hook 说了话时，携带那个
  **report**，也就是它们的单元格（见下文 *hook 运行单元格*；对一个没有给
  对话增加任何内容的 report，`text` 此时为空）。循环先把在它之前的
  assistant 运行收尾（不变量 4 的「交叠之前先 flush」），记录一条
  **`HistoryItem::HookNote`**，并提交 report 的单元格；一条没有 report 的
  注记保持无单元格的内联形态，在 Ctrl+O transcript 里以它暗色的
  `● {label}` 标题下显示。这段文本由 `context_messages` **逐字**重放为
  模型实际读到的那条 user-role 消息，并经由 rollout 往返，好让一次
  `/resume` 留住它。context 的线上文本用 Claude Code 的确切形状 —— 被
  `<system-reminder>` 包住的 `{event} hook additional context: …`。agent
  循环自己的继续 prompt 也搭乘它 —— 一条空回复或一段被截断的回复所得到的
  那条注记，标题是 `Empty reply` / `Reply cut off`
  （`docs/architecture/unfinished-replies.md`）—— 因为一次继续所需要的
  恰恰就是一个 Stop block 的反馈所得到的。
- **`StreamEvent::PromptBlocked { reason }`** 是一个 `UserPromptSubmit`
  block 的终结事件 —— 代替 `StreamDone` 发出，后面什么都不跟；循环的那个
  分支负责上面说的回滚。

唯一**确实**需要的一处改动：`ToolAnswered` 和 `ToolRejected` 多了一个
`truncated` 标志。它们本来没有，因为它们唯一的用户 —— ask tool 和一次
权限拒绝 —— 从不产生被封顶的输出。把一次**已执行**调用的结果也路由过它们
就有了需要，所以没有它时，一个被 hook 改写、输出撞上
`TOOL_OUTPUT_MAX_BYTES` 的 `bash` 会悄悄丢掉它展开单元格所追加的 `…`
标记。每个什么都没跑的地方都是 `false`。

**注入的 context 追加在边界处，绝不在最前面。** 那个显而易见的设计 ——
复用 `user_instructions` 槽位 —— 两处都错。那个槽位归项目文档特性所有，
在回合开始时被整份重写；而 `context_messages_with` 把它放在*最前*，正是
隐式缓存型 provider 用来做 prompt 前缀键的位置，所以往前面注入会每一个回合
都让 prompt 缓存失效。因此 hook 的 context 落在产生它的那次 tool 调用上：
缓存稳定、`/resume` 安全，而且 —— 既然是一次追加 —— 不需要动
`history_generation`。

## 用户看到什么

hook 留在屏幕上的每一行都会说出它的**事件**，所以一个挂着好几个 hook 的
session —— 或者用户的和项目的 `hooks.json` 里都有 hook 的 session —— 从不会
让用户猜是哪一个在说话。一次拒绝还会说出**hook** 和它配置所在的**文件**，
因为一次没有运行的调用正是用户会去查找原因的那种东西：

| hook…                                                | 用户看到                                                               |
| ----------------------------------------------------- | ---------------------------------------------------------------------- |
| 拒绝一次调用（`PreToolUse`）                          | 红色单元格 `Blocked by PreToolUse hook: {reason}` · `Hook: {command}` · `Config: {hooks.json}` |
| 否决它（`PermissionRequest`）                         | 同上，写成 `Blocked by PermissionRequest hook: …`                       |
| 让它不问就跑                                          | 暗色行 `Allowed by PreToolUse hook`，或 `Allowed by PermissionRequest hook: {reason}` |
| 改写它（`PreToolUse` 的 `updatedInput`）              | 暗色行 `Rewritten by PreToolUse hook` —— 单元格显示的是 hook 的参数，不是模型的 |
| block 一个结果（`PostToolUse`、`PostToolUseFailure`） | 暗色行 `Flagged by {event} hook: {reason}`                             |
| 给一次调用加上 context                                | 暗色行 `Context added by {event} hook`                                 |
| block 一个 prompt（`UserPromptSubmit`）               | 红色通知 `Prompt blocked by UserPromptSubmit hook: {reason}`           |
| block 一次停止（`Stop`、`SubagentStop`）              | `● Ran 1 Stop hook` 单元格，其下是 `Stop hook feedback: {reason}`       |
| 在回合边缘说了什么（下面那些事件）                    | 同一个单元格 —— 见下文 *hook 运行单元格*                                 |

（`·` 分隔各行。）一次拒绝在屏幕上的样子：

```
● Bash(npm --version)
  ⎿  Blocked by PreToolUse hook: Use pnpm instead of npm.
     Hook: ~/.zeroth/hooks/pnpm-only.sh
     Config: ~/.zeroth/hooks.json
```

它过去只写 `Blocked by hook: {reason}` 而没有别的，把「究竟是哪个 hook、从
哪个文件挡住了这次调用」留给用户自己去琢磨 —— 而模型却被告知了这两件事。

**单元格的内容就是模型读到的那些，减去那句指示**，由同一个
`llm::hooks::block_texts` 构建，只有一处不同：`Config:` 在用户家目录之下
把文件写成 **`~` 相对**，别处写绝对路径 —— 启动审查的标题和 `/hooks` 详情
页的 `Source:` 行正是用这个拼写指向同一个文件（`ui::display_cwd` 的规则，
有一个测试拿它来对单元格自己的辅助函数，因为 backend 不 import 任何渲染器），
所以一个 hook 被信任时所在的文件，与它拒绝的那个单元格上写的是同一个。模型
保留绝对路径，那是它能打开的。这条规则是家目录相对、绝不是 cwd 相对，因为
单元格的文本会被记录：一次来自别的目录的 `/resume` 会按原样重放它。边界把
用户的家目录交给 sink（`CommandHooks::with_home`，通过 `HookSink::home`
读回），就像它把路径显示交给应用一样；一个没有家目录的 sink —— 测试里的
假货 —— 按记录原样命名文件，家目录不是绝对路径的也一样：一个设了但为空的
`$HOME` 否则会从每条路径上剥掉一段、显示出 `~//home/u/…` —— `display_cwd`
对这样的家目录也不关联任何东西。一个不从任何文件读来的 hook（从文本解析的
配置）没有 `Config:` 行，而在一个不知道 handler 的 block 只显示那行标题。一个
在这套措辞之前录下的对话，在 `/resume` 之后保留它旧的 `Blocked by hook:`
行：记录才是屏幕上曾经显示的东西。

**那些暗色行只说出事件。** 它们是调用之下的一行，说了几件事时用 ` · ` 连接
—— 那里若再加上一条命令和一个路径，会把其余的都挤掉；一条 `Flagged by` 行
背后的 hook 在模型读反馈的地方被点名（Ctrl+D）。一条关于坏掉 hook 的警告
用它的**标签**来点名 —— 命令的第一行，在 80 个字符处截断并以 `…` 标出
（`hooks::hook_label`，也就是 `Hook:` 行对它的称呼）—— 所以
`hook ./lint.sh failed (exit 1): …` 用一个路径点名一个脚本，而一个十几行的
内联启动器则是 `hook set -e… failed (exit 1): …`，而不是把整个程序灌进一行。

一次拒绝像单元格的其他输出那样折叠（`docs/ui/long-lines.md`）：一两行的理由
让整个拒绝都留在内联，更长的一个则保留它的头几行，而 `Hook:` 和 `Config:`
行连同其余一起躲到 `… +N lines (ctrl+o to expand)` 之后 —— 理由排在最前，
因为那才是用户读这个单元格的目的。

### hook 运行单元格

回答一次**调用**的 hook 会在那次调用的单元格上留下痕迹。回合边缘的那些事件
没有调用可以挂一行，所以它们的 hook 说了什么，过去都落在用户不会看的地方：
一个 `Stop` block 只是一条只有 Ctrl+O 才看得到的无单元格注记，而一个坏掉、
说了 `systemMessage` 或答了 `continue: false` 的 `Stop` hook 被直接丢掉 ——
一个 `SessionStart` 或 `UserPromptSubmit` hook 给出的东西，除了 context
之外同样被丢掉。它们现在有自己的单元格，也就是 Claude Code 的
`Ran 1 stop hook` 穿上这个应用的打扮：

```
● Ran 1 Stop hook (ctrl+o to expand)
  ⎿  Stop hook feedback: There are uncommitted Git changes. Review the
     changes and commit the relevant files with a descriptive commit
     message before finishing.
```

而在 Ctrl+O 里，每一个跑过的 hook 及其所说的话：

```
● Ran 1 Stop hook
  ⎿  bash "$HOME/.zeroth/hooks/check-git.sh" (0.4s)
     Config: ~/.zeroth/hooks.json
  ⎿  Stop hook feedback: There are uncommitted Git changes. Review the
     changes and commit the relevant files with a descriptive commit
     message before finishing.
```

**哪些事件，落在哪里。** 单元格落在 hook 跑的地方：

| 事件                            | 单元格所在位置                                            |
| ------------------------------ | -------------------------------------------------------- |
| `Stop`                         | 回复之下、它换来的那次继续或该回合的 `Worked for` 行之上    |
| `SubagentStop`                 | 同上，落在 agent 自己的 transcript 上（它的 session 视图与它的 Ctrl+O） |
| `SubagentStart`                | 打开 agent 自己 transcript 的那个 task 之下，它的第一轮在那里读到它们 |
| `SessionStart`、`UserPromptSubmit` | 读到它们的那个 prompt 之下                            |
| `PreCompact`、`PostCompact`    | 那次压缩的 `● Context compacted` 行之上                    |
| `StopFailure`                  | 回合那条红色错误之上                                       |

tool 事件没有 —— 它们的行在调用自己的单元格上 —— 两个无处显示的事件也没有：
`Notification` 在它宣告的 prompt 占着屏幕时跑在它的车道上，而 `SessionEnd`
在它所结束的对话消失时触发。

**一个 `SubagentStart` 单元格可能跑在它的 agent 前面。** hook 在那个 agent
自己的线程上、在它的第一轮之前跑，而 lead 在这批 agent 全部 spawn 之后才在
自己的通道上宣告这次启动（`StreamEvent::AgentBatch`）—— 循环对这两条通道的
读取顺序不定，所以 hook 的注记可能为一个名单里还没列出的 agent 先到。这样的
一条注记会等那个宣告（`App::start_agent_group`），而不是随一个未知 agent 的
其余事件一起被丢掉 —— 一个与清扫赛跑的 chunk 仍然被丢掉 —— 最多 32 条，
先到先留。

**只有当 hook 说了话时。** 一个退出 `0` 且什么都没打印的 hook 是用沉默表示
同意 —— 这是常见情况，而且每个回合都有 —— 所以它不留任何单元格，内联的
没有，Ctrl+O 的也没有，与从前完全一样。什么算数，逐行列出，最重要的在前：

| 行                                      | 何时出现                                                        |
| --------------------------------------- | -------------------------------------------------------------- |
| `Stopped by Stop hook: {reason}`        | `Stop`/`SubagentStop` 给出 `continue: false` —— agent 停下，兄弟的 block 也不例外 |
| `Stop hook feedback: {reason}`          | agent 据此继续下去的那个 block —— 模型读到的内容的第一行          |
| `Context added by {event} hook`         | `additionalContext`，或对该事件而言就是 context 的裸 stdout       |
| `{systemMessage}`                       | hook 对用户说的一行，逐字                                        |
| `{reason}`                              | 一个没有什么可 block 的 block —— 来自 `SessionStart`、`SubagentStart`、`PreCompact` 或 `PostCompact` 的 exit `2`（它的 stderr）或 `decision: "block"` —— 逐字 |
| `{event} hook error: {hook} {what broke}` | 它跑不起来、超时、以非零退出（不是 `2`）、打印了非法 JSON —— 红色：`Stop hook error: ./check.sh failed (exit 1): {stderr}` |
| `hooks.json: …`                         | 一路上遇到的配置问题：一个不是正则的 matcher、一个被跳过的 handler —— 红色 |

每一行都开自己的 `⎿`，在自己的文本下换行，而且**每一行各自折叠**，像 tool
单元格的输出那样（`docs/ui/long-lines.md`）：三行加一条 `… +N lines` 行，
或四行整个显示。逐行而不是整体折叠正文，是因为整体折叠会让一个长行 —— 一个
Stop hook 的一整段反馈 —— 把它后面的每一行都折出视野，让一个红点压住一行
谁也看不见的错误；现在每一行至少显示它的开头。标题带着
`(ctrl+o to expand)` 提示 —— 永远有东西可展开，就是那些 hook 本身 —— 所以
那条折叠行不再重复它。

**是 `feedback`，不是 `error`。** Claude Code 把阻塞性 Stop hook 的理由印成
`Stop hook error:`。它不是错误：hook 干了它的活，而它说的话就是 agent 接下来
要照着做的事。所以这一行写 *feedback* —— 也是模型读它时的那个词 —— 而
*error* 留给一个坏掉的 hook，用红色，好让一个干活的守卫和一个崩溃的守卫一眼
就能分清。一行 error 会说出出了什么错**以及是哪个 hook**：解析写下的警告
（`hook ./check.sh failed (exit 1): …`）只丢掉它的第一个词
（`HookSaid::error`）。折叠的单元格列它的各行时不带 hook，所以一个有好几个
hook 的单元格否则会把坏掉的那个留给 Ctrl+O；而且既然 hook 用它自己的标签，
一个内联程序只让这一行丢掉它的第一行，从不丢掉整段文本。展开时，这一行坐在
那个 hook 自己的行之下，于是它不再重复那个名字（`RanHook::row_under`），
而不是说两遍。

**只记生效的东西。** 这些行是结果，不是印出来的每一个字：反馈行是第一个
block 的，也就是 agent 据此继续的那个（第二个 hook 的 block 被合并掉，规则在
上面）；`continue: false` 高于一次 block，所以一个停下的回合显示停止、不显示
反馈；一个 `UserPromptSubmit` block 就是取代 prompt 的那条红色通知，所以
单元格从不重复它。一个在没有什么可 block 的事件上的 block ——
`SessionStart`、`SubagentStart`、`PreCompact`、`PostCompact` —— 什么也改变
不了，所以它的理由是一条给用户的消息，从不是反馈：Claude Code 把那段 stderr
只展示给用户，而这个 runner 过去把它丢掉了；一个光秃秃的 exit `2` 没给理由，
也就什么都不说。一个不接受 stdout 作为 context 的事件（`Stop`、
`PostCompact`、`StopFailure`）打出来的裸 stdout 是闲话，保持不显示。而一次
收割运行中的 Stop hook 的 Esc 不留单元格：runner 把这个 hook 报成
`did not run: interrupted`，那读起来会像一个坏掉的 hook，而实际上是用户停下了
这个回合。

**展开时**，单元格按顺序列出跑过的每一个 hook：它的命令（第一行，在 80 个
字符处截断 —— `Hook:` 行的标签）、它跑了多久，以及它配置在哪个
`hooks.json` 里（`~` 相对，拒绝单元格的规则）；然后是那个 hook 说的各行，一条
context 行后跟它所加的文本，一条 error 行则不带上一行已经给出的名字。一个
沉默的 hook 挨着一个话多的 hook 也会列出 —— 标题统计每一个跑过的 hook。

**那个圆点**在所有 hook 都干净跑过时是绿色 —— 一个 block 也是 hook 在正常
工作 —— 任何一行是 error 时是红色。

**记录。** 单元格就是这个 hook 注记的 **report**：`HistoryItem::HookNote`
在它的文本旁边携带一个可选的 `hooks::HookReport`，由
`llm::hooks::hook_report` 依据每个 handler 的运行（命令、文件、跑了多久）与
它解析后的答案构建，每一行的措辞都由别处为对应行措辞的同一个函数给出
（`context_added_note`，模型那边的 `Stop hook feedback:` 行）。注记的 `text`
仍然恰好是模型读到的内容 —— 对一个没有给对话增加任何内容的单元格（hook 的
失败、一条 `PostCompact` 消息）为空 —— 而派生出的 context 只重放非空文本，
所以**单元格不改变模型读到的任何东西**。落到磁盘上时，一条带文本的注记保留
它的 `hook_note` 记录，report 就在它旁边 —— 一个旧的构建会重放文本、忽略
report —— 而没有文本的一条是一条 `hook_report` 记录，旧的构建会跳过它（否则
它会重放一条空消息）。在这套措辞之前记录的注记保留它旧的 Ctrl+O 渲染：记录
才是屏幕上曾经显示的东西。agent 循环自己的继续 prompt
（`docs/architecture/unfinished-replies.md`）搭乘没有 report 的注记，保持
无单元格。

## 模型读到什么

模型读到的每一段 hook 文本都是 codex 的形状：一行说出是哪个事件的 hook 说的
话、说了什么，它做了决定时在下面点出那个 hook，而只有在一次调用被拒绝时才
附上指示。每一段都由 `llm::hooks` 里的一个函数构建，离线 demo 也调它。

| hook…                                            | 模型读到                                                                          |
| ------------------------------------------------ | ------------------------------------------------------------------------------- |
| 拒绝一次调用（`PreToolUse`）                      | `Blocked by PreToolUse hook: {reason}` · `Hook: {command}` · `Config: {hooks.json}` · `Do not retry the same call; address the reason or ask the user.` —— 作为调用的结果，它另外加的 context 排在其后 |
| 否决它（`PermissionRequest`）                     | 同上，写成 `Blocked by PermissionRequest hook: …`                                   |
| block 一个结果（`PostToolUse`、`PostToolUseFailure`） | `{event} hook feedback: {reason}` · `Hook: {command}` · `Config: {hooks.json}` —— 在调用自己的输出之后 |
| 给一次调用加上 context                            | `{event} hook additional context: {text}` —— 在调用自己的输出之后                   |
| 给一个回合或一个 agent 加上 context（`SessionStart`、`UserPromptSubmit`、`SubagentStart`、`PreCompact`） | 那一行放在 `<system-reminder>` 标签里，作为一条 user 消息 |
| block 一次停止（`Stop`、`SubagentStop`）          | `Stop hook feedback: {reason}` · `Hook: {command}` · `Config: {hooks.json}` —— 下一条 user 消息 |
| block 一个 prompt（`UserPromptSubmit`）           | 那个回合什么都没有（prompt 被抹除）；红色通知 `Prompt blocked by UserPromptSubmit hook: {reason}` 在之后的回合作为 `[error]` 行重放 |

（`·` 分隔各行。）一次拒绝过去是五行，而且从不说出是哪个 hook 拒绝的 ——
`A configured lifecycle hook blocked this tool call.`、`Reason: …`、两句
指示 —— 而一个 `PostToolUse` block 是
`A lifecycle hook flagged this result: …`。

**一个决定用命令和文件点名它的 hook。** 给出理由的那个 handler 随它一起
走（`ParsedHook::blocked_by`，一个 `HookOrigin` 与 `block_reason` 一起合并
进 `HookOutcome::block`），所以有好几个 handler 时，被点名的 hook 就是模型
读到其话语的那一个。`Hook:` 是用户写下它时的命令：对一个脚本来说它*就是*
hook 的路径，agent 需要那条规则时能读它，而且它是用户认识这个 hook 的名字。
只显示它的第一行，截到 80 个字符并以 `…` 标出截断，因为一个内联程序不是
名字。`Config:` 是它配置所在的 `hooks.json` 的绝对路径 —— 用户自己的或某个
项目的，这一点在两层合并之后才有意义 —— 好让 agent 能读那个文件，或者准确
告诉用户该去哪里改规则。每个组在载入时学到自己的文件
（`HooksFile::with_source`，一个 JSON 从不携带的字段），选择时把它拷到
handler 上（`CommandHook::source`），运行时拷到它的 report 上
（`HookRun::file`）；一个只从文本解析的配置没有这个字段，所以它的裁决只
点名命令。codex 在拒绝时不点名任何 hook —— 它的 `Command: …` 后缀只是把
模型自己的调用回声一遍，而在这里调用的参数已经带着它了 —— 只有一次 Stop
继续带一个，即它 `hook_run_id` 里的 `hooks.json` 路径；Claude Code 则点名
命令。一行 context 点名它的事件而不点名它的 hook：它报告的是一个事实，而不
是一个会有人追问的决定。

**只有拒绝带指示。** 一次被拒绝的调用是模型会可靠地做错下一件事的地方 ——
再发一次一模一样的调用 —— 所以它得到一行。反馈和 context 对「拿它们怎么办」
什么都不说，因为它们说的就是该怎么做。

**context 带标签**，因为它总是与一些绝不能被误认成它的文本同处一地。codex
给 `additionalContext` 一条自己的 developer 消息；我们的追加到调用的结果之后
（上面那个缓存稳定的边界），或作为 user 消息发出 —— 在那些地方，没有标签的
文本会读成 tool 的输出，或读成用户在说话。`SubagentStart` 的 context 过去
正是那样到达的，作为 agent 第一回合顶部的裸 user 消息；现在它像 session 的
那样被包起来（`subagent_start_note`）。

**没有理由的拒绝读作 `no reason given`**（`hooks::NO_REASON`）：exit 2 而
stderr 为空、一次没有消息的 deny、`continue: false` 而没有 `stopReason`。
它跟在 `Blocked by PreToolUse hook:` 之后，所以它只说缺了什么；旧的
`blocked by a hook (no reason given)` 让单元格把同一件事说了两遍。

在这套措辞之前录下的 rollout 保留它当时被送出的内容：一次 `/resume` 重放
模型实际读到的。

## 一份可用的配置

`~/.zeroth/hooks.json`，做的是人们最常要的那两件事 —— 守一条命令，以及在
文件改动之后跑 lint：

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "bash",
        "hooks": [
          {
            "type": "command",
            "command": "jq -re '.tool_input.command | test(\"rm -rf\") | not' >/dev/null || { echo 'no recursive deletes' >&2; exit 2; }",
            "timeout": 10
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "write|edit",
        "hooks": [
          {
            "type": "command",
            "command": "path=$(jq -r '.tool_input.path'); case \"$path\" in *.rs) cargo fmt -- \"$path\" >/dev/null 2>&1;; esac",
            "timeout": 60
          }
        ]
      }
    ]
  }
}
```

第一个走 shell 原生的路子拒绝 —— exit `2`，理由在 stderr。第二个想说点什么
的时候用 JSON 作答，其他时候保持沉默；一个什么都不打印、以 `0` 退出的 hook
就是用沉默表示同意，这是常见情况，代价是一个进程。

要把一个事实而不是一次拒绝交给模型，就打印裁决对象：

```bash
printf '%s' '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"the build id is ZX-4417"}}'
```

## 配置与信任

**两层，而且在被信任之前什么都不会跑。** `~/.zeroth/hooks.json`
（`ZEROTH_HOOKS_FILE` 覆盖那个用户路径；`ZEROTH_HOOKS` 把整个特性
关掉，即 `ZEROTH_PERMISSIONS` 那个模式）—— 以及，自
`docs/state/project-config.md` 起，项目自己的 `{root}/.zeroth/hooks.json`，
在最靠近的 `.git` 根上被发现，并在用户文件**之后并集合并**
（`HooksFile::merged` —— 加载器多读一个条目，不必发明任何优先级规则：
`select` 既有的按命令去重会保留用户的那一份，任何 block 胜出，context 拼接）。
hook 在自己的事件触发时就会跑一条自己的 shell 命令，绕开权限 prompt，所以
**两个文件里的每一个 hook 都只在用户信任它之后才跑** —— codex 的答案，它在
启动时逐个询问每个新增或改动过的 hook。**启动审查**
（`docs/state/config-review.md`）在第一帧上逐字列出它们，`trust.json` 按
事件、matcher 和命令记录每一个被信任的 hook，一个被改过的 hook 会被再次询问，
旧命令就摆在新命令旁边，而 `/hooks` 之后可以信任或取消信任一个。一个在
clone 时就跑的项目内 hooks 文件，会让一个仓库在第一次 tool 调用时就拿到任意
代码执行；而一个别的什么东西（包括 agent）能写的用户文件，是隔一个目录的
同一个洞。

一个格式不对的 `hooks.json` **不会**被悄悄当成「没有 hook」—— 用户正是这样
开始相信自己有个守卫在跑，而其实没有。解析返回错误，边界把它抬成一条红色的
启动 toast，session 在 hook 关闭的状态下继续。

子进程经由和其他每一个 shell 子进程相同的 **tty 分离层级走查**启动
（`docs/tools/tty-detach.md`），所以一个试图在 `/dev/tty` 上提问的 hook 会
快速失败，而不是劫持 TUI。它跑在 session 的 cwd 里，并得到
`ZEROTH_PROJECT_DIR` —— 外加 `CLAUDE_PROJECT_DIR`，好让为 Claude Code
写的脚本原样找到它们的项目根 —— 设为**项目根**：最靠近的 `.git` 祖先，也就是
项目层读取 `.zeroth/` 的那个根，而在一个仓库之外、或那个祖先就是家目录时则是
cwd（`trust::hooks_project_dir`）。所以一个项目从任何子目录都能以同样的方式
够到自己的脚本：

```json
{ "type": "command",
  "command": "${ZEROTH_PROJECT_DIR}/.zeroth/hooks/format-and-test.sh" }
```

它过去是 cwd，所以在 `repo/src` 里启动时，那条命令点名的是一个并不存在的
脚本，hook 失败开放，而它本该跑的那个守卫从未跑过。像每一个 shell 子进程
一样，hook 运行时没有 herdr 窗格 id（`docs/architecture/herdr.md`）。

`/settings` 菜单里有一行 **Hooks**。它是一个朴素的开关，一个没有自由文本
字段的循环菜单最多也只能给这个，而在没有解析出任何配置文件时它报
`false (unavailable)`。它**默认开着**，并且**按工作目录**
（`docs/state/per-directory-state.md`）—— 启动审查就是旧日那个 opt-in 所替代
的闸门 —— 所以一个不该跑它们的目录可以退出：循环一次就在它的
`settings.json` 条目里记下 `"hooks": false`。这一行关掉时，审查对 hook 什么
都不问（一个也不会跑），而在有 hook 等待信任时把它重新打开，会打开审查的
hooks 页。`ZEROTH_HOOKS` 为一次运行播种这一行（`=1` 开，假值关 ——
`ZEROTH_TOOLS` 的模式），且从不被保存。

## 模块布局

遵循 crate 其余部分所用的纯核心 / 边界拆分：

| 位置                | 内容                                                                 | 由什么覆盖  |
| ------------------- | -------------------------------------------------------------------- | ---------- |
| `src/hooks/`（纯）  | 配置解析 · matcher · 载荷构建 · 裁决解析 · 合并 · 单元格的 report        | 单元测试，TDD  |
| `src/subprocess.rs` | `spawn_shell_with` —— 带调用方所选 stdio 的层级走查                      | 单元 + 真实 `sh` |
| `src/llm/hooks.rs`  | runner 与 `HookSink` 胶水：载荷 → spawn → 解析 → 合并                    | 真实 `sh` 单元测试 + `smoke.sh` |
| `src/tui/config.rs` | 定位并载入 `hooks.json`                                                | 既有       |

纯的那一半把 hook 捕获的 stdout **当作字符串**接过去，所以每一条裁决规则和
合并规则都能从 fixture 做单元测试，全程不涉及任何进程 —— 与
`session`/`history` 为它们的 JSONL 格式所用的同一套拆分。只有 spawn 是边界
代码。

### 接缝：一个 trait，而不是十四个闭包

`run_agent` 已经在一个 `#[allow(clippy::too_many_arguments)]` 之下接了九个
参数。每个 hook 事件加一个闭包，就是让这个特性烂成十四个手工穿线的调用点和
一个无法审阅的签名。取而代之的是一个 trait 对象，方法都默认为空操作：

```rust
pub trait HookSink: Send + Sync + Debug {
    fn pre_tool_use(&self, …) -> PreToolVerdict { PreToolVerdict::default() }
    fn post_tool_use(&self, …) -> PostToolVerdict { PostToolVerdict::default() }
    fn post_tool_use_failure(&self, …) -> PostToolVerdict { PostToolVerdict::default() }
    fn permission_request(&self, …) -> Option<HookPermissionVerdict> { None }
    fn permission_prompt(&self, _call: &ToolCallRequest, _cancel: &CancelToken) -> Option<OnShown> { None }
    fn subagent_start(&self, …) -> Vec<String> { Vec::new() }
    fn stop(&self, _active: bool, _last: &str, _cancel: &CancelToken) -> StopVerdict { … }
    fn stop_failure(&self, _error: &str, _details: &str, _cancel: &CancelToken) -> Option<HookReport> { None }
    fn session_start(&self, _cancel: &CancelToken) -> HookContexts { … }
    fn user_prompt_submit(&self, _prompt: &str, _cancel: &CancelToken) -> PromptVerdict { … }
    fn session_end(&self, _reason: &str) {}
    fn for_subagent(&self, …) -> Option<Arc<dyn HookSink>> { None }
    fn prompt_hook_label(&self) -> &'static str { "UserPromptSubmit" }
    fn home(&self) -> Option<&Path> { None }
}
```

每一个会阻塞的方法都接该回合的 `CancelToken` —— 这个 trait 对阻塞只有一种
说法，而唯一的例外（`session_end`，它在自己的 2 s 预算下越过取消运行）在
方法处说明了这一点。`stop` 自己路由自己：lead 的 sink 触发 `Stop`，带
`for_subagent` 标签的触发 `SubagentStop` —— Claude Code 以同样的方式选事件，
看是否存在 agent id。

每个方法都有默认值，所以 `NoHooks` 不花任何代价，每一个既有测试都原样编译。
**加第六号事件就是一个有默认值的方法加一个调用点** —— 不是更宽的签名，也不
是一个新的子系统。这个性质就是整个设计的意义；若后来的改动打破了它，那改动
就是错的。它在最初十一个之后加的那三个事件上成立：
`post_tool_use_failure`、`stop_failure` 与 `permission_prompt` 是三个有默认
值的方法，各有一个调用点。那两个不是事件的方法回答循环关于 sink 本身的问题：
`prompt_hook_label` 是 compact 包装的 prompt hook 属于哪个事件，`home` 则是
一次拒绝的单元格据以把 hook 的文件写成相对路径的那个目录（*用户看到什么*）。

## 刻意不做

- **`mcp_tool`、`prompt` 与 `agent` 三种 handler 类型。** codex 会解析它们；
  那边也只实现了 `command`。我们的解析之后会带一条警告跳过，而不是让整个
  文件失败。
- **异步（发后不管）handler。** v1 的每个 hook 都是同步的。`"async": true`
  标志会被解析并忽略，附一条警告。codex 用的有界并发池是第二阶段的事。
- **输出溢出到磁盘。** codex 把过大的 `additionalContext` 写到磁盘并留一个
  预览。我们改为封顶并截断（`HOOK_OUTPUT_MAX_BYTES`），直到有人需要更多。

## 离线覆盖

`scripts/smoke.sh` 在启动前会 unset 每一个 `*_API_KEY`，所以边界套件总是跑在
dummy backend 上，而 `src/tui/` 按政策没有单元测试。一个 hook runner 因此会
完全没有自动化覆盖，除非有一个 dummy 场景驱动它 —— 所以有一个。一个提到
**`hook`** 的 prompt 会播放一段同时演示契约两半的脚本化回合：

```
● Bash(rm -rf build/)
  ⎿  Blocked by PreToolUse hook: no destructive deletes outside ./tmp
     Hook: ~/.zeroth/hooks/no-deletes.sh
     Config: ~/.zeroth/hooks.json

● Bash(ls -la src)
  ⎿  total 8
     drwxr-xr-x  app
     drwxr-xr-x  ui
  ⎿  Context added by PostToolUse hook

● …

● Ran 1 Stop hook (ctrl+o to expand)
  ⎿  Stop hook feedback: Before you finish, explain what a Stop hook does.

● And as I went to finish, a Stop hook exited 2: …
```

它的拒绝文本来自 `llm::hooks::block_texts`，它的行来自
`llm::hooks::context_added_note`，而它收尾那个 Stop hook 的反馈与单元格来自
`llm::hooks::stop_feedback_texts` 和 `llm::hooks::hook_report` —— 就是
live runner 所调的那些函数，只是喂进一条替身 hook 命令和一个替身文件
（Stop hook 的运行由 `hooks::parse_run` 读取，与真实的情形一样）—— 所以离线
的单元格与一个真实 `hooks.json` 产出的逐字节相同，这是其他每一个脚本化 demo
都在用真实 executor 的格式化器所遵循的规则。**`smoke.sh` 的 Phase 72** 驱动
它，并断言被 block 的调用没有产生任何自己的输出、它的单元格点名了事件、那个
hook 和那个文件，被允许的那个把自己的输出留在点名它事件的来源行之下，那条
仅面给模型的指示从未进入 scrollback，Ctrl+O transcript 保留了这次拒绝，而
收尾那个 Stop hook 的 block 在内联是一个 `● Ran 1 Stop hook` 单元格 —— 反馈
就在它的标题之下，它换来的那一轮排在它之后，它的命令在内联中无处出现 ——
Ctrl+O 把它展开成跑过的那个 hook、它跑了多久以及它的 `hooks.json`。
**Phase 150** 用真实 runner 和一个 stub provider 驱动 hook 运行单元格：一条
`SessionStart` hook 的消息和一条 `UserPromptSubmit` hook 的 context 作为
prompt 之下的单元格，一个 block 一次的 `Stop` hook 挨着一个总是坏掉的 ——
反馈领起第一个单元格，error 行说出哪里坏了、是哪个 hook，展开不再重复它的
名字；它换来的那一轮；第二次触发时一个只有 error 的单元格 —— 模型读到反馈和
context，读不到任何只属于用户的内容，Ctrl+O 列出两个 hook 及其耗时，而一个
被拒绝回合的 `StopFailure` hook 的单元格在红色错误之上。**Phase 151** 驱动
一个 subagent 的 `SubagentStart` 单元格：一个 lead 启动一个前台 agent，旁边
有两个 `SubagentStart` hook，一个向它交代情况，一个以 2 退出 —— agent 的第一
个请求带着那份交代而不带那个理由，它的 session 视图在它的 task 之下显示
`● Ran 2 SubagentStart hooks` 单元格（先是 context 行，然后是作为消息的理由），
而 lead 的对话从不显示它。**Phase 73** 通过真实的循环分支驱动
`prompt-block` 场景（cue：`hook` + `block my prompt`）：被 block 的提交离开
scrollback（只剩 composer 恢复的那份文本副本）在那条红色一行通知之下。
**Phase 133** 驱动那些需要真 backend 的 session 事件，对着一个拒绝每一个请求
的 stub provider：一条睡 2 s 的 `SessionStart` hook 在任何 prompt 之前触发，
且不拖住第一帧，它的 context 到达第一个请求，prompt 的 `session_id` 是那个
对话的、它的 `transcript_path` 点名那个文件，被拒绝的回合触发类型为
`invalid_request` 的 `StopFailure`，一次 `/clear` 把 id 换掉（`SessionEnd`
点名旧的那个，新 `SessionStart` 期间发出的那个回合在等它），一次退出的
`SessionEnd` 最后出现；一次裸 `--resume` 在它的 picker 还开着时什么都不触发，
然后在关掉它时触发 `SessionStart(startup)`，或在选中的对话的 id 之下触发
`SessionStart(resume)` —— 而这次启动所持有的那个新对话从未开始，也就没有
`SessionEnd` —— 离线 demo 则在两头都不触发。**Phase 134** 在一条睡 4 s 的
`Notification` hook 之下抬起一个真实的权限 prompt（stub 请求一次 `bash`
调用）：立刻作答，命令立刻运行，而 hook 们仍按那个顺序记录 `PreToolUse`、
`Notification`、`PostToolUse` —— 在上一个构建里，通知器落在最后。每一个这样
的阶段都先回答启动审查的 hooks 页，信任它 fixture 里的 hook
（`docs/state/config-review.md`）。**Phase 134b** 信任一个项目自己的
`hooks.json`，其守卫是 `${ZEROTH_PROJECT_DIR}/.zeroth/hooks/guard.sh`，
并在该仓库的 `src/` 里启动：守卫跑了（在 cwd 里），stub 的 `rm -rf build/`
从未跑，模型按路径读到那个 hook 和项目的 `hooks.json`，而拒绝的单元格也把
两者都点名 —— 文件按这个阶段自己的家目录写成
`~/projrepo/.zeroth/hooks.json`。

其余部分由三层覆盖：

- **纯核心**从 fixture 做单元测试，全程不涉及任何进程；
- **runner** 在 `llm::exec` 的那些旁边有真实 `sh` 测试 —— 一个从 stdin 读
  载荷、并依据所见阻塞的 hook，一个被收割的超时，一次被取消的回合收割一个
  正在跑的 hook，一条缺失的命令失败开放，一个留着进程占着管道的 hook，一次
  在后台的 `SessionStart` 被等待、被取消、被丢弃、被替换；
- **接线**有一个基于假 sink 的 `run_agent` 测试，以及
  `tests/live_openrouter.rs` 里三个 `#[ignore]` 的 **live** 测试，把一个真实
  模型送进一份真实的 `hooks.json`：一次 `PreToolUse` block（命令从未运行）、
  一次 `PostToolUse` 的 `additionalContext`（模型读到注入的事实并复述它），
  以及一次 `updatedInput` 改写（执行的是 hook 的命令）。

## 与参考实现的有意偏离

对两份源都查证过，且是有意选择 —— 不是意外：

- **在这里和在 codex 里，exit 2 都压过 JSON 裁决；Claude Code 先解析 JSON**
  （`hooks.ts:2533` 在 `:2648` 的 exit-2 分支之前返回），所以一个既打印了
  合法裁决*又*以 2 退出的脚本，在那边读起来不一样。两件事都做的脚本在哪里
  都是有歧义的；我们跟随 codex。
- **以 `{` 开头但解析失败的 stdout 是响亮的警告**，这里和 codex 都是；
  Claude Code 悄悄咽掉它（`hooks.ts:447-450`）并把输出当作纯文本。对一个守卫
  来说，响亮胜过沉默。
- **stderr 为空的 exit 2 仍然 block**（理由是 `no reason given`）—— 这是
  Claude Code 的行为（`'No stderr output'`）；codex 在那里失败开放。
- **一个 `PostToolUse` block 保留调用的输出。** codex 用 hook 的反馈替换结果；
  这里反馈跟在结果之后，因为调用已经跑过，它打印的内容仍然成立 —— 一个
  linter 对某个文件的抱怨，并不会让它上面的测试结果变错。
- **`Notification` 保持它在事件顺序中的位置。** Claude Code 触发它的通知 hook
  时不等待，所以慢的那个可能落在它之后的事件后面；这里它跑在通知车道上，而
  下一个有 hook 的事件会等它（见上），因为 hook 维护的状态必须结束在最后发生
  的那件事上。
- **handler 顺序执行**，不并行（两份参考实现都把同一个事件的 handler 并行化）。
  确定的顺序才是合并规则里「第一个理由胜出」有意义的前提 —— Claude Code 的
  并行版本有一个丢失拒绝理由的竞态（`hooks.ts:2862-2867`），我们拒绝照抄。
  代价是：N 个病态 handler 会叠加 N 个超时。
- **`tool_response` 是一个稳定的 `{"output": …, "success": …}` 对象。** 两份
  参考实现发的是原始 tool 结果（codex 的 bash 是一个裸字符串，Claude Code 是
  任意对象），也就是有时是字符串；我们的只有一种形状，脚本总能索引。
- **`stop_hook_active` 是建议性的，引擎侧也一样** —— 查证为 Claude Code 的
  确切姿态（没有任何代码拒绝再次 block；`maxTurns` 是它唯一的兜底）。Esc 是
  我们自己的：一次中断从不触发 Stop，所以它总会打断一条继续链。
- **一个带着 Claude Code 的 `if` 前置过滤的 handler 被带警告跳过**，而不是
  跑得*比作者划定的范围更宽* —— 我们不求值权限规则表达式。`shell:
  "powershell"` 同样跳过；`shell: "bash"` 在 `sh` 下跑。
- **过大的输出在读取时就地在内存里封顶在 64 KiB**，并带一个截断标记 ——
  codex 改为溢写到一个临时文件并留一个预览（默认约 2.5k token）；这里还没有
  人需要那种溢写。

## 已知的缺口

- **`"async": true` 同步执行**，附一条警告。`statusMessage` 也一样，它会被
  解析而不显示。
- **两个事件不留单元格。** `Notification` 在它宣告的 prompt 占着屏幕时跑在
  它的车道上，没有回到对话的通道，而 `SessionEnd` 在它所结束的对话消失时
  触发；两者按契约都忽略其 hook 的输出，所以只有坏掉的 hook 会看不见。
- **一个事件不读的裸 stdout 不显示。** 一个打印并退出 `0` 的 `Stop` hook 是
  闲话（Claude Code 的 `suppressOutput` 就是为把这类输出从它的 transcript 里
  藏起来而存在的）；它永远到不了单元格。
- **Claude Code 其余的事件没有建模**（`PermissionDenied`、`TeammateIdle`、
  `FileChanged`，……），除 `permission_prompt` 之外的 `Notification` 类型也
  一样；点名它们的配置能正常载入，且永不触发。
- **一个弃权的 PermissionRequest hook 会丢掉它的警告** —— 失败开放的姿态从
  那个位置没有通向用户的通道；一个做决定的裁决把警告带在它的注记上。
- **hooks 文件只在 bootstrap 时读一次。** 一个在会话中途出现（或改变）的
  `.zeroth/hooks.json` 差一次重启，启动审查那时才会问到它
  （`docs/state/config-review.md`）。
- **信任钉住的是命令，不是它读的东西。** `fmt.sh` 改了之后 `./fmt.sh` 仍然
  受信任 —— 两份参考实现划的是同一条线。
