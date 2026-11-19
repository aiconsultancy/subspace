# Spec: ACP agent chat

## Configuration

- `chat.engine: "acp"` selects `ChatEngine::Acp`, gated by the experimental
  feature `app.acp-chat` (`ACP_CHAT`), off by default.
- `chat.acp` (`AcpConfig`): `agent` (`opencode` default, `claude`, `codex`,
  `gemini`, `custom`), optional `command`, `args` and `env`. Invalid values
  warn and keep the preset.
- Settings > Chat lists **ACP agent (Experimental)** as an engine and shows an
  **Agent** select only while it is selected. Save writes `chat.acp.agent`
  and keeps the section's other keys. Turning the experiment off while ACP is
  selected resets the draft engine to Copilot.

## Runtime (`framework/runtimes/agent/src/runtime/acp*`)

- Hand-rolled JSON-RPC 2.0 over `serde_json::Value`, like Pi's protocol, not
  the `agent-client-protocol` crate: the crate had five major versions in
  2026 and its connection types assume their own async dispatch, while the
  runtime already owns a tokio actor, bounded stdio transport and event queue.
  Subspace needs about ten messages.
- `child.rs` now holds the stdio transport and child environment shared by Pi
  and ACP (service tokens removed, the program's folder first in `PATH`).
- Presets resolve through `subspace_core::path::find_tool`: `opencode acp`;
  `claude-agent-acp` or `npx -y @agentclientprotocol/claude-agent-acp`;
  `codex-acp` or `npx -y @agentclientprotocol/codex-acp`; `gemini --acp`.
- Per tab: spawn, `initialize` (protocol 1; no `fs` or `terminal`
  capability), then `session/new` or, for a saved tab when the agent supports
  it, `session/load` with replay folded into `History`. Both pass
  `mcpServers: [{name: "subspace", command: <current exe>, args: ["mcp"]}]`.
  On `-32000` (auth required) the chat calls `authenticate` once with the
  first advertised method, then retries.
- `session/prompt` carries the prepared prompt text and any images. Updates
  map to rows: message and thought chunks stream as deltas and are confirmed
  as one row when another kind of update starts (so ordering survives the
  panel's upsert), `tool_call`/`tool_call_update` become tool rows keyed by
  call id, `plan` becomes one Info row per turn. `available_commands_update`,
  `config_option_update` and `session_info_update` feed the slash list, model
  picker and title.
- `session/request_permission` goes through `Decisions::approval` (the
  approval card); Allow picks `allow_once` then `allow_always`, Deny
  `reject_once` then `reject_always`; Stop answers `cancelled`. Other agent
  requests get `-32601`.
- Stop sends `session/cancel`; the prompt's `cancelled` answer ends the turn
  and marks running calls aborted.
- Model choice uses the `model` category session config option
  (`session/set_config_option`), and `thought_level` as efforts.
- Tabs and the library are stored per agent in `acp-<agent>-tabs.json` and
  `acp-<agent>-library.json`; the agent runs in `acp-workspace`.
