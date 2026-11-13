# system prompt 里的环境上下文 —— 设计

日期：2026-07-19

## 目标

让 “Zeroth” agent 对它在何处、何时运行有**上下文感知**：当前**日期**、**os**、
**user** 与 **cwd** 每个 session 都随 system prompt 一起送进去。没有它们，模型
会猜日期、假设平台、分不清自己是不是 root，也完全不知道自己的
`bash`/`read`/`write`/`edit` 工具作用在哪个目录。

这个块写在 [`prompts/environment.md`](../../prompts/environment.md) 里，作为
persona 的 `## Environment` 一节：

```
## Environment

Date {date}
OS {os}
User {user}
CWD {cwd}
```

`{date}`/`{os}`/`{user}`/`{cwd}` 这些占位符在运行时被填上，例如：

```
## Environment

Date Sunday 2026-07-19
OS linux (Ubuntu 24.04.4 LTS)
User linuztx
CWD /home/linuztx/zeroth
```

## 它在 prompt 里的位置

真实 backend 发出的完整 system prompt 是三个块，按顺序 —— headless 运行里是
四个：

```
persona        prompts/zeroth.md   (who you are)
environment    prompts/environment.md  (where/when you are, as which user)   ← this doc
scratchpad     prompts/scratchpad.md   (where your scratch goes, docs/tools/scratchpad.md)
headless       prompts/headless.md     (nobody is watching — zeroth -p only, docs/cli/headless.md)
```

先是 `persona → environment`，由 `augment_with_environment` 拼起来；然后当
session 有 scratchpad 目录时，`augment_with_scratchpad` 追加 scratchpad 块；最后
—— 只在 headless 运行里 —— 由 `augment_with_headless` 追加那个块：没有人能作答
或批准、需要批准的调用被拒绝、回合结束运行就结束。没有别的了：工具 schema 自带
各自的能力细节，所以 `LlmBackend::configure` 不追加工具说明（已退役的
`prompts/tools.md` 把那些 token 在每次请求上又花了一遍）。Ctrl+D 的上下文调试
视图显示整个拼好的 prompt，所以环境块在那里也看得见。

## 为什么是这个形状

和 Ctrl+O 的时间戳时钟（`docs/ui/timestamps.md`）一样，读一次墙钟和一次 CWD
没法住在纯的、可确定性测试的库里。所以拆法是这样的：

1. **值在 I/O 边界收集。** `tui::host` 读日期（`local_date` —— `chrono::Local`，
   `%A %Y-%m-%d`）、os（`os_context` —— `std::env::consts::OS`，在 Linux 上再
   用 `/etc/os-release` 的发行版信息补充，例如 `linux (Ubuntu 24.04.4 LTS)`）、
   用户（`user_context` —— 有效 uid 及其名字，[见下](#用户那一行)）；cwd
   （`std::env::current_dir`）在启动时就已经在手上了。
   `tui::config::system_prompt` 把这些**一次**折进 prompt。循环在 `/model`
   切换时重建的每个 backend 都通过 `system_prompt.clone()` 继承这个块，所以
   注入点只有一个 —— 而 `tui::config::prompt_context` 用同样的读取渲染同一个
   块，供一个定义会替换 persona 的 subagent 使用（`docs/agents/subagents.md`）。

2. **格式化是纯的，且有单元测试**（`llm::backend`）：
   - `render_environment(date, os, user, cwd)` 填模板 —— 每个 `{token}` 都被
     替换，没有一个留下。
   - `augment_with_environment(base, date, os, user, cwd)` 在空行之后
     把渲染好的块追加到基础 prompt 后面。
   - `os_release_name(contents)` 解析 `/etc/os-release`，优先 `PRETTY_NAME`，
     其次 `NAME`（去掉引号），所以发行版补充这一点不用读真实文件就能测。
   - `passwd_name(contents, uid)` 与 `user_label(uid, name)` 是用户那一行的
     解析与它的措辞（[见下](#用户那一行)）。

3. **空白的基础保持空白。** “`ZEROTH_SYSTEM_PROMPT` 为空 → 没有 system 消息”
   这条契约（`docs/architecture/context.md`）被保住了：
   `augment_with_environment` 对空白的基础原样返回，所以 `configure` 仍然把它
   降成 `None`。任何非空的 prompt —— 默认 persona *或*自定义的
   `ZEROTH_SYSTEM_PROMPT` —— 都会拿到环境块，因为上下文感知与 persona 是正交的。

## 用户那一行

日期：2026-09-29

agent 是不是以 root 运行，决定了它到底需不需要 `sudo`，而一个靠猜的模型两种
情况都会猜错：在一个没装 `sudo` 的 root 容器里用它，或者以普通用户身份裸跑
`apt install`。另外，一个会索要密码的 `sudo` 本身就是一个只有 `bashsend` 能
回答的 prompt（`docs/tools/bash-tools.md`），所以事先知道就能省掉一次往返。
一行就定了，以事实陈述，不附带任何指令，用这个块自己的简洁风格：

| 进程                      | 那一行             |
|---------------------------|--------------------|
| uid 0，名字为 `root`      | `User root`        |
| uid 0 但名字是别的        | `User toor (root)` |
| 任何其他 uid              | `User linuztx`     |
| 一个没有名字的 uid        | `User uid 1000`    |

**uid 决定是不是 root；名字只是给它贴标签**（`user_label`）。一个换了个名字的
root 账号 —— BSD 的 `toor`，或 `su` 留下的 `$USER` —— 仍然写 `(root)`，而一个
非特权进程里过期的 `$USER=root` 永远不能让那一行声称自己是 root：那个 uid 读作
`uid 1000`，`docker run --user 1000` 这种不指名账号的情况也一样。在非 unix 上
没有 uid，所以名字按给出的写，否则就是 `unknown`。

这些值从哪来（`tui::host::user_context`）：

- **有效 uid**，`host::process_uid` —— 权限检查所依据的 uid，也是 `whoami`
  报告的那个 —— 通过 rustix 那个安全的 `geteuid` 读取，与命名 session 临时根
  目录用的是同一个答案。它过去取的是 `/proc/self` 的所有者，而在 `/proc` 不存在
  的环境（macOS、没有它的沙箱）里那个答案总是 0：放在这里就等于声称自己是 root。
- **名字**来自 `/etc/passwd`（`passwd_name`：第一条带该 uid 的条目，与
  `getpwuid` 的答案一致；注释和 NIS `+`/`-` 兼容条目跳过），逐行流式读取并做
  有损解码，所以一个大文件只花一行的内存，而一个老编码的 GECOS 字段也藏不住
  它后面的条目。文件里没有的账号 —— 每一个普通的 macOS 账号、目录服务
  登录 —— 都回退到 `$USER`，再 `$LOGNAME`，再 `$USERNAME`。这个 crate 禁止
  `unsafe`，所以没有 `getpwuid` 调用可以依靠。

## 测试

- `llm::backend`（纯）：`render_environment` 填满每个占位符且不留 `{`，用户
  单独占一行；`augment_with_environment` 把块追加到基础之后、对空白的基础不做
  改动，并且 —— 与 `configure` 组合起来 —— 完整产出 persona →
  environment 的 prompt，没有任何工具后缀。`passwd_name` 找到某个 uid 的账号、
  在两条同 uid 的条目中取第一条、跳过注释、空行与兼容条目，并把 uid 字段整体
  匹配；`user_label` 覆盖上表的每一行，包括过期 `$USER=root` 的防护。
- `tui::host`（边界）：日期/os/用户的收集 —— 包括 `os_context` 读真实的
  `/etc/os-release`、`user_context` 取真实 uid 与 `/etc/passwd` —— 由
  `smoke.sh` Phase 127 验证：一个本地替身顶替 OpenAI 兼容 provider 并记录
  请求，该阶段从 wire 上把这个块读回来 —— 它的 `User` 行与 `id -un` 对照、cwd
  与启动目录对照、四行的顺序 —— 然后在 Ctrl+D 里找到同一行。那个活的
  OpenRouter 检查（`live_environment_context_reaches_the_model`）让一个真实模型
  从 prompt 里报告它的 os、user 与 cwd。

## 已知局限

- 日期/os/用户/cwd 在**session 启动**时（以及 `/model` 重建时）捕获，不是每回合
  一次 —— 一个跨过午夜的 session 保留启动那天的日期，而某个 tool call 做的
  `cd` 不会被反映出来。这与 session 作用域的时钟一致，对一个终端 session 来说
  够用。
- 用户是进程运行时的那个。在 `bash` 调用里执行 `sudo` 或 `su` 改变的是那条命令
  以谁的身份运行，不是这一行。
- 一个只有目录服务知道、且没有设 `$USER`、`$LOGNAME` 或 `$USERNAME` 的账号，
  读作 `uid N`：名字来自 `/etc/passwd`，不是 NSS。
