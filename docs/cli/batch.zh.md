# 批量模式 — 一个进程里跑多个 headless session

`zeroth -p --jobs FILE` 把一个 JSONL 提示词文件里的条目当作相互独立的 headless
session，**在一个进程里**运行，跑在有界的工作线程池上。它是多线程内核
（`docs/architecture/multithread-core.md`）的第一个调用方：每个 job 就是一次 `-p`
调用所跑的那个 [`headless::run`](../cli/headless.md)，运行在属于自己的 OS 线程
和属于自己的 current-thread runtime 上。

```bash
cat > jobs.jsonl <<'EOF'
{"id": "api", "prompt": "List the public endpoints in src/api.rs"}
{"id": "tests", "prompt": "Run the test suite and summarise the failures"}
EOF

zeroth -p --jobs jobs.jsonl --concurrency 2 --out-dir job-logs
```

## jobs 文件

每行一个 JSON 对象；空行会被跳过。

| field | | |
| --- | --- | --- |
| `prompt` | required | 该次运行的第一个用户消息，与位置参数 `[PROMPT]` 完全等价。 |
| `id` | optional | 在结果、日志文件名和重复检测中给这个 job 命名。默认是 `job-N`，即其从 1 开始的行号。 |

缺少 `prompt`、`prompt` 为空白、JSON 格式有误、`id` 重复，或文件中一个 job 都没有，
都是用法错误：批量运行会在 stderr 说明原因，并在任何 session 启动之前以 `2` 退出。
`--jobs` 不能与 `[PROMPT]`、`--continue` 或 `--resume` 组合使用：每个 job 都开启
一段全新的对话。

## 输出

每个完成的 job 在 stdout 输出一个 JSON 对象，按完成顺序排列：

```json
{"id":"api","exit_code":0,"session_id":"18e75fc60a157a18-d324-7","result":"…","num_turns":2,"usage":{"input_tokens":1840,"output_tokens":212,"cached_input_tokens":0,"cache_write_input_tokens":0},"duration_ms":331,"log":"job-logs/001-api.log"}
```

- `exit_code` 是 job 自身的状态，含义与单次 headless 运行相同
  （`docs/cli/headless.md` 的 *Exit status* 一节）。
- `result` 是该次运行的最终答复；`session_id` 就是 `--resume` 会接受的那个值
  （仅当该次运行记录下了内容时才存在）。
- `log` 仅在给了 `--out-dir` 时存在；该文件保存这个 job 完整的 reporter 输出
  （由 `--output-format` 选定的格式），被捕获下来而不是写到进程的各个流上。
- job 之间绝不会在 stdout 上交错：每个 session 的 reporter 写入线程本地的 sink，
  批量层只输出完整的结果行。

## 并发与准入

`--concurrency N` 设定工作线程数（默认 4）。工作线程从一个队列里取活；一个返回
`130`/`143` 的 job 会清空队列，因此 Ctrl+C 会在已经在跑的 session 关闭之后停掉
整批。

预算是显式给出的，而不是自动推断的。每个 session 的 current-thread runtime 占着
一个 kqueue/epoll 描述符，它的 bootstrap 会打开 config、skill、agent 和 rollout
文件；设计笔记中的实测表明，15 个 session 稳稳落在默认 256-fd 软限制之内，而 64
个并发 bootstrap 会撞上 `EMFILE`。对普通机器来说 4 是安全的默认值；对于受网络
限制的负载——session 大部分时间都在等待——可以调高它。

## 哪些共享，哪些不共享

共享：config home、工作目录、进程的 stdout/stderr，以及内核的调度器。在任何
session 启动之前，批量层会解析出一个只读的 `Host` —— home 迁移、首次运行在
`skills/` 与 `agents/` 下的种子文件、skill 与 agent 遍历、本目录的设置、secrets
与 hooks 文件，以及模型解析器的 provider 表、`.env` 和已保存的选择。每个 job 把
这些值克隆进自己的注册表；`Host` 永远不会被修改。种子文件和迁移以原子重命名
的方式发布，因此并发的 bootstrap 不会让某个 session 读到另一个 session 写了一半
的文件，而 session id 携带一个进程内的序号，使得在同一纳秒内启动的两个 session
也不会共用一条 rollout 路径。

不共享：每个 session 拥有自己的 `App`、对话、rollout、权限 gate、MCP manager、
subagent 注册表和后台 shell。一个 job 看不到、也操纵不了另一个 job 的 turn。没有
TUI，也没有共享的 model 客户端；每个 session 都从同一份 config 构建自己的
provider 客户端。

## 退出状态

当每个 job 都以 `0` 退出时，批量层退出 `0`；否则退出第一个失败 job 的状态；当
信号终止了整批时退出 `128 + signal`。单个 job 的失败不会阻止其余 job 继续运行
—— 结果流会说明是哪一个失败了。
