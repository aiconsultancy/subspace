"""Keep the fleet installer's Stargate pin on Stargate's latest release.

Usage: python3 scripts/stargate-pin.py [--check | --update [--expect TAG] | --json
                                        | --pr-title STATUS | --pr-body STATUS --tests FILE]

The installer pins Stargate by commit: STARGATE_REF in
subspace-installer/lib/runtime.py, labelled with its release in
STARGATE_RELEASE. Every Subspace release carries the latest Stargate release.
This asks GitHub's API, and nothing else, for that release and the commit its
tag names (an annotated tag is followed to its commit), and for the commit the
pin's label names, so a label that no longer names the pinned commit shows.

  (none)     Report the pin and the latest release.
  --check    Also fail when the pin is not the latest release, unless a hold
             covers it (below).
  --update   Move STARGATE_RELEASE and STARGATE_REF to the latest release and
             change nothing else. --expect refuses if the latest is another tag.
  --json     The state as JSON, for .github/workflows/stargate-bump.yml.
  --pr-title, --pr-body
             The bump pull request's title and description, from saved --json
             output and a Markdown file of the installer's test results.

A hold, subspace-installer/stargate-hold.json, keeps the pin on its release
for a reviewed reason:

  {"pinned": "v0.43.0", "holding": "v0.44.0",
   "reason": "v0.44.0 renames the crew plugin's options; see #123.",
   "until": "2026-12-31"}

It applies only while the pin is `pinned`, the latest release is `holding`
and the date is on or before `until`. --check then passes with the reason as a
warning. A newer release, or the date passing, ends it, and a hold that no
longer describes the pin is reported and ignored.

GH_TOKEN or GITHUB_TOKEN is sent when set; otherwise the requests are
unauthenticated. Exit status: 0 the pin is current or held, 1 the pin is
behind (--check), 2 the pin, the hold or an answer from GitHub is not as
expected, 3 GitHub's API cannot be reached, 4 GitHub's API answered with an
error.
"""

import argparse
import datetime
import http.client
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNTIME = ROOT / "subspace-installer/lib/runtime.py"
HOLD = ROOT / "subspace-installer/stargate-hold.json"
API = "https://api.github.com"
STARGATE = "aiconsultancy/stargate"
TIMEOUT = 20
RELEASE_LINE = re.compile(r'^STARGATE_RELEASE = "(?P<value>[^"]*)"$', re.M)
REF_LINE = re.compile(r'^STARGATE_REF = "(?P<value>[^"]*)"$', re.M)
VERSION = re.compile(r"v(\d+)\.(\d+)\.(\d+)")
SHA = re.compile(r"[0-9a-f]{40}")
HOLD_KEYS = {"pinned", "holding", "reason", "until"}

OK, BEHIND, INVALID, UNREACHABLE, API_ERROR = 0, 1, 2, 3, 4


class Invalid(Exception):
    """The pin, the hold or an answer from GitHub is not as expected."""


class Unreachable(Exception):
    """GitHub's API could not be reached."""


class ApiError(Exception):
    """GitHub's API answered with an HTTP error."""

    def __init__(self, message: str, status: int):
        super().__init__(message)
        self.status = status


def version(tag: str) -> tuple[int, int, int] | None:
    match = VERSION.fullmatch(tag or "")
    return tuple(int(part) for part in match.groups()) if match else None


class GitHub:
    """GET requests to Stargate's repository on GitHub's API, and nowhere else."""

    def __init__(self, token: str | None = None, opener=urllib.request.urlopen):
        self.token = token
        self.opener = opener

    def get(self, path: str):
        if not path.startswith(f"/repos/{STARGATE}/"):
            raise ValueError(f"not a request for {STARGATE}: {path}")
        headers = {"Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28",
                   "User-Agent": "subspace-stargate-pin"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        request = urllib.request.Request(API + path, headers=headers)
        try:
            with self.opener(request, timeout=TIMEOUT) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            hint = ""
            if error.code in (403, 429) and error.headers and error.headers.get("x-ratelimit-remaining") == "0":
                hint = " (rate limited; set GH_TOKEN)" if not self.token else " (rate limited)"
            raise ApiError(f"GitHub's API answered {error.code} {error.reason} for {path}{hint}",
                           error.code) from None
        except (urllib.error.URLError, http.client.HTTPException, OSError) as error:
            reason = getattr(error, "reason", error)
            raise Unreachable(f"cannot reach GitHub's API at {API}: {reason}") from None
        except ValueError as error:
            raise Invalid(f"GitHub's API sent something other than JSON for {path}: {error}") from None

    def commit_of(self, tag: str) -> str | None:
        """The commit a tag names, following annotated tags; None when there is no such tag."""
        try:
            target = self.get(f"/repos/{STARGATE}/git/ref/tags/{urllib.parse.quote(tag, safe='')}")["object"]
        except ApiError as error:
            if error.status == 404:
                return None
            raise
        for _ in range(5):
            if target.get("type") == "commit" and SHA.fullmatch(target.get("sha", "")):
                return target["sha"]
            if target.get("type") != "tag":
                raise Invalid(f"Stargate's tag {tag} names a {target.get('type')}, not a commit")
            target = self.get(f"/repos/{STARGATE}/git/tags/{target['sha']}")["object"]
        raise Invalid(f"Stargate's tag {tag} is nested too deeply")

    def latest(self) -> dict:
        release = self.get(f"/repos/{STARGATE}/releases/latest")
        tag = release.get("tag_name", "")
        if version(tag) is None:
            raise Invalid(f"Stargate's latest release has the tag {tag!r}, not vX.Y.Z")
        return {"tag": tag, "url": release.get("html_url") or release_url(tag)}

    def releases(self) -> list[dict]:
        found = []
        for release in self.get(f"/repos/{STARGATE}/releases?per_page=100"):
            tag = release.get("tag_name", "")
            if version(tag) and not release.get("draft") and not release.get("prerelease"):
                found.append({"tag": tag, "url": release.get("html_url") or release_url(tag)})
        return found


def release_url(tag: str) -> str:
    return f"https://github.com/{STARGATE}/releases/tag/{urllib.parse.quote(tag)}"


def read_pin(text: str) -> dict:
    """STARGATE_RELEASE and STARGATE_REF from runtime.py's text."""
    found = {}
    for key, name, pattern, valid, kind in (("release", "STARGATE_RELEASE", RELEASE_LINE, version, "a vX.Y.Z tag"),
                                            ("sha", "STARGATE_REF", REF_LINE, SHA.fullmatch, "a full commit")):
        matches = pattern.findall(text)
        if len(matches) != 1:
            raise Invalid(f"runtime.py must assign {name} on exactly one line, not {len(matches)}")
        if not valid(matches[0]):
            raise Invalid(f"runtime.py's {name} is not {kind}: {matches[0]!r}")
        found[key] = matches[0]
    return found


def write_pin(text: str, release: str, sha: str) -> str:
    """runtime.py's text with the pin moved; nothing else changes."""
    read_pin(text)
    if version(release) is None or not SHA.fullmatch(sha):
        raise Invalid(f"not a release and commit: {release} {sha}")
    text = RELEASE_LINE.sub(f'STARGATE_RELEASE = "{release}"', text)
    return REF_LINE.sub(f'STARGATE_REF = "{sha}"', text)


def read_hold(text: str | None) -> dict | None:
    """The hold, checked for shape; None when there is none."""
    if text is None:
        return None
    try:
        hold = json.loads(text)
    except ValueError as error:
        raise Invalid(f"stargate-hold.json is not JSON: {error}") from None
    if not isinstance(hold, dict) or set(hold) != HOLD_KEYS:
        raise Invalid(f"stargate-hold.json must have exactly the keys {', '.join(sorted(HOLD_KEYS))}")
    for key in ("pinned", "holding"):
        if not isinstance(hold[key], str) or version(hold[key]) is None:
            raise Invalid(f"stargate-hold.json's {key} must be a vX.Y.Z tag")
    if version(hold["holding"]) <= version(hold["pinned"]):
        raise Invalid("stargate-hold.json's holding must be newer than its pinned")
    if not isinstance(hold["reason"], str) or len(hold["reason"].strip()) < 10:
        raise Invalid("stargate-hold.json needs a reason a reviewer can read")
    try:
        datetime.date.fromisoformat(hold["until"])
    except (TypeError, ValueError):
        raise Invalid("stargate-hold.json's until must be a date, YYYY-MM-DD") from None
    return hold


def state(github: GitHub, pin: dict, hold: dict | None, today: datetime.date) -> dict:
    """Everything the report, the check and the bump pull request need."""
    latest = github.latest()
    latest["sha"] = github.commit_of(latest["tag"])
    if latest["sha"] is None:
        raise Invalid(f"Stargate's latest release {latest['tag']} has no tag")
    labelled = latest["sha"] if pin["release"] == latest["tag"] else github.commit_of(pin["release"])
    pinned = {"sha": pin["sha"], "release": pin["release"], "labelled": labelled,
              "tag": pin["release"] if labelled == pin["sha"] else None}
    result = {"repository": STARGATE, "pinned": pinned, "latest": latest, "between": [],
              "hold": hold, "notes": [], "state": "current"}
    if pin["sha"] == latest["sha"]:
        if pinned["tag"] is None:
            raise Invalid(f"the pin is {latest['tag']}'s commit but STARGATE_RELEASE says "
                          f"{pin['release']}; run scripts/stargate-pin.py --update")
        if hold:
            result["notes"].append("stargate-hold.json is no longer needed: remove it.")
        return result
    after = version(pinned["tag"]) if pinned["tag"] else None
    newest = version(latest["tag"])
    result["between"] = sorted((release for release in github.releases()
                                if version(release["tag"]) <= newest
                                and (after is None or version(release["tag"]) > after)),
                               key=lambda release: version(release["tag"]))
    if not any(release["tag"] == latest["tag"] for release in result["between"]):
        result["between"].append({"tag": latest["tag"], "url": latest["url"]})
    result["state"] = "behind"
    if hold:
        until = datetime.date.fromisoformat(hold["until"])
        if hold["pinned"] != pinned["tag"]:
            result["notes"].append(f"stargate-hold.json holds {hold['pinned']}, which is not the pin; "
                                   "it is ignored: remove it.")
        elif hold["holding"] != latest["tag"]:
            result["notes"].append(f"stargate-hold.json covers {hold['holding']} only; "
                                   f"{latest['tag']} is newer, so the hold has ended.")
        elif today > until:
            result["notes"].append(f"stargate-hold.json lapsed after {hold['until']}.")
        else:
            result["state"] = "held"
    return result


def short(sha: str | None) -> str:
    return sha[:12] if sha else "none"


def describe_pin(pinned: dict) -> str:
    if pinned["tag"]:
        return pinned["tag"]
    if pinned["labelled"] is None:
        return f"not a release (labelled {pinned['release']}, which Stargate has no tag for)"
    return f"not a release (labelled {pinned['release']}, whose tag is at {short(pinned['labelled'])})"


def report(result: dict) -> list[str]:
    pinned, latest = result["pinned"], result["latest"]
    lines = [f"pinned {pinned['sha']}, which is {describe_pin(pinned)}; "
             f"the latest release is {latest['tag']} at {latest['sha']}"]
    if result["state"] == "current":
        lines.append("The pin is Stargate's latest release.")
    elif result["state"] == "held":
        hold = result["hold"]
        lines.append(f"HELD: the pin stays on {hold['pinned']} instead of {hold['holding']} until "
                     f"{hold['until']}, by subspace-installer/stargate-hold.json: {hold['reason']}")
    else:
        lines.append("The pin is behind. Merge the stargate-bump pull request, or run "
                     "`python3 scripts/stargate-pin.py --update` and check the release notes: "
                     + ", ".join(release["tag"] for release in result["between"]) + ".")
    return lines + result["notes"]


def annotate(result: dict, lines: list[str], check: bool) -> None:
    """GitHub Actions annotations and step summary, when running there."""
    if os.environ.get("GITHUB_ACTIONS") != "true":
        return
    level = {"current": "notice", "held": "warning"}.get(result["state"], "error" if check else "warning")
    print(f"::{level} title=Stargate pin::{lines[0]}. {lines[1]}".replace("\n", " "))
    for note in result["notes"]:
        print(f"::warning title=Stargate pin::{note}")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as file:
            file.write("### Stargate pin\n\n" + "\n".join(f"- {line}" for line in lines) + "\n")


def pr_title(result: dict) -> str:
    source = result["pinned"]["tag"] or short(result["pinned"]["sha"])
    return f"build(installer): move Stargate from {source} to {result['latest']['tag']}"


def pr_body(result: dict, tests: str, stale: list[str], run_url: str | None,
            repository: str = "aiconsultancy/subspace") -> str:
    pinned, latest = result["pinned"], result["latest"]
    guide = f"https://github.com/{repository}/blob/main/subspace-installer/README.md#stargate-pin"
    notes = "\n".join(f"- [{release['tag']}]({release['url']})" for release in result["between"])
    stale_files = ("these files still name the old commit; move each link to the new one, or keep it "
                   "with a reason:\n" + "\n".join(f"  - `{name}`" for name in stale)
                   if stale else "no other file names the old commit.")
    made = f"by the [stargate-bump workflow]({run_url})" if run_url else "by the stargate-bump workflow"
    checklist = [
        "**Generated configuration keys**: every key `runtime.py` writes into Stargate's configuration is "
        "still accepted, and none it now requires is missing.",
        "**Service and path names**: the entrypoints (`src/main.ts`, `src/fleet`), unit names, state "
        "directories and sockets the installer refers to still exist.",
        "**The crew plugin's options**: the options the installer passes to Stargate's crew plugin keep "
        "their names and meanings.",
        "**The Fleet API's CORS origins**: `FLEET_API_ORIGINS` and `stargate.api.corsOrigins` are still "
        "allowed, and the API's defaults have not changed under them.",
        "**Mesij**: the Mesij version this Stargate release expects matches the `mesij` rows in "
        "`subspace-installer/lib/releases.tsv`.",
        f"**Docs links that name the pin**: {stale_files}",
        *(["Remove `subspace-installer/stargate-hold.json`: it held the old release."] if result["hold"] else []),
        "Run the installer's tests (`python3 -B subspace-installer/tests/policy_test.py`, the other "
        "`*_test.py` there, `bash -n` and `shellcheck`, and `subspace-installer/tests/smoke.sh`), then install "
        "a test fleet on a VM from this branch (`sudo ./install.sh upgrade`) and check the gateways, crew "
        "mail and the Fleet API.",
    ]
    # One line a paragraph: GitHub shows each line break in a pull request's description.
    sections = [
        f"Stargate has a newer release than the fleet installer's pin. This moves `STARGATE_RELEASE` and "
        f"`STARGATE_REF` in `subspace-installer/lib/runtime.py` and nothing else. It was opened {made}, which "
        f"never merges anything. Every Subspace release carries Stargate's latest release; see "
        f"[Stargate pin]({guide}).",
        "| | Release | Commit |\n| --- | --- | --- |\n"
        f"| From | {pinned['tag'] or 'not a release'} | `{pinned['sha']}` |\n"
        f"| To | [{latest['tag']}]({latest['url']}) | `{latest['sha']}` |",
        "## Release notes",
        "Read each release's notes for changes the installer has to follow:",
        notes,
        "## Installer tests on this pull request's commit",
        tests.strip() or "No results were recorded.",
        "The workflow ran these. A pull request opened or pushed with the workflow's token starts no "
        "`pull_request` checks: push a commit to this branch, or close and reopen the pull request, to start "
        "them.",
        "## Before merging",
        "The installer generates Stargate's configuration, units and plugin options, so each must match the "
        "new release. Check, against the release notes and Stargate's source at the new commit:",
        "\n".join(f"- [ ] {item}" for item in checklist),
        "If this release cannot be adopted yet, close this pull request and add a hold instead: see "
        f"[Stargate pin]({guide}).",
    ]
    return "\n\n".join(sections) + "\n"


def stale_files(sha: str) -> list[str]:
    """Tracked files other than runtime.py that still name a commit."""
    result = subprocess.run(["git", "-C", str(ROOT), "grep", "-l", "-F", sha, "--", ".",
                             ":!subspace-installer/lib/runtime.py"], capture_output=True, text=True)
    return result.stdout.split()


def main(argv: list[str] | None = None, github: GitHub | None = None,
         today: datetime.date | None = None) -> int:
    parser = argparse.ArgumentParser(description="Keep the installer's Stargate pin on the latest release.")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--update", action="store_true")
    mode.add_argument("--json", action="store_true")
    mode.add_argument("--pr-title", metavar="STATUS")
    mode.add_argument("--pr-body", metavar="STATUS")
    parser.add_argument("--expect", metavar="TAG")
    parser.add_argument("--tests", metavar="FILE")
    options = parser.parse_args(argv)
    if options.expect and not options.update:
        parser.error("--expect goes with --update")
    if bool(options.tests) != bool(options.pr_body):
        parser.error("--pr-body and --tests go together")
    if options.pr_title or options.pr_body:
        result = json.loads(Path(options.pr_title or options.pr_body).read_text())
        if options.pr_title:
            print(pr_title(result))
        else:
            server, repository, run = (os.environ.get(name) for name in
                                       ("GITHUB_SERVER_URL", "GITHUB_REPOSITORY", "GITHUB_RUN_ID"))
            run_url = f"{server}/{repository}/actions/runs/{run}" if server and repository and run else None
            print(pr_body(result, Path(options.tests).read_text(), stale_files(result["pinned"]["sha"]),
                          run_url, repository or "aiconsultancy/subspace"), end="")
        return OK

    github = github or GitHub(os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN") or None)
    today = today or datetime.date.today()
    try:
        text = RUNTIME.read_text()
        pin = read_pin(text)
        hold = read_hold(HOLD.read_text() if HOLD.exists() else None)
        result = state(github, pin, hold, today)
    except Invalid as error:
        print(f"stargate-pin: {error}", file=sys.stderr)
        return INVALID
    except Unreachable as error:
        print(f"stargate-pin: {error}. The pin was not checked.", file=sys.stderr)
        return UNREACHABLE
    except ApiError as error:
        print(f"stargate-pin: {error}. The pin was not checked.", file=sys.stderr)
        return API_ERROR

    if options.json:
        print(json.dumps(result, indent=2))
        return OK
    if options.update:
        latest = result["latest"]
        if options.expect and latest["tag"] != options.expect:
            print(f"stargate-pin: the latest release is {latest['tag']}, not {options.expect}", file=sys.stderr)
            return INVALID
        if result["state"] == "current":
            print(f"The pin is already {latest['tag']} at {latest['sha']}.")
            return OK
        RUNTIME.write_text(write_pin(text, latest["tag"], latest["sha"]))
        print(f"Moved the pin from {describe_pin(result['pinned'])} ({result['pinned']['sha']}) "
              f"to {latest['tag']} ({latest['sha']}).")
        return OK
    lines = report(result)
    print("\n".join(lines))
    annotate(result, lines, options.check)
    return BEHIND if options.check and result["state"] == "behind" else OK


if __name__ == "__main__":
    sys.exit(main())
