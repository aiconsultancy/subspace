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


def private_credentials(helper, source, member=None, timeout=None):
    if member is None:
        command = f"!{shlex.quote(str(helper))} credential-read"
        token = {}
    else:
        # The reference checkout belongs to a service UID. Mint its read-only
        # token as the rear admiral and hand it to Git without a file or argv.
        limit = ["/usr/bin/timeout", "--kill-after=2s", f"{timeout}s"] if timeout else []
        value = as_user(member, *limit, helper, "bootstrap-token", source, output=True)
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


def crew_worktree(home, worker, repo_id):
    """A crew member's permanent worktree of one repository, under its captain's home."""
    return home / "worktrees" / worker["id"] / repo_id


def prepare_member(person, home, repo_id, source, mirror, cache_user, helper):
    user = f"ss-{person['id']}"
    dest = home / "workspace" / repo_id
    start = mirror or source
    options = ("--shared",) if mirror else ()
    auth = private_credentials(helper, source) if mirror is None else None
    clone(user, start, dest, *options, env=auth)
    if git(user, "-C", dest, "remote", "get-url", "origin", output=True) == str(start):
        git(user, "-C", dest, "remote", "set-url", "origin", source)
    check_origin(user, dest, source)
    hook = helper.parent / "git-hooks/pre-push"
    # Crew worktrees share this checkout's hooks, configuration and credentials.
    as_user(user, "/usr/bin/python3", Path(__file__), "hook", dest / ".git", hook, "on" if "githubApp" in person else "off")
    remote_head = []

    def start_point():
        # Resolved at most once per repository, and only when a new crew branch needs it.
        if not remote_head:
            if mirror:
                remote_head.append(git(cache_user, "-C", mirror, "rev-parse", "HEAD", output=True))
            else:
                git(user, "-C", dest, "fetch", "--quiet", "origin", "HEAD", env=auth)
                remote_head.append(git(user, "-C", dest, "rev-parse", "FETCH_HEAD", output=True))
        return remote_head[0]

    for worker in person.get("crew", ()):
        prepare_crew(user, dest, crew_worktree(home, worker, repo_id), worker["branch"], start_point)


def registered_worktrees(user, checkout):
    """Map each registered worktree path to its porcelain attribute lines, such as prunable or locked."""
    entries = {}
    for block in git(user, "-C", checkout, "worktree", "list", "--porcelain", output=True).split("\n\n"):
        lines = block.splitlines()
        if lines and lines[0].startswith("worktree "):
            entries[lines[0].removeprefix("worktree ")] = lines[1:]
    return entries


def prepare_crew(user, checkout, dest, branch, start_point):
    """Add a crew member's worktree of the captain's checkout once; never reset or move existing work."""
    registered = registered_worktrees(user, checkout)
    present = dest.exists() or dest.is_symlink()
    if str(dest) in registered and not present:
        # Its directory was deleted: forget only this registration (Git refuses a locked one), keep the
        # branch and its commits, and add the worktree again below.
        git(user, "-C", checkout, "worktree", "remove", dest)
        registered.pop(str(dest))
    if str(dest) not in registered:
        if present:
            raise ValueError(f"{dest} exists but is not a worktree of {checkout}. Move it aside.")
        as_user(user, "/usr/bin/mkdir", "-p", "--", dest.parent)
        existing = git(user, "-C", checkout, "branch", "--list", branch, output=True)
        target = [dest, branch] if existing else ["-b", branch, dest, start_point()]
        git(user, "-C", checkout, "worktree", "add", "--quiet", *target)
    # Stargate requires each worker to share its project checkout's Git common directory.
    common = git(user, "-C", dest, "rev-parse", "--path-format=absolute", "--git-common-dir", output=True)
    if common != str(checkout / ".git"):
        raise ValueError(f"Crew worktree {dest} belongs to another repository.")


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
    """Forget records of crew worktrees that no longer exist; existing directories are never removed."""
    for person in people:
        if person["role"] == "rear_admiral":
            continue
        for repo_id in repos:
            path = homes[person["id"]] / "workspace" / repo_id
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
