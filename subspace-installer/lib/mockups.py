"""Installer settings and files for the fleet's static mockup previews."""

from pathlib import Path

from fleet_records import normalize_web_address, require

PORT = 8090
USER = "ss-fleet-preview"
HOST = "127.0.0.1"
SOCKETS = Path("/run/subspace-fleet/previews")
TMPFILES = Path("/etc/tmpfiles.d/subspace-fleet-previews.conf")
SKILL = "fleet-mockup"
MCP = "stargate_serve"
IMPECCABLE = "impeccable"
IMPECCABLE_PATH = Path("/opt/subspace-fleet/share/impeccable")
SCRIPT = "officer-previews.ts"
SCRIPTS = tuple(f"scripts/{name}" for name in (SCRIPT, "preview-bundle.ts", "preview-store.ts", "officer-stdio.ts"))


def origin(value):
    """A dedicated web origin on the operator's private network."""
    problem = ("The preview URL must be a dedicated HTTP or HTTPS origin, "
               "without credentials, a path, a query or a fragment.")
    address = normalize_web_address(value, problem, allow_non_loopback_http=True)
    require("/" not in address.split("://", 1)[1], problem)
    return address


def address(options):
    return options.get("url") or f"http://{HOST}:{options['port']}"


def socket(captain):
    return SOCKETS / captain / "publish.sock"


def staging(home):
    return home / "workspace" / ".mockups"


def enabled(cfg, person):
    return cfg.get("mockupPreviews", {}).get("enabled", False) and person["role"] == "captain"


def caddyfile(home, port):
    return f"""{{
    admin off
    persist_config off
    storage file_system {{
        root {home / "caddy"}
    }}
}}

http://:{port} {{
    bind {HOST}

    handle /p/* {{
        root * {home / "public"}
        header {{
            Cache-Control "no-store"
            X-Content-Type-Options "nosniff"
            Content-Security-Policy "default-src 'self' data: blob:; script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; form-action 'none'; base-uri 'none'; object-src 'none'"
        }}
        file_server
    }}

    handle {{
        respond 404
    }}
}}
"""
