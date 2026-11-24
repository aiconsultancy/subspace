# Your deployment repository

When you run Subspace for yourself or your organisation, keep everything
that is yours in one private repository of your own, and take Subspace and
Stargate as pinned releases from their public repositories. Your repository
then says exactly what you run and how, people review every change to it,
and everyone who clones it, with their coding agents, can work with your
fleet from their own machine.

Create it with the scaffold, from a checkout of this repository:

```sh
python3 deployment-repo/scaffold.py --dir ../example-deployment --name "Example Org" \
  --api-url https://api.example.com --workos-client-id client_EXAMPLE --dry-run
```

Drop `--dry-run` to create it. Leave out `--api-url` and
`--workos-client-id` if you have no hosted Subspace API. The scaffold pins
the latest Subspace and Stargate releases unless you pass `--subspace` or
`--stargate` (a release tag, or a full commit SHA on `main`). It warns when
the pinned Subspace commit is too old for local agents to sign in to a
hosted API; pin a later commit on `main` until a release has it. It runs
`git init` and never commits, pushes or creates a remote repository: you do
that, for example with `gh repo create OWNER/NAME --private --source DIR`
after a first commit.

## What goes in it

| Path | What it holds |
| --- | --- |
| `pins.json` | The Subspace and Stargate releases you deploy: repository, tag pattern, release and the commit it resolved to |
| `fleet/` | The fleet's definition without secrets (a copy of the VM's `/root/fleet/fleet.json`) and the exact installer commands |
| `hosting/` | Cloud resources and deployment code: the VM, the private network, a shared IRC server, a hosted API, DNS, CI |
| `desktop/` | How your people get the desktop app, or the build settings for your own build |
| `runbooks/` | How to operate, update and recover the deployment |
| `.claude/skills/`, `.agents/skills/` | The Subspace skills for Claude Code and Codex, copied from the Subspace pin |
| `agent-kit/bin/` | The token and IRC helpers, copied from the Subspace pin |
| `.mcp.json`, `.codex/config.toml`, `opencode.json` | The Subspace API's MCP server for Claude Code, Codex (trusted projects) and OpenCode in this repository |
| `agent-kit/agent.json` | The API's address and WorkOS client ID: public settings, not secrets |
| `scripts/` | `update-pins.py`, `agent-kit.py` (sync, check, setup) and the kit's tests |
| `.github/workflows/agent-kit.yml` | Fails when a copied file differs from the Subspace pin |
| `.gitignore` | Keeps keys, certificates, tokens, `.env` files and Terraform state out |

**It never holds a secret.** Passwords, tokens, private keys, enrolment
tokens, database URLs and OAuth client secrets stay in your cloud's secret
store, a password manager or on the fleet VM, which generates its own. Write
down where each one lives, not what it is.

**It never holds Subspace's or Stargate's source.** Change them upstream,
then move a pin. The skills and helpers in the agent kit are the one
exception: exact copies, synced from the pin and checked in CI, so that
agents find them in a clone.

## Working with the fleet from your own machine

Everyone who clones the repository runs, once per computer:

```sh
python3 scripts/agent-kit.py setup --name "Your Name"
python3 ~/.subspace/bin/subspace_token.py --login
```

`setup` installs the helpers the skills run in `~/.subspace/bin/`, writes
`~/.subspace/agent.json` from `agent-kit/agent.json` (it will not replace
another API's settings without `--force`) and saves the name their agents
act for. Claude Code then offers the `subspace` server from `.mcp.json`, and
Codex loads `.codex/config.toml` once the repository is trusted. Their
agents can use the tracker and decisions and talk to the rear admiral and
captains on IRC, always marked as acting for that person (see
[the agent skills](../agent-skills/README.md)).

## Updating: bump the pins, sync, review, deploy

1. **Bump the pins.** `python3 scripts/update-pins.py --dry-run` shows the
   latest releases and links each comparison; `python3 scripts/update-pins.py`
   moves both pins, or one with `--subspace TAG|SHA` or `--stargate TAG|SHA`.
2. **Sync.** `python3 scripts/agent-kit.py sync` copies the skills and
   helpers at the new Subspace pin.
3. **Review.** Read the upstream changes and release notes, commit the pins
   and the synced copies together in a pull request, and let CI check them.
4. **Deploy.** From a checkout of Subspace at the pinned commit, run the
   installer as `fleet/` records it, with `--stargate-ref` set to the
   Stargate pin: dry run first, then the real run. Build images, the hosted
   API and the desktop from the same commits. Then ask everyone to run
   `python3 scripts/agent-kit.py setup` again so their helpers match.

Pins only move forward through review, so what is deployed is always
something the repository says, and going back is reverting a commit.
