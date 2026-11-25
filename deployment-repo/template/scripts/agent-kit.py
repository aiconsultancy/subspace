#!/usr/bin/env python3
"""The Subspace agent kit: skills and helpers this repository carries for local agents.

Claude Code and Codex started in a clone of this repository get the Subspace
skills (.claude/skills, .agents/skills) and the `subspace` MCP server (.mcp.json,
.codex/config.toml, opencode.json). The skills and helpers are copies of
upstream files at the Subspace commit this repository pins, never edited here.
The MCP registration and the IRC skill run this repository's own helpers with
its own settings (agent-kit/agent.json, through --config), so agents here act
on this deployment whatever else this computer is set up for.

  scripts/agent-kit.py sync [--source CHECKOUT]   copy the pinned upstream files into place
  scripts/agent-kit.py check [--source CHECKOUT]  fail if a copy differs from the pinned upstream (CI)
  scripts/agent-kit.py setup --name "Your Name" [--dry-run] [--force]
                                                  set up this computer: helpers, agent.json and your name

sync and check read the pin's commit from a clone (CHECKOUT, or a cached
fetch of the pinned repository under .cache/). setup changes only
~/.subspace (or SUBSPACE_HOME); it keeps an agent.json or a helper there that
differs from this repository's unless you pass --force, and says how they
differ.
"""

import argparse
import difflib
import filecmp
import json
import os
from pathlib import Path
import re
import shutil
import stat
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
# The Subspace commit the kit comes from (scripts/update-pins.py moves it).
PIN_FILE, PIN_KEY = "pins.json", "subspace"
FILES = {
    ".claude/skills/subspace-irc/SKILL.md": "agent-skills/subspace-irc/SKILL.md",
    ".claude/skills/subspace-tracker/SKILL.md": "agent-skills/claude/subspace-tracker/SKILL.md",
    ".agents/skills/subspace-irc/SKILL.md": "agent-skills/subspace-irc/SKILL.md",
    ".agents/skills/subspace-tracker/SKILL.md": "agent-skills/codex/subspace-tracker/SKILL.md",
    "agent-kit/bin/subspace_token.py": "agent-skills/bin/subspace_token.py",
    "agent-kit/bin/subspace_irc.py": "agent-skills/bin/subspace_irc.py",
}
HELPERS = ("subspace_token.py", "subspace_irc.py")


def die(message):
    sys.exit(f"agent-kit: {message}")


def git(*args, cwd=None, capture=True):
    result = subprocess.run(["git", *args], cwd=cwd, capture_output=capture)
    return result


def load_pin():
    try:
        pin = json.loads((ROOT / PIN_FILE).read_text())
        if PIN_KEY:
            pin = pin[PIN_KEY]
        repository, commit = pin["repository"], pin["commit"]
    except (OSError, ValueError, KeyError, TypeError):
        die(f"{PIN_FILE} has no repository and commit{f' under {PIN_KEY}' if PIN_KEY else ''}.")
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]*/[A-Za-z0-9][A-Za-z0-9._-]*", repository):
        die(f"{PIN_FILE}: repository must be OWNER/NAME.")
    if not re.fullmatch(r"[0-9a-f]{40}", commit):
        die(f"{PIN_FILE}: commit must be a full 40-character lowercase SHA.")
    return repository, commit


def clone_with(repository, commit, source):
    """A Git directory that has the pinned commit: the given checkout, or a cached shallow fetch."""
    if source:
        if git("cat-file", "-e", f"{commit}^{{commit}}", cwd=source).returncode:
            die(f"{source} does not have the pinned commit {commit}.")
        return Path(source)
    cache = ROOT / ".cache" / "agent-kit" / repository.replace("/", "-")
    url = os.environ.get("AGENT_KIT_UPSTREAM_URL") or f"https://github.com/{repository}.git"
    if not (cache / ".git").is_dir():
        cache.mkdir(parents=True, exist_ok=True)
        git("init", "-q", cwd=cache)
    git("remote", "remove", "origin", cwd=cache)
    git("remote", "add", "origin", url, cwd=cache)
    if git("cat-file", "-e", f"{commit}^{{commit}}", cwd=cache).returncode:
        fetched = git("fetch", "-q", "--depth", "1", "--filter=blob:none", "origin", commit, cwd=cache)
        if fetched.returncode:
            die(f"could not fetch {commit} from {url}: {fetched.stderr.decode(errors='replace').strip()}")
    return cache


def pinned_files(source=None):
    repository, commit = load_pin()
    clone = clone_with(repository, commit, source)
    files = {}
    for destination, path in FILES.items():
        shown = git("show", f"{commit}:{path}", cwd=clone)
        if shown.returncode:
            die(f"{repository} {commit[:12]} has no {path}.")
        files[destination] = shown.stdout
    return repository, commit, files


def sync(source):
    repository, commit, files = pinned_files(source)
    for destination, data in files.items():
        path = ROOT / destination
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.is_file() or path.read_bytes() != data:
            path.write_bytes(data)
            print(f"updated {destination}")
        if path.suffix == ".py":
            path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    print(f"The agent kit matches {repository} {commit[:12]}.")


def check(source):
    repository, commit, files = pinned_files(source)
    differ = [destination for destination, data in files.items()
              if not (ROOT / destination).is_file() or (ROOT / destination).read_bytes() != data]
    for destination in differ:
        print(f"{destination} differs from {FILES[destination]} at {repository} {commit[:12]}", file=sys.stderr)
    if differ:
        die("the agent kit does not match the pin. Run scripts/agent-kit.py sync and commit the result.")
    print(f"The agent kit matches {repository} {commit[:12]}.")


def read_env(path):
    values = {}
    if path.is_file():
        for line in path.read_text().splitlines():
            key, _, value = line.partition("=")
            if key and not key.startswith("#"):
                values[key.strip()] = value.strip()
    return values


def differences(installed, pinned):
    """How an installed helper differs from this repository's copy, in a line."""
    added = removed = 0
    for line in difflib.unified_diff(installed.read_text(errors="replace").splitlines(),
                                     pinned.read_text(errors="replace").splitlines(), lineterm="", n=0):
        if not line.startswith(("+++", "---")):
            added += line.startswith("+")
            removed += line.startswith("-")
    return f"differs from this repository's copy by +{added} -{removed} lines; see: diff -u {installed} {pinned}"


def setup(name, dry_run, force):
    """Install the helpers the skills name (~/.subspace/bin) and this deployment's sign-in settings."""
    home = Path(os.environ.get("SUBSPACE_HOME") or Path.home() / ".subspace")
    source = ROOT / "agent-kit" / "agent.json"
    # Without a hosted API, agents use the fleet's development sign-in through its tunnel.
    settings = json.loads(source.read_text()) if source.is_file() else None
    agents = read_env(home / "agents.env")
    name = (name or agents.get("SUBSPACE_HUMAN_NAME") or "").strip()
    if not name or len(name) > 60 or "\n" in name:
        die("pass --name with the name your agents act for, as people know you (at most 60 characters).")
    steps, kept, settings_file = [], [], home / "agent.json"
    try:
        current = json.loads(settings_file.read_text()) if settings_file.is_file() else None
    except ValueError:
        current = {}
    if settings and current is None:
        steps.append(("write", settings_file, f"sign-in settings for {settings['api_url']}"))
    elif settings and {key: current.get(key) for key in settings} != settings:
        changes = ", ".join(f"{key} {current.get(key)!r} there, {value!r} here"
                            for key, value in settings.items() if current.get(key) != value)
        if force:
            steps.append(("replace", settings_file, f"keeping a copy; {changes}"))
        else:
            kept.append((settings_file, f"is set up for another API or sign-in: {changes}"))
    for helper in HELPERS:
        pinned, target = ROOT / "agent-kit" / "bin" / helper, home / "bin" / helper
        if not target.is_file():
            steps.append(("helper", target, "the pinned helper the skills run"))
        elif not filecmp.cmp(pinned, target, shallow=False):
            # It may be newer, from another deployment or a Subspace checkout: replace it only when asked.
            if force:
                steps.append(("replace helper", target, f"keeping a copy; it {differences(target, pinned)}"))
            else:
                kept.append((target, differences(target, pinned)))
    if agents.get("SUBSPACE_HUMAN_NAME") != name:
        steps.append(("name", home / "agents.env", f"your agents mark their messages 'on behalf of {name} (<model>)'"))
    for kind, path, about in steps:
        print(f"{'would ' if dry_run else ''}{kind}: {path} ({about})")
    for path, about in kept:
        print(f"{'would keep' if dry_run else 'kept'}: {path}, which {about}")
    if not steps and not kept:
        print(f"{home} is already set up for this repository.")
    if dry_run:
        warn_kept(kept, dry_run)
        return
    stamp = time.strftime("%Y%m%d%H%M%S")
    for kind, path, _ in steps:
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        if kind.startswith("replace"):
            shutil.copy2(path, path.with_name(f"{path.name}.bak-{stamp}"))
        if kind in ("write", "replace"):
            path.write_text(json.dumps(settings, indent=2) + "\n")
            path.chmod(0o600)
        elif kind in ("helper", "replace helper"):
            shutil.copyfile(ROOT / "agent-kit" / "bin" / path.name, path)
            path.chmod(0o755)
        elif kind == "name":
            agents["SUBSPACE_HUMAN_NAME"] = name
            path.write_text("".join(f"{key}={value}\n" for key, value in agents.items()))
            path.chmod(0o600)
    if not settings:
        print("\nThis deployment has no hosted API: keep the fleet's tunnel running, and register the MCP server "
              "with ./install.sh skills from a Subspace checkout.")
    else:
        token = "python3 agent-kit/bin/subspace_token.py --config agent-kit/agent.json"
        print(f"""
Next, from this repository's root:
  1. Sign in once in your browser:  {token} --login
     Check it:                      {token} --me
  2. Start Claude Code in this repository and allow the `subspace` server from .mcp.json when asked.
     For Codex, trust this repository when asked: project MCP servers load only in trusted projects.
  3. Agent sessions already running see the skills and the server only after they restart.""")
    warn_kept(kept, dry_run)


def warn_kept(kept, dry_run):
    """Say last, so that it cannot be missed, what setup left as it was."""
    if not kept:
        return
    print(f"\nWARNING: {'setup would leave' if dry_run else 'setup left'} these as they are, because they differ from "
          "this repository's and --force was not given:")
    for path, about in kept:
        print(f"  {path}, which {about}")
    print("Agents in this repository use its own helpers and settings (agent-kit/) either way, but agents "
          "elsewhere on this computer keep using the files above. Run setup again with --force to replace them; "
          "a copy of each is kept.")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    for command in ("sync", "check"):
        sub.add_parser(command).add_argument("--source", help="a clone of the pinned repository to read from")
    person = sub.add_parser("setup")
    person.add_argument("--name")
    person.add_argument("--dry-run", action="store_true")
    person.add_argument("--force", action="store_true",
                        help="replace an agent.json or helpers in ~/.subspace that differ from this repository's")
    args = parser.parse_args()
    if args.command == "sync":
        sync(args.source)
    elif args.command == "check":
        check(args.source)
    else:
        setup(args.name, args.dry_run, args.force)


if __name__ == "__main__":
    main()
