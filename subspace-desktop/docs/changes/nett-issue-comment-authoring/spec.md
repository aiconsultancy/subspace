# Comment authoring contract

[Intent](intent.md) · [Design](design.md) · [Tasks](tasks.md) · [Delivery](delivery.md)

Status: approved with the captain's 15 November amendments; PR A authorised.
Applies equally to issue and tracker detail views. Comments remains the initial
view; choosing All never resets a composer or its draft.

## 1. Evidence and source boundary

Indra source was read from freshly fetched `origin/main` at
`d849f70060aa68fad1292f53ecc12dd6dc203fad` on 15 November 2026, using the
task's permitted read-only Indra checkout. All Indra links below pin that
revision. Existing Subspace behaviour is the assigned base
`52d27687946306f916e0e70d4bba1a5158b07858`.

| Reference | Source and confirmed behaviour |
| --- | --- |
| I1 | Comments/Create/Endpoint.cs, request, validator: POST, manage permission, closed fields, 201; write followed by an independent response read. |
| I2 | Shared/IssueCommentWriter.cs: authority/lifecycle checks, meaningful Markdown, media binding, draft reuse conflict, ordinary author subscription and server notifications. No REST retry key/receipt is supplied. |
| I3 | Domain/IssueTracking/IssueComment.cs: LF normalization, 50,000 UTF-16-unit limit, one reply level, no reply to a deleted/resolved root. |
| I4 | IssueCommentConfiguration.cs: unique `(org_id, media_draft_id)` for every non-null draft, including deleted comments. |
| I5 | IssueMediaContent.cs, IssueMediaBinding SQL: an empty reference set with a new non-empty draft is valid; current actor/context is fenced even without images. |
| I6 | Media/Contracts.cs, Prepare, Complete, Cancel, ReadUpload, Read: exact feature routes and closed requests. |
| I7 | IIssueMediaService.cs, IssueMediaService.cs, completion, cancellation, context SQL: stable Prepare key/declaration, upload identity, states, consumption, cancellation and authority. |
| I8 | ManagedFileDeclaration.cs, IssueMedia.cs: types, 25,000,000 bytes, image bounds and seven-day unbound Ready expiry. |
| I9 | IssueCollaborationTypes.cs, IssueCommentReader.cs: flat save DTO, comment fields and cursor ordering, default 20/max 50. |
| I10 | ManagedMediaMarkdown.cs, MarkdownLimits.cs: semantic `nett-media:` references, escaped labels, parser nesting rejection. |
| I11 | NewCommentForm.tsx, ThreadReply.tsx, IssueActivitySection.tsx, MarkdownSourceTextarea.tsx: compact composer below timeline, reply beneath root/thread, posting lock, failure notice/focus; Enter is not a submit binding. |
| I12 | managedMediaUpload.ts, nettUploadAttempt.ts, uploadFileToNett.ts, IFileUploadTransport.cs: immutable upload intent, recover-before-replace, transfer URL validation, status polling, descriptor shape. |
| I13 | useIssueAccess.ts, 40.5 frontend spec, 40.5 devdocs, feature-pack notes: permissions, intended interaction and notification side effects. Current handlers take precedence over older spec defaults. |
| I14 | EndpointResultExtensions.cs: the create endpoint's `SendFailureAsync` maps Conflict to 409 ProblemDetails and joins error strings into `detail`; draft reuse is exactly `detail: "IssueMedia.DraftAlreadyUsed"`. |
| I15 | ManagedMediaContentValidator.cs: image MIME/decoded format agreement, bounded dimensions and exactly one frame. |

Source confirmation is not live acceptance. Section 12 lists the remaining
uncertainties; no worker request to NETT, WorkOS or file delivery is authorised.

## 2. Profile, identity and permissions

Use the selected profile's immutable API origin, bearer session and captured
organisation for every API call; send `X-Org-Id` matching the path. Resolve the
open issue's canonical UUID once; never retarget an in-flight write through a
renamed identifier or the newly selected organisation. Session generation
binds drafts to the signed-in account. No actor identity read or comparison is
needed for the draft fence; do not add an auth/context request.

The existing `GET /api/v1/auth/orgs` result exposes `permissions` and
`featurePacks` ([client model](../../../clients/nett/src/model.rs),
[client call](../../../clients/nett/src/lib.rs)). A supported signed-in NETT
profile with selected eligible organisation, `issue-tracking`, `issues.view`
and `issues.manage`, and an unarchived issue and team may compose. Permissions
come from these existing organisation facts. Unknown permissions hold
authoring until that read succeeds. Known viewers see a quiet
explanation, “You can read comments, but you don't have permission to post.”
An existing dirty draft remains selectable and retained if manage permission
is later lost. Archived issue/team gets its own explanation; reads remain.

There is no separate upload capability in the inspected issue-media contract.
Upload requires that same current issue write authority; quota, declaration,
transport availability and readiness are decided by Prepare/Complete. Never
infer upload permission from file-read access or role names. A write/upload 403
preserves the draft, disables the refused action and explains that posting or
uploading is not permitted. Refresh organisation facts on deliberate Retry or
Reload. A write 403/404 does not run the read-refusal path: keep the issue,
timeline and saved images until their own read says access has gone. Upload
refusal alone need not disable text posting after the attachment is removed.

The standalone **Subspace API** profile supports neither comment writes nor
uploads in this repository's current client contract. `ApiProfile::standalone`
is a placeholder with `AuthKind::CloudflareAccess`; `Client::new` and session
sign-in reject it ([profile](../../../clients/nett/src/profile.rs),
[client](../../../clients/nett/src/lib.rs),
[session](../../../clients/nett/src/session.rs)). It remains hidden. If a
fixture/future selector exposes it, replace the composer with “Posting comments
isn't available for this profile.” Do not probe NETT-shaped routes or send its
credentials to NETT. Enabling it requires a separate supported write contract.

## 3. Comments and replies API

All paths below are relative to the selected API origin, include `/api/v1`,
and use the existing org-scoped authentication. JSON names are exact.

| Call | Request and result |
| --- | --- |
| `POST /api/v1/orgs/{orgId}/issues/{issueId}/comments` | Root: `{ "body": "…", "mediaDraftId": "<non-empty UUID>" }`. Reply adds `"parentCommentId": "<root UUID>"`. 201 returns the flat `CommentSaveResponse`. |
| `GET /api/v1/orgs/{orgId}/issues/{issueId}/comments?limit=50&cursor=…` | Root `CommentPage`: `items`, nullable `nextCursor`, newest first by `(displayCreatedAt, id)` descending. Omit cursor initially. |
| `GET /api/v1/orgs/{orgId}/issues/{issueId}/comments/{rootId}/replies?limit=50&cursor=…` | Same page shape; replies oldest first by `(displayCreatedAt, id)` ascending. Use its own forward cursor, never a root or location cursor. |
| `GET /api/v1/orgs/{orgId}/issues/{issueId}/activity` | Existing All timeline refresh; preserve its paging and ordering. |

Comment creation accepts `body` (required), optional `parentCommentId`,
`provenance`, `resolved`, `mediaDraftId`. Unknown fields are refused; optional
IDs sent as null/empty are invalid. Subspace sends only body, its non-empty
draft UUID, and the parent UUID for a reply. Omit parent on a root. Never send
`provenance`, `resolved`, actor, timestamp, comment ID, version or a fabricated
idempotency header. Provenance is an import operation with extra configure
authority; resolved-on-create is only for imported roots. [I1–I3]

Normalize CRLF and CR to LF, preserving other text and whitespace. Require
meaningful Markdown or at least one valid Ready image block, no NUL, and at
most 50,000 UTF-16 code units **including generated attachment blocks**.
Whitespace or formatting-only text cannot post. Server validation also rejects
excessive Markdown nesting, invalid managed references and more than 100
distinct semantic mentions; field errors stay beside the editor. Mention
picking is absent; typed `@name` is ordinary text and existing semantic mention
syntax is not rewritten into invented identities. [I2, I3, I10]

`CommentSaveResponse` is a comment object with `outcome` and optional
`mentionOutcome`, **not** `{comment: …}`. The comment includes `id`, `issueId`,
`parentCommentId`, `body`, `version`, `actualAuthor` (with `userId`),
`createdAt`, `displayCreatedAt`, edit/delete/resolution facts, reply counts,
`provenance`, `media` and `mentions`. Read unknown fields compatibly. Require
the returned issue and parent to match the captured request
before treating 201 as confirmed; malformed/mismatched success is uncertain.
`outcome` is currently `created`; no REST replay response is promised. [I9]

## 4. Draft lifecycle and submission state

The hub owns memory-only drafts keyed by `(profileId, orgId, canonicalIssueId,
thread)`, where thread is root-composer or a root-comment UUID. Store current
session generation with each key. The board retains editor/focus state;
it does not own the only copy of the draft. Switching issues or closing a
detail view within the same scope preserves text, attachment bytes and any
uncertain attempt. Switching Comments/All also preserves them. One visible
reply composer per open issue is sufficient; switching threads keeps both
drafts under their separate keys.

Clear drafts, bytes, upload/grant capabilities, identity and pending result
projections at sign-out, session expiry, actor change, profile or organisation
change, hub stop and process exit. Increment the scope generation, cancel
local tasks, and ignore late results before rendering or sending another
step. Never serialize drafts/bytes into preferences, files, caches, search,
chat/agent context, crash reports or logs. File-picker source paths are not
retained after their bounded read; never write clipboard bytes to a temp file.
At a scope boundary, best-effort cancellation may only use the old captured
scope while still authorised; never preserve a secret cleanup queue across
sign-out or send cancellation under the new account. Server cleanup handles
remaining abandoned uploads.

States: editing → posting → confirmed, known-failure, or outcome-unknown.
Retry of an unknown attempt returns to posting with its frozen intent. Take the per-draft write lock
synchronously before queuing work; pointer, key repeats and a second view of
the same draft share it. At most one submit is in flight for that draft.
Freeze the sent body, target and attachments while posting/unknown; keep
them selectable. Navigation may leave the issue; completion changes only its
captured draft, never the newly open editor. A known failure restores editing
with its text/caret/attachments and Retry where applicable. An uncertain
attempt stays frozen until Retry confirms it or the person chooses Discard.

Generate one random non-empty `mediaDraftId` per new comment intent, including
text-only comments. Reuse it on every retry, including corrected text after a
known refusal; rotate only after confirmed success or deliberate discard.
Discard of an unclear intent warns “This comment may already be posted.”
After discard a new draft for that thread receives a new UUID immediately;
do not retain an unresolved intent that blocks it. No crash recovery is promised
because nothing is on disk.

## 5. Duplicate safety through the draft fence

REST comment creation has no general retry key. However, I2 explicitly refuses
an already used `mediaDraftId` with `IssueMedia.DraftAlreadyUsed`, I4 enforces
unique organisation/draft identity, and I5 accepts it for text-only posts.
Every UUID is generated randomly and retained only in memory. A retry's 409
ProblemDetails with exact `detail: "IssueMedia.DraftAlreadyUsed"` therefore
proves this intent's earlier attempt saved. The endpoint uses `SendFailureAsync`
and I14 confirms this response shape: `status:409`, `title:"Conflict"`,
`type:"https://httpstatuses.com/409"`, `detail:"IssueMedia.DraftAlreadyUsed"`,
plus `instance` and `traceId`. Do not expect a media-specific `code` field.

For any unclear outcome, freeze body, parent and UUID and show:
“We couldn't confirm whether your comment was posted. Retry won't post it twice.”
Offer **Retry** and **Discard**. Retry sends exactly that frozen intent:

- 201 validates the issue/parent and confirms the new post; project the response.
- 409 with that exact detail confirms the previous post. Clear this draft, say
  “Your comment was already posted.” and refresh the discussion to show it.
- Anything else, including 403, 404, another 409, 5xx or a lost connection,
  stays unclear and frozen. Keep Retry/Discard and add a read-only
  **Refresh discussion**. A now-resolved root is one such refusal; never change
  the UUID or unresolve the thread to make a retry work.

Discard warns “This comment may already be posted.” Confirmation discards the
local intent and permits a new UUID, as section 4 specifies. It never deletes
a saved comment. A recognized pre-save validation refusal is a known failure
and permits correction with the same UUID. Even 403/404 can originate in the
response read after commit (I1), so ambiguous refusals use the frozen flow.
No automatic POST retry, refresh-middleware replay, matching, timestamp floor,
discussion traversal or identity read is part of this mechanism.

## 6. Status handling

Only structured, bounded, recognized validation codes become field messages.
Never surface arbitrary HTML, provider detail or transport URLs. Each failure
keeps the draft except actual session expiry, whose privacy clearing wins.

| Result | Comment handling | Media handling |
| --- | --- | --- |
| 201 | Validate flat DTO, project it immediately; clear only this confirmed intent. | Unexpected: retain state and read status before recovery. |
| 200 | Unexpected for comment create: frozen unclear flow in section 5. Reads accept normal pages. | Apply typed state; only `ready` plus descriptor is usable. Cancel reads its typed retention result. |
| 202 | Unexpected comment success: frozen unclear flow. | Completion is still `checking`; poll status, never insert an image as Ready just because bytes reached 100%. |
| 400 | Known body/parent validation: explain and keep editable. Unrecognized or post-read error: uncertain. | Fix declaration/target/type/size; retain bytes. No unchanged blind Retry. |
| 401 | Existing bounded token refresh may prepare credentials, but never replay the POST. If session is expired, clear private scope and offer sign-in. | Same session boundary. An edge challenge is separate and never follows redirects or forwards credentials. |
| 403 | Keep draft and reads; explain refused posting and refresh write facts. If commit cannot be excluded, use section 5. A retry remains unclear. | Keep bytes/text, explain upload refusal and disable upload until facts refresh; saved image reads stay. |
| 404 | Issue/thread/pack unavailable; keep existing reads and use frozen unclear flow. | Target/upload may be unavailable; resolve the original key/identity first. Do not allocate a replacement for an unknown attempt. |
| 409 | Exact `detail:IssueMedia.DraftAlreadyUsed` confirms the earlier post. Known lifecycle conflicts explain the reason; a retry with any other conflict remains frozen/unclear, with Refresh discussion. No resolution mutation. | Handle code: idempotency mismatch is a client defect, do not change the key; draft-used uses the comment fence; version conflict reads current upload before cancellation; state conflict rechecks/completes same upload; quota exceeded waits for storage to be freed; reconciliation-pending offers later Retry; expired/terminal state needs confirmed closure before new intent. |
| 408 / 429 | Frozen unclear flow. Honour valid Retry-After, no automatic repost. | Retain original key/IDs, honour Retry-After and recover state on Retry. |
| 413 / 415 | Explain too-large/unsupported request; no unchanged known-failure Retry. If not known to be pre-write, use frozen unclear flow. | Explain size/type, retain row/bytes; remove or choose a supported file after original intent is closed. |
| 5xx, including 503 | Frozen unclear flow; never assume rollback. | Temporarily unavailable, retained same-key intent; Retry resumes it after status read. |
| Network/timeout/abort after dispatch; invalid/oversized response | Frozen unclear flow with same-intent Retry/Discard. | Outcome unknown at that step, recover original intent. |
| Redirect/HTML/other status | Never follow it. Explain edge access if recognized; otherwise hold outcome unknown after dispatch. | Never PUT/forward credentials elsewhere; retain original intent and recover via authenticated feature API. |

Reads are retryable after transient network/5xx/429 failures, without mutating
anything. Permission/lifecycle/validation failures
need changed facts or edited input before a deliberate retry.

## 7. Image intake, upload and removal (PR B)

Allow native **Attach image…** and clipboard image paste while the composer is
editable. Keep normal text paste, including text accompanying a clipboard image.
Accept `.jpg`/`.jpeg` `image/jpeg`, `.png`
`image/png`, `.webp` `image/webp`, with matching declarations and verified
bytes. GIF, SVG, HEIC, PDF and other files are outside this image-only change,
even though existing readers can render some other types. Clipboard raster
data with no supported encoded representation becomes PNG **in memory before
hashing**; the declared and sent bytes must then be identical. Do not re-encode
file-picker bytes. Give converted clipboard images a safe synthetic `.png`
filename; supported encoded clipboard images keep their encoding and receive
a safe synthetic filename with its matching extension.

Prepare's server limit is **1–25,000,000 bytes**, SHA-256 is exactly 64 lower-case
hex characters, and image validation bounds are 8,192 per edge and 16,777,216
pixels (I8). Images must be single-frame: animated PNG/WebP is rejected too;
the server verifies decoded format agrees with the declaration (I15).
Subspace deliberately accepts at most **8 MiB = 8,388,608 bytes**
per image to match its current bounded reader (`clients/nett/src/media.rs`).
Preflight both encoded bytes and decoder limits: Subspace's existing
`framework/board-kit/dashboard/src/images/decode.rs` also caps 8,192 per edge,
**8,388,608 pixels**, and 64 MiB decode allocation, so those smaller client
bounds apply. Say “Choose a JPEG, PNG or WebP image up to 8 MiB.” This is a client
limit, not a claim that NETT Prepare has an 8 MiB limit. Proposed local memory
budget: eight images and 32 MiB encoded bytes per draft, 64 MiB across drafts;
reject new intake with an inline explanation rather than evicting saved drafts.
Decoder work remains off the UI thread and within the existing decoded budget.
Give the PUT a size-scaled global timeout: 30 seconds plus one second per
started 32 KiB of encoded bytes (286 seconds at 8 MiB). API requests retain
their existing timeout; retries still recover the same intent before transfer.

| Step | Exact protocol |
| --- | --- |
| Prepare | `POST /api/v1/orgs/{orgId}/issues/media/prepare` with `requestKey` (non-empty UUID), `target: {kind:"issueCommentDraft",issueId,draftId}`, `fileName`, `contentType`, `sizeBytes`, `sha256`. Omit other target IDs. `draftId` is this comment's `mediaDraftId`. |
| Prepare result | 200 `IssueMediaUploadState`: `mediaId`, `uploadId`, `version`, `status`, `transferDeadline`, nullable `expiresAt`, `failureCode`, `media`, `upload`. `upload` is nullable and may be absent after transfer/completion. |
| Transfer | If still prepared and a valid descriptor is issued, PUT exact bytes to `upload.uploadUrl` with its `contentType`/size and `method:"PUT"`. Descriptor fields are `uploadId`, `uploadUrl`, `method`, `contentType`, `sizeBytes`, `expiresAtUtc`. No bearer, cookies or org header. |
| Complete | `POST /api/v1/orgs/{orgId}/issues/media/from-upload` with `{uploadId}`; 200 or 202 returns the same upload-state shape. |
| Status/recovery | `GET /api/v1/orgs/{orgId}/issues/media/uploads/{uploadId}`; 200 returns upload state but never reissues a PUT descriptor. Same-key Prepare renews a still-valid original intent's transport. |
| Optional known-media read | `GET /api/v1/orgs/{orgId}/issues/media/{mediaId}`; `mediaId`, `version`, `status`, `failureCode`, nullable `media`, `grant`. Keep grant in the private client boundary. |
| Cancel | `PUT /api/v1/orgs/{orgId}/issues/media/{mediaId}/upload` with `{version:<latest positive uint>,cancel:true}`. 200: `mediaId`, `version`, `outcome`, `retention` (`cleanupPending`, `retained`, `cleaned`). |
| Bind to post | Only Ready images become `![<escaped label>](nett-media:<canonical lower-case media UUID>)` in the submitted body. Send the same draft UUID on comment create. Response `media` descriptors feed the existing private reader. |

Upload statuses: `prepared`, `checking`, `ready`, `rejected`, `cleanupPending`,
`cleaned`. A Ready descriptor has `mediaId`, `version`, `kind`, `fileName`,
`contentType`, `sizeBytes`, nullable `expiresAt`, `publicFileId`; an opaque
public file ID confers no authority. Ready unbound media expires after seven
days; transfer reservations start with a 30-minute deadline. Use returned
deadlines rather than extending either on Retry. Check state/readiness again
when a retained draft is reopened after a long interval. Bound media loses
its draft expiry and belongs permanently to that saved comment. [I7, I8]

Represent each attachment with a stable local ID, immutable declaration/key,
bytes, media/upload IDs and version, progress and error. Show Preparing,
Uploading (bytes/percent if measurable), Checking, Ready or Failed with its own
Retry/Remove controls. Failed rows do not clear other rows. Poll checking
every two seconds, at most 30 checking poll reads per explicit run, only while
that issue is visible; then offer **Check again**. Retry/Check again's single
initial recovery read is separate from those 30 polls. A status read alone need not advance
scanning; Check again can call Complete for the same upload. Submit waits for
every included attachment to be Ready or removed. Attachment-only posts are
valid because the body contains a meaningful media block.

The attachment retry key is created once for the immutable target, name,
type, size, hash and bytes. Retrying or selecting/pasting the same declaration
and hash again in that draft reuses the open row/intent, never creates a
second reservation or block. A changed declaration must not reuse a key.
Prepare uses an actor-scoped operation key and declaration hash: replay gets
the same upload, mismatch gets `IssueMedia.IdempotencyConflict`. [I7, I12]

After uncertain Prepare with no IDs, replay the original key/declaration to
learn its IDs. After uncertain PUT or Complete, read that upload and resume
only the missing step. A PUT 409 may mean the original object already exists;
Complete verifies it instead of creating another upload. A transfer URL 404
can be renewed once with the same Prepare key while the original deadline
allows it; 413 rejects the file, 5xx/network retains the attempt. Never allocate
a new key just because an acknowledgement or capability was lost. Only a
confirmed closed/expired original and a deliberate new intake allow a new key.
After an unacknowledged Remove, the same-key memo below preserves the identity
needed to establish that closure before deliberate re-intake can allocate a
replacement.

Remove before posting detaches the block immediately and cancels that image's
upload intent, **not the whole comment draft or other attachments**. Abort
local transfer, recover unknown Prepare IDs with the same key if necessary,
read the latest version, then Cancel. A late completion must not reinsert it.
Make one best-effort Cancel with the latest version, rereading once and retrying
once on a version 409. If it still fails, drop the row locally; server expiry
cleans unbound uploads. Keep no cleanup UI or Retry-cleanup control. Keep only
a small memory-only **same-key memo** in this draft when Cancel was not
acknowledged: declaration hash → request key, plus any known media/upload IDs
and whether Prepare was dispatched (so cleanup never creates an unsent intent).
The hash covers the immutable name, type, size and content hash. The memo holds
no bytes, has no row, counts towards no row/byte budget, is never logged or
persisted, and clears on confirmed post, Discard or any private-scope clearing.
Any valid 200 from Cancel, whatever its retention, confirms closure of the
original intent: keep no memo, and later deliberate intake gets a new key.

Deliberate re-intake of the same declaration/hash reuses its memo key and
replays Prepare. A matching live `prepared`, `checking` or `ready` result
resumes that original upload as a row: use a returned transfer descriptor,
Complete/poll checking, or use the Ready descriptor. A closed `cleanupPending`,
`cleaned` or `rejected` result, or an expired transfer that cannot renew,
confirms closure: discard the memo and allocate a new key for this deliberate
intake. An uncertain replay keeps a Failed row with Retry on the memo key;
it never allocates a new key. A different name, type, size or content hash
never uses the memo key. Late completion alone never reinserts a removed row.
Do not show “cancelled” until acknowledged.
`cleanupPending` is accepted cleanup, not immediate quota release. `retained`
means a post already bound it: refresh the discussion and never delete saved
content. While comment posting/uncertain, freeze attachment removal until that
post is resolved. Unbound leftovers after scope clear use server cleanup.

## 8. Privacy and transport

The [workspace privacy rules](../nett-issue-workspace/spec.md#8-privacy) apply
to writes too. Bodies, filenames, raw server errors, hashes, byte buffers,
upload/read URLs and grant tokens never reach logs, diagnostics, the clipboard,
chat, search, notifications or disk. Copying user-selected draft text follows
normal text selection; no action copies transport URLs. URLs never appear in
attachment tooltips, errors, screenshot fixtures or accessible labels.

Use redacted Debug implementations for capability-bearing objects. Do not
send opaque upload PUTs through request logging that could expose a path or
query capability; the existing API path/status/duration logging policy may
continue without body/query logging. Validate upload origin against the
profile's independently pinned HTTPS files origin, exact `/u/{uploadId}` path,
one non-empty `grant` query, no credentials/fragment, expected method, matching
upload identity, type, byte length and unexpired expiry. Never trust the
descriptor to supply its own allowed origin. Follow no redirects. Debug
fixture configuration alone permits its explicit numeric loopback origin.

Image previews and posted images use existing bounded decoding/private grants;
no raw upload capability crosses into board UI. The failure path maps safe
codes to authored copy. Clear all bytes and capabilities with scope generation.

## 9. Timeline, replies, keys and focus

Root composer stays below the timeline/paging in either view. A quiet labelled
**Reply** action appears below each non-deleted unresolved root's thread in
Comments and beneath the corresponding root block in All. It is keyboard
reachable, not hover-only. It opens a compact composer immediately beneath
that thread, labelled “Reply to …”, and focuses its editor. Replies never get
their own Reply action: NETT supports root → replies, exactly one level. A
resolved root says “This thread is resolved”; no Unresolve command is added.
An existing reply draft is retained but held if the thread resolves/deletes.
Read expansion and pagination remain available. [I3, I11]

NETT's compact Markdown editor uses Enter for a new paragraph/line and an
explicit Comment/Reply button; no Enter-to-post binding was found in the form.
IRC's shared `ChatInput::message` uses Enter to send and Shift+Enter for a
newline ([shared input](../../../framework/board-kit/dashboard/src/chat_input/message.rs)).
They agree on Shift+Enter for a newline, but disagree on plain Enter. Choose
NETT's multiline behaviour here: **Enter and Shift+Enter insert a newline**,
**Command+Enter on macOS / Control+Enter elsewhere submits**, and the labelled
**Comment** / **Reply** button is always available. The modifier shortcut is
an approved Subspace addition, not a claimed NETT shortcut. Show its hint;
IME composition never submits. Share the existing input controls through an
explicit submit-binding option, preserving IRC's existing behaviour.

Focus order follows timeline, root editor, attachment controls, submit; an
open thread's reply editor/actions sit in its thread order. On a confirmed
root post clear that editor and return focus to it. A confirmed reply closes
its inline composer and returns focus to that root's Reply action (or the
named thread if no longer actionable). A known or uncertain failure focuses
the nearby notice; Tab reaches Retry/Discard and the retained editor.
Keep the caret for returning to edit. Never steal focus if the person has
left the issue while the request ran. File-picker cancellation restores focus
without changing the draft. Status/progress and errors have accessible text.

Escape first dismisses the topmost picker/menu/viewer and restores its trigger.
With none open, Escape in an inline reply closes that surface, retains its
draft/attachments and restores its Reply trigger; reopening restores them.
Escape in the root composer returns focus to the timeline without clearing
anything. While posting it cannot cancel a server request or discard the
frozen intent. Escape is never “discard draft.” Explicit discard clears only
an unsent draft and cancels its attachments; an unclear sent intent requires
the discard warning, as section 4 specifies.

## 10. Confirmed post and refresh

Project a valid 201 DTO immediately, keyed/deduplicated by canonical comment
ID, with its body, actual author and images from returned descriptors. A reply
updates its root's visible reply count and is revealed in that thread. Under
All insert the returned comment at its `displayCreatedAt`/ID position, then
replace the temporary projection with the authoritative activity entry on
refresh; never show both. Preserve Subspace's existing visible chronology:
oldest to newest, with replies beneath a visible root in Comments and as
individual ordered entries in All. The root-read endpoint's newest-first
wire order is for pagination, not a reason to reverse the UI.
The returned comment has no activity ID: key its provisional entry separately
and deduplicate against refreshed `comment.id`, then adopt the real activity
ID/display time. Do not fabricate an authoritative activity/change ID.

Refresh the issue's roots, affected replies, All activity and images in the
background after the projection. Keep the Comments/All switch, selected issue,
navigation, other drafts and unrelated paging choices. A refresh failure keeps
the confirmed post, says “Posted. Couldn't refresh the discussion.” and offers
read-only refresh Retry; it never retries POST. Don't lose the new item simply
because a stale first page omitted it. Response media can render immediately
through existing private grants; byte transfer completion alone cannot do so.

The server automatically enrols an ordinary first-time commenter unless they
have explicitly unsubscribed, and may dispatch subscriber/semantic-mention
notifications (I2). Subspace adds no notification UI or suppression flag and
makes no promise that a post notifies nobody.

## 11. Delivery and evidence

PR A: text comments/replies, scoped drafts, permissions, typed write failures,
single-flight protection, text-only media-draft fence, safe uncertain-post Retry,
immediate projection and refresh. PR B: clipboard/file intake, bounded memory,
Prepare/PUT/Complete/status/cancel, per-image failure/retry and returned images.
PR B waits for PR A to merge, then starts from updated `origin/feat/subspace-desktop`.

Use TDD: record a meaningful failing test before each behaviour is implemented.
Extend the loopback server and its independent Python tests. Include “commit
then drop response”, late commit, “commit then response-read denial”, request
refused before commit, duplicate draft conflict, uncertain Prepare/PUT/Complete,
cancel version race, permissions and scope changes. Make tests assert actual
server-side comment/upload/block counts after retry, not merely button states.
Model real flat DTOs, cursor order, strict fields and stable keys.
Assert exactly one server comment for commit/drop then Retry/409, pre-commit
refusal then Retry/201, and concurrent requests using one draft UUID. A retry
with a different refusal must leave the frozen draft and Refresh discussion.

Use the fixture hub and capture tooling used by workspace slices 5/7. Capture
Cupertino Dark with synthetic names, wide and narrow detail windows, at most
1600 px wide per PNG; compare spacing/composition with the source-confirmed
NETT desktop composer. Keep the workspace evidence basename style: state,
width, theme; place new frames in this record's `evidence/`:

| Frame | Required state |
| --- | --- |
| `composer-empty-1440-dark.png` | Timeline, Comments selected, empty composer below paging. |
| `composer-draft-1440-dark.png` | Multiline draft, labelled submit and key hint. |
| `composer-posting-1440-dark.png` | Frozen draft with Posting… and no second submit. |
| `composer-failed-1440-dark.png` | Preserved draft, failure reason and safe Retry. |
| `composer-unknown-1440-dark.png` | Fence copy with Retry and Discard. |
| `reply-composer-1440-dark.png` | Root/replies and inline reply composer beneath them. |
| `attachment-uploading-1440-dark.png` | One image's measured upload progress. |
| `attachment-failed-1440-dark.png` | Per-image failure/Retry/Remove, other image retained. |
| `composer-draft-960-dark.png` | Narrow wrapping, focus ring and controls without horizontal overflow. |

Capture a before frame from the assigned integration base and after frames
from each UI branch; use commit-pinned image URLs and captions in delivery
and PR bodies. Test keyboard/focus, reply depth, Comments/All retention and
draft lifetime through real headless component interaction; screenshots alone
do not prove these. Do not mark UI delivery ready without screenshots.

Run `make check`, `mise run quality`, `make interfaces`, `mise run audit` in
the foreground from `subspace-desktop/` for each implementation PR. Baseline
regeneration is allowed only for function-count change alone; report any other
movement. Update `spaces/issues/help.md`, `docs/issues.md`, tasks and delivery.
Publish signed through `createCommitOnBranch`, verify tree equality/clean
checkout, check the captain checkout for hook config leaks, and leave PR
creation to the captain. PR body files follow the task's A/B paths/template.

**Human acceptance remains a captain + jt step:** a person signs in with
WorkOS to an explicitly authorised synthetic NETT dev issue. They verify root,
reply, image paste and picker, failed submit, committed-but-lost response and
retry without duplicate comments/files, displayed images, permissions and
draft scope clearing. Record issue/account scope safely, deployed revision,
outcome/counts and redacted evidence. The worker uses fixtures only and cannot
claim this live check passed.

## 12. Approved decisions and follow-ons

Out of scope: create or edit issues; comment edit/delete; thread
resolve/unresolve; mention picker and notification UI; subscription controls;
general file attachments, drag/drop, clipboard export; live updates transport;
disk drafts/crash recovery; enabling the standalone API. Record newly found
needs as follow-ons rather than implementing them here.

Captain's approved decisions (15 November):

1. The text-only `mediaDraftId` fence is the duplicate-safety mechanism.
2. Enter inserts a newline; Command+Enter / Control+Enter submits.
3. JPEG/PNG/WebP only; 8 MiB/eight-images/32 MiB-per-draft/64 MiB-total limits;
   expanding the existing reader to NETT's full 25 MB is a separate decision.
4. Unclear outcomes use frozen same-UUID Retry and warned Discard, as section 5.
5. PR B waits for A to merge and starts from the updated integration branch.

NETT behaviour not confirmed from source/runtime evidence:

- Whether the deployed NETT dev revision equals the inspected Indra SHA, and
  whether its real schema/permissions/scanner/files service behave as read.
- Whether repeated text-only draft consumption has a deployed end-to-end test;
  the code paths and database index support it, but this worker has not run
  Indra tests or the live API. No general comment idempotency endpoint or
  lookup by draft UUID was found.
- No separate exposed per-user upload capability was found; authoritative
  Prepare can refuse even after `issues.manage` is advertised.
- Native clipboard image representations and exact macOS capture appearance
  need implementation/platform evidence. No documentation-only phase claims
  these or human WorkOS/live acceptance have passed.
