# 在 hook 与项目 MCP server 跑起来之前审查它们

一个 hook 会自己在它的事件每次触发时跑一条 shell 命令，绕开权限 prompt。一个项目
的 MCP server 是被 session 启动的一个程序、或者被连上的一个 URL，理由只是
checkout 里的某个文件这么说了。两者都是没有 tool call 可供批准的代码，所以在用户
看过它们之前，哪一个都不会跑。

这一页取代 `/trust` 闸门（`docs/state/project-config.md` 保留项目层的发现与合并
规则）。`/trust` 按内容哈希、从一个用户必须先知道要打开才行的菜单里，把项目的配
置文件整体批准：它只覆盖那三个项目文件，从不覆盖用户自己的 `hooks.json`，而它唯
一的一个答案一次覆盖所有 hook 和 server。取代它的是一个**审查对话框**，session
在启动时自己打开它，只要有任何新的或变了的决定要做 —— 两个参考实现最后都落到这个
形状（Claude Code 问要启用项目的 `.mcp.json` 里哪些 server；codex 请用户信任每个
新的或变了的 hook）。

## 什么需要一次决定

| 什么 | 从哪里读 | 那个决定 | 记成什么 |
| ---- | --------------------- | ------------ | ----------- |
| 每个能跑的 hook | 用户的 `hooks.json`（`~/.zeroth/hooks.json`，或 `ZEROTH_HOOKS_FILE`）**和**项目的 `{root}/.zeroth/hooks.json` | 信任它，还是不 | hook 本身，按文件 |
| 每个项目 MCP server | `{root}/.zeroth/mcp.json`、`{root}/.mcp.json` | 启用它，还是不 | 每个 server 一个决定，按文件，钉在它的条目上 |

一个 hook「能跑」，是指运行器真的会跑它：一个带命令的 `command` handler、在这个
应用建模过的事件之下、没有 `if` 条件、也没有 `powershell` shell（
`hooks::HookHandler::runnable_command`）。其余的这些不管信任怎么说都会带一句警告
被跳过，所以问它们等于问了个空。

不审查的：用户自己的 MCP server（`~/.zeroth/mcp.json`，由用户或
`zeroth mcp add` 写 —— Claude Code 也不问它们）、skill 与 subagent 定义（加
载时不跑任何东西的 markdown，`docs/state/project-config.md`）。

## 存储：`trust.json`

`{config_home}/trust.json` 记着用户决定了什么，以每个东西被读出的那个文件的绝对
路径为键 —— 从不放在项目里，因为一个能自带批准的 checkout 会自己批准自己。用户自
己的文件也按绝对路径取键：一个相对路径的 `ZEROTH_HOOKS_FILE` 若按原样保留，
就会在每个目录里都是同一个键、指向不同的文件（`config::user_hooks_path`）。

```json
{
  "hooks": {
    "/home/me/.zeroth/hooks.json": [
      { "event": "PreToolUse", "matcher": "bash", "command": "./guard.sh" }
    ]
  },
  "mcp": {
    "/home/me/repo/.mcp.json": {
      "github": {
        "enabled": true,
        "fingerprint": "sha256:…",
        "parts": {
          "transport": "http",
          "target": "sha256:…",
          "values": { "Authorization": "sha256:…" }
        }
      }
    }
  }
}
```

**一个 hook 被信任，当且仅当列在它文件下的正是那个 hook**：事件、matcher（缺席、
空字符串和 `*` 都是同一个匹配一切的 matcher）和命令。matcher 按**运行器读到的样
子原样**保留（`hooks::constraint`，两者共用的唯一一份定义）—— 从不 trim：` Bash`
是一个没有哪个 tool 名字匹配得上的正则，`Bash` 则在每次 bash 调用时都触发，而一
条被 trim 过的记录会让前者的信任顺延到后者身上。对话框会把带空格的 matcher 加引
号引出来（`PreToolUse · " Bash"`），好让两者读起来不会一样。那才是决定什么在什么
时候跑的东西；`timeout` 或 `statusMessage` 两者都不改，所以改它们不会问任何问
题。记录 hook 本身而不是它的哈希，正是让对话框能显示一个**变了的** hook 以前是什
么的原因：一个不受信任的 hook，其事件与 matcher 都匹配某个列表中已不存在于该文件
的 hook，它就是那个 hook 被改过了，而对话框会把旧命令印在新命令下面（
`was: cargo fmt`）—— 正是这一行让一个被动过手脚的 hook 显得刺眼。顺序无关：在文
件里移动一个受信任的 hook 不会问任何问题。

**一个 server 的决定钉在它的整个条目上** —— 命令、参数和 env，或者 url 和头部（
`trust::server_fingerprint`，作用在 `mcp::render_server` 写出的那个条目上）—— 所
以一个变了的条目会再被问一次，不管它叫什么名字。**条目里的任何值都不保留** —— 不
是 URL、不是命令行、不是 `env` 值也不是头部：一枚 token 可能坐在其中任何一个里（
URL 里的 `?api_key=`、参数里的 `--api-key`），而 `trust.json` 是一个人们会跟
dotfiles 一起同步的文件。指纹旁边保留的是条目的形状 `trust::ServerParts`：传输方
式、命令行或 URL 的一个指纹，以及它 `env` 变量或头部的**名字**，每个都带着它值的
一个指纹。这对一个变了的条目说出**什么**变了足够了（`ServerParts::changes`）——
`changed: env NODE_OPTIONS added, the command line` —— 印在当前这个条目下面，正
是这一行让一个被注入的变量显得刺眼；一个 parts 说不出的变化（一个 `http` 条目的
`type` 被去掉了，现在可能退回 SSE）只显示 `changed` 徽标。

每次写入都是一次读-改-写，替换一个文件的记录、不动其他每个文件的 —— 而且是作用在
**做决定那一刻磁盘上的** `trust.json` 上（`trust::update_store`，边界的
`persist`），从不是作用在 session 启动时读的那份拷贝上：两个同时开着的 session
各自保住对方的答案。一条 hook 记录会被重写成该文件当前的 hook：一个从文件里移除
的 hook 会被丢掉，所以同一个命令以后再加回来时会再被问一次。

一个畸形的 `trust.json` **失败即关闭** —— 什么都不信任、什么都不启用，一条点名该
文件的红色 toast —— 而且**从不被覆盖**：用户以前决定的还在里面，等着错字被改掉，
所以期间做的决定只在本次 session 有效，一条红色 toast 会这么说（
`trust.json: … — not saved, so this choice lasts this session only`）。

### 把 `/trust` 的批准带过来

`/trust` 时代保留了一个 `projects` 键：项目根 → 文件 → 它批准的那些字节的指纹。
那些记录会被读，并在启动时为正在打开的项目带过来（
`TrustFile::migrate_legacy`）：一个 `/trust` 在**恰好当前这些字节**上批准过的文
件保住它的批准 —— 它的 hook 记为受信任，它的 server 记为启用，除非此后在这里对某
个做过决定 —— 而它的旧记录被删掉。此后被编辑过的文件把它的记录读作未读（`/trust`
批准的不是现在这里的），所以对话框会问它。此后没被打开过的项目在每次写入中都保住
它们的记录，等它们被打开时用。用户自己的 hook 从来不在 `/trust` 之下，所以要审查
一次。

## 审查对话框

启动时，横幅排好队之后、在任何东西跑之前，只要有什么要决定，session 就打开审查。
它是 `/hooks` 家族里一个替换 composer 的内联页面 —— 由 picker 们的规则框起来，没
有文本输入，藏起来的光标坐在它的 `❯` 上 —— 最多两页：先是 hook，然后是项目的 MCP
server。

```
──────────────────────────────────────────────────────────────────────────
  2 hooks to review · 1 new, 1 changed  1/2
  Hooks run shell commands on their own whenever their event fires, outside
  the permission prompt. Only hooks you trust will run.

  User · ~/.zeroth/hooks.json
  ❯ [✔] Stop  changed
        cargo fmt && cargo clippy --all-targets -- -D warnings
        was: cargo fmt

  Project · ~/repo/.zeroth/hooks.json
    [✔] PreToolUse · bash
        ${ZEROTH_PROJECT_DIR}/.zeroth/hooks/guard.sh

    Trust selected and continue
    Continue without trusting

  Space to select · Enter to confirm · Esc to continue without trusting
──────────────────────────────────────────────────────────────────────────
```

```
──────────────────────────────────────────────────────────────────────────
  2 new MCP servers found in this project  2/2
  Select any you wish to enable.
  An enabled server runs its command, or connects to its URL, whenever a
  session starts here. Its tool calls still follow your permission mode.

  ~/repo/.mcp.json
  ❯ [✔] brave-devtools · stdio
        npx -y @brave/devtools-mcp
        env  BRAVE_API_KEY=${BRAVE_API_KEY}
    [✔] github · http
        https://api.githubcopilot.com/mcp/

    Enable selected
    Disable all

  Space to select · Enter to confirm · Esc to disable all
──────────────────────────────────────────────────────────────────────────
```

每个条目都**逐字**显示会跑什么 —— 一个 hook 的整条命令、一个 server 的命令行或
URL 及其环境 —— 逐词换行，从不截断，因为一条命令的尾巴（`| sh`、一个重定向）才是
值得读的部分。条目按它们来自的文件分组。一个变了的条目用琥珀色这么说，并显示它以
前是什么。一切都以选中开始，和 Claude Code 一样：对话框的存在是为了确保用户看见
会跑什么，而不是让跑起来变难。

键：↑/↓ 在条目和两行动作行上移动（两端环绕），Home/End 跳，**Space** 切换被高亮
的条目，**Enter** 从条目或第一行动作行确认选择、从第二行则拒绝，**Esc** 拒绝。
**Ctrl+C 退出**，什么都不回答 —— 不像 picker，它们的 Ctrl+C 只是关掉自己：这一页
是自己打开的，一个本来没打算面对它的用户想出去，而在 server 页上拒绝会把每个
server 都记成关闭。它问过的什么都不跑，下次启动会再问。唯一一个用户有意打开的审
查 —— 在 `/settings` 里打开的 **Hooks**（*After startup*）—— 是他自己要的
picker，所以它的 Ctrl+C 按 picker 的方式关闭它：什么都没回答、什么都没记录，
session 继续（`ConfigReview::requested`）。

那些答案是什么意思：

| 页面 | Enter（确认） | 第二行 / Esc |
| ---- | --------------- | -------------------- |
| hook | 选中的 hook 记为受信任、从现在起运行；其余的不记录，本次 session 保持关闭，下次启动再问 | 什么都不记录，列出的 hook 本次 session 一个都不跑，下次启动再问 |
| MCP server | 选中的 server 记为启用、立刻连接；其余记为关闭 | 列出的每个 server 都记为关闭 |

两页的拒绝刻意不同。一个 server 有一个确定的、可见的「关闭」状态和改它的地方 ——
`/mcp` 把一个被拒绝的 server 列成 `◯ disabled` 并带一个 Enable 动作 —— 所以一次
拒绝值得记住，Claude Code 就是这么记的。一个 hook 没有这样的状态：一个不受信任的
hook 是沉默的，而大多数 hook 是用户自己的，所以一个被拒绝的不会被记住；下次启动
会再问，而不再被问的办法是信任它、移除它，或者在 `/settings` 里为这个目录关掉
**Hooks**。

审查打开期间，没有任何东西在无预告地等它。在一个
[herdr](../architecture/herdr.md) 窗格里，session 报告 `blocked`，点名这一页要什
么（`Trust hooks: Stop, PreToolUse · bash`、`Enable MCP servers: github`）。
`SessionStart` hook 被扣住（`ModelSession::hold_session_start`），在审查关闭时触
发，所以在第一页上被信任的 hook 仍然听得到它为它配置的那个 `startup`，而来自命令
行的 `[PROMPT]` 也在那时提交，于是第一个回合在刚启用的 server 上开始。一个待定的
server 是 `⚠ awaiting review`，在它得到答案之前从不启动。

### 一个放得下的页面

审查在第一帧就打开，所以它绝不能把顶部流进 scrollback：一个流动的页面在关闭时会
purge 重建屏幕，而在启动时那会抹掉启动之前终端自己的 scrollback（
`docs/ui/view-flow.md`）。所以它像紧凑 picker 那样让行（
`model_view::first_page_that_fits`），最完整的在先：

1. 整页；
2. 去掉标题下面的解释；
3. **折叠**：每个条目一行 —— 它的标题和它要跑的东西的第一行，用 `…` 截断 —— 但被
   高亮的那个例外，它保住完整细节；
4. 折叠后的列表**开窗**在被高亮项周围，上下是 `↑ N more above` /
   `↓ N more below` 行，文件小标题去掉（被高亮条目改为点出自己的文件），窗口
   一次缩一行；
5. 被高亮条目的细节砍到三行，然后一行；
6. 标题。

两行动作行和提示在每一档上都在。

## 启动之后

- **`/hooks`** 显示每个配置过的 hook，信任与否都显示。一个不受信任的 hook 在列表
  里标着 `⚠ not trusted`，它的详情页说它不跑；在那里按 Enter 信任它，在一个受信
  任的 hook 上则停止信任它。两者都被记录并立刻生效（后端重建带上新集合；一个正在
  跑的回合在旧的上面跑完）。
- **`/settings` Hooks** 在有 hook 等待信任时打开，会打开审查的 hook 页；**
  `/settings` MCP** 在有项目 server 等待答案时打开，会打开 server 页（
  `docs/tools/mcp.md`）。
- **`/mcp`** 对一个被拒绝的项目 server 按 Enable 会把它记为启用并连上它；对一个
  项目 server 按 Disable 会把它记为关闭。一个在这个设计之前经由 `/mcp` 关闭的项
  目 server（用户文件的按目录 `disabled` 列表）保持关闭，直到在这里启用它，那会
  清掉它。

## 什么关着

- `ZEROTH_HOOKS=0`，或为这个目录关掉 **Hooks**：没有 hook 页 —— 没有东西
  会跑。
- 为这个目录关掉 **MCP**，或这次运行 `ZEROTH_MCP=0`：没有 MCP 页 —— 没有东西
  会跑。之后打开那一行会问。
- `ZEROTH_PROJECT_CONFIG=0`：完全没有项目层；用户自己的 hook 仍然被审查。
  smoke 套件在它的基础环境里设这个，而加载 `hooks.json` 的那些 phase 先回答 hook
  页。
- 没有配置目录（没有 `$HOME`，没有 `ZEROTH_CONFIG_DIR`）：决定只在本次 session
  有效，下次启动再问。

## 它不做什么

- **它钉的是命令，不是命令读的东西。** `./fmt.sh` 在 `fmt.sh` 改了之后仍然受
  信任；两个参考实现划的是同一条线。
- **没有逐回合重扫。** hook 与 MCP 文件保持它们启动时读一次的寿命（
  `docs/state/project-config.md`）；一个 session 中途加进来的 hook 在下次启动时
  被审查。

## 模块划分

纯的，有单元测试：

- `src/trust.rs` —— 存储（`TrustFile`：`review_hooks`、`server_trust`、
  `record_hooks`、`record_servers`、`migrate_legacy`），每个答案都经过的那次读-
  改-写（`update_store`，拒绝一个畸形的存储），hook 身份（`HookIdentity`，它的
  matcher 由 `hooks::constraint` 给出）、`trusted_only`、`server_fingerprint` 以
  及条目的无值形状（`ServerParts`、`ServerChange`）、`review_servers`，外加它一
  直有的项目根辅助函数。
- `src/app/config_review.rs` —— 对话框的状态与按键；
  `src/ui/config_review_view.rs` —— 它的页面与让位阶梯。
- `mcp::merge_project_scopes` —— 被批准的项目 server 遮蔽用户的，被扣住的一律不
  遮蔽；`mcp::McpServerStatus::AwaitingReview`。
- `hooks::HooksOverview` —— 每个 hook 及其信任状态，供 `/hooks` 用。
- `herdr::activity` —— 审查的页面作为 herdr 的 `blocked` 消息。

边界，经 smoke 验证：

- `src/tui/trust.rs` —— 读各层、把 `/trust` 的批准带过来、构建页面、应用每个答案
  （先 session 的拷贝，然后对磁盘上现状的 `persist`；hook 的重绑；MCP 的放行或扣
  住），以及结束审查（`SessionStart`、`[PROMPT]`）。
- `src/llm/mcp/manager.rs` —— 被扣住的集合（`McpHold::{Review, Declined}`）和
  `set_approved`。
- Smoke Phase 83 驱动两页、持久化、一个变了的 hook 和一个变了的 server、
  `/settings` 审查的 Ctrl+C、两个 session 同时作答、一个被放着不管的畸形存储，以
  及一个被带过来的 `/trust` 批准。
