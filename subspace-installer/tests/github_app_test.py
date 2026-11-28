"""Verify per-user key provisioning and GitHub installation auth without calling GitHub."""

import base64
import contextlib
import io
import json
from pathlib import Path
import pwd
import shutil
import stat
import subprocess
import sys
import tarfile
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import github_app
from dispatch import bundle


def prepare(work, source):
    config = json.loads(Path(source).read_text())
    people = [config["rearAdmiral"], *config["captains"]]
    for index, person in enumerate(people, 1):
        member = person["id"]
        person["githubApp"] = {
            "appId": index, "installationId": index + 10, "clientId": f"fixture-client-{member}",
            "botLogin": f"example-{member}[bot]", "botId": index + 20,
        }
        directory = work / "app-keys" / member
        directory.mkdir(mode=0o700, parents=True)
        key = directory / "github-app.pem"
        subprocess.run(
            ["openssl", "genrsa", "-out", str(key), "2048"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True,
        )
        key.chmod(0o600)
    if config.get("medical", {}).get("enabled"):
        shutil.copytree(work / "fleet.secrets" / "medical", work / "app-keys" / "medical")
    (work / "github-fleet.json").write_text(json.dumps(config))


def decode(value):
    return base64.urlsafe_b64decode(value + "=" * (-len(value) % 4))


class GitHubFixture:
    def __init__(self, public, work, metadata):
        self.public = public
        self.signature = work / "jwt-signature"
        self.metadata = metadata

    def open(self, request, timeout):
        assert timeout == 20
        assert request.get_method() == "POST"
        assert request.full_url == f"https://api.github.com/app/installations/{self.metadata['installationId']}/access_tokens"
        assert json.loads(request.data) == {"permissions": {"contents": "read"}, "repositories": ["fixture"]}
        scheme, jwt = request.get_header("Authorization").split()
        assert scheme == "Bearer"
        header, claims, signature = jwt.split(".")
        assert json.loads(decode(header))["alg"] == "RS256"
        assert json.loads(decode(claims))["iss"] == self.metadata["clientId"]
        self.signature.write_bytes(decode(signature))
        subprocess.run(
            ["openssl", "dgst", "-sha256", "-verify", str(self.public), "-signature", str(self.signature)],
            input=f"{header}.{claims}".encode(), stdout=subprocess.DEVNULL, check=True,
        )
        return io.BytesIO(b'{"token":"fixture-installation-token"}')


def check(work):
    expected = json.loads((work / "github-fleet.json").read_text())
    payload = bundle({"config": "github-fleet.json", "secrets": "fleet.secrets.json", "secretsDir": "app-keys"}, work)
    with tarfile.open(fileobj=io.BytesIO(payload)) as archive:
        for member in ("atlas", "orion"):
            entry = archive.getmember(f"fleet.secrets/{member}/github-app.pem")
            assert entry.mode == 0o600
            assert archive.extractfile(entry).read() == (work / "app-keys" / member / "github-app.pem").read_bytes()
        if expected.get("medical", {}).get("enabled"):
            for name in ("sentry-auth-token", "irc-password"):
                entry = archive.getmember(f"fleet.secrets/medical/{name}")
                assert entry.mode == 0o600
                assert archive.extractfile(entry).read() == (work / "app-keys" / "medical" / name).read_bytes()
    for person in (expected["rearAdmiral"], *expected["captains"]):
        user = f"ss-{person['id']}"
        identity = pwd.getpwnam(user)
        directory = Path(identity.pw_dir) / ".config/subspace"
        key = directory / "github-app.pem"
        metadata = directory / "github-app.json"
        for private in (metadata, key):
            assert stat.S_IMODE(private.stat().st_mode) == 0o600
            assert private.stat().st_uid == identity.pw_uid
        assert json.loads(metadata.read_text()) == person["githubApp"]
        assert key.read_bytes() == (work / "app-keys" / person["id"] / key.name).read_bytes()
        git = ["runuser", "-u", user, "--", "git", "config", "--global", "--get"]
        helper = subprocess.check_output([*git, "credential.https://github.com.helper"], text=True)
        assert helper.strip().endswith("/github_app.py credential")
        assert subprocess.check_output([*git, "user.name"], text=True).strip() == person["githubApp"]["botLogin"]
        policy = json.loads((directory / "github-policy.json").read_text())
        assert policy["contents"] == ("read" if person["id"] == "atlas" else "write")
        if person["id"] == "orion":
            checkout = Path(identity.pw_dir) / "workspace/sample"
            hook = checkout / ".git/hooks/pre-push"
            assert hook.is_symlink()
            denied = subprocess.run(
                ["runuser", "-u", user, "--", str(hook), "origin", "https://github.com/example/fixture.git"],
                cwd=checkout, text=True, capture_output=True,
                input=f"(delete) {'0' * 40} refs/heads/main {'a' * 40}\n",
            )
            assert denied.returncode == 1 and "Deleting protected branch" in denied.stderr

        public = work / "github-app.pub"
        subprocess.run(["openssl", "pkey", "-in", str(key), "-pubout", "-out", str(public)], check=True)
        opener = GitHubFixture(public, work, person["githubApp"])
        # Signature and HTTP request shape are checked against a synthetic response.
        with patch.object(github_app, "private_read", lambda path: path.read_bytes()):
            assert github_app.installation_token(directory, {"contents": "read"}, ["fixture"], opener=opener) == "fixture-installation-token"

    with patch.object(github_app, "installation_token", return_value="fixture-installation-token") as mint:
        output = io.StringIO()
        with contextlib.redirect_stdout(output), patch.object(sys, "stdin", io.StringIO("protocol=https\nhost=example.com\n\n")):
            github_app.credential(directory, "get")
        assert not output.getvalue() and not mint.called
        with patch.object(github_app, "private_read", lambda path: path.read_bytes()):
            with contextlib.redirect_stdout(output), patch.object(sys, "stdin", io.StringIO("protocol=https\nhost=github.com\npath=example/fixture.git\n\n")):
                github_app.credential(directory, "get", read_only=True)
            mint.assert_called_once_with(directory, {"contents": "read"}, ["fixture"])
            with patch.object(Path, "home", return_value=directory.parents[1]), patch.object(github_app.os, "execve") as gh:
                github_app.main(["gh", "issue", "list"])
            assert gh.call_args.args[2]["GH_TOKEN"] == "fixture-installation-token"
        assert "password=fixture-installation-token" in output.getvalue()
    workflow_scopes(directory)
    print("Per-agent GitHub App files, commit identities, RS256 minting, credential scope and gh environment passed.")


def workflow_scopes(directory):
    """Workflows write joins Contents only on a push token for a listed repository."""
    policy = {"enabled": True, "contents": "write", "protectedBranches": ["main"],
              "workflowRepositories": ["Example/Fixture"]}
    for path, read_only, contents, expected in (
            ("example/fixture.git", False, "write", {"contents": "write", "workflows": "write"}),
            ("example/fixture.git", True, "write", {"contents": "read"}),
            ("example/fixture.git", False, "read", {"contents": "read"}),
            ("example/other.git", False, "write", {"contents": "write"})):
        with patch.object(github_app, "active_policy", return_value={**policy, "contents": contents}), \
                patch.object(github_app, "installation_token", return_value="fixture-installation-token") as mint, \
                patch.object(sys, "stdin", io.StringIO(f"protocol=https\nhost=github.com\npath={path}\n\n")), \
                contextlib.redirect_stdout(io.StringIO()):
            github_app.credential(directory, "get", read_only=read_only)
        assert mint.call_args.args[1] == expected, (path, read_only, contents, mint.call_args.args[1])
    with patch.object(github_app, "active_policy", return_value=policy), \
            patch.object(github_app, "installation_token", return_value="fixture-installation-token") as mint, \
            contextlib.redirect_stdout(io.StringIO()):
        github_app.main(["bootstrap-token", "https://github.com/example/fixture.git"])
    assert mint.call_args.args[1] == {"contents": "read"}


if __name__ == "__main__":
    command, work, *args = sys.argv[1:]
    globals()[command](Path(work), *args)
