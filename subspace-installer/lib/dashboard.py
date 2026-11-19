#!/usr/bin/python3
"""Expose only Stargate's read-only dashboard over a local TCP listener."""

import http.client
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import ipaddress
import shutil
import socket
import sys
from urllib.parse import urlsplit


class UnixConnection(http.client.HTTPConnection):
    def connect(self):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.settimeout(self.timeout)
        self.sock.connect(self.host)


class Dashboard(BaseHTTPRequestHandler):
    def do_GET(self):
        self.forward()

    def do_HEAD(self):
        self.forward()

    def forward(self):
        try:
            host = urlsplit("//" + self.headers.get("Host", "")).hostname
            target = urlsplit(self.path)
        except ValueError:
            self.send_error(400)
            return
        if host not in self.server.hostnames or not self.path.startswith("/") or target.netloc or target.scheme:
            self.send_error(403)
            return
        upstream = UnixConnection(self.server.unix_path, timeout=10)
        try:
            upstream.request(self.command, self.path, headers={"Host": "stargate"})
            response = upstream.getresponse()
        except (OSError, http.client.HTTPException):
            upstream.close()
            self.send_error(502, "Stargate dashboard is unavailable")
            return
        try:
            self.send_response(response.status)
            for header in ("Content-Type", "Content-Length", "Content-Security-Policy",
                           "X-Content-Type-Options", "Referrer-Policy", "X-Frame-Options"):
                if value := response.getheader(header):
                    self.send_header(header, value)
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            if self.command == "GET":
                shutil.copyfileobj(response, self.wfile)
        except (OSError, http.client.HTTPException):
            self.close_connection = True
        finally:
            response.close()
            upstream.close()

    def log_message(self, *_args):
        # Session URLs can contain private metadata.
        pass


if __name__ == "__main__":
    bind, port, path = sys.argv[1:]
    if ipaddress.ip_address(bind).version == 6:
        ThreadingHTTPServer.address_family = socket.AF_INET6
    with ThreadingHTTPServer((bind, int(port)), Dashboard) as httpd:
        httpd.hostnames = {bind, "127.0.0.1", "::1", "localhost"}
        httpd.unix_path = path
        httpd.serve_forever()
