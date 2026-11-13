# `/init` —— 生成一份 AGENTS.md 贡献指南（codex 的 `/init`）

## codex 怎么做

Codex 的 `/init` 刻意做得很小。那条斜杠命令（`SlashCommand::Init`，描述
「create an AGENTS.md file with instructions for Codex」）只做一件事：

```rust
SlashCommand::Init => {
    const INIT_PROMPT: &str = include_str!("../../prompt_for_init_command.md");
    self.submit_user_message(INIT_PROMPT.to_string().into());
}
```

它把一段固定 prompt 作为**普通用户回合**提交。那段 prompt 让模型生成一个名为
`AGENTS.md` 的文件 —— 一份简洁（200–400 词）的贡献指南，带推荐的各小节（项目
结构、构建/测试命令、风格、测试、提交/PR 约定）—— 而且关键的是，先检查
`AGENTS.md` 是否已存在，且*不要*覆盖它。其他一切 —— 探索仓库、写文件 —— 都是
模型正常的 agentic tool 循环。没有应用侧的文件 I/O，没有特殊渲染，没有新机制：
那段 prompt *就是*这个功能。

Codex 对它的门控和 `/compact` 一样：`available_during_task` 为 `false` —— 一个
任务在跑时你不能启动它。

## 我们的

移植完全照着那个形状；每一块都骑在已有的机制上。

- **`prompts/init.md`** —— codex 的 `prompt_for_init_command.md`，逐字照抄。通过
  `include_str!` 编译进来作为 `app::INIT_PROMPT`（`prompts/compact_prompt.md`
  那条接缝 —— 措辞留在可维护的 markdown 里，而不是 Rust 字符串）。
- **palette 条目** —— `COMMANDS` 里加一行（`CommandEffect::Init`），就列在
  `/compact` 之前（codex 的 Init 紧挨 Compact）。描述改写成简洁、不含产品名的
  说法，像 `/quit` 那样：「Create an AGENTS.md contributor guide」（codex 说的是
  「…with instructions for Codex」）。palette 遍历 `COMMANDS`，所以它白得一条
  列表项。
- **分发**（`App::run_selected_command`）：
  - 空闲 → `Action::Submit(INIT_PROMPT.trim_end().to_string())`。循环已有的
    `Submit` 分支跑 `start_turn`，所以整段 prompt 作为用户的 `❯` 消息回显
    （codex 渲染它提交的 prompt 也是同样的方式），记录进 session rollout，打
    checkpoint，再由真实 backend 的 tool 循环回答 —— 模型自己 `read` 仓库并
    `write` `AGENTS.md`。没有新的 `Action` 变体。`trim_end` 剥掉文件末尾的
    换行：不剥的话它会折出一个空的最后一行，`message_lines` 会把它补成 `❯`
    单元格下一行多余的整宽暗色行（codex 提交时不剥，但在渲染时等价地剥掉了 ——
    它的用户单元格用 `trim_end_matches(['\r','\n'])`；在提交处剥，让我们的渲染
    路径一动不动，也让记录下来的历史在每一处重放时都干净）。
  - 回合中途 → `Action::Toast(INIT_BUSY_NOTICE)`（「/init is disabled while a
    task is in progress」）—— codex 的 `available_during_task = false`，用我们
    `/resume`/`/compact` 那套忙碌 toast 的模式表达（`docs/ui/toast.md`）。
    Enter 走这个分支；命令页打开时 Tab 在它的各页签之间循环，也绝不能落到
    「排成新批次」的那个 Tab —— 两个菜单分支都压过队列。
- **流式前沿窗口里的 Esc** —— 通用的中断撤销会把一个没有输出的回合的提交退回
  到 composer（`docs/input/interrupt.md`）。对一个 `/init` 回合来说，那段文字
  是 composer 从未持有过的固定 prompt，所以撤销改为召回字面的 `/init`（palette
  通过 `refresh_command_menu` 在它上面重开 —— 正是提交之前的状态），而随后的一次
  Ctrl+C 只把 `/init` 记进 ↑ 召回，保住下面那条不召回的保证。

## 闭环：生成的指南进到上下文里

`/init` 写 `AGENTS.md`；**`docs/state/project-doc.md`** 是另一半 —— codex 的
项目文档发现，移植过来了：每次回合开始都重读项目的 `AGENTS.md`（root→cwd，
32 KiB 上限）并把它作为上下文窗口开头那段 user-instructions 片段注入。所以模型
刚生成的那份指南，在紧接着的下一个回合就已经在它的上下文里，与 codex 完全一样。

## 刻意不在这里的东西

- **没有应用侧的 `AGENTS.md` 存在性检查。** prompt 本身就指示模型不要覆盖已存在
  的文件 —— 与 codex 给予的信任相同。
- **不记录历史。** Codex 在 `/init` 上记两样东西：字面的 `/init` 进它的本地
  composer 召回（`stage_slash_command_history`），以及完整提交的 prompt 进它的
  跨 session 消息历史。我们单一的 `App::input_history` 没有本地/跨 session
  之分，
  而 palette 命令从不记入其中（Enter 分支的记录是给键入的提交用的）—— 所以这次
  移植两样都不记。↑/Ctrl+R 这一面保持不含用户从未键入的文字，代价是 `/init`
  本身不进入 ↑ 召回（它只有五次击键）。
- **没有新的 smoke 阶段。** 这个决定是纯的（palette → `Submit`）且有单元测试；
  `Submit` → `start_turn` 这条路本来就有 smoke 覆盖。
- **没有特判的显示。** 整段 prompt 作为用户消息显示，codex 风格。如果哪天觉得
  太吵，显示文字/发送文字分离会是一次后续设计，不属于这次移植。

## 测试

`app::tests`（`--- /init (docs/state/init.md) ---` 那个块）：palette 条目 + 描述
+ 在 `/compact` 之前的顺序、prompt 的内容（提到 `AGENTS.md`、防覆盖）、空闲时
Enter → 剥过的 `Submit(INIT_PROMPT.trim_end())` 且 composer 被清空、不做 ↑ 召回
记录、回合中途在**两个**接受键上都是忙碌 toast（Enter，以及绝不能落到队列的
Tab），还有 Esc 撤销恢复字面的 `/init` 并重开 palette。
