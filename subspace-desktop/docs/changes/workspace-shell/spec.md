# Spec: workspace shell

See the [intent](intent.md) and [tasks](tasks.md). Back to
[change records](../README.md).

Each requirement shows where it came from: **[O]** the owner's direction of
20 November 2026, **[D]** the mock-ups the owner approved that day, **[S]**
added here because the build cannot work without it, with the reason.

## 1. Zones

Every Space fills the same zones [O][D]:

| Zone | Holds | Size |
| --- | --- | --- |
| Navigator, the leading dock | The Space's name in its header, then what there is to look at: views, projects, networks, repositories | `--nav-w`, 240 px; header `--strip-h`, 34 px |
| Tab strip, top of the pane | What is open: the list, and an opened item beside it | `--strip-h` |
| Toolbar, under the tab strip | Search and filters on the leading side, actions on the trailing side | at least `--toolbar-h` |
| Content | The list or the page | the rest |
| Inspector, the trailing dock | The detail of the selection, with its actions in a footer | `--inspector-w`, 320 px; header `--strip-h` |
| Status bar, the shell's | Account, connection, counts and sync time, through `Dashboard::status`; the navigator's switch at its leading end and the inspector's at its trailing end, through `Dashboard::docks` | unchanged |

- Panels tile to the window's edges and to each other, separated by
  hairlines. No Space pads its outer edge [O].
- Text starts on one edge inside every panel: the content inset plus the row
  inset (`--pad-x` + `--row-inset`), the edge a shared list row's content
  already uses [D]. The mock-ups drew it at 16 px; it follows the theme, so
  it is 20 px in Cupertino and 12 px in One Dark [S: a second inset token
  would let the strips and the rows drift apart].
- A dock's header and the pane's tab strip are the same height, so the
  hairlines under them meet [D].
- Reading content, such as an issue's body, is no wider than 760 px. Lists
  use the pane's full width [D].
- The large Space heading and its line of context go. The Space's name is
  the navigator's title, and the context moves to the status bar [D].

## 2. Shared components

The Framework's `dashboard::workspace` module owns the layout; Spaces compose
it and style nothing themselves [S: the component rules].

| Component | Does |
| --- | --- |
| `Workspace` | The three-column frame. Gives each dock its stored width and a draggable inner edge. A dock left out is hidden. |
| `Navigator`, `nav_section`, `NavRow` | The leading dock, its section labels and its rows. A row shows a glyph or a leading mark, a label, a count and a trailing mark; the selected row shows through its own fill. |
| `Pane`, `Toolbar` | The tab strip, the toolbar and the content. The toolbar's leading items wrap in a narrow pane; its trailing items stay on the first line. |
| `Inspector`, `section`, `fact` | The trailing dock with a title, header actions, Close, a scrolling body and a footer; titled groups and labelled facts for its body. |
| `Workspace::detail`, `pane_head` | A panel that owns its width in the inspector's place; the tab strip and toolbar alone, for a list drawn under them; the inspector's footer bar of actions. |

- A `NavRow` can take a right-click menu through `NavRow::menu`, built from
  the [standard groups](../standard-context-menu/spec.md). A right-click
  runs the row's click first, so the menu acts on the highlighted row.

- A dock's width is a density token, held to its limits when stored
  (navigator 180 to 480 px, inspector 260 to 640 px) and to a share of the
  window when drawn (30 % and 40 %), with the minimum winning in a narrow
  window [S: a dock must stay usable and must not swallow the pane].
- Dragging a dock's inner edge stores the width. A double click on the edge
  goes back to the default [D].
- The three tokens appear in the theme designer's Window group.

## 2a. Look

The mock-ups also set how the shell looks inside its zones, judged in the
bundled Twilight Two theme [D]:

- **Twilight Two is the mock-ups' look.** A first-party theme,
  `twilight-two`, carries the mock-ups' own colours and their sizes: 32 px
  rows with the selection inset 8 px and 6 px corners, text 16 px in from
  each panel's edge, 34 px strips, a 36 px toolbar with 24 px controls, a
  38 px title bar with 26 px Space tabs, 13 px body text and 11.5 px code,
  in Inter and JetBrains Mono, which the Subspace App embeds (see
  [bundled fonts](../bundled-fonts/README.md)). Screenshots are taken in it.
  Other themes keep their own sizes and colours; the shell's layout is the
  same in every theme [O]. The colours the mock-ups do not show, such as
  syntax, come from GPUI Kit's Twilight, whose gold accent they share [S:
  every key needs a value].

- **Accent Space name.** The navigator's title, the Space's name, is bold and
  takes the accent colour [D].
- **Section labels in capitals.** `nav_section` and the inspector's `section`
  labels are small, semibold, muted and shown in capitals; their accessible
  names and ids keep the text as written [D]. The mock-ups track them by
  0.06 em; GPUI has no letter-spacing, so they are not tracked [S: not
  available in GPUI].
- **Plain open tab.** In a pane's tab strip (`tabs::pane_tab_bar`) each tab
  is as tall as the strip; the open tab takes the content's surface over the
  strip's bottom hairline, keeps a hairline on its trailing side and has no
  accent line along its top [D]. An opened item's tab shows its glyph, its
  key in the monospace family and muted colour, then its title and Close
  [D].
- **Rectangular toolbar controls.** Toolbar controls are the Toolbar button
  role's rounded rectangles; filters such as Assignee and Priority are named
  by their labels alone, and Filters wears a funnel [D]. The corner radius,
  height and text size are the theme's button-role tokens; a theme that sets
  a radius of half the height draws capsules [S: the shape is a theme value].
- **Borderless search.** The toolbar's search field (`fields::toolbar_search`)
  is filled with the quiet `pane` surface, with no border at rest; keyboard
  focus still draws the focus ring [D][S: focus must stay visible].
- **One primary action.** A pane toolbar has at most one primary action, the
  Space's main one, at its trailing end (`action::main_action`): New issue…
  in Issues, Trigger… in Actions and New project in Board Lab [D]. The
  design guide's rule 8 says so.
- **Quiet dock switches.** The dock switches sit at the ends of the status
  bar (see section 3) and show their state by a 14 px panel glyph that takes
  the accent while the dock shows, with no fill
  (`workspace::dock_switch`, `IconButton::latched_glyph`) [D].
- **Issues toolbar contents.** The list toolbar's trailing side holds, in
  order, the quiet text **Grouped by state**, the **Sub-issues** switch,
  pressed while sub-issues show, and **New issue…** [D]. The count is the
  status bar's; while sub-issues are hidden, the status bar, the switch's
  accessible name and its tooltip say how many are [S: nothing may become
  unreachable or undiscoverable].
- **Title bar Space tabs.** In a theme without Liquid Glass, the current
  Space's tab has a faint accent fill, a one-pixel accent ring and an accent
  label; Liquid Glass themes keep the glass capsule. Tab labels are medium
  weight [D].
- **Navigator counts.** A row's numeric count is in the monospace family
  [D].

## 3. Dock state

- Each Space remembers whether its navigator and its inspector show. Both
  show by default [D].
- The switches are the first and last controls of the shell's status bar:
  the navigator's before the Space's status, the inspector's after Help [D].
  The owner confirmed this on 21 November 2026, so the pane's toolbar no
  longer has them [O].
- A Space tells the shell which docks it has and whether each shows through
  `Dashboard::docks`, a `workspace::SpaceDocks` with each dock `None` where
  the Space has none, and takes a switch's click back through
  `Dashboard::toggle_dock`. Both default to no docks, so a web Space and any
  Space off the workspace shell draw no switch. `DashboardHandle` carries
  both to the shell, beside `status` [S: the shell cannot reach a Space's
  stored choices otherwise].
- The status bar draws a switch only while the Space is on show: not over
  the log view, Search or a maximised terminal [S: a switch there would act
  on a dock nobody can see]. The switches keep the ids
  `workspace-navigator-toggle` and `workspace-inspector-toggle` and the
  names **Hide navigator** or **Show navigator** and **Hide inspector** or
  **Show inspector**, are compact and flat like the bar's other commands, and
  are native Tab stops.
- Each Space's inspector switch shows only where the Space has an inspector
  to show: Issues reports none on its activity feed or while a screen stands
  in for the list, Channels reports no docks before it has a network, Pull
  requests reports none while a diff fills the Space, and Board Lab reports
  a navigator only [S: as the toolbar's switches were before].
- With the navigator hidden, its views stay reachable from a view menu at the
  start of the toolbar [D].

## 4. What each Space puts where

| Space | Navigator | Tab strip | Toolbar | Inspector |
| --- | --- | --- | --- | --- |
| Issues | Views, projects, initiatives | The view, and the open issue | Search, Assignee, Priority, Filters; Sub-issues, New issue | A peek of the selected issue; Properties, Relations and Record on the issue page |
| Channels | Networks, channels, direct messages | The conversations' own tabs | Channel and topic; Join, More | Members |
| Pull requests | Views, then the section's repositories as a filter | The view, and the opened pull request | Repositories (favourites), the repository filter while one is on, Refresh | The selected pull request, with its actions in the footer |
| Actions | Views, then the groups of the current grouping | The view, and the opened item | Filters; Trigger | The selected run or item, with its actions |
| Copilot Agents | Views, then repositories | The view, and the opened task or chat | Filters; Back and Browser under an opened item | The selected task or chat, with its actions in the footer |
| Decisions | Views, makers, projects | The view, and the opened decision | Search, Tag | A peek of the selected decision; see [its record](../decisions-space/spec.md) |
| Board Lab | Projects, environments | The open project | Project actions; New project | None |
| Web Spaces | None | None | Back, Forward, Reload, Home | None |

## 5. Issues

- One click on an issue row selects it and loads it in the inspector. It no
  longer opens the page [O].
- A double click or Enter opens the page [O].
- The selected row shows an Open button at its trailing edge, and the peek's
  header has one too [O][D].
- The peek is drawn from the row the list already holds: state, title,
  summary, priority, assignee, project, labels, due date and latest
  activity. It makes no new read [S: the hub holds one open issue at a
  time].
- On the issue page the inspector holds Properties, Relations and Record, and
  the body is one reading column. With the inspector hidden, those three
  sections follow the body, as the old narrow layout stacked them [S:
  nothing may become unreachable].
- The page's path bar becomes the pane's toolbar: Back and the path on the
  leading side, Edit issue… and More on the trailing side [D].
- The issue page is restyled to the shell [D]: Properties are plain rows of
  label and value; the workflow is a slim bar with one segment per state,
  filled up to the current state, whose name is in strong text; editing
  pencils show only while the pointer is over their row or section, or when
  Tab reaches them; the discussion ends in a one-line **Write a comment…**
  box that opens the editor.

## 5a. Pull requests, Actions and Copilot Agents

These three Spaces follow the same select-then-open rule as Issues [O][D],
which section 7 first deferred:

- One click on a row selects it and shows it in the inspector, with its
  actions in the inspector's footer. With nothing selected, the inspector
  says so in one line.
- A double click, Enter or the inspector's **Open** opens the item as a
  second tab beside the view's tab, with a close button. Choosing the view's
  tab, or closing the item's tab, returns to the list. Opening another item
  replaces the opened one.
- Pull requests: the navigator's **Repositories** section lists the
  repositories with pull requests in the current section, with counts.
  Choosing one filters the list to it; choosing it again, or the toolbar's
  **&lt;owner/name&gt; only**, clears the filter. The filter is not stored.

## 5b. Channels

The conversation's content is restyled to the shell [D]: the toolbar names
the conversation in front in strong text with its topic beside it, message
text is no wider than the reading width, the day divider sits between two
hairlines, and server lines are smaller and quieter with a monospace time
column.

## 6. Status bar

Each Space's `Dashboard::status` carries what its heading used to: the
service and account, the connection or live state, counts and the time of
the last read, for example "Subspace API · Signed in as Ada Fixture · Live ·
44 issues · updated just now" [D].

## 7. Deferred

- **Keyboard resizing of the docks** [S: the detail panel's handle takes the
  arrow keys; the docks' handles take only the pointer for now].
- **More than one opened item at a time** [D]. Each Space shows one opened
  item as a tab beside the view; Issues holds one open issue in its hub.

No longer deferred:

- **Dock switches in the status bar** [D][O], now in section 3, through
  `Dashboard::docks` and `Dashboard::toggle_dock`.
- **A rebuilt inspector for Pull requests, Actions and Copilot Agents**,
  now in section 5a.
- **Opened items as tabs**, now in sections 5 and 5a, one at a time.
- **The standard right-click menu** and **the title bar's add-a-Space
  button** [O], each built as its own change:
  [the standard right-click menu](../standard-context-menu/spec.md) and
  [add a Space from the title bar](../add-space-from-title-bar/spec.md).

## 8. Tests

- Framework: the dock width limits and the text edge, as unit tests; a
  Component gallery card for each component.
- Each Space: the tests that pinned the old heading, tabs and insets now
  describe the new zones; new pure logic (which navigator row is selected,
  what the peek shows) has unit tests.
- Issues: one click selects without opening; Enter and the Open button open.
- Pull requests, Actions and Copilot Agents: one click selects into the
  inspector; Enter, a double click and Open open the item's tab; Pull
  requests' repository filter.
