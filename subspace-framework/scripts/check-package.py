"""Check what each published crate ships: metadata, licence, notices, files.

Lists every workspace package with `cargo package --list` (no build) and
checks:

- the manifest says it is published, with a description and the licence;
- the package ships `LICENSE`, identical to this folder's Apache-2.0 text;
- every embedded third-party file (THIRD_PARTY) ships with its licence text
  and is named in the crate's `NOTICE`, which ships too;
- a packaged file that says it is adapted, ported or copyrighted by someone
  else is in THIRD_PARTY, so a new third-party file cannot ship unnoticed;
- no build output, editor or OS litter, and no file over MAX_FILE_BYTES;
- each `.crate` under `target/package` (when present) is under crates.io's
  10 MB limit.
"""

import fnmatch
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAX_FILE_BYTES = 1024 * 1024
MAX_CRATE_BYTES = 10 * 1000 * 1000
FORBIDDEN = ("target/*", "*.crate", ".DS_Store", "*/.DS_Store", "*.orig.rs", ".env", "*.pem", "*.p12")
MARKER = re.compile(r"(?<![A-Za-z-])(?:[Aa]dapted|[Pp]orted) from|Copyright \(c\)")
OWN = ("LICENSE", "NOTICE", "*/LICENSE-*", "LICENSE-*", "*/PROVENANCE.md", "*/README.md",
       "Cargo.toml", "Cargo.toml.orig", "*/LICENSE")
# package -> {packaged file or glob: licence text it needs in the package}
THIRD_PARTY = {
    "subspace-charts": {
        "vendor/flint-chart-0.5.1/chartjs.js": "vendor/flint-chart-0.5.1/LICENSE",
        "src/guide.md": "vendor/flint-chart-0.5.1/LICENSE",
        "src/lib.rs": "vendor/flint-chart-0.5.1/LICENSE",
    },
    "subspace-shell": {
        "themes/gpui-kit/*.json": "themes/gpui-kit/LICENSE-APACHE",
        "themes/macos-classic-dark-remix.json": "themes/LICENSE-MACOS-CLASSIC",
        "themes/twilight-two.json": "themes/gpui-kit/LICENSE-APACHE",
    },
    "subspace-chat": {
        "assets/copilot.svg": "assets/LICENSE-OCTICONS",
        "assets/openai.svg": "assets/LICENSE-LOBE-ICONS",
        "assets/claude.svg": "assets/PROVENANCE.md",
        "assets/opencode.svg": "assets/PROVENANCE.md",
        "assets/gemini.svg": "assets/PROVENANCE.md",
    },
    "subspace-lucide": {
        "build.rs": "LICENSE",
        "src/lib.rs": "LICENSE-LUCIDE",
    },
    "subspace-component-gallery-board": {
        "assets/component.svg": "assets/LICENSE-LUCIDE",
    },
}


def metadata() -> list[dict]:
    output = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"],
        cwd=ROOT, text=True)
    return json.loads(output)["packages"]


def packaged(name: str) -> list[str]:
    output = subprocess.check_output(
        ["cargo", "package", "-p", name, "--list", "--allow-dirty", "--locked"],
        cwd=ROOT, text=True, stderr=subprocess.DEVNULL)
    return [line for line in output.splitlines() if line]


def matches(path: str, patterns) -> bool:
    return any(fnmatch.fnmatch(path, pattern) for pattern in patterns)


def manifest_errors(package: dict) -> list[str]:
    errors = []
    if package.get("publish") == []:
        errors.append("publish = false")
    if not package.get("description"):
        errors.append("no description")
    if package.get("license") != "Apache-2.0":
        errors.append("licence is not Apache-2.0")
    return errors


def licence_errors(directory: Path, files: list[str], licence: str) -> list[str]:
    if "LICENSE" not in files:
        return ["LICENSE is not packaged"]
    if (directory / "LICENSE").read_text() != licence:
        return ["LICENSE differs from the Framework's LICENSE"]
    return []


def notice_errors(name: str, directory: Path, files: list[str]) -> list[str]:
    entries = THIRD_PARTY.get(name, {})
    if not entries:
        return ["NOTICE ships but THIRD_PARTY lists nothing"] if "NOTICE" in files else []
    if "NOTICE" not in files:
        return ["NOTICE is not packaged"]
    notice = (directory / "NOTICE").read_text()
    errors = []
    for pattern, licence in entries.items():
        if not any(fnmatch.fnmatch(path, pattern) for path in files):
            errors.append(f"third-party file {pattern} is not packaged")
        if licence not in files:
            errors.append(f"{pattern}: its licence {licence} is not packaged")
        if pattern not in notice:
            errors.append(f"{pattern} is not named in NOTICE")
    return errors


def unlisted_third_party(name: str, directory: Path, files: list[str]) -> list[str]:
    """Files that credit someone else but are not in THIRD_PARTY."""
    listed = THIRD_PARTY.get(name, {})
    errors = []
    for path in files:
        if matches(path, OWN) or matches(path, listed):
            continue
        if path.startswith("tests/"):
            continue
        try:
            text = (directory / path).read_text(errors="strict")
        except (UnicodeDecodeError, OSError):
            continue
        if MARKER.search(text[:4000]):
            errors.append(f"{path} credits third-party work but is not in THIRD_PARTY")
    return errors


def file_errors(directory: Path, files: list[str]) -> list[str]:
    errors = [f"{path} must not ship" for path in files if matches(path, FORBIDDEN)]
    for path in files:
        source = directory / path
        if source.is_file() and source.stat().st_size > MAX_FILE_BYTES:
            errors.append(f"{path} is over {MAX_FILE_BYTES} bytes")
    return errors


def crate_size_errors(name: str, version: str) -> list[str]:
    crate = ROOT / "target/package" / f"{name}-{version}.crate"
    if crate.is_file() and crate.stat().st_size > MAX_CRATE_BYTES:
        return [f"{crate.name} is {crate.stat().st_size} bytes, over crates.io's limit"]
    return []


def package_errors(package: dict, licence: str) -> list[str]:
    directory = Path(package["manifest_path"]).parent
    files = packaged(package["name"])
    errors = manifest_errors(package)
    errors += licence_errors(directory, files, licence)
    errors += notice_errors(package["name"], directory, files)
    errors += unlisted_third_party(package["name"], directory, files)
    errors += file_errors(directory, files)
    errors += crate_size_errors(package["name"], package["version"])
    return [f"{package['name']}: {error}" for error in errors]


def main() -> int:
    licence = (ROOT / "LICENSE").read_text()
    packages = metadata()
    failures = [error for package in packages for error in package_errors(package, licence)]
    unknown = set(THIRD_PARTY) - {package["name"] for package in packages}
    failures += [f"THIRD_PARTY names an unknown package: {name}" for name in sorted(unknown)]
    if failures:
        print("Package check failed:\n" + "\n".join(failures), file=sys.stderr)
        return 1
    print(f"Package check passed for {len(packages)} crates: metadata, LICENSE, "
          "third-party notices, file list and sizes.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
