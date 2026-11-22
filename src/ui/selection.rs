//! Drag-selection on the screen-owning frame: highlighting exactly the cells
//! that will be copied, and copying whole graphemes
//! (`docs/ui/full-screen.md` *Selection semantics*).
//!
//! The selection's anchors are display cells — the terminal reports the mouse
//! in columns — while the copy is text. Two rules keep the pair honest:
//!
//! - a **grapheme** is the smallest unit, taken or left whole, so a ZWJ emoji
//!   or a combining accent never arrives split;
//! - **the highlight and the copy agree**: a grapheme is selected, reversed on
//!   screen and copied, on the same test — its first column lies in the range.
//!   The old code reversed every span it touched, so copying three characters
//!   into a twenty-character span flashed the whole span as selected.

use super::wrap::cols;
use super::*;

/// Is the grapheme starting at column `x` inside the selection `[from, to)`?
///
/// Decided by where it **starts**, not by overlap: a wide glyph the range
/// begins in the middle of cannot be half-selected, so it is left out — and
/// because the highlight and [`slice`] ask this same question, they can never
/// disagree about it.
fn selected(x: usize, from: usize, to: usize) -> bool {
    x >= from && x < to
}

/// `line` with the cells in display columns `[from, to)` reversed — the
/// selection's own highlight.
///
/// Spans are split at the selection's boundaries, so a span that the range
/// crosses comes back as up to three: before, inside, after. The reverse is
/// added to the existing style, so a selected link keeps its underline and a
/// selected dim row stays dim.
#[must_use]
pub fn highlight(line: &Line<'static>, from: u16, to: u16) -> Line<'static> {
    let from = usize::from(from);
    let to = usize::from(to);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut x = 0usize;
    for span in &line.spans {
        let mut run = String::new();
        let mut run_selected: Option<bool> = None;
        for grapheme in span.content.graphemes(true) {
            let inside = selected(x, from, to);
            match run_selected {
                Some(previous) if previous == inside => run.push_str(grapheme),
                Some(previous) => {
                    spans.push(styled(span, std::mem::take(&mut run), previous));
                    run.push_str(grapheme);
                    run_selected = Some(inside);
                }
                None => {
                    run.push_str(grapheme);
                    run_selected = Some(inside);
                }
            }
            x += cols(grapheme);
        }
        if let Some(inside) = run_selected {
            spans.push(styled(span, run, inside));
        }
    }
    // The line's own style and alignment survive the split: a centred or
    // styled row must not be re-flown by selecting part of it.
    let mut picked = Line::from(spans).style(line.style);
    if let Some(alignment) = line.alignment {
        picked = picked.alignment(alignment);
    }
    picked
}

/// One run of a span, reversed when it is inside the selection.
fn styled(span: &Span<'static>, text: String, inside: bool) -> Span<'static> {
    let style = if inside {
        span.style.add_modifier(Modifier::REVERSED)
    } else {
        span.style
    };
    Span::styled(text, style)
}

/// The text of `line` in display columns `[from, to)`, cut on grapheme
/// boundaries and with its trailing blanks trimmed.
///
/// The same [`selected`] test [`highlight`] paints with, so what is copied is
/// what was shown as copied.
#[must_use]
pub fn slice(line: &Line<'static>, from: u16, to: u16) -> String {
    let from = usize::from(from);
    let to = usize::from(to);
    let mut out = String::new();
    let mut x = 0usize;
    for span in &line.spans {
        for grapheme in span.content.graphemes(true) {
            if selected(x, from, to) {
                out.push_str(grapheme);
            }
            x += cols(grapheme);
        }
    }
    out.trim_end().to_string()
}

/// The selected text over a run of rows, `(row, from_column)` to
/// `(row, to_column)` inclusive of the end cell — the drag's two anchors in
/// either order.
///
/// Rows are joined with newlines and blank leading and trailing rows dropped,
/// so a drag that starts in the gutter copies the words rather than a frame of
/// spaces. `None` when the result is nothing but blanks.
#[must_use]
pub fn text<'a>(
    lines: impl Iterator<Item = &'a Line<'static>>,
    a: (usize, u16),
    b: (usize, u16),
) -> Option<String> {
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    let mut out = Vec::new();
    for (row, line) in lines.enumerate() {
        if row < start.0 {
            continue;
        }
        if row > end.0 {
            break;
        }
        let from = if row == start.0 { start.1 } else { 0 };
        let to = if row == end.0 {
            end.1.saturating_add(1)
        } else {
            u16::MAX
        };
        out.push(slice(line, from, to));
    }
    while out.first().is_some_and(String::is_empty) {
        out.remove(0);
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    Some(out.join("\n")).filter(|text| !text.trim().is_empty())
}
