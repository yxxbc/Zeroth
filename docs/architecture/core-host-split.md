# The core/host split — a phased plan

Date: 2026-11-23

Status: **proposal**. Phase 1 is a small, independently mergeable slice; nothing
past it starts without the trigger named below and the user's go-ahead. The
decision this plan extends — and deliberately does **not** overturn — is
[`core-seam-decision.md`](core-seam-decision.md).

## The question

"Does this need a core architecture?" The answer depends on which "core" is
meant, and the two readings have different answers:

- **A pure library with the terminal at the boundary** already exists and is
  enforced: `src/app/` and `src/ui/` contain no filesystem, clock or
  environment access, the whole crate is one binary over `src/lib.rs`, and
  `tests/api_surface.rs` pins the public surface at compile time. A separate
  `zeroth-core` crate would buy no isolation the `lib.rs` line does not.
- **A session core that owns a conversation without a terminal in its type** —
  so one process can run several conversations, and a frontend can attach
  later — does not exist. `tui::Session` is the only owner of a conversation,
  it borrows the terminal for its whole lifetime, and `App` is one
  conversation.

This plan covers the second reading. It keeps the decision's conclusion (the
full split is not worth its price yet) and turns the "keep it cheap" rules into
an executable order of work whose first steps pay for themselves on today's
single-session TUI.

## Where the cost actually is

Measured today (2026-11-23):

| Fact | Number | Why it costs |
| --- | --- | --- |
| `Session` fields | **113** | one struct holds the conversation, its runtime, its caches and the terminal |
| … of which full-screen view state (`full_*`) | **23** | cannot be rebuilt without the terminal, so a second pane cannot exist |
| `impl Session` blocks | **36**, in **29** files | a second conversation would need a second copy of every handler |
| `Session<'t>` holds | `&'t mut InlineViewport` | the terminal's lifetime *is* the session's lifetime |
| `fullscreen()` branches in `src/tui/` and `src/ui/` | **16** (15 production, 1 test) | the mode is a flag read at each call site, not a seam |
| `App` fields | **81** | `history`, `streaming`, `queued`, `input` — a conversation, already pure |

The important reading of that table: **`App` does not need splitting.** It is
already the per-conversation model, unit-tested and free of I/O. What N
conversations needs is one `App` **per** conversation plus a runtime and a view
around each — and today the runtime and the view are welded into `Session`,
beside the one terminal.

The process-wide half is already separated: `Host` (`src/tui/host.rs`) holds the
read-only facts `-p --jobs` shares, and `batch.rs` proves N sessions in one
process works when each owns a thread (`multithread-core.md`: 38.5 MB at 64
concurrent). The TUI is the client that cannot use that shape.

## The target shape

```
Host                 process facts: config home, provider table, secrets, hooks
 │
Workspace            the session table, the one terminal, the one runtime
 ├── Conversation 0   App · runtime (models, channels, stores, queues) · view
 ├── Conversation 1   …
 └── Screen           inline | full-screen | headless  (one small interface)
```

- **Downward**: a frontend turns keys, mouse and resize into `Action`s and calls
  one conversation's handlers.
- **Upward**: a conversation reports `StreamEvent`s on its own channel; the
  workspace merges them into the loop, tagging the conversation.
- **The invariant that must not move**: one stdin reader (invariant 1), the
  pure `App`/`ui` decision layer, and the `StreamEvent`-plus-channel shape
  (never shared mutable state across threads).

## The phases

Each phase is one or a few commits, buildable and shippable on its own. A phase
does not start until the one before it is green on CI and in the smoke suite.

### Phase 1 — `ConversationView` (the first slice, mergeable now)

**What**: move the 23 full-screen view fields — and, while here, the
screen-agnostic renderers (`render`, `agent_render`, `transcript`, `context`),
the frame's pin/hit offsets and the link-card previews — off `Session` into one
`ConversationView` value in `src/tui/view.rs`. The frame rows rebuild from
`App` plus a size; the pointer, scroll anchor and expanded set are interaction
state carried with the view, and the link-card previews are a display cache
restored from the process cache (`link_cards::load`, not product state).

**Why it pays off alone**: 23 of 113 fields become one value with one owner;
the frame cache becomes testable without a terminal; the "same screen always
draws the same frame" property gets a test that a second, fresh view agrees
with an incremented one. This is exactly the decision page's step 2, and it
needs no `Screen` seam.

**Files**:

- `src/tui/mod.rs` — the struct field and the `Default` construction;
  `Session` gains `view: ConversationView`.
- `src/tui/view.rs` — the new type, its `Default`, and the frame builders
  moved onto it (`full_tail_lines`, `tailed_with_stream`, `draw_conversation`,
  `apply_hover`, `apply_selection`, `full_window`, …), with the history half
  extracted as the free `frame_history(app, view, width, screen_width, …)` so
  its cache is testable without a terminal.
- `src/tui/actions.rs`, `src/tui/commit.rs`, `src/tui/stream.rs` — the call
  sites (`self.full_x` → `self.view.full_x`), mechanical.
- `src/tui/view.rs`'s tests — construct a view explicitly; add
  `a_fresh_view_rebuilds_the_same_frame`.

**Invariants**: no new public API (the TUI is binary-private); no behaviour
change; a view cache is never the only copy of anything the product needs (the
frame rows rebuild from `App` + size; the link cards are a display cache over
the wire's URLs). `src/ui` untouched.

**Verify**: `cargo test --bin zeroth` (the view tests), the broad lib sweep from
`AGENTS.md`, `cargo build && scripts/smoke.sh 156 164` plus one inline phase
(e.g. 055), `cargo fmt --check` and clippy.

**Rollback**: one `git revert`; nothing is persisted, no format changes.

### Phase 2 — the `Screen` seam

**What**: `Session` stops naming `InlineViewport`. A small `Screen` enum
(`Inline`, `Full`, `Headless`) implements `commit`, `draw_frame`,
`draw_overlay`, `suspend`, `resume`, `restore` and `live_rect`; the 15 production
`fullscreen()` branches in `src/tui/` move behind it. `App::fullscreen()` stays
as the *pure layout signal* the `ui` builders read — it is data, not a boundary
branch.

**Why it pays off alone**: the loop stops asking "which mode am I in" at every
call site, the full-screen frame gets one entry point, and the headless path
becomes a real third implementation instead of a `headless` field checked in
places.

**Files**: `src/tui/screen.rs` (new), `src/term.rs` (the viewport's public
surface narrows to what the enum needs), `src/tui/mod.rs`, `src/tui/view.rs`,
`src/tui/actions.rs`, `src/tui/bootstrap.rs`, `src/tui/startup.rs`,
`src/main.rs` (construct the variant).

**Invariants at risk**: one stdin reader, created after the init cursor query
(invariant 1); the inline mode's two-way resize reflow (invariant 3); the
overlay repaint rule. Each has a smoke canary (phases covering resize, Ctrl+O,
permission prompts) and must stay green.

**Verify**: the whole gate, `scripts/smoke.sh` for both modes' phase sets
(156/164 and the inline suite's resize/overlay phases), plus
`cargo test --bin zeroth`.

**Rollback**: land it whole; a per-mode flag to choose the old path is the
"half-built seam" the decision page forbids. Revert the series if it regresses.

### Phase 3 — `Conversation` (the runtime), `Workspace` (the table)

**What**: group the non-terminal, per-conversation fields of `Session` —
`models`, the nine channels, the registries and stores, the clocks, the
inflight/reaping handles, `cwd`, the recorder — into
`tui::conversation::Conversation`. `Session` becomes a thin driver over
`{ screen, view, conversation }`, and a `Workspace` owns one conversation plus
the one terminal and runtime. **No behaviour change and no second conversation
yet**: the point is that the shape admits one.

**Why it pays off alone**: every field on `Session` after this is either the
terminal, the view, or the conversation — the struct's section comments become
the type. A new handler can take `&mut Conversation` and cannot reach the
terminal by accident.

**Files**: `src/tui/conversation.rs` (new), `src/tui/mod.rs`, every module that
owns an `impl Session` block whose methods touch only conversation state (29
files — moved in batches, per area, each batch its own commit), `src/tui/event_loop.rs`
(the loop borrows the conversation, not the session, for arms that do not draw).

**Invariants**: the borrow split must keep every `select!` future borrowing a
**distinct field** (the comment on the channel fields is the contract); no
handler may hold two mutable borrows it used to get from one struct. If a
handler needs both the conversation and the screen, it stays on the driver.

**Verify**: full gate; the broad lib sweep; the whole smoke suite (this phase
touches the loop); `-p --jobs` batch tests.

**Rollback**: each area's move is its own commit, so a mis-split reverts one
area, not the phase.

### Phase 4 — a second conversation in one TUI (the product trigger)

**What**: the first feature that needs N conversations. The smallest useful
shape is **in-process tabs**: a `Workspace` with `Vec<Conversation>` (and one
`ConversationView` per tab), each with its own channels; the loop's `select!`
merges per-conversation events through one `(id, event)` channel; the existing
session bar (which already switches subagent views) grows a `+` row and a
`/sessions` command. Daemon-plus-attach is deliberately **not** in this phase:
it needs a transport, and tabs do not.

**Why the plan stops here without a trigger**: nothing in today's product needs
two conversations in one window; several terminals or `-p --jobs` cover the
benchmark. The trigger from the decision page still applies: only build this
when two live conversations in one TUI are asked for.

**Verify**: a new smoke phase starts two conversations, runs a dummy turn in
each, asserts both keep painting and neither's queue leaks into the other; a
memory line in `multithread-core.md` for two live `App`s plus two views.

**Rollback**: the tab feature can ship behind a command while the workspace
holds one conversation; removing the command returns to Phase 3.

### Phase 5 — Host completion (independent of the rest)

**What**: the two remaining process-level items — a shared **MCP connection
pool** and one **process-level signal handler** — on `Host`. They can land
before or after any phase above, and `core-seam-decision.md` already lists them
as triggers for the batch's benefit.

**Verify**: the MCP phases and the signal tests; `-p --jobs` memory unchanged.

## The rules that keep it cheap (until Phase 3 lands)

The decision page's five rules stay. This plan adds the checks that keep *this*
order honest:

1. **New per-conversation product state goes on `App`** (pure, unit-tested).
   New per-screen cache goes into `ConversationView`. New process fact goes
   through `Host`. Nothing new on `Session` unless it is the one terminal or a
   conversation's runtime.
2. **No new `static` or `thread_local` for session state.** The two known
   pieces of debt (`palette::ACTIVE`, `OVERLAY_ACTIVE`) do not grow.
3. **No new `self.fullscreen()` branch in `src/tui/`.** Phase 2 removes them;
   a new one is a regression. (`ui` may read `App::fullscreen()` — that is the
   pure layout signal — but a new branch there needs a test.)
4. **Send messages; never share mutable state.** `StreamEvent`-plus-channel,
   no `Arc<Mutex<App>>`.
5. **A new cache must be constructible from state plus a size**, and its test
   must build it twice and compare (Phase 1's test is the template).
6. **A session-scoped behaviour must name its field group** when it lands —
   view, conversation or terminal — in the struct's section comment. That
   comment is what makes Phase 3 mechanical.

## What would make us re-decide

- **The trigger fires**: a product feature needs two live conversations in one
  TUI, or a daemon/attach client, or two bugs in one release trace to "assumes
  one `App`/`Session`". Phase 4 starts.
- **Phase 1 or 2 does not pay off**: if the view extraction shows no measurable
  simplification and no test value, stop there and keep the rules. The decision
  stays "defer".
- **The batch needs more than the Host items**: if `-p --jobs` wants a second
  frontend (a machine-readable event stream), Phase 2's seam is the place to
  build it, and the order can swap Phases 3 and 5.

## What this plan is not

- Not a rewrite of `App` into several structs: it is already the conversation
  model, and moving its fields buys nothing before Phase 4.
- Not parallel tool calls inside a turn — *Option A* in
  [`concurrency.md`](concurrency.md), a separate decision.
- Not the inline/full-screen default — [`full-screen.md`](../ui/full-screen.md)
  owns that.
- Not a `zeroth-core` crate: the `lib.rs` line already is the crate boundary.

## Where this is tracked

The decision and its triggers: [`core-seam-decision.md`](core-seam-decision.md).
The inventory and prices: [`concurrency.md`](concurrency.md). The batch client
and measurements: [`multithread-core.md`](multithread-core.md), [`batch.md`](../cli/batch.md).
Module boundaries: [`module-layout.md`](module-layout.md). Code anchors:
`src/tui/mod.rs` (`Session`), `src/tui/view.rs` (`FullTail` and the frame),
`src/tui/bootstrap.rs` (the `Option<&Host>` entry), `src/term.rs`
(`InlineViewport`).

A pull request that adds a global, puts product state only on `Session`, or
adds a `fullscreen()` branch should say why in its description: that sentence is
the early warning this page exists to keep visible.
