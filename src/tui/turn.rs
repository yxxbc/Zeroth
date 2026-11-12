//! Starting and ending turns.
//!
//! Every way a turn can begin funnels through this module, and they all do the
//! same four things: record what caused it, reset the per-turn clocks and the
//! renderer, derive the context, and spawn the backend on it. The four kinds:
//!
//! - [`Session::start_turn`] — the user's message(s). A `Submit` is a batch of
//!   one; a queue flush sends one batch as a single turn (`docs/input/queue.md`).
//! - [`Session::run_shell`] — a `!command`, run locally but on the same reply
//!   channel, so the status strip, the Esc interrupt and the resize repaint work
//!   with no special cases (`docs/input/shell-command.md`).
//! - [`Session::start_background_turn`] — the automatic follow-up that tells the
//!   model a background shell or agent finished (`docs/tools/background.md`).
//! - [`Session::start_compact_turn`] — codex's summarization turn, whose reply is
//!   diverted into the compact buffer instead of the transcript
//!   (`docs/architecture/compact.md`).
//! - [`Session::submit_startup_prompt`] — the `[PROMPT]` given on the command
//!   line, submitted at bootstrap exactly as Enter would have (`docs/cli/cli.md`).
//!
//! The end of a turn is one function too: [`Session::dispatch_after_turn`], run
//! at *every* turn-end site (`StreamDone`, a backend error, both Esc-interrupt
//! outcomes, and an idle completion arrival) so those paths can never drift. It
//! snapshots the code state, settles held notices, then dispatches whatever
//! comes next — a queued batch, or a follow-up turn about a completion the model
//! never heard about.
//!
//! Stopping a turn is [`Session::abandon_inflight`], and the one rule there is
//! **never `join()` on the loop**: a real backend can be parked in a blocking
//! read for up to an op-timeout, and joining froze the whole UI for that long
//! (`docs/input/interrupt.md`).

use std::path::PathBuf;
use std::time::Instant;

use ratatui::text::Line;

use zeroth::app::{QueuedTurn, Role};
use zeroth::background::PendingNotice;
use zeroth::checkpoint;
use zeroth::context;
use zeroth::paste;
use zeroth::project_doc;
use zeroth::stream::{CancelToken, ReplySource};
use zeroth::ui;

use super::Session;
use super::shell::spawn_shell_command;

/// One turn's user input handed to [`Session::start_turn`]: the text message(s)
/// — a Submit is a batch of one, a queue flush a batch — plus any Ctrl+V-attached
/// images as their `(placeholder, path)` pairs (codex's `UserMessage`'s `text` +
/// `local_images`): the names let recording match each path to the message whose
/// text carries its placeholder. See `docs/input/image-paste.md`.
pub(crate) struct TurnInput {
    pub(crate) texts: Vec<String>,
    pub(crate) images: Vec<(String, PathBuf)>,
}

/// A turn-end snapshot that finished on its own thread
/// (`docs/state/checkpoint.md`): the history length it was taken at, the
/// rewind epoch it belongs to, and the store's commit when git succeeded.
pub(crate) struct CheckpointDone {
    pub(crate) after: usize,
    pub(crate) epoch: u64,
    pub(crate) commit: Option<String>,
}

/// The snapshot in flight, with the turn-end dispatch it deferred. A queued
/// follow-up (or the automatic completion turn) must not start editing before
/// the just-ended state is captured; it waits for this result instead of
/// blocking the loop.
pub(crate) struct PendingCheckpoint {
    after: usize,
    epoch: u64,
    began: Instant,
    /// The worker running `git`. Joined on a rewind: the restore must not run
    /// against the store while the snapshot still holds its index lock.
    handle: std::thread::JoinHandle<()>,
    /// The unheard completion notices collected for the deferred dispatch.
    /// `None` until `dispatch_after_turn` has finished its non-checkpoint
    /// work.
    dispatch: Option<Vec<PendingNotice>>,
}

impl Session<'_> {
    /// Start one turn for `input`'s texts: record + commit each user bullet to
    /// scrollback, open the stream, reset the per-turn clocks, and spawn the
    /// backend on the joined prompt. Shared by the `Submit` key arm *and* every
    /// queue flush, so the paths can never drift. Empty batches are the caller's
    /// job to skip. See `docs/input/queue.md`.
    pub(crate) fn start_turn(&mut self, input: TurnInput) {
        // A direct submission that lands while the turn-end snapshot is still
        // running waits in the queue: the snapshot must capture the
        // just-ended state before this turn edits anything
        // (`docs/state/checkpoint.md`). The checkpoint result dispatches it.
        if self.checkpoint_pending() {
            self.app.queue_messages(input.texts, input.images);
            return;
        }
        let TurnInput { texts, images } = input;
        let width = self.term.screen().width;
        // Each path is recorded onto the message whose text carries its
        // placeholder, in text-occurrence order — a merged batch's duplicate
        // `[Image #1]`s resolve to their own drafts' paths, and the
        // undo/backtrack re-key zips occurrences straight over the recorded
        // order (docs/input/image-paste.md).
        let image_count = images.len();
        let paths: Vec<PathBuf> = images.iter().map(|(_, path)| path.clone()).collect();
        let mut per_text = paste::distribute_images(&texts, images);
        // Committing is suppressed while an agent session view covers the screen
        // (a queued main turn can dispatch there) — the return's purge-rebuild
        // regenerates the bubbles from history (docs/agents/agent-tool.md) — and while
        // a framed view's flow does (a queued turn can dispatch at a turn end
        // under an open screen-tall menu; docs/ui/view-flow.md). Under the Ctrl+O
        // overlay the inserts merely queue and the return's draw flushes them
        // (invariant 4).
        let committing = self.commits_allowed();
        for (text, attached) in texts.iter().zip(&mut per_text) {
            self.app
                .record_user_message_with_images(text, std::mem::take(attached));
            if committing {
                self.term
                    .insert_before(ui::message_lines(Role::User, text, width));
                // A Ctrl+V-pasted screenshot draws under its own bubble, the
                // rows taken from the item just recorded so the commit and the
                // rebuild agree (`docs/ui/images.md`).
                if let Some(item) = self.app.history.last() {
                    self.term.insert_before(ui::image_block_lines(item, width));
                }
                self.term.insert_before(vec![Line::default()]);
            }
        }
        self.app.begin_stream();
        // Count the user's uploaded input into the tally (arrow ↑) so the status
        // shows `↑ N tokens` during the backend's pre-stream pause, before its
        // first chunk flips the arrow back to ↓. Any Ctrl+V-attached images add
        // to the same ↑ tally (docs/input/image-paste.md).
        let prompt = texts.join("\n");
        self.app.count_user_input(&prompt);
        self.app.count_input_images(image_count);
        self.render.reset();
        // Start the turn clock; the draw branch keeps the status animated from
        // here.
        self.clocks.start_turn();
        // Refresh the project's AGENTS.md instructions right before the context
        // derives (docs/state/project-doc.md): the turn that just ended may have
        // written the guide (`/init`'s whole point), and this turn must already
        // carry it. The TUI process never chdirs, so this is the loop's cwd; on
        // the odd read failure the startup seed stands. The `/settings`
        // **Project docs** knob can withhold them entirely — `apply_setting`
        // already cleared them, so there is simply nothing to refresh
        // (docs/state/settings.md).
        if self.app.settings().project_docs
            && let Ok(cwd) = std::env::current_dir()
        {
            self.app
                .set_user_instructions(project_doc::load_user_instructions(&cwd));
        }
        // …and re-walk the skill roots for the same reason (docs/tools/skills.md):
        // the turn that just ended may have written a `SKILL.md`, and a skill
        // the user drops in mid-session must not need a restart to be seen.
        // Re-renders the listing this turn's context is about to carry.
        self.rescan_skills();
        self.rescan_agents();
        // The daily ping's day rollover (`TELEMETRY.md`): a session left
        // open across midnight counts on the new day too — this app idles in
        // a terminal all day, so pinging only at bootstrap undercounts
        // exactly its heaviest users. Here rather than on every event because
        // a *turn* is what "used it today" means, and on the common path it
        // is a date read and a string compare that touch no file.
        self.telemetry_day_check();
        // …and the update check's (docs/ops/update.md): the same once-a-day rule.
        self.update_day_check();
        // The whole conversation — the just-recorded user message included —
        // rides the request so a real model keeps its context across turns (the
        // AGENTS.md instructions in front); the image paths also travel the
        // original typed channel (codex's `UserInput::LocalImage`). See
        // docs/architecture/context.md.
        let context = context::context_messages_full(
            self.app.user_instructions.as_deref(),
            self.app.listings.as_deref(),
            &self.app.history,
        );
        // The rollout exists before the turn's thread does (codex's rule: the
        // file exists before the hook runs), so the turn's first hook — its
        // `UserPromptSubmit` — already names the transcript instead of racing
        // the loop-bottom sync that would otherwise create it, which then has
        // nothing left to write (docs/tools/hooks.md).
        self.recorder
            .sync(&self.app.history, self.app.history_generation());
        // A real prompt is the user's, whatever a loop-initiated turn left
        // marked (docs/tools/hooks.md).
        self.models.clear_synthetic_turn();
        self.spawn_reply(prompt, paths, context);
    }

    /// The `[PROMPT]` CLI shortcut (`docs/cli/cli.md`): the message given on the
    /// command line, submitted as the session's first turn the way the
    /// `Submit` key arm submits a draft — a stored secret's value redacted to
    /// its placeholder as a draft's is (`docs/state/secrets.md`), recorded into the
    /// ↑ input history (it *was* submitted), then [`Session::start_turn`] on a
    /// batch of one with no attachments. Called once by `bootstrap`, after the
    /// session directive is applied and the first frame is scheduled, so a
    /// `--continue`/`--resume` load's transcript sits above the new bubble
    /// and the turn appends to the adopted rollout file.
    pub(crate) fn submit_startup_prompt(&mut self, prompt: String) {
        let prompt = self.app.redact_typed(prompt);
        self.toast_masked_secrets();
        // A headless run's prompt is a script's, not something the user
        // typed: ↑ in the TUI never walks through it (docs/cli/headless.md).
        if !self.headless {
            self.app.input_history.record(&prompt);
        }
        self.start_turn(TurnInput {
            texts: vec![prompt],
            images: Vec::new(),
        });
    }

    /// Run a `!command` locally as a turn (the `Action::RunShell` arm; see
    /// `docs/input/shell-command.md`). Echoes `❯ !command` to scrollback + history,
    /// calls `App::begin_shell` (which sets up the status strip with the command
    /// as its running tool), then spawns the runner on the same streamed-reply
    /// channel — so the existing `ToolEnd`/`StreamDone` arms commit the cell with
    /// **no** `Ran for Ns` summary (`App::end_turn` deliberately returns `None`
    /// for a shell turn — the cell is the record, codex parity), and Esc routes
    /// through the normal interrupt path.
    pub(crate) fn run_shell(&mut self, command: String) {
        // See `start_turn`: a `!` command is a turn too, and must not run
        // while the turn-end snapshot is still reading the tree.
        if self.checkpoint_pending() {
            self.app.queue_shell_command(command);
            return;
        }
        let width = self.term.screen().width;
        // begin_shell records the cell's `! command` header (Role::Shell) in
        // history; commit it with NO trailing blank — the `⎿ Running…` preview
        // (and later the committed `⎿` output) sits flush below it, forming the
        // codex-style exec cell (docs/input/shell-command.md). Suppressed under an
        // agent session view — and an active view flow — like every main
        // commit (docs/agents/agent-tool.md, docs/ui/view-flow.md).
        self.app.begin_shell(&command);
        if self.commits_allowed() {
            self.term
                .insert_before(ui::message_lines(Role::Shell, &command, width));
        }
        self.render.reset();
        self.clocks.start_turn();
        // The `!` shell run IS the command (no separate ToolStart), so start its
        // Ctrl+B-hint clock here — a quick `!` command never flashes the hint.
        self.clocks.command_start = Some(Instant::now());
        let cancel = CancelToken::new();
        let handle = spawn_shell_command(
            command,
            self.tx.clone(),
            cancel.clone(),
            self.registry.clone(),
        );
        self.inflight = Some((cancel, handle));
    }

    /// Start the automatic follow-up turn for background completions the model
    /// has not heard about: like [`Session::start_turn`] but with **no new user
    /// message** — the just-settled notices are the turn's cause and already sit
    /// in history, so the derived context carries them (the prompt text is their
    /// context form, for the empty-context fallback / the dummy). The model then
    /// reports the result, exactly like the user's example transcript. See
    /// `docs/tools/background.md`.
    fn start_background_turn(&mut self, notices: &[PendingNotice]) {
        // A loop-initiated turn: its prompt is synthesized from the notice
        // board, so the UserPromptSubmit hook must not fire for it
        // (docs/tools/hooks.md).
        self.models.mark_synthetic_turn();
        self.app.begin_stream();
        let prompt = notices
            .iter()
            .map(|note| note.context.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        self.app.count_user_input(&prompt);
        self.render.reset();
        // A follow-up turn about background completions — text, no command yet.
        self.clocks.start_turn();
        // The agent that just finished in the background may have written a
        // skill; this turn is a real model turn, so it re-walks like any other
        // (docs/tools/skills.md).
        self.rescan_skills();
        self.rescan_agents();
        let context = context::context_messages_full(
            self.app.user_instructions.as_deref(),
            self.app.listings.as_deref(),
            &self.app.history,
        );
        self.spawn_reply(prompt, Vec::new(), context);
    }

    /// Start a `/compact` summarization turn (`docs/architecture/compact.md`) — manual
    /// (`Action::Compact`) or auto-triggered (the loop bottom's
    /// `should_auto_compact`). Like [`Session::start_background_turn`], no user
    /// message is recorded: the context is derived as-is and codex's
    /// summarization prompt rides as its final user entry (the real backend
    /// ignores the bare `prompt` whenever the context is non-empty; the prompt
    /// argument still serves the dummy, which scripts a text-only canned summary
    /// for it). Spawns on the **tools-free** one-off backend when one resolved,
    /// else the session backend.
    pub(crate) fn start_compact_turn(&mut self, auto: bool) {
        let compact_backend = self
            .models
            .compact_backend(self.app.thinking.as_ref().map(|t| t.mode), auto);
        self.app.begin_compact(auto);
        self.app.count_user_input(context::SUMMARIZATION_PROMPT);
        self.render.reset();
        self.clocks.start_turn();
        let cancel = CancelToken::new();
        // The summarizer reads the same window the model does — the AGENTS.md
        // instructions in front (codex's compact request keeps its initial
        // context too). See docs/state/project-doc.md.
        // …but **not** the skill listing: the summarizer runs on the
        // tools-free backend, so naming a `skill` tool it was never given is
        // the very inconsistency the listing's gate exists to prevent — and
        // the handoff summary has no use for the roster either
        // (`docs/tools/skills.md`).
        let mut compact_context = context::context_messages_full(
            self.app.user_instructions.as_deref(),
            None,
            &self.app.history,
        );
        compact_context.push(context::ContextMessage::new(
            context::ContextRole::User,
            context::SUMMARIZATION_PROMPT,
        ));
        let spawn_on: &dyn ReplySource = match compact_backend.as_ref() {
            Some(one_off) => one_off,
            None => self.models.backend(),
        };
        let handle = spawn_on.spawn(
            context::SUMMARIZATION_PROMPT.to_string(),
            Vec::new(),
            compact_context,
            self.tx.clone(),
            cancel.clone(),
        );
        self.inflight = Some((cancel, handle));
    }

    /// Spawn the session backend on `prompt` and hold the turn's cancel token +
    /// thread handle, so a quit or an interrupt can stop it. The tail every
    /// model-facing turn start shares.
    fn spawn_reply(
        &mut self,
        prompt: String,
        images: Vec<PathBuf>,
        context: Vec<context::ContextMessage>,
    ) {
        let cancel = CancelToken::new();
        let handle =
            self.models
                .backend()
                .spawn(prompt, images, context, self.tx.clone(), cancel.clone());
        self.inflight = Some((cancel, handle));
    }

    /// The every-turn-end dispatch (`docs/tools/background.md`, `docs/input/queue.md`):
    /// snapshot the code state, settle held background completions, then send the
    /// next queued entry — or, with nothing queued and a model-launched
    /// completion **the in-flight agent never heard about** (its note still on
    /// the registry's board — an agent that read it mid-turn owes no follow-up),
    /// start the **automatic follow-up turn** that tells the model its command
    /// finished (the notices are already in history, so they ride the derived
    /// context either way — a queued user batch simply carries them along with no
    /// extra request).
    ///
    /// Shared by every turn-end site (`StreamDone`, a backend error, both
    /// Esc-interrupt outcomes) *and* the idle completion arrival, so the paths
    /// can never drift.
    pub(crate) fn dispatch_after_turn(&mut self) {
        let _timing = super::timing::TimingGuard::new("turn-end");
        // Every turn end closes the open read/search group where the turn
        // ended (docs/ui/tool-groups.md); this is the backstop, so a group no
        // close point reached can never hold its reads out of scrollback.
        let committing = self.commits_allowed();
        self.close_tool_group(committing, self.term.screen().width);
        // The turn just ended and its history is settled. The snapshot runs
        // on its own thread; when it lands, the queued follow-up (or the
        // automatic completion turn) is dispatched — so nothing starts
        // editing before the just-ended state is captured, and the loop never
        // waits on `git` (docs/state/checkpoint.md).
        let snapshotting = self.start_checkpoint_snapshot();
        self.settle_bg_completions();
        // A plan whose every task is completed retires here — the turn that
        // finished it showed the all-green rows, and from now on the list is
        // gone rather than hidden, so the model's next `taskcreate` starts a
        // genuinely new plan instead of appending to the old ticks. The
        // shared registry follows, so its next `tasklist` agrees with the
        // strip (docs/tools/task-tools.md).
        if self.app.retire_finished_tasks() {
            self.sync_task_registry();
        }
        let unheard = self.registry.take_pending_notices();
        // A message the turn never reached a round boundary to read — it
        // simply answered — becomes the next turn instead of being dropped
        // (docs/input/queue.md). Taken from the shared handle first so the backend
        // can't read it after the fact, then off the strip and onto the front
        // of the follow-up queue, which `flush_next_queued` dispatches below.
        let _ = self.steer.take();
        self.app.reclaim_steered();
        // The finished turn's handle is spent; whatever dispatches below takes
        // its place (and nothing dispatching leaves the loop idle).
        self.inflight = None;
        if snapshotting {
            if let Some(pending) = self.checkpoint_pending.as_mut() {
                pending.dispatch = Some(unheard);
            }
            return;
        }
        self.dispatch_after_checkpoint(unheard);
    }

    /// Dispatch whatever comes after the turn-end snapshot: the next queued
    /// turn, else the automatic follow-up about a completion the model never
    /// heard. Shared by the synchronous and deferred paths so they cannot
    /// drift.
    fn dispatch_after_checkpoint(&mut self, unheard: Vec<PendingNotice>) {
        if !self.flush_next_queued() && unheard.iter().any(|note| note.from_model) {
            self.start_background_turn(&unheard);
        }
    }

    /// Flush the next queued turn, if any, dispatching by its kind: a text batch
    /// goes to the model ([`Session::start_turn`]) and a `!` command runs locally
    /// ([`Session::run_shell`]) — codex's action-tagged drain
    /// (`maybe_send_next_queued_input`). Returns whether anything dispatched.
    /// Shared by every turn-end drain site (`StreamDone`/`Error` — in the overlay
    /// too — and an Esc interrupt) so they can't drift. See `docs/input/queue.md`.
    fn flush_next_queued(&mut self) -> bool {
        match self.app.drain_next_batch() {
            // The batch's Ctrl+V attachments dispatch with it — the whole
            // (placeholder, path) pairs, so start_turn can record each path on
            // the message carrying its placeholder (docs/input/image-paste.md).
            Some(QueuedTurn::Messages { texts, images }) => {
                self.start_turn(TurnInput { texts, images });
                true
            }
            Some(QueuedTurn::Shell(command)) => {
                self.run_shell(command);
                true
            }
            None => false,
        }
    }

    /// Start the turn-end snapshot on its own thread
    /// (`docs/state/checkpoint.md`). Returns whether the dispatch after the
    /// turn must wait for it; a disabled store or a snapshot already in
    /// flight dispatches immediately. A failed git step is a silent no-op
    /// (the TUI never dies for a checkpoint); the recorded line flushes with
    /// the loop's next `recorder.sync`.
    fn start_checkpoint_snapshot(&mut self) -> bool {
        if !self.checkpoints.is_enabled() || self.checkpoint_pending.is_some() {
            return false;
        }
        let after = self.app.history.len();
        let epoch = self.checkpoint_epoch;
        let store = self.checkpoints.clone();
        let tx = self.checkpoint_tx.clone();
        let message = format!("checkpoint after {after} items");
        let handle = std::thread::spawn(move || {
            let began = Instant::now();
            let commit = store.snapshot(&message);
            super::timing::record("checkpoint.snapshot", began.elapsed());
            let _ = tx.send(CheckpointDone {
                after,
                epoch,
                commit,
            });
        });
        self.checkpoint_pending = Some(PendingCheckpoint {
            after,
            epoch,
            began: Instant::now(),
            handle,
            dispatch: None,
        });
        true
    }

    /// A turn-end snapshot landed: record the commit when it still belongs to
    /// this conversation, then dispatch what the turn-end deferred. A snapshot
    /// that started before a rewind (`checkpoint_epoch`) or whose history
    /// position no longer exists is dropped.
    pub(crate) fn on_checkpoint_done(&mut self, done: CheckpointDone) {
        let Some(pending) = self.checkpoint_pending.take() else {
            return;
        };
        super::timing::record("checkpoint.dispatch-wait", pending.began.elapsed());
        if !checkpoint_applies(
            &pending,
            &done,
            self.checkpoint_epoch,
            self.app.history.len(),
        ) {
            return;
        }
        if let Some(commit) = done.commit {
            self.recorder.record_checkpoint(checkpoint::Checkpoint {
                after: done.after,
                commit,
            });
        }
        if let Some(unheard) = pending.dispatch {
            self.dispatch_after_checkpoint(unheard);
        }
    }

    /// A rewind (`/clear`, a `/resume` load, a backtrack) makes any snapshot
    /// in flight describe a conversation that no longer exists: bump the
    /// epoch and forget the pending dispatch.
    pub(crate) fn invalidate_checkpoints(&mut self) {
        // Join first: the restore that follows rewinds the same private git
        // store, and two git processes must not hold its index at once.
        if let Some(pending) = self.checkpoint_pending.take() {
            let _ = pending.handle.join();
        }
        self.checkpoint_epoch = self.checkpoint_epoch.wrapping_add(1);
    }

    /// Whether a turn-end snapshot is still in flight — the headless idle
    /// rule and the input queue's gate.
    pub(crate) fn checkpoint_pending(&self) -> bool {
        self.checkpoint_pending.is_some()
    }

    /// Drain a snapshot still in flight before the session ends, recording it
    /// if it still belongs to the conversation. Never dispatches the deferred
    /// turn: the session is closing. Bounded, so a stuck `git` cannot hold
    /// the process past `grace`.
    pub(crate) async fn settle_checkpoint_quietly(&mut self, grace: std::time::Duration) {
        if self.checkpoint_pending.is_none() {
            return;
        }
        let deadline = tokio::time::Instant::now() + grace;
        while self.checkpoint_pending.is_some() {
            let received = tokio::time::timeout_at(deadline, self.checkpoint_rx.recv()).await;
            let Ok(Some(done)) = received else {
                self.checkpoint_pending = None;
                return;
            };
            let Some(pending) = self.checkpoint_pending.take() else {
                return;
            };
            if checkpoint_applies(
                &pending,
                &done,
                self.checkpoint_epoch,
                self.app.history.len(),
            ) && let Some(commit) = done.commit
            {
                self.recorder.record_checkpoint(checkpoint::Checkpoint {
                    after: done.after,
                    commit,
                });
            }
        }
    }

    /// Stop tracking the in-flight turn **without blocking the event loop**,
    /// swapping in a fresh reply channel — both ends, in place.
    ///
    /// The backend observes cancellation *cooperatively*, but a real network
    /// backend can be parked in a blocking read (waiting for the response headers
    /// or the first SSE byte) for up to one op-timeout before it notices — so
    /// `join`ing the thread here would freeze the whole UI (spinner, timer,
    /// composer) for that long: the interrupt-lag bug. Instead we **detach** the
    /// thread (it exits on its own once its read returns and it re-checks the
    /// token) and mint a **fresh** channel. Any last event the dying thread emits
    /// goes to its old sender, whose receiver this call has just dropped, so it
    /// can never leak into the next turn (which spawns on the new sender). This
    /// replaces the old `cancel + join + drain` teardown wholesale — the channel
    /// swap is both the "thread stopped sending" guarantee *and* the drain. The
    /// detached handle is parked in `reaping`, swept when finished (invariant: a
    /// cancelled `LlmBackend` / `DummyAi` / `StallAi` / shell runner streams
    /// nothing further and returns within one op-timeout). See
    /// `docs/input/interrupt.md`.
    pub(crate) fn abandon_inflight(&mut self) {
        if let Some((cancel, handle)) = self.inflight.take() {
            cancel.cancel();
            self.reaping.push(handle);
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.tx = tx;
        self.reply_rx = rx;
        self.clocks.end_turn();
    }
}

/// Whether a snapshot that landed still describes this conversation: it must
/// be the one requested (same `after`), from before any rewind (both epochs
/// agree with the session's), and the history must still reach its position.
fn checkpoint_applies(
    pending: &PendingCheckpoint,
    done: &CheckpointDone,
    epoch: u64,
    history_len: usize,
) -> bool {
    pending.after == done.after
        && pending.epoch == done.epoch
        && done.epoch == epoch
        && done.after <= history_len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(after: usize, epoch: u64) -> PendingCheckpoint {
        PendingCheckpoint {
            after,
            epoch,
            began: Instant::now(),
            handle: std::thread::spawn(|| {}),
            dispatch: None,
        }
    }

    fn done(after: usize, epoch: u64) -> CheckpointDone {
        CheckpointDone {
            after,
            epoch,
            commit: Some("abc".to_string()),
        }
    }

    #[test]
    fn a_checkpoint_only_applies_to_its_own_unchanged_history() {
        assert!(checkpoint_applies(&pending(7, 1), &done(7, 1), 1, 7));
        assert!(
            checkpoint_applies(&pending(7, 1), &done(7, 1), 1, 9),
            "a later turn may have grown the history"
        );
        assert!(
            !checkpoint_applies(&pending(7, 1), &done(7, 1), 1, 6),
            "a rewind past the snapshot's position must drop it"
        );
        assert!(
            !checkpoint_applies(&pending(7, 1), &done(7, 2), 2, 7),
            "a result from another epoch is stale"
        );
        assert!(
            !checkpoint_applies(&pending(7, 1), &done(7, 1), 2, 7),
            "the session rewound after the snapshot started"
        );
        assert!(
            !checkpoint_applies(&pending(7, 1), &done(6, 1), 1, 7),
            "the result names a different history position"
        );
    }
}
