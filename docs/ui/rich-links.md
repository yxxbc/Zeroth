# Rich links — clickable paths, preview cards, and screen ownership

**Status: paths, click-to-expand and preview cards (text + cover) implemented.**
Assistant prose and inline code link file-path-like tokens to `file://` URIs in
both viewports ([`links.md`](links.md) *File paths*); the screen-owning mode
adds click-to-expand, the composer's click-to-place caret and **preview cards**
for a URL alone on a line: site, title, description, and — when the page has an
`og:image` and the terminal draws pictures — a cover thumbnail on the left
(fetched on demand through `zeroth::link_card`, cached under
`~/.zeroth/cache/link-cards.json` + `cache/link-cards/`, `ALTER_ZERO_LINK_CARDS=0`
disables). B's overlay-only fallback and the editor handoff remain later
phases ([`full-screen.md`](full-screen.md)).

## The ask

The backlog asks for three things the current link support does not give:

1. a **preview card** for a link in a reply — title, site, description, a cover;
2. **paths clickable** — a file path in prose, a `pwd`, a URL alike;
3. a **single event clickable to expand it** — one tool cell or one message,
   not the whole Ctrl+O transcript.

URLs are already clickable through OSC 8: the terminal turns a marked run into
a link, and clicking it is the terminal's own action, with no mouse capture and
no app involvement. File-tool **headers** are linked to their file the same
way. What is missing is path detection in prose and output, the card, and
anything that needs the app to know where the pointer is.

## What exists today

- **Links are carriers, not hit tests.** `links::find_urls` marks URL slices in
  the renderer, `links::linked(style, url)` stamps an interned id into
  `Style::underline_color`, and every cell-writing path (`term::draw_cells`)
  brackets marked runs with `OSC 8`. The visible text and styles are unchanged;
  the terminal owns the click. The overlay (`Ctrl+O`) goes through the same
  emitter, so links work there too.
- **File paths are linked only where a renderer was handed the policy.**
  `ui::tool::tool_header_lines` uses `PathDisplay::file_url` to link a
  `● Read/Write/Edit({path})` header. Assistant prose has no `PathDisplay`, so
  a path written in a reply (`src/app/tools.rs:657`) is plain text. Tool output
  is not linked at all ([`links.md`](links.md) *Limitations*).
- **The conversation is inline.** The viewport is content-anchored, and every
  completed row is committed into the terminal's scrollback (invariant 3).
  Committed rows cannot be repainted, re-measured, or attributed to a mouse
  coordinate: the terminal owns them. Only the **live region** and the
  **alternate-screen overlays** (Ctrl+O, the full-screen pages) are ours.
- **Mouse events are ignored.** `Session::on_terminal_event` handles key,
  resize and paste; mouse capture is never enabled (`crossterm`
  `EnableMouseCapture` appears nowhere). That is deliberate: it keeps the
  terminal's native selection, copy and link-hover working over the
  conversation.

## The reference: miyu's full-screen TUI

The local Miyu tree keeps the previous terminal head on `proto/tui-demo`
(`tui-demo/src/…`), and it is the model this page borrows from. It is a
**full-screen** app, which is what makes its interactions possible:

- **Ownership.** Alternate screen, raw mode, `EnableMouseCapture`,
  bracketed paste, focus reporting, kitty keyboard enhancement. It scrolls,
  selects and copies itself; `Ctrl+Z` restores the terminal, suspends, and
  re-enters on `fg`; opening `$EDITOR` yields the terminal, runs the editor,
  and re-enters, with the frame left in place.
- **A piece carries its target.** Markdown parsing emits
  `Piece { text, style, link: Option<String> }`; `links.rs` finds bare
  `http(s)://` and `file://` URLs, and a `title (url)` line relinks the whole
  title. A rendered row also carries `links: Vec<(from_col, to_col, url)>` and
  `target: Option<Target>` (the expandable entry the row belongs to).
- **The body owns hit testing.** `BodyView::mouse` maps a screen point to a
  row: `Moved` sets `hover` / `hover_link` (the hovered link is underlined),
  a left `Up` on a link returns `BodyAction::Open(url)`, an `Up` on an entry
  returns `Toggle(target)` and anchors that row so the expansion grows
  downward without moving the clicked row, drag selects and copies, and the
  wheel scrolls.
- **Cards.** A line whose non-blank cells all belong to one link becomes a
  **link card**: a cover image on the left (at most one third of the width,
  `link_cover_rows` tall), then an icon and the title in the link dress, the
  site and author dim, the description dim, and a video footer with its
  duration. Every painted cell is the link. The data comes from the core
  (`link.preview`): OpenGraph/meta title, site, description, cover and icon;
  images are content-addressed blobs read back in 512 KiB chunks and cached on
  disk, so a card survives a restart. The fetch is per URL, on demand.
- **Paths.** Miyu does **not** auto-detect bare paths in prose. Markdown link
  targets and `file://` URLs go through `local::resolve` (`file://` stripped,
  `~` expanded, relative resolved against the cwd); clicking a local **text**
  file opens the terminal editor, anything else goes to the system opener
  (`open` / `xdg-open` / `start`). Bare URLs are the only text it links.

The takeaway: hover cards and click-to-expand are not link-parsing features;
they are consequences of owning the screen. A pattern in the text is cheap; a
cell-to-target map at paint time is what needs the screen.

## Design space

| | Inline, terminal-native | Overlay-scoped mouse | Opt-in full screen |
|---|---|---|---|
| Clickable URLs | already | already | already |
| Clickable paths in prose | yes, OSC 8 | yes, OSC 8 | yes |
| Preview card | no | in an app-owned page | inline in the reply |
| Hover underline | no | only in the page | everywhere |
| Click one event to expand | no | in the transcript page | everywhere |
| Native text selection | untouched | untouched outside the page | lost; must reimplement |
| Screen ownership | none | while a page is open | always |
| Fits `CLAUDE.md` invariant 3 | yes | yes (overlays already exist) | replaces it deliberately |
| Screen ownership | none | while a page is open | always |

**Recommendation: A first, then C. B is an optional interim.** The owner has
decided that owning the screen matters — without it, every affordance after
this one is delegated to the terminal — so C is the target, not a contingency.
A is still the right first slice (it is renderer-level and lands in either
mode); B's hit map and mouse dispatch are the same work C needs, so B is worth
building only if C slips far enough that a stopgap is wanted. C itself has its
own page: [`full-screen.md`](full-screen.md).

## A. Clickable paths in the inline view

### Detection

A new pure detector beside `find_urls`:

```rust
/// Byte ranges of path-like tokens in one prose line.
pub fn find_paths(text: &str) -> Vec<Range<usize>>;
```

A token is a path when all of these hold:

- it is one whitespace-delimited word, not already part of a URL;
- it starts with `/`, `./`, `../` or `~/`, **or** it contains `/` and its last
  segment contains a `.` (`src/app/tools.rs`, `docs/ui/links.md`);
- it has at least one non-empty segment, and no scheme (`://`);
- flags (`--path=…`), globs (`src/*.rs`) and `word/word` prose without a dot
  in the last segment stay plain.

Trailing punctuation is trimmed with the URL rule (`.,;:!?'"` and unbalanced
closers). An optional `:line[:col]` suffix is stripped for the target and kept
in the visible text; whether it becomes a fragment (`#L657`) is an open
question below.

### Target

The target is `PathDisplay::file_url` — the same absolute `file://` URI the tool
header already links, percent-encoded, `~` and relative paths resolved against
the session cwd. No cwd (the verbatim policy, a unit test) means no link, which
is the header's existing rule. `file://` is the only scheme added; a path is
never turned into `http://`, and `javascript:`/`mailto:` text is not a path.

### Where it plugs in

- **Assistant prose.** `ui::inline::inline_spans` is the single funnel; path
  spans take the link dress and the carrier exactly as URL slices do. The
  renderer needs the session's `PathDisplay`, which it does not have today:
  thread an `Option<&PathDisplay>` through `ui::assistant::AssistantRenderer`
  and `ui::message::message_lines` (the public `message_lines` keeps its
  signature and passes `None`, so tests and the API-surface lock are stable;
  a `message_lines_with_paths` is the TUI's entry).
- **Tool output.** `ui::tool` already has the `PathDisplay`; mark paths in the
  peek/expanded output rows too (`ls`, compiler errors). The output's bytes
  and columns do not change — only carriers and the link dress on the marked
  slice.
- **User/shell text** stays literal, as it does for URLs.
- **Tool headers** are unchanged; they already link, and the corner rows stay
  plain.

### Prefix stability

The trailing line can still grow while a reply streams: `src/ap` can become
`src/app.rs`. The URL machinery already answers this with
`links::has_forming_url` — the stream commit withholds the growing line while a
target can still change. Path detection needs the same predicate
(`has_forming_path`): a path-like token that touches the line's end, or that
ends in a separator, withholds the line. A committed row therefore never
changes its target. A path sealed by whitespace or punctuation commits
progressively, like a sealed URL.

### Tests and smoke

- `links.rs`: the detector's positives and negatives, trailing punctuation,
  `:line` handling, percent-encoding, no-cwd, no-scheme.
- `ui/tests/inline.rs`: carriers across every wrap of a path in prose, code,
  headings, lists, quotes and tables; the visible text byte-identical.
- `ui/tests/tool.rs`: the same for peek and expanded output rows.
- Smoke: extend the dummy reply with `src/app.rs:42`; a raw-stream phase (the
  Phase 87 shape) asserts an `OSC 8` open carrying the resolved
  `file://…/src/app.rs` while the pane shows the unchanged text.

### Limits

- No existence check: detection happens in the renderer, which is pure and
  must not touch the filesystem. A linked path that does not exist is the
  user's terminal's problem (it will say so), and a bare `README` stays plain.
- Scheme-less `www.…` and bare file names stay plain, as URLs do.
- OSC 8 remains a terminal capability; the existing `ALTER_ZERO_HYPERLINKS`
  gate covers both kinds.

## B. Overlay-scoped mouse and the `/links` page

Alternate-screen overlays already exist (`term::enter_overlay` /
`draw_overlay` / `exit_overlay`), already get their cells through the link
emitter, and already have a scrolling row model (`ui::transcript::TranscriptCache`).
This design makes the **page** mouse-aware while it is open, and leaves the
conversation exactly as it is.

### Mouse capture lifecycle

`EnableMouseCapture` / `DisableMouseCapture` are written by
`term::enter_overlay` / `exit_overlay`, beside `EnterAlternateScreen` /
`LeaveAlternateScreen`, and on every restore/panic path that already emits
`LeaveAlternateScreen`. Capture is therefore on only while an app-owned page is
on the screen; the conversation keeps native selection, copy and terminal link
hover. A `Ctrl+O`/page exit that is already in the code (`exit_overlay`,
`overlay_return_repaint`) is the one place to get this wrong, so it is the one
place to test.

### The hit map

The transcript cache already builds a `Vec<Line>` and knows which row range
each history item occupies. While the overlay is open, build a parallel
`Vec<RowHit>` (discarded on close, never built inline):

```rust
struct RowHit {
    /// The expandable thing this row belongs to, if any.
    target: Option<usize>,          // history item index
    /// Byte/column ranges whose cells carry a link carrier.
    links: Vec<(u16, u16, String)>, // decoded from the spans' carriers
}
```

Links come from the carriers already in the line's span styles (decoded by the
same `links` module that encodes them), so a link drawn in the transcript is
clickable with no second source of truth. Targets come from the item ranges
the cache already tracks; a row of a tool cell and a row of its output both map
to the same item.

### Interactions

- `Moved` → set `hover: Option<(row, link)>`, repaint the hovered link run
  underlined (the overlay is redrawn every frame anyway; the underline is a
  style patch, not a text change).
- Left `Up` on a link → `open_link(url)`; this is the same opener policy as A
  (OSC 8 lets the terminal do it; here we do it ourselves because the mouse is
  ours): `http(s)` via the system opener, `file://` via the OS (the editor
  handoff is a separate question), errors as a one-line hint.
- Left `Up` on a row with a target → toggle that item's expansion set; keep
  the clicked row's screen position (the overlay already stores a scroll
  offset, so anchor `offset + row` and let the added lines grow downward).
- Wheel → scroll the page, as the keyboard already does.
- Drag → **native selection is gone while capture is on**. Phase B1 leaves
  selection alone and documents Shift+drag (most terminals bypass capture with
  Shift); B2, if wanted, implements drag-select + copy over the row plain text
  (Miyu's `selected_text` is the reference), using `Ctrl+C`/`c` like the rest
  of the UI.

`Ctrl+O` keeps its keyboard behaviour untouched; mouse is an additive layer.

### The `/links` page and cards

A new overlay page (`/links`, or `Ctrl+O`'s sibling) lists the **targets in the
current conversation** — URLs and paths found by A's detectors plus the
file-tool headers — deduplicated, newest reply first. It is the natural home
for cards:

- Selecting an entry and pressing `Enter` opens it; `p` (or `Enter` on the
  preview column) fetches the preview; `r` retries a failed one.
- A fetched card renders as in the reference: cover image on the left (at most
  one third of the width, a fixed row height), icon + title in the link dress,
  site · author dim, description dim, video duration in the footer. Text-only
  when there is no cover or the terminal cannot show images. The existing
  `images::fitted` / `ImageStore::stamp` pipeline already runs in
  `draw_overlay`, so a cover is the same picture path as a pasted image.
- Because the page owns the screen, a card that arrives late may change the
  page's height; the page's scroll offset is adjusted to keep the selected row
  in view. This is exactly what an inline commit cannot do.

### Fetch policy

Previewing a URL is a network request on the user's behalf, so it is
**explicit, never prefetched**:

- Only `http(s)`; `file://`, other schemes, and text that is not a URL are
  never fetched. The URL itself is untrusted model content.
- Request from a worker (the clipboard/git-diff/checkpoint pattern), not the
  event loop: `GET`, 5 s timeout, 1 MiB body cap, ≤ 3 redirects, no cookies,
  no auth headers, a generic user agent. `text/html` only.
- Parse `<title>`, `og:title`, `og:description`, `og:site_name`, `og:image`,
  `og:type`, `og:video:duration`; fall back to `description`. A page with
  nothing is a text-only card, not a failure.
- Refuse private/loopback/link-local addresses (the model can write any URL;
  a preview must not become a probe of the user's LAN). A redirect is
  re-checked the same way.
- Cache under `~/.zeroth/cache/link-cards/`: one JSON index keyed by the URL
  (hash) and content-addressed image blobs, with a size budget and an age cap
  (the existing image-cache pruning pattern). Cards survive restarts; a
  failed fetch is remembered for a short cooldown, not forever.
- A visible one-line notice when a fetch fails, and a `NO_PREVIEW`/setting
  switch if a user wants the feature off entirely.

### Tests and smoke

- A pure hit-map test: a transcript with a message, a tool cell and a link;
  row → target/link lookups at boundaries, after wrapping, after expansion.
- An App/overlay test for click dispatch: a synthetic `MouseEvent` at a row
  runs exactly one `Open`/`Toggle` and no keyboard action.
- Card parsing and policy are unit tests over fixture HTML (title/OG/relative
  image URL, non-HTML, oversized body, redirect to a private address).
- Smoke: entering Ctrl+O writes `\x1b[?1000h` (and exit `\x1b[?1000l`) on the
  raw stream; a `tmux send-keys -M` click on a known row opens the expected
  target (a stand-in opener records its argv, the Phase 113 pattern). If the
  tmux path proves flaky, the pty harness writes the SGR mouse sequence
  directly.

## C. Opt-in full screen (the target)

The full-screen mode is designed on its own page:
[`full-screen.md`](full-screen.md). In short: a screen-owning viewport over the
same `App`, `ui::transcript` as the frame builder, its own scroll model,
mouse capture and a per-frame hit map, selection/copy, and the alternate-screen
lifecycle. With the screen owned, cards can be drawn inline in a reply and a
click can toggle any entry; hover underlines work everywhere. It is the target
because it removes the palm-up dependency on the terminal for every future
interaction, and it replaces invariant 3 with a scroll contract of its own.
It lands as an opt-in mode first and becomes the default only on parity.

## Phasing

1. **A — paths — done.** Detector (`find_paths`), `file://` targets through
   `PathDisplay`, prose/code marking, and the `has_forming_path` stream hold;
   unit tests in `links.rs` and the inline renderer. No mouse, no new state.
   It landed in both viewport modes.
2. **C — the screen-owning mode — skeleton, mouse, scrolling and selection
   done.** See [`full-screen.md`](full-screen.md). Still open there for this
   page: click-to-expand and the preview card.
3. **Rich links in full mode — cards done (text + cover).** A lone URL line in
   an assistant reply becomes `● site — title` plus a dim description, and a
   cover thumbnail (≤ 1/3 width, ≤ 6 rows, aspect kept) when `og:image` was
   fetched and the terminal can draw pictures; the image store loads it from
   the card cache at paint time. The fetch is one worker thread per URL (at
   most eight in flight), 5 s/512 KiB/`text/html` for the page and
   2 MiB/`image/*` for the cover; hosts are refused after **resolution**
   (`host_is_private`: the string rules plus every address the name answers,
   so `nip.io`, IPv6-mapped and hex spellings are caught), and a redirect is
   re-checked the same way. Cards are cached across restarts (covers under a
   32 MiB oldest-first budget); the `/links` page remains.
4. **B — only if needed.** Overlay-scoped mouse and a `/links` page remain a
   useful stopgap for inline mode, and their hit map is a subset of C's, but
   they are not the target.

## Open questions

- **Fetching at all.** Is a preview worth any outbound request? Default off,
  fetched only on an explicit key, is the proposed answer; a build that never
  fetches keeps a text-only page.
- **`path:line`.** Do we link to the file only, or to `#L657`? The fragment is
  honoured by some editors and ignored by others; the file-only target is the
  safe default, and "open at line" is an app-level opener (editor handoff)
  feature.
- **Editor handoff.** Miyu opens local text files in `$EDITOR` by yielding the
  terminal. Zeroth has no editor handoff today; opening the system default is
  the smaller first step.
- **Selection inside the page.** Own selection/copy, or Shift-drag and be
  done?

## Non-goals

- Rewriting committed scrollback. It cannot be done, and no design here
  pretends otherwise.
- Unfurling every URL in a reply. A card is drawn only where the app owns the
  screen and only after the user asks for it.
- Fetching URLs at render time. The renderer is pure, and a preview is an
  explicit action.
