"""Limit the GitHub CLI operations that may receive an installation token."""

import fnmatch
import re
import subprocess
from urllib.parse import urlsplit


class PolicyError(ValueError):
    pass


REPOSITORY = r"[A-Za-z0-9_-]+/[A-Za-z0-9_.-]+"
ACTIONS = {
    ("issue", "list"): ("issues", "read"),
    ("issue", "view"): ("issues", "read"),
    ("issue", "create"): ("issues", "write"),
    ("issue", "edit"): ("issues", "write"),
    ("issue", "comment"): ("issues", "write"),
    ("issue", "close"): ("issues", "write"),
    ("issue", "reopen"): ("issues", "write"),
    ("pr", "list"): ("pull_requests", "read"),
    ("pr", "view"): ("pull_requests", "read"),
    ("pr", "diff"): ("pull_requests", "read"),
    ("pr", "create"): ("pull_requests", "write"),
    ("pr", "edit"): ("pull_requests", "write"),
    ("pr", "comment"): ("pull_requests", "write"),
    ("pr", "review"): ("pull_requests", "write"),
    ("run", "list"): ("actions", "read"),
    ("run", "view"): ("actions", "read"),
    ("repo", "view"): ("contents", "read"),
}
READ_ENDPOINTS = {
    r"": "metadata", r"/branches(?:/[A-Za-z0-9_.-]+)?": "contents", r"/commits(?:/[A-Fa-f0-9]+)?": "contents",
    r"/issues(?:/[0-9]+(?:/comments)?)?": "issues", r"/pulls(?:/[0-9]+(?:/files|/reviews)?)?": "pull_requests",
    r"/actions/runs(?:/[0-9]+(?:/jobs)?)?": "actions",
}


def repository_name(path):
    if re.fullmatch(REPOSITORY, path) and path.rsplit("/", 1)[-1] not in {".", "..", ".git"}:
        return path.rsplit("/", 1)[-1].removesuffix(".git")
    raise PolicyError("Expected a GitHub owner/repository path.")


def github_repository(address):
    """A GitHub HTTPS repository's owner/repository, as Stargate reads a remote, or None."""
    parts = urlsplit(address)
    if parts.scheme != "https" or (parts.hostname or "").lower() != "github.com" or parts.query or parts.fragment:
        return None
    name = parts.path[1:].removesuffix("/").removesuffix(".git")
    valid = re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]{0,38}/[A-Za-z0-9_.-]{1,100}", name) and not re.search(r"/\.{1,2}$", name)
    return name if valid else None


def officer_github_repositories(cfg):
    return {key: name for key, source in cfg.get("repositories", {}).items() if (name := github_repository(source["url"]))}


def merge_repository(args, policy, checkout):
    """Resolve a squash merge to the repository assigned to this captain checkout."""
    repository = policy.get("captainMergeCheckouts", {}).get(str(checkout))
    if policy["contents"] != "write" or not repository:
        raise PolicyError("Pull request merging is unavailable here. Use a captain checkout with merging enabled.")
    if (len(args) != 6 or not re.fullmatch(r"[1-9][0-9]*", args[2])
            or args[3:5] != ["--squash", "--match-head-commit"] or not re.fullmatch(r"[0-9a-fA-F]{40}", args[5])):
        raise PolicyError("Use gh pr merge NUMBER --squash --match-head-commit FULL_SHA from the captain checkout.")
    return repository


def cli_permissions(args):
    if not args:
        raise PolicyError("Specify an allowed GitHub CLI command.")
    if args[0] == "api":
        # Require a literal repository endpoint and allow GET only.
        if len(args) < 2 or any(arg != "--paginate" for arg in args[2:]):
            raise PolicyError("gh api accepts a read-only repository endpoint and optional --paginate.")
        if any(part in {"", ".", ".."} for part in args[1].split("/")):
            raise PolicyError("Specify a literal GitHub API path.")
        for suffix, permission in READ_ENDPOINTS.items():
            if re.fullmatch(fr"repos/{REPOSITORY}{suffix}", args[1]):
                return {permission: "read"}
        raise PolicyError("That GitHub API endpoint is outside the fleet policy.")

    action = tuple(args[:2])
    if action not in ACTIONS and action != ("pr", "checks"):
        raise PolicyError("That GitHub CLI command is outside the fleet policy.")
    for index, arg in enumerate(args[2:], 2):
        if arg.split("=", 1)[0] in {"--admin", "--hostname"}:
            raise PolicyError("GitHub admin options and alternate hosts are unavailable.")
        if arg in {"-R", "--repo"}:
            if index + 1 == len(args):
                raise PolicyError("Missing GitHub repository.")
            repository_name(args[index + 1])
        elif arg.startswith(("--repo=", "-R")):
            repository_name(arg.removeprefix("--repo=").removeprefix("-R").lstrip("="))
    if action == ("pr", "checks"):
        return {"pull_requests": "read", "checks": "read", "statuses": "read"}
    permission, level = ACTIONS[action]
    return {permission: level}


def check_push(remote_url, protected, updates):
    parts = urlsplit(remote_url)
    if parts.scheme != "https" or parts.hostname != "github.com":
        return
    for update in updates:
        _, local, remote_ref, remote = update.split()
        if remote_ref.startswith("refs/heads/") and any(
                fnmatch.fnmatchcase(remote_ref.removeprefix("refs/heads/"), branch) for branch in protected):
            if not local.strip("0"):
                raise PolicyError(f"Deleting protected branch {remote_ref} is disallowed.")
            if remote.strip("0") and subprocess.run(
                    ["/usr/bin/git", "merge-base", "--is-ancestor", remote, local], check=False).returncode:
                raise PolicyError(f"Update to protected branch {remote_ref} must be a fast-forward. Fetch and review first.")
