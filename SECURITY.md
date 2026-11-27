# Security policy

## Supported versions

Zeroth is pre-1.0: only the **latest release** gets security fixes. The
`CHANGELOG.md` names the version this file was last reviewed against; older
tags are not patched, and a fix ships in a new release.

| Version | Supported |
| --- | --- |
| newest `vX.Y.Z` release | yes |
| everything older | no — upgrade first, then report if it still reproduces |

## Reporting a vulnerability

**Do not open a public issue.** Use GitHub's private vulnerability reporting:

1. Open <https://github.com/yxxbc/Zeroth/security/advisories/new> (or the
   repository's **Security → Report a vulnerability**).
2. Say what you saw, how to reproduce it, and what you think it lets an
   attacker do.

You get an acknowledgement when a maintainer reads it; there is no bug bounty
and no response-time promise, but a report against the permission gate or the
project-config trust gate will be treated as urgent even if you are not sure
it is exploitable.

## What counts as a security issue

Zeroth runs commands and writes files on the user's machine, so its attack
surface is larger than a library's. These are in scope:

- **Escaping the permission gate.** A `bash`/`write`/`edit`/`agent` call that
  runs, or writes, without asking where the mode and the allowlist say it
  must ask ([`docs/state/permissions.md`](docs/state/permissions.md)).
- **Escaping the workspace jail.** A file change outside the session's
  workspace that auto/edit mode does not notice.
- **Untrusted project config.** A repository's `hooks.json`, skills, agent
  definitions or `.zeroth` layer running before it was trusted
  ([`docs/state/config-review.md`](docs/state/config-review.md),
  [`docs/state/project-config.md`](docs/state/project-config.md)).
- **Prompt injection with effect.** Model-written content that causes a
  privileged action the user did not ask for — most usefully with a
  reproduction that gets past the auto-mode classifier.
- **Credential handling.** A way to read `~/.zeroth/.env`, the secrets store,
  a sign-in token or the masked-value placeholder
  ([`docs/state/secrets.md`](docs/state/secrets.md),
  [`docs/providers/credential-lifecycle.md`](docs/providers/credential-lifecycle.md)).
- **Outbound requests the user did not ask for** — a preview fetch, a
  redirect, an SSRF through a URL the model wrote
  ([`docs/tools/web-search.md`](docs/tools/web-search.md)).
- **The update path.** `zeroth update` or the release pipeline installing
  anything but the published artifact it verified
  ([`docs/ops/update.md`](docs/ops/update.md),
  [`docs/ops/release.md`](docs/ops/release.md)).

Not in scope: a model saying something wrong, a command the user approved
doing what it says, an issue that needs the attacker to already have the
user's shell, and anything in a third-party provider's own service.

## What a good report contains

- the Zeroth version (`zeroth --version`) and OS/terminal,
- the permission mode in use (the footer's right-edge label), and whether the
  directory's rules were saved,
- the smallest sequence that reproduces it — a prompt, a config file, a URL —
- what you expected versus what happened.

A `RUST_BACKTRACE=1` backtrace helps when the report is a crash.

## Disclosure

Please give a fix a chance to ship before writing it up. When a report lands,
the fix and the advisory are prepared together and the advisory is published
with the release that carries the fix; the reporter is credited unless they
ask not to be.
