# `/cd` — moving the session's working directory

`/cd <path>` moves the directory the **tools** run in, mid-conversation: the
`bash` and file tools, the `!` shell, the footer's path and every `file://` link
the conversation resolves. The conversation itself stays — history, model,
context and prompt cache are untouched.

```
❯ /cd ../other-repo
● Working directory: ~/other-repo
```

## What moves and what stays

The split is the whole design. A session has two kinds of directory fact, and
conflating them is what makes a "just chdir" implementation untrustworthy.

| follows `/cd` | stays at the launch directory |
| --- | --- |
| `bash`, `bash_session`, `run_in_background` cwd | the permissions project key (`permissions.json`) |
| `read` / `write` / `edit` relative paths | the checkpoint store and its snapshots |
| the `!` shell | the rollout file's meta and the `/resume` cwd filter |
| the footer's path, `PathDisplay` (relative/tilde display, `file://` links) | the skills and agents discovered at startup |
| subagents' tools (they share the handle) | this directory's `/model` entry (`config.json`) |

The rule is: **where work happens moves; where the conversation is filed does
not**. Moving permissions with the session would silently carry an allowlist
granted in one repository into another; moving checkpoints would strand
snapshots under a tree nobody is working in; re-keying the rollout would make
`--continue` in the original directory lose the conversation. Claude Code's
`/cd` relocates the session's storage too; we deliberately do not, because our
rollouts are per conversation rather than per directory and a moved file would
break `--continue` where the user actually is.

## How it is wired

One handle, `zeroth::workdir::WorkDir`, is an `Arc<RwLock<PathBuf>>`:

- `tui::bootstrap` creates it from the launch cwd and hands clones to the
  `BackgroundRegistry` (`with_work_dir`), the `ModelSession` (which attaches it
  to every backend it builds), and keeps one on the `Conversation`.
- `RealToolExecutor` and `LlmBackend` carry it from the backend
  (`with_work_dir`) into each tool call. `bash` spawns through
  `subprocess::spawn_detached_in_dir`, the tty path through
  `pty::spawn::spawn_dir`, and file tools resolve a relative path with
  `resolve_tool_path`, all reading the handle **at the call**, never at build
  time — so a move reaches the very next tool call on an already-built backend,
  with no rebuild.
- The `!` runner reads the registry's `work_dir()` so the two shells can never
  disagree.
- The pure core never reads the process cwd: the loop formats the new
  directory with `ui::display_cwd` and calls `App::set_working_dir` (the
  `set_session_info` pattern), plus `App::set_path_display` with the new
  directory.

The resolution is lexical until the final check: `~`/`~/…` expand against
`$HOME`, anything else relative joins the current directory, then the loop
`canonicalize`s the result (so `..`, symlinks and a trailing slash all land on
one real directory) and requires `is_dir()`.

## The refusals

- **A bare `/cd`** — the palette row is the door, not a directory browser —
  answers with `Usage: /cd <path> — moves this session's working directory`.
- **A path that is not an existing directory** is a red toast and changes
  nothing.
- **Mid-turn** the move is refused (`/cd is disabled while a task is in
  progress`): a running turn's tools are resolved on the backend thread on its
  own schedule, and half a turn in one directory and half in another is never
  what the user meant. The check is `App::turn_active`, exactly like `/resume`
  and `/compact`.

## Cache

Nothing is invalidated. The prompt cache key is per **process**
(`tui::config::session_cache_key`, a pid + startup-time affix), not per
directory, so moving costs no cache hit; and the system prompt's environment
block — which names the cwd — was composed at startup and is not rewritten,
which is why the conversation's first request stays cacheable. The new
directory's `AGENTS.md` is **not** silently adopted either: project
instructions stay the launch project's, for the same reason its permissions do.

## Tests

- `zeroth::workdir` — `WorkDir` resolution (`~`, relative, absolute, a clone
  seeing a move) and the no-home case.
- `llm::exec::work_dir_tests` — `write`/`read` land under the handle, a move
  between calls reaches the next call, `edit` follows it, and `bash`'s `pwd`
  agrees.
- `app::tests::commands` — `/cd` is registered, a bare `/cd` answers the usage
  notice, and `/cd <path>` hands the loop the argument verbatim (spaces
  included).
