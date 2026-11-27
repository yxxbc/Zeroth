//! All of the TUI's styling and live-region geometry, in one place.
//!
//! Bullets, prompts, colours, the tool-cell and overlay chrome, the status
//! indicator's spinner/shimmer, the band and footer metrics — retheme or
//! re-size here, never inline at a call site. That rule is a project convention
//! (CLAUDE.md); the consts each feature owns are described in its own doc, e.g.
//! `docs/ui/status-indicator.md`, `docs/ui/footer.md`, `docs/ui/toast.md`,
//! `docs/ui/shortcuts.md`.
//!
//! The **colours** are accessor functions rather than consts: each names a
//! *role* (`tool_ok_color()` — a finished tool's bullet; `menu_selected_color()`
//! — what every picker selects with) and reads the active theme's
//! [`Palette`](super::palette::Palette), so a `/theme` switch recolours every
//! call site with no call site knowing (`docs/ui/theme.md`). A colour word in a
//! role's doc comment — "cyan", "green", "dim grey" — describes the role as
//! the palettes paint it (the original One Dark chrome's words, kept because
//! every other theme paints the same role with the same *kind* of colour);
//! the exact value is the palette's. A new colour is a new role on the
//! palette plus its accessor here, never a literal `Color::Rgb` at a call
//! site.

use super::palette::palette;
use super::*;

// --- Claude-Code-ish styling. Centralised so it's trivial to retheme. ---

/// Prompt shown at the start of the input field.
pub(super) const PROMPT: &str = "❯ ";

/// Bullet prefixing a user message.
pub(super) const USER_BULLET: &str = "❯ ";

/// Bullet prefixing an assistant message.
pub(super) const AI_BULLET: &str = "● ";

/// Bullet prefixing a backend-error notice — same glyph as the assistant, but
/// coloured red (see [`error_color`]) so a failure reads as a red bullet point.
pub(super) const ERROR_BULLET: &str = "● ";

/// Bullet prefixing a system notice (slash-command output) — same glyph, coloured
/// cyan (see [`system_color`]) so it reads as meta rather than an AI reply.
pub(super) const SYSTEM_BULLET: &str = "● ";

/// Indent for wrapped continuation lines (matches a bullet's width).
pub(super) const INDENT: &str = "  ";

/// Columns a bullet/indent occupies, subtracted from the content width.
pub(super) const BULLET_WIDTH: u16 = 2;

/// The column every text field keeps past its text for the caret — the cell
/// the cursor sits in at the end of a row the wrap left exactly full
/// (`layout::text_field_width`, `docs/input/textarea.md`).
pub(super) const CURSOR_COLUMN: u16 = 1;

// --- Assistant markdown rendering (fenced code blocks + ATX headings;
// `docs/ui/markdown.md`). Code sits under the bullet (no gutter, no language
// label), rendered VERBATIM (indentation preserved, no word-wrap) — the fix
// for code losing its indentation — and **syntax-highlighted** by the `highlight`
// module (syntect + two_face grammars, Catppuccin Mocha theme — codex parity;
// the theme owns the code palette now, so there are no `CODE_*_COLOR` consts).
// Headings keep their `#` markers and style the line per level, matching codex
// (`heading_style`); their text is inline markdown like any other. ---
/// A tab inside a code block expands to this many spaces **for display**. A tab
/// is zero display columns (unicode-width treats it as a control char), so code
/// rendered verbatim would lose all its tab indentation (Go, Makefiles, …
/// collapse flush-left). A fixed substitution — not tab-stop alignment — keeps
/// it simple and prefix-stable, matching codex's `expand_tabs`. Copy is
/// unaffected: `/copy` reads the raw message text, not the rendered rows.
pub(super) const CODE_TAB_WIDTH: usize = 4;

/// A markdown thematic break (`---` / `***` / `___`) renders as this em-dash rule,
/// a direct port of codex's `Event::Rule` (`Line::from("———")` — three U+2014 EM
/// DASH, unstyled/default foreground). See `docs/ui/markdown.md`.
pub(super) const THEMATIC_BREAK: &str = "———";

// --- Markdown table styling (docs/ui/markdown.md, docs/ui/table-streaming.md). A GFM
// pipe table renders as a box-drawing grid: dim borders, bold header cells.
// Column widths are locked from the header + first data row (fit to the width),
// so once the first row streams the block is prefix-stable and its rows commit
// to scrollback one at a time; every cell **word-wraps** into its column (taller
// rows) instead of truncating with `…` when the grid is narrow, matching codex.
// See `AssistantRenderer`/`StreamRender`. ---
/// Dim colour of a table's box-drawing borders (`│ ─ ┌┬┐ ├┼┤ └┴┘`).
pub(super) fn table_border_color() -> Color {
    tool_dim_color()
}

/// The floor a table column shrinks to before its cells word-wrap (codex uses 3):
/// a narrow column keeps at least this many display columns, and cells wrap into
/// it across multiple rows rather than losing text to a `…`.
pub(super) const TABLE_MIN_COL: usize = 3;

/// Records fallback (`docs/ui/table-streaming.md`, Claude Code's key/value transpose):
/// a column at least this wide is considered scannable, so it never triggers the
/// fallback even if its content wraps (it's a legitimately wide narrative column).
pub(super) const TABLE_SCANNABLE_COL: usize = 12;

/// A cell that wraps into at least this many rows *in a narrow column* means the
/// grid is growing tall because columns are starved — flip to vertical records.
pub(super) const TABLE_RECORDS_MIN_LINES: usize = 3;

/// The narrowest value column the inline `label: value` record form keeps; below
/// this the field stacks (label on its own line, value indented beneath).
pub(super) const TABLE_RECORD_MIN_VALUE: usize = 12;

/// A stacked record value's indent under its label line.
pub(super) const TABLE_RECORD_STACK_INDENT: usize = 2;

/// The `─` rule between records caps at this many columns instead of spanning the
/// full content width — Claude Code's shorter separator reads cleaner for the
/// short key/value fields (a wide records table's full-width rule looked heavy).
pub(super) const TABLE_RECORD_SEPARATOR_WIDTH: usize = 40;

// --- Inline markdown styling (docs/ui/markdown.md). A prose line's `**bold**`,
// `*italic*`, `~~strike~~`, `` `code` `` and `[text](url)` render with these;
// emphasis is modifier-only (bold/italic/crossed-out), code and links carry a
// colour. Parsing lives in `markdown::parse_inline`; `ui` owns the styling. ---
/// Inline `` `code` `` — a distinct cyan so it reads as code within prose.
pub(super) fn inline_code_color() -> Color {
    palette().accent
}

/// A link's URL, shown as ` (url)` after its text — blue and underlined.
pub(super) fn link_url_color() -> Color {
    palette().link
}

// --- List and blockquote styling (docs/ui/markdown.md), codex's look: every
// bullet renders `•` whatever its source marker, a task item a ballot box,
// an ordered item `N.` in an accent colour, renumbered from the list's start;
// each nesting level sits `LIST_NEST_WIDTH` columns deeper and continuation
// rows hang under the item's text. A blockquote's `>` and text render green,
// codex's quote dress. ---
/// The glyph an unordered list item renders with, at every depth (codex's
/// `"• "`), whether the model wrote `-`, `*` or `+`.
pub(super) const LIST_BULLET: &str = "•";

/// A task item's open checkbox (`- [ ]`), replacing its bullet (codex).
pub(super) const LIST_TASK_OPEN: &str = "☐";

/// A task item's ticked checkbox (`- [x]` / `- [X]`), replacing its bullet.
pub(super) const LIST_TASK_DONE: &str = "☑";

/// Columns each list nesting level adds — codex's `depth * 4 - 3` geometry: a
/// level's bullet sits this much deeper than its parent's, whatever indent the
/// source nested it by.
pub(super) const LIST_NEST_WIDTH: usize = 4;

/// The accent colour of an ordered list's `N.` marker.
pub(super) fn list_marker_color() -> Color {
    palette().link
}

/// A blockquote's `>` marker and text — green, codex's quote dress.
pub(super) fn quote_color() -> Color {
    palette().success
}

// Code syntax-highlight colours now come from the `highlight` module's theme
// (syntect + two_face, Catppuccin Mocha — codex parity), baked into each
// `highlight::Seg`'s `Style`. `ui` no longer owns a code palette or maps token
// kinds to colours; it just renders the segments the grammar produced. See
// `docs/ui/markdown.md`.

pub(super) fn user_color() -> Color {
    palette().user_fg
}

pub(super) fn user_bg_color() -> Color {
    palette().user_bg
}

/// The pinned prompt's ink at rest — the user bubble's, because the strip is
/// that message's summary (`docs/ui/conversation-tool-details-and-pin.md`).
pub(super) fn pinned_prompt_color() -> Color {
    user_color()
}

/// The pinned prompt's ink under the pointer. The accent, the same "this row
/// is the target" cue the pickers select with — no new colour role, since a
/// hovered pin says exactly what a selected row says.
pub(super) fn pinned_prompt_hover_color() -> Color {
    palette().accent
}

pub(super) fn ai_color() -> Color {
    palette().text
}

pub(super) fn error_color() -> Color {
    palette().error
}

/// Cyan — a system notice's bullet (slash-command output).
pub(super) fn system_color() -> Color {
    palette().accent
}

pub(super) fn prompt_color() -> Color {
    palette().text
}

pub(super) fn border_color() -> Color {
    palette().border
}

mod chrome;
mod tools;
mod views;

pub(super) use self::chrome::*;
pub use self::chrome::{COMPACTED_NOTICE, LIVE_MIN_HEIGHT};
pub(super) use self::tools::*;
pub(super) use self::views::*;
