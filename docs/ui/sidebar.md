# The sidebar

The screen-owning mode's **right** column: the brand, the session's facts, and
what is running — the things the inline viewport spreads between its banner and
its footer. It exists for the same reason the rest of the mode does: a painted
inline row belongs to the terminal, so there is no column to put anything in
there.

```
❯ why does the parser reject this line               │Zeroth v0.21.0
                                                      │a coding agent that lives in
● The lexer peeks one token ahead, and the tag it     │your terminal
  records is the one the parser sees later.           │
                                                      │session
                                                      │model deepseek-v4 · openrouter
                                                      │cwd ~/Projects/zeroth
                                                      │context ████░░░░ 12.4k/160k
                                                      │mode manual
                                                      │
                                                      │running
                                                      │▸ add the sidebar
                                                      │· write its docs
                                                      │◯ explore: find the callers
                                                      │⌁ 1 running
```

**It is opt-in, and off by default.** The column is a `/settings` row
(**Sidebar**) — `ZEROTH_SIDEBAR=1` for one run — because a new session should
spend every column on the conversation until the reader asks for the extra
panel. With the row off the frame lays out exactly as it did before the
feature existed: no column, no seam, and the brand rides the banner.

**It sits on the right** so the conversation — and the composer's caret — start
at **column 0**, where an inline composer and the shell's own prompt already
put them. A left-edge column put the whole composer thirty columns in: the
caret sat where the prompt was not, and the shell prompt after a quit no longer
lined up with the conversation.

## Responsive by three bands

Once the row is on, the column follows the convention every terminal sidebar
keeps — full size on a wide screen, narrower in the middle, gone when the
conversation needs the room. The width is the **whole** column, its one-cell
seam included:

| terminal width | sidebar |
| --- | --- |
| ≥ 112 columns | 30 columns |
| 96–111 | 24 columns |
| < 96 | hidden |

The bands are constants in [`src/ui/sidebar.rs`](../../src/ui/sidebar.rs)
(`WIDE_SCREEN_COLS`, `MIN_SCREEN_COLS`, `WIDE_COLS`, `NARROW_COLS`), read
through two functions ([`ui::sidebar_width`](../../src/ui/sidebar.rs) for the
band policy, `ui::sidebar_columns` for "the bands, unless the row is off") that
the frame and the conversation's wrap width both call — so a column can never
appear without the rows being rebuilt for it. That matters because the wrap
width is part of the frame cache's key: a sidebar that materialised on its own
would re-wrap every row under the reader.

A hidden sidebar costs the conversation **nothing**: the middle band narrows
the column rather than clipping it, which is why it exists rather than a column
that shrinks toward zero.

## Sections are a registry

The modularity is the point. A section is one entry in `SECTIONS`:

```rust
pub struct Section {
    /// Stable id — what a settings row, a test or a log names it by.
    pub id: &'static str,
    /// The rows for a `width`-column sidebar; empty means "nothing to say".
    pub rows: fn(&App, u16) -> Vec<Line<'static>>,
}
```

Stacking, spacing and truncation live in one place ([`sidebar::lines`]);
a section only answers "what are my rows". Adding a module — a diff summary, a
git status, a token ledger — is a new `fn` plus a new entry, never an edit to
the layout. A section that has nothing to say returns no rows and is dropped
**whole**, heading included, so the column never shows an empty title.

The stack is ordered by importance and cut to the screen's height from the top:
the brand is the last thing that may disappear.

## What each section carries today

The seam is the column's **left** cell, one dim rule separating it from the
conversation; the sections' rows are laid out in what remains
(`SIDEBAR_SEAM_COLS`).

- **`brand`** — the mascot's graphic where the terminal can show one (the
  banner's own placement, sized for the column and dropped rather than
  degraded when the terminal has no graphics), then the name, the version, the
  one-line slogan, and — on the offline demo — the `Connect a model → /login`
  hint the banner carries.
- **`session`** — the model (the viewed agent's own, inside an agent session
  view), the working directory, a context meter over its numbers, and the
  permission mode. The footer's chain, read vertically.
- **`running`** — the task checklist (in flight marked `▸`, waiting `·`,
  completed left out), the running subagents, and a background-shell count.
  What the inline mode crowds into the status strip and the roster.

## What it costs

The frame reserves the column before anything else is laid out, so the
conversation's wrap width — and therefore every cached row — depends on it.
That is why the visibility is a pure function of the setting and the screen
width and nothing else: no hidden per-session state, and the same screen with
the same setting always draws the same column. Flipping the row changes the
wrap width, which is already part of the frame cache's key, so the next frame
re-wraps by itself.

The banner is **not** drawn when the sidebar is: the sidebar *is* the brand
block, and printing it twice would spend rows saying one thing. The
conversation then starts at row zero, with no banner and no spacer
(`ui::banner_tail` returns its tail unchanged for an empty banner).

## Where this is covered

- `ui::tests::sidebar` — the three bands, the registry (a section's absence
  leaves no heading behind), the brand's wording, the fact rows, the truncation
  inside the column, what counts as "running", the height cut, and the task
  marks.
- `ui::tests::sidebar::the_column_is_opt_in_and_the_setting_gates_the_bands` —
  off by default, and each band gated by the setting.
- Smoke Phase 156 — a 140-column pane with the row off asserts no column and the
  banner's title, then a pane launched with `ZEROTH_SIDEBAR=1` drives 140 and 100
  columns asserting the seam and the name are there, then a narrow pane and
  asserts they are not.
