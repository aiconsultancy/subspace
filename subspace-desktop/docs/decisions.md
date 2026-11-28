# Decisions

The Decisions Space (`spaces/decisions`, package `subspace-decisions-board`,
board id `decisions`) lists the decisions recorded in **Subspace API** in one
searchable place, grouped by status, with who made each one and what
superseded it. It reads only. The [in-app guide](../spaces/decisions/help.md)
covers the controls, and the [change record](changes/decisions-space/intent.md)
the owner's request and the checks.

## Ownership and structure

`clients/subspace-api` owns the wire models (`DecisionListItem`,
`DecisionDetail`, `DecisionQuery`) and the blocking reads `Client::decisions`
and `Client::decision`. The Space owns presentation, the reads it starts and
the contract request that opens a linked issue. It does not use the Issues
Space's hub, which follows one open issue, and it does not depend on the
Issues crate.

| Module | Holds |
| --- | --- |
| `model` | The navigator's choices (view, maker, project, tag) and the `DecisionQuery` they and the search build. |
| `load` | The `Reader`: a list read (account, projects, people, then the first batch of decisions), a detail read and the `Lease` each read returns on the account of the client that made it, run with `cx.background_spawn`. A list read carries a `Stop` that a newer read sets. Tests replace the reader with fixture answers. |
| `account` | Following the signed-in account: the watch that notices an account or organisation change, and what the Space drops when it does. |
| `state`, `listing`, `words`, `peek` | Pure: what the Space holds between reads, the status groups and their first folds, the counts, the wording and the inspector's facts. |
| `board`, `search`, `page` | The Space's changes: reads and answers, choices, the debounced search, and the decision page with its tab. |
| `navigator`, `toolbar`, `menus`, `lanes`, `inspector`, `page/view`, `render` | The workspace: navigator, tab strip and toolbar, the row lanes, the inspector and the reading column. |
| `open_issue` | The `issues@1/current-scope` request that opens a linked issue. |

## Reads

A read builds a `DecisionQuery` from the view (one status, or none), the
maker (`human` or `agent`), the project key, the tag and the committed search
text (`q`, at most 200 characters). Each batch shows up to 100 decisions.
**Next** follows the continuation. **Previous** reads from the saved start of
the previous page. Only the current page is rendered, even after browsing
many pages. A new search, filter or refresh starts at Page 1.
Initiative views merge the heads of each project's cursor stream and buffer
rows that belong in the following batch. For NETT the initiative is filtered
locally because its decisions endpoint has no project predicate. A repeated
cursor fails the read. The toolbar and navigator counts refer to the visible page. A **+** after
a count means a next page exists.
The first read, a reload and showing a list that is more than a minute old
also read `GET /me`, every project and the account's people (`Client::issue_choices` with
`ChoiceSource::Assignees`), which name the people who made and recorded
decisions. A failure of the projects or people read leaves those parts empty.

Each list read has a number. A keystroke, new request or account change
supersedes old work and sets its `Stop`, which prevents further page requests.
Search debounces for 200 ms. The toolbar marks the search as running while
the previous results stay visible. The newest successful response replaces the
list and its cursor together. On a temporary search failure the list shows a
banner and labels the retained rows **Previous results**. A different choice
starts a fresh list. Sign-in failures, access refusals and missing organisations
clear the rows and continuation for the old account.

The page reads `GET /decisions/{id}` by UUID from the list or by reference
from Channels. It uses the returned UUID for later reads and writes. The
`decisions@1/open` navigation capability delivers the reference and the
conversation's return point through the shell. **Back** restores that
conversation, or the kept list and selection for a page opened in Decisions.

### Following the account

Each read, list or page, returns a `Lease` on the account and organisation
of the very client that made it, so the lease is as old as the client and
an account change at any point after the client was made ends it. A read
that can make no client reads nothing for any account: it has no lease,
shows only its refusal, and leaves the Space watching the account what it
holds was read for. The Space looks for an account or organisation change, or a
sign-out, every quarter of a second while it lives, shown or hidden (the
same interval as the Issues Space's hub poll), and again when it is shown or
drawn, before it opens a decision or a linked issue, and on a Back from
another Space.

When the lease has moved, the Space drops everything read for the previous
account: the list, the selection, the open page and its tab, the directory
(account, projects and people), the tags, the chosen tag and the search,
including the search field's text and any open menu, which a frame that
finds the change clears before it is drawn. Reads in flight stop,
and an answer that arrives under the old lease is discarded and drops what
the Space holds, as the change itself does. The Space bumps its account
epoch, then reads the list for the new account while it shows, or when it
is next shown. The view, maker and project choices stay.

## Linked issues

A decision's linked issues open in the Space that answers the `issues`
contract, as Channels and web Spaces open them. The Space declares
`requests issues@1/current-scope`, asks for the scope, builds
`{api_origin}/v1/issues/{identifier}` and binds it to the scope with
`IssueScope::navigation_url`, then emits `DashboardEvent::Navigate` with
`MessageKey::request::<CurrentScope>()` as the responder key. The shell calls
the answering Space's `open_link` and switches to it. With no answering
Space, or an identifier outside its scope, the Space shows a notice instead.
Nothing opens once the decision it was listed under was read for an account
that is no longer signed in. The declared role is in
[the Spaces audit](spaces-audit.md#declared-contracts).

The navigation carries an explicit `origin` for the destination's Back: the
open page (key `decision:<epoch>:<id>`), or, when the list shows, the
decision selected there (key `decision-row:<epoch>:<id>`), named by its
title. `Dashboard::return_location` gives the open page alone, for the
shell's own history. On a Back, `return_to` opens that page again, or shows
the list with that decision selected when the list still holds it. The
account epoch in the key changes on every account change, so a Back to a
place shown for a previous account does not return there.

## Stored choices and privacy

`state.json` keeps `decisions.view`, `decisions.maker`, `decisions.project`,
`decisions.navigator` and `decisions.inspector`. The tag and the search are
not kept. Decision content stays in memory: the Space has no agent tools,
chat context (`agent_context` is empty and row menus leave chat out),
full-text index, notifications, background updates or cache.

## Fixtures and tests

`scripts/subspace_api_test_server.py` serves three fictional decisions
(superseded, accepted and proposed) with the list's filters, so
`make capture` with `CAPTURE_STATE` choosing `decisions` shows the Space with
fixture data. The crate's tests use their own fictional decisions in
`src/fixtures.rs`: pure tests of the query, grouping, folds, counts, wording,
peek and page facts, and headless tests of the list, the navigator, the
toolbar, the inspector, the page and the linked-issue navigation.
`src/fixtures/accounts.rs` switches between two fictional signed-in
accounts, so the account tests show what an account change, a sign-out and
an answer read for the previous account each leave on screen.
