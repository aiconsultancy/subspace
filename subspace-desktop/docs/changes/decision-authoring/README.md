# Author and review decisions in the desktop

## Intent

SUB-66 makes the standalone Decisions Space a place to create proposals,
update them, accept or reject them, and inspect what changed.

## Spec

- **New decision** proposes a decision in an active project. The form records
  the date, decision, context, rationale, implications, alternatives, tags
  and linked issues. Issue identifiers resolve to that project's UUIDs.
- A person can record their own decision or attribute it to another human
  maker with evidence. The server authenticates the recorder and checks
  whether the named maker and evidence are valid.
- Open a proposal to **Edit**, **Accept**, or **Reject** it. A proposal can
  replace an accepted decision through **Propose replacement**. Accepting
  the replacement supersedes the earlier decision in the same transaction.
- Writes use the version shown when the dialog opened. A conflict keeps
  the draft and loads the current record. The user reviews that record and
  opts into its version before retrying. Issue links and tags retain concurrent
  additions and removals outside the draft's changes. When the API returns
  a partial list of linked issues, saving preserves the full list.
- **Revision history** loads immutable snapshots with their author, date and
  changed fields. The Issues Space still links to decisions from the issue.

## Delivery

The desktop uses the existing decision routes. All requests run in the
background. A change of account discards the open dialog and its results.
After an unconfirmed write, the app refreshes the log and asks the user to
check it before trying again.

## Board checklist

- Identity, registration, artwork and process retention follow the existing
  Decisions registration. The displayed description now includes authoring.
- The existing workspace, shared list, text edge, reading width and dock
  settings continue to provide the navigator, tab strip and inspector.
  The toolbar owns **New decision**. Record-specific actions sit on the page.
- The toolbar, dialogs, headings and form controls use the shared dashboard
  components and GPUI Kit. No new shared component needs a gallery entry.
- The command palette offers the available actions. Escape and Cancel
  dismiss the active dialog, and the modal contract prevents shortcuts
  from reaching the underlying Space while it is open.
- Loading and failure messages cover writes, conflicts and revisions.
  Revision reads and draft writes are scoped to the existing account.
- The existing connection, status, storage, refresh, search, cross-Space
  contracts and diagnostic arrangements apply. This change adds no process,
  connection setting, background refresh, index, agent tool, chat context,
  notification, badge or persistent draft store.
- Native and HTTP fixture tests exercise the write paths. The pull request
  includes Twilight Two captures from the isolated fixture.

## Tasks

- [x] Wire creation, edits, review and replacement to the existing decision routes.
- [x] Keep drafts through stale writes and show immutable revisions.
- [x] Document the controls and their account and project requirements.
