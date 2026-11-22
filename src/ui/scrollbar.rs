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
//! the thumb (its size, its clamped ends) is unit-tested without a terminal.

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
    let height = usize::from(height);
    if height == 0 || window == 0 {
        return None;
    }
    let max_top = len.saturating_sub(window);
    if max_top == 0 {
        return None;
    }
    let thumb = ((height * window) / len.max(1)).clamp(1, height);
    let travel = height - thumb;
    let top = top.min(max_top);
    let offset = (top * travel + max_top / 2) / max_top;
    let mut cells = vec![Cell::Track; height];
    for cell in cells.iter_mut().skip(offset).take(thumb) {
        *cell = Cell::Thumb;
    }
    Some(cells)
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
