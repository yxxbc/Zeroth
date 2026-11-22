//! Painting: the inline live region, the alternate-screen overlays, and the
//! repaints that rebuild the conversation from source.
//!
//! Two pure decisions from `ui` drive everything here, and this module is only
//! their boundary half:
//!
//! - **How tall the region is.** [`Session::live_region_height`] asks the same
//!   question the next draw will, which is why a post-stream commit can reseat
//!   the viewport *before* flushing (invariant 3 — otherwise the box rises off
//!   the bottom and leaves blank rows beneath it).
//! - **When the screen rebuilds.** A full rebuild (`InlineViewport::reflow`)
//!   purges the terminal's scrollback and repaints the whole conversation
//!   from history — every resize, `/clear`, and history rewind takes it. An
//!   ordinary overlay return does **not**: everything that committed while
//!   the overlay covered the screen sits in the viewport's pending queue, so
//!   one ordinary draw flushes it above the live region and the terminal's
//!   own scrollback survives untouched ([`Session::overlay_return_repaint`]).
//!   Only a resize that landed under the overlay forces the purge (the
//!   emulator reflowed the main screen underneath, and the queued lines were
//!   rendered at the stale width).
//!
//! A repaint must not lose the in-flight partial reply — it lives in the
//! streaming buffer, not `history` — so the rebuild re-commits it through the
//! normal `insert_before` pipeline right after the reflow (`docs/ui/flicker.md`).
//! Nor may it reserve rows for a picture it can no longer draw: the purge
//! drops every encoded picture with the scrollback, so each rebuild first
//! looks again for the pictures' files ([`recheck_pictures`]).
//!
//! The clock injections live here too ([`Session::update_status_times`]): time
//! is impure, so the pure `App`/`ui` only ever see already-computed durations.

use std::io;
use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;

use zeroth::app::{App, DebugPage, HistoryItem, Role, View, edits_composer};
use zeroth::images;
use zeroth::links;
use zeroth::paste;
use zeroth::ui;

use super::Session;

#[derive(Default)]
pub(crate) struct FullReading {
    top: Option<usize>,
    expanded: std::collections::HashSet<usize>,
    generation: u64,
}

/// The screen-owning frame's **history** rows, cached apart from the reply
/// that streams over them.
///
/// The split is the whole point. Keying one cache on the streaming text's
/// length re-rendered the entire conversation on every chunk — measured at
/// 12.8 ms for a 1,000-turn session, 77% of a 60 fps frame, *per chunk*
/// (`docs/ui/full-screen.md` *Performance*). Everything here is keyed on what
/// can change what the finished rows **are**, and the in-flight reply is
/// appended per frame on top: a chunk then costs O(reply), not O(session).
#[derive(Default)]
pub(crate) struct FullTail {
    /// The conversation these rows belong to: `None` for the main one, else
    /// the viewed agent's id.
    agent: Option<String>,
    /// The conversation's identity — its generation and length — so a
    /// `/clear`, a rewind or a `/resume` rebuilds, and an ordinary append does
    /// not (the streaming reply is not in `history` until it settles).
    generation: (u64, usize),
    width: u16,
    theme: zeroth::app::Theme,
    /// Banner and finished items. The per-item offsets the pin and the hit
    /// map read live beside it, in
    /// `full_prompts` and
    /// `full_items`, and are written when this
    /// rebuilds.
    lines: Vec<ratatui::text::Line<'static>>,
}

/// Everything the screen-owning mode derives from `App` plus a size — the
/// frame cache, its scroll anchor, the hit map, the pointer's state, the
/// expanded set and the link cards — in one value, so `Session` is not the
/// owner of the view (`docs/architecture/core-host-split.md` Phase 1).
///
/// Every field is rebuildable from `App` + the terminal size; none is the only
/// copy of anything the product needs. That is what will let a second
/// conversation have its own view later, and what the
/// `a_fresh_view_rebuilds_the_same_frame` test pins.
#[derive(Default)]
pub(crate) struct ConversationView {
    /// The streamed reply's incremental renderer (inline mode's commits and
    /// the strip's cheap preview).
    pub(crate) render: ui::StreamRender,
    /// The same, for an open agent session view's own transcript.
    pub(crate) agent_render: ui::StreamRender,
    /// The Ctrl+O overlay's incrementally-built transcript.
    pub(crate) transcript: ui::TranscriptCache,
    /// The Ctrl+D overlay's built context window.
    pub(crate) context: ui::ContextCache,
    /// The screen-owning frame's finished history rows ([`FullTail`]).
    pub(crate) full_tail: Option<FullTail>,
    /// The scroll anchor: the tail line shown at the top, or `None` to follow
    /// the tail.
    pub(crate) full_top: Option<usize>,
    pub(crate) full_tail_dirty: bool,
    pub(crate) full_tab_hits: ui::SessionBarHits,
    pub(crate) full_tab_press: Option<Option<String>>,
    pub(crate) full_reading: std::collections::HashMap<Option<String>, FullReading>,
    /// The painted tail's length and conversation-window height.
    pub(crate) full_len: usize,
    pub(crate) full_conv_h: usize,
    /// This frame's streaming rows, appended after [`full_tail`](Self::full_tail).
    pub(crate) full_stream_lines: Vec<ratatui::text::Line<'static>>,
    /// The conversation rows exactly as the last frame painted them.
    pub(crate) full_window: Vec<ratatui::text::Line<'static>>,
    pub(crate) full_hover: Option<(u16, u16)>,
    pub(crate) full_press: Option<(usize, u16)>,
    pub(crate) full_select: Option<((usize, u16), (usize, u16))>,
    pub(crate) full_bar_drag: Option<(u16, usize)>,
    pub(crate) full_start: usize,
    pub(crate) full_expanded: std::collections::HashSet<usize>,
    pub(crate) full_items: Vec<(usize, usize, usize)>,
    pub(crate) full_anchor: Option<(usize, usize)>,
    /// Link preview cards by URL (`docs/ui/rich-links.md`).
    pub(crate) cards: std::collections::HashMap<String, zeroth::link_card::LinkCard>,
    /// URLs already handed to a fetch this session.
    pub(crate) card_pending: std::collections::HashSet<String>,
    pub(crate) full_prompts: Vec<ui::UserPrompt>,
    pub(crate) full_sticky_rows: usize,
    pub(crate) full_pin_row: Option<usize>,
    pub(crate) full_pin_hover: bool,
    pub(crate) full_pin_press: bool,
}

/// Rebuild the screen-owning frame's **history** rows into `view` when its
/// cache is stale — the free half of [`Session::full_tail_lines`], taking the
/// app, the view and a width so the cache can be exercised without a terminal
/// (and, later, without a `Session`). It also stores the pin/hit offsets the
/// frame reads beside the rows.
fn frame_history(
    app: &App,
    view: &mut ConversationView,
    width: u16,
    screen_width: u16,
    link_cards: bool,
    card_tx: &tokio::sync::mpsc::UnboundedSender<(String, Option<zeroth::link_card::LinkCard>)>,
) {
    let theme = ui::active_theme();
    let agent_id = app.viewed_agent().map(|run| run.id.as_str());
    let viewing_agent = agent_id.is_some();
    view.full_reading
        .retain(|id, _| id.as_deref().is_none_or(|id| app.agent(id).is_some()));
    let generation = if viewing_agent {
        (app.agents_generation(), 0)
    } else {
        (app.history_generation(), app.history.len())
    };
    // A different conversation owns a different scroll anchor and expanded
    // cells. Main-history replacement invalidates only the main indices.
    if let Some(previous) = view
        .full_tail
        .as_ref()
        .map(|tail| (tail.agent.clone(), tail.generation, tail.width, tail.theme))
    {
        let (previous, g, _, _) = previous;
        let switched = previous.as_deref() != agent_id;
        if switched {
            view.full_reading.insert(
                previous.clone(),
                FullReading {
                    top: view.full_top.take(),
                    expanded: std::mem::take(&mut view.full_expanded),
                    generation: g.0,
                },
            );
            if let Some(saved) = view.full_reading.remove(&app.agent_view)
                && (viewing_agent || saved.generation == generation.0)
            {
                view.full_top = saved.top;
                view.full_expanded = saved.expanded;
            }
        } else if !viewing_agent && g.0 != generation.0 {
            view.full_top = None;
            view.full_expanded.clear();
        }
        if switched || (!viewing_agent && g.0 != generation.0) {
            view.full_hover = None;
            view.full_press = None;
            view.full_select = None;
            view.full_bar_drag = None;
            view.full_items.clear();
            view.full_anchor = None;
        }
    }
    let history_cached = view.full_tail.as_ref().is_some_and(|tail| {
        tail.agent.as_deref() == agent_id
            && tail.generation == generation
            && tail.width == width
            && tail.theme == theme
            && !view.full_tail_dirty
    });
    if history_cached {
        return;
    }
    // This frame scrolls its conversation under whatever is painted on it,
    // and a terminal picture is anchored to the screen's cells rather than
    // to the buffer above: leaving one in a scrolling row is the reported
    // "noise after scrolling" in `docs/ui/full-screen.md`. Every row built
    // below is therefore pictureless — the `read` cell's fact line is the
    // text stand-in, the link card keeps its head and description — while
    // the sidebar's fixed-column art and the Ctrl+O pager, built outside
    // this scope, keep theirs.
    let _scrolling = images::scrolling_frame();
    // Lone URL lines in assistant replies become preview cards once
    // fetched (`docs/ui/rich-links.md`). The closures capture the fields
    // directly, so the renderer's mutable borrows (`agent_render`) below
    // stay disjoint; the URLs that still need a fetch are collected and
    // enqueued after the build.
    let cards = &view.cards;
    let missing_cards = std::cell::RefCell::new(Vec::new());
    let card_line = |line: &Line<'static>| -> Option<Vec<Line<'static>>> {
        if !link_cards {
            return None;
        }
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        let trimmed = text.trim();
        // The assistant bullet prefixes the message's first row; a link on
        // it is still a lone link.
        let url = trimmed.strip_prefix("● ").unwrap_or(trimmed).trim();
        if !zeroth::links::is_whole_url(url) {
            return None;
        }
        if let Some(card) = cards.get(url) {
            return Some(ui::link_card_lines(card, url, width));
        }
        missing_cards.borrow_mut().push(url.to_string());
        None
    };
    let card_for = |item: &HistoryItem, line: &Line<'static>| match item {
        HistoryItem::Message(message) if message.role == Role::Assistant => card_line(line),
        _ => None,
    };
    let (mut lines, mut prompts, mut items) = match app.viewed_agent() {
        Some(run) => {
            // An agent's session view keeps a cell per call: its cells are
            // keyed on the records its own run makes
            // (`docs/agents/agent-view-streaming.md`).
            let history = ui::committed_history(&run.history, &run.tool_queue, None).to_vec();
            recheck_pictures(&history);
            let mut prompts = Vec::new();
            let mut items = Vec::new();
            let expanded = &view.full_expanded;
            let mut lines = ui::conversation_lines_carded(
                &history,
                width,
                app.path_display(),
                true,
                |index| expanded.contains(&index),
                card_for,
                |index, start, count| {
                    if let HistoryItem::Message(message) = &history[index]
                        && message.role == Role::User
                    {
                        prompts.push(ui::UserPrompt {
                            start,
                            end: start + count,
                            text: message.text.clone(),
                        });
                    }
                    if ui::is_expandable(&history[index]) {
                        items.push((start, count, index));
                    }
                },
            );
            if let Some(text) = run.streaming.as_deref().filter(|text| !text.is_empty()) {
                let start = lines.len();
                lines.extend(view.agent_render.committed_rows(text, width));
                for line in lines.split_off(start) {
                    if let Some(card) = card_line(&line) {
                        lines.extend(card);
                    } else {
                        lines.push(line);
                    }
                }
            }
            (lines, prompts, items)
        }
        None => {
            let mut prompts = Vec::new();
            let mut items = Vec::new();
            let expanded = &view.full_expanded;
            let mut lines = ui::conversation_lines_carded(
                &app.history,
                width,
                app.path_display(),
                app.settings().verbose,
                |index| expanded.contains(&index),
                card_for,
                |index, start, count| {
                    if let HistoryItem::Message(message) = &app.history[index]
                        && message.role == Role::User
                    {
                        prompts.push(ui::UserPrompt {
                            start,
                            end: start + count,
                            text: message.text.clone(),
                        });
                    }
                    if ui::is_expandable(&app.history[index]) {
                        items.push((start, count, index));
                    }
                },
            );
            if let Some(text) = app.streaming_text().filter(|text| !text.is_empty()) {
                let start = lines.len();
                lines.extend(ui::message_lines_with_paths(
                    Role::Assistant,
                    text,
                    width,
                    Some(app.path_display()),
                ));
                for line in lines.split_off(start) {
                    if let Some(card) = card_line(&line) {
                        lines.extend(card);
                    } else {
                        lines.push(line);
                    }
                }
            }
            (lines, prompts, items)
        }
    };
    // Fetch the lone links that have no card yet; a URL is requested once
    // per session (a failed fetch simply keeps its line).
    let missing: Vec<String> = missing_cards.borrow_mut().drain(..).collect();
    for url in missing {
        // A reply with a wall of links must not spawn a thread per URL.
        if view.card_pending.len() >= 8 {
            break;
        }
        if view.card_pending.insert(url.clone()) {
            super::link_cards::spawn(url, card_tx.clone());
        }
    }
    // The banner is the inline mode's brand block. In the screen-owning
    // mode the sidebar carries it (`docs/ui/sidebar.md`), so it is left out
    // rather than printed twice — and its rows are then zero.
    // …and it is the **plain** banner either way: a graphic mascot is
    // anchored to the screen's cells, and this frame scrolls and repaints
    // under it, which leaves the image behind as ghost blocks wherever it
    // used to sit (`ui::header_lines_plain`).
    let banner = if ui::sidebar_columns(app, screen_width) > 0 {
        Vec::new()
    } else {
        ui::header_lines_plain(app, width)
    };
    // `banner_tail` puts the banner and one blank row above the
    // conversation; the user-message and item offsets follow it.
    let offset = banner.len() + usize::from(!banner.is_empty());
    for prompt in &mut prompts {
        prompt.start += offset;
        prompt.end += offset;
    }
    for (start, _, _) in &mut items {
        *start += offset;
    }
    lines = ui::banner_tail(banner, lines);
    view.full_prompts = prompts;
    view.full_items = items;
    view.full_tail_dirty = false;
    view.full_tail = Some(FullTail {
        agent: app.agent_view.clone(),
        generation,
        width,
        theme,
        lines,
    });
}

/// How often the live status re-arms its next animation frame while a turn is in
/// flight (~30 fps — codex's status-widget cadence): drives the verb's shimmer
/// sweep and keeps the timer advancing through event-less pauses.
pub(crate) const STATUS_FRAME_INTERVAL: std::time::Duration = std::time::Duration::from_millis(32);

/// How often an open `/login` device page redraws — the granularity of the
/// `mm:ss` countdown that is its only moving part. See `docs/providers/copilot.md`.
const DEVICE_TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Upper bound on the rows a purge rebuild (`/clear`, resize, a history
/// rewind) re-renders into scrollback. A purge drops the terminal's own
/// scrollback, so the whole conversation is rebuilt from history — capped here
/// so a pathologically long conversation can't turn one resize into an
/// unbounded write. codex bounds its resize reflow the same way (per-terminal
/// 1k–10k rows); 10k is effectively "everything" for any real conversation.
pub(crate) const RESIZE_REFLOW_MAX_ROWS: usize = 10_000;

impl Session<'_> {
    /// The live region's height for the current state and screen size — exactly
    /// what the next [`Session::draw_conversation`] will use. Shared so a
    /// post-stream commit can reserve that same idle height before flushing the
    /// final lines (see `InlineViewport::set_view_height`).
    pub(crate) fn live_region_height(&self) -> u16 {
        live_region_height(&self.app, self.screen.screen())
    }

    /// Write the live status's times onto `App` before a draw: how long the turn
    /// has run (whole seconds for display; sub-second for the shimmer phase) and
    /// the current thinking-phase duration (`Some` while thinking). Time is
    /// impure, so this is the boundary's job — the pure `App`/`ui` only ever see
    /// the already-computed values. No-op when no turn is in flight.
    pub(crate) fn update_status_times(&mut self) {
        let elapsed = self
            .clocks
            .turn_start
            .map_or(std::time::Duration::ZERO, |start| start.elapsed());
        let thinking = self.clocks.thinking_start.map(|start| start.elapsed());
        self.app.set_status_times(elapsed, thinking);
        // The current running command's own elapsed (None when none is
        // running): the `(Ns)` its cell displays (docs/ui/tool-streaming.md) and,
        // masked, the delayed Ctrl+B hint's gate (docs/tools/background.md).
        self.app
            .set_command_elapsed(self.clocks.command_start.map(|start| start.elapsed()));
        // The animation phase for the live region's pulsing bullets — a phase,
        // not a measurement: nothing displays it (docs/ui/tool-pulse.md).
        let now = self.clocks.loop_start.elapsed();
        self.app.set_pulse(now);
        // The open read/search group's `⎿` row steps through its calls on
        // the same clock, each value held long enough to read
        // (docs/ui/tool-groups.md *The hint row*).
        self.app.advance_group_hint(now);
        // The `/usage` page's countdowns run on the wall clock
        // (docs/providers/usage.md).
        if self.app.usage_page.is_some() {
            self.app.set_usage_now(super::host::unix_secs_signed());
        }
        // How long a `/login` device code has left, when one is on screen
        // (docs/providers/copilot.md).
        if self.app.device_login_active() {
            let now = Instant::now();
            self.app.set_device_remaining(
                self.device_expires
                    .map(|at| at.saturating_duration_since(now)),
            );
        }
    }

    /// Schedule the redraw for a just-handled key. A plain typed character that
    /// lands in a `PasteBurst` asks for a *relaxed* frame a beat out instead of
    /// an immediate one; **the burst detector is consulted first**, so a paste's
    /// characters can never take the urgent path and saturate the pty. A key
    /// that edits the composer then gets an urgent frame — it uses its own 2 ms
    /// floor and pulls an already-queued animation frame earlier, so a streaming
    /// reply cannot hold a keystroke for a whole 8.33 ms beat. Every other key
    /// (navigation, shortcuts, the first characters of a run) keeps the 120 fps
    /// floor that coalesces redraws (`docs/architecture/input-latency.md`,
    /// `zeroth::paste`).
    pub(crate) fn schedule_for_key(&mut self, key: &KeyEvent) {
        let plain_char =
            matches!(key.code, KeyCode::Char(_)) && !key.modifiers.contains(KeyModifiers::CONTROL);
        if !plain_char {
            self.burst.reset(); // a navigation/submit key ends any burst
        }
        if plain_char && self.burst.note_char(std::time::Instant::now()) {
            self.frame.schedule_frame_in(paste::BURST_CHAR_INTERVAL);
        } else if edits_composer(key) {
            self.frame.schedule_frame_urgent();
        } else {
            self.frame.schedule_frame();
        }
    }

    /// The framed-view rows that must be **flowed** into scrollback for the
    /// current state — a screen-tall `/hooks` page's top —
    /// or `None` when nothing flows (`ui::view_flow`, `docs/ui/view-flow.md`).
    /// Capped like every purge-rebuild write.
    pub(crate) fn current_view_flow(&self) -> Option<ui::ViewFlow> {
        let screen = self.screen.screen();
        ui::view_flow(
            &self.app,
            screen.width,
            screen.height,
            RESIZE_REFLOW_MAX_ROWS,
        )
    }

    /// Whether the flow committed to scrollback no longer matches the current
    /// state — the page navigated, the search narrowed, the menu closed, a
    /// modal covered it — so this draw tick must purge-rebuild instead of
    /// painting in place: the rebuild re-establishes the current flow, or
    /// clears it and returns the composer (`docs/ui/view-flow.md`). Cheap when
    /// no framed view is open and nothing is flowed (both sides `None`).
    pub(crate) fn view_flow_stale(&self) -> bool {
        let screen = self.screen.screen();
        ui::view_flow_signature(
            &self.app,
            screen.width,
            screen.height,
            RESIZE_REFLOW_MAX_ROWS,
        ) != self.flowed_view
    }

    /// Whether this draw tick must purge-rebuild the conversation instead of
    /// painting the live region in place — the boundary read behind the pure
    /// `ui::modal_needs_rebuild`: the just-closed prompt's noted one-way move,
    /// or the still-open prompt about to seat short of the screen bottom it was
    /// painted flush against (a batch's back-to-back prompts of different
    /// heights — see the draw tick's rebuild arm and `docs/state/permissions.md`).
    pub(crate) fn modal_rebuild_due(&self) -> bool {
        ui::modal_needs_rebuild(
            ui::region_is_modal(&self.app),
            self.screen.modal_scrolled(),
            self.screen.painted_bottom(),
            self.screen.view_top(),
            self.screen.pending_rows(),
            self.live_region_height(),
            self.screen.screen().height,
        )
    }

    /// The strip's streaming preview: **the rows scrollback does not hold
    /// yet** — the uncommitted tail of the render, which is the reply's last
    /// row for settled prose, every wrapped row of a withheld source line, the
    /// whole forming table — never fewer rows than a token ago less what
    /// committed since, padded with blanks so the box holds still,
    /// `docs/ui/slow-stream.md` (capped to `ui::stream_preview_max_rows` so a tall tail
    /// tail-follows on a small screen) — computed cheaply by
    /// `ui::StreamRender::preview`; `None` when idle or while a tool runs (the
    /// tool's own header previews instead).
    /// Called before every conversation-view draw so the status animation never
    /// pays to re-render the whole reply, and **injects the row count into
    /// `App`** (`App::set_stream_preview_rows`) so `ui::preview_rows` — and with
    /// it [`Session::live_region_height`], the strip layout, and the cursor seat
    /// — reserve exactly the rows the strip draws. See `docs/ui/markdown.md`,
    /// `docs/ui/table-streaming.md`.
    ///
    /// **Whose reply** is the view's question, not the loop's: with an agent
    /// session view open the screen shows that agent's conversation, so the
    /// frontier comes from its buffer through `agent_render` — the very
    /// render `Session::commit_agent_view_event` commits with, which is what
    /// makes `committed ++ preview` the reply there too. Previewing the main
    /// buffer instead (or, as before, not at all) left a subagent's forming
    /// table on screen nowhere: withheld by the commit, one row of it in the
    /// strip (`docs/agents/agent-view-streaming.md`).
    fn stream_preview_lines(&mut self) -> Option<Vec<Line<'static>>> {
        let screen = self.screen.screen();
        let max_rows = ui::stream_preview_max_rows(&self.app, screen.width, screen.height);
        let preview = match self.app.viewed_agent() {
            Some(run) => run
                .streaming
                .clone()
                .filter(|text| !text.is_empty() && run.tool_queue.is_empty())
                .map(|text| {
                    self.view
                        .agent_render
                        .preview(&text, screen.width, max_rows)
                }),
            None => match self.app.streaming_text() {
                Some(text) if !text.is_empty() && self.app.current_tool().is_none() => {
                    Some(self.view.render.preview(text, screen.width, max_rows))
                }
                _ => None,
            },
        };
        self.app.set_stream_preview_rows(
            preview
                .as_ref()
                .map_or(1, |p| u16::try_from(p.len()).unwrap_or(u16::MAX)),
        );
        preview
    }

    /// Render the live region at its current grown height and place the cursor.
    /// The composer keeps its cursor even while a reply streams (codex-style —
    /// typing mid-turn edits the draft, Enter queues it); only the overlays hide
    /// it (`enter_overlay`).
    ///
    /// A permission prompt needs no branch of its own: the height is its
    /// `ui::permission_height` and `render_live` paints it in the composer's
    /// place, so the region simply grows to the prompt like any other view — the
    /// chat above it scrolling into real scrollback (`InlineViewport` notes that
    /// one-way move for the close's purge rebuild; `docs/state/permissions.md`).
    pub(crate) fn draw_conversation(&mut self) -> io::Result<()> {
        // Compute the strip preview cheaply once per frame (O(one line); a
        // forming table re-renders just its own block), so the status animation
        // never re-renders the whole reply — and inject its row count so the
        // layout reserves it. See `docs/ui/markdown.md`, `docs/ui/table-streaming.md`.
        //
        // The screen-owning mode already paints the streaming reply as part of
        // the conversation (`full_tail_lines`), so its strip previews nothing:
        // an empty preview with a zero-row budget, not the `None` fallback
        // (which renders the last line again).
        let full = self.screen.is_full();
        let preview = if full {
            self.app.set_stream_preview_rows(0);
            Some(Vec::new())
        } else {
            self.stream_preview_lines()
        };
        let height = self.live_region_height();
        // The screen-owning mode paints the conversation and the live region
        // into one frame (`docs/ui/full-screen.md`); the inline mode keeps its
        // bottom-anchored region and scrollback commits.
        if full {
            let tail = self.full_tail_lines();
            let screen = self.screen.screen();
            let live_h = usize::from(height.min(screen.height));
            let conv_area = usize::from(screen.height).saturating_sub(live_h);
            // A click-to-expand rebuild pins the toggled item's first line to
            // the screen row it occupied, so the cell does not jump under the
            // pointer (`docs/ui/full-screen.md`).
            if let Some((line, screen_row)) = self.view.full_anchor.take() {
                let max_top = tail.len().saturating_sub(conv_area);
                let top = line.saturating_sub(screen_row).min(max_top);
                self.view.full_top = (top < max_top).then_some(top);
            }
            let window_start = |content_h: usize| {
                let max_top = tail.len().saturating_sub(content_h);
                self.view.full_top.map_or(max_top, |top| top.min(max_top))
            };
            let mut content_h = conv_area;
            let mut start = window_start(content_h);
            // The turn the window's top sits in is pinned on one reserved row
            // while its whole message has scrolled away, so the question stays
            // with its answer (`docs/ui/conversation-tool-details-and-pin.md`).
            // The pinned strip is conversation: it spans the rows' width, not
            // the screen's — the sidebar and the bar are outside it.
            let width = usize::from(self.full_text_width());
            let mut pin = pinned_row(
                &self.view.full_prompts,
                start,
                width,
                self.view.full_pin_hover,
            );
            if pin.is_some() && content_h > 1 {
                content_h -= 1;
                start = window_start(content_h);
                pin = pinned_row(
                    &self.view.full_prompts,
                    start,
                    width,
                    self.view.full_pin_hover,
                );
            } else if content_h <= 1 {
                pin = None;
            }
            // The scrollbar: its cell for every row of the conversation,
            // prepared here so `term` stays a painter (`docs/ui/full-screen.md`).
            let bar = ui::scrollbar_rows(
                tail.len(),
                content_h,
                start,
                u16::try_from(content_h).unwrap_or(u16::MAX),
            )
            .unwrap_or_default();
            let end = (start + content_h).min(tail.len());
            let mut window: Vec<Line<'static>> = tail[start..end].to_vec();
            self.view.full_start = start;
            self.apply_hover(&mut window);
            self.apply_selection(&mut window);
            self.view.full_len = tail.len();
            self.view.full_conv_h = content_h;
            self.view.full_sticky_rows = usize::from(pin.is_some());
            self.view.full_pin_row = pin.as_ref().map(|pin| pin.start);
            if pin.is_none() {
                // A strip that went away takes its hover with it: the pointer
                // is now over conversation rows.
                self.view.full_pin_hover = false;
            }
            // The hit map is the content window the user can see — the same
            // rows the viewport paints below the pinned row (`full_window`).
            self.view.full_window = window.clone();
            let mut painted = Vec::with_capacity(window.len() + self.view.full_sticky_rows);
            if let Some(pin) = pin {
                painted.push(pin.line);
            }
            painted.extend(window);
            let app = &self.app;
            let mut hits = ui::SessionBarHits::default();
            self.screen.draw_frame(
                &painted,
                height,
                &bar,
                |area, buf| {
                    hits = ui::render_workspace_live(area, buf, app, preview.as_deref());
                },
                app,
            )?;
            self.view.full_tab_hits = hits;
            return Ok(());
        }
        let app = &self.app;
        // `term` places the cursor from the final (content-anchored) viewport
        // via `ui::cursor_position`, which mirrors render_live's layout exactly.
        self.screen.draw_region(
            height,
            |area, buf| ui::render_live_with_preview(area, buf, app, preview.as_deref()),
            app,
        )
    }

    /// The startup banner for a purge rebuild: the graphic mascot inline (its
    /// rows are committed to the terminal's scrollback once, where a placement
    /// is safe), the plain text badge in the screen-owning mode, whose frame
    /// scrolls and repaints under the image and would leave it behind
    /// (`ui::header_lines_plain`).
    fn banner_lines(&self, width: u16) -> Vec<Line<'static>> {
        if self.screen.is_full() {
            ui::header_lines_plain(&self.app, width)
        } else {
            ui::header_lines(&self.app, width)
        }
    }

    /// The columns the screen-owning conversation may paint into: the screen
    /// minus the sidebar's column and the scrollbar's, floored at one.
    ///
    /// The sidebar decides this before the rows are built, because the wrap
    /// width is part of the frame cache's key — a column appearing without a
    /// rebuild would re-wrap every row under the reader
    /// (`docs/ui/sidebar.md`).
    fn full_text_width(&self) -> u16 {
        let screen = self.screen.screen().width;
        screen
            .saturating_sub(ui::sidebar_columns(&self.app, screen))
            .saturating_sub(ui::SCROLLBAR_COLUMNS)
            .max(1)
    }

    /// The screen-owning mode's conversation rows: the startup banner, the
    /// finished history, the in-flight reply, and — inside an agent session
    /// view — that agent's own transcript.
    ///
    /// Two halves, and the split is what keeps a big session usable. The
    /// **history** rows go through [`FullTail`]'s cache, keyed on anything
    /// that changes what those rows are; the **reply** that is still streaming
    /// is appended here, per frame. One cache over both, keyed on the streamed
    /// text's length, re-rendered the whole session on every chunk — 12.8 ms
    /// at 1,000 turns, 77% of a 60 fps frame, per chunk
    /// (`docs/ui/full-screen.md` *Performance*).
    fn full_tail_lines(&mut self) -> Vec<ratatui::text::Line<'static>> {
        let width = self.full_text_width();
        frame_history(
            &self.app,
            &mut self.view,
            width,
            self.screen.screen().width,
            self.link_cards,
            &self.card_tx,
        );
        self.tailed_with_stream(width)
    }

    /// The cached history rows with this frame's **in-flight reply** and any
    /// flowed page appended.
    ///
    /// This is the per-chunk half: it renders only what is still streaming, so
    /// its cost is the reply's size, not the session's. The history rows are
    /// cloned (a `Vec<Line>` share of the cached copy) and the reply is built
    /// on top of them, because the frame paints the tail as one slice.
    fn tailed_with_stream(&mut self, width: u16) -> Vec<ratatui::text::Line<'static>> {
        let mut lines = self
            .view
            .full_tail
            .as_ref()
            .map(|tail| tail.lines.clone())
            .unwrap_or_default();
        let streaming = self
            .app
            .viewed_agent()
            .and_then(|run| run.streaming.as_deref().filter(|text| !text.is_empty()))
            .map(str::to_string);
        let streaming = match streaming {
            Some(text) => Some(text),
            None => {
                if self.app.viewed_agent().is_none() {
                    self.app
                        .streaming_text()
                        .filter(|text| !text.is_empty())
                        .map(str::to_string)
                } else {
                    None
                }
            }
        };
        let mut stream = Vec::new();
        if let Some(text) = streaming {
            stream.extend(if self.app.viewed_agent().is_some() {
                self.view.agent_render.committed_rows(&text, width)
            } else {
                ui::message_lines_with_paths(
                    Role::Assistant,
                    &text,
                    width,
                    Some(self.app.path_display()),
                )
            });
        }
        // The screen-owning mode's live cells ride the conversation, not the
        // strip (`ui::conversation_live_lines`): a running tool's tail and an
        // open thinking phase grow in the transcript, in the turn's own order,
        // and scroll with it — the strip above the composer keeps only the
        // status line and its tip. Empty inline and in an agent session view,
        // whose strip keeps its preview.
        stream.extend(ui::conversation_live_lines(&self.app, width));
        // A framed view opened over this screen flows its page the same way
        // the inline rebuild does (`docs/ui/view-flow.md`).
        let flow = self.current_view_flow();
        self.flowed_view = flow.as_ref().map(|flow| flow.signature);
        if let Some(flow) = flow {
            stream.extend(flow.lines);
        }
        // Kept for `full_selected_text`, which indexes the whole tail: the
        // history half is borrowed from the cache, this half is already here.
        self.view.full_stream_lines = stream.clone();
        lines.extend(stream);
        lines
    }

    /// Underline the link run under the pointer: the link spans on the hovered
    /// row that carry the same interned URL id. A terminal that supports OSC 8
    /// hover-underlines by itself; in the screen-owning mode the app draws the
    /// hover, because the pointer is ours (`docs/ui/full-screen.md`).
    fn apply_hover(&self, lines: &mut [Line<'static>]) {
        let Some((col, row)) = self.view.full_hover else {
            return;
        };
        let Some(line) = lines.get_mut(usize::from(row)) else {
            return;
        };
        let Some(id) = carrier_at(line, col) else {
            return;
        };
        for span in &mut line.spans {
            if span.style.underline_color.and_then(links::carrier_id) == Some(id) {
                span.style = span
                    .style
                    .add_modifier(ratatui::style::Modifier::UNDERLINED);
            }
        }
    }

    /// Reverse the cells covered by the drag selection, row by row — the
    /// screen-owning mode's own selection, because mouse capture takes the
    /// terminal's (`docs/ui/full-screen.md`).
    fn apply_selection(&self, lines: &mut [Line<'static>]) {
        let Some((a, b)) = self.view.full_select else {
            return;
        };
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        for (offset, line) in lines.iter_mut().enumerate() {
            let row = self.view.full_start + offset;
            if row < start.0 || row > end.0 {
                continue;
            }
            let from = if row == start.0 { start.1 } else { 0 };
            let to = if row == end.0 {
                end.1.saturating_add(1)
            } else {
                u16::MAX
            };
            // Exactly the cells the copy will take: `ui::highlight` splits the
            // spans at the selection's edges and reverses whole graphemes, the
            // same test `ui::slice` copies with
            // (`docs/ui/full-screen.md` *Selection semantics*).
            *line = ui::highlight(line, from, to);
        }
    }

    /// The selected text, from the **tail** the selection anchors index —
    /// joined with newlines, trailing blanks trimmed, gutter and all (the
    /// inline view's decoration rule comes later). `None` when nothing is
    /// selected or the selection covers only blanks.
    pub(crate) fn full_selected_text(&self) -> Option<String> {
        let ((a_line, a_col), (b_line, b_col)) = self.view.full_select?;
        let (start, end) = if (a_line, a_col) <= (b_line, b_col) {
            ((a_line, a_col), (b_line, b_col))
        } else {
            ((b_line, b_col), (a_line, a_col))
        };
        // The tail is two halves: the cached history rows, then this frame's
        // streaming ones. Selection indexes the whole thing, so walk both in
        // order — no need to have copied the history to complete it.
        let history = self.view.full_tail.as_ref()?.lines.iter();
        ui::selection::text(
            history.chain(self.view.full_stream_lines.iter()),
            start,
            end,
        )
    }

    /// Render the full-screen tool-output overlay. Clamps the scroll to the
    /// current screen first (so the last line can reach the bottom but not
    /// scroll past it), then paints the view onto the alternate screen.
    pub(crate) fn draw_tool_view(&mut self) -> io::Result<()> {
        let screen = self.screen.screen();
        // An agent session view's Ctrl+O shows the *viewed agent's* transcript —
        // a fresh, bounded build (docs/agents/agent-tool.md); the main cache below is
        // untouched, so the ordinary open stays warm.
        if let Some(lines) = ui::agent_transcript_lines(&self.app, screen.width) {
            let max = ui::tool_view_max_scroll_for(lines.len(), screen.height);
            self.app.settle_tool_scroll(max);
            let app = &self.app;
            return self
                .screen
                .draw_overlay(|area, buf| ui::render_tool_view(area, buf, app, &lines));
        }
        // A backtrack preview open/step requested a scroll to its highlighted
        // message (docs/input/backtrack.md): apply the pure decision once — consumed,
        // so it never fights the user's own scrolling — before the normal clamp.
        if self.app.take_backtrack_scroll() {
            let selection = self.view.transcript.selection(&self.app, screen.width);
            if let Some(scroll) =
                ui::backtrack_scroll_for(selection, self.app.tool_scroll, screen.height)
            {
                self.app.apply_backtrack_scroll(scroll);
            }
        }
        // Build the transcript at most once here (the cache skips even that
        // while the user only scrolls): the same cached lines feed the clamp and
        // the render, so a scroll keypress no longer re-highlights all of
        // history (twice).
        let max = ui::tool_view_max_scroll_for(
            self.view.transcript.line_count(&self.app, screen.width),
            screen.height,
        );
        self.app.settle_tool_scroll(max);
        let app = &self.app;
        let transcript = &mut self.view.transcript;
        self.screen
            .draw_overlay(|area, buf| transcript.render(area, buf, app))
    }

    /// Render the full-screen `/resume` session picker onto the alternate screen
    /// (the transcript overlay's twin). See `docs/state/resume.md`.
    pub(crate) fn draw_resume_picker(&mut self) -> io::Result<()> {
        let app = &self.app;
        self.screen
            .draw_overlay(|area, buf| ui::render_resume_picker(area, buf, app))
    }

    /// Render the Ctrl+D context-debug view onto the alternate screen — the
    /// transcript pager's raw-context sibling: resolve a pending ←/→ step and
    /// settle the scroll against the current screen, then paint. The page
    /// comes from the loop-owned [`ui::ContextCache`] — the view redraws every
    /// animation frame while a turn runs, and rebuilding the O(conversation)
    /// derivation per frame starved the scroll keys on a big context (the
    /// "Ctrl+D freezes" bug) — so the clamp, the step and the render share
    /// one build. See `docs/architecture/context.md`.
    pub(crate) fn draw_context_view(&mut self) -> io::Result<()> {
        // Tab's other page is a different body under the same chrome, and it
        // is built fresh from the backend rather than served from the cache
        // (`docs/state/permissions.md`).
        if self.app.debug_page == DebugPage::Classifier {
            return self.draw_classifier_page();
        }
        let screen = self.screen.screen();
        let body = self.view.context.body(&self.app, screen.width);
        let max = ui::tool_view_max_scroll_for(body.lines.len(), screen.height);
        // A ←/→ press lands against the page as built — its width, its
        // wrapping — before the settle clamps it.
        self.app.resolve_debug_jump(&body.headers, max);
        self.app.settle_debug_scroll(max);
        let app = &self.app;
        self.screen
            .draw_overlay(|area, buf| ui::render_context_view(area, buf, app, body))
    }

    /// Render the Ctrl+D view's **classifier page** (Tab) onto the alternate
    /// screen — the same chrome, a different body (`docs/state/permissions.md`).
    ///
    /// The block is pulled from the **backend** per draw rather than cached:
    /// it grows as the turn runs (every tool call appends a line) and rolls
    /// its windows as the conversation goes on, so a page left open must show
    /// the log as it lands. Cheap by construction — the context is bounded to
    /// `CONTEXT_MAX_REQUESTS` + `CONTEXT_MAX_ACTIONS` short lines, which is
    /// why this needs no `ContextCache` sibling.
    fn draw_classifier_page(&mut self) -> io::Result<()> {
        // Whose review log? An open agent session view debugs the **viewed
        // agent's** run, so its classifier page must show that agent's own
        // context — its launch prompt and its own calls, the block its own
        // verdicts were actually reviewed against. The lead's log describes
        // decisions nobody on that screen made. (The other Ctrl+D page has
        // always swapped this way — `ui::context_lines`' first branch keys on
        // `viewed_agent` — this one simply never did.) An agent the registry
        // no longer holds yields `None`, which renders as the page's empty
        // placeholder rather than silently falling back to the lead.
        // See `docs/state/permissions.md` / `docs/agents/agent-tool.md`.
        let context = match self.app.agent_view.as_deref() {
            Some(id) => self.agent_registry.classifier_context(id),
            None => self.models.backend().classifier_context(),
        };
        self.app.set_classifier_context(context);
        let screen = self.screen.screen();
        let body = ui::classifier_body(&self.app, screen.width);
        let max = ui::tool_view_max_scroll_for(body.lines.len(), screen.height);
        self.app.resolve_debug_jump(&body.headers, max);
        self.app.settle_classifier_scroll(max);
        let app = &self.app;
        self.screen
            .draw_overlay(|area, buf| ui::render_context_view(area, buf, app, &body))
    }

    /// Paint whichever view is current — the draw tick's whole body.
    pub(crate) fn draw_active_view(&mut self) -> io::Result<()> {
        match self.app.view {
            // The modal (permission-prompt) region needs a purge rebuild
            // instead of an in-place paint — two cases, one pure decision
            // (`ui::modal_needs_rebuild`, `docs/state/permissions.md`). The prompt
            // just CLOSED after the screen moved one-way under it — its own
            // growth scrolled chat into scrollback (the ordinary case), a commit
            // beneath it scrolled, or a rebuild reseated it (a mid-prompt
            // resize's purge, an overlay return whose prompt opened
            // underneath): the plain collapse cannot refill what moved. Or the
            // prompt is still OPEN and this frame would SHRINK the region seated
            // at the screen bottom — a batch's back-to-back prompts of different
            // heights (the tall body-capped one answered, a short one opening as
            // the resolved cell commits out of the live region), which used to
            // strand the open prompt above a band of blank rows until it was
            // answered. Purge-rebuild instead: region flush at the bottom,
            // scrollback rebuilt from history, nothing lost or doubled — and a
            // rebuild under a still-open prompt re-arms the note
            // (`InlineViewport::reflow`), so its eventual close still purges.
            // (An open agent session view rebuilds itself —
            // `repaint_active_view` routes there.) See
            // `InlineViewport::take_modal_scrolled` and `docs/state/permissions.md`.
            // …or a framed view's FLOW went stale — its page navigated, its
            // menu closed, a modal covered it, or a page that must flow has
            // no flow committed yet: the same purge rebuild re-establishes
            // the current flow, or clears it and returns the composer
            // (`docs/ui/view-flow.md`; the flow check runs first so a prompt
            // opening over a flowed menu cleans the flow in the very rebuild
            // that seats the prompt).
            View::Conversation if self.view_flow_stale() || self.modal_rebuild_due() => {
                let _ = self.screen.take_modal_scrolled();
                self.repaint_active_view()
            }
            View::Conversation => self.draw_conversation(),
            View::ToolOutput => self.draw_tool_view(),
            View::ResumePicker => self.draw_resume_picker(),
            View::DiffReview => self.draw_diff_review(),
            View::ContextDebug => self.draw_context_view(),
        }
    }

    /// Rebuild whatever the inline screen is showing — the **agent session
    /// view** when one is open (a purge rebuild of the agent's transcript),
    /// else the main conversation. The shared resize / rewind repaint
    /// (`docs/agents/agent-tool.md`).
    pub(crate) fn repaint_active_view(&mut self) -> io::Result<()> {
        if self.app.agent_view.is_some() {
            self.repaint_agent_view()
        } else {
            self.repaint_conversation()
        }
    }

    /// Catch the inline screen up when an alternate-screen overlay closes.
    ///
    /// Everything that committed while the overlay was up — streamed rows,
    /// tool cells, thought cells, queued user bubbles, the turn's summary —
    /// sits in the viewport's pending queue (`InlineViewport::insert_before`
    /// only queues, and nothing flushes onto the alternate screen), so the
    /// ordinary [`Session::draw_conversation`] flushes it above the live
    /// region and repaints the box, exactly as if the overlay had never been
    /// up: the terminal's own scrollback survives, and nothing is lost or
    /// doubled. The retired history-window rebuild could re-emit at most one
    /// screenful, which silently dropped the rest of an overlay-covered turn
    /// from the terminal — the scrollback-hole bug.
    ///
    /// Two cases still need the full purge rebuild: a **resize landed under
    /// the overlay** (the emulator reflowed the main screen and the queued
    /// lines were rendered at the stale width — `overlay_resized`, consumed
    /// here so only this return purges), and an open **agent session view**,
    /// which always rebuilds from its own transcript (`docs/agents/agent-tool.md`;
    /// its purge drops the queue, whose lines its history regenerates).
    pub(crate) fn overlay_return_repaint(&mut self) -> io::Result<()> {
        if std::mem::take(&mut self.overlay_resized) || self.app.agent_view.is_some() {
            self.repaint_active_view()
        } else {
            self.draw_conversation()
        }
    }

    /// Rebuild the inline conversation from `App`'s retained history,
    /// re-wrapped to the current width — scrollback purge, tail and live
    /// region in one synchronized frame (`InlineViewport::reflow`). Used
    /// after a resize, on `/clear`, and by every history rewind (a backtrack,
    /// a `/resume` load, an interrupt-undo); an ordinary overlay return
    /// instead just flushes its queued commits
    /// ([`Session::overlay_return_repaint`]).
    ///
    /// The purge drops the terminal's scrollback, so the **whole** history is
    /// repainted (bounded by [`RESIZE_REFLOW_MAX_ROWS`]) — otherwise older
    /// turns would be lost from the purged scrollback.
    ///
    /// A mid-stream rebuild must not lose the in-flight partial reply — it
    /// lives in `App`'s streaming buffer, not `history`. The purge dropped its
    /// committed rows with everything else, so the renderer resets and the
    /// whole partial is re-committed right after via `ui::StreamRender::commit`
    /// — the standard `insert_before` pipeline, landing in the next draw's
    /// synchronized update. (`reflow` cleared the pending queue first, so the
    /// re-commit can never double up with pre-rebuild leftovers.)
    pub(crate) fn repaint_conversation(&mut self) -> io::Result<()> {
        if self.screen.is_full() {
            return self.draw_conversation();
        }
        let screen = self.screen.screen();
        // The purge takes the encoded pictures with it, so a picture whose
        // file has gone since it was drawn — or since the conversation was
        // recorded — must not reserve its rows again (`docs/ui/images.md`).
        let held = self.app.tool_group_range().map(|range| range.start);
        recheck_pictures(ui::committed_history(
            &self.app.history,
            self.app.tool_queue(),
            held,
        ));
        // The purge drops every committed row (screen and scrollback alike),
        // so nothing is "already committed" any more: reset, and let the
        // catch-up below re-commit the whole partial at the current width.
        self.view.render.reset();
        // The preview comes FIRST: it injects the strip's preview row count
        // (`set_stream_preview_rows` — a forming table previews multi-row) that
        // `live_region_height` below must reserve (docs/ui/table-streaming.md).
        let preview = self.stream_preview_lines();
        let height = self.live_region_height();
        let tail = ui::repaint_tail(
            // Only what has actually reached scrollback: the open read/search
            // group has cells recorded but held, and the strip is showing
            // them (`docs/ui/tool-groups.md`).
            ui::committed_history(&self.app.history, self.app.tool_queue(), held),
            self.app.streaming_text(),
            &mut self.view.render,
            screen.width,
            RESIZE_REFLOW_MAX_ROWS,
            self.app.path_display(),
            self.app.settings().verbose,
        );
        // Re-emit the header banner atop the rebuilt tail — it lives outside
        // `history` and would otherwise be lost (docs/ui/header.md).
        let mut tail = ui::banner_tail(self.banner_lines(screen.width), tail);
        // A screen-tall framed view flows its page top into scrollback: the
        // rows ride the tail's very end, so they land directly above the live
        // region — the page reads whole across the seam. Recomputed here, at
        // the one place every purge rebuild goes through, so a resize or a
        // rewind under an open menu re-flows at the new state for free; the
        // stored signature is what the draw tick compares
        // (`docs/ui/view-flow.md`).
        let flow = self.current_view_flow();
        self.flowed_view = flow.as_ref().map(|flow| flow.signature);
        if let Some(flow) = flow {
            tail.extend(flow.lines);
        }
        let app = &self.app;
        self.screen.reflow(
            tail,
            height,
            |area, buf| ui::render_live_with_preview(area, buf, app, preview.as_deref()),
            app,
        )?;
        // Catch scrollback up on the in-flight partial the purge dropped:
        // exactly the rows live streaming would have committed, queued for
        // the next draw. Suppressed while a flow is active — the queued rows
        // would flush between the flowed page and the region, tearing it
        // (commits pause under a flow, `Session::commits_allowed`); the
        // partial stays in the streaming buffer and the first un-flowed
        // rebuild re-commits it whole.
        if self.flowed_view.is_none()
            && let Some(text) = self.app.streaming_text().filter(|text| !text.is_empty())
        {
            let lines = self.view.render.commit(text, screen.width);
            self.screen.insert_before(lines);
        }
        Ok(())
    }

    /// Rebuild the screen as an **agent session view** (`docs/agents/agent-tool.md`):
    /// purge scrollback + screen (the `/clear` shape — the main conversation
    /// returns the same way), then the banner over the agent's own transcript,
    /// with the live region (the agent's strip + the labelled composer + footer +
    /// roster) painted below in the same synchronized frame. The in-flight
    /// partial commits through `agent_render` so later chunks append seamlessly.
    pub(crate) fn repaint_agent_view(&mut self) -> io::Result<()> {
        if self.screen.is_full() {
            return self.draw_conversation();
        }
        let screen = self.screen.screen();
        self.view.agent_render.reset();
        let Some(run) = self.app.viewed_agent() else {
            return Ok(());
        };
        // Only the agent's committed cells, the main view's rule
        // (`ui::committed_history`) — and its pictures looked
        // for again, the main rebuild's rule.
        let history = ui::committed_history(&run.history, &run.tool_queue, None).to_vec();
        recheck_pictures(&history);
        let streaming = run.streaming.clone().filter(|text| !text.is_empty());
        // The preview comes FIRST, as in `repaint_conversation`: it injects
        // the strip's row count (`set_stream_preview_rows`) that
        // `live_region_height` below must reserve — a rebuild that skipped it
        // squeezed a multi-row frontier back to one row for that frame
        // (`docs/agents/agent-view-streaming.md`).
        let preview = self.stream_preview_lines();
        let height = self.live_region_height();
        // An agent's session view keeps a cell per call: its commits are
        // keyed on the records its own run makes (docs/ui/tool-groups.md).
        let mut tail =
            ui::conversation_lines(&history, screen.width, self.app.path_display(), true);
        if let Some(text) = &streaming {
            tail.extend(self.view.agent_render.committed_rows(text, screen.width));
        }
        let mut tail = ui::banner_tail(self.banner_lines(screen.width), tail);
        // A framed view opened from the agent view's own palette renders over
        // it and flows the same way (`docs/ui/view-flow.md`) — the rebuild is
        // this view's, so the flow rides this tail.
        let flow = self.current_view_flow();
        self.flowed_view = flow.as_ref().map(|flow| flow.signature);
        if let Some(flow) = flow {
            tail.extend(flow.lines);
        }
        let app = &self.app;
        self.screen.reflow(
            tail,
            height,
            |area, buf| ui::render_live_with_preview(area, buf, app, preview.as_deref()),
            app,
        )?;
        // Catch scrollback up on the agent's in-flight partial the purge
        // dropped — `repaint_conversation`'s catch-up over `agent_render`, so
        // a rebuild mid-reply leaves the same rows behind here as it does in
        // the main view (suppressed under a flow, for the same reason).
        if self.flowed_view.is_none()
            && let Some(text) = streaming.as_deref()
        {
            let lines = self.view.agent_render.commit(text, screen.width);
            self.screen.insert_before(lines);
        }
        Ok(())
    }

    /// The link target under a cell of the **last painted** conversation
    /// window — what a click opens and a hover underlines. `None` on a cell
    /// with no link or outside the painted rows.
    pub(crate) fn full_link_at(&self, col: u16, row: u16) -> Option<String> {
        let line = self.view.full_window.get(usize::from(row))?;
        let id = carrier_at(line, col)?;
        links::link_url(id).map(|url| url.to_string())
    }
}

/// The pinned prompt strip for this frame: the turn [`ui::pinned_prompt`]
/// picks, drawn as one summary row, and the tail row it stands for (the click
/// target). `None` when no prompt qualifies, or when the one that does says
/// nothing worth summarising.
fn pinned_row(
    prompts: &[ui::UserPrompt],
    window_start: usize,
    width: usize,
    hovered: bool,
) -> Option<PinnedRow> {
    let prompt = ui::pinned_prompt(prompts, window_start)?;
    let width = u16::try_from(width).unwrap_or(u16::MAX);
    let line = ui::pinned_prompt_line(&prompt.text, width, hovered)?;
    Some(PinnedRow {
        start: prompt.start,
        line,
    })
}

/// A frame's pinned prompt: the summary row to paint at the top of the
/// conversation area, and the tail row a click on it scrolls back to.
struct PinnedRow {
    start: usize,
    line: Line<'static>,
}

/// The interned link id of the span covering column `col` on `line`, if any.
fn carrier_at(line: &Line<'static>, col: u16) -> Option<u32> {
    use unicode_width::UnicodeWidthStr;
    let mut x = 0u16;
    for span in &line.spans {
        let width = u16::try_from(span.content.width()).unwrap_or(u16::MAX);
        if col < x.saturating_add(width) {
            return span.style.underline_color.and_then(links::carrier_id);
        }
        x = x.saturating_add(width);
    }
    None
}

/// Look again for the file behind every picture `history` shows, before it is
/// laid out — so a picture whose file is gone reserves no rows
/// (`docs/ui/images.md`, *A picture whose file is gone*).
///
/// A `read` names its picture's size in its own fact line and a resumed
/// conversation keeps that line, while the file can be long gone — a
/// screenshot the agent saved under `/tmp`, wiped by a reboot, used to come
/// back as a screenful of blank rows. And a paste has no fact line at all,
/// so this is also where a loaded conversation's pastes get their sizes
/// back. One header read per picture ([`super::workers::image_dimensions`]);
/// the decision is [`images::recheck_pictures`]'s.
pub(crate) fn recheck_pictures(history: &[HistoryItem]) {
    images::recheck_pictures(history.iter().flat_map(ui::item_image_paths), |path| {
        super::workers::image_dimensions(std::path::Path::new(path))
    });
}

/// The live region's height for `app` at the current screen size — exactly what
/// the next draw will use.
///
/// A free function (not a [`Session`] method) because the commit paths call it
/// while `app` is already borrowed out of `self`.
///
/// Every count is taken at [`ui::live_text_width`] — the width the region is
/// actually **painted** at, which in the screen-owning mode is the screen minus
/// the sidebar, not the screen. Reserving from the wider number paid for fewer
/// wrapped rows than the paint took, so an open thinking phase's strip
/// overflowed by one row and its bottom-anchored flow ate the `● Thinking…`
/// header (the reported bug, `docs/ui/full-screen.md` *The live region's
/// width*).
fn live_region_height(app: &App, screen: ratatui::layout::Rect) -> u16 {
    // The width the region is **painted** at — the screen minus the sidebar in
    // the screen-owning mode, the screen itself inline. Every height below is
    // a wrap count, so all of them are this width's (`ui::live_text_width`).
    let width = ui::live_text_width(app, screen.width);
    // The `AskUserQuestion` modal replaces the whole region — checked first,
    // like its render branch (the two modals queue, so at most one is open;
    // docs/tools/ask.md).
    if let Some(height) = ui::ask_height(app, width, screen.height) {
        return height;
    }
    // A pending tool-permission request replaces the whole region — the
    // streaming strip included, since the turn is blocked on the answer. It is
    // modal, so it is checked first (docs/state/permissions.md).
    if let Some(height) = ui::permission_height(app, width, screen.height) {
        return height;
    }
    // The startup review replaces the composer next — only a modal outranks
    // it, its render branch's order (docs/state/config-review.md).
    if let Some(height) = ui::config_review_height(app, width, screen.height) {
        return height;
    }
    // The inline `/model` picker, `/login` flow, `/settings` menu and ↓
    // background manager each replace the **composer** with their own framed
    // body — never the streaming strip, whose rows they reserve above
    // themselves so opening one mid-turn can't hide the running turn (see
    // docs/providers/llm.md / docs/state/settings.md / docs/tools/background.md); the open one's
    // height stands in for the composer's.
    if let Some(height) = ui::model_picker_height(app, width, screen.height) {
        return height;
    }
    if let Some(height) = ui::key_onboarding_height(app, width, screen.height) {
        return height;
    }
    // The inline `/settings` menu, likewise (docs/state/settings.md).
    if let Some(height) = ui::settings_height(app, width, screen.height) {
        return height;
    }
    // The inline `/mascot` picker, likewise (docs/ui/mascot.md).
    if let Some(height) = ui::mascot_picker_height(app, width, screen.height) {
        return height;
    }
    // The inline `/spinner` picker, likewise (docs/ui/spinner.md).
    if let Some(height) = ui::spinner_picker_height(app, width, screen.height) {
        return height;
    }
    // The inline `/theme` picker, likewise (docs/ui/theme.md).
    if let Some(height) = ui::theme_picker_height(app, width, screen.height) {
        return height;
    }
    // The inline `/skills` menu, likewise (docs/tools/skills.md).
    if let Some(height) = ui::skills_menu_height(app, width, screen.height) {
        return height;
    }
    // The read-only `/hooks` menu, likewise (docs/tools/hooks-menu.md).
    if let Some(height) = ui::hooks_menu_height(app, width, screen.height) {
        return height;
    }
    // The `/secrets` page, likewise (docs/state/secrets.md).
    if let Some(height) = ui::secrets_picker_height(app, width, screen.height) {
        return height;
    }
    // The read-only `/donate` page, likewise (docs/ui/donate.md).
    if let Some(height) = ui::donate_picker_height(app, width, screen.height) {
        return height;
    }
    // The read-only `/export` page, likewise (docs/ui/export.md).
    if let Some(height) = ui::export_picker_height(app, width, screen.height) {
        return height;
    }
    // The read-only `/context` page, likewise (docs/architecture/context-usage.md).
    if let Some(height) = ui::context_usage_height(app, width, screen.height) {
        return height;
    }
    // The read-only `/usage` page, likewise (docs/providers/usage.md).
    if let Some(height) = ui::usage_page_height(app, width, screen.height) {
        return height;
    }
    if let Some(height) = ui::background_view_height(app, width, screen.height) {
        return height;
    }
    let band = ui::band_rows(app, width);
    ui::live_height(
        &app.input,
        width,
        screen.height,
        ui::strip_has_status(app),
        ui::fitted_preview_rows(app, width, screen.height),
        ui::hang_rows(app, width),
        ui::queued_rows(app, width),
        ui::toast_rows(app),
        band,
        ui::footer_rows(app, band),
        ui::agent_list_rows(app),
    )
}

impl Session<'_> {
    /// Ctrl+O: `App::on_key` already flipped the view — sync the overlay. The
    /// transcript cache is deliberately RETAINED across the close
    /// (`docs/ui/tool-view-performance.md`): its frozen prefix is what makes the
    /// next Ctrl+O instant, and the loop-bottom `transcript.warm` keeps appending
    /// to it as items commit.
    pub(crate) fn toggle_tool_view(&mut self) -> io::Result<()> {
        if self.app.view == View::ToolOutput {
            self.screen.enter_overlay()?;
            // Paint the overlay now rather than on the next tick — the
            // freshly-cleared alt screen would show as a black flash for a frame
            // otherwise.
            self.draw_tool_view()
        } else {
            self.screen.exit_overlay()?;
            // Catch the inline view up on whatever committed — or was
            // dispatched off the queue — while the overlay was showing: it all
            // sits in the pending queue, and this draw flushes it above the
            // live region, keeping the terminal's own scrollback (invariant 4
            // / Phase 7). A resize under the overlay forces the purge-rebuild
            // every resize gets, and an open agent session view rebuilds
            // itself (docs/agents/agent-tool.md).
            self.overlay_return_repaint()
        }
    }

    /// Ctrl+D: the raw-context view — the same overlay dance as Ctrl+O
    /// (`docs/architecture/context.md`).
    pub(crate) fn toggle_context_debug(&mut self) -> io::Result<()> {
        if self.app.view == View::ContextDebug {
            self.screen.enter_overlay()?;
            self.draw_context_view()
        } else {
            // Return the rendered window's memory with the view: the cache
            // makes redraws cheap while it is up, but kept past the close it
            // is a second full rendered copy of the conversation resident
            // for the rest of the session. The next Ctrl+D rebuilds once.
            self.view.context.release();
            // …and the classifier page's injected block, which the next open
            // refreshes from the backend anyway.
            self.app.set_classifier_context(None);
            self.screen.exit_overlay()?;
            self.overlay_return_repaint()
        }
    }
}

impl Session<'_> {
    /// A coalesced draw tick: refresh the injected times, expire the toast, sweep
    /// the agent roster, paint — then, while a turn is in flight, **re-arm** the
    /// next animation frame (codex's status-widget pattern) so the verb's shimmer
    /// sweeps and the timer advances even when no reply event arrives (a tool run,
    /// a thinking pause). The chain seeds from the Submit keypress and stops by
    /// itself on the first draw after the turn ends.
    pub(crate) fn on_draw_tick(&mut self) -> io::Result<()> {
        self.update_status_times();
        // Inject each background shell's runtime (the boundary owns the started
        // clocks — docs/tools/background.md), so the manager's details view ticks.
        for (id, started) in &self.bg_clocks {
            self.app.set_background_runtime(id, started.elapsed());
        }
        // …and each running agent's, plus the linger sweep (docs/agents/agent-tool.md).
        self.tick_agent_roster();
        self.expire_toast();
        // Before the paint: a question whose idle clock ran out closes in
        // this very frame (docs/tools/ask.md).
        self.tick_ask_clock();
        // …and an open `/usage` page due its minute refresh starts it, the
        // page keeping its own slow frame (docs/providers/usage.md).
        self.tick_usage_page();
        self.draw_active_view()?;
        // The ↓ manager band re-arms frames like an active turn: its details
        // view's Runtime ticks with no events otherwise — and so does a non-empty
        // agent roster (its elapsed counters tick, and the linger sweep needs the
        // frames to fire). None of that re-arms under an alternate-screen
        // overlay, where none of it is on screen and a repaint for no reason
        // costs the user their text selection ([`App::wants_animation_frames`],
        // `docs/ui/overlay-repaint.md`); the chain re-seeds on the first draw after
        // the return.
        if self.app.wants_animation_frames() {
            self.frame.schedule_frame_in(STATUS_FRAME_INTERVAL);
        }
        // The `/login` device page keeps its own, much slower chain. Its only
        // moving part is an `mm:ss` countdown, so a frame a second is exactly
        // the rate its content changes at — and every frame costs a cursor
        // Hide plus a re-seat, which at the status chain's thirty a second is
        // what a terminal with a cursor-trail animation renders as a permanent
        // shimmer over the page (`docs/providers/copilot.md`).
        if self.app.device_login_active() {
            self.frame.schedule_frame_in(DEVICE_TICK_INTERVAL);
        }
        Ok(())
    }

    /// Expire the transient toast when its deadline passes (so this very frame
    /// paints without it); while it still lingers, keep a frame pending for the
    /// eventual clear — a coalesced keystroke frame can consume the one
    /// [`Session::toast`] scheduled, so re-arming here guarantees the clear fires.
    /// See `docs/ui/toast.md`.
    fn expire_toast(&mut self) {
        if let Some(deadline) = self.toast_deadline {
            let now = std::time::Instant::now();
            if now >= deadline {
                self.app.clear_toast();
                self.toast_deadline = None;
            } else {
                self.frame.schedule_frame_in(deadline - now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    use zeroth::app::App;
    use zeroth::ui;

    use super::{ConversationView, frame_history, live_region_height};

    /// A full-screen app with an open thinking phase whose block wraps into a
    /// different number of display rows on either side of the sidebar's edge.
    fn thinking_app() -> App {
        let mut app = App::new();
        app.set_fullscreen(true);
        // The sidebar is opt-in (`docs/ui/sidebar.md`); this sweep is about
        // the region painted one column narrower because of it.
        app.settings_mut().sidebar = true;
        app.begin_stream();
        app.begin_reasoning();
        app.push_thinking(
            "Interesting: tmux output missing version. Let me check tmux and also run a \
             quick sanity test to demonstrate. The user wants all capabilities shown: read \
             code, run a targeted test, use a subagent, web search. Let me check the \
             uncommitted docs changes too.",
        );
        app
    }

    /// The region's height from the `ui` counts alone, at the width it is
    /// painted at — what the boundary must reserve. Spelled here rather than
    /// read back out of [`live_region_height`], so the two are independent.
    fn wanted_height(app: &App, painted_w: u16, term_h: u16) -> u16 {
        let band = ui::band_rows(app, painted_w);
        ui::live_height(
            &app.input,
            painted_w,
            term_h,
            ui::strip_has_status(app),
            ui::fitted_preview_rows(app, painted_w, term_h),
            ui::hang_rows(app, painted_w),
            ui::queued_rows(app, painted_w),
            ui::toast_rows(app),
            band,
            ui::footer_rows(app, band),
            ui::agent_list_rows(app),
        )
    }

    #[test]
    fn a_fresh_view_rebuilds_the_same_frame_rows_from_the_app_and_a_width() {
        // Phase 1 of `docs/architecture/core-host-split.md`: the frame cache is
        // a view value derived from `App` plus a width, so a second, fresh view
        // paints the same history rows as the one that built incrementally —
        // and a cache hit is the same rows again.
        use zeroth::app::{HistoryItem, Message, Role};
        let mut app = App::new();
        app.set_fullscreen(true);
        for (role, text) in [
            (Role::User, "what does the parser do here?"),
            (
                Role::Assistant,
                "It peeks one token ahead, so the tag it records is the one the \
                 parser sees later.",
            ),
        ] {
            app.history.push(HistoryItem::Message(Message {
                role,
                text: text.to_string(),
                timestamp: "10:00 AM".to_string(),
                images: Vec::new(),
            }));
        }
        let (card_tx, _card_rx) = tokio::sync::mpsc::unbounded_channel();
        let plain = |lines: &[ratatui::text::Line<'static>]| -> Vec<String> {
            lines
                .iter()
                .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
                .collect()
        };
        let mut built = ConversationView::default();
        frame_history(&app, &mut built, 80, 80, true, &card_tx);
        let first = plain(&built.full_tail.as_ref().expect("built").lines);
        // A cache hit rebuilds nothing and paints the same rows.
        frame_history(&app, &mut built, 80, 80, true, &card_tx);
        let cached = plain(&built.full_tail.as_ref().expect("cached").lines);
        // A fresh view derives them again from the app and the width.
        let mut fresh = ConversationView::default();
        frame_history(&app, &mut fresh, 80, 80, true, &card_tx);
        let again = plain(&fresh.full_tail.as_ref().expect("rebuilt").lines);
        assert_eq!(first, cached, "a cache hit repaints the same rows");
        assert_eq!(
            first, again,
            "a fresh view derives the same rows from App + width"
        );
        assert!(!first.is_empty(), "the fixture paints something");
    }

    #[test]
    fn the_live_region_reserves_its_height_at_the_width_it_paints() {
        // The reported bug: full screen, sidebar up, and a thinking phase
        // painted its `⎿` body with the `● Thinking…` header gone. The live
        // region's height was reserved from the **full screen width** while
        // `term::InlineViewport::draw_full_with_tail` paints it into the
        // screen minus the sidebar, so the block wrapped into one more display
        // row at the painted width than the reservation had paid for — and the
        // strip's bottom-anchored flow kept its newest rows and dropped the
        // head (`docs/ui/strip-flow.md`, `docs/ui/full-screen.md`).
        //
        // This is the boundary's own width decision, which a `ui` test cannot
        // reach: the height must be reserved from the width the region is
        // painted at. Widths where the sidebar is hidden are skipped — there
        // the two are the same number and nothing can disagree.
        let app = thinking_app();
        let term_h = 31;
        let mut checked = 0;
        for screen_w in [96u16, 111, 112, 120, 149, 160, 180, 200] {
            let painted_w = screen_w - ui::sidebar_width(screen_w);
            if painted_w == screen_w {
                continue;
            }
            checked += 1;
            let screen = Rect::new(0, 0, screen_w, term_h);
            let reserved = live_region_height(&app, screen);
            let want = wanted_height(&app, painted_w, term_h);
            // Paint the region into exactly the rect the boundary gives it:
            // the screen minus the sidebar, the reserved height.
            let mut buf = Buffer::empty(Rect::new(0, 0, painted_w, reserved));
            ui::render_workspace_live(buf.area, &mut buf, &app, Some(&[]));
            let painted: Vec<String> = (0..reserved)
                .map(|y| (0..painted_w).map(|x| buf[(x, y)].symbol()).collect())
                .collect();
            assert_eq!(
                reserved,
                want,
                "screen_w={screen_w} (painted at {painted_w}): the region reserved \
                 {reserved} rows but its content needs {want}\n{}",
                painted.join("\n")
            );
            // The block itself is part of the conversation now, built at the
            // painted width: its header leads and every row fits
            // (`docs/ui/full-screen.md` *Live cells ride the conversation*).
            let block = ui::conversation_live_lines(&app, painted_w);
            let texts: Vec<String> = block
                .iter()
                .map(|line| line.spans.iter().map(|s| s.content.as_ref()).collect())
                .collect();
            assert!(
                texts.first().is_some_and(|row| row.contains("Thinking…")),
                "screen_w={screen_w} (painted at {painted_w}): the block's header is \
                 missing\n{}",
                texts.join("\n")
            );
            for line in &block {
                assert!(
                    line.width() <= usize::from(painted_w),
                    "screen_w={screen_w} (painted at {painted_w}): {:?} overflows",
                    line.spans
                        .iter()
                        .map(|s| s.content.as_ref())
                        .collect::<String>()
                );
            }
        }
        assert!(
            checked >= 4,
            "the sweep must cover the sidebar's three bands"
        );
    }
}
