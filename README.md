<div align="center">

<img src="assets/banner.jpg" alt="Zeroth" width="100%">

# Zeroth

### Give your terminal a coding agent.

Read, build, debug, and review code with an agent that works alongside you.<br>
Use the same tools for cybersecurity research, technical investigation, and everyday automation.

<p>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/Built_with-Rust-F5A97F?style=for-the-badge&amp;logo=rust&amp;logoColor=white&amp;labelColor=181825" alt="Built with Rust" height="32"></a>
  <a href="#quick-start"><img src="https://img.shields.io/badge/Platforms-Linux_%7C_macOS-89B4FA?style=for-the-badge&amp;logo=linux&amp;logoColor=white&amp;labelColor=181825" alt="Platforms: Linux and macOS" height="32"></a>
  <a href="https://github.com/yxxbc/zeroth/releases"><img src="https://img.shields.io/badge/Version-0.0.0-89DCEB?style=for-the-badge&amp;logo=github&amp;logoColor=white&amp;labelColor=181825" alt="Version: 0.0.0" height="32"></a>
  <br>
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-Apache--2.0-A6E3A1?style=for-the-badge&amp;logo=apache&amp;logoColor=white&amp;labelColor=181825" alt="License: Apache 2.0" height="32"></a>
</p>

[Quick start](#quick-start) ·
[Features](#features) ·
[Model providers](#model-providers) ·
[Commands & shortcuts](#commands--shortcuts) ·
[Acknowledgements](#acknowledgements)

</div>

---

## Why Zeroth

Zeroth is an open-source AI coding agent written in Rust, inspired by Claude Code and Codex CLI. It works directly in your project: reading files, making edits, running commands, and checking results while you follow along in your terminal. Coding is its focus, with the flexibility to investigate security issues, explore unfamiliar systems, and automate tasks using your existing tools.

| What you get | Why it matters |
| --- | --- |
| **Work you can follow** | Live command output, readable diffs, and a task checklist make progress easy to inspect. |
| **Control over execution** | Choose a permission mode, review proposed changes, and steer the agent while it works. |
| **Your choice of model** | Connect with a supported subscription or API key, run local models with Ollama or LM Studio, or add an OpenAI-compatible provider. |
| **Room to extend** | Add specialist subagents, reusable skills, lifecycle hooks, and MCP servers to fit your workflow. |
| **A native terminal experience** | A single Rust binary designed with a small memory footprint, persistent sessions, inline images, and keyboard-driven controls. |

## Quick start

Zeroth runs on **Linux** (x86_64, arm64) and **macOS** (Intel, Apple silicon) in a modern terminal.

> **Fork note:** Zeroth is a derivative of
> [Alter Zero](https://github.com/linuztx/alter-zero). The installer in this
> repository installs releases from
> [`yxxbc/zeroth`](https://github.com/yxxbc/zeroth); until the first one is
> published, use [Build from source](#build-from-source).

### Install

One line on Linux or macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh | sh
```

It picks the build for your machine, verifies its SHA-256 against the checksum published with the release, installs `zeroth` into `~/.local/bin`, and tells you if that directory is not on your `PATH`. Its banner is the app's own `crest` mascot, animated while it works; `sh install.sh --preview` plays that mascot alone for ten seconds (or `--preview 20`) without touching the network.

<details>
<summary><strong>Install options</strong></summary>

Set `ALTER_ZERO_INSTALL_DIR` to install somewhere else, or `ALTER_ZERO_VERSION=vX.Y.Z` to pin a release. [`CHANGELOG.md`](CHANGELOG.md) records what changed between versions.

```bash
ALTER_ZERO_INSTALL_DIR="$HOME/bin" ALTER_ZERO_VERSION=v0.1.0 \
  sh -c "$(curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh)"
```

To watch the banner without installing anything:

```bash
curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh | sh -s -- --preview 10
```

For a manual install, each release's notes list every archive with its SHA-256, so you can download, verify, and extract one by hand.

</details>

### Build from source

You need [rustup](https://rustup.rs) and a C compiler. The repository pins the Rust toolchain, which rustup installs automatically.

```bash
git clone https://github.com/yxxbc/zeroth.git
cd zeroth
cargo install --path .
zeroth
```

Without a configured provider, Zeroth opens an offline demo with scripted turns so you can explore the interface. To work on your own tasks:

1. Run **`/login`** and choose a supported subscription, API provider, or local model server. Follow the prompts to connect.
2. Run **`/model`** to choose a model. Zeroth remembers your selection for this directory.
3. Describe a task, review any approval prompts, and follow the work as it happens.

Sign-ins added through `/login` are saved to `~/.zeroth/.env`, outside your repository. Use `/` to browse commands or `?` in an empty composer to see shortcuts.

<details>
<summary><strong>Connect with environment variables</strong></summary>

You can also select your provider before launch. Set its API key variable and provider ID, then start the app. For example, with OpenRouter:

```bash
export OPENROUTER_API_KEY="your-api-key"
export ALTER_ZERO_PROVIDER=openrouter
zeroth
```

Choose a model with `/model`, or set `ALTER_ZERO_MODEL` to its model ID before launching.

</details>

## Put it to work

Start with a concrete task and let the agent work through it with you.

| Use case | Try asking |
| --- | --- |
| **Coding and debugging** | "Find why this test fails, fix the cause, and run the relevant tests." |
| **Cybersecurity research** | "Review this repository for security vulnerabilities. Trace each finding to the code and suggest a fix." |
| **Technical investigation** | "Trace how authentication works in this project and explain where access checks happen." |
| **Automation** | "Write a script that summarises these logs and flags recurring errors." |

## Model providers

Connect to a supported subscription service, API provider, or local model server. Use `/login` to connect and `/model` to choose your model.

| Provider | How you connect | Good to know |
| --- | --- | --- |
| **Agent Zero API** | API key. | Accesses Venice.ai models through the Agent Zero proxy, with a free daily quota for [A0T](https://www.agent-zero.ai/p/token/) token holders. |
| **Venice** | API key. | Venice.ai's private, uncensored models straight from [Venice's own API](https://venice.ai/settings/api) — the same catalog the Agent Zero API proxies, reached with your own account. |
| **GitHub Copilot** | Subscription, using a one-time device code. | Model capabilities, including context window, vision, and reasoning levels, come from Copilot. |
| **ChatGPT Codex** | Subscription, with a ChatGPT Plus or Pro account: a browser sign-in, or a one-time device code on a headless machine. | Uses the account sign-in flow; no API key to paste. |
| **Anthropic** | API key or account sign-in through the Anthropic Console. | Usage is billed to your API organisation with either method. |
| **Cline** | API key. | The models behind the Cline extension and CLI, through [Cline's OpenAI-compatible API](https://app.cline.bot) — Anthropic, OpenAI, Google and more, billed to your account's credits. |
| **ClinePass** | Subscription ($9.99/month), with a Cline account sign-in. | 2-5x usage on Cline's curated open coding models — the plan's `cline-pass/…` models appear in `/model`. |
| **Google Antigravity** | Google account sign-in in the browser. | Gemini, Claude and GPT-OSS on your Antigravity quota. Unofficial: Google may restrict accounts used outside Antigravity ([details](docs/providers/antigravity.md)). |
| **OpenRouter** | API key. | Access models from multiple providers through one account. |
| **Ollama** | Local model: enter your server's address, which is tested before it's saved; no key required. | Run inference on your own machine with locally hosted models. |
| **LM Studio** | Local model: enter your server's address, tested the same way; a token only if the server requires one. | Run the models you downloaded in [LM Studio](https://lmstudio.ai/download); start its server first. |
| **Ollama Cloud** | API key. | Use open models hosted by Ollama. |

The footer shows your active model, thinking mode, speed tier and context usage. **Ctrl+T** cycles the model's supported reasoning levels, a ChatGPT model that lists speed tiers gets a command per tier (**`/fast`**, `/ultrafast`) that switches it to that processing (faster replies at increased plan usage), vision support is checked before sending images, and requests support prompt caching where the provider allows it.

<details>
<summary><strong>Add a provider of your own</strong></summary>

Any service with an OpenAI-compatible API, or a local server such as llama.cpp or vLLM, can be added in `~/.zeroth/providers.toml`. The file adds to the providers above rather than replacing them:

```toml
[providers.groq]
name = "Groq"
api_key_url = "https://console.groq.com/keys"

[providers.groq.kwargs]
api_base = "https://api.groq.com/openai/v1"
```

Save its key with `/login` (this one is read from `GROQ_API_KEY`), then choose a model with `/model`. Both re-read the file, so there's no need to restart. A block that names a built-in provider changes only the keys it sets. [`docs/providers/user-providers.md`](docs/providers/user-providers.md) covers every key, local servers that need no key, and what each error message means.

</details>

## Features

### Follow the work as it happens

- **Readable file changes.** Numbered, syntax-highlighted file views and diffs highlight the characters that changed, so edits are easy to review.
- **Live commands and task progress.** Command output streams into the conversation, long results collapse into expandable previews, and a checklist tracks multi-step work.
- **Parallel and background work.** Tool batches are announced before execution. Move a running command to the background with **Ctrl+B**, then use **Down** from an empty composer to inspect or stop it.
- **Decisions in context.** Inline questions support multiple-choice and free-text answers. Send a message with **Enter** to steer the running turn, or use **Tab** to queue a follow-up.

### Choose how much control to keep

- **Review at the approval prompt.** Inspect proposed file content, diffs, and commands before approving them. Approve once, allow a session rule, or reject with instructions using **Tab**. **Ctrl+E** asks for a command explanation.
- **Four permission modes.** `manual` asks before file changes and commands; `edit` allows file changes; `auto`, the default, adds a reviewer for routine commands; `master` runs unattended. Cycle modes with **Shift+Tab** and keep rules per project.
- **Run it without a screen.** `zeroth -p "…"` answers one prompt for a script, a cron job or a CI step: the answer on stdout, or JSON with `--output-format`, and an exit status to branch on. Nothing waits for a reply; `--permission-mode` decides what runs unasked. See [headless mode](docs/cli/headless.md).
- **Checkpoints and rewind.** Enable checkpoints per directory to snapshot your working files in a separate store. **Esc Esc** returns to an earlier message and, when a checkpoint is available, restores the files with it.
- **Nothing runs on its own unseen.** When the app starts, it lists any new or changed hook, yours or a project's, and any new project MCP server, showing exactly what each runs. A hook runs only once you trust it, a server only once you enable it, and `/hooks` and `/mcp` change either later.
- **Credentials by placeholder.** Store a credential with `/secrets` and the agent can use its placeholder without seeing the value. See [how secrets are stored and used](docs/state/secrets.md).

### Give the agent the right tools

- **Subagents.** Delegate focused work to agents with their own context, model, and tool access. Built-in `general-purpose` and `explore` agents are editable, and you can open a running agent's session to chat, send follow-up work, or stop it — while the main agent does the same by each agent's id: checks its progress, sends a finished agent a follow-up with its context intact, or stops it.
- **Skills.** Add reusable `SKILL.md` folders, mention one with `$`, or browse them with `/skills`. Built-in `skill-creator` helps you write your own; `jina-reader` reads public webpages and PDFs through Jina Reader; `yt-dlp` guides media downloads with the separately installed CLI.

- **Web search and page reading.** `web_search` and `web_fetch` are native tools, not a skill: no key, no Python, and no setup. Search tries Parallel, AnySearch, Tavily and Bing in order, skips a source that reports a spent quota and comes back to it later, and `web_fetch` reads one URL as text a page at a time. Private, loopback and metadata addresses are refused unless a proxy or a rule says otherwise.
- **Public tunnels.** The built-in `flaredantic` skill guides you through exposing a local service via Cloudflare, Microsoft Dev Tunnels, or Serveo using the separately installed `flare` CLI. A tunnel makes the service publicly reachable; review what you expose.
- **MCP servers.** Connect tools over stdio, HTTP, or SSE, including remote servers with OAuth. Manage connections with `/mcp` or `zeroth mcp`.
- **Hooks and project instructions.** Use `hooks.json` to approve, reject, or adjust tool calls and supply context. `AGENTS.md` files carry your project's conventions; `/init` helps create one.

### Pick up where you left off

- **Saved conversations.** `/resume` opens a searchable session picker. Use `--continue` for the latest session in this directory or `--resume <id>` for a specific one.
- **Settings that stay with the project.** Model choices, permissions, skills, and appearance are remembered per directory, and every saved conversation resumes on the model it was using. Input history carries across sessions and is searchable with **Ctrl+R**.
- **Managed context.** `/context` shows what fills the context window and how much of it the prompt cache served, and `/usage` how much of each subscription's plan is left. `/compact` summarises long conversations, with automatic compaction as the window fills. A session scratchpad holds temporary scripts and notes outside your project.

### Make the terminal yours

- **Rich output.** Stream Markdown, tables, highlighted code, and collapsible reasoning blocks, with clickable URLs and file paths in supported terminals.
- **Inline images.** Paste screenshots with **Ctrl+V**. Images render natively in kitty, iTerm2, and sixel terminals, with a character-based fallback elsewhere.
- **Inspect the session.** **Ctrl+O** opens the expanded transcript; **Ctrl+D** shows the context sent to the model. Start a line with `!` for a shell command or use `@` to pick a file.
- **Live customisation.** Preview eleven themes with `/theme`, six mascots with `/mascot`, and nine spinner styles with `/spinner`. `/settings` brings the remaining controls together in a searchable menu.
- **Works with herdr.** In a [herdr](https://herdr.dev) pane, the sidebar shows when the agent is working, waiting on your approval, or done, and a restarted herdr resumes the conversation. There is nothing to set up.

## Commands & shortcuts

The essentials are always one keystroke away: `/` opens commands, and `?` in an empty composer shows shortcuts.

<details>
<summary><strong>Slash commands</strong></summary>

| Command | What it does |
| --- | --- |
| `/clear` | Clear the conversation |
| `/copy` | Copy the last response to the clipboard |
| `/export` | Copy or save the conversation as plain text |
| `/diff` | Review staged, unstaged, and untracked Git changes in a full-screen viewer |
| `/init` | Create an `AGENTS.md` contributor guide |
| `/compact` | Summarise the conversation to free up context |
| `/context` | See what fills the context window: tokens per category, the free space and the cache hit rate |
| `/usage` | See your subscriptions' usage limits: each window's share used and when it resets |
| `/resume` | Resume a saved chat |
| `/model` | Switch the active model |
| `/fast`, `/ultrafast`, … | Switch the model's speed tier — one command per tier its listing names, shown only on a model that lists it |
| `/login` | Sign in, add an API key, or connect a local model |
| `/settings` | Open the settings menu |
| `/theme` | Choose the colour theme |
| `/mascot` | Choose the banner mascot |
| `/spinner` | Choose the status spinner style |
| `/hooks` | Browse hooks and trust or stop trusting each one |
| `/skills` | Browse skills and enable or disable each one |
| `/mcp` | Manage MCP servers |
| `/secrets` | Store credentials the agent uses by placeholder, `<secret:NAME>`, without ever seeing the value |
| `/quit` | Exit the app |

</details>

<details>
<summary><strong>Keyboard shortcuts</strong></summary>

| Key | Action |
| --- | --- |
| `Enter` | Send, or hand a message to the running turn |
| `Tab` | Queue a follow-up turn |
| `Alt+↑` | Pull a queued message back to edit |
| `Shift+Enter` / `Ctrl+J` | Insert a newline |
| `↑` / `↓` | Recall input history |
| `Ctrl+R` | Search input history |
| `Esc` | Interrupt the running turn |
| `Esc Esc` | Step back to a previous message and rewind |
| `Ctrl+O` | Open the full transcript |
| `Ctrl+D` | Show the context sent to the model |
| `Ctrl+T` | Cycle the thinking mode |
| `Shift+Tab` | Cycle the permission mode |
| `Ctrl+B` | Move a running command or agent to the background |
| `↓` (empty composer) | Manage background shells and agents |
| `Ctrl+V` | Paste an image |
| `!` | Run a shell command |
| `@` | Pick a file path |
| `$` | Mention a skill |
| `/` | Open the command palette |
| `Ctrl+W` / `Ctrl+U` / `Ctrl+K` | Delete the previous word, to the start of the line, to the end of the line |
| `Ctrl+C` | Clear the draft, then quit |

</details>

<details>
<summary><strong>Command-line options</strong></summary>

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

Start a task directly, or continue the work from your last session:

```bash
zeroth "fix the failing test"
zeroth -c "now review the changes"
```

Or run it from a script, a cron job or CI, with no terminal at all:

```bash
zeroth -p "Run the test suite and summarize the failures" > report.md
git diff origin/main | zeroth -p --output-format json "Review this diff"
```

[Headless mode](docs/cli/headless.md) covers the output formats, exit codes and permission modes.

Run `zeroth mcp --help` for MCP server management commands, and `zeroth update` to install the newest release over the binary you are running.

</details>

## Where things live

Your sign-ins, saved sessions, input history, and per-directory settings live under `~/.zeroth/`. Keep shared extensions there, or add project-specific ones alongside your code.

If `~/.alter-zero/` exists from a previous install, the first launch copies `config.json`, `providers.toml` and `.env` out of it once. Sessions, history, caches, skills and agents stay behind, and the old directory is never modified — the previous app keeps working from it.

<details>
<summary><strong>Configuration and extension paths</strong></summary>

| Location | Contents |
| --- | --- |
| `~/.zeroth/.env` | Keys and tokens saved through `/login` |
| `~/.zeroth/agents/` and `~/.zeroth/skills/` | Your shared agent definitions and skills |
| `~/.zeroth/hooks.json` and `~/.zeroth/mcp.json` | Your shared hooks and MCP servers |
| `.alter-zero/` in a project | Project-specific `agents/`, `skills/`, `hooks.json`, and `mcp.json` |
| `AGENTS.md` | Project instructions read by the agent |

Claude Code-style `.claude/skills/` and `.mcp.json` files are also supported. Hooks, yours and a project's, and a project's MCP servers run only after you approve them when the app starts.

</details>

## License

Zeroth is released under the [Apache License 2.0](LICENSE). Copyright 2026 the Zeroth contributors. Upstream attribution is recorded in [NOTICE](NOTICE).

## Acknowledgements

Zeroth is made possible by the people, projects, and communities behind it:

- [Claude Code](https://claude.com/claude-code) and [Codex CLI](https://github.com/openai/codex) for the interaction patterns that helped shape Zeroth.
- [ratatui](https://ratatui.rs) and [crossterm](https://github.com/crossterm-rs/crossterm) for the foundation of the terminal UI.

See the additional [technical credits](docs/credits.md).
