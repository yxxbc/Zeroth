<!--
Thanks for the change. CONTRIBUTING.md has the full rules; the short version:
one logical change per commit, the smallest check that can fail, and no
`#[ignore]`/`#[allow]` to get green.
-->

## What changed

<!-- One or two sentences a reviewer can hold. -->

## Why

<!--
The problem, not the diff. If a design page exists or was written, link it —
docs/ holds the rationale, CHANGELOG.md the user-visible entry.
-->

## How it was verified

<!--
The checks from AGENTS.md's table for what you touched, and what you saw them
say. Say which platform you ran on: macOS says nothing about the pty, clipboard
or subprocess code, and 37 tests are Linux-only.
-->

- [ ] `cargo fmt --check`
- [ ] the filtered test run for the area (name it): 
- [ ] `cargo clippy --all-targets -- -D warnings` (CI runs it; worth running after a non-trivial change)
- [ ] smoke phase(s), for anything under `src/tui/`, `src/term.rs` or `scripts/smoke/` (name them): 

## Checklist

- [ ] One logical change (a fix and its test are one; a fix and an unrelated docs page are two)
- [ ] A `CHANGELOG.md` entry under **Unreleased**, if a user would notice
- [ ] The `docs/` page for the area updated, if behaviour changed
- [ ] If a page has a `.zh.md` twin: `python3 scripts/docs/zh_parity.py <page>` passes
- [ ] No new `Color::Rgb` outside `ui/palette.rs`, no `chars().count()` for widths
