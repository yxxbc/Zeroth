# Naming — finishing the Zeroth rename

Date: 2026-11-14

## Goal

Zeroth is a fork of Alter Zero. The rename reached the parts a user sees first
— `Cargo.toml`'s package and binary, [`crate::APP_NAME`](../../src/lib.rs), the
`~/.zeroth/` config home, `prompts/zeroth.md`, and the one-time
`migrate_from_alter_zero()` that copies three identity files out of the old
home — but stopped there. What remained was a second, stale identity: 73
`ALTER_ZERO_*` environment variables, the session temp root
`{tmp}/alter-zero-{uid}`, the `.alter-zero-paste-*` staging prefix, the
`alter-zero/{version}` User-Agent, and a long tail of docs, smoke assertions
and test fixtures that still spell the old name. Several smoke phases asserted
a literal `Alter Zero` banner that [`APP_NAME`](../../src/lib.rs) no longer
paints, so those were already failing for the right reason.

This change finishes the rename in one cut: every name the program uses for
*itself* becomes `Zeroth` / `zeroth` / `ZEROTH_*`. It is deliberately a
**breaking change with no compatibility shim** — the old spellings are not
read, aliased or migrated at runtime.

## Decision: one cut, no aliases

The alternative was a dual-read layer (`ZEROTH_*` preferred, `ALTER_ZERO_*`
still honoured). It was rejected: it would leave two names in the code forever,
make every future override ambiguous, and keep the permission and hooks
payloads carrying a field nobody could tell was deprecated. A fork at version
`0.0.0` with no published release of its own has no installed base worth that
cost.

The one-time home migration in `src/tui/config.rs` is **not** part of this
rename and stays: it is a data import from a *previous application's* directory,
not an alias for this one's config home. It reads `~/.alter-zero/` and never
writes it, and the old app keeps working from it.

## The mapping

| Before | After | Applies to |
| --- | --- | --- |
| `ALTER_ZERO_*` | `ZEROTH_*` | All 73 override variables, hook payload fields, docs, smoke phases |
| `alter-zero-{uid}` | `zeroth-{uid}` | `scratchpad::session_root`, `is_session_scratchpad`, prompt/docs layout |
| `.alter-zero-paste-*` | `.zeroth-paste-*` | `clipboard.rs` staging prefix |
| `alter-zero/{version}` | `zeroth/{version}` | Telemetry User-Agent (`TELEMETRY.md`) |
| `alter-zero` (command, path, temp dir) | `zeroth` | CLI examples, smoke fixtures, test paths, docs |
| `Alter Zero` (as the app's name) | `Zeroth` | Header/help/callback/banner docs and smoke markers |
| `.alter-zero/` (as the current config root) | `.zeroth/` | Docs and fixtures that describe the *current* home |
| `git clone https://github.com/linuztx/alter-zero.git` | `yxxbc/zeroth` | `CONTRIBUTING.md` — the repository being developed now |
| Upstream release URLs in `docs/ops/update.md` | `yxxbc/zeroth` | The update channel; `DEFAULT_REPO_URL` already reads `CARGO_PKG_REPOSITORY` |

A `tests/naming.rs` contract test pins the new spellings. It fails before the
cut (production code still returns `alter-zero-{uid}`) and is the gate for the
whole sweep.

## What is deliberately *not* renamed

These name the **upstream project or historical facts**, not this application.
Rewriting them would produce broken links, false attribution and a wrong
legal notice:

- `NOTICE` — the Apache-2.0 derivative-work attribution.
- `README.md`'s Fork note and `CHANGELOG.md`'s "inherited from Alter Zero",
  plus the historical `github.com/linuztx/alter-zero/compare/…` and
  `releases/tag/v0.1.0` link definitions at the foot of the changelog.
- Upstream issue links in `docs/architecture/multithread-core.md` and the
  creator's `github.com/linuztx` homepage in `docs/ui/links.md` and the dummy
  backend docs.
- `NPM_PACKAGE = "@linuztx/alter-zero"` in `src/update.rs`: the name older
  releases were *actually published under*. `is_package_manager_install` and
  the update command both need it to recognise an npm-owned install. It will
  change when a Zeroth npm package exists.
- `migrate_from_alter_zero()` and the `~/.alter-zero/` **source** path it
  reads, `prompts/skills/README.md`'s note on unlicensed personal skills from
  that home, `scripts/release/notes.sh`'s historical
  `**Alter Zero Initial Release**` heading, and `docs/research/harness-security-threats.zh.md`'s
  recorded rollout paths. All are descriptions of the past.
- `src/update.rs`'s `is_cargo_build_dir` fixtures are *not* historical: they
  describe this repository's build tree and are renamed with everything else.

## Breaking changes

- Every `ALTER_ZERO_*` override stops working. Scripts, hooks and CI that set
  one must switch to the `ZEROTH_*` spelling. `docs/architecture/environment.md`
  and `docs/cli/cli.md` carry the new names.
- A resumed conversation's scratchpad moves to a new session root. Files
  written by older sessions under `alter-zero-{uid}` are no longer exempt from
  the write permission prompt and no longer displayed as scratchpad edits.
- The telemetry User-Agent changes; any allow-list keyed on the old string
  must be updated.
- Clipboard staging files get a new prefix; a stale `.alter-zero-paste-*`
  file is simply never cleaned up by the new name's sweep.

## Verification

`cargo test --test naming` pins the contract; `cargo test --lib --` with the
pty/background/exec skips covers the renamed fixtures; `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings` and `cargo doc --no-deps --lib`
are the CI gate. `python3 scripts/docs/zh_parity.py` keeps the bilingual docs
in step, and `bash scripts/smoke/selftest.sh` checks the renamed smoke
helpers. On macOS the 38 Linux-only tests never compile, so the Linux gate
still has to run in CI.
