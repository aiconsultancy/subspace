"""Print or install the desktop's local Channels settings."""

import argparse
from datetime import UTC, datetime
import json
import os
from pathlib import Path
import sys
from tempfile import NamedTemporaryFile


NETWORK_IDS = ("aurora", "borealis")
LOCAL_HOSTS = {"127.0.0.1", "::1", "[::1]", "localhost", "irc.localhost"}


def config_path():
    override = os.environ.get("SUBSPACE_STACK_DESKTOP_CONFIG_FILE")
    if override:
        return Path(override).expanduser()

    # Match the Framework's product config lookup. The debug bundle ID
    # does not affect the product's data directory.
    home = Path.home()
    root = home / ".config" / "subspace"
    if sys.platform == "darwin":
        if not (root / "config.json").exists():
            root = home / "Library" / "Application Support" / "subspace"
    elif os.environ.get("XDG_CONFIG_HOME"):
        root = Path(os.environ["XDG_CONFIG_HOME"]) / "subspace"
    return root / "irc" / "config.json"


def stack_config(password_file):
    def port(variable, default):
        try:
            value = int(os.environ.get(variable) or default)
            if 1 <= value <= 65535:
                return value
        except ValueError:
            pass
        raise ValueError(f"{variable} must be a port from 1 to 65535")

    networks = []
    for network_id, name, tls, listen_port in (
        ("aurora", "Aurora (local IRC)", False, port("IRC_PLAIN_PORT", 6667)),
        ("borealis", "Borealis (local IRC, TLS)", True, port("IRC_TLS_PORT", 6697)),
    ):
        networks.append({
            "id": network_id,
            "name": name,
            "host": "127.0.0.1",
            "port": listen_port,
            "tls": tls,
            "acceptInvalidCerts": tls,
            "nickname": "developer",
            "sasl": {"username": "developer", "passwordFile": password_file},
        })
    return {"networks": networks, "captainNotifications": False}


def matches_endpoint(saved, local):
    host = saved.get("host")
    return (
        isinstance(saved.get("id"), str)
        and isinstance(host, str)
        and host.lower().rstrip(".") in LOCAL_HOSTS
        and saved.get("port", 6697) == local["port"]
    )


def merge(config, local_config):
    if not isinstance(config, dict) or not isinstance(config.get("networks", []), list):
        raise ValueError("expected a JSON object with a networks array")

    networks = config.setdefault("networks", [])
    matched = set()
    for local in local_config["networks"]:
        candidates = [
            (index, entry) for index, entry in enumerate(networks)
            if index not in matched and isinstance(entry, dict)
        ]
        match = next(((i, n) for i, n in candidates if n.get("id") == local["id"]), None)
        if match is None or not matches_endpoint(match[1], local):
            # A stale managed ID must not move onto a hand-added endpoint.
            match = next((
                (i, n) for i, n in candidates
                if n.get("id") not in NETWORK_IDS and matches_endpoint(n, local)
            ), match)
        if match is None:
            matched.add(len(networks))
            networks.append(local)
            continue

        index, network = match
        matched.add(index)
        sasl = network.get("sasl")
        sasl = sasl if isinstance(sasl, dict) else {}
        # A saved inline password overrides passwordFile in the client.
        sasl.pop("password", None)
        sasl.pop("external", None)
        sasl.update(local["sasl"])
        for key in ("host", "port", "tls", "acceptInvalidCerts", "nickname"):
            network[key] = local[key]
        network["sasl"] = sasl
    return config


def encode(config):
    return json.dumps(config, indent=2, ensure_ascii=True, allow_nan=False) + "\n"


def install(path, local_config):
    if path.is_symlink():
        path = path.resolve()
    try:
        previous = path.read_bytes()
    except FileNotFoundError:
        previous = None
    config = local_config
    if previous is not None:
        try:
            config = merge(json.loads(previous), local_config)
        except (json.JSONDecodeError, UnicodeDecodeError):
            raise ValueError("could not read existing settings as JSON") from None

    os.umask(0o077)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with NamedTemporaryFile(mode="w", encoding="utf-8", dir=path.parent, delete_on_close=False) as temp:
        temp.write(encode(config))
        temp.close()
        if previous is not None:
            timestamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%S%fZ")
            backup = path.with_name(f"{path.name}.{timestamp}.bak")
            with backup.open("xb") as file:
                file.write(previous)
        os.replace(temp.name, path)
    print(f"{'Created' if previous is None else 'Updated'} IRC config at {path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("password_file")
    parser.add_argument("--install", action="store_true")
    args = parser.parse_args()
    try:
        config = stack_config(args.password_file)
    except ValueError as error:
        parser.error(str(error))
    if not args.install:
        print(encode(config), end="")
        return

    path = config_path()
    try:
        install(path, config)
    except (OSError, RuntimeError, ValueError) as error:
        sys.exit(f"local-stack: {path}: {error}")


if __name__ == "__main__":
    main()
