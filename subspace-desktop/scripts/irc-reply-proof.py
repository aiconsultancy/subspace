#!/usr/bin/env python3
"""Prove reply-tag replay without client code or database access.

Usage: python3 scripts/irc-reply-proof.py '#fixture-channel' 'fixture text'
Required environment: IRC_PROOF_SERVER, IRC_PROOF_NICK, and exactly one of:
  IRC_PROOF_CLIENT_PEM: combined PEM certificate and key, for SASL EXTERNAL;
  IRC_PROOF_PASSWORD and IRC_PROOF_ACCOUNT: for SASL PLAIN.
Optional: IRC_PROOF_PORT (default 6697). EXTERNAL infers the account from the
certificate; IRC_PROOF_ACCOUNT is not used for its empty authorization identity.
TLS certificates are verified against the system roots (SSL_CERT_FILE supported).
Credentials are never accepted as arguments. Use a dedicated fixture channel
whose history policy you know: PostgreSQL persistence requires a registered
channel with history enabled. Two raw connections use the same account, so
Ergo's account multiclient setting must be enabled. This writes three messages
and leaves them there. A sends the parent; B sends the reply; A receives it.
PASS proves replay after a new connection, not survival of a server restart.
The exact three channel message texts are previewed before any connection.
Confirm with "yes", or pass --yes to skip the prompt (the preview still prints).

For scripts/irc-test-server.sh only, --local-plaintext permits unauthenticated
loopback connections (default port 16667); B uses a derived fixture nick when
unauthenticated. B stays joined during A's reconnect so in-memory history can
survive. If account and password are supplied locally, SASL is exercised too.
"""

import argparse
import base64
from dataclasses import dataclass, field, replace
import ipaddress
import os
import re
import socket
import ssl
import sys
import time
import uuid


MESSAGE_PREFIX = "[reply-tag proof, please ignore]"


class ProofError(Exception):
    """A protocol assertion or configuration requirement failed."""


@dataclass
class Settings:
    server: str
    port: int
    nick: str
    account: str
    password: str = field(repr=False)
    plaintext: bool = False
    client_pem: str = field(default="", repr=False)


def settings_from_env(plaintext):
    server = os.environ.get("IRC_PROOF_SERVER", "")
    nick = os.environ.get("IRC_PROOF_NICK", "")
    account = os.environ.get("IRC_PROOF_ACCOUNT", "")
    password = os.environ.get("IRC_PROOF_PASSWORD", "")
    client_pem = os.environ.get("IRC_PROOF_CLIENT_PEM", "")
    if not server or not re.fullmatch(r"[A-Za-z_\[\]\\`^{}|][A-Za-z0-9_\[\]\\`^{}|\-]*", nick):
        raise ProofError("set IRC_PROOF_SERVER and a valid IRC_PROOF_NICK")
    if plaintext and not ipaddress.ip_address(server).is_loopback:
        raise ProofError("--local-plaintext requires a literal loopback address")
    if client_pem and (password or plaintext):
        raise ProofError("client PEM requires TLS and cannot be combined with a password")
    if not plaintext and not (client_pem or password):
        raise ProofError("TLS runs require exactly one of client PEM or password")
    if not client_pem and bool(account) != bool(password):
        raise ProofError("SASL PLAIN requires both account and password")
    if any(c in account + password + client_pem for c in "\x00\r\n"):
        raise ProofError("invalid or incomplete SASL credentials")
    if client_pem and not os.path.isfile(client_pem):
        raise ProofError("client PEM must be an existing file")
    port = int(os.environ.get("IRC_PROOF_PORT", "16667" if plaintext else "6697"))
    return Settings(server, port, nick, account, password, plaintext, client_pem)


def tls_context(settings):
    context = ssl.create_default_context()
    if settings.client_pem:
        try:
            context.load_cert_chain(settings.client_pem, password=lambda: "")
        except (OSError, ValueError):
            raise ProofError("unable to load client certificate and key") from None
    return context


def parse_line(raw):
    tags = {}
    rest = raw
    if rest.startswith("@"):
        tagtext, rest = rest[1:].split(" ", 1)
        escapes = {":": ";", "s": " ", "\\": "\\", "r": "\r", "n": "\n"}
        for tag in tagtext.split(";"):
            key, _, value = tag.partition("=")
            tags[key] = re.sub(r"\\(.)", lambda m: escapes.get(m[1], m[1]), value)
    if rest.startswith(":"):
        _, rest = rest.split(" ", 1)
    head, sep, tail = rest.partition(" :")
    words = head.split()
    return tags, words[0], words[1:] + ([tail] if sep else [])


def escape_tag(value):
    return value.replace("\\", "\\\\").replace(";", "\\:").replace(" ", "\\s").replace("\r", "\\r").replace("\n", "\\n")


class Connection:
    def __init__(self, settings, marker):
        self.settings = settings
        self.marker = marker
        self.buffer = b""
        self.sock = None
        self.caps = set()
        self.secrets = [value for value in (settings.password, settings.client_pem) if value]
        self.label = ""

    def log(self, direction, raw):
        for secret in self.secrets:
            raw = raw.replace(secret, "<redacted>")
        raw = re.sub(r"(?i)((?:token|password|secret|authorization)[^ =;]*=)[^; ]+", r"\1<redacted>", raw)
        print(f"{self.label}{direction} {raw}", flush=True)

    def send(self, raw, sensitive=False):
        if any(c in raw for c in "\r\n\x00") or len(raw.encode()) > 510:
            raise ProofError("unsafe or oversized outgoing IRC line")
        if not sensitive:
            self.log(">>", raw)
        self.sock.sendall((raw + "\r\n").encode())

    def read(self, deadline):
        while b"\r\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise ProofError("timed out waiting for IRC response")
            self.sock.settimeout(remaining)
            chunk = self.sock.recv(65536)
            if not chunk:
                raise ProofError("server closed the connection")
            self.buffer += chunk
            if len(self.buffer) > 1048576:
                raise ProofError("oversized IRC receive buffer")
        raw, self.buffer = self.buffer.split(b"\r\n", 1)
        line = raw.decode("utf-8", errors="replace")
        tags, command, params = parse_line(line)
        if command == "PING":
            self.send("PONG :" + params[-1], sensitive=True)
        relevant = command in {"CAP", "BATCH", "FAIL", "ERROR", "CHATHISTORY", "432", "433", "464", "465", "903", "904", "905", "906", "907", "908"}
        if relevant or (command == "PRIVMSG" and self.marker in params[-1]):
            self.log("<<", line)
        return tags, command, params

    def until(self, predicate):
        deadline = time.monotonic() + 30
        while True:
            message = self.read(deadline)
            _, command, params = message
            if command in {"ERROR", "FAIL", "432", "433", "464", "465", "904", "905", "906", "907", "908"}:
                raise ProofError("IRC rejected operation (see redacted raw lines)")
            if predicate(message):
                return message

    def connect(self):
        context = None if self.settings.plaintext else tls_context(self.settings)
        self.sock = socket.create_connection((self.settings.server, self.settings.port), timeout=15)
        if context:
            self.sock = context.wrap_socket(self.sock, server_hostname=self.settings.server)
        self.send("CAP LS 302")
        self.send("NICK " + self.settings.nick)
        self.send("USER replyproof 0 * :IRC reply proof")
        offered = {}
        while True:
            _, _, params = self.until(lambda m: m[1] == "CAP" and m[2][1] == "LS")
            offered.update(item.partition("=")[::2] for item in params[-1].split())
            if params[-2] != "*":
                break
        history = "chathistory" if "chathistory" in offered else "draft/chathistory"
        self.caps = {"message-tags", "echo-message", "server-time", "batch", history}
        if self.settings.account or self.settings.client_pem:
            self.caps.add("sasl")
        if "draft/multiline" in offered:
            self.caps.add("draft/multiline")
        if not self.caps <= offered.keys():
            raise ProofError("server lacks required capabilities")
        self.send("CAP REQ :" + " ".join(sorted(self.caps)))
        _, _, ack = self.until(lambda m: m[1] == "CAP" and m[2][1] in {"ACK", "NAK"})
        if ack[1] != "ACK" or not self.caps <= set(ack[-1].split()):
            raise ProofError("server did not acknowledge all capabilities")
        if self.settings.account or self.settings.client_pem:
            self.authenticate()
        self.send("CAP END")
        self.until(lambda m: m[1] == "001")

    def authenticate(self):
        if self.settings.client_pem:
            self.send("AUTHENTICATE EXTERNAL", sensitive=True)
            self.until(lambda m: m[1] == "AUTHENTICATE" and m[2] == ["+"])
            self.send("AUTHENTICATE +", sensitive=True)
            self.until(lambda m: m[1] == "903")
            return
        self.send("AUTHENTICATE PLAIN", sensitive=True)
        self.until(lambda m: m[1] == "AUTHENTICATE" and m[2] == ["+"])
        payload = base64.b64encode(("\0" + self.settings.account + "\0" + self.settings.password).encode()).decode()
        self.secrets.append(payload)
        for start in range(0, len(payload), 400):
            self.send("AUTHENTICATE " + payload[start:start + 400], sensitive=True)
        if len(payload) % 400 == 0:
            self.send("AUTHENTICATE +", sensitive=True)
        self.until(lambda m: m[1] == "903")

    def join(self, channel):
        self.send("JOIN " + channel)
        self.until(lambda m: m[1] == "366" and m[2][1].lower() == channel.lower())

    def message(self, target, text, reply=None):
        tag = "@+draft/reply=" + escape_tag(reply) + " " if reply else ""
        self.send(tag + "PRIVMSG " + target + " :" + text)
        return self.until(lambda m: m[1] == "PRIVMSG" and m[2] == [target, text])

    def history(self, query):
        self.send("CHATHISTORY " + query)
        opening = self.until(lambda m: m[1] == "BATCH" and m[2][0].startswith("+") and m[2][1] in {"chathistory", "draft/chathistory-targets"})
        batch = opening[2][0][1:]
        messages = []
        deadline = time.monotonic() + 30
        while True:
            message = self.read(deadline)
            if message[1] in {"FAIL", "ERROR"}:
                raise ProofError("history failed")
            if message[1] == "BATCH" and message[2][0] == "-" + batch:
                return messages
            messages.append(message)

    def close(self):
        if self.sock is not None:
            try:
                self.send("QUIT :fixture proof complete")
            except OSError:
                pass
            self.sock.close()
            self.sock = None


def require_message_id(message):
    msgid = message[0].get("msgid")
    if not msgid:
        raise ProofError("echo did not contain msgid")
    return msgid


def require_reply(messages, expected):
    tags, _, params = expected
    if not any(m[1] == "PRIVMSG" and m[2] == params and
               m[0].get("msgid") == tags.get("msgid") and
               m[0].get("+draft/reply") == tags.get("+draft/reply") for m in messages):
        raise ProofError("history did not preserve reply text, msgid and +draft/reply")


def prove(connection, channel, text):
    sender_settings = connection.settings
    if not sender_settings.account and not sender_settings.client_pem:
        sender_settings = replace(sender_settings, nick=sender_settings.nick[:20] + "Reply")
    sender = Connection(sender_settings, text)
    connection.label = "A "
    sender.label = "B "
    try:
        prove_channel(connection, sender, channel, text)
    finally:
        sender.close()


def prove_channel(connection, sender, channel, text):
    connection.connect()
    connection.join(channel)
    sender.connect()
    sender.join(channel)
    parent = connection.message(channel, text + " parent")
    parent_id = require_message_id(parent)
    reply = sender.message(channel, text + " reply", parent_id)
    require_message_id(reply)
    if reply[0].get("+draft/reply") != parent_id:
        raise ProofError("live echo lost +draft/reply")
    received = connection.until(lambda m: m[1] == "PRIVMSG" and m[2] == reply[2])
    require_reply([received], reply)
    end = connection.message(channel, text + " end")
    end_id = require_message_id(end)
    print("PASS A parent msgid, B tagged reply echo, A received matching reply", flush=True)
    connection.close()
    # A new socket repeats TLS, CAP and SASL; no earlier receive state is reused.
    connection.buffer = b""
    connection.connect()
    connection.join(channel)
    require_reply(connection.history(f"LATEST {channel} * 100"), reply)
    print("PASS fresh-connection CHATHISTORY LATEST preserves reply and msgid", flush=True)
    require_reply(connection.history(f"BEFORE {channel} msgid={end_id} 100"), reply)
    print("PASS fresh-connection CHATHISTORY BEFORE msgid anchor preserves reply", flush=True)


def confirm_messages(channel, text, yes):
    print("Channel messages (B's reply also carries +draft/reply=<parent msgid>):", flush=True)
    for sender, suffix in (("A", "parent"), ("B", "reply"), ("A", "end")):
        print(f"{sender}: PRIVMSG {channel} :{text} {suffix}", flush=True)
    if yes:
        return True
    try:
        return input('Send these three messages? Type "yes" to continue: ').strip().lower() == "yes"
    except (EOFError, KeyboardInterrupt):
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("channel", help="dedicated fixture channel, already registered for PostgreSQL proof")
    parser.add_argument("text", help="short fixture text; a unique run marker is appended")
    parser.add_argument("--local-plaintext", action="store_true", help="loopback local test server only")
    parser.add_argument("--yes", action="store_true", help="send the previewed messages without prompting")
    args = parser.parse_args()
    connection = None
    try:
        if not re.fullmatch(r"#[^ ,:\x00-\x20\x7f]+", args.channel):
            raise ProofError("expected one channel beginning with #")
        if not args.text or any(ord(c) < 32 or ord(c) == 127 for c in args.text):
            raise ProofError("fixture text must be a nonempty line without controls")
        text = MESSAGE_PREFIX + " " + args.text + " [reply-proof-" + uuid.uuid4().hex[:12] + "]"
        if len((args.channel + text).encode()) > 300:
            raise ProofError("use shorter fixture channel/text (combined maximum 300 bytes)")
        settings = settings_from_env(args.local_plaintext)
        if not confirm_messages(args.channel, text, args.yes):
            print("Cancelled; no connection made")
            return 1
        connection = Connection(settings, text)
        prove(connection, args.channel, text)
        print("PASS reply-tag history proof")
        return 0
    except (ProofError, OSError, ValueError) as error:
        # Network/library errors may include untrusted text; apply the same redaction.
        if connection:
            connection.log("FAIL", str(error))
        else:
            print("FAIL invalid environment or arguments; check --help")
        return 1
    finally:
        if connection:
            connection.close()


if __name__ == "__main__":
    sys.exit(main())
