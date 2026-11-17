# Concurrency — where the turn blocks, and what actually runs at once

Date: 2026-11-12

Status: **measurement and inventory**, not a proposal. The "what could change"
sections are options with their prices attached, for a decision to be made
before any code moves. Two of them were taken since this page was written: the
turn-end checkpoint now snapshots on its own thread (*Option B*) and the urgent
frame path landed (`input-latency.md`). What is left, and what each measured
cost is, is in *The render loop shares the input thread* below.

Prototype results for both axes — N sessions in one process and a parallel
tool batch — are in [`multithread-core.md`](multithread-core.md).

## The complaint

"A competitor's benchmark has 15 sessions working at once, a 16-thread compile
in the background, and stays responsive. zeroth is one session." That is
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
| Checkpoint snapshot | a thread of its own (`tui/turn.rs::start_checkpoint_snapshot`) | no |
| Checkpoint **restore** (a backtrack, a `/resume`) | the loop's own thread | **yes** (see below) |
| Session-file append (`recorder.sync`) | the loop's own thread (event-loop bottom) | **yes**, but append-only and no fsync |
| Ctrl+O transcript warm (`transcript.warm`) | the loop's own thread (event-loop bottom) | **yes**, one new item per turn, a whole reload after `/resume` |
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

### 4. Checkpoint snapshots left the loop; their restore did not

This was the freeze *Option B* was written for, and it is fixed:
`tui/turn.rs::start_checkpoint_snapshot` spawns the snapshot and the loop keeps
drawing, taking the result on `checkpoint_rx` (`tui/event_loop.rs` arm 13).
Measured on a 20 000-file tree, the snapshot itself is 0.17 s — no longer
anybody's frame.

Its **mirror** did not move. `restore_to` (`tui/resume.rs`) runs `snapshot` +
`git reset --hard` + `git clean -fdq` inline, so a backtrack or a `/resume`
still freezes the loop: 1.5 s measured on a dirty 20 000-file tree, and nothing
bounds `git clean` on a slow disk. The `checkpoint_epoch` guard the late
snapshot already needs applies unchanged to a late restore, which is what makes
moving it the cheap half of the remaining work
(*The render loop shares the input thread*, item 3).

### 5. The recorder appends on the loop's thread, without fsync

`recorder.sync` runs at the loop bottom (`bootstrap.rs:1141`) and on shutdown.
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

### Option B — take the checkpoint snapshot off the loop (done)

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

**Option B — done.** It was small, it removed a measured freeze, it needed no
new concurrency model, and it was invisible to every other invariant. The same
argument now picks the three items at the end of *The render loop shares the
input thread* below: each is one call site, and each removes a measured stall
rather than changing how a turn is structured.

## What to measure before choosing

None of the above is worth deciding on intuition. The first item below is the
one that was acted on — `ZEROTH_TIMING=<path>` now writes it
(`tui/timing.rs`), and it is what priced the checkpoint move. What is still
missing:

- **Time per turn end spent in `recorder.sync`** — the checkpoint half is
  measured (`checkpoint.snapshot`, `checkpoint.dispatch-wait`); the recorder's
  own `write_all` has no line yet.
- **Thread count and stacked `ps -M` under a 3-call batch and a 4-agent batch** —
  to know whether the per-request transport thread + per-agent thread + per-tool
  thread shape has a ceiling worth raising.
- **A wall-clock line per round**: request sent → first token → last token →
  tools done. The benchmark's own numbers are medians over a whole scenario; per
  round is what a fix would move.

A frame-time log (`docs/architecture/input-latency.md`, item 5) would serve both documents;
the same env-gated writer can time a turn end. The measurements below came from
throwaway `examples/` probes instead, which is the argument for that log.

## The render loop shares the input thread

Added 2026-11-17. This is the answer to "the UI renders on one thread, and that
thread is also the input thread — isn't that dangerous?" It is worth separating
the two halves of that sentence, because they have different answers: the risks
that actually bite are not the ones the shape suggests.

### What is true

Rendering and input handling do run on the same thread — the `current_thread`
tokio runtime in `main.rs`. `draw_rx` and `events` are two branches of one
`select!` (`tui/event_loop.rs`), so a frame and a keypress are never *concurrent*,
by construction. That is not an oversight: it is the whole reason the frame
decisions are simple. One thread owns `App`, the viewport's diff (`prev`), the
cursor seat and the scrollback queue, so none of them needs a lock, and a frame
can never observe a half-applied state change.

The premise that this is dangerous usually imports two assumptions that do not
hold here:

- **Input is not lost while a frame runs.** crossterm's `EventStream` is its own
  reader thread (`crossterm-0.29.0/src/event/stream.rs:44`), so keys are drained
  from the pty even while the loop is busy, and the pty itself buffers what has
  not been read. Measured both ways: with nothing reading, 1024 bytes went in and
  1024 came back out — identically — and the writer simply blocked past that,
  rather than being dropped. A slow frame costs *latency*, then, not keystrokes.
- **State is not shared.** Nothing renders off a snapshot some other thread is
  writing, so there is no torn frame to be careful about. Cheaper than any lock.

So the question is not "is this unsafe" — it is "how long can one iteration of
this thread block, and what is the user doing meanwhile". The rest of this
section is that number.

### What one iteration costs (measured)

Release build, 120-column frame, macOS, on the transcripts in this machine's own
`~/.zeroth/sessions` (611–1309 items, 3.3–4.1 MB each) — the worst case for a
renderer, and a real one.

| Work | On the loop's thread | Measured |
| --- | --- | --- |
| A streaming frame, 1 KB reply | yes | 0.05 ms |
| A streaming frame, 64 KB reply | yes | 0.10 ms p50 |
| A streaming frame, 1 MB reply | yes | 1.2 ms p50, 2.3 ms p95 |
| Idle frame, 1066-item history | yes | **0.02 ms** |
| Conversation rebuild (resize) | yes | 24–36 ms |
| Ctrl+O transcript, one new tool result | yes | **1.4 ms** |
| Ctrl+O transcript, `/resume` swapped in | yes | **1.07 s → 0.11 s**, after the highlight cap |
| Checkpoint snapshot at turn end | no (own thread) | 0.17 s incremental, 0.93 s cold, on a 20k-file tree |
| Backtrack/resume restore (`git reset --hard` + `git clean -fdq`) | no (own thread) | 1.5 s on a dirty 20k-file tree |

The steady state is not the problem, and that is the part the shape was chosen
for: a streaming frame is cheap because `StreamRender` commits completed lines
to scrollback once and the strip previews one row, so a frame's cost follows the
*last line*, not the reply — 1 KB and 64 KB are indistinguishable, and a
1066-item history costs nothing at all because it is not re-rendered. `render_live`
re-rendering the whole reply (36 ms at 1 MB) is the unit-test fallback only; the
loop always passes `Some(preview)`.

Both of the stalls below were closed on 2026-11-17; the numbers above are the
after, and *What to do, in order* records what each cost.

1. **`transcript.warm` after a `/resume` — 1.07 s → 0.11 s.** The Ctrl+O cache
   freezes a rendered prefix and appends one item per turn (1.4 ms — the design
   is right), but a `/resume` bumps `history_generation`, the frozen key
   mismatches, and the whole loaded conversation re-renders in one call. All of
   that second was syntax highlighting: 986 ms of colour across the session's
   eight markdown file cells, worst single cell 526 ms. Bounding it by bytes
   (`CODE_HIGHLIGHT_MAX_BYTES`) is the fix — the full measurement and why a
   time-based guard does not work are in `docs/ui/long-lines.md` §7b. The warm
   itself necessarily stays whole-history: a partial cache would open Ctrl+O
   missing items, and the overlay cannot be entered mid-warm anyway.
2. **The checkpoint restore — off the loop.** Its mirror, `restore_to`, ran
   `snapshot` + `git reset --hard` + `git clean -fdq` inline: 1.5 s frozen on a
   dirty 20 000-file tree, spinner and Esc with it. It now spawns like
   `start_checkpoint_snapshot`, reports on `restore_rx`
   (`tui/event_loop.rs` arm 14), and `on_restore_done` toasts the outcome. The
   token check is the same shape as `checkpoint_epoch`: a restore that lands
   after a newer one started is dropped. The caller has already rewound history
   by then, so nothing downstream waited on the tree having moved.

Everything else that *could* be a stall is already off-thread: the reply backend,
the transport, `@` search, clipboard, model lists, `/login`, `/usage`, MCP, link
cards, git diff, and the checkpoint snapshot (`tui/workers.rs`, `tui/diff.rs`,
`tui/turn.rs`).

### Why a render thread is not the fix

The obvious move — a second thread that owns the viewport, fed frames over a
channel — buys 1.2 ms of a 1.07 s stall, and pays for it by giving up the two
properties above: `App` would have to be `Send` (it holds `RefCell` render
caches by design, `app/drafts.rs`), and a frame would then race the state change
that requested it. The stalls are not *rendering* costs in the sense that
matters — they are one `O(history)` render and one `git` invocation, both
avoidable at their source. A frame-time log
([`input-latency.md`](input-latency.md) item 5) is the cheap way to keep this honest.

### What to do, in order

All three are done (2026-11-17), and none needed a concurrency model — each was
one call site, which is the actual answer to the complaint: the thread shape is
fine, and what was left were measured one-off costs that a single thread made
easy to find.

1. **Bound the highlighter — done, `CODE_HIGHLIGHT_MAX_BYTES` (2 KB).** Not
   the "one `if`" this doc guessed at: the expensive rows cannot be predicted
   from the text, so the cap bounds *total* work per cell and the constant came
   from a sweep (`docs/ui/long-lines.md` §7b). 986 ms of colour → 40 ms.
2. **Do the `/resume` warm after the paint — dropped, with the reason.** It
   cannot be chunked usefully: the cache has to hold the whole history or Ctrl+O
   opens with items missing, and the overlay cannot be entered while the warm is
   in flight. Item 1 removed the cost it was meant to hide.
3. **Move the restore to a worker — done.** `restore_to` spawns, the result
   lands on `restore_rx`, and the token guard replaces the inline ordering.

## The render loop shares the input thread — reproducing

The probes were throwaway `examples/` files, deleted; the numbers above are the
record. Rebuild any of them the same way: `cargo run --release --example …`
against a real rollout (`~/.zeroth/sessions/YYYY/MM/DD/rollout-*.jsonl`,
`session::parse_session` to items, `App::load_session` to load them). The frame
figures need the loop's own shape — `StreamRender::preview` once, then
`render_live_with_preview` — not `render_live`, which is the unit-test fallback.

## Reproducing the inventory

- **Sequential batch**: `llm/agent.rs:725` (`for call in &rest`).
- **Subagent poll**: `llm/backend.rs:1586` (`run_agent_calls`) and the
  `AGENT_WAIT_POLL` sleep at `:1708` (30 ms, `:1484`).
- **One turn at a time**: `App::turn_active`; the queue semantics in
  `docs/input/queue.md`.
- **Checkpoint off the loop**: `tui/turn.rs::start_checkpoint_snapshot` →
  `checkpoint.rs:897` on a spawned thread, its result on `checkpoint_rx`
  (`tui/event_loop.rs` arm 13). Enabled by default at `checkpoint.rs:536`. Its
  mirror, `restore`, still runs inline (`tui/resume.rs::restore_to`).
- **Recorder on the loop**: `tui/bootstrap.rs:1141` → `recorder.rs:279`.
- **The transcript warm**: `tui/bootstrap.rs:1295` (`self.transcript.warm`),
  now bounded by `ui::file_cell::HighlightBudget`;
  `ui/transcript.rs::ensure_frozen`; the per-item cell is
  `ui/tool.rs::tool_full_lines` → `ui/file_cell.rs::numbered_body_lines`, the
  highlighter `highlight::Highlighter::line`.
- **Input liveness**: `crossterm`'s `EventStream` reader thread,
  `crossterm-0.29.0/src/event/stream.rs:44`; `docs/architecture/input-latency.md`
  for the frame floor.
- **Transport thread per request**: `llm/openai.rs:548`.
