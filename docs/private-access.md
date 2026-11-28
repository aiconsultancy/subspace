# Private access to a fleet VM

A fleet VM holds the agents' GitHub keys, model provider logins and private
code. **None of its services are designed to face the internet.** By default
the services listen on `127.0.0.1`, the installer's lockdown drops unsolicited
inbound connections except SSH, and SSH accepts keys only, for your login only.

Keep an SSH route for administration. Workstations can either tunnel through
SSH or reach the API and IRC directly on an existing private network.

| Method | Good for | What crosses the network |
| --- | --- | --- |
| [SSH tunnel](#ssh-tunnel) (default for a local VM) | One person, local VMs, getting started | SSH only |
| [Tailscale for SSH](#tailscale) | Cloud VMs with no public IP; several fleets; shared IRC | SSH over WireGuard between your devices |
| [Cloudflare Zero Trust for SSH](#cloudflare-zero-trust) | Cloud VMs with no inbound ports at all; identity-checked access; free for up to 50 users | SSH over an outbound tunnel from the VM to Cloudflare |
| [Dedicated Headscale coordinator](headscale.md) | Self-hosted Subspace Desktop access through a separate public coordinator | HTTPS control and an authenticated encrypted relay to private API and IRC |
| [An existing private network](#an-existing-private-network) | Workstations already on the organisation's ZTNA, VPN or routed network | HTTPS and TLS IRC to a private name; no SSH tunnel for either |

Optionally, add [nginx with internal host names](#nginx-with-internal-host-names)
on top of Tailscale for browser-friendly addresses.

Coding agents: explain these options to the user and let them choose. Help
them set up the one they pick, step by step. Keep fleet services off the
internet and preserve the fleet lockdown.

## An existing private network

Use this when the VM and workstations already have a private route. The
installer does not create a VPN, change DNS or open the cloud firewall. Ask
the network operator for a stable private VM address, its Linux ingress
interface, the narrow client or connector CIDR **as seen by the VM**, and a
DNS name resolving to that address on the workstations. Use one IPv4 or
IPv6 address and a client CIDR of the same family that excludes the listener
address. Use the actual client or connector addresses, not the whole subnet
if it includes the listener. Allow TCP 443 and 6697 only on that private path
in the cloud and network access policies. Do not forward public traffic to
those ports, including through a public tunnel.

**Existing guided fleets:** production sign-in removes the local API block
from `fleet.json`. Officers lose installer-managed tracker access (Subspace
MCP and REST tools, signed enrolment and dispatch) until setup supports SSO
fleet registration. If you need those tools, keep your current sign-in and
leave private ingress off. If you choose to proceed, pass
`--accept-officer-tracker-access-loss` on the dry run and the real run that
remove the block. This flag is not saved.

The API must already be configured for production WorkOS or Entra sign-in.
Set `NODE_ENV=production`, `ENABLE_DEV_AUTH=false`, and `AUTH_PROVIDERS` to
`workos`, `entra` or `workos,entra` in the VM's root-only `/root/fleet/api.env`,
with the provider's settings from the [API guide](../subspace-api/README.md#external-sign-in).
Keep `AGENT_PORT` empty. Provision the organisation and its members using
the membership command in that guide. This version of setup does not
automatically register a local SSO API's fleet or agents or seed demo data.
Turn off guided agent dispatch for this run. **Never serve the development
API through a proxy**: reaching it is enough to act as its administrator.

Install the distribution's `haproxy` package on the VM. Subspace uses a
separate unit and does not use or reconfigure the package's own proxy
service. Check that it and any other proxy are not listening on these ports.
Provide a PEM certificate chain (leaf first) covering the private DNS name
and its matching unencrypted PEM key. Put them on the VM through your
approved private channel, as root-owned mode-0600 files:

- `/root/fleet/private-network/fullchain.pem`
- `/root/fleet/private-network/privkey.pem`

Use a certificate from the organisation's CA if all clients trust it, or
one issued by a public CA for a name you control using DNS validation. The
name can still resolve only in private DNS. Do not issue an untrusted
self-signed leaf or turn off verification. Distribute an internal CA to
the system trust store used by Desktop and to the runtimes the skills use:
Python/OpenSSL and, for a client that makes its own HTTPS requests, the
client's trust store. A CA trusted by all those runtimes avoids per-client
configuration. The installer checks the key and the leaf's DNS SAN
and validity; the workstation must verify the full chain. The installer
does not renew the certificate. Replace the two files and rerun setup to
pick up a renewal. The privacy audit warns if the installed certificate
expires within 30 days.

For example, on a fleet VM with a configured production API and no local
`api` block in `fleet.json`:

```sh
sudo subspace-setup --dry-run --no-agent-dispatch \
  --private-network fleet.example.com,eth0,10.20.1.4,10.21.0.0/24
sudo subspace-setup --no-agent-dispatch \
  --private-network fleet.example.com,eth0,10.20.1.4,10.21.0.0/24
```

Confirm the lockdown from a new SSH session, as setup directs. If using
`./install.sh --remote …`, pass the same options; put the certificates and
API settings on the VM first. Remove 443 and 6697 from any existing
`--allow-port` list: pass a new list or `--allow-port ''` to clear it.
`--access` still only selects the SSH policy;
for a routed ZTNA connector elsewhere, use `--access ssh --ssh-from` with
the source allowed to administer the VM. It can differ from the client CIDR.

The managed proxy listens on the one private address. It terminates TLS
and opens connections to the API and Ergo's plain listener **on loopback**.
Its unit retains `CAP_NET_RAW` for HAProxy's `interface` socket binding.
Use IRC TLS with SASL PLAIN, not client certificate login, on the proxy
address. Officer connections, local IRC certificate login and the SSH
tunnel still use their existing endpoints. PostgreSQL is not published;
the header-only agent listener, dashboard, AgentsView and Stargate Fleet
API are not proxied. Both backends see the relay's address, not individual
workstations. IRC accounts still authenticate separately, but an enforced
host ban or DLINE on the shared address affects all relayed clients. Look
for the original connection addresses in the proxy's journal:
`sudo journalctl -u subspace-private-network.service`.

The name, interface, address and client CIDR are saved in
`/root/fleet/answers.json`. Omit `--private-network` to reuse them or pass
`--private-network off` to remove the proxy and its inbound rules. Allow for
a brief interruption to direct connections on a setup rerun. The proxy's
guard rechecks the live firewall and API about once a minute and stops the
proxy if either check fails. It does not restart the proxy. Turn off direct
access **before** changing the API's sign-in outside guided setup; the
periodic check is a fallback, not a safe way to make that change.

While the guard or a lockdown rollback has stopped the proxy, a local user
on the VM can bind the private listener address on port 6697. Workstation
clients must verify the server's TLS certificate, as the agent skills do.

On a workstation, use `https://fleet.example.com` in a Desktop build for
your identity provider. For IRC, add `fleet.example.com:6697` with TLS on
and the Ergo account and password supplied by the operator. No SSH local
forward is needed. The [WorkOS agent skills](../agent-skills/README.md#hosted-api-with-workos)
use the same HTTPS origin for MCP. For their API-issued IRC login, the
registered fleet's IRC endpoint and `IRC_AUTH_SERVERS` must name
`fleet.example.com:6697` and Ergo must use the [API sign-in hook](../subspace-irc/README.md#sign-in-through-the-subspace-api).
An organisation administrator creates or edits the API's fleet record with
`ircHost: "fleet.example.com"`, `ircPort: 6697` and `ircTls: true`.
On a single VM, the installer mounts the hook files and points Ergo at the
loopback API's container when `api.env` sets:

```dotenv
IRC_CREDENTIAL_SECRET_FILE=/run/subspace-managed-irc/credential-secret
IRC_AUTH_HOOK_TOKEN_FILE=/run/subspace-managed-irc/hook-token
IRC_AUTH_ORGANISATION=workos:org_EXAMPLE
IRC_AUTH_SERVERS=fleet.example.com:6697
```

The current agent skills support WorkOS, not Entra. Network presence does
not substitute for sign-in.

Test on a workstation on that route, with the tunnel closed:

```sh
curl --fail https://fleet.example.com/health
openssl s_client -connect fleet.example.com:6697 -servername fleet.example.com \
  -verify_hostname fleet.example.com -verify_return_error </dev/null
```

Then sign in to the API and IRC, run `sudo subspace-setup audit` on the VM,
and verify the two ports cannot be reached from outside the allowed path.

### Recovering the proxy account

The proxy runs as the `subspace-ingress` system account and group. Setup
creates them only when neither exists and records their `uid:gid` in
`/etc/subspace-lockdown/private-ingress-account`. It never adopts an
account or group it did not create. Fleet uninstall keeps them, because
lockdown's isolation rules can still exempt the UID after a reboot.
`--private-network off` removes them and the receipt once its new isolation
rules no longer exempt that UID, and a rerun finishes a removal that stopped
part-way. If `off` finds an account, group or receipt it will not retire,
or `userdel` or `groupdel` fails, it prints a warning, keeps the receipt,
completes the rest of the run and repeats the warning last. Any other
failure, such as an error reading the receipt file, still stops the run.

If setup or the proxy's guard cannot verify the account, or `off` reports
one it cannot remove, compare `getent passwd subspace-ingress` and
`getent group subspace-ingress` with the receipt. Do not delete an account
the receipt names while lockdown may still exempt its UID: rerun setup with
`--private-network off` first. Then:

- If `off` lists added members, remove each with
  `sudo gpasswd -d MEMBER subspace-ingress`. It lists them only when they
  are all that differs from the receipt, so run `off` again to finish
  retiring the account and group and clear the receipt, then enable direct
  access.
- Otherwise, once nothing depends on the account or group that remains,
  remove or rename it: setup did not create it as it is now. Run `off` again
  to clear the receipt, then enable direct access.

If setup reports that it cannot read the receipt, because it does not hold
a `uid:gid` pair, it removes and adopts nothing: it cannot tell which
account or group it created. After `off`, check both with `getent`. Once
nothing depends on an account or group that remains, remove or rename it.
Then move the receipt out of the active path, keeping it, before you enable
direct access again:

```sh
sudo mv /etc/subspace-lockdown/private-ingress-account /root/private-ingress-account.bad
```

## SSH tunnel

This is what the installer sets up. `./install.sh tunnel` forwards the fleet's
ports to the same ports on `127.0.0.1` on your computer:

| Port | Service |
| --- | --- |
| 3000 | Subspace API |
| 6667, 6697 | IRC (plain and TLS) when IRC runs on the fleet VM |
| 8070 | Stargate dashboard |
| 8080 | AgentsView |
| 8484 | Stargate Fleet API, when it is on |

For a local VM, SSH listens only on your computer's loopback, so nothing else
can reach the VM. For a cloud VM, keep SSH closed to the internet:

- Best: no public IP. Reach SSH over Tailscale or Cloudflare (below).
- Otherwise, allow SSH only from your own address in the cloud firewall, and
  pass the same address to the installer with `--ssh-from 203.0.113.7/32`, so
  the VM's own firewall enforces it too.

The lockdown already sets: key-only logins, no root login, `AllowUsers` your
login only, no agent or X11 forwarding, local port forwarding only (no remote
forwarding), and at most three authentication attempts.

## Tailscale

[Tailscale](https://tailscale.com) puts your computer and the VM on a private
WireGuard network (a tailnet). It has a free plan for personal use.

1. Install Tailscale on the VM, from its package repository (Ubuntu 24.04
   shown; see Tailscale's docs for Debian):

   ```sh
   curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/noble.noarmor.gpg |
     sudo tee /usr/share/keyrings/tailscale-archive-keyring.gpg >/dev/null
   curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/noble.tailscale-keyring.list |
     sudo tee /etc/apt/sources.list.d/tailscale.list
   sudo apt-get update && sudo apt-get install -y tailscale
   sudo tailscale up --advertise-tags=tag:fleet
   ```

   On a cloud VM without public SSH, run these through your cloud's
   run-command or serial console, using an auth key: `sudo tailscale up
   --auth-key <key> --advertise-tags=tag:fleet`.
2. Restrict who can reach fleets in the tailnet policy. For example, only
   tailnet admins, and only SSH:

   ```json
   {
     "tagOwners": { "tag:fleet": ["autogroup:admin"] },
     "acls": [
       { "action": "accept", "src": ["autogroup:admin"], "dst": ["tag:fleet:22"] }
     ]
   }
   ```

   Keep your existing rules for other devices. For a
   [shared IRC server](../INSTALL.md#one-fleet-or-several-shared-irc) on a
   tailnet VM, add `tag:fleet:6697` to that rule's destinations for the
   desktops, and a rule from `tag:fleet` to `tag:fleet:6697` for the fleets.
3. Point the installer at the VM's tailnet name and limit SSH to the tailnet:

   ```sh
   ./install.sh --remote azureuser@fleet.example.ts.net --access tailscale --yes
   ```

   Then remove any public SSH rule from the cloud firewall, or the public IP
   itself, and run `./install.sh audit`.

The SSH tunnel then runs over the tailnet. Tailscale SSH is not needed; the
VM's own SSH, with its keys, stays in charge.

Do not use `tailscale serve` or `tailscale funnel` to publish the fleet's
services. Serve proxies traffic as root, past the per-user firewall, and Funnel
publishes to the internet.

## Cloudflare Zero Trust

[Cloudflare Zero Trust](https://www.cloudflare.com/zero-trust/) is free for up
to 50 users. A `cloudflared` tunnel on the VM makes an outbound connection to
Cloudflare, so the VM needs no inbound ports at all, and Cloudflare Access
checks each person's identity before letting them through.

1. In the Zero Trust dashboard, create a tunnel and copy its install token.
   On the VM, install `cloudflared` from Cloudflare's package repository and
   run `sudo cloudflared service install <token>`.
2. Add a public hostname to the tunnel for SSH only, for example
   `fleet-ssh.example.com` to `ssh://localhost:22`. Do not add hostnames for the
   API, IRC, dashboard or AgentsView; the SSH tunnel carries those.
3. Create an Access application for that hostname with a policy that allows
   only you (your email or identity provider group).
4. Install `cloudflared` on your computer and run the installer through it:

   ```sh
   ./install.sh --remote azureuser@fleet-ssh.example.com --access cloudflare \
     --ssh-proxy 'cloudflared access ssh --hostname %h' --host-key SHA256:... --yes
   ```

   The first connection opens your browser for the Access login.
5. Remove every inbound rule from the cloud firewall and run `./install.sh audit`.

## nginx with internal host names

Some teams prefer addresses like `https://dashboard.fleet.example.ts.net` to
port numbers. nginx on the fleet VM can map host names to the loopback
services, if it listens only on a private interface.

- Listen on the VM's tailnet address only, never on `0.0.0.0`, and allow the
  port in the lockdown by running the install command again with
  `--allow-port 443`. The lockdown then accepts that port from private address
  ranges only, and keeps doing so when setup runs again.
- Use real TLS: `sudo tailscale cert fleet.example.ts.net` gives a certificate
  for the MagicDNS name.
- The fleet's per-user firewall lets only root and listed operators reach the
  dashboard and AgentsView. Add nginx's user (`www-data`) to `operatorUsers` in
  `/root/fleet/fleet.json` and run `sudo subspace-setup`.
- Proxy the read-only views (dashboard, AgentsView). **Do not proxy the
  Subspace API while development sign-in is on**: anyone who reached it could
  act as its administrator. Switch the API to WorkOS or Entra first.

```nginx
server {
    listen 100.64.0.10:443 ssl;            # the VM's tailnet address
    server_name dashboard.fleet.example.ts.net;
    ssl_certificate     /etc/nginx/tls/fleet.crt;
    ssl_certificate_key /etc/nginx/tls/fleet.key;
    location / {
        proxy_pass http://127.0.0.1:8070;
        proxy_set_header Host 127.0.0.1;  # the dashboard expects a loopback Host
    }
}
```

## Check it

Whatever you choose, run the audit after every change:

```sh
./install.sh audit
```

From a machine outside your private network, nothing on the VM's public
address (if it has one) should answer.
