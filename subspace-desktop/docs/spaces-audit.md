# The Subspace App's Spaces: tools, contracts and audit

What the Subspace App's Spaces declare and use: the agent tools it serves,
the contracts its Spaces exchange, and which Framework integration points each
Space uses today. The Framework's side is in
[What the Framework gives a board](../../subspace-framework/docs/board-integration.md) and its
[MCP server](../../subspace-framework/docs/mcp.md). Tests in `products/subspace`
compare the tool and declared-contract tables with the compiled registrations,
in the release configuration and with the unreleased Spaces built in.

Space Lab (`space-lab`) and the Component gallery (`component-gallery`) are
not ready to ship. Each is in the app only with its Cargo feature of the same
name (`unreleased-spaces` turns on both), which release and packaging builds
leave off; built in, each starts off. Their rows below describe such a build.

## Tools

Subspace serves these tools. A disabled board contributes none of its
rows. The two shared search tools appear only when the product has a
searchable board and the search index is available; their arguments are
fields, not actions. A test in the product crate compares this table with the
compiled catalog.

| Board | Tool | Actions |
| --- | --- | --- |
| Active boards | `board_help` | `list`, `get`, `image` |
| Shared search | `search_content` | — |
| Shared search | `read_search_result` | — |
| Shared search | `related_items` | — |
| Web research | `web_read` | — |
| GitHub | `github_read` | `scope`, `github_search`, `github_item`, `github_diff`, `refresh` |
| GitHub | `github_write` | `github_comment`, `github_update_pr`, `github_review`, `github_assignees` |
| Copilot Agents | `github_copilot_agents_read` | `scope`, `tasks`, `task`, `refresh` |
| Copilot Agents | `github_copilot_agents_write` | `github_agent_task` |
| IRC, running app only | `irc_read` | `networks`, `buffers`, `messages`, `members`, `search` |
| IRC, running app only | `irc_write` | `message`, `join`, `part`, `connect`, `disconnect` |
| Space Lab alpha, `space-lab` builds on macOS ARM64 | `space_lab_project_inspect` | `schemas`, `list`, `project`, `source`, `state`, `settings_names` |
| Space Lab alpha, `space-lab` builds on macOS ARM64 | `space_lab_project_edit` | `create`, `save`, `edit_file`, `apply`, `start`, `stop`, `close`, `set_value`, `invoke`, `export` |
| Space Lab alpha, legacy fixtures | `space_lab_inspect` | — |
| Space Lab alpha, legacy fixtures | `space_lab_edit` | — |
| Space Lab alpha, legacy fixtures | `space_lab_run` | — |
| Space Lab alpha, legacy fixtures | `space_lab_environment` | — |

Terminal advertises no tools.

The IRC tools act on the board's live connections, so only the running app
serves them: Subspace chat and **Serve MCP over HTTP**. `subspace mcp` and the
generated commands run without the app and list no IRC tools; they never open
a second connection under your nick. To reach IRC from another harness, turn on
**Serve MCP over HTTP** and connect the harness to the app's URL. See
[IRC](irc.md#agent-tools).

Release builds do not include Space Lab or its tools; a build with the
`space-lab` feature includes it, off by default. Its tools take one flat
object with an `action` field, not a write batch. See [Space Lab](space-lab.md)
for enablement, provisioning and execution limits. Unsupported platforms do
not include these tools.

Native project writes require the expected definition and source revisions.
Input and action requests also name the accepted session and input revision, so
a request cannot accidentally target a restarted worker. Start and Apply name
the selected environment variable names and requested destinations in their
approval disclosure. They never accept credentials as ordinary tool arguments.
Saved source does not execute until Start or Apply.

Project state includes values, options, enabled controls, results with their
scope and evaluation time, and bounded shared action outcomes. The host filters
exact configured private values before returning this state. Runtime code can
still encode or transform delegated values; this filter cannot guarantee that
arbitrary project output contains no sensitive information.

A read tool takes one action: `{"action":"scope"}`. A write tool
takes 1 to 20 operations of its own board:
`{"operations":[{"action":"github_comment","key":"owner/repo#7","body":"..."}]}`.
The tools keep every limit the dashboards apply: only the configured
repositories and no credentials in results.

**Writes run without a Subspace prompt.** Over MCP, the client that calls the
tool is responsible for asking its user. Subspace still checks every operation
and reads its target before the first change, runs each operation once in order
and never retries one. Write tools are annotated `destructiveHint: true`, so
clients that ask before destructive tools ask for these. When a request carries
a progress token, a write tool reports progress before each operation. A
cancelled call starts no further operation.

MCP also publishes read-only resources. `resources/list` contains the concrete
`subspace://help` resource when the help capability is available. Parameterised
resources are listed through `resources/templates/list`: search uses the RFC
6570 template `subspace://search{?q,boards}`, and indexed documents use
`subspace://cache/{board}/{account}/{id}`. `resources/read` accepts the expanded
URIs and returns the JSON body of the corresponding read capability; no write
capability is exposed as a resource.
HTTP requests are limited to 60 requests per bind per minute and receive
`429 Too Many Requests` after the limit.

## Contracts

| Contract | Messages | Answers and publishes | Subscribes and asks | Status |
| --- | --- | --- | --- | --- |
| `issues` v1 (`subspace_contracts::issues`) | `current-scope` request: profile, API origin and project keys, or none while unavailable. `resolve` request: one resolution per identifier (summary, unavailable, pending, signed out or failed), at most 50; `refresh` request: read up to 50 identifiers again after the asker changed them, keeping the old summary until the new read lands, then `changed`; a summary's optional facts include whether the issue is flagged Ready for agents (`IssueSummary::ready_for_agents`, from the API's `readyForAgents`). `changed` event | The Issues board, from its Subspace API hub | Channels, to recognise and show issue references ([Channels issue references](irc.md#issue-references)); web Spaces, for app pages allowed `issues` ([Show issues](web-spaces-reference.md#show-issues)); Decisions, which asks for the scope to open a decision's linked issues in the answering Space ([Decisions](decisions.md#linked-issues)) | Delivered: phases 3 and 4 of the [delivery plan](changes/board-contracts/tasks.md); per-reference HTTP reads, with batch HTTP resolution to follow |
| `github` v1 (`subspace_contracts::github`) | `open-pull-request`: navigation responder role for a validated GitHub PR URL. `resolve-pull-requests` request: one resolution per `owner/repo#n` key (summary, unavailable, pending, signed out or failed), at most 50; a summary has the combined check state and, when the pull request's check runs were read with it, how many passed of how many (`check_counts`). `changed` event | PRs, through `Dashboard::open_link`; summaries from what its current account loaded and a refresh confirmed, else one background read; a signed-out login answers signed out until it signs in or reads again | Issues, from its pull requests panel and palette. Channels, for pull request chips and hover tiles ([change record](changes/subspace-api-accounts/README.md)) | Delivered: [pull request navigation](changes/subspace-api-accounts/README.md) and [change record](changes/subspace-api-accounts/README.md); unavailable responders and unsupported links fall back to the browser |

**Pull request navigation.** `github@1/open-pull-request`
(`subspace_framework::contracts::github::OpenPullRequest`) is a navigation responder
role. The PRs board declares `responds`; a requester declares `requests`.
Build the navigation with
`subspace_framework::dashboard::contracts::github::pull_request_navigation(url)` and
emit `DashboardEvent::Navigate` with the returned value. The shell resolves
the running responder through the broker, calls its `Dashboard::open_link`
and switches boards only when it returns true. This role needs no attached
`contracts::respond` handler and is not sent with `contracts::request`.

The shared `github::recognition::PullRequestLink` parser accepts canonical
`https://github.com/<owner>/<repo>/pull/<number>` links. Owner names use up to
39 ASCII letters/digits with single internal hyphens; repository names use
up to 100 ASCII letters/digits, periods, underscores and hyphens, except `.`
and `..`. Numbers must be positive and below the GitHub client's one-billion
limit. Ports, userinfo, encoded or rewritten paths, path suffixes, queries
and fragments are refused. The helper normalises leading zeroes in numbers.
No account or credentials travel with navigation: the PRs board uses its
current account and its normal detail reader, including for a pull request
outside the loaded searches. With no running responder, navigation does
nothing. Requesters must keep the external GitHub action available.

**Pull request summaries.** `github@1/resolve-pull-requests`
(`ResolvePullRequests`) asks the PRs board for short summaries of at most 50
canonical `owner/repo#number` keys. It answers with its current account from
what it already loaded; any other key is `Pending` while one background read
fetches it, and `github@1/changed` announces the landing. A pull request the
account cannot read, or an issue with that number, is `Unavailable`; with no
account or after a sign-out the answer is `SignedOut`. An account switch,
sign-in, sign-out or reload forgets every answer and publishes `changed`, so
askers keep nothing across a `changed`. The PRs board creates its view at
window start, so it answers before it is first opened. Summaries hold pull
request content: they stay in memory and the types do not implement `Debug`.
A summary's check counts (`PullRequestDetails::check_counts`, passed of
total, a skipped or neutral check counting as passed) come only from check
runs the board has read: a pull request it opened or read for a resolution
has them. One it knows only from a list answers at once with the combined
state alone and is read once for its runs (the same deduplicated read an
unknown key gets); later answers carry the counts. A list refresh (every
section, or the favourites' sections) marks those reads stale once, when
its first section lands, so each is read again at most once per refresh,
when next asked, however many sections land or pull requests are opened.
Until then a stale read lends only its counts to the list's item, and only
while the list's combined state and last update match the read's. A read
of a pull request the board neither opened nor lists is checked again once
per refresh too, so one the account can no longer read stops answering. A
signed-out account starts no read.

```rust
// In the requester's contracts():
const CONTRACTS: BoardContracts =
    BoardContracts::new(&[Declaration::requests::<OpenPullRequest>()]);

// In the requester's in-app Open action:
if let Some(link) = contracts::github::pull_request_navigation(url) {
    cx.emit(DashboardEvent::Navigate(link));
}
```


**Subspace API initiative session.** Issues and Channels observe the
desktop client's initiative catalogue and session selection. The service
owns this state for both consumers, as it owns the membership read and
account invalidation. Each Space consumes the service directly through
the desktop client interface and applies the selection locally.

## Declared contracts

Every contract role a registered board declares, as
`| board | role | message |` rows. The product test fails when this list and
the registrations differ.

| `irc` | subscribes | `issues@1/changed` |
| `irc` | requests | `issues@1/current-scope` |
| `irc` | requests | `issues@1/resolve` |
| `irc` | requests | `issues@1/refresh` |
| `irc` | subscribes | `github@1/changed` |
| `irc` | requests | `github@1/resolve-pull-requests` |
| `irc` | requests | `github@1/open-pull-request` |
| `issues` | responds | `issues@1/current-scope` |
| `issues` | responds | `issues@1/resolve` |
| `issues` | responds | `issues@1/refresh` |
| `issues` | publishes | `issues@1/changed` |
| `issues` | requests | `github@1/open-pull-request` |
| `decisions` | requests | `issues@1/current-scope` |
| `github` | responds | `github@1/open-pull-request` |
| `github` | responds | `github@1/resolve-pull-requests` |
| `github` | publishes | `github@1/changed` |

Configured web Spaces are not in the product registrations this test reads:
each one declares the same three roles as `irc` under its own configured ID
(`spaces/web/src/bridge/issues.rs`). Only a page whose manifest asks for
`issues`, and whose user allowed it, reaches the contract.

The Issues board answers whenever enabled, including fixture builds. With it
disabled, requests are unavailable and references stay plain. Debug capture
builds can alternatively route a stand-in responder for isolated IRC evidence
(`SUBSPACE_CAPTURE_ISSUES`, `subspace-framework/host/app/src/capture/issues.rs`).

## Candidate contracts

| Contract and message | Publisher or responder | Subscribers or askers | What it replaces or enables | Plan phase |
| --- | --- | --- | --- | --- |
| `references` `mentioned` event: reference, framework store document, time; no message text | IRC, for a line with a GitHub pull request or issue link | PRs: mark the pull request as discussed and refresh it | Nothing today. IRC's messages in the framework store record the same reference for `related_items` | 5 |
| `people` avatar request and `changed` event | The board holding a person directory (Issues, for NETT users) | IRC: show a person's avatar | the avatar support change's cross-board part; design first | 6 |
| `pull-requests` `changed` event: reference, state, checks, review decision, head | PRs background refresh | Copilot Agents: update the task tied to that pull request. IRC: refresh a reference tooltip | Each board polling GitHub on its own | 7 |
| `actions` `run-finished` event: repository, branch, commit, conclusion | Actions poll | PRs: refresh checks on the pull request with that head | The PRs board waiting for its next refresh | 7 |

Favourite repositories and chat sessions do not need contracts: they are
Framework-owned latest values, and the globals already serve them.

## Board audit

The state of `feat/subspace-desktop` at the time of writing (November 2026).
✓ implemented, – not applicable, ✗ a gap to implement, ○ worth considering.
Column names: PRs (`github`), Agents (`github-copilot-agents`), Actions,
IRC, Lab (`space-lab`), Gallery (`component-gallery`) and Web (configured
web boards). The terminal is a Framework panel, not a board; see
[Terminal](terminal.md). The Issues board's audit follows this table.

| Integration point | PRs | Agents | Actions | IRC | Lab | Gallery | Web |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ID, title, description, icon | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Board artwork | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | – ¹ |
| Help guide | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Help images | ○ | ○ | ✓ | ○ | ✓ | – | – |
| Settings page description | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Service connections or own editor | ✓ | ✓ | ✓ | ✓ ² | – | – | ✓ ³ |
| Feature flags | – | – | – | – | ✓ | – | – |
| List board hooks | ✓ | ✓ | ✓ | ✓ | – | – | – |
| Toolbar | ✓ | ✓ | ✓ | ✓ | – | – | ✓ |
| Status and busy | ✓ | ✓ | ✓ | ✓ ⁴ | ✓ | – | ✓ |
| Badge | ○ | ✓ | ✓ | ✓ | ○ | – | ✓ |
| Palette commands | ✓ | ✓ | ✗ | ✓ | ✗ | – | ○ |
| Key help matches the board | ✓ | ✓ | ✓ | ✓ | ✗ ⁵ | ✓ | ✓ |
| Stored choices | ✓ | ✓ | ✓ | ✓ | ✓ | – | ✓ |
| Cache of loaded data | ✓ | ✓ ⁶ | ✗ | ✓ ¹¹ | – | – | – |
| Automatic refresh controls | ✓ | ✓ | ✓ | – | – | – | – |
| Process-owned background work | – | ○ ⁷ | – | ✓ | – | – | – |
| Item search (`/`) | ✓ | ✓ | ✓ | ✓ | ○ | – | – |
| Recent items (`Opened`) | ✓ | ✗ | ✗ | ✓ | – | – | – |
| Full-text index | ✓ | ✓ | ✗ | ✓ | ○ | – | – ¹ |
| Open indexed result | ✓ | ✓ | – | ✓ | – | – | – |
| Cross-board references | ✓ | ✓ | ○ | ✓ | – | – | – |
| Agent read tool | ✓ | ✓ | ✗ | ✓ | ✓ | – | – ¹ |
| Agent write tool | ✓ | ✓ | ○ | ✓ | ✓ | – | – ¹ |
| Chat context beyond the title | ✓ | ✗ | ✗ | ✓ | ✓ | – | ✓ |
| Shared notifications | ○ | ✗ ⁸ | ○ | ✓ | – | – | ✓ |
| Notification click-through | – | ✗ | – | ✓ | – | – | ○ |
| Quit warnings | – | – | – | – | ○ | – | – |
| Diagnostic config path | – | – | – | ✓ | – | – | ○ |
| Retain when disabled | – | – | – | – | ✓ | – | – |
| Shared state it writes or follows (globals) | ✓ ⁹ | ✓ | ✓ | ✓ | – | – | ✓ ¹² |
| Contracts it publishes or answers ¹⁰ | ○ | – | ○ | ○ | – | – | – |
| Contracts it subscribes to or asks ¹⁰ | ○ | ○ | – | ✓ | – | – | ✓ |

The Issues board (`spaces/issues`) registers its own identity, artwork,
guide, list/detail views, Settings editor, toolbar, status, palette, keys
and diagnostics. Its retained hub owns in-memory data and the existing
100 ms poll; no issue content enters disk caches, search, tools, chat or
notifications. It answers `issues@1/current-scope` and `issues@1/resolve`
and publishes `issues@1/changed`, including while its view is hidden.
The broker holds its responder weakly and stop drops it. See the completed
[board checklist](changes/nett-issue-workspace/tasks.md) and the
[phase 3 integration checklist](changes/board-contracts/delivery.md#phase-3-issues-responder).

The Decisions Space (`spaces/decisions`, board id `decisions`) registers its
identity, artwork (Lucide `scale`), guide, list hooks with the workspace
layout, toolbar, status, busy state, palette, keys and stored choices (view,
maker, project and both docks). It reads `GET /decisions` and
`GET /decisions/{id}` with the shared Subspace API account, in the
background, and keeps decision content in memory only: no agent tools, chat
context, full-text index, notifications, background updates or cache. It
requests `issues@1/current-scope` to open a linked issue: it binds the
issue's address to the answering Space's scope and emits
`DashboardEvent::Navigate` with the `current-scope` responder key, so the
shell calls that Space's `open_link`, as Channels does. It answers and
publishes nothing. See the [developer page](decisions.md) and the completed
[board checklist](changes/decisions-space/tasks.md).

1. Configured boards get no artwork, tools, search, features or background
   work of their own (`BoardRegistration::configured`).
2. Its own config file and `settings_editor` (networks, highlights).
3. Run-time fields from the app manifest (`with_field_source`).
4. Status shows networks and how many are connected; a live connection has no
   reload spinner.
5. Space Lab does not override `keys`, so its help shows the list keys
   (`j`, `k`, Enter, Esc) although it is not a list board.
6. Session logs only (`logs.rs`); the task list reloads on each launch.
7. Copilot Agents monitors tasks and plays alerts from the view's load loop,
   not from `start_background`.
8. Copilot Agents uses the older `alerts` module, so its alerts ignore the
   per-board notification controls and have no click-through.
9. PRs writes favourite repositories with `repositories::apply` and follows
   `Services`; Actions and IRC's hub follow `Services`; Copilot Agents
   follows `ChatSessions`. See [Framework paths](../../subspace-framework/docs/board-integration.md#framework-paths).
10. [Board-to-board contracts](../../subspace-framework/docs/board-integration.md#board-to-board-contracts): IRC and web
    Spaces follow and ask the `issues` contract ([Declared contracts](#declared-contracts)).
    ○ marks the [candidate contracts](#candidate-contracts), in the
    [delivery plan](changes/board-contracts/tasks.md).
11. IRC keeps its messages in the [board store](../../subspace-framework/docs/board-integration.md#board-storage), not a cache
    file ([IRC search and history](changes/irc-search-history/spec.md)).
12. Web Spaces follow the GPUI Kit `Theme` and the Framework's `ThemeId` to
    push `subspace:theme` to app pages ([Follow the app's theme](web-spaces-reference.md#follow-the-apps-theme)).

## Gaps

Concrete follow-ups, most valuable first. Each is a separate change with its
own record.

## Gaps: IRC

Done in [IRC search and history](changes/irc-search-history/spec.md): messages
persist in IRC's board store and restore on start without notifying; joining
asks only for what was missed after the newest stored line; IRC is a Search
source with one local scope and the network as `project`; results open at
their line, asking `CHATHISTORY AROUND` when needed; GitHub links are exact
references; `irc_read search` reads the store; `/` item search, recent items,
palette commands, chat context and the diagnostic config path. Remaining:

1. **Missed-message gaps.** After a long absence, more than 500 missed lines
   leave a gap between the stored lines and the newest page; page forward with
   `CHATHISTORY AFTER` until it closes.
2. **Joins and server lines.** Only messages are stored; consider storing
   membership lines for the restored transcript without indexing them.
3. **Removed networks.** Removing a network from Settings keeps its stored
   messages until retention or Clear; delete them by `project` once a removal
   is confirmed rather than inferred from a config that failed to parse.
4. **Issue references in formatted lines.** Markdown references have tiles.
   Their inline muted and keyboard highlight styles remain in
   board-kit's shared reference plugin work (Kit's `MarkdownPlugin`),
   with channel and member references
   ([Channels issue references](changes/irc-issue-references/tasks.md)).
5. **Open issue reference…** in the palette and More menu (slice 4's
   keyboard fallback) is not built; the transcript's keyboard focus covers
   it.

## Gaps: other boards

- **Copilot Agents**: move task alerts from `alerts` to
  `notifications::request` with `BoardSettings::notifications`, a target per
  task and `open_notification`; run monitoring from `start_background`;
  override `agent_context` with the task detail; emit `Opened`; cache the task
  list for an instant first paint.
- **Actions**: add `actions_read` (runs, jobs, logs, deployments, releases) and
  consider write actions such as re-run and cancel; add palette commands;
  cache the loaded runs; override `agent_context`; consider notifications for
  failed runs on favourite repositories and full-text indexing of run logs.
- **PRs**: consider notifications (review requested, checks failed) and a
  badge.
- **Space Lab**: override `keys` with its own keys; add palette commands;
  consider `quit_warnings` for running project workers and reporting its
  project browser dialogs through `modal`.
- **Board-to-board**: the [delivery plan](changes/board-contracts/tasks.md)
  orders the work: the Issues board and IRC adopt the `issues` contract, then
  IRC to PRs `references`, avatars and the other
  [candidate contracts](#candidate-contracts).
- **Framework**: `docs/logging.md` documents
  `subspace_dashboard::logger::<Self>()`, which does not exist; boards use
  `subspace_core::log::Logger::new`. `docs/search.md` says only GitHub has an
  adapter; Copilot Agents session logs are indexed too.
