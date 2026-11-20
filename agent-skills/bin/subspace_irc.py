#!/usr/bin/env python3
"""Talk to your Subspace fleet on IRC from a local coding agent.

It signs in with YOUR IRC account (the fleet's agents take orders from it), so
every message it sends starts with who is really speaking:

  [on behalf of George (Claude Opus 5.5)] Can you take NAV-12 next?

  subspace_irc.py channels
  subspace_irc.py history '#fleet-starbase' [--limit 30]
  subspace_irc.py say '#fleet-starbase' --as 'Claude Opus 5.5' [--for George] 'message'

--as (your model and version) is required. --for defaults to the name saved
by `./install.sh skills --name`. It reaches IRC through the fleet's tunnel, or
directly for a shared IRC server on your private network.
"""

import argparse
import base64
import os
from pathlib import Path
import socket
import ssl
import sys

HOME = Path(os.environ.get("SUBSPACE_HOME", Path.home() / ".subspace"))
LINE_BYTES = 400


def read_env(path):
    values = {}
    if path.is_file():
        for line in path.read_text().splitlines():
            key, _, value = line.partition("=")
            if key and not key.startswith("#"):
                values[key.strip()] = value.strip()
    return values


def settings(name):
    agents = read_env(HOME / "agents.env")
    name = name or agents.get("SUBSPACE_TARGET") or "subspace-fleet"
    target = HOME / "targets" / name
    summary = read_env(target / "summary.env")
    if not summary:
        sys.exit(f"subspace: no installed fleet '{name}'. Run the installer first.")
    account = summary["SUBSPACE_ADMIN_ACCOUNT"]
    tls = summary.get("SUBSPACE_IRC_TLS") == "1"
    password_file, ca = target / "irc.password", None
    shared = target / "irc-target"
    if summary.get("SUBSPACE_IRC_MODE") == "remote":
        irc_dir = HOME / "targets" / shared.read_text().strip() if shared.is_file() else None
        # Only the shared server this fleet uses now: after a move its saved password and CA no longer apply.
        if not irc_dir or read_env(irc_dir / "summary.env").get("SUBSPACE_IRC_HOST") != summary.get("SUBSPACE_IRC_HOST"):
            sys.exit("subspace: this fleet's IRC server is not managed by the installer, so there is no saved "
                     "password for it here. Use the desktop app for IRC.")
        password_file = irc_dir / f"{account}.password"
        ca = irc_dir / "ca.pem" if (irc_dir / "ca.pem").is_file() else None
    if not password_file.is_file():
        sys.exit(f"subspace: no IRC password file at {password_file}.")
    return {
        "host": summary.get("SUBSPACE_IRC_HOST", "127.0.0.1"), "port": int(summary.get("SUBSPACE_IRC_PORT", 6667)),
        "tls": tls, "ca": ca, "account": account, "password": password_file.read_text().strip(),
        "channels": summary.get("SUBSPACE_CHANNELS", "").split(),
        "human": agents.get("SUBSPACE_HUMAN_NAME") or account,
    }


class Irc:
    def __init__(self, cfg, caps=()):
        raw = socket.create_connection((cfg["host"], cfg["port"]), timeout=20)
        if cfg["tls"]:
            context = ssl.create_default_context(cafile=str(cfg["ca"]) if cfg["ca"] else None)
            raw = context.wrap_socket(raw, server_hostname=cfg["host"])
        self.stream = raw.makefile("rwb")
        account = cfg["account"]
        self.send("CAP REQ :" + " ".join(["sasl", *caps]))
        self.send(f"NICK {account}")
        self.send(f"USER {account} 0 * :{account}")
        while True:
            line, parts = self.read()
            if parts[1:2] == ["CAP"] and "ACK" in parts[2:4]:
                self.send("AUTHENTICATE PLAIN")
            elif line.startswith("AUTHENTICATE +"):
                secret = base64.b64encode(f"{account}\0{account}\0{cfg['password']}".encode()).decode()
                self.send(f"AUTHENTICATE {secret}")
            elif parts[1:2] == ["903"]:
                self.send("CAP END")
            elif parts[1:2] in (["904"], ["905"], ["902"]):
                sys.exit("subspace: IRC sign-in failed for your account.")
            elif parts[1:2] == ["001"]:
                return

    def send(self, line):
        self.stream.write((line + "\r\n").encode())
        self.stream.flush()

    def read(self):
        line = self.stream.readline().decode(errors="replace").rstrip("\r\n")
        if not line:
            sys.exit("subspace: the IRC server closed the connection.")
        if line.startswith("PING"):
            self.send("PONG" + line[4:])
        tagless = line.split(" ", 1)[1] if line.startswith("@") else line
        return line, tagless.split()

    def until(self, numeric):
        while True:
            line, parts = self.read()
            if parts[1:2] == [numeric]:
                return line

    def quit(self):
        self.send("QUIT :done")


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


def say(cfg, channel, model, human, message):
    model, human = model.strip(), (human or cfg["human"]).strip()
    if not model or len(model) > 60:
        sys.exit("subspace: --as must name your model and version, for example 'Claude Opus 5.5'.")
    if not human or len(human) > 60:
        sys.exit("subspace: --for must name the person you act for, in at most 60 characters.")
    prefix = f"[on behalf of {human} ({model})] "
    # Budget against the whole line: "PRIVMSG <channel> :<prefix><text>".
    budget = LINE_BYTES - len(f"PRIVMSG {channel} :{prefix}".encode())
    if budget < 100:
        sys.exit("subspace: the channel name and attribution leave too little room for a message.")
    lines = [line for paragraph in message.splitlines() for line in split(paragraph, budget)]
    if not lines:
        sys.exit("subspace: nothing to say.")
    irc = Irc(cfg)
    irc.send(f"JOIN {channel}")
    irc.until("366")
    for line in lines:
        irc.send(f"PRIVMSG {channel} :{prefix}{line}")
    irc.send("PING :sent")
    while True:
        line, parts = irc.read()
        if parts[1:2] == ["PONG"]:
            break
        # Ergo answers a refused message with an error numeric (for example 404 or 417) or FAIL.
        if parts[1:2] == ["FAIL"] or (len(parts) > 1 and parts[1].isdigit() and 400 <= int(parts[1]) < 600):
            irc.quit()
            sys.exit(f"subspace: the IRC server refused the message: {line}")
    irc.quit()
    print(f"sent {len(lines)} line(s) to {channel} as {cfg['account']}, marked {prefix.strip()}")


def history(cfg, channel, limit):
    irc = Irc(cfg, caps=("server-time", "batch", "message-tags", "draft/chathistory"))
    # Ergo serves history only to members of the channel.
    irc.send(f"JOIN {channel}")
    irc.until("366")
    irc.send(f"CHATHISTORY LATEST {channel} * {limit}")
    while True:
        line, parts = irc.read()
        if parts[1:2] == ["BATCH"] and parts[2].startswith("-"):
            break
        if parts[1:2] == ["FAIL"] or parts[1:2] in (["403"], ["442"]):
            sys.exit(f"subspace: cannot read {channel}: {line}")
        if parts[1:2] == ["PRIVMSG"]:
            tags = dict(tag.partition("=")[::2] for tag in line[1:].split(" ", 1)[0].split(";")) if line.startswith("@") else {}
            stamp = tags.get("time", "")[:16].replace("T", " ")
            nick = parts[0][1:].split("!", 1)[0]
            if nick == "HistServ":
                continue  # Ergo's join and quit notices
            tagless = line.split(" ", 1)[1] if line.startswith("@") else line
            text = tagless.split(" :", 1)[1] if " :" in tagless else ""
            print(f"{stamp} <{nick}> {text}".strip())
    irc.quit()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--target")
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("channels", help="list this fleet's channels")
    read = sub.add_parser("history", help="print recent messages in a channel")
    read.add_argument("channel")
    read.add_argument("--limit", type=int, default=30)
    write = sub.add_parser("say", help="send a message, marked as from you via your agent")
    write.add_argument("channel")
    write.add_argument("message", nargs="+")
    write.add_argument("--as", dest="model", required=True, help="your model and version, for example 'Claude Opus 5.5'")
    write.add_argument("--for", dest="human", help="the person you act for (default: saved name)")
    args = parser.parse_args()
    cfg = settings(args.target)
    if args.command == "channels":
        print("\n".join(cfg["channels"]))
    elif args.command == "history":
        history(cfg, args.channel, max(1, min(args.limit, 200)))
    else:
        if not args.channel.startswith("#"):
            sys.exit("subspace: post to a channel (#name). Direct messages from your account are not supported here.")
        say(cfg, args.channel, args.model, args.human, " ".join(args.message))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ssl.SSLError) as error:
        sys.exit(f"subspace: IRC connection failed ({error}). Is the tunnel running? ./install.sh tunnel --background")
