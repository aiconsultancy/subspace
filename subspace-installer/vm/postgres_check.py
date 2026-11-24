"""Read-only checks of an existing PostgreSQL for subspace-setup's dry run, with no driver to install.

A minimal client for PostgreSQL's frontend/backend protocol 3.0: TLS as the API's driver uses it, password,
MD5 and SCRAM-SHA-256 authentication, and simple queries in a read-only session. Messages never include the
URL or the password.
"""

import base64
import hashlib
import hmac
import secrets
import socket
import ssl
import struct
from urllib.parse import parse_qs, unquote, urlsplit

TIMEOUT = 15
SSL_REQUEST = 80877103
PROTOCOL = 196608  # 3.0
VERIFIED_TLS = ("require", "verify-ca", "verify-full")


class DatabaseError(Exception):
    """The server could not be reached, refused the sign-in or a query, or broke the protocol."""


def parameters(url):
    """Host, port, database, user, password and sslmode from a postgresql:// URL."""
    parts = urlsplit(url)
    query = parse_qs(parts.query)
    return {
        "host": parts.hostname or (query.get("host") or [""])[0],
        "port": parts.port or 5432,
        "database": unquote(parts.path.lstrip("/")) or (query.get("dbname") or [""])[0],
        "user": unquote(parts.username or ""),
        "password": unquote(parts.password or ""),
        "sslmode": (query.get("sslmode") or [""])[-1],
    }


def scram_proof(password, client_nonce, server_first, client_first_bare):
    """The SCRAM-SHA-256 client-final message and the server signature to expect (RFC 5802, RFC 7677)."""
    fields = dict(item.split("=", 1) for item in server_first.split(","))
    if not fields.get("r", "").startswith(client_nonce):
        raise DatabaseError("The PostgreSQL server's SCRAM nonce does not extend this client's.")
    salted = hashlib.pbkdf2_hmac("sha256", password.encode(), base64.b64decode(fields["s"]), int(fields["i"]))
    client_key = hmac.new(salted, b"Client Key", hashlib.sha256).digest()
    without_proof = f"c=biws,r={fields['r']}"
    auth_message = f"{client_first_bare},{server_first},{without_proof}".encode()
    signature = hmac.new(hashlib.sha256(client_key).digest(), auth_message, hashlib.sha256).digest()
    proof = bytes(a ^ b for a, b in zip(client_key, signature, strict=True))
    server_key = hmac.new(salted, b"Server Key", hashlib.sha256).digest()
    expected = base64.b64encode(hmac.new(server_key, auth_message, hashlib.sha256).digest()).decode()
    return f"{without_proof},p={base64.b64encode(proof).decode()}", expected


class Connection:
    """A read-only session on the server a URL names. tls says whether it is encrypted."""

    def __init__(self, url, ca_file=None):
        options = parameters(url)
        self.tls, self.user = False, options["user"]
        try:
            self.sock = socket.create_connection((options["host"], options["port"]), timeout=TIMEOUT)
        except OSError as error:
            raise DatabaseError(f"cannot reach the server ({error.__class__.__name__})") from None
        if options["sslmode"] not in ("", "disable"):
            self.sock.sendall(struct.pack("!ii", 8, SSL_REQUEST))
            answer = self.sock.recv(1)
            if answer == b"S":
                # As the API's driver does: the chain and the host name, for every mode that uses TLS.
                context = ssl.create_default_context(cafile=ca_file)
                try:
                    self.sock = context.wrap_socket(self.sock, server_hostname=options["host"])
                except (ssl.SSLError, OSError) as error:
                    raise DatabaseError(f"TLS with the server failed ({getattr(error, 'reason', None) or 'error'}); "
                                        "pass --database-ca-file for a private CA") from None
                self.tls = True
            elif options["sslmode"] in VERIFIED_TLS:
                raise DatabaseError(f"the server does not offer TLS, which sslmode={options['sslmode']} needs")
        self.buffer = b""
        self.send(None, struct.pack("!i", PROTOCOL) + b"".join(
            key.encode() + b"\0" + value.encode() + b"\0" for key, value in (
                ("user", options["user"]), ("database", options["database"]),
                ("application_name", "subspace-setup-check"), ("client_encoding", "UTF8")) if value) + b"\0")
        self.authenticate(options["password"])
        self.query("SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY")

    def send(self, kind, body):
        message = struct.pack("!i", len(body) + 4) + body
        self.sock.sendall((kind or b"") + message)

    def read(self, count):
        while len(self.buffer) < count:
            try:
                data = self.sock.recv(65536)
            except (OSError, ssl.SSLError):
                data = b""
            if not data:
                raise DatabaseError("The PostgreSQL server closed the connection.")
            self.buffer += data
        data, self.buffer = self.buffer[:count], self.buffer[count:]
        return data

    def message(self):
        kind = self.read(1)
        length = struct.unpack("!i", self.read(4))[0]
        body = self.read(length - 4)
        if kind == b"E":
            fields = {item[:1]: item[1:].decode("utf-8", "replace") for item in body.split(b"\0") if item}
            raise DatabaseError(f"{fields.get(b'M', 'error')} (SQLSTATE {fields.get(b'C', '?')})")
        return kind, body

    def authenticate(self, password):
        nonce = client_first_bare = expected = None
        while True:
            kind, body = self.message()
            if kind == b"Z":
                return
            if kind != b"R":
                continue
            code = struct.unpack("!i", body[:4])[0]
            if code == 3:
                self.send(b"p", password.encode() + b"\0")
            elif code == 5:
                inner = hashlib.md5(password.encode() + self.user.encode()).hexdigest()
                self.send(b"p", b"md5" + hashlib.md5(inner.encode() + body[4:8]).hexdigest().encode() + b"\0")
            elif code == 10:
                if b"SCRAM-SHA-256" not in body[4:].split(b"\0"):
                    raise DatabaseError("The PostgreSQL server offers no SASL mechanism this check knows.")
                nonce = base64.b64encode(secrets.token_bytes(18)).decode()
                client_first_bare = f"n=,r={nonce}"
                first = f"n,,{client_first_bare}".encode()
                self.send(b"p", b"SCRAM-SHA-256\0" + struct.pack("!i", len(first)) + first)
            elif code == 11:
                final, expected = scram_proof(password, nonce, body[4:].decode(), client_first_bare)
                self.send(b"p", final.encode())
            elif code == 12:
                if dict(item.split("=", 1) for item in body[4:].decode().split(",")).get("v") != expected:
                    raise DatabaseError("The PostgreSQL server's SCRAM signature does not match.")
            elif code != 0:
                raise DatabaseError(f"The PostgreSQL server asks for an authentication method this check lacks ({code}).")

    def query(self, sql, *args):
        """Rows of a simple query as lists of text values (None for NULL). args are quoted as literals."""
        text = sql % tuple("'" + str(arg).replace("'", "''") + "'" for arg in args) if args else sql
        self.send(b"Q", text.encode() + b"\0")
        rows = []
        while True:
            kind, body = self.message()
            if kind == b"D":
                count, offset, row = struct.unpack("!h", body[:2])[0], 2, []
                for _ in range(count):
                    size = struct.unpack("!i", body[offset:offset + 4])[0]
                    offset += 4
                    row.append(None if size < 0 else body[offset:offset + size].decode("utf-8", "replace"))
                    offset += max(size, 0)
                rows.append(row)
            elif kind == b"Z":
                return rows

    def close(self):
        try:
            self.send(b"X", b"")
            self.sock.close()
        except OSError:
            pass


def applied_migrations(connection):
    """The names in Drizzle's migration history, or an empty set when the database has none."""
    if connection.query("SELECT to_regclass('drizzle.__drizzle_migrations')::text")[0][0] is None:
        return set()
    return {row[0] for row in connection.query("SELECT name FROM drizzle.__drizzle_migrations") if row[0]}


def administrator(connection, organisation, email):
    """'empty' (no organisation at all), 'admin', 'member', 'agent' or 'missing' for the development sign-in's
    person in the existing database, as subspace-api's auth:dev with DEV_TOKEN_EXISTING_ONLY sees them."""
    if not connection.query("SELECT 1 FROM organisations LIMIT 1"):
        return "empty"
    rows = connection.query(
        "SELECT u.role, u.agent_rank FROM users u JOIN organisations o ON o.id = u.organisation_id "
        "WHERE o.provider = 'dev' AND o.external_id = %s AND u.subject = %s", organisation, email.lower())
    if not rows:
        return "missing"
    role, rank = rows[0]
    return "agent" if rank else role
