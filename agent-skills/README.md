# Subspace skills for your local agents

These skills let the coding agents on your own computer (Claude Code, Codex
and OpenCode) work with your Subspace fleet: the issue tracker, decisions,
documents, projects and initiatives through the Subspace API's MCP server, and
the fleet's IRC channels.

| Path | What it is |
| --- | --- |
| [`subspace-irc/`](subspace-irc/SKILL.md) | Read and post in Starbase and the ship channels. The same skill works in every agent. |
| [`claude/subspace-tracker/`](claude/subspace-tracker/SKILL.md) | The tracker through MCP, for Claude Code (`mcp__subspace__*` tools; tokens refresh through `headersHelper`). |
| [`codex/subspace-tracker/`](codex/subspace-tracker/SKILL.md) | The same, for Codex (`codex mcp`, bearer token from `SUBSPACE_API_TOKEN`). |
| [`opencode/subspace-tracker/`](opencode/subspace-tracker/SKILL.md) | The same, for OpenCode (remote MCP server in `opencode.json`). |
| [`bin/subspace_token.py`](bin/subspace_token.py) | Mints and caches your API token through the fleet's tunnel. |
| [`bin/subspace_irc.py`](bin/subspace_irc.py) | A small IRC client that signs in as you and marks every line as from your agent. |

## The fleet's own agents

A fleet's rear admiral and captains get the OpenCode `subspace-tracker` skill
from the fleet installer, which also connects them to the API's MCP server at
its agent listener as their own agent records, with no login and no token
(see the installer README, "The API's agent listener"). The same skill file
serves both: it tells an agent signed in as itself to write as itself, and an
agent using a person's login to mark its text. The commands below are for
your own computer only.

The API's MCP server also states the essential rules for filing work in its
server instructions and in the issue tools' descriptions, so an agent that has
not loaded the skill still sees them: search first, one working session per
issue, a one-line summary on every issue, related issues grouped under a
tracker, and the on-behalf marker.

The API's own tests check that every tool these skills name is in the MCP
tool catalogue, that the three tracker skills share the same rules, and that
the server's instructions agree with them
(`subspace-api/tests/agent-skills.test.ts`).

## Your login, always marked

Local agents use **your** login: your API account and your IRC account. They
must always say that they are an agent acting for you, with their model and
version. The skills require it, and the IRC helper adds the marker to every
line and refuses to post without `--as`:

```text
[on behalf of George (Claude Opus 5.5)] Atlas, please ask Orion to pick up NAV-12 next.
```

In the tracker, every text they write starts with
`**On behalf of George (Claude Opus 5.5):**`. The fleet's agents take orders
from your account, so the skills also tell agents to send only what you asked
for.

## Install

From the repository root, after installing a fleet:

```sh
./install.sh skills --name "Your Name"                      # every agent found on this computer
./install.sh skills --agents claude,codex --name "Your Name"
```

On Windows, use `.\install.ps1 skills ...`. The command:

- copies the helpers to `~/.subspace/bin/` and saves your name and fleet in
  `~/.subspace/agents.env`;
- copies the skills into each agent's user profile: `~/.claude/skills/`,
  `~/.codex/skills/` (or `$CODEX_HOME/skills/`) and
  `~/.config/opencode/skills/`, replacing only `subspace-irc` and
  `subspace-tracker`;
- registers the MCP server `subspace` at `http://127.0.0.1:3000/mcp`: with
  `claude mcp add-json --scope user` (and a `headersHelper`), with `codex mcp
  add --bearer-token-env-var SUBSPACE_API_TOKEN`, and under `mcp.subspace` in
  `opencode.json`.

The helpers need Python 3. Codex and OpenCode read the token from the
environment, so start them with:

```sh
export SUBSPACE_API_TOKEN="$(python3 ~/.subspace/bin/subspace_token.py)"
```

The token lasts about eight hours. Keep the fleet's tunnel running
(`./install.sh tunnel --background`). The token comes from the API's
development sign-in. A WorkOS or Entra deployment needs a token from that
provider instead.

To remove the skills, delete those two folders from each agent's skills
folder and run `claude mcp remove --scope user subspace`, `codex mcp remove
subspace`, or delete `mcp.subspace` from `opencode.json`.
