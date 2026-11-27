# Crew worktree diffs

Back to the [change records](../README.md).

## Intent

Reuse the PRs Space’s file tree and unified and split diff viewer for changes
that a crew member has not pushed to GitHub.

## Spec

The `open-crew-diff` navigation request identifies the fleet, ship, crew member
and project. The PRs Space resolves the fleet connection and displays **All
changes**, **Committed only** or **Uncommitted only**. It reads file-list pages
for one snapshot; **Load next page** continues a long patch at that snapshot’s
fingerprint. The toolbar shows the base ref, short base commit and short head,
or says that the diff starts from an empty tree when there is no base. File
headers label untracked files, submodules and symlinks. A submodule without a
patch has its own note, not a binary reader. The shared Fleet feed can also
report a new fingerprint for that checkout. A change observed during a load or after it can mark the
snapshot stale; the feed value already known when the load started cannot.
The viewer checks for changes again when the Space becomes active. A conflict
or missing list continuation also asks for **Refresh**. If a list or file
page reports an unstable read, the viewer still displays the content and
shows **Changed while loading · Refresh**. Closing or refreshing cancels the
previous file-list load. **Refresh** keeps the displayed files, selection and
scroll position until the new snapshot for the same mode is ready. While it
runs, no new file, byte or preview read starts. Reads already in flight can
finish against the displayed fingerprint; an unchanged refresh keeps their
results and retry countdowns. If the refresh fails temporarily, the diff stays
visible with an inline **Retry**. Replacing or clearing the snapshot cancels
outstanding reads. A mode or access change, or a confirmed unavailable diff,
clears the previous result. A stale snapshot cancels file, byte and preview
reads but leaves already loaded content visible.
Busy requests retry
the same page up to twice, respecting a numeric `Retry-After`. If the wait is
over ten seconds or the retries run out, **Retry page** repeats the failed file,
binary or preview page without reloading the file list or discarding loaded
pages. A positive `Retry-After` disables that control until the countdown
ends. The pinned file side records the deadline before any automatic retry
wait, so closing the reader or switching sides does not reset it, even if a
byte or preview read was still retrying. Before each binary request, including
every automatic retry and preview continuation, the reader checks the same
pinned deadline and waits for the longer of it and the reply's wait. If the
wait would exceed the ten-second budget, the pending action stays on
**Retry page**. A late reply
can start or extend the visible side's countdown even after a replacement
reader has loaded. **Preview image** and **Retry page** are unavailable until
the countdown ends. Hidden sides keep only a deadline. A different fingerprint
or diff mode starts a new read. A stale snapshot or a failed file-list load
still uses **Refresh**.

**View binary** opens a readonly hex and ASCII dump. Its pages are read on
demand at the original fingerprint; modified binaries offer **Old** and
**New**, and removals show **Old**. PNG, JPEG, GIF and WebP files also offer
**Preview image**, which assembles the selected side in memory and shows a
still preview for up to 10 MiB in at most 16 pages, with a maximum width
and height of 4,096 pixels. **Show bytes** remains
available; larger or unsupported images can be read as bytes page by page.
Leaving or replacing a preview removes its texture from the window atlas.

The shared unified and split lists keep their fixed-height virtual rows.
**Read full line** opens a horizontally scrollable reader for a line from
either layout, including lines without spaces. Tabs show as `⇥` and carriage
returns as `␍`. The virtual list sizes its horizontal extent from one
measurement item; this avoids a guessed width that could still clip one of
the split columns, or measuring the whole diff.

The Fleet client does not put diff content in the activity feed. The viewer
keeps it in memory only, with no viewed marks, search index or notification
content. The PRs Space’s GitHub path keeps its own account, links and marks.

## Delivery and follow-up

- This change adds the Fleet reader and native navigation responder. The
  Channels control that calls it is a separate change.
- Fictional HTTP and GPUI fixtures cover paging, stale snapshots and opening
  the view without a GitHub account. The capture Fleet API serves Wren’s
  paged chart diff, Talon’s clean checkout and Corvin’s unavailable checkout.
  The capture-only `link:` step opens the native PRs viewer without the
  separate Channels control.
- Progressive file-list display and a shared patch cache budget are SUB-67
  follow-ups, deferred from this change under STAR decision `01c2e208`.
- An empty HTTP 404 means either the checkout is unavailable or the gateway
  predates the route. A successful empty snapshot means no changes, including
  on an unborn branch.

## Screenshot evidence

Linux native captures use the fictional Aurora Fleet API. Wren’s six-file
tree combines two successful file-list requests.

- Wren: [first patch page](evidence/01-text-first.png),
  [continued patch](evidence/02-text-complete.png),
  [full-line reader with a leading `⇥`](evidence/03-full-line.png) and
  [scrolled to the closing `";` and `␍`](evidence/03-full-line-end.png).
- Wren: [new PNG preview](evidence/04-png-new.png) and
  [old PNG preview](evidence/05-png-old.png).
- Wren: removed binary, [first hex page](evidence/06-hex-first.png) and
  [last hex page](evidence/07-hex-last.png).
- Talon: [no changed files](evidence/08-empty.png) (HTTP 200).
- Corvin: [diff unavailable](evidence/09-unavailable.png) (HTTP 404).
- Wren: disabled **Retry page** during the countdown for a
  [text continuation](evidence/10-text-retry.png) and a
  [binary continuation](evidence/11-binary-retry.png). The loaded patch and
  bytes remain visible while the Fleet API asks the viewer to wait.

## Board checklist: `github`

- **Registration and layout:** reuses the PRs registration, file tree and
  diff layout. No new Space, artwork, dock, inspector or process. The mode
  control and refresh action use existing board-kit components and have
  palette entries. Status shows the member and file count; the viewer retains
  its file navigation and layout keys and omits viewed-mark keys from help.
- **Settings and refresh:** no new settings, credential storage or feature
  switches. The Fleet service owns the connection and the viewer follows its
  existing event feed while visible. A refresh reads in the background.
- **Storage, search and chat:** unified/split remains a viewer preference.
  Diff content stays in memory. File-tree search is local; Fleet diffs are
  not history entries, searchable documents, PR subjects or agent context.
- **Tools and notifications:** no added tools, CLI, notification rules,
  badges or work that needs a quit warning.
- **Navigation and diagnostics:** the audit lists the broker responder.
  Navigation and Back carry IDs only; errors omit paths and diff content.
- **Verification:** fixture tests cover HTTP paging, event decoding and
  opening the viewer through its route. The native captures above show the
  paged diff, readers and distinct empty states.
