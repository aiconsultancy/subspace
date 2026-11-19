# Spec

Documentation only; no code changes.

- `docs/board-integration.md` explains, from the code, every hook a board
  implements (`Dashboard`, `ListDashboard`, `BoardRegistration`) and
  everything the Framework gives it (`Services`, `prefs` and caches, refresh
  policy, search, agent tools, chat context, notifications, help, links,
  logging, test fixtures). Each item says what it does, when to use it, how,
  an example board and the tests. It ends with an audit matrix of the current
  boards and a list of gaps, IRC first, with implementation outlines for
  persisting IRC history and indexing IRC messages.
- "Communicating with other boards" in `docs/board-integration.md` lists,
  from the code, every path between boards and between a board and the host
  (`DashboardEvent`, shell-called hooks, shared globals, search navigation,
  search references, notification click-through, the NETT client hub
  snapshot, agent tools), says plainly that no declared publish and subscribe
  mechanism exists, explains how to use each path, and sets out a proposal,
  marked as not implemented, for declared, typed and versioned board events
  routed by the host, with candidate first events. Events are notifications
  only; data stays in the framework store (search service), which events
  refer to by `ReferenceIdentity` and document ID. The "Data caching and
  refresh" and "Search" sections are left to the IRC change, which adds the
  board-facing store API. The audit gains rows for
  shared state and declared events; the checklist gains an item for them.
- `docs/board-checklist.md` is the short checklist a board change copies into
  its record and pull request, with each item linked to its explanation.
- `.github/pull_request_template.md` at the repository root holds the pull
  request checklist. `docs/ways-of-working.md` repeats it under "Before
  opening or merging a pull request", including the board checklist item.
- The documentation index, AGENTS.md (beside "Adding a dashboard"), the
  developer overview's "Add a dashboard" and the board guide template link to
  the new pages.
