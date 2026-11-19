# Create and edit issues

People with issue-manage permission can create a NETT issue and edit an issue
they can open. **New issue** appears in the issue list and **Edit issue** in
the record header. Both are also Commands. They open the same form, using
the board’s current profile, organisation and team catalogues. The form
covers the title, Markdown description, workflow state, priority, assignee,
labels, due date, project, milestone, event and parent. New issues can
choose a team. A tracker's state stays calculated from its children, and
an event-linked issue takes its project from the event.

The board holds the draft in memory. A write goes to the same NETT
organisation and session that opened the form. Edits send only changed
properties and the loaded issue version; a conflict preserves the draft and
offers to read and adopt the latest version before resubmitting. Label
additions and removals apply to the latest label set. Opening an issue tile
from IRC returns to any open draft. A create
uses a stable draft identifier, so an uncertain retry cannot create a second
issue. NETT reports if it already consumed the draft; in that case the board
refreshes the list. Confirmed saves refresh the list and issue view.

The service contract is the issue POST and PATCH under
`/api/v1/orgs/{orgId}/issues`, as implemented in Indra main
`fdc1b63fdffdeabd34f9f3cf49c43cdd2e08dba9`. The
`mediaDraftId` create fence is independent of the file transfer.
Attachment and inline image authoring remains a follow-up once
the common attachment upload path lands. Existing image reads work as before.

## macOS evidence

The macOS captures were [removed for publication](../public-repository-examples/README.md).

Automated coverage uses local fixtures for permission gating, native
keyboard submission, edit conflicts, sparse clears and create retries.
No test issues were written to NETT dev.

### Board checklist: issues

- [x] Identity and registration: unchanged; board does not own processes.
- [x] Layout and UI: the native form is a board view, with focused input,
  board-scoped Escape and submit, Commands, tracked picker menus, loading and
  retry notices, shared action and field controls, and quit warnings for
  unsaved edits or an active save.
- [x] Settings and configuration: board Settings description updated.
  Sign-in, connections, notifications and refresh choices are unchanged.
- [x] Data caching and refresh: form and choice loads require a current
  session, run outside the UI thread, and stay in memory. A successful save
  refreshes through existing list and detail reads.
- [x] Search: the existing opened-issue search and cross-board references are
  unchanged. Drafts are never indexed.
- [x] Agent tools, MCP and CLI: no new tools; saves are explicit user actions.
- [x] Chat context: drafts, read data and credentials are never attached.
- [x] Notifications and attention: no new notifications or badges. Existing
  toolbar busy state includes saving and reading the latest issue.
- [x] Help: the linked Issues guide documents the form and its keyboard flow.
- [x] Communicating with other boards: no added contract or board dependency.
- [x] Links, logging and diagnostics: existing bounded NETT transport and
  typed errors; no body, credentials or private draft content is logged.
- [x] Testing and evidence: contract and headless interaction checks plus
  native macOS fixture captures above.
