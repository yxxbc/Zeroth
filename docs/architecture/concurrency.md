# Concurrency — where the turn blocks, and what actually runs at once

Date: 2026-11-12

Status: **measurement and inventory**, not a proposal. Nothing here is
implemented; the "what could change" sections are options with their prices
attached, for a decision to be made before any code moves.

Prototype results for both axes — N sessions in one process and a parallel
tool batch — are in [`multithread-core.md`](multithread-core.md).

## The complaint

"A competitor's benchmark has 15 sessions working at once, a 16-thread compile
in the background, and stays responsive. alter-zero is one session." That is
the observable half. The half worth writing down is **what this design
deliberately serialises and what it does not**, because several of the
serialisation points are load-bearing (cancellation, permissions, the transcript
invariant) and only one of them is free to change.

## What runs concurrently today

| Work | Runs on | Blocks the TUI? |
| --- | --- | --- |
| The turn itself (`run_agent`'s round loop) | one `std::thread::spawn` per turn (`backend.rs::spawn`) | no |
| Each request's network I/O | a **second** detached thread per request (`openai.rs::run_transport`, `thread::spawn` at `:548`) | no |
| `agent` tool calls (subagents) | one thread each, launched from `run_agent_calls` | **the calling turn's thread — yes** (see below) |
| A `!command` | one thread (`tui/shell.rs::spawn_shell_command`) + 2 pipe-drain readers | no |
| MCP connects at startup | worker threads, concurrently | no |
| One MCP `call_tool` | worker thread under the call's deadline | no |
| `@` file search, Ctrl+V clipboard, `/model` fetches, `/login`, `/usage` | per-task worker threads (`tui/workers.rs`) | no |
| Background shells, background agents | own threads, reported on their own channels | no |
| Checkpoint snapshot | the loop's own thread | **yes** (see below) |
| Session-file append (`recorder.sync`) | the loop's own thread (event-loop bottom) | **yes**, but append-only and no fsync |
| Frame scheduling, rendering, all `App` state | the loop's thread (`current_thread` tokio runtime) | — |

So the shape is: **one turn at a time by design, but a turn is not one thread.**
A single turn already fans out to a transport thread, N subagent threads, N tool
threads. What it never does is run **two turns**, or two ordinary tool calls,
at once.

## The serialisation points, and why each exists

### 1. Ordinary tool calls in a round are sequential — by choice

`llm/agent.rs:725` is `for call in &rest { … execute(call, …) }`. The batch is
*announced* up front (`StreamEvent::ToolBatch`) so the UI shows every call with
`⎿ Waiting…`, but execution is one at a time. `docs/tools/tools.md` and
`docs/tools/parallel-tools.md` both say so, and the rationale is in the latter: the
existing single-child timeout, `kill_process_group` and cancellation machinery
assumes **one** child; interleaved stdout needs multi-child reaping; and the
progressive-commit pipeline assumes calls resolve in order.

The cost: a round asking for three 20-second commands takes 60 seconds. This is
the most visible "concurrency" gap and also the most expensive to close (see
*Option A*).

### 2. A subagent batch blocks its parent turn

`run_agent_calls` (`backend.rs:1586`) launches every foreground agent as a
thread, then **polls in a loop** with `std::thread::sleep(AGENT_WAIT_POLL)` —
30 ms — until they are all done (`backend.rs:1700-1709`). The polling thread is
the turn's thread, so the lead cannot do anything else while a foreground
subagent group runs. That is intended (`docs/agents/agent-tool.md`: the lead's result
*is* the agents' results), but the 30 ms sleep also means the parent's own
cancellation and the Ctrl+B hand-off are polled, not awaited.

The cost is not latency to the user — the agents run in parallel and the roster
ticks — it is that the lead's conversation is **idle** for the duration. A model
that launches one agent and could usefully read a file meanwhile cannot.

### 3. One turn at a time, everything else queues

`App::turn_active` gates `start_turn`; Enter steers the running turn, Tab queues
a follow-up (`docs/input/queue.md`). This is a product decision, and it is the one the
benchmark is really pointing at: 15 *sessions*, not 15 turns in one session.
Nothing in the architecture forbids N conversations in one process — `agents`
already keeps N transcripts with their own channels, renders and queues. The
blocker is `App`: one `history`, one `streaming`, one `queued`, one composer.

### 4. Checkpoint snapshots run on the event loop

`tui/turn.rs:402::checkpoint_turn_end` → `CheckpointStore::snapshot`
(`checkpoint.rs:821`) runs **three `git` subprocesses synchronously on the
loop's thread** at every turn end: `add -A`, `commit`, `rev-parse HEAD`. Enabled
by default (`ALTER_ZERO_CHECKPOINTS` unset = on). `docs/state/checkpoint.md` records
the price: *"a cold snapshot around a second on ordinary hardware and several on
the slowest"*. Everything the loop owns — input, frames, the spinner, Esc — is
frozen for that long, **once per turn**.

The three-way budget (`20 000` files / `256 MiB` / `500 ms` probe) bounds the
*probe*, not the snapshot. There is no timeout on `git add -A` itself.

### 5. The recorder appends on the loop's thread, without fsync

`recorder.sync` runs at the loop bottom (`bootstrap.rs:1134`) and on shutdown.
It is append-only, watermarked, and skips entirely when history did not move — so
a streaming turn pays nothing. But a turn that recorded a history item does a
`write_all` of that item on the loop's thread. No `sync_all`, so this is a page
cache write: microseconds normally, but a full page cache or a slow disk turns
it into a stall nobody bounded.

### 6. One HTTP connection pool, one request at a time per turn

`llm/mod.rs::cached_client` builds one `reqwest::blocking::Client` (cached per
`op_timeout`) with `pool_idle_timeout(30 s)`. A turn makes one request at a
time, so the pool serves exactly one connection. Nothing is wrong; it simply
means concurrency work would need `pool_max_idle_per_host` raised from its
default (unbounded actually, so fine) and the **blocking** client's pool is not
the limiter — the one-request-at-a-time turn loop is.

## What "concurrency" could mean here, priced

### Option A — run an ordinary tool batch concurrently

Touch: `llm/agent.rs`'s loop, `llm/exec.rs` (single-child assumptions),
`llm/backend.rs` (the executor's `&mut dyn FnMut` sink becomes per-call),
`app/tools.rs` (per-call output buffers), `ui/tool.rs` (several live cells
already render — `ToolBatch` did that part).

Risks that decide it:

- **Cancellation.** `kill_process_group` currently kills *the* child. A batch
  means N children, each with its own process group, all needing the same
  guarantee. `docs/tools/parallel-tools.md` calls this out as the reason it was not
  done.
- **Output ordering.** `ToolOutput` chunks currently arrive in one stream and
  are appended to the front call. Concurrent calls need per-call tagging on the
  event, which changes `StreamEvent::ToolOutput`'s shape and every consumer.
- **Permissions.** Two calls asking at once is a queue the permission system
  has never had to model (it is explicitly one modal at a time —
  `app/permission.rs`, `docs/state/permissions.md`).
- **Transcript invariant 4.** Progressive commit assumes the front call commits
  first.

Price: high. Payoff: a 3×20 s batch becomes 20 s. Worth it, but it is a
project of its own and it is the change most likely to break cancellation.

### Option B — take the checkpoint snapshot off the loop

Touch: `checkpoint.rs` (make `snapshot` spawnable), `tui/turn.rs` (fire and
forget, record the SHA when it arrives), `recorder` (accept a checkpoint that
lands after the turn end), `docs/state/checkpoint.md`.

Risks: the checkpoint is recorded at a history length, and a late one could land
after a backtrack — the recorder keys checkpoints by `after-items`, so a late
result needs a generation check before it is accepted. `git add -A` and the
user's own `git` in their work tree would race (already possible, but the window
currently is "the turn end" rather than "whenever git finishes").

Price: low — it is one call site and a channel. Payoff: removes a
once-per-turn freeze of up to several seconds on a cold store, on every
machine. **This is the single cheapest concurrency fix in the file.**

### Option C — let the lead work while a foreground agent runs

Touch: `run_agent_calls`'s poll loop becomes a channel wait; the lead's loop
needs a "results pending" state so a round's tool results can arrive late.

Price: medium-high, and it collides with the prompt-caching rule that tool
results stay contiguous (`docs/providers/prompt-caching.md`). Probably not worth it: the
lead *asked* for the agents' answers, so idling is semantically right.

### Option D — N sessions in one process

This is what the benchmark actually measures and what `agents` already
demonstrates is possible. Touch: everything that assumes one `App` — `history`,
`streaming`, `queued`, `composer`, `recorder`, `checkpoints`, `session` file,
plus a session switcher. The view machinery (per-view flow, per-session
transcript caches) is the easy half; the `App` split is the rest.

Price: very high. Payoff: the thing that makes a 15-session comparison look like
a comparison. Note the roadmap already treats sessions as separate **processes**
(`docs/state/resume.md` is a file-per-session store), so this is a redesign, not a
refactor.

## The one thing worth doing first

**Option B.** It is small, it removes a measured freeze, it needs no new
concurrency model, and it is invisible to every other invariant. Everything else
on this page changes how a turn is structured; that one only changes *when* a
subprocess runs.

## What to measure before choosing

None of the above is worth deciding on intuition. What this codebase lacks is
the instrumentation to see it:

- **Time per turn end spent in `checkpoint_turn_end` and `recorder.sync`** —
  one log line each, gated on an env var, would settle whether Option B is worth
  the ~50 lines.
- **Thread count and stacked `ps -M` under a 3-call batch and a 4-agent batch** —
  to know whether the per-request transport thread + per-agent thread + per-tool
  thread shape has a ceiling worth raising.
- **A wall-clock line per round**: request sent → first token → last token →
  tools done. The benchmark's own numbers are medians over a whole scenario; per
  round is what a fix would move.

A frame-time log (`docs/architecture/input-latency.md`, item 5) would serve both documents;
the same env-gated writer can time a turn end.

## Reproducing the inventory

- **Sequential batch**: `llm/agent.rs:725` (`for call in &rest`).
- **Subagent poll**: `llm/backend.rs:1586` (`run_agent_calls`) and the
  `AGENT_WAIT_POLL` sleep at `:1708` (30 ms, `:1484`).
- **One turn at a time**: `App::turn_active`; the queue semantics in
  `docs/input/queue.md`.
- **Checkpoint on the loop**: `tui/turn.rs:345` → `:402` →
  `checkpoint.rs:821`; enabled-by-default at `checkpoint.rs:536`.
- **Recorder on the loop**: `tui/bootstrap.rs:1134` → `recorder.rs:268`.
- **Transport thread per request**: `llm/openai.rs:548`.
