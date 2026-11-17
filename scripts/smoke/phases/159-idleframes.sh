#!/usr/bin/env bash
# Phase 159 — an idle ask modal emits no frames at all

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# The inline animation chain ticks every 32 ms while a turn runs. It used to
# paint (and *seat*) the live region on every tick even when not one cell had
# changed: an idle ask modal recorded 34/35 frames whose whole body was
# `Hide` + a cursor move to the same seat. On a terminal with a cursor-trail
# animation (kitty's `cursor_trail`) those idle writes play as a flicker.
# `InlineViewport::draw_still` now returns before any I/O when the buffer and
# the seat are both unchanged; only the modal's once-a-second Timeout tick may
# still write. Record the raw stream while the modal is idle and require no
# Hide+CUP-only frame in it.
S159="${S}_idleframes"
APP_IDLE="env $CFG_ENV_NOHIST ZEROTH_STARTUP_DELAY_MS=500 $BIN"
launch "$S159" 80 24 "$APP_IDLE"
submit "$S159" "ask me some questions"
ask_shown=""
for _ in $(seq 1 250); do
	if tmux capture-pane -t "$S159" -p | grep -qF "favorite way to drink coffee"; then
		ask_shown=1
		break
	fi
	sleep 0.05
done
if [ -z "$ask_shown" ]; then
	fail "the ask modal never showed"
	tmux kill-session -t "$S159" 2>/dev/null
else
	RAW159="$(mktemp)"
	tmux pipe-pane -t "$S159" -o "cat > $RAW159"
	sleep 1.0
	tmux pipe-pane -t "$S159"
	idle_frames=$(python3 - "$RAW159" <<'PY'
import re, sys
raw = open(sys.argv[1], "rb").read()
frames = re.findall(rb"\x1b\[\?2026h(.*?)\x1b\[\?2026l", raw, re.S)
pure = sum(1 for f in frames if re.fullmatch(rb"\x1b\[\?25l\x1b\[\d+;\d+H", f))
print(f"frames={len(frames)} pure={pure}")
PY
)
	rm -f "$RAW159"
	echo "==== Phase 159: idle ask-modal frames in the raw output stream — $idle_frames ===="
	case "$idle_frames" in
	*pure=0) ;;
	*)
		fail "the idle ask modal wrote Hide+cursor-only frames ($idle_frames) — an animation tick repainted an unchanged region"
		;;
	esac
	tmux kill-session -t "$S159" 2>/dev/null
fi
echo "==== Phase 159: the idle ask modal is silent between its Timeout ticks ===="
