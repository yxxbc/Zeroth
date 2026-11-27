# Brand marks — a model, a provider and a link, at a glance

A model id is the honest name of what answers, but it is a wire detail.
`[openrouter]claude-sonnet-4` spends fourteen columns saying something one
single-width mark says better, and `https://github.com/yxxbc/zeroth` on its own
line says "a link" eight times over. This page is the one table both read from:
which brand a value names, the mark it wears in a one-row surface, and the tile
it gets where there are rows to spare.

```
  ◈ deepseek-v4.1-flash medium · ~/Projects/zeroth                 auto
  ⎇ github.com/yxxbc/zeroth
```

## What it is

- **Pure.** `src/brand.rs` maps a **provider id**, a **model id** or a **link
  host** to one [`Brand`](../src/brand.rs). Nothing here touches the network or
  the filesystem, so the same value is the same mark in a capture and on a
  screen, and every table is unit-tested (`cargo test --lib -- brand::`).
- **A closed set.** An unknown provider, model family or host is `None` — never
  a guessed look-alike. The caller keeps its own fallback: the footer's
  `[provider]` bracket, the model row's `[provider]` tag, the bare URL.
- **Single-width glyphs.** Every mark is one display column
  (`every_glyph_is_one_column_and_carries_no_space`). A wide glyph would shift
  every column after it, and the footer truncates by display width.
- **Theme colours, not brand colours.** A mark wears a [`Tone`](../src/brand.rs)
  — accent, link, success, warning, purple, text, dim — which
  `ui::theme::brand_color` reads off the active palette. A new brand therefore
  never means a new colour in every theme table (`docs/ui/theme.md`), and
  `/theme` recolours the marks with everything else.

## The three tables

### Model families

Detection is lexical: a model id is split on `/` and each segment is read for
the **longest known prefix** — `claude`, `gpt`, `gpt-oss`, `gemini`, `gemma`,
`deepseek`, `qwen`, `kimi`, `glm`, `grok`, `mistral`, `llama`, the `o1`/`o3`/`o4`
lines, … — with a trailing `:free`-style tag dropped first. So
`anthropic/claude-sonnet-4` reads Claude from the vendor segment,
`openrouter/deepseek/deepseek-v4` reads DeepSeek from the model's own, and
`gpt-5.5` reads OpenAI. The family is checked **before** the provider: an
aggregator's bracket in front of a model whose own family the reader recognises
is the case this exists for.

### Providers

The `providers.toml` key, lowercased, plus the aliases the same backends answer
to — `anthropic_console` → Claude, `chatgpt_codex` → OpenAI, `a0_venice` →
Venice, `cline_pass` → Cline, `ollama_cloud` → Ollama, `lm_studio`/`lm-studio` →
LM Studio, … The provider is the **fallback**: it decides the mark only when the
model name says nothing (a local checkpoint) or when the surface is naming the
*provider* rather than the model.

### Link hosts

A link's host, matched on its registrable suffix so `docs.anthropic.com` and
`anthropic.com` agree and `anthropic.com.evil.example` does not.

## Where each shape shows up

| surface | shape | notes |
| --- | --- | --- |
| footer's model segment | glyph + suffix | `◈ deepseek-v4.1-flash`; `[provider]` only for an unknown pair |
| `/model` picker rows | `[provider]` tag (no mark) | the list is where you **choose a route**, so the provider must stay readable — one model name can come from several providers |
| sidebar `model` fact | glyph + name | inside an agent view, the pinned model's own family |
| sidebar brand section | **real logo tile** | graphic terminals only (`docs/ui/images.md`) |
| assistant's lone link line | glyph + clickable host/path | replaces the fetched preview card (`docs/ui/rich-links.md`) |
| `web_fetch` header | glyph + URL | a known host only |

The one-row surfaces **never** reserve picture rows: a mark is a character, and
the footer's truncation counts it as one.

## The tiles

`assets/brands/*.png` are 64×64 RGBA tiles — the brand's own colour as a rounded
ground, its mark in white, kept at `Brand::LOGO_PX`. They are **generated and
checked in**, never downloaded at runtime:

```bash
scripts/brand-logos.sh          # render every mark from the SVG set
scripts/brand-logos.sh --check  # report missing files only
```

The script records the whole provenance: which icon file, which ground colour,
which transform. Its source is `@yldm-tech/ai-logo-static-svg` (MIT, a fork of
[lobe-icons](https://github.com/lobehub/lobe-icons)) — see `NOTICE`. A brand
with no usable mark in that set (`llama.cpp`, and the link-only brands) claims
no tile; the roomy surfaces then draw the glyph, never a wrong logo.

The boundary materialises the tiles into `~/.zeroth/cache/brands/` beside the
mascot icon (`tui::settings::materialize_brand_images`) and publishes the
directory through `images::set_brand_image_dir`, so an installed binary can draw
one. They are drawn by the **existing** image pipeline (`images::place_in`,
`images::carrier`), which means they follow the same rules as every other
picture: `/settings` **Image width**, **Show images**, the terminal's protocol,
and the scrolling-frame suppression that keeps a full-screen scroll from leaving
ghost blocks (`docs/ui/images.md`, `docs/ui/full-screen.md`).

The sidebar tile is the only graphic surface today, and only in the
graphic-protocol branch (`images::graphics_available`).

The `/model` picker carries **no** mark, deliberately. It is where you choose a
route, and the same model name can come from several providers
(`cline-pass/glm-5.3` from a subscription beside `glm-5.3` from an API key), so
replacing its `[provider]` tag with an icon trades the one fact the row exists
for against a decoration. The mark belongs where the question is "what am I
talking to", which is the footer and the sidebar.

## The alternative that lost

Drawing the real logo in the **footer** was the first sketch and is still
tempting. It loses on three counts the footer cannot answer: a graphic protocol
is per-terminal, so the row would change height on some terminals and not
others; a picture is anchored to a screen cell, and the inline viewport commits
and scrolls rows under it; and the row is one line tall, which is smaller than
any legible mark. The glyph is the mark every terminal can draw, and the tile is
the mark where the terminal has rows to paint.
