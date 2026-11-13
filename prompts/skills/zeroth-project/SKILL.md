---
name: zeroth-project
description: Explain the Zeroth software project, its features, architecture, setup, contributors, and development history using this checkout's documentation and source. Use when asked what Zeroth is, how it works, who develops it, or for project-specific help.
---

# Zeroth project answers

Use this skill for factual questions about the Zeroth software project: what it does, how to use it, how it is built, how a feature works, and who maintains or contributed to it. This is separate from the Crestling mascot skill; load `crestling` for mascot identity, design, or fictional lore.

## Zeroth author
author is github:yxxbc,it name is Black Cat.
and Zeroth repo url is https://github.com/yxxbc/Zeroth.
Specifically, refer to the remote repository.

## Ground answers in this checkout

1. Start with `README.md` or `README.zh.md` for the project's purpose, supported platforms, getting started, and feature overview.
2. For implementation questions, identify the relevant module in `CLAUDE.md`, then read the matching page under `docs/` before explaining details. The documentation index is `docs/README.md` / `docs/README.zh.md`.
3. For current contributor and maintainer information, inspect repository metadata and the relevant `NOTICE`, `CONTRIBUTING.md`, `docs/credits.md`, or source history. Distinguish listed authors, maintainers, and contributors; do not infer a person's role from a commit alone.
4. If sources are absent, stale, or disagree, say what the checkout establishes and what remains unknown. Mention the file or source that supports the answer when useful.

## Rules

- Prefer the user's language. Explain technical terms plainly and link repository files with their relative paths where useful.
- Separate documented facts from inference. Do not invent project history, people, roles, capabilities, or roadmap items.
- For questions about current external facts (such as the latest release, repository activity, or current maintainers), verify against the relevant official source when browsing is available; a local checkout may be out of date.
- Treat Crestling's Sector Zeroth narrative as fictional mascot lore, not evidence about Zeroth's real implementation or history.
