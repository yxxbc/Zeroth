#!/bin/sh
# install.sh — the one-line installer for Zeroth (docs/ops/release.md).
#
#   curl -fsSL https://raw.githubusercontent.com/yxxbc/zeroth/main/install.sh | sh
#
# Picks the release archive built for this machine (Linux x86_64 or arm64,
# macOS Intel or Apple silicon), verifies its SHA-256 against the checksum
# published beside it, and installs the binary to ~/.local/bin. Nothing is
# installed unless the checksum matches and the binary runs.
#
#   ZEROTH_VERSION=v0.1.0 … | sh               # pin a release (default: the latest)
#   ZEROTH_INSTALL_DIR=/usr/local/bin … | sh   # install somewhere else
#   … | sh -s -- --version v0.1.0 --dir ~/bin      # the same two, as flags
#   … | sh -s -- --preview                         # the banner alone, 10 s, installing nothing
#   … | sh -s -- --preview 20                      # the same, for 20 s (--preview=20 too)
#   … | sh -s -- --help
#
# POSIX sh only — no arrays, no `local`, no `[[` — so `sh` on macOS (bash 3.2
# in POSIX mode), dash and busybox all run it. Everything is a function and
# the last line calls `main`, so a download cut short executes nothing.
# ZEROTH_INSTALL_BASE_URL points it at a stand-in for github.com; the
# release tooling's selftest uses that to install from a local server.
set -eu

REPO="${ZEROTH_REPO:-yxxbc/zeroth}"
# The installed binary is named after the repository's slug — Cargo.toml's
# `name`, which is what a release archive is named after too. One variable,
# so this repository (`yxxbc/zeroth` → `zeroth`) and a stand-in (the release
# tooling's fixture is a repository called `example/zeroth`) both resolve.
BIN="${REPO##*/}"
BASE_URL="${ZEROTH_INSTALL_BASE_URL:-https://github.com/$REPO}"
GLIBC_FLOOR_MINOR=35

# ---------------------------------------------------------------------------
# Looks: colour on a terminal (truecolor for the mascot's gradient when the
# terminal says it has it), the app's own glyphs in a UTF-8 locale, plain
# ASCII otherwise, and no colour at all under NO_COLOR or in a pipe.
# ---------------------------------------------------------------------------
setup_looks() {
	esc=$(printf '\033')
	bold='' dim='' reset='' red='' green='' yellow='' cyan='' g1='' g2='' g3=''
	cursor_hidden=0
	if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-}" != dumb ]; then
		bold="${esc}[1m" dim="${esc}[2m" reset="${esc}[0m"
		red="${esc}[31m" green="${esc}[32m" yellow="${esc}[33m" cyan="${esc}[36m"
		case "${COLORTERM:-}" in
		truecolor | 24bit)
			g1="${esc}[38;2;137;220;235m" # #89DCEB, the banner gradient's near end
			g2="${esc}[38;2;137;200;242m"
			g3="${esc}[38;2;137;180;250m" # #89B4FA, its far end
			;;
		*) g1="$cyan" g2="$cyan" g3="${esc}[34m" ;;
		esac
	fi
	case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
	*UTF-8* | *utf-8* | *UTF8* | *utf8*) utf8=1 ;;
	*) utf8=0 ;;
	esac
	if [ "$utf8" -eq 1 ]; then
		arrow='→' tick='✔' cross='✘' bang='!'
		bar_fill='━' bar_empty='╌'
	else
		arrow='>' tick='+' cross='x' bang='!'
		bar_fill='=' bar_empty='.'
	fi
	# A frame's pause. `sleep` is not required to take a fraction: where it
	# does not, a frame lasts a second rather than a tenth, and a preview of
	# ten seconds steps ten times.
	if sleep 0.1 2>/dev/null; then
		fps=10
	else
		fps=1
	fi
}

# ---------------------------------------------------------------------------
# The mascot: `crest`, the app's default (src/app/mascot.rs), beside the name.
# Three rows of single-width block glyphs, tinted from the banner's gradient
# with the tint walking down the rows, its crest swinging and its eyes
# blinking on a short cycle. On a terminal it animates — a short intro before
# the steps, a download progress bar below the log, and `--preview N`
# for as long as you ask. In a pipe or under `TERM=dumb`, the art is drawn once
# and left still; `NO_COLOR` suppresses tint without disabling terminal motion.
# ---------------------------------------------------------------------------
# Every row is written with its trailing padding already in the string: the
# captions' column is fixed, and a UTF-8 block glyph is three bytes wide but
# one column, so no byte count could pad it.
art_set() { # FRAME -> r1 r2 r3
	frame=$(($1 % 6))
	blink=0
	case "$(($1 % 20))" in
	15 | 16) blink=1 ;;
	esac
	if [ "$utf8" -eq 1 ]; then
		case "$frame" in
		3 | 4 | 5) r1='▙▄▙▄▟▄▟    ' ;;
		*) r1=' ▙▄▙▄▟▄▟   ' ;;
		esac
		if [ "$blink" -eq 1 ]; then r2='▝▜▄▀▀▀▄▛▘  '; else r2='▝▜▄███▄▛▘  '; fi
		case "$frame" in
		3 | 4 | 5) r3='  ▝▝ ▘▘    ' ;;
		*) r3='  ▘▘ ▝▝    ' ;;
		esac
	else
		case "$frame" in
		3 | 4 | 5) r1='.--.       ' ;;
		*) r1=' .--.      ' ;;
		esac
		if [ "$blink" -eq 1 ]; then r2='( -- )     '; else r2='( oo )     '; fi
		# The ASCII stand-in's feet are backticks, so they are escaped
		# rather than quoted into a substitution.
		r3=" \`--\`      "
	fi
}

art_tint() { # FRAME ROW -> col
	case "$((($1 + $2) % 3))" in
	0) col="$g1" ;;
	1) col="$g2" ;;
	*) col="$g3" ;;
	esac
}

draw_mascot() { # FRAME CAP1 CAP2 CAP3
	art_set "$1"
	art_tint "$1" 0
	printf '  %s%s%s  %s%s%s\n' "$col" "$r1" "$reset" "$bold" "$2" "$reset"
	art_tint "$1" 1
	printf '  %s%s%s  %s%s%s\n' "$col" "$r2" "$reset" "$dim" "$3" "$reset"
	art_tint "$1" 2
	printf '  %s%s%s  %s%s%s\n' "$col" "$r3" "$reset" "$dim" "$4" "$reset"
}

# Whether the art is allowed to move: a terminal, and not one that cannot
# place a cursor (`TERM=dumb`, so a captured run).
animate_ok() { [ -t 1 ] && [ "${TERM:-}" != dumb ]; }

hide_cursor() {
	if [ "$cursor_hidden" -eq 0 ] && animate_ok; then
		printf '\033[?25l'
		cursor_hidden=1
	fi
}

show_cursor() {
	if [ "${cursor_hidden:-0}" -eq 1 ]; then
		printf '\033[?25h'
		cursor_hidden=0
	fi
}

tick() { sleep "0.1" 2>/dev/null || sleep 1; }

# animate FRAMES CAP1 CAP2 CAP3 — the three rows redrawn in place.
animate() {
	frames="$1"
	hide_cursor
	i=0
	while [ "$i" -lt "$frames" ]; do
		if [ "$i" -gt 0 ]; then printf '\033[3A'; fi
		draw_mascot "$i" "$2" "$3" "$4"
		tick
		i=$((i + 1))
	done
	show_cursor
}

# script_version — the version to name in the banner, or empty where there is
# none to name. Two sources, both local: a pinned `ZEROTH_VERSION`/`--version`,
# and the `Cargo.toml` beside a checkout's copy of this script. The piped
# installer has neither (the tag is a network lookup, and the banner is drawn
# before one is made), so it names the release in its Release step instead.
script_version() {
	if [ -n "${want:-}" ]; then
		case "$want" in v*) printf '%s\n' "${want#v}" ;; *) printf '%s\n' "$want" ;; esac
		return
	fi
	case "$0" in
	*/*) here=$(dirname "$0") ;;
	*) here='' ;;
	esac
	manifest="${here:+$here/}Cargo.toml"
	[ -f "$manifest" ] || return 0
	# Only `[package]`'s own `version`, never a dependency's.
	awk '
		/^\[package\]/ { in_package = 1; next }
		/^\[/ { in_package = 0 }
		in_package && /^version[ \t]*=/ {
			value = $0
			sub(/^[^=]*=[ \t]*/, "", value)
			gsub(/["\047 \t]/, "", value)
			print value
			exit
		}
	' "$manifest"
}

# The same art, once, for a pipe or a terminal that cannot hold it still.
static_banner() {
	draw_mascot 0 "$(banner_name)" 'Open-source coding agent' 'Secure, verified installer'
}

# banner_name — `Zeroth`, or `Zeroth (v0.4.2)` when a version is known. The
# shape is the app's own header (`docs/ui/header.md`), so the installer and
# the session it installs introduce themselves the same way.
banner_name() {
	# Memoized: the animated banner asks once per frame, and resolving the
	# version costs a subprocess.
	if [ -n "${banner_name_done:-}" ]; then
		printf '%s\n' "$banner_name_text"
		return
	fi
	version=$(script_version)
	if [ -n "$version" ]; then
		banner_name_text="Zeroth (v$version)"
	else
		banner_name_text='Zeroth'
	fi
	banner_name_done=1
	printf '%s\n' "$banner_name_text"
}

# Graphical terminals that implement the Kitty graphics protocol (including
# iTerm2 and Ghostty). The installer is commonly piped into `sh`, so assets
# are fetched from the repository only in this mode. A checkout can use its
# local artwork, and every failure falls back to the text banner.
graphics_terminal() {
	[ -t 1 ] || return 1
	case "${TERM:-}:${TERM_PROGRAM:-}" in
	*kitty*:* | *:iTerm.app | *:WezTerm | *:Ghostty | ghostty:*) return 0 ;;
	esac
	[ -n "${KITTY_WINDOW_ID:-}${GHOSTTY_RESOURCES_DIR:-}${ITERM_SESSION_ID:-}" ]
}

prepare_art() {
	graphics_terminal || return 1
	local_assets=${ZEROTH_INSTALL_ASSET_DIR:-assets}
	if [ -f "$local_assets/banner-v2.png" ] && [ -f "$local_assets/mascot-crest.png" ]; then
		banner_image="$local_assets/banner-v2.png"
		icon_image="$local_assets/mascot-crest.png"
		return 0
	fi
	art_base=${ZEROTH_INSTALL_ART_BASE_URL:-https://raw.githubusercontent.com/$REPO/main/assets}
	banner_image="$tmp/banner-v2.png"
	icon_image="$tmp/mascot-crest.png"
	# No fetcher, no download: the caller falls back to the text banner.
	[ -n "${fetcher:-}" ] || return 1
	if [ "$fetcher" = curl ]; then
		art_fetch() { curl --connect-timeout 2 --max-time 8 -fsSL -o "$2" "$1"; }
	else
		art_fetch() { wget -q --timeout=8 -O "$2" "$1"; }
	fi
	if art_fetch "$art_base/banner-v2.png" "$banner_image" && art_fetch "$art_base/mascot-crest.png" "$icon_image"; then
		return 0
	fi
	return 1
}

# Kitty's remote protocol takes PNG and caps payload chunks at 4096 bytes.
# The image is given a column count and no row count: the protocol then scales
# it from the picture's own aspect ratio, so nothing is stretched, and the
# default cursor policy leaves the cursor on the row below the placement, so
# what is drawn next lands under the art rather than over it. A negative
# z-index keeps the text on top of the artwork even in a terminal that gets
# the cursor policy wrong — the failure is then a readable banner, not a
# caption hidden behind a picture.
#
# Two things here are not obvious. The chunk count cannot come from `wc -l`:
# BSD `fold` does not newline-terminate its last line, so the count came out
# one short, the real tail went out flagged `m=1` (more to come), and the
# terminal sat waiting for bytes that never arrived — EBADPNG, and half an
# image. Buffering one line is the fix, so the last chunk is known rather
# than counted. And `q=2` asks for no reply at all: the protocol answers a
# transmit with an OK or an error on the terminal's *input*, and a `sh` that
# has already exited leaves that answer for the shell to read as a command.
kitty_png() { # FILE ID COLUMNS
	encoded_chunks="$tmp/art-chunks"
	base64 <"$1" | tr -d '\r\n' | fold -w 4096 >"$encoded_chunks"
	[ -s "$encoded_chunks" ] || return 1
	awk -v esc="$esc" -v id="$2" -v cols="$3" '
		BEGIN { first = 1; bs = sprintf("%c", 92) }
		function send(chunk, last, meta) {
			if (first) {
				meta = "a=T,f=100,i=" id ",c=" cols ",z=-1,q=2"
				first = 0
			} else {
				meta = "i=" id
			}
			printf "%s_G%s,m=%d;%s%s", esc, meta, last, chunk, (esc bs)
		}
		{ if (have) send(prev, 1); prev = $0; have = 1 }
		END { if (have) send(prev, 0) }
	' "$encoded_chunks"
}

# term_cols — the screen's width, or 80 where nothing answers.
term_cols() {
	cols=''
	if command -v tput >/dev/null 2>&1; then
		cols=$(tput cols 2>/dev/null) || cols=''
	fi
	case "$cols" in '' | *[!0-9]*) cols=80 ;; esac
	[ "$cols" -ge 8 ] || cols=80
	printf '%s\n' "$cols"
}

image_banner() {
	# Kitty protocol is also implemented by iTerm2 and Ghostty. Each image
	# leaves the cursor on the row below itself, so the composition needs no
	# cursor arithmetic — the mascot sits under the banner and the name under
	# the mascot.
	cols=$(term_cols)
	banner_cols=$((cols - 2))
	[ "$banner_cols" -le 72 ] || banner_cols=72
	[ "$banner_cols" -ge 16 ] || banner_cols=16
	kitty_png "$banner_image" 31 "$banner_cols"
	printf '\r'
	kitty_png "$icon_image" 32 12
	printf '\r'
	printf '  %s%s%s\n  %sOpen-source coding agent%s\n  %sSecure, verified installer%s\n' \
		"$bold" "$(banner_name)" "$reset" "$dim" "$reset" "$dim" "$reset"
}

banner() {
	printf '\n'
	if [ "${art_ready:-0}" -eq 1 ]; then
		image_banner
	elif animate_ok; then
		animate $((fps * 3 / 2)) "$(banner_name)" 'Open-source coding agent' 'Secure, verified installer'
	else
		static_banner
	fi
	printf '\n'
}

# bar_of FILLED WIDTH -> the bar in $bar.
bar_of() {
	bar=''
	cell=0
	while [ "$cell" -lt "$2" ]; do
		if [ "$cell" -lt "$1" ]; then bar="$bar$bar_fill"; else bar="$bar$bar_empty"; fi
		cell=$((cell + 1))
	done
}

# preview_hold SECONDS — waits out the seconds the preview asked for, showing
# them pass. An image banner is printed, not animated, so without this a
# graphics terminal came straight back and `--preview 5` was over in a flash.
# The frames are counted, not timed, so a slow terminal runs long rather than
# dropping frames.
preview_hold() {
	frames=$(($1 * fps))
	[ "$frames" -gt 0 ] || frames=1
	hide_cursor
	i=0
	while [ "$i" -lt "$frames" ]; do
		bar_of $((i * 24 / frames)) 24
		printf '\r  %s%s%s  %-9s [%s%s%s] %s%d of %d s · nothing is installed%s\033[K' \
			"$cyan" "$arrow" "$reset" 'Preview' "$cyan" "$bar" "$reset" "$dim" "$((i / fps))" "$1" "$reset"
		tick
		i=$((i + 1))
	done
	bar_of 24 24
	printf '\r  %s%s%s  %-9s [%s%s%s] %s%d s · nothing is installed%s\033[K\n' \
		"$green" "$tick" "$reset" 'Preview' "$cyan" "$bar" "$reset" "$dim" "$1" "$reset"
	show_cursor
}

# preview SECONDS — the banner alone, drawn the way an install draws it, and
# nothing installed.
preview() {
	printf '\n'
	if [ "${art_ready:-0}" -eq 1 ]; then
		image_banner
		preview_hold "$1"
	elif animate_ok; then
		animate "$(($1 * fps))" "$(banner_name)" 'Open-source coding agent' 'Secure, verified installer'
		printf '  %s%s  %spreview · %s s · nothing is installed%s\n' \
			"$dim" "$bang" "$dim" "$1" "$reset"
	else
		static_banner
		printf '\n  %s--preview needs a terminal to animate on.%s\n' "$dim" "$reset"
	fi
	printf '\n'
}

# step LABEL DETAIL — a row in progress; done_ LABEL DETAIL — a row finished.
step() { printf '  %s%s%s  %-9s %s\n' "$cyan" "$arrow" "$reset" "$1" "$2"; }
done_() { printf '  %s%s%s  %-9s %s\n' "$green" "$tick" "$reset" "$1" "$2"; }
note() { printf '  %s%s%s  %s\n' "$yellow" "$bang" "$reset" "$1"; }
# fail WHAT [WHY] — the one exit that is not success; nothing has been
# installed when it fires, and it says so.
fail() {
	printf '\n  %s%s  %s%s\n' "$red" "$cross" "$1" "$reset" >&2
	[ -n "${2:-}" ] && printf '     %s%s%s\n' "$dim" "$2" "$reset" >&2
	printf '     %sNothing was installed.%s\n\n' "$dim" "$reset" >&2
	exit 1
}

usage() {
	if [ -f "$0" ]; then
		sed -n '2,/^set -eu/p' "$0" | sed '$d' | sed 's/^# \{0,1\}//'
	else
		printf 'usage: curl -fsSL https://raw.githubusercontent.com/%s/main/install.sh | sh -s -- [--version vX.Y.Z] [--dir DIR] [--preview [SECONDS]]\n' "$REPO"
	fi
}

# ---------------------------------------------------------------------------
# The machine: OS and CPU to a Rust target triple, plus the two things that
# make a Linux download useless — musl, and a glibc older than the build's.
# ---------------------------------------------------------------------------
detect_target() {
	os=$(uname -s 2>/dev/null || echo unknown)
	cpu=$(uname -m 2>/dev/null || echo unknown)
	case "$cpu" in
	x86_64 | amd64) cpu=x86_64 ;;
	aarch64 | arm64) cpu=aarch64 ;;
	*) fail "unsupported CPU: $cpu" "Zeroth ships for x86_64 and arm64." ;;
	esac
	case "$os" in
	Linux)
		target="$cpu-unknown-linux-gnu"
		platform="Linux $cpu"
		if command -v ldd >/dev/null 2>&1; then
			ldd_out=$(ldd --version 2>&1 || true)
			case "$ldd_out" in
			*musl*) fail "this Linux uses musl, and the release build needs glibc" "Build from source instead: https://github.com/$REPO#build-from-source" ;;
			esac
			glibc=$(printf '%s\n' "$ldd_out" | sed -n '1s/.*[^0-9.]\([0-9][0-9]*\.[0-9][0-9]*\).*/\1/p')
			if [ -n "$glibc" ]; then
				major=${glibc%%.*}
				minor=${glibc#*.}
				minor=${minor%%.*}
				if [ "$major" -eq 2 ] && [ "$minor" -lt "$GLIBC_FLOOR_MINOR" ]; then
					fail "glibc $glibc is older than the 2.$GLIBC_FLOOR_MINOR the release build needs" "Build from source instead: https://github.com/$REPO#build-from-source"
				fi
				platform="$platform · glibc $glibc"
			fi
		fi
		;;
	Darwin)
		target="$cpu-apple-darwin"
		if [ "$cpu" = aarch64 ]; then platform="macOS Apple silicon"; else platform="macOS Intel"; fi
		;;
	MINGW* | MSYS* | CYGWIN* | Windows_NT)
		fail "Windows is not supported" "Zeroth runs on Linux and macOS; on Windows, use it inside WSL."
		;;
	*) fail "unsupported operating system: $os" "Zeroth ships for Linux and macOS." ;;
	esac
}

# ---------------------------------------------------------------------------
# The network: curl, or wget where curl is missing.
# ---------------------------------------------------------------------------
# need_fetcher — sets `fetcher`, or returns 1 where neither tool is installed.
# Split from need_tools so a preview (which fetches only its artwork, and can
# do without) asks for this much and no more.
need_fetcher() {
	if command -v curl >/dev/null 2>&1; then
		fetcher=curl
	elif command -v wget >/dev/null 2>&1; then
		fetcher=wget
	else
		fetcher=''
		return 1
	fi
}

need_tools() {
	need_fetcher || fail "neither curl nor wget is installed" "Install one and run this again."
	command -v tar >/dev/null 2>&1 || fail "tar is not installed"
	if command -v sha256sum >/dev/null 2>&1; then
		hasher=sha256sum
	elif command -v shasum >/dev/null 2>&1; then
		hasher=shasum
	elif command -v openssl >/dev/null 2>&1; then
		hasher=openssl
	else
		fail "no SHA-256 tool found" "Install sha256sum, shasum or openssl."
	fi
}

# fetch URL DEST [progress] — a file; `progress` shows curl's bar on a terminal.
fetch() {
	if [ "$fetcher" = curl ]; then
		if [ "${3:-}" = progress ] && [ -t 2 ]; then
			curl -fL --progress-bar -o "$2" "$1"
		else
			curl -fsSL -o "$2" "$1"
		fi
	else
		wget -q -O "$2" "$1"
	fi
}

# remote_size URL — curl's final response Content-Length, or zero when the
# server does not publish one (or wget is the fetcher). This is only a hint
# for the visual bar; the download itself works without a known size.
remote_size() {
	if [ "$fetcher" = curl ]; then
		curl --connect-timeout 2 --max-time 5 -fLsSI "$1" 2>/dev/null |
			awk 'tolower($1) == "content-length:" { gsub("\\r", "", $2); size = $2 } END { if (size ~ /^[0-9]+$/) print size; else print 0 }'
	else
		printf '0\n'
	fi
}

# progress_line CURRENT TOTAL FRAME — a fixed-width determinate bar when the
# server gave us a size, otherwise a bouncing segment plus actual bytes.
progress_line() {
	current="$1" total="$2" frame="$3"
	bar_width=24
	completed="${4:-0}"
	marker="$arrow"
	if [ "$completed" = 1 ]; then
		filled="$bar_width"
		progress_text='100%'
		marker="$tick"
	elif [ "$total" -gt 0 ] 2>/dev/null; then
		filled=$((current * bar_width / total))
		[ "$filled" -le "$bar_width" ] || filled=$bar_width
		percent=$((current * 100 / total))
		[ "$percent" -le 100 ] || percent=100
		progress_text="$percent%"
	else
		phase=$((frame % 36))
		if [ "$phase" -le 18 ]; then position="$phase"; else position=$((36 - phase)); fi
		filled=0
		progress_text='downloading'
	fi
	bar=''
	cell=0
	while [ "$cell" -lt "$bar_width" ]; do
		if [ "$completed" = 1 ] || [ "$total" -gt 0 ] 2>/dev/null; then
			if [ "$cell" -lt "$filled" ]; then char="$bar_fill"; else char="$bar_empty"; fi
		else
			if [ "$cell" -ge "$position" ] && [ "$cell" -lt $((position + 5)) ]; then char="$bar_fill"; else char="$bar_empty"; fi
		fi
		bar="$bar$char"
		cell=$((cell + 1))
	done
	bytes=$(human_size "$current")
	printf '\r  %s%s%s  %-9s [%s%s%s] %s  %s%s%s\033[K' \
		"$cyan" "$marker" "$reset" "Download" "$cyan" "$bar" "$reset" "$progress_text" "$dim" "$bytes" "$reset"
}

# fetch_live URL DEST LABEL — the Download row. On a terminal the fetch runs
# in the background and its progress bar is redrawn in place; the command's
# own exit status is what this returns. The banner stays above the step log.
# A background job's `kill -0` still answers for a corpse the shell has not
# reaped, so the job writes its status to a file and the poll watches that.
fetch_live() {
	if ! animate_ok; then
		fetch "$1" "$2" progress
		return
	fi
	rm -f "$tmp/fetch.status"
	total=$(remote_size "$1" || printf '0')
	# `set -e` would end the subshell at the failed fetch, before it wrote
	# anything — and the poll below would spin forever.
	(
		if fetch "$1" "$2"; then rc=0; else rc=$?; fi
		printf '%s\n' "$rc" >"$tmp/fetch.status"
	) &
	pid=$!
	hide_cursor
	i=0
	# A cap, so a fetch that is killed rather than failing cannot hang the
	# install: two minutes of frames is ten times the longest real download.
	while [ ! -s "$tmp/fetch.status" ] && [ "$i" -lt 1200 ]; do
		size=''
		if [ -f "$2" ]; then size=$(wc -c <"$2" | tr -d ' '); else size=0; fi
		progress_line "$size" "$total" "$i"
		tick
		i=$((i + 1))
	done
	# The cap fired and left no status: the fetch is still running, or died
	# without a word. Either way this must not answer "installed", and it
	# must not sit in `wait` until a stalled fetch finishes.
	if [ ! -s "$tmp/fetch.status" ]; then
		kill "$pid" 2>/dev/null || true
		show_cursor
		printf '\r\033[K'
		return 1
	fi
	wait "$pid" 2>/dev/null || true
	rc=$(cat "$tmp/fetch.status")
	if [ "$rc" = 0 ]; then
		if [ -f "$2" ]; then size=$(wc -c <"$2" | tr -d ' '); else size=0; fi
		progress_line "$size" "$total" "$i" 1
		printf '\n'
		show_cursor
		download_logged=1
		return 0
	fi
	show_cursor
	printf '\r\033[K'
	return 1
}

# The tag the repository's "latest release" redirect points at — read off the
# redirect itself with either fetcher, never GitHub's API: the API is
# rate-limited per address (a shared NAT or a CI runner spends its hour's
# allowance fast) and knows nothing of ZEROTH_INSTALL_BASE_URL, so a fork
# asking it would be handed this repository's newest tag.
latest_tag() {
	if [ "$fetcher" = curl ]; then
		final=$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$BASE_URL/releases/latest") || return 1
	else
		# -S prints each hop's headers on stderr; the last Location is where
		# the chain ended. GitHub sends it absolute and a stand-in may send it
		# relative, so only the last path component is read, as with curl.
		final=$(wget -q -S --spider --max-redirect=10 "$BASE_URL/releases/latest" 2>&1 |
			awk 'tolower($1) == "location:" { url = $2 } END { if (url == "") exit 1; print url }') || return 1
	fi
	printf '%s\n' "${final##*/}"
}

sha256_of() {
	case "$hasher" in
	sha256sum) sha256sum "$1" | awk '{ print $1 }' ;;
	shasum) shasum -a 256 "$1" | awk '{ print $1 }' ;;
	openssl) openssl dgst -sha256 "$1" | awk '{ print $NF }' ;;
	esac
}

human_size() {
	awk -v b="$1" 'BEGIN { if (b >= 1048576) printf "%.1f MB", b / 1048576; else if (b >= 1024) printf "%d KB", b / 1024; else printf "%d B", b }'
}

# ~-shorten a path for display.
pretty() {
	case "$1" in
	"$HOME"/*) printf '~%s\n' "${1#"$HOME"}" ;;
	*) printf '%s\n' "$1" ;;
	esac
}

# ---------------------------------------------------------------------------
main() {
	# Before anything can `fail`: the glyphs and colours in its message come
	# from here, and `set -u` would stop it on an unset one.
	setup_looks
	want="${ZEROTH_VERSION:-}"
	dir="${ZEROTH_INSTALL_DIR:-$HOME/.local/bin}"
	preview_secs=''
	while [ $# -gt 0 ]; do
		case "$1" in
		--version | -v)
			[ $# -ge 2 ] || fail "--version needs a value, like v0.1.0"
			want="$2"
			shift 2
			;;
		--version=*) want="${1#--version=}"; shift ;;
		--dir | -d)
			[ $# -ge 2 ] || fail "--dir needs a directory"
			dir="$2"
			shift 2
			;;
		--dir=*) dir="${1#--dir=}"; shift ;;
		# `--proview` is the spelling this script shipped its first day with;
		# it stays a synonym so a script quoting it keeps working.
		--preview | --proview)
			preview_secs=10
			case "${2:-}" in
			'' | *[!0-9]*) shift ;;
			*) preview_secs="$2"; shift 2 ;;
			esac
			;;
		--preview=* | --proview=*) preview_secs="${1#*=}"; shift ;;
		-h | --help)
			usage
			exit 0
			;;
		*) fail "unknown option: $1" "Try --help." ;;
		esac
	done
	if [ -n "$preview_secs" ]; then
		case "$preview_secs" in
		'' | *[!0-9]*) fail "--preview needs a whole number of seconds" "Try --preview 10." ;;
		esac
		[ "$preview_secs" -gt 0 ] || fail "--preview needs at least one second"
	fi

	tmp=$(mktemp -d 2>/dev/null || mktemp -d -t zeroth)
	trap 'show_cursor; rm -rf "${tmp:-}"' EXIT INT TERM HUP

	if [ -n "$preview_secs" ]; then
		# The preview draws the banner an install draws, artwork and all — the
		# same temp dir and the same fetch, so what it shows is what you get.
		# It stops there: no platform check, no release, nothing installed.
		# A machine with neither curl nor wget keeps the text banner.
		need_fetcher >/dev/null 2>&1 || true
		if prepare_art; then art_ready=1; else art_ready=0; fi
		preview "$preview_secs"
		exit 0
	fi

	need_tools
	detect_target
	if prepare_art; then art_ready=1; else art_ready=0; fi
	banner
	step "Platform" "$platform $dim($target)$reset"

	if [ -n "$want" ]; then
		case "$want" in v*) tag="$want" ;; *) tag="v$want" ;; esac
		how="pinned"
	else
		tag=$(latest_tag) || fail "could not reach $BASE_URL/releases/latest" "Check your connection, or pin a release with ZEROTH_VERSION=vX.Y.Z."
		case "$tag" in v[0-9]*) ;; *) fail "could not work out the latest release" "The redirect ended at '$tag'. Pin one with ZEROTH_VERSION=vX.Y.Z." ;; esac
		how="latest"
	fi
	version="${tag#v}"
	step "Release" "$tag ${dim}· $how$reset"

	asset="$BIN-$tag-$target"
	archive="$tmp/$asset.tar.gz"
	url="$BASE_URL/releases/download/$tag/$asset.tar.gz"

	fetch_live "$url" "$archive" "$asset.tar.gz" || fail "no release asset for $target at $tag" "Looked for $url"
	size=$(human_size "$(wc -c <"$archive" | tr -d ' ')")
	if [ "${download_logged:-0}" -ne 1 ]; then
		done_ "Download" "$asset.tar.gz $dim($size)$reset"
	fi
	# SHA256SUMS covers every archive in the release — one published file
	# rather than one beside each asset, which is what the release page
	# lists and `sha256sum -c` reads.
	sums="$tmp/SHA256SUMS"
	sums_url="$BASE_URL/releases/download/$tag/SHA256SUMS"
	fetch "$sums_url" "$sums" || fail "the checksum file for this release is missing" "Refusing to install an unverified download. Looked for $sums_url"

	expected=$(awk -v name="$asset.tar.gz" '$2 == name { print $1; exit }' "$sums")
	[ -n "$expected" ] || fail "SHA256SUMS does not list $asset.tar.gz" "Refusing to install an unverified download. It lists: $(awk '{ print $2 }' "$sums" | paste -sd ' ' - | cut -c1-160)"
	case "$expected" in
	[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]*) ;;
	*) fail "the published checksum file is malformed" "SHA256SUMS reads: $(head -c 80 "$sums")" ;;
	esac
	[ "${#expected}" -eq 64 ] || fail "the published checksum file is malformed" "Expected 64 hex digits, got ${#expected}."
	actual=$(sha256_of "$archive")
	if [ "$actual" != "$expected" ]; then
		fail "checksum mismatch for $asset.tar.gz" "published $expected
     computed  $actual
     The download is corrupt or was tampered with."
	fi
	done_ "Checksum" "sha256 $dim$(printf '%s' "$expected" | cut -c1-12)…$(printf '%s' "$expected" | cut -c57-64)$reset matches the published value $dim($size)$reset"

	tar -xzf "$archive" -C "$tmp" || fail "could not extract $asset.tar.gz"
	src="$tmp/$asset/$BIN"
	[ -f "$src" ] || fail "the archive does not contain $asset/$BIN"

	previous=""
	if command -v "$BIN" >/dev/null 2>&1; then
		previous=$("$BIN" --version 2>/dev/null | awk '{ print $2 }') || previous=""
	fi

	mkdir -p "$dir" 2>/dev/null || fail "cannot create $(pretty "$dir")" "Pick another place: ZEROTH_INSTALL_DIR=\$HOME/bin"
	[ -w "$dir" ] || fail "$(pretty "$dir") is not writable" "Pick a directory you own: ZEROTH_INSTALL_DIR=\$HOME/bin"
	# Land beside the old binary, then move over it: a running zeroth
	# keeps its mapped file and the new one appears whole, never half-written.
	staged="$dir/.$BIN.$$"
	if ! { cp "$src" "$staged" && chmod 755 "$staged" && mv -f "$staged" "$dir/$BIN"; }; then
		rm -f "$staged"
		fail "could not write $(pretty "$dir/$BIN")"
	fi

	reported=$("$dir/$BIN" --version 2>&1) || fail "the installed binary did not run" "$reported"
	case "$reported" in
	"$BIN $version") ;;
	*) note "the binary reports '$reported', expected '$BIN $version'" ;;
	esac
	if [ -n "$previous" ] && [ "$previous" != "$version" ]; then
		done_ "Installed" "$(pretty "$dir/$BIN") ${dim}· upgraded from $previous$reset"
	else
		done_ "Installed" "$(pretty "$dir/$BIN") ${dim}· $reported$reset"
	fi

	printf '\n'
	printf '  %sZeroth %s is ready.%s Start it with\n\n' "$bold" "$tag" "$reset"
	printf '      %s%s%s\n\n' "$cyan" "$BIN" "$reset"
	printf '  then %s/login%s to connect a provider and %s/model%s to choose a model.\n' "$bold" "$reset" "$bold" "$reset"
	case ":$PATH:" in
	*":$dir:"*) ;;
	*)
		printf '\n'
		note "$(pretty "$dir") is not on your PATH. Add it to your shell's startup file:"
		case "$(basename "${SHELL:-sh}")" in
		zsh) rc=".zshrc" ;;
		bash) if [ "$os" = Darwin ]; then rc=".bash_profile"; else rc=".bashrc"; fi ;;
		fish) rc="" ;;
		*) rc=".profile" ;;
		esac
		if [ -n "$rc" ]; then
			printf "     %secho 'export PATH=\"%s:\$PATH\"' >> ~/%s%s\n" "$cyan" "$(pretty "$dir" | sed "s|^~|\\\$HOME|")" "$rc" "$reset"
		else
			printf '     %sfish_add_path %s%s\n' "$cyan" "$dir" "$reset"
		fi
		;;
	esac
	printf '\n  %sUninstall any time with: rm %s%s\n\n' "$dim" "$(pretty "$dir/$BIN")" "$reset"
}

main "$@"
