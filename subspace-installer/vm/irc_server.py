#!/usr/bin/python3
"""Run one Subspace IRC server (Ergo) that several fleets share.

With a shared server, every fleet keeps its own standard channels (Starbase,
fleet command and one ship channel per captain) on one network. The desktop
stays connected to that one network, and picking an initiative in Channels
joins that initiative's channels on its fleet, so
switching between fleets is just switching initiative. Stargate on each fleet
VM connects its agents to their channels with their own SASL accounts.

Commands (as root, through `subspace-setup irc-server`):
  setup [options]   install or update the server (the default command)
  add FILE|-        add or update a fleet from its irc-bundle.json and restart Ergo
  remove SLUG       stop seeding a fleet's accounts (existing accounts remain)
  ca                print the server's TLS certificate for fleets and desktops
  secret ACCOUNT    print a human account's password
  status            show the fleets, humans and container state
"""

import argparse
import ipaddress
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys

from fleet_setup import PAYLOAD, SetupError, read_json, require, run, say, write_json, write_private
import standard_channels  # On the path through fleet_setup, from the installer's lib.

HERE = Path(__file__).resolve().parent
STATE = Path("/root/irc")
ETC = Path("/etc/subspace-irc")
SOURCE = Path("/opt/subspace/irc")
PROJECT = "subspace-irc"
ERGO_GID = 10001
ACCOUNT = re.compile(r"[a-z][a-z0-9_-]{0,31}")
CHANNEL = re.compile(r"#[^\s,:\x00-\x1f]{1,49}")
HOSTNAME = re.compile(r"[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+")
OPERATOR = "ircop"
DB_PROJECT = "subspace-irc-db"
POSTGRES_IMAGE = "postgres:17.12-alpine@sha256:d3244e8ce3f165ee3c93d9c4a4b1d0fe48b3a89e2287b717fdcbbde92d405065"


def compose(*args, capture=False, check=True):
    return run("docker", "compose", "--project-name", PROJECT, "--project-directory", SOURCE,
               "--env-file", ETC / "compose.env", "-f", SOURCE / "compose.yaml", "-f", ETC / "override.yaml",
               *args, capture=capture, check=check)


def addresses():
    output = run("ip", "-o", "addr", "show", "scope", "global", capture=True)
    found = []
    for line in output.splitlines():
        parts = line.split()
        found.append((parts[1], parts[3].split("/")[0]))
    return found


PRIVATE_NETWORKS = [ipaddress.ip_network(network) for network in
                    ("10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "100.64.0.0/10", "fc00::/7")]


def private_ip(value):
    """RFC 1918, carrier-grade NAT (Tailscale) or IPv6 unique local; never loopback or public."""
    try:
        address = ipaddress.ip_address(value)
    except ValueError:
        return False
    return any(address.version == network.version and address in network for network in PRIVATE_NETWORKS)


def gather(args, saved):
    interactive = sys.stdin.isatty() and not args.non_interactive
    answers = dict(saved)

    def ask(label, default):
        if not interactive:
            return default
        value = input(f"{label}{f' [{default}]' if default else ''}: ").strip()
        return value or default

    if args.server_name:
        answers["serverName"] = args.server_name.lower()
    elif "serverName" not in answers:
        answers["serverName"] = (ask("DNS name fleets and desktops use for this server (for example irc.example.ts.net)", "") or "").lower()
    if args.bind:
        answers["bind"] = args.bind
    elif "bind" not in answers:
        candidates = addresses()
        tailnet = [ip for name, ip in candidates if name == "tailscale0" and ":" not in ip]
        private = [ip for _, ip in candidates if private_ip(ip) and ":" not in ip]
        answers["bind"] = ask("Private address for the TLS listener", (tailnet or private or [""])[0])
    answers["port"] = args.port or answers.get("port", 6697)
    answers["history"] = args.history or answers.get("history", "postgresql")
    humans = [name.strip().lower() for name in (args.admin or "").split(",") if name.strip()]
    if humans:
        answers["humans"] = sorted(set(answers.get("humans", [])) | set(humans))
    elif not answers.get("humans"):
        answers["humans"] = [(ask("Your IRC account name", "admiral") or "admiral").lower()]
    for key, value in (("tlsCert", args.tls_cert), ("tlsKey", args.tls_key)):
        if value:
            answers[key] = value
    answers["operator"] = args.operator or answers.get("operator") or os.environ.get("SUDO_USER") or ""
    answers["access"] = args.access or answers.get("access", "ssh")
    if args.ssh_from is not None:
        answers["sshFrom"] = [value.strip() for value in args.ssh_from.split(",") if value.strip()]
    answers.setdefault("sshFrom", [])
    if args.lockdown is not None:
        answers["lockdown"] = args.lockdown
    answers.setdefault("lockdown", True)
    return answers


def check(answers):
    require(HOSTNAME.fullmatch(answers["serverName"] or ""),
            "Give the server's DNS name with --server-name. Fleets verify its TLS certificate against that name.")
    require(private_ip(answers["bind"]),
            "Bind the TLS listener to a private address (RFC 1918, Tailscale 100.64.0.0/10 or IPv6 ULA) with --bind.")
    require(any(ip == answers["bind"] for _, ip in addresses()), f"No interface on this host has {answers['bind']}.")
    require(isinstance(answers["port"], int) and 1 <= answers["port"] <= 65535, "Invalid --port.")
    for human in answers["humans"]:
        require(ACCOUNT.fullmatch(human) and human != OPERATOR, f"Invalid IRC account {human}.")
    require(("tlsCert" in answers) == ("tlsKey" in answers), "Give both --tls-cert and --tls-key, or neither.")
    for key in ("tlsCert", "tlsKey"):
        if key in answers:
            require(Path(answers[key]).is_absolute() and Path(answers[key]).is_file(), f"{answers[key]} is not a file.")
    require(answers["operator"] and answers["operator"] != "root", "Pass --operator with the Linux login that SSHes in.")


def generated():
    return read_json(STATE / "generated-secrets.json", {})


def password(key):
    values = generated()
    if key not in values:
        values[key] = secrets.token_hex(24)
        write_json(STATE / "generated-secrets.json", values)
    return values[key]


def ergo_file(path, text):
    """Write a file Ergo (UID and GID 10001) reads through a read-only bind mount."""
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    write_private(path, text)
    os.chown(path, 0, ERGO_GID)
    os.chmod(path, 0o440)


def database(answers):
    """Ergo's history in its own PostgreSQL container on this host (the default), or SQLite."""
    if answers["history"] != "postgresql":
        return None
    secret = password("ergo-history")
    # Readable by the database container's user; the 0700 directory keeps it private on the host.
    path = ETC / "db-password"
    write_private(path, secret)
    os.chmod(path, 0o444)
    spec = {
        "name": DB_PROJECT,
        "services": {"postgres": {
            "image": POSTGRES_IMAGE, "restart": "unless-stopped",
            "environment": {"POSTGRES_DB": "ergo_history", "POSTGRES_USER": "ergo", "POSTGRES_PASSWORD_FILE": "/run/secrets/db-password"},
            "volumes": ["subspace_irc_postgres:/var/lib/postgresql/data", f"{path}:/run/secrets/db-password:ro"],
            "healthcheck": {"test": ["CMD", "pg_isready", "-U", "ergo", "-d", "ergo_history"], "interval": "2s", "timeout": "3s", "retries": 30},
        }},
        "volumes": {"subspace_irc_postgres": {}},
    }
    write_private(ETC / "db-compose.json", json.dumps(spec, indent=2))
    run("docker", "compose", "-f", ETC / "db-compose.json", "up", "--detach", "--wait", "--wait-timeout", "120")
    return {"password": secret, "network": f"{DB_PROJECT}_default"}


def render(answers, db=None):
    fleets = {path.stem: read_json(path) for path in sorted((STATE / "fleets").glob("*.json"))}
    accounts, owners, channels = {}, {}, {}
    for human in answers["humans"]:
        accounts[human] = password(f"irc-{human}")
        owners[human] = "humans"
    for slug, bundle in fleets.items():
        for row in bundle["accounts"]:
            name = row["name"]
            require(name not in owners or owners[name] == slug,
                    f"IRC account {name} is claimed by both {owners.get(name)} and {slug}.")
            accounts[name], owners[name] = row["password"], slug
        for row in bundle["channels"]:
            channels.setdefault(row["name"], row.get("topic", ""))
    ergo_file(ETC / "accounts.json", json.dumps([{"name": k, "password": v} for k, v in sorted(accounts.items())], indent=2))
    ergo_file(ETC / "channels.json", json.dumps(
        [{"name": name, **({"topic": topic} if topic else {})} for name, topic in sorted(channels.items())], indent=2))
    ergo_file(ETC / "operator-password", password("irc-operator") + "\n")
    bind = f"[{answers['bind']}]" if ":" in answers["bind"] else answers["bind"]
    write_private(ETC / "compose.env", "".join(f"{key}={value}\n" for key, value in {
        "IRC_SERVER_NAME": answers["serverName"],
        "IRC_PLAIN_BIND": "127.0.0.1", "IRC_PLAIN_PORT": 6667,
        "IRC_TLS_BIND": bind, "IRC_TLS_PORT": answers["port"],
        "IRC_WS_BIND": "127.0.0.1", "IRC_WS_PORT": 8097,
    }.items()))
    volumes = [f"{ETC / 'accounts.json'}:/run/subspace/accounts.json:ro",
               f"{ETC / 'channels.json'}:/run/subspace/channels.json:ro",
               f"{ETC / 'operator-password'}:/run/subspace/operator-password:ro"]
    environment = {
        "IRC_ACCOUNTS_FILE": "/run/subspace/accounts.json", "IRC_CHANNELS_FILE": "/run/subspace/channels.json",
        "IRC_OPER_NAME": OPERATOR, "IRC_OPER_PASSWORD_FILE": "/run/subspace/operator-password",
    }
    if "tlsCert" in answers:
        for key, variable, name in (("tlsCert", "IRC_TLS_CERT", "fullchain.pem"), ("tlsKey", "IRC_TLS_KEY", "privkey.pem")):
            ergo_file(ETC / name, Path(answers[key]).read_text())
            volumes.append(f"{ETC / name}:/run/subspace/{name}:ro")
            environment[variable] = f"/run/subspace/{name}"
    service = {"environment": environment, "volumes": volumes}
    override = {"services": {"irc": service}}
    environment["IRC_HISTORY_DATABASE"] = "postgresql" if db else "sqlite"
    if db:
        ergo_file(ETC / "history-password", db["password"])
        volumes.append(f"{ETC / 'history-password'}:/run/subspace/history-password:ro")
        environment.update({"IRC_POSTGRES_HOST": "postgres", "IRC_POSTGRES_PASSWORD_FILE": "/run/subspace/history-password"})
        service["networks"] = ["default", db["network"]]
        override["networks"] = {db["network"]: {"external": True}}
    write_private(ETC / "override.yaml", json.dumps(override, indent=2))
    return fleets


def restart():
    say("starting Ergo (fleets reconnect automatically)")
    compose("up", "--detach", "--wait", "--wait-timeout", "120", "--build", "--force-recreate")


def setup(args):
    require(os.geteuid() == 0, "Run as root (sudo).")
    STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    (STATE / "fleets").mkdir(mode=0o700, exist_ok=True)
    answers = gather(args, read_json(STATE / "answers.json", {}))
    check(answers)
    write_json(STATE / "answers.json", answers)
    run(HERE / "docker.sh")
    staging = SOURCE.with_name("irc.new")
    shutil.rmtree(staging, ignore_errors=True)
    shutil.copytree(PAYLOAD / "subspace-irc", staging, ignore=shutil.ignore_patterns(".env", "__pycache__"))
    shutil.rmtree(SOURCE, ignore_errors=True)
    staging.rename(SOURCE)
    ETC.mkdir(mode=0o700, exist_ok=True)
    # No agents run here, but keep everyone but root and the operator off cloud metadata from the start.
    run(HERE / "lockdown.sh", "isolate", "--operator", answers["operator"], "--protect-metadata")
    render(answers, database(answers))
    restart()
    lockdown_id = ""
    if answers["lockdown"]:
        command = [HERE / "lockdown.sh", "apply", "--operator", answers["operator"], "--allow-port", str(answers["port"]),
                   "--protect-metadata"]
        for address in answers["sshFrom"]:
            command += ["--ssh-from", address]
        if answers["access"] in ("tailscale", "cloudflare"):
            command.append(f"--{answers['access']}")
        run(*command)
        lockdown_id = run(HERE / "lockdown.sh", "applied-id", capture=True)
    write_private(STATE / "summary.env", "".join(f"{key}={value}\n" for key, value in {
        "SUBSPACE_IRC_HOST": answers["serverName"], "SUBSPACE_IRC_PORT": answers["port"], "SUBSPACE_IRC_TLS": 1,
        "SUBSPACE_IRC_SELF_SIGNED": 0 if "tlsCert" in answers else 1,
        "SUBSPACE_IRC_HUMANS": " ".join(answers["humans"]), "SUBSPACE_OPERATOR": answers["operator"],
        "SUBSPACE_LOCKDOWN": 1 if answers["lockdown"] else 0, "SUBSPACE_LOCKDOWN_ID": lockdown_id,
    }.items()))
    if run(HERE / "audit.sh", check=False):
        say("WARNING: the privacy audit found problems (FAIL lines above). Fix them before adding fleets.")
    say(f"shared IRC server {answers['serverName']}:{answers['port']} (TLS) is up on {answers['bind']}.")
    say("add each fleet with 'subspace-setup irc-server add <irc-bundle.json>' (the host installer does this).")


def add(source):
    answers = read_json(STATE / "answers.json")
    require(answers, "Set the server up first.")
    text = sys.stdin.read() if source == "-" else Path(source).read_text()
    bundle = json.loads(text)
    slug = bundle.get("fleet", "")
    require(re.fullmatch(r"[a-z0-9][a-z0-9-]{0,34}", slug), "The bundle has no valid fleet ID.")
    for row in bundle.get("accounts", []):
        require(ACCOUNT.fullmatch(row.get("name", "")) and row["name"] not in answers["humans"] and row["name"] != OPERATOR,
                f"Invalid or reserved fleet account {row.get('name')}.")
        require(isinstance(row.get("password"), str) and 1 <= len(row["password"].encode()) <= 72
                and not re.search(r"\s", row["password"]), f"Invalid password for {row['name']}.")
    for row in bundle.get("channels", []):
        require(CHANNEL.fullmatch(row.get("name", "")), f"Invalid channel {row.get('name')}.")
    names = {row["name"] for row in bundle.get("channels", [])}
    require(set(standard_channels.standard(slug, [])) <= names,
            f"The bundle for fleet {slug} lacks its standard channels ({standard_channels.summary(slug)}). "
            "Make the bundle again with this version of subspace-setup.")
    conflicts(slug, bundle)
    humans = [name for name in bundle.get("humans", []) if ACCOUNT.fullmatch(name)]
    missing = [name for name in humans if name not in answers["humans"]]
    path = STATE / "fleets" / f"{slug}.json"
    previous = path.read_text() if path.is_file() else None
    saved_answers = (STATE / "answers.json").read_text()
    if missing:
        answers["humans"] = sorted(set(answers["humans"]) | set(missing))
        write_json(STATE / "answers.json", answers)
    write_json(path, {key: bundle[key] for key in ("fleet", "accounts", "channels")})
    try:
        render(answers, database(answers))
        restart()
    except SetupError:
        # Ergo refuses to start when a seeded password differs from an existing account's.
        say("Ergo did not accept the fleet's accounts; restoring the previous accounts and channels")
        if previous is None:
            path.unlink()
        else:
            write_private(path, previous)
        write_private(STATE / "answers.json", saved_answers)
        answers = read_json(STATE / "answers.json")
        render(answers, database(answers))
        restart()
        raise SetupError(f"Could not add fleet {slug}. If Ergo already has some of its accounts with other "
                         "passwords (for example from a fleet VM that was rebuilt), drop those accounts first "
                         "or give the fleet a new ID.") from None
    if missing:
        say(f"added human account(s) {', '.join(missing)}; read a password with 'subspace-setup irc-server secret <account>'")
    say(f"fleet {slug} added: {len(bundle['accounts'])} accounts, {len(bundle['channels'])} channels.")


def conflicts(slug, bundle):
    """Refuse a bundle that would take another fleet's accounts or change this fleet's passwords."""
    accounts = {row["name"] for row in bundle["accounts"]}
    for path in sorted((STATE / "fleets").glob("*.json")):
        accounts |= {row["name"] for row in read_json(path).get("accounts", []) if path.stem != slug}
    for human in bundle.get("humans", []):
        require(human not in accounts, f"Human account {human} would clash with a fleet agent's account.")
    for path in sorted((STATE / "fleets").glob("*.json")):
        other = read_json(path)
        saved = {row["name"]: row["password"] for row in other.get("accounts", [])}
        for row in bundle["accounts"]:
            if row["name"] not in saved:
                continue
            require(path.stem == slug, f"IRC account {row['name']} already belongs to fleet {path.stem}.")
            require(saved[row["name"]] == row["password"],
                    f"Fleet {slug} is already on this server with other passwords, so a VM with the same fleet "
                    "ID was set up again. Give the new fleet a different ID, or remove the old fleet "
                    f"('subspace-setup irc-server remove {slug}') and drop its accounts in Ergo first.")


def remove(slug):
    path = STATE / "fleets" / f"{slug}.json"
    require(path.is_file(), f"No fleet {slug} on this server.")
    path.unlink()
    answers = read_json(STATE / "answers.json")
    render(answers, database(answers))
    restart()
    say(f"fleet {slug} removed from the seed list. Its accounts remain in Ergo until you drop them with NickServ.")


def certificate():
    answers = read_json(STATE / "answers.json", {})
    if "tlsCert" in answers:
        sys.stdout.write(Path(answers["tlsCert"]).read_text())
    else:
        sys.stdout.write(compose("exec", "-T", "irc", "openssl", "x509", "-in", "/ircd/local-tls.pem", capture=True) + "\n")


def main(argv):
    command = argv[0] if argv and not argv[0].startswith("-") else "setup"
    rest = argv[1:] if argv and not argv[0].startswith("-") else argv
    if command == "setup":
        parser = argparse.ArgumentParser(prog="subspace-setup irc-server")
        parser.add_argument("--server-name", help="DNS name fleets and desktops use, for example irc.example.ts.net")
        parser.add_argument("--bind", help="private address for the TLS listener (default: the Tailscale or private address)")
        parser.add_argument("--port", type=int, help="TLS port (default 6697)")
        parser.add_argument("--history", choices=["postgresql", "sqlite"],
                            help="where Ergo keeps history: its own PostgreSQL container (default) or SQLite in its volume")
        parser.add_argument("--admin", help="comma-separated human IRC accounts (default admiral)")
        parser.add_argument("--tls-cert", help="absolute path to a certificate chain for the server name")
        parser.add_argument("--tls-key", help="absolute path to its private key")
        parser.add_argument("--operator", help="Linux login that may SSH in (default: the sudo user)")
        parser.add_argument("--access", choices=["ssh", "tailscale", "cloudflare"])
        parser.add_argument("--ssh-from", help="comma-separated addresses or CIDRs allowed to SSH in")
        parser.add_argument("--no-lockdown", dest="lockdown", action="store_false", default=None)
        parser.add_argument("--non-interactive", action="store_true")
        setup(parser.parse_args(rest))
    elif command == "add":
        require(len(rest) == 1, "Usage: subspace-setup irc-server add FILE|-")
        add(rest[0])
    elif command == "remove":
        require(len(rest) == 1, "Usage: subspace-setup irc-server remove SLUG")
        remove(rest[0])
    elif command == "ca":
        certificate()
    elif command == "secret":
        require(len(rest) == 1, "Usage: subspace-setup irc-server secret ACCOUNT")
        answers = read_json(STATE / "answers.json", {})
        require(rest[0] in answers.get("humans", []), f"{rest[0]} is not a human account on this server.")
        sys.stdout.write(generated()[f"irc-{rest[0]}"] + "\n")
    elif command == "status":
        answers = read_json(STATE / "answers.json", {})
        print(f"server: {answers.get('serverName')}:{answers.get('port')} on {answers.get('bind')}")
        print(f"humans: {' '.join(answers.get('humans', []))}")
        print(f"fleets: {' '.join(path.stem for path in sorted((STATE / 'fleets').glob('*.json'))) or 'none'}")
        compose("ps", check=False)
    else:
        raise SetupError(f"Unknown irc-server command {command}.")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except SetupError as error:
        sys.exit(f"subspace-setup: {error}")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"subspace-setup: {error}")
