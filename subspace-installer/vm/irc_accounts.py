"""Certificate-only IRC accounts for a fleet's officers on a remote server (subspace-setup).

With an IRC operator's credential, register each officer's account with NickServ and bind its certificate's
fingerprint (Ergo's SAREGISTER and CERT ADD), then check that the certificate signs in by SASL EXTERNAL.
The operator's password goes only to the server, over TLS, and appears in no message or file.
"""

import secrets
import socket
import ssl

TIMEOUT = 20


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
            self.sock.close()
        except OSError:
            pass


def nick():
    return f"ss-setup-{secrets.token_hex(3)}"


def register(host, port, ca_file, operator, password, fingerprints):
    """As the IRC operator, register each account with no password and bind its certificate fingerprint.

    fingerprints: {account: SHA-256 hex}. Returns NickServ's replies by account, for the operator to read; an
    account or fingerprint that already exists only gets a reply saying so. check_login says whether it worked.
    """
    connection = Connection(host, port, ca_file)
    replies = {}
    try:
        name = nick()
        connection.send(f"NICK {name}")
        connection.send(f"USER {name} 0 * :subspace-setup")
        connection.until("001")
        connection.send(f"OPER {operator} {password}")
        _, command, _ = connection.until("381", "464", "491")
        if command != "381":
            raise IrcError(f"The IRC server {host} refused the operator credential for {operator}.")
        for account, fingerprint in fingerprints.items():
            replies[account] = []
            for request in (f"SAREGISTER {account}", f"CERT ADD {account} {fingerprint}"):
                connection.send(f"PRIVMSG NickServ :{request}")
                source, _, params = connection.until("NOTICE")
                while source.lower() != "nickserv":
                    source, _, params = connection.until("NOTICE")
                replies[account].append(params[-1])
    finally:
        connection.close()
    return replies


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
