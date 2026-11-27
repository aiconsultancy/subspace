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
from ipaddress import ip_address
import json
import os
from pathlib import Path
import pwd
import re
import secrets
import shutil
import socket
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
import memory  # noqa: E402
import mockups  # noqa: E402
import postgres_check  # noqa: E402
import private_network  # noqa: E402
import runtime  # noqa: E402
import standard_channels  # noqa: E402
import toolchains  # noqa: E402
INPUTS = Path("/root/fleet")
API_DIR = Path("/opt/subspace/api")
FLEET_APP = Path("/opt/subspace-fleet")
FLEET_ETC = Path("/etc/subspace-fleet")
API_PROJECT = "subspace-api"
API_PORT = runtime.SUBSPACE_API_PORT
API_IRC_MOUNT = "/run/subspace-managed-irc"
API_IRC_HOOK_SETTINGS = {"IRC_AUTH_ORGANISATION", "IRC_AUTH_SERVERS", "IRC_AUTH_HOOK_TOKEN",
                         "IRC_AUTH_HOOK_TOKEN_FILE", "IRC_CREDENTIAL_SECRET", "IRC_CREDENTIAL_SECRET_FILE"}
# The API's agent-only listener: a fleet agent names itself in a header there and needs no token. It serves no
# sign-in or administrator route, so unlike API_PORT the agents' Linux users may reach it.
AGENT_API_PORT = runtime.SUBSPACE_AGENT_API_PORT
SIGNED_LOCAL_API = "# subspace-setup: signed local officer access selected"
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
UUID = re.compile(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}")
VAULT = re.compile(r"[a-zA-Z][a-zA-Z0-9-]{1,22}[a-zA-Z0-9]")
GATEWAY_USER = "ss-fleet-gateway"  # The fleet installer's shared Stargate gateway user (runtime.GATEWAY).
LINUX_USER = re.compile(r"[a-z_][a-z0-9_.-]{0,31}")
# Development sign-in is only the default: an operator who switches the API to WorkOS or Entra keeps it.
DEV_AUTH = {"AUTH_PROVIDERS": "dev", "ENABLE_DEV_AUTH": "true", "NODE_ENV": "development"}
# Stand-ins, shaped as bearer JWTs, for the tokens a real run gets from the API: the installer checks inputs that
# name the API before the API runs (planned_inputs), and the dry run never signs in.
PLANNED_TOKENS = {"apiToken": "planned.administrator.token", "rearAdmiralApiToken": "planned.rear-admiral.token"}
PLANNED_ENROLMENT_TOKEN = "senr1." + "A" * 64
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
# Write new crew defaults to fleet.json. Older fleets that omitted the model keep the installer's legacy fallback.
CREW_MODEL_DEFAULTS = {"claude": "claude-opus-5-5", "codex": "gpt-6.2-sol"}
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
# The fleet's toolchains as the person confirmed them, each with where it came from, beside answers.json: what
# fleet.json's toolchains and systemPackages are written from, the repositories detection has proposed from, what
# was declined, and a proposal waiting for an answer.
TOOLCHAIN_RECORD = "toolchains.json"
# Where detection reads the repositories: the rear admiral's reference checkouts, which hold every repository,
# private ones included, as the mirror service refreshes them. Officers' homes hold their per-user tools.
REFERENCES = Path("/var/lib/subspace-fleet/references")
AGENT_HOMES = Path("/var/lib/subspace-fleet/agents")


class SetupError(Exception):
    pass


class ApiError(SetupError):
    def __init__(self, message, status, detail=""):
        super().__init__(message + (f" {detail}" if detail else ""))
        self.status = status
        self.detail = detail


def problem_detail(error):
    """The detail of an API problem response, one line of at most 400 characters, or ''."""
    try:
        detail = json.load(error).get("detail")
    except (ValueError, AttributeError, OSError):
        return ""
    return " ".join(str(detail).split())[:400] if isinstance(detail, str) else ""


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
    parser.add_argument("--topology", choices=["single", "several", "join"],
                        help="one fleet, the first of several, or joining an existing installation; kept on reruns")
    parser.add_argument("--name", help="fleet display name, for example 'Example Fleet'")
    parser.add_argument("--slug", help="stable fleet ID; defaults to the name in lower case")
    parser.add_argument("--rear-admiral", help="the rear admiral's name, for example Atlas")
    parser.add_argument("--captains", help="comma-separated captain (ship) names (new fleets default to four). Each "
                        "officer's name in lower case is its one name: IRC account and nick, Subspace API agent name "
                        "and Stargate ID, unique on the IRC server and in the API's organisation")
    parser.add_argument("--crew-names", help="names for each captain's crew, in slot order (Claude-family crew, then "
                        "Codex-family), as 'CAPTAIN=NAME,NAME,...;CAPTAIN=...', for example "
                        "'Orion=Rigel,Deneb,Mira,Sirius'. Required for a new captain with crew; unique in the fleet "
                        "ignoring case, and none an officer's name. Kept on later runs")
    parser.add_argument("--admin", help="your IRC account and nickname (default admiral)")
    parser.add_argument("--admin-email", help="development sign-in email (default developer@example.com)")
    parser.add_argument("--organisation", help="development sign-in organisation (default local)")
    parser.add_argument("--repositories", help="comma-separated HTTPS Git URLs that every captain checks out, "
                        "optionally as ID=URL; crew get one worktree of each. Replaces the fleet's list; "
                        "'' clears it. Private GitHub repositories need GitHub Apps first (installer README)")
    parser.add_argument("--crew-claude", type=int, help="Claude-family crew per captain (default 2; crew need a repository)")
    parser.add_argument("--crew-codex", type=int, help="Codex-family crew per captain (default 2; crew need a repository)")
    for family, model in CREW_MODEL_DEFAULTS.items():
        parser.add_argument(f"--crew-{family}-model", help=f"model of each {family.title()}-family crew member "
                            f"(new fleets default to {model}; kept on later runs; '' selects {model})")
    for role, flag, default in (("captain", "captain", runtime.OFFICER_DEFAULTS["captain"]),
                                ("rear_admiral", "rear-admiral", runtime.OFFICER_DEFAULTS["rear_admiral"])):
        who = "each captain" if role == "captain" else "the rear admiral"
        parser.add_argument(f"--{flag}-model", help=f"model of {who}, such as gpt-5.4 (default {default['model']}; "
                            "a claude-* model goes through the Claude family, any other through Codex; kept on later "
                            "runs; '' returns to the default)")
        parser.add_argument(f"--{flag}-effort", help=f"reasoning effort of {who}: {', '.join(EFFORTS)} (default "
                            f"{default['effort']} on {default['model']}, else the provider's; kept on later runs; '' "
                            "returns to the default)")
    parser.add_argument("--captain-memory", help="each captain's memory hard cap, its crew included, such as 24G: "
                        "'auto' (the default) sizes it from the VM's memory and the fleet, 'none' sets no cap. Kept on "
                        "later runs (installer README, 'Memory limits and swap')")
    parser.add_argument("--rear-admiral-memory", help="the rear admiral's memory hard cap, such as 4G, 'auto' (the "
                        "default) or 'none'. Kept on later runs")
    parser.add_argument("--swap", help="the VM's swap: 'auto' (the default: a swap file of an eighth of the VM's "
                        "memory, 2 to 16 GiB, when it has no swap), a size such as 8G, or 'none' to leave swap as it "
                        "is. Kept on later runs")
    parser.add_argument("--demo", dest="demo", action="store_true", default=None, help="seed fictional demo data")
    parser.add_argument("--no-demo", dest="demo", action="store_false")
    parser.add_argument("--irc", choices=["local", "remote"],
                        help="local runs Ergo on this VM; remote uses a shared IRC server for several fleets")
    parser.add_argument("--irc-host", help="shared IRC server's DNS name (remote mode; must match its TLS certificate)")
    parser.add_argument("--irc-port", type=int, help="shared IRC server's TLS port (default 6697)")
    parser.add_argument("--irc-ca", help="absolute path on this VM to the shared server's CA certificate (PEM)")
    parser.add_argument("--managed-shared-irc", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--irc-auth", choices=["certificate", "password"],
                        help="how officers sign in to IRC: certificate (SASL EXTERNAL, the default for new fleets) "
                        "or password (SASL PLAIN). A rerun keeps the installed mode.")
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
    parser.add_argument("--agent-listener", action=argparse.BooleanOptionalAction,
                        help="use the legacy loopback agent listener; --no-agent-listener selects signed access")
    parser.add_argument("--shared-api-target", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--database-url-file", help="run the API on this VM against an existing PostgreSQL, with no "
                        "PostgreSQL container here: absolute path to a root-only file (mode 0600) holding its "
                        "postgresql://user:password@host:5432/database?sslmode=verify-full URL. Kept on later runs")
    parser.add_argument("--database-ca-file", help="absolute path on this VM to the CA certificate (PEM) of that "
                        "PostgreSQL server, when the system trust store lacks it; '' removes it")
    parser.add_argument("--migrate", action="store_true", help="apply the API's database migrations to that existing "
                        "PostgreSQL on this run (never by default; see the installer README)")
    parser.add_argument("--agentsview", action=argparse.BooleanOptionalAction, default=None,
                        help="install the AgentsView session browser (default on; remembers your choice)")
    parser.add_argument("--usage-guard", action=argparse.BooleanOptionalAction, default=None,
                        help="enable Claude and Codex quota notices and crew cutoffs (default on)")
    parser.add_argument("--usage-guard-warning", action="append", default=[], metavar="[PROVIDER=]PERCENT",
                        help=f"quota warning for Claude and Codex, or one provider (default {runtime.USAGE_GUARD_DEFAULTS['warning']})")
    parser.add_argument("--usage-guard-cutoff", action="append", default=[], metavar="[PROVIDER=]PERCENT",
                        help=f"block crew starts and resumptions at this quota (default {runtime.USAGE_GUARD_DEFAULTS['cutoff']})")
    parser.add_argument("--stargate-api", choices=["auto", "on", "off"],
                        help="enable Stargate's Fleet API (default auto: when the pinned Stargate has it)")
    parser.add_argument("--mockup-previews", action=argparse.BooleanOptionalAction, default=None,
                        help="install Caddy and the captain preview tool (default on; keeps the saved choice)")
    parser.add_argument("--impeccable", action=argparse.BooleanOptionalAction, default=None,
                        help="install the Impeccable skill for captains (default on; keeps the saved choice)")
    parser.add_argument("--preview-url", help="dedicated private HTTP or HTTPS preview origin; "
                                             "'tunnel' restores the SSH default")
    parser.add_argument("--preview-proxy-user", help="Linux account of the local private gateway, allowed to reach Caddy")
    parser.add_argument("--fleet-api-url", help="the HTTPS address at which a desktop reaches Stargate's Fleet API, "
                        "such as a Tailscale or Cloudflare name, recorded in the Subspace API; 'tunnel' (the default) "
                        "records the SSH tunnel's end on the desktop, http://127.0.0.1:8484")
    parser.add_argument("--stargate-ref", help="'latest' (the default: Stargate's newest release, looked up on "
                        "every run) or a full commit SHA to pin")
    parser.add_argument("--captain-peer-messages", action=argparse.BooleanOptionalAction, default=None,
                        help="Enable cross-ship messages between pinned crew-hosting captains.")
    parser.add_argument("--agent-dispatch", dest="agent_dispatch", action="store_true", default=None,
                        help="switch on Stargate's agent dispatch: " + DISPATCH_SUMMARY + " (default off; needs the API)")
    parser.add_argument("--no-agent-dispatch", dest="agent_dispatch", action="store_false")
    parser.add_argument("--assignment-requests", action=argparse.BooleanOptionalAction, default=None,
                        help="ask captains to take assigned issues when crew are ready (default on with agent dispatch)")
    parser.add_argument("--ready-for-agents-offers", action=argparse.BooleanOptionalAction, default=None,
                        help="offer unassigned Ready for agents issues (default on with agent dispatch)")
    parser.add_argument("--decision-outcome-dms", action=argparse.BooleanOptionalAction, default=None,
                        help="Send decision outcomes by DM when the local API and Stargate support it.")
    parser.add_argument("--fleet-audit", action=argparse.BooleanOptionalAction, default=None,
                        help="mirror officer DMs to the private fleet audit channel (default on for new local IRC installs). "
                             "On remote IRC, provision the private channel before enabling it.")
    parser.add_argument("--operator", help="Linux login that may SSH in and tunnel (default: the sudo user)")
    parser.add_argument("--access", choices=["ssh", "tailscale", "cloudflare"],
                        help="how you reach the VM; tailscale limits SSH to the tailnet, cloudflare to cloudflared")
    parser.add_argument("--ssh-from", help="comma-separated source addresses or CIDRs allowed to SSH in")
    parser.add_argument("--allow-port", help="comma-separated TCP ports to accept from private networks only, "
                        "for example 443 for an nginx listener on the tailnet")
    parser.add_argument("--private-network", metavar="NAME,INTERFACE,ADDRESS,CLIENT-CIDR",
                        help="serve HTTPS and TLS IRC on an existing private route; remembered on reruns (or set off)")
    parser.add_argument("--accept-officer-tracker-access-loss", action="store_true",
                        help="allow private ingress to remove an existing local API's officer tracker tools, "
                        "signed enrolment and dispatch; not saved")
    parser.add_argument("--agent-egress", action="append", metavar="HOST:PORT",
                        help="allow a private TCP destination for officers. Repeat to set several; "
                        "use --agent-egress '' to clear the saved list")
    parser.add_argument("--key-vault", help="Azure Key Vault name for the generated secrets (managed identity)")
    parser.add_argument("--metadata-user", help="comma-separated Linux users, besides root and the operator, that "
                        "may reach the cloud metadata service (the agents never can)")
    parser.add_argument("--no-lockdown", dest="lockdown", action="store_false", default=None,
                        help="skip the inbound firewall and SSH hardening (not recommended; agent isolation stays)")
    parser.add_argument("--inputs", default=str(INPUTS), help=argparse.SUPPRESS)
    parser.add_argument("--dry-run", action="store_true", help="validate the inputs only, changing nothing")
    parser.add_argument("--non-interactive", action="store_true", help="never prompt; fail on missing answers")
    parser.add_argument("--initiative", help="adopt an existing initiative, by its UUID, as the fleet's, instead of "
                        "the one setup finds or creates by the fleet's name. With the API on this VM setup assigns it "
                        "to this fleet, moving it from any other; a hosted API's administrator assigns it. '' goes "
                        "back to finding one by name. Kept on later runs")
    parser.add_argument("--captain-tools", choices=["standard", "delegate"],
                        help="delegate (the default for a new fleet) leaves edits, commits and pushes to the crew and "
                        "gives captains an open shell without them; standard lets captains code. Kept on later runs; "
                        "a fleet set up before this option keeps standard")
    parser.add_argument("--captain-merge-repositories",
                        help="GitHub owner/repository names, comma-separated, where captains may squash-merge "
                        "reviewed heads. Use 'none' to turn it off. Kept on later runs")
    parser.add_argument("--toolchains", help="the toolchains the repositories need: 'derive' (after cloning, setup "
                        "reads the repositories and proposes what they need, installing nothing until you confirm "
                        "it), 'none' (the default: Node 24 and the base tools), or tools as TOOL@VERSION, "
                        "comma-separated, such as node@22.11.0,pnpm@9.12.3,cargo:cargo-nextest@0.9.100, which "
                        "confirms them; --toolchains=-TOOL or =-TOOL@VERSION removes one. Kept on later runs")
    parser.add_argument("--system-packages", help="Debian packages the repositories need, installed with apt, "
                        "comma-separated, such as zsh,clang,libssl-dev; --system-packages=-PACKAGE takes one off the "
                        "list (it stays installed). Kept on later runs")
    parser.add_argument("--verify-toolchains", action="store_true", help="after installing, run each repository's "
                        "basic check as a crew member would and report pass or fail per repository; never stops the "
                        "install")
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


def gather_topology(args, answers, saved, interactive):
    """Choose the fleet layout; use the existing options to place its services."""
    hosted_api(args, answers, saved)
    remote_irc = (args.irc or saved.get("irc", "local")) == "remote"
    shared_api = args.shared_api_target or bool(answers.get("apiUrl"))
    suggested = "join" if remote_irc and shared_api else "several" if remote_irc or shared_api else "single"
    saved_topology = saved.get("topology")
    choice = args.topology or saved_topology
    if choice is None and interactive and not saved:
        choice = prompt("Topology: single (recommended: this VM runs the fleet, API, Ergo and PostgreSQL), "
                        "several (first of several fleets), or join (use existing services)", suggested)
    require(choice is None or choice in ("single", "several", "join"), "Choose single, several or join for topology.")
    if choice == "join" and not (remote_irc and shared_api):
        require(saved_topology and args.topology is None,
                "To join, use shared IRC (--irc-server or --irc remote) and a shared API (--api-target or --api-url).")
        say(f"The saved topology 'join' needs both shared services; recording '{suggested}' for these endpoints.")
        choice = suggested
    elif choice == "single" and suggested != "single":
        require(saved_topology and args.topology is None,
                "The 'single' topology uses local IRC and no shared API; choose 'several' or 'join' for shared services.")
        say(f"The saved topology 'single' uses no shared services; recording '{suggested}' for these endpoints.")
        choice = suggested
    answers["topology"] = choice or suggested
    answers["irc"] = "remote" if remote_irc else "local"
    if remote_irc:
        if args.irc_host is not None:
            answers["ircHost"] = args.irc_host
        elif "ircHost" not in answers:
            answers["ircHost"] = prompt("Shared IRC server's DNS name", interactive=interactive)
    if args.shared_api_target:
        require(not answers.get("apiUrl"), "--api-target cannot replace an explicit or saved hosted API (--api-url).")
        hostname = answers.get("ircHost")
        require(remote_irc and hostname and not loopback_irc_host(hostname),
                "--api-target needs remote IRC: use --irc-server NAME or --irc remote --irc-host HOST.")
        args.api = False
    if answers.get("apiUrl"):
        answers["api"] = False  # The hosted API replaces one on this VM.
    elif args.api is not None:
        answers["api"] = args.api
    elif args.database_url_file and answers.get("api") is False:
        # A fleet set up with --no-api: asking for an existing PostgreSQL asks for the API that uses it.
        answers["api"] = True
    answers.setdefault("api", True)


def gather_workstation_network(args, answers, saved, interactive):
    # SSH controls administration, separately from whether workstations use direct endpoints.
    answers["access"] = args.access or saved.get("access", "ssh")
    choice = args.private_network if args.private_network is not None else saved.get("privateNetwork")
    if choice is None and not saved and interactive:
        choice = prompt("Private network for workstations: off (keep the SSH tunnel), or use an existing private "
                        "route (NAME,INTERFACE,ADDRESS,CLIENT-CIDR; requires local API and Ergo, configured SSO and TLS)",
                        "off")
    network = private_network.Network.parse(choice)
    answers["privateNetwork"] = network.choice if network else "off"


def gather_identity(args, answers, saved, interactive):
    # Until SUB-76 supplies owner bootstrap, keep the configured sign-in or the existing development path.
    environment = {**DEV_AUTH, **read_api_settings()} if interactive and answers["api"] else {}
    development = dev_sign_in(environment)
    if development and answers["privateNetwork"] != "off":
        private_network.require_sso(environment)
    if development and (not saved or not saved.get("api", True)):
        require(yes_no("Identity: use development sign-in to register this fleet on its local API for now? "
                       "Owner device pairing is not available in setup yet", False, interactive),
                "Set up the API's production sign-in first (see INSTALL.md), use a hosted API, pass --no-api, "
                "or rerun and answer y to use development sign-in.")
    elif interactive and not saved:
        say("Identity: setup leaves sign-in to the configured API, if any. It does not pair an owner.")
    for key, flag, label, default in (
            ("adminEmail", args.admin_email, "Development sign-in email", "developer@example.com"),
            ("organisation", args.organisation, "Development sign-in organisation", "local")):
        if flag is not None:
            answers[key] = flag
        elif key not in answers:
            answers[key] = prompt(label, default) if development else default


def gather(args, saved):
    interactive = sys.stdin.isatty() and not args.non_interactive
    answers = dict(saved)
    fresh = new_fleet(saved)

    def pick(key, flag, label, default=None):
        if flag is not None:
            answers[key] = flag
        elif key not in answers:
            answers[key] = prompt(label, default, interactive) if interactive else default
        return answers[key]

    gather_topology(args, answers, saved, interactive)
    gather_workstation_network(args, answers, saved, interactive)
    gather_identity(args, answers, saved, interactive)
    pick("name", args.name, "Fleet name (shown in the desktop and on IRC)")
    require(answers.get("name"), "Choose a fleet name with --name.")
    default_slug = re.sub(r"[^a-z0-9]+", "-", answers["name"].lower()).strip("-")[:20].strip("-")
    pick("slug", args.slug, "Fleet ID (lower case, never changes)", default_slug)
    pick("rearAdmiral", args.rear_admiral, "Rear admiral's name")
    require(answers.get("rearAdmiral"), "Choose the rear admiral's name with --rear-admiral.")
    captains = args.captains if args.captains is not None else answers.get("captains")
    if captains is None:
        default = None
        if fresh:
            stem = answers["slug"]
            if len(stem) > 12:
                # Preserve a distinction between long fleet IDs sharing a prefix within the 16-character name limit.
                suffix = hashlib.sha256(stem.encode()).hexdigest()[:4]
                stem = f"{stem[:7].rstrip('-')}-{suffix}"
            default = ",".join(f"c-{stem}-{number}" for number in range(1, 5))
        captains = prompt("Captain (ship) names, comma-separated", default, interactive)
    if isinstance(captains, str):
        captains = [name.strip() for name in captains.split(",") if name.strip()]
    require(captains, "Name at least one captain with --captains.")
    answers["captains"] = captains
    if "officerNames" not in answers:
        # A fleet set up before plain names keeps its fleet-prefixed accounts until 'rename-officers' changes them.
        answers["officerNames"] = "prefixed" if saved.get("slug") else "plain"
    pick("admin", args.admin, "Your IRC account name", "admiral")
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
    named = answers.setdefault("crewNames", {})
    # A captain taken out of the roster takes its crew's saved names and IDs along; one added again names its crew
    # anew, as a new captain does.
    captain_ids = {ident(name, "Captain") for name in answers["captains"]}
    for key in ("crewNames", "crewIds"):
        for captain_id in [captain_id for captain_id in answers.get(key) or {} if captain_id not in captain_ids]:
            del answers[key][captain_id]
    if args.crew_names is not None:
        given = parse_crew_names(args.crew_names, answers)
        refuse_installed_crew_ids(given)
        named.update(given)
    repositories = answers.get("repositories") or (read_json(INPUTS / "fleet.json", {}) or {}).get("repositories")
    if interactive and repositories:
        # Crew need a repository; ask for their names once a captain has any.
        for name in answers["captains"]:
            captain_id = ident(name, "Captain")
            slots = crew_slots(answers)
            if slots and captain_id not in named and not crew_installed(captain_id):
                given = prompt(f"Names for {name}'s crew ({', '.join(slot_label(slot) for slot in slots)}), "
                               "comma-separated", None, interactive) or ""
                named.update(parse_crew_names(f"{name}={given}", answers))
    if args.captain_peer_messages is not None:
        answers["captainPeerMessages"] = args.captain_peer_messages
    if args.captain_tools is not None:
        answers["captainTools"] = args.captain_tools
    elif fresh:
        # A new fleet: captains with crew leave coding to them. A fleet set up before captainTools existed has no
        # answer and keeps the standard policy, which a fleet.json without captainTools means.
        policy = prompt("Captain tools: delegate (crew edit, commit and push; captains plan, review and open pull "
                        "requests) or standard (captains code too)", "delegate", interactive)
        require(policy in ("standard", "delegate"), "Choose standard or delegate for --captain-tools.")
        answers["captainTools"] = policy
    merge_repos = args.captain_merge_repositories
    if merge_repos is None and interactive and not saved and repositories and not (INPUTS / "fleet.json").exists():
        merge_repos = prompt("May captains squash-merge pull requests? Enter the GitHub owner/repository names "
                             "(comma-separated), or 'none'", "none")
    if merge_repos is not None:
        answers["captainMergeRepositories"] = (
            [] if merge_repos.strip() in ("", "none") else [repo.strip() for repo in merge_repos.split(",")]
        )
    if args.toolchains in ("derive", "none"):
        answers["toolchains"] = args.toolchains
    elif args.toolchains is not None:
        # Tools listed by hand: a fleet that derives keeps deriving for repositories added later.
        if answers.get("toolchains") != "derive":
            answers["toolchains"] = "list"
    elif "toolchains" not in answers:
        if interactive and not saved and repositories:
            # The default is the documented one, none, as for unattended runs (INSTALL.md, question 21).
            choice = prompt("Toolchains the repositories need: derive (setup reads them after cloning and proposes "
                            "what they need, for you to confirm), none, or tools as TOOL@VERSION,...", "none",
                            interactive) or "none"
            if choice not in ("derive", "none"):
                args.toolchains, choice = choice, "list"
            answers["toolchains"] = choice
        else:
            # Unattended, and fleets set up before the question: no detection; Node 24 and the base tools.
            answers["toolchains"] = "none"
    if args.initiative == "":
        answers.pop("initiativeId", None)
    elif args.initiative is not None:
        require(UUID.fullmatch(args.initiative), "--initiative must be an initiative's UUID, such as "
                "00000000-0000-4000-8000-000000000001 (the desktop shows it, as does GET /api/v1/initiatives).")
        answers["initiativeId"] = args.initiative.lower()
    models = answers.setdefault("crewModels", {})
    for family, flag in (("claude", args.crew_claude_model), ("codex", args.crew_codex_model)):
        default = CREW_MODEL_DEFAULTS[family]
        if flag is not None:
            require(flag == "" or CREW_MODEL.fullmatch(flag),
                    f"--crew-{family}-model must be a model ID, such as {default}.")
            models[family] = flag or default
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
    # Memory: each role's hard cap and the VM's swap. auto is the fleet installer's own sizing, even over a value
    # set in fleet.json by hand; the soft limit and the swap allowance follow the hard cap (lib/memory.py).
    chosen = answers.setdefault("memory", {})
    for key, value, flag in (("captain", args.captain_memory, "--captain-memory"),
                             ("rearAdmiral", args.rear_admiral_memory, "--rear-admiral-memory"),
                             ("swap", args.swap, "--swap")):
        if value is None:
            continue
        value = value.strip() or "auto"
        check = memory.swap_setting if key == "swap" else memory.parse_size
        try:
            if value != "auto":
                check(value, flag)
        except memory.Refused as error:
            raise SetupError(f"{error} 'auto' is the installer's own sizing.") from None
        chosen[key] = value
    if answers["irc"] == "remote":
        if args.managed_shared_irc:
            answers["managedSharedIrc"] = True
        elif args.irc_host and args.irc_host != saved.get("ircHost"):
            answers.pop("managedSharedIrc", None)
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
        answers.pop("managedSharedIrc", None)
        answers["ircAuth"] = args.irc_auth or (
            saved.get("ircAuth", "password") if saved.get("irc") == "local" else "certificate")
        require(not (saved.get("irc") == "local" and saved.get("ircAuth", "password") != answers["ircAuth"]),
                "An installed local fleet must retain its IRC sign-in mode during setup.")
    require(not (args.irc_operator or args.irc_operator_password_file)
            or (answers["irc"] == "remote" and answers.get("ircAuth") == "certificate"),
            "--irc-operator is for certificate sign-in on a remote IRC server (--irc remote --irc-auth certificate).")
    require(bool(args.irc_operator) == bool(args.irc_operator_password_file),
            "Pass --irc-operator and --irc-operator-password-file together.")
    require(bool(args.irc_operator_account) == bool(args.irc_operator_account_password_file),
            "Pass --irc-operator-account and --irc-operator-account-password-file together.")
    require(not args.irc_operator_account or args.irc_operator,
            "--irc-operator-account is the account the operator's connection signs in to: pass --irc-operator too.")
    if args.vm_host == "":
        answers.pop("vmHost", None)
    elif args.vm_host is not None:
        try:
            answers["vmHost"] = runtime.host(args.vm_host.strip().lower())
        except runtime.Invalid:
            raise SetupError("--vm-host must be a DNS name or an IP address, such as fleet.example.com.") from None
    require(args.agent_listener is not True or answers["api"],
            "--agent-listener requires a Subspace API on this VM.")
    answers["agentListener"] = args.agent_listener
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
    installed = read_json(INPUTS / "fleet.json", {}) or {}
    if args.fleet_audit is not None:
        answers["fleetAudit"] = args.fleet_audit
    elif answers.get("fleetAudit") is True and answers["irc"] == "remote" and not same_remote_irc(answers, saved, installed):
        answers.pop("fleetAudit")
        say("Fleet Audit is off on the new IRC server. Provision its private channel before enabling it.")
    installed_dispatch = installed.get("agentDispatch")
    for key, flag in (("assignmentRequests", args.assignment_requests),
                      ("readyOffers", args.ready_for_agents_offers)):
        if flag is not None:
            answers[key] = flag
        elif isinstance(installed_dispatch, dict) and key in installed_dispatch:
            answers[key] = installed_dispatch[key]
    installed_view = installed.get("agentsview", {})
    if args.agentsview is not None:
        answers["agentsview"] = args.agentsview
    elif "enabled" in installed_view:
        answers["agentsview"] = installed_view["enabled"]
    elif "agentsview" not in answers:
        answers["agentsview"] = yes_no("Install the AgentsView session browser (recommended)?", True, interactive)
    preview = {**installed.get("mockupPreviews", {}), **answers.get("mockupPreviews", {})}
    if args.mockup_previews is not None:
        preview["enabled"] = args.mockup_previews
    if args.preview_url == "tunnel":
        preview["url"] = None
    elif args.preview_url is not None:
        try:
            preview["url"] = mockups.origin(args.preview_url)
        except fleet_records.Refused as error:
            raise SetupError(str(error)) from None
    if args.preview_proxy_user is not None:
        preview["proxyUser"] = args.preview_proxy_user or None
    answers["mockupPreviews"] = preview
    if args.impeccable is not None:
        answers["impeccable"] = args.impeccable
    elif "impeccable" not in answers:
        answers["impeccable"] = installed.get("impeccable", True)
    previous_dms = installed_dispatch.get("decisionOutcomeDms") if isinstance(installed_dispatch, dict) else None
    if args.decision_outcome_dms is not None:
        answers["decisionOutcomeMode"] = "on" if args.decision_outcome_dms else "off"
    elif previous_dms is False:
        answers["decisionOutcomeMode"] = "off"
    elif "decisionOutcomeMode" not in answers:
        answers["decisionOutcomeMode"] = (
            "on" if previous_dms is True else "auto" if fresh else "off"
        )
    if (args.decision_outcome_dms is None and answers["decisionOutcomeMode"] == "on"
            and answers["irc"] == "remote" and not same_remote_irc(answers, saved, installed)):
        answers["decisionOutcomeMode"] = "off"
        say("Decision outcome DMs are off on the new IRC connection. Confirm it supports delivery, then "
            "run setup with --decision-outcome-dms to opt in.")
    answers["usageGuard"] = usage_guard_answers(args, answers)
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
    if args.ssh_from is not None:
        answers["sshFrom"] = [value.strip() for value in args.ssh_from.split(",") if value.strip()]
    answers.setdefault("sshFrom", [])
    if args.allow_port is not None:
        answers["allowPorts"] = [value.strip() for value in args.allow_port.split(",") if value.strip()]
    answers.setdefault("allowPorts", [])
    if args.agent_egress is not None:
        answers["agentEgress"] = list(dict.fromkeys(value for value in args.agent_egress if value))
    if args.key_vault is not None:
        answers["keyVault"] = args.key_vault
    if args.metadata_user is not None:
        answers["metadataUsers"] = [value.strip() for value in args.metadata_user.split(",") if value.strip()]
    answers.setdefault("metadataUsers", [])
    if args.lockdown is not None:
        answers["lockdown"] = args.lockdown
    answers.setdefault("lockdown", True)
    return answers


def crew_slots(answers):
    """Each captain's crew slots in order, Claude-family first: claude-1, claude-2, codex-1, ..."""
    return [f"{family}-{slot}" for family in ("claude", "codex")
            for slot in range(1, int(answers.get("crew", {}).get(family, 0) or 0) + 1)]


def slot_label(slot):
    family, _, number = slot.partition("-")
    return f"{family.title()} {number}"


def parse_crew_names(value, answers):
    """--crew-names 'CAPTAIN=NAME,NAME,...;CAPTAIN=...' as {captain ID: {slot: name}}, names in slot order."""
    result = {}
    slots = crew_slots(answers)
    for part in (item.strip() for item in value.split(";")):
        if not part:
            continue
        captain, equals, names = part.partition("=")
        require(equals and captain.strip(), f"--crew-names takes CAPTAIN=NAME,NAME,... for each captain, separated by "
                f"semicolons, not {part!r}.")
        captain_id = ident(captain, "Captain")
        require(captain_id in [ident(name, "Captain") for name in answers["captains"]],
                f"--crew-names names {captain.strip()}, which is not one of the captains.")
        listed = [name.strip() for name in names.split(",") if name.strip()]
        require(len(listed) == len(slots), f"--crew-names gives {captain.strip()} {len(listed)} crew names for "
                f"{len(slots)} crew ({', '.join(slot_label(slot) for slot in slots)}): name each one, in that order.")
        result[captain_id] = dict(zip(slots, listed, strict=True))
    return result


def crew_installed(captain_id):
    """Whether this captain's crew are installed: the fleet's inputs have the captain and repositories, so its crew
    exist (with names or slot names) and keep them; a captain without crew yet names them when they first come."""
    config = read_json(INPUTS / "fleet.json", {}) or {}
    return bool(config.get("repositories")) and any(
        isinstance(person, dict) and person.get("id") == captain_id for person in config.get("captains", []))


def installed_crew(captain_id):
    """An installed captain's crew by slot as (ID, name), from the fleet's inputs, or None for a captain whose crew
    are not installed: the ID Stargate keeps each one's sessions, worktrees and queued tasks under, from crewIds,
    else its name's; the name from crewNames, else the slot's own name, 'Claude 1'."""
    if not crew_installed(captain_id):
        return None
    config = read_json(INPUTS / "fleet.json", {}) or {}
    person = next(person for person in config["captains"] if isinstance(person, dict) and person.get("id") == captain_id)
    counts = {}
    for family in ("claude", "codex"):
        team = {**((config.get("crew") or {}).get(family) or {}), **((person.get("crew") or {}).get(family) or {})}
        counts[family] = team.get("count", 2)  # The fleet installer's default.
    names, ids = person.get("crewNames") or {}, person.get("crewIds") or {}
    crew = {}
    for slot in crew_slots({"crew": counts}):
        name = names.get(slot, slot_label(slot))
        crew[slot] = (ids.get(slot) or runtime.crew_id(name), name)
    return crew


def refuse_installed_crew_ids(named):
    """Refuse --crew-names that would give an installed crew member another ID, before anything is saved: its
    sessions, worktrees and queued tasks are kept under its ID, and a captain whose session is pinned refuses any
    change on every later run. rename-officers --crew-names names them and keeps their IDs; the name a crew member
    has is accepted again, also when it keeps another ID under it (crewIds)."""
    changed = []
    for captain_id, names in named.items():
        installed = installed_crew(captain_id) or {}
        changed += [f"{captain_id}'s {slot_label(slot)} ({installed[slot][0]} would become {runtime.crew_id(name)})"
                    for slot, name in names.items() if slot in installed
                    and runtime.crew_id(name) not in (installed[slot][0], runtime.crew_id(installed[slot][1]))]
    require(not changed, f"--crew-names would give installed crew new IDs: {'; '.join(changed)}. Stargate keeps a crew "
            "member's sessions, worktrees and queued tasks under its ID, so setup never changes an installed one. "
            "Name an installed fleet's crew with 'sudo subspace-setup rename-officers --crew-names ...', which keeps "
            "their IDs (installer README, 'Crew names'), or give these captains their installed names. Nothing was "
            "changed.")


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
    names = [answers["rearAdmiral"], *answers["captains"]]
    for index, agent in enumerate(ids):
        for other in range(index):
            require(ids[other] != agent,
                    f"{names[other]} and {names[index]} differ only by case or punctuation, but each officer's name is "
                    "its IRC account and its agent name in the Subspace API, which compare names without case. Give "
                    "the rear admiral and every captain a distinct name." if names[other] != names[index] else
                    "The rear admiral and captains need distinct names.")
    require(len(answers["captains"]) <= 12, "Use at most 12 captains.")
    admin = answers["admin"]
    require(ACCOUNT.fullmatch(admin) and admin not in RESERVED and not admin.startswith("guest-"),
            "Choose an IRC account of lower-case letters, digits or hyphens that starts with a letter, such as 'admiral'.")
    plain = answers.get("officerNames") == "plain"
    agent_accounts = [officer_account(answers, agent) for agent in ids]
    require(admin not in ids and admin not in agent_accounts
            and not any(admin.startswith(f"{account}-") for account in agent_accounts[1:] if not plain),
            "Your IRC account must differ from the agents' names and accounts.")
    for agent in ids:
        # With plain names an officer's ID is its IRC account and API username, which these may not be.
        require(not plain or agent not in RESERVED | {"botserv", "guest"} and not agent.startswith("guest-"),
                f"{agent} is a name the IRC server, the Subspace API or the installer keeps for itself. Choose "
                "another name for that officer.")
        require(len(officer_account(answers, agent)) <= standard_channels.MAX_ACCOUNT,
                f"'{officer_account(answers, agent)}' is too long for its IRC account. Use a shorter fleet ID or name.")
    trouble = standard_channels.problem(answers["slug"], ids[1:], plain)
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
    require(answers["decisionOutcomeMode"] in ("auto", "on", "off"), "Invalid saved decision outcome DM setting.")
    require(not answers.get("agentDispatch") or answers["api"] or answers.get("apiUrl"),
            "Agent dispatch needs the API on this VM or a hosted API URL: drop --no-api, configure --api-url, or pass "
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
    try:
        runtime.agent_egress(answers.get("agentEgress", []))
    except runtime.Invalid as error:
        raise SetupError(f"--agent-egress: {error}") from None
    if answers.get("keyVault"):
        require(VAULT.fullmatch(answers["keyVault"]), "Invalid Key Vault name.")
    for user in answers.get("metadataUsers", []):
        require(LINUX_USER.fullmatch(user) and user != "root", f"Invalid --metadata-user {user}.")
        require(not user.startswith("ss-") or user == GATEWAY_USER,
                f"--metadata-user {user} is a fleet agent's account; agents never reach cloud metadata.")
    for name in [answers["name"], *answers["captains"], answers["rearAdmiral"]]:
        require("\n" not in name and len(name) <= 100, "Names must be single lines of at most 100 characters.")
    # The API keeps each captain's name as its ship's name, and with fleet-prefixed names the rear admiral's as its
    # agent name; with plain names every officer's agent name is its ID.
    require(plain or fleet_records.AGENT_NAME.fullmatch(answers["rearAdmiral"]),
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
    check_crew_answers(answers, ids)
    return rear, ids[1:]


def officer_account(answers, agent_id):
    """An officer's IRC account: its ID with plain names, else <slug>-<ID> as setup named them before."""
    return agent_id if answers.get("officerNames") == "plain" else f"{answers['slug']}-{agent_id}"


def check_crew_answers(answers, ids):
    """Crew names from --crew-names: as the fleet installer takes them (runtime.crew_roster), unique in the fleet
    ignoring case, none an officer's name, and one for every crew member of a captain not yet installed."""
    officers = {runtime.crew_id(value) for value in [*ids, answers["rearAdmiral"], *answers["captains"]]}
    seen = {}
    slots = crew_slots(answers)
    # Only the enabled slots: names saved for crew a later run no longer has stay unused (roster() passes none).
    named = {captain_id: {slot: name for slot, name in names.items() if slot in slots}
             for captain_id, names in (answers.get("crewNames") or {}).items()}
    for captain_id, names in named.items():
        require(captain_id in ids[1:], f"Crew names are given for {captain_id}, which is not one of the captains.")
        for slot, name in names.items():
            require(isinstance(name, str) and re.fullmatch(r"[A-Za-z0-9 _-]{1,64}", name.strip())
                    and re.fullmatch(runtime.CREW_ID, runtime.crew_id(name)),
                    f"Crew name {name!r} for {captain_id}: use ASCII letters, digits, spaces, hyphens and "
                    "underscores, which shorten to an ID of at most 32 characters.")
            key = runtime.crew_id(name)
            require(key not in officers, f"Crew name {name} for {captain_id} is an officer's name. Crew names are "
                    "unique in the fleet and differ from the officers' names.")
            require(key not in seen, f"Crew names {seen.get(key)} and {captain_id}'s {name} are the same, ignoring "
                    "case. Give each crew member in the fleet its own name.")
            seen[key] = f"{captain_id}'s {name}"
    repositories = answers.get("repositories")
    if repositories is None:
        repositories = (read_json(INPUTS / "fleet.json", {}) or {}).get("repositories") or []
    if not (repositories and slots):
        return  # Without repositories, captains have no crew.
    missing = [captain_id for captain_id in ids[1:] if not crew_installed(captain_id)
               and set(named.get(captain_id, {})) != set(slots)]
    require(not missing, f"Name every crew member of {', '.join(missing)}: pass --crew-names with "
            f"{len(slots)} names per captain, in slot order ({', '.join(slot_label(slot) for slot in slots)}), "
            "such as 'Orion=Rigel,Deneb,Mira,Sirius' (INSTALL.md, question 5). Crew names are unique in the fleet.")


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
    # With plain names (new fleets) an officer's ID is its one name; a fleet set up before them keeps accounts that
    # carry the fleet ID until 'rename-officers' changes them.
    members = [{"id": rear, "name": answers["rearAdmiral"], "account": officer_account(answers, rear)}]
    slots = set(crew_slots(answers))
    for captain_id, name in zip(captains, answers["captains"], strict=True):
        person = {"id": captain_id, "name": name, "account": officer_account(answers, captain_id)}
        # Names and IDs for the crew slots this run has: a slot taken away keeps its saved name for later.
        for key in ("crewNames", "crewIds"):
            given = {slot: value for slot, value in ((answers.get(key) or {}).get(captain_id) or {}).items()
                     if slot in slots}
            if given:
                person[key] = given
        members.append(person)
    # A renamed fleet's officers keep the account subjects the API bound their records to (rename-officers).
    for person in members:
        subject = ((answers.get("officerRename") or {}).get("subjects") or {}).get(person["id"])
        if subject:
            person["serviceTokenClientId"] = subject
    return members


def save_answers(answers):
    # Repositories and merge policy live in fleet.json. The listener setting lives in api.env.
    write_json(INPUTS / "answers.json", {key: value for key, value in answers.items()
                                        if key not in {"repositories", "captainMergeRepositories", "agentListener", "decisionOutcomeDms"}})


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


def usage_guard_answers(args, answers):
    try:
        installed = read_json(INPUTS / "fleet.json", {}) or {}
        saved = installed.get("usageGuard", answers.get("usageGuard", {}))
        guard = dict(runtime.fields(saved, "usageGuard", "enabled claude codex"))
        if args.usage_guard is not None:
            guard["enabled"] = args.usage_guard
        for key in ("warning", "cutoff"):
            for value in getattr(args, f"usage_guard_{key}"):
                provider, separator, number = value.partition("=")
                if not separator:
                    provider, number = "", provider
                require((not provider or provider in runtime.PROVIDERS) and re.fullmatch(r"[0-9]{1,2}", number),
                        f"--usage-guard-{key} expects [claude=|codex=]PERCENT from 0 to 99.")
                for name in (provider,) if provider else runtime.PROVIDERS:
                    provider_limits = runtime.fields(guard.get(name, {}), f"usageGuard.{name}", "warning cutoff")
                    guard[name] = {**provider_limits, key: int(number)}
        return runtime.usage_guard_config(guard)
    except runtime.Invalid as error:
        raise SetupError(str(error)) from None


def fleet_config(answers, members, irc_ref, api_block=None, stargate_api=False, stargate_ref=None, history=None):
    # The fleet installer names Starbase, the command channel and the ship channels itself (standard_channels).
    slug = answers["slug"]
    extra_channels = demo_channels() if answers["demo"] else []
    config = {
        "slug": slug,
        "name": answers["name"],
        "officerNames": answers.get("officerNames", "prefixed"),
        "rearAdmiral": members[0],
        "captains": members[1:],
        "crew": {family: {"count": count} for family, count in answers["crew"].items()},
        "operatorUsers": [answers["operator"]],
        "irc": {
            "mode": "local",
            "allowedAccounts": [answers["admin"]],
            "advertise": {"host": LOOPBACK, "port": IRC_PLAIN, "tls": False},
            "compose": {"ref": irc_ref, "operator": OPERATOR_ACCOUNT, "channels": extra_channels},
        },
    }
    if history:
        config["irc"]["compose"]["history"] = history
    if answers["irc"] == "local" and answers.get("api") and automatic_irc_hook(INPUTS / "api.env"):
        config["irc"]["compose"]["localApiIrcHook"] = str(INPUTS / "api-irc-auth" / "hook-token")
    if answers["irc"] == "local" and answers.get("ircAuth") == "certificate":
        tls = INPUTS / "irc-tls"
        config["irc"].update(auth="certificate", tls=True, port=IRC_TLS, caFile=str(tls / "cert.pem"))
        config["irc"]["compose"].update(tlsCert=str(tls / "cert.pem"), tlsKey=str(tls / "key.pem"))
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
    if "fleetAudit" in answers:
        config["irc"]["fleetAudit"] = answers["fleetAudit"]
    previous = read_json(INPUTS / "fleet.json", {})
    if endpoints := answers.get("agentEgress", previous.get("agentEgress", [])):
        config["agentEgress"] = endpoints
    if answers.get("captainPeerMessages", previous.get("captainPeerMessages", False)):
        config["captainPeerMessages"] = True
    if policy := answers.get("captainTools", previous.get("captainTools")):
        config["captainTools"] = policy
    config["toolchains"], packages = toolchain_settings(read_json(INPUTS / TOOLCHAIN_RECORD), previous)
    if packages:
        config["systemPackages"] = packages
    # Keep explicit answers and installed models. Write the new defaults only for a fresh fleet definition.
    chosen = answers.get("crewModels", {})
    for family, crew in config["crew"].items():
        default = (previous.get("crew", {}).get(family) or {}).get("model") if previous else CREW_MODEL_DEFAULTS[family]
        model = chosen.get(family, default)
        if model:
            crew["model"] = model
    # Keep anything the operator added by hand, such as private repositories, GitHub Apps or stakeholders.
    for key in ("harness", "stakeholders", "usage", "medical", "maintenance",
                "githubPolicy", "dashboard", "proxy", "agentsview", "screenshots", "memory"):
        if key in previous:
            config[key] = previous[key]
    preview = {**previous.get("mockupPreviews", {}), **answers.get("mockupPreviews", {})}
    for key in ("url", "proxyUser"):
        if preview.get(key) is None:
            preview.pop(key, None)
    config["mockupPreviews"] = {**preview, "enabled": preview.get("enabled", True) and len(members) > 1}
    config["impeccable"] = answers.get("impeccable", previous.get("impeccable", True))
    view = config.get("agentsview", {})
    if not answers.get("agentsview", True) or "enabled" in view:
        config["agentsview"] = {**view, "enabled": answers.get("agentsview", True)}
    if "captainMergeRepositories" in answers:
        config["githubPolicy"] = {**config.get("githubPolicy", {}),
                                  "captainMergeRepositories": answers["captainMergeRepositories"]}
    # Each role's memory hard cap and the VM's swap: this or an earlier run's answer, over one set by hand. A role's
    # auto is written as auto, which the installer reads as its sizing over a fleet-wide memory.max (an officer's
    # own memory still wins); auto is swap's default, so it needs no entry.
    limits = dict(config.get("memory") or {})
    for key, value in answers.get("memory", {}).items():
        if key == "swap":
            if value == "auto":
                limits.pop("swap", None)
            else:
                limits["swap"] = value
            continue
        limits[key] = {**(limits.get(key) or {}), "max": value}
    # Whether setup runs the API and its PostgreSQL on this VM, for which the installer's sizing leaves room. The api
    # block cannot say so: setup writes it only while the API offers development sign-in, not with WorkOS or Entra.
    if answers.get("api"):
        limits["localApi"] = True
    else:
        limits.pop("localApi", None)
    if limits:
        config["memory"] = limits
    else:
        config.pop("memory", None)
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
    config["usageGuard"] = runtime.usage_guard_config(answers.get("usageGuard", {}))
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
    # Keep hand-set timings and omit unsupported feature keys when their effective value is off.
    dispatch = previous.get("agentDispatch") if isinstance(previous.get("agentDispatch"), dict) else {}
    dispatch = {key: value for key, value in dispatch.items() if key != "decisionOutcomeDms"}
    dispatch_options = {key: answers[key] for key in ("assignmentRequests", "readyOffers") if key in answers}
    if answers.get("decisionOutcomeDms") and (config.get("api") or {}).get("enrolment"):
        dispatch_options["decisionOutcomeDms"] = True
    if dispatch or answers.get("agentDispatch") or dispatch_options:
        config["agentDispatch"] = {
            **dispatch, **dispatch_options, "enabled": bool(answers.get("agentDispatch") and config.get("api")),
        }
    return config


# fleet.json's harness block for each officer role, and who it is.
OFFICER_ROLES = {"captain": "the captains", "rearAdmiral": "the rear admiral"}


# Toolchains ------------------------------------------------------------------

def blank_record():
    return {"tools": [], "packages": [], "declined": {"tools": [], "packages": []}, "examined": [], "proposal": None}


def configured_rows(value):
    """fleet.json's toolchains as one row per tool version, Node 24 included unless node is false."""
    try:
        tools = runtime.toolchain_config(value if isinstance(value, dict) else {})
    except runtime.Invalid as error:
        raise SetupError(f"fleet.json's toolchains: {error}") from None
    return [{"tool": spec, "version": version, "install": entry["install"], "options": entry["options"]}
            for spec, entry in tools.items() for version in entry["versions"]]


def configured_packages(value):
    try:
        return runtime.toolchains.package_config(value if value is not None else [])
    except runtime.Invalid as error:
        raise SetupError(f"fleet.json's systemPackages: {error}") from None


def source_text(sources):
    """Where a confirmed item came from, in a few words."""
    if not sources:
        return "no recorded source"
    first = sources[0]
    if "by" in first:
        return {"default": "the installer's default", "fleet.json": "fleet.json", "listed": "listed by hand"}.get(
            first["by"], first["by"])
    more = f" and {len(sources) - 1} more" if len(sources) > 1 else ""
    return f"{toolchains.source_label(first)}{more}"


def parse_toolchain_items(value):
    """--toolchains TOOL@VERSION,-TOOL,-TOOL@VERSION as (remove, tool, version or None) triples."""
    items = []
    for raw in (part.strip() for part in value.split(",")):
        if not raw:
            continue
        remove, text = raw.startswith("-"), raw.removeprefix("-")
        tool, version = text, None
        match = re.fullmatch(r"(.+)@([A-Za-z0-9][A-Za-z0-9._+-]*)", text)
        if match and not match.group(1).endswith(":"):
            tool, version = match.groups()
        try:
            runtime.toolchains.tool_name(tool)
            if version is not None:
                runtime.toolchains.tool_version(version, tool)
        except runtime.Invalid as error:
            raise SetupError(f"--toolchains: {error}") from None
        require(remove or version, f"--toolchains: name {tool}'s version, as {tool}@VERSION (such as node@24).")
        items.append((remove, tool, version))
    return items


def parse_package_items(value):
    items = []
    for raw in (part.strip() for part in value.split(",")):
        if not raw:
            continue
        remove, name = raw.startswith("-"), raw.removeprefix("-")
        require(re.fullmatch(toolchains.PACKAGE, name), f"--system-packages: {name!r} is not a Debian package name "
                "apt installs, such as libssl-dev: no version, release, pattern or trailing minus (which apt reads "
                "as a removal).")
        items.append((remove, name))
    return items


def proposed_items(record, kind, name, version=None):
    """The pending proposal's items for a tool version or a package: more than one when repositories ask for the
    same version with different options."""
    proposal = record.get("proposal") or {}
    return [item for item in proposal.get("items", []) if item["kind"] == kind and item["name"] == name
            and item["version"] == version]


def proposed_item(record, kind, name, version=None):
    return next(iter(proposed_items(record, kind, name, version)), None)


def confirm_tool(record, tool, version, today):
    """Add a confirmed tool version to the record, with the proposal's sources and options. mise applies one set
    of options to every version of a tool, so the new version's options are joined with those already confirmed
    for the tool (toolchains.merge_options), and every version gets the result; a disagreement stops setup with
    a message saying what to choose. Returns the line for the operator."""
    proposed = proposed_items(record, "tool", tool, version)
    existing = [item for item in record["tools"] if item["tool"] == tool]
    try:
        options = {}
        for item in proposed:
            options = toolchains.merge_options(options, item["options"], tool)
        merged = options
        for item in existing:
            merged = toolchains.merge_options(item["options"], merged, tool)
    except runtime.Invalid as error:
        raise SetupError(f"--toolchains {tool}@{version}: {error}") from None
    changed = [item for item in existing if item["options"] != merged]
    for item in existing:
        item["options"] = merged
    sources = [source for item in proposed for source in item["sources"]]
    if any(item["version"] == version for item in existing):
        # Confirmed already; a proposal asking for it with other options still adds them.
        line = f"{tool}@{version} is already in the toolchains"
    else:
        install = existing[0]["install"] if existing else proposed[0]["where"] if proposed and proposed[0].get(
            "where") in ("shared", "user") else toolchains.default_install(tool)
        record["tools"].append({"tool": tool, "version": version, "install": install, "options": merged,
                                "sources": list({json.dumps(source, sort_keys=True): source
                                                 for source in sources}.values()) or [{"by": "listed"}],
                                "confirmed": today})
        line = f"confirmed {tool}@{version}" + (f", from {source_text(sources)}" if proposed else ", listed by hand")
    if changed or merged != options:
        line += (f"; mise applies {tool}'s options to every version, so every {tool} version now has "
                 f"{toolchains.options_label(merged)}")
    return line


def hand_edits(record, rows, packages, previous, today):
    """Record the operator's hand edits to fleet.json's toolchains and systemPackages, and return lines saying so.

    A hand edit is a difference between fleet.json now and what setup last wrote into it (the record's
    "generated", saved only once fleet.json is written: write_fleet). Comparing with the record instead would take
    a run that stopped between writing the record and writing fleet.json for hand edits, and undo what that run
    did, such as restoring a tool it had just removed."""
    lines = []
    baseline = record.get("generated")
    if baseline is None:
        # A record from before setup kept what it wrote: fleet.json as the record would have it written.
        baseline = dict(zip(("toolchains", "systemPackages"), toolchain_settings(record, previous)))
    before = {(row["tool"], row["version"]): row for row in configured_rows(baseline.get("toolchains", {}))}
    before_packages = configured_packages(baseline.get("systemPackages"))
    now = {(row["tool"], row["version"]): row for row in rows}
    for key, row in now.items():
        kept = next((item for item in record["tools"] if (item["tool"], item["version"]) == key), None)
        if key not in before:
            if kept is None:
                record["tools"].append({**row, "sources": [{"by": "fleet.json"}], "confirmed": today})
                lines.append(f"recorded {row['tool']}@{row['version']}, which fleet.json lists")
        elif (before[key]["install"], before[key]["options"]) != (row["install"], row["options"]) \
                and kept is not None and (kept["install"], kept["options"]) != (row["install"], row["options"]):
            kept.update(install=row["install"], options=row["options"])
            lines.append(f"recorded {row['tool']}@{row['version']}'s install and options from fleet.json")
    for key in [key for key in before if key not in now]:
        item = next((item for item in record["tools"] if (item["tool"], item["version"]) == key), None)
        if item is not None:
            lines.append(f"kept {item['tool']}@{item['version']} (confirmed {item['confirmed']}, from "
                         f"{source_text(item['sources'])}), which fleet.json no longer lists; "
                         f"'subspace-setup --toolchains=-{item['tool']}@{item['version']}' removes it")
    recorded = {item["package"]: item for item in record["packages"]}
    for name in packages:
        if name not in before_packages and name not in recorded:
            record["packages"].append({"package": name, "sources": [{"by": "fleet.json"}], "confirmed": today})
            lines.append(f"recorded the system package {name}, which fleet.json lists")
    for name in before_packages:
        if name not in packages and name in recorded:
            lines.append(f"kept the system package {name} (confirmed {recorded[name]['confirmed']}), which "
                         f"fleet.json no longer lists; 'subspace-setup --system-packages=-{name}' removes it")
    return lines


def write_fleet(config_path, config):
    """Write fleet.json, then note in the toolchains record the toolchains and systemPackages it was written with:
    the baseline for the next run's hand edits (hand_edits). Each write is atomic; if setup stops between the two,
    the next run sees the difference as setup's own, not the operator's."""
    write_json(config_path, config)
    record = read_json(INPUTS / TOOLCHAIN_RECORD)
    if record is not None:
        record["generated"] = {"toolchains": config.get("toolchains", {}),
                               "systemPackages": config.get("systemPackages", [])}
        write_json(INPUTS / TOOLCHAIN_RECORD, record)


def update_toolchains(args, answers):
    """Bring the recorded toolchains up to date and return what changed, as lines for the operator.

    The record starts from fleet.json (or the installer's default, Node 24), so an existing fleet's toolchains keep
    working; later hand edits to fleet.json are recorded too. --toolchains and --system-packages confirm or remove
    items, a confirmed item taking its sources from the pending proposal. Answering a proposal settles it: its
    repositories count as examined and what was not confirmed is recorded as declined. Nothing confirmed is dropped
    unless this run removes it by name, and nothing is added that nobody named."""
    lines = []
    path = INPUTS / "fleet.json"
    previous = read_json(path, {}) or {}
    rows = configured_rows(previous.get("toolchains", {}))
    packages = configured_packages(previous.get("systemPackages"))
    listed = previous.get("toolchains") if isinstance(previous.get("toolchains"), dict) else {}
    today = time.strftime("%Y-%m-%d", time.gmtime())
    record = read_json(INPUTS / TOOLCHAIN_RECORD)
    if record is None:
        record = blank_record()
        for row in rows:
            by = "fleet.json" if row["tool"] in listed else "default"
            record["tools"].append({**row, "sources": [{"by": by}], "confirmed": today})
        record["packages"] = [{"package": name, "sources": [{"by": "fleet.json"}], "confirmed": today}
                              for name in packages]
        # What fleet.json has now is the baseline for telling hand edits from setup's own writes.
        record["generated"] = {"toolchains": previous.get("toolchains", {}),
                               "systemPackages": previous.get("systemPackages", [])}
        if previous:
            lines.append("recorded the fleet's toolchains from fleet.json: " + (", ".join(
                f"{row['tool']}@{row['version']}" for row in record["tools"]) or "none") + (
                f"; system packages {', '.join(packages)}" if packages else ""))
    elif path.exists():
        lines += hand_edits(record, rows, packages, previous, today)
    confirmed = set()
    if args.toolchains not in (None, "derive", "none"):
        for remove, tool, version in parse_toolchain_items(args.toolchains):
            matching = [item for item in record["tools"] if item["tool"] == tool and version in (None, item["version"])]
            if remove:
                for item in matching:
                    record["tools"].remove(item)
                    lines.append(f"removed {tool}@{item['version']} from the toolchains (it stays installed)")
                if not matching:
                    lines.append(f"{tool}{'@' + version if version else ''} is not in the toolchains")
                continue
            confirmed.add(("tool", tool, version))
            lines.append(confirm_tool(record, tool, version, today))
    if args.system_packages is not None:
        for remove, name in parse_package_items(args.system_packages):
            matching = [item for item in record["packages"] if item["package"] == name]
            if remove:
                for item in matching:
                    record["packages"].remove(item)
                    lines.append(f"took the system package {name} off the list (it stays installed)")
                continue
            confirmed.add(("package", name, None))
            if not matching:
                proposed = proposed_item(record, "package", name)
                record["packages"].append({"package": name, "sources": proposed["sources"] if proposed
                                           else [{"by": "listed"}], "confirmed": today})
                lines.append(f"confirmed the system package {name}" + (
                    f", from {source_text(proposed['sources'])}" if proposed else ", listed by hand"))
    proposal = record.get("proposal")
    if proposal and (args.toolchains not in (None, "derive") or args.system_packages is not None):
        if args.toolchains == "none":
            lines.append(f"dropped the unanswered toolchain proposal from {', '.join(proposal['repositories'])}")
        else:
            declined = [item for item in proposal["items"] if not item.get("configured")
                        and (item["kind"], item["name"], item["version"]) not in confirmed]
            for item in declined:
                kind = "tools" if item["kind"] == "tool" else "packages"
                record["declined"][kind].append({"name": item["name"], "version": item["version"],
                                                 "sources": item["sources"], "declined": today})
            record["examined"] = list(dict.fromkeys([*record["examined"], *proposal["repositories"]]))
            lines.append(f"answered the toolchain proposal from {', '.join(proposal['repositories'])}"
                         + (f"; not confirmed, so recorded as declined: "
                            f"{', '.join(toolchains.item_label(item) for item in declined)}" if declined else ""))
        record["proposal"] = None
    write_json(INPUTS / TOOLCHAIN_RECORD, record)
    return record, lines


def toolchain_settings(record, previous):
    """fleet.json's toolchains and systemPackages, from the record (before one exists, as fleet.json has them)."""
    if record is None:
        return previous.get("toolchains", dict(toolchains.DEFAULT_TOOLS)), list(previous.get("systemPackages", []))
    tools = {}
    for row in record["tools"]:
        entry = tools.setdefault(row["tool"], {"versions": [], "install": row["install"], "options": {}})
        entry["versions"].append(row["version"])
        try:
            # Confirming keeps one set of options per tool; a record edited by hand is checked the same way.
            entry["options"] = toolchains.merge_options(entry["options"], row["options"], row["tool"])
        except runtime.Invalid as error:
            raise SetupError(f"{INPUTS / TOOLCHAIN_RECORD}: {error}") from None
    result = {} if "node" in tools else {"node": False}
    for spec, entry in tools.items():
        versions = entry["versions"][0] if len(entry["versions"]) == 1 else entry["versions"]
        if entry["install"] == toolchains.default_install(spec) and not entry["options"]:
            result[spec] = versions
        else:
            result[spec] = {"version": versions,
                            **({"install": entry["install"]} if entry["install"] != toolchains.default_install(spec)
                               else {}),
                            **({"options": entry["options"]} if entry["options"] else {})}
    return result, [row["package"] for row in record["packages"]]


def propose_toolchains(config, record, everything=False):
    """Detect what the repositories need, from those not examined yet (every one with everything), as an annotated
    proposal; None when there is no such repository."""
    repositories = [row["id"] for row in config.get("repositories", []) if isinstance(row, dict) and "id" in row]
    examined = set() if everything else set(record.get("examined", []))
    new = [repository for repository in repositories if repository not in examined]
    if not new:
        return None
    rear = config["rearAdmiral"]["id"]
    proposal = toolchains.detect({repository: REFERENCES / rear / repository for repository in new})
    try:
        tools = runtime.toolchain_config(config.get("toolchains", {}))
    except runtime.Invalid as error:
        raise SetupError(f"fleet.json's toolchains: {error}") from None
    homes = [AGENT_HOMES / person["id"] for person in [config["rearAdmiral"], *config.get("captains", [])]]
    return toolchains.annotate(proposal, tools, config.get("systemPackages", []), homes)


def uncloned(proposal):
    """The proposal's repositories that are not cloned on this VM yet."""
    return [entry["repository"] for entry in proposal["unknown"] if not entry["file"]]


def record_proposal(record, proposal):
    """Keep a proposal for the person to answer. One with nothing to confirm (everything configured already, or
    named only by documentation, which the confirming command leaves out) is answered at once: its repositories
    count as examined, and its documentation-only items are recorded as declined, as when a person answers."""
    cloned = [repository for repository in proposal["repositories"] if repository not in uncloned(proposal)]
    proposal = {**proposal, "repositories": cloned}
    if toolchains.confirmable(proposal["items"]):
        record["proposal"] = proposal
    else:
        today = time.strftime("%Y-%m-%d", time.gmtime())
        for item in proposal["items"]:
            if not item.get("configured"):
                kind = "tools" if item["kind"] == "tool" else "packages"
                record["declined"][kind].append({"name": item["name"], "version": item["version"],
                                                 "sources": item["sources"], "declined": today})
        record["examined"] = list(dict.fromkeys([*record["examined"], *cloned]))
        record["proposal"] = None
    write_json(INPUTS / TOOLCHAIN_RECORD, record)


def show_proposal(proposal, save, record=None):
    """Print a proposal; with save, record it for the person to answer."""
    if uncloned(proposal) == proposal["repositories"]:
        say("the repositories are not cloned on this VM yet: setup proposes their toolchains after the install "
            "clones them")
        return
    say("toolchain proposal" + ("" if save else " (dry run: not recorded)") + ":")
    print("\n".join(toolchains.proposal_lines(proposal)), flush=True)
    if save:
        record_proposal(record, proposal)


def propose_safely(config, record, save):
    """Detect and show (with save, record) a proposal from the repositories not examined yet. Repositories are less
    trusted than the operator, and detection reads them: whatever goes wrong here is reported, and setup carries on
    to the lockdown."""
    try:
        if proposal := propose_toolchains(config, record):
            show_proposal(proposal, save, record)
    except Exception as error:  # Detection never stops setup, so it never leaves a fleet without its lockdown.
        say(f"WARNING: the toolchain proposal failed ({type(error).__name__}: {error}); setup carries on without it. "
            "Run 'sudo subspace-setup toolchains detect' to try again.")


def record_lines(record):
    """'subspace-setup toolchains': the recorded toolchains, with their sources, and what waits for an answer."""
    lines = ["Toolchains, as confirmed:"]
    lines += [f"  {row['tool']}@{row['version']}: {toolchains.where_label(row['install'])}"
              + (f", options {row['options']}" if row["options"] else "")
              + f"; confirmed {row['confirmed']}, from {source_text(row['sources'])}" for row in record["tools"]] \
        or ["  none"]
    if record["packages"]:
        lines.append("System packages, as confirmed:")
        lines += [f"  {row['package']}: confirmed {row['confirmed']}, from {source_text(row['sources'])}"
                  for row in record["packages"]]
    declined = [*record["declined"]["tools"], *record["declined"]["packages"]]
    if declined:
        lines.append("Declined (not proposed again; 'subspace-setup toolchains detect --all' proposes them again): "
                     + ", ".join(f"{row['name']}" + (f"@{row['version']}" if row["version"] else "")
                                 for row in declined))
    lines.append("Repositories examined: " + (", ".join(record["examined"]) or "none"))
    if record.get("proposal"):
        lines.append("Waiting for an answer:")
        lines += toolchains.proposal_lines(record["proposal"])
    return lines


def toolchains_command(argv):
    """subspace-setup toolchains [detect [--all]] [--json]"""
    parser = argparse.ArgumentParser(prog="subspace-setup toolchains", description="Show the fleet's recorded "
                                     "toolchains, or propose what its repositories need (detect).")
    parser.add_argument("action", nargs="?", choices=["show", "detect"], default="show")
    parser.add_argument("--all", action="store_true", help="detect: read every repository again, not only those "
                        "added since the last answered proposal, and propose again what was declined")
    parser.add_argument("--json", action="store_true", help="print JSON")
    args = parser.parse_args(argv)
    config = read_json(INPUTS / "fleet.json")
    require(config, "Set the fleet up first: subspace-setup has no fleet.json on this VM.")
    record = read_json(INPUTS / TOOLCHAIN_RECORD)
    if args.action == "show":
        require(record is not None, "No toolchains are recorded yet: the next 'subspace-setup' run records "
                "fleet.json's.")
        print(json.dumps(record, indent=2) if args.json else "\n".join(record_lines(record)))
        return
    require(os.geteuid() == 0, "Run 'subspace-setup toolchains detect' as root: it reads the repositories the rear "
            "admiral's reference checkouts hold.")
    record = record or blank_record()
    proposal = propose_toolchains(config, record, args.all)
    if proposal is None:
        say("every repository has been examined: nothing new to propose ('--all' reads them all again)")
        return
    if args.json:
        print(json.dumps(proposal, indent=2))
        record_proposal(record, proposal)
        return
    show_proposal(proposal, True, record)


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


def local_irc_tls():
    """Keep a server certificate for the TLS loopback listener and trust it in Stargate."""
    target = INPUTS / "irc-tls"
    if target.exists():
        require((target / "cert.pem").is_file() and (target / "key.pem").is_file(),
                "Local IRC TLS needs both its saved certificate and key.")
        return
    with tempfile.TemporaryDirectory(prefix="irc-tls-", dir=INPUTS) as scratch:
        source = Path(scratch) / "tls"
        source.mkdir(mode=0o700)
        run("openssl", "req", "-x509", "-newkey", "rsa:3072", "-nodes", "-sha256", "-days", "3650",
            "-subj", "/CN=127.0.0.1", "-addext", "subjectAltName=IP:127.0.0.1",
            "-out", source / "cert.pem", "-keyout", source / "key.pem", capture=True)
        (source / "key.pem").chmod(0o600)
        source.rename(target)


def installer(config_path, dry_run=False, secrets_dir=None, quiet=False):
    """Run the fleet installer. quiet keeps its plan to itself, as a check whose plan would mislead does; what it
    refuses still reaches the terminal."""
    args = [INSTALLER / "install.sh", "--config", config_path]
    if secrets_dir:
        args += ["--secrets-dir", secrets_dir]
    if dry_run:
        args.append("--dry-run")
    say("running the fleet installer" + (" (dry run)" if dry_run else ""))
    run(*args, capture=quiet)


def preflight(config, private, before="the API changes", quiet=False):
    """Check the fleet's inputs as the installer will (configuration, credentials, the secrets directory and
    pinned sessions), before setup changes the API or its database. A run the installer would refuse then
    migrates nothing and restarts nothing."""
    with tempfile.TemporaryDirectory(prefix="subspace-setup-check-") as scratch:
        config_path = Path(scratch) / "fleet.json"
        write_json(config_path, config)
        write_json(Path(scratch) / "fleet.secrets.json", private)
        secrets_dir = planned_secrets(config, Path(scratch))
        say(f"checking the fleet's configuration before {before}")
        installer(config_path, dry_run=True, secrets_dir=secrets_dir, quiet=quiet)


def planned_secrets(config, scratch):
    """A local API issues the real enrolment token only after its fleet is registered."""
    source = INPUTS / "fleet.secrets"
    if not local_signed_api(config):
        return source if source.is_dir() else None
    planned = scratch / "planned-secrets"
    if source.is_dir():
        shutil.copytree(source, planned, symlinks=True)
    else:
        planned.mkdir(mode=0o700)
    token = planned / runtime.ENROLMENT_TOKEN
    if not token.exists() and not token.is_symlink():
        write_private(token, PLANNED_ENROLMENT_TOKEN + "\n")
    return planned


def stargate_release(answers):
    """The Stargate a run installs, as (tag, commit): the pinned --stargate-ref, else the newest release."""
    if answers.get("stargateRef"):
        return "pinned", answers["stargateRef"]
    return latest_stargate()


def dispatch_uses_api(answers):
    return bool(answers.get("agentDispatch") or answers.get("decisionOutcomeDms"))


def new_fleet(saved):
    return not saved and not (INPUTS / "fleet.json").exists() and not (FLEET_ETC / "fleet.json").exists()


def same_remote_irc(answers, saved, installed):
    """Compare the selected IRC connection to the effective settings of the last install."""
    prior = installed.get("irc") or {}
    previous_auth = (prior.get("auth", "certificate") if prior.get("mode") == "remote"
                     else saved.get("ircAuth", "password"))
    return (prior.get("mode", saved.get("irc")) == "remote"
            and prior.get("host", saved.get("ircHost")) == answers.get("ircHost")
            and prior.get("port", saved.get("ircPort", IRC_TLS)) == answers.get("ircPort")
            and previous_auth == answers.get("ircAuth"))


def configure_decision_outcome_dms(answers, rear, ref):
    """Resolve the saved preference against local API and Stargate capabilities before any writes."""
    mode = answers["decisionOutcomeMode"]
    answers.pop("decisionOutcomeDms", None)
    if mode == "off" or (mode == "auto" and answers.get("irc") != "local"):
        return
    supported = (answers["api"] and not answers.get("apiUrl")
                 and local_signed_api({"api": planned_api_block(answers)})
                 and (PAYLOAD / "subspace-api/src/decision-outcomes.ts").is_file())
    if not supported:
        require(mode == "auto", "Decision outcome DMs need the bundled Subspace API and signed officer access. "
                "Use the local API with signed access, or pass --no-decision-outcome-dms.")
        return
    if rear_harness_type(rear) != "opencode":
        require(mode == "auto",
                "Decision outcome DMs need an OpenCode rear admiral. Pass --no-decision-outcome-dms to opt out.")
        return
    try:
        runtime.require_decision_dm_source(STARGATE_REPO, ref)
    except runtime.Invalid as error:
        if mode == "on" or isinstance(error, runtime.StargateSourceUnavailable):
            raise SetupError(str(error)) from None
        say(f"Leaving decision outcome DMs off. {error}")
        return
    answers["decisionOutcomeDms"] = True


def rear_harness_type(rear):
    # The installed fleet's harness types stay in effect when setup changes models or effort.
    previous = read_json(INPUTS / "fleet.json", {})
    harness = previous.get("harness") or {}
    officer = previous.get("rearAdmiral") or {}
    role_type = (harness.get("rearAdmiral") or {}).get("type", harness.get("type", "opencode"))
    return (officer.get("harness") or {}).get("type", role_type) if officer.get("id") == rear else role_type


def check_usage_guard_source(answers, rear, ref):
    previous = read_json(INPUTS / "fleet.json", {})
    try:
        runtime.require_peer_message_source({
            "captainPeerMessages": answers.get("captainPeerMessages", previous.get("captainPeerMessages")),
            "stargate": {"repo": STARGATE_REPO, "ref": ref},
        })
        if rear_harness_type(rear) == "opencode":
            runtime.require_usage_guard_source({
                "usageGuard": answers["usageGuard"], "stargate": {"repo": STARGATE_REPO, "ref": ref},
            })
    except runtime.Invalid as error:
        raise SetupError(str(error)) from None


def check_officer_tool_source(answers, members, ref):
    # Build the same capabilities as the actual fleet using only public settings.
    planned = fleet_config(answers, members, '0' * 40, planned_api_block(answers), stargate_ref=ref)
    try:
        dispatch = planned.get("agentDispatch", {})
        if dispatch.get("enabled") and (answers.get("apiUrl") or dispatch.get("readyOffers") is False
                                        or dispatch.get("assignmentRequests") is False):
            runtime.require_assignment_requests_source(ref, STARGATE_REPO)
        api = dict(planned.get("api") or {})
        if api.get("agentUrl"):
            api["agentMcpUrl"] = api["agentUrl"]
        runtime.require_officer_tool_sources({
            **planned, "stargate": {**planned["stargate"], "repo": STARGATE_REPO},
            "repositories": runtime.repository_config(planned["repositories"]),
            "integrations": runtime.integrations_config(planned),
            "api": api, "members": [planned["rearAdmiral"], *planned["captains"]],
        })
    except runtime.Invalid as error:
        raise SetupError(str(error)) from None


def run_inputs(answers, members, secret_store, stargate_ref, irc_ref, api_block=None, stargate_api=False, token="",
               history=None, dispatch_token=""):
    """The fleet installer's inputs, fleet.json and its secrets, as a run writes them."""
    config = fleet_config(answers, members, irc_ref, api_block, stargate_api, stargate_ref, history)
    admin_token = "" if local_signed_api(config) else token
    private = fleet_secrets(secret_store, members, answers["admin"], admin_token, answers["irc"] == "local",
                            certificates=answers.get("ircAuth") == "certificate",
                            dispatch_token=dispatch_token if api_block else "")
    if history:
        private["ircHistoryPassword"] = secret_store.password("ergo-history")
    return config, private


def planned_run_inputs(answers, members, secret_store, stargate_ref):
    """The inputs a real run writes, as far as is known before the API runs, for the installer's check: the
    API settings it will have (planned_api_block), with stand-ins for the tokens it will get."""
    block = planned_api_block(answers)
    legacy_dispatch = block and answers.get("agentDispatch") and not answers.get("decisionOutcomeDms")
    dispatch = PLANNED_TOKENS["rearAdmiralApiToken"] if legacy_dispatch else ""
    return run_inputs(answers, members, secret_store, stargate_ref, hashlib.sha1(b"dry-run").hexdigest(), block,
                      False, PLANNED_TOKENS["apiToken"] if block else "", dispatch_token=dispatch)


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
    api_env = INPUTS / "api.env"
    if api_env.is_file() and SIGNED_LOCAL_API in api_env.read_text().splitlines():
        # Compose gives the caller's environment precedence over --env-file.
        env = {**(os.environ if env is None else env), "AGENT_TOKEN_AUDIENCE": local_api_origin()}
    # Docker Compose passes its standard input to 'exec' and 'run' commands, which would otherwise read on from
    # setup's own, so it gets none unless it has input of its own.
    return run("docker", "compose", "--project-name", API_PROJECT, "--project-directory", API_DIR,
               "--env-file", api_env, "-f", API_DIR / "compose.yaml", "-f", INPUTS / "api.override.yaml",
               *args, capture=capture, check=check, env=env, input_text=input_text, keep_stdin=False)


def loopback_irc_host(host):
    if host.lower().rstrip(".").split(".")[-1] == "localhost":
        return True
    try:
        address = ip_address(host.strip("[]"))
    except ValueError:
        try:
            address = ip_address(socket.inet_aton(host))
        except OSError:
            return False
    return (getattr(address, "ipv4_mapped", None) or address).is_loopback


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


# Names -----------------------------------------------------------------------

def hosted_call(origin, token, method, path, body=None):
    """One request to a hosted API under /api/v1 with a bearer token, no redirects followed; errors carry the
    status and the API's detail (ApiError)."""
    data = None if body is None else json.dumps(body).encode()
    request = Request(f"{origin}/api/v1{path}", data=data, method=method, headers={
        "Authorization": f"Bearer {token}", "Accept": "application/json", "Content-Type": "application/json"})
    try:
        with build_opener(NoRedirect).open(request, timeout=30) as response:
            text = response.read().decode()
    except HTTPError as error:
        detail = problem_detail(error)
        error.close()
        raise ApiError(f"The hosted Subspace API returned HTTP {error.code} for {method} {path}.", error.code,
                       detail) from None
    except (URLError, OSError):
        raise SetupError(f"Could not reach the hosted Subspace API at {origin} for {method} {path}.") from None
    return json.loads(text) if text else {}


def officer_records(answers, members):
    """Each officer as the API knows it with this fleet's naming: (rank, agent name, IRC nick, ship)."""
    plain = answers.get("officerNames") == "plain"
    rows = []
    for index, person in enumerate(members):
        rank = "rearAdmiral" if index == 0 else "captain"
        name = person["account"] if plain or index else person["name"]
        rows.append((rank, name, person["account"], None if index == 0 else person["name"]))
    return rows


def stored_enrolment_token():
    try:
        return (INPUTS / "fleet.secrets" / runtime.ENROLMENT_TOKEN).read_text().strip() or None
    except OSError:
        return None


def hosted_name_problems(answers, members, token):
    """With the fleet's enrolment token, ask the hosted API what enrolling each officer would do (POST
    /api/v1/agents/enrol/check), which uses nothing and changes nothing. Returns the refusals; says why when it
    cannot ask."""
    token = token or stored_enrolment_token()
    if not token:
        say("names: no enrolment token, so the hosted API was not asked about the officers' names")
        return []
    agents = [{"name": name, "rank": rank, "ircNick": nick, **({"ship": ship} if ship else {})}
              for rank, name, nick, ship in officer_records(answers, members)]
    try:
        items = hosted_call(answers["apiUrl"], token, "POST", "/agents/enrol/check", {"agents": agents})["items"]
    except ApiError as error:
        if error.status == 404:
            say("names: this hosted API predates POST /api/v1/agents/enrol/check, so it was not asked about the "
                "officers' names; enrolment still refuses a taken name")
        elif error.status == 401:
            say(f"names: the hosted API did not accept the enrolment token ({error.detail or 'refused'}), so it was "
                "not asked about the officers' names. A new officer needs a fresh token "
                "(--api-enrolment-token-file) anyway")
        else:
            raise
        return []
    except (KeyError, TypeError):
        raise SetupError("The hosted Subspace API answered the name check with an unexpected body.") from None
    except SetupError as error:
        say(f"names: {error} The officers' names were not checked; enrolment still refuses a taken name")
        return []
    problems = [f"{item.get('name')}: {item.get('detail')}" for item in items if item.get("outcome") == "refused"]
    if not problems:
        say("names: the hosted API would enrol every officer under its name ("
            + ", ".join(f"{item.get('name')} {item.get('outcome')}" for item in items) + ")")
    return problems


def api_reader(answers):
    """The admin's development token for reading the running API on this VM, signing in only as someone already
    there, or None when that fails."""
    try:
        return api_token(answers, existing_only=True)
    except SetupError:
        return None


def local_name_problems(answers, members, token, own=None):
    """With the API on this VM (an administrator's token): officers whose name or IRC nick another agent, or a
    person as their username, holds. own: {officer ID: nicks its own record may have now}, beside its new nick."""
    if not token:
        say("names: could not sign in to the API on this VM to read its agents; registration still refuses a taken "
            "name")
        return []
    call = lambda method, path, body: api_call(token, method, path, body)  # noqa: E731
    try:
        rows, people = fleet_records.agents(call), fleet_records.usernames(call)
    except SetupError as error:
        say(f"names: could not list the API's agents and people ({error}); registration still refuses a taken name")
        return []
    officers = [(name, nick, {nick, *(own or {}).get(person["id"], ())})
                for (_, name, nick, _), person in zip(officer_records(answers, members), members, strict=True)]
    problems = fleet_records.taken_names(rows, answers["slug"], officers, people)
    if not problems:
        say("names: no other agent or person in the Subspace API on this VM has an officer's name or IRC nick")
    return problems


def irc_name_problems(answers, members, certificates, operator, staged=None, connect_with=()):
    """On a remote IRC server with certificate sign-in: officers whose account NickServ knows as another's.
    An account counts as the officer's own when the officer's certificate (or, during a rename, its new staged
    one) signs in to it. Read-only. Signs in, for a server that admits only signed-in connections, with the
    operator's account when given, else with an officer certificate that already signs in (connect_with: more
    bundles to try, such as the officers' current ones during a rename)."""
    if answers.get("irc") != "remote" or answers.get("ircAuth") != "certificate":
        return []
    host, port, ca = answers["ircHost"], answers["ircPort"], answers.get("ircCa")
    bundles = {person["account"]: path for person in members
               for path in [(staged or {}).get(person["id"]) or (certificates or {}).get(person["account"], (None,))[0]]
               if path and Path(path).is_file()}
    signed_in = {}
    for account, path in bundles.items():
        try:
            signed_in[account] = irc_accounts.check_login(host, port, ca, str(path))
        except irc_accounts.IrcError:
            signed_in[account] = None
    account = operator[2] if operator else None
    client = None if account else next((str(bundles[name]) for name, as_ in signed_in.items() if as_), None)
    for path in connect_with if not (account or client) else ():
        try:
            if Path(path).is_file() and irc_accounts.check_login(host, port, ca, str(path)):
                client = str(path)
                break
        except irc_accounts.IrcError:
            continue
    try:
        taken = irc_accounts.accounts_taken(host, port, ca, [person["account"] for person in members], client, account)
    except irc_accounts.IrcError as error:
        say(f"names: could not ask {host}'s NickServ about the officers' accounts ({error}); registration there still "
            "refuses a taken account")
        return []
    # An account an earlier run registered for this very certificate, and stopped before binding, is the officer's.
    registered = registered_accounts()
    problems = []
    for person in members:
        name = person["account"]
        holder = taken.get(name)
        if not holder:
            continue
        signs_in = (signed_in.get(name) or "").lower() == name.lower()
        fingerprint = registered.get(name) if holder.lower() == name.lower() and not signs_in else None
        if fingerprint and fingerprint != bundle_fingerprint(bundles.get(name)):
            fingerprint = None
        if fingerprint:
            say(f"names: {name} on {host} was registered by an earlier run that stopped before binding its "
                f"certificate; --irc-operator finishes it, or an IRC operator runs: /NS CERT ADD {name} {fingerprint}")
        elif not (holder.lower() == name.lower() and signs_in):
            problems.append(f"{name} is taken on {host}: NickServ knows it as account {holder}, which the officer's "
                            "certificate does not sign in to")
    if not problems:
        say(f"names: {host}'s NickServ has none of the officers' accounts under another owner")
    return problems


def bundle_fingerprint(path):
    """The SHA-256 fingerprint of a certificate bundle's certificate, or None when there is no valid bundle."""
    try:
        return runtime.irc_fingerprint(Path(path).read_text(), str(path)) if path else None
    except (OSError, UnicodeDecodeError, runtime.Invalid):
        return None


def require_names(problems):
    require(not problems, "An officer's name is taken: " + "; ".join(problems) + ". Each officer's name is its IRC "
            "account and its agent name in the Subspace API, which must be unique on the IRC server and in the "
            "API's organisation. Pick another name for that officer (--rear-admiral, --captains), or free the name "
            "there, then run setup again. Nothing was changed.")


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


def dev_sign_in(environment):
    """Whether api.env's settings offer development sign-in, which setup uses to act as the admin."""
    providers = [value.strip() for value in environment.get("AUTH_PROVIDERS", "").split(",")]
    return environment.get("ENABLE_DEV_AUTH") == "true" and "dev" in providers


def read_api_settings():
    previous = INPUTS / "api.env"
    settings = {}
    if previous.exists():
        # Keep what the operator set, such as WorkOS or Entra and turning development sign-in off.
        for line in previous.read_text().splitlines():
            key, _, value = line.partition("=")
            if key and not key.startswith("#"):
                settings[key] = value
    return settings


def api_environment(answers, environment, external=False):
    """api.env as api_up writes it: environment (setup's own settings) over what an earlier run wrote there and
    the operator kept or changed. Returns the settings and whether the agent listener is on.

    external says the API uses an existing PostgreSQL, whose URL api_up passes in environment.
    """
    kept = {key: value for key, value in read_api_settings().items() if key not in environment}
    if not external:
        # The API's own PostgreSQL: never a URL an earlier run, or anyone, left in api.env, which Compose would
        # prefer to the local database and the image would migrate on start.
        kept.pop("DATABASE_URL", None)
    merged = {**environment, **DEV_AUTH, "AGENT_PORT": "", **kept}
    requested = answers.get("agentListener")
    if requested is True:
        require(dev_sign_in(merged) and merged.get("NODE_ENV") != "production",
                "--agent-listener requires development sign-in outside production.")
        merged["AGENT_PORT"] = AGENT_API_PORT
    elif requested is False:
        merged["AGENT_PORT"] = ""
    organisation = merged.get("AGENT_ORGANISATION", "")
    if (not organisation and merged["AGENT_PORT"]) or re.fullmatch(r"dev:[a-z0-9][a-z0-9-]{0,62}", organisation):
        # The installer's own form follows the fleet's development sign-in organisation.
        merged["AGENT_ORGANISATION"] = f"dev:{answers['organisation']}"
    if not dev_sign_in(merged) and merged.get("AGENT_ORGANISATION", "").startswith("dev:"):
        # The listener answers for development sign-in's organisation and trusts the network as it does:
        # an operator who turns development sign-in off turns it off too.
        merged["AGENT_PORT"] = ""
        merged.pop("AGENT_ORGANISATION", None)
    if merged.get("NODE_ENV") == "production":
        merged["AGENT_PORT"] = ""
    listener = str(merged.get("AGENT_PORT", "")) == str(AGENT_API_PORT)
    if signed_local_access(answers, merged, listener):
        audience = merged.get("AGENT_TOKEN_AUDIENCE", "").strip()
        origin = re.escape(local_api_origin())
        # Accept the canonical origin in Compose's quoted or unquoted form, with an optional comment.
        require(not audience or re.fullmatch(rf"""(['"]?){origin}\1(?:\s+#.*)?""", audience),
                "Signed officer access on this VM requires AGENT_TOKEN_AUDIENCE to match the local API origin.")
        merged["AGENT_TOKEN_AUDIENCE"] = local_api_origin()
    return merged, listener


def local_api_origin():
    return f"http://{LOOPBACK}:{API_PORT}"


def local_api_block(answers, initiatives, listener):
    """The fleet installer's api block for the API on this VM, which setup registers with as its admin."""
    block = {"url": local_api_origin(), "initiatives": initiatives}
    if answers.get("vmHost"):
        block["vmHost"] = answers["vmHost"]  # The fleet's record in the API names the VM.
    if listener:
        # Officers get the API's MCP server at the agent listener, as their own agent records.
        block["agentUrl"] = f"http://{LOOPBACK}:{AGENT_API_PORT}"
    return block


def local_signed_api(config):
    api = config.get("api") or {}
    return bool(api.get("enrolment") and urlsplit(api["url"]).hostname == LOOPBACK)


def signed_local_access(answers, environment, listener):
    """Default new fleets to signed access and keep earlier operator choices."""
    if listener or not dev_sign_in(environment):
        return False
    previous = INPUTS / "api.env"
    return (answers.get("agentListener") is False or not previous.is_file()
            or SIGNED_LOCAL_API in previous.read_text().splitlines()
            or local_signed_api(read_json(INPUTS / "fleet.json", {})))


def planned_api_block(answers):
    """The api block a real run writes, as far as is known before the API runs: for the dry run, and the check
    before the API changes, so the installer previews the officers' tool policy with the API's MCP tools a real
    run gives them. None when a real run writes none: no API on this VM, or one without development sign-in."""
    if not answers.get("api"):
        return None
    environment, listener = api_environment(answers, {})
    if not dev_sign_in(environment):
        return None
    block = local_api_block(answers, [answers["initiativeId"]] if answers.get("initiativeId") else [], listener)
    if signed_local_access(answers, environment, listener):
        block["enrolment"] = True
    return block


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
    merged, listener = api_environment(answers, environment, external=bool(external))
    irc_volumes = api_irc_hook(answers, merged, secret_store)
    signing_marker = SIGNED_LOCAL_API + "\n" if signed_local_access(answers, merged, listener) else ""
    write_private(INPUTS / "api.env", signing_marker + "".join(
        f"{key}={value}\n" for key, value in merged.items()))
    if external:
        ca = ""
        if answers.get("databaseCaFile"):
            # A CA certificate is public: the API's own user reads this copy through a read-only mount. It lives
            # beside API_DIR, which every run replaces, and is rewritten in place so the mount keeps its file.
            ca = hashlib.sha256(Path(answers["databaseCaFile"]).read_bytes()).hexdigest()
            shutil.copyfile(answers["databaseCaFile"], database_ca_copy())
            os.chmod(database_ca_copy(), 0o644)
        write_private(INPUTS / "api.override.yaml", external_override(listener, ca, irc_volumes))
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
        *(["    volumes:", *irc_volumes] if irc_volumes else []),
        "",
    ]))
    say("building and starting the Subspace API and PostgreSQL (loopback only)")
    compose("up", "--detach", "--wait", "--wait-timeout", "300", "--build")
    return dev_auth_enabled()


def database_ca_copy():
    return API_DIR.with_name("database-ca.pem")


def automatic_irc_hook(path):
    return path.is_file() and f"IRC_AUTH_HOOK_TOKEN_FILE={API_IRC_MOUNT}/hook-token" in path.read_text().splitlines()


def api_irc_hook(answers, environment, secret_store):
    """Wire the private link from local Ergo to a development API that setup manages."""
    file_settings = {"IRC_CREDENTIAL_SECRET_FILE": "credential-secret", "IRC_AUTH_HOOK_TOKEN_FILE": "hook-token"}
    managed = {
        **{setting: f"{API_IRC_MOUNT}/{name}" for setting, name in file_settings.items()},
        "IRC_AUTH_ORGANISATION": f"dev:{answers['organisation']}",
        "IRC_AUTH_SERVERS": f"{LOOPBACK}:{IRC_PLAIN},{LOOPBACK}:{IRC_TLS}",
    }
    configured = {key: environment[key] for key in API_IRC_HOOK_SETTINGS if environment.get(key)}
    if answers.get("irc") == "local" and dev_sign_in(environment):
        if not configured:
            environment.update(managed)
    elif configured == managed:
        for key in managed:
            del environment[key]

    directory = INPUTS / "api-irc-auth"
    files = {name: directory / name for setting, name in file_settings.items()
             if environment.get(setting) == f"{API_IRC_MOUNT}/{name}"}
    if not files:
        return []

    directory.mkdir(mode=0o700, exist_ok=True)
    os.chmod(directory, 0o700)
    for name, path in files.items():
        if not path.is_file():
            write_private(path, secret_store.password(f"irc-{name}") + "\n")
        # Containers run under distinct UIDs. The owner-only host directory protects both files.
        os.chmod(path, 0o444)
    return [f"      - {json.dumps(f'{path}:{API_IRC_MOUNT}/{name}:ro')}" for name, path in files.items()]


def external_override(listener, ca, irc_volumes=()):
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
        *(["    environment:", f"      NODE_EXTRA_CA_CERTS: {DATABASE_CA}", f"      SUBSPACE_DATABASE_CA_SHA256: {ca}"]
          if ca else []),
        *(["    volumes:", *([f'      - "{database_ca_copy()}:{DATABASE_CA}:ro"'] if ca else []), *irc_volumes]
          if ca or irc_volumes else []),
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


def api_healthy():
    """Whether the API on this VM answers GET /health, so a dry run can read it without starting it."""
    try:
        with urlopen(Request(f"http://{LOOPBACK}:{API_PORT}/health"), timeout=5) as response:
            return json.load(response).get("status") == "ok"
    except (HTTPError, URLError, OSError, ValueError):
        return False


def dev_auth_enabled():
    """Whether the API still offers development sign-in, which setup uses to act as the admin."""
    environment = {}
    for line in (INPUTS / "api.env").read_text().splitlines():
        key, _, value = line.partition("=")
        environment[key] = value
    return dev_sign_in(environment)


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


def api_token(answers, email=None, existing_only=False):
    # In a database setup did not create, sign in only as someone already there (subspace-api's auth:dev), so a
    # mistyped --organisation or --admin-email never adds an organisation or a person. An empty one is exempt.
    existing = ["-e", "DEV_TOKEN_EXISTING_ONLY=true"] if external_database(answers) or existing_only else []
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


def bootstrap_local_owner(answers):
    """SUB-76 seam: currently obtain the administrator token through development sign-in only."""
    # The first development sign-in to a new organisation becomes its administrator.
    token = api_token(answers)
    if external_database(answers):
        existing_administrator(answers, token)
    return token


def api_call(token, method, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    request = Request(f"http://{LOOPBACK}:{API_PORT}/api/v1{path}", data=data, method=method, headers={
        "Authorization": f"Bearer {token}", "Accept": "application/json", "Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=30) as response:
            text = response.read().decode()
    except HTTPError as error:
        detail = problem_detail(error)
        error.close()
        raise ApiError(f"The Subspace API returned HTTP {error.code} for {method} {path}.", error.code, detail) \
            from None
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


def fleet_initiative(token, name, slug, known=None, exclude=(), adopt=False, hint=""):
    """Create, or find, a fleet's initiative; None when the one setup recorded now belongs to another fleet.

    known is the initiative setup recorded as the fleet's. With adopt, --initiative named it on this run: it is
    the fleet's whichever fleet has it now, and adopt_initiative moves it (configure has checked that it exists).
    Otherwise it stands while it belongs to no fleet or to this one (slug). One that has gone to another fleet
    since stays there, as only an explicit adoption moves an initiative: setup warns, adding hint, and registers
    none. One that is gone is replaced by one found by name among initiatives that belong to no fleet or to this
    one, else created. Setup lists no channels on it: an initiative's channels are its own, beyond its fleet's
    standard channels, which the fleet and its ships record (fleet_records) and the desktop joins.
    """
    mine = {row["id"]: row for row in api_call(token, "GET", "/me/initiatives").get("items", [])}
    row = initiative_row(token, known) if known else None
    require(row or not adopt, f"There is no initiative {known} to adopt.")
    if row and not adopt and row.get("fleetId") and (row.get("fleet") or {}).get("slug") != slug:
        say(f"WARNING: initiative {row.get('name')} ({known}), recorded as fleet {slug}'s, now belongs to "
            f"{fleet_label(row)}. Setup leaves it there and registers no initiative for {slug} on this run: only an "
            f"explicit adoption moves one. To move it back, run 'sudo subspace-fleetctl assign {known} {slug}'"
            f"{hint}.")
        return None
    found = known if row else None
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
            say(f"the fleet's initiative {found} does not list you as a member; add yourself in the desktop"
                if found == known else
                f"found an initiative named {name} that you are not a member of; add yourself in the desktop")
        return found
    me = api_call(token, "GET", "/me")
    return api_call(token, "POST", "/initiatives", {
        "name": name, "ownerId": me["userId"], "status": "active",
        "description": f"Work for the {name} fleet. Talk to the rear admiral in {standard_channels.starbase(slug)}.",
        "channelConfiguration": {"memberUserIds": [me["userId"]]},
    })["id"]


def initiative_row(token, initiative):
    """The initiative as the API on this VM has it, or None when there is none with that ID."""
    try:
        return api_call(token, "GET", f"/initiatives/{initiative}")
    except ApiError as error:
        if error.status == 404:
            return None
        raise


def fleet_label(row):
    """The fleet an initiative row belongs to, for a message."""
    other = row.get("fleet") or {}
    return f"fleet {other.get('name') or other['slug']} ({other['slug']})" if other.get("slug") else f"fleet {row['fleetId']}"


def unknown_initiative(answers, saved):
    """Stop because the initiative --initiative names does not exist, after putting back the one the answers had,
    so a later run neither takes the mistyped ID for a recorded one nor finds or creates another in its place."""
    initiative = answers.pop("initiativeId")
    if saved.get("initiativeId"):
        answers["initiativeId"] = saved["initiativeId"]
    save_answers(answers)
    raise SetupError(f"There is no initiative {initiative} in the development organisation {answers['organisation']}, "
                     "so setup stopped before changing any tracker record. Check the ID given with --initiative (the "
                     "desktop shows it, as does GET /api/v1/initiatives), or leave --initiative out to keep the "
                     "fleet's current one.")


def adopt_initiative(token, slug, initiative, *, adopt=True):
    """Assign the initiative --initiative names on this run to this fleet, moving it from any other: the one move
    of an initiative from another fleet setup makes, and only when asked."""
    fleet = api_call(token, "GET", f"/fleets/{slug}")["id"]
    row = api_call(token, "GET", f"/initiatives/{initiative}")
    if row.get("fleetId") != fleet and (adopt or not row.get("fleetId")):
        api_call(token, "PUT", f"/initiatives/{initiative}/fleet", {"fleetId": fleet})
        say(f"assigned initiative {row.get('name')} ({initiative}) to this fleet"
            + (f", moving it from {fleet_label(row)}" if row.get("fleetId") else ""))


def agent_reader(answers, members):
    """A read-only GET of the fleet's Subspace API as its rear admiral's own agent record, with a hosted API
    through its signing-proxy socket and with an API on this VM through the agent listener, so it sees what the
    fleet's officers see; or None and the reason there is none yet."""
    rear = members[0]
    if answers.get("apiUrl") or (answers.get("api") and local_signed_api(read_json(INPUTS / "fleet.json", {}))):
        source, socket = installed_stargate_source(), runtime.agent_socket(rear["id"])
        if not source or not socket.is_socket():
            return None, "the officers have not enrolled with the Subspace API yet"

        def get(path):
            try:
                # Bun keeps no transpiler cache for this read, so a dry run writes nothing.
                result = subprocess.run([FLEET_APP / "bin/bun", source / "src/main.ts", "subspace", "request", "--socket",
                                         socket, f"/api/v1{path}"], capture_output=True, text=True, timeout=30,
                                        check=False, stdin=subprocess.DEVNULL,
                                        env={**os.environ, "BUN_RUNTIME_TRANSPILER_CACHE_PATH": "0"})
            except (OSError, subprocess.TimeoutExpired):
                raise ApiError(f"Could not ask the hosted Subspace API for GET {path}.", None) from None
            if result.returncode:
                raise ApiError(f"The hosted Subspace API refused GET {path}: {result.stderr.strip()[:200]}", None)
            return json.loads(result.stdout or "{}")
        return get, None
    if not answers.get("api") or not (INPUTS / "api.env").is_file() or not agent_listener_enabled():
        return None, "this VM's API has no agent listener"

    def get(path):
        request = Request(f"http://{LOOPBACK}:{AGENT_API_PORT}/api/v1{path}",
                          headers={"X-Subspace-Agent": rear["account"], "Accept": "application/json"})
        try:
            with urlopen(request, timeout=15) as response:
                return json.loads(response.read().decode() or "{}")
        except HTTPError as error:
            error.close()
            raise ApiError(f"The Subspace API returned HTTP {error.code} for GET {path}.", error.code) from None
        except (URLError, OSError, ValueError):
            raise ApiError(f"Could not reach the Subspace API's agent listener for GET {path}.", None) from None
    return get, None


def ownership_lines(get, repositories, initiative=None, assigns=False, adopt=False):
    """What the tracker says about this fleet's work, as its rear admiral reads it (get).

    A project belongs to its initiative's fleet, or else to the fleet of the initiative's nearest ancestor with one,
    and the API refuses a fleet agent's assignment, or Ready for agents flag, in another fleet's project
    (subspace-api src/fleet-ownership.ts). So this names each tracker project linked to one of the fleet's GitHub
    repositories that belongs to another fleet, with the initiative that makes it so, and the fleet's own
    initiative (setup's answer, which --initiative sets) when it is not the fleet's yet. assigns says whether setup
    can assign that initiative to the fleet (an API on this VM with development sign-in); otherwise only an
    administrator of the API can. Setup assigns one that belongs to no fleet, and moves one from another fleet
    only when --initiative names it on this run (adopt).
    """
    me = get("/me")
    fleet = me.get("fleetId")
    if me.get("kind") != "agent" or not fleet:
        return ["could not check which fleet owns this fleet's tracker projects: its rear admiral is not yet a "
                "registered agent of a fleet"]
    rows = {}

    def initiative_row(initiative_id):
        if initiative_id not in rows:
            rows[initiative_id] = get(f"/initiatives/{quote(initiative_id, safe='')}")
        return rows[initiative_id]

    def owner(initiative_id):
        """The initiative that gives a project its fleet, and the initiatives on the way to it."""
        path = []
        while initiative_id and initiative_id not in path:
            path.append(initiative_id)
            row = initiative_row(initiative_id)
            if row.get("fleetId"):
                return row, path
            initiative_id = row.get("parentId")
        return None, path

    def remedy(initiative_id, record=True):
        if assigns:
            return f"run setup again with --initiative {initiative_id}, which assigns it, and all below it, to this fleet"
        return (f"ask an administrator of the Subspace API to assign it to this fleet (PUT /api/v1/initiatives/"
                f'{initiative_id}/fleet with {{"fleetId": "{fleet}"}})'
                + (f", and pass --initiative {initiative_id} to record it as the fleet's" if record else ""))

    lines, ours, seen = [], [], set()
    own, takes = None, False
    if initiative:
        try:
            own = initiative_row(initiative)
        except ApiError as error:
            if error.status != 404:
                raise
            return [f"WARNING: the tracker has no initiative {initiative}: check the ID given with --initiative"
                    + ("; a real run stops before changing any tracker record." if assigns else ".")
                    if adopt else
                    f"WARNING: the tracker has no initiative {initiative}, which setup recorded as the fleet's"
                    + ("; a real run finds or creates one by the fleet's name." if assigns else ".")]
        # The run gives it to this fleet: one of no fleet as the installer registers it, one of another fleet only
        # when --initiative names it.
        takes = assigns and (adopt or not own.get("fleetId"))
    for repository in repositories:
        page = get(f"/repositories/projects?repository={quote(repository, safe='')}&limit=100")
        for project in page.get("items", []):
            if project["id"] in seen:
                continue
            seen.add(project["id"])
            row, path = owner(project.get("initiativeId"))
            if not row or row["fleetId"] == fleet:
                ours.append(project["key"])
                continue
            where = (f"tracker project {project['key']} ({repository}) belongs to {fleet_label(row)} through "
                     f"initiative {row.get('name')} ({row['id']})")
            if initiative in path and takes:
                lines.append(f"{where}; setup assigns the fleet's initiative {initiative}, on that path, to this "
                             "fleet, which makes it this fleet's.")
            else:
                lines.append(f"WARNING: {where}, so this fleet's officers are refused when they assign its issues or "
                             f"mark them Ready for agents. To make it this fleet's, {remedy(row['id'])}.")
    if own and own.get("fleetId") != fleet:
        name = f"the fleet's initiative {own.get('name')} ({initiative})"
        if takes:
            moved = f", moving it from {fleet_label(own)}" if own.get("fleetId") else ""
            lines.append(f"setup assigns {name} to this fleet{moved}.")
        elif assigns:
            lines.append(f"WARNING: {name} belongs to {fleet_label(own)}, and setup leaves it there. To make it this "
                         f"fleet's, {remedy(initiative)}.")
        else:
            other = f" but to {fleet_label(own)}" if own.get("fleetId") else ""
            lines.append(f"WARNING: {name} is not assigned to it{other}: {remedy(initiative, record=False)}.")
    if ours:
        lines.append(f"tracker projects {', '.join(sorted(set(ours)))} belong to this fleet or to none, so its "
                     "officers may assign their issues")
    elif not lines:
        lines.append("no tracker project lists this fleet's GitHub repositories, so setup cannot tell which fleet "
                     "owns its work")
    return lines


def manual_adoption(answers, initiative):
    """The request for an administrator of the Subspace API that adopts the initiative --initiative names, for a
    fleet whose setup cannot assign it: with a hosted API, or one on this VM without development sign-in."""
    why = ("a hosted API's administrators keep its records" if answers.get("apiUrl") else
           "this VM's API no longer offers development sign-in, through which setup acts as its administrator")
    return (f"WARNING: setup cannot assign the initiative given with --initiative ({initiative}) to this fleet, as "
            f"{why}. Ask an administrator of the Subspace API to assign it: PUT /api/v1/initiatives/{initiative}/fleet "
            f'with {{"fleetId": "<the fleet\'s ID>"}}, where the fleet\'s ID is the "id" that GET /api/v1/fleets/'
            f"{answers['slug']} returns.")


def ownership_check(answers, members, config, adopt=False):
    """Say what ownership_lines finds; a check that cannot run says why, and never stops setup.

    adopt says --initiative named the fleet's initiative on this run. When setup cannot assign it, the request
    for an administrator follows a check that cannot run; one that runs names the fleet's own ID in it.
    """
    initiative = answers.get("initiativeId")
    local = bool(answers.get("api")) and not answers.get("apiUrl")
    # Setup acts as the admin through development sign-in, which a new fleet's API offers.
    assigns = local and (not (INPUTS / "api.env").is_file() or dev_auth_enabled())
    manual = adopt and initiative and (local or answers.get("apiUrl")) and not assigns
    get, reason = agent_reader(answers, members)
    repositories = [name for row in config.get("repositories", []) if isinstance(row, dict)
                    if (name := runtime.github_repository(str(row.get("url", ""))))]
    if not get:
        say(f"not checking which fleet owns this fleet's tracker projects: {reason}")
        if manual:
            say(manual_adoption(answers, initiative))
        return
    try:
        lines = ownership_lines(get, repositories, initiative, assigns=assigns, adopt=adopt)
    except (ApiError, ValueError, KeyError, TypeError, AttributeError) as error:
        say(f"could not check which fleet owns this fleet's tracker projects: {error}")
        if manual:
            say(manual_adoption(answers, initiative))
        return
    for line in lines:
        say(line)


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
    hostname = registration.get("ircHost")
    require(isinstance(hostname, str) and not loopback_irc_host(hostname),
            "A further fleet needs a remote IRC server (--irc-server or --irc remote --irc-host).")
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
    if initiative:
        assigned = api_call(token, "GET", f"/initiatives/{initiative}").get("fleetId")
        require(assigned in (None, fleet["id"]), f"Initiative {initiative} already belongs to another fleet.")
        if assigned != fleet["id"]:
            api_call(token, "PUT", f"/initiatives/{initiative}/fleet", {"fleetId": fleet["id"]})
        registered.update(initiativeId=initiative)
    registered.pop("channels", None)
    write_json(INPUTS / "answers.json", answers)
    say(f"registered fleet {name} ({slug}), its ships and officers" + (f", with initiative {initiative}" if initiative else ""))


# Summary ---------------------------------------------------------------------

def fleet_channels(answers, members):
    """The fleet's standard channels: Starbase, the command channel, then one ship channel per captain."""
    return standard_channels.standard(answers["slug"], [person["id"] for person in members[1:]],
                                      answers.get("officerNames") == "plain")


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


def secret_file(path, option, label):
    """What a root-only file holds, stripped, for this run only. Messages name the file, never what it holds."""
    require(Path(path).is_absolute(), f"{option} needs an absolute path to a file on this VM.")
    try:
        info = os.lstat(path)
        require(stat.S_ISREG(info.st_mode) and info.st_uid == SECRET_OWNER and not info.st_mode & 0o077,
                f"The {label} file {path} must be a regular file owned by root with mode 0600: chmod 600 {path}.")
        return Path(path).read_text().strip()
    except (OSError, UnicodeDecodeError):
        raise SetupError(f"Cannot read the {label} file {path}.") from None


def password_file(path, option, label):
    """A password from a root-only file, for this run only. Messages name the file, never what it holds."""
    password = secret_file(path, option, label)
    require(password and len(password) <= 512 and not re.search(r"[\s\x00]", password),
            f"The {label} file {path} must hold one password on one line.")
    return password


# The longest bearer token the Subspace API reads (subspace-api src/auth.ts); identity providers' access tokens
# are often 600 to 2,000 characters.
API_TOKEN_LIMIT = 16_384


def bearer_token_file(path, option, label):
    """A bearer token from a root-only file, for this run only: one line of printable ASCII, at most the API's own
    limit. Messages name the file, never what it holds."""
    token = secret_file(path, option, label)
    require(re.fullmatch(rf"[\x21-\x7e]{{1,{API_TOKEN_LIMIT}}}", token),
            f"The {label} file {path} must hold one bearer token on one line, at most {API_TOKEN_LIMIT:,} characters.")
    return token


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


# IRC accounts setup registered (SAREGISTER) for a certificate whose binding (CERT ADD) it has not seen succeed.
IRC_REGISTERED = "irc-registered.json"


def registered_accounts():
    """{account: certificate fingerprint} for accounts an earlier run registered with NickServ and may have stopped
    before binding: the next run finishes the binding instead of taking the account for someone else's."""
    value = read_json(INPUTS / IRC_REGISTERED, {})
    return value if isinstance(value, dict) else {}


def note_registered(account, fingerprint=None):
    """Record that setup created an account for a certificate (fingerprint), or forget it once it signs in."""
    registered = registered_accounts()
    if fingerprint:
        registered[account] = fingerprint
    elif registered.pop(account, None) is None:
        return
    write_json(INPUTS / IRC_REGISTERED, registered)


def check_certificates(answers, members, certificates, operator, register):
    """With the operator's credential: check it, register the officers' accounts and bind their certificates
    (register), and check that each certificate signs in as its account. A dry run only checks. An account an
    earlier run registered and stopped before binding (registered_accounts) is only bound."""
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
        earlier = {account for account, fingerprint in registered_accounts().items() if missing.get(account) == fingerprint}
        replies = irc_accounts.register(host, port, ca, name, password, missing if register else {}, signed_in_as,
                                        registered=earlier, created=note_registered)
        if not register:
            say(f"the IRC operator {name} is accepted on {host}")
        for account, lines in replies.items():
            say(f"NickServ for {account}: " + " / ".join(lines))
        failed = list(unbound()) if register and missing else []
        for account in set(missing) - set(failed) if register else ():
            note_registered(account)
    except irc_accounts.IrcError as error:
        raise SetupError(str(error)) from None
    require(not register or not failed, f"{', '.join(failed)} cannot sign in to {host} with {'its' if len(failed) == 1 else 'their'} "
            "certificate after registration (see NickServ's replies above). Check that the operator's class has "
            "NickServ's accreg capability, or bind the fingerprints by hand, then run setup again.")


def irc_bundle(answers, members, secret_store, certificates=None):
    """What a shared IRC server needs for this fleet: its SASL accounts and channels."""
    channels = fleet_channels(answers, members)
    topics = [f"{answers['name']} Starbase: admirals and the rear admiral",
              f"{answers['name']} fleet command: the rear admiral and the captains, and agent dispatch"] + [
        f"{answers['name']}: {person['name']}'s ship" for person in members[1:]]
    bundle = {
        "fleet": answers["slug"],
        "accounts": ([{"name": account, "fingerprint": fingerprint} for account, (_, fingerprint) in certificates.items()]
                     if certificates is not None else [
                         {"name": person["account"], "password": secret_store.password(f"irc-{person['account']}")}
                         for person in members]),
        "channels": [{"name": name, "topic": topic} for name, topic in zip(channels, topics, strict=True)],
        "humans": [answers["admin"]],
    }
    write_json(INPUTS / "irc-bundle.json", bundle)
    return bundle


def summary(answers, members, stargate_api, lockdown_id=""):
    local = answers["irc"] == "local"
    installed = read_json(INPUTS / "fleet.json", {}) or {}
    view = installed.get("agentsview", {})
    previews = {"port": mockups.PORT, **installed.get("mockupPreviews", answers.get("mockupPreviews", {}))}
    previews_on = len(members) > 1 and previews.get("enabled", True)
    agentsview = view.get("enabled", answers.get("agentsview", True))
    view_port = view.get("port", AGENTSVIEW)
    ports = [DASHBOARD]
    if agentsview:
        ports.append(view_port)
    if local:
        ports = [IRC_PLAIN, IRC_TLS, *ports]
    if answers["api"]:
        ports.insert(0, API_PORT)
    if stargate_api:
        ports.append(STARGATE_API_PORT)
    if previews_on:
        ports.append(previews["port"])
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
        "SUBSPACE_AGENTSVIEW": "1" if agentsview else "0",
        "SUBSPACE_AGENTSVIEW_PORT": str(view_port),
        "SUBSPACE_PORTS": " ".join(str(port) for port in ports),
        "SUBSPACE_PREVIEW_URL": mockups.address(previews) if previews_on else "",
        "SUBSPACE_OPERATOR": answers["operator"],
        "SUBSPACE_ACCESS": answers["access"],
        "SUBSPACE_LOCKDOWN": "1" if answers["lockdown"] else "0",
        "SUBSPACE_LOCKDOWN_ID": lockdown_id,
        **({"SUBSPACE_HOSTED_API": answers["apiUrl"]} if answers.get("apiUrl") else {}),
    }
    write_private(INPUTS / "summary.env", "".join(f"{key}={value}\n" for key, value in values.items()))
    return values


# Main ------------------------------------------------------------------------

def planned_private_ingress(network, answers, *, accept_officer_tracker_access_loss=False):
    require(answers["api"] and answers["irc"] == "local" and answers["lockdown"],
            "--private-network requires an API and Ergo on this VM and lockdown enabled.")
    require(not answers["agentDispatch"], "Guided agent dispatch still uses development sign-in; pass --no-agent-dispatch.")
    https, irc_tls = private_network.PORTS
    require(not (set(map(int, answers["allowPorts"])) & {https, irc_tls}),
            f"Private ingress sets its own firewall policy for TCP {https} and {irc_tls}. Remove them from --allow-port.")
    for path in (FLEET_ETC / "fleet.json", INPUTS / "fleet.json"):
        api = read_json(path, {}).get("api") or {}
        if urlsplit(api.get("url", "")).hostname in {"localhost", LOOPBACK, "::1"}:
            impact = ("Production sign-in removes officers' installer-managed tracker access (Subspace MCP and REST "
                      "tools, signed enrolment and dispatch) until setup supports SSO fleet registration.")
            require(accept_officer_tracker_access_loss,
                    f"{impact} Leave private ingress off or pass --accept-officer-tracker-access-loss.")
            say(f"WARNING: {impact}")
            break
    private_network.require_sso(api_environment(answers, {}, external=external_database(answers))[0])
    ingress = private_network.IngressPlan.prepare(network, INPUTS, API_PORT, IRC_PLAIN,
                                                 secret_file=secret_file, write_private=write_private, run=run)
    say(f"private workstations: https://{network.name} and TLS IRC at {network.name}:{irc_tls} "
        f"(on {network.interface}, from {network.clients}); other endpoints stay on loopback")
    return ingress


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
    network = private_network.Network.parse(answers.get("privateNetwork"))
    require(answers["lockdown"] or not private_network.UNIT.is_file(),
            "Remove existing private ingress with --private-network off and lockdown enabled before skipping it.")
    ingress = planned_private_ingress(
        network, answers, accept_officer_tracker_access_loss=args.accept_officer_tracker_access_loss,
    ) if network else None
    if saved.get("slug") and saved["slug"] != answers["slug"]:
        raise SetupError("The fleet ID cannot change on an installed VM. Reinstall on a fresh VM to rename it.")
    # The API has run here with its own PostgreSQL (setup writes api.env only then): its tracker is in that
    # container's volume, and setup does not move it.
    require(not (external_database(answers) and not saved.get("databaseUrlFile") and (INPUTS / "api.env").is_file()),
            "This VM's API already keeps its tracker in its own PostgreSQL, which setup does not move. To use an "
            "existing PostgreSQL, restore a dump of that database there and set up a fresh VM with "
            "--database-url-file (installer README, 'An existing PostgreSQL').")
    stargate_tag, stargate_ref = stargate_release(answers)
    say(f"installing Stargate {stargate_tag} ({stargate_ref[:12]})")
    check_usage_guard_source(answers, rear, stargate_ref)
    members = roster(answers, rear, captains)
    check_officer_tool_source(answers, members, stargate_ref)
    configure_decision_outcome_dms(answers, rear, stargate_ref)
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
    if not args.dry_run and not ingress:
        # Disable ingress before recording off. An interrupted run must not bring it back on boot.
        private_network.remove()
    save_answers(answers)
    if enrolment:
        store_enrolment_token(enrolment)
    # A dry run neither reads nor writes Key Vault: a value the copied store lacks is made up for validation.
    secret_store = Secrets(INPUTS, answers["slug"], None if args.dry_run else answers.get("keyVault"))
    # The toolchains as confirmed, with this run's choices: fleet.json's toolchains and systemPackages come from it.
    record, changes = update_toolchains(args, answers)
    for line in changes:
        say(line)
    local_irc = answers["irc"] == "local"
    config_path = INPUTS / "fleet.json"
    secrets_path = INPUTS / "fleet.secrets.json"

    dispatch_token = ""

    def inputs(irc_ref, api_block=None, stargate_api=False, token="", history=None, dispatch=None):
        return run_inputs(answers, members, secret_store, stargate_ref, irc_ref, api_block, stargate_api, token,
                          history, dispatch_token if dispatch is None else dispatch)

    def planned_inputs():
        return planned_run_inputs(answers, members, secret_store, stargate_ref)

    def write_inputs(*args):
        config, private = inputs(*args)
        write_fleet(config_path, config)
        write_json(secrets_path, private)

    certificates = operator = None
    say("officer names (" + ("plain: each officer's ID is its IRC account, API name and Stargate ID" if
                             answers["officerNames"] == "plain" else "fleet-prefixed, as installed; 'subspace-setup "
                             "rename-officers' gives them plain names") + "): "
        + ", ".join(person["account"] for person in members))
    if local_irc and answers.get("ircAuth") == "certificate":
        local_irc_tls()
    elif answers.get("ircAuth") == "certificate":
        operator = irc_operator(args)
        certificates = officer_certificates(members, generate=not args.dry_run)
        if answers.get("managedSharedIrc") and not args.dry_run:
            irc_bundle(answers, members, secret_store, certificates)
        else:
            for line in certificate_lines(answers, certificates):
                say(line)
        if operator and args.dry_run:
            check_certificates(answers, members, certificates, operator, register=False)
    elif not local_irc:
        irc_bundle(answers, members, secret_store)
    if external_database(answers):
        for line in external_plan(answers, args.migrate):
            say(line)
    # Before setup changes anything here, on the IRC server or in the API: every officer's name must be free there.
    problems = irc_name_problems(answers, members, certificates, operator)
    if answers.get("apiUrl"):
        problems += hosted_name_problems(answers, members, enrolment)
    elif answers["api"] and (INPUTS / "api.env").is_file() and dev_auth_enabled() and api_healthy():
        problems += local_name_problems(answers, members, api_reader(answers))
    require_names(problems)
    if args.dry_run:
        if external_database(answers):
            database_checks(answers, args.migrate)
        config, private = planned_inputs()
        write_fleet(config_path, config)
        write_json(secrets_path, private)
        if answers["agentDispatch"] and not config["agentDispatch"]["enabled"]:
            say("agent dispatch needs the API, which the dry run leaves out; the real run checks it")
        if local_signed_api(config):
            with tempfile.TemporaryDirectory(prefix="subspace-planned-secrets-") as scratch:
                installer(config_path, dry_run=True, secrets_dir=planned_secrets(config, Path(scratch)))
        else:
            installer(config_path, dry_run=True)
        ownership_check(answers, members, read_json(config_path, {}), adopt=bool(args.initiative))
        if answers["toolchains"] == "derive":
            propose_safely(read_json(config_path, {}), read_json(INPUTS / TOOLCHAIN_RECORD) or record, save=False)
        say("dry run complete; nothing on this VM changed")
        return

    if ingress:
        # Never proxy the API while changing its sign-in settings, even if setup is interrupted.
        ingress.quiesce(write_private=write_private, run=run)
    local_signing = False
    if answers["api"]:
        # The API comes first (its token and registration go into the fleet's inputs), so check now that the
        # installer will accept those inputs: with --migrate, a refused run must not have migrated the database.
        config, private = planned_inputs()
        preflight(config, private)
        local_signing = local_signed_api(config)
        if local_signing:
            try:
                runtime.directory(runtime.STATE, 0o755)
                runtime.account(GATEWAY_USER, runtime.STATE / "gateway")
            except runtime.Invalid as error:
                raise SetupError(str(error)) from None

    irc_ref = irc_source() if local_irc else None
    # Releases from v0.40.0 have the Fleet API; a pinned commit is checked after it is installed.
    stargate_api = {"on": True, "off": False}.get(answers["stargateApi"])
    if stargate_api is None and stargate_tag != "pinned":
        stargate_api = tuple(int(part) for part in stargate_tag[1:].split(".")) >= (0, 40, 0)

    # The API and PostgreSQL come first. On a single host, Ergo keeps its history in the
    # same PostgreSQL (a second database), so the fleet installer runs once with both.
    # Keep the agents away from operator-only ports and cloud metadata before any service starts.
    # This never blocks the operator, so it applies at once and is never rolled back.
    run(HERE / "lockdown.sh", "isolate", *isolation_args(answers, ingress))
    api_block, token, history = None, "", None
    if answers["api"]:
        run(HERE / "docker.sh")
        dev_auth = api_up(answers, secret_store, args.migrate)
        if local_irc and not external_database(answers):
            history = history_database(secret_store)
        elif local_irc:
            say("Ergo keeps IRC history in its own SQLite file on this VM, not in the existing PostgreSQL")
        if dev_auth:
            token = bootstrap_local_owner(answers)
            # An existing database may already hold an officer's name: stop before anything is registered.
            require_names(local_name_problems(answers, members, token))
            # Only --initiative on this run adopts an initiative, and only one that exists: checked before any
            # tracker record changes.
            adopt = bool(args.initiative)
            if adopt and not initiative_row(token, answers["initiativeId"]):
                unknown_initiative(answers, saved)
            if answers["demo"]:
                seed_demo(answers)
            initiative = fleet_initiative(token, answers["name"], answers["slug"], answers.get("initiativeId"),
                                          adopt=adopt, hint=", or setup with --initiative "
                                          f"{answers.get('initiativeId')}; to have setup find or create one by the "
                                          "fleet's name instead, pass --initiative ''")
            if initiative:
                answers.update(initiativeId=initiative)
                save_answers(answers)
            api_block = local_api_block(answers, [initiative] if initiative else [], agent_listener_enabled())
            # Register the fleet, its channels, ships and officers now: agent dispatch signs in as the rear
            # admiral's agent record. The installer's 'fleetctl register' sends the same records again, and
            # assigns the initiative when it belongs to no fleet; an adopted one setup moves here itself.
            records = register_own(token, *inputs(irc_ref, api_block, bool(stargate_api), token, history))
            if local_signing:
                api_block["enrolment"] = True
            if initiative and (adopt or api_block.get("enrolment")):
                adopt_initiative(token, answers["slug"], initiative, adopt=adopt)
            if api_block.get("enrolment"):
                # Refresh the short-lived grant when installing. Existing keys stay valid and use none.
                grant = api_call(token, "POST", f"/fleets/{answers['slug']}/enrolment-tokens",
                                 {"expiresInSeconds": 3600, "maxUses": len(members)})["token"]
                require(re.fullmatch(runtime.ENROLMENT_TOKEN_TEXT, grant),
                        "The local API did not return a fleet enrolment token.")
                store_enrolment_token(grant)
            if answers["agentDispatch"] and not answers.get("decisionOutcomeDms"):
                dispatch_token = rear_admiral_token(answers, records)
        else:
            say("the API no longer offers development sign-in, so setup leaves its fleet registration, "
                "initiative and demo data as they are")
            # Without development sign-in, setup writes no API settings, which dispatch needs.
            require(not dispatch_uses_api(answers),
                    "Agent dispatch needs the API's registration in the fleet's inputs, which setup writes only while "
                    "the API offers development sign-in. Pass --no-agent-dispatch and --no-decision-outcome-dms, "
                    "and configure api and agentDispatch for the fleet installer by hand (subspace-installer/README.md, "
                    "'Agent dispatch').")

    if certificates and operator:
        # Before any officer starts, and once the API has its records: register the accounts, bind the certificates
        # and check each sign-in.
        check_certificates(answers, members, certificates, operator, register=True)

    write_inputs(irc_ref, api_block, bool(stargate_api), token, history)
    installer(config_path)
    if not answers["api"]:
        api_down()
    if dispatch_uses_api(answers) or answers["api"]:
        # The installer has created the officers' users and Stargate's gateway user: let the gateway, and only it,
        # reach the API's port, and the officers, and only they, reach the agent listener.
        run(HERE / "lockdown.sh", "isolate", *isolation_args(answers, ingress))
    if stargate_api is None:
        stargate_api = confirm_fleet_api(None, installed_stargate_source())
        if stargate_api:
            # Again with the Fleet API: its listener, firewall rule and token, and its address in the fleet's record.
            if api_block and api_block.get("enrolment"):
                register_own(token, *inputs(irc_ref, {key: value for key, value in api_block.items()
                                                     if key != "enrolment"}, True, token, history))
            write_inputs(irc_ref, api_block, True, token, history)
            installer(config_path)
    elif stargate_api:
        confirm_fleet_api(True, installed_stargate_source())
    ownership_check(answers, members, read_json(config_path, {}), adopt=bool(args.initiative))
    if answers["toolchains"] == "derive":
        # The repositories are cloned now. Nothing proposed is installed until the person confirms it, and nothing
        # in a repository stops setup before the lockdown below.
        # The record as write_fleet left it, with what fleet.json was written with.
        propose_safely(read_json(config_path, {}), read_json(INPUTS / TOOLCHAIN_RECORD) or record, save=True)
    if args.verify_toolchains:
        # Each repository's basic check as a crew member; a failure is reported, never fatal.
        run(FLEET_APP / "lib/runtime.py", "verify-toolchains", check=False)

    lockdown_id = ""
    if answers["lockdown"]:
        lockdown = [HERE / "lockdown.sh", "apply", *isolation_args(answers, ingress)]
        for address in answers["sshFrom"]:
            lockdown += ["--ssh-from", address]
        for value in answers["allowPorts"]:
            lockdown += ["--allow-port", str(value)]
        if answers["access"] in ("tailscale", "cloudflare"):
            lockdown.append(f"--{answers['access']}")
        if ingress:
            lockdown.extend(["--private-ingress", ingress.network.firewall_target])
        run(*lockdown)
        lockdown_id = run(HERE / "lockdown.sh", "applied-id", capture=True)
    else:
        say("WARNING: lockdown skipped. Run 'sudo subspace-setup lockdown apply' before putting anything on this VM.")
    retirement = None
    if ingress:
        ingress.enable(lockdown_id, write_private=write_private, run=run)
    else:
        try:
            private_network.retire_account(run=run)
        except private_network.RetirementBlocked as error:
            # The proxy is stopped and isolation no longer exempts the account. Stopping now would only skip the
            # host's confirmation of this lockdown, so it rolls back, and fail every later run the same way.
            retirement = error
            say(f"WARNING: {error}")
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
    else:
        say("agent dispatch is off; 'subspace-setup --agent-dispatch' switches it on.")
    if (read_json(config_path, {}).get("agentDispatch") or {}).get("decisionOutcomeDms"):
        say("captains receive accepted or rejected decisions by private IRC message.")
    if dispatch_token:
        say("WARNING: the rear admiral's development sign-in token lasts eight hours. Run setup again to renew "
            "it, or move the API to WorkOS or Entra and give the rear admiral a long-lived token "
            "(installer README, 'Agent dispatch').")
    if not local_irc:
        say(remote_irc_next_step(answers, operator))
    say(f"forward these loopback ports to reach it: {values['SUBSPACE_PORTS']}")
    # The VM's memory, each engine's limits and why, limits set by hand, and swap, for the operator and their agent
    # (installer README, 'Memory limits and swap'). --captain-memory, --rear-admiral-memory and --swap change them.
    run(FLEET_APP / "lib/runtime.py", "memory-summary", check=False)
    if run(HERE / "providers.sh", "status", check=False):
        say("next: bring the agents online by signing CLIProxyAPI in to a provider account "
            "(INSTALL.md, 'Bring the agents online').")
    if retirement:
        # Again last, where the summary and audit cannot scroll it away.
        say(f"WARNING: {retirement}")


def remote_irc_next_step(answers, operator):
    """What is left to do on a remote IRC server once setup is done."""
    if answers.get("ircAuth") == "certificate" and not answers.get("managedSharedIrc"):
        # No password bundle exists in this mode: the officers' accounts are bound to their certificates.
        if operator:
            return (f"officers sign in to {answers['ircHost']} with certificates, which setup registered and checked "
                    "above with --irc-operator.")
        return (f"officers sign in to {answers['ircHost']} with certificates: each can sign in once the IRC server's "
                "operator has run the /NS lines printed above for it (or run setup again with --irc-operator). "
                "'sudo /opt/subspace-fleet/lib/runtime.py irc-fingerprints' prints the fingerprints again.")
    return (f"add this fleet to the shared IRC server: copy {INPUTS / 'irc-bundle.json'} there and run "
            "'sudo subspace-setup irc-server add <file>' (the host installer does this with --irc-server).")


def isolation_args(answers, ingress=None):
    """lockdown.sh options that keep the agents away from operator-only ports and cloud metadata."""
    args = ["--operator", answers["operator"], "--protect-metadata"]
    if answers["api"] or (INPUTS / "api.env").is_file():
        # The API (development sign-in) and PostgreSQL, also through their container addresses,
        # and still after a switch to --no-api in case stopping them failed.
        args += ["--operator-only-port", str(API_PORT)]
        if ingress:
            args += ingress.isolation_args
        if not answers.get("databaseUrlFile"):
            args += ["--operator-only-port", str(runtime.SUBSPACE_DATABASE_PORT)]  # Bundled PostgreSQL.
        try:
            pwd.getpwnam(GATEWAY_USER)
            gateway = dispatch_uses_api(answers) or (answers["api"] and local_signed_api(
                {"api": planned_api_block(answers)}))
        except KeyError:
            gateway = False  # Not yet created: setup isolates again once the installer has run.
        if gateway:
            # Agent dispatch: Stargate's gateway polls the API as the rear admiral. Agents never reach this port.
            args += ["--port-user", f"{API_PORT}:{GATEWAY_USER}"]
        # Keep the legacy port restricted even when it is off. Admit officers only while setup enables it.
        args += ["--operator-only-port", str(AGENT_API_PORT)]
        if answers["api"] and api_environment(answers, {})[1]:
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


# Rename ----------------------------------------------------------------------

# Stargate's subspaceAgents state for the shared gateway: <dataDirectory>/subspace-agents, <name>.pem per officer.
GATEWAY_KEYS = runtime.STATE / "gateway" / "data" / "subspace-agents"


def rename_parser():
    parser = argparse.ArgumentParser(
        prog="subspace-setup rename-officers",
        description="Give an installed fleet's officers their plain names: each officer's ID becomes its IRC account "
                    "and nick, its Subspace API agent name and its ship channel (#<captain-id>), as a new fleet "
                    "has them. Safe to run again after a failure. See 'Officer names' in the installer README.")
    parser.add_argument("--dry-run", action="store_true", help="check and print the plan, changing nothing")
    for flag, text in (("--irc-operator", "an IRC operator's name (OPER) with NickServ's accreg capability, to "
                        "register the new accounts and bind their certificates"),
                       ("--irc-operator-password-file", "a root-only file (mode 0600) with that operator's password"),
                       ("--irc-operator-account", "an IRC account the operator's connection signs in to first"),
                       ("--irc-operator-account-password-file", "a root-only file with that account's password"),
                       ("--api-admin-token-file", "a root-only file (mode 0600) with a bearer token of an "
                        "administrator of the hosted Subspace API, which renames the agent records in place "
                        "(PUT /api/v1/agents/{id}); read for this run only, never printed or saved"),
                       ("--crew-names", "names for the crew of captains already installed, as for setup: needs a "
                        "Stargate whose crew plugin lets a crew member's name differ from its ID")):
        parser.add_argument(flag, help=text)
    parser.add_argument("--no-api-records", action="store_true",
                        help="for a fleet with no Subspace API on this VM and no hosted one: no Subspace API keeps "
                             "this fleet's agent records (not one installed with --api-target), so only IRC and "
                             "Stargate are renamed")
    parser.add_argument("--inputs", default=str(INPUTS), help=argparse.SUPPRESS)
    return parser


def installed_plan(answers):
    """The installed fleet's officers and their renames: [(person, old account, new account, old API name,
    new API name, old ship channel, new ship channel)], from the fleet's own inputs."""
    config = read_json(INPUTS / "fleet.json", {}) or {}
    people = [config.get("rearAdmiral") or {}, *(config.get("captains") or [])]
    require(all(isinstance(person, dict) and NAME_ID.fullmatch(str(person.get("id", ""))) for person in people),
            "Set the fleet up first: its inputs have no officers.")
    renamed = (answers.get("officerRename") or {}).get("from", {})
    plan = []
    for index, person in enumerate(people):
        old = renamed.get(person["id"]) or person.get("account") or officer_account(answers, person["id"])
        old_api = (person["name"] if index == 0 else old)
        channel = None if index == 0 else (standard_channels.ship(answers["slug"], person["id"]),
                                           standard_channels.ship(answers["slug"], person["id"], plain=True))
        plan.append((person, old, person["id"], old_api, person["id"], channel))
    return plan


def staged_bundle(agent_id, account, generate):
    """The new IRC certificate for an officer's new account, made once and kept for re-runs, or None."""
    path = INPUTS / "rename" / agent_id / runtime.IRC_CLIENT_PEM
    if not path.exists() and generate:
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(path.parent.parent, 0o700)
        write_private(path, runtime.new_irc_bundle(account))
    return path if path.exists() else None


def copy_gateway_key(old, new):
    """Give Stargate's key bundle and enrolment record for an officer's old API name its new name too, so the
    gateway keeps the officer's enrolled key under the new name (a copy: the old files stay until removed)."""
    for suffix in (".pem", ".pem.enrolment.json"):
        source, target = GATEWAY_KEYS / f"{old}{suffix}", GATEWAY_KEYS / f"{new}{suffix}"
        if not source.is_file() or source == target:
            continue
        if target.exists():
            require(target.read_bytes() == source.read_bytes(),
                    f"{target} exists with other contents than {source}. Move it aside, then run the rename again.")
            continue
        info = source.stat()
        with tempfile.NamedTemporaryFile(dir=GATEWAY_KEYS, delete=False) as handle:
            os.fchmod(handle.fileno(), 0o600)
            os.fchown(handle.fileno(), info.st_uid, info.st_gid)
            handle.write(source.read_bytes())
            temporary = handle.name
        os.replace(temporary, target)
        say(f"copied Stargate's {source.name} to {target.name}")


def check_switch(answers):
    """Check the rename's last step, the switch (setup's run with the new names), before the rename changes
    anything, with the code and checks the switch itself runs. answers are the switch's: plain names, and any new
    crew names with the crew's kept IDs. They are read and checked as setup reads and checks them, and the
    fleet.json and secrets setup then hands the fleet installer are checked by the fleet installer's validation,
    then by its own dry run (crew names across the fleet, pinned sessions, a new captain's crew, repositories). It
    works on a private copy of the fleet's inputs, as setup's dry run does, so nothing it writes stays. Returns
    lines that show each ship's crew as the switch installs them."""
    global INPUTS
    original = INPUTS
    with tempfile.TemporaryDirectory(prefix="subspace-setup-rename-check-") as scratch:
        INPUTS = Path(scratch) / "inputs"
        if original.is_dir():
            shutil.copytree(original, INPUTS, symlinks=True)
        try:
            switch = gather(parse_args(["--non-interactive"]), json.loads(json.dumps(answers)))
            rear, captains = check_answers(switch)
            members = roster(switch, rear, captains)
            ref = stargate_release(switch)[1]
            configure_decision_outcome_dms(switch, rear, ref)
            config, private = planned_run_inputs(switch, members, Secrets(INPUTS, switch["slug"], None), ref)
            try:
                cfg = runtime.validate(config, private, check_files=False)
            except runtime.Invalid as error:
                raise SetupError(f"{error} The fleet installer would refuse the switch to the new names, so the "
                                 "rename stops here. Nothing was changed.") from None
            # Quietly: before the switch, the installer's plan would show the officers' current certificates
            # against their new accounts, which the rename binds to new certificates of its own.
            preflight(config, private, "the rename changes anything", quiet=True)
        finally:
            INPUTS = original
    crews = [person for person in cfg["members"][1:] if person["crew"]]
    if not crews:
        return []
    return ["crew after the switch, by ship (name, ID, model family and slot):"] + [
        f"  {person['name']}: " + "; ".join(f"{worker['name']} ({worker['id']}, {slot_label(worker['slot'])})"
                                            for worker in person["crew"]) for person in crews]


def rename_officers(argv):
    """subspace-setup rename-officers: move an installed fleet from fleet-prefixed officer names to plain ones, and
    name the crew of installed captains where the installed Stargate allows it.

    The rename is recorded in answers.json (officerRename): "from", the old names, before anything changes; the
    plain names once IRC and the API have them; "switched" only once setup has run with them and, with certificate
    sign-in, every installed certificate signs in as its new account. Until then every run resumes at that last
    step, and only then does it print what is left to remove."""
    global INPUTS
    args = rename_parser().parse_args(argv)
    INPUTS = Path(args.inputs)
    require(args.dry_run or os.geteuid() == 0, "Run subspace-setup as root (sudo).")
    answers = read_json(INPUTS / "answers.json", {})
    require(answers.get("slug"), "Set the fleet up first.")
    crew = rename_crew(answers, args.crew_names) if args.crew_names else {}
    record = answers.get("officerRename") or {}
    plan = installed_plan(answers)
    officers = answers.get("officerNames") != "plain"
    # Renamed in IRC and the API, but setup has not yet finished a run with the new names.
    pending = not officers and bool(record.get("from")) and not record.get("switched")
    if not (officers or pending or crew):
        say("this fleet's officers already have plain names" + (f" (renamed from {', '.join(record['from'].values())})"
                                                               if record.get("from") else "") + ".")
        for line in rename_cleanup(answers, plan, record):
            say(line)
        return
    members = [{"id": person["id"], "name": person["name"], "account": new} for person, _, new, *_ in plan]
    # The answers the switch runs setup with: plain names, and any new crew names with the crew's kept IDs.
    plain = {**answers, "officerNames": "plain",
             **{key: {**(answers.get(key) or {}), **values} for key, values in crew.items()}}
    hosted = bool(answers.get("apiUrl"))
    local_api = answers.get("api") and not hosted and (INPUTS / "api.env").is_file()
    certificate = answers.get("irc") == "remote" and answers.get("ircAuth") == "certificate"
    staged, operator, admin, unasked = {}, None, None, False
    if officers:
        say(f"renaming fleet {answers['slug']}'s officers to plain names:")
        for person, old, new, old_api, new_api, channel in plan:
            say(f"  {person['name']}: IRC account {old} -> {new}; API agent {old_api} -> {new_api}"
                + (f"; ship channel {channel[0]} -> {channel[1]}" if channel else ""))
        require(hosted or local_api or args.no_api_records,
                f"Fleet {answers['slug']} has no Subspace API on this VM and no hosted API (--api-url), so this VM "
                "cannot rename its agent records. If another fleet's API keeps them (a fleet installed with "
                "--api-target, or registered there by hand), renaming here would leave those records under the old "
                "names, and the next registration would refuse the new names or add duplicate officers: the rename "
                "does not reach another fleet's API, so keep this fleet's fleet-prefixed names. If no Subspace API "
                "keeps this fleet's records, run the rename again with --no-api-records. Nothing was changed.")
        require(not local_api or dev_auth_enabled(),
                "The Subspace API on this VM no longer offers development sign-in, which setup renames its agent "
                "records with. Keep the fleet's fleet-prefixed names. Nothing was changed.")
    elif pending:
        say(f"fleet {answers['slug']}'s officers have their plain names on IRC and in the API, but setup has not yet "
            "finished a run with them: this run does that, then checks the installed certificates")
    # Read-only checks first, the switch's own before any other: what the switch (step 4) hands the fleet
    # installer, checked as it will be, so that neither the dry run nor this check passes what the switch refuses.
    for line in check_switch(plain):
        say(line)
    if officers:
        admin = bearer_token_file(args.api_admin_token_file, "--api-admin-token-file", "API administrator token") \
            if args.api_admin_token_file else None
        require(not hosted or admin or args.dry_run,
                f"This fleet uses the hosted Subspace API at {answers.get('apiUrl')}, whose agent records only an "
                "administrator may rename: pass --api-admin-token-file with a token of an administrator of that API. "
                "Nothing was changed.")
        operator = irc_operator(args)
        staged = {person["id"]: staged_bundle(person["id"], new, not args.dry_run) for person, _, new, *_ in plan
                  if certificate}
        # Every new name must be free on the IRC server and in the API, or this fleet's own.
        current = [INPUTS / "fleet.secrets" / person["id"] / runtime.IRC_CLIENT_PEM for person, *_ in plan]
        problems = irc_name_problems(plain, members, None, operator, staged, current)
        own = {person["id"]: {old} for person, old, *_ in plan}
        if hosted and admin:
            call = lambda method, path, body: hosted_call(answers["apiUrl"], admin, method, path, body)  # noqa: E731
            wanted = [(name, nick, {nick, *own[person["id"]]})
                      for (_, name, nick, _), person in zip(officer_records(plain, members), members, strict=True)]
            problems += fleet_records.taken_names(fleet_records.agents(call), answers["slug"], wanted,
                                                  fleet_records.usernames(call))
        elif hosted:
            unasked = True
            say("names: without --api-admin-token-file the hosted Subspace API was not asked about the new names")
        elif local_api and api_healthy():
            problems += local_name_problems(plain, members, api_reader(answers), own)
        elif local_api:
            unasked = True
            say("names: the Subspace API on this VM is not running, so it was not asked about the new names")
        require(not problems, "Cannot rename: " + "; ".join(problems) + ". An officer's ID is its plain name and "
                "cannot change here; free the name on the IRC server or in the API, or keep the fleet's prefixed "
                "names. Nothing was changed.")
    if args.dry_run:
        if officers:
            steps = []
            if certificate:
                steps.append("makes a new IRC certificate for each new account and "
                             + ("registers it with --irc-operator" if operator else "prints the NickServ lines to "
                                "register it"))
            elif answers.get("irc") == "remote":
                steps.append("writes the fleet's IRC bundle with the new accounts and stops until they sign in")
            if hosted or local_api:
                steps.append("renames the API agent records in place and moves the ship channels")
            say("dry run: " + ("the API was not asked, so its names are unchecked; the other checks found the new "
                               "names free. " if unasked else "the names are free. ")
                + "The real run " + ", then ".join([*steps, "runs setup again with the new names"])
                + ". Nothing on this VM changed.")
        else:
            say("dry run: the real run runs setup again with the new names. Nothing on this VM changed.")
        return
    if crew:
        for key in ("crewNames", "crewIds"):
            answers.setdefault(key, {}).update(crew[key])
        save_answers(answers)
    if officers:
        answers["officerRename"] = record = {**record, "from": {person["id"]: old for person, old, *_ in plan}}
        save_answers(answers)
        # 1. IRC: the new accounts, while the officers keep signing in to the old ones.
        if certificate:
            rename_irc_certificates(answers, plan, staged, operator)
        elif answers.get("irc") == "remote":
            rename_irc_passwords(answers, members)
        # 2. The API's agent records, in place, and the ships' channels.
        renames = [{"old": old, "name": new_api, "nick": new, "channel": channel}
                   for _, old, new, _, new_api, channel in plan]
        if hosted:
            fleet_records.rename(lambda method, path, body: hosted_call(answers["apiUrl"], admin, method, path, body),
                                 answers["slug"], renames, say=say)
            for _, _, _, old_api, new_api, _ in plan:
                copy_gateway_key(old_api, new_api)
        elif local_api:
            token = api_token(answers)
            rows = {str(row.get("ircNick", "")).lower(): row for row in
                    fleet_records.agents(lambda method, path, body: api_call(token, method, path, body))}
            # The account subjects stay: setup signs officers in by them, and the API binds each record to its own.
            subjects = dict(record.get("subjects") or {})
            for person, old, new, *_ in plan:
                row = rows.get(old.lower()) or rows.get(new.lower())
                if row and person["id"] not in subjects:
                    subjects[person["id"]] = row["serviceTokenClientId"]
            answers["officerRename"] = record = {**record, "subjects": subjects}
            save_answers(answers)
            fleet_records.rename(lambda method, path, body: api_call(token, method, path, body), answers["slug"],
                                 renames, say=say)
        # 3. The new certificates, kept beside the old ones for a rollback.
        for person, *_ in plan:
            if staged.get(person["id"]):
                current = INPUTS / "fleet.secrets" / person["id"] / runtime.IRC_CLIENT_PEM
                previous = INPUTS / "rename" / person["id"] / "irc-client.previous.pem"
                if current.is_file() and current.read_text() != staged[person["id"]].read_text() \
                        and not previous.exists():
                    write_private(previous, current.read_text())
                write_private(current, staged[person["id"]].read_text())
        answers["officerNames"] = "plain"
        save_answers(answers)
    # 4. Switch: the fleet's inputs, Stargate, the officers' configuration and the summary, as a re-run does. A run
    # that stops here, or whose certificates do not sign in yet, is resumed from here by the next one.
    say("running setup with the new names")
    configure(parse_args(["--non-interactive"]))
    if officers or pending:
        finish_rename(plan)


def finish_rename(plan):
    """The rename's last check: with certificate sign-in, every installed certificate signs in as its officer's new
    account. Only then is the rename recorded as switched, and what is left to remove printed."""
    answers = read_json(INPUTS / "answers.json", {})
    if answers.get("irc") == "remote" and answers.get("ircAuth") == "certificate":
        host, port, ca = answers["ircHost"], answers["ircPort"], answers.get("ircCa")
        wrong = []
        for person, _, new, *_ in plan:
            bundle = FLEET_ETC / "irc-certs" / f"{person['id']}.pem"
            try:
                signed = irc_accounts.check_login(host, port, ca, str(bundle)) if bundle.is_file() else None
            except irc_accounts.IrcError as error:
                raise SetupError(f"{error} The rename is not finished: run 'sudo subspace-setup rename-officers' "
                                 "again once the IRC server answers.") from None
            say(f"{new}: the installed certificate {'signs in as ' + signed if signed else 'does not sign in'}")
            if (signed or "").lower() != new.lower():
                wrong.append(new)
        require(not wrong, f"The installed certificate of {', '.join(wrong)} does not sign in as the new account on "
                f"{host}, so the rename is not finished: keep the old accounts. Check the new accounts' certificates "
                "there (NickServ CERT LIST), then run 'sudo subspace-setup rename-officers' again, which runs setup "
                "with the new names and checks again.")
    answers["officerRename"] = {**(answers.get("officerRename") or {}), "switched": True}
    save_answers(answers)
    for line in rename_cleanup(answers, plan, answers["officerRename"]):
        say(line)


def rename_crew(answers, value):
    """--crew-names for installed captains: each crew member keeps its ID, so its sessions, worktrees and queued
    tasks stay, and takes the new name. Needs a Stargate whose crew plugin lets the two differ."""
    config = read_json(INPUTS / "fleet.json", {}) or {}
    installed = {person["id"]: person for person in config.get("captains", []) if isinstance(person, dict)}
    named = parse_crew_names(value, answers)
    ids = {}
    for captain_id in named:
        person = installed.get(captain_id) or {}
        current = {**{slot: slot_label(slot) for slot in crew_slots(answers)}, **(person.get("crewNames") or {})}
        ids[captain_id] = {slot: (person.get("crewIds") or {}).get(slot) or runtime.crew_id(name)
                           for slot, name in current.items()}
    changed = [f"{captain_id} {slot}" for captain_id, names in named.items() for slot, name in names.items()
               if runtime.crew_id(name) != ids[captain_id].get(slot)]
    require(not changed or crew_names_supported(),
            "The installed Stargate's crew plugin requires a crew member's name to match its ID (Stargate "
            "src/coordination/crew/team.ts: 'Agent IDs must match assigned names'), and a crew member's ID cannot "
            "change without losing its sessions, worktrees and queued tasks. Naming an installed fleet's crew ("
            + ", ".join(changed) + ") needs a Stargate that drops that rule (installer README, 'Crew names'). "
            "Nothing was changed.")
    check_crew_answers({**answers, "crewNames": {**(answers.get("crewNames") or {}), **named}},
                       [ident(answers["rearAdmiral"], "Rear admiral"),
                        *(ident(name, "Captain") for name in answers["captains"])])
    return {"crewNames": named, "crewIds": ids}


def rename_irc_certificates(answers, plan, staged, operator):
    """Register each new account and bind the officer's new certificate, or print the lines that do; stop until
    every new certificate signs in as its new account. The old accounts keep their certificates meanwhile:
    Ergo binds a certificate fingerprint to one account only, so each new account gets a certificate of its own."""
    host, port, ca = answers["ircHost"], answers["ircPort"], answers.get("ircCa")
    certificates = {new: (staged[person["id"]], runtime.irc_fingerprint(staged[person["id"]].read_text(),
                                                                        str(staged[person["id"]])))
                    for person, _, new, *_ in plan}
    if operator:
        check_certificates(answers, None, certificates, operator, register=True)
        return
    missing = []
    try:
        for account, (path, fingerprint) in certificates.items():
            if irc_accounts.check_login(host, port, ca, str(path)) != account:
                missing.append((account, fingerprint))
    except irc_accounts.IrcError as error:
        raise SetupError(str(error)) from None
    if not missing:
        say("every new account signs in with its new certificate")
        return
    say(f"register the new accounts on {host} as an IRC operator with NickServ's accreg capability:")
    registered = registered_accounts()
    for account, fingerprint in missing:
        # An account an earlier run registered, and stopped before binding, needs only its certificate.
        if registered.get(account) != fingerprint:
            say(f"  /NS SAREGISTER {account}")
        say(f"  /NS CERT ADD {account} {fingerprint}")
    raise SetupError("The new IRC accounts are not registered yet. Send the lines above (or run rename-officers "
                     "again with --irc-operator), then run 'sudo subspace-setup rename-officers' again. The officers "
                     "keep signing in to their old accounts; nothing else was changed.")


def rename_irc_passwords(answers, members):
    """A shared IRC server that seeds accounts from each fleet's bundle: write the bundle with the new accounts
    and stop until they sign in with their passwords. The old accounts stay in Ergo until dropped."""
    store = Secrets(INPUTS, answers["slug"], answers.get("keyVault"))
    plain = {**answers, "officerNames": "plain"}
    bundle = irc_bundle(plain, members, store)
    host, port, ca = answers["ircHost"], answers["ircPort"], answers.get("ircCa")
    missing = [row["name"] for row in bundle["accounts"]
               if not irc_accounts.check_password(host, port, ca, row["name"], row["password"])]
    if missing:
        raise SetupError(f"{', '.join(missing)} cannot sign in to {host} yet. Add the fleet's new bundle there: copy "
                         f"{INPUTS / 'irc-bundle.json'} to the IRC server and run 'sudo subspace-setup irc-server add "
                         "<file>' (running the host installer again does it), then run 'sudo subspace-setup "
                         "rename-officers' again. The officers keep their old accounts meanwhile.")
    say("every new account signs in with its password")


def rename_cleanup(answers, plan, record):
    """What is left once the officers run under their plain names: steps outside the installer, and how to
    remove the old names once the new ones are verified."""
    old = record.get("from") or {}
    if not old:
        return []
    olds = [old[person["id"]] for person, *_ in plan if old.get(person["id"]) and old[person["id"]] != person["id"]]
    lines = ["once the officers are on IRC under their new names and the desktop shows their ships:"]
    if answers.get("irc") == "remote":
        lines += [f"  - on {answers['ircHost']}, if its Subspace API sign-in hook lists local-only accounts "
                  "(IRC_AUTH_LOCAL_ACCOUNTS), add " + " ".join(person["id"] for person, *_ in plan)
                  + " there and restart the IRC server; take the old names out when you drop them",
                  "  - drop the old accounts as an IRC operator: " + "; ".join(f"/NS ERASE {name}" for name in olds),
                  "  - the old ship channels (" + ", ".join(channel[0] for *_, channel in plan if channel)
                  + ") stay with their history; drop or unregister them with ChanServ when no longer needed"]
    else:
        lines.append("  - the fleet's Ergo keeps the old accounts; drop them as its operator with /NS ERASE "
                     + ", ".join(olds))
    if answers.get("apiUrl"):
        lines.append(f"  - Stargate kept the old key files beside the new ones in {GATEWAY_KEYS} ("
                     + ", ".join(f"{row[3]}.pem" for row in plan if row[3] != row[4]) + "); remove them")
    lines += ["  - the API keeps each old IRC nick retired for its own agent: no one else can take it",
              "  - desktops: run the host installer again (or update Settings > Channels) for the new ship channels"]
    return lines


def crew_names_supported():
    """Whether the installed Stargate lets a crew member's name differ from its ID."""
    source = installed_stargate_source()
    team = source / "src/coordination/crew/team.ts" if source else None
    try:
        return bool(team) and "Agent IDs must match assigned names" not in team.read_text()
    except OSError:
        return False


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
    elif argv and argv[0] == "rename-officers":
        rename_officers(argv[1:])
    elif argv and argv[0] == "toolchains":
        toolchains_command(argv[1:])
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
    except (SetupError, private_network.PrivateNetworkError) as error:
        sys.exit(f"subspace-setup: {error}")
    except KeyboardInterrupt:
        sys.exit(130)
