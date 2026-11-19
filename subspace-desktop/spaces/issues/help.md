# Issues guide

<!-- Sources: spaces/issues/src/{board,view,keys,settings,authoring,detail}. -->

## Overview

The Issues board reads Subspace API issues and trackers. Open an issue to
read its description, sub-issues, relations and discussion, edit its properties,
or post a comment or reply with images and files. Choose **New issue…** to create an issue.

Choose **Issues** in the board switcher. Open **Help** in **Commands**, or
press `?` while the board has focus.

### Find and copy text

Press **⌘F** on macOS or **Ctrl+F** on Windows and Linux to find text on the
open issue page, including its description, properties, comments, activity
and an open text attachment. Matching ignores case. The bar shows the current
match and total. **Enter** and **Shift+Enter** move forward and back;
**⌘G / ⇧⌘G** (Ctrl+G / Ctrl+Shift+G elsewhere) do the same. **Escape** closes
find and returns focus. App-wide Search keeps its **⌘⇧F / Ctrl+Shift+F** shortcut.

Select text, then right-click for **Copy**, **Select all** for that block and
**Add to chat**. On a link the menu also offers **Copy link**. With no selection,
Copy uses the block under the pointer. Add to chat is an explicit manual action;
opening the menu sends nothing to chat and Issues supplies no automatic chat
context. The issue key retains its **Copy issue number** menu.

## Before you start

Sign in under **Settings > Account**. Subspace API uses that account; you do
not enter a token.

The first load can take up to a minute while Subspace API wakes.

## Main view

The heading shows **Subspace API** and the connected person's display name.
Choose **My issues**, **Active**, **Backlog**, **All issues** or **Activity**.

The list groups issues by workflow state within one project, or by state
type across projects. **Done**, **Cancelled** and **Duplicate** start folded.
Unfold a group to read its issues. The board remembers your folded groups.

Rows show the priority, project identifier (such as **SUB-12**), state,
tracker mark, title, progress, labels, due date, estimate and assignee when
available. Labels use their service colours. Narrow windows omit label chips.
A group loads another page when you move near its end. **Show more** reads
the next page; a failed page offers **Retry**.

The status line shows the issue count and when the list was read. A failed
network read keeps previously loaded rows under an **Offline** message.

### Activity

Choose **Activity**, or press `5`, for **Trackers in progress** and **Recently
updated · Past 7 days**. The project picker scopes both sections. Recent rows
show the latest change, its actor when available, and its age. Open a row to
read the issue. **Back** returns to the same Activity selection. **Load more**
reads another page for that section. Activity is not updated live: press
**Reload**, or `r`, to read it again. Activity does not update live yet.

## Toolbar and filters

| Control | What it does |
| --- | --- |
| **My issues** | Issues assigned to you. Its count shows your open issues. |
| **Active** | Issues in todo or in progress states. |
| **Backlog** | Backlog issues. |
| **All issues** | Issues across workflow states. |
| **Search issues** | Searches the current tab after a short typing pause. If only **All issues** has matches, it becomes the selected tab and keeps the query and explicit filters. From **My issues**, this can show work assigned to other people. An exact identifier takes precedence over text matches. |
| Initiative | Choose from your Subspace initiative memberships. This stays in sync with Channels. **All initiatives** clears the filter. |
| Project | Choose an active project in the current initiative. Clearing the initiative makes the full project list and **All projects** available. |
| **Assignee** | **Me**, **No assignee** or **All assignees**. In My issues, choose **All assignees** to open All issues. |
| **Priority** | Select priorities from **No priority** to **Low**, or clear with **Any priority**. |
| **Filters** | Workflow statuses for one project, or state types for all projects; issue or tracker type, due date and **Show archived**. |
| **Sub-issues** | Include sub-issues in their state groups. Off by default. |

Type in a menu to find an option. Use arrows and Enter to choose it, or its
number while the search is empty. Priority and Filters remain open for more
choices. Escape, Tab or a click outside closes the menu and restores focus.
Toolbar filters stay in memory. Changing the tab or project clears status
filters. My issues uses **Me**. Leaving it restores **All assignees**.

**Reload**, or `r`, reads the current list or issue again. Returning to the
board refreshes a list whose last read is more than a minute old.

Changes made elsewhere appear on their own in the issue list and the open
issue. The dot under the connection line shows **Live** once updates start.
It shows **Connecting…** while the Space opens the stream for the projects
you are viewing, **Reconnecting…** after an interruption, and
**Polling every 30 s** when the stream is unavailable. The Space reads the
open issue every 30 seconds until the stream is live.
Rows you loaded further down a list, sub-issues or relations are read again
and stay shown. Your scroll position and selection stay where they are.

If the issue you are editing changes elsewhere, the editor shows **Changed
elsewhere.** and **Reload**. Your draft is not touched until you choose
Reload, which takes the new version and keeps the fields you changed.

## Details

Click a row or press Enter. The issue page shows its ordered workflow, title,
state and metadata. Tracker progress comes from the server; **How tracker status
works** explains the calculation. The workflow ends in one slot: **Done**, or
**Cancelled** or **Duplicate** when the issue ended that way. Click the end slot,
or focus it and press Enter or Space, to choose the end state from the
project's completed, cancelled and duplicate states; **Edit issue…** changes
any state. A tracker's end slot follows its sub-issues and cannot be changed.
Narrow windows show the current state and its position instead.

Description, images, attachments, Sub-issues, Relations, Pull requests and Activity
form the main column. Properties and Record sit on the right, or below the main
column in a narrow window. Properties show state, priority, assignee, labels,
due date, project, milestone, parent and type; populated estimates and cycles
also appear. Record shows the creator, lifecycle dates, archive facts and update
date. Imported dates are labelled **Originally created**, **Originally started**,
**Originally completed** or **Originally cancelled** independently.

Open a parent, child or related issue from this page. **Show more** continues
Relations or Pull requests; **Show n more** continues Sub-issues when its total
is known. Each section keeps its rows on failure and offers **Retry**. Pull
requests show each repository, PR number and title beside a pull-request glyph.
The row action has the **Open in PRs** tooltip and accessible label. Click it or
focus it and press Enter to switch to PRs and open the detail panel. **Open on
GitHub** opens the browser. Both actions are also in the command palette,
named with the repository and PR number. If PRs is disabled or the URL is not
a supported pull request link, the primary action opens the browser and shows
a short notice. Older servers show **Pull requests are not available from
this server yet**. This does not stop the rest of the issue loading.

Blocked and archived notices reflect the server's flags. A partial list names
loaded blockers with **and others**. **More** contains **Copy identifier** and
**Reload**. Press `y` or Command/Ctrl+Period to copy the open issue identifier.
You can also right-click the identifier below the title and choose **Copy issue
number**. This copies the complete key, such as `SUB-31`.

The timeline starts on **Comments**. Choose **All** to include other activity.
**Show earlier comments** or **Show earlier activity** reveals earlier entries.
Replies appear beneath their root comment. Deleted comments show a tombstone.

Choose **Decisions** under **Activity** to read decisions linked to the issue.
**Show more** loads another page. The most recently decided appear first.
This tab has no comment composer. Open a decision here or from a
**Decision logged** entry in **All**. Both open the same side panel. Use
the panel's Back arrow to return after following a supersede link.

Choose **Back** or press Escape to return to where you opened the issue. Its
label names the place, such as **Back to issues** or **Back to #discovery**.
An issue opened from the list returns to the selected list row, with your
filters as they were. An issue opened from an issue key in a Channels
conversation returns to that conversation, where you were reading. An issue
opened any other way, such as from search, the palette or a notification,
returns to the issues list. Issues you open
from the issue page, such as a parent or sub-issue, keep that first place, so
Back leaves the issues rather than stepping through them. If the place is gone,
for example a channel you left, Back shows the issues list. Escape closes a
text attachment viewer first, then the decision panel, then follows the issue's
Back destination. Use `j`
and `k` to move between issues. Right-click a list row to open it or copy
its title or identifier. Supported Markdown media references show images or file chips.

## Actions

### Create or edit an issue

Choose **New issue…** or press `c`. From loaded detail, choose **Edit issue…**
or press `e` to change several fields. A property pencil instead edits just that
value inline, keeping the issue page visible. Choose the checkmark to save or
× to cancel. Command/Ctrl+Enter saves; Escape closes a picker before cancelling
the field. The Description pencil opens the full editor.

A new issue needs a **Summary**: one line of plain text, up to 300 characters,
which explains the problem the issue solves. The count below the field updates
as you type. Leading and trailing whitespace is removed. Most emoji count as
two characters toward the limit. **Create issue** stays disabled until the
required fields are valid.
The issue page starts with Summary, above Description. Older issues show
**No summary yet**. Use the Summary pencil to edit it inline, then save or cancel.
Once a summary is set, it can be changed but cannot be cleared.

Summaries need an API with the issue `summary` field; an older API rejects
unknown write fields.

Use the Project pencil to select another project, then choose the checkmark to
review the move. Confirmation lists the total and every new key and number.
Moving a tracker includes all its children. Cycle, milestone and estimate are
cleared; a separately moved child leaves its parent. History, files and old
issue links remain available. Review changed previews again before moving.
Assignees include people and agents. Estimates follow the project scale.
Milestones come from the issue's project. Cycles offer current and upcoming
cycles in that project; choose **No milestone** or **No cycle** to clear a link.
Trackers derive their state from children, accept no direct estimate and cannot join a cycle; the
disabled controls explain this in their tooltips. Description mentions use
`@[label](member:<user-uuid>)`, as ordinary Markdown text.

Choose **Create issue** or **Save changes**. Enter in a single-line field or
Command/Ctrl+Enter saves. Escape closes a picker first, then offers **Keep editing**
or **Discard** for an edited draft. Quitting warns about unsaved work.

An archived project cannot accept a new issue; choose an active project. An
archived issue must be restored before you save. Your draft stays in the form.
If an issue changed, choose **Read latest issue**, then **Use latest version and
keep edits**. Review the combined draft and save explicitly. Offline or failed
saves keep your draft. An unclear save freezes it; **Check issue** reads the latest
issue before you re-apply an edit.

For an unclear creation, **Check issue** looks for a new issue created by you
in the chosen project with the exact title. An older issue with that title
does not count. A failed check keeps the draft frozen. If a complete check
finds several new matches, check the issue list; the draft stays frozen. If it
finds no new issue, **Create again** becomes available. Creating again may
create a duplicate. Subspace never retries an unclear creation automatically.

### Post a comment or reply

1. Open an issue and type in **Leave a comment**.
2. Choose **Comment**. For a thread, choose **Reply**, enter text and submit.
3. Wait for the result. Drafts remain in memory while you browse issues.

If the result is unclear, the draft freezes. Choose **Check discussion** to
look for your matching comment. This reads the discussion and does not send
another comment. If no match is found, **Post again** becomes available.
Posting again may post the comment twice. Subspace never retries an unclear
post automatically.

**Discard** removes a draft. An unclear outcome first warns that the comment
may already be posted. Choose **Discard draft** to remove it or **Keep draft**
to retain it. Quitting warns about unsent or unconfirmed drafts.

### Images and attachments

Images appear in descriptions, comments and the Images section. Open an image
to view it. Use Left and Right to move between images, Tab to move between
viewer controls, and Escape to close the viewer and return to its opener.
**Show in the description** or **Show in the comment** reveals an image whose source is already loaded.
More image metadata loads as you page. Previews load near the visible area;
a cleared preview loads again when you open it.

Choose **Attach files…** in a comment or reply, or paste an image from the
clipboard. You can also paste copied files. Images can be PNG, JPEG, GIF or WebP, up to 10 MiB and 4,096 pixels
per side. Files can also be PDF, DOCX, XLSX, PPTX, TXT or Markdown, up to 10 MB.
Each draft can hold eight attachments and 32 MiB. Submission waits until all
attachments are ready. Remove deletes an unposted attachment; a failed removal
stays visible so you can try Remove again. After a complete check finds no
matching upload, **Remove** drops the row and lets you post. A short notice
explains that an orphaned upload may still exist on the server.

If an upload result is unclear, choose **Check again**. The app checks for the
uploaded file without sending it again. Keep the row while the result is
unclear; a failed check does not mean the upload failed. If an upload was
refused, **Retry** sends it only when you choose that action. File chips show
the name, type and size. Issue content and attachment drafts stay in memory.

Choose a text or Markdown file, in **Attachments** or where a description or
comment refers to it, to read it in a panel beside the issue. Markdown looks as
it does in descriptions: remote images stay links, and only web and email links
open, in your browser. Text files show in a monospace font. The panel shows the
file's name and size, and scrolls a long file. Drag the divider to resize the
panel, or focus it and use the left and right arrow keys. Double-click the
divider to restore the default width. This width is shared with PR details.
Choose **Close attachment** or press Escape to close it and return focus to
where it was. If a menu is open,
Escape closes the menu first. Files over 1 MiB (1,048,576 bytes) are too
large to show, and an empty file says so. If the file cannot be read, choose
**Retry**. Other files show their name, type and size only.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `j` / Down, `k` / Up | Move through list rows; `j` / `k` step between issues in detail. |
| Enter | Open an issue, fold a group or read more rows. |
| Escape | Close a menu, then dismiss the text attachment viewer, then the decision panel, then follow the issue's Back destination. |
| `1`–`4` | My issues, Active, Backlog, All issues. |
| `5` | Activity. |
| `m` | Focus the detail timeline. |
| `y` / Command/Ctrl+Period | Copy the open issue identifier. |
| `c` / `e` | New issue / edit the loaded issue. |
| Command/Ctrl+Enter | Save the issue draft. |
| Shift+S | Show or hide sub-issues in the list. |
| `/` | Focus the issue search. Enter or Escape returns focus to the list. |
| `r` | Reload the current view. |
| `?` | Open this guide. |

Plain keys do not switch views while you type in an input.

## Settings

**Settings › Board › Issues** shows **Subspace API**, its API address and:

“Sign in under Settings › Account.”

The connection state is **Signed in as** your display name, **Sign-in
needed**, **Offline**, or a connecting message. This page has no inputs.

## Troubleshooting

- **Sign-in required:** Sign in under **Settings > Account**, then reload Issues.
- **Offline:** “Couldn't reach Subspace API. Try again.” Check the connection
  and reload. The first load can take up to a minute while the API wakes.
- A failed list or detail read can offer **Copy details**, which copies the
  read type and HTTP status without response content or credentials.
- If a comment result is unclear, use **Check discussion** before **Post again**.

## Limits

This version creates and edits issues, displays images in an image viewer
and text or Markdown attachments in a panel, and posts comments and replies
with images and files. Archived issues and projects cannot accept edits or
comments. Choose an active project when
creating an issue. Pull requests require a server with the association route.
There is one built-in API connection.

Issue content and drafts stay in memory. The board has no agent tools,
chat context, full-text index, notifications or scheduled background updates.
Disabling the board stops its retained hub. Drafts do not survive quitting.

Debug builds can use a numeric-loopback API through `SUBSPACE_API_URL`, with
`SUBSPACE_API_ACCESS_JWT_FILE` passed through the client's fixture seam. Release
builds ignore both values. The token is never stored or displayed.

The offline fixture (`scripts/subspace_api_test_server.py`) exercises every
route the board uses.
