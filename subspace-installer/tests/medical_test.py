"""Exercise the Sentry issue-list contract and medical poll receipts offline."""

from contextlib import contextmanager
from datetime import datetime, timezone
from email.utils import formatdate
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import socket
import sqlite3
import sys
from tempfile import TemporaryDirectory
from threading import Thread
from time import sleep, time
from unittest import TestCase, main
from unittest.mock import patch
from urllib.parse import parse_qs, urlsplit

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import medical
from runtime import Invalid, integrations_config, medical_config, member


ISSUES = "/api/0/organizations/your-org/issues/"


def iso(instant):
    return datetime.fromtimestamp(instant, timezone.utc).isoformat().replace("+00:00", "Z")


def cursor(value, results="true"):
    return f'<https://sentry.io{ISSUES}?cursor={value}>; rel="next"; results="{results}"; cursor="{value}"'


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        target = urlsplit(self.path)
        query = parse_qs(target.query)
        self.server.requests.append((target.path, query, self.headers.get("Authorization")))
        if target.path != ISSUES:
            self.send_error(404)
            return
        status, body, headers = self.server.respond(query)
        payload = body if isinstance(body, bytes) else json.dumps(body).encode()
        self.send_response(status)
        self.send_header("Content-Length", str(len(payload)))
        for key, value in headers.items():
            self.send_header(key, value)
        self.end_headers()
        try:
            self.wfile.write(payload)
        except (BrokenPipeError, ConnectionResetError):
            pass  # The timeout test has already disconnected.

    def log_message(self, *_args):
        pass


class MedicalTest(TestCase):
    def setUp(self):
        files = self.enterContext(TemporaryDirectory())
        self.db = str(Path(files) / "receipts.sqlite3")
        self.server = self.enterContext(ThreadingHTTPServer(("127.0.0.1", 0), Handler))
        self.server.requests = []
        self.server.respond = lambda _: (200, [], {})
        thread = Thread(target=self.server.serve_forever, kwargs={"poll_interval": 0.01})
        thread.start()
        self.addCleanup(thread.join)
        self.addCleanup(self.server.shutdown)
        self.port = self.server.server_port
        self.config = {"baseUrl": f"http://127.0.0.1:{self.port}", "organisation": "your-org",
                       "projects": ["your-project"], "environment": "production", "token": "fixture-read-token",
                       "irc": {"channel": "#test"}, "target": "example-officer"}
        self.now = time() - 60
        self.messages = []
        self.enterContext(patch.object(medical, "irc_connection", self.connect))

    @contextmanager
    def connect(self, config):
        self.assertEqual(config["channel"], "#test")
        yield self

    def announce(self, room, message):
        self.assertEqual(room, "#test")
        self.messages.append(message)

    def issue(self, issue_id, **fields):
        return {"id": str(issue_id), "project": {"slug": "your-project", "id": "200"},
                "status": "unresolved", "substatus": "ongoing", "level": "error",
                "lastSeen": iso(self.now), "count": "10", "userCount": 12,
                "permalink": f"https://127.0.0.1:{self.port}/organizations/your-org/issues/{issue_id}/",
                "title": "Fixture details must stay in Sentry", **fields}

    def polls(self, message_ids):
        actual = [message.rsplit("/", 2)[-2] for message in self.messages if "Sentry error in" in message
                  or "Sentry fatal in" in message]
        self.assertEqual(actual, message_ids)

    def test_default_ranking_paging_and_alert_cap(self):
        self.config["environment"] = ""
        rows = [self.issue(41), self.issue(42, level="fatal", userCount=0),
                self.issue(43, userCount=13), self.issue(44, userCount=9),
                self.issue(46, level="warning"), self.issue(47, status="resolved"),
                self.issue(48, lastSeen=iso(self.now - medical.LOOKBACK - 1))]
        production = [self.issue(44, userCount=9)]

        def respond(query):
            if query["query"][0].startswith("is:resolved"):
                return 200, [], {}
            if query.get("environment") == ["production"]:
                return 200, production, {}
            page = int(query.get("cursor", ["0:0:0"])[0][0])
            body = rows if page == 0 else [self.issue(45, userCount=10)] if page == 1 else []
            return 200, body, {"Link": cursor(f"{page + 1}:1:0")}

        self.server.respond = respond
        self.assertEqual(medical.poll(self.config, self.db), 3)
        self.polls(["44", "42", "43"])
        self.assertEqual(len(self.server.requests), 4)
        self.assertEqual(self.server.requests[1][1]["query"], ["is:unresolved lastSeen:-7d level:[error,fatal]"])
        self.assertNotIn("environment", self.server.requests[1][1])
        self.assertEqual(medical.poll(self.config, self.db), 2)
        self.polls(["44", "42", "43", "41", "45"])
        self.assertTrue(all(path == ISSUES and token == "Bearer fixture-read-token"
                            for path, _, token in self.server.requests))

    def test_implicit_production_can_be_absent_and_filter_uses_issue_search(self):
        self.config.update(environment="", projects=["200"], http5xxTag="http.server.status_code")

        def respond(query):
            if "environment" in query:
                return 404, {}, {}
            return 200, [self.issue(41)], {}

        self.server.respond = respond
        self.assertEqual(medical.poll(self.config, self.db), 1)
        self.polls(["41"])
        self.assertEqual(len(self.server.requests), 2)
        query = self.server.requests[-1][1]
        self.assertEqual(query["project"], ["200"])
        self.assertEqual(query["query"], ["is:unresolved lastSeen:-7d level:[error,fatal] tags[http.server.status_code]:5*"])

    def test_partial_429_poll_keeps_alert_and_observes_retry_after(self):
        def respond(query):
            if "cursor" in query:
                return 429, {"detail": "private failure details"}, {"Retry-After": "600"}
            return 200, [self.issue(41)], {"Link": cursor("10:1:0")}

        self.server.respond = respond
        with self.assertRaisesRegex(medical.PollError, "HTTP 429"):
            medical.poll(self.config, self.db)
        self.assertEqual(self.messages[0], medical.POLL_FAILED)
        self.polls(["41"])
        self.assertEqual(medical.poll(self.config, self.db), 0)
        self.assertEqual(len(self.server.requests), 2)
        self.assertEqual(len(self.messages), 2)
        self.server.respond = lambda _: (200, [self.issue(41)], {})
        with patch.object(medical, "time", return_value=self.now + 1000):
            self.assertEqual(medical.poll(self.config, self.db), 0)
        self.assertEqual(self.messages[-1], medical.POLL_RECOVERED)

    def test_5xx_notices_once_per_outage_and_resume_pending_delivery(self):
        self.server.respond = lambda query: (503, {}, {}) if "cursor" in query else (
            200, [self.issue(41), self.issue(42)], {"Link": cursor("10:1:0")})
        deliver = self.announce

        def reject_last(room, message):
            if "/issues/42/" in message:
                raise medical.PollError("IRC rejected the medical ship command.")
            deliver(room, message)

        with patch.object(self, "announce", side_effect=reject_last):
            with self.assertRaisesRegex(medical.PollError, "IRC rejected"):
                medical.poll(self.config, self.db)
        self.polls(["41"])
        self.server.respond = lambda _: (503, {}, {})
        for _ in range(2):
            with self.assertRaisesRegex(medical.PollError, "HTTP 503"):
                medical.poll(self.config, self.db)
        self.polls(["41", "42"])
        self.assertEqual(self.messages.count(medical.POLL_FAILED), 1)
        self.server.respond = lambda _: (200, [], {})
        for _ in range(2):
            self.assertEqual(medical.poll(self.config, self.db), 0)
        self.assertEqual(self.messages.count(medical.POLL_RECOVERED), 1)

    def test_recovery_skips_notices_when_failure_was_never_delivered(self):
        self.server.respond = lambda _: (503, {}, {})
        with patch.object(self, "announce", side_effect=medical.PollError("IRC unavailable")):
            with self.assertRaisesRegex(medical.PollError, "IRC unavailable"):
                medical.poll(self.config, self.db)

        self.server.respond = lambda _: (200, [self.issue(41)], {})
        self.assertEqual(medical.poll(self.config, self.db), 1)
        self.polls(["41"])
        self.assertEqual(len(self.messages), 1)

    def test_self_hosted_permalink_uses_configured_https_host(self):
        self.config["permalinkHost"] = "issues.example.com"
        valid = "https://issues.example.com/organizations/your-org/issues/42/"
        self.server.respond = lambda _: (200, [self.issue(42, permalink=valid)], {})
        self.assertEqual(medical.poll(self.config, self.db), 1)
        self.assertIn(valid, self.messages[0])
        for url in (valid.replace("https:", "http:"), valid.replace(".com/", ".com:8443/"),
                    valid.replace("issues.example.com", f"127.0.0.1:{self.port}")):
            with self.subTest(url=url):
                self.assertIsNone(medical.issue_link(self.config, url, "your-project", "42"))

    def test_regressions_and_legacy_receipt(self):
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE TABLE alerts (origin TEXT, org TEXT, environment TEXT, project TEXT, issue TEXT, "
                       "PRIMARY KEY(origin, org, environment, project, issue))")
            db.execute("INSERT INTO alerts VALUES (?, ?, ?, ?, ?)", (*medical.scope(self.config), "your-project", "41"))
        rows = [self.issue(41), self.issue(42)]
        resolved = []
        self.server.respond = lambda query: (200, resolved if "is:resolved" in query["query"][0] else rows, {})
        self.assertEqual(medical.poll(self.config, self.db), 1)
        rows[0] = self.issue(41, substatus="regressed")
        self.assertEqual(medical.poll(self.config, self.db), 1)
        rows[0]["lastSeen"] = iso(self.now + 1)
        self.assertEqual(medical.poll(self.config, self.db), 0)
        rows[0]["inbox"] = {"reason": 2, "reason_details": {}, "date_added": iso(self.now)}
        self.assertEqual(medical.poll(self.config, self.db), 0)
        rows[0]["inbox"] = {"reason": 1, "reason_details": {}, "date_added": iso(self.now + 1)}
        rows[0]["lastSeen"] = iso(self.now + 2)
        self.assertEqual(medical.poll(self.config, self.db), 0)
        rows[0]["inbox"] = {"reason": 2, "reason_details": {}, "date_added": iso(self.now + 2)}
        rows[0]["lastSeen"] = iso(self.now + 3)
        self.assertEqual(medical.poll(self.config, self.db), 1)
        rows.pop()
        resolved.append(self.issue(42, status="resolved", lastSeen=iso(self.now + 5)))
        self.assertEqual(medical.poll(self.config, self.db), 0)
        resolved.clear()
        rows.append(self.issue(42, lastSeen=iso(self.now + 10)))
        self.assertEqual(medical.poll(self.config, self.db), 1)
        self.assertEqual(medical.poll(self.config, self.db), 0)
        self.polls(["42", "41", "41", "42"])
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute("SELECT count(alerted_at) FROM alerts").fetchone(), (2,))

    def test_redirect_timeout_oversized_and_repeated_cursor(self):
        def slow(_):
            sleep(0.15)
            return 200, [], {}

        cases = [
            (lambda _: (302, {}, {"Location": f"{self.config['baseUrl']}/redirect"}), "HTTP 302"),
            (lambda _: (200, b"x" * 129, {}), "size limit"),
            (slow, "Could not read"),
            (lambda _: (200, [self.issue(41)], {"Link": cursor("10:1:0")}), "repeated"),
        ]
        for respond, expected in cases:
            with self.subTest(expected=expected):
                self.server.respond = respond
                with patch.object(medical, "TIMEOUT", 0.1), patch.object(medical, "RESPONSE_LIMIT", 128 if expected == "size limit" else 4096):
                    with self.assertRaisesRegex(medical.PollError, expected):
                        medical.poll(self.config, self.db)
        self.polls(["41"])
        self.assertTrue(all(path == ISSUES for path, _, _ in self.server.requests))
        self.assertEqual(self.messages.count(medical.POLL_FAILED), 1)

    def test_sentry_errors_and_invalid_links_are_not_echoed(self):
        for code in (401, 404, 500):
            self.server.respond = lambda _, status=code: (status, {"detail": "private failure details"}, {})
            with self.assertRaisesRegex(medical.PollError, f"HTTP {code}"):
                medical.poll(self.config, self.db)
        self.server.respond = lambda _: (200, [
            self.issue(41, permalink="https://example.com/issues/41/"), self.issue(42),
        ], {})
        with self.assertRaisesRegex(medical.PollError, "unexpected issue permalink"):
            medical.poll(self.config, self.db)
        self.assertEqual(self.messages[0], medical.POLL_FAILED)
        self.polls(["42"])

    def test_concurrent_poll_cannot_replay_the_queue(self):
        with open(self.db + ".lock", "a", encoding="utf-8") as lock:
            medical.flock(lock, medical.LOCK_EX)
            with self.assertRaisesRegex(medical.PollError, "Another medical poll"):
                medical.poll(self.config, self.db)
        self.assertEqual(self.server.requests, [])


class ProtocolTest(TestCase):
    def test_cursor_and_retry_after(self):
        self.assertIsNone(medical.next_cursor(cursor("100:1:0", "false")))
        self.assertEqual(medical.next_cursor('<https://sentry.io/>; rel="previous", ' + cursor("100:1:0")), "100:1:0")
        with self.assertRaisesRegex(medical.PollError, "invalid pagination cursor"):
            medical.next_cursor(cursor("unexpected/redirect"))
        self.assertEqual(medical.retry_after("300", 1234), 1534)
        self.assertEqual(medical.retry_after(formatdate(1534, usegmt=True), 1234), 1534)
        self.assertEqual(medical.retry_after("999999999999", 1234), 1234 + 86400)
        self.assertEqual(medical.retry_after(formatdate(1234 + 7 * 86400, usegmt=True), 1234), 1234 + 86400)
        self.assertEqual(medical.retry_after("invalid", 1234), 0)

    def test_issue_permalink_checks_expected_host_scheme_and_id(self):
        config = {"organisation": "your-org", "baseUrl": "https://de.sentry.io"}
        valid = "https://your-org.sentry.io/issues/42/"
        self.assertEqual(medical.issue_link(config, valid + "?foo=bar#event", "sample", "42"), valid)
        for url in ("http://your-org.sentry.io/issues/42/", "https://your-org.sentry.io.evil.example.com/issues/42/",
                    "https://different.sentry.io/issues/42/", "https://user@your-org.sentry.io/issues/42/",
                    "https://your-org.sentry.io/issues/43/", valid + "\n"):
            with self.subTest(url=url):
                self.assertIsNone(medical.issue_link(config, url, "sample", "42"))

    def test_irc_send_and_command_specific_errors(self):
        client, server = socket.socketpair()
        with client, server:
            connection = medical.IRC(client)
            with connection.stream:
                for char in ("\r", "\n", "\0"):
                    with self.assertRaisesRegex(medical.PollError, "line break or NUL"):
                        connection.send("PRIVMSG #test :first" + char + "second")
                server.sendall(b":localhost 422 bot :MOTD missing\r\n:localhost 366 bot #test :End of NAMES\r\n")
                connection.wait(lambda verb, *_: verb == "366", ("403", "473"))
                for code in (902, 904, 905, 906):
                    server.sendall(f":localhost {code} bot :SASL failure\r\n".encode())
                    with self.assertRaisesRegex(medical.PollError, "IRC rejected"):
                        connection.wait(lambda verb, *_: verb == "903", (str(code),))


class ConfigTest(TestCase):
    def test_medical_settings_validation(self):
        opts = {"enabled": True, "organisation": "your-org", "projects": ["123"], "account": "medical-bot"}
        irc = {"allowedAccounts": [], "mode": "remote", "starbaseChannel": "#starbase"}
        self.assertIsNone(medical_config(opts, irc, [])["http5xxTag"])
        self.assertIsNone(medical_config(opts, irc, [])["permalinkHost"])
        parsed = medical_config({**opts, "intervalSeconds": 300, "http5xxTag": "http.server.status_code"}, irc, [])
        self.assertEqual(parsed["intervalSeconds"], 300)
        self.assertEqual(parsed["http5xxTag"], "http.server.status_code")
        with self.assertRaisesRegex(Invalid, "between 300 and 86400"):
            medical_config({**opts, "intervalSeconds": 299}, irc, [])
        with self.assertRaisesRegex(Invalid, "Invalid Sentry HTTP status tag"):
            medical_config({**opts, "http5xxTag": "http] :500"}, irc, [])
        parsed = medical_config({**opts, "permalinkHost": "Issues.Example.COM"}, irc, [])
        self.assertEqual(parsed["permalinkHost"], "issues.example.com")
        with self.assertRaisesRegex(Invalid, "Invalid host"):
            medical_config({**opts, "permalinkHost": "https://issues.example.com"}, irc, [])
        with self.assertRaisesRegex(Invalid, "medical member ID is reserved"):
            member({"id": "medical", "name": "Medical Captain", "account": "member-account"})

    def test_quota_families_only_select_providers(self):
        config = {"usage": {"families": {"codex": {}}}}
        self.assertEqual(integrations_config(config)["usage"]["providers"], ["codex"])
        config["usage"]["families"]["codex"] = {"limitAtPercent": 25}
        with self.assertRaisesRegex(Invalid, "unknown field"):
            integrations_config(config)


if __name__ == "__main__":
    main()
