#!/usr/bin/env bash
# Phase 158 — an alternate-screen overlay opens and closes inside synchronized frames

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# The Ctrl+O transcript (and every `/resume`, `/diff` page with it) paints on
# the alternate screen. Its switch and clear used to be queued *before* the
# first frame's synchronized update, and the return's LeaveAlternateScreen was
# flushed on its own before the inline repaint — three presentable blank-screen
# states on a terminal that renders what it has parsed. Record the raw stream
# across an open and a close and require every switch and every ED2 inside a
# `ESC[?2026h` … `ESC[?2026l` block (docs/ui/flicker.md).
S158="${S}_overlaysync"
RAW158="$(mktemp)"
launch "$S158" 80 24
tmux pipe-pane -t "$S158" -o "cat > $RAW158"
tmux send-keys -t "$S158" C-o
wait_for 5 "$S158" -F "T R A N S C R I P T"
tmux send-keys -t "$S158" Escape
sleep 0.6
# Quit only after the return's repaint has run, so the teardown's own
# LeaveAlternateScreen (if any) is not what the check reads.
tmux send-keys -t "$S158" C-c
sleep 0.4
tmux pipe-pane -t "$S158"
tmux kill-session -t "$S158" 2>/dev/null

sync_switch=$(sed -e $'s/\x1b\[?2026h/\\\n@SYNC@\\\n/g' \
	-e $'s/\x1b\[?2026l/\\\n@ENDS@\\\n/g' \
	-e $'s/\x1b\[?1049h/\\\n@ALTIN@\\\n/g' \
	-e $'s/\x1b\[?1049l/\\\n@ALTOUT@\\\n/g' \
	-e $'s/\x1b\[2J/\\\n@CLR@\\\n/g' "$RAW158" | awk '
	/@SYNC@/ { depth = 1; frames++; next }
	/@ENDS@/ { depth = 0; next }
	/@ALTIN@/ { altin++; if (depth == 0) bad++ }
	/@ALTOUT@/ { altout++; if (depth == 0) bad++ }
	/@CLR@/ { clr++; if (depth == 0) bad++ }
	END { printf "frames=%d altin=%d altout=%d clr=%d outside=%d", frames + 0, altin + 0, altout + 0, clr + 0, bad + 0 }')
rm -f "$RAW158"
echo "==== Phase 158: alternate-screen switch and clear in the raw output stream — $sync_switch ===="

case "$sync_switch" in
*altin=0* | *altout=0*)
	fail "the Ctrl+O overlay never entered or left the alternate screen ($sync_switch) — the recording or the overlay is broken"
	;;
*outside=0) ;;
*)
	fail "an alternate-screen switch or clear sits OUTSIDE a synchronized-update frame ($sync_switch) — opening or closing the overlay can present a blank screen"
	;;
esac
