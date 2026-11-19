"""Offline contract checks for the Sign in with GitHub part of the GitHub fixture."""
import json
import threading
import unittest
from http.server import ThreadingHTTPServer
from urllib.error import HTTPError
from urllib.parse import parse_qs, urlencode, urlparse
from urllib.request import HTTPRedirectHandler, Request, build_opener, urlopen

import github_test_server
from github_test_server import Handler


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


class SignInFixtureTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"
        github_test_server.BASE_URL = cls.base
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()

    def post(self, path, form):
        request = Request(self.base + path, data=urlencode(form).encode(), method="POST",
                          headers={"Accept": "application/json"})
        with urlopen(request, timeout=5) as response:
            return json.loads(response.read())

    def test_authorize_redirects_back_to_a_loopback_callback_with_the_state(self):
        query = urlencode({"client_id": "id", "redirect_uri": "http://127.0.0.1:5555/callback",
                           "state": "the-state", "code_challenge": "c", "code_challenge_method": "S256"})
        opener = build_opener(NoRedirect)
        with self.assertRaises(HTTPError) as caught:
            opener.open(f"{self.base}/login/oauth/authorize?{query}", timeout=5)
        self.assertEqual(caught.exception.code, 302)
        location = urlparse(caught.exception.headers["Location"])
        self.assertEqual((location.hostname, location.port, location.path), ("127.0.0.1", 5555, "/callback"))
        returned = parse_qs(location.query)
        self.assertEqual(returned["state"], ["the-state"])
        answer = self.post("/login/oauth/access_token", {"code": returned["code"][0], "code_verifier": "v",
                                                         "client_id": "id", "client_secret": "s"})
        self.assertEqual(answer["access_token"], "fixture-mona")
        self.assertEqual(answer["scope"], "repo,read:org")

    def test_authorize_refuses_a_redirect_off_the_machine(self):
        query = urlencode({"redirect_uri": "https://example.com/callback", "state": "s"})
        with self.assertRaises(HTTPError) as caught:
            urlopen(f"{self.base}/login/oauth/authorize?{query}", timeout=5)
        self.assertEqual(caught.exception.code, 400)

    def test_authorize_refuses_a_callback_that_could_split_the_header(self):
        for uri in ("http://127.0.0.1:5555/cb\r\nSet-Cookie: x=1", "http://127.0.0.1:5555/c b",
                    "http://127.0.0.1:port/cb"):
            with self.subTest(uri=uri):
                query = urlencode({"redirect_uri": uri, "state": "s"})
                with self.assertRaises(HTTPError) as caught:
                    urlopen(f"{self.base}/login/oauth/authorize?{query}", timeout=5)
                self.assertEqual(caught.exception.code, 400)
        location = github_test_server.authorize({"redirect_uri": ["http://[::1]:6000/callback"], "state": ["s"]})
        self.assertTrue(location.startswith("http://[::1]:6000/callback?"), location)

    def test_the_device_grant_is_pending_twice_then_granted(self):
        code = self.post("/login/device/code", {"client_id": "id", "scope": "repo read:org"})
        self.assertEqual(code["user_code"], "OCTO-1234")
        self.assertEqual(code["verification_uri"], self.base + "/login/device")
        form = {"client_id": "id", "device_code": code["device_code"],
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code"}
        answers = [self.post("/login/oauth/access_token", form) for _ in range(3)]
        self.assertEqual([answer.get("error") for answer in answers[:2]], ["authorization_pending"] * 2)
        self.assertEqual(answers[2]["access_token"], "fixture-mona")

    def test_a_wrong_code_is_refused(self):
        answer = self.post("/login/oauth/access_token", {"code": "forged", "code_verifier": "v"})
        self.assertEqual(answer["error"], "bad_verification_code")

    def test_the_new_token_names_its_account_and_revoking_succeeds(self):
        request = Request(self.base + "/user", headers={"Authorization": "Bearer fixture-mona"})
        with urlopen(request, timeout=5) as response:
            self.assertEqual(json.loads(response.read())["login"], "mona")
        revoke = Request(self.base + "/applications/id/grant", data=b'{"access_token":"fixture-mona"}',
                         method="DELETE")
        with urlopen(revoke, timeout=5) as response:
            self.assertEqual(response.status, 204)


if __name__ == "__main__":
    unittest.main()
