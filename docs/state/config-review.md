# Reviewing hooks before they run

A hook runs a shell command on its own whenever its event fires, outside the
permission prompt. Neither user nor project hooks run until the user has
reviewed them. The startup review replaces the old `/trust` gate;
`docs/state/project-config.md` describes project discovery and merge rules.

## What needs a decision

| what | where it is read from | the decision | recorded as |
| ---- | --------------------- | ------------ | ----------- |
| every hook that can run | the user's `hooks.json` (`~/.zeroth/hooks.json`, or `ZEROTH_HOOKS_FILE`) **and** the project's `{root}/.zeroth/hooks.json` | trust it, or not | the hook itself, per file |

A hook can run when the runner would select it: a `command` handler with a
command and a supported timeout, under a modelled event, with no `if` condition
and no `powershell` shell (`hooks::HookHandler::runnable_command`,
`hooks::HookHandler::runnable_timeout_secs`). Other handlers are skipped with a
warning regardless of trust, so they are not reviewed. Skills and subagent
definitions are inert markdown on load and stay outside this review.

## The store: `trust.json`

`{config_home}/trust.json` keeps approvals keyed by the absolute path of the
source file, never inside the project: a checkout must not approve itself.
The user's file is absolute too (`config::user_hooks_path`), so a relative
`ZEROTH_HOOKS_FILE` cannot accidentally share trust across working directories.

```json
{
  "hooks": {
    "/home/me/.zeroth/hooks.json": [
      { "event": "PreToolUse", "matcher": "bash", "command": "./guard.sh" }
    ]
  }
}
```

**A hook is trusted while exactly that identity is listed for its file:**
event, matcher, command, and resolved timeout. Absent, empty and `*` matchers
are equivalent; other matchers stay **exactly as the runner reads them**
(`hooks::constraint`), never trimmed. ` Bash` and `Bash` select different
commands; the dialog quotes padded matchers (`PreToolUse · " Bash"`).

A resolved `timeout` changes how long the hook can guard before it fails open,
so editing it requires review. `HookIdentity::timeout_secs` defaults to the
historical runner default when an old record omits it, and the default is not
written out. A nondefault timeout appears in the item's title.
`statusMessage` is display-only and does not affect trust. Moving a hook within
its file asks nothing.

A new identity is **changed** when a recorded hook under the same event and
matcher disappeared from the file; each disappearing identity pairs with one
new identity in order. The dialog shows the former command (`was: cargo fmt`).
When that edit is declined, recording another choice for the file preserves
**the actual old identity, including its old timeout**. It must not rebuild an
old command using the new timeout: doing so would approve a timeout-only edit
without consent. Accepting the edit replaces the old identity.

Every write is a read-modify-write over `trust.json` **as it is on disk when
the decision is made** (`trust::update_store`, the boundary's `persist`). It
updates one file's hooks without replacing another file's approvals. Removed
hooks are dropped unless retained as the old side of an unaccepted edit, so a
removed command added back later requires review.

A malformed active store **fails closed**: no hook trust, a red toast naming
the file, and no overwrite. Decisions made meanwhile hold for this session
only (`trust.json: … — not saved, so this choice lasts this session only`).
The obsolete top-level `mcp` key is a deliberate compatibility exception: its
contents are ignored, without parsing server records, and omitted on the next
successful write. It cannot grant or revoke hook trust. Other unknown fields,
malformed hook records, and invalid JSON remain errors.

### Carrying `/trust` approvals over

Old stores may contain `projects`: project root → file → fingerprint of the
approved bytes. At startup `TrustFile::migrate_legacy` carries over only a
project hooks file whose bytes still match its approval, records its hooks as
trusted, then removes that file's legacy entry. Edited files and other
projects' entries remain untouched. Old hashes for removed configuration
features remain inert; they are never read as hook approvals. The user's own
hooks were not under the old gate and require their own review.

## The review dialog

After queuing the startup banner and before running hooks, the session opens
one hooks page if any runnable hooks are new or changed. It replaces the
composer, uses the hooks picker's frame, takes every key, and hides the cursor
on its highlighted row.

```
──────────────────────────────────────────────────────────────────────────
  2 hooks to review · 1 new, 1 changed
  Hooks run shell commands on their own whenever their event fires, outside
  the permission prompt. Only hooks you trust will run.

  User · ~/.zeroth/hooks.json
  ❯ [✔] Stop  changed
        cargo fmt && cargo clippy --all-targets -- -D warnings
        was: cargo fmt

  Project · ~/repo/.zeroth/hooks.json
    [✔] PreToolUse · bash
        ${ZEROTH_PROJECT_DIR}/.zeroth/hooks/guard.sh

    Trust selected and continue
    Continue without trusting

  Space to select · Enter to confirm · Esc to continue without trusting
──────────────────────────────────────────────────────────────────────────
```

Every hook shows its full command verbatim, word-wrapped rather than cut:
a dangerous tail matters. Items are grouped by source file; changed items
carry an amber badge and the former command. Everything starts selected.

Keys: ↑/↓ move over items and the two action rows, wrapping at the ends;
Home/End jump; **Space** toggles an item; **Enter** confirms on an item or the
first action and declines on the second; **Esc** declines. **Ctrl+C quits** a
startup review without answering. In the review explicitly opened by enabling
**Hooks** in `/settings`, Ctrl+C instead closes it unanswered and the session
continues (`ConfigReview::requested`).

Confirming records the selected hooks as trusted and enables them immediately.
Unchecked hooks remain untrusted. Declining records nothing and runs none of
the listed hooks. Untrusted hooks are asked about next launch; trust them,
remove them, or disable **Hooks** for this directory to stop being asked.

A [herdr](../architecture/herdr.md) pane reports the review as blocked,
with a message such as `Trust hooks: Stop, PreToolUse · bash`. The session
holds its startup hooks (`ModelSession::hold_session_start`) until the review
closes, then releases `SessionStart` with the approved hooks and submits any
held command-line `[PROMPT]`.

### A page that fits

The first-frame review must not flow into scrollback: closing a flowed page
would purge-rebuild and erase the terminal's pre-launch scrollback
(`docs/ui/view-flow.md`). It sheds rows, most complete first:

1. the whole page;
2. without the explanation under the title;
3. every item folded to one title/command row, except the highlighted item,
   whose command remains complete;
4. a shrinking window around the highlight, with overflow counts and the
   highlighted file named in its detail instead of section headings;
5. the highlighted detail capped to three rows, then one;
6. without the title, and finally with a one-row hint.

The two actions and the hint stay on every rung.

## After startup

- **`/hooks`** shows every configured hook. Untrusted ones show `⚠ not trusted`
  and do not run; Enter in the detail page trusts one or stops trusting it.
  Decisions persist and rebind the backend hooks; an active turn finishes
  using its existing set.
- **`/settings` Hooks** turned on while hooks await trust opens this review.

## What is off

- `ZEROTH_HOOKS=0`, or **Hooks** off for this directory: no review, no hooks.
- `ZEROTH_PROJECT_CONFIG=0`: no project layer; user hooks are still reviewed.
- No config home (no `$HOME`, no `ZEROTH_CONFIG_DIR`): approvals last only for
  this session.

## What it does not do

- Trust pins the command and timeout, not what the command reads. Editing a
  trusted script's contents does not invalidate its command's approval.
- No per-turn rescan: hooks are read at startup, so edits require a restart.

## Module split

Pure, unit-tested:

- `src/trust.rs`: `HookIdentity`, hook review and recording, `trusted_only`,
  legacy migration, project-root helpers, and the fail-closed store update.
- `src/app/config_review.rs`: dialog state and keys.
- `src/ui/config_review_view.rs`: rendering and the row-shedding ladder.
- `hooks::HooksOverview`: configured hooks with trust for the browser.
- `herdr::activity`: blocked review status.

Boundary:

- `src/tui/trust.rs`: reading hook layers, migrating approvals, constructing
  the review, recording decisions over the current store, rebinding hooks,
  and releasing startup hooks and the held prompt.
