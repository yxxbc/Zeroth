# 变更日志

Zeroth 的所有值得注意的变更都记录在这里。格式遵循
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/)，项目使用
[语义化版本](https://semver.org/spec/v2.0.0.html)。

`0.0.0` 以下的条目继承自
[Alter Zero](https://github.com/linuztx/alter-zero)；`0.0.0` 是这个 fork 的
开发基线。

一次发布在 GitHub 上的说明由它下面那一节生成（`scripts/release.sh notes`，
见 `docs/ops/release.md`），所以写在这里的条目就是用户读到的条目。变更在
合并时落到 **Unreleased** 之下；切版本时 `scripts/release.sh prepare X.Y.Z`
把那一节卷成一个带日期的发布标题。

## [Unreleased]

### 新增

- **账户和 API key 现在都可以退出。** 登录菜单增加连接管理入口，
  同时提供直达同一页面的命令。确认后删除本地凭据和认证缓存，并阻止环境变量
  或后台刷新恢复已退出的连接，重新登录后恢复。OAuth 刷新增加跨进程事务锁，
  保存失败会提示，推理请求的首次未授权拒绝会受控刷新一次。
  详见[凭据生命周期](docs/providers/credential-lifecycle.md)。

- **在一份长草稿上打字不再每帧复制它的换行结果。** composer 的换行缓存现在
  交出的是一份共享的 `Arc<[Range<usize>]>`；`display_rows`、`row_count`、
  `cursor_row_col`、竖向移动和渲染器的 `InputBox` 都读它，而不是每帧把整个
  `Vec` 克隆四到六次。`live_rows` 也把 live 区域的八个行计数一次推导出来，
  而不是推两遍（在一个打开 palette 的队列上实测为每帧两次 13 µs）。增量重换行
  与对召回草稿的上限仍未完成（`docs/architecture/input-latency.md`、
  `docs/design.md`）。

- **冒烟套件在 macOS 上恢复可移植。** `smoke_now_ms` 在六个阶段里取代了只有
  GNU 才有的 `date +%s%N` / `+%s%3N` / `+%s.%N`，`smoke_braille_cells` 取代了
  BSD/toybox 的 `grep` 会拒绝的一段字节范围的 `grep`；两者都由
  `scripts/smoke/selftest.sh` 钉住。阶段 6、32、48、110、133 和 134 现在在
  Mac 上通过（`docs/ops/smoke.md`）。

- **回复流式传输时打字仍然跟手。** 一个编辑 composer 的按键不再排在 120 fps 的
  帧下限后面：帧调度器现在给它自己的 2 ms 紧急下限，并把一个已经排队的动画帧
  提前，于是一条正在流的回复、状态行的 shimmer 或一条打开的条带，都不再能把一次
  击键压住整整一个 8.33 ms 的拍子。粘贴突发里的字符仍先被识别，走原来的宽松
  路径，而导航键、流进来的文本和 resize 保持普通 120 fps 的合并。composer 自己
  那份 O(draft) 的换行开销仍然存在（`docs/architecture/input-latency.md`、
  `docs/design.md`）。

- **命令页上的选项卡：Commands 与 Skills。** 光敲一个 `/` 现在会打开一个页面，
  它的活动选项卡是一个朴素、带下划线的标签（`Commands  Skills`）；只有当这个
  session 确实有 skill 时那条栏才出现，所以全新安装的仍是旧的平铺 palette。
  Tab/Shift+Tab 循环切换选项卡，Enter 运行活动行，每个选项卡各自记住自己的
  高亮 —— 一个选项卡里的过滤或选中永远不会漏到另一个里。skill 行把 `$name `
  插进 composer（由模型加载它，这是 `$` picker 的约定）。窄栏会收成活动标签。
  Enter 现在是唯一的接受键；Tab 负责循环（`docs/ui/command-tabs.md`）。

- **`frontend-design`，一个 Apache-2.0 的内置 skill。** Anthropic 官方的设计
  指导 skill 被内嵌并像其他内置 skill 一样播种，它的 `LICENSE.txt` 与
  `SKILL.md` 放在一起；`prompts/skills/README.md` 现在记录每个内置 skill 的
  来源与许可规则（没有再分发授权就一个字都不抄 —— 个人的 `~/.claude/skills`
  与没有许可的 `~/.alter-zero/skills` 条目是刻意不随包发布的）。

- **回复里可点击的文件路径。** 助手的散文与行内代码现在把形似路径的 token
  （`src/app.rs:42`、`./scripts/run.sh`、`~/notes/today.md`）按 session 的
  cwd/home 规则链到它们绝对、百分号编码的 `file://` URI。`:line[:col]` 后缀
  仍是普通散文；glob、flag、`key=value` token 和 URL 一律拒绝，检测过程从不
  触碰文件系统。两种 viewport 里链接都能用，流式传输时只要一个路径还有可能
  变长，那一行就先不发（`docs/ui/links.md`）。

- **实验性的屏幕独占模式：`--fullscreen`。** 终端 UI 可以在备用屏幕上运行，把
  对话、live 区域、composer 和页脚画进一个自有帧里，而不是把已完成的行提交进
  终端的 scrollback（`ZEROTH_TUI=full` 效果相同）。流式传输、tool 单元格、
  resize、Ctrl+O 和全屏页面都能用；PageUp/PageDown、Ctrl+Home/End 和滚轮滚动
  对话，拥有窗口顶部的那条用户消息钉在它滚动的回复之上；鼠标捕获带来链接
  悬停/点击（经由系统打开器）、拖选/复制、tool call/思考/组的**点击展开**
  （它的 Ctrl+O 形态，就地钉住）、**点击放置 composer 的光标**，以及一行里
  孤零零一个 URL 的**预览卡**（站点/标题/描述，页面有 `og:image` 时再加一张
  封面缩略图；只取一次并缓存，封面预算 32 MiB，`ZEROTH_LINK_CARDS=0`
  关闭）。编辑器交接是下一个阶段。内联 viewport 仍是默认
  （`docs/ui/full-screen.md`）。

- **并行 tool 批次现在在 TUI 里同时跑 —— 也同时流式显示 —— 好几个 call。**
  每个按 call 分派的 stream 事件（`ToolStart`、`ToolEnd`、`ToolOutput`、
  `ToolScreen`、`ToolTitle`、`ToolNote`、`ToolAnswered`、`ToolRejected`、
  `ToolBackgrounded`）都带着该 call 的 id，app 把每一个路由到它自己的单元格：
  一个批次的 worker 一启动就把自己的单元格翻成 `Running`，各自跟随自己的输出，
  一个 chunk 永远不会落到兄弟 call 的单元格上。记录仍按模型的顺序解析。权限
  prompt 现在用它请求里点名的那个 call 打头（即使更早的兄弟还排着队），
  Ctrl+B 按 call id 把第一个运行中的命令放到后台，所以兄弟的 runner 永远抢
  不到那个闩。`!` shell / 脚本化后端保持那条不带 id 的旧路径不变
  （`docs/tools/parallel-tools.md`）。

- **session 文件现在记录每个 tool call 是怎么跑的，以及每次模型请求花了多少
  钱。** rollout（`~/.zeroth/sessions/…/rollout-*.jsonl`）里的每一条
  `tool` 行都带着命令的 `exit_code`、这个 call 跑了多久（`duration_ms`），
  以及失败时的原因（`error_kind`：`tool_error`、`timed_out`、`interrupted`、
  `rejected`、`classifier_denied`、`hook_blocked`、`approval_required`、
  `tool_call_limit` 或 `backend_error`）。脚本不用再在输出文本里找
  `Exit code: N` 了。每次模型请求也会留下一条 `usage` 行，带 provider 自己的
  计数（`input`、`output`、`cached`、`cache_write` 和 `reasoning`，子代理再加
  `agent`，写出压缩摘要的那次请求再加 `compact`），写在请求结束的地方，所以
  每次请求和每个 tool 步骤的花费都能从文件里读出来。既有字段不变，旧版本会
  跳过它不认识的东西，按回合的 `summary` 保留它的总计
  （`docs/state/rollout-records.md`）。

- **无头模式：`zeroth -p "prompt"` 完全不带终端地跑。** 以前这个 app 需要一个
  会回应它光标查询的终端才能读 prompt，所以 cron 任务、CI 步骤或 n8n 节点都跑
  不了它（`Device not configured`）。用 `-p` 时同一个 session 不带屏幕运行：
  答案进 stdout，运行日志进 stderr，退出状态说明结果（0 完成，1 失败，2 用法
  错误，130/143 被中断）。`--output-format json` 打印一个结果对象，带 session
  id、用量和任何被拒绝的 tool call；`stream-json` 在运行过程中每行打印一个
  事件。prompt 也可以来自 stdin，如
  `git diff | zeroth -p "Review this diff"`。什么都不等回复：一个本该请求批准
  的 call 会被拒绝并告知模型，`--permission-mode` 决定多少东西不经询问就跑，
  提问工具不提供。`--max-turns` 给模型回复数封顶，`-C` 设置工作目录，
  `-c`/`--resume` 继续一段对话，所以一个每晚的任务可以让同一个 session 延续
  下去。配置、provider、skill、agent、hook 和 MCP 服务器都是交互式 app 用的
  那一套（`docs/cli/headless.md`）。除非出了事，stderr 保持安静 —— 失败的一次
  运行会说明原因，被拒绝的 call 和被停掉的后台工作会被点名 —— 而 `--verbose`
  把每一步都记到那里。管道输入会被读到底，不管写它的命令多慢；而没人写的
  socket（Node，也即 n8n）3 秒后就不管了；被拒绝的 call 按 Claude Code 的
  `permission_denials` 字段上报；auto 模式的安全检查判不下来的 call（那是
  错误，不是拒绝）最多重试一次，而不是永久拒绝；起不来的一次运行仍会打印一个
  JSON 结果；永不结束的后台 agent 会在
  `ZEROTH_HEADLESS_AGENT_WAIT_SECS`（默认 10 分钟）之后被停掉。没有终端时
  不用 `-p` 启动，app 现在会说它需要一个终端并点名 `-p`，而不是以
  `Error: Os { code: 6, … }` 收场。

- **`scripts/preflight.sh` 在一次红灯运行被读之前先检查这台机器。** 它每个
  工具打一行 —— 钉住的工具链、rustfmt/clippy、C 编译器、`mbx`，带 `--smoke`
  时还有 `tmux`、bash 4+ 和 `setsid` —— 并为缺的那一个点出修法，以非零码
  退出。在 macOS 上这就是「这些失败里哪个是我的」的答案：没有 `setsid`，
  46 个单元测试会以*在这里没法给命令一个它自己的终端*失败，而在 bash 3.2 下
  smoke 套件会以 `declare: -A: invalid option` 死掉，两者都不说明哪里不对。

- **自己的 provider 可以把思考档位当作 `reasoning_effort` 收下。** Ctrl+T 的
  档位乘在每次 chat 请求的 OpenRouter `reasoning` 对象里，有些 OpenAI 兼容
  服务器不理它：OmniRoute 通向 Claude 模型的路由只读 OpenAI 自己的顶层
  `reasoning_effort`，所以为它们选定的档位什么也没改变。
  `~/.zeroth/providers.toml` 里一块设了
  `reasoning_param = "reasoning_effort"` 的配置现在改为把档位作为那个字符串
  发出，**off** 发 `none`（`docs/providers/user-providers.md`）。

- **可以从 `/settings` 把 MCP 对整个 session 关掉。** 新的一行 **MCP** 停掉
  每一个 MCP 服务器、一个工具都不给模型，而不用在 `/mcp` 里一个一个禁用：
  每个服务器保住自己启用或禁用的状态，所以把这一行重新打开，重连的正好是原本
  启用的那些。这个选择和 **Hooks** 一样按目录保存。MCP 关着时，`/mcp` 仍能
  打开并说明这一点 —— 在列表上，也在它持有的每个服务器的页面上，写作
  `◯ off` —— 启动时的那次复核也会等 MCP 重新打开之后，才来问项目的服务器。
  `ZEROTH_MCP=0` 现在让一次运行以一个该行关闭的 session 启动，而从前它会
  把 MCP 摘掉直到重启（`docs/tools/mcp.md`）。

- **在回合边缘运行的 hook 现在会显示它们做了什么。** 一个让 agent 继续走下去
  的 Stop hook 在对话里没留下任何痕迹，一个失败了的、答了 `continue: false`
  的、或返回了 `systemMessage` 的 hook 则被悄悄丢掉。现在每一个都会在回复
  下方留下一个 `● Ran 1 Stop hook` 单元格：agent 据以继续的反馈、hook 停住
  它的原因、它的消息，以及坏掉的 hook 的红色 `Stop hook error: …`，点名是哪
  个 hook，每一段都像 tool 的输出那样折叠。**Ctrl+O** 把单元格展开成每一个跑
  过的 hook、它们跑了多久、以及配置它们的那个 `hooks.json`。SessionStart 和
  UserPromptSubmit hook 在它们添加上下文或说了什么时，也在你的 prompt 下方
  得到同样的单元格；PreCompact 和 PostCompact hook 在 `Context compacted`
  之上，StopFailure hook 在失败回合的错误之上，子代理的 SubagentStart 和
  SubagentStop hook 在它自己的 transcript 上。以 2 退出的 SessionStart、
  SubagentStart、PreCompact 或 PostCompact hook 没有可阻断的东西，所以它的
  原因显示在它的单元格里而不是被丢掉。改写了一个 call 参数的 PreToolUse hook
  现在在 call 上说明这一点，位于一行暗色的
  `Rewritten by PreToolUse hook` 之下，而从前那个单元格把 hook 的版本显示得
  像是 agent 自己要的。agent 读到的东西不变，安静跑的 hook 仍留下什么都没有
  （`docs/tools/hooks.md`）。

### 修复

- **`install.sh --preview` 现在画的是安装时真正画出的那条横幅。** 它过去即使在
  会显示图片横幅的终端上也只播放文字吉祥物，副标题还是过期的
  “the one-line installer”——而用户装之前看主题，跑的就是这一次，它说的却不是
  真执行时看到的东西。预览现在走和安装同一条横幅路径，图也照同样的方式取，取
  不到就退回文字吉祥物；它仍然不查 release、什么都不安装。中途被打断时光标会被
  恢复，以前那个提前的 `exit` 会跳过这一步。图现在也能完整送达了：块数曾用
  `wc -l` 数，而 BSD 的 `fold` 不给最后一行补换行，于是数少了一块，真正的尾块被
  标成「还有更多」，终端就一直等永远不会到的字节（`EBADPNG`，只画出半个横幅）。
  协议对传输的回复也一并关掉了（`q=2`）——那条回复是写进终端*输入*的，安装器一
  退出 shell 就把它当命令读了。构图也不再靠猜格子：横幅按列数摆放、不给行数，终端
  于是照着图片自身的长宽比缩放，而不是把它拉伸进一个 8×7 的框；协议默认的光标策略
  把吉祥物放在横幅下方，而不是压进横幅里。名字像应用页眉那样带上版本——
  `Zeroth (v0.4.2)`，来自 pin 住的 `ZEROTH_VERSION` 或 checkout 旁的 `Cargo.toml`，
  绝不联网查。`--preview N` 会在 `Preview` 进度条上真等满那几秒，不再是一印完图就
  返回（`docs/ops/release.md`）。

- **Antigravity 登录不再冻结——也不再让界面崩溃。** 它的可达性探测（一次
  30 秒的 `HEAD`）跑在 async 事件循环上，并在那里构建 blocking 的 `reqwest`
  客户端。界面在整个探测期限内冻结，而在 async 上下文里丢弃那个客户端会以
  `Cannot drop a runtime in a context where blocking is not allowed` panic，
  就在登录页打开的那一刻。探测现在跑在登录 worker 上，flow 的其余部分本来
  就在那里阻塞；Google 不可达仍然在浏览器打开之前报告在页面上
  （`docs/providers/antigravity.md`、冒烟阶段 148）。
- **一次失败的启动会把终端原样还回去，一个字节都不差。** 在 macOS 上，当
  canonical 输入恢复时驱动会置起 `PENDIN`，而即使没有任何 pending 输入，
  `stty -g` 也会把它报告成模式变了。现在每一条恢复路径——失败的 `init`、
  正常退出和 panic hook——都会服务输入队列并清掉它，于是
  `zeroth "prompt"` 在没有终端应答时也能把 pane 精确还原（`src/term.rs`、
  冒烟阶段 153）。
- **剩余的 harness 安全 P0。** auto 模式的只读快路径不再放过点名私钥、云凭证、
  dotenv 文件、令牌库或环境变量的命令（`cat ~/.ssh/id_rsa`、
  `base64 ~/.aws/credentials`、`printenv`、`echo $OPENAI_API_KEY`）：这些一律
  交给 classifier，它的规则覆盖了它们。provider 的 HTTP 客户端只跟随
  **同源**重定向，所以 `x-api-key` 和配置里的 header 再也不能搭上跨源的一跳。
  `write`/`edit` 在执行器处拒绝符号链接目标 —— 悬空的也算 —— workspace 的
  路径 jail 同样拒绝符号链接目标。远程 MCP 服务器 URL 必须是 `https`
  （只有 localhost 服务器才允许 `http`），rollout/checkpoint 存储以
  `0600`/`0700` 创建，checkpoint 的排除项里含 `.env`、密钥文件和凭证库
  （`docs/state/permissions.md`、
  `docs/research/harness-network-security.md`）。
- **经由 skill、agent 与抓取到的网页内容的 prompt 注入。** skill 与 agent 的
  遍历把 `$HOME` 当成了一个项目：在 `~` 里启动会读 `~/.alter-zero/skills`
  （fork 之前的个人根目录），所以装在那里的一个 `SKILL.md` 能劫持一个毫不相干
  的问题 —— 这是一次真实事故，一个 skill 让 `zeroth -p 你是谁` 换上了另一个
  人格，并告诉模型放弃权限和范围。现在 home 永远不是一个项目（这是配置层
  已有的规则），两条遍历都是如此。纵深防御：默认 system prompt 声明 tool 的
  结果只是数据，不能改变身份、安全规则或权限；skill/agent 清单说明描述是
  作者写的元数据，不是指令；加载的 skill 正文与每一个抓取到的页面/搜索结果都
  带着一条明确不可信内容的边界（`docs/tools/skills.md`、
  `docs/tools/web-search.md`）。项目的 `AGENTS.md`/`AGENTS.override.md` 不再
  声称「OVERRIDE any default behavior」：指令的前言和提醒自己的前言都说明它们
  是项目指导，不能改变身份、安全规则或权限，home 同样永远不是项目文档根
  （在 `~` 里启动时，`~/AGENTS.md` 不会被当作签入的指南读）。链接卡的 host
  在 DNS 解析之后被拒绝（`nip.io`、IPv6 映射与十六进制的写法），而在 session
  workspace 与 scratchpad 之外的 `write`/`edit` 现在即使在 `auto`/`edit`
  模式下也会发问（字面的 `..` 被拒绝，已存在的路径先规范化，所以符号链接
  指不出去；`master` 仍然绕过）。

### 变更

- **吉祥物 PNG 只剩原来四分之一的体积，在二进制里省得更多。**
  `assets/mascot-crest.png` 本来是 1312×1199——为了画一个 18 列的小标记要 561 KB
  ——于是每个图形终端启动都背着它，安装器也要用 138 个协议分块才能发完。改成
  384×351 之后，屏幕上那个标记没变，文件是 78 KB，分块数变成 18。它声明的尺寸
  现在叫 `ui::MASCOT_ICON_PX`，并由一条测试拿文件自己的 IHDR 钉住；图标缓存也
  迁到 `mascot-crest-v2.png`，免得某次构建把旧的缓存副本按新尺寸画出来
  （`docs/ui/mascot.md`）。

- **一次发布在构建任何东西之前先跑终端套件。** Release workflow 现在和单元
  闸门一起，把 tmux 冒烟套件当作 gate，于是一个 tag 不能再从一个终端边界是
  红的提交里产出四个已验证的归档。套件本身也跟上了改名：过时的横幅、argv 和
  `.alter-zero` fixture，以及旧的 `provider/model` footer 断言都已更新，
  更新日的遥测 fixture 也不再 stage 它正在测试的那个版本
  （`docs/ops/release.md`）。
- **残留的 `alter-zero` 名称已全部清除 —— 破坏性改名。** 所有
  `ALTER_ZERO_*` 覆盖变量改为 `ZEROTH_*`，没有回退：用旧变量的脚本、hook
  与 CI 都得改名。会话临时根是
  `{tmp}/zeroth-{uid}/{session}/scratchpad`（旧会话的 scratchpad 不再享有
  写入权限豁免），剪贴板暂存文件用 `.zeroth-paste-`，遥测 User-Agent 是
  `zeroth/{version}`。文档、smoke 阶段与夹具里的标题、帮助页、回调页现在
  统一写 `Zeroth`，与 `APP_NAME` 一致。上游链接、`NOTICE`、fork 历史、
  一次性的 `~/.alter-zero/` 主目录迁移，以及已发布的
  `@linuztx/alter-zero` npm 包名，作为历史引用保留
  （`docs/architecture/naming.md`）。

- **项目级配置现在住在 `.zeroth/`。** 项目层读
  `{root}/.zeroth/{skills,agents,hooks.json,mcp.json}`（外加 Claude Code 兼容的
  `{root}/.mcp.json`）；项目的 `.alter-zero/` 不再被读取，于是 Zeroth 和上游
  那个 app 可以待在同一个仓库里而不共享 hook、MCP 服务器、skill 或 agent。
  对依赖旧目录的项目来说这是破坏性变更 —— 把它挪到 `.zeroth/`
  （`docs/state/project-config.md`）。

- **那个一行安装脚本是 Zeroth 自己的了，它的 mascot 也动了。**
  `install.sh` 现在从 `yxxbc/zeroth` 解析发布并安装 `zeroth` 这个二进制，
  按仓库 slug 命名，就像发布归档按 Cargo.toml 的 `name` 命名一样；
  `ZEROTH_REPO` 可以为 fixture 发布覆盖这个 slug。它的横幅是 app 默认的
  `crest` mascot，用横幅的渐变画出来，在终端上会动 —— 色调沿行向下走、冠饰
  摆动、眼睛眨 —— 大约一秒半，然后作为 Download 行的 spinner，报告已经落了
  多少字节。`--preview`（也可写作 `--proview`）单独播放 mascot，默认十秒，
  `--preview 20` 或 `--preview=20` 可以给任何整数，不做平台检查、不联网、
  不安装。在管道里、`NO_COLOR` 下或非 UTF-8 的 locale 里，图案只画一次且
  静止，和从前一样（`docs/ops/release.md`）。

- **启动横幅的提示会看状态。** 一个配置好的 session 教的是打字之外的三条入口
  —— `/ commands`、`@ files`、`? shortcuts` —— 记号用强调色、名字用暗色，
  而不是点名三个可能用不上的命令。跑在离线 demo 上的 session（没有解析出
  provider、模型和 key）显示的是唯一能改变这一点的动作：
  `Connect a model → /login`（`docs/header.md`）。

- **页脚把 provider 括在模型自己的名字旁边。** 输入框下的模型标题现在读作
  `[provider]model-suffix` ——
  `commandcode/deepseek/deepseek-v4.1-flash` 写成
  `[magpie]deepseek-v4.1-flash` —— provider 用页脚的暗色，模型用它的强调色，
  这样这一行就不用再重复 provider 的前缀。离线 demo（没有 provider）保留
  光秃秃的模型名，完整 id 仍在 `/model` 和 Ctrl+O 里（`docs/footer.md`）。

- **`/model` picker 按 provider 给列表分组。** 跨好几个 provider 的列表现在先
  按 provider、再按 id 排序，并在每组之前画一个粗体的 provider 标题；标题在
  屏幕上时那里的 `[provider]` 标签就去掉，因为标题已经说了（一个从组中间开始
  的窗口保留这个标签）。只有一个 provider 的列表 —— 以及收窄到某一个 provider
  的搜索 —— 保留旧的平铺行和标签。选中、翻页和 `(n/total)` 计数仍然只在模型
  之间步进，绝不落到标题上（`docs/llm.md`）。

- **发布二进制小了四分之一还多，画得也更快。** 发布现在带全程序链接时优化、
  在自己的 build profile 里构建，编译器一次看到每个 crate：Linux x86_64 的
  二进制从 25.6 MB 变成 18.6 MB，下载从 10.1 MB 变成 8.6 MB，重画一条很长的
  代码回复快了 4–13%。hook 的 matcher 也不再带着正则引擎对长文本的加速，因为
  matcher 只会看到一个 tool 名；它们匹配的结果和从前完全一样。
  `RUST_BACKTRACE=1` 的报告仍点名函数，从 checkout 里跑
  `cargo build --release` 不变（`docs/ops/binary-size.md`）。

- **smoke 套件能在 macOS 上跑了。** 有几个阶段是 Linux 形状的，在 Mac 上
  *悄悄*失败而不是报红：`stat -c%s` 打一条错误和一个空字符串，所以一个在
  `$(stat -c%s f) + 1` 处切分字节流的阶段把整个文件当成了它的后一半，还通过了；
  `sed -i`、`sha256sum` 和 `readlink -f` 是 GNU 的；BSD 的 `wc` 用空格补齐它
  的计数；而 macOS 的临时目录是 `/var/...` 名字背后真实的 `/private/var/...`，
  所以一个埋下绝对路径再拿一行去比对的阶段永远匹配不上。现在这些是
  `smoke_abs_path`、`smoke_real_path`、`smoke_file_size`、`smoke_file_mode`
  和 `smoke_sed_in_place`，由 `scripts/smoke/selftest.sh` 钉住；那三个以
  *没有*原生剪贴板为 fixture 的阶段在 GUI 登录下报 `skip`（runner 把它和通过
  分开计数），而不是覆盖开发者的粘贴板。一次完整的 macOS 运行从 9 个失败变成
  0 个（`docs/smoke.md`，*Portability*）。
- **上下文量表在回复与回复之间跟着对话走，还学会了你的 provider 怎么数。**
  页脚的 `{used}/{window}` 过去只在 provider 报告一次请求时移动，所以一次大的
  粘贴或一段长的 tool 输出要晚一整个请求才露出来。现在两者中任何一个被加进去
  的那一刻它就长。它从 provider 自己对上一次请求的计数出发，再加上自那以后
  加入的东西的估算。每次报告也训练这个估算去了解该模型的 tokenizer 怎么比，
  所以加入的那一份按那个模型的 token 计。量表也不再计入推理模型隐藏的推理，
  因为下一次请求并不携带它：从前它每次回复之后都读得偏高，高到足以提前开始
  一次压缩。在一次压缩或一次回退之后，它保留 provider 对每个请求都携带的东西
  的计数，比如 tool schema，从前压缩的 `before → after` 会把它丢掉，让它看
  起来省下得比实际更多。在第一次报告之前它也会数上 tool schema。`/context`
  页面和 Ctrl+D 视图用同样校准过的 token 计数，所以它们读起来和页脚一致
  （`docs/architecture/token-estimate.md`）。

- **token 计数不再随包携带 OpenAI 的词表。** 状态行的统计和每一次上下文估算
  都用 OpenAI 的 `o200k_base` 编码来数，那只对 OpenAI 模型精确，却意味着二进制
  里装着一份 1.6 MB 的词表、加载后约 5 MB 内存，以及启动时一个加载它的线程。
  一个不带词表的估算取代了它，拟合在 OpenAI 现在这两份词表之间：在代码、英文、
  JSON 和 shell 输出上两者都在几个百分点之内，在其他语言上则落在两者之间 ——
  那正是词表差别最大的地方。然后每个 provider 用自己的报告来纠正它。二进制更
  小了，启动线程也没了。

- **`/` 命令 palette 用 `→` 标出高亮的命令。** palette 过去只靠颜色表示 Enter
  会跑哪条命令，单色终端或一张普通的屏幕截图都看不出来。高亮行现在以
  `@` 文件 picker 和 `$` skill picker 早已在用的那个 `→` 开头，其他每一行以
  两个空格开头，于是命令名对齐在同一列里。`No matching commands`、
  `No matching files` 或 `No matching skills` 行现在也坐在同一列。

- **`/` 命令 palette 一次列五条命令，还带一个计数。** 它过去显示八条，在小
  终端上占掉输入框下面一大块。现在显示五条，高亮的那条留在中间，↑/↓ 滚过
  其余的，下面一行暗色的 `(1/21)` 计数说明高亮的是哪条、总共有多少条，或者
  有多少条匹配你打的字。窄到描述要换行的终端显示得更少，而 palette 占的行
  永不比它从前的八行更多。

- **`$` skill picker 一次列五个 skill，还带一个计数。** 它过去最多显示八个。
  现在显示五个，高亮的那条留在中间，↑/↓ 滚过其余的，方式与 `/` palette 滚动
  一样，下面是和 palette 一样的暗色计数 `(1/7)`，说明高亮的是哪个 skill、
  有多少个匹配你打的字（`docs/input/skill-mentions.md`）。

- **`@` 文件 picker 列出每一个匹配的文件，一次五个，还带一个计数。** 它过去
  只显示最好的八个匹配，再往后的什么都没有，所以 ↑/↓ 永远够不到第九个。现在
  它保留每一个匹配你所打的字的文件和文件夹，一次显示五个，高亮的那条留在中间
  供 ↑/↓ 滚动，并在列表下面数它们，`(1/12)`，就像 `/` palette 和 `$` picker
  那样。光秃秃一个 `@` 会像目录清单读起来那样列出整个 workspace：工作目录
  自己的条目在前，文件夹在文件之前，然后逐层往下。搜索仍在 10,000 个条目之后
  停下，好让一个巨大的目录保持迅速；停下时计数以 `+` 结尾，如
  `(1/10000+)`，说明这个列表是不完整的（`docs/input/file-search.md`）。

- **各个 picker 以标题开场，并且一次显示更少的行。** `/model`、`/settings`、
  `/theme`、`/mascot` 和 `/spinner` 现在以标题开场（`Select model`、
  `Settings`、`Select theme`、`Select mascot`、`Select spinner`），下面是
  一行暗色文字，说明这个选择改变什么、持续多久，就像 `/skills` 以 `Skills`
  和它的计数开场那样。在每一个 picker 里，标记高亮行的 `→` 现在坐在 `❯`
  搜索行下面、缩进两列，`(n/total)` 计数、高亮行的描述和 `/model` 的
  `Model Name:` 行都从名字下面开始。`/theme`、`/mascot` 和 `/spinner` 过去
  用第二个 `❯` 标记高亮行，现在改显示同样的计数，取代它们原来的
  `↑ N more above` 和 `↓ N more below` 行。列表 `/model` 里一次最多七行，
  `/settings`、`/skills`、`/mascot` 和 `/spinner` 里最多五行，而它们从前最多
  显示十行，于是 picker 不再填满一个小终端。`/theme` 保留它的十行，那些行
  并不比旁边的预览高。`/skills` 的提示现在读作 `Space to toggle`；Enter 仍然
  也能切换。在短到放不下整页的终端上，只要两者都放得下，标题就留在三行列表
  之上（`docs/ui/view-flow.md`）。

- **`/login` 以 `Select login method` 开场。** 第一页现在在一个标题下问你怎么
  登录，没有搜索行，因为三个选项是一个问题而不是一份要过滤的列表。它的行读作
  `Sign in with an account`、`Sign in with an API key` 和
  `Sign in with a local model`，而从前读作 `Use a subscription`、
  `Use an API key` 和 `Use a local model`：第一行背后并不每个账号都是订阅，
  因为 Anthropic Console 的登录是按 token 计费的。那些把你送到这些行去的
  消息，比如什么都没登进去时的 `/usage`，用新的说法点名它们
  （`docs/providers/llm.md`）。

- **`?` 快捷键列表自己适配终端。** 它过去是一张固定的两列网格，在宽终端上把
  右边留空，在输入框下面占八行。现在它用宽度容得下的列数，而每个快捷键保持
  完整：80 列下三列（六行），大约 100 列起四列，宽屏上最多六列（三行）。在
  窄窗格里它靠从每个快捷键里去掉 "for" 和 "to"（`ctrl+r search history`）
  保住两列，只有当这样也放不下时才一行一个，而从前它会从词中间把第二列切掉。
  多余宽度把各列摊到整行上，列表沿每列往下读：先是输入键和历史，然后是编辑
  和运行中的回合，再是各种视图、模式和退出。在短到放不下整张列表的终端上，
  它显示放得下的部分，并以 `… +N more` 结尾，而不是悄悄丢掉最后几个快捷键。
  它从缩进两列处开始，和它所取代的页脚行一样，现在还列出 `@` 用于文件路径、
  `ctrl+b` 用于把运行中的命令移到后台。`ctrl+o` 现在读作 `for transcript`，
  也就是它打开的视图，而不是 `for tool output`（`docs/ui/shortcuts.md`）。

- **状态行和页脚现在穿上主题的颜色。** 状态动词（`Working…`，或某个任务的
  `Setting up project structure…`）停在和 `/mcp`、`/skills` 标题一样的渐变里，
  shimmer 仍在它上面扫过，模型思考时 `Thinking for 4s` 是紫色。页脚里模型名
  用强调色，思考模式和速度档（`medium fast`）用和 `Thinking for` 一样的紫色，
  工作目录用绿色，横幅里的路径现在也跟着匹配。上下文量表在窗口填到 75% 之前
  保持暗色，之后转琥珀，到自动压缩阈值转红，与 `/context` 页面用的是同一条
  规则。右边缘的权限模式随它让 agent 不问就做多少事而升温：`manual` 保持暗色，
  `edit` 是蓝色，`auto` 是琥珀，`master` 是红色。每一种颜色都来自活动的
  `/theme`，而消息、回复和 tool 单元格不变（`docs/ui/footer.md`、
  `docs/ui/status-indicator.md`）。

- **Ctrl+D 上下文视图更好读，也更好挪动。** 每条消息现在以一个带颜色的
  `SYSTEM`、`USER`、`ASSISTANT` 或 `TOOL` 标签开场，旁边是它在请求里的位置
  和一个粗略的 token 计数，它的正文坐在一条 `⎿` 装订线之下，在词与词之间
  换行，而不是从词中间断开。tool 结果点名它答复的那个 call，按 tool 和 call
  id 点名，而每个 tool call 以自己的 id 结尾，于是一个批次的结果与它们的
  call 配得上对。回复结尾的空行不再给它后面 tool call 之前的地方垫空。
  **←/→** 跳到上一条或下一条消息，你滚进去的那条消息把它的标签留在顶行，
  底部那条横线显示窗口里装着多少条消息、它们加起来大约多少 token —— 和页脚
  上下文量表显示的是同一个估算。在子代理的 session 视图里，标题点名你正在读
  谁的 context。Tab 那一页，也就是 auto 模式 classifier 读的东西，外观相同。

### 移除

- **npm、Docker、AUR 和遥测收集器的打包。** 这个 fork 只发布二进制和它的
  GitHub 发布归档；那些打包目录树、它们的 CI 任务和描述它们的文档都没了。

- **`/help`。** 它把命令列表打印进对话，而 `/` palette 已经显示了这些：
  每条命令和它做什么。敲 `/` 就能浏览它们。

### 修复

- **名字里带反斜杠的文件，`@` 会用那个名字提供它。** 在 Linux 和 macOS 上
  `\` 是文件名里的普通字符，但 `@` picker 把它变成了文件夹分隔符，于是
  `a\b.txt` 被列成一个并不存在的文件夹 `a` 里的 `b.txt`，选中它会把一个
  什么也没指的路径放进你的消息（`docs/input/file-search.md`）。

- **OpenAI 兼容网关发现的模型留在 `/model` 里。** OmniRoute 把它在上游找到的
  模型，比如一个 Codex 模型，按上游自己的 endpoint 列出来
  （`"supported_endpoints": ["responses"]`），却照样通过 chat completions 提供
  它。那份列表被那条本来给 GitHub Copilot 记录用的规则读了，而那条规则会漏掉
  只从 `/responses` 提供的模型，于是每一个这样的模型都从 picker 里消失了。
  现在只有 Copilot 自己的记录才按它们的 endpoint 过滤
  （`docs/providers/copilot.md`）。

- **OpenAI 兼容网关背后的模型能看见你发的图片了，Ctrl+T 也提供它们的思考
  档位。** 像 OmniRoute 这样的网关把每个模型的能力列为朴素的 flag
  （`"vision": true`、`"thinking": true`），而 `thinking` 这个 flag 让它的
  列表读起来像是 Anthropic 自己的，后者把每项能力拼成 `{"supported": …}`。
  于是网关背后的每个模型都被算成了看不见也不会思考的那一类：**Ctrl+V** 用
  `does not support image input` 拒绝一张图片，`read` tool 拒绝图片文件 ——
  尽管网关会把它们透传 —— **Ctrl+T** 也没有思考档位可循环。现在一个模型的
  图片输入来自它自己记录点名的模态，记录没点名时则来自它的 `vision` flag，
  Ctrl+T 循环网关为这个模型列出的档位（`effort_tiers`），只有那份列表里含
  `none` 时才提供 Off。在这个修复之前选定的模型会保留为它保存的东西，直到你
  在 `/model` 里重新选它一次（`docs/providers/user-providers.md`）。

- **坏掉的 hook 用它命令的第一行来点名。** 当一个写成多行内联脚本的 hook
  失败、超时或打出无效 JSON 时，它在 tool call 下面留下的那行暗色文字装着
  整个脚本。现在 hook 用它命令的第一行来点名，截到 80 个字符，那里和
  hook-run 单元格上都是如此（`docs/tools/hooks.md`）。

- **在 macOS 上，`bashkill` 让命令在被杀之前先收拾干净。** 停一个 session 会
  先发 `SIGINT`、再发 `SIGTERM`，2 秒后杀掉剩下的东西，所以一次
  `docker compose up` 能把它的容器带下去。这两个信号走的是 `pkill -s`，而
  macOS 的 `pkill` 没有 `-s`：它拒绝这个选项，什么信号都没发，于是那里的每次
  停止都成了 `SIGKILL`，却仍然报告说 session 已停。在 `pkill` 找不到 session
  的地方，现在这次停止按 session 的终端找它的进程，最后一次 kill 在那里也能
  打到 shell 自己的作业（`docs/tools/bash-tools.md`）。

## [0.0.0] - 2026-11-12

Zeroth 的开发基线。第一个打上标签的发布将决定它从哪个版本起步；这一条下面的
一切都是 Alter Zero 的历史。

## [0.18.0] - 2026-11-03

### 新增

- **用 Google 账号登录，以使用 Google Antigravity 的模型。**
  `/login` → **Use a subscription** → **Google Antigravity** 会在你的浏览器里
  打开 Google 的登录并自己回来；`/model` 随后列出你账号的 Antigravity 配额
  覆盖的 Gemini、Claude 和 GPT-OSS 模型，一个模型的 Low/Medium/High 变体折成
  一行，档位由 **Ctrl+T** 循环，而 `/usage` 显示每份配额还剩多少、什么时候
  重置。Antigravity 从未设置过的账号会在登录过程中被设置好。这是一条非官方
  路径，用的是 Antigravity 自己的登录客户端，Google 可能限制在 Antigravity
  之外使用它的账号，所以登录前请斟酌（`docs/providers/antigravity.md`）。
- **登录需要你自己的 OAuth 客户端的 MCP 服务器。** 有些服务器 —— Google 的
  就在其中 —— 只接受你在 provider 控制台里创建的登录客户端，登录它们会以错误
  收场。`mcp.json` 里一个服务器的条目现在可以在 `oauth` 块里点名那个客户端
  （`clientId`，以及如果它的设置要求的话还有 `callbackPort` 和 `scopes`），
  或者 `zeroth mcp add <name> --url <url> --client-id <id>
  --client-secret` 把它设置好：密钥会被询问，或从 `MCP_CLIENT_SECRET` 读取，
  并与你的 MCP 登录放在一起而不是放在文件里。`/mcp` 会立刻把这样的服务器
  显示为需要认证，Google 的登录会自己续期而不是在一小时之内失效，
  `zeroth mcp get` 显示客户端但永不显示它的密钥。这是实验性的：以这种方式
  登录 Google 的服务器还没有端到端试过（`docs/tools/mcp.md`、
  `docs/cli/mcp-cli.md`）。
- **一个 MCP 服务器的新 tool 一有就会被提供。** 一个在运行中 tool 会变的
  服务器 —— 你在开发的，或者会打开关掉 tool 集合的 —— 会说出这一点，而客户端
  过去会一直攥着连接时读到的那份列表，直到你在 `/mcp` 里重连这个服务器。现在
  服务器宣布变更时它会重新读那份列表，agent 从下一次请求起就能用上新的 tool。
  按最新 MCP 版本工作的服务器会被要求发这些通知，这是该版本的要求
  （`docs/tools/mcp.md`）。
- **MCP 服务器的说明能到达 agent 了。** 一个服务器可以说明它的 tool 该怎么用：
  先调哪个、某个参数是什么意思、要避免什么。那段文字过去只在 `/mcp` 里该
  服务器的页面上显示，别处没有。现在它随每个回合发给 agent，覆盖每一个连着的、
  它用得上其 tool 的服务器，而子代理会拿到它可能调用的那些服务器的说明。
  每个服务器的文字都有上限、不能冒充别的服务器的，并作为 **MCP
  instructions** 出现在 `/context` 里（`docs/tools/mcp.md`）。
- **hook 和项目的 MCP 服务器在运行之前先被复核。** hook 在它的事件触发时自行
  运行一条 shell 命令，而项目的 `.mcp.json` 或 `.alter-zero/mcp.json` 在
  session 一打开就启动一个程序或连上一个 URL，所以在你见过它们之前，两者都
  不再运行。当 app 启动时发现一个 hook 是新的、或自你信任它之后改过（在你
  自己的 `~/.alter-zero/hooks.json` 里，或在项目的 `.alter-zero/hooks.json`
  里），或者发现一个它没问过的项目服务器，它会先于一切打开一次复核：先是
  hook，然后是服务器，每一个列在它来自的文件之下，带着它究竟运行什么，每条
  命令都是完整的。一切都以选中开始；**Space** 取消选中一个，**Enter** 确认。
  被信任的 hook 从那时起运行，被改过的一个会被再问一次，并把它过去运行的东西
  显示在旁边；你留着不选的、或用 **Esc** 跳过的 hook（"Continue without
  trusting"）保持关闭，并在下一次启动时被问到。你启用的服务器立刻连接；其余
  的被禁用并记住，而 **Esc** 把它们全部禁用。条目变了的服务器会被再问一次，
  并点名变的是什么：加了一个 `env` 变量、换了一条命令行、多了一个 header。
  **Ctrl+C** 什么也不决定就退出。复核开着的时候，`SessionStart` hook 等待，
  命令行上给出的 prompt 在你答完之后才发，而 [herdr](https://herdr.dev) 窗格
  把该 session 显示为阻塞。你的选择保存在 `~/.alter-zero/trust.json` 里，
  永不放在项目里。它不保存任何服务器的命令行、URL 或 token，只有指纹和名字。
  两个 session 同时作答会保住彼此的答案，而一个不再能解析的 `trust.json` 就
  原样放着：你的选择此后只在本次 session 有效，并有一条消息说明这一点
  （`docs/state/config-review.md`）。
- **`/hooks` 信任一个 hook，或撤销对它的信任。** 浏览器把不受信任的 hook 标上
  `⚠ not trusted`，并说明有多少个不运行；在 hook 的页面上按 **Enter** 立刻
  信任它或撤销信任。在 `/settings` 里对着等待信任的 hook 打开 **Hooks**，会
  打开复核的 hook 页，**Ctrl+C** 像关任何 picker 一样把它关掉。在 `/mcp` 里，
  对项目的服务器点 Enable 和 Disable 改变的正是复核记录下的那个决定
  （`docs/tools/hooks-menu.md`）。
- **读取、搜索和列目录折成一行。** 一个在项目里摸路的 agent 过去会用每次调用
  一个单元格把屏幕填满：每一次 `ls`、每一次 `grep`、每一个它读的文件，每个
  下面都是它的输出。一串这样的调用现在是一个单元格，说明这一串在做什么，
  `● Searching for 1 pattern, reading 2 files, listing 1 directory…`，下面是
  它正在处理的命令或文件，一行放不下时换到更多行而不是被截短。当 agent 同时
  跑好几个时，这一行按它们运行的顺序轮流点名每一个，每个 0.7 秒，而不是直接
  跳到最后一个；跑过两秒还没结束的命令会显示它跑了多久、打出了多少行。它的
  圆点在调用之间继续闪，因为下一个往往还是读取；一旦别的事情发生（另一种
  调用、一条回复、一个错误、回合结束），它就沉降成一行暗色文字：
  `Searched for 1
  pattern, read 2 files, listed 1 directory (ctrl+o to expand)`。agent 在这些
  调用之间想的东西留在对话之外：它思考时单元格下面不打开思考块（状态行说它
  在思考），它想完之后也不加一行 `Thought for …`。Ctrl+O 仍显示每一个思考和
  每个调用及其完整输出。被拒绝、被中断或移到后台的读取保有自己的单元格，读
  一张图片的也是，这样它的图仍然画得出来。`/settings` 里新的一行
  **Verbose output** 把每次调用一个单元格、每个思考一行找回来，
  `ALTER_ZERO_VERBOSE=1` 为一次运行设定它（`docs/ui/tool-groups.md`）。
- **scratchpad 文件也折进那一行。** agent 把它的一次性活儿 —— 试东西的脚本或
  一条笔记 —— 放在 session 的 scratchpad 里，过去它在那里写或改的每个文件都
  得到一个完整的单元格，下面带着文件内容。那些写入和编辑现在与读取并列加入
  那一行：agent 写它时是 `● Making 1 scratchpad edit +55… (ctrl+o to
  expand)` 下面是文件路径，文件流入时增加的行数往上走，agent 接着往下做之后
  变成 `Made 1 scratchpad edit
  +55 (ctrl+o to expand)`，增加和删除的行数暗色显示在计数旁边。长路径在行下面
  换行，而不是丢掉文件名，而一个恢复的 session 会把早先那次运行写的文件同样
  折起来。失败或被拒绝的编辑保有自己的红色单元格，**Verbose output** 仍把每次
  调用一个单元格找回来（`docs/ui/tool-groups.md`）。

### 变更

- **auto 模式跑只读的命令时不再问 classifier。** 在 auto 模式下，每条命令过去
  都要等一次由模型做的安全检查，连 `ls` 或 `git status` 也是，而每次检查都是
  一次发往你 provider 的请求。一条能证明只读的命令（`ls -la`、
  `grep -rn foo src`、`cat README.md | head -20`、`git log --oneline`）现在
  直接就跑，下面没有 `Allowed by auto mode classifier` 的注记。检查无法确定的
  东西仍交给 classifier：重定向进一个文件、`$(…)`、未知的 flag、会写东西的
  flag 如 `find -delete` 或 `sort -o`，或者在一个看起来像裸仓库的文件夹里跑
  `git`。manual 和 edit 模式仍然对每条命令发问
  （`docs/state/permissions.md`）。
- **hook 默认就运行。** `/settings` 的 **Hooks** 行过去在每个文件夹里都是关的，
  直到你在那里打开它，所以一个 `~/.alter-zero/hooks.json` 在你没设置过的
  文件夹里什么也不做。现在它到处都开着，每个 hook 在你于上面那次复核中信任它
  之后就运行。在一个文件夹里关掉这一行会为该文件夹记住，而
  `ALTER_ZERO_HOOKS=0` 为一次运行关掉 hook（`docs/state/settings.md`）。
- **hook 告诉 agent 的东西很短，并且说明是哪个 hook 说的。** 当一个 hook 拒绝
  一次 tool call 时，agent 过去读到一段五行长的文字，里面从不说出是哪个 hook
  拒绝了它。现在它读到 codex 形状的一行，
  `Blocked by PreToolUse hook: no destructive deletes`，然后是 hook 的命令
  （`Hook: ~/.alter-zero/hooks/guard.sh`）和配置它的那个 `hooks.json`
  （`Config: /home/you/.alter-zero/hooks.json`，或者被信任的项目自己的），
  这样它既能读到那条规则，也能告诉你去哪里改；再下一行告诉它不要重试同一个
  call。`Stop` hook 的反馈、`PostToolUse` hook 的抱怨和 `PermissionRequest`
  hook 的拒绝都以同样的方式点名自己的 hook。hook 给一次 call 添加上下文时会被
  标注（`PostToolUse hook
  additional context: …`），所以它不再读起来像是这个 call 自己的输出；而
  `SubagentStart` hook 给一个新 agent 的上下文像 session 的那样被包起来，
  而不是像 lead 亲自说过那样到达。hook 拒绝一个 prompt 时显示一行，
  `Prompt blocked by UserPromptSubmit hook: …`，而没有给理由就拒绝的 hook 现在
  读作 `no reason given`，不再把 "blocked" 说两次（`docs/tools/hooks.md`）。
- **被拒绝的 tool call 显示是哪个 hook 拒绝了它。** 红色单元格过去只读
  `Blocked by hook: …`，再无别的，所以有好几个 hook、或者你自己的和项目的
  `hooks.json` 里都有 hook 时，你只能猜是哪一个停下了这个 call，而 agent 却
  被告知了。现在它点名事件、hook 和配置它的文件，与 agent 读到的一致：
  `Blocked by PreToolUse hook: Use pnpm instead of npm.`，下面是
  `Hook: ~/.alter-zero/hooks/pnpm-only.sh` 和
  `Config: ~/.alter-zero/hooks.json`。`PermissionRequest` hook 的拒绝读起来
  一样，而 hook 在一次 call 下面留下的那些暗色行也点名它的事件：
  `Context added by PostToolUse hook`、`Flagged by PostToolUse
  hook: …`、`Allowed by PermissionRequest hook`（`docs/tools/hooks.md`）。
- **登录之后浏览器落到的那一页看起来像 Alter Zero。** 在浏览器里登录一个 MCP
  服务器、ChatGPT、Anthropic Console 或 Google Antigravity，过去会停在一行
  白底黑字上。那一页现在在一个大标题之上用像素显示 app 的 mascot，背景是暗色
  的、隐隐有星（如果你的浏览器是浅色的，那就是浅色），它的颜色和动作告诉你
  结果如何：登录成功时 mascot 蹦起来冒星光，服务器拒绝时它摇头，下面跟着
  服务器自己的理由，而面对一个给更早一次登录的答复时它歪头。它时不时眨眼，
  如果系统要求减少动态它就静止不动。这一页内建在 app 里，不从互联网加载任何
  东西（`docs/providers/callback-page.md`）。

### 移除

- **`/trust`。** 它把项目的 `.alter-zero/hooks.json` 和 MCP 文件整个批准，
  而且只有你想起来去打开它才行；启动时的那次复核取代了它，连你自己的
  `hooks.json` 也覆盖，并且逐个询问每个 hook 和服务器。你用 `/trust` 信任过的
  项目，只要它的文件此后没变过就仍保有那次批准；被改过的一个会被问到
  （`docs/state/config-review.md`）。

### 修复

- **`/mcp` 的 Authenticate 页面上的登录链接点一下就开。** 链接比任何终端行都
  宽，所以要换行，而只有 `/login` 的登录页把它做成了真链接：在 `/mcp` 上你的
  终端把每一行看成独立的一行，所以点一下打开的是第一行那个被截断的地址，其余
  的行压根不是链接。现在它的每一行都打开整条链接，就是 `c` 复制、你的浏览器
  被送去的那一条，并且像 `/login` 的那样带下划线。`/mcp` 页面上服务器的地址，
  以及那里登录或连接错误中点名的一个网址，也能整个打开
  （`docs/ui/links.md`、`docs/tools/mcp.md`）。
- **Google 的 MCP 服务器不再在每次调用都失败时看起来还是连着的。**
  Google Drive、Calendar 和 Gmail 把它们的 tool 列给任何人，却拒绝不带登录的
  每一次调用，于是 `/mcp` 读作 "✔ connected" 和 "✔ authenticated"，agent 被
  提供了只会失败的 tool。这样的服务器现在读作 "△ needs authentication" 和
  "✘ not authenticated"，在你登录之前不提供它的 tool。这一类服务器还不被支持：
  它们的登录需要你用 provider 创建的 OAuth 客户端，那是实验性的。不需要登录
  就能用的服务器，比如 Hugging Face、Context7 和 Exa，保持连接
  （`docs/tools/mcp.md`）。
- **一个 tool 名很长或长得相像的 MCP 服务器不再弄坏每一次请求。** 每个 MCP
  tool 以 `mcp__{server}__{tool}` 的形式到达模型，而 provider 会因为一个坏的
  tool 名拒掉整个请求：一个超过 64 个字符的名字
  （`mcp__atlassian-confluence-cloud__` 已经有 33 个），或者两个在把名字承载
  不了的字符替换掉之后变得一样的 tool（`files.read` 和 `files_read`），会在
  那个服务器连着的时候让每个回合都失败。现在长名字被裁到合适，并以一个短哈希
  结尾，而长得相像或非 ASCII 的名字各得一个自己的，不论服务器以什么顺序列出
  它们结果都一样。`/mcp` 显示模型调用的那个名字。两个结果一样的服务器没法
  这样区分开，所以第二个不连接，`/mcp` 说明原因（`docs/tools/mcp.md`）。
- **一个输入 schema 不寻常的 MCP tool 不再让每个回合都失败。** tool 的 schema
  乘在每次请求里，而一个顶层没有 `"type": "object"`、没有 `properties`、或者
  根部带 `oneOf`/`anyOf` 的 schema 会被 provider 连同整个请求一起拒掉。现在
  提供给模型的是修好的副本：顶层是一个带它各属性的对象，备选分支折进一个对象
  并在描述里注记说明，模型 API 都不读的方言标签被去掉。任何修法都救不回来的
  tool —— 比如输入被声明为字符串的，或者有个参数名被 API 拒绝的 —— 就被略过，
  而 `/mcp` 里该服务器的页面把它列在 *Not offered to the model* 之下并给出
  理由（`docs/tools/mcp.md`）。
- **MCP tool 的图片、音频和文件不再用 base64 把它的结果填满。** 过去只有第一
  张图片被变成模型看得见的图；第二张图片、一段声音或一个附件以原始 base64
  文本的形式到达模型，把输出额度耗在噪声上。现在每一个都是一行，说明它是什么、
  多大，资源链接说明它指向哪里，而附带的文本文件就是它的文本。客户端也遵循
  最新的 MCP 规则（2026-07-28）来处理它过去读错的结果：未知种类的结果报成错误
  而不是空成功，答「再问我一次」的服务器会被再问一次，下一页由一个空字符串
  点名的 tool 列表会一直读到尾，而绕回自己的列表会停下而不是重复 tool。发
  `"error": null` 的、把请求编号当文本回显的、或者报错误但不带请求编号的
  服务器都能被理解，而不是超时（`docs/tools/mcp.md`）。
- **一个按 tool 参数路由的远程 MCP 服务器可以被调用了。** 2026-07-28 的 MCP
  修订允许服务器标记某些 tool 参数 —— 比如一个 region 或一个 tenant —— 它们
  还必须作为请求 header 发出，好让网关不读调用内容也能路由；过去对这样一个
  tool 的每次调用都以 header 不匹配被拒绝。那些值现在作为 `Mcp-Param-…`
  header 随行。标记破坏规则的 tool 被略过，并在 `/mcp` 里带理由列出；而服务器
  仍然拒绝的一次调用，会让客户端重新取一次 tool 列表并重试一次
  （`docs/tools/mcp.md`）。
- **一个远程 MCP tool 得到它的调用所允许的全部时间。** 走 HTTP 时，每个 MCP
  请求都在 60 秒后切断，不管 tool 超时说多少，所以一个花一两分钟的 tool 以
  含糊的 "request failed" 失败，而同一个 tool 在本地却能跑；调高
  `ALTER_ZERO_MCP_TOOL_TIMEOUT_MS` 什么也不改变。连接一个服务器也整体受它的
  启动超时约束，而过去每一步都拿到全部额度，一个有很多页 tool 的慢服务器可以
  花上好几分钟。
- **跟远程 MCP 服务器说话更谨慎了。** 一个在调用中途用 `ping` 查客户端的老
  服务器会得到回答，而不是把调用耗完。一个回复流中断的较新服务器会被再发一次
  请求，这是 2026-07-28 修订的要求。像 `403 Forbidden` 这样的拒绝就按其本身
  上报，而不是被当作老式服务器重试并归咎于那个（"HTTP 405 opening the event
  stream"），而两种连接都失败时消息保留两个答案。一个要求更多权限的 `403`
  把该服务器显示为需要认证。对通知的回复不再需要是空的。
- **一个 MCP 服务器没法把你的凭证重定向到别处。** 远程服务器的 header 和
  token 乘在每个请求上。客户端现在只在重定向仍停在该服务器自己的地址时才
  跟随，而一个老式 SSE 服务器若把消息 endpoint 指到另一个地址、或从 `https`
  切换到 `http`，会被拒绝。来自服务器的每条消息以 16 MB 封顶
  （`docs/tools/mcp.md`）。
- **一个本地 MCP 服务器再也不能接管终端。** 从你的配置启动的服务器与应用共享
  终端，所以一个跑了 `sudo`、要 SSH 口令或 git 凭证的服务器会把它的提示画在
  屏幕上，并读你敲的键。服务器现在像 shell 命令和 hook 早已那样，跑在自己
  的 session 里，而这样的提示会立刻失败（`docs/tools/tty-detach.md`）。
- **本地 MCP 服务器被温和而彻底地停掉。** 在一次 call 上按 Esc，或者一次 call
  超时，现在会告诉服务器别再干那件事。一个停止读输入的服务器再也冻不住一次
  call，一行不是合法 UTF-8 的日志不再结束连接，而发 `ping` 的老服务器会得到
  回答。服务器被停时，先得到自己退出的机会（这是 MCP 规范要求的），然后是一个
  客气的信号，最后才是一个强制信号；它启动的东西一并带走：一个 `npx` 服务器
  的真实进程过去会被留在那里跑。连接失败的消息现在包含服务器打出的东西，而
  一个只提供 resource 或 prompt 的服务器以没有 tool 的方式连接，而不是失败
  （`docs/tools/mcp.md`）。
- **崩溃的 MCP 服务器自己回来。** 一个死掉的本地服务器仍然被列为已连接，之后
  每次调用都失败，直到你在 `/mcp` 里重连它。现在只要一次 call 发现它没了，
  它就会被重启，并告诉 agent 再试一次；30 秒内又死掉的一个就留着让你看。
- **Esc 能停掉一个正在等轮次的 MCP call。** 发往同一个服务器的两次调用轮流
  进行，而第二个过去在等待时忽略 Esc，然后照样跑 —— 哪怕它是一次会改动东西的
  调用。
- **`/mcp` 不再因为连按两下 Enter 就禁用一个服务器。** 在 **Reconnect** 或
  **Enable** 上连按两下 Enter 会把服务器关掉，因为服务器一开始连接，第二下
  就落在了 **Disable** 上。**Clear authentication** 现在还会停掉正在用那份被
  清掉的登录的连接，而在你正在开发的服务器里改动一个 tool 的描述或输入，在
  重连之后就能到达 agent。
- **登录一个 MCP 服务器时会核对别人告诉它的话，也能找到更多登录服务器。**
  地址里带路径的登录服务器 —— 比如 Keycloak realm 和 Microsoft Entra
  tenant —— 把它们的细节公布在客户端从不看的地方，于是登录以 "could not
  discover the authorization server" 失败。客户端现在去 MCP 规范说的位置找，
  绝不看隔壁登录服务器的细节，并且只有在一份文档正是你要登录的那个服务器的、
  且点明了它是从哪个登录服务器取来的时才用它。登录服务器必须用 HTTPS（你自己
  机器上的除外）并支持安全的 PKCE 方法。一个要求更多权限的服务器会连同它已有
  的一起被要求，而它点名的权限被正确读取：过去一次解析疏忽会把它丢掉
  （`docs/tools/mcp.md`）。
- **MCP tool 的描述保住它的用法说明。** 描述过去被截在 2,048 个字符，丢掉了
  较长的那些 —— 服务器写来解释自己 tool 的那些 —— 的结尾；现在上限是 4,096，
  也就是 Claude Code 的。
- **在 `/mcp` 里关掉一个 MCP 服务器不再可能抹掉 `mcp.json`。** 文件里有一个
  笔误时，那个开关会用自己写的几行替换整个文件，连里面的每个服务器一起；现在
  它会放着这样的文件不动，本次 session 保持该服务器关闭并说明原因。一个开关也
  不再撤销同一项目里另一个 session 做的开关。文件的每一次写入 —— 来自 `/mcp`
  的，以及来自 `zeroth mcp add` 和 `remove` 的 —— 现在都是一步替换，所以磁盘
  满或崩溃不会把它留成空的；它保留权限（新文件只有你能读，因为它可能装着
  token），并穿过符号链接写进去，而不是替换掉符号链接。
- **MCP 配置里的 `${VAR}` 会从你的环境里填上。** 一个写着
  `"Authorization": "Bearer ${GITHUB_TOKEN}"` 的项目 `.mcp.json` 过去就发这串
  字面文本。现在 `${NAME}` 和 `${NAME:-default}` 会在服务器的命令、参数、
  环境、URL 或 header 用到它们的地方被填上，与 Claude Code 一致。一个点名了
  未设置又没有默认值的变量的服务器不启动，`/mcp` 点名那个变量；`/mcp` 和
  `zeroth mcp get` 仍显示那个引用，永不显示值。项目的文件没法用这种方式把你
  在用的 provider 的 API key 发给一个远程服务器；一个索要它们的服务器不启动。
  一个空的配置文件不再在每次启动时都报错，`"type": "streamable-http"` 能被
  理解，而缺少 command 的 stdio 条目会说明这一点（`docs/tools/mcp.md`）。
- **MCP 登录不再丢在一次续期竞态上。** 登录服务器可能发出一次性的续期 token，
  而一旦看到它被用两次就吊销整个登录。两个不同终端里的 session 同时续期同一个
  服务器的登录时可能都把它发出去，然后两个都需要重新走浏览器；现在第二个
  session 等第一个，并用它拿到的结果。一次续期也不再删掉它没有拒绝的登录：
  一个在续期途中被你清掉的登录保持清掉，而一个你在这期间又登进去的登录被保留。
  一个对登录本身什么也没说的答复，比如 `invalid_request`，同样不再删掉它；而
  一个只活几分钟的 token 在它生命的一半处续期，而不是每次调用都续。token 文件
  从创建那一刻起就是私有的，一次 token 存不下来的登录会说明这一点，而不是看
  起来像是成功了，**Clear authentication** 在清不掉时也会说
  （`docs/tools/mcp.md`）。
- **登录 ChatGPT 或 Anthropic Console 不再挂住，也不再接受一个走错的答复。**
  `/login` 里那两个浏览器登录有 MCP 登录有的那些毛病：一个浏览器打开了备用连接
  却不用它，可以把登录拖住 —— 连 Esc 一起；一个回来时没带本次登录校验值的
  code 仍被使用；一个旧的浏览器标签作答就以错误结束了登录；而在登录页上拒绝
  会让它白等十分钟。现在它们共用 MCP 登录的回调：不是本次登录的答复被打回去
  而登录继续等，拒绝立刻带着理由结束它，而当你的系统把 `localhost` 送到 IPv6
  地址时它也会作答（`docs/providers/chatgpt.md`、`docs/providers/claude.md`）。
- **登录一个 MCP 服务器不再挂住，也不再接受一个走错的答复。** 过去在登录页上
  拒绝，会让登录白等五分钟等一个永远不会来的答复；现在它立刻带着登录服务器的
  理由结束。一个浏览器打开了备用连接却不用它，可以把登录拖住，而在 macOS 上
  一次登录可能错过浏览器的答复；当你的系统把 `localhost` 送到 IPv6 地址时登录
  也会作答。一个不带本次登录自己的校验值的答复 —— 来自更早的一次尝试或其他
  任何地方 —— 被打回去，登录继续等正确的那一个；而一个粘错的 URL 不再结束
  登录：页面说明原因，再收一次。再按一次 **Authenticate** 不再让新登录听不到
  它粘贴的 URL 和 **Esc**，也不再以第一次的 "cancelled" 错误把它关掉，而
  **Esc** 不再显示它自己的错误。一个发出 client secret 的登录服务器会按它要求
  的方式被答复，一次被拒绝的注册会说明原因，而一次登录在同一端口上复用自己的
  client，或者重新注册，而不是被一个校验端口的登录服务器拒绝。向一个登录服务
  器注册的 client 永不被发给另一个，而登录刚开始时按 **Esc** 不再照样打开
  浏览器（`docs/tools/mcp.md`）。
- **`$ALTER_ZERO_PROJECT_DIR` 是项目根，不是你启动所在的那个文件夹。** 一个
  写成 `"${ALTER_ZERO_PROJECT_DIR}/.alter-zero/hooks/format-and-test.sh"` 的
  hook，在 app 从仓库的子目录启动时现在能找到项目的脚本。它过去指向那个子
  目录，于是脚本找不到，hook 悄无声息地什么也没做。`$CLAUDE_PROJECT_DIR` 得到
  同样的值；hook 仍在你启动所在的那个文件夹里运行
  （`docs/tools/hooks.md`）。
- **以 `2` 退出的 `PermissionRequest` hook 拒绝这次调用。** 它的 stderr 就是
  理由，与 Claude Code 和 codex 一致，也和 `/hooks` 页面说的一致；过去这次
  调用会继续走到 auto 模式的 classifier 和权限 prompt，就好像 hook 什么也没说
  （`docs/tools/hooks.md`）。
- **拒绝一次调用的 hook 保住它说过的其余东西。** 同样的那些 hook 阻断这次调用
  时，一个 `PreToolUse` hook 添加的上下文被丢掉了，它的注记（一个坏掉的兄弟
  hook 的警告、一个 `systemMessage`）也是。现在 agent 在拒绝之后读到那段上下文，
  而注记坐在红色单元格下面（`docs/tools/hooks.md`）。
- **hook 的上下文跟着一条被送到后台的命令留下。** agent 在命令启动时读到了它，
  但对话只留住了启动文本，所以下一个回合就没有它了
  （`docs/tools/hooks.md`）。
- **tool call 的暗色注记行保住每一条注记。** 一次由 auto 模式 classifier 允许、
  而 hook 也评论了的调用，过去只显示 hook 的注记，而一个在 call 之前跑的 hook
  的注记会输给在它之后跑的。现在这一行按到来的顺序列出每一条注记，用 `·` 分隔，
  在主对话里和在 agent 自己的视图里都是如此。
- **回合不再终结在一个什么也不说的回复上。** 模型有时以一个既无回复也无 tool
  call 的方式结束它的回合 —— 最常见的是紧跟在一轮 tool 之后，有时是经过一段
  用光了整个输出预算的长思考之后 —— 而回合就停在那里，屏幕上只有
  `Thought for …`，直到你敲 `continue`。现在 app 自己在同一个回合里要求模型
  继续，而一个在 provider 输出上限处被截断的回复从它停下的地方继续。如果模型
  在两次尝试之后仍然什么也不答，回合以一个说明这一点的红色通知结束，而不是
  看起来像是完成了。一个被模型的上下文窗口或被内容过滤器停下的回复，现在也以
  一个点名原因的通知结束。Ctrl+O 显示每一次继续以及模型被告知了什么
  （`docs/architecture/unfinished-replies.md`）。
- **一条保存下来的命令规则不再覆盖被引号藏起来的命令。** 对一条命令点
  "Don't ask again" 会为它的程序保存一条规则，比如 `echo *`，而之后一条命令在
  它的每一部分都被覆盖时不问就跑。把命令拆成各部分过去只考虑引号，所以像
  `echo \"; rm -rf ~; echo \"` 这样一条命令 —— 其中反斜杠让引号成了普通字符
  —— 被读作一个 `echo`，然后不问就跑了 `rm -rf ~`。一个 `#` 注释或 `$'…'`
  引号也能干同样的事。现在命令按 bash 运行它们的方式拆分。一条以关键字如
  `if`、`then`、`do` 或 `{` 开头、以 `.` 或 `builtin` 开头、或以子 shell 的
  `(` 开头的命令，现在只可能匹配为那条确切命令保存的规则，因为从前为 `then`
  保存的规则会覆盖它后面来的任何东西（`docs/state/permissions.md`）。
- **文件已经不在了的图片不再留下一个空白块。** agent 常常读它存在 `/tmp` 下的
  截图，而重启会清掉 `/tmp`。恢复这样的 session，会在 `Read image` 行和回复
  之间留下一屏空行 —— 那是一张再也画不出来的图片占的地方。恢复的 session 现在
  会检查每张图片的文件还在不在，不在的一张一行都不占。每当对话被重画时检查都
  会再跑一次 —— 一次 resize 或图片设置的改动 —— 所以一张在 session 中途被删掉
  的图片也不再占着它的地方，而一张文件又回来的图片会重新画出来
  （`docs/ui/images.md`）。
- **文件单元格的行内预览留在它的十行之内。** 一个 `read`、`write` 或 `edit`
  单元格显示文件最前面的几行，其余留在 Ctrl+O 后面，但一条从这几行里开始的
  长行过去总是显示到它结尾 —— 于是一次压缩过的样式表的 `write` 把它的一行画过
  scrollback 一百多行，下面还有一行提示在数它之后的那些行。现在预览停在它的
  第十行：这些行放得下的一条仍整个显示，需要更多空间的那条在最后一行被切断并
  标上 `…`。当被切断的那一行是文件的最后一行时，它下面那一行仍指向 Ctrl+O。
  Ctrl+O 和权限 prompt 仍整行显示每一行（`docs/ui/long-lines.md`）。

## [0.17.0] - 2026-10-27

### 新增

- **被中断的回合现在显示它的花费。** 过去在回合中途按 Esc 只留下红色的
  `Conversation interrupted` 一行，状态行的运行时间和 token 计数也随之消失。
  现在完成回合结尾的那份摘要出现在通知下面：
  `Worked for 8s · ≈24.3k tokens`。`≈` 意味着计数里有一部分是 app 自己的估算：
  provider 只在请求结束时报告它的用量，而你打断的那次请求永远不会结束。什么都
  没估算时 —— 比如模型那次请求已经结束之后你停掉一条运行中的命令 —— 数字就是
  provider 自己的，不带 `≈`。`!` 命令和被中断的 `/compact` 仍然不带摘要结束
  （`docs/input/interrupt.md`）。
- **`/usage` 显示你的订阅还剩多少。** 在这之前，你第一次听说某个套餐的额度，
  是因为一个回合被拒绝。`/usage` 打开一页，为你登录的每一份订阅各一节，正在用
  的那份排在最前：ChatGPT 的 5 小时窗口和周窗口、它的 credits 以及可以兑换的
  额度重置，GitHub Copilot 的每月额度（AI credits 或 premium requests、chat、
  code completions，各按自己的单位计，Free 席位就叫 Free），以及 ClinePass 的
  5 小时、每周和每月窗口。每个额度是一根用量占比的条，绿色，到 75% 转琥珀、
  到 90% 转红，后面跟着数字和重置时间 —— 既是倒计时也是本地时间
  （`resets in 1d 23h · Tue 2:36 PM`）。用光的额度在自己的行上以红色写着
  `limit reached`，一个暂停的 ClinePass 窗口说明该切到什么。高亮订阅自己的用量
  页链接在列表下面，`c` 复制它。Console 登录没有套餐，所以它那一节说明按 token
  计费，不去问 provider 任何东西。这一页在回合中途也能打开，说明它的数字有多
  新，开着时每分钟重读一次、按 `r` 立刻重读，刷新失败就保住上一批数字并在你
  再次打开时立刻显示，还能通过把其他订阅各折成一行来适配矮终端。API key 的
  provider 按 token 计费，不列出；一份订阅都没登录时 `/usage` 用一个 toast
  说明这一点（`docs/providers/usage.md`）。

### 变更

- **session 从 auto 模式起步。** 权限模式过去从 `manual` 起步，那会为每一次
  文件改动和每一条命令停住回合。新的 session 现在从 `auto` 起步：文件改动不问
  就执行，你的 allowlist 没覆盖的命令交给 auto 模式的 classifier，它要么运行、
  要么阻断、要么判不下来时问你。它复核的每条命令都是一次发往你 provider 的
  请求。页脚右边缘写着 `auto`，Shift+Tab 仍循环切换模式；从 `auto` 起，按一下
  是 `master`，按两下是 `manual`。你选过 `edit`、`auto` 或 `master` 的项目
  保持不变。但旧版本从不保存 `manual`，所以一个停在 `manual` 的项目现在从
  `auto` 起步；用 Shift+Tab 切回去，它就留下了。要在任何地方都继续发问，
  用 `ALTER_ZERO_PERMISSION_MODE=manual` 启动。它接受任何模式名，为那次启动
  设定模式，且永不保存（`docs/state/permissions.md`）。
- **长命令的头部停在两行。** 一个 `Bash(…)` 头部最多显示命令的 160 个字符，
  而在窄于大约九十列的终端里，它们会溢到第三行，那一行只装着截断的结尾，往往
  是半个词（`scripts/parallel_se…)`）。现在头部在第二行以 `…)` 结束。更宽的
  终端显示的和从前一样，Ctrl+O 仍显示整条命令。每个 tool 的头部都遵循同一条
  规则，所以一个长文件路径或 MCP 调用也以同样的方式被截断
  （`docs/tools/tools.md` *Long headers*）。

### 修复

- **被 Esc 切断的思考保住它的时长。** 模型正在思考时按 Esc，无论它已经想了
  多久，过去都会把这段思考结算成 `Thought for 0s`。现在它说出模型实际想了多久。
- **回复中途断掉的连接不再结束回合。** 回复正在流式传输时网络掉了 —— Wi-Fi
  切换、VPN 重连、笔记本从睡眠中醒来 —— 过去回合会死在
  `request failed: request or response body error: error reading a body from
  connection: Connection timed out (os error 110)`，或者一次 reset 上，不管
  任务进行到了哪里。现在回复从头再来：已经流出来的部分从屏幕上收回，一个 toast
  说明连接断了，状态行显示 `retrying 1/3`，然后请求重新发出，如果网络还没回来
  就先等网络。之后回复一次、完整地流出来。子代理以同样的方式重来，并且只交回
  完成了的那份回复。一直掉的回复在 `/settings` 的 **Error retry** 行允许的
  次数之后仍然会停下（`docs/providers/offline.md`）。

## [0.16.0] - 2026-10-25

### 新增

- **`/context` 显示什么在占用上下文窗口。** 页脚的量表说明窗口有多满；
  `/context` 说明是什么在填它。这一页把窗口拆成 system prompt、内置 tool 的
  schema、每个 MCP 服务器的 tool、项目的 `AGENTS.md`、skill 与 agent 类型的
  清单、任何 secret 的名字，以及对话本身，每一项带着它的 token 和占比，下面是
  一根整窗的条，配色相同，每一类按它的占比那么长、随它一起长，末端是空闲空间
  和自动压缩的预留。↑/↓ 选一行，旁边的细节说明它由什么构成：最贵的那个 tool、
  那个文件、那个 skill，或者对话在你输入的内容、你的 shell 命令、app 自己的
  通知、回复、tool call 和 tool 结果之间怎么分。下面是 provider 报告的东西：
  上一次请求进出的 token、其中多少读自 prompt cache，以及自启动以来的总计。
  它在回合中途也能打开，回合跑着的时候继续数，通过把列表收窄到高亮周围的几行
  来适配任何高度的终端，而在子代理的 session 视图里显示那个 agent 自己的窗口。
  Ctrl+D 显示数字背后的原始上下文，Esc 关掉它
  （`docs/architecture/context-usage.md`）。
- **`/login` 能连一个本地模型，并在保存前先测试它。** `/login` 现在提供三种
  入口：订阅、API key，或者 **Use a local model**。第三种列出你自己跑的模型
  服务器：Ollama、LM Studio，以及你自己 `providers.toml` 里任何不要 key 的
  服务器。选一个并输入它的地址。`localhost:1234` 就够了，空的字段用已经生效的
  地址。Enter 按 `/model` 将要用的方式列出服务器的模型来测试连接，测试通过之前
  什么都不保存。如果服务器没在跑、host 打错了、端口回答的是别的东西、或者证书
  不被信任，这一页会说明这一点和该检查什么，你可以改地址再试。设成要求 token
  的服务器在同一页上索要它。测试通过就把地址和任何 token 保存到
  `~/.alter-zero/.env`，并说明找到了多少个模型。如果你的 shell 导出了那个变量
  且值不同，导出的那个优先，所以这一页不会确认，而是告诉你该 unset 哪个变量
  （`docs/providers/local-models.md`）。
- **LM Studio 是一个内置 provider。** 它经由 LM Studio 的 OpenAI 兼容服务器
  连接。它的模型从 LM Studio 自己的目录列出，所以 `/model` 显示每个模型的名字、
  它以多长的上下文加载、以及它是否接受图片，并略过 embedding 模型。早于 0.4
  的 LM Studio 没有这样的目录，就通过 `/v1/models` 列出。还没加载的模型显示
  不出上下文量表，而不是显示它的上限，因为 LM Studio 用一个它不上报的设置去
  加载它。比模型加载时的上下文更长的 prompt 会以如何调高它来结束回合
  （`lms load <model> --context-length 32768`），而一个开始要求 token 的服务器
  会把你送去 `/login`。设成要求认证的服务器从 `LM_API_TOKEN` 取它的 token
  （`docs/providers/local-models.md`）。
- **运行中的子代理显示它在说什么。** 它在 `● Agent(…)` 单元格下面那一行过去
  只点名它的调用（`⎿ Bash: …`、`⎿ Write: …`）。agent 写东西的时候，这一行现在
  显示它的话，`⎿ Agent: Checking the
  recent events next.`，取自它文本里最新的一段。这一行一直显示它们，直到 agent
  开始下一次调用（`docs/agents/agent-progress.md`）。
- **`agentoutput` 报告 agent 一路上说了什么。** agent 在继续干活之前写下的
  文字，作为一条 `Agent: …` 条目列在它在各个 call 之间本来的位置上。lead 现在
  能读到 agent 发现了什么、接下来打算做什么，而不只是它跑了哪些命令。每条条目
  截到 2,000 个字符之前都是整个保留的，`agentkill` 的报告里也包含它们。一个
  等待中的 `agentoutput` 会把这些条目随到达随流进它的单元格，连同 agent 读到的
  消息一起。

### 变更

- **本地的 Ollama 从 Use an API key 挪到了 Use a local model。** 它从来不要
  key。它的页面现在保存之前先测试地址，而空的 Enter 会重新测试已保存的地址，
  而不是把它重置为默认值。Ollama Cloud 确实要 key，它留在 Use an API key 下。
- **什么都没设置时 `/model` 现在说
  `No provider set up yet — run /login to add one`**，而不是索要一个本地模型
  并不使用的 API key。`/` palette 里 `/login` 的条目和离线 demo 的收尾行现在
  都点名全部三种入口。
- **你自己 `providers.toml` 里的一个 `[providers.lmstudio]` 块**（0.15.0 的
  文档曾这样建议）现在调整内置的 LM Studio，而不是再加一个。在
  `/login` → Use a local model 下连它一次，或者删掉那个块。
- **停掉的本地服务器会立刻被报出来，而不是等到超时。** 当这台机器上一个不要
  key 的服务器拒绝连接时（LM Studio，或者你自己 `providers.toml` 里的
  llama.cpp 或 vLLM 块），回合现在几秒内就以
  `Nothing is answering at 127.0.0.1:1234 — is LM Studio's
  server running?` 结束。它过去显示 `Waiting for internet…`，那是错的。停掉的
  Ollama 早已是这个行为。要 key 的 provider 保留等待，即使在 localhost 上也是
  如此（`docs/providers/offline.md`）。
- **不要 key 的 provider 的请求不再携带 prompt cache 标记。** 你自己跑的
  服务器上一个叫 `qwen/…` 或 `Qwen/…` 的模型（LM Studio 的
  `qwen/qwen3-8b`，一个 vLLM 仓库 id）曾被发以 OpenRouter 的
  `cache_control` 断点，而那只有对缓存计费的托管服务才用
  （`docs/providers/prompt-caching.md`）。
- **读取、写入和编辑单元格整行显示长行。** 一条换行超过三行的行过去在第三行
  以一个 `…` 停住，于是一段 Markdown、一个长字符串或一行 HTML 的结尾在单元格
  里永远看不到。现在每一行都换到它的完整长度。单元格在
  `… +N lines (ctrl+o to expand)` 之前仍显示大约十行：有地方就开新行，一旦开了
  就把那一行写完，所以这些行接近末尾处的一条长行会让单元格因为那一行剩下的部分
  而变高。正在写入的文件在流式传输时显示最新的十行，所以一条长行末尾到达的
  文本一到就在屏幕上（`docs/ui/long-lines.md`）。

### 修复

- **Kali 容器在一个 Kali 镜像挂掉时也能构建。** Kali 的重定向器
  `http.kali.org` 把每个文件交给它挑的一个镜像，而当它挑中一个已经不答应的
  镜像时，`docker/build.sh` 以 `Unable to fetch some archives` 失败，Docker 和
  Podman 都一样。现在镜像里的 apt 仍先问重定向器，而它挑中的那个交付不了的文件
  改从 `kali.download` 取，那是 Kali 的 Cloudflare 镜像。这对构建和容器里每一次
  `apt install` 都成立（`docs/ops/docker.md`）。
- **`agentoutput` 把 agent 读到的一条消息列在它读到的位置上。** 一个
  `— message received —` 标记可能出现在 agent 读到该消息之前就已经做出的一个
  call 前面，条件是 agent 跑在了记录它的那个线程前面 —— 一次很快的调用，或一台
  忙碌的机器。现在标记按 agent 干活儿的顺序放置
  （`docs/agents/agent-progress.md`）。
- **服务器在回复里报的错误现在以那个错误结束回合。** 一帧只装了消息的流
  （`data: "…"`），或者一个用字符串点名自己错误的 `{"error": "…"}`，过去被
  跳过，于是回合以既无回复也无错误结束。LM Studio 就是用这种方式报告比它加载的
  上下文更长的 prompt（`docs/providers/local-models.md`）。
- **没有模型列表的模型列表是错误，而不是一个空的 provider。** 一个以 200 和一
  个错误对象答复 `/models` 的服务器，过去在 `/model` 里显得是没有模型、也没有
  原因；现在它被标为不可用，并带上服务器自己的话
  （`docs/providers/user-providers.md`）。
- **你的消息重新启动子代理时，它的答复不再丢掉。** 如果你在一个子代理写最终
  回复时给它发消息，它会在那条回复一写完就拾起你的消息 —— 但当主 agent 正用
  `agentoutput` 等它时，那次等待会继续去报告新一轮的答复，并把第一个答复扔掉，
  也就是对主 agent 所给任务的答复。现在报告在自己的答复之后带上那个更早的答复，
  并标明它先来（`docs/agents/agent-tools.md`）。
- **你让一个子代理回来时，主 agent 会知道。** 给一个失败了、结束了或被停掉的
  子代理发消息会让它重新干活，但主 agent 只有在那次运行结束之后才知道 —— 在那
  之前，它继续相信这个 agent 已经失败，可以随意重做它的工作或另起一个替代者。
  现在你的消息一启动那次运行它就被告知：这个 agent 应你请求又在跑了，并引用
  你的消息，而它的答复在结束时仍作为一条通知到达。对话把它显示为
  `Agent "…" was resumed by user`。如果主 agent 在读到这条注记之前就得知那次
  运行是怎么结束的 —— 通过 `agentoutput`，或者通过一次停掉该 agent 或发现它
  已经完成的 `agentkill` —— 这条注记就被丢掉，所以它永远不会在 agent 已经停下
  之后被告知还在跑。一条正好在某个 agent 那次运行结束时到达的消息 —— 一次 Tab
  追问，或者在主 agent 等它时发出的一条 —— 也可能在 app 看到上一次运行结束之前
  就启动下一次运行，把新运行做的一切都藏起来、也让主 agent 拿不到它的答复；
  现在 app 先把那个结束收进来（`docs/agents/agent-tools.md`）。
- **代码生成占用的内存更少。** 预热好的 Ctrl+O transcript 把它的文本和样式打包，
  只展开可见的行，同时保住完整输出、语法颜色和快速打开。文件改写用一张 2 MB 的
  diff 表取代从前大约 8 MB 的那张，匹配与平局选择完全一致。请求快照共享不可变
  的 tool 参数，而不是复制生成的源码，而刷新 live transcript 会释放被丢弃的样式
  条目。一个离线的 CLI 压力探针和内存回归测试覆盖这些路径
  （`docs/architecture/memory.md`）。

- **npm 启动器的 fallback 在启动二进制之前接住信号。** 一个作为快速子进程启动时
  收到的信号可能在其处理器装好之前就把启动器杀掉，留下子进程还在跑。现在处理器
  在 spawn 之前就装好了，`SIGQUIT` 也是。

- **Ctrl+O 在一段思考之后只留一行空白。** 当一个模型的推理以空行结尾时，
  transcript 在展开的思考和下一个单元格之间显示三行空白，而一段以空行开头的
  推理会丢掉它的 `⎿` 角。现在思考开头和结尾的空行不再画出；它各段之间的空行
  保留。

## [0.15.0] - 2026-10-20

### 新增

- **`flaredantic` 加入内置 skill。** 用外部的 `flare` CLI 分享 localhost 或测试
  webhook：默认 Cloudflare HTTP，也可特意选 Serveo TCP，或 Microsoft（带匿名
  访问与既有端口检查）。可编辑的 `SKILL.md` 和参考指南公布对外暴露的检查、被
  跟踪的 session、本地/公网验证与清理。不添加任何原生 tool 或依赖，加载这个
  skill 不运行任何命令或隧道。已有的修改保留，项目的 skill 优先，而
  `ALTER_ZERO_SKILLS_DIR` 覆盖的目录拿不到内置 skill。

- **文件在模型写它的时候流进它的 `Write` 单元格。** 单元格过去只在模型把整个
  call 生成完之后才出现，所以一个大文件意味着漫长的等待，其间只有 token 计数在
  动。现在模型一开始这个 call，`● Write(path)` 就出现了，文件逐行出现在里面，
  带编号、带语法高亮，与完成的单元格一样。内容到达时你看到最新的十行，最新的
  一行在最底下，上面是一个 `Writing N lines…` 计数。全部到齐之后单元格显示最前
  十行，带 `… +N lines (ctrl+o to expand)` 提示 —— 就是完成后的单元格保留的那
  个视图，所以文件写完时什么也不会跳。Ctrl+O 在流式传输时显示整个文件。这在每个
  provider 上都能用，在子代理的 session 视图里和离线 demo 里也是
  （`docs/ui/write-streaming.md`）。

- **在 `~/.alter-zero/providers.toml` 里加自己的 provider。** 任何 OpenAI 兼容
  的 API 都能用几行加进来，它会和内置 provider 一起出现在 `/login` 与 `/model`
  里（`docs/providers/user-providers.md`）。

### 变更

- **内联页面不再用空行给它们的框垫边。** 每个取代输入框打开的页面过去都会在
  顶线下面留一行空行、在底线上面再留一行。这包括 `/model`、`/login`、
  `/settings`、`/theme`、`/mascot`、`/spinner`、`/skills`、`/hooks`、`/mcp`、
  `/trust`、`/secrets`、`/export`、`/donate`、↓ shell 管理器、权限 prompt 和
  `AskUserQuestion` 模态框。那两行没了：搜索行或标题现在紧贴顶线之下，提示紧贴
  底线之上，于是每个页面把两行还给上面的对话。页面各节之间的空行保留
  （`docs/ui/view-flow.md`）。
- **`/theme` 像 codex 那样把预览放在列表旁边。** 在 71 列或更宽的终端上，示例
  对话现在坐在主题列表右边而不是下面，于是整个 picker 塞进十八行；更窄的终端
  仍把它叠在列表下面。列表读起来像 `/mcp` 的：`❯` 标出高亮的主题，而一行暗色的
  `↓ 1 more below` 或 `↑ 1 more above` 说明什么被滚出了视野，取代 `(1/11)`
  计数。预览以主题名粗体开场，下面是简短描述，而提示现在以
  `↑/↓ to navigate` 开头。在短到放不下整个 picker 的终端上，预览被略过，列表
  一次显示三个主题，于是搜索行、主题名和提示留在屏幕上，而不是滚到预览上方去
  （`docs/ui/theme.md`）。
- **`/mascot` 和 `/spinner` 按 `/theme` 的方式摆放它们的预览。** `/mascot` 里
  的横幅预览和 `/spinner` 里的实时状态行现在在标准终端上坐在列表右边
  （`/mascot` 是 56 列或更宽，`/spinner` 是 72 列），在更窄的终端上叠在它下面。
  两个列表都读起来像 `/theme` 的：`❯` 标出高亮的条目，`(n/total)` 计数没了，
  预览以条目名粗体开场、下面是它的描述，而提示以 `↑/↓ to navigate` 开头。
  并排放的时候，`/mascot` 塞进十二行，`/spinner` 塞进十五行。三条 spinner 描述
  被缩短，好让每一条在 80 列的终端上都能整个读在列表旁边
  （`docs/ui/mascot.md`、`docs/ui/spinner.md`）。
- **`/skills` 以标题和计数开场，并标出哪些是开着的。** 菜单现在以一个 `Skills`
  标题开场，下面是像 `6 skills · 5 enabled` 的一行，就像 `/hooks` 和 `/mcp`
  开场那样，而每个 skill 读作 `✔ enabled` 或 `◯ disabled`，所以开着的那些在
  列表里一眼就看得出来。计数是已安装 skill 的数量，不是搜索结果的数量。说明
  skill 在本次 session 里关着的那句注记现在坐在计数下面，而不是在搜索行和列表
  之间。在短到放不下整个菜单的终端上，先是一条长描述被裁成一行，然后被略过
  （在更短的终端上标题也一样），同时列表一次显示三个 skill，于是搜索行、你
  光标所在的那个 skill 和提示留在屏幕上，而不是滚走；当你越过一条长描述时，
  菜单也不再在两种版式之间跳（`docs/tools/skills.md`）。
- **`/settings`、`/mascot`、`/spinner`、`/model` 和 `/login` 在矮终端上让你正在
  选的东西保持可见。** 每一个都要十五到二十行；行数更少时，你高亮的那一行和
  搜索行过去会滚出顶部。现在选择用不着的东西先让位 —— 一条长的 `/settings`
  描述被裁成一行，然后描述、计数、`/mascot` 与 `/spinner` 的预览以及 `/login`
  关于 key 存放在哪里的注记都被略过 —— 而列表一次显示三行，用
  `↑ N more above` 和 `↓ N more below` 行点名剩下的，于是搜索行、高亮的行和
  提示永远留在屏幕上。`/model` 把高亮模型的完整名字留在列表下面。`/settings`
  的值和 `/spinner` 的符号在滚动时保持对齐在一列里（`docs/ui/view-flow.md`）。
- **`/donate` 和 `/secrets` 在矮终端上让高亮的条目留在屏幕上。** 在 `/donate`
  上，你不在的地址折成它们一行的标签，而高亮的那一个保住它的框和它的网络，然后
  标题让位，在更短的终端上高亮的地址单独显示在它带编号的标签下面；那句告诫和
  提示留下。一个长的 `/secrets` 列表在高亮的那一条周围一次显示三个 secret，
  用行点名上面和下面还有多少，然后丢掉它的标题，在最小的终端上那些行也丢掉
  （`docs/ui/donate.md`、`docs/state/secrets.md`）。
- **agent 的多选题在矮终端上把问题留在屏幕上。** 问题过去会滚出顶部，往往连带
  最前面的几个选项。现在选项的描述先被缩短，然后被略过，选项在高亮的那一个周围
  一次显示三个，而问题标签那一行最后让位，于是问题、你光标所在的选项和按键
  保持可见。一个带代码预览的问题先丢掉注记行、缩短预览
  （`docs/tools/ask.md`）。
- **各个 picker 在很短的终端上保住它们的搜索行。** 在大约九行以下，`/theme`、
  `/skills`、`/settings`、`/mascot`、`/spinner`、`/model` 和 `/login` 过去把
  搜索行滚出顶部，连带你已经打进去的字和光标，然后是高亮的那一行。现在它们
  改让出 `↑`/`↓ N more` 行、边框，然后一次一行地让出列表行，于是搜索行留在
  顶部，高亮的行和按键在它下面，一直到最小的终端。一个什么都匹配不到的搜索也
  以同样的方式把你打的字留在屏幕上（`docs/ui/view-flow.md`）。
- **回复像 codex 那样渲染它的 markdown，没有多余的标记。** 一个被强调包起来的
  标题，比如 `## **Top 10 PHP
  Frameworks**`，现在读作 `## Top 10 PHP Frameworks`。项目符号显示 `•`，而不是
  模型的 `-`、`*` 或 `+`，任务项显示 `☐`/`☑`，而一个有序列表从它的第一个数字
  开始数，所以 `1.`、`1.`、`1.` 读作 1、2、3。嵌套列表每层缩进四列，而一个在
  `1.` 下面只缩进两个空格、或用了一个 tab 的子列表仍然嵌套。条目下的文本挂在
  它下面，包括一行没缩进的懒惰行、第二段或一个代码块。引用是绿色的，和 codex
  一样，并且能装一个列表（`> - item` 显示一个项目符号）或一个标题。块与块之间
  隔一行空行，即使模型没留；一串空行塌成一行；一个松散的列表收紧；一个换到
  第二行的条目在下一个之前加一行空行，而紧跟在条目文本下面的引用、标题或表格
  则贴着它。一行文本后面紧跟的 `---` 画出一条横线，而不是显示那些短横
  （`docs/ui/markdown.md`）。
- **live 思考块显示三行，不是五行。** 模型思考时，输入框上方的 `● Thinking…`
  块现在只保留它推理的最新三行，于是一段长思考少占两行屏幕。行按它们在屏幕上
  的样子计数，所以一条换成两行的线用掉其中两行。整段思考仍在 Ctrl+O 里，流式
  传输时和沉降之后都是（`docs/ui/thinking-stream.md`）。
- **失败命令的单元格不再显示它的 exit code。** 一个失败的 `Bash(…)` 调用过去在
  它的输出开头放一行 `Error: Exit code 1`（或者 `Error: killed by signal`），
  于是 `false` 在 `(no output)` 上面显示了那一行。现在单元格只显示命令打出的
  东西，和它对成功的命令早已做的一样，而表明它失败的是那个红色的圆点。exit
  code 仍随输出发给 agent，所以它知道命令失败了、失败在哪个码上，Ctrl+D 也仍
  显示它（`docs/ui/tool-streaming.md`）。

### 修复

- **Ctrl+V 在 macOS 上粘贴截图。** 复制到剪贴板（⌃⇧⌘4）的截图以 16 位浮点 TIFF
  落到粘贴板上，而剪贴板背后那个 TIFF 解码器拒绝这种形状，于是对一个装着一张
  完全没问题的图片的剪贴板，粘贴报 `no image on the clipboard`。现在读取回退到
  AppKit 自己的强转（`the clipboard as «class PNGf»`），它把粘贴板上任何图片都
  变成 PNG，不管复制它的 app 选了什么风味。Cmd+V 仍不能用，也没法用 —— 那个
  组合属于终端自己的 Paste 菜单 —— 所以绑定是 Ctrl+V，而一个确实会转发 Cmd+V
  的终端现在会粘贴，而不是打出一个走错的 `v`（`docs/input/image-paste.md`）。
- **粘进来的图片不再把它的元数据带给模型。** AppKit 的强转连同粘贴板上图片的
  标签一起复制：TIFF 的 ImageDescription、`Artist` 和 `Software` 落进 PNG 的
  `eXIf` 块，又在 XMP 包里再来一遍，而一张照片的 GPS 和相机标签也以同样的方式
  随行。粘进来的图片会随之后每一个回合重发，所以那会在此后对话的每个回合到达
  provider。现在粘贴只保留画出图片的那些块 —— 它的像素逐字节复制，从不解码
  —— 其余的都丢掉：`eXIf`、`iTXt`、`tEXt`、`zTXt`、ICC 配置文件，以及任何它
  不认识的块。读取你点名的文件的 tool 仍按原样发送它
  （`docs/input/image-paste.md`）。
- **macOS 上的粘贴说明它为什么跑不了，而不是含糊地失败。** 一个从未进入窗口
  服务器的 session —— 一个守护进程、一次 `ssh` 登录 —— 没有粘贴板可读，而
  macOS 把终端读它挡在系统设置 → 隐私与安全性 → 自动化 之后。Ctrl+V 现在说明
  是这两种里的哪一种，并点名该打开哪个面板，而不是报一个打不开的剪贴板。两种
  拒绝都留着绕过它的办法：把图片存下来，让 Zeroth 读它的路径
  （`docs/input/image-paste.md`）。
- **什么都没找到的粘贴说明原因。** `no image on the clipboard` 现在点名两种看
  起来像图片、其实不是的形状 —— 一次*文件*复制，和网页里的一段选中 —— 以及
  绕过它的办法：把图片存下来，让 Zeroth 读它的路径
  （`docs/input/image-paste.md`）。
- **一个 skill 都没装时 `/skills` 不再说 skill 是关着的。** 没找到 skill 时，
  这一页让你去 `/settings` 打开它们，而那一行在有一个 skill 之前根本打不开。
  现在它只显示去哪里加一个（`docs/tools/skills.md`）。
- **粗斜体和转义的标记不再把 `*` 漏进回复。** `***both***` 渲染成粗斜体，
  而不是粗体的 `*both*`；结尾的 `***` 把它结束的粗体和斜体一起关上；而
  `\*literal\*` 显示 `*literal*`。强调遵循 CommonMark 的规则，但对 CJK 文本
  有一个例外，所以 `**标题：**内容` 仍然是粗体。`<https://…>` 链接去掉它们的
  尖括号，`&amp;` 这样的 HTML 实体被解码。一个在编号条目下缩进四格的段落不再
  被画成代码，嵌套条目下的代码围栏不再显示它的反引号，而一个缩进的围栏把它的
  代码留在围栏所在的位置，而不是往右挪。
- **一行很长的未闭合 `[` 或 `![` 不再卡住一条回复。** 链接方括号现在每行配对
  一次，而不是每个开括号配一次，于是一行 25 KB 的这种东西在毫秒级渲染完，而
  不是要一秒多。
- **某些 Svelte 和 Scala 代码不再冻住 app。** syntect 的解析器可能在一行上永远
  循环 —— 它的语法一直匹配却不往前走（一个 Svelte `{…}` 表达式里跨行的块注释
  就够了）—— 而高亮跑在 UI 循环上，于是 TUI 占住一个核、不再读键，在恢复一个
  显示这种文件的 session 时，还没等第一下按键就发生了。syntect 带着这个修复
  被 vendor 进来，直到某个发布带上它（`vendor/README.md`）。

## [0.14.0] - 2026-10-14

### 新增

- **一个内置的 `web-search` skill 搜索网页。** 提到 `$web-search`，或者让
  agent 去搜，它会跑这个 skill 自己的某个脚本：通过它们免费的托管搜索服务器用
  Exa、Parallel 或 Tavily，或者通过 Jina Reader 用 DuckDuckGo、Yahoo、
  Wikipedia、GitHub、arXiv 或 Hacker News。agent 对大多数搜索从 Exa 起步，要
  一份快速链接清单或要那几个站点之一时用 Jina，某个免费额度用完之后就换到下一
  个 provider，还能读它找到的一个页面。不需要 key；一个既有的 `EXA_API_KEY`、
  `PARALLEL_API_KEY`、`TAVILY_API_KEY` 或 `JINA_API_KEY` 会抬高那个 provider
  的额度，并且只发给它。脚本只需要 Python 3，不用安装任何东西，并在第一次启动
  时装在 skill 旁边，可编辑，也能像其他内置 skill 一样通过 `/skills` 关掉
  （`docs/tools/skills.md`）。
- **Cline 作为一个 provider。** 两种入口：一个来自
  [app.cline.bot/dashboard/account](https://app.cline.bot/dashboard/account) 的
  API key（`/login` → *Use an API key* → *Cline*），用于按次计费的模型；或者
  `/login` → *Use a subscription* → *ClinePass*，用于 Cline 每月 $9.99 的套餐，
  它用 Cline 扩展和 CLI 用的同一套设备码登录。ClinePass 列出 Cline 为该套餐
  发布的 `cline-pass/…` 模型，所以它新增的模型无需更新就出现。每个模型的上下文
  窗口、图片输入和思考档位都来自 Cline 的 API，于是页脚能对上下文打表、自动
  压缩知道何时该跑、Ctrl+T 能循环那些档位
  （`docs/providers/cline.md`）。

### 修复

- **spinner 不再让一条运行中命令的单元格闪烁。** 当一个 `Bash(…)` 或
  `BashWait(…)` 调用在等一条 `npm` 命令，或等别的什么在画 npm 的 `⠋⠙⠹…`
  spinner 时，它的 live 单元格过去不停在 spinner 和 `⎿ Running…` 之间跳，每次
  都把输入框上下挪一行。单元格只要回到模型上一次看到的那一帧就丢掉 spinner，
  还可能抓到半画的一帧 —— 在 spinner 擦掉它那一行之后、画下一个字形之前。现在
  单元格保留程序还在画的每一行，只在两帧之间的停顿处读输出
  （`docs/tools/interactive-shell.md`）。
- **你打的 secret 值以占位符的形式到达 agent。** 存了 `ROOT_PASSWORD` 时，一句
  "the password is password123" 过去会把密码本身发给模型；现在它说 "the
  password is `<secret:ROOT_PASSWORD>`"，屏幕上那条消息、transcript、保存下的
  session 和 ↑ 回溯也是如此，同时一个 toast 说明这个值对 agent 隐藏了。打进
  一条 `!` 命令里的值同理（那条命令仍用真值运行），发给子代理的消息、一条排队的
  追问、对 agent 提问的回答、对一次被拒绝的 tool call 的反馈，以及命令行上给出
  的 prompt 也都一样。你自己打进去的占位符按写的样子发出去
  （`docs/state/secrets.md`）。

## [0.13.0] - 2026-10-10

### 新增

- **herdr 显示 zeroth 在做什么。** 跑在一个 [herdr](https://herdr.dev) 窗格里
  时，app 现在向 herdr 报告自己的状态。侧边栏在一个回合或一个子代理运行期间把
  它显示为工作中，一个权限 prompt 或一个问题在等你时瞬间显示为阻塞，结束时显示
  为完成，而 herdr 的 "needs attention" 和 "finished" 通知在那些时刻触发。一个
  因 API 错误失败的回合显示为需要注意，带着那个错误，直到你重试或清除它为止，
  而不是显示为完成。没有什么要安装或配置。herdr 还得知恢复这段对话的命令
  （`zeroth --resume {id}`，或者 `/clear` 之后一个全新的 `zeroth`），所以重启
  的 herdr 能把 session 带回来 —— 而且只在 `zeroth` 在你的 `PATH` 上时。一个
  后台 worker 用很短的超时发这些报告，丢了就重试，并且永不拖慢 app。agent 启动
  的任何东西都看不到 `HERDR_PANE_ID`（它的命令、你的 `!` 命令、hook、MCP
  服务器），所以里面另跑一个 agent 接管不了这个窗格，而 herdr 自己的 CLI 在那里
  照常可用。`ALTER_ZERO_HERDR=0` 关掉它（`docs/architecture/herdr.md`）。
- **一个内置的 `yt-dlp` skill 指导媒体下载，而不打包下载器。** 第一次启动时它
  可编辑的 `SKILL.md` 被播到其他内置 skill 旁边；agent 可以在需要时加载它来处理
  视频、播放列表、字幕和音频。它会检查已安装的 CLI 工具和可用的格式，除非要求
  了播放列表否则默认只一个视频，使用浏览器 cookie 之前先问，报告成功之前先验证
  输出文件（`docs/tools/skills.md`）。
- **用 npm 安装。** `npm install -g @linuztx/alter-zero` 装的是和一行安装脚本
  同一个预编译二进制，支持 Linux（x86_64、arm64）和 macOS（Intel、Apple
  silicon）。npm 只下载为你的机器构建的那个包，安装时不跑任何东西，所以
  `--ignore-scripts`、pnpm 和 Bun 也能用；在 Node.js 22.15 或更新版本上，二进制
  启动时接管 `zeroth` 命令的进程，于是不会有一个 Node.js 在旁边留着。在这样的
  安装上，更新通知点名 `npm install -g @linuztx/alter-zero@latest`（或者 pnpm、
  Bun、Yarn 自己的命令），而 `zeroth update` 指向那里，而不是替换 npm 管理的
  文件（`docs/ops/npm.md`）。
- **hook 得到 Claude Code 的三个事件：`PostToolUseFailure`、`StopFailure` 和
  `Notification`。** 一个运行了却失败的 tool call 现在触发
  `PostToolUseFailure`，带着错误，而 hook 可以像 `PostToolUse` 对成功所做的那样
  给模型添加上下文或反馈。一个以错误结束的回合触发 `StopFailure`，按 Claude Code
  的错误种类键入（`rate_limit`、`authentication_failed`、`invalid_request`、
  `server_error`、……），所以 hook 可以按种类匹配。一个到达屏幕的权限 prompt
  用 `permission_prompt` 触发 `Notification`，时点正是你确实被问到的那一刻：
  一个排在别的后面等待的 prompt 在它打开时触发，一个在打开之前就已结算的永不
  触发。它从不拖住 prompt，也从不乱序到达：通知按它们的 prompt 出现的顺序一个
  一个跑，而该 call 的下一个 hook 等它的通知。为 Claude Code 写的、用到这些
  事件的 hook 脚本现在在这里原样可用（`docs/tools/hooks.md`）。

### 变更

- **hook 的 `session_id` 是你能恢复的那段对话。** 它过去是正在运行的进程的 id，
  `/clear` 和 `/resume` 都不改它，`--resume` 也接不了它。现在它是对话自己的
  id，也就是 `zeroth --resume` 接的那个，于是把 id 记下来的 hook 能把对话带回
  来。`/clear` 把它往前推，`/resume` 采用被恢复对话的 id，而 `transcript_path`
  跟着变。载荷还带着 Claude Code 的 `scratchpad_dir`，hook 就靠它找到 session 的
  scratchpad 和任务输出（`docs/tools/hooks.md`）。
- **`SessionStart` hook 在 session 打开时就在后台跑了。** 它们过去等你第一条
  消息。与 Claude Code 一样，现在它们在启动时、`/clear` 之后和 `/resume` 之后
  就开始：你可以立刻打字，而第一个回合等它们，好让它们的上下文仍能到达模型。
  对话结束时还在跑的 hook 被停掉，它们的输出被丢弃。`SessionEnd` 现在在
  `/resume` 切换对话时也触发（原因 `resume`），而在退出时它在运行中的回合和
  agent 都停下之后才触发，所以它们的 hook 没有一个会在它之后跑
  （`docs/tools/hooks.md`）。

### 修复

- **一个留下后台进程运行的 hook 不再卡住 agent。** 那个进程继承了 hook 的输出
  管道，而 app 等它们关闭，于是只要那个进程活着，就拖住 tool call、回合或
  `/clear`。现在 hook 一退出，它的输出最多再读一秒
  （`docs/tools/hooks.md`）。
- **停掉一个子代理会把它的权限 prompt 从屏幕上拿掉。** 被停掉的 agent 正在等的
  那个 prompt 过去仍留在那里，却没有人来对它作答了。现在它的 prompt 被撤回，
  而下一个等待着的（如果有）在原位打开（`docs/state/permissions.md`）。
- **`SessionEnd` 不再为一个从未开始的对话触发。** 离线 demo 不跑任何 hook，所以
  从不触发 `SessionStart`，却在 `/clear` 和退出时触发 `SessionEnd`，于是记录
  session 的 hook 看到一个只有结束没有开始的东西。现在一个对话的结束只在它的
  开始发生过时才触发（`docs/tools/hooks.md`）。
- **`UserPromptSubmit` 不再在一次后台追问之后跳过你的 prompt。** 一个在 hook
  关着时、或在离线 demo 上启动的追问回合，留下了那个告诉 hook 跳过 app 自己写的
  prompt 的标记，于是 hook 跳过了你下一个真正的 prompt。现在你自己的回合会清掉
  那个标记（`docs/tools/hooks.md`）。
- **一个对话第一次 `UserPromptSubmit` 点名它的 transcript。** 它的载荷过去把
  `transcript_path` 报成 null，因为对话的文件在 hook 跑完之后才创建。现在文件在
  回合开始之前就存在（`docs/tools/hooks.md`）。
- **herdr 窗格里的图片被画成真图片。** herdr 的窗格自称 `xterm-256color`，对
  图形什么也不说，于是那里每张图片都回退成粗糙的半块字符，尽管 herdr 的终端
  能画 kitty 图形。当环境没有点名任何图形协议时，Zeroth 现在在启动时问终端，
  就在已经用来找光标的那次读取里，而终端说可以就用 kitty 协议。什么也不说的
  终端保留半块字符，而 WezTerm 和 Konsole —— 它们说可以却显示不了这些图片 ——
  按它们给出的名字被认出来。在 tmux、screen 或 zellij 下不询问，设了
  `ALTER_ZERO_IMAGE_PROTOCOL` 时也不询问；而在 Zeroth 启动时打的字仍能到达
  prompt。GNU screen 和 zellij 现在像 tmux 一样被认出是 multiplexer，所以外层
  终端的变量不再在它们里面决定协议（`docs/ui/images.md`）。
- **后台通知现在坐在模型实际读到它的地方。** 一条在模型正写下一次 tool call 时
  结束的后台命令，只在那次 call 的结果之后才到达模型，但 transcript 把它的
  `[background] … completed` 通知记在 call 前面。于是 Ctrl+D、rollout 和之后
  每一次请求都显示模型读到 "completed (exit code 0)"，然后照样向那个已经结束的
  session `bashsend`，就好像通知被注入过却从没发出去。现在 agent 每读到一条
  通知就宣布一次，而通知正好记在那一刻。一条它从未读到的通知记在回合末尾，在
  下一个回合到达模型。后台 agent 的完成通知遵循同一条规则
  （`docs/tools/background.md`）。
- **对一个刚刚结束的 session 的调用报告它是怎么结束的。** 当模型正写一个
  `bashsend`、`bashwait` 或 `bashkill` 发给某个 session，而这个 session 自己
  结束了，那个调用会回答说该 session 的最终输出"已经报告过"，而模型其实一点都
  还没看到。现在这个调用自己报告退出：模型上次看之后的输出、exit code，以及
  一句说明什么也没打进去（或者什么也没被停）。那条本该重复它的通知被撤回。在
  子代理里也一样，它的 shell 报告到它自己的队列。一个看向运行中 session 的调用
  同样撤回该 session 未读的 "waiting for input" 通知，否则它会在它所要的答案
  之后才到（`docs/tools/bash-tools.md`）。
- **一个 session 的通知属于读它们的那个对话。** 一个子代理点名主对话某个 session
  的调用，不再收回主对话关于它的通知，于是主对话仍能听到它自己的命令是怎么结束
  的；反过来也一样。一个点名由另一个对话读取的 session 的调用，会被告知它是
  怎么结束的、它属于谁，而不是 "already reported or never existed"。一个看向
  自己 session 的子代理也会撤掉它队列里那条过期的 "waiting for input" 通知
  （`docs/tools/bash-tools.md`）。
- **离线 demo 在真模型会读到的地方交出排队的消息和通知。** 它过去在一个并行
  批次的第一个 call 之后立刻交付它们，在模型从未在其中看到它们的一轮里。现在它
  像 agent 循环那样，等整个批次解析完（`docs/input/queue.md`）。
- **一次编辑改过的词在它们的高亮上读得出来。** 在一个 `Edit` 单元格里，改动的
  字符坐在一块更亮的底色上，而在大多数主题里，注释的灰色和那块底色一样亮。当
  模型改写一条注释时，改动过的词就消失在自己的高亮里：Dracula 和 One Dark 里
  1.0:1 的对比度，Nord 和 Monokai 里 1.05:1，暗色的 Catppuccin 各风味里不到
  2:1。现在一段改动的颜色与它的底色至少保持 3:1。本来就读得出来的颜色保留原样；
  读不出来的往主题的正文色提，提到刚刚够，所以一条改动过的注释看起来仍像注释。
  底色本身不变，那一行的其余部分也不变。这对内联单元格、Ctrl+O transcript 和
  权限 prompt 的预览一样适用（`docs/ui/inline-diff.md`）。
- **一行安装脚本在 UTF-8 locale 下的 macOS 上能跑。** `curl … | sh` 把脚本交给
  `/bin/sh`，在 macOS 上那是 bash 3.2 —— 而在 UTF-8 locale 里，那个 bash 把
  光秃秃的 `$name` 之后的一个字节当成名字的一部分：`"$dim·"` 索要的是一个名叫
  `dim` 加上 `·` 的第一个字节的变量，于是在 `set -u` 下安装会在 `Release` 那
  一步就中止，什么都还没下载（C locale、dash、bash 4 及以上和 busybox 都把
  它按写的样子读）。那三处展开现在以一个大括号结尾，而发布工具链的 selftest
  在光秃秃的 `$name` 再次碰到非 ASCII 字节时会失败（`docs/ops/release.md`）。

## [0.12.0] - 2026-10-05

### 新增

- **主 agent 可以像操作一个 shell 那样操作它的子代理。** 现在每个 agent 都有一个
  id，在它启动时命名，出现在它的结果和它的完成通知里，而四个新 tool 把它接
  回来：`agentsend` 给一个 agent 发消息 —— 正在跑的会在它当前这一步之后读到，
  一个已完成的会**恢复它的对话**，于是一个追问只花一条消息，而不是另起一个新
  agent 从头再来；`agentoutput` 把某个 agent 一直在做的事显示成它 tool call 的
  摘要（`Bash(curl -s …)`，各一行）加它的最终回复，带 `wait` 时阻塞到它结束，
  它的单元格随每个 call 启动而流式显示；`agentkill` 停掉一个运行中的 agent 并
  保持它能被恢复；而 `agentlist` 点名这个 session 的每个 agent。`agentoutput`
  已经交给主 agent 的答案后面不会跟一条重复它的通知，而完成后的 agent 保持可
  恢复 —— 最多最近 16 个 —— 在它们的 roster 行消失之后。这个 id 也显示在 agent
  的 Ctrl+O 单元格上（`docs/agents/agent-tools.md`）。
- **主 agent 知道你自己跟子代理说过话。** 你在某个 agent 的 session 视图里打进去
  的消息现在以你的身份到达主 agent：agent 那条答复的通知 —— 或一个等待中 agent
  的结果 —— 列出你发给它的东西，说明它来自你而不是主 agent，而 `agentoutput`
  在它到达的地方引用它。从前，那条答复回来时看起来像是主 agent 自己对话的
  一部分，而一段与一个已在前台完成的 agent 的聊天根本没到达主 agent
  （`docs/agents/agent-tools.md`）。

### 修复

- **一条运行中命令的单元格跟随还在动的行。** live 的 `Bash(…)` 单元格显示输出的
  最后四行，而对一个就地重画的程序 —— `sudo pacman -Syy` 并行下载它的数据库
  —— 那最后四行已经是跑完的进度条，而还在动的那几条藏在它们上面
  `+N lines` 的计数里，只能在 Ctrl+O 里看到。现在窗口结束在最新的改动之后两秒内
  变过的最低那一行，于是还在下载的条留在屏幕上，跑完的才是折起来的那些，窗口
  永不藏起一行它有地方放的行，页脚数出每一行被藏起来的行。一条最新输出落在
  末尾的命令 —— 每一条普通命令 —— 显示的正是它一直显示的那个尾巴；子代理的
  session 视图遵循同一条规则（`docs/ui/tool-streaming.md`）。

## [0.11.0] - 2026-10-03

### 新增

- **断掉的连接不再让回合失败。** 当连不上 provider 时 —— Wi-Fi 断了、VPN 正在
  重连、DNS 不再应答 —— 请求每隔几秒重发一次，要多久就多久，而网络一回来回合
  就自己继续。与此同时状态行说明正在发生什么：琥珀色的 `Waiting for internet…`
  带上 host 已经消失多久，下面一行点名这个 host 并数着尝试次数。Esc 仍能中断，
  一条从未发出去的消息会回到 composer。答复得不好的 provider 仍受
  `/settings` **Error retry** 的约束，它在 `0` 时也把这段等待关掉；一个没在跑
  的本地 Ollama 仍会告诉你去启动它（`docs/providers/offline.md`）。

- **没人回答的问题不再卡住 agent。** 当 agent 问你一件事而十分钟没有按任何键，
  这个问题以未作答关闭，agent 继续干活：它会被告知你不在，用它最好的判断，并在
  回复里说明它假设了什么。任何按键都让这段等待重新开始，所以你永远不会在作答
  中途超时，而问题下边框里一个 `Timeout: 9:41` 的小条把它倒数出来，最后一分钟
  转为琥珀色。你在离开之前给出的回答仍然算数。用 `/settings` 里新的一行
  **Ask timeout**，或者 `ALTER_ZERO_ASK_TIMEOUT_SECS`，来设定这段等待 —— 5 分钟
  到一小时，或者永不（`docs/tools/ask.md`）。

- Kali 容器默认包含 Node.js 和 npm，并声明 TCP 端口 22、80、3000、5000、8000
  和 9000–9009，以及 8080/8888。除非用 `docker/run.sh --port` 配置，否则发布到
  host 的端口仍限于回环端口 8080/8888。不启动任何额外的服务。

- **AUR 上的 Arch Linux 包。** `alter-zero-bin` 几秒内装好预编译的发布二进制，
  按发布的校验和验证过，而 `zeroth` 用 Arch 自己的 Rust 从源码构建打了标签的发布
  并跑测试套件。两者都覆盖 x86_64 和 aarch64，并且互相取代，所以一次只装一个。
  通过你的 AUR 助手（`yay -Syu`、`paru -Syu`）更新它们，而不是用
  `zeroth update` —— 后者会覆盖运行中二进制自己所在的目录
  （`docs/ops/aur.md`）。

### 变更

- **`/` 命令 palette 让高亮保持居中。** 滚动斜杠命令列表现在和 `/model`、
  `/resume` 这两个 picker 一样：高亮一到达中间那一行就留在那里，列表在它下面
  滚动，于是它上面和下面的命令都保持可见。只有在列表的两端它才滑到顶行或底行。
  从前列表一开始滚动，高亮就被钉在底行，把它之后的每条命令都藏起来。在描述要
  换成好几行的窄终端上，窗口会平衡高亮上方和下方显示的行数
  （`docs/design.md`）。

## [0.10.0] - 2026-09-30

### 新增

- **Jina Reader 作为一个内置 skill 随包发布。** 提到 `$jina-reader` 并给一个
  公开网页或 PDF 的 URL，就能用 curl 通过 Jina Reader 读它。它覆盖 Markdown、
  文本、HTML、JSON 和截图链接，基础访问不需要 key 就能用，并支持可选的
  `JINA_API_KEY` 认证。和 `skill-creator` 一样，它在第一次启动时安装，保留本地
  修改，能通过 `/skills` 关掉；自定义 skill 目录的覆盖不被改动。加载这个 skill
  本身不发起任何网络请求（`docs/tools/skills.md`）。

- **agent 知道它以哪个用户运行。** 它 system prompt 里的环境块多了一行
  `User` —— `User root`、`User linuztx` —— 于是它在伸手去够 `sudo` 之前就知道
  自己是不是 root。root 由用户 id 判定，绝不由名字判定：一个改了名的 root 账号
  读作 `toor (root)`，一个残留的 `$USER=root` 没法让普通用户看起来像 root，
  而一个没有名字的账号读作 `uid 1000`。子代理带着同样的一行
  （`docs/architecture/environment.md`）。

### 变更

- **更短、更清楚的 skill tool 指导。** 描述告诉模型把一个匹配的或被提到的 skill
  加载一次，并在它的说明还在上下文里时一直复用。它去掉了相互冲突的那种建议 ——
  那种建议要求每次提到一个 skill 都要再发一次 tool call。

### 修复

- **重复的 skill 调用不再在上下文里复制说明。** 执行器返回一句简短的「已经加载
  过」的提醒，不重读 `SKILL.md`，也不追加它的正文，即使模型在同一个批次里重复
  了一次调用。检测跟随每个 agent 实际的上下文，跨回合、恢复的 session 和子代理
  的续接都算；每个新子代理可以加载自己的一份。压缩、清空或回退在原说明已经不
  在时允许重新加载。失败的加载仍可重试，被禁用的 skill 仍不可用，而名字别名、
  被 hook 改写的调用、以及被掩码的目录 header 都被处理
  （`docs/tools/skills.md`）。
- **在没有 `/proc` 的地方，每个账号得到自己的临时目录。** session 的 scratchpad
  和后台输出住在 `alter-zero-{uid}` 下面，而 uid 过去从 `/proc/self` 读，在没有
  `/proc` 的地方它回答 0：在一个没有它的 Linux 沙箱里，每个账号共享
  `/tmp/alter-zero-0`，第二个账号在那里建不了自己的 session。现在 uid 在每个
  平台上都来自内核的有效 uid（`docs/tools/background.md`）。

## [0.9.0] - 2026-09-29

### 新增

- **`/secrets` —— agent 会用、却从来看不见的凭证。** 在一个新页面上存一个
  secret —— 一个名字、一个值和一行上下文 —— 而 agent 通过在命令、文件或打进去
  的输入里写它的占位符 `<secret:ROOT_PASSWORD>` 来用它：占位符只在真正运行的那个
  tool 内部变成值，而这个值在 tool 报告的每一样东西里、在权限 prompt 的预览里、
  在 `/diff` 的复核里，都又变回占位符。agent 在它的 `<system-reminder>` 里被告知
  名字和上下文，从不被告知值，于是一个 session 可以被录制或分享，而不暴露一个
  密码或一个 API key。一个点名了没有存过的 secret 的占位符在任何东西运行之前就
  被拒绝，并附上已经存了的那些名字，于是一个笔误永远不会被打进密码提示，也不会
  被写进配置文件；而 auto 模式的复核者被告知占位符是什么，所以为它自己的服务用
  一个是不会被读成窃取凭证的。这一页掩码每一个值，`secrets.json` 以仅属主可
  读写的方式写出，而一条 `!` 命令以同样的方式展开、拒绝和脱敏
  （`docs/state/secrets.md`）。

### 变更

- **每一页的标题都穿上主题。** `/hooks`、`/mcp`、`/trust`、`/export`、↓ shell
  管理器和 `/login` 各页的标题现在加粗，并浸在活动主题的横幅渐变里，就像
  `/donate` 和 `/secrets` 早已那样，而不是一个平色 —— 于是它们随 `/theme` 而变，
  和横幅一样（`docs/ui/theme.md`）。

### 修复

- **pacman 的进度不再在 sudo 密码之后看起来像又一个问题，而隐藏光标的菜单仍能
  提问。** 光标的位置只有在它被隐藏、并且伴随一次在 live 输出末尾之上的重画，
  或者伴随一个带进度条或传输线索的数字百分比时，才被当作 prompt 拒掉。被擦掉的
  菜单尾巴不算 live 输出，而一个问题里单独的一个百分比不是进度条。这覆盖了暂停
  的第一个 0% 帧和被重写的仓库状态行，而不会把每个隐藏光标都当成运行中的命令
  （`docs/tools/interactive-shell.md`）。那些显示也让一次安静的启动或
  `bashsend` 等到它的退出、下一个输入信号或等待预算。
- **一条已提交、仍在 Node 的 REPL 里跑的行不再看起来像新的 prompt。** 顶层
  `await` 跑着的时候，REPL 仍让它的终端可以接受按键。当 Linux 在终端的前台进程
  组里认出受支持的 Node REPL 时，在它 prompt 上提交的一行会为一个新 prompt 或
  一次屏幕接管而被扣住；它自己的回显和普通以换行结尾的日志不算新的输入请求。
  `bashsend` 能报 `Running`，而 `bashwait` 等着那个答案。这限定在可识别的 REPL
  调用上，包括受支持的 Node/V8 堆、栈、GC 和 JIT 选项，以及经符号链接解析出来的
  `ts-node`/`tsx` 启动器别名；已知的编译器入口路径保留它们的直接匹配。选项名
  接受下划线别名，数字操作数必须用 `=` 附着，未知选项和应用模式仍被排除。一个
  安静的异步 stdin 监听器仍可能报 `Running`；可见的问题和内核观察到的读取保留
  它们各自独立的信号（`docs/tools/interactive-shell.md`）。
- **一次密码校验不再看起来像是又一次密码请求。** `bashsend` 提交密码之后打出的
  一个换行或终端控制序列，可能在程序还在校验它的时候复活旧 prompt 或终端的
  echo-off 信号。现在两者都等一个可见的答案；重复的重新尝试 prompt 和进程探针
  探测到的终端读取仍报为等待输入。
- **一个仍在收到受节奏控制的按键的 session 不再报又一次输入请求。** 如果一次很长
  的 `bashsend` 在全部按键交付完之前就到了它的等待预算，它的结果、`bashlist` 和
  一次零预算的 `bashwait` 现在把输入交付也计入安静。剩余按键还在路上或刚刚到
  时，旧 prompt 不被报为等待，而后台的等待通知遵循同一条规则。一个在中间的输出
  或输入之前采样的进程探针也被丢弃，而不是恢复一个过时的输入状态。
- **新版 GNU `cat` 等终端输入现在被认得出。** Linux 可以把它的输入操作报成
  `splice` 而不是 `read`。进程探针现在检查输入描述符和内核等待通道，以区分一次
  终端读取和一个在全满管道上卡住的传输；一个不可用或不熟悉的等待通道仍算作
  不确定。
- **退出的帮手不再看起来像是被停掉的后台工作。** 在 Linux 上，一个未被回收的
  僵尸进程能让一条命令的进程组在它的工作全都结束之后仍然可见。那条残留进程的
  警告现在检查活着的线程，在某个 worker 比它的主线程活得久、或进程细节读不出来
  时保留警告。清理不变。
- **一个静静编译的构建不再在中途被交回来。** 一次跑 `cargo build` 的 `bash`
  调用，在长编译进行到大约十秒时会回来 `Running`，不管 agent 让它等多久：
  cargo 等编译器输出的方式本身看起来就像一个可能在等输入的程序，于是十秒安静
  就结束了这次调用，agent 只好再等一次。现在这样的等待究竟在等什么，是从程序
  自己的内存里读出来的，而一个通过管道等一个在用 CPU 的子进程的程序被算作在
  工作，于是这次调用把构建等完 —— 对 cargo、make、驱动编译器的 Node 或
  Python 脚本、在 `script` 下跑的构建，以及任何别的在等一个干活的子进程的
  东西。一个在等终端本身的程序从不会被这样算，所以一个停在 prompt 上的
  shell 或 REPL 仍被看作在那里，即使它启动的某个作业在计算
  （`docs/tools/interactive-shell.md`）。
- **一行在一个等子进程的程序之上留着的行不再被当成问题。** 打出 `Building... `
  而没换行，然后安静地等一个子进程，过去在半个秒时就回来 `waiting for input`。
  现在那个等待被看出是在子进程的管道上，那里什么也打不进去，于是命令安静满十秒
  之后调用回来 `Running`。

## [0.8.0] - 2026-09-27

### 新增

- **spinner 下面的 tip。** 一个回合跑满三秒之后，一行暗色的 `⎿  Tip: …` 出现在
  状态行下面，带着一个值得知道的键或命令 ——
  `Press ctrl+o to see the whole transcript and every tool's
  output`、`Type @ to insert a file path from this
  directory` —— 就像 Claude Code 的 spinner 提供一条那样。快速的回答永不闪出
  一条；每个回合显示下一条 tip，长回合每三分钟得到一条新的，而这一走遍跨 session
  延续，于是每条 tip 都会在任何一条重复之前轮过一遍。这一行永不落进 scrollback，
  并让位给任务 checklist。用 `/settings` → **Show tips**（按用户记住，存在
  `tips.json` 里）或一次运行的 `ALTER_ZERO_TIPS=0` 关掉它
  （`docs/ui/tips.md`）。

### 变更

- **状态行的动词随着回合进行而变，摘要与它相配。** 一个回合的 `Working…` 每 30
  秒换到下一个动词 —— `Generating…`、`Pondering…`、`Cooking…` —— 于是一个长的
  agentic 回合每半分钟换一个新词，而快速的回答保持一个词不变，下一个回合从上一个
  停下的地方接上。回合结束时，摘要是该行当时显示的那个动词的过去式：
  `Working…` 沉降成 `Worked for 12s`，`Brewing…` 沉降成 `Brewed for 3m 2s`，
  而不是自己随便挑的一个不相干的 `Done`、`Finished` 或 `Wrapped up`。子代理的
  session 视图遵循同一条规则，而 `/spinner` picker 的预览行也以同样的方式走过
  这些动词。恢复的对话保留它们被记录时的那份摘要
  （`docs/ui/status-indicator.md`）。
- **后台管理器的页面标题与其他菜单一致。** ↓ 条带的 `Background` 和
  `Shell details` 标题现在用强调色（默认青色），加粗，像 `/hooks` 和 `/mcp` 里的
  标题一样（`docs/tools/background.md`）。

### 修复

- **`gh run watch` 和其他 live 显示不再被报为等待输入。** 一个切换到全屏、每隔
  几秒重画状态、又不读键的程序 —— `gh run watch`、`gh pr checks --watch`、一个
  `tput smcup` 循环 —— 在两次重画之间安静下来时，看起来和一个停在 prompt 上的
  程序一模一样：对它的一次 `bashwait` 几秒内就回来 `waiting for input`，后台
  通知说它在问问题，而一个信了这话的模型杀掉了一条正在做它被要求做的事的命令。
  在全屏上接受按键的程序会先把终端切出行模式，所以一个把它留在行模式的显示现在
  被当作任何其他在干活儿的命令：`bashwait` 等这次运行结束而不是立刻回来，一次
  `bash` 调用把它当作运行中交回来而不是等待，而报告、`bashlist` 和后台通知都不说
  它在等输入。在 Linux 上 agent 还会问内核一个事件循环在等什么 —— 每个 Go、
  Node 和 Python asyncio 程序都以同样的方式空闲 —— 于是一个在等网络的程序，即使
  屏幕上留着一个像 prompt 的 `Fetching... `，也不再被当成问题
  （`docs/tools/interactive-shell.md`）。
- **`bashwait` 下面的 Ctrl+B 提示说明那个键在那里做什么。** 一个 `bashwait` 或
  `bashsend` 单元格过去给一条已经在后台运行的命令提供
  `(ctrl+b to run in background)`；按那个键只会结束等待，所以提示现在读作
  `(ctrl+b to stop waiting)`。这个键不起作用的 `bashkill` 不显示提示
  （`docs/tools/background.md`）。

## [0.7.0] - 2026-09-26

### 新增

- **交互式命令。** agent 现在能操作需要终端的程序：prompt（`[Y/n]` 问题、
  安装向导、密码提示）、REPL（`python3`、`node`、`psql`）、全屏程序（`vim`、
  `less`、`top`）和方向键菜单。每条命令都跑在自己的终端里，而 `bash` 调用在
  命令退出或停下来等输入时就返回；四个新 tool 操作一条仍在运行的命令 ——
  `bashsend` 往里打字，文本和具名键如 `<Enter>`、`<C-c>` 和 `<Up>`；`bashwait`
  等它；`bashkill` 结束它；`bashlist` 点名正在跑的那些 —— 读取它打出的东西
  （或一个全屏程序的屏幕）。每次调用只报告自上一次以来新增或改变的行，而它的
  单元格随它们到来而流式显示，一个就地重画的进度条。全屏程序的屏幕按你会看到的
  样子读出来 —— 框画成框，即使没有 UTF-8 locale，列和缩进完整 —— 并说明它高亮
  什么，于是 agent 能分辨一个 `whiptail`、`dialog`、`htop` 或 `mc` 菜单里选中的
  是哪一项；`btop`、`ranger`、`tig`、`fzf`、`ncdu`、`nvim` 和一场 27 个程序
  的扫测里其余那些，读出来和 tmux 画出的一模一样。当你的环境没有点名任何 UTF-8
  locale 时，session 会得到一个，所以 `btop` 在一个裸容器里也能启动；而按键一个
  一个地到达程序，每一个等它读完上一个，像人打字那样，于是一个每次读取只读一个
  键的程序 —— 又是 `btop`，还有 `top` —— 能看见每一个方向键、每一个打进它过滤器
  的字母，在忙碌的机器上和在 `sudo` 背后也是如此。一个切换屏幕、要一会儿才画出
  第一帧的程序（`btop` 探测 GPU）在它画出来之后才显示，而不是一个空屏；一个画在
  主屏幕上的菜单（`dialog`）在焦点移动时继续显示它的屏幕；而屏幕的标题引用光标
  所在的那一行（`"File Name to Write: notes.txt‸"`），于是 agent 看到 prompt 的
  默认值，而不是把它再打一遍。一个永不停止重画的屏幕（`watch -n 0.1`、
  `top -d 0.1`）在两秒内就作答，而不是把调用拖到超时；而带修饰键的按键 ——
  `<C-Left>`、`<S-Up>`、`<M-F7>` —— 按终端发送它们的方式发出。打进一个接受
  粘贴的编辑器或 REPL（Vim、nano、Python 3.13）的代码以粘贴的形式到达，于是它的
  自动缩进再也不能把一个函数变成一段楼梯；而 shell 仍是逐行打进去的，所以
  `python3` 和给它的代码在一次调用里发出去也能到达 `python3`；模型混进它按键里的
  终端输出码被丢掉而不是打出来，而它做了 HTML 转义的键（`&lt;Esc&gt;`）仍被
  按下。它按一个程序的帮助文档那样命名的键 —— `^O`，或者在全屏程序里不带括号
  的 `F3` 和 `Esc` —— 被按下而不是打成文本，写成 `30.0` 或 `"30"` 的 `wait`
  像 `30` 一样被接受，空的 `input` 会被告知什么也没打，而一个和别的文字连在
  一起的 `session_id` 会用它所点名的那个 session 作答，而不是被报为不存在。
  一条用 `&` 留了个进程在跑然后退出的命令会告诉 agent 它被停掉了，并把它指向
  `wait: 0`；而以 `&` 结尾的命令在后台启动，而不是立刻停住。一个进度条，或者在
  `sudo`、
  `ssh`、`docker run -it` 之下跑的命令，被等到结束而不是被当成 prompt ——
  `sudo pacman` 之下的一个下载是一次等待，而不是十几次 —— 而在 Linux 上会问内核
  这条命令阻塞在哪里，于是一个根本没有 prompt 在等的（`read`、`cat`）被认出来，
  而一个把 `Compiling… ` 留在屏幕上的忙碌命令不被当成问题。密码提示由终端
  自己说出来，它用 echo off 读它 —— 在 `sudo` 之下也是，那里问不到内核 —— 于是
  agent 在半秒内就听说要一个密码，而且只有它能往那里打，所以它会向你要一个它没有
  拿到的密码，而不是把你送去一个你够不到的终端；密码打错之后的一次重试不再让它
  白等；它在那里打的东西仍显示为已打。它打的密码在同一次调用里就被答复 —— 一次
  拒绝加下一个 prompt，或者命令的头几个字 —— 而不是在 sudo 还在校验它时就给
  `(no new output)`；而一个在 agent 正决定要不要等的时候冒出来的问题也结束那次
  等待，而不是等它耗完而问题早已在屏幕上。一次调用的头部点名它往里打的那个程序
  （`● BashSend(python3 ← print(1)⏎)`），在它的权限 prompt 之上也是如此，在被你
  拒绝的调用上也是。在后台启动的命令可以被同样地等待、中断和结束，而一条在没人看
  的时候停下来问问题的命令会说明这一点：一行琥珀色的
  `● Background command
  "…" is waiting for input`，以及一条让 agent 能回答它的注记。一个仍在跑的
  session 出现在页脚的 shell 计数里，而 ↓ 管理器显示它的 live 屏幕；它的单元格
  以一行暗色的 `Waiting for input · session …` 结尾（在密码提示上是
  `Waiting for a password
  · …`）。往一个 session 里打字像命令一样征得许可 —— 由启动它的那条命令的
  「不再问」规则覆盖，或者为整个 session 批准一次 —— 而 auto 模式的 classifier
  会复核它；等一个 session、中断它或结束它从不发问。agent 永不猜一个它没有拿到的
  密码：它问你（`docs/tools/bash-tools.md`、
  `docs/tools/interactive-shell.md`）。

### 变更

- **命令在终端里、在 `bash` 之下运行，并且永不因为耗时长而被杀掉。** `bash`
  tool 的 `timeout`（毫秒，超过之后命令被杀）现在是 `wait`，以秒计：调用在把命令
  作为仍运行中的 session 交回来之前等多久，agent 可以再等它一次 —— 一个长构建
  不再输给对它要花多久的猜测，而当作秒写的 `"timeout": 30` 不再在 30 ms 之后杀掉
  一条命令。`wait: 0` 取代 `run_in_background`；旧名字仍能被理解。命令在 `bash`
  装着的地方在它之下运行，而不是 `sh` —— 在 Debian、Ubuntu 和 Kali 上那是
  `dash` —— 于是 `[[ … ]]`、`source` 和 `{1..3}` 都能用。一条安静的命令
  （`sleep`、一个链接步骤、一个在等网络的 `curl`）跑到它的退出，而不是在两秒
  沉默之后回来，好像它在等输入。一条结束的命令打出的东西按它被写出来的样子到达
  agent，tab 和行尾空格都在，而一段很长的输出保住它的开头和结尾，并有一行点名
  装着全部内容的日志文件 —— 从前一条普通命令只保留它的前 64 KB，对其余的什么也
  不说。`bashkill` 在被杀之前先请求一条命令停下（`Ctrl+C`，然后 `SIGTERM`），
  于是一个 dev server 或 `docker compose up` 干干净净地关掉
  （`docs/tools/bash-tools.md`）。
- **命令输出里的进度条读作一行。** 一条 `bash` 命令的输出、一个后台 shell 的输出
  和一条 `!` 命令的输出，现在按终端显示它们的方式读：一个用 `\r` 重画的 `curl`、
  `tqdm`、`ffmpeg` 或 `rsync` 进度条一次到达 agent，在它的最终状态，而不是它画过
  的每一帧挤在一行里；颜色转义也不见了；tab 和行尾空格保留。一条运行中命令的
  单元格显示那条条在原地走。一个后台 shell 的 `.output` 文件仍原样保留写出的输出
  （`docs/tools/interactive-shell.md`）。
- **错误的 toast 用一个更柔和的红色。** 一个失败的 toast —— `Copy failed: …`、
  `Can't switch to …`、一个解析不了配置文件 —— 过去被涂上主题完整的错误红，那是
  错误圆点和失败 tool 单元格的墨色，这会让一行四秒的状态行成为屏幕上最吵的东西。
  现在它穿的是主题的红色往 info toast 用的暗色掺了三分之一：一眼还是红的，但比
  错误圆点安静（在默认的 Catppuccin Mocha 上 `#F38BA8` 变成 `#CA89A4`）。
  `ansi` 主题保留终端自己的红，那个没什么可掺。`/skills` picker 的
  session-off 注记用同一种颜色，所以它也柔和了
  （`docs/ui/toast.md`）。

### 修复

- **并行 tool call 期间的 Esc 保住每一个 call —— 在屏幕上，也在模型的上下文里。*
*
  当模型一次跑好几个 tool 时，一次中断过去只结算批次最前面的那个 call，把还在
  `⎿ Waiting…` 的那些扔掉：它们从屏幕上和对话里都消失了，于是下一个回合的模型
  不再知道它曾要过它们。现在批次里每个 call 按顺序结算成它自己红色的
  `⎿ Interrupted by user` 单元格，在 `Conversation
  interrupted` 通知之上，而下一次请求重放整个回合 —— 每个 call 带着模型给它的
  参数，每个都被答以 `Interrupted by user`。批次中途的后端错误和中途被停掉的子
  代理也一样。这也修好了被中断的单元格根本到不了屏幕的问题：在一条运行中的 tool
  上按 Esc 之后，窗格过去只显示那条通知，直到有什么东西把它重画
  （`docs/input/interrupt.md`、`docs/tools/parallel-tools.md`）。
- **在 MCP tool 的 prompt 上点「不再问」不再声称切换到 edit 模式。** 选它会按
  应该的那样存下这个 tool 的规则，但页脚随后读作 `edit`，还冒出一个
  `Mode: edit — file edits run without asking` 的 toast，尽管文件改动仍会发问。
  现在模式不动，而 toast 点名那条规则：
  `Won't ask again for mcp__… in this project`
  （`docs/state/permissions.md`）。
- **没有东西可复制的 `/copy` 不再显示为错误。** `No agent response to copy` 过去
  是红的，而 `Nothing to export` 和 `Nothing to
  compact` —— `/export` 和 `/compact` 同属的「暂时没什么可动手」的情况 —— 是暗色
  的。没有任何东西失败，所以它现在是一个普通的 info toast，和那两个以同样的方式
  弹出。一次真正失败的剪贴板写入仍是错误（`docs/ui/toast.md`、
  `docs/ui/copy.md`）。
- **登录 Ollama Cloud 不再连本地 Ollama provider 也一起配置好。** 两者都从
  `OLLAMA_API_KEY` 读它们的 key，而一个 key 解析出来是让一个 provider 算作已配置
  的条件之一，于是为 **Ollama Cloud** 粘贴一个 key，在同一次击键里就把普通的
  **Ollama** 标成了已配置：它的行在 `/login` 里变成 `✔ configured`，下一次打开
  `/model` 会在云端的列表旁边去取 `http://127.0.0.1:11434`，把那个服务器的拒绝
  涂成红色，而云端的 key 会作为发往 `OLLAMA_HOST` 所指任何地方的每个请求的
  bearer
  随行。现在两个 provider 读两个变量：`ollama_cloud` 保住 `OLLAMA_API_KEY` ——
  这是 Ollama 自己对托管 API 的 key 的叫法，所以已经存下或导出的都不必动 —— 而
  你自己跑的服务器读 `OLLAMA_HOST_API_KEY`，它是给它前面的代理的可选 bearer。
  如果你曾为自己的服务器而不是为云端设过 `OLLAMA_API_KEY`，把它改名；一个未授权
  的答复现在会点名这两个变量（`docs/providers/ollama.md`）。

## [0.6.0] - 2026-09-22

### 新增

- **`/export` —— 把对话当作纯文本，送到剪贴板或一个文件。** 它是 `/copy` 覆盖
  整个 transcript 的兄弟：命令打开一个两行的页面 —— `Copy to clipboard` /
  `Save to file` —— 而这一次选择把 Ctrl+O 那一页按文本从上到下写出 —— 它开头的
  启动横幅，然后每条消息和每个 tool call 的完整输出 —— 按终端的宽度，或者送到
  系统剪贴板（`/copy` 自己的路径，对无头或 tmux session 走 OSC 52 回退），或者
  写到一个在工作目录里的 `conversation-YYYY-MM-DD-HHMMSS.txt`，永不覆盖已经在那
  里的文件。在一个子代理的 session 视图里它导出那个 agent 的 transcript；空的
  对话
  是 `Nothing to export` 的 toast，而不是一个空文件（`docs/ui/export.md`、
  `scripts/smoke.sh` Phase 122）。
- **一个给离线后端用的慢速流压力台。** `ALTER_ZERO_CHUNK_DELAY_MS` 设定 dummy
  在每一个流式片段之后的停顿（`ALTER_ZERO_STARTUP_DELAY_MS` 的双胞胎），而一个
  提到 *markdown* 的 prompt 播放一段新 demo：渲染器认识的每一个 markdown 元素
  —— 标题、行内样式、链接、嵌套列表和有序列表、任务项、一个引用块、两个围栏代码
  块、一张表、一条横线 —— 装进一条很长的回复，以 token 大小的片段流出来，把标记
  一个字符一个字符地拆开，就像真模型的 token 到达那样。用
  `ALTER_ZERO_CHUNK_DELAY_MS=400
  zeroth` 然后 `stream some markdown`，就看这条管线在一个吃力的模型的节奏上保持
  平稳；`scripts/smoke.sh` Phase 121 证明它做得到。

### 修复

- **什么都没配置时 `/mcp` 不再把光标停在它的底线上。** 一个没有服务器行的
  `/mcp` 页面没有高亮的 `❯` 给隐藏的光标停，于是它落到收尾横线的远角 —— 在一个
  带光标移动动画的终端（kitty 之类）上，每次打开都飞到不知哪里去。现在没有高亮的
  页面停在它收尾提示之后的紧后面，就像 Ctrl+O、Ctrl+D 和 `/resume` 那样；
  `/hooks` 和 `/mcp` 的详情页以及一个没有 hook 的事件的空状态遵循同一条规则，
  只有一个短到显示不了提示的终端保留那个角。
- **慢模型不再在没有同步输出的终端上让输入框闪。** 每一条提交进 scrollback 的
  回复行都会在重画之前把 live 区域清空。在一次同步更新（DEC mode 2026）里那看不
  出来，但 Terminal.app、xterm 和较老的 VTE 终端在它们的刷新到点时渲染已经解析的
  东西，而一个几 KB 的提交帧可能跨一次 pty 读取分开，清屏在前半段 —— 一帧没有
  框，
  每提交一行一次，于是一个每秒流几个 token 的模型造成大约每秒一次的闪烁。现在
  这一区域就地重画，只有上一个区域留在它下面的行被清掉，于是没有任何一帧装着无框
  的状态，加括号不加括号都一样（`docs/ui/slow-stream.md`）。
- **输入框不再在一个流式代码围栏处弹跳。** 一个收尾围栏让流式条带没有东西可预
览，
  于是条带丢掉它的预览行和空隙，输入框跳上去两行，直到下一行的第一个字符到达；
  一个逐字符流进来的开头围栏，在整整一个 token 的时间里从一行散文变成一行都没
  有，
  一次一行的跳。快流下各一帧，慢模型节奏下每个代码块半秒。现在条带保住它原有的行
  直到下一段内容填满它们（同一条规则让一张在窄终端上重排出更少行的表格不把输入框
  抬起来），于是输入框在 token 之间保持不动，且只会往下移。
- **跨回合的 prompt 缓存，以及它背后的 token 刷新。** 一个新的真人回合所发的请
求，
  过去会从显示历史重建此前的每一个 tool call —— 一个并行批次被拆成每条 assistant
  消息一个 call，而在那些原样传递 id 的 wire 上给出新的 `call_0`、`call_1`… id
  —— 于是 provider 看到一个与它已缓存的不同的前缀，从第一个批次起就按全价重读
  对话
  （实测：一个并行批次之后的一个追问回合，从前它的输入有 62% 读自 OpenRouter 的
  cache，现在是 99.9%）。后端现在保住它上一次的请求，每当重建出的对话与它匹配时
  就复用那个确切的前缀 —— 文本、图片、tool 名、参数和结果都一样，于是一次回退、
  一次编辑或一次压缩仍发出它们本来的意思 —— 而一个模型在它的 call 之前打出的一行
  空行不再算作差异，那正是从前让 Claude 和 ChatGPT 两条 wire 永远匹配不上的原
  因。
  那些显式的 cache 断点（OpenRouter 的 Anthropic 和 Qwen 路由）锚在上一次请求的
  边界上 —— 一个 tool 结果的边界，不只是最后一条人类消息 —— 于是一个大的并行批次
  没法把上一次的写入顶出 provider 的回看窗口。同一时刻错过 access-token cache 的
  请求（一批子代理，回合旁边那次 `/model` 取数）共用一次刷新，而不是各自呈上
  同一个一次性 refresh token，那会为它们所有人把它作废；一次来自重建配置的轮换
  重新指向每一个更旧的别名，而一次新的登录忘掉它取代的那个账号的每一个缓存
  bearer。`.env` 的 key 存储通过一个串行化的更新器原子地写入，登录和刷新都一样
  —— 以私有方式创建，穿符号链接写进它点名的那个文件，在旧文件读不出来时永不替换
  成一个残缺文件。而一个 ChatGPT token 的缓存寿命来自授权自己的 `expires_in`
  时长，而不是 bearer 绝对的 `exp`，于是一个走快的时钟没法让每个请求都重新铸造
  （并轮换）token。（`docs/providers/prompt-caching.md`、
  `docs/providers/chatgpt.md`、`docs/providers/claude.md`、
  `docs/providers/copilot.md`）
- **缓存的前缀现在经得住 `/resume`、一次后端重建和一次重启。** 每个 tool 回合的
  记录保住 provider 自己的 call id 和它们所属的批次，加上每个 call 在那个批次
  里的
  位置，于是之后某个回合推导出的请求能把一个并行批次 —— 包含 task 调用和子代理
  启动，按模型发出它们的顺序，即使某个子代理组的记录落在了那一轮普通 call 的前面
  —— 重放成 provider 缓存的那一条消息，而上面那个修复只能复用运行中的后端仍持有
  的东西。一个被 `UserPromptSubmit` hook 阻断的 prompt 也不再牺牲那个前缀。还有
  两个登录竞态：再次登录不再排在一次进行中的 token 刷新后面（屏幕可能在整个请求
  超时里冻住，而那次刷新随后可能为登录刚替换掉的凭证缓存一个 bearer），而一次
  轮换了 token 的刷新只在存储仍持有这个 session 自己的 token 时才写回 —— 永不
  覆盖
  一次更新的登录在这期间存下的 —— 从前那会在下一次启动时迫使做一遍登录刚做过
  的重新登录。（`docs/providers/prompt-caching.md`、
  `docs/architecture/context.md`、`docs/providers/chatgpt.md`）
- **`install.sh` 用 wget 也从重定向里读出最新的发布。** 在一台有 wget 没有 curl
的
  机器上，安装脚本过去改去向 GitHub 的 API 要最新的 tag —— 按地址限流，而且看
  不见
  `ALTER_ZERO_INSTALL_BASE_URL`，于是一个 fork，或发布工具链自己的替身服务器，
  会被答以这个仓库最新的发布，然后去它被告知的位置找不到那样的资产。现在两个取数
  器都从 `/releases/latest` 最终的 `Location` 读 tag，就像 `docker/build.sh`
  早已
  做的那样，而 selftest 在一个没有 curl 的 PATH 下驱动这个安装脚本。
  （`docs/ops/release.md`）

### 变更

- **agent session 视图的 composer 标签是一块点亮的 chip。**
  `── {description} ─` 这个标签过去是嵌在一条暗色横线里的暗色文字 —— 唯一一行
  说明 composer 喂的是子代理而不是主对话的文字，也是最容易被略过的一行。现在它
  骑在横线上，用活动主题的强调色配 on-accent 的墨色，也就是 ↓ 聚焦时页脚 chip 的
  装束，于是它像别的一切一样跟随 `/theme`
  （`docs/agents/agent-tool.md`）。

## [0.5.0] - 2026-09-20

### 新增

- **一个无头的 Kali Linux 容器**（`docker/README.md`、`docs/ops/docker.md`）。
  `docker/build.sh` 围绕最新的已发布版本构建一个小小的 Kali Rolling 镜像 ——
  由 `install.sh` 下载并做 SHA-256 校验，从不编译，所以一次构建大约一分钟 ——
  而 `docker/build.sh ~/projects/site` 接着创建一个以那个文件夹为 `/workspace`
  的容器。用 Podman 也一样：`--engine podman`。容器以 root 运行，把登录、设置和
  session 存在一个比它活得久的卷里，并把端口 8080 和 8888 发布到你的机器上，供你
  在里面启动的任何东西使用（`docker/run.sh --port`、`--bind`、`--no-ports`）。
  这些工具是按实测体积挑的 —— `git`、`ssh`、`curl`、`jq`、`rg`、`python3`、
  `nmap`、`nc`、`socat`、`whois`、`dig`、`ping`、`traceroute`、`ip`、`ss`、
  `net-tools` 和另外几个，在 Kali 基础之上约 280 MB，没有桌面、元包或编译器 ——
  而 `--with "PKG …"` 把你自己的烤进去。用
  `docker exec -it -e TERM -e COLORTERM -e TERM_PROGRAM -e KITTY_WINDOW_ID
  -e TMUX alter-zero-kali zeroth` 打开它，会转发你终端的身份，那正是让 kitty、
  Ghostty、iTerm2 和 WezTerm 里画出真图片而不是半块字符的原因；
  `docker/run.sh --clipboard` 转发桌面的 Wayland 和/或 X11 socket，好让 Ctrl+V
  能粘贴一张复制来的图片。遥测在任何安装里都保持原样，而容器把它的发行版报成
  `kali`。容器被授予 `NET_RAW` —— 作为 root，`nmap localhost` 是一次 SYN 扫描，
  而 Docker 默认就授予这个能力，Podman 4.x 不授予，所以没有它，同一个镜像在一个
  引擎上答 `Couldn't open a raw socket`，在另一个上照扫。它停留在一个具名能力，
  永不 `--privileged`，永不用 host 网络；`docker/run.sh --no-net-raw` 把它去掉。
  `python3` 和 `pip` 来自一个在 `/opt/az-venv` 的 virtualenv，它默认在你的 shell
  和 agent 自己的命令里是激活的，所以 `pip install` 开箱即用。Kali 的
  系统 Python 被标记为外部管理（PEP 668），而镜像不带系统 `pip`，所以没有它根本
  装不上一个 Python 包；系统解释器原样不动地在 `/usr/bin/python3`，而 pip 缓存到
  `/var/cache/pip` 而不是 `/root` 之下，于是一个容器写不进去的 home 卷不会让每次
  安装都以一条缓存被禁用的警告开场。`docker/run.sh` 创建的容器名叫
  **`alter-zero-kali`**，Linux hostname 是 **`az-kali`**。这个 virtualenv 在登录
  shell 里也能用：`bash -l` 保住 `VIRTUAL_ENV` 却丢掉它的 `PATH` 条目，而
  `su - root` 还会清掉 `VIRTUAL_ENV` 和 `PIP_CACHE_DIR`。那个共用的 shell hook
  恢复缺失的默认值和 `PATH`，保留非空的覆盖。交互式 shell 从镜像而不是从 `/root`
  卷得到 `deactivate`。用重建的镜像重建容器以收到这些改动，同时保住 home 数据。

### 变更

- **图片粘贴说明在没有显示的时候它为什么用不了**
（`docs/input/image-paste.md`）。
  在一个容器或一个 SSH session 里，Ctrl+V 没有桌面剪贴板可读，而它过去用探测
  X server 时撞上的任何东西作答，有时还是在一段暂停之后。现在它立刻作答：
  `no desktop clipboard in this session (neither DISPLAY nor WAYLAND_DISPLAY
  is set)`，后面跟着改用什么 —— 把图片存到 session 够得到的地方，让 Zeroth 读
  它的路径。一个被点名却连不上的显示保留它原本的原因，并加上同样的建议。

- **运行中的 `Bash` 单元格显示它自己的时钟和它的超时，不管它的输出**
  （`docs/ui/tool-streaming.md`）。live 单元格的页脚读作
  `+18 lines (22s · timeout 1m 50s)`：命令已经跑了多久，旁边是它所受的超时 ——
  模型自己的 `timeout`，或者它一个都没点名时 tool 的 2m 默认值 —— 于是一条长命令
  说出它的预算还剩多少。时钟那一行现在永远在：一条输出装得下窗口的命令在它下面
  显示一行光秃秃的 `(10s · timeout 10m)`，而一条什么都没打出来的命令在它
  `⎿ Running… (10s · timeout 2m)` 那一行上计数，而不是在一行光秃秃的
  `Running…` 上坐到底。Ctrl+B 提示照旧跟上；`!` shell 的
  `⎿ Running… (Ns)` 行不变，`!` 命令没有超时。
- **一条 `!` shell 命令把它的全部输出就地显示出来**
  （`docs/input/shell-command.md`）。提交后的单元格过去在三行之后就折在
  `… +N lines (ctrl+o to expand)` 后面，所以读一个 `! git status` 或
  `! ls -la` 意味着要打开 transcript。现在每一行都显示在对话里 —— 空行保留，
  长行按词换行，重整过的 JSON 文档整个显示 —— 没有东西要展开。超过内存上限的
  输出仍在它一直停的地方停，而从前只有 Ctrl+O 视图携带的那个暗色 `…` 标记现在也
  收尾内联单元格，于是一次截断在读输出的地方就看得见。模型的 `Bash` 单元格保留
  Claude Code 的折叠。
- **一个运行中 tool 的圆点在闪，而不是在呼吸**
  （`docs/ui/tool-pulse.md`）。一个仍在执行的 `Bash`、`Read`、`Write` 或 `Edit`
  单元格上的 `●`，过去每秒在两个灰之间缓动一次。现在它是 Claude Code 的运行点：
  那一个静止的灰色，显示半秒、藏起半秒，圆点不在时头部文字保住它的列，所以
  `Bash(…)` 永不位移。同一个闪烁也跑着 live 的 `● Running {n} agents…` 树、一个
  孤零零的 `● Agent(…)` 单元格、`● Calling …` MCP 单元格和 `● Thinking…` 头部，
  全在一个时钟上，而在 `ansi` 主题下它以同样的方式工作 —— 从前那个呼吸没有第二个
  色调可在其间移动，于是一动不动。提交后的单元格和 Ctrl+O transcript 照旧保着
  它们
  的圆点；`pulse` spinner 样式保留它的呼吸。

## [0.4.0] - 2026-09-18

### 新增

- **用 `/diff` 做全屏的 Git 改动复核**（`docs/ui/diff.md`）。在一个可搜索的文件
  浏览器里复核已暂存、未暂存和未跟踪的改动，旁边是所选文件的补丁。一个部分暂存
  的文件为索引和工作树分别出现，于是后来的编辑不会把已暂存的工作藏起来。这次复核
  跟随活动主题，带新增/删除计数、旧/新行号、着色的改动、hunk 导航和水平滚动。
  Tab 切换面板；窄终端全宽显示聚焦的那一面。Git 的读取在后台跑，`r` 刷新快照，
  而 Esc、`q` 或 Ctrl+C 回到对话，包括在复核开着时完成的回复。它从仓库子目录和
  链接的 worktree 里都能用，包括还没有第一个 commit 的仓库。二进制文件和大的预览
  带有明确的注记。只在 Git worktree 里打开；复核从不暂存或改动项目文件。

## [0.3.0] - 2026-09-17

### 新增

- **ChatGPT Codex 的设备码登录**（`docs/providers/chatgpt.md`）。一台没有浏览器
的
  机器 —— 一个 SSH session、一个容器 —— 过去登录不了 ChatGPT 订阅，因为它唯一的
  流程交给用户一个必须由浏览器带回本地端口的链接。在 `/login` 的 `ChatGPT Codex`
  行上按 Enter 现在先问怎么登录，`Browser login (default)` 或
  `Device code login (headless)`，而设备码那一行跑 Codex 自己的设备码流程：一个
  一次性代码显示在 `auth.openai.com/codex/device` 旁边，在 GitHub Copilot 的代码
  落到的同一个页面上，在轮询 OpenAI 等批准的时候倒数，最后存下同样的 refresh
  token —— `/model`、✓ 标记和下一次启动都不需要任何新东西。从任一登录页按 Esc
  回到那个选择，刚试过的那一行高亮。`ALTER_ZERO_OPENAI_ISSUER` 把两个流程都指向
  另一个认证服务器（一个 fork 自己的，或者 smoke 套件的本地桩，它离线驱动整个
  流程）。

### 变更

- **一个模型列出的每一个速度档都是它自己的一条命令**
  （`docs/providers/fast-mode.md`）。`/fast` 过去是一个静态的 palette 行，循环
  标准 → 列出的每一档 → 标准，而在一个一档都没列的模型上答
  `does not support fast mode`。现在 palette 按模型自己的 `service_tiers` 为每
  一档
  构建一条命令 —— `/fast`、`/ultrafast`，记录点名什么就是什么 —— 紧跟在
  `/model` 之后列出，带后端自己的描述（`1.5x speed, increased usage`），于是一个
  后端明天加的档，在它被列出的那天就是一条命令，而关于一档没有任何东西是硬编码
  的。每条命令切换它的那一档，codex 的方式：跑它来选中那一档
  （`Speed: fast — 1.5x speed, increased usage`，页脚在思考模式之后穿上那个
  名字），再跑一次回到标准，跑另一档的则直接切过去。一个一档都没列的模型不列这样
  的命令 —— 在那里敲 `/fast` 什么也匹配不到 —— 而 `/help` 列出 palette 显示的
  东西。`smoke.sh` Phase 117 既驱动一个一档都没列的模型，也驱动一个由桩提供的、
  点名了两档的列表。
- OpenAI 订阅 provider 在 `/login` 和文档里叫 **ChatGPT Codex**（它过去叫
  `OpenAI (ChatGPT)`），**它的 id 是 `chatgpt_codex`，它的 token 变量是
  `CHATGPT_CODEX_REFRESH_TOKEN`**（它们过去是 `openai_chatgpt` 和
  `OPENAI_CHATGPT_REFRESH_TOKEN`）：它是按 Codex 到达它的方式到达的 ChatGPT
  席位，而旧拼写在读取时像是第二个 OpenAI API-key provider
  （`docs/providers/chatgpt.md`）。不需要重新登录或重新选择 —— 存在旧变量下的
  token 仍能登录，直到下一次轮换把它写回新变量之下；一个点名旧 id 的
  `config.json` 选择或 rollout 的 `model` 记录会落到这个 provider 上；仍然导出
  它的 `ALTER_ZERO_PROVIDER` 钉住同一个 provider；而你自己写
  `auth = "openai_chatgpt"` 的 provider 文件仍被读作这次登录。

### 修复

- **就地重写的图片显示它的新字节。** 让 agent 下载一张图片，再把它变成黑白：
  转换发生了，agent 重新读了文件，而新的内联单元格仍显示彩色版本 —— 而 Ctrl+O
  显示的是转换后的。编码后的图片按摆放位置缓存，而一个摆放位置是该文件的路径和
  单元格尺寸，所以同一个文件在同样尺寸上重写，被端上的是改动之前编码的那一条。
  现在一次编码记住它被做出来时文件的尺寸和 mtime —— 上传缓存早已据以建键的那两个
  事实 —— 而一个不再匹配的文件被重新编码（`docs/ui/images.md`，
  *A file rewritten in place*；`smoke.sh` Phase 107d）。

## [0.2.0] - 2026-09-16

### 新增

- **ChatGPT 模型的 fast 模式** —— codex 的 `/fast`，移植过来
  （`docs/providers/fast-mode.md`）。一个 ChatGPT 列表里点名了速度档的模型
  （今天一个已登录账号上的每个模型：`Fast — 1.5x speed, increased
  usage`，`gpt-6-astra` 上是 `2x`）可以用 `/fast` 切到优先处理：请求在 codex 的
  `x-codex-routing-hint` header 旁边带上 `service_tier: "priority"`，页脚在思考
  模式之后显示 `fast`，toast 重复后端自己的费用说明，而这个选择和模型选择一起按
  目录持久化（`config.json` 新的 `speed` 块），并在一次 `/model` 切到另一个列出
  该档的模型时随身带过去。一个列了不止一档的模型逐档循环；一个一档都没列的模型
  对 `/fast` 答一个 `does not support fast mode` 的 toast，就像 Ctrl+T 对一个
  不是推理模型的模型那样。一个钉在别的模型上的子代理和 auto 模式的 classifier 以
  标准速度运行。既有的安装在下一次启动时于后台探测一次列表，就像 vision 字段
  刚出现时那样，并记下它们学到的东西。

### 变更

- **一段对话记得它的模型。** 每个 session 的 rollout 现在记录它跑在哪个模型上
  —— 随文件一起，并在每次 `/model` 选择、Ctrl+T 循环和能力探测时再记一次 ——
  而 `/resume`、`--resume` 和 `--continue` 把那个模型带回来，而不是目录当前的
  条目。同一个目录里的两个 zeroth 实例可以各跑各的模型：在其中一个里做一次
  `/model` 选择仍决定那里一个*新* session 从什么开始，但另一个实例保住它已有的
  模型，而之后恢复哪一段对话都会在它当时所用的模型上重新打开它。一次
  `ALTER_ZERO_MODEL`/`ALTER_ZERO_PROVIDER` 的钉住仍在那次运行里胜出，而一个记录
  下来的模型，如果它的 provider 在这台机器上没有 key，会以一个
  `Can't resume on …` 的 toast 报告，而不是被悄悄换掉。记录还带着模型的速度档，
  于是一段切到 `/fast` 的对话就在那一档上回来
  （`docs/state/session-model.md`）。

## [0.1.2] - 2026-09-15

### 变更

- `/resume` picker 像 `/model` 列表那样滚动：在一个比屏幕更高的列表上，高亮的
  session 骑在中间那一行，于是它上面和下面的 session 都留在视野里，每次 ↑/↓
  把下一个滚进来。它过去把高亮钉在它刚越过的那条边上，于是往下滚只在底部显示
  一个新行、再往后什么都没有，而往回滚只把高亮移到屏幕上已有的行上，根本不滚。
- `bash` tool 的 `run_in_background` 现在说明这个 flag 是*干什么用*的 —— 一个
  dev server、一个 watch 构建、一整套测试 —— 而不只是它怎么工作，并指出它默认
  为 false（`agent` tool 自己的 `run_in_background` 默认反过来）。而两种后台启动
  —— 一条命令和一个子代理一样 —— 现在都以*你会在它结束时收到最终输出*作答，
  取代*你会被重新调用*：一个该期待的结果，而不是一个要提防的内部事件。这样一
  来，在前台跑的长命令少了，为了一份会自己到达的结果去重读中间输出文件的轮次
  也少了。

### 修复

- 遥测现在在更新发生的那天就报告它。每日 ping 点名 app 版本，但它一天一次的
  节流只按日子建键，于是 `zeroth update` 之后的第一次启动什么也没发，直到 UTC
  午夜；而收集器的 `INSERT OR IGNORE` 直接把同一天的 ping 丢掉（回 `204`），
  于是一次强制的也把那一天的行 —— 和 dashboard 的 Versions 面板 —— 留在旧版本
  上。
  `telemetry.json` 现在在 `last_ping_day` 旁边记录 `last_ping_version`，在不同
  版本上的一次启动会再 ping 一次，而收集器的 `(day, id)` 行是一个 upsert，把这个
  安装挪到它的新版本上。安装 id 经得住一次更新，于是一次更新仍是一个用户，永不
  是一次新安装。重新部署收集器（`cd telemetry && npx wrangler deploy`）；不需要
  数据库迁移。

## [0.1.1] - 2026-09-15

### 新增

- **Venice 作为一个直接的 provider。** `/login` → **Use an API key** →
  **Venice** 收下你自己 Venice.ai 账号的 key，而 `/model` 直接从
  `api.venice.ai` 列出 Venice 的目录 —— Agent Zero API 经由它的代理到达的同一批
  私密、无审查模型，走同一个请求：思考模式仍驱动
  `venice_parameters.disable_thinking`，session 的 `prompt_cache_key` 仍钉住
  Venice 的隐式 prompt cache，而每个模型的上下文窗口、视觉和推理支持仍从它的
  `model_spec` 记录读出。设 `VENICE_API_KEY`（或者 `ALTER_ZERO_PROVIDER=venice`
  并把 key 放在 `.env` 里）来直接在它上面启动。

### 修复

- 运行中 `bash` 单元格的 `+N lines (Ns)` 页脚现在从命令启动那一刻起计数，而不是
  抄状态指示器的回合计时器，于是一条在一个回合进行到一分钟时启动的命令以 `(0s)`
  开场，而不是 `(60s)`。子代理的 session 视图以同样的方式数它自己那条运行中的
  命令，从那次 call 的启动起，而不是 agent 的全部运行时长。

## [0.1.0] - 2026-09-13

**Zeroth 初始发布**

### 新增

- **一个在你的项目里干活的 agent。** 模型读文件、写文件和改文件，并通过一个
  agentic tool 循环运行 shell 命令。并行 tool 批次在运行之前先宣布，命令输出实时
  流进对话并折成可展开的预览，而长时间运行的命令用 **Ctrl+B** 移到后台（或者从
  那里启动），在空 composer 上按 **↓** 则是管理器条带。
- **对跑什么有控制。** 一个内联的批准 prompt 在执行之前显示整个文件、diff 或
  命令，带「允许一次」、一条 session 或按项目的规则，或者一次带着你的指令的拒绝
  （**Tab**）。四种权限模式 —— `manual`、`edit`、`auto`（一个 LLM 安全
  classifier
  复核日常命令）、`master` —— 用 **Shift+Tab** 循环并按项目持久化。shell 子进程
  从终端脱离，于是一个 `sudo` 提示很快失败，而不是劫持 UI。
- **checkpoint 和回退。** 按目录的文件系统 checkpoint 每个回合把工作树快照进一个
  隔离的 git 存储；**Esc Esc** 退回到更早的一条消息并连同它恢复文件。
- **可扩展性。** 在 `agents/*.md` 里定义的子代理（内置的 `general-purpose` 和
  `explore`），各有自己的 context、模型和 tool allowlist，以及一个与运行中的子
  代理聊天的内联 session 视图；skill 作为 `SKILL.md` 文件夹，带一个 `$` 提及
  picker、`/skills` 浏览器和一个内置的 `skill-creator`；MCP 服务器走 stdio、HTTP
  和 SSE（含 OAuth），用 `/mcp` 和 `zeroth mcp` 管理；`hooks.json` 里
  Claude Code 兼容的生命周期 hook；`AGENTS.md` 项目指令，带 `/init` 来写一个；
  结构化任务列表带一个实时 checklist；回合中途向用户提问；一个按 session 的
  scratchpad 放临时文件。
- **项目信任。** 项目级的 `.alter-zero/` hook 和 MCP 服务器被取指纹，在没被
  `/trust` 批准之前是惰性的。
- **模型 provider。** OpenAI 兼容的 Chat Completions（OpenRouter、Agent Zero
  API、Ollama Cloud）、经由设备码登录的 GitHub Copilot、经浏览器登录走
  Responses API 的 OpenAI ChatGPT、经 API key 或 Console 登录走 Messages API 的
  Anthropic，以及经它原生 `/api/chat` 的本地 Ollama。`/login` 连接，`/model` 选
  模型，每个 provider 的目录报告上下文窗口、视觉和推理支持，**Ctrl+T** 循环思考
  模式，而凡 provider 提供的地方都请求 prompt cache。
- **上下文管理。** 页脚一个上下文窗口的量表，`/compact` 在超过 90% 时自动压缩，
  以及 **Ctrl+D** 视图显示下一次请求究竟携带什么（包括 classifier 自己的
  context）。
- **一个原生终端 UI。** 对话流进终端真实的 scrollback，下面钉着 composer；流式
  Markdown 带表格和语法高亮的代码；可折叠的推理；可点击的 URL 和文件路径
  （OSC 8）；在 kitty、iTerm2 和 sixel 终端里内联图片，带半块字符回退；
  **Ctrl+V** 图片粘贴；**Ctrl+O** 完整 transcript；一个 readline 风格的
  composer，带 **Shift+Enter** 换行、跨 session 的输入历史和 **Ctrl+R** 搜索；
  `@` 文件 picker 和 `!` shell 命令；引导运行中回合（**Enter**）或排队一条追问
  （**Tab**）的消息。
- **外观。** 十一个配色主题（`/theme`）、六个横幅 mascot（`/mascot`）、九种
  spinner 样式（`/spinner`），以及一个可搜索的 `/settings` 菜单，全部按目录
  记住。
- **session 和命令行。** 每段对话都被录制；`/resume` 打开一个可搜索的 picker，
  `--continue` 重新打开目录里最新的 session，`--resume <id>` 打开指定的一个，
  而一个带引号的 `[PROMPT]` 参数直接开始一个回合。`--help` 和 `--version` 可用，
  `mcp` 子命令家族也一样。
- **遥测，公开说明。** 每天一次匿名 ping（安装 id、版本、OS、架构），在
  `TELEMETRY.md` 里完整说明，启动时显示一次，并用 **Telemetry** 设置、
  `ALTER_ZERO_TELEMETRY=0` 或 `DO_NOT_TRACK=1` 关掉。
- **一行安装。**
  `curl -fsSL https://raw.githubusercontent.com/linuztx/alter-zero/main/install.sh | sh`
  为这台机器挑出构建，按已发布的校验和验证它的 SHA-256，并把 `zeroth` 装进
  `~/.local/bin`。
- **更新通知。** 每天一次，app 问仓库的发布页有没有更新的版本出来 —— 一个请求，
  不携带任何关于你的东西 —— 并在横幅下面说明，点名那个发布和
  `zeroth update`，后者通过同一个校验过校验和的安装脚本把它装到运行中的二进制
  之上。用 **Update check** 设置或 `ALTER_ZERO_UPDATE_CHECK=0` 关掉；
  `TELEMETRY.md` 完整陈述这个请求。
- **发布工具链。** 一个 CI workflow 跑项目的闸门（`fmt`、`clippy`、`test`、
  `doc`）、smoke 套件和发布工具链自己的测试；一个发布 workflow，在 `vX.Y.Z` 标签
  上，把这个版本对照本变更日志校验、构建 Linux（x86_64、arm64）和 macOS
  （Intel、Apple silicon）归档并附 SHA-256 校验和，发布一个 GitHub release，其
  说明来自本文件 —— 全程由 `scripts/release.sh` 驱动，它还能在本地排演一次发布。

[Unreleased]: https://github.com/yxxbc/zeroth/commits/main
[0.0.0]: https://github.com/yxxbc/zeroth
[0.18.0]: https://github.com/linuztx/alter-zero/compare/v0.17.0...v0.18.0
[0.17.0]: https://github.com/linuztx/alter-zero/compare/v0.16.0...v0.17.0
[0.16.0]: https://github.com/linuztx/alter-zero/compare/v0.15.0...v0.16.0
[0.15.0]: https://github.com/linuztx/alter-zero/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/linuztx/alter-zero/compare/v0.13.0...v0.14.0
[0.13.0]: https://github.com/linuztx/alter-zero/compare/v0.12.0...v0.13.0
[0.12.0]: https://github.com/linuztx/alter-zero/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/linuztx/alter-zero/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/linuztx/alter-zero/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/linuztx/alter-zero/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/linuztx/alter-zero/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/linuztx/alter-zero/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/linuztx/alter-zero/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/linuztx/alter-zero/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/linuztx/alter-zero/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/linuztx/alter-zero/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/linuztx/alter-zero/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/linuztx/alter-zero/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/linuztx/alter-zero/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/linuztx/alter-zero/releases/tag/v0.1.0
