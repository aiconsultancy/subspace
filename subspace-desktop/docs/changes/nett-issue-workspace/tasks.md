# Tasks: NETT issue workspace

Each slice is one pull request into `feat/subspace-desktop`. Each one passes
`make check` and `mise run quality`, adds the tests [spec](spec.md) section 11
lists for what it changes, captures its changed states from the capture build
in Cupertino Dark under `evidence/`, and updates the Help guides it affects.
Section numbers refer to the spec.

## Order

Slices 2 to 4 reach the first milestone (M1): issues from NETT dev after
device sign-in, and IRC references opening them. They hold only what M1
needs, so it lands sooner. Slice 5 then completes NETT's desktop list and
issue view; the layout is NETT's throughout, and only the order of delivery
changes. Activity and images follow, as the owner asked for both. The
tooltip is the nice-to-have, so it comes after them, and the full NETT dev
check runs once it lands. The IRC keyboard model changes more of the IRC
board than issues need, so it has its own slice. The last three wait for
NETT to ship their reads on dev, in whatever order NETT ships them.

## Slices

- [x] **1. NETT client core** (Bashir, the issue workspace implementation): `clients/nett`,
      `framework/services/issues`, device sign-in, keychain, profiles and
      recognition. Recorded in `docs/changes/nett-issue-client-core/`;
      merged at `e8757cf` (aiconsultancy/subspace#2).
- [x] **2. Board, sign-in and the list** (M1). Delivered as slices 2a (the
      hub, Bashir) and 2b (the board, Dax); see [delivery](delivery.md).
  - `spaces/issues` registered in the Subspace product, with descriptor,
    `circle-dot` artwork and a first `help.md`, listed in
    `docs/subspace/toc.yml` and `docs/subspace/boards.md`; `docs/issues.md`
    listed in the root `toc.yml` (section 11).
  - [x] The hub in `clients/nett`, with no GPUI: one session per profile, two
    API readers, the priority queue, scope generations, reading
    `issues/config.json` at start and accepting new profile settings from
    the board, the compare-and-drop of the access token and single retry
    after a 401, and the device poll on its own thread, with a client cloned
    from the session (`WorkOsClient` derives `Clone`) and a short install
    under the lock (sections 1 and 2). The file fetcher waits for slice 7.
  - The board subscribes with `observe_global::<Services>` and hands the hub
    saved profile settings when they change (section 2).
  - [x] The list query type in `clients/nett`, building both the grouped read
    and its continuation `GET org/issues` read with an optional team, for
    All teams; slice 1's amended `IssueQuery` (a team or a parent, an
    optional `order`) stays the per-team and per-parent type.
  - [x] Data-side organisation eligibility, refused-read membership checks,
    team/workflow paging, freshness and the versioned IRC snapshot shape.
  - [x] Developer page `docs/issues.md`, listed beside IRC in root `toc.yml`,
    with the board's side of the hub.
  - Settings › Board › Issues: the NETT dev profile, device sign-in, its
    expired, denied and cancelled states, the organisation rules (one used
    without saving, a picker for several, the *Choose an organisation*
    list state) and sign-out; the title-bar profile picker.
  - The list: heading and scope line, the four tabs, search, team, the
    Sub-issues switch (Hidden by default, flat when on), grouped rows,
    folded terminal groups, more rows, the status line and freshness
    (sections 3 and 7).
  - Keys: `j`/`k`, `1`–`4`, `Shift+S`, `/` (board binding) and `u`. Enter on
    a row does nothing yet, and rows offer no open action (section 10).
  - States: loading, signed out, no organisation chosen, `error-session` (a
    401 after the refresh), *no longer belongs to {organisation}*,
    `error-access`, network, edge and offline (sections 2 and 5).
  - Chat, wired off (section 8): `agent_context` returns `None`; board-kit's
    chat-free option for `text_menu`, `row_menu_items` and
    `panel_menu_items`, used on every Issues surface; the chat test.
  - The fixture seam, a stated change to slice 1's API (section 11):
    - [x] The `#[cfg(debug_assertions)]` `WorkOsSession::fixture(api_origin,
      workos_base, store)` that checks loopback itself and builds its
      `Client` without `validate()`;
    - [x] `WorkOsClient::fixture` widened to `cfg(any(test, debug_assertions))`,
      with its own loopback verification origin, and device-flow validation
      checking the client's origin instead of the `VERIFICATION_ORIGIN`
      constant;
    - [x] The `qual:test_helper` marker removed from `MemoryCredentialStore`;
    - [x] The pure, unconditional `fixture_from_env(debug, api, workos)`, with a
      unit test that `debug = false` ignores both variables;
    - [x] The board's fixture branch under `#[cfg(debug_assertions)]`, and
      `cargo check --release` for the board crate among the slice's checks;
    - [x] The fixture picker calling `select_organization` on the fixture
      session and leaving the editor's snapshot unchanged, with the test
      `fixture_run_never_writes_config`;
    - [x] The test `fixture_sign_in_stays_on_loopback`: no poll to
      `api.workos.com` and no real AuthKit page;
    - [x] `scripts/nett-test-server.sh`, answering the device flow with the
      loopback verification origin.
  - The privacy tests, including the log test (section 8).
    - [x] Data-side log canaries and metadata-only `issues/config.json`.
    - [x] Board state/cache/search/chat and fixture Settings no-write tests.
  - Evidence: `list` (without the Assignee, Priority and Filters pickers and
    the counts), `list-loading`, `list-signed-out`, the no-organisation
    state, `error-session`, the *no longer belongs* state,
    `settings-profiles`, `signin-code`, `signin-waiting`, `signin-signed-in`,
    `signin-expired`, `signin-denied`, `signed-out`.
- [x] **3. The read-only issue view, first part** (M1)
  - The frame: Back, path, title with the state and Tracker badges, the
    identifier line and provenance.
  - [x] Description preparation (slice 3a): the pure NETT Markdown pre-processor
    in `spaces/issues/src/nett_markdown.rs`, with its direct `markdown` 1.0.0 dependency
    and `mise run audit`, and board-kit's
    `rich_text_prepared` option (section 9). Until the plugins exist, it
    writes *Image: {alt text}* for inline images (until slice 7), plain
    *@Name* for mentions, and for issue references a labelled link's label
    or a bare link's identifier as plain text (until slice 4). Every `$`
    outside code and link destinations is escaped. The
    `nett_markdown_keeps_all_text` test, with its `$` cases (`$$` plain,
    indented, quoted and in a list item, and `costs $5 and $10`) and its
    labelled and bare issue links. The captain chose the original service
    placement on 15 November 2026 so slices 2 and 3 could run in parallel. The issue workspace implementation
    placement follow-up moves the pre-processor and combined renderer tests
    into the Issues board, with one shared JSON fixture including origin,
    team keys and stage. Dashboard keeps literal prepared-input tests and
    no longer depends on NETT preparation. Slice 3b maps `clients/nett` DTOs
    into the board's plain input types and reuses the fixture for wiring coverage.
  - [x] issue workspace placement follow-up: move profiles, credentials, scalar
    resolution and the resolution cache into `clients/nett`; remove the old
    service crate after moving NETT Markdown into the board. No hub publish
    or IRC snapshot changes; contract adoption remains a separate phase.
  - Sub-issues with their states and **Show n more**.
  - Enter opens a row's issue; Escape and Back return to the list; `j`/`k`
    step through the list it came from.
  - States: loading, `error-session`, `error-access`, `error-not-found`,
    network, edge and offline.
  - Evidence: `tracker` and `issue` (description and sub-issues only, with
    mentions and references as plain text), `long-description`, `loading`,
    the four error states, `offline`.
- [ ] **4. IRC references and opening an issue** (M1, completes it). The
      IRC side (4a) moved to [board-contracts tasks](../board-contracts/tasks.md), phases 3 and 4.
  - `BoardNavigation` and `Dashboard::open_link` in the framework, with the
    return target (section 4).
  - The hub's snapshot for IRC, read on IRC's 50 ms timer; references live
    only while the Issues board is enabled and signed in (section 1).
  - Recognition in formatted and Markdown lines; issue links as
    references; opening through the scalar read.
  - The look: board-kit's shared reference plugin (Kit's `MarkdownPlugin`)
    for Markdown lines and issue text, `InteractiveText` tones for formatted
    lines, the normal and unavailable variants, with their entry in
    `docs/ui-components.md` (section 4).
  - **Open issue reference…** in the palette and the conversation's More
    menu; focus back to the message input (section 10).
  - `spaces/irc/help.md` and `docs/irc.md` describe references.
  - The plugin replaces slice 3's plain *@Name* and plain issue-link text
    in issue text; labelled links keep their labels.
  - Evidence, before and after: `irc-references` (without keyboard focus),
    `irc-open-issue`, `error-not-found` from IRC.
  - The slice merges on its fixture evidence and checks. **The M1 check** in
    spec section 12 runs on NETT dev as soon as the development access update's route works; it
    follows the merge and doesn't gate it.
- [ ] **5. The rest of the list and issue view**
  - [x] The list's Assignee, Priority and Filters menus, the hidden sub-issues
    summary and the My issues count (section 3).
    The list follows the approved desktop design. The original design images
    were [removed for publication](../public-repository-examples/README.md).
    Slice 2b's captures show where it differs:
    - [x] identifiers in the code font;
    - [x] each group's count straight after its name, as in NETT, not at the
      end of the line;
    - [x] 24 px side insets for the list, if the shared list chrome can take a
      per-board inset without changing other boards;
    - [x] tabs without glyphs (the shared tabs' `TabSpec::plain`), and the
      My issues count.
  - [x] Copy details includes the HTTP status: `ReadError` carries the optional
    status as safe metadata, and the real-500 regression inspects the copied
    text (Sol final review of slice 2, P2-3).
  - [x] List review follow-up: My issues does not retain its assignee when
    leaving the tab; active filters have their own empty wording; one assigned
    open issue has a singular spoken count. Each regression failed first.
  - The workflow line, the tracker note and **How tracker status works**,
    the Blocked notice, Relations and Pull requests with **Show n more**,
    Properties, Record and header progress, More (**Copy identifier**,
    **Reload**), and `y` and `Cmd/Ctrl+.`.
  - Evidence: `list` and `tracker` complete, `tracker-subissues`, `issue`
    without images.
- [ ] **6. Activity, comments and decisions**
  - The activity pages: **Comments** by default with replies, tombstones
    (including a null comment) and imports. **All** interleaves events
    by time, following the Activity filter change. **Show n earlier comments** is counted from the
    loaded entries; at most four pages read by themselves (section 3).
  - The Decisions panel with its per-decision path reads; no panel on 403
    or 404, and no path when `provenance` or `sourcePath` is null.
  - `m` focuses Activity.
  - Evidence: `activity-collapsed`, `activity-expanded`, `many-comments`,
    `empty-activity`, `keyboard-focus`, Decisions in `tracker-subissues`.
- [x] **7. Images and the viewer**
  - [x] `clients/nett` gains the file-grant POST and a byte fetch that sends no
    bearer, `X-Org-Id` or cookies, limited to the profile's file origin
    (plain `http` only on the fixture's loopback origin), and the hub gains
    its file fetcher, one per session.
  - [x] Grants, fetching, decoding and memory limits (section 6); the
    pre-processor's `subspace-media:` links and the board's media plugin;
    inline images that open the viewer; the Images panel beside the
    description from a 1,000 px main column, shown unless the first page is
    empty with no cursor; file chips; a 404 on an `around=` read opens the
    image alone.
  - [x] The viewer, with the `media-stage` theme token in every theme file and
    the host's palette test.
  - [x] Evidence: `issue` with images at wide and narrow widths,
    `image-viewer`, a persistently failed tile with Retry, and *Too large to
    preview here*. Inspected, unedited Xvfb frames are listed in `delivery.md`.
- [ ] **8. Reference tooltip**: moved to [board-contracts tasks](../board-contracts/tasks.md), phase 4.
  - Resolution on hover and focus, and the hub's reference cache (section
    4); tooltips from the reference plugin and `InteractiveText`;
    unavailable references turn muted once resolved.
  - Evidence: `irc-tooltip`, `irc-tooltip-unavailable`, signed-out and
    error tooltips.
  - **The full check** in spec section 12, on NETT dev after the merge; it
    doesn't gate it.
- [ ] **9. IRC transcript keyboard focus**
  - Design section 6's transcript model for every IRC reference kind; IRC's
    channel and member references in Markdown lines move onto the shared
    reference plugin, whose elements take focus (section 10); focus returns
    to the opened reference.
  - Evidence: `irc-references` with keyboard focus, before and after.
- [ ] **10. Nested sub-issues** (waits for the NETT progress and nested-issues rollout PR 3 on NETT dev)
  - NETT's tree reads, context parents with reasons, folds, ← and →, and
    the summary's *n under their parents*.
  - Evidence: `list-nested`.
- [ ] **11. Progress panel** (waits for the NETT progress and nested-issues rollout PR 4 on NETT dev)
  - Scope, Started, Completed, *Not counted* and *Over time* with
    `chart::Chart`, replacing header progress for trackers.
  - Evidence: `tracker` with the panel.
- [ ] **12. Batch reference resolver** (waits for the batch reference endpoint on NETT dev):
      moved to [board-contracts tasks](../board-contracts/tasks.md), phase 4.
  - Visible references resolved in batches, the same cache, the scalar
    reads removed (section 4).

## Outside these slices

- [ ] The Subspace API profile, once Kirk confirms its contract (spec
      section 13, question 1).
- [ ] Production NETT, under the production profile rollout.
- [ ] user avatars, cross-board part: moved to [board-contracts tasks](../board-contracts/tasks.md), phase 6.

### Board checklist: issues

Identity and registration
- [x] ID, title, description and icon: `Issues` in `spaces/issues/src/lib.rs`; `issues`, Issues, NETT issues/read only, `icons/issues.svg`.
- [x] Product descriptor and `BoardRegistration`: `framework/services/product/src/known.rs` and `products/subspace/src/main.rs`; `Issues::ARTWORK` owns the icon.
- [x] `RETAIN_WHEN_DISABLED`: default false; `link::stop` stops the hub and clears its content.

Layout and UI
- [x] List board hooks: `impl ListDashboard for Issues` supplies rows, groups, selection, folds and the shared list shell.
- [x] Toolbar, status, badge and busy: `render_toolbar` supplies the profile picker, `status` supplies count/freshness and `busy` follows list loading. The My issues tab displays the hub's organisation-wide open assigned count. No board badge because no background attention is raised.
- [ ] Palette commands: `palette.rs` supplies profile/team sub-lists, tabs, sub-issues, search, Assignee/Priority/Filters menu access, Sign in/organisation recovery, Retry and selected-row Copy title/Copy id. The shell supplies Reload. **Deferred because** full coverage also needs shared Copy details and arbitrary text-selection menu actions: those Framework controls expose no callable palette seam. A board-local replacement would duplicate their diagnostics/selection and confirmation behavior. Route that remaining Framework work separately; no later slice of the issue workspace implementation owns it.
- [x] Own keys: `keys::bind` uses `view_keys(ID)`; `Issues::keys` returns `keys::HELP`, displayed by the shared help footer. `tests/keyboard.rs` checks the bindings.
- [x] Menus/dialogs: `Issues::modal`, `dismiss` and `deactivate` report and close the board's menu; the host owns the Settings dialog.
- [x] Empty/loading/error states: `view/states.rs` and `scope.rs` supply Sign in (ConfigureBoard), Retry and the account-specific states; list loading and offline content are covered by headless tests.
- [x] `quit_warnings`: not applicable because the board starts only cancellable reads, with no user job or unsaved issue edits.

Settings and configuration
- [x] `settings()`: description in `lib.rs`; notifications and background updates explicitly false, as spec section 1 requires.
- [x] Service configuration: `settings::factory` edits the board-owned `issues/config.json`; `profiles::saved_settings` and `link::apply_saved_settings` hand saved preferences to the hub. Credentials remain in the session/keychain, outside editor drafts.
- [x] Feature flags: not applicable because this slice uses the product board enable switch, not an experimental feature. The descriptor enables Issues by default only in debug builds.

Data caching and refresh
- [x] Stored choices: `board.rs` and `lines::Folds` use `prefs` for profile, tab, team, sub-issues and folds; `tests/storage.rs` checks that no issue content is persisted.
- [x] Cache first: `poll_hub` rebuilds from the hub's in-memory snapshot on activation; `read_if_stale` then refreshes when required. Disk caching is forbidden by spec section 8.
- [x] Automatic refresh: not applicable because spec section 7 forbids timers and focus refresh. `background_updates` is false; explicit/activation reads run on the hub's API workers, outside render and the UI thread.

Search
- [x] Item search: `keys::FocusSearch`, `board::commit_search` and the debounced query implement `/`. `Opened` is not applicable because slice 2 opens no issue; opening an issue is deferred to slice 3.
- [x] Full-text index: not applicable because spec sections 1 and 8 explicitly forbid a search adapter and issue indexing; `Issues` retains the default `SEARCHABLE = false`.
- [x] Exact cross-board index references: not applicable because issue content must not enter the index; slice 4 owns explicit IRC references through the hub instead.

Agent tools, MCP and CLI
- [x] Read/write tools: no tools because spec sections 1 and 8 forbid exposing issue content to agents; `Issues` uses the empty default tool list, checked by the product composition test.
- [x] App-only tools: not applicable because Issues has no tools.
- [x] Tools/actions in `docs/mcp.md`: not applicable because this board contributes no MCP tools or CLI actions.

Chat context
- [x] `agent_context`: explicitly returns `None`; `menu_chat` and row menus use `MenuChat::Off`, with privacy regressions in `tests/chat.rs`.

Notifications and attention
- [x] Notifications: not applicable because spec sections 1 and 8 forbid issue notifications; `settings().notifications` is false.
- [x] Process notices/badge: not applicable because no attention notices are raised and background updates are disabled.

Help and docs
- [x] `spaces/issues/help.md` follows the board guide template and describes the list menus, memory-only filters, count scope and keyboard interaction. Fixture images are recorded in `delivery.md`; guide checks validate its resources.
- [x] Guide/developer links: `docs/README.md`, `docs/subspace/boards.md` and the product TOC link the guide and `docs/issues.md`.

Communicating with other boards
- [x] Shared state: `seam::poll_hub` reads the NETT `HubSnapshot`; `link` publishes that same hub's IRC projection. The board has no dependency on another board's crate or globals. No board events are published or consumed beyond the Framework's `DashboardEvent` seam.

Links, logging and diagnostics
- [x] URLs: Settings device approval goes through `subspace_dashboard::links`; no issue URL or indexed citation opens in slice 2. Issue/reference opening is deferred to slices 3 and 4.
- [x] Logging/diagnostics: `seam::request` logs only refusal metadata, the client logs sanitized request paths and safe failures, and `view/states.rs` builds safe Copy details, including the optional HTTP status from `ReadError::Response`. The real-500 regression clicks Copy details and inspects the clipboard. `Issues::diagnostic_config_path` returns the board-owned `issues/config.json` path through `config::board_file`; fixture sessions return None because their connection is in memory and ignores that file. Headless tests cover both cases.

Testing and evidence
- [x] Pure and headless tests: `tests/listing.rs`, `tests/sync.rs`, `tests/first_pages.rs`, `tests/list_filters.rs`, `tests/http_details.rs`, keyboard, layout, lifecycle, Settings and privacy suites exercise the board and the real offline hub.
- [x] Capture-build screenshots: seven slice 2 review frames, five complete slice 5 list/menu frames and the filtered empty state are inspected and listed in `delivery.md`, including both requested window widths. Slice 3's issue detail is delivered independently; this slice changes the list only.

### Board checklist for Issues detail

Identity and registration
- [x] The product already registers the Issues ID, title, description, icon and artwork. There is no retained process after the board stops.

Layout and UI
- [x] The issue page replaces the list inside the same board and retains the list state and focus.
- [x] Reload, busy and status follow the visible list or issue.
- [x] The palette offers Open issue, Back, pagination and the existing recovery actions. Breadcrumbs and child rows navigate directly. Copy details and text selection stay on the shared controls, which have no palette entry points.
- [x] Contextual keys and Help cover Enter, Escape and stepping through the list. Menus use the existing modal lifecycle.
- [x] Loading, unavailable, expired session, access, network, Cloudflare One and offline states offer recovery. Read-only requests need no quit warning.

Settings and configuration
- [x] The Issues settings editor owns sign-in, profile and organisation. Notifications and background refresh are off. The detail page uses the existing feature flag.

Data caching and refresh
- [x] Existing list preferences survive restart. Issue contents stay in the hub's memory. Offline recovery shows the current issue's last successful read.
- [x] Explicit Reload fetches the visible page through the existing hub workers.

Search
- [x] The existing slash shortcut searches the list. Issue contents are kept out of recent items and the full-text index. Cross-board activation is part of slice 4.

Agent tools, MCP, CLI and chat
- [x] This read-only UI adds no tools or commands. Issue contents are unavailable to chat.

Notifications and attention
- [x] The detail page needs no notifications or attention badges.

Help and docs
- [x] The linked Issues guide covers the issue page, pagination and return keys.

Communicating with other boards
- [x] The page reads the hub snapshot and leaves Hub::publish as the change point.

Links, logging and diagnostics
- [x] External Markdown links use the shared link policy. Team, parent and child navigation stays inside Issues.
- [x] Copy details omits the issue identifier, response body and credentials. The existing diagnostic configuration path applies.

Testing and evidence
- [x] Tests cover Enter, stepping, Escape and focus, Markdown, cursor failure and retry, and offline retention.
- [x] The macOS capture-build evidence is listed in [delivery.md](delivery.md#slice-3-read-only-issue-view).

### Board checklist for Issues images

- [x] Identity, product registration, artwork and process retention: existing Issues registration; no new board or retained user process.
- [x] Layout, toolbar/status/busy and palette: existing issue page; the viewer is a custom content overlay with native actions, using the existing board Reload/status. Viewer controls have direct contextual keys.
- [x] Keys, Help, menus/dialogs: viewer arrows, Escape and Tab are scoped inside the modal; `modal` reports Dialog and `dismiss` closes it and restores focus.
- [x] Empty/loading/error recovery and quit: image placeholders, Retry and over-limit wording; no quit warning because reads are cancellable and there are no edits.
- [x] Settings/connections/experiments: existing board config, profile and organisation; no new settings, connections or experiment flag. Notifications and background updates remain off.
- [x] Choices/data/cache/refresh: existing non-content preferences; image pixels and grants live only in memory. Disk storage and automatic refresh are not applicable because spec sections 6–8 forbid them. Explicit issue Reload re-reads images.
- [x] Item search/full-text/references: existing list search; image indexing and recent-content persistence are not applicable because private issue content must stay out of them.
- [x] Tools/MCP/CLI/chat: no additions because private image content is not exposed to agents. Existing empty tool list and `agent_context: None` apply.
- [x] Notifications/attention: not applicable because private images add no process notices or notifications.
- [x] Guide and developer docs: existing linked pages explain image tiles, file chips, viewer keys, limits, memory and fixture routes.
- [x] Board contracts/globals: existing hub projection and broker declarations; no image contract or other-board dependency. The shared token is the only theme integration.
- [x] Links/logging/diagnostics: external prose keeps the shared policy; grant URLs never reach UI, clipboard or provider error text. The privacy test covers actual file requests and log redaction.
- [x] Pure/headless tests: client, hub, cache, Markdown, decoder, palette and viewer interaction tests.
- [x] Capture evidence: nine inspected, unedited Linux Xvfb frames cover the image states, viewer positions and panel breakpoint; see `delivery.md`.

### Slice 7 review fixes

- [x] Grant network/5xx/404 errors fail only the tile; 403 clears private scope.
- [x] Comment reveal uses the loaded count, preserving bounded later reads and the four-page automatic limit.
- [x] Real PNG decoding reaches Ready; failed tiles recover through Retry; `around=` 404 opens a decoded image alone. Remove the unused `images-renew` scenario.
- [x] Unknown viewer positions hide counts and disable navigation; standalone state latches until close.
- [x] Keep replacement `around=` windows explicitly partial, including without a forward cursor; document the spec's unspecified replacement policy.
- [x] Neutralise authored private media/file schemes and show unsupported raster types as file chips.
- [x] Issue revisions retire stale image tasks and let Reload retry failures.
- [x] Opaque `media-stage` fallback for custom themes.
- [x] Recheck evidence and recapture the viewer states; list the final frames in `delivery.md`.
- [x] Leave review P2-8 (shared-pool blocking fetch) and P2-9 (LRU thrash) to the image loading and cache-pressure change.
