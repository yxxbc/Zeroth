---
name: Zeroth about page
description: A broadcast information service, built on a fixed cell grid.
colors:
  ground: "#07080A"
  panel: "#0C0E11"
  cell: "#161C24"
  cell-major: "#2A3340"
  rule: "#313A45"
  rule-soft: "#1F262F"
  ink: "#EDE7D6"
  ink-2: "#C6C2B4"
  ink-3: "#9BA3AD"
  accent: "#5FE0DC"
  status-ok: "#8FD08A"
  status-warn: "#E2B45C"
  status-bad: "#E0705F"
typography:
  display:
    fontFamily: "Zeroth Broadcast, PingFang SC, Hiragino Sans GB, sans-serif"
    fontSize: "clamp(2.6rem, 7.4vw, 5.4rem)"
    fontWeight: 400
    lineHeight: 1.06
    letterSpacing: "0.01em"
  section-head:
    fontFamily: "Zeroth Broadcast, PingFang SC, Hiragino Sans GB, sans-serif"
    fontSize: "clamp(1.6rem, 3.4vw, 2.6rem)"
    fontWeight: 400
    lineHeight: 1.28
    letterSpacing: "0.01em"
  body:
    fontFamily: "PingFang SC, Hiragino Sans GB, Source Han Sans SC, Noto Sans CJK SC, Microsoft YaHei, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.8
    letterSpacing: "0em"
  code:
    fontFamily: "ui-monospace, SF Mono, SFMono-Regular, Menlo, Consolas, Liberation Mono, monospace"
    fontSize: "12.64px"
    fontWeight: 500
    lineHeight: 1.75
    letterSpacing: "0em"
  label:
    fontFamily: "ui-monospace, SF Mono, SFMono-Regular, Menlo, Consolas, Liberation Mono, monospace"
    fontSize: "12.8px"
    fontWeight: 400
    lineHeight: 1.5
    letterSpacing: "0.04em"
rounded:
  flat: "0px"
spacing:
  gutter: "clamp(16px, 4vw, 56px)"
  hero-y: "clamp(34px, 5vw, 62px)"
  section-y: "clamp(30px, 4.4vw, 52px)"
  row-y: "17px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.ground}"
    rounded: "{rounded.flat}"
    padding: "0 18px"
    height: "46px"
  button-primary-hover:
    backgroundColor: "#7BEBE7"
    textColor: "{colors.ground}"
  cell-field:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink-2}"
    rounded: "{rounded.flat}"
    padding: "14px"
---

# Zeroth about page — design system

This records the page at `about.html`. The terminal application's own design
document is `docs/DESIGN.md`; the two share a brand, not a medium. Everything
below is evidenced by the shipped file.

## Overview

The page is a broadcast information service rather than a page *about* one.
Its ceiling is the category default — a centred headline, a row of equal
feature cards, a product screenshot below — and it refuses that by borrowing
teletext's topology: a masthead that names the service and a page number, a
fixed character grid every panel edge lands on, numbered sections that read as
pages, and an index that is the navigation.

Persuade surface, Simplified Chinese, one file, no build step, no external
requests except two GitHub links.

## Colors

A near-black receiver ground carrying a real material: a 40-column grid at
`--u` with a seam every 8 cells (`cell`, `cell-major`) and a 2px-period
scanline layer, both fixed to the viewport. Panels sit on that grid one step
up (`panel`); they get a faint top-down falloff and a single drop shadow.

Text is a neutral cream scale — `ink` for headings, `ink-2` for body,
`ink-3` for labels and metadata. Nothing is tinted warm; the ground is very
slightly cool (`#07080A`, R<G<B) and the text keeps its own temperature.

`accent` is the only saturated colour, in two tiers: three filled fields (the
two copy controls and the meter bars, ~93% of its ink) and four single lit
marks (the pager, the current-page marker, the four section numbers, the
transcript's prompt glyph and spinner). Status green / amber / red appear
only inside the transcript, where they carry meaning rather than decoration.

Contrast: every text role ≥ 4.5:1 on both `ground` and `panel`
(`ink` 16.2:1, `ink-2` 11.2:1, `ink-3` 7.9:1, `accent` 12.6:1).

## Typography

One display face, one body face, one mono face, each with a job.

`Zeroth Broadcast` is 得意黑 (Smiley Sans, atelierAnchor, SIL OFL 1.1),
subset to the 79 glyphs this page sets in it and renamed per the licence's
Reserved Font Name clause, embedded as a base64 `@font-face` so the file stays
self-contained. It carries the page's headings only: an oblique broadcast cut
at 86px is a title voice, not a reading face. Body copy stays on the system
Chinese sans, where a gig face would be tiring over paragraphs.

The mono face carries code, numerals and short labels — never prose. Labels
track at `0.04em`; nothing else tracks.

## Layout

A 40-column grid is the skeleton, and the page states it visibly rather than
drawing boxes on top of it. The hero splits at column 24: copy and the install
action in the left 23 columns, the live session window in the right 17.
Sections begin with a page number in the first three columns, a heading across
4–27, and a short qualifier in 29–40.

Measured rhythm: sections pad `clamp(30px, 4.4vw, 52px)` top and
`clamp(46px, 6vw, 72px)` bottom, so a section's last row clears its own rule
by 67–73px. Rows inside a section are 17px apart with a `rule-soft` divider.

Breakpoints: 980 collapses the hero to one column and pairs the meters two-up;
620 drops the index to a stack, moves step numbers beside their titles, and
lets both command fields scroll horizontally in place.

## Elevation & Depth

Depth is the grid, not shadow. Exactly one element carries a shadow — the
session panel, `0 18px 44px -28px #000000CC` — because it is the one surface
that sits above the page rather than in it. Everything else is flat on the
ground, separated by its own border or by the cell material.

## Shapes

No radius anywhere: teletext has no curves, and a single `border-radius` would
soften the whole world into a card layout. 1px rules are the only line weight;
panels are 1px boxes at `rule` on `panel`.

## Components

**Command field.** A bordered `panel` holding the install command at 12.64px
mono in one unbroken line. At desktop widths the whole line fits its column
and reads in one pass; at ≤620px it scrolls horizontally with a note directly
beneath stating that swiping reveals the rest and that the copy is complete —
a URL broken mid-token reads as a broken command, which a page whose action is
"copy this" cannot afford. Both command fields (hero and install section)
follow the same rule; they must never diverge.

**Copy control.** The only filled element on the page: `accent` fill, `ground`
label, 46px tall, flat corners. Hover lightens to `#7BEBE7`; the live region
beneath it reports the clipboard's verdict and explains the manual fallback.

**Session window.** A `panel` with a mono header (title left, model right) over
the transcript. The transcript is literal text, not a screenshot: `●` opens a
tool call, `⎿` opens its result, status colours carry meaning, and the closing
receipt line is the turn summary. It is labelled 示例会话 in page copy — its
string shapes are copied from the renderer, its file paths are illustrative.

**Meter strip.** Measured counts (22 commands, 11 themes, 6 mascots,
9 spinners) as mono label, Chinese label, tabular numeral and an eight-segment
bar filled proportionally. The world's reading for a measurement, not four
equal cards.

**Index.** Four cells, one per page, each a page number over its name. The
current page is marked by `aria-current` driving an inset 2px accent rule and
the accent numeral, updated by scroll position; with no JavaScript the strip
carries no stale highlight.

## Do's and Don'ts

- Do land every panel edge and section start on a grid line. The grid is the
  page's skeleton and its material at once.
- Do keep the accent to the two tiers above. A third usage dilutes it into
  decoration.
- Do set Chinese headings in `Zeroth Broadcast` and Chinese body copy in the
  system sans. Mixing them mid-line, or setting a paragraph in the display
  face, breaks the reading register.
- Do keep the command one unbroken line at every width, with a note wherever
  it scrolls.
- Don't add radius, gradients as decoration, or a second shadow. One surface
  floats; the rest sits.
- Don't let a command field wrap mid-token. If it will not fit, scroll it and
  say so.
- Don't present illustrative paths as a real session. Label it, or use a real
  one.
- Don't reuse the terminal app's chrome. The brand is shared; the medium is
  not.
