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

A reference the task list left out shows as `unknown` until its panel opens or
a poll fills it in; the dashboard shows what the file holds and invents
nothing.

Rows group by what the user must do about them — **Needs input**, **Active**,
**Finished** — and sort newest first inside each group. A row shows its kind
mark, its repository (local chats show `local`), its title, its state, its
pull request, a watched mark and how long ago it changed.

Three filters narrow the list: kind, state and repository. They are stored
under `github-copilot-agents.kind`, `github-copilot-agents.state` and `github-copilot-agents.repo`.

## The detail panel

A cloud task shows its state, repository, times, monitor flag, subject and
pull request, then one section per agent session: state, model, base and head
branch, times, usage, error, and the prompt as Markdown. The detail loads on
selection through `cx.background_spawn`; an answer for a row the user already
left is dropped.

A local chat shows its title, times and whether a chat tab holds it.

## Open a cloud session in the browser

Select a cloud task and click **Browser (o)**, or press `o`. The panel header,
palette, row menu, Copy link and chat replay use the same browser destination.
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

**l** shows the log of the open task's last session, and hides it again. The
log is read only when asked for; selecting a row never reads one.

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

One log is held at a time. It is cached by session id and last update in
`github-copilot-agents-session-log.json` beside the state file (`prefs::write_cache`), never
in `state.json`. While the open task is still running, the watcher's round
reads the log again; nothing is streamed, and a refresh neither reads nor
writes the cache.

The palette's **Session logs** toggle (`github-copilot-agents.logs`, on by default) turns the
feature off.

## Keys

| Key | Action |
| --- | --- |
| j / k, Enter, Esc | The shared list keys |
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
