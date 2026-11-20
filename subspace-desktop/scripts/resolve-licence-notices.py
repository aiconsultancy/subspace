#!/usr/bin/env python3
"""Apply versioned notice files that cargo-about cannot discover."""

import argparse
from collections import defaultdict
import hashlib
import json
import os
from pathlib import Path
import re
import sys


FALLBACK = re.compile(
    r"unable to find text for license '([^']+)' for crate '(\S+) (\S+)', "
    r"falling back to canonical text"
)
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"


def crate_name(package):
    return f"{package['name']} v{package['version']}"


def checked_bytes(path, checksum):
    try:
        data = path.read_bytes()
    except OSError as exc:
        raise ValueError(f"cannot read source-backed licence {path}: {exc}") from exc
    if not data or hashlib.sha256(data).hexdigest() != checksum:
        raise ValueError(f"licence checksum mismatch for {path}")
    return data


def cargo_tree_label(package):
    name = crate_name(package)
    source = package["source"]
    if source is None:
        return f"{name} ({Path(package['manifest_path']).parent})"
    if source.startswith("git+"):
        url, _, revision = source[4:].rpartition("#")
        return f"{name} ({url}#{revision[:8]})"
    if source == CRATES_IO:
        return name
    # Cargo displays a configured registry name here, which metadata omits.
    # Leave unrecognised sources unmatched so they cannot pass coverage.
    return None


def linked_packages(tree, packages):
    by_label = defaultdict(list)
    for package in packages.values():
        by_label[cargo_tree_label(package)].append(package)
    linked = []
    for label in sorted({line.removesuffix(" (*)") for line in tree.splitlines() if line}):
        matches = by_label[label]
        if not matches:
            raise ValueError(f"missing licence records for {label}")
        if len(matches) != 1:
            raise ValueError(f"ambiguous Cargo sources in the release tree for {label}")
        linked.append(matches[0])
    if not linked:
        raise ValueError("empty release dependency tree")
    return linked


def has_own_text(licence, package):
    # cargo-about merges byte-identical texts and only records the first
    # source path, which may be null even when another crate supplies a file.
    directory = Path(package["manifest_path"]).parent
    if licence["source_path"] and Path(licence["source_path"]).parent == directory:
        return True
    contents = licence["text"].encode("utf-8")
    for file in directory.glob("*"):
        if (
            file.name.upper().startswith(("LICENSE", "LICENCE", "COPYING"))
            and file.is_file()
            and file.stat().st_size == len(contents)
            and file.read_bytes() == contents
        ):
            return True
    return False


def fallback_packages(report, diagnostics):
    by_name = defaultdict(dict)
    for licence in report["licenses"]:
        for item in licence["used_by"]:
            package = item["crate"]
            by_name[crate_name(package), licence["id"]].setdefault(package["id"], []).append(
                (licence, package)
            )
    fallbacks = set()
    for spdx, name, version in FALLBACK.findall(diagnostics):
        label = f"{name} v{version}"
        matches = by_name[label, spdx]
        if len(matches) > 1:
            matches = {
                package_id: uses for package_id, uses in matches.items()
                if not any(has_own_text(licence, package) for licence, package in uses)
            }
        if len(matches) != 1:
            raise ValueError(f"cannot identify the Cargo source for fallback {label} ({spdx})")
        fallbacks.add((next(iter(matches)), spdx))
    return fallbacks


def verify_native_notices(root, target, linked):
    pins = json.loads((root / "native-notices.json").read_text(encoding="utf-8"))
    for component, pin in pins.items():
        if target not in pin["targets"]:
            continue
        matches = [package for package in linked if package["name"] == pin["crate"]]
        if len(matches) != 1 or (matches[0]["version"], matches[0]["source"]) != (
            pin["crate_version"], CRATES_IO
        ):
            raise ValueError(
                f"{component} {pin['version']} notices require {pin['crate']} "
                f"v{pin['crate_version']} from crates.io for {target}"
            )
        if component == "onnxruntime" and any(
            variable in os.environ for variable in ("ORT_LIB_PATH", "ORT_LIB_LOCATION")
        ):
            raise ValueError(
                "ONNX Runtime release notices require the pinned download "
                "without ORT_LIB_PATH or ORT_LIB_LOCATION"
            )
        directory = Path(matches[0]["manifest_path"]).parent
        for filename, checksum in pin["artifacts"].items():
            try:
                checked_bytes(directory / filename, checksum)
            except ValueError as exc:
                raise ValueError(f"{component} {pin['version']} artefact mismatch: {exc}") from exc
        for filename, checksum in pin["notices"].items():
            checked_bytes(root / "licenses" / component / filename, checksum)


def resolve_notices(report, diagnostics, root, tree=None, target=None):
    # cargo-about 0.9.3 can fall back to scanning even with --fail.
    if "failed to validate all files specified in clarification for crate" in diagnostics:
        raise ValueError("cargo-about clarification failed its source validation")
    packages = {entry["package"]["id"]: entry["package"] for entry in report["crates"]}
    fallbacks = fallback_packages(report, diagnostics)
    if tree is not None:
        linked = linked_packages(tree, packages)
        verify_native_notices(root, target, linked)
        report["linked"] = [package["id"] for package in linked]
    replacements = set()
    supplemental = []
    notices = json.loads((root / "about-notices.json").read_text(encoding="utf-8"))
    for notice in notices:
        for key, package in packages.items():
            if (
                crate_name(package) not in notice["crates"]
                or package["source"] != notice.get("source", CRATES_IO)
            ):
                continue
            directory = Path(package["manifest_path"]).parent if notice.get("crate_file") else root
            path = directory / notice["path"]
            text = checked_bytes(path, notice["sha256"]).decode("utf-8")
            spdx = notice["id"]
            replaces = notice.get("replaces", spdx)
            if replaces is not None and (key, replaces) not in fallbacks:
                continue
            supplemental.append({
                "id": spdx, "text": text, "source_path": str(path), "used_by": [{"crate": package}]
            })
            if replaces is not None:
                replacements.add((key, replaces))

    # Resolve the Framework through Cargo, including local paths other than
    # the usual sibling checkout. Published crates carry their own notices.
    project_licences = {}
    for package in packages.values():
        if package["name"] == "subspace-framework" and package["source"] is None:
            framework = Path(package["manifest_path"]).resolve().parent
            project_licences[framework] = framework / "LICENSE"
    project_licences[root] = root.parent / "LICENSE"
    for key, spdx in fallbacks:
        package = packages[key]
        directory = Path(package["manifest_path"]).resolve().parent
        if (
            spdx != "Apache-2.0"
            or package["source"] is not None
            or package["name"].split("-")[0] != "subspace"
        ):
            continue
        for project, path in project_licences.items():
            if directory.is_relative_to(project):
                supplemental.append({
                    "id": spdx, "text": path.read_text(encoding="utf-8"),
                    "source_path": str(path), "used_by": [{"crate": package}]
                })
                replacements.add((key, spdx))
                break

    linked_ids = set(report.get("linked", []))
    for licence in report["licenses"]:
        spdx = licence["id"]
        has_fallback = licence["source_path"] is None or any(
            (item["crate"]["id"], spdx) in fallbacks for item in licence["used_by"]
        )
        licence["used_by"] = [
            item for item in licence["used_by"]
            if (item["crate"]["id"], spdx) not in replacements
        ]
        if has_fallback and tree is not None:
            licence["verified_crates"] = [
                package["id"] for item in licence["used_by"]
                if (package := item["crate"])["id"] in linked_ids
                and ((package["id"], spdx) in fallbacks or has_own_text(licence, package))
            ]
    report["licenses"].extend(supplemental)
    report["fallbacks"] = [
        {"crate": packages[key], "id": spdx} for key, spdx in sorted(fallbacks - replacements)
    ]
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("log", type=Path)
    parser.add_argument("--tree", type=Path)
    parser.add_argument("--target")
    args = parser.parse_args()
    if bool(args.tree) != bool(args.target):
        parser.error("--tree and --target must be supplied together")
    try:
        report = resolve_notices(
            json.loads(args.report.read_text(encoding="utf-8")),
            args.log.read_text(encoding="utf-8"),
            Path.cwd().resolve(),
            args.tree.read_text(encoding="utf-8") if args.tree else None,
            args.target,
        )
    except (OSError, ValueError) as exc:
        sys.exit(str(exc))
    json.dump(report, sys.stdout, ensure_ascii=True)


if __name__ == "__main__":
    main()
