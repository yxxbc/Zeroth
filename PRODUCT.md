# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Developer tooling: people who already work in a terminal and are deciding
whether Zeroth earns a slot beside the agents they run today. The about page
is also forwarded to colleagues who do not use a terminal, so it must be
readable cold. Confirmed with the owner: 面向全世界的用户 (a worldwide
audience), Chinese copy retained.

## Product Purpose

Zeroth is an AI coding agent that runs in the terminal as one Rust binary: it
reads code, makes edits, runs the checks, and shows each step. Success for
this page is a reader deciding to install it and run one real task — not
admiring a landing page.

## Positioning

The work is shown as it happens, in the shape the app really paints it: a tool
call is `● name(args)` with a `⎿` result row, a finished turn commits
`Worked for Ns · ≈Nk tokens`. Most competitors advertise autonomy; Zeroth
shows the transcript.

## Operating Context

macOS (Intel / Apple silicon) and Linux (x86_64 / arm64). Installed by one
`curl … | sh` line that verifies a SHA-256, or from source. Configured through
`/login` and `/model`; sessions are per-directory; a session resumes later.
Read at night, in a dim room, on the same screen the terminal occupies.

## Capabilities and Constraints

Real capability counts that may be stated: 22 slash commands, 11 colour
themes, 6 mascots, 9 spinner styles, 12 ways to connect a provider (6 of them
subscription sign-ins), four permission modes (`manual`, `edit`, `auto`,
`master`). Products: one binary, no alternate-screen TUI by default, offline
demo with no provider configured.

Constraints: no fabricated benchmarks, customer logos, or pricing. The
`assets/mascot-crest.png` (384×351, transparent) and `assets/banner-v2.jpg`
are shipping build assets — `docs/ui/mascot.md` pins the PNG's size to
`ui::MASCOT_ICON_PX` and a test reads its IHDR, so neither may be overwritten
for the web page. Page-specific imagery belongs in separate files.

## Brand Commitments

The mascot is the crested hatchling (`crest`), purple with cyan eyes and wing
membranes, usually beside a glowing laptop under a midnight-blue lake sky with
a cyan zero ring. The name is `Zeroth`, never lowercased in prose. Existing
palette roles in `src/ui/palette.rs` are the product's own colours
(accent `#89DCEB`, purple `#CBA6F7`, warning `#F4B66A`, success `#A6E3A1`,
error `#F38BA8`).

## Evidence on Hand

`README.md` / `README.zh.md` (front door), `docs/` (44k lines, a page per
area), `CHANGELOG.md`, `PRODUCT.md`. Real transcripts can be taken from the
source: `src/ui/tool.rs` (header and `⎿` rows, `  ⎿  ` gutter), 
`src/ui/file_cell.rs` (`Read N lines`, `Updated path (+A -D)`), 
`src/ui/status.rs` (`Working… (9s · ↓ 1.2k tokens · Thinking for 3s · esc to
interrupt)`, `Worked for 21s · ≈2.4k tokens`), `src/app/commands.rs` (the
COMMANDS table).

## Product Principles

- Show the work, not the promise: every claim on the page traces to a real
  string or count in this repository.
- The reader stays in control: permission modes, approval prompts, and
  trusted hooks are part of the pitch, not small print.
- Say what is not done. Unverified and undone work is reported as such.
- One binary, one terminal: no account, no dashboard, no second app.

## Accessibility & Inclusion

Body and UI text meet WCAG AA (≥4.5:1). Motion is a single authored moment and
is fully disabled under `prefers-reduced-motion`. Keyboard focus is visible on
every control. The page must survive 390–1600 px without horizontal scroll.
