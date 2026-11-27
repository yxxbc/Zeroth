# Startup, idle cost and the first turn — measured

Date: 2026-11-27

A comparison table put zeroth behind `alter-zero` and `litemiyu` on six
numbers at once: startup to first byte, startup to typable, idle
`phys_footprint`, idle RSS and thread count, and the CPU and memory peak of
one tool-calling turn. This page is what each number actually is, what was
fixed, and what is left. Every number here is measured on the development
machine (8-core Apple silicon, macOS 27.2, release build) with the harnesses
listed below — not estimated.

## What was wrong

Two things, and neither was the renderer.

### 1. A child process in the first-paint path — fixed

`checkpoint::git_available()` answered "can checkpoints run here?" by running
`git --version` and waiting for it. Measured: **5.0–6.1 ms** per spawn (five
runs), inside bootstrap, before the first frame. On macOS `/usr/bin/git` is
the Xcode command-line-tools shim, which can additionally block on an
*install dialog* the first time it runs — a greenhouse for a "the app hung on
launch" report.

The answer only ever needed a `PATH` lookup: is there an executable file
named `git` on `PATH`? That is `subprocess::find_on_path` + `is_executable_file`,
already in the crate for the shell probe. A `git` that is present but broken
still fails per-call and loudly — `snapshot()` returns `None` and the turn
carries on, which is the failure mode the feature already had.

### 2. A permanent HTTP runtime for a once-a-day request — fixed

The update check (`docs/ops/update.md`) went through the **cached** provider
HTTP client. Right for provider traffic — a warm pool saves a TLS handshake —
wrong for one request a day: the client keeps reqwest's internal runtime
thread and its timer alive for the whole session. Measured, it was

- the session's **fourth thread** (main, two crossterm readers, this one), and
- the **only** source of idle CPU wakeups: 6–7/min on the process's own
  `ri_pkg_idle_wkups`/`ri_interrupt_wkups` counters, against 0 for a plain
  sleeping process.

`llm::ephemeral_client` builds the same policy without caching; the update
worker drops it after the request and the thread goes with it.

## The numbers now

| | before | after |
| --- | --- | --- |
| threads, idle | 4 | **3** |
| idle wakeups (own counters) | 6–7/min | **0/min** |
| `phys_footprint`, idle | 4.6 MB | **4.55 MB** |
| RSS, idle | 16.4 MB | **16.4 MB** |
| startup, process → first byte | 5.7 ms | **5.7 ms** |
| startup, process → typable prompt | 13.4 ms | **12.5 ms** |
| one dummy turn: peak `phys_footprint` | 5.6 MB | **5.6 MB** |
| one dummy turn: CPU | 2–3 ms | **2–3 ms** |

The two fixed items are worth ~2.4 ms of startup (the git spawn, measured at
5–6 ms standalone, of which the part inside bootstrap was 2.4 ms across 11
runs) and the whole of the idle thread and wakeup column. The startup rows
are the same in-process measurement before and after the git fix; the pty
harness adds its own shell spawn on top.

## What the other numbers are, and why they are not going to move much

**`phys_footprint` 4.55 MB / RSS 16.4 MB.** The RSS is the shared read-only
pages of the frameworks the binary links (AppKit, CoreText, Security,
CFNetwork — all pulled in by `arboard` and `reqwest`'s system TLS
configuration); `phys_footprint`, which counts what the process actually
dirtied, is 4.55 MB. A TUI in Rust with those two dependencies starts around
there; there is no per-session structure to delete.

**Three threads.** Main (the tokio current-thread runtime), crossterm's
`EventStream` reader, and the crossterm internal event reader it owns. The
sleeping session has nothing else; a probe of the running binary's stacks
(`sample`) shows all three parked in `kevent` or a semaphore wait. Getting to
two would mean replacing crossterm's event source with our own — a rewrite of
invariant 1's one reader for one thread, which this page records as a
rejected trade.

**A turn's 2–3 ms of CPU with the dummy backend** is the UI: wrapping the
reply, committing rows, one frame. The model round trip is the wall clock
(2 s in the dummy's scripted pace, network time otherwise), not CPU.

**Startup to "typable" (12.5 ms in-process).** Of it, ~1.4 ms is argument
parsing and session resolution, ~6 ms is the terminal's answer to the cursor
query (a real round trip — the viewport cannot seat the box until it knows
where the cursor is, invariant 1), and the rest is bootstrap: the web rules
parse (~0.5 ms), the skill and agent walks (~1 ms warm), the backend build,
and the first frame (~0.1 ms). The comparison table's 35.3 ms includes
spawning the tmux session that hosts the app (measured: 80 ms of tmux
session creation alone on this machine), so the two numbers are not the same
measurement.

## Reproducing

The harnesses are throwaway scripts, kept here as the record rather than
committed (they are `proc_pid_rusage` and pty glue, not part of the product):

- **startup** — spawn the release binary in a pty, answer its `ESC[6n` with a
  cursor row, time the first output byte and the first `❯`, median of at least
  seven runs.
- **idle** — read `proc_pid_rusage(pid, RUSAGE_INFO_V4)` twice a minute apart:
  `ri_pkg_idle_wkups`, `ri_interrupt_wkups`, `ri_phys_footprint`,
  `ri_resident_size`, user + system time. Control: a plain `sleep 300`.
- **a turn** — drive one dummy turn in the pty with
  `ZEROTH_STARTUP_DELAY_MS=100 ZEROTH_CHUNK_DELAY_MS=1`, sampling
  `ri_phys_footprint` every 50 ms and diffing the CPU counters around it.

## What was deliberately not changed

- **The cached provider client.** It is the right shape for provider traffic
  and its thread only exists once a real backend is in use.
- **The cursor query.** It is invariant 1's one stdin read, and it is the
  round trip every terminal TUI pays to seat its viewport.
- **The crossterm event readers.** Two threads for one keyboard is a fair
  price for a safe, upstream-maintained reader.
