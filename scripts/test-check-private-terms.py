"""Verify the private-terms check in throwaway repositories, with invented terms."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).with_name("check-private-terms.py")
ROOT = SCRIPT.parents[1]
TERMS = "# invented\nzorblat\n\n\\bquux\\b\n[a-z.]+@corp\\.example\n"
IDENT = {
    "GIT_AUTHOR_NAME": "Ada", "GIT_AUTHOR_EMAIL": "ada@users.example",
    "GIT_COMMITTER_NAME": "Ada", "GIT_COMMITTER_EMAIL": "ada@users.example",
}


class PrivateTerms(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.repo = Path(folder.name)
        self.env = {
            "PATH": os.environ["PATH"], "HOME": f"{folder.name}/.git/home", "XDG_CONFIG_HOME": f"{folder.name}/.git/xdg",
            "GIT_CONFIG_NOSYSTEM": "1", "SUBSPACE_PRIVATE_TERMS": TERMS, **IDENT,
        }
        self.git("init", "--quiet", "--initial-branch=main")
        self.commit("first", {"README.md": "hello\n"})

    def git(self, *args, env=None):
        run = subprocess.run(["git", *args], cwd=self.repo, env={**self.env, **(env or {})},
                             check=True, capture_output=True, text=True)
        return run.stdout.strip()

    def write(self, files):
        for name, text in files.items():
            path = self.repo / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(text, bytes):
                path.write_bytes(text)
            else:
                path.write_text(text)
        self.git("add", "--all")

    def commit(self, message, files, env=None):
        self.write(files)
        self.git("commit", "--quiet", "--allow-empty", "-m", message, env=env)
        return self.git("rev-parse", "HEAD")

    def check(self, *args, env=None, stdin=None):
        run = subprocess.run([sys.executable, "-B", str(SCRIPT), *args], cwd=self.repo,
                             env={**self.env, **(env or {})}, input=stdin, capture_output=True, text=True)
        return run.returncode, run.stdout + run.stderr

    def test_a_clean_range_passes(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add notes", {"notes.txt": "quuxes and a zorb\n"})
        self.assertEqual(self.check("--range", base, head), (0, ""))

    def test_an_added_line_is_found_by_place_and_term_number_only(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add notes", {"notes.txt": "one\ntwo Zorblat\nthree\n"})
        code, output = self.check("--range", base, head)
        self.assertEqual(code, 1)
        self.assertIn(f"private term 1: commit {head[:9]}: notes.txt:2\n", output)
        self.assertIn("1 finding(s)", output)
        self.assertNotIn("orblat", output.lower())

    def test_a_term_removed_by_a_later_commit_is_still_found(self):
        base = self.git("rev-parse", "HEAD")
        added = self.commit("add", {"notes.txt": "quux\n"})
        head = self.commit("remove", {"notes.txt": "fine\n"})
        code, output = self.check("--range", base, head)
        self.assertEqual(code, 1)
        self.assertIn(f"private term 2: commit {added[:9]}: notes.txt:1\n", output)

    def test_lines_and_paths_that_were_already_there_are_not_read(self):
        base = self.commit("before the check", {"quux/old.txt": "zorblat\nkeep\n"})
        head = self.commit("edit", {"quux/old.txt": "zorblat\nkept\n"})
        self.assertEqual(self.check("--range", base, head)[0], 0)

    def test_the_message_author_committer_and_path_are_read(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("fix\n\nCo-authored-by: B <b.b@corp.example>", {"zorblat/a.txt": "a\n"},
                           env={"GIT_AUTHOR_EMAIL": "ada@corp.example", "GIT_COMMITTER_NAME": "quux"})
        code, output = self.check("--range", base, head)
        self.assertEqual(code, 1)
        short = head[:9]
        for line in (f"private term 3: commit {short}: the author's address\n",
                     f"private term 2: commit {short}: the committer's name\n",
                     f"private term 3: commit {short}: the message: line 3\n",
                     f"private term 1: commit {short}: the path zorblat/a.txt\n"):
            self.assertIn(line, output)
        self.assertIn("4 finding(s)", output)

    def test_a_term_anchored_to_a_whole_name_or_address_matches_it(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add", {"a.txt": "a\n"}, env={"GIT_AUTHOR_NAME": "Zed", "GIT_COMMITTER_EMAIL": "z@z.example"})
        env = {"SUBSPACE_PRIVATE_TERMS": "^zed$\n^z@z\\.example$\n"}
        code, output = self.check("--range", base, head, env=env)
        self.assertEqual(code, 1)
        self.assertIn(f"private term 1: commit {head[:9]}: the author's name\n", output)
        self.assertIn(f"private term 2: commit {head[:9]}: the committer's address\n", output)
        self.write({"b.txt": "b\n"})
        code, output = self.check("--staged", env={**env, "GIT_AUTHOR_NAME": "Zed", "GIT_COMMITTER_EMAIL": "z@z.example"})
        self.assertEqual(code, 1)
        self.assertIn("private term 1: your git author name\n", output)
        self.assertIn("private term 2: your git committer e-mail address\n", output)

    def test_the_path_of_an_added_empty_or_binary_file_is_read(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add", {"zorblat.bin": b"\0\1", "quux/empty.txt": "", "caf\u00e9 quux.txt": "a\n"})
        code, output = self.check("--range", base, head)
        self.assertEqual(code, 1)
        for path in ("zorblat.bin", "quux/empty.txt", "caf\u00e9 quux.txt"):
            self.assertIn(f"commit {head[:9]}: the path {path}\n", output)
        self.write({"zorblat-2.bin": b"\0\2", "quux/empty-2.txt": ""})
        code, output = self.check("--staged")
        self.assertEqual(code, 1)
        self.assertIn("private term 1: the path zorblat-2.bin\n", output)
        self.assertIn("private term 2: the path quux/empty-2.txt\n", output)

    def test_text_after_a_separator_other_than_a_line_feed_is_read(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add", {"notes.txt": "one\ntwo\x0czorblat\nthree\u2028quux\rmore\nfour\n"})
        code, output = self.check("--range", base, head)
        self.assertEqual(code, 1)
        self.assertIn(f"private term 1: commit {head[:9]}: notes.txt:2\n", output)
        self.assertIn(f"private term 2: commit {head[:9]}: notes.txt:3\n", output)

    def test_a_term_anchored_to_the_end_of_a_line_matches_before_a_carriage_return(self):
        base = self.git("rev-parse", "HEAD")
        self.git("config", "core.autocrlf", "false")
        head = self.commit("add", {"dos.txt": b"one\r\ntwo zorblat\r\n"})
        code, output = self.check("--range", base, head, env={"SUBSPACE_PRIVATE_TERMS": "zorblat$\n"})
        self.assertEqual(code, 1)
        self.assertIn(f"private term 1: commit {head[:9]}: dos.txt:2\n", output)
        code, _ = self.check("--text", "text", env={"SUBSPACE_PRIVATE_TERMS": "^zorblat$\n"}, stdin="a\r\nzorblat\r\n")
        self.assertEqual(code, 1)

    def test_a_rename_is_read_as_an_added_path(self):
        base = self.commit("add", {"plain.txt": "one\ntwo\nthree\n"})
        self.git("mv", "plain.txt", "quux.txt")
        head = self.commit("rename", {})
        code, output = self.check("--range", base, head)
        self.assertEqual(code, 1)
        self.assertIn("the path quux.txt\n", output)

    def test_an_added_line_that_looks_like_a_file_header_is_read_as_text(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add", {"notes.txt": "++ b/elsewhere\nzorblat\n"})
        code, output = self.check("--range", base, head)
        self.assertEqual(code, 1)
        self.assertIn(f"commit {head[:9]}: notes.txt:2\n", output)

    def test_binary_files_are_not_read(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add image", {"image.bin": b"\0\1 zorblat \0"})
        self.assertEqual(self.check("--range", base, head)[0], 0)

    def test_a_merge_is_read_for_what_it_brings_in(self):
        base = self.git("rev-parse", "HEAD")
        self.git("checkout", "--quiet", "-b", "side")
        side = self.commit("side", {"side.txt": "quux\n"})
        self.git("checkout", "--quiet", "main")
        moved = self.commit("main moves", {"main.txt": "zorblat\n"})
        # Whatever the developer's Git shows for a merge by default.
        self.git("config", "log.diffMerges", "dense-combined")
        self.git("merge", "--quiet", "--no-ff", "-m", "merge side", "side")
        merge = self.git("rev-parse", "HEAD")
        code, output = self.check("--range", base, merge)
        self.assertEqual(code, 1)
        found = [line for line in output.splitlines() if line.startswith("private term ")]
        self.assertEqual(sorted(found), sorted([
            f"private term 2: commit {side[:9]}: side.txt:1",
            f"private term 1: commit {moved[:9]}: main.txt:1",
            f"private term 2: commit {merge[:9]}: side.txt:1",
        ]))

    def test_staged_changes_and_the_identity_are_read(self):
        self.write({"notes.txt": "fine\nquux\n"})
        code, output = self.check("--staged", env={"GIT_AUTHOR_EMAIL": "ada@corp.example"})
        self.assertEqual(code, 1)
        self.assertIn("private term 2: notes.txt:2\n", output)
        self.assertIn("private term 3: your git author e-mail address\n", output)
        self.assertNotIn("committer", output)

    def test_a_push_reads_the_pushed_commits_and_the_branch_name(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add", {"notes.txt": "zorblat\n"})
        refs = {"PRE_COMMIT_FROM_REF": base, "PRE_COMMIT_TO_REF": head}
        code, output = self.check("--push", env={**refs, "PRE_COMMIT_REMOTE_BRANCH": "refs/heads/fix/quux"})
        self.assertEqual(code, 1)
        self.assertIn(f"private term 1: commit {head[:9]}: notes.txt:1\n", output)
        self.assertIn("private term 2: the name of the branch\n", output)
        self.assertEqual(self.check("--push", env={"PRE_COMMIT_FROM_REF": head, "PRE_COMMIT_TO_REF": head})[0], 0)
        # The short name, as the pull request check reads it.
        named = {"PRE_COMMIT_FROM_REF": head, "PRE_COMMIT_TO_REF": head, "SUBSPACE_PRIVATE_TERMS": "^fix/quux$\n",
                 "PRE_COMMIT_REMOTE_BRANCH": "refs/heads/fix/quux"}
        self.assertEqual(self.check("--push", env=named)[0], 1)

    def test_a_push_with_no_base_reads_the_whole_history_of_the_pushed_branch(self):
        self.git("checkout", "--quiet", "-b", "side")
        side = self.commit("add", {"notes.txt": "zorblat\n"})
        fixed = self.commit("remove", {"notes.txt": "fine\n"})
        self.git("checkout", "--quiet", "main")
        # Another branch than the one checked out, and tracking refs that say the remote has the commits.
        self.git("update-ref", "refs/remotes/origin/side", fixed)
        code, output = self.check("--push", env={"PRE_COMMIT_TO_REF": fixed, "PRE_COMMIT_REMOTE_NAME": "origin"})
        self.assertEqual(code, 1)
        self.assertIn(f"commit {side[:9]}: notes.txt:1\n", output)

    def test_a_run_by_hand_reads_what_no_remote_has(self):
        self.git("update-ref", "refs/remotes/origin/main", self.git("rev-parse", "HEAD"))
        self.assertEqual(self.check("--push")[0], 0)
        head = self.commit("add", {"notes.txt": "zorblat\n"})
        code, output = self.check("--push")
        self.assertEqual(code, 1)
        self.assertIn(f"commit {head[:9]}: notes.txt:1\n", output)
        self.git("update-ref", "refs/remotes/origin/main", head)
        self.assertEqual(self.check("--push")[0], 0)

    def test_text_on_standard_input(self):
        code, output = self.check("--text", "the description", stdin="Title\n\nSee quux.\n")
        self.assertEqual((code, output.splitlines()[0]), (1, "private term 2: the description: line 3"))
        self.assertEqual(self.check("--text", "the description", stdin="Title\n")[0], 0)

    def test_no_places_says_only_how_many(self):
        base = self.git("rev-parse", "HEAD")
        head = self.commit("add zorblat", {"notes.txt": "quux\n"})
        code, output = self.check("--range", base, head, "--no-places")
        self.assertEqual(code, 1)
        self.assertTrue(output.startswith("private terms: 2 finding(s). This log does not say where."))
        for text in ("notes.txt", head[:9], "private term "):
            self.assertNotIn(text, output)

    def test_the_list_comes_from_a_file_when_the_variable_is_unset(self):
        self.write({"notes.txt": "zorblat\n"})
        del self.env["SUBSPACE_PRIVATE_TERMS"]
        self.assertEqual(self.check("--staged")[0], 0)
        named = self.repo / ".git/terms"
        named.write_text("zorblat\n")
        self.assertEqual(self.check("--staged", env={"SUBSPACE_PRIVATE_TERMS_FILE": str(named)})[0], 1)
        default = Path(self.env["HOME"]) / ".config/subspace-private-terms"
        default.parent.mkdir(parents=True)
        default.write_text("zorblat\n")
        self.assertEqual(self.check("--staged")[0], 1)

    def test_without_a_list_it_passes_unless_one_is_required(self):
        self.write({"notes.txt": "zorblat\n"})
        empty = {"SUBSPACE_PRIVATE_TERMS": "# nothing\n\n"}
        self.assertEqual(self.check("--staged", env=empty),
                         (0, "private terms: no list configured, nothing checked\n"))
        code, output = self.check("--staged", "--require-terms", env=empty)
        self.assertEqual((code, output), (2, "private terms: no list of terms is configured\n"))

    def test_a_commit_git_cannot_read_is_an_error_not_a_pass(self):
        code, output = self.check("--range", "HEAD", "0" * 40)
        self.assertEqual(code, 2)
        self.assertTrue(output.startswith("private terms: git failed, the check is incomplete: fatal:"), output)
        code, output = self.check("--range", "HEAD", "0" * 40, "--no-places")
        self.assertEqual((code, output), (2, "private terms: git failed, the check is incomplete\n"))

    def test_an_invalid_term_is_reported_without_its_text(self):
        code, output = self.check("--staged", env={"SUBSPACE_PRIVATE_TERMS": "fine\n(?P<zorb>a)(?P=blat)\n"})
        self.assertEqual((code, output), (2, "private terms: term 2 is not a valid regular expression\n"))


class Wiring(unittest.TestCase):
    def test_the_hooks_run_on_every_commit_and_push(self):
        hooks = (ROOT / "prek.toml").read_text()
        for entry, stage in (("python3 scripts/check-private-terms.py --staged", "pre-commit"),
                             ("python3 scripts/check-private-terms.py --push", "pre-push")):
            line = next(line for line in hooks.splitlines() if f'entry = "{entry}"' in line)
            self.assertIn(f'stages = ["{stage}"]', line)
            self.assertIn("always_run = true", line)
            self.assertIn("pass_filenames = false", line)

    def test_the_workflow_never_runs_the_pull_request_s_code(self):
        workflow = (ROOT / ".github/workflows/private-terms.yml").read_text()
        for line in ("  pull_request_target:\n", "permissions: {}\n", "      contents: read\n",
                     "          persist-credentials: false\n",
                     "          SUBSPACE_PRIVATE_TERMS: ${{ secrets.PRIVATE_TERMS }}\n",
                     '--range "$BASE_SHA" "$HEAD_SHA"\n'):
            self.assertIn(line, workflow)
        steps = workflow.split("\n    steps:\n", 1)[1]
        self.assertEqual(steps.count("--require-terms --no-places"), steps.count("check-private-terms.py"))
        # The base branch's script; the pull request only as fetched objects and environment text.
        # Nor its title, description or branch name in `env`, which the log prints.
        for text in ("write", "continue-on-error", " ref:", "git checkout", "git switch", "git merge",
                     "pull_request.title", "pull_request.body", "pull_request.head.ref", "pull_request.head.label"):
            self.assertNotIn(text, steps)
        self.assertIn("""jq --raw-output '.pull_request | .head.ref, .title, .body // ""' "$GITHUB_EVENT_PATH" |""",
                      steps)
        for line in steps.splitlines():
            if "${{" in line and "!cancelled()" not in line:
                self.assertRegex(line, r"^          [A-Z_]+: \$\{\{ [a-z_.A-Z]+ \}\}$")


if __name__ == "__main__":
    unittest.main()
