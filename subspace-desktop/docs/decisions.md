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
| `load` | The `Reader`: a list read (account, projects, people, then every page of decisions) and a detail read, run with `cx.background_spawn`. Tests replace it with fixture answers. |
| `state`, `listing`, `words`, `peek` | Pure: what the Space holds between reads, the status groups and their first folds, the counts, the wording and the inspector's facts. |
| `board`, `search`, `page` | The Space's changes: reads and answers, choices, the debounced search, and the decision page with its tab. |
| `navigator`, `toolbar`, `menus`, `lanes`, `inspector`, `page/view`, `render` | The workspace: navigator, tab strip and toolbar, the row lanes, the inspector and the reading column. |
| `open_issue` | The `issues@1/current-scope` request that opens a linked issue. |

## Reads

A read builds a `DecisionQuery` from the view (one status, or none), the
maker (`human` or `agent`), the project key, the tag and the committed search
text (`q`, at most 200 characters), and asks for pages of 100 until the last,
at most ten pages. A repeated cursor fails the read. The first read, a reload
and showing a list that is more than a minute old also read `GET /me`, every
project and the account's people (`Client::issue_choices` with
`ChoiceSource::Assignees`), which name the people who made and recorded
decisions. A failure of the projects or people read leaves those parts empty.

Each list read has a number; an answer to an older read is dropped. A failed
sign-in, a refusal or a missing organisation clears what was read with that
account. A network failure keeps the decisions on screen under the shared
list's banner.

The page reads `GET /decisions/{id}` once, again on **Retry** and on reload.
Not found shows that the decision is no longer available.

## Linked issues

A decision's linked issues open in the Space that answers the `issues`
contract, as Channels and web Spaces open them. The Space declares
`requests issues@1/current-scope`, asks for the scope, builds
`{api_origin}/v1/issues/{identifier}` and binds it to the scope with
`IssueScope::navigation_url`, then emits `DashboardEvent::Navigate` with
`MessageKey::request::<CurrentScope>()` as the responder key. The shell calls
the answering Space's `open_link` and switches to it; its Back returns to the
open decision through `Dashboard::return_location`. With no answering Space,
or an identifier outside its scope, the Space shows a notice instead. The
declared role is in [the Spaces audit](spaces-audit.md#declared-contracts).

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
