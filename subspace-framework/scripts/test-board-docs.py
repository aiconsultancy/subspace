"""Exercise board-guide contract failures without Cargo or service access."""

import importlib.util
import tempfile
import time
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "board_docs", Path(__file__).with_name("check-board-docs.py")
)
DOCS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DOCS)
HEADINGS = ["Overview", "Before you start", "Limits"]


class BoardDocsTests(unittest.TestCase):
    def test_sections_must_match_template_order_and_ignore_code(self) -> None:
        text = "# Test guide\n\n## Overview\n\n## Before you start\n\n## Limits\n"
        self.assertEqual(DOCS.section_errors(text, HEADINGS), [])
        self.assertTrue(DOCS.section_errors(text.replace("## Limits", "## Other"), HEADINGS))
        self.assertTrue(DOCS.section_errors(text.replace("# Test guide", "Test"), HEADINGS))
        self.assertEqual(DOCS.section_errors(text + "\n```md\n## Example\n```", HEADINGS), [])

    def test_missing_file_and_anchor_fail_but_valid_local_links_pass(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            guide = Path(directory) / "help.md"
            guide.write_text("# Test\n\n## Main view\n")
            self.assertEqual(DOCS.link_errors(guide, "[View](#main-view)"), [])
            self.assertTrue(DOCS.link_errors(guide, "[View](#missing)"))
            self.assertTrue(DOCS.link_errors(guide, "[Other](missing.md)"))
            other = guide.with_name("other.md")
            other.write_text("# Other\n")
            self.assertEqual(DOCS.link_errors(guide, "[Other](other.md#other)"), [])

    def test_links_to_another_project_are_not_checked(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "project"
            guide = root / "spaces/demo/help.md"
            guide.parent.mkdir(parents=True)
            guide.write_text("# Demo\n")
            original = DOCS.TOOL
            DOCS.TOOL = root
            try:
                self.assertEqual(DOCS.link_errors(guide, "[Other](../../../other/docs/x.md)"), [])
                self.assertTrue(DOCS.link_errors(guide, "[Mine](../../docs/missing.md)"))
            finally:
                DOCS.TOOL = original

    def test_images_require_alt_text_webp_and_embedded_registration(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            board = Path(directory)
            (board / "help-assets").mkdir()
            (board / "help-assets/view.webp").write_bytes(b"test-only-raster")
            (board / "src").mkdir()
            (board / "src/lib.rs").write_text('include_bytes!("../help-assets/view.webp")')
            text = "![Synthetic view](help-assets/view.webp)"
            self.assertEqual(DOCS.image_errors(board, text), [])
            self.assertTrue(DOCS.image_errors(board, text.replace("Synthetic view", "")))
            self.assertTrue(DOCS.image_errors(board, text.replace("view.webp", "view.png")))
            (board / "src/lib.rs").write_text("")
            self.assertTrue(DOCS.image_errors(board, text))

    def test_every_framework_space_is_checked_against_the_template(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "docs").mkdir()
            headings = "".join(f"## H{index}\n" for index in range(10))
            (root / "docs/board-guide-template.md").write_text("# Template\n" + headings)
            space = root / "spaces/demo"
            (space / "src").mkdir(parents=True)
            (space / "Cargo.toml").write_text("[package]\n")
            (space / "src/lib.rs").write_text('include_str!("../help.md")')
            (space / "help.md").write_text("# Demo\n" + headings)
            self.assertEqual(DOCS.check(root), [])
            (space / "help.md").write_text("# Demo\n")
            self.assertTrue(all(error.startswith("demo: ") for error in DOCS.check(root)))
            (root / "docs/board-guide-template.md").write_text("# Template\n")
            self.assertEqual(DOCS.check(root), ["template must declare the ten required H2 headings"])

    def test_placeholders_cannot_hide_text_or_start_docfx_html_blocks(self) -> None:
        self.assertTrue(DOCS.markup_errors("The toast is `No longer monitoring\n<title>`."))
        self.assertTrue(DOCS.markup_errors("Choose **Accept: <change>**."))
        self.assertEqual(DOCS.markup_errors("The toast is `No longer monitoring <title>`."), [])
        self.assertEqual(DOCS.markup_errors("Choose **Accept: &lt;change&gt;**."), [])
        self.assertEqual(DOCS.markup_errors("<!-- source <example> -->\n```md\n<title>\n```"), [])

    def test_fences_close_only_on_an_equal_or_longer_bare_fence(self) -> None:
        self.assertEqual(DOCS.prose("a\n```\n## Hidden\n```\nb"), "a\n\nb")
        self.assertNotIn("## Hidden", DOCS.prose("````\n```\n## Hidden\n```\n````\n"))
        self.assertNotIn("## Hidden", DOCS.prose("```\n```md\n## Hidden\n```\n"))
        self.assertNotIn("## Hidden", DOCS.prose("~~~\n## Hidden\n"))
        self.assertEqual(DOCS.prose("## Shown\n``\n"), "## Shown\n``\n")

    def test_closing_fence_allows_only_trailing_spaces_or_tabs(self) -> None:
        self.assertEqual(DOCS.prose("a\n```\nx\n``` \t\nb"), "a\n\nb")
        for separator in ("\v", "\f", "\x1c", "\u2028"):
            with self.subTest(separator=repr(separator)):
                self.assertNotIn("## Hidden", DOCS.prose(f"```\n```{separator}\n## Hidden\n```\n"))

    def test_crlf_fence_leaves_only_the_line_feed_like_the_old_regex(self) -> None:
        self.assertEqual(DOCS.prose("a\r\n```\r\nx\r\n```\r\nb"), "a\r\n\nb")
        self.assertEqual(DOCS.prose("a\r\n```\r\nx\r\n```"), "a\r\n")

    def test_only_line_feeds_split_lines(self) -> None:
        self.assertEqual(DOCS.prose("a\f```\nb\n"), "a\f```\nb\n")

    def test_inline_backtick_span_at_column_zero_is_not_a_fence(self) -> None:
        text = "# Test guide\n```example``` text\n\n## Overview\n\n## Before you start\n\n## Limits\n"
        self.assertEqual(DOCS.prose(text), text)
        self.assertEqual(DOCS.section_errors(text, HEADINGS), [])
        self.assertNotIn("## Hidden", DOCS.prose("~~~ `info` ~\n## Hidden\n~~~\n"))

    def test_code_spans_close_on_a_run_of_the_same_length(self) -> None:
        self.assertEqual(DOCS.strip_code_spans("`a` b ``c`d`` e"), " b  e")
        self.assertEqual(DOCS.strip_code_spans("`a``"), "`a``")
        self.assertEqual(DOCS.strip_code_spans("<x> `<y>`"), "<x> ")

    def test_long_backtick_runs_are_scanned_in_linear_time(self) -> None:
        started = time.perf_counter()
        DOCS.prose("`" * 100_000 + "\n" + "x\n" * 100_000)
        DOCS.strip_code_spans("`" * 100_000 + "a" * 100_000)
        DOCS.strip_code_spans("`a" * 100_000)
        self.assertLess(time.perf_counter() - started, 2.0)  # the old regexes take minutes


if __name__ == "__main__":
    unittest.main()
