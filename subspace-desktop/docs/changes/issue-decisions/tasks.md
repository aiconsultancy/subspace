# Issue decisions delivery checklist

Back to the [change record](intent.md).

- [x] Add the issue-local Decisions tab and connect both openers to the panel.
- [x] Finish decision query, projection and read-error handling in the client.
- [x] Cover paging, refresh, stale reads, navigation, focus and 700 px layout.
- [x] Document the behaviour, limitations and verification.
- [x] Capture the tab, the panel and the empty, no-access and narrow states
      with `make capture` and the Decisions fixture.
- [ ] Embed the captures in the pull request with commit-pinned URLs.

## Board checklist: Issues

| Area | Check |
| --- | --- |
| Identity and registration | Unchanged: board ID, description, icon, product descriptor, registration and retention. |
| Layout and UI | Uses the existing Activity heading and Kit tab, action, focus trap and panel styling. Loading, empty, unavailable and retryable states have separate text. Status has a text label. No board toolbar or quit warning changes. |
| Keys, palette and modal | Adds issue-local tab focus and **Show issue decisions**. **Show all issue activity** also focuses its tab when switching from **Decisions**. The panel isolates focus and pointer input and preserves the existing Escape and Back layering. |
| Settings and configuration | No new setting, connection or flag. Uses the existing Subspace API service. |
| Caching and refresh | Memory-only and issue-scoped. Worker reads are lazy and generation-fenced; a retryable refresh failure retains the last successful rows. |
| Search, agent tools, MCP and CLI | No new surface. |
| Chat context and attention | No automatic chat context, notifications or new attention state. |
| Help and docs | The [Issues guide](../../issues.md), the Space's `help.md` and this change record describe the feature. |
| Other boards | No contract changes. The existing first-origin Back behaviour is retained. |
| Links, logging and diagnostics | Issue links use existing navigation. Markdown uses the shared safe-link policy. Decision text, evidence and people names are not added to Debug output. |
| Tests and evidence | Loopback and headless GPUI tests cover behaviour. Native captures are on the [delivery page](delivery.md); the keyboard walkthrough is covered by a headless test only. |
