#!/usr/bin/env bash
# Phase 160 — typing moves the caret without a Hide/Show pair per frame

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# The ask entry field shows the hardware cursor. Typing used to make every
# frame `Hide` … `Show` around the cell diff; kitty's cursor trail (and an
# IME's preedit) reacts to that toggle. The fixed model is the one the other
# terminal harnesses use: a visible caret only moves, a hidden one only moves
# (no per-frame hide/show). Record ten typed characters and require no frame
# with both escapes — and no Hide at all, since the entry's caret is visible.
S160="${S}_caretshow"
APP_CARET="env $CFG_ENV_NOHIST ZEROTH_STARTUP_DELAY_MS=500 $BIN"
launch "$S160" 80 24 "$APP_CARET"
submit "$S160" "ask me some questions"
ask_shown=""
for _ in $(seq 1 250); do
	if tmux capture-pane -t "$S160" -p | grep -qF "favorite way to drink coffee"; then
		ask_shown=1
		break
	fi
	sleep 0.05
done
if [ -z "$ask_shown" ]; then
	fail "the ask modal never showed"
	tmux kill-session -t "$S160" 2>/dev/null
else
	# 4 opens the free-text Other row; its entry field owns the caret.
	tmux send-keys -t "$S160" -l "4"
	sleep 0.4
	cursor_shown="$(tmux display-message -p -t "$S160" '#{cursor_flag}')"
	RAW160="$(mktemp)"
	tmux pipe-pane -t "$S160" -o "cat > $RAW160"
	for c in a b c d e f g h i j; do
		tmux send-keys -t "$S160" -l "$c"
		sleep 0.08
	done
	sleep 0.2
	tmux pipe-pane -t "$S160"
	typed="$(tmux capture-pane -t "$S160" -p)"
	caret_frames=$(python3 - "$RAW160" <<'PY'
import re, sys
raw = open(sys.argv[1], "rb").read()
frames = re.findall(rb"\x1b\[\?2026h(.*?)\x1b\[\?2026l", raw, re.S)
hide = sum(b"\x1b[?25l" in f for f in frames)
show = sum(b"\x1b[?25h" in f for f in frames)
both = sum(b"\x1b[?25l" in f and b"\x1b[?25h" in f for f in frames)
print(f"frames={len(frames)} hide={hide} show={show} both={both}")
PY
)
	rm -f "$RAW160"
	echo "==== Phase 160: ask entry typing frames — $caret_frames (cursor_flag=$cursor_shown) ===="
	if [ "$cursor_shown" != "1" ]; then
		fail "the ask entry field is typed into but shows no cursor (cursor_flag=$cursor_shown)"
	fi
	expect_has "$typed" -F "abcdefghij" "the typed characters never reached the entry field"
	case "$caret_frames" in
	*both=0)
		;;
	*)
		fail "a typing frame hides and shows the cursor ($caret_frames) — a visible caret must only move"
		;;
	esac
	case "$caret_frames" in
	*hide=0*) ;;
	*)
		fail "a typing frame hides the visible caret ($caret_frames)"
		;;
	esac
	tmux kill-session -t "$S160" 2>/dev/null
fi
echo "==== Phase 160: a visible ask caret only moves ===="
