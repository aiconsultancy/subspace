#!/usr/bin/env python3
"""Move the pins in pins.json: the Subspace and Stargate releases this deployment runs.

  scripts/update-pins.py                     both projects to their latest releases
  scripts/update-pins.py --subspace REF      Subspace only: a release tag, or a full commit SHA on main
  scripts/update-pins.py --stargate REF      Stargate only (pass both flags to move both)
  scripts/update-pins.py --dry-run           print the moves, change nothing

A pin records the release tag and the commit it resolves to, so a moved tag
cannot change what is deployed. After moving the pins: run
scripts/agent-kit.py sync, review the upstream changes it links, commit, then
deploy (README, "Updating").
"""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PINS = ROOT / "pins.json"
SHA = re.compile(r"[0-9a-f]{40}")


def die(message):
    sys.exit(f"update-pins: {message}")


def version(tag, pattern):
    match = re.fullmatch(pattern.replace("*", r"(\d+)\.(\d+)\.(\d+)"), tag)
    return tuple(int(part) for part in match.groups()) if match else None


def remote_tags(repository):
    """{tag: commit} from the repository's remote, with annotated tags peeled to their commit."""
    url = os.environ.get("UPDATE_PINS_URL_" + repository.split("/")[-1].upper().replace("-", "_")) \
        or f"https://github.com/{repository}.git"
    listed = subprocess.run(["git", "ls-remote", "--tags", url], capture_output=True, text=True)
    if listed.returncode:
        die(f"cannot list the tags of {url}: {listed.stderr.strip()}")
    tags = {}
    for line in listed.stdout.splitlines():
        commit, _, ref = line.partition("\t")
        name = ref.removeprefix("refs/tags/")
        if name.endswith("^{}"):
            tags[name[:-3]] = commit  # the commit an annotated tag points at
        else:
            tags.setdefault(name, commit)
    return tags


def resolve(project, reference=None):
    """The {release, commit} a project's pin should move to: its latest release, a tag or a commit."""
    if reference and SHA.fullmatch(reference):
        return {"release": "", "commit": reference}
    tags = remote_tags(project["repository"])
    if reference:
        if reference not in tags:
            die(f"{project['repository']} has no tag {reference}.")
        return {"release": reference, "commit": tags[reference]}
    releases = [(version(tag, project["tag_pattern"]), tag) for tag in tags if version(tag, project["tag_pattern"])]
    if not releases:
        die(f"{project['repository']} has no release tag like {project['tag_pattern']}.")
    tag = max(releases)[1]
    return {"release": tag, "commit": tags[tag]}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--subspace", metavar="REF")
    parser.add_argument("--stargate", metavar="REF")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    pins = json.loads(PINS.read_text())
    moved = False
    # A flag names the projects to move; the others stay where they are pinned.
    named = [name for name in ("subspace", "stargate") if getattr(args, name) is not None] \
        or ["subspace", "stargate"]
    for name in named:
        project = pins[name]
        target = resolve(project, getattr(args, name))
        old = project["commit"]
        if target["commit"] == old and target["release"] == project.get("release", ""):
            print(f"{name}: stays at {project.get('release') or old[:12]}")
            continue
        moved = True
        print(f"{name}: {project.get('release') or old[:12]} -> {target['release'] or target['commit'][:12]}"
              f"  https://github.com/{project['repository']}/compare/{old}...{target['commit']}")
        project.update(target)
    if moved and not args.dry_run:
        PINS.write_text(json.dumps(pins, indent=2) + "\n")
        print("Next: python3 scripts/agent-kit.py sync, review the changes above, commit, then deploy.")


if __name__ == "__main__":
    main()
