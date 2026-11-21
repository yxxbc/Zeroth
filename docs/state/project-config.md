# Project-level `.zeroth` config — hooks, skills, agents

A project can now carry its own Zeroth config in a `.zeroth` directory
at its root — `/home/user/project/.zeroth/` — so different projects load
different hooks, skills and agents. The directory name is `.zeroth`
(the crate's name, the config home's name, and the name the skills walk already
uses), not a second spelling. A project's `.alter-zero/` — the upstream app's
directory — is **not** read, so the two applications never share project
configuration.

The **project root** is the nearest ancestor-or-self of the cwd carrying a
`.git` entry (`project_doc::find_project_root` — the `AGENTS.md`/skills
walk-up, so launching in `repo/src` still finds `repo/.zeroth`), falling
back to the cwd itself in a directory that isn't a repo — **except the home
directory, which is never a project** (`trust::is_project_root`, the
`checkpoint::cwd_scope` instinct): launched in `~` the fallback root would
make `{root}/.zeroth` the user's own config home, and the layer would
rediscover the user's `hooks.json` as "project config" and read
the user's own files twice. In `~` the layer is simply off, while the same
files keep loading as the user layer they are. A per-file identity guard in `tui::trust` catches
the same collision anywhere else (an `ZEROTH_CONFIG_DIR` pointed inside
a project): a project path that *is* the user layer's file is never re-read
as project config.

What the project layer reads:

| file                            | what                                        | merge rule                                   |
| ------------------------------- | ------------------------------------------- | -------------------------------------------- |
| `{root}/.zeroth/skills/`    | project skills (already shipped, unchanged) | first-root-wins walk (`docs/tools/skills.md`)      |
| `{root}/.zeroth/agents/`    | project subagent types (outside the gate, like skills) | first-root-wins walk (`docs/agents/subagents.md`) |
| `{root}/.zeroth/hooks.json` | project lifecycle hooks — **new**           | **union** after the user file (see below)    |

Not among them: a `providers.toml`. A provider block in a checkout could
point a stored key at a host of the checkout's choosing, and unlike a hook it
would do so before anything could be reviewed — the startup capability probe
lists the active provider's models unasked — so provider files are read from
the user's config home only (`docs/providers/user-providers.md`). The app used to read
one from the working directory; it no longer does.

## Nothing in it runs until the user has seen it

A checked-in `hooks.json` runs shell commands on lifecycle events:
**cloning a repository must not earn it code execution.** So every project
hook waits for the **startup review** (`docs/state/config-review.md`), which
opens on the first frame whenever one is new or changed, lists each verbatim,
and records the answer per hook in `{config_home}/trust.json`. `/hooks` lets
the user change that trust later.

The review replaced a `/trust` menu that approved the project's config files
whole, by content hash, and did not review the user's own `hooks.json`, which
the review now covers too.

- **Skills stay outside the review**: a `SKILL.md` is inert markdown until the
  model explicitly loads it, and any command in its body still meets the
  permission gate like every other tool call. (Its one-line description does
  ride the context listing — the standing behavior since `docs/tools/skills.md`.)
- **Subagent definitions stay outside it too**, for the same reasons
  (`docs/agents/subagents.md`): an `agents/*.md` is markdown and metadata that runs
  nothing on load, its `tools:` can only *narrow* what a launched agent may
  reach, its `model:` names a model on the session's own provider, and every
  `write`/`edit`/`bash` that agent does still meets the permission gate. What
  a project file can do is put words in a subagent's system prompt — which a
  checked-in `AGENTS.md` or `SKILL.md` can do already, so gating this one
  would be a lock on an open door.

## Merge semantics

**Hooks: layers union.** The merged file is the user file with the project
file's matcher groups appended per event (`HooksFile::merged`) — purely
additive, the `docs/tools/hooks.md` pre-commitment: one more entry from the loader,
no precedence rules to invent — and each layer contributes only the hooks
trusted in it (`trust::trusted_only`). The existing `select()` rules already
answer every collision: duplicates dedup by command (first wins — the user's
copy), any block wins, `deny` > `ask` > `allow`, contexts concatenate. A
malformed project file is a red toast naming it and the project's hooks stay
off; the user's own hooks still load (each layer fails alone).

## Lifecycle and knobs

Project hooks files keep their subsystem's **read-once-at-bootstrap**
lifetime; a decision made in the review or `/hooks` applies to the
copy read then. Mid-session edits need a restart, exactly like the
user-level `hooks.json`, and are reviewed at the next launch.
Skills keep their per-turn rescan.

- `ZEROTH_PROJECT_CONFIG` — default on; `0`/`false`/`no`/`off` turns the
  project hooks layer off: no project hook discovery, and so nothing of
  the project's in the review. **This is the
  smoke suite's hermeticity switch** — `scripts/smoke.sh` sets it to `0` in
  the base env (the suite runs with cwd = a real checkout; a developer repo
  carrying `.zeroth/hooks.json` would otherwise be reviewed in every
  phase — the skills Phase 36 lesson), and the review phase turns it back on
  inside a `mktemp -d` workdir.
- `ZEROTH_HOOKS_FILE` keeps replacing the **user** file only, project
  discovery still runs; the project hooks layer's own kill switch is
  `ZEROTH_PROJECT_CONFIG=0`.
- `trust.json` hangs off the config home like `permissions.json` — no
  override var of its own; `ZEROTH_CONFIG_DIR` moves it.

## Module split

Pure (unit-tested, TDD):

- `src/trust.rs` — the project-file path builder (`project_hooks_file`), the root rules (`is_project_root`,
  `hooks_project_dir`), and the review's store (`docs/state/config-review.md`).
- `hooks::HooksFile::merged` — the union.

Boundary (smoke-verified):

- `tui/config.rs` — `project_config_enabled()`, `trust_json_path()`,
  `load_trust_file` (fail-closed + loud).
- `tui/trust.rs` — reading the project layer at bootstrap
  (`load_project_layer`) and everything the review does with it.

## Deliberately not doing

- **No project-level `permissions.json`/`settings.json`.** Those are
  preference files, not capabilities; nothing in them is worth the trust
  surface today.
- **No file watcher / per-turn rescan for hooks.** They keep their
  established read-once lifetime; skills keep their rescan. A project layer
  that appears mid-session is a restart away, like every other hooks
  edit.
