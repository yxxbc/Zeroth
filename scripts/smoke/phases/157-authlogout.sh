#!/usr/bin/env bash
# Phase 157 — credential refresh across processes, one 401 recovery and account/API logout

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

A_WORK="$(work_dir authlogout)"
A_ENV="$SMOKE_TMP/credentials.env"
A_LOG="$SMOKE_TMP/auth.log"
A_READY="$SMOKE_TMP/auth.port"
printf '%s\n' 'CUSTOM_AUTH_RT=rt-0' 'CHATGPT_CODEX_REFRESH_TOKEN=unrelated-account' 'FIXTURE_API_KEY=saved-key' >"$A_ENV"

# Strict, single-use refresh tokens. These are entirely synthetic credentials;
# every URL this phase configures points at this loopback server.
python3 - "$A_READY" "$A_LOG" <<'PY' &
import base64
import json
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ready, log = map(Path, sys.argv[1:])
gate = threading.Lock()
generation = 0
rejected = False

def jwt(n):
    claims = {"exp": int(time.time()) + 3600,
              "https://api.openai.com/auth": {"chatgpt_account_id": "fixture", "chatgpt_plan_type": "pro"},
              "generation": n}
    return "e30." + base64.urlsafe_b64encode(json.dumps(claims).encode()).rstrip(b"=").decode() + ".sig"

class Stub(BaseHTTPRequestHandler):
    def reply(self, status, obj):
        data = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def note(self, text):
        with log.open("a") as stream:
            stream.write(text + "\n")

    def do_GET(self):
        self.reply(200, {"models": [{"slug": "fixture", "id": "fixture", "display_name": "Fixture", "context_window": 32768}],
                         "data": [{"id": "fixture"}]})

    def do_POST(self):
        global generation, rejected
        body = json.loads(self.rfile.read(int(self.headers.get("content-length", "0"))))
        if self.path == "/oauth/token":
            with gate:
                if body.get("refresh_token") != f"rt-{generation}":
                    self.note("REUSED_REFRESH")
                    self.reply(400, {"error": "refresh_token_reused"})
                    return
                generation += 1
                self.note(f"ROTATED {generation}")
                # Widen the overlap so separate processes would replay rt-0
                # without a lock held through exchange and persistence.
                time.sleep(0.15)
                self.reply(200, {"access_token": jwt(generation), "refresh_token": f"rt-{generation}", "expires_in": 3600})
            return
        if self.path.endswith("/responses"):
            with gate:
                if not rejected:
                    rejected = True
                    self.note("INFERENCE_401")
                    self.reply(401, {"error": "invalid_token"})
                    return
            self.note("INFERENCE_OK")
            data = ('data: ' + json.dumps({"type": "response.output_text.delta", "delta": "AUTH_OK"}) + '\n\n'
                    + 'data: ' + json.dumps({"type": "response.completed", "response": {"status": "completed"}}) + '\n\n').encode()
            self.send_response(200)
            self.send_header("content-type", "text/event-stream")
            self.send_header("content-length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        self.reply(404, {})

    def log_message(self, *_args):
        pass

server = ThreadingHTTPServer(("127.0.0.1", 0), Stub)
ready.write_text(str(server.server_port))
server.serve_forever()
PY
A_SERVER=$!
smoke_on_exit kill "$A_SERVER"
for _ in $(seq 1 50); do
	[ -s "$A_READY" ] && break
	sleep 0.1
done
[ -s "$A_READY" ] || fail "the authentication fixture never started"
A_BASE="http://127.0.0.1:$(cat "$A_READY")"
A_PROVIDERS="$SMOKE_TMP/auth-providers.toml"
cat >"$A_PROVIDERS" <<TOML
[providers.chatgpt_codex]
name = "ChatGPT Codex"
auth = "chatgpt_codex"
wire_api = "responses"
api_key_env = "CUSTOM_AUTH_RT"
[providers.chatgpt_codex.kwargs]
api_base = "$A_BASE"
[providers.fixture_api]
name = "Fixture API"
api_key_env = "FIXTURE_API_KEY"
[providers.fixture_api.kwargs]
api_base = "$A_BASE"
TOML
A_FLAGS="NO_PROXY=127.0.0.1 no_proxy=127.0.0.1 ZEROTH_OPENAI_ISSUER=$A_BASE ZEROTH_ENV_FILE=$A_ENV ZEROTH_PROVIDERS_FILE=$A_PROVIDERS ZEROTH_PROVIDER=chatgpt_codex ZEROTH_MODEL=fixture CUSTOM_AUTH_RT=rt-0"

# Separate processes start with the same stale exported token and share only
# the on-disk store. Both must finish; the first inference 401 recovers once.
env $CFG_ENV_NOHIST $A_FLAGS "$BIN_ABS" -C "$A_WORK" -p "reply" >"$SMOKE_TMP/one.out" 2>"$SMOKE_TMP/one.err" &
A_ONE=$!
env $CFG_ENV_NOHIST $A_FLAGS "$BIN_ABS" -C "$A_WORK" -p "reply" >"$SMOKE_TMP/two.out" 2>"$SMOKE_TMP/two.err" &
A_TWO=$!
wait "$A_ONE"; A_CODE_ONE=$?
wait "$A_TWO"; A_CODE_TWO=$?
dump "first process stderr" "$(cat "$SMOKE_TMP/one.err")"
dump "second process stderr" "$(cat "$SMOKE_TMP/two.err")"
expect_eq "$A_CODE_ONE" "0" "the first process could not refresh/recover"
expect_eq "$A_CODE_TWO" "0" "the second process replayed a rotating credential"
expect_has "$(cat "$SMOKE_TMP/one.out")" -F "AUTH_OK" "first process never answered"
expect_has "$(cat "$SMOKE_TMP/two.out")" -F "AUTH_OK" "second process never answered"
expect_lacks "$(cat "$A_LOG")" -F "REUSED_REFRESH" "a retired refresh token was presented"
expect_has "$(cat "$A_ENV")" -E '^CHATGPT_CODEX_REFRESH_TOKEN=unrelated-account$' "refresh overwrote an unrelated default credential"
expect_eq "$(grep -c '^INFERENCE_401$' "$A_LOG")" "1" "the controlled 401 was not exercised"
if [ "$A_CODE_ONE" -ne 0 ] || [ "$A_CODE_TWO" -ne 0 ]; then
	smoke_finish
fi

A_SESSION="${S}_authlogout"
A_APP="env $A_FLAGS FIXTURE_API_KEY=exported-key ZEROTH_API_KEY=generic-key $APP_ABS"
launch -c "$A_WORK" "$A_SESSION" 100 34 "$A_APP"
submit "$A_SESSION" "/logout"
wait_pane 10 "$A_SESSION" -F "API key" >/dev/null || fail "the connection page did not open"
type_text "$A_SESSION" "fixture"
sleep "$SMOKE_TYPE_SETTLE"
keys "$A_SESSION" Enter
a_confirm="$(wait_pane 10 "$A_SESSION" -F "Remove API key")" || fail "API logout did not ask for confirmation"
expect_has "$a_confirm" -F "Cancel" "the confirmation has no cancel choice"
keys "$A_SESSION" Enter
expect_has "$(cat "$A_ENV")" -E '^FIXTURE_API_KEY=' "the default confirmation action removed a key"
keys "$A_SESSION" Enter Down Enter
a_removed="$(wait_pane 10 "$A_SESSION" -F "Disconnected Fixture API")" || fail "the API connection was not removed"
expect_lacks "$(cat "$A_ENV")" -E '^FIXTURE_API_KEY=' "the API key remained on disk"

# The /login root opens exactly the same manager; account rows say Sign out.
submit "$A_SESSION" "/login"
keys "$A_SESSION" End Enter
type_text "$A_SESSION" "chatgpt"
sleep "$SMOKE_TYPE_SETTLE"
keys "$A_SESSION" Enter
a_account="$(wait_pane 10 "$A_SESSION" -F "Sign out · ChatGPT Codex")" || fail "the /login management entry did not reach account sign-out"
keys "$A_SESSION" Down Enter
a_signed_out="$(wait_pane 10 "$A_SESSION" -F "Disconnected ChatGPT Codex")" || fail "account logout did not complete"
expect_lacks "$(cat "$A_ENV")" -E '^CUSTOM_AUTH_RT=' "the refresh token remained on disk"
tmux kill-session -t "$A_SESSION" 2>/dev/null

# Restart with both specific and generic environment credentials. A deliberate
# logout must stay a logout and must not fall back to the scripted demo.
env $CFG_ENV_NOHIST $A_FLAGS FIXTURE_API_KEY=exported-key ZEROTH_API_KEY=generic-key ZEROTH_PROVIDER=fixture_api \
	"$BIN_ABS" -C "$A_WORK" -p "reply" >"$SMOKE_TMP/disabled.out" 2>"$SMOKE_TMP/disabled.err"
A_DISABLED=$?
expect_eq "$A_DISABLED" "1" "a logged-out API provider silently reactivated after restart"
expect_has "$(cat "$SMOKE_TMP/disabled.err")" -F "signed out" "the refusal does not explain how to reconnect"
env $CFG_ENV_NOHIST $A_FLAGS ZEROTH_API_KEY=generic-key \
	"$BIN_ABS" -C "$A_WORK" -p "reply" >"$SMOKE_TMP/account.out" 2>"$SMOKE_TMP/account.err"
expect_eq "$?" "1" "a logged-out account reactivated from its exported token"
expect_has "$(cat "$SMOKE_TMP/account.err")" -F "signed out" "account restart did not preserve the logout"
expect_lacks "$(cat "$A_LOG")" -F "REUSED_REFRESH" "logout allowed an exported retired token to refresh"

# Saving a key clears the persisted disconnection marker.
launch -c "$A_WORK" "$A_SESSION" 100 34 "$A_APP"
submit "$A_SESSION" "/login"
keys "$A_SESSION" Down Enter
type_text "$A_SESSION" "fixture"
sleep "$SMOKE_TYPE_SETTLE"
keys "$A_SESSION" Enter
type_text "$A_SESSION" "new-key"
sleep "$SMOKE_TYPE_SETTLE"
keys "$A_SESSION" Enter
wait_pane 10 "$A_SESSION" -F "Saved" >/dev/null || fail "API login did not save the replacement"
submit "$A_SESSION" "/logout"
wait_pane 10 "$A_SESSION" -F "API key" >/dev/null || fail "the connection page did not open"
type_text "$A_SESSION" "fixture"
expect_has "$(pane "$A_SESSION")" -F "Fixture API · API key" "login did not restore the connection"
smoke_finish
