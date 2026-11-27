#!/usr/bin/env bash
# Regenerate the vendored brand logos (`assets/brands/*.png`) from the
# `@yldm-tech/ai-logo` static SVG set (MIT; a fork of lobe-icons —
# `NOTICE`).
#
# Run from the repository root:
#
#     scripts/brand-logos.sh            # render every mark
#     scripts/brand-logos.sh --check    # only report what is missing
#
# The script is the record of how the PNGs were made, and running it again
# must produce the same files: the marks are a build input, checked in, not a
# download at runtime (`docs/brand-icons.md`). It needs `npm` and
# `rsvg-convert` (librsvg); both are dev-machine tools, never shipped.
#
# Each mark is a 64×64 tile: the brand's own colour as the ground, the white
# mark centred in it — the shape every app icon in a terminal or a launcher
# wears, and the one that stays legible at four text rows. `tone` comes from
# `src/brand.rs`, so a new brand is an entry here and a table row there.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

OUT=assets/brands
VERSION=1.22.0
PACKAGE="@yldm-tech/ai-logo-static-svg@${VERSION}"
CHECK_ONLY=0
[ "${1:-}" = "--check" ] && CHECK_ONLY=1

# brand.rs logo name → the ai-logo icon file, and the tile's ground.
#
# The ground is the brand's primary colour as ai-logo records it
# (`es/<Brand>/style.mjs`, COLOR_PRIMARY), darkened where the brand itself is
# white so a white mark still reads. `claude` keeps its terracotta, `openai`
# its black, `ollama`/`lmstudio` the monochrome grey they actually are.
MARKS=(
  "claude.png      claude      #D97757"
  "openai.png      openai      #000000"
  "gemini.png      gemini      #1A73E8"
  "deepseek.png    deepseek    #4D6BFE"
  "qwen.png        qwen        #615CED"
  "kimi.png        kimi        #000000"
  "glm.png         zhipu       #1F64F0"
  "grok.png        grok        #000000"
  "mistral.png     mistral     #FF7000"
  "meta.png        meta        #0866FF"
  "ollama.png      ollama      #3D3D3D"
  "copilot.png     copilot     #000000"
  "openrouter.png  openrouter  #000000"
  "venice.png      venice      #2B2B2B"
  "cline.png       cline       #1F2937"
  "antigravity.png antigravity #4285F4"
  "lmstudio.png    lmstudio    #3D3D3D"
  "github.png      github      #24292F"
  # llama.cpp has no mark in the set: the app falls back to its glyph, so no
  # PNG is claimed for it (`Brand::logo` says `None`).
)

missing=0
for row in "${MARKS[@]}"; do
  read -r out icon ground <<<"$row"
  if [ -f "$OUT/$out" ]; then
    continue
  fi
  missing=$((missing + 1))
  [ "$CHECK_ONLY" -eq 1 ] && { echo "missing: $OUT/$out"; continue; }
  echo "rendering $OUT/$out <- $icon.svg on $ground"
done

if [ "$CHECK_ONLY" -eq 1 ]; then
  [ "$missing" -eq 0 ] || { echo "$missing brand logos missing"; exit 1; }
  echo "all brand logos present"
  exit 0
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/zeroth-brands.XXXXXX")
trap 'rm -rf "$work"' EXIT

# The static set, once per run, with npm's cache outside the tree.
( cd "$work" && npm pack --silent "$PACKAGE" >/dev/null && tar -xzf ./*.tgz )

mkdir -p "$OUT"
for row in "${MARKS[@]}"; do
  read -r out icon ground <<<"$row"
  src="$work/package/icons/$icon.svg"
  [ -f "$src" ] || { echo "no $icon.svg in $PACKAGE" >&2; exit 1; }
  # Only the drawing: the source's own `<svg>` wrapper is dropped (a nested
  # `<svg>` opens a second viewport, which is what cropped the mark the first
  # time this ran) and its children are placed by the tile's transform. Every
  # mark in the set is a 24×24 viewBox.
  sed -E -e 's$<svg[^>]*>$$' -e 's$</svg>$$' -e 's/style="[^"]*"//g' \
    "$src" >"$work/mark.svg"
  # `color`/`fill` on the group are the mark's defaults: an unfilled path
  # inherits from `fill`, and a `currentColor` one from `color` — white, so a
  # mark on a black tile still reads. A brand that hard-codes its own colour
  # (OpenRouter's lime) keeps it: a path's own attribute wins over the group's.
  cat >"$work/tile.svg" <<SVG
<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">
  <rect x="0" y="0" width="64" height="64" rx="14" fill="$ground"/>
  <g transform="translate(12.8 12.8) scale(1.6)" color="#FFFFFF" fill="#FFFFFF">$(cat "$work/mark.svg")</g>
</svg>
SVG
  rsvg-convert --width 64 --height 64 --format png --output "$OUT/$out" "$work/tile.svg"
done

echo "wrote ${#MARKS[@]} marks into $OUT/"
