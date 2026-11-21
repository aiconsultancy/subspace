#!/usr/bin/env python3
"""Keep fictional agents with status lines, and a person typing, in a
channel for the Channels agent status capture.

Usage, with a fresh local Ergo server from scripts/irc-test-server.sh running:

    python3 scripts/irc-agent-status-fixture.py --config <file> &

It writes a Channels configuration for CAPTURE_IRC_CONFIG to <file>, whose
nick (`subspace` unless --nick says otherwise) the capture build uses. Then
it connects three fictional clients to `#agent-status` on the local server
and keeps them there until it is stopped:

1. `alice`, a person, joins first, so she operates the channel.
2. `galactica` and `discovery`, two agents, request `draft/metadata-2` with
   `batch` and set their nick-wide `status` with `METADATA * SET status`.
3. alice, galactica and discovery post a few ordinary messages, which the
   capture build reads back from the server's history when it joins.
4. alice, and galactica too, send a `+typing=active` TAGMSG every few
   seconds, so whenever the capture looks, alice is typing beside the
   agents' statuses. galactica has a status, so its typing must not show.

The statuses stay set while the agents are connected; stopping the fixture
disconnects them, which clears them. Only 127.0.0.1 is accepted; nothing
here reaches another server. This is disposable fixture evidence, not
evidence from a live fleet.
"""

import argparse
import json
import os
import re
import select
import signal
import socket
import sys
import time

PERSON_CAPS = ["message-tags", "server-time", "batch"]
AGENT_CAPS = ["draft/metadata-2", "batch", "message-tags", "server-time"]
FIXTURE_NICKS = {"alice", "galactica", "discovery"}
# Reported as an error while connecting, joining or setting a status.
REFUSALS = {"ERROR", "FAIL", "403", "404", "432", "433", "473", "474", "475"}


class FixtureError(Exception):
    """The server refused something, went away or did not answer in time."""


def parse_line(raw):
    """The source nick, command and parameters of one IRC line, tags
    dropped."""
    rest = raw
    if rest.startswith("@"):
        rest = rest.split(" ", 1)[1] if " " in rest else ""
    nick = ""
    if rest.startswith(":"):
        source, _, rest = rest[1:].partition(" ")
        nick = source.split("!", 1)[0]
    head, sep, trailing = rest.partition(" :")
    words = head.split()
    if not words:
        return nick, "", []
    return nick, words[0].upper(), words[1:] + ([trailing] if sep else [])


class Client:
    """One fixture connection, answering PINGs whenever it reads."""

    def __init__(self, host, port, nick):
        self.nick = nick
        self.buffer = b""
        self.sock = socket.create_connection((host, port), timeout=15)

    def send(self, line):
        if any(c in line for c in "\r\n\0") or len(line.encode()) > 4000:
            raise FixtureError(f"{self.nick}: unsafe or oversized line")
        self.sock.sendall((line + "\r\n").encode())

    def receive(self):
        """Reads what has arrived; the server closing the connection is an
        error."""
        chunk = self.sock.recv(65536)
        if not chunk:
            raise FixtureError(f"{self.nick}: the server closed the connection")
        self.buffer += chunk

    def take(self):
        """The complete lines read so far, PINGs answered."""
        messages = []
        while b"\r\n" in self.buffer:
            raw, self.buffer = self.buffer.split(b"\r\n", 1)
            message = parse_line(raw.decode("utf-8", errors="replace"))
            if message[1] == "PING":
                self.send("PONG :" + (message[2][-1] if message[2] else ""))
            elif message[1] == "ERROR":
                raise FixtureError(f"{self.nick}: {raw.decode(errors='replace')}")
            messages.append((message, raw))
        return messages

    def until(self, predicate, seconds=30):
        """The first line that matches `predicate`; a refusal before it is
        an error."""
        deadline = time.monotonic() + seconds
        while True:
            for message, raw in self.take():
                if predicate(message):
                    return message
                if message[1] in REFUSALS:
                    raise FixtureError(f"{self.nick}: {raw.decode(errors='replace')}")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise FixtureError(f"{self.nick}: no answer in {seconds} s")
            self.sock.settimeout(remaining)
            try:
                self.receive()
            except socket.timeout:
                continue

    def register(self, caps, realname):
        self.send("CAP LS 302")
        self.send("NICK " + self.nick)
        self.send(f"USER fixture 0 * :{realname}")
        self.send("CAP REQ :" + " ".join(caps))
        reply = self.until(lambda m: m[1] == "CAP" and len(m[2]) > 1 and m[2][1] in {"ACK", "NAK"})
        if reply[2][1] == "NAK":
            raise FixtureError(f"{self.nick}: the server refused {' '.join(caps)}")
        self.send("CAP END")
        self.until(lambda m: m[1] == "001")

    def join(self, channel):
        self.send("JOIN " + channel)
        self.until(lambda m: m[1] == "366")

    def set_status(self, text):
        """Sets this nick's `status` metadata and waits for the server's
        761 (RPL_KEYVALUE) for it."""
        self.send("METADATA * SET status :" + text)
        self.until(lambda m: m[1] == "761" and "status" in m[2])

    def say(self, channel, text):
        self.send(f"PRIVMSG {channel} :{text}")
        # Keeps the lines in order across connections.
        time.sleep(0.2)

    def typing(self, channel):
        self.send(f"@+typing=active TAGMSG {channel}")

    def quit(self):
        try:
            self.send("QUIT :fixture done")
        except (OSError, FixtureError):
            pass
        self.sock.close()


def write_config(path, host, port, nick, channel):
    config = {
        "networks": [
            {
                "id": "local",
                "name": "Local Ergo",
                "host": host,
                "port": port,
                "tls": False,
                "nickname": nick,
                "channels": [channel],
            }
        ],
        "highlights": [],
    }
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w") as file:
        json.dump(config, file, indent=2)


def connect(host, port, nick, channel, caps):
    client = Client(host, port, nick)
    client.register(caps, "Agent status fixture")
    client.join(channel)
    return client


def converse(alice, galactica, discovery, channel):
    """A few ordinary messages, which say nothing about the statuses."""
    alice.say(channel, "Morning! Could someone look over the release notes before lunch?")
    galactica.say(channel, "I'll take the release notes.")
    discovery.say(channel, "And I'll go through the open pull requests.")
    alice.say(channel, "Thanks, both.")


def hold(clients, typists, channel, every):
    """Answers PINGs and sends the typing notices until stopped."""
    due = 0.0
    while True:
        if time.monotonic() >= due:
            for typist in typists:
                typist.typing(channel)
            due = time.monotonic() + every
        wait = max(0.0, due - time.monotonic())
        readable, _, _ = select.select([client.sock for client in clients], [], [], wait)
        for client in clients:
            if client.sock in readable:
                client.receive()
                client.take()


def loopback(address):
    match = re.fullmatch(r"127\.0\.0\.1:(\d+)", address)
    if not match:
        raise FixtureError(f"--server must be 127.0.0.1:<port>, not {address}")
    return "127.0.0.1", int(match.group(1))


def arguments():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--config", required=True, help="where to write the Channels config for CAPTURE_IRC_CONFIG")
    parser.add_argument("--server", default=os.environ.get("IRC_TEST_LISTEN", "127.0.0.1:16667"))
    parser.add_argument("--channel", default="#agent-status")
    parser.add_argument("--nick", default="subspace", help="the capture build's nick, written to the config")
    parser.add_argument("--galactica-status", default="Galactica is thinking…")
    parser.add_argument("--discovery-status", default="Discovery is noodling on it…")
    parser.add_argument("--typing-every", type=float, default=3, help="seconds between typing notices")
    return parser.parse_args()


def run(args, clients):
    host, port = loopback(args.server)
    if args.nick.lower() in FIXTURE_NICKS:
        raise FixtureError(f"--nick must not be one of the fixture's nicks: {args.nick}")
    write_config(args.config, host, port, args.nick, args.channel)
    alice = connect(host, port, "alice", args.channel, PERSON_CAPS)
    clients.append(alice)
    alice.send(f"TOPIC {args.channel} :Fictional agents at work, for the status capture")
    galactica = connect(host, port, "galactica", args.channel, AGENT_CAPS)
    clients.append(galactica)
    discovery = connect(host, port, "discovery", args.channel, AGENT_CAPS)
    clients.append(discovery)
    converse(alice, galactica, discovery, args.channel)
    galactica.set_status(args.galactica_status)
    discovery.set_status(args.discovery_status)
    print(
        f"agent-status fixture: ready; start the capture with CAPTURE_IRC_CONFIG={args.config}",
        flush=True,
    )
    hold(clients, [alice, galactica], args.channel, args.typing_every)


def main():
    args = arguments()
    # Stopping the fixture with kill still says goodbye to the server.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    clients = []
    try:
        run(args, clients)
        return 0
    except KeyboardInterrupt:
        return 0
    except (FixtureError, OSError, ValueError) as error:
        print(f"agent-status fixture: {error}", file=sys.stderr)
        return 1
    finally:
        for client in clients:
            client.quit()


if __name__ == "__main__":
    sys.exit(main())
