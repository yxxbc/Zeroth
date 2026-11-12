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
#   ALTER_ZERO_VERSION=v0.1.0 … | sh               # pin a release (default: the latest)
#   ALTER_ZERO_INSTALL_DIR=/usr/local/bin … | sh   # install somewhere else
#   … | sh -s -- --version v0.1.0 --dir ~/bin      # the same two, as flags
#   … | sh -s -- --preview                         # the mascot alone, 10 s, installing nothing
#   … | sh -s -- --preview 20                      # the same, for 20 s (--preview=20 too)
#   … | sh -s -- --help
#
# POSIX sh only — no arrays, no `local`, no `[[` — so `sh` on macOS (bash 3.2
# in POSIX mode), dash and busybox all run it. Everything is a function and
# the last line calls `main`, so a download cut short executes nothing.
# ALTER_ZERO_INSTALL_BASE_URL points it at a stand-in for github.com; the
# release tooling's selftest uses that to install from a local server.
set -eu

REPO="${ALTER_ZERO_REPO:-yxxbc/zeroth}"
# The installed binary is named after the repository's slug — Cargo.toml's
# `name`, which is what a release archive is named after too. One variable,
# so this repository (`yxxbc/zeroth` → `zeroth`) and a stand-in (the release
# tooling's fixture is a repository called `example/alter-zero`) both resolve.
BIN="${REPO##*/}"
BASE_URL="${ALTER_ZERO_INSTALL_BASE_URL:-https://github.com/$REPO}"
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
		spin1='▙' spin2='▛' spin3='▜' spin4='▟'
	else
		arrow='>' tick='+' cross='x' bang='!'
		spin1='|' spin2='/' spin3='-' spin4="\\"
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
# the steps, a one-row spinner while the archive downloads, and `--preview N`
# for as long as you ask. In a pipe, under `TERM=dumb` or with no colour, the
# art is drawn once and left still.
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

# The same art, once, for a pipe or a terminal that cannot hold it still.
static_banner() {
	printf '   %s▙▄▙▄▟▄▟%s      %sZeroth%s\n' "$g1" "$reset" "$bold" "$reset"
	printf '  %s▝▜▄███▄▛▘%s     %sthe one-line installer%s\n' "$g2" "$reset" "$dim" "$reset"
	printf '    %s▘▘ ▝▝%s\n' "$g3" "$reset"
}

banner() {
	printf '\n'
	if animate_ok; then
		animate $((fps * 3 / 2)) 'Zeroth' 'the one-line installer' "building $BIN"
	else
		printf '  %sZeroth%s  %sthe one-line installer%s\n' "$bold" "$reset" "$dim" "$reset"
	fi
	printf '\n'
}

# preview SECONDS — the mascot alone: no platform check, no network, and
# nothing installed. The frames are counted, not timed, so a slow terminal
# runs long rather than dropping frames.
preview() {
	printf '\n'
	if animate_ok; then
		animate "$(($1 * fps))" 'Zeroth' 'the one-line installer' "preview · $1 s · nothing is installed"
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
need_tools() {
	if command -v curl >/dev/null 2>&1; then
		fetcher=curl
	elif command -v wget >/dev/null 2>&1; then
		fetcher=wget
	else
		fail "neither curl nor wget is installed" "Install one and run this again."
	fi
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

# fetch_live URL DEST LABEL — the Download row. On a terminal the fetch runs
# in the background and its row spins in the mascot's place, reporting the
# bytes landed so far; the command's own exit status is what this returns.
# A background job's `kill -0` still answers for a corpse the shell has not
# reaped, so the job writes its status to a file and the poll watches that.
fetch_live() {
	if ! animate_ok; then
		fetch "$1" "$2" progress
		return
	fi
	rm -f "$tmp/fetch.status"
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
		case "$((i % 4))" in
		0) spin="$spin1" ;;
		1) spin="$spin2" ;;
		2) spin="$spin3" ;;
		*) spin="$spin4" ;;
		esac
		size=''
		if [ -f "$2" ]; then size=$(human_size "$(wc -c <"$2" | tr -d ' ')"); fi
		printf '\r  %s%s%s  %-9s %s %s%s\033[K' "$cyan" "$spin" "$reset" "Download" "$3" "$dim" "$size"
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
	show_cursor
	printf '\r\033[K'
	rc=$(cat "$tmp/fetch.status")
	[ "$rc" = 0 ]
}

# The tag the repository's "latest release" redirect points at — read off the
# redirect itself with either fetcher, never GitHub's API: the API is
# rate-limited per address (a shared NAT or a CI runner spends its hour's
# allowance fast) and knows nothing of ALTER_ZERO_INSTALL_BASE_URL, so a fork
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
	want="${ALTER_ZERO_VERSION:-}"
	dir="${ALTER_ZERO_INSTALL_DIR:-$HOME/.local/bin}"
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

	if [ -n "$preview_secs" ]; then
		preview "$preview_secs"
		exit 0
	fi

	tmp=$(mktemp -d 2>/dev/null || mktemp -d -t zeroth)
	trap 'show_cursor; rm -rf "${tmp:-}"' EXIT INT TERM HUP

	banner
	need_tools
	detect_target
	step "Platform" "$platform $dim($target)$reset"

	if [ -n "$want" ]; then
		case "$want" in v*) tag="$want" ;; *) tag="v$want" ;; esac
		how="pinned"
	else
		tag=$(latest_tag) || fail "could not reach $BASE_URL/releases/latest" "Check your connection, or pin a release with ALTER_ZERO_VERSION=vX.Y.Z."
		case "$tag" in v[0-9]*) ;; *) fail "could not work out the latest release" "The redirect ended at '$tag'. Pin one with ALTER_ZERO_VERSION=vX.Y.Z." ;; esac
		how="latest"
	fi
	version="${tag#v}"
	step "Release" "$tag ${dim}· $how$reset"

	asset="$BIN-$tag-$target"
	archive="$tmp/$asset.tar.gz"
	url="$BASE_URL/releases/download/$tag/$asset.tar.gz"

	fetch_live "$url" "$archive" "$asset.tar.gz" || fail "no release asset for $target at $tag" "Looked for $url"
	size=$(human_size "$(wc -c <"$archive" | tr -d ' ')")
	done_ "Download" "$asset.tar.gz $dim($size)$reset"
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

	mkdir -p "$dir" 2>/dev/null || fail "cannot create $(pretty "$dir")" "Pick another place: ALTER_ZERO_INSTALL_DIR=\$HOME/bin"
	[ -w "$dir" ] || fail "$(pretty "$dir") is not writable" "Pick a directory you own: ALTER_ZERO_INSTALL_DIR=\$HOME/bin"
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
