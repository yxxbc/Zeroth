#!/usr/bin/env bash
# Phase 162 — restored files precede queued shell turns

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

CK_DIR="$(mktemp -d "$SMOKE_TMP/ck.XXXXXX")"
CK_SESS="$(mktemp -d "$SMOKE_TMP/cksess.XXXXXX")"

# /resume resets the CODE to the saved session's checkpoint
# (docs/state/checkpoint.md). Launch 1 records a session whose `!` shell turn leaves
# the file at v1; the process quits. The file is then DIVERGED on disk (as if
# later work changed it). Launch 2's /resume of that session must restore the
# file to v1 — the transcript and the code agree again. Same cwd across
# launches, so the isolated store (keyed by cwd) still holds the v1 commit.
S47="${S}_ckresume"
WORK47="$(mktemp -d "$SMOKE_TMP/work47.XXXXXX")"
printf 'pristine\n' >"$WORK47/file.txt"
CKAPP2="env $CFG_ENV ZEROTH_CHECKPOINTS=1 ZEROTH_CHECKPOINTS_DIR=$CK_DIR ZEROTH_SESSIONS_DIR=$CK_SESS ZEROTH_STARTUP_DELAY_MS=$SMOKE_STARTUP_MS $BIN_ABS"
launch -c "$WORK47" "$S47" 80 24 "$CKAPP2"
submit "$S47" "$USER_MSG" # a user message so the session lists in the picker
wait_for 20.1 "$S47" -S -40 -- -E "$SUMMARY_RE"
submit "$S47" "!echo v1 > file.txt" # the session's final code state
for _ in $(seq 1 80); do # wait for the shell TURN to END so its {v1} checkpoint records
	if [ "$(cat "$WORK47/file.txt" 2>/dev/null)" = "v1" ] &&
		! tmux capture-pane -t "$S47" -p -S -20 | grep -qF "Running"; then
		break
	fi
	sleep 0.15
done
sleep 0.5 # let the shell turn's {v1} checkpoint record + flush to the session file
tmux send-keys -t "$S47" C-c # quit launch 1 (empty composer)
sleep 0.4
tmux kill-session -t "$S47" 2>/dev/null
printf 'divergent\n' >"$WORK47/file.txt" # later work diverges the code on disk

PROBE_REAL_GIT="$(command -v git)"
PROBE_MARKER="$SMOKE_TMP/restore-running"
mkdir -p "$SMOKE_TMP/probe-bin"
cat > "$SMOKE_TMP/probe-bin/git" <<'SH'
#!/bin/sh
case " $* " in
  *" reset "*)
    : > "$ZEROTH_PROBE_MARKER"
    sleep 6
    ;;
esac
"$ZEROTH_PROBE_REAL_GIT" "$@"
probe_status=$?
case " $* " in *" reset "*) rm -f "$ZEROTH_PROBE_MARKER" ;; esac
exit "$probe_status"
SH
chmod +x "$SMOKE_TMP/probe-bin/git"
CKAPP3="env PATH=$SMOKE_TMP/probe-bin:/opt/homebrew/bin:/usr/bin:/bin:/usr/sbin:/sbin ZEROTH_PROBE_MARKER=$PROBE_MARKER ZEROTH_PROBE_REAL_GIT=$PROBE_REAL_GIT $CFG_ENV ZEROTH_CHECKPOINTS=1 ZEROTH_CHECKPOINTS_DIR=$CK_DIR ZEROTH_SESSIONS_DIR=$CK_SESS ZEROTH_STARTUP_DELAY_MS=$SMOKE_STARTUP_MS $BIN_ABS"
launch -c "$WORK47" "$S47" 80 24 "$CKAPP3" || { fail "probe launch failed"; smoke_finish; }
submit "$S47" "/resume"
sleep 0.6
tmux send-keys -t "$S47" Enter
for _ in $(seq 1 100); do
  [ -f "$PROBE_MARKER" ] && break
  sleep 0.1
done
[ -f "$PROBE_MARKER" ] || { fail "restore never began"; smoke_finish; }
submit "$S47" "!echo NEW_TASK_RESULT > file.txt"
sleep 0.5
before="$(cat "$WORK47/file.txt")"
echo "File while restore is running: $before"
for _ in $(seq 1 100); do
  [ "$(cat "$WORK47/file.txt")" = NEW_TASK_RESULT ] && [ ! -e "$PROBE_MARKER" ] && break
  sleep 0.1
done
after="$(cat "$WORK47/file.txt")"
echo "File after restore completes: $after"
expect_eq "$before" "divergent" "a queued shell ran before file restoration finished"
expect_eq "$after" "NEW_TASK_RESULT" "the queued shell did not run after restore, or its result was overwritten"
smoke_finish
