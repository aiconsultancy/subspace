"""Validate a web Space folder as Subspace's installer does and write a reproducible ZIP.

usage: package-web-space.py FOLDER [OUT.zip] [--include PATTERN]... [--exclude PATTERN]... [--check]

The rules mirror spaces/web/src/manifest.rs, spaces/web/src/manifest/fields.rs and
spaces/web/src/install/{paths,files,archive,stage}.rs. Error texts match the app's.
The manifest keeps its compatibility name, subspace-board.json.
"""

import argparse
import fnmatch
import hashlib
import json
import os
import re
import stat
import sys
import tempfile
import zipfile
from pathlib import Path

MANIFEST = "subspace-board.json"
MAX_MANIFEST_BYTES = 256 * 1024
MAX_NAME_CHARS = 60
MAX_VERSION_CHARS = 40
MAX_HELP_CHARS = 200
MAX_FIELDS = 32
MAX_KEY_CHARS = 40
MAX_OPTIONS = 50
PERMISSIONS = ("notifications", "settings", "badge", "context")
FIELD_TYPES = "text, url, number, boolean, select or secret"

MAX_PATH_BYTES = 1024
MAX_COMPONENT_BYTES = 255
MAX_DEPTH = 32
MAX_ENTRIES = 10_000
MAX_FILE_BYTES = 32 * 1024 * 1024
MAX_EXPANDED_BYTES = 256 * 1024 * 1024
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024
PORTABLE = re.compile(r"^[A-Za-z0-9 \-_.@()+,\[\]{}]+$")
DEVICES = re.compile(r"^(CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])$")
UNSAFE_NAME = (
    "The package contains an unsafe or unsupported filename. "
    "Use portable ASCII names without .git or device names."
)

# Development files that a running web Space never needs. --include keeps one.
EXCLUDED_DIRECTORIES = {"test", "tests", "__tests__", "node_modules"}
EXCLUDED_FILES = (
    "mise.toml",
    "package.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "tsconfig.json",
    "*.test.js",
    "*.test.mjs",
    "*.test.ts",
    "*.spec.js",
    "*.spec.mjs",
    "*.spec.ts",
)
ZIP_TIME = (1980, 1, 1, 0, 0, 0)
UNIX = 3


class PackageError(Exception):
    """A rule the installer would reject."""


def bounded(entry: dict, key: str, limit: int) -> str:
    value = entry.get(key)
    text = value.strip() if isinstance(value, str) else ""
    if not text or len(text) > limit:
        raise PackageError(f"expected {key} to be text of 1 to {limit} characters.")
    return text


def listed(value, name: str) -> list:
    if value is None:
        return []
    if not isinstance(value, list):
        raise PackageError(f"expected {name} to be a list.")
    return value


def text_default(default) -> None:
    if default is not None and not isinstance(default, str):
        raise PackageError("expected default to be text.")


def select(entry: dict, default) -> None:
    options = listed(entry.get("options"), "options")
    if not all(isinstance(option, str) for option in options):
        raise PackageError("expected options to be a list of text.")
    usable = 0 < len(options) <= MAX_OPTIONS and all(
        0 < len(option) <= MAX_NAME_CHARS for option in options
    )
    if not usable:
        raise PackageError(
            f"expected 1 to {MAX_OPTIONS} options of 1 to {MAX_NAME_CHARS} characters."
        )
    text_default(default)
    if default is not None and default not in options:
        raise PackageError("expected default to be one of the options.")


def kind(entry: dict) -> None:
    default = entry.get("default")
    kind_name = entry.get("type")
    if kind_name in ("text", "url"):
        text_default(default)
    elif kind_name == "number":
        if default is not None and (
            isinstance(default, bool) or not isinstance(default, (int, float))
        ):
            raise PackageError("expected default to be a number.")
    elif kind_name == "boolean":
        if default is not None and not isinstance(default, bool):
            raise PackageError("expected default to be true or false.")
    elif kind_name == "select":
        select(entry, default)
    elif kind_name == "secret":
        if default is not None:
            raise PackageError("a secret cannot have a default.")
    else:
        raise PackageError(f"expected type to be {FIELD_TYPES}.")


def valid_key(key) -> bool:
    return (
        isinstance(key, str)
        and len(key.encode()) <= MAX_KEY_CHARS
        and key[:1].isascii()
        and key[:1].isalpha()
        and all(c.isascii() and (c.isalnum() or c in "_-") for c in key)
    )


def field(entry) -> str:
    if not isinstance(entry, dict):
        raise PackageError("expected an object.")
    key = entry.get("key")
    if not valid_key(key):
        raise PackageError(
            f"expected key to be 1 to {MAX_KEY_CHARS} letters, digits, _ or -, "
            "starting with a letter."
        )
    if entry.get("help") is not None:
        bounded(entry, "help", MAX_HELP_CHARS)
    bounded(entry, "label", MAX_NAME_CHARS)
    kind(entry)
    return key


def settings(value) -> int:
    entries = listed(value, "settings")
    if len(entries) > MAX_FIELDS:
        raise PackageError(f"expected at most {MAX_FIELDS} settings.")
    keys = set()
    for index, entry in enumerate(entries):
        try:
            key = field(entry)
        except PackageError as error:
            raise PackageError(f"settings[{index}]: {error}") from None
        if key in keys:
            raise PackageError(f"settings[{index}]: the key {key} is used twice.")
        keys.add(key)
    return len(entries)


def permissions(value) -> tuple[list[str], list[str]]:
    granted, warnings = set(), []
    for index, entry in enumerate(listed(value, "permissions")):
        if not isinstance(entry, str):
            raise PackageError(f"expected permissions[{index}] to be text.")
        if entry in PERMISSIONS:
            granted.add(entry)
        else:
            warnings.append(f"Unknown permission {json.dumps(entry)} ignored.")
    return sorted(granted, key=PERMISSIONS.index), warnings


def reject_constant(name: str):
    raise ValueError(f"{name} is not valid JSON")


def parse_manifest(text: str) -> dict:
    try:
        root = json.loads(text, parse_constant=reject_constant)
    except (json.JSONDecodeError, ValueError) as error:
        raise PackageError(f"invalid JSON ({error}).") from None
    if not isinstance(root, dict):
        raise PackageError("expected a JSON object.")
    granted, warnings = permissions(root.get("permissions"))
    return {
        "name": bounded(root, "name", MAX_NAME_CHARS),
        "version": bounded(root, "version", MAX_VERSION_CHARS),
        "permissions": granted,
        "settings": settings(root.get("settings")),
        "warnings": warnings,
    }


def read_manifest(folder: Path) -> dict:
    data = (folder / MANIFEST).read_bytes()
    if len(data) > MAX_MANIFEST_BYTES:
        raise PackageError(f"{MANIFEST} is larger than 256 KiB.")
    try:
        return parse_manifest(data.decode("utf-8"))
    except (PackageError, UnicodeDecodeError) as error:
        raise PackageError(f"{MANIFEST}: {error}") from None


def check_path(text: str) -> None:
    if len(text.encode()) > MAX_PATH_BYTES:
        raise PackageError("Select a non-empty relative board path of at most 1024 bytes.")
    for part in text.split("/"):
        stem = part.split(".")[0].upper()
        portable = (
            part not in ("", ".", "..")
            and part.lower() != ".git"
            and not part.endswith((".", " "))
            and len(part.encode()) <= MAX_COMPONENT_BYTES
            and PORTABLE.match(part) is not None
        )
        if not portable or DEVICES.match(stem):
            raise PackageError(UNSAFE_NAME)
    if text.count("/") + 1 > MAX_DEPTH:
        raise PackageError("The package exceeds 32 directory levels.")


def excluded(relative: str, is_dir: bool, includes: list[str], excludes: list[str]) -> bool:
    if any(
        fnmatch.fnmatchcase(relative, pattern) or (is_dir and pattern.startswith(f"{relative}/"))
        for pattern in includes
    ):
        return False
    name = relative.rsplit("/", 1)[-1]
    if any(fnmatch.fnmatchcase(relative, pattern) for pattern in excludes):
        return True
    if name.startswith("."):
        return True
    if is_dir:
        return name in EXCLUDED_DIRECTORIES
    return any(fnmatch.fnmatchcase(name, pattern) for pattern in EXCLUDED_FILES)


def collect(root: Path, includes: list[str], excludes: list[str]) -> tuple[list[str], list[str]]:
    """Return the packaged files and warnings, walking in a stable order."""
    files, warnings, seen, entries = [], [], {}, 0
    pending = [""]
    while pending:
        directory = pending.pop()
        for name in sorted(os.listdir(root / directory)):
            relative = f"{directory}/{name}" if directory else name
            info = os.lstat(root / relative)
            is_dir = stat.S_ISDIR(info.st_mode)
            if excluded(relative, is_dir, includes, excludes):
                continue
            check_path(relative)
            if not is_dir and not stat.S_ISREG(info.st_mode):
                raise PackageError("Package links and special files are not supported.")
            key = relative.lower()
            if key in seen:
                raise PackageError("The package has duplicate or platform-colliding paths.")
            seen[key] = relative
            entries += 1
            if entries > MAX_ENTRIES:
                raise PackageError("Package contains more than 10,000 entries.")
            if is_dir:
                pending.append(relative)
                continue
            if info.st_nlink != 1:
                warnings.append(
                    f"{relative} is hard-linked. A Local folder install rejects it; "
                    "the ZIP is unaffected."
                )
            files.append(relative)
    return sorted(files), warnings


def check_sizes(root: Path, files: list[str]) -> int:
    total = 0
    for relative in files:
        size = (root / relative).stat().st_size
        if size > MAX_FILE_BYTES:
            raise PackageError("A package file exceeds 32 MiB.")
        total += size
        if total > MAX_EXPANDED_BYTES:
            raise PackageError("Package expansion exceeds 256 MiB.")
    return total


def require_root_files(files: list[str]) -> None:
    if "index.html" not in files:
        raise PackageError(
            "The board root needs a regular, readable index.html. Select the built app folder."
        )
    if MANIFEST not in files:
        raise PackageError(f"The board root needs a regular, readable {MANIFEST}.")


def write_zip(root: Path, files: list[str], out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    handle, temporary = tempfile.mkstemp(prefix=".web-space-", suffix=".zip", dir=out.parent)
    os.close(handle)
    try:
        with zipfile.ZipFile(temporary, "w", allowZip64=False) as archive:
            for relative in files:
                info = zipfile.ZipInfo(relative, ZIP_TIME)
                info.create_system = UNIX
                info.external_attr = (stat.S_IFREG | 0o644) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, (root / relative).read_bytes(), compresslevel=9)
        if os.path.getsize(temporary) > MAX_ARCHIVE_BYTES:
            raise PackageError("ZIP exceeds 64 MiB.")
        os.replace(temporary, out)
    finally:
        if os.path.exists(temporary):
            os.remove(temporary)


def slug(text: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "-", text).strip("-.") or "web-space"


def package(folder: Path, out: Path | None, options) -> dict:
    if folder.is_symlink() or not folder.is_dir():
        raise PackageError("Select a regular app folder, not a link.")
    root = folder.resolve()
    files, warnings = collect(root, options.include, options.exclude)
    require_root_files(files)
    manifest = read_manifest(root)
    size = check_sizes(root, files)
    result = {**manifest, "files": files, "bytes": size}
    result["warnings"] = manifest["warnings"] + warnings
    if options.check:
        return result
    if out is None:
        out = Path("dist/web-spaces") / f"{slug(root.name)}-{slug(manifest['version'])}.zip"
    out = out.resolve()
    if out.is_relative_to(root):
        raise PackageError("Write the ZIP outside the web Space folder.")
    write_zip(root, files, out)
    result["zip"] = out
    result["sha256"] = hashlib.sha256(out.read_bytes()).hexdigest()
    return result


def report(result: dict) -> None:
    permissions_text = ", ".join(result["permissions"]) or "none"
    print(f"web Space: {result['name']} {result['version']}")
    print(f"permissions: {permissions_text}; settings: {result['settings']}")
    print(f"files: {len(result['files'])}, {result['bytes']} bytes")
    for warning in result["warnings"]:
        print(f"warning: {warning}")
    if "zip" in result:
        print(f"sha256: {result['sha256']}")
        print(f"wrote {result['zip']}")
    else:
        print("valid; no ZIP written (--check)")


def arguments(argv: list[str]):
    parser = argparse.ArgumentParser(
        description="Validate a web Space folder and write a reproducible ZIP."
    )
    parser.add_argument("folder", type=Path, help="the folder that holds subspace-board.json")
    parser.add_argument(
        "out",
        type=Path,
        nargs="?",
        help="the ZIP to write (default: dist/web-spaces/<folder>-<version>.zip)",
    )
    parser.add_argument(
        "--include",
        action="append",
        default=[],
        metavar="PATTERN",
        help="keep a path that is excluded by default, such as 'test/*'",
    )
    parser.add_argument(
        "--exclude",
        action="append",
        default=[],
        metavar="PATTERN",
        help="leave out another path, matched against its folder-relative path",
    )
    parser.add_argument("--check", action="store_true", help="validate only")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    options = arguments(argv)
    try:
        report(package(options.folder, options.out, options))
    except PackageError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
