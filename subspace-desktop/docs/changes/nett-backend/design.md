# NETT backend for Subspace Desktop

Status is accepted, with implementation in progress. The desktop keeps the
same Issues and Channels Spaces in both builds. A NETT build uses NETT as its
only tracker, with an explicit API URL, its own WorkOS sign-in and its own
organisation scope. Channels keeps using IRC on its own. The checked-in build
still uses Subspace API with loopback development sign-in, and nothing about
it changes.

The API mapping checks the desktop's call sites against NETT's published
OpenAPI document, and against the agreed surface contract for the features
NETT is adding. NETT-side prerequisites are tracked in the Indra repository.

## Owner decisions

1. **Public, behind a build setting.** The NETT adapter lives in this
   repository and is off by default, like Entra. A build selects it with the
   build configuration's `api_backend`. The default binary contains the
   adapter but never constructs it.
2. **Milestone.** Full read and write plus media: sign-in, organisation
   selection, initiatives, issue teams and planning projects, issues (list,
   detail, create, edit, status, assignee, labels, priority, estimate),
   sub-issues, comments (read and write), issue lists as to-dos, decisions,
   attachments and inline images through NETT file grants, and live updates.
3. **Raw configuration values, no named environments.** A NETT build needs a
   NETT API URL and a WorkOS client ID, both build settings and both
   overridable at launch, so the same binary can be pointed at any NETT
   environment. The WorkOS client ID uses the same setting name and the same
   validation as Subspace API's WorkOS mode.
4. **Live updates are server-sent events only.** NETT serves the same update
   stream Subspace API serves, for native clients, and keeps its own
   real-time channel for its web app. The desktop uses its existing stream
   client. There is no delta polling and no SignalR client.
5. **NETT adds most of what it lacks, feature by feature.** Initiative
   channels with fleets and stakeholders, chat uploads, standalone
   attachments, issue move previews and the update stream arrive in separate
   NETT releases, so the desktop gates each on what NETT says it serves for
   the selected organisation, not on a fixed list (see
   [Capabilities](#capabilities)). NETT does not add initiative membership
   (the desktop shows every initiative the person can see) or Entra and
   development tokens.
6. **Issues shows the issue team.** A Subspace project is an issue team
   (key, workflow states, estimates and cycles) plus a NETT project
   (milestones, updates, review policy and initiative link). In a NETT build
   Issues names the picker and the issue's fact **Team**, and shows the
   planning project as **Project**.

## Design notes

- **One WorkOS client per NETT environment.** The desktop uses the WorkOS
  client that the NETT environment accepts for native clients, the same one
  its command-line client uses.
- **Device authorisation, not PKCE.** See [Sign-in](#sign-in).
- **Repeated `project` filters are declared.** NETT's OpenAPI document
  declares `project`, `assignee`, `label` and `state` as repeated query
  parameters.
- **Uncertain-create recovery carries over.** NETT's REST create and comment
  routes take no idempotency key. The hub's existing recovery, which reads
  `GET /issues?creator=me` and the discussion after an unclear result, works
  unchanged against NETT.
- **WorkOS refresh pins the organisation.** The Framework's WorkOS refresh
  sends the account's WorkOS organisation when there is one. NETT ignores
  WorkOS organisations. The device flow keeps the organisation WorkOS
  returned at sign-in, which is usually none, so the pin is a no-op unless
  the user belongs to several WorkOS organisations.
- **No SignalR.** Rust has no mature SignalR client, so NETT serves a
  server-sent event stream for native clients instead (see
  [Live updates](#live-updates)).
- **Fail closed on Subspace settings.** A NETT build refuses every
  Subspace-only setting (see [Build and launch settings](#build-and-launch-settings))
  and never configures the Subspace API origin, so nothing can reach Subspace
  API from it. A test proves it.
- **One parsed URL validator.** The build script and the app share the URL
  and client ID validators through `#[path]`, as they already share
  `build/config.rs`.
- **The kill switch comes first.** Construction is centralised: the product
  installs one backend, and every client construction site in the Spaces goes
  through it.

## Build and launch settings

Build settings live in `products/subspace/build-config.json` or the file
`SUBSPACE_BUILD_CONFIG` names. A private NETT build file might contain:

```json
{
  "api_backend": "nett",
  "nett_api_url": "https://api.nett.example",
  "identity_provider": "workos",
  "workos_client_id": "client_00000000000000000000000000"
}
```

| Build field | Launch override | NETT build | Subspace build |
| --- | --- | --- | --- |
| `api_backend` | none | `nett` | `subspace` or empty (the default) |
| `nett_api_url` | `SUBSPACE_NETT_API_URL` | required | refused |
| `identity_provider` | `SUBSPACE_IDENTITY_PROVIDER` | `workos` (empty turns sign-in off) | `dev`, `workos`, `entra` or empty |
| `workos_client_id` | `SUBSPACE_WORKOS_CLIENT_ID` | required | required for `workos` |
| `workos_api_url` | `SUBSPACE_WORKOS_API_URL` | optional | optional |
| `workos_redirect_port` | `SUBSPACE_WORKOS_REDIRECT_PORT` | refused (no redirect) | optional |
| `api_url` | `SUBSPACE_API_URL` | refused | the Subspace API URL |
| `dev_*`, `entra_*` | `SUBSPACE_DEV_*`, `SUBSPACE_ENTRA_*` | refused | as before |
| none | `SUBSPACE_API_ACCESS_JWT_FILE` (debug fixtures) | refused | as before |

- There is no run-time backend selector. The backend is a build constant.
- **One resolved launch configuration.** The app resolves the stamped build
  settings and the launch overrides once, with the same rules for both
  variants, before any account or client exists. A Subspace build refuses a
  non-empty `SUBSPACE_NETT_API_URL`, and one that signs in with WorkOS
  checks its client ID as the build does, so an invalid override fails at
  launch rather than at sign-in. A NETT build with sign-in turned off
  still validates an overridden client ID, so an invalid or empty override
  is an error in either case.
- **The same WorkOS client ID setting serves both backends.** The desktop's
  `workos_client_id` and `SUBSPACE_WORKOS_CLIENT_ID` and Subspace API's
  `WORKOS_CLIENT_ID` all take the client ID from the WorkOS dashboard. All
  three accept only `client_` followed by 1 to 121 ASCII letters and digits.
- **URLs.** One parsed validator accepts an absolute HTTPS origin, with an
  optional `/api` path that is normalised away, or plain HTTP on a loopback
  address for local fixtures. It refuses user information, queries,
  fragments, other paths and control characters.
- **Refused settings are configuration errors.** In a NETT build, a
  non-empty `SUBSPACE_API_URL`, `SUBSPACE_API_ACCESS_JWT_FILE`,
  `SUBSPACE_WORKOS_REDIRECT_PORT`, `SUBSPACE_DEV_*` or `SUBSPACE_ENTRA_*`, or
  a `SUBSPACE_IDENTITY_PROVIDER` other than `workos` or empty, stops the
  backend before any account is registered. The app then shows Issues as
  unavailable and sends no tracker request at all. The build script refuses
  the equivalent build fields, and refuses `nett_api_url` in a Subspace
  build, so the active endpoint is never ambiguous.
- **Credentials and state.** The WorkOS keychain namespace includes the
  product, the NETT API origin, the client ID and the WorkOS origin. A
  sign-in kept for one NETT API is therefore never restored, and its bearer
  never sent, when a launch points the build at another, such as a loopback
  fixture: that origin asks for its own sign-in first. The saved
  organisation is keyed by NETT API origin and user. Run builds for different NETT
  environments with different data directories if they run at once.
- **Distribution.** Variant bundle identifiers and asset stems, and turning
  off the updater for NETT builds, are deferred until NETT builds go to
  anyone but the owner. Until then NETT builds are local `make run` builds.

## Sign-in

NETT builds sign in with **WorkOS device authorisation**, the flow NETT's own
command-line client uses. The Framework's WorkOS provider gains it as a
second flow (`workos_auth::Config::with_device_authorization`), and keeps its
refresh, keychain and generation handling.

1. `POST {workos}/user_management/authorize/device` with the client ID
   returns a device code, a user code and a verification URL.
2. The shell opens the verification URL in the system browser and Settings ›
   Account shows the user code, so the person can check it matches.
3. The provider polls `POST {workos}/user_management/authenticate` with the
   device-code grant at the given interval, adding the extra delay
   `slow_down` asks for, until WorkOS returns tokens, the code expires,
   access is denied or sign-in is cancelled. Each exchange is bounded by the
   code's remaining lifetime, and the deadline is checked again when the
   exchange returns, so a success that arrives after the code expired is
   discarded.
4. The refresh token goes to the keychain and the access token stays in
   memory, as with the PKCE flow. In device mode a refresh posts the
   form-encoded refresh grant, as NETT's command-line client does. A reply
   that names a WorkOS organisation keeps it for later refreshes, and
   switching account replaces the stored credential.

Why device authorisation:

- NETT accepts one WorkOS client per environment, and that client is the one
  its command-line client already signs in with by device authorisation.
- PKCE with a loopback redirect would need a fixed `http://127.0.0.1:<port>/callback`
  redirect registered in every WorkOS environment for the shared client.
  Device authorisation needs no redirect and no listening port.
- It works over remote sessions and in headless test runs.

The cost is one more step for the person (confirming the code), and each
WorkOS environment must allow device authorisation for the client.

## Organisation scope

- `GET /api/v1/auth/context` gives the NETT user, the accessible
  organisations and their roles. A context without a user means signed out.
- Eligible organisations are the business organisations. Personal
  organisations cannot use the issue tracker.
- The app restores the organisation saved for this NETT API origin and user,
  or selects the only eligible one. With several and no valid saved choice,
  Issues asks the person to choose one from the **Organisation** menu in its
  header. The choice is saved. With none, Issues says the account has no
  organisation that can use NETT's issue tracker, and offers no menu.
- Every tracker request uses `/api/v1/orgs/{orgId}/…` and also sends
  `x-org-id` with the same organisation, including file grants and the
  update stream.
- Changing organisation advances the account generation. The hub stops,
  drops its snapshots, drafts, grants and in-flight results, and starts
  again in the new organisation. A 403 or 404 on one record keeps the
  organisation; only a context refresh that no longer lists it clears it.
- **Everything is bound to the account that read it.** The auth context is
  adopted only if the account that asked is still signed in when its body
  has arrived. The Organisation menu shows nothing read for another
  account, and a selection is stored in the organisation's own spelling of
  its ID. When the account changes, its organisations, its cached grants and
  its initiative project lists go. Those lists are keyed by API origin,
  account and organisation, so two people in one organisation who see
  different projects never share one.
- **The initiative catalogue is fenced.** The shared catalogue is keyed by
  account and organisation generation. A switch resets it, and a read that
  started under the old scope is discarded when it returns, whether it
  succeeded or failed, so the former organisation's initiatives and
  selection never reappear. Channels removes the old initiative's autojoin
  as soon as the scope changes, and applies a selection again only once the
  new organisation's catalogue and capability list are known.

## Client boundary

The backend seam is the `subspace_api_client::Client` operation set. The hub,
the initiative catalogue and the reference cache call typed operations such
as `issues`, `issue`, `comments`, `post_comment`, `create_issue`,
`update_issue`, `decisions` and `issue_choices`, and never build URLs. The
client holds the selected backend:

- `Backend::Subspace` keeps today's routes and models.
- `Backend::Nett` sends the same operation to NETT, translating paths, query
  parameters and request bodies, and maps NETT's JSON into the desktop
  models. NETT's shapes are close to the desktop models, so the mapping is
  mostly field renames and defaults: an issue's team becomes the desktop
  project, NETT's planning project is kept apart, people are human unless
  NETT names an agent, and the Subspace-only fields (summary, owner, relayed
  crew) are empty.

The product calls `subspace_api_client::configure_nett_origin` (or, for
Subspace API, `configure_api_origin`) once, before any Space starts. Every
`Client::new()` then builds a client for that backend. Spaces ask
`subspace_api_client::capabilities()` before offering a feature that a
backend may not serve, and the client refuses such an operation before
sending anything while it is not served.

| Desktop operation | Subspace API (`/api/v1`) | NETT (`/api/v1/orgs/{orgId}`) |
| --- | --- | --- |
| Caller | `GET /me` | `GET /api/v1/auth/context` (user ID and name, kind human) |
| Initiatives | `GET /me/initiatives`, `GET /fleets/{id}` | with `initiativeChannels`, paged `GET /me/initiatives` and `GET /fleets/{fleetRef}`; without it `GET /initiatives` (no channels, no fleet) |
| Projects | `GET /projects` | `GET /issue-teams` (team key and states) |
| Workflow states | `GET /projects/{key}/workflow-states` | `GET /issue-teams/{teamRef}` states |
| Estimates | `GET /projects/{key}` estimate scale | `GET /issue-teams/{teamRef}` estimate scale |
| Assignees | `GET /users` | `GET /members` (page and page size) |
| Labels | `GET /labels?state=active` | `GET /labels` (active only) |
| Milestones | `GET /projects/{key}/milestones` | `GET /projects/{planningProjectId}/milestones` |
| Cycles | `GET /issue-cycles?project=` | `GET /issue-cycles?team=` |
| Issue lists and groups | `GET /issues`, `GET /issues/groups` with `project`, `stateId` | Same routes with `team`, `state`; limits clamped to NETT's |
| Initiative filter | not used | repeated `project` for the initiative's planning projects |
| Issue detail, relations, activity, PRs | `GET /issues/{ref}/…` | Same routes |
| Create and edit | `POST /issues` (`projectId`), `PATCH /issues/{ref}` | `POST /issues` (`teamId`), `PATCH /issues/{ref}`; no summary or owner |
| Comments and replies | `GET/POST /issues/{ref}/comments`, replies | Same routes; a comment with images carries `mediaDraftId` |
| Inline images and comment files | `GET /issues/{ref}/images`, bytes from `GET /issues/media/{id}` | managed media: `GET /issues/{ref}/images`, a grant from `GET /issues/media/{mediaId}`, uploads through `POST /issues/media/prepare` |
| Attachments | `GET /issues/{ref}/attachments` | with `attachments`, `GET /issues/{ref}/attachments` and its prepare, completion and delete routes, grants from `POST /files/grants`; without it the description's file media |
| To-dos | detail `todos` | `GET /issues/{id}/lists`, `…/lists/{listId}/items`, `PUT …/items/{itemId}` with the item version |
| Decisions | `GET /decisions?issue=`, `GET /decisions/{id}` | `GET /decisions?issueId=&status=all`, `GET /decisions/{id}`; made by the first decider, recorded by `createdByUserId` |
| Live updates | SSE `GET /updates?project=` | with `updates`, `GET /api/v1/updates?project=` with `x-org-id` |
| Moves | `POST /issues/{ref}/move-preview`, `POST /issues/{ref}/move` | with `issueMoves`, `POST /issues/{ref}/move-preview` then `PATCH /issues/{ref}` with the preview token |
| Chat uploads and `/resolve` | `POST /chat/uploads`, `POST /chat/uploads/resolve`, `…/content` | with `chatUploads`, prepare, completion, `GET /chat/uploads/{reference}` and `POST /chat/uploads/{reference}/grants` |

## Media and files

The NETT bearer goes only to the configured API origin. Bytes move through
NETT's file host, which never sees the bearer, a cookie or `x-org-id`.

- **Uploads.** Every NETT upload is the same three steps. A prepare request
  declares `{requestKey, fileName, contentType, sizeBytes, sha256}` and
  returns an upload `{uploadId, uploadUrl, method: "PUT", contentType,
  sizeBytes, expiresAtUtc}`. The client sends the bytes exactly once to the
  upload URL, with the declared type and length, from a separate
  credential-free agent that refuses redirects. Completion posts
  `{uploadId}`. A 202 means NETT is still checking the bytes: the client
  waits for the whole seconds in `Retry-After`, posts completion again and
  never sends the bytes a second time. One completion waits at most a
  minute in all; a wait that would take it past that leaves the upload
  pending, to be checked later. An unclear result keeps the pending
  upload so the same draft can be completed or cancelled.
- **Comment images and files** use NETT's managed issue media, which NETT
  already serves, so they do not depend on `attachments`. Prepare targets
  `{kind: "issueCommentDraft", issueId, draftId}`, where the draft ID is a
  random UUID kept with the comment draft and renewed after each post. Each
  file has its own random request key, so after an unclear result the same
  prepare request finds the upload again instead of sending it twice.
  Completion is
  `POST /issues/media/from-upload`, then `GET /issues/media/uploads/{uploadId}`
  while it is checking. Removing an image from a draft cancels it with
  `PUT /issues/media/{mediaId}/upload {version, cancel: true}`. The comment
  is posted with `mediaDraftId` and `nett-media:` references in its body.
- **Standalone attachments**, with `attachments`, are added, saved and
  removed from the issue's Attachments panel, which lists them whatever
  their kind. An upload uses `POST /issues/{ref}/attachments/prepare` and
  `…/from-upload`, which returns 201 the first time, 200 on a replay and 202
  while checking. The panel keeps each upload's request key, its transfer
  and whether its bytes may have been stored. Until then a retry prepares
  the same reservation again; after it, any failure, a lost completion
  reply or a refused sign-in included, is resolved by sending the same
  completion again, which replays the stored record if it already happened.
  The bytes are never sent twice. A completion refused with 409 or 404
  proves nothing by itself, because NETT also answers so when the issue
  changed or was archived after the upload was stored. The panel then reads
  the issue's attachments, each of which names the `uploadId` it was
  completed from (null for a file that was never uploaded): the record from
  this transfer was stored. A complete read without it means nothing was
  stored, and the next try reserves afresh under a new key; an identical
  file from another transfer is someone else's, but only a transfer named
  in full: an empty `uploadId` names none. A record that names no
  transfer leaves the outcome unknown, to be checked again, unless the
  name, size or SHA-256 it gives shows another file; an incomplete record,
  missing its type or digest, proves nothing, and neither does a malformed
  field, such as an empty name, a size below one byte or a digest that is
  not 64 hexadecimal digits. So does any read that
  fails. So does a record that names this transfer but cannot be
  read or differs from the file sent, and a reply that does not carry the
  list or show where it ends: only a complete read proves the upload
  absent. After three checks in a row that could not tell, the panel
  also offers Send again, warning that the file may then be attached
  twice; it sends the file under a new key and leaves the first
  reservation to lapse. A completion NETT rejects (400) or forbids
  (403) is final, and only Dismiss is offered. A reservation that lapsed
  before its bytes were sent is refused with 409; the next try reserves
  again under a new key. Uploads in progress are kept when the issue is
  refreshed, and when it closes: they are shown again, with whatever
  happened to them meanwhile, when it reopens in the same Issues hub,
  account and organisation, so a transfer whose bytes may be stored can
  still be checked or cancelled. Every upload lives in one store from the
  moment it is added, under the stable ID of its issue, which is known
  then because a file is only added to an issue whose detail is read; the
  open issue shows its own, so the issue finds them however it opens
  again, by its ID or under a new key after a move, even if it closed
  before its detail was read. Each use of the store, taking a snapshot
  included, first follows the open issue: uploads of other issues count as
  closed from then, each with its own close time, and a closed issue keeps
  only uploads still under way or awaiting a check that can change the
  outcome; those that stored nothing go, and their reservations are
  cancelled under the issue's ID. An upload whose completion may have
  landed is never cancelled by the store: only a check or the person's
  Dismiss ends it. What the store lets go is its bytes, which a check does
  not need, and only once no attempt is under way, since an attempt holds
  them too and one that fails before sending can still be retried: 15
  minutes after its issue closed, once more than eight closed issues hold
  bytes, counting only issues that still hold some, or when a new file
  needs the room. Past 64 closed uploads that only wait for a check, their
  bytes gone, the oldest are forgotten, still without cancelling. This
  needs no timer and happens at the next use however long the hub sat
  idle; a use that finds the open issue and the store as the last one
  left them, with no bytes fallen due, does no work, so a snapshot costs
  the same however many uploads wait. Such an upload shows again when its
  issue reopens, to be checked;
  one found not stored then asks for the file to be chosen again. A
  stored file reads the open issue's attachments again. Every upload request, completion and cancellation
  names the issue by its ID, which still names it after a move. A snapshot
  shows uploads without their bytes, so a view keeping an old snapshot
  never keeps them from being freed. Uploads run two at a time on a small pool,
  and the panel takes at most eight uploads and 32 MiB waiting on an
  issue, and 64 MiB across comment drafts and uploads, since a failed
  upload keeps its bytes for Retry. A file is admitted on its issue or
  draft first; only then, if it would not fit in the shared budget, do
  failed uploads kept for closed issues let their bytes go, the issue
  closed first first, and only as far as lets it in, counting only bytes
  nothing else holds, which letting go actually frees: if even all of them
  would not, nothing goes and the file is refused. Dismissing an upload that never
  completed cancels its reservation with `DELETE …/attachments/pending/{uploadId}`. A file is
  added to the issue, opening, organisation and account it was chosen in,
  or not at all: closing and opening the same issue again counts as a
  change. A file the client refuses, such as one over 10 MB, keeps its own
  message. A chosen file is read only as far as the largest upload of its
  type allows, and one byte more, so a larger one is refused before it is
  held in memory. A stored
  attachment is removed with `DELETE …/attachments/{mediaId}?version=`; a
  409 means it changed since it was listed, and the list is read again.
  Saving reads the file's grant. The save dialog opens in the Downloads
  folder and offers the attachment's name without path separators, control
  characters, Unicode format or bidirectional characters, leading dots, or
  the characters and device names Windows refuses, since the name comes
  from the service.
- **Downloads.** Managed media get their grant from
  `GET /issues/media/{mediaId}`. Standalone attachments get theirs from
  `POST /files/grants` with the descriptor's `publicFileId`. Either grant
  URL is read with the credential-free agent, with no redirects. The
  response must carry the stored content type exactly and exactly the
  descriptor's `sizeBytes`. A standalone attachment, image or file, must
  also carry `X-Content-Type-Options: nosniff` and a `Content-Length` equal
  to that size; managed media need `nosniff` for text.
- **Grants belong to an account.** A grant URL is a short-lived bearer
  secret. Grants are cached until 30 seconds before they expire, keyed by
  API origin, organisation, account lease and file. The client checks the
  lease before it looks a grant up, before it redeems a kept grant, after a
  new grant arrives and before it returns bytes, so a grant that arrives
  late is neither kept nor redeemed.
  The cache is dropped when the organisation or the account changes, so a
  second account never redeems the first account's grant.
- **Upload URLs belong to their reservation.** A prepared upload URL is
  used only while the account and organisation that prepared it are
  current, for standalone attachments, comment media and chat alike. Once
  the bytes may have been stored, any failure, a refused sign-in included,
  is resolved by checking the same transfer.

## Flow control

A NETT build has no fixed quota per time window. Each account keeps at most
six REST requests in flight, counted from before the bearer is attached
until the reply's body has been read. A request that waited for a place is
checked against the current account and organisation before it is sent. A
read already in flight for the same account and URL is shared rather than
sent again. A 429 or 503 is recorded before its place is given up and
pauses the account's requests for the whole seconds in `Retry-After`, up
to five minutes, or, when NETT gives none, for an exponential back-off from
one second with jitter, up to 30 seconds. Five minutes is the comment
path's limit too: NETT, or anything in front of it, may ask for a day, but
a pause that long would leave Issues dead for the session, since no request
would be sent that could end it. A request waits out a pause of up to 30
seconds; a longer one is refused at once, so nothing is sent before the
server's time and no request blocks for minutes. An explicit reload, of the
list or of the issue, or choosing an organisation, ends a pause at once,
because the person asked for the requests. Reads are tried again only
after a pause they waited out. Upload completion waits as asked for up to a
minute in all, giving up its request place while it waits; a longer wait, up to
five minutes, is kept with the transfer, and a check asked for before it
ends is refused without being sent, not even a read of the capability
list. Every reply that arrives
after the account or organisation changed, a read, a write, a managed
media completion or state, a to-do save, a move preview or a grant, is
dropped rather than returned or kept. The
update stream is separate: one per Issues hub, which follows one
organisation scope, and at most 64 team keys, the open issue's and the
chosen team first.

## Capabilities

NETT adds features over several releases, so a NETT build asks NETT which it
serves rather than assuming either way. The first time a feature is needed,
normally when it first reads the organisation's teams, the client reads
`GET /api/v1/orgs/{orgId}/desktop/capabilities`, which returns
`{"features": [...]}` naming the features served for that organisation. A
404 means none, and a list that cannot be read is tried again later rather
than taken as none. The answer is kept per API origin, organisation and
account, since it may differ from one person to the next, and only for the
account the read was made for: a list that arrives after another account
signed in is dropped. It is read again whenever the update stream closes
or is refused, after an explicit reload of the list or the issue, and
once it is five minutes old, since NETT can enable a feature while the
app runs. With the stream off, the hub reads the list again as soon as it
is due and opens the stream if it is now served. Each list that differs
from the last is a new revision, whichever read found it, so a read
elsewhere, such as an explicit reload's first team read, that finds
updates enabled also turns the stream on and has the Spaces read their
initiatives again. Until a new list
arrives, the previous one still answers; one that cannot be read again is
tried again after 30 seconds, then twice as long after each failure in a
row, up to five minutes. The desktop
uses five names and ignores every other name,
including `issueSubscribers`, `notificationUnread`, `issueDocumentArchive`
and `repositories`.

| Feature | Without it | With it |
| --- | --- | --- |
| `initiativeChannels` | Issues lists the visible initiatives without channels. Channels hides its initiative picker, never switches network or autojoins, and uses its configured connections. | Channels and fleet switching work as with Subspace API. Picker entries carry NETT's initiative path, such as **Platform › Desktop**, falling back to the name. Initiatives without a fleet use the configured connections. |
| `attachments` | The description's file media stand in for the attachment list, and the panel offers no add, save or remove. Comment images and files still work. | The Attachments panel adds, saves and removes standalone attachments. |
| `chatUploads` | File and paste-upload actions in Channels are hidden; a paste stays plain text. | Uploads work as described under [Chat uploads](#chat-uploads). |
| `issueMoves` | Moving an issue to another team is hidden. | The move preview and confirmation work as described under [Moves](#moves). |
| `updates` | No stream is opened, and nothing is polled: Issues says live updates are off, and changes made elsewhere show after a reload. | The hub follows the stream as described under [Live updates](#live-updates). |

Some things stay off because NETT is not adding them: the issue summary and
a separate owner (their editors and the summary requirement are hidden),
initiative membership (every visible initiative is listed), and captain
notices, which route by an agent delegate's ship. Ship and captain channels
that NETT generates for a fleet start with the fleet's slug, such as
`#aurora-nav`; matching them waits for captain notices to come to NETT
builds. The Subspace build matches a ship by its channel suffix, whatever
the initiative's prefix.

**Agent attribution.** When NETT names an agent as an activity actor, comment
author or issue creator, the adapter shows the agent with its rank and, when
it has them, its fleet and ship. It keeps the person the agent acted for
alongside it, so the issue view can show both, as "Name (agent, for
Person)". Subspace API's people and agents are shown as before, with the
former-member mark where it applied.

## Chat uploads

With `chatUploads`, a file or a long paste in Channels becomes a NETT chat
upload.

- The draft's random UUID is the prepare `requestKey`, and becomes the
  upload's ID. The stable reference is `UPL-` followed by the unpadded
  base64url of the UUID's 16 bytes, so the desktop can recover an upload by
  its draft after an unclear result with `GET /chat/uploads/{reference}`.
- Prepare, one credential-free `PUT` and completion work as for every NETT
  upload. Completion returns 201 the first time, 200 on a replay and 202
  while checking. A reservation whose bytes did not arrive waits for a retry
  under the same key, and once the bytes may be stored every failure, a
  refused sign-in included, is resolved by a check that completes the same
  transfer.
- The IRC message carries only the reference and display metadata, as
  `UPL-<22 characters> name (N bytes)`.
- Resolving a pasted reference reads `GET /chat/uploads/{reference}`. NETT
  answers unknown, deleted and expired references with the same 404, which
  the card shows as missing.
- Opening the content posts `POST /chat/uploads/{reference}/grants` and reads
  the grant URL without credentials, with the same type, length and
  `nosniff` checks as issue files.
- The desktop neither cancels nor deletes chat uploads: a reservation that
  is never completed expires, and a completed upload is deleted by NETT
  after 30 days.
- A 429 or 503 waits at least the `Retry-After` NETT sends. A check asked
  for before a wait NETT gave for the draft's completion ends sends
  nothing at all: no reference read, no reservation and no completion.

## Moves

With `issueMoves`, Issues can move an issue to another team.

- The preview posts `POST /issues/{ref}/move-preview` with
  `{destinationTeamId, version}`. It lists the issue first and then every
  sub-issue that moves with it, because sub-issues always move with their
  parent. NETT numbers the moved issues when the move commits, so each
  row's `newIdentifier` is null and the editor shows that the number is
  allocated on move. The preview also returns a token and a `reviewContext`.
- Confirming sends `PATCH /issues/{ref}` with `{teamId, version,
  reviewContext, movePreviewToken}`. NETT moves the whole previewed subtree
  at once and returns the issue. The client then checks every previewed
  issue: each must now be in the destination team, numbered there, at a
  newer version than previewed. If the reply is lost or does not show
  the root moved, the move is unconfirmed and **Check issue** repeats
  only this check, never the move. A root still where it was means the
  plan did not happen, and the editor offers a fresh preview; a root that
  cannot be read leaves the move to be checked again. NETT's reply is this
  plan's own, so a reply showing the root in the destination proves the
  move committed. A check's read of the root there could be a teammate's
  own move of it, so the plan's sub-issues must tie it to this plan:
  because NETT moves the whole subtree at once, at least one must read
  back moved with the root and none may still be in the source team. A
  sub-issue still there, or none moved once every one was read, means the
  plan did not happen, a conflict; with none moved and some not read, the
  move is left to be checked again. A plan without sub-issues is the root
  alone. A large move's check reads sub-issues only while time is left,
  and one read back moved is enough to tie it. Once the move is proven,
  a sub-issue read back somewhere else was moved again since, and one
  that cannot be read may have been, so neither undoes its success. The
  editor lists each such sub-issue, in a list that scrolls however long it
  is so Open stays in view, with what reading it found before it opens
  the moved issue: moved again since, only for one newer than
  previewed in a third team; not confirmed in the destination yet, for a
  stale or unchanged read; could not be read back; or not read back in
  time. Reading a move back takes at most 20 seconds in all, waiting for a
  place among the account's requests included, each read cut to the time
  left and never shared with a read already in flight, which could predate
  the move. Only a reply about the sub-issue itself can show it moved
  again. A read that finds the account or organisation
  changed ends the check with a request to sign in, and nothing it found
  is returned. A 503 on the move
  itself may hide a move that committed, so it too is unconfirmed, to be
  checked; a 503 on a preview asks for a wait like a 429.
- A 409 means the plan changed, and the editor asks for a fresh preview.
  NETT refuses a move that would take an archived sub-issue with it, and a
  NETT build's editor shows that refusal plainly. The default build keeps
  moving archived sub-issues with their parent, as Subspace API does.

## To-dos

NETT issues have named lists of items. In a NETT build the issue view shows
each list with its done and total counts and a checkbox per item. Checking
an item sends `PUT /issues/{issueId}/lists/{listId}/items/{itemId}` with the
item's version, and the saved item must come back from the same list at a
newer version. A 409 means someone else changed it, so the lists are read
again and the checkbox shows the current state; a 429 or 503 says NETT is
busy. A list read that started before a check was saved never undoes it,
and a save whose answer arrives late never undoes a newer version read
meanwhile: an item is only ever replaced by a newer version. Each save
holds its item's pending mark, and only its own answer clears it, so a
late answer never re-enables a checkbox another save still holds. A save
stored but answered after the issue was opened again or refreshed is
reconciled with a fresh read of the lists.

## Live updates

With `updates`, the hub follows NETT's update stream, using the desktop's
existing stream client.

- **Request.** `GET /api/v1/updates?project=KEY`, repeated for 1 to 64 team
  keys, with `Accept: text/event-stream`, the bearer and exactly one
  `x-org-id` naming the organisation. Team keys are unique only inside an
  organisation, so the header is required; a NETT client without an
  organisation opens no stream.
- **Events.** `event: ready` with `data: null` once the subscription is
  active, then `event: issueChanged` with `{projectKey, issueId, identifier,
  parentIdentifier, version, action, auditId?, actorType, occurredAt}`,
  where `projectKey` is the team key and `auditId` may be absent. A
  `: keepalive` comment arrives every 15 seconds.
- **Hints only.** Each event is a hint: the hub coalesces them and re-reads
  the open issue and the list head through REST, with its existing spacing.
- **SSE only.** NETT builds never poll. Without a stream, Issues says live
  updates are off. The default build keeps its 30-second fallback poll.
- **Server closes.** NETT closes the stream at token expiry, after three
  minutes, when the person loses access, and when the stream's queue
  overflows, and a connection can break. After any end of a live stream the
  hub first catches up through REST (the open issue and the list head), and
  waits for those reads, then reconnects with jittered backoff. A close
  also refreshes the capability list and the initiative catalogue, since it
  may follow a change in what the person can see.
- **Refusals.** A 401 renews the token once and reconnects; a second 401
  stops the stream until the account or organisation changes. A renewal
  counts only when the provider now holds a different token: a refresh that
  failed leaves the refused one, which is not tried again as if it were
  fresh. One account's renewal never stands for another's. The stream
  and the hub's reads share that renewal: a read refused with 401 renews
  the token once, and once it is renewed, by either, the list and the open
  issue are read again with it. A 401 a renewal answers never clears what
  is shown or asks the person to sign in; only a 403, a renewed token
  refused again or a renewal that fails does. A 403 reads again what the
  person may see: the capability list, the initiatives, the open issue and
  the list head; an issue or list the person can no longer read is
  cleared, and reads already out for it, such as its counts, pages or
  attachments, are retired with it. A second 403 in a row stops the stream until the
  followed scope or the capability list changes, whichever read finds the
  change, so a lasting refusal never turns into polling and access found
  again by a reload brings the stream back. A 404 means a requested team is unknown or archived, or the
  organisation no longer serves the stream: the hub stops streaming for the
  rest of its session and reads the capability list and team catalogue
  again. A 429 or 503 waits at least the whole seconds in `Retry-After`, up
  to five minutes, which acts as a floor under the jittered backoff and
  holds even when the followed teams change meanwhile.

## Verification without NETT

No test calls a real NETT environment.

- `scripts/nett_api_test_server.py` is a loopback, memory-only mock of the
  NETT routes the desktop uses, built from NETT's OpenAPI document, plus a
  fake WorkOS device authority and a credential-free file host that refuses
  any request carrying a bearer, cookie or organisation header.
  `scripts/test-nett-api-test-server.py` tests it and, given the OpenAPI
  document, checks every existing route and response property against it.
- The mock implements every route of every feature it advertises:
  initiative discovery (with fleetless, nested and archived initiatives),
  standalone attachments, chat uploads, moves and the update stream, from
  dedicated contract fixtures. Every organisation route needs exactly one
  matching `x-org-id` (403 when missing or different), each token acts as
  its own person, and a stream rechecks its bearer and the person's access
  before every flush. Test hooks set the features per organisation, make
  completion answer 202 first or managed media ready at once, fail, delay
  or drop a chosen request, drop a PUT's reply, expire reservations, refuse
  grants to a second account, revoke a token or a membership, hide a
  project from one person, close or refuse streams and archive a team or an
  issue.
- Client unit tests cover the path, query and body translation, the JSON
  mapping, organisation selection, the grant cache and flow control.
- End-to-end client tests run the actual adapter operations against the mock
  for organisation selection, reads, writes, to-dos, every advertised
  feature and the update stream. They check headers, nullable fields,
  cursors, conflicts, pending completion, that the file host receives no
  credentials and that the bytes are sent exactly once, including after a
  lost PUT reply, a lost or refused completion reply and a long check, for
  standalone attachments, comment media and chat. They also cover a stored
  upload whose check is refused after the issue was archived, a proven
  absence, a check after a refresh, a lapsed reservation, a final refusal,
  a wait held for five minutes at most, a day-long wait that a reload
  ends, and comment media whose lease ends between its
  reservation and its bytes. Hub tests cover the stream's reconnect,
  refusal, repeated 403, lost access, broken connection and catch-up paths,
  a `Retry-After` kept across a team change, counts and
  attachment lists still out when access goes, the absence of polling,
  standalone attachments and to-dos, including a late answer racing a
  newer save. Separate test processes bind the client to a native account:
  one covers a context, issue, grant, managed media, to-do or move-preview
  reply that arrives after the account or organisation changed, sign-out,
  two people with different project visibility, uploads prepared for the
  previous account, a capability list read again for a new account and a
  day-long wait that choosing an organisation ends; one renews a refused token through the registered
  renewal and gets the list, the issue and the stream back; one switches
  the shared initiative catalogue from an organisation that serves
  initiative channels to one that does not. Channels tests an organisation
  switch with a read still in flight, and Issues tests that a chosen file
  goes only to the issue opening and hub it was chosen in, and that a
  refused file keeps its own message.
- The Framework's provider tests cover device sign-in, including the
  deadline-aware exchange, interval and `slow_down` delays, form-encoded
  refresh, an organisation-bearing reply and an account switch.
- A test running the product's launch resolution as a NETT build, with the
  Subspace API fixture variables set, proves that native initialisation and
  every construction path, Channels included, reach only NETT and send
  nothing to Subspace API. Product tests cover both stamped variants and
  every per-variant setting.

## Delivery plan

| Step | Exit check |
| --- | --- |
| 1. Backend setting, validation and the kill switch | Default build unchanged; NETT build refuses Subspace settings; zero Subspace clients in NETT. |
| 2. Device-flow sign-in in the Framework | Offline provider tests for pending, slow down, expiry, denial, cancellation and refresh. |
| 3. NETT client: scope, reads, writes, decisions, media, to-dos | Unit tests and the end-to-end mock tests. |
| 4. Capability gating and the organisation menu | Space tests; Channels hides uploads, notices and its initiative picker. |
| 5. Coming features: initiative channels, attachments, chat uploads, moves, updates | The mock's contract fixtures and adapter tests for each. |
| 6. Acceptance against a NETT environment | Sign-in, organisation switch, reads and writes, media, live updates, IRC with SASL. |

## Deferred

- **Captain notices and the issue delegate.** An agent delegate on NETT
  issues is out of scope, so captain notices stay off in NETT builds.
- **Repositories.** NETT's repository mapping is not used yet.
- **Other advertised features.** Subscriber lists, marking a notification
  unread and issue document archiving are ignored.
- **Milestones.** NETT milestones belong to planning projects, not teams,
  so the milestone picker is empty in a NETT build.
- **Release builds.** A NETT build turns the updater off and enables Issues
  by default. A separate bundle identifier and release channel wait until
  NETT builds go to anyone but the owner.

## Open questions

- Whether the first non-owner NETT build needs its own bundle identifier and
  update channel.
