#!/usr/bin/env bash
# Phase 155 — zeroth -p --jobs runs several headless sessions in one process

. "$(dirname "${BASH_SOURCE[0]}")/../lib.sh"
smoke_begin

# Batch mode (docs/cli/batch.md): the first client of the multi-threaded core.
# Two dummy-backend jobs run in one process with a bounded pool; stdout carries
# one JSON result per job, the per-job logs carry the reporter output, and the
# process exits 0. No terminal is involved, so this phase needs no tmux.
B_WORK="$(work_dir batch)"
B_JOBS="$B_WORK/jobs.jsonl"
printf '%s\n' \
	'{"id":"one","prompt":"hello"}' \
	'{"id":"two","prompt":"hello"}' > "$B_JOBS"
B_OUT="$SMOKE_TMP/batch155.out"
B_ERR="$SMOKE_TMP/batch155.err"
env $CFG_ENV_NOHIST ZEROTH_DUMMY=1 ZEROTH_STARTUP_DELAY_MS=0 ZEROTH_CHUNK_DELAY_MS=0 \
	"$BIN_ABS" -p --jobs "$B_JOBS" --concurrency 2 --out-dir "$B_WORK/logs" \
	> "$B_OUT" 2> "$B_ERR"
B_CODE=$?
dump "zeroth -p --jobs jobs.jsonl --concurrency 2 (stdout)" "$(cat "$B_OUT")"
dump "zeroth -p --jobs jobs.jsonl --concurrency 2 (stderr)" "$(cat "$B_ERR")"

expect_eq "$B_CODE" "0" "the batch did not exit 0"
expect_eq "$(wc -l < "$B_OUT" | tr -d ' ')" "2" "the batch did not print one result per job"
expect_has "$(cat "$B_OUT")" -F '"id":"one"' "the first job is missing from the results"
expect_has "$(cat "$B_OUT")" -F '"id":"two"' "the second job is missing from the results"
expect_eq "$(grep -c '"exit_code":0' "$B_OUT" | tr -d ' ')" "2" "a job did not report exit_code 0"
expect_eq "$(cat "$B_ERR")" "" "the batch wrote to stderr"
for b_log in "$B_WORK"/logs/001-one.log "$B_WORK"/logs/002-two.log; do
	[ -s "$b_log" ] || fail "the job log $b_log is missing or empty"
	expect_has "$(cat "$b_log")" -F "Two commands away" "the job log $b_log does not hold the run's answer"
done

smoke_finish
