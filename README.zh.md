<div align="center">

<img src="assets/banner-v2.jpg" alt="Zeroth 的冠羽幼龙与发光零环" width="100%">

<h1><img src="assets/mascot-crest.png" alt="" height="58" align="center"> Zeroth</h1>

### 给你的终端配一个编码 agent。

和一个与你并肩工作的 agent 一起，读代码、构建、调试、评审。<br>
同一套 tool 也可用于网络安全研究、技术调查与日常自动化。

<p>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Built_with-Rust-F5A97F?style=for-the-badge&amp;logo=rust&amp;logoColor=white&amp;labelColor=181825" alt="Built with Rust" height="32"></a>
  <a href="#快速开始"><img src="https://img.shields.io/badge/Platforms-Linux_%7C_macOS-89B4FA?style=for-the-badge&amp;logo=linux&amp;logoColor=white&amp;labelColor=181825" alt="Platforms: Linux and macOS" height="32"></a>
  <a href="https://github.com/yxxbc/zeroth/releases"><img src="https://img.shields.io/badge/Version-0.0.0-89DCEB?style=for-the-badge&amp;logo=github&amp;logoColor=white&amp;labelColor=181825" alt="Version: 0.0.0" height="32"></a>
  <br>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache--2.0-A6E3A1?style=for-the-badge&amp;logo=apache&amp;logoColor=white&amp;labelColor=181825" alt="License: Apache 2.0" height="32"></a>
</p>

[快速开始](#快速开始) ·
[功能](#功能) ·
[模型供应商](#模型供应商) ·
[命令与快捷键](#命令与快捷键) ·
[致谢](#致谢)

</div>

---

## 为什么是 Zeroth

Zeroth 是一个用 Rust 写的开源 AI 编码 agent，灵感来自 Claude Code 与 Codex CLI。
它直接在你的项目里工作：读文件、做修改、跑命令、看结果，而你在终端里全程跟着看。
编码是它的重点，同时也留出了足够的灵活度去调查安全问题、探索陌生的系统，用你手上
已有的 tool 把任务自动化。

| 你得到什么 | 为什么重要 |
| --- | --- |
| **看得懂的工作过程** | 实时的命令输出、可读的 diff 和一份任务清单，让进展一眼可查。 |
| **对执行的掌控** | 选一个权限模式、复核提议的改动，并在 agent 干活时给它掌舵。 |
| **模型由你挑** | 用某个支持的订阅或 API key 接入，用 Ollama 或 LM Studio 跑本地模型，或者添加一个 OpenAI 兼容的供应商。 |
| **可扩展的余地** | 加入专职的 subagent、可复用的 skill、生命周期 hook 与 MCP 服务器，贴合你的工作方式。 |
| **原生的终端体验** | 一个内存占用很小的 Rust 二进制，带持久 session、内联图片与键盘操作。 |

## 快速开始

Zeroth 跑在**Linux**（x86_64、arm64）与**macOS**（Intel、Apple silicon）的现代终
端里。

> **fork 说明：** Zeroth 派生自
> [Alter Zero](https://github.com/linuztx/alter-zero)。本仓库里的安装器安装的是
> [`yxxbc/zeroth`](https://github.com/yxxbc/zeroth) 的 release；在第一个
> release 发布之前，请用[从源码构建](#从源码构建)。

### 安装

Linux 或 macOS 上的一行命令：

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh | sh
```

它会挑出适合你机器的构建、对照随 release 发布的校验和验证它的 SHA-256、把
`zeroth` 装进 `~/.local/bin`，并在这个目录不在你的 `PATH` 里时告诉你。它的横幅是
应用自己的 `crest` mascot，干活时会动起来；`sh install.sh --preview` 只把那支
mascot 播十秒（或 `--preview 20`），完全不碰网络。

<details>
<summary><strong>安装选项</strong></summary>

设 `ZEROTH_INSTALL_DIR` 换个安装位置，或用 `ZEROTH_VERSION=vX.Y.Z` 钉住某个
release。[`CHANGELOG.md`](CHANGELOG.md) 记录了各版本之间变了什么。

```bash
ZEROTH_INSTALL_DIR="$HOME/bin" ZEROTH_VERSION=v0.1.0 \
  sh -c "$(curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh)"
```

只想看看横幅、什么都不装：

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh | sh -s -- --preview 10
```

手工安装的话，每个 release 的说明里列出了每个归档及其 SHA-256，你可以自己下载、
校验并解压一份。

</details>

### 从源码构建

你需要 [rustup](https://rustup.rs) 和一个 C 编译器。仓库钉住了 Rust toolchain，
rustup 会自动装上它。

```bash
git clone https://github.com/yxxbc/zeroth.git
cd zeroth
cargo install --path .
zeroth
```

没有配置供应商时，Zeroth 会打开一个带脚本化回合的离线 demo，好让你先摸清界面。要
干自己的活：

1. 运行 **`/login`**，挑一个支持的订阅、API 供应商或本地模型服务器。按提示连上。
2. 运行 **`/model`** 选一个模型。Zeroth 会为这个目录记住你的选择。
3. 描述一个任务，复核出现的批准提示，然后看着活干下去。

通过 `/login` 添加的登录信息保存在 `~/.zeroth/.env`，在你的仓库之外。用 `/` 浏览
命令，或在空的 composer 里按 `?` 看快捷键。

<details>
<summary><strong>用环境变量接入</strong></summary>

你也可以在启动之前选好供应商：设好它的 API key 变量与供应商 id，再启动应用。比如
用 OpenRouter：

```bash
export OPENROUTER_API_KEY="your-api-key"
export ZEROTH_PROVIDER=openrouter
zeroth
```

用 `/model` 选模型，或在启动前把 `ZEROTH_MODEL` 设成它的模型 id。

</details>

## 让它干活

从一个具体的任务开始，让 agent 陪你把它做完。

| 场景 | 试着这样说 |
| --- | --- |
| **编码与调试** | 「查清这个测试为什么失败，修掉原因，再跑相关的测试。」 |
| **网络安全研究** | 「审一遍这个仓库有没有安全漏洞。把每一条发现追溯到代码，并给出修法。」 |
| **技术调查** | 「理清这个项目里的认证是怎么走的，说明权限检查发生在哪里。」 |
| **自动化** | 「写个脚本把这些日志汇总，并标出反复出现的错误。」 |

## 模型供应商

接到一个支持的订阅服务、API 供应商或本地模型服务器上。用 `/login` 接入，用
`/model` 选模型。

| 供应商 | 怎么接入 | 值得知道 |
| --- | --- | --- |
| **Agent Zero API** | API key。 | 经 Agent Zero 的代理访问 Venice.ai 的模型，[A0T](https://www.agent-zero.ai/p/token/) 持币者每天有免费额度。 |
| **Venice** | API key。 | 从 [Venice 自己的 API](https://venice.ai/settings/api) 直取 Venice.ai 私密、不审查的模型 —— 与 Agent Zero API 所代理的是同一批，只是用你自己的账号拿到。 |
| **GitHub Copilot** | 订阅，一次性设备码。 | 模型能力（上下文窗口、视觉、推理档位）由 Copilot 提供。 |
| **ChatGPT Codex** | 订阅，需 ChatGPT Plus 或 Pro 账号：浏览器登录，或在 headless 机器上用一次性设备码。 | 走账号登录流程，没有 API key 要粘。 |
| **Anthropic** | API key，或经 Anthropic Console 的账号登录。 | 两种方式都把用量记在你的 API 组织账上。 |
| **Cline** | API key。 | Cline 扩展与 CLI 背后的那些模型，经 [Cline 的 OpenAI 兼容 API](https://app.cline.bot) —— Anthropic、OpenAI、Google 等等，记在你账号的额度上。 |
| **ClinePass** | 订阅（每月 $9.99），需 Cline 账号登录。 | 在 Cline 精选的开源编码模型上 2-5 倍用量 —— 这个方案的 `cline-pass/…` 模型会出现在 `/model` 里。 |
| **Google Antigravity** | 在浏览器里用 Google 账号登录。 | 用你的 Antigravity 额度跑 Gemini、Claude 与 GPT-OSS。非官方：Google 可能限制在 Antigravity 之外使用的账号（[细节](docs/providers/antigravity.md)）。 |
| **OpenRouter** | API key。 | 用一个账号访问多家供应商的模型。 |
| **Ollama** | 本地模型：填你服务器的地址，保存前会先测一遍；不需要 key。 | 用本地托管的模型在自己的机器上跑推理。 |
| **LM Studio** | 本地模型：填你服务器的地址，测法相同；只有服务器要 token 时才需要 token。 | 跑你在 [LM Studio](https://lmstudio.ai/download) 里下好的模型；先把它自己的服务器启起来。 |
| **Ollama Cloud** | API key。 | 用 Ollama 托管的开源模型。 |

页脚显示当前生效的模型、思考模式、速度档与上下文用量。**Ctrl+T** 轮转模型支持的
各个推理档位，列出速度档的 ChatGPT 模型会为每一档给出一个命令
（**`/fast`**、`/ultrafast`）把它切到那种处理方式（回复更快，但方案的用量更
高），发图片之前会检查视觉支持，而在供应商允许的地方请求会启用 prompt 缓存。

<details>
<summary><strong>添加你自己的供应商</strong></summary>

任何提供 OpenAI 兼容 API 的服务，或者 llama.cpp、vLLM 这样的本地服务器，都可以写
进 `~/.zeroth/providers.toml`。这个文件是往上加，而不是替换掉上面那些供应商：

```toml
[providers.groq]
name = "Groq"
api_key_url = "https://console.groq.com/keys"

[providers.groq.kwargs]
api_base = "https://api.groq.com/openai/v1"
```

用 `/login` 存下它的 key（这一个从 `GROQ_API_KEY` 读），然后用 `/model` 选模型。
两者都会重读这个文件，所以不必重启。一个点名了内置供应商的块只改它设置的那几个
键。[`docs/providers/user-providers.md`](docs/providers/user-providers.md) 覆盖
了每个键、不需要 key 的本地服务器，以及每条错误信息是什么意思。

</details>

## 功能

### 跟着看活是怎么干的

- **可读的文件改动。** 带编号、带语法高亮的文件视图与 diff 会标出变化的字符，于
  是改动很容易复核。
- **实时命令与任务进度。** 命令输出流进对话，长结果折叠成可展开的预览，一份清单
  跟踪多步骤的工作。
- **并行与后台作业。** tool 批次在执行之前先公告。用 **Ctrl+B** 把跑着的命令移到
  后台，然后在空的 composer 里按 **Down** 去查看或停掉它。
- **就地决策。** 内联提问支持多选与自由作答。按 **Enter** 发一条消息给正在跑的回
  合掌舵，或用 **Tab** 排一个后续回合。

### 选择保留多少控制权

- **在批准提示处复核。** 批准之前先看清提议的文件内容、diff 与命令。批准一次、允
  许一条会话规则，或用 **Tab** 带指示拒绝。**Ctrl+E** 要求解释一条命令。
- **四种权限模式。** `manual` 在改文件与跑命令之前都问；`edit` 允许改文
  件；`auto` 是默认，给常规命令再加一个审查者；`master` 无人值守地跑。用
  **Shift+Tab** 轮转模式，并按项目保存规则。
- **不开屏幕也能跑。** `zeroth -p "…"` 为一段脚本、一个 cron 任务或一个 CI 步骤
  回答一个 prompt：答案进 stdout，或加 `--output-format` 得到 JSON，并给出一个可
  以用以分支的退出状态。没有任何东西在等回复；`--permission-mode` 决定什么不问就
  跑。见 [headless 模式](docs/cli/headless.md)。
- **回滚点与回退。** 按目录启用回滚点，把工作文件快照到另一个存储里。**Esc Esc**
  回到之前的某条消息，并且在有回滚点可用时连文件一起恢复。
- **没有东西会在你看不见的地方自己跑起来。** 应用启动时会列出任何新增或变过的
  hook（你的或项目的），以及任何新增的项目 MCP 服务器，并如实显示每一个会跑什
  么。hook 只在你信任它之后才跑，服务器只在你启用它之后才跑，而 `/hooks` 与
  `/mcp` 之后可以改这两者。
- **用占位符管凭据。** 用 `/secrets` 存一份凭据，agent 就能用它的占位符而不看见
  值。见 [密钥是怎么存与怎么用](docs/state/secrets.md)。

### 给 agent 趁手的 tool

- **Subagent。** 把专注的活分给带有自己上下文、模型与 tool 访问权限的 agent。内
  置的 `general-purpose` 与 `explore` 可以改，你也可以打开一个正在跑的 agent 的
  session 去聊、派后续的活或停掉它 —— 与此同时主 agent 按每个 agent 的 id 做同样
  的事：查看它的进度、给已完成的 agent 连上下文一起派后续，或把它停掉。
- **Skill。** 添加可复用的 `SKILL.md` 目录，用 `$` 提到某个，或用 `/skills` 浏
  览。内置的 `skill-creator` 帮你写自己的；`jina-reader` 经 Jina Reader 读公开网
  页与 PDF；`yt-dlp` 借助另行安装的 CLI 指导媒体下载。

- **网页搜索与读页。** `web_search` 与 `web_fetch` 是原生 tool，不是 skill：不要
  key、不要 Python、不要任何准备。搜索依次试 Parallel、AnySearch、Tavily 与
  Bing，跳过报告额度已用完的来源并在之后回来找它，而 `web_fetch` 一次一页地把一
  个 URL 读成文本。私有地址、回环地址与元数据地址一律拒绝，除非有代理或某条规则
  另说。
- **公网隧道。** 内置的 `flaredantic` skill 指导你借助另行安装的 `flare` CLI，经
  Cloudflare、Microsoft Dev Tunnels 或 Serveo 把一个本地服务暴露出去。隧道会让这
  个服务在公网可达；先想清楚你暴露了什么。
- **MCP 服务器。** 经 stdio、HTTP 或 SSE 接入 tool，包括带 OAuth 的远程服务器。
  用 `/mcp` 或 `zeroth mcp` 管理连接。
- **Hook 与项目说明。** 用 `hooks.json` 批准、拒绝或调整 tool call，并补充上下
  文。`AGENTS.md` 文件承载你项目的约定；`/init` 帮你写一个。

### 从上次停下的地方接着干

- **保存下来的对话。** `/resume` 打开一个可搜索的 session picker。用
  `--continue` 接本目录里最新的 session，或用 `--resume <id>` 指定某一个。
- **跟着项目走的设置。** 模型选择、权限、skill 与外观按目录记住，而每一段保存下
  来的对话都在它当时用的模型上续上。输入历史跨 session 保留，用 **Ctrl+R** 搜
  索。
- **被管理的上下文。** `/context` 显示上下文窗口被什么占满、其中有多少是 prompt
  缓存伺候的，`/usage` 显示每个订阅的方案还剩多少。`/compact` 摘要长对话，窗口填
  满时还会自动压缩。session 的 scratchpad 存放你项目之外的临时脚本与笔记。

### 把终端变成你自己的

- **丰富的输出。** 流式输出 Markdown、表格、高亮代码与可折叠的推理块，在支持的终
  端里 URL 与文件路径可点击。
- **内联图片。** 用 **Ctrl+V** 粘贴截图。图片在 kitty、iTerm2 与 sixel 终端里原
  生渲染，别处则回退到基于字符的画法。
- **检查 session。** **Ctrl+O** 打开展开的 transcript；**Ctrl+D** 显示发给模型的
  上下文。以 `!` 开头写一行就是 shell 命令，用 `@` 挑文件。
- **实时定制。** 用 `/theme` 预览十一个主题，用 `/mascot` 预览六支 mascot，用
  `/spinner` 预览九种 spinner 样式。`/settings` 把剩下的控件收进一个可搜索的菜
  单。
- **与 herdr 配合。** 在一个 [herdr](https://herdr.dev) 窗格里，侧栏会显示 agent
  是在干活、在等你批准，还是已做完，而重启后的 herdr 会续上对话。什么都不用配。

## 命令与快捷键

要点永远只差一次按键：`/` 打开命令，在空的 composer 里按 `?` 看快捷键。

<details>
<summary><strong>斜杠命令</strong></summary>

| 命令 | 做什么 |
| --- | --- |
| `/clear` | 清空对话 |
| `/copy` | 把最后一条回复复制到剪贴板 |
| `/export` | 把对话按纯文本复制或保存 |
| `/diff` | 在全屏查看器里复核已暂存、未暂存与未跟踪的 Git 改动 |
| `/init` | 创建一个 `AGENTS.md` 贡献指南 |
| `/compact` | 摘要对话以腾出上下文 |
| `/context` | 看上下文窗口被什么占满：各分类的 token、空闲空间与缓存命中率 |
| `/usage` | 看各订阅的用量上限：每个窗口用了多少、何时重置 |
| `/resume` | 续上一段保存的对话 |
| `/model` | 切换当前模型 |
| `/fast`、`/ultrafast`、…… | 切换模型的速度档 —— 它的列表点名几档就有一个命令，只在列出这一档的模型上出现 |
| `/login` | 登录、添加 API key，或接入一个本地模型 |
| `/logout` | 退出账户或移除已保存的 API key（也可从登录菜单管理连接） |
| `/settings` | 打开设置菜单 |
| `/theme` | 选配色主题 |
| `/mascot` | 选横幅 mascot |
| `/spinner` | 选状态 spinner 样式 |
| `/hooks` | 浏览 hook，逐个信任或取消信任 |
| `/skills` | 浏览 skill，逐个启用或停用 |
| `/mcp` | 管理 MCP 服务器 |
| `/secrets` | 存下 agent 以占位符 `<secret:NAME>` 使用的凭据，agent 全程看不见值 |
| `/quit` | 退出应用 |

</details>

<details>
<summary><strong>键盘快捷键</strong></summary>

| 按键 | 动作 |
| --- | --- |
| `Enter` | 发送，或把一条消息交给正在跑的回合 |
| `Tab` | 排一个后续回合 |
| `Alt+↑` | 把排队的消息取回来编辑 |
| `Shift+Enter` / `Ctrl+J` | 插入换行 |
| `↑` / `↓` | 回溯输入历史 |
| `Ctrl+R` | 搜索输入历史 |
| `Esc` | 中断正在跑的回合 |
| `Esc Esc` | 退回之前的某条消息并回退 |
| `Ctrl+O` | 打开完整 transcript |
| `Ctrl+D` | 显示发给模型的上下文 |
| `Ctrl+T` | 轮转思考模式 |
| `Shift+Tab` | 轮转权限模式 |
| `Ctrl+B` | 把跑着的命令或 agent 移到后台 |
| `↓`（空的 composer） | 管理后台 shell 与 agent |
| `Ctrl+V` | 粘贴一张图片 |
| `!` | 跑一条 shell 命令 |
| `@` | 挑一个文件路径 |
| `$` | 提到一个 skill |
| `/` | 打开命令 palette |
| `Ctrl+W` / `Ctrl+U` / `Ctrl+K` | 删掉前一个词、删到行首、删到行尾 |
| `Ctrl+C` | 清掉草稿，再按一次退出 |

</details>

<details>
<summary><strong>命令行选项</strong></summary>

```text
Usage: zeroth [OPTIONS] [PROMPT]
       zeroth -p [OPTIONS] [PROMPT]
       zeroth mcp <COMMAND>
       zeroth update

Arguments:
  [PROMPT]                      Send this message as the first turn

Options:
  -c, --continue                Continue the most recent conversation in this directory
  -r, --resume [ID]             Resume a conversation by id, or pick one from a list
  -C, --cwd <DIR>               Run in DIR instead of the current directory
      --permission-mode <MODE>  manual, edit, auto or master, for this run only
  -h, --help                    Print help
  -V, --version                 Print version

Print mode:
  -p, --print                   Answer once with no screen: the answer on stdout, problems on stderr
      --output-format <FORMAT>  text, json or stream-json
      --max-turns <N>           Stop after N model replies
      --verbose                 Log each step of the run to stderr
```

直接起一个任务，或接着上次 session 的活干：

```bash
zeroth "fix the failing test"
zeroth -c "now review the changes"
```

或者从一个脚本、一个 cron 任务或 CI 里跑它，完全不要终端：

```bash
zeroth -p "Run the test suite and summarize the failures" > report.md
git diff origin/main | zeroth -p --output-format json "Review this diff"
```

[Headless 模式](docs/cli/headless.md) 讲了各输出格式、退出码与权限模式。

跑 `zeroth mcp --help` 看 MCP 服务器的管理命令，跑 `zeroth update` 把最新
release 装在你正在跑的这个二进制之上。

</details>

## 东西都在哪

你的登录信息、保存的 session、输入历史与按目录的设置都在 `~/.zeroth/` 下。共享扩
展放那里，或把项目专属的放在你的代码旁边。

如果之前的安装留下了 `~/.alter-zero/`，第一次启动会从里面一次性拷出
`config.json`、`providers.toml` 与 `.env`。session、历史、缓存、skill 与 agent
留在原处，旧目录永远不会被改动 —— 上一个应用仍然能靠它工作。

环境变量覆盖一律是 `ZEROTH_*`；脚本里还在用这个 fork 旧前缀的必须改。完整的改名
及其破坏性变更见 [`docs/architecture/naming.md`](docs/architecture/naming.md)。

<details>
<summary><strong>配置与扩展的路径</strong></summary>

| 位置 | 内容 |
| --- | --- |
| `~/.zeroth/.env` | 通过 `/login` 保存的 key 与 token |
| `~/.zeroth/agents/` 与 `~/.zeroth/skills/` | 你的共享 agent 定义与 skill |
| `~/.zeroth/hooks.json` 与 `~/.zeroth/mcp.json` | 你的共享 hook 与 MCP 服务器 |
| 项目里的 `.zeroth/` | 项目专属的 `agents/`、`skills/`、`hooks.json` 与 `mcp.json` |
| `AGENTS.md` | agent 读取的项目说明 |

也支持 Claude Code 风格的 `.claude/skills/` 与 `.mcp.json`。你的 hook、项目的
hook，以及项目的 MCP 服务器，都只在你开机时于应用里批准过之后才运行。

</details>

## 许可

Zeroth 以 [Apache License 2.0](LICENSE) 发布。Copyright 2026 the Zeroth
contributors。上游的署名记录在 [NOTICE](NOTICE)。

## 致谢

Zeroth 得以存在，靠的是它背后的人、项目与社区：

- [Claude Code](https://claude.com/claude-code) 与 [Codex
  CLI](https://github.com/openai/codex)，塑造 Zeroth 交互方式的那些模式来自它
  们。
- [ratatui](https://ratatui.rs) 与
  [crossterm](https://github.com/crossterm-rs/crossterm)，终端 UI 的地基来自它
  们。

另见[技术致谢](docs/credits.md)。
