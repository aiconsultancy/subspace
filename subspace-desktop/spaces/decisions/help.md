# Decisions guide

<!-- Sources: spaces/decisions/src/{board,navigator,toolbar,inspector,page,keys,palette,open_issue,search}. -->

## Overview

The Decisions Space lists the decisions recorded in Subspace API in one
searchable place: what was decided, who made it, when, and whether a later
decision superseded it. Decisions are logged against issues, by people and by
agents; the Issues Space shows the decisions of one issue, and this Space
shows them all and lets you propose and review decisions.

Choose **Decisions** in the Space switcher. Open **Help** in **Commands**, or
press `?` while the Space has focus.

## Before you start

Sign in under **Settings > Account**. Decisions uses the same account as
Issues; you do not enter a token. The first read can take up to a minute
while Subspace API wakes.

When you switch account or organisation, or sign out, the Space at once
drops everything it read for the previous account: the list, the selection,
an open decision and its tab, the account's projects, people and tags, the
chosen tag and the search. A read still in progress for the previous account
is discarded when it finishes. The Space then reads the list for the new
account, or shows **Sign-in required** after a sign-out. The view, maker and
project you chose stay. **Back** from another Space no longer returns to a
decision shown before the switch.

## Main view

The navigator on the left is titled **Decisions**. Under **Views**, choose
**All decisions**, **Proposed**, **Accepted**, **Rejected** or
**Superseded**. Under **Made by**, choose **Anyone**, **People** or
**Agents**. Under **Projects**, choose **All projects** or one project you
can see. Each choice reads the list again, and the Space remembers the view,
the maker and the project.

While a section is on its first row, every row in it shows how many of the
listed decisions it holds. Otherwise only the chosen row shows its count. A
count ending in **+** means there were more decisions than the Space reads at
once.

The list groups decisions by status: **Proposed**, **Accepted**,
**Superseded**, then **Rejected**. In **All decisions**, Superseded and
Rejected start folded; a search opens every group. Within a group the most
recently decided come first. A row shows the decision reference, such as
`D-00042-EXA`, the title, a
**supersedes** or **superseded** mark, the number of linked issues, who made
the decision (a person's name, or an agent's name and rank) and the date it
was decided. Superseded and rejected titles read quietly. The detail page
also shows the reference. Paste it into a Channels message to share the
decision. **Back** returns to the channel or direct message it came from, or
to the Decisions list when opened there.

The status bar shows **Subspace API**, the signed-in account, the number of
decisions and when the list was read. A failed reload keeps the decisions
already listed under an **Offline** message. A new search or choice clears
the list first, so if its read fails the Space says why and offers
**Retry** rather than showing the earlier decisions.

## Toolbar and filters

| Control | What it does |
| --- | --- |
| **Search decisions** | Searches titles, decisions and their context after a short typing pause. Enter or Escape returns to the list. |
| **Tag** | Shows only decisions with the chosen tag. **Any tag** clears it. It appears once a read has shown a tag. The tag is not remembered. |
| View menu | While the navigator is hidden, it names the current view and holds the views, the makers and the projects. |
| Count | How many decisions the list holds. |
| Navigator and inspector switches | Show or hide the navigator, at the start of the status bar, and the inspector, at its end. The Space remembers both. |

Type in a menu to find an option. Use the arrow keys and Enter to choose it.
Escape, Tab or a click outside closes the menu.

**Refresh**, or `r`, rereads the list, account, projects and people. It
also rereads the decision when its page is open. Showing the Space reads a
list that is more than a minute old.

## Details

Click a row to select it. The inspector on the right shows the decision's
status and title, when it was decided, who made it (an agent with its rank
and model), who recorded it, its project and tags. **Supersedes** and
**Superseded by** name the other decision: choose it to select it in the
list, or to open it when the list does not hold it. **Linked issues** lists
the issues the decision is linked to, and **Evidence** shows where it came
from, with its reference and excerpt. With nothing selected the inspector
says **Select a decision to see it here.**

Double-click a row, press Enter, or choose **Open** in the inspector to open
the decision as a page. It opens in a tab beside the list's tab. The page
starts with a banner when the decision was superseded (**Superseded by**) or
supersedes another (**Supersedes**); choose the named decision to open it.
Then come the title, the status and the date, and the sections **Decision**,
**Context**, **Why**, **Implications** and **Alternatives**, leaving out any
that are empty, then **Evidence**. The text is no wider than a comfortable
reading width. The detail toolbar puts Back and the decision reference on
the left. **Revision history** is the clock button on the right, beside
the actions available for the open decision.

Beside the page, the inspector shows the decision's facts: status, decided
on, made by, recorded by, when and by whom it was accepted or rejected, the
project, tags and revision, then its supersession and linked issues.

Choose the list's tab to see the list again; the decision's tab stays open.
Close the decision's tab, or press Escape on the page, to close it.

## Actions

### Open a linked issue

Choose a linked issue in the inspector, or **Open issue** in the command
palette. The issue opens in the Issues Space, and its **Back** returns to the
decision you opened it from: its page, or the list with that decision
selected. If Issues is turned off, or not signed in, the
Space says so and stays where it is.

### Follow a supersession

Choose the decision named under **Supersedes** or **Superseded by**, or in
the page's banner. In the list it selects that decision and unfolds its
group; on a page, or when the list does not hold it, it opens that decision.

### Copy a title

Right-click a row for **Open** and **Copy title**. Select text in the
inspector or on the page and right-click to copy it.

### Propose, edit and review

Choose **New decision**, select a project, and give the proposal a title and
the decision itself. Enter issue identifiers to link work in the same project.
You can also record the context, reasoning, implications, alternatives and tags.

The app records you as the recorder. To credit someone else, select the human
maker and provide an evidence type and reference. For a Subspace comment, use
the ID of a comment written by that maker. Evidence stays available on the
decision page.

Select a proposed decision in the list and use **Accept** in the toolbar.
Confirming opens the accepted decision. Open a proposed decision to **Edit**,
**Accept**, or **Reject** it from the detail page. These actions are
available on active projects to members allowed to change the proposal. If the record changed while you were editing, the app keeps your
draft and shows the current revision. Select **Use current version** after
reviewing it, then save again. Your link and tag changes are applied to the
current list. If the API returns only some of a decision's linked issues,
the editor preserves all of its links when you save other changes.

To replace an accepted decision, open it and choose **Propose replacement**.
The earlier decision is superseded when the replacement is accepted.

Choose **Revision history** on a decision's page and open a revision to see
its saved snapshot. Revisions include who saved them and which fields changed.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `j` / Down, `k` / Up | Move through the list. |
| Enter | Open the selected decision as a page, or fold or unfold a group. A click only selects a decision; a double click opens it. |
| Escape | Close a dialog or menu, then close the open decision. |
| `1`–`5` | All decisions, Proposed, Accepted, Rejected, Superseded. |
| `/` | Focus the search. Enter or Escape returns focus to the list. |
| `r` | Read the list again. |
| `?` | Open this guide. |

Plain keys do not act while you type in the search. The command palette lists
every view, maker and project, **Filter by tag…**, the available decision
actions, the linked issues of the selected decision and the dock switches.

## Settings

**Settings › Board › Decisions** turns the Space on or off. It has no other
settings: the account is the one under **Settings › Account**.

## Troubleshooting

- **Sign-in required:** Sign in under **Settings > Account**, then choose
  **Retry**.
- **Offline:** “Couldn't reach Subspace API. Try again.” Check the connection
  and choose **Retry** or press `r`.
- **Access denied:** your account may not read decisions. Try again once
  access is restored.
- A linked issue that does not open: turn on Issues in Settings and sign in.
- A person shows as **A person** when the account's people list does not
  include them; reload to read it again.

## Limits

One read lists at most a thousand decisions. When more match, the count ends
in **+** and the decisions after the first thousand are not listed; narrow
the view, maker, project, tag or search to reach them. A decision shows at
most the first fifty linked issues the API returns, with the number of the
others.

Decision content stays in memory. The Space has no agent tools, chat
context, full-text index, notifications or background updates, and it keeps
nothing on disk except the navigator choices and dock switches. Switching
account or organisation, or signing out, drops what was read for the
previous account without a reload.

The offline fixture (`scripts/subspace_api_test_server.py`) serves three
fictional decisions for local work.
