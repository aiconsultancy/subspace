#!/usr/bin/python3
"""Validate fleet inputs and converge the system configuration."""

import base64
import difflib
import fcntl
import grp
import hashlib
import hmac
import ipaddress
import json
import os
from pathlib import Path
import pwd
import re
import secrets
import socket
import ssl
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit

from agent_files import collect_cloud, private_directory, private_file, provision_cloud, write_private
import fleet_records
import memory
from network import rules
from roles import API_SKILL_MISSING, add_request_guidance, api_skill_path, default_permissions, provision_role
from roles import role_documents
from roles import provision_workspace, workspace_documents
import screenshots
import sessions
import standard_channels
import toolchains
from toolchains import Invalid
from workspaces import crew_worktree, prepare_workspaces, prune_worktrees


APP = Path("/opt/subspace-fleet")
ETC = Path("/etc/subspace-fleet")
STATE = Path("/var/lib/subspace-fleet")
SYSTEMD = Path("/etc/systemd/system")
PREFIX = "subspace-fleet-"
GATEWAY = "ss-fleet-gateway"
PROXY = "ss-fleet-proxy"
VIEW = "ss-fleet-view"
REPOS = "ss-fleet-repos"
MEDICAL = "ss-fleet-medical"
SENTRY_TOKEN = ("medical", "sentry-auth-token")
MEDICAL_IRC_PASSWORD = ("medical", "irc-password")
# IRC sign-in with certificates (SASL EXTERNAL, irc.auth "certificate"): one owner-only PEM bundle per officer,
# a certificate and its private key, owned by the gateway user that serves the officer. Stargate reads it
# as clientPemFile. An operator may supply one as <secrets-dir>/<officer-id>/irc-client.pem.
IRC_CERTS = ETC / "irc-certs"
IRC_CLIENT_PEM = "irc-client.pem"
# Stargate's own limits for clientPemFile (src/irc/client-pem.ts): at most 64 KiB, a certificate whose first
# one matches an unencrypted private key in one of these PEM forms.
IRC_PEM_BYTES = 64 * 1024
IRC_PEM_CERTIFICATE = re.compile(r"-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----")
IRC_PEM_KEY = re.compile(r"-----BEGIN (PRIVATE KEY|EC PRIVATE KEY|RSA PRIVATE KEY)-----[\s\S]*?-----END \1-----")
# Stargate's agent dispatch defaults (docs/agent-dispatch.md), each overridable under agentDispatch.
DISPATCH_DEFAULTS = {"pollIntervalSeconds": 120, "answerTimeoutSeconds": 300,
                     "retryLadderSeconds": [3600, 10800, 21600, 43200]}
# The gateway variable holding the rear admiral agent's API bearer; Stargate's configuration names only the variable.
DISPATCH_TOKEN_ENV = "SUBSPACE_DISPATCH_TOKEN"
JWT = r"[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+"
# Captains answer dispatch offers with Stargate's fleet plugin, through a per-captain socket that the gateway
# owns under STATE/fleet-control/<captain-id>/.
DISPATCH_GRANT = {"action": "fleet_answer_offer", "resource": "*", "effect": "allow"}
FLEET_ACTIONS = ("fleet_verify_instruction", "fleet_dispatch", "fleet_status", "fleet_review", "fleet_answer_offer")
FIREWALL = f"{PREFIX}firewall.service"
# The shared toolchain store and the fleet's mise configuration (lib/toolchains.py). Fleet services that run no
# officer's tools keep this environment; officers and crew run theirs with toolchains.officer_environment.
TOOL_HOME = toolchains.SHARED
TOOL_ENV = {"MISE_DATA_DIR": str(TOOL_HOME), "MISE_GLOBAL_CONFIG_FILE": str(ETC / "toolchains.toml"),
            "MISE_SYSTEM_CONFIG_DIR": str(TOOL_HOME / "system-config")}
FLEET_PATH = f"{APP}/bin:{TOOL_HOME}/shims:/usr/local/bin:/usr/bin:/bin"
STARGATE_REPO = "https://github.com/aiconsultancy/stargate.git"
# Stargate's latest release and the commit its tag names; every install gets exactly that commit. Agent
# dispatch on the rear admiral's gateway needs v0.43.0 or later. scripts/stargate-pin.py checks the two
# against GitHub and moves them together: see "Stargate pin" in subspace-installer/README.md.
STARGATE_RELEASE = "v0.43.0"
STARGATE_REF = "fcf6ac75d9c780878c06c8b49fb160e4a27832ff"
# A hosted Subspace API (api.enrolment): Stargate's subspaceAgents section, its signing proxy and its MCP bridge,
# first in this commit; a checkout has them when it has SUBSPACE_AGENTS_FILE. Releases up to v0.43 lack them.
SUBSPACE_AGENTS_COMMIT = "5630a5ce9ca1e7cd533bc7a5ab951bb4681757be"
SUBSPACE_AGENTS_FILE = "src/subspace/bridge.ts"
SUBSPACE_AGENTS_RELEASE = (0, 44, 0)
# Each officer's signing-proxy socket, in a directory of its own that admits the gateway and the officer's
# engine group only. /run is emptied at boot, so a tmpfiles.d entry makes the directories again.
AGENT_SOCKETS = Path("/run/subspace-fleet/agents")
AGENT_TMPFILES = Path("/etc/tmpfiles.d/subspace-fleet-agents.conf")
ENROLMENT_TOKEN = "subspace-enrolment-token"
ENROLMENT_TOKEN_TEXT = r"senr1\.[A-Za-z0-9_-]{64}"
# Fleet API browser origins: macOS Spaces, and the built-in Fleet Console's WebView2 origin on
# Windows, which Stargate stopped allowing by default in v0.41.0.
FLEET_API_ORIGINS = ["subspace://*", "https://subspace.fleet-console"]
SUBSPACE_REPO = "https://github.com/aiconsultancy/subspace.git"
LOOPBACK = "127.0.0.1"
# Matches the Ergo container's service group in subspace-irc/Dockerfile.
ERGO_GID = 10001
ACCOUNT = r"[a-zA-Z][a-zA-Z0-9_-]{0,31}"
ERGO_ACCOUNT = r"[a-z][a-z0-9_-]{0,31}"
UUID = r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}"
FAMILIES = {"claude": "claude-sonnet-5-5", "codex": "gpt-5.4"}
# Each officer role's default harness model and reasoning effort; fleet.json's harness and its captain and
# rearAdmiral blocks override them. Crew keep FAMILIES.
OFFICER_DEFAULTS = {
    "rear_admiral": {"family": "claude", "model": "claude-fable-5-2", "effort": "high"},
    "captain": {"family": "claude", "model": "claude-opus-5-5", "effort": "xhigh"},
}
# The harness block for each role, under fleet.json's harness.
ROLE_HARNESS = {"rear_admiral": "rearAdmiral", "captain": "captain"}
# Reasoning efforts per harness and family. OpenCode spells each as a variant of the model, which it derives
# from the provider package (Variant.resolve): anthropic-compatible gives Claude low to max, and
# openai-compatible responses gives low, medium and high. Stargate's Claude Agent SDK backend takes effort.
EFFORTS = {
    ("opencode", "claude"): ("low", "medium", "high", "xhigh", "max"),
    ("opencode", "codex"): ("low", "medium", "high"),
    ("claude-sdk", "claude"): ("low", "medium", "high", "xhigh", "max"),
}
# OpenCode provider IDs per model family, as in Stargate's OpenCode template.
PROVIDERS = {"claude": "claude-proxy", "codex": "openai-proxy"}
USAGE_GUARD_DEFAULTS = {"warning": 20, "cutoff": 10}
USAGE_GUARD_MODELS = {
    "claude": ["claude/", "claude-proxy/", "anthropic/"],
    "codex": ["codex/", "codex-proxy/", "openai-proxy/", "openai/gpt-", "openai/codex"],
}
USAGE_GUARD_SOURCE = "src/backends/quota/usage-guard.ts"

MODEL = r"[a-zA-Z0-9][a-zA-Z0-9._/:#-]{0,127}"
SESSIONS = ETC / "sessions"
# Where each captain's sessions start: "workspace", the directory above its repositories (the default), or
# "repository", its checkout of the first repository, as before. ETC/session-roots.json keeps each root.
ROOT_MODES = ("workspace", "repository")
MESIJ = APP / "bin/mesij"
CREW_ID = r"[a-z0-9][a-z0-9_-]{0,31}"
# Stargate's crew plugin accepts up to nine repositories and fifty member contexts per captain.
MAX_CREW_REPOSITORIES = 9
MAX_CREW_CONTEXTS = 50
# The captain's pinned root needs the crew tools; Stargate checks ownership on every call.
CREW_GRANTS = [
    *({"action": action, "resource": "*", "effect": "allow"} for action in ("stargate_crew", "stargate_mail", "stargate_history")),
    {"action": "stargate_model", "resource": "get", "effect": "allow"},
]


def require(condition, message):
    if not condition:
        raise Invalid(message)


def read_json(path, private=False):
    flags = os.O_RDONLY | os.O_NONBLOCK
    if private:
        flags |= os.O_NOFOLLOW
    try:
        with os.fdopen(os.open(path, flags), encoding="utf-8") as stream:
            info = os.fstat(stream.fileno())
            require(stat.S_ISREG(info.st_mode), f"Expected a regular file at {path}.")
            if private:
                require(info.st_mode & 0o077 == 0, f"Set private permissions on {path} (chmod 600).")
            return json.load(stream)
    except (OSError, ValueError) as error:
        if isinstance(error, Invalid):
            raise
        raise Invalid(f"Cannot read JSON from {path}.") from None


def fields(value, label, allowed, required=()):
    require(isinstance(value, dict), f"{label} must be an object.")
    require(value.keys() <= set(allowed.split()), f"{label} contains an unknown field.")
    require(value.keys() >= set(required), f"{label} is missing a required field.")
    return value


def pattern(value, expression, label):
    require(isinstance(value, str) and re.fullmatch(expression, value), f"Invalid {label}.")
    return value


def string(value, label, maximum=100, minimum=1):
    require(
        isinstance(value, str) and minimum <= len(value) <= maximum
        and all(ord(char) >= 32 and ord(char) != 127 for char in value)
        and (not value or value.strip()),
        f"Invalid {label}.",
    )
    return value


def ergo_password(value):
    value = string(value, "Ergo password", 72)
    require(
        len(value.encode()) <= 72 and value != "*" and not value.startswith(":")
        and not any(char.isspace() for char in value),
        "Ergo passwords must fit 72 bytes and avoid whitespace, a leading colon and the value '*'.",
    )


def port(value):
    require(type(value) is int and 1 <= value <= 65535, "Expected a port from 1 to 65535.")
    return value


def boolean(value):
    require(type(value) is bool, "Expected true or false.")
    return value


def channel(value):
    return pattern(value, r"#[a-z0-9][a-z0-9_-]{0,48}", "IRC channel")


def not_standard(slug, label, value, expected, plain=False):
    """Refusal for a renamed standard channel: every fleet keeps the standard names."""
    shown = value if isinstance(value, str) and len(value) <= 64 and value.isprintable() else "another name"
    return (f"{label} is {shown}, but it must be {expected}. Every fleet has the standard channels "
            f"{standard_channels.summary(slug, plain)}, and the desktop recognises them by name, so they cannot be "
            "renamed or dropped. Leave the setting out or use the standard name. Extra channels are welcome: "
            "seed them with irc.compose.channels on local IRC, register them on a shared server, or list them "
            "on an initiative.")


def ip(value):
    return str(ipaddress.ip_address(string(value, "IP address")))


def host(value):
    name = string(value, "host", 253)
    try:
        return ip(name)
    except ValueError:
        return pattern(name, r"[a-zA-Z0-9](?:[a-zA-Z0-9.-]*[a-zA-Z0-9])?", "host").lower()


def absolute_file(value, check=True):
    value = string(value, "absolute file path", 1024)
    require(Path(value).is_absolute() and (not check or Path(value).is_file()), "Expected an existing absolute file path.")
    return value


def url(value, label, loopback_http=False, paths=None, websocket=False):
    value = string(value, label, 2048)
    parts = urlsplit(value)
    local = parts.hostname in {"localhost", LOOPBACK, "::1"}
    secure = "wss" if websocket else "https"
    plain = "ws" if websocket else "http"
    require(
        parts.hostname and (parts.scheme == secure or (loopback_http and local and parts.scheme == plain))
        and not parts.username and not parts.password and "@" not in parts.netloc
        and not parts.query and not parts.fragment and (paths is None or parts.path in paths),
        f"Invalid {label}. Use a secure URL without credentials, queries or fragments.",
    )
    if parts.port is not None:
        port(parts.port)
    return value


def source(value, label, repo, ref=None):
    value = fields(value, label, "repo ref")
    repo = pattern(value.get("repo", repo), r"https://[a-zA-Z0-9.-]+(?::[0-9]+)?/[a-zA-Z0-9._/-]+", f"{label} Git URL")
    ref = pattern(value.get("ref", ref), r"[0-9a-f]{40}", f"{label} full commit SHA")
    return {"repo": repo, "ref": ref}


def member(value):
    require(not isinstance(value, dict) or "crewGitHubApps" not in value,
            "Remove crewGitHubApps: crew run as their captain and commit and push with the captain's githubApp.")
    value = fields(value, "fleet member", "id name account channel git githubApp harness crew crewNames crewIds serviceTokenClientId sessionRoot slack memory", ["id", "name"])
    pattern(value["id"], r"[a-z][a-z0-9-]{0,19}", "member ID")
    require(not value["id"].startswith("fleet-"), "Member IDs cannot start with fleet-.")
    require(value["id"] != SENTRY_TOKEN[0], "The medical member ID is reserved for integration credentials.")
    require(value["id"] not in {"all", "captain"}, "The member IDs all and captain are reserved for crew messaging.")
    string(value["name"], "member name")
    # An officer's IRC account: its ID in a fleet with plain names, where it may be left out (validate()).
    require("account" not in value or isinstance(value["account"], str) and re.fullmatch(ACCOUNT, value["account"]),
            f"Invalid SASL account for member {value['id']}: start with a letter and use at most "
            f"{standard_channels.MAX_ACCOUNT} letters, digits, hyphens or underscores, the IRC limit for "
            "accounts and nicks.")
    # A member's channel, if given, must be its standard channel; validate() checks it against the roster.
    if "git" in value:
        git = fields(value["git"], "git", "email signingKey")
        if "email" in git:
            pattern(git["email"], r"[^\s@]+@[^\s@]+", "Git email")
        if "signingKey" in git:
            path = string(git["signingKey"], "SSH signing key path", 1024)
            require(Path(path).is_absolute() and path.endswith(".pub"), "Git signingKey must name an absolute SSH public key file.")
    if "githubApp" in value:
        github_app(value["githubApp"])
    if "serviceTokenClientId" in value:
        # The member's account subject at the API's identity provider, for its API agent record.
        string(value["serviceTokenClientId"], f"Member {value['id']}'s serviceTokenClientId", 200)
    if "sessionRoot" in value:
        # Where a captain's sessions start: its workspace (the default) or its first repository's checkout.
        pattern(value["sessionRoot"], "|".join(ROOT_MODES), f"sessionRoot for {value['id']} (workspace or repository)")
    if "slack" in value:
        officer_slack(value["slack"], value["id"])
    return value


def memory_config(parse, *args):
    """fleet.json's memory settings (lib/memory.py), refused as invalid configuration."""
    try:
        return parse(*args)
    except memory.Refused as error:
        raise Invalid(str(error)) from None


def github_app(value):
    keys = "appId installationId clientId botLogin botId"
    app = fields(value, "githubApp", keys + " contents", keys.split())
    for key in ("appId", "installationId", "botId"):
        require(type(app[key]) is int and app[key] > 0, f"GitHub {key} must be a positive integer.")
    pattern(app["clientId"], r"[a-zA-Z0-9_.-]{1,64}", "GitHub App client ID")
    pattern(app["botLogin"], r"[a-zA-Z0-9][a-zA-Z0-9-]{0,99}\[bot\]", "GitHub App bot login")
    if "contents" in app:
        pattern(app["contents"], r"read|write", "GitHub App Contents access")
    return app


# Slack Socket Mode tokens, as files in an officer's secrets directory and the prefix each must have. They never
# go in fleet.json; the gateway reads them from its private environment, under the names slack_env() gives.
SLACK_TOKEN_FILES = {"slack-app-token": "xapp", "slack-bot-token": "xoxb"}


def officer_slack(value, agent_id):
    """An officer's own Slack bot: its app, and explicit lists of who may address it and where."""
    label = f"Member {agent_id}'s slack"
    slack = fields(value, label, "appId users channels mentionOnly", ["appId", "users", "channels"])
    pattern(slack["appId"], r"A[A-Z0-9]+", f"{label}.appId: expected a Slack app ID (A...)")
    for key, expression, kind in (("users", r"[UW][A-Z0-9]+", "member IDs (U... or W...)"),
                                  ("channels", r"[CGD][A-Z0-9]+", "conversation IDs (C..., G... or D...)")):
        entries = slack[key]
        # Nobody may address the bot by default: Stargate's * is refused, as for stakeholders.slack.
        require(isinstance(entries, list) and entries,
                f"{label}.{key} must list explicit Slack {kind}; nobody may address the bot by default.")
        for entry in entries:
            pattern(entry, expression, f"{label}.{key} entry: use explicit Slack {kind}, not *")
        require(len(set(entries)) == len(entries), f"{label}.{key} lists an ID twice.")
    if "mentionOnly" in slack:
        require(type(slack["mentionOnly"]) is bool, f"{label}.mentionOnly must be true or false.")
    return slack


def slack_env(agent_id):
    """The gateway environment variables that hold an officer's Slack app-level and bot tokens."""
    suffix = agent_id.upper().replace("-", "_")
    return f"SLACK_APP_TOKEN_{suffix}", f"SLACK_BOT_TOKEN_{suffix}"


def slack_bot_id(agent_id):
    """The officer's Stargate bot ID. Member IDs never start with fleet-, so it cannot meet an IRC registration's."""
    return f"fleet-slack-{agent_id}"


def officer_title(person):
    return f"{'Rear admiral' if person.get('role') == 'rear_admiral' else 'Captain'} {person['id']}"


def check_officer_slack(agents, stakeholder):
    """Officers' own Slack bots: each a distinct Slack app, on an OpenCode officer.

    The rear admiral's own bot replaces stakeholders.slack, which reads the same token files.
    """
    apps = [stakeholder["appId"]] if stakeholder else []
    for person in agents:
        if "slack" not in person:
            continue
        require(person["harness"]["type"] == "opencode",
                f"{officer_title(person)} uses claude-sdk and cannot have its own Slack bot: Stargate's Claude "
                "backend has no root session for Slack and IRC to share, and its gateway runs as the officer, whose "
                "model could read the Slack tokens. Remove its slack, or use the opencode harness.")
        require(not (person["role"] == "rear_admiral" and stakeholder),
                "Give the rear admiral one Slack bot: rearAdmiral.slack shares its root session with IRC, and "
                "stakeholders.slack keeps sessions of its own. Both read the same token files; remove one.")
        apps.append(person["slack"]["appId"])
    require(len(set(apps)) == len(apps), "Each Slack bot needs its own Slack app: an appId appears twice.")


def slack_members(cfg):
    """The officers whose secrets directories hold Slack tokens: each with its own bot, and the rear admiral for
    stakeholders.slack."""
    return [person["id"] for person in cfg["members"]
            if "slack" in person or person["role"] == "rear_admiral" and cfg["integrations"]["slack"]]


def slack_app_id(cfg, agent_id):
    """The Slack app whose tokens an officer's secrets directory holds: its own bot's, else stakeholders.slack's."""
    person = next(person for person in cfg["members"] if person["id"] == agent_id)
    return (person.get("slack") or cfg["integrations"]["slack"])["appId"]


def slack_owner(cfg, agent_id):
    return "the rear admiral's" if agent_id == cfg["members"][0]["id"] else f"captain {agent_id}'s"


def officer_slack_bot(person, registration, tokens):
    """The officer's Stargate Slack bot and the environment it reads, or (None, {}) without one.

    The bot takes the backend of the officer's IRC registration, its pinned sessionID included: Stargate lets one
    Slack bot and one IRC registration pin the same OpenCode root, so a request on either reaches that session.
    Each still delivers only the replies to its own requests.
    """
    slack = person.get("slack")
    if not slack:
        return None, {}
    require(tokens and SLACK_TOKEN_FILES.keys() <= tokens.keys() and tokens.get("appId", slack["appId"]) == slack["appId"],
            f"Supply slack-app-token and slack-bot-token for Slack app {slack['appId']} in {person['id']}'s secrets "
            "directory.")
    app_env, bot_env = slack_env(person["id"])
    bot = {
        "id": slack_bot_id(person["id"]), "displayName": person["name"], "backend": dict(registration["backend"]),
        "slack": {"appId": slack["appId"], "appTokenEnv": app_env, "botTokenEnv": bot_env},
        "access": {"users": slack["users"], "channels": slack["channels"],
                   "mentionOnly": slack.get("mentionOnly", True)},
    }
    return bot, {app_env: tokens["slack-app-token"], bot_env: tokens["slack-bot-token"]}


def slack_plan(cfg):
    """What --dry-run says about officers' own Slack bots, one line each."""
    lines = []
    for person in cfg["members"]:
        if not (slack := person.get("slack")):
            continue
        app_env, bot_env = slack_env(person["id"])
        if person["role"] == "rear_admiral" or person["crew"]:
            session = "shares its pinned root session with its IRC registration"
        else:
            session = ("has no pinned root without crew, so Slack threads and IRC conversations keep separate "
                       "sessions")
        lines.append(
            f"Slack: {officer_title(person)} answers as Slack app {slack['appId']} ({slack_bot_id(person['id'])}), "
            f"to users {', '.join(slack['users'])} in {', '.join(slack['channels'])}"
            f"{', when mentioned' if slack.get('mentionOnly', True) else ''}; it {session}. Its tokens come from "
            f"{person['id']}/slack-app-token and slack-bot-token in the secrets directory, as {app_env} and {bot_env}.")
    return lines


def rear_root(cfg):
    """Whether the rear admiral has a pinned root: agent dispatch needs one, and its own Slack bot shares it."""
    return bool(cfg["agentDispatch"] or "slack" in cfg["members"][0])


def listener(value, label, default_port, allow_bind=False):
    value = fields(value, label, "port bind" if allow_bind else "port")
    return {"port": port(value.get("port", default_port)), "bind": ip(value.get("bind", LOOPBACK))}


def irc_config(value, slug, check_files, plain=False):
    value = fields(value, "irc", "mode host port tls caFile allowedAccounts commandChannel starbaseChannel advertise compose auth", ["mode", "allowedAccounts"])
    mode = pattern(value["mode"], r"local|remote", "IRC mode")
    tls = boolean(value.get("tls", mode == "remote"))
    require(mode == "local" or tls, "Remote IRC requires TLS.")
    auth = irc_auth(value, mode, tls)
    server_host = host(value.get("host", LOOPBACK if mode == "local" else None))
    server_port = port(value.get("port", 6697 if tls else 6667))
    if mode == "local":
        require(not tls and server_host == LOOPBACK, "Local fleet members use loopback IRC. Configure advertise for desktop TLS.")
    allowed = value["allowedAccounts"]
    require(isinstance(allowed, list) and 1 <= len(allowed) <= 50, "Specify 1 to 50 allowed IRC accounts.")
    for account in allowed:
        pattern(account, ACCOUNT, "allowed IRC account")
    require(len({name.lower() for name in allowed}) == len(allowed), "Allowed IRC accounts must be unique.")
    irc = {
        "mode": mode, "host": server_host, "port": server_port, "tls": tls, "allowedAccounts": allowed,
        "starbaseChannel": standard_channels.starbase(slug), "commandChannel": standard_channels.command(slug),
    }
    irc["auth"] = auth
    for key, label in (("starbaseChannel", "irc.starbaseChannel"), ("commandChannel", "irc.commandChannel")):
        if key in value:
            require(value[key] == irc[key], not_standard(slug, label, value[key], irc[key], plain))
    if "caFile" in value:
        require(tls, "IRC caFile requires TLS.")
        irc["caFile"] = absolute_file(value["caFile"], check_files)
    if "compose" in value:
        require(mode == "local", "IRC compose settings require local mode.")
    if mode == "local":
        compose = fields(value.get("compose", {}), "irc.compose", "repo ref tlsBind tlsPort tlsCert tlsKey serverName operator channels history", ["operator"])
        irc["compose"] = source({key: compose[key] for key in ("repo", "ref") if key in compose}, "IRC source", SUBSPACE_REPO)
        for key in ("tlsCert", "tlsKey"):
            if key in compose:
                irc["compose"][key] = absolute_file(compose[key], check_files)
        bind = ip(compose.get("tlsBind", LOOPBACK))
        require(bind == LOOPBACK or {"tlsCert", "tlsKey", "serverName"} <= compose.keys(), "An exposed IRC TLS listener requires tlsCert, tlsKey and serverName.")
        require(("tlsCert" in compose) == ("tlsKey" in compose), "Specify both IRC TLS files.")
        server_name = pattern(
            compose.get("serverName", "irc.example.com"),
            r"[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)*",
            "Ergo server name",
        )
        channels = compose.get("channels", [])
        require(isinstance(channels, list), "IRC compose channels must be an array.")
        irc["compose"].update(
            tlsBind=bind, tlsPort=port(compose.get("tlsPort", 6697)),
            serverName=server_name, operator=pattern(compose["operator"], ERGO_ACCOUNT, "Ergo operator account"),
            channels=[channel(name) for name in channels],
            history=history_config(compose.get("history", {})),
        )
    if "advertise" in value:
        advertised = fields(value["advertise"], "irc.advertise", "host port tls websocketUrl", ["host", "port", "tls"])
        irc["advertise"] = {"host": host(advertised["host"]), "port": port(advertised["port"]), "tls": boolean(advertised["tls"])}
        if "websocketUrl" in advertised:
            irc["advertise"]["websocketUrl"] = url(advertised["websocketUrl"], "IRC WebSocket URL", True, websocket=True)
    return irc


def irc_auth(value, mode, tls):
    """How officers sign in to IRC: certificates (SASL EXTERNAL) by default on a remote server, else passwords."""
    auth = pattern(value.get("auth", "certificate" if mode == "remote" else "password"), r"certificate|password",
                   "irc.auth (certificate or password)")
    if auth == "certificate":
        # Stargate takes exactly one of passwordEnv and clientPemFile per account, and clientPemFile needs TLS.
        require(mode == "remote", "irc.auth certificate needs a remote IRC server for now. The fleet reaches the "
                "Ergo this installer manages over plain loopback IRC, and Stargate presents a client certificate "
                "only over TLS. Leave irc.auth out, or set it to password (README, 'IRC sign-in').")
        require(tls, "IRC certificates (SASL EXTERNAL) need TLS: set irc.tls to true, or irc.auth to password.")
    return auth


def history_config(value):
    """Ergo's persistent history: SQLite in its volume, or a PostgreSQL database such as the API's."""
    value = fields(value, "irc.compose.history", "database host port user name network sslMode")
    database = pattern(value.get("database", "sqlite"), r"sqlite|postgresql", "IRC history database (sqlite or postgresql)")
    if database == "sqlite":
        require(value.keys() <= {"database"}, "SQLite IRC history takes no connection settings.")
        return {"database": "sqlite"}
    require("host" in value, "PostgreSQL IRC history needs a host.")
    history = {
        "database": "postgresql", "host": host(value["host"]), "port": port(value.get("port", 5432)),
        "user": pattern(value.get("user", "ergo"), r"[A-Za-z0-9_]{1,63}", "IRC history user"),
        "name": pattern(value.get("name", "ergo_history"), r"[A-Za-z0-9_]{1,63}", "IRC history database name"),
        "sslMode": pattern(value.get("sslMode", "disable"), r"disable|require|verify-ca|verify-full", "IRC history sslMode"),
    }
    if "network" in value:
        # An existing Docker network that reaches the database, such as the API's Compose network.
        history["network"] = pattern(value["network"], r"[A-Za-z0-9][A-Za-z0-9_.-]{0,63}", "IRC history Docker network")
    return history


def api_config(value):
    value = fields(value, "api", "url initiatives vmHost fleetApiUrl agentUrl enrolment", ["url"])
    base = url(value["url"], "API base URL", True, ("", "/", "/api", "/api/")).rstrip("/")
    initiatives = value.get("initiatives", [])
    require(isinstance(initiatives, list), "API initiatives must be an array.")
    ids = [pattern(item, UUID, "initiative UUID").lower() for item in initiatives]
    cfg = {"url": base if base.endswith("/api") else f"{base}/api", "initiatives": list(dict.fromkeys(ids))}
    if "vmHost" in value:
        cfg["vmHost"] = host(value["vmHost"])
    if value.get("enrolment") is not None:
        require(type(value["enrolment"]) is bool, "api.enrolment must be true or false.")
    if value.get("enrolment"):
        # A hosted API: officers enrol their own keys with the fleet's enrolment token, and Stargate signs their
        # requests; nothing names an agent by a header.
        require("agentUrl" not in value, "api.enrolment and api.agentUrl exclude each other: an enrolled fleet's "
                "officers reach the API through Stargate, never through an agent listener.")
        parts = urlsplit(base)
        cfg["origin"] = f"{parts.scheme}://{parts.netloc.lower()}"
        cfg["enrolment"] = True
        require(api_skill_path() is not None, API_SKILL_MISSING)
    if "agentUrl" in value:
        # The API's agent-only listener, on this VM's loopback: agents name themselves there and need no token.
        agent_url = url(value["agentUrl"], "API agent listener URL", True, ("", "/"))
        require(urlsplit(agent_url).hostname in {"localhost", LOOPBACK, "::1"},
                "api.agentUrl must be the agent listener on this VM's loopback.")
        cfg["agentMcpUrl"] = f"{agent_url.rstrip('/')}/mcp"
        # Officers then get the Subspace API skill: refuse the configuration now, naming the file, if it is missing.
        require(api_skill_path() is not None, API_SKILL_MISSING)
    if "fleetApiUrl" in value:
        # Where a desktop reaches Stargate's Fleet API, when not through the SSH tunnel's loopback end.
        try:
            cfg["fleetApiUrl"] = fleet_records.fleet_api_url(value["fleetApiUrl"], "api.fleetApiUrl")
        except fleet_records.Refused as error:
            raise Invalid(str(error)) from None
    return cfg


def dispatch_seconds(value, label, low, high):
    require(type(value) is int and low <= value <= high, f"{label} must be a whole number of seconds from {low} to {high}.")
    return value


def agent_dispatch_config(value, api, agents):
    """Stargate's agent dispatch on the rear admiral's gateway, or None while it is off (the default).

    Timings default to Stargate's own and use its ranges, so the generated block always passes its check.
    """
    value = fields(value, "agentDispatch", "enabled pollIntervalSeconds answerTimeoutSeconds retryLadderSeconds")
    settings = {
        "pollIntervalSeconds": dispatch_seconds(value.get("pollIntervalSeconds", DISPATCH_DEFAULTS["pollIntervalSeconds"]),
                                                "agentDispatch.pollIntervalSeconds", 30, 3600),
        "answerTimeoutSeconds": dispatch_seconds(value.get("answerTimeoutSeconds", DISPATCH_DEFAULTS["answerTimeoutSeconds"]),
                                                 "agentDispatch.answerTimeoutSeconds", 60, 3600),
    }
    ladder = value.get("retryLadderSeconds", DISPATCH_DEFAULTS["retryLadderSeconds"])
    require(isinstance(ladder, list) and len(ladder) <= 10,
            "agentDispatch.retryLadderSeconds must be an array of at most 10 waits ([] means a single round).")
    settings["retryLadderSeconds"] = [dispatch_seconds(wait, "Each agentDispatch.retryLadderSeconds entry", 60, 604800)
                                      for wait in ladder]
    if not boolean(value.get("enabled", False)):
        return None
    require(api is not None, "Agent dispatch is on, but this fleet has no Subspace API registration. The rear admiral's "
            "gateway polls the API for issues: set api.url (with apiToken) or switch agentDispatch.enabled off.")
    rear, captains = agents[0], agents[1:]
    require(captains, "Agent dispatch is on, but the fleet has no captains to offer issues to. Add a captain or switch "
            "agentDispatch.enabled off.")
    require(rear["harness"]["type"] == "opencode",
            "Agent dispatch is on, but the rear admiral uses claude-sdk. Dispatch runs in the rear admiral's gateway, "
            "which for a Claude Agent SDK rear admiral runs as the rear admiral's own Linux user, so its model could read "
            "the dispatcher's API token; and captains recognise their rear admiral by its pinned OpenCode session, which "
            "a Claude Agent SDK harness cannot have. Use an OpenCode rear admiral or switch agentDispatch.enabled off.")
    # Stargate's answer tool needs a captain with a pinned OpenCode root, which only crew-hosting captains have.
    require(any(person["crew"] for person in captains),
            "Agent dispatch is on, but no captain can answer an offer: Stargate's fleet_answer_offer tool needs a captain "
            "with a pinned OpenCode session, and only captains with crew (an OpenCode harness, crew counts above 0 and "
            "at least one repository) have one. Give a captain crew or switch agentDispatch.enabled off.")
    return settings


def fleet_ipc_policy(rules, label):
    """Stargate's fleet IPC check (src/fleet/permissions.ts): rules that could match a fleet tool must be
    exact, '*' or a literal prefix followed by '*', and allow or deny, never ask."""
    literal = r"[A-Za-z0-9_./:@%#=-]+"

    def supported(value):
        return value == "*" or re.fullmatch(literal, value) or (value.endswith("*") and re.fullmatch(literal, value[:-1]))

    for rule in rules:
        action = rule["action"]
        if supported(action):
            prefix = action[:-1] if action.endswith("*") else None
            if not any(name == action or (prefix is not None and name.startswith(prefix)) for name in FLEET_ACTIONS):
                continue
        require(supported(action) and supported(rule["resource"]) and rule["effect"] in ("allow", "deny"),
                f"{label}: agent dispatch gives this captain Stargate's fleet tools, which accept only explicit allow or "
                f"deny rules with exact, '*' or prefix-'*' patterns for fleet actions. Change the rule for {action!r} "
                "(an ask, or a pattern with other wildcards, cannot authorise a fleet tool).")


def model_family(model):
    """The family whose provider serves a model ID: claude-* through claude-proxy, any other through openai-proxy."""
    return "claude" if model.startswith("claude-") else "codex"


def claude_efforts(model):
    """The efforts OpenCode 2.3.2 gives a Claude model through anthropic-compatible (Variant.resolve): Claude
    4.6 has no xhigh, and Claude 3.7 and 4.0 to 4.5 (but Opus 4.5) think on a token budget, as high and max."""
    # Versions are one or two digits, so a date such as claude-opus-4-20250514's is not read as a version, and
    # the version-first spelling (claude-3-7-sonnet-20250219) is tried first.
    found = re.search(r"claude-(\d{1,2})(?:[.-](\d{1,2}))?-(opus|sonnet|haiku|fable|mythos)(?![a-z])", model, re.I)
    if found:
        family, major, minor = found.group(3), found.group(1), found.group(2)
    else:
        found = re.search(r"(?:claude-)?(opus|sonnet|haiku|fable|mythos)-(\d{1,2})(?:[.-](\d{1,2}))?(?!\d)", model, re.I)
        if not found:
            return EFFORTS["opencode", "claude"]
        family, major, minor = found.group(1), found.group(2), found.group(3)
    major, minor = int(major), int(minor or 0)
    if (major, minor) == (3, 7) or major == 4 and minor < 6 and (family.lower(), minor) != ("opus", 5):
        return ("high", "max")
    if (major, minor) == (4, 6):
        return ("low", "medium", "high", "max")
    return EFFORTS["opencode", "claude"]


def permission_rules(rules, label):
    """An ordered list of OpenCode permission rules, each with action, resource and effect."""
    require(isinstance(rules, list), f"{label} must be an array of OpenCode permission rules.")
    for rule in rules:
        fields(rule, "permission", "action resource effect", ["action", "resource", "effect"])
        pattern(rule["effect"], r"allow|deny|ask", "permission effect")
        string(rule["action"], "permission action")
        string(rule["resource"], "permission resource", 1024)
    return rules


def harness_config(value, inherited):
    value = fields(value, "harness", "type model family effort permissions extraPermissions allowedTools")
    backend = pattern(value.get("type", inherited["type"]), r"opencode|claude-sdk", "harness type (opencode or claude-sdk)")
    if "model" in value and "family" not in value and isinstance(value["model"], str):
        # A model named without its family goes where its ID says, as subspace-setup writes it: claude-* through
        # the Claude family and any other through Codex, whatever the default family is.
        value = {**value, "family": model_family(value["model"])}
    family = pattern(value.get("family", inherited["family"]), r"claude|codex", "model family")
    candidate = value.get("model", inherited["model"] if family == inherited["family"] else FAMILIES[family])
    model = pattern(candidate, MODEL if backend == "opencode" else r"[a-zA-Z0-9][a-zA-Z0-9._:\[\]-]{0,127}", "CLIProxyAPI model ID")
    require(model_family(model) == family, f"Harness model {model} does not belong to the {family} family: Claude "
            "models (claude-*) go through the claude family and any other through codex. Set harness family to "
            f"{model_family(model)}, or leave it out.")
    require("#" not in model, f"Harness model {model} names an OpenCode variant: give the model ID alone and the "
            "reasoning effort as harness effort.")
    # An effort belongs to its model: a harness that names another model has the provider's default effort
    # unless it names one too.
    same = (family, model) == (inherited["family"], inherited["model"])
    effort = value.get("effort", inherited.get("effort") if same else None)
    if effort is not None:
        offered = EFFORTS.get((backend, family), ())
        if backend == "opencode" and family == "claude":
            offered = claude_efforts(model)
        require(effort in offered, f"Reasoning effort {effort!r} is not available for {model} with the {backend} "
                f"harness, which offers {', '.join(offered) or 'none'}." if offered else
                f"The {backend} harness cannot set a reasoning effort for the {family} family. Remove harness effort.")
    tools = value.get("allowedTools", inherited.get("allowedTools", []))
    require(isinstance(tools, list), "Claude allowedTools must be an array.")
    for tool in tools:
        pattern(tool, r"Agent|Bash|Edit|Glob|Grep|LSP|NotebookEdit|Read|TodoWrite|WebFetch|WebSearch|Write", "Claude built-in tool")
    require(len(set(tools)) == len(tools), "Claude tools must be unique.")
    if backend == "claude-sdk":
        require(family == "claude" and not {"permissions", "extraPermissions"} & value.keys(),
                "Claude Agent SDK requires the Claude family and uses allowedTools.")
    else:
        require("allowedTools" not in value, "OpenCode uses permissions.")
    permissions = value.get("permissions", inherited["permissions"])
    require("permissions" not in value or isinstance(permissions, list) and permissions, "Specify a non-empty list of OpenCode permissions.")
    permission_rules(permissions or [], "harness.permissions")
    # Rules that extend the policy rather than replace it: the fleet's, then the role's, then the officer's own.
    extra = [*inherited.get("extraPermissions", []),
             *permission_rules(value.get("extraPermissions", []), "harness.extraPermissions")]
    return {"type": backend, "family": family, "model": model, "effort": effort, "permissions": permissions,
            "extraPermissions": extra, "allowedTools": tools}


def crew_config(value, inherited):
    """Crew counts and models per family. Crew run in their captain's OpenCode server."""
    value = fields(value, "crew", "claude codex")
    teams = {}
    for family in FAMILIES:
        team = value.get(family, {})
        require(isinstance(team, dict), f"{family} crew must be an object.")
        require("type" not in team, "Crew have no harness type: they run inside their captain's OpenCode server "
                "through Stargate's crew plugin, so claude-sdk crew are not supported. Remove crew type.")
        require(not {"permissions", "allowedTools"} & team.keys(), "Crew tool access comes from Stargate's crew "
                "coding profile. Remove crew permissions and allowedTools.")
        team = fields(team, f"{family} crew", "count model")
        count = team.get("count", inherited[family]["count"])
        require(type(count) is int and 0 <= count <= 10, "Crew count per family must be between 0 and 10.")
        model = pattern(team.get("model", inherited[family]["model"]), MODEL, "crew CLIProxyAPI model ID")
        teams[family] = {"count": count, "model": model}
    return teams


def crew_id(name):
    """Stargate derives a crew ID from its display name (agentIDForName)."""
    return re.sub(r"[^a-z0-9_-]+", "-", name.strip().lower()).strip("-")


def crew_slot_label(worker):
    """A crew member's model family and slot, such as 'Claude-family crew slot claude-1': shown beside its
    name (in the Fleet API's role, the captain's roster and the dry run), never as the name."""
    return f"{worker['family'].title()}-family crew slot {worker['slot']}"


def crew_roster(captain, teams, names, slug, ids=None):
    """A captain's crew: one member per slot (claude-1, codex-2, ...), named by crewNames. Stargate's crew
    plugin derives a member's ID from its name, and keeps its state, worktrees and sessions under that ID, so
    a slot without a name of its own is called after its slot ('Claude 1'), as in fleets installed before crew
    names were asked for. ids (crewIds) keeps an installed member's ID when its name changes (rename-officers),
    which needs a Stargate whose crew plugin lets a name differ from its ID (stargate_crew_names_free)."""
    roster = []
    for family, team in teams.items():
        for slot in range(1, team["count"] + 1):
            key = f"{family}-{slot}"
            name = string(names.get(key, f"{family.title()} {slot}"), "crew display name").strip()
            require(re.fullmatch(r"[A-Za-z0-9 _-]+", name) and re.fullmatch(CREW_ID, crew_id(name)),
                    "Crew names use ASCII letters, digits, spaces, hyphens and underscores, and shorten to "
                    "an ID of at most 32 characters.")
            member_id = (ids or {}).get(key) or crew_id(name)
            roster.append({
                "id": member_id, "slot": key, "family": family, "name": name, "named": key in names,
                "model": team["model"],
                # Native OpenCode agent and initial worktree branch, both namespaced by fleet and captain.
                "agent": f"{captain['id']}-crew-{member_id}", "branch": f"crew/{slug}-{captain['id']}-{member_id}",
            })
    ids = [person["id"] for person in roster]
    require(len(set(ids)) == len(ids) and not set(ids) & {captain["id"], "captain", "all"},
            "Crew names must give distinct IDs that differ from the captain's ID, captain and all.")
    # Stargate's team rule, which a kept ID (crewIds) makes reachable: compared without case, a crew member's name
    # is no other member's name or ID, nor the captain's ID, captain or all. Without crewIds each ID is its name's.
    taken = {captain["id"]: f"captain {captain['id']}'s ID", "captain": "the alias captain", "all": "the alias all"}
    for worker in roster:
        taken.setdefault(worker["id"], f"the ID of {worker['name']} ({worker['id']})")
    for worker in roster:
        key = crew_id(worker["name"])
        clash = taken.get(key) if key != worker["id"] else None
        require(clash is None, f"Captain {captain['id']}'s crew member {worker['name']} ({worker['id']}) has the "
                f"same name as {clash}, ignoring case. In a captain's crew a name is no other member's name or ID.")
        taken[key] = f"{worker['name']} ({worker['id']})"
    return roster


def names_plan(cfg):
    """--dry-run: each officer's one name (IRC account, API name, Stargate ID) and its ship channel, and each
    captain's crew by name, with the model family, slot and model beside each."""
    plain = cfg["officerNames"] == "plain"
    lines = ["Officer names (IRC account and nick, Subspace API agent name, Stargate ID): " + (
        "plain, one name each." if plain else
        "fleet-prefixed, as installed; 'subspace-setup rename-officers' gives each officer its plain name "
        "(installer README, 'Officer names').")]
    for person in cfg["members"]:
        role = "Rear admiral" if person["role"] == "rear_admiral" else "Captain"
        api = fleet_records.agent_name(cfg, person)
        same = api == person["account"] == person["id"]
        names = person["account"] if same else (f"IRC {person['account']}, API {api}, Stargate {person['id']}")
        lines.append(f"  {role} {person['name']}: {names}, channel {person['channel']}")
        if person["crew"]:
            # A crew member that keeps its installed ID under a new name (crewIds) shows the ID too.
            lines.append("    crew: " + "; ".join(
                f"{worker['name']} ({worker['family'].title()} {worker['slot'].rpartition('-')[2]}, {worker['model']}"
                + ("" if crew_id(worker["name"]) == worker["id"] else f", ID {worker['id']}") + ")"
                + ("" if worker["named"] else " [slot name]") for worker in person["crew"]))
    return lines


def stargate_crew_names_free(cfg):
    """Whether the installed Stargate's crew plugin takes a crew member's name that differs from its ID. Stargate
    releases to date refuse it ('Agent IDs must match assigned names'); a checkout that is not installed yet is
    taken as refusing."""
    team = APP / "src" / f"stargate-{cfg['stargate']['ref']}" / "src/coordination/crew/team.ts"
    try:
        return "Agent IDs must match assigned names" not in team.read_text()
    except OSError:
        return False


def check_crew_ids(cfg):
    """Refuse a crew member whose name and ID differ (crewIds) unless the installed Stargate allows it."""
    split = [f"{person['id']}'s {worker['name']} ({worker['id']})" for person in cfg["members"]
             for worker in person["crew"] if crew_id(worker["name"]) != worker["id"]]
    require(not split or stargate_crew_names_free(cfg),
            f"Crew {', '.join(split)} keep installed IDs under new names, but Stargate {cfg['stargate']['ref'][:12]}'s "
            "crew plugin requires a crew member's name to match its ID. Remove crewIds and crewNames for them, or "
            "install a Stargate that drops that rule (installer README, 'Crew names').")


def check_crew_names(agents):
    """Crew names (crewNames) are unique in the fleet, compared without case (as Stargate shortens a name to an ID),
    and none is an officer's ID, account or name: people tell crew apart by name, and the tracker records a crew
    member's work by its name beside its captain (relayedCrew). It is the names that are compared, not the IDs: a
    crew member's ID need only be unique in its captain's crew (crew_roster), and an installed one keeps its ID
    under a new name (crewIds), so every ship may have a claude-1. A slot without a name of its own keeps its slot
    name, 'Claude 1', on every ship, as fleets installed before crew names do."""
    officers = {}
    for person in agents:
        for value in (person["id"], person["account"], person["name"]):
            officers.setdefault(crew_id(value), person["id"])
    seen = {}
    for person in agents:
        for worker in person["crew"]:
            if not worker["named"]:
                continue
            key = crew_id(worker["name"])
            owner = officers.get(key)
            require(owner is None, f"Crew member {worker['name']} of captain {person['id']} has an officer's name "
                    f"({owner}). Crew names are unique in the fleet and differ from the officers' names.")
            other = seen.get(key)
            require(other is None, f"Crew member {worker['name']} of captain {person['id']} has the same name as "
                    f"{other}, ignoring case. Crew names are unique in the fleet: give each crew member its own.")
            seen[key] = f"captain {person['id']}'s {worker['name']}"


def check_new_crew_named(cfg, installed):
    """In a fleet with plain officer names, every crew member of a captain not installed yet has a name of its own
    in crewNames, as subspace-setup asks: a slot name ('Claude 1') repeats on every ship. installed: the installed
    fleet.json, {} on a new VM. A captain installed with repositories keeps its crew as they are, named or not."""
    if cfg["officerNames"] != "plain":
        return
    known = {person.get("id") for person in installed.get("captains") or [] if isinstance(person, dict)} \
        if installed.get("repositories") else set()
    unnamed = [f"{person['id']} {worker['slot']}" for person in cfg["members"][1:] if person["id"] not in known
               for worker in person["crew"] if not worker["named"]]
    require(not unnamed, f"Crew {', '.join(unnamed)} have no name: in a fleet with plain officer names every crew "
            "member of a new captain needs one, in its captain's crewNames by slot, unique in the fleet (installer "
            "README, 'Crew names').")


def repository_config(value):
    require(isinstance(value, list), "repositories must be an array.")
    repos = {}
    for row in value:
        row = fields(row, "repository", "id url private description tracker check", ["id", "url"])
        # IDs are also Stargate crew project keys: lowercase, starting with a letter, never "default".
        name = pattern(row["id"], r"[a-z][a-z0-9_-]{0,39}", "repository ID (start with a lowercase letter)")
        require(name != "default", "The repository ID default is reserved by Stargate's crew plugin.")
        require(name not in repos, "Repository IDs must be unique.")
        address = url(row["url"], "repository Git URL", loopback_http=True)
        require(urlsplit(address).path not in {"", "/"}, "Repository Git URL must include a path.")
        private = boolean(row.get("private", False))
        if private:
            pattern(address, r"https://github\.com/[a-zA-Z0-9_-]+/[a-zA-Z0-9_.-]+", "private GitHub repository HTTPS URL")
        repos[name] = {"url": address, "private": private}
    return repos


def github_repository(address):
    """A GitHub HTTPS repository URL's owner/repository, as Stargate reads a remote
    (src/api/v1/github-repository.ts), or None for any other URL."""
    parts = urlsplit(address)
    if parts.scheme != "https" or (parts.hostname or "").lower() != "github.com" or parts.query or parts.fragment:
        return None
    name = parts.path[1:].removesuffix("/").removesuffix(".git")
    valid = re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9-]{0,38}/[A-Za-z0-9_.-]{1,100}", name) and not re.search(r"/\.{1,2}$", name)
    return name if valid else None


def pull_request_repositories(cfg, homes):
    """Stargate's api.pullRequestRepositories: each crew worktree of a GitHub repository, keyed by its path as the
    crew plugin names it, and that repository's owner/repository, so the Fleet API can look up the pull request
    of the worktree's branch."""
    return {str(crew_worktree(homes[person["id"]], worker, repo_id)): name
            for person in cfg["members"][1:] for worker in person["crew"]
            for repo_id, repo in cfg["repositories"].items() if (name := github_repository(repo["url"]))}


def repository_notes(value):
    """Each repository's optional one-line description and tracker note, for its captains' workspace instructions.

    Kept apart from repository_config, whose URL and visibility are fixed once installed; these may change.
    """
    notes = {}
    for row in value:
        note = {}
        for key, maximum in (("description", 200), ("tracker", 300)):
            if key in row:
                note[key] = string(row[key], f"repository {row['id']} {key} (one line)", maximum).strip()
        notes[row["id"]] = note
    return notes


def repository_checks(value):
    """Each repository's optional basic check, a command line its captain's user runs in a crew worktree when the
    installer verifies the toolchains (--verify-toolchains)."""
    return {row["id"]: string(row["check"], f"repository {row['id']} check (one command line)", 300).strip()
            for row in value if "check" in row}


# fleet.json's toolchains and systemPackages (lib/toolchains.py).
toolchain_config = toolchains.toolchain_config


def maintenance_config(value):
    options = fields(value, "maintenance", "mirrorCalendar toolsCalendar pruneCalendar logDays worktreeDays")
    result = {}
    for key, fallback in (("mirrorCalendar", "hourly"), ("toolsCalendar", "weekly"), ("pruneCalendar", "daily")):
        calendar = options.get(key, fallback)
        if calendar is not False:
            result[key] = pattern(calendar, r"[a-zA-Z0-9*~:,./ +_-]{1,100}", f"systemd calendar for {key}")
    for key, default in (("logDays", 14), ("worktreeDays", 30)):
        days = options.get(key, default)
        require(type(days) is int and 1 <= days <= 365, f"{key} must be between 1 and 365.")
        result[key] = days
    return result


def integrations_config(cfg):
    stakeholder = fields(cfg.get("stakeholders", {}), "stakeholders", "slack teams")
    slack = None
    if "slack" in stakeholder:
        slack = fields(stakeholder["slack"], "stakeholders.slack", "appId users channels", ["appId", "users", "channels"])
        pattern(slack["appId"], r"A[A-Z0-9]+", "Slack app ID")
        for key, expression in (("users", r"[UW][A-Z0-9]+"), ("channels", r"[CGD][A-Z0-9]+")):
            require(isinstance(slack[key], list) and slack[key], f"Slack {key} requires an explicit allowlist.")
            for val in slack[key]:
                pattern(val, expression, f"Slack {key} entry")
    teams = None
    if "teams" in stakeholder:
        teams = fields(stakeholder["teams"], "stakeholders.teams", "botClientId tenantId users channels socketGroup", ["botClientId", "tenantId", "users"])
        for key in ("botClientId", "tenantId"):
            pattern(teams[key], UUID, f"Teams {key}")
        require(isinstance(teams["users"], list) and teams["users"], "Teams users requires an explicit allowlist.")
        for user in teams["users"]:
            pattern(user, UUID, "Teams user object ID")
        require(isinstance(teams.get("channels", []), list), "Teams channels must be an array.")
        for name in teams.get("channels", []):
            string(name, "Teams channel ID", 1024)
        if "socketGroup" in teams:
            pattern(teams["socketGroup"], r"[a-z_][a-z0-9_-]{0,31}", "Teams socket group")
    usage = fields(cfg.get("usage", {}), "usage", "reportChannel families")
    families = fields(usage.get("families", {}), "usage.families", "claude codex")
    for family, settings in families.items():
        fields(settings, f"{family} quota provider", "")
    if "reportChannel" in usage:
        require(slack is not None or "slack" in cfg["rearAdmiral"], "Usage notices require rear-admiral Slack.")
        pattern(usage["reportChannel"], r"[CG][A-Z0-9]+", "Slack quota report channel")
    return {"slack": slack, "teams": teams,
            "usage": {**usage, "providers": sorted(families or FAMILIES)}}


def medical_config(value, irc, agents):
    allowed = "enabled baseUrl permalinkHost organisation projects environment http5xxTag intervalSeconds channel account target"
    value = fields(value, "medical", allowed)
    if not boolean(value.get("enabled", False)):
        return None
    fields(value, "medical", allowed, ["organisation", "projects", "account"])
    base = url(value.get("baseUrl", "https://sentry.io"), "Sentry base URL", True, ("", "/")).rstrip("/")
    host(urlsplit(base).hostname)
    permalink_host = host(value["permalinkHost"]) if "permalinkHost" in value else None
    slug = r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}"
    organisation = pattern(value["organisation"], slug, "Sentry organisation slug")
    projects = value["projects"]
    require(isinstance(projects, list) and 1 <= len(projects) <= 50, "Specify 1 to 50 Sentry projects.")
    projects = [pattern(project, r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,99}", "Sentry project slug or ID") for project in projects]
    require(len(set(projects)) == len(projects), "Sentry projects must be unique.")
    tag = None
    if "http5xxTag" in value:
        tag = pattern(value["http5xxTag"], r"[a-zA-Z0-9_][a-zA-Z0-9_.-]{0,31}", "Sentry HTTP status tag")
    interval = value.get("intervalSeconds", 10800)
    require(type(interval) is int and 300 <= interval <= 86400, "Medical intervalSeconds must be between 300 and 86400.")
    account_name = pattern(value["account"], ERGO_ACCOUNT, "medical IRC account")
    reserved = {*irc["allowedAccounts"], *(person["account"] for person in agents)}
    if irc["mode"] == "local":
        reserved.add(irc["compose"]["operator"])
    require(account_name not in {name.lower() for name in reserved}, "Use a dedicated medical IRC account.")
    target = None
    room = irc["starbaseChannel"]
    if "target" in value:
        target = next((person for person in agents if person["id"] == value["target"]), None)
        require(target is not None, "Medical target must be a captain or rear-admiral ID.")
        room = target["channel"]
    room = channel(value.get("channel", room))
    require(target is None or room == target["channel"], "Use the medical target's own channel.")
    return {"baseUrl": base, "permalinkHost": permalink_host, "organisation": organisation, "projects": projects,
            "environment": string(value["environment"], "Sentry environment", 64) if "environment" in value else "",
            "http5xxTag": tag, "intervalSeconds": interval, "channel": room, "account": account_name, "target": target}


def history_password(irc, private):
    postgres = irc["mode"] == "local" and irc["compose"]["history"]["database"] == "postgresql"
    secret = private.get("ircHistoryPassword")
    if postgres:
        return string(secret, "ircHistoryPassword", 256)
    require(secret is None, "ircHistoryPassword is only for PostgreSQL IRC history.")
    return None


def officer_names(officers, naming):
    """Give each officer its IRC account by the fleet's naming: in a fleet with plain names, every officer's
    account is its ID, so its IRC account and nick, its Subspace API agent name and its Stargate registration
    ID are one name; a fleet with fleet-prefixed names keeps the accounts its fleet.json gives."""
    for person in officers:
        if naming == "plain":
            account = person.setdefault("account", person["id"])
            require(account == person["id"],
                    f"Officer {person['id']} has the IRC account {account}, but in a fleet with plain officer names "
                    "(officerNames plain) an officer's IRC account is its ID, so its IRC account, API name and "
                    "Stargate ID are one name. Leave account out, or set officerNames to prefixed for a fleet "
                    "installed with fleet-prefixed accounts (installer README, 'Officer names').")
        else:
            require("account" in person, f"Officer {person['id']} needs an IRC account (account), or set "
                    "officerNames to plain so that its account is its ID.")


def require_usage_guard_source(cfg, installed_only=False):
    if not cfg["usageGuard"]["enabled"]:
        return

    ref = cfg["stargate"]["ref"]
    repo = cfg["stargate"]["repo"]
    source = APP / "src" / f"stargate-{ref}"
    message = ("Usage guard requires a newer Stargate. For a guided fleet, re-run subspace-setup "
               "--stargate-ref latest or --no-usage-guard. For a hand-written fleet.json, update "
               "stargate.ref or turn off usageGuard.")

    if installed_only or source.is_dir():
        require((source / USAGE_GUARD_SOURCE).is_file(), message)
        return

    if (repo, ref) == (STARGATE_REPO, STARGATE_REF):
        return  # The installer's published pin includes the guard.

    with tempfile.TemporaryDirectory(prefix="subspace-stargate-check-") as work:
        try:
            subprocess.run(["git", "init", "--quiet", "--bare", work], check=True,
                           capture_output=True, timeout=20)
            subprocess.run(["git", "-C", work, "fetch", "--quiet", "--no-tags", "--depth", "1", repo, ref],
                           check=True, capture_output=True, timeout=60)
            check = subprocess.run(["git", "-C", work, "cat-file", "-e", f"FETCH_HEAD:{USAGE_GUARD_SOURCE}"],
                                   capture_output=True, timeout=20)
        except (OSError, subprocess.SubprocessError):
            raise Invalid(f"Could not check usage guard support in Stargate {ref[:12]}. Check GitHub access "
                          "or choose a known Stargate release.") from None
        require(check.returncode == 0, message)


def usage_guard_config(value):
    config = fields(value, "usageGuard", "enabled claude codex")
    result = {"enabled": boolean(config.get("enabled", True))}
    for provider in PROVIDERS:
        limits = {**USAGE_GUARD_DEFAULTS, **fields(config.get(provider, {}), f"usageGuard.{provider}", "warning cutoff")}
        for name, limit in limits.items():
            require(type(limit) is int and 0 <= limit < 100,
                    f"usageGuard.{provider}.{name} must be an integer from 0 to 99.")
        require(limits["cutoff"] < limits["warning"],
                f"usageGuard.{provider}.cutoff must be lower than its warning.")
        result[provider] = limits
    return result


def fleet_usage_guard(value, rear):
    guard = usage_guard_config(value)
    if guard["enabled"] and rear["harness"]["type"] != "opencode":
        guard.update(enabled=False, unsupported=True)
    return guard


def usage_guard_line(guard, rear):
    if guard.get("unsupported"):
        return "Usage guard is off for a Claude SDK rear admiral because its agent tools share the gateway user."
    if not guard["enabled"]:
        return "Usage guard is off."
    limits = ", ".join(f"{provider.title()} {guard[provider]['warning']}% warning / {guard[provider]['cutoff']}% cutoff"
                       for provider in PROVIDERS)
    return f"Usage guard is on. {rear['account']} sends ship notices. {limits}."


def usage_guard_file():
    return STATE / "usage-guard" / "quota.json"


def quota_proxy(cfg, home):
    return {"baseURL": f"http://{LOOPBACK}:{cfg['proxy']['port']}",
            "managementKeyFile": str(home / "quota-management-key")}


def usage_guard_settings(cfg, home):
    rear, *captains = cfg["members"]
    return {
        "enabled": True,
        "sender": rear["id"],
        "captains": [{"account": person["account"], "channel": person["channel"]} for person in captains],
        "stateFile": str(usage_guard_file()),
        "proxy": quota_proxy(cfg, home),
        "providers": {
            provider: {
                "modelPrefixes": model_prefixes,
                "limits": [
                    {"remainingPercent": cfg["usageGuard"][provider]["warning"], "action": "warn"},
                    {"remainingPercent": cfg["usageGuard"][provider]["cutoff"], "action": "pause"},
                ],
            }
            for provider, model_prefixes in USAGE_GUARD_MODELS.items()
        },
    }


def validate(config, credentials, check_files=True):
    cfg = fields(config, "fleet", "slug name officerNames rearAdmiral captains harness crew captainTools repositories stakeholders usage usageGuard medical irc stargate dashboard proxy agentsview api agentDispatch operatorUsers toolchains systemPackages screenshots maintenance githubPolicy memory", ["slug", "name", "rearAdmiral", "captains", "irc"])
    slug = pattern(cfg["slug"], r"[a-z0-9][a-z0-9-]{0,34}", "fleet slug")
    # Fleets written before plain names leave it out, and keep their fleet-prefixed accounts and ship channels.
    naming = pattern(cfg.get("officerNames", "prefixed"), "|".join(standard_channels.NAMING),
                     "officerNames (plain or prefixed)")
    plain = naming == "plain"
    require(isinstance(cfg["captains"], list), "Captains must be an array.")
    officers = [member(person) for person in [cfg["rearAdmiral"], *cfg["captains"]]]
    officer_names(officers, naming)
    require(not {"crew", "crewNames", "crewIds"} & officers[0].keys(), "Only captains may declare crew.")
    require("sessionRoot" not in officers[0], "Only captains take sessionRoot: the rear admiral works in its "
            "reference checkout of the first repository.")
    irc = irc_config(cfg["irc"], slug, check_files, plain)
    captain_ids = [person["id"] for person in officers[1:]]
    trouble = standard_channels.problem(slug, captain_ids, plain)
    require(trouble is None, trouble)
    # The rear admiral's room is Starbase; each captain's is its ship channel.
    officer_rooms = [irc["starbaseChannel"], *(standard_channels.ship(slug, captain_id, plain) for captain_id in captain_ids)]
    for index, (person, room) in enumerate(zip(officers, officer_rooms, strict=True)):
        if "channel" in person:
            label = "rearAdmiral.channel" if index == 0 else f"Captain {person['id']}'s channel"
            require(person["channel"] == room, not_standard(slug, label, person["channel"], room, plain))
    channels = standard_channels.standard(slug, captain_ids, plain)
    for name in channels:
        channel(name)
    # Stargate joins at most 20 channels per registration, and the rear admiral joins every standard one.
    require(len(channels) <= 20, f"Use at most 18 captains: the rear admiral joins Starbase, {irc['commandChannel']} "
            "and every ship channel, and Stargate joins at most 20 channels.")
    irc.update(channels=channels)
    # Each role's default model, then the fleet's harness, then its captain or rearAdmiral block, then the officer's.
    fleet_harness = cfg.get("harness", {})
    require(isinstance(fleet_harness, dict), "harness must be an object.")
    roles = {key: fields(fleet_harness.get(key, {}), f"harness.{key}", "family model effort extraPermissions")
             for key in ROLE_HARNESS.values()}
    shared = {key: val for key, val in fleet_harness.items() if key not in roles}
    hulls = {role: harness_config(roles[key], harness_config(shared, {"type": "opencode", **OFFICER_DEFAULTS[role],
                                                                         "permissions": None}))
             for role, key in ROLE_HARNESS.items()}
    crew = crew_config(cfg.get("crew", {}), {family: {"count": 2, "model": model} for family, model in FAMILIES.items()})
    captain_tools = pattern(cfg.get("captainTools", "standard"), r"standard|delegate", "captain tool policy")
    repositories = repository_config(cfg.get("repositories", []))
    agents = []
    for index, (person, room) in enumerate(zip(officers, officer_rooms, strict=True)):
        role = "captain" if index else "rear_admiral"
        officer = {key: val for key, val in person.items() if key not in ("crew", "crewNames", "crewIds")}
        agents.append({**officer, "role": role, "channel": room, "crew": [],
                       "harness": harness_config(person.get("harness", {}), hulls[role]),
                       "memory": memory_config(memory.limits, person.get("memory", {}), f"{person['id']}'s memory")})
        if role != "captain":
            continue
        crew_names = person.get("crewNames", {})
        require(isinstance(crew_names, dict), "crewNames must be an object keyed by crew slot.")
        teams = crew_config(person.get("crew", {}), crew)
        slots = {f"{family}-{slot}" for family, team in teams.items() for slot in range(1, team["count"] + 1)}
        require(crew_names.keys() <= slots, "crewNames may name only enabled crew slots.")
        crew_ids = person.get("crewIds", {})
        require(isinstance(crew_ids, dict) and crew_ids.keys() <= slots
                and all(isinstance(value, str) and re.fullmatch(CREW_ID, value) for value in crew_ids.values()),
                "crewIds may give only enabled crew slots their installed crew IDs.")
        if not slots:
            continue
        require(agents[-1]["harness"]["type"] == "opencode",
                f"Captain {person['id']} uses claude-sdk, which cannot host crew: crew run inside their captain's "
                "OpenCode server through Stargate's crew plugin. Set this captain's crew counts to 0.")
        # Crew work in Git worktrees of their captain's checkouts, so a fleet without repositories has none.
        if repositories:
            require(len(repositories) <= MAX_CREW_REPOSITORIES,
                    f"Captains with crew support at most {MAX_CREW_REPOSITORIES} repositories.")
            roster = crew_roster(person, teams, crew_names, slug, crew_ids)
            require(len(roster) * len(repositories) <= MAX_CREW_CONTEXTS,
                    f"Crew times repositories must not exceed {MAX_CREW_CONTEXTS} per captain (Stargate's crew limit).")
            agents[-1]["crew"] = roster
    if plain:
        # A fleet installed before plain names keeps the crew names it has, unique per captain as they were.
        check_crew_names(agents)
    applications = [person["githubApp"] for person in agents if "githubApp" in person]
    for key in ("appId", "installationId", "botId"):
        require(len({app[key] for app in applications}) == len(applications), "Each fleet member needs a distinct GitHub App identity.")
    require(len({person["id"] for person in agents}) == len(agents), "Rear-admiral and captain IDs must be unique.")
    accounts = {person["account"].lower() for person in agents}
    require(len(accounts) == len(agents), "Member SASL accounts must be unique.")
    require(not accounts.intersection(name.lower() for name in irc["allowedAccounts"]), "Human and fleet SASL accounts must differ.")
    dashboard = listener(cfg.get("dashboard", {}), "dashboard", 8070, True)
    view = listener(cfg.get("agentsview", {}), "agentsview", 8080)
    proxy = fields(cfg.get("proxy", {}), "proxy", "port requestRetries")
    proxy_port = port(proxy.get("port", 8317))
    retries = proxy.get("requestRetries", 2)
    require(type(retries) is int and 0 <= retries <= 10, "Proxy requestRetries must be between 0 and 10.")
    stargate = fields(cfg.get("stargate", {}), "stargate", "repo ref api")
    stargate_api = None
    if "api" in stargate:
        # Stargate's private Fleet API, on loopback for operators and their tunnels or proxies.
        options = fields(stargate["api"], "stargate.api", "port corsOrigins")
        origins = options.get("corsOrigins", FLEET_API_ORIGINS)
        require(isinstance(origins, list) and len(origins) <= 20, "stargate.api.corsOrigins must be an array.")
        for origin in origins:
            # Exact serialised origins, or Stargate's whole-label * patterns, such as subspace://*.
            pattern(origin, r"[a-z][a-z0-9+.-]*://[a-z0-9*](?:[a-z0-9*.-]*[a-z0-9*])?(?::[0-9]{1,5})?",
                    "Fleet API CORS origin")
        stargate_api = {"port": port(options.get("port", 8484)), "corsOrigins": origins}
    listeners = [dashboard["port"], proxy_port, view["port"]]
    if stargate_api:
        listeners.append(stargate_api["port"])
    if irc["mode"] == "local":
        listeners += [irc["port"], irc["compose"]["tlsPort"], 8097]
    require(len(set(listeners)) == len(listeners), "Fleet listeners need distinct ports.")
    api = api_config(cfg["api"]) if "api" in cfg else None
    require(not (api or {}).get("fleetApiUrl") or stargate_api,
            "api.fleetApiUrl is the address of Stargate's Fleet API, which is off: set stargate.api, or remove it.")
    dispatch = agent_dispatch_config(cfg.get("agentDispatch", {}), api, agents)
    operators = cfg.get("operatorUsers", [])
    require(isinstance(operators, list), "operatorUsers must be an array.")
    operators = [pattern(user, r"[a-z_][a-z0-9_-]{0,31}", "Linux operator username") for user in operators]
    require(all(not user.startswith("ss-") for user in operators), "Fleet service users cannot be operators.")
    github_policy = fields(cfg.get("githubPolicy", {}), "githubPolicy", "protectedBranches")
    protected = github_policy.get("protectedBranches", ["main", "master", "release/*"])
    require(isinstance(protected, list) and all(isinstance(branch, str) for branch in protected),
            "githubPolicy.protectedBranches must be an array of branch patterns.")
    protected = [pattern(branch, r"[a-zA-Z0-9*?._/-]{1,120}", "GitHub protected branch pattern") for branch in protected]

    private = fields(credentials, "secrets", "proxyKey ircPasswords ircOperatorPassword ircHistoryPassword apiToken rearAdmiralApiToken", ["proxyKey"])
    proxy_key = pattern(private["proxyKey"], r"[a-zA-Z0-9_=-]{20,256}", "proxy client key")
    passwords = private.get("ircPasswords", {})
    require(isinstance(passwords, dict), "ircPasswords must be an object.")
    medical = medical_config(cfg.get("medical", {}), irc, agents)
    required = {person["account"] for person in agents}
    operator_password = private.get("ircOperatorPassword")
    permitted = required
    if irc["mode"] == "local":
        operator = irc["compose"]["operator"]
        require(operator not in accounts, "The Ergo operator account must differ from fleet accounts.")
        permitted = (required | set(irc["allowedAccounts"])) - {operator}
        ergo_password(operator_password)
        for account in required | set(irc["allowedAccounts"]):
            pattern(account, ERGO_ACCOUNT, "local Ergo account")
    else:
        require(operator_password is None, "An IRC operator password requires local mode.")
    if irc["auth"] == "certificate":
        # With certificates no IRC password exists for an officer, so none is accepted or stored.
        require(not {name.lower() for name in passwords} & accounts,
                "Officers sign in to IRC with certificates (irc.auth certificate, the default with a remote server), "
                "so the secrets file keeps no IRC password for the rear admiral or a captain. Remove their entries "
                "from ircPasswords, or set irc.auth to password to keep signing in with passwords.")
    else:
        require(required <= passwords.keys(), "Supply an IRC password for the rear admiral and every captain.")
    require(passwords.keys() <= permitted, "ircPasswords names an account that is not the rear admiral's or a captain's. "
            "Crew have no IRC accounts. Local mode also accepts passwords for allowlisted humans.")
    for password in passwords.values():
        if irc["mode"] == "local":
            ergo_password(password)
        else:
            string(password, "IRC password", 256)
    token = private.get("apiToken", "")
    if api and api.get("enrolment"):
        # Officers enrol with the fleet's enrolment token, so no administrator token is kept on the VM.
        require(token == "", "api.enrolment keeps no administrator token on the VM: remove apiToken from the "
                "secrets file. The hosted API's administrators register the fleet, and its officers enrol "
                "themselves.")
        require(any(person["harness"]["type"] == "opencode" for person in agents),
                "api.enrolment needs an OpenCode officer: the shared gateway that holds the officers' keys serves "
                "OpenCode officers, and a fleet of Claude Agent SDK officers has none.")
        release = tuple(int(part) for part in re.findall(r"\d+", STARGATE_RELEASE)[:3])
        require(stargate.get("ref", STARGATE_REF) != STARGATE_REF or release >= SUBSPACE_AGENTS_RELEASE,
                f"api.enrolment needs a Stargate with its subspaceAgents section, from commit "
                f"{SUBSPACE_AGENTS_COMMIT[:12]} on; this installer's pinned release, {STARGATE_RELEASE}, has not "
                "got it. Set stargate.ref (subspace-setup --stargate-ref) to that commit or a later one.")
    elif api:
        pattern(token, JWT, "API bearer JWT")
    else:
        require(token == "", "An API token requires API configuration.")
    dispatch_token = private.get("rearAdmiralApiToken") or None
    if dispatch:
        require(dispatch_token is not None, "Agent dispatch is on: supply rearAdmiralApiToken in the secrets file, a "
                "bearer token for the rear admiral's own Subspace API agent identity (rank rearAdmiral, in this fleet).")
        require(isinstance(dispatch_token, str) and len(dispatch_token) <= 16384 and re.fullmatch(JWT, dispatch_token),
                "Invalid rearAdmiralApiToken: expected a bearer JWT.")
        # The dispatcher must act as the rear admiral; an administrator's token is refused by Stargate.
        require(dispatch_token != token, "rearAdmiralApiToken must be the rear admiral agent's own token, not apiToken, "
                "which belongs to an organisation administrator.")
    else:
        require(dispatch_token is None, "rearAdmiralApiToken is only for agent dispatch. Remove it, or switch "
                "agentDispatch.enabled on.")
    require(not any(repo["private"] for repo in repositories.values()) or len(applications) == len(agents),
            "Private repositories require a distinct GitHub App for the rear admiral and every captain.")
    for person in agents:
        # With captainTools delegate, a captain with crew leaves coding to them, whatever its harness.permissions;
        # a captain without crew has no one to delegate to, so it keeps the standard policy.
        delegate = person["role"] == "captain" and captain_tools == "delegate" and bool(person["crew"])
        if delegate:
            person["delegatesCoding"] = True
        starter = person["harness"]["permissions"] is None
        if starter:
            references = STATE / "references" / person["id"] if cfg.get("repositories") else None
            worktrees = STATE / "agents" / person["id"] / "worktrees" if person["crew"] else None
            person["harness"]["permissions"] = default_permissions(person["role"], references, delegate=delegate,
                                                                   worktrees=worktrees)
        # Where the operator's own rules begin, an explicit harness.permissions or else extraPermissions: a grant
        # the installer adds goes before them (harness_permissions), so they have the last word.
        person["harness"]["operatorRulesFrom"] = len(person["harness"]["permissions"]) if starter else 0
        if extra := person["harness"]["extraPermissions"]:
            # After the starter policy or an explicit harness.permissions, so the operator's rules win.
            person["harness"]["permissions"] = [*person["harness"]["permissions"], *extra]
        if dispatch and person["crew"]:
            fleet_ipc_policy(person["harness"]["permissions"], f"Captain {person['id']}'s harness.permissions")
    if api:
        # fleetctl register records the officers, their ships and the standard channels in the API.
        try:
            fleet_records.check(slug, fleet_records.roster({"members": agents, "irc": irc, "officerNames": naming}))
        except fleet_records.Refused as error:
            raise Invalid(str(error)) from None
    integrations = integrations_config(cfg)
    check_officer_slack(agents, integrations["slack"])
    return {
        "slug": slug, "name": string(cfg["name"], "fleet name"), "officerNames": naming, "members": agents, "irc": irc,
        "medical": medical,
        "repositories": repositories,
        "repositoryNotes": repository_notes(cfg.get("repositories", [])),
        "repositoryChecks": repository_checks(cfg.get("repositories", [])),
        "integrations": integrations, "operators": operators, "protectedBranches": protected,
        "toolchains": toolchain_config(cfg.get("toolchains", {})),
        "systemPackages": toolchains.package_config(cfg.get("systemPackages", [])),
        # The base screenshot tools (lib/screenshots.py): on unless fleet.json says "screenshots": false.
        "screenshots": boolean(cfg.get("screenshots", True)),
        "maintenance": maintenance_config(cfg.get("maintenance", {})),
        "stargate": {**source({key: stargate[key] for key in ("repo", "ref") if key in stargate},
                              "Stargate source", STARGATE_REPO, STARGATE_REF), "api": stargate_api},
        "dashboard": dashboard, "agentsview": view, "api": api,
        "agentDispatch": dispatch and {**dispatch, "token": dispatch_token},
        "usageGuard": fleet_usage_guard(cfg.get("usageGuard", {}), agents[0]),
        "proxy": {"port": proxy_port, "requestRetries": retries, "key": proxy_key},
        "passwords": passwords, "operatorPassword": operator_password, "token": token, "listeners": listeners,
        "historyPassword": history_password(irc, private),
        "captainTools": captain_tools,
        "memory": memory_config(memory.config, cfg.get("memory", {})),
    }


def json_text(data):
    return json.dumps(data, indent=2, ensure_ascii=True) + "\n"


def write(path, content, mode=0o600, group="root"):
    gid = group if isinstance(group, int) else grp.getgrnam(group).gr_gid
    if path.exists() and not path.is_symlink():
        info = path.stat()
        if info.st_uid == 0 and info.st_gid == gid and stat.S_IMODE(info.st_mode) == mode and path.read_text() == content:
            return
    (STATE / "pending").touch()
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False, encoding="utf-8") as stream:
        temp = Path(stream.name)
        try:
            os.fchmod(stream.fileno(), mode)
            os.fchown(stream.fileno(), 0, gid)
            stream.write(content)
            stream.close()
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)


def run(*args, user=None, quiet=False):
    command = [str(arg) for arg in args]
    if user:
        command = ["runuser", "-u", user, "--", *command]
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL if quiet else None)


def app_key(path):
    try:
        with private_directory(path.parent.parent) as root:
            require(root is not None, f"Missing secrets directory for GitHub App key at {path}.")
            with private_directory(path.parent.name, root) as agent:
                require(agent is not None, f"Missing agent secrets directory for GitHub App key at {path}.")
                key = private_file(path.name, dir_fd=agent).decode("ascii")
    except (OSError, UnicodeError):
        raise Invalid(f"Cannot read the private GitHub App key at {path}.") from None
    require(
        len(key) <= 65536 and re.fullmatch(
            r"-----BEGIN (RSA |)PRIVATE KEY-----\r?\n[A-Za-z0-9+/=\r\n]+-----END \1PRIVATE KEY-----\r?\n?",
            key,
        ),
        f"Expected an unencrypted GitHub App private key at {path}.",
    )
    return key


def app_keys(people, directory, saved_people=()):
    previous = {person["id"]: person.get("githubApp") for person in saved_people}
    keys = {}
    for person in people:
        if "githubApp" not in person:
            continue
        agent_id = person["id"]
        path = directory / agent_id / "github-app.pem" if directory else None
        old = previous.get(agent_id)
        same_app = old and all(old[key] == person["githubApp"][key] for key in ("appId", "installationId", "clientId"))
        if (path is None or not os.path.lexists(path)) and same_app:
            path = ETC / "github-apps" / agent_id / "github-app.pem"
        require(path is not None, f"Supply the GitHub App key for {agent_id} in a secrets directory.")
        keys[agent_id] = app_key(path)
    return keys


def extra_sources(cfg, directory, partial=False):
    integrations = cfg["integrations"]
    slack = slack_members(cfg)
    quota = "reportChannel" in integrations["usage"]
    with private_directory(directory) as root:
        if root is None:
            require(partial or not (slack or quota or cfg["medical"]), "Integration credentials require a secrets directory.")
            return {}
        cloud = collect_cloud(root, [person["id"] for person in cfg["members"]])
        files = {(agent, "cloud", name): contents
                 for agent, entries in cloud.items() for name, contents in entries.items()}
        for agent in slack:
            with private_directory(agent, root) as agent_dir:
                require(partial or agent_dir is not None, f"Supply Slack credentials in {slack_owner(cfg, agent)} secrets directory.")
                if agent_dir is not None:
                    for name in SLACK_TOKEN_FILES:
                        contents = private_file(name, optional=partial, dir_fd=agent_dir)
                        if contents is not None:
                            files[agent, name] = contents
        if quota or cfg["usageGuard"]["enabled"]:
            contents = private_file("proxy-management-key", optional=partial or not quota, dir_fd=root)
            if contents is not None:
                files["proxy-management-key",] = contents
        if enrolled(cfg):
            # Optional on every run: an upgrade keeps the installed token, and enrolled officers need none.
            contents = private_file(ENROLMENT_TOKEN, optional=True, dir_fd=root)
            if contents is not None:
                files[ENROLMENT_TOKEN,] = contents
        if cfg["medical"]:
            with private_directory(SENTRY_TOKEN[0], root) as medical_dir:
                require(partial or medical_dir is not None, "Supply credentials in the medical secrets directory.")
                if medical_dir is not None:
                    for path in (SENTRY_TOKEN, MEDICAL_IRC_PASSWORD):
                        contents = private_file(path[-1], optional=partial, dir_fd=medical_dir)
                        if contents is not None:
                            files[path] = contents
    return files


def extra_snapshot(cfg, directory, previous, prune):
    files = extra_sources(cfg, directory, partial=not prune) if directory else {}
    tokens = {} if prune else previous.get("tokens", {}).copy()
    # Slack tokens by officer. An upgrade keeps tokens it is not given, as for other credentials; pruning keeps
    # only those of officers whose Slack bot is configured, from the secrets directory.
    slack_tokens = {} if prune else {agent: entries.copy() for agent, entries in previous.get("slack", {}).items()}
    for agent in slack_members(cfg):
        app = slack_app_id(cfg, agent)
        slack = slack_tokens.setdefault(agent, {})
        replaced = slack.get("appId", app) != app
        if replaced:
            # The tokens belong to a Slack app: none kept for the officer's previous app may pair with the new one.
            slack = slack_tokens[agent] = {}
        for name, prefix in SLACK_TOKEN_FILES.items():
            if (agent, name) in files:
                slack[name] = pattern(files[agent, name].decode("ascii").strip(), fr"{prefix}-[a-zA-Z0-9-]{{8,512}}", name)
            require(name in slack, f"Supply {name} in {slack_owner(cfg, agent)} secrets directory"
                    + (f": its Slack app changed to {app}, so both of the new app's tokens are needed." if replaced
                       else "."))
        # Installed copies from before this record are taken to be the configured app's.
        slack["appId"] = app
    for name in SLACK_TOKEN_FILES:
        configured = [slack_tokens[agent][name] for agent in slack_members(cfg)]
        require(len(set(configured)) == len(configured), f"Each Slack bot needs its own {name}: two officers share one.")
    if ("proxy-management-key",) in files:
        tokens["proxy-management-key"] = pattern(files["proxy-management-key",].decode("ascii").strip(), r"[a-zA-Z0-9_=-]{20,256}", "proxy management key")
    if enrolled(cfg):
        if (ENROLMENT_TOKEN,) in files:
            try:
                tokens[ENROLMENT_TOKEN] = pattern(files[ENROLMENT_TOKEN,].decode("ascii").strip(), ENROLMENT_TOKEN_TEXT,
                                                  "fleet enrolment token")
            except (Invalid, UnicodeDecodeError):
                raise Invalid(f"The {ENROLMENT_TOKEN} in the secrets directory is not a fleet enrolment token "
                              "(senr1.…).") from None
        require(ENROLMENT_TOKEN in tokens, f"api.enrolment needs the fleet's enrolment token, from the hosted API's "
                f"administrator, as {ENROLMENT_TOKEN} in the secrets directory (mode 0600).")
    else:
        tokens.pop(ENROLMENT_TOKEN, None)
    if "reportChannel" in cfg["integrations"]["usage"]:
        require("proxy-management-key" in tokens, "Supply a proxy management key in the secrets directory.")
    if cfg["usageGuard"]["enabled"] and "proxy-management-key" not in tokens:
        tokens["proxy-management-key"] = previous.get("tokens", {}).get("proxy-management-key") or secrets.token_hex(32)
    if "proxy-management-key" in tokens:
        require(tokens["proxy-management-key"] != cfg["proxy"]["key"], "Use distinct proxy management and model client keys.")
    medical = {} if prune else previous.get("medical", {}).copy()
    if cfg["medical"]:
        for path in (SENTRY_TOKEN, MEDICAL_IRC_PASSWORD):
            key = path[-1]
            if path in files:
                medical[key] = files[path].decode("utf-8").removesuffix("\n").removesuffix("\r")
            require(key in medical, f"Supply {key} in the medical secrets directory.")
        pattern(medical[SENTRY_TOKEN[-1]], r"[a-zA-Z0-9._~+/=-]{1,512}", "Sentry bearer token")
        password = medical[MEDICAL_IRC_PASSWORD[-1]]
        if cfg["irc"]["mode"] == "local":
            ergo_password(password)
        else:
            string(password, "medical IRC password", 256)
    active = {person["id"] for person in cfg["members"]}
    cloud = {agent: entries.copy() for agent, entries in previous.get("cloud", {}).items() if not prune or agent not in active}
    for parts, contents in files.items():
        if len(parts) == 3:
            cloud.setdefault(parts[0], {})[parts[2]] = base64.b64encode(contents).decode("ascii")
    return {"cloud": cloud, "tokens": tokens, "slack": slack_tokens, "medical": medical}


def directory(path, mode, owner="root", group=None):
    if not path.exists():
        path.mkdir(mode=mode)
    info = path.lstat()
    require(stat.S_ISDIR(info.st_mode), f"Expected a directory at {path}.")
    uid = pwd.getpwnam(owner).pw_uid
    gid = grp.getgrnam(group or owner).gr_gid
    if (info.st_uid, info.st_gid) != (uid, gid):
        os.chown(path, uid, gid, follow_symlinks=False)
    if stat.S_IMODE(info.st_mode) != mode:
        path.chmod(mode)


def account(user, home, mode=0o700):
    registry = STATE / "identity"
    directory(registry, 0o700)
    record = registry / user
    previous = record.read_text().strip() if record.exists() else None
    try:
        current = pwd.getpwnam(user)
    except KeyError:
        run("useradd", "--system", "--user-group", "--no-create-home", "--home-dir", home, "--shell", "/usr/sbin/nologin", user)
        current = pwd.getpwnam(user)
        write(record, f"{current.pw_uid}\n")
        previous = str(current.pw_uid)
    require(previous == str(current.pw_uid) and current.pw_dir == str(home), f"Linux user {user} is outside this fleet. Resolve the account collision.")
    directory(home, mode, user)
    return home


def serve_unit(name, description, command, user, after=(), env=None, user_paths=(), post_start=None, *,
               oneshot=False, toolchain=True, credential=None, service_options=(), unit_options=()):
    dependencies = " ".join(["network-online.target", FIREWALL, *after])
    lines = [
        "[Unit]", f"Description={description}", f"After={dependencies}", f"Requires={FIREWALL}",
        f"Wants={' '.join(['network-online.target', *after])}", *unit_options,
        "", "[Service]", "Type=oneshot" if oneshot else "Type=simple", f"User={user}",
        *([f"Environment=PATH={FLEET_PATH}"] if toolchain else []), "LogNamespace=subspace-fleet",
        "UMask=0077", "NoNewPrivileges=yes", "PrivateTmp=yes", "ProtectSystem=strict",
        # Agent homes live under /var/lib, so /home and /root can be hidden.
        "ProtectHome=yes", "PrivateDevices=yes", "ProtectProc=invisible", "CapabilityBoundingSet=",
        "ProtectKernelTunables=yes", "ProtectKernelModules=yes", "ProtectKernelLogs=yes",
        "ProtectControlGroups=yes", "ProtectClock=yes", "ProtectHostname=yes",
        "RestrictSUIDSGID=yes", "RestrictRealtime=yes", "LockPersonality=yes",
        "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK",
        *(f"ReadWritePaths={path}" for path in user_paths),
        *([] if oneshot else ["Restart=on-failure", "RestartSec=5s"]),
        f"ExecStart={command}",
    ]
    defaults = TOOL_ENV if toolchain else {}
    for key, value in {**defaults, **(env or {})}.items():
        lines.append(f'Environment="{key}={value}"')
    if post_start:
        lines.append(f"ExecStartPost={post_start}")
    if credential:
        lines.append(f"LoadCredential={credential.name}:{credential}")
    lines.extend(service_options)
    if not oneshot:
        lines += ["", "[Install]", "WantedBy=multi-user.target", ""]
    else:
        lines.append("")
    unit = f"{PREFIX}{name}.service"
    write(SYSTEMD / unit, "\n".join(lines), 0o644)
    return unit


def save_settings(name, settings, user):
    path = ETC / f"{name}.json"
    write(path, json_text(settings), 0o640, user)
    return path


def configure_git(person, user, prune, protected):
    settings = {"user.name": person["name"]}
    git = person.get("git", {})
    if "email" in git:
        settings["user.email"] = git["email"]
    if "signingKey" in git:
        settings.update({"gpg.format": "ssh", "user.signingkey": git["signingKey"], "commit.gpgsign": "true"})
    app = person.get("githubApp")
    helper = "credential.https://github.com.helper"
    credential_command = f"!{APP}/lib/github_app.py credential"
    contents = (app or {}).get("contents", "write") if person["role"] != "rear_admiral" else "read"
    if app:
        settings.update({
            "user.name": app["botLogin"],
            "user.email": f"{app['botId']}+{app['botLogin']}@users.noreply.github.com",
            helper: credential_command,
            "credential.https://github.com.useHttpPath": "true",
        })
    key = app_key(ETC / "github-apps" / person["id"] / "github-app.pem") if app else None
    metadata = Path(pwd.getpwnam(user).pw_dir) / ".config/subspace/github-app.json"
    if app or metadata.is_file():
        packet = {
            "metadata": app, "key": key, "prune": prune,
            "policy": {"enabled": app is not None, "contents": contents, "protectedBranches": protected},
        }
        subprocess.run(
            ["runuser", "-u", user, "--", str(APP / "lib/github_app.py"), "provision"],
            input=json_text(packet), text=True, check=True,
        )
    if not app:
        subprocess.run(
            ["runuser", "-u", user, "--", "git", "config", "--global", "--fixed-value", "--unset-all", helper, credential_command],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False,
        )
    for key, value in settings.items():
        result = subprocess.run(
            ["runuser", "-u", user, "--", "git", "config", "--global", "--get", key],
            capture_output=True, text=True,
        )
        if result.stdout.rstrip("\n") != value:
            run("git", "config", "--global", "--replace-all", "--", key, value, user=user)


def model_ref(family, model, effort=None):
    """OpenCode's provider/model reference, with #effort for a reasoning-effort variant."""
    return f"{PROVIDERS[family]}/{model}" + (f"#{effort}" if effort else "")


def harness_model(harness):
    """An officer's OpenCode model reference: its model and reasoning effort."""
    return model_ref(harness["family"], harness["model"], harness["effort"])


def session_model(harness):
    """An officer's model as an OpenCode session records it."""
    return {"providerID": PROVIDERS[harness["family"]], "id": harness["model"],
            **({"variant": harness["effort"]} if harness["effort"] else {})}


def model_provider(cfg, family, models):
    settings = {"baseURL": f"http://{LOOPBACK}:{cfg['proxy']['port']}/v1", "apiKey": "{env:CLIPROXYAPI_API_KEY}"}
    if family == "claude":
        package = "@opencode/ai/providers/anthropic-compatible"
    else:
        package = "@opencode/ai/providers/openai-compatible/responses"
        settings.update(encoding="none", transport="http")
    return {"name": f"Fleet model proxy, {family.title()} family", "package": package, "settings": settings,
            "models": {model: {"name": model} for model in models}}


# The Subspace API's MCP server, as the officer's harness names it, and the grant for its tools.
SUBSPACE_MCP = "subspace"
AGENT_HEADER = "X-Subspace-Agent"
SUBSPACE_MCP_GRANT = {"action": f"{SUBSPACE_MCP}_*", "resource": "*", "effect": "allow"}


def subspace_mcp_url(cfg):
    """The agent listener's MCP endpoint, when this fleet runs the API with one."""
    return (cfg.get("api") or {}).get("agentMcpUrl")


def enrolled(cfg):
    """Whether the fleet's officers reach a hosted API through Stargate's signing proxy (api.enrolment)."""
    return bool((cfg.get("api") or {}).get("enrolment"))


def agent_socket(agent_id):
    return AGENT_SOCKETS / agent_id / "api.sock"


def stargate_entry(cfg):
    return APP / "src" / f"stargate-{cfg['stargate']['ref']}" / "src/main.ts"


def subspace_bridge(cfg, agent_id):
    """Stargate's stdio MCP bridge to an officer's signing-proxy socket, as a command line."""
    return [str(APP / "bin/bun"), str(stargate_entry(cfg)), "subspace", "mcp", "--socket", str(agent_socket(agent_id))]


def subspace_request(cfg, agent_id):
    """Stargate's command for a REST call to the hosted API on an officer's socket, before the path."""
    return [str(APP / "bin/bun"), str(stargate_entry(cfg)), "subspace", "request", "--socket",
            str(agent_socket(agent_id))]


# Every officer's stable command for REST calls to a hosted API, whatever Stargate's checkout is called, and its
# grant: Stargate's request command on the socket of the officer that runs it.
SUBSPACE_REQUEST = APP / "bin/subspace-request"
SUBSPACE_REQUEST_GRANT = {"action": "shell", "resource": "subspace-request *", "effect": "allow"}


def subspace_request_script(cfg):
    """SUBSPACE_REQUEST: the officer is the Linux user that runs it (ss-<id>), so crew, who run as their captain,
    act as the captain, as through its MCP server."""
    request = subspace_request(cfg, "$officer")
    return "\n".join([
        "#!/bin/sh",
        "# Generated by the Subspace fleet installer. A REST call to the fleet's hosted Subspace API, signed as the",
        "# officer that runs it by Stargate's signing proxy:",
        "#   subspace-request [--method METHOD] [--input FILE] /api/v1/PATH",
        "set -eu",
        "user=$(id -un)",
        "officer=${user#ss-}",
        f'socket="{request[-1]}"',
        'if [ "$officer" = "$user" ] || [ ! -S "$socket" ]; then',
        '    echo "subspace-request: run it as a fleet officer; there is no signing-proxy socket at $socket." >&2',
        "    exit 2",
        "fi",
        f'exec {" ".join(request[:-1])} "$socket" "$@"',
        "",
    ])


def stargate_has_subspace_agents(cfg):
    """Whether the installed Stargate checkout has the subspaceAgents section (SUBSPACE_AGENTS_FILE)."""
    return (APP / "src" / f"stargate-{cfg['stargate']['ref']}" / SUBSPACE_AGENTS_FILE).is_file()


def stargate_has_pull_request_repositories(cfg):
    """Whether the installed Stargate checkout's Fleet API takes api.pullRequestRepositories. Releases from v0.42.0
    do; v0.40 and v0.41 refuse the key in their config check, so their gateway would not start. The Fleet API's
    configuration sits beside its server, which moved between releases (FLEET_API_SERVERS in subspace-setup)."""
    for schema in (APP / "src" / f"stargate-{cfg['stargate']['ref']}").glob("src/**/api/v1/config.ts"):
        try:
            if "pullRequestRepositories" in schema.read_text():
                return True
        except (OSError, UnicodeDecodeError):
            continue
    return False


def require_subspace_agents(cfg):
    require(stargate_has_subspace_agents(cfg),
            f"api.enrolment needs a Stargate with its subspaceAgents section ({SUBSPACE_AGENTS_FILE}), from commit "
            f"{SUBSPACE_AGENTS_COMMIT[:12]} on, and Stargate {cfg['stargate']['ref'][:12]} has not got it. Set "
            "stargate.ref (subspace-setup --stargate-ref) to that commit or a later one.")


def subspace_agents_settings(cfg, gateway_home):
    """Stargate's subspaceAgents section for the gateway that runs as the gateway user: one entry per officer,
    named as the API registration names them (fleet_records.agent_name: its ID with plain names; the rear
    admiral by its name and a captain by its account with fleet-prefixed ones), with its ship, and each
    officer's socket."""
    officers = []
    for person in cfg["members"]:
        rear = person["role"] == "rear_admiral"
        officers.append({
            "name": fleet_records.agent_name(cfg, person), "rank": "rearAdmiral" if rear else "captain",
            **({} if rear else {"ship": person["name"]}), "model": person["harness"]["model"],
            "ircNick": person["account"], "socketPath": str(agent_socket(person["id"])),
        })
    return {
        "enabled": True, "api": {"baseUrl": cfg["api"]["origin"]},
        "enrolment": {"tokenFile": str(gateway_home / ENROLMENT_TOKEN)}, "officers": officers,
    }


def agent_socket_directories(cfg):
    """Each officer's socket directory: the gateway's, entered only by the officer's engine group (mode 2750),
    under parents that only root writes. Made now and, through tmpfiles.d, at every boot before the gateway."""
    lines = [f"d {AGENT_SOCKETS.parent} 0711 root root -", f"d {AGENT_SOCKETS} 0711 root root -"]
    directory(AGENT_SOCKETS.parent, 0o711)
    directory(AGENT_SOCKETS, 0o711)
    for person in cfg["members"]:
        path, group = agent_socket(person["id"]).parent, f"ss-{person['id']}"
        directory(path, 0o2750, GATEWAY, group)
        lines.append(f"d {path} 2750 {GATEWAY} {group} -")
    write(AGENT_TMPFILES, "".join(f"{line}\n" for line in lines), 0o644)
    return [agent_socket(person["id"]).parent for person in cfg["members"]]


def harness_permissions(cfg, person):
    """The officer's harness policy, plus the Subspace API's MCP tools when the agent listener or a hosted API is
    configured, and with a hosted API the officer's REST command (SUBSPACE_REQUEST).

    An operator rule that already names a subspace_ action (an allow, ask or deny) stands alone: the installer
    adds the MCP tools' grant only when the policy says nothing about them. The REST command's grant goes after
    the starter policy and before the operator's own rules (operatorRulesFrom): the last matching rule wins, so
    any operator rule that matches the command, such as a deny of `subspace-*`, decides.
    """
    permissions = person["harness"]["permissions"]
    granted = permissions
    if enrolled(cfg):
        own = person["harness"].get("operatorRulesFrom", 0)
        granted = [*permissions[:own], SUBSPACE_REQUEST_GRANT, *permissions[own:]]
    if (subspace_mcp_url(cfg) or enrolled(cfg)) and not any(
            str(rule.get("action", "")).startswith(f"{SUBSPACE_MCP}_") for rule in permissions):
        granted = [*granted, SUBSPACE_MCP_GRANT]
    return granted


def captain_permissions(cfg, person):
    """The captain root's stored grants: its harness policy plus the crew tools when it hosts crew, and the
    dispatch answer tool when agent dispatch is on."""
    permissions = harness_permissions(cfg, person)
    if not person["crew"]:
        return permissions
    return [*permissions, *CREW_GRANTS, *([DISPATCH_GRANT] if cfg["agentDispatch"] else [])]


def crew_options(cfg, person, home, engine_port, session_id, captain_directory=None):
    """Stargate crew plugin options: one worktree per repository per crew member, under the captain.

    The captain's directory is where its pinned root works: by default its workspace, which need not be a Git
    repository and lies outside every worktree; the first repository stays the default project either way.
    """
    repos = list(cfg["repositories"])

    def project(repo):
        return {"projectDirectory": str(home / "workspace" / repo),
                "worktrees": {worker["id"]: str(crew_worktree(home, worker, repo)) for worker in person["crew"]}}

    harness = person["harness"]
    default = project(repos[0])
    options = {
        "namespace": f"{cfg['slug']}-{person['id']}",
        "stateDirectory": str(home / ".local/stargate-crew"),
        "captainDirectory": str(captain_directory or default["projectDirectory"]),
        **default,
        "mesijCommand": str(MESIJ),
        "team": {
            "captain": {"id": person["id"], "agent": sessions.CAPTAIN_AGENT,
                        "model": harness_model(harness)},
            "historyScope": "team",
            "toolAccess": "curated",
            # The plugin reads crew history through this captain's own authenticated loopback engine.
            "historyAPI": {"url": f"http://{LOOPBACK}:{engine_port}",
                           "headersEnv": {"authorization": "OPENCODE_AUTHORIZATION"}},
            "modelProviders": sorted({PROVIDERS[harness["family"]], *(PROVIDERS[w["family"]] for w in person["crew"])}),
            "crew": [{
                "id": worker["id"], "name": worker["name"], "agent": worker["agent"],
                "model": model_ref(worker["family"], worker["model"]),
                # The role is not part of the crew's identity, so it carries the model family and slot.
                "role": f"Implementation and review ({crew_slot_label(worker)})",
                "permissionProfile": "coding",
            } for worker in person["crew"]],
        },
        "captainSessionID": session_id,
    }
    if cfg["usageGuard"]["enabled"]:
        options["usageGuardFile"] = str(usage_guard_file())
    if len(repos) > 1:
        options["projects"] = {repo: project(repo) for repo in repos[1:]}
    return options


def pinned_session(person, workspace, project=None):
    """A captain's confirmed root session, after checking it still fits the configured crew.

    The record is read whatever the requested roster: zero crew or no repositories must not drop a pin.
    workspace is the root's directory; project, for a captain with crew, the first repository's checkout,
    which Stargate's crew state names whether or not the root works there.
    """
    try:
        record = sessions.SessionRecord(SESSIONS, person["id"]).load()
        if not record or not record.get("confirmed"):
            return None
        sessions.check_roster(record, [worker["id"] for worker in person["crew"]])
        fixed = ("its first repository is fixed once its session is pinned, because Stargate's crew state names "
                 "that checkout. Restore the original first repository.")
        require(record.get("directory") == str(workspace), fixed)
        # Records from before the workspace root name no project: their root is the first repository.
        require(project is None or record.get("project", record.get("directory")) == str(project), fixed)
        return pattern(record.get("sessionID"), sessions.SESSION_ID.pattern, "pinned captain session ID")
    except (Invalid, sessions.SessionError) as error:
        raise Invalid(f"{officer_title(person)}: {error}") from None


def recorded_roots():
    """Each installed captain's session root mode, by captain ID, kept when a captain leaves the roster."""
    path = ETC / "session-roots.json"
    recorded = read_json(path) if os.access(path, os.R_OK) else {}
    require(isinstance(recorded, dict) and all(mode in ROOT_MODES for mode in recorded.values()),
            f"Invalid captain session root record {path}.")
    return recorded


def root_label(root, first):
    """How --dry-run and notices describe a captain's session root."""
    if root["mode"] == "workspace" or not first:
        return f"{root['path']}, the workspace above its repositories"
    return f"{root['path']}, its checkout of the first repository"


def captain_roots(cfg):
    """Where each captain's sessions start, checked before anything is installed: {captain ID: root}.

    Each root has "path", "mode" (workspace or repository) and "notice", a line for the operator or None.
    A pinned root never moves: Stargate's crew state and the gateway's backend name its directory, so a
    sessionRoot that asks for another is refused. An unpinned captain keeps the root it was installed with
    (recorded_roots; a checkout of the first repository from an installer without that record
    means it started there) unless its sessionRoot names another. A new captain starts in its workspace.
    """
    first = next(iter(cfg["repositories"]), None)
    recorded = recorded_roots()
    readable = os.access(SESSIONS, os.R_OK | os.X_OK)
    roots = {}
    for person in cfg["members"][1:]:
        agent_id = person["id"]
        workspace = STATE / "agents" / agent_id / "workspace"
        paths = {"workspace": workspace, "repository": workspace / first if first else workspace}
        configured = person.get("sessionRoot")
        record = None
        if readable:
            try:
                record = sessions.SessionRecord(SESSIONS, agent_id).load()
            except sessions.SessionError as error:
                raise Invalid(f"Captain {agent_id}: {error}") from None
        if record and record.get("confirmed"):
            pinned = Path(str(record.get("directory")))
            mode = "workspace" if pinned == workspace else "repository"
            require(configured in (None, mode),
                    f"Captain {agent_id}'s root session is pinned in {pinned}, but its sessionRoot is {configured}. "
                    "The installer never moves a pinned session: Stargate's crew state and the gateway name its "
                    f"directory. Remove sessionRoot or set it to {mode}, or give the crew to a new captain ID, "
                    "whose session starts in its workspace (see Captain sessions in the installer README).")
            root = {"path": pinned, "mode": mode, "pinned": True, "notice": None}
            if mode == "repository" and configured is None:
                root["notice"] = (
                    f"Captain {agent_id}'s root session is pinned in {pinned}, its checkout of the first repository, "
                    "and stays there: the installer never moves a pinned session, though new captains start in "
                    'their workspace. Set its sessionRoot to "repository" to keep it there without this notice; '
                    "see Captain sessions in the installer README to move it.")
            roots[agent_id] = root
            continue
        previous = recorded.get(agent_id)
        if previous is None and first and os.path.isdir(paths["repository"]):
            # Installed before the workspace root: its sessions started in the first repository's checkout.
            previous = "repository"
        mode = configured or previous or "workspace"
        root = {"path": paths[mode], "mode": mode, "pinned": False, "notice": None}
        if previous and paths[previous] != paths[mode]:
            root["notice"] = (f"Captain {agent_id} moves from {paths[previous]} to {root_label(root, first)}, as "
                              "its sessionRoot asks: its conversations start new sessions there, and earlier "
                              "sessions stay in its history.")
        elif mode == "repository" and configured is None and first:
            root["notice"] = (
                f"Captain {agent_id} keeps starting its sessions in {root_label(root, first)}, as installed; new "
                'captains start in their workspace. Set its sessionRoot to "workspace" to move it (its '
                'conversations then start new sessions), or to "repository" to keep it without this notice.')
        roots[agent_id] = root
    return roots


def session_root_lines(cfg):
    """--dry-run: where each captain's sessions will start, and any pinned or installed root that differs, after
    the rear admiral's pinned root when it has one."""
    first = next(iter(cfg["repositories"]), None)
    lines = rear_root_lines(cfg)
    for agent_id, root in captain_roots(cfg).items():
        lines.append(f"Captain {agent_id}'s root session is pinned in {root_label(root, first)}." if root["pinned"]
                     else f"Captain {agent_id}'s sessions will start in {root_label(root, first)}.")
        if root["notice"]:
            lines.append(root["notice"])
    if lines and os.geteuid() != 0 and STATE.exists():
        lines.append("Run the dry run as root to compare these with the sessions already pinned on this host.")
    return lines


def captain_pins(cfg, roots=None):
    """Each captain's pinned root session or None, checked before anything is installed."""
    roots = roots or captain_roots(cfg)
    first = next(iter(cfg["repositories"]), None)
    pins = {}
    for person in cfg["members"][1:]:
        workspace = STATE / "agents" / person["id"] / "workspace"
        project = workspace / first if first and person["crew"] else None
        pins[person["id"]] = pinned_session(person, roots[person["id"]]["path"], project)
    return pins


def pinned_directory(person):
    """The directory of an officer's confirmed root session, or None (also when this user cannot read the records)."""
    if not os.access(SESSIONS, os.R_OK | os.X_OK):
        return None
    try:
        record = sessions.SessionRecord(SESSIONS, person["id"]).load()
    except sessions.SessionError as error:
        raise Invalid(f"{officer_title(person)}: {error}") from None
    return Path(str(record.get("directory"))) if record and record.get("confirmed") else None


def rear_workspace(cfg):
    """The rear admiral's working directory: its reference checkout of the first repository, if any.

    Its own Slack bot pins its root even in a fleet without repositories, in its workspace. Like a captain's, a
    pinned root never moves, so it stays in the workspace when repositories are added (rear_root_lines says so).
    """
    rear = cfg["members"][0]
    workspace = STATE / "agents" / rear["id"] / "workspace"
    first = next(iter(cfg["repositories"]), None)
    if first and rear_root(cfg) and pinned_directory(rear) == workspace:
        return workspace
    return STATE / "references" / rear["id"] / first if first else workspace


def rear_root_lines(cfg):
    """--dry-run: where the rear admiral's root is or will be pinned, when agent dispatch or its own Slack bot pins
    it, and what that fixes."""
    if not rear_root(cfg):
        return []
    rear = cfg["members"][0]
    path = rear_workspace(cfg)
    first = next(iter(cfg["repositories"]), None)
    in_reference = path.parent.parent == STATE / "references"
    where = f"{path}, {'its reference checkout of the first repository' if in_reference else 'its workspace'}"
    if pinned_directory(rear):
        lines = [f"The rear admiral's root session is pinned in {where}."]
    else:
        reasons = [reason for reason, on in (("agent dispatch is on", cfg["agentDispatch"]),
                                             ("it has its own Slack bot", "slack" in rear)) if on]
        lines = [f"The rear admiral's root session will be pinned in {where}, because {' and '.join(reasons)}."]
    if in_reference:
        lines.append("Its first repository is then fixed, as a captain's is: the installer refuses a later change.")
    elif first:
        lines.append(f"It was pinned before the fleet had a repository and stays there, because the installer never "
                     f"moves a pinned session; its reference checkouts are under {STATE / 'references' / rear['id']}.")
    else:
        lines.append("It stays in the workspace when repositories are added: the installer never moves a pinned "
                     "session.")
    return lines


def rear_pin(cfg):
    """The rear admiral's pinned root session while agent dispatch is on or it has its own Slack bot, or None.

    Captains name it in their fleetControl target, which is how they recognise their rear admiral's offers.
    """
    if not rear_root(cfg):
        return None
    return pinned_session(cfg["members"][0], rear_workspace(cfg))


def fleet_socket(agent_id):
    return STATE / "fleet-control" / agent_id / "fleet.sock"


def fleet_control(cfg, person, pin, rear_session, gateway_home):
    """Stargate fleetControl for a captain that answers dispatch offers, or None.

    It needs the captain's own pin and the rear admiral's, whose target entry (role rear_admiral) is how the
    captain recognises dispatch requests. The target channel is the captain's ship, where the rear admiral listens.
    """
    if not (cfg["agentDispatch"] and person["role"] == "captain" and pin and rear_session):
        return None
    rear = cfg["members"][0]
    return {
        "socketPath": str(fleet_socket(person["id"])),
        "statePath": str(gateway_home / "fleet-control" / "evidence.sqlite"),
        "targets": [{"id": rear["id"], "account": rear["account"], "role": "rear_admiral",
                     "channel": person["channel"], "sessionID": rear_session}],
    }


def ready_marker(agent_id):
    """Written once a captain's engine and gateway configuration carry its pin; the gateway unit waits for it."""
    return SESSIONS / f"{agent_id}.ready"


def withdraw_ready(pins):
    """Remove readiness markers of captains this render does not pin, before their configuration changes."""
    if not SESSIONS.is_dir():
        return
    for marker in SESSIONS.glob("*.ready"):
        if not pins.get(marker.name.removesuffix(".ready")):
            marker.unlink(missing_ok=True)


def publish_ready(pins):
    """Mark pinned captains ready, after their configuration is installed and flushed to disk."""
    pinned = {agent_id: session_id for agent_id, session_id in pins.items() if session_id}
    if not pinned:
        return
    os.sync()
    for agent_id, session_id in pinned.items():
        marker = ready_marker(agent_id)
        if not marker.is_file() or marker.is_symlink() or marker.read_text() != f"{session_id}\n":
            sessions.write_durable(marker, f"{session_id}\n")


def officer_tool_environment(cfg, person, home):
    """The environment an officer runs its tools in, and its crew with it (toolchains.officer_environment): its
    own mise data directory, the shared store as mise's system data directory, mise configuration trusted where
    it works (its workspace and its crew's worktrees; the rear admiral's reference checkouts), a private TMPDIR."""
    trusted = [home / "workspace", home / "worktrees"] if person["role"] == "captain" else \
        [home / "workspace", STATE / "references" / person["id"]]
    return toolchains.officer_environment(cfg["toolchains"], f"ss-{person['id']}", home, trusted)


def render_engine(cfg, person, home, workspace, engine_port, seed, documents, session_id, answers_offers=False,
                  limits=None):
    agent_id = person["id"]
    user = f"ss-{agent_id}"
    database_dir = STATE / "sessions" / agent_id
    directory(database_dir, 0o2750, user, VIEW)
    harness = person["harness"]
    # One provider per model family on this server: the captain's and its crew's.
    catalogue = {harness["family"]: [harness["model"]]}
    for worker in person["crew"]:
        models = catalogue.setdefault(worker["family"], [])
        if worker["model"] not in models:
            models.append(worker["model"])
    permissions = harness_permissions(cfg, person)
    engine = {
        "model": harness_model(harness), "permissions": permissions,
        "providers": {PROVIDERS[family]: model_provider(cfg, family, models) for family, models in catalogue.items()},
    }
    if enrolled(cfg):
        # A hosted API: Stargate's stdio bridge to the officer's signing proxy, which signs as the officer with
        # its enrolled key. The engine holds no credential and names no agent.
        engine["mcp"] = {"servers": {SUBSPACE_MCP: {"type": "local", "command": subspace_bridge(cfg, agent_id)}}}
    elif mcp_url := subspace_mcp_url(cfg):
        # The Subspace API's agent listener: the officer, and the crew sessions on this server, act as the
        # officer's own agent record, named by its IRC account. The listener needs no token.
        engine["mcp"] = {"servers": {SUBSPACE_MCP: {
            "type": "remote", "url": mcp_url, "headers": {AGENT_HEADER: person["account"]},
        }}}
    password = hmac.new(seed, agent_id.encode(), hashlib.sha256).hexdigest()
    header = "Basic " + base64.b64encode(f"opencode:{password}".encode()).decode()
    environment = {
        "OPENCODE_DB": str(database_dir / "opencode.db"),
        "OPENCODE_PASSWORD": password,
        "CLIPROXYAPI_API_KEY": cfg["proxy"]["key"],
        "OPENCODE_DISABLE_AUTOUPDATE": "true",
        "OPENCODE_DISABLE_MODELS_FETCH": "true",
        "OPENCODE_DISABLE_TERMINAL_TITLE": "true",
        **officer_tool_environment(cfg, person, home),
    }
    auth_var = "ENGINE_" + agent_id.upper().replace("-", "_")
    backend = {"type": "opencode", "url": f"http://{LOOPBACK}:{engine_port}", "directory": str(workspace),
               "permissions": permissions, "headersEnv": {"authorization": auth_var}}
    if person["crew"]:
        # Crew sessions run on this server, as this captain's Linux user, each with its own native agent.
        environment["OPENCODE_AUTHORIZATION"] = header
        engine["default_agent"] = sessions.CAPTAIN_AGENT
        engine["agents"] = {sessions.CAPTAIN_AGENT: {"model": engine["model"], "mode": "primary"}}
        for worker in person["crew"]:
            engine["agents"][worker["agent"]] = {
                "model": model_ref(worker["family"], worker["model"]), "mode": "primary",
                "description": f"{worker['name']}, {person['name']}'s crew member ({crew_slot_label(worker)})",
                "system": documents["crew"][worker["id"]],
            }
        if session_id:
            engine["plugins"] = [{
                "package": str(APP / "src" / f"stargate-{cfg['stargate']['ref']}"),
                "options": crew_options(cfg, person, home, engine_port, session_id, workspace),
            }]
            if answers_offers:
                # Stargate's fleet plugin gives the pinned root fleet_answer_offer, over the gateway's socket.
                engine["plugins"].append({
                    "package": str(APP / "src" / f"stargate-{cfg['stargate']['ref']}" / "src/fleet"),
                    "options": {"socketPath": str(fleet_socket(agent_id))},
                })
            backend["sessionID"] = session_id
        backend.update(agent=sessions.CAPTAIN_AGENT, permissions=captain_permissions(cfg, person), enforcePermissions=True)
    elif person["role"] == "rear_admiral" and session_id:
        # Agent dispatch pins the rear admiral's IRC registration to one root, which captains name as their
        # rear admiral. Slack and Teams keep their own sessions (render passes them an unpinned copy).
        backend["sessionID"] = session_id
    environment["OPENCODE_CONFIG"] = str(save_settings(f"opencode-{agent_id}", engine, user))
    settings = save_settings(f"engine-{agent_id}", {
        "environment": environment, "port": engine_port, "directory": str(workspace), "slug": cfg["slug"],
    }, user)
    # The engine's tool environment is in its settings (runtime.py engine), not the unit's.
    unit = serve_unit(
        f"engine-{agent_id}", f"OpenCode engine {agent_id}",
        f"{APP}/lib/runtime.py engine {settings}", user, [f"{PREFIX}proxy.service"],
        user_paths=[home, database_dir], post_start=f"{APP}/lib/runtime.py share-database {settings}",
        toolchain=False,
        # The officer's memory and task limits (lib/memory.py); its crew run in this engine, within them.
        service_options=memory.unit_options(limits) if limits else (),
    )
    return unit, backend, {auth_var: header}, database_dir


def memory_plan(cfg, total=None):
    """Each officer's memory and task limits for this VM (lib/memory.py), from its memory and the fleet's shape."""
    api = cfg["api"]
    # The API and its PostgreSQL run on this VM: memory.localApi, which subspace-setup sets whatever the API's
    # sign-in (its api block is there only with development sign-in), or an api block on this VM's loopback.
    api_here = cfg["memory"]["localApi"] or (bool(api) and not enrolled(cfg)
                                             and urlsplit(api["url"]).hostname in {"localhost", LOOPBACK, "::1"})
    officers = [{"id": person["id"], "role": person["role"], "memory": person["memory"]} for person in cfg["members"]]
    return memory.plan(cfg["memory"], officers, memory.total_memory() if total is None else total,
                       api_here=api_here, irc_here=cfg["irc"]["mode"] == "local")


def officer_unit(cfg, person):
    """The unit an officer's work runs in: its OpenCode engine, or a Claude Agent SDK officer's own gateway."""
    if person["harness"]["type"] == "opencode":
        return f"{PREFIX}engine-{person['id']}.service"
    if person["role"] == "rear_admiral":
        return f"{PREFIX}gateway.service"
    return f"{PREFIX}gateway-agent-{person['id']}.service"


def memory_lines(cfg, installed=False, managed=True):
    """The dry run's and the install summary's account of memory: each engine's limits and why, limits set by hand,
    and swap (left alone by an install with --no-systemd, which managed false says)."""
    result = memory_plan(cfg)
    officers = {person["id"]: officer_unit(cfg, person) for person in cfg["members"]}
    rear, gateways = cfg["members"][0], {}
    if rear["harness"]["type"] != "opencode":
        # A Claude Agent SDK rear admiral's work runs in the fleet's own gateway, which takes its limits; the OpenCode
        # officers, if any, share another, which has none (render's gateway groups).
        opencode = any(person["harness"]["type"] == "opencode" for person in cfg["members"])
        gateways = {"rear_gateway": officers[rear["id"]],
                    "shared_gateway": f"{PREFIX}gateway-opencode.service" if opencode else None}
    return [*memory.plan_lines(result, **gateways), *memory.hand_made_lines(officers, result),
            *memory.swap_lines(cfg["memory"]["swap"], memory.swap_state(), result["total"], installed, managed)]


def claude_backend(cfg, person, workspace, documents):
    """A Claude Agent SDK officer's Stargate backend, with the Subspace API's MCP server when it has the agent
    listener."""
    harness = person["harness"]
    backend = {
        "type": "claude", "directory": str(workspace), "model": harness["model"], "allowedTools": harness["allowedTools"],
        **({"effort": harness["effort"]} if harness["effort"] else {}),
        "systemPrompt": documents["claudePrompt"],
        "provider": {"baseURL": f"http://{LOOPBACK}:{cfg['proxy']['port']}/v1", "apiKeyEnv": "CLIPROXYAPI_API_KEY"},
    }
    if enrolled(cfg):
        # The officer's own gateway process, as its own user, runs Stargate's bridge to its signing-proxy socket,
        # whose directory admits that user's group.
        command, *arguments = subspace_bridge(cfg, person["id"])
        backend["mcpServers"] = {SUBSPACE_MCP: {"type": "stdio", "command": command, "args": arguments}}
    elif mcp_url := subspace_mcp_url(cfg):
        # Stargate's Claude backend takes header values only from a helper command, and permits every
        # tool of a configured server itself (mcp__subspace); allowedTools holds built-in tools only.
        backend["mcpServers"] = {SUBSPACE_MCP: {
            "type": "http", "url": mcp_url,
            "headersHelper": f"{APP}/lib/runtime.py agent-header {person['account']}",
        }}
    return backend


def irc_agent(cfg, person, backend):
    agent_id = person["id"]
    if cfg["irc"]["auth"] == "certificate":
        # SASL EXTERNAL: the gateway reads the officer's certificate and key; no password variable exists.
        return {
            "id": agent_id, "displayName": person["name"], "account": person["account"],
            "clientPemFile": str(irc_client_pem(agent_id)), "backend": backend,
        }, {}
    password_var = "IRC_" + agent_id.upper().replace("-", "_")
    return {
        "id": agent_id, "displayName": person["name"], "account": person["account"],
        "passwordEnv": password_var, "backend": backend,
    }, {password_var: cfg["passwords"][person["account"]]}


def check_sign_in(registration, server):
    """The pinned Stargate's rule per IRC account: exactly one of passwordEnv (SASL PLAIN) and clientPemFile
    (SASL EXTERNAL), and a client certificate only over TLS."""
    require(("passwordEnv" in registration) != ("clientPemFile" in registration),
            f"IRC account {registration['account']} needs exactly one of a password and a client certificate.")
    require("clientPemFile" not in registration or server["tls"],
            f"IRC account {registration['account']} signs in with a client certificate, which needs TLS.")


def irc_client_pem(agent_id):
    return IRC_CERTS / f"{agent_id}.pem"


def irc_gateway_user(person):
    """The Linux user whose gateway serves an officer: its own for a Claude SDK officer, else the shared one."""
    return GATEWAY if person["harness"]["type"] == "opencode" else f"ss-{person['id']}"


def irc_fingerprint(bundle, label):
    """Check a client bundle as Stargate does and return the SHA-256 fingerprint of its first certificate, the
    lower-case hex form that Ergo's NickServ CERT ADD takes."""
    certificates = IRC_PEM_CERTIFICATE.findall(bundle)
    require(len(bundle.encode()) <= IRC_PEM_BYTES and certificates and IRC_PEM_KEY.search(bundle),
            f"{label} must hold a PEM certificate and its unencrypted private key, at most 64 KiB.")
    with tempfile.TemporaryDirectory() as work:
        path = Path(work) / "bundle.pem"
        path.write_text(bundle)
        path.chmod(0o600)
        try:
            # OpenSSL takes the first certificate as the leaf and checks that the key matches it, as Stargate does.
            ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT).load_cert_chain(path, password=lambda: b"")
            leaf = ssl.PEM_cert_to_DER_cert(certificates[0])
        except (ssl.SSLError, ValueError):
            raise Invalid(f"{label}: the first certificate does not match an unencrypted private key.") from None
    return hashlib.sha256(leaf).hexdigest()


def read_irc_bundle(path):
    """An installed bundle's text, or None when there is none. Reads only a regular file, never a link."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    except FileNotFoundError:
        return None
    except OSError:
        raise Invalid(f"Cannot read the IRC certificate bundle at {path}.") from None
    with os.fdopen(fd, "rb") as stream:
        require(stat.S_ISREG(os.fstat(fd).st_mode), f"Expected a regular file at {path}.")
        contents = stream.read(IRC_PEM_BYTES + 1)
    try:
        return contents.decode("ascii")
    except UnicodeError:
        raise Invalid(f"Expected a PEM certificate bundle at {path}.") from None


def irc_bundles(cfg, directory):
    """Bundles an operator supplies as <secrets-dir>/<officer-id>/irc-client.pem, checked, by officer ID."""
    found = {}
    if cfg["irc"]["auth"] != "certificate" or directory is None:
        return found
    try:
        with private_directory(directory) as root:
            for person in cfg["members"] if root is not None else ():
                with private_directory(person["id"], root) as agent:
                    if agent is not None:
                        found[person["id"]] = private_file(IRC_CLIENT_PEM, optional=True, dir_fd=agent)
    except ValueError as error:
        if isinstance(error, Invalid):
            raise
        raise Invalid(f"Cannot read an IRC certificate bundle in {directory}: {error} Supply owner-only regular "
                      f"{IRC_CLIENT_PEM} files (chmod 600) in private directories.") from None
    bundles = {}
    for agent_id, contents in found.items():
        if contents is None:
            continue
        label = str(Path(directory) / agent_id / IRC_CLIENT_PEM)
        try:
            bundles[agent_id] = contents.decode("ascii")
        except UnicodeError:
            raise Invalid(f"Expected a PEM certificate bundle at {label}.") from None
        irc_fingerprint(bundles[agent_id], label)
    return bundles


def new_irc_bundle(account):
    """A new P-256 key and self-signed certificate for an officer's IRC account, as one PEM bundle."""
    with tempfile.TemporaryDirectory() as work:
        key, certificate = Path(work) / "key.pem", Path(work) / "certificate.pem"
        subprocess.run(
            # A named curve: an OpenSSL that writes explicit curve parameters (LibreSSL) makes a certificate that
            # Go TLS servers such as Ergo refuse during the handshake.
            ["openssl", "req", "-x509", "-newkey", "ec", "-pkeyopt", "ec_paramgen_curve:prime256v1",
             "-pkeyopt", "ec_param_enc:named_curve", "-nodes",
             "-sha256", "-days", "3650", "-subj", f"/CN={account}", "-keyout", key, "-out", certificate],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        return certificate.read_text() + key.read_text()


def own_file(path, content, user):
    """Write a file owned by user with mode 0600: Stargate takes a client PEM only from its own, owner-only file."""
    account = pwd.getpwnam(user)
    try:
        info = path.lstat()
    except FileNotFoundError:
        info = None
    if (info and stat.S_ISREG(info.st_mode) and (info.st_uid, info.st_gid) == (account.pw_uid, account.pw_gid)
            and stat.S_IMODE(info.st_mode) == 0o600 and read_irc_bundle(path) == content):
        return
    (STATE / "pending").touch()
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False, encoding="ascii") as stream:
        temp = Path(stream.name)
        try:
            os.fchmod(stream.fileno(), 0o600)
            os.fchown(stream.fileno(), account.pw_uid, account.pw_gid)
            stream.write(content)
            stream.close()
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)


def irc_certificates(cfg):
    """Give each officer's gateway user its IRC certificate bundle: the installed or supplied one, else a new one.

    The installer keeps it across upgrades; install.sh puts a supplied bundle in place before rendering.
    """
    directory(IRC_CERTS, 0o711)
    for person in cfg["members"]:
        path = irc_client_pem(person["id"])
        bundle = read_irc_bundle(path)
        if bundle is None:
            bundle = new_irc_bundle(person["account"])
        irc_fingerprint(bundle, str(path))
        own_file(path, bundle, irc_gateway_user(person))


def irc_sign_in(cfg, bundles=None, installed=True):
    """What the operator needs to know about the officers' IRC sign-in, and each certificate's fingerprint.

    Never prints key material: only the SHA-256 fingerprints of the certificates.
    """
    irc = cfg["irc"]
    if irc["auth"] != "certificate":
        return ["Officers sign in to IRC with passwords (SASL PLAIN, irc.auth password)."]
    lines = ["Officers sign in to IRC with certificates (SASL EXTERNAL, irc.auth certificate). An officer can sign "
             f"in only once its fingerprint is bound to its account on {irc['host']}. Bind each one as an IRC "
             "operator with NickServ's accreg capability, as below, or signed in as the account itself, without "
             "the account name:"]
    for person in cfg["members"]:
        bundle = (bundles or {}).get(person["id"])
        source = "supplied"
        if bundle is None and os.access(irc_client_pem(person["id"]), os.R_OK):
            bundle, source = read_irc_bundle(irc_client_pem(person["id"])), "installed"
        if bundle is None:
            lines.append(f"  {person['account']}: a new certificate is generated on install; the install summary "
                         "prints its fingerprint.")
            continue
        fingerprint = irc_fingerprint(bundle, f"The IRC certificate of {person['id']}")
        note = "" if installed else f" ({source})"
        lines.append(f"  /msg NickServ CERT ADD {person['account']} {fingerprint}{note}")
    return lines


def irc_officer(cfg, person, registration):
    server = {key: cfg["irc"][key] for key in ("host", "port", "tls")}
    if "caFile" in cfg["irc"]:
        server["caFile"] = str(ETC / "irc-ca.pem")
    room = person["channel"]
    role = person["role"]
    channels = [room, *(name for name in cfg["irc"]["channels"] if name not in (room, cfg["irc"]["starbaseChannel"]))]
    candidates = [member for member in cfg["members"] if member["account"] != person["account"]]
    candidates.sort(key=lambda member: member["channel"] != room)
    peers = [member["account"] for member in candidates[:50]]
    medical = cfg["medical"]
    if medical and medical["target"] and medical["target"]["id"] == person["id"]:
        peers = [medical["account"], *peers[:49]]
    access = {"allCallWords": [], "channelRequests": "nick-mention", "peerCaptains": peers,
              "humanWakeChannels": [{"channel": room, "replies": "all"}]}
    return {
        # Crew have no IRC registrations: their captain reaches them through Stargate's crew plugin.
        **registration, "server": server, "channels": channels, "access": access,
        # Direct messages are for people only: an agent answers one with a notice naming its first
        # channel, Starbase for the rear admiral and its ship for a captain, and starts no turn.
        "dmPolicy": "redirect",
        # Route by authenticated account in strict mode.
        "features": {"botMode": False},
        "fleetPolicy": {"role": role, "humanAccounts": cfg["irc"]["allowedAccounts"],
                        "agentAccounts": sorted({person["account"], *peers}), "assignedCheckout": registration["backend"]["directory"],
                        "capabilities": ["read_code"] if role == "rear_admiral" else ["read_code", "implement"]},
    }


def render_proxy(cfg, tokens):
    home = account(PROXY, STATE / "proxy")
    proxy = cfg["proxy"]
    options = {
        "server": {"host": LOOPBACK, "port": proxy["port"], "kv-store": {"disabled": True}},
        "oauth": {"auth-dir": str(home / "auth")},
        "access": {"api-keys": [proxy["key"]]},
        "management": {"allow-remote": False, "disable-control-panel": True},
        "routing": {"retry": {"request-retry": proxy["requestRetries"]}},
        "plugins": {"enabled": False},
    }
    if "proxy-management-key" in tokens:
        options["management"]["secret-key"] = tokens["proxy-management-key"]
    config_path = save_settings("proxy", options, PROXY)
    return serve_unit("proxy", "CLIProxyAPI model proxy", f"{APP}/bin/cli-proxy-api --local-model --config {config_path}", PROXY, user_paths=[home],
                      service_options=memory.SHARED_OPTIONS)


def render_view(cfg, session_roots, home):
    roots = json.dumps([str(path) for path in session_roots])
    source = ETC / "agentsview.toml"
    write(source, f"[agents.opencode]\ndirs = {roots}\n", 0o640, VIEW)
    run(APP / "lib/runtime.py", "view-config", source, home / "config.toml", user=VIEW)
    return serve_unit(
        "view", "AgentsView session browser",
        f"{APP}/bin/agentsview serve --host {LOOPBACK} --port {cfg['agentsview']['port']} --no-browser --no-update-check",
        VIEW, env={"AGENTSVIEW_DATA_DIR": str(home), "AGENTSVIEW_TELEMETRY_ENABLED": "0"},
        user_paths=[home],
    )


def render_firewall(cfg):
    try:
        admins = {pwd.getpwnam(user).pw_uid for user in cfg["operators"]}
    except KeyError:
        raise Invalid("Every operatorUsers entry must name an existing Linux account.") from None
    service_ids = {int(path.read_text()) for path in (STATE / "identity").iterdir()}
    require(not admins & service_ids, "Fleet service UIDs cannot be operators.")
    clients = {pwd.getpwnam(f"ss-{person['id']}").pw_uid for person in cfg["members"]}
    clients.add(pwd.getpwnam(GATEWAY).pw_uid)
    ports = {service: cfg[service]["port"] for service in ("dashboard", "agentsview", "proxy")}
    if cfg["stargate"]["api"]:
        ports["stargate-api"] = cfg["stargate"]["api"]["port"]
    config = ETC / "firewall.nft"
    write(config, rules(ports, admins, clients, pwd.getpwnam(VIEW).pw_uid), 0o600)
    write(SYSTEMD / FIREWALL, "\n".join([
        "[Unit]", "Description=Subspace fleet local service firewall",
        "After=nftables.service",
        "", "[Service]", "Type=oneshot", "RemainAfterExit=yes",
        f"ExecStart=/usr/sbin/nft -f {config}",
        "", "[Install]", "WantedBy=multi-user.target", "",
    ]), 0o644)
    return FIREWALL


def install_toolchains(cfg, homes, update=False):
    """Install the configured toolchains (ETC/toolchains.toml is written first), or with update, upgrade them within
    their configured versions. Shared tools: as root, explicit tool@version arguments, from /opt/subspace-fleet,
    into the shared store. Per-user tools: as each officer's user, the same way, from its home, into its own mise
    data directory. Neither runs mise in a checkout, so no repository's configuration is read."""
    tools = cfg["toolchains"]
    directory(TOOL_HOME, 0o755)
    directory(TOOL_HOME / "system-config", 0o755)

    def command(arguments):
        names = list(dict.fromkeys(argument.rsplit("@", 1)[0] for argument in arguments))
        return [str(toolchains.MISE), *(["upgrade", "--no-prune", *names] if update else ["install", *arguments])]

    if shared := toolchains.requests(tools, "shared"):
        previous = os.umask(0o022)
        environment = toolchains.root_environment(tools)
        try:
            subprocess.run(command(shared), cwd=APP, env=environment, check=True)
        except subprocess.CalledProcessError as error:
            raise Invalid(f"root's mise could not install the shared tools (it exited {error.returncode}; its messages "
                          "are above). Root's mise does not use the backends that run third-party code while they "
                          f"install ({environment['MISE_DISABLE_BACKENDS']}), so a tool that mise's registry installs "
                          "only that way, such as poetry or prettier, must be installed per officer: give it "
                          "\"install\": \"user\" in fleet.json's toolchains.") from None
        finally:
            os.umask(previous)
    if personal := toolchains.requests(tools, "user"):
        for person in cfg["members"]:
            home = homes[person["id"]]
            # The installer is not in the engine's private /tmp, so mise keeps the system's temporary directory.
            environment = {key: value for key, value in officer_tool_environment(cfg, person, home).items()
                           if key != "TMPDIR"}
            subprocess.run(["runuser", "-u", f"ss-{person['id']}", "--", *toolchains.clean_environment(
                f"ss-{person['id']}", home, environment), *command(personal)], cwd=home, check=True)


def toolchain_plan(cfg):
    """--dry-run: the toolchains this run installs and where, and what changes from the installed fleet.json."""
    officers = {f"ss-{person['id']}": STATE / "agents" / person["id"] for person in cfg["members"]}
    previous = None
    installed = ETC / "fleet.json"
    if os.access(installed, os.R_OK):
        try:
            saved = read_json(installed, private=True)
            previous = (toolchain_config(saved.get("toolchains", {})),
                        toolchains.package_config(saved.get("systemPackages", [])))
        except (Invalid, AttributeError):
            previous = None
    return toolchains.plan_lines(cfg["toolchains"], cfg["systemPackages"], officers, previous)


def verify_toolchains():
    """Run each repository's basic check as a crew member would: as its captain's user, in a crew worktree (the
    checkout for a captain without crew), with the engine's tool environment. Each tool the repository's files
    name is asked its version where the file that names it is, and that version must meet what the file asks for
    (a tool that merely runs is not enough; one the system already provides, such as Python, passes when its
    version meets the request); then the repository's own check, if fleet.json gives one. Reports pass or fail
    per repository and never fails the install."""
    cfg = validate(read_json(ETC / "fleet.json", private=True), read_json(ETC / "secrets.json", private=True),
                   check_files=False)
    captains = [person for person in cfg["members"] if person["role"] == "captain"]
    if not cfg["repositories"] or not captains:
        print("Toolchain check: no repositories or no captains, so nothing to check.")
        return
    captain = next((person for person in captains if person["crew"]), captains[0])
    home, user = STATE / "agents" / captain["id"], f"ss-{captain['id']}"
    environment = officer_tool_environment(cfg, captain, home)
    rear = cfg["members"][0]["id"]
    for repo_id in cfg["repositories"]:
        directory = crew_worktree(home, captain["crew"][0], repo_id) if captain["crew"] else home / "workspace" / repo_id
        # Which tools the repository names: read as data from the rear admiral's reference checkout.
        found = toolchains.detect({repo_id: STATE / "references" / rear / repo_id})
        checks, unchecked = toolchains.verification_checks(found, cfg["toolchains"],
                                                           cfg["repositoryChecks"].get(repo_id))
        if not directory.is_dir():
            passed, report = False, f"{directory} does not exist"
        elif not checks:
            passed, report = True, "no tool to check and no check configured"
        else:
            passed, report = toolchains.verify_repository(user, home, directory, environment, checks, 900)
        if unchecked:
            passed, report = False, f"{report}; named by its files but not in toolchains: {', '.join(unchecked)}"
        print(f"Toolchain check of {repo_id}, as {user} in {directory}: {'PASS' if passed else 'FAIL'} ({report}).")


def install_screenshots(cfg):
    """The base screenshot tools (lib/screenshots.py), as root, unless fleet.json turns them off. A problem is
    reported and never stops the install; the next run tries again, and the installer's screenshot check says
    whether a crew member can take one."""
    if not cfg["screenshots"]:
        return
    for problem in screenshots.install(screenshots.pins(), toolchains.root_environment(cfg["toolchains"])):
        print(f"WARNING: the screenshot tools are not complete: {problem}. The install carries on; the next run "
              "tries again.", file=sys.stderr)


def check_screenshots():
    """The installer's screenshot check: one screenshot of a local page as a crew member would take it, as the
    user of a captain with crew (else any captain, else the rear admiral), with its tool environment. Reports pass
    or fail, and never fails the install."""
    cfg = validate(read_json(ETC / "fleet.json", private=True), read_json(ETC / "secrets.json", private=True),
                   check_files=False)
    if not cfg["screenshots"]:
        print("Screenshot check: screenshots are off (\"screenshots\": false in fleet.json).")
        return
    captains = [person for person in cfg["members"] if person["role"] == "captain"]
    person = next((captain for captain in captains if captain["crew"]), captains[0] if captains else cfg["members"][0])
    home, user = STATE / "agents" / person["id"], f"ss-{person['id']}"
    passed, report = screenshots.check(user, home, officer_tool_environment(cfg, person, home))
    if passed:
        print(f"Screenshot check, as {user}: PASS ({report}).")
    else:
        print(f"WARNING: screenshot check, as {user}: FAIL ({report}). Officers and crew may not be able to take "
              "screenshots; the install carries on. See the installer README, 'Screenshots'.")


def maintenance_units(cfg):
    settings = cfg["maintenance"]
    actions = {
        "mirrors": ("mirrorCalendar", "Refresh fleet repository mirrors"),
        "tools": ("toolsCalendar", "Update configured fleet development tools"),
        "prune": ("pruneCalendar", "Prune fleet logs and stale Git worktree records"),
    }
    timers = []
    for action, (key, description) in actions.items():
        if key not in settings or action == "mirrors" and not cfg["repositories"] or action == "tools" and not cfg["toolchains"]:
            continue
        calendar = settings[key]
        run("systemd-analyze", "calendar", "--", calendar, quiet=True)
        unit = f"{PREFIX}{action}"
        write(SYSTEMD / f"{unit}.service", "\n".join([
            "[Unit]", f"Description={description}", "After=network-online.target", "Wants=network-online.target",
            "", "[Service]", "Type=oneshot", "Nice=10", "UMask=0077",
            f"ExecStart={APP}/lib/runtime.py maintenance {action}", "",
        ]), 0o644)
        timer = f"{unit}.timer"
        write(SYSTEMD / timer, "\n".join([
            "[Unit]", f"Description={description}", "", "[Timer]", f"OnCalendar={calendar}",
            "Persistent=true", "RandomizedDelaySec=5min", "", "[Install]", "WantedBy=timers.target", "",
        ]), 0o644)
        timers.extend((f"{unit}.service", timer))
    return timers


def readable_without_credentials(url):
    """Whether Git can list a repository's branches with no credentials, as the installer clones a public one."""
    environment = {**os.environ, "GIT_TERMINAL_PROMPT": "0", "GIT_ASKPASS": "true", "SSH_ASKPASS": "true"}
    try:
        return subprocess.run(["git", "-c", "credential.helper=", "ls-remote", "--heads", "--", url], env=environment,
                              stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                              timeout=30, check=False).returncode == 0
    except (OSError, subprocess.TimeoutExpired):
        return False


def unreadable_repositories(cfg):
    """Repositories the installer would clone as public that Git cannot read without credentials, such as
    private ones that fleet.json does not mark private. Those already installed are not checked again."""
    path = ETC / "repository-sources.json"
    installed = read_json(path, private=True) if os.access(path, os.R_OK) else {}
    return [name for name, repo in cfg["repositories"].items()
            if not repo["private"] and installed.get(name) != repo and not readable_without_credentials(repo["url"])]


def unreadable_message(names):
    return (f"Git cannot read {', '.join(names)} without credentials, so the installer cannot clone "
            f"{'it' if len(names) == 1 else 'them'} as public repositories. If they are private, install without "
            "them now (leave them out of repositories: subspace-setup --repositories with the others, or '' for "
            "none). Once the rear admiral and every captain have a GitHub App that can read them, add each "
            "officer's githubApp and key, and the repositories marked \"private\": true, and run the installer "
            "again (installer README, 'Add private repositories later'). Otherwise check the URLs.")


# The account type of CLIProxyAPI's that serves each family's models by their own IDs: Claude models through a
# Claude account, GPT models through a Codex (ChatGPT) account.
FAMILY_ACCOUNTS = {"claude": ("claude", "claude-"), "codex": ("codex", "gpt-")}
ACCOUNT_LOGINS = {"claude": "claude", "codex": "codex-device"}


def proxy_accounts():
    """The types of CLIProxyAPI's signed-in accounts (each auth file's type, as providers.sh lists them), or none
    before the first sign-in."""
    types = set()
    try:
        paths = sorted((STATE / "proxy" / "auth").glob("*.json"))
    except OSError:
        return types
    for path in paths:
        try:
            kind = json.loads(path.read_text()).get("type")
        except (OSError, ValueError, AttributeError):
            continue
        if isinstance(kind, str):
            types.add(kind)
    return types


def unserved_officers(cfg, accounts):
    """Officers whose Claude or GPT model no signed-in account serves, as (officer, model, account type).
    Nothing is checked before the first sign-in, nor a model of another provider."""
    if not accounts:
        return []
    unserved = []
    for person in cfg["members"]:
        harness = person["harness"]
        kind, prefix = FAMILY_ACCOUNTS[harness["family"]]
        if harness["model"].startswith(prefix) and kind not in accounts:
            unserved.append((person["id"], harness["model"], kind))
    return unserved


def officer_models_line(cfg):
    """The officers' models and efforts, and the account type CLIProxyAPI needs for them."""
    groups = {}
    for person in cfg["members"]:
        harness = person["harness"]
        groups.setdefault((harness["model"], harness["effort"]), []).append(person["id"])
    kinds = sorted({kind for person in cfg["members"]
                    for kind, prefix in [FAMILY_ACCOUNTS[person["harness"]["family"]]]
                    if person["harness"]["model"].startswith(prefix)})
    parts = [f"{', '.join(ids)} on {model}" + (f" at {effort} effort" if effort else " at the provider's default effort")
             for (model, effort), ids in groups.items()]
    need = f" The model proxy needs a {' and a '.join(kinds)} account for them." if kinds else ""
    return f"Officer models: {'; '.join(parts)}.{need}"


def captain_tools_line(cfg):
    """--dry-run: which captains code and which delegate coding to their crew (captainTools)."""
    captains = cfg["members"][1:]
    if not captains:
        return None
    if cfg["captainTools"] == "standard":
        return "Captain tools: standard. Captains edit, commit and run builds and tests themselves."
    delegates = [person["id"] for person in captains if person.get("delegatesCoding")]
    solo = [person["id"] for person in captains if not person.get("delegatesCoding")]
    line = "Captain tools: delegate."
    if delegates:
        line += (f" {', '.join(delegates)} leave edits, commits and pushes to their crew: no edit tool, and a shell "
                 "that runs anything but the Git commands that change a checkout or publish it, and sed -i and perl -i.")
    if solo:
        line += (f" {', '.join(solo)} {'has no crew, so keeps' if len(solo) == 1 else 'have no crew, so keep'} "
                 "the standard policy.")
    return line


def policy_lines(cfg):
    """--dry-run: each OpenCode officer's tool policy against the one installed in its engine configuration: the
    rules this run adds (+) and removes (-), in policy order, since the last matching rule wins. Officers with
    the same changes share one entry."""

    def text(rule):
        return f"{rule['action']} {json.dumps(rule['resource'])} {rule['effect']}"

    changed, unchanged, new, unreadable = {}, [], [], []
    for person in cfg["members"]:
        if person["harness"]["type"] != "opencode":
            continue
        path = ETC / f"opencode-{person['id']}.json"
        policy = [text(rule) for rule in harness_permissions(cfg, person)]
        if not path.exists():
            new.append(f"{person['id']} ({len(policy)} rules)")
            continue
        if not os.access(path, os.R_OK):
            unreadable.append(person["id"])
            continue
        try:
            installed = [text(rule) for rule in read_json(path).get("permissions", [])]
        except (Invalid, AttributeError, KeyError, TypeError):
            unreadable.append(person["id"])
            continue
        lines = []
        for tag, old_start, old_end, new_start, new_end in difflib.SequenceMatcher(
                None, installed, policy, autojunk=False).get_opcodes():
            if tag != "equal":
                lines += [f"  - {rule}" for rule in installed[old_start:old_end]]
                lines += [f"  + {rule}" for rule in policy[new_start:new_end]]
        if lines:
            changed.setdefault(tuple(lines), []).append(person["id"])
        else:
            unchanged.append(person["id"])
    output = []
    for lines, ids in changed.items():
        added = sum(line.startswith("  +") for line in lines)
        output.append(f"Tool policy of {', '.join(ids)} changes: {added} rules added and {len(lines) - added} removed "
                      "(OpenCode permissions; the last matching rule wins):")
        output.extend(lines)
    if unchanged:
        output.append(f"Tool policy unchanged: {', '.join(unchanged)}.")
    if new:
        output.append(f"Tool policy of officers with none installed yet: {', '.join(new)}.")
    if unreadable:
        output.append(f"Run the dry run as root to compare the tool policy of {', '.join(unreadable)} with the "
                      "installed one.")
    return output


def unserved_message(unserved, accounts):
    models = ", ".join(sorted({f"{model} ({kind} account)" for _, model, kind in unserved}))
    officers = ", ".join(officer for officer, _, _ in unserved)
    logins = " and ".join(sorted({ACCOUNT_LOGINS[kind] for _, _, kind in unserved}))
    return (f"The model proxy is signed in to {', '.join(sorted(accounts))} accounts only, which cannot serve "
            f"{models}, the model of {officers}. Sign it in to such an account (./install.sh providers login "
            f"{logins} from your computer, or sudo subspace-setup providers login {logins} on the VM) and run "
            "this again, or give those officers a model the signed-in accounts serve (subspace-setup "
            "--captain-model and --rear-admiral-model, or harness in fleet.json; installer README, 'Officer models').")


def reconcile_workspaces(cfg, homes, refresh_only=False):
    if not cfg["repositories"]:
        return None
    cache = account(REPOS, STATE / "repositories", 0o711)
    directory(STATE / "references", 0o711)
    rear = cfg["members"][0]["id"]
    references = STATE / "references" / rear
    directory(references, 0o2750, REPOS, f"ss-{rear}")
    path = ETC / "repository-sources.json"
    previous = read_json(path, private=True) if path.exists() else {}
    for name, repo in cfg["repositories"].items():
        if repo["private"]:
            require(not (cache / f"{name}.git").exists(), "Previously shared repositories need a new ID for private access.")
        require(name not in previous or previous[name] == repo, "Repository URL and visibility are immutable for an installed ID.")
    # Reserve the ID/visibility before any clones are written, including partial runs.
    write(path, json_text({**previous, **cfg["repositories"]}))
    prepare_workspaces(cfg["repositories"], cfg["members"], homes, cache, REPOS, references,
                       APP / "lib/github_app.py", refresh_only=refresh_only)
    return references


def maintain(action):
    with open("/run/lock/subspace-fleet.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        cfg = validate(read_json(ETC / "fleet.json", private=True), read_json(ETC / "secrets.json", private=True),
                       check_files=False)
        homes = {person["id"]: STATE / "agents" / person["id"] for person in cfg["members"]}
        settings = cfg["maintenance"]
        if action == "mirrors":
            reconcile_workspaces(cfg, homes, refresh_only=True)
        elif action == "tools":
            install_toolchains(cfg, homes, update=True)
        elif action == "prune":
            run("journalctl", "--namespace=subspace-fleet", "--rotate", f"--vacuum-time={settings['logDays']}d")
            prune_worktrees(cfg["repositories"], cfg["members"], homes, settings["worktreeDays"])
        else:
            raise Invalid("Unknown maintenance task.")


def render_irc(cfg, extra):
    local = cfg["irc"]["compose"]
    config_dir = ETC / "irc"
    directory(config_dir, 0o700)
    account_path = config_dir / "accounts.json"
    operator_path = config_dir / "operator-password"
    channel_path = config_dir / "channels.json"
    passwords = cfg["passwords"].copy()
    channels = set(local["channels"]) | set(cfg["irc"]["channels"])
    if medical := cfg["medical"]:
        passwords[medical["account"]] = extra[MEDICAL_IRC_PASSWORD[-1]]
        channels.add(medical["channel"])
    entries = [{"name": name, "password": password} for name, password in sorted(passwords.items())]
    write(account_path, json_text(entries), 0o440, ERGO_GID)
    write(operator_path, cfg["operatorPassword"] + "\n", 0o440, ERGO_GID)
    write(channel_path, json_text([{"name": name} for name in sorted(channels)]), 0o440, ERGO_GID)
    environment = {
        "IRC_PLAIN_BIND": LOOPBACK, "IRC_PLAIN_PORT": cfg["irc"]["port"],
        "IRC_TLS_BIND": f"[{local['tlsBind']}]" if ":" in local["tlsBind"] else local["tlsBind"],
        "IRC_TLS_PORT": local["tlsPort"],
        "IRC_WS_BIND": LOOPBACK, "IRC_WS_PORT": 8097,
        "IRC_SERVER_NAME": local["serverName"],
    }
    write(config_dir / "compose.env", "".join(f"{key}={value}\n" for key, value in environment.items()))
    volumes = [
        f"{account_path}:/run/subspace/accounts.json:ro",
        f"{operator_path}:/run/subspace/operator-password:ro",
        f"{channel_path}:/run/subspace/channels.json:ro",
    ]
    service_env = {
        "IRC_ACCOUNTS_FILE": "/run/subspace/accounts.json",
        "IRC_CHANNELS_FILE": "/run/subspace/channels.json",
        "IRC_OPER_NAME": local["operator"],
        "IRC_OPER_PASSWORD_FILE": "/run/subspace/operator-password",
    }
    for setting, var, filename in (("tlsCert", "IRC_TLS_CERT", "fullchain.pem"), ("tlsKey", "IRC_TLS_KEY", "privkey.pem")):
        if setting in local:
            location = config_dir / filename
            write(location, Path(local[setting]).read_text(), 0o440, ERGO_GID)
            volumes.append(f"{location}:/run/subspace/{filename}:ro")
            service_env[var] = f"/run/subspace/{filename}"
    history = local["history"]
    service = {"environment": service_env, "volumes": volumes}
    override = {"services": {"irc": service}}
    service_env["IRC_HISTORY_DATABASE"] = history["database"]
    history_path = config_dir / "history-password"
    if history["database"] == "postgresql":
        write(history_path, cfg["historyPassword"], 0o440, ERGO_GID)
        volumes.append(f"{history_path}:/run/subspace/history-password:ro")
        service_env.update({
            "IRC_POSTGRES_HOST": history["host"], "IRC_POSTGRES_PORT": history["port"],
            "IRC_POSTGRES_USER": history["user"], "IRC_POSTGRES_DATABASE": history["name"],
            "IRC_POSTGRES_SSL_MODE": history["sslMode"], "IRC_POSTGRES_PASSWORD_FILE": "/run/subspace/history-password",
        })
        if "network" in history:
            service["networks"] = ["default", history["network"]]
            override["networks"] = {history["network"]: {"external": True}}
    else:
        history_path.unlink(missing_ok=True)
    write(config_dir / "override.json", json_text(override))
    write(config_dir / "source", str(APP / "src" / f"irc-{local['ref']}" / "subspace-irc") + "\n")
    unit = f"{PREFIX}irc.service"
    write(SYSTEMD / unit, f"""[Unit]
Description=Subspace Ergo IRC
Requires=docker.service
After=docker.service
[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart={APP}/lib/runtime.py irc up
ExecStop={APP}/lib/runtime.py irc stop
TimeoutStartSec=600
[Install]
WantedBy=multi-user.target
""", 0o644)
    return unit


def medical_units(cfg, credentials, after):
    path = ETC / "medical.json"
    medical = cfg["medical"]
    if medical is None:
        path.unlink(missing_ok=True)
        return []
    irc = {key: cfg["irc"][key] for key in ("host", "port", "tls")}
    if "caFile" in cfg["irc"]:
        irc["caFile"] = str(ETC / "irc-ca.pem")
    irc.update(account=medical["account"], channel=medical["channel"], password=credentials[MEDICAL_IRC_PASSWORD[-1]])
    settings = {key: medical[key] for key in ("baseUrl", "permalinkHost", "organisation", "projects", "environment", "http5xxTag")}
    settings.update(irc=irc, token=credentials[SENTRY_TOKEN[-1]],
                    target=medical["target"]["account"] if medical["target"] else None)
    write(path, json_text(settings))
    home = account(MEDICAL, STATE / "medical")
    service = serve_unit(
        "medical", "Poll Sentry for fleet errors",
        f"{APP}/lib/medical.py %d/{path.name} {home}/receipts.sqlite3",
        MEDICAL, after, user_paths=[home], oneshot=True, toolchain=False, credential=path,
        service_options=["TimeoutStartSec=5min", "SystemCallFilter=@system-service",
                         "SystemCallArchitectures=native", "RestrictNamespaces=yes",
                         "MemoryDenyWriteExecute=yes", "PrivateIPC=yes"],
    )
    timer = f"{PREFIX}medical.timer"
    write(SYSTEMD / timer, "\n".join([
        "[Unit]", "Description=Poll Sentry for the Subspace medical ship", "",
        "[Timer]", "OnBootSec=1min", f"OnUnitInactiveSec={medical['intervalSeconds']}s",
        "AccuracySec=1s", "", "[Install]", "WantedBy=timers.target", "",
    ]), 0o644)
    return [service, timer]


def allocate_ports(cfg):
    path = STATE / "engine-ports.json"
    ports = read_json(path) if path.exists() else {}
    unavailable = set(ports.values()) | set(cfg["listeners"])
    for person in cfg["members"]:
        if person["harness"]["type"] != "opencode":
            continue
        if person["id"] not in ports:
            number = next((candidate for candidate in range(4096, 65536) if candidate not in unavailable), None)
            require(number is not None, "No port remains for an OpenCode engine.")
            ports[person["id"]] = number
            unavailable.add(number)
        require(ports[person["id"]] not in cfg["listeners"], "A service listener conflicts with an allocated OpenCode port.")
    write(path, json_text(ports))
    return ports


def stakeholder_settings(cfg, tokens, backend, user, data, environment):
    integrations = cfg["integrations"]
    settings, writable = {}, []
    if slack := integrations["slack"]:
        environment.update(SLACK_APP_TOKEN=tokens["slack-app-token"], SLACK_BOT_TOKEN=tokens["slack-bot-token"])
        settings["bots"] = [{
            "id": "fleet-stakeholder-slack", "backend": backend,
            "slack": {"appId": slack["appId"], "appTokenEnv": "SLACK_APP_TOKEN", "botTokenEnv": "SLACK_BOT_TOKEN"},
            "access": {"users": slack["users"], "channels": slack["channels"], "mentionOnly": True},
        }]
    if teams := integrations["teams"]:
        socket_dir = STATE / "teams"
        directory(socket_dir, 0o2750, user, teams.get("socketGroup", user))
        writable.append(socket_dir)
        settings["teams"] = [{
            "id": "fleet-stakeholder-teams", "backend": backend,
            "tenantId": teams["tenantId"], "botClientId": teams["botClientId"], "socketPath": str(socket_dir / "bot.sock"),
            "access": {"users": teams["users"], "channels": teams.get("channels", [])},
        }]
    usage = integrations["usage"]
    if "reportChannel" in usage:
        settings["quotaWatch"] = {
            "proxy": quota_proxy(cfg, data.parent),
            "stateFile": str(data / "quota.json"), "providers": usage["providers"],
            # The rear admiral posts as its stakeholder bot, or as its own Slack bot when it has that instead.
            "report": {"bot": "fleet-stakeholder-slack" if slack else slack_bot_id(cfg["members"][0]["id"]),
                       "channel": usage["reportChannel"]},
        }
    return settings, writable


def agent_dispatch_settings(cfg):
    """Stargate's agentDispatch block for the rear admiral's gateway, and the environment it reads.

    The block names only the token's variable; the token goes in the gateway's private runtime settings.
    """
    dispatch = cfg["agentDispatch"]
    if not dispatch:
        return None, {}
    rear, captains = cfg["members"][0], cfg["members"][1:]
    # The dispatcher finds captains in the API's fleet roster by agent name; an unlisted name is its own IRC
    # account. Register each captain under its member ID or its name, and both map to its account here.
    accounts = {}
    for person in captains:
        for name in (person["id"], person["name"]):
            if re.fullmatch(r"[A-Za-z0-9-]{1,64}", name) and name != person["account"]:
                accounts[name] = person["account"]
    return {
        "enabled": True, "fleet": cfg["slug"],
        "api": {"baseUrl": cfg["api"]["url"], "tokenEnv": DISPATCH_TOKEN_ENV},
        "messaging": {"type": "irc", "registration": rear["id"], "channel": cfg["irc"]["commandChannel"]},
        "captainAccounts": accounts,
        **{key: dispatch[key] for key in DISPATCH_DEFAULTS},
    }, {DISPATCH_TOKEN_ENV: dispatch["token"]}


def render(cfg, extra, prune=False):
    check_crew_ids(cfg)
    slug_path = STATE / "slug"
    if slug_path.exists():
        require(slug_path.read_text().strip() == cfg["slug"], "Fleet slug is immutable on this VM.")
    write(slug_path, cfg["slug"] + "\n")
    roots = captain_roots(cfg)
    pins = captain_pins(cfg, roots)
    rear_session = rear_pin(cfg)
    rear_directory = rear_workspace(cfg)
    withdraw_ready(pins)
    # Record each captain's root before its checkouts are cloned, so a later run keeps it (captain_roots).
    write(ETC / "session-roots.json",
          json_text({**recorded_roots(), **{agent_id: root["mode"] for agent_id, root in roots.items()}}))
    tokens = {**extra["tokens"], **extra.get("slack", {}).get(cfg["members"][0]["id"], {})}
    guard_enabled = cfg["usageGuard"]["enabled"]
    if not guard_enabled and "reportChannel" not in cfg["integrations"]["usage"]:
        tokens.pop("proxy-management-key", None)
    units = [render_proxy(cfg, tokens)]
    # Every officer's memory and task limits, sized from this VM's memory and the fleet (lib/memory.py).
    limits = memory_plan(cfg)["limits"]
    view_home = account(VIEW, STATE / "view")
    for path in (STATE / "agents", STATE / "sessions"):
        directory(path, 0o711)
    seed_path = STATE / "engine.seed"
    if not seed_path.exists():
        write(seed_path, secrets.token_hex(32) + "\n")
    seed = bytes.fromhex(seed_path.read_text().strip())
    ports = allocate_ports(cfg)

    gateway_home = account(GATEWAY, STATE / "gateway")
    sockets = []
    if enrolled(cfg):
        # A hosted API: the gateway user keeps the fleet's enrolment token and, through Stargate, each officer's
        # key and signing-proxy socket.
        require_subspace_agents(cfg)
        own_file(gateway_home / ENROLMENT_TOKEN, extra["tokens"][ENROLMENT_TOKEN] + "\n", GATEWAY)
        sockets = agent_socket_directories(cfg)
        write(SUBSPACE_REQUEST, subspace_request_script(cfg), 0o755)
    elif AGENT_TMPFILES.exists():
        AGENT_TMPFILES.unlink()
    if not enrolled(cfg):
        SUBSPACE_REQUEST.unlink(missing_ok=True)
    if "caFile" in cfg["irc"]:
        write(ETC / "irc-ca.pem", Path(cfg["irc"]["caFile"]).read_text(), 0o644)
    homes = {}
    documents = {}
    for person in cfg["members"]:
        agent_id = person["id"]
        user = f"ss-{agent_id}"
        home = homes[agent_id] = account(user, STATE / "agents" / agent_id)
        run("mkdir", "-p", "--", home / "workspace", user=user)
        configure_git(person, user, prune, cfg["protectedBranches"])
        references = STATE / "references" / agent_id if cfg["repositories"] else None
        documents[agent_id] = role_documents(cfg, person, references, home)
        if enrolled(cfg):
            add_request_guidance(documents[agent_id], [SUBSPACE_REQUEST.name])
        subprocess.run(
            ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "role"],
            input=json_text(documents[agent_id]), text=True, check=True,
        )
        if person["role"] == "captain":
            # The workspace's own AGENTS.md and CLAUDE.md, written as the captain beside its checkouts, never in one.
            subprocess.run(
                ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "workspace-instructions"],
                input=json_text(workspace_documents(cfg, person, roots[agent_id]["path"], home)), text=True,
                check=True,
            )
        files = extra["cloud"].get(agent_id, {})
        if files or (home / ".config/subspace/cloud/.installer-files").is_file():
            subprocess.run(
                ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "cloud", str(prune).lower()],
                input=json_text(files), text=True, check=True,
            )
    reconcile_workspaces(cfg, homes)
    databases, engines = [], []
    registrations, environments = {}, {}
    for person in cfg["members"]:
        agent_id = person["id"]
        home = homes[agent_id]
        harness = person["harness"]
        if person["role"] == "captain":
            # The captain's session root: its workspace above the checkouts, or where it was installed or pinned.
            workspace = roots[agent_id]["path"]
        else:
            # The rear admiral's reference checkout of the first repository, or the workspace its root is pinned in.
            workspace = rear_directory
        if harness["type"] == "opencode":
            pin = rear_session if person["role"] == "rear_admiral" else pins.get(agent_id)
            unit, backend, headers, database = render_engine(
                cfg, person, home, workspace, ports[agent_id], seed, documents[agent_id], pin,
                answers_offers=bool(fleet_control(cfg, person, pin, rear_session, gateway_home)),
                limits=limits[agent_id])
            databases.append(database)
            units.append(unit)
            engines.append(unit)
        else:
            backend = claude_backend(cfg, person, workspace, documents[agent_id])
            headers = {"CLIPROXYAPI_API_KEY": cfg["proxy"]["key"]}
        registrations[agent_id], password = irc_agent(cfg, person, backend)
        environments[agent_id] = {**headers, **password}
    after = [f"{PREFIX}proxy.service", *engines]
    medical_after = []
    if cfg["irc"]["mode"] == "local":
        local_unit = render_irc(cfg, extra.get("medical", {}))
        units.append(local_unit)
        after.append(local_unit)
        medical_after.append(local_unit)
    units.extend(medical_units(cfg, extra.get("medical", {}), medical_after))
    if cfg["irc"]["auth"] == "certificate":
        irc_certificates(cfg)
    groups = {}
    rear = cfg["members"][0]
    default_group = "gateway" if rear["harness"]["type"] == "opencode" else "gateway-opencode"
    for person in cfg["members"]:
        agent_id = person["id"]
        if person["crew"] and not pins.get(agent_id):
            # Joins the shared gateway once its root session is pinned (runtime.py commission), so the
            # gateway never serves it unpinned and keeps serving the rest of the fleet meanwhile.
            continue
        native = person["harness"]["type"] != "opencode"
        group = ("gateway" if person is rear else f"gateway-agent-{agent_id}") if native else default_group
        user = f"ss-{agent_id}" if native else GATEWAY
        home = homes[agent_id] if native else gateway_home
        opts = groups.setdefault(group, {"user": user, "home": home, "irc": [], "environment": {}, "ready": [],
                                         "writable": [], "native": native})
        if native:
            # A Claude Agent SDK officer's gateway runs its tools, as its own user, as an engine does.
            opts["environment"].update(officer_tool_environment(cfg, person, home))
        # A Claude Agent SDK officer's own gateway runs its tools, so it takes the officer's limits, as an engine
        # does; the shared gateway has no cap (memory.SHARED_OPTIONS).
        opts["service"] = memory.unit_options(limits[agent_id]) if native else memory.SHARED_OPTIONS
        entry = irc_officer(cfg, person, registrations[agent_id])
        check_sign_in(entry, entry["server"])
        if control := fleet_control(cfg, person, pins.get(agent_id), rear_session, gateway_home):
            # This captain answers dispatch offers with Stargate's fleet plugin. The socket directory belongs to the
            # gateway and admits only this captain's engine group; the evidence ledger is the gateway's alone.
            directory(STATE / "fleet-control", 0o711)
            directory(fleet_socket(agent_id).parent, 0o2750, GATEWAY, f"ss-{agent_id}")
            directory(gateway_home / "fleet-control", 0o2770, GATEWAY)
            entry.update(fleetControl=control, fleetChannels={"home": person["channel"],
                                                              "command": cfg["irc"]["commandChannel"]})
            opts["writable"].append(fleet_socket(agent_id).parent)
        opts["irc"].append(entry)
        opts["environment"].update(environments[agent_id])
        # The officer's own Slack bot joins the gateway that serves its IRC registration, on the same backend.
        bot, slack_environment = officer_slack_bot(person, registrations[agent_id], extra.get("slack", {}).get(agent_id))
        if bot:
            opts.setdefault("bots", []).append(bot)
            opts["environment"].update(slack_environment)
        if person["crew"]:
            opts["ready"].append(ready_marker(agent_id))

    checks = []
    for group, options in groups.items():
        user, home, environment = options["user"], options["home"], options["environment"]
        data = home / ("gateway" if user != GATEWAY else "data")
        is_primary = group == "gateway"
        settings = {"dataDirectory": str(data), "dashboard": is_primary, "irc": options["irc"]}
        writable = [home, *options["writable"]]
        if enrolled(cfg) and user == GATEWAY:
            # The gateway user's process holds every officer's key and opens each officer's socket.
            settings["subspaceAgents"] = subspace_agents_settings(cfg, gateway_home)
            writable.extend(sockets)
        if is_primary:
            # Slack and Teams keep their own sessions; a pinned root has one chat owner, the rear admiral's IRC.
            stakeholder_backend = {key: value for key, value in registrations[rear["id"]]["backend"].items()
                                   if key != "sessionID"}
            integrations, paths = stakeholder_settings(cfg, tokens, stakeholder_backend, user, data, environment)
            settings.update(integrations)
            writable.extend(paths)
            if guard_enabled or "quotaWatch" in settings:
                subprocess.run(
                    ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "gateway-key"],
                    input=tokens["proxy-management-key"], text=True, check=True,
                )
            if guard_enabled:
                directory(usage_guard_file().parent, 0o755, user)
                writable.append(usage_guard_file().parent)
                settings["usageGuard"] = usage_guard_settings(cfg, home)
            dispatch, dispatch_env = agent_dispatch_settings(cfg)
            if dispatch:
                settings["agentDispatch"] = dispatch
                environment.update(dispatch_env)
            dashboard_socket = data / "dashboard.sock"
            dashboard_user = user
            if cfg["stargate"]["api"]:
                settings["api"] = {
                    "listen": f"{LOOPBACK}:{cfg['stargate']['api']['port']}",
                    "allowedNetworks": ["127.0.0.0/8", "::1/128"], "fleetName": cfg["name"],
                    "corsOrigins": cfg["stargate"]["api"]["corsOrigins"],
                }
                if stargate_has_pull_request_repositories(cfg) and (
                        repositories := pull_request_repositories(cfg, homes)):
                    settings["api"]["pullRequestRepositories"] = repositories
        if options.get("bots"):
            # Officers' own Slack bots, before the rear admiral's stakeholder bot when it has one.
            settings["bots"] = [*options["bots"], *settings.get("bots", [])]
        name = "stargate" if is_primary else f"stargate-{group}"
        config_path = save_settings(name, settings, user)
        runtime_path = save_settings(group, {
            "environment": environment, "config": str(config_path),
            "entrypoint": str(APP / "src" / f"stargate-{cfg['stargate']['ref']}" / "src/main.ts"),
        }, user)
        checks.append([user, str(runtime_path)])
        units.append(serve_unit(
            group, "Stargate fleet gateway", f"{APP}/lib/runtime.py gateway {runtime_path} start", user, after,
            user_paths=writable, unit_options=[f"ConditionPathExists={path}" for path in options["ready"]],
            toolchain=not options["native"], service_options=options["service"],
        ))
    write(ETC / "gateway-checks.json", json_text(checks))
    dash = cfg["dashboard"]
    bind = dash["bind"]
    units.append(serve_unit("dashboard", "Stargate dashboard", f"{APP}/lib/dashboard.py {bind} {dash['port']} {dashboard_socket}", dashboard_user, [f"{PREFIX}gateway.service"]))
    units.append(render_view(cfg, databases, view_home))
    units.insert(0, render_firewall(cfg))
    write(ETC / "toolchains.toml", toolchains.mise_config(cfg["toolchains"]), 0o644)
    install_toolchains(cfg, homes)
    install_screenshots(cfg)
    units.extend(maintenance_units(cfg))

    path = ETC / "units"
    retired_path = ETC / "retired-units"
    previous = set(path.read_text().splitlines()) if path.exists() else set()
    retired = set(retired_path.read_text().splitlines()) if retired_path.exists() else set()
    removed = (retired | previous) - set(units)
    if removed:
        write(retired_path, "".join(f"{unit}\n" for unit in sorted(removed)))
    else:
        retired_path.unlink(missing_ok=True)
    write(path, "".join(f"{unit}\n" for unit in units))
    publish_ready(pins)


def commission(prune, existing=False):
    """Pin every crew-hosting captain's root session on its running engine; re-render once if any is new.

    With existing, only reconcile already pinned roots on engines that answer now, so a changed policy
    reaches the session before the gateway restarts with it. That pass is best effort and renders nothing.
    """
    cfg = validate(read_json(ETC / "fleet.json", private=True), read_json(ETC / "secrets.json", private=True))
    pinned, failed = [], []
    for person in cfg["members"]:
        # Captains with crew, and the rear admiral while agent dispatch is on (captains name its root).
        rear = person["role"] == "rear_admiral"
        if not (person["crew"] or rear and rear_root(cfg)):
            continue
        store = sessions.SessionRecord(SESSIONS, person["id"])
        try:
            if existing and not (store.load() or {}).get("confirmed"):
                continue
            settings = read_json(ETC / f"engine-{person['id']}.json")
            harness = person["harness"]
            client = sessions.OpenCode(f"http://{LOOPBACK}:{settings['port']}", settings["environment"]["OPENCODE_PASSWORD"])
            expected = {
                "directory": settings["directory"], "title": f"{cfg['slug']}/{person['id']}",
                "crew": [worker["id"] for worker in person["crew"]],
                "model": session_model(harness),
                "permissions": harness_permissions(cfg, person) if rear else captain_permissions(cfg, person),
            }
            if person["crew"]:
                # Stargate's crew state names the first repository's checkout as the default project, whether
                # the root works there or in the workspace above it, so the record keeps it fixed too.
                expected["project"] = str(STATE / "agents" / person["id"] / "workspace" / next(iter(cfg["repositories"])))
            if existing:
                _, new = sessions.ensure(client, store, expected, wait=lambda c, i: sessions.wait_until_ready(c, i, 2),
                                         notice=lambda _text: None)
            else:
                _, new = sessions.ensure(client, store, expected)
        except (Invalid, sessions.SessionError) as error:
            if not existing:
                # Keep going: one captain's failure must not hold back the others' pins.
                failed.append(f"{'Rear admiral' if rear else 'Captain'} {person['id']}: {error}")
            continue
        if new:
            pinned.append(person["id"])
    if pinned:
        try:
            render(cfg, read_json(ETC / "extra-secrets.json", private=True), prune)
        except BaseException:
            # A failed re-render restarts nothing: running services keep the configuration they loaded, and
            # a captain's readiness marker is only written once a render completes.
            (STATE / "pending").unlink(missing_ok=True)
            raise
        print(f"Pinned the root session of {', '.join(pinned)}. Fleet services restart to load the new pins.")
    require(not failed, " ".join(failed) + " The gateway keeps serving the rest of the fleet without "
            "unpinned captains. Fix the cause and run the installer again.")


def engine(path, attach=False):
    settings = read_json(path)
    environment = settings["environment"]
    if "MISE_DATA_DIR" not in environment:
        # Settings rendered before officers had a tool environment of their own.
        environment = {**TOOL_ENV, "PATH": FLEET_PATH, **environment}
    env = {**os.environ, **environment}
    if attach:
        # The TUI only talks to the engine, which runs the tools; the private TMPDIR is the engine's.
        env.pop("TMPDIR", None)
    elif "TMPDIR" in env:
        toolchains.private_tmp(env["TMPDIR"])
    os.umask(0o027)
    os.chdir(settings["directory"])
    binary = str(APP / "bin/opencode")
    port = str(settings["port"])
    if attach:
        os.execvpe("tmux", ["tmux", "new-session", "-A", "-s", settings["slug"],
                            binary, "--server", f"http://{LOOPBACK}:{port}"], env)
    os.execve(binary, [binary, "serve", "--hostname", LOOPBACK, "--port", port], env)


def share_database(path):
    settings = read_json(path)
    database = Path(settings["environment"]["OPENCODE_DB"])
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((LOOPBACK, settings["port"]), timeout=1):
                if database.is_file():
                    break
        except OSError:
            pass
        time.sleep(0.2)
    else:
        raise Invalid("OpenCode did not initialise its session database.")

    for suffix in ("", "-wal", "-shm"):
        file = database.with_name(database.name + suffix)
        try:
            fd = os.open(file, os.O_RDONLY | os.O_NOFOLLOW)
        except FileNotFoundError:
            continue
        with os.fdopen(fd, "rb") as stream:
            os.fchmod(stream.fileno(), 0o640)


def view_config(source, destination):
    import tomllib

    source, destination = Path(source), Path(destination)
    desired = source.read_text()
    previous = {}
    if destination.exists() and not destination.is_symlink():
        previous = tomllib.loads(destination.read_text())
        if previous.get("agents", {}).get("opencode", {}) == tomllib.loads(desired)["agents"]["opencode"]:
            return
    stable = "".join(f"{key} = {json.dumps(previous[key])}\n"
                     for key in ("cursor_secret", "auth_token") if isinstance(previous.get(key), str))
    with tempfile.NamedTemporaryFile(mode="w", dir=destination.parent, delete=False) as stream:
        temp = Path(stream.name)
        try:
            stream.write(stable + desired)
            stream.close()
            os.replace(temp, destination)
        finally:
            temp.unlink(missing_ok=True)


def gateway(path, verb):
    settings = read_json(path)
    env = {**os.environ, **settings["environment"]}
    if "TMPDIR" in settings["environment"]:
        # A Claude Agent SDK officer's gateway: its private TMPDIR is made in the service's own /tmp.
        if verb[:1] == ["start"]:
            toolchains.private_tmp(env["TMPDIR"])
        else:
            env.pop("TMPDIR")
    bun = str(APP / "bin/bun")
    args = [bun, settings["entrypoint"], *verb, "--config", settings["config"]]
    os.execve(bun, args, env)


def gateway_key():
    key = sys.stdin.buffer.read(257)
    write_private(Path.home() / "quota-management-key", key)


def compose(args):
    directory = ETC / "irc"
    source = Path((directory / "source").read_text().strip())
    opts = [
        "docker", "compose", "--project-name", "subspace-fleet-irc", "--project-directory", str(source),
        "--env-file", str(directory / "compose.env"), "-f", str(source / "compose.yaml"),
        "-f", str(directory / "override.json"),
    ]
    if args == ["up"]:
        args = ["up", "--detach", "--wait", "--wait-timeout", "240", "--build"]
    env = {key: value for key, value in os.environ.items() if not key.startswith(("IRC_", "COMPOSE_"))}
    os.execvpe("docker", [*opts, *args], env)


def main(args):
    verb, *tail = args
    if verb == "usage-guard-summary":
        config = read_json(ETC / "fleet.json", private=True)
        configured = validate(config, read_json(ETC / "secrets.json", private=True), check_files=False)
        print(usage_guard_line(configured["usageGuard"], configured["members"][0]))
        return
    # install.sh --dry-run --no-systemd: the plan says what a run that leaves systemd and swap alone would do.
    managed = not (verb == "plan" and tail[-1:] == ["no-systemd"])
    if not managed:
        tail = tail[:-1]
    if verb in {"prepare", "plan", "render"}:
        config, private = read_json(tail[0]), read_json(tail[1], private=True)
        cache = ETC / "extra-secrets.json"
        cfg = validate(config, private)
        slug_path = STATE / "slug"
        if slug_path.exists() and os.access(slug_path, os.R_OK):
            require(slug_path.read_text().strip() == cfg["slug"], "Fleet slug is immutable on this VM.")
        if os.access(SESSIONS, os.R_OK | os.X_OK):
            # Refuse a changed crew roster or first repository before anything is installed.
            captain_pins(cfg)
            rear_pin(cfg)
        # A new captain's crew are named; the installed fleet says which captains are new. A dry run that cannot
        # read it leaves the check to the real run.
        installed = ETC / "fleet.json"
        if not installed.exists() or os.access(installed, os.R_OK):
            check_new_crew_named(cfg, read_json(installed, private=True) if installed.exists() else {})
        prune = tail[-1] == "true"
        require_usage_guard_source(cfg, installed_only=verb == "render")
        if verb == "render":
            # Say once per install which captains keep a root other than the workspace, and why.
            for root in captain_roots(cfg).values():
                if root["notice"]:
                    print(root["notice"])
            render(cfg, read_json(cache, private=True), prune)
            return
        directory = Path(tail[-2]) if tail[-2] else None
        saved = ETC / "fleet.json"
        saved_config = read_json(saved, private=True) if os.access(saved, os.R_OK) else {}
        previous = read_json(cache, private=True) if os.access(cache, os.R_OK) else {}
        if saved_config and directory is None:
            print("subspace-fleet: no secrets directory supplied. Keeping installed credentials.", file=sys.stderr)
        saved_people = []
        if saved_config and not prune:
            saved_people = [saved_config["rearAdmiral"], *saved_config["captains"]]
        if saved_config and previous and "slack" not in previous:
            tokens = previous["tokens"]
            previous["slack"] = {saved_config["rearAdmiral"]["id"]: {
                name: tokens.pop(name) for name in ("slack-app-token", "slack-bot-token") if name in tokens
            }}
        keys = app_keys(cfg["members"], directory, saved_people)
        extra = extra_snapshot(cfg, directory, previous, prune)
        bundles = irc_bundles(cfg, directory)
        for person in cfg["members"]:
            check_sign_in(irc_agent(cfg, person, {})[0], cfg["irc"])
        # The real run would stop before it clones anything, so the dry run stops too, saying how to go on.
        unreadable = unreadable_repositories(cfg)
        require(not unreadable, unreadable_message(unreadable))
        # Officers whose model no signed-in account serves would stay silent, so both runs stop.
        accounts = proxy_accounts()
        unserved = unserved_officers(cfg, accounts)
        require(not unserved, unserved_message(unserved, accounts))
        if verb == "prepare":
            stage = Path(tail[2])
            (stage / "fleet.json").write_text(json_text(config))
            (stage / "secrets.json").write_text(json_text(private))
            (stage / "extra-secrets.json").write_text(json_text(extra))
            for member_id, key in keys.items():
                directory = stage / "github-apps" / member_id
                directory.mkdir(parents=True)
                (directory / "github-app.pem").write_text(key)
            # fleet.json's systemPackages, validated as Debian package names, which install.sh installs with apt.
            (stage / "system-packages").write_text("".join(f"{name}\n" for name in cfg["systemPackages"]))
            # The base screenshot tools' packages for this distribution, unless screenshots are off.
            wanted = screenshots.packages(screenshots.pins(), screenshots.release()) if cfg["screenshots"] else []
            (stage / "screenshot-packages").write_text("".join(f"{name}\n" for name in wanted))
            sources = [cfg["stargate"]["repo"], cfg["stargate"]["ref"], cfg["irc"]["mode"]]
            if cfg["irc"]["mode"] == "local":
                sources.extend(cfg["irc"]["compose"][key] for key in ("repo", "ref"))
            (stage / "sources").write_text("\n".join(sources) + "\n")
            # Supplied IRC certificate bundles: install.sh puts them in IRC_CERTS, where render gives each to its gateway.
            for member_id, bundle in bundles.items():
                (stage / "irc-certs").mkdir(mode=0o700, exist_ok=True)
                (stage / "irc-certs" / f"{member_id}.pem").write_text(bundle)
                installed = read_irc_bundle(irc_client_pem(member_id)) if os.access(IRC_CERTS, os.X_OK) else None
                if installed and installed != bundle:
                    print(f"Replacing {member_id}'s IRC certificate {irc_fingerprint(installed, member_id)} with the "
                          f"supplied {irc_fingerprint(bundle, member_id)}. Bind the new fingerprint on the IRC server.")
        else:
            count = sum(len(person["crew"]) for person in cfg["members"])
            print(f"Fleet {cfg['slug']} has {len(cfg['members']) - 1} captains, {count} crew and one rear admiral. "
                  f"IRC mode is {cfg['irc']['mode']}. Crew run as their captain and have no IRC accounts.")
            print(officer_models_line(cfg))
            if cfg["usageGuard"]["enabled"] and saved_config and "usageGuard" not in saved_config:
                print("This upgrade enables the usage guard. Pass --no-usage-guard in guided setup to keep it off.")
            print(usage_guard_line(cfg["usageGuard"], cfg["members"][0]))
            for line in names_plan(cfg):
                print(line)
            if (APP / "src" / f"stargate-{cfg['stargate']['ref']}").is_dir():
                check_crew_ids(cfg)
            elif any(crew_id(worker["name"]) != worker["id"] for person in cfg["members"] for worker in person["crew"]):
                print(f"Stargate {cfg['stargate']['ref'][:12]} is not checked out here yet: the install checks, once "
                      "it is, that its crew plugin lets a crew member keep its installed ID under a new name.")
            print(f"Standard IRC channels: {' '.join(cfg['irc']['channels'])} (Starbase, fleet command, then one "
                  "ship channel per captain)." + ("" if cfg["irc"]["mode"] == "local" else
                                                   " On a remote server the officers create them by joining, so they "
                                                   "need no registration; registering them with ChanServ is optional "
                                                   "and keeps their topic and modes when they empty. Only a server "
                                                   "that lets operators alone create channels needs them created "
                                                   "first."))
            if dispatch := cfg["agentDispatch"]:
                print(f"Agent dispatch is on: the rear admiral checks {cfg['api']['url']} every "
                      f"{dispatch['pollIntervalSeconds']} seconds as its own API agent and offers issues in "
                      f"{cfg['irc']['commandChannel']}.")
            else:
                print("Agent dispatch is off.")
            if enrolled(cfg):
                print(f"The Subspace API is hosted at {cfg['api']['origin']}: each officer enrols its own key through "
                      "Stargate with the fleet's enrolment token and reaches the API through its signing proxy. This "
                      "VM keeps no administrator token and registers nothing; the API's administrators keep the "
                      "fleet's records.")
                if (APP / "src" / f"stargate-{cfg['stargate']['ref']}").is_dir():
                    require_subspace_agents(cfg)
                else:
                    print(f"The install stops unless Stargate {cfg['stargate']['ref'][:12]} has its subspaceAgents "
                          f"section ({SUBSPACE_AGENTS_FILE}, from commit {SUBSPACE_AGENTS_COMMIT[:12]} on).")
            elif cfg["api"]:
                address = fleet_records.fleet_api_address(cfg)
                print(f"The Subspace API will record the fleet's Stargate Fleet API at {address}." if address else
                      "Stargate's Fleet API is off, so the Subspace API will record no Fleet API address.")
            if not cfg["repositories"] and len(cfg["members"]) > 1:
                print("No repositories are configured, so captains have no crew: crew work in Git worktrees of "
                      "their captain's checkouts.")
            for line in session_root_lines(cfg):
                print(line)
            for line in slack_plan(cfg):
                print(line)
            if line := captain_tools_line(cfg):
                print(line)
            for line in policy_lines(cfg):
                print(line)
            for line in toolchain_plan(cfg):
                print(line)
            for line in screenshots.plan_lines(cfg["screenshots"], screenshots.pins(), screenshots.release()):
                print(line)
            for line in memory_lines(cfg, managed=managed):
                print(line)
            print("\n".join(irc_sign_in(cfg, bundles, installed=False)))
    elif verb == "commission":
        commission(tail[0] == "true", existing="existing" in tail[1:])
    elif verb == "engine":
        engine(tail[0], attach="attach" in tail[1:])
    elif verb == "gateway":
        gateway(tail[0], tail[1:])
    elif verb == "maintenance":
        maintain(tail[0])
    elif verb == "verify-toolchains":
        verify_toolchains()
    elif verb == "check-screenshots":
        check_screenshots()
    elif verb == "swap":
        # Make sure the VM has swap (lib/memory.py). Never fails the install: a problem is reported.
        cfg = validate(read_json(ETC / "fleet.json", private=True), read_json(ETC / "secrets.json", private=True),
                       check_files=False)
        for line in memory.apply_swap(cfg["memory"]["swap"], memory.swap_state(), memory.total_memory()):
            print(line)
    elif verb == "memory-summary":
        # The install summary: each engine's limits and why, limits set by hand, and the VM's swap.
        cfg = validate(read_json(ETC / "fleet.json", private=True), read_json(ETC / "secrets.json", private=True),
                       check_files=False)
        for line in memory_lines(cfg, installed=True, managed="no-systemd" not in tail):
            print(line)
    elif verb == "share-database":
        share_database(tail[0])
    elif verb == "view-config":
        view_config(*tail)
    elif verb == "check":
        for user, path in read_json(ETC / "gateway-checks.json", private=True):
            run(APP / "lib/runtime.py", "gateway", path, "config", "check", user=user)
    elif verb == "cloud":
        provision_cloud(json.load(sys.stdin), tail == ["true"])
    elif verb == "role":
        provision_role(json.load(sys.stdin))
    elif verb == "workspace-instructions":
        # Run as the captain: its workspace AGENTS.md and CLAUDE.md, keeping the operator notes.
        provision_workspace(json.load(sys.stdin))
    elif verb == "agent-header":
        # Stargate's Claude backend runs this as the officer for its Subspace MCP server's headers.
        require(len(tail) == 1 and re.fullmatch(r"[a-z][a-z0-9-]{0,23}", tail[0]), "Name one agent account.")
        print(json.dumps({AGENT_HEADER: tail[0]}))
    elif verb == "prune-app-keys":
        cfg = validate(read_json(ETC / "fleet.json"), read_json(ETC / "secrets.json", private=True))
        for person in cfg["members"]:
            if "githubApp" not in person:
                (ETC / "github-apps" / person["id"] / "github-app.pem").unlink(missing_ok=True)
    elif verb == "gateway-key":
        gateway_key()
    elif verb == "irc":
        compose(tail)
    elif verb == "irc-fingerprints":
        # The install summary: each officer's certificate fingerprint for the IRC operator to bind.
        cfg = validate(read_json(ETC / "fleet.json", private=True), read_json(ETC / "secrets.json", private=True))
        if cfg["irc"]["auth"] == "certificate":
            print("\n".join(irc_sign_in(cfg)))
    else:
        raise Invalid("Unknown runtime action.")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (Invalid, sessions.SessionError) as exc:
        sys.exit(f"subspace-fleet: {exc}")
    except (OSError, ValueError, subprocess.CalledProcessError):
        sys.exit("subspace-fleet: runtime command or configuration failed.")
