# The screen-owning TUI — full-screen mode (design)

**Status: phase 1 (skeleton) implemented.** `--fullscreen` or
`ZEROTH_TUI=full` paints the conversation and the live region into one
alternate-screen frame. Scrolling, own selection/copy, mouse and cards are the
next phases; the inline viewport remains the default until the parity checklist
below is green.

## Why

The inline viewport is a conversation that lives in the terminal's own
scrollback: every finished row is painted once, then belongs to the terminal.
That is the feature — your shell history and the chat share one scrollback — and
it is also the ceiling. A finished row cannot be repainted, re-measured, or
attributed to a mouse coordinate. So every interaction that needs to know
*where* a cell is — hover, click-to-expand, a preview card under a link, a drawn
scrollbar, a selection that follows a link — either has to be delegated to the
terminal (OSC 8 is exactly such a delegation) or cannot exist at all. Adding
each one later means asking the terminal for another favour: the screen is not
ours, so the design is always palm-up.

Owning the screen removes the ceiling. The app knows every cell it painted, so
it can hit-test any of them, repaint any of them, and let any future affordance
be an ordinary rendering decision. The cost is real — the terminal's native
scrollback, selection and copy stop being available — and this page is the plan
for paying it deliberately, in a mode that is opt-in until it is at least as
good as what it replaces.

**Decision:** build a screen-owning full-screen mode. Keep the inline viewport
as the default until the parity checklist below is green, then revisit the
default (and whether inline remains an option) as a separate product call.

## What stays

- **`App` is the single source of truth.** `history`, the live tool queue, the
  agents, the overlays, the composer, the queue, the checkpoints: unchanged.
  The full-screen mode is a second **viewport** over the same state, not a
  second app.
- **The event loop and the backend seam.** `StreamEvent`s, the agent loop,
  `ReplySource`, permissions, hooks, MCP, background shells: unchanged.
- **Headless.** `-p` never creates a viewport and is unaffected.
- **Invariants 1, 2 and 4** (`CLAUDE.md`): one stdin reader; prefix-stable wrap
  for line building; tool calls interleave with text. Invariant 2 still matters
  even though the whole screen is repainted: the row builders and the transcript
  cache rely on a completed line never changing when more text arrives.
- **The builders.** `ui::transcript`, `ui::live`, `ui::header`, `ui::footer`,
  `ui::composer`, `ui::toast`, the pickers, the permission prompt,
  `images::place`/`ImageStore::stamp`, and `term::draw_cells` (OSC 8, images,
  synchronized updates) are the raw material. The full-screen frame is a new
  composition of them, not a rewrite of them.

## What changes

Invariant 3 — the content-anchored inline viewport and its two-way resize
reflow — is the **inline mode's** invariant. The full-screen mode replaces it
with a different contract, stated below, and the two must not be mixed.

### The viewport seam

`InlineViewport` gains a sibling, and the call sites see one `Screen` value
(an enum, not a `dyn` trait — the draw methods take a closure):

```rust
enum Screen {
    Inline(InlineViewport),
    Full(FullViewport),
}

impl Screen {
    /// Show `lines` as part of the conversation. Inline: commit into
    /// scrollback (`insert_before`). Full: no-op — history already holds it.
    fn commit(&mut self, lines: Vec<Line<'static>>) -> io::Result<()>;
    /// Reseat before a commit (inline) / no-op.
    fn set_view_height(&mut self, height: u16) -> io::Result<()>;
    /// The caller composes a region: inline paints the live region at the
    /// bottom; full paints its whole frame and places that region in it.
    fn draw(&mut self, render: impl FnOnce(Rect, &mut Buffer)) -> io::Result<()>;
    /// An app-owned page: inline opens an alt-screen overlay; full paints the
    /// same page into its frame.
    fn draw_overlay(&mut self, render: impl FnOnce(Rect, &mut Buffer)) -> io::Result<()>;
    fn exit_overlay(&mut self) -> io::Result<()>;
    fn suspend(&mut self) -> io::Result<()>;
    fn resume(&mut self) -> io::Result<()>;
    fn restore(&mut self) -> io::Result<()>;
}
```

The TUI handlers already funnel their scrollback writes through a handful of
`Session` methods (`commit_tool_cell`, `flush_segment`, `insert_before`,
`reflow`, `overlay_return_repaint`, …). In full mode the commit methods become
no-ops (the record in `history` is the frame's source) and the frame is rebuilt
from state each draw. That keeps the handlers and the event contract untouched;
the mode switch is at the boundary, not in every `if fullscreen`.

### The frame

A frame is a pure function of:

```
App state × width × height × scroll offset × open overlay × theme
```

and is built top to bottom:

1. the header banner (the same builder as inline, when configured),
2. the conversation: finished history rendered by the transcript builders,
   followed by the live region (streaming text, thinking, tool cells/groups,
   the agent tree, queued messages, background notices),
3. the status line and tips,
4. the composer and footer,
5. any overlay page (permission prompt, ask, pickers, `/usage`, Ctrl+O…), which
   replaces the area or the whole frame exactly as its inline counterpart does.

The transcript cache (`ui::transcript::TranscriptCache`) already renders frozen
history once per width and appends only the tail; the frame reuses it. The live
region's height, cursor position and preview are the same `ui::live` functions
the inline strip uses. Because the frame is rebuilt from history, `/resume`, a
rewind, `/clear` and a resize are just a redraw — no purge/rebuild dance, no
`insert_before` queue, no viewport pinning.

### Scrolling

The conversation is taller than the screen, so the mode owns a scroll model
(the reference `BodyView`):

- **Follow-tail by default**, with a floor that only moves forward: new output
  pushes the view down, never yanks it up.
- **PageUp/PageDown, Home/End, wheel**; any key that submits or starts a turn
  returns to the tail; a scrolled-up view stays put while new text arrives.
- **A bottom hint** (the existing button pattern) when the view is not at the
  tail, and a `N new lines` marker.
- **Resize anchors** the first visible row (or the selected row) instead of
  the previous screen geometry; a reflow re-wraps from history.
- **Selection** anchors to `(row, column)` in the frame, re-resolved after a
  rebuild.

### Mouse and hit-testing

`EnableMouseCapture` is on for the whole full-screen session (there is no
native scrollback left to protect). A frame carries a **hit map** built beside
it:

```rust
struct RowHit {
    target: Option<Target>,          // history item / tool cell / agent group
    links: Vec<(u16, u16, String)>,  // decoded from the spans' carriers
    button: Option<Action>,          // e.g. the bottom hint
}
```

- `Moved` → hover: underline the link run, highlight the togglable row;
- left `Up` on a link → open it (OSC 8's terminal job is now the app's);
- left `Up` on a target → toggle that item's expansion, anchoring the clicked
  row so the expansion grows downward;
- wheel / drag as above; double-click selects a word (optional), triple a line;
- the keyboard stays the primary interface — mouse is always additive.

The permission prompt, the ask modal and the pickers keep their keyboard
contracts; click targets on their option rows are an optional later layer.

### Selection and copy

Losing native selection is the main cost. The mode implements drag-select over
the frame, with `Ctrl+C`/`c` copying through the existing clipboard module; a
selection survives new output, marks links by their full target, and copies
text without the gutter/indent decoration (the `BodyView::selected_text` rule).
`Shift+drag` can still be left to the terminal on terminals that implement the
bypass; the app should not depend on it. Whether OSC 52 is added as a fallback
for copy over ssh is an open question below.

### Alt-screen lifecycle

- Enter: raw mode (already), `EnterAlternateScreen`, mouse capture, bracketed
  paste (already), keyboard enhancement when supported (optional), then the
  first full repaint.
- Leave: the reverse, on every exit path, `SIGINT`, panic (the existing panic
  hook already restores the inline screen).
- `Ctrl+Z` / `SIGTSTP`: leave the alternate screen and raw mode, suspend, and
  on `fg` re-enter and repaint. This is exactly Miyu's `suspend`/`reenter`
  path, and it is the one place where re-entering must not re-query the
  terminal (the single stdin reader is still alive; invariant 1).
- Editor handoff (optional, later): leave raw/capture, run `$EDITOR`, re-enter,
  repaint; the terminal is not left on the shell.

### Links, paths and cards

The rich-links design ([`rich-links.md`](rich-links.md)) becomes simpler once
the screen is owned: bare paths get `file://` targets through the existing
carrier, hover underlines, click opens; a link card can be drawn **inline** in
the reply because late metadata may legally change the frame's height (the
scroll anchor keeps the reading position). The fetch/cache policy on that page
still applies.

## What the skeleton does today

- `--fullscreen` (or `ZEROTH_TUI=full`) enters the alternate screen after the
  usual startup query; every exit path — a normal quit, an error, a panic —
  leaves it (`OVERLAY_ACTIVE` is used for the session, so the existing panic
  hook and `restore` do the teardown).
- `Session::draw_conversation` composes one frame: the banner, the finished
  history, the in-flight reply as an assistant message, then the live region,
  composer and footer at the bottom. The conversation follows its tail (the
  last `screen − live` rows); a short conversation starts at the top.
- The strip's streaming preview is suppressed in this mode (the reply is
  already in the conversation), and the tail is cached by
  `(agent view, history generation, width, streaming length, theme)` so a 120-fps
  animation frame does not re-render the whole conversation.
- **Scrolling.** PageUp/PageDown move a viewport-sized page, Ctrl+Home/Ctrl+End
  go to the top/tail, and the wheel moves three lines. Scrolling up anchors the
  window to that tail line (`full_top`), so new output does not move it;
  reaching the bottom resumes follow-tail. The keys are claimed only when the
  boundary marked the app full-screen, so the inline composer keeps them.
  When the user message that owns the top of the window has scrolled above it,
  its first line is **pinned** on one reserved row (the latest user message
  above the window), so the question stays with its answer; while the prompt
  itself is visible no row is spent (`sticky_line`, `full_user_starts`).
- **Mouse.** Capture is on for the session (and disabled on exit). The last
  painted conversation rows are the hit map (`full_window`): moving over a link
  underlines its run, a left click with no drag opens it through the platform
  opener (`open`/`xdg-open`/`cmd start`), a left drag selects and copies (the
  app's own selection, since capture takes the terminal's; Shift+drag remains
  the terminal's escape hatch), and Shift is not required. Wheel scrolls.
  A click with no link **toggles the expandable cell under it**
  (`full_items` → `conversation_lines_expanded`): a tool call, thought, task
  call, agent group or hook report switches between its collapsed inline form
  and the Ctrl+O form, with the item's first line pinned to the row it
  occupied (`full_anchor`). A left click in the composer's text rows places
  the caret there (`ui::composer_cursor_at` + `TextArea::set_cursor`).
  Links are the carriers already painted — OSC 8 URLs, file-tool headers and
  the prose/code file paths `links::find_paths` now marks — so a click opens
  a URL or a file through the same platform opener.
- Ctrl+O, the context view and every full-screen page paint over the frame
  through the existing overlay path; leaving a page repaints the frame. A
  resize re-wraps from history at the new width.
- **Not yet:** character-precise selection (the highlight is span-level),
  pointer clicks on the live region's options/pickers (the composer's caret is
  in), link cards, suspend (`Ctrl+Z`) / editor handoff, and agent-view parity
  beyond the transcript tail. They are the phases below.

## Mode selection and rollout

- **Opt-in first:** `--fullscreen` (name to settle) plus a config key
  (`ui.screen = "inline" | "full"`) and an environment override for tests
  (`ZEROTH_TUI=full`, the Miyu `MIYU_TUI=1` precedent). Default: `inline`.
- **Parity checklist** before the default flips: streaming text, thinking,
  tool cells/groups, parallel batches, agents/agent view, background shells,
  permission + ask prompts, every picker and page, `/clear`, `/resume`,
  rewind/backtrack, Ctrl+O (expanded transcript), images, toasts, status/tips,
  footer, queue/steer, checkpoints notices, Ctrl+B, paste, resize, suspend,
  exit restoration, headless untouched.
- **Smoke:** the existing phases stay green against the inline default. A
  full-screen phase set runs the binary with `ZEROTH_TUI=full` and asserts on
  `tmux capture-pane` of the alternate screen; mouse can be injected with
  `tmux send-keys -M` (or an SGR sequence through the pty harness). Keyboard
  parity gets the bulk of the assertions; mouse gets unit-level hit-map tests
  plus a few end-to-end clicks.
- **Then** flip the default, keep `--inline` for a deprecation window, and
  decide (with the owner) whether inline remains a permanent option or the
  full mode becomes the only interactive front end.

## Risks and mitigations

- **Losing native scrollback.** In this mode the conversation leaves with the
  screen. Mitigation: `/export` and copy already exist; the frame's own
  scroll model covers reading; the alternate screen restores the shell's screen
  on exit by design.
- **Selection/copy quality.** A hand-rolled selection is never as good as the
  terminal's first day. Mitigation: copy via the system clipboard, full-target
  copy for links, and a Shift-drag escape hatch documented per terminal.
- **Terminal/tmux quirks.** Alternate screen, mouse reporting and image
  protocols differ. Mitigation: the mode is opt-in, the inline path stays
  available, and the image/OSC 8 gates are shared.
- **Performance.** Rebuilding the whole frame per draw is O(screen), and the
  transcript cache already does the expensive part. Diffing is per cell. The
  budget is 60 fps while streaming at 200×60 over a large session; measure
  before flipping the default.
- **A second front end to test.** The smoke matrix doubles. Mitigation: keep
  the frame composition shared, and phase the feature set.

## Phases

1. **Skeleton — done.** `--fullscreen`: alt-screen lifecycle, frame from
   history + live region + composer + status, follow-tail, keyboard only,
   exit/panic restore, a resize re-wrap, and Phase 156 driving it end to end.
2. **Parity — next.** Agents/agent view, prompts, pickers, images, toasts,
   queue, backtrack, resume/clear, Ctrl+O, resize anchors; the parity checklist
   green.
3. **Mouse and scrolling — done except cards/selection polish.** Capture, hit
   map, hover, click links, click-to-expand, wheel,
   PageUp/PageDown/Ctrl+Home/End, drag-select/copy, and the composer's
   click-to-place caret all work; still open here: character-precise selection
   and mouse on picker/option rows.
4. **Rich links.** Preview cards inline; the preview policy.
5. **Default decision.** Flip the default, keep `--inline`, evaluate removal.

## Open questions

- **Is inline permanent?** Keeping both is a real cost (two viewports, two
  renderers, a doubled smoke matrix). The owner decision to pursue full screen
  is recorded; removal is a later call.
- **Ctrl+O's role.** In full screen the conversation is already the transcript;
  Ctrl+O can stay as "expanded everywhere" (the current overlay), become a
  jump-to-last-reply action, or be retired. Decide during parity.
- **Selection semantics.** Word/line selection, full-target copy for a link,
  OSC 52 as a copy fallback over ssh.
- **The mode's name and flag.** `--fullscreen` is descriptive; `--tui` pairs
  badly with `--inline`; settle before the first release that carries it.
- **Scrollback on exit.** Should the mode leave the final screen (a "last
  frame" harvest) or restore the shell's screen like every alternate-screen
  app? Restoring is the default; a `/copy transcript` is the escape hatch.

## Non-goals

- Changing the agent loop, the tool contract, the session format or headless.
- Making the full-screen mode the default before parity.
- Re-implementing the terminal's emulator: scrolling, selection and copy are
  ours in this mode, but interpretation of pasted text, bracketed paste and
  image protocols stays with the terminal and the existing modules.
