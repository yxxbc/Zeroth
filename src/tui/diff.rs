//! Off-thread Git reads and the alternate-screen review lifecycle.

use std::io;

use zeroth::app::{ToastKind, View};
use zeroth::git_diff::DiffSnapshot;
use zeroth::ui;

use super::Session;

pub(crate) type DiffResult = (u64, DiffUpdate);

pub(crate) enum DiffUpdate {
    Repository(Result<(), String>),
    Snapshot(Result<DiffSnapshot, String>),
}

impl Session<'_> {
    pub(crate) fn load_diff_review(&mut self) {
        self.conversation.diff_generation = self.conversation.diff_generation.wrapping_add(1);
        let generation = self.conversation.diff_generation;
        let tx = self.conversation.worker_tx.clone();
        let cwd = self.conversation.cwd.clone();
        let secrets = self.secrets.clone();
        std::thread::spawn(move || {
            let result = super::git_diff_loader::load(&cwd).map(|mut snapshot| {
                // A file the agent wrote a credential into holds the value on
                // disk; the review shows the placeholder, like every other
                // view of the session (docs/state/secrets.md).
                secrets.with(|store| {
                    for line in snapshot.files.iter_mut().flat_map(|file| &mut file.lines) {
                        if let std::borrow::Cow::Owned(text) = store.redact(&line.text) {
                            line.text = text;
                        }
                    }
                });
                snapshot
            });
            let _ = tx.send(super::conversation::WorkerEvent::DiffResult((
                generation,
                DiffUpdate::Snapshot(result),
            )));
        });
    }

    pub(crate) fn on_diff_result(&mut self, (generation, update): DiffResult) -> io::Result<()> {
        if generation != self.conversation.diff_generation {
            return Ok(());
        }
        let result = match update {
            DiffUpdate::Repository(result) => {
                if self.conversation.app.view != View::Conversation
                    || self.conversation.app.modal_open()
                {
                    return Ok(());
                }
                match result {
                    Ok(()) => {
                        self.conversation.app.open_diff_review();
                        self.screen.enter_overlay()?;
                        self.load_diff_review();
                        self.draw_diff_review()?;
                    }
                    Err(error) => self.toast(error, ToastKind::Info),
                }
                self.frame.schedule_frame();
                return Ok(());
            }
            DiffUpdate::Snapshot(result) => result,
        };
        if self.conversation.app.view != View::DiffReview {
            return Ok(());
        }
        match result {
            Ok(snapshot) => self.conversation.app.set_diff_snapshot(snapshot),
            Err(error) => {
                if self
                    .conversation
                    .app
                    .diff_review()
                    .is_some_and(|review| review.snapshot.is_some())
                {
                    self.conversation.app.set_diff_error(error);
                } else {
                    self.conversation.app.close_diff_review();
                    self.toast(error, ToastKind::Error);
                    self.close_diff_review()?;
                }
            }
        }
        self.frame.schedule_frame();
        Ok(())
    }

    pub(crate) fn close_diff_review(&mut self) -> io::Result<()> {
        self.conversation.diff_generation = self.conversation.diff_generation.wrapping_add(1);
        self.screen.exit_overlay()?;
        self.overlay_return_repaint()
    }

    pub(crate) fn draw_diff_review(&mut self) -> io::Result<()> {
        self.conversation
            .app
            .settle_diff_scroll(ui::diff_body_rows(self.screen.screen()));
        let app = &self.conversation.app;
        self.screen
            .draw_overlay(|area, buf| ui::render_diff_view(app, area, buf))
    }
}

impl super::conversation::Conversation {
    pub(crate) fn open_diff_review(&mut self) {
        // Stay inline during validation. An unavailable repository should
        // produce a quiet toast without ever flashing the alternate screen.
        self.diff_generation = self.diff_generation.wrapping_add(1);
        let generation = self.diff_generation;
        let tx = self.worker_tx.clone();
        let cwd = self.cwd.clone();
        std::thread::spawn(move || {
            let result = super::git_diff_loader::repository_root(&cwd).map(|_| ());
            let _ = tx.send(super::conversation::WorkerEvent::DiffResult((
                generation,
                DiffUpdate::Repository(result),
            )));
        });
    }
}
