# AGENTS.md

Instructions for coding agents (Claude Code, Codex, Copilot, Cursor, anything
that reads `AGENTS.md`). `CLAUDE.md` is the architecture map and wins on **how
the code works**; this file wins on **what to run**. Read the `docs/` page for
the area you touch before changing it — what looks arbitrary is usually a bug
someone already paid for.

## Run the smallest check that can fail

Do **not** run `cargo test`, `cargo clippy --all-targets` or the full smoke
suite after every edit. Pick the row for what you touched; CI runs the full
gate on every push. A filtered lib run costs a relink (~9 s), not a rebuild.

Before reading a red run on a new machine, `scripts/preflight.sh --smoke`
prints one line per required tool (the pinned toolchain, rustfmt/clippy, `mbx`,
`tmux`, bash 4+, `setsid`) and names the fix for whichever is missing.

| Touched | Run |
| --- | --- |
| `src/app/<area>.rs` (pure state) | `cargo test --lib app::tests::<area>` |
| `src/ui/<area>.rs` (renderers, theme) | `cargo test --lib ui::` + one smoke phase if visible |
| `src/llm/<provider or wire>` | `cargo test --lib llm::` (live tests only when the user asks — they spend tokens) |
| `src/tui/`, `src/main.rs` | `cargo build && scripts/smoke.sh <id>` + `cargo test --bin zeroth` |
| `src/term.rs` | `cargo test --lib term::` + the smoke phases that paint it |
| public item moved/renamed, `mod.rs` re-export | `cargo test --test api_surface` + `cargo doc --no-deps --lib` |
| `src/cli.rs`, `--help`, arg parsing | `cargo test --lib cli::` + `cargo test --test cli_help` |
| `src/headless.rs`, `src/tui/headless.rs`, anything `-p` | `cargo test --lib headless::` + `cargo test --test headless` + `scripts/smoke.sh 153` |
| `src/pty/`, `src/subprocess.rs` | `cargo test --lib pty::` or `subprocess::` + `cargo test --test detached_exec` (Mac: `cargo build` first) |
| `src/mcp/`, `src/llm/mcp/`, the `mcp` CLI | `cargo test --lib mcp::` + `cargo test --test mcp_cli` |
| a memory-sensitive path | the matching `tests/*_memory.rs` |
| `providers.toml`, `src/llm/config.rs` | `cargo test --lib llm::config` |
| `scripts/smoke/lib.sh`, `scripts/smoke.sh` | `bash scripts/smoke/selftest.sh` |
| `scripts/smoke/phases/` | `cargo build && scripts/smoke.sh <ids>` |
| `scripts/`, `install.sh` | that subtree's suite (CONTRIBUTING *Testing*), not `cargo test` |
| `prompts/`, `CHANGELOG.md` | nothing; prose has no gate |
| a page that has a `.zh.md` beside it | `python3 scripts/docs/zh_parity.py [path]` — fences, code spans, link targets, heading levels vs the English original |
| `scripts/docs/` | `python3 -m unittest discover -s scripts/docs -p 'test_*.py'` |

Filters are substrings: a module path selects everything under it, so
`app::tests` covers every area and costs the same as one file. When the areas
share no prefix, pass several after `--` (`cargo test --lib -- app::tests::login
term::`). Don't leave two filtered runs in one edit; widen the filter. A change
spanning several rows runs their **union** — not the full suite because it
crosses a boundary.

When you want a broad local pass rather than one row: `cargo test --lib --
--skip pty:: --skip background::tests:: --skip llm::exec:: --test-threads=16`
— 6,072 tests in ~49 s on an 8-core Mac. The suite is wait-bound (fixed
sleeps), so more threads buy little and flake the timing tests; the full
`cargo test` is CI's.

## Before you call it done

The gate CI runs: `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`, `cargo test`, `cargo doc --no-deps --lib`. Run fmt, clippy and doc
locally; leave the full `cargo test` to CI unless the user asks or CI is
unavailable. `Cargo.toml` denies warnings, `clippy::all` and `unsafe_code`, and
a broken intra-doc link is an error.

For a cherry-pick or rebase, compare the trees first (`git diff <old> <new>
-- ':!docs' ':!CHANGELOG.md'`); identical code needs no retest. Warm costs on
an 8-core Mac: `cargo check --lib` ~6 s; a filtered lib run
~0.4 s warm / ~9 s after an edit; `cargo test` ~2 m 20 s; one smoke phase
~2–20 s; a release/dist build minutes, only for a shipped-binary task
(`docs/ops/binary-size.md`). `target/` is a ~10 GB incremental cache — never
`rm -rf` it.

## Linux and macOS

CI runs the gate and smoke on Ubuntu, and the gate (no fmt) plus the clipboard
tests and smoke readers on macOS. Linux is the reference; both should be green.

- **macOS has no `setsid`.** Terminal tests start their sessions through the
  built `zeroth` helper. `cargo test` builds it; `cargo test --lib` does
  not, so run `cargo build` first — otherwise ~46 terminal tests fail with
  "no way to give the command a terminal of its own here". Rebuild after
  changing the helper.
- **38 tests are Linux-only** (`cfg(target_os = "linux")`): `llm/exec` 18,
  `pty/probe` 8, `clipboard` 4, `subprocess` 4, `background` 2, `pty/spawn` 1,
  `llm/mcp/transport` 1. A green macOS run says nothing about the `/proc`
  probe, the streamed clipboard, `setsid` detaching or the Linux executor.
  Say in the commit which platform you ran.
- **BSD tools:** `pkill` has no `-s` (and its `-t` matches nothing), `ps` has
  no `sid`, `od` pads its columns. Don't lean on GNU forms.
- **Smoke needs bash 4+** (macOS `/bin/bash` is 3.2). After touching the
  readers or selector, run `bash scripts/smoke/selftest.sh`.
- **The suite's scripts are ported to both platforms** (the `smoke_*` helpers
  in `scripts/smoke/lib.sh`; `docs/ops/smoke.md` *Portability*). A phase that
  calls `stat -c`, `sed -i` or `sha256sum` directly is a bug — on macOS those
  fail with an empty string, which turns an assertion into a false pass
  instead of a red run.
- **A stale incremental link** (`Undefined symbols: … .llvm.…`) after a branch
  switch is codegen: `cargo clean -p zeroth`, never `rm -rf target/`.
- **Network/clipboard/X11 tests are `#[ignore]`d.** Never add `--ignored` to a
  routine run — they need keys, spend real tokens and overwrite the pasteboard.

Two false alarms: "my change broke 46 tests" on a fresh Mac (build the binary),
and "it passes on macOS" for `pty`/`exec`/`subprocess`/`background`/`clipboard`
(half of that code never compiled there). And don't trust GNU tool output on a
Mac.

## Working style

From `CONTRIBUTING.md`: **TDD** — no production code before a failing test you
saw fail for the right reason; the exception is `src/main.rs` + `src/tui/`,
verified by running the app and the smoke suite. **Design before implementing**
a feature or non-trivial change: explore, ask one question at a time, propose
2–3 approaches, agree, write it in `docs/` and update `docs/design.md` when
behaviour changes. **One logical change per commit**; don't bump the version or
edit a released changelog section. Intent in `docs/`, user-visible changes in
`CHANGELOG.md`, README a front door.

## Conventions

- Styling: glyphs/geometry as `const`s in `src/ui/theme.rs`, colours as
  `Palette` accessors in `src/ui/palette.rs` — never `Color::Rgb` elsewhere; a
  new colour is a new role in every theme table.
- Widths through `cols()` (display columns), every cell through
  `term::visible_cells`; the input is a `textarea::TextArea`.
- No `Value` tree for a body you read a few fields from; no whole-picture
  decode for a thumbnail (`docs/architecture/memory.md`, `tests/*_memory.rs`).
- `unsafe_code = "forbid"`; the terminal and pty go through the safe `rustix`
  wrappers, not `pre_exec`.
- Decisions belong in a pure helper the boundary calls — keep logic out of
  `tui/` and `main.rs`.

## Don't

- Don't run the whole `cargo test` or smoke suite "to check" a small edit.
- Don't `rm -rf target/`, change `incremental`, `[profile.*]` or
  `rust-toolchain.toml`; `cargo clean -p zeroth` fixes a stale link.
- Don't run release/publish commands (`scripts/release.sh`
  prepare/build/download, pushes) — those are the user's calls.
  `scripts/release.sh selftest`/`check` are fine when touching it.
- Don't add `#[ignore]` or `#[allow]` to get green; fix the cause or say you're
  blocked.
