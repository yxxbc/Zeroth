#!/usr/bin/env bash
# Phase 156 — --fullscreen runs the screen-owning TUI on the alternate screen

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# The screen-owning mode (docs/ui/full-screen.md): the conversation is painted
# into an alternate screen instead of committed to scrollback. A dummy turn
# must render there in full, the raw stream must show the alternate-screen
# enter, and quitting must leave the screen (the shell gets its own back).
S_FULL="${S}_full"
RAW156="$(mktemp)"
tmux new-session -d -s "$S_FULL" -x 100 -y 32 \
	"$APP_ABS --fullscreen"
# Record from before the first frame: the alternate-screen enter happens during
# startup, so a pipe attached after the prompt appears would miss it.
tmux pipe-pane -t "$S_FULL" -o "cat >> $RAW156"
if ! wait_for 10 "$S_FULL" -E '^❯'; then
	fail "the full-screen app never came up (no '❯' prompt within 10s)"
fi
alt_on="$(tmux display-message -p -t "$S_FULL" '#{alternate_on}')"
expect_eq "$alt_on" "1" "the pane is not showing the alternate screen"
submit "$S_FULL" "hello"
full_turn=""
for _ in $(seq 1 200); do # ~25s: the dummy's pre-stream pause + thinking + reply
	cap="$(tmux capture-pane -t "$S_FULL" -p)"
	if printf '%s' "$cap" | grep -qF "Two commands away"; then
		full_turn="$cap"
		break
	fi
	sleep 0.15
done
echo "==== Phase 156: captured pane (--fullscreen — a dummy turn painted in the alternate screen) ===="
printf '%s\n' "$full_turn"

# The turn paints in the screen-owning frame: the reply, the tool cells, and
# the composer/footer are all on one owned screen.
expect_has "$full_turn" -F "Two commands away" "the full-screen frame did not show the dummy reply"
expect_has "$full_turn" -F "Bash(python3 about.py)" "the full-screen frame did not show the tool cell"
expect_has "$full_turn" -F "dummy_model_name ·" "the full-screen frame did not show the footer"
expect_has "$full_turn" -F "❯" "the full-screen frame did not show the composer"

# Ctrl+Home scrolls the conversation window to the top (the banner and the
# user's message are above the tail); Ctrl+End returns to follow-tail.
tmux send-keys -t "$S_FULL" C-Home
sleep 0.5
scrolled="$(tmux capture-pane -p -t "$S_FULL")"
echo "==== Phase 156: captured pane (Ctrl+Home — the conversation window at its top) ===="
printf '%s\n' "$scrolled"
expect_has "$scrolled" -F "Connect a model" "Ctrl+Home did not scroll to the startup banner"
expect_has "$scrolled" -F "❯ hello" "Ctrl+Home did not scroll to the user's message"
tmux send-keys -t "$S_FULL" C-End
sleep 0.5
back="$(tmux capture-pane -p -t "$S_FULL")"
expect_has "$back" -F "Two commands away" "Ctrl+End did not return to the conversation's tail"

# Quit, then read the recording: entry and exit must both be real
# alternate-screen switches, so the shell is never stranded on the alt screen.
tmux send-keys -t "$S_FULL" C-c
for _ in $(seq 1 50); do
	if ! tmux has-session -t "$S_FULL" 2>/dev/null; then
		break
	fi
	sleep 0.1
done
if tmux has-session -t "$S_FULL" 2>/dev/null; then
	tmux kill-session -t "$S_FULL" 2>/dev/null
	fail "the full-screen app did not quit on Ctrl+C"
fi
sleep 0.2
raw="$(cat "$RAW156" 2>/dev/null)"
rm -f "$RAW156"
expect_has "$raw" -F $'\033[?1049h' "the raw stream never entered the alternate screen"
expect_has "$raw" -F $'\033[?1049l' "the raw stream never left the alternate screen"
# Mouse capture is the screen-owning mode's: on at entry, off at exit.
expect_has "$raw" -F $'\033[?1000h' "the raw stream never enabled mouse capture"
expect_has "$raw" -F $'\033[?1000l' "the raw stream never disabled mouse capture"

smoke_finish
