//! Whole-conversation rendering: the inline scrollback walk over `history` and
//! the tail a resize or an overlay return repaints from it.
//!
//! The repaint is content-anchored and reflows both directions (CLAUDE.md
//! invariant 3); the tail restores the startup banner over a short conversation
//! (`docs/ui/header.md`).

use super::theme::*;
use super::wrap::{cols, truncate_cols};
use super::*;

/// Whether `item` is a `!` shell command's header message (`Role::Shell`).
/// Such an item gets **no** blank spacer after it: its tool's `⎿` output (or
/// the live `⎿ Running…` preview) sits flush below, forming one exec cell
/// (docs/input/shell-command.md). Shared by [`conversation_lines`] and
/// [`transcript_lines`] so the inline view and the Ctrl+O overlay agree.
pub(super) fn is_shell_header(item: &HistoryItem) -> bool {
    matches!(item, HistoryItem::Message(m) if m.role == Role::Shell)
}

/// The slice of `history` that has actually reached scrollback: everything
/// except what the commit path is still holding while the live strip speaks
/// for it — the **open collapsed group** from `group_start` on
/// ([`App::tool_group_range`](crate::app::App::tool_group_range),
/// `docs/ui/tool-groups.md`). Every rebuild renders this, so a repaint
/// landing mid-group cannot paint a line the
/// commit is about to write.
#[must_use]
pub fn committed_history<'a>(
    history: &'a [HistoryItem],
    _queue: &VecDeque<ToolCall>,
    group_start: Option<usize>,
) -> &'a [HistoryItem] {
    match group_start {
        Some(start) => &history[..start.min(history.len())],
        None => history,
    }
}

/// Build the whole conversation as styled lines, mirroring how it was streamed
/// to scrollback: each message's wrapped lines (or each tool call's collapsed
/// peek), with a blank spacer after every item. Used to repaint after a resize
/// clears the screen, or when returning from the tool-output view.
///
/// Unless `verbose` (the `/settings` **Verbose output** row), a run of reads,
/// searches and listings is one settled line — the one the commit path wrote
/// when the group closed ([`tool_group_lines`], `docs/ui/tool-groups.md`).
#[must_use]
pub fn conversation_lines(
    history: &[HistoryItem],
    width: u16,
    paths: &PathDisplay,
    verbose: bool,
) -> Vec<Line<'static>> {
    conversation_lines_tracked(history, width, paths, verbose, |_, _, _| {})
}

/// [`conversation_lines`] with a per-item report: after an item's own rows are
/// appended (its image block included, its trailing spacer not), `built` gets
/// its history index and `[start, start + count)` in the returned lines. A
/// collapsed group is one report-less block — it holds tool cells,
/// not the messages a caller tracks. The full-screen mode uses this to know
/// where each user message's pinned header starts (`docs/ui/full-screen.md`).
#[must_use]
pub fn conversation_lines_tracked(
    history: &[HistoryItem],
    width: u16,
    paths: &PathDisplay,
    verbose: bool,
    built: impl FnMut(usize, usize, usize),
) -> Vec<Line<'static>> {
    conversation_lines_expanded(history, width, paths, verbose, |_| false, built)
}

/// [`conversation_lines_tracked`] that can render selected items **expanded**
/// — the Ctrl+O form ([`super::transcript::expanded_item_lines`]) instead of
/// their collapsed inline cell. What the screen-owning mode's click-to-expand
/// toggles (`docs/ui/full-screen.md`); an item the predicate does not select,
/// or one with no fuller form ([`is_expandable`]), renders as usual.
#[must_use]
pub fn conversation_lines_expanded(
    history: &[HistoryItem],
    width: u16,
    paths: &PathDisplay,
    verbose: bool,
    expanded: impl Fn(usize) -> bool,
    built: impl FnMut(usize, usize, usize),
) -> Vec<Line<'static>> {
    conversation_lines_carded(history, width, paths, verbose, expanded, |_, _| None, built)
}

/// [`conversation_lines_expanded`] with a **card** transform: after an item's
/// rows are rendered, each line is offered to `card_for`; `Some(rows)` replaces
/// it (a lone URL line becoming a link preview card, `docs/ui/rich-links.md`).
/// The transform runs per item, so the item's reported range and every later
/// offset include the card rows naturally — no post-hoc line arithmetic.
#[must_use]
pub fn conversation_lines_carded(
    history: &[HistoryItem],
    width: u16,
    paths: &PathDisplay,
    verbose: bool,
    expanded: impl Fn(usize) -> bool,
    card_for: impl Fn(&HistoryItem, &Line<'static>) -> Option<Vec<Line<'static>>>,
    mut built: impl FnMut(usize, usize, usize),
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut index = 0;
    while index < history.len() {
        if !verbose && let Some(len) = crate::app::group_run(&history[index..], paths) {
            lines.extend(tool_group_lines(&history[index..index + len], width, paths));
            lines.push(Line::default());
            index += len;
            continue;
        }
        let item = &history[index];
        let item_index = index;
        let start = lines.len();
        index += 1;
        let expanded_item = expanded(item_index) && is_expandable(item);
        if expanded_item {
            lines.extend(super::transcript::expanded_item_lines(item, width, paths));
        } else {
            match item {
                // A task tool call renders NOTHING inline — no cell, no spacer:
                // the live checklist is its display, and the record expands only
                // in the Ctrl+O transcript (docs/tools/task-tools.md).
                HistoryItem::TaskCall(_) => continue,
                // What the hooks said is their `● Ran 1 Stop hook` cell; a
                // note no hook said anything about — a continuation prompt, a
                // note recorded before reports — is cell-less inline, the Ctrl+O
                // transcript its record (docs/tools/hooks.md *The hook-run cell*).
                HistoryItem::HookNote(note) => match &note.report {
                    Some(report) => lines.extend(hook_report_lines(report, width)),
                    None => continue,
                },
                HistoryItem::Message(m) => lines.extend(message_lines_with_paths(
                    m.role,
                    &m.text,
                    width,
                    Some(paths),
                )),
                HistoryItem::Tool(t) => lines.extend(tool_lines(t, width, paths)),
                HistoryItem::Summary(_) => lines.extend(super::status::review_turn_summary_lines(
                    history, item_index, width, paths,
                )),
                HistoryItem::Background(n) => lines.extend(background_notice_lines(n, width)),
                HistoryItem::AgentGroup(g) => lines.extend(agent_group_lines(g, width)),
                HistoryItem::AgentNotice(n) => lines.extend(agent_notice_lines(n, width)),
                HistoryItem::Compaction(c) => lines.extend(compaction_lines(c, width)),
                // Collapsed: only the `Thought for …` line reaches scrollback —
                // the chain-of-thought expands in Ctrl+O (docs/ui/thinking-stream.md).
                HistoryItem::Reasoning(r) => lines.extend(reasoning_lines(r, width)),
            }
            // A picture the item carries — an image `read`'s result — hangs
            // below the cell, one blank row apart and flush at the left margin
            // (`docs/ui/images.md`). Empty for everything else, for a pasted
            // attachment (the user picked that picture; the wire carries it,
            // the screen does not repeat it), and for every session where
            // images are off.
            lines.extend(super::image::image_block_lines(item, width));
        }
        // A lone-link line becomes its preview card, still inside this item so
        // `built` and every later offset count the card's rows.
        if lines.len() > start {
            let rendered = lines.split_off(start);
            for line in rendered {
                if let Some(card) = card_for(item, &line) {
                    lines.extend(card);
                } else {
                    lines.push(line);
                }
            }
        }
        built(item_index, start, lines.len() - start);
        // Blank spacer after every item — except a shell command's header:
        // its cell stays flush ([`is_shell_header`]). The expanded path already
        // carries its own spacer ([`super::transcript::expanded_item_lines`]).
        if !expanded_item && !is_shell_header(item) {
            lines.push(Line::default());
        }
    }
    lines
}

/// Whether an item has a fuller form than its inline cell: a tool call's whole
/// output, a thought's chain, a compaction's summary body, an agent group's
/// members, a hook report, a task call's hidden record. Messages, notices and
/// summaries are already whole inline.
#[must_use]
pub fn is_expandable(item: &HistoryItem) -> bool {
    matches!(
        item,
        HistoryItem::Tool(_)
            | HistoryItem::TaskCall(_)
            | HistoryItem::Reasoning(_)
            | HistoryItem::Compaction(_)
            | HistoryItem::AgentGroup(_)
            | HistoryItem::HookNote(_)
    )
}

/// The last `max_rows` lines of the conversation — i.e. the tail that fits on
/// screen above the live region. After a width shrink ratatui clears the visible
/// screen (older lines survive in the terminal's own scrollback), so we only
/// need to repaint what was on screen; capping at `max_rows` also avoids
/// re-scrolling content the terminal already kept.
#[must_use]
pub fn repaint_lines(
    history: &[HistoryItem],
    width: u16,
    max_rows: usize,
    paths: &PathDisplay,
    verbose: bool,
) -> Vec<Line<'static>> {
    keep_last_rows(conversation_lines(history, width, paths, verbose), max_rows)
}

/// The repaint tail for a mid-stream conversation rebuild
/// (`main.rs::repaint_conversation`): the finished history plus the rows of
/// the in-flight partial reply that were **already committed** to scrollback
/// ([`StreamRender::committed_rows`]), re-rendered in place. Repainting from
/// history alone blanks the partial until its next chunk arrives (the Ctrl+O
/// disappear-then-flicker bug). The rows still to come are deliberately *not*
/// included — the caller queues them right after via [`StreamRender::commit`]
/// (the standard `insert_before` pipeline), so rows that streamed while the
/// overlay was up reach scrollback exactly once, however many there are.
#[must_use]
pub fn repaint_tail(
    history: &[HistoryItem],
    streaming: Option<&str>,
    render: &mut StreamRender,
    width: u16,
    max_rows: usize,
    paths: &PathDisplay,
    verbose: bool,
) -> Vec<Line<'static>> {
    let mut lines = conversation_lines(history, width, paths, verbose);
    if let Some(text) = streaming.filter(|text| !text.is_empty()) {
        lines.extend(render.committed_rows(text, width));
    }
    keep_last_rows(lines, max_rows)
}

/// The last `max_rows` of `lines` — the shared cap of [`repaint_lines`] and
/// [`repaint_tail`] (applied *after* the partial's rows join the tail, so the
/// budget always keeps the newest rows, like a screen would).
fn keep_last_rows(mut lines: Vec<Line<'static>>, max_rows: usize) -> Vec<Line<'static>> {
    if lines.len() > max_rows {
        lines = lines.split_off(lines.len() - max_rows);
    }
    lines
}

/// One user message's place in the rendered conversation: the rows it occupies,
/// the history item it came from, and its wording — what the pinned prompt row
/// needs (`docs/ui/conversation-tool-details-and-pin.md`).
///
/// Built by whoever lays the conversation out (the frame builders in
/// `src/tui/`), since only they know the row offsets a wrapped message
/// produced; the two decisions the pin makes on top of them are pure and live
/// here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPrompt {
    /// The first row it occupies, inclusive.
    pub start: usize,
    /// One past its last row.
    pub end: usize,
    /// The message's raw text, as recorded in history.
    pub text: String,
}

/// The prompt the pinned row shows: the latest user message that has scrolled
/// **entirely** above `window_start`.
///
/// A message whose last row is still on screen is not pinned, even when its
/// first row has gone: the reader can already see the question, and a copy of
/// its opening line directly above its own continuation reads as two
/// paragraphs of the same sentence. This is the difference from the rule the
/// pin shipped with (`start < window_start`), which behaved that way for every
/// wrapped prompt. `None` while any part of a prompt is visible, and for an
/// empty conversation.
#[must_use]
pub fn pinned_prompt(prompts: &[UserPrompt], window_start: usize) -> Option<&UserPrompt> {
    // The turn the window's top sits in: the last prompt that has begun by
    // the time the window starts. Only that one may be pinned. Falling back to
    // an earlier turn would name the wrong question — with a tall window the
    // newest turn's reply fills the screen while its question is still inside
    // it, and the strip would show the turn before.
    let turn = prompts
        .iter()
        .rev()
        .find(|prompt| prompt.start <= window_start)?;
    (turn.end <= window_start).then_some(turn)
}

/// The pinned row: a prompt's whole wording folded onto one line — its breaks
/// and runs of blanks become single spaces — behind the user bullet, cut to
/// `width` display columns with the theme's pinned-prompt ellipsis when it does not
/// fit, and padded to the full width so its ground reads as one strip.
///
/// The whole message is what gets summarised, not its first drawn line: a long
/// question usually opens with the least informative part of itself ("look at
/// this:"), while the line that actually asks it comes after a blank row.
///
/// `hovered` lights the ink without changing the ground — the row is a control
/// that scrolls back to the prompt, so pointing at it has to say so. `None`
/// when the prompt has nothing but blanks: there is no summary to show, and a
/// bullet over an empty strip would only cost a row of conversation.
#[must_use]
pub fn pinned_prompt_line(text: &str, width: u16, hovered: bool) -> Option<Line<'static>> {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.is_empty() {
        return None;
    }
    let width = usize::from(width);
    let room = width.saturating_sub(cols(USER_BULLET));
    if room == 0 {
        return None;
    }
    let body = if cols(&folded) <= room {
        folded
    } else {
        // One column of the room buys the ellipsis; the rest is cut on a
        // grapheme boundary so a wide glyph is never split.
        let kept = truncate_cols(&folded, room.saturating_sub(cols(PINNED_PROMPT_ELLIPSIS)));
        format!("{kept}{PINNED_PROMPT_ELLIPSIS}")
    };
    let pad = " ".repeat(width.saturating_sub(cols(USER_BULLET) + cols(&body)));
    let ink = if hovered {
        pinned_prompt_hover_color()
    } else {
        pinned_prompt_color()
    };
    let ground = Style::new().bg(user_bg_color());
    Some(Line::from(vec![
        Span::styled(USER_BULLET, ground.fg(ink).add_modifier(Modifier::BOLD)),
        Span::styled(body, ground.fg(ink)),
        Span::styled(pad, ground),
    ]))
}

/// A rebuilt repaint tail with the header banner (docs/ui/header.md) restored
/// above it: `banner`, a blank spacer, then `tail`. Every purge rebuild goes
/// through this — the purge dropped scrollback whole, so the banner
/// unconditionally tops the reconstructed conversation. (An ordinary overlay
/// return never rebuilds: the on-screen banner survives untouched and the
/// queued commits flush beneath it.)
#[must_use]
pub fn banner_tail(mut banner: Vec<Line<'static>>, tail: Vec<Line<'static>>) -> Vec<Line<'static>> {
    // No banner means no spacer either: the screen-owning mode drops it when
    // the sidebar carries the brand (`docs/ui/sidebar.md`), and a stray blank
    // row there would be the one trace of a block that is not on screen.
    if banner.is_empty() {
        return tail;
    }
    banner.push(Line::default());
    banner.extend(tail);
    banner
}
