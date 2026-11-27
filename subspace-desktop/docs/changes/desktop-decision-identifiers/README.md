# Decision references and workspace refinements

## Intent and specification

Show each decision's project-scoped reference in the Decisions list and on its
page. Recognise it in Channels, fetch the current title and status on hover or
keyboard focus, and open the decision when activated. Back returns to the
source conversation or the preserved decision list. Make refreshes retain
the current view while fetching new data, improve the narrow PR review
composer, and bring Issues group headings up to the issue title size.

## Delivery

- Decisions displays the server's identifier and uses the UUID for record
  state and writes. A list supplied by an older API shows the project key.
- Channels uses the shared reference chip and passive preview frame. The
  preview performs a scoped read and expires on an account change.
- The broker routes `decisions@1/open` and carries the originating
  conversation as the return location, including from a split conversation.
- Decisions shares the initiative selection with Issues, Projects and
  Channels. Issues starts on All and remembers the last selected view.
- Fresh Space order is Channels, Issues, PRs, Fleet Console, Projects,
  Decisions and Actions. Saved order takes precedence. Projects starts
  enabled with an API backend. Copilot Agents requires the Copilot feature.
- Issues, Decisions, Actions, Channels and Projects have a toolbar refresh
  control. Refresh is the last control on the right, after the primary
  action where present. Same-account refreshes keep existing results
  visible while their replacements load. Channels hides unavailable issue tiles.
- The PR page toolbar shows Approve, Files, Close and Merge when applicable.
  The review composer fits the inspector and keeps a stable width on submit.
- Twilight Two's type sizes rise by one point and Issues group headings use
  the Issues list's row font size.

The Decisions, Channels, Issues, PRs, Actions, Projects, product and
contract tests cover the changed behaviour. The native capture used a
loopback Ergo server and the fixture API. It opened `D-00002-SUB` from
`#design`, displayed the current accepted decision and returned to the
same transcript through **Back to #design**. A second native run validated
the Space order, the Issues typography, and the composer buttons in the
default-width PR inspector.

### Capture evidence

- Decisions table [before](evidence/list-before.png) and [after](evidence/list-after.png).
- Decision detail [before](evidence/detail-before.png) and [after](evidence/detail-after.png).
- Channels [before](evidence/channels-before.png), [hover preview](evidence/channels-hover.png),
  [decision opened from Channels](evidence/channels-opened-decision.png), and
  [returned conversation](evidence/channels-back.png).
- Issues typography [before](evidence/issues-before.png) and [after](evidence/issues-after.png).
- Shared initiative navigation in [Decisions](evidence/initiatives-after.png)
  and [Projects](evidence/projects-after.png).
- Refresh placement in Issues [before](evidence/issues-after.png) and
  [after](evidence/refresh-right-issues.png), and at the right edge in
  [Projects](evidence/refresh-right-projects.png),
  [Decisions](evidence/refresh-right-decisions.png),
  [Actions](evidence/refresh-right-actions.png) and
  [Channels](evidence/refresh-right-channels.png).
- PR detail toolbar [before](evidence/pr-detail-before.png) and [after](evidence/pr-detail-after.png).
- PR review composer [before](evidence/pr-composer-before.png) and [after](evidence/pr-composer-after.png).

## Board checklist

The existing Space identities, artwork, workspaces, navigator, docks,
selection and storage are reused. The list and reading page use the shared
list and action button. The conversation uses the existing chip and hover
reference frame, with an equivalent keyboard path. The application shell
restores the buffered conversation and scroll position. The new navigation
contract is declared by both Spaces and documented in the
[Space audit](../../spaces-audit.md). Existing settings, status, notification,
indexing, chat context, diagnostics, tools and data storage do not need a
new entry for the reference and its session-bound preview.

## Tasks

- [x] Read and render references in the Decisions list and page.
- [x] Route Channels references with preview, activation and Back.
- [x] Cover reference recognition, preserved list selection and return routing
  in tests.
- [x] Verify the PR review controls and composer geometry in a narrow inspector.
- [x] Check fresh defaults, persistent choices and toolbar refreshes.
- [x] Compare Twilight Two and Issues grouping in fixture captures.
