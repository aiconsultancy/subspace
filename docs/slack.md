# Connect a fleet to Slack

The fleet's **rear admiral** can talk to your stakeholders in Slack: answer
questions, send screenshots and gather requirements. Captains and crew stay on
IRC. Stargate uses Slack **Socket Mode**, an outbound connection from the
fleet VM to Slack, so the VM still needs no inbound ports.

Coding agents: walk the user through the Slack steps. Never ask them to paste
tokens into the chat. They put the tokens on the VM themselves, as shown in
step 3.

## 1. Create or update the Slack app

Use one Slack app per fleet, named after the rear admiral, for example
"Atlas". Create it at <https://api.slack.com/apps> (**Create New App > From
scratch**) in your workspace, or open the existing app. Then:

1. **OAuth & Permissions > Bot Token Scopes:** add these scopes, and reinstall
   or reauthorise the app when Slack prompts you:

   | Scope | Why |
   | --- | --- |
   | `app_mentions:read` | Requests that mention the bot |
   | `channels:history`, `groups:history`, `im:history` | Messages and thread context in channels, private channels and DMs it is in |
   | `chat:write` | Replies and status messages |
   | `reactions:write` | Reaction replies |
   | `commands` | Stargate's slash commands and interactive controls |
   | `files:read`, `files:write` | Reading attachments and uploading screenshots or evidence |

   Optionally add `users:read`, used to verify who pressed a button. Stargate's
   [Slack scopes guide](https://github.com/aiconsultancy/stargate/blob/df03d10b039c126a2fc0b5522bf7841ef07bc799/docs/slack-scopes.md)
   explains every scope and the broader development set.
2. **Socket Mode:** turn it on.
3. **Basic Information > App-Level Tokens:** generate a token, for example
   named `stargate-socket`, with the `connections:write` scope. Note the
   `xapp-…` token, and the **App ID** (`A…`) at the top of the page.
4. **Event Subscriptions:** turn events on. Under **Subscribe to bot events**,
   add `app_mention`, `message.channels`, `message.groups` and `message.im`.
5. **App Home:** under **Show Tabs**, allow users to send messages from the
   Messages tab, so stakeholders can DM the rear admiral.
6. **OAuth & Permissions:** install, or reinstall, the app to the workspace and
   note the **Bot User OAuth Token** (`xoxb-…`). A workspace admin may need to
   approve it.

Invite the bot into each channel it should work in: `/invite @Atlas`.

## 2. Collect the IDs

- **People:** each person allowed to talk to the rear admiral. In Slack, open
  their profile, then **More (⋮) > Copy member ID** (`U…`).
- **Channels:** each channel it may work in. Open the channel's details; the
  ID (`C…` or `G…`) is at the bottom.

Only these people and channels are accepted. Everyone else is ignored.

## 3. Put the tokens on the VM

Open a shell on the VM with `./install.sh ssh` (Windows: `.\install.ps1 ssh`).
Replace `atlas` with your rear admiral's ID, which is its name in lower case.
Each command asks for a token without showing it; paste it and press Enter:

```sh
sudo install -d -m 700 /root/fleet/fleet.secrets/atlas
sudo bash -c 'umask 077; read -rsp "Bot token (xoxb-…): " t; echo; printf "%s\n" "$t" > /root/fleet/fleet.secrets/atlas/slack-bot-token'
sudo bash -c 'umask 077; read -rsp "App token (xapp-…): " t; echo; printf "%s\n" "$t" > /root/fleet/fleet.secrets/atlas/slack-app-token'
```

## 4. Configure the fleet

Add a `stakeholders` block to `/root/fleet/fleet.json` (`sudo nano
/root/fleet/fleet.json`), with your IDs:

```json
"stakeholders": {
  "slack": {
    "appId": "A0123456789",
    "users": ["U0123ABCD"],
    "channels": ["C0123ABCD"]
  }
}
```

Then apply it from your computer, which re-runs setup with your saved answers
and confirms the lockdown (use the same `--target` as the install, if any):

```sh
./install.sh --yes
```

Then watch the gateway on the VM (`./install.sh ssh`):

```sh
sudo journalctl --namespace=subspace-fleet -u subspace-fleet-gateway.service -f
```

Setup keeps the block on later runs. Look for `Bot connected` in the gateway's
log, then mention the bot in an allowed channel. For quota notices in Slack,
also set `usage.reportChannel` to a channel ID (see the
[fleet installer's guide](../subspace-installer/README.md#crew-quota)).

## Notes

- **Several fleets:** give each fleet's rear admiral its own Slack app. One
  app's Socket Mode connection belongs to one Stargate.
- **Tokens:** they stay in root-only files under `/root/fleet/fleet.secrets/`,
  and are installed for the rear admiral's gateway only. To rotate them,
  regenerate them in Slack, replace the files and run `./install.sh --yes`
  again. Revoke them in Slack when you retire a fleet.
- **Adding scopes later:** Slack applies new scopes only after you reinstall
  the app, so reinstall it and replace `slack-bot-token` if the token changed.
