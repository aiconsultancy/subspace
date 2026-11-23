# Fleet agents use the Subspace API as themselves

## Intent

The owner's requirement: captains must be able to reach the Subspace API,
which is its whole point. They need full access and must identify themselves
as agents, but never administrator access. They need the API's MCP server
and generic skills on how to use the API through it. The installer adds the
skills and the MCP configuration, and none of it may require WorkOS or Entra.

Before this change the agents' Linux users were kept away from the API,
because on a fleet VM development sign-in trusts anyone who reaches the API's
port to act as anyone, administrator included. Only the gateway's user
reached it, with an eight-hour rear admiral token.

## Specification

- **API: an agent-only listener** (`subspace-api/src/agent-listener.ts`,
  `api.ts`, `config.ts`). It is optional (`AGENT_PORT`), loopback or
  local-only container publishing, and never in production. A request names
  its agent in `X-Subspace-Agent`. The API resolves the name, by agent name or
  IRC nick and without regard to case, to an active agent record of the one
  organisation in `AGENT_ORGANISATION` and provides it as the current member,
  so every existing rule for agents applies. Unknown, ambiguous and human
  names and a missing header receive 401, and a deactivated agent 403.
  Development sign-in, the GitHub webhook, `/openapi.json`, the administrator
  routes and the administrator MCP tools are not served. Both listeners share
  one database pool, update hub and storage.
- **API: fleet initiatives.** Initiative membership only decides
  `/me/initiatives`; every other read and write is open to any active member
  of the organisation. A registered agent now counts as a member of the
  initiatives that belong to its fleet, by their own fleet or their nearest
  ancestor's, so it finds its fleet's work with no membership granted by hand.
  The fleet limits on dispatch, Ready for agents and moves are unchanged.
- **Installer.** `subspace-setup` turns the listener on at `127.0.0.1:3001`
  for the development sign-in organisation and keeps port 3000 and PostgreSQL
  operator-only, so the agents' users reach port 3001 only. The audit fails
  if port 3001 answers the sign-in route or shares an operator port. Each
  OpenCode officer's engine gets the MCP server `subspace` with its IRC
  account in the header and a `subspace_*` permission grant; Claude Agent SDK
  officers get it through Stargate's `mcpServers` and a header helper, which
  permits its tools, and the skill's text in their system prompt. Crew
  share their captain's server. Officers receive the `subspace-tracker` skill.
- **Skills.** The three `subspace-tracker` skills were rewritten from the
  API's tool catalogue and rules and share one body. An API test checks that
  every tool they name exists, that none of them teaches an administrator
  tool, and that the shared body matches.

## Trust

Anyone who can reach the agent port can act as any registered agent of its
organisation: the trust development sign-in already places in the network,
minus administrator access. The owner accepts that agents can act as each
other for now.

## Verification

- API: `bun run typecheck`, `bun run lint`, `bun test`. The agent listener's
  routing, refusals, MCP tool list, crew relay and configuration are covered
  in-process with a stub database. Its tests against PostgreSQL
  (`tests/agent-listener.test.ts`, "the agent listener over HTTP") need a
  Compose stack with the agent port and were not run.
- Installer: `python3 -B` on `policy_test.py`, `setup_test.py`,
  `sessions_test.py` and `medical_test.py`; `bash -n` and `shellcheck` on the
  changed scripts.

## Not verified

- Nothing ran on a fleet VM: the listener in the API container, the
  published port, the lockdown and audit against real nftables, the engines'
  MCP connection and the skill on disk. The container smoke test's
  `check_shape` asserts the engine and skill but was not run.
- OpenCode's remote MCP `headers` field in the configuration shape the fleet
  pins.
- Whether Stargate's curated crew permissions let crew call `subspace_*`
  tools; they allow only listed actions.
- A further fleet installed with `--api-target` gets no agent listener.
