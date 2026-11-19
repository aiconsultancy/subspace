# Board checklist

Complete this list for every new board and for every change that adds or
changes a board's integration with the Framework. A Space is a board, so it
applies to every Space. Copy it into the change
record (`docs/changes/<slug>/tasks.md` or `spec.md`) and the pull request.
Tick each item and say how it is implemented, or write "not applicable
because …". An unticked item with no reason is not ready for review. Each item
links to its explanation in [What the Framework gives a board](board-integration.md).

```markdown
### Board checklist: <board id>

Identity and registration
- [ ] ID, title, description and icon are final (ID renames break state, tools and search)
- [ ] Product descriptor and `BoardRegistration` added; artwork declared with `ARTWORK`
- [ ] `RETAIN_WHEN_DISABLED`: set only if the board owns processes

Layout and UI
- [ ] List board hooks (`ListDashboard`), or the reason for a custom layout
- [ ] Components follow the rules in docs/ui-components.md: GPUI Kit or shared components only; any new shared component has a Component gallery card
- [ ] Toolbar, status, badge and busy
- [ ] Palette commands for every toolbar and menu action
- [ ] Own keys bound in `view_keys(ID)`; `keys` and `help_foot` match the board
- [ ] Menus and dialogs reported with `modal` and closed by `dismiss`
- [ ] Empty, loading and error states with Configure or Retry
- [ ] `quit_warnings` if quitting ends something the person started

Settings and configuration
- [ ] `settings()` description; notifications and background updates opt-ins decided
- [ ] Service connections (`connection_settings`) or the board's own config file and editor
- [ ] Feature flags for experiments

Data caching and refresh
- [ ] Stored choices in `prefs` survive a restart
- [ ] Persists its data in the framework store (board storage API), or not applicable because …
- [ ] Loaded data shown from the store first, then refreshed
- [ ] Automatic refresh honours `refresh::enabled` and `interval`; nothing blocks the UI thread

Search
- [ ] Item search (`/`) and `Opened` for recent items
- [ ] Full-text index (`SEARCHABLE`, verified scope, documents, `open_indexed_result`)
- [ ] Exact cross-board references recorded with `index_metadata`

Agent tools, MCP and CLI
- [ ] Read tool, and write tool with per-call approval, or "no tools because …"
- [ ] App-only tools built only for `ToolHost::App`
- [ ] Tools and actions listed in docs/mcp.md

Chat context
- [ ] `agent_context` attaches what the detail shows, bounded and without credentials

Notifications and attention
- [ ] `notifications::request` with `Notice::described`, defaults and `open_notification`
- [ ] Notices raised from process-owned work, paired with a badge

Help and docs
- [ ] `help.md` follows the board guide template; images considered
- [ ] Guide and developer page linked from docs/README.md and docs/subspace/boards.md

Communicating with other boards
- [ ] Contracts it publishes / receives — declared in `contracts()`, receivers and responders attached, listed in Declared contracts — or not applicable because …
- [ ] Framework state it writes or follows (globals); no dependency on another board's crate, globals or stored choices

Links, logging and diagnostics
- [ ] URLs opened through `links`; citations through `SearchNavigation`
- [ ] Writes and failures logged without content or secrets; `diagnostic_config_path` if the board has a config file

Testing and evidence
- [ ] Pure logic tested; interaction and layout invariants tested headless
- [ ] Screenshots from the capture build with fixture data
```

| Item | Explained in |
| --- | --- |
| Identity, registration, artwork, retain | [Identity and registration](board-integration.md#identity-and-registration) |
| Lists, toolbar, palette, keys, menus, quit warnings | [Layout and UI](board-integration.md#layout-and-ui) |
| Settings page, connections, config file, features | [Settings and configuration](board-integration.md#settings-and-configuration) |
| Stored choices, caches, refresh | [Data caching and refresh](board-integration.md#data-caching-and-refresh) |
| Item search, full-text index, references | [Search](board-integration.md#search) |
| Agent tools, MCP, CLI | [Agent tools, MCP and CLI](board-integration.md#agent-tools-mcp-and-cli) |
| Chat context and chat requests | [Chat context](board-integration.md#chat-context) |
| Notifications and badges | [Notifications and attention](board-integration.md#notifications-and-attention) |
| Help guide and images | [Help and docs](board-integration.md#help-and-docs) |
| Contracts published and received, shared state | [Communicating with other boards](board-integration.md#communicating-with-other-boards) |
| Links and references | [Links and navigation](board-integration.md#links-and-navigation) |
| Logging and diagnostics | [Logging and diagnostics](board-integration.md#logging-and-diagnostics) |
| Tests and screenshots | [Testing and evidence](board-integration.md#testing-and-evidence) |

An app can keep an audit of how its boards answer each item.
