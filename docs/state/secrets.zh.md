# `/secrets` —— agent 会用、但永远看不见的凭据

**`/secrets`** 命令打开一个页面，用户在那里存**密钥**：一个名字、一个值，以及
可选的一行上下文。agent 被告知名与上下文，而使用某个密钥的方式，是在 tool call
里写出它的**占位符** —— `<secret:ROOT_PASSWORD>`。占位符在 tool 运行的那一刻
变成真正的值，而在这个 tool 报告的一切里，那个值又变回占位符。于是模型可以用一
个它从未见过的凭据登录、调 API、写配置文件，而这段 session 的屏幕录制里，本该
是值的地方显示的都是占位符。

从用户这一侧也一样：用户**敲进去**的值 —— 敲进一条消息、一个 `!` 命令、一次回
答 —— 都以它的占位符进入对话。粘贴进聊天的一个密码，对模型来说是
`<secret:ROOT_PASSWORD>`，在气泡里和在记录里都是如此（*用户敲了什么*）。

## 一个值可能存在于哪里

一个值只住在三个地方：

1. **`{config_home}/secrets.json`**，以仅所有者可读写（`0600`）的方式，经由一
   个临时文件加一次 rename 写入 —— 就是 `.env` key 存储的那个写
   （`llm::keystore::EnvFile::update_if`），绝不是裸的 `fs::write`。
2. **内存里那个共享的 [`SecretRegistry`](../../src/secrets.rs)** ——
   `SkillRegistry` 的形状：一个 `Arc<Mutex<SecretStore>>`，按句柄克隆进
   backend、subagent、后台注册表和 `!` runner。
3. **那一次需要它的 tool call 的参数**里，只在那次 call 运行期间。

在存储之外，值只在 tool 代表用户把它放下的地方抵达磁盘：agent 把它写进去的一个
文件（`.env` 的意义所在），以及这个 session 的原始任务日志
（`{tmp}/zeroth-{uid}/{session}/tasks/*.output`）—— 那些日志按命令打印时的样子
保留输出；通过 tool `read` 回来，日志和其它任何输出一样是脱敏的。这些日志以仅
所有者可读（`0600`）创建，因为临时目录的默认模式会让机器上每个用户都能读它们。

它永远不会在一条消息里 —— 包括用户自己的消息 —— 不在一个 tool cell 里，不在
Ctrl+O transcript 里，不在 Ctrl+D 上下文里，不在 rollout 里，不在请求体里，不
在 classifier 的上下文里，不在 hook 的 `tool_input` 里，也不在任何 `Debug` 打
印里：`SecretValue` 的 `Debug` 只写 `<redacted>`，它没有 `Display`，而
`expose()` 只被展开和文件写入者调用。值能到达的唯一一块屏幕是 composer —— 在用
户敲它的时候：击键显示到 Enter，Enter 把它们换掉。

## 占位符

`<secret:NAME>`，`NAME` 是 `[A-Z_][A-Z0-9_]*` —— 环境变量的形状，最长 64 个字
符。展开也接受小写的名字；脱敏总是按存储时的样子写。插入进去的值永远不会被重新
扫描。

这个功能建的时候叫 `/secrete`、占位符是 `<secrete:NAME>`，在发布前改的名：
「secrete」是个真词，而这个拼写和模型的先验打架 —— 代码当初不得不把
`<secret:NAME>` 当作别名接受，以免模型把它「纠正」过去。既然模型伸手去用的那个
拼写成了正字，那就只有一个开头词，没有别名。

**一个什么都没指到的占位符会拒绝这次 call。** 照原样跑下去，一个拼错的或猜出来
的名字会以字面文本抵达 tool：在提示符上敲错的密码（一次记在账号头上的尝试）、
一个带着 `Bearer <secret:TOKN>` 发出去的请求、一行看起来写好了的 `.env`。所以
会行动的 tool、以及 `!` 命令，在它的任何部分运行之前就被拒绝，消息里点出它写了
什么、存了什么（`SecretStore::unknown_refusal`）：

```
Not run: <secret:DEPLOY_TOKN> is not a stored secret. Stored:
<secret:DEPLOY_TOKEN>. The user adds secrets with /secrets.
```

一步就能纠正一次手滑，而一个用户从未存过的密钥会把模型送去那个页面，而不是在聊
天里向他要值。什么密钥都没存时，什么都不拒绝 —— 从未用过这个功能的 session 可以
把占位符语法当文本写。代价是：一旦存了密钥，一个 tool 就不能写出一个不指任何密
钥的占位符的字面文本；而把一个已知的当文本写从来就不可能，因为它会展开。

## 展开：只在 tool 行动的地方

`secrets::expands_placeholders` 逐 tool 决定。会**行动**的 tool —— `bash`、
`bashsend`（以及遗留的 `bash_session`）、`read`、`write`、`edit`，以及每一个
`mcp__server__tool` —— 参数会被展开
（`secrets::expand_arguments`：走过每一个 JSON **字符串**，键不动，文档重新序
列化，所以一个含 `"` 或 `\` 的值会被正确转义）。那些参数要展示给人看、或者要
交给另一个模型的 tool 永远不展开：`agent`（subagent 拿到占位符和它自己的清
单）、`askuserquestion`（那个 modal 会把值显示出来）、`skill`，以及各个 task
tool。

**敲进去的输入是唯一的例外。** `bashsend` 的输入会先按键记法读一遍 ——
`<Enter>`、一个手滑的 `\\n` 被还原成换行、`&lt;Esc&gt;` —— 所以它的占位符在那
遍读取**之后**才展开，展开在它留下的那些文本段里（`llm::exec::run_session`）：
先展开的话，一个含 `\n` 或 `<Up>` 的密码就会被当作按键敲进去。值永远按文本敲
—— 这一点由一个测试钉住：它通过一个真终端敲入 `C:\new<Up>&lt;x`，再反过来读回
来。

这件事发生在 `llm::secret_exec::run_with_secrets` 里，`llm::backend` 中两个
execute 闭包都会调它 —— 主回合的和 subagent 的 —— 并且在 classifier 的动作日
志记下这次 call **之后**，所以 auto 模式的审查者读到的是占位符；也在
`ToolStart`/`RoundCalls` 把模型自己的参数带进 transcript 和 rollout 之后。hook
看到的也是占位符：`PreToolUse` 在闭包之前运行。

未知占位符的拒绝在四个地方检查，每一处都在任何东西运行之前：
`approval::approve_call`，这样一个跑不起来的 call 永远不会弹出提示（在那里判定
为拒绝还意味着：在答复与运行之间存下的密钥，无法让一次没人问过的 call 溜过去）；
`run_with_secrets`，那个权威检查，给那些跳过提示的路径（没有闸门、一个预先
批准的 hook）；`run_session`，在 `bashsend` 输入的按键记法留下的文本上 —— 所以
一个被转义成 `&lt;secret:X&gt;` 的占位符也会被抓到；以及 `tui::shell`，针对一
个 `!` 命令。

## 脱敏：输出离开 tool 的每一条路

`SecretStore::redact` 用一趟贪婪的从左到右扫描，把每一处精确出现的值换成它的占
位符 —— 两处都可能匹配时**更长**的那个值胜出，所以一个包含另一个值的值被整块
藏起来，而已经发出的占位符永远不会被再次扫描。长度至少为
`WRAP_MATCH_MIN_CHARS`（12）个字符的值还能跨一个换行匹配，就像终端 120 列的屏
幕把一长串 token 折行那样；更短的值只做精确匹配，因为 `12\n34` 是普通输出。值
至少要有 `MIN_SECRET_VALUE_CHARS`（4）个字符：每一处出现都会被替换，而一个单字
母的「密钥」会把每一份含有这个字母的输出撕碎。

这些路径：

| 路径 | 在哪里脱敏 |
| --- | --- |
| 一个 tool 的结果 —— `ToolEnd`、`ToolAnswered`、`ToolRejected`、模型的 `tool` 消息、`PostToolUse` 载荷 | `run_with_secrets`，作用于 `output` 与 `context` |
| 一条运行中命令的 live cell（`ToolProgress::Screen`） | `run_with_secrets`：`settled` 经一个 `StreamRedactor`，`live` 连同被扣住的尾巴一起 |
| 一个精炼过的 header（`ToolProgress::Title`）—— 由展开后的输入构造 | `run_with_secrets` |
| 一个后台任务的输出、屏幕、命令、描述 | `BackgroundRegistry`（`with_secrets`）：`send_output`、`send_screen`、存下的 `Launch` —— 描述也算，因为执行器把描述和这次 call 的其余部分一起展开了，而 ↓ 管理器、完成通知和模型看到的完成注记都会点它的名 |
| 权限提示的预览 | `approval::approve_call`（见下） |
| 一个 `!` 命令的输出 | `tui::shell`，在它的 `ToolEnd` 之前（命令本身用 `expand_checked` 展开，所以一个未知占位符什么都不跑） |
| `/diff` 复审 | `tui::diff`，作用于加载进来的快照 |

**流式。** 一个值可能被拆到一条运行中命令输出的两片里抵达。`StreamRedactor`
把还可能长成一个值的尾巴扣住，并把之前的一切按整段文本该有的脱敏样子发出 —— 由
一个在每个字节处切分流进行验证的测试钉住。被扣住的尾巴随 live 行一起显示，如果
它是一段足够长的值就掩码。

**截断。** 三个 tool 在末尾切掉输出 —— `read`、MCP，以及 `!` shell 的上限 ——
并把它们标记为 `truncated`。一刀切在一个值中间会留下它的前半段，所以被截断的输
出用 `redact_cut_tail` 脱敏：一个以某个值开头、长度四个字符以上的尾部片段，被掩
码成那个值的占位符。`HeadTail`，bash 工具们从中间切的做法，保留整行，什么都不
需要。

**它看不见什么。** 脱敏是文本层面的：一个被 tool 变换过的值 —— base64 编码、反
转、被换行以外的东西切开 —— 出来时就是变换后的样子。这就是这个功能立下的契
约：**精确**的文本才被藏起来。

## 用户敲了什么

一个值也可能从另一边来：用户把密码粘贴进聊天。用户发出的一切在任何东西保留它
之前就被脱敏，所以用户写下那个值的地方，模型读到的是
`<secret:ROOT_PASSWORD>` —— 由对话构建出来的一切也是如此：气泡、Ctrl+O
transcript、Ctrl+D 上下文、rollout、请求体、`UserPromptSubmit` 载荷、↑ 回溯和
`history.jsonl`。

草稿在它离开 composer 的地方脱敏，即 `App::take_input` —— 正好在一次大粘贴被拼
回之后，所以一个 `[Pasted Content N chars]` 里的值也会被抓到。每一条出路都经过
它：Enter（一个新回合、一条被引到运行中回合里的消息、一次 subagent 聊天）、
Tab（一个后续回合，主 session 的或某个 agent 的）、一个 `!` 命令，以及 Ctrl+C
的清空 —— 它的 ↑ 回溯副本和已发送的一样被脱敏。另外三段文本不经 composer 就抵
达模型，它们在采集的地方脱敏：敲进 `AskUserQuestion` 输入栏的一个回答或注记
（`take_entry_text`）、Tab 给一个被拒的权限提示添加的反馈，以及命令行上给出的
`[PROMPT]`。

纯核心从不持有存下的值，所以它通过一个 **`SecretRedactor`** 来脱敏 —— 注册表的
脱敏那一半，在启动时交给 `App` 一次（`App::set_secret_redactor`）：它能把值从
文本里取出，别的什么都不做，不展开、不列表、也没有可读的值。它共享注册表的存
储，所以 session 中途存下的密钥会掩码紧接着的下一条消息。

`SecretStore::redact_message` 是 `redact` 加一处不同：用户为某个已存密钥**写下**
的占位符按写下的样子保留，不管敲的是什么大小写 —— 页面上的 `c` 正是为此复制一
个 —— 即使某个值本身就是它的一部分（`secret` 是一个常见的开发密码，而
`<secret:DB_PASSWORD>` 含有它）。一个什么都没指到的占位符是普通文本，所以一个
被当名字敲的值 —— `<secret:hunter22>` —— 仍然被藏起来。一趟脱敏发出的每个占位
符都指着一个已存的密钥，所以下一趟会保留它：一条被回溯、回退或撤销的消息，脱敏
到它自己。

一个 `!` 命令是往返的：`!mysql -p hunter22` 被记录并显示为
`! mysql -p <secret:ROOT_PASSWORD>`，而 runner 在 shell 看到它之前把它展开回去
（`tui::shell`），所以命令照敲的样子运行。

一次掩码了东西的发送会用暗色 toast 说出来 ——
`Hid the value of
<secret:ROOT_PASSWORD> from the agent`（`secrets::masked_notice`）—— 因为气泡
不再显示敲了什么。`RedactedMessage::masked` 点名一趟里被替换掉的密钥（用户自己
敲的占位符不是被替换的值，所以从不会被点名），`App` 记下每次发送的这些名字，而
边界在发送的那次按键之后、在发送自己的动作运行之前把它们取走
（`App::take_masked_secrets`），这样一个动作用以答复的拒绝 —— 一个 subagent 拒
绝聊天 —— 就是留在屏幕上的那条 toast。只有发送会宣告：Ctrl+C 的清空一声不响地
掩码它的回溯副本，而用 Esc 留下的一个提问回答已经显示了占位符，只会在某次提交
带着它时才宣告，所以一个被拒的或超时的问题什么都不说。

规矩就是输出脱敏的那一套：精确文本、最长的值优先、长值可跨一个换行被抓到。一个
敲在更长单词里的值也在那儿被替换 —— 这是短值的代价，四个字符的下限只是限制它
—— 而一个变换过再敲的值（加了空格、反转、编码）照敲的样子抵达模型。一个存下的
值不可能故意让模型看到：模型可以读的值，是用户不存的值。

脱敏在文本发出的那一刻发生，用的是那一刻存着的密钥。一条在它的值被存下之前发出
的消息 —— 在 session 更早的时候，或者在 `/resume` 加载的一个 rollout 里 —— 在
记录里、在它携带的上下文里都保留那个值：对话永远不会事后重写，因为那会让 Ctrl+D
显示的东西与发出去的不同，也会移动它之后每个请求的缓存前缀。

## 权限提示

提示里的预览由**展开后**的 call 构造，然后脱敏：

- 一个 `old_string` 里带占位符的 `edit`，只有展开后才匹配得上文件。按模型的文
  本构造，预览就套用失败 —— `permission_request` 返回 `None`，而
  `approve_call` 把它读成*没什么要问的* —— 于是那次编辑会在**无人过问**的情况
  下跑掉。
- 对一个已经装着某个值的文件做 `write`，或者一个上下文行里带值的 `edit`，否则
  会把那个值放进 diff 显示在屏幕上。
请求的 `target`、`body` 和 `detail` 在任何东西读取它们之前就被脱敏，所以允许
列表匹配、scratchpad 规则和 auto 模式的 classifier 看到的都是占位符 —— 和用户
批准的是同一段文本。一个点名了未存密钥的 call 在这一切之前就被拒绝，什么都不
问。

classifier 被告知占位符是什么（`prompts/classifier.md`）：用户为 agent 存下的
一个凭据，在动作运行时填入，它的名字说明了它是做什么用的 —— 所以对它自己服务
的一个登录或一个 authorization 头读作日常工作，而把它发到别处读作外泄。少了这
句话，评分表里「读取或发送密钥」那条规则会把每一次使用这个功能都读成凭据窃取。

## 提示段

密钥作为**第三段列表**骑在 session 唯一的那个 `<system-reminder>` 上，排在
skill 和 agent 类型之后（`Session::sync_listings` → `App::listings`；
`docs/architecture/context.md`）：

```
Each <secret:NAME> below is one of the user's credentials. Use it verbatim
in any tool argument and the tool gets the real value (quote it in shell).
Output shows the placeholder only where the exact value appears, so never
encode or hash one. Never ask for or reveal a value.

- <secret:ROOT_PASSWORD>: Root password for the staging box
- <secret:VENICE_API_KEY>
```

四句话，每一句都是 live 套件依赖的一个行为：占位符是什么；它照写的样子进入 tool
call（一条命令、一个文件的内容、敲进去的输入 —— 「任何 tool 参数」）；为 shell
加上引号，因为值是原样插入的；以及值永远不会被索要。第三句是模型必须知道的那一
个限制：掩码是精确文本，所以一个 `curl -v` 打印出来的 Basic-auth 头、或者值的
一份 base64，会把密钥同时给模型和屏幕看 —— 被告知这一点之后，模型会把占位符交
给需要它的东西，而不是自己动手变换它。这段 header 骑在每个请求上，所以它被钉在
55 个词，其中每一点都各有一个测试
（`the_header_says_what_the_placeholders_are_for_and_stays_short`）。

排最后，因为它在三者中最少变化，而提示段是一个 prompt 缓存前缀。受
**Tools**（`/settings`）控制：没有 tool，占位符无处可去。一个 subagent 的简报
在它的类型能用上会展开占位符的 tool 时，会在自己的 skill 旁边带上同一段
（`SubagentConfig::briefing_for`）。

## 页面

```
──────────────────────────────────────────────────────────────────
  Secrets
  Credentials the agent uses by placeholder. The values stay out of
  the conversation, the screen and the model's context.

  ❯ <secret:ROOT_PASSWORD>    ••••••••  Root password for staging
    <secret:VENICE_API_KEY>   ••••••••
    + Add a secret

  Enter edit · n new · c copy placeholder · d delete · Esc close
──────────────────────────────────────────────────────────────────
```

列表显示每个密钥的占位符、一个**固定的**八点掩码 —— 从不是一字符一点，那会把长
度放到屏幕上 —— 以及它的上下文。最后一行是添加。`d` 在删除前会问（提示行变红：
`Delete <secret:NAME>? d again to confirm`），`c` 把占位符复制到剪贴板，好粘进
一条消息。

表单：

```
  New secret

  Name     ❯ ROOT_PASSWORD
             Use it as <secret:ROOT_PASSWORD>
  Value      ••••••••••
  Context    Root password for the staging box

  Enter next · Tab/↑↓ move · Esc back
```

- **Name** 边敲边归一 —— `root password` 变成 `ROOT_PASSWORD` —— 并在自己下方
  预览占位符。
- **Value** 每敲一个字符显示一个点，最多到输入框的宽度，所以一次击键看得见，而
  值读不出来；一次粘贴整段进去，保存时裁掉两侧的空白。编辑一个已存的密钥时它从
  **空**开始 —— `leave empty to keep the current value` —— 所以值根本不会被加载
  回界面里。
- **Context** 是自由文本，在占位符旁边告诉 agent。
- Enter 移到下一个字段，从最后一个字段保存；Tab、Shift+Tab、↑ 和 ↓ 移动；Esc
  回到列表；Ctrl+C 关闭页面。一次拒绝 —— 名字被占用、值不到四个字符 —— 以红色
  显示在字段下方，并把焦点移到出问题的那个字段。

这个页面属于替换 composer 的那一族（`docs/ui/view-flow.md`）：`App` 上的一个
`Option<SecretsPage>`，由 `ui::secrets_view_lines` 构造为若干行，值在**构造器
里**就掩码 —— 一个流动的行是作为文本提交进真实 scrollback 的，所以在绘制时施加
的掩码就太晚了。粘贴在 composer 的兜底之前被路由给聚焦的字段，否则一个粘贴的值
会落进隐藏的草稿里。它在回合中途也能用：一次保存在当下就重绑下一次 tool call
（注册表是共享的），而提示段在下一个回合生效。

列表列出每一个密钥，所以一个长的列表比一个短终端更高，而把它底部锚定会让标题和
最前面的几个密钥 —— 包括高亮的那个 —— 流过顶部。于是列表取它可能拥有的行数，
并构造出第一个装得下它们的分页（`docs/ui/view-flow.md` *A page
that sheds rows before it flows*）：整张列表；然后是标题压在列表的一个三行窗口
之上、窗口随选择移动，夹在 `↑ N more
above` 与 `↓ N more
below` 两个位置之间，每个位置替代整页放在那里的一行空白；然后是那个窗口，不带
标题和简介 —— 也不带那两个位置，因为 `↑` 那个会直接坐在顶部的横线之下，空空地
贴着它，而上面什么都没被藏起来（`docs/ui/view-flow.md` *The frame hugs its
content*），窗口加上提示就是这一页的两块。

窗口滚动时掩码始终在一列里，因为这一列是按每一个密钥定宽的。一个空存储只有它的
添加行，所以它的短页面直接丢掉标题，让那句空提示说明密钥是干什么用的。提示在每
一页都留着，而表单一样都不需要：它聚焦的字段本来就靠近底部，锚定会让它待在那儿。
`smoke.sh` 的 Phase 139 在 12 行上打开列表。

## 持久化

`secrets.json` 是按**用户**分的，不是按目录分的 —— 一个凭据属于那个人，这就是
`.env` 存储的姿态：

```json
{
  "secrets": [
    { "name": "ROOT_PASSWORD", "value": "…", "context": "Root password for staging" }
  ]
}
```

每一次保存和删除都是在进程级锁之下对文件做读—改—写 —— 所以两个 session 永远不
会丢掉彼此的密钥 —— 而一个再也解析不了的文件会被**拒绝，绝不覆盖**：保存失败，
并弹一条点它名字的红色 toast。页面打开时会重新加载文件，所以在另一个 session 里
添加的密钥在那儿就有。名字不合规、值不到四个字符、或者名字重复的记录，在加载时
被跳过。

## API

- `secrets`（纯）：`SecretStore`（`apply`、`remove`、`expand`、
  `expand_checked`、`unknown_placeholders`、`unknown_refusal`、`redact`、
  `redact_cut_tail`、`redact_message`、`listing`）、`RedactedMessage`、
  `SecretRegistry`（`redactor`）、`SecretRedactor`、`SecretDraft`、
  `SecretMeta`、`SecretValue`、`StreamRedactor`、`expand_arguments`、
  `check_arguments`、`expands_placeholders`、`secret_section`、
  `masked_notice`、`parse_secrets_file`、`format_secrets_file`、
  `normalize_name`、`validate_draft`。
- `llm::secret_exec` —— `run_with_secrets`，那个执行器接缝，以及给权限预览用的
  `expand_call`（两者都拒绝未知占位符）。
- `app::secrets` —— 页面的状态与按键，以及用户敲进来的东西
  （`App::set_secret_redactor`、`App::redact_typed`、
  `App::take_masked_secrets`）；`ui::secrets_view` —— 页面的行。
- `tui::secrets` —— 加载、保存、删除、复制、掩码值 toast；脱敏器在
  `tui::bootstrap` 的 `seed_app` 里交给 `App`。

## 测试

纯模块钉住语法、两个方向、拒绝、流式与截断规则、文件格式，以及没有任何东西会把
值 `Debug` 打印出来。`llm::secret_exec` 让一个假 tool 走接缝：参数被展开、每个
输出通道都被脱敏、未知占位符未被运行就被拒绝、不行动的 tool 不被触碰。审批测试
钉住带占位符的 `edit` 仍然会带着脱敏后的预览发问，以及一个被拒的 call 不弹任何
提示。页面的 app 与 ui 测试断言值永远不抵达任何渲染出来的行或 composer。
`redact_message` 钉在 `redact` 旁边：用户自己的占位符被保留，即使某个值就是其中
一部分；一个被当占位符名字敲的值仍被藏起来；第二趟不做任何改动；而每一趟都点名
它掩码了哪些密钥。app 测试把值沿草稿离开 composer 的每一条路都敲一遍 —— Enter
的空闲与回合中途、Tab、一个 `!` 命令、一个 subagent 的聊天与后续、一次大粘贴、
Ctrl+C 的回溯 —— 也敲进 ask modal 的各项和权限提示的反馈，并在每一个交出去的东
西里只找到占位符；每次发送都为自己的 toast 而宣告，而那些什么都不发的路径 ——
清空、用 Esc 留下的回答、一个被拒的或超时的问题 —— 什么都不宣告。

`tests/secrets_wire.rs` 用真执行器对着回环上的一个 provider 替身跑真的
`LlmBackend` 回合，并把每个请求体读回来：一条对值采取行动的命令、一次经由占位符
的 `write` → `read` → `edit` 往返、一个敲进真实终端 session 的密码、一个由自己
的占位符点名的后台 shell、一个在有无权限闸门两种情况下都被拒绝的未知占位符，以及
一个敲进 `App` 的 composer、以占位符抵达请求的值 —— 值不在任何事件里、不在任何
shell 报告里、也不在任何请求里，而第一个请求的提示段列出了那个密钥。
`tests/live_secrets.rs`（标了 `#[ignore]`，`A0_VENICE_API_KEY`）让真模型去用一个
它只从提示段得知的密钥：把它写进一个文件、给一个装着它的文件加引号、通过一个密
码提示登录、熬过一次拼错的名字，并用测试自己跑在其上的那把 key 发出一个真正的认
证请求。

`scripts/smoke/phases/126-secrets.sh` 在 tmux 里驱动页面 —— 添加、掩码、编辑、
删除、持久化 —— 以及会展开占位符、输出被脱敏、并拒绝未知占位符的 `!` 命令；一
个带着值本身敲的 `!` 命令带着它运行、却以占位符显示；一条带着值敲的消息以占位符
抵达气泡、rollout 和 ↑ 回溯；一次带着值给出的 `[PROMPT]` 启动以占位符开场 ——
每一项都在一条说值已被藏起来的 toast 之下；然后第二次启动拿真 backend 对着一个
stub provider 跑：启动时从文件加载密钥、第一个请求里的提示段、一条带着值敲出的
第一条消息以占位符抵达 provider 和真的 `history.jsonl`、权限提示显示占位符、命令
写下值、cell 掩码它、未知占位符不弹提示就被拒，以及值在窗格、Ctrl+O transcript、
rollout 和 stub 记下的每个请求里都不出现。
