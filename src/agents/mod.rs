//! Subagents — the model's `Agent` tool (`docs/agents/agent-tool.md`).
//!
//! The **pure state** ([`AgentRun`], [`AgentStatus`]) folds a subagent's own
//! [`StreamEvent`] stream into a roster entry: its transcript
//! (`Vec<HistoryItem>` — what the agent session view renders), its live tool
//! queue, its token tally and tool-use count, and its lifecycle status. The
//! **boundary** [`AgentRegistry`] (like [`crate::background::BackgroundRegistry`])
//! is the cloneable handle the event loop and the backend share: it allocates
//! ids, keeps each running subagent's [`crate::stream::CancelToken`] + completion slot +
//! pending chat inputs, and carries the dedicated [`AgentEvent`] channel the
//! subagent threads report on (agents outlive turns, so their events must
//! survive the reply channel's interrupt swaps).

use std::collections::VecDeque;
use std::time::Duration;

use crate::app::{HistoryItem, Message, Role, StatusVerb, ToolCall, ToolStatus, TurnSummary};
use crate::stream::StreamEvent;

/// The default subagent type when the model omits `subagent_type`.
pub const GENERAL_PURPOSE: &str = "general-purpose";

/// How long a naturally finished agent lingers in the footer roster with its
/// green `◯` before the boundary sweeps it (`main.rs`, the toast-deadline
/// pattern). The same 30s window as a stop, for the same reason: the row is
/// the roster's only evidence the agent ran, and swept in a few seconds it
/// could vanish before the user had read it — mid-group, even before its
/// group cell committed. `x` clears it sooner (`docs/agents/agent-tool.md`).
pub const AGENT_LINGER: Duration = Duration::from_secs(30);

/// How long an agent the user **stopped** (`x` on its roster row) lingers
/// with its red `◯` before the same sweep. Its own constant, not a reuse of
/// [`AGENT_LINGER`]: a stop's countdown must never be restarted or shortened
/// by the group resolving after it (`or_insert` at the arming sites keys off
/// this intent), and a row that vanished on the keypress would leave no
/// evidence of what was stopped — so the stopped agent stays put, and the
/// row's `x` becomes the *clear* that removes it at once
/// (`docs/agents/agent-tool.md`).
pub const AGENT_STOPPED_LINGER: Duration = Duration::from_secs(30);

/// How many **settled** agents stay resumable — their conversations in the
/// registry, their swept roster entries in `App` — so the lead can still send
/// a finished agent a follow-up (`docs/agents/agent-tools.md` *Retention*). The
/// oldest settled one goes first; a running agent never counts.
pub const AGENT_RETAINED_MAX: usize = 16;

/// The most tool calls an `agentoutput` report lists — the newest; the
/// earlier ones are counted on one line (`docs/agents/agent-tools.md`).
pub const AGENT_REPORT_CALLS_MAX: usize = 30;

/// The most characters a reported tool call keeps, on its one line.
pub const AGENT_REPORT_CALL_CHARS: usize = 160;

/// The most characters a progress update keeps — the agent's own account of
/// what it found and what it is doing, which the lead reads whole up to here
/// (`docs/agents/agent-progress.md`).
pub const AGENT_REPORT_UPDATE_CHARS: usize = 2_000;

/// The label the agent's **own words** wear — its activity row's
/// `Agent: {update}` while it talks, beside a call's `Bash: …`
/// (`docs/agents/agent-progress.md`).
pub const AGENT_UPDATE_LABEL: &str = "Agent";

/// The line an `agentoutput` report marks a delivered message with — where,
/// among the agent's calls, a message reached it: the lead's `agentsend` or a
/// background shell's completion routed to it. One the user typed into its
/// session is quoted instead ([`FollowUp::from_user`], `docs/agents/agent-tools.md`).
pub const AGENT_FOLLOW_UP_MARK: &str = "— message received —";

/// Where a delivered message came from, as the lead must know it: a message
/// the **user** typed into the agent's session view is one the lead never
/// wrote, so it is quoted rather than marked ([`FollowUp::from_user`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    /// How many of the agent's calls it had made when the message arrived.
    pub at: usize,
    /// The text, when the user sent it; `None` for the lead's own
    /// `agentsend` and a background shell's routed completion.
    pub from_user: Option<String>,
}

impl FollowUp {
    /// A message the lead (or a routed note) delivered after `at` calls.
    #[must_use]
    pub const fn lead(at: usize) -> Self {
        Self {
            at,
            from_user: None,
        }
    }
}

/// Something an agent said on its way — a run of its reply that more work
/// followed, so not its answer — placed where it said it: an `Agent: …`
/// entry of the `agentoutput` report (`docs/agents/agent-progress.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressUpdate {
    /// How many of the agent's calls it had made when it said this.
    pub at: usize,
    /// How many messages it had read by then — what places it among the
    /// marks at the same `at` ([`AgentSnapshot::follow_ups`]).
    pub read: usize,
    /// What it said, trimmed and cut at [`AGENT_REPORT_UPDATE_CHARS`].
    pub text: String,
}

/// The note telling the lead which messages the **user** sent an agent
/// directly, in its session view (`docs/agents/agent-tools.md`): the completion
/// notice and a foreground result both carry it, because a model that reads
/// an answer to a question it never asked cannot otherwise know who asked.
/// One `- ` bullet per message, a multi-line one indented beneath its own.
/// `None` when the user sent nothing.
#[must_use]
pub fn user_messages_note(messages: &[String]) -> Option<String> {
    if messages.is_empty() {
        return None;
    }
    let mut lines = vec![
        "The user messaged this agent directly in its session view — these came from the \
         user, not from you:"
            .to_string(),
    ];
    for message in messages {
        let mut rows = message.trim_end_matches('\n').lines();
        lines.push(format!("- {}", rows.next().unwrap_or_default()));
        lines.extend(rows.map(|row| format!("  {row}")));
    }
    Some(lines.join("\n"))
}

/// A subagent's lifecycle — the tree row / footer `◯` colour and the
/// `⎿ Done` / `⎿ Interrupted` footers key off it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatus {
    /// Announced but no event has arrived yet — `⎿ Initializing…`.
    Pending,
    /// Its loop is streaming rounds / running tools.
    Running,
    /// Finished with a final response (green).
    Done,
    /// Its backend errored (red).
    Failed,
    /// Stopped by the user (`x`, Esc on the foreground group, `/clear`) (red).
    Interrupted,
}

impl AgentStatus {
    /// Has the agent reached a terminal state?
    #[must_use]
    pub const fn is_final(self) -> bool {
        matches!(self, Self::Done | Self::Failed | Self::Interrupted)
    }

    /// Did it finish successfully? Picks the green vs red colouring.
    #[must_use]
    pub const fn ok(self) -> bool {
        matches!(self, Self::Done)
    }

    /// The `⎿ {label}` status word shown for a settled agent.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pending => "Initializing…",
            Self::Running => "Running…",
            Self::Done => "Done",
            Self::Failed => "Failed",
            Self::Interrupted => "Interrupted",
        }
    }
}

/// One subagent as the roster sees it: identity, live counters, and its own
/// growing transcript. Fed by [`AgentRun::apply`] from the agent channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRun {
    /// The registry id (`a7k2m9x4q`, …).
    pub id: String,
    /// The model's short (3-5 word) task label.
    pub description: String,
    /// The `subagent_type` argument (default [`GENERAL_PURPOSE`]).
    pub agent_type: String,
    /// The full task prompt — the first user message of its transcript.
    pub prompt: String,
    /// Whether it was launched `run_in_background` (the schema default).
    pub background: bool,
    /// The `agent` call this run answers, as the round announced it: the
    /// provider's id and the model's verbatim arguments, carried onto the
    /// group's record so the next request replays the launch as the
    /// provider saw it (`docs/providers/prompt-caching.md`). `None` for a scripted
    /// launch.
    pub call_id: Option<String>,
    pub arguments: Option<String>,
    /// The call's index in its round (`ToolCall::position`'s twin), looked
    /// up by the loop from the round it announced.
    pub position: Option<usize>,
    pub status: AgentStatus,
    /// How many tool calls it has started.
    pub tool_uses: usize,
    /// Its live token tally: an app-side estimate that snaps to the
    /// provider's usage frames as they arrive (the [`crate::app::App`]
    /// pattern).
    pub tokens: u64,
    /// Billed tokens summed over its usage frames (what `tokens` snaps to).
    usage_tokens: u64,
    /// Billed tokens summed over the **current turn's** usage frames — the
    /// `Done for Ns · {n} tokens` receipt the settle records
    /// ([`TurnSummary::tokens`]); reset by [`reopen`](Self::reopen) so each
    /// chat continuation gets its own receipt while `usage_tokens` stays
    /// cumulative for the roster.
    turn_usage_tokens: u64,
    /// The cache-served share of `turn_usage_tokens` — the `({c} cached)`
    /// suffix ([`TurnSummary::cached`]).
    turn_usage_cached: u64,
    /// The cache-write share of `turn_usage_tokens` — the `({c} written)`
    /// half ([`TurnSummary::cache_write`]).
    turn_usage_cache_write: u64,
    /// Its **context size** in tokens — the footer gauge's numerator while
    /// its session view is on screen (`docs/agents/agent-context-gauge.md`), by the
    /// main session's exact rule ([`crate::app::App::context_used`],
    /// `docs/architecture/token-estimate.md`): the last usage frame's `input` — the
    /// provider's own count of the re-sent context — which each frame
    /// **replaces** (the next round's `input` already carries the last one's
    /// whole context, so summing frames is the billed tally,
    /// [`tokens`](Self::tokens), not the context), plus the calibrated
    /// estimate of what joined after it, read again at every settle. A turn
    /// that settles having seen no frame (the offline dummy, a provider that
    /// omits usage) stands on the estimate over its own transcript
    /// (`estimate_context`). Zero until the first of either lands, where the
    /// main gauge starts too.
    context_used: u64,
    /// What its usage reports say about its context — the base
    /// [`context_used`](Self::context_used) reads from and the calibration
    /// they taught, the main session's [`crate::tokens::ContextMeter`] one
    /// level down.
    context_meter: crate::tokens::ContextMeter,
    /// Its newest usage frame — what its session view's `/context` page
    /// reports as the **Last request** (`docs/architecture/context-usage.md`).
    pub last_usage: Option<crate::stream::TokenUsage>,
    /// Every usage frame it has reported, summed — the page's **This agent**
    /// row (`docs/architecture/context-usage.md`).
    pub usage: crate::context_usage::UsageLedger,
    /// How long it has been running — boundary-injected each frame
    /// ([`crate::app::App::set_agent_runtime`], the `set_status_times`
    /// pattern). Frozen at its final value once the agent settles.
    pub runtime: Duration,
    /// The [`crate::app::STATUS_VERBS`] index this turn's session-view
    /// status line opened on: the table's first verb for the agent's first
    /// turn, one past the last verb a turn wore for each continuation
    /// ([`reopen`](Self::reopen)) — the main session's walk, per agent.
    verb_start: usize,
    /// The index of the verb that line wears now — moved on every
    /// [`crate::app::VERB_ROTATION`] of runtime by the per-frame injection
    /// ([`rotate_verb`](Self::rotate_verb)), never by an event's runtime
    /// freeze, so the turn's summary names the verb the view last drew
    /// (`docs/ui/status-indicator.md`).
    verb_index: usize,
    /// Its own conversation: the prompt as a user message, its replies, its
    /// finished tool calls — everything the agent session view renders and
    /// the Ctrl+O agent cell expands.
    pub history: Vec<HistoryItem>,
    /// The in-flight reply text (its streaming buffer).
    pub streaming: Option<String>,
    /// Messages the user typed into this agent's session **while it was
    /// running** — the main session's [`steered`](crate::app::App::steered)
    /// queue, one level down (`docs/input/queue.md`). They show above the box in
    /// the agent's session view until its loop's next round boundary takes
    /// them, when [`StreamEvent::Steered`] turns each into a real user
    /// message on this transcript — the one event a **settled** run still
    /// folds, reopening it, because the continuation that delivered it is
    /// otherwise invisible. A run the user **stopped** instead drops these
    /// outright ([`interrupt`](AgentRun::interrupt)): its loop is cancelled,
    /// so a row left up would promise a delivery that cannot happen.
    pub queued: Vec<String>,
    /// **Follow-up turns** the user queued into this agent's session with
    /// **Tab** — the main session's [`queued`](crate::app::App::queued), one
    /// level down (`docs/input/queue.md`). Where [`queued`](Self::queued) rides the
    /// running loop's next round boundary, each of these waits for that loop
    /// to *settle* and then runs as its own chat continuation, one per
    /// settle, in submission order. They render above the box beneath the
    /// steered rows, blank-divided, exactly as a Tab follow-up does in the
    /// main view. A run the user **stopped** drops these too
    /// ([`interrupt`](AgentRun::interrupt)) — nothing will continue it.
    pub followups: VecDeque<String>,
    /// The messages the **user** sent this agent that its current run has
    /// read — a queued one once [`StreamEvent::Steered`] delivers it, the
    /// chat that started a continuation at once. What its settle tells the
    /// lead the answer also answered: the lead never wrote these, and its
    /// notice is the only place it can learn of them (`docs/agents/agent-tools.md`).
    /// Cleared by [`reopen`](Self::reopen), so each notice names its own
    /// run's.
    pub user_messages: Vec<String>,
    /// Its live tool calls, front running — the parallel-batch queue shape.
    pub tool_queue: VecDeque<ToolCall>,
    /// The calls it is still **generating** this round, a `write` previewed
    /// as its content streams, and the previewed ones its announced batch
    /// took over — the main session's [`App::tool_drafts`](crate::app::App::tool_drafts),
    /// one level down (`docs/ui/write-streaming.md`). Live-only, like the queue.
    drafts: crate::app::ToolDrafts,
    /// The running call's live rows and when each last changed — the main
    /// session's [`App::live_tail`](crate::app::App::live_tail), stamped
    /// with this agent's own command clock ([`Self::command_elapsed`]) so its
    /// session view's running cell follows the rows still moving too
    /// (`docs/ui/tool-streaming.md`).
    tool_live: crate::app::LiveTail,
    /// The final response text once it settles (what the caller received).
    pub result: Option<String>,
    /// The error message when it failed.
    pub error: Option<String>,
    /// Set by the user's **second** `x` (the clear) — the footer roster drops
    /// the row at once, while the entry's data stays for its group's
    /// resolution until the boundary's sweep collects it.
    pub hidden: bool,
    /// Set by the user's **first** `x` (the stop) — it buys the row the
    /// longer [`AGENT_STOPPED_LINGER`] so the red `◯` is there to be read
    /// (and cleared) instead of vanishing under the keypress.
    pub stopped_by_user: bool,
    /// The **newest call** the agent started — what the sticky
    /// `{Name}: {detail}` activity row is built from ([`activity`] /
    /// [`activity_shown`], over `activity_line`: a `bash` call's
    /// model-supplied `description`, else its args summary). Kept until the
    /// *next* tool starts, so the tree row shows what the agent is doing
    /// (or just did) rather than dropping
    /// to a generic `Working…` between calls (`docs/agents/agent-tool.md`). The
    /// record as it came: a file tool's path is the model's own, shown by
    /// the session's rule only when the row is painted.
    ///
    /// [`activity`]: AgentRun::activity
    /// [`activity_shown`]: AgentRun::activity_shown
    pub last_call: Option<AgentCall>,
    /// What the agent **said** last, once that run of text was finalised —
    /// its newest paragraph on one line — kept while it is the newest thing
    /// the agent did: the activity row's `Agent: {update}` outranks
    /// [`last_call`](Self::last_call) until the next call starts, which
    /// clears it. While the text still streams the row reads it off
    /// [`streaming`](Self::streaming) instead (`docs/agents/agent-progress.md`).
    last_update: Option<String>,
    /// Where the newest paragraph of [`streaming`](Self::streaming) starts,
    /// kept as the text arrives so the row never rescans it.
    paragraph: ParagraphStart,
    /// The **open** thinking phase's chain-of-thought, or `None` outside one
    /// — what the agent session view's strip previews while the phase runs
    /// (`ui::live_reasoning_lines`). Opened only by
    /// [`begin_reasoning`](Self::begin_reasoning), which the boundary calls
    /// only when the display is on, so with `/settings` **Hide thinking**
    /// set a delta is counted and dropped exactly as it always was
    /// (`docs/ui/thinking-stream.md`, `docs/agents/agent-view-streaming.md`).
    reasoning: Option<String>,
    /// Where this round's settled thinking cells landed in
    /// [`history`](Self::history), so the round's own [`StreamEvent::Usage`]
    /// frame can snap their token estimates to the provider's
    /// `reasoning_tokens` (`app::reasoning::snap_reasoning_tokens`). Cleared by
    /// each frame — one frame ends one round.
    round_reasoning: Vec<usize>,
    /// Where the reply attempt now streaming began on
    /// [`history`](Self::history) — the mark a [`StreamEvent::Retrying`]
    /// withdraws back to, taken at the attempt's first piece of output since
    /// an event of any other kind (`App::track_attempt`'s rule, over this
    /// transcript; `docs/providers/offline.md` *A connection lost mid-reply*).
    attempt_floor: Option<usize>,
    /// How long the open thinking phase has run — boundary-injected each
    /// frame from `Session::agent_thinking_clocks`, the
    /// [`runtime`](Self::runtime) pattern — so the session view's status line
    /// shows `Thinking for Ns` and a settle needs no clock of its own.
    /// `None` outside a phase.
    pub thinking: Option<Duration>,
    /// How long its **current running command** has executed —
    /// boundary-injected each frame from `Session::agent_command_clocks`, the
    /// [`thinking`](Self::thinking) pattern (cleared, then re-injected for the
    /// running ones), so the session view's `bash` tail counts its `+N lines
    /// (Ns · wait …)` clock row from the call's own `ToolStart` — never
    /// from the agent's whole [`runtime`](Self::runtime), which its status
    /// line shows
    /// (`docs/agents/agent-view-streaming.md`). `None` when no command is running.
    pub command_elapsed: Option<Duration>,
    /// Set while this agent's failed request is being retried
    /// ([`StreamEvent::Retrying`]) — its session view's status line shows the
    /// same `retrying {attempt}/{max}` clause the main turn's does, and the
    /// next streamed content clears it (`docs/providers/llm.md`).
    pub retry: Option<crate::app::RetryInfo>,
    /// Set while this agent's request **waits for a lost connection**
    /// ([`StreamEvent::Offline`]) — its session view's status line wears the
    /// same `Waiting for internet…` the main turn's does, over the same `No
    /// connection to {host}` row, the wait's start stamped off this run's
    /// [`runtime`](Self::runtime). Cleared the moment the host answers, and
    /// never `Some` beside [`retry`](Self::retry) (`docs/providers/offline.md`).
    pub offline: Option<crate::app::OfflineInfo>,
}

impl AgentRun {
    /// The `agent` call this run answers — the provider's id and the model's
    /// verbatim arguments the launch's [`AgentSpec`](crate::stream::AgentSpec)
    /// carried.
    #[must_use]
    pub fn with_call(
        mut self,
        call_id: Option<String>,
        arguments: Option<String>,
        position: Option<usize>,
    ) -> Self {
        self.call_id = call_id;
        self.arguments = arguments;
        self.position = position;
        self
    }

    /// A fresh, just-announced agent: transcript seeded with its prompt.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        agent_type: impl Into<String>,
        prompt: impl Into<String>,
        background: bool,
    ) -> Self {
        let prompt = prompt.into();
        Self {
            id: id.into(),
            description: description.into(),
            agent_type: agent_type.into(),
            prompt: prompt.clone(),
            background,
            call_id: None,
            position: None,
            arguments: None,
            status: AgentStatus::Pending,
            tool_uses: 0,
            tokens: 0,
            usage_tokens: 0,
            turn_usage_tokens: 0,
            turn_usage_cached: 0,
            turn_usage_cache_write: 0,
            context_used: 0,
            context_meter: crate::tokens::ContextMeter::default(),
            last_usage: None,
            usage: crate::context_usage::UsageLedger::default(),
            runtime: Duration::ZERO,
            verb_start: 0,
            verb_index: 0,
            history: vec![HistoryItem::Message(Message {
                role: Role::User,
                text: prompt,
                timestamp: String::new(),
                images: Vec::new(),
            })],
            streaming: None,
            queued: Vec::new(),
            followups: VecDeque::new(),
            user_messages: Vec::new(),
            tool_queue: VecDeque::new(),
            drafts: crate::app::ToolDrafts::default(),
            tool_live: crate::app::LiveTail::default(),
            result: None,
            error: None,
            hidden: false,
            stopped_by_user: false,
            last_call: None,
            last_update: None,
            paragraph: ParagraphStart::default(),
            reasoning: None,
            round_reasoning: Vec::new(),
            attempt_floor: None,
            thinking: None,
            command_elapsed: None,
            retry: None,
            offline: None,
        }
    }

    /// Does folding `event` **finalise the run of streamed text before it**
    /// — the segment boundaries [`apply`](Self::apply) flushes at (CLAUDE.md
    /// invariant 4's flush-before-you-interleave)?
    ///
    /// One definition, two readers: `apply` consumes the buffer at these
    /// points, and the session view commits the tail its `StreamRender`
    /// withheld — then resets that render — at exactly the same ones
    /// (`tui::agent`, through [`will_flush_segment`](Self::will_flush_segment),
    /// which adds the one boundary only the run's state can answer — so this
    /// stateless half is private, and the boundary cannot ask it alone). A
    /// hand-kept list in the boundary is what drifted
    /// before: [`StreamEvent::HookNote`] flushes the buffer in the fold but
    /// was missing from the view's list, so a `SubagentStop` continuation
    /// dropped the reply's withheld tail from the screen *and* left the
    /// render holding a prefix of a buffer that no longer existed
    /// (`docs/agents/agent-view-streaming.md`).
    ///
    /// [`StreamEvent::ThinkingStart`] is deliberately not here: it flushes
    /// too, but only when the display is on — the boundary reads that gate
    /// itself (`/settings` **Hide thinking**), so this stays the question
    /// `apply` alone answers. `every_flush_point_is_declared` pins the pair.
    #[must_use]
    const fn flushes_segment(event: &StreamEvent) -> bool {
        matches!(
            event,
            StreamEvent::ToolBatch(_)
                | StreamEvent::ToolStart { .. }
                | StreamEvent::HookNote { .. }
                | StreamEvent::Steered { .. }
                | StreamEvent::StreamDone
                | StreamEvent::Error(_)
        )
    }

    /// Will folding `event` into **this run** finalise the run of streamed
    /// text before it? The fold's static segment boundaries, plus the one
    /// only its state can answer — a streamed piece that **opens a previewed
    /// call**, a `write` whose cell goes up under the text before it
    /// (`docs/ui/write-streaming.md`). What the session view's flush, which
    /// runs before the fold, asks; `apply` flushes at exactly these points,
    /// which `every_flush_point_is_declared` pins for every event.
    #[must_use]
    pub fn will_flush_segment(&self, event: &StreamEvent) -> bool {
        Self::flushes_segment(event)
            || matches!(event, StreamEvent::ToolCallDelta(piece) if self.drafts.opens_preview(piece))
    }

    /// The calls it is still generating this round, and the previewed ones
    /// its announced batch took over (`docs/ui/write-streaming.md`).
    #[must_use]
    pub const fn drafts(&self) -> &crate::app::ToolDrafts {
        &self.drafts
    }

    /// Fold one of the subagent's own [`StreamEvent`]s into this entry —
    /// the roster-sized mirror of the main loop's `on_stream_event`. Returns
    /// `true` when the event settled the agent (reached a terminal status),
    /// so the boundary can post its completion notice / start the linger.
    /// Events for an already-settled agent are dropped (a late chunk racing
    /// a local kill).
    pub fn apply(&mut self, event: &StreamEvent) -> bool {
        if self.status.is_final() {
            // One event reaches a settled run: a message delivered into it
            // (`docs/input/queue.md`). A continuation is what delivered it — the
            // one its own thread starts for a message queued in the settle
            // window — and folding it is what keeps that continuation
            // visible: without this it runs with the model working, the
            // transcript showing nothing and the pending row never clearing.
            // A run the user **stopped** is closed for good: the `x` said so,
            // and nothing may resurrect it.
            if !matches!(event, StreamEvent::Steered { .. }) || self.stopped_by_user {
                return false;
            }
            self.reopen();
        }
        if self.status == AgentStatus::Pending && !matches!(event, StreamEvent::StreamDone) {
            self.status = AgentStatus::Running;
        }
        // Where the attempt now streaming began (`App::track_attempt`'s rule):
        // marked at its first piece of output, closed by any other event —
        // a `Retrying` leaves it for the withdrawal below.
        if event.is_attempt_output() {
            self.attempt_floor.get_or_insert(self.history.len());
        } else if !matches!(event, StreamEvent::Retrying { .. }) {
            self.attempt_floor = None;
        }
        match event {
            StreamEvent::Chunk(chunk) => {
                let buffer = self.streaming.get_or_insert_with(String::new);
                let from = buffer.len();
                buffer.push_str(chunk);
                self.paragraph.extend(buffer, from);
                self.tokens += crate::app::count_tokens(chunk) as u64;
                // Content arrived — the reconnect, or the wait for the
                // connection, is over (the main turn's rule, `docs/providers/llm.md`,
                // `docs/providers/offline.md`).
                self.recovered();
            }
            // A SubagentStop block's continuation feedback on this agent's
            // own loop (docs/tools/hooks.md): the reply so far becomes its own
            // message — the continuation streams a fresh one — and the note
            // stays on the agent's transcript, exactly like the main turn's,
            // its report the `● Ran 1 SubagentStop hook` cell the session
            // view commits (*The hook-run cell*).
            StreamEvent::HookNote {
                label,
                text,
                report,
            } => {
                self.flush_segment();
                self.history
                    .push(HistoryItem::HookNote(crate::app::HookNote {
                        label: label.clone(),
                        text: text.clone(),
                        report: report.clone(),
                        timestamp: String::new(),
                    }));
                self.tokens += crate::app::count_tokens(text) as u64;
            }
            // The agent's loop took a message the user queued into its
            // session (docs/input/queue.md): it stops waiting above the box and
            // becomes a real user message on this transcript, after the run
            // of streamed text ahead of it (invariant 4).
            StreamEvent::Steered { text } => {
                // Only the user's own messages wait on `queued`: the lead's
                // `agentsend` and a routed shell note arrive unannounced.
                if self.queued.contains(text) {
                    self.user_messages.push(text.clone());
                }
                self.queued.retain(|pending| pending != text);
                self.push_user_message(text);
                self.tokens += crate::app::count_tokens(text) as u64;
            }
            // Never sent on an agent's channel — the prompt-submit hook fires
            // only at the main session's spawn top (docs/tools/hooks.md); mapped so
            // the match stays total and honest if that ever changes.
            StreamEvent::PromptBlocked { .. } => {}
            // Never sent on an agent's channel either: only the main turn
            // reads the registry's board, and a shell a subagent launched
            // reports to it through its own queue, as a `Steered` message
            // (docs/tools/background.md).
            StreamEvent::NoticeDelivered { .. } => {}
            // A reasoning delta: counted so the footer tally ticks while the
            // agent thinks (the status-line pattern) and — **while a phase is
            // open** — accumulated for the live `● Thinking…` block the
            // session view previews. With the display off no phase is ever
            // opened, so it stays counted-and-dropped exactly as before
            // (`docs/agents/agent-view-streaming.md`).
            StreamEvent::ThinkingChunk(text) => {
                self.tokens += crate::app::count_tokens(text) as u64;
                self.recovered();
                if let Some(buffer) = self.reasoning.as_mut() {
                    buffer.push_str(text);
                }
            }
            // A piece of a call being generated — counted so the footer
            // tally ticks while the agent generates it (the status-line
            // pattern).
            StreamEvent::ToolCallDelta(piece) => {
                self.tokens += crate::app::count_tokens(&piece.tally_text()) as u64;
                self.recovered();
                // A `write` whose cell goes up: the text before it is its own
                // message first (invariant 4, `will_flush_segment`).
                if self.drafts.opens_preview(piece) {
                    self.flush_segment();
                }
                self.drafts.push(piece);
            }
            StreamEvent::ToolBatch(items) => {
                // The round came back (the wires that deliver a call whole
                // stream no delta first) — any wait is over.
                self.recovered();
                self.flush_segment();
                // The announcement is the round's **whole** queue, exactly as
                // `App::start_tool_batch` treats it: anything left over from
                // an earlier round would otherwise sit at the front and take
                // this round's resolutions, putting every cell off by one.
                self.tool_queue.clear();
                for item in items {
                    self.tool_queue.push_back(ToolCall {
                        name: item.name.clone(),
                        args: item.args.clone(),
                        status: ToolStatus::Waiting,
                        output: String::new(),
                        timestamp: String::new(),
                        shell: false,
                        truncated: false,
                        context_output: None,
                        arguments: None,
                        approval_note: None,
                        // A subagent's parallel calls are not aggregated: its
                        // session view keeps a cell per call.
                        batch: None,
                        call_id: item.call_id.clone(),
                        position: None,
                        run: crate::stream::ToolRun::default(),
                    });
                }
                // Each queued `Write` takes the draft it streamed
                // (docs/ui/write-streaming.md).
                self.drafts
                    .adopt(items.iter().map(|item| item.name.as_str()));
            }
            StreamEvent::ToolStart {
                // A subagent's calls are serial, so the queue front is its
                // call; the id still rides onto the record and lets the
                // agent view's permission prompt lead with the right cell.
                call_id,
                name,
                args,
                detail,
                arguments,
            } => {
                self.flush_segment();
                self.tool_uses += 1;
                self.tool_live.reset();
                // A new command has run no time yet: the roster tick injects
                // its clock, and until then the previous call's reading must
                // not stamp this one's first live rows (the main session's
                // `start_tool` rule).
                self.command_elapsed = None;
                // The call is now the newest thing it did: what it said
                // before gives the row back (docs/agents/agent-progress.md).
                self.last_update = None;
                self.last_call = Some(AgentCall {
                    name: name.clone(),
                    args: args.clone(),
                    detail: detail.clone(),
                });
                match self.tool_queue.front_mut() {
                    Some(front) if front.status == ToolStatus::Waiting => {
                        front.status = ToolStatus::Running;
                        front.name.clone_from(name);
                        front.args.clone_from(args);
                        front.arguments.clone_from(arguments);
                        front.call_id.clone_from(call_id);
                    }
                    _ => {
                        // A call no batch announced is its round's whole
                        // announcement — when it is the only call: pushed
                        // ahead of live ones, pairing by order could hand a
                        // cell another's file, so nothing previews
                        // (docs/ui/write-streaming.md).
                        if self.tool_queue.is_empty() {
                            self.drafts.adopt([name.as_str()]);
                        } else {
                            self.drafts.clear();
                        }
                        self.tool_queue.push_front(ToolCall {
                            name: name.clone(),
                            args: args.clone(),
                            arguments: arguments.clone(),
                            status: ToolStatus::Running,
                            output: String::new(),
                            timestamp: String::new(),
                            shell: false,
                            truncated: false,
                            context_output: None,
                            approval_note: None,
                            batch: None,
                            call_id: call_id.clone(),
                            position: None,
                            run: crate::stream::ToolRun::default(),
                        });
                    }
                }
            }
            // The auto mode classifier allowed this agent's command: the note
            // rides the running call so its resolved cell appends the
            // provenance row, exactly like the main turn's
            // (docs/state/permissions.md).
            StreamEvent::ToolNote { note, .. } => {
                if let Some(front) = self.tool_queue.front_mut()
                    && front.status == ToolStatus::Running
                {
                    front.add_note(note);
                }
            }
            StreamEvent::ToolOutput { chunk, .. } => {
                if let Some(front) = self.tool_queue.front_mut()
                    && front.status == ToolStatus::Running
                {
                    front.output.push_str(chunk);
                    self.tool_live.reset();
                }
            }
            // A terminal session's live output and a refined header, folded
            // exactly as the main session folds them
            // (docs/tools/interactive-shell.md).
            StreamEvent::ToolScreen {
                call_id: _,
                settled,
                live,
            } => {
                if let Some(front) = self.tool_queue.front_mut()
                    && front.status == ToolStatus::Running
                {
                    let now = self.command_elapsed.unwrap_or_default();
                    self.tool_live.apply(&mut front.output, settled, live, now);
                }
            }
            StreamEvent::ToolTitle { title, .. } => {
                if let Some(front) = self.tool_queue.front_mut()
                    && front.status == ToolStatus::Running
                {
                    front.args.clone_from(title);
                }
            }
            StreamEvent::ToolEnd {
                call_id: _,
                output,
                ok,
                truncated,
                run,
            } => {
                self.tokens += crate::app::count_tokens(output) as u64;
                if let Some(mut front) = self.pop_front_call() {
                    front.status = if *ok {
                        ToolStatus::Ok
                    } else {
                        ToolStatus::Failed
                    };
                    front.output = output.clone();
                    front.truncated = *truncated;
                    // How it ran, on every resolution, as the main session
                    // keeps it (docs/state/rollout-records.md).
                    front.run = *run;
                    self.history.push(HistoryItem::Tool(front));
                }
            }
            // The user refused this agent's call at the shared permission
            // prompt: red like any failure, with the model-facing `result`
            // kept beside the cell text so the agent's own derived context
            // (its Ctrl+D view, and a continuation run over its stored
            // messages) replays what it actually read (docs/state/permissions.md).
            StreamEvent::ToolRejected {
                call_id: _,
                display,
                result,
                truncated,
                run,
            } => {
                self.tokens += crate::app::count_tokens(result) as u64;
                if let Some(mut front) = self.pop_front_call() {
                    front.status = ToolStatus::Failed;
                    front.output = display.clone();
                    front.context_output = Some(result.clone());
                    // The main boundary flags it on all three resolutions
                    // (`tui::stream`), so the expanded cell appends its dim
                    // `…` marker here too.
                    front.truncated = *truncated;
                    front.run = *run;
                    self.history.push(HistoryItem::Tool(front));
                }
            }
            // A subagent's `run_in_background` launch (its executor carries
            // the shared registry, attributed via `BgOrigin`): resolve the
            // front call as backgrounded — the shell itself lives on the
            // shared list (`docs/tools/background.md`).
            StreamEvent::ToolBackgrounded { output, run, .. } => {
                // The launch acknowledgement is text the model reads, so it
                // is charged like every other resolution's — the main
                // session's `resolve_front_tool` charges it too.
                self.tokens += crate::app::count_tokens(output) as u64;
                if let Some(mut front) = self.pop_front_call() {
                    front.status = ToolStatus::Backgrounded;
                    front.output = output.clone();
                    front.run = *run;
                    self.history.push(HistoryItem::Tool(front));
                }
            }
            StreamEvent::Usage(usage) => {
                // One frame ends one round: this is where the round's
                // `Thought for …` cells trade their token estimate for
                // the provider's own `reasoning_tokens`, the main session's
                // snap over this agent's transcript
                // (`docs/ui/thinking-stream.md`).
                let targets = std::mem::take(&mut self.round_reasoning);
                crate::app::snap_reasoning_tokens(&mut self.history, &targets, usage.reasoning);
                self.usage_tokens += usage.total();
                self.tokens = self.usage_tokens;
                // Its session view's `/context` page (docs/architecture/context-usage.md).
                self.last_usage = Some(*usage);
                self.usage.record(usage);
                self.turn_usage_tokens += usage.total();
                self.turn_usage_cached += usage.cached;
                self.turn_usage_cache_write += usage.cache_write;
                // The round's `input` is the provider's count of the whole
                // re-sent context — this agent's context size, and its
                // meter's base — the main session's `apply_usage` rule
                // (`docs/agents/agent-context-gauge.md`, `docs/architecture/token-estimate.md`).
                let (estimate, images) = self.estimate_context();
                self.context_meter.measured(usage.input, estimate, images);
                self.context_used = self.context_meter.reading(estimate);
            }
            StreamEvent::StreamDone => {
                // A phase still open when the round ended keeps what streamed
                // — settled first, so its cell sits ahead of the reply it
                // preceded (`docs/ui/thinking-stream.md`).
                self.settle_thinking();
                self.result = self.flush_segment();
                self.tool_queue.clear();
                self.drafts.clear();
                self.status = AgentStatus::Done;
                // The gauge folds in what joined after the last usage frame —
                // the reply — or, with no frame this turn (the dummy, a
                // provider that omits them), stands on the estimate over this
                // transcript: `App::take_turn_summary`'s rule
                // (`docs/agents/agent-context-gauge.md`).
                self.context_used = match self.estimate_context() {
                    (0, _) => 0,
                    (estimate, _) => self.context_meter.reading(estimate),
                };
                // The turn's dim receipt, the main turn-end's exact shape
                // (`Done for 59s · 6.1k tokens (2.8k cached)`): the agent
                // session view commits it and every rebuild/Ctrl+O renders it
                // from here, while the derived context skips it as chrome
                // like any summary (docs/agents/agent-tool.md). The runtime was
                // frozen at its live value just before this event
                // (`tui::agent`), so it is the turn's elapsed. A failure
                // records none, main parity; a stop records none either — the
                // main turn's interrupt receipt has no twin here, the stopped
                // agent's red roster row keeping its elapsed and tally in view
                // instead (docs/input/interrupt.md).
                self.history.push(HistoryItem::Summary(TurnSummary {
                    // The past tense of the verb the view's line last wore.
                    verb: self.status_verb().done,
                    secs: self.runtime.as_secs(),
                    timestamp: String::new(),
                    shells: 0,
                    tokens: usize::try_from(self.turn_usage_tokens).unwrap_or(usize::MAX),
                    estimated: 0,
                    cached: usize::try_from(self.turn_usage_cached).unwrap_or(usize::MAX),
                    cache_write: usize::try_from(self.turn_usage_cache_write).unwrap_or(usize::MAX),
                }));
                return true;
            }
            StreamEvent::Error(message) => {
                // The open thought survives the failure, ahead of the partial
                // reply and the red notice (the main session's order).
                self.settle_thinking();
                self.flush_segment();
                self.resolve_tools(message);
                self.drafts.clear();
                self.error = Some(message.clone());
                self.status = AgentStatus::Failed;
                return true;
            }
            // The ToolRejected shape's green twin: a resolution whose
            // model-facing text differs from its cell. Subagents reach it on
            // every `write`/`edit` — the file tools' one-line ack over the
            // numbered body (docs/tools/tools.md) — and on a loaded `skill`
            // (docs/tools/skills.md). (The ask tool sends it too, but subagents are
            // never offered that one.) Keeping both texts is what lets the
            // agent's own Ctrl+D and a continuation run replay what it read.
            StreamEvent::ToolAnswered {
                call_id: _,
                display,
                result,
                truncated,
                run,
            } => {
                self.tokens += crate::app::count_tokens(result) as u64;
                if let Some(mut front) = self.pop_front_call() {
                    front.status = ToolStatus::Ok;
                    front.output = display.clone();
                    front.context_output = Some(result.clone());
                    front.truncated = *truncated;
                    front.run = *run;
                    self.history.push(HistoryItem::Tool(front));
                }
            }
            // Subagents are never offered the task tools (the lead agent
            // plans, subagents execute — docs/tools/task-tools.md), so this is
            // unreachable today: recorded as an ordinary resolved tool cell
            // on the agent's own transcript so the mapping stays total and
            // honest if that ever changes. The snapshot is ignored — the
            // checklist is the main session's.
            StreamEvent::TaskCall {
                name,
                args,
                output,
                ok,
                ..
            } => {
                self.tokens += crate::app::count_tokens(output) as u64;
                self.tool_uses += 1;
                self.history.push(HistoryItem::Tool(ToolCall {
                    name: name.clone(),
                    args: args.clone(),
                    status: if *ok {
                        ToolStatus::Ok
                    } else {
                        ToolStatus::Failed
                    },
                    output: output.clone(),
                    timestamp: String::new(),
                    shell: false,
                    truncated: false,
                    context_output: None,
                    arguments: None,
                    approval_note: None,
                    batch: None,
                    call_id: None,
                    position: None,
                    run: crate::stream::ToolRun::default(),
                }));
            }
            // A permission request is the *user's* business, not the roster's:
            // the boundary lifts it out of the agent channel and raises the
            // shared inline prompt (docs/state/permissions.md). Nothing about this
            // agent's own state changes while it waits. An ask request would
            // be too (docs/tools/ask.md) — unreachable, since subagents never get
            // the tool.
            // This agent's request failed and is being retried: its session
            // view's status line says so, exactly like the main turn's
            // (`docs/providers/llm.md`). The failed attempt is void — whatever it
            // streamed is withdrawn before the next one's first byte, so a
            // reply restarted after its connection dropped is never shown
            // twice (`docs/providers/offline.md` *A connection lost mid-reply*).
            StreamEvent::Retrying { attempt, max } => {
                let _ = self.withdraw_attempt();
                // The host answered (badly): any outage is over.
                self.recovered();
                self.retry = Some(crate::app::RetryInfo {
                    attempt: *attempt,
                    max: *max,
                });
            }
            // No connection could be made for this agent's request and its
            // loop is waiting for one: the session view says so, exactly
            // like the main turn's strip (`docs/providers/offline.md`). The first
            // announcement stamps the wait's start with the runtime the view
            // shows; later ones move the count on and keep it.
            StreamEvent::Offline { host, attempts } => {
                let began = self
                    .offline
                    .as_ref()
                    .map_or(self.runtime, |outage| outage.began);
                self.recovered();
                self.offline = Some(crate::app::OfflineInfo {
                    host: host.clone(),
                    attempts: *attempts,
                    began,
                });
            }
            // The round's calls are all made: nothing more of them streams
            // (docs/ui/write-streaming.md).
            StreamEvent::RoundCalls(_) => self.drafts.seal(),
            StreamEvent::Permission(_)
            | StreamEvent::AskUser(_)
            | StreamEvent::ThinkingStart
            | StreamEvent::ThinkingEnd
            | StreamEvent::AgentBatch { .. }
            | StreamEvent::AgentGroupDone { .. } => {}
        }
        false
    }

    /// The agent's context size in tokens (see the field doc) — what its
    /// session view's footer gauges ([`crate::app::App::context_gauge`]).
    #[must_use]
    pub const fn context_used(&self) -> u64 {
        self.context_used
    }

    /// The estimate of this agent's context, and the pictures in it: its own
    /// transcript derived exactly as a request would be
    /// ([`crate::context::context_messages`]), counted by the rule the main
    /// gauge counts with (`app::estimate_messages_tokens`) — and a **true
    /// zero** for a transcript that derives nothing, the main estimate's
    /// empty-conversation rule. The agent's system prompt and briefing are
    /// not in it: this type knows neither (they are the launch's, shown by
    /// the view's Ctrl+D) — the provider's count of them rides the meter's
    /// base, which only the change since is estimated against.
    fn estimate_context(&self) -> (u64, usize) {
        if !crate::context::derives_conversation(&self.history) {
            return (0, 0);
        }
        let messages = crate::context::context_messages(&self.history);
        let tokens = crate::app::estimate_messages_tokens(&messages);
        let images = messages.iter().map(|message| message.images.len()).sum();
        (u64::try_from(tokens).unwrap_or(u64::MAX), images)
    }

    /// What its usage reports taught about its provider's tokens against the
    /// estimate — what its session view's `/context` and Ctrl+D count in.
    #[must_use]
    pub const fn context_calibration(&self) -> crate::tokens::Calibration {
        self.context_meter.calibration()
    }

    /// Locally settle the agent as user-stopped (`x`, Esc on its foreground
    /// group, `/clear`): resolve any running tool, keep the partial, flip to
    /// [`AgentStatus::Interrupted`]. Returns `true` when this call did the
    /// settling (`false` if it was already final).
    pub fn interrupt(&mut self) -> bool {
        if self.status.is_final() {
            return false;
        }
        // Keep a partial thought: what streamed is what the user saw.
        self.settle_thinking();
        self.flush_segment();
        self.resolve_tools(crate::app::INTERRUPT_TOOL_OUTPUT);
        // A stopped agent's loop is cancelled, so nothing will ever take what
        // is still queued for it: drop the rows rather than leave them
        // promising a delivery that cannot happen (`docs/input/queue.md`). Only a
        // *stop* does this — a natural settle with a message still queued
        // hands it back for a continuation to deliver. The Tab follow-ups go
        // with them: each would have run as a continuation of a run that is
        // now cancelled.
        self.queued.clear();
        self.followups.clear();
        // A call it never finished making is never recorded
        // (docs/ui/write-streaming.md).
        self.drafts.clear();
        self.status = AgentStatus::Interrupted;
        true
    }

    /// Settle this run from its **group's** resolution — the path for a
    /// member whose own terminal event never arrived (a cancelled loop
    /// returns without one, and the offline dummy scripts none at all), so
    /// the foreground group's outcome is all there is to settle from.
    ///
    /// Every other settle resolves what is in flight; this one must too, or
    /// a *finished* agent goes on owning live cells — its session view
    /// previews a `⎿ Running…` that can never resolve, and the next chat
    /// continuation's batch queues behind the ghost. `output` is the resolved
    /// cell's text (the call did not complete, whatever the run reported).
    /// Inert once final.
    pub fn settle_from_group(&mut self, status: AgentStatus, output: &str) {
        if self.status.is_final() {
            return;
        }
        // A partial thought is what the user saw — the interrupt's rule.
        self.settle_thinking();
        self.resolve_tools(output);
        self.drafts.clear();
        self.status = status;
    }

    /// How long this entry lingers on the roster once it settles, before the
    /// boundary's sweep drops it. A user stop earns
    /// [`AGENT_STOPPED_LINGER`] (the red row is there to be read and
    /// cleared); everything else keeps [`AGENT_LINGER`]. Both windows are
    /// 30s today — only the *reason* differs, and the stop's constant stays
    /// separate so its countdown can never be restarted by a later group
    /// resolution.
    #[must_use]
    pub const fn linger(&self) -> Duration {
        if self.stopped_by_user {
            AGENT_STOPPED_LINGER
        } else {
            AGENT_LINGER
        }
    }

    /// A user chat message sent into the agent's session — recorded into its
    /// transcript (the registry delivers the same text to the running loop).
    pub fn push_user_message(&mut self, text: &str) {
        self.flush_segment();
        self.history.push(HistoryItem::Message(Message {
            role: Role::User,
            text: text.to_string(),
            timestamp: String::new(),
            images: Vec::new(),
        }));
    }

    /// Park a message the user typed into this agent's session while it was
    /// running: it shows above the box until the agent's next round boundary
    /// takes it ([`StreamEvent::Steered`]). Deliberately **not** recorded on
    /// the transcript yet — claiming the agent has read something it has not
    /// is what the main session's queue avoids too (`docs/input/queue.md`).
    pub fn queue_chat(&mut self, text: &str) {
        self.queued.push(text.to_string());
    }

    /// Park a **follow-up turn** (Tab) for this agent: it waits below the
    /// steered rows until the agent's loop settles, when the boundary hands
    /// it over as a chat continuation — the main session's `queue_draft`, one
    /// level down (`docs/input/queue.md`). Always its own entry: Tab means "a
    /// separate turn", here as in the main view.
    pub fn queue_followup(&mut self, text: &str) {
        self.followups.push_back(text.to_string());
    }

    /// Take the **next** follow-up turn (the boundary drains one per settle,
    /// `App::drain_next_batch`'s twin). `None` when nothing is waiting.
    pub fn take_followup(&mut self) -> Option<String> {
        self.followups.pop_front()
    }

    /// Take the **last** follow-up back (Alt+Up's pull into the composer,
    /// `App::drain_last_batch`'s twin), leaving the earlier ones queued.
    pub fn take_last_followup(&mut self) -> Option<String> {
        self.followups.pop_back()
    }

    /// The host answered: the reconnect or the wait for the connection is
    /// over, so both live indicators come down together (the main turn's
    /// `TurnStatus::recovered`).
    fn recovered(&mut self) {
        self.retry = None;
        self.offline = None;
    }

    /// The failed reply attempt is void: take back what it streamed — the
    /// main session's [`App::withdraw_attempt`](crate::app::App::withdraw_attempt)
    /// over this agent's transcript and strip (`docs/providers/offline.md` *A
    /// connection lost mid-reply*). The reply text and where its newest
    /// paragraph starts (the activity row reads the buffer at those offsets),
    /// an open thought and its injected clock, the calls being generated, and
    /// the transcript items recorded since the attempt's mark — only reply
    /// text and thoughts, never an item of any other kind. A withdrawn run of
    /// text takes the `Agent: …` update it set with it. What
    /// [`apply`](Self::apply) does at a [`StreamEvent::Retrying`]; the session
    /// view runs it first to learn what went (`tui::agent`), and the fold's
    /// own call then finds nothing.
    pub fn withdraw_attempt(&mut self) -> crate::app::Withdrawn {
        let live = self
            .streaming
            .as_deref()
            .is_some_and(|text| !text.is_empty())
            || self.reasoning.is_some()
            || self.drafts.previews().next().is_some();
        self.streaming = None;
        self.paragraph = ParagraphStart::default();
        self.reasoning = None;
        self.thinking = None;
        self.drafts.clear();
        let mut records = 0;
        if let Some(floor) = self.attempt_floor.take() {
            while self.history.len() > floor
                && let Some(item) = self.history.last()
            {
                match item {
                    HistoryItem::Reasoning(_) => {}
                    HistoryItem::Message(message) if message.role == Role::Assistant => {
                        // The update it set quoted text that is gone now.
                        self.last_update = None;
                    }
                    _ => break,
                }
                self.history.pop();
                records += 1;
            }
        }
        let len = self.history.len();
        self.round_reasoning.retain(|&index| index < len);
        crate::app::Withdrawn { records, live }
    }

    /// The status verb its session view's line wears now — the synthesized
    /// status line shows it and the turn's summary records its past tense.
    #[must_use]
    pub fn status_verb(&self) -> StatusVerb {
        StatusVerb::at(self.verb_index)
    }

    /// Move the status verb on to where the current
    /// [`runtime`](Self::runtime) puts it — one verb further every
    /// [`crate::app::VERB_ROTATION`]. The per-frame half of the runtime
    /// injection ([`crate::app::App::set_agent_runtime`]); an event's runtime
    /// freeze never calls it, so a turn that settles just past a rotation no
    /// frame drew still names the verb the view showed.
    pub fn rotate_verb(&mut self) {
        self.verb_index = StatusVerb::rotated(self.verb_start, self.runtime);
    }

    /// A follow-up chat turn began (a continuation run): reopen a settled
    /// agent so new events fold in again. The turn-scoped usage counters
    /// reset — the next settle's `Done for Ns · {n} tokens` receipt is the
    /// continuation's own — while the cumulative roster tally stands, and the
    /// status verb walks on one past the last verb the finished turn wore,
    /// as a new main turn's does.
    pub fn reopen(&mut self) {
        self.verb_start = self.verb_index + 1;
        self.verb_index = self.verb_start;
        self.status = AgentStatus::Running;
        self.stopped_by_user = false;
        self.result = None;
        self.error = None;
        self.turn_usage_tokens = 0;
        self.turn_usage_cached = 0;
        self.turn_usage_cache_write = 0;
        self.round_reasoning.clear();
        self.user_messages.clear();
        self.drafts.clear();
    }

    /// The tree row's activity: what the agent is doing — or, between tool
    /// calls, what it just did (the sticky [`last_call`] holds until the
    /// next call starts, so the row keeps its context instead of dropping to
    /// `Working…` while the agent reasons over a result). What the agent
    /// **says** is its activity too, `Agent: {update}`, and the newer of the
    /// two wins (`docs/agents/agent-progress.md`). The record's own text, a file
    /// tool's path as the model sent it.
    ///
    /// [`last_call`]: AgentRun::last_call
    #[must_use]
    pub fn activity(&self) -> String {
        self.activity_shown(|_, args| args.to_string())
    }

    /// [`activity`](Self::activity) with the newest call's `args` summary
    /// shown the way the caller says — `show_args(name, args)` maps it to
    /// its display form (the TUI hands in its session's path rule, so a
    /// file tool's row reads `Write: ~/x.py`; `docs/tools/tools.md` *Path
    /// display*) — over the same grammar. The labels, a `bash` call's
    /// description and the record itself are untouched.
    #[must_use]
    pub fn activity_shown(&self, show_args: impl FnOnce(&str, &str) -> String) -> String {
        match self.status {
            AgentStatus::Pending => AgentStatus::Pending.label().to_string(),
            AgentStatus::Running => match self.update() {
                Some(update) => format!("{AGENT_UPDATE_LABEL}: {update}"),
                None => self.last_call.as_ref().map_or_else(
                    || "Working…".to_string(),
                    |call| {
                        activity_line(
                            &call.name,
                            &show_args(&call.name, &call.args),
                            call.detail.as_deref(),
                        )
                    },
                ),
            },
            settled => settled.label().to_string(),
        }
    }

    /// What the agent said last, while that is the newest thing it did: the
    /// paragraph it is streaming now, else the one its last finalised run of
    /// text ended on — until a call starts (`docs/agents/agent-progress.md`).
    fn update(&self) -> Option<String> {
        self.streaming
            .as_deref()
            .and_then(|text| self.paragraph.head(text))
            .or_else(|| self.last_update.clone())
    }

    /// Finalise the streamed run of text into the transcript (the
    /// `flush_streaming_segment` shape) and return it. Its newest paragraph
    /// stays on the activity row until the next call starts.
    fn flush_segment(&mut self) -> Option<String> {
        let text = self.streaming.take()?;
        let paragraph = std::mem::take(&mut self.paragraph);
        if text.is_empty() {
            return None;
        }
        if let Some(update) = paragraph.head(&text) {
            self.last_update = Some(update);
        }
        self.history.push(HistoryItem::Message(Message {
            role: Role::Assistant,
            text: text.clone(),
            timestamp: String::new(),
            images: Vec::new(),
        }));
        Some(text)
    }

    /// Open a thinking phase on this agent — the boundary calls it at its
    /// [`StreamEvent::ThinkingStart`], and **only when the display is on**
    /// (`/settings` **Hide thinking**), which is the whole gate: with it off
    /// no buffer exists and every delta stays counted-and-dropped
    /// (`docs/agents/agent-view-streaming.md`).
    ///
    /// **Flush before you interleave** (CLAUDE.md invariant 4): a model can
    /// reason after it has begun answering, so the run of text before the
    /// phase becomes its own message here, as the live block goes up — the
    /// `ThinkingStart` dance the main session does.
    pub fn begin_reasoning(&mut self) {
        self.flush_segment();
        self.reasoning = Some(String::new());
    }

    /// The chain-of-thought streamed so far in the open phase, or `None` when
    /// none is open — what the session view's strip previews. `Some("")`
    /// right after [`begin_reasoning`](Self::begin_reasoning): the phase is
    /// real before its first delta, and the block should say so.
    #[must_use]
    pub fn reasoning(&self) -> Option<&str> {
        self.reasoning.as_deref()
    }

    /// Inject the open phase's elapsed (the boundary's thinking clock, the
    /// [`crate::app::App::set_agent_runtime`] pattern) so the session view's
    /// status line shows `Thinking for Ns` and a settle reached from the pure
    /// side ([`interrupt`](Self::interrupt), an `Error`) needs no clock.
    pub fn set_thinking(&mut self, elapsed: Option<Duration>) {
        self.thinking = elapsed;
    }

    /// Inject the running command's elapsed (the boundary's per-agent
    /// command clock, the [`set_thinking`](Self::set_thinking) pattern) —
    /// what the session view's running `bash` cell counts its `+N lines
    /// (Ns · wait …)` clock row with. `None` when no command is running.
    pub fn set_command_elapsed(&mut self, elapsed: Option<Duration>) {
        self.command_elapsed = elapsed;
    }

    /// The running call's live rows and when each last changed — what this
    /// agent's session view's running cell follows
    /// ([`crate::app::LiveTail`]).
    #[must_use]
    pub fn live_tail(&self) -> &crate::app::LiveTail {
        &self.tool_live
    }

    /// Close the open thinking phase, recording it as a
    /// [`HistoryItem::Reasoning`] on **this agent's** transcript and returning
    /// it for the session view to commit as the collapsed `Thought for …`
    /// cell. `secs` is the phase's wall-clock (the boundary's clock at a
    /// `ThinkingEnd`; the injected [`thinking`](Self::thinking) at every other
    /// settle point).
    ///
    /// `None` — recording nothing — when no phase was open, or when it
    /// produced no text: some providers open and close one without a single
    /// delta, and a `Thought for 0s` cell for that is noise. The main
    /// session's rule exactly (`docs/ui/thinking-stream.md`).
    pub fn finish_reasoning(&mut self, secs: u64) -> Option<crate::app::Reasoning> {
        self.thinking = None;
        let text = self.reasoning.take()?;
        if text.trim().is_empty() {
            return None;
        }
        let reasoning = crate::app::Reasoning {
            tokens: crate::app::count_tokens(&text),
            text,
            secs,
            timestamp: String::new(),
        };
        // Remember where it landed so this round's usage frame can snap its
        // estimate to the provider's own count.
        self.round_reasoning.push(self.history.len());
        self.history.push(HistoryItem::Reasoning(reasoning.clone()));
        Some(reasoning)
    }

    /// Settle an open phase from the injected [`thinking`](Self::thinking)
    /// elapsed — the pure settle points (`StreamDone`, `Error`,
    /// [`interrupt`](Self::interrupt)), which have no clock of their own.
    /// A no-op when no phase is open.
    fn settle_thinking(&mut self) {
        let secs = self.thinking.unwrap_or_default().as_secs();
        self.finish_reasoning(secs);
    }

    /// Take the front call off the live queue — it resolved — and, with it,
    /// the draft a previewed one owns (`docs/ui/write-streaming.md`): both are
    /// front-first in queue order, so the pair stays aligned.
    fn pop_front_call(&mut self) -> Option<ToolCall> {
        let call = self.tool_queue.pop_front()?;
        self.drafts.resolve(&call.name);
        Some(call)
    }

    /// Resolve every live tool call as failed with `output` (an interrupt or
    /// backend error killed the run): the running — or permission-waiting —
    /// front call and each `⎿ Waiting…` sibling behind it, in the batch's
    /// order, each recorded on the agent's transcript. The main session's
    /// rule (`App::fail_live_queue`, `docs/input/interrupt.md`): every call the
    /// agent made keeps its cell in the session view, and its Ctrl+D agrees
    /// with what a chat continuation resumes from.
    fn resolve_tools(&mut self, output: &str) {
        while let Some(mut call) = self.pop_front_call() {
            call.status = ToolStatus::Failed;
            call.output = output.to_string();
            self.history.push(HistoryItem::Tool(call));
        }
    }
}

/// The call an agent's sticky activity row names ([`AgentRun::last_call`]):
/// the [`StreamEvent::ToolStart`] fields the row's grammar
/// (`activity_line`) reads, kept as they came.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCall {
    /// The display name (`Write`, `Bash`, `WebSearch`).
    pub name: String,
    /// The one-line args summary — a file tool's path, a command, JSON.
    pub args: String,
    /// The model's own description, when the call carried one (`bash`).
    pub detail: Option<String>,
}

/// The sticky `{Name}: {detail}` activity row for one starting call — the one
/// grammar every agent activity wears (`docs/agents/agent-tool.md`): the model's own
/// description when it gave one (`Bash: Fetching current weather…`), else the
/// call's args summary (`Write: game.py` — never the cell header's
/// `Write({args})`, whose parens read as clutter on a dim clipped one-liner).
/// A call with nothing to say after the colon is the bare name.
fn activity_line(name: &str, args: &str, detail: Option<&str>) -> String {
    if let Some(detail) = detail {
        return format!("{name}: {detail}");
    }
    // One dim clipped row: a `bash` summary keeps the command's newlines and
    // space runs for the cell header, flattened here to the row's one line.
    let args = crate::llm::tools::flatten_one_line(args);
    if args.is_empty() {
        return name.to_string();
    }
    format!("{name}: {args}")
}

/// How much of the newest paragraph the activity row keeps — wider than any
/// row it is clipped to, and a bound on the work: the row is built several
/// times a frame while the text streams (`docs/agents/agent-progress.md`).
const ACTIVITY_UPDATE_MAX_CHARS: usize = 512;

/// Where the newest paragraph of a growing run of reply text starts — its
/// lines after the last blank one, the way the activity row shows what the
/// agent is saying: the run's opening is stale by the time it says *now let
/// me…*, and a single line can be a fragment of a list. Found as the text
/// arrives ([`extend`](Self::extend)), so a frame never reads the run again —
/// a table or a code block can be one paragraph thousands of lines long, and
/// the line being written can end in a page of blank space
/// (`docs/agents/agent-progress.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ParagraphStart {
    /// Where the line being written starts.
    line: usize,
    /// The line being written has something on it so far.
    line_has_text: bool,
    /// Where the newest paragraph with a finished line starts.
    paragraph: Option<usize>,
    /// A blank line has ended since that paragraph's last line: the next
    /// line with something on it opens a new one.
    closed: bool,
}

impl ParagraphStart {
    /// Take in what `text` gained since it was `from` bytes long, reading
    /// nothing before it.
    fn extend(&mut self, text: &str, from: usize) {
        let Some(added) = text.get(from..) else {
            return;
        };
        let mut at = from;
        for piece in added.split_inclusive('\n') {
            at += piece.len();
            let (body, ended) = piece
                .strip_suffix('\n')
                .map_or((piece, false), |body| (body, true));
            // Once the line has something on it, it is not checked again.
            self.line_has_text =
                self.line_has_text || body.contains(|ch: char| !ch.is_whitespace());
            if !ended {
                continue;
            }
            if !self.line_has_text {
                self.closed |= self.paragraph.is_some();
            } else if self.closed || self.paragraph.is_none() {
                self.paragraph = Some(self.line);
                self.closed = false;
            }
            self.line = at;
            self.line_has_text = false;
        }
    }

    /// The newest paragraph's head in `text` on one line, up to
    /// [`ACTIVITY_UPDATE_MAX_CHARS`] — the line being written, when it opens
    /// one — or `None` while `text` says nothing.
    fn head(&self, text: &str) -> Option<String> {
        let opens = self.line_has_text && (self.closed || self.paragraph.is_none());
        let start = if opens { self.line } else { self.paragraph? };
        let head = one_line_head(text.get(start..)?, ACTIVITY_UPDATE_MAX_CHARS);
        (!head.is_empty()).then_some(head)
    }
}

/// `text`'s head on one line — every whitespace run one space, the start
/// trimmed — at most `max` characters of it, read from no more than four
/// times as many: a run with no space in it (a URL, a blob) is cut inside
/// itself rather than copied whole, and a run of nothing but whitespace is
/// never walked to its end. Four times leaves room for any indentation or
/// table padding a paragraph carries.
fn one_line_head(text: &str, max: usize) -> String {
    let mut line = String::new();
    let mut kept = 0;
    let mut space = false;
    for ch in text.chars().take(max.saturating_mul(4)) {
        if ch.is_whitespace() {
            space = !line.is_empty();
            continue;
        }
        if kept + usize::from(space) >= max {
            break;
        }
        if space {
            line.push(' ');
            kept += 1;
            space = false;
        }
        line.push(ch);
        kept += 1;
    }
    line
}

/// What a subagent's forwarder keeps of the run it relays
/// (`llm::backend::spawn_subagent_run`): the run of reply text since the
/// agent last did anything else — its **answer**, if nothing follows it —
/// and, as the run goes, the record `agentoutput` reports from: each call it
/// starts, each run of text that more work followed, as a **progress
/// update**, and the mark of each message the agent read, where the stream
/// announces it (`docs/agents/agent-progress.md`). Pure bookkeeping over the events
/// the forwarder relays, so the rule is tested here rather than in a thread
/// body.
#[derive(Debug)]
pub struct RunRecorder {
    registry: AgentRegistry,
    id: String,
    text: String,
}

impl RunRecorder {
    /// A recorder for agent `id`'s run, keeping its record on `registry`.
    #[must_use]
    pub fn new(registry: AgentRegistry, id: impl Into<String>) -> Self {
        Self {
            registry,
            id: id.into(),
            text: String::new(),
        }
    }

    /// Take one event of the run into account.
    pub fn observe(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::Chunk(chunk) => self.text.push_str(chunk),
            // The text before a call is something it said on the way — on
            // the record ahead of the call, where it said it.
            StreamEvent::ToolStart { name, args, .. } => {
                self.said();
                self.registry
                    .record_call(&self.id, &crate::app::tool_header_text(name, args));
            }
            // More work follows the text: a tool round, or a message landing
            // mid-run — whatever the agent had said before it is no longer
            // its final answer, so the parent must not be handed the two
            // concatenated (docs/input/queue.md). Every round that reaches a
            // boundary emits a tool event first today, which already ends the
            // text — but that is a property of the current paths, not of the
            // rule, and the rule is what belongs here. A stop hook's
            // continuation (docs/tools/hooks.md) turns the answer so far into
            // something said on the way, too.
            StreamEvent::ToolBatch(_)
            | StreamEvent::AgentBatch { .. }
            | StreamEvent::HookNote { .. } => self.said(),
            // A message the loop read: what the agent said before it, then
            // the mark, placed here in the stream's order.
            StreamEvent::Steered { .. } => {
                self.said();
                self.registry.announce_read(&self.id);
            }
            // The attempt that streamed it is void and its request is being
            // sent again (docs/providers/offline.md *A connection lost mid-reply*): the
            // text is neither the answer nor something said on the way.
            StreamEvent::Retrying { .. } => self.text.clear(),
            _ => {}
        }
    }

    /// The run's answer: the text since the agent last did anything else.
    #[must_use]
    pub fn into_answer(self) -> String {
        self.text
    }

    /// The text so far was said on the way: a progress update, not the
    /// answer.
    fn said(&mut self) {
        self.registry
            .record_update(&self.id, &std::mem::take(&mut self.text));
    }
}

/// The base36 alphabet agent ids are drawn from (the task-id alphabet).
const AGENT_ID_ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// A Claude-Code-style agent id — an `a` prefix + 8 lowercase base36 chars
/// (`a7k2m9x4q`), the background registry's `task_id` shape with its own
/// prefix so a shell id can never be mistaken for an agent id. Deterministic
/// in `seed` (pure, testable); the registry feeds fresh entropy per launch.
#[must_use]
pub fn agent_id(seed: u64) -> String {
    let mut mixed = splitmix64(seed);
    let mut id = String::with_capacity(9);
    id.push('a');
    for _ in 0..8 {
        id.push(AGENT_ID_ALPHABET[(mixed % 36) as usize] as char);
        mixed /= 36;
    }
    id
}

/// SplitMix64 (the `background::splitmix64` mixer — duplicated rather than
/// exported so the two modules stay independent).
fn splitmix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Fresh entropy for one id roll (the `background::entropy_seed` pattern).
fn entropy_seed(counter: u64) -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let folded = (nanos as u64) ^ ((nanos >> 64) as u64);
    folded ^ (u64::from(std::process::id()) << 32) ^ counter
}

mod registry;
#[cfg(test)]
mod tests;

pub use self::registry::*;
