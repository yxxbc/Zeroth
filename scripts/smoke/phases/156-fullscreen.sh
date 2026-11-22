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

# The prompt stays pinned while its reply is scrolled: shrink the pane so the
# conversation no longer fits, return to the tail, and the first conversation
# row must be the user's message (docs/ui/full-screen.md).
tmux resize-window -t "$S_FULL" -x 90 -y 20
sleep 0.6
tmux send-keys -t "$S_FULL" C-End
sleep 0.5
pinned="$(tmux capture-pane -p -t "$S_FULL")"
echo "==== Phase 156: captured pane (the prompt pinned above its scrolled reply) ===="
printf '%s\n' "$pinned" | sed -n '1,6p'
expect_eq "$(printf '%s\n' "$pinned" | head -1)" "❯ hello" \
	"the user prompt is not pinned at the top of its scrolled reply"

# The pinned row is a **summary**, and a click on it returns to the prompt
# (docs/ui/conversation-tool-details-and-pin.md). A question wider than the
# frame is the shape that tells the two apart: the strip folds it and marks the
# cut with `…`, while the message's own first row is the plain wrap. The table
# demo is the vehicle because its reply is tall enough to push the question off
# the top.
S_PIN="${S}_pin"
LONG156="why does the parser reject this particular input line even though every other line in the same table parses without complaint at all"
tmux new-session -d -s "$S_PIN" -x 90 -y 20 "$APP_ABS --fullscreen"
wait_for 10 "$S_PIN" -E '^❯'
submit "$S_PIN" "$LONG156"
if ! wait_for 30 "$S_PIN" -F "Two commands away"; then
	fail "the pin fixture's turn never finished"
fi
sleep 1 # the receipt row lands after the reply
tmux send-keys -t "$S_PIN" C-End
sleep 0.6
summary="$(tmux capture-pane -p -t "$S_PIN")"
echo "==== Phase 156: captured pane (the long prompt summarised on the pinned row) ===="
printf '%s\n' "$summary" | sed -n '1,4p'
case "$(printf '%s\n' "$summary" | head -1)" in
*…) ;; # the summary was cut: what the strip shows
*)
	fail "the pinned row is not a summary of the whole prompt (no … on a question wider than the frame)"
	;;
esac
# A left click on the strip's own row (screen row 1, one-based SGR) returns to
# the prompt: its first row becomes the window's top, so the `…` is gone.
tmux send-keys -t "$S_PIN" -l $'\033[<0;5;1M\033[<0;5;1m'
sleep 0.6
returned="$(tmux capture-pane -p -t "$S_PIN")"
echo "==== Phase 156: captured pane (after clicking the pinned row — back at the prompt) ===="
printf '%s\n' "$returned" | sed -n '1,4p'
first_returned="$(printf '%s\n' "$returned" | head -1)"
case "$first_returned" in
*…) fail "clicking the pinned row did not return to the prompt (still the summary)" ;;
esac
case "$first_returned" in
"❯ "*why*) ;; # the prompt's own first row, back at the top
*) fail "clicking the pinned row left the conversation elsewhere: $first_returned" ;;
esac
# A drag over the conversation copies what it highlighted
# (docs/ui/full-screen.md *Selection semantics*). The toast is the portable
# proof: the branch that raises it is the one that hands the text to the
# clipboard, whichever backend takes it. The *exact* cells (the highlight and
# the copy agreeing, no split grapheme) are pinned deterministically by
# `src/ui/tests/selection.rs`; reading the clipboard back here is not — a GUI
# login takes the native pasteboard, so tmux's OSC 52 buffer stays empty by
# design.
#
# The click above left the window's top on the prompt, so its first row is the
# text to drag across (one-based SGR rows and columns).
first_row="$(tmux capture-pane -p -t "$S_PIN" | head -1)"
case "$first_row" in
"❯ why does the parser"*) ;;
*) fail "the drag fixture's prompt is not the window's first row: $first_row" ;;
esac
tmux send-keys -t "$S_PIN" -l $'\033[<0;3;1M\033[<32;3;1M\033[<32;28;1M\033[<0;28;1m'
if ! wait_for 3 "$S_PIN" -F "Copied selection to clipboard"; then
	fail "a drag over the conversation did not copy the selection"
fi
echo "==== Phase 156: captured pane (a drag selected and copied) ===="
tmux capture-pane -p -t "$S_PIN" | sed -n '1p'

# Ctrl+O still opens the whole-transcript pager in this mode
# (docs/ui/full-screen.md *Ctrl+O's role*): it is also the Esc-Esc backtrack
# surface, so retiring it would take backtracking with it.
tmux send-keys -t "$S_FULL" C-o
if ! wait_for 5 "$S_FULL" -F "T R A N S C R I P T"; then
	fail "Ctrl+O did not open the transcript pager in full-screen mode"
fi
tmux send-keys -t "$S_FULL" q
sleep 0.4
if ! wait_for 5 "$S_FULL" -E '^❯'; then
	fail "leaving the transcript pager did not return to the conversation"
fi

# A click answers the permission prompt, in two steps (docs/state/permissions.md):
# the first highlights the option, the second takes it. The prompt is the one
# modal a click can answer, and the rows come from the frame that was painted —
# recomputing the page at click time mapped a click on "No" past the option
# list entirely (the bug this pins).
S_PERM="${S}_perm"
tmux new-session -d -s "$S_PERM" -x 100 -y 40 \
	"env $CFG_ENV_NOHIST ZEROTH_STARTUP_DELAY_MS=2500 ZEROTH_PERMISSION_MODE=manual $BIN --fullscreen"
wait_for 10 "$S_PERM" -E '^❯'
submit "$S_PERM" "permission demo please"
if ! wait_for 20 "$S_PERM" -F "Do you want to create hello.py?"; then
	fail "the permission prompt never appeared in full-screen mode"
fi
no_row="$(tmux capture-pane -p -t "$S_PERM" | grep -n '3\. No' | head -1 | cut -d: -f1)"
if [ -z "$no_row" ]; then
	fail "the permission prompt's No option is not on screen"
fi
# One-based SGR: press+release on the No row.
tmux send-keys -t "$S_PERM" -l "$(printf '\033[<0;5;%sM\033[<0;5;%sm' "$no_row" "$no_row")"
if ! wait_for 5 "$S_PERM" -F "❯ 3. No"; then
	fail "a click did not highlight the option it landed on"
fi
tmux send-keys -t "$S_PERM" -l "$(printf '\033[<0;5;%sM\033[<0;5;%sm' "$no_row" "$no_row")"
if ! wait_for 10 "$S_PERM" -F "User rejected write to hello.py"; then
	fail "a second click on the highlighted option did not answer the prompt"
fi
echo "==== Phase 156: captured pane (a click answered the permission prompt) ===="
tmux capture-pane -p -t "$S_PERM" | grep -F "User rejected" | head -1
tmux kill-session -t "$S_PERM"

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
# Leaving the mode restores the shell's screen — the conversation does not stay
# behind in scrollback (docs/ui/full-screen.md *Scrollback on exit*) — and the
# conversation is still recoverable: the exit hint names the rollout to resume.
expect_has "$raw" -F "Resume this session with:" \
	"the exit hint did not print after leaving the full-screen mode"
expect_has "$raw" -F -- "--resume" \
	"the exit hint did not carry a resume id"
if printf '%s' "$raw" | grep -qF "Two commands away" &&
	! printf '%s' "$raw" | grep -qF $'\033[?1049l'; then
	fail "the conversation was left on the shell's screen without leaving the alternate screen"
fi

smoke_finish
