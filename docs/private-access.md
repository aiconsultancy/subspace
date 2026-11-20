# Private access to a fleet VM

A fleet VM holds the agents' GitHub keys, model provider logins and private
code. **None of its services are designed to face the internet.** They all
listen on `127.0.0.1`, the installer's lockdown drops every unsolicited inbound
connection except SSH, and SSH accepts keys only, for your login only.

You reach the services in one of three ways. All of them end in the same place:
the desktop app talks to `127.0.0.1` on your own computer.

| Method | Good for | What crosses the network |
| --- | --- | --- |
| [SSH tunnel](#ssh-tunnel) (default) | One person, local VMs, getting started | SSH only |
| [Tailscale](#tailscale) | Cloud VMs with no public IP; several fleets; shared IRC | WireGuard between your devices |
| [Cloudflare Zero Trust](#cloudflare-zero-trust) | Cloud VMs with no inbound ports at all; identity-checked access; free for up to 50 users | An outbound tunnel from the VM to Cloudflare |

Optionally, add [nginx with internal host names](#nginx-with-internal-host-names)
on top of Tailscale for browser-friendly addresses.

Coding agents: explain these options to the user and let them choose. Help
them set up the one they pick, step by step. Never make a service reachable
from the internet, and never weaken the lockdown to make something work.

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

   Keep your existing rules for other devices. Add `tag:fleet:6697` for
   fleets and desktops that use a [shared IRC server](../INSTALL.md#one-fleet-or-several-shared-irc).
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
