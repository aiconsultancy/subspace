"""Exercise documentation index failures in isolated fixture trees."""

import importlib.util
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "docs_index", Path(__file__).with_name("check-docs-index.py")
)
INDEX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(INDEX)


def write(root: Path, name: str, text: str) -> None:
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def fixture(root: Path, index: str) -> None:
    write(root, "docs/README.md", index)
    write(root, "docs/guide.md", "# Guide\n\n## Usage\n\n[Deep](section/deep.md)\n")
    write(root, "docs/section/deep.md", "# Deep\n")
    write(root, "spaces/demo/help.md", "# Demo\n")
    write(root, "AGENTS.md", "# Agents\n\n## Structure\n")


COMPLETE = (
    "# Docs\n\n[Guide](guide.md#usage)\n"
    "[Demo](../spaces/demo/help.md)\n[Rules](../AGENTS.md#structure)\n"
    "[Built](../../obj/docs/includes/versions.md)\n"
)


class DocsIndexTests(unittest.TestCase):
    def check(self, index: str) -> list[str]:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixture(root, index)
            return INDEX.check(root)

    def test_complete_index_passes_through_section_pages(self) -> None:
        self.assertEqual(self.check(COMPLETE), [])

    def test_unlinked_page_and_board_guide_fail(self) -> None:
        errors = self.check(COMPLETE.replace("[Guide](guide.md#usage)\n", ""))
        self.assertTrue(any(error.startswith("docs/guide.md:") for error in errors))
        self.assertTrue(any(error.startswith("docs/section/deep.md:") for error in errors))
        errors = self.check(COMPLETE.replace("[Demo](../spaces/demo/help.md)\n", ""))
        self.assertTrue(any(error.startswith("spaces/demo/help.md:") for error in errors))

    def test_broken_link_and_heading_fail(self) -> None:
        self.assertTrue(self.check(COMPLETE + "[Gone](gone.md)\n"))
        self.assertTrue(self.check(COMPLETE.replace("#usage", "#missing")))
        self.assertTrue(self.check(COMPLETE.replace("#structure", "#missing")))

    def test_links_in_code_are_ignored(self) -> None:
        self.assertEqual(self.check(COMPLETE + "`[text](url)`\n\n```md\n[x](y.md)\n```\n"), [])

    def test_links_to_another_project_are_external(self) -> None:
        self.assertEqual(self.check(COMPLETE + "[Framework](../../other/docs/missing.md)\n"), [])

    def test_missing_index_fails(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(INDEX.check(Path(directory)), ["missing docs/README.md"])


if __name__ == "__main__":
    unittest.main()
