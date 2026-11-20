#!/usr/bin/env python3
"""Apply versioned notice files that cargo-about cannot discover."""

import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re
import sys


FALLBACK = re.compile(
    r"unable to find text for license '([^']+)' for crate '(\S+) (\S+)', "
    r"falling back to canonical text"
)
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
# cargo-about 0.9.3 uses these exact spdx 0.13.6 texts as fallbacks. A
# source-backed first entry can hide the fallback in a merged text group.
CANONICAL_SPDX = {
    "0BSD": "e3f18c71e10d673590eb9856c1d79dd3b4b0d65404efb5e8584dbede7edd608b",
    "Apache-2.0": "074e6e32c86a4c0ef8b3ed25b721ca23aca83df277cd88106ef7177c354615ff",
    "BSD-2-Clause": "f32fb3b417a194167cfad068223fc975ba96c5960513a10f66a3c28720aec1df",
    "BSD-3-Clause": "5a93d5831e1297ab10fe643e1a631e83be392896da14ee2951285a79012df69d",
    "BSL-1.0": "84c6ef3ea9e3254a54d0acf5d3e0c61ae011b8fef7dd6940591cf060e6380a8f",
    "CC0-1.0": "a2010f343487d3f7618affe54f789f5487602331c0a8d03f49e9a7c547cf0499",
    "ISC": "521c6f0ed8e64736684f89843d16a4df6b4b7449acbcfc6ca6630a3037ca8c53",
    "MIT": "b05785f9f18e6716bab63424b11454513b9943a222595b70411009202fc592b5",
    "MIT-0": "59746d6285ffa44bfc7ecada352aa5d6a20dc8eab418a60ce091cc739012c135",
    "MPL-2.0": "66a3107d5ad6a058aab753eaac2047ccb2ed0e39465dd0fe5844da3e300d5172",
    "Unicode-3.0": "f7db81051789b729fea528a63ec4c938fdcb93d9d61d97dc8cc2e9df6d47f2a1",
    "Unicode-DFS-2016": "1a33fc12a3abb0b3eb14db70e36ef1d0af51c93b1cc1f80f022c3bd406e9b874",
    "Zlib": "bfb1112d49db5b1daecdfef24bd7e2f3ea0bafb33aa67aa0ab51e2bf8407c03d",
}


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
    directory = Path(package["manifest_path"]).parent.resolve()
    contents = licence["text"].encode("utf-8")
    files = [file for file in directory.glob("*")
             if file.name.upper().startswith(("LICENSE", "LICENCE", "COPYING"))]
    if licence["source_path"]:
        source = Path(licence["source_path"]).resolve()
        if source.is_relative_to(directory):
            files.append(source)
    if package.get("license_file"):
        files.append(directory / package["license_file"])
    return any(file.is_file() and file.stat().st_size == len(contents)
               and file.read_bytes() == contents for file in files)


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
        canonical = hashlib.sha256(licence["text"].encode("utf-8")).hexdigest() == CANONICAL_SPDX.get(spdx)
        has_fallback = canonical or licence["source_path"] is None or any(
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
