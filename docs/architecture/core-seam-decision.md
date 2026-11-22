# The core seam — a host refactor deferred, and the rules that keep it cheap

Date: 2026-11-22

Status: **accepted**. A decision page, not a plan: the refactor is deferred, and
the boundary rules below are review criteria until it is scheduled.

## Decision

Do not build a codex-style session core with several frontends now. Keep the
shape that exists:

```
main.rs ──owns──▶ InlineViewport
                      ▲
                      │ &mut
                 tui::Session ──▶ App (pure) · ui (pure)
```

The pure library stays the place where behaviour is decided and tested; the TUI
stays the only boundary; `Host` stays the process-wide facts that `-p --jobs`
already shares. Revisit the split only when one of the triggers below fires.
Until then, the rules in *The rules that keep it cheap* are what a review checks.

## What "core" would mean here

A core/host split moves the conversation out of the terminal:

- **one session core** that owns `App`, the turn loop, rollout, permissions and
  MCP, with no terminal in its type;
- **frontends over a transport** — the TUI, headless runs, and a future server
  or attach client;
- **N conversations per process**, with a session switcher or panes, and
  sessions that can be created and dropped without a new bootstrap each time.

That is *Option D* in [`concurrency.md`](concurrency.md), and the measurement
that says the thread shape works is
[`multithread-core.md`](multithread-core.md). It is **not** parallel tool calls
inside one turn — that is *Option A* on the same page, a separate decision.

The first client already exists: `-p --jobs` runs N independent sessions in one
process ([`batch.md`](../cli/batch.md)). The TUI is the client that does not.

## What already exists

- **`Host`** (`src/tui/host.rs`): the read-only process facts — config home,
  first-run seeds, skill and agent walks, settings, secrets, hooks, the provider
  table. `-p --jobs` resolves it once and hands `Option<&Host>` to every
  bootstrap; a single TUI or headless run resolves the same facts itself.
- **The batch shape**: one OS thread per session, each with its own
  current-thread runtime, on a bounded `--concurrency` (default 4). Measured at
  64 jobs and 64 concurrent: 38.5 MB, empty stderr.
- **`InlineViewport::headless()`**: a session can already run with no screen.
- **`ReplySource` / `LlmBackend`**: the backend seam is independent of the UI.
- **Per-session rollouts**: a conversation is a file under the sessions tree,
  and `/resume` loads one without a process-global table.
- **The pure surface is pinned**: `tests/api_surface.rs` and the `app`/`ui` unit
  tests make moving behaviour a test move, not a rewrite.

## What the refactor would have to change

In rough order of cost:

1. **`App` is one conversation.** `history`, the private `streaming`, `queued`
   and the composer `input` live directly on it. N conversations means either
   one `App` per conversation (and a container) or a `Conversation` struct that
   `App` owns. Everything that renders or decides reads one of these fields.
2. **`Session<'t>` borrows `&'t mut InlineViewport`.** The terminal's lifetime
   is the session's lifetime. The `Screen` seam designed in
   [`full-screen.md`](../ui/full-screen.md) was not implemented: `term` carries a
   `full` flag and branches, and the loop reads `fullscreen()` at each call
   site. A second frontend needs that seam to exist for real.
3. **Full-screen view state lives on `Session`.** The tail cache, scroll anchor,
   selection, hover, hit map, expanded set and pinned prompt are all per-pane
   state stored beside the one app. A pane model has to be reconstructible from
   `App` plus a size, the way the inline viewport already is.
4. **`Session` is `!Send`.** The documented cause is `syntect`'s `onig` raw
   pointer across an await. Thread-per-session sidesteps it; a daemon or a
   shared async runtime would have to change the highlighter first.
5. **Process-level assumptions are scattered.** `palette::ACTIVE` is a
   `thread_local`, `OVERLAY_ACTIVE` is a `static` atomic, and signals, MCP
   managers and the provider client are per session. The batch works because
   each session owns its thread; a multi-conversation TUI would share them.
6. **`bootstrap.rs` and `models.rs` still assume one session.** `Host` moved the
   read-only half; the mutable half is assembled per bootstrap.

None of this is wrong today. It is the price of a single-session type, paid once.

## Why defer it

- **No product feature is blocked.** Running several tasks means several
  processes, or `-p --jobs`; both work today.
- **The price is real.** `concurrency.md` prices *Option D* as very high: it
  touches `App`, `Session`, the event loop, and the cancellation and permission
  invariants that are load-bearing.
- **The seam is in the right place.** Pure `App`/`ui` against an I/O boundary is
  the hard part, and it is already there. The refactor is a move along that seam,
  not a rewrite of the model layer.
- **Debt does not compound if the rules hold.** The expensive failure mode is
  not "we did not refactor"; it is "we added a second source of truth while
  waiting".

## The rules that keep it cheap

These are the review criteria until the refactor is scheduled:

1. **Put state where it belongs.** Per-conversation product state goes on `App`
   (pure, unit-tested). Per-process facts go through `Host`. Per-screen caches
   belong to the view layer and must be rebuildable from `App` plus a size — a
   cache may never be the only copy of something the product needs.
2. **Add no new globals.** No new `static` or `thread_local` for session state.
   `palette::ACTIVE` and `OVERLAY_ACTIVE` are known debt; new work takes the
   value as an argument or through `Host`.
3. **Send messages across threads, never shared mutable state.** Follow the
   `StreamEvent`-plus-channel shape. No `Arc<Mutex<App>>`, and no thread reads
   or writes `Session` fields directly.
4. **Take `&App`, not the session.** New pure logic in `app` or `ui` takes the
   data it needs. A helper that reaches through `Session` cannot serve a second
   conversation.
5. **Do not half-build the seam.** Do not add a `Screen` enum for one caller or
   a second viewport. When it lands, both viewports implement it and the `full`
   branches move behind it in one change, with the smoke phases proving both.

## When to schedule it

Any one of these is enough to open the work:

- a product feature needs **two conversations live in one TUI** — a switcher or
  panes beyond the subagent views;
- a **host or daemon** is wanted: attach/detach, several clients, or a session
  that outlives the terminal;
- the batch needs the two remaining `Host` items — a shared **MCP connection
  pool** and one **process-level signal handler**;
- two or more bugs in one release trace back to "assumes one `App`" or "assumes
  one `Session`";
- a frontend other than the terminal is required for headless parity.

The absence of a trigger is the answer: keep deferring.

## The path when it happens

1. **The `Screen` seam.** `Session` stops naming `InlineViewport`; the inline,
   full-screen and headless paths implement one small interface
   (`commit`/`draw`/`draw_overlay`/`restore`). Done when the loop no longer
   branches on `fullscreen()`.
2. **A per-conversation view object.** The full-screen caches move off `Session`
   into one value per conversation, rebuilt from `App` plus a size. Done when a
   second view can be constructed without touching the event loop.
3. **Split `App`.** `history`, `streaming`, `queued` and `input` move into a
   `Conversation`; `App` owns one or many. Done when `-p --jobs` can reuse the
   same type and the TUI can hold N of them.
4. **The TUI client.** A session switcher and the transport behind it (#28).
   Done when a second conversation runs a turn while the first keeps painting —
   the thread shape the batch already measured.

Steps 1 and 2 pay for themselves on the current single-session TUI: fewer
`full` branches and a view that can be tested without a terminal.

## What this does not decide

- **Inline vs full-screen default** — [`full-screen.md`](../ui/full-screen.md)
  owns that.
- **Parallel tool calls inside one round** — *Option A* in
  [`concurrency.md`](concurrency.md), with its cancellation and ordering costs.
- **MCP pool sharing and a process signal handler** — Host steps that can land
  on their own, without the session split.
- **One process or attach-to-daemon** — a later product call, not a prerequisite.

## Where this is tracked

The inventory is [`concurrency.md`](concurrency.md); the measurements and the
batch client are [`multithread-core.md`](multithread-core.md) and
[`batch.md`](../cli/batch.md); the module boundaries are
[`module-layout.md`](module-layout.md). The code anchors are `src/tui/mod.rs`
(`Session`), `src/tui/host.rs` (`Host`), `src/tui/bootstrap.rs` (the
`Option<&Host>` entry), and `src/term.rs` (`InlineViewport`).

A pull request that adds a global or puts product state only on `Session` should
say why in its description; that sentence is the early warning this page exists
to keep visible.
