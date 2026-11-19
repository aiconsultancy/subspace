"""Publish every Framework crate to crates.io in dependency order.

Usage: scripts/publish.sh [--dry-run] [--ci]

A real run refuses unless the tree is clean, HEAD is on a remote branch,
`make check` and `make publish-check` passed for HEAD (their stamps in
.local/checks/) and CARGO_REGISTRY_TOKEN is set. Cargo reads the token from
the environment; this script never prints it and never runs `cargo login`.

`--ci` is for the publish-framework GitHub workflow, whose own jobs are the
gate: instead of the remote branch and the local stamps, it requires GitHub
Actions and a run on the release tag, `subspace-framework-v<version>`.

Crates whose version is already on crates.io are skipped, so an interrupted
run can be re-run. crates.io limits new crates to a burst of 5, then one
every 10 minutes, and new versions to a burst of 30, then one a minute: on
HTTP 429 the script waits and retries, and it estimates the time up front.
After each upload it waits until the version is in the index, so dependents
resolve. `--dry-run` does everything except the upload, using
`cargo publish --workspace --dry-run`, and reports what a real run would
refuse instead of stopping.

Environment, for tests and other registries:
  SUBSPACE_PUBLISH_ROOT      the Framework folder (default: this script's parent)
  SUBSPACE_PUBLISH_API       registry web API (default: https://crates.io)
  SUBSPACE_PUBLISH_INDEX     sparse index (default: https://index.crates.io)
  SUBSPACE_PUBLISH_TIME_SCALE  multiplies every wait; 0 in tests (default: 1)
"""

import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(os.environ.get("SUBSPACE_PUBLISH_ROOT", Path(__file__).resolve().parents[1]))
API = os.environ.get("SUBSPACE_PUBLISH_API", "https://crates.io").rstrip("/")
INDEX = os.environ.get("SUBSPACE_PUBLISH_INDEX", "https://index.crates.io").rstrip("/")
SCALE = float(os.environ.get("SUBSPACE_PUBLISH_TIME_SCALE", "1"))
USER_AGENT = "subspace-framework-publish (scripts/publish.sh)"
NEW_BURST, NEW_INTERVAL = 5, 600
UPDATE_BURST, UPDATE_INTERVAL = 30, 60
INDEX_POLL, INDEX_TIMEOUT = 5, 900
MAX_ATTEMPTS = 40
STAMPS = (".local/checks/check-{sha}", ".local/checks/publish-check-{sha}")
FACADE = "subspace-framework"
RELEASE_TAG = "subspace-framework-v{version}"
FLAGS = {"--dry-run", "--ci"}


@dataclass
class Crate:
    name: str
    version: str
    state: str = "unknown"  # new, update or present
    result: str = ""


@dataclass
class Plan:
    crates: list[Crate] = field(default_factory=list)
    refusals: list[str] = field(default_factory=list)


def run(args: list[str], check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=check)


def say(line: str) -> None:
    print(line, flush=True)


def publish_order(packages: list[dict]) -> list[dict]:
    """Workspace packages, each after every workspace package it depends on
    (normal, build and dev edges), ties broken by name."""
    names = {package["name"]: package for package in packages}
    waiting = {name: {dependency["name"] for dependency in package["dependencies"]
                      if dependency["name"] in names and dependency["name"] != name}
               for name, package in names.items()}
    order = []
    while waiting:
        ready = sorted(name for name, needs in waiting.items() if not needs)
        if not ready:
            raise SystemExit(f"publish: dependency cycle among {sorted(waiting)}")
        for name in ready:
            order.append(names[name])
            del waiting[name]
        for needs in waiting.values():
            needs.difference_update(ready)
    return order


def workspace_packages() -> list[dict]:
    output = run(["cargo", "metadata", "--format-version", "1", "--no-deps", "--locked"]).stdout
    return [package for package in json.loads(output)["packages"]
            if package.get("publish") != []]


def fetch(url: str) -> tuple[int, str]:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return response.status, response.read().decode()
    except urllib.error.HTTPError as error:
        return error.code, ""


def registry_state(crate: Crate) -> str:
    """`present` when this version is on crates.io, `update` when only other
    versions are, `new` when the crate is not."""
    status, body = fetch(f"{API}/api/v1/crates/{crate.name}")
    if status == 404:
        return "new"
    if status != 200:
        raise SystemExit(f"publish: {API} answered {status} for {crate.name}")
    versions = {version["num"] for version in json.loads(body).get("versions", [])}
    return "present" if crate.version in versions else "update"


def index_path(name: str) -> str:
    if len(name) <= 2:
        return f"{len(name)}/{name}"
    if len(name) == 3:
        return f"3/{name[0]}/{name}"
    return f"{name[:2]}/{name[2:4]}/{name}"


def in_index(crate: Crate) -> bool:
    status, body = fetch(f"{INDEX}/{index_path(crate.name)}")
    if status != 200:
        return False
    return any(json.loads(line).get("vers") == crate.version
               for line in body.splitlines() if line.strip())


def estimate_minutes(crates: list[Crate]) -> int:
    new = sum(crate.state == "new" for crate in crates)
    updates = sum(crate.state == "update" for crate in crates)
    seconds = (max(0, new - NEW_BURST) * NEW_INTERVAL
               + max(0, updates - UPDATE_BURST) * UPDATE_INTERVAL)
    return round(seconds / 60)


def local_refusals() -> list[str]:
    """A run on a developer's machine: HEAD pushed and both checks stamped."""
    found = []
    sha = run(["git", "rev-parse", "HEAD"]).stdout.strip()
    if not run(["git", "branch", "-r", "--contains", sha]).stdout.strip():
        found.append(f"HEAD {sha[:12]} is not on a remote branch (push, then fetch)")
    for stamp in STAMPS:
        if not (ROOT / stamp.format(sha=sha)).is_file():
            target = "make " + stamp.split("/")[-1].rsplit("-", 1)[0]
            found.append(f"`{target}` has not passed for {sha[:12]}")
    return found


def ci_refusals(version: str) -> list[str]:
    """A run in the publish-framework workflow: on the release tag."""
    found = []
    if os.environ.get("GITHUB_ACTIONS") != "true":
        found.append("--ci runs only in GitHub Actions")
    tag = RELEASE_TAG.format(version=version)
    ref = os.environ.get("GITHUB_REF_NAME", "")
    if ref != tag:
        found.append(f"the workflow ran on `{ref}`, not the release tag `{tag}`")
    return found


def refusals(ci: bool, version: str) -> list[str]:
    """What a real run refuses: a dirty tree, the local or CI conditions, no
    token."""
    found = []
    if run(["git", "status", "--porcelain", "--", "."]).stdout.strip():
        found.append("the tree has uncommitted changes")
    found.extend(ci_refusals(version) if ci else local_refusals())
    if not os.environ.get("CARGO_REGISTRY_TOKEN"):
        found.append("CARGO_REGISTRY_TOKEN is not set")
    return found


def plan(ci: bool) -> Plan:
    packages = publish_order(workspace_packages())
    version = next((package["version"] for package in packages if package["name"] == FACADE), "")
    result = Plan(refusals=refusals(ci, version))
    result.crates = [Crate(package["name"], package["version"]) for package in packages]
    for crate in result.crates:
        crate.state = registry_state(crate)
    return result


def rate_limited(output: str) -> bool:
    text = output.lower()
    return "429" in text or "too many requests" in text or "too many new crates" in text


def wait(seconds: float, line: str) -> None:
    say(line)
    time.sleep(seconds * SCALE)


def upload(crate: Crate) -> bool:
    interval = NEW_INTERVAL if crate.state == "new" else UPDATE_INTERVAL
    for attempt in range(1, MAX_ATTEMPTS + 1):
        result = run(["cargo", "publish", "-p", crate.name, "--locked", "--no-verify"],
                     check=False)
        if result.returncode == 0:
            return True
        output = result.stdout + result.stderr
        if not rate_limited(output):
            say(output.strip())
            return False
        wait(interval, f"  {crate.name}: crates.io rate limit, attempt {attempt}; "
                       f"retrying in {interval // 60} min")
    return False


def await_index(crate: Crate) -> bool:
    waited = 0
    while not in_index(crate):
        if waited >= INDEX_TIMEOUT:
            return False
        wait(INDEX_POLL, f"  {crate.name} {crate.version}: waiting for the index")
        waited += INDEX_POLL
    return True


def publish(crates: list[Crate]) -> bool:
    for position, crate in enumerate(crates, 1):
        prefix = f"[{position}/{len(crates)}] {crate.name} {crate.version}"
        if crate.state == "present":
            crate.result = "already on crates.io"
            say(f"{prefix}: already on crates.io, skipped")
            continue
        say(f"{prefix}: publishing ({crate.state} crate)")
        if not upload(crate) or not await_index(crate):
            crate.result = "failed"
            return False
        crate.result = "published"
    return True


def dry_run() -> bool:
    result = run(["cargo", "publish", "--workspace", "--dry-run", "--no-verify",
                  "--allow-dirty", "--locked"], check=False)
    if result.returncode != 0:
        say((result.stdout + result.stderr).strip())
        return False
    return True


def summary(crates: list[Crate], dry: bool) -> None:
    say("Summary:")
    for crate in crates:
        outcome = crate.result or ("would publish" if dry and crate.state != "present"
                                   else "not reached")
        say(f"  {crate.name} {crate.version}: {outcome}")


def main(arguments: list[str]) -> int:
    if set(arguments) - FLAGS or len(set(arguments)) != len(arguments):
        say(__doc__.split("\n\n")[1])
        return 2
    dry = "--dry-run" in arguments
    current = plan(ci="--ci" in arguments)
    pending = [crate for crate in current.crates if crate.state != "present"]
    say(f"{len(current.crates)} crates; {sum(c.state == 'new' for c in pending)} new, "
        f"{sum(c.state == 'update' for c in pending)} new versions, "
        f"{len(current.crates) - len(pending)} already on crates.io.")
    say(f"Estimated time: about {estimate_minutes(current.crates)} min, "
        "from crates.io's rate limits.")
    say("Order: " + ", ".join(crate.name for crate in current.crates))
    for refusal in current.refusals:
        say(f"{'A real run would refuse' if dry else 'Refusing'}: {refusal}")
    if dry:
        for crate in current.crates:
            crate.result = "already on crates.io" if crate.state == "present" else ""
        passed = dry_run()
        summary(current.crates, dry=True)
        say("Dry run passed; nothing was uploaded." if passed else "Dry run failed.")
        return 0 if passed else 1
    if current.refusals:
        return 1
    passed = publish(current.crates)
    summary(current.crates, dry=False)
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
