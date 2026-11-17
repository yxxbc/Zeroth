# Search summaries and tool details

Status: implementation design, approved direction A. Existing tool expansion,
Ctrl+O, keyboard navigation and full-screen click-to-expand remain unchanged.

## Goals

A search summary says what was searched and what the returned output proves.
Expanded details separate the query, grouped results and execution command.
The presentation never rewrites tool arguments, output, context or rollouts.

## Summaries

A group counts search calls as searches, not patterns. It keeps a bounded
preview of the latest searches: query and scope, plus result statistics when
the command and complete output have a recognised format. File enumeration
is labelled as files, not content matches. Unknown formats keep the command
and do not invent a result count. Failure and zero results remain distinct.

## Expanded results

Code matches are grouped by file in first-appearance order. Each file heading
is shown once; source line numbers align beside their code. The original
command has its own section after the results. Recorded exit status and
elapsed time remain separate execution facts. There is no new detail page.

Web results show numbered titles, publication dates when supplied, full links
and excerpts in distinct rows. Repeated model-directed boundary instructions
are not rendered as search excerpts; the stored model-facing output is kept.
Unknown or malformed web output stays in the ordinary complete-output view.

## Recognition and fallback

The shell recogniser accepts a single literal search command and known output
options. Pipelines, substitutions, ambiguous flags, unsupported output modes
and malformed result rows fall back to the original command/output layout.
No display recognition changes permission decisions. It is never a shell
execution parser or a claim that a command is safe.

Only recognised complete match output receives an exact match/file count.
Truncated output is labelled as retained results, not as an exhaustive total.
Counts describe matching output lines, not regex occurrence counts within a
line. Empty output at the documented no-match exit code says no matches;
execution errors keep their error text. Binary notices, context separators,
file headings or unusual records that cannot be understood are not dropped.

## Layout and performance

All width measurements use display columns. Long Unicode paths, URLs and
source lines wrap without changing their stored content. Narrow layouts keep
query and result identity ahead of secondary metadata. Group previews have
fixed row and entry budgets; expanded output remains complete. Existing theme
roles provide text, path/link, line-number and status contrast.

Parsing reads retained text directly, without a JSON tree for long outputs.
The transcript cache keeps its incremental invalidation and visible-window
rendering contracts. A malformed result does not create a partially formatted
view that hides the rest of the output.

## Verification

Tests cover call counts, query retention, file grouping, file enumeration,
web titles/URLs/dates/excerpts, unknown command fallback, empty and error
states, truncation and Unicode at narrow widths. They also assert that the
ToolCall remains unchanged. Real tmux smoke scenarios check the collapsed
view and Ctrl+O, including the search/read group in phase 146 and saved code/web results in phase 163. Unit and smoke
fixtures use local canned results and do not call a real search provider.
