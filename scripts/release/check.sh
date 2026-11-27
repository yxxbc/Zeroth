#!/usr/bin/env bash
# scripts/release/check.sh — everything a version is written in agrees.
#
#   scripts/release.sh check            # the tree is consistent with itself
#   scripts/release.sh check v0.1.0     # …and the tag names that version
#
# Reports EVERY mismatch before failing. The workflow runs this first on a
# tag push (with $GITHUB_REF_NAME), CI runs it on every push without a tag,
# so a commit that bumps Cargo.toml without a changelog entry — or the
# other way round — never reaches a tag. The rules (docs/ops/release.md):
#
#   1. Cargo.toml's [package] version is semver.
#   2. Cargo.lock records the same version for the crate (`--locked` would
#      refuse the build otherwise, but this says so in one line).
#   3. The README's version badge — URL label and alt text — says the same.
#   4. CHANGELOG.md has a `## [X.Y.Z] - YYYY-MM-DD` section for it, with a
#      non-empty body and a `[X.Y.Z]: url` link reference.
#   5. CHANGELOG.md keeps a `## [Unreleased]` section and its link.
#   6. With a TAG: it is `vX.Y.Z` for that version, and the `[Unreleased]`
#      section is empty — an entry left there was written for this release
#      and never rolled into it.
#   7. When `CHANGELOG.zh.md` exists: it carries the same dated section, the
#      same date, and the same `[Unreleased]` target (its prose is the
#      translation's, its structure is not).
set -euo pipefail
# shellcheck source-path=SCRIPTDIR
# shellcheck source=lib.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/lib.sh"

tag="${1:-}"
if [ -z "$tag" ] && [ "${GITHUB_REF_TYPE:-}" = tag ]; then
	tag="${GITHUB_REF_NAME:-}"
fi

version="$(manifest_version)"
name="$(manifest_name)"
[ -n "$version" ] || die "Cargo.toml: no [package] version found under $RELEASE_ROOT"
if is_semver "$version"; then
	ok "Cargo.toml: $name $version"
else
	fail "Cargo.toml: version '$version' is not semver (X.Y.Z, optionally -pre or +build)"
fi

lock="$(lock_version "$name")"
if [ "$lock" = "$version" ]; then
	ok "Cargo.lock: $name $lock"
else
	fail "Cargo.lock records $name ${lock:-<none>}, Cargo.toml says $version — run scripts/release.sh prepare, or cargo update --workspace"
fi

while read -r readme; do
	[ -f "$RELEASE_ROOT/$readme" ] || continue
	badge="$(readme_badge_version "$RELEASE_ROOT/$readme")"
	alt="$(readme_alt_version "$RELEASE_ROOT/$readme")"
	if [ "$badge" = "$version" ] && [ "$alt" = "$version" ]; then
		ok "$readme: version badge $version"
	else
		fail "$readme: version badge says '${badge:-<none>}' (alt '${alt:-<none>}'), Cargo.toml says $version"
	fi
done < <(readme_badge_files)

if [ ! -f "$(changelog_path)" ]; then
	fail "CHANGELOG.md is missing"
else
	date="$(changelog_date "$version")"
	if [ -z "$date" ]; then
		fail "CHANGELOG.md: no '## [$version] - YYYY-MM-DD' section — write the release's entry (scripts/release.sh prepare rolls [Unreleased] into one)"
	elif [ -z "$(changelog_section "$version")" ]; then
		fail "CHANGELOG.md: the [$version] section is empty"
	else
		ok "CHANGELOG.md: [$version] - $date"
	fi
	if [ -z "$(changelog_link "$version")" ]; then
		fail "CHANGELOG.md: no '[$version]: <url>' link reference at the bottom"
	fi
	if ! grep -q '^## \[Unreleased\]$' "$(changelog_path)"; then
		fail "CHANGELOG.md: no '## [Unreleased]' section"
	elif [ -z "$(changelog_link Unreleased)" ]; then
		fail "CHANGELOG.md: no '[Unreleased]: <url>' link reference"
	else
		ok "CHANGELOG.md: [Unreleased] section and link present"
	fi
fi

# 7. The Chinese twin, when the tree carries one, names the same release: a
#    dated section for it and the same version in its [Unreleased] link. The
#    section's **prose** is the translation's business; this is the half that
#    went wrong for 0.22.0–0.23.0, where `prepare` rolled the English file
#    alone and the twin kept the entries the release had taken.
if changelog_zh_exists; then
	zh="$(changelog_zh_path)"
	zh_date="$(changelog_date "$version" "$zh")"
	if [ -z "$zh_date" ]; then
		fail "CHANGELOG.zh.md: no '## [$version] - YYYY-MM-DD' section — run scripts/release.sh prepare, or roll it by hand"
	elif [ -z "$(changelog_section "$version" "$zh")" ]; then
		fail "CHANGELOG.zh.md: the [$version] section is empty"
	elif [ "$zh_date" != "$date" ]; then
		fail "CHANGELOG.zh.md: [$version] is dated $zh_date, CHANGELOG.md says $date"
	else
		ok "CHANGELOG.zh.md: [$version] - $zh_date"
	fi
	zh_unreleased="$(changelog_link Unreleased "$zh")"
	en_unreleased="$(changelog_link Unreleased)"
	if [ -z "$zh_unreleased" ]; then
		fail "CHANGELOG.zh.md: no '[Unreleased]: <url>' link reference"
	elif [ -n "$en_unreleased" ] && [ "$zh_unreleased" != "$en_unreleased" ]; then
		fail "CHANGELOG.zh.md: [Unreleased] points at $zh_unreleased, CHANGELOG.md at $en_unreleased"
	else
		ok "CHANGELOG.zh.md: [Unreleased] link agrees"
	fi
	if [ -z "$(changelog_link "$version" "$zh")" ]; then
		fail "CHANGELOG.zh.md: no '[$version]: <url>' link reference at the bottom"
	fi
fi

if [ -n "$tag" ]; then
	if [ "$tag" = "$(tag_of "$version")" ]; then
		ok "tag $tag names $version"
	else
		fail "tag '$tag' does not name Cargo.toml's version $version (expected $(tag_of "$version"))"
	fi
	if [ -f "$(changelog_path)" ] && [ -n "$(changelog_section Unreleased)" ]; then
		fail "CHANGELOG.md: [Unreleased] still has entries — roll them into [$version] before tagging"
	fi
fi

finish "release check passed for $name $version${tag:+ ($tag)}"
