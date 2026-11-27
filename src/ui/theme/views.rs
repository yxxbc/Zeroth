//! Overlay, picker, and read-only page styling constants.

use super::palette::{bright, palette};
use super::wrap::lerp_color;
use super::*;

// --- Tool-output view (the Ctrl+O full-screen overlay) — codex's Ctrl+T
// transcript pager: a slash-tiled dim title row over a scrolling body (the
// full conversation transcript — every message plus every tool call's
// *complete* (expanded) output — with vi-style `~` filler rows past its end),
// closed by a `─` separator carrying the scroll percentage and two dim
// key-hint rows above a final blank row. ---

/// The pager's spaced-caps title, overlaid on the slash tiling as
/// `/ T R A N S C R I P T` (codex's transcript overlay header).
pub const TOOL_VIEW_TITLE: &str = "T R A N S C R I P T";

/// Rows of chrome above the scrolling body (the slash-tiled title row).
pub const TOOL_VIEW_TITLE_ROWS: u16 = 1;

/// Rows of chrome below the body: the `─` separator carrying the scroll
/// percentage, two key-hint rows, and the final blank row (codex's pager).
pub const TOOL_VIEW_FOOTER_ROWS: u16 = 4;

/// First key-hint row under the separator (codex's pager hints, all dim).
pub const TOOL_VIEW_HINT_KEYS: &str = " ↑/↓ to scroll   pgup/pgdn to page   home/end to jump";

/// Second key-hint row: every key that closes the overlay.
pub const TOOL_VIEW_HINT_QUIT: &str = " q/esc/ctrl+o to quit";

/// Second key-hint row when idle Esc would instead **begin** the backtrack
/// preview (`App::overlay_esc_backtracks` — `docs/input/backtrack.md`): Esc is not
/// a quit key in that state, and promising it was ("I pressed Esc to exit
/// and got edit-previous-message") is the reported surprise this hint fixes.
/// Same three-space entry separator as [`TOOL_VIEW_HINT_KEYS`].
pub const TOOL_VIEW_HINT_QUIT_EDIT: &str = " q/ctrl+o to quit   esc to edit prev";

/// Second key-hint row while a backtrack preview highlights a user message —
/// codex's highlighted-pager footer (`docs/input/backtrack.md`); it replaces
/// [`TOOL_VIEW_HINT_QUIT`], whose Esc meaning the preview takes over.
pub const TOOL_VIEW_HINT_BACKTRACK: &str =
    " esc/← to edit prev   → to edit next   enter to edit message   q to cancel";

/// The vi-style filler marking body rows below the transcript's end.
pub const TOOL_VIEW_FILL: &str = "~";

/// The dim placeholder shown when the transcript has nothing to list yet.
pub const TOOL_VIEW_EMPTY: &str = "Nothing here yet.";

// --- Ctrl+D context-debug view (the third alternate-screen overlay) — the
// raw LLM context window (docs/architecture/context.md): the transcript pager's chrome
// (slash-tiled title, `~` filler, percentage separator, dim key hints) over
// a body listing exactly what the model is sent — the system prompt, then
// every derived context message, each opening on a lit role chip over its
// text hung on the tool cell's `⎿` gutter, the text **verbatim** (image
// placeholders and bracketed tool/shell/notice formats unrendered) with the
// attachment paths dim beneath. ---

/// The view's spaced-caps title, overlaid on the slash tiling.
pub const CONTEXT_VIEW_TITLE: &str = "C O N T E X T";

/// Inside an agent session view the title goes on to name whose window it
/// is — `C O N T E X T · explore agent · Fetch the weather` — the type
/// before [`CONTEXT_VIEW_AGENT_SUFFIX`], then the task, the roster's own
/// naming. Nothing else on the page tells the lead's window from an agent's.
pub const CONTEXT_VIEW_AGENT_SEPARATOR: &str = " · ";

/// See [`CONTEXT_VIEW_AGENT_SEPARATOR`].
pub const CONTEXT_VIEW_AGENT_SUFFIX: &str = " agent";

/// First key-hint row — the pager's ([`TOOL_VIEW_HINT_KEYS`]) with the
/// message step added, since a window of a few dozen entries is thousands
/// of rows and ←/→ open the previous or next one at the top.
pub const CONTEXT_VIEW_HINT_KEYS: &str =
    " ↑/↓ to scroll   ←/→ prev/next message   pgup/pgdn to page   home/end to jump";

/// Second key-hint row: every key that closes the view.
pub const CONTEXT_VIEW_HINT_QUIT: &str = " q/esc/ctrl+d to quit";

/// The dim placeholder when the context window is empty (no system prompt —
/// the dummy sends none — and nothing said yet).
pub const CONTEXT_VIEW_EMPTY: &str = "Context is empty — send a message to fill it.";

/// Each entry's role chip label, by wire role — upper-case, so the row the
/// eye hunts for reads as a heading and never as a word of the text under
/// it (the lower-case `user:` tags it replaced did).
pub const CONTEXT_ROLE_SYSTEM: &str = "SYSTEM";
pub const CONTEXT_ROLE_USER: &str = "USER";
pub const CONTEXT_ROLE_ASSISTANT: &str = "ASSISTANT";
pub const CONTEXT_ROLE_TOOL: &str = "TOOL";

/// The padding inside a role chip's fill, either side of its label — the
/// agent label's own, so the chip reads as a tab rather than a smudge.
pub const CONTEXT_CHIP_PAD: &str = " ";

/// Between a role chip and the dim facts beside it — one bare cell, the
/// chip's own padding already standing inside its fill.
pub const CONTEXT_HEADER_GAP: &str = " ";

/// Between the facts of an entry header (`#4 · read · ≈120 tokens`) and of
/// the bottom rule's summary.
pub const CONTEXT_DETAIL_SEPARATOR: &str = " · ";

/// The mark before an entry's position in the request.
pub const CONTEXT_INDEX_MARK: &str = "#";

/// What the gutter shows for an entry with no text, no calls and no
/// attachments — an empty tool result still went over the wire, and a
/// header over nothing would read as a rendering bug.
pub const CONTEXT_EMPTY_BODY: &str = "(empty)";

/// The fewest columns an entry's text keeps beside the [`TOOL_RESULT_PREFIX`]
/// gutter — room for one wide glyph. A terminal narrower than the gutter
/// plus this gives the gutter's columns back to the text.
pub const CONTEXT_BODY_MIN_COLS: usize = 2;

/// The fewest body rows that may give their top one up to a **pinned
/// header** — the entry the window opens inside, named on its top row while
/// its own header is scrolled off (`docs/architecture/context.md`). Fewer, and the pin
/// would cost the very text it names.
pub const CONTEXT_PIN_MIN_ROWS: usize = 3;

// --- The Ctrl+D view's classifier page (`docs/state/permissions.md`) — Tab's
// other half: the same pager chrome over the bounded task context auto
// mode's classifier reads before every command and MCP call. ---

/// The view's spaced-caps title, overlaid on the slash tiling.
pub const CLASSIFIER_VIEW_TITLE: &str = "C L A S S I F I E R";

/// The page-flip hint appended to the second key-hint row, naming the page
/// Tab would show — the view's only discovery affordance for its other half.
pub const CONTEXT_VIEW_HINT_TAB_CLASSIFIER: &str = "   tab for classifier context";

/// …and the way back.
pub const CONTEXT_VIEW_HINT_TAB_LLM: &str = "   tab for llm context";

/// The dim placeholder before anything is recorded (a fresh session, or a
/// backend that keeps no log — the dummy).
pub const CLASSIFIER_VIEW_EMPTY: &str = "No classifier context yet — send a message to fill it.";

/// The dim note above the block in **auto** mode: the log is live, and this
/// is what the next verdict reads.
pub const CLASSIFIER_VIEW_NOTE_AUTO: &str =
    "Auto mode — the classifier reads this before each command.";

/// The same note in every other mode. The log is recorded in all of them (the
/// boundary feeds it per call, not per verdict), so a view that stayed silent
/// here would read as "the classifier is deciding this" when it is not.
pub const CLASSIFIER_VIEW_NOTE_INACTIVE: &str =
    "Recorded every turn; consulted only in auto mode (shift+tab to switch).";

/// …and with the gate off entirely (`ZEROTH_PERMISSIONS=0`), where no
/// verdict is ever asked for.
pub const CLASSIFIER_VIEW_NOTE_OFF: &str = "Tool permissions are disabled — no classifier runs.";

/// The display-column budget for each section's opening paragraph in the
/// page's **abridged system prompt** (`ui::classifier_view::abridge_prompt`)
/// — three rows of an 80-column terminal, enough to read what a section is
/// about. The overflowing line is cut and closed with `…`; the section's
/// remaining lines fold into the counted row under it. Shown whole, the
/// 3.6 KB rubric would push the live block the page exists for sixty rows
/// down (`docs/state/permissions.md`).
pub const CLASSIFIER_PROMPT_PEEK_COLS: usize = 240;

/// The lead of the dim `… +N lines` row that counts a section's folded
/// lines — the tool cell's [`TOOL_MORE_PREFIX`] idiom at the page's own
/// indent and without its `(ctrl+o to expand)`, there being nothing to
/// expand it into.
pub const CLASSIFIER_PROMPT_MORE_PREFIX: &str = "… ";

/// The classifier page's system prompt header fact: the page shows the
/// rubric's structure, not all of it, and the header says so where the
/// sibling page's says what an entry costs.
pub const CLASSIFIER_PROMPT_ABRIDGED: &str = "abridged";

/// The classifier page's header facts naming what each entry holds — the
/// rubric every verdict is judged by, and the bounded task context it reads.
pub const CLASSIFIER_PROMPT_META: &str = "classifier prompt";
pub const CLASSIFIER_CONTEXT_META: &str = "task context";

/// The dim placeholder under the `## Action to review` header that closes
/// the page's `user:` message: the request's real shape ends on the one
/// action being judged, which is only known when a verdict is asked.
pub const CLASSIFIER_ACTION_PLACEHOLDER: &str =
    "(the command being judged — filled in when a verdict is asked)";

/// The label of an attachment row under a user entry's text.
pub const CONTEXT_IMAGE_LABEL: &str = "image: ";

/// The prefix of a native tool-call row under an assistant entry —
/// `→ name(arguments)`, the model's request in the raw wire form.
pub const CONTEXT_TOOL_CALL_PREFIX: &str = "→ ";

/// Between a tool call's closing `)` and its dim id — the id its `TOOL`
/// entry names too, so a parallel batch's results pair with their calls.
pub const CONTEXT_CALL_ID_GAP: &str = "  ";

/// A role chip's ink — `on_accent`, the pair the agent session view's label
/// and the ask countdown already stand on ([`agent_view_label_fg`]), over
/// every role's fill: near-black on a dark theme's hues, Latte's pale `base`
/// on its darker ones, black on ANSI's named colours.
pub fn context_chip_fg() -> Color {
    palette().on_accent
}

/// The ink of an entry's text, its call arguments included — the reply's
/// own ([`ai_color`]), so the raw window reads like the conversation it was
/// derived from rather than in the terminal's default foreground.
pub fn context_text_color() -> Color {
    ai_color()
}

/// Role-chip fills — the tool palette's hues (user blue, assistant green,
/// system amber, tool-result purple) so the roles scan apart at a glance. The
/// blue was the running tool's until that went grey ([`tool_running_color`]);
/// a role chip is not a running state, so it keeps the hue as its own value.
pub fn context_user_color() -> Color {
    palette().link
}

pub fn context_assistant_color() -> Color {
    tool_ok_color()
}

pub fn context_system_color() -> Color {
    palette().warning
}

/// The TOOL result chip's fill and the `→ name` head of a call row under an
/// assistant entry — a distinct purple so the native tool round-trip reads
/// apart from plain assistant text.
pub fn context_tool_color() -> Color {
    palette().purple
}

// --- /resume session picker (the other alternate-screen overlay) — codex's
// resume picker, sized down (docs/state/resume.md): the same slash-tiled title
// chrome as the transcript pager, a type-to-search line, dense one-line
// session rows (`❯ {age:12}{preview}`, the palette's selection-by-colour),
// and a bottom rule carrying `{selected+1}/{total}` over a dim key-hint row. ---

/// The picker's spaced-caps title, overlaid on the slash tiling.
pub const RESUME_TITLE: &str = "R E S U M E";

/// The dim search-line placeholder while the query is empty (codex's).
pub const RESUME_SEARCH_PLACEHOLDER: &str = "Type to search";

/// The search line's prefix once a query is typed.
pub const RESUME_SEARCH_PROMPT: &str = "Search: ";

/// The picker's key-hint row (dim, under the separator). The search line's
/// own placeholder carries the type-to-search hint.
pub const RESUME_HINTS: &str = " ↑/↓ select   enter resume   esc cancel   tab + ←/→ filter/sort";

/// The dim list placeholder when nothing was ever saved (codex's).
pub const RESUME_NO_SESSIONS: &str = "No sessions yet";

/// The dim list placeholder when the query matches nothing (codex's).
pub const RESUME_NO_MATCH: &str = "No results for your search";

/// The age column's width in the dense rows — codex's 12-col relative date.
pub const RESUME_AGE_WIDTH: usize = 12;

/// The two-space inset shared by the search line and the placeholder rows
/// (the row marker is the same width, so everything lines up).
pub const RESUME_INDENT: &str = "  ";

/// The selected row's marker; unselected rows get spaces (codex's `❯ `).
pub const RESUME_MARKER: &str = "❯ ";

/// The selected row's full-width background tint — codex blends white over
/// the terminal background; a grey lift noticeably lighter than the
/// user-message block plays that role here.
pub fn resume_selected_bg() -> Color {
    palette().selection_bg
}

/// The Tab-focused toolbar control's active value — codex's magenta.
pub fn resume_focus_color() -> Color {
    palette().purple
}

/// The gap between the toolbar's Filter and Sort tab pairs (codex's).
pub const RESUME_TOOLBAR_GAP: &str = "   ";

/// The smallest gap kept between the search text and the toolbar before the
/// toolbar compacts (and then drops).
pub const RESUME_TOOLBAR_MIN_GAP: usize = 2;

// --- Inline `/model` picker (docs/providers/llm.md). Unlike `/resume`, this one is
// **inline** — it replaces the composer in the bottom live region with its own
// `❯` search prompt over a scrolling model list, framed by top/bottom rules
// like the input box. The picker family's page (docs/ui/view-flow.md *A
// picker's page*): a title over a dim subtitle, the search line, the rows
// with their `→` under the `❯`, the palette's cyan selection accent, a dim
// `[provider]` tag, a green ✓ on the active model, then a `(n/total)`
// counter and a `Model Name:` line hanging under the ids — the shape of the
// user's mock. ---

/// The two-space inset shared by every picker row (title, search, list rows,
/// hint) so the content sits off the frame's left edge.
pub const MODEL_INDENT: &str = "  ";

/// The column a list's **detail** hangs at — past [`MODEL_INDENT`] and the
/// [`MODEL_MARKER`]'s width, so the `(n/total)` counter, the highlighted
/// row's description and `/model`'s `Model Name:` line line up under the
/// row names they speak for, while the title, the search line and the hint
/// keep the frame's own inset.
pub const MODEL_DETAIL_INDENT: &str = "    ";

/// The `/model` page's title, in the pages' title dress
/// (`header::page_title_line`).
pub const MODEL_TITLE: &str = "Select model";

/// The dim line under the title: what a pick changes. A switch moves this
/// session's model and the directory's own entry, which a new session here
/// starts on (`docs/state/per-directory-state.md`, `docs/state/session-model.md`).
pub const MODEL_SUBTITLE: &str = "Applies to this session and new ones in this directory";

/// The glyph a framed view's top and bottom rules repeat across the width
/// (`model_rule`). Named so the hidden-cursor seat can tell a rule row from
/// text: a page with no `❯` seats after its last text row and skips the
/// rule under it (`layout::menu_marker_seat`, `docs/ui/view-flow.md`).
pub const VIEW_RULE: &str = "─";

/// The search line's prompt glyph (cyan), the `❯` the query types after.
pub const MODEL_PROMPT: &str = "❯ ";

/// Cyan — the `❯` prompt and the selected row (the palette-selection accent).
pub fn model_selected_color() -> Color {
    menu_selected_color()
}

/// The selected row's marker in every picker's list — `/model`, `/login`,
/// `/settings`, `/skills`, `/theme`, `/mascot`, `/spinner` — at the frame's
/// inset, so it sits straight under the search line's `❯`, which stays the
/// only `❯` on the page. Unselected rows get spaces the same width
/// (`model_view::marked_row_head`).
pub const MODEL_MARKER: &str = "→ ";

/// Light grey — an unselected model id (readable but quieter than the selection).
pub fn model_id_color() -> Color {
    palette().text_muted
}

/// Dim — the `[provider]` tag, the counter, and the `Model Name:` line.
pub fn model_meta_color() -> Color {
    tool_dim_color()
}

/// Green — the ✓ marking the currently-active model (shares the tool-ok green).
pub fn model_active_color() -> Color {
    tool_ok_color()
}

/// The columns a model row's brand mark and its trailing space spend before
/// the id — one glyph from [`crate::brand::Brand::glyph`] plus the gap, so
/// the id column stays aligned whether or not a row has a mark.
pub const BRAND_MARK_COLS: usize = 2;

/// The mark appended to the active model's row.
pub const MODEL_ACTIVE_MARK: &str = " ✓";

/// The label opening the friendly-name line under the list.
pub const MODEL_NAME_LABEL: &str = "Model Name: ";

/// The most model rows shown at once; longer lists scroll to keep the selection
/// **centered** (`centered_window`) so the user sees the models above and below
/// it, not just up to the edge it last crossed. Seven: enough of a long
/// catalog to read the neighbourhood of a search, few enough that the page
/// leaves the conversation above it most of the terminal.
pub const MODEL_MENU_MAX_ROWS: u16 = 7;

/// The list placeholder while the fetch is in flight.
pub const MODEL_LOADING: &str = "Loading models…";

/// The list placeholder when the provider returned no models.
pub const MODEL_NONE: &str = "No models available";

/// The list placeholder when the query matches nothing.
pub const MODEL_NO_MATCH: &str = "No matching models";

/// The list placeholder when no provider is set up yet ([`ModelLoad::NeedsLogin`])
/// — shown cyan (an actionable hint, not a red error) pointing at `/login`.
/// It names no API key: `/login` also signs in to a subscription or connects
/// a local server, which has none (`docs/providers/local-models.md`).
pub const MODEL_LOGIN_HINT: &str = "No provider set up yet — run /login to add one";

/// The separator between the `(n/total)` counter and its trailing load status.
pub const MODEL_STATUS_SEP: &str = "   ·   ";

/// The counter's dim suffix while other providers are still being fetched (the
/// list shows what's landed so far and keeps growing). See `docs/providers/llm.md`.
pub const MODEL_LOADING_MORE: &str = "loading more…";

/// The most display rows a failed provider's *reason* block may take under the
/// list. The collapsed `{provider} unavailable` note names the provider and
/// nothing else, so the reason is rendered beneath it — but a provider that
/// answers with an HTML page (an intercepting proxy, a captive portal) would
/// otherwise push the model list off the top of the frame.
pub const MODEL_ERROR_MAX_ROWS: u16 = 3;

// --- The `/login` onboarding flow (docs/providers/llm.md). An inline flow sharing the
// model picker's framed look and colours: the root asks how you sign in under
// a title (no filter — three rows are a question), the lists below it filter
// under a `❯` line, and the key step collects the key masked. All styling
// reuses the `MODEL_*` consts (indent, `❯` prompt, cyan selection, dim meta,
// green ✓, `→` marker) plus the `LOGIN_*` strings/geometry below. ---

/// The separator between a `/login` row's name and its configured status.
pub const LOGIN_STATUS_SEP: &str = " · ";

/// What a provider / subscription row says about itself: whether this session
/// can actually reach it. Both states are **spelled out**, where a bare row
/// used to mean "no key yet" — a fact the reader could only take from the
/// *absence* of a mark two columns further right, which is a poor way to
/// answer the one question a sign-in list is opened to answer.
///
/// Split into a **mark** and a **label** because they are coloured
/// differently. The `✔` keeps the green the `/model` picker's ✓ wears
/// ([`model_active_color`]) — it is the thing worth finding down a list of
/// names — while its word, the `◯`, and the separator before them stay dim
/// ([`model_meta_color`]): a status is a fact about a row rather than an
/// alert, and colouring the whole tail made a list of facts read as a column
/// of them.
pub const LOGIN_CONFIGURED_MARK: &str = "✔";
pub const LOGIN_CONFIGURED_LABEL: &str = " configured";
pub const LOGIN_UNCONFIGURED_MARK: &str = "◯";
pub const LOGIN_UNCONFIGURED_LABEL: &str = " unconfigured";

/// The method step's title — the root's question, over its three ways in.
/// "Login" is the command's own word and the sign-in method choice's
/// (`Select ChatGPT Codex login method:`), so the two questions read alike.
pub const LOGIN_METHOD_TITLE: &str = "Select login method";

/// The dim hint under the method step (the root: Esc closes).
pub const LOGIN_METHOD_HINT: &str = "↑↓ navigate  enter select  escape/ctrl+c cancel";

/// The dim hint under the subscription list.
pub const LOGIN_SUBSCRIPTION_HINT: &str = "↑↓ navigate  enter sign in  esc back";

/// The dim hint under the API-key provider list, below the `.env` path row.
pub const LOGIN_PROVIDER_HINT: &str = "↑↓ navigate  enter select  esc back";

/// The dim hint under the local server list (`docs/providers/local-models.md`).
pub const LOGIN_LOCAL_HINT: &str = "↑↓ navigate  enter connect  esc back";
pub const LOGIN_LOGOUT_HINT: &str = "↑↓ navigate  enter select  esc back";
pub const LOGIN_LOGOUT_CONFIRM_HINT: &str = "Removes local credentials only · Esc to go back";
pub const LOGIN_NO_CONNECTIONS: &str = "No connected accounts or API keys";

/// The list placeholder when the local server filter matches nothing.
pub const LOGIN_NO_LOCAL_MATCH: &str = "No matching servers";

/// The list placeholder when the subscription filter matches nothing.
pub const LOGIN_NO_SUBSCRIPTION_MATCH: &str = "No matching subscriptions";

// --- The sign-in method choice (docs/providers/chatgpt.md). A subscription offering two
// ways in — ChatGPT Codex's browser flow, or a device code for a headless
// machine — asks which before opening a page: a cyan title naming the
// subscription over two rows in the root's own dress, and no filter, since a
// question with two answers is not searched. ---

/// The choice's title, around the subscription's name:
/// `Select ChatGPT Codex login method:`.
pub const LOGIN_SIGNIN_METHOD_TITLE_PREFIX: &str = "Select ";
pub const LOGIN_SIGNIN_METHOD_TITLE_SUFFIX: &str = " login method:";

/// The dim hint under the choice — the root's own words: Esc steps back to
/// the subscription list here, which is what cancelling a choice means.
pub const LOGIN_SIGNIN_METHOD_HINT: &str = LOGIN_METHOD_HINT;

// --- The device-code page (docs/providers/copilot.md). A subscription sign-in shows the
// provider's one-time code in a rounded box over the URL to enter it at, and
// waits. No browser is launched — the URL is text the user opens themselves. ---

/// The device page's title prefix — `Sign in to {provider}`.
pub const DEVICE_TITLE_PREFIX: &str = "Sign in to ";

/// The device page's instruction, in two rows: `Visit {uri}` then this.
pub const DEVICE_VISIT_PREFIX: &str = "Visit ";
pub const DEVICE_ENTER_LINE: &str = "and enter this one-time code";

/// A **browser** sign-in's twin of the pair above (`docs/providers/chatgpt.md`): there
/// is no code to follow the link, because the browser redirects back to this
/// process on its own. The URL takes the row **bare** — it is an OSC 8
/// hyperlink (`docs/ui/links.md`), and a verb in front of a clickable link is a
/// word doing nothing but pushing the target off the start of its own row —
/// so only the row beneath it is needed, saying what happens next instead of
/// pointing at a box that isn't there.
pub const DEVICE_RETURN_LINE: &str = "Sign in there — this window continues by itself";

/// The code box's extra indent past [`MODEL_INDENT`], and its rounded corners.
pub const DEVICE_BOX_INDENT: &str = "   ";
pub const DEVICE_BOX_TOP_LEFT: &str = "╭";
pub const DEVICE_BOX_TOP_RIGHT: &str = "╮";
pub const DEVICE_BOX_BOTTOM_LEFT: &str = "╰";
pub const DEVICE_BOX_BOTTOM_RIGHT: &str = "╯";
pub const DEVICE_BOX_HORIZONTAL: &str = "─";
pub const DEVICE_BOX_VERTICAL: &str = "│";
/// The padding inside the box, each side of the code.
pub const DEVICE_BOX_PAD: &str = "  ";

/// The code itself — bright and bold, the one thing on the page to transcribe.
pub fn device_code_color() -> Color {
    ai_color()
}

/// The verification URL — **dim**, like the sentence under it. The one thing
/// on this page the eye should land on is the code in its box; an accented URL
/// competed with it, and the URL is an instruction rather than a choice. It is
/// still a clickable hyperlink; the underline is what says so, which is why
/// linking it does not repaint it (`docs/ui/links.md`).
pub fn device_uri_color() -> Color {
    model_meta_color()
}

/// The status line's two states, and the countdown clause appended to the wait.
pub const DEVICE_STARTING: &str = "Requesting a code…";
pub const DEVICE_WAITING: &str = "Waiting for approval…";
pub const DEVICE_EXPIRES_PREFIX: &str = " · expires in ";
pub const DEVICE_EXPIRED: &str = " · code expired";

/// A browser sign-in's twin of the two states above: nothing is requested
/// from a provider first (the link is built locally), and what is waited on
/// is the browser coming back, not a code being approved.
pub const DEVICE_LINK_STARTING: &str = "Opening the sign-in…";
pub const DEVICE_LINK_WAITING: &str = "Waiting for the browser…";
pub const DEVICE_LINK_EXPIRED: &str = " · timed out";

/// The dim hint under the sign-in page — the code page's, and the browser
/// page's, which copies the link instead.
pub const DEVICE_HINT: &str = "c copy code  esc cancel";
pub const DEVICE_LINK_HINT: &str = "c copy link  esc cancel";

/// Where the device page's **hidden** cursor parks — the frame's first content
/// row, the title, straight under the top rule (`docs/ui/view-flow.md`).
///
/// The caret is invisible here ([`cursor_visible`]), but a terminal with a
/// cursor-trail animation (kitty and kin) still animates toward wherever it is
/// *seated*, and this page re-arms a frame every 32 ms to tick its countdown.
/// So the seat must be somewhere harmless and somewhere **still**: the code
/// box is neither — anything the emulator paints at the cursor lands on the
/// one thing the page exists to be read from, and the box's row moves when the
/// page grows from the waiting shape to the full one.
pub const DEVICE_CURSOR_ROW: u16 = 1;

/// The dim hint under the provider list, prefixing the real `.env` path
/// (`onboarding.env_path`) so it names where the key actually lands.
pub const LOGIN_PROVIDER_HINT_PREFIX: &str = "Keys are saved to ";

/// The dim hint under the key-entry field.
pub const LOGIN_KEY_HINT: &str = "Enter to save · Esc to go back";

/// What the key step's link is introduced by: the page a **key** is created
/// on.
pub const LOGIN_KEY_URL_PREFIX: &str = "Create a key at ";

// --- The local server page (docs/providers/local-models.md): `/login` → Use a local
// model → a server. The key step's frame around an address field shown as
// typed, a connection test whose verdict lands under the field, and — for a
// server that wants one — a masked token field. ---

/// What a local server's link is introduced by: there is no key to create,
/// so it points at where the server itself comes from.
pub const LOGIN_INSTALL_URL_PREFIX: &str = "Install it from ";

/// The hint under an address or token field: Enter tests the connection,
/// and saves only what passes.
pub const LOGIN_SERVER_HINT: &str = "Enter to test and save · Esc to go back";

/// The hint under an address the provider block fixes — there is nothing to
/// save, only the connection to test.
pub const LOGIN_FIXED_SERVER_HINT: &str = "Enter to test · Esc to go back";

/// The hint while a connection test runs: the page waits, Esc stops it.
pub const LOGIN_TESTING_HINT: &str = "Esc to stop the test";

/// The dim row under the field while a connection test runs.
pub const LOGIN_TESTING: &str = "Testing the connection…";

/// The dim placeholder in an empty token field.
pub const LOGIN_TOKEN_PLACEHOLDER: &str = "paste the token, then press Enter";

/// The dim placeholder shown in the key field before anything is entered.
pub const LOGIN_KEY_PLACEHOLDER: &str = "paste your API key, then press Enter";

/// The glyph each entered key character is masked to.
pub const LOGIN_MASK_CHAR: char = '•';

/// The list placeholder when the provider filter matches nothing.
pub const LOGIN_NO_MATCH: &str = "No matching providers";

/// The most provider rows shown at once (longer lists scroll to keep the
/// selection **centered**, like the `/model` list — `centered_window`).
pub const LOGIN_MENU_MAX_ROWS: u16 = 8;

// --- The inline `/settings` menu (docs/state/settings.md). The `/model` picker's
// framed shape with one extra row — the key hint under the description — and a
// value column instead of a `[provider]` tag. Every colour is borrowed from the
// `MODEL_*` set so the three inline pickers read as one family. ---

/// The `/settings` page's title, in the pages' title dress.
pub const SETTINGS_TITLE: &str = "Settings";

/// The dim line under the title: what a change does. Every row takes effect
/// at once and is saved — most per directory, the tips and the update check
/// per user (`docs/state/per-directory-state.md`) — so the line says what
/// holds for all of them.
pub const SETTINGS_SUBTITLE: &str = "Changes take effect at once and persist across sessions";

/// The most setting rows shown at once; a longer (or unfiltered) list scrolls
/// to keep the selection **centered**, like the `/model` list. Five: the
/// counter under them says how many more there are, and a search finds any
/// of them by its label or its description.
pub const SETTINGS_MENU_MAX_ROWS: u16 = 5;

/// The list rows a **compact** page shows — the page a picker builds instead
/// of its whole one on a terminal too short for that. codex's minimum result
/// rows (`selection_picker_layout.rs`): what is optional yields first — a
/// description, a preview, the counter, then a header — so the search line,
/// these rows and the hint stay on screen. One size for the whole family, so
/// every short page reads the same (`docs/ui/view-flow.md`).
pub const COMPACT_LIST_ROWS: u16 = 3;

/// Columns between the longest visible label and the value column, so the
/// values line up in a block (the palette's `MENU_DESC_COL` idea, sized to the
/// visible rows rather than pinned).
pub const SETTINGS_VALUE_GAP: usize = 3;

/// Light grey — a value that is *on* (`true`, a retry count, a temperature):
/// readable, and the same weight an unselected model id carries.
pub fn settings_value_color() -> Color {
    model_id_color()
}

/// Dim — a value that is *off* (`false`, `default`) or unavailable, so a
/// glance down the column shows what is actually doing something.
pub fn settings_value_off_color() -> Color {
    model_meta_color()
}

/// The values rendered in the dim "off" colour — `never` being the **Ask
/// timeout** row's (no clock runs, `docs/tools/ask.md`).
pub const SETTINGS_OFF_VALUES: &[&str] = &[
    "false",
    "default",
    "0",
    "disabled",
    crate::settings::ASK_TIMEOUT_NEVER_LABEL,
];

/// The key hint pinned under the description — the menu's whole grammar.
pub const SETTINGS_HINT: &str = "Type to search · Enter/Space to change · Esc to cancel";

/// The list placeholder when the search matches no setting.
pub const SETTINGS_NO_MATCH: &str = "No matching settings";

// --- the inline /skills menu (docs/tools/skills.md). Deliberately the /settings
// menu's twin rather than a new shape: it reuses that menu's frame, its
// value column geometry (SETTINGS_VALUE_GAP) and its two-tone value colours
// wholesale, so the only consts it needs of its own are the words. ---

/// The page's title, in the pages' one title dress
/// (`header::page_title_line`) — the headline `/hooks` and `/mcp` open on.
pub const SKILLS_TITLE: &str = "Skills";

/// The value column's two states. `disabled` is already in
/// [`SETTINGS_OFF_VALUES`], so it dims through the same rule that dims a
/// `false` knob — one look down the column shows what is live.
pub const SKILLS_ON_VALUE: &str = "enabled";
pub const SKILLS_OFF_VALUE: &str = "disabled";

/// The mark leading each value — `✔ enabled`, `◯ disabled` — the `/login`
/// rows' own pair, so the two lists that answer "is this on?" answer it with
/// one glyph. The check alone is green ([`model_active_color`]): it is what
/// the eye hunts for down a column of names, while the circle stays dim.
pub const SKILLS_ON_MARK: &str = LOGIN_CONFIGURED_MARK;
pub const SKILLS_OFF_MARK: &str = LOGIN_UNCONFIGURED_MARK;

/// The key hint pinned under the description — the menu's whole grammar
/// (Enter toggles too, the `/settings` menu's twin key).
pub const SKILLS_HINT: &str = "Type to search · Space to toggle · Esc to cancel";

/// The list placeholder when the search matches no skill.
pub const SKILLS_NO_MATCH: &str = "No matching skills";

/// The list placeholder when **nothing was discovered** — an empty list's only
/// real question is "where should I put one?", so the answer is the row.
pub const SKILLS_NONE_FOUND: &str = "No skills found. Add one at:";

/// The note under the search line when skills are off for the whole session
/// (the `/settings` **Skills** row): the rows still browse and toggle, but
/// nothing here reaches the model until that row goes back on. The `/hooks`
/// menu's disabled-note rule.
pub const SKILLS_SESSION_OFF: &str = "Skills are off for this session — turn them on in /settings";

// --- the inline /mascot picker (docs/ui/mascot.md). The /theme picker's
// layout over the mascot catalog: the family's header, frame and `❯` search
// line, the rows with the family's `→`, and the live banner preview, drawn by
// the header's own builder so it can never disagree with the real thing,
// laid BESIDE the list once the terminal has the room and stacked under it
// when it doesn't. ---

/// The page's title, in the pages' title dress.
pub const MASCOT_TITLE: &str = "Select mascot";

/// The dim line under the title: what a mascot is and where the choice is
/// kept (`docs/state/per-directory-state.md`).
pub const MASCOT_SUBTITLE: &str = "The startup banner's art — saved for this directory";

/// The key hint pinned under the list and its preview — the picker's whole
/// grammar, led by the navigation like the `/theme` picker's.
pub const MASCOT_HINT: &str = THEME_HINT;

/// The list placeholder when the search matches no mascot.
pub const MASCOT_NO_MATCH: &str = "No matching mascots";

/// The cap on the picker's visible list rows — the settings window's size,
/// so the six-mascot catalog windows by one row, which the counter under it
/// names.
pub const MASCOT_MENU_MAX_ROWS: u16 = SETTINGS_MENU_MAX_ROWS;

/// Columns between the list column and the preview beside it — the
/// `/theme` picker's gap.
pub const MASCOT_PREVIEW_GAP: usize = THEME_PREVIEW_GAP;

/// The narrowest preview column the side-by-side page lays out: the banner
/// keeps its art beside its title and its `/ commands   @ files   ? shortcuts`
/// hint (36
/// columns) and every caption reads whole (37), with a little room left for
/// the cwd row, which the banner `…`-cuts as it always has. A terminal with
/// less than the list, the gap and this stacks the preview under the list.
pub const MASCOT_PREVIEW_MIN_COLS: usize = 40;

// --- the inline /spinner picker (docs/ui/spinner.md). The /theme picker's
// layout over the spinner-style catalog: the family's header, frame and `❯`
// search line, the rows with the family's `→` and a name column that leaves
// room for each row's live spinner, and the live status-line preview laid
// BESIDE the list once the terminal has the room and stacked under it when
// it doesn't. The styles' own frames and colours sit with the status
// indicator's consts below (`SPINNER_*_FRAMES`). ---

/// The page's title, in the pages' title dress.
pub const SPINNER_TITLE: &str = "Select spinner";

/// The dim line under the title: what a spinner is and where the choice is
/// kept (`docs/state/per-directory-state.md`).
pub const SPINNER_SUBTITLE: &str = "The status line's animation — saved for this directory";

/// The key hint pinned under the list and its preview — the picker's whole
/// grammar, led by the navigation like the `/theme` picker's.
pub const SPINNER_HINT: &str = THEME_HINT;

/// The list placeholder when the search matches no style.
pub const SPINNER_NO_MATCH: &str = "No matching spinners";

/// The cap on the picker's visible list rows — the settings window's size,
/// so the nine-style catalog windows around the highlighted style, the
/// counter under it naming where it stands.
pub const SPINNER_MENU_MAX_ROWS: u16 = SETTINGS_MENU_MAX_ROWS;

/// Columns between the widest style name in the catalog and the spinner
/// column, so the live spinners line up down the list.
pub const SPINNER_MENU_GAP: usize = 3;

/// Columns between the list column and the preview beside it — the
/// `/theme` picker's gap.
pub const SPINNER_PREVIEW_GAP: usize = THEME_PREVIEW_GAP;

/// The narrowest preview column the side-by-side page lays out: the sample
/// status line reads whole with a minute on its clock (`Working… (1m 5s ·
/// esc to interrupt)` behind the eight-cell track), the `/theme` sample's
/// width. A terminal with less than the list, the gap and this stacks the
/// preview under the list.
pub const SPINNER_PREVIEW_MIN_COLS: usize = THEME_PREVIEW_MIN_COLS;

// --- The inline `/theme` picker (docs/ui/theme.md). codex's theme picker in
// this TUI's dress: the family's header, frame and `❯` search line over one
// row per colour theme (the family's `→` on the selection, a `(n/total)`
// counter under the window), each row wearing a swatch of its own accents,
// and a preview built from REAL cells (a user bubble, an `Edit` diff cell, an
// assistant reply with a code block) rendered under the highlighted theme
// through the conversation's own builders — laid BESIDE the list once the
// terminal has the room, stacked under it when it doesn't. The palettes
// themselves live in `ui::palette`; only the picker's words and geometry are
// here. ---

/// The page's title, in the pages' title dress.
pub const THEME_TITLE: &str = "Select theme";

/// The dim line under the title: what a theme changes and where the choice
/// is kept — one `theme.json` per user, where `/mascot` and `/spinner` keep
/// theirs per directory (`docs/ui/theme.md`).
pub const THEME_SUBTITLE: &str = "The interface's colours — applies in every directory";

/// The key hint pinned under the list and its preview — the picker's whole
/// grammar, led by the navigation the `/mcp` list's hint leads with.
pub const THEME_HINT: &str = "↑/↓ to navigate · Type to search · Enter to choose · Esc to cancel";

/// The list placeholder when the search matches no theme.
pub const THEME_NO_MATCH: &str = "No matching themes";

/// The cap on the picker's visible list rows. Ten, where the other previewed
/// pickers show five: the eleven-theme catalog windows by one row, which the
/// counter under it names. The ten rows and the counter stand about as tall
/// as the preview beside them, so the side-by-side page is no taller for
/// showing nearly the whole catalog at once.
pub const THEME_MENU_MAX_ROWS: u16 = 10;

/// Columns between the widest theme name in the catalog and the swatch
/// column, so the swatches line up down the list.
pub const THEME_MENU_GAP: usize = 3;

/// Columns between the list column and the preview beside it — codex's
/// side gap plus its panel inset.
pub const THEME_PREVIEW_GAP: usize = 4;

/// The narrowest preview column the side-by-side page lays out (codex's
/// `WIDE_PREVIEW_MIN_WIDTH`): the room the sample needs unwrapped, so the
/// block keeps its height at every width that gets it. A terminal with less
/// than the list, the gap and this stacks the preview under the list.
pub const THEME_PREVIEW_MIN_COLS: usize = 44;

/// One swatch cell — a row wears five, in its own theme's accent, link,
/// success, warning and error colours, so the palettes compare at a glance
/// the way the `/spinner` rows' live spinners do.
pub const THEME_SWATCH: &str = "●";

/// The preview's sample conversation: the user asks for a change, an `Edit`
/// call makes it (a real numbered diff cell — the code theme, the diff tints,
/// the inline-diff marks and the success bullet in one cell), and the reply
/// names the file in inline code over a fenced code line. Rendered by the
/// conversation's own builders under the highlighted theme, so the preview
/// and the conversation can never disagree.
pub const THEME_PREVIEW_USER: &str = "Rename the greeting in greet.py";
pub const THEME_PREVIEW_TOOL: &str = "Edit";
pub const THEME_PREVIEW_PATH: &str = "greet.py";
pub const THEME_PREVIEW_OLD: &str = "def greet(name):\n    print(\"Hello, world\")\n";
pub const THEME_PREVIEW_NEW: &str = "def greet(name):\n    print(f\"Hello, {name}\")\n";
pub const THEME_PREVIEW_REPLY: &str =
    "Done — `greet.py` greets by name now:\n```python\ngreet(\"Zeroth\")  # Hello, Zeroth\n```";

// --- the read-only /donate page (docs/ui/donate.md). The /hooks menu's frame —
// the family's indent, its `❯` marker, the dim meta ink — over the const
// donation-address catalog, each address inside the /login device page's
// rounded box (the `DEVICE_BOX_*` glyphs). Only its own words and the three
// colours below live here; the title borrows the banner's gradient. ---

/// The heart leading the title — the sponsor glyph, in a span of its own so
/// it keeps a colour of its own beside the gradient-washed name.
pub const DONATE_HEART: &str = "♥ ";

/// The title's verb; the app's name ([`HEADER_NAME`]) follows it, so the
/// page reads `Support Zeroth` from the one place the name lives.
pub const DONATE_TITLE_PREFIX: &str = "Support ";

/// The dim rows under the title: what the project is, what a donation does.
/// Wrapped, never cut (`docs/ui/view-flow.md`).
pub const DONATE_BLURB: &str = "Free and open source, developed in the open. \
If it earns a place in your terminal, a donation keeps the work going — thank you.";

/// The amber caution over the hint — the one irreversible mistake the page
/// can lead to. It points at the rows rather than restating them, so a
/// catalog entry (or a network) added later is covered without touching
/// it: the entries say where their address is reachable, and this says
/// nowhere else is.
pub const DONATE_CAUTION: &str = "Send each coin only over a network listed under \
its address — a transfer on any other network cannot be recovered.";

/// The dim key hint — the page's whole grammar.
pub const DONATE_HINT: &str = "↑↓ navigate  enter/c copy address  esc close";

/// Columns between a row's ticker and its coin name.
pub const DONATE_LABEL_GAP: &str = "  ";

/// The networks caption's lead-in when the address is reachable on exactly
/// one network, and when it is reachable on several. Two constants rather
/// than one plus an `s`: the label agrees with what it introduces, and a
/// `Network(s):` hedge would read as generated text on the one page a
/// reader is checking character by character.
pub const DONATE_NETWORK_LABEL: &str = "Network: ";
pub const DONATE_NETWORKS_LABEL: &str = "Networks: ";

/// What joins the network names in that caption.
pub const DONATE_NETWORK_SEPARATOR: &str = ", ";

/// The heart's colour — the palette's red, the one place that hue means
/// affection rather than failure.
pub fn donate_heart_color() -> Color {
    error_color()
}

/// An address — bright and bold like the device page's one-time code
/// ([`device_code_color`]): the one thing on the page to transcribe.
pub fn donate_address_color() -> Color {
    device_code_color()
}

/// The caution's amber — the ask review's warning hue
/// ([`ask_warning_color`]), on a page where everything else is dim or
/// bright, so the eye lands on the one line that can go wrong first.
pub fn donate_caution_color() -> Color {
    ask_warning_color()
}

// --- the /secrets page (docs/state/secrets.md). The /donate page's frame — the
// family's indent, its `❯` marker, the dim meta ink, the banner-gradient
// title — over the user's secrets, and a three-field form under the same
// frame. Every value is masked **in the builder**: a flowed row is committed
// to real scrollback as text, so a mask applied at paint time would come too
// late. Only the page's own words and geometry live here; its colours are
// the family's. ---

/// The list page's title.
pub const SECRETS_TITLE: &str = "Secrets";

/// The dim rows under the list's title: what a secret is for. Wrapped, never
/// cut (`docs/ui/view-flow.md`).
pub const SECRETS_BLURB: &str = "Credentials the agent uses by placeholder. The values \
stay out of the conversation, the screen and the model's context.";

/// The dim row an empty store shows over its add row.
pub const SECRETS_EMPTY: &str = "No secrets yet. Add one, and the agent can use it as \
<secret:NAME> in any command, file or input.";

/// The list's last row: add a secret.
pub const SECRETS_ADD_ROW: &str = "+ Add a secret";

/// The list's mask — **fixed** at eight dots whatever the value's length,
/// so the list says a value is there and never how long it is.
pub const SECRETS_LIST_MASK: &str = "••••••••";

/// Columns between a list row's placeholder, its mask and its context.
pub const SECRETS_COLUMN_GAP: &str = "  ";

/// The list's dim key hint — its whole grammar — and the add row's.
pub const SECRETS_LIST_HINT: &str =
    "↑↓ navigate  enter edit  n new  c copy placeholder  d delete  esc close";
pub const SECRETS_ADD_HINT: &str = "↑↓ navigate  enter add a secret  esc close";

/// The form's titles: a new secret, or `Edit <secret:NAME>`.
pub const SECRETS_NEW_TITLE: &str = "New secret";
pub const SECRETS_EDIT_TITLE: &str = "Edit ";

/// The dim row under the form's title: who sees what.
pub const SECRETS_FORM_BLURB: &str = "The agent is told the name and the context, never the value.";

/// The form's field labels, padded to one column ([`SECRETS_LABEL_WIDTH`]).
pub const SECRETS_NAME_LABEL: &str = "Name";
pub const SECRETS_VALUE_LABEL: &str = "Value";
pub const SECRETS_CONTEXT_LABEL: &str = "Context";

/// The label column's width: the longest label and a two-column gap.
pub const SECRETS_LABEL_WIDTH: usize = 9;

/// The column every field's text starts at — the indent, the focus marker
/// and the label column. The form's caret is seated from it.
pub const SECRETS_FIELD_COL: u16 = 2 + 2 + SECRETS_LABEL_WIDTH as u16;

/// What an empty field shows, dim, where its text will go.
pub const SECRETS_NAME_EMPTY: &str = "e.g. ROOT_PASSWORD";
pub const SECRETS_VALUE_EMPTY: &str = "type or paste the value";
pub const SECRETS_VALUE_KEEP: &str = "leave empty to keep the current value";
pub const SECRETS_CONTEXT_EMPTY: &str = "optional: what it is for, told to the agent";

/// The dim lead-in to the placeholder a typed name makes.
pub const SECRETS_USE_AS: &str = "Use it as ";

/// The form's dim key hints: on the name and the value, then on the last
/// field, where Enter saves.
pub const SECRETS_FORM_HINT_NEXT: &str = "enter next  tab/↑↓ move  esc back";
pub const SECRETS_FORM_HINT_SAVE: &str = "enter save  tab/↑↓ move  esc back";

/// What leads a refusal's red row.
pub const SECRETS_ERROR_MARK: &str = "✗ ";

/// The red question a first `d` asks in place of the hint.
pub fn secrets_delete_question(name: &str) -> String {
    format!(
        "Delete {}? Press d again to confirm, any other key keeps it.",
        crate::secrets::placeholder(name)
    )
}

// --- the read-only /export page (docs/ui/export.md). The /donate page's frame
// — the family's indent, its `❯` marker, the dim meta ink — over the two
// export targets, the highlighted row's description under the list (the
// /settings shape).

/// The page's title.
pub const EXPORT_TITLE: &str = "Export conversation";

/// The dim rows under the title: what the export *is*. Wrapped, never cut
/// (`docs/ui/view-flow.md`).
pub const EXPORT_BLURB: &str = "The whole transcript as plain text — every message and \
every tool call's full output, as Ctrl+O shows it.";

/// The clipboard row's description.
pub const EXPORT_CLIPBOARD_DESC: &str = "Copies the transcript to the system clipboard.";

/// The file row's description: the file's shape, then `into {cwd}.` — the
/// directory as the footer shows it ([`EXPORT_FILE_DESC_DIR_FALLBACK`] before
/// the boundary has injected one).
pub const EXPORT_FILE_DESC_PREFIX: &str = "Writes conversation-YYYY-MM-DD-HHMMSS.txt into ";

/// The directory the file row names when no session info has been injected
/// yet (the pure core never reads the environment).
pub const EXPORT_FILE_DESC_DIR_FALLBACK: &str = "the working directory";

/// The dim key hint — the page's whole grammar.
pub const EXPORT_HINT: &str = "↑↓ navigate  enter select  esc close";

// --- the read-only /context page (docs/architecture/context-usage.md). The /donate
// page's frame — the family's indent, its `❯` marker, the dim meta ink, the
// pages' title dress — over the context window: a bar standing for the
// whole window, a legend of what fills it with the highlighted row's detail
// beside or under it, and what the provider reported about the cache. ---

/// The page's title.
pub const CONTEXT_USAGE_TITLE: &str = "Context usage";

/// The dim key hint — the page's whole grammar.
pub const CONTEXT_USAGE_HINT: &str = "↑↓ select  ctrl+d raw context  esc close";

/// Marks the subtitle's total as an estimate — the categories
/// are counted app-side, the usage rows are the provider's own.
pub const CONTEXT_USAGE_ESTIMATE_MARK: &str = "≈";

/// What the subtitle says when the model's window is unknown.
pub const CONTEXT_USAGE_WINDOW_UNKNOWN: &str = "window unknown";

/// The window bar's cells: a category's, the free space's, and the hatched
/// headroom auto-compact keeps.
pub const CONTEXT_USAGE_BAR_USED: &str = "█";
pub const CONTEXT_USAGE_BAR_FREE: &str = "░";
pub const CONTEXT_USAGE_BAR_RESERVED: &str = "╱";

/// The legend's swatches, echoing the bar's cells.
pub const CONTEXT_USAGE_SWATCH: &str = "■";
pub const CONTEXT_USAGE_FREE_SWATCH: &str = "□";
pub const CONTEXT_USAGE_RESERVED_SWATCH: &str = "╱";

/// The two legend rows for the room that is left.
pub const CONTEXT_USAGE_FREE_LABEL: &str = "Free space";
pub const CONTEXT_USAGE_RESERVED_LABEL: &str = "Auto-compact";

/// The gap between the legend's columns, and between a detail row's.
pub const CONTEXT_USAGE_COLUMN_GAP: &str = "  ";

/// The gap between the legend and the detail beside it.
pub const CONTEXT_USAGE_PANEL_GAP: &str = "   ";

/// The narrowest terminal the detail sits **beside** the legend on; under
/// it, the detail stacks under the legend.
pub const CONTEXT_USAGE_SIDE_BY_SIDE_MIN_COLS: u16 = 76;

/// The narrowest the detail beside the legend may be; a terminal that
/// leaves it less stacks the detail under the legend instead.
pub const CONTEXT_USAGE_DETAIL_MIN_COLS: usize = 24;

/// The most parts a whole page's detail lists (more fold into one `… N
/// more` row), and the most a compact page's stacked detail does.
pub const CONTEXT_USAGE_DETAIL_PARTS: usize = 6;
pub const CONTEXT_USAGE_DETAIL_COMPACT_PARTS: usize = 3;

/// The **Messages** detail of an empty conversation — a fact worth
/// stating, rather than an empty panel.
pub const CONTEXT_USAGE_NOTHING_YET: &str =
    "Nothing yet — the conversation starts with your first message.";

/// The **Free space** detail: how far the conversation is from where it
/// compacts itself — or, with no threshold, from the window's end. `free`,
/// `threshold` are humanized token counts; `settings_hint` adds where auto
/// compact is turned back on, for the main session that turned it off.
pub fn context_usage_free_text(
    free: &str,
    threshold: Option<&str>,
    past_threshold: bool,
    settings_hint: bool,
) -> String {
    match threshold {
        Some(threshold) if past_threshold => {
            format!("Past the auto-compact threshold of {threshold}. /compact frees room now.")
        }
        Some(threshold) => format!("{free} until auto-compact runs at {threshold}."),
        None if settings_hint => {
            format!("{free} until the window is full. Auto compact is off — /settings turns it on.")
        }
        None => format!("{free} until the window is full."),
    }
}

/// The **Auto-compact** detail: the headroom it keeps, and what runs there.
pub fn context_usage_headroom_text(headroom: &str, threshold: &str) -> String {
    format!(
        "The last {headroom} of the window stays free: past {threshold} the session \
         summarizes itself. /compact does it now."
    )
}

/// A detail part's bar — measured against the slice's largest part — and
/// its longest run.
pub const CONTEXT_USAGE_PART_BAR: &str = "━";
pub const CONTEXT_USAGE_PART_BAR_MAX: usize = 12;

/// The usage section's row labels — the provider's own numbers.
pub const CONTEXT_USAGE_LAST_REQUEST: &str = "Last request";
pub const CONTEXT_USAGE_CACHE_HIT: &str = "Cache hit";
pub const CONTEXT_USAGE_SINCE_LAUNCH: &str = "Since launch";
pub const CONTEXT_USAGE_THIS_AGENT: &str = "This agent";

/// The gap after the usage section's labels.
pub const CONTEXT_USAGE_LABEL_GAP: &str = "   ";

/// The usage section before any frame arrived (the offline dummy never
/// sends one).
pub const CONTEXT_USAGE_NO_USAGE: &str = "no usage reported yet";

/// The cache meter's cells, and its shortest and longest run.
pub const CONTEXT_USAGE_METER_FILL: &str = "█";
pub const CONTEXT_USAGE_METER_EMPTY: &str = "░";
pub const CONTEXT_USAGE_METER_MIN: usize = 8;
pub const CONTEXT_USAGE_METER_MAX: usize = 24;

/// The window's fill past which the subtitle's share turns amber; past the
/// auto-compact threshold (or, with none, 90%) it turns red.
pub const CONTEXT_USAGE_WARN_SHARE: f64 = 0.75;
pub const CONTEXT_USAGE_FULL_SHARE: f64 = 0.9;

/// A category's colour — the bar's cells, the legend's swatch, the detail's
/// bars. The system prompt and the tools wear the Ctrl+D view's system and
/// tool tags (amber, purple), the conversation the brand accent, and the
/// reminder's sections the other hues — three of them blends, so nine
/// categories read apart on a palette of six. They are chosen in bar order,
/// so every pair of neighbours on the bar differs in hue: amber, purple,
/// peach (amber into red), blue, green, orchid (purple into red), red, teal
/// (green into blue), cyan. Under the `ansi` theme the three blends are
/// bright yellow, bright magenta and bright green ([`category_blend`]), so
/// no two categories share a colour in any theme.
pub fn context_usage_color(category: crate::context_usage::Category) -> Color {
    use crate::context_usage::Category;
    match category {
        Category::SystemPrompt => context_system_color(),
        Category::SystemTools => context_tool_color(),
        Category::ProjectDocs => palette().link,
        Category::Skills => palette().success,
        Category::AgentTypes => category_blend(palette().purple, palette().error),
        Category::Secrets => palette().error,
        Category::Messages => palette().accent,
    }
}

/// Halfway between two hues: the colour of a category no role covers. A
/// theme of named colours cannot mix one — [`lerp_color`] steps to an end,
/// and both blends landed on the red the secrets wear — so there it is the
/// first end's bright twin instead, every category still its own colour.
fn category_blend(a: Color, b: Color) -> Color {
    match (a, b) {
        (Color::Rgb(..), Color::Rgb(..)) => lerp_color(a, b, 0.5),
        _ => bright(a),
    }
}

/// The free space's cells and swatch — the dim everything empty wears.
pub fn context_usage_free_color() -> Color {
    tool_dim_color()
}

/// The auto-compact headroom's hatching — the frame's rule colour, a shade
/// apart from the free space.
pub fn context_usage_reserved_color() -> Color {
    border_color()
}

/// The cache meter's filled cells — the success green.
pub fn context_usage_meter_color() -> Color {
    tool_ok_color()
}

/// The subtitle's share past [`CONTEXT_USAGE_WARN_SHARE`] — the caution
/// amber the ask review warns in.
pub fn context_usage_warn_color() -> Color {
    ask_warning_color()
}

/// The subtitle's share past the auto-compact threshold — the error red.
pub fn context_usage_full_color() -> Color {
    error_color()
}

/// How full a context window reads wherever its share is shown — the
/// `/context` page's subtitle and the footer's gauge, one rule so the two can
/// never disagree: `None` while there is room (the caller's own dim), amber
/// ([`context_usage_warn_color`]) from [`CONTEXT_USAGE_WARN_SHARE`], red
/// ([`context_usage_full_color`]) from the auto-compact `threshold` — or,
/// with none, [`CONTEXT_USAGE_FULL_SHARE`]. An empty window has no share.
pub fn context_fill_color(used: u64, window: u64, threshold: Option<u64>) -> Option<Color> {
    if window == 0 {
        return None;
    }
    let fill = used as f64 / window as f64;
    let full = threshold.map_or(CONTEXT_USAGE_FULL_SHARE, |threshold| {
        threshold as f64 / window as f64
    });
    if fill >= full {
        Some(context_usage_full_color())
    } else if fill >= CONTEXT_USAGE_WARN_SHARE {
        Some(context_usage_warn_color())
    } else {
        None
    }
}

// --- the read-only /usage page (docs/providers/usage.md). The /context page's frame
// and meter language over every signed-in subscription: a section each, its
// head naming the provider and plan, a bar per limit filling as it is spent
// — green, amber, red at the /context thresholds — and when it resets. ---

/// The page's title.
pub const USAGE_TITLE: &str = "Usage limits";

/// The dim key hint's parts — the page's whole grammar — joined by
/// [`USAGE_LABEL_GAP`]: `↑↓ select` only with more than one subscription to
/// move between, `c copy link` only with a link to copy.
pub const USAGE_HINT_SELECT: &str = "↑↓ select";
pub const USAGE_HINT_REFRESH: &str = "r refresh";
pub const USAGE_HINT_COPY: &str = "c copy link";
pub const USAGE_HINT_CLOSE: &str = "esc close";

/// The subtitle's word for a read in flight: over numbers already shown,
/// and before any are.
pub const USAGE_REFRESHING: &str = "refreshing…";
pub const USAGE_LOADING_ALL: &str = "loading…";

/// What a spent limit's row says first, in red.
pub const USAGE_REACHED: &str = "limit reached";

/// A section whose first read is in flight.
pub const USAGE_LOADING: &str = "Loading usage…";

/// The page with no subscription signed in — advice, in `/model`'s accent.
pub const USAGE_NO_SUBSCRIPTION: &str = "No subscription is signed in — run /login and \
     choose Sign in with an account to see its plan limits here. An API key bills per token \
     and has no plan to measure.";

/// The tag on the subscription the session is running on.
pub const USAGE_IN_USE: &str = "in use";

/// A meter with no limit.
pub const USAGE_UNLIMITED: &str = "Unlimited";

/// What leads the link to the highlighted subscription's own usage page,
/// over the hint.
pub const USAGE_DETAILS_PREFIX: &str = "Details at ";

/// What leads a failed read's reason: before anything was shown, and over a
/// report kept from the last good read.
pub const USAGE_LOAD_FAILED: &str = "Couldn't load usage: ";
pub const USAGE_REFRESH_FAILED: &str = "Couldn't refresh: ";

/// The mark a warning or error note opens with.
pub const USAGE_NOTE_MARK: &str = "⚠ ";

/// What a section's rows sit under: the marker's width and two columns
/// more, so they read as one block under the provider's name.
pub const USAGE_SECTION_INDENT: &str = "    ";

/// The extra inset of a per-model limit's rows under its sub-heading.
pub const USAGE_GROUP_INDENT: &str = "  ";

/// Between a label and its value, and between a bar's parts.
pub const USAGE_LABEL_GAP: &str = "  ";

/// Between the parts of a meter's trailer (`420 of 1,000 credits · resets
/// in 6d`), a balance and its detail, and the parts of a section's head.
pub const USAGE_SEPARATOR: &str = " · ";

/// A limit's bar: the `/context` meter's cells, the longest on any terminal
/// with room for it and a countdown beside it, shorter only on a narrower
/// one, and never below the shortest — the text beside it giving way first.
pub const USAGE_BAR_MIN: usize = 8;
pub const USAGE_BAR_MAX: usize = 20;

/// The share used at which a bar turns amber, and red — the `/context`
/// meter's thresholds, as percentages.
pub const USAGE_WARN_PERCENT: f64 = 75.0;
pub const USAGE_FULL_PERCENT: f64 = 90.0;

/// A bar's spent cells while the limit is healthy — the success green.
pub fn usage_meter_color() -> Color {
    context_usage_meter_color()
}

/// A bar's remaining cells.
pub fn usage_empty_color() -> Color {
    context_usage_free_color()
}

/// A bar past [`USAGE_WARN_PERCENT`], and a warning note — the amber.
pub fn usage_warn_color() -> Color {
    context_usage_warn_color()
}

/// A bar past [`USAGE_FULL_PERCENT`], an error note, a failed read — the red.
pub fn usage_full_color() -> Color {
    context_usage_full_color()
}

/// The `in use` tag and `Unlimited` — the success green.
pub fn usage_ok_color() -> Color {
    tool_ok_color()
}

// --- the read-only /hooks menu (docs/tools/hooks-menu.md). It reuses the picker
// family's accents — model_selected_color() for the selection, model_id_color()
// for unselected labels, model_meta_color() for everything dim,
// the pages' title dress (`header::page_title_line`) for the titles,
// border_color() for the frame and the detail page's command box. ---

/// The events-level title.
pub const HOOKS_TITLE: &str = "Hooks";

/// The detail page's title.
pub const HOOKS_DETAIL_TITLE: &str = "Hook details";

/// The most list rows shown at once — the reference `Select`'s visible-option
/// count; a longer list scrolls to keep the selection **centered**
/// (`centered_window`, the `/model` list's), the window's edge rows wearing
/// the [`HOOKS_UP_MARKER`]/[`HOOKS_DOWN_MARKER`] overflow arrows.
pub const HOOKS_MENU_MAX_ROWS: usize = 5;

/// The selected row's marker (the permission prompt's `❯`); unselected rows
/// get spaces the same width.
pub const HOOKS_MARKER: &str = "❯ ";

/// The scrolled window's edge markers — more rows above / below.
pub const HOOKS_UP_MARKER: &str = "↑ ";
pub const HOOKS_DOWN_MARKER: &str = "↓ ";

/// Columns between the widest visible label and the description column, so
/// the summaries line up in a block (the `/settings` value column's idea).
pub const HOOKS_DESC_GAP: usize = 3;

/// The read-only banner under the count (events level only) — the reference's
/// info line with our file and assistant names, its docs link swapped for the
/// format's own doc.
pub const HOOKS_INFO: &str = "ℹ Open a hook to trust it, or to stop trusting it. To \
add or modify hooks, edit hooks.json directly or ask zeroth. See docs/tools/hooks.md";

/// The note when the file holds hooks but the session has them off
/// (`/settings`, `ZEROTH_HOOKS=0`) — the reference's restricted-by-policy
/// slot, red because every listed guard is currently not running.
pub const HOOKS_DISABLED_NOTE: &str = "Hooks are disabled this session — enable them in /settings";

/// The empty state (an event with nothing configured), two dim lines.
pub const HOOKS_EMPTY: &str = "No hooks configured for this event.";
pub const HOOKS_EMPTY_HINT: &str = "To add hooks, edit hooks.json directly or ask zeroth.";

/// The list levels' key hint, and the detail/empty pages' Esc-only one.
pub const HOOKS_HINT: &str = "Enter to confirm · Esc to cancel";
pub const HOOKS_DETAIL_HINT: &str = "Esc to go back";

/// Trust in the `/hooks` menu (`docs/state/config-review.md`): the amber badge an
/// untrusted hook's row wears, the detail page's `Trust:` values, and its
/// hint — Enter toggles.
pub const HOOKS_UNTRUSTED_BADGE: &str = "⚠ not trusted";
pub const HOOKS_TRUSTED_VALUE: &str = "✔ trusted";
pub const HOOKS_UNTRUSTED_VALUE: &str = "⚠ not trusted — it does not run";
pub const HOOKS_TRUST_HINT: &str = "Enter to trust · Esc to go back";
pub const HOOKS_DISTRUST_HINT: &str = "Enter to stop trusting · Esc to go back";

/// The single-source tags — one `hooks.json`, one origin (the reference has
/// user/project/local/plugin layers; ours is user-level only,
/// `docs/tools/hooks.md`): the `[User]` matcher-row prefix, the hook rows'
/// `User Settings` description, and the detail page's `Source:` label.
pub const HOOKS_SOURCE_INLINE: &str = "User";
pub const HOOKS_SOURCE_HEADER: &str = "User Settings";
pub const HOOKS_SOURCE_LABEL: &str = "User settings";

/// The detail page's field-name column: `Source:` (the widest) plus its pad,
/// so the values line up (`Event:    PreToolUse`).
pub const HOOKS_FIELD_COL: usize = 10;

/// The startup review (`docs/state/config-review.md`) — the hooks menu's sibling,
/// borrowing the `HOOKS_*` marker and frame and the ask modal's checkboxes;
/// only its own vocabulary lives here.
///
/// The explanation under each page's title.
pub const REVIEW_HOOKS_INFO: &str = "Hooks run shell commands on their own whenever their \
event fires, outside the permission prompt. Only hooks you trust will run.";
/// The two action rows on each page — what Enter on them answers.
pub const REVIEW_TRUST_LABEL: &str = "Trust selected and continue";
pub const REVIEW_SKIP_LABEL: &str = "Continue without trusting";

/// The key hints, saying what Esc means on each page.
pub const REVIEW_HOOKS_HINT: &str =
    "Space to select · Enter to confirm · Esc to continue without trusting";

/// A changed item's amber badge, and the line saying what it was: a hook's
/// old command, or what changed in a server's entry.
pub const REVIEW_CHANGED_BADGE: &str = "changed";
pub const REVIEW_WAS_PREFIX: &str = "was: ";

/// The gap before a badge (and a folded item's first detail row), and
/// before the title's `1/2` page counter.
pub const REVIEW_BADGE_GAP: &str = "  ";
pub const REVIEW_COUNTER_GAP: &str = "  ";

/// Between a file heading's scope word and its path: `User · ~/…`.
pub const REVIEW_SCOPE_SEPARATOR: &str = " · ";

/// The line a windowed page's highlighted item names its file on, the file
/// headings being gone.
pub const REVIEW_FROM_PREFIX: &str = "from ";

/// How many detail rows the highlighted item keeps on the rung that first
/// cuts it.
pub const REVIEW_CAPPED_DETAIL_ROWS: usize = 3;

/// The closing direction on the detail page.
pub const HOOKS_MODIFY_NOTE: &str =
    "To modify or remove this hook, edit hooks.json directly or ask zeroth to help.";
