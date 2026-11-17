#!/usr/bin/env bash
# Phase 91 — the SESSION TEMP TREE

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# the SESSION TEMP TREE (docs/tools/scratchpad.md). One per-user,
# per-session root holds two named leaves: `scratchpad/` — created before the
# first frame, so the system prompt can name a directory that exists — and
# `tasks/`, where a background shell tees its interim output. TMPDIR points the
# whole tree at a throwaway dir so this phase can find it unambiguously (and so
# a concurrent session of the developer's own is never mistaken for it).
S91="${S}_scratchpad"
SP_TMP="$(mktemp -d "$SMOKE_TMP/pad.XXXXXX")"
SPAPP="env $CFG_ENV_NOHIST TMPDIR=$SP_TMP ZEROTH_STARTUP_DELAY_MS=$SMOKE_STARTUP_MS $BIN_ABS"
tmux new-session -d -s "$S91" -x 80 -y 24 "$SPAPP"
wait_for 10 "$S91" -E '^❯' || fail "the app did not start"
# A backgrounded `!` command tees into the tasks leaf beside it.
submit "$S91" "!sleep 30"
sleep 0.6
tmux send-keys -t "$S91" C-b # move the running command to the background
sp_out=""
for _ in $(seq 1 60); do
	sp_out="$(find "$SP_TMP" -maxdepth 4 -path '*/tasks/*.output' 2>/dev/null | head -1)"
	if [ -n "$sp_out" ]; then break; fi
	sleep 0.15
done
# The output file is created before the cell redraws as backgrounded, so one
# capture taken the moment the file appears can still hold the old frame.
sp_pane="$(wait_pane 3 "$S91" -F "Running in the background")"
tmux kill-session -t "$S91" 2>/dev/null
sp_dir="$(find "$SP_TMP" -maxdepth 3 -type d -name scratchpad 2>/dev/null | head -1)"
echo "==== Phase 91: scratchpad='$sp_dir', task output='${sp_out#"$SP_TMP"}' ===="
[ -z "$sp_dir" ] || fail "an agent scratchpad directory was created"
if [ -z "$sp_out" ]; then
	fail "a backgrounded command left no interim output file"
elif ! printf '%s' "$sp_out" | grep -qE "^$SP_TMP/zeroth-[0-9]+/[^/]+/tasks/[^/]+\.output$"; then
    fail "background output is not scoped to the session: $sp_out"
fi
expect_has "$sp_pane" -F "Running in the background" "Ctrl+B did not move the command to the background"
rm -rf "$SP_TMP" 2>/dev/null
