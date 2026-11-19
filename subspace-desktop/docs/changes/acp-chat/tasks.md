# Tasks: ACP agent chat

- [x] `ChatEngine::Acp`, `AcpAgent`, `AcpConfig`, `chat.acp` parsing and
      saving, `ACP_CHAT` experiment.
- [x] Shared child-process transport (`runtime/child.rs`) for Pi and ACP.
- [x] ACP runtime: presets, setup, sign-in retry, prompt turns, streaming,
      tool calls, plans, permissions, cancel, model options, slash commands,
      session load, tabs, library, export of open chats.
- [x] Panel: engine name per agent, Plug icon in the strip and toolbar,
      guidance, remembered model; composer placeholder takes the name.
- [x] Settings > Chat engine and Agent choices; Experiments toggle.
- [x] Pure protocol, transcript and launch tests; fake shell agent covering
      setup, streaming, tool call, permission allow and deny, refused `fs`
      request, cancel, model confirmation, session replay and early exit.
- [x] `acp_chat` example; manual runs against OpenCode, Codex, Claude Code and
      Gemini CLI.
- [x] Docs: [ACP agent chat](../../acp-chat.md), agent chat, feature flags.
- [ ] Follow-ups: agent modes; steering while busy; respawn an exited agent
      in place; a read-only or approval-gated MCP mode for agents whose
      permission rules allow MCP writes.
- [x] The status bar names the selected engine ("OpenCode (ACP) ready").
