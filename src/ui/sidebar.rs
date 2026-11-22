//! The screen-owning mode's **sidebar** (`docs/ui/sidebar.md`).
//!
//! A column at the screen's left edge carrying what the inline mode spreads
//! between the banner and the footer: the brand, the session's facts, and what
//! is running. Owning the screen is what makes it possible — an inline row,
//! once painted, belongs to the terminal and cannot be given a column.
//!
//! **Modular by construction.** A sidebar section is one entry in
//! [`SECTIONS`] — an id and a builder that turns `App` into rows — so adding
//! one (a diff summary, a git status, a token ledger) is a new function and a
//! new entry, never an edit to the layout. Sections that have nothing to say
//! return no rows and are skipped whole, so the column never shows an empty
//! heading.
//!
//! **Responsive, the way a terminal sidebar is.** Wide screens get the full
//! column, a middle band narrows it, anything narrower hides it and the
//! conversation keeps the whole width ([`width`]). The bands are constants, so
//! the policy is one place.

use super::theme::*;
use super::wrap::{cols, truncate_cols, wrap_output};
use super::*;

use crate::images;

/// One section of the sidebar.
pub struct Section {
    /// Stable id — what a future settings row, a test or a log names it by.
    pub id: &'static str,
    /// The section's rows for a `width`-column sidebar. Empty means "nothing
    /// to say right now": the section is dropped, heading and all.
    pub rows: fn(&App, u16) -> Vec<Line<'static>>,
}

/// Every section, **in the order they stack**. Adding one is a new `fn` below
/// plus an entry here; nothing else in the file knows about it.
pub const SECTIONS: &[Section] = &[
    Section {
        id: "brand",
        rows: brand_rows,
    },
    Section {
        id: "session",
        rows: session_rows,
    },
    Section {
        id: "running",
        rows: running_rows,
    },
];

/// Below this screen width the sidebar is hidden — the conversation keeps
/// every column.
pub const MIN_SCREEN_COLS: u16 = 96;

/// At or above this width the sidebar takes its full width; between
/// [`MIN_SCREEN_COLS`] and here it narrows to [`NARROW_COLS`].
pub const WIDE_SCREEN_COLS: u16 = 112;

/// The column's width on a wide screen.
pub const WIDE_COLS: u16 = 30;

/// The column's width in the middle band — 80-column terminals and the
/// split-pane cases land here.
pub const NARROW_COLS: u16 = 24;

/// How wide the sidebar is on a `screen_width`-column terminal: `0` hides it.
///
/// Three bands, the policy every terminal sidebar follows: wide screens get
/// the full column, a middle band a narrowed one, and narrower than
/// `MIN_SCREEN_COLS` nothing at all. A hidden sidebar costs the conversation
/// nothing, which is the point of the middle band existing rather than the
/// column shrinking to nothing.
#[must_use]
pub const fn width(screen_width: u16) -> u16 {
    if screen_width >= WIDE_SCREEN_COLS {
        WIDE_COLS
    } else if screen_width >= MIN_SCREEN_COLS {
        NARROW_COLS
    } else {
        0
    }
}

/// The seam's column, on the sidebar's left edge — the conversation is to its
/// left, the shell's own prompt and the composer are at column 0.
pub const SIDEBAR_SEAM_COLS: u16 = 1;

/// The sidebar's rows: every section that has something to say, a blank row
/// between them, cut to `height` rows from the top.
///
/// A section that does not fit is cut, not skipped: the stack is ordered by
/// importance, so the brand is never the thing that disappears.
#[must_use]
pub fn lines(app: &App, width: u16, height: u16) -> Vec<Line<'static>> {
    // One column of the width is the seam, so the text wraps inside what is
    // left — a section measures `width`, which is the body's.
    let width = width.saturating_sub(SIDEBAR_SEAM_COLS);
    let mut out: Vec<Line<'static>> = Vec::new();
    for section in SECTIONS {
        let rows = (section.rows)(app, width);
        if rows.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(Line::default());
        }
        out.extend(rows);
    }
    out.truncate(usize::from(height));
    out
}

/// Paint the sidebar down `area`, framed by a rule on its right edge so it
/// reads as a column rather than as the conversation's margin.
pub fn render(area: Rect, buf: &mut Buffer, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let rows = lines(app, area.width, area.height);
    // The body sits one column in from the seam: the sidebar is the screen's
    // **right** edge, so its seam is its left cell and the text follows it.
    let body = Rect::new(
        area.x.saturating_add(SIDEBAR_SEAM_COLS),
        area.y,
        area.width.saturating_sub(SIDEBAR_SEAM_COLS),
        area.height,
    );
    Paragraph::new(rows).render(body, buf);
    for row in area.y..area.y.saturating_add(area.height) {
        buf[(area.x, row)]
            .set_symbol(SIDEBAR_SEAM)
            .set_style(Style::new().fg(border_color()));
    }
}

// ---------------------------------------------------------------------------
// The sections
// ---------------------------------------------------------------------------

/// A dim, one-row heading over a section's body.
fn heading(text: &str, width: u16) -> Line<'static> {
    Line::from(Span::styled(
        truncate_cols(text, usize::from(width)),
        Style::new().fg(sidebar_heading_color()),
    ))
}

/// The brand: the mascot's graphic when the terminal can show it, then the
/// name, the version and the one-line slogan.
///
/// The art is the banner's own (`images::place_in`, the same placement the
/// startup banner makes), sized to fit the narrow band. When the terminal has
/// no graphics — or the sidebar is too narrow even for that — the section
/// keeps the words and drops the picture rather than degrading to a box of
/// question marks.
fn brand_rows(app: &App, width: u16) -> Vec<Line<'static>> {
    let mut rows = mascot_rows(width);
    rows.push(Line::from(vec![
        Span::styled(
            SIDEBAR_NAME,
            Style::new()
                .fg(sidebar_name_color())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" v{}", env!("CARGO_PKG_VERSION")),
            Style::new().fg(sidebar_meta_color()),
        ),
    ]));
    for row in wrap_output(SIDEBAR_SLOGAN, width) {
        rows.push(Line::from(Span::styled(
            row,
            Style::new().fg(sidebar_slogan_color()),
        )));
    }
    // The offline demo's one action, where the banner puts it too.
    if app.offline_demo() {
        rows.push(Line::from(vec![
            Span::styled(
                format!("{SIDEBAR_CONNECT} "),
                Style::new().fg(sidebar_meta_color()),
            ),
            Span::styled("/login", Style::new().fg(sidebar_accent_color())),
        ]));
    }
    rows
}

/// The mascot's graphic header rows, or none when this terminal cannot show
/// one (or the column is too narrow for the icon).
fn mascot_rows(width: u16) -> Vec<Line<'static>> {
    if !images::graphics_available() {
        return Vec::new();
    }
    let Some(path) = images::mascot_image_path() else {
        return Vec::new();
    };
    let cols = SIDEBAR_MASCOT_COLS.min(width.saturating_sub(2));
    if cols < SIDEBAR_MASCOT_MIN_COLS {
        return Vec::new();
    }
    let Some(place) = images::place_in(&path, SIDEBAR_MASCOT_PX, cols, SIDEBAR_MASCOT_ROWS) else {
        return Vec::new();
    };
    (0..place.rows)
        .map(|row| {
            Line::from(Span::styled(
                " ".repeat(usize::from(place.cols)),
                Style::default().underline_color(images::carrier(place.id, row)),
            ))
        })
        .collect()
}

/// The session's facts: the model, the working directory, the context gauge
/// and the permission mode — the footer's chain, read vertically.
fn session_rows(app: &App, width: u16) -> Vec<Line<'static>> {
    let Some(session) = &app.session else {
        return Vec::new();
    };
    let mut rows = vec![heading(SIDEBAR_SESSION, width)];
    let model = match app.viewed_agent_model() {
        Some(pinned) => pinned.to_string(),
        None if session.provider.is_empty() => session.model.clone(),
        None => format!("{} · {}", session.model, session.provider),
    };
    rows.push(fact(SIDEBAR_MODEL, &model, width));
    rows.push(fact(SIDEBAR_CWD, &session.cwd, width));
    if let Some((used, window)) = app.context_gauge() {
        rows.push(gauge_row(app, used, window, width));
    }
    if let Some(mode) = app.permission_mode() {
        rows.push(fact(SIDEBAR_MODE, mode.label(), width));
    }
    rows
}

/// `label value` with the label dim and the value on the next row when the two
/// do not fit — the sidebar's one row shape, so the column reads as a table.
fn fact(label: &str, value: &str, width: u16) -> Line<'static> {
    let room = usize::from(width).saturating_sub(cols(label) + 1);
    let value = truncate_cols(value, room.max(1));
    Line::from(vec![
        Span::styled(format!("{label} "), Style::new().fg(sidebar_meta_color())),
        Span::styled(value, Style::new().fg(sidebar_value_color())),
    ])
}

/// The context gauge as a one-row meter over its own numbers: the share as
/// blocks, then `used/window` dim. The bar is the quick read; the numbers are
/// what auto-compact turns on (`docs/architecture/compact.md`).
fn gauge_row(app: &App, used: u64, window: u64, width: u16) -> Line<'static> {
    #[allow(clippy::cast_precision_loss)] // display only
    let share = (used as f64 / window.max(1) as f64).clamp(0.0, 1.0);
    let meter_cols = usize::from(SIDEBAR_GAUGE_COLS.min(width.saturating_sub(12)));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let filled = (share * meter_cols as f64).round() as usize;
    let filled = filled.min(meter_cols);
    let mut spans = vec![Span::styled(
        format!("{SIDEBAR_CONTEXT} "),
        Style::new().fg(sidebar_meta_color()),
    )];
    if meter_cols > 0 {
        spans.push(Span::styled(
            SIDEBAR_GAUGE_FILL.repeat(filled),
            Style::new().fg(footer_gauge_color(
                used,
                window,
                app.context_usage_threshold(),
            )),
        ));
        spans.push(Span::styled(
            SIDEBAR_GAUGE_EMPTY.repeat(meter_cols - filled),
            Style::new().fg(sidebar_meta_color()),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!(
            "{}/{}",
            format_token_count(usize::try_from(used).unwrap_or(usize::MAX)),
            format_token_count(usize::try_from(window).unwrap_or(usize::MAX)),
        ),
        Style::new().fg(sidebar_meta_color()),
    ));
    Line::from(spans)
}

/// What is running: the task checklist, the agents, the background shells —
/// the things the inline mode crowds into the strip and the roster.
fn running_rows(app: &App, width: u16) -> Vec<Line<'static>> {
    let mut rows = Vec::new();
    let tasks: Vec<&crate::tasks::Task> = app
        .tasks()
        .tasks()
        .iter()
        .filter(|task| task.status != crate::tasks::TaskStatus::Completed)
        .collect();
    let agents: Vec<&crate::agents::AgentRun> = app
        .agents()
        .iter()
        .filter(|run| !run.status.is_final())
        .collect();
    let shells: Vec<&crate::app::BackgroundShell> = app.background().iter().collect();
    if tasks.is_empty() && agents.is_empty() && shells.is_empty() {
        return Vec::new();
    }
    rows.push(heading(SIDEBAR_RUNNING, width));
    // The task list first: it is the plan the rest is executing.
    for task in tasks.iter().take(SIDEBAR_MAX_ROWS) {
        let mark = match task.status {
            crate::tasks::TaskStatus::InProgress => SIDEBAR_TASK_ACTIVE,
            _ => SIDEBAR_TASK_PENDING,
        };
        rows.push(Line::from(vec![
            Span::styled(
                format!("{mark} "),
                Style::new().fg(if task.status == crate::tasks::TaskStatus::InProgress {
                    sidebar_accent_color()
                } else {
                    sidebar_meta_color()
                }),
            ),
            Span::styled(
                truncate_cols(&task.subject, usize::from(width).saturating_sub(2)),
                Style::new().fg(sidebar_value_color()),
            ),
        ]));
    }
    for run in agents.iter().take(SIDEBAR_MAX_ROWS) {
        rows.push(Line::from(vec![
            Span::styled(
                format!("{SIDEBAR_AGENT} "),
                Style::new().fg(sidebar_accent_color()),
            ),
            Span::styled(
                truncate_cols(&run.description, usize::from(width).saturating_sub(2)),
                Style::new().fg(sidebar_value_color()),
            ),
        ]));
    }
    if !shells.is_empty() {
        rows.push(Line::from(Span::styled(
            format!("{SIDEBAR_SHELL} {} running", shells.len()),
            Style::new().fg(sidebar_meta_color()),
        )));
    }
    rows
}

/// How many rows a running section spends on one kind of item.
const SIDEBAR_MAX_ROWS: usize = 3;
