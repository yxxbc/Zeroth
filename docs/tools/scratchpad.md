# Session temporary storage and retired agent scratchpad

Date: 2026-11-17

## Decision

Zeroth no longer provisions a scratchpad for the agent. Sessions do not create
a scratchpad directory, append scratchpad instructions or paths to the system
prompt, export scratchpad variables, or grant a special write exemption.
The same rule applies to subagents and batch jobs. Long generated paths and
instructions about where every working note belongs add context without
advancing the user's task.

The former `ZEROTH_SCRATCHPAD` and `ZEROTH_SCRATCHPAD_DIR` switches are ignored
at startup and removed from tool shell environments. No replacement directory
is imposed on the model. Temporary files explicitly requested by the user use
the ordinary file tools and permission policy.

## Internal storage

Background process output and resized image payloads still require storage:

```text
{tmp}/zeroth-{uid}/{session}/
├── tasks/                 background process output
└── images/                resized image payload cache
```

These paths are computed per session. There is no process-global scratchpad
value. Background tools expose output locations when needed; the image cache
is an implementation detail and is not injected into the prompt.
See [background commands](background.md) and [images](../ui/images.md).

## Compatibility

Existing files are left in place. Old rollout records keep their tool outputs,
approval notes and paths. Display-only recognition of historical scratchpad
edits remains so saved conversations continue to render. It grants no current
permission and does not create a new directory. Hook payloads omit
`scratchpad_dir` because new sessions have no agent scratchpad.

## Verification

The headless integration test starts with the retired switches set and checks
that no directory or prompt block is created and shell children receive neither
variable. The session-storage smoke phase checks that background process output
still works without an agent scratchpad. Historical display rendering remains
covered by the existing UI tests.
