"""Stand in for the Subspace API's IRC hook endpoint in tests/auth_hook.py.

Runs inside the image over HTTPS. The passphrase picks the answer, so the
test can drive every case through real SASL logins. It prints only the
account names it sees, never passphrases.
"""

import hmac
import json
import ssl
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

OPERATOR = sys.argv[1]
with open("/stub/token", encoding="utf-8") as token_file:
    TOKEN = token_file.read().strip()
INPUT_KEYS = {"accountName", "passphrase", "certfp", "peerCerts", "ip"}


def answer(name, passphrase):
    """Return (status, body bytes, delay in seconds, declared length) for one login."""
    def body(value):
        return value if isinstance(value, bytes) else json.dumps(value).encode()

    answers = {
        # Accepted only for the presented name, answered with a different one.
        "sirc1.valid": (200, {"success": True, "accountName": "hook-member"})
        if name == "hook-presented" else (200, {"success": False}),
        "custom-valid": (200, {"success": True, "accountName": "hook-custom"})
        if name == "hook-custom-user" else (200, {"success": False}),
        "sirc1.refuse": (200, {"success": False}),
        "sirc1.status500": (500, {"success": True, "accountName": "hook-status"}),
        "sirc1.redirect": (302, {"success": True, "accountName": "hook-redirect"}),
        "sirc1.text": (200, b"accepted"),
        "sirc1.twice": (200, b'{"success":true,"accountName":"hook-twice"}'
                             b'{"success":true,"accountName":"hook-twice"}'),
        "sirc1.string": (200, {"success": "true", "accountName": "hook-string"}),
        "sirc1.array": (200, [{"success": True, "accountName": "hook-array"}]),
        "sirc1.nameless": (200, {"success": True}),
        "sirc1.operator": (200, {"success": True, "accountName": OPERATOR.upper()}),
        "sirc1.hang": (200, {"success": True, "accountName": "hook-hang"}),
        # Go matches JSON keys without regard to case: "Success" would win in Ergo.
        "sirc1.dupcase": (200, b'{"success":false,"Success":true,"accountName":"hook-dupcase"}'),
        "sirc1.echo": (200, {"success": False, "error": f"bad {passphrase}\nINJECTED log line"}),
        "sirc1.extra": (200, {"success": True, "accountName": "hook-extra",
                              "displayName": "Extra", "error": "API text never logged"}),
        "sirc1.short": (200, {"success": True, "accountName": "hook-short"}),
        "sirc1.huge": (200, {"success": True, "accountName": "hook-huge", "padding": "x" * 70000}),
        "sirc1.seeded": (200, {"success": True, "accountName": "HOOK-AGENT"}),
        "sirc1.handmade": (200, {"success": True, "accountName": "hook-handmade"}),
        "sirc1.badname": (200, {"success": True, "accountName": "bad name!"}),
        "sirc1.reserved": (200, {"success": True, "accountName": "NickServ"}),
    }
    status, value = answers.get(passphrase, (200, {"success": False}))
    data = body(value)
    # A body shorter than its declared Content-Length.
    declared = len(data) + 100 if passphrase == "sirc1.short" else len(data)
    return status, data, 8 if passphrase == "sirc1.hang" else 0, declared


class Handler(BaseHTTPRequestHandler):
    def setup(self):
        # Handshake in the request's own thread, so many concurrent clients
        # are not serialised behind the accept loop.
        self.request = CONTEXT.wrap_socket(self.request, server_side=True)
        super().setup()

    def do_POST(self):
        expected = f"Bearer {TOKEN}"
        if not hmac.compare_digest(self.headers.get("Authorization", ""), expected):
            return self.reply(401, b"{}")
        if self.path != "/api/v1/irc/authenticate":
            return self.reply(404, b"{}")
        try:
            request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        except (TypeError, ValueError):
            return self.reply(400, b"{}")
        if not isinstance(request, dict) or not set(request) <= INPUT_KEYS:
            return self.reply(400, b"{}")
        if "certfp" in request and "passphrase" not in request:
            # Certificate login: the test's client certificate maps to one account.
            print("request certfp", flush=True)
            return self.reply(200, json.dumps({"success": True, "accountName": "hook-cert"}).encode())
        name = request.get("accountName", "")
        print("request", name, flush=True)
        status, body, delay, declared = answer(name, request.get("passphrase", ""))
        time.sleep(delay)
        self.reply(status, body, declared)

    def reply(self, status, body, declared=None):
        self.send_response(status)
        if status == 302:
            self.send_header("Location", "https://example.com/")
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body) if declared is None else declared))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, *_):
        pass


class Server(ThreadingHTTPServer):
    request_queue_size = 512

    def handle_error(self, request, client_address):
        pass  # Health checks connect without TLS.


CONTEXT = ssl.create_default_context(ssl.Purpose.CLIENT_AUTH)
CONTEXT.load_cert_chain("/stub/cert.pem", "/stub/key.pem")
Server(("0.0.0.0", 8443), Handler).serve_forever()
