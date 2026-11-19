# Issues and Subspace API

The Issues board reads **Subspace API**, creates and edits issues, displays
images and text attachments, and posts comments and replies with images and files.
Sign in under **Settings > Account** first. Projects organise the issues;
there is no organisation selection. The
[in-app guide](../spaces/issues/help.md) covers the controls. The
[change record](changes/subspace-api-accounts/README.md) records the scope.

## Ownership and structure

`clients/subspace-api` (package `subspace-api-client`, Rust import
`subspace_api_client`) owns transport, wire models, queries, the shared hub,
reference resolution and memory-only state. It has no GPUI dependency.
`spaces/issues` owns presentation, navigation and the contract responder.
Blocking client calls run in the hub or `cx.background_spawn`, never in render.

The only built-in profile has id `subspace-api`, name **Subspace API**, and
the base URL the build config or `SUBSPACE_API_URL` names, normalised to
`{origin}/api` (by default `http://127.0.0.1:3000/api`). Routes append
`/v1/...`. See
[Identity providers and the API URL](../README.md#identity-providers-and-the-api-url).
There is no profile picker, saved profile configuration, organisation picker,
`X-Org-Id` header or `/orgs/{org}` path.

The source of truth is the API's documentation in `subspace-api/`; the
offline fixture `scripts/subspace_api_test_server.py` implements every route
the desktop uses with the exact shapes. Pages use `{ items, nextCursor }`, errors use
`application/problem+json`, and JSON names stay camelCase. Unknown response
fields are ignored for forward compatibility.

## Connect and sign in

Sign in under **Settings > Account** with the identity provider the build
selects (`dev` against a local stack, WorkOS or Entra). The shared native
account supplies a bearer token for Subspace API and renews it silently. `GET /v1/me` checks the native identity and registers
a new user with no initiative memberships. An app owner grants membership
before the new user can read or write issue data.

If the API returns 401, the board directs the user to **Settings > Account**. The client
uses bounded timeouts and queues demand-driven reads to avoid startup fan-out.

## Projects and issue data

Discover projects with `GET /v1/projects?includeArchived=true&limit=200`,
following every cursor. `Project` replaces the former team model: `id`, `key`,
`name`, `default_state_id`, `archived_at` and retained `states`.
Fetch `GET /v1/projects/{key}/workflow-states?limit=200` only when needed,
for example grouping by state inside one project. Do not load every project's
states at startup. States have `id`, `projectId`, `name`, `type`, `position`
and `isDefault`; they do not have issue counts.

Issue reads use these routes relative to the API base:

| Route | Purpose |
| --- | --- |
| `GET /v1/issues` | Filtered list, recent activity, trackers and child continuation |
| `GET /v1/issues/groups` | Counts and first pages grouped by state or state type |
| `GET /v1/issues/{ref}` | Detail, first 20 children and first 20 relations |
| `GET /v1/issues/{ref}/relations` | Relation continuation |
| `GET /v1/issues/{ref}/pull-requests` | Pull requests, first page and cursor continuations |
| `GET /v1/issues/{ref}/activity` | Per-issue activity and reference dictionaries |
| `GET /v1/issues/{ref}/comments` | Root comment threads |
| `GET /v1/issues/{ref}/comments/{rootId}/replies` | Replies to one root |
| `POST /v1/issues/{ref}/comments` | A text comment or reply |

An issue reference is an identifier such as `SUB-42` or an issue UUID.
Rows use `projectId` and `projectKey`. They include state, assignee, labels,
priority, estimate, due date, parent, tracker progress, archive state, version,
comment count, `todoCounts { total, done }`, and non-null `lastActivityAt`.
`lastActivity` is nullable and carries the kind, time and actor. Preserve API
fields such as `projectRank`, `subIssueRank`, `cycleId` and provenance without
inventing the former service's fields.

Detail adds description, creator, lifecycle timestamps, todos, parent,
duplicate target and paged children and relations. Activity pages also have
`references { people, projects, issues, labels, decisions }`. Relation rows
are issue rows with `kind`, such as `relatedTo` or `blockedBy`.

`ListQuery::project(..)` and `selected_project()` replace team selection.
Supported filters include project key, `stateId`, repeated `stateType`,
assignee (`me`, `none` or user ID), priority, parent (`none` or reference),
type (`issue` or `tracker`), due (`overdue`, `thisWeek`, `none`), search `q`
and `includeArchived`. Orders are `created`, `priority`, `manual` and
`activity`. Manual order needs a project or parent. `activitySince` is an
inclusive offset-qualified instant. Pass continuation cursors with the same
filters; do not exchange them between lists.

Groups use `groupBy=state` with one project, or `groupBy=stateType` across
projects, and `limitPerGroup` from 0 to 25. Each group has `stateId`,
`stateType`, `count`, `items`, `nextCursor`. Terminal groups initially return
counts only. Expand or continue through `/v1/issues` with the group's state
filter. Children from detail continue through `/v1/issues?parent={issue UUID}&order=manual`,
using the same raw parent string for every cursor. Relation pages allow 1–100 items.

## Shared hub operations

Keep one retained hub handle in the board's lifecycle. Check its version
before cloning a snapshot. Snapshots and reference projections do no network
I/O and remain in memory.

| Operation | Meaning |
| --- | --- |
| `Hub::start() -> Result<Hub, HubError>` | Start or reuse the built-in Subspace API hub |
| `Hub::from_client(client: Client, name: &str) -> Hub` | Isolated injected client for tests and debug fixtures |
| `snapshot()`, `version()` | Coherent board state and change version |
| `HubSnapshot::projects()`, `me()` | Project directory and optional current identity |
| `activate(query)`, `reload(query)` | Demand-driven list load or explicit refresh of list and discovery |
| `load_more(group)` | Continue a group or expand a terminal group |
| `activate_activity(query)`, `reload_activity(query)` | Load or refresh in-progress trackers and recent issues |
| `retry_activity(section)`, `load_more_activity(section)` | Retry or continue Trackers or Recent independently |
| `HubSnapshot::activity()` | Separate retained activity snapshot |
| `open_issue(identifier, profile_id)`, `reload_issue()` | Read or refresh detail, checking navigation scope |
| `load_more_subissues()`, `load_more_relations()` | Continue the open detail's children or relations |
| `load_more_pull_requests()` | Continue associations or retry their failed first page |
| `close_issue()` | Drop detail and retire pending detail reads |
| `irc_snapshot()`, `reference_version()` | Restricted reference projection for the contract responder |
| `resolve_reference(identifier, profile_id)` | Queue a demand-only reference read |
| `stop()` | Discard queued work and snapshots; reject stale in-flight results |

`HubStatus` is `Loading`, `Ready`, `Error(ReadError)` or `Stopped`.
`ReadError` keeps `SignInNeeded`, `Network`, `Forbidden` and
`Response { status }`. Discovery reads `/me` and projects. Activity queries
use `project(..)` and `selected_project()` too; trackers use `type=tracker`,
`stateType=started`, `order=activity`, while recent issues use
`order=activity` and `activitySince`. Neither needs an eager grouped read.

There are no hub operations for signing in or out, selecting an organisation
or applying profile settings. Issue writes, media reads and comment uploads
are described below. The snapshot has no organisations, sign-in state or
organisation ID.

## Live updates

The Issues client opens one authenticated HTTP event stream at
`GET {api}/v1/updates?project=SUB` once it reads `/me` and has a project to
follow. Each connection uses the native account's current Subspace API token.
It subscribes to the selected project or the projects shown in the
All projects list, and includes the open issue's project. On a scope change
it cancels the old stream and opens a new one for the settled scope. The
`ready` event confirms the subscription. The open issue and list head are
then read again, including on reconnect.

An `issueChanged` hint queues `Task::Recheck` for the open issue (when it or
its parent matches) and `Task::ListHead` for the list (when it holds that row
or shows that project), whatever the hint's `version`: the API keeps the
version for comments, threads, attachments, pull-request links, summaries,
documents and reviews. Bursts coalesce for 300 ms; issue reads are at least
2 s apart and list reads 3 s. `LiveReads` (`hub/live/flight.rs`) keeps at most
one live read out per target and scope; hints that arrive meanwhile owe one
trailing read, sent when it lands. `Recheck` drops a reply older than the held
version. Otherwise it replaces the detail, keeping paged sub-issue and
relation rows while the fresh first page starts the same way, reads the pull
request head, merges the timeline head and reads the attachments and images
again. A media scan that reaches its last page retires items it did not see
and takes their new metadata; a failed scan keeps what is shown.
Continuations (list groups, sub-issues, relations, pull requests) apply only
to the cursor they read from. `ListHead` installs the fresh first pages and
their cursors as a new list installation, so rows paged in before are paged
in again, and reads the My issues and hidden sub-issue counts again. Failures
keep what is shown. Activity is not refreshed by hints or reconnects; Reload
reads it (live Activity is not yet streamed).

Reconnects back off from 1 s to 30 s with jitter. The backoff resets after
60 s live or when the project scope changes. The open issue is read every
30 s while connecting, reconnecting or polling. An unavailable stream or an
Access refusal shows **Polling every 30 s** and the client retries in the
background. `HubSnapshot::live()` drives the header indicator.
`Client::without_live()` disables the stream for tests. The loopback fixture
can answer 404 for the stream with `--no-updates-stream`. See the
[change record](changes/subspace-api-accounts/README.md).

## Posting comments and uncertain outcomes

Keep `CommentDraft`, `CommentState`, `comment_draft`, `edit_comment`,
`edit_comment_at`, `discard_comment`, `has_comment_drafts` and the existing
submit/retry/discard flow. Drafts stay in memory. Submit text to:

```http
POST /api/v1/issues/SUB-1/comments
Content-Type: application/json

{ "body": "The fixture check passed.", "parentCommentId": null }
```

Use a root comment UUID for `parentCommentId` when replying. The API returns
201 and a comment with its author, body, timestamps, version, resolution,
reply count and media list. It also supplies `displayCreatedAt`, `mentions`
and provenance. Root and reply pagination are independent. There is one reply
level; replies target a root on the same issue.

Before POST, scan the relevant thread and record its comment IDs. If that
read fails, preserve the draft without sending POST. Recovery uses those IDs
instead of either clock. There is **no idempotency key or media draft ID**.
Never retry a POST automatically. After a network failure following send, timeout or 5xx, the
outcome is unclear. Freeze the draft and offer **Check discussion**. Reload
the relevant thread and look for a comment with all of these:

- `actualAuthor.userId` equals `me.user_id`;
- body and parent equal the submitted body and parent;
- `id` was absent from the complete thread scan taken before POST.

A match confirms success. If there is no match, offer **Post again** and
explain that it may post twice. A failed discussion check is not evidence
that the POST failed. Preserve the draft and let the user check again.

## Issue creation and editing

The focused form is shared by **New issue…** (`c`) and **Edit issue…** (`e` in
loaded detail). Enter in a single-line field, or Command/Ctrl+Enter anywhere
in the form, saves. Escape closes the topmost picker, then offers to discard
an edited draft. Drafts remain in memory across board activation and quitting
warns about unsaved or unconfirmed work.

`PATCH /v1/issues/{issueRef}` contains only changed fields and the basis
`version`: title, description, stateId, assigneeId, priority, estimate, dueDate,
labelIds, parentId, milestoneId, cycleId and type. Null clears nullable fields; an empty labelIds
array clears labels. Project is fixed for an existing issue; project moves use the separate move flow.
`POST /v1/issues` uses projectId, title and the selected optional fields,
including nullable milestoneId and cycleId (see the API's Planning API contract).
The hub's `IssueWriteScope` fences writes and picker results by connection
generation and current user. Run `create_issue`, `update_issue`, `latest_issue`,
`issue_choices` and `check_created_issue` in background tasks.

The pencils beside properties and Description open the same retained editor.
Property entry points open their picker during activation; the shared anchor
positions it after measuring the new form. Text entry points focus their input.
The existing Edit issue command remains the entry point for title changes.
Writes use the API's existing activity and audit transaction, then reload the
issue and timeline. The desktop does not manufacture audit rows.

Milestones load on demand from `/v1/projects/{key}/milestones` and cycles from
`/v1/issue-cycles?project={key}`, in pages of 50. Milestone reads send `includeContent=false`. Only current and upcoming
cycles can be newly selected. An existing historic cycle remains visible and
is omitted from PATCH unless changed. Assigned milestone names resolve after
the issue detail is published; lookup failure keeps the issue usable and shows
**Unavailable** when no name is known. Refreshes keep a known name while the
same milestone remains assigned, including on lookup transport failure.
This extra read is fenced by the open issue revision.

Workflow states load on demand from `/v1/projects/{key}/workflow-states`;
assignees from paged `/v1/users` include humans and agents. Labels come from
paged `/v1/labels?state=active`. Estimates follow the project's estimateScale
from `/v1/projects/{key}`: Fibonacci, exponential, linear or T-shirt values.
A tracker derives state from children, has no direct estimate and cannot join
a cycle; those controls are disabled with a reason in their tooltip. Markdown remains a plain
textarea, preserving `@[label](member:<user-uuid>)` mentions verbatim.

A create 409 reports an archived project and asks the person to choose an
active project. For a PATCH 409, read the latest issue to check its archive
state. An archived issue asks the person to restore it before saving; retain
the draft without offering version recovery. Other conflicts require
**Read latest issue**, then **Use latest version and keep edits**.
Unchanged text adopts the server value, explicit property edits remain, and
label additions/removals merge with the latest labels. Nothing resubmits
automatically. Failed or offline saves retain the draft. An unclear PATCH
freezes the draft until the latest issue is read and the person re-applies it.

Creation has no service idempotency key. Before every POST, read every page of
`/v1/issues?project={key}&creator=me&q={exact title}&includeArchived=true` and
record existing IDs. The title is matched exactly in the client. A failed or
cyclic preflight sends no POST. A timeout, lost response, malformed success or
5xx freezes the draft and offers **Check issue**. Only a new ID absent before
POST, with the exact title, project and creator, confirms success. Failed
checks keep it frozen. Several new matches also keep it frozen and ask the
person to check the issue list. A complete check without a match enables **Create again**
and says it may create a duplicate. No POST retries automatically, and the
local async identity is never sent as an idempotency key.

## Channels references

The board title is **Channels**; the board id remains `irc` and the transport
remains IRC. Its issue references use the Framework's `issues` contract,
never the Issues board's entity or the API client directly.
`IssueScope { profile_id, profile_name, api_origin, project_keys }` is built
with `IssueScope::new(profile_id, profile_name, api_origin)` and
`.with_project_keys(..)`. Project keys follow `[A-Z][A-Z0-9]{1,9}`.
Links to other trackers stay links.
See [issue references](irc.md#issue-references).

## Back from the issue page

Back (header button, Escape, the palette's Back command) goes where the issue
was opened from, and its label names it (`spaces/issues/src/back.rs`):

- Opened from the list, the board or Activity: the list, with its filters,
  selection and scroll as they were. The label is **Back to issues**.
- Opened by a link from another board (`DashboardEvent::Navigate`): that
  view. Today only an issue key in a Channels conversation sends one. The
  shell tells the board which view it came from (`Dashboard::arrived_from`)
  just before it opens the link. Channels names the conversation the key was
  clicked in (`BoardNavigation::origin`, `spaces/irc/src/back.rs`), so the
  label is **Back to #discovery**. A board that sends a link without a place
  is named by its title.
- Opened from search, the palette or a notification: the list, as from the
  list itself.
- Opened from an issue (parent, sub-issue, relation, path link): the first
  issue's origin stays. Back leaves the issues instead of stepping through
  each one, so it never loops between two issues.

Back always closes the page into the list first, then emits
`DashboardEvent::Return`. When the origin's Space or place no longer exists,
such as a conversation closed or a channel left, parted or kicked from
since, the shell stays on Issues and shows a short
toast; the list is the fallback.

## Complete issue view

Ported from slice 5 of the upstream issue workspace. The issue page keeps the editor, image
viewer and comment attachments. Its main column orders Description, media,
Sub-issues, Relations, Pull requests and Activity; Properties and Record form
the right column and stack below it under 1000 px. More offers Copy identifier
(`y`, Command/Ctrl+Period) and Reload. Tracker help uses the shared anchor's
Escape, outside dismissal and focus return.
Right-click the identifier below the title for **Copy issue number**, which
copies the complete issue key through the shared native context-menu adapter.

The workflow reads only the open issue's project states through
`GET /v1/projects/{key}/workflow-states` and groups them by type, so custom
workflows keep their shape. Triage, backlog, unstarted, started and unknown
types are path steps in workflow order. Completed, canceled and duplicate
types share one end slot, which shows the issue's end state, or the first
completed state while it has none. The workflow starts with the full drawing
before measurement and uses the current-state fallback in narrow or zoomed
layouts. The trail describes position; the server remains authoritative for
state.

Where the issue can be edited and is not a tracker, the end slot is a quiet
`row_button` that opens the shared menu through the board's `MenuAnchors`. It
lists every end state, completed then canceled then duplicate, with the
current one checked. A pick sends the editor's sparse PATCH (`stateId` and
`version`) through `Hub::update_issue` in a background task, keeps the slot
busy until it returns, then reloads the issue and the list. A refusal raises
an error toast naming the issue and reloads it. The board has no duplicate-of
picker, so Duplicate is a plain state change.

Relations use `detail.relations` then `/v1/issues/{ref}/relations`.
Sub-issues use `detail.children` then the UUID-scoped parent query. Tracker
header progress uses `fields.progress`, clamps done to 0–total, uses marks
through 16 and Kit Progress above 16, and separates exclusions. Blocked and
archived notices use server flags; a partial blocker page says “and others”.

Properties use issue fields and the project directory, including former
members, service-coloured labels and parent navigation. Only populated
estimates and cycles appear. Milestone IDs without a name show Unavailable;
this API has no event property or tracker points. Record uses `createdBy`,
`startedAt`, `completedAt`, `canceledAt`, `archivedBy`, creation/update/archive
dates and provenance. Each populated original provenance date replaces only
its corresponding native date, with an Originally label.

Rows carry a nullable `pullRequestSummary`. Missing summaries
mean no summary. The Pull requests heading uses the summary's own open, merged
and closed counts, omitting zero counts (for example, **Pull requests · 2 open ·
5 merged**); it does not count the loaded page. Rendering performs no I/O.
The independent memory-only association snapshot reads
`GET /v1/issues/{ref}/pull-requests?limit=20`, then its cursor pages, bounded at
100 items per request. It maps repository fullName, number, title, state/draft,
baseBranch, referenceActive, referenceKind and completion to the panel.
URLs follow the shared external-link policy. Rows survive continuation failures
and offer Retry; stale results are fenced on close, reload, scope change and
stop. A 404 on this route alone shows “Pull requests are not available from
this server yet”. It does not make the issue unavailable. Reload checks the
route again after deployment. Issue content is not persisted by this board or supplied as automatic chat context. A person may explicitly select text and choose Add to chat.

## Removed and deferred

NETT transport and its file/upload origins, WorkOS device sign-in,
refresh credentials, organisation selection, profile storage and permissions
or feature-pack discovery are removed. Historical `docs/changes/nett-*`
records describe the old implementation and remain unchanged.

Issue creation, editing and property pickers are available through Subspace
API, as are images, the image viewer, comment attachments and issue pull
requests. The former NETT file-grant workflow remains deferred. The former
NETT implementation is not a compatibility layer.

## Debug overrides and offline fixtures

Debug builds pair `SUBSPACE_API_URL` with `SUBSPACE_API_ACCESS_JWT_FILE` for
offline fixtures; without the file, `SUBSPACE_API_URL` only selects the API and
the signed-in account supplies the token, as in release builds. The URL must
be numeric-loopback HTTP, for example `http://127.0.0.1:18161/api`; the client
validates it through the fixture URL boundary. The file holds the token sent
as `Authorization: Bearer`. The client reads it when created and trims
trailing newlines. This applies only with an explicit
loopback URL; loopback requests bypass environment proxies. Never log or display
a real JWT. Release builds ignore both variables.

Start the memory-only fixture with a deliberately fake token:

```bash
bash scripts/subspace-api-test-server.sh --access-jwt fixture-access
```

In another terminal, run the debug app:

```bash
(
    umask 077
    jwt_dir="$(mktemp -d)"
    trap 'rm -rf "$jwt_dir"' EXIT
    printf '%s\n' fixture-access >"$jwt_dir/token"
    SUBSPACE_API_URL=http://127.0.0.1:18161/api \
    SUBSPACE_API_ACCESS_JWT_FILE="$jwt_dir/token" make run
)
```

The server binds only `127.0.0.1`, requires the exact command-line token as
`Authorization: Bearer` on every route and answers a missing or wrong token
with 401 ProblemDetails.
It serves projects `SUB` and `OPS`, Ada Fixture and Grace Fixture, a tracker
with children, root/reply discussion and recent activity. Posting mutates only
that server's memory. It uses no credentials, files, Docker or upstream API.
It deliberately provides no idempotency: repeated posts create distinct comments.

`make capture OUT=... STEPS='...'` supplies its own new HOME and starts the
GitHub and Subspace API fixtures on free loopback ports. It passes only a
private temporary token-file path and the loopback URL; use it for
screenshot evidence. Debug launcher overrides also reach the macOS app through explicit `open --env`
arguments, never the token value. The launcher creates a private temporary
copy and waits for the app to exit before removing it. The fixture replaces
the old NETT/WorkOS scripts and environment variables; old names have no effect.

Run the portable fixture checks with:

```bash
python3 -I scripts/test-subspace-api-test-server.py
```

When the API checkout is available, add
`--openapi /path/to/api/docs/openapi/subspace-api.v1.json` to verify nested
response property names, types, enum values, UUIDs and dates against the source
contract with no third-party Python packages.

These checks cover HTTP authentication, camelCase wire shapes, discovery,
filters, groups and continuation, detail, relations, activity, root/reply
posting, invalid requests and isolated in-memory state. They never call a
live service. Desktop render tests and screenshot evidence are separate
integration checks owned by the client, board and Framework parts.

## Images and comment attachments

Media descriptors in comments use `mediaId`, `kind`, `fileName`, `contentType`,
`sizeBytes`, `url` and `sha256`. Issue detail currently has no descriptors;
`GET /v1/issues/{ref}/attachments` supplies them for descriptions. Cursor-paged
`GET /v1/issues/{ref}/images` supplies the viewer's flat metadata. The wire
Markdown scheme is `nett-media:`; it is never shown as a product name.
Only descriptor-backed references become image tiles or file chips. Code stays
literal, untrusted internal links stay text and external images stay links.

Image bytes come from `GET /v1/issues/media/{mediaId}`, through the same
origin, pacing, User-Agent, Access detection and 45-second transport as JSON.
Descriptor URLs are data and are never followed. Reads require the exact stored
PNG/JPEG/GIF/WebP content type, `nosniff`, a bounded body and the declared length.
Tiles request bytes near the viewport, with three decode slots and one reserved
for the viewer. The decoded LRU holds at most 128 MiB across the last two issues.
Closing an issue cancels pending decodes and clears the viewer and anchors;
hiding the board, stopping the hub, changing scope or losing access clears all
images. Reload retires pending/failed reads while keeping decoded images.
Evicted tiles remain placeholders until opened, avoiding a fetch/eviction loop.

Open a tile to view it. Left/Right page images; Tab and Shift+Tab cycle available
controls. Escape returns focus to the opener. The API has no image-anchor query,
so an inline image outside loaded metadata pages triggers cursor reads until it
is found or the list ends. **Show in the description** and **Show in the comment** reveal loaded source prose.

### Attachment panel

A `text/plain` or `text/markdown` file, in **Attachments** or as an inline file
reference, is a native card button that opens a panel to the right of the issue
page, under the path bar. Other files keep their read-only chips. The panel uses
the shared detail panel's stored width (clamped to 320 px and 75% of the window),
surface and left edge, and has no resize handle. Its title bar shows the file
name and size with a quiet **Close attachment** button, which takes focus on
open. Escape in the issue page closes an open menu first, then the panel,
before it goes back to the list; closing returns focus to the opener. Opening
another issue, going back, hiding the board or losing access clears it. The
page subtracts the panel's width (`media::text::panel_width`) before it
chooses one or two columns.

Bytes come from `GET /v1/issues/attachments/{mediaId}` through the hub's paced
byte reads and file slots, fenced to the scope and issue revision, never from
render. The read has the image rules: the exact stored type, `nosniff`, a
bounded body and the declared length. Files over 1 MiB (`MAX_TEXT_BYTES`) are
refused before any request, with both sizes in exact bytes and the limit in
MiB. A 0-byte file shows *This file is empty.* with no request and no Retry.
Non-UTF-8 bytes show an explanation with no Retry. A failed read offers
Retry, and reads again after a reload or revision change. A result is shown
only while the board is in the read's scope and issue revision and the panel
waits for that read of that attachment; a late result after a reload, a close
or opening another file is dropped, and a dropped read is read again once the
new revision loads.
Markdown is prepared off the UI thread with the description's preparation and
no media descriptors, then drawn by `rich_text_prepared` in Kit's virtualised,
scrolling `TextView`: no HTML, remote images become `alt ↗` links, `nett-media:`
references read *Attachment unavailable*, and only `safe_url` links open, through
the shared link opener. Plain text uses selectable text in the theme's
monospace face, with the shared text menu, including explicit manual Add to chat.

Attach files… opens the native picker; clipboard images and copied file paths use the composer's
existing paste intake. Preflight runs in the background and checks names, MIME,
encoding, length and image dimensions. Image uploads use multipart field `file`
at `POST /v1/issues/{ref}/images`: PNG/JPEG/GIF/WebP, at most 10 MiB, 4,096 px per
side and 16,777,216 pixels. Other files use `POST /v1/issues/{ref}/attachments`:
PDF, DOCX, XLSX, PPTX, UTF-8 TXT and MD, at most 10,000,000 bytes. A draft holds
at most eight attachments and 32 MiB; retained encoded buffers across drafts
are limited to 64 MiB. Pending intake or unfinished rows block submission.
Ready rows append Markdown references to the returned media ID, including for
attachment-only comments. The existing discussion check compares the complete
submitted body, including those references.

Upload sends have a five-minute deadline and are never retried automatically.
Uploads have no idempotency key. A complete
attachment-list scan records existing IDs before POST. An unclear result keeps
Check/Remove; Check scans metadata for exactly one new matching normalised name, size and
SHA-256. Names strip Unicode format characters (Cf), trim surrounding
whitespace and allow at most 200 UTF-16 units before declaration. Several
matches preserve the unresolved row. After a complete successful scan finds
no match, Remove drops the row locally, releases posting and warns that an
orphaned upload may still exist on the server.
A failed check cannot prove that an upload failed. Known refusals offer an
explicit Retry and honour Retry-After; a 503 remains unclear and offers Check.
Remove deletes known images with `POST /v1/issues/media/{id}/delete`, and files
with `DELETE /v1/issues/attachments/{id}`. Failed cleanup stays visible for
another explicit Remove. Discard invalidates outstanding intake leases and
cleans up unposted rows. An unclear comment retains its attachments until the
discussion check or explicit discard; discarding an unclear post does not delete
media that may already belong to a posted comment. All drafts stay in memory.

## Local text tools

The shared rich renderer retains native selection and supplies the default text
menu. The issue title, property labels and values, timeline summaries and
comments participate in the same local-find scope as the description and open
attachment. The issue identifier keeps its Copy issue number menu.

Find uses Cmd+F on macOS or Ctrl+F elsewhere, independent of app-wide Search.
The issue page and literal attachment each provide their own ScrollHandle;
Markdown attachments retain their native virtual scrolling. Changing profile or
issue resets the find document scope. Queries and matches are in-memory UI state
and are not indexed, logged, stored or sent to a service.

## Issue summaries

The client’s shared `IssueFields` carries nullable `summary` on detail and list
items. `ReferenceCard` and the `issues` contract’s `IssueSummary::with_summary`
retain it in the current viewer session; the Issues responder supplies it to
Channels. Older records may omit it or return null.

The New issue and Edit issue forms share the retained draft in `editing/`
(`authoring/` owns comments). Creation requires a non-empty, single-line summary
of at most 300 Unicode characters. The input shows its count and validation;
submission is disabled for invalid values, and the client validates again
before transport. POST sends `summary`; PATCH sends it only when changed, with
`version`. A set summary cannot be cleared. The field error explains this rule.

The issue page starts with Summary above Description and uses the existing
inline Save/Cancel controls, keyboard submit, draft retention and conflict
handling. Null renders **No summary yet**. Fixture data covers both cases.
The [delivery record](changes/subspace-api-accounts/README.md) describes the desktop work.
