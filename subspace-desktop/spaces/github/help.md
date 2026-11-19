# PRs guide

<!-- Evidence: spaces/github/src (lib.rs, model, rows, detail, diff, agent_tools,
search_index, store), clients/github/src (lib.rs, model, queries, validate),
subspace-framework/host/app/src/settings. The board ID is `github`; its tab title is
PRs. Live GitHub behavior was not tested while this guide was written. -->

## Overview

The PRs board (board ID `github`, also called the GitHub board) lists the open
pull requests that involve your GitHub account, and every open pull request in
your favourite repositories. It groups them by repository in four sections:
**Mine**, **Review requested**, **Involved** and **All open**. You can read
details, checks, reviewers and comments, view the changed files, and comment,
review, close a pull request or change assignees without leaving Subspace.

The board shows pull requests only. Issues do not appear here.

To open this guide, press `?` or choose **Help**. The Help panel shows this
guide and the keyboard shortcuts. Copilot can read the same guide through
`board_help`.

## Before you start

1. Sign in to GitHub: in **Settings > Spaces > PRs**, choose **Sign in with
   GitHub** and approve Subspace on GitHub. Subspace uses the first credential
   that it finds: `GH_TOKEN`, then `GITHUB_TOKEN`, then `github.token` in
   `config.json`, then the chosen account's sign-in, then its `gh` CLI login.
   You can also run `gh auth login` instead.
2. Restart Subspace after you change a token or an account.
3. Optional: star favourite repositories with **Repositories…** in the
   toolbar (see [Favourite repositories](#favourite-repositories)), in
   **Settings > Boards > PRs**, or in `repos` in `config.json`. **All open**
   lists their open pull requests, and they limit the agent tools. The other
   sections search everything the account can see unless **Favourites only**
   is on.

Your account needs read access to the pull requests. Commenting, reviewing and
assigning need the matching rights on the repository. Subspace does not check a
permission before it sends a request. GitHub accepts or refuses it. The board
hides **Approve** and **Request changes** when the loaded details say that you
cannot review, and hides **Comment** when you cannot comment.

## Main view

### Sections

**Pull requests** heads the list and shows the current GitHub account when
one is available. The toolbar beneath it has one tab for each section, with a
count. Select a tab, or press
`1` to `4`. The first launch opens **Review requested**. Subspace remembers
the last section.

| Key | Section | GitHub search |
| --- | --- | --- |
| `1` | **Mine** | `is:pr is:open author:@me archived:false` |
| `2` | **Review requested** | `is:pr is:open review-requested:@me archived:false` |
| `3` | **Involved** | `is:pr is:open involves:@me -author:@me archived:false` |
| `4` | **All open** | `is:pr is:open archived:false repo:<owner/name> …`, one `repo:` for each favourite repository |

**All open** is empty until a repository is a favourite; its empty state reads
**No favourite repositories** and offers **Add repositories…**, which opens
the Repositories search. With many repositories, Subspace splits the `repo:`
qualifiers over several searches so each stays within GitHub's 256-character
query limit, then merges them: the newest 50 pull requests show, and the count
adds up every search. With **Favourites only** on, Mine, Review requested and
Involved add the same `repo:` qualifiers to their searches.

### Favourite repositories

The favourites are the `repos` list that Actions, Copilot Agents and the agent
tools share. Manage them here or in Settings:

- **Repositories…** (star icon, `Shift+F`) opens a search. With nothing typed
  it lists your favourites, then **Your repositories**: the repositories you
  own or belong to, most recently pushed first. Typing searches GitHub
  (**GitHub results**). A filled star marks a favourite.
- Pick a repository (click, or the arrows and Enter) to star or unstar it. The
  search stays open. Each change is saved at once to the config file, and
  All open, Actions, Copilot Agents and the agent tools follow it without a
  restart.
- **Favourites only** at the top of the search limits **Mine**, **Review
  requested** and **Involved** to the favourites. Subspace remembers it. The
  control looks pressed while it is on.
- A favourite repository's header shows a filled star. Right-click a header
  or a pull request for **Add `owner/name` to favourites** or **Remove
  `owner/name` from favourites**, or select it and press `f`.
- In Settings, edits wait for **Save**; see [Settings](#settings).

Each section loads at most 50 pull requests, most recently updated first. The
status line shows `<n> open PRs · <n> repos`, or `<shown> of <total> open PRs`
when GitHub has more than 50. A tab count shows the total. A red count means the
last load of that section failed. A yellow count means it is loading.

### Groups and rows

- Pull requests are grouped by repository, with a count. The icon of a
  repository that belongs to your own account is muted.
- Each row shows a state mark, `#<number>`, the title, a review mark, a CI mark,
  `+<additions>` and `−<deletions>`, the author's initials and the age.
- State marks: open (green), draft (muted, and the row is dimmed), merged
  (magenta) and closed (red).
- Review marks: `✓` approved, `±` changes requested, `•` review required.
- CI marks: `✓` success, `✕` failure, `◌` pending.
- Click a repository header, or select it and press Enter, to fold or unfold
  it. Subspace remembers folded repositories.
- An empty section shows a message, such as **Nothing waiting for your
  review**.

### Saved results and load states

Subspace shows the pull requests that it saved on the last run first, then
refreshes them from GitHub. It keeps the saved lists in `github-cache.json`.

- **Cannot load GitHub**: Subspace could not read the account. A message such as
  `No GitHub accounts found. Sign in with GitHub in Settings, …` explains why.
- **Showing saved results. Refresh failed:** the section kept its saved list.
- **Cannot load this section**: no saved list exists and the load failed.

### Screenshots

This guide has no screenshot. The board needs live GitHub data, and the board
has no synthetic fixture mode.

## Toolbar and filters

| Control | Behavior |
| --- | --- |
| Section tabs | **Mine**, **Review requested**, **Involved**, **All open**, as a segmented control. Same as `1` to `4`. |
| **Repositories** (star icon) | Opens the repository search with stars and **Favourites only**. Same as `Shift+F`. Shown pressed while Favourites only is on. |
| Account (user icon and short login), last | When more than one account is available, opens the account menu. Same as `u`. |

When the window is narrow, the controls move to an overflow menu from the
end, as `Go to <section> · <n> items`, `Repositories…` and
`Account: <login>…` when there is an account picker.

- **Account**: `u` opens the menu **Account** with the placeholder **Switch
  account**. If only one account is signed in, a toast reads `Only one GitHub
  account is signed in.` The accounts are the `gh` logins, or the login of the
  token. Each account keeps its own lists and details.
- The board has no text filter. Use `/` to search the loaded pull requests. The
  placeholder reads **Search pull requests and issues**. It searches the
  sections that are loaded for the account. A result in another section switches
  to that section.
- Press `r`, or use Reload, to load all sections again. Reload does nothing for
  sections that are already loading. It also clears loaded details and reloads
  the open item, or the open diff.
- Command palette commands: **Switch account…** (with two or more accounts),
  **Go to &lt;section&gt;**, **Assign…**, **Assign to me** or **Unassign me**,
  **Files: show the diff**, **Open in GitHub**, **Copy URL**, and when allowed
  **Approve**, **Request changes** and **Comment**. The palette key is `Cmd+K`
  on macOS, `Ctrl+K` on Windows and `Ctrl+Shift+K` on Linux.

### Refresh

- Opening the board loads the saved lists, then loads all three sections.
- When you open the board again after more than 5 minutes, it loads all
  sections again.
- With **Background updates** and **Load and refresh boards automatically**
  on, Subspace loads PRs at startup and refreshes them in the background about
  every 10 minutes, on the same default cadence as Actions. You can set a
  different **Update interval** for PRs. These reads use the saved GitHub
  credentials.
- If local search caching is enabled for GitHub in the Search view, Subspace
  prefetches the details and diffs of loaded pull requests. Unchanged retained
  details and diffs can be reused for up to 15 minutes without changing their
  original fetch time. Opening a detail or diff still uses the normal read path.

## Details

Press Enter on a pull request, or click it, to open the detail panel. Press
Escape to close it. While the panel is open, `j` and `k` step to the next and
previous item. A short delay avoids a request for each key press. Drag the left
edge of the panel to change its width. Tab to the divider and use the arrow
keys to adjust the width from the keyboard. Double-click the divider to reset.

The title bar shows **Pull request**, `<repository name>#<number>`, previous and
next buttons, a **GitHub ↗** link and a close button.

### Opening from another board

A board that supports opening GitHub pull requests in Subspace can open this
same detail panel. Subspace switches to PRs and uses the account currently
selected here. The pull request can be outside the current section or your
favourite repositories, including a closed or merged pull request.

If the pull request is not loaded, the panel shows **Loading…**, then its
details or the read error. Your account still needs repository access. This
does not add the pull request to a section's saved search results. Escape
closes the panel as usual and removes the temporary row. Switching sections
or selecting another row also removes it. In the Issues board's Pull requests
panel, each repository-and-title row has an **Open in PRs** tooltip and opens
this panel. Its **Open on GitHub** link opens the external page.

Channels shows pull request chips with a hover card. PRs supplies the card
with the account currently selected here, from what it already loaded or
with one read in the background, and only for pull requests that account can
read. It keeps those cards in memory and forgets them when you switch
account, sign in or out, or refresh. After you sign an account out, its cards
stay unavailable until that account signs in again, even if you choose it
again here. While a refresh runs, the cards wait for it instead of showing
the lists it replaces. With PRs off, the chips stay but show no details.

### Properties

| Property | Content |
| --- | --- |
| **State** | Open, Draft, Merged or Closed. |
| **Author** | The author's name or login. |
| **Files** | `<n> files · +<additions> −<deletions> (d)`, or `Show diff` before details load. |
| **Branch** | `<head> → <base>`. |
| **Assignees** | Up to four names then `…`, with the key hint `(a)`, or `Nobody (a)`. |
| **Labels** | Label names, when the pull request has labels. |
| **Updated** | `<age> ago · <full date>`. |

### Sections

- **Reviewers**: each reviewer with a state of approved, requested changes,
  reviewed, review requested or dismissed, or **None**.
- **Checks (&lt;n&gt;)**: each check with a result of passed, failed, pending,
  neutral, skipped or cancelled, and a `↗` link when it has one. With more than
  5 checks, the list shows only failing checks until you choose the summary
  line `<summary> · show all`. Choose `show failing` to collapse it again.
- **Description**: the pull request text, **Loading…**, or **No description.**
- **Activity (&lt;n&gt;)**: the latest comments and reviews, with author, review
  state, age and text. The age links to the item on GitHub.
- The action bar appears above the checks and again under the activity.

If the details fail to load, the panel shows the error text.

### Right-click menu

Right-click a row for **Add to chat**, **Open details**, **Open in GitHub**,
**Copy link**, **Copy title** and **Copy id**.

### Diff view

Press `d`, choose **Files d** in the panel, or choose **Files: show the diff**.
The diff view replaces the list.

- The title bar has **Back**, the `owner/name#<number>` reference, the title,
  `<head> → <base>`, the `+` and `−` totals, the **Unified** and **Split**
  choice, and **GitHub**, which opens the files tab. Subspace remembers the
  layout. The first launch uses **Unified**.
- The file tree on the left has the header **Files** with `<n> / <total>
  viewed`. Each file has a status letter: `A` added, `D` deleted, `R` renamed,
  `M` modified. Viewed files are dimmed. GitHub lists at most 3000 files. The
  tree then shows `GitHub lists the first <n> of <total> files.`
- Press `/` to search file names. The box has the placeholder **Search files**.
  Enter picks the first match, and Escape closes the box.
- Hunks are separated by rows that read `⋯ expand`. Choose one to load the
  file at the pull request's head (or at the base for a removed file). The file
  read is limited to 1 MB.
- A file without a patch shows `Renamed without changes`, `Binary file, or no
  text changes`, or `Diff too large to show here` with **View on GitHub ↗**.
- An image file shows **Image file** and **View image**. The viewer shows
  **Before** and **After**. Image files are limited to 10 MB.
- Viewed marks are stored for the repository, the pull request number and the
  head commit. A new commit starts with no viewed files.

## Actions

Every write goes to GitHub at once after you confirm. Only one write runs at a
time.

### Review a pull request

1. Select a pull request, or open its diff.
2. Press `Shift+A` for **Approve**, or `Shift+R` for **Request changes**, or
   choose the button in the panel.
3. Enter the text. **Approve** has the placeholder **Optional comment**.
   **Request changes** needs text (`What needs to change?`), or the composer
   shows `Text is required.`
4. Choose the submit button, or press `Cmd/Ctrl+Enter`. Escape cancels.

A toast reads `Approved #<n>` or `Requested changes on #<n>`. An error stays in
the composer. If you cannot review, a toast reads `You cannot review this pull
request`.

### Comment

1. Press `c`, or choose **Comment c**. The composer has the placeholder **Leave
   a comment**. The text is required.
2. Submit with `Cmd/Ctrl+Enter`.

A toast reads `Commented on #<n>`. When two modes are available, a switch in the
composer changes between **Approve**, **Request changes** and **Comment**. If
you cannot comment, a toast reads `You cannot comment here`.

### Close a pull request

1. Select an open pull request (including a draft), then press `x` or choose
   **Close… x** in the panel or **Close pull request…** in Commands.
2. The dialog asks `Close pull request <repository>#<number>?`. Add an
   optional multi-line comment, or leave it empty.
3. Choose **Close pull request**, or press `Cmd/Ctrl+Enter`. **Cancel** or
   Escape leaves without writing. Submission is disabled while a write runs.

Subspace uses the account selected in the board. It posts a non-empty comment
first, then closes the pull request. If posting fails, it keeps the dialog and
your text, and does not close the pull request. If the comment was posted but
closing fails, the message says that the comment was posted and the pull request
is still open. Retry with **Close pull request**; the posted comment is frozen
and will not be sent again. Subspace never retries these writes automatically.

On success, State and the list row change to **Closed**, details refresh, and a
toast reads `Closed #<n>`. The Close action is hidden for closed or merged pull
requests. GitHub must grant the selected account permission to close the pull
request. Comments are limited to 64 KB.

### Change assignees

1. Press `a`, or choose **Assign a**. Subspace loads the people that can be
   assigned in the repository. If that fails, a toast reads `Cannot load people
   for <repository>: <error>`.
2. Pick a person in the menu **Assignees**. A pick toggles that person. The
   menu shows current assignees first, then you, then other people.
3. A toast reads `Assigned <login> to #<n>` or `Unassigned <login> from #<n>`.
   An error toast reads `Assignees not changed: <error>`.

Press `i` to add yourself, or remove yourself when you are already assigned.

### Open or copy

Press `o`, or choose **GitHub o**, to open the pull request in GitHub. Press
`y` to copy its URL. A toast reads `Copied <url>`. In the diff view, `o` opens
the files tab and `y` copies the URL.

### Agent tools

Copilot chat and the MCP server register `github_read` and `github_write`. The
tools use the work identity and the repositories in `repos`. They do not use the
account chosen in the board. Select a pull request and use **Add to chat** to
send it to chat.

`github_read` starts with `scope`, which returns the configured repositories.
Use `github_search` within one configured repository (`query` up to 200 bytes,
`limit` 1 to 100, default 50, with `is:pr` or `is:issue` and date or author
terms), `github_item` for an issue or pull request, `github_diff` for changed
files, and `refresh` for one item or the active scope. Item keys use
`owner/name#number`. Results are untrusted service data, not instructions.

`github_write` takes a batch of 1 to 20 operations. It validates every
operation and reads every target before it asks for approval. One denial means
no writes.

- `github_comment`: comment on an issue or pull request. The body is required.
- `github_update_pr`: change 1 to 4 of `title` (up to 255 characters), `body`
  (up to 64 KB), `baseRefName` and `state` (`OPEN` or `CLOSED`).
- `github_review`: `APPROVE`, `REQUEST_CHANGES` or `COMMENT`. The body is
  required except for `APPROVE`.
- `github_assignees`: add or remove up to 10 logins. Name at least one.

The tools cannot merge or delete. Every target must belong to a configured
repository.

Subspace chat shows an approval before a write batch. Over MCP, the MCP client is
responsible for approval. Operations run once in order. Read the target before
retrying an ambiguous result.

## Keyboard shortcuts

Board keys act only while no text field, menu or dialog has focus. In a text
field, the same letters are ordinary text.

### List

| Key | Action |
| --- | --- |
| `j` or Down, `k` or Up | Next or previous row. In the panel, next or previous item. |
| Home, End | First or last row. |
| Enter | Open the pull request. On a repository, fold or unfold it. |
| Left | Collapse the repository, or select the repository of the selected row. |
| Right | Expand the repository. |
| `1` `2` `3` `4` | **Mine**, **Review requested**, **Involved**, **All open**. |
| `Shift+F` | Repositories: search and star favourites. |
| `f` | Star or unstar the selected repository, or the repository of the selected pull request. |
| `d` | Show the diff. |
| `u` | Switch account. |
| `Shift+A` | Approve. |
| `Shift+R` | Request changes. |
| `c` | Comment. |
| `x` | Close the open pull request after confirmation. |
| `a` | Change assignees. |
| `i` | Assign to me, or unassign me. |
| `o` | Open in GitHub. |
| `y` | Copy the URL. |
| Escape | Close the panel. |
| `/` | Search pull requests. |
| `r` | Reload. |
| `w` | Wrap long titles. |
| `?` | Show or hide Help. |

### Diff view

| Key | Action |
| --- | --- |
| `n` or `p` | Next or previous file. An image file opens as images. |
| `j` or Down, `k` or Up | Scroll down or up. |
| Home, End | First or last file. |
| `v` | Mark the file as viewed and go to the next, or unmark it. |
| `s` | Switch between split and unified layout. |
| `Shift+A`, `Shift+R`, `c` | Approve, request changes, comment. |
| `x` | Close the open pull request after confirmation. |
| `o` | Open the files on GitHub. |
| `y` | Copy the URL. |
| `/` | Search file names. |
| Escape | Close the composer or the image, then return to the list. |

In a menu: Up and Down, `Ctrl+N` and `Ctrl+P` move, Enter picks, `1` to `9` pick
the numbered option, and Tab or Escape closes. Section keys, account, assignee
and `i` do nothing while the diff view is open.

## Settings

Open **Settings > Boards > PRs**.

| Setting | Default | Notes |
| --- | --- | --- |
| **Enabled** | On | Save applies the board choice without a restart. |
| **Background updates** | On | Save applies without a restart. Off pauses automatic reads. Manual reload and opening the board still load data. |
| **Update interval** | **Default** | **Default**, 30 seconds, 1, 2, 5, 10, 15 and 30 minutes, or 1 hour. **Default** keeps the 5 minute refresh age. |
| **Account** | Not set | Choose a `gh` account (**Choose a gh account**), or see **No gh accounts found**. Restart Subspace to apply the account to other GitHub boards and Copilot chat. |
| **GitHub token** | Not set | The input never shows a saved token. **Clear saved token** removes it on Save. `GH_TOKEN` and `GITHUB_TOKEN` take priority. |
| **Favourite repositories** | Empty | The repositories of **All open**. Shared with Actions, Copilot Agents and the agent tools. **Remove** takes one out; **Add repository…** opens the same search as **Repositories…**. Save applies the list without a restart. |

Background updates also need the global **Load and refresh boards
automatically** switch in **Settings > General**. Tokens are stored in the
private config file, not in the Keychain. The section, folded repositories,
diff layout and viewed files are preferences that the board saves when you
change them. They are not Settings fields.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| **Cannot load GitHub** | Choose **Sign in with GitHub** in Settings, run `gh auth login`, or set `GH_TOKEN`. Press `r`; restart Subspace after a token change. |
| **Showing saved results. Refresh failed:** | Read the error. Press `r` to try again. Check the token and network. |
| **Cannot load this section** | The first load of the section failed. Read the error. Check the token scopes and any SSO authorization for your organization. |
| A pull request is missing. | The board shows open pull requests that match the section search, newest 50 only. Items in organizations that need SSO authorization can be missing until the token is authorized. |
| `You cannot review this pull request` | The loaded details say that you cannot review it, or the pull request is closed or merged. |
| `Cannot load people for <repository>` | The account cannot list assignable people. Check repository access. |
| The agent tools refuse a repository. | Star it with **Repositories…**. The tools use the work identity. |
| The diff shows `Diff too large to show here`. | Choose **View on GitHub ↗**. |

A successful local check does not prove that GitHub will accept a write.

## Limits

- Live GitHub behavior, writes, sign-in, diffs, background refresh and Settings
  saves were not tested while this guide was written. The statements above come
  from the board source and its tests.
- The lists come from GitHub searches for the selected account. **Mine**,
  **Review requested** and **Involved** are not limited by the favourites
  unless **Favourites only** is on; **All open** searches only the
  favourites, which also limit the agent tools.
- The repository search shows at most 30 repositories.
- Each section loads at most 50 pull requests. A tab count can be larger.
- A diff lists at most 3000 files. Expanding a hunk reads a file of up to 1 MB.
  An image is limited to 10 MB.
- Activity shows at most the latest 50 comments and 50 reviews. Checks list at
  most 100.
- The board has no issue sections, no merge action and no filters other than
  search.
- Saved lists are older data. They refresh from GitHub when the board loads.
- Isolated fixture screenshots for closing pull requests are in the
  [close action change record](../../docs/changes/subspace-api-accounts/README.md).
