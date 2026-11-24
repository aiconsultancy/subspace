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
4. starts the Subspace API and PostgreSQL on loopback (or the API alone against an
   existing PostgreSQL, --database-url-file, migrating it only with --migrate), signs the admin in
   first so they become the organisation administrator, seeds demo data on
   request, creates an initiative for the fleet and registers the fleet, its
   standard channels, a ship per captain and the rear admiral's and captains'
   agent records (lib/fleet_records.py);
5. enables Stargate's Fleet API when the pinned Stargate has it, and agent
   dispatch when asked (off by default), giving the rear admiral's gateway a
   token for the rear admiral's own agent identity;
6. locks the VM down (see lockdown.sh) and writes a non-secret summary that
   the host reads.

Re-running reuses the saved answers and secrets, so it is safe to run again
after a failure or to change a setting.
"""

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, quote, urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener, urlopen

HERE = Path(__file__).resolve().parent
PAYLOAD = HERE.parents[1]
INSTALLER = PAYLOAD / "subspace-installer"
# The standard fleet channels are defined once, beside the fleet installer that enforces them.
sys.path.insert(0, str(HERE.parent / "lib"))
import fleet_records  # noqa: E402
import irc_accounts  # noqa: E402
import postgres_check  # noqa: E402
import runtime  # noqa: E402
import standard_channels  # noqa: E402
INPUTS = Path("/root/fleet")
API_DIR = Path("/opt/subspace/api")
FLEET_APP = Path("/opt/subspace-fleet")
FLEET_ETC = Path("/etc/subspace-fleet")
API_PROJECT = "subspace-api"
API_PORT = 3000
# The API's agent-only listener: a fleet agent names itself in a header there and needs no token. It serves no
# sign-in or administrator route, so unlike API_PORT the agents' Linux users may reach it.
AGENT_API_PORT = 3001
# The line in api.env that records setup has offered the agent listener, so an operator's "off" is kept.
AGENT_OFFERED = "# subspace-setup has offered the agent listener (AGENT_PORT=3001); AGENT_PORT= keeps it off."
STARGATE_API_PORT = 8484
STARGATE_REPO = "https://github.com/aiconsultancy/stargate.git"
OUTBOUND_TIMEOUT = 8
OUTBOUND_HELP = ("Open outbound HTTPS (TCP 443) from the VM to github.com and *.githubusercontent.com, "
                 "download.docker.com and the package mirror in /etc/apt (HTTP, TCP 80, unless it uses HTTPS), in "
                 "the VM's network security group, firewall or proxy settings, then run setup again.")
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
# Agent dispatch: what switching it on does, in the words the prompt and the summary use.
DISPATCH_SUMMARY = ("issues flagged Ready for agents are offered to the captains in fleet command, and issues "
                    "assigned to a captain are announced to it there")
# The fleet installer's repository IDs (also Stargate crew project keys) and its crew limit of nine repositories.
REPO_ID = re.compile(r"[a-z][a-z0-9_-]{0,39}")
MAX_REPOSITORIES = 9
# A crew member's model ID, as the fleet installer accepts it (runtime.MODEL).
CREW_MODEL = re.compile(r"[a-zA-Z0-9][a-zA-Z0-9._/:#-]{0,127}")
# An officer's model ID and reasoning effort (runtime.harness_config checks the effort against the model).
OFFICER_MODEL = re.compile(r"[a-zA-Z0-9][a-zA-Z0-9._/:-]{0,127}")
EFFORTS = ("low", "medium", "high", "xhigh", "max")
# The officers' model before each role had its own default. A fleet set up then keeps it until it chooses.
LEGACY_OFFICER_MODEL = "gpt-5.4"
# The API on this VM against an existing PostgreSQL (--database-url-file). The URL file is root's alone; the URL
# is written only to the root-only api.env, single-quoted so Compose takes it literally, hence no quote in it.
DATABASE_URL_TEXT = re.compile(r"[A-Za-z0-9._~:/?#\[\]@!$&()*+,;=%-]+")
DATABASE_URL_MAX = 4096
SECRET_OWNER = 0  # root
# A CA certificate the API trusts for that server (--database-ca-file): public, so mounted read-only for the
# API's own user. The API's PostgreSQL driver ignores the URL's sslrootcert, so Bun's NODE_EXTRA_CA_CERTS adds it.
DATABASE_CA = "/etc/subspace/database-ca.pem"
# The exit code of the API's `scripts/migrate.ts --check` when the database lacks migrations the API needs.
MIGRATIONS_PENDING = 3


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


def run(*args, check=True, capture=False, env=None, input_text=None, timeout=None, keep_stdin=True):
    """keep_stdin=False gives the command no standard input (unless input_text is its input), so it cannot read
    on from setup's own: a script piped to 'subspace-setup token' over SSH, say."""
    stdin = subprocess.DEVNULL if input_text is None and not keep_stdin else None
    try:
        result = subprocess.run([str(arg) for arg in args], check=False, text=True, env=env, input=input_text,
                                stdin=stdin, stdout=subprocess.PIPE if capture else None, timeout=timeout)
    except subprocess.TimeoutExpired:
        raise SetupError(f"{Path(str(args[0])).name} {args[1] if len(args) > 1 else ''} did not finish within "
                         f"{timeout} seconds.") from None
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
    parser.add_argument("--crew-claude-model", help="model of each Claude-family crew member, such as claude-opus-5-5 "
                        "(default claude-sonnet-5-5; kept on later runs; '' returns to the default)")
    parser.add_argument("--crew-codex-model", help="model of each Codex-family crew member, such as gpt-6.2-sol "
                        "(default gpt-5.4; kept on later runs; '' returns to the default)")
    for role, flag, default in (("captain", "captain", runtime.OFFICER_DEFAULTS["captain"]),
                                ("rear_admiral", "rear-admiral", runtime.OFFICER_DEFAULTS["rear_admiral"])):
        who = "each captain" if role == "captain" else "the rear admiral"
        parser.add_argument(f"--{flag}-model", help=f"model of {who}, such as gpt-5.4 (default {default['model']}; "
                            "a claude-* model goes through the Claude family, any other through Codex; kept on later "
                            "runs; '' returns to the default)")
        parser.add_argument(f"--{flag}-effort", help=f"reasoning effort of {who}: {', '.join(EFFORTS)} (default "
                            f"{default['effort']} on {default['model']}, else the provider's; kept on later runs; '' "
                            "returns to the default)")
    parser.add_argument("--demo", dest="demo", action="store_true", default=None, help="seed fictional demo data")
    parser.add_argument("--no-demo", dest="demo", action="store_false")
    parser.add_argument("--irc", choices=["local", "remote"],
                        help="local runs Ergo on this VM; remote uses a shared IRC server for several fleets")
    parser.add_argument("--irc-host", help="shared IRC server's DNS name (remote mode; must match its TLS certificate)")
    parser.add_argument("--irc-port", type=int, help="shared IRC server's TLS port (default 6697)")
    parser.add_argument("--irc-ca", help="absolute path on this VM to the shared server's CA certificate (PEM)")
    parser.add_argument("--irc-auth", choices=["certificate", "password"],
                        help="how officers sign in to a remote IRC server: certificate (SASL EXTERNAL, the default; "
                        "setup prints the NickServ lines that bind each one) or password (SASL PLAIN, for the shared "
                        "server ./install.sh irc-server runs). Kept on later runs")
    parser.add_argument("--irc-operator", help="an IRC operator's name (OPER) with NickServ's accreg capability: setup "
                        "then registers each officer's account and binds its certificate itself (certificate sign-in)")
    parser.add_argument("--irc-operator-account", help="an IRC account the operator's connection signs in to first "
                        "(SASL PLAIN), for a server that admits only signed-in connections (Ergo's require-sasl)")
    parser.add_argument("--irc-operator-account-password-file", help="absolute path to a root-only file (mode 0600) "
                        "holding that account's password. Read for this run only; never printed or saved")
    parser.add_argument("--irc-operator-password-file", help="absolute path to a root-only file (mode 0600) holding "
                        "that operator's password. Read for this run only; never printed or saved")
    parser.add_argument("--api-url", help="use a Subspace API on another host, such as https://subspace.example.com, "
                        "instead of one on this VM: no API container or PostgreSQL here, and the officers enrol "
                        "their own keys through Stargate. Needs --api-enrolment-token-file on the first run and a "
                        "Stargate with subspaceAgents (--stargate-ref). Kept on later runs")
    parser.add_argument("--api-enrolment-token-file", help="absolute path to a root-only file (mode 0600) holding "
                        "the fleet's enrolment token (senr1.…) from that API's administrator. Never printed")
    parser.add_argument("--switch-to-hosted-api", action="store_true",
                        help="with --api-url on a VM whose API runs here: switch anyway, leaving that tracker behind")
    parser.add_argument("--vm-host", help="the name or address operators reach this VM by, recorded as the fleet's "
                        "vmHost in the Subspace API (the host installers pass the --remote host). Kept on later runs; "
                        "'' removes it")
    parser.add_argument("--no-api", dest="api", action="store_false", default=None,
                        help="skip the Subspace API and PostgreSQL on this VM")
    parser.add_argument("--database-url-file", help="run the API on this VM against an existing PostgreSQL, with no "
                        "PostgreSQL container here: absolute path to a root-only file (mode 0600) holding its "
                        "postgresql://user:password@host:5432/database?sslmode=verify-full URL. Kept on later runs")
    parser.add_argument("--database-ca-file", help="absolute path on this VM to the CA certificate (PEM) of that "
                        "PostgreSQL server, when the system trust store lacks it; '' removes it")
    parser.add_argument("--migrate", action="store_true", help="apply the API's database migrations to that existing "
                        "PostgreSQL on this run (never by default; see the installer README)")
    parser.add_argument("--stargate-api", choices=["auto", "on", "off"],
                        help="enable Stargate's Fleet API (default auto: when the pinned Stargate has it)")
    parser.add_argument("--fleet-api-url", help="the HTTPS address at which a desktop reaches Stargate's Fleet API, "
                        "such as a Tailscale or Cloudflare name, recorded in the Subspace API; 'tunnel' (the default) "
                        "records the SSH tunnel's end on the desktop, http://127.0.0.1:8484")
    parser.add_argument("--stargate-ref", help="'latest' (the default: Stargate's newest release, looked up on "
                        "every run) or a full commit SHA to pin")
    parser.add_argument("--agent-dispatch", dest="agent_dispatch", action="store_true", default=None,
                        help="switch on Stargate's agent dispatch: " + DISPATCH_SUMMARY + " (default off; needs the API)")
    parser.add_argument("--no-agent-dispatch", dest="agent_dispatch", action="store_false")
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
    parser.add_argument("--dry-run", action="store_true", help="validate the inputs only, changing nothing")
    parser.add_argument("--non-interactive", action="store_true", help="never prompt; fail on missing answers")
    parser.add_argument("--captain-tools", choices=["standard", "delegate"],
                        help="standard lets captains code; delegate reserves coding for crew and lets captains "
                        "inspect changes, create GitHub PRs and use the Subspace API. Kept on later runs")
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
    if args.captain_tools is not None:
        answers["captainTools"] = args.captain_tools
    elif interactive and not saved and not (INPUTS / "fleet.json").exists():
        policy = prompt("Captain tools: standard (captains can code) or delegate (crew code; captains manage work, PRs and issues)",
                        "standard", interactive)
        require(policy in ("standard", "delegate"), "Choose standard or delegate for --captain-tools.")
        answers["captainTools"] = policy
    models = answers.setdefault("crewModels", {})
    for family, flag in (("claude", args.crew_claude_model), ("codex", args.crew_codex_model)):
        if flag == "":
            models[family] = ""  # The fleet installer's default, even if fleet.json names a model.
        elif flag is not None:
            require(CREW_MODEL.fullmatch(flag), f"--crew-{family}-model must be a model ID, such as "
                    f"{'claude-opus-5-5' if family == 'claude' else 'gpt-6.2-sol'}.")
            models[family] = flag
    officers = answers.get("officerModels")
    if officers is None:
        officers = {}
        harness = read_json(INPUTS / "fleet.json", {}).get("harness")
        chosen = isinstance(harness, dict) and harness.keys() & {"family", "model", "captain", "rearAdmiral"}
        if saved.get("slug") and not chosen:
            # Set up when every officer ran gpt-5.4: it keeps that model until the operator chooses.
            officers = {role: {"model": LEGACY_OFFICER_MODEL, "effort": ""} for role in ("captain", "rearAdmiral")}
            if args.captain_model is None or args.rear_admiral_model is None:
                say(f"this fleet was set up when officers defaulted to {LEGACY_OFFICER_MODEL}, so they keep it. To "
                    "move them to the default models (Claude, which needs a claude account in the model proxy): "
                    "subspace-setup --captain-model '' --rear-admiral-model '' (installer README, 'Officer models').")
    for role, model, effort in (("captain", args.captain_model, args.captain_effort),
                                ("rearAdmiral", args.rear_admiral_model, args.rear_admiral_effort)):
        flag = "captain" if role == "captain" else "rear-admiral"
        # '' is the fleet installer's default, even over one set in fleet.json by hand.
        choice = officers.setdefault(role, {})
        if model is not None:
            require(model == "" or OFFICER_MODEL.fullmatch(model), f"--{flag}-model must be a model ID, such as "
                    f"claude-opus-5-5. Give the reasoning effort with --{flag}-effort.")
            # An effort belongs to its model: a new model has its default effort unless this run names one.
            choice.update(model=model, effort="")
        if effort is not None:
            require(effort == "" or effort in EFFORTS, f"--{flag}-effort must be one of {', '.join(EFFORTS)}.")
            choice["effort"] = effort
    answers["officerModels"] = officers
    answers["irc"] = args.irc or answers.get("irc", "local")
    if answers["irc"] == "remote":
        pick("ircHost", args.irc_host, "Shared IRC server's DNS name")
        if args.irc_port is not None:
            answers["ircPort"] = args.irc_port
        answers.setdefault("ircPort", IRC_TLS)
        if args.irc_ca is not None:
            answers["ircCa"] = args.irc_ca
        if args.irc_auth:
            answers["ircAuth"] = args.irc_auth
        elif installed_irc_auth():
            # Keep how the installed fleet signs in, which the fleet installer alone may have changed since.
            answers["ircAuth"] = installed_irc_auth()
        elif "ircAuth" not in answers:
            # A remote fleet set up before certificates were the default signs in with passwords; keep it so.
            answers["ircAuth"] = "password" if saved.get("irc") == "remote" else "certificate"
    else:
        require(args.irc_auth in (None, "password"), "--irc-auth certificate needs a remote IRC server (--irc remote): "
                "officers reach the Ergo on this VM without TLS, which certificates need.")
        answers.pop("ircAuth", None)
    require(not (args.irc_operator or args.irc_operator_password_file) or answers.get("ircAuth") == "certificate",
            "--irc-operator is for certificate sign-in on a remote IRC server (--irc remote --irc-auth certificate).")
    require(bool(args.irc_operator) == bool(args.irc_operator_password_file),
            "Pass --irc-operator and --irc-operator-password-file together.")
    require(bool(args.irc_operator_account) == bool(args.irc_operator_account_password_file),
            "Pass --irc-operator-account and --irc-operator-account-password-file together.")
    require(not args.irc_operator_account or args.irc_operator,
            "--irc-operator-account is the account the operator's connection signs in to: pass --irc-operator too.")
    hosted_api(args, answers, saved)
    if args.vm_host == "":
        answers.pop("vmHost", None)
    elif args.vm_host is not None:
        try:
            answers["vmHost"] = runtime.host(args.vm_host.strip().lower())
        except runtime.Invalid:
            raise SetupError("--vm-host must be a DNS name or an IP address, such as fleet.example.com.") from None
    if answers.get("apiUrl"):
        answers["api"] = False  # The hosted API replaces one on this VM.
    elif args.api is not None:
        answers["api"] = args.api
    elif args.database_url_file and answers.get("api") is False:
        # A fleet set up with --no-api: asking for an existing PostgreSQL asks for the API that uses it.
        answers["api"] = True
    answers.setdefault("api", True)
    # An existing PostgreSQL for the API: the file's path is kept, never the URL. --migrate is for this run only.
    require(not (args.database_url_file and args.api is False),
            "--database-url-file runs the API on this VM, which --no-api leaves out. Pass one or the other.")
    if args.database_url_file == "":
        # The API's tracker is in that server; a PostgreSQL on this VM would start empty, and the API image's
        # automatic migration would run against whatever DATABASE_URL api.env still names.
        require(not answers.get("databaseUrlFile"),
                f"This VM's API keeps its tracker in the existing PostgreSQL named in {answers.get('databaseUrlFile')}, "
                "and setup has no way back to a PostgreSQL on this VM. Keep --database-url-file, or restore a dump "
                "of that database on a fresh VM (installer README, 'An existing PostgreSQL').")
        answers.pop("databaseUrlFile", None)
    elif args.database_url_file is not None:
        answers["databaseUrlFile"] = args.database_url_file
    if args.database_ca_file == "":
        answers.pop("databaseCaFile", None)
    elif args.database_ca_file is not None:
        answers["databaseCaFile"] = args.database_ca_file
    require(not args.migrate or (answers.get("databaseUrlFile") and answers["api"]),
            "--migrate is for an existing PostgreSQL (--database-url-file): the API migrates its own PostgreSQL "
            "on this VM itself.")
    if answers.get("databaseUrlFile") and args.demo is None:
        answers["demo"] = False  # Fictional data never goes into a database the installer did not create.
    if args.demo is not None:
        answers["demo"] = args.demo
    elif "demo" not in answers:
        answers["demo"] = yes_no("Seed fictional demo data so you can look around straight away?", True, interactive)
    if args.agent_dispatch is not None:
        answers["agentDispatch"] = args.agent_dispatch
    elif "agentDispatch" not in answers:
        # Off unless asked: an unattended run never switches it on.
        answers["agentDispatch"] = yes_no(f"Switch on agent dispatch? ({DISPATCH_SUMMARY[0].upper()}"
                                          f"{DISPATCH_SUMMARY[1:]}.)", False, interactive)
    answers["stargateApi"] = args.stargate_api or answers.get("stargateApi", "auto")
    if args.fleet_api_url == "tunnel":
        answers.pop("fleetApiUrl", None)
    elif args.fleet_api_url is not None:
        answers["fleetApiUrl"] = args.fleet_api_url
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
    """'URL' or 'ID=URL' items as [ID, URL, named]. The ID defaults to the repository's name."""
    rows = []
    for item in (part.strip() for part in value.split(",")):
        if not item:
            continue
        named = re.fullmatch(r"([a-z][a-z0-9_-]{0,39})=(.+)", item)
        url = named.group(2) if named else item
        try:
            path = urlsplit(url).path
        except ValueError:
            path = ""
        name = named.group(1) if named else re.sub(
            r"[^a-z0-9_-]+", "-", path.rstrip("/").rpartition("/")[2].removesuffix(".git").lower()).strip("-")
        rows.append([name, url, bool(named)])
    return rows


def check_repositories(rows):
    for index, (name, url, _named) in enumerate(rows, 1):
        try:
            parts = urlsplit(url)
            host = parts.hostname
        except ValueError:
            parts, host = None, None
        # Name a rejected item by position and host only: the URL may carry credentials.
        require(parts and parts.scheme == "https" and host and parts.path.strip("/") and not parts.username
                and not parts.password and not parts.query and not parts.fragment,
                f"Repository {index} ({host or 'no host'}) needs an HTTPS Git URL without credentials, "
                "a query or a fragment.")
        require(REPO_ID.fullmatch(name) and name != "default",
                f"Give repository {url} an ID that starts with a letter, as ID=URL (for example web={url}).")
    require(len({row[0] for row in rows}) == len(rows) and len({row[1] for row in rows}) == len(rows),
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
        require(len(f"{answers['slug']}-{agent}") <= standard_channels.MAX_ACCOUNT,
                f"'{answers['slug']}-{agent}' is too long for its IRC account. Use a shorter fleet ID or name.")
    trouble = standard_channels.problem(answers["slug"], ids[1:])
    require(trouble is None, trouble)
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
    if answers.get("fleetApiUrl"):
        try:
            answers["fleetApiUrl"] = fleet_records.fleet_api_url(answers["fleetApiUrl"], "--fleet-api-url")
        except fleet_records.Refused as error:
            raise SetupError(str(error)) from None
        require(answers["stargateApi"] != "off",
                "--fleet-api-url is the address of Stargate's Fleet API, which --stargate-api off turns off. Pass "
                "--fleet-api-url tunnel, or switch the Fleet API on.")
    require(isinstance(answers.get("agentDispatch", False), bool), "Agent dispatch is on or off.")
    require(not answers.get("agentDispatch") or answers["api"],
            "Agent dispatch polls the Subspace API, so it needs the API on this VM: drop --no-api, or pass "
            "--no-agent-dispatch.")
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
    # The API keeps the rear admiral's name as its agent name and each captain's as its ship's name.
    require(fleet_records.AGENT_NAME.fullmatch(answers["rearAdmiral"]),
            "The rear admiral's name is also its agent name in the Subspace API: use letters, digits and hyphens, "
            "at most 64, such as Atlas.")
    require(all(len(name) <= 64 for name in answers["captains"]), "Ship names must be at most 64 characters.")
    require(not answers.get("databaseCaFile") or answers.get("databaseUrlFile"),
            "--database-ca-file is the CA of an existing PostgreSQL: pass --database-url-file too.")
    if external_database(answers):
        database_url(answers["databaseUrlFile"])
        require(not answers["demo"], "Demo data is fictional and never goes into a database the installer did not "
                "create: drop --demo with --database-url-file.")
        if answers.get("databaseCaFile"):
            database_ca(answers["databaseCaFile"])
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


def apt_mirrors(sources=Path("/etc/apt")):
    """The package mirrors this VM's enabled apt sources name (one-line and deb822 formats), as base URLs."""
    found = []
    files = [sources / "sources.list", *sorted((sources / "sources.list.d").glob("*.list")),
             *sorted((sources / "sources.list.d").glob("*.sources"))]
    for path in files:
        try:
            text = path.read_text()
        except OSError:
            continue
        if path.suffix == ".sources":
            # deb822: one stanza per paragraph; "Enabled: no" switches a stanza off.
            for stanza in re.split(r"\n\s*\n", text):
                fields = {}
                for line in stanza.splitlines():
                    key, _, value = line.split("#", 1)[0].partition(":")
                    if value:
                        fields[key.strip().lower()] = value.split()
                if fields.get("enabled", ["yes"])[0].lower() != "no":
                    found += fields.get("uris", [])
            continue
        for line in text.splitlines():
            words = line.split("#", 1)[0].split()
            if words[:1] in (["deb"], ["deb-src"]):
                found += [word for word in words[1:] if not word.startswith("[")][:1]
    return list(dict.fromkeys(url for url in found if url.startswith(("http://", "https://"))))


def apt_proxies():
    """apt's own proxies by scheme (Acquire::http::Proxy, Acquire::https::Proxy), which its downloads use."""
    try:
        dump = subprocess.run(["apt-config", "dump"], capture_output=True, text=True, timeout=10, check=False).stdout
    except (OSError, subprocess.TimeoutExpired):
        return {}
    proxies = {}
    for line in (dump or "").splitlines():
        match = re.fullmatch(r'Acquire::(https?)::Proxy "([^"]*)";', line.strip())
        if match and match.group(2).lower() not in ("", "direct", "false"):
            proxies[match.group(1)] = match.group(2)
    return proxies


def docker_ready():
    """Whether Docker with its Compose plugin is already installed, so Docker's repository is not needed."""
    if not shutil.which("docker"):
        return False
    try:
        return subprocess.run(["docker", "compose", "version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                              timeout=20, check=False).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def outbound_targets(answers):
    """What the install downloads from, as (label, URL, proxy): GitHub (Stargate and the pinned releases),
    Docker's repository when the VM runs containers and has no Docker yet, and the package mirrors, probed
    through apt's own proxy as apt reaches them."""
    targets = [("GitHub", "https://github.com", None)]
    if (answers.get("api") or answers.get("irc") == "local") and not docker_ready():
        targets.append(("Docker's package repository", "https://download.docker.com", None))
    proxies = apt_proxies()
    targets += [("the package mirror", url, proxies.get(urlsplit(url).scheme)) for url in apt_mirrors()]
    return targets


def reachable(url, proxy=None):
    """Whether a URL answers at all. proxy: one to use instead of the environment's, such as apt's."""
    opener = build_opener(ProxyHandler({"http": proxy, "https": proxy})) if proxy else None
    request = Request(url, method="HEAD", headers={"User-Agent": "subspace-setup"})
    try:
        with (opener.open if opener else urlopen)(request, timeout=OUTBOUND_TIMEOUT):
            return True
    except HTTPError:
        return True  # It answered.
    except (URLError, OSError, ValueError):
        return False


def check_outbound(answers):
    """Stop at once, saying what to open, when the VM cannot reach what the install downloads; without this a
    run with no outbound access waits minutes on its first download and fails without saying why."""
    targets = outbound_targets(answers)
    with ThreadPoolExecutor(max_workers=len(targets)) as pool:
        results = list(pool.map(lambda target: reachable(target[1], target[2]), targets))
    missing = [f"{label} ({urlsplit(url).hostname}{' through apt' + chr(39) + 's proxy' if proxy else ''})"
               for (label, url, proxy), ok in zip(targets, results) if not ok]
    require(not missing, "This VM has no outbound access to " + ", ".join(missing) + ", which the install "
            "downloads from. " + OUTBOUND_HELP)


def latest_stargate():
    """Stargate's newest release (highest vX.Y.Z tag) as (tag, commit), so every run installs the latest."""
    try:
        output = run("git", "ls-remote", "--tags", STARGATE_REPO, "refs/tags/v*", capture=True, timeout=60)
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
    require(releases, f"Could not look up Stargate's latest release at {STARGATE_REPO}: the VM has no outbound "
            "access to GitHub, or it is too slow. " + OUTBOUND_HELP + " Or pin a release with --stargate-ref <full SHA>.")
    tag = max(releases, key=lambda name: releases[name][0])
    return tag, releases[tag][1]


def fleet_config(answers, members, irc_ref, api_block=None, stargate_api=False, stargate_ref=None, history=None):
    # The fleet installer names Starbase, the command channel and the ship channels itself (standard_channels).
    slug = answers["slug"]
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
            "allowedAccounts": [answers["admin"]],
            # Certificates by default. The shared server ./install.sh irc-server runs (irc_server.py) seeds each
            # officer's account with the password in this fleet's bundle, so those fleets use passwords.
            "auth": answers.get("ircAuth", "password"),
        }
        if answers.get("ircCa"):
            config["irc"]["caFile"] = answers["ircCa"]
    previous = read_json(INPUTS / "fleet.json", {})
    if policy := answers.get("captainTools", previous.get("captainTools")):
        config["captainTools"] = policy
    # Each crew family's model: this or an earlier run's answer, else one set in fleet.json by hand.
    for family, crew in config["crew"].items():
        chosen = answers.get("crewModels", {})
        model = chosen[family] if family in chosen else (previous.get("crew", {}).get(family) or {}).get("model")
        if model:
            crew["model"] = model
    # Keep anything the operator added by hand, such as private repositories, GitHub Apps or stakeholders.
    for key in ("harness", "stakeholders", "usage", "medical", "toolchains", "maintenance",
                "githubPolicy", "dashboard", "proxy", "agentsview"):
        if key in previous:
            config[key] = previous[key]
    # Each role's model and effort: this or an earlier run's answer, over one set in fleet.json by hand.
    harness = dict(config.get("harness") or {})
    officers = answers.get("officerModels", {})
    resets = [role for role in OFFICER_ROLES if any(key in officers.get(role, {}) and not officers[role][key]
                                                    for key in ("model", "effort"))]
    shared = {key: harness[key] for key in ("family", "model", "effort") if key in harness}
    if resets and shared:
        # '' returns a role to its default, so a fleet-wide model set by hand no longer applies to it. Each role
        # takes what it ran as its own block first, so the other role keeps it; the reset below then clears it.
        resolved = {role: resolved_officer(harness, role) for role in OFFICER_ROLES}
        for key in shared:
            harness.pop(key)
        for role in OFFICER_ROLES:
            block = {key: val for key, val in (harness.get(role) or {}).items() if key not in ("family", "model", "effort")}
            harness[role] = {**block, **resolved[role]}
        kept = [role for role in OFFICER_ROLES if role not in resets]
        say(f"fleet.json's fleet-wide harness {', '.join(f'{key} {val}' for key, val in shared.items())} no longer "
            f"applies to {' or '.join(OFFICER_ROLES[role] for role in resets)}"
            + (f"; {' and '.join(OFFICER_ROLES[role] for role in kept)} keeps it, now as "
               f"{' and '.join(f'harness.{role}' for role in kept)}." if kept else "."))
    for role, choice in officers.items():
        block = dict(harness.get(role) or {})
        if choice.get("model"):
            block.update(model=choice["model"], family=runtime.model_family(choice["model"]))
        elif "model" in choice:
            block.pop("model", None)
            block.pop("family", None)
        if choice.get("effort"):
            block["effort"] = choice["effort"]
        elif "effort" in choice:
            block.pop("effort", None)
        if block:
            harness[role] = block
        else:
            harness.pop(role, None)
    if harness:
        config["harness"] = harness
    else:
        config.pop("harness", None)
    repositories = previous.get("repositories", [])
    if "repositories" in answers:
        # This run's answer replaces the list; a repository that stays keeps its visibility, and its ID
        # unless this answer names one.
        known = {row.get("url"): row for row in repositories if isinstance(row, dict)}
        repositories = [{**known[url], "id": name} if named and url in known else known.get(url, {"id": name, "url": url})
                        for name, url, named in answers["repositories"]]
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
        config["api"] = dict(api_block)
        # An address the operator gave; without one, the installer records the SSH tunnel's end.
        if stargate_api and answers.get("fleetApiUrl"):
            config["api"]["fleetApiUrl"] = answers["fleetApiUrl"]
    elif answers.get("apiUrl"):
        # A hosted API: the officers enrol through Stargate, and this VM keeps no administrator token.
        config["api"] = {"url": answers["apiUrl"], "enrolment": True}
    # Timings someone set by hand stay; the answer switches dispatch on or off. It needs the API block.
    dispatch = previous.get("agentDispatch") if isinstance(previous.get("agentDispatch"), dict) else {}
    if dispatch or answers.get("agentDispatch"):
        config["agentDispatch"] = {**dispatch, "enabled": bool(answers.get("agentDispatch") and api_block)}
    return config


# fleet.json's harness block for each officer role, and who it is.
OFFICER_ROLES = {"captain": "the captains", "rearAdmiral": "the rear admiral"}


def resolved_officer(harness, role):
    """The family, model and effort a role runs with fleet.json's harness, as the fleet installer resolves it."""
    key = "captain" if role == "captain" else "rear_admiral"
    try:
        hull = runtime.harness_config({k: v for k, v in harness.items() if k not in OFFICER_ROLES},
                                      {"type": "opencode", **runtime.OFFICER_DEFAULTS[key], "permissions": None})
        result = runtime.harness_config(harness.get(role) or {}, hull)
    except runtime.Invalid as error:
        raise SetupError(f"fleet.json's harness: {error}") from None
    return {"family": result["family"], "model": result["model"],
            **({"effort": result["effort"]} if result["effort"] else {})}


def fleet_secrets(secret_store, members, admin, token="", local_irc=True, dispatch_token="", certificates=False):
    # A shared IRC server owns the human accounts; a local one is seeded with the admin's. Officers who sign in
    # with certificates have no IRC password at all.
    humans = [admin] if local_irc else []
    officers = [] if certificates else [person["account"] for person in members]
    passwords = {account: secret_store.password(f"irc-{account}") for account in officers + humans}
    result = {
        "proxyKey": secret_store.get("proxy-key", lambda: secrets.token_urlsafe(32)),
        "ircPasswords": passwords,
    }
    if local_irc:
        result["ircOperatorPassword"] = secret_store.password("irc-operator")
    if token:
        result["apiToken"] = token
    if dispatch_token:
        result["rearAdmiralApiToken"] = dispatch_token
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


def installer(config_path, dry_run=False, secrets_dir=None):
    args = [INSTALLER / "install.sh", "--config", config_path]
    if secrets_dir:
        args += ["--secrets-dir", secrets_dir]
    if dry_run:
        args.append("--dry-run")
    say("running the fleet installer" + (" (dry run)" if dry_run else ""))
    run(*args)


def preflight(config, private):
    """Check the fleet's inputs as the installer will (configuration, credentials, the secrets directory and
    pinned sessions), before setup changes the API or its database. A run the installer would refuse then
    migrates nothing and restarts nothing."""
    secrets_dir = INPUTS / "fleet.secrets"
    with tempfile.TemporaryDirectory(prefix="subspace-setup-check-") as scratch:
        config_path = Path(scratch) / "fleet.json"
        write_json(config_path, config)
        write_json(Path(scratch) / "fleet.secrets.json", private)
        say("checking the fleet's configuration before the API changes")
        installer(config_path, dry_run=True, secrets_dir=secrets_dir if secrets_dir.is_dir() else None)


def installed_stargate_source():
    settings = read_json(FLEET_ETC / "gateway.json", {}) if os.access(FLEET_ETC / "gateway.json", os.R_OK) else {}
    entry = settings.get("entrypoint", "")
    return Path(entry).parents[1] if entry else None


# Stargate's Fleet API server moved between the releases the installer supports: src/api/v1 in v0.40 to v0.43,
# src/observability/dashboard/api/v1 from v0.44 (and commit 5630a5c). Any other src/**/api/v1/server.ts is
# taken for a later move.
FLEET_API_SERVERS = ("src/api/v1/server.ts", "src/observability/dashboard/api/v1/server.ts")


def fleet_api_server(source):
    """Where an installed Stargate checkout keeps its Fleet API server, relative to it, or None."""
    if not source:
        return None
    for relative in FLEET_API_SERVERS:
        if (source / relative).is_file():
            return relative
    found = sorted(path.relative_to(source) for path in source.glob("src/**/api/v1/server.ts"))
    return str(found[0]) if found else None


def confirm_fleet_api(requested, source):
    """Whether the installed Stargate gets its Fleet API, once it is installed: requested is True (--stargate-api
    on) or None (auto, for a pinned commit). Asked for but missing stops setup; auto but missing says so."""
    found = fleet_api_server(source)
    looked = " or ".join(FLEET_API_SERVERS) + ", or any src/**/api/v1/server.ts"
    if found:
        say(f"Stargate's Fleet API is in {found}; it is on")
        return True
    where = f"in {source}" if source else "(no installed Stargate checkout was found)"
    require(not requested, f"--stargate-api on, but the installed Stargate has no Fleet API {where}: looked for "
            f"{looked}. Pass --stargate-api off, or a --stargate-ref that has the Fleet API (v0.40.0 or later).")
    say(f"NOTE: the installed Stargate has no Fleet API {where} (looked for {looked}), so it stays off: the Subspace "
        "API records no Fleet API address for this fleet and Fleet Console has nothing to reach. Pass a "
        "--stargate-ref that has it, or --stargate-api off to say so.")
    return False


# Subspace API ----------------------------------------------------------------

def compose(*args, capture=False, check=True, env=None, input_text=None):
    # Docker Compose passes its standard input to 'exec' and 'run' commands, which would otherwise read on from
    # setup's own, so it gets none unless it has input of its own.
    return run("docker", "compose", "--project-name", API_PROJECT, "--project-directory", API_DIR,
               "--env-file", INPUTS / "api.env", "-f", API_DIR / "compose.yaml", "-f", INPUTS / "api.override.yaml",
               *args, capture=capture, check=check, env=env, input_text=input_text, keep_stdin=False)


def hosted_api(args, answers, saved):
    """A Subspace API on another host (--api-url): kept on later runs, exclusive of an API on this VM, and never
    switched to from one without --switch-to-hosted-api."""
    if args.api_url is None:
        require(not args.api_enrolment_token_file or answers.get("apiUrl"),
                "--api-enrolment-token-file is for a hosted API: pass --api-url too.")
        require(not args.switch_to_hosted_api, "--switch-to-hosted-api goes with --api-url.")
        require(not (answers.get("apiUrl") and args.database_url_file),
                f"This fleet uses the hosted Subspace API at {answers.get('apiUrl')}, so its API has no database "
                "here: leave out --database-url-file.")
        return
    require(args.api_url != "", "setup has no way back from a hosted API to one on this VM: set up a fresh VM.")
    try:
        parts = urlsplit(args.api_url)
        parts.port  # noqa: B018 -- raises ValueError for an invalid port
    except ValueError:
        raise SetupError("--api-url must be the hosted API's origin, such as https://subspace.example.com.") from None
    require(parts.scheme == "https" and parts.hostname and parts.path in ("", "/", "/api", "/api/")
            and not (parts.query or parts.fragment or parts.username or parts.password),
            "--api-url must be the hosted API's HTTPS origin, such as https://subspace.example.com, with no path.")
    require(not args.database_url_file and not args.migrate and not args.database_ca_file,
            "--api-url uses an API on another host, which keeps its own database: leave out --database-url-file, "
            "--database-ca-file and --migrate.")
    require(not args.demo, "Demo data is fictional and never goes into a hosted API: drop --demo.")
    require(not args.agent_dispatch, "Agent dispatch needs a rear admiral token that setup mints from an API on this "
            "VM; with --api-url, leave out --agent-dispatch.")
    origin = f"https://{parts.netloc.lower()}"
    # The fleet's tracker is in an API on this VM, with its own PostgreSQL or an existing one: switching leaves it.
    tracker = not saved.get("apiUrl") and saved.get("api", True) and (
        saved.get("databaseUrlFile") or (INPUTS / "api.env").is_file())
    require(not tracker or args.switch_to_hosted_api,
            "This fleet's API runs on this VM and keeps its tracker. A hosted API starts without that tracker, and "
            "setup does not move it: pass --switch-to-hosted-api with --api-url to switch anyway, leaving it behind.")
    for key in ("databaseUrlFile", "databaseCaFile"):
        answers.pop(key, None)
    answers["apiUrl"] = origin
    answers["demo"] = False
    answers.setdefault("agentDispatch", False)


def enrolment_token(args, saved):
    """The enrolment token given on this run, read from a root-only file and checked for its form, or None.
    Nothing is written here: store_enrolment_token keeps it once the run's checks have passed. Never printed.

    A run that makes the fleet hosted, a first run or a switch from an API on this VM, needs one: only a fleet
    already saved as hosted has a token from an earlier run."""
    if not args.api_enrolment_token_file:
        require(saved.get("apiUrl"),
                "The first run with --api-url, and a switch to it, need the fleet's enrolment token: pass "
                "--api-enrolment-token-file FILE (an administrator of the hosted API issues it for this fleet).")
        return None
    token = password_file(args.api_enrolment_token_file, "--api-enrolment-token-file", "enrolment token")
    require(re.fullmatch(runtime.ENROLMENT_TOKEN_TEXT, token),
            f"The enrolment token file {args.api_enrolment_token_file} must hold the fleet's enrolment token "
            "(senr1.…) that the hosted API's administrator issued.")
    return token


def store_enrolment_token(token):
    """Keep a token given on this run in the fleet's secrets directory, where the fleet installer takes it from.
    An earlier token stays until a new one replaces it: Stargate reads it only to enrol an officer that has no
    enrolment yet, so an expired one never troubles officers already enrolled."""
    destination = INPUTS / "fleet.secrets" / runtime.ENROLMENT_TOKEN
    destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(destination.parent, 0o700)
    write_private(destination, token + "\n")


class NoRedirect(HTTPRedirectHandler):
    """Refuse a redirect rather than follow it: urllib would send the enrolment token on to the new host."""

    def redirect_request(self, request, _fp, code, _message, headers, _new_url):
        target = urlsplit(headers.get("Location", "")).hostname or "an unnamed host"
        raise SetupError(f"The hosted Subspace API at {urlsplit(request.full_url).scheme}://"
                         f"{urlsplit(request.full_url).netloc} answered with a redirect ({code}) to {target}. Setup "
                         "does not follow it with the enrolment token: pass the API's own origin with --api-url.")


def hosted_checks(answers, token):
    """Checks of a hosted API on every run: that it answers (GET /health), and that a token given on this run is
    valid, which means its fleet exists. An enrolment request with an empty body is refused for its body (400)
    only once the token is accepted, and uses nothing. No redirect is followed."""
    origin = answers["apiUrl"]
    opener = build_opener(NoRedirect)
    try:
        with opener.open(Request(f"{origin}/health", headers={"Accept": "application/json"}), timeout=15) as reply:
            healthy = json.load(reply).get("status") == "ok"
    except (HTTPError, URLError, OSError, ValueError):
        healthy = False
    require(healthy, f"The hosted Subspace API at {origin} does not answer GET /health. Check the URL and that this "
            "VM reaches it (outbound HTTPS, and any private network or access proxy).")
    say(f"hosted API: {origin} answers")
    if not token:
        say("hosted API: no enrolment token given on this run, so none is checked. Officers already enrolled keep "
            "working without one; a new officer, or one whose key is replaced, needs a fresh token "
            "(--api-enrolment-token-file).")
        return
    request = Request(f"{origin}/api/v1/agents/enrol", data=b"{}", method="POST", headers={
        "Authorization": f"Bearer {token}", "Content-Type": "application/json", "Accept": "application/json"})
    try:
        with opener.open(request, timeout=15) as reply:
            status, detail = reply.status, ""
    except HTTPError as error:
        status = error.code
        try:
            detail = str(json.load(error).get("detail", ""))[:200]
        except (ValueError, AttributeError):
            detail = ""
    except (URLError, OSError):
        raise SetupError(f"The hosted Subspace API at {origin} did not answer the enrolment check.") from None
    require(status == 400, f"The hosted Subspace API at {origin} refused the enrolment token ({status}"
            + (f": {detail}" if detail else "") + "). Ask its administrator for a new one for this fleet "
            "(POST /api/v1/fleets/<fleet>/enrolment-tokens).")
    say("hosted API: the enrolment token is valid, so its fleet exists (checked without using it)")


def stargate_agents(ref):
    """Whether Stargate at ref has the subspaceAgents section a hosted API needs: fetch that one commit, without
    history, and look for runtime.SUBSPACE_AGENTS_FILE in it."""
    with tempfile.TemporaryDirectory(prefix="subspace-setup-stargate-") as work:
        try:
            run("git", "init", "--quiet", "--bare", work, capture=True, timeout=30)
            run("git", "-C", work, "fetch", "--quiet", "--depth", "1", STARGATE_REPO, ref, capture=True, timeout=120)
        except SetupError:
            raise SetupError(f"Could not fetch Stargate {ref[:12]} from {STARGATE_REPO} to check it. "
                             + OUTBOUND_HELP) from None
        return run("git", "-C", work, "cat-file", "-e", f"FETCH_HEAD:{runtime.SUBSPACE_AGENTS_FILE}",
                   check=False) == 0


def external_database(answers):
    """Whether the API on this VM uses an existing PostgreSQL (--database-url-file) instead of its own."""
    return bool(answers.get("api") and answers.get("databaseUrlFile"))


def database_url(path):
    """The existing PostgreSQL's URL, read from a root-only file. Messages name the file, never what it holds."""
    require(isinstance(path, str) and Path(path).is_absolute(),
            "--database-url-file needs an absolute path to a file on this VM.")
    try:
        info = os.lstat(path)
    except OSError:
        raise SetupError(f"Cannot read the database URL file {path}. Create it as root with mode 0600.") from None
    require(stat.S_ISREG(info.st_mode), f"The database URL file {path} must be a regular file, not a link.")
    require(info.st_uid == SECRET_OWNER and not info.st_mode & 0o077,
            f"The database URL file {path} must be owned by root and private to it: chmod 600 {path}.")
    require(info.st_size <= DATABASE_URL_MAX, f"The database URL file {path} is too large for one URL.")
    try:
        url = Path(path).read_text().strip()
    except (OSError, UnicodeDecodeError):
        raise SetupError(f"Cannot read the database URL file {path} as text.") from None
    require(DATABASE_URL_TEXT.fullmatch(url),
            f"The database URL file {path} must hold one postgresql:// URL on a single line. Percent-encode "
            "spaces, quotes and other special characters in its user name and password.")
    try:
        parts = urlsplit(url)
        hosts = [parts.hostname] if parts.hostname else parse_qs(parts.query).get("host", [])
        parts.port  # noqa: B018 -- raises ValueError for an invalid port
    except ValueError:
        raise SetupError(f"The database URL in {path} is not a valid URL.") from None
    require(parts.scheme in ("postgres", "postgresql"), f"The database URL in {path} must start with postgresql://.")
    require(hosts and not {host.lower() for host in hosts} & {"localhost", "127.0.0.1", "::1"},
            f"The database URL in {path} must name the PostgreSQL server's host. Loopback is the API container "
            "itself, not this VM.")
    require(parts.path.strip("/") or parse_qs(parts.query).get("dbname"),
            f"The database URL in {path} must name the database, as in "
            "postgresql://subspace:password@db.example.com:5432/subspace?sslmode=verify-full.")
    require(sslmode(url) in ("", "disable", "allow", "prefer", "require", "verify-ca", "verify-full"),
            f"The database URL in {path} has an sslmode the API does not know: use verify-full.")
    return url


def sslmode(url):
    return (parse_qs(urlsplit(url).query).get("sslmode") or [""])[-1]


def database_tls(url):
    """What the API's PostgreSQL driver makes of the URL's sslmode, in words that do not repeat the URL.

    The driver (@effect/sql-pg) verifies the certificate and host name for require, verify-ca and verify-full
    alike, tries TLS first for prefer and allow, and uses none without an sslmode.
    """
    mode = sslmode(url)
    if mode in ("require", "verify-ca", "verify-full"):
        return f"over TLS, checking the server's certificate and name (sslmode={mode})"
    if mode in ("prefer", "allow"):
        return f"over TLS when the server offers it, else unencrypted (sslmode={mode})"
    return "unencrypted (sslmode=disable)" if mode == "disable" else "unencrypted (the URL has no sslmode)"


def database_ca(path):
    require(Path(path).is_absolute() and Path(path).is_file(),
            "--database-ca-file needs an absolute path to a PEM file on this VM.")
    try:
        text = Path(path).read_text()
    except (OSError, UnicodeDecodeError):
        raise SetupError(f"Cannot read the database CA file {path}.") from None
    require("-----BEGIN CERTIFICATE-----" in text and "PRIVATE KEY" not in text,
            f"The database CA file {path} must hold the server's CA certificate (PEM) and no private key.")


def database_checks(answers, migrate):
    """The dry run's read-only checks of an existing PostgreSQL: that the API's URL connects, which of this API's
    migrations it lacks, and whether the admin setup acts as is already an administrator there. Stops, after
    saying everything it found, when the real run would stop."""
    path = answers["databaseUrlFile"]
    url = database_url(path)
    folder = PAYLOAD / "subspace-api" / "drizzle"
    local = sorted(entry.name for entry in folder.iterdir() if (entry / "migration.sql").is_file()) \
        if folder.is_dir() else []
    lines, problems = [], []
    try:
        connection = postgres_check.Connection(url, answers.get("databaseCaFile"))
        try:
            lines.append(f"database check: connected read-only to the existing PostgreSQL named in {path}, "
                         + ("over TLS" if connection.tls else "unencrypted"))
            applied = postgres_check.applied_migrations(connection)
            standing = postgres_check.administrator(connection, answers["organisation"], answers["adminEmail"]) \
                if applied else "empty"
        finally:
            connection.close()
    except postgres_check.DatabaseError as error:
        raise SetupError(f"The dry run could not check the existing PostgreSQL named in {path}: {error}. The real run "
                         "would stop there too: check the URL, the server's firewall and the role's privileges.") \
            from None
    pending = [name for name in local if name not in applied]
    unknown = applied - set(local)
    if not pending:
        lines.append(f"database check: it has all {len(local)} of this API's migrations")
    elif migrate:
        lines.append(f"database check: it lacks {len(pending)} of this API's {len(local)} migrations, which the real "
                     "run applies (--migrate)")
    else:
        problems.append(f"the database lacks {len(pending)} of this API's {len(local)} migrations: back it up and "
                        "add --migrate")
    if unknown:
        lines.append(f"database check: it has {len(unknown)} migrations this API does not know; it may be newer")
    who = f"{answers['adminEmail']} in the development organisation {answers['organisation']}"
    if standing == "empty":
        lines.append("database check: it has no organisation yet, so the first sign-in creates one with "
                     f"{answers['adminEmail']} as its administrator")
    elif standing == "admin":
        lines.append(f"database check: {who} is an administrator")
    elif standing == "missing":
        problems.append(f"{who} is not in the database, and setup adds no organisation or person to an existing "
                        "one; pass the --organisation and --admin-email of an administrator who is there")
    else:
        problems.append(f"{who} is not an administrator, and only one may register fleets; pass an administrator's "
                        "--admin-email")
    for line in lines:
        say(line)
    require(not problems, "The real run would stop: " + "; ".join(problems) + ".")


def external_plan(answers, migrate):
    """What setup does with an existing PostgreSQL, for the dry run and the real one. Never prints the URL."""
    url = database_url(answers["databaseUrlFile"])
    lines = [
        f"the API runs on this VM against the existing PostgreSQL named in {answers['databaseUrlFile']}, "
        f"{database_tls(url)}; this VM runs no PostgreSQL container and keeps no database volume",
        "migrations: " + ("applied before the API starts (--migrate)" if migrate else
                          "checked before the API starts; setup stops if any are missing, and applies them only "
                          "with --migrate"),
        f"API sign-in: setup acts as {answers['adminEmail']}, who must already be an administrator of the development "
        f"organisation {answers['organisation']} there; it adds no organisation or person unless the database "
        "has none yet",
    ]
    if answers.get("databaseCaFile"):
        lines.append(f"the API trusts the CA in {answers['databaseCaFile']} for that server")
    if sslmode(url) not in ("require", "verify-ca", "verify-full"):
        lines.append("WARNING: the connection can be unencrypted; add sslmode=verify-full to the URL")
    return lines


def api_up(answers, secret_store, migrate=False):
    source = PAYLOAD / "subspace-api"
    require((source / "compose.yaml").is_file(), "The payload has no subspace-api/compose.yaml.")
    API_DIR.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    staging = API_DIR.with_name("api.new")
    shutil.rmtree(staging, ignore_errors=True)
    shutil.copytree(source, staging, ignore=shutil.ignore_patterns("node_modules", ".data", ".env", "__pycache__"))
    shutil.rmtree(API_DIR, ignore_errors=True)
    staging.rename(API_DIR)
    slug = answers["slug"]
    external = database_url(answers["databaseUrlFile"]) if external_database(answers) else None
    environment = {
        # An existing PostgreSQL's URL goes only into this root-only file, single-quoted so Compose takes it
        # literally; the API's own PostgreSQL gets a generated password instead.
        **({"DATABASE_URL": f"'{external}'"} if external else {"POSTGRES_PASSWORD": secret_store.password("postgres")}),
        "API_PORT": API_PORT,
        "DEV_CHANNELS": f"#{slug}-starbase",
        "DEV_IRC_HOST": irc_endpoint(answers)[0],
        "DEV_IRC_PORT": irc_endpoint(answers)[1],
    }
    previous = INPUTS / "api.env"
    kept = {}
    offered = False
    if previous.exists():
        # Keep what the operator set, such as WorkOS or Entra and turning development sign-in off.
        for line in previous.read_text().splitlines():
            offered = offered or line == AGENT_OFFERED
            key, _, value = line.partition("=")
            if key and not key.startswith("#") and key not in environment:
                kept[key] = value
    if not external:
        # The API's own PostgreSQL: never a URL an earlier run, or anyone, left in api.env, which Compose would
        # prefer to the local database and the image would migrate on start.
        kept.pop("DATABASE_URL", None)
    # The agent listener answers for the development sign-in organisation, which the fleet's agents are
    # registered in. Setup offers it once: after that, an operator who sets AGENT_PORT= or deletes it keeps
    # it off, and setup never turns it back on. The API refuses the listener in production.
    agent = {} if offered else {"AGENT_PORT": AGENT_API_PORT, "AGENT_ORGANISATION": f"dev:{answers['organisation']}"}
    merged = {**environment, **DEV_AUTH, **agent, **kept}
    if offered and "AGENT_PORT" not in kept:
        merged["AGENT_PORT"] = ""
    if re.fullmatch(r"dev:[a-z0-9][a-z0-9-]{0,62}", merged.get("AGENT_ORGANISATION", "")):
        # The installer's own form follows the fleet's development sign-in organisation.
        merged["AGENT_ORGANISATION"] = f"dev:{answers['organisation']}"
    providers = [value.strip() for value in merged.get("AUTH_PROVIDERS", "").split(",")]
    dev_sign_in = merged.get("ENABLE_DEV_AUTH") == "true" and "dev" in providers
    if not dev_sign_in and merged.get("AGENT_ORGANISATION", "").startswith("dev:"):
        # The listener answers for development sign-in's organisation and trusts the network as it does:
        # an operator who turns development sign-in off turns it off too.
        merged["AGENT_PORT"] = ""
        merged.pop("AGENT_ORGANISATION", None)
    if merged.get("NODE_ENV") == "production":
        merged["AGENT_PORT"] = ""
    listener = str(merged.get("AGENT_PORT", "")) == str(AGENT_API_PORT)
    write_private(INPUTS / "api.env", AGENT_OFFERED + "\n" + "".join(
        f"{key}={value}\n" for key, value in merged.items()))
    if external:
        ca = ""
        if answers.get("databaseCaFile"):
            # A CA certificate is public: the API's own user reads this copy through a read-only mount. It lives
            # beside API_DIR, which every run replaces, and is rewritten in place so the mount keeps its file.
            ca = hashlib.sha256(Path(answers["databaseCaFile"]).read_bytes()).hexdigest()
            shutil.copyfile(answers["databaseCaFile"], database_ca_copy())
            os.chmod(database_ca_copy(), 0o644)
        write_private(INPUTS / "api.override.yaml", external_override(listener, ca))
        external_schema(answers, migrate)
        say("building and starting the Subspace API (loopback only) against the existing PostgreSQL")
        compose("up", "--detach", "--wait", "--wait-timeout", "300", "--build")
        return dev_auth_enabled()
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
        *([f'      - "{LOOPBACK}:{AGENT_API_PORT}:{AGENT_API_PORT}"'] if listener else []),
        "",
    ]))
    say("building and starting the Subspace API and PostgreSQL (loopback only)")
    compose("up", "--detach", "--wait", "--wait-timeout", "300", "--build")
    return dev_auth_enabled()


def database_ca_copy():
    return API_DIR.with_name("database-ca.pem")


def external_override(listener, ca):
    """The Compose override for an API that uses an existing PostgreSQL (--database-url-file).

    The postgres service moves to a profile nothing activates, so no container or volume is created for it, and
    the API no longer waits for it. The API starts without migrating (the image's own command migrates first):
    setup checks and applies migrations as a separate step, and only with --migrate (external_schema). The API and
    its agent listener are published on loopback exactly as with the API's own PostgreSQL. ca is the SHA-256 of
    the CA certificate the API trusts for the server, if any: a changed certificate changes the environment, so
    Compose recreates the API, which reads NODE_EXTRA_CA_CERTS only when it starts.
    """
    return "\n".join([
        "services:",
        "  postgres:",
        '    profiles: ["subspace-local-database"]',
        "  api:",
        "    restart: unless-stopped",
        "    depends_on: !reset {}",
        '    command: ["bun", "src/main.ts"]',
        "    ports: !override",
        f'      - "{LOOPBACK}:{API_PORT}:3000"',
        *([f'      - "{LOOPBACK}:{AGENT_API_PORT}:{AGENT_API_PORT}"'] if listener else []),
        *(["    environment:", f"      NODE_EXTRA_CA_CERTS: {DATABASE_CA}", f"      SUBSPACE_DATABASE_CA_SHA256: {ca}",
           "    volumes:", f'      - "{database_ca_copy()}:{DATABASE_CA}:ro"'] if ca else []),
        "",
    ])


def external_schema(answers, migrate):
    """Compare an existing PostgreSQL's migration history with this API's, and migrate it only with --migrate.

    Both run as one-off API containers that publish no port. The check (scripts/migrate.ts --check) changes
    nothing; it exits MIGRATIONS_PENDING when the database lacks migrations, as an empty one does.
    """
    path = answers["databaseUrlFile"]
    compose("build", "api")
    one_off = ("run", "--rm", "--no-deps", "-T", "api", "bun", "scripts/migrate.ts")
    status = compose(*one_off, "--check", check=False)
    if status == MIGRATIONS_PENDING:
        require(migrate, "The existing PostgreSQL lacks database migrations this API needs (see above). Setup "
                "never migrates a database it did not create unless asked: back it up, give the role in "
                f"{path} the privileges the installer README lists ('An existing PostgreSQL'), and run setup "
                "again with --migrate.")
        say("applying the API's database migrations to the existing PostgreSQL (--migrate)")
        if compose(*one_off, check=False):
            raise SetupError("The migrations failed (see above); a failed schema migration is rolled back. The "
                             f"role in {path} needs CREATE on the database and the right to create and alter "
                             "the API's tables (installer README, 'An existing PostgreSQL').")
    elif status:
        raise SetupError("Could not read the existing PostgreSQL's migration history (see above). Check that this "
                         f"VM reaches the server, the URL in {path} (its sslmode, and --database-ca-file for a "
                         "private CA) and its role's privileges.")
    elif migrate:
        say("the existing PostgreSQL already has every migration this API needs; nothing to apply")


def dev_auth_enabled():
    """Whether the API still offers development sign-in, which setup uses to act as the admin."""
    environment = {}
    for line in (INPUTS / "api.env").read_text().splitlines():
        key, _, value = line.partition("=")
        environment[key] = value
    providers = [value.strip() for value in environment.get("AUTH_PROVIDERS", "").split(",")]
    return environment.get("ENABLE_DEV_AUTH") == "true" and "dev" in providers


def agent_listener_enabled():
    """Whether api.env turns the API's agent listener on, on the port the fleet's agents are given."""
    environment = dict(line.partition("=")[::2] for line in (INPUTS / "api.env").read_text().splitlines())
    return environment.get("AGENT_PORT") == str(AGENT_API_PORT)


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


def api_token(answers, email=None):
    # In a database setup did not create, sign in only as someone already there (subspace-api's auth:dev), so a
    # mistyped --organisation or --admin-email never adds an organisation or a person. An empty one is exempt.
    existing = ["-e", "DEV_TOKEN_EXISTING_ONLY=true"] if external_database(answers) else []
    try:
        token = compose("exec", "-T", *existing, "api", "bun", "run", "--silent", "auth:dev",
                        email or answers["adminEmail"], answers["organisation"], capture=True).splitlines()[-1].strip()
    except SetupError:
        if not existing:
            raise
        raise SetupError(f"Could not sign {email or answers['adminEmail']} in to the development organisation "
                         f"{answers['organisation']} in the existing PostgreSQL (see above). Setup adds no organisation "
                         "or person to a database it did not create: pass the --organisation and --admin-email of an "
                         "administrator who is already there.") from None
    require(JWT.fullmatch(token), "The API did not return a development token.")
    return token


def existing_administrator(answers, token):
    """Stop unless the admin's token is an administrator's: in an existing database an ordinary member could
    create the fleet's initiative before registration refused them, so this comes before any write."""
    require(api_call(token, "GET", "/me").get("isAdmin"),
            f"{answers['adminEmail']} is not an administrator of the development organisation "
            f"{answers['organisation']} in the existing PostgreSQL, and only one may register fleets. "
            "Pass an administrator's email with --admin-email.")


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


def fleet_records_for(config, private):
    """The fleet's validated config and its API records, as the installer will validate them.

    Agent dispatch is left out: its token is minted for the rear admiral's agent record, made from these records.
    """
    config = {key: value for key, value in config.items() if key != "agentDispatch"}
    private = {key: value for key, value in private.items() if key != "rearAdmiralApiToken"}
    try:
        cfg = runtime.validate(config, private, check_files=False)
        return cfg, fleet_records.check(cfg["slug"], fleet_records.roster(cfg))
    except (runtime.Invalid, fleet_records.Refused) as error:
        raise SetupError(str(error)) from None


def register_own(token, config, private):
    """Register this fleet, its standard channels, ships and officers with the API on this VM; return the records.

    The fleet installer's 'fleetctl register' sends the same records again afterwards and finds nothing to change.
    """
    cfg, records = fleet_records_for(config, private)
    try:
        fleet_records.register(lambda method, path, body: api_call(token, method, path, body),
                               fleet_records.fleet_fields(cfg), records, say=say)
    except fleet_records.Refused as error:
        raise SetupError(str(error)) from None
    return records


def rear_admiral_token(answers, records):
    """A development token for the rear admiral's own agent identity, which agent dispatch acts as."""
    # A development token expires after eight hours, so dispatch stops until setup runs again or the API moves
    # to WorkOS or Entra with a long-lived token (installer README, 'Agent dispatch'). Keep the current one
    # while it has more than half its life left, so a rerun does not restart the gateway for a new token.
    current = read_json(INPUTS / "fleet.secrets.json", {}).get("rearAdmiralApiToken", "")
    if token_seconds_left(current) > 4 * 3600:
        return current
    return api_token(answers, records["rearAdmiral"]["serviceTokenClientId"])


def token_seconds_left(token, now=None):
    """Seconds until a bearer JWT's exp claim, read without verifying it; 0 when unknown or expired."""
    try:
        payload = token.split(".")[1]
        claims = json.loads(base64.urlsafe_b64decode(payload + "=" * (-len(payload) % 4)))
        return max(0, int(claims["exp"]) - int(time.time() if now is None else now))
    except (AttributeError, IndexError, KeyError, TypeError, ValueError):
        return 0


def seed_demo(answers):
    say("seeding fictional demo data")
    compose("exec", "-T", "-e", f"SEED_EMAIL={answers['adminEmail']}", "-e", f"SEED_ORGANISATION={answers['organisation']}",
            "-e", f"SEED_IRC_HOST={irc_endpoint(answers)[0]}", "-e", f"SEED_IRC_PLAIN_PORT={IRC_PLAIN}",
            "-e", f"SEED_IRC_TLS_PORT={IRC_TLS if answers['irc'] == 'local' else answers['ircPort']}",
            "api", "bun", "run", "--silent", "seed:demo")


def fleet_initiative(token, name, slug, known=None, exclude=()):
    """Create, or find, a fleet's initiative.

    The initiative is found by its saved ID, else by name among initiatives that belong to no fleet or to
    this one (slug). Setup lists no channels on it: an initiative's channels are its own, beyond its fleet's
    standard channels, which the fleet and its ships record (fleet_records) and the desktop joins.
    """
    mine = {row["id"]: row for row in api_call(token, "GET", "/me/initiatives").get("items", [])}
    found = known if known in mine else None
    cursor = ""
    while not found:
        page = api_call(token, "GET", "/initiatives?limit=100" + (f"&cursor={quote(cursor)}" if cursor else ""))
        found = next((row["id"] for row in page.get("items", []) if row.get("name") == name
                      and row["id"] not in exclude and (row.get("fleet") is None or (row.get("fleet") or {}).get("slug") == slug)), None)
        cursor = page.get("nextCursor") or ""
        if not cursor:
            break
    if found:
        if found not in mine:
            say(f"found an initiative named {name} that you are not a member of; add yourself in the desktop")
        return found
    me = api_call(token, "GET", "/me")
    return api_call(token, "POST", "/initiatives", {
        "name": name, "ownerId": me["userId"], "status": "active",
        "description": f"Work for the {name} fleet. Talk to the rear admiral in {standard_channels.starbase(slug)}.",
        "channelConfiguration": {"memberUserIds": [me["userId"]]},
    })["id"]


def installed_records():
    """This fleet's validated config and API records, from the inputs setup last wrote."""
    config, private = read_json(INPUTS / "fleet.json"), read_json(INPUTS / "fleet.secrets.json")
    require(config and private, "Set the fleet up first.")
    return fleet_records_for(config, private)


def fleet_registration(answers):
    """The non-secret description of this fleet that another fleet's API needs: the fleet, its standard
    channels, a ship per captain and the officers' agent records (fleet_records).

    fleetApiUrl is the address --fleet-api-url gave, or null while the Fleet API is off. Otherwise it is left
    out, so the API keeps what it has: the SSH tunnel's loopback end on the desktop is the first fleet's, since
    every fleet's tunnel uses the same local ports.
    """
    cfg, records = installed_records()
    host, port = irc_endpoint(answers)
    registration = {"slug": answers["slug"], "name": answers["name"], "ircHost": host, "ircPort": port,
                    "ircTls": answers["irc"] != "local", **records}
    if not cfg["stargate"].get("api"):
        registration["fleetApiUrl"] = None
    elif answers.get("fleetApiUrl"):
        registration["fleetApiUrl"] = answers["fleetApiUrl"]
    return registration


def registration_ready():
    """Fail unless this VM's API can register further fleets."""
    answers = read_json(INPUTS / "answers.json", {})
    require(answers.get("api"), "The Subspace API is not set up on this VM.")
    require(dev_auth_enabled(), "This API no longer offers development sign-in, so the installer cannot register "
            "further fleets with it. Register them as an administrator instead (INSTALL.md, 'One fleet or several').")
    return answers


def register_fleet(registration):
    """Register another fleet, its standard channels, ships and officers, and an initiative for it, with the
    API on this VM."""
    answers = registration_ready()
    require(isinstance(registration, dict), "The fleet registration must be a JSON object.")
    slug, name = registration.get("slug"), registration.get("name")
    require(isinstance(slug, str) and SLUG.fullmatch(slug), "The registration has an invalid fleet ID.")
    require(slug != answers["slug"], "That is this VM's own fleet.")
    require(isinstance(name, str) and 1 <= len(name) <= 100 and "\n" not in name, "The registration has an invalid name.")
    known = {"slug", "name", "ircHost", "ircPort", "ircTls", "starbaseChannel", "commandChannel", "rearAdmiral", "captains",
             "fleetApiUrl"}
    require(registration.keys() <= known, "The registration has fields this installer does not know. Set the fleet "
            "up again with the same version of the installer as this VM.")
    records = {key: registration.get(key) for key in ("starbaseChannel", "commandChannel", "rearAdmiral", "captains")}
    try:
        fleet_records.check(slug, records)
    except fleet_records.Refused as error:
        raise SetupError(f"{error} Set the fleet up again with this version of the installer.") from None
    require(registration.get("ircHost") not in ("127.0.0.1", "localhost", "::1"),
            "A further fleet needs a shared IRC server (--irc-server): IRC on its own VM is unreachable from here.")
    port = registration.get("ircPort")
    require(isinstance(registration.get("ircHost"), str) and isinstance(port, int) and 1 <= port <= 65535,
            "The registration has an invalid IRC server.")
    fields = {key: registration[key] for key in ("slug", "name", "ircHost", "ircPort")}
    fields["ircTls"] = bool(registration.get("ircTls"))
    if registration.get("fleetApiUrl") is not None:
        try:
            fields["fleetApiUrl"] = fleet_records.fleet_api_url(registration["fleetApiUrl"])
        except fleet_records.Refused as error:
            raise SetupError(f"The registration has an invalid fleetApiUrl. {error}") from None
        require(urlsplit(fields["fleetApiUrl"]).hostname not in ("localhost", LOOPBACK, "::1"),
                "A further fleet's Fleet API address cannot be on the desktop's loopback, which the first fleet's "
                "SSH tunnel uses. Give that fleet an HTTPS address with --fleet-api-url, or 'tunnel' to leave it out.")
    elif "fleetApiUrl" in registration:
        fields["fleetApiUrl"] = None
    token = api_token(answers)
    try:
        fleet = fleet_records.register(lambda method, path, body: api_call(token, method, path, body), fields, records,
                                       say=say)
    except fleet_records.Refused as error:
        raise SetupError(str(error)) from None
    registered = answers.setdefault("registeredFleets", {}).setdefault(slug, {})
    initiative = fleet_initiative(token, name, slug, registered.get("initiativeId"), exclude=[answers.get("initiativeId")])
    assigned = api_call(token, "GET", f"/initiatives/{initiative}").get("fleetId")
    require(assigned in (None, fleet["id"]), f"Initiative {initiative} already belongs to another fleet.")
    if assigned != fleet["id"]:
        api_call(token, "PUT", f"/initiatives/{initiative}/fleet", {"fleetId": fleet["id"]})
    registered.pop("channels", None)
    registered.update(initiativeId=initiative)
    write_json(INPUTS / "answers.json", answers)
    say(f"registered fleet {name} ({slug}), its ships and officers, with initiative {initiative}")


# Summary ---------------------------------------------------------------------

def fleet_channels(answers, members):
    """The fleet's standard channels: Starbase, the command channel, then one ship channel per captain."""
    return standard_channels.standard(answers["slug"], [person["id"] for person in members[1:]])


def installed_irc_auth():
    """How the installed fleet's officers sign in to a remote IRC server, from its own fleet.json, or None."""
    try:
        irc = read_json(FLEET_ETC / "fleet.json", {}).get("irc") or {}
    except (OSError, ValueError, AttributeError):
        return None
    auth = irc.get("auth") if irc.get("mode") == "remote" else None
    return auth if auth in ("certificate", "password") else None


def installed_bundle(agent_id):
    """The certificate bundle the installed fleet signs an officer in with, or None."""
    try:
        text = (FLEET_ETC / "irc-certs" / f"{agent_id}.pem").read_text()
    except (OSError, UnicodeDecodeError):
        return None
    return text


def officer_certificates(members, generate):
    """Each officer's IRC certificate bundle in the fleet's secrets directory, where the fleet installer takes it
    from, as {account: (path, SHA-256 fingerprint)}. A missing one is the installed fleet's, so a fingerprint
    already bound stays valid, else a new one, made once. A dry run writes nothing: it reads the installed
    bundle, or reports None for one the real run will make."""
    result = {}
    for person in members:
        path = INPUTS / "fleet.secrets" / person["id"] / runtime.IRC_CLIENT_PEM
        installed = None if path.exists() else installed_bundle(person["id"])
        if not path.exists() and generate:
            path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            os.chmod(path.parent.parent, 0o700)
            write_private(path, installed or runtime.new_irc_bundle(person["account"]))
        text = path.read_text() if path.exists() else installed
        fingerprint = runtime.irc_fingerprint(text, str(path)) if text else None
        result[person["account"]] = (path, fingerprint)
    return result


def certificate_lines(answers, certificates):
    """What the IRC server's operator runs for each officer: register its account with no password, then bind its
    certificate. Fingerprints only, never key material."""
    lines = [f"officers sign in to {answers['ircHost']} with certificates (SASL EXTERNAL); an IRC operator with "
             "NickServ's accreg capability registers each account and binds its certificate (or pass "
             "--irc-operator and --irc-operator-password-file and setup does it):"]
    for account, (_, fingerprint) in certificates.items():
        if fingerprint:
            lines += [f"  {account}: SHA-256 {fingerprint}", f"    /NS SAREGISTER {account}",
                      f"    /NS CERT ADD {account} {fingerprint}"]
        else:
            lines.append(f"  {account}: the real run generates its certificate and prints these lines for it")
    return lines


def password_file(path, option, label):
    """A password from a root-only file, for this run only. Messages name the file, never what it holds."""
    require(Path(path).is_absolute(), f"{option} needs an absolute path to a file on this VM.")
    try:
        info = os.lstat(path)
        require(stat.S_ISREG(info.st_mode) and info.st_uid == SECRET_OWNER and not info.st_mode & 0o077,
                f"The {label} file {path} must be a regular file owned by root with mode 0600: chmod 600 {path}.")
        password = Path(path).read_text().strip()
    except (OSError, UnicodeDecodeError):
        raise SetupError(f"Cannot read the {label} file {path}.") from None
    require(password and len(password) <= 512 and not re.search(r"[\s\x00]", password),
            f"The {label} file {path} must hold one password on one line.")
    return password


def irc_operator(args):
    """The IRC operator's name and password for this run, and the account its connection signs in to first, or
    None. Passwords are read from root-only files and are never printed or saved."""
    if not args.irc_operator:
        return None
    require(re.fullmatch(r"[A-Za-z0-9_.\[\]{}|^`-]{1,64}", args.irc_operator), "Invalid --irc-operator name.")
    password = password_file(args.irc_operator_password_file, "--irc-operator-password-file", "IRC operator password")
    account = None
    if args.irc_operator_account:
        require(re.fullmatch(r"[A-Za-z0-9_.\[\]{}|^`-]{1,64}", args.irc_operator_account),
                "Invalid --irc-operator-account name.")
        account = (args.irc_operator_account, password_file(
            args.irc_operator_account_password_file, "--irc-operator-account-password-file", "IRC account password"))
    return args.irc_operator, password, account


def check_certificates(answers, members, certificates, operator, register):
    """With the operator's credential: check it, register the officers' accounts and bind their certificates
    (register), and check that each certificate signs in as its account. A dry run only checks."""
    host, port, ca = answers["ircHost"], answers["ircPort"], answers.get("ircCa")
    name, password, signed_in_as = operator

    def unbound():
        """The officers whose certificate does not sign in to their account yet."""
        missing = {}
        for account, (path, fingerprint) in certificates.items():
            if fingerprint:
                signed_in = irc_accounts.check_login(host, port, ca, str(path))
                say(f"{account}: its certificate {'signs in' if signed_in == account else 'does not sign in yet'}")
                if signed_in != account:
                    missing[account] = fingerprint
        return missing
    try:
        missing = unbound()
        # Only accounts whose certificate does not sign in yet are registered; a rerun registers nothing.
        replies = irc_accounts.register(host, port, ca, name, password, missing if register else {}, signed_in_as)
        if not register:
            say(f"the IRC operator {name} is accepted on {host}")
        for account, lines in replies.items():
            say(f"NickServ for {account}: " + " / ".join(lines))
        failed = list(unbound()) if register and missing else []
    except irc_accounts.IrcError as error:
        raise SetupError(str(error)) from None
    require(not register or not failed, f"{', '.join(failed)} cannot sign in to {host} with {'its' if len(failed) == 1 else 'their'} "
            "certificate after registration (see NickServ's replies above). Check that the operator's class has "
            "NickServ's accreg capability, or bind the fingerprints by hand, then run setup again.")


def irc_bundle(answers, members, secret_store):
    """What a shared IRC server needs for this fleet: its SASL accounts and channels."""
    channels = fleet_channels(answers, members)
    topics = [f"{answers['name']} Starbase: admirals and the rear admiral",
              f"{answers['name']} fleet command: the rear admiral and the captains, and agent dispatch"] + [
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
        **({"SUBSPACE_HOSTED_API": answers["apiUrl"]} if answers.get("apiUrl") else {}),
    }
    write_private(INPUTS / "summary.env", "".join(f"{key}={value}\n" for key, value in values.items()))
    return values


# Main ------------------------------------------------------------------------

def setup(args):
    global INPUTS
    INPUTS = Path(args.inputs)
    require(args.dry_run or os.geteuid() == 0, "Run subspace-setup as root (sudo).")
    if args.dry_run:
        # A dry run works on a private copy of the inputs, so the answers, generated secrets and fleet files it
        # writes, a database URL file it is given included, never become the next real run's.
        original = INPUTS
        with tempfile.TemporaryDirectory(prefix="subspace-setup-dry-run-") as scratch:
            INPUTS = Path(scratch) / "inputs"
            if original.is_dir():
                shutil.copytree(original, INPUTS, symlinks=True)
            try:
                return configure(args)
            finally:
                INPUTS = original
    return configure(args)


def configure(args):
    INPUTS.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(INPUTS, 0o700)
    saved = read_json(INPUTS / "answers.json", {})
    answers = gather(args, saved)
    check_outbound(answers)
    rear, captains = check_answers(answers)
    if saved.get("slug") and saved["slug"] != answers["slug"]:
        raise SetupError("The fleet ID cannot change on an installed VM. Reinstall on a fresh VM to rename it.")
    # The API has run here with its own PostgreSQL (setup writes api.env only then): its tracker is in that
    # container's volume, and setup does not move it.
    require(not (external_database(answers) and not saved.get("databaseUrlFile") and (INPUTS / "api.env").is_file()),
            "This VM's API already keeps its tracker in its own PostgreSQL, which setup does not move. To use an "
            "existing PostgreSQL, restore a dump of that database there and set up a fresh VM with "
            "--database-url-file (installer README, 'An existing PostgreSQL').")
    if answers.get("stargateRef"):
        stargate_tag, stargate_ref = "pinned", answers["stargateRef"]
    else:
        stargate_tag, stargate_ref = latest_stargate()
    say(f"installing Stargate {stargate_tag} ({stargate_ref[:12]})")
    enrolment = None
    if answers.get("apiUrl"):
        # Every check of the hosted API comes before setup saves anything, so a refused run, a switch from an
        # API on this VM included, leaves the fleet's answers and secrets exactly as they were.
        require(stargate_agents(stargate_ref),
                f"A hosted API (--api-url) needs a Stargate with its subspaceAgents section, from commit "
                f"{runtime.SUBSPACE_AGENTS_COMMIT[:12]} on, and Stargate {stargate_tag} ({stargate_ref[:12]}) has not "
                "got it: pass --stargate-ref with that commit or a later one.")
        say(f"Stargate {stargate_ref[:12]} has subspaceAgents ({runtime.SUBSPACE_AGENTS_FILE})")
        enrolment = enrolment_token(args, saved)
        hosted_checks(answers, enrolment)
    save_answers(answers)
    if enrolment:
        store_enrolment_token(enrolment)
    # A dry run neither reads nor writes Key Vault: a value the copied store lacks is made up for validation.
    secret_store = Secrets(INPUTS, answers["slug"], None if args.dry_run else answers.get("keyVault"))
    members = roster(answers, rear, captains)
    local_irc = answers["irc"] == "local"
    config_path = INPUTS / "fleet.json"
    secrets_path = INPUTS / "fleet.secrets.json"

    dispatch_token = ""

    def inputs(irc_ref, api_block=None, stargate_api=False, token="", history=None):
        config = fleet_config(answers, members, irc_ref, api_block, stargate_api, stargate_ref, history)
        private = fleet_secrets(secret_store, members, answers["admin"], token, local_irc,
                                certificates=answers.get("ircAuth") == "certificate",
                                dispatch_token=dispatch_token if api_block else "")
        if history:
            private["ircHistoryPassword"] = secret_store.password("ergo-history")
        return config, private

    def write_inputs(*args):
        config, private = inputs(*args)
        write_json(config_path, config)
        write_json(secrets_path, private)

    certificates = operator = None
    if answers.get("ircAuth") == "certificate":
        operator = irc_operator(args)
        certificates = officer_certificates(members, generate=not args.dry_run)
        for line in certificate_lines(answers, certificates):
            say(line)
        if operator and args.dry_run:
            check_certificates(answers, members, certificates, operator, register=False)
    elif not local_irc:
        irc_bundle(answers, members, secret_store)
    if external_database(answers):
        for line in external_plan(answers, args.migrate):
            say(line)
    if args.dry_run:
        if external_database(answers):
            database_checks(answers, args.migrate)
        write_inputs(hashlib.sha1(b"dry-run").hexdigest())
        if answers["agentDispatch"]:
            say("agent dispatch needs the API, which the dry run leaves out; the real run checks it")
        installer(config_path, dry_run=True)
        say("dry run complete; nothing on this VM changed")
        return

    if answers["api"]:
        # The API comes first (its token and registration go into the fleet's inputs), so check now that the
        # installer will accept those inputs: with --migrate, a refused run must not have migrated the database.
        preflight(*inputs(hashlib.sha1(b"dry-run").hexdigest()))

    if certificates and operator:
        # Before any officer starts: register the accounts, bind the certificates and check each sign-in.
        check_certificates(answers, members, certificates, operator, register=True)

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
        dev_auth = api_up(answers, secret_store, args.migrate)
        if local_irc and not external_database(answers):
            history = history_database(secret_store)
        elif local_irc:
            say("Ergo keeps IRC history in its own SQLite file on this VM, not in the existing PostgreSQL")
        if dev_auth:
            # The first sign-in to an organisation becomes its administrator, so sign the admin in first.
            token = api_token(answers)
            if external_database(answers):
                existing_administrator(answers, token)
            if answers["demo"]:
                seed_demo(answers)
            initiative = fleet_initiative(token, answers["name"], answers["slug"], answers.get("initiativeId"))
            answers.update(initiativeId=initiative)
            save_answers(answers)
            api_block = {"url": f"http://{LOOPBACK}:{API_PORT}", "initiatives": [initiative]}
            if answers.get("vmHost"):
                api_block["vmHost"] = answers["vmHost"]  # The fleet's record in the API names the VM.
            if agent_listener_enabled():
                # Officers get the API's MCP server at the agent listener, as their own agent records.
                api_block["agentUrl"] = f"http://{LOOPBACK}:{AGENT_API_PORT}"
            # Register the fleet, its channels, ships and officers now: agent dispatch signs in as the rear
            # admiral's agent record. The installer's 'fleetctl register' sends the same records again.
            records = register_own(token, *inputs(irc_ref, api_block, bool(stargate_api), token, history))
            if answers["agentDispatch"]:
                dispatch_token = rear_admiral_token(answers, records)
        else:
            say("the API no longer offers development sign-in, so setup leaves its fleet registration, "
                "initiative and demo data as they are")
            # Without development sign-in, setup writes no API settings, which dispatch needs.
            require(not answers["agentDispatch"],
                    "Agent dispatch needs the API's registration in the fleet's inputs, which setup writes only while "
                    "the API offers development sign-in. Pass --no-agent-dispatch, and configure api, agentDispatch "
                    "and rearAdmiralApiToken for the fleet installer by hand (subspace-installer/README.md, "
                    "'Agent dispatch').")

    write_inputs(irc_ref, api_block, bool(stargate_api), token, history)
    installer(config_path)
    if not answers["api"]:
        api_down()
    if answers["agentDispatch"] or answers["api"]:
        # The installer has created the officers' users and Stargate's gateway user: let the gateway, and only it,
        # reach the API's port, and the officers, and only they, reach the agent listener.
        run(HERE / "lockdown.sh", "isolate", *isolation_args(answers))
    if stargate_api is None:
        stargate_api = confirm_fleet_api(None, installed_stargate_source())
        if stargate_api:
            # Again with the Fleet API: its listener, firewall rule and token, and its address in the fleet's record.
            write_inputs(irc_ref, api_block, True, token, history)
            installer(config_path)
    elif stargate_api:
        confirm_fleet_api(True, installed_stargate_source())

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
    say(f"Starbase is {channels[0]}; fleet command is {channels[1]}; ship channels: {' '.join(channels[2:])}.")
    if api_block and stargate_api:
        address = answers.get("fleetApiUrl") or f"http://{LOOPBACK}:{STARGATE_API_PORT}"
        say(f"the Subspace API records the fleet's Stargate Fleet API at {address}; the desktop app asks for the "
            "fleet's token once.")
    elif answers.get("fleetApiUrl") and not stargate_api:
        say("the installed Stargate has no Fleet API, so the fleet records no Fleet API address.")
    if answers["agentDispatch"]:
        say(f"agent dispatch is on: {DISPATCH_SUMMARY}.")
        if api_block:
            say("WARNING: the rear admiral's development sign-in token lasts eight hours. Run setup again to renew "
                "it, or move the API to WorkOS or Entra and give the rear admiral a long-lived token "
                "(installer README, 'Agent dispatch').")
    else:
        say("agent dispatch is off; 'subspace-setup --agent-dispatch' switches it on.")
    if not local_irc:
        say(remote_irc_next_step(answers, operator))
    say(f"forward these loopback ports to reach it: {values['SUBSPACE_PORTS']}")
    if run(HERE / "providers.sh", "status", check=False):
        say("next: bring the agents online by signing CLIProxyAPI in to a provider account "
            "(INSTALL.md, 'Bring the agents online').")


def remote_irc_next_step(answers, operator):
    """What is left to do on a remote IRC server once setup is done."""
    if answers.get("ircAuth") == "certificate":
        # No password bundle exists in this mode: the officers' accounts are bound to their certificates.
        if operator:
            return (f"officers sign in to {answers['ircHost']} with certificates, which setup registered and checked "
                    "above with --irc-operator.")
        return (f"officers sign in to {answers['ircHost']} with certificates: each can sign in once the IRC server's "
                "operator has run the /NS lines printed above for it (or run setup again with --irc-operator). "
                "'sudo /opt/subspace-fleet/lib/runtime.py irc-fingerprints' prints the fingerprints again.")
    return (f"add this fleet to the shared IRC server: copy {INPUTS / 'irc-bundle.json'} there and run "
            "'sudo subspace-setup irc-server add <file>' (the host installer does this with --irc-server).")


def isolation_args(answers):
    """lockdown.sh options that keep the agents away from operator-only ports and cloud metadata."""
    args = ["--operator", answers["operator"], "--protect-metadata"]
    if answers["api"] or (INPUTS / "api.env").is_file():
        # The API (development sign-in) and PostgreSQL, also through their container addresses,
        # and still after a switch to --no-api in case stopping them failed.
        args += ["--operator-only-port", str(API_PORT)]
        if not answers.get("databaseUrlFile"):
            args += ["--operator-only-port", "5432"]  # The API's own PostgreSQL; an existing one is elsewhere.
        try:
            pwd.getpwnam(GATEWAY_USER)
            gateway = answers.get("agentDispatch")
        except KeyError:
            gateway = False  # Not yet created: setup isolates again once the installer has run.
        if gateway:
            # Agent dispatch: Stargate's gateway polls the API as the rear admiral. Agents never reach this port.
            args += ["--port-user", f"{API_PORT}:{GATEWAY_USER}"]
        # The agent listener: only the operator and the officers' own users (crew run as their captain's),
        # never the fleet's other service users or containers.
        args += ["--operator-only-port", str(AGENT_API_PORT)]
        for user in officer_users():
            args += ["--port-user", f"{AGENT_API_PORT}:{user}"]
    for user in metadata_users(answers):
        args += ["--metadata-user", user]
    return args


def officer_users():
    """The rear admiral's and captains' Linux users that exist, from the fleet's inputs."""
    config = read_json(INPUTS / "fleet.json", {})
    people = [config.get("rearAdmiral") or {}, *(config.get("captains") or [])]
    users = []
    for person in people:
        agent_id = person.get("id") if isinstance(person, dict) else None
        if not (isinstance(agent_id, str) and NAME_ID.fullmatch(agent_id)):
            continue
        try:
            pwd.getpwnam(f"ss-{agent_id}")
        except KeyError:
            continue  # Not yet created: setup isolates again once the installer has run.
        users.append(f"ss-{agent_id}")
    return users


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
        require(read_json(INPUTS / "answers.json", {}).get("ircAuth") != "certificate",
                "This fleet's officers sign in with certificates, so it has no password bundle for a shared server.")
        require(bundle.is_file(), "This fleet uses local IRC, so it has no bundle for a shared server.")
        sys.stdout.write(bundle.read_text())
    elif argv and argv[0] == "fleet-registration":
        answers = read_json(INPUTS / "answers.json", {})
        require(answers.get("slug"), "Set the fleet up first.")
        sys.stdout.write(json.dumps(fleet_registration(answers), indent=2) + "\n")
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
        require(not external_database(answers), "Demo data is fictional and never goes into a database the installer "
                "did not create, such as this API's existing PostgreSQL.")
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
