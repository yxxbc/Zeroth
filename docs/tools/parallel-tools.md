# Parallel tool calls — the visible batch + `⎿ Waiting…`

When the model requests **several tool calls in one assistant turn** (e.g. three
`Bash` commands at once), the TUI shows **all of them up front**: every call a
worker has picked up renders live (its own blinking grey bullet over its own
tailed output — `docs/ui/tool-pulse.md`), possibly several at once, and the ones
no worker has reached yet render as dim `⎿ Waiting…` cells — Claude-Code's
parallel-tool look:

```
● Bash(ping -c 20 google.com)
  ⎿  64 bytes from … icmp_seq=6 ttl=117 time=68.3 ms
     64 bytes from … icmp_seq=7 ttl=117 time=247 ms
     64 bytes from … icmp_seq=8 ttl=117 time=332 ms
     64 bytes from … icmp_seq=9 ttl=117 time=144 ms
     +5 lines (9s)

● Bash(ping -c 20 facebook.com)
  ⎿  64 bytes from … icmp_seq=3 ttl=117 time=71.0 ms
     +2 lines (9s)

● Bash(ping -c 20 x.com)
  ⎿  Waiting…
```

Every running call **tails its own live output** (the last lines + a `+N lines
(Ns · timeout …)` footer, its clock beside the timeout it runs under) as it
streams — see `docs/ui/tool-streaming.md`; the `⎿ Waiting…` siblings are
unchanged, except a queued `Write` whose file streamed into its cell while the
round was generated: it keeps that file's first ten numbered rows under its
`Waiting…` corner, so the cell neither collapses nor grows back when it resolves
(`docs/ui/write-streaming.md`). Only in the live strip: the permission prompt's
context cells keep the one-row `⎿ Waiting…`, since the prompt shows the file.

This lifts the old limitation (`docs/tools/tools.md`, before this change: *"Parallel
tool calls … are executed sequentially (the TUI shows one running tool at a
time)"*). The whole batch is **visible**, and each call commits to scrollback the
moment its turn in the model's order resolves.

**Execution is no longer always sequential.** A batch whose calls are
independent runs on a bounded worker pool (`run_agent_parallel`, below): a round
of three one-second `bash` calls finishes in about one second when the machine
has workers to spare. A batch that is not provably independent keeps the serial
loop. The gate is `llm::tools::batch_is_parallelizable`:

- every call must be a `bash`/`read`/`write`/`edit` call (the tools whose
  executors hold no shared session state);
- a batch mixing `bash` with file tools is serial — the shell can read or write
  any path the file calls name, and a `Write(script.py)` + `Bash(python3
  script.py)` round depends on the write landing first;
- two file calls naming the same path (canonicalized, so `/repo/a`,
  `/repo/./a` and a symlinked directory collide) are serial — `write`/`edit` are
  unlocked read-modify-write, so concurrent edits to one file would silently
  drop one;
- unparseable arguments keep the serial loop, where tool errors are answered
  deterministically.

## Why the batch is displayed before it runs

The display predates the executor: the visible batch was built so every call is
announced up front, then resolved in the model's order. That ordering is now
also the *record* contract — the cells still resolve in the announced order even
when jobs finish in another — while the **execution and the live strip overlap**.
Each event that belongs to one call names it (`call_id`), so a chunk from the
second worker can never land on the first call's cell. The concurrent executor
reuses the same Waiting/Running/Ok/Failed cell states; only `llm` gained a worker
pool, and `app` gained an id-keyed route beside its order. The announced order
remains what `ToolCall::position` replays into the next request.

## The mechanism

### Protocol (`stream/event.rs`)

A new event announces the batch **before** the first `ToolStart`, carrying a
small named struct per call (`ToolCallSummary { name, args, call_id }` — the
*same* strings the call's own `ToolStart` carries, so a `⎿ Waiting…` header
matches the header it shows once running, and the id its later events carry):

```rust
pub struct ToolCallSummary {
    pub name: String,
    pub args: String,
    /// The provider's id for the call; `None` for a scripted/dummy batch.
    pub call_id: Option<String>,
}

/// The model requested a batch of tool calls this round, announced up front so
/// the UI can show every call — the ones not yet executing as `⎿ Waiting…`.
ToolBatch(Vec<ToolCallSummary>),
```

Every call-scoped event gained the same `call_id`: `ToolStart`, `ToolEnd`,
`ToolAnswered`, `ToolRejected`, `ToolNote`, `ToolOutput`, `ToolScreen`,
`ToolTitle`, and `ToolBackgrounded` (which keeps its own `id` — the background
registry's task id — beside it). `None` means "the queue front", which is the
serial path (`!` shell, scripted test) exactly as before.

The `ToolStart`/`ToolEnd` pair's payload is otherwise unchanged. A backend that
never batches (the `!` shell) simply never sends `ToolBatch` — a lone
`ToolStart` still works.

### App state (`app/tools.rs`)

`current_tool: Option<ToolCall>` becomes a **queue**: `tool_queue:
VecDeque<ToolCall>`. Each call is `Waiting` (no worker reached it), `Running`
(its worker announced it), or resolved and gone.

- `ToolStatus` gains a `Waiting` variant (dim, rendered `⎿ Waiting…`).
- `start_tool_batch(&[ToolCallSummary])` fills the queue with `Waiting` cells,
  each carrying its summary's `call_id` (and, when a round announced it, the
  provider's arguments and position). Each call is also stamped with the
  batch's own id (`ToolCall::batch`, a plain counter) — what lets a later
  renderer tell one round's parallel calls from two rounds' sequential ones
  (the MCP run collapse, `docs/tools/mcp.md`). A lone call's is `None`.
- `start_tool_for(call_id, name, args, arguments)` flips **that** `Waiting` cell
  to `Running`; an id no cell holds pushes a fresh `Running` call (a lone call
  whose start names its id). `start_tool(name,args,…)` is the `None` twin and
  keeps the serial path byte-for-byte: flip the front `Waiting`, else push.
- `end_tool_for(call_id, output, ok)` removes the cell that id names — not
  necessarily the front — finalizes it (Ok/Failed), records it in `history`,
  and returns it to commit. `end_tool`/`reject_tool`/`answer_tool`/
  `background_tool` keep their `None` twins; `*_for` variants take the id.
- The live-output setters (`push_tool_output_for`, `push_tool_screen_for`,
  `set_tool_title_for`, `set_tool_note_for`, `set_tool_run_for`,
  `set_tool_truncated_for`) all address the call by id, and `live_tail_for`
  (`App::live_tail_for`) returns **that call's** live rows — two concurrent
  `bash` cells each follow their own progress redraws.
- `current_tool()` still returns the front (old call sites keep working), and
  `tool_queue()` exposes the whole batch for rendering. With several cells
  `Running`, the front is only the model's first call, not "the" running one.
- On interrupt / backend error **every** call in the queue resolves as `Failed`
  (`Interrupted by user` / `Interrupted by a backend error`) — running and
  `Waiting` cells alike, in the batch's order (`App::fail_live_queue`) — and
  each one is recorded and committed above the notice. A call that never started
  is still a call the model made: its cell stays on screen, and its record keeps
  it in the next request's context (see *Interrupting a batch* below). The
  undo-the-submission fast path keys off an **empty** queue
  (`tool_queue.is_empty()`), the old `current_tool.is_none()`. A resolution
  event for a call already resolved (Esc got there first) is a no-op.

### Backend (`llm::agent::run_agent`)

The agentic loop already has every call up front (`RoundOutcome::ToolCalls{calls}`).
It emits one `ToolBatch` (built from `calls`) before any execution, as before.

`run_agent_parallel` (the backend's entry point) hands the loop a
`ParallelTools` seam; `run_agent` passes `None` and keeps the serial loop for
subagents, tests, and every non-qualifying batch. For a qualifying batch the
loop's pre-pass still runs `PreToolUse` hooks and the permission gate **serially
in model order** — nothing runs, and nothing shows as running, while a hook or
the user decides — and writes each allowed call to the classifier's action log
at that point, so a later verdict in the same batch reads the earlier calls in
the order the model asked. Only then do the workers start.

Jobs execute on `min(worker count, jobs)` scoped threads fed by one atomic
index. A worker checks the turn's `CancelToken` before it picks a job up, so a
call the serial loop would never have reached after Esc is not started; the
resolution pass answers it as `Interrupted` without firing a
`PostToolUseFailure` hook for a tool that never ran. **Each worker announces its
own call when it actually starts it** — its `ToolStart` and notes, built on the
loop thread beforehand because `session_command` is not `Sync` — so every call
being executed shows `Running`, and its `ToolProgress` (`ToolScreen`/`ToolTitle`)
carries that call's `call_id`. Resolutions and `PostToolUse` hooks still happen
on the loop thread, in model order, after the workers join; the pass announces
only a step no worker reached (a refusal, or a job Esc cancelled before
dispatch), so no note is sent twice and every `ToolStart` has exactly one
`ToolStart`-shaped event on screen.

The backend supplies the same secret-expanded executor and classifier log it
gives the serial loop, just through `Sync` closures: `ParallelTools::execute`
and `ParallelTools::record`.

### Rendering (`ui/tool.rs`, `ui/live.rs`)

The streaming strip's **preview** slot renders the *whole* live queue (each call's
collapsed cell, blank-line-separated), not just one call. `preview_rows` /
`preview_lines` size and draw from the same queue walk so the box + cursor
geometry stay exact (guarded by the existing `debug_assert`). A `Waiting` cell is
`● name(args)` + a dim `⎿ Waiting…` row; **every `Running` cell tails its own
live rows** (`App::live_tail_for`), and the Ctrl+O transcript's live tail walks
the whole queue too, so a waiting call is never hidden. The `TranscriptCache`
signature keys off the queue's length and each call's `(status, output length)`,
so a Waiting→Running flip, a second call starting, or either call's output
growing invalidates the cache.

The single-`!`-shell preview (its `⎿ Running… (Ns)` elapsed row) is unchanged: the
shell is never batched (queue length 1, `shell` flag).

Two surfaces that used to assume the queue front now ask the **id**:

- The **permission prompt** lifts the call its request names
  (`PermissionRequest::call_id`) to the front of the context
  (`ui::permission_view::asked_first`), so a batch's fifth question shows the
  fifth cell even though the first four are still queued ahead of it.
- **Ctrl+B** names the first running command cell
  (`App::background_target` → `BackgroundRegistry::request_background_for`),
  and only that call's runner takes the latch; its siblings keep running
  (`docs/tools/background.md`).

## Interrupting a batch

Esc — or a backend error — mid-batch ends every call the model asked for at
once, and every one of them keeps its cell, the running call and the
`⎿ Waiting…` siblings alike:

```
● Bash(ping google.com -c 10)
  ⎿  Interrupted by user

● Bash(ls -la)
  ⎿  Interrupted by user

● Conversation interrupted - tell the model what to do differently.
```

Three pieces make that hold:

- **The record.** `App::fail_live_queue` resolves the running (or
  permission-waiting) front call and each `Waiting` sibling, in the batch's
  order, through the same `end_tool` path a `ToolEnd` takes. The records keep
  their batch id, provider call id and position, so the next request's derived
  context (`context::context_messages`) replays the whole round the way the
  wire carried it — one assistant message holding every call, each answered
  `Interrupted by user` (Claude Code's own answer for a tool use an abort left
  without a result), then the `[error]` notice. The siblings used to be
  dropped: the model's next turn saw the first call and nothing of the rest,
  so it no longer knew it had asked for them.
- **The arguments.** A call that never reached its own `ToolStart` knows only
  the header summary the batch announcement carries. The round announcement
  ahead of it (`StreamEvent::RoundCalls`) carries each call's verbatim
  arguments (`RoundCall::arguments`), stamped onto its `Waiting` cell, so the
  replay is the call the model made — a `bash` call's `timeout`, a `write`'s
  `content` — rather than one rebuilt from the summary. A call that does start
  takes its `ToolStart`'s arguments instead, since a `PreToolUse` hook may
  have rewritten what runs (`docs/tools/hooks.md`). The offline dummy announces no
  round, so its interrupted siblings replay through the summary
  reconstruction, as every pre-field record does.
- **The commit.** The interrupt records its red notice *behind* the cells, so
  the history no longer ends at a cell and `ui::tool_commit_lines` — built for
  the one call that just resolved — finds nothing to commit. That used to
  leave the notice alone on screen, with no cell at all, until something
  rebuilt the screen from history (closing a permission prompt does, which is
  why the first cell sometimes appeared). `ui::resolved_tools_commit_lines`
  commits the last N tool records instead, each exactly as its own resolution
  would have — the first flushing any parallel MCP run it was holding
  (`docs/tools/mcp.md`) — which is the rows a rebuild paints from the same history.

A subagent's batch settles the same way (`AgentRun::resolve_tools`, on the
roster's `x`, an Esc that takes its group down, or its own backend error):
every call lands on its transcript, its session view commits them all, and its
stored message list — what a chat continuation resumes from — answers a call
the cancel left unexecuted `Interrupted by user` as well, where it used to say
`[not executed]`, so the continuation reads what the view shows.

## Demo (the dummy backend)

`stream::turn_events` is **prompt-gated** so the demo is vivid without bloating
every turn's scrollback (which would push content off the fixed-size panes the
smoke suite asserts against):

- a prompt mentioning **"parallel"** runs the vivid three-call `Bash(ping …)`
  batch — the user's example (two green, one red) — so `cargo run` with a
  "parallel" prompt and `scripts/smoke.sh`'s dedicated phase show all three cells,
  the running one live over two dim `⎿ Waiting…` siblings;
- any **other** prompt runs a compact two-call `Read`+`Bash` batch (the `Bash`
  waits while the `Read` runs) — the feature is still visible every turn, at the
  **baseline scrollback footprint**, so unrelated smoke phases keep their sizing.

A real backend gets the same display for free via the `ToolBatch` the agent loop
emits, rendering however many parallel calls the model actually requests.

## Inside a subagent

A subagent runs the **same** `llm::agent::run_agent`, so its rounds announce
their batches the same way, and the forwarder passes every event through to the
agent channel verbatim. `AgentRun::apply`'s `ToolBatch` arm is `App`'s twin —
one `ToolStatus::Waiting` cell per announced call, replacing the queue (the
announcement is the round's whole set) — so a subagent's parallel batch shows
exactly as the main turn's does on all four surfaces of that agent's session
view (`docs/agents/agent-tool.md`): the strip previews the whole queue through the
shared `ui::live::live_call_lines`, Ctrl+O expands every cell, each resolution
commits to the agent's own scrollback, and a **permission prompt raised there
shows the asked-about call over every `⎿ Waiting…` sibling** through the same
`queue_chunks` walk the main branch uses (`docs/state/permissions.md`,
`docs/agents/agent-view-streaming.md`).

Two deliberate differences remain, both about aggregation rather than display:
a subagent's calls carry no batch id, so a run of MCP cells is never collapsed
into one `Called {server} N times` line (`docs/tools/mcp.md`), and the parent's own
`● Running {n} agents…` tree shows one sticky activity row per agent rather
than a cell per call — the tree is a roster, not a transcript.

## Limitations

- A batch's cells resolve only after the **whole** batch finishes, so an Esc
  that lands while a slow sibling still runs records a sibling that already
  succeeded as `Interrupted by user`; its side effect is on disk. The resolution
  order is the model's, deliberately — the next iteration can resolve each
  worker's call as it finishes without reordering the records, since records
  already carry `position` (`docs/providers/prompt-caching.md`).
- A parallel batch still asks every call's permission **before** the first
  `ToolStart`, so the waits are serialized in the pre-pass and the batch gains
  no overlap while the user decides. The prompt's *context* is right — it leads
  with the asked-about call by its id (`docs/state/permissions.md`) — but the
  approvals themselves cannot run concurrently (one modal, one question at a
  time).
- `write`/`edit` on two distinct paths may still race through an **aliased**
  path the canonicalizer cannot see (a symlink created during the batch), and a
  `bash` batch can of course make its own calls depend on each other. The gate
  is a safety rail, not a dependency analysis; the model is told independent
  calls are the ones that may be batched.
- A very large batch grows the live region upward; it is clamped to the terminal
  height like the rest of the live region (the box then scrolls internally). No
  per-batch cap / "+N more" collapse yet.
- An interrupt keeps the calls it can see, which is the live queue. A round's
  calls that were never announced still leave no record, so the next request
  does not carry them: the ordinary calls behind a foreground agent group (the
  batch is announced only once the group finishes, `docs/agents/agent-tool.md`), and
  a task call the loop had not reached yet (task calls render no
  cell and are never announced, `docs/tools/task-tools.md`).
