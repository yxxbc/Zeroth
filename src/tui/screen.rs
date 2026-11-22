//! The session's **one screen**: which viewport contract the TUI is painting
//! through (`docs/architecture/core-host-split.md` Phase 2).
//!
//! `Session` used to name `InlineViewport` directly and ask it
//! `fullscreen()` at each call site. The three modes share that viewport (the
//! full-screen mode is a flag on it), so the seam names the **mode** once and
//! dispatches: a caller asks the screen to paint a region or a frame, and the
//! screen decides which of the viewport's entry points that means. The
//! remaining viewport methods (commits, the cursor query, overlays) are
//! reached through `Deref` until Phase 3 moves them behind methods of their
//! own.
//!
//! The enum is deliberately not a trait: there are exactly three arms, the
//! dispatch is one `match`, and a trait object would buy a vtable and no
//! clarity. This is the seam the decision page asked not to half-build — the
//! inline, full-screen and headless paths are all named here.

use std::io;
use std::ops::{Deref, DerefMut};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use zeroth::app::App;
use zeroth::term::InlineViewport;
use zeroth::ui::ScrollbarCell;

/// Which of the three contracts a [`Screen`] is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScreenMode {
    /// The conversation lives in the terminal's own scrollback; the live
    /// region is a bottom-anchored growable block (`docs/ui/full-screen.md`).
    Inline,
    /// The frame owns the alternate screen: the conversation is painted above
    /// the live region, with its own scrolling and mouse.
    Full,
    /// No terminal at all (`-p`): every paint is a no-op.
    Headless,
}

/// One session's screen: the viewport and the mode it is in.
pub(crate) struct Screen<'t> {
    viewport: &'t mut InlineViewport,
    mode: ScreenMode,
}

impl<'t> Screen<'t> {
    /// Wrap `viewport` in the mode it was built for.
    #[must_use]
    pub(crate) fn new(viewport: &'t mut InlineViewport) -> Self {
        let mode = if viewport.is_headless() {
            ScreenMode::Headless
        } else if viewport.fullscreen() {
            ScreenMode::Full
        } else {
            ScreenMode::Inline
        };
        Self { viewport, mode }
    }

    /// Whether the conversation is painted into an owned frame.
    #[must_use]
    pub(crate) const fn is_full(&self) -> bool {
        matches!(self.mode, ScreenMode::Full)
    }

    /// Whether there is no terminal at all.
    #[must_use]
    pub(crate) const fn is_headless(&self) -> bool {
        matches!(self.mode, ScreenMode::Headless)
    }

    /// Whether a mouse event can mean anything here: only the screen-owning
    /// mode has a hit map and a pointer (`docs/ui/full-screen.md`).
    #[must_use]
    pub(crate) const fn accepts_pointer(&self) -> bool {
        self.is_full()
    }

    /// Paint the inline live region: used by the inline and headless paths.
    pub(crate) fn draw_region(
        &mut self,
        height: u16,
        render: impl FnOnce(Rect, &mut Buffer),
        app: &App,
    ) -> io::Result<()> {
        self.viewport.draw(height, render, app)
    }

    /// Paint a whole frame from a conversation tail: the screen-owning mode's
    /// entry point. `tail`/`scrollbar` are ignored by the other modes (they
    /// have no frame; the caller builds them only when [`is_full`](Self::is_full)).
    pub(crate) fn draw_frame(
        &mut self,
        tail: &[Line<'static>],
        height: u16,
        scrollbar: &[ScrollbarCell],
        render: impl FnOnce(Rect, &mut Buffer),
        app: &App,
    ) -> io::Result<()> {
        self.viewport
            .draw_full_with_tail(tail, height, scrollbar, render, app)
    }
}

impl Deref for Screen<'_> {
    type Target = InlineViewport;

    fn deref(&self) -> &Self::Target {
        self.viewport
    }
}

impl DerefMut for Screen<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.viewport
    }
}
