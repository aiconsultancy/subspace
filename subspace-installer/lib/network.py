"""Render the fleet's host-local service access rules."""


def rules(ports, admins, model_clients, view_user):
    """Match local client socket owners, independently of the listener UID."""
    admin_ids = {0, *admins}
    listeners = ", ".join(str(port) for port in sorted(ports.values()))
    output = []
    service_clients = {"proxy": set(model_clients), "agentsview": {view_user}}
    for service, port in ports.items():
        allowed = admin_ids | service_clients.get(service, set())
        ids = ", ".join(str(uid) for uid in sorted(allowed))
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
        "}",
        "",
    ])
