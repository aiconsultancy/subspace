"""Synthetic plaintext local API over CONNECT, not a TLS-validation fixture."""

import http.client
import json
import os
from urllib.parse import urlsplit

import packaging
from packaging.version import Version


def on_init(context, event):
    assert packaging.__version__ == "24.2"
    assert "SUBSPACE_RUNTIME_TEST_ROOT" not in os.environ
    assert os.environ["SYNTHETIC_VALUE"] == "dummy-test-only-selected-value"


def call_api(authority):
    proxy = urlsplit(os.environ["HTTPS_PROXY"])
    assert proxy.scheme == "http" and proxy.hostname and proxy.port
    host, port = authority.rsplit(":", 1)
    connection = http.client.HTTPConnection(proxy.hostname, proxy.port, timeout=2)
    connection.set_tunnel(host, int(port))
    try:
        connection.request(
            "GET", "/metrics",
            headers={"Authorization": "Bearer " + os.environ["SYNTHETIC_VALUE"]},
        )
        response = connection.getresponse()
        if response.status != 200:
            raise RuntimeError("synthetic API did not return 200")
        # The fixture's response is bounded as well as the host protocol frame.
        body = response.read(4097)
        if len(body) > 4096:
            raise ValueError("synthetic API response exceeds fixture budget")
        return json.loads(body)
    finally:
        connection.close()


def on_action(context, event):
    key = "SYNTHETIC_API_ALLOWED" if event["action"] == "fetch" else "SYNTHETIC_API_DENIED"
    payload = call_api(os.environ[key])
    if event["action"] == "denied":
        raise RuntimeError("ungranted destination was unexpectedly contacted")
    text = json.dumps({
        "count": payload["count"],
        "package_version": packaging.__version__,
        "parsed_major": Version(payload["version"]).major,
    })
    return {"results": {"heading": {
        "scope": payload["scope"],
        "evaluated_at_ms": payload["evaluated_at_ms"],
        "value": {"type": "text", "text": text},
    }}}
