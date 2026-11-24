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
the model, reasoning effort and harness per role, and the model per crew
family; captains default to Claude Opus 5.5 and the rear admiral to Claude
Fable 5.2 ([officer models](#officer-models)). The dashboard
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

The two example files are the smallest fleet that installs: a rear admiral
and one captain on a remote IRC server with certificates, so the secrets
file holds no IRC password, and every optional feature off.
[Optional settings](#optional-settings) shows the others together.

For remote IRC, register a SASL account for the rear admiral and each
captain, such as `example-atlas` and `example-orion`. They sign in with a
certificate each, whose fingerprints the installer prints for you to bind to
the accounts ([IRC sign-in](#irc-sign-in)); with `"auth": "password"` under
`irc`, supply their passwords in `ircPasswords` instead. Crew have no IRC accounts. Add trusted human
SASL accounts to `irc.allowedAccounts`. The officers create the fleet's
[standard channels](#channels) (`--dry-run` lists them) by joining them, so
they need no registration; registering them with ChanServ is optional, keeps
their topic and modes when they empty, and lets the IRC operator grant access
to the appropriate accounts. Only a server that lets operators alone create
channels needs them created first. For local IRC,
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

### Officer models

Captains run `claude-opus-5-5` at reasoning effort `xhigh`, and the rear
admiral runs `claude-fable-5-2` at `high`, both through `claude-proxy`, so the
model proxy needs a Claude account (`./install.sh providers login claude`).
Crew keep their own family defaults (see [Crew and
repositories](#crew-and-repositories)).

Choose others with `subspace-setup` (or the same options on `./install.sh`):

```sh
sudo subspace-setup --captain-model gpt-6.2-sol --captain-effort high
sudo subspace-setup --rear-admiral-model claude-opus-5-5 --rear-admiral-effort max
sudo subspace-setup --captain-model '' --rear-admiral-model ''   # back to the defaults
```

- A `claude-*` model goes through the Claude family (`claude-proxy`), and any
  other through the Codex family (`openai-proxy`).
- An effort belongs to its model. A new model gets the provider's default
  effort unless the same run names one with `--captain-effort` or
  `--rear-admiral-effort`.
- Later runs keep each choice. `''` returns that role's model or effort to
  the default, even over one set in `fleet.json` by hand. If a fleet-wide
  `harness` model or effort is set there, setup moves it into the other
  role's block, so that role keeps running it, and says so. An officer's
  own `harness` still wins, and crew are not affected.

`subspace-setup` writes these choices as `harness.captain` and
`harness.rearAdmiral` in `fleet.json`, for example
`"captain": {"family": "codex", "model": "gpt-6.2-sol", "effort": "high"}`.

The installer resolves each officer's harness in this order:

1. The role's default.
2. The fleet-wide `harness` (its `model`, `family` and `effort` apply to
   both roles).
3. The role's block.
4. The officer's own `harness`.

At any of these levels, a `model` without a `family` goes where its ID says:
`claude-*` through the Claude family and any other through Codex. A harness
written for the old all-Codex default, such as `{"model": "gpt-6.2-sol"}`,
therefore runs as it did. A model and a family that disagree are refused.
A harness that names only a family runs that family's default for the
role. With `codex` that is `gpt-5.4`. With `claude` it is the role's Claude
default, `claude-opus-5-5` or `claude-fable-5-2`, rather than
`claude-sonnet-5-5` as before.

How the effort reaches the model:

- **OpenCode.** OpenCode gives each model reasoning variants for its
  provider package, and the installer names the variant in the model
  reference, as in `claude-proxy/claude-opus-5-5#xhigh`. Claude models take
  `low`, `medium`, `high`, `xhigh` and `max`. Claude 4.6 has no `xhigh`.
  Claude 3.7 and 4.0 to 4.5, except Opus 4.5, take only `high` and `max`.
  The installer reads dated IDs by their version, so
  `claude-opus-4-20250514` counts as Claude 4.0.
  The Codex family takes `low`, `medium` and `high`.
- **Claude Agent SDK.** An officer with `"type": "claude-sdk"` passes the
  effort to Stargate's Claude backend as `effort`.

The installer refuses an effort that the officer's harness does not offer.

Both the dry run and the install stop if the model proxy has signed-in
accounts but none can serve an officer's model:

- A `claude-*` model needs a Claude account.
- A `gpt-*` model needs a Codex account.

The message names the officers and the login to run. Before the first
sign-in the dry run only names the account type the officers need. Models
of other providers are not checked.

A fleet that `subspace-setup` set up before these defaults, and whose
`fleet.json` names no officer model, keeps `gpt-5.4` at the provider's
default effort. Setup says so once. To move it to the defaults, sign the
proxy in to a Claude account, then run:

```sh
sudo subspace-setup --captain-model '' --rear-admiral-model '' --dry-run
sudo subspace-setup --captain-model '' --rear-admiral-model ''
```

A hand-written `fleet.json` with no officer model takes the new defaults
on its next run.

### Crew and repositories

List repositories in order as `{"id":"project","url":"https://github.com/example/project.git"}`
objects under `repositories`. Every captain gets a checkout of each one, side
by side, in its workspace,
`/var/lib/subspace-fleet/agents/<captain-id>/workspace/<repository-id>`. A
captain's sessions start in the workspace itself, above the checkouts, so one
session reaches every repository (see [Captain sessions](#captain-sessions)),
and the first repository is its crew's default. An empty list gives each
agent an empty workspace and gives captains no crew. Repository IDs start
with a lowercase letter, `default` is reserved, and a captain with crew takes
at most nine repositories. The rear admiral gets read-only reference
checkouts under `/var/lib/subspace-fleet/references/<rear-admiral-id>/` that
the mirror service refreshes, and works in its reference checkout of the
first repository. The rear admiral can inspect them from its own Linux
account, including Git history. `subspace-setup --repositories` fills this
list from comma-separated HTTPS URLs, optionally written as `ID=URL`.
A repository without `"private": true` is cloned without credentials, so the
installer first checks that Git can read it that way. When it cannot, the dry
run and the install both stop before cloning anything and say how to go on:
install without those repositories now, and add them as private ones once the
officers have GitHub Apps ([Add private repositories later](#add-private-repositories-later)).
An installed repository is not checked again.

A repository also takes an optional one-line `description` and `tracker`,
free text that names where its work is tracked, for example
`"tracker": "Example issues at https://tracker.example.com/project"`. Both
appear in each captain's [workspace instructions](#workspace-instructions),
so a fleet whose repositories use more than one tracker can say which is
which. A repository without a `tracker` uses the fleet's Subspace tracker
when `api` is set; otherwise its instructions say the tracker is not
recorded. Unlike the URL and visibility, both may change at any time.

Set fleet-wide `crew.claude` and `crew.codex` defaults, or override either
under a captain's `crew`. Each family takes a `count` from 0 to 10 and a
`model`. `subspace-setup` writes the fleet-wide ones from `--crew-claude`,
`--crew-codex`, `--crew-claude-model` and `--crew-codex-model`, and keeps a
model set in `fleet.json` by hand when no option names one. Crew run inside their captain's OpenCode server, as the captain's
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
  identities and worktrees permanent and names the first repository's
  checkout as the crew's default, wherever the captain's session works. The
  installer refuses a later change,
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
workspace and the captain's permissions. Creating a session starts no model
turn. The session ID goes into both the gateway's `backend.sessionID` and the
plugin's `captainSessionID`, and the installer restarts the services to load
the plugin. The plugin's `captainDirectory` is the workspace and its default
project is the checkout of the first repository: the layout Stargate's own
guide uses, a captain directory outside every crew worktree beside a separate
project checkout.

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

A changed officer model or effort reaches a pinned root, too. The installer
switches the root's model, as Stargate's captain model tool does, and the
root's next turn uses the new model with its history intact. If OpenCode
refuses the switch, the installer says so and the root keeps its model. The
next run tries again.

If commissioning fails before a session is confirmed, fix the cause, such as
the engine or the model proxy, and run the installer again. It retries the
reserved ID. Until the session exists you can still change the first
repository. If a confirmed root is missing from the engine, the installer
stops for that captain. Restore
`/var/lib/subspace-fleet/sessions/<captain-id>/opencode.db` from a backup, or
retire the captain and give its crew to a new captain ID.

**Where sessions start.** A captain's sessions start in its workspace,
`/var/lib/subspace-fleet/agents/<captain-id>/workspace`, above its
checkouts, with or without crew. Its optional `sessionRoot` chooses:
`"workspace"`, the default, or `"repository"`, its checkout of the first
repository, where installers before this one started every captain. The
rear admiral takes no `sessionRoot`: it works in its reference checkout of
the first repository, and its root is pinned only for
[agent dispatch](#agent-dispatch) or its own
[Slack bot](#slack-for-officers). `--dry-run` prints where each captain's
sessions will start, and where the rear admiral's root is or will be pinned,
and says when a pinned or installed root differs from the default or the
setting. The installer records each captain's root in
`/etc/subspace-fleet/session-roots.json`.

**Upgrading a running fleet never moves a session.**

- A captain pinned in its first repository stays there, and each run says
  so until you set its `sessionRoot` to `"repository"`. Setting it to
  `"workspace"` is refused: the pinned directory is named by Stargate's crew
  state and by the gateway's records for that captain, and Stargate refuses
  a changed captain directory once its crew state exists. To work from the
  workspace, give the crew to a new captain ID, whose session is created
  there.
- A captain without a pinned session keeps the root it was installed with;
  one whose checkout of the first repository already exists started there.
  To move it, set its `sessionRoot` to `"workspace"`: its conversations then
  start new sessions in the workspace, and earlier sessions stay in its
  history. `--dry-run` shows the move first.
- A new captain starts in its workspace.

**What changes with a workspace root.** The workspace is not a Git
repository. OpenCode 2.3.2, the pinned release, gives such a directory a
project of its own, keyed by its path (checked in its source), and:

- takes no snapshots there, so the root session cannot restore files
  through OpenCode's snapshots; each checkout's Git history is the record;
- reads `AGENTS.md` from the workspace and the directories above it, up to
  the captain's home, and `.opencode/`, `opencode.json` and `.claude/` from
  the workspace and every directory above it, never from the repositories
  below. The root session therefore no longer loads the first repository's
  own `AGENTS.md` or `.opencode/` automatically: the
  [workspace instructions](#workspace-instructions) name each repository's
  instructions instead, and crew sessions, in their worktrees, still load
  their repository's;
- treats every checkout as inside the session's directory, so a captain
  without crew can work in all of its repositories under its permissions,
  not only the first, and runs commands in a checkout by setting the shell
  tool's working directory.

The installer's engine configuration, with the crew plugin, crew agents,
providers, MCP server and permissions, is the file `OPENCODE_CONFIG` names,
`/etc/subspace-fleet/opencode-<captain-id>.json`, and the role instructions
and skills are under `~/.config/opencode/`, so none of it depends on where a
session starts. Stargate's crew plugin needs no Git repository for the
captain's directory: it checks only that the directory exists and lies
outside every crew worktree, and runs Git in the project checkouts and
worktrees. Its shared working agreement, `WAYS_OF_WORKING.md`, is read from
the captain's directory, so it now belongs in the workspace rather than in a
checkout. Stargate's Fleet API reports a ship's repository from its crew's
default project; for a captain without crew, which has none, it reports no
repository.

### Workspace instructions

Every install and upgrade writes two files into each captain's workspace,
beside its checkouts and never inside one:

- `AGENTS.md`, which OpenCode loads for every session in the workspace and,
  because it reads `AGENTS.md` up to the captain's home, for a session in one
  of its checkouts too, such as a root pinned in the first repository. It
  says who the captain is and who its rear admiral is; lists each
  repository with its ID, `description`, remote, tracker (see
  [Crew and repositories](#crew-and-repositories)), where its own
  instructions are and, for a captain with crew, where its crew's worktrees
  are; lists the fleet's [channels](#channels) and, for a captain with its
  own [Slack bot](#slack-for-officers), that Slack is another way in; and
  says that work in a repository follows that repository's own `AGENTS.md`,
  rules and tracker. The role instructions, with the Slack section that
  explains Slack requests, stay in `~/.config/opencode/AGENTS.md`, which
  OpenCode loads as well, so the workspace file adds to them and replaces
  nothing.
- `CLAUDE.md`, which imports it (`@AGENTS.md`) for Claude Code. OpenCode
  reads a `CLAUDE.md` only where a directory has no `AGENTS.md`, so it
  ignores this one. Stargate's Claude backend loads neither file by default,
  so a `claude-sdk` captain does not see them.

The text comes from [`lib/roles/captain/WORKSPACE.md`](lib/roles/captain/WORKSPACE.md);
edit it and re-run the installer to change every captain's file. Both files
start with a comment that marks them as generated, and the installer writes
them as the captain's Linux user, mode `0600`, rewriting them only when they
change.

To add your own guidance for one captain, write it between the two operator
notes markers at the end of its `AGENTS.md`, as the captain's user, for
example `sudo -e -u ss-orion /var/lib/subspace-fleet/agents/orion/workspace/AGENTS.md`.
Everything between the markers survives regeneration; anything else in the
file is replaced. The captain can add notes there too. If the installer finds
an `AGENTS.md` or `CLAUDE.md` it did not write, it keeps that text in the
operator notes. If a file is not a regular UTF-8 file the captain can read,
such as a symbolic link, it leaves both files unchanged and says so.

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
A captain with crew can also read its crew's worktrees,
`/var/lib/subspace-fleet/agents/<captain-id>/worktrees/`, with OpenCode's
read tools and as its shell's working directory. What else a captain may do
depends on `captainTools`.

**Standard captains** (`captainTools: "standard"`) can edit their checkouts,
stage and commit locally, and run common build and test commands. For a
captain without crew, external paths, publication and other shell commands
require an explicit grant. A captain with crew can reach them through its
crew (see below). Build and test commands can execute code from the project,
so use trusted repositories.

**Delegate captains** (`captainTools: "delegate"`) plan, review and open pull
requests with `subspace-github gh pr create` and their GitHub App; their crew
edit, test, commit and push. A delegate captain has no edit, write or patch
tool, may work in any directory, and has an open shell with writes refused:
any command runs, chained, piped or in a subshell, except

- the Git commands that change a checkout or publish it: `push`, `commit`,
  `merge`, `merge-file`, `mergetool`, `rebase`, `cherry-pick`, `revert`,
  `am`, `pull`, `reset`, `add`, `stage`, `rm`, `mv`, `restore`, `checkout`,
  `switch`, `clean`, `apply`, `bisect`, `init`, `update-ref`,
  `update-index`, `read-tree`, `checkout-index`, `replace`,
  `filter-branch`, `filter-repo` and `fast-import`;
- `branch`, `tag`, `stash`, `notes`, `worktree`, `submodule`,
  `sparse-checkout`, `config` and `remote`, but for their listing forms,
  such as `git branch -a`, `git branch --show-current`, `git tag --list`,
  `git stash list`, `git worktree list`, `git config --get` and
  `git remote -v` (`DELEGATE_GIT_READS` in [`lib/roles.py`](lib/roles.py));
- the in-place editors `sed -i` and `perl -i`.

Each Git refusal covers `git` itself, Git's global options before the
subcommand (`git -C project push`) and anything before `git`
(`GIT_TERMINAL_PROMPT=0 git push`, `env git push`, `xargs git rm`).
`git fetch`, `git log`, `git diff`, builds, tests and everything else run.

**This is a working agreement, not a security boundary.** OpenCode 2.3.2
checks each command of a command line separately, by the command's text, and
the last matching rule decides. Text cannot tell every write from a read:

- Still allowed: a redirect (`echo … > file`), `tee`, `cp`, `mv`, `rm`, a
  script or build that writes, an interpreter (`python3 -c …`,
  `bash -c 'git push'`), a path to Git (`/usr/bin/git push`) and a Git alias.
- Refused although it only reads: a command in which one of those Git words
  follows `git` and its options as a bare word, such as
  `git -C project log --grep stash` (quote the word, or set the shell's
  working directory instead of using `-C`), and an argument, such as a pull
  request body, that contains ` git push ` or the like.

The captain's instructions state the agreement, and its crew can do all of
it anyway (see below), so protect the branches that matter on the server. A
captain without crew has no one to delegate to, so in a delegate fleet it
keeps the standard policy, and the dry run names it. The `fleet-captain`
skill and a delegate captain's instructions follow `captainTools`, whatever
`harness.permissions` says.

**New fleets delegate.** `subspace-setup` gives a new fleet `delegate`, the
default of its question and of an unattended run, and writes it to
`fleet.json`; [`fleet.example.json`](fleet.example.json) has it too. Choose
with `--captain-tools standard` or `--captain-tools delegate` on the host
installer or `subspace-setup`; reruns keep the choice. In a hand-written
fleet file, use `captainTools`.

**Existing fleets keep theirs.** A fleet file without `captainTools` means
`standard`. That is how a fleet installed before `captainTools` existed is
read, so an upgrade leaves its captains coding as they were installed to,
perhaps part-way through work, rather than taking their edits and commits
away unannounced; `--captain-tools delegate` moves it. Setup sets no default
on a VM it already has answers or inputs for, or whose fleet the fleet
installer set up alone.

`--dry-run` says which captains delegate, and compares each OpenCode
officer's tool policy with the one installed in its engine configuration
(`/etc/subspace-fleet/opencode-<id>.json`): it lists the rules the run adds
(`+`) and removes (`-`) in policy order, since the last matching rule wins,
or says the policy is unchanged or new. Run it as root to read the installed
policies. `subspace-setup --dry-run` gives the installer the API settings a
real run will write, from `api.env` as the run will leave it (development
sign-in and the agent listener), the saved initiative and stand-ins for the
tokens, so the comparison includes the API's tools. The installer applies a changed policy to a pinned root session too
([Captain sessions](#captain-sessions)).

To extend the starter policy, list rules in `harness.extraPermissions`: for
the whole fleet, in `harness.captain` or `harness.rearAdmiral` for one role,
or in an officer's own `harness`. The installer appends them in that order
after the policy, so they win; for example
`{"action": "shell", "resource": "kubectl get *", "effect": "allow"}` lets
standard captains and the rear admiral run read-only `kubectl get`, and a
`deny` takes a starter rule away. To replace the starter policy instead,
supply an ordered `harness.permissions` array across the fleet or for the
rear admiral or a single captain; `extraPermissions` still extends it. Rules
use `action`, `resource` and `effect` (`allow`, `deny` or `ask`), the last
matching rule wins, so put narrower rules after broader ones. See
[OpenCode permissions](https://opencode.ai/docs/permissions/). Neither key
applies to a Claude Agent SDK officer, which uses `allowedTools`. A captain
with crew also gets `stargate_crew`, `stargate_mail` and `stargate_history`,
and `stargate_model` for `get`, on its pinned session.

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
and prints them in `--dry-run`. On a server it does not manage, the officers
create them by joining; registering them there is optional. Their names are fixed. `irc.starbaseChannel`,
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

### IRC sign-in

`irc.auth` sets how the rear admiral and the captains sign in to IRC:
`certificate` (SASL EXTERNAL), the default with a remote server, or
`password` (SASL PLAIN), the default and for now the only choice with the
[Ergo the installer manages](#local-irc-and-tls). Crew have no IRC accounts,
and the [medical ship](#medical-ship) keeps its own password either way.

**With certificates, no IRC password exists for an officer.** The installer
gives each officer a P-256 key and a self-signed certificate, generated on
the VM at the first install, and keeps them across upgrades in one PEM
bundle, `/etc/subspace-fleet/irc-certs/<officer-id>.pem`. The bundle is
owned by the Linux user whose gateway serves the officer
(`ss-fleet-gateway`, or the officer's own user for a Claude Agent SDK
officer), with mode `0600`, and the gateway's Stargate registration names it
as `clientPemFile`. The installer refuses officer entries in `ircPasswords`
and passes no password to the gateway.

The pinned Stargate takes exactly one of `passwordEnv` and `clientPemFile`
per account, presents a client certificate only over TLS, and reads only an
owner-only regular file of its own user, at most 64 KiB, whose first
certificate matches an unencrypted private key (`PRIVATE KEY`,
`EC PRIVATE KEY` or `RSA PRIVATE KEY`). `--dry-run` and every render check
the same, so a bundle Stargate would refuse stops the installer instead.

**Bind the fingerprints on the IRC server.** An officer can sign in only
once its certificate's fingerprint is bound to its account. `--dry-run` and
the install summary print one line per officer, with the SHA-256 fingerprint
in the lower-case hex form Ergo's NickServ takes, and never key material:

```text
/msg NickServ CERT ADD example-orion 4b1d…0c9e
```

Send each line as an IRC operator whose operator class has Ergo's `accreg`
capability, or signed in as the account itself, leaving out the account name.
On another server, bind the same fingerprint as its services expect. A dry
run writes nothing, so a certificate the VM has yet to generate shows its
fingerprint only in the install summary. Print them again at any time with
`sudo /opt/subspace-fleet/lib/runtime.py irc-fingerprints`.

**Bring existing certificates.** A fleet whose IRC server already has
certificates bound can supply them: put each officer's certificate and its
unencrypted key, certificate first, in
`<secrets-dir>/<officer-id>/irc-client.pem` with mode `0600`, as for
[GitHub App keys](#github-apps-per-fleet-member). Dispatch carries them in its
private bundle. The installer checks each one and installs it in place of the
officer's current bundle, printing the old and new fingerprints when it
replaces one. Later runs keep it, with or without the secrets directory.
`--dry-run` with the secrets directory prints a supplied bundle's
fingerprint, so you can bind it first. To replace a certificate, supply a new
bundle, or delete the officer's bundle under `/etc/subspace-fleet/irc-certs/`
and run the installer again to generate one; then bind the new fingerprint
and remove the old one with `NickServ CERT DEL`.

**Keep passwords.** For a server that cannot do SASL EXTERNAL, set
`"auth": "password"` under `irc` and supply each officer's password in
`ircPasswords`. A remote fleet installed before certificates became the
default has those passwords: the installer refuses it, a dry run included,
until you either set `"auth": "password"`, or move to certificates. The
installed fleet keeps signing in with its passwords until the upgrade, so
to move without a gap, work on your copies of the inputs in this order:

1. In your secrets file, remove the rear admiral's and the captains'
   entries from `ircPasswords` (keep any for allowlisted humans in local
   mode). In `fleet.json`, leave `irc.auth` out or set it to
   `"certificate"`.
2. Generate each officer's bundle yourself and put it in your secrets
   directory as `<officer-id>/irc-client.pem`, mode `0600`, for example

   ```sh
   umask 077
   openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 \
     -pkeyopt ec_param_enc:named_curve -nodes \
     -days 3650 -sha256 -subj "/CN=example-orion" -keyout key.pem -out irc-client.pem
   cat key.pem >> irc-client.pem && rm key.pem
   ```

3. Run the installer with `--dry-run` and those inputs, such as
   `sudo ./install.sh upgrade --config ~/fleet.json --secrets-dir ~/fleet.secrets --dry-run`.
   It prints a `NickServ CERT ADD` line with each supplied bundle's
   fingerprint, marked `(supplied)`.
4. Bind those fingerprints on the IRC server.
5. Upgrade with the same inputs, without `--dry-run`.

Without step 2 the dry run cannot print fingerprints, because the VM
generates the certificates during the upgrade: the install summary prints
them, and the officers stay off IRC until you bind them.

**With `subspace-setup` and a remote server**, officers sign in by
certificate too, unless `--irc-auth password` says otherwise. Without the
option, setup keeps how the installed fleet signs in (its
`/etc/subspace-fleet/fleet.json`, which the fleet installer alone may have
changed), else its own earlier choice; a remote fleet it set up before this
release keeps passwords. Setup keeps each officer's bundle in
`/root/fleet/fleet.secrets/<officer-id>/irc-client.pem`, from where the
fleet installer takes it: the installed fleet's bundle when there is one, so
a fingerprint already bound stays valid, else a new one, made once. It
prints, for each officer, its account, its
SHA-256 fingerprint and the two lines the server's operator runs:

```text
/NS SAREGISTER example-fleet-orion
/NS CERT ADD example-fleet-orion 4b1d…0c9e
```

A dry run generates nothing, so it prints these only for certificates a
real run has made. When you hold an operator credential whose class has
NickServ's `accreg` capability, pass `--irc-operator NAME` and
`--irc-operator-password-file FILE` (a root-only file with mode `0600`; from
your machine, the host installers copy it for that run and remove it
afterwards). On a server that admits only signed-in connections (Ergo's
`require-sasl`), whose operators are not accounts, also pass
`--irc-operator-account NAME` and `--irc-operator-account-password-file FILE`
(the same rules) for an IRC account the connection signs in to first by SASL
PLAIN; without them setup stops when the server answers `ACCOUNT_REQUIRED`,
naming these options. Setup then opers up (`OPER`) and, before any officer
starts, sends both lines for every officer whose certificate does not sign in
yet, binding a certificate only to an account `SAREGISTER` has just created:
it never changes an existing account's certificates, and stops, naming the
account, when one exists without the officer's certificate. It then checks
that each certificate signs in by SASL EXTERNAL as its account, stopping if one does not. A dry run with
the credential only checks it and reports which certificates already sign
in. The passwords are sent only to the server, over TLS, and never printed or
saved.

**The Ergo the installer manages keeps passwords for now**, in local mode
and on the shared server `./install.sh irc-server` runs, for which the host
installer passes `--irc-auth password`. Three things are missing:

- Fleet members reach local Ergo over plain loopback IRC, and Stargate
  presents a client certificate only over TLS. The gateways would need the
  local TLS listener and a CA file for it, but Ergo generates that
  certificate in its own volume when it starts, so the installer would have
  to generate or pin it itself.
- The image seeds every account with a password (`IRC_ACCOUNTS_FILE`).
  Officers would instead need certificate-only accounts, registered with
  `NickServ SAREGISTER <account>` and no passphrase, and their fingerprints
  bound with `NickServ CERT ADD <account> <fingerprint>` as the operator at
  seed time. `seed-channels.py` already signs in as the operator, so it could
  do this from a fingerprint file the installer mounts, without changing the
  entrypoint.
- For a shared server, each fleet's bundle would carry its officers'
  fingerprints instead of passwords, and `subspace-setup irc-server add`
  would seed them the same way.

## Subspace API registration

With `api` set, `subspace-fleetctl register`, which the installer runs after
each install or upgrade, records the fleet in the Subspace API as the
desktop needs it: the desktop marks an agent's rank in Channels only by the
IRC nick on its API agent record, and joins the channels the fleet and its
ships record plus the initiative's own.

- **The fleet**: its name, the IRC endpoint the desktop uses
  (`irc.advertise`, else the gateway's), `vmHost`, its standard
  `starbaseChannel` and `commandChannel`, and `fleetApiUrl`, where the
  desktop reaches Stargate's Fleet API (below).
- **One ship per captain**, named after the ship, the captain's `name`, and
  recording its ship channel, `#<slug>-<captain-id>`.
- **The rear admiral's agent record**, rank `rearAdmiral`, named with its
  own `name`, and **one per captain**, rank `captain`, on its ship. A captain
  has no name of its own, so its agent name is its account,
  `<slug>-<captain-id>`. Each record has `ircNick` (the officer's IRC
  account), `defaultModel` (its harness model) and `serviceTokenClientId`,
  the account subject the API knows it by: by default
  `<account>@agents.example.com`, which suits the API's development sign-in.
  With WorkOS or Entra, set the member's `serviceTokenClientId` to its
  provider subject (an Entra object ID, for example).
- **Crew have no records.** They are not on IRC, and the API records crew
  work under their captain with the crew member's name and model.

The API keeps the rear admiral's name as its agent name, which allows only
letters, digits and hyphens (at most 64), and is unique in the organisation
ignoring case, as is every agent's IRC nick. Ship names are one line of at
most 64 characters, distinct in the fleet, and a captain's account, its agent
name, has no underscores. `--dry-run` refuses a fleet with `api` that breaks
these rules. The `apiToken` must belong to a human organisation
administrator: only one may register fleets and agents.

Registration is idempotent. Existing records are found by IRC nick, else by
account subject, and changed only where they differ, so a re-run writes
nothing. A renamed ship keeps its record. A captain removed from the roster
has its agent record deactivated and its ship's channel cleared (the API
cannot delete ships); add the captain back and its record is reactivated. The
installer refuses, naming the record, to take over an agent with the same IRC
nick under another account subject, an agent of another fleet, or an agent
name already in use.

**The Fleet API address.** The desktop reads each fleet's live state from
its Stargate Fleet API, at the `fleetApiUrl` the fleet records. With
`stargate.api` on, the installer records `api.fleetApiUrl` when it is set,
for example `https://fleet.example.ts.net:8484` for an HTTPS name on the
tailnet that you set up yourself. Otherwise it records the SSH tunnel's end on
the desktop, `http://127.0.0.1:<stargate.api.port>`: every way of reaching a
fleet (the SSH tunnel, over Tailscale or through Cloudflare) forwards that
port to the same port on the desktop's loopback. With `stargate.api` off it
records none, and `api.fleetApiUrl` is refused. The address must be `https`,
or `http` only on `localhost`, `127.0.0.1` or `[::1]`, with no credentials,
query or fragment, and is sent in the form the API stores it, so a re-run
writes nothing. Registration never sends the Fleet API's token to the
Subspace API or prints it: the desktop app asks for the fleet's token once and
keeps it on the user's own machine.

**Pull requests in the Fleet API.** With `stargate.api` on, the installer
lists each crew worktree of a GitHub repository, by its path, with that
repository's `owner/repository` in Stargate's `api.pullRequestRepositories`,
so the Fleet API can show the pull request of a crew member's branch. It
writes the key only when the installed Stargate's Fleet API configuration
has it (from v0.42.0): v0.40 and v0.41 refuse it.
Stargate reads the branch with Git and looks the pull request up with `gh`,
both as the gateway's Linux user. Until that user can read the captains'
worktrees, which lie in their mode `0700` homes (STAR-4), and has `gh`
installed and signed in to GitHub, which the installer does not do, a
crew context's `pullRequest` stays `null`.

`subspace-setup` registers the same records with the API on its VM before it
runs the installer, so [agent dispatch](#agent-dispatch) can sign the rear
admiral in, and `subspace-setup register-fleet` registers a further fleet's
from its `subspace-setup fleet-registration` output. `subspace-setup
--fleet-api-url <https address>` sets the address (`--fleet-api-url tunnel`
goes back to the tunnel's end). A further fleet's registration carries only
an address given that way, or none while its Fleet API is off: every fleet's
tunnel uses the same local ports, so the tunnel's loopback end belongs to the
first fleet, and `register-fleet` refuses a loopback address for a further
fleet. Without one, the API keeps the address it has. Setup gives the fleet's
initiative no channels of its own: the standard channels belong to the fleet
and its ships.

**Who owns the fleet's work.** A tracker project belongs to its initiative's
fleet, or to the fleet of that initiative's nearest ancestor with one, and the
API refuses a fleet agent's assignment, or *Ready for agents* flag, in another
fleet's project. So `subspace-setup`, in its dry run and at the end of a real
run, reads the tracker as the rear admiral (through the agent listener, or a
hosted API's signing-proxy socket once the officers have enrolled; read-only)
and names, with a `WARNING`, each tracker project linked to one of the fleet's
GitHub repositories that belongs to another fleet, with the initiative that
makes it so, for example one the API's development sign-in created for its
default `local` fleet. To make that initiative the fleet's own, pass
`--initiative <its UUID>` (kept on later runs; `--initiative ''` goes back to
finding one by the fleet's name). With an API on this VM that still offers
development sign-in, setup checks that it exists, and stops before changing
any tracker record, recording nothing, if it does not; otherwise it uses it
as the fleet's initiative and assigns it, and everything below it, to this
fleet, moving it from the other fleet; the dry run says so first. Only a run
with `--initiative` moves an initiative: if another fleet takes the fleet's
recorded initiative later, a run without it leaves the initiative there,
says so and registers none for this fleet, and `subspace-fleetctl register`
likewise assigns only an initiative of no fleet. A hosted API's
administrators keep its records, as do those of an API without development
sign-in, so setup prints the request for one of them to make instead
(`PUT /api/v1/initiatives/<id>/fleet` with the fleet's UUID, the `id` that
`GET /api/v1/fleets/<slug>` returns), whether or not it can read the tracker.
A project that lists none of the fleet's repositories is not checked.

## The API's agent listener

The fleet's rear admiral and captains use the Subspace API as their own agent
records, through its MCP server, without WorkOS, Entra or a token.

- **`subspace-setup`** offers the API's agent listener once:
  `AGENT_PORT=3001` and `AGENT_ORGANISATION=dev:<organisation>` in
  `/root/fleet/api.env`, the development sign-in organisation the agents are
  registered in, with a comment line recording the offer. It publishes
  `127.0.0.1:3001` only while the listener is on, and keeps
  `AGENT_ORGANISATION` in step with the organisation while it has the
  installer's `dev:<organisation>` form. An operator turns the listener off
  for good by setting `AGENT_PORT=` (or deleting it), and back on with
  `AGENT_PORT=3001`. Setup also turns it off when `NODE_ENV` is `production`,
  where the API refuses it, and when development sign-in is off (WorkOS or
  Entra); the agents then have no way into the API yet. While it is on, setup adds
  `api.agentUrl: "http://127.0.0.1:3001"` to the fleet configuration.
- **The lockdown** keeps port 3000 (the main listener, with development
  sign-in) and PostgreSQL operator-only, and admits to port 3001 only root,
  the operator and the officers' Linux users (`--port-user 3001:ss-<id>` for
  the rear admiral and each captain; crew run as their captain). The fleet's
  other service users and containers cannot reach it. The audit fails if the
  agent port is open to every local user, is 3000 or 5432, or answers the
  development sign-in route; it still fails if port 3000 is open to the
  agents.
- **OpenCode officers.** With `api.agentUrl` set, each officer's engine
  configuration (`/etc/subspace-fleet/opencode-<id>.json`) gets a remote MCP
  server `subspace` at `<agentUrl>/mcp` with the header
  `X-Subspace-Agent: <the officer's IRC account>`, which the API resolves to
  the officer's agent record by IRC nick. The engine's permissions, and the
  stored grants of a pinned root, gain `{"action": "subspace_*", "resource":
  "*", "effect": "allow"}`. Crew sessions run on their captain's server, so
  they share its MCP server and act as the captain, naming themselves with
  `relayedCrew` on each write.
- **Claude Agent SDK officers** get the same server through Stargate's
  `backend.mcpServers`, with a `headersHelper` that runs
  `/opt/subspace-fleet/lib/runtime.py agent-header <account>`, since that
  backend takes header values only from a helper. Stargate permits every tool
  of a configured server itself (`mcp__subspace`), and `allowedTools` holds
  built-in tools only, so it is left as configured. That backend loads no
  skill files, so the `subspace-tracker` skill's text is appended to the
  officer's system prompt instead.
- **Skills.** Each officer also receives the generic `subspace-tracker` skill
  in `~/.config/opencode/skills/`, the same file as
  `agent-skills/opencode/subspace-tracker/SKILL.md`, which the host installer
  now copies to the VM with the rest of the payload. With `api.agentUrl` set, the
  installer and the configuration check both stop, naming that file, when the
  payload lacks it. It covers both ways in
  and the API's rules for issues, trackers, decisions and the rest.

The agent listener trusts its network as development sign-in does, minus
administrator access: anyone who can reach port 3001 on the VM, which the
lockdown limits to root, the operator and the officers' users, can act as any
registered agent of the organisation, so agents can act as each other. It
serves no sign-in route and no administrator route or MCP tool, it never
resolves a person, and it refuses browser requests (an `Origin` header) and
any host other than loopback. A further fleet installed with `--api-target` gets no
agent listener (INSTALL.md, "One fleet or several").

What is not verified yet: the OpenCode release the fleet pins has not been
checked for the `mcp.servers.<name>.headers` field, and Stargate's curated
crew permissions allow only listed MCP prefixes, so crew may not be able to
call `subspace_*` tools until Stargate allows them.

## An existing PostgreSQL

`subspace-setup` normally runs the Subspace API on the fleet VM with a
PostgreSQL container of its own. To keep the tracker on a PostgreSQL server
that already exists, such as a managed one, put its URL in a file only root
can read and give setup the file:

```sh
sudo install -m 600 /dev/null /root/fleet/database-url
sudoedit /root/fleet/database-url
# postgresql://subspace:password@db.example.com:5432/subspace?sslmode=verify-full
sudo subspace-setup --database-url-file /root/fleet/database-url --dry-run
sudo subspace-setup --database-url-file /root/fleet/database-url --migrate
```

From your machine, `./install.sh --remote USER@HOST --database-url-file FILE`
(or `install.ps1` with the same options) takes a local file that is private to
you (`chmod 600`), copies it over SSH to `/root/fleet/database-url` as root's
and passes it on. `--database-ca-file FILE` and `--migrate` work the same way.
A `--dry-run` leaves the installed fleet as it is: the host installers copy
the installer and these files to `/root/subspace-setup-dry-run`, run setup
from there and remove it, and `subspace-setup --dry-run` works on a private
copy of `/root/fleet`, so `/opt/subspace/bootstrap`, the `subspace-setup`
command, the saved answers, the database URL file and every other input stay
as the last real run left them. The one exception is a VM without
`python3`, `git`, `curl` or `nftables`: setup installs those first, saying
so, because it cannot run without them.

- **No database on the VM.** The Compose project's `postgres` service moves
  to a profile nothing starts, so there is no PostgreSQL container, no
  database volume and no lockdown rule for port 5432. The API and its agent
  listener are published on loopback as before (`127.0.0.1:3000`, and
  `127.0.0.1:3001` with the lockdown rule that admits only root, the operator
  and the officers; see [The API's agent listener](#the-apis-agent-listener)).
- **The URL stays private.** The file must be a regular file owned by root
  with mode `0600`, holding one `postgresql://` URL on one line that names the
  server's host and the database; percent-encode special characters in the
  user name and password (a single quote is refused). Setup reads it on every
  run and writes it only to the root-only `/root/fleet/api.env`, single-quoted,
  from which Compose sets the API container's `DATABASE_URL`. It never appears
  in a command line, the Compose files under `/opt/subspace/api`,
  `answers.json`, the summary or setup's output: `--dry-run` and every error
  name the file, not what it holds. As with any container's environment, root
  and members of the `docker` group can read it with `docker inspect`.
- **IRC history.** Local Ergo keeps its history in its own SQLite file on the
  VM. Setup creates no `ergo` role or database on the existing server.
- **No demo data.** `--demo` and `subspace-setup demo` are refused: demo data
  never goes into a database the installer did not create.
- `--database-url-file` runs the API on this VM, so it cannot be combined with
  `--no-api` or, on the host, `--api-target`. On a fleet set up with
  `--no-api`, it turns the API back on.
- **The dry run checks the database, read-only.** `--dry-run` connects with
  the URL as the API will, TLS and `--database-ca-file` included, in a
  read-only session, and prints what it finds: whether it connected and over
  TLS, how many of this API's migrations the database lacks, and whether the
  admin (`--admin-email` in the development `--organisation`) is already an
  administrator there, or the database has no organisation yet. It stops
  when the real run would: no connection, missing migrations without
  `--migrate`, or an admin who is not there or not an administrator.

### Migrations

The API's image migrates its database every time it starts. Against an
existing PostgreSQL the API container starts without migrating
(`bun src/main.ts`), and setup takes migrations as a separate step on every
run, in one-off API containers that publish no port:

1. `bun scripts/migrate.ts --check` reads Drizzle's migration history,
   `drizzle.__drizzle_migrations`, and changes nothing.
2. If the database lacks any of the API's migrations, an empty database
   included, setup stops before the API starts, unless this run has
   `--migrate`. Then it applies them in one transaction, under the API's
   migration lock, and starts the API only if they succeed. A failed
   migration is rolled back. After the transaction commits, the API's
   migration step also gives every person without a username one, in short
   transactions of their own; it fills in only missing usernames, and a rerun
   finishes an interrupted one.

`--migrate` applies to that run only. An upgrade whose API brings new
migrations stops at the check until you back the database up and run setup
again with `--migrate`. Before either step, setup has the fleet installer
check the fleet's configuration, credentials and pinned sessions, as a dry
run does, so a run the installer would refuse, such as one missing a Slack
token, stops before it changes the API or migrates anything.

**Privileges.** The role in the URL needs, to migrate: `CREATE` on the
database, because Drizzle creates its own `drizzle` schema for the history
table; `CREATE` on the `public` schema, which PostgreSQL 15 and later no
longer grant to everyone; and ownership of the API's tables, to alter them.
The database's owner has all three. To run without migrating it needs
`CONNECT`, `USAGE` on the `public` and `drizzle` schemas, `SELECT`, `INSERT`,
`UPDATE` and `DELETE` on the API's tables, `USAGE` and `SELECT` on their
sequences, and `SELECT` on `drizzle.__drizzle_migrations`. To keep the
running role that narrow, put the owner's URL in the file for a `--migrate`
run, then the narrow role's URL for the others, and have the owner grant the
narrow role access to new tables with `ALTER DEFAULT PRIVILEGES`.

### TLS and reaching the server

Setup passes the URL through unchanged. The API's PostgreSQL driver
(`@effect/sql-pg`) reads its `sslmode`:

| `sslmode` | Connection |
| --- | --- |
| `verify-full`, `verify-ca`, `require` | TLS, checking the certificate chain and the host name for all three (stricter than libpq's `require`). |
| `prefer`, `allow` | TLS, checked, when the server offers it, else unencrypted. |
| `disable`, or none | Unencrypted. |

Setup warns unless the URL asks for TLS. The driver ignores `sslrootcert`,
`sslcert` and `sslkey`, so client certificates are not supported. When the
server's certificate comes from a CA the VM does not trust, pass
`--database-ca-file /absolute/path/ca.pem`: setup copies it to
`/opt/subspace/database-ca.pem` (mode `0644`: a CA certificate is public),
mounts it read-only into the API container at
`/etc/subspace/database-ca.pem` and adds it to the CAs the API trusts with
`NODE_EXTRA_CA_CERTS`. A changed certificate recreates the API container;
`--database-ca-file ''` removes it.

The VM must reach the server: allow the VM's address in the server's
firewall, and its outbound route to the server's port. The lockdown does not
limit outbound connections, from the API or from anyone else on the VM.

### An existing organisation

Setup still acts through the API's development sign-in, which reaches only
development organisations (`dev:<organisation>`), never WorkOS or Entra ones.
In an existing database it signs in only as someone already there: it mints
tokens with `DEV_TOKEN_EXISTING_ONLY=true`, so `auth:dev` refuses an
`--organisation` or `--admin-email` that is not already in the database
instead of creating it, and setup then stops unless the admin is an
administrator of that organisation, before it writes anything. The one
exception is a database with no organisation at all, a new tracker: the first
sign-in creates the organisation, the admin as its administrator, its
default `local` fleet and its `Local workspace` initiative, as on the API's
own PostgreSQL.

As that administrator, setup then creates or updates, as
[Subspace API registration](#subspace-api-registration) describes:

- **the fleet** with this fleet's slug, its IRC endpoint, standard channels,
  `vmHost` (setup's `--vm-host`, which the host installers set to the
  `--remote` host) and Fleet API address. Its rear admiral and captains come
  from the agent records below: the `fleets` table's own `rear_admiral_name`
  column is a legacy one the API no longer fills. A fleet the organisation already has under
  that slug is taken over and brought in step, so give a new fleet a slug of
  its own;
- **one ship per captain**, and **an agent record** for the rear admiral and
  for each captain. A captain dropped from the roster has its record
  deactivated. The rear admiral's name and every IRC nick must be unused in
  the organisation; registration stops otherwise, possibly after creating
  the fleet's record, and a run with a new name completes it;
- **the fleet's initiative**, the one `--initiative` names, else found by the
  ID setup saved, while it belongs to no fleet or to this one, or by the
  fleet's name among initiatives that belong to no fleet or to this one, else
  created with the admin as owner and member, and assigned to the fleet (see
  "Who owns the fleet's work" under
  [Subspace API registration](#subspace-api-registration)).

It leaves everything else alone: it creates no organisation or person and
changes no one's role, and it does not touch other fleets, ships, agents,
initiatives, issues or any other record, but for an initiative `--initiative`
adopts from another fleet. With agent dispatch on, the rear
admiral's token is minted for its existing agent record. Only a `--migrate`
run changes more: the schema, and the usernames of people who had none.

The API on the VM keeps development sign-in on, on port 3000, which only root
and the operator can reach. Its sign-in route can still create development
organisations and people in the database for anyone who reaches it. If the
database's people sign in with WorkOS or Entra, configure that in
`/root/fleet/api.env` and set `ENABLE_DEV_AUTH=false`; setup then leaves the
registration to you, as it does on its own PostgreSQL.

### Upgrade, switching and uninstall

- **Upgrades keep the setting.** Setup saves the file's path in
  `answers.json` and reads the file on every run, so later runs of setup or
  of the host installer need no option. To change the URL, edit the file (or
  pass the host option again) and run setup; Compose recreates the API with
  the new value.
- **No switching.** Setup refuses `--database-url-file` on a VM whose API has
  already run with its own PostgreSQL: the tracker stays in that container's
  `subspace-api_subspace_postgres` volume, and setup does not move it. Restore
  a dump of it on the server and set up a fresh VM. Nor is there a way back to
  a PostgreSQL on the VM: `--database-url-file ''` is refused, and a run in
  the API's own mode never keeps a `DATABASE_URL` in `api.env`.
- **Nothing drops the database.** `--no-api` stops the API's containers and
  keeps the setting for when the API returns. The fleet installer's
  `uninstall` never runs Compose or connects to a database, and neither
  setup nor the fleet installer drops, truncates or deletes anything in an
  existing database. Deleting the VM leaves the database as it is; revoke the
  VM's role or change its password when you retire it.

What is verified: the start-up, the migration check and `--migrate`, a
failed migration's rollback, a runtime role with only the privileges above,
sign-in and registration in an empty and in an existing organisation, the
refusals for an unknown organisation or person, and TLS with
`sslmode=verify-full` and a private CA (refused without `--database-ca-file`),
all in Docker against a throwaway PostgreSQL 17. It has not run on a fleet VM
or against a managed PostgreSQL service.

## A hosted Subspace API

A fleet can use a Subspace API on another host, such as one an organisation
runs for several fleets, instead of one on its VM. The fleet then runs no API
container, PostgreSQL or agent listener and keeps no administrator token: the
API's administrators create the fleet there and issue it a one-time
**enrolment token** (`POST /api/v1/fleets/<fleet>/enrolment-tokens`, a
`senr1.…` value; see the API README's
[enrolment](../subspace-api/README.md#enrolment)), and Stargate does the
rest, as its `docs/subspace-agents.md` describes: it keeps one key and
certificate per officer, enrols each with the token, and signs each
officer's requests to the API with that officer's key. The API needs
`AGENT_TOKEN_AUDIENCE` set to its public origin.

```sh
sudo install -m 600 /dev/null /root/fleet/enrolment-token
sudoedit /root/fleet/enrolment-token          # senr1.…
sudo subspace-setup --api-url https://subspace.example.com \
  --api-enrolment-token-file /root/fleet/enrolment-token \
  --stargate-ref <commit at or after 5630a5c> --dry-run
```

From your machine, `./install.sh --api-url URL --api-enrolment-token-file FILE`
(or `install.ps1` with the same options) copies a file that is private to you
(`chmod 600`) to the VM for that run only and removes it afterwards, however
the run ends.

- **Options.** `--api-url` is the API's HTTPS origin, kept on later runs. It
  excludes `--database-url-file`, `--database-ca-file`, `--migrate`, `--demo`
  and `--agent-dispatch`, and, on the host, `--api-target`. There is no way
  back to an API on the VM. A fleet whose tracker is in an API on its VM, with
  its own PostgreSQL or an existing one, switches only with
  `--switch-to-hosted-api`, which leaves that tracker behind. The token file
  must be owned by root with mode `0600` and is never printed; setup keeps the
  token in `/root/fleet/fleet.secrets/subspace-enrolment-token` for the fleet
  installer, so later runs need no token.
- **A Stargate that has it.** The mode needs Stargate's `subspaceAgents`
  section, first in commit `5630a5c` and in no release yet, so this
  installer's pinned v0.43.0 has not got it: pass `--stargate-ref` with that
  commit or a later one. Setup checks every run by fetching that one commit
  and looking for `src/subspace/bridge.ts`; the fleet installer refuses its
  pinned release and stops before rendering unless the checked-out Stargate
  has that file.
- **Checks.** Before setup saves anything, every run, the dry run included,
  checks that the API answers `GET /health` and, when the run is given a
  token, checks it by posting an empty enrolment: the API refuses that for
  its body (400) only once the token is valid, which means the token's fleet
  exists, and it uses none of the token's uses. An expired, revoked or
  unknown token is reported with the API's reason. Neither request follows a
  redirect: the token goes only to the origin given. A refused run, a refused
  `--switch-to-hosted-api` included, leaves the fleet's answers and secrets
  as they were.
- **Later runs.** A run without `--api-enrolment-token-file` checks no
  token: tokens last an hour by default and a week at most, and officers
  already enrolled need none. Stargate reads the stored token only to enrol
  an officer with no enrolment yet, in the background, so an expired one
  only logs that officer's failure and never stops the gateway. The stored
  token stays (root-only, and the gateway user's copy mode `0600`) until a
  new one replaces it. To add an officer or replace a key, ask for a fresh
  token and pass it.
- **Names.** Officers enrol under the names [registration](#subspace-api-registration)
  uses: the rear admiral by its name, a captain by its IRC account with its
  name as its ship, and each with its model and IRC nick. An agent of the
  same name in another fleet is refused (409).

What the fleet installer renders, with `api.enrolment` set:

- In the gateway that runs as the gateway user, Stargate's `subspaceAgents`
  section: the API's origin, the token file
  (`/var/lib/subspace-fleet/gateway/subspace-enrolment-token`, the gateway
  user's, mode `0600`) and one entry per officer with its socket,
  `/run/subspace-fleet/agents/<officer-id>/api.sock`. Stargate enrols the
  officers in the background each time the gateway starts; an officer
  already enrolled with its key is left alone.
- Each socket's directory, mode `2750`, owned by the gateway user with the
  officer's engine group (`ss-<officer-id>`), under parents with mode `0711`.
  `/run` is emptied at boot, so `/etc/tmpfiles.d/subspace-fleet-agents.conf`
  makes them again before the gateway starts.
- For an OpenCode officer, a local MCP server named `subspace` that runs
  Stargate's bridge, `bun <stargate>/src/main.ts subspace mcp --socket
  <socket>`, as the engine's own user. A Claude Agent SDK officer, in its own
  gateway process, gets the same command as a stdio server. Neither holds a
  credential or names itself with a header.
- The `subspace_*` tool grant, the API skill, and a section in each officer's
  instructions on making REST calls with `subspace-request /api/v1/...`
  (`--method` and `--input FILE` for a body). That command,
  `/opt/subspace-fleet/bin/subspace-request`, runs Stargate's
  `subspace request` on the socket of the officer whose Linux user runs it,
  so its name stays the same when Stargate's checkout changes, and crew act
  as their captain, as through its MCP server. Every OpenCode officer's
  policy allows it (`{"action": "shell", "resource": "subspace-request *",
  "effect": "allow"}`), the rear admiral's included, unless a rule of its own
  names it.
- Nothing of the agent listener: no `X-Subspace-Agent` header, no
  `agent-header` helper and no `AGENT_PORT`. `subspace-fleetctl register`
  registers nothing, since the API's administrators keep the fleet's records.

With the fleet installer alone, set `api` to
`{"url": "https://subspace.example.com", "enrolment": true}`, `stargate.ref`
to a commit that has `subspaceAgents`, leave `apiToken` out of the secrets
file, and put the token in the secrets directory as
`subspace-enrolment-token`. The gateway user must reach the API's port (the
lockdown limits only this VM's loopback ports).

What is verified: the rendered files and refusals in the installer's tests,
and the token check against the merged API in a throwaway PostgreSQL. No
hosted API was available, so enrolment, the signing proxy and a harness
calling the tools through the bridge have not run against one.

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
administrator's `apiToken`. The installer registers the rear admiral's agent
record (rank `rearAdmiral`, in this fleet) and each captain's (rank
`captain`) whether or not dispatch is on (see
[Subspace API registration](#subspace-api-registration)); put a token for the
rear admiral's account subject in the secrets file as `rearAdmiralApiToken`.
The token needs no other scope: it is an ordinary member's token
for that agent identity, and the API limits a fleet agent's assignments and
*Ready for agents* changes to its own fleet. The installer passes it to the
gateway only through its private runtime settings, as
`SUBSPACE_DISPATCH_TOKEN`, and never prints it. Stargate reads it when the
gateway starts and has no refresh of its own, so use a long-lived token, or
renew it and re-run the installer. `subspace-setup`, on an API with
development sign-in, registers the agents first and mints the rear admiral's
token for its account subject (by default `<account>@agents.example.com`) on
each run; a development token lasts eight hours. The gateway's
Linux user must be able to reach the API: `subspace-setup` lets it, and no
agent, through to the API's main port. Agents reach the API only on its agent
listener ([The API's agent listener](#the-apis-agent-listener)).

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

### Add private repositories later

A fleet installed without its private repositories gets them once the rear
admiral and every captain have a GitHub App that can read them (a distinct
App each; the installer refuses private repositories otherwise). On a VM set
up by `subspace-setup`, as root:

1. Add each officer's `githubApp` block, as above, to its entry in
   `/root/fleet/fleet.json` (`rearAdmiral` and each of `captains`).
2. Put each officer's private key in
   `/root/fleet/fleet.secrets/<member-id>/github-app.pem`, mode `0600`, in a
   directory with mode `0700`.
3. Add the repositories to `repositories` in `/root/fleet/fleet.json`, in the
   order you want, each as `{"id": "...", "url": "https://github.com/...",
   "private": true}`. The first is each captain's crew's default and is fixed
   once a captain with crew starts.
4. Check, then apply: `sudo subspace-setup --dry-run`, then
   `sudo subspace-setup`; or, from your machine, `./install.sh --target NAME
   --yes --dry-run` and then without `--dry-run`.

A run without `--repositories` keeps the list in `fleet.json` exactly as it
is, in its order, and keeps every field added to an officer's entry by hand,
`githubApp` included; setup reads the keys from `/root/fleet/fleet.secrets/`
itself. Passing `--repositories` instead replaces the list: a repository it
names keeps its `private` flag, but a new one is public, so add private ones
by editing `fleet.json` as above. Adding repositories also gives the captains
their crew (`--crew-claude` and `--crew-codex`, two of each by default).

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

Stakeholder messaging connects only the rear admiral to Slack or Teams, with
sessions of their own; to give an officer its own Slack bot on the session
its IRC uses, see [Slack for officers](#slack-for-officers). Either transport
can be configured independently. Captains coordinate over IRC, and crew only
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

## Slack for officers

The rear admiral and any OpenCode captain can have a Slack bot of their own,
so people reach that officer in Slack as well as on IRC, in the same session.

**Create one Slack app per officer**, named after it. Follow steps 1 to 3 of
the [Slack guide](../docs/slack.md) for each app, with the officer's member
ID in place of the rear admiral's, or
[Stargate's Slack setup](https://github.com/aiconsultancy/stargate/blob/fcf6ac75d9c780878c06c8b49fb160e4a27832ff/docs/setup.md)
and its [scope list](https://github.com/aiconsultancy/stargate/blob/fcf6ac75d9c780878c06c8b49fb160e4a27832ff/docs/slack-scopes.md).
Stargate uses Socket Mode, so the VM needs no inbound port and Slack needs no
signing secret. You need the app's **App ID** (`A…`), an app-level token
(`xapp-…`) with `connections:write` and, once the app is installed, its bot
token (`xoxb-…`). Invite each bot to the channels where it should answer.

**Configure** the officer's entry in `fleet.json` (fictional IDs shown):

```json
{
  "id": "orion",
  "name": "Orion",
  "account": "example-orion",
  "slack": {
    "appId": "A0EXAMPLE01",
    "users": ["U0EXAMPLE01"],
    "channels": ["C0EXAMPLE01"]
  }
}
```

- `appId` is the officer's own app. Every bot needs a distinct app, distinct
  from `stakeholders.slack` too.
- `users` and `channels` say who may address the bot and where: explicit
  member IDs (`U…` or `W…`) and conversation IDs (`C…`, `G…`, or `D…` for a
  direct message). Both are required and must not be empty, and `*` is
  refused, so nobody can address a bot unless you list them.
- `mentionOnly` defaults to `true`: the bot starts a turn only when it is
  mentioned. Set `false` to answer every message from a listed user in a
  listed channel.

**Secrets.** Put the tokens in the officer's secrets directory, never in
`fleet.json`: `<secrets-dir>/<member-id>/slack-app-token` (`xapp-…`) and
`slack-bot-token` (`xoxb-…`), owner-only files (mode `0600`) in a directory
without group or other write access, as for the rear admiral's stakeholder
bot. The installer keeps root-only copies in
`/etc/subspace-fleet/extra-secrets.json` and gives them to the gateway only
through its private runtime settings, as `SLACK_APP_TOKEN_<ID>` and
`SLACK_BOT_TOKEN_<ID>`, where `<ID>` is the member ID in capitals with
hyphens as underscores, such as `SLACK_BOT_TOKEN_ORION`. Stargate's
configuration names only the variables. Two officers cannot share a token.
The installed copies record their app: after changing an officer's `appId`,
supply both of the new app's tokens, or the run stops.

**One session on Slack and IRC.** The installer adds the bot to the gateway
that serves the officer's IRC registration, with the same backend: engine,
agent, directory, permissions and pinned root (`backend.sessionID`). The
pinned Stargate, v0.43.0, lets one Slack bot and one IRC registration pin the
same OpenCode root
([Stargate's notes](https://github.com/aiconsultancy/stargate/blob/fcf6ac75d9c780878c06c8b49fb160e4a27832ff/docs/irc.md#one-captain-on-slack-and-irc)),
so a request on either reaches that session. Each transport keeps its own
queue and answers only the requests it received: a Slack request is answered
in Slack, an IRC request on IRC. A turn that arrives while the other
transport's turn runs waits for it, within its time limit, and crew results
return to the transport that asked. The officer's `AGENTS.md` gains a Slack
section explaining the source line (`s=slack;c=…;t=…;m=…;u=…`) that Stargate
puts at the start of each pinned Slack request.

- **A captain with crew** has its pinned root already
  ([Captain sessions](#captain-sessions)). Its bot, like its IRC
  registration, joins the gateway once the pin is rendered.
- **The rear admiral** gets a pinned root when it has its own bot, as it does
  for [agent dispatch](#agent-dispatch), in its reference checkout of the
  first repository. Its first repository is then fixed, as for a captain. In
  a fleet without repositories the root is pinned in the rear admiral's
  workspace and stays there when repositories are added, because the
  installer never moves a pinned session. Remove its bot, with dispatch off,
  and its IRC goes back to a session per conversation.
- **A captain without crew** has no pinned root, and the installer does not
  pin one for Slack, because a pin fixes its crew roster. Its bot answers,
  but Slack threads and IRC conversations keep separate sessions. Give it
  crew to share one.
- **`claude-sdk` officers** cannot have a bot: Stargate's Claude backend has
  no root to share, and their gateway runs as the officer, whose model could
  read the tokens.
- **The rear admiral's own bot replaces `stakeholders.slack`**, which keeps
  sessions of its own and reads the same token files; set one or the other.
  [Quota notices](#crew-quota) post through whichever it has. Teams keeps
  its own sessions.

`--dry-run` validates the settings and both tokens, lists each bot with its
app, allowlists, session and variable names, and refuses an officer whose
settings or tokens are incomplete. Upgrades keep the settings, and
`subspace-setup` keeps them as it keeps other fields added to a member by
hand. Without a secrets directory, an upgrade keeps the installed tokens.
Remove `slack` from an officer to remove its bot and its variables from the
gateway. Its tokens stay in the root-only copy until `--prune-secrets`;
revoke them in Slack, or delete the app.

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
| `rearAdmiral`, `captains[]` | Each has `id`, `name`, `account` and optional `channel`, `git`, `githubApp`, `harness` and `serviceTokenClientId` (its [API agent record's](#subspace-api-registration) account subject). Captains also take optional `crew`, `crewNames` and `sessionRoot`: `"workspace"` (the default for a new captain) or `"repository"`, where its sessions start; a pinned root never moves (see [Captain sessions](#captain-sessions)). IDs start with a lowercase letter and use up to 20 lowercase letters, digits and hyphens; `all` and `captain` are reserved. Keep IDs and SASL accounts unique; accounts have at most 32 characters. Choose a slug and captain IDs short enough that each [standard channel](#channels) fits 50 characters. A member's optional `channel` must be its standard channel. An empty captain array is valid; the most is 18, because the rear admiral joins every standard channel and Stargate joins at most 20. |
| Member `git` | Optional `email` and SSH `signingKey` (absolute path to the public key). Provision an agent-owned private key or an SSH agent and register its public signing key with your Git host. The rear admiral and each captain have their own Git identity and workspace under `/var/lib/subspace-fleet/agents/<id>/workspace`. Crew commit with their captain's. |
| Member `slack` | Optional, for an OpenCode officer: its own Slack bot, with `appId`, explicit `users` and `channels`, and `mentionOnly` (default `true`). Tokens go in its [secrets directory](#secrets-directory). The rear admiral's bot pins its root and replaces `stakeholders.slack`. See [Slack for officers](#slack-for-officers). |
| `harness` | Default `{"type":"opencode"}` with each role's [model and effort](#officer-models) (captains `claude-opus-5-5` at `xhigh`, the rear admiral `claude-fable-5-2` at `high`) and the [role's starter permissions](#instructions-permissions-and-toolchains). `family`, `model` and `effort` here apply to both roles; `captain` and `rearAdmiral` blocks, each with `family`, `model`, `effort` and `extraPermissions`, apply to one. Explicit OpenCode V2 rules in `permissions` replace the starter policy; rules in `extraPermissions` (fleet, role and officer, appended in that order) extend whichever applies. Each rule has `action`, `resource`, and `effect` (`allow`, `deny` or `ask`). Applies to the rear admiral and captains. |
| `captainTools` | `standard`, captains code, or `delegate`, captains with crew leave edits, commits and pushes to them and have an open shell with writes refused (see [Instructions, permissions and toolchains](#instructions-permissions-and-toolchains)). Absent means `standard`, as for a fleet installed before the setting; `subspace-setup` and `fleet.example.json` give a new fleet `delegate`. |
| `crew` | Fleet-wide `claude` and `codex` objects with `count` (0 to 10, default 2) and `model` (defaults `claude-sonnet-5-5` and `gpt-5.4`); a captain's `crew` overrides them. Crew need at least one repository and an OpenCode captain. See [crew and repositories](#crew-and-repositories). |
| `repositories` | Ordered `{"id", "url", "private"}` objects that every captain checks out, each with an optional one-line `description` (at most 200 characters) and `tracker` (at most 300) for the captains' [workspace instructions](#workspace-instructions). The ID, URL and visibility are fixed once installed; the description and tracker may change. See [crew and repositories](#crew-and-repositories). |
| `irc` | `mode` is `remote` or `local`. Remote requires `host` and uses TLS/6697 unless a different port is set. Local listens on loopback/6667 for fleet members. Optional `caFile` is an absolute, local PEM path for remote TLS. `allowedAccounts` is the explicit human SASL allowlist. `auth` is `certificate` (the remote default) or `password` (the local default and only choice); see [IRC sign-in](#irc-sign-in). |
| `irc.starbaseChannel`, `irc.commandChannel`, member `channel` | Optional, and only ever the [standard names](#channels): `#<slug>-starbase`, `#<slug>-command`, and Starbase for the rear admiral and `#<slug>-<id>` for a captain. Any other name is refused. |
| `irc.advertise` | Optional `host`, `port`, `tls` and `websocketUrl` that the API publishes. Defaults to the IRC endpoint used by the gateway. For a desktop on another machine, advertise an endpoint it can actually reach. |
| `irc.compose` | Local mode only. Defaults to the Subspace repository. Requires a full `ref` and an `operator` account. Optional `repo`, `serverName`, `tlsBind`, `tlsPort` (6697), `tlsCert`, `tlsKey`, `channels` (additional channel names to seed) and `history`: `{"database": "sqlite"}` (the default) or `{"database": "postgresql", "host": …, "port": 5432, "user": "ergo", "name": "ergo_history", "sslMode": "disable", "network": <Docker network that reaches it>}` with `ircHistoryPassword` in the secrets file. `subspace-setup` uses the API's PostgreSQL this way on a single-host fleet. |
| `stargate` | Optional `repo` and `ref`. Defaults to the [Stargate pin](#stargate-pin): Stargate's latest release when this installer was released, and never older than v0.43.0, the first with [agent dispatch](#agent-dispatch). `subspace-setup` (see [INSTALL.md](../INSTALL.md)) instead pins Stargate's newest `vX.Y.Z` release, looked up on every run. Changes must preserve the OpenCode V2 and IRC configuration contract. Optional `api` (`{"port": 8484}` by default) turns on Stargate's private Fleet API on loopback for the primary gateway; the default pin has it. Only root and `operatorUsers` can connect to it. `sudo subspace-fleetctl api-token` prints its bearer token. Its `corsOrigins` default to `subspace://*` and `https://subspace.fleet-console`, the built-in Fleet Console's origin on Windows; list your own instead if you installed the Fleet Console under another Board ID. |
| `dashboard` | `bind` defaults to `127.0.0.1`. `port` defaults to 8070. The listener proxies only the Stargate read-only dashboard socket. |
| `proxy`, `agentsview` | Loopback-only by default. Proxy `port` defaults to 8317 and `requestRetries` to 2. AgentsView `port` defaults to 8080. |
| `operatorUsers` | Existing Linux usernames that may access the local operator interfaces and model proxy. Root always has access. Defaults to an empty array. |
| `toolchains` | Mise versions. Defaults to Node 24 plus the base OS tooling. |
| `maintenance` | `mirrorCalendar` (hourly), `toolsCalendar` (weekly), `pruneCalendar` (daily), `logDays` (14) and `worktreeDays` (30). |
| `medical` | Optional [medical ship](#medical-ship) Sentry polling and IRC alerts. |
| `usage` | Optional [crew quota](#crew-quota) notices, with a Slack `reportChannel` and provider selection. The rear admiral posts them through `stakeholders.slack` or its own Slack bot, so one of them is required. |
| `api` | Optional `url` (origin or ending in `/api`), `vmHost`, `fleetApiUrl` (where a desktop reaches the Fleet API, when not through the SSH tunnel; needs `stargate.api`; see [Subspace API registration](#subspace-api-registration)) `initiatives` (array of UUIDs) and `agentUrl` (the API's agent listener on this VM's loopback, such as `http://127.0.0.1:3001`; see [The API's agent listener](#the-apis-agent-listener)), or `enrolment: true` for [a hosted Subspace API](#a-hosted-subspace-api), which excludes `agentUrl` and `apiToken`. HTTPS required except on loopback. The matching `apiToken` needs organisation administrator permissions. With it, the [registration's naming rules](#subspace-api-registration) apply to the rear admiral's name, ship names and accounts. |
| `agentDispatch` | Optional [agent dispatch](#agent-dispatch): `enabled` (default `false`), `pollIntervalSeconds` (120), `answerTimeoutSeconds` (300) and `retryLadderSeconds` (`[3600, 10800, 21600, 43200]`). On, it needs `api`, a captain with crew, an OpenCode rear admiral and `rearAdmiralApiToken`. |

The mode-0600 secrets JSON has `proxyKey` (a random proxy client key),
`ircPasswords` (map by SASL account: with `irc.auth` `password`, the rear
admiral and each captain; in local mode, also any allowlisted human; crew have
none; officers have none with [certificates](#irc-sign-in)), `ircOperatorPassword`
for local IRC, `ircHistoryPassword` for PostgreSQL IRC history, and optional
`apiToken` (bearer JWT) and `rearAdmiralApiToken` (the rear admiral agent's
bearer JWT, only with [agent dispatch](#agent-dispatch) on).
A named `fleet.json` looks for `fleet.secrets.json` beside it unless
`--secrets` is supplied. Installed copies live in `/etc/subspace-fleet/`. The
gateway receives IRC passwords or certificate bundles and its OpenCode authentication headers, plus
any explicitly enabled integration credentials. Each OpenCode engine has an
independent loopback port and server password.

### Secrets directory

Files that do not belong in either JSON file go in a secrets directory:
`fleet.secrets/` beside `fleet.json`, or `--secrets-dir DIR`. Use owner-only
regular files (mode `0600`) in directories without group or other write
access. Dispatch carries the same layout in its private bundle, and
`subspace-setup` uses `/root/fleet/fleet.secrets/`, beside its
`/root/fleet/fleet.json`. Every file is optional unless the setting in its
row is on.

| Path | Needed for | Installed as |
| --- | --- | --- |
| `<member-id>/github-app.pem` | A member's `githubApp` ([GitHub Apps](#github-apps-per-fleet-member)) | The member's `~/.config/subspace/github-app.pem`, and a root-only copy under `/etc/subspace-fleet/github-apps/` |
| `<member-id>/kubeconfig`, `<member-id>/database-dsn`, `<member-id>/cloud/<name>` | [Read-only cloud access](#read-only-cloud-access) | The member's `~/.config/subspace/cloud/` |
| `<member-id>/slack-app-token`, `<member-id>/slack-bot-token` | An officer's own [Slack bot](#slack-for-officers), or the rear admiral's `stakeholders.slack` | Root-only `/etc/subspace-fleet/extra-secrets.json`; the gateway gets them as `SLACK_APP_TOKEN_<ID>` and `SLACK_BOT_TOKEN_<ID>` (`SLACK_APP_TOKEN` and `SLACK_BOT_TOKEN` for `stakeholders.slack`) |
| `<member-id>/irc-client.pem` | An officer's existing IRC certificate and key, with `irc.auth` `certificate` ([IRC sign-in](#irc-sign-in)); without it the VM generates one | `/etc/subspace-fleet/irc-certs/<member-id>.pem`, owned by the gateway user that serves the officer |
| `medical/sentry-auth-token`, `medical/irc-password` | The [medical ship](#medical-ship) | Root-only files that systemd gives its service |
| `proxy-management-key` | [Crew quota](#crew-quota) notices | Root-only `/etc/subspace-fleet/extra-secrets.json`, and the gateway's quota watcher |
| `subspace-enrolment-token` | [A hosted Subspace API](#a-hosted-subspace-api) (`api.enrolment`): the fleet's `senr1.` enrolment token | Root-only `/etc/subspace-fleet/extra-secrets.json`, and the gateway user's `/var/lib/subspace-fleet/gateway/subspace-enrolment-token` (mode `0600`) |

Upgrades without a secrets directory keep the installed copies, and
`--prune-secrets` with a secrets directory removes the installed cloud
files, App keys and Slack tokens that it and the configuration no longer
name. An installed IRC certificate is kept with or without the directory.

`subspace-setup` keeps two more inputs of its own, outside the secrets
directory, for an [existing PostgreSQL](#an-existing-postgresql):
`/root/fleet/database-url`, root's alone with mode `0600`, and the optional
`/root/fleet/database-ca.pem`. The host installers copy them there from
`--database-url-file` and `--database-ca-file`.

### Optional settings

This fragment of a `fleet.json` adds this page's optional settings to the
example: repository notes, a captain's `sessionRoot` and Slack bots for the
rear admiral and the captain. Leave out any of them. Each bot needs its two
token files in the [secrets directory](#secrets-directory), here
`fleet.secrets/atlas/` and `fleet.secrets/orion/`. The captain's
`sessionRoot` names the default, which a captain already pinned in its first
repository cannot take. With an IRC server that cannot take
certificates, set `"auth": "password"` and add `ircPasswords` to the
secrets file. An [existing PostgreSQL](#an-existing-postgresql) for the API
is a `subspace-setup` option, not part of this file.

```json
{
  "rearAdmiral": {
    "id": "atlas", "name": "Atlas", "account": "example-atlas",
    "slack": { "appId": "A0EXAMPLE01", "users": ["U0EXAMPLE01"], "channels": ["C0EXAMPLE01"] }
  },
  "captains": [
    {
      "id": "orion", "name": "Orion", "account": "example-orion",
      "sessionRoot": "workspace",
      "slack": { "appId": "A0EXAMPLE02", "users": ["U0EXAMPLE01"], "channels": ["C0EXAMPLE02"], "mentionOnly": true }
    }
  ],
  "repositories": [
    {
      "id": "project", "url": "https://github.com/example/project.git",
      "description": "The example service.",
      "tracker": "Example issues at https://tracker.example.com/project"
    }
  ],
  "irc": { "mode": "remote", "host": "irc.example.com", "allowedAccounts": ["operator"], "auth": "certificate" }
}
```

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
registration, with its ships and officers (see
[Subspace API registration](#subspace-api-registration)). After startup it
assigns each listed initiative that belongs to no fleet, and leaves one that
another fleet has where it is, saying so: `assign` moves it. An API failure
leaves the fleet running and can be retried independently.

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

### Upgrade notes

Run the upgrade with `--dry-run` first. The installer checks every
setting together before it changes the fleet, in the dry run and the real
run alike, so when an upgrade introduces several new settings at once and
one is refused, none of them is applied.

- **IRC certificates are the default for a remote server (breaking).** A
  remote fleet whose secrets file has IRC passwords for its rear admiral or
  captains, and whose `fleet.json` does not set `irc.auth`, is refused until
  you either add `"auth": "password"` under `irc` or move to certificates
  ([IRC sign-in](#irc-sign-in)). To move without a gap, remove the
  officers' `ircPasswords` entries from your copy of the secrets file and
  supply bundles you generated yourself, dry-run to print their
  fingerprints, bind them on the server, then upgrade; "Keep passwords"
  under IRC sign-in has the steps. Local IRC keeps
  passwords. `subspace-setup` keeps passwords for a remote fleet it set up
  before, and writes `"auth": "password"` on its next run, so upgrade those
  fleets through setup (or the host installer), not with the fleet
  installer and the installed inputs alone; `--irc-auth certificate` moves
  one to certificates.
- **Captains start in their workspace.** Only new captains: a pinned root
  never moves, and an installed captain keeps its root until its
  `sessionRoot` moves it ([Captain sessions](#captain-sessions)). Asking a
  pinned captain to move is refused before anything else in the run
  applies.
- **Slack bots for officers.** A rear admiral's new bot pins its root on
  that run, which fixes its first repository; a captain's bot joins
  whatever root its IRC registration has, so a captain kept in its first
  repository is served there on both transports. Supply both tokens for
  every bot first: a missing one stops the run.
- **An existing PostgreSQL** is a `subspace-setup` setting, kept in its
  answers; it changes nothing in the fleet installer's inputs and combines
  with any of the above.
- **Captain policies.** A fleet without `captainTools` keeps the standard
  policy. Captains with crew may now read their crew's worktrees, and with a
  hosted API every officer may run `subspace-request`. A delegate fleet's
  captains trade their allowlist for the open shell with writes refused
  ([Instructions, permissions and toolchains](#instructions-permissions-and-toolchains)).
  An officer with a hand-written `harness.permissions` keeps it. To move
  captains from one to the starter policy, remove it, keep any rule you
  still need in `harness.extraPermissions`, and check the dry run's policy
  changes, as root:

  ```sh
  sudo cp -p /root/fleet/fleet.json /root/fleet/fleet.json.before-starter-policy
  sudo sh -c 'jq "del(.harness.permissions, .captains[].harness.permissions)" \
    /root/fleet/fleet.json.before-starter-policy > /root/fleet/fleet.json'
  sudo subspace-setup --dry-run
  sudo subspace-setup
  ```

  A fleet-wide `harness.permissions` applies to the rear admiral too, so
  removing it returns the rear admiral to its starter policy as well. With
  the fleet installer alone, edit your `fleet.json` the same way and run
  `sudo ./install.sh upgrade --config ~/fleet.json --dry-run`.

## Stargate pin

The installer pins Stargate by commit: `STARGATE_REF` in
[lib/runtime.py](lib/runtime.py), with the release whose tag names that
commit beside it in `STARGATE_RELEASE`. Every fleet installed with the
default `stargate` settings gets exactly that commit. The installer generates
Stargate's configuration, units and plugin options, which must match the
pinned version, so the pin only moves deliberately and after testing; it
never follows Stargate's releases on its own.

Each Subspace release carries Stargate's latest release. Three things keep it
so:

- **The check.** From the repository root,
  `python3 scripts/stargate-pin.py` reports the pin and Stargate's latest
  release, `--check` also fails when the pin is behind, and `--update` moves
  `STARGATE_RELEASE` and `STARGATE_REF` to the latest release and changes
  nothing else. It asks only GitHub's API, about `aiconsultancy/stargate`,
  with `GH_TOKEN` or `GITHUB_TOKEN` when set. It exits 0 when the pin is
  current or held, 1 when it is behind, 2 when the pin, the hold or GitHub's
  answer is malformed, 3 when GitHub cannot be reached and 4 when GitHub
  answers with an error. It never passes a pin it could not check.
- **The bump pull request.** The daily `stargate-bump` workflow
  (`.github/workflows/stargate-bump.yml`) finds a newer release, moves the
  pin, runs the installer's offline tests and opens a pull request from
  `automation/stargate-pin`, or updates the open one. Its description gives
  the releases from and to, the release notes of each release in between,
  the test results and a checklist. The workflow never merges, and leaves
  the branch alone once someone else has pushed to it.
- **The release gate.** The app's release pull request gets a
  `stargate-pin` check, started by the release workflow beside its
  `lockfiles` check, which fails while the pin is behind; see
  [Releases](../subspace-desktop/docs/ways-of-working.md#releases).

### Merge a bump

Before merging the bump pull request, check the new release against what
broke the installer before, using its release notes and Stargate's source at
the new commit:

- the configuration keys the installer generates for Stargate;
- the service, unit and path names it refers to, such as the `src/main.ts`
  and `src/fleet` entrypoints;
- the options it passes to Stargate's crew plugin;
- the Fleet API's CORS origins (`FLEET_API_ORIGINS`, `stargate.api.corsOrigins`);
- the Mesij version Stargate expects, against the `mesij` rows in
  [releases.tsv](lib/releases.tsv);
- the documentation links that name the old commit, which the pull request
  lists.

Fix anything that needs it on the same branch. Then run the
[verification](#verification) checks and install a test fleet on a VM from
the branch before merging. The workflow's token starts no `pull_request`
checks on the pull request it opens: push a commit to the branch, or close
and reopen the pull request, to start them.

### Hold a release back

When Stargate's latest release cannot be adopted yet, add
`subspace-installer/stargate-hold.json` in a reviewed pull request:

```json
{
  "pinned": "v0.43.0",
  "holding": "v0.44.0",
  "reason": "v0.44.0 renames the crew plugin's options; adopting it is tracked in #123.",
  "until": "2026-12-31"
}
```

`pinned` is the pin's release, `holding` the release not adopted yet, and
`until` the last day the hold applies. While all three match, the check and
the release gate pass with the reason as a warning, and no bump pull request
is opened. A newer Stargate release than `holding`, or the day after
`until`, ends the hold, and the check fails again until the pin moves or the
hold is renewed in another reviewed change. A hold that no longer describes
the pin is reported and ignored. Remove the file when the pin moves.

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
