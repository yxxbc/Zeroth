# Tool details and the pinned prompt in the conversation

The screen-owning mode (`--fullscreen` or `ZEROTH_TUI=full`, [`full-screen`](full-screen.md))
turns the conversation itself into something you can work in. A tool call opens
where it sits instead of on another page, and however far a reply scrolls, the
question it answers stays on one row at the top. Neither is possible in the
inline viewport, which is the reason the mode exists: a finished row there is
the terminal's, so it can be neither toggled nor hit-tested. The inline default
keeps the collapsed cells and Ctrl+O.

## Tool calls expand in the conversation

A backend call starts collapsed: the `● name(args)` header coloured by
lifecycle and its `⎿` output. The ordinary output peek shows three display
rows (`TOOL_FOLD_ROWS`) plus `… +N lines (ctrl+o to expand)` when it folds;
an output of four rows costs no more to show whole. A `!` shell command does
not fold ([`src/ui/tool.rs`](../../src/ui/tool.rs), `tool_cell_body`).

A left click anywhere on the rows of an expandable item replaces that cell with
the form the Ctrl+O transcript draws for it — the same builder,
[`ui::transcript::expanded_item_lines`](../../src/ui/transcript.rs), reached from
[`conversation_lines_carded`](../../src/ui/conversation.rs), which the
full-screen frame is built from ([`src/tui/view.rs`](../../src/tui/view.rs), the
`expanded` predicate over `full_expanded`). Clicking any of its rows folds it
back. Clicks are only taken in the plain conversation: an open overlay, the
permission prompt and the `AskUserQuestion` question take the mouse first.

"Expandable" is [`ui::is_expandable`](../../src/ui/conversation.rs): a tool
call, a task call, a thought, a compaction marker, an agent group, a hook
report. Messages, notices and turn summaries are already whole on screen, so
they have no second form to switch to, and a run of `read`/`search` calls folded
into one collapsed group ([`tool-groups`](tool-groups.md)) is a single line with
no item under it.

Expanding adds rows below the header; folding takes them back. The click
records the item's start and its first line's screen row (`full_anchor`,
[`src/tui/actions.rs`](../../src/tui/actions.rs), `toggle_full_item`). The
next frame tries to keep that first line in place, clamped to the rebuilt
conversation ([`src/tui/view.rs`](../../src/tui/view.rs)). This pins a row
when the resulting view is scrolled up (`full_top`). If the clamped position
lands at the new bottom, the view follows the tail instead. A tall expanded
cell can then scroll its own header off the top. A click at the tail is not
itself a guarantee either way: a growing cell can put the anchor above the
new bottom. Keeping the clicked item reliably in view is a workspace goal,
not a guarantee of the current toggle.

The details are not a second scroller. They are laid out as ordinary
conversation rows, so they scroll with the conversation (PageUp/PageDown,
Ctrl+Home/Ctrl+End, the wheel — `scroll_full`), a drag selects and copies them
with everything else, and every row of the item is a click target, so there is
no small "collapse" control to miss.

### What the expanded form shows

The expanded cell is the same record as the collapsed one, whole:

- a `bash` call is its header over every row the collapsed peek folded, closed
  by the recorded execution facts — exit code, error kind, elapsed — on a `⎿`
  row ([`src/ui/tool.rs`](../../src/ui/tool.rs), `tool_full_lines` and
  `command_receipt_lines`). The runner's `Exit code: N` frame is kept out of the
  display in both forms, and a line that is a whole JSON document is reshaped
  for reading in both;
- a `read`, `write` or `edit` in the numbered file format shows the numbered
  block uncapped — line numbers, `⋮` gaps between hunks, the `+`/`-` tints and
  the character-level refinement ([`src/ui/file_cell.rs`](../../src/ui/file_cell.rs)) —
  where the collapsed cell shows up to ten body display rows (`FILE_PEEK_LINES`).
  Its `… +N lines (ctrl+o to expand)` hint counts source lines it never started,
  not wrapped display rows;
- a recognised search is grouped by file with line-number gutters, a statistics
  row (matches and files, or `No matches`) and the command in its own section;
  output the recogniser does not understand keeps the `Output` label and the
  raw lines ([`src/ui/search.rs`](../../src/ui/search.rs), `search_lines` with
  `full` on);
- a thought shows its whole chain, a compaction its summary body, an agent group
  one cell per subagent, a hook report each hook that ran.

Three states need saying plainly because their length does not say them. A call
that produced nothing reads `(no output)`, one still queued reads `Waiting…`,
and a running one with no output yet `Running…`. Truncated output, including
a capped `read`, `web_fetch` or `web_search`, is marked with a dim `…` ([`TOOL_TRUNCATED_MARKER`](../../src/ui/theme.rs)): past the cap
nothing is left to expand to, so the marker is the only thing that can say the
output shown is not the output there was. A recognised search whose retained
output was cut appends `retained results` to its count rather than claim a total
([`search-details`](search-details.md)). A failed call's expanded form is its
error text under the red bullet, and since the expanded cell draws the stored
record, a conversation brought back by `/resume` opens the same way.

The main conversation builds its click targets (`full_items`) from `history`
and adds the streaming reply separately ([`src/tui/view.rs`](../../src/tui/view.rs)).
It does not draw `app.tool_queue` into that conversation window, so there is
no live call there to click. The live strip still draws the running cell —
for a command, its header, newest rows of output and `+N lines (Ns · wait …)`
clock row ([`src/ui/tool.rs`](../../src/ui/tool.rs), `running_command_lines`).
There is no finished-only rule in the expanded renderer: it can show waiting
and running records when Ctrl+O includes the live queue. A resumed rollout
restores only settled tool records ([`src/session.rs`](../../src/session.rs));
it does not restore the queue. Live output updates the strip rather than
adding rows to the conversation. A tool cell that carries a picture hangs it
under the cell in both forms
([`src/ui/image.rs`](../../src/ui/image.rs)).

### Ctrl+O

Ctrl+O still opens the whole conversation as a pager over the alternate screen:
every call expanded in order, the live tail at the end. `q` and `ctrl+o`
leave; `esc` leaves only when it is not opening or stepping through the
backtrack preview ([`src/app/views.rs`](../../src/app/views.rs), `on_key_overlay_toggle`;
[`src/ui/transcript.rs`](../../src/ui/transcript.rs), `render_tool_view`). It
never was the way to look at one call — it has no selection — and it keeps its
job: read the lot, scroll it, copy out of it. Screen ownership adds the smaller
gesture of one cell opened where it sits, with the conversation still scrolling
around it, and takes nothing from Ctrl+O. The `(ctrl+o to expand)` hint on a
collapsed cell still points at the pager that holds every call; which of the two
survives a default flip is left open on the design page.

## The user's prompt pinned above it

While you are reading a reply whose question has scrolled off the top, the
question stays on one row at the top of the conversation area: a summary of the
whole message, with the conversation beneath it holding its scroll. A reader who
joins a long answer halfway can still see what was asked without having to
spend rows of the answer finding out.

**The row is a summary, not the question's first line.** The prompt's wording
is folded onto one line — every line break and run of blanks becomes a single
space — so the words that actually asked the question are what survives. A long
question usually opens with its least informative part (`look at this:`), which
is exactly the piece a first-line copy would have shown. A summary too long for
the frame is cut on a grapheme boundary and ends in `…`
([`src/ui/conversation.rs`](../../src/ui/conversation.rs),
`pinned_prompt_line`).

**It appears only once the whole message is gone.** The turn the window's top
sits in — the latest user message starting above it — is the candidate, and it
is pinned only when its **last** row has passed the top (`pinned_prompt`). While
any part of that message is still on screen nothing is pinned: a copy of its
opening line directly above its own continuation reads as two paragraphs of one
sentence. This is what makes the rule turn-aware rather than line-aware, and it
is why a one-row prompt behaves exactly as it always did — it leaves the screen
all at once. Choosing by the window's position rather than by the newest message
in the session means scrolling up through old turns takes the pin back with you.

The pin costs one row, so it never covers text. With room for a pin, the frame
reserves that row and recomputes the window start; at the tail this may drop one
more row from the top. A one-row conversation area suppresses the pin rather
than hiding the only visible line. Incoming text can change the pin if following
the tail moves the window past another prompt.

**It is a control.** The strip stands for a prompt above the window, so pointing
at it lights its ink — the accent, the same cue a picker's selected row uses —
and a left click scrolls that prompt back into view: its first row becomes the
window's top, which retires the strip on the next frame because the message is
visible again
([`src/tui/actions.rs`](../../src/tui/actions.rs), `on_pin_row`,
`scroll_to_pinned_prompt`). The scroll is clamped like every other anchor
([`src/ui/layout.rs`](../../src/ui/layout.rs), `scroll_anchor`), so landing on
the last screenful re-arms follow-tail exactly as PageDown there does. The strip
itself is still not part of the conversation's hit map (`full_window`): a click
on it returns to the prompt, and never selects text or opens a link.

The pin sits at the top of the conversation area, below the banner; a session
short enough that the banner and the conversation fit without scrolling shows
nothing. Inside an agent's session view the frame uses the agent's own history
for the same rule, not the main conversation's prompts.

## Where this is covered

The frame and the mouse are the boundary, verified by running the app. The pure
halves carry their tests: the conversation walk's expanded branch and the
`is_expandable` set ([`src/ui/tests/transcript.rs`](../../src/ui/tests/transcript.rs)),
the expanded cell's per-tool bodies in the Ctrl+O tests
([`src/ui/tests/tool.rs`](../../src/ui/tests/tool.rs),
[`src/ui/tests/search.rs`](../../src/ui/tests/search.rs)), the pinned strip's
rule and its summary row in
[`src/ui/tests/pinned_prompt.rs`](../../src/ui/tests/pinned_prompt.rs), the
scroll anchor it shares with every scroll key in
[`src/ui/tests/layout.rs`](../../src/ui/tests/layout.rs). The mode runs end to
end under tmux in smoke Phase 156
([`docs/ops/smoke.md`](../ops/smoke.md)) — which also clicks the strip and
checks the prompt comes back.
