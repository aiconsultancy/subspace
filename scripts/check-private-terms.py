"""Fail when a change holds a private term.

The terms are regular expressions, one per line, matched without regard to
case. They are private themselves, so they never live in this repository:
locally they come from `SUBSPACE_PRIVATE_TERMS` (the list itself),
`SUBSPACE_PRIVATE_TERMS_FILE`, or `~/.config/subspace-private-terms`; in
GitHub Actions from the `PRIVATE_TERMS` secret. Blank lines and lines that
start with `#` are ignored. Without a list the check passes and says so,
unless `--require-terms` is given.

A finding names the place and the number of the term (counting the list's
terms from 1), never the text that matched. `--no-places` prints only how
many there are and no detail of an error, for a log that anyone can read.

    check-private-terms.py --staged             what `git commit` would record
    check-private-terms.py --push               the commits a push would send
    check-private-terms.py --range BASE HEAD    the commits in BASE..HEAD
    check-private-terms.py --text LABEL         text on standard input

Only added lines, the paths of added files, commit messages, and author and
committer names and addresses are read. The content of a file that Git
treats as binary is not, nor is the message of an annotated tag.

prek limits what `--push` sees: of several branches pushed at once it checks
only the first with new commits, and it runs no hook for a branch whose
commits the remote already has, so that branch's name is not read. The pull
request check reads both, after they are on GitHub.
"""

import argparse
import os
from pathlib import Path
import re
import subprocess
import sys


DIFF = ("--no-ext-diff", "--no-textconv", "--no-color", "--no-renames")
LINES = (*DIFF, "--unified=0")
# A rename counts as an added file, because renames are not paired.
PATHS = (*DIFF, "--name-only", "-z", "--diff-filter=A")
HUNK = re.compile(r"^@@ -\d+(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")


class TermsError(Exception):
    pass


def terms_text():
    text = os.environ.get("SUBSPACE_PRIVATE_TERMS")
    if text is not None:
        return text
    file = Path(os.environ.get("SUBSPACE_PRIVATE_TERMS_FILE") or Path.home() / ".config/subspace-private-terms")
    return file.read_text(encoding="utf-8") if file.is_file() else ""


def compile_terms(text):
    terms = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        number = len(terms) + 1
        try:
            terms.append((number, re.compile(line, re.IGNORECASE)))
        except re.error as error:
            # Neither the expression nor the error's text, which can quote it: it is private.
            raise TermsError(f"term {number} is not a valid regular expression") from None
    return terms


def git(*args):
    run = subprocess.run(
        ["git", "-c", "core.quotePath=false", *args], check=True, capture_output=True
    )
    return run.stdout.decode("utf-8", errors="replace")


def lines(text):
    """Git ends a line with a line feed, or a carriage return and one; other separators stay inside it."""
    return [line.removesuffix("\r") for line in text.split("\n")]


def matches(terms, text):
    return [number for number, term in terms if term.search(text)]


def added_lines(diff):
    """Each added line of a unified diff with no context, as (path, line number, text)."""
    path, number, removed, added = None, 0, 0, 0
    for line in lines(diff):
        if removed and line.startswith("-"):
            removed -= 1
        elif added and line.startswith("+"):
            added -= 1
            yield path, number, line[1:]
            number += 1
        elif line.startswith("\\"):
            continue
        elif line.startswith("+++ "):
            name = line[4:].strip('"')
            path = None if name == "/dev/null" else name.removeprefix("b/")
        elif match := HUNK.match(line):
            removed = int(match.group(1) or 1)
            number = int(match.group(2))
            added = int(match.group(3) or 1) if path else 0


def diff_findings(terms, paths, diff, where=""):
    for path in filter(None, paths.split("\0")):
        for term in matches(terms, path):
            yield term, f"{where}the path {path}"
    for path, number, text in added_lines(diff):
        for term in matches(terms, text):
            yield term, f"{where}{path}:{number}"


def staged_findings(terms):
    yield from diff_findings(terms, git("diff", "--cached", *PATHS), git("diff", "--cached", *LINES))
    for name, role in (("GIT_AUTHOR_IDENT", "author"), ("GIT_COMMITTER_IDENT", "committer")):
        # "Name <address> time zone"
        ident = git("var", name).rsplit(">", 1)[0].rsplit(" <", 1)
        for field, value in zip(("name", "e-mail address"), ident):
            for term in matches(terms, value):
                yield term, f"your git {role} {field}"


def commit_findings(terms, commit):
    short = commit[:9]
    fields = git("show", "--no-patch", "--format=%an%x00%ae%x00%cn%x00%ce%x00%B", commit).split("\0")
    labels = ("author's name", "author's address", "committer's name", "committer's address")
    for label, value in zip(labels, fields):
        for term in matches(terms, value):
            yield term, f"commit {short}: the {label}"
    yield from text_findings(terms, f"commit {short}: the message", fields[4])
    # Against the first parent, so a merge is read for what it brings in.
    show = ("show", "--format=", "--diff-merges=first-parent")
    yield from diff_findings(terms, git(*show, *PATHS, commit), git(*show, *LINES, commit), f"commit {short}: ")


def range_findings(terms, commits):
    for commit in commits:
        yield from commit_findings(terms, commit)


def push_findings(terms):
    before = os.environ.get("PRE_COMMIT_FROM_REF")
    tip = os.environ.get("PRE_COMMIT_TO_REF")
    if before and tip:
        commits = git("rev-list", "--reverse", f"{before}..{tip}")
    elif tip:
        # prek found no commit of this history on the remote: all of it is new there.
        commits = git("rev-list", "--reverse", tip)
    else:
        # A run by hand: what no remote has yet.
        commits = git("rev-list", "--reverse", "HEAD", "--not", "--remotes")
    yield from range_findings(terms, commits.split())
    for term in matches(terms, os.environ.get("PRE_COMMIT_REMOTE_BRANCH", "").removeprefix("refs/heads/")):
        yield term, "the name of the branch"


def text_findings(terms, label, text):
    for number, line in enumerate(lines(text), 1):
        for term in matches(terms, line):
            yield term, f"{label}: line {number}"


def findings(terms, args):
    if args.staged:
        return staged_findings(terms)
    if args.push:
        return push_findings(terms)
    if args.range:
        base, head = args.range
        return range_findings(terms, git("rev-list", "--reverse", f"{base}..{head}").split())
    return text_findings(terms, args.text, sys.stdin.read())


def report(found, places):
    if places:
        for term, place in found:
            print(f"private term {term}: {place}", file=sys.stderr)
    advice = (
        "Remove them, and rewrite the commits that hold them, before this is pushed or merged."
        if places else
        "This log does not say where. Someone who holds the list can run "
        "scripts/check-private-terms.py to find them."
    )
    print(f"private terms: {len(found)} finding(s). {advice}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--staged", action="store_true")
    mode.add_argument("--push", action="store_true")
    mode.add_argument("--range", nargs=2, metavar=("BASE", "HEAD"))
    mode.add_argument("--text", metavar="LABEL")
    parser.add_argument("--require-terms", action="store_true")
    parser.add_argument("--no-places", action="store_true")
    args = parser.parse_args()

    try:
        terms = compile_terms(terms_text())
    except TermsError as error:
        print(f"private terms: {error}", file=sys.stderr)
        return 2
    if not terms:
        if args.require_terms:
            print("private terms: no list of terms is configured", file=sys.stderr)
            return 2
        print("private terms: no list configured, nothing checked")
        return 0

    try:
        found = list(dict.fromkeys(findings(terms, args)))
    except subprocess.CalledProcessError as error:
        detail = "" if args.no_places else ": " + error.stderr.decode("utf-8", errors="replace").strip()
        print(f"private terms: git failed, the check is incomplete{detail}", file=sys.stderr)
        return 2
    if found:
        report(found, not args.no_places)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
