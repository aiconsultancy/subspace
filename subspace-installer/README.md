# Subspace fleet installer

A fleet is one Linux VM. Human **admirals** use **Starbase** to work out what
to build with the **rear admiral**, who coordinates the **captains** and talks
to stakeholders. Each captain has a ship channel and a **crew**. The default
crew is two Claude-family and two Codex-family workers. Each captain gets a
checkout of the configured repositories. Each crew member gets a dedicated
Git worktree per repository. Agents have separate Linux accounts.

Stargate routes IRC conversations to the agents. The installer defaults to
OpenCode V2, using CLIProxyAPI for models. Configure the model and harness
per role and crew family. The dashboard shows the primary Stargate gateway,
and AgentsView indexes OpenCode session databases for local browsing and
search. The installer can run a [medical ship](#medical-ship) to flag Sentry
errors and enable [quota notices](#crew-quota) so the rear admiral can
wind down crew as a model family's budget runs low.

A fleet can use an existing IRC network or host Ergo using the pinned
[Subspace IRC Compose project](../subspace-irc/). The desktop's Channels Space
and each fleet connect to IRC independently. The optional Subspace API stores
the fleet's advertised IRC endpoint and which initiatives belong to it.

## Quick start

Most people should start with [INSTALL.md](../INSTALL.md): the root
`install.sh` (macOS, Linux) and `install.ps1` (Windows) create or connect the
VM, generate this installer's inputs with [`vm/setup.sh`](vm/setup.sh)
(`subspace-setup` on the VM), add the Subspace API, lock the VM down and
connect the desktop. The rest of this page describes the installer itself.

Use a fresh Ubuntu 24.04 LTS or Debian 12 VM, on amd64 or arm64. Copy the
`subspace-installer/` directory onto the VM. Store the fleet files outside
the repository.

```sh
install -m 600 fleet.example.json ~/fleet.json
install -m 600 secrets.example.json ~/fleet.secrets.json
${EDITOR:-vi} ~/fleet.json ~/fleet.secrets.json
sudo ./install.sh --config ~/fleet.json --dry-run
sudo ./install.sh --config ~/fleet.json
sudo systemctl status subspace-fleet-gateway.service
```

For remote IRC, register a SASL account for every officer and crew member
and supply their passwords in `ircPasswords`. The default accounts for
captain `example-orion` are `example-orion-claude-1`,
`example-orion-claude-2`, `example-orion-codex-1` and
`example-orion-codex-2`. Add trusted human SASL accounts to
`irc.allowedAccounts`. Ask the IRC operator to register the ship, Starbase
and any initiative channels and grant access to the appropriate accounts.
For local IRC, [pin a revision that contains the Compose project](#local-irc-and-tls).

The installer validates JSON, installs base tooling and verified release
binaries, checks Stargate's configuration, and starts systemd services.
`--dry-run` requires Python 3 and validates the inputs without writing to the
host. `--no-systemd` prepares units and validates configuration and Compose
without activating systemd services or writing to the Subspace API.

### Model provider sign-in

Select exact CLIProxyAPI model IDs. OpenCode routes the `codex` family to
the OpenAI-compatible Responses endpoint and the `claude` family to the
Anthropic-compatible Messages endpoint. Verify the model IDs on your proxy.
Run the provider login as the proxy user on the VM.

```sh
sudo -H -u ss-fleet-proxy /opt/subspace-fleet/bin/cli-proxy-api \
  --config /etc/subspace-fleet/proxy.json --codex-device-login --no-browser
sudo -H -u ss-fleet-proxy /opt/subspace-fleet/bin/cli-proxy-api \
  --config /etc/subspace-fleet/proxy.json --codex-login --no-browser
sudo -H -u ss-fleet-proxy /opt/subspace-fleet/bin/cli-proxy-api \
  --config /etc/subspace-fleet/proxy.json --claude-login --no-browser
```

Use `--oauth-callback-port` and an SSH tunnel for a browser callback on a
remote VM. The installer leaves OAuth and model use to the operator.
Provider credentials are kept under `/var/lib/subspace-fleet/proxy/auth`. Restart
`subspace-fleet-proxy.service` if the process has not picked up the new
credentials. CLIProxyAPI uses its standard routing. The [upstream CLI flags](https://github.com/router-for-me/CLIProxyAPI/blob/v8.0.78/cmd/server/main.go)
describe the available logins.

### Crew and repositories

Set fleet-wide `crew.claude` and `crew.codex` defaults, or override either
under a captain's `crew`. Set a count from 0 to 10 per family. Crew IDs are
`<captain-id>-<family>-<number>`. Keep the corresponding IRC accounts and
passwords in step when adding or removing slots. Use the same IDs for
per-agent credential directories. To override a crew display name, set the
captain's `crewNames` map, for example `"crewNames": {"codex-1": "Rigel"}`.

List repositories in order as `{"id":"project","url":"https://github.com/example/project.git"}`
objects under `repositories`. The first is each agent's default working
directory. An empty list gives each agent an empty workspace. The rear
admiral gets read-only reference checkouts under
`/var/lib/subspace-fleet/references/<rear-admiral-id>/` that the mirror
service refreshes. The rear admiral can inspect them from its own Linux
account, including Git history.

Public checkouts and crew worktrees share a read-only Git object cache.
The installer refreshes its refs before creating a worktree and preserves
existing branches and work in progress. Backing objects are retained because
active or retired worktrees may still refer to them.

For a private GitHub repository, set `"private": true` and use an HTTPS
GitHub URL. Give each active agent a distinct GitHub App installed on that
repository with at least Contents read access. The installer mints a
repository-scoped read token for bootstrap and keeps private objects in
individual checkouts and backing repositories. For crew, put the App
metadata in the captain's `crewGitHubApps` map keyed by slot, for example
`"codex-1"`, and give the key file the full agent ID directory, for example
`orion-codex-1/github-app.pem`. Use a new repository ID when its URL or
visibility changes.

The `opencode` harness uses `permissions`. Select `claude-sdk` with
`family: "claude"` in a harness or under `crew.claude` to use the Claude
Agent SDK. Its `allowedTools` defaults to an empty list. Each SDK agent
runs its own Stargate process as its Linux user. SDK crew get tagged in
their captain's channel and have their own registration, while OpenCode
crew are nested under their captain in Stargate. The primary dashboard
shows registrations in the rear admiral's gateway. The pinned revision
has a Codex SDK adapter. The OpenAI Agents SDK (`openai-sdk`) is currently
unavailable.

### Instructions, permissions and toolchains

The installer renders role-specific OpenCode `AGENTS.md` and
`skills/fleet-*/SKILL.md` under each agent's `~/.config/opencode/`
using the fleet name, agent name and assigned channel. Edit the templates in
`lib/roles/` and re-run the installer to customise them. Claude SDK agents
receive the same role guidance and skill procedure in their system prompt.

The default OpenCode policy grants read, glob, grep, skills and Git inspection.
Captains and crew can edit their checkout, stage and commit locally, and run
common build and test commands. External paths, publication and other shell
commands require an explicit grant. Build and test commands can execute code
from the project, so use trusted repositories. Supply an ordered
`harness.permissions` array to replace the starter policy across the fleet
or for a single agent or crew family. Rules use `action`, `resource` and
`effect`. Put narrower rules after broader ones. See
[OpenCode permissions](https://opencode.ai/docs/permissions/).

The base tools include Git, Bun, a C/C++ build toolchain, and system Python with
virtual environments. Mise installs Node 24 by default. Use `toolchains` to
select versions of Node, Rust, Go, Python, kubectl, gh, Zig or Deno. For
example, `"toolchains": {"node":"24","go":"1.26","rust":"stable","kubectl":"1.35"}`
adds Go, Rust and kubectl. Use a full version to pin a tool or `false` to
disable it. Managed tools are placed on the PATH for fleet processes and
`fleetctl attach`. Repository-specific versions can still use the agent's own
tool setup.

### Channels

The rear admiral joins Starbase and each ship channel. Captains join all
ship channels. Allowlisted humans wake the rear admiral in Starbase and
the owning captain in its ship channel. Tag an agent by its current IRC
nick to wake it elsewhere. Stargate verifies the sender's SASL account
and keeps human authority separate from agent requests. Crew join their
captain's ship channel. Stargate uses IRC message IDs for strict wake rules.
A separate legacy `irc.commandChannel` is optional and uses mention-only
wakes.

### Local IRC and TLS

Set `irc.mode` to `local`, omit `irc.host` and `irc.tls`, and set
`irc.compose.ref` to the full commit SHA of a Subspace IRC revision that
supports `IRC_CHANNELS_FILE`. Set `irc.compose.operator` and the matching
`ircOperatorPassword` in your secrets file. The installer installs Docker
Engine from Docker's apt repository and runs that Compose project under systemd.
Ergo seeds the fleet's SASL accounts from the private secrets file. To seed
a human login, include its account in `irc.allowedAccounts` and its password
in `ircPasswords`. Account names in local mode are lowercase. Passwords must
fit 72 UTF-8 bytes and follow Ergo's passphrase rules. Supply the operator
password only in `ircOperatorPassword`. To rotate an existing account password,
change it in Ergo before updating the fleet secrets file and upgrading.

Ergo registers Starbase, the ship channels and any separate command channel
when the container starts. The installer generates `IRC_CHANNELS_FILE`.
Add the intended initiative channel names to `irc.compose.channels` to seed
them at the same time. The operator is the founder of seeded channels.
For existing channels, keep the configured founder consistent. To manage
access later, sign in as the operator and use your client's operator
login prompt or `/oper <operator> :<password>`.

Plain IRC and IRC WebSocket listen on loopback. To make the VM's Ergo
available to other machines, set `irc.compose.serverName` to its TLS name,
`tlsCert` and `tlsKey` to absolute PEM paths on the VM, and `tlsBind` to a
private or deliberately exposed interface IP. Set `irc.advertise` to the
corresponding host, port and `"tls": true` for desktop clients. Other fleets
use that same TLS endpoint in remote mode. They can trust a private CA with
`irc.caFile`.

## GitHub Apps per fleet member

Create a GitHub App and installation for each fleet member who uses GitHub.
Grant access to the assigned repositories and the GitHub permissions the
member needs. Git pushes need Contents write. Issue and pull request
operations use Issues and Pull requests permissions as appropriate. See
[GitHub's installation authentication guide](https://docs.github.com/en/apps/creating-github-apps/authenticating-with-a-github-app/authenticating-as-a-github-app-installation).

Add `githubApp` to that member's fleet entry. The identifiers below are
placeholders. Get the bot's numeric ID using
`gh api 'users/example-agent%5Bbot%5D' --jq .id` with the real app slug.

```json
{
  "githubApp": {
    "appId": 1,
    "installationId": 2,
    "clientId": "your-client-id",
    "botLogin": "example-agent[bot]",
    "botId": 3
  }
}
```

Store the private RSA key in `fleet.secrets/<member-id>/github-app.pem`
beside `fleet.json`, or pass `--secrets-dir DIR`. Set key mode to `0600`
before installing. Dispatch uses the same directory convention and accepts
an optional `secretsDir` path per inventory entry. It transfers the keys in
the same private SSH bundle as the fleet config.

Each enabled Linux user gets `~/.config/subspace/github-app.json` and
`github-app.pem` with mode `0600`. The installer keeps root-only copies
under `/etc/subspace-fleet/github-apps` for upgrades. Git uses the assigned bot
name and noreply email for commits and mints installation tokens for
`https://github.com` credentials. Use HTTPS Git remotes for that helper.
Run `subspace-github gh issue ...`, `subspace-github gh pr ...` or
`subspace-github gh api repos/example/project/issues` from the member's
environment. The wrapper mints a short-lived installation token with the
permissions needed for the requested operation. Issue and PR collaboration
and CI reads are allowed. Raw API calls are limited to selected read-only
repository endpoints. The wrapper rejects administrative commands and flags.
To use it from an operator shell, run
`sudo -H -u ss-orion /opt/subspace-fleet/bin/subspace-github gh ...`.

Git credentials request repository-scoped Contents access. The rear admiral
uses read access. Captain and crew Apps default to write access for pushing
feature branches. Set an App's `contents` to `"read"` for a read-only
installation. The installer provides a Git pre-push hook in managed
checkouts for App users and preserves existing hooks.
The hook blocks deletions and non-fast-forward changes to
`githubPolicy.protectedBranches` (defaults to `main`, `master` and
`release/*`). A repository-level `core.hooksPath` takes precedence.
Enforce the same protections in GitHub rulesets, omit the Apps from bypass
lists, and grant the Apps only the repository permissions they need. Agents
hold their own App keys, so GitHub must enforce the operation limits for
requests outside the wrapper. See [GitHub rulesets](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-rulesets/available-rules-for-rulesets).

Optional SSH signing uses the separate public key configured in
`git.signingKey`. GitHub App installation tokens authenticate pushes and API
requests. GitHub signs bot commits made through its API when the request
uses the app identity without custom author, committer or signature data.
See [GitHub's bot signature verification rules](https://docs.github.com/en/authentication/managing-commit-signature-verification/about-commit-signature-verification).
Rotate or revoke a retired member's App key from the App settings.

## Read-only cloud access

The reference deployment used Azure managed identity and Key Vault. The
public installer provisions individual credential files into
`~/.config/subspace/cloud/` for each agent. Put `kubeconfig` and
`database-dsn` in `<secrets-dir>/<agent-id>/` or its `cloud/` subdirectory,
and use `cloud/` for other client configurations. Supply owner-only regular
files.
Installed files are owned by the agent and have mode `0600`. The private
SSH bundle also carries these files. The VM keeps an owner-only input
snapshot for unattended upgrades.

Point clients explicitly at these files, for example
`KUBECONFIG=~/.config/subspace/cloud/kubeconfig`. Scope Kubernetes access
to the needed namespaces and read verbs, restrict database users to read
privileges on the required schemas, and use repository-scoped GitHub Apps.
Use short-lived credentials or rotate them regularly. Pass a fresh
`--secrets-dir` to replace individual credentials. Upgrades keep omitted
files and warn when reusing installed credentials without a secrets
directory. Use `--prune-secrets` with an existing secrets directory to
remove managed cloud files omitted for active agents and the keys for Apps
removed from their configuration. Credentials for enabled integrations must
still be supplied for that operation. Revoke credentials when retiring an
agent.

## Stakeholder messaging

Only the rear admiral connects to Slack or Teams. Either transport can
be configured independently. Captains and crew coordinate over IRC.
Step-by-step guides: [Slack](../docs/slack.md) and [Teams](../docs/teams.md).

- Set `stakeholders.slack` to an `appId` and explicit `users` and
  `channels` arrays of Slack IDs. Supply Socket Mode app and bot tokens
  as `<secrets-dir>/<rear-admiral-id>/slack-app-token` and
  `slack-bot-token`, both mode `0600`. See
  [Stargate's Slack scope list](https://github.com/aiconsultancy/stargate/blob/df03d10b039c126a2fc0b5522bf7841ef07bc799/docs/slack-scopes.md).
- Set `stakeholders.teams` to an Azure Bot `botClientId` and `tenantId`
  plus an explicit `users` array of Entra object IDs and optional
  `channels`. The supported Bot Framework backend uses an Azure
  user-assigned managed identity at runtime. Route the bot's HTTPS
  `/api/messages` callback to `/var/lib/subspace-fleet/teams/bot.sock`.
  Set `socketGroup` to an existing Linux group for the trusted reverse
  proxy. Follow [Stargate's Teams guide](https://github.com/aiconsultancy/stargate/blob/df03d10b039c126a2fc0b5522bf7841ef07bc799/docs/teams.md).

## Medical ship

Set `medical.enabled` to `true` and specify the Sentry `organisation` slug
and `projects` to watch. Use project slugs with Sentry SaaS or self-hosted
Sentry 26.6.0 or later. Use quoted numeric project IDs on older installations.
`baseUrl` defaults to `https://sentry.io` and accepts the organisation's
regional API origin or your self-hosted HTTPS origin. If a self-hosted
instance serves issue links on a separate HTTPS host, set `permalinkHost`
to that hostname, for example `issues.example.com`. Links use HTTPS port 443
when this setting is supplied. Set `environment`
to filter issues, or omit it to cover all environments. `intervalSeconds`
defaults to 10,800 (three hours) and accepts 300 to 86,400. Increase any
shorter interval in the fleet configuration before upgrading.
[Sentry issues API](https://docs.sentry.io/api/events/list-an-organizations-issues/)
and [Sentry 26.6.0 API changes](https://github.com/getsentry/publish/issues/8571).

Choose a dedicated IRC SASL `account`. `channel` defaults to Starbase.
Set `target` to a captain or rear-admiral ID to tag and wake that officer
in their own channel. The target's channel becomes the default, and a
supplied channel must match it. Without a target, the post is informational.
Local Ergo seeds the account and channel. On remote IRC, register the
account (use the same nick), grant it access to the channel and ensure the
server supports IRCv3 `echo-message` for delivery confirmation.

In `<secrets-dir>/medical/`, put the Sentry bearer token in
`sentry-auth-token` and the SASL password in `irc-password`. Set both
files to mode `0600`. Create a Sentry internal integration with
**Issue & Event: Read** (`event:read`). Its token is organisation-wide.
The installer stores the service credential in a root-owned
mode-`0600` file and systemd supplies it to the dedicated service account.
Dispatch bundles and upgrades use the same private inputs, and the
`medical` member ID is reserved. For investigation, give the target
officer a separate read-only integration token at
`<secrets-dir>/<agent-id>/cloud/sentry-auth-token`, also mode `0600`.
The installer puts it in that officer's
`~/.config/subspace/cloud/sentry-auth-token`. Configure a read-only Sentry
client in the officer's harness to use this file. The officer reports
findings in IRC and routes repairs through normal review and delivery.

`subspace-fleet-medical.timer` checks unresolved error and fatal issues
seen in the last seven days. To restrict alerts to HTTP 5xx issues, set
`http5xxTag` to the name of a Sentry tag that your service sets to its own
three-digit HTTP response status, such as `http.server.status_code`.
Sentry applies the tag filter in issue search. Untagged errors are omitted
when the filter is enabled.
[Sentry search syntax](https://docs.sentry.io/concepts/search/).

Each poll scans up to three unresolved issue pages of 100 rows. With no
configured environment, it also checks the first production page and gives
those matches priority. Within those groups it ranks fatal before error,
then by affected users, event count and most recent occurrence. It records
up to three pending alerts before sending and retries unconfirmed posts.
Up to three issue alerts are sent per poll, each with severity, project
slug and a validated HTTPS issue permalink. IRC confirmation clears the
queued delivery and records the issue's latest occurrence in private SQLite
state, which survives restart and upgrade.

A later occurrence can become eligible again when Sentry marks it regressed
or after the poller sees it resolved. The poller reads one page of recent
resolutions for that check. When Sentry supplies a regression inbox entry,
its `date_added` distinguishes a new regression from a continuing one.
If a request fails, the poller stops scanning and keeps the alerts it
already found. For rate limits and server errors it respects `Retry-After`
up to 24 hours before contacting Sentry again. It retries a failure notice while the
outage continues and posts a recovery notice after a confirmed failure notice.

Use
`sudo systemctl start subspace-fleet-medical.service` to poll immediately
and `sudo journalctl --namespace=subspace-fleet -u subspace-fleet-medical.service`
to inspect the result.

## Crew quota

Set `usage.reportChannel` to a Slack channel that the rear admiral can post
to, and supply a distinct owner-only `<secrets-dir>/proxy-management-key`.
The installer enables authenticated, loopback-only CLIProxyAPI management
and Stargate's quota watcher. The optional `usage.families` keys select
Claude, Codex or both through empty objects such as `"claude": {}`. Omit
the selection to watch both. On upgrade, remove any previous
`limitAtPercent` and `pauseAtPercent` settings.

Stargate writes the current state to `quota.json` in the primary gateway's
data directory and posts to Slack as the rear admiral's bot. Slack intake
ignores bot messages. An admiral relays the notice to the rear admiral in
Slack or Starbase, and the rear admiral directs the captains.

- At 25% or less remaining, run at most one worker from that family per
  captain and hold extra work as in-flight work finishes.
- At 15% or less remaining, pause launches and resumes for that family,
  checkpoint active work at its next safe point and keep task claims.
- Above 25% after a warning, Stargate posts “Quota restored.” and the rear
  admiral tells captains to resume normal staffing.

[Stargate's quota guide](https://github.com/aiconsultancy/stargate/blob/df03d10b039c126a2fc0b5522bf7841ef07bc799/docs/usage.md).

## Configuration reference

| Field | Meaning and default |
| --- | --- |
| `slug`, `name` | Stable fleet ID and display label. Keep the slug fixed on a VM. Use up to 35 lowercase ASCII letters, digits and hyphens, starting with a letter or digit. |
| `rearAdmiral`, `captains[]` | Each has `id`, `name`, `account` and optional `channel`, `git` and `githubApp`. IDs start with a lowercase letter and use up to 20 lowercase letters, digits and hyphens. Keep IDs and SASL accounts unique. Choose short enough slug/IDs that each derived channel fits 50 characters, or specify a channel. An empty captain array is valid. |
| Member `git` | Optional `email` and SSH `signingKey` (absolute path to the public key). Provision an agent-owned private key or an SSH agent and register its public signing key with your Git host. Each fleet member has its own Git identity and workspace under `/var/lib/subspace-fleet/agents/<id>/workspace`. |
| `harness` | Default `{"type":"opencode","model":"gpt-5.4"}` with the [role's starter permissions](#instructions-permissions-and-toolchains). Explicit OpenCode V2 rules have `action`, `resource`, and `effect` (`allow`, `deny` or `ask`). |
| `irc` | `mode` is `remote` or `local`. Remote requires `host` and uses TLS/6697 unless a different port is set. Local listens on loopback/6667 for fleet members. Optional `caFile` is an absolute, local PEM path for remote TLS. `allowedAccounts` is the explicit human SASL allowlist. |
| `irc.starbaseChannel`, `irc.commandChannel`, member `channel` | Starbase defaults to the rear-admiral channel if set, then `irc.commandChannel` (default `#<slug>-command`). A separately named command channel is shared by officers. Captain channels default to `#<slug>-<id>`. |
| `irc.advertise` | Optional `host`, `port`, `tls` and `websocketUrl` that the API publishes. Defaults to the IRC endpoint used by the gateway. For a desktop on another machine, advertise an endpoint it can actually reach. |
| `irc.compose` | Local mode only. Defaults to the Subspace repository. Requires a full `ref` and an `operator` account. Optional `repo`, `serverName`, `tlsBind`, `tlsPort` (6697), `tlsCert`, `tlsKey`, `channels` (additional channel names to seed) and `history`: `{"database": "sqlite"}` (the default) or `{"database": "postgresql", "host": …, "port": 5432, "user": "ergo", "name": "ergo_history", "sslMode": "disable", "network": <Docker network that reaches it>}` with `ircHistoryPassword` in the secrets file. `subspace-setup` uses the API's PostgreSQL this way on a single-host fleet. |
| `stargate` | Optional `repo` and `ref`. Defaults to the [pinned upstream Stargate revision](https://github.com/aiconsultancy/stargate/commit/df03d10b039c126a2fc0b5522bf7841ef07bc799). `subspace-setup` (see [INSTALL.md](../INSTALL.md)) instead pins Stargate's newest `vX.Y.Z` release, looked up on every run. Changes must preserve the OpenCode V2 and IRC configuration contract. Optional `api` (`{"port": 8484}` by default) turns on Stargate's private Fleet API on loopback for the primary gateway; the default pin has it. Only root and `operatorUsers` can connect to it. `sudo subspace-fleetctl api-token` prints its bearer token. |
| `dashboard` | `bind` defaults to `127.0.0.1`. `port` defaults to 8070. The listener proxies only the Stargate read-only dashboard socket. |
| `proxy`, `agentsview` | Loopback-only by default. Proxy `port` defaults to 8317 and `requestRetries` to 2. AgentsView `port` defaults to 8080. |
| `operatorUsers` | Existing Linux usernames that may access the local operator interfaces and model proxy. Root always has access. Defaults to an empty array. |
| `toolchains` | Mise versions. Defaults to Node 24 plus the base OS tooling. |
| `maintenance` | `mirrorCalendar` (hourly), `toolsCalendar` (weekly), `pruneCalendar` (daily), `logDays` (14) and `worktreeDays` (30). |
| `medical` | Optional [medical ship](#medical-ship) Sentry polling and IRC alerts. |
| `usage` | Optional [crew quota](#crew-quota) notices, with a Slack `reportChannel` and provider selection. |
| `api` | Optional `url` (origin or ending in `/api`), `vmHost` and `initiatives` (array of UUIDs). HTTPS required except on loopback. The matching `apiToken` needs organisation administrator permissions. |

The mode-0600 secrets JSON has `proxyKey` (a random proxy client key),
`ircPasswords` (map by SASL account), `ircOperatorPassword` for local IRC,
`ircHistoryPassword` for PostgreSQL IRC history, and optional `apiToken`
(bearer JWT).
A named `fleet.json` looks for `fleet.secrets.json` beside it unless
`--secrets` is supplied. Installed copies live in `/etc/subspace-fleet/`. The
gateway receives IRC passwords and its OpenCode authentication headers, plus
any explicitly enabled integration credentials. Each OpenCode engine has an
independent loopback port and server password.

## Operate a fleet

```sh
sudo subspace-fleetctl attach orion
sudo journalctl --namespace=subspace-fleet -u subspace-fleet-gateway.service -f
ssh -N -L 8070:127.0.0.1:8070 -L 8080:127.0.0.1:8080 operator@fleet.example.com
```

`sudo subspace-fleetctl api-token` prints the Stargate Fleet API token for a
desktop or other operator client when `stargate.api` is set.

`attach` allocates a PTY, creates or reuses the selected member's tmux session
and connects its OpenCode TUI to its own server. Use the dashboard on
`http://127.0.0.1:8070` and AgentsView on `http://127.0.0.1:8080` through
the tunnel. These pages can show prompts, tool output and private code.
Add the SSH account to `operatorUsers` before opening a tunnel. A trusted
authenticating reverse proxy can also use an allowlisted local service
account. If it forwards to the dashboard, set its upstream Host header to
`127.0.0.1`.

The installer loads its own `inet subspace_fleet` nftables table before
starting fleet services. Its socket UID rules allow only root and the
listed operators to reach the dashboard and AgentsView. Active fleet agents
and Stargate can also reach CLIProxyAPI, which still requires its client key
and a distinct key for enabled management access. Incoming connections to
all three ports from other interfaces are rejected. Keep this table when
reloading a site-managed firewall. Apply your site's SSH and deliberate IRC
listener policy separately, including Docker-published ports.

The API integration looks up the fleet by slug and creates or patches its
registration. It applies the listed initiative assignments after startup.
An API failure leaves the fleet running and can be retried independently.

```sh
sudo subspace-fleetctl register
sudo subspace-fleetctl assign 00000000-0000-0000-0000-000000000000 example-fleet
sudo subspace-fleetctl assign 00000000-0000-0000-0000-000000000000 none
```

Removing an initiative from the config leaves its current assignment in place.
To change an assignment, also update any fleet configs that still claim it.

### Maintenance

The mirror service refreshes public refs and the rear admiral's reference
checkouts. It leaves active captain and crew branches in place. The tool
service updates Mise packages within their configured versions or channels.
The prune service vacuums archived fleet service journals and Git records of
missing worktrees. Existing worktree directories and journals outside the
fleet's namespace are retained. Set any calendar to `false` to disable its
timer, or use a systemd calendar expression. Inspect the schedule with
`systemctl list-timers 'subspace-fleet-*'` and service logs with
`journalctl --namespace=subspace-fleet`.

## Dispatch, upgrade and uninstall

Create an inventory like [inventory.example.json](inventory.example.json).
Paths are relative to the inventory file. Dispatch expects known SSH host
keys, key-based login, and a site-managed account authorised to run the
installer via non-interactive sudo.

```sh
./dispatch.sh inventory.json --parallel 4 --dry-run
./dispatch.sh inventory.json --parallel 4
sudo ./install.sh upgrade
sudo ./install.sh upgrade --config ~/fleet.json
sudo ./install.sh upgrade --config ~/fleet.json --secrets-dir ~/fleet.secrets --prune-secrets
sudo ./install.sh uninstall
```

Dispatch streams the installer and private config to a temporary remote
directory over SSH, cleans it up on exit and returns failure if any host
fails. Each run has a new private log directory, with one log per host.
Re-running with identical inputs leaves binaries, units and workspaces in
place. An upgrade reusing a pre-crew configuration keeps its original roster.
An external config without a companion secrets JSON can reuse the installed
JSON on upgrade. Pass an updated secrets JSON when changing the SASL roster.
To enable crew, provide the updated config and matching IRC passwords.
Update a source SHA or run a newer installer to upgrade the runtime
pins in [releases.tsv](lib/releases.tsv). Schedule upgrades during an operator
maintenance window since changes restart fleet services. Removed captains'
services stop and their workspaces and queues remain on disk.

Uninstall disables and removes the fleet units, stops local IRC, and removes
the installed programs. It preserves Linux accounts, Git workspaces, API
registrations, config, OAuth sessions, and the Docker volume. Back up
`/etc/subspace-fleet/` and `/var/lib/subspace-fleet/` and, for local IRC,
the Ergo volume. Keep backups private. The installer leaves OS packages in
place.

## Verification

From the repository root, run
`shellcheck subspace-installer/*.sh subspace-installer/tests/*.sh` and
`subspace-installer/tests/smoke.sh`. The Ubuntu container installs pinned
binaries, checks rendered units, exercises a second no-op install, and runs
the application processes against local fixtures without model calls. It
also checks crew worktrees, credential ownership and mode, and local Ergo
channel seed generation. It verifies role provisioning, local service access
rules, normal credential retention and explicit pruning, the Node toolchain,
read-only reference checkouts and fresh worktree bases. It exercises nftables
when permitted by the container and checks rule generation otherwise. Run
`python3 -B subspace-installer/tests/policy_test.py` for the offline policy
checks.

## Licence

The Subspace fleet installer is licensed under the
[Apache License 2.0](../LICENSE).
