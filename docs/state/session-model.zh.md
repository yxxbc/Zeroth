# 按 session 的模型 —— 一个对话记得它所跑的模型

`/model` 的选择是**按工作目录**保存的（`docs/state/per-directory-state.md`）：
`config.json` 为每个 cwd 保留一个条目，在那里启动的每个 session 都用它。那回答了
「这个*项目*用哪个模型？」，却留下第二个问题没答 —— 这个*对话*用哪个模型？一个
目录可以同时有两个 zeroth 实例，一个跑前沿模型，旁边一个跑便宜的本地模型；而
一个被恢复的对话是由某个特定模型产出的，不管那个目录的条目今天怎么说。两者过去都
输给目录：

- 同一目录里的两个实例共用一个 `config.json` 条目。各自跑它内存里的模型，但其中
  任何一个的 `/model` 选择都会移动这个共享条目，于是另一个的*下一次*启动 —— 或者
  它的 `--resume` —— 会起在一个它从未用过的模型上。
- `/resume`、`--resume {id}` 与 `--continue` 恢复了 transcript 和代码，然后用目录
  当前的条目跑对话剩下的部分。rollout 的 `session_meta` 行点名了一个模型，但那只是
  文件*被创建*时的那个，而且从来没有谁把它读回来。

现在的规则：**模型是关于这个 session 的一个事实，记录在它的 rollout 里，而一次
恢复把它带回来。** 目录的条目变成*新* session 的起点 —— 种子 —— 仅此而已。

## 记录

每个 rollout 文件（`docs/state/resume.md`）在 `session_meta` 行旁边多一条 `model`
记录：

```json
{"timestamp":"2026-09-15T10:00:00.000Z","type":"model","payload":{"provider":"openrouter","model":"openai/gpt-4o-mini","thinking":{"supported":false},"vision":true,"context":128000}}
```

载荷是 `llm::settings::ModelSelection` —— 与 `config.json` 条目完全相同的形状：
`(provider, model)` 这一对，加上这个 session 所知道的关于该模型的东西（`thinking`
块、`vision`、`context`，以及一个 ChatGPT 模型的 `/fast` 档位所搭乘的 `speed` 块
—— `docs/providers/fast-mode.md`；仍然未知的事实不出现在这一行上，所以一次恢复
为它所做的探测，与在一个没有这些事实的条目上启动时所做的完全一样）。「一份保存的
模型选择」处处只有一种格式，所以未来的一个字段会同时到达两个文件：`speed` 就是
这样到达的，而一个被恢复的对话免费地回到了它的档位上。

它是**只追加的，最新的胜出**（`session::parse_model` —— `parse_checkpoints`
sidecar 的孪生体，对 `parse_session` 不可见，所以加载出来的 transcript 有没有它
都逐字节相同）。一行会在这些时候被写出：

- 与文件本身一起 —— 第一个历史条目物化这个文件（codex 的延迟创建被保留：一个从不
  说话的 session 不留文件，而一个选择本身从不创建它），而 model 行落在 meta 行之后
  紧跟着；
- 每一次 `/model` 选择时；
- 每一次 Ctrl+T 循环、每一次 `/fast` 切换，以及启动时能力探测的答案 —— 与
  `config.json` 相同的那些写入者，所以这些事实与目录条目所能有的同样新鲜；
- `/clear` 启动的那个新文件的第一个条目时：新对话跑在同一个 session 的模型上。

一次落在已记录模型上的恢复什么都不写（记录已经这么说了）。旧的构建跳过这个未知的
记录类型（向前兼容契约），并如从前一样读 transcript。

`SessionRecorder` 把选择与 meta 放在一起（`set_model`，一个
`selection_written` 水位线 —— checkpoint 的模式），在循环底部的 `sync` 时冲刷一个
变化过的选择，与其他每一行一样。它是 model 行唯一被写出的地方，所以一次重写（一次
回溯的截断）会在 meta 之后重新发出当前的选择。

## 恢复它

`/resume`（那个 picker）、`--resume {id}` 与 `--continue` 都会解析文件最新的记录
并交给 `ModelSession::restore` —— 一条路径，启动指令也在内
（`Session::restore_session_model`；启动时的能力探测在指令被应用*之后*才 spawn，
所以一个被恢复的 session 探测的是它自己模型的事实，而不是目录条目的事实）。这次
恢复是一次 `/model` 切换减去写入：backend 为记录的那一对重建，带上记录下来的思考
模式、vision、窗口与速度档位，Ctrl+T 循环与 `/fast` 从它们停下的地方起步，页脚与
context 量表跟随 —— 而 `config.json` **不被触碰**。一次恢复什么都不选；目录的条目
保持那里的最后一次 `/model` 选择所造就的样子。

一份点名的恰好是这个 session 已经在跑的记录 —— 在那个对话所跑的目录里
`--continue` 的常见情形 —— 什么也不恢复：不重建，也不为排队着的那次探测会学到的
事实做第二次探测。比较的是整个选择，不是那一对，所以一份携带另一个 Ctrl+T 模式的
记录仍然会恢复。三种情况拦下它：

- **一个环境钉住。** `ZEROTH_MODEL` 为这次运行钉住模型，不管记录怎么说，而
  `ZEROTH_PROVIDER` 钉住 provider —— 一个保存下来的对只在与它一起保存的那个
  provider 之下生效（启动优先级，`docs/providers/llm.md`；纯函数
  `llm::settings::env_outranks` 回答它）。session 静默地保持被钉住的模型 —— 钉住
  是用户自己做的事 —— 而记录不被碰（环境赢，且从不粘住，即 `config.json` 的规则
  扩展到 rollout）。
- **没有可用的配置。** 记录下来的 provider 不在表里，或者它的 key 没了（这台机器
  上从未跑过 `/login`）。session 停在它解决出来的那个 —— 目录的条目，或 dummy ——
  之下是一条红色的 `Can't resume on {model}: run /login …` toast，而记录不被碰：
  一个由缺失 key 迫使的 fallback 不是一个选择，所以文件仍然点名这个对话所在的
  模型，而在 `/login` 之后一次恢复会把它带回来。
- **没有记录。** 一个记录在 dummy 上的 rollout，或者由记录存在之前的一个构建所写：
  没有什么可恢复，session 保持它的模型。

## `config.json` 现在是

| | 之前 | 现在 |
|---|---|---|
| 一个**新** session 用什么起步 | 目录的条目 | 目录的条目 —— 不变 |
| 一个**被恢复的** session 跑什么 | 目录的条目 | **rollout 最新的 `model` 记录** |
| 一次 `/model` 选择 | 目录的条目*和*最后一次选择 | 同上 —— **加上**这个 session 的记录 |
| Ctrl+T / 探测 | 目录的条目，当它点名这同一个模型时 | 同上 —— **加上**这个 session 的记录，总是 |
| 一次恢复 | — | 什么都不写 |

有一处不对称值得说出来：在一个*被恢复的*对话里做的 `/model` 选择仍然会移动目录的
条目。那个条目是「在这里做出的最后一次选择」，而那次选择就是一次。同一目录里的另一
个实例两种情况下都不受影响 —— 它跑它内存里的模型，它的 rollout 点名那个模型，
而它自己的恢复把那一个带回来。`ModelSession::persisted_selection` 仍然意为
「`config.json` 为这个目录记录的这一对」，所以在一个模型与目录条目不同的 session
里按 Ctrl+T，写的是 rollout、拒绝的是条目，与一个被环境覆盖的选择一向所做的一样。

## 两个实例，一个目录

实例 A 在 `x` 上，实例 B 在 `y` 上，同一个 cwd：

1. B 在 `/model` 里选了 `y`：目录的条目与最后一次选择是 `y`；B 的 rollout 记录
   `y`。A 的 rollout 仍然记录 `x`；A 继续跑 `x`。
2. A 退出。`zeroth --resume {A}` 回到 `x` 上。`zeroth --continue` 重开这里
   最新的对话 —— B 的 —— 在 `y` 上。一个光秃秃的 `zeroth` 在 `y` 上（目录的
   条目）开始一个新 session。
3. 在 B 仍在跑时恢复 A 不会交错任何东西：它们是两个文件。（从两个实例恢复*同一个*
   文件仍然是 `docs/state/resume.md` 记录在案的局限。）

## 测试

- `session`：`model_line` 是一条带标签的 `model` 行，携带选择、未知事实被省略；
  `parse_model` 把**最新**的记录整个返回（含 thinking 块），对一个没有它的文件、
  一个空文件或一条畸形行返回 `None`；model 行对 `parse_session` 与
  `parse_checkpoints` 不可见。
- `llm::settings`：`env_outranks` —— 没有钉住时记录生效，钉住的模型高于它，钉住的
  provider 高于在另一个 provider 之下保存的记录，并让在同一个 provider 之下保存的
  记录生效。
- `scripts/smoke.sh` Phase 118（边界）：一个目录里两个实例，第一个在目录的条目上、
  第二个在 `ZEROTH_MODEL` 钉住上，各自记录自己的 `model` 行；然后目录的条目
  像别处一次 `/model` 选择会移动它那样被移动；对一个跑在条目自己模型上的对话做
  `--resume {path}` 不追加多余的行；然后条目被移动，`--resume {path}` 回到第一个
  session 的模型上并让 `config.json` 不受影响，`--continue` 回到第二个的，一个新
  启动落在目录的条目上，在那个新 session 里的 `/resume` picker 落在第一个的上，
  一次环境钉住压过 `--resume`、落在钉住的模型上，一份点名这台机器够不到的 provider
  的记录落在新启动的模型上、之下是 `Can't resume on …` toast，以及一个没有记录的
  rollout（一个特性之前的文件）落在目录的条目上，而它随后把它记录下来。
