# Self-hosted Headscale access

Subspace Desktop builds with embedded Headscale support can reach the
fleet's private HTTPS API and TLS IRC through a separate Headscale
coordinator. Each workstation enrols once with a device key. The API and IRC
still authenticate people normally. Keep a separate private SSH path for
fleet administration.

## Prepare the coordinator

Use a dedicated Ubuntu 24.04 LTS VM with a static public IPv4 address and a
provider management channel that you have tested, such as Azure Run
Command, AWS Systems Manager or the cloud console. If the user asks you to
create the VM, use their confirmed account and size via the provider's
tools. Enable Secure Boot and a virtual TPM if the image and provider support
them. Put the coordinator on its own subnet or network and allow only
inbound TCP 443 in the provider firewall. Keep the fleet VM private.

Create a public DNS A record for the coordinator. When using Cloudflare DNS,
set the record to **DNS only**. Let's Encrypt uses TLS-ALPN-01 and Headscale
uses a custom protocol upgrade on the same TCP 443 listener, so use direct
TLS to the coordinator. The built-in relay is for enrolled nodes and the
configuration needs no public STUN port.
[Headscale reverse proxy requirements](https://headscale.net/stable/ref/integration/reverse-proxy/)
and [Headscale TLS](https://headscale.net/stable/ref/tls/).

From the Subspace checkout, review the plan. Then send
`subspace-installer/vm/headscale.sh` with the same four arguments through the
provider's management channel and run it as root on the coordinator. A
checkout on the coordinator can use the same `install.sh` command without
`--dry-run`. The script closes SSH, so execute it through the tested
management channel.

```sh
./install.sh headscale --dry-run headscale.example.com 203.0.113.42 10.20.0.4 10.20.0.5
```

The script updates Ubuntu from its configured repositories, enables
unattended Ubuntu security updates and installs a checksum-pinned stable
Headscale release. It runs Headscale under a dedicated system account and
systemd sandbox, limits local administration to its UNIX socket and loads a
default-drop host firewall. Reboot through the management channel if setup
reports that Ubuntu requires it. Check that the firewall and Headscale units
are active afterwards. Verify trusted TLS from another machine.

```sh
curl --fail https://headscale.example.com/health
```

## Enrol the private router

The API and IRC must have private IPv4 addresses that the router itself can
reach and valid certificates for the names Desktop uses. The two services
can share an address. Use a production sign-in provider for the API. For an
installer-managed API, see
[direct private access](private-access.md#an-existing-private-network) for
its SSO and listener requirements. Verify from the intended router that TCP
443 and 6697 reach the correct listeners before enrolling it.

On the coordinator, use the local CLI from a private administrator session.
Create the two Headscale roles once. Create a short-lived, single-use key
with the router tag and transfer it via your approved secret delivery
channel. Keep both the source and destination files private. With a
provider-run command that keeps its output, use an approved secret store or
encrypt the key before it leaves the coordinator.

```sh
umask 077
hs() {
    sudo -u subspace-headscale /usr/local/bin/subspace-headscale \
        --config /etc/subspace-headscale/config.yaml "$@"
}
hs users create router
hs users create workstation
hs users list
hs preauthkeys create --user ROUTER_ID --tags tag:router --expiration 20m --output json \
    | jq -er .key >router.key
```

On the fleet VM or an existing private host with the required path, install
the router. The key file must be root-owned with mode 0600. Remove your
copies of the key after the device joins. The router uses an unprivileged
userspace Tailscale service, advertises the specified host routes and does
not change the host's firewall or SSH policy.

```sh
sudo ./install.sh headscale-router headscale.example.com 10.20.0.4 10.20.0.5 /run/router.key
```

## Optional operator HTTPS gateway

A fleet with an existing private HTTPS gateway can offer its operator
services over the same private network. Use a private address distinct
from the API and IRC addresses. Give it only virtual hosts intended for the
operators and keep their existing application authentication. Access is
to the gateway's HTTPS listener, including the names it serves on that
address and port.

On a new coordinator, pass the gateway address as the fifth argument.
On the private router, pass the same address with `--operator-gateway`.
The operator and workstation roles both get the API and IRC. Only the
operator role gets TCP 443 on the gateway. Create the `operator` user on the
coordinator and issue its one-use keys only to authorised administrators.
Leave the extra address out for ordinary API and IRC access.

```sh
./install.sh headscale --dry-run headscale.example.com 203.0.113.42 10.20.0.4 10.20.0.5 10.20.0.6
sudo ./install.sh headscale-router --operator-gateway 10.20.0.6 \
    headscale.example.com 10.20.0.4 10.20.0.5 /run/router.key
```

Verify the gateway's trusted HTTPS origins from an operator device. Verify
that an ordinary workstation cannot reach the gateway.

## Enrol Subspace Desktop

For each workstation, create a fresh key for the `workstation` user. The
network policy grants its enrolled devices API TCP 443 and IRC TCP 6697.

```sh
hs preauthkeys create --user WORKSTATION_ID --expiration 20m --output json \
    | jq -er .key >workstation.key
```

With a Desktop build that supports embedded networking, write this
`embedded-network.json` alongside its `connection.json` in the Subspace
application data directory. Configure the HTTPS API origin and identity
provider as usual in `connection.json`.

```json
{
  "control_url": "https://headscale.example.com",
  "api_address": "10.20.0.4",
  "irc_host": "irc.example.com",
  "irc_port": 6697,
  "irc_address": "10.20.0.5"
}
```

Place the device key next to it as `embedded-network.key`, restricted to
that workstation's user. Desktop consumes the key after enrolment. Start
Desktop with the previous VPN or SSH tunnel disconnected and verify
authenticated API and IRC access. Remove a retired device through the
coordinator's local Headscale CLI.

## Maintain the coordinator

Ubuntu security updates run unattended. Schedule reboots when required.
Before updating Headscale, read its release and upgrade notes and back up
`/etc/subspace-headscale` and `/var/lib/subspace-headscale` while the service
is stopped. Keep the backups private because the state includes device
registrations and private keys. Update to a Subspace installer revision
with the desired stable Headscale release, then rerun setup with the same
endpoint and routes. It preserves the installed access policy and validates
the release checksum and configuration before restarting the service. Check the TLS endpoint, enrolment and one
existing workstation after the upgrade.
[Headscale upgrade documentation](https://headscale.net/stable/setup/upgrade/).
