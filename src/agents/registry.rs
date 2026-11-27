//! Subagent registry, lifecycle slots, snapshots, and report formatting.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::mpsc::UnboundedSender;

use super::*;
use crate::llm::ChatMessage;
use crate::llm::classifier::ClassifierContext;
use crate::stream::{CancelToken, StreamEvent};

/// What a subagent thread reports on the dedicated agent channel — the
/// seventh `select!` source. Every event is tagged with its agent's id; the
/// loop folds it into the roster entry ([`AgentRun::apply`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    /// One event of the subagent's own stream (chunks, tool calls, usage —
    /// and, from a scripted run, its terminal done/error).
    Stream { id: String, event: StreamEvent },
    /// A live run's **terminal** event (`StreamDone`/`Error`), sent once the
    /// registry has recorded the outcome ([`AgentRegistry::settle`]) — at
    /// once, or by the `agentoutput` call that was waiting on the agent.
    /// `observed` when that call already reported the outcome to the lead, so
    /// no completion notice is owed (`docs/agents/agent-tools.md` *One notice per
    /// answer*).
    Settled {
        id: String,
        event: StreamEvent,
        observed: bool,
        /// The run that settled — what its notice is owed for
        /// ([`AgentRegistry::post_notice`]).
        run: u64,
    },
    /// The **lead** stopped this agent (`agentkill`): its loop is cancelled,
    /// and a cancelled loop sends nothing of its own, so this is the roster's
    /// only word of it.
    Stopped { id: String },
    /// The lead **resumed** this settled agent (`agentsend`): a continuation
    /// is starting, its first round announcing the message (`Steered`).
    /// Carries what the roster needs to bring back a row its sweep already
    /// collected.
    Resumed {
        id: String,
        description: String,
        agent_type: String,
        prompt: String,
    },
}

/// Where an agent stands, as the lead's companions report it
/// (`docs/agents/agent-tools.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    /// Its run (the launch or a continuation) is still going.
    Running,
    /// Finished — with its final response.
    Done(String),
    /// Its backend errored — with the error.
    Failed(String),
    /// Stopped: by the user's `x` (final), or by the lead's `agentkill`
    /// (resumable with `agentsend`).
    Stopped { by_user: bool },
}

/// One agent as the lead's companions see it — what the registry knows,
/// taken under its lock at one instant ([`AgentRegistry::snapshot`]), so a
/// report can be rendered from it without holding anything
/// ([`output_report`], [`list_report`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSnapshot {
    pub id: String,
    pub description: String,
    pub agent_type: String,
    pub state: AgentState,
    /// How long it has worked, over every run — the launch and each
    /// continuation — so the frame's two numbers both cover its whole life.
    pub runtime: Duration,
    /// How many tool calls it has started, over every run.
    pub tool_uses: usize,
    /// Each call's `Name(args)` one-liner, oldest first.
    pub calls: Vec<String>,
    /// Where each message it read arrived, and whether the user sent it, in
    /// order ([`AGENT_FOLLOW_UP_MARK`]).
    pub follow_ups: Vec<FollowUp>,
    /// What it said on its way, each where it said it, oldest first
    /// ([`AgentRegistry::record_update`]).
    pub updates: Vec<ProgressUpdate>,
    /// The run this snapshot describes — the one a report of it covers
    /// ([`AgentRegistry::report_settled`]).
    pub run: u64,
}

/// How far a waiting `agentoutput` has streamed an agent's timeline — how
/// many of its calls, the messages it read and its updates the cell has
/// shown ([`AgentRegistry::progress`], `docs/agents/agent-progress.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgressCursor {
    calls: usize,
    follow_ups: usize,
    updates: usize,
}

impl ProgressCursor {
    /// Past everything `snapshot` holds: what the report will list, never
    /// streamed as new.
    #[must_use]
    pub fn at(snapshot: &AgentSnapshot) -> Self {
        Self {
            calls: snapshot.calls.len(),
            follow_ups: snapshot.follow_ups.len(),
            updates: snapshot.updates.len(),
        }
    }
}

/// What one poll of a waited-on agent found ([`AgentRegistry::progress`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// What it did past the cursor, one line each, in the order it did it.
    pub lines: Vec<String>,
    /// Its run is still going.
    pub running: bool,
}

/// What stopping an agent found ([`AgentRegistry::kill`],
/// [`AgentRegistry::stop`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stop {
    /// Its run had not settled, so this stopped it.
    pub was_live: bool,
    /// An `agentoutput` call was waiting on it and will report the stop
    /// itself — no completion notice is owed (`docs/agents/agent-tools.md`).
    pub watched: bool,
    /// The run it found — the one a note of the stop is tagged with
    /// (`docs/agents/agent-tools.md` *A report covers one run*).
    pub run: u64,
}

/// What became of an `agentsend` ([`AgentRegistry::resume`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resume {
    /// The agent is running: the message waits on its queue for its next
    /// round boundary.
    Queued,
    /// The agent had settled: its conversation is handed back to continue
    /// from — the message already on its queue, so the continuation's first
    /// round announces it — and the slot is busy again under `cancel`. The
    /// caller spawns the run.
    Continue {
        messages: Vec<ChatMessage>,
        cancel: CancelToken,
        agent_type: String,
    },
    /// The user's `x` stopped it, and a model may not undo that.
    StoppedByUser,
    /// The lead stopped it and its thread has not let go yet — try again in
    /// a moment.
    Stopping,
    /// No such agent here (or no longer).
    Unknown,
}

/// A message waiting for an agent's next round boundary.
#[derive(Debug, Clone)]
struct PendingInput {
    text: String,
    /// The user typed it into the agent's session view.
    from_user: bool,
    /// A note of this agent's own shell's, routed here unread (see
    /// [`RoutedNote`]).
    routed: Option<RoutedNote>,
}

impl PendingInput {
    fn lead(text: &str) -> Self {
        Self {
            text: text.to_string(),
            from_user: false,
            routed: None,
        }
    }
}

/// The messages an agent's loop took off its queue at one round boundary,
/// waiting to be marked where its stream announces them
/// ([`AgentRegistry::announce_read`], `docs/agents/agent-progress.md`).
#[derive(Debug)]
struct Drained {
    /// The marks still to place — one for the lead's messages, then each
    /// message the user sent, quoted ([`FollowUp::from_user`]). The first
    /// announcement places them all.
    marks: Vec<Option<String>>,
    /// The announcements still to come, one per message taken.
    announcements: usize,
}

/// A shell's note routed to the agent that launched it
/// ([`AgentRegistry::route_shell_note`]) — what its own call into that
/// session takes back while the note is unread (`docs/tools/bash-tools.md` *One
/// notice per exit*): a look takes back a `… is waiting for input` note it
/// just answered, and a call into a session that ended reports the exit
/// instead of being told it was already reported.
#[derive(Debug, Clone)]
struct RoutedNote {
    /// Its number, from the board's own count — what the loop holds the
    /// notice cell by, so a note taken back costs its cell too.
    seq: u64,
    /// The session it reports.
    shell: String,
    /// For an exit, what a claim needs.
    exit: Option<crate::background::ClaimedExit>,
}

/// Where [`AgentRegistry::route_shell_note`] sent a shell's note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// Queued for the launching agent's next round boundary, as this note
    /// number — what the loop holds the notice cell by.
    Routed(u64),
    /// The launcher cannot hear it (settled, stopped): the shared board must
    /// take it, the note still owed.
    Unheard,
    /// A companion call already reported what the note says: none is owed.
    Covered,
}

/// One running subagent's shared slot: its cancel token, completion state,
/// stored conversation (for chat continuations), and pending chat inputs.
#[derive(Default)]
struct AgentSlot {
    cancel: CancelToken,
    /// The registered subagent type (`general-purpose` / `explore`) — a chat
    /// continuation rebuilds the same tool set from it.
    agent_type: String,
    /// The task label and the launch prompt — what `agentlist` names it by,
    /// and what the roster rebuilds a swept row from when the lead resumes it
    /// (`docs/agents/agent-tools.md`). Empty for an identity-free registration.
    description: String,
    prompt: String,
    /// The run (initial or a chat continuation) is still executing.
    busy: bool,
    /// The agent has settled at least once (a result/failure is stored).
    done: bool,
    /// It was stopped ([`AgentRegistry::kill`] or [`AgentRegistry::stop`]).
    killed: bool,
    /// …by the **user** — final, where the lead's `agentkill` is resumable.
    killed_by_user: bool,
    /// Each tool call it has started, as its `Name(args)` one-liner — the
    /// `agentoutput` summary ([`AgentRegistry::record_call`]).
    calls: Vec<String>,
    /// Where each message it read arrived, in calls made by then
    /// ([`AgentSnapshot::follow_ups`]).
    follow_ups: Vec<FollowUp>,
    /// How many of `follow_ups` came before the current run — the user's
    /// messages *this* run answered are the ones after it
    /// ([`AgentRegistry::user_messages`]).
    run_follow_ups: usize,
    /// The messages its loop has taken off the queue whose announcement the
    /// forwarder has not relayed yet, oldest first — marked once it has, so
    /// a mark is placed on the thread that records the calls and the
    /// updates ([`AgentRegistry::announce_read`]).
    drained: VecDeque<Drained>,
    /// What it said on its way, each where it said it — the report's
    /// `Agent: …` entries ([`AgentRegistry::record_update`]).
    updates: Vec<ProgressUpdate>,
    /// Which run is current: 0 for the launch, one more per continuation
    /// ([`restart`](Self::restart)) — what a settle, a report and a notice
    /// name, since a report covers one run (`docs/agents/agent-tools.md` *One
    /// notice per answer*).
    run: u64,
    /// When its current run started; `None` once settled, the run's length
    /// then added to `active`, the time its finished runs took.
    run_started: Option<Instant>,
    active: Duration,
    /// Its launch order — `agentlist` lists by it.
    launched: u64,
    /// Its settle order — retention evicts by it; `None` while running.
    settled: Option<u64>,
    /// How many `agentoutput` calls are waiting on it to settle.
    waiters: usize,
    /// The terminal event a settle **held** for those waiters to release
    /// ([`AgentRegistry::end_wait`]), and whether one of them reported the
    /// outcome it carries.
    held: Option<StreamEvent>,
    held_observed: bool,
    /// The runs whose terminal events went out **unobserved** and whose
    /// completion notices the loop has not posted yet — what a report of
    /// one of them cancels when it gets there first
    /// ([`AgentRegistry::report_settled`]). By run, since a report covers
    /// only the run it read.
    notices_owed: Vec<u64>,
    /// Its final response text (present once `done`, unless it failed).
    result: Option<String>,
    /// Its failure message (present once `done` when the run errored).
    failed: Option<String>,
    /// The full request message list at settle — a chat continuation resumes
    /// from here.
    messages: Option<Vec<ChatMessage>>,
    /// Messages awaiting delivery into the running loop (taken per round via
    /// the `pending_notices` seam), each remembering whether the user sent it.
    pending_inputs: Vec<PendingInput>,
    /// The auto-mode classifier's task context for **this agent's** run — its
    /// own launch prompt and its own executed calls, never the lead's
    /// (`docs/state/permissions.md`). It lives here rather than as a local in the
    /// agent's thread for one reason: the Ctrl+D classifier page reads it from
    /// the event loop while that agent's session view is open, and a page that
    /// showed the lead's log there would be describing decisions no one on
    /// screen made. Shared with the thread, which seeds it from the launch
    /// prompt and records one line per call; a chat continuation pushes onto
    /// the same rolling windows, because it is the same agent's conversation.
    classifier: Arc<Mutex<ClassifierContext>>,
}

impl AgentSlot {
    /// How long it has worked, over every run.
    fn runtime(&self) -> Duration {
        self.active
            + self
                .run_started
                .map_or(Duration::ZERO, |started| started.elapsed())
    }

    /// Freeze the runtime: the run is over, its length added to the rest.
    fn freeze_runtime(&mut self) {
        if let Some(started) = self.run_started.take() {
            self.active += started.elapsed();
        }
    }

    /// One message its loop read was announced on its stream: the first
    /// announcement of the oldest drain places that drain's marks after the
    /// calls recorded so far.
    fn announce_read(&mut self) {
        let at = self.calls.len();
        let Some(drained) = self.drained.front_mut() else {
            return;
        };
        self.follow_ups.extend(
            drained
                .marks
                .drain(..)
                .map(|from_user| FollowUp { at, from_user }),
        );
        drained.announcements = drained.announcements.saturating_sub(1);
        if drained.announcements == 0 {
            self.drained.pop_front();
        }
    }

    /// Mark every drain no announcement reached — the run is over, or
    /// starting again: a message the agent read is marked late rather than
    /// never.
    fn place_unannounced(&mut self) {
        let at = self.calls.len();
        for drained in self.drained.drain(..) {
            self.follow_ups.extend(
                drained
                    .marks
                    .into_iter()
                    .map(|from_user| FollowUp { at, from_user }),
            );
        }
    }

    /// Is its run still going — [`state`](Self::state)'s `Running`, read
    /// without copying an outcome?
    fn running(&self) -> bool {
        !self.killed && (self.busy || !self.done)
    }

    /// Where it stands — a stop outranks the outcome its thread reported
    /// late.
    fn state(&self) -> AgentState {
        if self.killed {
            return AgentState::Stopped {
                by_user: self.killed_by_user,
            };
        }
        if self.running() {
            return AgentState::Running;
        }
        match &self.failed {
            Some(error) => AgentState::Failed(error.clone()),
            None => AgentState::Done(self.result.clone().unwrap_or_default()),
        }
    }

    fn snapshot(&self, id: &str) -> AgentSnapshot {
        AgentSnapshot {
            id: id.to_string(),
            description: self.description.clone(),
            agent_type: self.agent_type.clone(),
            state: self.state(),
            runtime: self.runtime(),
            tool_uses: self.calls.len(),
            calls: self.calls.clone(),
            follow_ups: self.follow_ups.clone(),
            updates: self.updates.clone(),
            run: self.run,
        }
    }

    /// Begin a new run on this slot — a continuation: busy again under a
    /// fresh cancel token, the last outcome and any stop cleared, the clock
    /// restarted.
    fn restart(&mut self) -> CancelToken {
        let cancel = CancelToken::new();
        self.cancel = cancel.clone();
        self.busy = true;
        self.done = false;
        self.killed = false;
        self.killed_by_user = false;
        self.result = None;
        self.failed = None;
        self.run_started = Some(Instant::now());
        self.settled = None;
        self.place_unannounced();
        self.run_follow_ups = self.follow_ups.len();
        self.run += 1;
        cancel
    }

    /// May retention let it go? Settled, with nothing still owed on it — no
    /// waiter, no held event.
    fn evictable(&self) -> Option<u64> {
        (!self.busy && self.waiters == 0 && self.held.is_none())
            .then_some(self.settled)
            .flatten()
    }
}

struct Inner {
    /// The launch counter mixed into each id roll.
    next_id: u64,
    /// The order counter launches and settles are stamped from.
    seq: u64,
    slots: HashMap<String, AgentSlot>,
}

impl Inner {
    /// The next order stamp.
    fn stamp(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Let go of the oldest settled agents past [`AGENT_RETAINED_MAX`]
    /// (`docs/agents/agent-tools.md` *Retention*).
    fn evict_settled(&mut self) {
        let mut settled: Vec<(u64, String)> = self
            .slots
            .iter()
            .filter_map(|(id, slot)| slot.evictable().map(|seq| (seq, id.clone())))
            .collect();
        if settled.len() <= AGENT_RETAINED_MAX {
            return;
        }
        settled.sort_unstable();
        let excess = settled.len() - AGENT_RETAINED_MAX;
        for (_, id) in settled.into_iter().take(excess) {
            self.slots.remove(&id);
        }
    }
}

/// What [`AgentRegistry::post_notice`] did with one of an agent's
/// unobserved settles (`docs/agents/agent-tools.md` *One notice per answer*).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticePost {
    /// An `agentoutput` report already handed the lead this answer: nothing
    /// was posted, and no notice cell is owed.
    Covered,
    /// Owed: posted on the board as this
    /// [`seq`](crate::background::PendingNotice::seq) — `None` when the
    /// settle carried no notice to post.
    Posted(Option<u64>),
}

/// The shared subagent registry (see the module docs). Cloneable like the
/// background registry; the boundary (`main.rs`) creates it with the agent
/// channel's sender and hands clones to the backend.
#[derive(Clone)]
pub struct AgentRegistry {
    inner: Arc<Mutex<Inner>>,
    events: UnboundedSender<AgentEvent>,
}

impl std::fmt::Debug for AgentRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentRegistry").finish_non_exhaustive()
    }
}

impl AgentRegistry {
    /// A registry that reports subagent events on `events`.
    #[must_use]
    pub fn new(events: UnboundedSender<AgentEvent>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                next_id: 0,
                seq: 0,
                slots: HashMap::new(),
            })),
            events,
        }
    }

    /// Acquire the registry lock, recovering from poison so a panicked worker
    /// thread does not crash the event loop.
    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Register a fresh subagent of `agent_type` that names no task — a
    /// scripted launch, a test. See [`register_agent`](Self::register_agent).
    #[must_use]
    pub fn register(&self, agent_type: &str) -> (String, CancelToken) {
        self.register_agent(agent_type, "", "")
    }

    /// Register a fresh subagent of `agent_type`, launched to do
    /// `description` with `prompt`: allocate its id (collision re-roll) and
    /// its cancel token, and keep what it was launched as for the lead's
    /// companions (`docs/agents/agent-tools.md`). The caller spawns the thread.
    #[must_use]
    pub fn register_agent(
        &self,
        agent_type: &str,
        description: &str,
        prompt: &str,
    ) -> (String, CancelToken) {
        let mut inner = self.lock_inner();
        let id = loop {
            let seed = entropy_seed(inner.next_id);
            inner.next_id = inner.next_id.wrapping_add(1);
            let id = agent_id(seed);
            if !inner.slots.contains_key(&id) {
                break id;
            }
        };
        let cancel = CancelToken::new();
        let launched = inner.stamp();
        inner.slots.insert(
            id.clone(),
            AgentSlot {
                classifier: Arc::new(Mutex::new(ClassifierContext::new())),
                cancel: cancel.clone(),
                busy: true,
                agent_type: agent_type.to_string(),
                description: description.to_string(),
                prompt: prompt.to_string(),
                run_started: Some(Instant::now()),
                launched,
                ..AgentSlot::default()
            },
        );
        inner.evict_settled();
        (id, cancel)
    }

    /// The task `id` was launched to do — what a companion's cell header
    /// names the agent by (`tools::summarize_call_naming`). `None` for an id
    /// nothing answers to, or one registered without a task.
    #[must_use]
    pub fn description(&self, id: &str) -> Option<String> {
        let inner = self.lock_inner();
        inner
            .slots
            .get(id)
            .map(|slot| slot.description.clone())
            .filter(|description| !description.is_empty())
    }

    /// Record a tool call `id` started, as its `Name(args)` one-liner — the
    /// forwarder's, at each `ToolStart`, so `agentoutput` can summarize what
    /// the agent has been doing (`docs/agents/agent-tools.md`).
    pub fn record_call(&self, id: &str, header: &str) {
        let mut inner = self.lock_inner();
        if let Some(slot) = inner.slots.get_mut(id) {
            slot.calls.push(header.to_string());
        }
    }

    /// Record a run of text `id` said before doing more — a progress update
    /// — where it said it: after the calls and the messages recorded so far
    /// (`docs/agents/agent-progress.md`). Trimmed and cut at
    /// [`AGENT_REPORT_UPDATE_CHARS`]; a blank run is no update. The
    /// forwarder's, through [`RunRecorder`].
    pub fn record_update(&self, id: &str, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let text = cut_chars(text, AGENT_REPORT_UPDATE_CHARS);
        let mut inner = self.lock_inner();
        if let Some(slot) = inner.slots.get_mut(id) {
            let (at, read) = (slot.calls.len(), slot.follow_ups.len());
            slot.updates.push(ProgressUpdate { at, read, text });
        }
    }

    /// What `id` did past `cursor` — each call, message and update recorded
    /// since, one line each, in the order it did them — and whether its run
    /// is still going, the cursor moved past what is returned. A waiting
    /// `agentoutput` polls this every few milliseconds, so it reads the slot
    /// under the lock and copies only what is new, never the agent's whole
    /// history as a [`snapshot`](Self::snapshot) would. `None` for an id
    /// nothing answers to.
    #[must_use]
    pub fn progress(&self, id: &str, cursor: &mut ProgressCursor) -> Option<Progress> {
        let inner = self.lock_inner();
        let slot = inner.slots.get(id)?;
        let lines = timeline(&slot.calls, &slot.follow_ups, &slot.updates, *cursor)
            .iter()
            .map(stream_line)
            .collect();
        *cursor = ProgressCursor {
            calls: slot.calls.len(),
            follow_ups: slot.follow_ups.len(),
            updates: slot.updates.len(),
        };
        Some(Progress {
            lines,
            running: slot.running(),
        })
    }

    /// `id` as the lead's companions see it, taken at one instant.
    #[must_use]
    pub fn snapshot(&self, id: &str) -> Option<AgentSnapshot> {
        let inner = self.lock_inner();
        inner.slots.get(id).map(|slot| slot.snapshot(id))
    }

    /// Every agent the registry holds — the running ones and the retained
    /// settled ones — in launch order (`agentlist`).
    #[must_use]
    pub fn snapshots(&self) -> Vec<AgentSnapshot> {
        let inner = self.lock_inner();
        let mut slots: Vec<(&String, &AgentSlot)> = inner.slots.iter().collect();
        slots.sort_unstable_by_key(|(_, slot)| slot.launched);
        slots
            .into_iter()
            .map(|(id, slot)| slot.snapshot(id))
            .collect()
    }

    /// An `agentoutput` call starts waiting on `id` to settle: a settle that
    /// lands meanwhile holds its terminal event for the wait to release
    /// ([`end_wait`](Self::end_wait)). `false` for an id nothing answers to.
    #[must_use]
    pub fn begin_wait(&self, id: &str) -> bool {
        let mut inner = self.lock_inner();
        let Some(slot) = inner.slots.get_mut(id) else {
            return false;
        };
        slot.waiters += 1;
        true
    }

    /// The wait on `id` is over: read where the agent stands **and**, in the
    /// same locked step, release a terminal event its settle held — marked
    /// observed when this wait reports the settled outcome (`reporting`: the
    /// call will hand the lead what it read; a cancelled one won't). Taking
    /// both at one instant is the point: a settle the wait did not see
    /// cannot be marked observed, and one it did cannot be noticed twice
    /// (`docs/agents/agent-tools.md` *One notice per answer*).
    pub fn end_wait(&self, id: &str, reporting: bool) -> Option<AgentSnapshot> {
        let mut inner = self.lock_inner();
        let slot = inner.slots.get_mut(id)?;
        slot.waiters = slot.waiters.saturating_sub(1);
        let snapshot = slot.snapshot(id);
        // A held event is a settled run's (a run that starts over releases it
        // first), so a reporting wait is reporting exactly that outcome.
        if reporting && slot.held.is_some() {
            slot.held_observed = true;
        }
        if slot.waiters == 0
            && let Some(event) = slot.held.take()
        {
            let observed = std::mem::take(&mut slot.held_observed);
            if !observed {
                slot.notices_owed.push(slot.run);
            }
            let _ = self.events.send(AgentEvent::Settled {
                id: id.to_string(),
                event,
                observed,
                run: slot.run,
            });
        }
        Some(snapshot)
    }

    /// Release a terminal event still held for a waiter, unobserved — a run
    /// is about to start over, and its events must never overtake the last
    /// run's end on the channel.
    fn release_held(&self, id: &str, slot: &mut AgentSlot) {
        if let Some(event) = slot.held.take() {
            slot.held_observed = false;
            slot.notices_owed.push(slot.run);
            let _ = self.events.send(AgentEvent::Settled {
                id: id.to_string(),
                event,
                observed: false,
                run: slot.run,
            });
        }
    }

    /// The shared handle to `id`'s classifier context — for the agent's own
    /// thread, which seeds it from the launch prompt and records one line per
    /// executed call. `None` once the slot is gone.
    #[must_use]
    pub fn classifier_handle(&self, id: &str) -> Option<Arc<Mutex<ClassifierContext>>> {
        let inner = self.lock_inner();
        inner.slots.get(id).map(|slot| Arc::clone(&slot.classifier))
    }

    /// `id`'s classifier context, rendered — exactly what its own auto-mode
    /// verdicts are reviewed against. This is what the Ctrl+D classifier page
    /// shows while that agent's session view is open; `None` for an agent the
    /// registry no longer holds, which the page renders as its empty
    /// placeholder rather than falling back to the lead's log
    /// (`docs/state/permissions.md`).
    #[must_use]
    pub fn classifier_context(&self, id: &str) -> Option<String> {
        // Take the handle first, so the registry lock is released before the
        // context's own is acquired: the draw tick calls this every frame the
        // classifier page is open, and it must never queue behind the whole
        // map while an agent thread records a call.
        let handle = self.classifier_handle(id)?;
        handle.lock().ok().map(|log| log.render())
    }

    /// The registered subagent type (for a chat continuation's tool set).
    #[must_use]
    pub fn agent_type(&self, id: &str) -> String {
        let inner = self.lock_inner();
        inner
            .slots
            .get(id)
            .map(|slot| slot.agent_type.clone())
            .unwrap_or_else(|| GENERAL_PURPOSE.to_string())
    }

    /// Send one subagent event to the loop (used by the forwarder threads).
    pub fn send(&self, event: AgentEvent) {
        let _ = self.events.send(event);
    }

    /// A run finished with no end to deliver: [`settle`](Self::settle) with
    /// none. A run that ended on its own sends its end through `settle`,
    /// which names the run; sent ahead of it, the end names none
    /// (`docs/agents/agent-tools.md` *A report covers one run*).
    pub fn finish(&self, id: &str, outcome: Result<String, String>, messages: Vec<ChatMessage>) {
        self.settle(id, outcome, messages, None);
    }

    /// A run finished: store its outcome + final message list so the parent's
    /// wait loop resolves and a chat continuation can resume — and deliver
    /// its `terminal` event (`docs/agents/agent-tools.md` *One notice per answer*):
    /// sent here, **under the lock**, so whoever sees the agent done (the
    /// foreground group's wait loop) knows the event is already queued; or,
    /// with an `agentoutput` waiting, held for that wait to release once it
    /// has read the outcome. A slot already killed keeps its killed outcome
    /// (the thread noticed late).
    pub fn settle(
        &self,
        id: &str,
        outcome: Result<String, String>,
        messages: Vec<ChatMessage>,
        terminal: Option<StreamEvent>,
    ) {
        let mut inner = self.lock_inner();
        let seq = inner.stamp();
        if let Some(slot) = inner.slots.get_mut(id) {
            slot.place_unannounced();
            slot.busy = false;
            slot.done = true;
            slot.messages = Some(messages);
            slot.freeze_runtime();
            slot.settled = Some(seq);
            if !slot.killed {
                match outcome {
                    Ok(result) => slot.result = Some(result),
                    Err(error) => slot.failed = Some(error),
                }
            }
            if let Some(event) = terminal {
                if slot.waiters > 0 {
                    slot.held = Some(event);
                } else {
                    slot.notices_owed.push(slot.run);
                    let _ = self.events.send(AgentEvent::Settled {
                        id: id.to_string(),
                        event,
                        observed: false,
                        run: slot.run,
                    });
                }
            }
        }
        inner.evict_settled();
    }

    /// The **user** stops one agent (the roster's `x`, Esc on its foreground
    /// group): cancel its token and mark it killed **and settled**, so the
    /// parent's wait loop resolves at once instead of waiting for the thread
    /// to notice (a blocking read can take seconds). Final: the lead may not
    /// resume it. Reports whether the agent was live, and whether a waiting
    /// `agentoutput` will report the stop — read in the same locked step, so
    /// the notice the stop would owe is never owed twice.
    pub fn kill(&self, id: &str) -> Stop {
        let mut inner = self.lock_inner();
        let seq = inner.stamp();
        let Some(slot) = inner.slots.get_mut(id) else {
            return Stop::default();
        };
        slot.cancel.cancel();
        // Nothing will read them now, and the roster drops their rows.
        slot.pending_inputs.clear();
        let was_live = !slot.done;
        if was_live {
            slot.freeze_runtime();
            slot.settled = Some(seq);
        }
        slot.killed = true;
        slot.killed_by_user = true;
        slot.done = true;
        Stop {
            was_live,
            watched: was_live && slot.waiters > 0,
            run: slot.run,
        }
    }

    /// The **lead** stops one agent (`agentkill`): cancelled and settled like
    /// the user's [`kill`](Self::kill), but its conversation stays resumable,
    /// and — since a cancelled loop sends nothing of its own — the loop is
    /// told ([`AgentEvent::Stopped`]). An agent already settled is left as it
    /// was (`was_live: false`). `None` for an id nothing answers to.
    ///
    /// Either way the lead's result says the run is over — the stop's report,
    /// or that nothing was running — so the note that the user started the
    /// run, still on `board` unread, would contradict it afterwards: it says
    /// the agent runs and that a notice will come. It is taken back in the
    /// same locked step (`docs/agents/agent-tools.md` *A resume is news when it
    /// starts*); a run that ended on its own still sends its completion.
    pub fn stop(
        &self,
        id: &str,
        board: Option<&crate::background::BackgroundRegistry>,
    ) -> Option<Stop> {
        let mut inner = self.lock_inner();
        let seq = inner.stamp();
        let slot = inner.slots.get_mut(id)?;
        if let Some(board) = board {
            board.retract_agent_resume_notice(id);
        }
        if slot.done {
            return Some(Stop {
                run: slot.run,
                ..Stop::default()
            });
        }
        slot.cancel.cancel();
        // The roster drops their rows: a resume delivers only its own message.
        slot.pending_inputs.clear();
        slot.freeze_runtime();
        slot.settled = Some(seq);
        slot.killed = true;
        slot.killed_by_user = false;
        slot.done = true;
        let _ = self.events.send(AgentEvent::Stopped { id: id.to_string() });
        Some(Stop {
            was_live: true,
            watched: false,
            run: slot.run,
        })
    }

    /// Stop every agent (quit).
    pub fn kill_all(&self) {
        let mut inner = self.lock_inner();
        for slot in inner.slots.values_mut() {
            slot.cancel.cancel();
            slot.killed = true;
            slot.killed_by_user = true;
            slot.done = true;
        }
    }

    /// Stop every agent **and forget them** — `/clear`: the conversation that
    /// knew their ids is gone, so nothing may list or resume them
    /// (`docs/agents/agent-tools.md` *Retention*).
    pub fn clear(&self) {
        self.kill_all();
        let mut inner = self.lock_inner();
        inner.slots.clear();
    }

    /// Deliver an `agentsend` (`docs/agents/agent-tools.md`): onto a running
    /// agent's queue for its next round boundary, or — for a settled one —
    /// as the start of a **continuation** of its stored conversation, the
    /// message queued for that run's first round to announce. The roster is
    /// told of a continuation before it starts ([`AgentEvent::Resumed`]). A
    /// stop by the user is final; a stop by the lead resumes once its thread
    /// has let go.
    pub fn resume(&self, id: &str, text: &str) -> Resume {
        let mut inner = self.lock_inner();
        let Some(slot) = inner.slots.get_mut(id) else {
            return Resume::Unknown;
        };
        if slot.killed_by_user {
            return Resume::StoppedByUser;
        }
        if slot.busy {
            if slot.killed {
                return Resume::Stopping;
            }
            slot.pending_inputs.push(PendingInput::lead(text));
            return Resume::Queued;
        }
        let Some(messages) = slot.messages.take() else {
            return Resume::Unknown;
        };
        self.release_held(id, slot);
        slot.pending_inputs.push(PendingInput::lead(text));
        let cancel = slot.restart();
        let _ = self.events.send(AgentEvent::Resumed {
            id: id.to_string(),
            description: slot.description.clone(),
            agent_type: slot.agent_type.clone(),
            prompt: slot.prompt.clone(),
        });
        Resume::Continue {
            messages,
            cancel,
            agent_type: slot.agent_type.clone(),
        }
    }

    /// Drop an agent's slot entirely (its roster entry was swept).
    pub fn remove(&self, id: &str) {
        let mut inner = self.lock_inner();
        inner.slots.remove(id);
    }

    /// Has this agent's current run settled (finished, failed, or killed)?
    #[must_use]
    pub fn is_done(&self, id: &str) -> bool {
        let inner = self.lock_inner();
        inner.slots.get(id).is_none_or(|slot| slot.done)
    }

    /// Was this agent stopped by the user?
    #[must_use]
    pub fn is_killed(&self, id: &str) -> bool {
        let inner = self.lock_inner();
        inner.slots.get(id).is_some_and(|slot| slot.killed)
    }

    /// The settled outcome for the parent's tool result: `Ok(final response)`
    /// / `Err(error)` — `Err("stopped by the user")` for a kill, `None` while
    /// still running.
    #[must_use]
    pub fn outcome(&self, id: &str) -> Option<Result<String, String>> {
        let inner = self.lock_inner();
        let slot = inner.slots.get(id)?;
        if !slot.done {
            return None;
        }
        if slot.killed {
            return Some(Err("stopped by the user".to_string()));
        }
        if let Some(error) = &slot.failed {
            return Some(Err(error.clone()));
        }
        Some(Ok(slot.result.clone().unwrap_or_default()))
    }

    /// Queue a message for a **running** agent — delivered into its loop at
    /// the next round boundary (the `pending_notices` seam). Returns `false`
    /// when the agent isn't currently running a turn (the caller should spawn
    /// a continuation instead). The lead's and the system's door: a
    /// background shell's completion routed to the agent that launched it.
    /// A message the user typed takes
    /// [`queue_user_input`](Self::queue_user_input).
    #[must_use]
    pub fn queue_input(&self, id: &str, text: &str) -> bool {
        self.queue(id, PendingInput::lead(text))
    }

    /// [`queue_input`](Self::queue_input) for a message the **user** typed
    /// into the agent's session view — remembered as theirs, so the lead is
    /// told who wrote it (`docs/agents/agent-tools.md`).
    #[must_use]
    pub fn queue_user_input(&self, id: &str, text: &str) -> bool {
        self.queue(
            id,
            PendingInput {
                text: text.to_string(),
                from_user: true,
                routed: None,
            },
        )
    }

    /// Route the note of shell `shell` — one this agent launched, which
    /// exited or stopped to ask for input — onto the agent's queue
    /// (`docs/agents/agent-tool.md`), asking the board under this registry's lock
    /// whether it is still owed ([`BackgroundRegistry::take_shell_notice`]):
    /// a companion call can never claim the exit in between, and an owed
    /// exit's claim details travel with the note so the agent's own call can
    /// claim it from here ([`claim_routed_exit`](Self::claim_routed_exit)).
    ///
    /// [`BackgroundRegistry::take_shell_notice`]: crate::background::BackgroundRegistry::take_shell_notice
    pub fn route_shell_note(
        &self,
        id: &str,
        shell: &str,
        text: &str,
        waiting: bool,
        board: &crate::background::BackgroundRegistry,
    ) -> Route {
        let mut inner = self.lock_inner();
        let Some(slot) = inner
            .slots
            .get_mut(id)
            .filter(|slot| slot.busy && !slot.killed)
        else {
            return Route::Unheard;
        };
        match board.take_shell_notice(shell, waiting) {
            crate::background::ShellNotice::Covered => Route::Covered,
            crate::background::ShellNotice::Owed { seq, exit } => {
                slot.pending_inputs.push(PendingInput {
                    text: text.to_string(),
                    from_user: false,
                    routed: Some(RoutedNote {
                        seq,
                        shell: shell.to_string(),
                        exit,
                    }),
                });
                Route::Routed(seq)
            }
        }
    }

    /// Agent `id`'s own call named shell `shell`, which exited with its note
    /// still in the agent's queue, unread: take the note back and hand over
    /// what the call's report of the exit needs, with the note's number.
    /// `None` once the agent's loop has read it — then it was reported.
    pub fn claim_routed_exit(
        &self,
        id: &str,
        shell: &str,
    ) -> Option<(crate::background::ClaimedExit, u64)> {
        let mut inner = self.lock_inner();
        let pending = &mut inner.slots.get_mut(id)?.pending_inputs;
        let index = pending.iter().position(|input| {
            input
                .routed
                .as_ref()
                .is_some_and(|note| note.shell == shell && note.exit.is_some())
        })?;
        let note = pending.remove(index).routed?;
        Some((note.exit?, note.seq))
    }

    /// Agent `id`'s own call just looked at shell `shell`: take back the
    /// shell's notes still unread in its queue — a `… is waiting for input`
    /// note says nothing the look did not show, and would reach the agent
    /// only after its answer. Their numbers, so the cells held for them go
    /// too.
    pub fn withdraw_routed_notes(&self, id: &str, shell: &str) -> Vec<u64> {
        let mut inner = self.lock_inner();
        let Some(slot) = inner.slots.get_mut(id) else {
            return Vec::new();
        };
        let mut seqs = Vec::new();
        slot.pending_inputs.retain(|input| match &input.routed {
            Some(note) if note.shell == shell => {
                seqs.push(note.seq);
                false
            }
            _ => true,
        });
        seqs
    }

    fn queue(&self, id: &str, input: PendingInput) -> bool {
        let mut inner = self.lock_inner();
        match inner.slots.get_mut(id) {
            Some(slot) if slot.busy && !slot.killed => {
                slot.pending_inputs.push(input);
                true
            }
            _ => false,
        }
    }

    /// Drain the pending chat inputs for one agent (its loop calls this per
    /// round).
    #[must_use]
    pub fn take_pending_inputs(&self, id: &str) -> Vec<String> {
        let mut inner = self.lock_inner();
        let Some(slot) = inner.slots.get_mut(id) else {
            return Vec::new();
        };
        let taken = std::mem::take(&mut slot.pending_inputs);
        // Read now, at this round boundary — one mark for the lead's
        // messages, one per message the user sent, each quoted
        // (docs/agents/agent-tools.md). Placed where the stream announces them, not
        // here: this is the agent's own thread, and the forwarder recording
        // its calls can still be relaying the round before
        // (docs/agents/agent-progress.md).
        if !taken.is_empty() {
            let lead = taken.iter().any(|input| !input.from_user).then_some(None);
            let users = taken
                .iter()
                .filter(|input| input.from_user)
                .map(|input| Some(input.text.clone()));
            slot.drained.push_back(Drained {
                marks: lead.into_iter().chain(users).collect(),
                announcements: taken.len(),
            });
        }
        taken.into_iter().map(|input| input.text).collect()
    }

    /// The forwarder relayed `id`'s announcement of one message its loop read
    /// (`StreamEvent::Steered`): the first announcement of a drain places
    /// that drain's marks after the calls relayed before it. The forwarder
    /// records the calls and the updates too, so the three land in the order
    /// the stream carried them, however far behind the loop it runs
    /// (`docs/agents/agent-progress.md`).
    pub fn announce_read(&self, id: &str) {
        let mut inner = self.lock_inner();
        if let Some(slot) = inner.slots.get_mut(id) {
            slot.announce_read();
        }
    }

    /// Mark that `id` read a message the user sent **outside** its queue — a
    /// session-view chat that continues it with the message already in its
    /// conversation (`spawn_agent_chat`); a queued one is marked where its
    /// stream announces it ([`announce_read`](Self::announce_read)).
    pub fn note_user_message(&self, id: &str, text: &str) {
        let mut inner = self.lock_inner();
        if let Some(slot) = inner.slots.get_mut(id) {
            let at = slot.calls.len();
            slot.follow_ups.push(FollowUp {
                at,
                from_user: Some(text.to_string()),
            });
        }
    }

    /// The messages the user sent `id` that its **current run** has read —
    /// what the run's answer is told it answered (the foreground result's
    /// note, `user_messages_note`). Empty for an unknown id.
    #[must_use]
    pub fn user_messages(&self, id: &str) -> Vec<String> {
        let inner = self.lock_inner();
        inner.slots.get(id).map_or_else(Vec::new, |slot| {
            let placed = slot.follow_ups[slot.run_follow_ups.min(slot.follow_ups.len())..]
                .iter()
                .filter_map(|follow_up| follow_up.from_user.clone());
            // Read already, though not marked yet: a stop can be read before
            // the forwarder relays the announcement.
            let unannounced = slot
                .drained
                .iter()
                .flat_map(|drained| drained.marks.iter().flatten().cloned());
            placed.chain(unannounced).collect()
        })
    }

    /// Does `id` still hold a message nobody has delivered?
    ///
    /// The **settle guard** (`docs/input/queue.md`): a slot stays `busy` between its
    /// loop's last drain and [`finish`](Self::finish), so a message typed in
    /// that window is accepted by [`queue_input`](Self::queue_input) and then
    /// read by nobody — the loop is over, and the roster's terminal event may
    /// already have been folded, so the boundary's own reconciliation has been
    /// and gone. The run's thread asks this before exiting and continues
    /// itself when the answer is yes.
    #[must_use]
    pub fn has_pending_inputs(&self, id: &str) -> bool {
        let inner = self.lock_inner();
        inner
            .slots
            .get(id)
            .is_some_and(|slot| !slot.pending_inputs.is_empty())
    }

    /// Take the **last** message queued into this agent's loop back — Alt+Up's
    /// pull into the composer inside its session view, and
    /// [`SteerQueue::take_last`](crate::steer::SteerQueue::take_last)'s twin.
    /// `None` once the round boundary has drained it, which is the only
    /// honest answer: a message the loop has read is in its context now.
    /// Only the **user's** own: a message the lead queued (`agentsend`, a
    /// routed shell note) has no row and no composer to come back to, so
    /// taking it would drop it silently.
    pub fn take_last_input(&self, id: &str) -> Option<String> {
        let mut inner = self.lock_inner();
        let pending = &mut inner.slots.get_mut(id)?.pending_inputs;
        let index = pending.iter().rposition(|input| input.from_user)?;
        Some(pending.remove(index).text)
    }

    /// An `agentoutput` call handed the lead the outcome of `id`'s run `run`
    /// (`docs/agents/agent-tools.md` *One notice per answer*): the completion
    /// notice saying the same must not reach it too. Not posted yet — the
    /// loop's [`post_notice`](Self::post_notice) is told to skip it; posted
    /// and still on `board` — taken back, the board remembering its number
    /// so the cell the loop holds for it is dropped too
    /// ([`BackgroundRegistry::take_retracted`]), as is any other note of the
    /// run's end posted meanwhile (the user's stop); already taken by the
    /// lead — nothing to do, this was a re-read. Under the registry lock, as
    /// the loop's post is, so the two can never cross.
    ///
    /// Only that run's: an earlier run's completion still unread on `board`
    /// says what the report does not — the user's message can start the
    /// next run the instant one ends, and a wait then reports the run after
    /// — so it is taken off the board and **returned**, oldest first, for
    /// the report to carry rather than lost or delivered after the newer
    /// answer. (One the loop has not posted yet stays owed, and reaches the
    /// lead as a notice.)
    ///
    /// The note that the user started the run, still unread, goes too —
    /// whichever way the completion stood — since read after the answer it
    /// would say the agent runs. Not when the agent has started over since
    /// the report read it: that note is about a run the lead has not heard
    /// of (`docs/agents/agent-tools.md` *A resume is news when it starts*).
    ///
    /// [`BackgroundRegistry::take_retracted`]: crate::background::BackgroundRegistry::take_retracted
    #[must_use]
    pub fn report_settled(
        &self,
        id: &str,
        run: u64,
        board: Option<&crate::background::BackgroundRegistry>,
    ) -> Vec<String> {
        let mut inner = self.lock_inner();
        let Some(slot) = inner.slots.get_mut(id) else {
            return Vec::new();
        };
        if slot.done
            && let Some(board) = board
        {
            board.retract_agent_resume_notice(id);
        }
        if let Some(at) = slot.notices_owed.iter().position(|owed| *owed == run) {
            slot.notices_owed.remove(at);
        }
        let Some(board) = board else {
            return Vec::new();
        };
        board.retract_agent_notice(id, run);
        board.take_earlier_agent_notices(id, run)
    }

    /// The loop handled the **unobserved** settle of `id`'s run `run`: post
    /// its completion notice on `board` when it owes one (`context`), tagged
    /// with the run — unless a report already handed the lead that answer
    /// ([`report_settled`](Self::report_settled)). Called for every such
    /// settle, a notice owed or not, so each one is accounted for once.
    /// [`NoticePost::Covered`] when a report covered it: no notice cell is
    /// owed either.
    #[must_use]
    pub fn post_notice(
        &self,
        id: &str,
        run: u64,
        board: &crate::background::BackgroundRegistry,
        context: Option<String>,
    ) -> NoticePost {
        let mut inner = self.lock_inner();
        if let Some(slot) = inner.slots.get_mut(id) {
            let Some(at) = slot.notices_owed.iter().position(|owed| *owed == run) else {
                return NoticePost::Covered;
            };
            slot.notices_owed.remove(at);
        }
        NoticePost::Posted(context.map(|context| board.post_agent_notice(context, id, Some(run))))
    }

    /// Post the note that the user started `id` again (`context`) on `board`
    /// — only while that run is going, and under the registry lock, where a
    /// report of the run's end and the lead's stop take the note back too
    /// ([`report_settled`](Self::report_settled), [`stop`](Self::stop)), so
    /// the three never cross (`docs/agents/agent-tools.md` *A resume is news when it
    /// starts*). A run already over owes no such note: its completion notice
    /// names the user's messages as well, and the report of a stop the lead
    /// made first marks them. The note's board number, or `None` when nothing
    /// was posted.
    #[must_use]
    pub fn post_resume_notice(
        &self,
        id: &str,
        board: &crate::background::BackgroundRegistry,
        context: String,
    ) -> Option<u64> {
        let inner = self.lock_inner();
        let slot = inner.slots.get(id)?;
        (!slot.done).then(|| board.post_agent_resume_notice(context, id, slot.run))
    }

    /// Can this agent take a **new turn** right now — settled, not stopped,
    /// and with a stored conversation a continuation can resume from? The
    /// question the Tab follow-up queue's dispatch asks before it hands a
    /// message over (`docs/input/queue.md`): the registry is the only honest
    /// answer, exactly as it is for [`spawn_agent_chat`], and asking it is
    /// what keeps a follow-up its **own** turn instead of a steer folded into
    /// a run that is still going.
    ///
    /// [`spawn_agent_chat`]: crate::stream::ReplySource::spawn_agent_chat
    #[must_use]
    pub fn ready_for_turn(&self, id: &str) -> bool {
        let inner = self.lock_inner();
        inner
            .slots
            .get(id)
            .is_some_and(|slot| !slot.busy && !slot.killed && slot.messages.is_some())
    }

    /// Begin a chat continuation on a settled agent: reclaim its stored
    /// message list, mark it busy again with a fresh cancel token, and return
    /// what the continuation thread needs. `None` while it is still busy or
    /// no conversation is stored.
    #[must_use]
    pub fn begin_continuation(&self, id: &str) -> Option<(Vec<ChatMessage>, CancelToken)> {
        let mut inner = self.lock_inner();
        let slot = inner.slots.get_mut(id)?;
        if slot.busy {
            return None;
        }
        let messages = slot.messages.take()?;
        // The last run's end goes out ahead of anything this one sends.
        self.release_held(id, slot);
        Some((messages, slot.restart()))
    }
}

/// `1 tool use`, `3 tool uses`.
fn tool_uses_label(count: usize) -> String {
    match count {
        1 => "1 tool use".to_string(),
        n => format!("{n} tool uses"),
    }
}

impl AgentSnapshot {
    /// The report's frame line — where the agent stands, over everything
    /// else a report says: `Running (agent a7k2m9x4q · Fetch GitHub profile ·
    /// 42s · 3 tool uses)`.
    #[must_use]
    pub fn frame(&self) -> String {
        let state = match self.state {
            AgentState::Running => "Running",
            AgentState::Done(_) => "Done",
            AgentState::Failed(_) => "Failed",
            AgentState::Stopped { .. } => "Stopped",
        };
        let task = if self.description.is_empty() {
            String::new()
        } else {
            format!(" · {}", self.description)
        };
        format!(
            "{state} (agent {}{task} · {} · {})",
            self.id,
            crate::app::format_elapsed(self.runtime.as_secs()),
            tool_uses_label(self.tool_uses)
        )
    }
}

/// One reported call, on one line and cut at [`AGENT_REPORT_CALL_CHARS`] —
/// as the report lists it, and as a waiting `agentoutput` streams it.
#[must_use]
pub fn report_call(header: &str) -> String {
    cut_chars(
        &crate::llm::tools::flatten_one_line(header),
        AGENT_REPORT_CALL_CHARS,
    )
}

/// `text` kept to `max` characters, a cut marked `…`.
fn cut_chars(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}

/// One progress update as a report lists it: [`AGENT_UPDATE_LABEL`] over its
/// first line and the rest indented beneath it — the user-messages note's
/// shape — its blank lines dropped, since a blank line in a report is a
/// section break (`docs/agents/agent-progress.md`).
fn report_update(text: &str) -> Vec<String> {
    let mut rows = text
        .lines()
        .map(str::trim_end)
        .filter(|row| !row.is_empty());
    let mut lines = vec![format!(
        "{AGENT_UPDATE_LABEL}: {}",
        rows.next().unwrap_or_default()
    )];
    lines.extend(rows.map(|row| format!("  {row}")));
    lines
}

/// One thing an agent did, as its timeline lists it
/// ([`AgentSnapshot::timeline`]).
enum TimelineEntry<'a> {
    /// Its `index`th tool call, as its `Name(args)` one-liner.
    Call { index: usize, header: &'a str },
    /// A message it read.
    FollowUp(&'a FollowUp),
    /// A progress update it gave.
    Update(&'a ProgressUpdate),
}

/// What an agent did past `from`, in the order it did it: each call, each
/// message it read, each update it gave. A message or an update recorded
/// after `n` calls falls before call `n`; among those, an update follows the
/// messages it had read. The forwarder records the calls and the updates as
/// it relays the stream, and places a message's mark where the stream
/// announces it ([`AgentRegistry::announce_read`]), so the counts each is
/// keyed by follow the stream's own order (`docs/agents/agent-progress.md`). Every
/// entry recorded later keys after every one recorded earlier, so the
/// entries past a cursor are the timeline's own tail.
fn timeline<'a>(
    calls: &'a [String],
    follow_ups: &'a [FollowUp],
    updates: &'a [ProgressUpdate],
    from: ProgressCursor,
) -> Vec<TimelineEntry<'a>> {
    // (after how many calls, after how many messages, which goes first)
    let mut keyed: Vec<((usize, usize, u8), TimelineEntry<'a>)> = Vec::new();
    keyed.extend(
        updates
            .iter()
            .skip(from.updates)
            .map(|update| ((update.at, update.read, 0), TimelineEntry::Update(update))),
    );
    keyed.extend(
        follow_ups
            .iter()
            .enumerate()
            .skip(from.follow_ups)
            .map(|(read, follow_up)| ((follow_up.at, read, 1), TimelineEntry::FollowUp(follow_up))),
    );
    keyed.extend(
        calls
            .iter()
            .enumerate()
            .skip(from.calls)
            .map(|(index, header)| {
                (
                    (index, usize::MAX, 2),
                    TimelineEntry::Call { index, header },
                )
            }),
    );
    // Stable: two updates between the same marks keep their order.
    keyed.sort_by_key(|(key, _)| *key);
    keyed.into_iter().map(|(_, entry)| entry).collect()
}

impl AgentSnapshot {
    /// Everything the agent did, in the order it did it ([`timeline`]).
    fn timeline(&self) -> Vec<TimelineEntry<'_>> {
        timeline(
            &self.calls,
            &self.follow_ups,
            &self.updates,
            ProgressCursor::default(),
        )
    }
}

/// One timeline entry as a waiting `agentoutput` streams it, on one line: a
/// call or a message as the report lists it, an update under
/// [`AGENT_UPDATE_LABEL`] cut like a call. The running cell's tail is a few
/// rows tall, and a whole update there would push what surrounds it out of
/// view; the report lists it whole (`docs/agents/agent-progress.md`).
fn stream_line(entry: &TimelineEntry<'_>) -> String {
    match entry {
        TimelineEntry::Call { header, .. } => report_call(header),
        TimelineEntry::FollowUp(follow_up) => follow_up_mark(follow_up),
        TimelineEntry::Update(update) => {
            report_call(&format!("{AGENT_UPDATE_LABEL}: {}", update.text))
        }
    }
}

/// The report line for one delivered message: the bare
/// [`AGENT_FOLLOW_UP_MARK`] for the lead's own, the user's quoted on one
/// line — the lead never saw it, so naming that it arrived is not enough.
fn follow_up_mark(follow_up: &FollowUp) -> String {
    follow_up.from_user.as_deref().map_or_else(
        || AGENT_FOLLOW_UP_MARK.to_string(),
        |text| format!("— message from the user: {} —", report_call(text)),
    )
}

/// `agentoutput`'s report (`docs/agents/agent-tools.md`): the frame line, then what
/// the agent has been doing as a **summary** — one `Name(args)` line per tool
/// call, the newest [`AGENT_REPORT_CALLS_MAX`] with the earlier ones counted,
/// never their outputs, and what it said on its way, whole
/// (`docs/agents/agent-progress.md`) — then, once it has settled, what it answered.
#[must_use]
pub fn output_report(snapshot: &AgentSnapshot) -> String {
    let mut lines = vec![snapshot.frame()];
    let hidden = snapshot.calls.len().saturating_sub(AGENT_REPORT_CALLS_MAX);
    match hidden {
        0 => {}
        1 => lines.push("… 1 earlier tool call".to_string()),
        n => lines.push(format!("… {n} earlier tool calls")),
    }
    // The calls, each delivered message marked where it arrived and each
    // update where the agent gave it — one recorded after `n` calls came
    // before call `n`. What happened among the hidden calls is counted with
    // them, never shown out of place.
    for entry in snapshot.timeline() {
        match entry {
            TimelineEntry::Call { index, header } if index >= hidden => {
                lines.push(report_call(header));
            }
            TimelineEntry::FollowUp(follow_up) if follow_up.at >= hidden => {
                lines.push(follow_up_mark(follow_up));
            }
            TimelineEntry::Update(update) if update.at >= hidden => {
                lines.extend(report_update(&update.text));
            }
            _ => {}
        }
    }
    match &snapshot.state {
        AgentState::Running if snapshot.calls.is_empty() => {
            lines.push("No tool calls yet.".to_string());
        }
        AgentState::Running => {}
        AgentState::Done(response) => {
            let response = response.trim_end_matches('\n');
            lines.push(String::new());
            lines.push("Final response:".to_string());
            lines.push(if response.trim().is_empty() {
                "(no output)".to_string()
            } else {
                response.to_string()
            });
        }
        AgentState::Failed(error) => {
            lines.push(String::new());
            lines.push(format!("Error: {error}"));
        }
        AgentState::Stopped { by_user: true } => {
            lines.push(String::new());
            lines.push(
                "[Stopped by the user — it cannot be resumed; launch a new agent if the \
                 task still needs doing.]"
                    .to_string(),
            );
        }
        AgentState::Stopped { by_user: false } => {
            lines.push(String::new());
            lines.push("[Stopped — its conversation is kept: agentsend resumes it.]".to_string());
        }
    }
    lines.join("\n")
}

/// `agentlist`'s report (`docs/agents/agent-tools.md`): every agent, launch order —
/// its id, type, task, where it stands and for how long, and how many tool
/// calls it has made.
#[must_use]
pub fn list_report(snapshots: &[AgentSnapshot]) -> String {
    if snapshots.is_empty() {
        return "No agents in this session.".to_string();
    }
    let rows: Vec<String> = snapshots
        .iter()
        .map(|snapshot| {
            let elapsed = crate::app::format_elapsed(snapshot.runtime.as_secs());
            let state = match snapshot.state {
                AgentState::Running => format!("running {elapsed}"),
                AgentState::Done(_) => format!("done in {elapsed}"),
                AgentState::Failed(_) => format!("failed after {elapsed}"),
                AgentState::Stopped { by_user: true } => {
                    format!("stopped by the user after {elapsed}")
                }
                AgentState::Stopped { by_user: false } => format!("stopped after {elapsed}"),
            };
            let task = if snapshot.description.is_empty() {
                String::new()
            } else {
                format!(
                    " {}",
                    crate::llm::tools::flatten_one_line(&snapshot.description)
                )
            };
            format!(
                "- {} ({}){task} — {state} · {}",
                snapshot.id,
                snapshot.agent_type,
                tool_uses_label(snapshot.tool_uses)
            )
        })
        .collect();
    let head = match snapshots.len() {
        1 => "1 agent:".to_string(),
        n => format!("{n} agents:"),
    };
    format!("{head}\n{}", rows.join("\n"))
}
