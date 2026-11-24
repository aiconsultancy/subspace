"""Offline tests for the local agents' IRC helper, against a fixture API and IRC server."""
import base64
import contextlib
from http.server import BaseHTTPRequestHandler
import io
import json
import os
from pathlib import Path
import socket
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
SCRIPT = HERE.parents[1] / "agent-skills/bin/subspace_irc.py"
sys.path.insert(0, str(SCRIPT.parent))
sys.path.insert(0, str(HERE))
import subspace_irc as irc_helper
import subspace_token as token
from subspace_token_test import local_http

FLEET = {
    "id": "00000000-0000-4000-8000-000000000001", "slug": "example-fleet", "name": "Example Fleet",
    "irc": {"host": "irc.example.com", "port": 6697, "tls": False, "websocketUrl": None},
    "starbaseChannel": "#example-fleet-starbase", "commandChannel": "#example-fleet-command",
    "ships": [{"id": "00000000-0000-4000-8000-000000000002", "name": "Orion", "channel": "#example-fleet-orion"}],
    "agents": [
        {"name": "Atlas", "rank": "rearAdmiral", "ircNick": "example-fleet-atlas", "shipId": None, "available": True},
        {"name": "Orion", "rank": "captain", "ircNick": "example-fleet-orion",
         "shipId": "00000000-0000-4000-8000-000000000002", "available": True},
    ],
}
PASSWORD = "sirc1.fixturefixturefixturefixture"


class FixtureApi(BaseHTTPRequestHandler):
    wbufsize = -1

    def log_message(self, *_):
        pass

    def answer(self, status, payload):
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(payload).encode())

    def do_GET(self):
        self.server.requests.append((self.path, None, self.headers.get("Authorization")))
        if self.path == "/api/v1/me/initiatives":
            self.answer(200, {"items": [
                {"name": "One", "channels": "", "fleet": FLEET},
                {"name": "Two", "channels": "#example-notes, #example-fleet-starbase", "fleet": FLEET},
                {"name": "No fleet", "channels": "#elsewhere", "fleet": None},
            ]})
        else:
            self.answer(200, {"displayName": "Example Person", "username": "example-person"})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.server.requests.append((self.path, body, self.headers.get("Authorization")))
        if self.server.credential_status != 200:
            self.answer(self.server.credential_status, {"detail": "fixture refusal"})
        else:
            self.answer(200, {"account": "example-person", "password": PASSWORD, "expiresAt": "2026-01-01T00:00:00Z"})


class FixtureIrc:
    """Just enough of Ergo: SASL PLAIN, joins, echo, draft/multiline, chathistory and scripted replies."""

    def __init__(self, history=(), on_message=None, caps=None):
        self.client, self.server = socket.socketpair()
        self.history, self.on_message = list(history), on_message
        self.caps = caps or ("sasl=PLAIN message-tags server-time batch echo-message account-tag "
                             "draft/multiline=max-bytes=4096,max-lines=100 draft/chathistory")
        self.received, self.sasl, self.count = [], None, 0
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def send(self, line):
        self.server.sendall((line + "\r\n").encode())

    def msgid(self):
        self.count += 1
        return f"fixture{self.count}"

    def deliver(self, message):
        """Send a message from someone else into the conversation, as the server would."""
        self.history.append(message)
        tags = f"@msgid={message['msgid']};account={message['nick']};time=2026-01-01T00:00:00.000Z"
        if message.get("reply"):
            tags += f";+draft/reply={message['reply']}"
        if message.get("progress"):
            tags += f";+stargate/progress={message['progress']}"
        self.send(f"{tags} :{message['nick']}!u@h {message.get('command', 'PRIVMSG')} {message['target']} :{message['text']}")

    def serve(self):
        buffer, batch = b"", None
        with self.server:
            while True:
                data = self.server.recv(65536)
                if not data:
                    return
                buffer += data
                while b"\r\n" in buffer:
                    raw, buffer = buffer.split(b"\r\n", 1)
                    line = raw.decode()
                    tags, _, command, params = irc_helper.parse(line)
                    self.received.append(line)
                    if command == "CAP" and params[:1] == ["LS"]:
                        self.send(f":srv CAP * LS :{self.caps}")
                    elif command == "CAP" and params[:1] == ["REQ"]:
                        self.send(f":srv CAP * ACK :{params[-1]}")
                    elif command == "AUTHENTICATE" and params == ["PLAIN"]:
                        self.send("AUTHENTICATE +")
                    elif command == "AUTHENTICATE":
                        self.sasl = base64.b64decode(params[0]).split(b"\0")
                        ok = self.sasl == [b"example-person", b"example-person", PASSWORD.encode()]
                        self.send(":srv 903 * :ok" if ok else ":srv 904 * :failed")
                    elif command == "CAP" and params[:1] == ["END"]:
                        self.send(":srv 001 example-person :Welcome")
                    elif command == "JOIN":
                        self.send(f":example-person!u@h JOIN {params[0]}")
                        self.send(f":srv 366 example-person {params[0]} :End of NAMES list")
                    elif command == "PING":
                        self.send(f":srv PONG srv :{params[-1]}")
                    elif command == "BATCH" and params[0].startswith("+"):
                        batch = {"tags": tags, "target": params[2], "lines": []}
                    elif command == "BATCH":
                        self.echo(batch["tags"], batch["target"], batch["lines"])
                        batch = None
                    elif command == "PRIVMSG" and batch is not None:
                        batch["lines"].append((tags, params[1]))
                    elif command == "PRIVMSG":
                        self.echo(tags, params[0], [(tags, params[1])])
                    elif command == "CHATHISTORY":
                        self.replay(params)

    def echo(self, tags, target, lines):
        msgid = self.msgid()
        reply = f";+draft/reply={tags['+draft/reply']}" if "+draft/reply" in tags else ""
        text = ""
        for index, (line_tags, part) in enumerate(lines):
            text += part if "draft/multiline-concat" in line_tags else ("\n" if index else "") + part
        posted = {"msgid": msgid, "nick": "example-person", "target": target, "text": text,
                  "reply": tags.get("+draft/reply", "")}
        if len(lines) > 1:
            self.send(f"@msgid={msgid};account=example-person{reply} :example-person!u@h BATCH +e draft/multiline {target}")
            for line_tags, part in lines:
                concat = ";draft/multiline-concat" if "draft/multiline-concat" in line_tags else ""
                self.send(f"@batch=e{concat} :example-person!u@h PRIVMSG {target} :{part}")
            self.send(":srv BATCH -e")
        else:
            self.send(f"@msgid={msgid};account=example-person{reply} :example-person!u@h PRIVMSG {target} :{text}")
        self.history.append(posted)
        if self.on_message:
            self.on_message(self, posted)

    def replay(self, params):
        mode, target = params[0], params[1]
        messages = self.history
        if mode == "AFTER":
            after = params[2].removeprefix("msgid=")
            ids = [message["msgid"] for message in messages]
            messages = messages[ids.index(after) + 1:] if after in ids else []
        else:
            messages = messages[-int(params[3]):]
        self.send(f":srv BATCH +h chathistory {target}")
        for message in messages:
            if "\n" in message["text"]:
                self.send(f"@batch=h;msgid={message['msgid']};account={message['nick']} :{message['nick']}!u@h "
                          f"BATCH +m draft/multiline {target}")
                for part in message["text"].split("\n"):
                    self.send(f"@batch=m :{message['nick']}!u@h PRIVMSG {target} :{part}")
                self.send(":srv BATCH -m")
                continue
            reply = f";+draft/reply={message['reply']}" if message.get("reply") else ""
            self.send(f"@batch=h;msgid={message['msgid']};account={message['nick']};time=2026-01-01T00:00:00.000Z{reply} "
                      f":{message['nick']}!u@h PRIVMSG {target} :{message['text']}")
        self.send(":srv BATCH -h")


class IrcHelperTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.home = Path(temporary.name)
        env = {key: value for key, value in os.environ.items() if not key.startswith("SUBSPACE_")}
        env.update(SUBSPACE_HOME=str(self.home), no_proxy="127.0.0.1")
        for fixture in (
            patch.dict(os.environ, env, clear=True),
            patch.object(token, "HOME", self.home),
            patch.object(irc_helper, "HOME", self.home),
            patch.object(irc_helper, "TIMEOUT", 5),
        ):
            fixture.start()
            self.addCleanup(fixture.stop)

    @contextlib.contextmanager
    def hosted(self, credential_status=200, signed_in=True):
        with local_http(FixtureApi) as api:
            api.requests, api.credential_status = [], credential_status
            url = f"http://127.0.0.1:{api.server_port}"
            (self.home / "agent.json").write_text(json.dumps(
                {"identity_provider": "workos", "api_url": url, "workos_client_id": "client_EXAMPLE"}))
            session = patch.object(token, "workos_token", return_value="fixture-access") if signed_in else \
                patch.object(token, "workos_token", side_effect=token.TokenError(token.SIGN_IN_REQUIRED))
            with session:
                yield api

    @contextlib.contextmanager
    def irc(self, **options):
        server = FixtureIrc(**options)
        connect = patch.object(irc_helper, "open_connection", return_value=server.client)
        peer = patch.object(irc_helper, "peer_address", return_value="127.0.0.1")
        with connect as connection, peer:
            yield server, connection

    def run_helper(self, *args):
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(sys, "argv", [str(SCRIPT), *args]), patch.object(sys, "stdout", output), \
                patch.object(sys, "stderr", errors):
            try:
                irc_helper.main()
                code = 0
            except SystemExit as exit:
                code = exit.code
        return code, output.getvalue(), errors.getvalue()

    def test_channels_come_from_the_api_once_per_fleet_with_who_answers(self):
        with self.hosted() as api:
            code, output, _ = self.run_helper("channels")
        self.assertEqual(code, 0)
        self.assertIn("You are Example Person, IRC account example-person.", output)
        self.assertEqual(output.count("Example Fleet (fleet example-fleet) on irc.example.com:6697"), 1)
        self.assertIn("#example-fleet-starbase  Starbase: people and the rear admiral, example-fleet-atlas,", output)
        self.assertIn("#example-fleet-orion     Ship Orion: captain example-fleet-orion answers every message", output)
        self.assertIn("#example-notes", output)
        self.assertNotIn("#elsewhere", output)
        self.assertTrue(all(request[2] == "Bearer fixture-access" for request in api.requests))

    def test_say_signs_in_with_an_api_credential_for_that_server_and_marks_one_threaded_message(self):
        with self.hosted() as api, self.irc() as (server, connection):
            code, output, _ = self.run_helper(
                "say", "#example-fleet-orion", "--as", "Claude Opus 5.5", "--for", "George",
                "--reply-to", "root1", "Orion, please take NAV-12.\n\nThen tell me what blocks it.")
        self.assertEqual(code, 0, output)
        connection.assert_called_once_with(("irc.example.com", 6697))
        self.assertIn(("/api/v1/me/irc-credential", {"host": "irc.example.com", "port": 6697}, "Bearer fixture-access"),
                      api.requests)
        self.assertEqual(server.sasl, [b"example-person", b"example-person", PASSWORD.encode()])
        posted = server.history[-1]
        self.assertEqual(posted["text"], "[on behalf of George (Claude Opus 5.5)] Orion, please take NAV-12.\n"
                                         "Then tell me what blocks it.")
        self.assertEqual(posted["reply"], "root1")
        self.assertIn("@+draft/reply=root1 BATCH +say draft/multiline #example-fleet-orion", server.received)
        self.assertEqual(output.strip(), "sent message to #example-fleet-orion as example-person, marked "
                                         "[on behalf of George (Claude Opus 5.5)] [id fixture1]")
        self.assertNotIn(PASSWORD, output)

    def test_a_long_paragraph_wraps_inside_one_message_and_lines_without_multiline_each_carry_the_marker(self):
        words = " ".join(f"word{index}" for index in range(120))
        with self.hosted(), self.irc() as (server, _):
            self.assertEqual(self.run_helper("say", "#example-fleet-orion", "--as", "Model 1", "--for", "George", words)[0], 0)
        self.assertEqual(server.history[-1]["text"], "[on behalf of George (Model 1)] " + words)
        self.assertTrue(any(line.startswith("@batch=say;draft/multiline-concat PRIVMSG") for line in server.received))
        with self.hosted(), self.irc(caps="sasl=PLAIN message-tags batch echo-message") as (server, _):
            code, output, _ = self.run_helper("say", "#example-fleet-orion", "--as", "Model 1", "--for", "George", words)
        self.assertEqual(code, 0)
        lines = [line for line in server.received if line.startswith("PRIVMSG")]
        self.assertGreater(len(lines), 1)
        self.assertTrue(all(":[on behalf of George (Model 1)] " in line and len(line.encode()) <= 400 for line in lines))
        self.assertIn(f"sent {len(lines)} lines", output)

    def test_say_wait_prints_progress_and_the_officers_reply_in_its_thread_and_ignores_your_own(self):
        def officer(server, posted):
            if posted["nick"] != "example-person":
                return
            server.deliver({"msgid": "other", "nick": "example-person", "target": posted["target"],
                            "text": "the person typing in the desktop", "reply": posted["msgid"]})
            server.deliver({"msgid": "elsewhere", "nick": "example-fleet-atlas", "target": posted["target"],
                            "text": "an unrelated message"})
            server.deliver({"msgid": "p1", "nick": "example-fleet-orion", "target": posted["target"], "command": "NOTICE",
                            "text": "Looking at NAV-12", "reply": posted["msgid"], "progress": "working"})
            server.deliver({"msgid": "r1", "nick": "example-fleet-orion", "target": posted["target"],
                            "text": "On it.", "reply": posted["msgid"]})

        with self.hosted(), self.irc(on_message=officer):
            code, output, _ = self.run_helper("say", "#example-fleet-orion", "--as", "Model 1", "--for", "George",
                                              "--wait", "5", "Orion, please take NAV-12.")
        self.assertEqual(code, 0)
        self.assertIn("... example-fleet-orion is working (working): Looking at NAV-12", output)
        self.assertIn("<example-fleet-orion> On it.  [id r1, reply to fixture1]", output)
        self.assertNotIn("the person typing", output)
        self.assertNotIn("unrelated", output)

    def test_wait_finds_a_reply_already_in_history_and_times_out_cleanly_otherwise(self):
        history = [
            {"msgid": "q1", "nick": "example-person", "target": "#example-fleet-orion", "text": "question"},
            {"msgid": "r1", "nick": "example-fleet-orion", "target": "#example-fleet-orion", "text": "answer",
             "reply": "q1"},
        ]
        with self.hosted(), self.irc(history=history):
            code, output, _ = self.run_helper("wait", "#example-fleet-orion", "--thread", "q1")
        self.assertEqual(code, 0)
        self.assertIn("<example-fleet-orion> answer  [id r1, reply to q1]", output)
        with self.hosted(), self.irc(history=history[:1]):
            code, output, errors = self.run_helper("wait", "#example-fleet-orion", "--thread", "q1", "--timeout", "1")
        self.assertIn("no reply within 1 seconds", str(code))
        self.assertEqual(output, "")

    def test_history_assembles_multiline_messages_skips_join_notices_and_filters_a_thread(self):
        history = [
            {"msgid": "j1", "nick": "HistServ", "target": "#example-fleet-starbase", "text": "someone joined"},
            {"msgid": "m1", "nick": "example-fleet-atlas", "target": "#example-fleet-starbase", "text": "line one\nline two"},
            {"msgid": "m2", "nick": "example-person", "target": "#example-fleet-starbase", "text": "thanks", "reply": "m1"},
            {"msgid": "m3", "nick": "example-person", "target": "#example-fleet-starbase", "text": "something else"},
        ]
        with self.hosted(), self.irc(history=history):
            code, output, _ = self.run_helper("history", "#example-fleet-starbase", "--limit", "10")
        self.assertEqual(code, 0)
        self.assertIn("<example-fleet-atlas> line one\n    line two  [id m1]", output)
        self.assertIn("thanks  [id m2, reply to m1]", output)
        self.assertNotIn("someone joined", output)
        with self.hosted(), self.irc(history=history):
            _, output, _ = self.run_helper("history", "#example-fleet-starbase", "--thread", "m1")
        self.assertNotIn("something else", output)
        self.assertIn("thanks", output)

    def test_refusals_come_before_any_credential_is_requested(self):
        with self.hosted(signed_in=False):
            code, _, _ = self.run_helper("history", "#example-fleet-orion")
        self.assertIn("not signed in to Subspace", str(code))
        self.assertIn("subspace_token.py --login", str(code))
        with self.hosted(credential_status=404), self.irc():
            code, _, _ = self.run_helper("history", "#example-fleet-orion")
        self.assertIn("issues no IRC sign-in for irc.example.com:6697", str(code))
        with self.hosted() as api, self.irc() as (server, connection):
            code, _, _ = self.run_helper("say", "#example-fleet-orion", "--as", "Model 1", "--for", "George",
                                         "here is the key -----BEGIN " + "OPENSSH PRIVATE KEY----- abc")
            self.assertIn("looks like it contains a key or token", str(code))
            connection.assert_not_called()
            code, _, _ = self.run_helper("say", "#example-fleet-orion", "--for", "George", "hello")
            self.assertEqual(code, 2)  # --as is required
        self.assertFalse(any(path == "/api/v1/me/irc-credential" for path, _, _ in api.requests))
        with self.hosted() as api, self.irc() as (server, _):
            with patch.object(irc_helper, "peer_address", return_value="203.0.113.7"):
                code, _, _ = self.run_helper("history", "#example-fleet-orion")
        self.assertIn("an IRC sign-in is sent only over TLS", str(code))
        self.assertFalse(any(path == "/api/v1/me/irc-credential" for path, _, _ in api.requests))

    def test_development_sign_in_uses_the_installed_fleets_saved_password(self):
        target = self.home / "targets" / "example-fleet"
        target.mkdir(parents=True)
        (target / "summary.env").write_text("SUBSPACE_ADMIN_ACCOUNT=example-person\n"
                                            "SUBSPACE_CHANNELS=#example-fleet-starbase #example-fleet-orion\n")
        (target / "irc.password").write_text(PASSWORD + "\n")
        (self.home / "agents.env").write_text("SUBSPACE_TARGET=example-fleet\nSUBSPACE_HUMAN_NAME=George\n")
        self.assertEqual(self.run_helper("channels")[1], "#example-fleet-starbase\n#example-fleet-orion\n")
        with self.irc() as (server, connection):
            code, output, _ = self.run_helper("say", "#example-fleet-starbase", "--as", "Model 1", "hello")
        self.assertEqual(code, 0)
        connection.assert_called_once_with(("127.0.0.1", 6667))
        self.assertEqual(server.history[-1]["text"], "[on behalf of George (Model 1)] hello")


if __name__ == "__main__":
    unittest.main()
