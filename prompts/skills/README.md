# Built-in skills

Everything in this directory is embedded in the binary (`include_str!` in
`src/llm/skill.rs`) and seeded into `<config home>/skills` on first run, so a
fresh install lists exactly what is authored here. A skill is a directory: its
`SKILL.md` is parsed, and any sibling files ride along.

## Provenance and licensing

The crate is Apache-2.0 (root [`LICENSE`](../../LICENSE)). This directory only
carries material that may be redistributed under that license:

| Skill | Source | License |
|---|---|---|
| `frontend-design` | Anthropic's official skill set, via the local skill library | Apache-2.0, carried verbatim in `frontend-design/LICENSE.txt` |
| `flaredantic`, `jina-reader`, `skill-creator`, `yt-dlp` | the Alter Zero base project (this fork's ancestor) | Apache-2.0, the same root license |
| `crestling`, `zeroth-project` | Original Zeroth project content, contributed by the project maintainer | Apache-2.0, the same root license |

`web-search` is deliberately absent: Zeroth's native `web_search`/`web_fetch`
tools replace it (`docs/tools/web-search.md`).

## What was not copied

`~/.claude/skills` and `~/.alter-zero/skills` also held personal or
third-party skills with **no redistribution grant** — for example
`agent-browser`, `agently-mail`, the `cnb-*` set, `agent-desktop`,
`mygo-automation`, `little-code-sauce`, `reverse-engineering` and
`security-engagement`. They are not built in. A skill added here later must
bring a license that permits redistribution, next to its `SKILL.md`; the seed
copies it into the user's skill directory like any other sibling file.
