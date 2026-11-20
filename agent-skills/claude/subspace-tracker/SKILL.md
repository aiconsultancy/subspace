---
name: subspace-tracker
description: Use the user's Subspace issue tracker, decisions, documents, projects and initiatives through the Subspace API MCP server (tools mcp__subspace__*), on the user's behalf and always marked as written by you. Use when the user mentions Subspace issues such as NAV-12, decisions, initiatives, projects, cycles or the fleet's work.
---

# Subspace tracker

Subspace's tracker holds the user's **issues** (keys like `NAV-12`),
**decisions**, **documents** and reviews, **projects**, **cycles**,
**milestones** and **initiatives**. Each initiative belongs to a fleet. The
fleet's rear admiral and captains use the same tracker, and the user sees it in
the Subspace desktop app. You reach it through the Subspace API's MCP server,
`subspace`.

## You act with the user's login, so always say who you are

The MCP server signs you in as the user. Everything you write appears under
their name. Therefore:

- **Start every text you write with a marker naming you and your model**:
  `**On behalf of <Name> (<Model and version>):**`. This covers issue
  descriptions, comments, decisions and their rationale, documents and project
  updates. Use your exact model and version as your system prompt states it,
  for example "Claude Opus 5.5"; if you do not know it, ask the user. Keep issue and
  document titles clean.
- For a change without text of its own (status, assignee, labels, cycle,
  moving an issue), add a one-line comment on the issue saying what you changed,
  with the same marker, unless the user tells you not to.
- Get the user's name from `python3 ~/.subspace/bin/subspace_token.py --me`, or ask them.
- Ask before archiving, deleting, changing a decision's status or rewriting
  someone else's text. Never write secrets into the tracker.

## Tools

`tools/list` gives every tool and its input schema. The main groups are:

| Area | Tools (prefix `subspace_`) |
| --- | --- |
| Find work | `list_initiatives`, `list_projects`, `get_project`, `list_issues`, `group_issues`, `list_issue_views`, `list_issue_statuses` |
| Issues | `get_issue`, `create_issue`, `update_issue`, `move_issue` (with `preview_issue_move`), `archive_issue`, `list_issue_activity`, `list_issue_relations`, `list_issue_pull_requests`, `set_issue_subscription` |
| Comments | `list_comments`, `list_comment_replies`, `create_comment`, `edit_comment`, `set_comment_resolution` |
| Decisions | `list_decisions`, `get_decision`, `log_decision`, `update_decision`, `set_decision_status`, `list_decision_revisions` |
| Documents and reviews | `list_documents`, `get_document`, `create_document`, `update_document`, `get_review_requirements` |
| Planning | `list_cycles`, `add_issues_to_cycle`, `list_milestones`, `list_project_updates`, `create_project_update` |

Updates take the record's current `version`. If one fails with a conflict,
read the record again, check what changed and retry only if your change still
makes sense.

## Setup and troubleshooting

The installer registered `subspace` as a user-scope MCP server for Claude
Code: `http://127.0.0.1:3000/mcp`, with a `headersHelper` that runs
`python3 ~/.subspace/bin/subspace_token.py --header` and refreshes the token
by itself. Check it with `claude mcp get subspace`. Its tools appear as
`mcp__subspace__subspace_<name>`.

- **Cannot connect**: the fleet's tunnel is not running. Ask the user to run
  `./install.sh tunnel --background` (Windows: `.\install.ps1 tunnel
  --background`) in the Subspace repository.
- **HTTP 401**: the token expired. The helper mints a new one on the next call; if it keeps failing, check `python3 ~/.subspace/bin/subspace_token.py --me`.
- **HTTP 403**: the user's account may not do that; tell them rather than
  working around it.
