#!/usr/bin/env bash
# scripts/release/gate.sh — a release is built only from a commit whose CI
# passed.
#
#   scripts/release.sh gate [REPO] [COMMIT]           # COMMIT defaults to HEAD
#   scripts/release.sh gate --sha COMMIT --token-var GH_TOKEN_FILE
#   scripts/release.sh gate --inspect CHECKS.json [RUNS.json]
#
# A push to main already runs CI (gate, smoke, macOS, the release tooling).
# The release workflow used to run those same jobs a second time for one
# commit. It now reads their result instead: `inspect` is the pure rule over
# the GitHub checks API's answer — the required jobs must each be `success`
# for this exact commit, and a release is refused when one failed, is still
# running, or has no run at all.
#
#   fmt · clippy · test · doc      required
#   clippy · test · doc (macOS)    required
#   smoke suite (tmux)             required
#   release tooling                required
#   clipboard (X11), clipboard (macOS pasteboard), smoke readers (BSD sed)
#                                  reported, not gating
#
# The `inspect` form reads a saved checks answer (a workflow-runs answer may
# be given beside it, so a job still queued is `pending`, not `missing`) and
# prints one `<name><TAB><state>` line per job. The remote form fetches those
# answers with curl; `--token-var NAME` reads the token from the file NAME
# holds (a workflow passes `GH_TOKEN_FILE`, so the value never reaches a
# command line). See docs/ops/release.md.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

api="${GITHUB_API_URL:-https://api.github.com}"
repo=""
ref=""
sha=""
inspect=""
runs=""
token_var=""
while [ $# -gt 0 ]; do
	case "$1" in
	--inspect)
		inspect="$2"
		shift 2
		;;
	--runs)
		runs="$2"
		shift 2
		;;
	--sha)
		sha="$2"
		shift 2
		;;
	--token-var)
		token_var="$2"
		shift 2
		;;
	-h | --help)
		sed -n '2,/^set -euo/p' "${BASH_SOURCE[0]}" | sed '$d' | sed 's/^# \{0,1\}//'
		exit 0
		;;
	-*) die "gate: unknown option $1" ;;
	*)
		if [ -z "$repo" ]; then
			repo="$1"
		elif [ -z "$ref" ]; then
			ref="$1"
		else
			die "gate: unexpected argument $1"
		fi
		shift
		;;
	esac
done

# The pure rule: one `<name><TAB><state>` line per gate job, `state` one of
# `success | failure | pending | missing`.
#
# A job may appear more than once in the answer (a workflow re-run, a
# previous attempt): the latest attempt is its state — a failed run re-run
# green passes; a success re-run still running does not. A required job with no check at all is `missing` — except while a
# workflow run for the commit is itself queued/in progress, when it is
# `pending`: its job simply has not been created yet.
inspect() {
	python3 - "$1" "${2:-}" <<'PY'
import json
import sys

CHECKS_PATH, RUNS_PATH = sys.argv[1], sys.argv[2]
REQUIRED = [
    "fmt · clippy · test · doc",
    "clippy · test · doc (macOS)",
    "smoke suite (tmux)",
    "release tooling",
]
REPORTED = ["smoke readers (BSD sed)", "clipboard (X11)", "clipboard (macOS pasteboard)"]
SUCCESS = {"success", "skipped", "neutral"}


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def named(data):
    """Every check in either shape the API answers with."""
    if isinstance(data, list):
        return data
    for key in ("check_runs", "workflow_runs"):
        if key in data:
            return data[key]
    return []


def state_of(check):
    if (check.get("status") or "completed") != "completed":
        return "pending"
    return "success" if (check.get("conclusion") or "") in SUCCESS else "failure"


def pick(states):
    # The latest attempt is the job's state: a failed run re-run green is a
    # pass, and a success re-run that is still going is not proof any more.
    for wanted in ("success", "pending", "failure"):
        if wanted in states:
            return wanted
    return ""


checks = named(load(CHECKS_PATH))
by_name = {}
for check in checks:
    name = check.get("name", "")
    by_name.setdefault(name, []).append(state_of(check))

delayed = []
if RUNS_PATH:
    delayed = [
        run.get("name", "")
        for run in named(load(RUNS_PATH))
        if (run.get("status") or "") != "completed"
    ]

for name in REQUIRED:
    state = pick(by_name.get(name, []))
    if not state:
        state = "pending" if delayed else "missing"
    print(f"{name}\t{state}")
for name in REPORTED:
    if name in by_name:
        print(f"reported\t{name}\t{pick(by_name[name])}")
PY
}

sign_request() {
	local token="$1"
	printf 'header = "Authorization: Bearer %s"\n' "$token"
}

if [ -n "$inspect" ]; then
	[ -f "$inspect" ] || die "gate --inspect: no file at $inspect"
	if [ -n "$runs" ] && [ ! -f "$runs" ]; then
		die "gate --runs: no file at $runs"
	fi
	rows="$(inspect "$inspect" "$runs")"
	tab="$(printf '\t')"
	failed=0
	waiting=0
	while IFS="$tab" read -r first second third; do
		[ -n "$first" ] || continue
		if [ "$first" = reported ]; then
			case "$third" in
			success) ok "$second (reported)" ;;
			*) warn "$second (reported): $(printf '%s' "$third" | tr '[:upper:]' '[:lower:]')" ;;
			esac
			continue
		fi
		[ -n "$second" ] || continue
		state="$second"
		case "$state" in
		success) ok "$first" ;;
		pending) warn "$first: still running"; waiting=1 ;;
		missing) fail "$first: no CI evidence"; failed=1 ;;
		*) fail "$first: $(printf '%s' "$state" | tr '[:upper:]' '[:lower:]')"; failed=1 ;;
		esac
	done <<<"$rows"
	if [ "$waiting" -eq 1 ]; then
		fail "CI for this commit has not finished — wait for it rather than build on it"
		failed=1
	fi
	# `finish` exits 1 when a check failed and prints the summary either way;
# a clean pass is this call's own `ok` first, so both modes read alike.
	finish "release gate passed"
	[ "$failed" -eq 0 ]
	exit
fi

[ -n "$repo" ] || repo="$(repo_slug)"
if [ -z "$ref" ]; then
	ref="$sha"
fi
if [ -z "$ref" ]; then
	ref="$(git -C "$RELEASE_ROOT" rev-parse HEAD 2>/dev/null)" ||
		die "gate: not in a git checkout — give a COMMIT"
fi
if [ -n "$sha" ] && [ "$sha" != "$ref" ]; then
	die "gate: $sha is not $ref — the release is built from the tag's commit"
fi

token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
if [ -n "$token_var" ]; then
	token_file="${!token_var:-}"
	[ -n "$token_file" ] && [ -f "$token_file" ] ||
		die "gate: \$$token_var does not name a readable token file"
	token="$(cat "$token_file")"
fi
[ -n "$token" ] ||
	die "gate: no GitHub token — set GITHUB_TOKEN, or --token-var NAME naming the file that holds it"
command -v curl >/dev/null 2>&1 || die "gate: curl is not installed"
[ -n "$api" ] || die "gate: the API address is empty"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
sign_request "$token" >"$work/auth"
chmod 600 "$work/auth"
fetch() {
	curl -fsSL --retry 2 --config "$work/auth" \
		-H 'Accept: application/vnd.github+json' \
		-H 'X-GitHub-Api-Version: 2022-11-28' \
		-H 'User-Agent: zeroth-release' "$1"
}
fetch "$api/repos/$repo/commits/$ref/check-runs?per_page=100" >"$work/checks" ||
	die "gate: could not read the check runs for $repo@$ref"
fetch "$api/repos/$repo/actions/runs?head_sha=$ref&per_page=100" >"$work/runs" 2>/dev/null ||
	: >"$work/runs"
printf '{"workflow_runs":[]}\n' >"$work/no-runs"
runs_file="$work/runs"
[ -s "$runs_file" ] || runs_file="$work/no-runs"

rows="$(inspect "$work/checks" "$runs_file")" || die "gate: the API answer is not the shape this reads"
tab="$(printf '\t')"
failed=0
while IFS="$tab" read -r label state; do
	[ -n "$label" ] || continue
	case "$state" in
	success) ok "$repo@$ref · $label" ;;
	missing) fail "$repo@$ref · $label: no CI evidence — push it to main and let CI finish"; failed=1 ;;
	*) fail "$repo@$ref · $label: $(printf '%s' "$state" | tr '[:upper:]' '[:lower:]')"; failed=1 ;;
	esac
done <<<"$rows"
[ "$failed" -eq 0 ] || finish "release gate failed for $repo@$ref — publish an unproven commit"
finish "release gate passed for $repo@$ref"
