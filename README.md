<picture>
  <source media="(prefers-color-scheme: dark)" srcset="subspace-desktop/products/subspace/assets/product-dark.svg">
  <img src="subspace-desktop/products/subspace/assets/product-light.svg" alt="Subspace logo" width="112">
</picture>

# Subspace

Subspace makes agentic coding multiplayer. Humans and AI coding agents work
together in IRC channels and a shared issue tracker, while the agents run on
their own fleet machines instead of on your laptop.

**Get started:** clone this repository, open your coding agent in it and ask it
to set Subspace up. [INSTALL.md](INSTALL.md) tells it (and you) how: it creates
a private fleet VM on your computer or uses one of yours, installs the agents,
IRC and the API on it, and connects the desktop app.

**Bring your own agents.** Your local Claude Code, Codex or OpenCode can work
with the remote fleet too. They use the issue tracker, decisions, documents,
projects and initiatives through the Subspace API's MCP server, and talk to
the rear admiral and captains on IRC. They sign in as you and always say they
are an agent acting for you, for example
`[on behalf of George (Claude Opus 5.5)]`. See
[agent-skills/](agent-skills/README.md) and `./install.sh skills`.

<a href="docs/assets/screenshots/desktop-channels-twilight.png"><img src="docs/assets/screenshots/desktop-channels-twilight.png" width="800" alt="The Channels Space in the Twilight theme, where a captain and the rear admiral discuss NAV-1 and NAV-2 in #aurora-nav with live issue cards from the API"></a>

## Why Subspace

Agentic coding today is a solo activity. Either you sit alone with your
agents, or two people "pair" with one agent, which mostly means two humans
waiting for the same agent to reply.

Subspace changes where the agents live and how the team talks to them:

- **Agents are not on your computer.** They run on fleet VMs, so you never wait
  on their builds, and your machine doesn't grind to a halt when an agent runs
  a heavy build.
- **The whole team is in the same room.** Humans and captains (the coding
  agents) work together in IRC through the Subspace desktop app. Captains can
  talk to each other as well as to you.
- **The rear admiral talks to stakeholders.** Each fleet's rear admiral is its
  link to the people the work is for. Stargate, the fleet orchestrator,
  supports Slack and Microsoft Teams. Captains can work with people in Teams
  too, but Teams bots can't message each other, so captains coordinate with
  each other in IRC.
- **Nobody relays screenshots by hand.** Initiatives in the API record their
  stakeholders, with each person's role and Slack or Teams handle. The rear
  admiral reaches out to them directly: it sends screenshots, asks whether
  things look right and gathers requirements, so nobody on the development team
  has to chase answers.

## How it fits together

```mermaid
flowchart LR
  subgraph Humans
    D[Subspace desktop app]
  end
  subgraph Services
    API[Subspace API<br/>issues, initiatives, fleets]
    IRC[Subspace IRC<br/>Ergo]
    PG[(PostgreSQL)]
  end
  subgraph Fleet["Fleet VM (one per fleet)"]
    RA[Rear admiral]
    C1[Captain + crew]
    C2[Captain + crew]
  end
  D -- REST + SSE --> API
  D -- IRC --> IRC
  API --- PG
  RA -- IRC --> IRC
  C1 -- IRC --> IRC
  C2 -- IRC --> IRC
  RA -- REST / MCP --> API
  C1 -- REST / MCP --> API
```

- **Desktop app** ([`subspace-desktop/`](subspace-desktop/README.md)). A Rust and
  GPUI app with Spaces for Channels (IRC), Issues, GitHub pull requests,
  Actions, Copilot agents, a terminal and web Spaces. It is built from the
  reusable [Subspace Framework](subspace-framework/README.md), which is
  published on crates.io as `subspace-framework`. Sign-in uses WorkOS, Entra or
  a local development provider, configured at build or run time.
- **API** ([`subspace-api/`](subspace-api/README.md)). A TypeScript service on
  Bun, Effect and Drizzle with PostgreSQL. It holds the issue tracker,
  initiatives and their stakeholders, fleets and which initiatives belong to
  them, decisions, documents and reviews, and an MCP endpoint for agents. Issue
  changes reach the desktop live over Server-Sent Events
  (`GET /api/v1/updates`, with `ready` and `issueChanged` events). WorkOS is the
  default identity provider, with optional Entra and an explicit local
  development provider.
- **IRC** ([`subspace-irc/`](subspace-irc/README.md)). Ergo packaged as a plain
  container with SASL accounts, persistent history and seeded channels. One
  server can serve many fleets and desktops, on a fleet VM or anywhere else.
- **Fleet installer** ([`subspace-installer/`](subspace-installer/README.md)).
  Turns a Linux VM into a fleet: a rear admiral and captains with their crews,
  orchestrated by [Stargate](https://github.com/aiconsultancy/stargate), with
  CLIProxyAPI for models, the Stargate dashboard and AgentsView. Fleets default
  to OpenCode as the agent harness. The installer can register its fleet with
  the API.
- **A GitHub App per agent.** Every captain and the rear admiral has its own
  GitHub App, so each one commits, comments and opens pull requests under its
  own identity. The installer provisions each agent's App metadata and private
  key as owner-only files under `~/.config/subspace/` in that agent's Linux
  home, from a secrets directory the operator supplies, and wires up Git
  credentials, the commit identity and a scoped `gh` wrapper.

Our reference deployment ran on Azure, with credentials such as the agents'
GitHub App keys in Azure Key Vault. The public version uses per-agent
credential files that the installer provisions instead.

## Screenshots

The desktop app in the Twilight theme, signed in to the local stack with
seeded demo data.

| Signed in with the local development provider | Issues with live data from the API |
| --- | --- |
| <a href="docs/assets/screenshots/desktop-sign-in-twilight.png"><img src="docs/assets/screenshots/desktop-sign-in-twilight.png" width="440" alt="Settings › Account showing the local development account developer@example.com and the API at 127.0.0.1:3000"></a> | <a href="docs/assets/screenshots/desktop-issues-twilight.png"><img src="docs/assets/screenshots/desktop-issues-twilight.png" width="440" alt="The Issues Space listing NAV and WARP issues by workflow state, with the Live indicator for issue updates"></a> |

## Run the whole system locally

You need Docker with Compose v2, `openssl` and Python 3.12 or newer for Channels
configuration. The desktop app also needs the toolchain from
[`subspace-desktop/mise.toml`](subspace-desktop/mise.toml)
(`mise install` in that folder).

From the repository root:

```sh
make stack-up
```

This generates random IRC passwords in `.stack/` (an owner-only folder that Git
ignores), then starts PostgreSQL, the API and Ergo from the root
[`compose.yaml`](compose.yaml), with the API's development sign-in enabled.
It configures the desktop's Channels Space to sign in to both local IRC
listeners as `developer`. The default seed creates fictional demo records
through the API and sets historical timestamps on new issues and comments
in the local database. Use `DEMO=0` to skip the rehearsal.

| Service | Address on this machine | Override |
| --- | --- | --- |
| API | `http://127.0.0.1:3000` | `API_PORT` |
| PostgreSQL | `127.0.0.1:5432` | `POSTGRES_PORT` |
| IRC | `127.0.0.1:6667` (plain), `6697` (TLS, self-signed), `8097` (WebSocket) | `IRC_PLAIN_PORT`, `IRC_TLS_PORT`, `IRC_WS_PORT` |

Everything binds to `127.0.0.1` only. The `local` organisation has the
`developer@example.com` account, the bootstrap Local workspace, and IRC
accounts for the developer and fleet agents. The two demo fleets share Ergo:
`aurora` uses the plain listener and `borealis` uses TLS.

The default seed adds the fictional Fallow Reach rehearsal. Navigation console,
Warp diagnostics and Relay reliability span four projects (`NAV`, `MAP`,
`WARP` and `RELAY`) with 48 issues, decision records, threaded comments, todos,
review screenshots and local simulated pull requests and deployments. The
Channels Space has conversation history in six channels, including a screenshot
in `#aurora-nav`. The same screenshot is kept on the issue for when the IRC
upload expires. No agent processes or GitHub credentials are needed.

To start with the developer account, fleet records, IRC accounts and just the
bootstrap workspace, use either command on a fresh stack:

```sh
make stack-up DEMO=0
SUBSPACE_STACK_DEMO_DATA=0 make stack-up
```

The switch controls seeding on that run and keeps existing data. Run
`make stack-demo` to add missing fixtures to a running stack. Run
`make stack-reset` followed by `make stack-up` to recreate the scenario with
the same issue keys and content. Rehearsal dates use fixed offsets from
Monday at 00:00 UTC in the seed week. The reset removes the local
database, uploaded files, channel history and generated credentials.

Check the API with a development token:

```sh
curl -H "Authorization: Bearer $(make -s dev-token)" \
  http://127.0.0.1:3000/api/v1/me/initiatives
```

Then run the desktop app. Its default build configuration signs in to the
local API as `developer@example.com` with no browser:

```sh
make run
```

`make run` also builds the terminal Space, which uses the Zig pin in
[`subspace-desktop/mise.terminal.toml`](subspace-desktop/mise.terminal.toml)
(`MISE_ENV=terminal mise install`). `make run FEATURES=` builds without it.

By default, `make stack-up` writes `irc/config.json` beside the desktop's
main `config.json`, with mode `0600`. This is shared app configuration,
including for the installed release. Both IRC networks read the SASL
password from the absolute `.stack/irc/developer.password` path in the
worktree that ran the command. Keep that worktree available while using its
IRC stack. On macOS the config is under
`~/Library/Application Support/subspace/`, or under `~/.config/subspace/`
if the main config file already exists there. On Linux the folder is
`${XDG_CONFIG_HOME:-$HOME/.config}/subspace/`.

The stack skips unchanged writes and keeps one private timestamped backup.
It matches loopback entries by network ID or host and port. It can adopt
entries without credentials or using the `developer` SASL password account.
It keeps other credentials and custom certificates intact and adds a local
entry under a free ID if needed. A hand-added connection it can adopt takes
precedence over a managed ID on a different endpoint. For host matching,
`localhost`, `irc.localhost`, `::1` and `127.0.0.1` are equivalent.
Existing connections keep their IDs, names and other settings such as
auto-connect, channels and highlights. TLS on the loopback listener accepts
Ergo's self-signed certificate.

Use `SUBSPACE_STACK_DESKTOP_CONFIG=0 make stack-up` to skip the config write.
Run `make stack-desktop-config` to apply it using the existing credentials.
`SUBSPACE_STACK_DESKTOP_CONFIG_FILE` selects a different output file.
`make stack-reset` keeps the desktop settings and removes the password files.
The next `make stack-up` generates passwords and refreshes the connections.

The desktop reads the file at startup. In **Channels**, pick an initiative
to join its channels on its fleet's IRC server. Picking an initiative of
the other fleet switches connection.

| Command | Effect |
| --- | --- |
| `make stack-up` | Start the stack, seed demo data and configure Channels. Use `DEMO=0` for a clean workspace. |
| `make stack-desktop-config` | Install the local IRC networks using the generated passwords |
| `make stack-demo` | Add or top up the demo data and channel history. `make stack-seed` is an alias. |
| `make stack-logs` | Follow the logs |
| `make stack-down` | Stop the stack and keep its data |
| `make stack-reset` | Remove the containers, data volumes and generated passwords |
| `make dev-token` | Print a development bearer token |

`make stack-check` checks the stack commands with fixtures. For a live check,
use a fresh checkout with a distinct `COMPOSE_PROJECT_NAME` and spare ports
for all five bindings. Set `SUBSPACE_STACK_DESKTOP_CONFIG=0` to skip the
desktop config write, start that stack with `DEMO=0`, then run
`make stack-check STACK_LIVE=1` with the same environment. It adds the demo
and checks that a second seed preserves the API and IRC data.

## How we run it

This is the setup that works best for us. None of it is required: the public
installer defaults to OpenCode as the harness, and the models and crew mix are
configurable.

- **Roles.** Humans are **admirals**. The **rear admiral** is a top model such
  as Fable or Astra. You spitball with it, raise issues and work out the shape
  of the work, and it assigns work to the captains so things keep moving.
  **Captains** are Opus 5.5, each with a **crew** of two Claude and two Codex
  workers.
- **Channels.** Each captain has its own channel. The other captains are in it
  too, but only the owning captain is woken by its messages; the others wake
  only when tagged. The **Starbase** channel is where admirals talk to the rear
  admiral and to each other.
- **Checkouts.** Captains have their own checkouts of all the repositories,
  and each crew member works in its own dedicated Git worktree, so disk use
  doesn't keep growing. Stargate manages all of this; it is the glue.
- **Cloud access.** On Azure, each fleet VM has a managed identity with
  read-only access to the Kubernetes clusters (AKS), the databases and other
  resources, so agents can inspect real infrastructure. Agents also have
  GitHub access to read CI and troubleshoot failures. The public equivalent is
  per-agent read-only credentials (a kubeconfig, a read-only database user and
  GitHub App permissions), which the installer provisions into each agent's
  home.
- **Medical ship.** The optional installer service watches selected Sentry
  projects for unresolved errors and fatal issues and posts links to fleet
  IRC. It can address a captain or the rear admiral for read-only triage.
  Operators can opt in to an HTTP 5xx filter for a server-side status tag.
- **Usage-aware wind-down.** Stargate posts quota notices through the rear
  admiral's Slack bot. An admiral relays them to the rear admiral in Slack or
  Starbase. At 25% remaining, the rear admiral asks captains to keep at most
  one worker from the affected family per captain and hold extra work. At 15%,
  captains pause launches and resumes, checkpoint active work at its next
  safe point and keep task claims. When Stargate posts “Quota restored.” above
  25%, the rear admiral tells captains to resume normal staffing.

### The fleet at work

In our reference setup, a captain automatically reviews every pull request
and the repository requires the **Fleet app review** check before merging.
Each captain posts reviews under its own GitHub App identity. See the
[per-agent GitHub App setup](subspace-installer/README.md#github-apps-per-fleet-member).

Select an image to see it full size.

<table>
<tr>
<td width="50%" valign="top"><a href="docs/assets/screenshots/fleet-captain-review-approved.png"><img src="docs/assets/screenshots/fleet-captain-review-approved.png" width="400" alt="GitHub approval from a captain’s app, with a Go verdict and a list of release checks"></a><br>A captain approves a Release Please pull request after checking the version bump and matching lockfiles.</td>
<td width="50%" valign="top"><a href="docs/assets/screenshots/fleet-captain-review-changes.png"><img src="docs/assets/screenshots/fleet-captain-review-changes.png" width="400" alt="GitHub review from a captain’s app, with a No go verdict and a blocking finding about duplicate Effect versions"></a><br>A captain requests changes on a dependency update that would load two copies of Effect.</td>
</tr>
<tr>
<td width="50%" valign="top"><a href="docs/assets/screenshots/fleet-captain-review-tested.png"><img src="docs/assets/screenshots/fleet-captain-review-tested.png" width="400" alt="GitHub approval from another captain’s app, after running the crate tests and Clippy on the exact commit"></a><br>Another captain runs the crate tests and Clippy on the exact commit before it approves.</td>
<td width="50%" valign="top"><a href="docs/assets/screenshots/fleet-captain-teams.png"><img src="docs/assets/screenshots/fleet-captain-teams.png" width="400" alt="A captain’s Teams reply compares memory-based and CPU-only autoscaling for staging and production"></a><br>A captain works through a staging and production capacity plan in Microsoft Teams.</td>
</tr>
<tr>
<td width="50%" valign="top"><a href="docs/assets/screenshots/fleet-rear-admiral-slack-asked.png"><img src="docs/assets/screenshots/fleet-rear-admiral-slack-asked.png" width="400" alt="An admiral asks Kirk about the Stargate repository in Slack, and Kirk shows that it is thinking"></a><br>An admiral asks the rear admiral for its take on Stargate in Slack.</td>
<td width="50%" valign="top"><a href="docs/assets/screenshots/fleet-rear-admiral-slack-review.png"><img src="docs/assets/screenshots/fleet-rear-admiral-slack-review.png" width="400" alt="Kirk’s Slack reply highlights Stargate’s security and restart handling, then flags the pace of change and a breaking rename"></a><br>The rear admiral replies in the thread with what looks good and what it would watch.</td>
</tr>
</table>

See the setup guides for connecting a rear admiral to [Slack](docs/slack.md)
or [Microsoft Teams](docs/teams.md).

## Repository layout

| Folder | What it is |
| --- | --- |
| [`subspace-framework/`](subspace-framework/README.md) | The Rust crates desktop dashboard apps are built from, behind one crate, `subspace-framework` |
| [`subspace-desktop/`](subspace-desktop/README.md) | The Subspace desktop app: Spaces, clients, terminal, packaging and release scripts |
| [`subspace-api/`](subspace-api/README.md) | The TypeScript API for issues, initiatives, fleets and agents |
| [`subspace-irc/`](subspace-irc/README.md) | Ergo IRC packaged as a container |
| [`agent-skills/`](agent-skills/README.md) | Skills that connect your local Claude Code, Codex and OpenCode to the tracker (MCP) and the fleet's IRC |
| [`subspace-installer/`](subspace-installer/README.md) | The fleet VM installer, and in `vm/` the setup the root `install.sh` and `install.ps1` run on a VM |

Each folder is a project of its own, with its own toolchain, checks and docs.
From the root, `make check`, `make test` and the other shared targets run in
the Framework and then the desktop app, and the app's own targets, such as
`make run`, run in `subspace-desktop/`. Plain `make` lists them. Contributors
and coding agents should start with [AGENTS.md](AGENTS.md).

## Licence

Subspace is licensed under the [Apache License 2.0](LICENSE).
See [NOTICE](NOTICE) for attribution and links to third-party notices.
See also the [desktop third-party licences](subspace-desktop/licenses/),
the [web component gallery notices](subspace-desktop/web-spaces/examples/component-gallery/public/),
the [Framework crate notices](subspace-framework/docs/publishing-audit.md#licence-and-notices),
and the licences for the vendored
[GPUI Base](subspace-framework/vendor/gpui-base/LICENSE-APACHE) and
[Markdown](subspace-framework/vendor/markdown/license) crates.

The IRC client protocol layer was reimplemented in a clean room.
The [clean-room log](subspace-desktop/clients/irc/CLEANROOM.md) records the
specification, implementation and review.

## Status

Early development.
