"""Pin each crew-hosting captain's one root OpenCode session.

Stargate's crew plugin gives captain authority to one native root session. The
gateway's ``backend.sessionID`` and the plugin's ``captainSessionID`` both name
it. The ID is reserved in a root-only record before the session is created, so
an interrupted run retries the same ID instead of creating a second root.
Creating a session starts no model turn.
"""

import base64
import json
import os
from pathlib import Path
import re
import secrets
import sys
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener


SESSION_ID = re.compile(r"ses_[A-Za-z0-9]+")
CAPTAIN_AGENT = "build"


class SessionError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise SessionError(message)


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_args):
        return None


class OpenCode:
    """The few V2 session routes the installer needs, on a captain's loopback engine."""

    def __init__(self, url, password, opener=None, timeout=10):
        self.url = url.rstrip("/")
        self.headers = {
            "Authorization": "Basic " + base64.b64encode(f"opencode:{password}".encode()).decode(),
            "Accept": "application/json",
        }
        # Loopback only, with the captain's credentials: never through a proxy from the environment.
        self.opener = opener or build_opener(ProxyHandler({}), NoRedirect())
        self.timeout = timeout

    def request(self, method, path, body=None, empty=False):
        data = None if body is None else json.dumps(body).encode()
        headers = {**self.headers, **({"Content-Type": "application/json"} if data is not None else {})}
        request = Request(self.url + path, data=data, headers=headers, method=method)
        with self.opener.open(request, timeout=self.timeout) as response:
            payload = response.read()
        if empty:
            return None
        value = json.loads(payload)
        require(isinstance(value, dict) and isinstance(value.get("data"), dict), "Unexpected OpenCode session response.")
        return value["data"]

    def get(self, session_id):
        try:
            return self.request("GET", f"/api/session/{quote(session_id, safe='')}")
        except HTTPError as error:
            error.close()
            if error.code == 404:
                return None
            raise SessionError(f"OpenCode returned HTTP {error.code} for a session lookup.") from None

    def create(self, body):
        try:
            return self.request("POST", "/api/session", body)
        except HTTPError as error:
            error.close()
            raise SessionError(f"OpenCode returned HTTP {error.code} when creating the captain session.") from None

    def switch_model(self, session_id, model):
        try:
            self.request("POST", f"/api/session/{quote(session_id, safe='')}/model", {"model": model}, empty=True)
        except HTTPError as error:
            error.close()
            raise SessionError(f"OpenCode returned HTTP {error.code} when switching the captain session's model.") from None

    def update_permissions(self, session_id, permissions):
        try:
            self.request("PATCH", f"/api/session/{quote(session_id, safe='')}", {"permissions": permissions}, empty=True)
        except HTTPError as error:
            error.close()
            raise SessionError(f"OpenCode returned HTTP {error.code} when updating the captain session.") from None


def wait_until_ready(client, session_id, timeout=90, sleep=time.sleep, clock=time.monotonic):
    """Wait for the engine to answer, then return the reserved session or None."""
    deadline = clock() + timeout
    while True:
        try:
            return client.get(session_id)
        except (URLError, ConnectionError, TimeoutError, OSError) as error:
            if isinstance(error, HTTPError) or clock() >= deadline:
                raise SessionError("The captain's OpenCode engine did not answer. Check its service journal.") from None
            sleep(0.5)


def new_session_id():
    return "ses_" + secrets.token_hex(16)


def read_record(path):
    try:
        with os.fdopen(os.open(path, os.O_RDONLY | os.O_NOFOLLOW), encoding="utf-8") as stream:
            value = json.load(stream)
    except FileNotFoundError:
        return None
    except (OSError, ValueError):
        raise SessionError(f"Cannot read the captain session record {path}.") from None
    require(isinstance(value, dict), f"Invalid captain session record {path}.")
    return value


def write_durable(path, text):
    """Replace a root-only file atomically, with its contents and the rename on disk first."""
    with tempfile.NamedTemporaryFile("w", dir=path.parent, delete=False, encoding="utf-8") as stream:
        temp = Path(stream.name)
        try:
            os.fchmod(stream.fileno(), 0o600)
            stream.write(text)
            stream.flush()
            os.fsync(stream.fileno())
            stream.close()
            os.replace(temp, path)
        finally:
            temp.unlink(missing_ok=True)
    sync_directory(path.parent)


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def write_record(path, value):
    write_durable(path, json.dumps(value, indent=2) + "\n")


class SessionRecord:
    """Root-only records: <captain>.pending.json while reserved, <captain>.json once confirmed.

    The gateway unit waits for the confirmed file, so a reservation alone never starts it.
    """

    def __init__(self, directory, captain):
        self.confirmed = Path(directory) / f"{captain}.json"
        self.pending = Path(directory) / f"{captain}.pending.json"

    def load(self):
        record = read_record(self.confirmed)
        if record is not None:
            require(record.get("confirmed") is True, f"Invalid captain session record {self.confirmed}.")
            return record
        return read_record(self.pending)

    def save(self, value):
        self.confirmed.parent.mkdir(mode=0o700, exist_ok=True)
        if value.get("confirmed"):
            write_record(self.confirmed, value)
            self.pending.unlink(missing_ok=True)
            sync_directory(self.pending.parent)
        else:
            write_record(self.pending, value)


def check_roster(record, crew):
    """Stargate keeps a pinned team's crew IDs and worktrees fixed; refuse a silent rebind."""
    recorded = record.get("crew")
    require(recorded == crew, (
        "This captain's crew are fixed once its session is pinned, because Stargate keeps crew identities and "
        f"worktrees permanent (recorded: {', '.join(recorded or []) or 'none'}; configured: {', '.join(crew) or 'none'}). "
        "Restore the recorded crew counts and names, and keep at least one repository, which crew need, "
        "or give the new roster to a new captain."
    ))


def verify(session, record, expected):
    location = session.get("location") if isinstance(session.get("location"), dict) else {}
    require(
        session.get("id") == record["sessionID"] and not session.get("parentID") and session.get("fork") is None
        and session.get("agent") == CAPTAIN_AGENT and location.get("directory") == expected["directory"],
        "The pinned captain session differs from its record (identity, agent or directory). "
        "Inspect it before changing anything; the installer never replaces a captain root.",
    )


def model_name(model):
    """provider/model, with #variant for a reasoning effort."""
    return f"{model.get('providerID')}/{model.get('id')}" + (f"#{model['variant']}" if model.get("variant") else "")


def model_differs(session, model):
    stored = session.get("model")
    return isinstance(stored, dict) and model_name(stored) != model_name(model)


def ensure(client, store, expected, wait=wait_until_ready, notice=None):
    """Create or confirm the captain's root session; return (sessionID, newly confirmed).

    expected["directory"] is where the root works. A captain with crew also has expected["project"], the first
    repository's checkout, which Stargate's crew state names even when the root works in the workspace above it;
    a record without one predates the workspace root, so its directory is that checkout.
    """
    notice = notice or (lambda text: print(text, file=sys.stderr))
    project = {"project": expected["project"]} if "project" in expected else {}
    fixed = ("The captain's first repository is fixed once its session is pinned, because Stargate's crew "
             "state names that checkout. Restore the original first repository.")
    record = store.load()
    if record is not None:
        require(SESSION_ID.fullmatch(str(record.get("sessionID", ""))), "Invalid pinned captain session record.")
        if record.get("confirmed"):
            require(record.get("directory") == expected["directory"], fixed)
            require(not project or record.get("project", record.get("directory")) == project["project"], fixed)
            check_roster(record, expected["crew"])
    else:
        # Reserve before creating so an interrupted create is retried with the same ID.
        record = {"sessionID": new_session_id(), "directory": expected["directory"], **project, "agent": CAPTAIN_AGENT,
                  "crew": expected["crew"], "confirmed": False}
        store.save(record)
    session = wait(client, record["sessionID"])
    if not record.get("confirmed") and record.get("directory") != expected["directory"]:
        # Only a reservation: re-reserve the same unused ID for the new first repository or root.
        require(session is None,
                "The captain's root session already exists in its original first repository, although the "
                "installer has not confirmed it. Restore the original first repository; the installer never "
                "replaces a captain root.")
        record = {**record, "directory": expected["directory"], **project, "crew": expected["crew"]}
        store.save(record)
    elif not record.get("confirmed") and project and record.get("project") != project["project"]:
        # A reservation in the workspace whose first repository changed: nothing names it yet.
        record = {**record, **project, "crew": expected["crew"]}
        store.save(record)
    if session is None:
        require(not record.get("confirmed"),
                "The pinned captain session is missing from OpenCode. The installer does not replace a captain "
                "root; restore the engine's session database or inspect the record.")
        body = {
            "id": record["sessionID"], "title": expected["title"], "agent": CAPTAIN_AGENT,
            "model": expected["model"], "location": {"directory": expected["directory"]},
            "permissions": expected["permissions"],
        }
        try:
            session = client.create(body)
        except SessionError:
            # A create that failed after the server stored it is adopted, never repeated with a new ID.
            session = client.get(record["sessionID"])
            if session is None:
                raise
    verify(session, record, expected)
    if model_differs(session, expected["model"]):
        # A changed harness model or effort: OpenCode switches the pinned root's model (as Stargate's captain
        # model tool does), and the root's next turn uses it. Its history stays.
        stored = model_name(session["model"])
        try:
            client.switch_model(record["sessionID"], expected["model"])
            session = client.get(record["sessionID"])
            require(session is not None and not model_differs(session, expected["model"]),
                    "The captain session's model did not read back after the switch.")
            notice(f"Switched the captain's pinned session from {stored} to {model_name(expected['model'])}.")
        except SessionError as error:
            notice(f"The captain's pinned session keeps its model {stored}: switching it to "
                   f"{model_name(expected['model'])} failed ({error}). The next installer run tries again.")
            session = client.get(record["sessionID"]) or session
    if session.get("permissions") != expected["permissions"]:
        client.update_permissions(record["sessionID"], expected["permissions"])
        session = client.get(record["sessionID"])
        require(session is not None and session.get("permissions") == expected["permissions"],
                "The captain session's permissions did not read back after the update.")
    if record.get("confirmed"):
        return record["sessionID"], False
    store.save({**record, "crew": expected["crew"], "confirmed": True})
    return record["sessionID"], True
