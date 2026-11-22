//! What a terminal event asks the loop to do.
//!
//! `App::on_key` is pure: it updates the composer, moves a selection, decides —
//! and returns an [`Action`] describing the I/O it cannot do itself. This module
//! is the other half: [`Session::on_terminal_event`] routes each key press,
//! resize and bracketed paste, and [`Session::on_action`] is the routing table
//! from an `Action` to the method that performs it.
//!
//! Most arms are one line, because the work lives in the area that owns it —
//! `turn`, `agent`, `background`, `permission`, `models`, `resume`, `view`. That
//! is the point: **adding a feature means adding a method there and one line
//! here**, not editing a 900-line match.
//!
//! The arms that do stay here are the ones with no other home: quitting, the
//! `/clear` reset (which reaches into every subsystem at once, deliberately),
//! `/copy`'s clipboard write, a system notice, and a toast.
//!
//! Every key press ends with [`Session::after_key`] — schedule the frame, re-derive
//! the `@` picker's query, delete any temp image the key discarded.

use ratatui::crossterm::event::{Event, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};

use zeroth::app::{Action, COPY_OK_NOTICE, ToastKind, View};
use zeroth::checkpoint;
use zeroth::clipboard;
use zeroth::ui;

use super::turn::TurnInput;
use super::{Session, workers};

/// Whether the event loop keeps running after an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flow {
    Continue,
    Quit,
}

/// One scroll request for the screen-owning mode's conversation window
/// (`Session::scroll_full`, `docs/ui/full-screen.md`).
enum Scroll {
    PageUp,
    PageDown,
    Top,
    Bottom,
    Lines(isize),
}

impl Session<'_> {
    /// Handle one terminal event: a key press (through `App::on_key` and the
    /// action it returns), a resize, or a bracketed paste. Anything else — key
    /// releases, focus, mouse — is ignored.
    pub(crate) fn on_terminal_event(&mut self, event: Event) -> std::io::Result<Flow> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                // A key says the user is here: a waiting question's idle
                // clock starts over (docs/tools/ask.md).
                self.note_user_activity();
                let action = self.app.on_key(key);
                // A send that masked a typed secret's value says so — first,
                // so the action's own toast (a refusal) is the one left
                // showing (docs/state/secrets.md).
                self.toast_masked_secrets();
                let flow = self.on_action(action)?;
                if flow == Flow::Quit {
                    return Ok(Flow::Quit);
                }
                self.after_key(&key);
                Ok(Flow::Continue)
            }
            Event::Resize(width, height) => {
                self.on_resize(width, height)?;
                Ok(Flow::Continue)
            }
            // A real bracketed paste (term::init enables it). A large paste
            // collapses to a `[Pasted Content N chars]` placeholder in the
            // composer, expanded back on send — docs/input/paste.md.
            Event::Paste(pasted) => {
                self.note_user_activity();
                self.on_paste(&pasted);
                Ok(Flow::Continue)
            }
            // Mouse is the screen-owning mode's (`docs/ui/full-screen.md`): the
            // inline viewport never captures it, so these events only arrive
            // when a frame owns the screen.
            Event::Mouse(mouse) => {
                self.on_mouse(mouse)?;
                Ok(Flow::Continue)
            }
            _ => Ok(Flow::Continue),
        }
    }

    /// Perform the I/O an `Action` asks for. One arm per variant; the work lives
    /// in the module that owns the feature (see the module doc).
    fn on_action(&mut self, action: Action) -> std::io::Result<Flow> {
        if self.restore_pending.is_some()
            && matches!(
                action,
                Action::Clear
                    | Action::ConfirmBacktrack
                    | Action::ResumeSession(_)
                    | Action::Compact
                    | Action::Quit
            )
        {
            self.toast("Wait for the file restore to finish", ToastKind::Info);
            return Ok(Flow::Continue);
        }
        match action {
            Action::None => {}
            Action::Quit => {
                // Drop back to the main screen before the loop exits if an
                // overlay (the Ctrl+O transcript or the /resume picker) is up,
                // so restore() lands on the chat. A turn may have finished
                // while the overlay was showing — its scrollback commits are
                // queued (invariant 4) — so flush them and repaint the inline
                // view the same way a normal Ctrl+O return does; otherwise
                // restore() lands on the stale live status strip
                // ("Working… (… tokens)") instead of the committed
                // "Done for Ns" summary.
                if self.app.view != View::Conversation {
                    self.term.exit_overlay()?;
                    self.overlay_return_repaint()?;
                }
                // A `/login` device flow's worker parks on a poll for minutes;
                // cancel it so the process can actually exit (`docs/providers/copilot.md`).
                self.cancel_device_login();
                return Ok(Flow::Quit);
            }
            Action::Submit(text) => {
                // Drain any Ctrl+V-attached images staged by the submit
                // (docs/input/image-paste.md) to deliver them on the turn's typed
                // image channel.
                let images = self.app.take_submission_images();
                self.start_turn(TurnInput {
                    texts: vec![text],
                    images,
                });
            }
            Action::PasteImage => {
                // Ctrl+V: the clipboard I/O happens at the boundary (on_key
                // stayed pure) but on a worker thread, not here — the read +
                // decode + PNG encode of a large screenshot takes long enough to
                // freeze the status animations and swallow keystrokes if run
                // inline. The result arrives on the image channel, which attaches
                // it or commits the red notice.
                workers::spawn_image_paste(self.paste_dir.clone(), self.img_tx.clone());
            }
            Action::RunShell(command) => {
                // `!command` from an idle composer: echo it, then run it locally
                // as a turn (docs/input/shell-command.md). Reuses the streamed-reply
                // channel + inflight handle, so the status strip, Esc-interrupt,
                // and resize repaint all work exactly like an AI turn.
                self.run_shell(command);
            }
            // Enter mid-turn: hand the draft to the turn already running
            // (docs/input/queue.md). `App` has parked it in `steered` for the strip;
            // pushing it here is what puts it in front of the backend, which
            // takes it at its next round boundary and answers with
            // `StreamEvent::Steered`.
            Action::Steer(text) => self.steer.push(&text),
            // Alt+Up reaching past an empty follow-up queue to the messages
            // handed to the running turn: only the shared queue knows whether
            // the turn has read one yet, so it answers. Still waiting → back
            // to the composer. Already read → there is nothing to edit; the
            // fallback finds the queue empty (that is why this arm ran) and
            // does nothing, which is the honest outcome.
            Action::ReclaimSteered => match self.steer.take_last() {
                Some(text) => self.app.recall_steered(&text),
                None => self.app.recall_last_queued(),
            },
            // The same pull-back inside an agent session view, over that
            // agent's own loop (docs/input/queue.md). No `recall_last_queued`
            // fallback: the key arm already tried that agent's follow-ups and
            // must never reach the main session's backlog from here.
            Action::ReclaimAgentChat { id } => {
                if let Some(text) = self.models.backend().reclaim_agent_input(&id) {
                    self.app.recall_agent_chat(&id, &text);
                }
            }
            Action::Interrupt => self.interrupt_turn()?,
            Action::Compact => {
                // /compact (docs/architecture/compact.md): run codex's summarization turn —
                // the whole current context plus the fixed handoff prompt — with
                // the reply diverted into the compact buffer (never rendered);
                // `finish_compact` appends the marker at StreamDone and the
                // derivation compacts the model's context from there on. The turn
                // rides the normal `inflight` slot so Esc, /clear, and quit reap
                // it like any other turn.
                self.start_compact_turn(/*auto=*/ false);
            }
            Action::Clear => self.clear_conversation()?,
            Action::KillBackground(id) => self.kill_background(&id),
            Action::MoveToBackground => self.move_to_background(),
            Action::ScrollPageUp => self.scroll_full(Scroll::PageUp),
            Action::ScrollPageDown => self.scroll_full(Scroll::PageDown),
            Action::ScrollTop => self.scroll_full(Scroll::Top),
            Action::ScrollBottom => self.scroll_full(Scroll::Bottom),
            Action::StopAgent(id) => self.stop_agent(&id)?,
            Action::ViewAgent(_) => self.enter_agent_view()?,
            Action::LeaveAgentView => self.leave_agent_view()?,
            Action::AgentChat { id, text } => self.agent_chat(&id, &text),
            Action::ToggleToolView => self.toggle_tool_view()?,
            Action::OpenDiffReview => self.open_diff_review(),
            Action::RefreshDiffReview => self.load_diff_review(),
            Action::CloseDiffReview => self.close_diff_review()?,
            Action::ToggleContextDebug => self.toggle_context_debug()?,
            Action::ConfirmBacktrack => self.confirm_backtrack()?,
            Action::OpenResumePicker => self.open_resume_picker()?,
            Action::CloseResumePicker => self.close_resume_picker()?,
            Action::ResumeSession(path) => self.resume_session(path)?,
            Action::ResolvePermission { request, decision } => {
                self.resolve_permission(&request, decision);
            }
            Action::ResolveAsk { id, decision } => {
                // Post the user's answers (or decline/chat) on the ask gate,
                // waking the tool thread parked on it — the modal is already
                // closed and the composer draft restored (docs/tools/ask.md).
                self.ask.resolve(&id, decision);
                self.frame.schedule_frame();
            }
            Action::SetPermissionMode(mode) => self.set_permission_mode(mode),
            Action::OpenModelPicker => self.open_model_picker(),
            Action::CloseModelPicker => self.close_model_picker(),
            Action::SelectModel {
                provider,
                id,
                reasoning,
                vision,
                context,
                service_tiers,
            } => self.select_model(&provider, &id, reasoning, vision, context, service_tiers),
            Action::SetThinking(mode) => self.set_thinking(mode),
            Action::SetSpeed(tier) => self.set_speed(tier),
            Action::OpenKeyOnboarding => self.open_key_onboarding(),
            Action::OpenLogout => self.open_logout(),
            Action::LogoutProvider(provider) => self.logout_provider(&provider)?,
            Action::CloseKeyOnboarding => {
                // Esc/Ctrl+C dismissed the flow. A device sign-in it was
                // *past* answers Action::CancelDeviceLogin instead, so there
                // is nothing to reap here; the region collapses back to the
                // composer on the next draw.
            }
            Action::SaveApiKey {
                provider,
                env_var,
                key,
            } => self.save_api_key(&provider, &env_var, &key),
            Action::StartDeviceLogin { provider, kind } => {
                self.start_device_login(&provider, kind);
            }
            Action::CancelDeviceLogin => self.cancel_device_login(),
            Action::TestLocalServer { provider, test } => self.test_local_server(&provider, test),
            // Esc (or Ctrl+C) while a connection test runs: the page already
            // stopped waiting; the worker's answer must never arrive.
            Action::CancelServerTest => self.cancel_device_login(),
            Action::CopyDeviceCode(code) => self.copy_device_code(&code),
            Action::OpenSettings => self.open_settings(),
            Action::CloseSettings => {
                // Esc/Ctrl+C dismissed the menu: nothing to reap; the region
                // collapses back to the composer on the next draw.
            }
            Action::SettingChanged(key) => self.apply_setting(key)?,
            Action::OpenMascotPicker => {
                // The pure open already happened (the catalog is a const —
                // nothing to fetch or inject); after_key schedules the frame.
            }
            Action::CloseMascotPicker => {
                // Esc/Ctrl+C dismissed the picker: nothing to reap; the
                // region collapses back to the composer on the next draw.
            }
            Action::SelectMascot(mascot) => self.select_mascot(mascot)?,
            Action::OpenSpinnerPicker => {
                // The pure open already happened (the catalog is a const);
                // after_key schedules the frame, and the open picker's own
                // animation re-arm keeps its preview turning (docs/ui/spinner.md).
            }
            Action::CloseSpinnerPicker => {
                // Esc/Ctrl+C dismissed the picker: nothing to reap; the
                // region collapses back to the composer on the next draw.
            }
            Action::SelectSpinner(spinner) => self.select_spinner(spinner),
            Action::OpenThemePicker => {
                // The pure open already happened (the catalog is a const);
                // after_key schedules the frame (docs/ui/theme.md).
            }
            Action::CloseThemePicker => {
                // Esc/Ctrl+C dismissed the picker: nothing to reap; the
                // region collapses back to the composer on the next draw.
            }
            Action::SelectTheme(theme) => self.select_theme(theme)?,
            Action::OpenDonatePicker => {
                // The pure open already happened (the catalog is a const —
                // nothing to fetch or inject); after_key schedules the frame
                // (docs/ui/donate.md).
            }
            Action::CloseDonatePicker => {
                // Esc/Ctrl+C dismissed the page: nothing to reap; the region
                // collapses back to the composer on the next draw.
            }
            Action::CopyDonationAddress(address) => self.copy_donation_address(address),
            // The pure open already happened; the boundary reloads the file
            // so a secret another session saved is listed (docs/state/secrets.md).
            Action::OpenSecretsPage => self.open_secrets_page(),
            Action::CloseSecretsPage => {
                // Esc/Ctrl+C dismissed the page — and its draft with it; the
                // region collapses back to the composer on the next draw.
            }
            Action::SaveSecret(draft) => self.save_secret(&draft),
            Action::DeleteSecret(name) => self.delete_secret(&name),
            Action::CopySecretPlaceholder(name) => self.copy_secret_placeholder(&name),
            // The pure open already counted the window; the boundary hands
            // it the backend's tool set before the first frame
            // (docs/architecture/context-usage.md).
            Action::OpenContextUsage => self.sync_context_usage(),
            // Only the boundary knows which subscriptions are signed in, so
            // it opens the usage page over them and reads each one — or
            // toasts that none is — and reads them again on `r`
            // (docs/providers/usage.md).
            Action::OpenUsagePage => self.open_usage_page(),
            Action::RefreshUsagePage => self.refresh_usage_page(),
            Action::CloseUsagePage => self.cancel_usage_fetch(),
            Action::CopyUsageLink(url) => self.copy_usage_link(&url),
            Action::CloseContextUsage => {
                // Esc/Ctrl+C dismissed the page: nothing to reap; the region
                // collapses back to the composer on the next draw.
            }
            Action::OpenExportPicker => {
                // The pure open already happened (the rows are a const —
                // nothing to fetch or inject); after_key schedules the frame
                // (docs/ui/export.md).
            }
            Action::CloseExportPicker => {
                // Esc/Ctrl+C dismissed the page: nothing to reap; the region
                // collapses back to the composer on the next draw.
            }
            Action::Export(target) => self.export_conversation(target),
            Action::OpenHooksMenu => self.open_hooks_menu(),
            Action::OpenSkillsMenu => self.open_skills_menu(),
            Action::CloseSkillsMenu => {
                // Esc/Ctrl+C dismissed the menu: nothing to reap; the region
                // collapses back to the composer on the next draw.
            }
            Action::SkillToggled { name, enabled } => self.apply_skill_toggle(&name, enabled),
            Action::CloseHooksMenu => {
                // Esc/Ctrl+C dismissed the browser: nothing to reap; the
                // region collapses back to the composer on the next draw.
            }
            Action::ReviewAnswered(answer) => self.apply_review_answer(answer),
            Action::SetHookTrust {
                source,
                identity,
                trusted,
            } => self.set_hook_trust(&source, identity, trusted),
            Action::Notice(text) => {
                // A one-off system notice (the empty `!` shell's hint). The
                // helper finalises any mid-flight reply segment first (same
                // ordering trick as a tool call) so the notice slots after it
                // in scrollback and history alike.
                self.commit_system_notice(&text);
            }
            Action::Toast(text) => {
                // A slash command's transient info toast — a soft rejection
                // (/resume, /compact run mid-turn) that self-clears above the box
                // instead of landing in scrollback. See docs/ui/toast.md.
                self.toast(text, ToastKind::Info);
            }
            Action::Copy(text) => self.copy_to_clipboard(&text),
        }
        Ok(Flow::Continue)
    }

    /// The bookkeeping every handled key press owes: schedule its redraw, kick
    /// off a file search if the active `@token` changed, and delete the temp PNGs
    /// of any attachments the key discarded (an atomic placeholder delete, a
    /// Ctrl+C clear, a `/clear`ed queue) — the pure core records the drops, the
    /// file I/O lives here (`docs/input/image-paste.md`).
    fn after_key(&mut self, key: &ratatui::crossterm::event::KeyEvent) {
        self.schedule_for_key(key);
        self.dispatch_file_search();
        for path in self.app.take_discarded_images() {
            let _ = std::fs::remove_file(path);
        }
    }

    /// `/copy`: do the clipboard I/O here at the boundary (`run_selected_command`
    /// stayed pure). The pure core already decided *what* to copy — and, with
    /// nothing to copy, answered with its own info toast instead of reaching
    /// here. The result surfaces as a transient toast, not a scrollback bullet.
    /// See `docs/ui/copy.md` / `docs/ui/toast.md`.
    fn copy_to_clipboard(&mut self, text: &str) {
        match clipboard::copy_to_clipboard(text) {
            Ok(lease) => {
                // Hold the native selection alive for the app's lifetime
                // (Linux); None over OSC 52.
                self.clipboard_lease = lease;
                self.toast(COPY_OK_NOTICE, ToastKind::Info);
            }
            Err(reason) => self.toast(format!("Copy failed: {reason}"), ToastKind::Error),
        }
    }

    /// Move the screen-owning mode's conversation window. Nothing to do in
    /// the inline viewport (its history is the terminal's scrollback) or before
    /// the first frame has measured the painted tail.
    fn scroll_full(&mut self, scroll: Scroll) {
        if !self.term.fullscreen() || self.view.full_len == 0 {
            return;
        }
        let conv = self.view.full_conv_h;
        let max_top = self.view.full_len.saturating_sub(conv);
        let current = self.full_top_now();
        let page = conv.saturating_sub(2).max(1) as isize;
        let target = match scroll {
            Scroll::PageUp => current as isize - page,
            Scroll::PageDown => current as isize + page,
            Scroll::Top => 0,
            Scroll::Bottom => max_top as isize,
            Scroll::Lines(lines) => current as isize + lines,
        };
        let target = target.clamp(0, max_top as isize) as usize;
        // Reaching the bottom resumes follow-tail; anywhere above it anchors
        // the view to that line, so new output does not move it.
        self.view.full_top = ui::scroll_anchor(target, self.view.full_len, conv);
        self.view.full_hover = None;
        self.frame.schedule_frame();
    }

    /// The screen-owning mode's mouse: wheel scrolls the conversation, move
    /// hovers a link (the app draws the underline), a left drag selects and
    /// copies (mouse capture takes the terminal's own selection; Shift+drag is
    /// the escape hatch on terminals that keep it), and a left click with no
    /// drag opens the link under the pointer.
    fn on_mouse(&mut self, mouse: MouseEvent) -> std::io::Result<()> {
        if !self.term.fullscreen() {
            return Ok(());
        }
        // The permission prompt takes the pointer before anything else: it is
        // the one modal whose answer a click can give, and a click must never
        // reach the conversation through it (`docs/state/permissions.md`).
        if self.app.permission().is_some() {
            self.view.full_bar_drag = None;
            return self.on_permission_mouse(mouse);
        }
        // The `/resume` picker is a page of rows, so a click picks one
        // (`docs/state/resume.md`).
        if self.app.view == View::ResumePicker {
            self.view.full_bar_drag = None;
            return self.on_resume_mouse(mouse);
        }
        if self.app.view != View::Conversation || self.app.modal_open() {
            self.view.full_bar_drag = None;
            return Ok(());
        }
        // The scrollbar owns the conversation's last column, and it takes the
        // pointer before every link, cell and selection: a click on its track
        // jumps the window there, a press on the thumb and a drag slide it
        // (`docs/ui/full-screen.md`). Wheel and keys are handled below and
        // keep working as they did.
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(row) = self.full_bar_row(&mouse) {
                    let top = self.full_bar_press(row);
                    self.view.full_top =
                        ui::scroll_anchor(top, self.view.full_len, self.view.full_conv_h);
                    // The anchor is the **screen** row: a drag is measured in
                    // screen rows, so a pinned prompt appearing mid-drag (it
                    // shifts the track down one) does not teleport the thumb.
                    self.view.full_bar_drag = Some((mouse.row, top));
                    self.view.full_press = None;
                    self.view.full_select = None;
                    self.view.full_pin_press = false;
                    self.view.full_hover = None;
                    self.frame.schedule_frame();
                    return Ok(());
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((press_row, press_top)) = self.view.full_bar_drag {
                    let height = u16::try_from(self.view.full_conv_h).unwrap_or(u16::MAX);
                    let delta = i32::from(mouse.row) - i32::from(press_row);
                    let delta = isize::try_from(delta).unwrap_or(0);
                    let top = ui::scrollbar_top_after_drag(
                        self.view.full_len,
                        self.view.full_conv_h,
                        height,
                        press_top,
                        delta,
                    )
                    .unwrap_or(press_top);
                    self.view.full_top =
                        ui::scroll_anchor(top, self.view.full_len, self.view.full_conv_h);
                    self.view.full_press = None;
                    self.view.full_select = None;
                    self.frame.schedule_frame();
                    return Ok(());
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // The bar's release ends the drag; it is never a link click or
                // the end of a copy, and the selection anchors never opened.
                if self.view.full_bar_drag.take().is_some() {
                    self.frame.schedule_frame();
                    return Ok(());
                }
            }
            _ => {}
        }
        // Use the last painted tabs, not a layout recomputed after roster events.
        let tab = self
            .view
            .full_tab_hits
            .tab_at(mouse.column, mouse.row)
            .filter(|_| self.app.session_tabs_available());
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.view.full_tab_press = tab.map(|id| id.map(str::to_string));
                if self.view.full_tab_press.is_some() {
                    self.view.full_press = None;
                    self.view.full_select = None;
                    self.frame.schedule_frame();
                    return Ok(());
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => self.view.full_tab_press = None,
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(pressed) = self.view.full_tab_press.take() {
                    if tab == Some(pressed.as_deref()) {
                        let action = self.app.select_session_tab(pressed.as_deref());
                        self.on_action(action)?;
                    }
                    self.frame.schedule_frame();
                    return Ok(());
                }
            }
            _ => {}
        }
        // A left click in the composer places the caret there — the one
        // live-region click the mode supports so far (`docs/ui/full-screen.md`).
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let screen = self.term.screen();
            let height = self.live_region_height();
            // The composer lives in the conversation's width: the sidebar's
            // column is not the live region (`docs/ui/sidebar.md`), so a click
            // there is not a click in the input.
            let live = ratatui::layout::Rect::new(
                0,
                screen.height.saturating_sub(height),
                screen
                    .width
                    .saturating_sub(ui::sidebar_columns(&self.app, screen.width)),
                height.min(screen.height),
            );
            if let Some(path) = ui::composer_image_at(live, &self.app, mouse.column, mouse.row) {
                // A pasted picture's placeholder is the one composer cell a
                // click *opens* rather than seats the caret in
                // (`docs/input/image-paste.md`).
                let url = path
                    .to_str()
                    .and_then(|path| self.app.path_display().file_url(path));
                match url {
                    Some(url) => self.open_full_link(&url),
                    None => {
                        self.toast(format!("Can't open {}", path.display()), ToastKind::Error);
                    }
                }
                self.view.full_press = None;
                self.view.full_select = None;
                self.frame.schedule_frame();
                return Ok(());
            }
            if let Some(cursor) = ui::composer_cursor_at(live, &self.app, mouse.column, mouse.row) {
                self.app.focus_workspace_composer();
                self.app.input.set_cursor(cursor);
                self.view.full_press = None;
                self.view.full_select = None;
                self.frame.schedule_frame();
                return Ok(());
            }
        }
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                self.view.full_press = None;
                self.view.full_select = None;
                self.scroll_full(Scroll::Lines(-3));
            }
            MouseEventKind::ScrollDown => {
                self.view.full_press = None;
                self.view.full_select = None;
                self.scroll_full(Scroll::Lines(3));
            }
            MouseEventKind::Moved => {
                let hover = self.content_row(&mouse).map(|row| (mouse.column, row));
                let on_pin = self.on_pin_row(&mouse);
                let moved = self.view.full_hover != hover || self.view.full_pin_hover != on_pin;
                if self.view.full_pin_hover != on_pin {
                    self.view.full_pin_hover = on_pin;
                }
                self.view.full_hover = hover;
                if moved {
                    self.frame.schedule_frame();
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                self.view.full_pin_press = self.on_pin_row(&mouse);
                self.view.full_press = self.full_cell(&mouse);
                self.view.full_select = None;
                self.view.full_hover = None;
                self.frame.schedule_frame();
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // A drag started on the strip is not a click on it.
                self.view.full_pin_press = false;
                if let (Some(press), Some(current)) = (self.view.full_press, self.full_cell(&mouse))
                {
                    self.view.full_select = (press != current).then_some((press, current));
                    self.view.full_hover = None;
                    self.frame.schedule_frame();
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                // A drag copies; a click (no drag) opens the link under it.
                let text = self.full_selected_text();
                let clicked = self.view.full_press.take().is_some();
                let pin_click = std::mem::take(&mut self.view.full_pin_press);
                self.view.full_select = None;
                if let Some(text) = text {
                    // A selection is not "/copy": say what was copied.
                    match clipboard::copy_to_clipboard(&text) {
                        Ok(lease) => {
                            self.clipboard_lease = lease;
                            self.toast("Copied selection to clipboard", ToastKind::Info);
                        }
                        Err(reason) => {
                            self.toast(format!("Copy failed: {reason}"), ToastKind::Error);
                        }
                    }
                } else if pin_click && self.on_pin_row(&mouse) {
                    // The strip stands for a prompt above the window: a click
                    // brings it back into view (`docs/ui/conversation-tool-details-and-pin.md`).
                    self.scroll_to_pinned_prompt();
                } else if clicked
                    && let Some(row) = self.content_row(&mouse)
                    && let Some(url) = self.full_link_at(mouse.column, row)
                {
                    self.open_full_link(&url);
                } else if clicked && let Some(row) = self.content_row(&mouse) {
                    self.toggle_full_item(row);
                }
                self.frame.schedule_frame();
            }
            _ => {}
        }
        Ok(())
    }

    /// A fetched link card: remember it (and persist the cache), then rebuild
    /// the tail so the lone URL line becomes the card (`docs/ui/rich-links.md`).
    /// A failed fetch sends nothing and leaves its URL in `card_pending`, so it
    /// is not retried in a loop.
    pub(crate) fn on_card(&mut self, url: String, card: Option<zeroth::link_card::LinkCard>) {
        if let Some(card) = card {
            self.view.cards.insert(url, card);
            super::link_cards::save(&self.view.cards);
            self.view.full_tail_dirty = true;
            self.frame.schedule_frame();
        }
    }

    /// Toggle the expandable history item under a conversation row: its cell
    /// switches between the collapsed inline form and the Ctrl+O form, and the
    /// next rebuild pins the item's first line where it was so the click does
    /// not move the text under the pointer (`docs/ui/full-screen.md`).
    fn toggle_full_item(&mut self, content_row: u16) {
        let line = self.view.full_start + usize::from(content_row);
        let Some(&(item_start, _, index)) = self
            .view
            .full_items
            .iter()
            .find(|(start, count, _)| line >= *start && line < start + count)
        else {
            return;
        };
        if !self.view.full_expanded.remove(&index) {
            self.view.full_expanded.insert(index);
        }
        let item_screen_row =
            usize::from(content_row).saturating_sub(line.saturating_sub(item_start));
        self.view.full_anchor = Some((item_start, item_screen_row));
        self.view.full_tail_dirty = true;
        self.view.full_hover = None;
        self.frame.schedule_frame();
    }

    /// A mouse event's cell as a **tail** position, when it lands in the
    /// painted conversation window below the pinned prompt row.
    fn full_cell(&self, mouse: &MouseEvent) -> Option<(usize, u16)> {
        self.content_row(mouse)
            .map(|row| (self.view.full_start + usize::from(row), mouse.column))
    }

    /// Is the pointer on the pinned prompt strip? It occupies the frame's
    /// first conversation row, which [`content_row`](Self::content_row)
    /// deliberately skips — the strip is a control, not conversation.
    fn on_pin_row(&self, mouse: &MouseEvent) -> bool {
        self.view.full_sticky_rows == 1 && mouse.row == 0
    }

    /// Where the conversation window starts right now: the anchor, or the
    /// tail when the view has not been scrolled.
    fn full_top_now(&self) -> usize {
        let max_top = self.view.full_len.saturating_sub(self.view.full_conv_h);
        self.view.full_top.unwrap_or(max_top).min(max_top)
    }

    /// The scrollbar's track row under the pointer, when the pointer is on the
    /// bar's column and the conversation is taller than its window.
    ///
    /// The bar is the conversation's last column, left of the sidebar (which
    /// owns the screen's right edge); it starts below the pinned prompt's row
    /// because it tracks the window, not the whole conversation area
    /// (`InlineViewport::draw_full_with_tail`).
    fn full_bar_row(&self, mouse: &MouseEvent) -> Option<u16> {
        let screen = self.term.screen();
        let rest = screen
            .width
            .saturating_sub(ui::sidebar_columns(&self.app, screen.width));
        if rest <= ui::SCROLLBAR_COLUMNS {
            return None;
        }
        if mouse.column != rest.saturating_sub(ui::SCROLLBAR_COLUMNS) {
            return None;
        }
        let row = self.content_row(mouse)?;
        let height = u16::try_from(self.view.full_conv_h).ok()?;
        if row >= height {
            return None;
        }
        // A conversation that fits paints no bar, so its reserved column takes
        // no pointer either.
        ui::scrollbar_thumb(
            self.view.full_len,
            self.view.full_conv_h,
            self.full_top_now(),
            height,
        )?;
        Some(row)
    }

    /// The conversation top a press at track `row` asks for: a click on the
    /// track puts the pointer's row at the thumb, while a press on the thumb
    /// grabs it exactly where it is (the painter's rounding is not invertible,
    /// so mapping a grab through it would jump the window before the drag).
    fn full_bar_press(&self, row: u16) -> usize {
        let current = self.full_top_now();
        let height = u16::try_from(self.view.full_conv_h).unwrap_or(u16::MAX);
        let index = usize::from(row);
        let on_thumb =
            ui::scrollbar_thumb(self.view.full_len, self.view.full_conv_h, current, height)
                .is_some_and(|(start, size)| index >= start && index < start.saturating_add(size));
        if on_thumb {
            return current;
        }
        ui::scrollbar_top_for_row(self.view.full_len, self.view.full_conv_h, height, row)
            .unwrap_or(current)
    }

    /// Scroll the prompt the pinned strip stands for back into view: its first
    /// row becomes the window's top, which retires the strip on the next frame
    /// (the message is visible again, so nothing needs summarising). The
    /// target is clamped like every other scroll anchor.
    fn scroll_to_pinned_prompt(&mut self) {
        let Some(start) = self.view.full_pin_row else {
            return;
        };
        self.view.full_top = ui::scroll_anchor(start, self.view.full_len, self.view.full_conv_h);
        self.view.full_hover = None;
        self.view.full_pin_hover = false;
        self.frame.schedule_frame();
    }

    /// A click inside the `/resume` picker: the row under the pointer is
    /// highlighted, or resumed when it is already highlighted — the picker's
    /// Enter, shared with the click ([`App::click_resume_row`]).
    fn on_resume_mouse(&mut self, mouse: MouseEvent) -> std::io::Result<()> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        let screen = self.term.screen();
        let area = ratatui::layout::Rect::new(0, 0, screen.width, screen.height);
        let Some(index) = ui::resume_row_at(&self.app, area, mouse.row, mouse.column) else {
            return Ok(());
        };
        let action = self.app.click_resume_row(index);
        self.on_action(action).map(|_| ())
    }

    /// A click on the open **permission prompt**: the option whose rows it
    /// landed on is highlighted, or answered when it is already highlighted
    /// ([`App::click_permission_option`]).
    ///
    /// The prompt is painted as its page's **tail** in the live region
    /// (`render_permission`), so the pointer's row is mapped from the bottom
    /// of the page — the same arithmetic
    /// [`ui::permission_option_at`] does, with the region's own geometry.
    fn on_permission_mouse(&mut self, mouse: MouseEvent) -> std::io::Result<()> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(());
        }
        // The rows come from the frame that was **painted**, never from a
        // fresh layout: the prompt is laid out for the live region's height at
        // paint time, and that height can change before the click lands —
        // recomputing then missed the options entirely.
        let Some(index) = self.view.full_tab_hits.permission_option_at(mouse.row) else {
            return Ok(());
        };
        let action = self.app.click_permission_option(index);
        // A resolved prompt answers on another thread; the flow only says
        // whether the app should quit, which answering never does.
        self.on_action(action).map(|_| ())
    }

    /// The conversation-content row under the pointer (0-based, below the
    /// pinned prompt's row), when the pointer is inside it.
    fn content_row(&self, mouse: &MouseEvent) -> Option<u16> {
        let row = usize::from(mouse.row).checked_sub(self.view.full_sticky_rows)?;
        (row < self.view.full_conv_h).then(|| u16::try_from(row).unwrap_or(u16::MAX))
    }

    /// Open a link the screen-owning mode's mouse found: the same three
    /// schemes the OSC 8 path trusts, through the platform's opener. The child
    /// is reaped on its own thread so no zombie is left behind.
    fn open_full_link(&mut self, url: &str) {
        let supported = ["http://", "https://", "file://"]
            .iter()
            .any(|scheme| url.starts_with(scheme));
        if !supported {
            self.toast(format!("Can't open {url}"), ToastKind::Error);
            return;
        }
        match opener(url).spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(error) => self.toast(format!("Couldn't open {url}: {error}"), ToastKind::Error),
        }
    }

    /// Esc mid-generation (codex-style): stop the backend but NEVER join it on
    /// the loop. A real backend can be parked in a blocking network read (the
    /// pre-first-token pause) for up to one op-timeout before it observes the
    /// cancel; join()ing there froze the whole UI — spinner, timer, composer —
    /// for that long (the interrupt-lag bug). Instead
    /// [`Session::abandon_inflight`] cancels + detaches the thread and swaps in a
    /// fresh reply channel, so any stale event it still emits (a final chunk, or a
    /// ToolStart that would otherwise wedge a phantom running tool) lands on the
    /// dropped receiver and can't reach the next turn. See `docs/input/interrupt.md`.
    fn interrupt_turn(&mut self) -> std::io::Result<()> {
        self.interrupt_turn_impl(true)
    }

    /// Logout stops work without automatically submitting queued follow-ups.
    pub(crate) fn interrupt_for_logout(&mut self) -> std::io::Result<()> {
        self.interrupt_turn_impl(false)
    }

    fn interrupt_turn_impl(&mut self, dispatch: bool) -> std::io::Result<()> {
        // The turn's clock, read before `abandon_inflight` stops it: a kept
        // turn's receipt says how long the turn ran (docs/input/interrupt.md).
        let elapsed = self
            .clocks
            .turn_start
            .map_or(0, |start| start.elapsed().as_secs());
        // The running command's own clock, read now rather than at the last
        // draw: the call the interrupt resolves records how long it ran
        // (docs/state/rollout-records.md).
        self.app
            .set_command_elapsed(self.clocks.command_start.map(|start| start.elapsed()));
        // A thinking phase caught mid-thought settles first, so its
        // `Thought for …` cell sits ahead of everything the interrupt keeps.
        // Recording it is also what makes this a *Kept* interrupt: real work
        // streamed, and codex never retracts what streamed. With nothing
        // thought (or the display off) it is a no-op and the undo path is
        // untouched. It runs before `abandon_inflight` too, which stops the
        // thinking clock with the others: settled after it, the cell read
        // `Thought for 0s` however long the model had thought
        // (docs/ui/thinking-stream.md).
        self.settle_reasoning();
        // The open read/search group closes on what it held — the thought
        // just settled included — above the interrupt's remains
        // (docs/ui/tool-groups.md).
        let committing = self.commits_allowed();
        self.close_tool_group(committing, self.term.screen().width);
        self.abandon_inflight();
        // The interrupt takes a live foreground agent group down with it, and
        // a member may be the conversation on screen — the `Error` arm's rule
        // (`docs/agents/agent-view-streaming.md`). A no-op with no agent view open,
        // which is the ordinary case: Esc inside one leaves the view instead.
        let width = self.term.screen().width;
        let owed = self.begin_turn_agent_settle(width);
        // Interrupt only arises in the conversation view (overlay Esc returns
        // instead), so nothing here touches the alternate screen.
        let interrupted = self.app.interrupt_turn(elapsed);
        self.finish_turn_agent_settle(owed, width);
        match interrupted {
            Some(zeroth::app::InterruptedTurn::Undone) => {
                // Nothing had streamed and nothing was queued: the submission is
                // undone — interrupt_turn put the message back in the composer
                // and dropped it from history. Repaint scrollback without it (a
                // purge rebuild from the truncated history, like /clear — it
                // resets `render` itself). No notice, and no queue flush — the
                // empty queue is the undo's precondition — but background
                // completions held during the turn still settle
                // (docs/tools/background.md).
                self.repaint_conversation()?;
                if dispatch {
                    self.dispatch_after_turn();
                }
            }
            Some(zeroth::app::InterruptedTurn::Kept {
                partial,
                tools,
                notice,
                agents,
                summary,
            }) => {
                // Something streamed — keep it, commit the notice (None for a `!`
                // shell turn, whose `⎿ Interrupted by user` cell already says it
                // — req 2). A live agent group resolved as interrupted: stop its
                // subagent threads (the abandoned backend's own kill sweep may
                // still be parked in a network read) and commit its red tree cell
                // (docs/agents/agent-tool.md). `commit_turn_failure`'s
                // `render.finish(partial)` needs the committed-lines cache intact
                // (it flushes only the not-yet-committed tail), so reset `render`
                // AFTER it, never before.
                if let Some(group) = &agents {
                    for entry in &group.agents {
                        let _ = self.agent_registry.kill(&entry.id);
                        self.agent_clocks.remove(&entry.id);
                    }
                }
                self.commit_turn_failure(partial, &tools, agents, notice);
                self.view.render.reset();
                // The receipt closes the turn, under the notice: a background
                // completion no round read settles above it first — the
                // `StreamDone` order — and `dispatch_after_turn`'s own settle
                // below then finds nothing left (docs/input/interrupt.md).
                self.settle_bg_completions();
                if let Some(summary) = summary {
                    self.commit_turn_summary(summary);
                }
                // The user interrupted to send their queued follow-ups right away
                // (their spec; codex's submit-pending-steers-after-interrupt).
                // The front entry — the first queue — goes out now (a text batch
                // to the model, or a `!` command run locally); any later batches
                // iterate at the following turn-ends. Held background completions
                // settle first, like every turn end (docs/tools/background.md).
                if dispatch {
                    self.dispatch_after_turn();
                }
            }
            None => self.view.render.reset(),
        }
        Ok(())
    }

    /// `/clear`: the app state is already wiped (history, streaming buffer,
    /// running tool, status, queued backlog). Mid-turn it is also a **kill** — the
    /// user asked for a fresh slate, not a finished turn — so stop the backend and
    /// isolate it from the blank screen, then wipe every subsystem that outlives a
    /// turn and rebuild the screen from nothing.
    fn clear_conversation(&mut self) -> std::io::Result<()> {
        // `abandon_inflight` cancels + detaches the backend and hands back a
        // fresh channel (never join() on the loop — the interrupt-lag freeze):
        // any stale chunk or ToolStart the dying thread still emits lands on the
        // dropped receiver, so it can't repopulate the cleared state. See
        // docs/input/interrupt.md.
        self.abandon_inflight();
        self.view.render.reset();
        // …and the pictures the wiped conversation kept encoded for the wire
        // (`docs/architecture/memory.md`): nothing re-sends them now.
        zeroth::images::clear_attachments();
        // A fresh slate kills the background shells too (clear_conversation
        // already forgot them, so their Exited events find nothing and owe no
        // notice — docs/tools/background.md). Notes already posted for the wiped
        // conversation are dropped with it, else the next turn end would start a
        // phantom follow-up turn about them.
        self.registry.kill_all();
        self.registry.clear_notices();
        // …and the mid-turn queue: `App::clear_conversation` dropped the rows,
        // so the handle the dying backend was about to drain must go too, or
        // the next turn would open by reading a message from the conversation
        // that was just wiped (docs/input/queue.md).
        let _ = self.steer.take();
        // …and the subagents (docs/agents/agent-tool.md): the wiped roster drops their
        // late events, and the registry forgets them — the conversation that
        // knew their ids is gone, so nothing may list or resume them
        // (docs/agents/agent-tools.md).
        self.agent_registry.clear();
        self.agent_clocks.clear();
        self.agent_expiry.clear();
        self.view.full_reading.clear();
        self.view.full_tail = None;
        self.view.full_top = None;
        self.view.full_expanded.clear();
        self.view.full_anchor = None;
        self.view.full_select = None;
        self.view.full_press = None;
        self.view.full_hover = None;
        self.view.full_bar_drag = None;
        // …and the permission gate: `clear_conversation` dropped the prompt, the
        // cancelled threads reap themselves, so no unclaimed decision may linger
        // for the next turn (docs/state/permissions.md). The ask gate's board goes
        // the same way (docs/tools/ask.md).
        self.permissions.clear();
        self.ask.clear();
        // …and the task list: `clear_conversation` wiped App's snapshot; the
        // shared registry follows so the model's next `tasklist` agrees
        // (docs/tools/task-tools.md).
        self.sync_task_registry();
        // The session boundary for the hooks (docs/tools/hooks.md), Claude Code's
        // pair for its /clear: SessionEnd(clear) now — before the recorder
        // moves on, so it names the conversation that ended, and bounded, so
        // /clear cannot hang — and SessionStart(clear) in the background once
        // the new conversation has its id.
        self.models.end_conversation("clear");
        // A cleared conversation starts a fresh session file (codex's /new); the
        // old one keeps what it had (docs/state/resume.md). Re-seed the checkpoint chain
        // with a pristine snapshot of the current tree so a backtrack in the new
        // session can restore its starting point (docs/state/checkpoint.md).
        self.recorder.start_new();
        // A snapshot still running describes the conversation just wiped.
        self.invalidate_checkpoints();
        self.models.begin_conversation("clear");
        if let Some(commit) = self.checkpoints.snapshot("session start") {
            self.recorder
                .record_checkpoint(checkpoint::Checkpoint { after: 0, commit });
        }
        // Purge scrollback + clear the screen (codex's /clear), not just blank
        // the visible screen — the old conversation must be gone from scrollback
        // too, so scrolling up shows nothing.
        self.repaint_conversation()
    }

    /// A terminal resize. Repaint from history on ANY dimension change (codex
    /// redraws from source on every resize): a width change stales the wrapping,
    /// and a height change moves the screen contents out from under the tracked
    /// viewport row — repainting at a stale row leaves phantom input boxes
    /// behind. Purge scrollback + clear the screen first (codex's resize replay),
    /// rebuilding the whole conversation from history — the in-place overwrite
    /// otherwise left the emulator's own reflowed copy of the old content on
    /// screen, duplicating the TUI text. Only the inline view reflows; the overlay
    /// just redraws at the new size (reflowing would write the alternate screen).
    fn on_resize(&mut self, width: u16, height: u16) -> std::io::Result<()> {
        let size_changed = self.term.resized(width, height);
        if size_changed && self.app.view == View::Conversation {
            self.view.full_tab_press = None;
            // A resize under an open permission prompt reseats it with this
            // purge; the reflow notes it itself (`term.take_modal_scrolled`) so
            // the prompt's close purge-rebuilds too instead of stranding the box
            // (docs/state/permissions.md).
            self.repaint_active_view()?;
        } else if size_changed {
            // Under an overlay the inline view can't reflow (it would write the
            // alternate screen) — remember to purge-rebuild on return instead of
            // the usual in-place overwrite. A prompt open beneath the overlay is
            // reseated by that return's reflow, which notes it then
            // (`term.take_modal_scrolled`).
            self.overlay_resized = true;
        }
        self.burst.reset();
        self.frame.schedule_frame();
        Ok(())
    }

    /// A bracketed paste, routed to whatever field the user is looking at. The
    /// `/login` flow takes pastes (an API key is always pasted) and so does the
    /// `/model` filter (a model id is copied from a provider's dashboard far more
    /// often than it is typed), so both route to their own handler and the paste
    /// never reaches the composer draft underneath. The `/resume` picker's
    /// type-to-search accepts pastes too (codex normalizes them into the query);
    /// only the overlays ignore them, like typing there.
    fn on_paste(&mut self, pasted: &str) {
        match self.app.view {
            // The ask modal owns every key, so it owns pastes too: the live
            // entry field takes them (placeholder-collapsed over the
            // threshold), the option pages swallow them (docs/tools/ask.md).
            View::Conversation if self.app.ask().is_some() => {
                self.app.paste_into_ask(pasted);
            }
            View::Conversation if self.app.key_onboarding.is_some() => {
                self.app.paste_into_key_onboarding(pasted);
            }
            View::Conversation if self.app.model_picker.is_some() => {
                self.app.paste_into_model_filter(pasted);
            }
            // The `/settings` menu has a search field too, but nothing anyone
            // pastes is a setting name — swallow it rather than letting it
            // reach the composer draft underneath (docs/state/settings.md).
            View::Conversation if self.app.settings_picker.is_some() => {}
            // …and neither is a mascot name (docs/ui/mascot.md)…
            View::Conversation if self.app.mascot_picker.is_some() => {}
            // …nor a spinner style (docs/ui/spinner.md).
            View::Conversation if self.app.spinner_picker.is_some() => {}
            // …nor a theme name (docs/ui/theme.md)…
            View::Conversation if self.app.theme_picker.is_some() => {}
            // …and the read-only `/donate` page has no field at all
            // (docs/ui/donate.md)…
            View::Conversation if self.app.donate_picker.is_some() => {}
            // …nor does the `/export` page, its sibling (docs/ui/export.md)…
            View::Conversation if self.app.export_picker.is_some() => {}
            // …nor the `/context` page (docs/architecture/context-usage.md).
            View::Conversation if self.app.context_usage.is_some() => {}
            // …nor the `/usage` page (docs/providers/usage.md).
            View::Conversation if self.app.usage_page.is_some() => {}
            // The `/secrets` form takes pastes — a credential is pasted far
            // more often than typed — into its focused field, never the
            // composer draft underneath (docs/state/secrets.md).
            View::Conversation if self.app.secrets_page.is_some() => {
                self.app.on_paste(pasted);
            }
            View::Conversation => {
                self.app.on_paste(pasted);
                // The paste may have changed the active `@token`.
                self.dispatch_file_search();
            }
            View::ResumePicker => self.app.paste_into_resume_search(pasted),
            View::DiffReview => self.app.paste_into_diff_search(pasted),
            View::ToolOutput | View::ContextDebug => {}
        }
        self.burst.reset();
        self.frame.schedule_frame();
    }

    /// Esc-Esc backtrack confirmed: history is already truncated to the chosen
    /// message and the composer prefilled. Reset the code to that point's
    /// checkpoint (`docs/state/checkpoint.md`) before returning to the inline view.
    ///
    /// The restore target is the latest checkpoint at or before the new history
    /// length; the current tree is backed up first (recoverable via the store's
    /// reflog). The loop-bottom `recorder.sync` sees history shrink and rewrites
    /// the file, dropping the rewound-away checkpoints in lockstep.
    fn confirm_backtrack(&mut self) -> std::io::Result<()> {
        // The history was rewound: a snapshot still in flight is about a
        // conversation that no longer exists.
        self.invalidate_checkpoints();
        // The reset runs on a worker (docs/state/checkpoint.md); its outcome is
        // toasted when it lands, so this path does not wait for `git`.
        let _restoring = self.restore_checkpoint_at(self.app.history.len());
        // `App::confirm_backtrack` already rewound the checklist to the
        // snapshot at the cut; the shared registry follows
        // (docs/tools/task-tools.md).
        self.sync_task_registry();
        // Consume the overlay-resized flag (a resize under the overlay must not
        // leak to the next return); we purge unconditionally below anyway.
        self.overlay_resized = false;
        self.term.exit_overlay()?;
        // (The transcript cache needs no explicit clear: the truncation bumped
        // the history generation, so the loop-bottom warm rebuilds the kept
        // prefix.)
        // Backtrack TRUNCATES history, so lines already on screen or queued
        // for scrollback may belong to the dropped exchange. Purge-rebuild
        // like /resume and resize (the reflow drops the queue): the truncated
        // conversation replaces the screen AND scrollback cleanly
        // (invariant 3).
        self.repaint_conversation()?;
        Ok(())
    }

    /// Sync the shared task registry to `App`'s checklist — after any history
    /// rewind (`/clear`, a `/resume` load, a backtrack truncation), so the
    /// model's next `tasklist` call agrees with what the strip shows
    /// (`docs/tools/task-tools.md`). The forward direction never needs this: a task
    /// call *starts* at the registry and reaches `App` on its `TaskCall`
    /// event.
    pub(crate) fn sync_task_registry(&self) {
        self.task_registry.replace(self.app.tasks().clone());
    }
}

/// The platform's opener for a URL or `file://` target — the reference's trio
/// (`open`/`xdg-open`/`cmd start`), spawned without waiting (`docs/ui/full-screen.md`).
#[cfg(target_os = "macos")]
fn opener(url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("open");
    command.arg(url);
    detach_stdio(&mut command);
    command
}

#[cfg(windows)]
fn opener(url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("cmd");
    command.args(["/C", "start", "", url]);
    detach_stdio(&mut command);
    command
}

#[cfg(all(unix, not(target_os = "macos")))]
fn opener(url: &str) -> std::process::Command {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url);
    detach_stdio(&mut command);
    command
}

/// A missing handler's complaint must not land on the TUI's terminal: the
/// opener inherits this process's stdio by default, and one line of
/// `xdg-open: no method available` written over the live region is a flash
/// nobody asked for.
fn detach_stdio(command: &mut std::process::Command) {
    use std::process::Stdio;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
}
