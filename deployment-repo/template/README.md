# @@NAME@@ deployment

Everything specific to how @@NAME@@ runs [Subspace](https://github.com/aiconsultancy/subspace):
which releases it deploys, its fleet's definition, its hosting and deployment
code, its desktop build settings and its runbooks. The application itself is
never copied here; it is fetched at the commits pinned in `pins.json`. This
repository never holds a secret.

| Path | What it is |
| --- | --- |
| `pins.json` | The Subspace and Stargate releases this deployment runs, each with the commit it resolves to |
| `fleet/` | The fleet's definition, without secrets, and the exact installer commands |
| `hosting/` | Cloud resources and deployment code: the VM, the private network, a shared IRC server, a hosted API |
| `desktop/` | Settings for the desktop app or for your own desktop build |
| `runbooks/` | How to operate, upgrade and recover this deployment |
| `.claude/skills/`, `.agents/skills/`, `agent-kit/bin/` | The Subspace skills and helpers for local agents, copied from the Subspace pin |
| `.mcp.json`, `.codex/config.toml`, `opencode.json` | The Subspace API's MCP server for Claude Code, Codex and OpenCode in this repository |
| `agent-kit/agent.json` | The API's address and sign-in client ID (public settings, not secrets) |
| `scripts/` | `update-pins.py`, `agent-kit.py` and its tests |

## Working with the fleet from your own machine

Coding agents started in a clone of this repository can use the Subspace
tracker (issues, decisions, documents) through the API's MCP server and talk
to the fleet's rear admiral and captains on IRC. They do it **as you**, with
your own sign-in, and say so on everything they write:
`[on behalf of <you> (<model and version>)]` on IRC and
`**On behalf of <you> (<model and version>):**` in the tracker. The fleet's
agents take orders from people's accounts, so your agents send only what you
asked them to send.

Set up once per computer (macOS or Linux, Python 3.9 or later, on the
private network that reaches the API and IRC):

```sh
python3 scripts/agent-kit.py setup --name "Your Name" --dry-run
python3 scripts/agent-kit.py setup --name "Your Name"
python3 ~/.subspace/bin/subspace_token.py --login     # once, in your browser
python3 ~/.subspace/bin/subspace_token.py --me        # check who you are
```

Then start Claude Code here and allow the `subspace` server from `.mcp.json`
when it asks; in Codex, trust this repository when it asks. Sessions that
were already running see the skills and the server after they restart.

## Updating

Bump the pins, sync, review, deploy:

```sh
python3 scripts/update-pins.py --dry-run     # what the latest releases are
python3 scripts/update-pins.py               # move both pins (or --subspace TAG|SHA, --stargate TAG|SHA)
python3 scripts/agent-kit.py sync            # copy the skills and helpers at the new Subspace pin
```

Read the upstream changes the first command links, commit the pins and the
synced copies together, and let CI check them. Then deploy from the pins as
`runbooks/` describes, and ask everyone to run
`python3 scripts/agent-kit.py setup` again to refresh their helpers. See
[Your deployment repository](https://github.com/aiconsultancy/subspace/blob/main/docs/deployment-repository.md)
in Subspace for the whole routine.
