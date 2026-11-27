# `/cd` —— 移动会话的工作目录

`/cd <path>` 在对话中途把**工具**运行的目录移走：`bash` 与文件工具、`!` shell、
页脚的路径，以及对话解析的每一个 `file://` 链接。对话本身留下 —— 历史、模型、
上下文和 prompt cache 都不动。

```
❯ /cd ../other-repo
● Working directory: ~/other-repo
```

## 什么跟着走，什么留下

这个划分就是整个设计。一个会话有两类目录事实，把它们混为一谈正是一个「只是
chdir 一下」的实现不可信的原因。

| 跟随 `/cd` | 留在启动目录 |
| --- | --- |
| `bash`、`bash_session`、`run_in_background` 的 cwd | 权限项目键（`permissions.json`） |
| `read` / `write` / `edit` 的相对路径 | checkpoint 存储及其快照 |
| `!` shell | rollout 文件的 meta 与 `/resume` 的 cwd 过滤 |
| 页脚的路径、`PathDisplay`（相对/波浪线显示、`file://` 链接） | 启动时发现的 skills 与 agents |
| 子 agent 的工具（它们共享同一个句柄） | 这个目录的 `/model` 条目（`config.json`） |

规则是：**干活的地方移动，归档对话的地方不动**。把权限跟着会话一起搬，会把在一个
仓库里授予的允许清单悄悄带进另一个；搬走 checkpoint 会把快照丢在一个没人干活的
树下面；重新给 rollout 定键会让在原目录里 `--continue` 丢掉这段对话。Claude Code
的 `/cd` 连会话的存储一起搬；我们有意识地不这么做，因为我们的 rollout 是按对话而不
是按目录的，搬走一个文件会破坏用户实际所在的目录里的 `--continue`。

## 它是怎么接起来的

一个句柄，`zeroth::workdir::WorkDir`，是一个 `Arc<RwLock<PathBuf>>`：

- `tui::bootstrap` 从启动 cwd 建它，把克隆分别交给 `BackgroundRegistry`
  （`with_work_dir`）和 `ModelSession`（后者把它挂到它构建的每一个后端上），
  并在 `Conversation` 上留一个。
- `RealToolExecutor` 和 `LlmBackend` 把它从后端带进每一次工具调用
  （`with_work_dir`）。`bash` 经由 `subprocess::spawn_detached_in_dir` 启动，
  tty 那条走 `pty::spawn::spawn_dir`，文件工具用 `resolve_tool_path` 解析相对
  路径，全部都是**在调用时**读这个句柄，而不是在构建时 —— 所以一次移动能到达一个
  已经建好的后端上的下一次工具调用，不需要重建。
- `!` 运行器读注册表的 `work_dir()`，于是两个 shell 永远不会不一致。
- 纯核心从不读进程 cwd：循环用 `ui::display_cwd` 格式化新目录，并调用
  `App::set_working_dir`（`set_session_info` 那套模式），再用新目录调用
  `App::set_path_display`。

解析在最后一步检查之前都是词法的：`~`/`~/…` 按 `$HOME` 展开，其他相对的都接在
当前目录上，然后循环对结果 `canonicalize`（于是 `..`、符号链接和末尾斜杠都会落到
同一个真实目录上），并要求 `is_dir()`。

## 拒绝

- **光一个 `/cd`** —— 面板那一行是门，不是一个目录浏览器 —— 回答
  `Usage: /cd <path> — moves this session's working directory`。
- **一个不是已存在目录的路径** 是一条红色提示，什么也不改。
- **回合进行中** 移动会被拒绝（`/cd is disabled while a task is in progress`）：
  一个运行中回合的工具是在后端线程上按它自己的节奏解析的，一半回合在这个目录、一半
  在另一个，绝不是用户的本意。检查的是 `App::turn_active`，与 `/resume` 和
  `/compact` 完全一样。

## 缓存

什么都不失效。prompt cache 键是按**进程**的
（`tui::config::session_cache_key`，pid + 启动时间的词缀），不是按目录的，所以移动
不损失任何缓存命中；而系统提示词里的环境块 —— 它点名 cwd —— 是在启动时组成的、
不会重写，这正是这段对话的第一次请求仍可缓存的原因。新目录的 `AGENTS.md` 也**不会**
被悄悄采纳：项目指令仍是启动项目的，理由和它的权限一样。

## 测试

- `zeroth::workdir` —— `WorkDir` 的解析（`~`、相对、绝对、一个克隆看到一次移动）
  以及没有 home 的情形。
- `llm::exec::work_dir_tests` —— `write`/`read` 落在句柄之下，两次调用之间的一次
  移动能到达下一次调用，`edit` 跟着走，而 `bash` 的 `pwd` 与之相符。
- `app::tests::commands` —— `/cd` 已注册，光一个 `/cd` 回答用法提示，而
  `/cd <path>` 把参数原样交给循环（含空格）。
