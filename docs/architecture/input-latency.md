# Input latency — the keystroke that waits for the frame floor

Date: 2026-11-12

Status (2026-11-14): **Causes 1, 2 and 3 are fixed.** The global frame floor
now has a 2 ms urgent path for composer-editing keypresses; the wrap cache is
shared (`Arc`) instead of cloned four to six times per frame; and `live_rows`
derives its eight row counts once. Still open: incremental re-wrap, a cap on
recalled drafts (Cause 4) and a frame-time log (item 5). The measurements
below are kept as the record of what was wrong and how it was found.

## The question

A competitor's own benchmark (`miyu`, 15 sessions + a 16-thread compile running
beside it, medians) puts one keystroke at **7.36 ms** here against **0.39 ms**
for the fastest TUI in the table. Nothing in this codebase is slow enough for
that: an idle composer renders a whole live region in **41 µs**. So where do
the other 7.3 ms go, and what makes the number get *worse* on a loaded machine?

Two separate answers, and they need separating because only one of them is a
scheduling decision while the other is an algorithmic one.

## What was measured

A throwaway `examples/` probe (deleted; the numbers are the record), release
build, 120-column frame, macOS, on the same eight-core machine the repo is
developed on. One "keystroke" = `TextArea::insert_char` + a full
`ui::render_live` into a `Buffer`.

| Draft in the composer | avg | best |
| --- | --- | --- |
| empty | 64 µs | 40 µs |
| 300 B sentence | 80 µs | 78 µs |
| 4 KB paragraph | 254 µs | 237 µs |
| 20 KB pasted code | 398 µs | 332 µs |
| 64 KB single line | **1.63 ms** | 1.38 ms |

And the pieces underneath (`wrap_rows` is the cache fill the composer does on
every edit, because `forget_geometry` drops the cache):

| Text | `wrapped_rows` | `display_rows` + `cursor_row_col` |
| --- | --- | --- |
| 84 B | 3.4 µs | 3.0 µs |
| 3.8 KB | 5.4 µs | 6.6 µs |
| 19 KB, 640 lines | 26 µs | 67 µs |
| 64 KB, one line | 193 µs | 214 µs |

Conclusion from the numbers: **an ordinary keystroke costs well under 0.1 ms of
CPU.** The 7.36 ms in the benchmark is not this code computing.

## Cause 1 — the frame floor was global, so a keystroke queued behind animation (fixed)

At the time of this measurement, `frame.rs::FrameRateLimiter::clamp_deadline`
never emitted a frame sooner than `MIN_FRAME_INTERVAL` (8.333 ms, 120 fps)
**after the last frame emitted**, whatever asked for it. `schedule_frame()` from a keypress and
`schedule_frame_in(32 ms)` from the status spinner travel the same channel, and
the limiter cannot tell them apart.

So on an idle app a keystroke draws at once (64 µs, measured), but **as soon as
any animation is running** — a streaming reply, the status shimmer
(`STATUS_FRAME_INTERVAL` 32 ms), the ↓ manager band, an open `/spinner` page —
the earliest a keypress can paint is `last frame + 8.33 ms`. A 7.36 ms median
sits inside exactly that window, and on a machine with 15 sessions streaming it
is the normal case, not the tail.

`docs/architecture/async-rewrite.md` already says this was chosen deliberately: the floor
was ported from openai/codex for architectural fidelity, and its *Why* note
observes that "for typing latency on this app the win is marginal (already
~1 ms)". That is true while idle and false while a turn streams.

### What miyu does instead

`miyu`'s TUI (`proto/tui-demo`, `src/pacing.rs`) has one function, and the
whole difference is in it:

```rust
pub fn deadline(drawn_at, beat, now, burst, urgent) -> Instant {
    let paced = drawn_at + beat;              // beat = 33 ms
    if urgent { paced.min(now + burst) }      // burst = 2 ms
    else { paced }
}
```

`pacing::urgent` is true for key presses/repeats and pastes, false for streamed
text, mouse moves and resizes. A keypress is therefore due **2 ms from now**,
never later than the next beat; the join-first loop also *pulls an already
computed deadline earlier* when a later message turns out to be a key. There is
no separate rate limiter to clamp against — the beat is a target, not a floor.

That is why miyu measures sub-millisecond: same termbox-class work, a different
scheduling policy. We did not lose 7 ms of rendering; we scheduled it away.

### What the fix cost (implemented 2026-11-14)

The implemented shape is the one proposed here. `FrameRequest` carries
`urgent`, `FrameRequester::schedule_frame_urgent()` sits beside the two existing
calls, and `FrameRateLimiter::clamp_request` picks `URGENT_FRAME_INTERVAL` (2 ms)
for it; `soonest` already keeps the earliest pending deadline, so the scheduler
itself only had to pass the request through. `Session::schedule_for_key`
consults `PasteBurst` **first**, then the pure `app::edits_composer` helper
(plain characters, Backspace, Delete, Enter) before choosing the urgent call.
`clamp_deadline` keeps the ordinary 120 fps contract and its locking test;
the new tests cover the urgent floor, idle, late requests and the pull-forward.

Risks, in the order they would bite — all three were satisfied:

- **A held key becomes a 500 Hz repaint.** Terminals send repeats at the OS key
  rate; an urgent floor of 2 ms caps it at ~500 fps. `PasteBurst`
  (`BURST_CHAR_INTERVAL` 8 ms, `BURST_MIN_CHARS` 3) already asks for relaxed
  frames once a run is detected, so a held key falls back to the beat after
  three characters — but the burst detector must be consulted *before*
  choosing urgent, or a paste floods the terminal.
- **The tests pin the current contract.** `frame.rs`'s unit tests
  (`clamp_pushes_a_too_soon_request_to_the_frame_floor`) assert the exact
  clamping rule; `docs/design.md` describes the 120 fps floor as the coarse
  coalescer. Both need the new rule stated, not a quiet change.
- **Slow terminals.** 120 fps exists partly so a fast text stream does not
  saturate a pty; an urgent path that also repaints the *streaming strip* would
  undo that. The urgent call must only be reachable from a keypress.

## Cause 2 — the composer's wrap was O(draft) and copied 4–5× per frame (fixed in part)

Independent of scheduling, a long draft makes each frame cost real time. The
`TextArea` drops its `wrap_cache` on every edit (`forget_geometry`), and one
frame then asks for it several times over:

- `ui::layout::live_height` → `input.row_count` (1 wrap)
- `render_live` → `input_box`: `display_rows` + `wrapped_rows` +
  `cursor_row_col` (3 more, each a cache rebuild or a `Vec<Range>` clone)
- the boundary's `ui::cursor_position` → `cursor_row_col` + `row_count` (2 more)

`wrapped_rows` also **clones the whole `Vec<Range<usize>>`** on every call,
which is pure overhead when the caller only reads.

64 KB of draft, one frame: 1.63 ms here. Three to four times that on a slower
machine is the 6 ms a loaded laptop would show — and it is the *only* path where
the content of the draft, rather than the scheduling, decides the latency.

Three fixes, cheapest first (1 and 2 done 2026-11-14):

1. **Borrow instead of clone.** Return `Rc<[Range<usize>]>` (or an `&[..]` with
   a lifetime tied to the `RefCell` borrow) from the cache; the render path
   never needs an owned copy. **Done:** `WrapCache.rows` is an
   `Arc<[Range<usize>]>` (`Arc` keeps `TextArea` `Send`), with
   `wrapped_rows_shared()` handing out an O(1) clone and `wrapped_rows()`
   keeping the owned `Vec` for the tests. `display_rows`, `row_count`,
   `cursor_row_col`, `cached_rows` and `InputBox.row_ranges` all read the shared
   handle, so a frame copies the ranges **zero** times. Locked by
   `textarea::tests::wrapped_rows_shared_reuses_one_cache_instead_of_copying_it`.
2. **Compute the wrap once per frame.** `render_live` already builds an
   `InputBox` carrying `rows`/`row_ranges`; `live_height` and `cursor_position`
   could read that instead of re-deriving. **Done in the cheaper form:** every
   caller now hits the same shared cache, so re-deriving costs one `Arc` clone
   rather than an O(draft) copy. Plumbing one `InputBox` into `live_height` and
   `cursor_position` would remove even that, but needs signature changes across
   the boundary for a few hundred nanoseconds.
3. **Incremental re-wrap — still open.** An edit touches one logical line; only
   that line's rows change (prefix-stable, like `StreamRender`'s frozen lines).
   Recompute from the edited line onward and splice, instead of the whole draft.
   This is the one remaining O(draft) cost (the wrap pass itself).

There is also no cap on the composer's text: `paste.rs`'s
`LARGE_PASTE_CHAR_THRESHOLD` (1000 chars) only applies to **bracketed pastes**.
A recalled or programmatically set draft has no limit — `ui::layout`'s own
`non_preview_rows` doc admits "a recalled multi-megabyte paste wraps to tens of
thousands of rows". miyu folds anything over 10 lines / 800 chars into a
`[已粘贴 14 行]` block *in the editor*, so its composer text is structurally
small. Folding on recall (`app/input_history.rs::recall_input`) would close the
same hole here.

## Cause 3 — the per-frame row sums were recomputed two or three times (fixed)

Not the keystroke path, but it compounds it under load. `live_rows`,
`non_preview_rows` and `live_height` each re-derive the same eight counts, and
`non_preview_rows`' own doc records the cost:

> Measured: 53 ns on an ordinary frame, and 13 µs on one whose palette is open
> over a four-message queue (`queued_lines` renders each message).

13 µs twice per frame is 4% of a 700 µs frame budget, and `queued_lines` is
already memoized — the remaining cost is the *sum*, called from
`preview_budget` and again from `live_height`. One `LiveRows` built once per
frame and handed down (as `ui::strip_paint_rows` already does) removes it.

**Fixed (2026-11-14):** `ui::layout::RowCounts` derives the seven counts once,
and both `live_rows` and `fitted_preview_rows` fit the preview from that one
set; `strip_other_rows` and `RowCounts::non_preview` share a single spelling of
the strip's sum, so nothing can drift. The second derivation inside `live_rows`
is gone, pinned by
`ui::tests::layout::live_rows_shares_one_set_of_counts_with_the_public_fit`.
Passing one `LiveRows` across `strip_paint_rows` / `view_flow` / the boundary
would also remove the remaining cross-function derivation, but that is a
signature change with no measured payoff yet.

## What is not the problem

- **The render path itself.** 41 µs for a whole live region, 64 µs for
  keystroke + frame. Cell diffing (`term::paint_frame`'s `prev.diff`) and the
  synchronized update are already doing their job.
- **The memos.** `QueuedMemo`, `StreamRender::preview_memo`, and
  `TranscriptCache`'s frozen prefix each already cut a measured cost by an order
  of magnitude (75→13 CPU ticks per five idle seconds; Ctrl+O 273 ms → 14 ms).
- **I/O.** No `Command::new`, `fs::read` or `metadata` appears anywhere in
  `src/app/` or `src/ui/` outside tests; nothing per-frame touches the disk.

## What to do, in order

1. **Urgent frames — done (2026-11-14).** The only change that moves the number
   the benchmark measures, because it is a scheduling constant and not a
   rendering cost. `docs/design.md`'s frame-scheduler paragraph and
   `docs/architecture/async-rewrite.md`'s *Why* note now say the floor is no longer
   uniform, and `frame.rs`'s locking tests cover both floors.
2. **Borrow the wrap cache — done (2026-11-14).** `Arc<[Range<usize>]>` +
   `wrapped_rows_shared`; every render-path reader uses it, locked by two new
   `textarea::tests`.
3. **One set of row counts per frame — done (2026-11-14).** `RowCounts` feeds
   both `live_rows` and the public fit; `ui::tests::layout` pins the
   equivalence.
4. **Fold long recalls** (Cause 2, the cap). A product decision as much as an
   engineering one: does recalling a 40 KB message show it whole, or a block?
5. **A frame-time log.** miyu has `MIYU_TUI_FRAME_LOG` writing
   `elapsed prep total` per frame plus a per-section breakdown over
   `slow_frame_ms`; it is how its own checkups found what to fix. We have no
   equivalent (there is no `ZEROTH_*` frame knob), which is why this
   document's numbers came from a probe that had to be written and then thrown
   away.

## Reproducing

The probe is gone; rebuild it if the numbers need re-checking. It was
`examples/ink_probe.rs`, ~90 lines: build an `App`, set `input` from a string,
time `insert_char` + `ui::render_live` into a `Buffer` for each draft size. The
`wrap_rows` table came from timing `TextArea::wrapped_rows` / `display_rows` /
`cursor_row_col` directly.

`cargo run --release --example ink_probe` — the release profile matters: a debug
build is 10–30× slower here and measures nothing real. The tables above predate
the urgent-frame fix; a post-fix measurement needs the same probe driven during
a streaming turn (or the frame-time log in item 5), since the 8.33 ms wait was
scheduling, not rendering cost.
