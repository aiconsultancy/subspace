# Intent: ACP agent chat

Let the chat panel talk to OpenCode, Claude Code, Codex and Gemini CLI
through one integration, the [Agent Client Protocol](https://agentclientprotocol.com)
(ACP), instead of one adapter per agent. The goal is a working engine the
owner can use and iterate on, not a finished one: streamed replies, tool
calls, permission requests on the existing approval card, Stop, the model
picker, and Subspace's board tools over MCP.

ACP sits beside Copilot and Pi as a third engine behind its own experiment,
and reuses Pi's shape: one external process per chat tab, speaking
newline-delimited JSON-RPC on its standard streams, mapped onto the runtime's
existing command and event contract so the panel needs no new UI.
