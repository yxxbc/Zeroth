# Accessibility

Zeroth is a terminal application. "Accessibility" here means: what a screen
reader sees, what a low-vision or photosensitive user can turn off, what works
in a monochrome or a light terminal, and what to do when the interface is a
barrier rather than merely awkward.

This page is the one to report against. It is also the page GitHub shows in
the **Accessibility** tab of the repository overview.

## Report a barrier

Open an issue titled `accessibility: <the barrier>`, or mail
<xynrin@icloud.com> if the issue itself is the barrier. Say which terminal and
OS you use, which part of the interface got in the way, and what you expected.

A report against anything below — a screen reader reading the conversation as
one run, a theme with unreadable contrast, an animation that cannot be turned
off — is a bug in the sense `CONTRIBUTING.md` means, and it is worth an issue
even if you cannot say how to fix it.

## What the interface assumes

- **A terminal, 80 columns and up.** Below 80 the live region sheds rows before
  it flows ([`docs/ui/view-flow.md`](docs/ui/view-flow.md)), but the design
  target is a standard terminal. There is no GUI and no browser surface.
- **A monospace font with a Unicode fallback.** Braille (the default spinner's
  track), box-drawing (the diff and the sidebar) and a handful of dingbats (the
  brand marks) are used. A font that lacks a glyph shows a placeholder box and
  nothing breaks; the **`line`** spinner style is pure ASCII for exactly this
  case.
- **Colour inside the terminal.** The conversation is also readable without it
  — a tool's OK/failed state is carried by its verb and its `⎿` rows, not by
  the bullet alone — but the theming is colour-first
  ([`docs/ui/theme.md`](docs/ui/theme.md)).

## What can be turned off

Every animation and every colour decision has an off switch, and none of them
changes what the conversation contains.

| Barrier | Switch |
| --- | --- |
| Motion — the spinner, the shimmer, the running bullet's pulse | `/spinner` → **`line`** (a static ASCII bar), saved per directory in `spinner.json` |
| Streaming chain-of-thought | `/settings` → **Hide thinking**; `ZEROTH_SHOW_THINKING=0` |
| Pictures in the conversation | `/settings` → **Show images** off; `ZEROTH_IMAGES=0` |
| The banner's graphic mascot | `/mascot`; a text art is used wherever the terminal cannot draw a picture |
| The sidebar column (a second layout, not a colour choice) | `/settings` → **Sidebar** off, the default; `ZEROTH_SIDEBAR=0` |
| Tips hanging off the status line | `/settings` → **Show tips** off |
| Colour, for maximum contrast | `/theme` → **`ansi`** — the terminal's own sixteen colours, so the terminal's contrast settings decide, not ours |
| Light terminals | `/theme` → **`latte`** (Catppuccin's light flavour) |
| Hyperlinks | `ZEROTH_HYPERLINKS=0` — no OSC 8 sequences are emitted at all |
| Mouse capture | never enabled in the inline mode; only while a full-screen page is open (`docs/ui/full-screen.md`), and released on close |

Settings chosen with `/settings` persist per directory
([`docs/state/per-directory-state.md`](docs/state/per-directory-state.md));
`/mascot` and `/spinner` persist the same way, and `/theme` is one choice for
the user.

## What a screen reader gets

Terminals expose their screen buffer, and the app paints a normal one. Two
consequences:

- **The conversation is text, top to bottom.** Tool cells, diffs and tables are
  characters in the buffer like prose. A reader that reads the buffer linearly
  gets the same order a sighted user reads.
- **The frame is repainted, not scrolled, when a view takes the screen.** The
  Ctrl+O transcript, the pickers and the full-screen mode paint into the
  alternate screen; a reader attached to the main screen will not see them
  until the page closes. A reader that follows the alternate screen sees them
  normally.

The inline default exists partly for this: finished turns are **committed to
the terminal's own scrollback**
([`docs/architecture/async-rewrite.md`](docs/architecture/async-rewrite.md)),
so the conversation is ordinary terminal history that a reader, `tmux
capture-pane`, or a copy-paste can reach. `--inline` (or `ZEROTH_TUI=inline`)
forces that mode.

The **Ctrl+O** transcript is the complete record — every tool's full output,
every thought, every compaction — reachable without a mouse, and `q` or Esc
leaves it ([`docs/ui/full-screen.md`](docs/ui/full-screen.md),
[`docs/ui/overlay-repaint.md`](docs/ui/overlay-repaint.md)).

## Known barriers

These are real, open, and not yet fixed:

- **No reduced-motion detection.** The app does not read the platform's
  reduced-motion setting; motion is turned off by choosing the `line` spinner,
  not automatically. The shimmer on the status verb keeps running even with
  the `line` spinner — it is not a switch of its own yet.
- **No key remapping.** The shortcuts are fixed
  ([`docs/ui/shortcuts.md`](docs/ui/shortcuts.md), `?` in an empty composer).
  A terminal-level remap works, but the app does not offer one.
- **Colour is a channel, not the only one.** The permission mode at the
  footer's right edge warms from dim to red as it allows more, and the
  word itself is always there; but a user who cannot see colour must read
  the word. The same is true of the context gauge.
- **The live region is a single repainting row.** A screen reader reading the
  status line may re-read it on every frame. Working in `--inline` and
  waiting for a turn to finish avoids this; there is no "announce once" mode.
- **No audio or haptic feedback.** There is none by design, and no plan for
  it.

If one of these is the reason you cannot use Zeroth, say so in an issue — a
barrier someone names is worth more than one we guessed at.

## Where this page is covered

Nothing enforces this page mechanically. A change that removes a switch from
the table above, or that makes an animation unskippable, is a change to this
file in the same commit.
