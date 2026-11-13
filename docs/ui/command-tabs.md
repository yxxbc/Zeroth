# The command page — tabs, and keeping their state apart

**Status: Commands + Skills shipped.** `CommandMenu` carries an `active` tab
and one highlight per tab (`src/app/commands.rs`); the bar and rows are drawn
by `ui::menu::command_menu_lines` (`src/ui/menu.rs`). The Agents tab is the
next phase; `/mcp` keeps its own manager page.

## Shipped look: underline tabs

The bar is one plain row of labels — no box, no background pills — so it
belongs to the same flat band as the rows under it:

```
Commands  Skills
‾‾‾‾‾‾‾‾
→ /clear                   Clear the conversation
  /copy                    Copy the last response to the clipboard
  /export                  Copy or save the conversation as plain text
  (1/48)
```

The active label is accent-coloured, bold and underlined; the inactive one is
dim. A terminal too narrow for the whole bar falls back to the active label
alone. **The bar is only drawn when a second tab exists** — a session with no
skills keeps the exact flat palette it had before tabs.

Tab and Shift+Tab cycle the tabs while the page is open; Enter runs the active
tab's highlighted row. Tab no longer runs a command (Enter always did), and
the plain-Tab follow-up/accept arms return as soon as the page closes.

## Before tabs

Typing a bare `/` opens the palette above the composer. Every entry comes from
one registry — `COMMANDS` plus the active model's speed tiers spliced after
`/model` (`App::commands`, `src/app/commands.rs`) — filtered by a
case-insensitive prefix of the token after `/`, drawn by
`ui::menu::command_menu_lines` in the band above the box. `CommandMenu` holds
one number, `selected`, into the **filtered** list; Up/Down move it, Enter (or
Tab with the menu open) runs it, Esc closes, and a size guard scrolls the
window with a `(n/total)` counter.

Three lists that are not slash commands already live beside it:

- **skills** — `$name` mentions and the `Skill` tool (`src/skills.rs`,
  `SkillRegistry`), browsed by the `$` picker and `/skills`;
- **agent types** — the `Agent` tool's `subagent_type` names
  (`src/subagents.rs`), shown by the `agent` tool's description and `/skills`'
  sibling menus;
- **MCP servers/tools** — `/mcp`, with its own manager page.

The palette knows none of them. A user who wants “the thing called `review`”
has to know whether it is a slash command, a skill, an agent, or an MCP tool
before they can type the right sigil.

## The ask

One command page with **tabs**: `Commands`, `Skills`, `Agents` (and later
`MCP`), each listing its own source, with the states **isolated** — a filter
or a highlighted row in one tab never leaks into another, and Enter can only
ever run an item from the active tab. The existing sigils keep their
shortcuts: `/` opens on Commands, `$` on Skills, `@` stays the file picker.

## Design space

### A. Tabs inside one palette (shipped)

```
Commands  Skills
‾‾‾‾‾‾‾‾
→ /clear    Clear the conversation
  /compact  Summarize the conversation
  /copy     Copy the last response
  (3/48)
```

- `CommandMenu` carries `{ active: CommandTab, selections: [usize; 2] }`.
  The composer text is still the one query string; each tab filters **its own
  source** with that query, and each keeps its own highlight. Switching tabs
  restores that tab's selection, clamped to its current matches.
- **Tab / Shift+Tab cycle tabs while the palette is open.** The plain Tab arms
  (accept a highlighted `@` file / `$` skill, queue a follow-up turn) return
  the moment the palette closes; the menu's own Tab arm wins only when
  `command_menu.is_some()`, so no existing binding changes.
- **Enter runs the active tab's item.** Commands dispatch their
  `CommandEffect`; a skill row inserts `$name ` into the composer (the model
  decides to load it, the `$` picker's own contract). An agent row is phase 2
  (read-only; launching is the model's `agent` tool, not a command).
- **Tab bar** costs one band row when shown; `menu_rows` is still the built
  line count, so the layout needs no special case. A tab with no matches shows
  its own placeholder row (`No matching skills`), not an empty band.
- **Isolation** is structural: each tab owns its selected index; the filtered
  list is re-derived per source from the shared query, never carried as “the
  first match from the other tab”.

### B. Separate pages (`/commands`, `/skills`, `/agents`)

Three full-screen pages, no tabs. Cheapest to make correct (each page already
has a list model), but the user must still know the category and run a
different command to see it; it does not solve discovery, only isolation. It
also duplicates `/skills` and `/mcp`, which already exist.

### C. Tabs only when the category is non-empty

A hides tabs that have nothing to show (no skills installed, no agent types).
Cleaner on a fresh install and on a narrow terminal; the tab bar appears on
the first install that has something in a second category. The state model is
A's; only the bar's visibility differs.

**Recommendation: C on top of A.** One page, tabs for the categories that
actually have entries, per-tab isolation. B stays a fallback if the palette
band gets too tall.

## Implementation map

| piece | file | change |
| --- | --- | --- |
| catalogue | `src/app/commands.rs` | `CommandTab`, `PaletteItem`, `CommandMenu { active, selections }`; `palette_matches` per tab; `run_selected_command` dispatches by `active` |
| sources | `src/skills.rs`, `src/subagents.rs` | `App::skills` (the `$` picker's snapshot) now also feeds the Skills tab; agent types await phase 2 |
| keys | `src/app/keys.rs` | Tab/Shift+Tab cycle only while `command_menu.is_some()`; Enter accepts; Up/Down keep moving within the active tab |
| render | `src/ui/menu.rs` | the underline bar (only with 2+ tabs), active-tab rows, per-tab placeholder, counter |
| layout | `src/ui/menu.rs` | `menu_rows` is the built line count, so the tab row needs no separate layout change |
| tests | `src/app/tests/keys.rs`, `src/ui/tests/menu.rs` | per-tab selection/filter isolation; Tab cycles; Enter cannot reach an inactive tab; one tab hides the bar; a narrow bar folds to the active label |

## Isolation rules (the contract)

1. `selected` is per tab, clamped to that tab's current match count on every
   filter change and on switching back.
2. The query is global (one textarea) but never produces a cross-tab result:
   a row is only ever drawn and run from its own tab.
3. Switching tabs does not run, dispatch, or mutate any item; it only changes
   which list is painted.
4. Closing the palette discards nothing in history; reopening starts on
   Commands (`CommandMenu::new`), the tab `/` opens.
5. A command that is unavailable mid-turn (`/clear`, `/resume`, `/compact` …)
   keeps its existing toast/rejection path — the tab changes nothing about
   availability.

## Phasing

1. **Commands + Skills tabs** — shipped: per-tab selection, Tab/Shift+Tab,
   skills rows insert `$name `, the bar hides when only Commands exists, and
   a narrow bar folds to the active label.
2. **Agents tab** (read-only), and the `/skills` snapshot's agent-type
   listing if it earns a slot.
3. **MCP** if it earns a slot (its manager page already exists); otherwise
   leave it to `/mcp`.
4. **Per-tab query memory** only if users ask: switching tabs would then save/
   restore the token — a textarea sync we deliberately avoid in v1.

## Resolved questions

- **Enter on a skill inserts `$name `**; it does not call the `Skill` tool.
  The palette is pure `App`; a direct load is an `Action` and would bypass the
  model's “load once” bookkeeping. Insert is the smaller contract and matches
  `$` mentions.
- **Tab cycles; Enter accepts.** Tab is free only while the palette is open.
- **A narrow bar shows only the active label** — no wrapping, no sigil
  fallback; the row never grows.

## Non-goals

- Running agents or MCP tools from the palette (they are model tools, not
  user commands).
- A second registry: tabs are views over the existing sources, not a new
  catalogue.
- Changing `$`/`@` behaviour; they stay the direct sigils, and the palette
  tabs are the discovery path.
