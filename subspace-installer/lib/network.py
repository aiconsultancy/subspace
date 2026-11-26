"""Render host-local service access and officer egress rules."""

from ipaddress import ip_address
from pathlib import Path
import socket
from urllib.parse import urlsplit

PRIVATE_V4 = "10.0.0.0/8, 100.64.0.0/10, 169.254.0.0/16, 172.16.0.0/12, 192.168.0.0/16"
PRIVATE_V6 = "fc00::/7, fe80::/10"


def endpoint(value):
    parts = urlsplit(f"//{value}")
    if parts.username is not None or parts.path or parts.query or parts.fragment or not parts.hostname or not parts.port:
        raise ValueError("Expected HOST:PORT or [IPv6]:PORT.")
    return parts.hostname, parts.port


def destinations(host, port, uid=None):
    entries = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    return {(ip_address(result[4][0].split("%", 1)[0]), port, uid) for result in entries}


def resolvers(path=Path("/etc/resolv.conf")):
    addresses = set()
    for line in path.read_text().splitlines():
        fields = line.split()
        if fields and fields[0] == "nameserver" and len(fields) > 1:
            try:
                addresses.add(ip_address(fields[1].split("%", 1)[0]))
            except ValueError:
                continue
    return addresses


def egress_rules(officers, dns, allowed):
    if not officers:
        return []
    ids = ", ".join(str(uid) for uid in sorted(officers))
    ruleset = [
        "    chain private_egress {",
        # Match the destination before Docker rewrites a published loopback port.
        "        type filter hook output priority -110; policy accept;",
        f"        meta skuid {{ {ids} }} jump officer_destinations",
        "    }",
        "    chain officer_destinations {",
    ]
    for address in sorted(dns, key=lambda value: (value.version, int(value))):
        family = "ip" if address.version == 4 else "ip6"
        for protocol in ("tcp", "udp"):
            ruleset.append(f"        {family} daddr {address} {protocol} dport 53 return")
    for address, port, uid in sorted(allowed, key=lambda row: (row[0].version, int(row[0]), row[1], row[2] or 0)):
        family = "ip" if address.version == 4 else "ip6"
        owner = f"meta skuid {uid} " if uid is not None else ""
        ruleset.append(f"        {owner}{family} daddr {address} tcp dport {port} return")
    ruleset += [f"        ip daddr {{ {PRIVATE_V4} }} reject",
                f"        ip6 daddr {{ {PRIVATE_V6} }} reject", "    }"]
    return ruleset


def rules(ports, admins, model_clients, view_user, *, officers=(), dns=(), allowed=()):
    """Match local client socket owners, independently of the listener UID."""
    admin_ids = {0, *admins}
    listeners = ", ".join(str(port) for port in sorted(ports.values()))
    output = []
    service_clients = {"proxy": set(model_clients), "agentsview": {view_user}}
    for service, port in ports.items():
        users = admin_ids | service_clients.get(service, set())
        ids = ", ".join(str(uid) for uid in sorted(users))
        output.append(f'        oifname "lo" tcp dport {port} meta skuid != {{ {ids} }} reject with tcp reset')
    return "\n".join([
        "add table inet subspace_fleet",
        "delete table inet subspace_fleet",
        "table inet subspace_fleet {",
        "    chain output {",
        "        type filter hook output priority filter; policy accept;",
        *output,
        "    }",
        "    chain input {",
        "        type filter hook input priority filter; policy accept;",
        f'        iifname != "lo" tcp dport {{ {listeners} }} reject with tcp reset',
        "    }",
        *egress_rules(officers, dns, allowed),
        "}",
        "",
    ])
