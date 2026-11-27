#!/usr/bin/env bash
# Phase 165 — /cd moves the tools' working directory, not the conversation

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# `/cd` (docs/state/change-directory.md). The session launches in one
# directory, moves to a second with `/cd <path>`, and from then on:
#
#   * the model's `bash` runs there (`pwd` in a `!` command is the check the
#     phase can make without a model — the `!` shell reads the same handle);
#   * the footer names the new directory, `~`-relative;
#   * the conversation, its history and the rollout are untouched — the
#     transcript above the move is still there, and the session file still
#     records the LAUNCH directory (so `--continue` in the original directory
#     still finds it).
#
# A path that is not a directory is refused and moves nothing: that is the
# second half of the phase, so a bad `/cd` can never look like a good one.
S165="${S}_cd"
# Short paths on purpose: the footer `…`-cuts at the pane's width, and an
# assertion on a truncated directory would test the clamp rather than the
# move. `/tmp` is short everywhere the suite runs.
first="/tmp/zd165a"
second="/tmp/zd165b"
rm -rf "$first" "$second"
mkdir -p "$first" "$second"
trap 'rm -rf "$first" "$second"' EXIT

# The launch directory is the app's cwd; `-C` is the CLI's chdir, run before
# anything reads the working directory (docs/cli/cli.md).
tmux new-session -d -s "$S165" -x 90 -y 30 -c "$first" \
	"env $CFG_ENV_NOHIST ZEROTH_STARTUP_DELAY_MS=$SMOKE_STARTUP_MS $BIN"
sleep 0.5

# The banner's cwd row and the footer both name the launch directory.
launch_pane="$(wait_pane 10 "$S165" -- -F "$first")"
echo "==== Phase 165: the launch directory is on screen ===="
printf '%s\n' "$launch_pane" | tail -8
expect_has "$launch_pane" -F "$first" "the banner/footer did not name the launch directory"

# A `!pwd` before the move: the shell reads the process cwd.
submit "$S165" "!pwd"
before_move="$(wait_pane 8 "$S165" -S -60 -- -F "$first")"
echo "==== Phase 165: ! pwd before the move ===="
printf '%s\n' "$before_move" | tail -10
expect_has "$before_move" -E "⎿  $first" "the ! shell did not run in the launch directory"

# Move. The toast names the new directory, the footer follows, and the
# transcript above the move stays where it was.
submit "$S165" "/cd $second"
moved="$(wait_pane 8 "$S165" -S -80 -- -F "$second")"
echo "==== Phase 165: after /cd ===="
printf '%s\n' "$moved" | tail -14
expect_has "$moved" -F "Working directory: $second" "the /cd toast did not name the new directory"
expect_has "$moved" -F "$second" "the footer did not follow the move"
expect_has "$moved" -F "! pwd" "the conversation was not kept across /cd"

# A `!pwd` after the move runs in the new directory — the same handle the
# model's tools read.
submit "$S165" "!pwd"
after_move="$(wait_pane 8 "$S165" -S -80 -- -F "$second")"
echo "==== Phase 165: ! pwd after the move ===="
printf '%s\n' "$after_move" | tail -12
# The output line is the new directory, not the old one: the last `⎿` row of
# the pane is the one to read, since both `pwd`s printed a directory.
last_gutter="$(printf '%s\n' "$after_move" | grep -E '^  ⎿  /' | tail -1)"
if [ "$last_gutter" != "  ⎿  $second" ]; then
	fail "the ! shell did not follow /cd (its last ⎿ row reads '$last_gutter', expected '  ⎿  $second')"
fi

# A path that is not a directory is refused, and nothing moves.
submit "$S165" "/cd $first/does-not-exist"
refused="$(wait_pane 8 "$S165" -S -80 -- -F "Cannot move to")"
echo "==== Phase 165: a refused /cd ===="
printf '%s\n' "$refused" | tail -8
expect_has "$refused" -F "Cannot move to" "a missing /cd path was not refused"
expect_has "$refused" -F "$second" "a refused /cd moved the session anyway"

# A bare `/cd` explains the shape rather than moving.
submit "$S165" "/cd"
usage="$(wait_pane 8 "$S165" -S -80 -- -F "Usage: /cd")"
echo "==== Phase 165: a bare /cd ===="
printf '%s\n' "$usage" | tail -8
expect_has "$usage" -F "Usage: /cd <path>" "a bare /cd did not print the usage line"

tmux kill-session -t "$S165" 2>/dev/null

# The rollout still records the LAUNCH directory: the conversation is filed
# where it started, so `--continue` there finds it. (The meta line is written
# at the first item, before any `/cd` this phase issued.)
rollout_meta="$(cat "$SMOKE_SESSIONS"/*/*/*/rollout-*.jsonl 2>/dev/null | grep -F '"type":"session_meta"' | head -1)"
echo "==== Phase 165: the rollout meta ===="
printf '%s\n' "$rollout_meta"
expect_has "$rollout_meta" -F "\"cwd\":\"$first\"" "the rollout recorded the moved directory instead of the launch one"
if printf '%s\n' "$rollout_meta" | grep -qF "\"cwd\":\"$second\""; then
	fail "the rollout meta recorded the /cd directory — the conversation must stay filed where it started"
fi
