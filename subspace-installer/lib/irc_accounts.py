"""Certificate-only IRC accounts for a fleet's officers on a remote server (subspace-setup).

With an IRC operator's credential, register each officer's account with NickServ and bind its certificate's
fingerprint (Ergo's SAREGISTER and CERT ADD), then check that the certificate signs in by SASL EXTERNAL.
The operator's password goes only to the server, over TLS, and appears in no message or file.
"""

import base64
import re
import secrets
import socket
import ssl

TIMEOUT = 20
# NickServ's reply when SAREGISTER created the account (Ergo: "Successfully registered account NAME").
CREATED = re.compile(r"successfully registered", re.I)


class IrcError(Exception):
    """The IRC server could not be reached, refused a step, or did not answer in time."""


def parse(line):
    """An IRC line as (source nick, command, params)."""
    source = ""
    if line.startswith(":"):
        source, _, line = line[1:].partition(" ")
    head, _, trailing = line.partition(" :")
    words = head.split()
    params = words[1:] + ([trailing] if " :" in line else [])
    return source.split("!", 1)[0], (words[0].upper() if words else ""), params


class Connection:
    """One TLS connection to the server, optionally presenting a client certificate bundle."""

    def __init__(self, host, port, ca_file=None, client_pem=None, timeout=TIMEOUT):
        context = ssl.create_default_context(cafile=ca_file)
        if client_pem:
            context.load_cert_chain(client_pem)
        try:
            raw = socket.create_connection((host, port), timeout=timeout)
            self.sock = context.wrap_socket(raw, server_hostname=host)
        except (OSError, ssl.SSLError) as error:
            raise IrcError(f"Cannot connect to the IRC server {host}:{port} over TLS ({error.__class__.__name__}).") \
                from None
        self.host, self.buffer = host, b""

    def send(self, line):
        self.sock.sendall(line.encode() + b"\r\n")

    def receive(self):
        while b"\n" not in self.buffer:
            try:
                data = self.sock.recv(4096)
            except (OSError, ssl.SSLError):
                data = b""
            if not data:
                raise IrcError(f"The IRC server {self.host} closed the connection or did not answer in time.")
            self.buffer += data
        line, _, self.buffer = self.buffer.partition(b"\n")
        message = parse(line.decode("utf-8", "replace").rstrip("\r"))
        if message[1] == "PING":
            self.send("PONG :" + (message[2][-1] if message[2] else ""))
            return self.receive()
        if message[1] == "ERROR":
            raise IrcError(f"The IRC server {self.host} closed the connection: {' '.join(message[2])}")
        return message

    def until(self, *commands):
        """Read until one of commands (numerics included) arrives, and return that message."""
        while True:
            message = self.receive()
            if message[1] in commands:
                return message

    def close(self):
        try:
            self.send("QUIT :subspace-setup")
        except OSError:
            pass  # The server closed it first.
        try:
            self.sock.close()
        except OSError:
            pass


def nick():
    return f"ss-setup-{secrets.token_hex(3)}"


ACCOUNT_REQUIRED = ("The IRC server {host} lets only signed-in accounts connect, so the operator's connection must "
                    "sign in to an IRC account first: pass --irc-operator-account and "
                    "--irc-operator-account-password-file.")


def sign_in(connection, host, account, password):
    """Sign the connection in to an IRC account with SASL PLAIN, before registration completes."""
    connection.send("AUTHENTICATE PLAIN")
    _, command, _ = connection.until("AUTHENTICATE", "904", "908")
    if command != "AUTHENTICATE":
        raise IrcError(f"The IRC server {host} does not accept SASL PLAIN for the account {account}.")
    connection.send("AUTHENTICATE " + base64.b64encode(f"\0{account}\0{password}".encode()).decode())
    _, command, _ = connection.until("903", "902", "904", "905", "906")
    if command != "903":
        raise IrcError(f"The IRC server {host} refused the sign-in of the account {account}.")


def register(host, port, ca_file, operator, password, fingerprints, account=None, registered=(), created=None):
    """As the IRC operator, register each account with no password and bind its certificate fingerprint.

    fingerprints: {account: SHA-256 hex}, for accounts whose certificate does not sign in yet. account: an IRC
    account and its password to sign in to first, for a server that admits only signed-in connections (Ergo's
    require-sasl). Returns NickServ's replies by account. An account SAREGISTER does not create (it exists, or
    NickServ refuses it) gets no CERT ADD, and stops the run once every other account is done.

    registered: accounts an earlier run created for these fingerprints (created() recorded them) and that stopped
    before binding them, which are only bound now. created(account, fingerprint) is called as soon as SAREGISTER
    creates an account, before its CERT ADD, so a run that stops between the two can finish the binding.
    """
    connection = Connection(host, port, ca_file)
    replies = {}
    try:
        name = nick()
        if account:
            connection.send("CAP REQ :sasl")
        connection.send(f"NICK {name}")
        connection.send(f"USER {name} 0 * :subspace-setup")
        if account:
            _, _, params = connection.until("CAP")
            if len(params) < 2 or params[1].upper() != "ACK":
                raise IrcError(f"The IRC server {host} does not offer SASL.")
            sign_in(connection, host, *account)
            connection.send("CAP END")
        _, command, params = connection.until("001", "FAIL")
        if command == "FAIL":
            if "ACCOUNT_REQUIRED" in params:
                raise IrcError(ACCOUNT_REQUIRED.format(host=host))
            raise IrcError(f"The IRC server {host} refused the connection: {params[-1] if params else ''}")
        connection.send(f"OPER {operator} {password}")
        _, command, _ = connection.until("381", "464", "491")
        if command != "381":
            raise IrcError(f"The IRC server {host} refused the operator credential for {operator}.")
        refused = []
        for target, fingerprint in fingerprints.items():
            if target in registered:
                replies[target] = [nickserv(connection, f"CERT ADD {target} {fingerprint}")]
                continue
            replies[target] = [nickserv(connection, f"SAREGISTER {target}")]
            # Bind a certificate only to an account this run (or an earlier one, recorded) created: an existing
            # account's certificates are never changed, whoever registered it.
            if not CREATED.search(replies[target][0]):
                refused.append(f"{target} (NickServ: {replies[target][0]})")
                continue
            if created:
                created(target, fingerprint)
            replies[target].append(nickserv(connection, f"CERT ADD {target} {fingerprint}"))
    finally:
        connection.close()
    if refused:
        raise IrcError(f"Setup did not register {', '.join(refused)} on {host}: the account already exists, or "
                       "NickServ refused it, and its certificate does not sign in to it. Setup never changes an "
                       "existing account's certificates; an operator of the IRC server must resolve it, for example "
                       "by binding the fingerprint printed above with NS CERT ADD, then run setup again.")
    return replies


def nickserv(connection, request):
    """Send NickServ a request and return its first reply."""
    connection.send(f"PRIVMSG NickServ :{request}")
    source, _, params = connection.until("NOTICE")
    while source.lower() != "nickserv":
        source, _, params = connection.until("NOTICE")
    return params[-1]


# NickServ INFO's first line for a registered account (Ergo: "Account: NAME"); any other first line means the
# name is not registered, or not known as an account or a reserved nick.
INFO_ACCOUNT = re.compile(r"Account: (\S+)")


def nickserv_lines(connection, request):
    """Send NickServ a request and return every line of its reply: the lines before the PONG to a PING sent
    after it, as the server answers in order."""
    token = f"ss-{secrets.token_hex(4)}"
    connection.send(f"PRIVMSG NickServ :{request}")
    connection.send(f"PING :{token}")
    lines = []
    while True:
        source, command, params = connection.receive()
        if command == "PONG" and params and params[-1] == token:
            return lines
        if command == "NOTICE" and source.lower() == "nickserv" and params:
            lines.append(params[-1])


def register_client(connection, host, client_pem, account):
    name = nick()
    if client_pem or account:
        connection.send("CAP REQ :sasl")
    connection.send(f"NICK {name}")
    connection.send(f"USER {name} 0 * :subspace-setup")
    if client_pem or account:
        _, _, params = connection.until("CAP")
        if len(params) < 2 or params[1].upper() != "ACK":
            raise IrcError(f"The IRC server {host} does not offer SASL.")
        if account:
            sign_in(connection, host, *account)
        else:
            connection.send("AUTHENTICATE EXTERNAL")
            _, command, _ = connection.until("AUTHENTICATE", "904", "908")
            if command == "AUTHENTICATE":
                connection.send("AUTHENTICATE +")
                connection.until("900", "902", "903", "904", "905", "906")
        connection.send("CAP END")
    _, command, params = connection.until("001", "FAIL")
    if command == "FAIL":
        if "ACCOUNT_REQUIRED" in params:
            raise IrcError(ACCOUNT_REQUIRED.format(host=host))
        raise IrcError(f"The IRC server {host} refused the connection: {params[-1] if params else ''}")


def require_private_channel(host, port, ca_file, channel, client_pem=None, account=None):
    """Read the channel's IRC modes without joining it or sending it any messages."""
    connection = Connection(host, port, ca_file, client_pem)
    try:
        register_client(connection, host, client_pem, account)
        connection.send(f"MODE {channel}")
        _, command, params = connection.until("324", "403", "442")
        if command != "324" or len(params) < 3 or params[1].lower() != channel or not {"i", "s"} <= set(params[2]):
            raise IrcError(f"{channel} must exist and report invite-only and secret modes.")
    finally:
        connection.close()


def accounts_taken(host, port, ca_file, names, client_pem=None, account=None):
    """Which of names NickServ knows as a registered account or a nick reserved by one, as {name: account}.

    Signs in first, when given, with a client certificate (SASL EXTERNAL) or an account and its password (SASL
    PLAIN), for a server that admits only signed-in connections. Read-only: NickServ INFO.
    """
    connection = Connection(host, port, ca_file, client_pem)
    try:
        register_client(connection, host, client_pem, account)
        taken = {}
        for wanted in names:
            lines = nickserv_lines(connection, f"INFO {wanted}")
            match = INFO_ACCOUNT.match(lines[0]) if lines else None
            if match:
                taken[wanted] = match.group(1)
        return taken
    finally:
        connection.close()


def check_password(host, port, ca_file, account, password):
    """Whether an account signs in with its password (SASL PLAIN)."""
    connection = Connection(host, port, ca_file)
    try:
        connection.send("CAP REQ :sasl")
        connection.send(f"NICK {nick()}")
        connection.send("USER subspace-setup 0 * :subspace-setup")
        _, _, params = connection.until("CAP")
        if len(params) < 2 or params[1].upper() != "ACK":
            raise IrcError(f"The IRC server {host} does not offer SASL.")
        try:
            sign_in(connection, host, account, password)
        except IrcError:
            return False
        return True
    finally:
        connection.close()


def check_login(host, port, ca_file, client_pem):
    """The account a certificate bundle signs in as by SASL EXTERNAL, or None when the server refuses it."""
    connection = Connection(host, port, ca_file, client_pem)
    try:
        name = nick()
        connection.send("CAP REQ :sasl")
        connection.send(f"NICK {name}")
        connection.send(f"USER {name} 0 * :subspace-setup")
        _, _, params = connection.until("CAP")
        if len(params) < 2 or params[1].upper() != "ACK":
            raise IrcError(f"The IRC server {host} does not offer SASL, which certificate sign-in needs.")
        connection.send("AUTHENTICATE EXTERNAL")
        _, command, _ = connection.until("AUTHENTICATE", "904", "908")
        if command != "AUTHENTICATE":
            return None
        connection.send("AUTHENTICATE +")
        _, command, params = connection.until("900", "902", "904", "905", "906")
        return params[2] if command == "900" and len(params) > 2 else None
    finally:
        connection.close()
