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
# The composer starts the row: the sidebar is the screen's right edge, so
# nothing indents the conversation (docs/ui/sidebar.md).
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
# The row's ink: strip the scrollbar's column and the trailing blanks (the
# sidebar is on the right, so it never touches this row).
pinned_row="$(printf '%s\n' "$pinned" | head -1 | sed 's/[│▐ ]*$//')"
expect_eq "$pinned_row" "❯ hello" \
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
# The row's ink, without the scrollbar's column or the trailing blanks.
strip_chrome() { printf '%s' "$1" | sed 's/[│▐ ]*$//'; }
summary_row="$(strip_chrome "$(printf '%s\n' "$summary" | head -1)")"
case "$summary_row" in
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
first_returned="$(strip_chrome "$(printf '%s\n' "$returned" | head -1)")"
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

# A picture in the conversation is **reserved but never placed** in this mode
# (docs/ui/full-screen.md *No terminal images in the frame*): the frame scrolls
# under a terminal placement, so the rows the inline view fills with a picture
# stay text here — the `read` cell's fact line names it — and scrolling can
# never leave a block behind. Half-blocks are pinned for the same reason Phase
# 107 pins them: they are ordinary coloured cells, so `capture-pane` sees them
# (a kitty placeholder is a glyph too, but one that means "picture" only to a
# terminal that will draw it).
S_IMG="${S}_img"
IMG156_DIR="$(mktemp -d "$SMOKE_TMP/img156.XXXXXX")"
IMG156_SESS="$(mktemp -d "$SMOKE_TMP/img156sess.XXXXXX")"
img156_day="$IMG156_SESS/2026/07/23"
mkdir -p "$img156_day"
base64 -d >"$IMG156_DIR/shot.png" <<'PNG64'
iVBORw0KGgoAAAANSUhEUgAAAGQAAAA8CAIAAAAfXYiZAAADYUlEQVR4Ae3AA6AkWZbG8f937o3I
zKdyS2Oubdu2bdu2bdu2bWmMnpZKr54yMyLu+Xa3anqmhztr1a/aLvqjQguy0IIstCALLchCK7Qg
Cy3IQguy0IIstEILstCCLLQgCy3IQguy0AotyEILstCCLLQgC63Qgiy0IAstyEILstCCLLRCC7LQ
giy0IAstyEIrtCALLchCC7LQgiy0IAut0IIstCALLchCC7KQBQIKBBQIKBBQIKBAgYACAQUCCgQU
qOr+JCpX/QsAqFSu+pcBUKk80P7+6x3f/lWuei4AVDoeaPvkb5SOq54bAJXKVf8yACqVF+Kuu976
5ht+gqsAqFReiBtu+elSuQoAKh1X/csAqFRedE984ns85lHfw/9DAFQqL7pHvdj3ReX/IwAqlav+
ZQBUOv7N/vzPP/QVX/7r+f8AgErl3+zlX/kbo/L/AgCVylX/MgAqlf8ov/3bn/h6r/1F/J8EQKXj
P8prv8GXlo7/mwCoVK76lwFQqfwn+bmf+7y3fotP5f8GACqV/yRv8TafUSr/RwBQ6bjqXwZApfJf
4wd/8Kvf/V0/kv+lAKhU/mu863t+dFT+twKgUrnqXwZApeO/xbd8y3d8yAe9D/9bAFCp/Lf4oA97
v6j8rwFApXLVvwyASuV/gq/4ih/5xI97e/7HAqDS8T/Bx33yO5WO/7kAqFSu+pcBUKn8D/TZn/2L
n/fZb8T/HABUKv8Dffbnv2mp/A8CQKXjqn8ZAJXK/3wf93G/91Vf8Wr8NwKgUvmf7yu+5jWi8t8J
gErlqn8ZAJWO/3U+8AP/+tu/9SX5rwRApfK/zrd+50tH5b8UAJXKVf8yACqV/+3e9V2f8iM/+BD+
UwFQ6fjf7gd/7OGl4z8XAJXKVf8yACqV/2Pe4i3u/cWfO81/LAAqlf9jfu6Xri2V/2AAVDqu+pcB
UKn83/bar330u789598JgErl/7bf/v2NqPx7AVCpXPUvA6DS8f/Ky72c/+ovxL8WAJXK/yt/8TeK
yr8aAJXKVf8yACqV/88e9Sg/5YniXwRApeP/syc+TaXjXwZApXLVvwyASuWqZ7nhBt97l3heAFQq
Vz3LXfepVJ4PACodV/3LAKhUrnpBtrd9uC8AACqVq16Q/aWiAgBApXLVvwyAfwRqc6dvjMnyQgAA
AABJRU5ErkJggg==
PNG64
img156_rollout="$img156_day/rollout-2026-07-23T10-00-00-15615616.jsonl"
{
	printf '{"timestamp":"2026-07-23T10:00:00.000Z","type":"session_meta","payload":{"id":"smoke-156-img","timestamp":"2026-07-23T10:00:00.000Z","cwd":"%s","model":"dummy_model_name","originator":"zeroth","version":"0.1.0"}}\n' "$IMG156_DIR"
	printf '{"timestamp":"2026-07-23T10:00:01.000Z","type":"message","payload":{"role":"user","text":"look at shot.png","timestamp":"10:00 AM"}}\n'
	printf '{"timestamp":"2026-07-23T10:00:02.000Z","type":"tool","payload":{"name":"Read","args":"%s/shot.png","ok":true,"output":"Read image (PNG, 100x60, 2 KB)","timestamp":"10:00 AM","shell":false,"truncated":false,"arguments":"{\\"path\\":\\"%s/shot.png\\"}"}}\n' "$IMG156_DIR" "$IMG156_DIR"
	printf '{"timestamp":"2026-07-23T10:00:03.000Z","type":"message","payload":{"role":"assistant","text":"A colour gradient with a white diagonal.","timestamp":"10:00 AM"}}\n'
} >"$img156_rollout"
tmux new-session -d -s "$S_IMG" -x 100 -y 30 -c "$IMG156_DIR" \
	"env $CFG_ENV_NOHIST ZEROTH_SESSIONS_DIR=$IMG156_SESS ZEROTH_IMAGE_PROTOCOL=halfblocks ZEROTH_IMAGE_CELL_SIZE=5x10 ZEROTH_STARTUP_DELAY_MS=$SMOKE_STARTUP_MS $BIN_ABS --fullscreen --resume $img156_rollout"
if ! wait_for 10 "$S_IMG" -E '^❯'; then
	fail "the image fixture never came up in full-screen mode"
fi
tmux send-keys -t "$S_IMG" C-Home
sleep 0.6
img156_frame="$(tmux capture-pane -p -t "$S_IMG")"
echo "==== Phase 156: captured pane (an image read in the frame — text, never a placement) ===="
printf '%s\n' "$img156_frame" | sed -n '1,8p'
expect_has "$img156_frame" -F "Read image (PNG, 100x60" \
	"the image read's cell is missing from the full-screen frame"
if printf '%s' "$img156_frame" | grep -qF "▄"; then
	fail "a terminal picture was placed in the scrolling full-screen frame"
fi
tmux send-keys -t "$S_IMG" C-End
sleep 0.4
tmux send-keys -t "$S_IMG" C-Home
sleep 0.5
img156_back="$(tmux capture-pane -p -t "$S_IMG")"
if printf '%s' "$img156_back" | grep -qF "▄"; then
	fail "scrolling left a picture block behind in the full-screen frame"
fi
tmux kill-session -t "$S_IMG"
rm -rf "$IMG156_DIR" "$IMG156_SESS"

# The sidebar's three bands (docs/ui/sidebar.md): full column when wide, a
# narrower one in the middle, and nothing at all when the conversation needs
# the room. The seam is the mark that says a column is there.
tmux resize-window -t "$S_FULL" -x 140 -y 30
sleep 0.6
wide="$(tmux capture-pane -p -t "$S_FULL")"
echo "==== Phase 156: captured pane (140 columns — the sidebar) ===="
printf '%s\n' "$wide" | sed -n '1,3p'
expect_has "$wide" -F "Zeroth v" "the sidebar's brand block is missing at 140 columns"
expect_has "$wide" -F "│" "the sidebar's seam is missing at 140 columns"
# It is the screen's RIGHT edge: the brand sits past the seam, and the
# conversation's rows start at column 0.
expect_has "$(printf '%s\n' "$wide" | head -1)" -F "│Zeroth v" \
	"the sidebar is not on the right edge"
tmux resize-window -t "$S_FULL" -x 100 -y 30
sleep 0.6
middle="$(tmux capture-pane -p -t "$S_FULL")"
expect_has "$middle" -F "Zeroth v" "the narrowed sidebar is missing at 100 columns"
tmux resize-window -t "$S_FULL" -x 80 -y 30
sleep 0.6
narrow="$(tmux capture-pane -p -t "$S_FULL")"
if printf '%s\n' "$narrow" | grep -qF "Zeroth v"; then
	fail "the sidebar did not collapse at 80 columns"
fi
# The scrollbar marks the conversation it cannot all show: the thumb glyph is
# the bar's (`docs/ui/full-screen.md` *The frame's own chrome*).
expect_has "$narrow" -F "▐" "the conversation's scrollbar is missing while scrolled"

# The bar is a control, not just chrome: a drag on the thumb moves the window
# with the pointer, a click on the track jumps to the row it landed on, and
# the wheel still moves three lines (`docs/ui/full-screen.md`). One-based SGR
# mouse sequences: press, drag, release.
tmux send-keys -t "$S_FULL" C-End
sleep 0.5
at_tail="$(tmux capture-pane -p -t "$S_FULL")"
# Press the **bottom** row of the thumb and drag it to the pane's first row:
# the mapping's travel ratio carries the window all the way to the top, where
# the banner and the user's message are.
thumb_row="$(printf '%s\n' "$at_tail" | grep -nF '▐' | tail -1 | cut -d: -f1)"
if [ -z "$thumb_row" ]; then
	fail "the scrollbar's thumb is not on screen at the tail"
fi
tmux send-keys -t "$S_FULL" -l "$(printf '\033[<0;80;%sM\033[<32;80;1M\033[<0;80;1m' "$thumb_row")"
if ! wait_for 3 "$S_FULL" -F "Connect a model"; then
	fail "dragging the scrollbar thumb to the top did not scroll to the banner"
fi
dragged="$(tmux capture-pane -p -t "$S_FULL")"
echo "==== Phase 156: captured pane (dragging the scrollbar thumb scrolled to the top) ===="
printf '%s\n' "$dragged" | sed -n '1,4p'
expect_has "$dragged" -F "❯ hello" "dragging the thumb to the top did not show the user's message"

# The wheel still moves three lines: from the top, three wheel-down events
# take the banner's first row off screen.
tmux send-keys -t "$S_FULL" -l $'\033[<65;40;10M\033[<65;40;10M\033[<65;40;10M'
sleep 0.5
wheeled="$(tmux capture-pane -p -t "$S_FULL")"
echo "==== Phase 156: captured pane (three wheel-down lines) ===="
printf '%s\n' "$wheeled" | sed -n '1,3p'
if printf '%s\n' "$wheeled" | grep -qF "Zeroth (v"; then
	fail "the wheel did not move the conversation (the banner is still at the top)"
fi

# A click on the track's last row is the tail: the window jumps there, so the
# closing rows are back. The composer's top rule is the first full-width rule
# below the conversation.
rule_row="$(printf '%s\n' "$wheeled" | grep -n '^──' | head -1 | cut -d: -f1)"
track_bottom=$((${rule_row:-0} - 1))
if [ "$track_bottom" -le 0 ]; then
	fail "the composer's rule is not below the conversation"
fi
tmux send-keys -t "$S_FULL" -l "$(printf '\033[<0;80;%sM\033[<0;80;%sm' "$track_bottom" "$track_bottom")"
if ! wait_for 3 "$S_FULL" -F "Review the current worktree with /diff"; then
	fail "a click on the scrollbar's track did not jump to the tail"
fi

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
