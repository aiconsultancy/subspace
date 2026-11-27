# Tasks: nest issues and attach files

- [x] Open the existing New issue form from the Sub-issues panel and show the saved child.
- [x] Reconcile projected children on refresh without paging through the tracker.
- [x] Replace free-text Subspace API Parent fields with the shared searchable menu.
- [x] Exclude the issue itself on older servers and show parent refusals without entering version-conflict recovery or losing the draft.
- [x] Use the server's `parentFor` filter and existing save-time parent rule.
- [x] Keep NETT parent editing and the versioned save process intact.
- [x] Integrate **Add file**, **Save** and **Remove** only when `removal=available`.
- [x] Explain linked rows, reload on a removal `409`, and omit **Remove** on older servers.
- [x] Report the attachment removal refusal and reload without listing possible causes in the notice.
- [x] Block removal from the attachment panel while a local comment draft holds the file.
- [x] Exercise linked and available removals in the isolated fixture server.
- [x] Run the scoped desktop checks and fixture tests.
- [x] Run the full project gates through the pre-push hook (push7, `6c5af7e1`).
- [x] Capture screenshots of the changed states.
- [ ] Complete both independent reviews.

## Board checklist: `issues`

### Identity and registration

- [x] ID, title, description, icon, artwork, descriptor, registration and retention policy are unchanged.

### Layout and UI

- [x] The existing Workspace, text edge, reading width, docks and view menu are unchanged.
- [x] Existing list hooks and account, status, badge and busy surfaces are unchanged.
- [x] Uses the shared action button, anchored searchable menu and section treatment; no new shared component or catalogue card is needed.
- [x] **New sub-issue** and **Add file** are keyboard-reachable actions on the open issue. Existing editor keys perform save and dismissal. The attachment panel keeps per-row actions and explains linked rows.
- [x] Menu dismissal uses the existing board anchors; loading, no results and retry are shown in the picker.
- [x] Quit warnings use the existing draft protection. Initial prefill alone is not an edit.
- [x] Compare screenshots, including the narrow layout, with the design guide.

### Settings and configuration

- [x] Settings, connection and notifications choices are unchanged. Parent search is limited to the Subspace API.

### Data caching and refresh

- [x] Stored choices and caching are unchanged. The open page temporarily retains newly created children beyond the loaded pages, rechecking their eligibility on refresh. Picker reads do not replace the Issues list. Attachment transfers stay bound to the open issue, and its file list reloads after changes or a stale removal conflict.

### Search, agent surfaces and context

- [x] Item search, Opened, full-text indexing and references are unchanged; picker search is scoped to choosing a parent.
- [x] No new MCP, CLI or app tools or changes to the issue context sent to chat.

### Notifications and attention

- [x] No new global notices or badges. The attachment panel shows upload progress, retryable failures and removal eligibility in place.

### Help and docs

- [x] The [Issues guide](../../issues.md) describes sub-issues, parent selection and standalone attachment controls and limits. Its in-app help links remain in place.

### Communication and diagnostics

- [x] No new contracts, Framework globals, links, diagnostic configuration or logging.

### Testing and evidence

- [x] Loopback API and headless UI coverage for sub-issues, parent selection and standalone attachments.
- [x] Evidence captured from isolated fixture builds.
