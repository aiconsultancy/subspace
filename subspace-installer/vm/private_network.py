#!/usr/bin/python3
"""Workstation ingress on an existing private route. Backends stay on loopback."""

import argparse
from dataclasses import dataclass
from errno import ENOTEMPTY
import grp
from ipaddress import IPv4Address, IPv4Network, IPv6Address, IPv6Network, ip_address, ip_network
import json
import os
from pathlib import Path
import pwd
import re
import subprocess
import sys
import tempfile


PORTS = (443, 6697)
PRIVATE_RANGES = tuple(map(ip_network, ("10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "100.64.0.0/10", "fc00::/7")))
HAPROXY = Path("/usr/sbin/haproxy")
USER = "subspace-ingress"
ACCOUNT = ("Subspace private ingress", "/nonexistent", "/usr/sbin/nologin")
# Keep the receipt with the separately persistent firewall state, not with the proxy files.
ACCOUNT_RECEIPT = Path("/etc/subspace-lockdown/private-ingress-account")
RECEIPT_IDS = re.compile(r"[1-9][0-9]*:([1-9][0-9]*)\n")
UNIT = Path("/etc/systemd/system/subspace-private-network.service")
GUARD = UNIT.with_name("subspace-private-network-check.service")
TIMER = GUARD.with_suffix(".timer")
CONFIG = Path("/etc/subspace-private-network/haproxy.cfg")
TLS = CONFIG.with_name("tls.pem")
CERT_INPUT = "private-network/fullchain.pem"
KEY_INPUT = "private-network/privkey.pem"


class PrivateNetworkError(ValueError):
    pass


class RetirementBlocked(PrivateNetworkError):
    """Off could not finish retiring the proxy account. Its proxy and exemption are already gone."""


@dataclass(frozen=True)
class Network:
    name: str
    interface: str
    address: IPv4Address | IPv6Address
    clients: IPv4Network | IPv6Network

    @classmethod
    def parse(cls, value):
        if value in (None, "off"):
            return None
        try:
            name, interface, raw_address, raw_clients = value.split(",")
            address, clients = ip_address(raw_address), ip_network(raw_clients)
        except (AttributeError, ValueError):
            raise PrivateNetworkError("Use --private-network NAME,INTERFACE,ADDRESS,CLIENT-CIDR or off.") from None
        name = name.lower()
        if len(name) > 253 or name.endswith(".localhost") or not re.fullmatch(
                r"(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?", name):
            raise PrivateNetworkError("Use a private DNS name, not an IP address or a localhost name.")
        if interface == "lo" or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]{0,14}", interface):
            raise PrivateNetworkError("Name the private network's non-loopback Linux ingress interface.")
        if "%" in raw_address or address.version != clients.version or not any(
                address.version == scope.version and address in scope for scope in PRIVATE_RANGES) or not any(
                clients.version == scope.version and clients.subnet_of(scope) for scope in PRIVATE_RANGES):
            raise PrivateNetworkError("The listener address and client CIDR must be private, of the same IP family.")
        if address in clients:
            raise PrivateNetworkError("Keep the listener address outside the client CIDR.")
        return cls(name, interface, address, clients)

    @property
    def choice(self):
        return f"{self.name},{self.interface},{self.address},{self.clients}"

    def check_interface(self):
        links = json.loads(command("ip", "-j", "address", "show", "dev", self.interface))
        if not any(ip_address(item["local"]) == self.address
                   for link in links for item in link.get("addr_info", [])):
            raise PrivateNetworkError("Assign the private listener address to the named interface first.")

    def proxy_config(self, api_port, irc_port):
        bind = f"[{self.address}]" if self.address.version == 6 else str(self.address)
        # SO_BINDTODEVICE and the connection ACL also restrict ingress if a pending lockdown rolls back.
        # No caller-supplied identity is trusted. HAProxy only unwraps TLS, then streams to the loopback services.
        header = ("global\n    log stdout format raw local0\n\n"
                  "defaults\n    mode tcp\n    log global\n    option tcplog\n"
                  "    timeout connect 5s\n    timeout client 1h\n    timeout server 1h\n")
        return header + "".join(
            f"\nlisten {name}\n"
            f'    bind {bind}:{port} interface {self.interface} ssl crt "${{CREDENTIALS_DIRECTORY}}/tls.pem" '
            "ssl-min-ver TLSv1.2\n"
            f"    tcp-request connection reject unless {{ src {self.clients} }}\n"
            f"    server loopback 127.0.0.1:{target}\n"
            for name, port, target in zip(("api", "irc"), PORTS, (api_port, irc_port), strict=True)
        )

    @property
    def firewall_target(self):
        return ",".join(map(str, (self.interface, self.address, self.clients, *PORTS)))

    def audit_firewall(self):
        command(Path(__file__).resolve().with_name("lockdown.sh"), "check-private-ingress",
                self.firewall_target, timeout=22, report_stderr=True)


@dataclass(frozen=True, repr=False)
class IngressPlan:
    network: Network
    api_port: int
    config: str
    bundle: str

    @classmethod
    def prepare(cls, network, inputs, api_port, irc_port, *, secret_file, write_private, run):
        network.check_interface()
        if not HAPROXY.is_file():
            raise PrivateNetworkError("Install the distribution's HAProxy package on the VM first.")
        cert, key = inputs / CERT_INPUT, inputs / KEY_INPUT
        cert_pem = secret_file(cert, "--private-network", "private TLS certificate chain")
        key_pem = secret_file(key, "--private-network", "private TLS key")
        check_certificate(network, cert, key)
        config = network.proxy_config(api_port, irc_port)
        bundle = f"{cert_pem}\n{key_pem}\n"
        with tempfile.TemporaryDirectory(prefix="subspace-ingress-") as scratch:
            write_private(Path(scratch) / "tls.pem", bundle)
            config_path = Path(scratch) / "haproxy.cfg"
            write_private(config_path, config)
            run(HAPROXY, "-c", "-f", config_path, capture=True, keep_stdin=False,
                env={**os.environ, "CREDENTIALS_DIRECTORY": scratch})
        return cls(network, api_port, config, bundle)

    @property
    def isolation_args(self):
        return ["--port-user", f"{self.api_port}:{USER}"]

    def quiesce(self, *, write_private, run):
        disable()
        ensure_account(write_private=write_private, run=run)

    def enable(self, lockdown_id, *, write_private, run):
        write_private(CONFIG, self.config)
        write_private(TLS, self.bundle)
        write_private(UNIT, unit_text(self.network))
        write_private(GUARD, guard_text(self.network))
        write_private(TIMER, timer_text())
        run("systemctl", "daemon-reload")
        # If the guard cannot be armed, do not bring the proxy up unguarded.
        run("systemctl", "enable", "--now", TIMER.name)
        run(Path(__file__).resolve().with_name("lockdown.sh"), "start-private-ingress", lockdown_id)


def account_entries():
    try:
        account = pwd.getpwnam(USER)
    except KeyError:
        account = None
    try:
        group = grp.getgrnam(USER)
    except KeyError:
        group = None
    return account, group


def account_identity(account, group, *, members=False):
    if (account and group and account.pw_uid > 0 and account.pw_gid == group.gr_gid > 0
            and (members or not group.gr_mem) and (account.pw_gecos, account.pw_dir, account.pw_shell) == ACCOUNT):
        return f"{account.pw_uid}:{group.gr_gid}\n"
    return None


def account_receipt():
    try:
        return ACCOUNT_RECEIPT.read_text(errors="replace")
    except FileNotFoundError:
        return None


def owns_account(account, group, receipt):
    return receipt is not None and receipt == account_identity(account, group)


def receipt_unreadable(error=PrivateNetworkError):
    first = ("Lockdown no longer exempts the proxy's UID." if error is RetirementBlocked else
             "Rerun setup with --private-network off first, so that lockdown no longer exempts the proxy's UID.")
    return error(
        f"Cannot read the receipt {ACCOUNT_RECEIPT}, which is not a uid:gid pair. Setup cannot tell which {USER} "
        f"account or group it created, so it will not adopt or remove one. {first} Check 'getent passwd {USER}' "
        f"and 'getent group {USER}', and once nothing depends on an account or group that remains, remove or rename "
        f"it. Then move the receipt out of the active path, for example to /root/{ACCOUNT_RECEIPT.name}.bad, "
        "before enabling direct access again. See Recovering the proxy account in docs/private-access.md.")


def account_conflict(receipt):
    if receipt is not None and RECEIPT_IDS.fullmatch(receipt) is None:
        return receipt_unreadable()
    return PrivateNetworkError(
        f"Cannot verify ownership of the {USER} account and group: setup will not adopt them. The receipt, "
        f"{ACCOUNT_RECEIPT}, must name their uid:gid, and they must keep the profile setup gave them. To recover, "
        "rerun setup with --private-network off, so that lockdown no longer exempts the receipt's UID and setup "
        "retires what the receipt proves it created. Remove or rename whatever remains, run off again to clear "
        "the receipt, then enable. See Recovering the proxy account in docs/private-access.md.")


def retirement_blocked(state, remedy="Once nothing depends on the account or group that remains, remove or rename "
                                     "it, and rerun to clear the receipt."):
    return RetirementBlocked(
        f"Cannot finish retiring the {USER} account: {state}. {remedy} Lockdown no longer exempts its UID, and "
        f"setup removes only what its receipt, {ACCOUNT_RECEIPT}, proves it created, keeping the receipt until "
        "then. See Recovering the proxy account in docs/private-access.md.")


def members_blocked(group):
    return retirement_blocked(f"its group lists {', '.join(group.gr_mem)} as members",
                              f"Remove each with 'gpasswd -d MEMBER {USER}' and rerun: setup then retires it.")


def ensure_account(*, write_private, run):
    account, group = account_entries()
    receipt = account_receipt()
    if owns_account(account, group, receipt):
        return
    if account is not None or group is not None or receipt is not None:
        raise account_conflict(receipt)
    description, home, shell = ACCOUNT
    # Unlike sysusers, useradd fails if the account or group already exists.
    run("useradd", "--system", "--user-group", "--no-create-home", "--comment", description,
        "--home-dir", home, "--shell", shell, "--", USER)
    identity = account_identity(*account_entries())
    if identity is None:
        raise account_conflict(None)
    write_private(ACCOUNT_RECEIPT, identity)


def retire_account(*, run):
    # The new isolation rules no longer exempt this UID. Only now may it be recycled. Finish only what
    # the receipt proves setup created, including a retirement that stopped part-way. Its refusals and a failing
    # userdel or groupdel are RetirementBlocked, which setup reports and carries on from; nothing else is.
    receipt = account_receipt()
    if receipt is None:
        return
    recorded = RECEIPT_IDS.fullmatch(receipt)
    if recorded is None:
        raise receipt_unreadable(RetirementBlocked)
    account, group = account_entries()
    if account is not None:
        if not owns_account(account, group, receipt):
            if group is not None and group.gr_mem and account_identity(account, group, members=True) == receipt:
                raise members_blocked(group)
            raise retirement_blocked("the account or its group does not match the receipt")
        if status := run("userdel", "--", USER, check=False):
            raise retirement_blocked(f"userdel failed with exit code {status}")
        # userdel normally removes a private group; also handle USERGROUPS_ENAB=no.
        _, group = account_entries()
    if group is not None:
        if group.gr_gid != int(recorded[1]):
            raise retirement_blocked("the group does not match the receipt")
        if group.gr_mem:
            raise members_blocked(group)
        if status := run("groupdel", "--", USER, check=False):
            raise retirement_blocked(f"groupdel failed with exit code {status}, for example because the group is "
                                     "still another account's primary group")
    ACCOUNT_RECEIPT.unlink()


def command(*args, timeout=10, report_stderr=False):
    label = f"{Path(args[0]).name} {args[1]}"
    try:
        result = subprocess.run(args, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=timeout,
                                env={**os.environ, "LC_ALL": "C"})
    except subprocess.TimeoutExpired:
        raise PrivateNetworkError(f"Private network check timed out ({label}).") from None
    if result.returncode:
        message = result.stderr.strip() if report_stderr else ""
        raise PrivateNetworkError(message or f"Private network command failed ({label}).")
    return result.stdout.strip()


def require_sso(environment):
    providers = {value.strip() for value in environment.get("AUTH_PROVIDERS", "").split(",")}
    if not providers <= {"workos", "entra"} or environment.get("NODE_ENV") != "production" or \
            environment.get("ENABLE_DEV_AUTH") != "false" or environment.get("AGENT_PORT"):
        raise PrivateNetworkError("Private ingress requires WorkOS or Entra in production, with development sign-in "
                                  "and the header-only agent listener off. Configure the API before enabling it.")


def check_running_api():
    # Check the running container, not just api.env: a failed Compose update may have left the old API serving.
    containers = command("docker", "ps", "-q", "--filter", "label=com.docker.compose.project=subspace-api",
                         "--filter", "label=com.docker.compose.service=api").splitlines()
    if len(containers) != 1:
        raise PrivateNetworkError("Expected exactly one running installer-managed Subspace API container.")
    env = json.loads(command("docker", "inspect", "--format", "{{json .Config.Env}}", containers[0]))
    require_sso(dict(line.split("=", 1) for line in env))


def check_certificate(network, cert, key):
    leaf_key = command("openssl", "x509", "-in", cert, "-noout", "-pubkey")
    if leaf_key != command("openssl", "pkey", "-in", key, "-passin", "pass:", "-pubout"):
        raise PrivateNetworkError("The private TLS certificate and key do not match.")
    if "DNS:" not in command("openssl", "x509", "-in", cert, "-noout", "-ext", "subjectAltName"):
        raise PrivateNetworkError("The private TLS certificate needs a DNS subject alternative name.")
    # This checks the leaf's hostname, validity and purpose, not the workstation's trust chain.
    command("openssl", "verify", "-partial_chain", "-trusted", cert, "-verify_hostname", network.name,
            "-purpose", "sslserver", cert)


def unit_text(network):
    return f"""[Unit]
Description=Subspace private workstation access
Wants=network-online.target
Requires=docker.service
After=network-online.target subspace-lockdown.service docker.service subspace-fleet-irc.service
StartLimitIntervalSec=60s
StartLimitBurst=6

[Service]
Type=notify
User={USER}
Group={USER}
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK
SystemCallFilter=@system-service
AmbientCapabilities=CAP_NET_BIND_SERVICE CAP_NET_RAW
CapabilityBoundingSet=CAP_NET_BIND_SERVICE CAP_NET_RAW
LoadCredential=haproxy.cfg:{CONFIG}
LoadCredential=tls.pem:{TLS}
ExecStartPre=+{sys.executable} -B {Path(__file__).resolve()} verify {network.choice}
ExecStart={HAPROXY} -Ws -f %d/haproxy.cfg
Restart=on-failure
RestartSec=5s

[Install]
WantedBy=multi-user.target
"""


def guard_text(network):
    return f"""[Unit]
Description=Check private workstation access

[Service]
Type=oneshot
ExecCondition=/usr/bin/systemctl is-active --quiet {UNIT.name}
ExecStart={sys.executable} -B {Path(__file__).resolve()} verify {network.choice} --stop-on-failure
TimeoutStartSec=60s
"""


def timer_text():
    return """[Unit]
Description=Recheck Subspace private ingress every minute

[Timer]
OnActiveSec=60s
OnUnitActiveSec=60s
AccuracySec=1s

[Install]
WantedBy=timers.target
"""


def disable():
    # Stop the timer and drain its checker before stopping the proxy.
    for path, action in ((TIMER, ("disable", "--now")), (GUARD, ("stop",)), (UNIT, ("disable", "--now"))):
        if path.is_file():
            command("systemctl", *action, path.name)


def remove(*, systemd=True):
    units = [path for path in (TIMER, GUARD, UNIT) if path.is_file()]
    if systemd and units:
        disable()
    for path in units:
        path.unlink(missing_ok=True)
    directory = CONFIG.parent
    if directory.is_dir():
        CONFIG.unlink(missing_ok=True)
        TLS.unlink(missing_ok=True)
        try:
            directory.rmdir()
        except FileNotFoundError:
            pass
        except OSError as error:
            if error.errno != ENOTEMPTY:
                raise
    if systemd and units:
        command("systemctl", "daemon-reload")


def verify(choice):
    network = Network.parse(choice)
    if network is None:
        raise PrivateNetworkError("A private network choice is required.")
    receipt = account_receipt()
    if not owns_account(*account_entries(), receipt):
        raise account_conflict(receipt)
    network.audit_firewall()
    check_running_api()


def main(argv=None):
    parser = argparse.ArgumentParser()
    subcommands = parser.add_subparsers(dest="command", required=True)
    check = subcommands.add_parser("verify")
    check.add_argument("choice")
    check.add_argument("--stop-on-failure", action="store_true")
    cleanup = subcommands.add_parser("remove")
    cleanup.add_argument("--no-systemd", action="store_true")
    listeners = subcommands.add_parser("listeners")
    listeners.add_argument("choice")
    subcommands.add_parser("ports")
    args = parser.parse_args(argv)
    if args.command == "remove":
        remove(systemd=not args.no_systemd)
        return
    if args.command == "ports":
        print(*PORTS, sep=",")
        return
    if args.command == "listeners":
        network = Network.parse(args.choice)
        if network:
            # Match the canonical bind address that ss reports, not the saved spelling.
            print(network.address, *PORTS)
        return
    try:
        verify(args.choice)
    except Exception:
        # An unexpected result from Docker or nft must fail closed, too.
        if args.stop_on_failure:
            command("systemctl", "stop", "--no-block", UNIT.name)
        raise


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError) as error:
        sys.exit(f"Private network: {error}")
