# The multi-threaded core — session concurrency, measured

Date: 2026-11-12

Status: **measurement, recommendation, and a first client**. Following
[`concurrency.md`](concurrency.md). The benchmark harness is
`src/bin/sessionbench.rs`, behind `--features bench`. The first product client
is `-p --jobs` (`docs/cli/batch.md`); the remaining Host work is listed below.

## The question

[#24](https://github.com/linuztx/alter-zero/issues/24) asks whether one
process per session is the intended ceiling. The benchmark it compares
against runs 15 sessions at once beside a 16-thread compile and stays
responsive. The smallest unit of that problem is: **N sessions in one
process** ([#27](https://github.com/linuztx/alter-zero/issues/27)), before a
session host and a TUI client exist ([#28](https://github.com/linuztx/alter-zero/issues/28)).

"Supports multi-threading" therefore means: several independent sessions run
at the same time on several cores, each with its own conversation, rollout,
permission gate and MCP manager, without one session's turn blocking another.
It does **not** mean parallel tool calls inside one turn
([`tools/parallel-tools.md`](../tools/parallel-tools.md)) — that is a
separate serialisation point.

## What was measured

A temporary binary runs the **real** `tui::headless::run` N times in one
process — the same bootstrap, turn loop, recorder and shutdown the `-p`
binary uses. It offers four shapes:

| shape | how |
| --- | --- |
| `serial` | N sessions back to back, no concurrency |
| `thread` | one `std::thread` per session, each with its own current-thread tokio runtime |
| `pool` | W worker threads consuming N jobs from a channel |
| `async` | one multi-thread tokio runtime; each session is a `spawn_blocking` job (see *Why not a task?*) |

Workload: the built-in dummy backend with `ALTER_ZERO_STALL_MS` set, so every
session spends a known 300–800 ms in the pre-first-token stall — the shape of
a network-bound turn — then answers without network or tokens. Metrics:
process RSS (`ps -o rss`), thread count (`ps -M`), wall clock, and stderr
(the bootstrap reports races). The config home is shared, as a host would
share it.

Reproduce:

```bash
cargo build --features bench --bin sessionbench
ALTER_ZERO_DUMMY=1 ALTER_ZERO_STALL_MS=300 ZEROTH_CONFIG_DIR=$(mktemp -d) \
  target/debug/sessionbench thread 15
```

## Results

15 sessions, 500 ms stall — the comparison's own shape:

| shape | wall | RSS | threads | bootstrap stderr |
| --- | --- | --- | --- | --- |
| one process per session (16 sessions, 800 ms) | 0.98 s | 232 MB summed | — | — |
| `thread` | 0.77 s | 21.7 MB | 46 | 138 B |
| `async` (`spawn_blocking`, 4 workers) | 0.64 s | 19.8 MB | 50 | 0 |
| `pool` (8 workers) | 1.41 s | 17.7 MB | 25 | 0 |
| `pool` (4 workers) | 2.37 s | 16.5 MB | 13 | 0 |

Scaling, 300 ms stall (fd soft limit raised to 4096 — see *The first wall*):

| N | `thread` wall / RSS / threads | `async` wall / RSS / threads | `pool` W=4 wall / RSS |
| --- | --- | --- | --- |
| 1 | 0.38 s / 13.9 MB / 4 | 0.52 s / 14.1 MB / 8 | 0.46 s / 14.0 MB |
| 8 | 0.46 s / 17.6 MB / 25 | 0.45 s / 17.7 MB / 29 | 0.84 s / 16.8 MB |
| 16 | 0.46 s / 20.1 MB / 49 | 0.52 s / 20.5 MB / 53 | 1.57 s / 17.0 MB |
| 32 | 0.61 s / 26.1 MB / 97 | 0.54 s / 26.3 MB / 101 | 2.99 s / 16.9 MB |
| 64 | 0.63 s / 37.6 MB / 193 | 0.62 s / 37.8 MB / 197 | — |
| 128 | 0.80 s / 60.5 MB / 385 | 0.68 s / 60.9 MB / 389 | — |
| 256 | 1.08 s / 101.1 MB / 720 | 1.15 s / 105.9 MB / 773 | — |

Serial cost for comparison: 8 sessions 3.04 s, 16 sessions 5.74 s, RSS flat
at 15 MB.

## What the numbers say

**Threads are cheap here; sessions are not.** A session costs ~0.4 MB RSS and
three threads (its runtime's reactor/worker, the turn thread, the transport
or dummy worker). 15 sessions fit in 22 MB and 46 threads; 256 fit in 101 MB
and 720 threads. Memory grows linearly and the wall clock stays near one
stall until the scheduler is saturated. The process model sums to 14.5 MB RSS per
session (232 MB for 16); that sum over-counts shared pages — each process's
peak memory footprint was ~7 MB — but even the unique share is several times
the in-process shape.

**A true async task per session is not available.** `Session` is `!Send` —
it holds syntect/onig's raw pointer across an await — so `JoinSet::spawn`
refuses it. The async shape that does compile is tokio's blocking pool
(`spawn_blocking`), which is thread-per-session with four extra runtime
threads, ~1 MB more RSS and no throughput gain. An async core is a rewrite
of `Session` and the blocking LLM client, not a runtime flag.

**A bounded pool trades ceiling for a bound.** Four workers cap concurrency
at four: 15 jobs take 2.37 s instead of 0.77 s, with 16.5 MB instead of
21.7 MB. Eight workers close most of the gap (1.41 s, 17.7 MB). That is the
right knob for admission control, not for the highest ceiling.

**The first wall is file descriptors, not threads or memory.** With the
default 256-fd soft limit, 64 sessions fail to create their tokio runtimes
(`Too many open files`, EMFILE); 15 are fine. Each session's current-thread
runtime owns a kqueue/epoll descriptor and bootstrap opens config, skill,
agent and rollout files. A host must raise the limit (or share one runtime)
before it approaches 50 sessions.

**Shared bootstrap is the second wall.** At 64+ sessions, concurrent
bootstraps write the same `agents/*.md` and `skills/*/SKILL.md` files and
read them back half-written: `missing YAML frontmatter delimited by ---`
appeared in every 64-session run, and one async run at 8 sessions hit it
too. The write path is not idempotent under concurrency; one Host must do
the config/skill/bootstrap writes before sessions start.

## Constraints found in the code

- `Session<'t>` borrows a viewport; a pool needs a sink viewport per session
  (`InlineViewport::headless()` already exists, and this bench uses it).
- `host::session_id()` is nanos + pid — two sessions started in the same
  nanosecond share a rollout path.
- `palette::ACTIVE` is thread-local; session threads must not assume the
  main thread's palette.
- `Signals::install()` is per run; a host wants one process-level handler and
  one shutdown broadcast, not N signal streams.
- MCP managers, provider clients and skill/agent roots are assembled per
  session by `Session::bootstrap` (1,185 lines); the shared half belongs to a
  Host.
- The TUI loop is one `current_thread` runtime over one `Session`. Running N
  sessions in one process does not give the TUI N panes; that is #28's
  transport/client work, not this benchmark.

## The other axis: one turn's tool calls

`examples/concurrency_probe.rs` drives the **real** `RealToolExecutor` (a pty
and a child per call) over `K` independent `sleep 1` commands in three shapes:
`seq` (today's `llm/agent.rs` loop), `threads` (one thread per call) and a
bounded `pool`. This is [#24](https://github.com/linuztx/alter-zero/issues/24)
Option A, not the session host.

8 calls, 1 s each (external RSS/thread sampling):

| shape | wall | RSS | threads |
| --- | --- | --- | --- |
| `seq` | 8.91 s | 8.1 MB | 3 |
| `threads` | 1.23 s | 8.6 MB | 25 |
| `pool` W=4 | 2.34 s | 8.4 MB | 13 |
| `threads` 16 calls | 1.26 s | 9.2 MB | 49 |
| `threads` 32 calls | 1.31 s | 10.2 MB | 97 |
| `pool` W=8, 32 calls | 4.58 s | — | — |
| `pool` W=16, 32 calls | 2.33 s | — | — |

A concurrent call costs ~65 KB RSS and three threads; 32 calls still fit in
10 MB. Cancellation already works across them: with `PROBE_CANCEL_MS=300`,
four `sleep 5` calls stop in 0.45 s and all report failed, in both the
thread and pool shapes — the shared `CancelToken` reaches every child. The
ceiling here is pty/child-process limits and the executor's single-child
assumptions, not thread cost:

- `llm/exec.rs`'s timeout, `kill_process_group` and cancel path assume **one**
  child; a batch needs N process groups and a joint reap.
- `ToolOutput`/`ToolScreen` used to be one untagged stream appended to the front
  call; they now carry a `call_id` and the app routes each chunk to its cell
  (`docs/tools/parallel-tools.md`), so concurrent calls on one executor are
  representable — the executor itself still assumes one child per call, and the
  permission gate is one modal at a time.
- Progressive commit assumes the front call commits first; with per-call ids the
  remaining piece is letting a finished worker resolve while its siblings run.

**If the target is the tool executor**, the measured winner for ceiling is
thread-per-call; for predictable memory and UI ordering, a bounded pool of
4–8 with per-call event tags is the safer first step (8 calls: 2.3 s at W=4,
1.2 s at W=8 vs 8.9 s serial). Start with the cap, keep the thread-per-call
shape behind it.

## Recommendation

**Thread-per-session with bounded admission, plus a shared Host.** For the
session axis, concretely:

1. A `Host` owns the config home, provider/model catalog, MCP configuration,
   skill/agent registries, the bootstrap writes, the signal handler and the
   session-id source.
2. A `Session` owns `App`, rollout, permission gate, its own MCP manager and
   its own current-thread runtime, pinned to one OS thread.
3. Admission is a fixed thread budget (`--concurrency N`), not an unbounded
   `thread::spawn`; the queue is explicit and measurable.
4. Batch `-p` (JSONL jobs) is the first client; the TUI becomes a second
   client only after #28's transport.

Why not the alternatives: `spawn_blocking` adds runtime threads and memory
without throughput; a true async core needs `Session: Send` and an async
transport first; process-per-session remains the isolation baseline but
costs ~7 MB peak footprint (14.5 MB summed RSS) and a full bootstrap per task.

Acceptance for the first Host/Session milestone: 15 jobs in one process,
RSS under 100 MB, no bootstrap warnings, default 256-fd limit respected, and
wall clock within 1.5× the slowest job.

## The first client, implemented

`-p --jobs` (`docs/cli/batch.md`) runs the real headless session per job on a
fixed worker budget. It takes the **thread-per-session with bounded admission**
shape, not `spawn_blocking`: each worker owns a current-thread runtime on its
own OS thread, so `Session`'s `!Send` is irrelevant — it never leaves the
thread that built it. The shared-state walls were fixed where they were
smallest: the first-run seeds write through a temporary file and an atomic
rename, and session ids carry a process-local sequence.

Measured 2026-11-13 with the offline dummy backend and `ALTER_ZERO_STALL_MS=300`
(the worker's own bootstrap included, unlike the harness above):

| jobs | concurrency | fd soft limit | wall | max RSS | stderr |
| --- | --- | --- | --- | --- | --- |
| 8 | 4 | default (1048576 here) | 0.98 s | — | empty |
| 15 | 8 | default | 0.66 s | 18.6 MB | empty |
| 15 | 15 | 256 (`ulimit -n`) | 0.35 s | — | empty |
| 64 | 64 | 4096 (`ulimit -n`) | 0.43 s | 38.5 MB | empty |

The 64-session run used the checked-in `skills/`/`agents/` seeds concurrently —
the scale at which the harness above saw half-written files and `missing YAML
frontmatter` — and booted with empty stderr. That is the atomic-seed fix doing
its job.

A `SIGINT` one second into a two-worker run of five-second jobs exited `130`,
reported both in-flight jobs as interrupted, started none of the queued jobs,
and left stderr empty. That is the acceptance shape: no bootstrap warnings,
well under the RSS bound, and 15 concurrent sessions inside the original
256-fd soft limit.

## The Host

The batch now resolves the process-wide facts **once**, in `tui::host::Host`,
and hands a `&Host` to every session:

- the working directory, home and config home;
- the pre-fork home migration and the first-run skill/agent seeds (already
  atomic — the Host is now the one place that runs them before sessions start);
- the skill and subagent walks, with their parse errors, and this project's
  `skills.json` disabled set;
- this directory's `/settings` entry plus the telemetry/update/tips files and
  the `ALTER_ZERO_*` overrides, and the tips cursor;
- the secrets file, the user's `hooks.json` (path and parse), and the model
  resolver's inputs: the provider table, `.env`, and the saved selection
  (`adopt_selection` now pins a directory once per process, not once per
  session).

A session still clones what it needs into its own registries and stores; the
Host itself is read-only after `Host::resolve`. A single `-p` run or the TUI
passes `None` and resolves the same facts inside bootstrap, so its behavior is
unchanged.

Still **not** done:

- **MCP configuration and managers** are still per session. The parsed sources
  depend on this session's trust state, and each manager owns live connections;
  sharing a connection pool is the next Host step, and needs a measurement
  before it is a design.
- **Signals** are still per session (tokio broadcasts one process handler, and
  each run installs its own stream). The batch is correct because every
  in-flight session answers the signal and stops; a process-level shutdown
  broadcast is still cleaner.
- **The TUI is not a client.** It is one `current_thread` runtime over one
  `Session`; N panes need the transport work in #28, not this batch path.
- **The provider HTTP client** is still built per session. Sharing it is a Host
  concern and a measurement, not a guess.
- The Host is a **snapshot**. A config file edited while a batch runs is not
  re-read by the sessions already started; a fresh process sees it. That was
  already true of a single headless run for everything resolved at bootstrap.
