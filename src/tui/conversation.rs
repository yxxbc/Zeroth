//! A conversation's runtime — everything `Session` used to hold beside the one
//! terminal (`docs/architecture/core-host-split.md` Phase 3).
//!
//! `App` is the conversation's pure model; this struct is the boundary half
//! around it: who replies (`ModelSession`), the channels a turn, a subagent, a
//! background shell and each worker report on, the registries and stores, the
//! clocks, the in-flight turn's handles and the working directory. The screen
//! ([`Screen`](super::screen::Screen)) and the view
//! ([`ConversationView`](super::view::ConversationView)) sit beside it, so a
//! second conversation would be another one of each.
//!
//! Nothing here decides anything: the decisions are `App`'s and `ui`'s, tested
//! without a terminal. The struct exists so the fields have a name and an
//! owner, and so the 62 of them are out of `Session`'s own list.
//!
//! Fields are `pub(crate)` because the `impl Session` handlers still reach
//! them through `self.conversation`; the method moves onto `impl Conversation`
//! follow per area.

use std::collections::HashMap;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::Instant;

use zeroth::agents::{AgentEvent, AgentRegistry};
use zeroth::app::App;
use zeroth::background::{BackgroundRegistry, BgEvent};
use zeroth::checkpoint::CheckpointStore;
use zeroth::stream::{CancelToken, StreamEvent};

use super::StatusClocks;
use super::models::ModelSession;
use super::permission::PermissionStore;
use super::recorder::SessionRecorder;
use super::turn::{CheckpointDone, PendingCheckpoint, PendingRestore, RestoreDone};
use super::usage;
use super::workers::{DeviceEvent, FileSearchResult, ModelFetch};

/// One-shot background worker results multiplexed onto a single event-loop channel.
pub(crate) enum WorkerEvent {
    FileMatches(FileSearchResult),
    ImagePaste(Result<PathBuf, String>),
    DiffResult(super::diff::DiffResult),
    UsageResult(usage::UsageResult),
}

/// One conversation's runtime state.
pub(crate) struct Conversation {
    /// The pure conversation state — the only thing that *decides* anything.
    pub(crate) app: App,
    /// Who replies, and how (`docs/providers/llm.md`).
    pub(crate) models: ModelSession,
    pub(crate) card_tx:
        tokio::sync::mpsc::UnboundedSender<(String, Option<zeroth::link_card::LinkCard>)>,
    pub(crate) card_rx:
        tokio::sync::mpsc::UnboundedReceiver<(String, Option<zeroth::link_card::LinkCard>)>,
    /// The live status indicator's clocks.
    pub(crate) clocks: StatusClocks,
    /// When the transient toast above the box should self-clear (`docs/ui/toast.md`).
    pub(crate) toast_deadline: Option<Instant>,
    /// The `AskUserQuestion` idle clock (`docs/tools/ask.md`): read by the draw
    /// tick, started over by every key or paste, resolving the waiting
    /// questions unanswered on [`Self::ask`] when it runs out — on this
    /// thread, so an answer and the expiry can never race.
    pub(crate) ask_timer: zeroth::ask::AskTimer,
    /// When each background shell started, so the manager's Runtime ticks
    /// (`docs/tools/background.md`).
    pub(crate) bg_clocks: HashMap<String, Instant>,
    /// When each running subagent started, so the roster's elapsed ticks.
    pub(crate) agent_clocks: HashMap<String, Instant>,
    /// When each subagent's **open thinking phase** started — the per-agent
    /// sibling of `StatusClocks::thinking_start`, so its session view's
    /// status line shows `Thinking for Ns` and the settle knows the phase's
    /// wall-clock. An entry exists only while that agent is thinking
    /// (`docs/agents/agent-view-streaming.md`).
    pub(crate) agent_thinking_clocks: HashMap<String, Instant>,
    /// When each subagent's **current running command** started — the
    /// per-agent sibling of `StatusClocks::command_start`, injected each
    /// frame as `AgentRun::command_elapsed` so its session view's `bash`
    /// tail counts its `+N lines (Ns · wait …)` clock row from the call's own
    /// `ToolStart`, never from the agent's runtime. Inserted at that
    /// `ToolStart`; **pruned by the roster tick** from the run's own queue —
    /// an agent without a running front call loses its entry — rather than
    /// removed at each resolution path (`docs/agents/agent-view-streaming.md`).
    pub(crate) agent_command_clocks: HashMap<String, Instant>,
    /// When each finished subagent's row should sweep off the roster
    /// (`docs/agents/agent-tool.md`).
    pub(crate) agent_expiry: HashMap<String, Instant>,
    /// The reply channel a turn streams on. Both ends are swapped wholesale by
    /// [`Conversation::abandon_inflight`] so a detached backend can't reach the next
    /// turn (`docs/input/interrupt.md`).
    pub(crate) tx: tokio::sync::mpsc::UnboundedSender<StreamEvent>,
    pub(crate) reply_rx: tokio::sync::mpsc::UnboundedReceiver<StreamEvent>,
    /// Queries for the `@` file-search worker and its last query sent — the
    /// dedupe that keeps an unchanged token from re-walking the tree
    /// (`docs/input/file-search.md`).
    pub(crate) file_req_tx: std::sync::mpsc::Sender<String>,
    pub(crate) last_file_query: Option<String>,
    /// Where this session's pasted images are saved —
    /// `{config_home}/image-cache/{session}`, numbered in paste order
    /// (`docs/input/image-paste.md`). Handed to each paste worker, which creates it.
    pub(crate) paste_dir: PathBuf,
    /// Git review generation so closed reviews cannot reopen.
    pub(crate) diff_generation: u64,
    /// Shared channel for one-shot background worker results (`file_search`,
    /// `image_paste`, `diff`, `usage`).
    pub(crate) worker_tx: tokio::sync::mpsc::UnboundedSender<WorkerEvent>,
    pub(crate) worker_rx: tokio::sync::mpsc::UnboundedReceiver<WorkerEvent>,
    /// The `/login` workers' channel — a sign-in's device flow
    /// (`docs/providers/copilot.md`) or a local server's connection test
    /// (`docs/providers/local-models.md`), one at a time — the `CancelToken` Esc reaps
    /// either with, and when the shown code expires — the countdown the page
    /// ticks, injected per draw like every other clock.
    pub(crate) device_tx: tokio::sync::mpsc::UnboundedSender<DeviceEvent>,
    pub(crate) device_rx: tokio::sync::mpsc::UnboundedReceiver<DeviceEvent>,
    pub(crate) device_cancel: Option<CancelToken>,
    pub(crate) device_expires: Option<Instant>,
    /// The `/model` picker's fetch results (`docs/providers/llm.md`), and the startup
    /// capability probe's own channel — separate so a concurrently-open picker
    /// can't confuse the results (`docs/providers/reasoning.md`).
    pub(crate) model_tx: tokio::sync::mpsc::UnboundedSender<ModelFetch>,
    pub(crate) model_rx: tokio::sync::mpsc::UnboundedReceiver<ModelFetch>,
    /// The token a closed `/usage` page or a newer round cancels its workers with.
    pub(crate) usage_cancel: Option<CancelToken>,
    /// The probe's sender is kept because a probe is spawned more than
    /// once: at bootstrap, and by a resume whose recorded model has facts
    /// this session never learned (`docs/state/session-model.md`).
    pub(crate) probe_tx: tokio::sync::mpsc::UnboundedSender<ModelFetch>,
    pub(crate) probe_rx: tokio::sync::mpsc::UnboundedReceiver<ModelFetch>,
    /// Background shells (`docs/tools/background.md`) and subagents
    /// (`docs/agents/agent-tool.md`) — never swapped, because both outlive turns.
    pub(crate) bg_rx: tokio::sync::mpsc::UnboundedReceiver<BgEvent>,
    pub(crate) agent_rx: tokio::sync::mpsc::UnboundedReceiver<AgentEvent>,
    /// The `@` file-search worker's handle, kept so the thread's lifetime is
    /// tied to the session's. Never joined.
    pub(crate) _file_worker: JoinHandle<()>,
    /// Background shells: the `run_in_background` launches, Ctrl+B hand-offs,
    /// and the shared notice board (`docs/tools/background.md`).
    pub(crate) registry: BackgroundRegistry,
    /// Subagents: the `agent` tool's roster and its threads
    /// (`docs/agents/agent-tool.md`).
    pub(crate) agent_registry: AgentRegistry,
    /// The **mid-turn message queue** (`docs/input/queue.md`): what the user
    /// submitted while a turn was already running, waiting for that turn's
    /// next round boundary. This side pushes; the backend thread drains.
    /// `App::steered` is the mirror the strip renders — and the one that
    /// survives a turn that ended without reading them, which is why the
    /// reclaim at every turn end goes through both.
    pub(crate) steer: zeroth::steer::SteerQueue,
    /// The tool-permission gate and this project's saved rules
    /// (`docs/state/permissions.md`).
    pub(crate) permissions: PermissionStore,
    /// The `AskUserQuestion` gate the modal's answers post on
    /// (`docs/tools/ask.md`) — always present (asking is not a permission), shared
    /// by every backend build.
    pub(crate) ask: zeroth::ask::AskGate,
    /// The shared task list the task tools operate on
    /// (`docs/tools/task-tools.md`) — the loop syncs it to `App::tasks` after every
    /// history rewind so the model's next `tasklist` agrees with the strip.
    pub(crate) task_registry: zeroth::tasks::TaskRegistry,
    /// The skills on disk (`docs/tools/skills.md`) — the set every backend build
    /// offers the `skill` tool over, and the source of the
    /// `<system-reminder>` listing the derived context leads with. Re-walked
    /// at every turn start ([`Session::rescan_skills`]), so a skill added
    /// mid-session — or written by the agent itself — is live on the next
    /// turn instead of waiting for a restart.
    pub(crate) skill_registry: zeroth::skills::SkillRegistry,
    /// The subagent definitions on disk (`docs/agents/subagents.md`) — the
    /// `agents/*.md` types a launch resolves `subagent_type` against, and the
    /// source of the agent half of the `<system-reminder>`. Re-walked at
    /// every turn start ([`Session::rescan_agents`]) beside the skills, so a
    /// type added mid-session — or written by the agent itself — is
    /// launchable on the next turn.
    pub(crate) subagents: zeroth::subagents::SubagentRegistry,
    /// A `[PROMPT]` from the command line, held while the startup review
    /// asks what may run, so the first turn starts with its answers
    /// (`docs/state/config-review.md`).
    pub(crate) held_prompt: Option<String>,
    /// The `SKILL.md` files whose parse failure has already been raised as a
    /// toast, so the rescan doesn't repeat itself every turn. Re-seeded from
    /// each walk's errors, so a file that is fixed and broken again reports
    /// again (`zeroth::skills::unreported_errors`).
    pub(crate) reported_skill_errors: std::collections::BTreeSet<std::path::PathBuf>,
    /// The agent definition files whose parse failure has already been raised
    /// as a toast — [`reported_skill_errors`](Self::reported_skill_errors)'s
    /// twin, one feature over (`zeroth::subagents::unreported_errors`).
    pub(crate) reported_agent_errors: std::collections::BTreeSet<std::path::PathBuf>,
    /// Mirrors history to the `/resume` rollout file (`docs/state/resume.md`).
    pub(crate) recorder: SessionRecorder,
    /// Per-turn working-directory snapshots (`docs/state/checkpoint.md`).
    pub(crate) checkpoints: CheckpointStore,
    /// The turn-end snapshot runs on its own thread; both ends of its result
    /// channel live here, so the loop records the commit without waiting for
    /// `git`.
    pub(crate) checkpoint_tx: tokio::sync::mpsc::UnboundedSender<CheckpointDone>,
    pub(crate) checkpoint_rx: tokio::sync::mpsc::UnboundedReceiver<CheckpointDone>,
    /// The snapshot in flight, with the turn-end dispatch deferred behind it
    /// (a queued follow-up must not edit before the just-ended state is
    /// captured).
    pub(crate) checkpoint_pending: Option<PendingCheckpoint>,
    /// Bumped whenever history is replaced or rewound: a snapshot that
    /// started before the rewind is dropped when it lands.
    pub(crate) checkpoint_epoch: u64,
    /// The checkpoint **restore** runs on its own thread too — a backtrack or a
    /// `/resume` resets the work tree with `git reset --hard` + `git clean`,
    /// which measured 1.5 s on a large tree (`docs/state/checkpoint.md`). Both
    /// ends of its result channel live here, so the loop keeps painting and
    /// reports the outcome when it lands.
    pub(crate) restore_tx: tokio::sync::mpsc::UnboundedSender<RestoreDone>,
    pub(crate) restore_rx: tokio::sync::mpsc::UnboundedReceiver<RestoreDone>,
    /// The restore in flight, and the token the next one takes.
    pub(crate) restore_pending: Option<PendingRestore>,
    pub(crate) restore_token: u64,
    /// The streaming reply's cancel token + thread handle; `None` when idle.
    pub(crate) inflight: Option<(CancelToken, JoinHandle<()>)>,
    /// Backend threads whose turn was interrupted or `/clear`ed: cancelled and
    /// **detached**, swept here when finished. Never joined on the loop — that
    /// was the interrupt-lag freeze (`docs/input/interrupt.md`).
    pub(crate) reaping: Vec<JoinHandle<()>>,
    /// The working directory: the tasks root, the permissions project key, the
    /// checkpoint store, the `/resume` filter, and the footer's `~`-relative
    /// form all key off it.
    pub(crate) cwd: PathBuf,
    pub(crate) cwd_display: String,
}
