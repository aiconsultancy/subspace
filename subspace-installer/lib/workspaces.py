"""Prepare and refresh fleet Git workspaces with per-user access."""

import os
from pathlib import Path
import shlex
import subprocess
import tempfile


def as_user(user, *args, output=False, env=None, shared=False):
    result = subprocess.run(
        ["/usr/sbin/runuser", "-u", user, "--", *map(str, args)], umask=0o022 if shared else 0o077,
        env={"PATH": "/usr/bin:/bin", "GIT_TERMINAL_PROMPT": "0", **(env or {})},
        check=True, text=True, stdout=subprocess.PIPE if output else subprocess.DEVNULL,
    )
    return result.stdout.strip() if output else None


def git(user, *args, output=False, env=None, shared=False):
    return as_user(user, "/usr/bin/git", *args, output=output, env=env, shared=shared)


def clone(user, source, dest, *options, env=None):
    # Do not dereference a destination chosen by an existing checkout's owner.
    if dest.exists() or dest.is_symlink():
        return
    as_user(user, "/usr/bin/python3", Path(__file__), "clone", source, dest, *options, env=env)


def trusted_upload_pack(source):
    # The cache has a different UID. Trust only this cache path in upload-pack.
    return f"git -c {shlex.quote(f'safe.directory={source}')} upload-pack"


def clone_as_user(source, dest, *options):
    with tempfile.TemporaryDirectory(dir=dest.parent) as temp:
        checkout = Path(temp) / "clone"
        if "--shared" in options:
            options = (*options, f"--upload-pack={trusted_upload_pack(source)}")
        subprocess.run(
            ["/usr/bin/git", "clone", "--quiet", *options, "--", source, str(checkout)], check=True,
        )
        checkout.rename(dest)


def check_origin(user, repo, source):
    if git(user, "-C", repo, "remote", "get-url", "origin", output=True) != str(source):
        raise ValueError(f"Repository {repo.name} already has a different URL. Use a new repository ID.")


def private_credentials(helper, source, member=None):
    if member is None:
        command = f"!{shlex.quote(str(helper))} credential-read"
        token = {}
    else:
        # The reference checkout belongs to a service UID. Mint its read-only
        # token as the rear admiral and hand it to Git without a file or argv.
        value = as_user(member, helper, "bootstrap-token", source, output=True)
        token = {"SUBSPACE_GIT_TOKEN": value}
        command = """!f() { printf 'username=x-access-token\npassword=%s\n' "$SUBSPACE_GIT_TOKEN"; }; f"""
    return {
        **token, "GIT_CONFIG_COUNT": "4",
        "GIT_CONFIG_KEY_0": "credential.helper", "GIT_CONFIG_VALUE_0": "",
        "GIT_CONFIG_KEY_1": "credential.https://github.com.helper", "GIT_CONFIG_VALUE_1": command,
        "GIT_CONFIG_KEY_2": "http.followRedirects", "GIT_CONFIG_VALUE_2": "false",
        "GIT_CONFIG_KEY_3": "credential.useHttpPath", "GIT_CONFIG_VALUE_3": "true",
    }


def refresh_mirror(user, mirror, source):
    clone(user, source, mirror, "--bare")
    check_origin(user, mirror, source)
    head = git(user, "-C", mirror, "ls-remote", "--symref", "origin", "HEAD", output=True)
    # Existing alternates may still need objects from deleted or rebased refs.
    git(user, "-C", mirror, "-c", "gc.auto=0", "-c", "maintenance.auto=false",
        "fetch", "--quiet", "--prune", "origin", "+refs/heads/*:refs/heads/*", shared=True)
    for line in head.splitlines():
        ref, sep, name = line.partition("\t")
        if sep and name == "HEAD" and ref.startswith("ref: refs/heads/"):
            git(user, "-C", mirror, "symbolic-ref", "HEAD", ref.removeprefix("ref: "), shared=True)
            break


def refresh_reference(user, dest, source, mirror, helper, member):
    env = private_credentials(helper, source, member) if mirror is None else None
    remote = mirror or source
    options = ("--shared",) if mirror else ()
    clone(user, remote, dest, *options, env=env)
    check_origin(user, dest, remote)
    upload = (f"--upload-pack={trusted_upload_pack(mirror)}",) if mirror else ()
    git(user, "-C", dest, "fetch", "--quiet", *upload, "origin", "HEAD", env=env, shared=True)
    latest = git(user, "-C", dest, "rev-parse", "FETCH_HEAD", output=True)
    current = git(user, "-C", dest, "rev-parse", "HEAD", output=True)
    if current != latest:
        git(user, "-C", dest, "reset", "--quiet", "--hard", latest, shared=True)
    # Git's trust setting is scoped to this checkout in the reader's home.
    safe = subprocess.run(
        ["runuser", "-u", member, "--", "/usr/bin/git", "config", "--global", "--get-all", "safe.directory"],
        capture_output=True, text=True, check=False,
    )
    if str(dest) not in safe.stdout.splitlines():
        git(member, "config", "--global", "--add", "safe.directory", dest)


def prepare_member(person, home, repo_id, source, mirror, cache_user, helper):
    agent_id = person["id"]
    user = f"ss-{agent_id}"
    dest = home / "workspace" / repo_id
    start = mirror or source
    options = ("--shared",) if mirror else ()
    auth = private_credentials(helper, source) if mirror is None else None
    if person["role"] == "crew":
        backing = home / f".{repo_id}.git"
        clone(user, start, backing, *options, "--bare", env=auth)
        if git(user, "-C", backing, "remote", "get-url", "origin", output=True) == str(start):
            git(user, "-C", backing, "remote", "set-url", "origin", source)
        check_origin(user, backing, source)
        worktrees = git(user, "-C", backing, "worktree", "list", "--porcelain", output=True)
        if f"worktree {dest}" not in worktrees.splitlines():
            if mirror:
                base = git(cache_user, "-C", mirror, "rev-parse", "HEAD", output=True)
            else:
                git(user, "-C", backing, "fetch", "--quiet", "origin", "HEAD", env=auth)
                base = "FETCH_HEAD"
            branches = git(user, "-C", backing, "branch", "--list", f"crew/{agent_id}", output=True)
            branch = ["-b", f"crew/{agent_id}", dest, base] if not branches else [dest, f"crew/{agent_id}"]
            git(user, "-C", backing, "worktree", "add", "--quiet", *branch)
        common = git(user, "-C", dest, "rev-parse", "--path-format=absolute", "--git-common-dir", output=True)
        if common != str(backing):
            raise ValueError(f"Crew checkout {dest} is outside its assigned repository.")
        git_dir = backing
    else:
        clone(user, start, dest, *options, env=auth)
        if git(user, "-C", dest, "remote", "get-url", "origin", output=True) == str(start):
            git(user, "-C", dest, "remote", "set-url", "origin", source)
        check_origin(user, dest, source)
        git_dir = dest / ".git"
    hook = helper.parent / "git-hooks/pre-push"
    as_user(user, "/usr/bin/python3", Path(__file__), "hook", git_dir, hook, "on" if "githubApp" in person else "off")


def manage_hook(git_dir, source, enabled):
    target = git_dir / "hooks/pre-push"
    if enabled:
        target.parent.mkdir(exist_ok=True)
        try:
            target.symlink_to(source)
        except FileExistsError:
            pass
    elif target.is_symlink() and target.readlink() == source:
        target.unlink()


def prepare_workspaces(repos, people, homes, cache, cache_user, references, helper, refresh_only=False):
    rear = f"ss-{people[0]['id']}"
    for repo_id, repo in repos.items():
        source = repo["url"]
        mirror = None if repo["private"] else cache / f"{repo_id}.git"
        if mirror:
            refresh_mirror(cache_user, mirror, source)
        refresh_reference(cache_user, references / repo_id, source, mirror, helper, rear)
        if not refresh_only:
            for person in people[1:]:
                prepare_member(person, homes[person["id"]], repo_id, source, mirror, cache_user, helper)


def prune_worktrees(repos, people, homes, days):
    for person in people:
        if person["role"] == "rear_admiral":
            continue
        home = homes[person["id"]]
        for repo_id in repos:
            path = home / (f".{repo_id}.git" if person["role"] == "crew" else f"workspace/{repo_id}")
            if path.is_dir():
                git(f"ss-{person['id']}", "-C", path, "worktree", "prune", f"--expire={days}.days.ago")


if __name__ == "__main__":
    import sys

    _, action, source, dest, *options = sys.argv
    if action == "clone":
        # A private mirror lives below an agent-only home or reference directory.
        os.umask(0o022)
        clone_as_user(source, Path(dest), *options)
    elif action == "hook":
        manage_hook(Path(source), Path(dest), options == ["on"])
