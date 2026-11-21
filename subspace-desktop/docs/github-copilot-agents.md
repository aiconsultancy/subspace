# Copilot Agents dashboard

The Copilot Agents dashboard lists the Copilot **cloud agent tasks** of the work
GitHub account and the selected engine's **local chat sessions**, and watches the tasks
the user asked it to watch.

It is read-only. It never starts a task, sends a steering message or stops a
session.

The board prefers the shared GitHub account saved in Settings when it is a
configured `gh` login (case-insensitive match). Otherwise it uses the first
enterprise managed-user (EMU) login ending in `_shortcode`. Credentials are
still verified against the selected login. Settings account/token changes
need a restart. The PRs board passes the same shared preference to its client.

Agent-tool clients built from the tool environment do not receive this saved
preference and still use EMU detection only. Passing the chosen account into
that environment is a separate follow-up.

## What the list holds

| Kind | Where it comes from |
| --- | --- |
| Cloud task | `GET /agents/tasks` for the work account, last 30 days |
| Cloud task | A reference in `copilot-agent-tasks.json`, so a task the chat tool started always has a row and carries its subject |
| Local chat | The Copilot chat pane's saved sessions |

A reference the task list left out shows as `unknown` until its detail is
read (see [Reads](#reads)) or a poll fills it in; the dashboard shows what the file holds and invents
nothing.

Rows group by what the user must do about them — **Needs input**, **Active**,
**Finished** — and sort newest first inside each group. A row has no id column
(`Row::id` is `None`, so the titles line up). `ListDashboard::row_leading`
draws the state's glyph in its tone, one shape per state
(`peek::state_icon`), and `row_trailing` draws fixed-width lanes
(`lanes.rs`): the repository or `Local chat`, the pull request's glyph and
number, a bell when monitored, and the age, right-aligned. Rows are
comfortable and a group's count follows its name, as in the pull request
list. The accessible name reads all of it (`peek::row_label`).

Three filters narrow the list: kind, state and repository. They are stored
under `github-copilot-agents.kind`, `github-copilot-agents.state` and `github-copilot-agents.repo`.
The navigator sets the kind and the repository as well.

## The workspace shell

The Space draws in the Framework's workspace shell
(`subspace_framework::dashboard::workspace`), as every Space does. The
[workspace shell change record](changes/workspace-shell/intent.md) gives the
reasons.

| Zone | What it holds here |
| --- | --- |
| Navigator | Titled **Copilot Agents**. **Views** has a row for each kind: **All tasks and chats**, **Cloud tasks** and **Local chats**. The selected row is the kind filter. **Repositories** has a row for each repository of the loaded rows: a click sets the repository filter and a second click clears it. Each row counts the rows it would show with the other filters as they are. The footer is the account line. |
| Tab strip | The list's tab, named after the view on show, and the opened row's tab beside it, which closes (`opened::OpenItem`). |
| Toolbar | Over the list: the three filter menus and **Clear filters**, in the shared overflow toolbar. Over an opened row: **Back** (`github-copilot-agents-back`) and **Browser (o)**. The two dock switches are the status bar's (`Dashboard::docks`). |
| Content | The shared list, drawn with `list::render_under` under `workspace::pane_head`; or, while the opened row's tab shows, a `workspace::Pane` holding its page (`item.rs`, id `github-copilot-agents-item`). The list's own detail panel never draws. |
| Inspector | A `workspace::Inspector` in the workspace's `.inspector(..)` slot (`inspector.rs`), showing the target: the opened row while its tab shows, else the selection. With nothing selected it reads "Select a task to see it here." |
| Status bar | `Dashboard::status`: the account, then the counts. |

The Space draws no heading, and `Dashboard::render_toolbar` returns nothing,
so the shell's own toolbar no longer carries the filters.

`github-copilot-agents.navigator` and `github-copilot-agents.inspector` record
whether each dock shows. Both show unless the value is `0`
(`shell::stored_docks`).

- The filter menus are always drawn, also while the navigator shows, so that
  keys **1** to **3** have a button to anchor their menus to. With the
  navigator hidden, the **Kind** menu at the start of the toolbar is the view
  menu: unset, it reads **All tasks and chats** rather than **Kind**
  (`ui::unset_label`).
- A click only selects a row (`activates_on_click` is `false`); a group
  header still folds. Enter, a double click, a search result, the row menu's
  **Open** and the inspector's **Open** reach `ListDashboard::on_open`, which
  shuts the shared panel (`ListState::close_detail`) and opens the row as a
  tab (`open_item`). Escape (`CloseDetail`) and **Back** show the list and
  leave the tab open; choosing a view or a repository in the navigator also
  shows the list. Opening another row replaces the tab, and closing it closes
  its session log, so the watcher stops reading that log.
- The inspector switch and the inspector's **Close** only show or hide the
  inspector; they open and close nothing.
- The account line needs the work login. Every list load asks
  `Client::work_login` for it in the same background job, before the list and
  again after it, and keeps the list and its login only when the two agree
  (`load::as_one_account`), so the line never names another account than the
  list's own. A list read while the work account changed is refused, and the
  list on show stays with the account it was read as. Until a load succeeds
  the line reads **Checking account…**, **Account unavailable** after a
  failed load, or **GitHub** alone.

`navigator.rs` holds the pure logic: the views, the repositories, their counts
and the row ids. `peek.rs` holds what the inspector, the lanes and the tab say
of a row: its status words, glyph and tone, its short name, excerpt, facts and
footer actions. `opened.rs` holds which row is open and which tab shows.
`ui.rs` holds the account words, the status line and the dock values. `shell.rs` composes the elements, and `view.rs` gives the Space's root
its key context, so the Space's keys answer wherever in the workspace the
pointer last went.

## The inspector

The header names the row briefly (`peek::name`: `app #7`, **Agent task** or
**Chat session**) and carries **Open** (`github-copilot-agents-open`, left out
beside an opened row), the GitHub link (`browser::link`) and **Close
inspector**. The body shows the status in words beside its glyph, the title,
the first paragraph of the last session's prompt, **Properties** (repository,
branch, model, started, updated, monitor, subject: whichever are known) and the
pull request as one row that runs **OpenPullRequest**. A failed read shows the
shared error banner with Retry. The footer's buttons (`peek::acts`) run the
same actions as **c**, **l**, **m**, **y** and **u**, offered for the same
rows as the palette's commands.

## The opened row

The page shows what the old detail panel showed (`ListDashboard::detail`,
built by `panels.rs`): the title and status, the properties, then the pull
request and one section per agent session: state, model, base and head
branch, times, usage, error, and the prompt as Markdown. That column is capped
at `workspace::READING_WIDTH_PX`; the session log section runs the pane's full
width under it. A local chat shows its title, times and whether a chat tab
holds it.

## Reads

There is one detail slot (`detail`, `detail_key`): the sessions of the target.
`sync_detail` reads them through `cx.background_spawn` when the target is not
already the one held, and only while the inspector shows or an opened row's
tab shows. So a read happens when a row is selected with the inspector shown,
when the inspector is shown with a row selected, when a row is opened whose
sessions are not held, and when switching to the other tab changes the target.
With the inspector hidden, selecting reads nothing. Opening a row the
inspector already read reads nothing more, where the old panel read again. An
answer for a row the user already left is dropped.

When a task's sessions load (a row selected with the inspector shown, or
opened), `logs::load::prefetch_latest` reads the last session's log for the
search index only if session logs are on, the app's full-text search service
exists and this Space's full-text search policy is on. With the policy off,
nothing is read. The read takes the cache file's copy when it holds that
session, and otherwise calls the Copilot API. The log goes only into the
search index: it is not shown, and it is not written to the cache file.

Every log read, shown or for the index, is bound to one work account: the
work login is resolved before the read and again after it, the cache lookup
uses that login, and the log is cached and indexed under that login's search
scope only when the two agree. A log read while the work account changed is
neither cached nor indexed, and shows as **Log unavailable**.

## Open a cloud session in the browser

Select a cloud task and choose the inspector's GitHub link, or press `o`. The
opened row's **Browser (o)**, the palette, row menu, Copy link and chat replay use the same browser destination.
Each loaded session also has its own browser link when its pull request is known.

GitHub sometimes omits `html_url` and returns an empty pull-request node ID.
Subspace resolves a missing repository name and searches that repository's most
recent 100 pull requests, once per repository per load. It matches the artifact's
database ID, not its title or branch. A known pull request uses
`<pull-request-url>/agent-sessions/<session-id>`, or the session overview when no
run ID is available.

If those read-only lookups fail or no pull request is available, the action says
**Open GitHub Agents in browser** and opens GitHub's Agents home page. It cannot focus
a particular session there. Subspace does not invent a task-specific URL.

Failed session loads show a shared error with selectable diagnostics and Retry.

## The session log

**l** opens the target cloud task as a tab and shows the log of its last
session there, and hides it again. The log is shown only when asked for.
When this Space's full-text search policy is on, loading a task's sessions
may already have read its last session's log for the search index (see
[Reads](#reads)); turning **Session logs** off stops both. While the task's
sessions are still loading, a toast asks to try again.

It comes from the undocumented Copilot session API, which the `gh` CLI itself
uses:

| Step | Request |
| --- | --- |
| Find the host | GraphQL `viewer { copilotEndpoints { api } }` as the work login, once per run. No host is hardcoded. |
| Read the log | `GET {api}/agents/sessions/{session id}/logs`, with `Copilot-Integration-Id: copilot-4-cli` and `X-GitHub-Api-Version: 2026-01-09` |

The endpoint accepts the `gh` CLI OAuth token only. With `GH_TOKEN`,
`GITHUB_TOKEN` or a config-file token, no request is sent and the panel says
the log needs the `gh` CLI login.

The answer is an event stream of `data:` lines in three shapes
(`chat.completion.chunk`, `{role: user}` and `{role: tool}`). Each line is a
whole message, not a piece of one: a chunk with tool calls and no text is what
the agent asked for, the same shape with text is that call's answer, and the
answer arrives again as a `{role: tool}` line. The repeats are dropped, so one
call and one answer make one row each. Unknown and broken lines are skipped.
The read stops at 2 MB and says when it was cut. A 404 or a log that parses to
nothing reads as **Log unavailable**, not as an error.

The panel shows what the agent said as Markdown, each tool call as a row with
the tool's name, each tool answer collapsed to one line with its length, and
the entry's `created` time as the stamp the other panels use. A log over 400
entries says how many it left out.

The panel holds one log at a time. Each shown log is cached by the work
login it was read as, session id and last update in `github-copilot-agents-session-log.json` beside the state
file (`prefs::write_cache`), never in `state.json`. The file keeps up to 8
logs and at most 4 MB in all (`MAX_CACHED_LOGS`, `MAX_CACHE_BYTES` in
`logs.rs`), dropping the oldest first but always keeping at least one. While the open task is still running, the watcher's round
reads the log again; nothing is streamed, and a refresh neither reads nor
writes the cache.

The palette's **Session logs** toggle (`github-copilot-agents.logs`, on by default) turns the
feature off.

## Keys

| Key | Action |
| --- | --- |
| j / k, Enter | The shared list keys: move the selection; open the row as a tab |
| Esc | Back to the list from an opened row |
| 1 / 2 / 3 | Filter by kind, state or repository |
| 0 | Clear the filters |
| o | Open the session in the browser, or GitHub Agents if no focused page is available |
| p | Open the pull request on GitHub |
| y | Copy the link |
| m | Monitor the task, or stop monitoring it |
| u | Name the chat or other subject the task came from |
| c | Open the row in a chat tab: a local chat opens its saved session, a cloud task replays its last session read-only |
| l | Show the task's session log, or hide it |

Cross-dashboard navigation has no generic hook, so **u** names the subject
(`chat:abc`) rather than moving to another dashboard.

## Monitoring and alerts

Tasks Subspace starts are monitored. A task found only through the task list is
not; **m** turns monitoring on, and records a reference under the task's
subject, or under `other:<task id>` when it has none, so the choice lasts.

The dashboard owns the watcher. Dashboards are created once and stay alive
while hidden, so the watcher follows monitored tasks whichever dashboard
shows. It polls the monitored running tasks every 30 seconds, backing off to
2 minutes after a failure (`subspace_core::task_state::PollBackoff`), reloads
the whole list on activation and every 5 minutes while anything is running,
and asks about at most 10 tasks per round.

When a monitored task leaves a running state for `waiting_for_user`,
`completed`, `failed`, `timed_out`, `cancelled` or `idle` it raises:

- a toast naming the task, its repository and what happened, linking to the
  pull request when there is one and to the session page otherwise;
- the alert sound, when `alerts.sound` is on;
- a macOS notification, when `alerts.notify` is on **and** the window is not
  active.

A task the watcher has not seen before never alerts, so a first load is
silent. The palette offers **Alert sound**, **Alert notification** and
**Alert sound…** (Glass, Ping, Submarine, Blow, Funk).

## The switcher badge

The tab shows `!` while a monitored task waits for an answer, else the number
of running rows, else nothing.

## The generic shell hooks this needed

The Copilot Agents crate depends on `subspace-dashboard`, `gpui-kit` and the client and
core crates only. Three generic additions carry it, and no per-dashboard code
went into the app shell:

| Hook | Where | What it does |
| --- | --- | --- |
| `Dashboard::badge(&self, cx) -> Option<SharedString>` | `subspace-framework/board-kit/dashboard/src/lib.rs`, `handle.rs` | A mark on the switcher tab. Defaults to `None`; the shell draws it in `app/toolbar.rs`. |
| `DashboardEvent::Chat(ChatRequest)` | `subspace-framework/board-kit/dashboard/src/lib.rs` | `ListSessions`, `OpenSession(id)` and `OpenCloudSession(task)`. The shell opens the chat pane when it is closed and calls the panel's open-session or cloud-replay path. The replay is read-only; see [the chat prototype](../../subspace-framework/docs/agent-chat.md). |
| `chat_sessions::sessions(cx) -> Vec<ChatSession>` | `subspace-framework/board-kit/dashboard/src/chat_sessions.rs` | A GPUI global the host chat panel publishes after every poll: each saved session, with whether a tab holds it and whether that tab is busy. Any dashboard can read it; none can touch the runtime. |

A global was chosen over a `DashboardHandle` hook because the chat panel is a
shared widget rather than a dashboard: a `Global` lets any dashboard observe
the list with `cx.observe_global`, which is how the Copilot Agents list stays current
without polling the panel.
