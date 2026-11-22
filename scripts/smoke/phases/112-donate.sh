#!/usr/bin/env bash
# Phase 112 — the `/donate` page

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# This phase's fixture is the ABSENCE of a native clipboard: the app's
# OSC 52 fallback is what it reads back, out of tmux's paste buffer. A macOS
# GUI login HAS a pasteboard — the native path wins, nothing is written as
# OSC 52, and making the fallback fire would mean overwriting the developer's
# real clipboard (the suite's rule: never). So skip, loudly, rather than fail
# a check about a mechanism this machine cannot reach. The clipboard's macOS
# half is covered by `tests/clipboard_macos.rs`, which CI runs on a macOS
# runner with `--ignored`.
if [ "$(uname -s)" = Darwin ] && command -v launchctl >/dev/null 2>&1 &&
	[ "$(launchctl managername 2>/dev/null)" = Aqua ]; then
	smoke_skip "a GUI login has a native clipboard, so the OSC 52 fallback this phase reads cannot fire (set-clipboard would not see it)"
fi

# the `/donate` page (docs/ui/donate.md). The `/hooks` browser's sibling over
# the project's sponsorship links and its one crypto address: a gradient
# `♥ Support Zeroth` title over a dim blurb, the three off-chain links
# (Afdian, Email, WeChat) as clickable rows, then the `❯ 1. XKO  X Layer`
# row (the ticker and the coin — no network clause) over its rounded box,
# with the network it is reachable on captioned UNDER that box
# (`Network: X Layer`), an amber wrong-network caution pointing at that
# caption and a key hint; `c` copies the highlighted address — the toast
# names the coin, and headless here (the suite's own unset of the display
# variables) arboard has no clipboard server so the OSC 52 fallback lands the
# address VERBATIM in tmux's paste buffer (Phase 28's trick) while the page
# stays open; a digit past the one row names nothing and changes nothing;
# Esc brings the composer and its footer back.
S112="${S}_donate"
tmux new-session -d -s "$S112" -x 90 -y 40 "$APP"
# tmux must capture OSC 52 from the app into its own buffer (Phase 28).
tmux set-option -g set-clipboard on
sleep 0.5
tmux send-keys -t "$S112" -l "/donate"
sleep 0.4
donate_palette="$(tmux capture-pane -t "$S112" -p)"
echo "==== Phase 112: the palette filtered to /donate ===="
printf '%s\n' "$donate_palette"
expect_has "$donate_palette" -F "Support the project: links and the X Layer address" "/donate is missing from the slash-command palette"
tmux send-keys -t "$S112" Enter
sleep 0.5
donate_open="$(tmux capture-pane -t "$S112" -p)"
echo "==== Phase 112: the page open (XKO highlighted) ===="
printf '%s\n' "$donate_open"
for expect in "♥ Support Zeroth" "Free and open source" \
	"Afdian" "afdian.com/a/xynrin" "Email" "xynrin@icloud.com" \
	"WeChat" "github.com/yxxbc/zeroth#sponsorship" \
	"❯ 1. XKO  X Layer" "XKO456c0e87173789d770d6a031ba320863dce6d2cc" "Network: X Layer" \
	"↑↓ navigate  enter/c copy address  esc close"; do
	expect_has "$donate_open" -F "$expect" "the open page is missing '$expect'"
done
# The deleted addresses are gone, not merely folded: the catalog is one row.
for gone in "Bitcoin" "Ethereum" "Solana" "bc1qhwamfrwuhz64pk00l75ykfff2ang22ns64chf7" \
	"0xEAf6fbabB9DBE7a23BfE22A7A6c4aCe02063524b" "Gwhv5c6uAa6aAz1MjwzV9QJpbm7CJWy2kuCeZ75mFc94"; do
	expect_lacks "$donate_open" -F "$gone" "the page still shows the removed '$gone'"
done
# The caution is prose wrapped to the pane's width, so a phrase of it can
# straddle a row break (at these 90 columns it breaks after `cannot be`):
# read it whole off the rows joined back into one line, and read the whole
# sentence — the one line on the page that must say exactly what it says.
donate_open_prose="$(printf '%s\n' "$donate_open" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | paste -sd' ')"
if ! printf '%s' "$donate_open_prose" | grep -qF \
	"Send each coin only over a network listed under its address — a transfer on any other network cannot be recovered."; then
	fail "the open page is missing the wrong-network caution, whole"
fi
# A row is the ticker and the coin's name and nothing more: no network
# clause after the coin (read as the whole trimmed row, not a substring).
if ! printf '%s\n' "$donate_open" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | grep -qxF "❯ 1. XKO  X Layer"; then
	fail "the XKO row carries something after the coin"
fi
# The address sits in its rounded box: the row over it opens with ╭ and the
# row under it with ╰.
if ! printf '%s\n' "$donate_open" | grep -B1 -F "XKO456c0e87173789d770d6a031ba320863dce6d2cc" | head -1 | grep -qF "╭"; then
	fail "no rounded top over the XKO address"
fi
if ! printf '%s\n' "$donate_open" | grep -A1 -F "XKO456c0e87173789d770d6a031ba320863dce6d2cc" | tail -1 | grep -qF "╰"; then
	fail "no rounded bottom under the XKO address"
fi
# The network caption sits DIRECTLY under its box — the row two below the
# address (the box's bottom wall is the one between them) — so a reader who
# has found an address has found where it may be sent without hunting.
donate_caption_under() { # $1 address, $2 caption
	printf '%s\n' "$donate_open" | grep -A2 -F "$1" | tail -1 |
		sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | grep -qxF "$2"
}
if ! donate_caption_under "XKO456c0e87173789d770d6a031ba320863dce6d2cc" "Network: X Layer"; then
	fail "the XKO network caption is not under its box"
fi
# The page replaces the composer: its footer is off screen while it is up.
expect_lacks "$donate_open" -F "dummy_model_name ·" "the footer is still on screen under the page"
# `c` copies the highlighted address: the toast names the coin, and the
# OSC 52 fallback lands the address verbatim in tmux's buffer.
while tmux delete-buffer 2>/dev/null; do :; done
tmux send-keys -t "$S112" -l "c"
donate_copied="$(wait_pane 3 "$S112" -F "Copied the XKO address to clipboard")" # up to ~3s
echo "==== Phase 112: after c — the toast ===="
printf '%s\n' "$donate_copied"
expect_has "$donate_copied" -F "Copied the XKO address to clipboard" "c did not confirm the copy with a toast naming the coin"
donate_clip="$(tmux show-buffer 2>/dev/null)"
echo "==== Phase 112: tmux clipboard buffer (the OSC 52 fallback landed here) ===="
printf '%s\n' "$donate_clip"
if [ "$donate_clip" != "XKO456c0e87173789d770d6a031ba320863dce6d2cc" ]; then
	fail "the clipboard does not hold the XKO address verbatim"
fi
expect_has "$donate_copied" -F "❯ 1. XKO" "copying closed the page"
# A digit past the one row names nothing: `2` leaves the page open, the
# highlight where it was, and the draft untouched.
tmux send-keys -t "$S112" -l "2"
sleep 0.4
donate_two="$(tmux capture-pane -t "$S112" -p)"
echo "==== Phase 112: 2 names no row ===="
printf '%s\n' "$donate_two"
expect_has "$donate_two" -F "❯ 1. XKO  X Layer" "a digit past the last row moved the ❯"
expect_has "$donate_two" -F "♥ Support Zeroth" "a digit past the last row closed the page"
# Esc closes: the composer and its footer come back, the page is gone.
tmux send-keys -t "$S112" Escape
sleep 0.4
donate_closed="$(tmux capture-pane -t "$S112" -p)"
echo "==== Phase 112: after Esc ===="
printf '%s\n' "$donate_closed"
expect_has "$donate_closed" -F "dummy_model_name ·" "the composer and footer did not come back after Esc"
expect_lacks "$donate_closed" -F "XKO456c0e87173789d770d6a031ba320863dce6d2cc" "the page is still on screen after Esc"
tmux kill-session -t "$S112" 2>/dev/null
