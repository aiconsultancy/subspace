"""Offline checks for pinning a captain's root OpenCode session, against a fake server.

Run with Python 3.10 or later: python3 -B subspace-installer/tests/sessions_test.py
"""

import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
from threading import Thread
import unittest
from unittest.mock import patch
from urllib.error import HTTPError, URLError

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "lib"))
import runtime  # noqa: E402
import sessions  # noqa: E402

# Tests never reach the network (policy_test.py checks the anonymous repository check itself).
runtime.readable_without_credentials = lambda url: True

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
        self.lookups = 0

    def get(self, session_id):
        self.lookups += 1
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

    def test_an_unconfirmed_reservation_follows_a_changed_first_repository(self):
        self.server.fail_create = "before"
        with self.assertRaises(sessions.SessionError):
            self.ensure()
        reserved = json.loads(self.record.pending.read_text())["sessionID"]
        self.server.fail_create = None
        library = DIRECTORY.replace("project", "library")
        self.assertEqual(self.ensure({**EXPECTED, "directory": library}), (reserved, True))
        self.assertEqual(self.server.sessions[reserved]["location"], {"directory": library})
        self.assertEqual(json.loads(self.record.confirmed.read_text())["directory"], library)

    def test_a_reservation_whose_root_exists_keeps_its_first_repository(self):
        # Interrupted after the create was stored but before the record was confirmed.
        reserved = sessions.new_session_id()
        self.record.save({"sessionID": reserved, "directory": DIRECTORY, "agent": "build",
                          "crew": EXPECTED["crew"], "confirmed": False})
        self.server.sessions[reserved] = {"id": reserved, "agent": "build", "location": {"directory": DIRECTORY},
                                          "model": EXPECTED["model"], "permissions": PERMISSIONS}
        with self.assertRaisesRegex(sessions.SessionError, "already exists in its original first repository"):
            self.ensure({**EXPECTED, "directory": DIRECTORY.replace("project", "library")})

    def test_a_changed_captain_model_is_reported_not_silently_ignored(self):
        session_id, _ = self.ensure()
        notices = []
        claude = {"providerID": "claude-proxy", "id": "claude-sonnet-5-5"}
        self.assertEqual(sessions.ensure(self.server, self.record, {**EXPECTED, "model": claude}, wait=ready,
                                         notice=notices.append), (session_id, False))
        [notice] = notices
        self.assertIn("keeps its model openai-proxy/gpt-5.4", notice)
        self.assertIn("claude-proxy/claude-sonnet-5-5", notice)
        self.assertEqual(self.server.sessions[session_id]["model"], EXPECTED["model"])
        notices.clear()
        sessions.ensure(self.server, self.record, EXPECTED, wait=ready, notice=notices.append)
        self.assertEqual(notices, [])

    def test_records_reach_the_disk_before_and_after_the_rename(self):
        events = []
        real_fsync, real_replace = os.fsync, os.replace

        def fsync(fd):
            events.append("fsync directory" if stat.S_ISDIR(os.fstat(fd).st_mode) else "fsync file")
            real_fsync(fd)

        def replace(source, target):
            events.append(f"replace {Path(target).name}")
            real_replace(source, target)

        with patch.object(sessions.os, "fsync", side_effect=fsync), patch.object(sessions.os, "replace", side_effect=replace):
            self.ensure()
        # The reservation and the confirmed record each reach the disk, then the pending file's removal does.
        self.assertEqual(events, ["fsync file", "replace orion.pending.json", "fsync directory",
                                  "fsync file", "replace orion.json", "fsync directory", "fsync directory"])

    def test_changed_permissions_are_updated_on_the_same_root(self):
        session_id, _ = self.ensure()
        tighter = [*PERMISSIONS, {"action": "stargate_model", "resource": "get", "effect": "allow"}]
        self.assertEqual(self.ensure({**EXPECTED, "permissions": tighter}), (session_id, False))
        self.assertEqual(self.server.updates, [tighter])
        self.assertEqual(self.server.sessions[session_id]["permissions"], tighter)

    def test_a_workspace_root_records_its_first_repository_as_the_project(self):
        workspace = DIRECTORY.rsplit("/", 1)[0]
        expected = {**EXPECTED, "directory": workspace, "project": DIRECTORY}
        session_id, _ = self.ensure(expected)
        self.assertEqual(self.server.sessions[session_id]["location"], {"directory": workspace})
        self.assertEqual(json.loads(self.record.confirmed.read_text())["project"], DIRECTORY)
        self.assertEqual(self.ensure(expected), (session_id, False))
        # The root does not work in the first repository, but Stargate's crew state still names it.
        with self.assertRaisesRegex(sessions.SessionError, "first repository is fixed"):
            self.ensure({**expected, "project": DIRECTORY.replace("project", "library")})
        person = {"id": "orion", "crew": [{"id": "claude-1"}, {"id": "codex-1"}]}
        with patch.object(runtime, "SESSIONS", Path(self.directory.name)):
            self.assertEqual(runtime.pinned_session(person, Path(workspace), Path(DIRECTORY)), session_id)
            with self.assertRaisesRegex(runtime.Invalid, "Captain orion: .*first repository is fixed"):
                runtime.pinned_session(person, Path(workspace), Path(DIRECTORY.replace("project", "library")))

    def test_a_record_from_before_the_workspace_root_is_its_own_project(self):
        session_id, _ = self.ensure()
        self.assertNotIn("project", json.loads(self.record.confirmed.read_text()))
        self.assertEqual(self.ensure({**EXPECTED, "project": DIRECTORY}), (session_id, False))
        with self.assertRaisesRegex(sessions.SessionError, "first repository is fixed"):
            self.ensure({**EXPECTED, "project": DIRECTORY.replace("project", "library")})

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
            # Zero crew or no repositories would drop the pin: refused, naming the recorded crew.
            with self.assertRaisesRegex(runtime.Invalid, r"Captain orion: .*crew are fixed .*configured: none"):
                runtime.pinned_session({**person, "crew": []}, Path(DIRECTORY))
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

    def test_the_loopback_client_ignores_proxy_settings_from_the_environment(self):
        seen = []

        class Engine(BaseHTTPRequestHandler):
            def do_GET(self):
                seen.append((self.path, self.headers.get("Authorization")))
                body = json.dumps({"data": {"id": "ses_a"}}).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_args):
                pass

        server = ThreadingHTTPServer(("127.0.0.1", 0), Engine)
        Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        # A proxy that would receive the captain's Basic auth header; nothing listens on port 9.
        proxies = {name: "http://127.0.0.1:9" for name in ("http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY")}
        with patch.dict(os.environ, {**proxies, "no_proxy": "", "NO_PROXY": ""}):
            client = sessions.OpenCode(f"http://127.0.0.1:{server.server_port}", "fixture-password")
            self.assertEqual(client.get("ses_a"), {"id": "ses_a"})
        token = base64.b64encode(b"opencode:fixture-password").decode()
        self.assertEqual(seen, [("/api/session/ses_a", f"Basic {token}")])

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



def secrets_for(*accounts):
    return {"proxyKey": "fixture-model-proxy-key",
            "ircPasswords": {account: "fixture-password" for account in accounts}}


def crew_fleet(*captains):
    config = json.loads((Path(__file__).resolve().parents[1] / "fleet.example.json").read_text())
    config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
    config["irc"]["auth"] = "password"
    config["captains"] = [{"id": name, "name": name.title(), "account": f"example-{name}"} for name in captains]
    secrets = secrets_for("example-atlas", *(f"example-{name}" for name in captains))
    return config, secrets


class InstalledFleet(unittest.TestCase):
    """Render and commission against temporary /etc and /var/lib trees, without root or systemd."""

    @classmethod
    def setUpClass(cls):
        # Real IRC certificate bundles, made with openssl before setUp replaces subprocess.run.
        cls.bundles = {account: runtime.new_irc_bundle(account) for account in ("example-atlas", "example-orion")}

    def setUp(self):
        self.root = tempfile.TemporaryDirectory()
        self.addCleanup(self.root.cleanup)
        root = Path(self.root.name)
        self.etc, self.state, self.systemd = root / "etc", root / "state", root / "systemd"
        for path in (self.etc, self.state, self.systemd):
            path.mkdir()
        self.events = []
        real_durable = sessions.write_durable

        def write(path, content, mode=0o600, group="root"):
            path.parent.mkdir(parents=True, exist_ok=True)
            if not path.exists() or path.read_text() != content:
                path.write_text(content)
                self.events.append(("write", path.name))

        def durable(path, text):
            self.events.append(("ready", path.name))
            real_durable(path, text)

        for name, value in (("ETC", self.etc), ("STATE", self.state), ("SYSTEMD", self.systemd),
                            ("SESSIONS", self.etc / "sessions")):
            self.start(patch.object(runtime, name, value))
        self.start(patch.object(runtime, "write", side_effect=write))
        self.start(patch.object(runtime.sessions, "write_durable", side_effect=durable))
        for name in ("directory", "run", "configure_git", "toolchains"):
            self.start(patch.object(runtime, name))
        self.start(patch.object(runtime.subprocess, "run"))
        self.start(patch.object(runtime, "account", side_effect=lambda user, home, mode=0o700: home))
        self.start(patch.object(runtime, "render_proxy", return_value="subspace-fleet-proxy.service"))
        self.start(patch.object(runtime, "render_view", return_value="subspace-fleet-view.service"))
        self.start(patch.object(runtime, "render_firewall", return_value=runtime.FIREWALL))
        self.start(patch.object(runtime, "maintenance_units", return_value=[]))
        self.start(patch.object(runtime, "reconcile_workspaces",
                                       side_effect=lambda cfg, homes: self.state / "references" / "atlas"))
        self.start(patch.object(runtime, "allocate_ports",
                                       side_effect=lambda cfg: {person["id"]: 4096 + index
                                                                for index, person in enumerate(cfg["members"])}))
        self.start(patch.object(sys, "stdout", io.StringIO()))
        self.store = sessions.SessionRecord(self.etc / "sessions", "orion")
        self.workspace = str(self.state / "agents/orion/workspace/project")

    def start(self, patcher):
        value = patcher.start()
        self.addCleanup(patcher.stop)
        return value

    def render(self, config, secrets):
        self.events.clear()
        runtime.render(runtime.validate(config, secrets), {"tokens": {}, "cloud": {}})

    def pin(self, captain="orion"):
        server = FakeOpenCode()
        expected = {**EXPECTED, "directory": str(self.state / f"agents/{captain}/workspace/project"),
                    "crew": ["claude-1", "claude-2", "codex-1", "codex-2"]}
        session_id, _ = sessions.ensure(server, sessions.SessionRecord(self.etc / "sessions", captain), expected,
                                        wait=ready)
        return session_id

    def gateway(self):
        config = json.loads((self.etc / "stargate.json").read_text())
        unit = (self.systemd / "subspace-fleet-gateway.service").read_text()
        return {entry["id"]: entry["backend"] for entry in config["irc"]}, unit

    def test_the_gateway_waits_for_a_rendered_pin_not_just_a_confirmed_record(self):
        config, secrets = crew_fleet("orion")
        marker = self.etc / "sessions/orion.ready"
        self.render(config, secrets)
        served, unit = self.gateway()
        # Unpinned, the captain is left out of the shared gateway, which still serves the rear admiral.
        self.assertEqual(list(served), ["atlas"])
        self.assertNotIn("ConditionPathExists", unit)
        self.assertNotIn("plugins", json.loads((self.etc / "opencode-orion.json").read_text()))
        self.assertFalse(marker.exists())

        session_id = self.pin()
        # Confirmed but not yet rendered (a reboot or failed re-render here): still not ready, still left out.
        self.assertFalse(marker.exists())
        self.assertEqual(list(self.gateway()[0]), ["atlas"])

        self.render(config, secrets)
        served, unit = self.gateway()
        self.assertEqual(served["orion"]["sessionID"], session_id)
        self.assertIn(f"ConditionPathExists={marker}", unit.splitlines())
        engine = json.loads((self.etc / "opencode-orion.json").read_text())
        self.assertEqual(engine["plugins"][0]["options"]["captainSessionID"], session_id)
        self.assertEqual(marker.read_text(), f"{session_id}\n")
        self.assertEqual(marker.stat().st_mode & 0o777, 0o600)
        # The marker follows every configuration that changed to name the pin.
        written = [name for kind, name in self.events if kind == "write"]
        self.assertEqual(self.events[-1], ("ready", "orion.ready"))
        for name in ("opencode-orion.json", "stargate.json", "gateway.json", "subspace-fleet-gateway.service"):
            self.assertIn(name, written)

        # A re-run with nothing changed rewrites nothing, the marker included.
        self.render(config, secrets)
        self.assertEqual(self.events, [])
        self.assertEqual(marker.read_text(), f"{session_id}\n")

    def test_a_marker_without_a_confirmed_record_is_withdrawn_before_anything_is_written(self):
        config, secrets = crew_fleet("orion")
        self.pin()
        self.render(config, secrets)
        marker = self.etc / "sessions/orion.ready"
        self.assertTrue(marker.exists())
        self.store.confirmed.unlink()
        real_unlink = Path.unlink

        def unlink(path, missing_ok=False):
            self.events.append(("unlink", path.name))
            real_unlink(path, missing_ok=missing_ok)

        with patch.object(Path, "unlink", unlink):
            self.render(config, secrets)
        self.assertFalse(marker.exists())
        self.assertEqual(self.events[0], ("unlink", "orion.ready"))
        self.assertEqual(list(self.gateway()[0]), ["atlas"])

    def test_zero_crew_or_no_repositories_cannot_drop_a_pin(self):
        config, secrets = crew_fleet("orion")
        self.pin()
        self.render(config, secrets)
        for change in ({"crew": {"claude": {"count": 0}, "codex": {"count": 0}}}, {"repositories": []}):
            with self.subTest(change=change):
                self.events.clear()
                with self.assertRaisesRegex(runtime.Invalid, "Captain orion: .*crew are fixed .*configured: none"):
                    runtime.render(runtime.validate({**config, **change}, secrets), {"tokens": {}, "cloud": {}})
                self.assertEqual(self.events, [])
        self.assertTrue((self.etc / "sessions/orion.ready").exists())

    def installed(self, config, secrets):
        """The installed inputs commission reads, and a fake engine per officer on its allocated port."""
        extra = {"tokens": {}, "cloud": {}}
        for name, value in (("fleet.json", config), ("secrets.json", secrets), ("extra-secrets.json", extra)):
            (self.etc / name).write_text(json.dumps(value))
            (self.etc / name).chmod(0o600)
        servers = {4096 + index: FakeOpenCode() for index in range(len(config["captains"]) + 1)}
        self.start(patch.object(runtime.sessions, "OpenCode",
                                side_effect=lambda url, _password: servers[int(url.rsplit(":", 1)[1])]))
        return servers

    def roots_record(self):
        return json.loads((self.etc / "session-roots.json").read_text())

    def engine_directory(self, captain="orion"):
        return json.loads((self.etc / f"engine-{captain}.json").read_text())["directory"]

    def test_a_new_captain_pins_its_root_in_the_workspace_above_its_checkouts(self):
        config, secrets = crew_fleet("orion")
        servers = self.installed(config, secrets)
        workspace = self.state / "agents/orion/workspace"
        self.render(config, secrets)
        self.assertEqual(self.roots_record(), {"orion": "workspace"})
        self.assertEqual(self.engine_directory(), str(workspace))
        # The workspace instructions are written as the captain, which owns the workspace.
        calls = [call for call in runtime.subprocess.run.call_args_list if "workspace-instructions" in call.args[0]]
        self.assertEqual(calls[-1].args[0][:3], ["runuser", "-u", "ss-orion"])
        self.assertIn(f"`{workspace}`", json.loads(calls[-1].kwargs["input"])["AGENTS.md"])

        runtime.commission(False)
        record = json.loads(self.store.confirmed.read_text())
        self.assertEqual((record["directory"], record["project"]), (str(workspace), str(workspace / "project")))
        self.assertEqual(servers[4097].sessions[record["sessionID"]]["location"], {"directory": str(workspace)})
        options = json.loads((self.etc / "opencode-orion.json").read_text())["plugins"][0]["options"]
        self.assertEqual((options["captainDirectory"], options["projectDirectory"], options["captainSessionID"]),
                         (str(workspace), str(workspace / "project"), record["sessionID"]))
        served, _ = self.gateway()
        self.assertEqual((served["orion"]["directory"], served["orion"]["sessionID"]), (str(workspace), record["sessionID"]))
        orion = next(entry for entry in json.loads((self.etc / "stargate.json").read_text())["irc"] if entry["id"] == "orion")
        self.assertEqual(orion["fleetPolicy"]["assignedCheckout"], str(workspace))
        # The rear admiral keeps its read-only reference checkout of the first repository.
        self.assertEqual(served["atlas"]["directory"], str(self.state / "references/atlas/project"))
        self.render(config, secrets)
        self.assertEqual(self.events, [])
        # The first repository stays fixed: Stargate's crew state names it as the default project.
        config["repositories"].insert(0, {"id": "library", "url": "https://github.com/example/library.git"})
        with self.assertRaisesRegex(runtime.Invalid, "Captain orion: .*first repository is fixed"):
            self.render(config, secrets)
        self.assertEqual(self.events, [])

    def test_an_upgrade_never_moves_a_session_pinned_in_the_first_repository(self):
        config, secrets = crew_fleet("orion")
        self.pin()  # pinned before the workspace root: in the first repository's checkout
        root = runtime.captain_roots(runtime.validate(config, secrets))["orion"]
        self.assertEqual((root["path"], root["mode"], root["pinned"]), (Path(self.workspace), "repository", True))
        self.assertIn("stays there: the installer never moves a pinned session", root["notice"])
        self.render(config, secrets)
        self.assertEqual(self.engine_directory(), self.workspace)
        options = json.loads((self.etc / "opencode-orion.json").read_text())["plugins"][0]["options"]
        self.assertEqual(options["captainDirectory"], self.workspace)
        self.assertEqual(self.roots_record(), {"orion": "repository"})
        # Asking for the workspace is refused before anything is written; asking to stay silences the notice.
        config["captains"][0]["sessionRoot"] = "workspace"
        with self.assertRaisesRegex(runtime.Invalid, "pinned in .*but its sessionRoot is workspace.*never moves"):
            self.render(config, secrets)
        self.assertEqual(self.events, [])
        config["captains"][0]["sessionRoot"] = "repository"
        self.assertIsNone(runtime.captain_roots(runtime.validate(config, secrets))["orion"]["notice"])
        self.render(config, secrets)
        self.assertEqual(self.events, [])

    def test_an_installed_captain_keeps_its_root_until_its_session_root_moves_it(self):
        config, secrets = crew_fleet("orion")
        config["captains"][0]["crew"] = {"claude": {"count": 0}, "codex": {"count": 0}}
        checkout = self.state / "agents/orion/workspace/project"
        checkout.mkdir(parents=True)  # installed by an installer without the workspace root
        root = runtime.captain_roots(runtime.validate(config, secrets))["orion"]
        self.assertIn("keeps starting its sessions in", root["notice"])
        self.render(config, secrets)
        self.assertEqual((self.engine_directory(), self.roots_record()), (str(checkout), {"orion": "repository"}))
        # Recorded, it stays there even though its checkout no longer tells.
        checkout.rmdir()
        self.assertEqual(runtime.captain_roots(runtime.validate(config, secrets))["orion"]["path"], checkout)
        # Moving it is an explicit setting, which says what happens to its conversations.
        config["captains"][0]["sessionRoot"] = "workspace"
        root = runtime.captain_roots(runtime.validate(config, secrets))["orion"]
        self.assertIn("moves from", root["notice"])
        self.assertIn("start new sessions there", root["notice"])
        self.render(config, secrets)
        self.assertEqual((self.engine_directory(), self.roots_record()), (str(checkout.parent), {"orion": "workspace"}))
        del config["captains"][0]["sessionRoot"]
        root = runtime.captain_roots(runtime.validate(config, secrets))["orion"]
        self.assertEqual((root["path"], root["notice"]), (checkout.parent, None))
        self.render(config, secrets)
        self.assertEqual(self.events, [])

    def test_the_dry_run_shows_each_captains_root_and_a_pinned_one_that_differs(self):
        config, secrets = crew_fleet("orion", "vega")
        self.pin("orion")
        self.events.clear()
        paths = []
        for name, value in (("fleet.json", config), ("fleet.secrets.json", secrets)):
            paths.append(Path(self.root.name) / name)
            paths[-1].write_text(json.dumps(value))
            paths[-1].chmod(0o600)
        with patch.object(sys, "stdout", io.StringIO()) as output:
            runtime.main(["plan", *map(str, paths), "", "false"])
        text = output.getvalue()
        self.assertIn(f"Captain orion's root session is pinned in {self.workspace}, its checkout of the first "
                      "repository.", text)
        self.assertIn("stays there: the installer never moves a pinned session", text)
        self.assertIn(f"Captain vega's sessions will start in {self.state}/agents/vega/workspace, the workspace above "
                      "its repositories.", text)
        self.assertEqual(self.events, [])
        self.assertFalse((self.etc / "session-roots.json").exists())

    def commission_setup(self, *captains):
        config, secrets = crew_fleet(*captains)
        for name, value in (("fleet.json", config), ("secrets.json", secrets), ("extra-secrets.json", {})):
            (self.etc / name).write_text(json.dumps(value))
            (self.etc / name).chmod(0o600)
        servers = {}
        for index, captain in enumerate(captains, 1):
            (self.etc / f"engine-{captain}.json").write_text(json.dumps({
                "port": 4096 + index, "directory": str(self.state / f"agents/{captain}/workspace/project"),
                "environment": {"OPENCODE_PASSWORD": "fixture-password"}}))
            servers[4096 + index] = FakeOpenCode()
        clients = patch.object(runtime.sessions, "OpenCode",
                               side_effect=lambda url, _password: servers[int(url.rsplit(":", 1)[1])])
        self.start(clients)
        return servers

    def test_one_failing_captain_does_not_hold_back_the_others(self):
        servers = self.commission_setup("orion", "vega")
        servers[4098].fail_create = "before"
        with patch.object(runtime, "render") as render, \
                self.assertRaisesRegex(runtime.Invalid, r"^Captain vega: .*HTTP 503.*keeps serving the rest"):
            runtime.commission(False)
        render.assert_called_once()
        self.assertTrue(self.store.confirmed.exists())
        self.assertFalse(sessions.SessionRecord(self.etc / "sessions", "vega").confirmed.exists())

    def test_a_failed_re_render_asks_for_no_restart(self):
        self.commission_setup("orion")
        pending = self.state / "pending"
        with patch.object(runtime, "render", side_effect=lambda *_args: pending.touch() or 1 / 0), \
                self.assertRaises(ZeroDivisionError):
            runtime.commission(False)
        self.assertFalse(pending.exists())
        self.assertTrue(self.store.confirmed.exists())
        self.assertFalse((self.etc / "sessions/orion.ready").exists())

    def test_the_pre_restart_pass_only_updates_pinned_roots_that_answer(self):
        servers = self.commission_setup("orion", "vega")
        with patch.object(runtime, "render"):
            runtime.commission(False)
        session_id = json.loads(self.store.confirmed.read_text())["sessionID"]
        servers[4097].sessions[session_id]["permissions"] = PERMISSIONS
        vega = sessions.SessionRecord(self.etc / "sessions", "vega")
        vega.confirmed.unlink()
        with patch.object(runtime, "render") as render:
            runtime.commission(False, existing=True)
        render.assert_not_called()
        self.assertEqual(servers[4097].sessions[session_id]["permissions"],
                         runtime.captain_permissions(cfg := runtime.validate(*crew_fleet("orion", "vega")),
                                                     cfg["members"][1]))
        # Unconfirmed captains are left to the full commission after the restart.
        self.assertEqual(len(servers[4098].creates), 1)

    def test_agent_dispatch_pins_the_rear_admiral_and_lets_captains_answer(self):
        config, secrets = crew_fleet("orion")
        config["api"] = {"url": "https://subspace.example.com"}
        config["agentDispatch"] = {"enabled": True, "pollIntervalSeconds": 60}
        secrets.update(apiToken="aaa.bbb.admin", rearAdmiralApiToken="aaa.bbb.rear")
        extra = {"tokens": {}, "cloud": {}}
        for name, value in (("fleet.json", config), ("secrets.json", secrets), ("extra-secrets.json", extra)):
            (self.etc / name).write_text(json.dumps(value))
            (self.etc / name).chmod(0o600)
        servers = {4096: FakeOpenCode(), 4097: FakeOpenCode()}
        for port, member, directory in ((4096, "atlas", "references/atlas/project"),
                                        (4097, "orion", "agents/orion/workspace/project")):
            (self.etc / f"engine-{member}.json").write_text(json.dumps({
                "port": port, "directory": str(self.state / directory),
                "environment": {"OPENCODE_PASSWORD": "fixture-password"}}))
        self.start(patch.object(runtime.sessions, "OpenCode",
                                side_effect=lambda url, _password: servers[int(url.rsplit(":", 1)[1])]))
        self.render(config, secrets)
        served, _ = self.gateway()
        self.assertNotIn("sessionID", served["atlas"])
        stargate = json.loads((self.etc / "stargate.json").read_text())
        self.assertEqual(stargate["agentDispatch"]["messaging"],
                         {"type": "irc", "registration": "atlas", "channel": "#example-fleet-command"})
        self.assertEqual(stargate["agentDispatch"]["pollIntervalSeconds"], 60)
        self.assertNotIn("aaa.bbb.rear", json.dumps(stargate))
        runner = json.loads((self.etc / "gateway.json").read_text())
        self.assertEqual(runner["environment"]["SUBSPACE_DISPATCH_TOKEN"], "aaa.bbb.rear")

        with patch.object(runtime, "directory") as directories:
            runtime.commission(False)
        rear_session = json.loads((self.etc / "sessions/atlas.json").read_text())["sessionID"]
        captain_session = json.loads(self.store.confirmed.read_text())["sessionID"]
        # The rear admiral's root is created with its own policy, no crew grants.
        self.assertEqual(servers[4096].sessions[rear_session]["permissions"],
                         runtime.validate(config, secrets)["members"][0]["harness"]["permissions"])
        self.assertIn(runtime.DISPATCH_GRANT, servers[4097].sessions[captain_session]["permissions"])
        stargate = json.loads((self.etc / "stargate.json").read_text())
        rear, orion = stargate["irc"]
        self.assertEqual(rear["backend"]["sessionID"], rear_session)
        self.assertEqual(orion["fleetControl"]["targets"],
                         [{"id": "atlas", "account": "example-atlas", "role": "rear_admiral",
                           "channel": "#example-fleet-orion", "sessionID": rear_session}])
        self.assertEqual(orion["fleetChannels"], {"home": "#example-fleet-orion", "command": "#example-fleet-command"})
        self.assertNotIn("fleetControl", rear)
        engine = json.loads((self.etc / "opencode-orion.json").read_text())
        self.assertEqual(engine["plugins"][1]["options"]["socketPath"], orion["fleetControl"]["socketPath"])
        unit = (self.systemd / "subspace-fleet-gateway.service").read_text()
        self.assertIn(f"ReadWritePaths={self.state}/fleet-control/orion", unit.splitlines())
        directories.assert_any_call(self.state / "fleet-control/orion", 0o2750, runtime.GATEWAY, "ss-orion")
        # A clean re-run changes nothing.
        self.render(config, secrets)
        self.assertEqual(self.events, [])

    def test_officers_slack_bots_reach_the_same_root_as_their_irc(self):
        config, secrets = crew_fleet("orion")
        for person, app in ((config["rearAdmiral"], "AFIXTURE1"), (config["captains"][0], "AFIXTURE2")):
            person["slack"] = {"appId": app, "users": ["UFIXTURE1"], "channels": ["CFIXTURE1"]}
        extra = {"tokens": {}, "cloud": {}, "slack": {
            agent: {"slack-app-token": f"xapp-1-fixture-{agent}", "slack-bot-token": f"xoxb-1-fixture-{agent}"}
            for agent in ("atlas", "orion")}}
        for name, value in (("fleet.json", config), ("secrets.json", secrets), ("extra-secrets.json", extra)):
            (self.etc / name).write_text(json.dumps(value))
            (self.etc / name).chmod(0o600)
        servers = {4096: FakeOpenCode(), 4097: FakeOpenCode()}
        for port, member, directory in ((4096, "atlas", "references/atlas/project"),
                                        (4097, "orion", "agents/orion/workspace/project")):
            (self.etc / f"engine-{member}.json").write_text(json.dumps({
                "port": port, "directory": str(self.state / directory),
                "environment": {"OPENCODE_PASSWORD": "fixture-password"}}))
        self.start(patch.object(runtime.sessions, "OpenCode",
                                side_effect=lambda url, _password: servers[int(url.rsplit(":", 1)[1])]))

        def bots():
            stargate = json.loads((self.etc / "stargate.json").read_text())
            return ({entry["id"]: entry["backend"] for entry in stargate["irc"]},
                    {bot["id"]: bot for bot in stargate.get("bots", [])})

        runtime.render(runtime.validate(config, secrets), extra)
        irc, slack = bots()
        # Before commissioning, the captain with crew is served on neither transport.
        self.assertEqual((list(irc), list(slack)), (["atlas"], ["fleet-slack-atlas"]))
        self.assertEqual(slack["fleet-slack-atlas"]["backend"], irc["atlas"])

        # Without agent dispatch, the rear admiral's own Slack bot is reason enough to pin its root.
        runtime.commission(False)
        rear_session = json.loads((self.etc / "sessions/atlas.json").read_text())["sessionID"]
        captain_session = json.loads(self.store.confirmed.read_text())["sessionID"]
        irc, slack = bots()
        self.assertEqual(list(slack), ["fleet-slack-atlas", "fleet-slack-orion"])
        for agent, session_id in (("atlas", rear_session), ("orion", captain_session)):
            bot = slack[f"fleet-slack-{agent}"]
            self.assertEqual(bot["backend"]["sessionID"], session_id)
            self.assertEqual(bot["backend"], irc[agent])
            self.assertEqual(bot["slack"]["appTokenEnv"], f"SLACK_APP_TOKEN_{agent.upper()}")
            self.assertTrue(bot["access"]["mentionOnly"])
        self.assertEqual(slack["fleet-slack-orion"]["backend"]["agent"], "build")
        self.assertNotIn("xoxb-", (self.etc / "stargate.json").read_text())
        environment = json.loads((self.etc / "gateway.json").read_text())["environment"]
        self.assertEqual(environment["SLACK_BOT_TOKEN_ORION"], "xoxb-1-fixture-orion")
        self.assertEqual(environment["SLACK_APP_TOKEN_ATLAS"], "xapp-1-fixture-atlas")

        # Removing the captain's Slack settings removes its bot and its tokens from the gateway; IRC keeps its root.
        del config["captains"][0]["slack"]
        runtime.render(runtime.validate(config, secrets), extra)
        irc, slack = bots()
        self.assertEqual(list(slack), ["fleet-slack-atlas"])
        self.assertEqual(irc["orion"]["sessionID"], captain_session)
        environment = json.loads((self.etc / "gateway.json").read_text())["environment"]
        self.assertFalse({"SLACK_APP_TOKEN_ORION", "SLACK_BOT_TOKEN_ORION"} & environment.keys())
        del config["rearAdmiral"]["slack"]
        runtime.render(runtime.validate(config, secrets), extra)
        irc, slack = bots()
        self.assertEqual(slack, {})
        # Like turning agent dispatch off, the rear admiral's IRC goes back to a session per conversation.
        self.assertNotIn("sessionID", irc["atlas"])

    def test_a_rear_admiral_pinned_by_its_slack_bot_before_any_repository_keeps_its_root(self):
        # Agent dispatch needs a captain with crew, so it never pinned a rear admiral without repositories; the
        # rear admiral's own Slack bot does. Adding repositories later must neither move that root nor be refused.
        config, secrets = crew_fleet("orion")
        config["repositories"] = []
        config["rearAdmiral"]["slack"] = {"appId": "AFIXTURE1", "users": ["UFIXTURE1"], "channels": ["CFIXTURE1"]}
        extra = {"tokens": {}, "cloud": {}, "slack": {"atlas": {
            "slack-app-token": "xapp-1-fixture-atlas", "slack-bot-token": "xoxb-1-fixture-atlas"}}}
        for name, value in (("fleet.json", config), ("secrets.json", secrets), ("extra-secrets.json", extra)):
            (self.etc / name).write_text(json.dumps(value))
            (self.etc / name).chmod(0o600)
        self.start(patch.object(runtime, "reconcile_workspaces", side_effect=lambda cfg, homes: (
            self.state / "references/atlas" if cfg["repositories"] else None)))
        server = FakeOpenCode()
        self.start(patch.object(runtime.sessions, "OpenCode", return_value=server))
        workspace = self.state / "agents/atlas/workspace"
        runtime.render(runtime.validate(config, secrets), extra)
        runtime.commission(False)
        record = json.loads((self.etc / "sessions/atlas.json").read_text())
        self.assertEqual(record["directory"], str(workspace))
        self.assertEqual(server.sessions[record["sessionID"]]["location"], {"directory": str(workspace)})

        def served():
            stargate = json.loads((self.etc / "stargate.json").read_text())
            return (next(entry["backend"] for entry in stargate["irc"] if entry["id"] == "atlas"),
                    next(bot["backend"] for bot in stargate["bots"] if bot["id"] == "fleet-slack-atlas"))

        config["repositories"] = [{"id": "project", "url": "https://github.com/example/project.git"}]
        cfg = runtime.validate(config, secrets)
        self.assertEqual(runtime.rear_pin(cfg), record["sessionID"])
        runtime.render(cfg, extra)
        irc, bot = served()
        self.assertEqual((irc["directory"], irc["sessionID"]), (str(workspace), record["sessionID"]))
        self.assertEqual(bot, irc)
        self.assertEqual(json.loads((self.etc / "engine-atlas.json").read_text())["directory"], str(workspace))
        # Without a root to keep, the rear admiral works in its reference checkout of the first repository.
        del config["rearAdmiral"]["slack"]
        self.assertEqual(runtime.rear_workspace(runtime.validate(config, secrets)),
                         self.state / "references/atlas/project")
        # A root pinned in a reference checkout is fixed, and the refusal names the rear admiral, not a captain.
        config["rearAdmiral"]["slack"] = {"appId": "AFIXTURE1", "users": ["UFIXTURE1"], "channels": ["CFIXTURE1"]}
        record["directory"] = str(self.state / "references/atlas/project")
        (self.etc / "sessions/atlas.json").write_text(json.dumps(record))
        config["repositories"].insert(0, {"id": "library", "url": "https://github.com/example/library.git"})
        with self.assertRaisesRegex(runtime.Invalid, "^Rear admiral atlas: its first repository is fixed"):
            runtime.rear_pin(runtime.validate(config, secrets))

    def combined(self, config):
        """The four installer features together: IRC certificates (the remote default), Slack bots for the rear
        admiral and the captain, repository notes, and the API block subspace-setup writes, which is the same
        whether the VM's API keeps its own PostgreSQL or uses an existing one (--database-url-file)."""
        config.pop("irc", None)
        config["irc"] = {"mode": "remote", "host": "irc.example.com", "allowedAccounts": ["operator"]}
        config["repositories"] = [
            {"id": "project", "url": "https://github.com/example/project.git", "description": "The example service.",
             "tracker": "Example issues at https://tracker.example.com/project"},
            {"id": "library", "url": "https://github.com/example/library.git"},
        ]
        config["api"] = {"url": "http://127.0.0.1:3000", "initiatives": ["00000000-0000-4000-8000-000000000001"],
                         "agentUrl": "http://127.0.0.1:3001"}
        for person, app in ((config["rearAdmiral"], "AFIXTURE1"), (config["captains"][0], "AFIXTURE2")):
            person["slack"] = {"appId": app, "users": ["UFIXTURE1"], "channels": ["CFIXTURE1"]}
        secrets = {"proxyKey": "fixture-model-proxy-key", "apiToken": "aaa.bbb.admin"}
        extra = {"tokens": {}, "cloud": {}, "slack": {
            agent: {"slack-app-token": f"xapp-1-fixture-{agent}", "slack-bot-token": f"xoxb-1-fixture-{agent}"}
            for agent in ("atlas", "orion")}}
        for name, value in (("fleet.json", config), ("secrets.json", secrets), ("extra-secrets.json", extra)):
            (self.etc / name).write_text(json.dumps(value))
            (self.etc / name).chmod(0o600)
        certificates = self.etc / "irc-certs"
        certificates.mkdir(exist_ok=True)
        self.start(patch.object(runtime, "IRC_CERTS", certificates))
        self.start(patch.object(runtime, "new_irc_bundle", side_effect=lambda account: self.bundles[account]))
        self.start(patch.object(runtime.pwd, "getpwnam", return_value=runtime.pwd.getpwuid(os.getuid())))
        servers = {4096: FakeOpenCode(), 4097: FakeOpenCode()}
        self.start(patch.object(runtime.sessions, "OpenCode",
                                side_effect=lambda url, _password: servers[int(url.rsplit(":", 1)[1])]))
        return secrets, extra, servers

    def stargate(self):
        stargate = json.loads((self.etc / "stargate.json").read_text())
        return ({entry["id"]: entry for entry in stargate["irc"]}, {bot["id"]: bot for bot in stargate.get("bots", [])},
                json.loads((self.etc / "gateway.json").read_text())["environment"])

    def provisioned(self, verb, user="ss-orion"):
        """The last documents render handed to a runtime.py verb run as an officer."""
        calls = [call for call in runtime.subprocess.run.call_args_list
                 if verb in call.args[0] and call.args[0][:3] == ["runuser", "-u", user]]
        return json.loads(calls[-1].kwargs["input"])

    def test_certificates_slack_a_workspace_root_and_the_vms_api_render_one_coherent_fleet(self):
        config, _ = crew_fleet("orion")
        secrets, extra, servers = self.combined(config)
        cfg = runtime.validate(config, secrets)
        self.assertEqual((cfg["irc"]["auth"], cfg["passwords"]), ("certificate", {}))
        runtime.render(cfg, extra)
        runtime.commission(False)

        workspace = self.state / "agents/orion/workspace"
        reference = self.state / "references/atlas/project"
        record = json.loads(self.store.confirmed.read_text())
        rear_record = json.loads((self.etc / "sessions/atlas.json").read_text())
        irc, bots, environment = self.stargate()
        # One root per officer, named alike by its session record, its IRC registration and its Slack bot.
        self.assertEqual((record["directory"], record["project"]), (str(workspace), str(workspace / "project")))
        self.assertEqual(servers[4097].sessions[record["sessionID"]]["location"], {"directory": str(workspace)})
        self.assertEqual(rear_record["directory"], str(reference))
        for agent, session, directory in (("atlas", rear_record, reference), ("orion", record, workspace)):
            backend = irc[agent]["backend"]
            self.assertEqual((backend["directory"], backend["sessionID"]), (str(directory), session["sessionID"]))
            self.assertEqual(bots[f"fleet-slack-{agent}"]["backend"], backend)
            self.assertEqual(irc[agent]["fleetPolicy"]["assignedCheckout"], str(directory))
            # Certificates, not passwords: the registration names the bundle, and no IRC password reaches Stargate.
            self.assertEqual(irc[agent]["clientPemFile"], str(self.etc / f"irc-certs/{agent}.pem"))
            self.assertNotIn("passwordEnv", irc[agent])
            self.assertEqual(stat.S_IMODE((self.etc / f"irc-certs/{agent}.pem").stat().st_mode), 0o600)
        self.assertFalse({"IRC_ATLAS", "IRC_ORION"} & environment.keys())
        self.assertEqual((environment["SLACK_BOT_TOKEN_ATLAS"], environment["SLACK_BOT_TOKEN_ORION"]),
                         ("xoxb-1-fixture-atlas", "xoxb-1-fixture-orion"))
        options = json.loads((self.etc / "opencode-orion.json").read_text())
        self.assertEqual(options["plugins"][0]["options"]["captainDirectory"], str(workspace))
        self.assertEqual(options["mcp"]["servers"][runtime.SUBSPACE_MCP]["url"], "http://127.0.0.1:3001/mcp")

        # The captain's instructions agree with that root, its repositories' trackers and its Slack bot.
        role = self.provisioned("role")["instructions"]
        agents = self.provisioned("workspace-instructions")["AGENTS.md"]
        self.assertIn("s=slack;c=<channel>", role)
        self.assertIn("Your sessions start here, so one session reaches every repository below.", agents)
        self.assertIn("- Tracker: Example issues at https://tracker.example.com/project", agents)
        self.assertIn("- Tracker: the fleet's Subspace tracker, through your `subspace` tools", agents)
        self.assertIn("- Slack: people can also reach you through your own Slack bot", agents)
        self.assertIn(f"`{self.state}/agents/orion/worktrees/<crew ID>/library`", agents)
        # A clean re-run changes nothing, the certificates included.
        bundles = {path.name: path.read_text() for path in (self.etc / "irc-certs").iterdir()}
        self.events.clear()
        runtime.render(runtime.validate(config, secrets), extra)
        self.assertEqual(self.events, [])
        self.assertEqual({path.name: path.read_text() for path in (self.etc / "irc-certs").iterdir()}, bundles)

    def test_an_upgrade_that_brings_several_new_settings_at_once(self):
        # Installed by an earlier installer: remote IRC with officer passwords, the captain pinned in its first
        # repository's checkout.
        config, old_secrets = crew_fleet("orion")
        del config["irc"]["auth"]
        with self.assertRaisesRegex(runtime.Invalid, "Remove their entries from ircPasswords, or set irc.auth to "
                                    "password"):
            runtime.validate(config, old_secrets)
        # The operator moves to certificates, adds Slack bots and repository notes, and asks for the workspace
        # root at once. The pinned root cannot move, so the run stops before it writes anything.
        secrets, extra, servers = self.combined(config)
        expected = {**EXPECTED, "directory": self.workspace, "crew": ["claude-1", "claude-2", "codex-1", "codex-2"]}
        session_id, _ = sessions.ensure(servers[4097], self.store, expected, wait=ready)
        config["repositories"] = config["repositories"][:1]
        config["captains"][0]["sessionRoot"] = "workspace"
        (self.state / "slug").write_text("example-fleet\n")
        self.events.clear()
        with self.assertRaisesRegex(runtime.Invalid, "pinned in .*but its sessionRoot is workspace"):
            runtime.render(runtime.validate(config, secrets), extra)
        self.assertEqual(self.events, [])
        self.assertEqual(list((self.etc / "irc-certs").iterdir()), [])
        self.assertFalse((self.etc / "session-roots.json").exists())

        # Without the move, everything else applies, and every part names the root where the session is pinned.
        del config["captains"][0]["sessionRoot"]
        cfg = runtime.validate(config, secrets)
        self.assertIn("stays there", runtime.captain_roots(cfg)["orion"]["notice"])
        runtime.render(cfg, extra)
        runtime.commission(False)
        irc, bots, _ = self.stargate()
        self.assertEqual((irc["orion"]["backend"]["directory"], irc["orion"]["backend"]["sessionID"]),
                         (self.workspace, session_id))
        self.assertEqual(bots["fleet-slack-orion"]["backend"], irc["orion"]["backend"])
        self.assertEqual(irc["orion"]["clientPemFile"], str(self.etc / "irc-certs/orion.pem"))
        self.assertIn("Your sessions start in `project/`, your checkout of the first repository",
                      self.provisioned("workspace-instructions")["AGENTS.md"])
        self.assertEqual(self.roots_record(), {"orion": "repository"})
        # The rear admiral's new Slack bot pinned its root, in its reference checkout.
        self.assertEqual(irc["atlas"]["backend"]["sessionID"],
                         json.loads((self.etc / "sessions/atlas.json").read_text())["sessionID"])
        self.assertEqual(bots["fleet-slack-atlas"]["backend"], irc["atlas"]["backend"])


if __name__ == "__main__":
    unittest.main()
