# Workspace shell review findings: behaviour

Back to the [change record](intent.md).

The behaviour after this change, by area. The finding each part answers is
in brackets; [delivery](delivery.md) lists every finding with its outcome
and test.

## Decisions

- **Following the account (S12, F1, O1).** Every list and page read takes a
  lease on the signed-in account and organisation just before it starts.
  The Space looks for a change every quarter of a second, shown or hidden
  (the interval of the Issues Space's hub poll), and again before drawing,
  on showing, on Reload and on a Back, and before it opens a decision,
  follows a supersession, opens a menu, switches tab or opens a linked
  issue. When the account or organisation changes, or the person signs
  out, it drops the list, the selection, the open page and its tab, the
  directory (account, projects and people), the tags, the chosen tag, the
  search and the search field's text, and closes any menu. Reads in flight
  stop, and an answer read under the old lease is discarded. The Space then
  reads the list for the new account while shown, or when next shown; the
  next read always reads the directory again. The view, maker and project
  choices stay. The guide no longer tells the person to reload.
- **Superseded reads (S16).** A newer list read stops the one it supersedes
  before its next page.
- **Search (S14).** Committing a search clears the listed rows before the
  read, as a new choice does, so a failed search shows its reason and
  **Retry** rather than the rows found for the previous text. A failed
  Reload still keeps the rows under the **Offline** banner.
- **Back from a linked issue (S15).** Opening a linked issue gives the
  navigation an explicit origin: the open page, or in the list the selected
  decision. Back in Issues returns to that page, or shows the list with that
  decision selected again. Return keys carry an account epoch that changes
  with every account change, so a Back to a place shown for a previous
  account does not return there.
- **The 1,000-match limit (S36, left).** One read still lists at most 1,000
  decisions. The guide and the spec say so: the count ends in **+**, and
  narrowing the view, maker, project, tag or search reaches the rest.
  Paging past the limit is left for the owner to decide.
- The [developer page](../../decisions.md) describes the reads, the lease
  and the linked-issue origin.

## Issues

- **Organisation menu with the navigator hidden (S13).** On NETT, the
  full-pane screen that asks for an organisation keeps a toolbar holding
  only the organisation menu while the navigator, which otherwise holds it,
  is hidden. It names no view tab.
- **Back across a hub restart (N1).** When the hub is attached again, after
  an account change or after the Space was hidden, the issue page's Back
  origin goes with the page, so no view from the previous account, such as
  a decision, can become the Back destination. The arrival the shell has
  just named stays, so a link opening as the hub restarts still returns to
  the view it came from.

## Framework

The shared kit and host app changes (S1 to S4, S6 to S9 and S11) are
described in the
[Framework's record](../../../../subspace-framework/docs/changes/workspace-shell-review-findings/README.md):
row menus never act on a selection that did not move, docks yield so the
centre pane keeps its least width, navigator rows work from the keyboard
as a tree, a right-click on the selected row does not choose it again, the
**Add a Space** items are named, a Space added for after a restart is
current after it, zoomed Help hides the dock switches, Twilight Two's
Markdown colours meet normal-text contrast, and the close notice points to
the rail when navigation is vertical. [Settings](../../settings.md) says
where to add a closed Space again.

## Pull requests

- **Review drafts and accounts (S17).** A real account change drops the
  open review composer. A send already in flight writes into the sending
  account's data and closes only its own composer.
- **Toolbar actions and drafts (S18).** A review action on the same pull
  request switches the composer's mode and keeps its text. On another pull
  request with a non-empty draft it refuses, with "Send or cancel the draft
  on <key> first". Nothing changes while a review is sending.
- **Check links (S21).** The check link names the site it opens: "Open the
  <name> check on GitHub", or "on <host>" for another site; the detail
  panel's link is labelled the same way.
- **Status counts (S22).** While a repository filter is on, the repository
  count is taken across the whole section, like the pull request count.

## Channels

- **Toolbar target with the navigator hidden (S19).** Connection actions
  act on the network and conversation in front, the one the toolbar names,
  rather than the navigator's selection.
- **Focus with the navigator hidden (S20).** The Channels root always
  carries its key context and focus target, so keys keep working.

## Actions

- **Writes (S23).** The writes, such as re-run and cancel, act on the
  selected run only while the opened tab shows it. While the tab shows
  another run they act on nothing, never on a run out of sight; clicking
  the tab selects its run again.
- **Failed-job logs (S24).** The log picker lists only the selected run's
  failed jobs, and a log read checks the job belongs to it.
- **Attempts (S25).** Jobs reads carry a token. A new attempt clears the
  tab's jobs and log, and the page shows jobs only from the attempt it
  names.
- **Log reads (S26, S29, O2).** Only the newest log read lands. A failed
  log read shows "The job log did not load: <error>" in the job log and
  leaves the jobs as they were.
- **Columns (S27).** The trailing columns take at most 60% of the row.
  Branch and actor give way first, then status; the age keeps its width.
- **Quick selection (S28).** One jobs read runs at a time; rows passed over
  are never read.

## Copilot Agents

- **A task that leaves the list (S30).** Its tab stays, named
  **Not listed**, and closes.
- **Filter keys on an opened task (S31).** The list shows first and the
  filter menu opens there.
- **The work login (O3).** Every list load reads the work login again; a
  failed load keeps the last one.
- **Log prefetch (S35).** Selecting a session prefetches its log only when
  this Space's search policy is on. The
  [guide](../../github-copilot-agents.md) and the in-app guide describe the
  prefetch and the cache limits (eight logs, 4 MB).

## Packaging, guidance and docs

- **Unreleased features in release archives (S34).** `package-release.sh`
  and `package-macos.sh` first call `refuse_unreleased_features` in
  `scripts/product.sh`, which refuses a `SUBSPACE_BUILD_FEATURES` naming
  `board-lab`, `component-gallery` or `unreleased-spaces`. A local
  `make install-macos` still allows them.
- **Board Lab fixture target (F2).** `scripts/test-product-interfaces.sh`
  checks the Rust target before it compares the Space list, and exits 2
  naming the target.
- **Hooks on every commit (S33).** The root `AGENTS.md` runs the commit
  hooks on each commit with `git rebase --exec 'prek run --last-commit'`,
  then the push hooks on the range, and says why one ranged run is not
  enough.
- **Screenshot pairs (S38).** The desktop `AGENTS.md` asks for before and
  after in the same theme; when the before build lacks the prescribed
  theme, the pair is taken in a shared theme and the after state added in
  the prescribed one, with captions saying so.
- **Space directory (S39).** [Boards](../../subspace/boards.md) lists the
  Decisions guide.
