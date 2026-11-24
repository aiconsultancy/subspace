# Working in the @@NAME@@ deployment repository

This repository holds @@NAME@@'s Subspace configuration and deployment only.
Read `README.md` first.

## Rules

- **Never commit a secret**: no token, password, private key, certificate,
  database URL with a password, enrolment token or OAuth client secret.
  Secrets live in the cloud's secret store, a password manager or on the
  fleet VM; refer to them here by name and location. Public IDs (API
  addresses, sign-in client IDs, tenant and subscription IDs, repository
  names) may be committed.
- **Configuration and deployment only.** Change Subspace or Stargate
  upstream, then move the pin in `pins.json` (`scripts/update-pins.py`). Do
  not copy their source in.
- **The agent kit is copied, never edited**: `.claude/skills/`,
  `.agents/skills/subspace-*` and `agent-kit/bin/` come from the Subspace pin
  through `scripts/agent-kit.py sync`, and CI fails when they differ.
- **Deploy only what is pinned and reviewed.** Run the installer from a
  Subspace checkout at the pinned commit, with the Stargate pin, after a dry
  run. Ask a person before anything hard to undo or outward-facing.

## The tracker and the fleet

- Use the Subspace MCP server (`subspace`, registered in `.mcp.json`,
  `.codex/config.toml` and `opencode.json`) and follow the
  `subspace-tracker` skill: call `subspace_whoami` first; as a person's
  agent, start everything you write with
  `**On behalf of <Name> (<your model and version>):**`; search before you
  create; one issue per working session. If the server is missing or answers
  401, the person runs the set-up in the README; never ask for a password or
  a code.
- Talk to the fleet with the `subspace-irc` skill. You speak with the
  person's account, which the fleet's agents obey: post only what they asked
  for or approved, with the `--as` marker, and never a secret.
- Record decisions in the tracker with `subspace_log_decision`, with the
  actual maker and their evidence, and link the issue they belong to.
