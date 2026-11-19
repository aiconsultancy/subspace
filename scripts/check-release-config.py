"""Check that Release Please's configuration matches the manifests it edits.

Usage: python3 scripts/check-release-config.py

For each package in release-please-config.json: its version in
.release-please-manifest.json is its Cargo workspace version, every TOML
`extra-files` path resolves to that version, and every block between
`x-release-please-start-version` and `x-release-please-end` holds only exact
internal pins on it. Generic annotations may only sit inside the package's own
folder: Release Please applies every annotation in a file to each package that
lists the file, so an annotation in a file two packages share would take the
wrong version.
"""

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEGMENT = re.compile(r"\.([A-Za-z0-9_-]+)|\['([^']+)'\]")
PIN = re.compile(r'^[a-z0-9-]+ = \{ path = "[^"]+", version = "=(?P<version>[^"]+)" \}$')
ANNOTATION = re.compile(r"x-release-please-")


def resolve(data: dict, jsonpath: str):
    """The value at a `$.a['b'].c` path."""
    if not jsonpath.startswith("$"):
        raise ValueError(f"not a JSONPath: {jsonpath}")
    rest = jsonpath[1:]
    position = 0
    value = data
    while position < len(rest):
        segment = SEGMENT.match(rest, position)
        if segment is None:
            raise ValueError(f"unsupported JSONPath: {jsonpath}")
        value = value[segment.group(1) or segment.group(2)]
        position = segment.end()
    return value


def pinned_blocks(text: str) -> list[list[str]]:
    """The lines inside each version block."""
    blocks: list[list[str]] = []
    current: list[str] | None = None
    for line in text.splitlines():
        if "x-release-please-start-version" in line:
            current = []
        elif "x-release-please-end" in line:
            if current is not None:
                blocks.append(current)
            current = None
        elif current is not None:
            current.append(line)
    return blocks


def extra_file_errors(root: Path, package: str, version: str, extra: dict) -> list[str]:
    path = extra["path"]
    file = root / path.lstrip("/") if path.startswith("/") else root / package / path
    name = file.relative_to(root)
    if not file.is_file():
        return [f"{package}: {name} does not exist"]
    if extra["type"] == "toml":
        found = resolve(tomllib.loads(file.read_text()), extra["jsonpath"])
        if found != version:
            return [f"{package}: {name} {extra['jsonpath']} is {found}, not {version}"]
        return []
    if extra["type"] != "generic":
        return [f"{package}: {name} uses an unchecked type {extra['type']}"]
    if not file.is_relative_to(root / package):
        return [f"{package}: generic annotations in {name}, outside the package"]
    blocks = pinned_blocks(file.read_text())
    lines = [line for block in blocks for line in block if line.strip() and not line.startswith("#")]
    if not lines:
        return [f"{package}: no version block in {name}"]
    return [f"{package}: {name} block line is not a pin on ={version}: {line}"
            for line in lines if (pin := PIN.match(line)) is None or pin["version"] != version]


def errors(root: Path, manifests: list[Path]) -> list[str]:
    """Every problem; `manifests` are the Cargo.toml files to search for annotations."""
    config = json.loads((root / "release-please-config.json").read_text())
    manifest = json.loads((root / ".release-please-manifest.json").read_text())
    found: list[str] = []
    if set(config["packages"]) != set(manifest):
        found.append("the config and the manifest list different packages")
    for key, expected in (("separate-pull-requests", True), ("bump-minor-pre-major", True),
                          ("bump-patch-for-minor-pre-major", False)):
        if config.get(key) is not expected:
            found.append(f"{key} must be {str(expected).lower()} (docs/ways-of-working.md)")
    for package, options in config["packages"].items():
        version = manifest.get(package, "")
        cargo = tomllib.loads((root / package / "Cargo.toml").read_text())
        if cargo["workspace"]["package"]["version"] != version:
            found.append(f"{package}: Cargo.toml is not at the manifest's {version}")
        for extra in options.get("extra-files", []):
            found += extra_file_errors(root, package, version, extra)
    for file in manifests:
        if not ANNOTATION.search(file.read_text()):
            continue
        owners = [package for package in config["packages"] if file.is_relative_to(root / package)]
        generic = [extra for package in owners for extra in config["packages"][package].get("extra-files", [])
                   if extra["type"] == "generic" and root / package / extra["path"] == file]
        if not generic:
            found.append(f"{file.relative_to(root)} has annotations no package's generic extra file reads")
    return found


def main() -> int:
    tracked = subprocess.check_output(["git", "-C", str(ROOT), "ls-files", "*Cargo.toml"], text=True)
    found = errors(ROOT, [ROOT / name for name in tracked.splitlines()])
    for error in found:
        print(error, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
