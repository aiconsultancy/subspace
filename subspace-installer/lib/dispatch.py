"""Run the fleet installer on independent SSH hosts."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tarfile
import tempfile

from runtime import Invalid, app_keys, extra_sources, fields, pattern, read_json, require, validate


PACKAGE = Path(__file__).resolve().parents[1]
FILES = (
    "install.sh", "lib/runtime.py", "lib/fleetctl.py", "lib/dashboard.py", "lib/github_app.py", "lib/github_policy.py",
    "lib/git-hooks/pre-push",
    "lib/agent_files.py", "lib/workspaces.py", "lib/network.py", "lib/roles.py", "lib/medical.py", "lib/sessions.py",
    "lib/standard_channels.py",
    "lib/releases.tsv",
    *(f"lib/roles/{role}/{template}" for role in ("rear-admiral", "captain", "crew") for template in ("AGENTS.md", "SKILL.md")),
    "lib/roles/captain/CREW.md",
)
REMOTE = """
set -eu
umask 077
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
tar --no-same-owner --no-same-permissions -xf - -C "$stage"
sudo -n bash "$stage/install.sh" ACTION --config "$stage/fleet.json" --secrets "$stage/secrets.json" MODE
"""


def add(archive, name, contents, mode=0o600):
    entry = tarfile.TarInfo(name)
    entry.mode = mode
    entry.size = len(contents)
    archive.addfile(entry, io.BytesIO(contents))


def bundle(row, base):
    config = base / row["config"]
    secrets = base / row["secrets"]
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w") as archive:
        for name in FILES:
            add(archive, name, (PACKAGE / name).read_bytes(), 0o700)
        for source, name in ((config, "fleet.json"), (secrets, "secrets.json")):
            flags = os.O_NONBLOCK | os.O_NOFOLLOW
            with os.fdopen(os.open(source, flags), "rb") as stream:
                mode = os.fstat(stream.fileno()).st_mode
                require(stat.S_ISREG(mode), f"Expected a regular file at {source}.")
                if name == "secrets.json":
                    require(mode & 0o077 == 0, f"Set private permissions on {source} (chmod 600).")
                data = stream.read()
                if name == "fleet.json":
                    fleet = data
                add(archive, name, data)
        try:
            cfg = validate(json.loads(fleet), json.loads(data), check_files=False)
        except (ValueError, UnicodeError) as error:
            if isinstance(error, Invalid):
                raise
            raise Invalid("Cannot read the fleet configuration or private inputs.") from None
        keys_dir = base / row.get("secretsDir", config.with_suffix(".secrets"))
        for agent_id, key in app_keys(cfg["members"], keys_dir).items():
            add(archive, f"fleet.secrets/{agent_id}/github-app.pem", key.encode())
        for parts, contents in extra_sources(cfg, keys_dir).items():
            add(archive, "fleet.secrets/" + "/".join(parts), contents)
    return buffer.getvalue()


def deploy(host, payload, remote, log):
    with log.open("xb") as stream:
        try:
            result = subprocess.run(
                ["ssh", "-T", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes",
                 "-o", "ConnectTimeout=15", "--", host, remote],
                input=payload, stdout=stream, stderr=subprocess.STDOUT, check=False,
            )
        except OSError:
            stream.write(b"Could not execute ssh.\n")
            return False
        return result.returncode == 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("--parallel", type=int, default=4)
    parser.add_argument("--action", choices=("install", "upgrade"), default="upgrade")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--log-dir", type=Path)
    args = parser.parse_args()
    require(1 <= args.parallel <= 64, "--parallel must be between 1 and 64.")
    os.umask(0o077)
    rows = read_json(args.inventory)
    require(isinstance(rows, list) and rows, "Expected a non-empty inventory array.")
    jobs = {}
    for row in rows:
        row = fields(row, "inventory entry", "host config secrets secretsDir", ["host", "config", "secrets"])
        host = pattern(row["host"], r"(?:[a-zA-Z0-9_][a-zA-Z0-9_.-]*@)?[a-zA-Z0-9][a-zA-Z0-9_.:-]*", "SSH host")
        require(host not in jobs, "Each SSH host must be unique.")
        require(all(isinstance(row[key], str) and row[key] for key in row.keys() - {"host"}), "Inventory paths must be strings.")
        jobs[host] = bundle(row, args.inventory.resolve().parent)

    log_dir = args.log_dir or Path(tempfile.mkdtemp(prefix="subspace-fleet-"))
    if args.log_dir:
        log_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    # A per-run directory permits repeat dispatches and prevents log overwrite.
    log_dir = Path(tempfile.mkdtemp(prefix="run-", dir=log_dir))
    remote = REMOTE.replace("ACTION", args.action).replace("MODE", "--dry-run" if args.dry_run else "")
    print(f"Dispatch logs at {log_dir}", flush=True)
    with ThreadPoolExecutor(max_workers=args.parallel) as pool:
        runs = []
        for index, (host, payload) in enumerate(jobs.items(), 1):
            log = log_dir / f"{index:03}.log"
            runs.append((host, log, pool.submit(deploy, host, payload, remote, log)))
        successes = 0
        for host, log, future in runs:
            status = future.result()
            successes += status
            print(f"{'OK' if status else 'FAILED'} {host} ({log.name})", flush=True)
    print(f"{successes}/{len(jobs)} fleets completed.")
    return 0 if successes == len(jobs) else 1


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Invalid as error:
        sys.exit(f"subspace-fleet: {error}")
    except (OSError, ValueError):
        sys.exit("subspace-fleet: could not access the inventory inputs or logs.")
