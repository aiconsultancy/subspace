# Specification: nest issues and attach files

- A writable parent with Subspace API sub-issue support offers **New sub-issue**
  in its Sub-issues panel, including an empty tracker. The existing New issue
  form starts with that issue's project and parent. If the person saves under
  that parent, the page stays open and shows the new row without a manual
  refresh, even when the child falls after the first page. If a refresh still
  has more pages but doesn't include the child, read it once by ID. Keep its
  current fields if it is still an unarchived child of this tracker. Drop
  a child that is no longer eligible or accessible. A temporary read failure
  keeps the last known fields and allows another check on the next refresh.
- The New issue, Edit issue and inline Parent controls use the same anchored
  picker. It sends `q`, `type=tracker` and `parentFor` (the edited issue or
  `new`), uses the server's cursor pages and offers **No parent**. Search
  matches key, title, summary or description. The API's shared parent rule
  excludes self, descendants and archived or cyclic ancestry. The client also
  excludes self when older servers ignore `parentFor`. The API still validates
  the current graph and issue version. A refused parent returns `409` with
  `errors.parentId`; the editor keeps the draft and asks for another parent
  instead of opening version-conflict recovery. NETT keeps its existing text field.
- In the Subspace **Attachments** panel, **Add file** uploads a standalone
  file and **Save** downloads its bytes. **Remove** appears only when the API
  reports `removal=available`. Linked rows explain why removal is not offered;
  archived rows and older servers without `removal` do not offer **Remove**.
  If removal returns `409`, keep the file and reload the list so the row
  reflects its current state. The notice reports the refusal and reload
  without guessing why removal was refused.
  Files held by this client's open comment drafts on the issue cannot be
  removed from the panel. The draft owns the file through upload and recovery;
  posting hands protection to the server's link check, and discarding uses
  draft cleanup. Other clients' unposted drafts remain outside this protection.
- The Subspace API change is additive: the attachment list's optional
  `removal` can be `available`, `linked` or `archived`. The server checks the
  issue description, live comments and all issue document revisions before
  deleting. `/attachments` accepts validated raster images as files under
  the same 10 MB limit as other files and serves file bytes with
  `Content-Disposition: attachment` and `X-Content-Type-Options: nosniff`.
  Inline and comment images still use `/images`. See the
  [Issues guide](../../issues.md) for NETT's separate upload flow.
- NETT also re-reads the attachment list after a successful removal. A
  refused file pick shows NETT's `InvalidImage` message, with its image and
  file limits, rather than the generic file message.
