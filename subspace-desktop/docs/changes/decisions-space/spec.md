# Decisions Space behaviour

Back to the [change record](intent.md).

## Workspace

- **Navigator**, titled **Decisions**. **Views**: All decisions, Proposed,
  Accepted, Rejected, Superseded, each a status filter on the list read.
  **Made by**: Anyone, People, Agents (`madeByType`). **Projects**: All
  projects, then each active project the account can see, and the chosen
  one even when it is archived or gone. While a section is on its first row
  every row shows its count from the loaded decisions; otherwise only the
  chosen row does. A count ends in `+` when the read stopped before the last
  page. The footer names the signed-in account.
- **Tab strip**: the current view's tab and, while a decision is open, a
  closable tab with its title. The list's tab shows the list and keeps the
  decision's tab.
- **Toolbar**: the view menu while the navigator is hidden (views, makers and
  projects in sections), the search (`q`, read after a 300 ms pause, Enter or
  Escape returns to the list), the **Tag** menu once a read has shown a tag,
  and the count. The dock switches are at the status bar's two ends,
  reported through `Dashboard::docks`, as the
  [workspace shell](../workspace-shell/spec.md#3-dock-state) sets out.
- **Content**: the shared list (`ListDashboard`), grouped by status in the
  order Proposed, Accepted, Superseded, Rejected. In All decisions, Superseded
  and Rejected start folded; another view or a search starts with every group
  open. A row shows the project key, the title, a **supersedes** or
  **superseded** mark, the linked issue count, the maker (a person's name, or
  an agent's name and rank) and the decided-on date (with the year when it is
  not this year). Superseded and rejected titles are muted.
- **Inspector**, a peek from the list read: status, title, Decided on, Made
  by (an agent with rank and model), Recorded by, Project, Tags;
  **Supersedes** and **Superseded by** rows that select that decision
  (unfolding its group) or open it when the list does not hold it; **Linked
  issues** rows that open the issue; **Evidence** (kind, reference as a link
  when it is a safe address, excerpt). Header action **Open**. With nothing
  selected: "Select a decision to see it here."
- **Page**: one reading column no wider than `workspace::READING_WIDTH_PX`:
  a **Superseded by** or **Supersedes** banner that opens the other decision,
  the title, status and decided-on date, the sections Decision, Context, Why,
  Implications and Alternatives as Markdown (an empty one is left out), then
  Evidence. Beside it the inspector holds the facts: status, decided on, made
  by, recorded by, the accepted or rejected stamp and who made it, project,
  tags, revision, then supersession and linked issues.
- **Status bar** (`Dashboard::status`): `Subspace API · Signed in as <name> ·
  <n> decisions · updated <age>`, or how the read went.

## Interaction

- One click selects a row and loads the peek (`activates_on_click` is false
  for decision rows). A double click, Enter, the inspector's **Open** or
  the row menu's **Open** opens the decision as a page. Escape or the tab's Close
  closes it, leaving the decision selected.
- Keys: `1`–`5` the views, `/` the search, the shared list keys, `r` reload.
  The palette offers every view, maker and project, **Filter by tag…**,
  **Search decisions**, **Open decision** or **Close the decision**, **Open
  issue …** for the selection's linked issues and the dock switches.
- A linked issue asks `issues@1/current-scope`, binds
  `{api_origin}/v1/issues/{identifier}` to the scope and emits
  `DashboardEvent::Navigate` with that responder key. With no answering Space
  the Space shows "… can't open here. Turn on Issues in Settings and sign in,
  then try again." `return_location` names the open decision, so Back in
  Issues returns to it.

## Reads and states

- The list reads up to ten pages of 100. The first read, reload and a stale
  list (over a minute old on showing the Space) also read `/me`, the projects
  and the people, which name human makers and recorders.
- Before the first answer the list shows its skeleton. A sign-in, access or
  organisation failure clears what was read and shows the reason with
  **Retry**. A network failure keeps the decisions under the shared
  **Offline** banner. Empty lists say whether the search or the choices left
  nothing.
- A decision page read that is not found says the decision is no longer
  available; another failure offers **Retry**.

## Stored choices and privacy

View, maker, project and both docks are stored in `prefs`; the tag and search
are not. Decision content stays in memory: no tools, chat context, index,
notifications, background updates or cache.
