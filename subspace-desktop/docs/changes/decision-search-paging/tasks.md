# Decisions search and paging tasks

- [x] Route exact and partial references through the API's identifier field.
- [x] Preserve ordinary text search, account boundaries and existing filters.
- [x] Read a cursor page at a time, including initiative-scoped decisions.
- [x] Preserve the list while a search or page request is pending.
- [x] Check paging across more than 5,000 API records and document the controls.

### Board checklist — Decisions (`decisions`)

| Area | Integration |
| --- | --- |
| Identity, registration, artwork and retention | Uses the existing Decisions registration and artwork. |
| Workspace, lists, docks and text edge | Uses the shared workspace, existing ListDashboard list and the shared footer actions. The navigator, pane tabs and inspector follow the workspace guide. |
| Components and gallery | The footer and loading indicator use the existing action button, toolbar label and GPUI Kit Spinner. |
| Status, keys, palette, menus, empty state and quit | Search and paging participate in the existing busy state. Previous and Next are reachable by Tab. Refresh and Retry use the existing account-aware read. |
| Settings, flags and service | Uses the signed-in Subspace API connection and the existing preferences for filters. |
| Data, storage and refresh | The cursor history and visible page live for the current account; no persistence is needed. Refresh opens the first page. |
| Search and references | Decision references and text use the API's existing search query. Existing linked-issue navigation and reference contracts apply. |
| Agent tools, CLI, chat, notifications and diagnostics | This work changes desktop navigation and read filters. The same list endpoint serves other existing callers. |
| Help, tests and evidence | In-app and developer docs describe the controls. Scope and account tests, the isolated API test and the native captures cover the changes. |
