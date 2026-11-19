"""Print or install the desktop's local Channels settings."""

import argparse
from datetime import UTC, datetime
import json
import os
from pathlib import Path
import re
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


def is_loopback(network):
    host = network.get("host")
    return isinstance(host, str) and host.lower().rstrip(".") in LOCAL_HOSTS


def can_adopt(network):
    if any(network.get(key) for key in ("password", "caCert", "clientCert", "clientKey")):
        return False
    sasl = network.get("sasl")
    return sasl is None or (
        isinstance(sasl, dict) and not sasl.get("external")
        and (not sasl or sasl.get("username") == "developer")
    )


def managed_id(network_id, local_id):
    return network_id == local_id or re.fullmatch(rf"{local_id}-local(?:-[0-9]+)?", network_id) is not None


def free_id(network_id, used):
    if network_id not in used:
        return network_id
    candidate = f"{network_id}-local"
    suffix = 2
    while candidate in used:
        candidate = f"{network_id}-local-{suffix}"
        suffix += 1
    return candidate


def find_network(networks, matched, local):
    local_id = local["id"]
    on_port, stale, protected = [], [], False
    for index, network in enumerate(networks):
        if index in matched or not isinstance(network, dict) or not isinstance(network.get("id"), str):
            continue
        network_id = network["id"]
        managed = managed_id(network_id, local_id)
        loopback = is_loopback(network)
        endpoint = loopback and network.get("port", 6697) == local["port"]
        if not (managed or endpoint):
            continue
        if not loopback or not can_adopt(network):
            protected = True
            continue
        # Leave the other listener's managed entry available for its own pass.
        if any(other != local_id and managed_id(network_id, other) for other in NETWORK_IDS):
            continue
        if managed and endpoint:
            return index, protected
        if endpoint:
            on_port.append(index)
        elif managed:
            stale.append(index)
    return next(iter(on_port or stale), None), protected


def merge(config, local_config):
    if not isinstance(config, dict) or not isinstance(config.get("networks", []), list):
        raise ValueError("expected a JSON object with a networks array")

    networks = config.setdefault("networks", [])
    used = {entry["id"] for entry in networks if isinstance(entry, dict) and isinstance(entry.get("id"), str)}
    matched = set()
    for local in local_config["networks"]:
        index, protected = find_network(networks, matched, local)
        if index is None:
            network_id = free_id(local["id"], used)
            if protected:
                print(f"local-stack: kept existing IRC connection and added {network_id}", file=sys.stderr)
            used.add(network_id)
            matched.add(len(networks))
            networks.append({**local, "id": network_id})
            continue

        network = networks[index]
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


def keep_latest_backup(path):
    backups = sorted(
        backup for backup in path.parent.iterdir()
        if re.fullmatch(rf"{re.escape(path.name)}\.\d{{8}}T\d{{12}}Z\.bak", backup.name)
    )
    for backup in backups[:-1]:
        backup.unlink()


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

    encoded = encode(config).encode()
    if encoded == previous:
        keep_latest_backup(path)
        print(f"IRC config unchanged at {path}")
        return

    os.umask(0o077)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with NamedTemporaryFile(dir=path.parent, delete_on_close=False) as temp:
        temp.write(encoded)
        temp.close()
        if previous is not None:
            timestamp = datetime.now(UTC).strftime("%Y%m%dT%H%M%S%fZ")
            backup = path.with_name(f"{path.name}.{timestamp}.bak")
            with backup.open("xb") as file:
                file.write(previous)
        os.replace(temp.name, path)
    keep_latest_backup(path)
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
