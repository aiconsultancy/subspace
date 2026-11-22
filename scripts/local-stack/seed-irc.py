"""Seed the local Ergo's persistent history using the supplied SASL accounts."""

import base64
import fcntl
import hashlib
import json
import os
import socket
import ssl
import sys
from contextlib import ExitStack
from pathlib import Path


class Client:
    def __init__(self, scope, nick, password):
        # This runs inside the Ergo container and connects only to its own
        # loopback listener. The local TLS certificate is self-signed.
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
        context.check_hostname = False
        context.verify_mode = ssl.CERT_NONE
        tcp = scope.enter_context(socket.create_connection(("127.0.0.1", 6697), timeout=20))
        self.sock = scope.enter_context(context.wrap_socket(tcp))
        self.reader = scope.enter_context(self.sock.makefile("rb"))
        self.nick = nick
        self.send("CAP REQ :sasl echo-message draft/chathistory server-time batch")
        self.send(f"NICK {nick}")
        self.send(f"USER {nick} 0 * :Fallow Reach rehearsal")
        _, _, args = self.until(lambda _, command, args: command == "CAP" and args[1] == "ACK")
        if "sasl" not in args[-1].split():
            raise RuntimeError("Ergo did not negotiate SASL.")
        self.send("AUTHENTICATE PLAIN")
        self.until(lambda _, command, args: command == "AUTHENTICATE" and args == ["+"])
        credential = base64.b64encode(f"\0{nick}\0{password}".encode()).decode()
        self.send(f"AUTHENTICATE {credential}")
        self.until(lambda _, command, _args: command == "903")
        self.send("CAP END")
        self.until(lambda _, command, _args: command == "001")

    def send(self, line):
        self.sock.sendall(f"{line}\r\n".encode())

    def receive(self):
        raw = self.reader.readline(8192)
        if not raw:
            raise RuntimeError("Ergo closed the connection.")
        line = raw.decode().rstrip("\r\n")
        if line.startswith("@"):
            line = line.partition(" ")[2]
        sender = ""
        if line.startswith(":"):
            prefix, _, line = line.partition(" ")
            sender = prefix[1:].split("!", 1)[0]
        head, separator, body = line.partition(" :")
        command, *args = head.split()
        if separator:
            args.append(body)
        if command == "PING":
            self.send(f"PONG :{args[-1]}")
        protocol_error = command.isdecimal() and 400 <= int(command) < 600
        if protocol_error or command in {"ERROR", "FAIL", "904", "905", "906", "907"}:
            raise RuntimeError(f"Ergo returned {command} to {self.nick}.")
        return sender, command, args

    def until(self, matches):
        for _ in range(3000):
            message = self.receive()
            if matches(*message):
                return message
        raise RuntimeError(f"Ergo did not confirm the request for {self.nick}.")

    def join(self, channel):
        self.send(f"JOIN {channel}")
        self.until(lambda _, command, args: command == "366" and args[1] == channel)

    def history(self, channel):
        self.send(f"CHATHISTORY LATEST {channel} * 1000")
        _, _, args = self.until(lambda _, command, args: command == "BATCH" and args[0].startswith("+"))
        end = "-" + args[0][1:]
        found = []
        while True:
            sender, command, args = self.receive()
            if command == "BATCH" and args[0] == end:
                return found
            if command == "PRIVMSG" and args[0] == channel:
                found.append((sender, args[-1]))

    def post(self, channel, message):
        self.send(f"PRIVMSG {channel} :{message}")
        self.until(
            lambda sender, command, args: sender == self.nick
            and command == "PRIVMSG" and args == [channel, message]
        )


def apply(channels, accounts, journal):
    posted = 0
    with ExitStack() as scope:
        clients = {}
        for channel, lines in channels.items():
            reader = None
            history = set()
            joined = set()
            for nick, message, *previous in lines:
                # Earlier wording still identifies the same fixture on an upgraded stack.
                variants = [message, *previous]
                keys = [hashlib.sha256(f"{channel}\0{nick}\0{text}".encode()).hexdigest() for text in variants]
                if any(key in journal for key in keys):
                    continue
                if nick not in clients:
                    clients[nick] = Client(scope, nick, accounts[nick])
                client = clients[nick]
                if nick not in joined:
                    client.join(channel)
                    joined.add(nick)
                if reader is None:
                    reader = client
                    # Recover a message accepted before a previous seed stopped
                    # or posted by a previous version without a journal entry.
                    history = set(client.history(channel))
                if all((nick, text) not in history for text in variants):
                    client.post(channel, message)
                    posted += 1
                journal[keys[0]] = True
                # Replace atomically, so a cancelled process can resume.
                replacement = Path("/ircd/demo-history.tmp")
                replacement.write_text(json.dumps(journal), encoding="utf-8")
                replacement.replace("/ircd/demo-history.json")
    return posted


def main():
    channels = json.load(sys.stdin)
    # Conversations go only to the channels the stack registers, the
    # standard fleet channels among them, never to a channel JOIN would create.
    registered = {
        row["name"].lower()
        for row in json.loads(Path(os.environ["IRC_CHANNELS_FILE"]).read_text(encoding="utf-8"))
    }
    unknown = sorted(name for name in channels if name.lower() not in registered)
    if unknown:
        raise RuntimeError(f"the demo conversations use unregistered channels: {', '.join(unknown)}")
    accounts = {
        account["name"]: account["password"]
        for account in json.loads(Path(os.environ["IRC_ACCOUNTS_FILE"]).read_text(encoding="utf-8"))
    }
    # The volume survives a container restart and is cleared by stack-reset.
    with Path("/ircd/demo-history.lock").open("a", encoding="utf-8") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = Path("/ircd/demo-history.json")
        journal = json.loads(path.read_text(encoding="utf-8")) if path.exists() else {}
        print(f"Demo IRC history ready: {apply(channels, accounts, journal)} new messages.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError) as error:
        sys.exit(f"local-stack IRC seed: {error}")
