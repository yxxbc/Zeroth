# Async runtime rewrite (codex-style)

The event loop is built on **tokio** with a `select!` over five sources (input,
reply, draw ticks, `@` file-search results — `docs/input/file-search.md` — and
finished Ctrl+V clipboard reads — `docs/input/image-paste.md`), mirroring how
openai/codex drives its TUI. This replaces the previous synchronous
`event::poll` + drain loop. The *render* path (`term.rs`: diff + synchronized
update) is unchanged — only how input/stream/redraw are scheduled changed.

## Why

Codex achieves a very responsive TUI with: cell-diffing, synchronized frames,
a 120 fps frame scheduler that coalesces redraw requests, a paste-burst
detector, and round-robin fairness between input and draw. The first two were
already in this codebase; this change ports the rest.

> Note: for *typing latency* on this app the win is marginal (already ~1 ms,
> dominated by terminal I/O). This rewrite is for architectural fidelity to
> codex, chosen deliberately.
>
> **Measured later, and only half true** (`docs/architecture/input-latency.md`,
> 2026-11-12): the ~1 ms figure holds while the app is idle. `MIN_FRAME_INTERVAL`
> is a floor for *every* requester, so as soon as anything animates — a
> streaming reply, the status shimmer, an open band — a keypress's earliest
> paint is `last frame + 8.33 ms`, and a competitor's benchmark measured
> 7.36 ms per keystroke under load against its own 0.39 ms. The floor coalesces
> correctly; it just does not know a keypress from an animation tick.
>
> **Fixed 2026-11-14:** the request now carries urgency. A keypress that edits
> the composer is clamped against `URGENT_FRAME_INTERVAL` (2 ms) and `soonest`
> pulls the pending deadline forward; the paste-burst check still runs first.
> Ordinary requests keep the 120 fps floor. The rest of that document's
> measurements (the composer's O(draft) wrap, the per-frame row sums) are still
> open work.

## Pieces

- **`frame.rs`** — `FrameRateLimiter` (`MIN_FRAME_INTERVAL = 8.333 ms` = 120 fps
  for ordinary requests, `URGENT_FRAME_INTERVAL = 2 ms` for composer-editing
  keypresses) with a pure `clamp_request` (never emit sooner than the request's
  floor past the last frame; `clamp_deadline` keeps the ordinary contract) and
  `soonest` (fold a new request into the pending deadline, keeping the earliest —
  an urgent keypress therefore pulls an animation frame forward). A thin async
  `run_scheduler` task receives `FrameRequest`s on an mpsc channel, coalesces +
  rate-limits them, sleeps until due, then sends one draw tick.
  `FrameRequester` is the cloneable send handle (`schedule_frame`,
  `schedule_frame_urgent`, `schedule_frame_in`). Pure parts are unit-tested; the
  async task is covered by `smoke.sh`.

- **`paste.rs`** — `PasteBurst`, a pure state machine: characters arriving within
  `BURST_CHAR_INTERVAL` (8 ms) of each other are a *burst* once `BURST_MIN_CHARS`
  (3) accumulate. The loop uses this to coalesce a paste/fast-type run into a
  single scheduled frame instead of one per char. Unit-tested with injected
  `Instant`s.

- **`stream/`** — `StreamEvent` now travels a `tokio::sync::mpsc::Unbounded`
  channel. The backend still runs on a plain `std::thread` that *only sends*
  (tokio's unbounded `send` is sync, callable off-runtime), preserving "the
  backend never reads stdin".

- **`main.rs` + `src/tui/`** — `#[tokio::main(flavor = "current_thread")]`. After
  the (sync) viewport init queries the cursor, an `EventStream` becomes the sole
  stdin reader. The loop lives in `tui::event_loop::run`, one handler call per
  branch on the `Session` that holds the loop's state (`docs/architecture/module-layout.md`):

  ```text
  tokio::select! {
    Some(Ok(ev)) = events.next()      => on_key / resize → schedule_frame
    Some(se)     = reply_rx.recv()    => stream event   → (commit) + schedule_frame
    Some(())     = draw_rx.recv()     => render current view
    Some(res)    = file_rx.recv()     => @ file-search results → set_file_matches
  }
  ```

  (The fourth branch arrived with the `@` file picker — its background worker
  answers ranked matches on a tokio channel the loop can `select!` on; see
  `docs/input/file-search.md`.)

  `schedule_frame()` after every state change requests a redraw; a
  composer-editing keypress instead uses `schedule_frame_urgent()` (after the
  paste-burst check), whose 2 ms floor keeps it ahead of the animation chain.
  The scheduler emits a coalesced tick that drives the actual paint. `insert_before` queues its
  lines and `set_view_height` adjusts the tracked geometry inline; the tick then
  writes the queued lines *and* repaints the live region in one synchronized
  frame (the flicker fix — `docs/ui/flicker.md`).

## Invariant 1, restated

The previous "only the main thread reads stdin" becomes: **the `EventStream` is
the sole stdin reader, created *after* the init cursor-position query; the reply
backend only ever sends on its channel.** The reason is unchanged — a second
stdin reader would steal the cursor-position (DSR) reply. `init()` queries the
cursor once, synchronously, before the `EventStream` exists; `insert_before`
tracks the viewport row itself and never queries.

## Fairness

`tokio::select!` polls its branches in a randomized order each iteration, so
neither input nor draw ticks can starve the other — the effect codex gets from
its explicit round-robin `poll_draw_first` toggle.
