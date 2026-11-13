# CLAUDE.md

Guidance for Claude Code in this repository.

**[`AGENTS.md`](AGENTS.md) governs what you run and when** — read it before your
first edit. It maps a change to the smallest check that can fail, lists what
differs on macOS, and runs `scripts/preflight.sh --smoke`. This file is the
architecture map: how the code works, and the invariants that break silently
when you get them wrong.

## What this is

Zeroth is an inline terminal coding agent: one Rust binary (edition 2024) with a
custom inline viewport over crossterm, a tokio `select!` loop, and an agentic
tool loop over five wire formats (Chat Completions, Responses, Anthropic
Messages, Ollama native, Cloud Code). It runs interactively or headless (`-p`).
Design rationale: `docs/design.md`. Module map:
`docs/architecture/module-layout.md`. Every area has a page under `docs/`.

## Commands

```bash
cargo run                                   # the TUI
cargo test --lib <module>::                 # the loop after an edit
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                  # once, before calling it done
cargo doc --no-deps --lib
cargo build && scripts/smoke.sh <id>        # the terminal boundary
scripts/smoke.sh --list                     # phases, tags, durations
scripts/preflight.sh --smoke                # is this machine fit to run the gate
```

The per-change test matrix, what each command costs, and the macOS baseline are
in [`AGENTS.md`](AGENTS.md). `target/` is an incremental cache (a symlink into
mbx's store on this machine): never `rm -rf` it; `cargo clean -p zeroth` fixes a
stale link.

## Architecture at a glance

- **`src/lib.rs`** — the library and the testable core. Pure, unit-tested logic
  lives in `app`, `stream`, `ui`, `textarea`, `file_search`, `session`,
  `history`, `context`, plus the pure cores of `frame`/`paste`/`subprocess`.
- **`src/main.rs`** — a small shell: the detached-exec hook, CLI resolution, the
  viewport, the loop.
- **`src/tui/`** — the binary-private tree that drives the async loop
  (`event_loop`, `turn`, `stream`, `agent`, `background`, `permission`, `ask`,
  `view`, `commit`, `models`, …) with `Session` in `mod.rs`. Every handler is an
  `impl Session` block in its area module.
- **`src/app/`, `src/ui/`, `src/stream/`** are directories of per-area modules,
  re-exported by name — never a glob — so `tests/api_surface.rs` locks the public
  surface.
- **The backend seam** is `stream::ReplySource`; `llm::LlmBackend` implements it
  and runs the tool loop (`llm::agent::run_agent`) over
  `llm::exec::ToolExecutor`. The offline demo is `stream::dummy` (`DummyAi`), a
  registry of scenarios.
- **Headless** `-p` reuses the same `Session` over `InlineViewport::headless`
  (`docs/cli/headless.md`).
- **Boundary vs pure:** `src/tui/`, `term.rs`, the clipboard read, the session
  recorder and the input-history store are I/O; smoke covers them, not unit
  tests. Keep logic out of the boundary and in pure helpers it calls.

## The four invariants

Break one and a whole class of bugs returns. The tests and smoke phases named
below are the canaries.

1. **One stdin reader, created after the init cursor query.** `InlineViewport::init`
   performs the DSR read once, synchronously, before the crossterm
   `EventStream` exists. Any other question for the terminal rides in that same
   read (`term::query`), and keys typed before it are replayed
   (`take_typeahead`). Never add a second reader. The detached-exec hook
   (`subprocess::run_detached_exec_if_requested`) must stay the **first
   statement** of `main()`, before the runtime or any terminal I/O.
2. **Greedy word-wrap is prefix-stable.** `ui::wrap_text` changes only the last
   wrapped line when text is appended. `ui::StreamRender` leans on that to commit
   completed lines to scrollback as a reply grows, withholding the growing last
   line, trailing blanks, and a whole open code line. Do not change the wrap
   algorithm or the markdown/highlight renderers without the three locking tests
   (`incremental_commits_reconstruct_the_whole_reply`,
   `stream_render_is_prefix_stable_over_every_prefix`,
   `streamed_code_never_recolours_a_committed_row`).
3. **The viewport is content-anchored, and resize reflows both ways.** The box
   grows downward in place; `term::draw` scrolls only when it must, and
   `ui::repin` decides the shrink. Never pin it to `screen.height - height`. An
   inline *modal* (the permission prompt) may be screen-tall: it grows into real
   scrollback, and the first draw after it closes purge-rebuilds
   (`modal_scrolled`, `ui::modal_needs_rebuild`; smoke Phases 58/60/62/63). When
   a streaming reply ends, reseat the viewport to idle height *before* the final
   commits or the box rises. Every full rebuild (resize, `/clear`, rewind,
   `/resume`) goes through `term::reflow` — purge scrollback and screen, rewrite
   history, paint the region in one synchronized frame. An ordinary overlay
   return is **not** a rebuild: commits queue while the overlay is up, and the
   return flushes them.
   **This invariant describes the inline default.** The opt-in screen-owning
   mode (`--fullscreen` / `ZEROTH_TUI=full`, `docs/ui/full-screen.md`) paints a
   whole frame from history into the alternate screen; its `reflow` and
   `insert_before` take the full-screen branches, and its own scroll contract is
   the one to keep there. Do not mix the two.
4. **Tool calls interleave with text.** A `ToolStart` finalises the assistant
   text before it as its own history message. A parallel batch is announced up
   front (`ToolBatch` → `App::tool_queue`); the running call shows live while its
   siblings show `⎿ Waiting…`, and execution stays sequential. Ctrl+O is the full
   transcript, built incrementally by `ui::TranscriptCache`; while it is up the
   loop still commits — the commits only *queue* (`insert_before` does no I/O,
   and only the inline draw/reflow/restore flush). Quitting from the overlay must
   `exit_overlay` then `overlay_return_repaint` before breaking. An open agent
   session view is the one view that defers commits (`commits_allowed`).

## Data flow

`select!` runs over five sources: input, reply events, draw ticks, `@`-search
results, and finished Ctrl+V clipboard reads. Every state change schedules a
frame; the scheduler coalesces ordinary requests to 120 fps
(`MIN_FRAME_INTERVAL`), but a key that edits the composer gets its own 2 ms
urgent floor (`URGENT_FRAME_INTERVAL`, after the paste-burst check) so typing
does not queue behind an animation tick, and while a turn is active the draw
branch re-arms a 32 ms animation frame.

`Submit(text)` → `tui::turn::Session::start_turn`: record, commit, spawn one
`ReplySource`. **Enter during a turn steers** it: the backend drains the shared
`SteerQueue` at every round boundary, and `StreamEvent::Steered` lands the
message as a real user bubble. **Tab queues** a separate follow-up turn.
`!command` runs locally as its own shell turn (queued if a turn is running).

The backend interleaves `Chunk`, `ThinkingStart`/`ThinkingChunk`/`ThinkingEnd`,
`ToolCallDelta`, `ToolStart`/`ToolOutput`/`ToolEnd`, `Usage`, then `StreamDone`
or `Error`. Esc mid-turn cancels and either keeps the partial (resolving every
in-flight call as failed, with the red notice and receipt) or, when nothing had
streamed, undoes the submission. Every turn end dispatches **one** queued entry.
`/clear` mid-turn is a kill: cancel, reap, drain, wipe, repaint.

## Conventions

- **Styling is centralized** in `ui/theme.rs`: glyphs and geometry are consts,
  colours are accessors reading the active theme's `Palette` in `ui/palette.rs`.
  No `Color::Rgb` outside `palette.rs`; a new colour is a new palette role in
  every theme table (`docs/ui/theme.md`).
- **All width math goes through `cols()`**, never `chars().count()`. Every paint
  that hands cells to the backend goes through `term::visible_cells` — three
  paths, and missing one revives the wide-glyph bug in that view alone
  (`docs/ui/table-streaming.md`).
- **The input is a `textarea::TextArea`**, never a `String`. Route edits through
  it; size fields with `ui::layout::text_field_width` so the caret keeps a cell
  (`docs/input/textarea.md`).
- **Never build a `Value` tree of a body you read a few fields out of**, and
  never allocate anything picture-sized per turn. Bound network reads; decode a
  PNG at its fitted size (`images::fitted`); stream the request body
  (`llm::body::streamed_request`, `docs/architecture/memory.md`).
- **A picture is reserved in `ui` and drawn at the boundary**: line builders call
  `images::place` and emit marked blank rows; `ImageStore::stamp` runs in all
  four paint paths (`docs/ui/images.md`).
- **A framed inline view hugs its rules**, and on a short terminal it sheds rows
  before it flows (`ui::view_flow`, `docs/ui/view-flow.md`).
- **A lifecycle-hook event is one defaulted method on `llm::hooks::HookSink`
  plus one call site.** Anything a hook blocks on must poll the turn's
  `CancelToken` (`docs/tools/hooks.md`).
- **Tool schemas are short and concrete.** The path params say "the absolute
  path to …" and stop; every word rides every request.
- **The agent's own name is one constant**: `zeroth::APP_NAME`. Ported sentences
  keep their source tool's name unless you change it deliberately.
- **The README is the front door, not the manual.** Mechanism goes in `docs/`,
  changes in `CHANGELOG.md`, the privacy statement in `TELEMETRY.md`.
- **Working style: TDD.** No production code before a failing test you watched
  fail. `src/main.rs` + `src/tui/` are the exception — verify with the app and
  smoke. For non-trivial features: explore, ask, propose 2–3 designs, agree,
  write it in `docs/`, and update `docs/design.md` when behaviour changes.

## Where the detail lives

`docs/` is categorized. Start at `docs/design.md` and
`docs/architecture/module-layout.md`, then the page for the area:

- `docs/architecture/` — async loop, compact, context/usage counting, memory,
  token estimate, dummy backend, herdr, plus the measured input-latency and
  concurrency notes.
- `docs/input/` — textarea, history/search/persistence, queue, paste, image
  paste, file search, shell commands, backtrack, interrupt.
- `docs/ui/` — theme, layout, images, tool cells/streaming/groups, view flow,
  the pickers, overlays, footer/header/status.
- `docs/tools/` — tools, bash/interactive shell, background, hooks, MCP, ask,
  tasks, skills, scratchpad, tty detach.
- `docs/providers/` — every wire, sign-in, caching, offline, usage, reasoning,
  fast mode, local models.
- `docs/state/` — resume, rollouts, checkpoints, permissions, settings,
  per-directory state, trust/config review.
- `docs/ops/` — release, smoke, update, binary size, build time.
- `docs/agents/`, `docs/cli/` — subagents/agent tools, and CLI/headless/MCP.
