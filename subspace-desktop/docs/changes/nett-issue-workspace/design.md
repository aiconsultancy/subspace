# Subspace issue workspace design

The issue list and issue view follow NETT's desktop layout using Subspace's shell, tokens and components. Comments are always shown, other activity stays folded until opened, and costs, documents and every write are left out.

> **In this record.** This is the approved issue workspace design, with public examples and relative paths. The captain's decisions in [design-approval.md](design-approval.md) win where they differ, and [spec.md](spec.md) settles section 11. The design evidence used 1440 × 900 captures. The text below records the states and comparisons. The full package (the HTML mocks, all 128 captures, the NETT references and the capture tools) stays with the design hand-off and is not in this repository. Paths below that start `frontend/indra/` or `.impeccable/` are in the Indra repository.

- **Mocks:** `mocks/subspace-issues.html?s=<state>&t=<dark|light>` (one page, every state) and `mocks/compare.html?s=<state>` (NETT and Subspace side by side), in the design package.
- **Captures:** `{state}-{width}-{theme}.png`, at 1440 × 900 and 1100 × 720, in **Cupertino Dark** (Subspace's default) and **Cupertino Light**. Their colours, sizes and radii come from `framework/host/app/themes/cupertino.json` and its `subspace.density` block. The design review used the 1440 × 900 Cupertino Dark captures for its key states.
- **Side by side:** The comparison captures put NETT's approved Design 40.7 mock next to the Subspace version for each main state. Numbered notes under each pair say where every NETT region went.
- **NETT references:** `nett-*-1440-light.png` in the design package, captured from Indra's `.impeccable/mocks/issues-tracking-40-7-network-lines.html` in its clean screenshot mode. I checked the region order against the delivered React source in Indra (`frontend/indra/src/pages/issues/IssueRecord.tsx`, `IssuePropertiesPanel.tsx`, `activity/IssueActivitySection.tsx`, `workspace/IssueRow.tsx`, `workspace/IssueWorkspaceTabs.tsx`).

## 1. Placement

### Recommended: an Issues board, opened from IRC references

Add an **Issues** board to the board switcher (board id `issues`, Lucide `circle-dot`). It holds NETT's Issues list, and an issue opens in the same board at full width with NETT's two-column layout. A reference such as `TST-827` in IRC switches to the Issues board and opens that issue. The issue's back control is labelled with where you came from (**#ops-deck**). Back or Escape returns to the conversation, with focus on the reference you opened.

Captures: `list-*`, `tracker-*`, `irc-open-issue-*`.

Why this placement:

- **The owner asked for both surfaces at desktop width.** Only a board gives NETT's list and its issue page the window's full width. At 1440 px the main column is over 1,000 px, so the Images panel sits beside the description, as NETT's own rule says.
- **One issue view, one owner.** Subspace boards may not depend on one another's crates. IRC asks the shell to open an issue, as Search opens a result in the board that owns it, so the issue view lives in one place.
- **It fits the shell.** The board brings its own Settings page (profiles and sign-in), status line, reload, palette commands, keyboard map and Help guide, like every other board.

### Alternative: a tab in the IRC dock, split beside the conversation

The issue opens as a tab in a new group to the right of the conversation, using the dock's existing **Open to the right**. Capture: `placement-alt-irc-split-*`.

| Gains | Costs |
| --- | --- |
| The conversation stays visible while you read the issue. | The pane is about 580 px wide, so NETT's layout falls back to its single column: Properties end up at the very bottom, after Activity. |
| It reuses the dock's tabs, splits, resizing and saved layout. | The issue view would have to be shared between IRC and an issue list. A board can't use another board's crate, so it would need a shared crate for a NETT-specific view. |
| | There's still no home for the issue list the owner asked for. |

A narrow pane fails the owner's main requirement, so I recommend the board. The dock could later show a small preview, and the tooltip already covers a quick glance.

## 2. NETT's layout inside Subspace

**Subspace keeps** its window chrome, title bar and board tabs, status bar, theme tokens, type, density, Lucide icons and controls.

**NETT supplies** the regions, their order and their labels.

**Left out:** NETT's sidebar, search bar, notifications and organisation switcher. The **profile picker** sits in the board's title-bar controls instead, like the PRs board's account picker.

### Issue list (NETT's Issues workspace)

| NETT region | Subspace |
| --- | --- |
| Page title **Issues** | Heading **Issues** with the scope line *NETT dev · NETT organisation · read only*. |
| Tabs: My issues, Active, Backlog, All issues, Cycles, Views | Shared `tabs::tab_bar`: **My issues**, **Active**, **Backlog**, **All issues**. Cycles and Views are left out for now (see open questions). |
| Toolbar: search, team, Assignee, Priority, Filters; Sub-issues switch; List / Board | Search field, team picker **NET · NETT**, **Assignee**, **Priority** and **Filters** pickers. **Sub-issues** is a latched toolbar button, Subspace's control for an on/off mode. List only. |
| Summary: *n issues · n sub-issues hidden · Show sub-issues* | The same line and the same button. |
| Groups by state, with counts | Shared list group headers, folding as in every Subspace list. |
| Row: priority, identifier, state, title, progress or sub-issue count, labels, milestone, project, event, due date, estimate, assignee | The same lanes in the same order. As the window narrows, project goes first, then labels, as in NETT. |
| 40.7 nested sub-issues, folding and muted context parents | Same (`list-nested-*`): sub-issues sit indented under their parent, a folded parent says how many it hides, and an out-of-filter parent is muted with its reason and isn't counted. |

### Issue view (NETT's issue page)

| NETT region | Subspace |
| --- | --- |
| Back, then the path: team key and name, then the parent | Same. Path segments are quiet buttons, because they navigate inside the app. |
| Title, state badge and **Tracker** badge | Same. |
| Identifier, *Opened by / Added by …*, and the tracker's *Calculated from its sub-issues · n of m done* | Same. |
| Provenance: *Imported from Linear · TST-827 · originally by …* | Same. NETT dev's issues are Linear mirrors. |
| Workflow line, with Cancelled and Duplicate set aside | Same line, as a board-owned drawing. The current state is accent-filled. |
| Tracker note and **How tracker status works** | Same, as a read-only explanation popover. |
| Notices: Blocked, Archived / View only, stale data | Blocked: same words. Stale data becomes the offline banner (`offline-*`). |
| Main column: Description (with Images beside it from a 1,000 px main column), Decisions, Sub-issues, Relations, Pull requests, [Costs], [Documents], Activity | The same order. Costs and Documents are left out, as the brief says. |
| Side column: [Progress, trackers only], Properties, Record | Same. Properties keep NETT's order and labels: State, Priority, Assignee, Labels, Due date, Project, Milestone, Event, Parent, Type, Team. Estimate and Cycle appear only when the team uses them. |

**Progress panel (trackers only).** It follows approved Design 40.7: Scope, Started and Completed, *Not counted*, and *Over time*. NETT ships it in the fourth PR in the progress and nested-issues rollout. If Subspace's Build lands first, it shows NETT's current header progress and leaves the panel out until then (open question 8).

## 3. States and captures

Every state has four captures in the design package: `{state}-1440-dark.png`, `-1440-light.png`, `-1100-dark.png` and `-1100-light.png`. States marked † have their 1440 Cupertino Dark capture in [evidence/design/](evidence/design/).

| State | What it shows |
| --- | --- |
| `list` † | Issues list, Active tab, sub-issues hidden (NETT's default), one row selected. Fixture titles, states, priorities and labels (section 12). |
| `list-nested` † | Sub-issues shown under their parents, a folded tracker, and a muted context parent (TST-827, *In Backlog*) holding a Todo sub-issue. |
| `list-loading` | Loading line and skeleton rows, shaped like the list. |
| `list-signed-out` | Board empty state while the current profile is signed out: **Sign in…** and **Choose another profile**. |
| `tracker` † | **TST-827**, the top of the page: header, workflow line, tracker note, Blocked notice, Description, Progress and Properties. |
| `tracker-subissues` | TST-827 scrolled to Decisions and Sub-issues. It has 19 fixture sub-issues in a mix of states. One child tracker (TST-1606) is unfolded to show nesting. |
| `long-description` | TST-827's long description, scrolled to its Delivery order. The whole issue region scrolls, as on NETT's page, and its scrollbar sits at the region's trailing edge. |
| `issue` † | **TST-155**, an ordinary sub-issue: parent path, Description with inline images, the Images panel beside it, Properties and Record. |
| `activity-collapsed` † | TST-155's Activity: comments and replies always visible; **Show activity · 5 events** folded. |
| `activity-expanded` † | The same with activity open: events interleave with the comments by time, as NETT's *All* view does. |
| `many-comments` | TST-827 with 24 comments: the latest six, plus **Show 18 earlier comments**. |
| `empty-activity` | TST-1593 with no comments and no activity: *No comments or activity yet.* Decisions is hidden and Relations reads *No relations.* |
| `image-viewer` † | The in-app viewer: *1 of 3 on this issue*, Previous and Next, the image's description and **Show in the description**. |
| `loading` | Issue view skeleton with *Loading TST-155…*. Back is already available. |
| `error-network` | *Couldn't load TST-155*, with **Retry**, **Copy details** and **Show log** (Subspace's shared error recovery). |
| `error-access` | *NETT dev can't read issues in NETT*: the role or the organisation's pack. This depends on the account, not the issue, so it can be named. |
| `error-not-found` † | *TST-9999 isn't available*, reached from IRC. One neutral wording covers unknown, inaccessible and other-organisation issues, as NETT's resolver contract requires. |
| `error-session` | *Your NETT dev session has expired*: **Sign in…** or **Choose another profile**. |
| `offline` | The issue as last read in this session, under an **Offline** banner with **Retry**. The status bar says *Offline*. |
| `irc-references` † | IRC transcript with several references in one message, `TST-9999` shown as unavailable, `TST-1612` inside code left as plain text, and keyboard focus on `TST-864`. |
| `irc-tooltip` † | Hover tooltip on `TST-827`: identifier, title, state, *Tracker* and the profile it came from. |
| `irc-tooltip-unavailable` | Tooltip on `TST-9999`: *isn't available with NETT dev.* |
| `irc-open-issue` † | After clicking `TST-827` in IRC: the Issues board shows the issue, and back reads **#ops-deck**. |
| `placement-alt-irc-split` | The alternative placement (section 1). |
| `settings-profiles` | **Settings › Board › Issues**: NETT dev signed in, with its organisation and *In use*; Subspace API not signed in. |
| `signin-code` † | Device sign-in started: the verification address (an external link), the code, **Copy code**, **Open sign-in page**, and *Waiting for approval · code expires in 4:58*. |
| `signin-waiting` | After the browser opened: *Waiting for you to approve in your browser…*, with **Open again** and **Cancel**. |
| `signin-signed-in` | Subspace API signed in, with its organisation picker and **Use for issues**. |
| `signin-expired` | *The code expired before it was approved. No session was stored.* **Get a new code**. |
| `signin-denied` | *Sign-in was denied in the browser. No session was stored.* **Try again**. |
| `signed-out` | After **Sign out** on NETT dev: the note in Settings, and the board's signed-out state behind it. |
| `keyboard-focus` | Keyboard focus ring on **Show activity**. |

Side-by-side captures (all `-1440-light.png`, all in [evidence/design/](evidence/design/)): `compare-list`, `compare-list-nested`, `compare-tracker`, `compare-issue`, `compare-activity` and `compare-image-viewer`.

## 4. References in IRC

- **Recognition.** A word that matches `KEY-number`, where `KEY` is an issue-team key the current profile's organisation knows, retired keys included (for example `TST-1612`). Other capitalised hyphenated words are left alone, such as `UTF-8` and `ISO-8601`. Text in code spans and code blocks stays plain. NETT's own issue links and linear.app issue links in Markdown messages count as references too, so they open in Subspace rather than the browser.
- **Look.** The same accent-on-tint treatment as `#channel` and `@nick` references. Once resolved, an unavailable reference turns muted with a dotted underline, so it no longer promises an issue.
- **Resolution.** Visible references are resolved in batches with NETT's reference resolver, up to 50 per call. Results are kept in memory per profile for a short while. Unknown and inaccessible references get the same neutral result and wording.
- **Tooltip** (nice-to-have). On hover, or on keyboard focus after the usual delay: identifier, title, state glyph and name, *Tracker* where it applies, and the profile name. The tooltip is a popover surface, never a capsule.
- **Signed out.** While the current profile is signed out, Subspace can't read the organisation's team keys, so references show as plain text. They come back when you sign in.
- **Opening.** Click or Enter opens the issue in the Issues board, and Back or Escape returns to the conversation. An unavailable reference opens the neutral *isn't available* state, which leads back to the conversation (`error-not-found-*`).
- **Several references in one message** each keep their own target and tooltip, and each is its own stop for Tab.

## 5. Sign-in and profiles

- **Where.** Profiles live in **Settings › Board › Issues**: one entry per API profile, in the same row style as IRC's network list. **NETT dev** uses its configured address, illustrated by `https://nett.example.com/api`. **Subspace API** uses its own address. Production NETT is added later and isn't shown until it exists.
- **Device sign-in, as `indractl` does it.**
  1. Request a code.
  2. Show the verification address, as an external link, and the code, with **Copy code**.
  3. **Open sign-in page** opens the system browser. This is the only place Subspace leaves the app.
  4. Wait for approval. The code's expiry counts down, and **Cancel** abandons the attempt.
  5. The profile is signed in.

  An expired or denied code stores nothing and offers a new code. The words follow `indractl`'s errors (`WorkOsDeviceClient.cs`), in plain English.
- **Each profile signs in separately** and has its own organisation. A picker appears when the account belongs to several organisations.
- **The current profile** is chosen with the board's title-bar picker. It serves both the Issues board and IRC references, and Settings marks it *In use*.
- **Sign out** removes that profile's stored session at once. References then show as plain text, and the board shows its signed-out state.
- **Sign-in and sign-out take effect at once.** Settings says so, because they don't wait for Save or Cancel (see departures).

## 6. Keyboard

Plain keys work only while no text field, menu or dialog has focus. They follow the PRs board's map where NETT has no read-only equivalent.

| Where | Key | Action |
| --- | --- | --- |
| List | `j` / ↓, `k` / ↑ | Next or previous row. |
| List | ← / → | Fold or unfold a parent (nested), or step to it. |
| List | Enter | Open the issue. |
| List | `1` `2` `3` `4` | My issues, Active, Backlog, All issues. |
| List | `Shift+S` | Show or hide sub-issues (NETT's key). |
| List | `/` | Search issues. |
| List | `u` | Switch profile. |
| Issue | Escape | Close an open menu or tooltip first, then go back to the list, or to IRC when opened from a reference. Focus returns to the row or reference that opened it. |
| Issue | `j` / `k` | Next or previous issue in the list it was opened from (Subspace's detail convention). Not available when opened from IRC. |
| Issue | Tab / Shift+Tab | Back, path, More, then the main column in reading order (references and images in Description, Images, Decisions, Sub-issues rows, Relations rows, Pull requests, **Show activity**, **Show earlier comments**, references in comments), then the side column (How tracker status works, Parent). |
| Issue | Enter / Space | Open the focused issue, reference or image, or toggle **Show activity**. |
| Issue | `m` | Move focus to Activity (NETT uses `M` to reach the comment box). |
| Issue | Space, Shift+Space, Home, End | Scroll the issue region. |
| Issue | `y` or `Cmd/Ctrl+.` | Copy the identifier. |
| Both | `r`, `?`, F-keys, `Cmd+K` | Reload, Help and the shared shell keys. |
| Viewer | ← / → | Previous or next image across the whole issue. |
| Viewer | Escape | Close, and return focus to the tile or inline image that opened it. |
| Viewer | Tab | Close, Previous, Next, Show in the description. |
| IRC | Shift+Tab from the message input | Move into the transcript, at the newest message. |
| IRC | ↑ / ↓, then Tab / Shift+Tab | Move between messages, then between the references and links in a message. |
| IRC | Enter | Open the focused reference. |
| IRC | Escape | Back to the message input. |

The IRC transcript's focus model is a proposal: the current board doesn't document one (open question 10).

## 7. Where Subspace differs from NETT

| NETT | Subspace | Why |
| --- | --- | --- |
| Activity: *All / Comments* tabs. All is the URL default. | Comments are always visible, and other activity is folded behind **Show activity · n events**. Opened, events interleave as in *All*. | Owner direction. NETT's own default is the Activity filter change's subject. |
| New issue, quick edits, drag ordering, row menus | Left out. A row opens the issue. | Read only. |
| Subscribe, Edit description, Add sub-issue, Add relation, Log decision, comment composer, Reply, Edit, Delete, Resolve | Left out. | Read only. |
| Property editors | Plain read-only values without editing hovers, with NETT's placeholders (*No labels*, *No due date*). | Read only. |
| More actions (convert, move team, archive, log decision) | **More** holds only **Copy identifier** and **Reload**. | Read only. |
| Cycles and Views tabs, Board display | Left out for the first release. | Smallest useful list (open question 5). |
| Relations: Next and Previous cursor pages | **Show n more** below the first page. | A Subspace list control. Same bounded reads. |
| Pull requests open on GitHub | Rows open the PR in Subspace's PRs board (proposal). | Keeps navigation in the app. |
| Decision titles open the decision page | Title, status and date, and the decision's current identifier (its repository path). Not opened yet. | No decision reader in scope (open question 6). |
| Image viewer **Download** | Left out. | Open question 7. |
| Costs and Documents panels | Left out. | Brief. |
| Sidebar, search bar, notifications, organisation switcher | Subspace's shell. The organisation is fixed per profile in Settings. | Owner direction. |
| Square Network Lines state glyphs, burnt-orange current state | Lucide circles in Subspace tones (dashed for backlog, hollow for unstarted, half for started, check for completed, cross and slash for cancelled and duplicate). The current workflow node uses the accent colour. | One icon family. Status is never shown by colour alone. |
| Comment rail with square markers | Subspace's message rows with `avatar::sender` initials, as in IRC. Events are compact muted lines. | Subspace's messaging look. |

## 8. Departures from the GPUI Kit guides and existing Subspace UI

1. **Page regions inside the board.** The tabs, filters and summary sit in the content area, as NETT has them, not in the title-bar toolbar where PRs and Actions put theirs. The title bar keeps only the profile picker. *Why:* the owner asked for the same regions in the same places.
2. **The issue view replaces the list** instead of using the shared list-and-detail split, where a 720 px panel sits beside the list. *Why:* NETT's issue page and its two columns need the board's width. Escape and Back return to the list, as Logs and Search return to the board.
3. **NETT panels become GPUI Kit `GroupBox`es**, a title row above a `group_box` surface, instead of the detail panel's flat hairline sections. Comments inside Activity aren't boxed, so no card sits inside another card.
4. **Prose stays near 75 characters a line** inside a full-width box, which leaves space on the right at 1440 px. *Why:* NETT's 40.7 measure rule. The structured panels use the full width.
5. **Issue references inside issue text** use IRC's accent-on-tint reference treatment, not plain links. *Why:* they open in the app, and the guide says in-app navigation mustn't look like a web link.
6. **New reference variant: unavailable.** Muted with a dotted underline. It needs a catalogue entry beside IRC's channel and member references.
7. **Board-owned drawings:** the workflow line, the tracker progress strip (16 or fewer) and the Progress panel's *Over time* chart. More than 16 uses Kit `Progress`, and the chart uses the shared `chart::Chart`. They are domain drawings, like the Actions job graph.
8. **Service colours for NETT labels.** Label swatches keep NETT's own label colours, as GitHub labels do through `Glyph::rgb`.
9. **Media stage.** The image viewer uses a near-black backdrop in both themes, as NETT and Quick Look do. The mock uses raw values; the Build needs a theme token such as `media-stage`, not literals.
10. **Settings' Save boundary.** Signing in and out applies at once, inside a dialog where everything else waits for Save. *Why:* an approved device grant has already created a NETT session, so holding it as a draft would leave a live session that Cancel can't undo. Settings states this.
11. **One external link.** The verification address is a `links::external` link, and **Open sign-in page** opens the browser. Nothing else leaves the app.
12. **The board heading repeats the tab name.** The guide's context economy would drop *Issues*. I kept it for NETT familiarity and because its line carries the scope (profile, organisation, read only). It can go if the captain prefers.
13. **Capture fonts.** The capture machine runs Linux, so Inter and Liberation Mono stand in for the system font and Menlo. Spacing and sizes follow the Cupertino density tokens; the glyph shapes differ slightly from macOS.

## 9. Component map for the Build

| Part | Reuses | New |
| --- | --- | --- |
| Board | `Dashboard`, `BoardRegistration`, product descriptor, board artwork (Lucide `circle-dot`) | `spaces/issues` (`subspace-issues-board`) and its `help.md` |
| NETT access | `subspace-core` private config/credential storage, `cx.background_spawn` | `clients/nett`: HTTP, WorkOS device flow, refresh, typed reads, reference resolver |
| Title-bar profile picker | `anchored::dropdown_menu` or `menu::MenuAnchors`, like the PRs account picker | — |
| Heading and tabs | typography roles, `tabs::tab_bar` (with counts) | — |
| Toolbar | `fields::input` (search), `menu::MenuAnchors` pickers, `IconButton::latched` (Sub-issues), `overflow_toolbar` | — |
| List | `list` (grouped rows, folding, keyboard selection, `row_leading`), `badge`, `avatar::member`, `truncated` | Row lanes: priority, state and progress glyphs; nested context rows |
| Issue frame | Kit `Scrollable`, `action::icon_button` (back, More), `anchored::dropdown_menu`, `badge`, `truncated::text` | Header composition, workflow line |
| Notices | `feedback::error_banner` (offline), Kit `Alert` (warning) for Blocked | — |
| Panels | Kit `GroupBox` | A shared panel title row (icon, title, count, hint) if a second board wants it |
| Description and comments | `rich::rich_text_with_links` (the board claims issue and NETT/Linear links), `images` (bounded decode) | Authenticated inline image blocks (shared rich text refuses remote images) |
| Images panel and viewer | `images`, the chat canvas gallery's keyboard pattern, `dialog::backdrop` | Tile strip and viewer overlay |
| Sub-issues, Relations, Decisions, Pull requests | `action::row_button`, `Glyph`, `badge`, `avatar::member`, `rich::plain_text` | — |
| Activity | `chat_bubble::message`, `avatar::sender`, IRC's compact-line pattern, `gpui_kit::Collapsible`, `feedback::retry_action` | — |
| Properties and Record | The two-column read-only grid (as Report issue's diagnostics) | — |
| Progress | `chart::Chart` (line and area), its Data table | Figures row |
| Loading and errors | `feedback::skeleton`, `feedback::loading_line`, `feedback::error_box_with_actions`, `copy_details::CopyDetails`, `feedback::recovery_action` | — |
| IRC references | `markup.rs` reference finding, `rich_text_with_links`, Kit `Tooltip` | Issue-key matcher, reference tooltip card, unavailable variant |
| Open from IRC | The shell's navigation request (as `SearchNavigation`) with a return target | — |
| Settings | `Dashboard::settings_editor`, `fields::heading`, `note`, `form_field`, `toggle::toggle_row`, `anchored::select` (organisation), `links::external`, `copy_details::copy_text`, Kit `Spinner` | Inline device-code panel |

## 10. Open questions for the owner and captain

1. **Placement.** Approve the Issues board, with IRC references opening into it, rather than the IRC split?
2. **TST-827's state.** The sample, Linear and NETT dev all say Backlog. But its sub-issues include Done and In Progress work, and NETT's rule would calculate a tracker from them as started (In Progress). Is the dev record a tracker, and is it being recalculated? The mock shows Backlog, as the sample says.
3. **One current profile.** Should one profile serve both the board and IRC references, or should each IRC network map to a profile?
4. **Subspace API profile.** What is its address, does it serve NETT's issue REST contract, and does it sign in through WorkOS too? The mock uses a placeholder address.
5. **List scope.** Are Cycles, Views and the Board display out of the first release?
6. **Decisions.** Show them by title and repository path only, or add a read-only decision reader later?
7. **Images.** Add **Save image…** to the viewer?
8. **Progress panel.** Ship it with the Build (it needs NETT's progress read from the NETT progress and nested-issues rollout's fourth PR), or follow NETT's current header progress until NETT ships it?
9. **Links to other sites in issue text** (GitHub, docs): open in the browser as Subspace normally does, or in the Web board? NETT and Linear issue links always stay in the app.
10. **IRC keyboard model.** Is the transcript focus model in section 6 acceptable for the IRC board?
11. **Sign-in effect.** Is immediate sign-in and sign-out inside Settings acceptable?

## 11. What the specification must settle

- **Reads by part:**
  - List: issue list keyset pages with tab, team, assignee, priority and filter parameters; 40.7 nested branch reads and context parents; *My issues* needs the signed-in member.
  - Issue: issue detail (the first 20 sub-issues and relations, and tracker progress samples), and the team's workflow for the line.
  - Further pages: sub-issue and relation pages.
  - Decisions: the decision list by issue, which needs `projects.view`.
  - Pull requests: linked PRs (40.6).
  - Activity: comment roots and threads in pages, and typed activity pages (40.5).
  - Images: the whole-issue image read (40.7).
  - Tracker progress: the 40.7 progress read.
  - References: the issue-team list (current and retired keys), for recognising references.
- **The reference resolver.** Batching (up to 50 per call, a 16 KiB cap), in-memory cache lifetime, resolving only visible lines, cancellation on profile switch, and neutral `unavailable` handling.
- **Authentication:**
  - WorkOS device flow per profile, and where tokens are stored (the private config store, never UI state).
  - Refresh, and signing out (local removal, or revocation too).
  - NETT dev's private edge access through Cloudflare One / WARP (the development access update, the edge access follow-up), and the production switch (the production profile rollout).
- **Organisation.** Choosing it per profile, and what happens when membership changes.
- **Error mapping.** 401 means the session expired; 403 means account access (role or pack); 404 means unavailable; then network failures and timeouts.
- **Images:**
  - Batched private file grants (they expire within five minutes) and renewal.
  - Bytes fetched off the UI thread, bounded decode and size limits.
  - Failed tiles, and clearing everything on access loss.
  - Never written to disk.
- **Freshness and live updates:** reload on open and on `r`, whether the board's automatic refresh (about 60 s) applies, refresh when the window regains focus, and whether NETT's realtime invalidation is worth using.
- **Privacy and caching.** Issue content stays in memory only: not in `state.json`, the board cache or the search index. It is cleared on sign-out and profile switch. Offline shows only what this session has already read.
- **Markdown.** NETT's Markdown subset, mention rendering, and the elements and links the Linear mirror leaves in descriptions (`linear.app` issue links become references).
- **Delivery evidence.** Keyboard and focus tests, headless render tests, capture evidence under `docs/changes/<slug>/evidence/`, and the board's Help guide.

## 12. Data and mock notes

- **Fixture data.** The captures in this record use fixture data throughout, as Subspace's evidence rules require.
  - People: Jo Fixture, Ada Fixture, Bo Sample and Sam Fixture.
  - Titles, descriptions, comments, projects, labels, decisions and the pull request (`example/app#3307`) are synthetic, in the shapes and lengths of NETT dev's records. No customer or company is named.
  - The fictional TST key and identifiers (TST-827, TST-155 and the rest) keep NETT dev's shape.
  - The NETT dev profile shows `https://nett.example.test/api` in captures; the deployment address is configured separately.
  - The NETT halves of the compares are NETT's own approved 40.7 mock, whose people and organisation are fictional.
- **Illustrative:** progress counts, assignees, the states of relation targets and the decisions linked to each issue.
- **Placeholders:** `nett.example.test`, `auth.example.test`, `subspace.example.test` and `example.org`.
- **Regenerating** (the tools are in the design package, not this repository):
  - all captures: `node tools/capture.cjs shots <states> 1440x900,1100x720 dark,light`;
  - the NETT references and comparisons: `node tools/shoot.cjs tools/ref-jobs.json` and `node tools/shoot.cjs tools/compare-jobs.json`.

  Both use the playwright-core bundled with the machine's existing `playwright-cli`, with no installs.
