# Tasks: IRC agent tools

- [x] Add `ToolHost` to `ToolEnvironment`; the app builds `App`, and headless
      routes build `Headless`.
- [x] Add a `Command::Say` client command for literal text and `/me` actions.
- [x] Add the hub bridge: open at start, close at stop, answer on each drain.
- [x] Add `irc_read` with `networks`, `buffers`, `messages` (paging and server
      history), `members` and `search`.
- [x] Add `irc_write` with `message`, `join`, `part`, `connect` and
      `disconnect`: validation, live checks, the approval text, info logs and
      at most 10 operations.
- [x] Register the tools through `Dashboard::agent_tools`.
- [x] Add unit, hub and Ergo integration tests.
- [x] Update `docs/irc.md`, the guide, `docs/mcp.md`, `docs/cli.md` and
      `docs/agent-chat.md`.
- [x] Make the interfaces fixture check that no headless `irc` commands exist.
- [x] Capture Settings › MCP before and after in Cupertino Dark.
- [ ] Follow-up: give ACP chat sessions the app's own MCP server, so they also
      get the app-only tools.
