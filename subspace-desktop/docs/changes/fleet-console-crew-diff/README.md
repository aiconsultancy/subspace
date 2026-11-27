# Fleet Console: crew diffs

## Intent

People see everything agents see (SUB-70), including what a crew member has
changed in its checkout but not pushed. Stargate now serves each crew
member's worktree diff (STAR-5). This adds **View diff** to each project on
a crew member's inspector card in Fleet Console. It opens a plain web viewer
for that worktree, built against the STAR-5 contract with fictional
fixtures and a sample checkout, so it works without a gateway. The desktop
viewer in the PRs Space (#110) is separate work; this view shows what that
one shows, and the differences are listed below with their reasons.

## Spec

- **Gate.** View diff shows on a project row only when the fleet gateway's
  feature list has `api.crewDiffs` with `enabled: true`, and the project's
  `git` is present with no `error`. An older gateway lists no such feature
  and shows nothing new.
- **Routes.** `src/api.js` adds `diff(ship, member, {project, mode,
  page})` for `GET /api/v1/ships/{id}/crew/{member}/diff` and
  `diffFile(ship, member, {project, mode, path, page, at, side})` for
  `…/diff/file`. Only the parameters given are sent; the default project
  sends no `project`. A list page body is capped at 400 MiB and a file page
  body at 7 MiB before decoding, as #110. Error bodies are empty and never
  read.
- **Status codes.** 400: "Stargate refused the request as malformed (400)."
  404 on the first list page: "Diff not available", as an older gateway and
  a member with no checkout answer alike. 404 on a later list page, 409 and
  a page at another fingerprint: "The checkout changed while loading.
  Refresh to try again." 404 on a file page: "This file is no longer
  available. Refresh the diff." 500: Git or the crew plugin failed. 503
  retries the same request up to twice after `Retry-After` (one second
  when it sends none); a wait over ten seconds, or a third 503, stops with
  "Stargate is busy. Wait N seconds, then select Refresh." (or **Retry
  page** for a page). A successful empty snapshot is "No changes.",
  including on an unborn branch.
- **List paging.** `src/diff.js` `loadDiffList` reads every page before
  showing the list, sending each page's opaque `nextPage` back as `?page=`,
  as STAR-5 names them (Stargate #380). Each later page must have the first
  page's fingerprint (else stale) and the same ship, member, project, mode
  and total, and add files; a repeated token, a token after `total` files
  or a list that does not add up to `total` is an invalid response. Nothing
  is cut. Each page is cut down to its rows' metadata as it arrives: the
  list keeps every row but none of the inline patch text, so a 17-page list
  of 1,088 files (68 MiB of inline patches) holds no patch byte outside the
  budget.
- **Patch parts.** Each file's patch is one `createPager` (`src/pager.js`,
  through `src/diff-parts.js`), keyed by the snapshot's fingerprint, the
  mode and the row's place in the list, not its path, as two rows can share
  a display path (SUB decision 75c3ed93 §1). A part is one gateway
  patch page, all from one sequence: the file route's first page,
  `…/diff/file?path=&at=<fingerprint>`, read when the patch is first shown,
  then each page its `patchNextPage` names, read with `&page=`. The list's
  inline patch (its first 64 KiB) is never shown or followed: the file
  route's first page holds up to 1 MiB, so the two sequences' pages end in
  different places. One part shows at a time, with **First part**,
  **Previous part** and **Next part**, and **Last part** once the count is
  known (a page naming no next page is the last); Last reads only the last
  page, by its remembered cursor. A patch with no next page is one part,
  with no part controls. Each answer must be at `at`, for the same path,
  carry patch text and not point back at the page asked for. The part on
  show stays on show until its replacement arrives; a failed read keeps it
  and offers **Retry page** for the same page.
- **Rows sharing a display path.** Git can show two byte paths as one
  display path, and the file route answers a path with the first file that
  has it, so it cannot address either row on its own (STAR-5). No read by
  such a path is made. A shared row's patch is one part: its inline patch,
  read again from the list page that listed it, by that page's token (the
  first page with none), which must be at the snapshot's fingerprint (else
  stale) and list the same path in the same place. If the patch runs on, the
  view says plainly that the gateway cannot address the file yet and the
  rest cannot be read here; a shared binary row says its bytes cannot be
  read here, with no bytes or preview buttons. Another file's bytes never
  show.
- **Binary files.** **Old bytes** and **New bytes** (an added file only
  New, a removed one only Old) open a hex and ASCII view: a 16-digit offset
  in the file, 8 bytes a row and `.` for bytes outside printable ASCII, as
  #110, shown 4 KiB at a time. Each side is one pager (75c3ed93 §2): a part
  is one gateway binary page, read by `?side=` from the token the page
  before gave, at the snapshot's fingerprint, and held as its base64; one
  window is decoded at a time. **First**, **Previous** and **Next** move a
  window; past a part's first or last window they read the part before or
  after, Previous landing on its last window. **Last** shows only once the
  last page's token is known, so it never reads the pages between. Each
  page must be for the side asked for, `encoding: "base64"`, start at the
  offset the previous one ended at, give the same size, decode to at most
  1 MiB and not run past the size. A page ending before the size must name
  a next page and hold bytes; a page ending exactly at the size is the
  last, even if it names a next page. An empty file is one page of size 0.
  A failed page keeps the part on show and offers **Retry page**. PNG,
  JPEG, GIF and WebP paths also offer **Preview image** for each side,
  which assembles the side in memory (at most 10 MiB in 16 pages) and
  shows it through a `blob:` URL, refusing images over 4,096 pixels a side;
  **Show bytes** stays available. Submodules and symbolic links say what
  their patch shows; a link is never followed.
- **One budget for the console.** The crew diff view and the Activity view
  share one 64 MiB `createBudget` (`src/budget.js`; 75c3ed93 §3), each
  through its own `scope`: the same room, order and latest request, with
  keys kept apart. Each patch or binary part is charged with the window it
  draws. Past the budget the least recently opened entry of either view is
  freed, never the latest request; a freed part says so and is read again,
  at the same `at`, when next shown (**Show it again**), at the part shown
  before, by the cursor already learned for it, whether or not the part
  count is known yet (Last still waits for the count). A 409
  or another fingerprint on that read shows the Refresh banner. The image
  preview is a whole read (SUB decision feb0d839): once its first page
  gives its size, it is charged that size only in room no other read holds,
  frees nothing, and gives its room back before any other entry when a
  later read needs it (**Show image again**). An image that does not fit
  says so with its size and the free room, and keeps Show bytes. The
  Activity view's refit (#115 round 7, SUB a5275d81) asks its own scope
  what a larger inline charge would free (`wouldFree`, as a pinned
  reservation); a scope names only its own keys, in the order `reserve`
  frees them, and admits as `reserve` does, passing on its options (#115
  round 8, SUB cf21d510), so the Activity view hands its displayed items
  read whole to parts and leaves the diff view's entries to be freed, or
  kept, by the reservation itself. The budget is a cache budget only:
  nothing is hidden or cut to keep within it.
- **Banners.** `unstable: true` on any list page shows "Changed while
  loading · Refresh". The `ship` event's
  `crewMembers[].contexts[].git.fingerprint` is passed to the view on each
  update: once the snapshot has loaded, a value other than its fingerprint
  shows "Changed since loaded · Refresh", as does a stale page. As in #110,
  a value seen before the load started is not a change, a repeat proves
  nothing, and no fingerprint shows no banner.
- **Cancellation.** All reads of one snapshot share one token. Refresh, a
  mode change, closing the view (Back, Escape or a tier change), a source
  change and a stale fingerprint (a moved live value, a 409 or another
  fingerprint) cancel it: fetches abort, waits in busy backoff end at once,
  and an answer that arrives anyway is dropped. Hide bytes cancels its
  binary read.
- **Source change.** `src/fleet-manager.js` `openDiff(fleet)` gives the view
  a client that resolves the fleet's current client at each read. A change
  of address or token drops what was read and reads again from the new
  client; losing the token or the fleet closes the view.
- **Rendering.** `src/diff-view.js` builds DOM nodes and text nodes only,
  never `innerHTML`, through the Activity view's `h` and `mount`. Control
  and bidi formatting characters show as the Activity view's `⟨U+XXXX⟩`
  markers. Long lines wrap and stay selectable; the hex view scrolls
  sideways. Large content is windowed, never hidden: the file list 200 files
  at a time, and a patch part in the Activity view's windows (at most
  64 KiB as shown, markers included, and 1,000 lines, measured by
  `src/document.js` `measurePieces`), each with First, Previous, Next and
  Last. Line numbers run on across windows and parts; a line split by a
  boundary continues in the next.
- **Memory only and chat-free.** Diffs are never stored or logged, never
  reach chat or the page's `setContext` value, and are dropped when the
  view closes. Image previews revoke their object URL when replaced. The
  panel lets go of a part's windows when the budget frees the part, and of
  the last view when it closes.
- **Sample mode.** `src/sample-diff.js` answers the same routes for every
  sample checkout: eight files over two list pages, a patch in three pages,
  a removal, a rename with no content change, an untracked note with a bidi
  override, a bell and a long line without spaces, a PNG, a binary of
  1 MiB + 4 KiB whose old side is empty, a submodule and a symbolic link.
  The sample `ship` events carry a fingerprint that moves as the simulation
  advances, so the Changed since loaded banner and the 409 on a stale `at`
  show too.

## Differences from desktop #110

- **A web viewer, not the PRs Space.** Asked for by the captain for this
  PR. SUB-DOC-1 §5 proposed opening the native viewer through a bridge call,
  which waits on SUB-63.
- **Unified only, with a flat file list.** No split layout or file tree: the
  inspector column is narrow, and a flat list shows each path in full.
- **Long lines wrap.** #110's virtual rows have a fixed height, so it opens
  long lines in **Read full line**. Here rows are DOM and wrap, so every
  line is reachable in place.
- **Markers in text.** #110 shows patch text as written, expanding tabs;
  here control and bidi characters show as markers (Trojan Source), as in
  the Activity view. The hex ASCII column keeps `.` as #110.
- **Windows of 200 files and 1,000 lines.** #110 virtualises its lists;
  here windows keep the page small.
- **One patch part at a time.** #110 appends each loaded page to the
  patch and keeps them all; here a part replaces the one before, so what is
  held stays within the console's budget, which the map's page shares.
- **No check when the Space becomes active.** The view follows each `ship`
  event while open instead.
- **File pages carry no `unstable`.** The contract gives a file page
  `{fingerprint, file, content?}`, so only list pages set the banner.

## Contract questions

Each carries the assumption this change makes; none blocked the work.

1. **Where `api.crewDiffs` is listed.** STAR-5 puts it next to `api`; the
   console reads the fleet gateway's feature list (`fleet.gateway.features`),
   where `api` is. Recommend that list, and the console could accept
   `ship.features` too if a ship can lack the route.
2. **A page ending exactly at `size` with a `nextPage`.** The contract says
   it has none; the console treats such a page as the last rather than
   refusing it. Recommend the gateway never sends one and the client
   tolerates it.
3. **The default project.** The console omits `project` for a context whose
   `project` is null, and expects `project: null` back. Recommend the
   gateway documents this.
4. **`unstable` on file pages.** Not in the contract; ignored. Recommend
   leaving file pages without it, as a stale `at` already answers 409.
5. **Web viewer against SUB-DOC-1 §5.** Recommend keeping this viewer for
   browsers and the Fleet Console web Space, and opening the native viewer
   when SUB-63 lands, behind the same gate.

## Checks

`node --test` in `web-spaces/fleet-console`: `test/diff.test.js` covers the
feature gate on the crew card (on, off, absent, Git error, no worktree), the
fingerprint on `isGit`, the routes' query strings and unread error bodies,
each status code of the first page, busy retries with `Retry-After`, the
default wait and the ten-second cap, an empty snapshot, list paging by
`nextPage` through the real client with an opaque token,
stale and invalid list pages, a 17-page list keeping no patch text outside
the budget and reading an opened patch at the snapshot, charged, rows
sharing a display path each showing their own patch from their own list
page (stale at another fingerprint) and no bytes, patch parts one at a time
with Last once the count is known, a one-part patch, windows of a long part
numbered across windows, **Retry page** for a patch and a binary page keeping the part on
show, busy and gone file pages, a self-pointing or wrong-path page, a 409
or another fingerprint for a stale `at` leading to Refresh, a part freed by
the budget read again at the same `at` and at the same part, before the
count is known too (by its own cursor, with no Last), a 409 on that read, an empty binary file, hex windows paging across a part boundary with
offsets running on, no Last until the last page's token is known, binary
bytes freed and read again at the same part, binary page checks, an image
preview grown into free room and given back first, a preview that cannot
fit, the Activity and diff views sharing one budget without freeing the
latest request, `wouldFree` on a scope naming only that view's keys with a
yielding entry first and admitting as `reserve` does with its `pinned`
option, the Activity view's growth refit on the shared budget, the
fingerprint banner (moved, repeated, absent, moved during the load, unstable), cancellation of reads in
backoff when the fingerprint goes stale, the view closes or Hide bytes is
pressed, a late answer dropped after a stale fingerprint, Refresh and a mode
change dropping the old snapshot, a token change while a read is held, and
the sample checkout end to end with markers.
