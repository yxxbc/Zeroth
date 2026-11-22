#!/usr/bin/env bash
# Phase 164 — full-screen session tabs route focus, switches and mouse clicks.
. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

T="${S}_workspace"
tmux new-session -d -s "$T" -x 100 -y 34 "$APP_ABS --fullscreen"
wait_for 10 "$T" -E '^❯'
expect_has "$(tmux capture-pane -p -t "$T" | tail -1)" -F '● main' 'main tab missing'
submit "$T" 'launch a subagent that streams a table'
wait_for 15 "$T" -F '◯ Stream a comparison table'
tmux send-keys -t "$T" Down Right
sleep 0.2
expect_has "$(tmux capture-pane -p -t "$T" | tail -1)" -F '● main' 'focus switched the conversation'
tmux send-keys -t "$T" Enter
wait_for 10 "$T" -F 'Compare four languages'
expect_has "$(tmux capture-pane -p -t "$T" | tail -1)" -F '● Stream a comparison table' 'viewed agent tab not marked'
tmux send-keys -t "$T" Up
sleep 0.1
tmux send-keys -t "$T" -l 'agent draft'
wait_for 5 "$T" -F '❯ agent draft'
# SGR left press/release, one-based coordinates: main tab on the last row.
tmux send-keys -t "$T" -l $'\033[<0;3;34M\033[<0;3;34m'
wait_for 5 "$T" -F '● main'
# Reopen with the keyboard and verify the draft stayed with the agent.
tmux send-keys -t "$T" Right Enter
wait_for 5 "$T" -F '❯ agent draft'
tmux kill-session -t "$T"
smoke_finish
