"""Offline tests for the local agent token helper."""
import base64
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
import http.client
from http.server import BaseHTTPRequestHandler, HTTPServer
import io
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from urllib.parse import parse_qs, urlsplit

SCRIPT = Path(__file__).resolve().parents[2] / "agent-skills/bin/subspace_token.py"
sys.path.insert(0, str(SCRIPT.parent))
import subspace_token as token

HOSTED = token.Config("https://api.example.com", "workos", "client_EXAMPLE", "https://auth.example.com")


def jwt(expires):
    claims = {"exp": expires, "sub": "user_EXAMPLE", "org_id": "org_EXAMPLE"}
    payload = base64.urlsafe_b64encode(json.dumps(claims).encode()).decode().rstrip("=")
    return f"fixture.{payload}.fixture"


@contextmanager
def local_http(handler):
    """Use loopback TCP where allowed and HTTP over socket pairs in restricted test sandboxes."""
    server = HTTPServer(("127.0.0.1", 0), handler, bind_and_activate=False)
    with server:
        try:
            server.server_bind()
            server.server_activate()
        except PermissionError:
            server.server_port = 8000
            workers = []

            def connect(connection):
                connection.sock, peer = socket.socketpair()

                def handle():
                    with peer:
                        server.finish_request(peer, ("127.0.0.1", 0))

                worker = threading.Thread(target=handle, daemon=True)
                workers.append(worker)
                worker.start()

            try:
                with patch.object(http.client.HTTPConnection, "connect", connect):
                    yield server
            finally:
                for worker in workers:
                    worker.join(timeout=5)
        else:
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            try:
                yield server
            finally:
                server.shutdown()
                worker.join()


class FixtureApi(BaseHTTPRequestHandler):
    wbufsize = -1

    def log_message(self, *_):
        pass

    def send_json(self, status, payload):
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(payload).encode())

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        self.server.requests.append((self.path, body, self.headers.get("Authorization")))
        if self.path == "/api/v1/auth/dev/token":
            self.send_json(200, {"accessToken": "fixture-dev", "expiresIn": 3600})
        elif self.path == "/mcp":
            self.send_json(200, {"jsonrpc": "2.0", "id": body["id"], "result": {}})
        elif self.path != "/user_management/authenticate" or self.server.reject:
            self.send_json(400, {"error": "fixture refusal"})
        elif body["grant_type"] == "authorization_code":
            self.send_json(200, {"access_token": jwt(int(time.time()) + 60), "refresh_token": "fixture-first"})
        elif body["refresh_token"] == "fixture-first":
            self.send_json(200, {"access_token": jwt(int(time.time()) + 600), "refresh_token": "fixture-rotated"})
        else:
            self.send_json(400, {"error": "fixture rotation"})

    def do_GET(self):
        self.server.requests.append((self.path, None, self.headers.get("Authorization")))
        self.send_json(200, {"displayName": "Example Person", "email": "developer@example.com"})


class TokenTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.home = Path(temporary.name)
        self.env = {key: value for key, value in os.environ.items() if not key.startswith("SUBSPACE_")}
        self.env.update(SUBSPACE_HOME=str(self.home), no_proxy="127.0.0.1")
        for fixture in (
            patch.dict(os.environ, self.env, clear=True),
            patch.object(token, "HOME", self.home),
            patch.object(token.SessionStore, "keychain", return_value=None),
        ):
            fixture.start()
            self.addCleanup(fixture.stop)

    def write_config(self, **settings):
        (self.home / "agent.json").write_text(json.dumps(settings))

    def run_helper(self, *args):
        return subprocess.run([sys.executable, "-B", str(SCRIPT), *args], capture_output=True, text=True, timeout=10)

    def invoke(self, *args):
        output = io.StringIO()
        with patch.object(sys, "argv", [str(SCRIPT), *args]), patch.object(sys, "stdout", output):
            token.main()
        return output.getvalue()

    def test_config_file_defaults_and_environment_precedence(self):
        self.assertEqual(token.load_config(), token.Config("http://127.0.0.1:3000", "dev"))
        self.write_config(
            identity_provider="workos", api_url="https://api.example.com/api/",
            workos_client_id="client_EXAMPLE", workos_api_url="https://auth.example.com/",
            workos_redirect_port=8787,
        )
        self.assertEqual(token.load_config(), token.Config(**{**HOSTED.__dict__, "workos_redirect_port": 8787}))
        with patch.dict(os.environ, SUBSPACE_API_URL="https://other.example.com/",
                        SUBSPACE_WORKOS_API_URL="", SUBSPACE_WORKOS_REDIRECT_PORT="0"):
            config = token.load_config()
            self.assertEqual(config.api_url, "https://other.example.com")
            self.assertEqual(config.workos_api_url, token.DEFAULT_WORKOS_URL)
            self.assertEqual(config.workos_redirect_port, 0)
        with patch.dict(os.environ, SUBSPACE_API_URL="http://api.example.com"):
            with self.assertRaisesRegex(token.TokenError, "api_url"):
                token.load_config()
        with patch.dict(os.environ, SUBSPACE_IDENTITY_PROVIDER="dev"):
            self.assertEqual(token.load_config().identity_provider, "dev")

    def test_pkce_and_authorisation_url(self):
        verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"
        redirect = "http://127.0.0.1:8787/callback"
        url = urlsplit(token.authorisation_url(HOSTED, redirect, verifier, "state+/"))
        self.assertEqual((url.netloc, url.path), ("auth.example.com", "/user_management/authorize"))
        self.assertEqual(parse_qs(url.query), {
            "client_id": ["client_EXAMPLE"], "redirect_uri": [redirect],
            "response_type": ["code"], "provider": ["authkit"], "state": ["state+/"],
            "code_challenge": ["E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"],
            "code_challenge_method": ["S256"],
        })

    def test_callback_refuses_wrong_or_repeated_state_before_accepting_the_code(self):
        with local_http(token.Callback) as server:
            server.state, server.result = "expected", None
            for path, status in [
                ("/wrong?code=fixture&state=expected", 404),
                ("/callback?code=fixture&state=wrong", 400),
                ("/callback?code=fixture&state=expected&state=expected", 400),
                ("/callback?code=fixture&state=expected", 200),
            ]:
                conn = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=3)
                try:
                    conn.request("GET", path)
                    response = conn.getresponse()
                    self.assertEqual(response.status, status)
                    self.assertNotIn(b"fixture", response.read())
                    self.assertEqual(server.result, "fixture" if status == 200 else None)
                finally:
                    conn.close()

    def test_exchange_rotates_once_when_two_agents_refresh_then_forwards_mcp(self):
        with local_http(FixtureApi) as server:
            server.requests, server.reject = [], False
            url = f"http://127.0.0.1:{server.server_port}"
            config = token.Config(url, "workos", "client_EXAMPLE", url)
            store = token.SessionStore(config)
            with store.locked():
                session = token.authenticate(config, grant_type="authorization_code", code="fixture-code",
                                             code_verifier="fixture-verifier")
                store.save(session)
            self.assertEqual(server.requests[0], ("/user_management/authenticate", {
                "grant_type": "authorization_code", "client_id": "client_EXAMPLE",
                "code": "fixture-code", "code_verifier": "fixture-verifier",
            }, None))
            with ThreadPoolExecutor(max_workers=2) as workers:
                a, b = workers.map(lambda _: token.workos_token(config, token.SessionStore(config)), range(2))
            self.assertEqual(a, b)
            self.assertNotEqual(a, session["access_token"])
            self.assertEqual(len(server.requests), 2)
            self.assertEqual(server.requests[1], ("/user_management/authenticate", {
                "client_id": "client_EXAMPLE", "grant_type": "refresh_token",
                "refresh_token": "fixture-first", "organization_id": "org_EXAMPLE",
            }, None))
            self.assertEqual(store.read()["refresh_token"], "fixture-rotated")
            message = {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}
            output = io.StringIO()
            with patch.object(sys, "stdin", io.TextIOWrapper(io.BytesIO(json.dumps(message).encode()))), \
                    patch.object(sys, "stdout", output):
                token.mcp_stdio(config, store)
            self.assertEqual(json.loads(output.getvalue()), {"jsonrpc": "2.0", "id": 1, "result": {}})
            self.assertEqual(server.requests[-1], ("/mcp", message, f"Bearer {a}"))
            self.assertEqual(len(server.requests), 3)
            store.save({**store.read(), "expires": int(time.time()) - 1})
            server.reject = True
            with self.assertRaisesRegex(token.TokenError, "Run subspace_token.py --login"):
                token.workos_token(config, store)

    def test_file_storage_is_private_and_scoped_and_logout_forgets_it(self):
        store = token.SessionStore(HOSTED)
        with store.locked():
            store.save({"refresh_token": "fixture-first"})
            store.save({"refresh_token": "fixture-rotated"})
            self.assertEqual(store.read(), {"refresh_token": "fixture-rotated"})
            self.assertIsNone(token.SessionStore(token.Config(
                "https://other.example.com", "workos", HOSTED.workos_client_id, HOSTED.workos_api_url,
            )).read())
            if os.name != "nt":
                self.assertEqual(stat.S_IMODE(store.directory.stat().st_mode), 0o700)
                self.assertEqual(stat.S_IMODE(store.file.stat().st_mode), 0o600)
                self.assertEqual(stat.S_IMODE(store.file.with_suffix(".lock").stat().st_mode), 0o600)
            with patch.object(token.sys, "platform", "fixture-posix"):
                store.clear()
            self.assertIsNone(store.read())
            self.assertNotIn(b"fixture-rotated", store.file.read_bytes() if store.file.exists() else b"")

    def test_keychain_is_preferred_and_a_file_can_hold_a_rotated_grant(self):
        store = token.SessionStore(HOSTED)
        old = {"refresh_token": "fixture-first"}
        new = {"refresh_token": "fixture-rotated"}
        success = subprocess.CompletedProcess([], 0, b"", b"")
        chain_read = subprocess.CompletedProcess([], 0, b'{"refresh_token":"fixture-first"}\n', b"")
        with store.locked(), patch.object(store, "keychain", side_effect=[success, chain_read, chain_read, None]):
            store.save(old)
            self.assertFalse(store.file.exists())
            self.assertEqual(store.read(), old)
            store.save(new)
            self.assertEqual(store.read(), new)

    def test_login_saves_the_grant_and_me_uses_the_current_access_token(self):
        with local_http(FixtureApi) as server:
            server.requests, server.reject = [], False
            url = f"http://127.0.0.1:{server.server_port}"
            self.write_config(identity_provider="workos", api_url=url, workos_api_url=url, workos_client_id="client_EXAMPLE")
            with patch.object(token, "browser_code", return_value="fixture-code") as browser:
                self.assertEqual(self.invoke("--login"), "Signed in to WorkOS.\n")
                self.assertGreaterEqual(len(browser.call_args.args[1]), 43)
            header = json.loads(self.invoke("--header"))
            self.assertEqual(self.invoke("--me"), "Example Person <developer@example.com>\n")
            self.assertEqual(server.requests[-1], ("/api/v1/me", None, header["Authorization"]))

    def test_missing_session_gives_one_line_without_opening_browser(self):
        self.write_config(identity_provider="workos", api_url=HOSTED.api_url, workos_client_id=HOSTED.workos_client_id)
        # Keep the test off the real Keychain as well as off the network.
        with patch.object(sys, "argv", [str(SCRIPT), "--header"]), patch.object(token.webbrowser, "open") as open_browser:
            with self.assertRaisesRegex(token.TokenError, "^WorkOS session unavailable. Run subspace_token.py --login.$"):
                token.main()
        open_browser.assert_not_called()
        store = token.SessionStore(token.load_config())
        with store.locked():
            store.write_file(b"null")
        result = self.run_helper("--header")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "subspace: WorkOS session unavailable. Run subspace_token.py --login.\n")
        result = self.run_helper("--header", "--mcp-endpoint", "https://other.example.com/mcp")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("MCP URL changed", result.stderr)

    def test_a_named_settings_file_binds_the_api_and_its_own_session(self):
        # A deployment repository's agent-kit/agent.json wins over ~/.subspace/agent.json and the environment.
        self.write_config(identity_provider="workos", api_url="https://other.example.com", workos_client_id="client_OTHER")
        bound = self.home / "example-deployment" / "agent.json"
        bound.parent.mkdir()
        bound.write_text(json.dumps({"identity_provider": "workos", "api_url": HOSTED.api_url,
                                     "workos_client_id": HOSTED.workos_client_id}))
        with patch.dict(os.environ, SUBSPACE_API_URL="https://elsewhere.example.com"):
            config = token.load_config(str(bound))
        self.assertEqual((config.api_url, config.workos_client_id), (HOSTED.api_url, HOSTED.workos_client_id))
        with self.assertRaisesRegex(token.TokenError, "Cannot find"):
            token.load_config(str(bound.with_name("missing.json")))
        store = token.SessionStore(config)
        with store.locked():
            store.write_file(b"null")  # signed out for this API, and off the real Keychain
        result = self.run_helper("--config", str(bound), "--header", "--mcp-endpoint", f"{HOSTED.api_url}/mcp")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stderr, f"subspace: WorkOS session unavailable. Run subspace_token.py --config {bound} --login.\n")
        result = self.run_helper("--config", str(bound), "--header", "--mcp-endpoint", "https://other.example.com/mcp")
        self.assertIn(f"not the one in {bound}", result.stderr)
        with store.locked():
            store.save({"access_token": "fixture-bound", "refresh_token": "fixture", "expires": int(time.time()) + 600})
        self.assertEqual(self.invoke("--config", str(bound), "--header"), '{"Authorization": "Bearer fixture-bound"}\n')
        message = {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}
        output = io.StringIO()
        with patch.object(sys, "stdin", io.TextIOWrapper(io.BytesIO(json.dumps(message).encode()))), \
                patch.object(sys, "stdout", output), patch.object(token, "workos_token",
                                                                  side_effect=token.TokenError(token.SIGN_IN_REQUIRED)):
            token.mcp_stdio(config, store, str(bound))
        self.assertIn(f"--config {bound} --login", json.loads(output.getvalue())["error"]["message"])

    def test_a_loopback_api_is_never_reached_through_a_proxy(self):
        def proxied(url):
            return [handler.proxies for handler in token.opener(url).handlers if isinstance(handler, token.ProxyHandler)]

        with patch.dict(os.environ, http_proxy="http://127.0.0.1:9", https_proxy="http://127.0.0.1:9"):
            os.environ.pop("no_proxy", None)
            # An opener keeps only handlers that open something: none carries a proxy for this computer.
            for url in ("http://127.0.0.1:3000", "http://[::1]:3000", "http://localhost:3000"):
                self.assertEqual(proxied(url), [], url)
            self.assertIn("https", proxied("https://api.example.com")[0])
            with local_http(FixtureApi) as server:
                server.requests, server.reject = [], False
                url = f"http://127.0.0.1:{server.server_port}"
                config = token.Config(url, "workos", "client_EXAMPLE", url)
                self.assertEqual(token.call(config, "GET", "/me", token="fixture-access")["displayName"], "Example Person")
            self.assertEqual(server.requests, [("/api/v1/me", None, "Bearer fixture-access")])

    def test_development_login_and_token_behave_as_before(self):
        self.assertEqual(self.run_helper("--login").stdout, "Development sign-in needs no login.\n")
        target = self.home / "targets" / "example-fleet"
        target.mkdir(parents=True)
        (target / "summary.env").write_text("SUBSPACE_ADMIN_EMAIL=developer@example.com\nSUBSPACE_ORGANISATION=example\n")
        (self.home / "agents.env").write_text("SUBSPACE_TARGET=example-fleet\n")
        with local_http(FixtureApi) as server:
            server.requests, server.reject = [], False
            with patch.dict(os.environ, SUBSPACE_API_URL=f"http://127.0.0.1:{server.server_port}/api/"):
                for args, expected in [
                    ([], "fixture-dev\n"),
                    (["--header"], '{"Authorization": "Bearer fixture-dev"}\n'),
                    (["--me"], "Example Person <developer@example.com>\n"),
                ]:
                    self.assertEqual(self.invoke(*args), expected)
            self.assertEqual(server.requests, [
                ("/api/v1/auth/dev/token", {"email": "developer@example.com", "organisation": "example"}, None),
                ("/api/v1/me", None, "Bearer fixture-dev"),
            ])


if __name__ == "__main__":
    unittest.main()
