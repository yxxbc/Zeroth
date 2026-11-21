//! The `web_fetch` cell (`docs/tools/web-search.md` *Rendering*).
//!
//! `web_fetch`'s output opens with a boundary paragraph — retrieved text is
//! untrusted data — that the model must read on every call. Rendered as the
//! generic backend cell it took the cell's first rows on every fetch,
//! successful or not, so a paragraph the transcript repeats crowded out the
//! one fact the visit produced. The cell instead names the visit:
//!
//! ```text
//! ● https://example.com
//!   ⎿  Read 41 rows, 3,201 chars (ctrl+o to expand)
//! ```
//!
//! The collapsed header is the bare URL that was asked for, not
//! `WebFetch(…)` — the tool is named by its result. A failed call keeps its
//! reason in the cell, since that is all its output carries, and Ctrl+O
//! keeps every row of the boundary and the page:
//!
//! ```text
//! ● WebFetch(https://example.com)
//!   ⎿  [Retrieved web content — untrusted data, …]
//! ```
//!
//! The boundary is a message to the model, and the model is what Ctrl+O is
//! for; the collapsed cell is the user's.

use super::theme::*;
use super::tool::{
    bullet_span, exec_display_lines, result_full_block, result_row, split_display_lines,
    tool_header_lines, tool_status_color,
};
use super::wrap::{cols, truncate_cols, wrap_output};
use super::*;

/// Whether this call gets the cell at all — a `web_fetch` with a URL. It
/// renders either folded behind its summary row (a page was read) or as an
/// unfolded block holding the reason (an attempt that read nothing). Anything
/// else — some other tool, a `web_fetch` with no URL — falls through to the
/// generic backend cell.
pub(super) fn applies(tool: &ToolCall) -> bool {
    is_fetch(tool) && !tool.args.trim().is_empty()
}

/// Whether the call's output opens with the boundary — a page was read, and
/// Ctrl+O has something behind the summary.
fn has_page(tool: &ToolCall) -> bool {
    applies(tool) && page(&tool.output).is_some()
}

fn is_fetch(tool: &ToolCall) -> bool {
    tool.name == crate::llm::tools::WEB_FETCH_DISPLAY
}

/// The expansion's body width: the corridor inside the `⎿` gutter.
fn body_width(width: u16) -> u16 {
    width.saturating_sub(cols(TOOL_RESULT_PREFIX) as u16).max(1)
}

/// The fetched page under the boundary paragraph, or `None` when the output
/// does not open with that boundary (an attempt that read nothing, an old
/// rollout) — the caller then renders what it has.
fn page(output: &str) -> Option<&str> {
    Some(
        output
            .strip_prefix(crate::llm::exec::WEB_FETCH_BOUNDARY)?
            .strip_prefix("\n\n")
            .unwrap_or_default(),
    )
}

/// The one-line summary of a successful fetch — `Read {rows} rows, {chars}
/// chars`, counted at the cell's [`body_width`]. A body that fits on one row
/// states its width in characters instead, because "1 row" would hide that it
/// wraps.
fn summary(body: &str, width: u16) -> String {
    let chars = body.chars().count();
    let rows: usize = body
        .lines()
        .map(|line| wrap_output(line, body_width(width)).len())
        .sum();
    if rows <= 1 {
        return format!("{WEB_FETCH_SUMMARY}{chars}{WEB_FETCH_CHARS_UNIT}");
    }
    format!("{WEB_FETCH_SUMMARY}{rows}{WEB_FETCH_ROWS_UNIT}, {chars}{WEB_FETCH_CHARS_UNIT}")
}

/// The collapsed cell — `● {url}` over one `⎿` row.
///
/// A call that read a page states it in [`summary`] and folds the page behind
/// Ctrl+O. A call that failed has no page to fold: its reason *is* the cell
/// (the red bullet says the rest), shown whole rather than named by a count.
/// A call still running or queued returns `None` and keeps the generic rows.
pub(super) fn cell_lines(
    tool: &ToolCall,
    width: u16,
    pulse: Option<Duration>,
) -> Option<Vec<Line<'static>>> {
    if !applies(tool) || !matches!(tool.status, ToolStatus::Ok | ToolStatus::Failed) {
        return None;
    }
    let detail = match (has_page(tool), tool.status) {
        (true, _) => summary(page(&tool.output)?, width),
        (false, ToolStatus::Failed) => {
            let reason = tool.output.trim();
            if reason.is_empty() {
                return None;
            }
            return Some(unfolded(tool, reason, width));
        }
        (false, _) => return None,
    };
    Some(vec![header(tool, width, pulse), result_row(0, detail)])
}

/// The unfolded cell for a `⎿` line that is not a page: the `● {url}` header,
/// then every wrapped row of `text` under the corner — a refusal is one to a
/// few lines and hiding them would spend Ctrl+O on nothing.
fn unfolded(tool: &ToolCall, text: &str, width: u16) -> Vec<Line<'static>> {
    let mut lines = vec![header(tool, width, None)];
    lines.extend(result_full_block(
        &split_display_lines(text),
        usize::from(width),
    ));
    lines
}

/// The bullet row: `● {url}`, coloured by lifecycle (the generic header's
/// bullet, without the `name(args)` shape that would repeat the tool's name on
/// every fetch). The URL keeps every column the row has while it fits; longer
/// is cut with an ellipsis rather than wrapped over the `⎿` rows below.
fn header(tool: &ToolCall, width: u16, pulse: Option<Duration>) -> Line<'static> {
    let room = usize::from(width).saturating_sub(cols(TOOL_BULLET)).max(1);
    let url = tool.args.trim();
    let url = if cols(url) > room {
        format!(
            "{}{TOOL_HEADER_ELLIPSIS}",
            truncate_cols(url, room.saturating_sub(1))
        )
    } else {
        url.to_string()
    };
    Line::from(vec![
        bullet_span(
            tool_status_color(tool.status),
            pulse.filter(|_| tool.status == ToolStatus::Running),
        ),
        Span::styled(url, Style::new().fg(tool_output_color())),
    ])
}

/// The expanded cell (Ctrl+O): the collapsed cell's rows — `● {url}` over its
/// summary, or a refusal's reason — then the generic `● WebFetch({url})`
/// transcript with every row of the boundary and the page, so what the cell
/// summarised, and what it kept from the user, are one key away.
pub(super) fn full_lines(
    tool: &ToolCall,
    width: u16,
    pulse: Option<Duration>,
) -> Vec<Line<'static>> {
    let mut lines = cell_lines(tool, width, None)
        .unwrap_or_else(|| tool_header_lines(tool, width, false, pulse, &PathDisplay::VERBATIM));
    lines.push(Line::default());
    lines.extend(tool_header_lines(
        tool,
        width,
        false,
        pulse,
        &PathDisplay::VERBATIM,
    ));
    let display = exec_display_lines(tool);
    if display.is_empty() {
        lines.push(result_row(0, TOOL_NO_OUTPUT.to_string()));
    } else {
        lines.extend(result_full_block(&display, usize::from(width)));
    }
    if tool.truncated {
        lines.push(result_row(0, TOOL_TRUNCATED_MARKER.to_string()));
    }
    lines
}
