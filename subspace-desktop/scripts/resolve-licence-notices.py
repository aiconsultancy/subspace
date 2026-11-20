#!/usr/bin/env python3
"""Apply versioned notice files that cargo-about cannot discover."""

import hashlib
import json
from pathlib import Path
import re
import sys


FALLBACK = re.compile(
    r"unable to find text for license '([^']+)' for crate '(\S+) (\S+)', "
    r"falling back to canonical text"
)


def crate_key(package):
    return f"{package['name']} v{package['version']}"


def checked_text(path, checksum):
    try:
        data = path.read_bytes()
    except OSError as exc:
        raise ValueError(f"cannot read source-backed licence {path}: {exc}") from exc
    if not data or hashlib.sha256(data).hexdigest() != checksum:
        raise ValueError(f"licence checksum mismatch for {path}")
    return data.decode("utf-8")


def resolve_notices(report, diagnostics, root):
    # cargo-about 0.9.3 can fall back to scanning even with --fail.
    if "failed to validate all files specified in clarification for crate" in diagnostics:
        raise ValueError("cargo-about clarification failed its source validation")
    fallbacks = {(f"{name} v{version}", spdx) for spdx, name, version in FALLBACK.findall(diagnostics)}
    packages = {crate_key(entry["package"]): entry["package"] for entry in report["crates"]}
    replacements = set()
    supplemental = []
    notices = json.loads((root / "about-notices.json").read_text(encoding="utf-8"))
    for notice in notices:
        for key in notice["crates"]:
            package = packages.get(key)
            if package is None:
                continue
            source = package.get("source")
            if source != notice.get("source", "registry+https://github.com/rust-lang/crates.io-index"):
                continue
            directory = Path(package["manifest_path"]).parent if notice.get("crate_file") else root
            path = directory / notice["path"]
            text = checked_text(path, notice["sha256"])
            spdx = notice["id"]
            replaces = notice.get("replaces", spdx)
            if replaces is not None and (key, replaces) not in fallbacks:
                continue
            supplemental.append({
                "id": spdx, "text": text, "source_path": str(path), "used_by": [{"crate": package}]
            })
            if replaces is not None:
                replacements.add((key, replaces))

    # The workspace crates share the project LICENSE shipped next to the report.
    for key, spdx in fallbacks:
        package = packages[key]
        if (
            spdx == "Apache-2.0"
            and package.get("source") is None
            and package["name"].split("-")[0] == "subspace"
            and root in Path(package["manifest_path"]).resolve().parents
        ):
            path = root.parent / "LICENSE"
            supplemental.append({
                "id": spdx, "text": path.read_text(encoding="utf-8"),
                "source_path": str(path), "used_by": [{"crate": package}]
            })
            replacements.add((key, spdx))

    # A generated text may be shared with a byte-identical source-backed file.
    # Only remove the affected crate from that text's use list.
    for licence in report["licenses"]:
        if licence.get("source_path") is None:
            # Retain the original diagnostic matches even when a pinned notice
            # replaces a fallback in a text shared with other crates.
            licence["fallback_crates"] = sorted({
                key for item in licence["used_by"]
                if (key := crate_key(item["crate"]), licence["id"]) in fallbacks
            })
        licence["used_by"] = [
            item for item in licence["used_by"]
            if (crate_key(item["crate"]), licence["id"]) not in replacements
        ]
    report["licenses"].extend(supplemental)
    report["fallbacks"] = [
        {"crate": key, "id": spdx} for key, spdx in sorted(fallbacks - replacements)
    ]
    return report


def main():
    report_path, log_path = map(Path, sys.argv[1:])
    report = json.loads(report_path.read_text(encoding="utf-8"))
    diagnostics = log_path.read_text(encoding="utf-8")
    try:
        report = resolve_notices(report, diagnostics, Path.cwd().resolve())
    except ValueError as exc:
        sys.exit(str(exc))
    json.dump(report, sys.stdout, ensure_ascii=True)


if __name__ == "__main__":
    main()
