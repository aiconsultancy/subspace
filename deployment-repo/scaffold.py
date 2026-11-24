#!/usr/bin/env python3
"""Create your own deployment repository for Subspace from the templates beside this script.

  python3 deployment-repo/scaffold.py --dir ../example-deployment --name "Example Org" --dry-run
  python3 deployment-repo/scaffold.py --dir ../example-deployment --name "Example Org" \\
      --api-url https://api.example.com --workos-client-id client_EXAMPLE

It pins the latest Subspace and Stargate releases (or --subspace and --stargate:
a release tag or a full commit SHA), writes the skeleton (README.md,
AGENTS.md, pins.json, fleet/, hosting/, desktop/, runbooks/, a .gitignore that
keeps secrets out, the agent kit and its CI check), copies the agent skills
and helpers at the Subspace pin, and runs git init. With a hosted API
(--api-url and --workos-client-id) it also registers the API's MCP server for
Claude Code, Codex and OpenCode. It never commits, pushes or creates a remote
repository. See docs/deployment-repository.md.
"""

import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

HERE = Path(__file__).resolve().parent
TEMPLATE = HERE / "template"
UPSTREAM = HERE.parent  # this Subspace checkout, which may already have the pinned commit
HOSTED_ONLY = {".mcp.json", ".codex/config.toml", "opencode.json", "agent-kit/agent.json"}
PLACEHOLDER = re.compile(r"@@[A-Z_]+@@")


def die(message):
    sys.exit(f"scaffold: {message}")


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def hosted_gaps(commit):
    """What a hosted API's agent kit needs that the pinned Subspace commit lacks, when this checkout has it."""
    gaps = []
    for path, marker, what in (("agent-skills/bin/subspace_token.py", "workos", "WorkOS sign-in for local agents"),
                               ("agent-skills/bin/subspace_irc.py", "irc-credential", "IRC sign-in through the API")):
        shown = subprocess.run(["git", "show", f"{commit}:{path}"], cwd=UPSTREAM, capture_output=True, text=True)
        if shown.returncode == 0 and marker not in shown.stdout:
            gaps.append(what)
    return gaps


def template_files(hosted):
    for path in sorted(TEMPLATE.rglob("*")):
        relative = path.relative_to(TEMPLATE).as_posix()
        if path.is_file() and "__pycache__" not in relative and (hosted or relative not in HOSTED_ONLY):
            yield relative, path


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--dir", required=True, type=Path, help="the new repository's folder (new or empty)")
    parser.add_argument("--name", required=True, help="whose deployment this is, for example your organisation")
    parser.add_argument("--api-url", default="", help="your hosted Subspace API's HTTPS origin")
    parser.add_argument("--workos-client-id", default="", help="the WorkOS AuthKit client ID that API uses")
    parser.add_argument("--subspace", metavar="REF", help="a Subspace release tag or commit (default: the latest release)")
    parser.add_argument("--stargate", metavar="REF", help="a Stargate release tag or commit (default: the latest release)")
    parser.add_argument("--no-git", action="store_true", help="do not run git init")
    parser.add_argument("--dry-run", action="store_true", help="print what would be created and change nothing")
    args = parser.parse_args()

    name, api_url, client = args.name.strip(), args.api_url.strip().rstrip("/"), args.workos_client_id.strip()
    if not name or len(name) > 60 or "\n" in name or PLACEHOLDER.search(name):
        die("--name must be one line of at most 60 characters.")
    if bool(api_url) != bool(client):
        die("pass --api-url and --workos-client-id together, or neither (no hosted API).")
    if api_url and not re.fullmatch(r"https://[A-Za-z0-9.-]+(?::[0-9]{1,5})?", api_url):
        die("--api-url must be an HTTPS origin such as https://api.example.com.")
    if client and not re.fullmatch(r"client_[A-Za-z0-9]{1,121}", client):
        die("--workos-client-id must look like client_...")
    target = args.dir.expanduser().resolve()
    if target.exists() and (not target.is_dir() or any(target.iterdir())):
        die(f"{target} exists and is not an empty folder; choose a new one.")

    pins_module = load("update_pins", TEMPLATE / "scripts" / "update-pins.py")
    pins = json.loads((TEMPLATE / "pins.json").read_text())
    values = {"NAME": name, "API_URL": api_url, "WORKOS_CLIENT_ID": client}
    for project in ("subspace", "stargate"):
        resolved = pins_module.resolve(pins[project], getattr(args, project))
        values[f"{project.upper()}_RELEASE"] = resolved["release"]
        values[f"{project.upper()}_COMMIT"] = resolved["commit"]
        print(f"{project}: {pins[project]['repository']} {resolved['release'] or '(commit)'} {resolved['commit']}")

    gaps = hosted_gaps(values["SUBSPACE_COMMIT"]) if api_url else []
    if gaps:
        print(f"warning: that Subspace commit has no {' and no '.join(gaps)}, so local agents cannot use your hosted "
              "API with it. Pin a later commit on main with --subspace <commit> until a release has it.")
    rendered = {}
    for relative, path in template_files(bool(api_url)):
        text = path.read_text()
        for key, value in values.items():
            text = text.replace(f"@@{key}@@", value)
        left = PLACEHOLDER.search(text)
        if left:
            die(f"template {relative} has an unknown placeholder {left.group(0)}.")
        rendered[relative] = (text, path.stat().st_mode & 0o111)
    if args.dry_run:
        print(f"would create {target}:")
        for relative in rendered:
            print(f"  {relative}")
        kit = load("agent_kit", TEMPLATE / "scripts" / "agent-kit.py")
        for relative in kit.FILES:
            print(f"  {relative}  (copied from Subspace {values['SUBSPACE_COMMIT'][:12]})")
        if not api_url:
            print("no hosted API: no MCP registration or agent.json; people use ./install.sh skills with the tunnel")
        print("would run git init" if not args.no_git else "would not run git init")
        return

    for relative, (text, executable) in rendered.items():
        path = target / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        if executable:
            path.chmod(0o755)
    have = subprocess.run(["git", "cat-file", "-e", values["SUBSPACE_COMMIT"] + "^{commit}"], cwd=UPSTREAM,
                          capture_output=True).returncode == 0
    sync = [sys.executable, "-B", str(target / "scripts" / "agent-kit.py"), "sync", *(["--source", str(UPSTREAM)] if have else [])]
    if subprocess.run(sync, cwd=target).returncode:
        die(f"created {target}, but copying the agent kit failed; run python3 scripts/agent-kit.py sync there.")
    if not args.no_git:
        subprocess.run(["git", "init", "-q", "-b", "main"], cwd=target, check=True)
    print(f"""
Created {target}. Nothing is committed or pushed. Next:
  1. Review it, add your fleet's definition (fleet/), hosting code and runbooks, without secrets.
  2. Create a private repository for it and push, for example:
       gh repo create OWNER/NAME --private --source {target} --push   (after a first commit)
  3. Everyone who clones it runs: python3 scripts/agent-kit.py setup --name "Their Name\"""")


if __name__ == "__main__":
    main()
