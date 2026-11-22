"""Local IRCv3 SASL transport and Subspace API fixtures. No model requests."""

import base64
from collections import Counter
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import pwd
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


class API(BaseHTTPRequestHandler):
    fleet = None
    assigned = None
    counts = Counter()

    def dispatch(self):
        if self.headers.get("Authorization") != f"Bearer {TOKEN}":
            self.reply(401, {})
            return
        path = self.path.removeprefix("/api/v1")
        method = self.command
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"]))) if "Content-Length" in self.headers else {}
        API.counts[method] += 1
        if path == "/fleets/example-fleet" and method == "GET":
            self.reply(200 if API.fleet else 404, API.fleet or {})
        elif path == "/fleets" and method == "POST":
            assert body["ircHost"] == "127.0.0.1" and body["ircTls"] is True
            assert "captainNames" not in body and "rearAdmiralName" not in body
            API.fleet = {"id": FLEET, **body}
            self.reply(201, API.fleet)
        elif path == "/fleets/example-fleet" and method == "PATCH":
            assert body == {"name": "Updated Fleet"}
            API.fleet.update(body)
            self.reply(200, API.fleet)
        elif path == f"/initiatives/{INITIATIVE}" and method == "GET":
            self.reply(200, {"id": INITIATIVE, "fleetId": API.assigned})
        elif path == f"/initiatives/{INITIATIVE}/fleet" and method == "PUT":
            assert body["fleetId"] in {None, FLEET}
            API.assigned = body["fleetId"]
            self.reply(200, {"id": INITIATIVE, "fleetId": API.assigned})
        else:
            self.reply(404, {})

    do_GET = dispatch
    do_POST = dispatch
    do_PUT = dispatch
    do_PATCH = dispatch

    def reply(self, status, value):
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        (WORK / "api-results.json").write_text(json.dumps({
            "counts": API.counts, "fleetId": API.assigned, "name": (API.fleet or {}).get("name"),
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
    cfg = json.loads(Path(example).read_text())
    cfg["irc"].update(host="127.0.0.1", caFile=str(WORK / "irc.pem"))
    cfg["operatorUsers"] = ["fixture-operator"]
    (WORK / "irc.pem").chmod(0o644)
    cfg["api"] = {"url": "http://127.0.0.1:3000/api", "initiatives": [INITIATIVE]}
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
        permissions = json.loads((etc / f"opencode-{agent}.json").read_text())["permissions"]
        assert {"action": "read", "resource": "*", "effect": "allow"} in permissions
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
    for repo in ("sample", "second"):
        checkout = CAPTAIN_HOME / "workspace" / repo
        git = ["runuser", "-u", "ss-orion", "--", "git", "-C", str(checkout)]
        assert subprocess.check_output([*git, "branch", "--show-current"], text=True).strip() == "main"
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
    assert results["counts"] == {"GET": 7, "POST": 1, "PUT": 3, "PATCH": 1}, results
    assert results["fleetId"] == FLEET
    assert results["name"] == "Updated Fleet"
    print("API creation, update, idempotent registration, unassign and reassign passed.")


if __name__ == "__main__":
    mode, work, *args = sys.argv[1:]
    WORK = Path(work)
    globals()[mode](*args)
