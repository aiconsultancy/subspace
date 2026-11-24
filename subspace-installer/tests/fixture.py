"""Local IRCv3 SASL transport and Subspace API fixtures. No model requests."""

import base64
from collections import Counter
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import pwd
import re
import socketserver
import ssl
import stat
import subprocess
import sys
from threading import Thread
from time import monotonic, sleep
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlsplit
from urllib.request import Request, urlopen

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
from standard_channels import command  # noqa: E402


FLEET = "00000000-0000-0000-0000-000000000002"
ADMIN = "00000000-0000-0000-0000-000000000003"
COMMAND = command("example-fleet")
INITIATIVE = "00000000-0000-0000-0000-000000000001"
TOKEN = "fixture.header.signature"
PROXY = "fixture-proxy-client-key"
# Crew run as their captain, ss-orion, with no Linux user, engine or IRC account of their own.
CREW = [f"{family}-{index}" for family in ("claude", "codex") for index in (1, 2)]
MEMBERS = ["atlas", "orion"]
CAPTAIN_HOME = Path("/var/lib/subspace-fleet/agents/orion")
SENTRY_TOKEN = "fixture-sentry-read-token"
MEDICAL_ACCOUNT = "example-medical"


def legacy_names(config):
    """The example fleet as subspace-setup installed fleets before plain officer names: fleet-prefixed accounts
    (example-<id>), slot-named crew and no officerNames, which keeps those accounts and #<slug>-<id> ship channels."""
    config.pop("officerNames", None)
    for person in [config["rearAdmiral"], *config["captains"]]:
        person["account"] = f"example-{person['id']}"
        person.pop("crewNames", None)
    return config


class IRC(socketserver.StreamRequestHandler):
    def handle(self):
        nick = "*"
        sasl = False
        sent = False
        if not hasattr(self.server, "seen"):
            self.server.seen = set()
        for line in self.rfile:
            command = line.decode().strip().split()
            if not command:
                continue
            verb, *args = command
            match verb:
                case "CAP" if args[0] == "LS":
                    self.reply(f"CAP * LS :sasl=PLAIN account-tag message-tags server-time echo-message")
                case "CAP" if args[0] == "REQ":
                    self.reply("CAP * ACK " + " ".join(args[1:]))
                case "AUTHENTICATE" if args[0] == "PLAIN":
                    self.send("AUTHENTICATE +")
                case "AUTHENTICATE":
                    _, account, password = base64.b64decode(args[0]).decode().split("\0")
                    assert password == "fixture-irc-password"
                    if account == MEDICAL_ACCOUNT and (WORK / "reject-sasl").exists():
                        self.reply(f"904 {nick} :SASL authentication failed")
                        continue
                    self.server.seen.add(account)
                    sasl = True
                    self.reply(f"900 {nick} {nick}!fleet@localhost {account} :Logged in")
                    self.reply(f"903 {nick} :SASL authentication successful")
                case "NICK":
                    nick = args[0]
                case "JOIN":
                    self.send(f"@account={nick} :{nick}!fleet@localhost JOIN {args[0]}")
                    self.reply(f"366 {nick} {args[0]} :End of NAMES list")
                case "PRIVMSG" if nick == MEDICAL_ACCOUNT:
                    if not sasl:
                        self.reply(f"451 {nick} :Authenticate first")
                        continue
                    if "issues/42/" in line.decode() and (WORK / "reject-medical").exists():
                        self.reply(f"404 {nick} {args[0]} :Cannot send to channel")
                        continue
                    text = line.decode().rstrip("\r\n").removeprefix("PRIVMSG ")
                    with (WORK / "medical-alerts").open("a") as log:
                        log.write(text + "\n")
                    self.send(f"@account={nick};msgid=fixture-message :{nick}!fleet@localhost PRIVMSG {text}")
            if sasl and nick != "*" and not sent and verb == "CAP" and args[0] == "END":
                self.reply(f"001 {nick} :Welcome")
                self.reply(f"005 {nick} CHANTYPES=# CASEMAPPING=rfc1459 :are supported")
                if nick == MEDICAL_ACCOUNT:
                    self.reply(f"422 {nick} :MOTD file is missing")
                else:
                    self.reply(f"376 {nick} :End of MOTD")
                sent = True

    def send(self, message):
        self.wfile.write((message + "\r\n").encode())

    def reply(self, message):
        self.send(":localhost " + message)


class Refusal(Exception):
    """An API error response; its status is what the installer's API clients report."""

    def __init__(self, status, message=""):
        super().__init__(message or f"HTTP {status}")
        self.status = status
        self.detail = message


class Subspace:
    """An in-memory Subspace API: the fleet, ship, agent and initiative rules the installer meets.

    It follows the API's rules for these records (subspace-api src/fleets.ts and src/agents.ts): only an
    administrator registers fleets and agents, agent names, IRC nicks and account subjects are unique in the
    organisation ignoring case, ship names are unique in a fleet ignoring case, a captain needs a ship and a
    rear admiral has none, and a versioned write must name the current version.
    """

    NICK = r"[A-Za-z][A-Za-z0-9_-]{0,63}"
    NAME = r"[A-Za-z0-9-]{1,64}"
    CHANNEL = r"#[^\s,]{1,63}"

    def __init__(self, provider="dev", page=200):
        self.fleets, self.ships, self.agents = {}, {}, {}
        self.initiatives = {INITIATIVE: {"id": INITIATIVE, "name": "Example initiative", "fleetId": None, "channels": ""}}
        self.provider, self.page, self.admin = provider, page, True
        self.counts, self.writes, self.serial = Counter(), [], 0

    def identity(self, slug=None):
        self.serial += 1
        return FLEET if slug == "example-fleet" else f"00000000-0000-4000-8000-{self.serial:012d}"

    def fleet_by(self, reference):
        return next((row for row in self.fleets.values() if reference in (row["id"], row["slug"])), None)

    def call(self, method, path, body=None):
        self.counts[method] += 1
        if method != "GET":
            self.writes.append((method, re.sub(r"[0-9a-f]{8}-[0-9a-f-]{27}", "<id>", path.split("?")[0])))
        route = path.split("?")[0].strip("/").split("/")
        query = parse_qs(urlsplit(path).query)
        match method, route:
            case "GET", ["fleets", reference, "ships"]:
                fleet = self.fleet_by(reference) or self.missing()
                return [dict(row) for row in self.ships.values() if row["fleetId"] == fleet["id"]]
            case "POST", ["fleets", reference, "ships"]:
                fleet = self.fleet_by(reference) or self.missing()
                self.channel(body.get("channel"))
                if any(row["fleetId"] == fleet["id"] and row["name"].lower() == body["name"].lower() for row in self.ships.values()):
                    raise Refusal(409, "A ship in this fleet already has that name.")
                ship = {"id": self.identity(), "fleetId": fleet["id"], "name": body["name"],
                        "channel": body.get("channel"), "version": 1}
                self.ships[ship["id"]] = ship
                return dict(ship)
            case "PATCH", ["ships", ship_id]:
                ship = self.ships.get(ship_id) or self.missing()
                self.versioned(ship, body)
                channel = body["channel"] if "channel" in body else ship["channel"]
                self.channel(channel)
                if any(row is not ship and row["fleetId"] == ship["fleetId"] and row["name"].lower() == body["name"].lower()
                       for row in self.ships.values()):
                    raise Refusal(409, "A ship in this fleet already has that name.")
                if (ship["name"], ship["channel"]) != (body["name"], channel):
                    ship.update(name=body["name"], channel=channel, version=ship["version"] + 1)
                return dict(ship)
            case "GET", ["fleets", reference]:
                return dict(self.fleet_by(reference) or self.missing())
            case "POST", ["fleets"]:
                self.administrator()
                if self.fleet_by(body["slug"]):
                    raise Refusal(409, "A fleet already has that slug.")
                for key in ("starbaseChannel", "commandChannel"):
                    self.channel(body.get(key))
                fleet = {"id": self.identity(body["slug"]), "version": 1, **body,
                         "fleetApiUrl": self.fleet_api_url(body.get("fleetApiUrl"))}
                self.fleets[fleet["id"]] = fleet
                return dict(fleet)
            case "PATCH", ["fleets", reference]:
                self.administrator()
                fleet = self.fleet_by(reference) or self.missing()
                for key in ("starbaseChannel", "commandChannel"):
                    self.channel(body.get(key, fleet.get(key)))
                if "fleetApiUrl" in body:
                    body = {**body, "fleetApiUrl": self.fleet_api_url(body["fleetApiUrl"])}
                fleet.update(body, version=fleet["version"] + 1)
                return dict(fleet)
            case "GET", ["agents"]:
                rows = list(self.agents.values())
                start = int(query.get("cursor", ["0"])[0])
                limit = min(int(query.get("limit", ["50"])[0]), self.page)
                following = start + limit
                return {"items": [dict(row) for row in rows[start:following]],
                        "nextCursor": str(following) if following < len(rows) else None}
            case "POST", ["agents"]:
                self.administrator()
                agent = self.registration(body)
                agent.update(userId=self.identity(), isActive=True, version=1)
                self.agents[agent["userId"]] = agent
                return dict(agent)
            case "PUT", ["agents", agent_id]:
                self.administrator()
                agent = self.agents.get(agent_id) or self.missing()
                self.versioned(agent, body)
                changed = self.registration({key: value for key, value in body.items() if key != "version"}, agent)
                if changed["serviceTokenClientId"] != agent["serviceTokenClientId"]:
                    raise Refusal(409, "Register a new agent to bind a different account subject.")
                agent.update(changed, version=agent["version"] + 1)
                return dict(agent)
            case "POST", ["agents", agent_id, ("deactivate" | "reactivate") as change]:
                self.administrator()
                agent = self.agents.get(agent_id) or self.missing()
                self.versioned(agent, body)
                active = change == "reactivate"
                if agent["isActive"] != active:
                    agent.update(isActive=active, version=agent["version"] + 1)
                return dict(agent)
            case "GET", ["me"]:
                return {"userId": ADMIN}
            case "GET", ["me", "initiatives"]:
                return {"items": [self.initiative(row) for row in self.initiatives.values()]}
            case "GET", ["initiatives"]:
                return {"items": [self.initiative(row) for row in self.initiatives.values()], "nextCursor": None}
            case "POST", ["initiatives"]:
                configuration = body.get("channelConfiguration", {})
                row = {"id": self.identity(), "name": body["name"], "fleetId": None,
                       "channels": configuration.get("channels", "")}
                self.initiatives[row["id"]] = row
                return self.initiative(row)
            case "GET", ["initiatives", initiative]:
                return dict(self.initiatives.get(initiative) or self.missing())
            case "PUT", ["initiatives", initiative, "fleet"]:
                row = self.initiatives.get(initiative) or self.missing()
                assert body["fleetId"] is None or any(fleet["id"] == body["fleetId"] for fleet in self.fleets.values())
                row["fleetId"] = body["fleetId"]
                return dict(row)
        self.missing()

    def initiative(self, row):
        fleet = next((fleet for fleet in self.fleets.values() if fleet["id"] == row["fleetId"]), None)
        return {**row, "version": 1, "fleet": fleet and {"slug": fleet["slug"]}}

    def missing(self):
        raise Refusal(404, "Not found.")

    def administrator(self):
        if not self.admin:
            raise Refusal(403, "Requires an organisation administrator.")

    def channel(self, value):
        if value is not None and not re.fullmatch(self.CHANNEL, value):
            raise Refusal(400, "Invalid channel.")

    @staticmethod
    def fleet_api_url(value):
        """The API's stored form of a Fleet API address: https anywhere, http only on loopback, no credentials,
        query or fragment; lower-case scheme and host, no default port, no trailing slash."""
        if value is None:
            return None
        parts = urlsplit(value)
        scheme, host = parts.scheme.lower(), parts.hostname
        if (not host or scheme not in ("http", "https") or parts.username is not None or "?" in value or "#" in value
                or (scheme == "http" and host not in ("localhost", "127.0.0.1", "::1"))):
            raise Refusal(400, "Invalid fleetApiUrl.")
        port = "" if parts.port in (None, {"https": 443, "http": 80}[scheme]) else f":{parts.port}"
        return f"{scheme}://{f'[{host}]' if ':' in host else host}{port}{parts.path}".rstrip("/")

    def versioned(self, row, body):
        if body.get("version") is not None and body["version"] != row["version"]:
            raise Refusal(409, f"The record changed. Current version is {row['version']}.")

    def registration(self, body, old=None):
        if not (re.fullmatch(self.NAME, body["name"]) and body["rank"] in ("rearAdmiral", "captain", "crew")
                and (body.get("ircNick") is None or re.fullmatch(self.NICK, body["ircNick"]))):
            raise Refusal(400, "Invalid agent registration.")
        subject = body["serviceTokenClientId"]
        if self.provider == "dev" and not re.fullmatch(r"[^\s@]+@[^\s@]+\.[^\s@]+", subject):
            raise Refusal(400, "Use the account subject format for this identity provider.")
        subject = subject if self.provider == "workos" else subject.lower()
        fleet = self.fleet_by(body.get("fleetId") or "") or self.missing()
        ship = self.ships.get(body.get("shipId") or "")
        if body.get("shipId") and (not ship or ship["fleetId"] != fleet["id"]):
            raise Refusal(400, "Choose a ship in the selected fleet.")
        if (body["rank"] == "captain") != bool(ship) and body["rank"] != "crew":
            raise Refusal(400, "A captain needs a ship; a rear admiral must not supply one.")
        nick = body["ircNick"] if "ircNick" in body else (old or {}).get("ircNick")
        # An agent has one name: a new or changed name may not be another's nick, nor a new nick another's name.
        new_name = old is None or old["name"].lower() != body["name"].lower()
        new_nick = bool(nick) and (old is None or (old.get("ircNick") or "").lower() != nick.lower())
        for row in self.agents.values():
            if row is old:
                continue
            row_nick = (row.get("ircNick") or "").lower()
            if (row["serviceTokenClientId"] == subject or row["name"].lower() == body["name"].lower()
                    or (nick and row_nick == nick.lower()) or (new_name and row_nick == body["name"].lower())
                    or (new_nick and row["name"].lower() == nick.lower())):
                raise Refusal(409, f"An account subject, agent name or IRC nick is already registered: agent "
                                   f"{row['name']} of fleet {row['fleet']} has it.")
        return {"name": body["name"], "rank": body["rank"], "fleetId": fleet["id"], "fleet": fleet["slug"],
                "shipId": body.get("shipId"), "defaultModel": body["defaultModel"], "serviceTokenClientId": subject,
                "ircNick": nick}

    def summary(self):
        """The records a desktop reads: the fleets' channels, ships and active officers."""
        return {
            "fleets": {row["slug"]: {"name": row["name"], "starbaseChannel": row.get("starbaseChannel"),
                                     "commandChannel": row.get("commandChannel")} for row in self.fleets.values()},
            "ships": sorted((row["name"], row["channel"]) for row in self.ships.values()),
            "agents": sorted((row["name"], row["rank"], row["ircNick"], (self.ships.get(row["shipId"]) or {}).get("name"),
                              row["isActive"]) for row in self.agents.values()),
        }


class API(BaseHTTPRequestHandler):
    state = Subspace()

    def dispatch(self):
        if self.headers.get("Authorization") != f"Bearer {TOKEN}":
            self.reply(401, {})
            return
        path = self.path.removeprefix("/api/v1")
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"]))) if "Content-Length" in self.headers else {}
        if path == "/fleets" and self.command == "POST":
            assert body["ircHost"] == "127.0.0.1" and body["ircTls"] is True
            assert "captainNames" not in body and "rearAdmiralName" not in body
        elif path == "/fleets/example-fleet" and self.command == "PATCH":
            assert body == {"name": "Updated Fleet"}
        try:
            value = API.state.call(self.command, path, body)
        except Refusal as error:
            self.reply(error.status, {"detail": str(error)})
            return
        self.reply(201 if self.command == "POST" and path in ("/fleets", "/agents") else 200, value)

    do_GET = dispatch
    do_POST = dispatch
    do_PUT = dispatch
    do_PATCH = dispatch

    def reply(self, status, value):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        fleet = API.state.fleet_by("example-fleet") or {}
        (WORK / "api-results.json").write_text(json.dumps({
            "counts": API.state.counts, "fleetId": API.state.initiatives[INITIATIVE]["fleetId"],
            "name": fleet.get("name"), "records": API.state.summary(),
        }))
        self.wfile.write(body)

    def log_message(self, *_args):
        pass


class Sentry(BaseHTTPRequestHandler):
    def do_GET(self):
        assert self.headers.get("Authorization") == f"Bearer {SENTRY_TOKEN}"
        target = urlsplit(self.path)
        assert target.path == "/api/0/organizations/your-org/issues/"
        query = parse_qs(target.query)
        assert query.get("environment", []) in ([], ["production"])
        assert query["project"] == ["your-project", "other-project"]
        if (WORK / "sentry-down").exists():
            self.send_error(503)
            return
        link = ""
        search = query["query"][0]
        if search == "is:resolved lastSeen:-7d level:[error,fatal]":
            body = []
        else:
            assert search == "is:unresolved lastSeen:-7d level:[error,fatal]"
            now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
            rows = [
                {"id": issue_id, "status": status, "level": level, "lastSeen": now,
                 "project": {"slug": project, "id": "1"}, "count": count, "userCount": 0,
                 "substatus": "ongoing" if status == "unresolved" else None,
                 "permalink": f"https://127.0.0.1:18081/organizations/your-org/issues/{issue_id}/",
                 "title": "Private error contents"}
                for issue_id, status, project, level, count in (
                    ("41", "unresolved", "your-project", "fatal", "1"),
                    ("43", "unresolved", "your-project", "error", "1"),
                    ("44", "resolved", "your-project", "error", "1"),
                    ("42", "unresolved", "other-project", "error", "5"),
                )
            ]
            if "cursor" in query:
                assert query["cursor"] == ["100:1:0"]
                body = rows[-1:]
            else:
                body = rows[:-1]
                link = f'<http://127.0.0.1:18081{target.path}?cursor=100:1:0>; rel="next"; results="true"; cursor="100:1:0"'
        self.send_response(200)
        if link:
            self.send_header("Link", link)
        self.end_headers()
        self.wfile.write(json.dumps(body).encode())

    def log_message(self, *_args):
        pass


def write(example):
    # The container smoke keeps the fleet-prefixed accounts its checks name (example-<id>): a fleet installed
    # before plain officer names, which the installer keeps as it was.
    cfg = legacy_names(json.loads(Path(example).read_text()))
    cfg["irc"].update(host="127.0.0.1", caFile=str(WORK / "irc.pem"))
    # The fixture IRC server takes SASL PLAIN only; policy_test.py covers certificates (SASL EXTERNAL).
    cfg["irc"]["auth"] = "password"
    cfg["operatorUsers"] = ["fixture-operator"]
    (WORK / "irc.pem").chmod(0o644)
    cfg["api"] = {"url": "http://127.0.0.1:3000/api", "initiatives": [INITIATIVE], "agentUrl": "http://127.0.0.1:3001"}
    cfg["medical"].update(enabled=True, baseUrl="http://127.0.0.1:18081",
                          projects=["your-project", "other-project"], intervalSeconds=300)
    cfg["repositories"] = [
        {"id": "sample", "url": "http://127.0.0.1:18080/sample.git"},
        {"id": "second", "url": "http://127.0.0.1:18080/second.git"},
    ]
    (WORK / "fleet.json").write_text(json.dumps(cfg))
    path = WORK / "fleet.secrets.json"
    path.write_text(json.dumps({
        "proxyKey": PROXY, "apiToken": TOKEN,
        "ircPasswords": dict.fromkeys([f"example-{agent}" for agent in MEMBERS], "fixture-irc-password"),
    }))
    path.chmod(0o600)
    directory = WORK / "fleet.secrets" / "medical"
    directory.mkdir(parents=True, mode=0o700)
    for name, value in (("sentry-auth-token", SENTRY_TOKEN), ("irc-password", "fixture-irc-password")):
        path = directory / name
        path.write_text(value + "\n")
        path.chmod(0o600)
    for agent in MEMBERS:
        names = ["kubeconfig", "database-dsn"]
        if agent == "atlas":
            names.append("sentry-auth-token")
        for name in names:
            file = cloud_source(agent, name)
            file.parent.mkdir(parents=True, mode=0o700, exist_ok=True)
            file.write_text(f"Synthetic read-only {name} for {agent}\n")
            file.chmod(0o600)


def cloud_source(agent, name):
    agent_dir = WORK / "fleet.secrets" / agent
    return agent_dir / name if name == "kubeconfig" else agent_dir / "cloud" / name


def serve():
    tls = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    tls.load_cert_chain(WORK / "irc.pem", WORK / "irc.key")
    with socketserver.ThreadingTCPServer(("127.0.0.1", 6697), IRC) as irc:
        irc.socket = tls.wrap_socket(irc.socket, server_side=True)
        Thread(target=irc.serve_forever, daemon=True).start()
        with (ThreadingHTTPServer(("127.0.0.1", 18081), Sentry) as sentry,
              ThreadingHTTPServer(("127.0.0.1", 3000), API) as api):
            Thread(target=sentry.serve_forever, daemon=True).start()
            api.serve_forever()


def request(port, path, headers=None):
    with urlopen(Request(f"http://127.0.0.1:{port}{path}", headers=headers or {}), timeout=3) as result:
        return result.read()


def retry(operation, label):
    deadline = monotonic() + 60
    while monotonic() < deadline:
        try:
            return operation()
        except (OSError, URLError, AssertionError, ValueError):
            sleep(1)
    raise AssertionError(f"{label} did not become ready")


def check():
    def gateway():
        ready = Path("/var/lib/subspace-fleet/gateway/data/ready.json")
        info = json.loads(ready.read_text())
        assert sum(item["state"] == "connected" for item in info["transports"]) == len(MEMBERS)
        value = json.loads(request(8070, "/api/fleet"))
        assert "atlas" in json.dumps(value) and "orion" in json.dumps(value)
    retry(gateway, "Stargate IRC and dashboard")

    for agent, port in zip(MEMBERS, range(4096, 4096 + len(MEMBERS))):
        cfg = json.loads(Path(f"/etc/subspace-fleet/engine-{agent}.json").read_text())
        password = cfg["environment"]["OPENCODE_PASSWORD"]
        auth = base64.b64encode(f"opencode:{password}".encode()).decode()
        retry(lambda: request(port, "/api/info", {"Authorization": f"Basic {auth}"}), f"OpenCode {agent}")
        try:
            request(port, "/api/info")
        except HTTPError as error:
            assert error.code == 401
        else:
            raise AssertionError("OpenCode API accepted an unauthenticated request")
        database = f"/var/lib/subspace-fleet/sessions/{agent}/opencode.db"
        subprocess.run(["runuser", "-u", "ss-fleet-view", "--", "test", "-r", database], check=True)
    check_pinned_session()
    retry(lambda: request(8317, "/v1/models", {"Authorization": f"Bearer {PROXY}"}), "CLIProxyAPI")
    retry(lambda: request(8080, "/"), "AgentsView")
    print("Rear-admiral and captain IRC registrations, the captain's pinned session, dashboard, authenticated "
          "OpenCode servers, CLIProxyAPI and AgentsView answered.")


def check_pinned_session():
    """The captain's root session exists on its engine and both Stargate pins name it."""
    etc = Path("/etc/subspace-fleet")
    record = json.loads((etc / "sessions/orion.json").read_text())
    assert record["confirmed"] and record["crew"] == CREW
    assert stat.S_IMODE((etc / "sessions/orion.json").stat().st_mode) == 0o600
    backend = next(entry for entry in json.loads((etc / "stargate.json").read_text())["irc"] if entry["id"] == "orion")["backend"]
    assert backend["sessionID"] == record["sessionID"] and backend["agent"] == "build" and backend["enforcePermissions"]
    profile = json.loads((etc / "opencode-orion.json").read_text())
    [plugin] = profile["plugins"]
    assert plugin["options"]["captainSessionID"] == record["sessionID"]
    engine = json.loads((etc / "engine-orion.json").read_text())
    auth = base64.b64encode(f"opencode:{engine['environment']['OPENCODE_PASSWORD']}".encode()).decode()
    session = json.loads(request(engine["port"], f"/api/session/{record['sessionID']}", {"Authorization": f"Basic {auth}"}))["data"]
    assert session["agent"] == "build" and session["location"]["directory"] == backend["directory"]
    assert session["permissions"] == backend["permissions"]
    # A new captain's root works in its workspace above the checkouts; the first repository stays the project.
    workspace = str(CAPTAIN_HOME / "workspace")
    assert backend["directory"] == plugin["options"]["captainDirectory"] == record["directory"] == workspace
    assert plugin["options"]["projectDirectory"] == record["project"] == f"{workspace}/sample"


def check_shape():
    etc = Path("/etc/subspace-fleet")
    roster = json.loads((etc / "stargate.json").read_text())["irc"]
    rear, captain = roster
    # The standard set: the rear admiral joins all of it, a captain its ship, command and the other ships.
    assert rear["channels"] == ["#example-fleet-starbase", COMMAND, "#example-fleet-orion"]
    assert captain["channels"] == ["#example-fleet-orion", COMMAND]
    assert rear["access"]["humanWakeChannels"] == [{"channel": "#example-fleet-starbase", "replies": "all"}]
    assert captain["access"]["humanWakeChannels"] == [{"channel": "#example-fleet-orion", "replies": "all"}]
    assert captain["access"]["channelRequests"] == "nick-mention"
    # A direct message starts no turn: Stargate points the sender at the agent's first channel.
    assert rear["dmPolicy"] == captain["dmPolicy"] == "redirect"
    # Crew have no IRC registration of their own: they are reached through the captain.
    assert not rear.get("crew") and not captain.get("crew")
    assert captain["access"]["peerCaptains"] == ["example-atlas"]
    assert captain["backend"]["agent"] == "build" and "sessionID" not in rear["backend"]
    firewall = (etc / "firewall.nft").read_text()
    operator = pwd.getpwnam("fixture-operator").pw_uid
    assert f'tcp dport 8070 meta skuid != {{ 0, {operator} }}' in firewall
    assert f'tcp dport 8080 meta skuid != {{ 0, {operator} }}' in firewall
    model_uids = {0, operator, pwd.getpwnam("ss-fleet-gateway").pw_uid, *(pwd.getpwnam(f"ss-{agent}").pw_uid for agent in MEMBERS)}
    for worker in CREW:
        try:
            pwd.getpwnam(f"ss-orion-{worker}")
        except KeyError:
            continue
        raise AssertionError(f"Crew member {worker} has its own Linux user")
    units = (etc / "units").read_text().splitlines()
    assert sorted(unit for unit in units if "-engine-" in unit) == [f"subspace-fleet-engine-{agent}.service" for agent in MEMBERS]
    gateway_unit = Path("/etc/systemd/system/subspace-fleet-gateway.service").read_text()
    assert f"ConditionPathExists={etc}/sessions/orion.json" in gateway_unit
    assert f'tcp dport 8317 meta skuid != {{ {", ".join(map(str, sorted(model_uids)))} }}' in firewall
    for service in ("proxy", "view", "dashboard", "engine-atlas", "gateway"):
        unit = Path(f"/etc/systemd/system/subspace-fleet-{service}.service").read_text()
        assert "Requires=subspace-fleet-firewall.service" in unit
        assert "LogNamespace=subspace-fleet" in unit
    for timer in ("mirrors", "tools", "prune"):
        assert f"subspace-fleet-{timer}.timer" in (etc / "units").read_text().splitlines()
    for agent, role in zip(MEMBERS, ["rear-admiral", "captain"]):
        identity = pwd.getpwnam(f"ss-{agent}")
        opencode = Path(identity.pw_dir) / ".config/opencode"
        instructions = (opencode / "AGENTS.md").read_text()
        assert "Example Fleet" in instructions
        assert (opencode / "skills" / f"fleet-{role}" / "SKILL.md").is_file()
        assert (opencode / "skills/fleet-crew/SKILL.md").is_file() == (role == "captain")
        engine = json.loads((etc / f"opencode-{agent}.json").read_text())
        permissions = engine["permissions"]
        assert {"action": "read", "resource": "*", "effect": "allow"} in permissions
        # The API's agent listener: the officer's own name in a header, no token, and its tools allowed.
        account = f"example-{agent}"
        assert engine["mcp"]["servers"]["subspace"]["headers"] == {"X-Subspace-Agent": account}
        assert engine["mcp"]["servers"]["subspace"]["url"] == "http://127.0.0.1:3001/mcp"
        assert {"action": "subspace_*", "resource": "*", "effect": "allow"} in permissions
        assert (opencode / "skills/subspace-tracker/SKILL.md").is_file()
        assert ({"action": "edit", "resource": "*", "effect": "allow"} in permissions) == (role != "rear-admiral")
    ref = Path("/var/lib/subspace-fleet/references/atlas/sample")
    assert subprocess.check_output(["runuser", "-u", "ss-atlas", "--", "git", "-C", ref, "show", "HEAD:README.md"], text=True).strip() == "sample"
    for user in ("ss-atlas", "ss-orion"):
        assert subprocess.run(["runuser", "-u", user, "--", "test", "-w", ref]).returncode == 1
        assert subprocess.run(["runuser", "-u", user, "--", "test", "-w", ref / "README.md"]).returncode == 1
    assert subprocess.run(["runuser", "-u", "ss-orion", "--", "test", "-r", ref / "README.md"]).returncode == 1
    command = ["runuser", "-u", "ss-orion", "--", "env", "MISE_DATA_DIR=/opt/subspace-fleet/toolchains",
               "MISE_GLOBAL_CONFIG_FILE=/etc/subspace-fleet/toolchains.toml",
               "/opt/subspace-fleet/toolchains/shims/node", "--version"]
    assert subprocess.check_output(command, text=True).startswith("v24.")
    captain = pwd.getpwnam("ss-orion")
    # The captain's workspace instructions sit beside its checkouts, written as the captain.
    workspace = CAPTAIN_HOME / "workspace"
    instructions = (workspace / "AGENTS.md").read_text()
    assert "### `sample`" in instructions and "### `second`" in instructions and "<!-- Operator notes end -->" in instructions
    assert (workspace / "CLAUDE.md").read_text().endswith("@AGENTS.md\n")
    assert {(workspace / name).stat().st_uid for name in ("AGENTS.md", "CLAUDE.md")} == {captain.pw_uid}
    for repo in ("sample", "second"):
        checkout = CAPTAIN_HOME / "workspace" / repo
        git = ["runuser", "-u", "ss-orion", "--", "git", "-C", str(checkout)]
        assert subprocess.check_output([*git, "branch", "--show-current"], text=True).strip() == "main"
        # Nothing is written into a checkout.
        assert subprocess.check_output([*git, "status", "--porcelain"], text=True) == ""
        assert (checkout / ".git").is_dir()
        objects = checkout / ".git/objects"
        assert (objects / "info/alternates").read_text().strip() == f"/var/lib/subspace-fleet/repositories/{repo}.git/objects"
        assert (checkout / "README.md").read_text().strip() == repo
        assert checkout.stat().st_uid == captain.pw_uid
        # One worktree per repository per crew member, owned by the captain and sharing its checkout's Git data.
        for worker in CREW:
            worktree = CAPTAIN_HOME / "worktrees" / worker / repo
            git = ["runuser", "-u", "ss-orion", "--", "git", "-C", str(worktree)]
            branch = subprocess.check_output([*git, "branch", "--show-current"], text=True).strip()
            assert branch == f"crew/example-fleet-orion-{worker}"
            common = subprocess.check_output([*git, "rev-parse", "--path-format=absolute", "--git-common-dir"], text=True).strip()
            assert common == str(checkout / ".git")
            assert (worktree / ".git").is_file() and (worktree / "README.md").read_text().strip() == repo
            assert worktree.stat().st_uid == captain.pw_uid
            assert stat.S_IMODE(worktree.parent.stat().st_mode) == 0o700
    engine = json.loads((etc / "opencode-orion.json").read_text())
    assert engine["default_agent"] == "build"
    for worker in CREW:
        agent = engine["agents"][f"orion-crew-{worker}"]
        assert agent["mode"] == "primary"
        family = "claude-proxy" if worker.startswith("claude") else "openai-proxy"
        assert agent["model"].startswith(f"{family}/")
    assert "anthropic-compatible" in engine["providers"]["claude-proxy"]["package"]
    assert "openai-compatible" in engine["providers"]["openai-proxy"]["package"]
    check_cloud()
    print("Crew under the captain, one worktree per repository per crew member, model routing and private credentials passed.")


def check_firewall():
    command = "import socket, sys; socket.create_connection(('127.0.0.1', int(sys.argv[1])), timeout=3).close()"
    for user, allowed in (("fixture-operator", {8070, 8080, 8317}), ("ss-orion", {8317}), ("nobody", set())):
        for port in (8070, 8080, 8317):
            result = subprocess.run(["runuser", "-u", user, "--", "python3", "-c", command, str(port)], capture_output=True)
            assert (result.returncode == 0) == (port in allowed), (user, port, result.stderr)
    print("Kernel nftables rules restrict operator services and admit only registered model clients.")


def check_cloud():
    for agent in MEMBERS:
        identity = pwd.getpwnam(f"ss-{agent}")
        for name in ("kubeconfig", "database-dsn"):
            file = Path(identity.pw_dir) / ".config/subspace/cloud" / name
            assert (file.stat().st_uid, file.stat().st_gid) == (identity.pw_uid, identity.pw_gid)
            assert stat.S_IMODE(file.stat().st_mode) == 0o600
            source = cloud_source(agent, name)
            if not source.exists():
                source = source.parent / "cloud" / name
            assert file.read_bytes() == source.read_bytes()
            other = "orion" if agent == "atlas" else "atlas"
            assert subprocess.run(["runuser", "-u", f"ss-{other}", "--", "test", "-r", str(file)]).returncode != 0


def local():
    config = json.loads((WORK / "fleet.json").read_text())
    config["irc"] = {"mode": "local", "allowedAccounts": ["operator"],
                     "compose": {"operator": "operator", "ref": "0" * 40, "channels": ["#initiative"]}}
    config["captains"][0]["channel"] = "#example-fleet-orion"
    config["medical"].update(channel="#example-fleet-orion", target="orion")
    # Test a second ship's membership without launching local Docker-in-Docker.
    config["captains"].append({"id": "lyra", "name": "Lyra", "account": "example-lyra",
                               "crew": {"claude": {"count": 0}, "codex": {"count": 1}}})
    source = json.loads((WORK / "fleet.secrets.json").read_text())
    source["ircPasswords"]["example-lyra"] = "fixture-irc-password"
    source["ircOperatorPassword"] = "fixture-operator-password"
    Path("/etc/subspace-fleet/local-fixture.json").write_text(json.dumps(config))
    path = Path("/etc/subspace-fleet/local-fixture-secrets.json")
    path.write_text(json.dumps(source))
    path.chmod(0o600)
    subprocess.run(
        ["/opt/subspace-fleet/lib/runtime.py", "render", "/etc/subspace-fleet/local-fixture.json", str(path)], check=True,
    )
    subprocess.run(["/opt/subspace-fleet/lib/runtime.py", "check"], check=True)
    etc = Path("/etc/subspace-fleet")
    expected = sorted([COMMAND, "#example-fleet-lyra", "#example-fleet-orion", "#example-fleet-starbase", "#initiative"])
    assert json.loads((etc / "irc/channels.json").read_text()) == [{"name": name} for name in expected]
    compose = json.loads((etc / "irc/override.json").read_text())["services"]["irc"]
    assert compose["environment"]["IRC_CHANNELS_FILE"] == "/run/subspace/channels.json"
    assert f"{etc}/irc/channels.json:/run/subspace/channels.json:ro" in compose["volumes"]
    assert {"name": MEDICAL_ACCOUNT, "password": "fixture-irc-password"} in json.loads((etc / "irc/accounts.json").read_text())
    medical_unit = Path("/etc/systemd/system/subspace-fleet-medical.service").read_text()
    assert "After=network-online.target subspace-fleet-firewall.service subspace-fleet-irc.service" in medical_unit
    registrations = json.loads((etc / "stargate.json").read_text())["irc"]
    assert registrations[0]["channels"] == ["#example-fleet-starbase", COMMAND, "#example-fleet-orion", "#example-fleet-lyra"]
    assert registrations[1]["channels"] == ["#example-fleet-orion", COMMAND, "#example-fleet-lyra"]
    assert registrations[2]["channels"] == ["#example-fleet-lyra", COMMAND, "#example-fleet-orion"]
    assert registrations[1]["access"]["humanWakeChannels"] == [{"channel": "#example-fleet-orion", "replies": "all"}]
    assert "example-lyra" in registrations[1]["access"]["peerCaptains"]
    subprocess.run(
        ["/opt/subspace-fleet/bin/bun", str(Path(__file__).with_name("routing.mjs"))], check=True,
    )
    print("Local mode generated and mounted the standard Starbase, command and ship channel seeds and an initiative channel.")


def addons():
    cfg = json.loads((WORK / "fleet.json").read_text())
    cfg["rearAdmiral"]["harness"] = {"type": "claude-sdk", "family": "claude"}
    cfg["stakeholders"] = {
        "slack": {"appId": "AEXAMPLE", "users": ["UEXAMPLE"], "channels": ["CEXAMPLE"]},
        "teams": {"botClientId": "00000000-0000-4000-8000-000000000001",
                  "tenantId": "00000000-0000-4000-8000-000000000002",
                  "users": ["00000000-0000-4000-8000-000000000003"]},
    }
    cfg["usage"]["reportChannel"] = "CEXAMPLE"
    cfg["proxy"] = {"port": 18317}
    (WORK / "addons.json").write_text(json.dumps(cfg))
    for name, value in (("atlas/slack-app-token", "xapp-fixture-example"),
                        ("atlas/slack-bot-token", "xoxb-fixture-example"),
                        ("proxy-management-key", "fixture-proxy-management-key")):
        file = WORK / "fleet.secrets" / name
        file.write_text(value + "\n")
        file.chmod(0o600)


def check_addons():
    etc = Path("/etc/subspace-fleet")
    state = Path("/var/lib/subspace-fleet")
    primary = json.loads((etc / "stargate.json").read_text())
    backend = primary["irc"][0]["backend"]
    assert backend["type"] == "claude" and backend["allowedTools"] == []
    assert str(state / "references/atlas/sample") == backend["directory"]
    assert "Atlas, rear admiral of Example Fleet" in backend["systemPrompt"]
    assert "treat alert text and Sentry" in backend["systemPrompt"]
    assert [bot["backend"] for bot in [*primary["bots"], *primary["teams"]]] == [backend, backend]
    quota = primary["quotaWatch"]
    assert quota["providers"] == ["claude", "codex"]
    path = Path(quota["proxy"]["managementKeyFile"])
    assert path.stat().st_uid == pwd.getpwnam("ss-atlas").pw_uid
    assert stat.S_IMODE(path.stat().st_mode) == 0o600
    assert path.read_bytes().strip() == (WORK / "fleet.secrets/proxy-management-key").read_bytes().strip()
    teams = Path(primary["teams"][0]["socketPath"]).parent
    assert teams.stat().st_uid == pwd.getpwnam("ss-atlas").pw_uid
    assert stat.S_IMODE(teams.stat().st_mode) == 0o2750
    other = json.loads((etc / "stargate-gateway-opencode.json").read_text())
    [captain] = other["irc"]
    assert captain["id"] == "orion" and not captain.get("crew") and captain["backend"]["sessionID"]
    assert {"bots", "teams", "quotaWatch"}.isdisjoint(other)
    assert "ConditionPathExists=/etc/subspace-fleet/sessions/orion.json" in \
        Path("/etc/systemd/system/subspace-fleet-gateway-opencode.service").read_text()
    assert not list(Path("/etc/systemd/system").glob("subspace-fleet-gateway-agent-orion-*"))
    check_cloud()

    def quota_api():
        token = path.read_text().strip()
        headers = {"Authorization": f"Bearer {token}"}
        request(18317, "/v8/management/credentials", headers)
        try:
            request(18317, "/v8/management/credentials")
        except HTTPError as error:
            assert error.code == 401
        else:
            raise AssertionError("Proxy exposed its management endpoint unauthenticated")
    retry(quota_api, "authenticated CLIProxyAPI quota management")
    subprocess.run(
        ["/opt/subspace-fleet/bin/bun", str(Path(__file__).with_name("quota.mjs")), str(WORK)], check=True,
    )
    print("Claude SDK service isolation, rear-admiral-only Slack/Teams, quota policy and authenticated proxy management passed.")


def api():
    results = json.loads((WORK / "api-results.json").read_text())
    # Each register reads the fleet, its ships, the agents and the initiative. The first creates the fleet, the
    # captain's ship and both officers' agent records; the others write nothing but the renamed fleet.
    assert results["counts"] == {"GET": 13, "POST": 4, "PUT": 3, "PATCH": 1}, results
    assert results["fleetId"] == FLEET
    assert results["name"] == "Updated Fleet"
    assert results["records"] == {
        "fleets": {"example-fleet": {"name": "Updated Fleet", "starbaseChannel": "#example-fleet-starbase",
                                     "commandChannel": COMMAND}},
        "ships": [["Orion", "#example-fleet-orion"]],
        "agents": [["Atlas", "rearAdmiral", "example-atlas", None, True],
                   ["example-orion", "captain", "example-orion", "Orion", True]],
    }, results
    print("API creation, update, idempotent registration of the fleet's channels, ship and officers, unassign "
          "and reassign passed.")


if __name__ == "__main__":
    mode, work, *args = sys.argv[1:]
    WORK = Path(work)
    globals()[mode](*args)
