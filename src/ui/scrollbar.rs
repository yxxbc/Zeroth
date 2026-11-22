//! The screen-owning conversation's scrollbar (`docs/ui/full-screen.md`).
//!
//! In the inline viewport the terminal draws the scrollbar; here the screen is
//! the app's, so it draws its own — otherwise a wheel or PageDown moves the
//! conversation with nothing on screen saying where in it you are.
//!
//! The bar owns the conversation's **last column**, reserved whether or not it
//! paints: a column that appeared only once the content overflowed would
//! re-wrap every row the moment the first screenful passed, which is exactly
//! when the reader is following the text.
//!
//! Pure: [`rows`] turns the scroll state into one cell per row, so the shape of
//! the thumb (its size, its clamped ends) is unit-tested without a terminal;
//! [`top_for_row`] and [`top_after_drag`] are its inverse for the pointer, so a
//! track click and a thumb drag map to a scroll position the same way without
//! a terminal too.

use super::theme::*;
use super::*;

/// One cell of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cell {
    /// Nothing to draw — the conversation fits, or this row is outside the bar.
    Blank,
    /// The unread part of the conversation.
    Track,
    /// The screenful on show.
    Thumb,
}

/// The geometry both the painter and the pointer mapping share: a `height`-row
/// track, the thumb's size, the travel left for it, and the largest top.
struct Track {
    height: usize,
    size: usize,
    travel: usize,
    max_top: usize,
}

/// The track for a conversation of `len` rows showing `window` of them, or
/// `None` when there is nothing to scroll (the one rule [`rows`] and the
/// pointer mappings agree on).
fn track(len: usize, window: usize, height: u16) -> Option<Track> {
    let height = usize::from(height);
    if height == 0 || window == 0 {
        return None;
    }
    let max_top = len.saturating_sub(window);
    if max_top == 0 {
        return None;
    }
    let size = ((height * window) / len.max(1)).clamp(1, height);
    Some(Track {
        height,
        size,
        travel: height - size,
        max_top,
    })
}

/// The thumb's first row and length on a `height`-row track for `top` — what a
/// press needs to tell a grab on the thumb from a click on the track.
#[must_use]
pub fn thumb(len: usize, window: usize, top: usize, height: u16) -> Option<(usize, usize)> {
    let track = track(len, window, height)?;
    let top = top.min(track.max_top);
    // Travel 0 means the thumb fills the track (a one-row window): there is
    // nowhere to put a position, so the thumb stays at the start.
    let offset = if track.travel == 0 {
        0
    } else {
        (top * track.travel + track.max_top / 2) / track.max_top
    };
    Some((offset, track.size))
}

/// The bar for a conversation of `len` rows showing `window` of them from
/// `top`, in a `height`-row column.
///
/// The thumb is at least one cell (a long conversation still says "you are
/// here"), grows to a full track when nothing is off screen, and its travel is
/// the remaining track — so `top == 0` starts it at the first row and
/// `top == len - window` ends it at the last, with no half-cell at either end.
/// `None` when there is nothing to scroll: the column then stays blank rather
/// than painting a full-length thumb that says nothing.
#[must_use]
pub fn rows(len: usize, window: usize, top: usize, height: u16) -> Option<Vec<Cell>> {
    let track = track(len, window, height)?;
    let top = top.min(track.max_top);
    let offset = if track.travel == 0 {
        0
    } else {
        (top * track.travel + track.max_top / 2) / track.max_top
    };
    let mut cells = vec![Cell::Track; track.height];
    for cell in cells.iter_mut().skip(offset).take(track.size) {
        *cell = Cell::Thumb;
    }
    Some(cells)
}

/// The conversation top for a click on the track's `row`: the pointer's row
/// becomes the thumb's first row, so a click near the bottom lands at the tail.
///
/// `None` when there is nothing to scroll — the bar is not painted and the
/// column takes no click (`Session::full_bar_row`).
#[must_use]
pub fn top_for_row(len: usize, window: usize, height: u16, row: u16) -> Option<usize> {
    let track = track(len, window, height)?;
    if track.travel == 0 {
        return Some(0);
    }
    let start = usize::from(row).min(track.travel);
    Some((start * track.max_top + track.travel / 2) / track.travel)
}

/// The conversation top after dragging a grabbed thumb `delta` track rows,
/// starting from `top`.
///
/// The travel is proportional — a short thumb covers many rows of conversation
/// per row of track — and a `delta` of zero keeps `top` exactly, so a press
/// with no movement never jumps under the pointer. Both ends clamp.
#[must_use]
pub fn top_after_drag(
    len: usize,
    window: usize,
    height: u16,
    top: usize,
    delta: isize,
) -> Option<usize> {
    let track = track(len, window, height)?;
    let top = top.min(track.max_top);
    if track.travel == 0 {
        return Some(0);
    }
    // Round half away from zero, so a drag of `travel` rows is the whole
    // conversation in either direction.
    let steps = (delta.unsigned_abs() as i128 * track.max_top as i128 + (track.travel / 2) as i128)
        / track.travel as i128;
    let sign = if delta < 0 { -1 } else { 1 };
    let target = top as i128 + sign * steps;
    Some(target.clamp(0, track.max_top as i128) as usize)
}

/// Paint a bar down the right edge of `area`, which must be one column wide.
/// `top` is the cell the bar starts on, for a bar shorter than the area.
///
/// The thumb is the accent, the track the rule's ink — the footer's own
/// two-step contrast, so the bar reads as chrome rather than content.
pub fn render(area: Rect, buf: &mut Buffer, cells: &[Cell], top: u16) {
    for (row, cell) in cells.iter().enumerate() {
        let Ok(row) = u16::try_from(row) else { break };
        let y = top.saturating_add(row);
        if y >= area.y.saturating_add(area.height) || area.width == 0 {
            break;
        }
        let (glyph, style) = match cell {
            Cell::Blank => (" ", Style::new()),
            // The track stays blank: a drawn rule here lands right beside the
            // sidebar's seam and reads as a second divider. The thumb alone
            // carries the position, the way an overlay scrollbar does.
            Cell::Track => (" ", Style::new()),
            Cell::Thumb => (SCROLLBAR_THUMB, Style::new().fg(scrollbar_thumb_color())),
        };
        buf[(area.x, y)].set_symbol(glyph).set_style(style);
    }
}

/// How many columns the bar takes from the conversation: always one in the
/// screen-owning mode, so the wrap width never moves under the reader.
pub const COLUMNS: u16 = 1;
