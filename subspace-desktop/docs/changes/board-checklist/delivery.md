# Delivery

Documentation only; Subspace's interface is unchanged, so there are no
screenshots.

## Checks

- `python3 scripts/check-docs-index.py` passes: the index reaches the new
  pages and every local link and heading fragment resolves.
- `python3 scripts/check-board-docs.py` passes with the edited guide template.
- `make check` passes (run as `MISE_AUTO_INSTALL=0 COPILOT_SKIP_CLI_DOWNLOAD=1
  mise exec -- make check`).

## Sources

The catalogue and audit were read from `framework/board-kit/dashboard/src/lib.rs`,
`list.rs`, `prefs.rs`, `refresh.rs`, `notifications.rs`, `alerts.rs`,
`search_adapter.rs`, `prefetch.rs`, `retained.rs` and `settings.rs`;
`framework/host/app/src/registry.rs` and `registry/configured.rs`;
`framework/services/agent-tools/src/lib.rs`; `framework/services/search/src`;
`products/subspace/src/main.rs`; and each board's `Dashboard` and
`ListDashboard` implementations under `spaces/`.

The communication section was read from `framework/host/app/src/app.rs`
(`dashboard_subscriptions`, `on_dashboard_event`, `open_notification`),
`app/chat.rs`, `app/search.rs`, `app/session.rs`, `notifications/runtime.rs`
and `registry/configured.rs`; `framework/board-kit/dashboard/src/handle.rs`,
`chat_sessions.rs`, `repositories.rs` and `rich.rs`; `spaces/github/src/favourites.rs`,
`spaces/actions/src/load.rs`, `spaces/irc/src/hub.rs` and `compose.rs`,
`spaces/github-copilot-agents/src/dashboard.rs`, `spaces/web/src/bridge/protocol.rs`;
`clients/nett/src/hub.rs`; and the issue workspace implementation spec. The section's docs-index
check was run again after it was added; `make check` was not re-run for this
documentation-only addition.

## Found during the survey

- `docs/logging.md` documents `subspace_dashboard::logger::<Self>()`, which
  does not exist.
- `docs/search.md` says Copilot Agents is opted out of search; it sets
  `SEARCHABLE` and indexes session logs.
- Board Lab shows the list keys in its key help although it is not a list
  board.
- `ChatRequest::OpenInSubspace` and `ChatRequest::RefreshVisibleBoard` are
  handled by the shell but emitted by no board.
- The NETT hub's `irc_snapshot()` and `irc_version()` have no consumer on this
  branch.
