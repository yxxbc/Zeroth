//! The terminal shell around the [`zeroth`] library: the async event loop
//! and every piece of real I/O it drives.
//!
//! `main.rs` opens the viewport and calls [`event_loop::run`]; everything else
//! about running the app lives in here, split one module per area the way
//! `app/` and `ui/` are (see `docs/architecture/module-layout.md`). The library stays pure —
//! `App` decides, `ui` renders, and this tree is the only place that reads a
//! clock, touches the filesystem, spawns a thread, or writes to a terminal.
//!
//! # The shape
//!
//! [`Session`] holds the loop's state and every handler is a method on it. That
//! is deliberate, and it is the same trick `app/mod.rs` plays with `App`: a
//! struct's private fields are visible to the module that defines it **and all
//! its descendants**, so each area module below can `impl Session` and reach
//! the state it needs directly. Before this, the same state was ~50 locals in
//! one 2,000-line function, threaded into helpers nine arguments at a time.
//!
//! Both ends of every channel live on it too — the senders *and* the receivers
//! the `select!` polls. That works because `select!` scopes its futures: the
//! nine it builds borrow nine distinct fields, and they are dropped before the
//! winning branch's body runs, so that body can take `&mut self`. Keeping the
//! receivers here is what lets `Session::abandon_inflight` swap the reply
//! channel in place instead of the loop threading a `&mut` receiver down
//! through three call layers.
//!
//! # Where things live
//!
//! | Module | Holds |
//! |--------|-------|
//! | `mod.rs` | The [`Session`] struct and [`StatusClocks`] — the shared state. |
//! | [`event_loop`] | [`event_loop::run`]: the `select!` over the eleven sources, the loop-bottom work, the teardown. |
//! | [`actions`] | The [`zeroth::app::Action`] dispatch — one method per key-press outcome. |
//! | [`turn`] | Starting and ending turns: user, queued, `!` shell, background follow-up, `/compact`. |
//! | [`stream`] | Folding one streamed reply event into `App` + scrollback. |
//! | [`agent`] | Subagent events, the roster's clocks, the linger sweep (`docs/agents/agent-tool.md`). |
//! | [`background`] | Background-shell events and settling their notices (`docs/tools/background.md`). |
//! | [`permission`] | The permission gate's session rules and the prompt's answers (`docs/state/permissions.md`). |
//! | [`view`] | Drawing: the inline region, the overlays, the repaints, the status clocks. |
//! | [`commit`] | Writing finished lines to scrollback — the one place invariant 4 is enforced. |
//! | [`models`] | The reply backend and everything that selects it (`docs/providers/llm.md`). |
//! | [`config`] | Reading the environment: providers, keys, settings, permission rules. |
//! | [`bootstrap`] | Assembling a [`Session`] before the first frame. |
//! | [`startup`] | The `--continue`/`--resume` argument resolution (`docs/cli/cli.md`). |
//! | [`headless`] | `zeroth -p`: the same session with no screen — its loop, its streams, its exit status (`docs/cli/headless.md`). |
//! | [`recorder`] | Mirroring history to a rollout file (`docs/state/resume.md`). |
//! | [`resume`] | Finding recorded sessions on disk. |
//! | [`settings`] | Applying a `/settings` knob the menu cycled (`docs/state/settings.md`). |
//! | [`update`] | The once-a-day update check: the request, the card under the banner, the recorded day (`docs/ops/update.md`). |
//! | [`update_cli`] | The `zeroth update` subcommand: the check, then the one-line installer over this binary (`docs/ops/update.md`). |
//! | [`herdr`] | Telling the herdr pane this session runs in what it is doing: the loop-bottom sync into the library's tracker and worker, and the failed-turn hold (`docs/architecture/herdr.md`). |
//! | [`history_store`] | The cross-session input history (`docs/input/history-persistence.md`). |
//! | [`shell`] | The `!` command runner (`docs/input/shell-command.md`). |
//! | [`usage`] | The `/usage` page's reads, one worker per subscription, its clock and its minute refresh (`docs/providers/usage.md`). |
//! | [`workers`] | The off-thread file-search / clipboard / model-list / usage jobs. |
//! | [`host`] | Clocks, dates, the OS string, ids — the raw impurities. |

use std::path::PathBuf;
use std::time::Instant;

use ratatui::crossterm::event::EventStream;

use zeroth::clipboard::ClipboardLease;
use zeroth::frame::FrameRequester;
use zeroth::paste::PasteBurst;

use self::history_store::InputHistoryStore;
use self::view::ConversationView;

pub(crate) mod actions;
pub(crate) mod agent;
pub(crate) mod ask;
pub(crate) mod background;
pub(crate) mod batch;
pub(crate) mod bootstrap;
pub(crate) mod commit;
pub(crate) mod config;
pub(crate) mod context_usage;
pub(crate) mod conversation;
pub(crate) mod diff;
pub(crate) mod donate;
pub(crate) mod event_loop;
pub(crate) mod export;
pub(crate) mod git_diff_loader;
pub(crate) mod headless;
pub(crate) mod herdr;
pub(crate) mod history_store;
pub(crate) mod host;
pub(crate) mod login;
pub(crate) mod mascot;
pub(crate) mod models;
pub(crate) mod permission;
pub(crate) mod recorder;
pub(crate) mod resume;
pub(crate) mod screen;
pub(crate) mod secrets;
pub(crate) mod settings;
pub(crate) mod shell;
pub(crate) mod spinner;
pub(crate) mod startup;
pub(crate) mod stream;
pub(crate) mod theme;
pub(crate) mod timing;
pub(crate) mod trust;
pub(crate) mod turn;
pub(crate) mod update;
pub(crate) mod update_cli;
pub(crate) mod usage;
pub(crate) mod view;
pub(crate) mod workers;

/// Everything the running app owns outside the pure library: the terminal, the
/// conversation state, the renderers, the boundary's clocks, the registries and
/// stores, and the in-flight turn.
///
/// The fields are private and every area module reaches them directly through
/// its own `impl Session` block (see the module doc). The borrow of the viewport
/// is what makes this a `'t` — `main.rs` keeps ownership so it can restore the
/// terminal even if the loop returns an error.
pub(crate) struct Session<'t> {
    /// The one screen: the viewport plus which contract it is painting
    /// through (`docs/architecture/core-host-split.md` Phase 2).
    screen: screen::Screen<'t>,
    /// The screen's own state — renderers, the frame cache, the scroll anchor,
    /// the hit map, the pointer and the expanded set, all derived from `App`
    /// plus a size (`docs/architecture/core-host-split.md` Phase 1).
    view: ConversationView,
    /// The conversation's runtime: its pure model, the channels its turn and
    /// workers report on, its registries, stores and clocks
    /// (`docs/architecture/core-host-split.md` Phase 3).
    conversation: conversation::Conversation,

    // ----- the view's channels: paste bursts -----
    /// Detects a paste / fast-type burst so its redraws coalesce.
    burst: PasteBurst,

    // ----- the boundary's clocks and deadlines (the set_status_times pattern) -----

    // ----- the loop's own channels: what wakes it, and what it sends on -----
    //
    // Both ends live here. `select!` creates all nine futures up front, each
    // borrowing a *distinct field* of this struct, and scopes them so the branch
    // body that wins can still take `&mut self` — which is what lets a handler
    // both read a receiver and mutate everything else. `recv`/`try_recv` hand
    // back owned values, so no borrow outlives the call itself.
    /// Terminal input — the **sole** stdin reader (invariant 1). Created after
    /// the viewport's synchronous cursor query, in [`Session::bootstrap`].
    /// `None` in a headless run, which has no terminal to read — and must not
    /// open one: crossterm opens the controlling terminal as the stream is
    /// built (`docs/cli/headless.md`).
    events: Option<EventStream>,
    /// A headless run's `--verbose`: its info toasts reach stderr too, beside
    /// the errors every headless run reports (`docs/cli/headless.md` *Output*).
    headless_verbose: bool,
    /// A headless run's reply budget (`--max-turns`), shared with every
    /// backend build — what the run reports as its `num_turns`. `None` in
    /// the TUI.
    round_budget: Option<zeroth::llm::agent::RoundBudget>,
    /// Asks the frame scheduler for a redraw, and its coalesced, rate-limited
    /// draw ticks coming back.
    frame: FrameRequester,
    draw_rx: tokio::sync::mpsc::UnboundedReceiver<()>,
    /// The update check worker's report (`docs/ops/update.md`): the newest
    /// release it found, which the **loop** then records in `update.json`
    /// and announces.
    update_tx: tokio::sync::mpsc::UnboundedSender<String>,
    update_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    /// The UTC day this **session** last spawned a check for, so the
    /// turn-start rollover costs a string compare on the common path.
    update_attempted: Option<String>,
    /// A newer release learned of while a turn was streaming: its card must
    /// not land in the middle of a reply, so it waits here for the loop
    /// bottom to find the session idle (`Session::flush_pending_update_notice`).
    update_notice_pending: Option<String>,
    /// The line to the herdr pane this session runs in (`docs/architecture/herdr.md`):
    /// what it is telling herdr and the worker that writes to herdr's
    /// socket. `None` outside herdr — and until bootstrap starts it after
    /// the first frame, so a slow socket can never delay that frame.
    herdr: Option<herdr::HerdrReporter>,

    // ----- registries, the gate, the stores -----
    /// The user's secrets (`docs/state/secrets.md`) — the one store the `/secrets`
    /// page edits, the reminder lists and every tool call expands and
    /// redacts with. The same handle the shell registry carries to the
    /// executor, the `!` runner and every backend build.
    secrets: zeroth::secrets::SecretRegistry,
    /// What the startup review decides over (`docs/state/config-review.md`):
    /// `trust.json`, the user's hooks file and the project's `.zeroth`
    /// layer, read once at bootstrap — every review answer and `/hooks`
    /// toggle records through it, and the hooks the session runs are
    /// its trusted share.
    trust: trust::TrustState,
    /// The cross-session input history (`docs/input/history-persistence.md`).
    hist_store: InputHistoryStore,
    /// Where the `/settings` knobs persist (`settings.json`, this directory's
    /// entry — `docs/state/per-directory-state.md`); `None` disables persistence,
    /// like `config.json`'s path (`docs/state/settings.md`). The live values
    /// themselves live on `App` — one copy, read where they are used; a save
    /// is a read-modify-write over the file itself, moving across only the
    /// key the user cycled, so an `ZEROTH_*` override merged in at startup
    /// never becomes the saved default.
    settings_path: Option<PathBuf>,
    /// The spinner tip walk's position as `tips.json` last recorded it
    /// (`docs/ui/tips.md`): the loop bottom writes the file back only when
    /// `App::tip_cursor` has moved off it, so an ordinary iteration costs an
    /// integer compare and no I/O.
    tips_saved: Option<usize>,

    // ----- the in-flight turn -----

    // ----- odds and ends the boundary has to remember -----
    /// Keeps the last `/copy`'s native clipboard selection alive (Linux/arboard
    /// serves it from a thread tied to the `Clipboard`'s lifetime). Held only
    /// for its `Drop` — never read (`docs/ui/copy.md`).
    clipboard_lease: Option<ClipboardLease>,
    /// Whether the terminal was resized while an overlay covered the inline
    /// view, which upgrades that return's repaint to a purge rebuild
    /// (invariant 3).
    overlay_resized: bool,
    /// The signature of the framed-view rows currently **flowed** into
    /// scrollback — a screen-tall `/hooks` page's top,
    /// committed above the live region so the terminal's own scrolling reads
    /// the whole page (`ui::view_flow`, `docs/ui/view-flow.md`). `None` when
    /// nothing is flowed. The draw tick compares it against the current
    /// state's signature and answers any mismatch — a navigation, a resize,
    /// the close — with the standard purge rebuild, which re-establishes (or
    /// clears) the flow; while it is `Some`, scrollback commits pause
    /// ([`Session::commits_allowed`]) so nothing tears the flowed page.
    flowed_view: Option<u64>,
}

/// The live status indicator's clocks, bundled (the impurity kept at the
/// boundary): the pure `App` only ever sees the *computed* durations, via
/// `set_status_times`. See `docs/ui/status-indicator.md`.
pub(crate) struct StatusClocks {
    /// When the turn was submitted — the elapsed timer and the verb's shimmer
    /// phase. `None` when no turn is in flight.
    turn_start: Option<Instant>,
    /// When the current thinking phase began (`None` outside one).
    thinking_start: Option<Instant>,
    /// When the **current running command** (a model `bash` call, set at its
    /// `ToolStart`; or a `!` shell run, set in `Session::run_shell`) began —
    /// cleared when it resolves (`ToolEnd`/`ToolBackgrounded`) and at each turn
    /// start. Drives the delayed `(ctrl+b to run in background)` hint via
    /// `App::set_command_elapsed` (`docs/tools/background.md`).
    command_start: Option<Instant>,
    /// When the event loop started — the epoch of the **animation phase** every
    /// blinking bullet blinks against (`App::set_pulse`, `docs/ui/tool-pulse.md`).
    /// Unlike the others this is never cleared: one monotonic clock, so a
    /// round's tool cells and its agent tree stay in step, and a background
    /// agent's live cell keeps animating between turns.
    loop_start: Instant,
}

impl StatusClocks {
    /// Idle clocks whose animation epoch is **now** — built once, at the top of
    /// the loop. No `Default`: `loop_start` is a real reading, and stamping it
    /// implicitly would let a stray `default()` silently restart every pulse.
    fn started_now() -> Self {
        Self {
            turn_start: None,
            thinking_start: None,
            command_start: None,
            loop_start: Instant::now(),
        }
    }

    /// Start a turn's clocks: the elapsed timer runs, no thinking phase is
    /// open, and no command is running yet (a model `bash` call starts its own
    /// hint clock at its `ToolStart`).
    fn start_turn(&mut self) {
        self.turn_start = Some(Instant::now());
        self.thinking_start = None;
        self.command_start = None;
    }

    /// Stop tracking a finished (or abandoned) turn. `command_start` is left
    /// alone: each resolution path clears it explicitly.
    fn end_turn(&mut self) {
        self.turn_start = None;
        self.thinking_start = None;
    }
}
