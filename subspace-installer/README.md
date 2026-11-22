# Subspace fleet installer

A fleet is one Linux VM. Human **admirals** use **Starbase** to work out what
to build with the **rear admiral**, who coordinates the **captains** and talks
to stakeholders. Each captain has a ship channel and a **crew**. The default
crew is two Claude-family and two Codex-family workers.

The rear admiral and the captains each have their own Linux user, IRC
account and GitHub App. Each captain gets a checkout of the configured
repositories. Crew run under their captain's Linux user, inside the captain's
OpenCode server, through Stargate's crew plugin. Each crew member has one Git
worktree per repository, commits as its captain and is reached only through
its captain: crew have no IRC accounts.

Stargate routes IRC conversations to the rear admiral and captains. The
installer defaults to OpenCode V2, using CLIProxyAPI for models. Configure
the model and harness per role and the model per crew family. The dashboard
shows the primary Stargate gateway, and AgentsView indexes OpenCode session
databases, including crew sessions, for local browsing and search. The
installer can run a [medical ship](#medical-ship) to flag Sentry errors and
enable [quota notices](#crew-quota) so the rear admiral can wind down crew as
a model family's budget runs low.

A fleet can use an existing IRC network or host Ergo using the pinned
[Subspace IRC Compose project](../subspace-irc/README.md). Ergo on the fleet
VM suits a trial or a single fleet; with several fleets, run one shared
server for all of them, as
[One fleet or several](../INSTALL.md#one-fleet-or-several-shared-irc)
recommends. The desktop's Channels Space and each fleet connect to IRC
independently. The optional Subspace API stores
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

For remote IRC, register a SASL account for the rear admiral and each
captain, such as `example-atlas` and `example-orion`, and supply their
passwords in `ircPasswords`. Crew have no IRC accounts. Add trusted human
SASL accounts to `irc.allowedAccounts`. Ask the IRC operator to register the
fleet's [standard channels](#channels) (`--dry-run` lists them) and any
initiative channels, and grant access to the appropriate accounts. For local IRC,
[pin a revision that contains the Compose project](#local-irc-and-tls).

The installer validates JSON, installs base tooling and verified release
binaries, checks Stargate's configuration, and starts systemd services. It
then [pins each crew-hosting captain's root session](#captain-sessions) and
restarts the services that use it.
`--dry-run` requires Python 3 and validates the inputs without writing to the
host. `--no-systemd` prepares units and validates configuration and Compose
without activating systemd services or writing to the Subspace API.

### Model provider sign-in

Select exact CLIProxyAPI model IDs. OpenCode routes the `codex` family to
the OpenAI-compatible Responses endpoint, as provider `openai-proxy`, and the
`claude` family to the Anthropic-compatible Messages endpoint, as provider
`claude-proxy`. A captain's OpenCode server defines the provider for its own
family and for each crew family it hosts. Verify the model IDs on your proxy.
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

List repositories in order as `{"id":"project","url":"https://github.com/example/project.git"}`
objects under `repositories`. Every captain gets a checkout of each one under
`/var/lib/subspace-fleet/agents/<captain-id>/workspace/<repository-id>`, and
the first is each agent's default working directory. An empty list gives each
agent an empty workspace and gives captains no crew. Repository IDs start
with a lowercase letter, `default` is reserved, and a captain with crew takes
at most nine repositories. The rear admiral gets read-only reference
checkouts under `/var/lib/subspace-fleet/references/<rear-admiral-id>/` that
the mirror service refreshes. The rear admiral can inspect them from its own
Linux account, including Git history. `subspace-setup --repositories` fills
this list from comma-separated HTTPS URLs, optionally written as `ID=URL`.

Set fleet-wide `crew.claude` and `crew.codex` defaults, or override either
under a captain's `crew`. Each family takes a `count` from 0 to 10 and a
`model`. Crew run inside their captain's OpenCode server, as the captain's
Linux user, through Stargate's crew plugin:

- Each crew member has one permanent worktree per repository, under
  `/var/lib/subspace-fleet/agents/<captain-id>/worktrees/<crew-id>/<repository-id>`.
  Each worktree belongs to the captain's checkout of that repository and
  starts on the branch `crew/<fleet-slug>-<captain-id>-<crew-id>`.
- Crew display names default to `Claude 1`, `Codex 1` and so on, which give
  the crew IDs `claude-1` and `codex-1`. To rename one, set the captain's
  `crewNames` map by slot, for example `"crewNames": {"codex-1": "Rigel"}`,
  which gives the crew ID `rigel`. Names use ASCII letters, digits, spaces,
  hyphens and underscores.
- Claude-family crew use `crew.claude.model` through `claude-proxy`, and
  Codex-family crew use `crew.codex.model` through `openai-proxy`, both
  served by CLIProxyAPI. The default is two of each.
- Crew have no Linux user, IRC account, GitHub App, cloud credentials or
  OpenCode engine of their own. Their captain delegates to them and receives
  their hand-backs through Stargate's `stargate_crew` tool, with
  `stargate_mail` and `stargate_dm` for messages.
- A captain's crew roster and first repository are fixed once its
  [session is pinned](#captain-sessions), because Stargate keeps crew
  identities and worktrees permanent. The installer refuses a later change,
  including crew counts of 0 or an empty repository list. Restore the
  recorded roster, or give a new roster to a new captain. Adding
  repositories after the first is allowed. A changed crew model applies to
  crew sessions created afterwards.

Public checkouts and crew worktrees share a read-only Git object cache.
The installer refreshes its refs before creating a worktree and preserves
existing branches and work in progress. Backing objects are retained because
active or retired worktrees may still refer to them.

For a private GitHub repository, set `"private": true` and use an HTTPS
GitHub URL. Give the rear admiral and every captain a distinct GitHub App
installed on that repository with at least Contents read access. Crew use
their captain's. The installer mints a repository-scoped read token for
bootstrap and keeps private objects in the captain's own checkouts, which its
crew's worktrees share. Use a new repository ID when its URL or visibility
changes.

The `opencode` harness uses `permissions`. Select `claude-sdk` with
`family: "claude"` in a rear-admiral or captain harness to use the Claude
Agent SDK. Its `allowedTools` defaults to an empty list. Each SDK agent runs
its own Stargate process as its Linux user. Stargate's crew plugin runs
inside an OpenCode server, so a `claude-sdk` captain cannot host crew: set
its crew counts to 0. Crew take no harness `type`, so Claude Agent SDK crew
are refused. The primary dashboard shows registrations in the rear admiral's
gateway. The pinned revision has a Codex SDK adapter. The OpenAI Agents SDK
(`openai-sdk`) is currently unavailable.

### Captain sessions

Stargate's crew plugin gives captain authority to one root OpenCode session.
For each captain with crew, the installer renders the plugin's configuration
into the captain's OpenCode profile: the team, with one native agent per crew
member, the worktrees for each repository, a private state directory under
the captain's home and the pinned Mesij command, `/opt/subspace-fleet/bin/mesij`.
After the services start, it creates the captain's root session through the
engine's authenticated loopback API, with the `build` agent, the captain's
checkout of the first repository and the captain's permissions. Creating a
session starts no model turn. The session ID goes into both the gateway's
`backend.sessionID` and the plugin's `captainSessionID`, and the installer
restarts the services to load the plugin.

The record lives in `/etc/subspace-fleet/sessions/<captain-id>.json`, owned
by root with mode `0600`. The installer first reserves the ID in
`<captain-id>.pending.json` beside it, before it creates the session, so an
interrupted run retries the same ID instead of creating a second root. Later
runs check the session's identity, agent, directory and permissions, and
never replace a missing root. When you change the captain's policy, the
installer updates the session's permissions on the running engine before it
restarts the gateway with the new policy.

The rear admiral and the OpenCode captains share one gateway. It serves a
captain with crew only once a render has put that captain's pin into both
its engine and gateway configuration. The installer then writes
`<captain-id>.ready` beside the record, and the gateway unit starts only
while the marker of every captain it serves exists. Until then the gateway
serves the rest of the fleet. If commissioning fails for a captain, the
installer names the captain, still restarts the services for the captains it
did pin, and exits with an error. The gateway keeps running without the
captain that failed.

OpenCode's session update cannot change a session's model, so a pinned root
keeps the model it was created with. If you change a captain's
`harness.model` or `harness.family`, each installer run reports that the
root keeps its original model. To run the captain on the new model, give its
crew to a new captain, or restore the original model.

If commissioning fails before a session is confirmed, fix the cause, such as
the engine or the model proxy, and run the installer again. It retries the
reserved ID. Until the session exists you can still change the first
repository. If a confirmed root is missing from the engine, the installer
stops for that captain. Restore
`/var/lib/subspace-fleet/sessions/<captain-id>/opencode.db` from a backup, or
retire the captain and give its crew to a new captain ID.

### Instructions, permissions and toolchains

The installer renders role-specific OpenCode `AGENTS.md` and
`skills/fleet-*/SKILL.md` under the rear admiral's and each captain's
`~/.config/opencode/` using the fleet name, agent name and assigned channel.
A captain with crew also gets the `fleet-crew` skill, and each crew member's
own instructions become its native agent's system prompt in the captain's
OpenCode profile. Captains assign work and receive hand-backs through
`stargate_crew` and its mail and DM tools. Crew work in their worktrees and
report to their captain, never in a ship channel. Edit the templates in
`lib/roles/` and re-run the installer to customise them. Claude SDK agents
receive the same role guidance and skill procedure in their system prompt.

The default OpenCode policy grants read, glob, grep, skills and Git inspection.
Captains can edit their checkout, stage and commit locally, and run common
build and test commands. For a captain without crew, external paths,
publication and other shell commands require an explicit grant. A captain
with crew can reach them through its crew (see below). Build and test
commands can execute code from the project, so use trusted repositories.
Supply an ordered `harness.permissions` array to replace the starter policy across the fleet
or for the rear admiral or a single captain. Rules use `action`, `resource`
and `effect`. Put narrower rules after broader ones. See
[OpenCode permissions](https://opencode.ai/docs/permissions/). A captain with
crew also gets `stargate_crew`, `stargate_mail` and `stargate_history`, and
`stargate_model` for `get`, on its pinned session.

Crew take no `permissions`. Stargate creates each crew session with its
crew `coding` profile and curated tool access: reading, searching, skills,
web fetch and search, edits, shell, browser and code execution, sub-agents
and the Stargate crew tools, with reads of `*.env`, `*.env.*`, `*.key` and
`*.pem` files denied. That profile also allows paths outside the worktree.
It is broader than the captain's starter policy, and tool rules are not an
operating-system boundary in either case.

**A captain with crew effectively has the crew profile's tools.** It directs
crew that run shell commands, edit files and use paths outside their
worktrees, so its own `harness.permissions` do not limit what it can get
done. Crew can also read the captain's engine credentials, such as its
OpenCode server password and the model proxy key. The
[protected-branch check](#github-apps-per-fleet-member) below is a
client-side Git `pre-push` hook, which a crew shell can skip with
`git push --no-verify`. Protect the branches that matter on the server, with
GitHub rulesets or branch protection on every configured repository.

**Crew share their captain's Linux user.** A crew member can read and change
whatever its captain can: the captain's checkouts and every crew worktree,
its GitHub App key and token helper, its cloud credentials and its OpenCode
session database. Treat a captain and its crew as one trust boundary, give
them the same repositories and credentials only, and keep secrets you would
not give the captain out of its home. The separation that matters for
security is unchanged: each captain and the rear admiral still has its own
Linux user, home (mode `0700`), credential files (mode `0600`), OpenCode
server password and firewall identity, so a captain and its crew cannot read
the rear admiral's or another captain's files, and the agents still cannot
reach operator services or the cloud metadata service.

The base tools include Git, Bun, a C/C++ build toolchain, and system Python with
virtual environments. Mise installs Node 24 by default. Use `toolchains` to
select versions of Node, Rust, Go, Python, kubectl, gh, Zig or Deno. For
example, `"toolchains": {"node":"24","go":"1.26","rust":"stable","kubectl":"1.35"}`
adds Go, Rust and kubectl. Use a full version to pin a tool or `false` to
disable it. Managed tools are placed on the PATH for fleet processes and
`fleetctl attach`. Repository-specific versions can still use the agent's own
tool setup.

### Channels

**Every fleet has the same standard channels**, named from its slug, and the
desktop recognises them by name:

| Channel | Name | Who is there | What it is for |
| --- | --- | --- | --- |
| Starbase | `#<slug>-starbase` | Admirals and the rear admiral | People talk to the rear admiral and to each other. |
| Fleet command | `#<slug>-command` | The rear admiral and every captain | The rear admiral and the captains coordinate. Agent dispatch offers and assigns issues here and captains answer here. |
| Ship | `#<slug>-<captain-id>`, one per captain | The captain, the rear admiral and the other captains | The captain's own channel, where people talk to it. |

The installer always creates them: it seeds them on local Ergo, puts them in
the bundle for a [shared server](../INSTALL.md#one-fleet-or-several-shared-irc),
and prints them in `--dry-run` so an operator can register them on a server
it does not manage. Their names are fixed. `irc.starbaseChannel`,
`irc.commandChannel` and a member's `channel` may be left out or set to the
standard name; any other value is refused, as is a slug or captain ID that
makes a name longer than 50 bytes (IRC and Stargate's limit) or a captain ID
of `starbase` or `command`. Extra channels are welcome: seed initiative
channels with `irc.compose.channels` on local IRC, register them on a shared
server, and list them on the initiative. The names are defined once, in
[`lib/standard_channels.py`](lib/standard_channels.py).

The rear admiral joins every standard channel. Each captain joins its own
ship channel, fleet command and the other ship channels. Allowlisted humans
wake the rear admiral in Starbase and the owning captain in its ship channel.
Elsewhere, fleet command included, tag an agent by its current IRC nick to
wake it; the rear admiral tags a captain the same way. Direct messages are
for people only: the installer sets Stargate's `dmPolicy` to `redirect`, so a
direct message to the rear admiral or a captain starts no turn, and the agent
answers a signed-in sender with a notice pointing at its own channel
(Starbase for the rear admiral, the ship channel for a captain). Stargate verifies the
sender's SASL account and keeps human authority separate from agent
requests. Crew are not on IRC: ask the captain, who relays to its crew
through Stargate's crew tools. Stargate uses IRC message IDs for strict wake
rules. [Agent dispatch](#agent-dispatch) posts its offers and assignment
notices to fleet command, each addressed to one captain's nick, so it wakes
only that captain.

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

Ergo registers the fleet's [standard channels](#channels) when the
container starts. The installer generates `IRC_CHANNELS_FILE`.
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
`irc.caFile`. For several fleets, we recommend a shared server of its own
rather than one fleet's Ergo:
[Run the shared IRC server in a container](../INSTALL.md#run-the-shared-irc-server-in-a-container).

## Agent dispatch

Stargate's dispatcher runs in the rear admiral's gateway and hands out
Subspace issues: it offers each unassigned issue flagged *Ready for agents*
to the captains in fleet command, one at a time, and tells a captain in
fleet command about an issue assigned to it. See the
[repository README](../README.md#agent-dispatch) for what people see and
[Stargate's guide](https://github.com/aiconsultancy/stargate/blob/fcf6ac75d9c780878c06c8b49fb160e4a27832ff/docs/agent-dispatch.md)
for the details. It is off by default. `subspace-setup` asks whether to
switch it on (`--agent-dispatch`); in `fleet.json` it is:

```json
{
  "agentDispatch": {
    "enabled": true,
    "pollIntervalSeconds": 120,
    "answerTimeoutSeconds": 300,
    "retryLadderSeconds": [3600, 10800, 21600, 43200]
  }
}
```

The timings are Stargate's defaults and may be left out: the poll interval
(30 to 3,600 seconds), the time each captain has to answer an offer (60 to
3,600 seconds) and the waits between rounds (at most ten, each 60 to
604,800 seconds; `[]` means one round).

With it on, the installer writes Stargate's `agentDispatch` block into the
primary gateway's `/etc/subspace-fleet/stargate.json`: the fleet's slug, the
API at `api.url`, the rear admiral's IRC registration, the fleet's command
channel and the timings. Captains are found in the API's fleet roster by
agent name; an agent named after the captain's member ID or name is mapped to
its IRC account, and any other name must be the IRC account itself.

**Credential.** The dispatcher calls the API as the rear admiral, so
assignments, comments and flag changes are recorded as the rear admiral's.
Give it a bearer token for the rear admiral's own API agent identity, not the
administrator's `apiToken`: register the agent (rank `rearAdmiral`, linked to
this fleet, by an organisation administrator with `POST /api/v1/agents`) and
put a token for its account subject in the secrets file as
`rearAdmiralApiToken`. Register each captain as an agent of rank `captain` in
the fleet. The token needs no other scope: it is an ordinary member's token
for that agent identity, and the API limits a fleet agent's assignments and
*Ready for agents* changes to its own fleet. The installer passes it to the
gateway only through its private runtime settings, as
`SUBSPACE_DISPATCH_TOKEN`, and never prints it. Stargate reads it when the
gateway starts and has no refresh of its own, so use a long-lived token, or
renew it and re-run the installer. `subspace-setup`, on an API with
development sign-in, registers the agents (named after their IRC accounts,
signing in as `<account>@agents.example.com`) and mints the rear admiral's
token on each run; a development token lasts eight hours. The gateway's
Linux user must be able to reach the API: `subspace-setup` lets it, and no
agent, through to the API's port.

**Answers.** A captain accepts or declines an offer only with Stargate's
`fleet_answer_offer` tool, which needs a pinned OpenCode root. So with
dispatch on, the installer also:

- pins the rear admiral's root session, as it does for a captain with crew
  ([Captain sessions](#captain-sessions)), and binds only its IRC
  registration to it; Slack and Teams keep their own sessions. Captains name
  that root as their rear admiral, which is how they recognise its offers;
- gives each captain with crew Stargate's `fleetControl` with the rear
  admiral as its `rear_admiral` target, `fleetChannels` so that offers count
  only in fleet command, the fleet plugin in its OpenCode server and
  `fleet_answer_offer` in its policy. The plugin reaches the gateway through
  `/var/lib/subspace-fleet/fleet-control/<captain-id>/fleet.sock`, in a
  directory the gateway owns and only that captain's engine group can enter;
  the gateway keeps its evidence ledger under its own home.

Captains without crew, and `claude-sdk` captains, cannot answer: their offers
time out and go to the next captain, but they are still told about issues
assigned to them. Requests are addressed to one captain's current nick and
answers and status lines are notices, so the usual
[wake rules](#channels) hold: nothing else wakes a captain in fleet command,
and the rear admiral's requests are exempt from the captain's peer turn
limit, though they count towards it.

The installer refuses to switch dispatch on without an `api` registration,
without captains, without a captain with crew, on a `claude-sdk` rear
admiral (its gateway would run as the rear admiral's own user, so its model
could read the token, and it cannot be pinned), without
`rearAdmiralApiToken` or with the administrator's token in its place, and
when a captain's `harness.permissions` has rules that Stargate's fleet tools
reject: an `ask`, or a pattern other than an exact name, `*` or a prefix
followed by `*`, for a rule that could match a fleet tool.

## GitHub Apps per fleet member

Create a GitHub App and installation for the rear admiral and for each
captain that uses GitHub. Crew have no App of their own: they commit and
push from their worktrees as their captain, with the captain's Git identity,
App and token helper. Grant each App access to the configured repositories
and the GitHub permissions its member and, for a captain, its crew need.
Git pushes need Contents write. Issue and pull request operations use Issues
and Pull requests permissions as appropriate. See
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

Each enabled rear-admiral or captain Linux user gets
`~/.config/subspace/github-app.json` and `github-app.pem` with mode `0600`. The installer keeps root-only copies
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
uses read access. Captain Apps default to write access for pushing feature
branches, which their crew share. Set an App's `contents` to `"read"` for a
read-only installation. The installer provides a Git pre-push hook in managed
checkouts for App users, which the crew worktrees share, and preserves
existing hooks.
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
`~/.config/subspace/cloud/` for the rear admiral and each captain. A
captain's crew use, and can read, the captain's files. Put `kubeconfig` and
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
be configured independently. Captains coordinate over IRC, and crew only
through their captain.
Step-by-step guides: [Slack](../docs/slack.md) and [Teams](../docs/teams.md).

- Set `stakeholders.slack` to an `appId` and explicit `users` and
  `channels` arrays of Slack IDs. Supply Socket Mode app and bot tokens
  as `<secrets-dir>/<rear-admiral-id>/slack-app-token` and
  `slack-bot-token`, both mode `0600`. See
  [Stargate's Slack scope list](https://github.com/aiconsultancy/stargate/blob/fcf6ac75d9c780878c06c8b49fb160e4a27832ff/docs/slack-scopes.md).
- Set `stakeholders.teams` to an Azure Bot `botClientId` and `tenantId`
  plus an explicit `users` array of Entra object IDs and optional
  `channels`. The supported Bot Framework backend uses an Azure
  user-assigned managed identity at runtime. Route the bot's HTTPS
  `/api/messages` callback to `/var/lib/subspace-fleet/teams/bot.sock`.
  Set `socketGroup` to an existing Linux group for the trusted reverse
  proxy. Follow [Stargate's Teams guide](https://github.com/aiconsultancy/stargate/blob/fcf6ac75d9c780878c06c8b49fb160e4a27832ff/docs/teams.md).

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
Slack or Starbase, and the rear admiral directs the captains, who control
their crew's assignments through `stargate_crew`.

- At 25% or less remaining, each captain keeps at most one crew member of
  that family working and holds further assignments as in-flight work
  finishes.
- At 15% or less remaining, captains pause new assignments and resumed work
  for that family, have active crew checkpoint at their next safe point and
  keep task claims.
- Above 25% after a warning, Stargate posts “Quota restored.” and the rear
  admiral tells captains to resume normal staffing.

[Stargate's quota guide](https://github.com/aiconsultancy/stargate/blob/fcf6ac75d9c780878c06c8b49fb160e4a27832ff/docs/usage.md).

## Configuration reference

| Field | Meaning and default |
| --- | --- |
| `slug`, `name` | Stable fleet ID and display label. Keep the slug fixed on a VM. Use up to 35 lowercase ASCII letters, digits and hyphens, starting with a letter or digit. |
| `rearAdmiral`, `captains[]` | Each has `id`, `name`, `account` and optional `channel`, `git`, `githubApp` and `harness`. Captains also take optional `crew` and `crewNames`. IDs start with a lowercase letter and use up to 20 lowercase letters, digits and hyphens; `all` and `captain` are reserved. Keep IDs and SASL accounts unique; accounts have at most 32 characters. Choose a slug and captain IDs short enough that each [standard channel](#channels) fits 50 characters. A member's optional `channel` must be its standard channel. An empty captain array is valid; the most is 18, because the rear admiral joins every standard channel and Stargate joins at most 20. |
| Member `git` | Optional `email` and SSH `signingKey` (absolute path to the public key). Provision an agent-owned private key or an SSH agent and register its public signing key with your Git host. The rear admiral and each captain have their own Git identity and workspace under `/var/lib/subspace-fleet/agents/<id>/workspace`. Crew commit with their captain's. |
| `harness` | Default `{"type":"opencode","model":"gpt-5.4"}` with the [role's starter permissions](#instructions-permissions-and-toolchains). Explicit OpenCode V2 rules have `action`, `resource`, and `effect` (`allow`, `deny` or `ask`). Applies to the rear admiral and captains. |
| `crew` | Fleet-wide `claude` and `codex` objects with `count` (0 to 10, default 2) and `model` (defaults `claude-sonnet-5-5` and `gpt-5.4`); a captain's `crew` overrides them. Crew need at least one repository and an OpenCode captain. See [crew and repositories](#crew-and-repositories). |
| `repositories` | Ordered `{"id", "url", "private"}` objects that every captain checks out. See [crew and repositories](#crew-and-repositories). |
| `irc` | `mode` is `remote` or `local`. Remote requires `host` and uses TLS/6697 unless a different port is set. Local listens on loopback/6667 for fleet members. Optional `caFile` is an absolute, local PEM path for remote TLS. `allowedAccounts` is the explicit human SASL allowlist. |
| `irc.starbaseChannel`, `irc.commandChannel`, member `channel` | Optional, and only ever the [standard names](#channels): `#<slug>-starbase`, `#<slug>-command`, and Starbase for the rear admiral and `#<slug>-<id>` for a captain. Any other name is refused. |
| `irc.advertise` | Optional `host`, `port`, `tls` and `websocketUrl` that the API publishes. Defaults to the IRC endpoint used by the gateway. For a desktop on another machine, advertise an endpoint it can actually reach. |
| `irc.compose` | Local mode only. Defaults to the Subspace repository. Requires a full `ref` and an `operator` account. Optional `repo`, `serverName`, `tlsBind`, `tlsPort` (6697), `tlsCert`, `tlsKey`, `channels` (additional channel names to seed) and `history`: `{"database": "sqlite"}` (the default) or `{"database": "postgresql", "host": …, "port": 5432, "user": "ergo", "name": "ergo_history", "sslMode": "disable", "network": <Docker network that reaches it>}` with `ircHistoryPassword` in the secrets file. `subspace-setup` uses the API's PostgreSQL this way on a single-host fleet. |
| `stargate` | Optional `repo` and `ref`. Defaults to the [pinned upstream Stargate release, v0.43.0](https://github.com/aiconsultancy/stargate/commit/fcf6ac75d9c780878c06c8b49fb160e4a27832ff), the first with [agent dispatch](#agent-dispatch). `subspace-setup` (see [INSTALL.md](../INSTALL.md)) instead pins Stargate's newest `vX.Y.Z` release, looked up on every run. Changes must preserve the OpenCode V2 and IRC configuration contract. Optional `api` (`{"port": 8484}` by default) turns on Stargate's private Fleet API on loopback for the primary gateway; the default pin has it. Only root and `operatorUsers` can connect to it. `sudo subspace-fleetctl api-token` prints its bearer token. Its `corsOrigins` default to `subspace://*` and `https://subspace.fleet-console`, the built-in Fleet Console's origin on Windows; list your own instead if you installed the Fleet Console under another Board ID. |
| `dashboard` | `bind` defaults to `127.0.0.1`. `port` defaults to 8070. The listener proxies only the Stargate read-only dashboard socket. |
| `proxy`, `agentsview` | Loopback-only by default. Proxy `port` defaults to 8317 and `requestRetries` to 2. AgentsView `port` defaults to 8080. |
| `operatorUsers` | Existing Linux usernames that may access the local operator interfaces and model proxy. Root always has access. Defaults to an empty array. |
| `toolchains` | Mise versions. Defaults to Node 24 plus the base OS tooling. |
| `maintenance` | `mirrorCalendar` (hourly), `toolsCalendar` (weekly), `pruneCalendar` (daily), `logDays` (14) and `worktreeDays` (30). |
| `medical` | Optional [medical ship](#medical-ship) Sentry polling and IRC alerts. |
| `usage` | Optional [crew quota](#crew-quota) notices, with a Slack `reportChannel` and provider selection. |
| `api` | Optional `url` (origin or ending in `/api`), `vmHost` and `initiatives` (array of UUIDs). HTTPS required except on loopback. The matching `apiToken` needs organisation administrator permissions. |
| `agentDispatch` | Optional [agent dispatch](#agent-dispatch): `enabled` (default `false`), `pollIntervalSeconds` (120), `answerTimeoutSeconds` (300) and `retryLadderSeconds` (`[3600, 10800, 21600, 43200]`). On, it needs `api`, a captain with crew, an OpenCode rear admiral and `rearAdmiralApiToken`. |

The mode-0600 secrets JSON has `proxyKey` (a random proxy client key),
`ircPasswords` (map by SASL account: the rear admiral, each captain and, in
local mode, any allowlisted human; crew have none), `ircOperatorPassword`
for local IRC, `ircHistoryPassword` for PostgreSQL IRC history, and optional
`apiToken` (bearer JWT) and `rearAdmiralApiToken` (the rear admiral agent's
bearer JWT, only with [agent dispatch](#agent-dispatch) on).
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

`attach` allocates a PTY, creates or reuses the selected rear admiral's or
captain's tmux session and connects its OpenCode TUI to its own server.
Crew sessions live on their captain's server: attach to the captain and open
them from the session list. Do not type into a session while Stargate or a
crew task is using it. Use the dashboard on
`http://127.0.0.1:8070` and AgentsView on `http://127.0.0.1:8080` through
the tunnel. These pages can show prompts, tool output and private code.
Add the SSH account to `operatorUsers` before opening a tunnel. A trusted
authenticating reverse proxy can also use an allowlisted local service
account. If it forwards to the dashboard, set its upstream Host header to
`127.0.0.1`.

The installer loads its own `inet subspace_fleet` nftables table before
starting fleet services. Its socket UID rules allow only root and the
listed operators to reach the dashboard and AgentsView. The rear admiral's
and captains' Linux users, which their crew share, and Stargate can also
reach CLIProxyAPI, which still requires its client key
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
Re-running with identical inputs leaves binaries, units, workspaces and
pinned captain sessions in place. An external config without a companion
secrets JSON can reuse the installed JSON on upgrade. Pass an updated secrets
JSON when adding or removing the rear admiral or a captain. Crew need no IRC
passwords; their roster is fixed once the captain's session is pinned (see
[crew and repositories](#crew-and-repositories)). Update a source SHA or run
a newer installer to upgrade the runtime pins in
[releases.tsv](lib/releases.tsv), which include Mesij for Stargate's crew
mail. Schedule upgrades during an operator
maintenance window since changes restart fleet services. Removed captains'
services stop and their workspaces and queues remain on disk.

Uninstall disables and removes the fleet units, stops local IRC, and removes
the installed programs. It preserves Linux accounts, Git workspaces and crew
worktrees, crew state, pinned session records, API registrations, config,
OAuth sessions, and the Docker volume. Back up
`/etc/subspace-fleet/` and `/var/lib/subspace-fleet/` and, for local IRC,
the Ergo volume. Keep backups private. The installer leaves OS packages in
place.

## Verification

From the repository root, run
`shellcheck subspace-installer/*.sh subspace-installer/tests/*.sh` and
`subspace-installer/tests/smoke.sh`. The Ubuntu container installs pinned
binaries, checks rendered units, exercises a second no-op install, and runs
the application processes against local fixtures without model calls. It
pins the captain's root session on its running engine and checks both
Stargate pins, that crew have no Linux user, engine or IRC registration, and
one worktree per repository per crew member under the captain. It also checks
credential ownership and mode, and local Ergo channel seed generation. It
verifies role provisioning, local service access rules, normal credential
retention and explicit pruning, the Node toolchain, read-only reference
checkouts and fresh worktree bases. It exercises nftables when permitted by
the container and checks rule generation otherwise. For the offline checks,
run `python3 -B subspace-installer/tests/policy_test.py`,
`python3 -B subspace-installer/tests/sessions_test.py`,
`python3 -B subspace-installer/tests/setup_test.py` and
`python3 -B subspace-installer/tests/medical_test.py`.

## Licence

The Subspace fleet installer is licensed under the
[Apache License 2.0](../LICENSE).
