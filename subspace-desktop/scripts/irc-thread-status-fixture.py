#!/usr/bin/env python3
"""Play a fictional agent conversation for the Channels thread status capture.

Usage, with a fresh local Ergo server from scripts/irc-test-server.sh running:

    python3 scripts/irc-thread-status-fixture.py --config <file> &

It plays requests to the fictional ship `astra` in `#thread-status`, then,
once the server has answered, writes a Channels configuration for
CAPTURE_IRC_CONFIG to <file> (a fresh owner-only file renamed onto it;
a symbolic link there is refused) and prints `ready`. `astra` and a second fictional client, `nova`, register
throwaway accounts on the local server, so their messages carry the
server's `account` tag:

1. `subspace` (the nick the configuration gives the capture build) sends
   three requests, the third inside the first one's thread.
2. `astra` answers each with `+stargate/progress` notices threaded to that
   request's own message ID: the first is using a tool, with one ordinary
   untagged notice; the third is queued; the second asks a question, with
   answer instructions, which `subspace` answers before leaving, so the
   capture build can take its nick and the requests read as its own.
3. `nova`, another account, sends a lookalike tagged `completed` notice on
   the first request, which must stay an ordinary notice.
4. When `subspace` joins again (the capture build), `astra` reports after
   --stale-after seconds that its event stream was lost on the first
   request; after --final-after seconds it starts the second request's reply
   and sends it to the main room without a reply tag, which ends nothing;
   after --finish-after seconds it marks the second request `completed`. The
   first and third stay in progress.

Only 127.0.0.1 is accepted; nothing here reaches another server. This is
disposable fixture evidence, not evidence from a live fleet.
"""

import argparse
import json
import os
import re
import secrets
import socket
import sys
import tempfile
import time
from datetime import datetime, timezone

TAG = "+stargate/progress"
CAPS = ["message-tags", "echo-message", "server-time", "batch"]
RECEIVED = "Stargate received your request. Waiting for backend activity."


class FixtureError(Exception):
    """The server refused something or did not answer in time."""


def parse_line(raw):
    """Tags, source nick, command and parameters of one IRC line."""
    tags, nick, rest = {}, "", raw
    if rest.startswith("@"):
        tagtext, rest = rest[1:].split(" ", 1)
        for tag in tagtext.split(";"):
            key, _, value = tag.partition("=")
            tags[key] = value
    if rest.startswith(":"):
        source, rest = rest[1:].split(" ", 1)
        nick = source.split("!", 1)[0]
    head, sep, tail = rest.partition(" :")
    words = head.split()
    return tags, nick, words[0].upper(), words[1:] + ([tail] if sep else [])


def tag_text(tags):
    """`@key=value;… ` for the given client tags, or nothing."""
    escapes = {"\\": "\\\\", ";": "\\:", " ": "\\s", "\r": "\\r", "\n": "\\n"}
    escape = lambda value: "".join(escapes.get(c, c) for c in value)  # noqa: E731
    pairs = [f"{key}={escape(value)}" for key, value in tags.items()]
    return "@" + ";".join(pairs) + " " if pairs else ""


class Client:
    def __init__(self, host, port, nick):
        self.nick = nick
        self.buffer = b""
        self.sock = socket.create_connection((host, port), timeout=15)

    def send(self, line):
        if any(c in line for c in "\r\n\0") or len(line.encode()) > 4600:
            raise FixtureError("unsafe or oversized line")
        self.sock.sendall((line + "\r\n").encode())

    def read(self, deadline):
        while b"\r\n" not in self.buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            self.sock.settimeout(remaining)
            try:
                chunk = self.sock.recv(65536)
            except socket.timeout:
                return None
            if not chunk:
                raise FixtureError(f"{self.nick}: the server closed the connection")
            self.buffer += chunk
        raw, self.buffer = self.buffer.split(b"\r\n", 1)
        message = parse_line(raw.decode("utf-8", errors="replace"))
        if message[2] == "PING":
            self.send("PONG :" + message[3][-1])
        if message[2] in {"ERROR", "FAIL", "433", "432", "404"}:
            raise FixtureError(f"{self.nick}: {raw.decode(errors='replace')}")
        return message

    def until(self, predicate, seconds=30):
        deadline = time.monotonic() + seconds
        while True:
            message = self.read(deadline)
            if message is None:
                raise FixtureError(f"{self.nick}: no answer in {seconds} s")
            if predicate(message):
                return message

    def idle(self, seconds, watch=None):
        """Answer PINGs for `seconds`; True as soon as `watch` matches."""
        deadline = time.monotonic() + seconds
        while (message := self.read(deadline)) is not None:
            if watch and watch(message):
                return True
        return False

    def register(self):
        self.send("CAP LS 302")
        self.send("NICK " + self.nick)
        self.send("USER fixture 0 * :Thread status fixture")
        self.send("CAP REQ :" + " ".join(CAPS))
        self.until(lambda m: m[2] == "CAP" and m[3][1] in {"ACK", "NAK"})
        self.send("CAP END")
        self.until(lambda m: m[2] == "001")

    def log_in(self):
        """Registers a throwaway account named after the nick and waits until
        the server says it is logged in, so its lines carry `account`. Ergo
        2.19.1 answers NickServ's REGISTER with notices rather than 900, so
        a WHOIS of our own nick confirms it with 330 (logged in as)."""
        self.send(f"PRIVMSG NickServ :REGISTER {secrets.token_hex(16)}")
        self.send(f"WHOIS {self.nick}")
        try:
            self.until(lambda m: m[2] == "900" or (m[2] == "330" and m[3][1:2] == [self.nick]))
        except FixtureError as error:
            raise FixtureError(f"{self.nick} was not logged in; is account registration on? ({error})")

    def join(self, channel):
        self.send("JOIN " + channel)
        self.until(lambda m: m[2] == "366")

    def say(self, command, channel, text, tags=None):
        """Send one line and return its server `msgid` from the echo."""
        self.send(f"{tag_text(tags or {})}{command} {channel} :{text}")
        echo = self.until(lambda m: m[1] == self.nick and m[2] == command and m[3][-1] == text)
        time.sleep(0.05)
        return echo[0].get("msgid", "")

    def quit(self):
        try:
            self.send("QUIT :fixture done")
        except OSError:
            pass
        self.sock.close()


def observed(text):
    return f"{text} Observed at {datetime.now(timezone.utc):%H:%M} UTC."


def progress(client, channel, request, stage, text):
    """A progress notice threaded to `request`'s own message ID."""
    client.say("NOTICE", channel, observed(text), {"+draft/reply": request, TAG: stage})


def reply(client, command, channel, request, text):
    client.say(command, channel, text, {"+draft/reply": request})


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
    # Never through a symbolic link: the config goes only where it was asked to.
    if os.path.islink(path):
        raise FixtureError(f"--config must not be a symbolic link: {path}")
    # A fresh owner-only file beside the target, renamed onto it, so an
    # existing file's mode is never kept.
    descriptor, fresh = tempfile.mkstemp(dir=os.path.dirname(os.path.abspath(path)), prefix=".irc-thread-status-")
    try:
        with os.fdopen(descriptor, "w") as file:
            json.dump(config, file, indent=2)
        os.chmod(fresh, 0o600)
        os.replace(fresh, path)
    except BaseException:
        os.unlink(fresh)
        raise


def connect(host, port, nick, channel, account=False):
    client = Client(host, port, nick)
    client.register()
    if account:
        client.log_in()
    client.join(channel)
    return client


def requests(requester, agent, channel):
    """The three requests; the second's question, asked and answered."""
    first = requester.say("PRIVMSG", channel, "astra: summarise the open pull requests for the release")
    second = requester.say("PRIVMSG", channel, "astra: check whether last night's build passed")
    third = requester.say(
        "PRIVMSG", channel, "astra: also list who reviewed each one", {"+draft/reply": first}
    )
    if not all([first, second, third]):
        raise FixtureError("the server sent no msgid; message-tags is required")
    for request in (first, second, third):
        progress(agent, channel, request, "received", RECEIVED)
    progress(agent, channel, second, "reasoning", "OpenCode reported model reasoning.")
    reply(agent, "PRIVMSG", channel, second, "Which branch should I check? 1. main 2. release")
    reply(
        agent,
        "PRIVMSG",
        channel,
        second,
        "Only the original requester may reply in this thread: astra: answer Q7 <choice number>",
    )
    progress(agent, channel, second, "waiting", "Waiting for a clarification answer.")
    reply(requester, "PRIVMSG", channel, second, "astra: answer Q7 1")
    return first, second, third


def before_capture(agent, nova, channel, ids):
    first, second, third = ids
    progress(agent, channel, second, "clarified", "OpenCode closed the clarification.")
    progress(agent, channel, first, "delivered", "OpenCode delivered the request to the session.")
    reply(agent, "NOTICE", channel, first, "Note: the release dashboard is slow today.")
    progress(agent, channel, first, "tool", "OpenCode started a tool call.")
    progress(agent, channel, third, "queued", "OpenCode queued the request. Delivery to the model is unconfirmed.")
    progress(nova, channel, first, "completed", "OpenCode finished this request.")


def after_join(agent, channel, ids, args):
    first, second, _ = ids
    agent.idle(args.stale_after)
    progress(
        agent,
        channel,
        first,
        "tool,disconnected",
        "OpenCode event stream disconnected. OpenCode started a tool call.",
    )
    print("thread-status fixture: first request stale", flush=True)
    agent.idle(max(0, args.final_after - args.stale_after))
    progress(agent, channel, second, "reply", "OpenCode started writing a reply.")
    agent.say("PRIVMSG", channel, "Last night's build on main passed.")
    print("thread-status fixture: main-room final reply sent", flush=True)
    agent.idle(max(0, args.finish_after - args.final_after))
    progress(agent, channel, second, "completed", "OpenCode finished this request.")
    print("thread-status fixture: second request completed", flush=True)
    agent.idle(args.hold)


def loopback(address):
    match = re.fullmatch(r"127\.0\.0\.1:(\d+)", address)
    if not match:
        raise FixtureError(f"--server must be 127.0.0.1:<port>, not {address}")
    return "127.0.0.1", int(match.group(1))


def arguments():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--config", required=True, help="where to write the Channels config for CAPTURE_IRC_CONFIG")
    parser.add_argument("--server", default=os.environ.get("IRC_TEST_LISTEN", "127.0.0.1:16667"))
    parser.add_argument("--channel", default="#thread-status")
    parser.add_argument("--nick", default="subspace", help="the capture build's nick, which sends the requests")
    parser.add_argument("--stale-after", type=float, default=15, help="seconds after the capture joins")
    parser.add_argument("--final-after", type=float, default=30, help="seconds after the capture joins")
    parser.add_argument("--finish-after", type=float, default=45, help="seconds after the capture joins")
    parser.add_argument("--hold", type=float, default=60, help="seconds to stay after finishing")
    parser.add_argument("--wait", type=float, default=1800, help="seconds to wait for the capture to join")
    return parser.parse_args()


def main():
    args = arguments()
    clients = []
    try:
        host, port = loopback(args.server)
        agent = connect(host, port, "astra", args.channel, account=True)
        clients.append(agent)
        agent.send(f"TOPIC {args.channel} :Fictional requests to the fixture ship astra")
        nova = connect(host, port, "nova", args.channel, account=True)
        clients.append(nova)
        requester = connect(host, port, args.nick, args.channel)
        try:
            ids = requests(requester, agent, args.channel)
        finally:
            requester.quit()
        before_capture(agent, nova, args.channel, ids)
        # Only once the server has answered, so a failed run leaves no config.
        write_config(args.config, host, port, args.nick, args.channel)
        print(f"thread-status fixture: ready; start the capture with CAPTURE_IRC_CONFIG={args.config}", flush=True)
        joined = lambda m: m[2] == "JOIN" and m[1] == args.nick  # noqa: E731
        if not agent.idle(args.wait, joined):
            raise FixtureError(f"{args.nick} did not join within {args.wait} s")
        after_join(agent, args.channel, ids, args)
        return 0
    except (FixtureError, OSError, ValueError) as error:
        print(f"thread-status fixture: {error}", file=sys.stderr)
        return 1
    finally:
        for client in clients:
            client.quit()


if __name__ == "__main__":
    sys.exit(main())
