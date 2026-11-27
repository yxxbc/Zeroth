//! Live status, spinner, footer, banner, and inline modal styling constants.

use super::palette::palette;
use super::wrap::lerp_color;
use super::*;

// --- Transcript timestamps (Ctrl+O view only). Only the *user* message shows
// its wall-clock stamp: dim, right-aligned on its own line below the message
// (`hh:mm AM/PM`). AI replies, tools, and turn summaries record a stamp too but
// never display it; the inline view never shows any. See docs/ui/timestamps.md. ---

/// Dim grey — the user message's right-aligned timestamp in the Ctrl+O transcript.
pub fn timestamp_color() -> Color {
    tool_dim_color()
}

// --- Live status indicator (codex / Claude-Code style). While a turn is in
// flight a status line sits in the strip above the box (with a blank gap row
// between it and the box's top rule):
// `⣤⣀⣀⣀⣀⣀⣀⣀ {verb}… ({elapsed} · {↓|↑} {n} tokens · Thinking for {m})`. The
// line opens with the session's **spinner style** — by default `gravity`, a
// ball hopping along a braille track (see [`spinner_spans`] and
// docs/ui/spinner.md); before there was a catalog it was always the **comet**
// (a Larson-scanner sweep: a white head dragging a fading grey tail back and
// forth between dim walls, one frame per `SPINNER_INTERVAL`), which is still
// what the `SPINNER_*` consts just below describe. The working verb is picked
// per-turn (in `App`) and rests in the page titles' gradient
// ([`status_verb_color`]) under a codex-style **shimmer**: a bright band
// sweeps across it (see [`shimmer_spans`], ported from openai/codex
// `tui/src/shimmer.rs`). The elapsed / thinking / done
// times are humanized by [`format_elapsed`] (`45s`, `1m 30s`, `1h 1m`). On
// finish a dim, bullet-less `{done verb} for {n}` summary commits to scrollback (a
// `HistoryItem::Summary`). See docs/ui/status-indicator.md. ---

/// White — the comet's head (matches the codex/Claude-Code white status text).
pub fn status_color() -> Color {
    ai_color()
}

/// The comet-spinner animation frames (a Larson-scanner sweep, ten frames):
/// the bright head (`●`) drags a two-cell fading tail (`•` then `·`) out to
/// the right wall and back across to the **left wall** (flush against `(` —
/// no wasted leading cell), the tail whipping around behind it at each
/// bounce (a tail cell the head overlaps is hidden under it). Every frame is
/// the same width, so the verb after it never jitters.
pub const SPINNER_FRAMES: &[&str] = &[
    "(●•·   )",
    "(•●    )",
    "(·•●   )",
    "( ·•●  )",
    "(  ·•● )",
    "(   ·•●)",
    "(    ●•)",
    "(   ●•·)",
    "(  ●•· )",
    "( ●•·  )",
];

/// How long each spinner frame shows (the classic cli-spinners 80 ms cadence —
/// well under the loop's ~30 fps animation re-arm, so no frame is skipped).
pub const SPINNER_INTERVAL: Duration = Duration::from_millis(80);

/// The comet's bright bold head in a [`SPINNER_FRAMES`] frame.
pub const SPINNER_HEAD: char = '●';

/// The tail cell right behind the head; the `·` end (and everything else in
/// the frame — walls, empty track) fades to [`status_detail_color`].
pub const SPINNER_TAIL_MID: char = '•';

/// Mid grey — the `•` tail cell, between the white head and the dim tail end.
pub fn spinner_tail_color() -> Color {
    model_id_color()
}

/// How many spans [`spinner_spans`] emits (one per frame cell: the left wall,
/// six track cells, the right wall) — the verb's per-char spans start at this
/// index in the status line.
pub const SPINNER_SPAN_COUNT: usize = 8;

/// Dim grey — the parenthesised metrics (`elapsed · tokens · thinking`).
pub fn status_detail_color() -> Color {
    tool_dim_color()
}

/// The `Thinking for {m}` clause — the palette's purple, the one hue on the
/// line that means *reasoning*: the footer's thinking mode wears it too
/// ([`footer_qualifier_color`]), so the effort and the thinking it buys read
/// as one fact. Apart from the dim metrics and the amber retry clause in
/// every theme (`docs/ui/status-indicator.md`).
pub fn status_thinking_color() -> Color {
    palette().purple
}

/// Amber — the `retrying {n}/{max}` clause. A warning hue (One-Dark yellow),
/// distinct from the dim metrics and the error red: the request hasn't failed,
/// it's recovering. See `docs/providers/llm.md`.
pub fn status_retry_color() -> Color {
    palette().warning
}

/// Trailing ellipsis after the working verb (`Working…`).
pub const STATUS_ELLIPSIS: &str = "…";

/// Arrow for output tokens while the reply streams.
pub const STATUS_ARROW_DOWN: &str = "↓";

/// Arrow once a tool result is folded back in.
pub const STATUS_ARROW_UP: &str = "↑";

/// The interrupt hint, the detail's final clause while a turn is in flight —
/// codex's `Esc to interrupt` discoverability hint, lowercased to match this
/// codebase's hint convention (`(ctrl+o to expand)`, `esc return`).
pub const STATUS_INTERRUPT_HINT: &str = "esc to interrupt";

// --- Waiting for a lost connection (docs/providers/offline.md): the status line's
// verb and clause while no connection can be made to the provider, and the
// ripple row hanging under it. ---

/// The verb the status line wears while the backend waits for a connection it
/// cannot make — in place of the turn's own verb and of any task's
/// activeForm, since waiting is all the turn is doing.
pub const OFFLINE_VERB: &str = "Waiting for internet";

/// The clause naming how long the host has been gone, in the retry clause's
/// slot: `offline for 45s`.
pub const OFFLINE_CLAUSE: &str = "offline for ";

/// Amber — the offline verb's resting colour under its shimmer, the `offline
/// for` clause, the ripple's dot and the host in the row under the line. The
/// retry clause's own caution tone: the request has not failed, it is
/// waiting.
pub fn status_offline_color() -> Color {
    palette().warning
}

/// The ripple's frames — a dot sending rings out and drawing them back in,
/// the shape of a signal being sent to a host that does not answer. Every
/// frame is the same width, so the text after it never jitters.
pub const OFFLINE_FRAMES: &[&str] = &[
    "   ·   ", "  (·)  ", " ((·)) ", "(((·)))", " ((·)) ", "  (·)  ",
];

/// How long each ripple frame shows — a sub-second cycle, calm beside the
/// spinner.
pub const OFFLINE_FRAME_INTERVAL: Duration = Duration::from_millis(160);

/// How much of the dot's amber each ring out loses toward the dim, so the
/// widest frame fades with distance instead of reading as a bracket soup.
pub const OFFLINE_RING_FADE: f32 = 0.25;

/// The gap between the ripple and the row's text.
pub const OFFLINE_RIPPLE_GAP: &str = "  ";

/// The row's text around the host: `No connection to {host} — trying again ·
/// {n} attempts`. Short on purpose — the status line's verb already says the
/// turn is waiting — so the row fits one 80-column line beside an ordinary
/// host and leaves the reassurance to the count ticking.
pub const OFFLINE_ROW_LEAD: &str = "No connection to ";
pub const OFFLINE_ROW_TAIL: &str = " — trying again · ";

/// Dim grey — the committed `"{done verb} for {n}"` turn summary.
pub fn status_done_color() -> Color {
    tool_dim_color()
}

/// Marks a summary's token total as partly the app's estimate: an
/// interrupted turn's receipt, whose cut round never reported its usage
/// (`docs/input/interrupt.md`). The `/context` page's mark for a count it made
/// itself, so a bare number on a receipt is always the provider's own.
pub const SUMMARY_ESTIMATE_MARK: &str = CONTEXT_USAGE_ESTIMATE_MARK;

/// The status line's row in the streaming strip.
pub const STATUS_ROWS: u16 = 1;

/// A blank row between the status line and the box's top rule, so the status
/// never butts up against the box (mirrors the gap above, under the preview).
pub const STATUS_GAP_ROWS: u16 = 1;

// --- The verb's shimmer wave (ported from openai/codex `shimmer_spans`): each
// char's colour blends from its resting colour toward the bright text colour
// by a raised-cosine band that sweeps the text once per `SHIMMER_SWEEP`. The wave's
// phase derives from the boundary-supplied `TurnStatus::elapsed`, keeping the
// renderer pure (codex reads a process clock instead). ---

/// The status verb's **resting** colour at display column `col` of its
/// `total`-column text (`Working…`, a task's activeForm): the page titles'
/// dress (`/mcp`, `/skills` — [`header_gradient_at`]), so the one word in
/// motion on the line follows `/theme` the way the headlines do. Codex rests
/// its verb on grey; the crest's sweep toward [`shimmer_highlight`] is
/// unchanged over the gradient (`docs/ui/status-indicator.md`).
pub fn status_verb_color(col: usize, total: usize) -> Color {
    header_gradient_at(col, total)
}

/// The bright white the band's crest blends toward.
pub fn shimmer_highlight() -> Color {
    ai_color()
}

/// One full sweep of the band across the text (codex's `sweep_seconds`).
pub const SHIMMER_SWEEP: Duration = Duration::from_secs(2);

/// Off-text run-in/out, in chars, so the band slides on and off the ends
/// instead of wrapping abruptly (codex's `padding`).
pub const SHIMMER_PADDING: usize = 10;

/// The band's half-width in chars (codex's `band_half_width`).
pub const SHIMMER_BAND_HALF_WIDTH: f32 = 5.0;

/// The crest's blend toward the highlight (codex blends `t * 0.9`).
pub const SHIMMER_MAX_BLEND: f32 = 0.9;

// --- The spinner **styles** (docs/ui/spinner.md). The comet above is one of
// them; `/spinner` swaps the frames the status line opens with, and the
// default is `gravity`, a track style drawn from the geometry below. The
// catalog's *identity* — names, order, descriptions — is `app::Spinner`; its
// *look* is here, beside every other styling decision, and `spinner_spans`
// maps one to the other. Two rules every style keeps, pinned by
// `ui::tests::status`: every frame of a style is the same width (so the verb
// after it never jitters) and every glyph is single-width
// (docs/ui/table-streaming.md "Wide glyphs"). A one-cell style is drawn as one
// span — the glyph plus the separator space — in the comet head's white bold
// unless its own colour rule below says otherwise. The two **track** styles
// (`gravity`, `wave`) have no frame table at all: they draw themselves on a
// braille track from the geometry consts below. ---

/// The braille **track** the `gravity` and `wave` styles draw on: this many
/// cells of 2 × 4 dots each — sixteen dot columns by four dot rows inside one
/// text row, the resolution that lets a ball visibly hop and a wave visibly
/// roll where a glyph table could only step. Eight, the comet's footprint,
/// so the three wide styles share one width.
pub const SPINNER_TRACK_CELLS: usize = 8;

/// `gravity` — one round trip of the ball along the track (left wall → right
/// wall → left) at constant speed, reversing hard at each wall.
pub const SPINNER_GRAVITY_SWEEP: Duration = Duration::from_millis(2400);

/// One hop of the `gravity` ball, floor to floor — four per round trip, so it
/// touches down exactly as it meets each wall.
pub const SPINNER_GRAVITY_HOP: Duration = Duration::from_millis(600);

/// `wave` — one crawl of the wave down the track and back.
pub const SPINNER_WAVE_SWEEP: Duration = Duration::from_millis(3400);

/// How many wavelengths the `wave` travels each way per sweep — a whole
/// number, so the frame at the reversal is the frame it set out from.
pub const SPINNER_WAVE_TRAVEL: f32 = 3.0;

/// The `wave`'s wavelength in dot columns — the whole track, so one crest and
/// one trough are always in view.
pub const SPINNER_WAVE_LENGTH: f32 = 16.0;

/// `sparkle` — a spark opening into a heavy star and closing again. Its
/// colour walks the banner's [`header_gradient_start`] → [`header_gradient_end`]
/// with the bloom (cyan at the spark, blue at the full star), so the theme's
/// accent rides the status line.
pub const SPINNER_SPARKLE_FRAMES: &[&str] = &["·", "✢", "✳", "✶", "✻", "✽", "✻", "✶", "✳", "✢"];

/// How long each `sparkle` frame shows — a 1.2 s bloom-and-fade.
pub const SPINNER_SPARKLE_INTERVAL: Duration = Duration::from_millis(120);

/// `dots` — the classic braille spinner (cli-spinners' `dots`), white bold.
pub const SPINNER_DOTS_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// How long each `dots` frame shows (the cli-spinners cadence).
pub const SPINNER_DOTS_INTERVAL: Duration = Duration::from_millis(80);

/// `blocks` — the mascots' own three-quarter block glyphs (docs/ui/mascot.md),
/// the missing quadrant walking clockwise, coloured along the banner's
/// gradient as it turns: cyan on the first frame, blue on the last.
pub const SPINNER_BLOCKS_FRAMES: &[&str] = &["▙", "▛", "▜", "▟"];

/// How long each `blocks` frame shows.
pub const SPINNER_BLOCKS_INTERVAL: Duration = Duration::from_millis(150);

/// `pulse` — one still `●` whose colour breathes [`spinner_pulse_dim`] →
/// [`spinner_pulse_bright`] → dim once per [`SPINNER_PULSE_PERIOD`]: a
/// raised-cosine swell at the running tool bullet's cadence (that bullet
/// itself blinks, docs/ui/tool-pulse.md), taken up to white at the crest so it
/// reads as a status line's head rather than a resting cell.
pub const SPINNER_PULSE_FRAMES: &[&str] = &["●"];

/// The bottom of the `pulse` breath — the palette's `pulse_dim`.
pub fn spinner_pulse_dim() -> Color {
    tool_pulse_dim()
}

/// The crest of the `pulse` breath — the shimmer's white.
pub fn spinner_pulse_bright() -> Color {
    shimmer_highlight()
}

/// One `pulse` breath — the tool bullet's blink period, so a pulsing status
/// line and a running tool cell move in step.
pub const SPINNER_PULSE_PERIOD: Duration = TOOL_PULSE_PERIOD;

/// `bars` — a bar rising `▁` → `█` and falling back, brightening
/// [`spinner_bars_low`] → [`spinner_bars_high`] with its height, like a
/// level meter.
pub const SPINNER_BARS_FRAMES: &[&str] = &[
    "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
];

/// How long each `bars` frame shows — a 0.84 s rise and fall.
pub const SPINNER_BARS_INTERVAL: Duration = Duration::from_millis(60);

/// The lowest bar's grey — the running tool bullet's own grey
/// ([`tool_pulse_bright`]), so even `▁` reads.
pub fn spinner_bars_low() -> Color {
    tool_pulse_bright()
}

/// The full bar's white.
pub fn spinner_bars_high() -> Color {
    shimmer_highlight()
}

/// `line` — the classic ASCII spinner, for a font with none of the glyphs
/// above; white bold.
pub const SPINNER_LINE_FRAMES: &[&str] = &["|", "/", "-", "\\"];

/// How long each `line` frame shows.
pub const SPINNER_LINE_INTERVAL: Duration = Duration::from_millis(100);

// --- Slash-command palette. A scrolling, single-line-per-command list pinned
// **below the input box** (a third live-region band) whenever the input is a bare
// command token. Each row is the `→ ` marker (on the selection) or its two-space
// inset, `/name` padded to a column, then its description — the marker the `@`
// and `$` pickers below share. The highlighted row also lights up cyan whole —
// name *and* description the same colour — while the others are dimmed grey.
// At most `MENU_MAX_COMMANDS` commands show at once, over a dim `(n/total)`
// counter; longer lists scroll to keep the selection centered
// (`centered_window_rows`, the `/model` list's rule), and the band never takes
// more than `MENU_MAX_ROWS`. ---

/// The most commands the palette lists at once — the `/settings` picker's
/// five; longer match lists scroll within this (`centered_window_rows` keeps
/// the selection on the middle row, like the `/model` and `/resume` lists, so
/// the commands above and below it stay in view), the `(n/total)` counter
/// under them saying where the highlight is and how many commands match. The
/// registry has outgrown the window — a bare `/` shows `/clear` through
/// `/compact` and ↓ scrolls the rest in (the
/// `the_palette_shows_five_commands_over_a_counter` /
/// `the_palette_keeps_the_highlight_centered_as_it_scrolls` tests pin both
/// halves).
pub const MENU_MAX_COMMANDS: usize = 5;

/// The most rows the palette band takes, its counter's row included — a
/// ceiling only a narrow terminal reaches. Where a description *wraps*
/// (`menu_row_lines` continues it on rows indented to the description column
/// instead of clipping it) each command takes more than one row, and the
/// window shows fewer whole commands rather than a band taller than this,
/// centering the selection by rows. At widths where every description fits
/// its row, the five commands and the counter take six.
pub const MENU_MAX_ROWS: u16 = 8;

/// The column descriptions start at — the [`MENU_MARKER`] (or its inset) and
/// the name are padded out to here so the descriptions line up in a tidy
/// column regardless of command-name length. Two past the 25 it was before the
/// rows wore the marker, so the name keeps the room it had; 80 columns still
/// leave every description (≤ 53 columns) its own row.
pub const MENU_DESC_COL: usize = 27;

/// The selected row's arrow marker, in every band below the box that lists
/// something to pick — the palette, the `@` file picker, the `$` skill
/// picker. Unselected rows open on [`MENU_INDENT`] instead.
pub const MENU_MARKER: &str = "→ ";

/// The unselected rows' inset — the marker's width in spaces, so the names
/// start at the same column on every row.
pub const MENU_INDENT: &str = "  ";

/// The gap between the command page's tab labels. Two spaces, the palette's
/// own rhythm: the bar stays a plain row of words, not buttons.
pub const MENU_TAB_GAP: &str = "  ";

/// Cyan — the **selected** row: its `/name` *and* description share this colour
/// (for consistency); the name is additionally bold.
pub fn menu_selected_color() -> Color {
    palette().accent
}

/// Dim grey — an unselected row (name and description alike).
pub fn menu_dim_color() -> Color {
    tool_dim_color()
}

/// The palette's single placeholder row when the `/token` matches no command.
pub const MENU_NO_MATCH: &str = "No matching commands";

/// The placeholder when the Skills tab has no row for the same token — the
/// command placeholder's sibling, so the tab never shows an empty band.
pub const MENU_NO_MATCH_SKILLS: &str = "No matching skills";

// --- The `@` file picker. A file list pinned **below the input box** (the
// palette's slot — the bands never show together), opened by an `@token` under
// the cursor — a port of codex's file-search popup. Each row is columned —
// `→ name  parent/  …  File|Dir` — reusing the palette's `→` marker and its
// cyan-selected / dim-unselected colours, additionally **bolding the
// characters the query matched** (worked out for the rows drawn), over the
// palette's dim `(n/total)` counter. See docs/input/file-search.md. ---

/// The most file rows shown at once — the `/` palette's five, as in the `$`
/// picker; the rest of the matches (every one the search found) scroll under
/// a highlight kept on the middle row (`centered_window`), and the
/// `(n/total)` counter under them is a row of its own.
pub const FILE_MENU_MAX_ROWS: u16 = 5;

/// The picker's single placeholder row while a search is in flight.
pub const FILE_MENU_SEARCHING: &str = "Searching…";

/// The picker's single placeholder row when the query matched nothing.
pub const FILE_MENU_NO_MATCH: &str = "No matching files";

/// Closes the `(n/total)` counter's total when the walk stopped at its cap
/// — `(1/10000+)` — since the list it counts is then only part of the
/// workspace, and the total a floor.
pub const FILE_MENU_PARTIAL_MARK: &str = "+";

/// Gap between the name column and the parent-dir column (the name column is
/// the widest visible name plus this).
pub const FILE_MENU_GAP: usize = 2;

/// Columns reserved at the right edge for the kind label — `File`/`Dir`
/// left-aligned in this slot, so both start at `width − FILE_MENU_TYPE_WIDTH`.
pub const FILE_MENU_TYPE_WIDTH: usize = 6;

/// The parent-dir cell of a root-level entry (its parent is the cwd itself).
pub const FILE_MENU_ROOT_DIR: &str = "./";

/// The kind-column label for a directory match.
pub const FILE_MENU_DIR_LABEL: &str = "Dir";

/// The kind-column label for a file match.
pub const FILE_MENU_FILE_LABEL: &str = "File";

// --- The `$` skill picker. A fourth band in the same slot, opened while the
// cursor sits in a usable `$mention`: one row per matching skill — the name
// column (widest visible name + `FILE_MENU_GAP`, matched characters bolded)
// then the skill's own description, `…`-cut at the width — over the
// palette's dim `(n/total)` counter. It reuses the palette's marker, inset
// and colours — the selection lights up cyan, the rest dim. See
// docs/input/skill-mentions.md. ---

/// The most skill rows shown at once — the `/` palette's five, a row each,
/// since a description is `…`-cut rather than wrapped; longer lists scroll
/// under a highlight kept on the middle row (`centered_window`, the palette's
/// window), and the `(n/total)` counter under them is a row of its own.
pub const SKILL_MENU_MAX_ROWS: u16 = 5;

/// The band's single placeholder row when the query matched no skill.
pub const SKILL_MENU_NO_MATCH: &str = "No matching skills";

// --- The `?` shortcuts band. A keyboard-shortcuts overview pinned **below the
// input box** (the palette's slot — the two never show together), toggled by
// `?` from an empty composer — a port of codex's footer shortcut overlay
// (`footer.rs::shortcut_overlay_lines`): `{key} for {thing}` entries, keys
// cyan, labels dim, inset by `MENU_INDENT` like the footer row whose slot it
// takes (codex's overlay, a footer mode, wears its footer's indent too). The
// entries run down **as many columns as the terminal holds whole** — up to
// six on a wide one; two with each label's `for`/`to` dropped on a narrow
// pane, then one — so a wider terminal spends its width on columns rather
// than leaving the row empty and spending rows below the box. See
// docs/ui/shortcuts.md. ---

/// The bindings listed in the band, as `(key, label)` pairs **in reading
/// order**: the band fills its columns top to bottom (column-major, like
/// `ls`), so neighbours here are neighbours in a column. Grouped so that
/// whatever the column count, each column reads as a run of related keys —
/// the composer's sigils and history, then editing and the running turn,
/// then the views, the modes and the way out (at three columns, the 80-column
/// layout, those are exactly the three columns). The `esc` entry is
/// context-sensitive — [`shortcuts_lines`] swaps it for
/// [`SHORTCUTS_INTERRUPT`] while a turn runs (codex's quit entry does the
/// same) and for the [`SHORTCUTS_BACKTRACK`] edit hint when idle with a
/// previous user message to edit (docs/input/backtrack.md).
///
/// Every label leads with ` for ` or ` to `, which a pane too narrow for two
/// columns in the full wording drops to keep two (`terse_label`). An entry is
/// a column's width wherever it lands, so a long one widens whichever column
/// the layout puts it in and can cost a column at the width it used to fit:
/// `the_band_spends_a_wider_terminal_on_columns_not_rows` pins where the
/// column counts change.
pub const SHORTCUTS: &[(&str, &str)] = &[
    ("/", " for commands"),
    ("!", " for shell command"),
    ("@", " for file paths"),
    ("$", " for skills"),
    ("↑", " for input history"),
    ("ctrl+r", " to search history"),
    ("shift+enter", " for newline"),
    ("ctrl+v", " for image paste"),
    ("ctrl+w/u/k", " to kill text"),
    ("tab", " to queue next turn"),
    ("alt+↑", " to edit queue"),
    ("ctrl+b", " to background"),
    ("ctrl+o", " for transcript"),
    ("ctrl+d", " for llm context"),
    ("ctrl+t", " to cycle thinking"),
    ("shift+tab", " for permissions"),
    ("esc", " to quit"),
    ("ctrl+c", " to quit"),
];

pub const WORKSPACE_SHORTCUTS: &[(&str, &str)] = &{
    let mut entries = [("", ""); SHORTCUTS.len() + 1];
    let mut index = 0;
    while index < SHORTCUTS.len() {
        entries[index] = SHORTCUTS[index];
        index += 1;
    }
    entries[index] = ("↓ ←/→ Enter ↑", " for sessions");
    entries
};

/// The `esc` entry while a turn is in flight, where Esc interrupts it (see
/// [`shortcuts_lines`]; the idle edit hint is [`SHORTCUTS_BACKTRACK`]).
pub const SHORTCUTS_INTERRUPT: (&str, &str) = ("esc", " to interrupt");

/// The fewest rows the band packs its entries into, however wide the
/// terminal: past three rows of six columns a wider terminal adds no more
/// columns, since a two-row ribbon across an ultra-wide pane reads as a
/// toolbar to scan rather than a list to read.
pub const SHORTCUTS_MIN_ROWS: usize = 3;

/// The narrowest gap between two columns — a layout fits a width only with
/// at least this many blank columns after each column's widest entry. Any
/// fewer and two entries read as one run-on line, as they did when a fixed
/// column once left a single space between them.
pub const SHORTCUTS_MIN_GUTTER: usize = 2;

/// The widest gap between two columns. The width a layout does not need is
/// shared out between its gaps, so the band reaches across the row rather
/// than bunching at its left with the right side empty — but no further
/// apart than this, or an ultra-wide terminal would scatter a three-row band
/// across the screen.
pub const SHORTCUTS_MAX_GUTTER: usize = 6;

/// What opens the dim last row of a band cut short by a terminal without
/// the rows for it — `… +12 more`, the permission prompt's `… +N more
/// waiting` idiom. A static overview has no window to scroll, so the row
/// names what is hidden rather than pointing at it.
pub const SHORTCUTS_MORE_MARKER: &str = "…";

/// Cyan — an entry's key (the palette-selection accent).
pub fn shortcuts_key_color() -> Color {
    menu_selected_color()
}

/// Dim grey — an entry's label (codex dims the whole overlay), and the
/// `… +N more` row of a band cut short.
pub fn shortcuts_text_color() -> Color {
    tool_dim_color()
}

// --- Esc-Esc backtrack (docs/input/backtrack.md). A primed first Esc takes the
// footer slot with a hint naming the second (codex's `esc_backtrack_hint`
// footer); the transcript overlay then highlights the selected user message
// by *reversing* its rows (codex's `user_message_style().reversed()`). ---

/// The primed hint's key, bold-cyan like the search-line hint keys.
pub const BACKTRACK_HINT_KEY: &str = "esc";

/// The primed hint's dim label — codex's "esc again to edit previous message"
/// wording, minus the key it highlights separately.
pub const BACKTRACK_HINT_LABEL: &str = " again to edit previous message";

/// The shortcuts-band `esc` entry while idle with a backtrack target: the
/// gesture replaces quit as Esc's idle meaning (see [`shortcuts_lines`]).
pub const SHORTCUTS_BACKTRACK: (&str, &str) = ("esc esc", " to edit previous");

// --- Queued messages. While a turn streams, messages submitted with Enter (or
// Tab) join `App::queued` and are shown **above the box** (in the strip, just
// under the status line's gap) styled exactly like a sent user message — the
// `❯ ` bullet, the dark background, wrapped — so a queued follow-up reads like
// it is already on its way. The queue is a sequence of turn-batches: Enter
// appends to the current batch, **Tab opens a new one**, and the loop sends one
// batch per turn-end (a blank row divides the batches). See docs/input/queue.md. ---

/// Indent prefixed to every queued row, insetting the queue from the strip's
/// left edge; the dark user-message block starts after it.
pub const QUEUED_INDENT: &str = "  ";
/// Messages awaiting the running model turn's next boundary.
pub const QUEUED_CURRENT_LABEL: &str = "Pending current turn";
/// Separate follow-up turns awaiting dispatch.
pub const QUEUED_NEXT_LABEL: &str = "Queued next turn";
/// Composer hint for a draft that can steer the running turn.
pub const FOOTER_STEER_QUEUE_HINT: &str = "Enter steers · Tab queues";
/// Recorded command exit status in a settled run receipt.
pub const TOOL_RUN_EXIT_LABEL: &str = "exit ";
/// Separator between recorded execution facts.
pub const TOOL_RUN_SEPARATOR: &str = " · ";
/// Review entry after a successful turn with recorded project file changes.
pub const TURN_REVIEW_HINT: &str = "Review the current worktree with /diff";

/// The indent a lone-URL link row shares with the conversation's bullet
/// column — `docs/ui/rich-links.md`'s link line (the card's replacement).
pub const LINK_LINE_INDENT: &str = "  ";

// --- The session-context footer: the dim `{model} · {cwd}` row pinned under
// the input box (codex's footer status line). See docs/ui/footer.md. ---

/// Indent prefixed to the footer row (codex's `FOOTER_INDENT_COLS`).
pub const FOOTER_INDENT: &str = "  ";

/// Separator between the footer's segments (codex's dim ` · `).
pub const FOOTER_SEPARATOR: &str = " · ";

/// The footer's text colour — every segment dim, codex's no-theme-colours
/// status-line style.
pub fn footer_color() -> Color {
    tool_dim_color()
}

/// The model's name — the accent, codex's cyan model (its status line with
/// theme colours on): what the session talks to is the row's first fact.
/// The separators stay [`footer_color`], so the named things stand out from
/// the punctuation between them (`docs/ui/footer.md`).
pub fn footer_model_color() -> Color {
    menu_selected_color()
}

/// The model's qualifiers beside its name — the Ctrl+T thinking mode and a
/// selected speed tier (`medium fast`) — in the status line's thinking
/// colour ([`status_thinking_color`]), one colour for what those keys turn.
pub fn footer_qualifier_color() -> Color {
    status_thinking_color()
}

/// The working directory — green, codex's path colour. The banner's cwd row
/// wears it too ([`header_cwd_color`]), so the path is one colour wherever
/// the app names it.
pub fn footer_cwd_color() -> Color {
    tool_ok_color()
}

/// The context gauge for `used` of a `window`-token window that compacts at
/// `threshold` — the `/context` page's share rule ([`context_fill_color`]),
/// resting on the row's dim while there is room.
pub fn footer_gauge_color(used: u64, window: u64, threshold: Option<u64>) -> Color {
    context_fill_color(used, window, threshold).unwrap_or_else(footer_color)
}

/// A brand mark's colour — [`crate::brand::Tone`] read off the active palette,
/// so a model or provider icon in the footer, the `/model` picker or the
/// sidebar follows `/theme` like every other role (`docs/ui/theme.md`). A
/// tone names a *meaning* the palette already has (`docs/brand-icons.md`), so
/// adding a brand never means a new colour in every theme table.
pub fn brand_color(tone: crate::brand::Tone) -> Color {
    use crate::brand::Tone;
    match tone {
        Tone::Accent => menu_selected_color(),
        Tone::Link => palette().link,
        Tone::Success => tool_ok_color(),
        Tone::Warning => palette().warning,
        Tone::Purple => palette().purple,
        Tone::Text => palette().text,
        Tone::Dim => footer_color(),
    }
}

/// The permission mode pinned at the row's right edge, warming with what the
/// mode lets the agent do unasked (`docs/state/permissions.md`): `manual` asks
/// before everything and stays dim with the rest of the row, `edit` (file
/// changes unasked) takes the link blue, `auto` (the classifier approves
/// commands) the caution amber, and `master` (asks nothing) the error red,
/// Claude Code's bypass-permissions dress. Four hues every theme paints apart.
pub fn footer_mode_color(mode: crate::permission::PermissionMode) -> Color {
    use crate::permission::PermissionMode;
    match mode {
        PermissionMode::Manual => footer_color(),
        PermissionMode::Edit => palette().link,
        PermissionMode::Auto => palette().warning,
        PermissionMode::Master => error_color(),
    }
}

/// The least gap kept between the footer's left chain and the permission
/// mode pinned at the row's right edge — the mode's reservation is its own
/// columns plus this, so the left content's `…` cut can never run into it
/// (`docs/state/permissions.md`).
pub const FOOTER_MODE_GAP: usize = 2;

/// The **focused** shell indicator's fill: ↓ lights the footer's `{n} shell(s)`
/// segment on the palette-selection cyan and waits for the Enter that opens the
/// ↓ manager band (Claude-Code-style — see `docs/tools/background.md`). Only that one
/// segment changes; the model / cwd / context-gauge segments stay dim.
pub fn footer_focus_bg() -> Color {
    menu_selected_color()
}

/// The focused indicator's ink on that cyan fill — near-black, so the lit
/// segment reads as a chip rather than a smudge.
pub fn footer_focus_fg() -> Color {
    palette().on_accent
}

// --- The transient toast: a one-line, self-clearing status message pinned just
// above the box (`Copied last message to clipboard`, `/resume is disabled …`).
// It occupies the bottom of the strip, directly above the box's top rule, and
// fades after a few seconds (the expiry timed at the I/O boundary). See
// docs/ui/toast.md. ---

/// Indent prefixed to the toast row — the two-space inset shared with the
/// footer and the queued messages.
pub const TOAST_INDENT: &str = "  ";

/// An info toast's colour (a confirmation / soft rejection) — dim, like the footer.
pub fn toast_color() -> Color {
    tool_dim_color()
}

/// How far an error toast's red is mixed toward the info toast's dim — 0 the
/// full error red, 1 the dim. A third of the way keeps a failure red at a
/// glance without the four-second line outshouting the error bullet. See
/// docs/ui/toast.md.
pub const TOAST_ERROR_DIM_MIX: f32 = 0.35;

/// An error toast's colour (a failure — a `Copy failed`, a refused model
/// switch): the theme's error red mixed [`TOAST_ERROR_DIM_MIX`] of the way
/// toward [`toast_color`], a softer red than the error bullet's. A named
/// terminal red (the `ansi` theme) has nothing to mix and stays itself.
pub fn toast_error_color() -> Color {
    lerp_color(error_color(), toast_color(), TOAST_ERROR_DIM_MIX)
}

// --- The Ctrl+R reverse history search line (codex's reverse-i-search footer,
// `chat_composer/history_search.rs::history_search_footer_line`). It takes the
// session footer's slot while a search is open, and the previewed match in the
// composer highlights the query occurrences. See docs/input/history-search.md. ---

/// The dim prompt opening the search line.
pub const SEARCH_PROMPT: &str = "reverse-i-search: ";

/// Cyan — the query text and the accept/cancel hint keys (codex's `.cyan()`;
/// the palette-selection accent).
pub fn search_query_color() -> Color {
    menu_selected_color()
}

/// The notice appended to the line when the query matches nothing — red, like
/// codex's `"  no match"`.
pub const SEARCH_NO_MATCH: &str = "  no match";

/// How a previewed match's query occurrences light up in the input box
/// (codex's `REVERSED | BOLD` textarea highlight).
pub const SEARCH_HIGHLIGHT: Modifier = Modifier::REVERSED.union(Modifier::BOLD);

// --- The `!` shell-mode footer hint. While the composer holds a `!command`
// the footer slot reads `Shell mode` in red (codex's light-red
// `shell_mode_footer_line`), displacing the `{model} · {cwd}` line. See
// docs/input/shell-command.md. ---

/// The shell-mode hint text.
pub const SHELL_MODE_LABEL: &str = "Shell mode";

/// The hint's colour — red, like codex's `light_red()` (reuses our error red).
/// Also colours the `! ` bullet/prompt everywhere shell mode shows.
pub fn shell_mode_color() -> Color {
    error_color()
}

/// The bullet opening a committed shell command's header (`! pwd` on the dark
/// user-style line) — and the composer prompt while shell mode is on (the
/// absorbed bang rendered back; same two columns as [`PROMPT`]).
pub const SHELL_BULLET: &str = "! ";

// --- The startup header banner (docs/ui/header.md, docs/ui/mascot.md): the
// gradient mascot beside the title + cwd + hint, committed to scrollback at
// launch and re-emitted atop every full repaint (resize, `/clear`, a /mascot
// switch) so it survives the scrollback purge. Pure chrome, like the footer —
// never in `history`, so it never reaches the model or the `/resume` rollout.
// Borderless (no `─` rule row, no bare prompt, no model name) so the smoke
// resize counters don't see it. The art itself lives on `app::Mascot`. ---

/// The product name — bold in the banner's title row, gradient-washed in the
/// narrow one-line badge. Also the smoke suite's tier-independent banner
/// marker (`scripts/smoke.sh` Phase 45), so every tier must carry it. Read
/// from [`crate::APP_NAME`], the one place the agent's name lives, so the
/// banner and every sentence the app speaks its name in can't drift apart.
pub const HEADER_NAME: &str = crate::APP_NAME;

/// The command hint beside the mascot — bare `/token`s in the accent colour,
/// three-space separated. Deliberately prose-free so it can't collide with
/// the smoke suite's `for commands` / footer markers.
pub const HEADER_HINTS: &[(&str, &str)] = &[("/", "commands"), ("@", "files"), ("?", "shortcuts")];

/// Columns between the mascot art's right edge and the metadata column —
/// two, per the user's spec (a wider gap read as detached).
pub const HEADER_ART_GAP: usize = 2;

/// Indent shared with the footer and messages — the badge tier's rows sit
/// two columns in. The mascot art is drawn flush-left.
pub const HEADER_INDENT: &str = "  ";

/// The logo gradient's left endpoint — the inline-code cyan ([`inline_code_color`]).
pub fn header_gradient_start() -> Color {
    menu_selected_color()
}

/// The logo gradient's right endpoint — the link blue ([`link_url_color`]).
pub fn header_gradient_end() -> Color {
    link_url_color()
}

/// The banner gradient at display column `col` of a `total`-column run:
/// [`header_gradient_start`] at the first column, [`header_gradient_end`] at
/// the last — the one wash the mascot, every page title and the status verb
/// wear (`docs/ui/header.md`).
pub fn header_gradient_at(col: usize, total: usize) -> Color {
    let t = if total <= 1 {
        0.0
    } else {
        col as f32 / (total - 1) as f32
    };
    lerp_color(header_gradient_start(), header_gradient_end(), t)
}

/// The version badge + hint-token colour — the cyan accent, so they pop.
pub fn header_accent_color() -> Color {
    inline_code_color()
}

/// The banner's cwd row — the footer's path colour ([`footer_cwd_color`]),
/// so the path reads the same at both ends of the screen.
pub fn header_cwd_color() -> Color {
    footer_cwd_color()
}

/// The tagline / cwd / separator colour — dim, like the footer.
pub fn header_meta_color() -> Color {
    footer_color()
}

/// Keep a startup notice card — the update notice — readable on wide
/// terminals, with room for a rounded
/// frame; very narrow panes use the same content without the box.
pub const NOTICE_CARD_MAX_WIDTH: usize = 76;
pub const NOTICE_CARD_MIN_WIDTH: usize = 24;
/// The update card's title (`docs/ops/update.md`).
pub const UPDATE_CARD_TITLE: &str = "Update available";

/// Notice prose needs more contrast than the banner's cwd metadata.
pub fn notice_text_color() -> Color {
    palette().text_muted
}

// --- Live-region geometry. The bottom region's height is dynamic: it grows with
// the wrapped input (see `live_height`). `render_live` and `cursor_position` both
// derive their layout from `input_box` so the drawn text and cursor never drift;
// `main.rs`/`term.rs` size the viewport from `live_height`/`LIVE_MIN_HEIGHT`. ---

/// A blank gap row between the streaming preview and the box, so the live reply
/// never butts up against the box's top rule. Present only while streaming.
pub const GAP_ROWS: u16 = 1;

/// The input box's non-text rows: a top rule and a bottom rule.
pub const INPUT_CHROME_ROWS: u16 = 2;

/// The smallest the live region ever gets: a one-text-row box framed by two
/// rules (idle has no preview strip). `main.rs` sizes the initial viewport from this.
pub const LIVE_MIN_HEIGHT: u16 = INPUT_CHROME_ROWS + 1;

/// The `/compact` marker cell's text — codex's "Context compacted" info cell,
/// verbatim. See `docs/architecture/compact.md`.
pub const COMPACTED_NOTICE: &str = "Context compacted";

// --- The `Agent` tool (docs/agents/agent-tool.md) ---

/// The tree connectors of a group cell's per-agent rows: `   ├ {description}`
/// for every agent but the last, `   └ {description}` for the last, with the
/// status row's gutter continuing the rail (`   │ ⎿  Done` / `     ⎿  Done`).
pub const AGENT_TREE_INDENT: &str = "   ";

pub const AGENT_TREE_MID: &str = "├ ";

pub const AGENT_TREE_LAST: &str = "└ ";

pub const AGENT_TREE_PIPE: &str = "│ ";

pub const AGENT_TREE_BLANK: &str = "  ";

/// The status row's corner inside the tree (`⎿  Done`).
pub const AGENT_TREE_CORNER: &str = "⎿  ";

/// The committed background-launch header's hint: the ↓ manager plus the
/// Ctrl+O transcript, where the launch cell expands to each agent's prompt
/// and tool headers (the user-requested pairing).
pub const AGENT_MANAGE_HINT: &str = " (↓ to manage · ctrl+o to expand)";

/// The fixed `⎿` body of a **lone** agent cell resolved by a background
/// launch — [`TOOL_BACKGROUNDED`]'s agent twin with the transcript hint
/// added, since the cell expands in Ctrl+O to the agent's prompt, nested
/// tool headers, and response.
pub const AGENT_BACKGROUNDED: &str = "Running in the background (↓ to manage · ctrl+o to expand)";

/// The Ctrl+O cell's `Prompt:` / `Response:` section labels (green bold,
/// Claude Code's transcript look).
pub const AGENT_PROMPT_LABEL: &str = "Prompt:";
/// What sets the dim agent id off the Ctrl+O agent cell's header —
/// `● Agent(Fetch Warsaw) · a7k2m9x4q` (`docs/agents/agent-tools.md`).
pub const AGENT_ID_SEPARATOR: &str = " · ";

pub const AGENT_RESPONSE_LABEL: &str = "Response:";

pub fn agent_section_color() -> Color {
    tool_ok_color()
}

/// The rule cell painted **after** the agent session view's composer label —
/// `── {description} ─` instead of `── {description} ` — so the label sits
/// embedded in the top rule rather than dangling off its right end
/// (`docs/agents/agent-tool.md`). One border glyph, [`border_color`]-styled at the
/// render site.
pub const AGENT_VIEW_RULE_TAIL: &str = "─";

/// The most of that top rule the label may take: **half** its width, the rest
/// staying rule. A `description` is the model's own sentence and can run the
/// width of the terminal, and ratatui skids an over-wide right-aligned title
/// off its **left** end — so an unclipped label ate the whole frame *and* lost
/// the head of the very text it was showing. Past this budget the description
/// is cut with [`TOOL_HEADER_ELLIPSIS`] instead (`docs/agents/agent-tool.md`).
pub const AGENT_VIEW_LABEL_DIVISOR: usize = 2;

/// That label's ground: the agent session view's composer label rides its
/// top rule as a **lit chip** — the theme's accent under the on-accent ink,
/// the ↓-focused footer chip's dress ([`footer_focus_bg`]/[`footer_focus_fg`]),
/// since it answers the same kind of question (*which* thing the keys act
/// on: here, which conversation the composer feeds). Dim text embedded in a
/// dim rule was the one row saying the screen was not the main session, and
/// the eye passed over it (the user-requested fill, `docs/agents/agent-tool.md`).
/// The label's own padding spaces sit inside the fill, the
/// [`AGENT_VIEW_RULE_TAIL`] outside it, so the chip reads as a tab set into
/// the rule rather than a smudge on it.
pub fn agent_view_label_bg() -> Color {
    menu_selected_color()
}

/// The chip's ink — `on_accent`, the pair the ↓-focused footer chip and the
/// current ask-question chip already stand on: near-black on a dark theme's
/// accent, black on ANSI's cyan, and Latte's pale `base` on its blue —
/// Catppuccin's own text-on-accent rule, and the catalog's weakest contrast
/// at about 2.5:1 — so a theme that reads at the footer chip reads here.
pub fn agent_view_label_fg() -> Color {
    palette().on_accent
}

/// Indent of a Ctrl+O agent cell's section bodies (under the `⎿  ` corner's
/// label, one level further in) and of its nested tool-header lines.
pub const AGENT_BODY_INDENT: &str = "       ";

pub const AGENT_NESTED_INDENT: &str = "     ";

/// The footer roster (the persistent agent list under the footer): the
/// selection marker, the main row's bullet, and an agent row's circle.
pub const AGENT_LIST_MARKER: &str = "❯ ";

pub const AGENT_LIST_INDENT: &str = "  ";

pub const AGENT_MAIN_BULLET: &str = "● ";

pub const AGENT_ROW_BULLET: &str = "◯ ";

pub const AGENT_MAIN_LABEL: &str = "main";

/// The roster selection's footer hints (they take the footer line's slot).
pub const AGENT_HINT_MAIN: &[(&str, &str)] = &[("↑/↓", " to select"), ("Enter", " to view")];

pub const AGENT_HINT_AGENT: &[(&str, &str)] = &[("Enter", " to view"), ("x", " to stop")];

/// The same hint on a **settled** row (a user stop, or a finished agent still
/// lingering): the key is the same, its job isn't — `x` clears the row it
/// stopped rather than stopping it twice (`docs/agents/agent-tool.md`).
pub const AGENT_HINT_AGENT_DONE: &[(&str, &str)] = &[("Enter", " to view"), ("x", " to clear")];

// --- the screen-owning mode's session bar (docs/ui/workspace.md) ---

pub const SESSION_TAB_WIDTH: usize = 28;
pub const SESSION_BAR_ROWS: u16 = 1;
pub const SESSION_BAR_CHIP_GAP: &str = "  ";

// --- the inline tool-permission prompt (docs/state/permissions.md) ---

/// The prompt's outer frame: a full-width rule above and below, in the input
/// box's border colour so the modal reads as the same surface.
pub const PERMISSION_RULE: &str = "─";

/// The dashed rules framing a file change's numbered body, dim so the outer
/// frame stays the stronger line.
pub const PERMISSION_BODY_RULE: &str = "╌";

pub fn permission_body_rule_color() -> Color {
    tool_dim_color()
}

/// One-space inset on every text row (the body's numbers land here too).
pub const PERMISSION_INDENT: &str = " ";

/// The extra inset on a `bash` prompt's command and description rows.
pub const PERMISSION_COMMAND_INDENT: &str = "   ";

/// The action title (`Create file` / `Edit file` / `Bash command`) — the
/// palette accent, bold.
pub fn permission_title_color() -> Color {
    menu_selected_color()
}

/// ` · from the {type} agent`, appended to the title when a subagent asked.
pub const PERMISSION_AGENT_SEPARATOR: &str = " · from the ";

pub const PERMISSION_AGENT_SUFFIX: &str = " agent";

pub fn permission_agent_color() -> Color {
    tool_dim_color()
}

/// The file path under the title, and the command on a `bash` prompt.
pub fn permission_target_color() -> Color {
    tool_output_color()
}

/// The model's own description of a `bash` call, under the command.
pub fn permission_detail_color() -> Color {
    tool_dim_color()
}

/// The standing notice above a `bash` prompt's question.
pub const PERMISSION_NOTICE: &str = "This command requires approval";

pub fn permission_notice_color() -> Color {
    tool_dim_color()
}

/// The `❯ ` on the highlighted option row (the unselected rows indent by its
/// width so the list stays aligned).
pub const PERMISSION_MARKER: &str = "❯ ";

/// The highlighted option row lights up whole — marker, number, and label —
/// like the slash-command palette's selection.
pub fn permission_selected_color() -> Color {
    menu_selected_color()
}

/// The hint row under the options, `{key}{label}` pairs joined by ` · `.
pub const PERMISSION_HINT_SEPARATOR: &str = " · ";

pub fn permission_hint_key_color() -> Color {
    menu_selected_color()
}

pub fn permission_hint_text_color() -> Color {
    tool_dim_color()
}

/// Tab's amend field: the hints that replace the option row's set.
pub const PERMISSION_AMEND_HINTS: &[(&str, &str)] = &[
    ("Enter", " to reject with this feedback"),
    ("Esc", " to go back"),
];

/// Rows between the last option/amend row and the bottom of the prompt (gap,
/// hint, rule — no blank row hugs the rule, `docs/ui/view-flow.md`) — how
/// [`cursor_position`] seats the cursor on the highlighted option (or in the
/// amend field) from the region's bottom edge without re-deriving the body.
/// `permission_lines` pads a capped prompt *above* the question to keep that
/// block flush against this tail.
pub const PERMISSION_TAIL_ROWS: u16 = 3;

/// The most rows one option label may wrap to before it caps with a `…` —
/// a long "don't ask again" rule (an exact command) wraps instead of hiding
/// its tail, but a *pathological* one (kilobytes on a line) must not stack
/// the option block taller than the terminal and push `3. No` and the hints
/// off the bottom (the region clamps to the screen and paints top-down).
/// Four ~100-column rows show any realistic command whole.
pub const PERMISSION_OPTION_MAX_ROWS: usize = 4;

/// The body rows a permission prompt is guaranteed even when a big parallel
/// batch queues a screenful of `⎿ Waiting…` siblings above it. The body is
/// the point of the prompt — you read what you approve — so the *context*
/// gives way first: excess sibling cells collapse into the dim
/// `… +N more waiting` summary row rather than squeezing the body's budget
/// to nothing (the "prompt with no content" bug). Sized like the inline
/// cell's [`FILE_PEEK_LINES`] peek, which is what a squeezed body degrades
/// to (numbered rows + the `… +N lines` tail). A body naturally shorter
/// reserves only what it needs.
pub const PERMISSION_MIN_BODY_ROWS: usize = 10;

/// The prompt body's safety ceiling. The body is shown **whole** — a page
/// taller than the terminal flows into scrollback rather than capping
/// (`docs/ui/view-flow.md`) — but the page is rebuilt (and its body
/// syntax-highlighted) every draw tick while the prompt is open, so a
/// pathological multi-megabyte write must not turn each frame into an
/// unbounded build. Past this many body rows the familiar `… +N lines` tail
/// returns — far beyond anything a human reviews, bounded for the loop.
pub const PERMISSION_BODY_MAX_ROWS: usize = 2_000;

// --- the inline AskUserQuestion modal (docs/tools/ask.md) ---

/// One-space inset on every text row — the permission prompt's, so the two
/// modals read as the same surface (both share [`PERMISSION_RULE`] frames,
/// [`PERMISSION_MARKER`], and the hint-row colours).
pub const ASK_INDENT: &str = " ";

/// The chip-strip glyphs: an unanswered question's box, an answered one's
/// checked box, and the Submit tab's check.
pub const ASK_CHIP_UNANSWERED: &str = "☐";

pub const ASK_CHIP_ANSWERED: &str = "☒";

pub const ASK_CHIP_SUBMIT: &str = "✔";

/// The dim `←`/`→` bookends of a multi-question chip strip — the reminder
/// that ←/→ move between the tabs.
pub const ASK_ARROW_LEFT: &str = "←";

pub const ASK_ARROW_RIGHT: &str = "→";

/// The gap between chips.
pub const ASK_CHIP_GAP: &str = "  ";

/// The **current** chip lights on the selection background — the cyan block
/// that says which section the keys act on (the user-requested highlight).
pub fn ask_chip_current_bg() -> Color {
    menu_selected_color()
}

pub fn ask_chip_current_fg() -> Color {
    footer_focus_fg()
}

/// The idle chips, dim so the current one carries the eye.
pub fn ask_chip_color() -> Color {
    tool_dim_color()
}

/// A multi-select option's checkbox, checked and not.
pub const ASK_CHECKED: &str = "[✔] ";

pub const ASK_UNCHECKED: &str = "[ ] ";

/// The green check after a single-select question's chosen label
/// (`1. Black ✔`).
pub const ASK_PICKED_MARK: &str = " ✔";

pub fn ask_picked_color() -> Color {
    tool_ok_color()
}

/// An option's description, dim under its label.
pub fn ask_desc_color() -> Color {
    tool_dim_color()
}

/// The auto-added free-text row's label.
pub const ASK_OTHER_LABEL: &str = "Type something.";

/// A multi-select question's own confirm row (unnumbered, per the reference).
pub const ASK_CONFIRM_LABEL: &str = "Submit";

/// The row that resolves the whole call as "let's talk instead".
pub const ASK_CHAT_LABEL: &str = "Chat about this";

/// The notes line under a preview panel: the label, the dim placeholder
/// before any notes exist, and the hint key that opens the field.
pub const ASK_NOTES_LABEL: &str = "Notes: ";

pub const ASK_NOTES_PLACEHOLDER: &str = "press n to add notes";

/// The preview panel's box-drawing corners and edges, dim like the body
/// rules so the content carries the eye.
pub fn ask_preview_color() -> Color {
    tool_dim_color()
}

/// The widest the option column may grow in the side-by-side layout, as a
/// share of the region width — the preview box keeps the rest.
pub const ASK_LEFT_MAX_SHARE: f32 = 0.45;

/// The most rows the preview box shows of an option's preview content before
/// clipping (the box is a peek, not a pager).
pub const ASK_PREVIEW_MAX_ROWS: usize = 12;

/// The Submit page's texts.
pub const ASK_REVIEW_TITLE: &str = "Review your answers";

pub const ASK_REVIEW_QUESTION: &str = "Ready to submit your answers?";

pub const ASK_SUBMIT_LABEL: &str = "Submit answers";

pub const ASK_CANCEL_LABEL: &str = "Cancel";

/// The review page's warning when any question is still unanswered — the
/// submission would be partial, and the list below shows only what was
/// answered.
pub const ASK_WARNING: &str = "⚠ You have not answered all questions";

/// The warning's colour — the retry/system amber, the one "caution" tone the
/// theme already speaks.
pub fn ask_warning_color() -> Color {
    status_retry_color()
}

/// The review page's `→ {answer}` text — green, so the recorded answer is
/// the row that carries the eye.
pub fn ask_answer_color() -> Color {
    tool_ok_color()
}

/// The most wrapped rows one review answer shows before capping with a dim
/// `…` row — an expanded multi-kilobyte paste must not flood the page (the
/// committed cell and the answers JSON still carry it whole).
pub const ASK_REVIEW_ANSWER_MAX_ROWS: usize = 4;

/// The idle clock's countdown, right-aligned in the modal's closing rule
/// while it runs (`docs/tools/ask.md`): `── Timeout: 9:41 ─`. The rule is the one
/// row a bottom-anchored page always paints, so a ticking label there never
/// re-signs the flow and never costs the page a row.
pub const ASK_TIMEOUT_PREFIX: &str = "Timeout: ";

/// The fewest rule cells left of the countdown — below this the closing rule
/// stays plain rather than reading as a chip with a stub of a frame.
pub const ASK_TIMEOUT_MIN_LEAD: usize = 2;

/// Inside its last minute the countdown takes [`ask_timeout_warn_bg`].
pub const ASK_TIMEOUT_WARN: std::time::Duration = std::time::Duration::from_secs(60);

/// The countdown's ground: it rides the closing rule as a **lit chip**, the
/// agent session view's label dress ([`agent_view_label_bg`]) — the theme's
/// accent, so the one row saying the question will not wait forever reads at
/// a glance rather than as dim text in a dim rule (the user-requested fill).
/// The label's padding sits inside the fill, one bare rule cell after it.
pub fn ask_timeout_bg() -> Color {
    agent_view_label_bg()
}

/// The chip's ink — `on_accent`, the agent label's own
/// ([`agent_view_label_fg`]), over the calm and the last-minute ground alike.
pub fn ask_timeout_fg() -> Color {
    agent_view_label_fg()
}

/// The chip's last-minute ground — the caution amber the review page's
/// warning already speaks, the cue that a user who is here should say so.
pub fn ask_timeout_warn_bg() -> Color {
    ask_warning_color()
}

/// The review page's `● {question}` bullet and the `→ {answer}` arrow.
pub const ASK_REVIEW_BULLET: &str = "● ";

pub const ASK_ANSWER_ARROW: &str = "→ ";

// --- Full-screen Git review (/diff). Geometry is shared with paging. ---
pub const DIFF_MARGIN: u16 = 1;
pub const DIFF_HEADER_ROWS: u16 = 4;
pub const DIFF_FOOTER_ROWS: u16 = 2;
pub const DIFF_PANE_HEADER_ROWS: u16 = 1;
pub const DIFF_SPLIT_MIN_WIDTH: u16 = 76;
pub const DIFF_SIDEBAR_MIN: u16 = 26;
pub const DIFF_SIDEBAR_MAX: u16 = 42;
pub const DIFF_PANE_GAP: u16 = 1;
pub const DIFF_FILE_ROWS: usize = 2;
pub const DIFF_MIN_GUTTER: usize = 3;
pub const DIFF_COMPACT_FILTER_WIDTH: u16 = 58;
pub const DIFF_BADGE: &str = " DIFF ";
pub const DIFF_SELECTED: &str = "▎";
pub const DIFF_SEPARATOR: &str = " · ";
pub const DIFF_GUTTER_SEPARATOR: &str = "│";
pub const DIFF_SCROLL_TRACK: &str = "│";
pub const DIFF_SCROLL_THUMB: &str = "┃";
pub const DIFF_CARET: &str = "▏";

pub fn diff_pane_bg() -> Color {
    palette().user_bg
}

pub fn diff_border_color(focused: bool) -> Color {
    if focused {
        menu_selected_color()
    } else {
        tool_dim_color()
    }
}
