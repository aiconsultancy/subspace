# ACP agent chat engine

The ACP engine runs an external coding agent through the
[Agent Client Protocol](https://agentclientprotocol.com) (ACP), so the chat
panel can talk to OpenCode, Claude Code, Codex, Gemini CLI or any other ACP
agent through one integration. Enable the experiment to use it. New
configurations start with chat off.

Enable **ACP agent chat** in **Settings > Experiments**, then choose
**Settings > Chat > Chat engine > ACP agent (Experimental)** and pick the
**Agent**. Save and restart Subspace. The same choices in `config.json`:

```json
{
  "chat": {
    "engine": "acp",
    "acp": { "agent": "opencode" }
  },
  "features": {
    "app.acp-chat": true
  }
}
```

Turning the experiment off while ACP is selected changes the draft engine to
Copilot, with an explanation. A hand-edited `"engine": "acp"` without the
opt-in cannot start an agent and says how to correct the settings.

## Agents

| `chat.acp.agent` | Shown as | Mark | Runs |
| --- | --- | --- | --- |
| `opencode` (default) | OpenCode | OpenCode | `opencode acp` |
| `claude` | Claude | Claude | `claude-agent-acp` when installed, else `npx -y @agentclientprotocol/claude-agent-acp` |
| `codex` | Codex | OpenAI | `codex-acp` when installed, else `npx -y @agentclientprotocol/codex-acp` |
| `gemini` | Gemini | Gemini | `gemini --acp` |
| `custom` | ACP agent | Lucide Plug | `chat.acp.command` |

The chat names the agent by this short name and shows its mark on the
toolbar's **Chat** button, in the chat header and before its state in the
status bar, such as "Codex ready". **Settings > Chat > Agent** keeps the full
product names: Claude Code and Gemini CLI.

Subspace finds each program through its shared tool lookup (`PATH`, the
remembered path, then the usual install folders). The first `npx` start
downloads the adapter, so it can take a minute.

Optional overrides in `chat.acp`:

- `command`: the program to run instead of the preset's. `custom` requires it.
- `args`: the arguments to pass instead of the preset's. Without `args`, a
  `command` override keeps the preset's arguments, so
  `{"agent":"opencode","command":"/opt/opencode/bin/opencode"}` still runs
  `acp`.
- `env`: extra environment variables for the agent, as `{"NAME":"value"}`.
  Values of names containing `KEY`, `TOKEN`, `SECRET` or `PASSWORD` are
  redacted from chat.

```json
{
  "chat": {
    "engine": "acp",
    "acp": {
      "agent": "custom",
      "command": "my-agent",
      "args": ["--acp"],
      "env": { "MY_AGENT_PROFILE": "work" }
    }
  }
}
```

## Sign-in

Each agent uses its own sign-in: `opencode auth login`, `claude` then
`/login`, `codex login`, or `gemini`. Subspace stores no agent credentials.
When an agent reports that it needs sign-in, Subspace calls ACP
`authenticate` once with the agent's first advertised method, which reuses an
existing CLI login, and then retries. If that fails, the chat shows the
agent's message and its sign-in instruction.

## What works

- Each chat tab runs its own agent process with one ACP session, as Pi does.
  **New chat** starts a new session. Closing a tab stops its process.
- Replies and thinking stream into the transcript. Tool calls appear as tool
  rows whose status follows the agent's updates (pending, running, done,
  failed). Plans appear as one **Plan** row that updates in place.
- **Stop** sends `session/cancel` and answers waiting agent permission
  requests as cancelled. Calls that never finished are marked as stopped.
  If the prompt response does not arrive within three seconds, Subspace ends
  the turn locally and replaces that tab's agent process. It reloads the
  session when the agent supports it, otherwise starts a fresh agent session
  while keeping the displayed transcript. A message sent during replacement
  waits for the agent to be ready. Late output from the old process is ignored.
  If the agent exits during Stop, the same recovery runs immediately. If a
  replacement cannot start or complete setup, the conversation stays in the
  tab with an error; sending another message retries the replacement and then
  delivers that message. Supported model and reasoning choices are restored
  and confirmed before the replacement accepts queued input.
- The model picker lists the agent's `model` session option and, when it
  offers one, its reasoning levels. Subspace remembers the last confirmed
  model in `chat.default-model.acp` and applies it to new chats when the agent
  offers it.
- The agent's slash commands appear in the `/` list and are sent as
  `/name arguments`.
- Saved tabs reopen after a restart through `session/load` when the agent
  supports it; the agent replays the history. Saved chats are listed per
  agent. Export works for chats open in a tab.
- Images are sent when the agent accepts them.
- If an agent exits, its tab reports the exit with the end of its diagnostic
  output. Open a new chat to start it again.

## Approvals and board tools

The agent keeps its own tools, sandbox and permission rules. When it asks
permission (`session/request_permission`), the request appears on the chat's
approval card. **Allow** answers with the agent's one-time allow option (or
its lasting one when it has no one-time option); **Deny** answers with its
reject option. Stop answers a waiting request as cancelled. Subspace's
**Approve all** default and `/yolo` apply only to Copilot.
Each permission request has its own local identity, so a delayed answer
cannot approve a later request that reuses the same ACP request ID.

Each session gets the running app's own tools as an MCP server named
`subspace`: the same in-app tools Copilot and Pi chat use, including the IRC
tools when the Channels Space runs, which act on its live connections. When a
session opens, the app serves them to that session alone on a Streamable HTTP
endpoint at `http://127.0.0.1:<ephemeral port>/mcp` that needs a random
per-session bearer token, and stops it when the tab closes. This is separate
from the app server in **Settings > MCP**: its switch, port, token and tool
groups apply to external clients only.

- An agent that advertises `mcpCapabilities.http` (Claude Code, Codex,
  OpenCode and Gemini CLI do) receives an `http` server with the endpoint's
  URL and an `Authorization` header.
- Any other agent receives a stdio server, `<this app> mcp --attach <URL>`
  with the token in `SUBSPACE_MCP_TOKEN`, which relays to the same endpoint.
  It serves no catalog of its own.

**Every board write asks on the chat's approval card**, as for Copilot and Pi,
and a write whose summary looks like a credential is refused. The token goes
only into `session/new` and `session/load`; Subspace treats it as a secret and
never logs it or shows it in the transcript. An agent may also ask its own
permission before calling an MCP tool, so a write can show two cards: the
agent's, then Subspace's, which describes the exact operations. An agent's
own tool-call timeout (Codex defaults to 60 seconds) can end a call that waits
for approval longer than that.

### Session context

Each session also tells the agent what Subspace serves: the enabled boards
with their purpose, the tools of each served group with their read or write
access and actions, the shared tools, and the rules for writes. It is the
same text as the session endpoint's MCP `instructions` (see
[MCP server](mcp.md#protocol)), built from the same catalog, so it lists the
in-app tools of the active boards.

- An agent that advertises `sessionCapabilities.systemPrompt` in `initialize`
  receives it as `systemPrompt` in append mode on `session/new` and
  `session/load`. Current Claude Code (`claude-agent-acp` 0.92 or later) and
  Codex (`codex-acp` 2.3 or later) adapters do.
- Any other agent, such as OpenCode and Gemini CLI, receives it once, as a
  text block marked `<subspace-context>` before the first message of a new
  session. A reopened session is not sent it again, and a replayed history
  hides the block.

The tools and context reflect the active boards when the chat runtime
started; **Reconnect** picks up later changes. Set the log level to `debug` to see the
text each session received (`acp session context sent`, target `agent`).

Subspace advertises no file system or terminal capability: requests for
`fs/*` or `terminal/*` are refused, and the agent uses its own tools. Agents
start in Subspace's private `acp-workspace` folder beside `state.json`.

Every unsupported agent request, including an unknown extension method,
receives a JSON-RPC method-not-found error with its original request ID.
Unknown `session/update` kinds and other notifications (including warnings)
are ignored. The response to `session/prompt` ends the turn for every stop
reason, including an unknown reason, and for JSON-RPC errors.
Transcript chunks arriving after that response are ignored so they cannot
restart the completed turn's busy status.

## Not yet supported

- Agent modes and other session options than model and reasoning level.
- Copilot custom agents, Plan/Autopilot modes, YOLO, canvases and cloud-agent
  replay.
- Sending a message while the agent is still answering: wait or press Stop.
- Exporting a saved chat that is not open, and deleting the agent's own copy
  of a deleted chat.
- ACP's draft version 2 and its unstable extensions.

## Check an agent without the app

The `acp_chat` example drives the real runtime and prints what the panel
receives. `ACP_APPROVE=1` allows permission requests; `ACP_AGENT_HOME` sets
the agent's `HOME` so Subspace's own state can live in a throwaway `HOME`:

```sh
cargo run -p subspace-agent --example acp_chat -- opencode "Say hello"
```

The example has no app, so its sessions get no `subspace` MCP server and run
without board tools.

See [agent chat](agent-chat.md) for shared context and service-write behavior.
