# IRC history and full-text search: tasks

Back to the [change records](../README.md).

## Work

- [x] Design note in [spec.md](spec.md) before code.
- [x] Search service: `records`, `groups`, `remove_matching`,
      `index_metadata_batch`, `offer_default`; schema version 3
      (`settings.chosen`); tests in `tests/cases/records.rs` and
      `migration.rs`.
- [x] `BoardSettings::store_by_default`, offered by the host at search start;
      test in `search_runtime.rs`.
- [x] Client: `Anchor`, `Command::Anchors`, `Command::LoadAround`,
      `Event::Backfill`; bounded `LATEST` on join and for stored private
      conversations after registration; `AROUND`; pending requests answered
      in order; anchors follow every message; tests in
      `session/tests/anchors.rs` and the Ergo test `ergo/anchors.rs`.
- [x] Board: records (`store/mapping.rs`), GitHub references
      (`store/links.rs`), ordered writes, deletes and restore (`store.rs`,
      `hub/persist.rs`), merge and dedupe (`state/merge.rs`), restore before
      the runtime starts, anchors after it.
- [x] `SEARCHABLE`, `open_indexed_result_fenced` (`reveal.rs`), line
      selection in the transcript.
- [x] `irc_read search` over held lines and the store.
- [x] Removal deletes a buffer's records.
- [x] `/` item search and `Opened`, palette commands, chat context,
      diagnostic config path.
- [x] Settings > IRC **History** note; Search view copy "Choose per board".
- [x] Docs: [IRC](../../irc.md), the board guide, [Full-text search](../../search.md),
      [What the Framework gives a board](../../board-integration.md) (pattern,
      board storage, audit, gaps), [board checklist](../../board-checklist.md).
- [x] Screenshots (see [delivery](delivery.md)).

## Board checklist: irc

Identity and registration
- [x] ID, title, description and icon are final: unchanged.
- [x] Product descriptor and `BoardRegistration`: unchanged; artwork unchanged.
- [x] `RETAIN_WHEN_DISABLED`: not set; the hub owns connections and stops with the board.

Layout and UI
- [x] List board hooks: unchanged (the buffer list); conversations in the dock.
- [x] Toolbar, status, badge and busy: unchanged; restored lines never change the badge.
- [x] Palette commands for every toolbar and menu action: Join channel…, Connect/Disconnect, Mark … as read, Leave/Remove/Close (`commands.rs`).
- [x] Own keys: unchanged.
- [x] Menus and dialogs reported with `modal`: unchanged.
- [x] Empty, loading and error states: a search result the server no longer has says so in a notice.
- [x] `quit_warnings`: not applicable; quitting only disconnects, and pending writes are flushed on stop.

Settings and configuration
- [x] `settings()`: description unchanged; notifications on; `store_by_default` on.
- [x] Own config file and editor: unchanged, plus the **History** note.
- [x] Feature flags: not applicable; the Search source switch is the control.

Data caching and refresh
- [x] Stored choices: unchanged (dock layout, removals).
- [x] Persists its data in the framework store: every channel and private message (`store.rs`).
- [x] Loaded data shown from the store first, then refreshed: restore before the runtime, then bounded `LATEST` after the anchor.
- [x] Automatic refresh: not applicable (live connection); store calls run on the background executor.

Search
- [x] Item search (`/`) and `Opened`: `rows::search`, `open_search_result`, `on_activate`.
- [x] Full-text index: `SEARCHABLE`, one local scope, documents, `open_indexed_result_fenced`.
- [x] Exact cross-board references: `mentioned-pull-request` and `mentioned-issue` via `index_metadata_batch`.

Agent tools, MCP and CLI
- [x] Read tool and write tool: `irc_read search` now also reads the store; `irc_write` unchanged.
- [x] App-only tools: unchanged.
- [x] Tools and actions listed in docs/mcp.md: unchanged actions.

Chat context
- [x] `agent_context`: the conversation in front with its 40 newest messages, marked as other people's text, capped at 16 KB (`context.rs`).

Notifications and attention
- [x] `notifications::request`: unchanged; restored and backfilled lines never notify.
- [x] Notices from process-owned work: unchanged.

Help and docs
- [x] `help.md`: Search messages, Keep or clear message history, settings, troubleshooting and limits; no images.
- [x] Guide and developer page linked: unchanged links.

Communicating with other boards
- [x] Contracts: not applicable to this change; relations go through the search references, not a contract.
- [x] Framework state it writes or follows: the search service through `Services::search`; no other board's crate or state.

Links, logging and diagnostics
- [x] Links: unchanged; search citations route through `SearchNavigation` to `open_indexed_result_fenced`.
- [x] Store failures logged at warn without message text; `diagnostic_config_path` returns `irc/config.json`.

Testing and evidence
- [x] Pure logic tested: mapping, links, merge and dedupe, anchors, palette offers, chat context, item search; hub and board with a real temporary store; Ergo catch-up.
- [x] Screenshots from the capture build with fictional data from a local Ergo server.
