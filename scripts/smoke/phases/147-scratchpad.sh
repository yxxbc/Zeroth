#!/usr/bin/env bash
# Phase 147 — no agent scratchpad is injected into the context

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

S147="${S}_noscratchpad"
PAD147="$SMOKE_TMP/retired-agent-pad"
APP147="env $CFG_ENV_NOHIST ZEROTH_SCRATCHPAD=1 ZEROTH_SCRATCHPAD_DIR=$PAD147 $BIN_ABS"
launch "$S147" 100 40 "$APP147"
keys "$S147" C-d
wait_for 5 "$S147" -F "C O N T E X T" || fail "the context view did not open"
keys "$S147" Home
context147="$(pane "$S147" -S -400)"
expect_lacks "$context147" -F "## Scratchpad" "the context still advertises a scratchpad"
expect_lacks "$context147" -F "$PAD147" "the retired directory is in the context"
[ ! -e "$PAD147" ] || fail "the retired scratchpad directory was created"
keys "$S147" q
submit "$S147" '!test -z "${ZEROTH_SCRATCHPAD+x}" && test -z "${ZEROTH_SCRATCHPAD_DIR+x}" && echo NO_SCRATCHPAD_""ENV'
wait_for 10 "$S147" -F "NO_SCRATCHPAD_ENV" || fail "the shell did not run"
smoke_finish
