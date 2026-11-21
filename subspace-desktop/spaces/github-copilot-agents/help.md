# Copilot Agents guide

<!-- Evidence: spaces/github-copilot-agents/src (dashboard, keys, model, filters, filters_ui,
navigator, shell, ui, view, merge, rows, panels, actions, load, watch, logs, palette,
agent_tools), subspace-framework/board-kit/dashboard/src/workspace.rs,
clients/github/src/copilot_agents.rs and copilot_agent_sessions.rs,
subspace-framework/services/core/src/task_state.rs,
subspace-framework/board-kit/dashboard/src/alerts.rs and refresh.rs,
subspace-framework/host/app/src/settings, docs/github-copilot-agents.md. Live GitHub behavior
was not tested while this guide was written. -->

## Overview

The Copilot Agents board lists the GitHub Copilot cloud agent tasks of the work account
and the saved local chat sessions of the selected chat engine, in one list. Rows
are grouped into **Needs input**, **Active** and **Finished**.

The board itself is read-only. It never starts, steers or stops a task. You can
monitor a cloud task so that Subspace alerts you when it finishes or asks for
input. The tools `github_copilot_agents_read` and `github_copilot_agents_write` can list cloud tasks and, after
approval, start new ones.

To open this guide, press `?` or choose **Help**. The Help panel shows this
guide and the keyboard shortcuts. Copilot can read the same guide through
`board_help`.

## Before you start

1. Enable the board. It is off by default. Open **Settings > Boards > Copilot
   Agents**, turn on **Enabled**, and save.
2. Sign in to GitHub with the work account. Subspace uses the first credential
   that it finds: `GH_TOKEN`, then `GITHUB_TOKEN`, then `github.token` in
   `config.json`, then the `gh` CLI login. The board uses the shared GitHub
   account chosen in Settings when it matches a `gh` login, ignoring case.
   Otherwise it uses the first enterprise managed-user (EMU) login ending in
   `_shortcode`. A chosen account need not have that suffix. The credential
   must still belong to the selected account.
3. Choose the favourite repositories: star them with **Repositories…** in
   the PRs board, add them in **Settings > Boards > Copilot Agents**, or set
   `repos` in `config.json`. The agent tools can start tasks only in these
   repositories, and follow changes without a restart. Restart Subspace after
   you change an account or a token.
4. For session logs, sign in as the work account with **Sign in with GitHub**
   in Settings or with `gh auth login`, and unset `GH_TOKEN` and
   `GITHUB_TOKEN`. The log endpoint is undocumented, and it accepts only an
   OAuth sign-in token. An environment or config-file token cannot read it.

Starting a cloud agent task through a tool changes the service. It starts an
autonomous agent that writes code and normally opens a pull request. It keeps
running after the tool call returns. Closing Subspace does not undo it.

Subspace does not check a permission before it sends a request. GitHub accepts
or refuses it.

## Main view

### Layout

The board has the same zones as every other Space.

| Zone | Contents |
| --- | --- |
| Navigator, on the leading side | The board name, the views, the repositories and the GitHub account. |
| Tab strip, at the top of the pane | The list's tab, named after the view on show, and the opened task or chat beside it. |
| Toolbar, under the tab strip | The filters on the leading side, and a switch at each end that shows or hides the navigator and the inspector. Under an opened task, **Back** and **Browser (o)**. |
| List | The rows, in their groups. |
| Inspector, on the trailing side | The selected task or chat, with its actions in the footer. See [Inspector](#inspector). |
| Status bar, at the bottom of the window | The account and the counts. |

### Navigator

- **Views** chooses the kind of row: **All tasks and chats**, **Cloud tasks**
  or **Local chats**. The selected view is the same choice as the **Kind**
  filter, and the tab in the tab strip carries its name.
- **Repositories** lists the repositories of the loaded rows. Click one to
  show only its rows. Click it again to show every repository. This is the
  same choice as the **Repository** filter. The section is left out while no
  row names a repository.
- The number beside a view or a repository is how many rows it would show with
  the other filters as they are. A row with nothing to show is dimmed.
- The footer reads `Signed in to GitHub as <login>` once a list has loaded.
  Before that it reads `Checking the GitHub account…`, and after a failed
  first load `GitHub account unavailable`.
- Choose **Hide navigator** at the leading end of the status bar to hide it,
  and **Show navigator** to show it again. While it is hidden, use the **Kind**
  and **Repository** filters instead. Drag its inner edge to change its
  width.

### Rows

- **Cloud tasks** come from the work account's Copilot agent tasks of the last
  30 days, up to 50, most recently updated first. Subspace also keeps a saved
  reference for each task that it started from chat, and for each task that
  you monitor. Such a task has a row even when the list omits it.
  Its title is `Task <id>` and its state is `unknown` until it is selected
  with the inspector shown, opened, or a poll fills it in.
- **Local chats** are the saved sessions of the chat panel. They show
  `Local chat` where a cloud task shows its repository.
- A row shows a status glyph, the title, then in aligned columns the
  repository or `Local chat`, the pull request's glyph and `#<number>`, a bell
  when monitored, and the age on the right. Each state has a glyph of its own
  shape as well as its colour: a question mark for `waiting_for_user`, a
  spinner for `queued` (a clock) and `in_progress`, a tick for `completed`, a
  cross for `failed` and `timed_out`, a slashed circle for `cancelled`, and a
  dashed circle for `idle` and `unknown`. The pull request's glyph says whether
  it is open, a draft, merged or closed. Finished rows are dimmed. Underscores
  in a state name show as spaces, such as `in progress` and `waiting for user`.
- Click a row to select it and show it in the inspector. Double-click it,
  press Enter, or choose **Open** in the inspector to open it as a tab.

### Groups

| Group | Contents |
| --- | --- |
| **Needs input** | Cloud tasks in state `waiting_for_user`. |
| **Active** | Cloud tasks in state `queued` or `in_progress`, and local chats that are `busy`. |
| **Finished** | `completed`, `failed`, `timed_out`, `cancelled`, `idle`, local chats that are `open` or `saved`, and any state that Subspace does not know, such as `unknown`. |

Groups appear in this order, and an empty group is left out. Inside a group,
rows are sorted newest first. Click a group header, or select it and press
Enter, to fold or unfold it.

### Status line and badge

The status bar starts with the account, `GitHub · Signed in as <login>`, then
reads `<n> rows · <n> active · <n> waiting for input`, with an added
`· <n> shown` while a filter hides rows. Until GitHub has named the account,
the first part reads `GitHub · Checking account…`,
`GitHub · Account unavailable` or `GitHub`. The board badge shows `!` when a
monitored task waits for input, else the number of active rows when it is
greater than 0.

### Load states

- **Cannot load the agent tasks**: the first load failed. Read the message.
- **Showing the last list. Refresh failed:** a later load failed and the board
  keeps the last list.
- **No agent tasks or chat sessions**: the list is empty and no filter is set.
- **Nothing matches the filters**: a filter hides all rows. Press `0`.

### Screenshots

This guide has no screenshot. Cloud tasks need live GitHub data, and the board
has no synthetic fixture mode.

## Toolbar and filters

The toolbar is at the top of the pane, under the tab strip. Its leading side
has three filter buttons and **Clear filters**. **Hide navigator** and **Hide
inspector**, at the two ends of the status bar, hide the navigator and the
inspector. When the pane is narrow, the filter buttons move to an overflow
menu. Under an opened task the toolbar has **Back**, which shows the list
again, and **Browser (o)**.

| Filter | Key | Choices |
| --- | --- | --- |
| **Kind** | `1` | **Any kind**, **Cloud tasks**, **Local chats** |
| **State** | `2` | **Any state**, **Needs input**, **Active**, **Finished** |
| **Repository** | `3` | **Any repository**, and the repositories of the loaded rows |

- A filter button shows its title while unset, and the chosen value while set.
  While the navigator is hidden, the unset **Kind** button reads **All tasks
  and chats**, the view on show.
- Filters combine. The repository match ignores case. A local chat has no
  repository, so a repository filter hides local chats.
- Press `0`, or choose **Clear filters**, to clear all three filters.
- The navigator sets the same **Kind** and **Repository** filters. See
  [Navigator](#navigator).
- Subspace saves the filters as preferences (`github-copilot-agents.kind`, `github-copilot-agents.state`,
  `github-copilot-agents.repo`), and whether the navigator and the inspector
  show (`github-copilot-agents.navigator`,
  `github-copilot-agents.inspector`).
- Press `/` to search. The placeholder is **Search agent tasks and chat
  sessions**. It searches the loaded rows. Selecting a result opens that row
  as a tab.
- Press `r`, or use Reload, to load again. Reload also asks the chat panel to
  list its sessions again.
- Command palette commands (`Cmd+K` on macOS, `Ctrl+K` on Windows, `Ctrl+Shift+K`
  on Linux): **Kind…**, **State…**, **Repository…**, **Clear the filters**, the
  actions for the selected row (**Open in chat**, **Open session in browser (o)**
  or **Open GitHub Agents in browser (o)**, **Open pull request ↗**, **Copy link**,
  **Monitor this task** or **Stop monitoring this task**, **Show the subject**),
  **Show the session log** or **Hide the session log**, **Session logs: on** or
  **off**, **Alert sound: on** or **off**, **Alert notification: on** or **off**
  and **Alert sound…** (Glass, Ping, Submarine, Blow or Funk).

### Refresh

- Opening the board loads the list and asks the chat panel for its sessions.
- With **Background updates** on, Subspace reloads the list when it is older than
  5 minutes (or the **Update interval**) and any row is not finished.
- The global **Load and refresh boards automatically** switch must also be on.
- When a task that you started creates or changes a saved reference, the list
  loads again.

## Details

### Inspector

The inspector, on the trailing side, shows the selected row. One click on a
row selects it; the inspector then shows it without opening it. With nothing
selected it reads `Select a task to see it here.` While an opened task's tab
shows, the inspector shows that task.

- The header names the item briefly: the repository's short name and the pull
  request number, such as `app #7`, **Agent task**, or **Chat session**. Its
  controls are **Open**, which opens the row as a tab, the GitHub link (a cloud
  task's **Open session in browser (o)** or **Open GitHub Agents in browser
  (o)**), and **Close inspector**.
- The body shows the state in words beside its glyph, the title, and the start
  of the last session's prompt once the sessions have loaded. **Properties**
  lists **Repository**, **Branch** (head, then the branch it merges into),
  **Model**, **Started**, **Updated**, **Monitor** (`on` or `off`) and
  **Subject**, each when it is known. **Pull request** is one row, such as
  `#7 open`, that opens the pull request on GitHub (`p`).
- **Cannot load sessions** shows as a banner with **Retry** when the sessions
  fail to load.
- The footer has the row's actions, the same as their keys: **Open in chat**
  (`c`); for a cloud task also **Session log** or **Hide log** (`l`),
  **Monitor** or **Stop monitoring** (`m`), **Copy link** (`y`), and **Show
  subject** (`u`) when a chat started the task.

Choose **Hide inspector** at the trailing end of the status bar, or **Close
inspector**, to hide it and give the list the whole pane. The row actions
still act on the selected row. **Show inspector** shows it again.

A cloud task's sessions are read when it is selected while the inspector
shows, when the inspector is shown with a task selected, and when a task is
opened whose sessions are not already loaded. With the inspector hidden,
selecting reads nothing.

### Opened task

Double-click a row, press Enter on it, or choose **Open** in the inspector to
open it as a tab beside the list's tab. Opening another row replaces it.
Choose the list's tab or **Back**, or press Escape, to show the list again;
the tab stays open. Close the tab with the X on it. Choosing a view or a
repository in the navigator also shows the list.

The page shows the title and state, then
**Properties** and the sections below, in a column no wider than easy reading.
The session log runs the pane's full width under it.

#### Cloud task

| Property | Content |
| --- | --- |
| **State** | The task state. |
| **Repository** | `owner/name`. |
| **Created**, **Updated** | Times. |
| **Monitor** | `on` or `off`. |
| **Subject** | The source of the task, such as `chat:<id>`. Shown when known. |

Sections:

- **Pull request**: `#<number> (<state>)` as a link, when the task has one.
- `Session <n>: <name>`: one section for each agent session, with state,
  model, an **Open this session in browser** link when it is known, base and head
  branch, start, update and finish times, usage, error message and the prompt.
- **Sessions**: `Loading the task…`, or `This task comes from a saved
  reference. Its state loads when it is selected or opened.`
- **Session log**: see [Read a session log](#read-a-session-log).
- **Cannot load sessions**: a banner with the error text and **Retry**. The error
  text can be selected and copied.

#### Local chat

The page shows **Chat session**, **State** (`busy`, `open` or `saved`),
**Created** and **Updated**, and the section **Chat**: `Open in chat (c) shows
this session in a chat tab.`

### Right-click menu

Right-click a row for **Open**, which opens it as a tab, **Copy title** and
**Add to chat**. A cloud task adds **Open in GitHub** after **Open** and **Copy link**
before **Copy title**.

## Actions

### Open a task in the browser

Select a cloud task and choose **Browser (o)**, or press `o`. The same
destination is used by the inspector's link, the palette, the right-click menu, **Copy
link** and the chat replay.

- A loaded session links to its own pull-request session page.
- If GitHub does not supply enough metadata, the action reads **Open GitHub Agents in
  browser** and opens the GitHub Agents home page. That page cannot focus a
  particular session. Subspace does not invent a task-specific address.
- With no pull request or page, a toast reads `This row has no session page on
  GitHub`.

Press `p` to open the task's pull request. Without one, a toast reads `This task
has no pull request`. Press `y` to copy the browser link. A toast reads `Copied
the browser link`, or `This row has no link to copy`.

### Monitor a cloud task

1. Select a cloud task.
2. Press `m`.

A toast reads `Monitoring <title>: it will alert when it finishes or asks for an answer`.
Press `m` again to stop. The toast reads `No longer monitoring <title>`. Subspace
records the choice in `copilot-agent-tasks.json`, so it lasts after a restart. If it
cannot write the file, a toast reads `Cannot record the choice: <error>`. For a
local chat, a toast reads `Select a cloud task first`.

Tasks that you start through `github_copilot_agents_write` are monitored from the start. A task
found only through the task list is not monitored until you press `m`.

### Alerts

The watcher runs while the app runs, also when another board is shown.

- It polls only monitored cloud tasks that are still running (`queued`,
  `in_progress` or `waiting_for_user`), at most 10 for each round.
- A poll runs every 30 seconds, or at the **Update interval**. After a failure
  the delay doubles, from 30 to 60 to at most 120 seconds. A success returns it
  to normal.
- When a monitored task leaves a running state for `waiting_for_user`,
  `completed`, `failed`, `timed_out`, `cancelled` or `idle`, Subspace shows a
  toast such as `<title> (<repository>) completed`, with a link to the pull
  request, or to the session page. It also plays the alert sound when it is on,
  and sends a notification when it is on and the window is not active.
- A task that the watcher has not seen before never alerts, so the first load is
  silent.
- Change **Alert sound**, **Alert notification** and the sound name in the
  command palette. The sound is on by default, the notification is on by
  default and the default sound is Glass.

With **Background updates** off, the watcher does not poll and no alert is
raised.

### Name the source of a task

Press `u`. A toast reads `Started from <kind>:<id>`, or `This task has no chat
behind it`. Subspace names the source and does not move to it.

### Open in chat

Press `c`. A local chat opens its saved session in a chat tab. A cloud task
replays its last session, read-only, in a chat tab. With no suitable row, a toast
reads `Select a chat session or a cloud task first`.

### Read a session log

1. Select or open a cloud task.
2. Press `l`, or choose **Session log** in the inspector. The task opens as a
   tab and the log shows there. Press `l` again to hide the log.

If the task's sessions are still loading, a toast reads `The task is still
loading. Show the log again in a moment`.

Subspace reads the log of the task's last session, only when you ask for it. It
shows **Loading the session log…**, then the log as Markdown.

- The log is limited to 2 MB. A cut log ends with `The log was cut at 2 MB; this
  is its start only.`
- The page shows at most 400 entries and says how many it leaves out. Each text
  entry is cut at 4,000 characters. A tool call and its answer are collapsed to
  one line, the answer with its length.
- A failed read shows **Log unavailable.** An empty log reads `This session
  logged nothing.`
- If you turned session logs off, a toast reads `Session logs are turned off`. If
  the task has no session, it reads `This task has no session to read a log for`.
  With no open cloud task, it reads `Open a cloud task first`.
- Turn session logs on or off with **Session logs** in the command palette. They
  are on by default.
- One log is open at a time. Subspace keeps up to 8 logs, at most 4 MB in total,
  in `github-copilot-agents-session-log.json`. While the open task still runs, each watcher
  round reads the log again.

### Agent tools

Copilot chat and the MCP server register `github_copilot_agents_read` and `github_copilot_agents_write`.
Results are untrusted service data.

`github_copilot_agents_read` starts with `scope`.

- `scope` returns the configured repositories, the kinds, the states and the
  limit.
- `tasks` lists recent cloud tasks. Filters: `kind` (`cloud` or `local`), `state`
  (`needs-input`, `active`, `finished`), `repo` and `limit` (1 to 50, default
  50). `kind=local` returns no rows, because local chats exist only in the
  interface.
- `task` shows one task by `id`, with its sessions and its pull request.
- `refresh` fetches the list, or one task, again.

Reads do not start or change tasks.

`github_copilot_agents_write` has one action, `github_agent_task`. It takes a batch of 1 to 20
tasks. It starts an autonomous GitHub Copilot cloud agent in a configured
repository. The task writes code and normally opens a pull request. It continues
after the tool call returns.

- `repo` (required) must be in `repos`. `prompt` (required) is the full
  instruction, up to 64 KB.
- `base_ref` defaults to the repository's default branch. Subspace resolves the
  base branch before approval.
- `model` is optional.
- `create_pull_request` defaults to `true`. With `false`, the task pushes a
  branch without a pull request.
- Subspace records each started task with monitoring on. If it cannot save the
  reference, the result carries a warning. Keep the GitHub link and do not start
  the task again.

Starting a task is not a local edit and cannot be undone by closing Subspace.
Subspace chat shows an approval before a task starts. Over MCP, the MCP client is
responsible for approval. Never repeat a start after an ambiguous result. Read
the Copilot Agents board and keep any returned GitHub link instead.

## Keyboard shortcuts

Board keys act only while no text field, menu or dialog has focus. In a text
field, the same letters are ordinary text.

| Key | Action |
| --- | --- |
| `j` or Down, `k` or Up | Next or previous row. |
| Home, End | First or last row. |
| Enter | Open the task or chat as a tab. On a group, fold or unfold it. |
| Escape | Back to the list from an opened task. |
| `1` `2` `3` | Open the Kind, State or Repository filter menu. |
| `0` | Clear the filters. |
| `o` | Open the session in the browser, or the GitHub Agents page. |
| `p` | Open the pull request. |
| `y` | Copy the browser link. |
| `m` | Monitor the task, or stop monitoring it. |
| `u` | Name the chat that started the task. |
| `c` | Open the chat session in a chat tab. |
| `l` | Show or hide the session log. |
| `/` | Search the board. |
| `r` | Reload. |
| `w` | Wrap long titles. |
| `?` | Show or hide Help. |

In a menu: Up and Down, `Ctrl+N` and `Ctrl+P` move, Enter picks, `1` to `9` pick
the numbered option, and Tab or Escape closes.

## Settings

Open **Settings > Boards > Copilot Agents**.

| Setting | Default | Notes |
| --- | --- | --- |
| **Enabled** | Off | Save applies the board choice without a restart. |
| **Background updates** | On | Save applies without a restart. Off pauses automatic reads, including notification and task polling. Manual reload and opening the board still load data. |
| **Update interval** | **Default** | **Default**, 30 seconds, 1, 2, 5, 10, 15 and 30 minutes, or 1 hour. **Default** keeps the 30 second poll and the 5 minute list reload. |
| **Account** | Not set | Choose a `gh` account (**Choose a gh account**), or see **No gh accounts found**. Restart Subspace to apply the account to other GitHub boards and Copilot chat. |
| **GitHub token** | Not set | The input never shows a saved token. **Clear saved token** removes it on Save. `GH_TOKEN` and `GITHUB_TOKEN` take priority. |
| **Favourite repositories** | Empty | Shared with PRs and Actions. **Remove** and **Add repository…**; Save applies without a restart. |

The Settings page notes: **Configure task filters, alerts and agent behavior in
the Copilot Agents dashboard.** Filters, alert sound, alert notification, alert sound name
and session logs are not Settings fields. Use the filter buttons, the navigator
and the command palette.

**Settings > Application > Notifications > Play notification sounds** mutes
this Space's task sounds as well as Copilot chat and every other Space on
Save. Its sound name and alert choices are kept while muted.

Background updates also need the global **Load and refresh boards
automatically** switch in **Settings > General**. Tokens are stored in the private
config file, not in the Keychain.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| The board tab is missing. | Turn on **Enabled** in **Settings > Boards > Copilot Agents** and save. The board is off by default. |
| **Cannot load the agent tasks** | Read the message. Choose **Sign in with GitHub** in Settings, run `gh auth login`, or set `GH_TOKEN`. Press `r`. |
| **Showing the last list. Refresh failed:** | The last list is kept. Read the error and press `r`. |
| A task row has the title `Task <id>` and state `unknown`. | The row comes from a saved reference. Select it with the inspector shown, or open it, to load it. |
| A click on a row does not open the task. | A click selects. Double-click the row, press Enter, or choose **Open** in the inspector. |
| The views and repositories are missing. | The navigator is hidden. Choose **Show navigator** at the leading end of the status bar. |
| **Cannot load sessions** | Read the error and choose **Retry**. |
| `Open GitHub Agents in browser` | GitHub did not supply a task page or a pull request. The GitHub Agents home page opens instead. |
| **Log unavailable.** | The read failed, or the answer held no log. The log needs an OAuth sign-in: sign in as the work account with **Sign in with GitHub** or `gh auth login`, and unset `GH_TOKEN` and `GITHUB_TOKEN`. The endpoint is undocumented. |
| No alert for a task. | Press `m` to monitor it. Check that **Background updates** and the global automatic refresh are on. A task seen for the first time does not alert. |
| An agent tool refuses a repository. | Star it with **Repositories…** in the PRs board, or add it in Settings and save. |
| A task start returned an unclear result. | Do not start it again. Read this board and keep any returned GitHub link. |

A successful local check does not prove that GitHub will accept a request.

## Limits

- Live GitHub behavior, task starts, the session log, alerts, notifications and
  Settings saves were not tested while this guide was written. The statements
  above come from the board source and its tests.
- The list holds at most 50 cloud tasks from the last 30 days, plus saved
  references. The tools return at most 50 tasks.
- Polling covers at most 10 monitored running tasks in each round.
- The board never starts, steers or stops a task. Only `github_copilot_agents_write` starts a
  task, after approval.
- `kind=local` in `github_copilot_agents_read` returns no rows. Local chats are not available to
  tools.
- The log endpoint is undocumented, needs the `gh` CLI login, and is limited as
  described in [Read a session log](#read-a-session-log).
- The board does not move to the board of a task source. It names the source.
- Notifications depend on the platform. They were not tested.
- This guide has no screenshot because the board needs live GitHub data.
