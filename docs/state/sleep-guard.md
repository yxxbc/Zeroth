# Sleep guard — refuse a leading `sleep` before execution

Date: 2026-11-27

A `bash("sleep 30")` in auto or master mode currently runs unasked — `sleep`
is on the `read_only::ANY_ARGS` allowlist, so the classifier is never
consulted.  The command ties up the tool thread for the full duration, the
user sees nothing happen, and the model learns nothing from it: a pure waste
of a turn.  Models produce these calls often — training data is full of
`sleep N; curl …` polling loops — and nothing in the pipeline steers them
toward the event-driven alternative this project already has
(`docs/tools/background.md`).

The fix is a **static, pre-execution check** that refuses a command whose
first segment is a `sleep` of ≥ 2 seconds, with a result that tells the model
what to do instead.  It is not a classifier rule, not a prompt-injection
defence, and not a monitoring feature — it is a pure-function gate in the same
family as `read_only::is_read_only`, placed before it.

## What changes

### 1. Remove `sleep` from the read-only allowlist

`src/read_only/mod.rs` `ANY_ARGS`: delete `"sleep"`.  A `sleep 5` alone is
read-only in the filesystem sense, but letting it through the read-only fast
path means auto mode runs it without review — the exact problem.  After the
removal it falls through to the classifier, where the sleep guard below
catches it first.

### 2. Add `is_leading_sleep` to `read_only`

A pure function in `src/read_only/mod.rs`:

```rust
/// Whether `command` opens with a `sleep` whose argument is ≥ 2 seconds.
/// Only the **first** segment is tested: `postfix start; sleep 2; postfix
/// status` sleeps in a later segment and is legitimate.
pub fn is_leading_sleep(command: &str) -> bool {
    let Some(parsed) = lex::parse(command) else {
        return false;
    };
    let Some(first) = parsed.segments.first() else {
        return false;
    };
    if first.program() != Some("sleep") {
        return false;
    }
    // The argument may be integral (`30`) or fractional (`0.5`).  Anything
    // unparseable — a variable, a negative, a suffix — is not caught here
    // and falls through to the classifier (safe side).
    first
        .args()
        .first()
        .and_then(|a| a.parse::<f64>().ok())
        .is_some_and(|secs| secs >= 2.0)
}
```

The 2-second threshold keeps `sleep 0.1` and `sleep 1` alive — they appear
in legitimate `service start; sleep 1; service status` patterns even as a
first segment.  Anything ≥ 2 s at the top of a command is overwhelmingly a
"wait and see" that the background system handles better.

Only the first segment matters.  The lexer (`read_only::lex`) already splits
on `;`, `&&` and `||`, so these cases work correctly:

| command | first segment | caught? |
| --- | --- | --- |
| `sleep 30` | `sleep 30` | yes |
| `sleep 5; curl …` | `sleep 5` | yes |
| `sleep 10 && check` | `sleep 10` | yes |
| `postfix start; sleep 2; postfix status` | `postfix start` | no — correct |
| `while true; do sleep 1; curl …; done` | — | no — the lexer raises `hazards` for the loop, `is_read_only` already refuses it, the classifier sees it |
| `sleep 0.5` | `sleep 0.5` | no — below threshold |
| `sleep $INTERVAL` | `sleep $INTERVAL` | no — parse fails, falls to classifier |

### 3. Intercept in `approval.rs`

In `approve_call`, after the `is_read_only` fast path and before the
classifier dispatch, test `is_leading_sleep(&request.target)`.  On a hit,
return `Approval::Reject` with the denial text — no classifier round-trip,
no user prompt.  The check applies in every permission mode that would
otherwise run the command unasked (auto, master); in manual mode the user
prompt already gates it, and manual users who approve a `sleep 30` meant it.

The denial text, model-facing:

```
Not run: no need to wait.  Background commands notify you when they finish.
Use `wait: 0` to run a command in the background, or a loop to poll for
a condition.
```

The display text (the red cell the user sees):

```
Refused: sleep as first command wastes the turn — use wait: 0 for background work.
```

### 4. The cell

The refused call renders like any classifier denial: a red `●` header, the
denial text as the `⎿` body, no prompt shown.  `ToolStatus::Denied` (the
existing variant) carries it.

## What does not change

- **`bashwait`** is unrelated.  It is the tool the model uses to check on a
  *running session* — it blocks the tool thread only until the session
  produces output, exits, or its own `wait` budget passes, and it returns
  what happened.  It is the right half of the event-driven design; the sleep
  guard is the left half that pushes models toward it.
- **The classifier prompt** (`prompts/classifier.md`) already blocks
  `curl … | sh` and privilege escalation; it does not need a `sleep` rule.
  The static check is cheaper (no LLM call) and more reliable (no prompt
  parse).
- **The background system** (`docs/tools/background.md`).  `BgEvent::Exited`
  already notifies the model when a background command finishes;
  `BgEvent::Waiting` notifies when one stops to ask for input; the notice
  board delivers the result into the same turn or starts a follow-up.  The
  sleep guard's denial text points the model here.
- **`sleep` in a later segment.**  `service start; sleep 2; service status`
  is a standard pattern.  The guard tests only the first segment; the rest
  fall through to the classifier (or run unasked when `is_read_only` covers
  them).
- **Pipes, loops, backgrounds, subshells.**  The lexer marks all of these as
  `hazards`; `is_read_only` already refuses them and sends them to the
  classifier.  A `sleep` inside any of them is not the first segment of a
  flat command and is not caught here.

## Tests

Unit tests in `src/read_only/mod.rs` (or its `tests` submodule):

- `is_leading_sleep("sleep 30")` → `true`
- `is_leading_sleep("sleep 5; curl http://example.com")` → `true`
- `is_leading_sleep("sleep 0.5")` → `false` (below threshold)
- `is_leading_sleep("sleep 1")` → `false` (below threshold)
- `is_leading_sleep("postfix start; sleep 2; postfix status")` → `false`
  (not the first segment)
- `is_leading_sleep("echo hello; sleep 10")` → `false` (not the first
  segment)
- `is_leading_sleep("SLEEP=5; echo $SLEEP")` → `false` (`SLEEP` is an
  assignment, not the program)

An integration test in `src/llm/approval.rs` (the `tests` module):

- In auto mode, a `bash("sleep 30")` call returns `Approval::Reject` without
  consulting the classifier and without raising a user prompt.
- The rejected result text contains `wait: 0`.

Per `AGENTS.md`: `cargo test --lib read_only::` and `cargo test --lib
llm::approval::` cover the change; no smoke phase needed (no TUI / terminal
boundary touched).

## Why not a classifier/prompt rule

Three reasons:

1. **Cost.**  The classifier is an LLM call — a few hundred ms even on a
   local model, a network round trip on a remote one.  The static check is a
   string parse.
2. **Reliability.**  A prompt rule can be overridden by a creative command
   description ("this sleep is necessary because …").  The static check
   cannot.
3. **Precedent.**  `is_read_only` is exactly this pattern: a pure function
   that handles the common case before the classifier, with a strict-side
   miss (a sleep the check doesn't catch still goes to the classifier, never
   to unasked execution).

## Why not block `sleep` everywhere

A `sleep` in the middle or tail of a command is a synchronisation primitive:
`service start; sleep 2; service status` is the standard way to wait for a
service to initialise.  Blocking it would break every real sysadmin pattern
the model knows.  The guard blocks only the degenerate case: a command that
*begins* by doing nothing.
