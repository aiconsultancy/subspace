"""Validate and provision channels over Ergo's private bootstrap IRC listener."""

import base64
import json
import re
import socket
import sys
import unicodedata
from collections import defaultdict
from pathlib import Path

# Ergo's CHANMODES group D, which takes no arguments.
FLAGS = "imntEsCRMuU"


class SeedError(Exception):
    pass


def definitions(path, accounts, operator, default_founder):
    channels = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(channels, list):
        raise SeedError("Channel seed must be a JSON array")
    if channels and operator not in accounts:
        raise SeedError("Channel seed requires IRC_OPER_NAME and its password")

    seen = set()
    by_founder = defaultdict(list)
    for index, channel in enumerate(channels, start=1):
        entry = f"Channel seed entry {index}"
        if not isinstance(channel, dict) or channel.keys() - {"name", "topic", "modes", "founder"}:
            raise SeedError(f"{entry} must be an object with name and optional topic, modes and founder")
        name = channel.get("name")
        if not isinstance(name, str) or not re.fullmatch(r"#[!-~]{1,63}", name) or set(name) & set(",*?"):
            raise SeedError(f"{entry} needs a valid ASCII IRC channel name up to 64 characters")
        if name.lower() in seen:
            raise SeedError(f"{entry} repeats a channel name")
        seen.add(name.lower())

        founder = channel.get("founder", default_founder)
        if not isinstance(founder, str) or founder not in accounts:
            raise SeedError(f"{entry} needs a founder from the operator or account seed")
        topic = channel.get("topic", "")
        if not isinstance(topic, str) or len(topic.encode()) > 390 or any(
            unicodedata.category(char) == "Cc" for char in topic
        ):
            raise SeedError(f"{entry} needs a topic of at most 390 UTF-8 bytes without control characters")
        modes = channel.get("modes", "")
        if "modes" in channel and (
            not isinstance(modes, str)
            or not re.fullmatch(rf"(?:[+-][{FLAGS}]+)+", modes)
            or len(set(modes) - {"+", "-"}) != sum(char in FLAGS for char in modes)
        ):
            raise SeedError(f"{entry} needs +/- channel flags ({FLAGS}) with each flag used once")
        by_founder[founder].append(channel)
    return by_founder


class Client:
    def __init__(self, connection, reader):
        self.connection = connection
        self.reader = reader
        self.account = ""
        self.joined = set()

    def send(self, *commands):
        self.connection.sendall(("\r\n".join(commands) + "\r\n").encode())

    def expect(self, commands, action, service=None):
        expected = commands.split()
        for _ in range(100):
            raw = self.reader.readline(8192)
            if not raw:
                break
            line = raw.decode().rstrip("\r\n")
            prefix, _, message = line.partition(" ") if line.startswith(":") else ("", "", line)
            middle, separator, trailing = message.partition(" :")
            command, *params = middle.split()
            if separator:
                params.append(trailing)
            if command == "PING":
                self.send(f"PONG :{params[-1]}")
            if command == "JOIN" and prefix.partition("!")[0].lower() == ":" + self.account.lower():
                self.joined.add(params[0].lower())
            if command == "PART" and prefix.partition("!")[0].lower() == ":" + self.account.lower():
                self.joined.discard(params[0].lower())
            if command in expected and (service is None or prefix.startswith(f":{service}!")):
                return command, params
            if command in ("ERROR", "FAIL") or (
                len(command) == 3 and command.isdecimal() and command[0] in "45"
            ):
                break
        raise SeedError(f"Ergo did not confirm {action}")

    def login(self, founder, password, operator, oper_password):
        self.account = founder
        self.send("CAP REQ :sasl", f"NICK {founder}", f"USER {founder} 0 * :Channel seed")
        _, params = self.expect("CAP", "SASL capability")
        if params != ["*", "ACK", "sasl"]:
            raise SeedError("Ergo did not accept the SASL capability")
        self.send("AUTHENTICATE PLAIN")
        self.expect("AUTHENTICATE", "SASL PLAIN challenge")
        payload = base64.b64encode(f"\0{founder}\0{password}".encode()).decode()
        self.send(f"AUTHENTICATE {payload}")
        self.expect("903", "SASL login")
        self.send("CAP END")
        self.expect("001", "connection registration")
        self.send(f"OPER {operator} :{oper_password}")
        self.expect("381", "operator login")

    def chanserv(self, command):
        self.send(f"PRIVMSG ChanServ :{command}")
        _, params = self.expect("NOTICE", command.split()[0] + " from ChanServ", "ChanServ")
        return params[-1]

    def provision(self, channel, founder):
        name = channel["name"]
        registration = self.chanserv(f"INFO {name}").lower()
        if registration == f"channel {name.lower()} is registered":
            _, params = self.expect("NOTICE", "channel founder from ChanServ", "ChanServ")
            if params[-1] != f"Founder: {founder}":
                raise SeedError("channel is registered to a different founder")
            self.expect("NOTICE", "registration date from ChanServ", "ChanServ")
            registered = True
        elif registration == f"channel {name.lower()} is not registered":
            registered = False
        else:
            raise SeedError("could not check the channel registration")
        if registered and "topic" not in channel and "modes" not in channel:
            return

        already_joined = name.lower() in self.joined
        if not already_joined:
            self.send(f"JOIN {name}")
            self.expect("366", "channel join")
        expected = f"channel {name.lower()} successfully registered"
        if not registered and self.chanserv(f"REGISTER {name}").lower() != expected:
            raise SeedError("ChanServ did not register the channel")
        if "topic" in channel:
            self.set_topic(name, channel["topic"])
        if "modes" in channel:
            self.set_modes(name, channel["modes"])
        if not already_joined:
            self.send(f"PART {name}")
            self.expect("PART", "channel part")

    def set_topic(self, name, topic):
        self.send(f"TOPIC {name}")
        reply, params = self.expect("331 332", "the channel topic")
        current = params[-1] if reply == "332" else ""
        if current != topic:
            self.send(f"TOPIC {name} :{topic}")
            _, params = self.expect("TOPIC", "the new channel topic")
            if params[-1] != topic:
                raise SeedError("Ergo did not apply the requested topic")

    def set_modes(self, name, modes):
        additions, removals = set(), set()
        for char in modes:
            if char in "+-":
                flags = additions if char == "+" else removals
            else:
                flags.add(char)

        self.send(f"MODE {name}")
        _, params = self.expect("324", "the channel modes")
        current = set(params[2])
        add = "".join(sorted(additions - current))
        remove = "".join(sorted(removals & current))
        if add or remove:
            delta = ("+" + add if add else "") + ("-" + remove if remove else "")
            self.send(f"MODE {name} {delta}", f"MODE {name}")
            _, params = self.expect("324", "the new channel modes")
            current = set(params[2])
            if additions - current or removals & current:
                raise SeedError("Ergo did not apply the requested modes")


def main():
    if sys.argv[1] == "certificate":
        _, account_path, operator, name, fingerprint = sys.argv[1:]
        accounts = read_passwords(account_path)
        with socket.create_connection(("127.0.0.1", 16670), timeout=15) as connection, connection.makefile("rb") as reader:
            client = Client(connection, reader)
            client.login(operator, accounts[operator], operator, accounts[operator])
            client.send(f"PRIVMSG NickServ :CERT ADD {name} {fingerprint}")
            client.expect("NOTICE", "certificate registration from NickServ", "NickServ")
        return
    action, path, account_path, operator, default_founder = sys.argv[1:]
    accounts = read_passwords(account_path)
    by_founder = definitions(path, accounts, operator, default_founder)
    if action == "apply":
        for founder, channels in by_founder.items():
            with socket.create_connection(
                ("127.0.0.1", 16670), timeout=15
            ) as connection, connection.makefile("rb") as reader:
                client = Client(connection, reader)
                client.login(founder, accounts[founder], operator, accounts[operator])
                for channel in channels:
                    try:
                        client.provision(channel, founder)
                    except SeedError as error:
                        raise SeedError(f"{channel['name']}: {error}") from None


def read_passwords(path):
    return {account["name"]: account["password"] for account in json.loads(Path(path).read_text(encoding="utf-8"))
            if "password" in account}


if __name__ == "__main__":
    try:
        main()
    except SeedError as error:
        sys.exit(f"subspace-irc: {error}")
    except (OSError, ValueError):
        sys.exit("subspace-irc: Could not read the channel seed or communicate with bootstrap IRC")
