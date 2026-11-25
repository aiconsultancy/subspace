#!/usr/bin/env python3
"""Print a Subspace API bearer token for your local coding agents.

The token is yours. Local agents act with your login and must say so (see the
subspace-tracker skill). Configure the helper in ~/.subspace/agent.json.

  subspace_token.py              print the token
  subspace_token.py --header     print {"Authorization": "Bearer ..."} (Claude Code headersHelper)
  subspace_token.py --me         print who the token signs in as
  subspace_token.py --login      sign in to WorkOS in your browser
  subspace_token.py --logout     forget the local session
  subspace_token.py --target T   use another installed fleet (default from ~/.subspace/agents.env)
  subspace_token.py --config F   use the settings in F, such as a deployment repository's
                                 agent-kit/agent.json, instead of ~/.subspace/agent.json and the
                                 environment (each API keeps its own session)
"""

import argparse
import base64
from contextlib import contextmanager
from dataclasses import dataclass
import hashlib
from http.client import HTTPException
from http.server import BaseHTTPRequestHandler, HTTPServer
import ipaddress
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import tempfile
import time
import webbrowser
from urllib.error import HTTPError, URLError
from urllib.parse import parse_qs, urlencode, urlsplit
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

HOME = Path(os.environ.get("SUBSPACE_HOME", Path.home() / ".subspace"))
DEFAULT_API_URL = "http://127.0.0.1:3000"
DEFAULT_WORKOS_URL = "https://api.workos.com"
REQUEST_TIMEOUT = 15
SIGN_IN_REQUIRED = "WorkOS session unavailable. Run subspace_token.py --login."


class TokenError(Exception):
    pass


def sign_in_required(config_file=None):
    """The one-line sign-in hint, naming the settings file when it is not the machine-wide one."""
    if not config_file:
        return SIGN_IN_REQUIRED
    return f"WorkOS session unavailable. Run subspace_token.py --config {config_file} --login."


def loopback_host(host):
    try:
        return ipaddress.ip_address((host or "").strip("[]")).is_loopback
    except ValueError:
        return host == "localhost"


def opener(url, redirects=False):
    """An opener for url: no redirects unless asked, and never through a proxy for this computer.

    A system or environment proxy would otherwise carry a loopback request, bearer token and all, to
    another host."""
    handlers = [] if redirects else [NoRedirect()]
    if loopback_host(urlsplit(url).hostname):
        handlers.append(ProxyHandler({}))
    return build_opener(*handlers)


@dataclass(frozen=True)
class Config:
    api_url: str
    identity_provider: str
    workos_client_id: str = ""
    workos_api_url: str = ""
    workos_redirect_port: int = 0


def base_url(value, setting, paths):
    try:
        url = urlsplit(value)
        if (
            (url.scheme == "https" or url.scheme == "http" and loopback_host(url.hostname))
            and re.fullmatch(r"(?:[A-Za-z0-9.-]+|\[[0-9A-Fa-f:.]+\])(?::[0-9]+)?", url.netloc)
            and url.path in paths and url.port != 0
            and not any(char.isspace() or ord(char) < 32 or char in "\\?#" for char in value)
        ):
            return value.rstrip("/")
    except ValueError:
        pass
    raise TokenError(f"{setting} must be an HTTPS URL or a loopback HTTP URL.")


def load_config(path=None):
    """The settings in ~/.subspace/agent.json, overridden by the environment, or exactly those in path."""
    try:
        values = json.loads(Path(path or HOME / "agent.json").read_text(encoding="utf-8"))
    except FileNotFoundError:
        if path:
            raise TokenError(f"Cannot find {path}.") from None
        values = {}
    except (OSError, ValueError):
        raise TokenError(f"Cannot read {path or 'agent.json'} as JSON.") from None
    if not isinstance(values, dict):
        raise TokenError("agent.json must contain a JSON object.")

    def setting(name, default):
        # A named file binds the settings to it, such as a deployment repository's: no overrides.
        value = values.get(name, default) if path else os.environ.get("SUBSPACE_" + name.upper(), values.get(name, default))
        if isinstance(value, str) or (name == "workos_redirect_port" and type(value) is int):
            return str(value).strip()
        raise TokenError(f"Invalid {name} in agent.json.")

    api = setting("api_url", DEFAULT_API_URL)
    provider = setting("identity_provider", "dev")
    if provider == "dev":
        return Config(api.rstrip("/").removesuffix("/api"), provider)
    if provider != "workos":
        raise TokenError("identity_provider must be dev or workos.")
    client = setting("workos_client_id", "")
    if not re.fullmatch(r"client_[A-Za-z0-9]{1,121}", client):
        raise TokenError("Set workos_client_id to your WorkOS AuthKit client ID.")
    authority = setting("workos_api_url", "") or DEFAULT_WORKOS_URL
    port = setting("workos_redirect_port", "") or "0"
    if not re.fullmatch(r"[0-9]{1,5}", port) or int(port) > 65535:
        raise TokenError("workos_redirect_port must be between 0 and 65535.")
    return Config(
        base_url(api, "api_url", ("", "/", "/api", "/api/")).removesuffix("/api"),
        provider, client, base_url(authority, "workos_api_url", ("", "/")), int(port),
    )


def read_env(path):
    values = {}
    if path.is_file():
        for line in path.read_text().splitlines():
            key, _, value = line.partition("=")
            if key and not key.startswith("#"):
                values[key.strip()] = value.strip()
    return values


def target_dir(name):
    name = name or read_env(HOME / "agents.env").get("SUBSPACE_TARGET") or "subspace-fleet"
    path = HOME / "targets" / name
    if not (path / "summary.env").is_file():
        sys.exit(f"subspace: no installed fleet '{name}' under {HOME / 'targets'}. Run the installer first.")
    return path


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def call(config, method, path, token=None, body=None):
    data = None if body is None else json.dumps(body).encode()
    headers = {"Accept": "application/json", "Content-Type": "application/json"}
    if token:
        headers["Authorization"] = f"Bearer {token}"
    try:
        open_url = opener(config.api_url, redirects=config.identity_provider == "dev").open
        request = Request(f"{config.api_url}/api/v1{path}", data=data, headers=headers, method=method)
        with open_url(request, timeout=REQUEST_TIMEOUT) as response:
            return json.load(response)
    except HTTPError as error:
        error.close()
        sys.exit(f"subspace: the Subspace API answered HTTP {error.code} for {path}.")
    except (URLError, OSError):
        message = f"cannot reach the Subspace API at {config.api_url}."
        if config.identity_provider == "dev":
            message += " Start the tunnel: ./install.sh tunnel --background"
        raise TokenError(message) from None


def dev_token(config, target):
    cache = target / "api-token.json"
    try:
        saved = json.loads(cache.read_text())
        if saved.get("expires", 0) - time.time() > 600:
            return saved["token"]
    except (OSError, ValueError, KeyError):
        pass
    summary = read_env(target / "summary.env")
    minted = call(config, "POST", "/auth/dev/token", body={
        "email": summary.get("SUBSPACE_ADMIN_EMAIL", "developer@example.com"),
        "organisation": summary.get("SUBSPACE_ORGANISATION", "local"),
    })
    value = minted["accessToken"]
    descriptor = os.open(cache, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w") as handle:
        json.dump({"token": value, "expires": time.time() + int(minted.get("expiresIn", 3600))}, handle)
    return value


def pkce_challenge(verifier):
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    return base64.urlsafe_b64encode(digest).decode("ascii").rstrip("=")


def authorisation_url(config, redirect_uri, verifier, state):
    query = urlencode({
        "client_id": config.workos_client_id,
        "redirect_uri": redirect_uri,
        "response_type": "code",
        "provider": "authkit",
        "state": state,
        "code_challenge": pkce_challenge(verifier),
        "code_challenge_method": "S256",
    })
    return f"{config.workos_api_url}/user_management/authorize?{query}"


class Callback(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass  # The request URL can contain an authorisation code.

    def do_GET(self):
        path, _, query = self.path.partition("?")
        params = parse_qs(query, keep_blank_values=True)
        states = params.get("state", [])
        if path != "/callback":
            self.reply(404, "Unknown callback path.")
        elif len(states) != 1 or not secrets.compare_digest(states[0].encode(), self.server.state.encode()):
            self.reply(400, "Invalid sign-in state.")
        elif "error" in params:
            self.server.result = ""
            self.reply(400, "Sign-in was refused. Return to your terminal.")
        elif len(params.get("code", [])) == 1 and params["code"][0]:
            self.server.result = params["code"][0]
            self.reply(200, "Return to your terminal to finish signing in.")
        else:
            self.reply(400, "Missing authorisation code.")

    def reply(self, status, message):
        self.send_response(status)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Cache-Control", "no-store")
        self.send_header("Referrer-Policy", "no-referrer")
        self.end_headers()
        self.wfile.write(message.encode("utf-8"))


class CallbackServer(HTTPServer):
    def get_request(self):
        connection, address = super().get_request()
        connection.settimeout(2)
        return connection, address

    def handle_error(self, request, client_address):
        pass  # A browser disconnect can leave the valid callback recorded.


def browser_code(config, verifier):
    state = secrets.token_urlsafe(32)
    with CallbackServer(("127.0.0.1", config.workos_redirect_port), Callback) as server:
        server.state = state
        server.result = None
        server.timeout = 1
        redirect = f"http://127.0.0.1:{server.server_port}/callback"
        if not webbrowser.open(authorisation_url(config, redirect, verifier, state)):
            raise TokenError("Could not open a browser for WorkOS sign-in.")
        deadline = time.monotonic() + 180
        while server.result is None and time.monotonic() < deadline:
            server.handle_request()
        if server.result is None:
            raise TokenError("WorkOS sign-in timed out.")
        if not server.result:
            raise TokenError("WorkOS sign-in was refused.")
        return server.result


def authenticate(config, *, previous=None, **grant):
    request = Request(
        f"{config.workos_api_url}/user_management/authenticate",
        data=json.dumps({"client_id": config.workos_client_id, **grant}).encode(),
        headers={"Accept": "application/json", "Content-Type": "application/json"},
    )
    try:
        with opener(config.workos_api_url).open(request, timeout=REQUEST_TIMEOUT) as response:
            reply = json.loads(response.read(1024 * 1024))
    except HTTPError as error:
        error.close()
        raise TokenError(f"WorkOS authentication returned HTTP {error.code}.") from None
    except (HTTPException, OSError, ValueError):
        raise TokenError("Could not read the WorkOS authentication response.") from None

    # The API verifies signatures and claims. Here the JWT only schedules
    # renewal and keeps a rotating session on the same account and organisation.
    try:
        access, refresh = reply["access_token"], reply["refresh_token"]
        if not re.fullmatch(r"[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+", access) or not (
            isinstance(refresh, str) and refresh
        ):
            raise ValueError
        payload = access.split(".")[1]
        claims = json.loads(base64.urlsafe_b64decode(payload + "=" * (-len(payload) % 4)))
        expires, subject, organisation = claims["exp"], claims["sub"], claims["org_id"]
        if type(expires) is not int or expires <= time.time():
            raise ValueError
        if not all(isinstance(value, str) and value for value in (subject, organisation)):
            raise ValueError
    except (KeyError, TypeError, ValueError):
        raise TokenError("WorkOS did not return a usable session for a Subspace organisation.") from None
    if previous and (previous["subject"], previous["organisation"]) != (subject, organisation):
        raise TokenError("WorkOS returned a different account or organisation.")
    return {
        "access_token": access, "refresh_token": refresh, "expires": expires,
        "subject": subject, "organisation": organisation,
    }


def private_directory(path):
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    if os.name != "nt":
        path.chmod(0o700)
        return

    # Windows chmod cannot restrict access. Apply a protected, inheritable
    # DACL granting full access to the directory owner before writing files.
    import ctypes
    from ctypes import wintypes

    security = ctypes.WinDLL("advapi32", use_last_error=True)
    convert = security.ConvertStringSecurityDescriptorToSecurityDescriptorW
    convert.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p]
    apply = security.SetFileSecurityW
    apply.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.c_void_p]
    descriptor = ctypes.c_void_p()
    if not convert("D:P(A;OICI;FA;;;OW)", 1, ctypes.byref(descriptor), None):
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        # PROTECTED_DACL_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION
        if not apply(str(path), 0x80000004, descriptor):
            raise ctypes.WinError(ctypes.get_last_error())
    finally:
        ctypes.WinDLL("kernel32").LocalFree(descriptor)


def try_lock(descriptor):
    if os.name == "nt":
        import msvcrt

        msvcrt.locking(descriptor, msvcrt.LK_NBLCK, 1)
    else:
        import fcntl

        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)


def unlock(descriptor):
    # Windows may release a byte-range lock some time after the file closes,
    # so the next agent's request would wait for it. Release it now.
    if os.name == "nt":
        import msvcrt

        try:
            msvcrt.locking(descriptor, msvcrt.LK_UNLCK, 1)
        except OSError:
            pass


class SessionStore:
    def __init__(self, config):
        self.directory = HOME / "workos"
        # Prevent a session from following a changed API, authority or client.
        account = json.dumps([str(HOME.resolve()), config.api_url, config.workos_api_url, config.workos_client_id])
        self.key = hashlib.sha256(account.encode()).hexdigest()
        self.file = self.directory / f"{self.key}.json"

    @contextmanager
    def locked(self):
        private_directory(self.directory)
        descriptor = os.open(self.file.with_suffix(".lock"), os.O_RDWR | os.O_CREAT, 0o600)
        try:
            deadline = time.monotonic() + 25
            while True:
                try:
                    try_lock(descriptor)
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise TokenError("Timed out waiting for the WorkOS session.") from None
                    time.sleep(0.05)
            try:
                yield
            finally:
                unlock(descriptor)
        finally:
            os.close(descriptor)

    def keychain(self, *args, value=None):
        if sys.platform != "darwin":
            return None
        security = "/usr/bin/security"
        options = ["-a", self.key, "-s", "subspace.local-agent.workos", *args[1:]]
        command = [security, args[0], *options, "login.keychain"]
        data = b""
        if value is not None:
            # Feed the password over stdin so it cannot appear in process arguments.
            line = " ".join([*command[1:-1], "-X", value.hex(), command[-1]])
            command, data = [security, "-i"], (line + "\n").encode()
        try:
            return subprocess.run(command, input=data, capture_output=True, timeout=3)
        except (OSError, subprocess.TimeoutExpired):
            return None

    def read(self):
        try:
            return json.loads(self.file.read_bytes())
        except FileNotFoundError:
            item = self.keychain("find-generic-password", "-w")
            return json.loads(item.stdout) if item and item.returncode == 0 else None

    def write_file(self, data):
        descriptor, name = tempfile.mkstemp(dir=self.directory)
        try:
            with os.fdopen(descriptor, "wb") as handle:
                handle.write(data)
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(name, self.file)
        finally:
            Path(name).unlink(missing_ok=True)

    def save(self, session):
        data = json.dumps(session, separators=(",", ":")).encode()
        item = self.keychain("add-generic-password", "-U", value=data)
        if item and item.returncode == 0:
            # Interactive security reports command errors on stderr.
            saved = self.keychain("find-generic-password", "-w")
            if saved and saved.returncode == 0 and saved.stdout.rstrip(b"\n") == data:
                self.file.unlink(missing_ok=True)
                return
        self.write_file(data)

    def clear(self):
        item = self.keychain("delete-generic-password")
        if sys.platform == "darwin" and (item is None or item.returncode not in (0, 44)):
            # Hide any older Keychain entry even if the Keychain is unavailable.
            self.write_file(b"null")
            if Path("/usr/bin/security").is_file():
                raise TokenError("Local session cleared. Unlock the login Keychain and retry --logout to clear it there.")
        else:
            self.file.unlink(missing_ok=True)


def workos_token(config, store):
    try:
        with store.locked():
            session = store.read()
            if session is None:
                raise TokenError(SIGN_IN_REQUIRED)
            if session["expires"] - time.time() > 60:
                return session["access_token"]
            session = authenticate(
                config, previous=session, grant_type="refresh_token",
                refresh_token=session["refresh_token"], organization_id=session["organisation"],
            )
            store.save(session)
            return session["access_token"]
    except (TokenError, OSError, ValueError, KeyError, TypeError):
        raise TokenError(SIGN_IN_REQUIRED) from None


def mcp_stdio(config, store, config_file=None):
    """Forward newline-delimited MCP to Subspace's stateless JSON HTTP endpoint."""
    forward = opener(config.api_url)
    for line in sys.stdin.buffer:
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if not isinstance(message, dict):
            continue
        try:
            value = workos_token(config, store)
            request = Request(f"{config.api_url}/mcp", data=line, headers={
                "Accept": "application/json, text/event-stream",
                "Content-Type": "application/json",
                "Authorization": f"Bearer {value}",
            })
            with forward.open(request, timeout=REQUEST_TIMEOUT) as response:
                reply = json.load(response) if response.status == 200 else None
        except (TokenError, HTTPException, OSError, ValueError) as error:
            if isinstance(error, TokenError):
                detail = sign_in_required(config_file) if str(error) == SIGN_IN_REQUIRED else str(error)
            elif isinstance(error, HTTPError):
                error.close()
                detail = f"Subspace MCP endpoint returned HTTP {error.code}."
            else:
                detail = "Subspace MCP request failed."
            if "id" not in message:
                continue
            reply = {"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32000, "message": detail}}
        if reply is not None:
            print(json.dumps(reply), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--target")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--header", action="store_true")
    group.add_argument("--me", action="store_true")
    group.add_argument("--login", action="store_true")
    group.add_argument("--logout", action="store_true")
    group.add_argument("--workos-mcp-url", action="store_true", help=argparse.SUPPRESS)
    group.add_argument("--mcp-stdio", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--mcp-endpoint", help=argparse.SUPPRESS)
    parser.add_argument("--config", help="use the settings in this file instead of ~/.subspace/agent.json and the "
                        "environment, such as a deployment repository's agent-kit/agent.json")
    args = parser.parse_args()
    config = load_config(args.config)
    workos = config.identity_provider == "workos"
    endpoint = f"{config.api_url if workos else DEFAULT_API_URL}/mcp"
    if args.workos_mcp_url:
        if workos:
            print(endpoint)
        return
    if args.mcp_endpoint is not None and args.mcp_endpoint != endpoint:
        raise TokenError("The Subspace MCP URL changed. Run the local skills installer again."
                         if not args.config else f"The Subspace MCP URL is not the one in {args.config}.")
    if args.mcp_stdio:
        if not workos:
            raise TokenError("The MCP stdio helper requires WorkOS configuration.")
        mcp_stdio(config, SessionStore(config), args.config)
        return
    if workos:
        store = SessionStore(config)
        if args.login:
            verifier = secrets.token_urlsafe(32)
            code = browser_code(config, verifier)
            with store.locked():
                store.save(authenticate(config, grant_type="authorization_code", code=code, code_verifier=verifier))
            print("Signed in to WorkOS.")
            return
        if args.logout:
            with store.locked():
                store.clear()
            print("Signed out.")
            return
        try:
            value = workos_token(config, store)
        except TokenError as error:
            raise TokenError(sign_in_required(args.config)) if str(error) == SIGN_IN_REQUIRED else error
    elif args.login:
        print("Development sign-in needs no login.")
        return
    else:
        target = target_dir(args.target)
        if args.logout:
            (target / "api-token.json").unlink(missing_ok=True)
            print("Cleared the development token cache.")
            return
        value = dev_token(config, target)
    if args.header:
        print(json.dumps({"Authorization": f"Bearer {value}"}))
    elif args.me:
        me = call(config, "GET", "/me", token=value)
        print(f"{me.get('displayName')} <{me.get('email')}>")
    else:
        print(value)


if __name__ == "__main__":
    try:
        main()
    except TokenError as error:
        sys.exit(f"subspace: {error}")
    except OSError:
        sys.exit("subspace: could not access the local sign-in listener or session store.")
    except KeyboardInterrupt:
        sys.exit("subspace: cancelled.")
