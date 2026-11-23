"""Exercise the real Ergo image on ephemeral loopback ports via Docker Compose."""

import base64
import hashlib
import json
import os
import secrets
import socket
import ssl
import subprocess
import uuid
from pathlib import Path
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[1]
CHANNEL = "#smoke-command"
CAPTAIN_CHANNEL = "#smoke-captain"
INITIATIVE_CHANNEL = "#smoke-initiative"
CAPS = (
    "sasl account-tag account-notify batch message-tags server-time echo-message "
    "draft/chathistory draft/multiline draft/read-marker draft/metadata-2"
)


class Irc:
    def __init__(self, connection):
        self.connection = connection
        self.reader = connection.makefile("rb")

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.reader.close()
        self.connection.close()

    def send(self, *lines):
        self.connection.sendall(("\r\n".join(lines) + "\r\n").encode())

    def until(self, predicate, description):
        for _ in range(120):
            line = self.reader.readline().decode().rstrip("\r\n")
            if not line:
                break
            if line.startswith("PING "):
                self.send("PONG " + line[5:])
            if predicate(line):
                return line
        raise AssertionError(f"Ergo did not return {description}")

    def expect(self, fragment):
        return self.until(lambda line: fragment in line, fragment)

    def login(self, name, password):
        self.send("CAP LS 302", f"NICK {name}", f"USER {name} 0 * :Test account")
        offer = []

        def last_offer(line):
            if " CAP * LS " not in line:
                return False
            offer.extend(line.split(" :", 1)[1].split())
            return " CAP * LS * " not in line

        self.until(last_offer, "end of CAP LS")
        offered = {name: value for name, _, value in (cap.partition("=") for cap in offer)}
        assert set(CAPS.split()) <= offered.keys(), "missing IRCv3 capabilities"
        assert "PLAIN" in offered["sasl"].split(","), "SASL PLAIN not offered"
        assert offered["draft/multiline"] == "max-bytes=4096,max-lines=100"
        self.send("CAP REQ :" + CAPS)
        self.expect(" ACK :")
        self.send("AUTHENTICATE PLAIN")
        self.expect("AUTHENTICATE +")
        payload = base64.b64encode(f"\0{name}\0{password}".encode()).decode()
        self.send("AUTHENTICATE " + payload)
        self.expect(" 903 ")
        self.send("CAP END")
        self.expect(f" 001 {name} ")
        self.expect(" 376 ")

    def join(self, channel=CHANNEL):
        self.send("JOIN " + channel)
        self.expect(" JOIN " + channel)
        self.expect(" 366 ")

    def replay(self, text):
        self.send(f"CHATHISTORY LATEST {CHANNEL} * 20")
        start = self.expect(f" chathistory {CHANNEL}")
        batch = start.split(" BATCH +")[1].split()[0]
        seen = self.expect(f" PRIVMSG {CHANNEL} :{text}")
        assert f"batch={batch}" in seen and "time=" in seen and "msgid=" in seen
        self.expect(" BATCH -" + batch)


def connect(port, context=None):
    connection = socket.create_connection(("127.0.0.1", port), timeout=8)
    if context is not None:
        try:
            connection = context.wrap_socket(connection, server_hostname="127.0.0.1")
        except BaseException:
            connection.close()
            raise
    return Irc(connection)


def check_websocket(port, origin=None, status=101):
    key = base64.b64encode(secrets.token_bytes(16)).decode()
    origin_header = f"Origin: {origin}\r\n" if origin is not None else ""
    with socket.create_connection(("127.0.0.1", port), timeout=8) as ws:
        ws.sendall((
            f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n"
            "Connection: Upgrade\r\nUpgrade: websocket\r\n"
            f"Sec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n"
            f"Sec-WebSocket-Protocol: text.ircv3.net\r\n{origin_header}\r\n"
        ).encode())
        with ws.makefile("rb") as reader:
            assert reader.readline().startswith(f"HTTP/1.1 {status} ".encode())
            if status != 101:
                return
            headers = {}
            for header in reader:
                if header == b"\r\n":
                    break
                name, value = header.decode().split(":", 1)
                headers[name.lower()] = value.strip()
            expected = base64.b64encode(
                hashlib.sha1((key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").encode()).digest()
            ).decode()
            assert headers["sec-websocket-accept"] == expected
            assert headers["sec-websocket-protocol"] == "text.ircv3.net"

            payload = b"PING :websocket-smoke"
            mask = secrets.token_bytes(4)
            masked = bytes(value ^ mask[i % 4] for i, value in enumerate(payload))
            ws.sendall(bytes([0x81, 0x80 | len(payload)]) + mask + masked)
            for _ in range(10):
                opcode, size = reader.read(2)
                assert not size & 0x80
                if size == 126:
                    size = int.from_bytes(reader.read(2), "big")
                frame = reader.read(size)
                if opcode == 0x81 and b" PONG " in frame and b"websocket-smoke" in frame:
                    return
    raise AssertionError("Ergo did not answer PING over WebSocket")


def exercise(port, tls_port, context, operator_pw, agent_pw):
    with connect(port) as anonymous:
        anonymous.send("NICK anonymous", "USER anonymous 0 * :Test account")
        anonymous.expect(" FAIL * ACCOUNT_REQUIRED ")

    with connect(tls_port, context) as operator, connect(port) as agent:
        operator.login("smoke-operator", operator_pw)
        agent.login("smoke-agent", agent_pw)
        unseeded = "#smoke-unseeded"
        agent.send(f"JOIN {unseeded}")
        agent.expect(" 403 ")
        operator.send(f"OPER smoke-operator :{operator_pw}")
        operator.expect(" 381 ")
        operator.join(unseeded)
        operator.send(f"PRIVMSG ChanServ :REGISTER {unseeded}")
        operator.expect("successfully registered")

        operator.join()
        agent.join()
        agent.send(f"PRIVMSG {CHANNEL} :persistent smoke message")
        live = operator.expect(f" PRIVMSG {CHANNEL} :persistent smoke message")
        assert "account=smoke-agent" in live and "time=" in live and "msgid=" in live
        timestamp = next(tag[5:] for tag in live[1:].split()[0].split(";") if tag.startswith("time="))
        agent.replay("persistent smoke message")

        agent.send(f"MARKREAD {CHANNEL} timestamp={timestamp}")
        agent.expect(f" MARKREAD {CHANNEL} timestamp={timestamp}")
        operator.send("METADATA * SUB status")
        operator.expect(" 770 ")
        agent.send("METADATA * SET status :Idle")
        operator.until(
            lambda line: line.endswith((" smoke-agent status * Idle", " smoke-agent status * :Idle")),
            "the agent's Idle status",
        )

        agent.send(
            f"BATCH +smoke draft/multiline {CHANNEL}",
            f"@batch=smoke PRIVMSG {CHANNEL} :first part",
            f"@batch=smoke PRIVMSG {CHANNEL} :second part",
            "BATCH -smoke",
        )
        start = operator.until(
            lambda line: line.endswith((f" draft/multiline {CHANNEL}", f" draft/multiline :{CHANNEL}")),
            "start of multiline batch",
        )
        batch = start.split(" BATCH +")[1].split()[0]
        for text in ("first part", "second part"):
            line = operator.expect(f" PRIVMSG {CHANNEL} :{text}")
            assert f"batch={batch}" in line
        operator.until(
            lambda line: line.endswith((f" BATCH -{batch}", f" BATCH :-{batch}")),
            "end of multiline batch",
        )


def check_seeded(port, agent_pw, captain_pw, command_topic, command_modes):
    with connect(port) as agent, connect(port) as captain:
        agent.login("smoke-agent", agent_pw)
        captain.login("smoke-captain", captain_pw)
        agent.join()
        for channel, founder in ((CHANNEL, "smoke-operator"), (CAPTAIN_CHANNEL, "smoke-captain")):
            agent.send(f"PRIVMSG ChanServ :INFO {channel}")
            agent.expect(f"Channel {channel} is registered")
            agent.expect(f"Founder: {founder}")

        agent.send(f"TOPIC {CHANNEL}")
        if command_topic:
            agent.expect(f" 332 smoke-agent {CHANNEL} :{command_topic}")
        else:
            agent.expect(f" 331 smoke-agent {CHANNEL} ")
        agent.send(f"MODE {CHANNEL}")
        mode_reply = agent.expect(f" 324 smoke-agent {CHANNEL} ")
        assert set(mode_reply.split()[-1].lstrip(":+")) == set(command_modes)

        captain.join(CAPTAIN_CHANNEL)
        captain.join(INITIATIVE_CHANNEL)
        captain.send(f"MODE {INITIATIVE_CHANNEL}")
        private_modes = captain.expect(f" 324 smoke-captain {INITIATIVE_CHANNEL} ")
        assert {"i", "s"} <= set(private_modes.split()[-1])
        agent.send(f"JOIN {INITIATIVE_CHANNEL}")
        agent.expect(f" 473 smoke-agent {INITIATIVE_CHANNEL} ")


def write_channel_mount(directory, channels):
    seed_file = directory / "channels.json"
    seed_file.write_text(json.dumps(channels), encoding="utf-8")
    override = directory / "compose.json"
    override.write_text(json.dumps({"services": {"irc": {"volumes": [{
        "type": "bind", "source": str(seed_file), "target": "/run/channels.json", "read_only": True,
    }]}}}), encoding="utf-8")
    return override


def smoke():
    project = "subspace-irc-smoke-" + uuid.uuid4().hex[:10]
    command = ["docker", "compose", "--env-file", "/dev/null", "-f", str(ROOT / "compose.yaml"),
               "-p", project]
    operator_pw = secrets.token_urlsafe(24)
    agent_pw = secrets.token_urlsafe(24)
    captain_pw = secrets.token_urlsafe(24)
    seed = [
        {"name": "smoke-agent", "password": agent_pw},
        {"name": "smoke-captain", "password": captain_pw},
    ]
    channels = [
        {"name": CHANNEL, "topic": "Fleet coordination", "modes": "+s-C"},
        {"name": CAPTAIN_CHANNEL, "founder": "smoke-captain"},
        {"name": INITIATIVE_CHANNEL, "founder": "smoke-captain", "modes": "+is"},
    ]
    env = {key: value for key, value in os.environ.items() if not key.startswith("IRC_")}
    env.update(
        IRC_OPER_NAME="smoke-operator",
        IRC_OPER_PASSWORD=operator_pw,
        IRC_ACCOUNTS_JSON=json.dumps(seed),
        IRC_CHANNELS_JSON=json.dumps(channels),
        IRC_PLAIN_PORT="0",
        IRC_TLS_PORT="0",
        IRC_WS_PORT="0",
    )

    def compose(*args, check=True, capture=False, overrides=None, extra_file=None):
        base = command if extra_file is None else command + ["-f", str(extra_file)]
        return subprocess.run(
            base + list(args), env={**env, **(overrides or {})},
            timeout=180, check=check, capture_output=capture, text=True,
        )

    def port(container_port):
        address = compose("port", "irc", str(container_port), capture=True).stdout.strip()
        host, host_port = address.rsplit(":", 1)
        assert host == "127.0.0.1", "test service must be on loopback"
        return int(host_port)

    def fails_to_start(message, **overrides):
        result = compose(
            "run", "--rm", "--no-deps", "-T", "irc",
            check=False, capture=True, overrides=overrides,
        )
        assert result.returncode != 0 and message in result.stderr, message
        for password in (operator_pw, agent_pw, captain_pw):
            assert password not in (result.stdout + result.stderr), "credential in startup logs"

    def check_cost(cost):
        config = compose(
            "exec", "-T", "irc", "grep", "bcrypt-cost:", "/run/subspace-irc/ircd.yaml", capture=True,
        ).stdout.strip()
        assert config == f"bcrypt-cost: {cost}"

    try:
        compose("up", "--build", "--wait", "--wait-timeout", "40")
        plain, tls, ws = (port(container_port) for container_port in (6667, 6697, 8097))
        assert compose("exec", "-T", "irc", "id", "-u", capture=True).stdout.strip() == "10001"
        cert = compose(
            "exec", "-T", "irc", "openssl", "x509", "-in", "/ircd/local-tls.pem", capture=True,
        ).stdout
        context = ssl.create_default_context(cadata=cert)
        check_cost(12)
        # Without IRC_AUTH_API_URL the auth hook is absent and no token is written.
        config = compose(
            "exec", "-T", "irc", "cat", "/run/subspace-irc/ircd.yaml", capture=True,
        ).stdout
        assert "auth-script:" not in config and "allow-environment-overrides: false" in config
        assert compose(
            "exec", "-T", "irc", "test", "-e", "/run/subspace-irc/auth-token", check=False,
        ).returncode == 1
        exercise(plain, tls, context, operator_pw, agent_pw)
        check_seeded(plain, agent_pw, captain_pw, "Fleet coordination", "nst")
        check_websocket(ws, "http://localhost:3000")
        check_websocket(ws, "https://example.com", 403)
        check_websocket(ws, status=403)
        print("Passed SASL, channel seeding, operator-only creation, IRCv3, TLS and WebSocket origins")

        fails_to_start("data volume is already in use")
        compose("stop", "irc")
        fails_to_start(
            "Expected distinct lower-case account names",
            IRC_ACCOUNTS_JSON=json.dumps(seed + [{"name": "bad", "password": "line\nbreak"}]),
        )
        fails_to_start(
            "Account name is reserved by Ergo",
            IRC_ACCOUNTS_JSON=json.dumps(seed + [{"name": "chanserv", "password": agent_pw}]),
        )
        fails_to_start("Account name is reserved by Ergo", IRC_OPER_NAME="operserv")
        fails_to_start("IRC_BCRYPT_COST must be an integer", IRC_BCRYPT_COST="3")
        fails_to_start(
            "repeats a channel name",
            IRC_CHANNELS_JSON=json.dumps(channels + [{"name": CHANNEL.upper()}]),
        )
        fails_to_start(
            "needs a founder from the operator or account seed",
            IRC_CHANNELS_JSON=json.dumps([{"name": CHANNEL, "founder": "missing-founder"}]),
        )
        fails_to_start(
            "channel is registered to a different founder",
            IRC_CHANNELS_JSON=json.dumps([{"name": CHANNEL, "founder": "smoke-captain"}]),
        )
        # Ergo refuses passphrases with whitespace; catch them before any account is created.
        fails_to_start(
            "Expected distinct lower-case account names",
            IRC_ACCOUNTS_JSON=json.dumps([{"name": "new-account", "password": "has space"}]),
        )
        fails_to_start(
            "does not match the existing account",
            IRC_ACCOUNTS_JSON=json.dumps([{**seed[0], "password": "wrong-synthetic-password"}, seed[1]]),
        )
        compose("up", "--force-recreate", "--wait", "--wait-timeout", "40")
        check_seeded(port(6667), agent_pw, captain_pw, "Fleet coordination", "nst")
        # Verify the same trust anchor and current persisted account credentials.
        with connect(port(6697), context) as agent:
            agent.login("smoke-agent", agent_pw)
            agent.join()
            agent.replay("persistent smoke message")

        # Exercise the mounted source, configured default founder and deliberate seed updates.
        updated = [
            {**channels[0], "founder": "smoke-operator", "topic": "", "modes": "+C-s"},
            {"name": CAPTAIN_CHANNEL},
            channels[2],
        ]
        with TemporaryDirectory(dir=ROOT, prefix=".channel-seed-") as directory:
            override = write_channel_mount(Path(directory), updated)
            compose(
                "up", "--force-recreate", "--wait", "--wait-timeout", "40",
                extra_file=override,
                overrides={
                    "IRC_BCRYPT_COST": "10",
                    "IRC_CHANNELS_FILE": "/run/channels.json",
                    "IRC_CHANNELS_JSON": "",
                    "IRC_CHANNEL_FOUNDER": "smoke-captain",
                    "IRC_WS_ORIGIN": "https://chat.example.com",
                },
            )
            check_cost(10)
            check_seeded(port(6667), agent_pw, captain_pw, "", "Cnt")
            check_websocket(port(8097), "https://chat.example.com")
            check_websocket(port(8097), "http://localhost:3000", 403)
        print("Passed volume locking, validation, idempotency, persistent history and mounted updates")
    except BaseException:
        compose("logs", "--tail", "50", "irc", check=False)
        raise
    finally:
        compose("down", "--volumes", "--remove-orphans", "--rmi", "local", check=False)


if __name__ == "__main__":
    smoke()
