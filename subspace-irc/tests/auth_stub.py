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
    """Return (status, body bytes, delay in seconds) for one login."""
    def body(value):
        return value if isinstance(value, bytes) else json.dumps(value).encode()

    answers = {
        # Accepted only for the presented name, answered with a different one.
        "sirc1.valid": (200, {"success": True, "accountName": "hook-member"})
        if name == "hook-presented" else (200, {"success": False}),
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
    }
    status, value = answers.get(passphrase, (200, {"success": False}))
    return status, body(value), 8 if passphrase == "sirc1.hang" else 0


class Handler(BaseHTTPRequestHandler):
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
        name = request.get("accountName", "")
        print("request", name, flush=True)
        status, body, delay = answer(name, request.get("passphrase", ""))
        time.sleep(delay)
        self.reply(status, body)

    def reply(self, status, body):
        self.send_response(status)
        if status == 302:
            self.send_header("Location", "https://example.com/")
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass

    def log_message(self, *_):
        pass


context = ssl.create_default_context(ssl.Purpose.CLIENT_AUTH)
context.load_cert_chain("/stub/cert.pem", "/stub/key.pem")
server = ThreadingHTTPServer(("0.0.0.0", 8443), Handler)
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
