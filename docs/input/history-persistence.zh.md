# 跨 session 的输入历史 —— 把 ↑/↓ 与 Ctrl+R 的历史持久化到磁盘

日期：2026-07-13

## 目标

让 composer 的输入历史**活过一次重启**，像 openai/codex 那样。今天
`App::input_history`（支撑 shell 风格 ↑/↓ 回溯的 `InputHistory` ——
`docs/input/input-history.md` —— *以及* Ctrl+R 反向搜索的 ——
`docs/input/history-search.md`）每次运行都从头积累，退出就丢。这次改动之后，提
交过的输入会写进一个只追加的历史文件，启动时读回来，而回溯和搜索都横跨每一个过
去的 session。

input-history.md 和 history-search.md 都明确没做这一块（「没有跨 session 的持
久化」）；这次补上那个缺口。

## codex 怎么做（`/tmp/codex/codex-rs/message-history`）

Codex 维护一个全局的、只追加的 **`~/.codex/history.jsonl`**，每行一个 JSON 对
象：

```text
{"session_id":"<id>","ts":<unix_seconds>,"text":"<message>"}
```

- **追加**（`append_entry`）把完整的行 + `\n` 构造好，用 `O_APPEND` 打开的 fd 在
  **一次 `write(2)`** 里写出去 —— POSIX 让不超过 `PIPE_BUF` 的写是原子的，所以
  并发的 TUI 进程不会交错。它还持有一个建议性的 `try_lock`（双保险，而且需要，
  因为 codex *也*会重写文件来裁剪它），并强制执行一个 `max_bytes` 上限，文件长
  得太大时把最旧的行丢到软上限。文件是 `0o600`（它能装下用户敲的任何东西）。
- **读取**是惰性、异步的：composer 只持有一个 `(log_id, entry_count)` 元数据快
  照，浏览/搜索时按 `(log_id, offset)` 按需取每一条跨 session 记录
  （`LookupMessageHistoryEntry` → `on_entry_response`）。那套机械 ——
  `Pending`/`Searching` 状态 —— 存在只是因为历史可能很大且跨进程共享。

## 映射到这套代码库 —— 启动时加载，不是惰性取

`docs/input/history-search.md` 已经记下了那个关键的简化：*「我们没有持久历史，所
以那一整套 pending 机器 —— 以及 Searching 状态 —— 都掉出去了；一切都在
`InputHistory::entries` 之上同步」*。我们保留这一点。取代 codex 的异步按偏移取，
我们**在启动时把文件一次读进来，填进 `InputHistory::entries`**。填的正是 ↑/↓ 和
Ctrl+R 已经在读的那个向量，意味着 *`up`/`down` 和 `search` 一点都不用改* —— 它
们透明地横跨各 session。新提交的输入在被记录时追加到磁盘。

这精确对应项目既有的划分：纯的磁盘格式住在一个新模块 `history` 里（像
`session` 那样），而每一处不纯 —— 来自环境的路径、给 `ts` 用的时钟、读与追加
—— 都住在边界上，在 `main.rs` 里（`InputHistoryStore`，像
`SessionRecorder`）。纯核心把新记录的输入**排队**，边界把它们**抽干**到磁盘
—— 和 `insert_before` 一样的「核心排队、边界冲刷」模式。

### 纯格式核心（`src/history.rs`）

Serde 放在它**自己的**记录类型上（应用的类型保持无 serde，像 `session` 那样），
与 codex 的 schema 一致，为前向兼容与互操作：

```rust
#[derive(Serialize, Deserialize)]
struct HistoryRecord { session_id: String, ts: u64, text: String }

/// Serialize one entry as a single JSONL line (no trailing newline). serde_json
/// escapes embedded newlines, so a multi-line draft is still ONE physical line.
pub fn history_line(session_id: &str, ts: u64, text: &str) -> String;

/// Parse a whole file's contents into the recorded texts, oldest-first (the
/// order `InputHistory::entries` wants). Blank/malformed/unknown-shape lines
/// **and records whose `text` is empty** are skipped (codex's forward-compatible
/// reader), so a torn or future-version line never breaks the load and a
/// hand-edited empty entry never seeds a blank recall.
pub fn parse_history(contents: &str) -> Vec<String>;
```

未知的额外字段解析得下去（serde 默认忽略它们），所以未来某个构建写的文件仍然读得
进来。

### 纯状态（`InputHistory`，在 `app/input_history.rs`）

一个新字段、两个新方法；**`record`、`up`、`down`、`search` 保持它们现有的行
为**：

```rust
pub struct InputHistory {
    entries: Vec<String>,
    cursor: Option<usize>,
    last_recall: Option<String>,
    unpersisted: Vec<String>,   // recorded-this-session, not yet flushed to disk
}
```

- `record(text)` —— 对 `entries` 的空白/相邻重复过滤不变；**它同时把文本排到
  `unpersisted` 上**，等边界去写，并与一个单独的 `last_persisted` 水位线（**不
  是** `entries`）去重，所以一个从未持久化的 `record_ephemeral` 草稿遮不住一次
  恰好重复它的真实提交 —— 而持久化的那条流仍然会折叠相邻重复。
- `record_ephemeral(text)` —— 记进 `entries`，**不**为磁盘排队。**Ctrl+C 清空的
  草稿**用它：codex 把清空的草稿只留在它的进程内 `local_history` 里，从不写持久
  文件 —— 所以一个被清空、被放弃的草稿能在本 session 回溯，却不污染跨 session 历
  史。（每一个*发出*的输入 —— 空闲提交、`!command`，以及回合中途排队的批次/
  shell —— 都用普通的 `record` 并持久化。）
- `seed(entries)` —— 启动时从磁盘批量加载进 `entries`，作为 `record` 的一次忠实
  **重放**（每段文本都跑同样的跳过空白 + 折叠相邻重复），所以一个乱的或被并发写
  的文件，填出来的和全新 session 会建起来的干净缓冲区一样。**不**把它们排队（它
  们已经在磁盘上了）；最新的那条成为 `last_persisted` 的去重目标。在全新的历史上
  调用一次。
- `take_unpersisted()` —— 抽干队列，交给边界去追加。

`App` 在这些之上暴露 `seed_input_history(entries)` 与
`take_unpersisted_inputs()`（也就是 `set_clock` 那套边界注入的模式）。`/clear`
仍然不动 `input_history`（↑ 回溯存活 —— `docs/input/input-history.md`），所以它
既不清内存也不清文件。

### 边界（`src/main.rs`，`InputHistoryStore`）

解析路径、启动时加载、每轮循环追加 —— 尽力而为，吞掉每一个错误（记录绝不能杀掉
TUI，像 `SessionRecorder` 和 `save_settings` 那样）：

- **路径**：`ZEROTH_HISTORY_FILE` 覆盖，否则 `{config_home}/history.jsonl`
  （`~/.zeroth/history.jsonl` —— 就在 `config.json`/`.env`/`sessions/` 旁边；
  `ZEROTH_CONFIG_DIR` 已经能重定向它，smoke 测试就是靠这个隔离它的）。`None`
  （没有 HOME、没有覆盖）则关闭持久化。
- **`load()`** → `Vec<String>`：读文件的**字节**并做有损解码
  （`String::from_utf8_lossy`，和 shell 读取器一样）—— 一次撕裂/交错的追加可能
  留下非法 UTF-8，而 `read_to_string` 会因此报错并扔掉*整份*历史；有损解码只把坏
  字节变成 U+FFFD，于是 `parse_history` 只跳过那一行。缺失/读不了 → 空。内存里截
  到最后 `HISTORY_MAX_ENTRIES`（10 000）条，这样不管文件长到多大，回溯/搜索都保
  持 O(small)。
- **压实**：当文件里超过 `HISTORY_MAX_ENTRIES * 5/4` 条时，`load()` 把它重写为最
  新的 `HISTORY_MAX_ENTRIES` 条（codex 那个硬上限 → 软上限的想法，按条数而不是
  按字节）。重写的条目用本 session 的 id + 当前时间重新打戳（`ts`/`session_id`
  两个字段应用并不使用 —— 它们只为了文件的自描述和 codex 互操作而存在）。启动时
  做一次；与一个并发实例的追加之间那点微小的竞态被接受（codex 在这里也接受最终
  一致）。
- **`append(texts)`**：把每个 `history_line(session_id, now, text)` + `\n` 都构
  造进一个缓冲区，用一次 `O_APPEND` 的 `write_all` 写出（在 `PIPE_BUF` 以内是原
  子的），首次写时创建父目录 + 一个 `0o600` 文件。超
  `HISTORY_MAX_ENTRY_BYTES`（100 KiB）的输入被**跳过** —— 大粘贴在 composer 里
  是展开的，否则它的全文就会写进去，而条目*数量*的上限管不住文件的*字节*；一个巨
  大的 blob 仍然能在本 session 回溯（从 `entries`），但不值得占一条反搜条目
  （codex 因为同样的理由用 `max_bytes` 设界）。在每轮循环对
  `app.take_unpersisted_inputs()` 调一次，就在 `recorder.sync(...)` 旁边，循环结
  束后再调一次做最后的冲刷。

`session_id` 复用 `main.rs` 已有的 `session_id()` 辅助函数（纳秒+pid 的十六进
制）；`ts` 是 `SystemTime::now()` —— 两处不纯都留在边界上。

## 已知限制

- **没有建议性锁。** 追加依赖 `O_APPEND` 单次写的原子性，而启动时的压实是无锁跑
  的。两个实例在同一瞬间追加非常大的（>`PIPE_BUF`）输入，或者一个正好在另一个稀
  有的启动压实期间追加，可能丢掉/撕裂一行 —— 加载器对文件做有损解码、解析器跳过
  撕裂的那一行，所以最坏情况是丢一条历史（绝不会是整份文件）。codex 加锁是因为
  它每次写都裁剪；我们只在启动时、且远超上限时才裁剪，所以那个窗口可以忽略。
- **顺序是追加时间，不是全局时钟。** 并发的 session 会把各自的追加交错开；单个
  session 永远是有序的。

## 测试

- `history`（单元）：`history_line` 经 `parse_history` 往返；一段多行/含 `"`/带
  unicode 的文本仍然是一条物理行并往返成功；空行、坏 JSON、形状不对的行、**以及
  `text` 为空的记录**都被跳过；额外未知字段仍然解析；顺序保持最旧在前；空输入 →
  空 vec。
- `app`（单元）：`record` 把文本排到 `unpersisted` 上（由
  `take_unpersisted` 抽干），而空白/相邻重复的 record 什么都不排；
  `record_ephemeral` 记进 `entries` 却什么都不排；`seed` 填进 `entries`（于是
  ↑ 能回溯到一条被种下的条目，`search` 也能找到它）而不排队；Ctrl+C 清空那条路
  做瞬时记录（清空的草稿能回溯，但不为磁盘排队）；**一条重复了已清空瞬时草稿的
  已发消息仍然被持久化**（持久化去重是冲着 `last_persisted`，不是 `entries`）；
  一条立刻重发的消息只持久化一次；一条等于最新种下条目的提交不会被重新持久化；
  **`seed` 像 `record` 一样折叠相邻重复**（乱的/并发的文件种出干净的缓冲区）。
- `src/tui/`（smoke，Phase 37）：在一个进程里提交一条消息，退出，**往文件里追加
  一行非法 UTF-8**，对着同一个 `ZEROTH_HISTORY_FILE` 启动**第二个**进程，确认
  ↑ 回溯到上一个 session 的消息、Ctrl+R 也能找到它 —— 端到端的跨 session 持久
  化，而一条坏掉的尾行不会抹掉历史（有损加载只跳过那一条坏行）。
