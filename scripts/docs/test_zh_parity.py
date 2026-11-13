#!/usr/bin/env python3
"""scripts/docs/test_zh_parity.py — the parity check's own tests.

    python3 -m unittest discover -s scripts/docs -p 'test_*.py'

`zh_parity.py` decides whether a translation still mirrors its original, and a
checker that quietly passes is worse than none: `docs/` would carry pages that
*look* maintained while their commands, links and headings have rotted. So the
cases here are fixtures, not the checkout — a pair in a temp directory, made to
break in exactly one way, asserted to be caught for that reason.

Each defect the checker claims to catch has a case that makes it fire, and each
thing the convention allows (prose differences, a translated anchor) has a case
that must NOT fire. The last two are what keeps the check usable: a translator
re-wrapping a paragraph or translating a heading must not see a red run.
"""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import zh_parity  # noqa: E402  (same-directory import, no package)


def doc(text: str, name: str = "page.md") -> zh_parity.Doc:
    """A Doc built from a string, in a temp dir, so nothing touches docs/."""
    handle = tempfile.NamedTemporaryFile("w", suffix=name, delete=False, encoding="utf-8")
    handle.write(text)
    handle.close()
    return zh_parity.Doc(Path(handle.name))


EN = """# Title

Prose about `some_symbol` and a [link](other.md).

```
$ run --flag
out
```

## Section

Another [anchor](#title).
"""


class Fences(unittest.TestCase):
    def test_a_translated_body_is_a_failure(self) -> None:
        zh = doc(EN.replace("$ run --flag", "$ run --flag # 运行"))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any("fence 1" in line for line in bad), bad)

    def test_a_translated_info_string_is_a_failure(self) -> None:
        zh = doc(EN.replace("```\n$ run", "```text\n$ run", 1))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any("info" in line for line in bad), bad)

    def test_a_missing_fence_is_a_failure(self) -> None:
        zh = doc(EN.replace("```\n$ run --flag\nout\n```\n\n", ""))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any(line.startswith("fences:") for line in bad), bad)

    def test_a_fence_holds_nested_backticks(self) -> None:
        # A ```` inside a ``` block is content; only a bare fence of the same
        # marker closes it. (The bug that made every later block misalign.)
        source = "```\na ```` b\n```\n\nafter\n"
        self.assertEqual(len(doc(source).fences), 1)
        self.assertIn("after", doc(source).prose)

    def test_an_untagged_close_does_not_end_an_untagged_fence_early(self) -> None:
        source = "```\nline\n~~~\nmore\n```\n"
        self.assertEqual(doc(source).fences[0][1], "line\n~~~\nmore")


class CodeSpans(unittest.TestCase):
    def test_a_paraphrased_symbol_is_a_failure(self) -> None:
        zh = doc(EN.replace("`some_symbol`", "某个符号"))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any("code spans missing" in line for line in bad), bad)

    def test_a_span_repeated_in_the_translation_is_only_a_note(self) -> None:
        # Naming a term twice in Chinese is a translator's call, not drift.
        zh = doc(EN.replace("and a [link]", "and `some_symbol` and a [link]"))
        bad, notes = zh_parity.compare(doc(EN), zh)
        self.assertEqual(bad, [])
        self.assertTrue(any("only in zh" in line for line in notes), notes)

    def test_a_span_wrapped_across_lines_matches(self) -> None:
        en = doc("Run `cargo test --lib app::tests`\n")
        zh = doc("运行 `cargo test --lib\napp::tests`\n")
        self.assertEqual(zh_parity.compare(en, zh)[0], [])

    def test_backticks_pair_the_way_the_renderer_pairs_them(self) -> None:
        # CommonMark pairs an opener with the NEXT run of the same length, and
        # only an equal-length run closes it. Getting this wrong would make a
        # stray backtick look like a span and hide a real one, so the checker
        # follows the renderer rather than a guess.
        zh = doc("A lone ` quote, then `kept`.\n")
        self.assertEqual(zh.code_spans(), ["quote, then"])
        mixed = doc("``a ` b`` then `c`\n")
        self.assertEqual(mixed.code_spans(), ["a ` b", "c"])


class Links(unittest.TestCase):
    def test_a_dropped_target_is_a_failure(self) -> None:
        # The defect this found in docs/README.zh.md: three index entries gone.
        zh = doc(EN.replace(" and a [link](other.md)", ""))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any("link targets missing: other.md" in line for line in bad), bad)

    def test_an_invented_target_is_a_failure(self) -> None:
        zh = doc(EN.replace("(other.md)", "(other.zh.md)"))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any("only in zh" in line for line in bad), bad)

    def test_a_translated_anchor_is_allowed_if_it_resolves(self) -> None:
        # The convention: an anchor moves with the heading it names.
        zh = doc(EN.replace("# Title", "# 标题").replace("(#title)", "(#标题)"))
        self.assertEqual(zh_parity.compare(doc(EN), zh)[0], [])

    def test_a_dangling_anchor_is_a_failure(self) -> None:
        zh = doc(EN.replace("# Title", "# 标题"))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any("dangling anchor" in line for line in bad), bad)

    def test_a_code_span_heading_matches_its_published_anchor(self) -> None:
        # `### `src/tui/` —— 终端外壳` publishes #srctui--终端外壳.
        page = doc("### `src/tui/` —— 终端外壳\n\n[go](#srctui--终端外壳)\n")
        self.assertEqual(zh_parity.compare(page, page)[0], [])


class Headings(unittest.TestCase):
    def test_a_lost_heading_is_a_failure(self) -> None:
        zh = doc(EN.replace("## Section\n\n", ""))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any(line.startswith("headings:") for line in bad), bad)

    def test_a_demoted_heading_is_a_failure(self) -> None:
        # The real one: an h3 shipped as h4 broke the anchor its parent
        # headings generate.
        zh = doc(EN.replace("## Section", "### Section"))
        bad, _ = zh_parity.compare(doc(EN), zh)
        self.assertTrue(any("headings:" in line for line in bad), bad)

    def test_a_heading_inside_a_fence_is_not_a_heading(self) -> None:
        page = doc("```\n## not a heading\n```\n")
        self.assertEqual(page.headings(), [])


class Prose(unittest.TestCase):
    def test_reflowing_a_paragraph_is_allowed(self) -> None:
        en = doc("# T\n\none two three four\nfive six\n")
        zh = doc("# T\n\n一 二 三 四 五 六\n")
        self.assertEqual(zh_parity.compare(en, zh)[0], [])

    def test_an_html_comment_is_not_compared(self) -> None:
        en = doc("# T\n\n<!-- internal note -->\ntext\n")
        zh = doc("# T\n\ntext\n")
        self.assertEqual(zh_parity.compare(en, zh)[0], [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
