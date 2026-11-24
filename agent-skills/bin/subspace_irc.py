#!/usr/bin/env python3
"""Talk to your Subspace fleet on IRC from a local coding agent.

It signs in with YOUR IRC account (the fleet's agents take orders from it), so
every message it sends starts with who is really speaking:

  [on behalf of George (Claude Opus 5.5)] Can you take NAV-12 next?

  subspace_irc.py channels
  subspace_irc.py history '#fleet-starbase' [--limit 30] [--thread ID]
  subspace_irc.py say '#fleet-starbase' --as 'Claude Opus 5.5' [--for George] [--reply-to ID] [--wait SECONDS] 'message'
  subspace_irc.py wait '#fleet-starbase' --thread ID [--from NICK] [--after ID] [--timeout SECONDS]

--as (your model and version) is required to post. --for defaults to the name
saved by `./install.sh skills --name`. In a deployment repository, run its own
copy with --config agent-kit/agent.json, so that you act on that deployment.

`wait --after ID` counts only messages after ID: pass the last ID you saw to
wait for the next reply. Exit status 3 from say means the connection failed
after the message was written: it was, or may have been, posted, so check
history before sending it again.

With a hosted Subspace API ("identity_provider": "workos" in
~/.subspace/agent.json) it reads your fleets' channels and IRC servers from the
API and signs in to IRC with a short-lived password the API issues to your
account, using the session of `subspace_token.py --login`. With development
sign-in it uses the installed fleet's saved IRC password and reaches IRC
through the fleet's tunnel, or directly for a shared IRC server on your
private network.
"""

import argparse
import base64
import ipaddress
import json
import os
from pathlib import Path
import re
import socket
import ssl
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlsplit
from urllib.request import ProxyHandler, Request, build_opener

HOME = Path(os.environ.get("SUBSPACE_HOME", Path.home() / ".subspace"))
LINE_BYTES = 400
TIMEOUT = 20
SETTLE = 3  # seconds to gather the lines an agent sends straight after its first one
POSTED = 3  # exit status: the message was, or may have been, posted before a failure
# Capabilities used when the server offers them; SASL is required.
CAPS = ("message-tags", "server-time", "batch", "echo-message", "account-tag",
        "draft/multiline", "draft/chathistory", "chathistory")
MSGID = re.compile(r"[A-Za-z0-9_.:+/=-]{1,128}")
JOIN_ERRORS = {"403", "405", "471", "473", "474", "475", "477", "489"}
# Shapes of keys and tokens that must never reach a channel's history.
SECRET = re.compile(
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----|\bsirc1\.[A-Za-z0-9_-]{16,}|\bsenr1\.[A-Za-z0-9_.-]{16,}"
    r"|\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.|\b(?:ghp|gho|ghs|ghu|github_pat)_[A-Za-z0-9_]{20,}"
    r"|\bsk-[A-Za-z0-9_-]{20,}|\bxox[abprs]-[A-Za-z0-9-]{10,}"
)


def fail(message):
    sys.exit(f"subspace: {message}")


def read_env(path):
    values = {}
    if path.is_file():
        for line in path.read_text().splitlines():
            key, _, value = line.partition("=")
            if key and not key.startswith("#"):
                values[key.strip()] = value.strip()
    return values


def fold(name):
    return name.casefold()


def open_connection(address):
    return socket.create_connection(address, timeout=TIMEOUT)


def peer_address(sock):
    peer = sock.getpeername()
    return peer[0] if isinstance(peer, tuple) else ""


def loopback(host):
    try:
        return ipaddress.ip_address(host.strip("[]")).is_loopback
    except ValueError:
        return host.lower() == "localhost"


# Development sign-in: the installed fleet's saved password -----------------

class Development:
    hint = "Is the tunnel running? ./install.sh tunnel --background"

    def __init__(self, name):
        agents = read_env(HOME / "agents.env")
        name = name or agents.get("SUBSPACE_TARGET") or "subspace-fleet"
        target = HOME / "targets" / name
        summary = read_env(target / "summary.env")
        if not summary:
            fail(f"no installed fleet '{name}'. Run the installer first.")
        account = summary["SUBSPACE_ADMIN_ACCOUNT"]
        password_file, ca = target / "irc.password", None
        shared = target / "irc-target"
        if summary.get("SUBSPACE_IRC_MODE") == "remote":
            irc_dir = HOME / "targets" / shared.read_text().strip() if shared.is_file() else None
            # Only the shared server this fleet uses now: after a move its saved password and CA no longer apply.
            if not irc_dir or read_env(irc_dir / "summary.env").get("SUBSPACE_IRC_HOST") != summary.get("SUBSPACE_IRC_HOST"):
                fail("this fleet's IRC server is not managed by the installer, so there is no saved password for it "
                     "here. Use the desktop app for IRC, or a hosted API's sign-in (agent-skills/README.md).")
            password_file = irc_dir / f"{account}.password"
            ca = irc_dir / "ca.pem" if (irc_dir / "ca.pem").is_file() else None
        if not password_file.is_file():
            fail(f"no IRC password file at {password_file}.")
        self.account, self.password_file = account, password_file
        self.server = {
            "host": summary.get("SUBSPACE_IRC_HOST", "127.0.0.1"), "port": int(summary.get("SUBSPACE_IRC_PORT", 6667)),
            "tls": summary.get("SUBSPACE_IRC_TLS") == "1", "ca": ca,
        }
        self.channels = summary.get("SUBSPACE_CHANNELS", "").split()
        self.human = agents.get("SUBSPACE_HUMAN_NAME") or account

    def list_channels(self):
        print("\n".join(self.channels))

    def server_for(self, channel, fleet=None):
        return self.server

    def login(self, server):
        return self.account, self.password_file.read_text().strip()

    def default_human(self):
        return self.human


# Hosted API: the person's session and an API-issued IRC credential ---------

class ApiError(Exception):
    def __init__(self, status):
        super().__init__(status)
        self.status = status


class Hosted:
    hint = "Check your connection to the private network the fleet is on."

    def __init__(self, helper, config, config_file=None):
        self.helper, self.config, self.config_file = helper, config, config_file
        self.helper_path = Path(helper.__file__).resolve()
        self._fleets = None
        self.human = read_env(HOME / "agents.env").get("SUBSPACE_HUMAN_NAME", "")

    def sign_in_required(self):
        bound = f" --config {self.config_file}" if self.config_file else ""
        fail(f"you are not signed in to Subspace on this computer, or the session ended. "
             f"Ask the person to run: python3 {self.helper_path}{bound} --login")

    def api(self, method, path, body=None):
        try:
            token = self.helper.workos_token(self.config, self.helper.SessionStore(self.config))
        except self.helper.TokenError:
            self.sign_in_required()
        request = Request(f"{self.config.api_url}/api/v1{path}", method=method,
                          data=None if body is None else json.dumps(body).encode(), headers={
                              "Accept": "application/json", "Content-Type": "application/json",
                              "Authorization": f"Bearer {token}",
                          })
        handlers = [self.helper.NoRedirect()]
        if loopback(urlsplit(self.config.api_url).hostname or ""):
            handlers.append(ProxyHandler({}))  # a proxy must never carry the bearer token for this computer
        try:
            with build_opener(*handlers).open(request, timeout=TIMEOUT) as response:
                return json.loads(response.read(4 * 1024 * 1024))
        except HTTPError as error:
            error.close()
            if error.code == 401:
                self.sign_in_required()
            raise ApiError(error.code) from None
        except (URLError, OSError, ValueError):
            fail(f"cannot reach the Subspace API at {self.config.api_url}. {self.hint}")

    def fleets(self):
        """Each fleet of the person's initiatives once, with the channels it is known to carry."""
        if self._fleets is None:
            try:
                items = self.api("GET", "/me/initiatives").get("items", [])
            except ApiError as error:
                fail(f"the Subspace API answered HTTP {error.status} for your initiatives.")
            fleets = {}
            for initiative in items:
                fleet = initiative.get("fleet")
                if not fleet:
                    continue
                entry = fleets.setdefault(fleet["id"], {"fleet": fleet, "extra": []})
                for channel in re.findall(r"#[^\s,]+", initiative.get("channels") or ""):
                    if fold(channel) not in map(fold, entry["extra"]):
                        entry["extra"].append(channel)
            self._fleets = list(fleets.values())
        return self._fleets

    @staticmethod
    def channels_of(entry):
        fleet = entry["fleet"]
        known = [fleet.get("starbaseChannel"), fleet.get("commandChannel"),
                 *(ship.get("channel") for ship in fleet.get("ships", [])), *entry["extra"]]
        return [channel for channel in known if channel]

    @staticmethod
    def server_of(fleet):
        irc = fleet.get("irc") or {}
        host, port, tls = irc.get("host") or fleet.get("ircHost"), irc.get("port") or fleet.get("ircPort"), irc.get("tls")
        if tls is None:
            tls = fleet.get("ircTls")
        return {"host": host, "port": port, "tls": bool(tls), "ca": None} if host and port else None

    def list_channels(self):
        try:
            me = self.api("GET", "/me")
            print(f"You are {me.get('displayName')}, IRC account {me.get('username') or '(none yet)'}.")
        except ApiError:
            pass
        fleets = self.fleets()
        if not fleets:
            print("None of your initiatives has a fleet.")
        for entry in fleets:
            fleet = entry["fleet"]
            server = self.server_of(fleet)
            where = f"{server['host']}:{server['port']}{', TLS' if server['tls'] else ''}" if server else "no IRC server recorded"
            print(f"\n{fleet.get('name')} (fleet {fleet.get('slug')}) on {where}")
            agents = fleet.get("agents", [])
            admiral = [agent for agent in agents if agent.get("rank") == "rearAdmiral" and agent.get("ircNick")]
            rows = []
            if fleet.get("starbaseChannel"):
                who = f"the rear admiral, {admiral[0]['ircNick']}," if admiral else "the rear admiral"
                rows.append((fleet["starbaseChannel"], f"Starbase: people and {who} who answers every message"))
            if fleet.get("commandChannel"):
                rows.append((fleet["commandChannel"], "Fleet command: officers coordinate; an officer answers only when named"))
            for ship in fleet.get("ships", []):
                if not ship.get("channel"):
                    continue
                captains = [agent for agent in agents if agent.get("shipId") == ship.get("id") and agent.get("ircNick")]
                who = ", ".join(f"captain {agent['ircNick']}" + ("" if agent.get("available", True) else " (inactive)")
                                for agent in captains) or "its captain"
                rows.append((ship["channel"], f"Ship {ship.get('name')}: {who} answers every message"))
            for channel in entry["extra"]:
                if fold(channel) not in {fold(row[0]) for row in rows}:
                    rows.append((channel, "initiative channel: agents answer only when named"))
            width = max((len(row[0]) for row in rows), default=0)
            for channel, about in rows:
                print(f"  {channel.ljust(width)}  {about}")

    def server_for(self, channel, fleet=None):
        entries = self.fleets()
        if fleet:
            entries = [entry for entry in entries if fold(fleet) in (fold(entry["fleet"].get("slug", "")), fold(entry["fleet"]["id"]))]
            if not entries:
                fail(f"none of your initiatives has the fleet '{fleet}'. See: subspace_irc.py channels")
        else:
            carrying = [entry for entry in entries if fold(channel) in map(fold, self.channels_of(entry))]
            entries = carrying or entries
        servers = []
        for entry in entries:
            server = self.server_of(entry["fleet"])
            if server and (fold(server["host"]), server["port"], server["tls"]) not in [
                    (fold(s["host"]), s["port"], s["tls"]) for s in servers]:
                servers.append(server)
        if not servers:
            fail(f"no IRC server is recorded for {channel} in your fleets. See: subspace_irc.py channels")
        if len(servers) > 1:
            fail(f"{channel} could be on more than one of your fleets' IRC servers; pass --fleet with the fleet's slug.")
        return servers[0]

    def login(self, server):
        try:
            issued = self.api("POST", "/me/irc-credential", {"host": server["host"], "port": server["port"]})
        except ApiError as error:
            if error.status == 404:
                fail(f"the Subspace API issues no IRC sign-in for {server['host']}:{server['port']}: that IRC server "
                     "does not check sign-ins with this API. Use the desktop app, or ask an administrator.")
            if error.status == 409:
                fail("your Subspace account has no IRC account name yet. Ask an administrator.")
            fail(f"the Subspace API refused an IRC sign-in (HTTP {error.status}).")
        account, password = issued.get("account"), issued.get("password")
        if not (isinstance(account, str) and re.fullmatch(r"[^\s:@!,*?]{1,64}", account)
                and isinstance(password, str) and password and not set(password) & {"\0", "\r", "\n"}):
            fail("the Subspace API returned an IRC sign-in this helper cannot use.")
        return account, password

    def default_human(self):
        if self.human:
            return self.human
        try:
            return self.api("GET", "/me").get("displayName") or ""
        except ApiError:
            return ""


def deployment(config):
    """What makes two sign-in settings the same deployment, and the same session."""
    return config and (config.identity_provider, config.api_url, config.workos_client_id, config.workos_api_url)


def repository_settings():
    """agent-kit/agent.json of the Git repository you are working in, when it is a deployment repository."""
    try:
        here = Path.cwd()
    except OSError:
        return None
    for directory in (here, *here.parents):
        if (directory / ".git").exists():
            settings = directory / "agent-kit" / "agent.json"
            return settings if settings.is_file() else None
    return None


def hosted_or_development(target, config_file=None):
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import subspace_token as helper
    except ImportError:
        helper = None

    def settings(path=None):
        try:
            return helper.load_config(path)
        except helper.TokenError as error:
            fail(str(error))

    if config_file:
        if helper is None:
            fail("--config needs subspace_token.py beside this helper.")
        config = settings(config_file)
        return Hosted(helper, config, config_file) if config.identity_provider == "workos" else Development(target)
    configured = (HOME / "agent.json").is_file() or os.environ.get("SUBSPACE_IDENTITY_PROVIDER")
    config = settings() if helper and configured else None
    repository = repository_settings()
    if repository:
        # A deployment repository's agents act on its deployment only, whatever this computer's default is.
        bound = settings(repository) if helper else None
        if not bound or deployment(bound) != deployment(config):
            own = repository.parent / "bin" / "subspace_irc.py"
            here = config.api_url if config and config.identity_provider == "workos" else "development sign-in"
            fail(f"this repository is a deployment with its own sign-in settings ({repository}), and this helper is "
                 f"set up for {here}. Use the repository's own: python3 {own} --config {repository} ...")
    if config is None or config.identity_provider != "workos":
        return Development(target)
    return Hosted(helper, config)


# IRC ----------------------------------------------------------------------

def unescape(value):
    """An IRCv3 message tag value."""
    out, index = [], 0
    while index < len(value):
        if value[index] == "\\":
            if index + 1 < len(value):
                out.append({":": ";", "s": " ", "\\": "\\", "r": "\r", "n": "\n"}.get(value[index + 1], value[index + 1]))
            index += 2
        else:
            out.append(value[index])
            index += 1
    return "".join(out)


def parse(line):
    """(tags, source nick, command, params) of one IRC line."""
    tags = {}
    if line.startswith("@"):
        raw, _, line = line[1:].partition(" ")
        for item in raw.split(";"):
            key, _, value = item.partition("=")
            tags[key] = unescape(value)
    source = ""
    if line.startswith(":"):
        source, _, line = line[1:].partition(" ")
    middle, has_trailing, trailing = line.partition(" :")
    params = middle.split()
    if has_trailing:
        params.append(trailing)
    command = params.pop(0).upper() if params else ""
    return tags, source.split("!", 1)[0], command, params


def message_of(tags, nick, command, target, text):
    return {
        "time": tags.get("time", ""), "msgid": tags.get("msgid", ""), "nick": nick,
        "account": tags.get("account", ""), "command": command, "target": target, "text": text,
        "reply": tags.get("+draft/reply") or tags.get("+reply") or "", "progress": tags.get("+stargate/progress", ""),
    }


class Assembler:
    """Turns PRIVMSG and NOTICE lines, and draft/multiline batches, into whole messages."""

    def __init__(self):
        self.batches = {}

    def feed(self, parsed):
        tags, nick, command, params = parsed
        if command == "BATCH" and params:
            ref = params[0][1:]
            if params[0].startswith("+") and params[1:2] == ["draft/multiline"] and len(params) > 2:
                self.batches[ref] = {"tags": tags, "nick": nick, "target": params[2], "lines": [], "command": "PRIVMSG"}
            elif params[0].startswith("-") and ref in self.batches:
                batch = self.batches.pop(ref)
                return message_of(batch["tags"], batch["nick"], batch["command"], batch["target"], "\n".join(batch["lines"]))
            return None
        if command not in ("PRIVMSG", "NOTICE") or len(params) < 2:
            return None
        batch = self.batches.get(tags.get("batch"))
        if batch is None:
            return message_of(tags, nick, command, params[0], params[1])
        batch["command"] = command
        if batch["lines"] and "draft/multiline-concat" in tags:
            batch["lines"][-1] += params[1]
        else:
            batch["lines"].append(params[1])
        return None


def show(message):
    stamp = message["time"][:16].replace("T", " ")
    text = message["text"].replace("\n", "\n    ")
    about = [f"id {message['msgid']}"] if message["msgid"] else []
    if message["reply"]:
        about.append(f"reply to {message['reply']}")
    line = f"{stamp} <{message['nick']}> {text}".strip()
    print(f"{line}  [{', '.join(about)}]" if about else line, flush=True)


class Lost(Exception):
    """The IRC connection failed, or the server stopped answering."""


class Refused(Exception):
    """The IRC server refused a history request."""


class Irc:
    def __init__(self, mode, server):
        host, port = server["host"], int(server["port"])
        raw = open_connection((host, port))
        if server["tls"]:
            context = ssl.create_default_context(cafile=str(server["ca"]) if server.get("ca") else None)
            raw = context.wrap_socket(raw, server_hostname=host)
        elif not loopback(peer_address(raw)):
            raw.close()
            fail(f"{host}:{port} is not a TLS connection to this computer; an IRC sign-in is sent only over TLS.")
        self.sock, self.buffer, self.caps, self.pending = raw, b"", {}, []
        # say: "maybe" once the message is being written, "sent" once the server has echoed it back.
        self.posted, self.watch = None, None
        # The credential is fetched only once the server is the one named and, over TLS, verified.
        account, password = mode.login(server)
        self.account = self.nick = account
        self.send("CAP LS 302")
        self.send(f"NICK {account}")
        self.send(f"USER {account} 0 * :{account}")
        offered, renamed, acknowledged = {}, False, False
        while True:
            _, _, command, params = self.receive()
            if command == "CAP" and len(params) >= 3 and params[1] == "LS":
                for item in params[-1].split():
                    key, _, value = item.partition("=")
                    offered[key] = value
                if params[2] != "*":
                    if "sasl" not in offered:
                        fail("the IRC server offers no SASL sign-in.")
                    self.send("CAP REQ :" + " ".join(["sasl", *(cap for cap in CAPS if cap in offered)]))
            elif command == "CAP" and len(params) >= 3 and params[1] == "ACK":
                self.caps.update({cap: offered.get(cap, "") for cap in params[-1].split()})
                if not acknowledged:
                    acknowledged = True
                    self.send("AUTHENTICATE PLAIN")
            elif command == "CAP" and len(params) >= 2 and params[1] == "NAK":
                fail("the IRC server refused the capabilities this helper asked for.")
            elif command == "AUTHENTICATE" and params == ["+"]:
                secret = base64.b64encode(f"{account}\0{account}\0{password}".encode()).decode()
                for start in range(0, len(secret), 400):
                    self.send("AUTHENTICATE " + secret[start:start + 400])
                if len(secret) % 400 == 0:
                    self.send("AUTHENTICATE +")
            elif command == "903":
                self.send("CAP END")
            elif command in ("902", "904", "905", "906"):
                fail("IRC sign-in failed for your account.")
            elif command == "433" and not renamed:
                # Without multiclient, another session holds the nick; the account still signs the messages.
                renamed, self.nick = True, f"{account}-agent"
                self.send(f"NICK {self.nick}")
            elif command in ("432", "433"):
                fail("the IRC server refused this session's nick.")
            elif command == "001":
                self.nick = params[0] if params else self.nick
                return

    def send(self, line):
        self.sock.sendall((line + "\r\n").encode())

    def receive(self, deadline=None):
        """The next line, parsed, or None once the deadline (time.monotonic()) has passed.

        Without a deadline, a server that stays silent for TIMEOUT seconds raises Lost."""
        while True:
            if deadline is not None and time.monotonic() >= deadline:
                return None
            if self.pending:
                return self.pending.pop(0)
            if b"\n" in self.buffer:
                raw, _, self.buffer = self.buffer.partition(b"\n")
                line = raw.rstrip(b"\r").decode(errors="replace")
                if not line:
                    continue
                parsed = parse(line)
                if parsed[2] == "PING":
                    self.send("PONG :" + (parsed[3][-1] if parsed[3] else ""))
                    continue
                if parsed[2] == "ERROR":
                    raise Lost("the IRC server closed the connection: " + (parsed[3][-1] if parsed[3] else ""))
                return parsed
            wait = TIMEOUT if deadline is None else min(deadline - time.monotonic(), TIMEOUT)
            if wait <= 0:
                continue
            self.sock.settimeout(wait)
            try:
                data = self.sock.recv(65536)
            except (socket.timeout, TimeoutError):
                if deadline is None:
                    raise Lost("the IRC server stopped answering.") from None
                continue
            if not data:
                raise Lost("the IRC server closed the connection.")
            self.buffer += data

    def sync(self, token, check=None, deadline=None):
        """Read until the server answers a PING, passing each other line to check; False if the deadline passes."""
        self.send(f"PING :{token}")
        while True:
            parsed = self.receive(deadline)
            if parsed is None:
                return False
            if parsed[2] == "PONG" and parsed[3][-1:] == [token]:
                return True
            if check:
                check(parsed)

    def join(self, channel, deadline=None):
        def check(parsed):
            _, _, command, params = parsed
            if command in JOIN_ERRORS and len(params) > 1 and fold(params[1]) == fold(channel):
                fail(f"cannot join {channel}: {params[-1]}")
        # An attached multiclient session may already be in the channel, so no reply is not an error.
        self.send(f"JOIN {channel}")
        return self.sync("joined", check, deadline)

    def quit(self):
        try:
            self.send("QUIT :done")
            self.sock.close()
        except OSError:
            pass


def wrap(line, limit):
    """Cut a line into pieces of at most limit bytes that join back into exactly that line.

    A cut goes before a space where one fits, so the space starts the next piece. A word longer than a
    piece, such as a long URL, is cut between two characters, never inside one, and nothing is added."""
    pieces = []
    while len(line.encode()) > limit:
        size, fits = 0, 0
        for index, character in enumerate(line):
            size += len(character.encode())
            if size > limit:
                break
            fits = index + 1
        space = line.rfind(" ", 1, fits + 1)
        cut = space if space > 0 else fits
        pieces.append(line[:cut])
        line = line[cut:]
    return pieces + ([line] if line else [])


def multiline_limits(caps):
    if "draft/multiline" not in caps or "batch" not in caps:
        return None
    limits = dict(item.partition("=")[::2] for item in caps["draft/multiline"].split(",") if item)
    try:
        return int(limits.get("max-bytes", 0)), int(limits.get("max-lines", 0) or 100)
    except ValueError:
        return None


def has_history(irc):
    return bool({"draft/chathistory", "chathistory"} & set(irc.caps)) and "batch" in irc.caps


def matches(message, irc, channel, threads, senders):
    """An answer: in the channel, from someone else, and in one of the threads and from one of the senders
    when those are given (both, when both are)."""
    if fold(message["target"]) != fold(channel):
        return False
    if fold(message["account"] or message["nick"]) in (fold(irc.account), fold(irc.nick)):
        return False  # your own messages, from this session or another
    if senders and fold(message["nick"]) not in senders:
        return False
    return not threads or message["reply"] in threads


class Watch:
    """What a wait looks for in a channel, and the newest message it has read there (the next wait's --after)."""

    def __init__(self, irc, channel, threads, senders, anchor):
        self.irc, self.channel, self.threads, self.senders = irc, channel, list(threads), list(senders)
        self.anchor, self.seen, self.found, self.ids = anchor, set(), False, []

    def offer(self, message):
        """Show the message if it answers; True if it did."""
        if fold(message["target"]) != fold(self.channel):
            return False
        if message["msgid"]:
            if message["msgid"] in self.seen:
                return False
            self.seen.add(message["msgid"])
            if message["command"] == "PRIVMSG":
                self.anchor = message["msgid"]
        if not matches(message, self.irc, self.channel, set(self.threads), {fold(nick) for nick in self.senders}):
            return False
        if message["command"] == "NOTICE":
            if message["progress"]:
                print(f"... {message['nick']} is working ({message['progress']}): {message['text']}", flush=True)
            return False
        show(message)
        self.found = True
        return True

    def again(self):
        """The command that waits for the next answer after what this wait has read."""
        return " ".join([f"wait '{self.channel}'", *(f"--thread {thread}" for thread in self.threads),
                         *(f"--from {nick}" for nick in self.senders), *([f"--after {self.anchor}"] if self.anchor else [])])


def await_reply(irc, watch, deadline):
    """Read until the first answer, and what follows it for a moment, or until the deadline."""
    assembler = Assembler()
    if watch.found:
        deadline = min(deadline, time.monotonic() + SETTLE)
    while True:
        parsed = irc.receive(deadline)
        if parsed is None:
            return watch.found
        message = assembler.feed(parsed)
        if message and watch.offer(message):
            deadline = min(deadline, time.monotonic() + SETTLE)


def history_batch(irc, channel, deadline=None):
    """The messages of the server's chathistory batch for channel, or None if the deadline passes first.

    Only lines in that batch, and in the multiline batches inside it, are history. Anything else that
    arrives meanwhile, such as live messages for another channel of a multiclient account, is kept in
    order for whatever reads next."""
    assembler, outer, inner, messages, later = Assembler(), None, set(), [], []
    try:
        while True:
            parsed = irc.receive(deadline)
            if parsed is None:
                return None
            tags, _, command, params = parsed
            ours = outer is not None and tags.get("batch") in inner | {outer}
            if command == "BATCH" and params:
                ref = params[0][1:]
                if outer is None and params[0].startswith("+") and params[1:2] == ["chathistory"] \
                        and len(params) > 2 and fold(params[2]) == fold(channel):
                    outer = ref
                    continue
                if outer is not None and params[0] == "-" + outer:
                    return messages
                if ours and params[0].startswith("+"):
                    inner.add(ref)
                elif params[0].startswith("-") and ref in inner:
                    inner.discard(ref)
                    ours = True
            elif (command == "FAIL" and params[:1] == ["CHATHISTORY"]) or \
                    (command in ("403", "442") and len(params) > 1 and fold(params[1]) == fold(channel)):
                raise Refused(" ".join([command, *params]))
            if not ours:
                later.append(parsed)
                continue
            message = assembler.feed(parsed)
            if message and fold(message["target"]) == fold(channel):
                messages.append(message)
    finally:
        irc.pending = later + irc.pending


def compose(mode, channel, model, human, message, reply_to):
    """Check a message before connecting: (marker, its lines, each cut into pieces that fit an IRC line)."""
    model, human = model.strip(), (human or mode.default_human()).strip()
    if not model or len(model) > 60:
        fail("--as must name your model and version, for example 'Claude Opus 5.5'.")
    if not human or len(human) > 60:
        fail("--for must name the person you act for, in at most 60 characters.")
    if SECRET.search(message):
        fail("the message looks like it contains a key or token. Channel history keeps it and the fleet reads it; "
             "remove it.")
    if "\0" in message:
        fail("the message contains a NUL character, which IRC cannot carry.")
    if reply_to and not MSGID.fullmatch(reply_to):
        fail("--reply-to takes a message ID as history and say print it.")
    prefix = f"[on behalf of {human} ({model})] "
    # Budget against the whole line: "PRIVMSG <channel> :<prefix><text>".
    budget = LINE_BYTES - len(f"PRIVMSG {channel} :{prefix}".encode())
    if budget < 100:
        fail("the channel name and attribution leave too little room for a message.")
    lines = [line if line.strip() else "" for line in message.replace("\r\n", "\n").replace("\r", "\n").split("\n")]
    while lines and not lines[0]:
        lines.pop(0)
    while lines and not lines[-1]:
        lines.pop()
    if not lines:
        fail("nothing to say.")
    return prefix, [wrap(line, budget) for line in lines]


def say(irc, channel, prefix, lines, reply_to, wait):
    if reply_to and "message-tags" not in irc.caps:
        fail("this IRC server does not carry replies (message-tags), so there are no threads.")
    reply_tag = f"@+draft/reply={reply_to} " if reply_to else ""
    limits = multiline_limits(irc.caps)
    count = sum(len(pieces) or 1 for pieces in lines)
    size = len(prefix.encode()) + sum(len(piece.encode()) for pieces in lines for piece in pieces) + len(lines) - 1
    irc.join(channel)
    irc.posted = "maybe"  # from here on, a failure may come after the server took the message
    if count > 1 and limits and count <= limits[1] and (not limits[0] or size <= limits[0]):
        # One message of several lines, exactly as written: the marker starts it, a blank line keeps a
        # paragraph break, and the pieces of a long line join again without a break or a space added.
        irc.send(f"{reply_tag}BATCH +say draft/multiline {channel}")
        first = True
        for pieces in lines:
            for index, piece in enumerate(pieces or [""]):
                tags = "@batch=say;draft/multiline-concat" if index else "@batch=say"
                irc.send(f"{tags} PRIVMSG {channel} :{prefix + piece if first else piece}")
                first = False
        irc.send("BATCH -say")
        expected = [prefix + "\n".join("".join(pieces) for pieces in lines)]
    else:
        # Without multiline, each line is its own message with the marker; blank lines are left out.
        expected = [prefix + (piece.lstrip(" ") if index else piece)
                    for pieces in lines for index, piece in enumerate(pieces) if piece.strip()]
        for text in expected:
            irc.send(f"{reply_tag}PRIVMSG {channel} :{text}")
    assembler, echoed, later = Assembler(), [], []

    def taken(ids):
        """The server has the message: what to wait on, its thread and the IDs it was given."""
        ids = [msgid for msgid in ids if msgid]
        # An agent may answer a reply in a thread under the thread's first message.
        threads = list(dict.fromkeys([*ids, *([reply_to] if reply_to else [])])) if ids else []
        irc.posted, irc.watch = "sent", Watch(irc, channel, threads, [], ids[-1] if ids else "")
        irc.watch.ids = ids

    def check(parsed):
        _, _, command, params = parsed
        # Ergo answers a refused message with an error numeric (for example 404 or 417) or FAIL.
        if command == "FAIL" or (command.isdigit() and 400 <= int(command) < 600):
            irc.quit()
            fail("the IRC server refused the message: " + " ".join([command, *params])
                 + (f" ({len(echoed)} of its {len(expected)} lines were posted)" if echoed else ""))
        later.append(parsed)  # a quick answer, kept for --wait
        message = assembler.feed(parsed)
        if (message and len(echoed) < len(expected) and fold(message["target"]) == fold(channel)
                and fold(message["nick"]) == fold(irc.nick) and message["text"].rstrip() == expected[len(echoed)].rstrip()):
            echoed.append(message["msgid"])
            if len(echoed) == len(expected):
                taken(echoed)

    irc.sync("sent", check)
    if irc.posted != "sent":
        taken(echoed)  # without echo-message, the server's answer to the PING is all there is
    irc.pending = later + irc.pending
    ids, threads = irc.watch.ids, irc.watch.threads
    noun = "message" if len(expected) == 1 else f"{len(expected)} lines"
    print(f"sent {noun} to {channel} as {irc.account}, marked {prefix.strip()}{shown_ids(ids)}", flush=True)
    if wait:
        print(f"waiting up to {wait} seconds for a reply" + (" in its thread" if threads else " in the channel")
              + "...", flush=True)
        if not await_reply(irc, irc.watch, time.monotonic() + wait):
            print("no reply yet. The message was sent; do not send it again."
                  + (f" Keep waiting with: {irc.watch.again()}" if threads else ""), flush=True)
        elif threads:
            print(f"To wait for the next reply: {irc.watch.again()}", flush=True)
    irc.quit()


def history(irc, channel, limit, thread):
    if not has_history(irc):
        fail("this IRC server keeps no history this helper can read.")
    # Ergo serves history only to members of the channel.
    irc.join(channel)
    irc.send(f"CHATHISTORY LATEST {channel} * {limit}")
    try:
        messages = history_batch(irc, channel)
    except Refused as error:
        fail(f"cannot read {channel}: {error}")
    irc.quit()
    # HistServ's are Ergo's join and quit notices.
    messages = [message for message in messages if message["command"] == "PRIVMSG" and message["nick"] != "HistServ"]
    if thread:
        messages = [message for message in messages if thread in (message["msgid"], message["reply"])]
    for message in messages:
        show(message)


def catch_up(irc, watch, deadline):
    """Show the answers already in the channel's history after the anchor; None if the deadline passes first."""
    anchor = watch.anchor
    for _ in range(10):  # at most 1,000 messages
        irc.send(f"CHATHISTORY AFTER {watch.channel} msgid={anchor} 100")
        try:
            messages = history_batch(irc, watch.channel, deadline)
        except Refused:
            break
        if messages is None:
            return None
        for message in messages:
            watch.offer(message)
        if len(messages) < 100 or not messages[-1]["msgid"]:
            break
        anchor = messages[-1]["msgid"]
    return watch.found


def latest(irc, channel, deadline):
    """The ID of the channel's newest message, from which a later wait can catch up."""
    irc.send(f"CHATHISTORY LATEST {channel} * 1")
    try:
        messages = history_batch(irc, channel, deadline) or []
    except Refused:
        return ""
    return next((message["msgid"] for message in reversed(messages) if message["command"] == "PRIVMSG"), "")


def wait_command(irc, channel, threads, senders, after, timeout):
    # One deadline for everything: joining, catching up from history and the live wait.
    deadline = time.monotonic() + timeout
    watch = Watch(irc, channel, threads, senders, after or (threads[0] if threads else ""))
    if irc.join(channel, deadline):
        if has_history(irc) and watch.anchor:
            catch_up(irc, watch, deadline)  # replies may have arrived while nobody was listening
        elif has_history(irc):
            watch.anchor = latest(irc, channel, deadline)  # nothing before now counts
        await_reply(irc, watch, deadline)
    irc.quit()
    if not watch.found:
        sys.exit(f"subspace: no reply within {timeout} seconds. Keep waiting with: {watch.again()}")
    print(f"To wait for the next reply: {watch.again()}", flush=True)


def shown_ids(ids):
    return f" [id {ids[0]}]" if len(ids) == 1 else f" [ids {', '.join(ids)}]" if ids else ""


def unsure(irc, channel, detail):
    """Exit after a failure that came once the message was written: it was, or may have been, posted."""
    if irc.posted == "sent":
        hint = f" Keep waiting with: {irc.watch.again()}" if irc.watch.threads else ""
        message = f"{detail} The message was sent{shown_ids(irc.watch.ids)}; do not send it again.{hint}"
    else:
        message = (f"{detail} This came after the message was written, so it may have been posted. Check with: "
                   f"history '{channel}' --limit 10, and send it again only if it is not there.")
    print(f"subspace: {message}", file=sys.stderr, flush=True)
    sys.exit(POSTED)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--target", help="development sign-in: the installed fleet to use")
    parser.add_argument("--fleet", help="hosted API: the fleet whose IRC server to use, when a channel is ambiguous")
    parser.add_argument("--config", help="use these sign-in settings, such as a deployment repository's "
                        "agent-kit/agent.json, instead of ~/.subspace/agent.json")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("channels", help="list your fleets' channels and who answers in each")
    read = sub.add_parser("history", help="print recent messages in a channel, with their IDs")
    read.add_argument("channel")
    read.add_argument("--limit", type=int, default=30)
    read.add_argument("--thread", help="only this message and the replies to it")
    write = sub.add_parser("say", help="send a message, marked as from you via your agent")
    write.add_argument("channel")
    write.add_argument("message", nargs="+")
    write.add_argument("--as", dest="model", required=True, help="your model and version, for example 'Claude Opus 5.5'")
    write.add_argument("--for", dest="human", help="the person you act for (default: saved name)")
    write.add_argument("--reply-to", help="post in the thread of this message ID")
    write.add_argument("--wait", type=int, default=0, metavar="SECONDS", help="then wait this long for a reply")
    listen = sub.add_parser("wait", help="wait for a reply in a thread or from an agent")
    listen.add_argument("channel")
    listen.add_argument("--thread", dest="threads", action="append", default=[], metavar="ID",
                        help="a message ID whose replies to wait for (repeat for a thread's first message too)")
    listen.add_argument("--from", dest="senders", action="append", default=[], metavar="NICK",
                        help="wait for this agent's messages (with --thread: its replies in the thread)")
    listen.add_argument("--after", metavar="ID", help="count only messages after this one: the last ID you saw, "
                        "to wait for the next reply (default: the first --thread, or now)")
    listen.add_argument("--timeout", type=int, default=240, metavar="SECONDS")
    for command in sub.choices.values():  # --config after the command works too
        command.add_argument("--config", default=argparse.SUPPRESS, help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.command == "wait":
        if not all(MSGID.fullmatch(value) for value in [*args.threads, *([args.after] if args.after else [])]):
            fail("--thread and --after take a message ID as history and say print it.")
        if not args.threads and not args.senders:
            fail("pass --thread with the ID that say printed, or --from with an agent's nick.")
    mode = hosted_or_development(args.target, args.config)
    if args.command == "channels":
        mode.list_channels()
        return
    if not args.channel.startswith("#"):
        fail("name a channel (#name). Direct messages from your account are not supported here.")
    if args.command == "say":
        prefix, lines = compose(mode, args.channel, args.model, args.human, " ".join(args.message), args.reply_to)
    server = mode.server_for(args.channel, args.fleet)
    irc = None
    try:
        irc = Irc(mode, server)
        if args.command == "history":
            history(irc, args.channel, max(1, min(args.limit, 200 if not args.thread else 500)), args.thread)
        elif args.command == "say":
            say(irc, args.channel, prefix, lines, args.reply_to, max(0, min(args.wait, 3600)))
        else:
            wait_command(irc, args.channel, args.threads, args.senders, args.after, max(1, min(args.timeout, 3600)))
    except ssl.SSLCertVerificationError as error:
        fail(f"the IRC server's certificate could not be verified ({error.verify_message}).")
    except (Lost, OSError, ssl.SSLError) as error:
        detail = str(error) if isinstance(error, Lost) else \
            f"IRC connection to {server['host']}:{server['port']} failed ({error})."
        if irc is not None:
            irc.sock.close()
            if irc.posted:
                unsure(irc, args.channel, detail)
        fail(f"{detail} {mode.hint}")


if __name__ == "__main__":
    main()
