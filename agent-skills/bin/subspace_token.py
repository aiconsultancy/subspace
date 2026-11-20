#!/usr/bin/env python3
"""Print a Subspace API bearer token for your local coding agents.

The token is yours: local agents act with your login and must say so (see the
subspace-tracker skill). It uses the API's development sign-in through the
fleet's tunnel (`./install.sh tunnel`), and caches the token until shortly
before it expires.

  subspace_token.py              print the token
  subspace_token.py --header     print {"Authorization": "Bearer ..."} (Claude Code headersHelper)
  subspace_token.py --me         print who the token signs in as
  subspace_token.py --target T   use another installed fleet (default from ~/.subspace/agents.env)
"""

import argparse
import json
import os
from pathlib import Path
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

HOME = Path(os.environ.get("SUBSPACE_HOME", Path.home() / ".subspace"))
API = os.environ.get("SUBSPACE_API_URL", "http://127.0.0.1:3000").rstrip("/").removesuffix("/api")


def read_env(path):
    values = {}
    if path.is_file():
        for line in path.read_text().splitlines():
            key, _, value = line.partition("=")
            if key and not key.startswith("#"):
                values[key.strip()] = value.strip()
    return values


def target_dir(name):
    name = name or read_env(HOME / "agents.env").get("SUBSPACE_TARGET") or "subspace-fleet"
    path = HOME / "targets" / name
    if not (path / "summary.env").is_file():
        sys.exit(f"subspace: no installed fleet '{name}' under {HOME / 'targets'}. Run the installer first.")
    return path


def call(method, path, token=None, body=None):
    data = None if body is None else json.dumps(body).encode()
    headers = {"Accept": "application/json", "Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    try:
        with urlopen(Request(f"{API}/api/v1{path}", data=data, headers=headers, method=method), timeout=15) as response:
            return json.load(response)
    except HTTPError as error:
        error.close()
        sys.exit(f"subspace: the Subspace API answered HTTP {error.code} for {path}.")
    except (URLError, OSError):
        sys.exit(f"subspace: cannot reach the Subspace API at {API}. Start the tunnel: ./install.sh tunnel --background")


def token(target):
    cache = target / "api-token.json"
    try:
        saved = json.loads(cache.read_text())
        if saved.get("expires", 0) - time.time() > 600:
            return saved["token"]
    except (OSError, ValueError, KeyError):
        pass
    summary = read_env(target / "summary.env")
    minted = call("POST", "/auth/dev/token", body={
        "email": summary.get("SUBSPACE_ADMIN_EMAIL", "developer@example.com"),
        "organisation": summary.get("SUBSPACE_ORGANISATION", "local"),
    })
    value = minted["accessToken"]
    descriptor = os.open(cache, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w") as handle:
        json.dump({"token": value, "expires": time.time() + int(minted.get("expiresIn", 3600))}, handle)
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--target")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--header", action="store_true")
    group.add_argument("--me", action="store_true")
    args = parser.parse_args()
    value = token(target_dir(args.target))
    if args.header:
        print(json.dumps({"Authorization": f"Bearer {value}"}))
    elif args.me:
        me = call("GET", "/me", token=value)
        print(f"{me.get('displayName')} <{me.get('email')}>")
    else:
        print(value)


if __name__ == "__main__":
    main()
