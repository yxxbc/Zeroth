//! Tool-call, diff, task, and background-cell styling constants.

use super::palette::palette;
use super::*;

// --- Tool-call styling. A tool renders as a coloured bullet header
// `● name(args)` plus a collapsed `⎿` peek of its output; the bullet colour is
// the tool's lifecycle (a blinking grey while it runs, green ok, red fail —
// `docs/ui/tool-pulse.md`). The full output is
// only shown in the Ctrl+O tool-output view, never inline. ---

/// Subtle card ground for expanded tool output blocks.
pub fn tool_bg_color() -> Color {
    palette().tool_bg
}

/// Bullet prefixing a tool call (same glyph as the assistant, recoloured by
/// status — see [`tool_status_color`]).
pub const TOOL_BULLET: &str = "● ";

/// Prefix for the first result line: indent + a turnstile glyph + two spaces
/// (Claude-Code's two-space corner). Continuation lines are indented by its
/// display width so a multi-line result aligns under the content (see
/// [`result_row`]).
pub const TOOL_RESULT_PREFIX: &str = "  ⎿  ";

/// Prefix for the "+N lines" hint line under a capped peek — the `…` aligns
/// under the corner content (the [`TOOL_RESULT_PREFIX`] width of leading
/// spaces).
pub const TOOL_MORE_PREFIX: &str = "     … ";

/// Hint telling the user how to see the full output.
pub const EXPAND_HINT: &str = " (ctrl+o to expand)";

/// The hook-run cell's words (`docs/tools/hooks.md` *The hook-run cell*): the
/// header's `Ran 1 Stop hook` around the count and the event, and the
/// expanded cell's file row under a hook's command — a refusal's `Config:`
/// line. Every row under the header is the report's own words.
pub const HOOK_CELL_RAN: &str = "Ran ";
pub const HOOK_CELL_HOOK: &str = " hook";
pub const HOOK_CELL_HOOKS: &str = " hooks";
pub const HOOK_CELL_CONFIG: &str = "Config: ";

/// The collapsed read/search group's glyphs (`docs/ui/tool-groups.md`): the `…`
/// that closes an open group's `● Reading 2 files…` line, and the `$ ` its
/// `⎿` hint row puts in front of a command (a read's hint is its path).
pub const TOOL_GROUP_ACTIVE_SUFFIX: &str = "…";
pub const TOOL_GROUP_COMMAND_PROMPT: &str = "$ ";

/// How long a command in an open read/search group must have run before the
/// group's `⎿` row wears its clock — `$ grep -rn todo . (12s · 34 lines)` —
/// the reference's two seconds, so a fast command never flashes one and a
/// slow one does not look frozen (`docs/ui/tool-groups.md` *The hint row*).
pub const TOOL_GROUP_CLOCK_DELAY: Duration = Duration::from_secs(2);

/// The window the **running** command tail shows (`running_command_lines`):
/// four wrapped display **rows** of what the command has printed, ending at
/// the lowest row still moving (`LiveTail::anchor` — the last row for every
/// command whose newest output is at its end, `docs/ui/tool-streaming.md`),
/// over the `+N lines (Ns · wait …)` clock row — the unit the user reads
/// the cell in (`docs/ui/long-lines.md`). It is also the ceiling of the
/// **settled** block,
/// hint included: [`TOOL_FOLD_ROWS`] rows over the `… +N lines` hint, or
/// `TOOL_FOLD_ROWS + 1` rows shown whole — so a `bash` cell never *grows*
/// when it settles, whatever shape its output has; the rest is one `ctrl+o`
/// away.
pub const TOOL_PEEK_ROWS: usize = 4;

/// The marker spliced in (before the closing `)`) when a collapsed header's
/// command is cut at [`TOOL_HEADER_MAX_LINES`] lines,
/// [`TOOL_HEADER_MAX_COLS`] columns or [`TOOL_HEADER_MAX_ROWS`] rows
/// (`tool_header_lines`).
pub const TOOL_HEADER_ELLIPSIS: &str = "…";

/// How many **lines** of a command the collapsed header shows before the rest
/// is cut — Claude Code's `Bash(…)` header keeps two (`docs/tools/tools.md`
/// *Long headers*). The Ctrl+O transcript shows every line.
pub const TOOL_HEADER_MAX_LINES: usize = 2;

/// How many display **columns** of a command the collapsed header shows
/// before the rest is cut with [`TOOL_HEADER_ELLIPSIS`] — Claude Code's 160,
/// a budget on the *text*, so a terminal wide enough to hold it in
/// [`TOOL_HEADER_MAX_ROWS`] rows cuts the same command at the same character.
pub const TOOL_HEADER_MAX_COLS: usize = 160;

/// How many display **rows** of arguments the collapsed header shows: where
/// [`TOOL_HEADER_MAX_COLS`] columns would wrap onto a third row, the header
/// stops at the end of the second, `…)` on its last word (`docs/tools/tools.md`
/// *Long headers*). A spilled header's name-only first row is not counted.
pub const TOOL_HEADER_MAX_ROWS: usize = 2;

/// How many display **rows** of its output a settled exec cell shows *above
/// the fold* — Claude Code's three — before the rest collapses behind the
/// `… +N lines (ctrl+o to expand)` hint (`docs/ui/long-lines.md`). An output of
/// exactly `TOOL_FOLD_ROWS + 1` rows shows whole: a hint hiding one row would
/// cost the very row it hides. So the block is at most [`TOOL_PEEK_ROWS`]
/// rows either way, hint included.
pub const TOOL_FOLD_ROWS: usize = 3;

/// Search previews stay bounded; expansion keeps all retained results.
pub const SEARCH_PREVIEW_ENTRIES: usize = 3;
pub const SEARCH_PREVIEW_ROWS: usize = 2;
pub const SEARCH_INDENT: &str = "  ";
pub const SEARCH_LINE_SEPARATOR: &str = " │ ";
pub const SEARCH_LABEL: &str = "Search";
pub const SEARCH_FILES_LABEL: &str = "Files";
pub const SEARCH_NO_MATCHES: &str = "No matches";
pub const SEARCH_RETAINED_LABEL: &str = "retained results";
pub const SEARCH_OUTPUT_LABEL: &str = "Output";
pub const SEARCH_COMMAND_LABEL: &str = "Command";

/// The `web_fetch` cell's opening words (`docs/tools/web-search.md`
/// *Rendering*): the fetched page under `● {url}`, and — in Ctrl+O, where the
/// boundary paragraph really shows — the cell's one-line summary.
pub const WEB_FETCH_SUMMARY: &str = "Read ";
pub const WEB_FETCH_ROWS_UNIT: &str = " rows";
pub const WEB_FETCH_CHARS_UNIT: &str = " chars";

/// The size cap on the display-only JSON reshaping of an exec cell's output
/// (`docs/ui/long-lines.md` *Pretty JSON*): an output longer than this many
/// bytes is shown as it came — Claude Code's own 10 000-character guard,
/// which bounds the `Value` tree the reshaping parses per render.
pub const TOOL_JSON_PRETTY_MAX_BYTES: usize = 10_000;

/// The size cap on the syntax highlighting of **one** expanded file body — the
/// Ctrl+O transcript's `read`/`write`/`edit` cell, which shows every line of the
/// file (`docs/ui/long-lines.md` *Highlighting a big body*).
///
/// Past it the remaining rows render plain: **every line of text still shows**,
/// only the syntax colour stops. Same shape as [`TOOL_JSON_PRETTY_MAX_BYTES`],
/// and the same trade the highlighter's own per-line cap already makes
/// (`highlight::MAX_LINE_BYTES`).
///
/// Chosen from measurement, not taste. Syntect's markdown grammar goes
/// non-linear on ordinary prose — a real 4.8 KB skill doc cost 91 ms to colour
/// while a 6.5 KB one cost 4 ms — so the cap bounds *total* colour work per
/// cell. Across the eight markdown cells of a real session (measured on the
/// expanded transcript path, warm grammar):
///
/// | cap | total | worst cell |
/// | --- | --- | --- |
/// | none | 986 ms | 526 ms |
/// | 4 KB | 166 ms | 91 ms |
/// | **2 KB** | **40 ms** | **25 ms** |
/// | 1 KB | 31 ms | 25 ms |
/// | 512 B | 29 ms | 25 ms |
///
/// 2 KB is where the curve flattens: the remaining cost is one grammar's
/// one-time work per cell, not the rows. It is also enough colour for the head
/// of any file a reader opens in the transcript — the part they read first.
pub const CODE_HIGHLIGHT_MAX_BYTES: usize = 2048;

/// The parens framing a header's arguments — `● {name}({args})`. The opening
/// one rides the name's row (so an argument too wide for what is left beside
/// the name spills to the continuation row whole), the closing one the last
/// argument row; both wear the arguments' bold white (`tool_header_lines`).
pub const TOOL_HEADER_OPEN: &str = "(";
pub const TOOL_HEADER_CLOSE: &str = ")";

/// The share of the terminal a wrapped header may spend aligning its
/// continuation rows under the opening `(`: at most one part in this many.
/// Past it (an MCP call's `● deepwiki - ask_question (MCP)` is 31 columns —
/// 40% of an 76-column terminal) the header hangs its continuations at the
/// bullet's own two columns instead, so the arguments get the width rather
/// than a ragged column. See `tool_header_lines` and `docs/tools/mcp.md`.
pub const TOOL_HEADER_ALIGN_SHARE: usize = 3;

/// Dim marker appended at the end of a `!` shell command's **expanded** output
/// (`tool_full_lines`) when it was cut at the in-memory cap (`tool.truncated`),
/// to show that more output was dropped. See `docs/input/shell-command.md`.
pub const TOOL_TRUNCATED_MARKER: &str = "…";

/// Placeholder body for a finished tool that produced no output.
pub const TOOL_NO_OUTPUT: &str = "(no output)";

// ---------------------------------------------------------------------------
// The screen-owning mode's sidebar (docs/ui/sidebar.md): a column at the
// screen's left edge carrying the brand, the session's facts and what is
// running. Its widths and band limits live with the module's policy; these are
// the words and marks it draws.
// ---------------------------------------------------------------------------

/// The seam between the sidebar and the conversation.
pub const SIDEBAR_SEAM: &str = "│";

/// The product name, as the banner spells it.
pub const SIDEBAR_NAME: &str = "Zeroth";

/// The one-line slogan — the page title of `about.html`, in the TUI's language.
pub const SIDEBAR_SLOGAN: &str = "a coding agent that lives in your terminal";

/// The offline demo's call to action, where the banner puts it.
pub const SIDEBAR_CONNECT: &str = "Connect a model →";

/// Section headings, lower-case like the footer's words.
pub const SIDEBAR_SESSION: &str = "session";
pub const SIDEBAR_RUNNING: &str = "running";

/// The session facts' labels.
pub const SIDEBAR_MODEL: &str = "model";
pub const SIDEBAR_CWD: &str = "cwd";
pub const SIDEBAR_MODE: &str = "mode";
pub const SIDEBAR_CONTEXT: &str = "context";

/// The task marks: the one in flight, and the ones waiting.
pub const SIDEBAR_TASK_ACTIVE: &str = "▸";
pub const SIDEBAR_TASK_PENDING: &str = "·";

/// A running agent, and a running background shell's row lead.
pub const SIDEBAR_AGENT: &str = "◯";
pub const SIDEBAR_SHELL: &str = "⌁";

/// The context meter: what is used and what is left, over `SIDEBAR_GAUGE_COLS`.
pub const SIDEBAR_GAUGE_FILL: &str = "█";
pub const SIDEBAR_GAUGE_EMPTY: &str = "░";
pub const SIDEBAR_GAUGE_COLS: u16 = 8;

/// The mascot graphic's box in the sidebar: its column count (clamped to the
/// column), its minimum before the art is dropped, and its rows. Narrower than
/// the banner's, and it never fights the meta rows beside it — the sidebar has
/// no beside.
pub const SIDEBAR_MASCOT_COLS: u16 = 16;
pub const SIDEBAR_MASCOT_MIN_COLS: u16 = 10;
pub const SIDEBAR_MASCOT_ROWS: u16 = 8;

/// The brand tile's rows in the sidebar's brand section (`docs/brand-icons.md`):
/// the session's model/provider logo, drawn under the mascot on a terminal
/// that paints graphics — a three-row square beside the name, so the column
/// says *who* this session talks to in the same block that says what the
/// project is. The glyph already leads the `model` fact row below; this is
/// the picture for the surface with rows to spare.
pub const SIDEBAR_BRAND_LOGO_COLS: u16 = 3;
pub const SIDEBAR_BRAND_LOGO_ROWS: u16 = 3;
pub const SIDEBAR_BRAND_LOGO_MIN_COLS: u16 = 2;
pub const SIDEBAR_MASCOT_PX: (u32, u32) = (384, 351);

/// A heading: dim, so the body under it is what the eye lands on.
pub fn sidebar_heading_color() -> Color {
    tool_dim_color()
}

/// The product name: the brightest ink, one step over the slogan.
pub fn sidebar_name_color() -> Color {
    palette().text
}

/// Labels and numbers — the footer's dim, for the same reason.
pub fn sidebar_meta_color() -> Color {
    tool_dim_color()
}

/// The slogan: quieter than the name, louder than a label.
pub fn sidebar_slogan_color() -> Color {
    palette().text_muted
}

/// A fact's value and a running item's name.
pub fn sidebar_value_color() -> Color {
    palette().text_muted
}

/// The accent: the in-flight task's mark, a running agent, `/login`.
pub fn sidebar_accent_color() -> Color {
    palette().accent
}

// ---------------------------------------------------------------------------
// The screen-owning conversation's scrollbar (docs/ui/full-screen.md). One
// column at the conversation's right edge; the thumb is the accent so it reads
// as "you are here" without competing with the reply's ink.
// ---------------------------------------------------------------------------

/// The thumb: a solid half-block, the densest thing a one-column bar can be.
pub const SCROLLBAR_THUMB: &str = "▐";

/// The thumb's ink — the accent, the same "this is the live thing" colour the
/// footer's model name wears.
pub fn scrollbar_thumb_color() -> Color {
    palette().accent
}

/// The pinned prompt's ellipsis: the one column a summary too long for the
/// frame gives up to say so (`pinned_prompt_line`,
/// `docs/ui/conversation-tool-details-and-pin.md`). A separate constant from
/// [`TOOL_TRUNCATED_MARKER`] because the two say different things — one marks
/// dropped *output*, the other a cut *summary* — and a theme may want them to
/// differ.
pub const PINNED_PROMPT_ELLIPSIS: &str = "…";

// ---------------------------------------------------------------------------
// The thinking stream (docs/ui/thinking-stream.md). It borrows the tool cell's
// shape while it runs — a [`TOOL_BULLET`] `● Thinking…` header (its bullet
// blinking like a running tool's, its label carrying the status line's
// white shimmer sweep) over the chain-of-thought in the
// [`TOOL_RESULT_PREFIX`] `⎿` gutter — and then collapses into a **bullet-less**
// `Thought for … · … tokens (ctrl+o to expand)` line: the [`summary_lines`]
// shape *and* its dim, because a settled thought is turn meta, not a cell —
// a footnote about work already done. The strip is where the weight and the
// motion are: nothing there is ever committed, so it can afford both.
// ---------------------------------------------------------------------------

/// The chain-of-thought's own colour — dim, and italic
/// ([`REASONING_TEXT_MODIFIER`]), so it reads as the model thinking aloud
/// rather than as a tool's output in the same `⎿` gutter.
pub fn reasoning_text_color() -> Color {
    tool_dim_color()
}

/// Italic: the one cue separating a thought's `⎿` body from a tool's.
pub const REASONING_TEXT_MODIFIER: Modifier = Modifier::ITALIC;

/// The live block's header, under which the thought tails.
pub const REASONING_RUNNING: &str = "Thinking…";

/// The **resting** colour of the live header's shimmer — what `Thinking…`
/// reads as between crests, which is most of the sweep (the band is
/// [`SHIMMER_BAND_HALF_WIDTH`] wide inside a period of the text plus twice
/// [`SHIMMER_PADDING`]).
///
/// Deliberately *not* codex's grey: that makes a verb read as grey text with
/// a white wave, which is right for a metric and wrong for a header — at rest
/// it is indistinguishable from the dim body under it. This near-white floor reads as **bold white**, with the wave a
/// brightening on top rather than the only thing making it visible — the one
/// place in the feature that draws the eye, because it is the one place
/// something is still happening.
pub fn reasoning_shimmer_base() -> Color {
    model_id_color()
}

/// The settled line's opener — `Thought for 1m 5s · 1.5k tokens`.
pub const REASONING_DONE: &str = "Thought for ";

/// The settled line's colour: [`status_done_color`], the dim the committed
/// `Done for Ns` summary wears. The two are the same kind of line — turn meta,
/// bullet-less, one row — and they bracket a turn, so they read as a pair.
///
/// One tone across the whole line and both surfaces: a settled thought is a
/// footnote about work already done, not a heading. What is *happening* — the
/// live `● Thinking…` block — is what carries weight and motion.
pub fn reasoning_label_color() -> Color {
    status_done_color()
}

/// How many **wrapped display rows** of the thought the live block tails —
/// three, so with its header the block is at most four rows tall. Rows, not
/// source lines: a line that wraps onto two rows spends two of them. The
/// window is small on purpose: it grows the live region upward (invariant 3),
/// and what the user wants is the frontier — what the model is thinking
/// *now*. The whole thing is in the Ctrl+O transcript.
pub const REASONING_PEEK_LINES: usize = 3;

// ---------------------------------------------------------------------------
// Background shells (docs/tools/background.md): the backgrounded cell's fixed row,
// the running-command Ctrl+B hint, the ↓ manager band, and the footer count.
// ---------------------------------------------------------------------------

/// The fixed `⎿` body of a call resolved as [`ToolStatus::Backgrounded`] —
/// the stored output (the model-facing launch text) is never shown.
pub const TOOL_BACKGROUNDED: &str = "Running in the background (↓ to manage)";

/// The dim live-only hint under a running command's preview cell: Ctrl+B
/// moves it to the background. Never committed to scrollback.
pub const TOOL_BACKGROUND_HINT: &str = "(ctrl+b to run in background)";

/// The same hint under a running `bashwait` or `bashsend`, whose session
/// already runs in the background: there Ctrl+B ends the wait, not the
/// command.
pub const TOOL_STOP_WAITING_HINT: &str = "(ctrl+b to stop waiting)";

/// How long a command must have been running before its preview shows the
/// `(ctrl+b to run in background)` hint — Claude-Code-style, so a command that
/// finishes right away never flashes it (Ctrl+B itself still works the whole
/// time; only the discoverability hint waits). Gated on the boundary-injected
/// [`App::background_hint_elapsed`]. See `docs/tools/background.md`.
pub const TOOL_BACKGROUND_HINT_DELAY: Duration = Duration::from_secs(3);

/// The separator inside a running command's clock clause, between the
/// command's own elapsed and how long its call waits for it:
/// `(22s · wait 1m 50s)` — the footer's [`FOOTER_SEPARATOR`], the one
/// dot the whole chrome joins facts with (`docs/ui/tool-streaming.md`).
pub const TOOL_CLOCK_SEPARATOR: &str = " · ";

/// The label in front of the wait in that clause — `wait 1m 50s`. A limit
/// needs naming where an elapsed does not: bare, the two numbers would read
/// as a range. A *wait*, not a timeout: the command is not stopped when it
/// passes, it goes on as a session (`docs/tools/bash-tools.md`).
pub const TOOL_TIMEOUT_LABEL: &str = "wait ";

/// The ↓ manager's list title.
pub const BG_TITLE: &str = "Background";

/// The details page's title.
pub const BG_DETAILS_TITLE: &str = "Shell details";

/// The list's empty state — shown when every shell has finished.
pub const BG_EMPTY: &str = "No tasks currently running";

/// The list page's key hints.
pub const BG_LIST_HINTS: &str = "↑/↓ to select · Enter to view · x to stop · Esc to close";

/// The empty state's key hints (nothing to stop).
pub const BG_EMPTY_HINTS: &str = "↑/↓ to select · Enter to view · Esc to close";

/// The details page's key hints.
pub const BG_DETAILS_HINTS: &str = "← to go back · Esc/Enter/Space to close · x to stop";

/// The `(running)` suffix on a list row.
pub const BG_ROW_SUFFIX: &str = " (running)";

/// The band's two-space inset (the picker/footer indent).
pub const BG_INDENT: &str = "  ";

/// The selected list row's marker (the resume picker's `❯`).
pub const BG_MARKER: &str = "❯ ";

/// At most this many list rows show at once (the window follows the
/// selection, like the pickers).
pub const BG_MENU_MAX_ROWS: usize = 8;

/// The details output box's interior height: the last rows of the live
/// output tail, blank-padded (the mock's fixed box).
pub const BG_OUTPUT_ROWS: usize = 10;

/// The details page's field labels, padded to one column.
pub const BG_FIELD_STATUS: &str = "Status:   ";

pub const BG_FIELD_RUNTIME: &str = "Runtime:  ";

pub const BG_FIELD_COMMAND: &str = "Command:  ";

/// The launcher field a subagent-launched shell adds (`From: {type} agent`).
pub const BG_FIELD_FROM: &str = "From:     ";

/// The details page's output-box heading.
pub const BG_OUTPUT_LABEL: &str = "Output:";

/// The value of the status field while listed (an exited shell leaves the
/// manager, so a listed one is always running).
pub const BG_STATUS_RUNNING: &str = "running";

/// The manager's selection accent (the palette accent the whole picker
/// family selects with) and dim text. Its titles wear the pages' title
/// dress (`header::page_title_line`).
pub fn bg_selected_color() -> Color {
    menu_selected_color()
}

pub fn bg_dim_color() -> Color {
    tool_dim_color()
}

/// The notice bullet colours: green success, red failure/stop — and the
/// caution amber for a session waiting for input (`docs/tools/bash-tools.md`),
/// which has not ended either way.
pub fn bg_notice_ok_color() -> Color {
    tool_ok_color()
}

pub fn bg_notice_fail_color() -> Color {
    tool_fail_color()
}

pub fn bg_notice_waiting_color() -> Color {
    palette().warning
}

/// A subagent the user's message started again (`docs/agents/agent-tools.md` *A
/// resume is news when it starts*): running, so neither an outcome's green
/// nor its red — the accent a system notice wears.
pub fn agent_notice_resumed_color() -> Color {
    system_color()
}

/// Placeholder body for a still-executing tool — the `⎿ Running…` row, shown
/// under a backend tool's `● name(args)` header (req 2: a running cell shows the
/// header *and* this row, previewed live) and as the whole `!` shell cell.
pub const TOOL_RUNNING: &str = "Running…";

/// Placeholder body for a tool queued in a **parallel batch** but not yet
/// started — the dim `⎿ Waiting…` row shown under a not-yet-running sibling's
/// `● name(args)` header while another call in the batch executes. See
/// `docs/tools/parallel-tools.md`.
pub const TOOL_WAITING: &str = "Waiting…";

/// The **resting** colour of a tool that is still executing: the same grey the
/// permission prompt shows over the call it is asking about, so everything
/// in flight reads muted and only the green/red *resolution* lands as colour.
/// (It was a blue `#61AFEF`; the blue survives as [`context_user_color`], which
/// is a role tag, not a running state.)
///
/// In the **live region** the bullet does not sit still — it **blinks**
/// ([`bullet_span`](super::tool::bullet_span)), Claude-Code's running dot:
/// shown in this one grey for half of every [`TOOL_PULSE_PERIOD`], then
/// hidden behind blanks of its own width for the other half. It is the only
/// colour a running bullet ever wears — there is no dimmer second shade for
/// it to ease toward — so a frozen render (a scrollback commit, the Ctrl+O
/// transcript's pager) shows exactly this, and the strip shows this or
/// nothing.
pub fn tool_running_color() -> Color {
    tool_dim_color()
}

// The running bullet's **cadence**, and the breath the `pulse` spinner style
// still borrows (`docs/ui/spinner.md`). The bullet itself no longer blends
// colours — it blinks (`tool::tool_pulse_visible`) — but the `pulse` spinner
// keeps the raised-cosine dim→bright→dim breath these two ends describe, its
// ends blended through `wrap::blend_color` (which steps rather than mixes when
// a theme names a terminal-palette colour, `docs/ui/theme.md`), in step with the
// bullet off the same boundary-injected frame clock (`App::set_pulse`). See
// `docs/ui/tool-pulse.md`.

/// The dim end of the `pulse` spinner's breath — where its cycle starts and
/// ends. Well below the resting grey, so the dip carries the whole animation
/// without the dot ever vanishing.
pub fn tool_pulse_dim() -> Color {
    palette().pulse_dim
}

/// The bright end of that breath at the tool bullet's level —
/// [`tool_dim_color`] exactly, the running bullet's own grey — which the
/// `bars` spinner style takes for its lowest bar so even `▁` reads
/// ([`spinner_bars_low`]); the `pulse` style crests past it to white.
pub fn tool_pulse_bright() -> Color {
    tool_dim_color()
}

/// One full blink — shown for the first half, hidden for the second — and one
/// full breath of the `pulse` spinner. Slow enough to read as a pulse rather
/// than a flicker, brisk enough to say "something is happening" — and
/// comfortably coarser than the 32 ms animation frame the loop re-arms while
/// a turn runs.
pub const TOOL_PULSE_PERIOD: Duration = Duration::from_millis(1000);

/// Dim grey — a tool queued in a batch but not yet started (its `● name(args)`
/// bullet and `⎿ Waiting…` row read muted; it is the running head's *resting*
/// grey too, the difference being that a running bullet **moves**,
/// since it hasn't begun). Shares the argument/peek dim grey.
pub fn tool_waiting_color() -> Color {
    tool_dim_color()
}

/// Green — a tool that finished successfully. A vivid, saturated green (rather
/// than the old muted `#98C379`) so the `●` success bullet clearly stands out,
/// Claude-Code style. Shared by the `+`-line diff colour, the active-model tick
/// and the context view's assistant tag ([`tool_diff_add_color`] etc.).
pub fn tool_ok_color() -> Color {
    palette().success
}

/// Red — a tool that failed (shares the backend-error red).
pub fn tool_fail_color() -> Color {
    error_color()
}

/// White — the tool's name.
pub fn tool_name_color() -> Color {
    ai_color()
}

/// White (the normal assistant reply colour) + bold — the whole `(...)` header
/// body: the command/args text **and** its framing `(`/`)` (and a truncation
/// `…`) alike, so a `bash` command and its brackets read as prominently as a
/// normal reply rather than the old dim grey. Claude-Code's noticeable tool
/// header; shared by every tool via [`tool_header_lines`].
pub fn tool_args_color() -> Color {
    ai_color()
}

/// White (the normal reply colour) — a finished tool's **output** under the `⎿`
/// gutter (command/shell output), so it's as legible as a normal message rather
/// than dim grey. The `⎿` corner, the `Running…`/`Waiting…`/`(no output)`
/// placeholders and the `… +N lines` hint all stay [`tool_dim_color`].
pub fn tool_output_color() -> Color {
    ai_color()
}

/// Dim grey — a tool's `⎿` gutter corner, its `Running…`/`Waiting…`/`(no output)`
/// placeholders and the `… +N lines` hint.
pub fn tool_dim_color() -> Color {
    palette().dim
}

// ===== The task tools' live checklist (docs/tools/task-tools.md) =====

/// The checklist's status glyphs — Claude Code's `figures` trio: pending's
/// empty square, in-progress's filled square, completed's tick.
pub const TASK_PENDING_GLYPH: &str = "◻";
pub const TASK_IN_PROGRESS_GLYPH: &str = "◼";
pub const TASK_COMPLETED_GLYPH: &str = "✔";

/// The in-progress glyph's colour — the system cyan (Claude Code paints its
/// filled square in the brand colour; cyan is this TUI's accent). The
/// subject beside it renders bold in the normal reply colour.
pub fn task_in_progress_color() -> Color {
    system_color()
}

/// The completed glyph's green ([`tool_ok_color`]); the subject beside it is
/// dim and struck through — Claude Code's done row.
pub fn task_completed_color() -> Color {
    tool_ok_color()
}

/// The dim `› blocked by #1, #2` suffix's marker (Claude Code's
/// `figures.pointerSmall`).
pub const TASK_BLOCKED_MARKER: &str = "›";

/// The inset the **idle** task block sits at — the count line and its rows
/// (`docs/tools/task-tools.md`). Two spaces, the same inset the footer and the
/// queued messages use, so the resting screen reads as one column; in a turn
/// the rows take [`TOOL_RESULT_PREFIX`]'s `⎿` gutter instead.
pub const TASK_IDLE_INDENT: &str = "  ";

/// The most checklist rows shown in the strip before the tail folds into a
/// dim `… +N pending` summary row (Claude Code caps at ten and prioritises
/// what is actionable — see `ui::tasks`).
pub const TASK_MAX_ROWS: usize = 10;

// ===== The spinner tip (docs/ui/tips.md) =====

/// The label opening the tip row, after the `⎿` gutter
/// ([`TOOL_RESULT_PREFIX`]) — Claude Code's `Tip: `.
pub const TIP_LABEL: &str = "Tip: ";

/// Dim grey — the whole tip row, gutter, label and text alike: Claude Code
/// dims it throughout, a hint beside the work rather than a line that
/// competes with it.
pub fn tip_color() -> Color {
    tool_dim_color()
}

/// Green — an added (`+`) line in an `edit`/`write` diff cell (codex's diff
/// look, adapted to the `⎿` gutter; see `docs/tools/tools.md`).
pub fn tool_diff_add_color() -> Color {
    tool_ok_color()
}

/// Red — a removed (`-`) line in an `edit`/`write` diff cell.
pub fn tool_diff_del_color() -> Color {
    tool_fail_color()
}

/// Dark-green background tint of an added row in a numbered `edit`/`write`
/// cell (codex's dark-theme add tint): the syntax-coloured text reads over it
/// and the row pads to the full width, like the user-message block.
pub fn tool_diff_add_bg() -> Color {
    palette().diff_add_bg
}

/// Dark-red background tint of a removed row (codex's dark-theme delete
/// tint); the removed text is additionally dimmed, codex-style.
pub fn tool_diff_del_bg() -> Color {
    palette().diff_del_bg
}

/// Background tint of the **characters that changed** on an added row — the
/// character-level refinement's own colour (`docs/ui/inline-diff.md`). A clearly
/// brighter green than the row's [`tool_diff_add_bg`], because the two are
/// read together: the muted row tint says *this line changed*, the bright mark
/// says *here*. Bright enough to find at a glance — which puts it at a middle
/// brightness, where a theme's dimmer syntax colours (a comment's grey above
/// all) can match it and vanish, so a changed run's ink is held to
/// [`INLINE_DIFF_MARK_MIN_CONTRAST`] against it.
pub fn tool_diff_add_mark_bg() -> Color {
    palette().diff_add_mark_bg
}

/// Background tint of the characters that changed on a removed row — the
/// [`tool_diff_add_mark_bg`] twin over [`tool_diff_del_bg`].
pub fn tool_diff_del_mark_bg() -> Color {
    palette().diff_del_mark_bg
}

/// The contrast every changed run's ink keeps against its mark, as a WCAG
/// ratio (`docs/ui/inline-diff.md`). A syntax colour that already reads keeps
/// its colour. One that doesn't is lifted toward [`tool_diff_mark_ink`]
/// until it does. In Dracula a doc comment's grey sat on the added-text mark
/// at 1.0:1, the two the same brightness, so the words the edit changed
/// disappeared into their own highlight. 3:1 is how Dracula and Monokai set
/// a comment against their own backgrounds, and more than One Dark, Nord or
/// Solarized give theirs, so it is a contrast these themes already treat as
/// readable. WCAG's 4.5:1 for body text is out of reach on a mark this
/// bright for any ink but near-white.
pub const INLINE_DIFF_MARK_MIN_CONTRAST: f32 = 3.0;

/// The ink a changed run is lifted toward when its syntax colour won't read
/// on its mark ([`INLINE_DIFF_MARK_MIN_CONTRAST`]): the theme's own text
/// colour, so a lifted run moves toward the ink the theme reads in.
pub fn tool_diff_mark_ink() -> Color {
    ai_color()
}

/// How much of a `-`/`+` line pair must be **common** for the character-level
/// refinement to run at all, as a percentage of the longer line's
/// non-whitespace display columns (`docs/ui/inline-diff.md`). Below it the two
/// lines are a replacement rather than an edit: there is no "what changed" to
/// point at, and marking most of the line is noisier than marking none of it,
/// so the row keeps its flat tint. Whitespace is excluded from the count so a
/// shared indent alone never reads as similarity.
pub const INLINE_DIFF_MIN_COMMON_PCT: usize = 30;

/// The most token-LCS cells one refined pair will fill. The common prefix and
/// suffix are trimmed first, so a typical edit's changed middle is a token or
/// two however long the line; past this bound (a minified bundle line) the
/// whole middle is marked changed instead — the trimmed ends are still real
/// common context, so the answer stays honest and the live cell keeps
/// re-rendering inside its 32 ms frame.
pub const INLINE_DIFF_MAX_CELLS: usize = 40_000;

/// The corner row of a `write` still being generated, before its first line
/// arrives — and, around the count, once lines do: `Writing 17 lines…`
/// (`docs/ui/write-streaming.md`). The count grows as the lines stream in; the
/// committed cell's `Wrote 17 lines to {path}` takes the row's place.
pub const TOOL_WRITING: &str = "Writing…";
pub const TOOL_WRITING_PREFIX: &str = "Writing ";
pub const TOOL_WRITING_SUFFIX: &str = "…";

/// The display-row budget — and ceiling — of a `read`/`write`/`edit` cell's
/// inline peek before the `… +N lines (ctrl+o to expand)` hint (Claude-Code's
/// ~10-row Write preview): the peek starts numbered lines while it has rows
/// of this budget left and shows each one whole when the rows left hold it;
/// the one line that needs more is cut on the last row with
/// [`FILE_PEEK_CUT_MARKER`], so no line, however long, carries the peek past
/// this many rows (`docs/ui/long-lines.md` §8). A streaming `write`'s tail is
/// exactly this many rows (`docs/ui/write-streaming.md`).
pub const FILE_PEEK_LINES: usize = 10;

/// The dim marker closing the last row the file peek keeps of a line it cuts
/// at [`FILE_PEEK_LINES`] — in the row's last column, on its tint — so a cut
/// line never reads as a line that ended (`docs/ui/long-lines.md` §8).
pub const FILE_PEEK_CUT_MARKER: &str = "…";

/// The model tools whose output is a file change — rendered as the numbered,
/// syntax-highlighted codex-style cell when the output is in the
/// `llm::tools` gutter format ([`file_cell_lines`]), or with the legacy
/// first-char `+`/`-` colouring when it isn't (old sessions, error bodies).
/// A `!` shell command is never one (its output is command output).
pub const DIFF_TOOL_NAMES: [&str; 2] = ["Edit", "Write"];
/// The model tools whose output is **command output** — a shell run, streamed
/// and framed with an `Exit code: N` line. They render like the `!` shell cell
/// (a multi-line `⎿` peek, the frame stripped for display) and **tail** their
/// output live while running (`docs/ui/tool-streaming.md`): `bash`, and the
/// companions that are the same command's later steps — `bashsend`,
/// `bashwait`, `bashkill` (`docs/tools/bash-tools.md`) — and the legacy
/// `bash_session`. `bashlist`'s list is no command output: a generic tool
/// keeps the single collapsed peek line. `agentoutput` joins them for the
/// same reason the companions do: a wait on it streams the agent's calls as
/// they start, and the cell tails them (`docs/agents/agent-tools.md`).
pub const COMMAND_TOOL_NAMES: [&str; 6] = [
    "Bash",
    "BashSend",
    "BashWait",
    "BashKill",
    "BashSession",
    "AgentOutput",
];

/// The dim closing row of a command cell whose session is still alive
/// (`docs/tools/interactive-shell.md`) — `{state} · session {id}`, in place of the
/// report's frame line, which is the model's: the program sits at a prompt,
/// sits at a password prompt, is still busy, or was ended by the call.
pub const SESSION_WAITING_ROW: &str = "Waiting for input";
/// See [`SESSION_WAITING_ROW`] — a prompt reading a line with echo off.
pub const SESSION_PASSWORD_ROW: &str = "Waiting for a password";
/// See [`SESSION_WAITING_ROW`].
pub const SESSION_RUNNING_ROW: &str = "Still running";
/// See [`SESSION_WAITING_ROW`].
pub const SESSION_STOPPED_ROW: &str = "Stopped";
/// The separator between a session row's state and its id.
pub const SESSION_ROW_ID: &str = " · session ";
