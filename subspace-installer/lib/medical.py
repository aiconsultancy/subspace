#!/usr/bin/python3
"""Poll Sentry and deliver fleet alerts over IRC."""

import base64
from contextlib import closing, contextmanager
from dataclasses import asdict, dataclass
from datetime import datetime
from email.utils import parsedate_to_datetime
from fcntl import LOCK_EX, LOCK_NB, flock
from http.client import HTTPException
import json
import os
import re
import socket
import sqlite3
import ssl
import sys
from time import monotonic, time
from urllib.error import HTTPError
from urllib.parse import urlencode, urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener


PAGE_LIMIT = 3
PAGE_SIZE = 100
ALERT_LIMIT = 3
RESPONSE_LIMIT = 8 * 1024 * 1024
LOOKBACK = 7 * 86400
TIMEOUT = 15
SAAS_HOSTS = {"sentry.io", "us.sentry.io", "de.sentry.io"}
ISSUE_FIELDS = "(origin, org, environment, project, issue)"
ISSUE_KEY = f"{ISSUE_FIELDS} = (?, ?, ?, ?, ?)"
POLL_FAILED = "Medical ship: Sentry polling failed. Check the medical service journal."
POLL_RECOVERED = "Medical ship: Sentry polling recovered."


class PollError(ValueError):
    def __init__(self, message, retry_at=0):
        super().__init__(message)
        self.retry_at = retry_at


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, *_args):
        return None


def retry_after(value, now):
    if not value:
        return 0
    if re.fullmatch(r"[0-9]{1,12}", value):
        return now + int(value)
    try:
        return max(now, parsedate_to_datetime(value).timestamp())
    except (ValueError, TypeError, OSError, OverflowError):
        return 0


def sentry_get(config, params, missing_ok=False):
    request = Request(
        f"{config['baseUrl']}/api/0/organizations/{config['organisation']}/issues/?{urlencode(params, doseq=True)}",
        headers={"Authorization": f"Bearer {config['token']}", "Accept": "application/json"},
    )
    try:
        with build_opener(NoRedirect()).open(request, timeout=TIMEOUT) as response:
            body = response.read(RESPONSE_LIMIT + 1)
            if len(body) > RESPONSE_LIMIT:
                raise PollError("Sentry response exceeded the size limit.")
            return json.loads(body), ", ".join(response.headers.get_all("Link", []))
    except HTTPError as error:
        with error:
            if missing_ok and error.code == 404:
                return [], ""
            wait = retry_after(error.headers.get("Retry-After"), time()) if error.code == 429 or error.code >= 500 else 0
            raise PollError(f"Sentry returned HTTP {error.code}.", wait) from None
    except (OSError, HTTPException, UnicodeError, json.JSONDecodeError, RecursionError):
        raise PollError("Could not read the Sentry API response.") from None


def next_cursor(links):
    for entry in re.split(r",\s*(?=<)", links):
        _, _, attributes = entry.partition(">")
        params = dict(re.findall(r';\s*([a-z]+)="([^"]*)"', attributes))
        if params.get("rel") == "next" and params.get("results") == "true":
            cursor = params.get("cursor", "")
            if not re.fullmatch(r"[a-zA-Z0-9:_-]{1,256}", cursor):
                raise PollError("Sentry returned an invalid pagination cursor.")
            return cursor
    return None


def recent_issues(config, status, environment, pages, missing_ok=False):
    query = f"is:{status} lastSeen:-7d level:[error,fatal]"
    if tag := config.get("http5xxTag"):
        query += f" tags[{tag}]:5*"
    params = {"project": config["projects"], "query": query, "statsPeriod": "7d",
              "sort": "date", "limit": PAGE_SIZE, "groupStatsPeriod": "", "expand": "inbox"}
    if environment:
        params["environment"] = environment
    cursors = set()
    for page in range(pages):
        issues, links = sentry_get(config, params, missing_ok=missing_ok and page == 0)
        if not isinstance(issues, list) or len(issues) > PAGE_SIZE:
            raise PollError("Sentry returned an invalid issue list.")
        yield from issues
        cursor = next_cursor(links)
        if cursor is None:
            return
        if cursor in cursors:
            raise PollError("Sentry repeated a pagination cursor.")
        cursors.add(cursor)
        params["cursor"] = cursor


def issue_link(config, permalink, project, issue):
    if not isinstance(permalink, str) or not re.fullmatch(r"[!-~]{1,350}", permalink):
        return None
    try:
        link = urlsplit(permalink)
        api = urlsplit(config["baseUrl"])
        org = config["organisation"]
        same_host = link.hostname == api.hostname and (link.port or 443) == (api.port or 443)
        saas_host = api.hostname in SAAS_HOSTS and api.port in (None, 443) and link.port in (None, 443)
        expected_host = same_host or (saas_host and link.hostname in {"sentry.io", f"{org.lower()}.sentry.io"})
        allowed_paths = {f"/issues/{issue}", f"/organizations/{org}/issues/{issue}", f"/{org}/{project}/issues/{issue}"}
        if link.scheme == "https" and expected_host and "@" not in link.netloc and link.path.rstrip("/") in allowed_paths:
            return link._replace(query="", fragment="").geturl()
    except ValueError:
        pass
    return None


@dataclass(frozen=True)
class Issue:
    project: str
    id: str
    last_seen: float
    level: str
    link: str
    regression: str | None


def timestamp(value):
    dt = datetime.fromisoformat(value)
    if dt.tzinfo is None:
        raise ValueError("Expected a timezone.")
    return dt.timestamp()


def issue_rank(row, issue, environment):
    try:
        return (environment == "production", issue.level == "fatal", int(row.get("userCount") or 0),
                int(row.get("count") or 0), issue.last_seen)
    except (TypeError, ValueError, OverflowError):
        raise PollError("Sentry returned invalid issue counts.") from None


def alert_text(config, issue):
    mention = f"{config['target']}: " if config["target"] else ""
    return f"{mention}Sentry {issue.level} in {issue.project} — {issue.link}"


def parse_issue(config, row, status, cutoff):
    try:
        project, issue_id = row["project"]["slug"], str(row["id"])
        selected = project in config["projects"] or str(row["project"].get("id", "")) in config["projects"]
        if (not selected or not re.fullmatch(r"[0-9]{1,32}", issue_id)
                or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_-]{0,99}", project)):
            raise PollError("Sentry returned an unexpected project or issue ID.")
        if row["status"] != status or row["level"] not in {"error", "fatal"} or row["lastSeen"] is None:
            return None
        last_seen = timestamp(row["lastSeen"])
        if last_seen < cutoff:
            return None
        link = issue_link(config, row.get("permalink"), project, issue_id)
        if status == "unresolved" and link is None:
            raise PollError("Sentry returned an unexpected issue permalink.")
        regression = None
        if row.get("substatus") == "regressed":
            marker = (row.get("inbox") or {}).get("reason_date")
            regression = str(timestamp(marker)) if marker else "regressed"
        issue = Issue(project, issue_id, last_seen, row["level"], link or "", regression)
        if status == "unresolved":
            wire = f"PRIVMSG {config['irc']['channel']} :{alert_text(config, issue)}\r\n"
            if len(wire.encode()) > 512:
                raise PollError("Sentry issue alert exceeded the IRC message limit.")
        return issue
    except PollError:
        raise
    except (KeyError, AttributeError, TypeError, ValueError, OverflowError, OSError):
        raise PollError("Sentry returned invalid issue data.") from None


class IRC:
    def __init__(self, sock):
        self.sock = sock
        self.stream = sock.makefile("rb")

    def send(self, line):
        if any(char in line for char in ("\r", "\n", "\0")):
            raise PollError("IRC command contains a line break or NUL.")
        payload = (line + "\r\n").encode()
        if len(payload) > 512:
            raise PollError("IRC command is too long.")
        self.sock.sendall(payload)

    def wait(self, expected, errors=(), rejected_cap=False):
        deadline = monotonic() + TIMEOUT
        while (remaining := deadline - monotonic()) > 0:
            self.sock.settimeout(remaining)
            try:
                line = self.stream.readline(8193).decode("utf-8", errors="replace").rstrip("\r\n")
            except TimeoutError:
                break
            if not line or len(line) > 8192:
                break
            if line.startswith("@"):
                _, _, line = line.partition(" ")
            sender = ""
            if line.startswith(":"):
                prefix, _, line = line.partition(" ")
                sender = prefix[1:].split("!", 1)[0]
            verb, _, args = line.partition(" ")
            if verb == "PING":
                self.send(f"PONG {args}")
            elif expected(verb, args, sender):
                self.sock.settimeout(TIMEOUT)
                return
            elif (verb == "ERROR" or verb in errors
                  or rejected_cap and verb == "CAP" and args.split()[1:2] == ["NAK"]):
                raise PollError("IRC rejected the medical ship command.")
        raise PollError("IRC did not confirm the medical ship command.")

    def register(self, account, password, channel):
        self.nick = account
        self.send("CAP LS 302")
        self.send(f"NICK {account}")
        self.send(f"USER {account} 0 * :Medical ship")
        self.send("CAP REQ :sasl echo-message")
        registration_errors = ("410", "421", "431", "432", "433", "436", "437", "461", "462", "464", "465")
        sasl_errors = (*registration_errors, "902", "904", "905", "906", "907")
        self.wait(lambda verb, args, _: verb == "CAP" and args.split()[1:2] == ["ACK"]
                  and {"sasl", "echo-message"} <= set(args.split(" :", 1)[-1].split()),
                  registration_errors, rejected_cap=True)
        self.send("AUTHENTICATE PLAIN")
        self.wait(lambda verb, args, _: (verb, args) == ("AUTHENTICATE", "+"), sasl_errors)
        encoded = base64.b64encode(f"\0{account}\0{password}".encode()).decode()
        for start in range(0, len(encoded), 400):
            self.send(f"AUTHENTICATE {encoded[start:start + 400]}")
        if len(encoded) % 400 == 0:
            self.send("AUTHENTICATE +")
        self.wait(lambda verb, *_: verb == "903", sasl_errors)
        self.send("CAP END")
        self.wait(lambda verb, args, _: verb == "001" and args.split()[:1] == [account], registration_errors)
        self.send(f"JOIN {channel}")
        self.wait(lambda verb, args, _: verb == "366" and args.split()[1:2] == [channel],
                  ("403", "405", "437", "451", "461", "471", "473", "474", "475", "476", "477", "489"))

    def announce(self, channel, message):
        args = f"{channel} :{message}"
        self.send(f"PRIVMSG {args}")
        self.wait(lambda verb, reply, sender: verb == "PRIVMSG" and reply == args and sender == self.nick,
                  ("401", "403", "404", "411", "412", "451", "461"))


@contextmanager
def irc_connection(config):
    with socket.create_connection((config["host"], config["port"]), timeout=TIMEOUT) as sock:
        if config["tls"]:
            context = ssl.create_default_context()
            if "caFile" in config:
                context.load_verify_locations(cafile=config["caFile"])
            sock = context.wrap_socket(sock, server_hostname=config["host"])
        with sock:
            irc = IRC(sock)
            with irc.stream:
                irc.register(config["account"], config["password"], config["channel"])
                yield irc


def prepare_receipts(db):
    db.execute("PRAGMA busy_timeout = 5000")
    db.execute("""
        CREATE TABLE IF NOT EXISTS alerts (
            origin TEXT, org TEXT, environment TEXT, project TEXT, issue TEXT,
            PRIMARY KEY (origin, org, environment, project, issue)
        )
    """)
    columns = {row[1] for row in db.execute("PRAGMA table_info(alerts)")}
    for name, kind in (("alerted_at", "REAL"), ("last_seen", "REAL"), ("regression", "TEXT"), ("resolved_seen", "REAL")):
        if name not in columns:
            db.execute(f"ALTER TABLE alerts ADD COLUMN {name} {kind}")
    db.execute("""
        CREATE TABLE IF NOT EXISTS poll_state (
            scope TEXT PRIMARY KEY, pending TEXT, outage TEXT, retry_at REAL
        )
    """)
    db.commit()


def scope(config):
    return config["baseUrl"], config["organisation"], config["environment"]


def scan(config, db, candidates, cutoff):
    environment = config["environment"]
    searches = [(environment, PAGE_LIMIT)] if environment else [("production", 1), ("", PAGE_LIMIT)]
    unreadable = None
    for env, pages in searches:
        rows = recent_issues(config, "unresolved", env, pages, missing_ok=not environment and env == "production")
        for row in rows:
            try:
                issue = parse_issue(config, row, "unresolved", cutoff)
                if issue is None:
                    continue
                rank = issue_rank(row, issue, env)
            except PollError as error:
                unreadable = unreadable or error
                continue
            key = (issue.project, issue.id)
            if key in candidates:
                earlier_issue, earlier_rank = candidates[key]
                issue = max(issue, earlier_issue, key=lambda item: item.last_seen)
                rank = max(rank, earlier_rank)
            candidates[key] = (issue, rank)
    if db.execute("SELECT 1 FROM alerts WHERE (origin, org, environment) = (?, ?, ?) LIMIT 1", scope(config)).fetchone():
        for row in recent_issues(config, "resolved", environment, 1):
            if issue := parse_issue(config, row, "resolved", cutoff):
                key = (issue.project, issue.id)
                candidate = candidates.get(key)
                if candidate and candidate[0].last_seen <= issue.last_seen:
                    del candidates[key]
                db.execute(f"UPDATE alerts SET resolved_seen = max(coalesce(resolved_seen, 0), ?) WHERE {ISSUE_KEY}",
                           (issue.last_seen, *scope(config), *key))
    if unreadable:
        raise unreadable


def eligible(db, key, issue):
    receipt = db.execute(f"SELECT last_seen, regression, resolved_seen FROM alerts WHERE {ISSUE_KEY}", key).fetchone()
    if receipt is None:
        return True
    last_seen, regression, resolved_seen = receipt
    marker = issue.regression
    if regression == "regressed" and marker not in (None, "regressed") and float(marker) <= last_seen:
        # Sentry can add the inbox date after we first see a regression.
        db.execute(f"UPDATE alerts SET regression = ? WHERE {ISSUE_KEY}", (marker, *key))
        regression = marker
    newer = last_seen is None or issue.last_seen > last_seen
    if not newer:
        return False
    if marker is None and regression == "regressed":
        # Without an inbox date, observe an intervening exit before re-alerting.
        db.execute(f"UPDATE alerts SET regression = NULL WHERE {ISSUE_KEY}", key)
    regressed = marker is not None and (
        regression is None or (marker != "regressed" and (regression == "regressed" or float(marker) > float(regression)))
    )
    return regressed or (resolved_seen is not None and issue.last_seen > resolved_seen)


def save_state(db, state_key, pending, outage, retry_at):
    db.execute("INSERT OR REPLACE INTO poll_state VALUES (?, ?, ?, ?)",
               (state_key, json.dumps([asdict(issue) for issue in pending]), outage, retry_at))
    db.commit()


def report(db, config, state_key, pending, outage, retry_at, healthy):
    if not pending and outage != "pending" and (outage != "reported" or not healthy):
        return 0
    room = config["irc"]["channel"]
    count = len(pending)
    with irc_connection(config["irc"]) as irc:
        if outage == "pending":
            irc.announce(room, POLL_FAILED)
            outage = "reported"
            save_state(db, state_key, pending, outage, retry_at)
        if outage == "reported" and healthy:
            irc.announce(room, POLL_RECOVERED)
            outage = "healthy"
            save_state(db, state_key, pending, outage, retry_at)
        while pending:
            issue = pending[0]
            irc.announce(room, alert_text(config, issue))
            key = (*scope(config), issue.project, issue.id)
            db.execute(f"INSERT OR IGNORE INTO alerts {ISSUE_FIELDS} VALUES (?, ?, ?, ?, ?)", key)
            db.execute(f"""
                UPDATE alerts SET alerted_at = ?, last_seen = ?, regression = ?, resolved_seen = NULL
                WHERE {ISSUE_KEY}
            """, (time(), issue.last_seen, issue.regression, *key))
            pending.pop(0)
            save_state(db, state_key, pending, outage, retry_at)
    return count


def poll_locked(config, db):
    state_key = json.dumps((*scope(config), sorted(config["projects"]), config.get("http5xxTag")))
    state = db.execute("SELECT pending, outage, retry_at FROM poll_state WHERE scope = ?", (state_key,)).fetchone()
    saved, outage, retry_at = state or ("[]", "healthy", 0)
    now = time()
    cutoff = now - LOOKBACK
    pending = []
    for item in json.loads(saved):
        issue = Issue(**item)
        if issue.last_seen >= cutoff and eligible(db, (*scope(config), issue.project, issue.id), issue):
            pending.append(issue)
    error = None
    backing_off = now < retry_at
    if not backing_off:
        candidates = {}
        try:
            scan(config, db, candidates, cutoff)
        except PollError as failure:
            error = failure
        retry_at = error.retry_at if error else 0
        if error and outage == "healthy":
            outage = "pending"
        queued = {(issue.project, issue.id) for issue in pending}
        for issue, _ in sorted(candidates.values(), key=lambda entry: entry[1], reverse=True):
            if len(pending) == ALERT_LIMIT:
                break
            key = (issue.project, issue.id)
            if key not in queued and eligible(db, (*scope(config), *key), issue):
                pending.append(issue)
    save_state(db, state_key, pending, outage, retry_at)
    count = report(db, config, state_key, pending, outage, retry_at, not backing_off and error is None)
    if error:
        raise error
    return count


def poll(config, database):
    # systemd serialises service starts. Also cover direct invocations with the same receipt path.
    with open(database + ".lock", "a", encoding="utf-8") as lock:
        try:
            flock(lock, LOCK_EX | LOCK_NB)
        except BlockingIOError:
            raise PollError("Another medical poll is running.") from None
        with closing(sqlite3.connect(database)) as db:
            prepare_receipts(db)
            return poll_locked(config, db)


def main(config_path, database):
    os.umask(0o077)
    try:
        with open(config_path, encoding="utf-8") as stream:
            config = json.load(stream)
        count = poll(config, database)
    except PollError as error:
        sys.exit(f"Medical ship: {error}")
    except (KeyError, AttributeError, TypeError, ValueError, OSError, sqlite3.Error, OverflowError):
        sys.exit("Medical ship: could not read poll settings, issue data, IRC transport or receipts.")
    print(f"Medical ship delivered {count} new Sentry alert(s).")


if __name__ == "__main__":
    main(*sys.argv[1:])
