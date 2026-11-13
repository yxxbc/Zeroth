# miyu 的路径监狱与文件边界研究

> 研究对象：`~/Projects/miyu` 当前 `main`（2026-11-13，含 A16/A17/A18 的权限改造），以及旧版全屏 TUI 分支 `origin/proto/tui-demo`。
> 方法：只读源码、设计文档和测试，不运行真实攻击。文中的“已实现 / 仅设计 / 已废弃”按当前 `main` 的代码判断，不按设计文档的旧段落判断。
> 结论先行：miyu 的路径限制不是单一“路径监狱”，而是三层——核心进程里的文件工具边界（`miyu-fs` + `miyu-session::guard`）、命令的 OS 沙盒（`miyu-sandbox`）、客户端侧的路径解析（旧 TUI）。`miyu-policy` 本身不做路径校验。

## 0. 结论先行

1. **`miyu-policy` 没有路径监狱**。它是策略快照与提示词文本的组装层，`attached_path`、`open_path`、`path_only` 只是“附件消息里怎么写路径”的模板（`crates/miyu-policy/src/lib.rs:1-10`、`crates/miyu-policy/src/path_only.rs:1-14`）。真正的路径校验在 `crates/miyu-session/src/guard.rs` 与 `crates/miyu-fs/`。
2. **当前 `main` 的 owner 会话只有“只读 / 完全放开”两级**，`workspace` 这一级已在 A16 删除（`docs/designs/11-权限与沙盒.md:5`、`crates/miyu-kernel/src/event/session.rs:259-302`）。因此“工作区外写要问人”主要是旧设计（A15 及以前），当前代码里保留下来的是**外部身份会话的 confined 工作区**与**文件工具对数据根 / 私密目录 / 技能目录的边界**。
3. **文件工具不依赖 OS 沙盒**，它们在核心进程里自己守边界：路径先 `canonicalize`，再查边界表，再用“判过的真实位置集合”做二次核对，最后用逐层不跟链接的 `openat2` / `O_NOFOLLOW_ANY` 打开（`docs/designs/11-权限与沙盒.md:242-257`、`crates/miyu-tool/src/checked.rs:34-67`、`crates/miyu-fs/src/nofollow.rs:14-60`）。
4. **`bash` 没有路径参数可校验**。`Tool::targets` 默认返回空（`crates/miyu-tool/src/lib.rs:199-203`），`shell` 不重写它；所以命令一旦要限制，只能靠 `miyu-sandbox` 的规格（能写白名单、藏起来、只准读名单）（`crates/miyu-sandbox/src/spec.rs:12-30`、`crates/miyu-session/src/sandbox.rs:103-140`）。
5. **边界表是“最深的那一片算”**：技能目录 > 数据根 > 私密目录 > 工作区 / 加进来的目录 > 临时目录 > 系统与工具链目录 > 边界以外（`crates/miyu-fs/src/boundary.rs:164-213`）。数据根在任何级别都不可读写；技能目录可读不可写。
6. **路径算法覆盖了 `~`、相对路径、`..`、符号链接、缺失文件、大小写和 Windows 盘符**，但有几个明确边界：大小写折叠只在 macOS/Windows；没有 Unicode NFC/NFD 归一化；Windows 只保证最后一层 reparse point 不跟，路上仍会跟（`docs/blueprint/fs.md:139`）；`authorized_keys` 没有专门规则，只随 `~/.ssh` 私密目录处理。
7. **Zeroth 已经有 write/edit 的路径监狱**：`PermissionGate::allows` 里的 `file_change_inside` 把 `workspace` + `scratchpad` 作为允许根，auto/edit 下工作区外的文件变更仍要问人（`src/permission.rs:1171-1205`、`1284-1303`）。真正缺的不是“加一个开关”，而是**执行侧的二次核对**与**read/bash 的边界**。
8. **最紧急的 P0 是 write/edit 的符号链接 TOCTOU**：`run_write` / `run_edit` 在权限检查之后直接 `std::fs::write`（`src/llm/exec.rs:1471`、`1502`），而 `file_change_inside` 对“悬空链接”用 `exists()` 判断会落到 false，随后把链接本身当作工作区内路径放行；执行时 `fs::write` 会跟随链接写到工作区外。先修这个，再谈扩展。

## 1. miyu 的威胁模型：为什么需要路径限制

### 1.1 资产与对手

| 资产 | 为什么重要 | 相关面 |
|---|---|---|
| 工作区源码 / 项目配置 | 项目本身的完整性 | `write`、`edit`、`bash`、glob/grep |
| 家目录中的凭据 | `~/.ssh`、`~/.aws`、`~/.config/gh`、浏览器配置、`.netrc` | 文件工具、命令沙盒、附件读取 |
| Miyu 数据根 | 本机令牌、会话日志、密钥；读到令牌可冒充本人 | 所有工具、命令沙盒 |
| shell 启动文件 / git 钩子 | 下次在沙盒外执行时获得代码执行 | `write`/`edit`/`bash` |
| 会话与审计记录 | 谁在什么时候读了/写了什么 | 内核事件、日志 |

miyu 的威胁模型把对手分成：沙盒里的命令、被提示注入的模型、外部身份（群聊/陌生人）、恶意扩展或 MCP、同机其他系统用户（`docs/designs/24-威胁模型.md:17-29`）。总规矩是“权限由代码判定，提示词里的任何文字都不授予权限”（`docs/designs/24-威胁模型.md:9-13`）。

### 1.2 具体防什么

| 攻击面 | miyu 的防线 | 当前实现状态 |
|---|---|---|
| 提示注入诱导写 `~/.bashrc` / `.git/hooks` / `authorized_keys` | 文件工具查边界；命令进 OS 沙盒 | **部分**：read-only 下写全拒；full 下文件工具允许工作区外写；`.git/hooks` 在 A17 后没有专门保护（`docs/designs/11-权限与沙盒.md:7`、`docs/blueprint/fs.md:92`） |
| 提示注入诱导读 `~/.ssh` / `.env` | 文件工具查私密目录；命令沙盒藏私密目录 | **部分**：read-only 下文件工具对私密目录“问人”，full 下放行；命令只在沙盒里藏（`crates/miyu-fs/src/boundary.rs:18-20`、`crates/miyu-session/src/guard/levels.rs:39-48`） |
| 写工作区外的文件 | 边界表 + 文件工具；旧设计还问人 | **已实现但有条件**：owner 的 full 级别不限制工作区外写；read-only 全拒（`crates/miyu-session/src/guard/levels.rs:41-48`） |
| 符号链接逃逸 | `canonicalize` + 判过的真实位置 + 逐层不跟链接打开 | **已实现**（`crates/miyu-tool/src/checked.rs:1-5`、`crates/miyu-fs/src/nofollow.rs:1-5`） |
| 绝对路径 / `..` | 相对路径接 cwd，`~` 接 home；缺失路径的 `..` 拒绝 | **已实现**（`crates/miyu-fs/src/resolve.rs:42-55`、`99-130`） |
| 外部身份越界 | confined 会话：文件工具只允许工作区/加进来的目录，命令只允许关进工作区的沙盒 | **已实现**（`crates/miyu-session/src/guard/confined.rs:38-60`、`crates/miyu-session/src/sandbox/confined.rs:102-149`） |
| 沙盒命令写会在沙盒外执行的文件 | 写白名单；数据根、私密目录藏起来 | **已实现**，但受平台能力限制（见 §2.7） |
| 网络外发 | 沙盒不管网络（明确接受） | **不防**（`docs/designs/11-权限与沙盒.md:123`、A5） |

### 1.3 已实现 / 仅设计 / 已废弃

| 机制 | 状态 | 证据 |
|---|---|---|
| 文件工具的边界表、`canonicalize`、逐层不跟链接 | 已实现 | `crates/miyu-fs/src/boundary.rs`、`resolve.rs`、`nofollow.rs`、`open.rs` |
| 数据根不可读写 | 已实现 | `crates/miyu-fs/src/boundary.rs:16-17`、`164-213` |
| 私密目录清单与“问人/藏起来” | 已实现 | `crates/miyu-fs/src/private.rs:15-74`、`private/shipped.rs` |
| 外部身份 confined 工作区 | 已实现 | `crates/miyu-session/src/guard/confined.rs:1-3`、`sandbox/confined.rs:1-6` |
| 命令的 OS 沙盒 | 已实现（Linux Landlock、macOS Seatbelt、Windows MXC） | `crates/miyu-sandbox/src/spec.rs`、`wrap.rs`、`linux/rules.rs`、`macos/profile.rs` |
| owner 的“工作区”权限级 + 越界问人 | **已废弃**（A16） | `docs/designs/11-权限与沙盒.md:5`、`crates/miyu-kernel/src/event/session.rs:294-301` |
| 受保护路径（`.git/hooks`、`.bashrc` 等只禁写） | **仅设计 / 已废弃**（A17 移除） | `docs/designs/11-权限与沙盒.md:7`、`docs/blueprint/fs.md:92` |
| Windows 上路上 reparse point 不跟 | **未实现** | `docs/blueprint/fs.md:139`、`docs/blueprint/fs.md:316` |
| `bash` 参数级路径校验 | **不适用**（没有参数可校验） | `crates/miyu-tool/src/lib.rs:199-203` |

## 2. 规则与算法

### 2.1 允许根（Places）与区域（Zone）

`Places` 定义一次判定的全部“地方”（`crates/miyu-fs/src/boundary.rs:29-49`）：

| 字段 | 含义 | 当前 main 的用法 |
|---|---|---|
| `workspace` | 这一轮的工作区 | confined 会话的写边界；边界表里的 `Writable` |
| `dirs` | 加进来的目录，每个都照工作区算 | confined 会话可写；边界表里 `Writable` |
| `data_root` | Miyu 数据根 | 任何级别都 `Forbidden` |
| `knowledge` | 数据根里的知识库目录 | 命中时换一句拒绝文案 |
| `temp` | 临时目录 | 边界表里 `Writable` |
| `readable` | 系统目录、工具链目录 | `Readable` |
| `shelves` | 数据根里单独放开的技能文件夹、附件目录 | `Shelf`：可读、写拒 |
| `private` | 别的程序的私密目录 | `Private`：read-only 读要问人，full 放行 |
| `own` | 家目录里 XDG 的 pictures/videos/documents/downloads | 照工作区算 |

区域判定是“最深的那一片算”（`Boundary::zone`，`crates/miyu-fs/src/boundary.rs:164-213`）：`shelves` → `Shelf`；`data_root` → `Forbidden`（除非工作区/`own` 落在数据根里且这一处在工作区里）；工作区、`own`、`dirs` → `Writable`；`private` → `Private`（工作区落在里面时让位）；`temp` → `Writable`；`readable` → `Readable`；其余 `Outside`。

注意：`Boundary` 里的每一片在构造时都 `canonicalize`，不存在的那一片直接不算（`crates/miyu-fs/src/boundary.rs:140-157`）。所以 `/lib32` 这类路径在机器上不存在时不会误判。

### 2.2 工作区的定义

工作区不是“用户家目录”，而是“这一轮能随便写的根”：平常就是 `cwd`（`crates/miyu-fs/src/wide.rs:32-44`）；`cwd` 太宽时退回会话默认工作区（`wide.rs:16-30`），外部身份会话即使 `cwd` 是 `~` 也会退回自己的默认工作区（`crates/miyu-endpoint/src/sessions/places.rs:115-125`）。旧版 TUI 的开会话路径先 `fs.realpath` 再 `fs.list`，但真正的边界仍由核心判（`origin/proto/tui-demo:tui-demo/src/core/workspace.rs:10-29`）。

当前 owner 的 `Effective::Full` 下，文件工具的 `mark` 对 `Outside` 也放行（`crates/miyu-session/src/guard/levels.rs:41-48`），所以“工作区”在 owner 的 full 级别不是写边界；它仍是 confined 会话的写边界、边界表分区和放行范围的基础。

### 2.3 规范化：词法还是真实位置

miyu 选择“整条换成真实位置”，不是纯词法拼接（`crates/miyu-fs/src/resolve.rs:42-55`）：

1. `~` 只认 `~`、`~/`（Windows 上还有 `~\`），接家目录；`~alice`、`a/~` 不展开（`resolve.rs:82-97`）。相对路径接 `cwd`，绝对路径原样。
2. 整条 `std::fs::canonicalize`：符号链接、Windows 目录联接都按指向的地方算，`.`、`..` 消掉。
3. 不存在时：先看这一条自己是不是悬空链接 → `DanglingLink`；否则找最近存在的上级，`canonicalize` 后接上剩余几段；剩余几段里有 `..` → `ParentOfMissing`（`resolve.rs:99-130`）。
4. `resolve_itself` 用于 `trash`：上级照 `resolve`，最后一段原样接上，删的是链接本身（`resolve.rs:64-80`）。

关键点：**不存在的新文件也走真实位置**，而不是把用户给的字符串直接拼到工作区后面。这样 `workspace/../outside/new.txt` 不会因为“目标还不存在”而被当作工作区内的路径。

### 2.4 符号链接与 TOCTOU

miyu 用了三道互相独立的检查：

1. **判定时换真实位置**：`Guard::real` 对每个 `Target` 调 `resolve` / `resolve_itself`（`crates/miyu-session/src/guard.rs:422-431`）。
2. **判定结果交给工具核对**：判过的真实位置收集进 `checked`，工具调 `Call::real` 时再换一次，不在集合里就报 `Moved`（`crates/miyu-session/src/guard.rs:209-224`、`crates/miyu-tool/src/checked.rs:34-67`）。
3. **打开时逐层不跟链接**：读 `openat2(RESOLVE_NO_SYMLINKS)` / `O_NOFOLLOW_ANY` / 逐层 `openat`；写先打开父目录，临时文件、改名都相对父目录（`crates/miyu-fs/src/nofollow.rs:14-94`、`replace.rs:120-163`）。

这覆盖“判定前就是链接”“判定后链接被换掉”“父目录被换掉”。第 2 道是 2026-10-22 漏洞排查后补的（`docs/designs/11-权限与沙盒.md:257`）。边界：Windows 只对最后一层带 `FILE_FLAG_OPEN_REPARSE_POINT`，路上仍会跟（`docs/blueprint/fs.md:139`）；悬空链接在 `resolve` 阶段直接报错；`trash` 判链接本身，写工具判链接指向的位置，两者不能混用。

### 2.5 大小写、Unicode、Windows 盘符与 UNC

`within(path, root)` 是包含判断的唯一入口（`crates/miyu-fs/src/boundary.rs:269-295`）：按 `Path::components()` 一段一段比，`root` 自己也算；macOS/Windows 上每段 `to_lowercase()` 再比，Linux 按原样。`/a/bc` 不会被当作 `/a/b` 里面，`Work` 和 `work` 在 macOS/Windows 上不会绕过。没有做 Unicode NFC/NFD 归一化；不同归一化形式最终仍会落到 `canonicalize`，字符串层面的放行范围对不上时结果是“多问一次”，不是“多放行”。

Windows：`canonicalize` 返回 `\\?\C:\…`，`Guard::real_home` 把家目录也换成真实位置，避免一边带前缀、一边不带而永远比不上（`crates/miyu-session/src/guard.rs:53-56`）；系统目录来自 `%SystemRoot%`、`%ProgramFiles%`、`%ProgramFiles(x86)%`（`crates/miyu-fs/src/boundary.rs:114-123`）；沙盒只读盘根只列固定盘（`crates/miyu-sandbox/src/bin/miyu-sandbox/windows/drives.rs:1-25`）。没有单独的 UNC 规则，最终由 `canonicalize` 和边界表决定；拿不准时落在 `Outside`。

### 2.6 包含判断与放行范围

- 边界判定用 `strip_prefix` / `starts_with`（`crates/miyu-fs/src/boundary.rs:168-190`），按段比。
- 放行规则（用户选“本会话都允许”）记录的 `scope` 是“目录本身（或文件所在目录）连同下面”；如果这个目录是家目录本身或更上层，只放这一条本身，避免一条规则把 `~/.ssh` 也放行（`crates/miyu-session/src/guard/asking.rs:62-76`）。
- 匹配时同样按 `Path::starts_with`，并且私密目录里的目标只认“范围本身也在私密目录里”的规则（`asking.rs:109-123`）。
- 放行范围是会话内的，不写盘；miyu 明确不做“这个工作区以后都允许”（A10）。

### 2.7 只读 / 只写 / 命令沙盒

文件工具的判定表在 `mark`（`crates/miyu-session/src/guard/levels.rs:39-48`）：

| 区域 \ 级别 | 完全放开 | 只读 |
|---|---|---|
| `Forbidden` | 拒 | 拒 |
| `Shelf` 且写 | 拒 | 拒 |
| 其他区域、写 | 放行 | 拒 |
| `Private`、读 | 放行 | 问人 |
| 其他区域、读 | 放行 | 放行 |

命令沙盒的规格是另一套（`crates/miyu-sandbox/src/spec.rs:12-30`）：

- `write`：能写的目录/文件白名单；空 = 全盘只读（`/dev/null` 例外）。
- `hidden`：读写都不行；数据根、私密目录。
- `read`：只准读这些；不写 = 整盘能读。这是外部身份会话用的。
- `reveal`：`hidden` 里单独放开只读的技能文件夹、附件目录。

当前 main 的 owner：只读时 `write = []`、`hidden = 数据根 + 私密目录`、`read = None`，命令全盘只读、读不到数据根/私密目录（`crates/miyu-session/src/sandbox.rs:113-140`）；完全放开时 `for_call` 直接返回 `None`，不进沙盒（`sandbox.rs:113-116`）。外部身份 confined 会话：`write = cwd + dirs + cwd/.tmp`（只读时为空），`hidden = 数据根`，`read = 系统目录 + 自己的目录 + 技能目录`，`TMPDIR` 指到 `cwd/.tmp`，`HOME` 指到工作区（`crates/miyu-session/src/sandbox/confined.rs:108-148`），读写都关进工作区，连完全放开也不看级别。

### 2.8 `bash` 的特殊性

`bash` / `shell` 不报路径（`crates/miyu-tool/src/lib.rs:199-203`），因为路径校验对 shell 根本不成立：命令可以 `cd`、用变量、命令替换、通配、脚本、解释器，静态看参数无法确定最终碰哪个文件；同一段文字可以 `cat` 也可以 `tee`，可以 `curl | sh`，可以运行 `make` 让 Makefile 决定写哪里。因此 miyu 在需要限制的级别（只读、外部身份）把 shell 放进 OS 沙盒，用内核强制“能写哪里、藏起哪里”；文件工具才用路径表。

这也解释了为什么 `miyu-policy/src/hazard.rs` 只做“高危命令”识别（删根、删家目录、`git push --force`、`curl | sh`、容器交宿主等，`crates/miyu-policy/src/hazard.rs:20-60`），而不是路径白名单。高危是“防幻觉”的第二道，不是路径边界。

### 2.9 各工具怎么报路径

| 工具 | `targets` 报什么 | 写? | 证据 |
|---|---|---|---|
| `read` | `file_path` | 否 | `crates/miyu-basesystem/src/read.rs:101-110` |
| `write` / `edit` | `file_path` | 是 | `write.rs:65-74`、`edit.rs:129-138` |
| `trash` | `file_path`，`itself = true` | 是 | `crates/miyu-basesystem/src/trash.rs:69-78` |
| `glob` / `grep` | 搜的目录 / 文件 | 否 | `glob.rs:76-85`、`grep.rs:142-151` |
| `shell` | 不报（空） | — | `crates/miyu-tool/src/lib.rs:199-203` |

`glob` / `grep` 的目录参数会被判，但“从上面搜下来会走进数据根/私密目录”的问题由走目录的 `Fence` 自己挡：不跟链接、不进 VCS 目录、不进数据根和私密目录（`crates/miyu-basesystem/src/walk.rs:1-14`、`64-124`、`174-200`）。

扩展工具可以在 `provide` 规格里声明 `paths`：参数名、读/写；核心照它生成 `Target`，与内置工具走同一张判法表（`crates/miyu-endpoint/src/provide/declared.rs:1-8`、`21-42`、`87-113`）。MCP 工具没有这套声明，它走的是“每次调用问人 / classifier”的路（`docs/designs/24-威胁模型.md:22`）。

### 2.10 旧版 TUI 的“客户端侧”路径解析

旧分支的 `local.rs` 只做显示/打开用的路径换算：去 `file://`、展开 `~/`、绝对路径原样、相对路径接 `cwd`，没有边界检查（`origin/proto/tui-demo:tui-demo/src/local.rs:20-32`）；`open.rs` 只允许 `http`、`https`、`file` 和本机存在的文件（`origin/proto/tui-demo:tui-demo/src/open.rs:38-67`）；拖入文件只认绝对路径、`~` 展开、解 `%XX`（`origin/proto/tui-demo:tui-demo/src/input/dropped.rs:104-118`）；换工作区先 `fs.realpath` 再 `fs.list`，然后交给 `session.create`（`origin/proto/tui-demo:tui-demo/src/core/workspace.rs:10-29`）。这些是**客户端侧**的便利逻辑，不是安全边界：真正的写读仍由核心的 guard / fs 判。

## 3. 执行点：在哪一层拦截

miyu 的执行顺序：

```
模型 tool call
  → 工具 targets() 报路径（不碰磁盘）
  → Guard::judge(cwd, dirs, permission, grants)
       → resolve 每个 target
       → Boundary::zone
       → confined / mark / hazard / install
       → Allow / Ask / Deny + checked 真实位置
  → 扩展守卫（可选，最严者胜）
  → 内核 approval：Ask 无人值守 → Deny；只读写 → Deny
  → 执行器把 checked 交给工具
  → 工具 Call::real 二次核对
  → 文件工具 nofollow 打开 / 命令进 miyu-sandbox
```

落点：

| 层 | 代码 | 作用 |
|---|---|---|
| 工具声明 | `crates/miyu-tool/src/lib.rs:199-203`、`crates/miyu-basesystem/src/*.rs` | 只报路径，不判 |
| 权限策略 | `crates/miyu-session/src/guard.rs:209-224`、`339-404` | 换真实位置、查边界、出 verdict、记 `checked` |
| 区域判定 | `crates/miyu-fs/src/boundary.rs:164-213` | `Writable` / `Readable` / `Forbidden` / `Private` / `Shelf` / `Outside` |
| 级别判定 | `crates/miyu-session/src/guard/levels.rs:29-48` | 只读 / 完全放开 |
| 扩展守卫 | `crates/miyu-session/src/actor/guarding.rs:153-169` | 只能收紧，不能放宽 |
| 内核确认 | `crates/miyu-kernel/src/session/approval.rs:51-92` | Ask 的落点；无人值守、只读写直接拒 |
| 工具执行 | `crates/miyu-tool/src/checked.rs:34-67` | 二次核对判过的真实位置 |
| 打开/写入 | `crates/miyu-fs/src/nofollow.rs`、`replace.rs` | 不跟链接、原子替换 |
| 命令沙盒 | `crates/miyu-sandbox/src/wrap.rs:35-48` | `miyu-sandbox run --spec <json> -- <program>` |

拒绝与询问的呈现：

- `Deny` 写一句给模型看的话，比如“在数据根里”“路径换不成真实位置”“只读时不能写”（`crates/miyu-session/src/guard.rs:431-440`、`crates/miyu-policy/src/guard.rs:20-59`）。
- `Ask` 带 `detail`：每条路径的真实位置、是读是写、区域（`private` / `outside`），以及可选的放行规则 `rule`（`crates/miyu-session/src/guard/asking.rs:26-59`）。
- 外部身份会话越过工作区：直接 `Deny`，不产生 `Ask`，因为没人能确认（`crates/miyu-session/src/guard/confined.rs:43-53`）。
- 审计：内核把 `tool.result` 记成 `Denied` 或发起 `approval_requested`；设计上所有操作进日志（`docs/designs/24-威胁模型.md:24`）。没有单独的“路径审计”结构，路径信息在请求的 `detail` 和结果里。

## 4. 对 Zeroth 的具体建议

### 4.1 Zeroth 现状：路径监狱已经存在

Zeroth 的 `PermissionGate::allows` 已经有一段路径监狱（`src/permission.rs:1284-1303`）：

```rust
// 文件变更落在 workspace 和 scratchpad 之外，即使在 auto/edit 也要走提示
if covered
    && inner.rules.mode != PermissionMode::Master
    && matches!(request.kind, PermissionKind::Write | PermissionKind::Edit)
    && inner.workspace.is_some()
    && !file_change_inside(&inner, &request.target)
{
    return false;
}
```

`file_change_inside`（`src/permission.rs:1171-1205`）的规则：

- 目标里出现任何 `..` 组件：直接 false（拒绝之外的路径）。
- 目标绝对路径就用它；相对路径分别接 `workspace` 和 `scratchpad`。
- 已存在的目标 `canonicalize`；不存在的目标 `canonicalize(parent) + file_name`。
- 根也 `canonicalize`，用 `starts_with` 且 `!= root` 判断“严格在里面”。

启动时由 `bootstrap` 把 `cwd` 和 scratchpad 接上（`src/tui/bootstrap.rs:261-274`）。headless 还会把 gate 设为 unattended（`src/tui/bootstrap.rs:272-274`、`src/permission.rs:1432-1453`），所以非 master 模式下工作区外的写最终会被拒绝，而不是弹一个没人能回答的窗；master 仍按“完全放开”语义放行。

### 4.2 缺口

| 缺口 | 现状 | 风险 | 优先级 |
|---|---|---|---|
| 执行侧符号链接 TOCTOU | `file_change_inside` 判完，`run_write`/`run_edit` 直接 `std::fs::write`（`src/llm/exec.rs:1471`、`1502`） | 工作区内存在指向工作区外的链接，或检查后被换成链接，可写到任意位置 | **P0** |
| 悬空链接 | `candidate.exists()` 对悬空链接为 false，于是按 `parent + name` 判成“里面” | `fs::write` 跟随悬空链接，在工作区外创建文件 | **P0** |
| `read` 无边界 | `permission_request` 对 `read` 返回 None（`src/llm/approval.rs:61-175`） | 模型可直接读 `~/.ssh/id_rsa`、`.env`、云凭据 | **P1** |
| auto 的只读命令 fast path | `read_only::is_read_only` 把 `cat`/`head`/`tail` 等列为 ANY_ARGS（`src/read_only/mod.rs:200-205`），auto 下直接放行（`src/llm/approval.rs:271-279`） | `cat ~/.ssh/id_rsa` 不需要任何确认，内容进入上下文 | **P1** |
| 工作区内的高危路径 | 没有 `.git/hooks`、`.git/config`、`.bashrc`、`authorized_keys` 等保护名单 | 工作区就是家目录时，auto 可直接改启动文件/钩子 | **P1** |
| 显式临时目录 | 只允许 `workspace` + `scratchpad` | 模型按提示写 `/tmp/foo` 会被问；降低可用性，可能诱导用户放宽 | **P1** |
| 大小写/Unicode | `file_change_inside` 用 `Path::starts_with`，macOS/Windows 不折叠 | 多数情况是“多问一次”（fail closed），但不是明确的统一策略 | **P2** |
| `bash` 无 OS 沙盒 | `run_bash` 直接用当前用户跑；没有 Landlock/Seatbelt/AppContainer | auto 的 classifier 不是强制边界，注入可诱导写工作区外 | **P2** |
| `web_fetch`/MCP 出站 | 只有 classifier/规则；没有私网地址、`file://`、重定向的统一拦截 | 读到的密钥可经 URL/请求外发 | **P2** |
| Master / 关闭权限 | `Master` 在 `allows` 里先返回 true；`ALTER_ZERO_PERMISSIONS=0` 直接没有 gate（`src/tui/config.rs:984-990`） | 显式选择，需文档写清“这不是路径监狱” | **P2** |

### 4.3 P0：把 write/edit 的判定做成“判定 + 执行”双重

建议不改提示词、不改 `PermissionMode` 语义，只加执行侧核对：

1. **目标解析统一**：把 `file_change_inside` 的逻辑抽成纯函数（建议放在 `src/permission.rs`，名字可叫 `resolve_write_target`），返回 `ResolvedTarget { real: PathBuf, inside: bool }`。规则与现在一致：拒绝词法 `..`；已存在 `canonicalize`；不存在 `canonicalize(parent) + name`；悬空链接视为“不 inside”。
2. **执行器带策略**：给 `RealToolExecutor` 加一个可选的写边界（建议 `with_path_jail(PathJail)`，`PathJail` 由 bootstrap 用 `cwd`、scratchpad、显式临时根构造）。`run_write` / `run_edit` 在 `std::fs::write` 之前再解析一次：
   - 如果目标本身是符号链接（`symlink_metadata().file_type().is_symlink()`），拒绝；或解析后不在允许根内，拒绝。
   - 如果父目录不是真实路径（`canonicalize(parent)` 不等于父路径），拒绝。
3. **打开方式**：用 Zeroth 已有的 `rustix`（`Cargo.toml:148`）做和 miyu 一样的事：Unix 上 `openat2(parent, name, O_NOFOLLOW | ...)` 或逐层 `openat`；写临时文件 + `renameat`。Windows 上至少 `FILE_FLAG_OPEN_REPARSE_POINT` 打开最后一层，并拒绝 reparse point。
4. **失败关闭**：执行侧解析失败、目标不存在但父目录不在允许根、父目录是链接，一律拒绝，不“退回原来的路径写”。
5. **不要只在 `allows` 里修**：`allows` 解决“要不要问”，执行侧解决“问了/没问都不能写错地方”。两者缺一不可。

### 4.4 P1：read、bash、web、MCP 的后续边界

**read**：在 `approve_call` 里 `read` 现在没有 `PermissionRequest`，建议单独加一个纯函数 `ReadPolicy`：允许根为 `workspace`、`scratchpad`、显式临时根；其余按“敏感/普通”分类。敏感路径包括 `~/.ssh`、`~/.aws`、`~/.config/gh`、`~/.kube`、`~/.docker`、`.env`、`.env.*`、`*.pem`、`*.key`、浏览器配置、`authorized_keys`；manual/edit 问人，auto 也问人（或直接拒），master 放行，headless 拒绝。只读命令 fast path 必须复用同一套分类：`cat ~/.ssh/id_rsa` 不能因为是 `cat` 就跳过；可以在 `reads_only` 之前扫命令里的绝对路径/`~` 路径，拿不准的交给 classifier。图片读取（`read_image`）同样走 `ReadPolicy`，否则换个图片扩展名就可能绕过。

**bash**：参数级校验不可行，结论和 miyu 一样——需要 OS 沙盒。短期在 auto 的 classifier 之前加“明显越界”预检（`> ~/.bashrc`、`cp ... /etc/`、`git config --global core.hooksPath`、`sudo`、`curl | sh`），命中即问人；但这只防幻觉，不是边界。长期用 macOS Seatbelt、Linux Landlock（Zeroth 已有 rustix）、Windows Job Object/AppContainer；规格可照 miyu 的 `Spec`（写白名单、hidden、read 名单）。没有沙盒时，bash 在 auto 下不应被 classifier 静默放行“可能写任意位置”的命令。

**web / MCP**：`web_fetch` 预检拒绝 `file://`、`localhost`、`127.0.0.0/8`、`::1`、私有网段、链路本地、云元数据地址（`169.254.169.254`），重定向每一跳重新检查；这不是路径监狱，而是同一类“读到的内容不要变成外发通道”的边界。Zeroth 的 MCP 调用已经走 `PermissionKind::Mcp` + classifier，不要把 MCP 参数当本地路径；Zeroth 目前没有通用的 MCP `paths` 声明机制，不要虚构，非只读调用保持默认问人。

### 4.5 代码落点

| 落点 | 现在做什么 | 建议 |
|---|---|---|
| `src/permission.rs:1171-1205` | `file_change_inside` | 抽纯函数，补悬空链接、符号链接、大小写测试 |
| `src/permission.rs:1284-1303` | `allows` 里的路径监狱 | 保持“auto/edit 生效、master 跳过”；补 `temp_roots` |
| `src/permission.rs:1306-1316` | `set_workspace` / `set_scratchpad` | 加显式临时根；或让 `PathJail` 持有根 |
| `src/llm/approval.rs:260-286` | `allows` → read-only fast path → scratchpad | 在 fast path 前插入 `ReadPolicy` 检查 |
| `src/llm/approval.rs:350-370` | unattended 拒绝 | 保持；新增策略也要走这个顺序 |
| `src/llm/exec.rs:1456-1520` | `run_write` / `run_edit` | 执行前二次解析 + `O_NOFOLLOW` + temp/rename |
| `src/llm/exec.rs:1249-1276` | `run_read` | 调用 `ReadPolicy` |
| `src/scratchpad.rs:83-112` | 词法 `contains` | 可复用于“显式临时根”的严格判定；但执行侧仍需真实路径核对 |
| `src/checkpoint.rs:374-411` | `cwd_scope` 的“太宽”判定 | 可复用于“workspace 本身是否是根/家目录/状态目录”的校验；不要把它当包含判断 |
| `src/tui/bootstrap.rs:261-274` | 接 gate | 把 `PathJail` 也接到 `RealToolExecutor` |
| `src/llm/backend.rs:1145-1175` | 构造 `approve_call` 闭包 | 新策略保持同一顺序：rules → jail → read fast path → scratchpad → hooks → classifier → unattended → prompt |

### 4.6 测试场景

建议先写 `src/permission.rs` 的纯测试，再加 `src/llm/exec.rs` 的临时目录测试，最后加 smoke phase。不要用真实 `$HOME`，全部在 tempdir 里造。

## 5. 与 Zeroth 现有机制的衔接

| 机制 | 现有语义 | 路径监狱应怎么衔接 |
|---|---|---|
| `PermissionMode` manual/edit/auto/master | manual 每个 write/edit/bash 都问；edit/auto 文件变更放行；master 全放行（`src/permission.rs:30-71`） | 路径监狱只在 edit/auto 的 `allows` 里生效；manual 本来就问；master 明确绕过。不要把 master 改成“部分放行”，否则语义变了 |
| `PermissionRules` | 命令前缀/精确 allowlist；写文件 option 2 切到 edit（`src/permission.rs:927-990`） | 路径监狱不是 allowlist；`PermissionRules::allows` 不覆盖它。保持“先规则、后路径监狱” |
| `PermissionRequest` | `target` 是模型原样路径，`kind` 决定提示形状（`src/permission.rs:135-176`） | 路径监狱用 `target` 解析，但不改写 `target`；提示仍显示模型原文 |
| `PermissionGate::allows` | 规则 + 路径监狱（`src/permission.rs:1284-1303`） | 在这里加 `temp_roots`；保持 false 表示“必须走提示/无人值守拒绝” |
| `scratchpad_covers` | 只覆盖 write/edit，严格词法（`src/scratchpad.rs:83-112`） | 可复用；它先于 classifier，且 `force_ask` 仍会问 |
| classifier | 只处理 bash/MCP/session，不处理 write/edit（`src/llm/approval.rs:313-324`） | 不能把 write/edit 的路径安全交给 classifier；路径监狱必须是确定性的 |
| `refuses_unattended` | headless 无人应答时拒绝（`src/permission.rs:1447-1453`、`src/llm/approval.rs:356-370`） | 新策略在 unattended 检查之前执行；最终仍由它兜底 |
| hooks | `PermissionRequest` hook 可 allow/deny（`src/llm/approval.rs:295-312`） | hook 的 allow 可以绕过路径监狱（它是用户配置的信任决定）；如果担心项目 hook，路径监狱应放在 hook 之后或对 hook allow 也做一次执行侧核对 |

## 6. 隔离实验 / 回归场景

以下都是**在临时目录里造数据**的测试设计，不接触真实家目录。

| # | 场景 | 构造 | 当前代码预期 | 加固后目标 |
|---|---|---|---|---|
| 1 | 工作区外绝对路径写 | workspace=`/tmp/t/ws`，目标 `/tmp/t/out/a.txt` | auto：问人；headless：拒绝 | 不变 |
| 2 | `..` 越界 | 目标 `../out/a.txt` | 问人/拒绝（`..` 被拒） | 不变 |
| 3 | 工作区内相对路径写 | 目标 `src/a.rs` | auto 放行 | 不变 |
| 4 | scratchpad 写 | 目标 `<scratchpad>/plan.md` | auto 放行（在根内）；manual 走 scratchpad note | 不变 |
| 5 | 已存在的符号链接指向工作区外 | `ws/link -> /tmp/t/out/a.txt` | `canonicalize` 到外部 → 问人 | 问人/拒绝 |
| 6 | 悬空符号链接指向工作区外 | `ws/link -> /tmp/t/out/new.txt`（目标不存在） | `exists()` false → 可能放行 → `fs::write` 跟随创建外部文件（**P0**） | 拒绝；执行侧 `O_NOFOLLOW` |
| 7 | 检查后换链接 | 权限检查通过后，用并发 `bash` 把 `ws/a` 换成 `-> /tmp/t/out/a` | `fs::write` 跟随（**P0**） | 执行侧二次解析 + 不跟链接；拒绝 |
| 8 | 读 `~/.ssh/id_rsa` | 在 temp home 造 `.ssh/id_rsa` | `read` 直接返回内容（**P1**） | 敏感路径问人/拒绝 |
| 9 | auto 的 `cat ~/.ssh/id_rsa` | 同上 | `is_read_only` 把 `cat` 当安全 → 放行（**P1**） | fast path 前查路径；问人/拒绝 |
| 10 | 工作区内 `.git/hooks/pre-commit` | `ws/.git/hooks/pre-commit` | auto 直接写（无保护名单） | 问人/拒绝（受保护路径） |
| 11 | 工作区内 `.bashrc`（workspace=home 的模拟） | `home/.bashrc` | auto 直接写 | 问人/拒绝（受保护路径） |
| 12 | `authorized_keys` | `home/.ssh/authorized_keys` | 在 workspace 外时问人；workspace=home 时 auto 直接写 | 受保护路径 + 私密目录双重；问人/拒绝 |
| 13 | 大小写变体（macOS/Windows） | `WS/a` vs `ws/a` | `starts_with` 可能判外部 → 问人（fail closed） | 统一折叠策略；仍问人 |
| 14 | 不存在的多级目录 | `ws/new/deep/a.txt` | 父目录 `canonicalize` 后判内部 → 允许 | 允许；执行时逐层创建并 nofollow |
| 15 | 显式 `/tmp` 临时文件 | `std::env::temp_dir()/foo` | 不在 workspace/scratchpad → 问人 | 显式临时根放行，或仍问人（二选一，写进文档） |
| 16 | Windows 盘符/UNC | `C:\...`、`\\server\share\...` | 未在 Windows 上验证；按 `canonicalize` + `within` | 加 Windows 测试；UNC 默认问人 |

## 7. 结论与优先级

**P0（先修，阻断真实逃逸）**

1. write/edit 的执行侧二次核对：悬空链接、检查后换链接、父目录链接三条路径都必须失败关闭。
2. 用 `O_NOFOLLOW` / `openat2` / 逐层 `openat` 打开父目录与目标，写临时文件后 `rename`，不要直接 `std::fs::write`。
3. 为上述场景补 `src/permission.rs` 纯测试和 `src/llm/exec.rs` 临时目录测试；headless 下同样应拒绝。

**P1（把边界补完整）**

1. `read` 的敏感路径策略：`~/.ssh`、云凭据、`.env`、`authorized_keys`、浏览器配置；auto 也不能静默读。
2. auto 的只读命令 fast path 复用同一套路径策略；`cat ~/.ssh/...` 不能因为“只读”就放行。
3. 受保护写路径：`.git/hooks`、`.git/config`、`.git`（文件）、`.bashrc`/`.zshrc`/`.profile`、`.config/fish`、`.envrc`、`.vscode`、`.mcp.json`、`authorized_keys`；在工作区内也要问/拒。
4. 显式临时根：把 `std::env::temp_dir()` 下会话级目录、`ALTER_ZERO_SCRATCHPAD_DIR` 纳入允许根，减少“正常临时文件也被问”的摩擦。
5. 文档写清 `Master`、`ALTER_ZERO_PERMISSIONS=0`、hooks allow 的绕过语义。

**P2（需要新机制）**

1. `bash` 的 OS 沙盒：macOS Seatbelt、Linux Landlock、Windows AppContainer/Job Object；规格照 miyu 的 `write` / `hidden` / `read`。
2. `web_fetch` 的私网/元数据/`file://`/重定向拦截；把“读到的密钥”与“外发通道”一起看。
3. MCP 工具的参数边界：没有通用 `paths` 声明前，保持非只读调用默认问人；不要假装 MCP 参数能被路径监狱覆盖。
4. Windows 路径硬化：UNC、盘符、短名、路上 reparse point；在 CI 或虚拟机里量过再写进承诺。
5. 统一大小写/Unicode 策略：macOS/Windows 折叠、NFC/NFD 归一化，或者明确“拿不准就问人”。

---

**一句话总结**：miyu 的路径限制是“核心内的文件工具边界 + 内核强制的命令沙盒 + 客户端侧的路径解析”三层，不是一个可以照搬的单一模块。Zeroth 已经具备 write/edit 的路径监狱；当前最值得抄的不是它的规则表，而是它的**双重解析 + 不跟链接打开**，以及“`bash` 不可能靠路径参数校验，必须上 OS 沙盒”这一结论。
