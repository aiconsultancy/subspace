#!/usr/bin/python3
"""Validate fleet inputs and converge the system configuration."""

import base64
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
import stat
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit

from agent_files import collect_cloud, private_directory, private_file, provision_cloud, write_private
from network import rules
from roles import default_permissions, provision_role, role_documents
import sessions
import standard_channels
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
TOOL_HOME = APP / "toolchains"
TOOL_ENV = {"MISE_DATA_DIR": str(TOOL_HOME), "MISE_GLOBAL_CONFIG_FILE": str(ETC / "toolchains.toml"),
            "MISE_SYSTEM_CONFIG_DIR": str(TOOL_HOME / "system-config")}
FLEET_PATH = f"{APP}/bin:{TOOL_HOME}/shims:/usr/local/bin:/usr/bin:/bin"
STARGATE_REPO = "https://github.com/aiconsultancy/stargate.git"
# Stargate v0.43.0, the first release with agent dispatch on the rear admiral's gateway.
STARGATE_REF = "fcf6ac75d9c780878c06c8b49fb160e4a27832ff"
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
# OpenCode provider IDs per model family, as in Stargate's OpenCode template.
PROVIDERS = {"claude": "claude-proxy", "codex": "openai-proxy"}
MODEL = r"[a-zA-Z0-9][a-zA-Z0-9._/:#-]{0,127}"
SESSIONS = ETC / "sessions"
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


class Invalid(ValueError):
    pass


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


def not_standard(slug, label, value, expected):
    """Refusal for a renamed standard channel: every fleet keeps the standard names."""
    shown = value if isinstance(value, str) and len(value) <= 64 and value.isprintable() else "another name"
    return (f"{label} is {shown}, but it must be {expected}. Every fleet has the standard channels "
            f"{standard_channels.summary(slug)}, and the desktop recognises them by name, so they cannot be "
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
    value = fields(value, "fleet member", "id name account channel git githubApp harness crew crewNames", ["id", "name", "account"])
    pattern(value["id"], r"[a-z][a-z0-9-]{0,19}", "member ID")
    require(not value["id"].startswith("fleet-"), "Member IDs cannot start with fleet-.")
    require(value["id"] != SENTRY_TOKEN[0], "The medical member ID is reserved for integration credentials.")
    require(value["id"] not in {"all", "captain"}, "The member IDs all and captain are reserved for crew messaging.")
    string(value["name"], "member name")
    require(isinstance(value["account"], str) and re.fullmatch(ACCOUNT, value["account"]),
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
    return value


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


def listener(value, label, default_port, allow_bind=False):
    value = fields(value, label, "port bind" if allow_bind else "port")
    return {"port": port(value.get("port", default_port)), "bind": ip(value.get("bind", LOOPBACK))}


def irc_config(value, slug, check_files):
    value = fields(value, "irc", "mode host port tls caFile allowedAccounts commandChannel starbaseChannel advertise compose", ["mode", "allowedAccounts"])
    mode = pattern(value["mode"], r"local|remote", "IRC mode")
    tls = boolean(value.get("tls", mode == "remote"))
    require(mode == "local" or tls, "Remote IRC requires TLS.")
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
    for key, label in (("starbaseChannel", "irc.starbaseChannel"), ("commandChannel", "irc.commandChannel")):
        if key in value:
            require(value[key] == irc[key], not_standard(slug, label, value[key], irc[key]))
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
    value = fields(value, "api", "url initiatives vmHost", ["url"])
    base = url(value["url"], "API base URL", True, ("", "/", "/api", "/api/")).rstrip("/")
    initiatives = value.get("initiatives", [])
    require(isinstance(initiatives, list), "API initiatives must be an array.")
    ids = [pattern(item, UUID, "initiative UUID").lower() for item in initiatives]
    cfg = {"url": base if base.endswith("/api") else f"{base}/api", "initiatives": list(dict.fromkeys(ids))}
    if "vmHost" in value:
        cfg["vmHost"] = host(value["vmHost"])
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


def harness_config(value, inherited):
    value = fields(value, "harness", "type model family permissions allowedTools")
    backend = pattern(value.get("type", inherited["type"]), r"opencode|claude-sdk", "harness type (opencode or claude-sdk)")
    family = pattern(value.get("family", inherited["family"]), r"claude|codex", "model family")
    candidate = value.get("model", inherited["model"] if family == inherited["family"] else FAMILIES[family])
    model = pattern(candidate, MODEL if backend == "opencode" else r"[a-zA-Z0-9][a-zA-Z0-9._:\[\]-]{0,127}", "CLIProxyAPI model ID")
    tools = value.get("allowedTools", inherited.get("allowedTools", []))
    require(isinstance(tools, list), "Claude allowedTools must be an array.")
    for tool in tools:
        pattern(tool, r"Agent|Bash|Edit|Glob|Grep|LSP|NotebookEdit|Read|TodoWrite|WebFetch|WebSearch|Write", "Claude built-in tool")
    require(len(set(tools)) == len(tools), "Claude tools must be unique.")
    if backend == "claude-sdk":
        require(family == "claude" and "permissions" not in value, "Claude Agent SDK requires the Claude family and uses allowedTools.")
    else:
        require("allowedTools" not in value, "OpenCode uses permissions.")
    permissions = value.get("permissions", inherited["permissions"])
    require("permissions" not in value or isinstance(permissions, list) and permissions, "Specify a non-empty list of OpenCode permissions.")
    for permission in permissions or ():
        fields(permission, "permission", "action resource effect", ["action", "resource", "effect"])
        pattern(permission["effect"], r"allow|deny|ask", "permission effect")
        string(permission["action"], "permission action")
        string(permission["resource"], "permission resource", 1024)
    return {"type": backend, "family": family, "model": model, "permissions": permissions, "allowedTools": tools}


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


def crew_roster(captain, teams, names, slug):
    roster = []
    for family, team in teams.items():
        for slot in range(1, team["count"] + 1):
            key = f"{family}-{slot}"
            name = string(names.get(key, f"{family.title()} {slot}"), "crew display name").strip()
            require(re.fullmatch(r"[A-Za-z0-9 _-]+", name) and re.fullmatch(CREW_ID, crew_id(name)),
                    "Crew names use ASCII letters, digits, spaces, hyphens and underscores, and shorten to "
                    "an ID of at most 32 characters.")
            member_id = crew_id(name)
            roster.append({
                "id": member_id, "slot": key, "family": family, "name": name, "model": team["model"],
                # Native OpenCode agent and initial worktree branch, both namespaced by fleet and captain.
                "agent": f"{captain['id']}-crew-{member_id}", "branch": f"crew/{slug}-{captain['id']}-{member_id}",
            })
    ids = [person["id"] for person in roster]
    require(len(set(ids)) == len(ids) and not set(ids) & {captain["id"], "captain", "all"},
            "Crew names must give distinct IDs that differ from the captain's ID, captain and all.")
    return roster


def repository_config(value):
    require(isinstance(value, list), "repositories must be an array.")
    repos = {}
    for row in value:
        row = fields(row, "repository", "id url private", ["id", "url"])
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


def toolchain_config(value):
    tools = fields(value, "toolchains", "node rust go python kubectl gh zig deno")
    result = {}
    for name, version in {"node": "24", **tools}.items():
        if version is not False:
            result[name] = pattern(version, r"(?:[0-9]+(?:\.[0-9]+){0,2}|stable|latest)", f"{name} version")
    return result


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
        require(slack is not None, "Usage notices require rear-admiral Slack.")
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


def validate(config, credentials, check_files=True):
    cfg = fields(config, "fleet", "slug name rearAdmiral captains harness crew repositories stakeholders usage medical irc stargate dashboard proxy agentsview api agentDispatch operatorUsers toolchains maintenance githubPolicy", ["slug", "name", "rearAdmiral", "captains", "irc"])
    slug = pattern(cfg["slug"], r"[a-z0-9][a-z0-9-]{0,34}", "fleet slug")
    require(isinstance(cfg["captains"], list), "Captains must be an array.")
    officers = [member(person) for person in [cfg["rearAdmiral"], *cfg["captains"]]]
    require(not {"crew", "crewNames"} & officers[0].keys(), "Only captains may declare crew.")
    irc = irc_config(cfg["irc"], slug, check_files)
    captain_ids = [person["id"] for person in officers[1:]]
    trouble = standard_channels.problem(slug, captain_ids)
    require(trouble is None, trouble)
    # The rear admiral's room is Starbase; each captain's is its ship channel.
    officer_rooms = [irc["starbaseChannel"], *(standard_channels.ship(slug, captain_id) for captain_id in captain_ids)]
    for index, (person, room) in enumerate(zip(officers, officer_rooms, strict=True)):
        if "channel" in person:
            label = "rearAdmiral.channel" if index == 0 else f"Captain {person['id']}'s channel"
            require(person["channel"] == room, not_standard(slug, label, person["channel"], room))
    channels = standard_channels.standard(slug, captain_ids)
    for name in channels:
        channel(name)
    # Stargate joins at most 20 channels per registration, and the rear admiral joins every standard one.
    require(len(channels) <= 20, f"Use at most 18 captains: the rear admiral joins Starbase, {irc['commandChannel']} "
            "and every ship channel, and Stargate joins at most 20 channels.")
    irc.update(channels=channels)
    hull = harness_config(cfg.get("harness", {}), {"type": "opencode", "family": "codex", "model": FAMILIES["codex"], "permissions": None})
    crew = crew_config(cfg.get("crew", {}), {family: {"count": 2, "model": model} for family, model in FAMILIES.items()})
    repositories = repository_config(cfg.get("repositories", []))
    agents = []
    for index, (person, room) in enumerate(zip(officers, officer_rooms, strict=True)):
        role = "captain" if index else "rear_admiral"
        officer = {key: val for key, val in person.items() if key not in ("crew", "crewNames")}
        agents.append({**officer, "role": role, "channel": room, "crew": [],
                       "harness": harness_config(person.get("harness", {}), hull)})
        if role != "captain":
            continue
        crew_names = person.get("crewNames", {})
        require(isinstance(crew_names, dict), "crewNames must be an object keyed by crew slot.")
        teams = crew_config(person.get("crew", {}), crew)
        slots = {f"{family}-{slot}" for family, team in teams.items() for slot in range(1, team["count"] + 1)}
        require(crew_names.keys() <= slots, "crewNames may name only enabled crew slots.")
        if not slots:
            continue
        require(agents[-1]["harness"]["type"] == "opencode",
                f"Captain {person['id']} uses claude-sdk, which cannot host crew: crew run inside their captain's "
                "OpenCode server through Stargate's crew plugin. Set this captain's crew counts to 0.")
        # Crew work in Git worktrees of their captain's checkouts, so a fleet without repositories has none.
        if repositories:
            require(len(repositories) <= MAX_CREW_REPOSITORIES,
                    f"Captains with crew support at most {MAX_CREW_REPOSITORIES} repositories.")
            roster = crew_roster(person, teams, crew_names, slug)
            require(len(roster) * len(repositories) <= MAX_CREW_CONTEXTS,
                    f"Crew times repositories must not exceed {MAX_CREW_CONTEXTS} per captain (Stargate's crew limit).")
            agents[-1]["crew"] = roster
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

    private = fields(credentials, "secrets", "proxyKey ircPasswords ircOperatorPassword ircHistoryPassword apiToken rearAdmiralApiToken", ["proxyKey", "ircPasswords"])
    proxy_key = pattern(private["proxyKey"], r"[a-zA-Z0-9_=-]{20,256}", "proxy client key")
    passwords = private["ircPasswords"]
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
    require(required <= passwords.keys(), "Supply an IRC password for the rear admiral and every captain.")
    require(passwords.keys() <= permitted, "ircPasswords names an account that is not the rear admiral's or a captain's. "
            "Crew have no IRC accounts. Local mode also accepts passwords for allowlisted humans.")
    for password in passwords.values():
        if irc["mode"] == "local":
            ergo_password(password)
        else:
            string(password, "IRC password", 256)
    token = private.get("apiToken", "")
    if api:
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
        if person["harness"]["permissions"] is None:
            references = STATE / "references" / person["id"] if cfg.get("repositories") else None
            person["harness"]["permissions"] = default_permissions(person["role"], references)
        if dispatch and person["crew"]:
            fleet_ipc_policy(person["harness"]["permissions"], f"Captain {person['id']}'s harness.permissions")
    return {
        "slug": slug, "name": string(cfg["name"], "fleet name"), "members": agents, "irc": irc, "medical": medical,
        "repositories": repositories,
        "integrations": integrations_config(cfg), "operators": operators, "protectedBranches": protected,
        "toolchains": toolchain_config(cfg.get("toolchains", {})),
        "maintenance": maintenance_config(cfg.get("maintenance", {})),
        "stargate": {**source({key: stargate[key] for key in ("repo", "ref") if key in stargate},
                              "Stargate source", STARGATE_REPO, STARGATE_REF), "api": stargate_api},
        "dashboard": dashboard, "agentsview": view, "api": api,
        "agentDispatch": dispatch and {**dispatch, "token": dispatch_token},
        "proxy": {"port": proxy_port, "requestRetries": retries, "key": proxy_key},
        "passwords": passwords, "operatorPassword": operator_password, "token": token, "listeners": listeners,
        "historyPassword": history_password(irc, private),
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
    rear = cfg["members"][0]["id"]
    slack = integrations["slack"]
    quota = "reportChannel" in integrations["usage"]
    with private_directory(directory) as root:
        if root is None:
            require(partial or not (slack or quota or cfg["medical"]), "Integration credentials require a secrets directory.")
            return {}
        cloud = collect_cloud(root, [person["id"] for person in cfg["members"]])
        files = {(agent, "cloud", name): contents
                 for agent, entries in cloud.items() for name, contents in entries.items()}
        if slack:
            with private_directory(rear, root) as agent_dir:
                require(partial or agent_dir is not None, "Supply Slack credentials in the rear admiral's secrets directory.")
                if agent_dir is not None:
                    for name in ("slack-app-token", "slack-bot-token"):
                        contents = private_file(name, optional=partial, dir_fd=agent_dir)
                        if contents is not None:
                            files[rear, name] = contents
        if quota:
            contents = private_file("proxy-management-key", optional=partial, dir_fd=root)
            if contents is not None:
                files["proxy-management-key",] = contents
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
    rear = cfg["members"][0]["id"]
    tokens = {} if prune else previous.get("tokens", {}).copy()
    slack_tokens = previous.get("slack", {}).copy()
    slack = {} if prune else slack_tokens.get(rear, {}).copy()
    for name, prefix in (("slack-app-token", "xapp"), ("slack-bot-token", "xoxb")):
        if (rear, name) in files:
            slack[name] = pattern(files[rear, name].decode("ascii").strip(), fr"{prefix}-[a-zA-Z0-9-]{{8,512}}", name)
        require(not cfg["integrations"]["slack"] or name in slack, f"Supply {name} in the rear admiral's secrets directory.")
    if slack:
        slack_tokens[rear] = slack
    else:
        slack_tokens.pop(rear, None)
    if ("proxy-management-key",) in files:
        tokens["proxy-management-key"] = pattern(files["proxy-management-key",].decode("ascii").strip(), r"[a-zA-Z0-9_=-]{20,256}", "proxy management key")
    if "reportChannel" in cfg["integrations"]["usage"]:
        require("proxy-management-key" in tokens, "Supply a proxy management key in the secrets directory.")
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


def model_ref(family, model):
    return f"{PROVIDERS[family]}/{model}"


def model_provider(cfg, family, models):
    settings = {"baseURL": f"http://{LOOPBACK}:{cfg['proxy']['port']}/v1", "apiKey": "{env:CLIPROXYAPI_API_KEY}"}
    if family == "claude":
        package = "@opencode/ai/providers/anthropic-compatible"
    else:
        package = "@opencode/ai/providers/openai-compatible/responses"
        settings.update(encoding="none", transport="http")
    return {"name": f"Fleet model proxy, {family.title()} family", "package": package, "settings": settings,
            "models": {model: {"name": model} for model in models}}


def captain_permissions(cfg, person):
    """The captain root's stored grants: its harness policy plus the crew tools when it hosts crew, and the
    dispatch answer tool when agent dispatch is on."""
    permissions = person["harness"]["permissions"]
    if not person["crew"]:
        return permissions
    return [*permissions, *CREW_GRANTS, *([DISPATCH_GRANT] if cfg["agentDispatch"] else [])]


def crew_options(cfg, person, home, engine_port, session_id):
    """Stargate crew plugin options: one worktree per repository per crew member, under the captain."""
    repos = list(cfg["repositories"])

    def project(repo):
        return {"projectDirectory": str(home / "workspace" / repo),
                "worktrees": {worker["id"]: str(crew_worktree(home, worker, repo)) for worker in person["crew"]}}

    harness = person["harness"]
    default = project(repos[0])
    options = {
        "namespace": f"{cfg['slug']}-{person['id']}",
        "stateDirectory": str(home / ".local/stargate-crew"),
        "captainDirectory": default["projectDirectory"],
        **default,
        "mesijCommand": str(MESIJ),
        "team": {
            "captain": {"id": person["id"], "agent": sessions.CAPTAIN_AGENT,
                        "model": model_ref(harness["family"], harness["model"])},
            "historyScope": "team",
            "toolAccess": "curated",
            # The plugin reads crew history through this captain's own authenticated loopback engine.
            "historyAPI": {"url": f"http://{LOOPBACK}:{engine_port}",
                           "headersEnv": {"authorization": "OPENCODE_AUTHORIZATION"}},
            "modelProviders": sorted({PROVIDERS[harness["family"]], *(PROVIDERS[w["family"]] for w in person["crew"])}),
            "crew": [{
                "id": worker["id"], "name": worker["name"], "agent": worker["agent"],
                "model": model_ref(worker["family"], worker["model"]),
                "role": f"{worker['family'].title()}-family implementation and review",
                "permissionProfile": "coding",
            } for worker in person["crew"]],
        },
        "captainSessionID": session_id,
    }
    if len(repos) > 1:
        options["projects"] = {repo: project(repo) for repo in repos[1:]}
    return options


def pinned_session(person, workspace):
    """A captain's confirmed root session, after checking it still fits the configured crew.

    The record is read whatever the requested roster: zero crew or no repositories must not drop a pin.
    """
    try:
        record = sessions.SessionRecord(SESSIONS, person["id"]).load()
        if not record or not record.get("confirmed"):
            return None
        sessions.check_roster(record, [worker["id"] for worker in person["crew"]])
        require(record.get("directory") == str(workspace),
                "its first repository is fixed once its session is pinned, because Stargate's crew state names "
                "that checkout. Restore the original first repository.")
        return pattern(record.get("sessionID"), sessions.SESSION_ID.pattern, "pinned captain session ID")
    except (Invalid, sessions.SessionError) as error:
        raise Invalid(f"Captain {person['id']}: {error}") from None


def captain_pins(cfg):
    """Each captain's pinned root session or None, checked before anything is installed."""
    first = next(iter(cfg["repositories"]), None)
    pins = {}
    for person in cfg["members"][1:]:
        workspace = STATE / "agents" / person["id"] / "workspace"
        pins[person["id"]] = pinned_session(person, workspace / first if first else workspace)
    return pins


def rear_workspace(cfg):
    """The rear admiral's working directory: its reference checkout of the first repository, if any."""
    rear = cfg["members"][0]["id"]
    first = next(iter(cfg["repositories"]), None)
    return STATE / "references" / rear / first if first else STATE / "agents" / rear / "workspace"


def rear_pin(cfg):
    """The rear admiral's pinned root session while agent dispatch is on, or None.

    Captains name it in their fleetControl target, which is how they recognise their rear admiral's offers.
    """
    if not cfg["agentDispatch"]:
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


def render_engine(cfg, person, home, workspace, engine_port, seed, documents, session_id, answers_offers=False):
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
    engine = {
        "model": model_ref(harness["family"], harness["model"]), "permissions": harness["permissions"],
        "providers": {PROVIDERS[family]: model_provider(cfg, family, models) for family, models in catalogue.items()},
    }
    password = hmac.new(seed, agent_id.encode(), hashlib.sha256).hexdigest()
    header = "Basic " + base64.b64encode(f"opencode:{password}".encode()).decode()
    environment = {
        "OPENCODE_DB": str(database_dir / "opencode.db"),
        "OPENCODE_PASSWORD": password,
        "CLIPROXYAPI_API_KEY": cfg["proxy"]["key"],
        "OPENCODE_DISABLE_AUTOUPDATE": "true",
        "OPENCODE_DISABLE_MODELS_FETCH": "true",
        "OPENCODE_DISABLE_TERMINAL_TITLE": "true",
    }
    auth_var = "ENGINE_" + agent_id.upper().replace("-", "_")
    backend = {"type": "opencode", "url": f"http://{LOOPBACK}:{engine_port}", "directory": str(workspace),
               "permissions": harness["permissions"], "headersEnv": {"authorization": auth_var}}
    if person["crew"]:
        # Crew sessions run on this server, as this captain's Linux user, each with its own native agent.
        environment["OPENCODE_AUTHORIZATION"] = header
        engine["default_agent"] = sessions.CAPTAIN_AGENT
        engine["agents"] = {sessions.CAPTAIN_AGENT: {"model": engine["model"], "mode": "primary"}}
        for worker in person["crew"]:
            engine["agents"][worker["agent"]] = {
                "model": model_ref(worker["family"], worker["model"]), "mode": "primary",
                "description": f"{worker['name']}, {person['name']}'s {worker['family'].title()}-family crew member",
                "system": documents["crew"][worker["id"]],
            }
        if session_id:
            engine["plugins"] = [{
                "package": str(APP / "src" / f"stargate-{cfg['stargate']['ref']}"),
                "options": crew_options(cfg, person, home, engine_port, session_id),
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
    unit = serve_unit(
        f"engine-{agent_id}", f"OpenCode engine {agent_id}",
        f"{APP}/lib/runtime.py engine {settings}", user, [f"{PREFIX}proxy.service"],
        user_paths=[home, database_dir], post_start=f"{APP}/lib/runtime.py share-database {settings}",
    )
    return unit, backend, {auth_var: header}, database_dir


def irc_agent(cfg, person, backend):
    agent_id = person["id"]
    password_var = "IRC_" + agent_id.upper().replace("-", "_")
    return {
        "id": agent_id, "displayName": person["name"], "account": person["account"],
        "passwordEnv": password_var, "backend": backend,
    }, {password_var: cfg["passwords"][person["account"]]}


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
    return serve_unit("proxy", "CLIProxyAPI model proxy", f"{APP}/bin/cli-proxy-api --local-model --config {config_path}", PROXY, user_paths=[home])


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
    write(config, rules(ports, admins, clients), 0o600)
    write(SYSTEMD / FIREWALL, "\n".join([
        "[Unit]", "Description=Subspace fleet local service firewall",
        "After=nftables.service",
        "", "[Service]", "Type=oneshot", "RemainAfterExit=yes",
        f"ExecStart=/usr/sbin/nft -f {config}",
        "", "[Install]", "WantedBy=multi-user.target", "",
    ]), 0o644)
    return FIREWALL


def toolchains(versions, update=False):
    directory(TOOL_HOME, 0o755)
    directory(TOOL_HOME / "system-config", 0o755)
    data = "[tools]\n" + "".join(f'{name} = "{version}"\n' for name, version in sorted(versions.items()))
    write(ETC / "toolchains.toml", data, 0o644)
    if not versions:
        return
    # Source configuration is operator-owned and the working directory is outside agent workspaces.
    env = {**TOOL_ENV, "PATH": FLEET_PATH, "HOME": "/root", "MISE_YES": "1", "MISE_LOCKFILE": "0", "MISE_SELF_UPDATE_AVAILABLE": "0"}
    previous = os.umask(0o022)
    try:
        command = [str(APP / "bin/mise"), "upgrade", "--no-prune"] if update else [str(APP / "bin/mise"), "install"]
        subprocess.run(command, cwd=APP, env=env, check=True)
    finally:
        os.umask(previous)


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
            toolchains(cfg["toolchains"], update=True)
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
        subprocess.run(
            ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "gateway-key"],
            input=tokens["proxy-management-key"], text=True, check=True,
        )
        settings["quotaWatch"] = {
            "proxy": {"baseURL": f"http://{LOOPBACK}:{cfg['proxy']['port']}", "managementKeyFile": str(data.parent / "quota-management-key")},
            "stateFile": str(data / "quota.json"), "providers": usage["providers"],
            "report": {"bot": "fleet-stakeholder-slack", "channel": usage["reportChannel"]},
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
    slug_path = STATE / "slug"
    if slug_path.exists():
        require(slug_path.read_text().strip() == cfg["slug"], "Fleet slug is immutable on this VM.")
    write(slug_path, cfg["slug"] + "\n")
    pins = captain_pins(cfg)
    rear_session = rear_pin(cfg)
    withdraw_ready(pins)
    tokens = {**extra["tokens"], **extra.get("slack", {}).get(cfg["members"][0]["id"], {})}
    if "reportChannel" not in cfg["integrations"]["usage"]:
        tokens.pop("proxy-management-key", None)
    units = [render_proxy(cfg, tokens)]
    view_home = account(VIEW, STATE / "view")
    for path in (STATE / "agents", STATE / "sessions"):
        directory(path, 0o711)
    seed_path = STATE / "engine.seed"
    if not seed_path.exists():
        write(seed_path, secrets.token_hex(32) + "\n")
    seed = bytes.fromhex(seed_path.read_text().strip())
    ports = allocate_ports(cfg)

    gateway_home = account(GATEWAY, STATE / "gateway")
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
        subprocess.run(
            ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "role"],
            input=json_text(documents[agent_id]), text=True, check=True,
        )
        files = extra["cloud"].get(agent_id, {})
        if files or (home / ".config/subspace/cloud/.installer-files").is_file():
            subprocess.run(
                ["runuser", "-u", user, "--", str(APP / "lib/runtime.py"), "cloud", str(prune).lower()],
                input=json_text(files), text=True, check=True,
            )
    references = reconcile_workspaces(cfg, homes)
    databases, engines = [], []
    registrations, environments = {}, {}
    for person in cfg["members"]:
        agent_id = person["id"]
        home = homes[agent_id]
        harness = person["harness"]
        workspace = home / "workspace"
        if references:
            if person["role"] == "rear_admiral":
                workspace = references
            workspace /= next(iter(cfg["repositories"]))
        if harness["type"] == "opencode":
            pin = rear_session if person["role"] == "rear_admiral" else pins.get(agent_id)
            unit, backend, headers, database = render_engine(
                cfg, person, home, workspace, ports[agent_id], seed, documents[agent_id], pin,
                answers_offers=bool(fleet_control(cfg, person, pin, rear_session, gateway_home)))
            databases.append(database)
            units.append(unit)
            engines.append(unit)
        else:
            backend = {
                "type": "claude", "directory": str(workspace), "model": harness["model"], "allowedTools": harness["allowedTools"],
                "systemPrompt": documents[agent_id]["claudePrompt"],
                "provider": {"baseURL": f"http://{LOOPBACK}:{cfg['proxy']['port']}/v1", "apiKeyEnv": "CLIPROXYAPI_API_KEY"},
            }
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
                                         "writable": []})
        entry = irc_officer(cfg, person, registrations[agent_id])
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
        if person["crew"]:
            opts["ready"].append(ready_marker(agent_id))

    checks = []
    for group, options in groups.items():
        user, home, environment = options["user"], options["home"], options["environment"]
        data = home / ("gateway" if user != GATEWAY else "data")
        is_primary = group == "gateway"
        settings = {"dataDirectory": str(data), "dashboard": is_primary, "irc": options["irc"]}
        writable = [home, *options["writable"]]
        if is_primary:
            # Slack and Teams keep their own sessions; a pinned root has one chat owner, the rear admiral's IRC.
            stakeholder_backend = {key: value for key, value in registrations[rear["id"]]["backend"].items()
                                   if key != "sessionID"}
            integrations, paths = stakeholder_settings(cfg, tokens, stakeholder_backend, user, data, environment)
            settings.update(integrations)
            writable.extend(paths)
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
        ))
    write(ETC / "gateway-checks.json", json_text(checks))
    dash = cfg["dashboard"]
    bind = dash["bind"]
    units.append(serve_unit("dashboard", "Stargate dashboard", f"{APP}/lib/dashboard.py {bind} {dash['port']} {dashboard_socket}", dashboard_user, [f"{PREFIX}gateway.service"]))
    units.append(render_view(cfg, databases, view_home))
    units.insert(0, render_firewall(cfg))
    toolchains(cfg["toolchains"])
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
        if not (person["crew"] or rear and cfg["agentDispatch"]):
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
                "model": {"providerID": PROVIDERS[harness["family"]], "id": harness["model"]},
                "permissions": person["harness"]["permissions"] if rear else captain_permissions(cfg, person),
            }
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
        print(f"Pinned the root session of {', '.join(pinned)}. Fleet services restart to load the crew plugin.")
    require(not failed, " ".join(failed) + " The gateway keeps serving the rest of the fleet without "
            "unpinned captains. Fix the cause and run the installer again.")


def engine(path, attach=False):
    settings = read_json(path)
    env = {**os.environ, **TOOL_ENV, **settings["environment"], "PATH": FLEET_PATH}
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
        prune = tail[-1] == "true"
        if verb == "render":
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
        if verb == "prepare":
            stage = Path(tail[2])
            (stage / "fleet.json").write_text(json_text(config))
            (stage / "secrets.json").write_text(json_text(private))
            (stage / "extra-secrets.json").write_text(json_text(extra))
            for member_id, key in keys.items():
                directory = stage / "github-apps" / member_id
                directory.mkdir(parents=True)
                (directory / "github-app.pem").write_text(key)
            sources = [cfg["stargate"]["repo"], cfg["stargate"]["ref"], cfg["irc"]["mode"]]
            if cfg["irc"]["mode"] == "local":
                sources.extend(cfg["irc"]["compose"][key] for key in ("repo", "ref"))
            (stage / "sources").write_text("\n".join(sources) + "\n")
        else:
            count = sum(len(person["crew"]) for person in cfg["members"])
            print(f"Fleet {cfg['slug']} has {len(cfg['members']) - 1} captains, {count} crew and one rear admiral. "
                  f"IRC mode is {cfg['irc']['mode']}. Crew run as their captain and have no IRC accounts.")
            print(f"Standard IRC channels: {' '.join(cfg['irc']['channels'])} (Starbase, fleet command, then one "
                  "ship channel per captain)." + ("" if cfg["irc"]["mode"] == "local" else
                                                   " Register them on the IRC server before installing."))
            if dispatch := cfg["agentDispatch"]:
                print(f"Agent dispatch is on: the rear admiral checks {cfg['api']['url']} every "
                      f"{dispatch['pollIntervalSeconds']} seconds as its own API agent and offers issues in "
                      f"{cfg['irc']['commandChannel']}.")
            else:
                print("Agent dispatch is off.")
            if not cfg["repositories"] and len(cfg["members"]) > 1:
                print("No repositories are configured, so captains have no crew: crew work in Git worktrees of "
                      "their captain's checkouts.")
    elif verb == "commission":
        commission(tail[0] == "true", existing="existing" in tail[1:])
    elif verb == "engine":
        engine(tail[0], attach="attach" in tail[1:])
    elif verb == "gateway":
        gateway(tail[0], tail[1:])
    elif verb == "maintenance":
        maintain(tail[0])
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
    elif verb == "prune-app-keys":
        cfg = validate(read_json(ETC / "fleet.json"), read_json(ETC / "secrets.json", private=True))
        for person in cfg["members"]:
            if "githubApp" not in person:
                (ETC / "github-apps" / person["id"] / "github-app.pem").unlink(missing_ok=True)
    elif verb == "gateway-key":
        gateway_key()
    elif verb == "irc":
        compose(tail)
    else:
        raise Invalid("Unknown runtime action.")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (Invalid, sessions.SessionError) as exc:
        sys.exit(f"subspace-fleet: {exc}")
    except (OSError, ValueError, subprocess.CalledProcessError):
        sys.exit("subspace-fleet: runtime command or configuration failed.")
