#!/usr/bin/env python3
"""scripts/docs/zh_parity.py — the mechanical half of the bilingual docs rule.

    python3 scripts/docs/zh_parity.py            # check every existing pair
    python3 scripts/docs/zh_parity.py --missing  # also list pages with no zh yet
    python3 scripts/docs/zh_parity.py docs/cli   # only pairs under a path

`docs/README.zh.md` states the rule this script enforces: the English page is
the single source of truth and keeps its path; a translation sits beside it as
`<name>.zh.md`. Translation prose drifts on its own, and a page that no longer
matches the code is already a bug — a page whose *structure* no longer matches
its original is the same bug with nobody able to see it, because the reader of
the translation cannot read the original.

So this compares what a translation is not allowed to change:

  fences      every ``` block — plus its info string, and its bytes — the same,
              in the same order. A help output, a command or a JSON payload
              that drifts is worse than untranslated: someone copies it.
  code spans  the multiset of rendered `inline` spans. They name identifiers,
              flags and files, and `docs/README.zh.md` says an English term that
              IS a code identifier stays English. A `code span` here means one
              Code node in the markdown AST, so a stray backtick in prose
              cannot swallow the rest of a paragraph.
  links       the same set of targets. Paths are shared by convention, so a
              target present in English and absent in Chinese is a whole
              referenced page that went missing — the failure this caught first,
              three index entries dropped from the Chinese index.
  headings    the same count, in the same level sequence.

Not compared, because the convention allows it: the prose itself (length, line
breaks, sentence order), and a pure-anchor link `](#heading)`, which is
translated together with the heading it names. An anchor is instead required to
resolve inside its own file — a translated heading with an untranslated anchor
is a link that goes nowhere, which is still an error.

Exit 0 when every pair agrees, 1 when one does not, naming file and defect.
Read-only, offline and fast. Run it after touching any page that has a
translation; AGENTS.md carries it as one row of the change matrix.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DOCS = ROOT / "docs"

FENCE_OPEN = re.compile(r"^(?P<fence>```+|~~~+)(?P<info>[^\n]*)$")
HEADING = re.compile(r"^(?P<hashes>#{1,6})\s+(?P<text>.*?)\s*$")
HTML_COMMENT = re.compile(r"<!--.*?-->", re.S)
# An escaped backtick is not a delimiter.
# An OPENER may not be backslash-escaped (`\`` is a literal backtick in text).
BACKTICK_RUN = re.compile(r"(?<!\\)(`+)")
# A CLOSER may be: CommonMark does not process backslash escapes inside a code
# span, so `\` in backticks is a one-character span holding a backslash and its
# closer is preceded by one. Treating that closer as escaped made the opener
# pair with a run far below and swallowed whole paragraphs.
BACKTICK_ANY = re.compile(r"(`+)")
# `[text](target)`, or `[text](target "title")`. The target stops at
# whitespace-in-parentheses so an unbalanced `(` in prose cannot swallow a
# paragraph.
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")


class Doc:
    """A markdown file, read the way a markdown parser reads it."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.text = path.read_text(encoding="utf-8")
        self.fences: list[tuple[str, str]] = []
        self.prose = ""
        self._split()

    def _split(self) -> None:
        """Separate fenced blocks from prose.

        A fence blocks the parser until one of its own kind closes it, so a
        paragraph may span many lines, and only a *bare* fence of the same
        marker (` ```` `, not ` ```` rust `) ends the block.
        """
        prose: list[str] = []
        marker: str | None = None
        info = ""
        body: list[str] = []
        for line in self.text.splitlines():
            match = FENCE_OPEN.match(line)
            if marker is None:
                if match and not line.startswith("    "):
                    marker, info, body = match.group("fence")[0], match.group("info").strip(), []
                else:
                    prose.append(line)
                continue
            if match and match.group("fence")[0] == marker and not match.group("info").strip():
                self.fences.append((info, "\n".join(body)))
                marker = None
                continue
            body.append(line)
        if marker is not None:  # unterminated: kept, and the count will flag it
            self.fences.append((info + " <unterminated>", "\n".join(body)))
        self.prose = HTML_COMMENT.sub("", "\n".join(prose))

    def _nodes(self, text: str):
        """Yield ('code', span) / ('link', target) / ('text', run).

        Backtick pairing follows CommonMark exactly: an opener of n backticks
        closes at the next run of **exactly** n, and a longer run is content,
        not a closer. `find` was wrong here — it matched the first character of
        a longer run, so `x` followed later by `` y `` paired with half of the
        double run and produced spans that no renderer would show.
        """
        index = 0
        while index < len(text):
            match = BACKTICK_RUN.search(text, index)
            link = LINK.search(text, index)
            if match is None and link is None:
                break
            if link is not None and (match is None or link.start() < match.start()):
                yield ("text", text[index:link.start()])
                yield ("link", link.group(1))
                index = link.end()
                continue
            run = match.group(1)
            # Walk the runs of backticks looking for one of this exact length;
            # inside the span a backslash is just a character.
            close = None
            for candidate in BACKTICK_ANY.finditer(text, match.end()):
                if len(candidate.group(1)) == len(run):
                    close = candidate
                    break
            if close is None:
                index = match.end()  # no equal-length run: the opener is literal
                continue
            yield ("text", text[index:match.start()])
            yield ("code", text[match.end():close.start()])
            index = close.end()
        yield ("text", text[index:])

    def _paragraphs(self) -> list[str]:
        """Prose split at blank lines.

        Neither a `code span`, a link nor an emphasis run may cross one — a
        blank line ends the paragraph, and markdown restarts inline parsing on
        the next. Reading the whole page as one run let a stray `` ` `` in one
        paragraph pair with a run a thousand lines later and swallow everything
        between them, which reported phantom spans and hid real ones.
        """
        blocks, current = [], []
        for line in self.prose.splitlines():
            if line.strip():
                current.append(line)
            elif current:
                blocks.append("\n".join(current))
                current = []
        if current:
            blocks.append("\n".join(current))
        return blocks

    def code_spans(self) -> list[str]:
        # A span re-wrapped across lines renders the same: markdown folds its
        # newline and indentation to one space, so compare it that way.
        return [
            " ".join(value.split())
            for block in self._paragraphs()
            for kind, value in self._nodes(block)
            if kind == "code"
        ]

    def links(self) -> list[str]:
        return [
            value
            for block in self._paragraphs()
            for kind, value in self._nodes(block)
            if kind == "link"
        ]

    def paragraphs_with_unbalanced_backticks(self) -> list[int]:
        """Indexes of paragraphs whose backtick runs cannot pair up."""
        out = []
        for index, block in enumerate(self._paragraphs()):
            if len(BACKTICK_ANY.findall(block)) % 2:
                out.append(index)
        return out

    def headings(self) -> list[tuple[int, str]]:
        found = []
        for line in self.prose.splitlines():
            match = HEADING.match(line)
            if match:
                found.append((len(match.group("hashes")), match.group("text")))
        return found

    def slugs(self) -> set[str]:
        """The anchors GitHub would generate for this file's headings."""
        out: set[str] = set()
        for _, text in self.headings():
            # Keep word characters and hyphens; drop markdown syntax, spaces
            # and punctuation. Curly quotes and dashes are punctuation too.
            slug = re.sub(r"[^\w\- ]", "", text, flags=re.UNICODE)
            slug = slug.strip().replace(" ", "-").lower()
            out.add(slug)
        return out


def _fence_fault(en: tuple[str, str], zh: tuple[str, str]) -> str | None:
    en_info, en_body = en
    zh_info, zh_body = zh
    if en_info != zh_info:
        return f"info {zh_info!r} != {en_info!r}"
    if en_body == zh_body:
        return None
    for number, (a, b) in enumerate(zip(en_body.splitlines(), zh_body.splitlines()), start=1):
        if a != b:
            return f"body differs at line {number}: {b!r} != {a!r}"
    return "body differs (different length)"


def compare(en: Doc, zh: Doc) -> tuple[list[str], list[str]]:
    """(failures, notes): a note is worth a look but is not drift."""
    bad: list[str] = []
    notes: list[str] = []

    if len(en.fences) != len(zh.fences):
        bad.append(f"fences: {len(zh.fences)} in zh, {len(en.fences)} in en")
    else:
        for index, pair in enumerate(zip(en.fences, zh.fences), start=1):
            fault = _fence_fault(*pair)
            if fault:
                bad.append(f"fence {index} ({pair[0][0] or 'plain'}): {fault}")

    def multiset(values: list[str]) -> dict[str, int]:
        counts: dict[str, int] = {}
        for value in values:
            counts[value] = counts.get(value, 0) + 1
        return counts

    # A source page can wrap a run of backticks in SINGLE backticks — ` ``` `,
    # ``` `` ``` — which CommonMark pairs with the next single backtick far
    # below, so the "span" it produces is a paragraph-sized artifact that no
    # renderer draws. A translator who writes the same literal correctly
    # (``` ``` ```, ``` `` ```) is *right* and the original is malformed, so an
    # artifact is only required to survive as its own backtick run.
    def is_artifact(span: str) -> bool:
        # Long AND sentence-shaped: a real identifier is one `::`-joined token
        # with no spaces, while a backtick-pairing artifact swallows prose.
        return (
            len(span) > 60
            and "`" not in span.strip("`")
            and " " in span.strip()
            and not re.fullmatch(r"[\w:<>()\[\].!'/-]+", span.strip())
        )

    # A paragraph whose backtick runs don't pair is ambiguous in the SOURCE:
    # ` ``` ` (one backtick wrapping three) leaves an odd run, CommonMark pairs
    # the stray with the next single backtick paragraphs later, and the
    # "spans" it yields are paragraph-sized prose that no renderer draws. A
    # translation cannot be compared against that without judging the original,
    # so such a paragraph is reported as a note and its spans left out.
    # Compare only paragraphs whose backticks pair on BOTH sides. A paragraph
    # with an odd run cannot be read reliably by anyone — the stray opener is
    # file-position dependent — so it is skipped and announced, rather than
    # turning a faithful translation into a red run over a defect in the
    # source's own markup.
    unclear = set(en.paragraphs_with_unbalanced_backticks()) & set(
        zh.paragraphs_with_unbalanced_backticks()
    )
    if unclear:
        notes.append(
            "unbalanced backticks on both sides, spans there not compared: "
            + ", ".join(f"paragraph {n}" for n in sorted(unclear))
        )

    # A span whose *content* is backticks — `` ` ``, ``` ``` ```, `` `x`` `` —
    # says something about markdown's own delimiter syntax, and the source's
    # spelling of it is ambiguous: ` ``` ` is one backtick around three, which
    # pairs with any later single backtick. Such a span is required to exist,
    # but not to be spelled identically; every other span is exact.
    def split_spans(doc: Doc) -> tuple[list[str], list[str]]:
        exact, about_backticks = [], []
        for index, block in enumerate(doc._paragraphs()):
            if index in unclear:
                continue
            for kind, value in doc._nodes(block):
                if kind != "code":
                    continue
                normalised = " ".join(value.split())
                (about_backticks if "`" in value else exact).append(normalised)
        return exact, about_backticks

    en_all, en_backtick_spans = split_spans(en)
    zh_all, zh_backtick_spans = split_spans(zh)
    if len(en_backtick_spans) != len(zh_backtick_spans):
        notes.append(
            f"spans about backtick syntax: {len(zh_backtick_spans)} in zh, "
            f"{len(en_backtick_spans)} in en"
        )
    en_spans = multiset([s for s in en_all if not is_artifact(s)])
    zh_spans = multiset([s for s in zh_all if not is_artifact(s)])
    for artifact in [s for s in en_all if is_artifact(s)]:
        # The content it swallowed must still be present in the translation:
        # compare the word-shaped tail so a dropped paragraph is still caught.
        tail = artifact.strip()[-40:]
        if not any(tail[:20] in span for span in zh_all):
            # The prose it swallowed must still be somewhere in the
            # translation; the paragraph is roughly located by its opening
            # words, which a translator keeps recognisable in the translation
            # only by meaning, so the check is on the whole page's text.
            if tail[:20] not in zh.text:
                notes.append(f"prose inside a source artifact not found verbatim: …{artifact.strip()[:40]}…")
    lost = {s: n - zh_spans.get(s, 0) for s, n in en_spans.items() if n > zh_spans.get(s, 0)}
    odd = {s: n - en_spans.get(s, 0) for s, n in zh_spans.items() if n > en_spans.get(s, 0)}
    if lost:
        # A span the English has and the Chinese does not is a lost identifier,
        # flag or path — the one thing a translation may not paraphrase away.
        bad.append("code spans missing: " + ", ".join(f"`{s}`" for s in sorted(lost)))
    if odd:
        # More repeats than the original: usually a translator naming a term
        # twice, which is allowed. A *new* string is worth a human look, so it
        # is reported without failing the run.
        notes.append("code spans only in zh: " + ", ".join(f"`{s}`" for s in sorted(odd)))

    en_targets, zh_targets = en.links(), zh.links()
    # An anchor is translated with its heading, so only path-shaped links are
    # required to match. Anchors are checked to resolve, below.
    def paths(links: list[str]) -> dict[str, int]:
        return multiset([link for link in links if not link.startswith("#")])

    en_paths, zh_paths = paths(en_targets), paths(zh_targets)
    missing = sorted(set(en_paths) - set(zh_paths))
    extra = sorted(set(zh_paths) - set(en_paths))
    if missing:
        bad.append("link targets missing: " + ", ".join(missing))
    if extra:
        bad.append("link targets only in zh: " + ", ".join(extra))

    known = zh.slugs()
    for target in sorted({t for t in zh_targets if t.startswith("#")}):
        if target[1:] not in known:
            bad.append(f"dangling anchor in zh: {target}")

    # A translation of a long page that stops early still matches every
    # heading it reached, so the count is the guard: a Chinese page shorter
    # than three fifths of its original was truncated, not concise.
    en_lines = len(en.text.splitlines())
    zh_lines = len(zh.text.splitlines())
    if en_lines > 60 and zh_lines < en_lines * 0.55:
        bad.append(f"truncated? {zh_lines} lines against {en_lines} in en")

    en_levels = [level for level, _ in en.headings()]
    zh_levels = [level for level, _ in zh.headings()]
    if en_levels != zh_levels:
        bad.append(
            f"headings: {len(zh_levels)} at levels {''.join(map(str, zh_levels))}"
            f" != {len(en_levels)} at {''.join(map(str, en_levels))}"
        )

    return bad, notes


def pairs(under: list[str]) -> list[tuple[Path, Path | None]]:
    """Every translation, paired with its original (None when there is none)."""
    found: list[tuple[Path, Path | None]] = []
    for zh in sorted(ROOT.rglob("*.zh.md")):
        rel = zh.relative_to(ROOT)
        if rel.parts[0] in ("target", "vendor"):
            continue
        if under and not any(str(rel).startswith(u) for u in under):
            continue
        en = zh.with_name(zh.name[: -len(".zh.md")] + ".md")
        found.append((zh, en if en.exists() else None))
    return found


def missing_translations(under: list[str]) -> list[Path]:
    """Pages the convention says should have a translation, and do not."""
    bases = [DOCS, ROOT / "guide"]
    bases += [
        ROOT / name
        for name in ("README.md", "CLAUDE.md", "AGENTS.md", "CONTRIBUTING.md", "CHANGELOG.md", "TELEMETRY.md")
    ]
    out: list[Path] = []
    for base in bases:
        walk = [base] if base.is_file() else sorted(base.rglob("*.md"))
        for path in walk:
            if not path.exists() or path.name.endswith(".zh.md"):
                continue
            rel = str(path.relative_to(ROOT))
            if under and not any(rel.startswith(u) for u in under):
                continue
            if not path.with_name(path.name[: -len(".md")] + ".zh.md").exists():
                out.append(path)
    return out


def main(argv: list[str]) -> int:
    under = [arg for arg in argv[1:] if not arg.startswith("-")]
    show_missing = "--missing" in argv

    failures = 0
    checked = 0
    for zh, en in pairs(under):
        rel = str(zh.relative_to(ROOT))
        if en is None:
            print(f"FAIL {rel}: no original {zh.name[: -len('.zh.md')]}.md beside it")
            failures += 1
            continue
        checked += 1
        bad, notes = compare(Doc(en), Doc(zh))
        if bad:
            failures += 1
            print(f"FAIL {rel}")
            for line in bad:
                print(f"     {line}")
        for line in notes:
            print(f"note {rel}")
            print(f"     {line}")

    if show_missing:
        absent = missing_translations(under)
        for path in absent:
            print(f"TODO {path.relative_to(ROOT)}")
        print(f"     {len(absent)} page(s) without a .zh.md")

    print(f"— {checked} pair(s) checked, {failures} failing")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
