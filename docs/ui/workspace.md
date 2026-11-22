# Full-screen workspace

The bottom session bar is available with `--fullscreen` or `ZEROTH_TUI=full`. The default inline mode keeps its existing footer and roster navigation.

The bar shows main first, then subagents in roster order, including settled agents until the existing roster removes them. Background shells are not tabs. A filled bullet marks the displayed conversation; a separate highlight marks keyboard focus. On a narrow screen the bar shows a window around the focused or displayed tab.

| Input | What it does |
| --- | --- |
| `↓` | Focuses the displayed tab from an empty composer. An open menu, shortcut band, modal, shell composer or active history recall keeps its existing key behavior. |
| `←` / `→` | Moves tab focus without changing conversations. |
| `Enter` | Shows the focused conversation and keeps focus on the bar. |
| `↑` / `Esc` / `Ctrl+C` | Returns focus to the composer without changing conversations. |
| Click a tab | Switches directly to that conversation. |

Switching tabs preserves draft text, cursor position, and the conversation's scroll and expansion state. The full-screen shell panel and shell input takeover are not implemented.
