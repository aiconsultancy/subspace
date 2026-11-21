"""Verify project routing without running application builds."""

import os
from pathlib import Path
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("check-push.py")
HOOK = runpy.run_path(str(SCRIPT))
DOCS = ("mise", "run", "--skip-tools", "board-docs", ":::", "docs-index")


class CheckPush(unittest.TestCase):
    def checks(self, *paths):
        return list(HOOK["project_checks"](paths))

    def test_markdown_only(self):
        self.assertEqual(self.checks(
            "AGENTS.md", ".github/pull_request_template.md", "subspace-api/README.md",
            "subspace-desktop/docs/change.md", "subspace-framework/docs/guide.MD",
        ), [("subspace-framework", DOCS), ("subspace-desktop", DOCS)])

    def test_mixed_api_and_desktop_docs(self):
        result = self.checks("subspace-api/src/main.ts", "subspace-desktop/docs/guide.md")
        self.assertEqual([project for project, _ in result],
                         ["subspace-desktop", "subspace-api", "subspace-api", "subspace-api"])
        self.assertEqual(result[0][1], DOCS)
        self.assertEqual([command[5:] for _, command in result[1:]],
                         [("run", "typecheck"), ("run", "lint"), ("test",)])

    def test_desktop_rust(self):
        self.assertEqual(self.checks("subspace-desktop/example.rs", "subspace-desktop/README.md"), [
            ("subspace-desktop", ("mise", "run", "check")),
            ("subspace-desktop", ("mise", "run", "quality")),
        ])

    def test_framework_manifest(self):
        self.assertEqual(self.checks("subspace-framework/Cargo.toml"), [
            ("subspace-framework", ("mise", "run", "check")),
        ])

    def test_hook_only(self):
        self.assertEqual(self.checks("prek.toml", "scripts/check-push.py",
                                     "subspace-framework/prek.toml", "subspace-desktop/prek.toml"), [])

    def test_pushed_diff_keeps_deleted_paths_and_both_sides_of_rename(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}

            def git(*args):
                return subprocess.check_output(
                    ["git", "-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                     "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", *args],
                    cwd=root, env=env, text=True,
                ).strip()

            git("init", "-q")
            deleted = root / "subspace-api" / "tests" / "removed.ts"
            moved = root / "subspace-framework" / "guide.md"
            destination = root / "subspace-desktop" / "guide.md"
            for file in (deleted, moved, destination):
                file.parent.mkdir(parents=True, exist_ok=True)
            deleted.write_text("export const removed = true;\n")
            moved.write_text("# Guide\n")
            git("add", ".")
            git("commit", "-qm", "Before")
            before = git("rev-parse", "HEAD")
            deleted.unlink()
            moved.rename(destination)
            git("add", "-A")
            git("commit", "-qm", "After")
            after = git("rev-parse", "HEAD")
            env.update(PRE_COMMIT_FROM_REF=before, PRE_COMMIT_TO_REF=after)
            with patch.dict(HOOK["changed_paths"].__globals__, {"ROOT": root}), patch.dict(
                os.environ, env, clear=True
            ):
                paths = HOOK["changed_paths"]()
            self.assertCountEqual(paths, [
                "subspace-api/tests/removed.ts", "subspace-framework/guide.md",
                "subspace-desktop/guide.md",
            ])


if __name__ == "__main__":
    unittest.main()
