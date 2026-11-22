"""Offline checks for pinning a captain's root OpenCode session, against a fake server.

Run with Python 3.10 or later: python3 -B subspace-installer/tests/sessions_test.py
"""

import base64
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import runtime  # noqa: E402
import sessions  # noqa: E402

DIRECTORY = "/var/lib/subspace-fleet/agents/orion/workspace/project"
PERMISSIONS = [{"action": "*", "resource": "*", "effect": "deny"},
               {"action": "stargate_crew", "resource": "*", "effect": "allow"}]
EXPECTED = {"directory": DIRECTORY, "title": "example-fleet/orion", "crew": ["claude-1", "codex-1"],
            "model": {"providerID": "openai-proxy", "id": "gpt-5.4"}, "permissions": PERMISSIONS}


class FakeOpenCode:
    """Stores sessions like OpenCode: a client-chosen ID, no model turn."""

    def __init__(self):
        self.sessions = {}
        self.creates = []
        self.updates = []
        self.fail_create = None

    def get(self, session_id):
        value = self.sessions.get(session_id)
        return json.loads(json.dumps(value)) if value else None

    def create(self, body):
        self.creates.append(body)
        if self.fail_create == "before":
            raise sessions.SessionError("OpenCode returned HTTP 503 when creating the captain session.")
        self.sessions[body["id"]] = {"id": body["id"], "agent": body["agent"], "location": body["location"],
                                     "model": body["model"], "permissions": body["permissions"]}
        if self.fail_create == "after":
            raise sessions.SessionError("The response was lost.")
        return self.get(body["id"])

    def update_permissions(self, session_id, permissions):
        self.updates.append(permissions)
        self.sessions[session_id]["permissions"] = permissions


def ready(client, session_id):
    return client.get(session_id)


class CaptainSessionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.record = sessions.SessionRecord(self.directory.name, "orion")
        self.server = FakeOpenCode()

    def tearDown(self):
        self.directory.cleanup()

    def ensure(self, expected=EXPECTED):
        return sessions.ensure(self.server, self.record, expected, wait=ready)

    def test_first_run_reserves_then_creates_and_reruns_change_nothing(self):
        session_id, new = self.ensure()
        self.assertTrue(new)
        self.assertRegex(session_id, r"^ses_[0-9a-f]{32}$")
        [body] = self.server.creates
        self.assertEqual(body, {"id": session_id, "title": "example-fleet/orion", "agent": "build",
                                "model": EXPECTED["model"], "location": {"directory": DIRECTORY},
                                "permissions": PERMISSIONS})
        record = json.loads(self.record.confirmed.read_text())
        self.assertEqual(record, {"sessionID": session_id, "directory": DIRECTORY, "agent": "build",
                                  "crew": ["claude-1", "codex-1"], "confirmed": True})
        self.assertEqual(self.record.confirmed.stat().st_mode & 0o777, 0o600)
        self.assertFalse(self.record.pending.exists())
        self.assertEqual(self.ensure(), (session_id, False))
        self.assertEqual(len(self.server.creates), 1)

    def test_an_interrupted_create_retries_the_same_reserved_id(self):
        self.server.fail_create = "before"
        with self.assertRaises(sessions.SessionError):
            self.ensure()
        reserved = json.loads(self.record.pending.read_text())["sessionID"]
        self.assertFalse(self.record.confirmed.exists())
        self.server.fail_create = None
        self.assertEqual(self.ensure(), (reserved, True))
        self.assertEqual([body["id"] for body in self.server.creates], [reserved, reserved])

    def test_a_create_that_was_stored_is_adopted_not_repeated(self):
        self.server.fail_create = "after"
        session_id, new = self.ensure()
        self.assertTrue(new)
        self.assertEqual(len(self.server.creates), 1)
        self.assertEqual(list(self.server.sessions), [session_id])

    def test_a_missing_confirmed_session_is_never_replaced(self):
        session_id, _ = self.ensure()
        del self.server.sessions[session_id]
        with self.assertRaisesRegex(sessions.SessionError, "does not replace a captain root"):
            self.ensure()
        self.assertEqual(len(self.server.creates), 1)

    def test_identity_directory_and_roster_changes_are_refused(self):
        session_id, _ = self.ensure()
        with self.assertRaisesRegex(sessions.SessionError, "first repository is fixed"):
            self.ensure({**EXPECTED, "directory": DIRECTORY.replace("project", "library")})
        with self.assertRaisesRegex(sessions.SessionError, "crew are fixed"):
            self.ensure({**EXPECTED, "crew": ["claude-1", "codex-1", "codex-2"]})
        self.server.sessions[session_id]["agent"] = "plan"
        with self.assertRaisesRegex(sessions.SessionError, "differs from its record"):
            self.ensure()
        self.server.sessions[session_id].update(agent="build", parentID="ses_parent")
        with self.assertRaisesRegex(sessions.SessionError, "differs from its record"):
            self.ensure()

    def test_changed_permissions_are_updated_on_the_same_root(self):
        session_id, _ = self.ensure()
        tighter = [*PERMISSIONS, {"action": "stargate_model", "resource": "get", "effect": "allow"}]
        self.assertEqual(self.ensure({**EXPECTED, "permissions": tighter}), (session_id, False))
        self.assertEqual(self.server.updates, [tighter])
        self.assertEqual(self.server.sessions[session_id]["permissions"], tighter)

    def test_render_reads_only_confirmed_records_that_still_fit(self):
        person = {"id": "orion", "crew": [{"id": "claude-1"}, {"id": "codex-1"}]}
        with patch.object(runtime, "SESSIONS", Path(self.directory.name)):
            self.server.fail_create = "before"
            with self.assertRaises(sessions.SessionError):
                self.ensure()
            self.assertIsNone(runtime.pinned_session(person, Path(DIRECTORY)))
            self.server.fail_create = None
            session_id, _ = self.ensure()
            self.assertEqual(runtime.pinned_session(person, Path(DIRECTORY)), session_id)
            self.assertIsNone(runtime.pinned_session({**person, "crew": []}, Path(DIRECTORY)))
            with self.assertRaisesRegex(runtime.Invalid, "Captain orion: .*crew are fixed"):
                runtime.pinned_session({**person, "crew": [{"id": "claude-1"}]}, Path(DIRECTORY))
            with self.assertRaisesRegex(runtime.Invalid, "Captain orion: .*first repository is fixed"):
                runtime.pinned_session(person, Path(DIRECTORY.replace("project", "library")))


class Response(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


class Opener:
    def __init__(self, *replies):
        self.replies = list(replies)
        self.requests = []

    def open(self, request, timeout):
        self.requests.append(request)
        reply = self.replies.pop(0)
        if isinstance(reply, Exception):
            raise reply
        return Response(json.dumps(reply).encode() if reply is not None else b"")


class OpenCodeClientTests(unittest.TestCase):
    def test_requests_use_basic_auth_and_the_v2_session_routes(self):
        opener = Opener(HTTPError("http://127.0.0.1:4097/api/session/x", 404, "Not Found", {}, io.BytesIO()),
                        {"data": {"id": "ses_a"}}, None)
        client = sessions.OpenCode("http://127.0.0.1:4097", "fixture-password", opener)
        self.assertIsNone(client.get("ses_a"))
        self.assertEqual(client.create({"id": "ses_a"}), {"id": "ses_a"})
        client.update_permissions("ses_a", PERMISSIONS)
        get, create, update = opener.requests
        self.assertEqual((get.get_method(), get.full_url), ("GET", "http://127.0.0.1:4097/api/session/ses_a"))
        self.assertEqual((create.get_method(), create.full_url), ("POST", "http://127.0.0.1:4097/api/session"))
        self.assertEqual(json.loads(create.data), {"id": "ses_a"})
        self.assertEqual((update.get_method(), json.loads(update.data)), ("PATCH", {"permissions": PERMISSIONS}))
        token = base64.b64encode(b"opencode:fixture-password").decode()
        self.assertTrue(all(request.get_header("Authorization") == f"Basic {token}" for request in opener.requests))

    def test_authentication_failures_are_errors_not_missing_sessions(self):
        opener = Opener(HTTPError("http://127.0.0.1:4097/api/session/x", 401, "Unauthorized", {}, io.BytesIO()))
        with self.assertRaisesRegex(sessions.SessionError, "HTTP 401"):
            sessions.OpenCode("http://127.0.0.1:4097", "wrong", opener).get("ses_a")

    def test_waiting_for_the_engine_retries_connections_then_gives_up(self):
        opener = Opener(URLError("refused"), URLError("refused"), {"data": {"id": "ses_a"}})
        client = sessions.OpenCode("http://127.0.0.1:4097", "fixture-password", opener)
        self.assertEqual(sessions.wait_until_ready(client, "ses_a", sleep=lambda _: None), {"id": "ses_a"})
        clock = iter([0, 0, 100])
        opener = Opener(URLError("refused"), URLError("refused"))
        client = sessions.OpenCode("http://127.0.0.1:4097", "fixture-password", opener)
        with self.assertRaisesRegex(sessions.SessionError, "did not answer"):
            sessions.wait_until_ready(client, "ses_a", timeout=10, sleep=lambda _: None, clock=lambda: next(clock))


if __name__ == "__main__":
    unittest.main()
