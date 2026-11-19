# Spec: NETT issue workspace

## Intent

A read-only Issues board laid out like NETT's desktop Issues pages, plus issue
references in IRC that open it. See [intent.md](intent.md) and the approved
[design](design.md) with its [approval](design-approval.md). This spec settles
the design's section 11 and builds on slice 1's
[nett-issue-client-core](../nett-issue-client-core/) record, merged at
`e8757cf` (aiconsultancy/subspace#2), which it does not repeat.

Each requirement shows where it came from:

- **[T]** requirements from the Issues and Channels initiative;
- **[O]** the owner's direction of 15 November 2026;
- **[D]** the approved design or the captain's approval;
- **[S]** added here, because the Build can't work, or can't be safe, without
  it. The reason follows.

Anything nobody asked for that the first milestone doesn't need is left out.
The **first milestone** (M1) is: issues load from NETT dev after device
sign-in, and an IRC reference opens the issue [T].

NETT's contract was read from Indra's
`frontend/indra/src/lib/api/generated/schema.d.ts` at `f88bff35e7` (15
November 2026). Routes below are relative to the profile's base
(`https://nett.example.com/api` for NETT dev); `org/` stands for
`v1/orgs/{orgId}/`.

## 1. Parts and ownership

| Part | Owns | Notes |
| --- | --- | --- |
| `clients/nett` (slice 1) | Transport, DTOs, sessions, profiles, credentials, `IssueResolver` and the resolution cache | Gains the reads in section 3, a process-wide **hub** (below), a **list query type** and the debug-only fixture seam (section 11). The list query type builds both the grouped read and its continuation `GET org/issues` read: the same filters, one `state` or `stateType` for a continuation, `limit`, `cursor` and an optional team, for All teams. Slice 1's `IssueQuery` (after its amendment: a team or a parent, an optional `order`) stays the per-team and per-parent type, as for **More sub-issues**. No GPUI. [S] Two boards need one session per profile. |
| `framework/services/contracts` | Recognition and issue-link parsing (section 4) | `subspace_contracts::issues::recognition`; shared without a NETT client dependency ([board-contracts spec](../board-contracts/spec.md) section 6). |
| `spaces/issues` (`subspace-issues-board`, id `issues`, Lucide `circle-dot`) | List, issue view, image viewer, Settings page, title-bar profile picker, pure NETT Markdown pre-processing (section 9), mapping NETT DTOs into its plain inputs, Help guide | Starts the hub in `start_background` and stops it in `stop_background`. [D] |
| `spaces/irc` | Recognising, styling and opening references | Reads the hub's snapshot; never depends on the Issues board. [D] |
| `framework/board-kit/dashboard` | `BoardNavigation` (section 4); a shared `rich` reference plugin with the normal and unavailable looks (section 4); a `rich` option that skips `prepare_markdown` for pre-processed text (section 9); a chat-free option for `text_menu`, `row_menu_items` and `panel_menu_items` (section 8) | [D, S] Shared widgets gain options rather than boards copying them, as AGENTS.md asks. |
| Theme files | `media-stage` token for the viewer backdrop | [D] Approval: a token, not literals. |

**The hub** [S] keeps one session per profile behind a lock, so token
refresh stays serialised, and two API readers per session. API reads queue
by priority: the visible list or issue first, then reference resolutions,
then later pages. The images slice adds one file fetcher per session, so a
30-second image fetch never holds up a list, issue or reference read.

**IRC and the hub** [S]. The hub publishes a versioned snapshot: current
profile ID and name, organisation ID, team keys and resolved reference cards,
and no other issue content. IRC reads the snapshot's version on its existing
50 ms timer and never drains the hub's events. The host calls
`start_background` at startup for every enabled board, and again when
Settings enables a board. So references are live only while the Issues board
is enabled and its current profile is signed in with an organisation. With
the Issues board disabled, IRC finds no hub and shows references as plain
text. IRC can't use the board's crate, hence the snapshot.

The board declares `background_updates: false`, no notifications, no agent
tools, no search adapter and no full-text indexing, and its `agent_context`
returns `None` (section 8). [S] Issue content must not leave memory.

Client calls block and run in `cx.background_spawn` or the hub, never in
render. Loads start from `ListDashboard::on_open` and `Dashboard::activate`.

## 2. Profiles, sign-in and organisation

- **Profiles.** The Build ships **NETT dev** only. The `subspace-api` profile
  stays hidden and refuses sign-in until Kirk confirms its address, contract
  and sign-in [D, approval 4]. Production NETT uses a separate pinned profile with its own WorkOS client
  ID and no edge step [T]. Example origins are `https://nett-prod.example.com/api`
  and `https://files.nett-prod.example.com`.
- **Current profile.** Chosen with the board's title-bar picker, like the PRs
  board's account picker, and stored as the `issues.profile` preference. It
  serves the board and IRC references [D, approval 3].
- **Sign-in** is in **Settings › Board › Issues**, as `signin-*` shows:
  slice 1's `start_sign_in`, then the verification address as an external
  link, the code with **Copy code**, **Open sign-in page** (the complete
  address), the expiry countdown and **Cancel** (sets the cancel flag).
  Expired, denied and cancelled attempts store nothing. A WorkOS network
  failure reads *Couldn't reach the sign-in service. Try again.* A locked or
  missing keychain reads *Subspace couldn't use the system keychain, so
  you're not signed in.* Sign-in and sign-out apply at once, and Settings
  says so [D, approval 11].
- **The device poll's thread** [S]. The poll can last minutes, so it runs on
  its own thread, outside the hub's session lock, with its own copy of the
  WorkOS client. When it finishes, it takes the lock only to install the
  tokens. No API reader and no IRC snapshot waits on it. This is a stated
  change to slice 1's API:
  - slice 1's `complete_sign_in` splits into a poll that holds no session
    borrow and a short install;
  - `WorkOsClient` derives `Clone`, and the poll's copy is cloned from the
    session's own client, never built with `WorkOsClient::development()`,
    so a fixture session polls its fixture server (section 11).
- **Organisation.** After sign-in the hub reads `v1/auth/orgs`. An
  organisation is eligible when it is not personal, its `featurePacks` hold
  `issue-tracking` and its `permissions` hold `issues.view`. [D, S] The
  device grant has no organisation.
  - **One eligible organisation** is used without saving anything, and
    Settings shows it as chosen.
  - **Several** give the organisation picker, a Settings draft saved with
    **Save**. Until one is saved, the list shows *Choose an organisation for
    NETT dev*, with **Choose organisation…** opening Settings.
  - **None** gives *None of your NETT dev organisations has Issue Tracking
    you can read.*

  Only the picker's Save writes the organisation, so no stale draft can
  overwrite an automatic choice. A fixture run is the exception (section
  11): its picker calls `select_organization` on the fixture session
  directly and leaves the editor's snapshot unchanged, so Save has nothing
  to write.
- **A saved organisation reaches the hub** [S] through the `Services`
  global. After Save, the host's `mirror_services` puts the boards' saved
  files into `Services`, so the board subscribes with
  `observe_global::<Services>` and hands the hub the new profile settings
  when they differ. The hub also reads `issues/config.json` at start. A
  changed profile or organisation starts a new scope generation (section
  4).
- **Membership changes** are found when a read is refused (section 5): the
  hub reads `v1/auth/orgs` again and names the account-level reason. It
  never changes the saved organisation by itself. [S]
- **Refresh.** Slice 1 refreshes within a minute of expiry. An API 401 with a
  token the session thought valid drops the access token, refreshes once and
  retries the read once. A second 401 is *session expired*. `invalid_grant`
  signs the profile out, as slice 1 does. [S] This needs a small
  `WorkOsSession` method that compares and drops: it drops the access token
  only while it is still the token that got the 401. With two API readers,
  a 401 that arrives after the other reader has already refreshed leaves
  the new token in place and simply retries.
- **Sign-out** deletes that profile's keychain entry and drops its access
  token, as `indractl logout` does. There is no server revocation, because
  the public device client has none in `indractl`. Open lists and issues
  change to the signed-out state, the viewer closes, and IRC references turn
  back into plain text. [D, S]
- **Edge.** NETT dev sits behind Cloudflare Access. The development access update admits Subspace
  through Cloudflare One (WARP) on the device, with no service token in the
  app [T].

## 3. Reads by part

Every organisation read sends the bearer and a matching `X-Org-Id` (slice 1).
The only POST routes are reads that change nothing: file grants now, then the
nested-children batch and the reference resolver when NETT ships them. The
client offers no other POST or any write. [S]

| Part | Route and parameters | When | If missing or refused |
| --- | --- | --- | --- |
| Organisations | `GET v1/auth/orgs` | Sign-in, hub start, after a 403 or 404 | Section 5 |
| Teams and workflows | `GET org/issue-teams?includeArchived=true&limit=100`, every page | Hub start or first activation with a signed-in profile, whichever comes first; sign-in, profile or organisation change, Reload | Team picker shows the profile's fallback keys; workflow line is left out; recognition uses fallback keys |
| List, grouped | `GET org/issues/groups` with `groupBy=state&team={key}`, or `groupBy=stateType` and no `team` for All teams, through the new list query type; `limitPerGroup=25`; `order=priority`, or `order=manual` on Backlog with one team; the tab's predicate; `parent=none` while sub-issues are hidden; toolbar filters | Board activation, tab, team or filter change, Reload | Section 5; list keeps NETT's empty wording |
| More rows in a group | `GET org/issues`, built by the same list query type: the same filters and optional team, the group's one `state` (or `stateType`) in place of the group dimension, `limit=50`, the group's `cursor` | Near a group's end | Row *Couldn't load more* with Retry |
| Folded groups (Done, Cancelled, Duplicate) | The same list read without a cursor | When unfolded | As above |
| Hidden sub-issues count | `GET org/issues/groups`, same filters without `parent`, `limitPerGroup=0`; n = total − total with `parent=none` | With the list | Segment left out, as NETT does |
| My issues count | `GET org/issues/groups?groupBy=stateType&assignee=me&stateType=triage&stateType=backlog&stateType=unstarted&stateType=started&limitPerGroup=0` | With the list | Count left out |
| Show sub-issues | Until NETT's tree reads reach dev: the grouped read without `parent` (a flat list). Then `GET org/issues/tree/groups`, `GET org/issues/tree` and `POST org/issues/tree/children` (40.7 section 3) | When switched on | As the list |
| Issue | `GET org/issues/{ref}`, `ref` an identifier or UUID. Carries the header, description and its `media`, `mentions`, `provenance`, properties, the first 20 sub-issues and relations, the first pull request page and header `progress` | Every open, Reload | Section 5 |
| More sub-issues | `GET org/issues?parent={id}&order=manual&cursor=…`, from the issue's `children.nextCursor`, through slice 1's amended `IssueQuery` with a parent and no team | **Show n more** | Row with Retry |
| More relations | `GET org/issues/{ref}/relations?limit=20&cursor=…` | **Show n more** | Row with Retry |
| More pull requests | `GET org/issues/{ref}/pull-requests?limit=20&cursor=…` | **Show n more** | Row with Retry |
| Decisions | `GET org/decisions?issueId={id}&status=all&limit=10`, then `GET org/decisions/{id}` for each listed decision, two at a time, for `provenance.sourcePath` | With the issue, only when the organisation's `permissions` hold `projects.view` | No panel on 403 or 404; the issue stays shown. A row without its path when its own read fails or its `provenance` or `sourcePath` is null |
| Activity | `GET org/issues/{ref}/activity?limit=50`, newest first; older pages by `cursor` | With the issue; **Show earlier comments** | Panel error with Retry; the rest of the issue stays |
| Images panel | `GET org/issues/{ref}/images?limit=50`, by `cursor`; `around={mediaId}` for an inline image outside the loaded pages | With the issue | Pages stop by encoded size and carry no total, so a page can be empty and still continue. No panel only when the first page is empty and has no `nextCursor`; an empty first page with a cursor reads one more page by itself, then offers **Look further for images**. Error tile with Retry |
| Image grants | `POST org/files/grants` with `{"publicFileIds": [...]}`: at most 50, no duplicates | Before drawing a tile or inline image | Failed tile with Retry |
| Image bytes | `GET` the grant's URL, which must be on the profile's file origin (`https://files.nett.example.com`) | After a grant | Failed tile with Retry |
| Tracker progress | Header progress from the issue's `progress` until NETT ships `GET org/issues/{ref}/progress` (the NETT progress and nested-issues rollout PR 4) on dev; then the Progress panel | With a tracker | Header progress only |
| References | Section 4 | Hover, focus or open | Section 4 |

**Tabs** add NETT's predicates [D]: My issues `assignee=me`; Active
`stateType=unstarted&stateType=started`; Backlog `stateType=backlog`; All
issues none. My issues shows Assigned only. NETT's Created and Subscribed
modes are left out [S], as nobody asked for them.

**Toolbar** [D, S]: search sends `q`; the team picker sends `team` (default:
the profile's first fallback key when it is an active team, otherwise the
only active team, otherwise All teams; kept per profile as a preference);
**Assignee** offers Me and No assignee (`assignee=me`, `assignee=none`);
**Priority** offers NETT's five values; **Filters** offers status, type, due
date and **Show archived** (`includeArchived=true`). Named people, labels,
projects, milestones and events are left out of the first release, because
each needs a catalogue read nobody has asked for.

**Pull request rows** open in the system browser, because the PRs board can't
open a pull request by reference [D, approval].

**Decisions** show title, status, decided date and the repository path, the
decision's current identifier [D, approval 6, T]. NETT's list carries no
path, so the Build reads each listed decision. If NETT adds the path to the
list item, those reads go.

**Activity** is one read, the activity pages, which hold comments, replies
and events in time order, as NETT's *All* view does [D, S]. One read means no
reply read per thread.

- `comment.created` entries are the comments, always shown. A reply sits
  under its root when the root is loaded; otherwise it reads *Reply in a
  thread*. A deleted comment, or a `comment.created` entry whose `comment`
  is null, shows as NETT's tombstone. An imported comment names its source
  author, with the NETT member who imported it.
- The Activity panel opens in **Comments**. Choose **All** to interleave
  events with comments by `displayAt`, following the Activity filter change and NETT's layout.
  The selection lasts while the issue is open.
- Known kinds read as plain sentences, naming records from the page's
  `references`. A record the page doesn't name gives NETT's neutral sentence,
  and an unknown kind reads *Issue activity recorded*. Documents are named
  as plain text, since they are out of scope.
- The six latest comments show first, then **Show n earlier comments**. n
  counts the comments, tombstones included, in the loaded entries that
  aren't shown yet (*n+* while older pages remain). It never comes from
  `commentCount`, which leaves out deleted comments while their tombstones
  still show.
- Finding six comments, or more after **Show n earlier comments**, reads
  older pages by itself, at most four pages for each open or action. Past
  that, the panel offers **Show earlier comments**, which reads the next
  page.

## 4. References

- **Count scheduling** [D] (captain's decision, 15 November 2026): My issues
  and hidden sub-issues counts remain at `ReadPriority::Visible`, queued after
  the list. Both are on screen with the list and use cheap grouped reads with
  `limitPerGroup=0`.
- **Recognition** [D, T] uses slice 1's `ReferenceRecognizer` with the
  current profile's loaded team keys, archived teams included, falling back
  to the profile's keys. Keys match exactly, so `tst-827` stays text, as
  NETT's GitHub linking does. Text in code spans and code blocks stays plain.
  An unmatched backtick is literal text, as in CommonMark; slice 1's review
  fixes cover this, with stricter word boundaries. Recognition uses current
  keys only. Retired keys would need NETT to list them, and nothing here
  plans for that. While the profile is signed out or has no organisation,
  nothing is recognised.
- **Links as references** [D]: a link in an IRC message (Markdown or a bare
  address) or in issue text is an issue reference when it is
  `/issues/{identifier}` (app-relative), the profile's app origin plus that
  path (`https://nett.example.com/issues/TST-827`),
  or a Linear issue URL in the configured mirror workspace, with a recognised
  issue key and an optional title slug. Links to other workspaces remain
  ordinary links [S]. NETT issue links must have no `orgId`, or every `orgId` they
  carry must name the profile's current organisation; compare UUIDs
  case-insensitively after percent-decoding the query. A link naming another
  organisation stays an ordinary link. Linear links are unaffected by this
  organisation check [D, captain's decision, 15 November 2026].
- **Look** [D]: IRC's accent-on-tint reference treatment. A reference
  resolved as unavailable turns muted with a dotted underline, the new
  catalogue variant. Kit's `TextView` has no per-link style, and only IRC's
  `InteractiveText` lines draw accent-on-tint today. [S] Markdown text (IRC's
  Markdown lines and issue text) therefore draws references through a
  shared reference plugin in board-kit, built on Kit 0.9.1's public
  `MarkdownPlugin` API: the pre-processed `subspace-issue:` links (section 9)
  become native inline elements with the accent-on-tint and unavailable
  looks, their own click and their tooltip. That needs no Kit change, and
  board-owned segments would mean redrawing Markdown. IRC's formatted lines
  keep `InteractiveText` with the same tones. Both arrive in slice 4.
- **Resolution now, without the batch reference endpoint** [D, T]: one reference at a time, when
  it is hovered past the tooltip delay, focused or opened, with slice 1's
  scalar `IssueResolver` (`GET org/issues/{identifier}`). The hub keeps only
  a card: identifier, title, state, type and archived. Nothing resolves
  just because it is on screen.
- **Cache** [D; its limits S]: in the hub's memory, per profile and
  organisation, keyed by the identifier. Available cards last 5 minutes and
  unavailable ones 1 minute; at most 500 entries, least recently used first
  out; duplicate requests share one read. Resolutions run on the hub's two
  API readers, behind the visible list or issue (section 1). Errors other
  than 404 are not cached.
- **Cancellation** [D, S]: every read carries the hub's scope generation,
  which changes on sign-in, sign-out, profile switch, organisation change,
  session expiry and board stop. The change empties the cache, and results
  from an older generation are dropped.
- **Unavailable** [D]: a 404 after the pack check (section 5) gives the
  neutral *{identifier} isn't available with NETT dev.* It never says whether
  the issue exists.
- **Moving to the batch resolver** [T]: once the batch reference endpoint's
  `POST org/references/resolve` is deployed on NETT dev, a later slice
  replaces the scalar reads with batches. It sends the references in IRC
  lines on screen, duplicates removed, at most 50 positions and 16 KiB a
  call; maps `availability: "unavailable"` to the unavailable state; and
  fills the same cache from each `card`. The switch follows the deployment,
  not a runtime probe, because a 404 can't tell a missing route from a
  missing pack. The batch card has no issue type, so the tooltip's *Tracker*
  needs NETT to add it, or it is dropped then.
- **Opening** [D]: click (and, from the keyboard, section 10) opens the issue
  in the Issues board through the new `BoardNavigation` request: target board
  `issues`, the identifier, the profile ID, and a return target (board `irc`
  and an IRC-owned token for the conversation). The shell switches boards
  and calls a new `Dashboard::open_link`, as Search reveals results. Back and
  Escape send the return target back, and IRC restores focus. A profile that
  changed in between opens the signed-out or unavailable state, never
  another profile's issue. [S] Search navigation needs indexed content, so
  it can't carry this.
  - **Opening during bootstrap** [D] (captain's decision, 15 November 2026):
    with the same profile, hub `Loading` and no organisation yet, keep a
    content-free `Loading` issue with its identifier and profile. Carry it
    only into the first organisation install after that open: when bootstrap
    becomes `Ready`, read it at visible priority under the new generation.
    Reload during this wait does nothing. If bootstrap ends in
    `ChooseOrganization` or `NoEligibleOrganizations`, close the issue and
    show the hub's state; opening in either state also leaves the issue
    closed. A bootstrap `Error(reason)` becomes the issue's `Error(reason)`.
    In a bootstrap error, when an open, content-free issue belongs to the
    current profile, `reload_issue` and `Hub::reload` both retry bootstrap:
    hub and issue become `Loading`, `Bootstrap` queues at visible priority,
    and the carry-over reads the issue once after the first organisation
    install. This extends the same captain decision of 15 November 2026.
    Every other scope change still clears this pending navigation, including
    sign-out, expiry, profile change, stop and a later organisation change.
    `Unavailable` remains for a mismatched profile or an issue 404 after the
    pack check; a missing organisation alone never implies it.
- **Tooltip** [D, nice-to-have]: the reference plugin's own tooltip on
  Markdown text and `InteractiveText`'s tooltip on formatted lines:
  identifier, title, state glyph and name, *Tracker*, and the profile name;
  or the unavailable, signed-out or error wording from section 5. It's a
  popover surface, with no menu, so it never offers chat (section 8).

## 5. Errors

Slice 1's transport classifies each response, and its messages stay
generic. The board maps each class to the design's states and wording; that
split is intended. The board never shows slice 1's messages directly and
writes nothing from a response body to the screen or the log. [D, S]

| Response | List or issue | IRC reference |
| --- | --- | --- |
| API 401 | Silent refresh and one retry; then `error-session`: *Your NETT dev session has expired*, **Sign in…**, and **Choose another profile** when there is another | Tooltip *Sign in to NETT dev to see this issue*; recognition stops |
| Refresh refused (`invalid_grant`) | Signed out (`list-signed-out`, `signed-out`) | Plain text |
| Access redirect or challenge (`EdgeAccessNeeded`) | Error state *NETT dev is behind Cloudflare One. Connect Cloudflare One (WARP) on this computer, then Retry.* | Tooltip *Couldn't reach NETT dev*; nothing cached |
| 403 | Read `v1/auth/orgs` again. Organisation gone: *Your NETT dev account no longer belongs to {organisation}*, with **Choose another organisation** (Settings). `issues.view` gone: `error-access`, *Your NETT dev account can't read issues in {organisation}* | Tooltip with the same account-level wording |
| 404 on the list or teams | Read `v1/auth/orgs` again. Pack gone: `error-access`, *Issue Tracking isn't turned on for {organisation} in NETT dev*. Pack present: the network state | Recognition stops while the pack is off |
| 404 on an issue | The same pack check; otherwise `error-not-found`, *{identifier} isn't available*, with Back | Unavailable (cached for 1 minute) |
| 404 on an images read with `around=` | Treated as *anchor gone*: that image opens alone and the issue stays. Slice 1 reads no error bodies, so the board infers this from the request rather than reading `ISSUE_IMAGE_ANCHOR_UNAVAILABLE`; a real loss of access shows on the next issue read | — |
| 400 on a read the hub sent with a cursor | Inferred from the request, since slice 1 maps 400 to an unexpected status: restart that list from its first page once, then the network state | — |
| Other statuses, oversized or invalid JSON | `error-network`: *Couldn't load {what}*, **Retry**, **Copy details** (status and route kind only), **Show log** | Tooltip *Couldn't reach NETT dev* |
| Network failure or the 30 s timeout | First load: `error-network`. With content already read this session: that content under the **Offline** banner with **Retry** (`offline`) | As above |

An unmarked, empty Access 401 can't be told from an API 401 (slice 1). Until
the development access update records a reliable signal, a 401 that survives a successful refresh
also says *If Cloudflare One isn't connected, connect it and Retry.* [S]

## 6. Images

- **Where from** [T, D]: the description's and each comment's `media`
  descriptors for inline images; the images read for the Images panel and
  the viewer. Non-image files show as a chip with name, type and size, and
  don't open [S]: downloads are out of scope.
- **Grants** [D]: asked for in batches of at most 50, only for images drawn
  or within a screen of view; reused until 30 seconds before `expiresAtUtc`;
  renewed once after a failed fetch; then the tile fails with Retry.
- **Fetching** [D, S]: on the hub's file fetcher, off the UI thread. The URL
  must be HTTPS on the profile's file origin, or it isn't fetched. The one
  exception is the debug-only fixture profile (section 11), whose file
  origin is its loopback server: there, and only there, plain `http` is
  allowed. No bearer,
  `X-Org-Id` or cookies; redirects off; the shared 30 s timeout; at most
  `subspace_dashboard::images::MAX_ENCODED_BYTES` (8 MiB) and the
  descriptor's size; the content type must match the descriptor and be PNG,
  JPEG, GIF or WebP. Grant URLs are capabilities: their grant tokens are
  never logged (section 8), and the URLs are never copied or
  shown.
- **Decoding** [D]: `subspace_dashboard::images::decode` in
  `cx.background_spawn` (8 MiB, 8 megapixels, 8,192 px, first frame).
  Larger images show *Too large to preview here* with the file's name and
  size.
- **Memory** [D, S]: decoded images for the open issue and the one before it
  (for Back), at most 128 MiB, least recently used first out. Encoded bytes
  go once decoded. Everything goes on sign-out, profile or organisation
  change, session expiry, a refused read and board stop.
- **Never on disk** [D, approval 7]: no Save image, copy image or drag-out.
- **Inline images** [D, S]: shared `rich` drops `nett-media:` images
  before any hook sees them, so the board's pre-processor (section 9)
  rewrites each one to a `subspace-media:` link. A board-owned
  `MarkdownPlugin` draws that link as an inline image element with the
  decoded image, and a click on it opens the viewer.
- **Viewer** [D]: *n of m on this issue* from the images read (*n of m+*
  while pages remain), Previous and Next across pages, the alt text, and
  **Show in the description** or **Show in the comment** when that comment
  is in the loaded activity. The backdrop uses the `media-stage` token.
- **Until the images slice** [S], the pre-processor rewrites an inline image
  to the text *Image: {alt text}*, so M1 never shows a broken image.

## 7. Freshness

- **Reads** [D; the 60 seconds S]: activating the board reads the list
  again when its last read is over 60 seconds old; opening an issue always
  reads it; **Reload** and `r` read the visible list, or the issue with its
  first activity, images and decisions pages. The status line says *updated
  n min ago*.
- **No automatic refresh** [S]: no timer (`background_updates: false`), no
  refresh on window focus and no NETT realtime hub. NETT's own pages don't
  poll; the board is for reading, and its age is on screen; and background
  reads would keep refreshing tokens and loading NETT dev while nobody
  reads. The development access update therefore needn't prove a realtime route.

## 8. Privacy

- **Memory only** [D]: lists, issues, activity, decisions, images, grants and
  reference cards. They are never serialised.
- **On disk, nothing but** [S]: `issues/config.json`, which after slice 1's
  amendment saves only the selected organisation and the fallback team keys
  (the built-in dev address and client are filled in on load); the
  keychain's refresh tokens (slice 1); and `state.json` preferences that
  hold no issue content: the current profile, the team key per profile, the
  tab, the sub-issues mode and folded state groups; and the app's log file,
  which holds request-path identifiers only, never content (below). A
  fixture run never writes `issues/config.json` or the keychain (section
  11).
- **Never** [D, S]: `prefs::write_cache`, the search index, agent tools or
  chat context, notifications, the window title (identifier only) or Report
  issue diagnostics.
- **Logs** [S, captain's decision, taken under the delegation jt gave the
  tracker's captain on 15 November 2026]. The shared `RequestLog` stays. It
  records the host, the path without its query string, the status and the
  duration, so organisation UUIDs, issue and decision identifiers and public
  file ids in request paths reach the log; they are identifiers, not
  content, and that is accepted. Titles, bodies, comments, query strings
  (search text, cursors, grant tokens, which sit in `?grant=`) and
  credentials never reach it, and the board writes no log record of its
  own that carries them.
- **Chat, wired off** [S]. Shared code sends a board's content to chat by
  default: the default `agent_context` is built from `subject()`, title
  included, and feeds both the chat's context provider and **Add to chat**;
  `text_menu` (and so `rich::plain_text`) always offers **Add to chat**;
  `row_menu_items` starts with it; and `panel_menu_items` adds **Add
  selection to chat**. So:
  - the board's `agent_context` returns `None`, and if the board implements
    `subject()`, its row carries the identifier only;
  - board-kit gains one chat-free option for `text_menu`, `row_menu_items`
    and `panel_menu_items` that leaves out `AddToChat` and
    `AddSelectionToChat`, and every Issues surface uses it, IRC's reference
    tooltip included;
  - the board shows no panel **Add to chat** button.
- **Cleared** [D]: on sign-out, profile switch, organisation change, session
  expiry, a refused read, board stop and quit. **Offline** shows only what
  this session has already read.
- **Tested** [S]:
  - after a fixture session, `state.json`, the cache files and the search
    store contain no fixture title, body or comment;
  - after the same session, the log holds request paths with identifiers
    and no fixture title, body, comment, search text, cursor, grant token
    or credential;
  - with an issue open and chat open, `visible_board_context` returns
    `None`, **Add to chat** attaches nothing, and no menu on the list, the
    issue view, its text or IRC's reference tooltip offers chat.

## 9. Markdown

NETT stores Markdown and renders it in the browser (Indra
`docs/markdown.md`). The board shows it through shared `rich` [D, S], but
`rich_text_with_links` runs `prepare_markdown` first, which would break it:

- `image_markdown` returns nothing for an image that is neither a `data:`
  image nor an http, https or mailto address, so `nett-media:` images
  vanish;
- it drops `script`, `style`, `form` and similar elements with their
  content, and it doesn't know about code, so even code-block text such as
  `<form>…</form>` goes;
- the link-claim callback can swallow a click, but it can't draw a link as
  plain text.

**The choice** [S]: the Issues board uses its own `nett_markdown` module to
pre-process NETT Markdown, and
`rich` gains one option, `rich_text_prepared`, that skips `prepare_markdown`
for text a caller has already made safe. It keeps `rich`'s link policy and
its image resolver, which still refuses everything but `data:` URLs. The
NETT rules (media, mentions, NETT and Linear links, references) are NETT's
contract and belong to the Issues board, as IRC's `markup` already rewrites its
Markdown before `rich`. Only skipping `prepare_markdown` can keep code text,
because escaping can't reach inside code.

**The pre-processor** is pure and tested. It parses with the `markdown`
crate's mdast, so code spans, code blocks, links and images are found
exactly. Kit re-exports only `markdown_ast`, not the parser, so
`spaces/issues` takes a direct `markdown` dependency pinned to the version
already in `Cargo.lock` (1.0.0), and `mise run audit` passes for it.

Subspace vendors that exact version with one change: public `to_mdast` catches
unwinding parser panics and returns a fixed parse error. This protects Kit's
background parser as well as its synchronous path. Shared `rich` preflights only
sources at most 4 KiB (Kit's synchronous bound) containing a backtick, tilde or
math fence run; a failed parse becomes escaped, readable prose. Larger failures
use Kit's ordinary parse-error display. A bounded LRU retains only a 64-bit
source hash, length and parse verdict, never issue or IRC text. See
`vendor/markdown/PATCHED.md` for the exact patch and removal condition.

The captain decided this placement on 15 November 2026 so slices 2 and 3
can run in parallel before `spaces/issues` exists. The board maps client
DTOs into the service's plain media descriptors and mention statuses, and
supplies a link context with the profile's app origin, current organisation
and the same recogniser used for text, plus the reader stage.
It rewrites in this order:

1. code spans and code blocks stay exactly as written;
2. `![alt](nett-media:{id})` becomes the text *Image: {alt}* until the
   images slice, then a `subspace-media:{id}` link that the board's media
   plugin draws (section 6); `[label](nett-media:{id})` becomes a file chip
   with name, type and human-readable decimal size, rounding before choosing
   the unit and omitting a trailing `.0` (for example `1 MB`); an id the body's
   descriptors don't name becomes *Image unavailable*. Trim image alt text;
   an empty alt becomes *Image*;
3. any other image becomes a `[alt ↗](url)` link for an http or https
   address, as `rich` does, or its alt text. A blank external-image alt uses
   `Image ↗` as its link label;
4. `@[Name](member:{uuid})` becomes a `subspace-member:` link that the
   shared reference plugin draws as a mention, *@Name*, with the current
   name from the issue's or comment's mention statuses, or the written name;
   a *not a member* mention is muted; mentions don't open anything. Until
   slice 4 brings the plugin, the pre-processor writes plain *@Name* text
   instead, as it writes *Image: {alt}* until the images slice;
5. issue links (section 4) become `subspace-issue:{identifier}` links that
   keep their label. Until slice 4, a labelled link such as `[the parent
   tracker](/issues/TST-827)` becomes its label as plain text, as step 6
   does for other NETT links; only a bare link, whose text is its address
   or identifier, becomes the identifier as plain text;
6. other NETT links (app-relative, or on the profile's app origin, such as
   `/projects/…` or `/issues/comments/…`) become their label as plain text,
   because the owner left NETT's website out;
7. bare `KEY-n` text outside code and links becomes a `subspace-issue:`
   link from slice 4, and stays plain text before it;
8. autolinks such as `<https://…>` become ordinary links, then every other
   `<` outside code becomes `&lt;`, which Markdown shows as `<`, so raw HTML
   reads as literal text, as NETT's reader shows it;
9. every `$` outside code spans, code blocks and link destinations becomes
   `\$`, which Markdown still shows as `$`. Kit's `TextView` parses with
   `math_flow` and `math_text` on: a `$$` fence (indented up to three
   spaces, or inside a block quote or list item) would turn the lines after
   it into a math block drawn like code, and `costs $5 and $10` would become
   inline maths. NETT has no maths, so escaping every `$` changes no
   visible text.

Literal brackets in text nodes are escaped so flattening an inner issue link,
mention or reference cannot activate surrounding outer-link syntax. Resolve
shortcut and collapsed references before applying steps 8 and 9: emit an
ordinary `[prose(label)](<resolved url>)` link and keep the original definition.
Definitions and link titles remain opaque metadata, while visible reference
labels follow the same HTML and dollar rules as inline links.

If the initial NETT parse fails, retry once with only complete backtick, tilde
and math fence runs escaped, then apply the normal rewrite. This keeps media,
mention and NETT-link rules active on malformed bodies. Use literal text only
if that retry also fails. Normally parsed code stays byte-for-byte unchanged.

`http`, `https` and `mailto` links open in the system browser [D, approval
9]; anything else is inert, by the shared link policy. The shared reference
plugin draws `subspace-issue:` links (section 4).

- **The subset**: NETT writes paragraphs, headings 1 to 3, bold, italics,
  strikethrough, inline code, links, bulleted, numbered and check lists,
  quotes, code blocks, GFM tables and dividers, and `rich` shows them all.
  Check-list boxes are read only.
- **No text is dropped.** The test `nett_markdown_keeps_all_text` feeds
  these through the pre-processor and `rich_text_prepared`, and checks every
  character of visible source text reaches the rendered text:
  - raw HTML (`<script>x</script>`, `<form>y</form>`) inside and outside
    code;
  - `$$` at the start of a line, indented (`   $$`), quoted (`> $$`) and in
    a list item (`- $$`), and `costs $5 and $10`;
  - `nett-media:` images and files and mentions;
  - every issue-link form, labelled and bare, before and after slice 4.
  The shared JSON fixture includes the app origin, organisation, team keys and stage.
  Dashboard tests call the real NETT pre-processor and assert native rendered/
  copied text against every fixture's `visible` value, including parse recovery.
- **Prose** keeps NETT's 75-character measure [D].

## 10. Keyboard and focus

The board binds its keys in `Dashboard::bind_keys` under
`view_keys("issues")`. Plain keys act only when no text field, menu or
dialog has focus. `r`, `?`, F-keys and `Cmd+K` stay the shell's. The
shell's `/` opens item search only when `Dashboard::search()` is `Some`, and
this board has no search adapter, so the board binds `/` itself to focus its
search field, as `spaces/github/src/diff/view.rs` does. The map follows the
PRs board, which already uses `1`–`4`, `u` and `y` the same way. [D,
checked]

Slice 2 binds `j`/`k`, `1`–`4`, `Shift+S`, `/` and `u`. Enter on a row does
nothing in slice 2, and the row offers no open action; slice 3 makes Enter
open the issue.

| Where | Keys | Action |
| --- | --- | --- |
| List | `j`/↓, `k`/↑, Enter | Shared list selection and open (Enter from slice 3) |
| List | `1`–`4` | My issues, Active, Backlog, All issues |
| List | `Shift+S` | Sub-issues on or off (NETT's key) |
| List | ← / → | Fold or unfold a parent (nested slice only) |
| List | `/` | Focus the board's search field (board binding) |
| Both | `u` | Profile picker |
| Issue | Escape | Closes a menu or tooltip first, then Back |
| Issue | `j` / `k` | Next or previous issue of the list it came from; not from IRC |
| Issue | Tab / Shift+Tab | Back, path, More, the main column in reading order, then the side column (design §6) |
| Issue | Enter / Space | Open the focused issue, reference or image, or select the focused timeline tab |
| Issue | `m` | Focus Activity |
| Issue | Space, Shift+Space, Home, End | Scroll the issue when no control has focus |
| Issue | `y`, `Cmd/Ctrl+.` | Copy the identifier |
| Viewer | ← / →, Escape, Tab | Previous and next image; close and return focus to what opened it; Close, Previous, Next, Show in … |

**IRC, checked against the code** [D, approval 10]: the transcript has no
keyboard focus today. `Conversation` tracks one focus handle; Kit's
`MessageScroller` has no item focus; `TextView` links and IRC's
`InteractiveText` references respond to the pointer only. So:

- **M1** [S]: the IRC command **Open issue reference…** (palette and the
  conversation's More menu) lists the issue references in the conversation's
  loaded lines, newest first, without duplicates, at most 20. Enter opens
  one; Escape closes. Coming back returns focus to the message input.
- **Later**: design section 6's transcript model (Shift+Tab from the input
  into the newest message, ↑/↓ between messages, Tab between a message's
  references, Enter, Escape) as its own slice. It covers channel, member and
  link references as well as issues. Plain `TextView` links take no
  keyboard focus, but in Markdown lines the shared reference plugin's
  inline elements are native GPUI elements that can (section 4), so the
  slice needs no Kit change; IRC's channel and member references in
  Markdown lines move onto the same plugin. Focus then returns to the
  opened reference, as the design shows.

## 11. Help guide and evidence

- **Guides** [D]:
  - `spaces/issues/help.md` from the board guide template (sign-in,
    profiles, tabs, filters, the issue view, images, keys, what is left out
    and why), listed in `docs/subspace/toc.yml` and in the table on
    `docs/subspace/boards.md`, as every board guide is;
  - `spaces/irc/help.md` gains references;
  - a new developer page, `docs/issues.md`, listed in the root `toc.yml`
    beside `docs/irc.md`, which is already there; `docs/irc.md` gains
    references.
- **Fixtures** [S]. In slice 1, every route to a session or client calls
  `ApiProfile::validate()`: `WorkOsSession::new` through `with_auth`, and
  `Client::new`. `validate()` accepts the WorkOS kind only for `nett-dev`,
  its HTTPS dev address and the dev client; `WorkOsClient::development()` is
  pinned to `https://api.workos.com`; and `WorkOsClient::fixture` is
  `#[cfg(test)]`. So a fixture run needs a seam, which slice 2 adds as a
  **stated change to slice 1's API**:
  - a `#[cfg(debug_assertions)]` constructor in `clients/nett`,
    `WorkOsSession::fixture(api_origin, workos_base, store)`, which checks
    itself that both addresses are loopback and builds its `Client` without
    `validate()`;
  - `WorkOsClient::fixture` widens from `cfg(test)` to
    `cfg(any(test, debug_assertions))`, and the fixture client carries its
    own **loopback verification origin**. Device-flow validation checks the
    verification address against the client's origin instead of
    `device.rs`'s `VERIFICATION_ORIGIN` constant, which stays the real
    client's origin;
  - `validate()` is unchanged and keeps refusing anything but the dev
    address and client, so the saved `nett-dev` profile can't be rebound;
  - `MemoryCredentialStore` loses its `qual:test_helper` marker in
    `clients/nett/src/credential.rs`, since the debug fixture
    is non-test code and rustqual would otherwise report the marker;
  - the environment is read through a pure, unconditional function,
    `fixture_from_env(debug: bool, api, workos)`, following
    `clients/github/src/lib.rs`'s `debug_api_url`. It returns a fixture only
    when `debug` is true and both `SUBSPACE_NETT_API_URL` and
    `SUBSPACE_WORKOS_URL` are loopback addresses. A unit test with
    `debug = false` proves a release build ignores both variables; no test
    tries to flip `debug_assertions`;
  - the board's fixture branch, which calls `fixture_from_env` with `true`
    and then `WorkOsSession::fixture`, carries `#[cfg(debug_assertions)]`
    itself. A call guarded only by `cfg!(debug_assertions)` wouldn't compile
    in release, so slice 2 adds `cargo check --release` for the board crate
    to its checks.

  With a fixture, the board offers only the **NETT fixture** profile:
  - its session uses slice 1's `MemoryCredentialStore`, so a capture never
    touches the real keychain;
  - its file origin is the loopback server, the only place plain `http` is
    allowed (section 6);
  - sign-in polls the fixture server, never `api.workos.com`, because the
    poll's client is cloned from the session (section 2), and **Open
    sign-in page** opens the fixture's loopback verification address, never
    the real AuthKit site. The test `fixture_sign_in_stays_on_loopback`
    proves both, with a transport that refuses any non-loopback address;
  - **a fixture run never writes `issues/config.json`.** Its organisation
    picker calls `select_organization` on the fixture session directly and
    leaves the Settings editor's snapshot unchanged, so the host's Save has
    no changed draft to write. The test `fixture_run_never_writes_config`
    picks an organisation and saves Settings in a fixture run, then checks
    the file is unchanged or absent.

  `scripts/nett-test-server.sh` serves recorded JSON and PNG fixtures, a fake
  device flow that answers with the fixture's loopback verification origin,
  and fictional people (Jo Fixture, Ada Fixture, Bo Sample) and titles.
- **Captures** [T]: each slice captures its changed states from the capture
  build in Cupertino Dark into `evidence/`, before and after for IRC
  changes, at most 1,600 px wide, with no real names or credentials.
- **Tests per slice** [T]: pure tests for query building, grouping, counts,
  link and Markdown claims, the cache and the error map; render tests for
  each design state; keyboard and focus tests; `make check` and
  `mise run quality`.

## 12. Acceptance

- **Offline, every slice**: the fixture server drives each design state the
  slice delivers, including every row of section 5.
- **NETT dev end to end** [T], once the development access update's route works, on an account
  the Admirals have already set up. Evidence goes in `delivery.md`, with
  captures only from fixtures. These checks follow the merges and never gate
  them: each slice merges on its fixture evidence and checks.
  - **M1 check, as soon as the development access update's route works after slice 4 merges:**
    - sign in with a device code, and the NETT organisation is chosen;
    - Active lists real issues;
    - a tracker opens with its sub-issues;
    - the same tracker opens from a message on the configured IRC server, and Back
      returns focus to the message input;
    - opening `TST-999999` shows the not-found state;
    - sign-out removes the keychain entry and turns references back into
      plain text;
    - without WARP, the edge state shows.
  - **Full check, after slice 8 (the tooltip):** the M1 check again, plus
    properties, relations and pull requests; comments show and activity
    unfolds; decisions show their paths; an image opens in the viewer;
    `TST-999999` in IRC turns muted once hovered.
- **Subspace API profile** [T]: a test proves it is hidden and refuses
  sign-in. Its own end-to-end check is written once Kirk confirms its
  contract.
- **Production** is a separate profile.

## 13. Open questions

1. **Subspace API profile** (Kirk): its address, whether it serves NETT's
   issue REST contract, and its sign-in (Cloudflare Access, WorkOS or
   both). Approval decision 4 holds the profile back until then.
2. **Access 401 signal** (the development access update): what NETT dev returns to a request
   without WARP, so the edge state needs no guess; and whether
   `files.nett.example.com` is behind Access too.
3. **Decision paths** (NETT): adding `provenance.sourcePath` to the decision
   list item would remove one read per decision.
4. **Issue type in the batch resolver's card**, for the tooltip's
   *Tracker*.
5. **Production WorkOS client ID** (for the production profile): `indractl` has only a
   development profile.
