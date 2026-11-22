#!/usr/bin/python3
"""Set up a Subspace fleet on this VM from a handful of answers.

Runs as root on the fleet VM, normally through `subspace-setup`, which the
host installer (`install.sh` or `install.ps1` at the repository root) calls
over SSH. It:

1. saves the answers and writes the fleet installer's inputs under
   /root/fleet (fleet.json, fleet.secrets.json and the API environment);
2. generates every password and key on this VM, optionally keeping them in
   Azure Key Vault through the VM's managed identity;
3. runs the fleet installer (Stargate, CLIProxyAPI, OpenCode, AgentsView, the
   agents' Linux users and local Ergo IRC);
4. starts the Subspace API and PostgreSQL on loopback, signs the admin in
   first so they become the organisation administrator, seeds demo data on
   request, creates an initiative for the fleet and registers the fleet;
5. enables Stargate's Fleet API when the pinned Stargate has it;
6. locks the VM down (see lockdown.sh) and writes a non-secret summary that
   the host reads.

Re-running reuses the saved answers and secrets, so it is safe to run again
after a failure or to change a setting.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
from urllib.error import HTTPError, URLError
from urllib.parse import quote, urlsplit
from urllib.request import Request, urlopen

HERE = Path(__file__).resolve().parent
PAYLOAD = HERE.parents[1]
INSTALLER = PAYLOAD / "subspace-installer"
INPUTS = Path("/root/fleet")
API_DIR = Path("/opt/subspace/api")
FLEET_APP = Path("/opt/subspace-fleet")
FLEET_ETC = Path("/etc/subspace-fleet")
API_PROJECT = "subspace-api"
API_PORT = 3000
STARGATE_API_PORT = 8484
STARGATE_REPO = "https://github.com/aiconsultancy/stargate.git"
LOOPBACK = "127.0.0.1"
IRC_PLAIN = 6667
IRC_TLS = 6697
DASHBOARD = 8070
AGENTSVIEW = 8080
OPERATOR_ACCOUNT = "ircop"
RESERVED = {"nickserv", "chanserv", "hostserv", "histserv", "operserv", "memoserv", "global", "ergo",
            "root", "admin", "operator", "ircop", "server", "services"}
NAME_ID = re.compile(r"[a-z][a-z0-9-]{0,15}")
SLUG = re.compile(r"[a-z0-9][a-z0-9-]{0,19}")
ACCOUNT = re.compile(r"[a-z][a-z0-9-]{0,23}")
EMAIL = re.compile(r"[^\s@]+@[^\s@]+\.[^\s@]+")
ORG = re.compile(r"[a-z0-9][a-z0-9-]{0,62}")
SHA = re.compile(r"[0-9a-f]{40}")
JWT = re.compile(r"[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+")
VAULT = re.compile(r"[a-zA-Z][a-zA-Z0-9-]{1,22}[a-zA-Z0-9]")
GATEWAY_USER = "ss-fleet-gateway"  # The fleet installer's shared Stargate gateway user (runtime.GATEWAY).
LINUX_USER = re.compile(r"[a-z_][a-z0-9_.-]{0,31}")
# Development sign-in is only the default: an operator who switches the API to WorkOS or Entra keeps it.
DEV_AUTH = {"AUTH_PROVIDERS": "dev", "ENABLE_DEV_AUTH": "true", "NODE_ENV": "development"}
CIDR = re.compile(r"[0-9a-fA-F:.]+(?:/[0-9]{1,3})?")
# The fleet installer's repository IDs (also Stargate crew project keys) and its crew limit of nine repositories.
REPO_ID = re.compile(r"[a-z][a-z0-9_-]{0,39}")
MAX_REPOSITORIES = 9


class SetupError(Exception):
    pass


class ApiError(SetupError):
    def __init__(self, message, status):
        super().__init__(message)
        self.status = status


def say(message):
    print(f"subspace-setup: {message}", flush=True)


def require(condition, message):
    if not condition:
        raise SetupError(message)


def ident(name, label):
    value = re.sub(r"[^a-z0-9]+", "-", name.strip().lower()).strip("-")
    require(NAME_ID.fullmatch(value) and not value.startswith("fleet-"),
            f"{label} '{name}' needs a short name that starts with a letter (at most 16 letters, digits or hyphens).")
    return value


def read_json(path, default=None):
    try:
        return json.loads(Path(path).read_text())
    except FileNotFoundError:
        return default


def write_private(path, text):
    path = Path(path)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", dir=path.parent, delete=False) as handle:
        os.fchmod(handle.fileno(), 0o600)
        handle.write(text)
        temporary = handle.name
    os.replace(temporary, path)


def write_json(path, data):
    write_private(path, json.dumps(data, indent=2, sort_keys=False) + "\n")


def run(*args, check=True, capture=False, env=None, input_text=None):
    result = subprocess.run([str(arg) for arg in args], check=False, text=True, env=env, input=input_text,
                            stdout=subprocess.PIPE if capture else None)
    if check and result.returncode:
        raise SetupError(f"{Path(str(args[0])).name} {args[1] if len(args) > 1 else ''} failed with exit code {result.returncode}.")
    return result.stdout.strip() if capture else result.returncode


# Answers ---------------------------------------------------------------------

def parse_args(argv):
    parser = argparse.ArgumentParser(prog="subspace-setup", description="Set up a Subspace fleet on this VM.")
    parser.add_argument("--name", help="fleet display name, for example 'Example Fleet'")
    parser.add_argument("--slug", help="stable fleet ID; defaults to the name in lower case")
    parser.add_argument("--rear-admiral", help="the rear admiral's name, for example Atlas")
    parser.add_argument("--captains", help="comma-separated captain (ship) names, for example 'Orion,Vega'")
    parser.add_argument("--admin", help="your IRC account and nickname (default admiral)")
    parser.add_argument("--admin-email", help="development sign-in email (default developer@example.com)")
    parser.add_argument("--organisation", help="development sign-in organisation (default local)")
    parser.add_argument("--repositories", help="comma-separated HTTPS Git URLs that every captain checks out, "
                        "optionally as ID=URL; crew get one worktree of each. Replaces the fleet's list; "
                        "'' clears it. Private GitHub repositories need GitHub Apps first (installer README)")
    parser.add_argument("--crew-claude", type=int, help="Claude-family crew per captain (default 2; crew need a repository)")
    parser.add_argument("--crew-codex", type=int, help="Codex-family crew per captain (default 2; crew need a repository)")
    parser.add_argument("--demo", dest="demo", action="store_true", default=None, help="seed fictional demo data")
    parser.add_argument("--no-demo", dest="demo", action="store_false")
    parser.add_argument("--irc", choices=["local", "remote"],
                        help="local runs Ergo on this VM; remote uses a shared IRC server for several fleets")
    parser.add_argument("--irc-host", help="shared IRC server's DNS name (remote mode; must match its TLS certificate)")
    parser.add_argument("--irc-port", type=int, help="shared IRC server's TLS port (default 6697)")
    parser.add_argument("--irc-ca", help="absolute path on this VM to the shared server's CA certificate (PEM)")
    parser.add_argument("--no-api", dest="api", action="store_false", default=None,
                        help="skip the Subspace API and PostgreSQL on this VM")
    parser.add_argument("--stargate-api", choices=["auto", "on", "off"],
                        help="enable Stargate's Fleet API (default auto: when the pinned Stargate has it)")
    parser.add_argument("--stargate-ref", help="'latest' (the default: Stargate's newest release, looked up on "
                        "every run) or a full commit SHA to pin")
    parser.add_argument("--operator", help="Linux login that may SSH in and tunnel (default: the sudo user)")
    parser.add_argument("--access", choices=["ssh", "tailscale", "cloudflare"],
                        help="how you reach the VM; tailscale limits SSH to the tailnet, cloudflare to cloudflared")
    parser.add_argument("--ssh-from", help="comma-separated source addresses or CIDRs allowed to SSH in")
    parser.add_argument("--allow-port", help="comma-separated TCP ports to accept from private networks only, "
                        "for example 443 for an nginx listener on the tailnet")
    parser.add_argument("--key-vault", help="Azure Key Vault name for the generated secrets (managed identity)")
    parser.add_argument("--metadata-user", help="comma-separated Linux users, besides root and the operator, that "
                        "may reach the cloud metadata service (the agents never can)")
    parser.add_argument("--no-lockdown", dest="lockdown", action="store_false", default=None,
                        help="skip the inbound firewall and SSH hardening (not recommended; agent isolation stays)")
    parser.add_argument("--inputs", default=str(INPUTS), help=argparse.SUPPRESS)
    parser.add_argument("--dry-run", action="store_true", help="write and validate the inputs only")
    parser.add_argument("--non-interactive", action="store_true", help="never prompt; fail on missing answers")
    return parser.parse_args(argv)


def prompt(label, default=None, interactive=True):
    if not interactive:
        return default
    suffix = f" [{default}]" if default not in (None, "") else ""
    try:
        value = input(f"{label}{suffix}: ").strip()
    except EOFError:
        value = ""
    return value or default


def yes_no(label, default, interactive):
    if not interactive:
        return default
    answer = prompt(f"{label} (y/n)", "y" if default else "n")
    return str(answer).lower().startswith("y")


def gather(args, saved):
    interactive = sys.stdin.isatty() and not args.non_interactive
    answers = dict(saved)

    def pick(key, flag, label, default=None):
        if flag is not None:
            answers[key] = flag
        elif key not in answers:
            answers[key] = prompt(label, default, interactive) if interactive else default
        return answers[key]

    pick("name", args.name, "Fleet name (shown in the desktop and on IRC)")
    require(answers.get("name"), "Choose a fleet name with --name.")
    default_slug = re.sub(r"[^a-z0-9]+", "-", answers["name"].lower()).strip("-")[:20].strip("-")
    pick("slug", args.slug, "Fleet ID (lower case, never changes)", default_slug)
    pick("rearAdmiral", args.rear_admiral, "Rear admiral's name")
    require(answers.get("rearAdmiral"), "Choose the rear admiral's name with --rear-admiral.")
    captains = args.captains if args.captains is not None else answers.get("captains")
    if captains is None:
        captains = prompt("Captain (ship) names, comma-separated", None, interactive)
    if isinstance(captains, str):
        captains = [name.strip() for name in captains.split(",") if name.strip()]
    require(captains, "Name at least one captain with --captains.")
    answers["captains"] = captains
    pick("admin", args.admin, "Your IRC account name", "admiral")
    pick("adminEmail", args.admin_email, "Development sign-in email", "developer@example.com")
    pick("organisation", args.organisation, "Development sign-in organisation", "local")
    # Repositories live in fleet.json, beside hand-added private ones, so only this run's answer applies.
    if args.repositories is not None:
        answers["repositories"] = parse_repositories(args.repositories)
    elif interactive and not (INPUTS / "fleet.json").exists():
        answers["repositories"] = parse_repositories(
            prompt("Repositories every captain checks out (comma-separated HTTPS Git URLs, blank for none)", "", interactive) or "")
    crew = answers.setdefault("crew", {})
    for family, flag in (("claude", args.crew_claude), ("codex", args.crew_codex)):
        if flag is not None:
            crew[family] = flag
        elif family not in crew:
            crew[family] = int(prompt(f"{family.title()}-family crew per captain", "2", interactive) or 2)
    answers["irc"] = args.irc or answers.get("irc", "local")
    if answers["irc"] == "remote":
        pick("ircHost", args.irc_host, "Shared IRC server's DNS name")
        if args.irc_port is not None:
            answers["ircPort"] = args.irc_port
        answers.setdefault("ircPort", IRC_TLS)
        if args.irc_ca is not None:
            answers["ircCa"] = args.irc_ca
    if args.api is not None:
        answers["api"] = args.api
    answers.setdefault("api", True)
    if args.demo is not None:
        answers["demo"] = args.demo
    elif "demo" not in answers:
        answers["demo"] = yes_no("Seed fictional demo data so you can look around straight away?", True, interactive)
    answers["stargateApi"] = args.stargate_api or answers.get("stargateApi", "auto")
    if args.stargate_ref == "latest":
        answers.pop("stargateRef", None)
    elif args.stargate_ref is not None:
        answers["stargateRef"] = args.stargate_ref
    answers["operator"] = args.operator or answers.get("operator") or os.environ.get("SUDO_USER") or ""
    answers["access"] = args.access or answers.get("access", "ssh")
    if args.ssh_from is not None:
        answers["sshFrom"] = [value.strip() for value in args.ssh_from.split(",") if value.strip()]
    answers.setdefault("sshFrom", [])
    if args.allow_port is not None:
        answers["allowPorts"] = [value.strip() for value in args.allow_port.split(",") if value.strip()]
    answers.setdefault("allowPorts", [])
    if args.key_vault is not None:
        answers["keyVault"] = args.key_vault
    if args.metadata_user is not None:
        answers["metadataUsers"] = [value.strip() for value in args.metadata_user.split(",") if value.strip()]
    answers.setdefault("metadataUsers", [])
    if args.lockdown is not None:
        answers["lockdown"] = args.lockdown
    answers.setdefault("lockdown", True)
    return answers


def parse_repositories(value):
    """'URL' or 'ID=URL' items. The ID defaults to the repository's name."""
    rows = []
    for item in (part.strip() for part in value.split(",")):
        if not item:
            continue
        named = re.fullmatch(r"([a-z][a-z0-9_-]{0,39})=(.+)", item)
        url = named.group(2) if named else item
        name = named.group(1) if named else re.sub(
            r"[^a-z0-9_-]+", "-", urlsplit(url).path.rstrip("/").rpartition("/")[2].removesuffix(".git").lower()).strip("-")
        rows.append([name, url])
    return rows


def check_repositories(rows):
    for name, url in rows:
        parts = urlsplit(url)
        require(parts.scheme == "https" and parts.hostname and parts.path.strip("/") and not parts.username
                and not parts.password and not parts.query and not parts.fragment,
                f"Repository {url} needs an HTTPS Git URL without credentials, a query or a fragment.")
        require(REPO_ID.fullmatch(name) and name != "default",
                f"Give repository {url} an ID that starts with a letter, as ID=URL (for example web={url}).")
    require(len({name for name, _ in rows}) == len(rows) and len({url for _, url in rows}) == len(rows),
            "Repository IDs and URLs must be unique.")
    require(len(rows) <= MAX_REPOSITORIES, f"Use at most {MAX_REPOSITORIES} repositories, the crew limit.")


def check_answers(answers):
    require(1 <= len(answers["name"]) <= 100, "The fleet name must be 1 to 100 characters.")
    require(SLUG.fullmatch(answers["slug"]), "The fleet ID must be at most 20 lower-case letters, digits or hyphens.")
    rear = ident(answers["rearAdmiral"], "Rear admiral")
    ids = [rear] + [ident(name, "Captain") for name in answers["captains"]]
    require(len(set(ids)) == len(ids), "The rear admiral and captains need distinct names.")
    require(len(answers["captains"]) <= 12, "Use at most 12 captains.")
    admin = answers["admin"]
    require(ACCOUNT.fullmatch(admin) and admin not in RESERVED and not admin.startswith("guest-"),
            "Choose an IRC account of lower-case letters, digits or hyphens that starts with a letter, such as 'admiral'.")
    agent_accounts = [f"{answers['slug']}-{agent}" for agent in ids]
    require(admin not in ids and admin not in agent_accounts
            and not any(admin.startswith(f"{account}-") for account in agent_accounts[1:]),
            "Your IRC account must differ from the agents' names and accounts.")
    for agent in ids:
        require(len(f"{answers['slug']}-{agent}") <= 32,
                f"'{answers['slug']}-{agent}' is too long for its IRC account. Use a shorter fleet ID or name.")
    require(EMAIL.fullmatch(answers["adminEmail"]), "Use a valid sign-in email address.")
    if answers["irc"] == "remote":
        host = answers.get("ircHost") or ""
        require(re.fullmatch(r"[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?", host.lower()) and "." in host,
                "Remote IRC needs the shared server's DNS name with --irc-host, matching its TLS certificate.")
        require(isinstance(answers["ircPort"], int) and 1 <= answers["ircPort"] <= 65535, "Invalid --irc-port.")
        if answers.get("ircCa"):
            require(Path(answers["ircCa"]).is_absolute() and Path(answers["ircCa"]).is_file(),
                    "--irc-ca must be an absolute path to a PEM file on this VM.")
    require(ORG.fullmatch(answers["organisation"]), "The organisation must be lower-case letters, digits or hyphens.")
    for family, count in answers["crew"].items():
        require(isinstance(count, int) and 0 <= count <= 10, f"{family} crew must be between 0 and 10 per captain.")
    check_repositories(answers.get("repositories", []))
    if answers.get("stargateRef"):
        require(SHA.fullmatch(answers["stargateRef"]), "--stargate-ref must be a full 40-character commit SHA.")
    require(answers["operator"] and answers["operator"] != "root",
            "Run setup with sudo from your own login, or pass --operator with the Linux user that SSHes in.")
    try:
        pwd.getpwnam(answers["operator"])
    except KeyError:
        raise SetupError(f"The operator login {answers['operator']} does not exist on this VM.") from None
    for source in answers["sshFrom"]:
        require(CIDR.fullmatch(source), f"Invalid --ssh-from address {source}.")
    for value in answers["allowPorts"]:
        require(str(value).isdigit() and 1 <= int(value) <= 65535 and int(value) != 22,
                f"Invalid --allow-port {value}.")
    if answers.get("keyVault"):
        require(VAULT.fullmatch(answers["keyVault"]), "Invalid Key Vault name.")
    for user in answers.get("metadataUsers", []):
        require(LINUX_USER.fullmatch(user) and user != "root", f"Invalid --metadata-user {user}.")
        require(not user.startswith("ss-") or user == GATEWAY_USER,
                f"--metadata-user {user} is a fleet agent's account; agents never reach cloud metadata.")
    for name in [answers["name"], *answers["captains"], answers["rearAdmiral"]]:
        require("\n" not in name and len(name) <= 100, "Names must be single lines of at most 100 characters.")
    return rear, ids[1:]


# Secrets ---------------------------------------------------------------------

class KeyVault:
    """Minimal Key Vault client using the VM's managed identity (IMDS)."""

    def __init__(self, name):
        self.base = f"https://{name}.vault.azure.net/secrets/"
        request = Request(
            "http://169.254.169.254/metadata/identity/oauth2/token?api-version=2018-02-01&resource="
            + quote("https://vault.azure.net", safe=""),
            headers={"Metadata": "true"},
        )
        try:
            with urlopen(request, timeout=10) as response:
                self.token = json.load(response)["access_token"]
        except (URLError, OSError, KeyError, ValueError):
            raise SetupError("Could not get a managed identity token. Give the VM a managed identity "
                             "with get and set permissions on the Key Vault's secrets.") from None

    def _call(self, method, name, body=None):
        data = None if body is None else json.dumps(body).encode()
        request = Request(f"{self.base}{name}?api-version=7.4", data=data, method=method, headers={
            "Authorization": f"Bearer {self.token}", "Content-Type": "application/json"})
        try:
            with urlopen(request, timeout=20) as response:
                return json.load(response)
        except HTTPError as error:
            error.close()
            if error.code == 404 and method == "GET":
                return None
            raise SetupError(f"Key Vault returned HTTP {error.code} for secret {name}.") from None
        except (URLError, OSError, ValueError):
            raise SetupError(f"Could not reach Key Vault for secret {name}.") from None

    def get(self, name):
        value = self._call("GET", name)
        return value["value"] if value else None

    def set(self, name, value):
        self._call("PUT", name, {"value": value, "contentType": "subspace-fleet"})


class Secrets:
    """Every generated credential, kept in /root/fleet and optionally Key Vault."""

    def __init__(self, inputs, slug, vault):
        self.path = inputs / "generated-secrets.json"
        self.values = read_json(self.path, {})
        self.slug = slug
        self.vault = KeyVault(vault) if vault else None

    def get(self, key, make):
        if key in self.values:
            return self.values[key]
        name = f"subspace-{self.slug}-{key}"
        value = self.vault.get(name) if self.vault else None
        if value is None:
            value = make()
            if self.vault:
                self.vault.set(name, value)
        self.values[key] = value
        write_json(self.path, self.values)
        return value

    def password(self, key):
        return self.get(key, lambda: secrets.token_hex(24))


# Fleet config ----------------------------------------------------------------

def roster(answers, rear, captains):
    """The rear admiral and captains: the only fleet members with Linux users and IRC accounts.

    Crew run as their captain through Stargate's crew plugin and have no IRC account.
    """
    # Accounts carry the fleet ID so that several fleets can share one IRC server.
    slug = answers["slug"]
    members = [{"id": rear, "name": answers["rearAdmiral"], "account": f"{slug}-{rear}"}]
    for captain_id, name in zip(captains, answers["captains"], strict=True):
        members.append({"id": captain_id, "name": name, "account": f"{slug}-{captain_id}"})
    return members


def save_answers(answers):
    # Repositories are kept in fleet.json, where an operator may also add private ones by hand.
    write_json(INPUTS / "answers.json", {key: value for key, value in answers.items() if key != "repositories"})


def demo_channels():
    channels = read_json(PAYLOAD / "scripts/local-stack/irc-channels.json", [])
    return [row["name"] for row in channels if isinstance(row, dict) and "name" in row]


def latest_stargate():
    """Stargate's newest release (highest vX.Y.Z tag) as (tag, commit), so every run installs the latest."""
    try:
        output = run("git", "ls-remote", "--tags", STARGATE_REPO, "refs/tags/v*", capture=True)
    except SetupError:
        output = ""
    releases = {}
    for line in output.splitlines():
        sha, _, ref = line.partition("\t")
        match = re.fullmatch(r"refs/tags/(v(\d+)\.(\d+)\.(\d+))(\^\{\})?", ref)
        if match and SHA.fullmatch(sha):
            tag, peeled = match.group(1), match.group(5)
            version = tuple(int(part) for part in match.group(2, 3, 4))
            # An annotated tag's peeled line (^{}) names the commit; prefer it.
            if peeled or tag not in releases:
                releases[tag] = (version, sha)
    require(releases, "Could not look up Stargate's latest release on GitHub. "
            "Check the VM's internet access, or pin one with --stargate-ref <full SHA>.")
    tag = max(releases, key=lambda name: releases[name][0])
    return tag, releases[tag][1]


def fleet_config(answers, members, irc_ref, api_block=None, stargate_api=False, stargate_ref=None, history=None):
    slug = answers["slug"]
    starbase = f"#{slug}-starbase"
    extra_channels = demo_channels() if answers["demo"] else []
    config = {
        "slug": slug,
        "name": answers["name"],
        "rearAdmiral": members[0],
        "captains": members[1:],
        "crew": {family: {"count": count} for family, count in answers["crew"].items()},
        "operatorUsers": [answers["operator"]],
        "toolchains": {"node": "24"},
        "irc": {
            "mode": "local",
            "commandChannel": starbase,
            "starbaseChannel": starbase,
            "allowedAccounts": [answers["admin"]],
            "advertise": {"host": LOOPBACK, "port": IRC_PLAIN, "tls": False},
            "compose": {"ref": irc_ref, "operator": OPERATOR_ACCOUNT, "channels": extra_channels},
        },
    }
    if history:
        config["irc"]["compose"]["history"] = history
    if answers["irc"] == "remote":
        config["irc"] = {
            "mode": "remote", "host": answers["ircHost"], "port": answers["ircPort"], "tls": True,
            "commandChannel": starbase, "starbaseChannel": starbase, "allowedAccounts": [answers["admin"]],
        }
        if answers.get("ircCa"):
            config["irc"]["caFile"] = answers["ircCa"]
    previous = read_json(INPUTS / "fleet.json", {})
    # Keep anything the operator added by hand, such as private repositories, GitHub Apps or stakeholders.
    for key in ("harness", "stakeholders", "usage", "medical", "toolchains", "maintenance",
                "githubPolicy", "dashboard", "proxy", "agentsview"):
        if key in previous:
            config[key] = previous[key]
    repositories = previous.get("repositories", [])
    if "repositories" in answers:
        # This run's answer replaces the list; a repository that stays keeps its ID and visibility.
        known = {row.get("url"): row for row in repositories if isinstance(row, dict)}
        repositories = [known.get(url, {"id": name, "url": url}) for name, url in answers["repositories"]]
    config["repositories"] = repositories
    by_id = {person["id"]: person for person in [previous.get("rearAdmiral", {}), *previous.get("captains", [])]
             if isinstance(person, dict) and "id" in person}
    for person in [config["rearAdmiral"], *config["captains"]]:
        for key, value in by_id.get(person["id"], {}).items():
            person.setdefault(key, value)
    operators = previous.get("operatorUsers", [])
    config["operatorUsers"] = list(dict.fromkeys([answers["operator"], *operators]))
    stargate = {}
    ref = stargate_ref or answers.get("stargateRef")
    if ref:
        stargate["ref"] = ref
    if stargate_api:
        stargate["api"] = {"port": STARGATE_API_PORT}
    if stargate:
        config["stargate"] = stargate
    if api_block:
        config["api"] = api_block
    return config


def fleet_secrets(secret_store, members, admin, token="", local_irc=True):
    # A shared IRC server owns the human accounts; a local one is seeded with the admin's.
    humans = [admin] if local_irc else []
    passwords = {account: secret_store.password(f"irc-{account}")
                 for account in [person["account"] for person in members] + humans}
    result = {
        "proxyKey": secret_store.get("proxy-key", lambda: secrets.token_urlsafe(32)),
        "ircPasswords": passwords,
    }
    if local_irc:
        result["ircOperatorPassword"] = secret_store.password("irc-operator")
    if token:
        result["apiToken"] = token
    return result


def irc_source():
    """Give the installer a local checkout of subspace-irc from this payload.

    The installer normally fetches the Subspace repository at a pinned commit.
    A deterministic local commit of the shipped subspace-irc folder works
    offline and for private clones, and changes only when that folder does.
    """
    source = PAYLOAD / "subspace-irc"
    require((source / "compose.yaml").is_file(), "The payload has no subspace-irc/compose.yaml.")
    work = Path(tempfile.mkdtemp(prefix="subspace-irc-"))
    try:
        shutil.copytree(source, work / "subspace-irc", ignore=shutil.ignore_patterns(".env", "__pycache__"))
        env = {**os.environ, "GIT_AUTHOR_NAME": "Subspace setup", "GIT_AUTHOR_EMAIL": "setup@example.com",
               "GIT_COMMITTER_NAME": "Subspace setup", "GIT_COMMITTER_EMAIL": "setup@example.com",
               "GIT_AUTHOR_DATE": "2000-01-01T00:00:00Z", "GIT_COMMITTER_DATE": "2000-01-01T00:00:00Z"}
        run("git", "-C", work, "init", "-q")
        run("git", "-C", work, "add", "-A")
        run("git", "-C", work, "-c", "commit.gpgsign=false", "commit", "-q", "-m", "subspace-irc from setup payload", env=env)
        ref = run("git", "-C", work, "rev-parse", "HEAD", capture=True)
        destination = FLEET_APP / "src" / f"irc-{ref}"
        if not destination.is_dir():
            (FLEET_APP / "src").mkdir(mode=0o755, parents=True, exist_ok=True)
            run("chmod", "-R", "a+rX", work)
            shutil.move(str(work), destination)
            work = None
        return ref
    finally:
        if work:
            shutil.rmtree(work, ignore_errors=True)


def installer(config_path, dry_run=False):
    args = [INSTALLER / "install.sh", "--config", config_path]
    if dry_run:
        args.append("--dry-run")
    say("running the fleet installer" + (" (dry run)" if dry_run else ""))
    run(*args)


def installed_stargate_source():
    settings = read_json(FLEET_ETC / "gateway.json", {}) if os.access(FLEET_ETC / "gateway.json", os.R_OK) else {}
    entry = settings.get("entrypoint", "")
    return Path(entry).parents[1] if entry else None


# Subspace API ----------------------------------------------------------------

def compose(*args, capture=False, check=True, env=None, input_text=None):
    return run("docker", "compose", "--project-name", API_PROJECT, "--project-directory", API_DIR,
               "--env-file", INPUTS / "api.env", "-f", API_DIR / "compose.yaml", "-f", INPUTS / "api.override.yaml",
               *args, capture=capture, check=check, env=env, input_text=input_text)


def api_up(answers, secret_store):
    source = PAYLOAD / "subspace-api"
    require((source / "compose.yaml").is_file(), "The payload has no subspace-api/compose.yaml.")
    API_DIR.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    staging = API_DIR.with_name("api.new")
    shutil.rmtree(staging, ignore_errors=True)
    shutil.copytree(source, staging, ignore=shutil.ignore_patterns("node_modules", ".data", ".env", "__pycache__"))
    shutil.rmtree(API_DIR, ignore_errors=True)
    staging.rename(API_DIR)
    slug = answers["slug"]
    environment = {
        "POSTGRES_PASSWORD": secret_store.password("postgres"),
        "API_PORT": API_PORT,
        "DEV_CHANNELS": f"#{slug}-starbase",
        "DEV_IRC_HOST": irc_endpoint(answers)[0],
        "DEV_IRC_PORT": irc_endpoint(answers)[1],
    }
    previous = INPUTS / "api.env"
    kept = {}
    if previous.exists():
        # Keep what the operator set, such as WorkOS or Entra and turning development sign-in off.
        for line in previous.read_text().splitlines():
            key, _, value = line.partition("=")
            if key and not key.startswith("#") and key not in environment:
                kept[key] = value
    merged = {**environment, **DEV_AUTH, **kept}
    write_private(INPUTS / "api.env", "".join(f"{key}={value}\n" for key, value in merged.items()))
    # PostgreSQL is not published at all; the API is published on loopback only.
    write_private(INPUTS / "api.override.yaml", "\n".join([
        "services:",
        "  postgres:",
        "    ports: !reset []",
        "    restart: unless-stopped",
        "  api:",
        "    restart: unless-stopped",
        "    ports: !override",
        f'      - "{LOOPBACK}:{API_PORT}:3000"',
        "",
    ]))
    say("building and starting the Subspace API and PostgreSQL (loopback only)")
    compose("up", "--detach", "--wait", "--wait-timeout", "300", "--build")
    return dev_auth_enabled()


def dev_auth_enabled():
    """Whether the API still offers development sign-in, which setup uses to act as the admin."""
    environment = {}
    for line in (INPUTS / "api.env").read_text().splitlines():
        key, _, value = line.partition("=")
        environment[key] = value
    providers = [value.strip() for value in environment.get("AUTH_PROVIDERS", "").split(",")]
    return environment.get("ENABLE_DEV_AUTH") == "true" and "dev" in providers


def api_down():
    """Stop the API and PostgreSQL after a switch to --no-api, keeping their data."""
    if not (INPUTS / "api.env").is_file() or not shutil.which("docker"):
        return
    running = run("docker", "ps", "--quiet", "--filter", f"label=com.docker.compose.project={API_PROJECT}",
                  capture=True, check=False)
    if running:
        say("stopping the Subspace API and PostgreSQL on this VM (--no-api); their data is kept")
        compose("down")


def irc_endpoint(answers):
    """The IRC host and port the desktop uses, which the API records for each fleet."""
    if answers["irc"] == "local":
        return LOOPBACK, IRC_PLAIN
    return answers["ircHost"], answers["ircPort"]


def history_database(secret_store):
    """Give Ergo its own role and database in the API's PostgreSQL; return the IRC history settings."""
    password = secret_store.password("ergo-history")
    require(re.fullmatch(r"[0-9a-f]{48}", password), "Unexpected IRC history password format.")
    sql = "\n".join([
        "DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'ergo') THEN CREATE ROLE ergo LOGIN; END IF; END $$;",
        f"ALTER ROLE ergo WITH LOGIN PASSWORD '{password}';",
        "SELECT 'CREATE DATABASE ergo_history OWNER ergo' WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'ergo_history')\\gexec",
        "",
    ])
    compose("exec", "-T", "postgres", "psql", "-q", "-v", "ON_ERROR_STOP=1", "-U", "subspace", "-d", "subspace",
            input_text=sql)
    say("PostgreSQL holds the API's database and Ergo's history database (ergo_history)")
    return {"database": "postgresql", "host": "postgres", "port": 5432, "user": "ergo", "name": "ergo_history",
            "network": f"{API_PROJECT}_default"}


def api_token(answers):
    token = compose("exec", "-T", "api", "bun", "run", "--silent", "auth:dev",
                    answers["adminEmail"], answers["organisation"], capture=True).splitlines()[-1].strip()
    require(JWT.fullmatch(token), "The API did not return a development token.")
    return token


def api_call(token, method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    request = Request(f"http://{LOOPBACK}:{API_PORT}/api/v1{path}", data=data, method=method, headers={
        "Authorization": f"Bearer {token}", "Accept": "application/json", "Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=30) as response:
            text = response.read().decode()
    except HTTPError as error:
        error.close()
        raise ApiError(f"The Subspace API returned HTTP {error.code} for {method} {path}.", error.code) from None
    except (URLError, OSError):
        raise SetupError(f"Could not reach the Subspace API for {method} {path}.") from None
    return json.loads(text) if text else {}


def seed_demo(answers):
    say("seeding fictional demo data")
    compose("exec", "-T", "-e", f"SEED_EMAIL={answers['adminEmail']}", "-e", f"SEED_ORGANISATION={answers['organisation']}",
            "-e", f"SEED_IRC_HOST={irc_endpoint(answers)[0]}", "-e", f"SEED_IRC_PLAIN_PORT={IRC_PLAIN}",
            "-e", f"SEED_IRC_TLS_PORT={IRC_TLS if answers['irc'] == 'local' else answers['ircPort']}",
            "api", "bun", "run", "--silent", "seed:demo")


def fleet_initiative(token, name, channels, known=None, managed=(), slug=None, exclude=()):
    """Create, or find, a fleet's initiative and keep its channels in step with the roster.

    The initiative is found by its saved ID, else by name among initiatives that belong to no
    fleet or to this one (slug). Channels someone added by hand stay; channels setup added
    before (managed) that the roster no longer has are removed.
    """
    mine = {row["id"]: row for row in api_call(token, "GET", "/me/initiatives").get("items", [])}
    found = known if known in mine else None
    cursor = ""
    while not found:
        page = api_call(token, "GET", "/initiatives?limit=100" + (f"&cursor={quote(cursor)}" if cursor else ""))
        found = next((row["id"] for row in page.get("items", []) if row.get("name") == name
                      and row["id"] not in exclude and (row.get("fleet") is None or (slug and (row.get("fleet") or {}).get("slug") == slug))), None)
        cursor = page.get("nextCursor") or ""
        if not cursor:
            break
    if not found:
        me = api_call(token, "GET", "/me")
        created = api_call(token, "POST", "/initiatives", {
            "name": name, "ownerId": me["userId"], "status": "active",
            "description": f"Work for the {name} fleet. Talk to the rear admiral in {channels[0]}.",
            "channelConfiguration": {"channels": " ".join(channels), "memberUserIds": [me["userId"]]},
        })
        return created["id"]
    if found not in mine:
        say(f"found an initiative named {name} that you are not a member of; check its channels in the desktop")
        return found
    retired = set(managed) - set(channels)
    for _attempt in range(3):
        # Read the version before the channels: an edit in between makes the update fail with 409.
        version = api_call(token, "GET", f"/initiatives/{found}")["version"]
        rows = {row["id"]: row for row in api_call(token, "GET", "/me/initiatives").get("items", [])}
        current = (rows.get(found, {}).get("channels") or "").split()
        desired = [channel for channel in current if channel not in retired]
        desired += [channel for channel in channels if channel not in desired]
        if desired == current:
            return found
        try:
            api_call(token, "PATCH", f"/initiatives/{found}/channel-configuration",
                     {"version": version, "channels": " ".join(desired)})
        except ApiError as error:
            if error.status == 409:
                continue
            raise
        say(f"the fleet's initiative now lists {' '.join(desired)}")
        return found
    raise SetupError("The fleet's initiative kept changing while setup updated its channels. Run setup again.")


def fleet_registration(answers, members):
    """The non-secret description of this fleet that another fleet's API needs to show it."""
    host, port = irc_endpoint(answers)
    return {
        "slug": answers["slug"], "name": answers["name"], "ircHost": host, "ircPort": port,
        "ircTls": answers["irc"] != "local", "rearAdmiralName": members[0]["name"],
        "captainNames": [person["name"] for person in members[1:]],
        "channels": fleet_channels(answers, members),
    }


def registration_ready():
    """Fail unless this VM's API can register further fleets."""
    answers = read_json(INPUTS / "answers.json", {})
    require(answers.get("api"), "The Subspace API is not set up on this VM.")
    require(dev_auth_enabled(), "This API no longer offers development sign-in, so the installer cannot register "
            "further fleets with it. Register them as an administrator instead (INSTALL.md, 'One fleet or several').")
    return answers


def register_fleet(registration):
    """Register another fleet, and an initiative for it, with the API on this VM."""
    answers = registration_ready()
    require(isinstance(registration, dict), "The fleet registration must be a JSON object.")
    slug, name = registration.get("slug"), registration.get("name")
    require(isinstance(slug, str) and SLUG.fullmatch(slug), "The registration has an invalid fleet ID.")
    require(slug != answers["slug"], "That is this VM's own fleet.")
    require(isinstance(name, str) and 1 <= len(name) <= 100 and "\n" not in name, "The registration has an invalid name.")
    channels = registration.get("channels")
    require(isinstance(channels, list) and channels
            and all(isinstance(c, str) and re.fullmatch(rf"#{re.escape(slug)}-[a-z0-9-]{{1,40}}", c) for c in channels),
            "The registration's channels must be the fleet's own (#<fleet>-...).")
    require(registration.get("ircHost") not in ("127.0.0.1", "localhost", "::1"),
            "A further fleet needs a shared IRC server (--irc-server): IRC on its own VM is unreachable from here.")
    port = registration.get("ircPort")
    require(isinstance(registration.get("ircHost"), str) and isinstance(port, int) and 1 <= port <= 65535,
            "The registration has an invalid IRC server.")
    fields = {key: registration[key] for key in ("slug", "name", "ircHost", "ircPort")}
    fields["ircTls"] = bool(registration.get("ircTls"))
    token = api_token(answers)
    try:
        fleet = api_call(token, "GET", f"/fleets/{slug}")
    except ApiError as error:
        if error.status != 404:
            raise
        fleet = api_call(token, "POST", "/fleets", fields)
    changes = {key: value for key, value in fields.items() if fleet.get(key) != value}
    if changes:
        fleet = api_call(token, "PATCH", f"/fleets/{slug}", changes)
    registered = answers.setdefault("registeredFleets", {}).setdefault(slug, {})
    initiative = fleet_initiative(token, name, channels, registered.get("initiativeId"),
                                  registered.get("channels", []), slug, exclude=[answers.get("initiativeId")])
    assigned = api_call(token, "GET", f"/initiatives/{initiative}").get("fleetId")
    require(assigned in (None, fleet["id"]), f"Initiative {initiative} already belongs to another fleet.")
    if assigned != fleet["id"]:
        api_call(token, "PUT", f"/initiatives/{initiative}/fleet", {"fleetId": fleet["id"]})
    registered.update(initiativeId=initiative, channels=channels)
    write_json(INPUTS / "answers.json", answers)
    say(f"registered fleet {name} ({slug}) with initiative {initiative}")


# Summary ---------------------------------------------------------------------

def fleet_channels(answers, members):
    slug = answers["slug"]
    return [f"#{slug}-starbase"] + [f"#{slug}-{person['id']}" for person in members[1:]]


def irc_bundle(answers, members, secret_store):
    """What a shared IRC server needs for this fleet: its SASL accounts and channels."""
    channels = fleet_channels(answers, members)
    topics = [f"{answers['name']} Starbase: admirals and the rear admiral"] + [
        f"{answers['name']}: {person['name']}'s ship" for person in members[1:]]
    bundle = {
        "fleet": answers["slug"],
        "accounts": [{"name": account, "password": secret_store.password(f"irc-{account}")}
                     for account in [person["account"] for person in members]],
        "channels": [{"name": name, "topic": topic} for name, topic in zip(channels, topics, strict=True)],
        "humans": [answers["admin"]],
    }
    write_json(INPUTS / "irc-bundle.json", bundle)
    return bundle


def summary(answers, members, stargate_api, lockdown_id=""):
    local = answers["irc"] == "local"
    ports = [DASHBOARD, AGENTSVIEW]
    if local:
        ports = [IRC_PLAIN, IRC_TLS, *ports]
    if answers["api"]:
        ports.insert(0, API_PORT)
    if stargate_api:
        ports.append(STARGATE_API_PORT)
    values = {
        "SUBSPACE_FLEET_SLUG": answers["slug"],
        "SUBSPACE_FLEET_NAME": answers["name"],
        "SUBSPACE_ADMIN_ACCOUNT": answers["admin"],
        "SUBSPACE_ADMIN_EMAIL": answers["adminEmail"],
        "SUBSPACE_ORGANISATION": answers["organisation"],
        "SUBSPACE_CHANNELS": " ".join(fleet_channels(answers, members)),
        "SUBSPACE_IRC_MODE": answers["irc"],
        "SUBSPACE_IRC_HOST": LOOPBACK if local else answers["ircHost"],
        "SUBSPACE_IRC_PORT": str(IRC_PLAIN if local else answers["ircPort"]),
        "SUBSPACE_IRC_TLS": "0" if local else "1",
        "SUBSPACE_DEMO": "1" if answers["demo"] else "0",
        "SUBSPACE_API": "1" if answers["api"] else "0",
        "SUBSPACE_STARGATE_API": "1" if stargate_api else "0",
        "SUBSPACE_STARGATE_API_PORT": str(STARGATE_API_PORT),
        "SUBSPACE_PORTS": " ".join(str(port) for port in ports),
        "SUBSPACE_OPERATOR": answers["operator"],
        "SUBSPACE_ACCESS": answers["access"],
        "SUBSPACE_LOCKDOWN": "1" if answers["lockdown"] else "0",
        "SUBSPACE_LOCKDOWN_ID": lockdown_id,
    }
    write_private(INPUTS / "summary.env", "".join(f"{key}={value}\n" for key, value in values.items()))
    return values


# Main ------------------------------------------------------------------------

def setup(args):
    global INPUTS
    INPUTS = Path(args.inputs)
    require(args.dry_run or os.geteuid() == 0, "Run subspace-setup as root (sudo).")
    INPUTS.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(INPUTS, 0o700)
    saved = read_json(INPUTS / "answers.json", {})
    answers = gather(args, saved)
    rear, captains = check_answers(answers)
    if saved.get("slug") and saved["slug"] != answers["slug"]:
        raise SetupError("The fleet ID cannot change on an installed VM. Reinstall on a fresh VM to rename it.")
    save_answers(answers)
    secret_store = Secrets(INPUTS, answers["slug"], answers.get("keyVault"))
    members = roster(answers, rear, captains)
    local_irc = answers["irc"] == "local"
    config_path = INPUTS / "fleet.json"
    secrets_path = INPUTS / "fleet.secrets.json"

    if answers.get("stargateRef"):
        stargate_tag, stargate_ref = "pinned", answers["stargateRef"]
    else:
        stargate_tag, stargate_ref = latest_stargate()
    say(f"installing Stargate {stargate_tag} ({stargate_ref[:12]})")

    def write_inputs(irc_ref, api_block=None, stargate_api=False, token="", history=None):
        write_json(config_path, fleet_config(answers, members, irc_ref, api_block, stargate_api, stargate_ref, history))
        private = fleet_secrets(secret_store, members, answers["admin"], token, local_irc)
        if history:
            private["ircHistoryPassword"] = secret_store.password("ergo-history")
        write_json(secrets_path, private)

    if not local_irc:
        irc_bundle(answers, members, secret_store)
    if args.dry_run:
        write_inputs(hashlib.sha1(b"dry-run").hexdigest())
        installer(config_path, dry_run=True)
        say(f"dry run complete; inputs are in {INPUTS}")
        return

    irc_ref = irc_source() if local_irc else None
    # Releases from v0.40.0 have the Fleet API; a pinned commit is checked after it is installed.
    stargate_api = {"on": True, "off": False}.get(answers["stargateApi"])
    if stargate_api is None and stargate_tag != "pinned":
        stargate_api = tuple(int(part) for part in stargate_tag[1:].split(".")) >= (0, 40, 0)

    # The API and PostgreSQL come first. On a single host, Ergo keeps its history in the
    # same PostgreSQL (a second database), so the fleet installer runs once with both.
    # Keep the agents away from operator-only ports and cloud metadata before any service starts.
    # This never blocks the operator, so it applies at once and is never rolled back.
    run(HERE / "lockdown.sh", "isolate", *isolation_args(answers))
    api_block, token, history = None, "", None
    if answers["api"]:
        run(HERE / "docker.sh")
        dev_auth = api_up(answers, secret_store)
        if local_irc:
            history = history_database(secret_store)
        if dev_auth:
            # The first sign-in to an organisation becomes its administrator, so sign the admin in first.
            token = api_token(answers)
            if answers["demo"]:
                seed_demo(answers)
            channels = fleet_channels(answers, members)
            initiative = fleet_initiative(token, answers["name"], channels, answers.get("initiativeId"),
                                          answers.get("managedChannels", []), answers["slug"])
            answers.update(initiativeId=initiative, managedChannels=channels)
            save_answers(answers)
            api_block = {"url": f"http://{LOOPBACK}:{API_PORT}", "initiatives": [initiative]}
        else:
            say("the API no longer offers development sign-in, so setup leaves its fleet registration, "
                "initiative and demo data as they are")

    write_inputs(irc_ref, api_block, bool(stargate_api), token, history)
    installer(config_path)
    if not answers["api"]:
        api_down()
    if stargate_api is None:
        source = installed_stargate_source()
        stargate_api = bool(source and (source / "src/api/v1/server.ts").is_file())
        if stargate_api:
            write_inputs(irc_ref, api_block, True, token, history)
            installer(config_path)
        else:
            say("the pinned Stargate has no Fleet API; leaving it off")
    elif stargate_api and not ((source := installed_stargate_source()) and (source / "src/api/v1/server.ts").is_file()):
        raise SetupError("The installed Stargate has no Fleet API. Pass --stargate-api off or a newer --stargate-ref.")

    lockdown_id = ""
    if answers["lockdown"]:
        lockdown = [HERE / "lockdown.sh", "apply", *isolation_args(answers)]
        for address in answers["sshFrom"]:
            lockdown += ["--ssh-from", address]
        for value in answers["allowPorts"]:
            lockdown += ["--allow-port", str(value)]
        if answers["access"] in ("tailscale", "cloudflare"):
            lockdown.append(f"--{answers['access']}")
        run(*lockdown)
        lockdown_id = run(HERE / "lockdown.sh", "applied-id", capture=True)
    else:
        say("WARNING: lockdown skipped. Run 'sudo subspace-setup lockdown apply' before putting anything on this VM.")
    values = summary(answers, members, stargate_api, lockdown_id)
    if run(HERE / "audit.sh", check=False):
        say("WARNING: the privacy audit found problems (FAIL lines above). Fix them before using this fleet.")
    run("sync")  # Persist tokens, inputs and lockdown before the host reads them.
    channels = values["SUBSPACE_CHANNELS"].split()
    print()
    say(f"fleet {values['SUBSPACE_FLEET_NAME']} ({values['SUBSPACE_FLEET_SLUG']}) is set up.")
    say(f"Starbase is {channels[0]}; ship channels: {' '.join(channels[1:])}.")
    if not local_irc:
        say(f"add this fleet to the shared IRC server: copy {INPUTS / 'irc-bundle.json'} there and run "
            "'sudo subspace-setup irc-server add <file>' (the host installer does this with --irc-server).")
    say(f"forward these loopback ports to reach it: {values['SUBSPACE_PORTS']}")
    if run(HERE / "providers.sh", "status", check=False):
        say("next: bring the agents online by signing CLIProxyAPI in to a provider account "
            "(INSTALL.md, 'Bring the agents online').")


def isolation_args(answers):
    """lockdown.sh options that keep the agents away from operator-only ports and cloud metadata."""
    args = ["--operator", answers["operator"], "--protect-metadata"]
    if answers["api"] or (INPUTS / "api.env").is_file():
        # The API (development sign-in) and PostgreSQL, also through their container addresses,
        # and still after a switch to --no-api in case stopping them failed.
        args += ["--operator-only-port", str(API_PORT), "--operator-only-port", "5432"]
    for user in metadata_users(answers):
        args += ["--metadata-user", user]
    return args


def metadata_users(answers):
    """Users besides root and the operator that may reach the cloud metadata service.

    Stargate's Teams bot signs in with the VM's managed identity, so its gateway user needs it.
    """
    users = list(answers.get("metadataUsers", []))
    if read_json(INPUTS / "fleet.json", {}).get("stakeholders", {}).get("teams"):
        checks = read_json(FLEET_ETC / "gateway-checks.json", []) if (FLEET_ETC / "gateway-checks.json").is_file() else []
        gateway = next((user for user, path in checks if str(path).endswith("/gateway.json")), None)
        require(gateway, "Teams is configured, but no primary Stargate gateway is installed.")
        # With a Claude Agent SDK rear admiral, the gateway runs as the rear admiral's own user.
        require(not answers.get("keyVault") or gateway == GATEWAY_USER,
                "Teams signs in through Stargate's gateway user, which here is the rear admiral's own user, so "
                "the rear admiral could use the VM's Key Vault identity. Use an OpenCode rear admiral, or keep "
                "Key Vault and Teams on separate VMs.")
        users.append(gateway)
    for user in users:
        try:
            pwd.getpwnam(user)
        except KeyError:
            raise SetupError(f"The metadata user {user} does not exist on this VM.") from None
    return list(dict.fromkeys(users))


def print_secret(args):
    store = read_json(INPUTS / "generated-secrets.json", {})
    answers = read_json(INPUTS / "answers.json", {})
    require(answers.get("irc", "local") == "local",
            "This fleet uses a shared IRC server, which holds the human accounts. Ask it with "
            "'sudo subspace-setup irc-server secret <account>' on that host.")
    names = {"irc-admin": f"irc-{answers.get('admin', '')}", "irc-operator": "irc-operator"}
    value = store.get(names[args.name])
    require(value, "That secret has not been generated yet. Run setup first.")
    sys.stdout.write(value + "\n")


def print_token(_args):
    answers = read_json(INPUTS / "answers.json", {})
    require(answers.get("api"), "The Subspace API is not set up on this VM.")
    sys.stdout.write(api_token(answers) + "\n")


def main(argv):
    if argv and argv[0] == "secret":
        parser = argparse.ArgumentParser(prog="subspace-setup secret")
        parser.add_argument("name", choices=["irc-admin", "irc-operator"])
        print_secret(parser.parse_args(argv[1:]))
    elif argv and argv[0] == "token":
        print_token(argv[1:])
    elif argv and argv[0] == "irc-bundle":
        bundle = INPUTS / "irc-bundle.json"
        require(bundle.is_file(), "This fleet uses local IRC, so it has no bundle for a shared server.")
        sys.stdout.write(bundle.read_text())
    elif argv and argv[0] == "fleet-registration":
        answers = read_json(INPUTS / "answers.json", {})
        require(answers.get("slug"), "Set the fleet up first.")
        rear, captains = check_answers(answers)
        members = roster(answers, rear, captains)
        sys.stdout.write(json.dumps(fleet_registration(answers, members), indent=2) + "\n")
    elif argv and argv[0] == "register-fleet":
        if argv[1:] == ["--check"]:
            registration_ready()
            say("this VM's API can register further fleets")
            return
        require(argv[1:] == ["-"], "Usage: subspace-setup register-fleet - < registration.json | --check")
        try:
            registration = json.loads(sys.stdin.read())
        except ValueError:
            raise SetupError("The fleet registration is not valid JSON.") from None
        register_fleet(registration)
    elif argv and argv[0] == "demo":
        answers = read_json(INPUTS / "answers.json", {})
        require(answers.get("api"), "The Subspace API is not set up on this VM.")
        api_token(answers)
        seed_demo(answers)
    else:
        if argv and argv[0] == "setup":
            argv = argv[1:]
        setup(parse_args(argv))


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except SetupError as error:
        sys.exit(f"subspace-setup: {error}")
    except KeyboardInterrupt:
        sys.exit(130)
