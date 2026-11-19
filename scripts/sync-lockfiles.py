"""Set each local package's version in the repository's lockfiles to its manifest's.

Usage: python3 scripts/sync-lockfiles.py [--check]

A release pull request from Release Please changes package versions in the
Cargo manifests but cannot run Cargo, so the four lockfiles still name the old
versions. This rewrites only the `version` of lockfile entries without a
`source` (packages built from a path) whose name a workspace here declares,
reading the names and versions with `cargo metadata --no-deps --offline`, which
needs neither the network nor a registry cache. Every other byte is kept, so
the result is what Cargo would write. With --check it changes nothing and
fails when a lockfile is out of date.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WORKSPACES = (
    "subspace-framework",
    "subspace-desktop",
    "subspace-desktop/terminal/panel",
    "subspace-desktop/terminal/runtime",
)
# Cargo writes name, version, then source for a registry or git package.
LOCAL_ENTRY = re.compile(
    r'^\[\[package\]\]\nname = "(?P<name>[^"]+)"\nversion = "(?P<version>[^"]+)"$(?!\nsource = )',
    re.M,
)


def local_versions(root: Path) -> dict[str, str]:
    """Every workspace member's name and version, across the four workspaces."""
    versions: dict[str, str] = {}
    for workspace in WORKSPACES:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--offline", "--format-version", "1",
             "--manifest-path", str(root / workspace / "Cargo.toml")],
            text=True,
        ))
        for package in metadata["packages"]:
            name, version = package["name"], package["version"]
            if versions.setdefault(name, version) != version:
                raise ValueError(f"{name} has two versions: {versions[name]} and {version}")
    return versions


def sync(text: str, versions: dict[str, str]) -> tuple[str, list[str]]:
    """The lockfile with local packages at their manifest versions, and what moved."""
    changes: list[str] = []

    def replace(entry: re.Match) -> str:
        name, old = entry["name"], entry["version"]
        new = versions.get(name, old)
        if new == old:
            return entry[0]
        changes.append(f"{name} {old} -> {new}")
        return f'[[package]]\nname = "{name}"\nversion = "{new}"'

    return LOCAL_ENTRY.sub(replace, text), changes


def main(arguments: list[str]) -> int:
    if arguments not in ([], ["--check"]):
        print(__doc__.split("\n\n")[1], file=sys.stderr)
        return 2
    check = arguments == ["--check"]
    versions = local_versions(ROOT)
    stale = False
    for workspace in WORKSPACES:
        lockfile = ROOT / workspace / "Cargo.lock"
        text, changes = sync(lockfile.read_text(), versions)
        for change in changes:
            print(f"{lockfile.relative_to(ROOT)}: {change}")
        if changes and not check:
            lockfile.write_text(text)
        stale = stale or bool(changes)
    if check and stale:
        print("lockfiles are out of date; run python3 scripts/sync-lockfiles.py", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
