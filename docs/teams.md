# Connect a fleet to Microsoft Teams

The fleet's **rear admiral** can talk to your stakeholders in Microsoft Teams.
**Teams bots cannot message each other**, a Teams limitation, so captains and
crew cannot coordinate through Teams; they stay on IRC, and only the rear
admiral uses Teams. If your stakeholders use Slack, [Slack](slack.md) is
simpler.

Teams needs more than Slack:

- **An Azure VM.** Stargate signs its replies with a user-assigned managed
  identity through Azure's metadata service. There is no bot secret.
- **One public HTTPS callback.** Microsoft's Bot Service delivers messages to
  `https://<your-host>/api/messages`. Publish **only** that path, and send it to
  Stargate's private Unix socket. Everything else on the VM stays private.
  Stargate verifies every request's Bot Framework signature, so the callback
  needs no other gate. Do not put an identity login (such as Cloudflare Access)
  in front of it, because Microsoft's servers cannot sign in.

Coding agents: walk the user through this, and ask before creating Azure
resources or a public hostname. Never weaken the VM's lockdown for Teams; the
callback arrives through an outbound tunnel.

## 1. Identity and Azure Bot

Use your own resource group, names and IDs:

```sh
az identity create -g <resource-group> -n fleet-teams-bot
az vm identity assign -g <resource-group> -n <fleet-vm> \
  --identities "$(az identity show -g <resource-group> -n fleet-teams-bot --query id -o tsv)"
az bot create -g <resource-group> -n <bot-name> --app-type UserAssignedMSI --sku F0 \
  --appid "$(az identity show -g <resource-group> -n fleet-teams-bot --query clientId -o tsv)" \
  --tenant-id "$(az account show --query tenantId -o tsv)" \
  --msi-resource-id "$(az identity show -g <resource-group> -n fleet-teams-bot --query id -o tsv)" \
  --endpoint https://teams-bot.example.com/api/messages
az bot msteams create -g <resource-group> -n <bot-name>
```

Give the identity no other Azure roles. The lockdown blocks the agents' Linux
users from Azure's metadata service, but Stargate's gateway user needs it for
the bot, and a rear admiral on the Claude Agent SDK runs as that user. Keep the
identity limited to the bot. With `--key-vault` on the same VM, setup refuses
Teams for a Claude Agent SDK rear admiral, which would share that user; use an
OpenCode rear admiral or separate VMs.

## 2. The Teams app

In the [Teams Developer Portal](https://dev.teams.microsoft.com/), create an
app named after the rear admiral. Under **App features > Bot**, choose
**Enter a bot ID** and use the identity's client ID. Allow the **Personal**
scope, and **Team** and **Group chat** if the rear admiral should work in
channels. Publish it to your organisation, or upload it as a custom app.

## 3. Collect the IDs

- **People:** Entra object IDs of everyone allowed to talk to the rear admiral:
  `az ad user show --id person@example.com --query id -o tsv`.
- **Channels (optional):** in Teams, use **Get link to channel**. The URL holds
  the channel ID (`19:…@thread.tacv2`, URL-encoded).
- **Tenant ID:** `az account show --query tenantId -o tsv`.

## 4. Publish only the callback

With Cloudflare Zero Trust, add a public hostname to the fleet's tunnel (see
[Private access](private-access.md#cloudflare-zero-trust)) that sends exactly
`/api/messages` to Stargate's socket. Return 404 for everything else. In a
locally managed tunnel's `config.yml`:

```yaml
ingress:
  - hostname: teams-bot.example.com
    path: ^/api/messages$
    service: unix:/var/lib/subspace-fleet/teams/bot.sock
  - service: http_status:404
```

In the dashboard, choose the **UNIX** service type with the same path and
socket. `cloudflared` running as root can open the socket. If it runs as
another user, put that user in a group and set `socketGroup` below. Another
HTTPS reverse proxy works the same way: publish exactly that path to that
socket, and nothing else.

## 5. Configure the fleet

Add the Teams settings to `/root/fleet/fleet.json` on the VM
(`./install.sh ssh`, then `sudo nano /root/fleet/fleet.json`):

```json
"stakeholders": {
  "teams": {
    "botClientId": "11111111-2222-4333-8444-555555555555",
    "tenantId": "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    "users": ["99999999-8888-4777-8666-555555555555"],
    "channels": []
  }
}
```

Then apply it from your computer, which re-runs setup with your saved answers
and confirms the lockdown (use the same `--target` as the install, if any):

```sh
./install.sh --yes
```

The lockdown keeps the agents away from Azure's metadata service. With Teams
configured, setup lets Stargate's gateway user reach it, because the bot signs
in with the managed identity. Watch the gateway on the VM (`./install.sh ssh`):

```sh
sudo journalctl --namespace=subspace-fleet -u subspace-fleet-gateway.service -f
```

Send the bot a personal message from an allowed account. Stargate refuses
other tenants, people and channels, and messages from bots.

## Notes

- **Slack and Teams together:** both can be configured for the same rear
  admiral; see [Slack](slack.md).
- **Approvals in Teams:** Stargate can offer native permission decisions to
  chosen allowed users. See
  [Stargate's Teams guide](https://github.com/aiconsultancy/stargate/blob/df03d10b039c126a2fc0b5522bf7841ef07bc799/docs/teams.md),
  which also covers authentication and delivery in detail.
- **Several fleets:** each fleet's rear admiral needs its own Azure Bot,
  identity, Teams app and callback hostname.
