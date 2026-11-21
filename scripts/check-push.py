"""Run the checks for the projects changed by a push."""

import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
RUST_PROJECTS = ("subspace-framework", "subspace-desktop")


def changed_paths():
    before = os.environ.get("PRE_COMMIT_FROM_REF")
    after = os.environ.get("PRE_COMMIT_TO_REF")
    if not before or not after:
        # Prek supplies paths for manual runs and pushes without a base.
        return sys.argv[1:]
    # Prek excludes deleted paths. Keep both sides of a rename so moving
    # code across projects checks both of them.
    paths = subprocess.check_output(
        ["git", "diff", "--no-ext-diff", "--no-renames", "--name-only", "-z", before, after, "--"],
        cwd=ROOT,
    )
    return os.fsdecode(paths).split("\0")[:-1]


def project_checks(paths):
    files = [PurePosixPath(path) for path in paths]
    for project in RUST_PROJECTS:
        changed = [file for file in files if file.parts[0] == project]
        code = [
            file for file in changed
            if file.suffix.lower() != ".md" and file != PurePosixPath(project, "prek.toml")
        ]
        if code:
            yield project, ("mise", "run", "check")
            if any(
                file.suffix == ".rs"
                or file in {PurePosixPath(project, "rustqual.toml"),
                            PurePosixPath(project, "rustqual-baseline.json")}
                for file in code
            ):
                yield project, ("mise", "run", "quality")
        elif any(file.suffix.lower() == ".md" for file in changed):
            yield project, ("mise", "run", "--skip-tools", "board-docs", ":::", "docs-index")

    if any(file.parts[0] == "subspace-api" and file.suffix.lower() != ".md" for file in files):
        package = json.loads((ROOT / "subspace-api" / "package.json").read_text(encoding="utf-8"))
        for args in (("run", "typecheck"), ("run", "lint"), ("test",)):
            yield "subspace-api", ("mise", "exec", package["packageManager"], "--", "bun", *args)


def main():
    for project, command in project_checks(changed_paths()):
        print(f"{project}: {' '.join(command)}", flush=True)
        subprocess.run(command, cwd=ROOT / project, check=True)


if __name__ == "__main__":
    main()
