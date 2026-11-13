# MCP 服务器 —— `/mcp` manager 与 `mcp__server__tool` 工具

Model Context Protocol 支持，Claude-Code 风格：用户在配置文件里声明 MCP
服务器，session 在启动时逐个连接，每个已连接服务器的工具都以全限定名
`mcp__{server}__{tool}` 加入模型的工具集，`/mcp` 命令打开一个内联 manager
（第六个替换 composer 的 picker），按 服务器 → 服务器详情 → 工具 → 工具详情
逐层走进，并为需要认证的远程服务器带一条 OAuth 认证流程。

## 配置

两个 scope、三个文件，同名的第一次出现者胜出（Claude Code 的优先级，去掉我们
不建模的 `local` scope）：

- **项目**：先 `{project_root}/.zeroth/mcp.json`（应用专属位置，
  `docs/state/project-config.md`），再 `{project_root}/.mcp.json`（Claude-Code
  兼容约定，提交进仓库）——两者都定位在最近的 `.git` 根（`AGENTS.md` 的向上
  查找，`project_doc::find_project_root`），所以在 `repo/src` 里启动仍然能找到
  这个 repo 的服务器。
- **用户**：`{config_home}/mcp.json`（`~/.zeroth/mcp.json`）——个人服务器，
  每个项目都用。

**项目服务器只有在用户启用它之后才会运行**（`docs/state/config-review.md`）：
一份提交进仓库的 stdio 条目，就是本应用启动时会 spawn 的一个进程，所以新增或
改动过的项目服务器要等**启动复核**，由它来问——Claude Code 的那套对话框——
其中哪些要启用。在它被回答之前，服务器在 `/mcp` 里只是被*列出*为
`⚠ awaiting review`，从不启动；被拒绝的读作 `◯ disabled`，在那里按 Enable 会
把它打开（对项目服务器按 Disable 记录的是同一个决定——记进 `trust.json`，
从不记进用户文件的 disabled 集合）。两者都不会遮蔽用户自己的同名服务器。每个
决定都钉在该服务器的整个条目上，所以改过的条目在下次启动时会被重新询问。这是
对最初双 scope 设计的唯一一处行为改动，而且是有意为之：`.mcp.json` 以前不问
就启动，那正是 hooks 文档拒绝打开的那个「克隆即代码执行」的洞。

两个文件都带 Claude Code 的精确形状，所以一份为它写的 `.mcp.json` 在这里原样
加载：

```json
{
  "mcpServers": {
    "deepwiki":  { "type": "http",  "url": "https://mcp.deepwiki.com/mcp" },
    "everything": { "type": "stdio", "command": "npx",
                    "args": ["-y", "@modelcontextprotocol/server-everything"],
                    "env": { "KEY": "value" } },
    "legacy":    { "type": "sse",   "url": "https://example.com/sse",
                   "headers": { "Authorization": "Bearer …" } }
  }
}
```

`type` 可选：带 `command` 的条目默认 `stdio`；只有 `url` 的条目默认 `http`，
**并带自动 fallback**——一次以客户端错误失败的 streamable-HTTP 握手，会把同一
个 URL 当作 legacy SSE 服务器重试（MCP 规范的向后兼容配方），所以一个光秃秃的
`{"url": …}` 对任何一代服务器都能用。`streamable-http` 与 `streamableHttp`——
别的客户端的拼法——读作 `http`；一个 `"type": "stdio"` 条目缺了它的 `command`
就照实说（以前它读作不受支持的传输）；而一个空的、被 `touch` 过的文件是空配置，
而不是每次启动都红字「not valid JSON」。

**`${VAR}` 引用会被展开**（`mcp::expand_config`），按 Claude Code 为
`.mcp.json` 定义的方式，于是一个项目的文件对两边含义相同：`${NAME}` 与
`${NAME:-default}` 出现在启动所读的每个值里——stdio 服务器的 `command`、`args`
和 `env` 值，远程服务器的 `url` 与 `headers` 值——从不出现在名字或键里。已设置
的变量取它的值，空的也取（不像 POSIX 的 `:-` 把空当作未设置）；未设置的取它的
默认值，按写下的样子；光秃秃的 `$NAME` 是字面量，没有转义。既未设置也无默认值
的变量会挡住它所在服务器的启动，`Failed` 里点名这个变量以及它被用在哪里——
把一个字面的 `${GITHUB_TOKEN}` 当凭据发出去、或当命令跑起来，从来不是本意。
展开由 manager 做一次，取自 session 的环境（`ServerState::resolved`），每一次
connect、restart 和 token store 查询都用它；声明的配置——`/mcp`、`mcp get`
和启动复核所显示的，以及复核指纹所覆盖的——保持原样，这样留在环境里的 secret
不上屏，改过的变量也不算改过的服务器。复核显示引用本身，包括 URL 里的
`${AWS_SECRET_ACCESS_KEY}`，而这也是一个项目的文件唯一能索要它本不该有的变量
的地方——而对最值得偷的那些变量，它连问都问不到：**项目的远程**服务器不得把
session 自己的凭据（每个 provider 的 key 变量，`McpSources::protected_env`，
在边界处从 provider 表填入）展开进它的 URL 或 headers（`mcp::resolve_config`）；
它失败时点名这些变量，从不点它们的值。用户自己的文件被信任持有它们，而项目的
*stdio* 服务器本来就继承环境，所以这道守卫恰好设在一个值会离开机器的位置。

## 协议版本 —— 双时代客户端

这个客户端说**现代的 `2026-07-28` 修订版**（无状态：没有 `initialize` 握手，
协议版本 + 客户端能力搭在每次请求的 `_meta` 里，`server/discover` 是身份面），
*也*说基于握手的 **legacy 修订版**（`2025-11-25` 及更早），用规范自己的配方
探测每台服务器属于哪个时代——见下面的 `mcp::protocol` 与 `llm::mcp::client`。
探测在**每一次** connect 时都跑，它最终落定的修订版是服务器自己的回答，绝不是
记住的那个（退役的时代缓存见下文）；修订版显示为详情页的 `Protocol:` 行。

**禁用状态**按项目存在**用户**文件里——从不放在可能与团队共享的项目文件里：

```json
{ "mcpServers": { … },
  "projects": { "/abs/cwd": { "disabled": ["github"] } } }
```

——`skills.json` 的模式：空集合会丢掉该条目，一个本地没装的名字会被保留（同一个
文件也服务别处的 checkout）。一次 toggle 是对**一个名字**、在文件当前内容之上
做读-改-写（`record_disabled`、`McpManager::set_disabled`）：旧的写入者存的是
session 的整个集合，会抹掉另一个 session 在同一项目里做过的 toggle，并且在文件
解析失败时退回 `{}`——一个笔误，所有声明的服务器就都没了。解析失败的文件现在
保持原样，toggle 只在本次 session 内生效，并挂一个红 toast 说明这一点；写入失败
也同样说明。文件的每一次写入——这一次的和 CLI 的——都走
`llm::mcp::write_config_file`：在目标旁边建临时文件、sync 后 rename 到位（以前
写到一半崩溃会留下空文件），保留文件自己的权限（新建的是仅所有者，因为它
headers 和 `env` 可能装着 token），并跟随符号链接到它指向的文件，这样 dotfiles
manager 的链接仍然是个链接。

**用户**文件可以从命令行编辑——`zeroth mcp add {name} --url {url}`（以及
`add-json`/`remove`/`get`/`list`），在 TUI 启动前就解析好，像
`--continue`/`--resume` 一样；见 `docs/cli/mcp-cli.md`。

Env 旋钮：`ZEROTH_MCP`（为本次运行给 `/settings` 的 **MCP** 行播种，从不
保存——falsy 让 session 以 MCP 关闭启动，见下面*本次 session 关闭 MCP*）、
`ZEROTH_MCP_FILE`（替换*用户*文件路径；项目文件仍会被发现）、
`ZEROTH_PROJECT_CONFIG`（falsy = 完全不要项目文件——项目层自己的总开关，
与 hooks 共享，也是 smoke 套件的封闭性机制；`docs/state/project-config.md`）、
`ZEROTH_MCP_STARTUP_TIMEOUT_MS`（每服务器 initialize + tools/list 的预算，
默认 30 000——两个参考实现的默认值）、`ZEROTH_MCP_TOOL_TIMEOUT_MS`（每次
`tools/call`，默认 120 000）。

## 本次 session 关闭 MCP —— `/settings` 的 **MCP** 行

在 `/mcp` 里禁用一个服务器，是*关于那个服务器*的决定，按项目存在用户文件里。
`/settings` 的 **MCP** 行是它们之上的总开关——`true`，直到某个目录把它关掉，
**Hooks** 的姿态（`docs/state/settings.md`、`docs/state/per-directory-state.md`）
——而关掉它从不碰服务器自己的开关。这就是它的意义所在：一个当下不想用 MCP 的
session，不该非得逐个禁用五个服务器，然后再记住哪五个要重新启用。

关就是**关**，不是隐藏（`McpManager::set_active(false)`）：

- 每个连接都被丢弃——stdio 子进程按 `/mcp` 里 Disable 的方式停掉，仍在飞行中的
  connect 被取代，正在跑的登录被取消。
- 之后没有任何东西再连，不管是谁要求的。启动时的 kick、Reconnect、Enable、复核
  的批准、死掉服务器的自动重启，全都落到 `spawn_connect` 上，它在 MCP 关闭时
  拒绝，所以守卫只有一处，而不是每个调用方各一处。登录也以同样的理由被拒。
- 模型不被提供任何 `mcp__*` 工具，reminder 也不带服务器 instructions：没有
  服务器是 `Connected`，所以 `tool_specs`、`instructions` 以及边界据以重建的
  fingerprint 全都为空。来自一个开始时工具曾被提供的回合的调用，被回答为
  `MCP is off for this session — ask the user to turn it on in /settings`，而
  不是泛泛的未知工具答复。
- 每个本该连上的服务器读作 `◯ off`（`McpServerStatus::Off`）；用户禁用的保持
  `◯ disabled`，启动复核扣着的保持 `⚠ awaiting review`。

再打开（`set_active(true)`）是一次全新开始，与启动 kick 相同：每个 `◯ off`
服务器都去连——之前失败过或要求登录的会被重新询问——而一个永远起不来的（没人
设置的 `${VAR}`，与别人相撞的名字）会再报一次它的理由。

MCP 关闭时 `/mcp` 仍然能打开，`/hooks` 与 `/skills` 菜单的那条规则，列表计数
下方有一行红字 `MCP is off for this session — turn it on in /settings`——直接
读自 `/settings` 行，从不存储，也从不加在空列表上，那里的占位符才是答案。各页
仍可浏览，每个服务器自己的开关也仍会动：一个 `◯ off` 服务器自己的页面重述这条
说明——它唯一那项动作没法把它重新打开，所以页面说明什么才行——并且只提供
**Disable**，而在一个 disabled 的服务器上按 **Enable** 会让它保持 `◯ off`，
等 MCP 重新打开后再去连。没有 Reconnect，也没有登录，因为两者都无事可做。

一处都没有声明服务器时，该行读作 `false (unavailable)`（**Skills** 的规则：
没有可连的东西）。MCP 关闭时启动复核对项目的服务器一概不问——没有东西能跑——
而在一批等着它的服务器之上把该行打开，那时就会打开复核的服务器页，**Hooks**
行的规则（`docs/state/config-review.md`）。启动 toast 本该点名的配置问题，在
MCP 关闭时保持安静，等该行打开 MCP 时再说。

`ZEROTH_MCP` 以前直接拆掉 manager，于是 `/mcp` 只会在一个 toast 里解释
自己，重启是唯一的回头路。现在 manager 在每次启动时都会建起来——行关着时它
什么都不启动——所以 `/settings` 不用重启就能打开 MCP。`smoke.sh` 的 Phase 152
在一个脚本化的 stdio 服务器上驱动该行，那个服务器会标记自己何时启动、stdin 何时
关闭，所以该 phase 看到的是进程停下又回来，而不只是行文字。

## 模块拆分

别处适用的规则在这里也适用：**`src/mcp/` 是纯的**，I/O 住在
**`src/llm/mcp/`**。

- `mcp::config` —— 文件解析（`McpFile`、`McpServerConfig`：
  `Stdio { command, args, env }` / `Http { url, headers }` /
  `Sse { url, headers }`，`type` 的默认规则，按项目的 disabled 集合，scope
  标签）。条目解析不了的服务器按名字上报（一个红色启动 toast），从不沉默——
  `SKILL.md` 的姿态。
- `mcp::names` —— 命名契约。`normalize_name` 把 `[A-Za-z0-9_-]` 之外的任何
  东西映射成 `_`（Claude Code 的 `normalizeNameForMCP`）；
  `tool_wire_name(server, tool)` 构造 `mcp__{server}__{tool}`；`is_mcp_tool`
  是 `mcp__` 前缀测试（`is_task_tool` 的形状）；`parse_wire_name` 把它拆回去
  （服务器部分永不含 `__`——归一化会把连续的下划线塌掉——所以拆分无歧义）。
  一个 wire name 在**每个请求里都是一个函数名**，而 provider 会因为一个好名字
  以外的名字拒掉*整个*请求——OpenAI 形状的 API 把名字上限压在 64 个字符
  （`MAX_WIRE_NAME_LEN`），每个 API 都拒绝重名——所以另外两条规则成立。超限的
  名字整体保留服务器部分（只有当单留它会让工具名不足八个字符时才切它），截短
  工具部分，并以 `_` 加全名 SHA-256 的八位十六进制结尾
  （`mcp__atlassian-confluence-cloud__confluence_search_usin_8124286d`）；而
  `wire_names(server, tools)`——specs、fingerprint、权限 prompt 的描述和调用
  路由**全部**读的那**唯一**一处推导，所以任意两者不可能不一致——给归一化后
  相同的工具（`files.read`、`files_read`）或归一化后为空的工具（`検索`）同样
  那种哈希形状，以*原始*名字为键，于是每个都仍能在一个不依赖服务器列出顺序的
  名字下被调用。两种形状都是 `tool_wire_name` 的不动点，这正是下面那套重放能
  精确还原的原因。两个*服务器*名字归一化后相同（`my.server`、`my_server`），
  或者一个名字归一化后什么都不剩，就没法这样区分——`server_name_problems`
  拒绝这对里靠后的那个并给出理由，在 `/mcp` 里是 `✘ failed`，Reconnect 也改变
  不了这一点；`tool_display_name(server, tool)` 是 Claude Code 面向用户的
  `{server} - {tool} (MCP)`，`tool_label`/`label_from_wire` 是去掉后缀的那个
  名字（权限 prompt 把参数渲染在两者*之间*，并在它的规则里点名该 label），
  `batch_label` 是聚合的 `Deepwiki, Context7 3 times` 形式，而
  `wire_from_display` 为上下文重放把显示名反演回来。`capitalize_server`/
  `capitalize_display` 把服务器的首字符大写（`deepwiki` → `Deepwiki`），
  **只用于渲染接缝**——单元格抬头、
  `Calling`/`Called` 行、权限 prompt 的 label 和 `/mcp` 详情页的大标题：记下的
  `ToolCall::name` 保留配置的拼法，因为 `wire_from_display` 保留大小写，所以
  一条被大写的记录会重放成 `mcp__Deepwiki__…`，一个模型从未被提供过的工具
  （`the_display_capitalization_never_reaches_the_wire` 测试就是这道守卫）。
  规则键（`permissions.json`）和面向模型的 classifier prompt 出于同样的理由
  留在原始拼法上。
- `mcp::protocol` —— JSON-RPC 2.0 框架（`request`/`notification` 构造器，
  `Response` 解析并拆出 `result`/`error`，`RpcError` 完整保留错误的 `data`
  ——`-32022` 在那里点名服务器的版本），**两个时代的词汇**：现代侧
  （2026-07-28，`PROTOCOL_VERSION`）——`request_meta`/`with_meta`（每个现代
  请求都带的 `io.modelcontextprotocol/*` `_meta` 块：版本、`clientInfo`，以及
  一个*空*的 `clientCapabilities`，即 schema 自己的「没有可选能力」）、
  `discover_params`/`parse_discover`（`server/discover`，它的 `serverInfo`
  搭在结果的 `_meta` 上，从不是顶层字段）、`is_modern_error`（时代探测所依据的
  `-32020`/`-32021`/`-32022` 允许清单）、`unsupported_versions` +
  `choose_version`（`-32022` 协商）——以及 legacy 侧：`initialize` 参数
  （`initialize_params_for`，默认提议 `LEGACY_PROTOCOL_VERSION`
  `2025-11-25`）与结果解析（`ServerIdentity`：落定的 `protocol_version`、
  名字、版本、能力、instructions）；此外还有与时代无关的 `tools/list` 分页解析
  （`McpToolInfo { name, description, input_schema }`，按 cursor 串接），
  **模型手里那份 schema 副本**——每个请求都把它当作函数的 `parameters` 带上，
  所以一个被 provider 拒绝的 schema 会让每个回合都失败：`model_input_schema`
  总是把它扎根在 `{"type": "object", "properties":
  {…}}`（两个 API 家族都坚持这一对），丢掉 `$schema` 和一个不是名字列表的
  `required`，并把根上的 `oneOf`/`anyOf`/`allOf`（Anthropic 在顶层拒绝的）
  折进那一个 object——每个分支的属性，`allOf` 要求或每个备选都要求的即为
  required，备选情形下 `FOLDED_SCHEMA_NOTE` 放在 description 最前面——而
  `input_schema_problem` 点名没有任何修复能救的那些（根类型不是 `object`，
  参数名超出 `^[a-zA-Z0-9_.-]{1,64}$`），connect 会把这样的工具搁置成一个
  `ExcludedTool`，`/mcp` 的服务器页把它列在 *Not offered to the model* 下；
  还有 `tools/call` 参数构造器，以及结果映射：`CallToolResult` → 单元格/模型
  文本——文本项就是它们的文本，按顺序（只有一个时原样照抄），而二进制内容
  **只被描述，绝不被倾倒**：第一个 `image` 项变成一个 `data:` URL，搭在
  `ToolOutcome::image` 上（`read` 工具的通道，于是一个视觉模型*看得见* MCP
  图片），上面压一行 `[image: …]`，而第二张图片、`audio` 和一个 blob 的
  `resource` 各自留一行占位符，点名类型与大小（文本里的 base64 会把 64 KiB
  的输出上限全花在噪声上），一个 `resource_link` 说明它链接到什么，一个内嵌
  的文本资源就是它的文本，而未知项是 JSON，其中的长字符串被截到只剩长度；
  别的东西都没有时 `structuredContent` 顶上，而 `is_error` → 红色单元格。
  `parse_incoming` 读真实服务器发的东西，而不是 JSON-RPC 说该发的东西：被
  回显成 `"5"` 或 `5.0` 的 id，结果旁边一个 `"error": null`，以及带
  `id: null` 的错误（`Incoming::OrphanError`——对唯一的在飞请求的答复；被丢掉
  时，那个请求会一直等到它的截止时间），服务器的自有请求则保留为
  `Incoming::Request`，交给 transport 去回答。**2026-07-28 的结果规则**：
  `result_type_problem` 放过缺失的 `resultType`（早一代的服务器）和
  `complete`，其余一律判为无效——一个 `task` 句柄读作红色结果，绝不读作空的
  成功——而 `input_required` 一分为二：只带 `requestState` 的答复，是一个在
  卸负载的服务器在说「再问一次」（`InputRequired::Retry`），`call_tool` 对它的
  回答是带上回显的 state 重复这次调用（`call_params_retrying`，最多三次，第一次
  立刻，之后每次多隔 250 ms，都在这通调用的预算之内），而 `inputRequests`
  要求的是这个客户端从未声明过的 elicitation、sampling 或 roots
  （`InputRequired::Unsupported`），解析为一个点名它们的红色结果。`-32021`
  的 `requiredCapabilities` 搭在模型读到的那段错误文本里。
- `mcp::headers` —— streamable-HTTP 的请求元数据头：`header_value`
  （`Mcp-Name`/`Mcp-Param-{Name}` 的值编码，一个不是纯可打印 ASCII 的值、
  某端带空白的值、形状与那个哨兵相符的值，或空值，都用 `=?base64?…?=` 哨兵
  包起来——中间层可能丢掉空 header，而每个服务器都会解码这个哨兵）以及
  **`x-mcp-header`**（2026-07-28，SEP-2243：工具可以标记某些参数，它们的值也
  必须作为 header 搭在请求上，供不读 body 就能路由的网关用——这对 streamable
  HTTP 上的客户端是 MUST）。`param_headers` 读取工具的声明，或者拒绝这个
  工具，规则与参考 TypeScript SDK 完全一致：名字必须是一个 RFC 9110 token，
  大小写不敏感地唯一，落在类型为 `string`、`integer` 或 `boolean` 的属性上
  （没有 `number`，没有类型联合），且由一串 `properties` 键可达——注解写在
  `items` 下、写在组合子、条件或 `$defs` 里，都会让工具无效。
  `param_header_values` 按每个值的精确路径读取（缺失或 `null` → 没有 header；
  非原始类型或超过 2^53 − 1 的整数 → 没有 header，而不是一个畸形的），
  `strip_header_annotations` 把它们从模型手里那份 schema 副本里拿掉，而且是
  认得 schema 的（一个*名为* `x-mcp-header` 的*属性*保留）。
- `mcp::sse` —— Server-Sent-Events 帧解析器（`data:`/`event:` 行、多行
  data、注释行），被两种 HTTP transport 共用；作用于 `&str` 推送的纯函数，
  不开 socket 就能测。
- `mcp::status` —— UI 所消费的东西：`McpServerSnapshot { name, scope,
  config_path, status, url_or_command, auth, server_info, tools }`，配
  `McpServerStatus`（`Connected`/`Pending`/`NeedsAuth`/`Failed(reason)`/
  `Disabled`/`AwaitingReview`/`Off`）；状态字形/文字映射——`glyph()` 与
  `label()` 在 `McpServerStatus` 和 `McpAuthState` 上都**分开**保留，好让
  渲染器能按状态给字形上色，而不必去拆一个不是它拼的字符串；`status_line()`
  拼出列表行的 `✔ connected · 3 tools`（工具数是*列表的*：详情页有一行自己
  的 `Tools:`，不能把同一件事说两遍）；**`auth_state` 规则**（见下文）；以及
  工具详情页据 `input_schema` 渲染的参数清单。

### `Auth:` 行 —— 五种事实，不是两种

过去两种状态让「没有存储的 token」同时覆盖了*服务器从没要过*和*登录已经没了*，
于是完全没有认证的 DeepWiki 会在 `✔ connected` 旁边报 `✘ not authenticated`。
`mcp::auth_state` 按这个顺序推出五种：

| 状态 | 行 | 何时 |
|---|---|---|
| `Header` | `✔ authenticated (config header)` | 配置自带 `Authorization` header |
| `Expired` | `✘ expired` | 存了 token **且**服务器拒收 |
| `Authenticated` | `✔ authenticated` | 存了 token |
| `NotRequired` | `✔ authenticated` | 连上了且什么都没出示 |
| `NotAuthenticated` | `✘ not authenticated` | 远程、无 token、也不提供服务 |

配置里的 header 压过存储的授权，因为**transport 也是这么做的**——它发那个
header，从不发 bearer——所以报成授权，会提供一个重跑并清掉服务器根本看不见的
登录的选项。只有 stdio 服务器没有这一行：没有*可去认证的*远程服务器。公开
服务器的这一行是**显示出来的，不是藏起来的**：这里要不要登录，正是这一行存在
要回答的问题，沉默只会让用户纳闷。红色只留给用户能动手的那两种状态。

**为什么 `NotRequired` 报落定的那一行，而不是它自己的措辞。** 它一度写作
`◯ not needed`，那是*精确*的事实，却是错误的强调：读起来像挂在 `✔ connected`
下的一个保留意见，而用户真正在问的——「我可以用这台服务器吗？」——在这里的
答案和手持已存授权时一样。所以这一行就照实说，而**状态在需要区分的地方仍然
保持区分**：正是它在扣着 `Re-authenticate` 和 `Clear authentication`，因为
没有授权可重跑、可删除。显示上合并两种状态；行为上并不合并。

动作遵循同一套推导：一份存储的授权（有效或过期）给出 `Re-authenticate` +
`Clear authentication`——于是一份死掉的授权能在报出它的那一页上删掉，而它以前
只能手工编辑——一个 `NotAuthenticated` 服务器给出 `Authenticate`（现在从已连接
页和失败页也够得着，不只从 needs-auth 页），而带 header 的或公开的服务器两者都
不给出。

### 会向任何人列出工具的那些服务器（Google 的）—— 尚不支持

Google 的 MCP 服务器（`drivemcp.googleapis.com`、`calendarmcp.googleapis.com`、
`gmailmcp.googleapis.com` 等）不带 token 就应答 connect 并列出工具，然后对所有
调用回 401。那样连上时，一行读作 `✔ connected` 和 `✔ authenticated`
（`NotRequired`），而模型被提供的是只会失败的工具。它们的登录 provider
`accounts.google.com` 自己不注册应用——没有动态注册，没有 client metadata
document——所以本应用不借助用户在 Google Cloud 里创建的 OAuth client 就没法
登录它们（见下文*预注册客户端*）。

**这一类服务器尚不支持。** 对其中之一的匿名 connect（远程、无已存授权、无
`Authorization` header）会被**扣下**：它读作 `△ needs authentication` 和
`Auth: ✘ not authenticated`，连接被丢弃，它的工具一个都不被提供给模型——主
session 的和 subagent 的都不——所以它的 instructions 也一样。connect worker 在
下列情况扣下它：

- 条目点名了它自己的 OAuth client（`oauth.clientId`）——用户主动要的登录，
  不管服务器放行什么；或者
- `oauth::protected_without_registration` 这么说：服务器发布了受保护资源
  元数据（RFC 9728，一份 `resource` 指向自己的文档），其中点名的授权服务器，
  其元数据不提供 `registration_endpoint`。

第二项检查在**握手旁边**自己的线程上跑，所以一个不发布任何元数据的服务器——
DeepWiki、Cloudflare 的文档站——连接不会更慢；每份文档给五秒
（`PROBE_TIMEOUT`），而读不到的文档永远不会扣下一个已经连上的服务器：只有正面
证据才会。

**为什么不干脆用「发布了 OAuth 元数据」。** 那是最初的想法，而它是错的：
Hugging Face（`huggingface.co/mcp`）、Context7（`mcp.context7.com/mcp`）和 Exa
（`mcp.exa.ai/mcp`）发布同样的元数据，却不带 token 就服务调用——在它们那里
登录是可选的——所以那条规则会切掉三台能用的服务器。它们的 provider 会注册
应用；Google 的不会。注册检查是启发式，不是保证：一台登录可选、但 provider
不注册应用的服务器也会被扣下。
`live_google_servers_wait_for_sign_in_while_open_ones_connect`
（`tests/live_mcp.rs`，默认 ignore）对着真实服务器检查两边。

在一个没配 client 的被扣服务器上按 **Authenticate**，以既有的解释收尾：provider
自己不注册 client，所以要么以 `oauth.clientId` 加一个，要么用
`zeroth mcp add --client-id … --client-secret`。用这样的 client 登录是
**实验性**的——它跟随 Google 发布的元数据（带尾斜杠的 issuer、元数据 URL 点名
的是工具而非服务器的那个 401、能拿到 refresh token 的那些参数）但还没有端到端
试过。更完整的支持是未来的工作：Authenticate 页上的引导式 client 设置、为提供
它们的 provider（Devin 的）支持 Client ID Metadata Document，以及一条验证过的
Google 登录。

`src/llm/mcp/`（边界，用进程内 fixture 封闭地验证——一个脚本化的 `sh` stdio
服务器、`std::net::TcpListener` 的 HTTP 服务器）：

- `transport` —— 三种 transport，藏在一个 `request`/`notify` 面之后：
  - **stdio**：配置的命令在启动时**与 TUI 的终端脱钩**——`exec {command line}`，
    每个词按 shell 规则加引号，走 `subprocess` 的分级尝试（先是 `setsid`，
    再是这个二进制自己的 helper 重执行，`docs/tools/tty-detach.md`），于是
    服务器领着属于自己的 session 和进程组，没有控制终端：一个跑 `sudo`、等
    SSH passphrase 提示或 git 凭据提示的服务器，会在 `/dev/tty` 上快速失败，
    而不是读走本该给 TUI 的按键。不存在的命令在启动前就被按名字拒绝（否则
    shell 会把它变成事后的一个退出码）。Env 合并到 session 的环境之上，去掉
    herdr 的 pane id——一台服务器可能是个 agent，它的 herdr hook 会接管 TUI
    的 pane（`docs/architecture/herdr.md`）。写入走一条**写线程**——一个不再读
    自己 stdin 的服务器（忙在一个同步工具里）只让一次调用输掉它的截止时间，
    而以前一个大于管道的请求会把回合的线程永久堵在 `write_all` 里——读线程把
    stdout 的每一行变成消息，有损解码（一行 Latin-1 以前就会结束整个
    session），上限 16 MiB。stderr 按块排空，从不按行——一条用 `\r` 重绘的
    进度条永远不结束一行，而停下来的读线程会让服务器死于 `EPIPE`——排进 4 KiB
    的尾部，失败的 connect 把它折进自己的消息里，而且是在失败*之后*读（以前
    在 spawn 时读，永远是空的）。请求是换行分隔的 JSON，答复按 id 配对；对一个
    已经放弃的请求的迟到答复被丢弃，如规范所说，而不是永久留着。一个被放弃的
    请求——Esc、它自己的截止时间——会在**服务器上被取消**，由
    `notifications/cancelled` 点名它（stdio 上是 MUST，因为那里没有流可关；
    `initialize` 永不如此），而一个已经取消的回合什么都不发。legacy 服务器
    自己的请求（`ping`）在 stdin 上回答；现代客户端 MUST NOT 写响应，它也不写。
    停服务器遵循规范：关 stdin，给它时间自己退出，`SIGTERM` 给它的进程组，
    然后 `SIGKILL`——它在组里留下的一切（`npx` 真正的服务器、一个 `sleep &`）
    随它一起去。session 中途被丢弃的 transport（一次重连、一次禁用）在丢弃线程
    之外停掉，每步 2 s；退出则一次停掉所有服务器，每步 300 ms、整体一个上限
    （`McpManager::shutdown`），于是一次退出等一个守规矩的服务器退出，绝不等
    一个卡死的。
  - **streamable HTTP**：每条消息一次 POST（`Accept: application/json,
    text/event-stream`），响应要么是 JSON body，要么是一条 SSE 流，排空到该
    请求的 id 被答复为止。**Legacy 模式**在 initialize 时捕获
    `Mcp-Session-Id` 响应头，此后回显，落定之后再加上
    `MCP-Protocol-Version` 头；**现代模式**无状态——每个请求里的 `_meta` 块，
    版本头取自第一次探测（它 MUST 与 `_meta` 一致），外加每个请求上的
    `Mcp-Method`、`tools/call` 上的 `Mcp-Name`（对不适合当 header 的名字用
    `header_value` 的哨兵编码）以及工具的 `Mcp-Param-{Name}` 镜像，且没有
    session。connect 时的工具清单（`client::list_tools`，两个时代共用）拒绝一个
    `x-mcp-header` 注解违反规则的——和其他一样是个 `ExcludedTool`，所以只让
    它自己付出代价——并把其余工具的声明装到 transport 上；一次仍被服务器以
    `400 HeaderMismatch`（`-32020`）拒绝的调用会重新列出工具、记下新清单，并
    重试一次，规范里的 SHOULD（通常的起因是 schema 变了）。一个 4xx 若 body
    能解析成 JSON-RPC 错误——它的 id 或 `id: null`，即 SDK 的 transport 层
    形状——就作为那个错误浮现（现代服务器把版本/header 问题答成 `400` + 一个
    现代错误 body）；**只有 400、404 和 405** 且没有此种 body 才是 SSE
    fallback 所依据的那种拒绝（`TransportError::HttpRefused`——规范里的那三个），
    而一个 403、一个 429 或一个没被跟随的重定向是服务器自己的回答，就照实报，
    而**一个挑战里写着 `insufficient_scope` 的 403** 是认证状态，和 401 一样
    （`NeedsAuth`，挑战留作 step-up 用）。通知的 2xx 就是它的接受，不管 body
    是什么，因为 body 从不被读——一个把通知扣着不回整的服务器，不该拖住握手。

    每个 POST 都跑在一个 worker 上，**以它自己那通请求的预算作 HTTP 超时**
    （`RequestBuilder::timeout`，在 reqwest 的阻塞客户端里它限定的是等 header
    *和*每一次 body 读取）。池化客户端那个 60 s 的停滞守卫以前就是实际限制，
    于是一个耗时 61–120 s 的工具在 HTTP 上失败，而同一个工具在 stdio 上能用，
    调高 `ZEROTH_MCP_TOOL_TIMEOUT_MS` 也毫无作用。一次放弃的等待（Esc、
    截止时间）把这通 POST 标记为放弃，worker 在它的下一行丢掉响应——关掉响应流
    *就是* streamable HTTP 的取消。每条消息上限 16 MiB（`MAX_MESSAGE_BYTES`，
    Claude Code 的上限），以有损 UTF-8 读取。客户端**只在请求的 origin 之内**
    跟随重定向（`llm::mcp_http_client`，十跳）：每个 MCP 请求都带着配置里的
    header，而 reqwest——跨 host 时只剥掉 `Authorization` 和 cookie——否则会把
    它们交给服务器把请求弹到的地方。

    在响应流上，**legacy** 服务器可能发自己的请求（多为 `ping`）并在自己的结果
    之前等答复：立刻回答——`ping` 给 `{}`，其他给 `-32601`
    （`mcp::server_request_answer`）——用一通自己的 POST，连 session id 一起发；
    不回答的话，这次调用以前会干等到自己的整个截止时间。一条**在答复之前就
    结束**的现代流会丢掉该请求——2026-07-28 移除了续传，并说客户端 MUST 用新的
    id 重新发起——所以它被重发一次。session id 只在 legacy 模式下捕获：一个
    现代答复无论如何点出了一个，也绝不回显。**一个带着 session id 的请求收到
    404**，意味着那个 session 没了（一次重启、一次空闲超时）：规范的答案是
    在落定的修订版上重新 `initialize`、重复一次 `notifications/initialized`、
    并重放该请求——于是回收自己 session 的服务器只花掉一个往返，而不是整条
    连接。重新握手自身的失败原样返回，因为掩盖它会把底下的 401 藏起来，而那
    正是 token 刷新所依仗的信号。只在确实*发过* session id 时才这样做，因为
    无 session 的现代服务器会用 404 回答一个未知**方法**。
  - **legacy HTTP+SSE**：GET 在自己的线程上打开事件流，第一个 `endpoint`
    事件点名 POST 目标（相对 base URL 解析——而**当它点名另一个 origin**、
    host、port 或 scheme 时被拒绝：每个 POST 都带 bearer 和配置的 header，所以
    一条指向别处、或把 `https` 降级成 `http` 的流会把它们交出去；参考 SDK
    做同样的拒绝），请求在 worker 线程上按同样的截止时间 POST 到那里、像其他
    每一种等待一样轮询取消（它以前会堵住回合的线程），答复在流上到达。服务器
    在流上发来的 `ping` 用一次 POST 回答。401 或 step-up 的 403 保留它的
    `WWW-Authenticate` 挑战，POST 一侧以前会把它丢掉。transport 被丢弃后，
    流线程在它的下一行停下——一条 keep-alive 注释就是一行——而它以前会一直
    扣着连接、也扣着自己。开头的字节序标记被剥掉（SSE 规范的规则；留着它，
    `endpoint` 事件就永远到不了）。
  三者都在等待时按 20 ms 的节奏轮询回合的 `CancelToken`（hooks-runner 的
  契约——卡住的服务器不该吃掉 Esc），并执行配置的截止时间；通知也取调用方的
  截止时间和取消，而它以前会在一张没人能取消的令牌上最多等 60 s。
- `client` —— 每服务器的 connect 序列，在**整个序列一个截止时间**之下（探测、
  握手、`tools/list` 的每一页和一次 SSE fallback 都从它里取——以前每个请求
  重启自己的时钟，于是一个慢而多页的服务器就花掉 页数 × 预算），**先做时代
  探测**（2026-07-28 规范自己的配方）：在现代版本下一个 `server/discover`
  探测（connect 预算里切出来的一块——一个*什么都不*答的服务器仍然给握手留下
  了余量）。发现出结果 = 现代服务器：解析它的身份，不 initialize、不发
  initialized 通知，直接进 `tools/list`。一个**现代错误**（`-32020`/
  `-32021`/`-32022`——一份允许清单，绝不是某个具体码：legacy 服务器会用实现
  自定义的错误回答 initialize 之前的未知请求，通常是 `-32601`）是「现代但
  做不了我们的修订版」的服务器——一个点名了我们会说的 legacy 修订版的
  `-32022`，退回到 `initialize` 并恰好提议那个修订版；没有共同的，就带着两份
  清单在消息里失败。**别的任何情况**——一个 legacy 错误码、一次 HTTP 拒绝、
  一个死掉的探测——是 legacy 服务器：`set_legacy()` → `initialize`（提议
  `2025-11-25`，最新的握手修订版）→ `notifications/initialized` →
  `tools/list`（排空 cursor——空字符串 cursor 会继续列举，如 2026-07-28
  所要求，而排空在它已经发过的**任何** cursor 上停下，不只是紧邻的重复，或者
  在一个超过 64 KiB 的 cursor 上停下；跨页重复的名字只列一次），落定的修订版
  在之后的请求里回显。**落定的修订版永远是服务器自己的回答**：legacy 侧是
  `initialize` 的 `protocolVersion`（只有当一个 `protocolVersion` 之前的
  服务器什么都不点名时，才由我们的提议顶上），现代侧则是发现结果
  `supportedVersions` 与我们共有、且最新的那个修订版——一份不点名我们能说的
  任何修订版的清单，会把它交给握手，用的就是它*确实*点名的那个，而这正是
  询问的意义。上一次 connect 的任何事情都不被参考。一份只有 `url` 的配置，
  若 `initialize` 被拒（`Handshake::Refused`），会去试 legacy transport，当它
  也失败时，消息里保留**两份**回答，streamable 的那份在前——通常才是解释得清
  的那个。任何位置的 401（或者 http→sse fallback 都因它失败）都解析为
  **NeedsAuth**，带上 `WWW-Authenticate` 细节供 OAuth 发现用——认证压过时代，
  探测在已认证的重连上重跑。

  **这个判定从不被记住。** 规范允许持久化它（在服务器进程或 origin 的生命期内
  `SHOULD` 缓存，跨重启 `MAY` 持久化），而 `{config_home}/mcp-era.json` 以前
  就这么做，以服务器的 `target()` 为键。它是错的，而它错的方式正是这一段存在
  的全部理由：**双时代服务器会愉快地应答 legacy 握手。** 于是一个被记住的
  `legacy` 判定从不失败、从不重探、也从不自我纠正——用户连上的是说
  `2026-07-28` 的服务器，而 `Protocol:` 行永久读作 `2025-11-25`。（这个缓存
  自己的逃生口——「换了时代的服务器只会失败一次并重探」——只对现代**独有**的
  服务器生效；真正获得现代支持的那些服务器保留着它们的握手，而这恰恰就是它们
  可升级的原因。）

  而且错误判定甚至不需要错*服务器*：fallback 分支是「任何不是现代错误的东西」，
  于是一次超时的探测、一次撞上代理抖动、或碰到一台还在启动收尾的服务器，写下
  `legacy` 的信心与收到 `-32601` 时一样足——而且是永久性的，来自一次不走运的
  初次接触。一个唯一的纠错机制是「它刚刚亲手弄成不可能的失败」的缓存，不是
  缓存。时代是关于服务器*当下*的事实，所以每次 connect 都重新推导，而那个文件
  在启动时被扫掉（`config::remove_retired_mcp_era_cache`），而不是留着一个陈旧
  的修订版、旁边配一个已经不存在的特性。

  当 `/mcp` 页上的修订版看着还是不对时，答案只差一条命令，且完全不涉及 TUI：
  `cargo run --example mcp_probe --
  {url|command}` 跑的就是这个 connect 序列，并打印服务器回了什么——身份、修订
  版、能力、工具。

  这要花的成本是每次 connect 一个往返——stdio 上亚毫秒，HTTP 上一个 RTT
  （对三台线上服务器实测：*整个* connect，含探测，分别是 0.43 s、0.69 s、
  0.93 s）——以及，面对一台**忽略**未知方法而不是报错（JSON-RPC 所禁止的）
  的服务器，握手之前那一段 `MODERN_PROBE_TIMEOUT`。这些花在一个后台 worker
  上，TUI 同时照跑，换来的是一个每次显示都为真的修订版。
- `oauth` —— RFC 形状的 authorization-code + PKCE 流程：受保护资源元数据
  （RFC 9728）→ 授权服务器元数据（RFC 8414，带 OIDC fallback 路径）→ 动态
  客户端注册（RFC 7591，若被提供）（`application_type: "native"`——2026-07-28
  的要求，其 OIDC 默认值 `web` 会拒掉 CLI 的 loopback 重定向）→ authorize
  URL（S256 challenge、`state`、RFC 8707 的 `resource`，以及
  `offline_access` **只有 AS 元数据提供它时**才折进 scope——SEP-2207 的可刷新
  授权，绝不盲目加）→ 一个 loopback 回调服务器*和*粘贴重定向 URL 的 fallback
  ——两者都接受，谁先到算谁 → token 交换。

  **响应在兑换任何东西之前就被检查**（`loopback::check_auth_response`，回调与
  粘贴同一套规则）：`state` 是**必需的**，且必须是本流程的——没有它的响应没法
  与伪造的区分（RFC 6749 §10.12），而它以前是被接受的——然后 RFC 9207 的
  `iss` 必须点名所发现的 issuer（逐字节，当元数据声明了该参数时缺失即拒），
  只有到这时内容才算数。任一检查失败的响应都被**挡回去，等待继续**：一个误入
  的请求、上一次尝试的重定向、或一个伪造的，都不能结束这次登录（浏览器得到一
  个 400 页面说明缘由；粘贴则在 auth 页上通过
  `AuthProgress::on_paste_rejected` 得到理由）。一个通过检查却带 `error=` 的
  ——用户在同意屏上拒绝——**以授权服务器自己的理由结束这次登录**，而它以前会
  让流程干等五分钟，等一个永远不会来的 code。

  **回调服务器**是 `llm::loopback`，每一次浏览器登录共用的那一个——本流程、
  ChatGPT 的和 Anthropic Console 的，它们各自带着同样的手搓循环和同样的弱点
  （`CallbackServer::bind` 在一串端口上，`0` 表示临时端口，外加一个路径；
  `wait_for_code` 依据流程的 `Expected` 状态与 issuer、它的 `Pages` 措辞、
  以及一个可选的粘贴通道）。它在同一端口上同时监听 `127.0.0.1` **和**
  `[::1]`，因为重定向点名的是 `localhost`，而浏览器可能把它解析到任一（机器
  没有 IPv6 loopback 时只有 IPv4）。每个连接在自己的线程上服务
  （`serve_connection`，最多 `MAX_CONNECTIONS` 个并发），带读超时，在回答之前
  读完整个请求头：旧循环内联读每个连接且没有超时，于是一次浏览器**预连接**
  ——开了 socket 却从不写——堵住了等待，Esc 和粘贴字段也跟着堵，直到浏览器
  放弃那个 socket；而在 macOS 上，已接受的 socket 会继承 listener 的非阻塞
  模式，于是仍在路上的请求被读成空的，它的 code 就丢了。`GET /callback` 之外
  的一律 404，而浏览器落到的那个页面说的是实际发生了什么——已收到、被拒绝
  （授权服务器自己的话单独排出）、或不是本次登录的——用的是项目自己的装扮：
  像素 mascot 压在衬线标题上，它的颜色与动效说明结果，一切来自外部的文字都被
  转义，什么都不去取（`docs/providers/callback-page.md`）。页面自带图标，所以
  浏览器不再向回调要 `/favicon.ico`。

  **客户端**遵循 SEP-2352：client 凭据与注册它的 issuer 一起保存
  （`StoredTokens::issuer`；在此之前存的授权按它的 token endpoint 匹配），且
  **永不出示给另一个授权服务器**——元数据现在点名了新 issuer 的服务器会重新
  注册（`reusable_client`）。被复用的 client 在它注册时的端口上登录
  （`StoredTokens::redirect_uri`），只要那个端口空闲，因为授权服务器可能精确
  匹配注册的重定向——RFC 8252 的 loopback 端口宽容只是 SHOULD——而端口不空闲
  时则**重新注册**；旧流程在这种情况下会送出一个服务器从未见过的重定向，把
  用户留在服务器的错误页上。注册时要求
  `token_endpoint_auth_method: "none"`，除非元数据列出的方法里没有它，那就
  要求它提供的 secret 方法（先是 `client_secret_basic`，RFC 6749 的默认）；
  以 `client_secret_basic` 注册的 client 用 HTTP Basic 认证，两半都不进 body
  （`client_authentication`，交换和每次刷新都一样）。被拒的注册或交换带着
  服务器自己的 `error` 与 `error_description`（`oauth_error_detail`），而不是
  一个光秃秃的状态码；一个不提供注册的服务器则说明如何添加预注册 client
  （见下文）。

  **预注册客户端。** 有些授权服务器完全不注册 client——Google 的，为它的 MCP
  服务器（`calendarmcp.googleapis.com` 之流）：用户在 provider 的控制台里建
  一个 OAuth client，MCP 客户端以它的身份登录。一份远程条目在 `oauth` 对象里
  点名它，也就是别的 MCP 客户端写的那种形状（`mcp::OAuthClientConfig`）：

  ```json
  "calendar": {
    "type": "http",
    "url": "https://calendarmcp.googleapis.com/mcp/v1",
    "oauth": { "clientId": "123.apps.googleusercontent.com",
               "callbackPort": 8976,
               "scopes": ["https://www.googleapis.com/auth/calendar"] }
  }
  ```

  `clientId` 是那个 client；`callbackPort` 是它注册的重定向 URI 所点名的端口
  （`http://localhost:{port}/callback`）——**精确**绑定，端口被占时登录失败并
  点名该端口，因为 provider 被告知的就是那一个（原生/桌面 client，按 RFC 8252
  可任取 loopback 端口，可以不写它）；`scopes`，字符串或列表，按写下的原样
  索要，替代发现所建议的。secret 最好别放进文件：
  `zeroth mcp add … --client-id … --client-secret` 把它留在 token store 的
  `clients` 段（`save_client_secret`——仅所有者，与登录分开，所以「Clear
  authentication」会留下它），而条目里的 `clientSecret`，像其余部分一样
  `${VAR}` 展开，也会被读——在同一道 provider-key 守卫之下，那道守卫也覆盖
  一个项目远程服务器的 `oauth` 值。`run_auth_flow`（作用于一个 `SignIn`：
  服务器、它的挑战、store、这个 client）优先使用配置的 client，先于任何被复用
  或注册的，且从不注册；把它当作无 secret 的公开 client 认证，或者把 secret
  放进 form body，除非服务器只接受 HTTP Basic（`configured_auth_method`）；
  并把 id 和 secret 与授权一起存下，于是刷新也出示它们。有自己需求的授权服务器
  通过这些需求体现在 authorize URL 上（`provider_authorize_params`）：Google
  只为 `access_type=offline` 发放 refresh token，且只在 `prompt=consent` 时
  再发一个，否则登录一小时内就失效。一个条目点名了 client 的服务器，即使它放
  过了匿名连接，也一样等那次登录（见上文*会向任何人列出工具的那些服务器*），
  而且用这种方式登录 Google 是**实验性**的：它跟随 Google 发布的元数据，但
  还没有端到端试过。复核的 `ServerParts` 把 `oauth` 的改动称作
  （`the OAuth client`）。

  **流程在该停的地方停。** cancel token 在发现、注册和浏览器之间被检查，所以
  发现期间的 Esc 之后绝不会再打开浏览器；没有 config home 可存授权时，流程在
  浏览器打开之前就说；而 PKCE verifier 与 `state` 取自系统的随机源，否则流程
  失败（`pkce_and_state`）——旧代码退回到全零字节、一个可猜的 state 和一个
  装饰性的 PKCE。token 存在 `{config_home}/mcp-auth.json` 里，以服务器 URL 为
  键——文件通过自己的临时文件以仅所有者（0600）创建，绝不先写再收窄，在保存
  创建目录时用 0700——并作为 `Authorization: Bearer …` 搭在每个 HTTP 请求上。
  一份存不下的授权会让登录失败，而不是「成功」进虚无（没有 config home、
  store 写不了），而一个存在却读不了的 store 是错误，绝不是「空」——重写它会
  丢掉其他每个服务器的授权。「Clear authentication」删除该条目，且只在删除
  真被写入时才这么说。

  **每份发现的文档在信任之前都被检查**——2026-07-28 授权规范的 MUST，以及
  参考 TypeScript SDK 对它们的读法：
  - **资源元数据**必须是*这台*服务器的（RFC 9728 §3.3，`resource_allowed`：
    同 origin，服务器的路径在文档的 `resource` 之下）；属于别的资源的会被跳过，
    因为它会把流程指向别人的授权服务器。它的 `resource` 就是流程所发的 RFC
    8707 指示符——authorize、token 交换**以及**每一次刷新（作为
    `StoredTokens::resource` 留在授权上，于是刷新所得 token 的 audience 永不
    漂移）；没有资源元数据时，用规范的服务器 URL（`canonical_resource`：
    scheme 和 host 小写，无 fragment，无孤零零的 `/`）。
  - **授权服务器元数据**恰好在规范说的地方去找
    （`authorization_server_candidates`）：对一个带路径的 issuer，先是 RFC
    8414 的路径插入，再是 OIDC 的路径插入，然后是 OIDC 的路径**追加**——那是
    Keycloak realm 和 Entra tenant 发布的地方，也是从未试过的——绝不用根，
    对带路径的 issuer 来说那是别人的文档；而且该文档的 `issuer` 必须就是它的
    URL 所据以构造的 issuer（RFC 8414 §3.3，`issuer_matches`，带 SDK 那一点
    尾斜杠宽容），否则跳过。被验证过的这个 issuer，才是 RFC 9207 的 `iss`
    随后要对照的。
  - 它的端点必须是 **HTTPS**（`endpoint_is_secure`；开发中的服务器可豁免
    loopback），且必须支持 **S256 的 PKCE**：元数据列出
    `code_challenge_methods_supported` 却没列 S256 的，被拒绝；而什么都没列的
    元数据则像 SDK 那样放行——规范的「MUST refuse」会把支持 PKCE、只是不说
    的服务器排除在外（`pkce_problem`）。
  - `WWW-Authenticate` 挑战按 RFC 9110 的 auth-params 解析，参数**按名字**
    匹配（`challenge_param`）：旧的子串搜索在 `insufficient_scope` 里找到了
    `scope`，于是规范自己给出的那例 step-up 挑战，携带的 scope 是空的。
  - 一次重新授权索要的是先前已授予的 scope 与服务器现在想要的 scope 的**并集**
    （`scope_union`，step-up 规则）——`403 insufficient_scope` 带着它的挑战
    以 `NeedsAuth` 到达，在 `/mcp` 上按 Authenticate 就恰好索要那一份。

  有两件事流程必须做对，否则授权一出生就不可续：**`offline_access`** 才是真正
  换来 refresh token 的东西，而 SEP-2207 刻意把它挡在*受保护资源*的元数据之外
  （「refresh token 不是资源的需求」），所以它只能来自授权服务器自己的目录
  ——Vercel 恰好是这个形状（resource 是 `openid`，AS 是 `offline_access`），
  没有这个并集，它的授权就没有东西可续。而 OIDC 只有在同意屏真的显示时才
  *放出*那个 refresh token，所以只要 scope 索要它，authorize URL 就带
  **`prompt=consent`**；一次静默的重新授权只回一个 access token，与从不索要
  是同一个死胡同。scope 是资源的（或者，在它之前，**401 挑战自己的 `scope`**，
  规范把它定为被拒操作的权威）加上 AS 公布时的 `offline_access`——**绝不是**
  AS 的整个目录，严格的 provider 会用 `invalid_scope` 回答它，也绝不是光秃秃的
  `offline_access`，那不是一个资源请求。

  **token 刷新 —— 为什么一个 session 从不问两次**（`refresh_grant` 及其同伴；
  参考客户端自己的那类 bug，这里按其 v2.1.206/211 的方式修好）：
  - **主动**：一份落在 300 秒过期偏移之内的已有授权——或者，对一个活不过十分
    钟的 token，落在它生命周期的后半段（`issued_at` 与 `expires_at` 并排保存；
    一个五分钟的 token 否则刚铸出来就陈旧，每个请求都是续期，每次续期都是一次
    轮换）——在它上线*之前*刷新——在 connect 时
    （`connect_bearer`）以及每次工具调用之前（`refresh_if_stale` →
    `Transport::set_bearer`，session 中途的重装接缝）——于是服务器在常规路径上
    从来看不到一个死 token。
  - **被动**：无论如何撞上一个 401（吊销、时钟偏移、一份过期时间未知的授权）
    得到**一次强制刷新和一次重试**——在 connect 时和每次调用时——才有人请用户
    重新认证。
  - **分类就是契约**（`RefreshFailure`、`refresh_failure_kind`）：只有 AS 自己
    在 400/401 上点名**授权本身**的结构化裁决——`invalid_grant`、
    `invalid_client`、`unauthorized_client`、`unsupported_grant_type`、
    `invalid_scope`——才是**永久**的：死掉的授权被*清掉*（留着它，会把失败带进
    之后的每一个请求），服务器如实读作 `needs authentication`。其余一切都是
    **暂时**的——网络失败、5xx、429、读不了的 body，以及那些对授权什么都没说的
    错误（`invalid_request`、AS 自己的 `temporarily_unavailable`、一个谁都没
    听说过的错误码），它们以前会清掉一份可能完好无损的授权：授权原样保留，
    服务器读作 `✘ failed (token refresh failed: …)`，它的 `Reconnect` 去重试
    ——绝不是 needs-auth，那会把用户推进一个为一次抖动而丢掉可用 refresh token
    的处境。
  - **轮换**：返回的 `refresh_token` 替换存下的那个（RFC 6749 §6 的 MUST——
    AS 可能在轮换时吊销旧的）；没有返回则保留旧的，绝不用空覆盖一个活 token。
    刷新请求带 `grant_type`/`refresh_token`/`client_id`
    （+ 机密 client 的 `client_secret`）和 RFC 8707 的 `resource`——**不带
    scope**（它只会收窄授权），也不带 PKCE（只用于 authorization-code 流程）。
    store 在每次刷新前重读，于是另一个进程更新的授权会被用上，而不是烧掉一个
    已轮换的 token——结果**只写回这次刷新所出示的那份授权之上**
    （`settle_refresh`，在 refresh token 上的一次 compare-and-swap）：请求在途
    时被「Clear authentication」删掉的授权保持删掉（clear 获胜），而期间某次
    登录存下的更新授权保留下来，也是调用方拿到的，于是一次永久拒绝永远清不掉
    它没有拒绝过的那份授权。
  - **单飞，按服务器。** 把 store 的*写入*串行化，并不能阻止两个线程把同一个
    refresh token 送到**网络**上，而那是更贵的竞态：OAuth 2.1 §4.3.1 要求
    轮换服务器检测重用并吊销**整个家族**，所以第二次出示不只是失败——它毁掉
    授权，只有浏览器能把它找回来。两个 subagent 调同一个服务器共用 manager，
    所以这在日常使用中可达（实测：在锁之前，8 次并发刷新中有 7 次被判为重放）。
    每次刷新取它服务器的锁，然后在锁下**重读 store**：如果它认定不可用的 access
    token 已经不再是存着的那个，说明另一个线程已经轮换了授权，就返回它的结果
    而不是一次重放。按服务器而不是全局，于是一个慢的授权服务器堵不住不相干的
    另一个。而且**也按 session**：不同终端里的两个 session 共用一个 store，而
    当第二个出示者是另一个进程时，重放同样致命，所以续期还在整个 token 请求
    期间持有 `mcp-auth.locks/{digest}.lock` 上的独占 `flock`（drop 时释放，
    进程死掉时由内核释放，于是一次崩溃永远不会把它丢在无主状态）。第二个
    session 等它——最多到 `REFRESH_LOCK_WAIT`，在第一个的 token 请求之后
    ——然后重读，取那个 session 铸出的授权。
  - **store 是每个服务器共用一个文件**，所以它的读-改-写在一个进程级锁**和**
    `mcp-auth.lock` 上的一个 `flock` 之后串行化（另一个 session 的写入会被等，
    但绝不等一个卡死的——丢掉一次轮换比一次竞态代价更高），并以**先写后改名**
    提交，临时名字对每次写入唯一，于是两个进程同时保存时永远不会写进对方的
    文件。没有锁的话，两个服务器同时续期（两个 subagent，或者循环重连另一个
    服务器时的一次工具调用）会交错 read/read/write/write，丢掉两个已轮换
    refresh token 中的一个；输家随后出示一个已用过的 token，合规的 AS 将其视为
    重放，并**吊销整个家族**作为回答——又是浏览器流程，而理由正是刷新存在的
    理由。没有改名的话，写到一半崩溃会留下一个被解析成*没有*服务器的截断文件，
    默默地把每一个服务器同时变成未认证。
  - 每次刷新都**跑在工具线程之外，按 20 ms 的节奏轮询回合的
    `CancelToken`**——transport 自己的契约，任何堵住回合线程的东西都必须遵守。
    内联跑的话，一个卡住的 token endpoint 会吃掉整个 20 s HTTP 超时期间的 Esc。
  - 401 的 **`WWW-Authenticate` 挑战被保留**在服务器状态上并交给流程。没有它，
    发现就是盲跑：挑战点名服务器发布在 well-known 路径之外的 `resource_metadata`
    URL，以及资源真正想要的 scope。
- `manager` —— session 拥有的注册表（`McpManager`，`BackgroundRegistry` 的
  `Arc<Mutex<…>>` 兄弟）：持有每个服务器的配置 + 活状态，**启动时在 worker
  线程上并发连接**它们（绝不堵住第一帧），暴露 `tool_specs()`（每个已连接、
  已启用服务器的工具的 Chat Completions 定义——其中 `parameters` 是
  `mcp::model_input_schema` 对服务器 `inputSchema` 修好的副本，见下文），执行
  `call_tool`（wire name → 服务器 + 工具，经 connect 时建的地图），并把每一次
  状态变化报到循环的 **MCP 事件通道**上（一个 `tokio` 无界 sender——
  文件搜索/剪贴板 worker 的模式），于是一台服务器连上、失败、或认证完成的
  那一刻 UI 就重绘。

  一次 connect 尝试**从零开始**：除了丢掉旧 transport（它顺带停掉 stdio 子
  进程），它还清掉身份、工具和被搁置的工具（`ServerState::drop_connection`，
  禁用一个、needs-auth 的拒绝和一次全新 connect 共用的那套拆解）。那些是
  *活*连接的事实——协议修订版、能力、详情页显示的工具清单——所以一次从未
  initialize 的重连，不得在一条说反话的红色 `✘ failed` 之下留下上一份的。模型
  反正两边都看不到（`tool_specs`/`fingerprint`/`has_tools` 都读
  `ServerState::offered`，除非 `Connected` 它是空的），所以这里改的只是服务器
  离线时页面说不说真话。

  `call_tool` 守着它所跑的那条连接。对同一个服务器的两次调用共用它，所以第二次
  **在轮询回合取消的同时等第一次**（`lock_unless_cancelled`）——它以前盲抢锁，
  于是一次用户已经按了 Esc 的调用，在锁空出来之后照样发了出去。一个 `401` 只
  拆掉**这次调用所走的那条连接**（它的 generation）：期间落地的重连是更新的
  一条。而一次发现自己的连接**已死**的调用——退出的 stdio 服务器、结束的
  legacy 流（`Transport::is_alive`）——把服务器标记为失败并带上理由，并立刻
  **重启它**（stdio 规范的「SHOULD restart」），告诉模型重试；它以前会坐成
  `✔ connected` 压着一条死管道，之后每次调用都失败，直到用户手工重连。最多每
  30 s 一次（`RESTART_COOLDOWN`）：那么快又死的服务器保持失败，留给手工
  Reconnect，绝不进入崩溃循环。**Clear authentication** 在授权没了之后重连
  一个已连接（或失败）的服务器，因为活连接上仍带着旧 bearer，服务器还在按一个
  页面说已清掉的登录工作。

## 工具集成

backend 遵循 skills 的模式：

- `LlmBackend::with_mcp(manager)`（受 `tools_enabled` 闸门控制）挂上 manager；
  `sync_tool_specs` 用 `manager.tool_specs()` 扩展工具集。subagent 也带着它
  （`SubagentConfig`）——侧边的 agent 查 wiki 的方式与 lead 完全相同。
- `spawn` 的执行闭包把 `mcp::is_mcp_tool(&call.name)` 路由到
  `manager.call_tool(call, &cancel)`，先于 `RealToolExecutor` 的兜底（即
  `is_task_tool`/`is_skill_tool` 的那个槽位）。
- 被提供的集合可能在 session 中途改变（服务器迟到连上、一次重连、一次 `/mcp`
  禁用）：边界在每次 MCP 事件和每个回合开始时，重新校验被提供工具的一个
  fingerprint——每个 wire name 旁边配它 description 与 schema 的摘要，于是一次
  只改了 schema 的重连（stdio 服务器自己的开发循环）也算数——（`Session::refresh_mcp`，`skills_attached` 的模式），并且只有它确实变了才重建 backend。
- **显示**：`llm::tools::display_name` 把 wire name 映射成
  `{server} - {tool} (MCP)` 抬头；`summarize_call` 对一次 MCP 调用返回**原始
  参数 JSON**。这个选择正是记录可重放的原因：`ToolCall.args` *就是*逐字的参数，
  所以 `context::context_messages` 能用模型自己的 JSON 重放原生的
  `tool_calls` 对（`reconstruct_arguments` 把它返回；`wire_tool_name` 反演
  显示名），`/resume` 保留它，而一个会做校验的 provider 从来看不到占位的
  `{}`。抬头所显示的**漂亮** `key: "value"` 形式在渲染时推导
  （`ui::tool` 的 MCP 分支），从不存储——**按模型自己的键序**，这也是
  `serde_json` 以 `preserve_order` 构建的原因：一份 schema 的 `repoName` 写在
  它所限定的那个长长的 `question` 之前，而把这二者按字母重排会把短参数埋进长
  参数下面。
- **权限**：MCP 调用先问，像 Claude Code。第四种 `PermissionKind::Mcp` 走既有
  的闸门，形状像 `bash` prompt——因为它回答的是同一个问题，*到底要跑的到底是
  什么？*：

  ```text
   Tool use

     Deepwiki - read_wiki_structure(repoName: "linuztx/flaredantic") (MCP)
     Get a list of documentation topics for a GitHub repository.

   Do you want to proceed?
   ❯ 1. Yes
     2. Yes, and don't ask again for Deepwiki - read_wiki_structure commands
        in ~/Codes/tests
     3. No

   Esc to cancel · Tab to amend
  ```

  body 就是**这个调用在它单元格里会读成的样子**——显示 label、同样
  `key: "value"` 形式写的参数、结尾 dim 的 ` (MCP)` 标记（好让视线落在工具
  上）——下面是**服务器自己对**这个工具的描述，dim（`McpManager::tool_description`
  → `approve_call` 的 `describe` 接缝 → `PermissionRequest::detail`，即 `bash`
  description 的槽位）。没有 `╌` 边框：它所替代的 pretty-printed JSON 花了五
  行去重新标点两个参数，还把选项挤向屏幕底部。选项 2 仍然把**精确的 wire name**
  记进精确命令允许清单（`mcp__deepwiki__read_wiki_structure`——像任何精确规则
  一样持久化在 `permissions.json` 里），而*说出来*的是显示 label 和这条规则
  所归属的项目（`App::project_dir`，页脚的 cwd——允许清单是按项目的）。有一处
  不对称要记住：一次拒绝的 `User rejected Deepwiki - … (MCP)` 文字会被**记入**
  单元格的输出（没有东西去解析它），所以一个从大写规则之前恢复的 session 显示
  它旧的小写文字，而它周围重新推导的抬头是大写的。`edit` 模式仍然会问（远程
  工具不是文件编辑），`auto` 模式把一次未被覆盖的调用发给 **auto mode
  classifier** 而不是用户——和它对一条 `bash` 命令所做的完全一样，也和参考
  实现把 MCP 调用喂给它的 auto-mode classifier 完全一样
  （`mcpToolInputToAutoClassifierInput`）：classifier 读到名为
  `{server} - {tool}` 的工具、服务器自己的描述，以及参数
  （`llm::classifier::classifier_request_prompt` 的 MCP 分支——光有 wire name
  会藏起风险所在），全都在回合被截断的任务上下文之下（用户请求 + 到目前的
  动作——`classifier::ClassifierContext`，`docs/state/permissions.md`），被
  允许的调用带着 `Allowed by auto mode classifier` 注记跑起来（在 Ctrl+O 里
  可见；安静的內联 `Called {server}` 行仍是一行——见下面的渲染一节），一次拒绝
  以红色带 classifier 的理由拒绝，而 classifier 的*失败*退回到这个 prompt
  （`docs/state/permissions.md`）——`master` 则像其他一切一样不问就执行。

## 渲染 —— 折叠的内联单元格 vs 展开的 transcript

一个 MCP 单元格靠它显示名的 ` (MCP)` 后缀被认出（纯的——`/resume` 之后依然
成立，不需要额外的记录字段）。内联时它刻意安静；Ctrl+O 承载完整故事：

- **运行中**：`● Calling {server}… (ctrl+o to expand)`——闪烁的灰点，提示在
  抬头上 dim——**别的什么都没有**。它以前带一行 `⎿ "{question}"` 窥视行；一个
  参数的一段片段，在宽度上换行，说的是抬头已经说过的话，还白占一行。
- **一个批次**：当**每一个**排队调用都是 MCP 时，整个批次是一个动作，显示为
  一个单元格——`● Calling Deepwiki 2 times…`、`● Calling Deepwiki,
  Context7 4 times…`（`mcp::names::batch_label`：按调用顺序的不重复服务器，
  多于一个时给总数）——在 live 条带里*也*在一个询问它某个调用的权限 prompt
  之上，而按调用渲染以前会用一屏相同的 `⎿ Waiting…` 单元格，堆在问题正需要的
  那些行上。计数是**批次的**：已解决的同伴从历史里读回（`ui::tool::
  mcp_batch_lines`），于是一个运行中批次的 label 不会把自己倒数下去。混合批次
  保留普通的逐单元格条带，`⎿ Waiting…` 行什么的都保留——并且逐单元格**提交**：
  它的 MCP 单元格从不被扣住（`held_run_len` 与 `tool_commit_lines` 里的 flush
  共用同一个谓词——*下一个*要跑的同批次调用按名字是 MCP 调用），因为一个被机制
  一端扣住、又被另一端 flush 的单元格会把它的 `Called {server}` 行打印两次
  （那就是被报上来的混合批次重复：bash 同伴的提交重新发出了已经在它自己
  ToolEnd 提交过的 deepwiki 行）。
- **已解决 ok**：无圆点的 dim 双色 `Called {server} (ctrl+o to expand)` 行
  ——落定的思考行的形状（`summary_lines`），因为留下的是一条关于本回合的事实，
  不是要读的输出。结果文本从不进入内联 scrollback；*这*正是单元格能折叠的原因。
  auto mode classifier 的 `⎿ Allowed by auto mode classifier` 出处行也留在
  外面（`ui::tool::tool_cell_lines` 恰好对这个单元格跳过这条注记）：在 auto
  模式下**每一个**服务器调用都带注记解决，于是这行把每个刻意只有一行的单元格
  翻倍成噪声——被报上来的 `Called vercel` + 注记对——而并行运行的聚合行本来
  也从没带过它；注记仍然收尾展开的 Ctrl+O 单元格并搭上 rollout，于是「没有人
  批准过这次调用」的记录，在完整故事活着的地方活着
  （`docs/state/permissions.md`）。一次**并行运行**解析成整个运行一条这样的行
  ——`Called Deepwiki 2 times (ctrl+o to expand)`——因为两行说
  `Called Deepwiki` 并不会比一行计数更好地描述这个批次。
- **失败**：响亮的通用红色单元格（完整抬头 + 错误窥视）——失败不许低语，而且
  它结束它所参与的那个运行（它之前的 ok 单元格作为它们自己的聚合行提交，然后
  是红色单元格）。
- **Ctrl+O**：不变，且按调用——完整的
  `● {server} - {tool} (MCP)({pretty args})` 抬头，压在 `⎿` 栏里的完整输出
  之上。transcript 是*发生过*的事；内联那行是它剩下的部分。那么宽的抬头用
  **悬挂缩进**换行，而不是把续行对齐到起始的 `(` 之下（`TOOL_HEADER_ALIGN_SHARE`
  ——超过三分之一宽度后，对齐花的比赚的多），于是参数拿到那一行，而不是排成
  参差的列——而当连第一个参数都放不下在名字旁边时（四十列在
  `● Deepwiki - ask_question (MCP)(` 之后只剩八列），它**整个**溢到续行，
  `(` 跟着名字，而不是硬断成 `(repoName` / `: "…"`（`docs/tools/tools.md`）。

**一行怎么可以两者都是**：聚合是*历史*的属性，不是 live 缓冲区的属性，所以
scrollback 与尺寸变化的重绘从同一处推导它（不变量 3）。一个已宣告批次的每次
调用都带着该批次的 id（`ToolCall::batch`，由 `App::
start_tool_batch` 打上，经 rollout 往返），而一次**并行 MCP 运行**是一段极大
的连续历史单元格，它们都是 MCP、都已 ok 解决、且共用一个 id
（`ui::tool::mcp_run`）——id 正是让两次只是挨在一起的*顺序*调用无法声称它们
并行跑过的原因。`conversation_lines` 在重绘时折叠每一段这样的运行；
`ui::tool_commit_lines`——每一次解决都经由它提交的那唯一一个构造器——在它的
批次下一个调用即将开始时**扣住**一个调用的行，然后在该运行结束时写出它那唯一
的一行。无论什么结束了这个运行，都会 flush 它：它的最后一个调用、中途的一次
失败、一次中断。（一段被扣住的运行也扣住后台完成的沉降，于是一个通知永远不会
落在其中。）subagent 的调用不带批次 id：它的 session 视图每次调用保留一个
单元格。

## `/mcp` manager

第六个替换 composer 的 picker（`app::McpMenu`、`ui::mcp_view`），而且刻意是
**hooks 菜单的孪生**而不是一个新形状：没有文本输入——所以菜单升起时边框里
**没有硬件光标**（权限 prompt 的规则，`ui::cursor_visible`：菜单没有东西给
光标指，而 kitty 光标动画会对着它选中的任何位置闪——被报上来的、底边线下面的
那处 artifact），取而代之，*座位*跟着高亮的 `❯` 行走（`ui::layout` 的标记
扫描——而在一个没有 `❯` 可跟的页上，空列表或工具详情，它就停在收尾提示刚刚
之外，像 Ctrl+O/Ctrl+D/`/resume` 那些 overlay 一样（`overlay_cursor_seat`
的规则），绝不落在它下面的框线上：底边线的远角，正是一个未配置的 `/mcp` 以前
每次打开都把 kitty 光标动画送过去的地方，`docs/ui/view-flow.md`；Auth 页的
`URL >` 字段是要输入的，所以它的光标会回来——amend 字段的例外）——↑/↓（两端
回绕）/数字/Enter/Esc，一个以选中项为中心的窗口，配上
`↑ N more above` / `↓ N more below` 溢出标记，回合中途也能用（条带停在它上面）。
各页：

1. **服务器列表** —— `Manage MCP servers` + `{n} servers`，行按 dim 的 scope
   标题分组，标题点名配置文件
   （`Project MCPs ({root}/.mcp.json)`、`User MCPs (~/.zeroth/mcp.json)`），
   每行 `{name} · {glyph} {status}`（`✔ connected · 3 tools`、
   `△ needs authentication`、`◯ disabled`、`✘ failed`）。` · ` 分隔符是
   **装饰，不是状态**，所以它在每种状态下都保持 dim（`MCP_ROW_SEPARATOR`），
   只有字形携带状态色——把状态色带着字形一路涂下去，会让一个已连接行的第一个
   `·` 变绿，而它工具计数前面的那个仍是 dim，同一行上同一个标记两种颜色。空
   列表点名两个文件路径——「我的服务器怎么不在这儿？」是它唯一的问题。
2. **服务器详情** —— 状态所给出的那些事实行：`Status:`、`Auth:`（只在认证有
   意义时——上面的 `auth_state` 规则）、`Protocol:`（时代探测落定的修订版
   ——现代服务器上是 `2026-07-28`，legacy 服务器上是 `initialize` 协商出的
   任何值；在服务器 initialize 之前隐藏）、URL 或命令、配置位置（已连接时
   再加上能力和工具数），然后是动作：已连接 → `View tools` /
   `Re-authenticate`（存在 OAuth token 时）/ `Clear authentication` /
   `Reconnect` / `Disable`；needs-auth → `Authenticate` / `Disable`；
   失败 → `Re-authenticate` + `Clear authentication`（存在 token 时）/
   `Reconnect` / `Disable`；连接中 → `Reconnect` / `Disable`；disabled →
   `Enable`；off（本次 session 关闭 MCP，见上）→ `Disable`。当一份新快照在
   用户脚下改变了列表时，**选中项跟着它的动作走**，而不是它的下标——一次
   Reconnect 让服务器变成 `connecting…`，其列表只有 `[Disable]`，于是第二次
   Enter 禁用了服务器并把那保存下来——而一个连接中的服务器的第一个动作是无害
   的那个。远程服务器的 `URL:` 行是指向它地址的链接，每个换行的行都打开整个
   URL，而失败服务器 `Error:` 理由里的 URL 也是链接（`docs/ui/links.md`）。
3. **工具列表** —— `Tools for {server}` 压在一串带编号的工具名之上。
4. **工具详情** —— 工具名 + 服务器，`Tool name:` / `Full name:`（模型真正
   调用的 wire `mcp__{server}__{tool}`）、描述，以及据 `input_schema` 推导的
   `Parameters:` 清单（`● {name} (required): {type} - {description}`）。
5. **认证** —— `Authenticating with {server}…`：流程打开浏览器（macOS 上是
   `open`，别处是 `xdg-open`——Linux 上绝不用 `open`，那是 `openvt`——
   launcher 在自己的线程上被回收），显示 authorize URL（`c` 经 `/copy` 剪贴板
   接缝复制它），并提供 `URL >` 粘贴重定向的 fallback（一个单行字段；页面升起
   时粘贴路由到它）；loopback 回调从任一侧解析它。流程挡回去的一次粘贴——
   不是重定向，或属于另一次尝试——以 `McpEvent::AuthPasteRejected` 回来，
   理由以红字在其下，它接受另一次粘贴（`App::reject_mcp_auth_paste`；编辑会
   清掉它）。

   **authorize URL 是一个真正的超链接**，就像 `/login` 登录页的那个一样
   （`docs/ui/links.md`）。它比任何一行都宽得多——PKCE challenge、state、编码
   过的 redirect 和 resource——所以它总是硬断行，而它以前被印成普通的行：终端
   自己的 URL 探测读的是行文本，于是一次点击打开第一行截断后的 URL，而它下面
   的那些行根本不是链接。现在每一行都把整个 URL 作为它的 OSC 8 目标，**逐字**
   （`model_url_rows`）：点击打开的东西与 `c` 复制的、交给浏览器的完全一致，
   不管 URL 以什么字符结尾。它保留自己的强调色并多一条下划线；一次被拒粘贴的
   理由里的 URL 也以同样方式成链。`smoke.sh` Phase 149 在一台要求 OAuth 的
   stub 服务器上，从原始字节流里读出这个链接。Esc 退出；结果落成一个 toast +
   服务器列表自己重排。

   **一次只做一次登录，而且每一次都知道自己是否还是那一次。** manager 把正在
   跑的流程存成一个带 id 的 `AuthFlow`；第二次 Authenticate 取消第一个并装上
   自己的。旧流程结束时只清自己那条：它以前会清掉装着的任何东西——*新*流程的
   粘贴通道和 cancel token，于是新页面的粘贴无处可去、它的 Esc 什么也停不下
   ——并把 `cancelled` 报成一次红色失败，把新页面关掉。以取消（Esc）或
   被取代结束的流程，除了一次快照刷新什么都不报，而
   `App::finish_mcp_auth` 只关掉流程已结束的那台服务器的页面。
   `McpSources::open_browser` 注入 launcher，于是一个测试不用它就能登录。

### 颜色去哪儿

这个孪生借了 `/hooks` 的边框，但**没借**它那平淡的白色标题，差别在于东西的
形状：`/hooks` 是一个从上往下读的浏览器，而这是四页深的*走*，所以大标题是
回答「我在哪儿？」的那一行——而且它必须是视线第一落之处。三条规则，都是
`ui/theme.rs` 里的 `MCP_*` 常量：

- **每一页的大标题都穿各页唯一那一套标题装扮**——粗体，浸在活动主题的横幅
  渐变里（`ui::header::page_title_line`，`docs/ui/theme.md`）——
  `Manage MCP servers`、`Deepwiki MCP Server`、`Tools for deepwiki`、
  `ask_question`、`Authenticating with deepwiki…`。
- **大标题会把名字大写开头**——`deepwiki` → `Deepwiki MCP Server`
  （`mcp::capitalize_server`，这个 helper 唯一的那一个拼法）。配置键按惯例
  是小写，而它一开始一个句子就像个错字——这条规则现在也覆盖**每个工具单元格
  表面**：`● Deepwiki - ask_question (MCP)(…)` 抬头、`Calling`/`Called` 行
  （经 `batch_label`），以及权限 prompt 的 label
  （`permission::mcp_display_label`）。凡是名字作为*身份*而非大标题的地方
  （`/mcp` 列表行、`Tools for …`、`Tool name:`/`Full name:` 的值、wire
  name、存下的 `ToolCall::name`、规则键、classifier 面向模型的 prompt），都
  保持逐字，因为那些是用户——或重放——必须与某个文件或某次工具调用对上号的
  字符串。
- **两个详情页上的每一个字段标签都亮，而值默认安静**
  （`mcp_detail_label_color()` / `mcp_detail_value_color()`）。标签是视线
  *上下扫*的那一列；值是一旦找到自己那行就停下的地方，所以一页白色的值没有
  可扫的依托。地址、路径、协议修订版、计数、一个工具的 wire name、一个参数的
  类型和 `(required)`——全都安静。

两个例外，每页一个，而两者是同一条规则：*本身就是答案的那个值保留亮光*。页面
**大标题**穿每一页共用的那套标题装扮，包括 `/hooks` 浏览器的：两个菜单都是深
好几层的走，而大标题就是回答「我在哪儿？」的那一行。

- 在**服务器页**（`mcp_detail_state_color()`）那是 `Status:`、`Auth:` 和
  `Capabilities:`——「它在工作吗，它能做什么？」——而 `Protocol:`、
  `URL:`/`Command:`、`Config location:` 和 `Tools:` 转为安静。那两行*状态*
  行画成双色：**字形保留它状态的颜色压在白色文字上**，因为字形是这行上在一台
  服务器失败时仍必须喊出来的那一样东西，而一行平白会把 `✘ failed` 洗成某种
  平静的东西。`Status:` 还丢掉了 `status_line()` 为列表追加的工具计数——三行
  之下的 `Tools:` 行已经说了它，而说了两遍的页面就有了一处重复。
- 在**工具页**上那是描述（`mcp_description_color()`），而它有自己的色调：
  **半白**，比宣告它的 label 低一档，又明显高于下面 schema 的散文。它是这一
  页上唯一一段*为*读者写的文字，而不是从 schema 推导来的，所以它不能读起来
  像样板——但全白又让它压过了组织这一页的那些 label。参数清单保留默认的分配，
  `● name` 作为 label 亮着（它确实是），而它引出的一切都是 dim。

工具页的字段行还把 `MCP_FIELD_COL` 的填充换成一个空格
（`MCP_TOOL_FIELD_GAP`）——它的两个 label 宽度相同，所以它们自己就对齐了，而
值就落在视线已经在的地方，而不是隔着一条 18 列的鸿沟。一个换行的参数悬挂在
自己名字之下（`MCP_PARAM_BULLET` / `MCP_PARAM_INDENT`，同样的宽度），其续行
全程 dim：只有真正携带名字的那一行被点亮。

两页，带注解：

```text
────────────────────────────────────────────────────────────────
  Context7 MCP Server              ← cyan, name capitalised

  Status:           ✔ connected    ← label white, ✔ green, words white
  Auth:             ✔ authenticated    (and no tool count here)
  Protocol:         2026-07-28     ← label white, value dim
  URL:              https://…      ← label white, value dim
  Config location:  ~/.alter-…     ← label white, value dim
  Capabilities:     prompts, res…  ← label white, value WHITE
  Tools:            2 tools        ← label white, value dim
────────────────────────────────────────────────────────────────
  ask_question                     ← cyan
  deepwiki                         ← dim

  Tool name: ask_question          ← label white, value dim, one space
  Full name: mcp__deepwiki__ask…   ← label white, value dim

  Description:                     ← white
  Ask any question about a GitHub  ← HALF white (the tool's own prose)
  repository…

  Parameters:                      ← white
    ● repoName (required): unknown ← "● repoName" white, the rest dim
      - GitHub repository or list… ← dim (a continuation carries no name)

  Esc to go back                   ← dim
────────────────────────────────────────────────────────────────
```

一个比终端还高的页面——一段真正的 `query-docs` 描述加上它的参数散文轻松就
是——以前在**底部**被裁掉，连提示和收尾框线都丢了，还看不出下面还有。它现在
**底部锚定**渲染，被跳过的顶部**流进终端的真实 scrollback**，就在区域的正上方，
于是整页靠终端自己的滚动来读，而交互的尾部原地不动；离开或关闭会 purge 重建，
所以没有陈旧页面文字残留（`docs/ui/view-flow.md`）。

操作以 `Action::McpOp(op)` 派发给 `tui::mcp::Session::apply_mcp_op`，它在 worker
线程上跑连接工作，并通过用户文件的读-改-写持久化禁用状态；每一次完成都报到
MCP 事件通道，它重新注入快照（`App::set_mcp_snapshot`）并排一帧——picker 永远
看的是活状态，而一行 pending 在它的线程上报之前显示 `… connecting`。

## 留在外面的东西（v1）

Resources、prompts、sampling、elicitation 和 roots 都不建模——工具面才是那个
特性——它们的现代替代品也一样，即 2026-07-28 多轮往返请求的 `inputRequests`：
这个客户端在每次请求里声明一个空的 `clientCapabilities`，所以合规的服务器只会
给它发只带 `requestState` 的 `input_required`（上面的重试），而一个无论如何都
索要输入的，得到一个可恢复的红色结果、点名它索要了什么，而不是一个卡住的回合。
响应缓存（`ttlMs`/`cacheScope` 能解析但没人缓存）和 CIMD 客户端注册（我们不是
一个拿 `https` client id 的托管客户端——已弃用但保留的 DCR 才是我们的注册路径）
一并留在外面。

**一个说自己的工具变了的服务器会被重新列举。** 一条
`notifications/tools/list_changed` 从 transport 读服务器的任何地方到达 manager
（`Transport::watch_changes`，每次 connect 注册一个 `ChangeSink`）：stdio 读
线程和 legacy SSE 流线程不管有没有调用在等，都能看到每条消息，而一条
streamable-HTTP 响应流会把随身携带的任何通知交出来。一个能力里承诺会变的服务器
（`tools.listChanged`、`ServerIdentity::tools_list_changed`）还会被主动去问，
因为现代的一个 MUST NOT 发送它没被要求的东西：stdio 上一条带
`toolsListChanged` 过滤的 `subscriptions/listen` 被写一次，从不等待（它最终的
答复是请求循环丢掉的迟到答复）；streamable HTTP 上一个后台线程把变更流开着
（`listen_for_changes`）——现代服务器上是 `subscriptions/listen` 的响应流，
早一代上是可选的 GET 流——在它结束时以逐渐变长的停顿重开，对一个不提供任何流
的服务器（400/401/403/404/405，或一个不是事件流的答复）或连续每次打开都失败的
服务器放弃，每次重开都从连接上读回最后那通请求留下的 bearer 和 session，并随
transport 一起停下。随后 manager 在 worker 上重新列举该服务器的工具
（`McpManager::relist`）——绝不在读线程上，也绝不在等一次持有该连接的调用时
压着 manager 锁——把一阵通知合并成一次列举，再加上一次为列举期间到达的那些，
列举失败则保留旧工具。新工具替换提供给模型的那个集合，集合变了时边界重新挂上
backend，改动抵达下一个请求。sink 弱持有 manager：transport 活在 manager 里。
服务器的 `instructions`——规范对服务器工具该怎么用的提示——搭在
`<system-reminder>` 的**最后一节**上（`mcp::instructions_section`，参考实现的
形状：`# MCP Server Instructions` 之上每个服务器一个 `## {server}` 段），在
边界处由 `sync_listings` 从 `McpManager::instructions` 构造，并在每次 MCP 事件
时重新渲染，于是一个在第一帧之后才连上的服务器，就在下一回合的上下文里。只有
一个已连接、且至少有一个工具被提供给模型的服务器才贡献（它的 instructions 讲
的是工具；**Tools** 关掉时一个都不提供，所以一个都不搭），按名字排序，好让
reminder 在重连之间保持不动，每个截在 `MAX_INSTRUCTIONS_CHARS`（4,096）。放在
最后，因为服务器来来去去，而只有变化之节后面的东西才重新渲染进 prompt 缓存。
那段文字是服务器的，所以它被关在自己的段里：每一行以标题的 `#` 开头的都被压
低两级——代码围栏里也一样，因为读原始文本的人分不清围栏里的 `# …` 和一个小节，
而 `#` 注释仍只是注释——一个没关的围栏会被关上，于没有服务器能开自己的小节，
或冒充另一个服务器的。subagent 的 briefing 带着它那种类型可以调用的那些服务器
的段（`briefing_for`，它的 `tools:` 允许清单），因为 lead 的 reminder 永远到
不了它崭新的上下文。`/context` 把这一节计作 **MCP instructions**，每个服务器
一段（`docs/architecture/context-usage.md`）。WebSocket transport 不提供：MCP
规范定义了 stdio 和 Streamable HTTP（以及本客户端也说的 legacy HTTP+SSE），而
Claude Code 的 `"type": "ws"` 是它自己的扩展，这里的条目会把它报成不受支持的
传输，而不是默默丢掉。dummy backend 不脚本任何 MCP 场景——manager 端到端都是
真 I/O；smoke 套件改在一个脚本化的（现代）stdio fixture 上驱动 `/mcp`。

## 测试

纯的那几半照常做单元测试（命名契约、参数美化器、`_meta`/discover/协商的形状、
刷新分类、每一种单元格/prompt/聚合渲染、提交与重绘的一致性），而边界对着进程内
fixture：一台现代 stdio 服务器（discover 优先）、一台 legacy 的（-32601 的
fallback）、一台严格的现代 HTTP 服务器，它*拒绝*缺少 2026-07-28 请求元数据的
请求（于是一次绿色的 connect 就证明我们发了它）、一台 `-32022` 协商服务器，
以及一对 OAuth——一个要求 `tok-{n}` 的 MCP endpoint 旁边配一个脚本化的 token
endpoint——驱动整套刷新矩阵（session 中途的 401 → 刷新 → 重试，服务器从未离开
Connected；调用前的主动刷新，见到**零**个 401；connect 时的刷新；
`invalid_grant` → 清掉授权 + needs-auth；503 → 保留授权 + `✘ failed`，绝不是
needs-auth）。那些证明不了的——一台真实服务器的工具、描述和并行批次出来是不是
对的样子——活在 **`tests/live_mcp.rs`** 里，像其他每一个 live 测试一样
`#[ignore]`：

```sh
# the public DeepWiki server only (no key needed)
cargo test --test live_mcp -- --ignored --nocapture live_deepwiki_tool_descriptions
# the whole round trip: a real model issuing a real parallel batch
OPENROUTER_API_KEY=sk-or-… cargo test --test live_mcp -- --ignored --nocapture
```

第二个像 `tui::stream` 那样把回合真实的 `StreamEvent` 折进 `App`，并对终端会
显示的那些行做断言：条带上那一个 `● Calling Deepwiki 2 times…` 单元格、唯一
提交的 `Called Deepwiki 2 times`，以及按调用、带模型自己参数顺序的 Ctrl+O 抬头。
