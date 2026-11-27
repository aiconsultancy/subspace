"""Run the shipped auth script against a stub API on loopback, without Docker.

It checks which passphrases the script forwards: both API credential formats
with the entrypoint's default list, only the configured prefixes otherwise, and
never a login to a local-only account, one made without a valid local-only
account list, or any without a prefix list.
It needs sh, jq, curl, flock and python3, as the image has.
"""

import json
import os
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "subspace-auth.sh"
TOKEN = "an-example-hook-token-of-at-least-32-bytes"


class StubApi(BaseHTTPRequestHandler):
    """Accept any login it is sent, naming the account after the passphrase's prefix."""

    def log_message(self, *_):
        pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        request = json.loads(self.rfile.read(length))
        self.server.requests.append((self.headers.get("Authorization"), request))
        prefix = request["passphrase"].split(".", 1)[0].split("-", 1)[0]
        body = json.dumps({"success": True, "accountName": f"api-{prefix}"}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)


class AuthScriptTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), StubApi)
        cls.server.requests = []
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def setUp(self):
        self.server.requests.clear()
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        (root / "slots").mkdir()
        (root / "token").write_text(TOKEN)
        self.local_list = root / "local-accounts.json"
        self.local_list.write_text(json.dumps({"version": 1, "names": ["operator"]}))
        self.env = {
            "PATH": os.environ["PATH"],
            "IRC_AUTH_API_URL": f"http://127.0.0.1:{self.server.server_port}",
            "IRC_AUTH_API_ALLOW_HTTP": "true",
            "IRC_AUTH_API_TOKEN_FILE": str(root / "token"),
            "IRC_AUTH_LOCAL_LIST": str(self.local_list),
            "IRC_AUTH_SLOTS_DIR": str(root / "slots"),
            "IRC_AUTH_API_CONCURRENCY": "4",
            # What the entrypoint passes by default.
            "IRC_AUTH_PASSPHRASE_PREFIX": "sirc1. sirc2.",
        }

    def tearDown(self):
        self.directory.cleanup()

    def login(self, account, passphrase, **env):
        """Ergo's request through the script: its answer, and whether the API saw it.

        A setting given as None is left out of the script's environment.
        """
        before = len(self.server.requests)
        request = json.dumps({"accountName": account, "passphrase": passphrase, "ip": "192.0.2.1"})
        result = subprocess.run(
            ["sh", str(SCRIPT)], input=request + "\n", capture_output=True, text=True,
            env={key: value for key, value in {**self.env, **env}.items() if value is not None},
            timeout=20, check=True,
        )
        return json.loads(result.stdout), len(self.server.requests) > before

    def test_both_api_credential_formats_reach_the_api_with_the_default_list(self):
        for passphrase, account in (("sirc1.example", "api-sirc1"), ("sirc2.example", "api-sirc2")):
            answer, forwarded = self.login("member", passphrase)
            self.assertEqual(answer, {"success": True, "accountName": account})
            self.assertTrue(forwarded)
        authorisation, request = self.server.requests[-1]
        self.assertEqual(authorisation, f"Bearer {TOKEN}")
        self.assertEqual(request["passphrase"], "sirc2.example")

    def test_other_passwords_and_local_only_accounts_stay_local(self):
        for account, passphrase in (
            ("member", "a local password"),
            ("member", "sirc3.example"),
            ("member", "xsirc2.example"),
            ("operator", "sirc2.example"),
            ("OPERATOR@phone", "sirc1.example"),
        ):
            answer, forwarded = self.login(account, passphrase)
            self.assertEqual(answer, {"success": False}, (account, passphrase))
            self.assertFalse(forwarded, (account, passphrase))

    def test_nothing_is_forwarded_without_a_valid_local_only_list(self):
        for content in ("", '{"names": []}', '{"version": 1, "names": [1]}'):
            self.local_list.write_text(content)
            for passphrase in ("sirc1.example", "sirc2.example"):
                answer, forwarded = self.login("member", passphrase)
                self.assertFalse(answer["success"])
                self.assertIn("local-only account list", answer["error"])
                self.assertFalse(forwarded)

    def test_nothing_is_forwarded_without_a_prefix_list(self):
        unset = "subspace-auth: IRC_AUTH_PASSPHRASE_PREFIX is not set"
        for prefixes in ("", None):
            for passphrase in ("sirc1.example", "sirc2.example"):
                self.assertEqual(
                    self.login("member", passphrase, IRC_AUTH_PASSPHRASE_PREFIX=prefixes),
                    ({"success": False, "error": unset}, False))

    def test_a_configured_list_replaces_the_default(self):
        prefixes = {"IRC_AUTH_PASSPHRASE_PREFIX": "custom-"}
        self.assertEqual(self.login("member", "custom-example", **prefixes),
                         ({"success": True, "accountName": "api-custom"}, True))
        for passphrase in ("sirc1.example", "sirc2.example"):
            self.assertEqual(self.login("member", passphrase, **prefixes), ({"success": False}, False))

        both = {"IRC_AUTH_PASSPHRASE_PREFIX": "sirc1.,\n custom-"}
        self.assertTrue(self.login("member", "sirc1.example", **both)[1])
        self.assertTrue(self.login("member", "custom-example", **both)[1])
        self.assertFalse(self.login("member", "sirc2.example", **both)[1])


if __name__ == "__main__":
    unittest.main()
