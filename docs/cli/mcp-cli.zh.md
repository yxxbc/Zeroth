# CLI MCP 安装 — `alter-zero mcp add/remove/list/get/add-json`

Claude Code 和 codex 都允许脚本在不打开应用的情况下安装一个 MCP 服务器；这就是
面向用户 config 文件的这个表面（`docs/tools/mcp.md`），像 `--continue`/`--resume`
一样**在 TUI 启动之前**被解析并应答（`docs/cli/cli.md`）：

```
$ alter-zero mcp add vercel --url https://mcp.vercel.com
Added http MCP server "vercel" (https://mcp.vercel.com)
File: /home/user/.alter-zero/mcp.json

$ alter-zero mcp add everything -e API_KEY=xxx -- npx -y @modelcontextprotocol/server-everything
Added stdio MCP server "everything" (npx -y @modelcontextprotocol/server-everything)
File: /home/user/.alter-zero/mcp.json

$ alter-zero mcp list
MCP servers in /home/user/.alter-zero/mcp.json:
  vercel: https://mcp.vercel.com (http)
  everything: npx -y @modelcontextprotocol/server-everything (stdio)
```

## 参考实现的做法（调研结论）

两个参考实现都被从头到尾读过；它们的语法在一个要紧的决策上不同。

- **codex** —— `codex mcp add <name> (--url <URL> | -- <command>...)`：传输方式
  由*你传了哪个 flag* 决定，由一个必需且互斥的参数组强制执行。`--env
  KEY=VALUE` 可以重复（仅 stdio —— 不过只有运行时检查才能抓到 `--url … --env`），
  尾随的命令被原样捕获（所以 `-- npx -y pkg --port 4000` 保留子进程自己的
  flag），而 `--` 是惯例而非必需。没有 `--header` flag，完全没有 SSE 传输。
  重名会被**静默覆盖**；`remove` 一个不存在的名字算*成功*。经由 `toml_edit`
  写 `~/.codex/config.toml`（注释得以保留）。
- **Claude Code** —— `claude mcp add [-t stdio|sse|http] <name>
  <commandOrUrl> [args...]`：一个位置参数同时充当命令和 URL，而传输方式
  **默认 stdio，从不推断** —— 所以 `claude mcp add name https://x` 会写下一个
  stdio 服务器、其*命令就是那个 URL*，从而"成功"（stderr 上有三行警告，config
  照样是坏的）。它自己的 help 示例也是坏的：可变参数的 `-e` 吞掉了服务器名
  （`mcp add -e K=V my-server -- npx …` 把 `npx` 解析成了名字）。好的方面：
  面向远程服务器的 `-H/--header "Name: Value"`、**重名即硬错误**（绝不覆盖）、
  用于粘贴 README 中确切条目的 `add-json <name> <json>`，以及名字校验
  （`[A-Za-z0-9_-]`，与 codex 强制的同一类字符）。

## 我们构建了什么

codex 的显式骨架（传输方式取决于你用的是哪种形式 —— 一个 URL flag 还是一条
尾随命令 —— 所以没有东西会被悄悄读错），再加上适合我们 config 格式的 Claude
Code 便利：`--transport` 用来点名 `sse`（codex 说不出它，而我们的
`McpServerConfig` 可以）、面向远程服务器的 `--header`、用于 README 片段的
`add-json`，以及重名错误。所有这些只针对**用户**文件
（`ALTER_ZERO_MCP_FILE`，否则 `{config_home}/mcp.json` —— 与 TUI 使用的同一套
解析）；项目文件与团队共享并等待启动审查，所以一个会写它们的 CLI 要么绕过了
审查，要么把它的服务器直接送回审查（`docs/state/config-review.md`）。

### 语法（纯函数，`src/cli.rs`）

```
alter-zero mcp add <name> --url <URL> [--transport http|sse] [-H|--header "Name: Value"]...
                   [--client-id <ID> [--client-secret] [--callback-port <N>]]
alter-zero mcp add <name> [--transport stdio] [-e|--env KEY=VALUE]... [--] <command> [args...]
alter-zero mcp add-json <name> <json> [--client-secret]
alter-zero mcp remove <name>
alter-zero mcp get <name>
alter-zero mcp list
```

- `mcp` 只在**第一个**参数的位置被识别；`parse` 把它剥下来，把其余交给
  `parse_mcp`，所以 session flag 的语法不受影响。`Cli` 增加一个变体，
  `Mcp(McpCli)`。
- **传输方式来自形式。** `--url` ⇒ 远程；一条尾随命令 ⇒ stdio。`--transport`
  随后可以*细化*远程的种类（`http` 钉住 streamable HTTP，`sse` 钉住旧式
  传输）；在没有匹配形式的情况下给出则是用法错误（带命令却给
  `--transport http`、带 `--url` 却给 `--transport stdio`）。**裸 `--url`
  （不带 `--transport`）映射到无类型的 `{"url": …}` 条目** ——
  `Http { sse_fallback: true }`，即规范里"先试 http 再退到 sse"的兼容配方 ——
  所以默认的 add 对两代服务器都有效。那种 fallback 形状正因如此，writer 绝不
  能为它写出 `"type"` 键（`docs/tools/mcp.md`）。
- **命令从其第一个 token 起被原样捕获。** 在 `<name>` 之后，flag 一直解析到
  第一个非 flag 参数或 `--`；从那里起一切逐字属于命令，所以
  `mcp add docs npx -y pkg --port 4000` 保留 `-y`/`--port` 作为子进程参数，
  无需分隔符（codex 的 `trailing_var_arg`；Claude Code 在那里会以
  `unknown option '-y'` 报错）。一个首词本身以 `-` 开头的命令需要那个 `--`。
- `-e/--env KEY=VALUE` 在**第一个** `=` 处切分（`A=b=c` 保留 `b=c`），拒绝缺少
  `=` 或空 key，保留空值，并且只适用于 stdio（`--env` 配 `--url` 是用法错误，
  而不是 codex 那种迟到的运行时错误）。`-H/--header "Name: Value"` 在第一个
  `:` 处切分，两侧都去空白，拒绝缺少 `:` 或空名字，并且只适用于远程。两者都是
  每个 flag 一个值（绝不使用可变参数 —— 正是那个陷阱弄坏了 Claude Code 自己的
  help 示例）。重复的键：后者胜出。
- **一个预注册的 OAuth 客户端** —— 用于这样一个服务器：其登录 provider 自己不
  注册客户端，例如 Google 的（`docs/tools/mcp.md` 的 *Pre-registered clients*
  一节；Google 的服务器尚不受支持，以此方式登录它们是实验性的，见 *Servers
  that list their tools to anyone*）—— 就是其他 MCP 客户端的 CLI 所记载的那些
  flag：`--client-id <ID>` 和 `--callback-port <N>` 写入条目的 `oauth` 对象
  （仅远程；配命令给出则是用法错误），而 `--client-secret` 是一个**开关**，
  绝不是值 —— `--client-secret=…` 会被拒绝，因为命令行上的 secret 会落进 shell
  历史和进程列表。`add-json` 也接受这个开关，通过条目自己的 `oauth.clientId`
  点名它的客户端（那里没有则用法错误）。边界在写出任何东西**之前**读取 secret，
  所以缺了它绝不会留下半添加的服务器：从 `MCP_CLIENT_SECRET` 读取（若已设置），
  否则当 stdin 是终端时在 stderr 上回显关闭地提示（rustix 的 `termios`，之后
  恢复），否则读 stdin 的一行 —— 并把它保存在仅 owner 可读的 token 存储的
  `clients` 段中、以服务器的 URL 为键（`llm::mcp::save_client_secret`），绝不
  写进 `mcp.json`。随后 `get` 会显示 `OAuth client:`、`Callback port:`、
  `Scopes:` 以及 secret **在哪里**（`kept with the tokens`，或者对写在其中的
  `clientSecret` 显示掩码 `***** (in the file)`），绝不显示它是什么。
- **名字**必须是非空的 `[A-Za-z0-9_-]+`（两个参考实现的规则，
  `mcp::validate_server_name`）。这正是 `mcp::normalize_name` 保留的字符类，
  所以 `/mcp` 列表、权限规则和 `mcp__{server}__{tool}` 的 wire 名都与键入的
  内容一致。
- `add-json <name> <json>` 把这个字符串喂给 config 解析器
  （`mcp::parse_server_entry` —— 每次读取文件所用的同一个 `parse_server`），
  所以 README 里的一段 `{"type":"http","url":…}` 能通过这一个 schema 往返；
  不存在第二个会漂移的解析器。
- `mcp` 参数中任何位置的 `-h/--help` 都打印 `mcp` help 页面（退出 0）——
  `cli::help(HelpPage::Mcp, style)`，主 `--help` 自己的渲染器
  （`docs/cli/cli.md`）：命令名作为标题、以 heading 样式呈现，描述，六行
  `Usage:` 形式，对齐的 `Options:` —— 除了在 `--` 之后，那里的 token 是命令
  自己的。裸 `alter-zero mcp` 是一个点出各子命令的用法错误。

### 写出方（纯函数，`src/mcp/config.rs`）

`record_server` / `remove_server` 是 `record_disabled` 的兄弟 —— 对整个文档做
字符串进、字符串出的读-改-写 —— 有两处刻意的不同：

- **无法解析的文件被拒绝，绝不被覆盖** —— 对一次 CLI 编辑而言，回退到 `{}`
  会丢掉每一个已声明的服务器（`/mcp` 开关的 `record_disabled` 现在也以同样的
  方式拒绝）。两个 writer 改为返回 `Err(McpWriteError::InvalidJson(reason))`
  （边界退出 1：`{path} is not valid JSON … refusing to rewrite it`）。空内容
  或仅空白的内容被视为全新的 `{}`（一个被 `touch` 过的文件，或边界读取不存在的
  文件），不是错误。
- **`remove_server` 使用 `Map::shift_remove`。** serde_json 以
  `preserve_order` 构建，其中普通的 `remove` 是一次 `swap_remove`，会把*最后*
  一个服务器移到被删除的位置 —— 删掉五个中的第一个会悄悄重排文件（以及渲染它
  的 `/mcp` 列表）。

`record_server` 在键重复时报 `Exists`（在原始 JSON 上检查，所以哪怕一个无法
解析的条目也不能被悄悄替换）；当名字不在其中时 `remove_server` 报 `Missing`。
其余一切 —— `projects` 下的禁用集合、未知键、其他服务器的条目 —— 都原样通过，
而一个条目序列化出来的样子与 `parse_server` 读它时完全一致：
`{"type":"stdio","command",…}` / `{"type":"http"|"sse","url",…}`，空的
`args`/`env`/`headers` 被省略，而 `sse_fallback` 的 Http 形状是一个**不带**
`type` 键的裸 `{"url": …}`。`parse_server_entry(render_server(config))` 对每种
形状都能往返。

### 边界（`src/tui/mcp_cli.rs`）

`resolve_cli` 匹配 `Cli::Mcp(cmd)` 并把它交给
`tui::mcp_cli::run(cmd) -> i32`，以返回的代码经由 `--help` 所用的同一个
`Err(code)` 通道退出 —— cooked 模式的 stdio、没有 tokio、没有终端，位于
`InlineViewport::init` 之前（`docs/cli/cli.md` 的顺序）。文件是
`config::mcp_user_file_path()`（`ALTER_ZERO_MCP_FILE`，否则
`{config_home}/mcp.json`）；为 `None` —— 没有 `HOME`、没有覆盖 —— 是硬退出 1，
与 TUI 那些尽力而为的 writer 不同，因为没有地方可以持久化。缺失的文件读作
`{}`；写入由 `llm::mcp::write_config_file` 完成 —— 首次使用时创建父目录
（仅 owner 可访问）、临时文件同步并重命名到位，使一次失败的写入留下完整的旧
文件、保留文件原有的权限（新文件则仅 owner 可访问）、并跟随软链接到它所指向的
文件 —— 它的错误会被呈现出来。`ALTER_ZERO_MCP=0` 不门控 CLI —— 它只是编辑
一个文件，什么都不启动；这个功能开关继续管着 TUI。

- `add`/`add-json`：经由 `record_server` 做读-改-写，然后两行 stdout ——
  `Added {stdio|http|sse} MCP server "{name}" ({target})` 与
  `File: {path}`。
- `remove`：经由 `remove_server` 做读-改-写 —— `Removed MCP server "{name}"`
  + 那一行 `File:`。
- `list`：对用户文件执行 `parse_mcp_file`，在点名路径的标题下按**文件顺序**
  为每个条目打印一行 `  {name}: {target} ({transport})`；空（或不存在）的文件
  打印 `No MCP servers in {path}` 以及
  `Add one with: {bin} mcp add <name> --url <url>` 提示；无法解析的条目是
  stderr 上的 `warning:` 行，而好的那些仍然会被列出。
- `get`：该条目的事实（`Type:`、`Command:`/`URL:`、`Args:`、`Env:`/
  `Headers:`，其中**值被掩码**为 `*****` —— header 常常携带 bearer token，
  codex 出于同样的原因也做掩码 —— 然后是 `File:`），以
  `Remove with: {bin} mcp remove {name}` 提示行收尾。

退出码遵循本仓的约定（`docs/cli/cli.md`）：**2** 用于语法错误（clap 形状的
`error: {message}` 位于 stderr 上的 `mcp` `Usage:` 块之上，
`cli::usage_error(HelpPage::Mcp, …)` —— 未知 flag、缺少名字、坏的
`--env`/`--header`/JSON、冲突的形式），**1** 用于解析失败（重名、未知名字、
无法解析的文件、没有 config home、一次失败的写入 —— 一行纯 stderr），
**0** 用于成功与 help。

## 与参考实现的已知差异

- **没有 `--scope`**：项目文件（`.mcp.json`、`.alter-zero/mcp.json`）在这里
  是只读的 —— 它们与团队共享、在运行之前先被审查，所以往它们里面安装属于一次
  编辑和启动审查，而不属于一次 CLI 写入。Claude Code 的 `local` 范围在我们
  的两范围模型里不存在。
- **`list`/`get` 读 config，它们不连接。** Claude Code 的 `mcp list` 会为健康
  检查启动每个 stdio 服务器（它自己的 help 就警告"only use this command in
  directories you trust"）；codex 则通过网络探测 OAuth 元数据。我们的是确定性
  且无副作用的 —— 连接状态活在 TUI 的 `/mcp` manager 里，它已经实时显示
  `✔ connected · 3 tools`。出于同样的原因，`list` 显示的是 CLI 所管理的那个
  文件（用户范围），而不是受信任门控的项目合并。
- **重名是错误**（Claude Code 的规则）：codex 的静默覆盖会因为一个笔误丢掉一个
  能用的条目。消息会点名 `mcp remove` 这条退路。
- **`remove` 一个不存在的名字会失败**（退出 1，又是 Claude Code 的规则）：
  codex 的"成功"会让一次脚本化的清理永远把某个名字拼错下去。

## 测试

- `cli`：整个 `mcp` 语法 —— 每个子命令的正常路径、原始命令捕获（保留子进程
  flag、以短横线开头的命令需要 `--`、`--` 之后没有 help）、`--env`/`--header`
  解析及其形式冲突、`--transport` 细化及其冲突、名字校验、随处可用的 help、
  裸 `mcp`/未知子命令的错误。
- `mcp::config`：`render_server`/`parse_server_entry` 对全部四种形状的往返
  （含无类型的 fallback）、`record_server`（全新文件、追加顺序、兄弟键保留、
  `Exists`、`InvalidJson`、空内容视为全新）、`remove_server`（**顺序保留** ——
  `shift_remove` 的保证 —— `Missing`、删除最后一条会丢掉 `mcpServers` 键而绝
  不丢掉整个文档）。
- `mcp::names`：`validate_server_name` 接受 wire 安全的字符类，拒绝其他任何
  东西并点名它。
- `tests/mcp_cli.rs`（`detached_exec.rs` 的模式 —— 经 `CARGO_BIN_EXE_zeroth`
  运行真实二进制，`ALTER_ZERO_MCP_FILE` 指向一个临时目录）：add stdio + http +
  sse + add-json、文件的精确 JSON、list 顺序、get 的掩码、remove 保留顺序、
  重复/缺失/无效文件的退出码、help，以及无效文件保持逐字节不变。
- `scripts/smoke.sh` Phase 84：CLI 把 Phase 80 的脚本化 stdio fixture 安装进
  一个临时用户文件，然后 TUI 对着同一个文件启动，`/mcp` 显示该服务器
  `✔ connected · 1 tool` —— 证明 CLI 写入的文件正是 session 读取的文件。
