#!/usr/bin/python3
"""Authenticate Git and gh as the GitHub App assigned to this Linux user."""

import base64
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

from agent_files import agent_directory
from github_policy import PolicyError, check_push, cli_permissions, repository_name


class GitHubAppError(ValueError):
    pass


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_args):
        return None


def private_read(path):
    with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK), "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
            raise GitHubAppError("GitHub App files must be owned by their user and mode 0600.")
        data = stream.read(65537)
        if len(data) > 65536:
            raise GitHubAppError("GitHub App file exceeds 64 KiB.")
        return data


def private_write(path, data):
    try:
        if private_read(path) == data and stat.S_IMODE(path.stat().st_mode) == 0o600:
            return
    except (FileNotFoundError, GitHubAppError):
        pass
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as stream:
        temp = Path(stream.name)
        try:
            stream.write(data)
            stream.close()
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)


def provision(directory, packet):
    config = directory / "github-app.json"
    key = directory / "github-app.pem"
    policy = directory / "github-policy.json"
    if packet.get("metadata"):
        private_write(key, packet["key"].encode())
        private_write(config, (json.dumps(packet["metadata"], indent=2) + "\n").encode())
    elif packet["prune"]:
        config.unlink(missing_ok=True)
        key.unlink(missing_ok=True)
    private_write(policy, (json.dumps(packet["policy"], indent=2) + "\n").encode())


def encode(value):
    return base64.urlsafe_b64encode(value).rstrip(b"=")


def app_jwt(metadata, key):
    issued = int(time.time())
    header = encode(b'{"alg":"RS256","typ":"JWT"}')
    claims = encode(json.dumps({"iat": issued - 60, "exp": issued + 540, "iss": metadata["clientId"]}).encode())
    signing_input = header + b"." + claims
    result = subprocess.run(
        ["/usr/bin/openssl", "dgst", "-sha256", "-sign", str(key), "-passin", "pass:"],
        input=signing_input, capture_output=True, check=True,
    )
    return (signing_input + b"." + encode(result.stdout)).decode()


def installation_token(directory, permissions, repositories=None, opener=None):
    metadata = json.loads(private_read(directory / "github-app.json"))
    key = directory / "github-app.pem"
    private_read(key)
    installation = metadata["installationId"]
    if type(installation) is not int or installation < 1:
        raise GitHubAppError("Invalid GitHub App installation ID.")
    body = {"permissions": permissions}
    if repositories:
        body["repositories"] = repositories
    request = Request(
        f"https://api.github.com/app/installations/{installation}/access_tokens",
        data=json.dumps(body).encode(), method="POST", headers={
            "Authorization": "Bearer " + app_jwt(metadata, key),
            "Accept": "application/vnd.github+json",
            "Content-Type": "application/json",
            "X-GitHub-Api-Version": "2022-11-28",
            "User-Agent": "subspace-fleet",
        },
    )
    try:
        with (opener or build_opener(NoRedirect())).open(request, timeout=20) as response:
            token = json.load(response).get("token")
    except HTTPError as error:
        error.close()
        raise GitHubAppError(f"GitHub App authentication returned HTTP {error.code}.") from None
    except (URLError, OSError, ValueError):
        raise GitHubAppError("GitHub App authentication failed.") from None
    if not isinstance(token, str) or not re.fullmatch(r"[A-Za-z0-9_.-]{10,4096}", token):
        raise GitHubAppError("GitHub returned an invalid installation token.")
    return token


def credential(directory, operation, read_only=False):
    if operation != "get":
        return
    fields = {}
    for line in sys.stdin:
        if not line.strip():
            break
        key, separator, value = line.rstrip("\n").partition("=")
        if separator:
            fields[key] = value
    if fields.get("protocol") == "https" and fields.get("host") == "github.com" and fields.get("path"):
        policy = active_policy(directory)
        repo = repository_name(fields["path"])
        token = installation_token(directory, {"contents": "read" if read_only else policy["contents"]}, [repo])
        print("username=x-access-token")
        print("password=" + token)


def active_policy(directory):
    policy = json.loads(private_read(directory / "github-policy.json"))
    if not policy["enabled"]:
        raise GitHubAppError("No active GitHub App is configured for this member.")
    return policy


def main(args):
    os.umask(0o077)
    directory = Path.home() / ".config/subspace"
    command, *rest = args
    if command == "provision" and not rest:
        provision(agent_directory(".config", "subspace"), json.load(sys.stdin))
    elif command in {"credential", "credential-read"} and len(rest) == 1:
        credential(directory, rest[0], read_only=command == "credential-read")
    elif command == "bootstrap-token" and len(rest) == 1:
        active_policy(directory)
        url = urlsplit(rest[0])
        if url.scheme != "https" or url.netloc != "github.com":
            raise GitHubAppError("Bootstrap requires a GitHub HTTPS repository URL.")
        print(installation_token(directory, {"contents": "read"}, [repository_name(url.path.removeprefix("/"))]))
    elif command == "pre-push" and len(rest) == 2:
        check_push(rest[1], active_policy(directory)["protectedBranches"], sys.stdin)
    elif command == "gh" and rest:
        active_policy(directory)
        permissions = cli_permissions(rest)
        env = {key: value for key, value in os.environ.items() if key not in
               {"GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"}}
        env.update(GH_TOKEN=installation_token(directory, permissions), GH_HOST="github.com", GH_PROMPT_DISABLED="1")
        os.execve("/usr/bin/gh", ["gh", *rest], env)
    else:
        raise GitHubAppError("Usage: subspace-github gh <command> [args].")


if __name__ == "__main__":
    try:
        main(sys.argv[1:] or ["help"])
    except (GitHubAppError, PolicyError) as error:
        sys.exit(f"subspace-github: {error}")
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError):
        sys.exit("subspace-github: could not use the local GitHub App configuration or key.")
