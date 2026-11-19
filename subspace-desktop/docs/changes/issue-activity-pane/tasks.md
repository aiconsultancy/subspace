# Board checklist for Issue Activity

- [x] Activity stays in the registered Issues Space and uses the common tab,
  list rows, controls and theme tokens. Its section state is board-owned.
- [x] Toolbar team scope, Reload and busy status follow the visible activity.
  The palette and keyboard include the new tab, and a selected row can
  open, retry or load more. Existing menu and dismissal behavior applies.
- [x] Each section has loading, empty and independent retry states. Paging
  commands and the issue viewer remain read only. No new quit warnings,
  settings, connection fields or feature flags apply.
- [x] The grouped list retains its own selected issue and existing choices.
  The hub retains Activity in memory within its account and team scope.
  The tab never sends background refreshes.
- [x] Activity does not add local search indexing, chat attachments,
  notifications, agent tools, MCP or CLI commands.
- [x] Help covers the Activity tab. The NETT hub guide documents the
  reads and its privacy boundary. No board contract or publish path is added.
- [x] Diagnostics identify the type of read without issue bodies, actor
  information, cursors or credentials.
- [x] Shared fixtures cover distinct identifiers, activity kinds and actors,
  selection and opening, and cursor failure and retry. macOS capture evidence
  is listed in [delivery.md](delivery.md).
- [ ] Live NETT dev verification after the Activity read rollout deploys, including events
  produced by the document and attachment writers as they become available.
