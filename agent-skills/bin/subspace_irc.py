#!/usr/bin/env python3
"""Talk to your Subspace fleet on IRC from a local coding agent.

It signs in with YOUR IRC account (the fleet's agents take orders from it), so
every message it sends starts with who is really speaking:

  [on behalf of George (Claude Opus 5.5)] Can you take NAV-12 next?

  subspace_irc.py channels
  subspace_irc.py history '#fleet-starbase' [--limit 30] [--thread ID]
  subspace_irc.py say '#fleet-starbase' --as 'Claude Opus 5.5' [--for George] [--reply-to ID] [--wait SECONDS] 'message'
  subspace_irc.py wait '#fleet-starbase' --thread ID [--from NICK] [--timeout SECONDS]

--as (your model and version) is required to post. --for defaults to the name
saved by `./install.sh skills --name`.

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
from urllib.request import Request, build_opener

HOME = Path(os.environ.get("SUBSPACE_HOME", Path.home() / ".subspace"))
LINE_BYTES = 400
TIMEOUT = 20
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

    def __init__(self, helper, config):
        self.helper, self.config = helper, config
        self.helper_path = Path(helper.__file__).resolve()
        self._fleets = None
        self.human = read_env(HOME / "agents.env").get("SUBSPACE_HUMAN_NAME", "")

    def sign_in_required(self):
        fail(f"you are not signed in to Subspace on this computer, or the session ended. "
             f"Ask the person to run: python3 {self.helper_path} --login")

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
        try:
            with build_opener(self.helper.NoRedirect()).open(request, timeout=TIMEOUT) as response:
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


def hosted_or_development(target):
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import subspace_token as helper
    except ImportError:
        helper = None
    if helper is None or (not (HOME / "agent.json").is_file() and not os.environ.get("SUBSPACE_IDENTITY_PROVIDER")):
        return Development(target)
    try:
        config = helper.load_config()
    except helper.TokenError as error:
        fail(str(error))
    return Hosted(helper, config) if config.identity_provider == "workos" else Development(target)


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
        # The credential is fetched only once the server is the one named and, over TLS, verified.
        account, password = mode.login(server)
        self.account = self.nick = account
        self.send("CAP LS 302")
        self.send(f"NICK {account}")
        self.send(f"USER {account} 0 * :{account}")
        offered, renamed, acknowledged = {}, False, False
        while True:
            _, _, command, params = self.receive_or_fail()
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
        """The next line, parsed, or None once the deadline (time.monotonic()) passes."""
        if self.pending:
            return self.pending.pop(0)
        while True:
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
                    fail("the IRC server closed the connection: " + (parsed[3][-1] if parsed[3] else ""))
                return parsed
            wait = TIMEOUT if deadline is None else deadline - time.monotonic()
            if wait <= 0:
                return None
            self.sock.settimeout(min(wait, TIMEOUT))
            try:
                data = self.sock.recv(65536)
            except (socket.timeout, TimeoutError):
                if deadline is None:
                    raise
                continue
            if not data:
                fail("the IRC server closed the connection.")
            self.buffer += data

    def receive_or_fail(self):
        try:
            return self.receive()
        except (socket.timeout, TimeoutError):
            fail("the IRC server stopped answering.")

    def sync(self, token, check=None):
        """Read until the server answers a PING, passing each line to check."""
        self.send(f"PING :{token}")
        while True:
            parsed = self.receive_or_fail()
            if parsed[2] == "PONG" and parsed[3][-1:] == [token]:
                return
            if check:
                check(parsed)

    def join(self, channel):
        def check(parsed):
            _, _, command, params = parsed
            if command in JOIN_ERRORS and len(params) > 1 and fold(params[1]) == fold(channel):
                fail(f"cannot join {channel}: {params[-1]}")
        # An attached multiclient session may already be in the channel, so no reply is not an error.
        self.send(f"JOIN {channel}")
        self.sync("joined", check)

    def quit(self):
        try:
            self.send("QUIT :done")
            self.sock.close()
        except OSError:
            pass


def pieces(word, limit):
    """Cut one word, such as a long URL, into parts of at most limit bytes without splitting a character."""
    parts, current = [], ""
    for character in word:
        if len((current + character).encode()) > limit:
            parts.append(current)
            current = character
        else:
            current += character
    return parts + ([current] if current else [])


def split(text, limit):
    chunks, current = [], ""
    for word in text.split():
        for part in pieces(word, limit):
            candidate = f"{current} {part}".strip()
            if len(candidate.encode()) > limit and current:
                chunks.append(current)
                current = part
            else:
                current = candidate
    return chunks + ([current] if current else [])


def multiline_limits(caps):
    if "draft/multiline" not in caps or "batch" not in caps:
        return None
    limits = dict(item.partition("=")[::2] for item in caps["draft/multiline"].split(",") if item)
    try:
        return int(limits.get("max-bytes", 0)), int(limits.get("max-lines", 0) or 100)
    except ValueError:
        return None


def matches(message, irc, channel, threads, senders):
    if fold(message["target"]) != fold(channel):
        return False
    if fold(message["account"] or message["nick"]) in (fold(irc.account), fold(irc.nick)):
        return False  # your own messages, from this session or another
    if senders:
        return fold(message["nick"]) in senders
    return not threads or message["reply"] in threads


def await_reply(irc, channel, threads, senders, deadline, seen):
    """Print the replies that match until the first whole message from someone else, or the deadline."""
    assembler, found = Assembler(), False
    while True:
        parsed = irc.receive(deadline)
        if parsed is None:
            return found
        message = assembler.feed(parsed)
        if not message or not matches(message, irc, channel, threads, senders):
            continue
        if message["msgid"]:
            if message["msgid"] in seen:
                continue
            seen.add(message["msgid"])
        if message["command"] == "NOTICE":
            if message["progress"]:
                print(f"... {message['nick']} is working ({message['progress']}): {message['text']}", flush=True)
            continue
        show(message)
        if not found:
            # Gather the lines an agent sends straight after its first one.
            found, deadline = True, min(deadline, time.monotonic() + 3)


def compose(mode, channel, model, human, message, reply_to):
    """Check a message before connecting: (marker, the lines of each paragraph)."""
    model, human = model.strip(), (human or mode.default_human()).strip()
    if not model or len(model) > 60:
        fail("--as must name your model and version, for example 'Claude Opus 5.5'.")
    if not human or len(human) > 60:
        fail("--for must name the person you act for, in at most 60 characters.")
    if SECRET.search(message):
        fail("the message looks like it contains a key or token. Channel history keeps it and the fleet reads it; "
             "remove it.")
    if reply_to and not MSGID.fullmatch(reply_to):
        fail("--reply-to takes a message ID as history and say print it.")
    prefix = f"[on behalf of {human} ({model})] "
    # Budget against the whole line: "PRIVMSG <channel> :<prefix><text>".
    budget = LINE_BYTES - len(f"PRIVMSG {channel} :{prefix}".encode())
    if budget < 100:
        fail("the channel name and attribution leave too little room for a message.")
    batch = [(index, chunk) for index, paragraph in enumerate(message.splitlines()) if paragraph.strip()
             for chunk in split(paragraph, budget)]
    if not batch:
        fail("nothing to say.")
    return prefix, batch


def say(irc, channel, prefix, batch, reply_to, wait):
    if reply_to and "message-tags" not in irc.caps:
        fail("this IRC server does not carry replies (message-tags), so there are no threads.")
    reply_tag = f"+draft/reply={reply_to}" if reply_to else ""
    limits = multiline_limits(irc.caps)
    text_bytes = len(prefix.encode()) + sum(len(chunk.encode()) + 1 for _, chunk in batch)
    irc.join(channel)
    if limits and len(batch) > 1 and len(batch) <= limits[1] and (not limits[0] or text_bytes <= limits[0]):
        # One message of several lines: the marker starts it, and wrapped lines join without a break.
        irc.send(f"{'@' + reply_tag + ' ' if reply_tag else ''}BATCH +say draft/multiline {channel}")
        previous = None
        for index, (paragraph_index, chunk) in enumerate(batch):
            text = prefix + chunk if index == 0 else chunk
            if paragraph_index == previous:
                irc.send(f"@batch=say;draft/multiline-concat PRIVMSG {channel} : {text}")
            else:
                irc.send(f"@batch=say PRIVMSG {channel} :{text}")
            previous = paragraph_index
        irc.send("BATCH -say")
        sent = 1
    else:
        for _, chunk in batch:
            irc.send(f"{'@' + reply_tag + ' ' if reply_tag else ''}PRIVMSG {channel} :{prefix}{chunk}")
        sent = len(batch)
    first = prefix + batch[0][1]
    assembler, echoed, later = Assembler(), {}, []

    def check(parsed):
        _, _, command, params = parsed
        # Ergo answers a refused message with an error numeric (for example 404 or 417) or FAIL.
        if command == "FAIL" or (command.isdigit() and 400 <= int(command) < 600):
            irc.quit()
            fail("the IRC server refused the message: " + " ".join([command, *params]))
        if echoed:
            later.append(parsed)  # a quick answer, kept for --wait
            return
        message = assembler.feed(parsed)
        if (message and fold(message["target"]) == fold(channel)
                and fold(message["nick"]) == fold(irc.nick) and message["text"].startswith(first)):
            echoed.update(message)

    irc.sync("sent", check)
    irc.pending = later + irc.pending
    msgid = echoed.get("msgid", "")
    noun = "message" if sent == 1 else f"{sent} lines"
    print(f"sent {noun} to {channel} as {irc.account}, marked {prefix.strip()}"
          + (f" [id {msgid}]" if msgid else ""), flush=True)
    if wait:
        # An agent may answer a reply in a thread under the thread's first message.
        threads = {msgid, reply_to} - {"", None} if msgid else set()
        print(f"waiting up to {wait} seconds for a reply"
              + (" in its thread" if threads else " in the channel") + "...", flush=True)
        if not await_reply(irc, channel, threads, set(), time.monotonic() + wait, {msgid} if msgid else set()):
            print("no reply yet. The message was sent; do not send it again."
                  + (f" Keep waiting with: wait '{channel}' --thread {msgid}" if msgid else ""), flush=True)
    irc.quit()


def history(irc, channel, limit, thread):
    if not ({"draft/chathistory", "chathistory"} & set(irc.caps) and "batch" in irc.caps):
        fail("this IRC server keeps no history this helper can read.")
    # Ergo serves history only to members of the channel.
    irc.join(channel)
    irc.send(f"CHATHISTORY LATEST {channel} * {limit}")
    assembler, reference, messages = Assembler(), None, []
    while True:
        parsed = irc.receive_or_fail()
        tags, nick, command, params = parsed
        if command == "BATCH" and params and reference is None and params[0].startswith("+") \
                and params[1:2] == ["chathistory"] and fold(params[2] if len(params) > 2 else "") == fold(channel):
            reference = params[0][1:]
            continue
        if command == "BATCH" and reference and params[:1] == ["-" + reference]:
            break
        if command == "FAIL" or command in ("403", "442"):
            fail(f"cannot read {channel}: {' '.join([command, *params])}")
        message = assembler.feed(parsed)
        if message and message["command"] == "PRIVMSG" and message["nick"] != "HistServ":  # Ergo's join and quit notices
            messages.append(message)
    irc.quit()
    if thread:
        messages = [message for message in messages if thread in (message["msgid"], message["reply"])]
    for message in messages:
        show(message)


def wait_command(irc, channel, threads, senders, timeout):
    if not all(MSGID.fullmatch(thread) for thread in threads):
        fail("--thread takes a message ID as history and say print it.")
    if not threads and not senders:
        fail("pass --thread with the ID that say printed, or --from with an agent's nick.")
    irc.join(channel)
    anchor, seen, deadline, threads = threads[:1], set(), time.monotonic() + timeout, set(threads)
    if anchor and {"draft/chathistory", "chathistory"} & set(irc.caps):
        # Replies may have arrived while nobody was listening.
        irc.send(f"CHATHISTORY AFTER {channel} msgid={anchor[0]} 100")
        assembler, reference, found = Assembler(), None, False
        while True:
            parsed = irc.receive_or_fail()
            command, params = parsed[2], parsed[3]
            if command == "BATCH" and params and reference is None and params[0].startswith("+") \
                    and params[1:2] == ["chathistory"]:
                reference = params[0][1:]
                continue
            if command == "FAIL" or (command == "BATCH" and reference and params[:1] == ["-" + reference]):
                break
            message = assembler.feed(parsed)
            if message and message["command"] == "PRIVMSG" and matches(message, irc, channel, threads, senders):
                seen.add(message["msgid"])
                show(message)
                found = True
        if found:
            irc.quit()
            return
    if not await_reply(irc, channel, threads, senders, deadline, seen):
        irc.quit()
        sys.exit(f"subspace: no reply within {timeout} seconds. Run wait again to keep waiting.")
    irc.quit()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--target", help="development sign-in: the installed fleet to use")
    parser.add_argument("--fleet", help="hosted API: the fleet whose IRC server to use, when a channel is ambiguous")
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
    listen.add_argument("--from", dest="senders", action="append", default=[], metavar="NICK")
    listen.add_argument("--timeout", type=int, default=240, metavar="SECONDS")
    args = parser.parse_args()
    mode = hosted_or_development(args.target)
    if args.command == "channels":
        mode.list_channels()
        return
    if not args.channel.startswith("#"):
        fail("name a channel (#name). Direct messages from your account are not supported here.")
    if args.command == "say":
        prefix, batch = compose(mode, args.channel, args.model, args.human, " ".join(args.message), args.reply_to)
    server = mode.server_for(args.channel, args.fleet)
    try:
        irc = Irc(mode, server)
        if args.command == "history":
            history(irc, args.channel, max(1, min(args.limit, 200 if not args.thread else 500)), args.thread)
        elif args.command == "say":
            say(irc, args.channel, prefix, batch, args.reply_to, max(0, min(args.wait, 3600)))
        else:
            wait_command(irc, args.channel, args.threads, {fold(nick) for nick in args.senders},
                         max(1, min(args.timeout, 3600)))
    except ssl.SSLCertVerificationError as error:
        fail(f"the IRC server's certificate could not be verified ({error.verify_message}).")
    except (OSError, ssl.SSLError) as error:
        fail(f"IRC connection to {server['host']}:{server['port']} failed ({error}). {mode.hint}")


if __name__ == "__main__":
    main()
