# Batch mode — several headless sessions in one process

`zeroth -p --jobs FILE` runs a JSONL file of prompts as independent headless
sessions **in one process**, on a bounded worker pool. It is the first client of
the multi-threaded core (`docs/architecture/multithread-core.md`): each job is
the same [`headless::run`](../cli/headless.md) a `-p` invocation runs, on its own
OS thread with its own current-thread runtime.

```bash
cat > jobs.jsonl <<'EOF'
{"id": "api", "prompt": "List the public endpoints in src/api.rs"}
{"id": "tests", "prompt": "Run the test suite and summarise the failures"}
EOF

zeroth -p --jobs jobs.jsonl --concurrency 2 --out-dir job-logs
```

## The jobs file

One JSON object per line; blank lines are skipped.

| field | | |
| --- | --- | --- |
| `prompt` | required | The run's first user message, exactly as a positional `[PROMPT]` would be. |
| `id` | optional | Names the job in its result, its log file and duplicate detection. Defaults to `job-N`, its 1-based line number. |

A missing or blank `prompt`, malformed JSON, a duplicate `id`, or a file with
no jobs is a usage error: the batch says why on stderr and exits `2` before any
session starts. `--jobs` cannot be combined with `[PROMPT]`, `--continue` or
`--resume`: every job starts a fresh conversation.

## Output

One JSON object per finished job on stdout, in completion order:

```json
{"id":"api","exit_code":0,"session_id":"18e75fc60a157a18-d324-7","result":"…","num_turns":2,"usage":{"input_tokens":1840,"output_tokens":212,"cached_input_tokens":0,"cache_write_input_tokens":0},"duration_ms":331,"log":"job-logs/001-api.log"}
```

- `exit_code` is the job's own status, with the same meanings as a single
  headless run (`docs/cli/headless.md` *Exit status*).
- `result` is the run's final answer; `session_id` is what `--resume` would
  take (present only when the run recorded something).
- `log` is present only with `--out-dir`; the file holds the job's complete
  reporter output (the format `--output-format` selects), captured instead of
  written to the process's streams.
- Jobs never interleave on stdout: each session's reporter writes to a
  thread-local sink, and the batch emits only whole result lines.

## Concurrency and admission

`--concurrency N` sets the number of worker threads (default 4). Workers pull
from one queue; a job that returns `130`/`143` clears the queue, so a Ctrl+C
stops the whole batch after the sessions already in flight have shut down.

The budget is explicit rather than automatic. Each session's current-thread
runtime owns a kqueue/epoll descriptor, and its bootstrap opens config, skill,
agent and rollout files; the measurements in the design note put 15 sessions
comfortably inside the default 256-fd soft limit, while 64 concurrent
bootstraps hit `EMFILE`. Four is a safe default for ordinary machines; raise it
for network-bound work, where sessions spend their time waiting.

## What is shared, and what is not

Shared: the config home (read-mostly), the working directory, the process's
stdout/stderr, and the kernel's scheduler. The first-run seeds under
`skills/` and `agents/` are written atomically, so concurrent bootstraps never
let one session parse another's half-written file, and session ids carry a
process-local sequence so two sessions started in the same nanosecond cannot
share a rollout path.

Not shared: each session owns its `App`, conversation, rollout, permission
gate, MCP manager, subagent registry and background shells. A job cannot see
or steer another job's turn. There is no TUI and no shared model client; each
session builds its provider client from the same config.

## Exit status

The batch exits `0` when every job exited `0`, the first failing job's status
otherwise, and `128 + signal` when a signal ended the batch. A per-job failure
does not stop the remaining jobs — the result stream says which one failed.
