# OpenCode Go as a provider

OpenCode Go is OpenCode's own subscription — $10/month for a curated set of open
coding models, served from the same gateway the official CLI talks to. Zeroth
ships it as one provider, reached the way OpenCode's own `/connect` reaches it:
**a pasted API key**, not an OAuth sign-in.

| provider id | how you authenticate | what is stored | what `/model` lists |
| --- | --- | --- | --- |
| `opencode_go` | the API key from your [Go page](https://opencode.ai/console/go) | the key, verbatim | Go's own `/models`: DeepSeek, Kimi, GLM, Qwen, MiniMax and the rest of the plan |

The key is read from `OPENCODE_API_KEY` — the same variable the official CLI
uses — so an exported key works without `/login`, and one saved in the app lands
in `~/.zeroth/.env` under that name. The free OpenCode endpoint
(`/inference/openai/v1`) is a different backend and is not part of this provider.

## The two headers Go requires

Go works like any other OpenAI-compatible endpoint only after the request
identifies the client. Its [documentation](https://opencode.ai/v2/docs/console/go/#where-can-i-use-it)
asks every harness to:

1. send typical coding-agent traffic;
2. name itself in `User-Agent` (`my-coding-agent/1.0`), not with a generic SDK
   or HTTP-library string;
3. send a stable per-conversation id in `x-opencode-session`, "so we can
   optimize routing and prompt caching".

Both are load-bearing, and the failure mode of getting them wrong is not a
slow reply but no reply at all — verified live:

- a generic `User-Agent` is answered by the edge with **`403 error code: 1010`**;
- a request with a named `User-Agent` but no `x-opencode-session` is answered
  with **`400 MissingSessionID`**: *"Request is missing x-opencode-session and
  cannot be routed efficiently."*

Zeroth adds both in `src/llm/opencode.rs`, for any base under
`/inference/go/`, and sends them on the chat request **and** the `/models`
listing:

- `User-Agent: zeroth/<version>` — the crate's own version, so the name is
  truthful across releases. A `User-Agent` the provider block spells itself
  wins, so a custom client identity is the user's to choose.
- `x-opencode-session: <per-session cache key>` — the same key the request
  carries as `prompt_cache_key` (`tui::config::session_cache_key`), stable for
  the life of the process and therefore for every round of the conversation.
  No key means no header, never an empty one.

**This is the speed story.** A local router in front of Go (Magpie and the
like) is an extra hop, and the ones that rewrite or mask prompts do real work
per request; the direct route is the one OpenCode's own client takes. The
session header is also what lets Go place the conversation where its prompt
cache is — the same reason `prompt_cache_key` is sent on every other provider
(`docs/providers/prompt-caching.md`).

## What a provider file says

```toml
[providers.opencode_go]
name = "OpenCode Go"
api_key_env = "OPENCODE_API_KEY"                              # ← the official CLI's variable
api_key_url = "https://opencode.ai/console/go"                # ← where `/login` says keys are made
api_model_base = "https://opencode.ai/inference/go/v1"        # ← the richer catalog
reasoning_param = "reasoning_effort"                          # ← Go reads OpenAI's top-level string
description = "OpenCode's $10/month subscription …"

[providers.opencode_go.kwargs]
api_base = "https://opencode.ai/inference/go/openai/v1"       # ← Chat Completions
```

The chat base is the one OpenCode's own catalog names for Go's
`chat-completions` API. The **listing** is a different path: Go publishes each
model's window, modalities and effort ladder from
`https://opencode.ai/inference/go/v1/models`, so `api_model_base` names it
rather than falling back to the chat base. The headers above are a property of
the path, not of the block, so a user who moves the base to a proxy under the
same path still gets them.

`reasoning_param = "reasoning_effort"` is what the official client sends: the
OpenAI-compatible SDK writes the model's effort as the top-level string, not as
OpenRouter's `{"reasoning": …}` object. Ctrl+T therefore cycles the ladder the
listing reports (`low → high → max` for DeepSeek V4.1 Flash) and Off omits the
field.

## What each model can do

The listing is the models.dev record shape, and `llm::models` reads it as such:

| record field | what it gives |
| --- | --- |
| `limit.context` | the window the footer gauges against, and auto-compaction's 90% |
| `modalities.input` | image input, spelled as MIME types (`image/jpeg`); `text` alone means no picture |
| `capabilities.reasoning.supported` + `effort` | the Ctrl+T ladder, exactly the levels the model takes |

A record that says nothing gets the old behaviour (no gauge, no forced
degradation, no ladder) rather than a guess. The free-vs-paid split is Go's to
change: switching plans or letting a model retire changes the listing, and the
picker follows it without a shipped copy to go stale.

## Limits

Go's paid models share one allowance, measured over a rolling 5-hour, a weekly
and a monthly window. At a limit the gateway answers `429` with a
`GoUsageLimitError`; Zeroth shows the server's words and does not retry it
(retrying cannot refill the window). The advice is Go's own: switch to a free
model, or turn on Extra Usage on the Go page. `/usage` reads the same meter
Zeroth sends the key against — `GET
https://opencode.ai/inference/go/v1/usage`, one bar per window and a `limit
reached` row when the status says so (`docs/providers/usage.md`). What each
window has used is also on [the Go page](https://opencode.ai/console/go#limits);
a refused read names whether the key itself was refused or its workspace has
no active subscription.
