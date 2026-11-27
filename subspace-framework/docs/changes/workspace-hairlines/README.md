# Workspace hairlines

## Intent

The owner found two alignment defects in the built app. In Projects, the
hairline under the navigator's header sat about 2 px above the one under
the pane's top strip. In Issues, the first pane tab, Backlog, drew its own
leading line against the navigator's trailing line, so the divider there
looked twice as thick. He asked for both to be fixed and for the rules to
be written down: lines closer than about 5 px across neighbouring panels
are made into one line, and two borders never butt together.

## Spec

### Root causes

- **Projects and Channels have no tab strip.** Their pane starts with a
  plain `workspace::Toolbar`, which is at least `--toolbar-h` high (36 px
  in Twilight Two, 44 px in Cupertino) and pads its items by 4 px. The dock
  headers are `--strip-h` (34 px). The toolbar's hairline therefore sat
  2 px (Twilight Two) or 10 px (Cupertino) below the navigator's, and below
  the inspector's in Channels.
- **The strips only had a least height.** The dock headers, the pane's tab
  strip and the detail panel's bar each took `min_h(strip_height)` and grew
  with their tallest control. At the Medium icon-button size and above, a
  header with an icon button (the inspector's Close, Channels' Join) grew
  while a header without one did not, so their hairlines parted.
- **GPUI Kit's first tab keeps its leading line.** Kit's
  `TabVariant::Tab` draws a 1 px line on both sides of the selected tab. It
  means to drop the first tab's leading line when the bar has no prefix,
  but its `TabBar` always marks each tab as having one
  (`tab_bar_prefix.unwrap_or(true)` in `gpui-component` 0.9.1), so the open
  first tab drew its line beside the navigator's. Kit paints borders after
  children, so no layer of ours could cover it.
- **Banded list groups drew a line above and below.** With no row inset
  (One Dark and other themes whose `--row-inset` is 0), a group header drew
  a top line as well as its bottom one. The top line sat against the
  toolbar's line for the first group, and against the hairline under the
  previous row for every later group.

### Changes

- `workspace::strip_height(window, cx)`: `--strip-h`, or the tallest
  button a strip holds (toolbar, action or compact role, read with the new crate-private
  `icon_button::role_target`), with 3 px for its focus ring above and below
  and the hairline, when the theme or the icon-button size makes that
  taller. The navigator and inspector headers, the pane's tab strip, the pane's tabs
  and the detail panel's bar (`panel::frame`) are exactly that high.
- `workspace::Toolbar::strip()`: a toolbar heading a pane that has no tab
  strip is the pane's strip, exactly `strip_height` high, its items on one
  line and centred. Projects, Channels and the Issues screens that stand in
  for the list (with the navigator hidden) use it. Its leading items' clip
  spans the strip's height and reaches a focus ring's width (Kit's
  `FOCUS_RING_WIDTH`, 3 px) past the text edge, so focus rings show.
- Projects' leading controls (the initiative, Overview, Milestones and
  Back) go through the shared `OverflowToolbar`, as Channels' actions and
  the GitHub Copilot agents filters do: in a narrow pane or at a large
  icon-button size the last of them move into its menu, and a hidden
  initiative picker opens from the chevron. A strip toolbar on its own no
  longer wraps, so before this the last controls were cut off.
- A strip toolbar's two sides share its one line: the leading side keeps
  room for an overflow menu's button (`Metrics::overflow_width` and a
  focus ring each side) while it holds anything, and the trailing side
  yields the rest of the line, clipped with ring room like the leading
  side. Before this the trailing side never shrank, so after main's
  #170 added Refresh to the Channels and Projects toolbars, a narrow pane
  at a large icon-button size left the leading side no width at all.
- `OverflowToolbar` tells a row measured with no width from one not yet
  measured. It used to show every unit whenever its width was 0, so a
  leading side squeezed to nothing drew its pickers clipped instead of
  moving them into the menu.
- Channels' Refresh is the last unit of its trailing `OverflowToolbar`
  (with Find and More), and Projects' page action (New milestone or Edit
  project) and Refresh form a trailing `OverflowToolbar` of their own, so
  both yield into a menu rather than being cut off. Both used the new
  `OverflowUnit::refresh`, the quiet Refresh button and its menu entry,
  since replaced by the general `OverflowUnit::icon_button`
  ([Review follow-ups](../review-follow-ups/README.md)).
- Channels' leading pickers (the conversation menu while the navigator is
  hidden, and the initiative picker) go through an `OverflowToolbar` the
  same way. The conversation menu's lines become the overflow menu's
  entries; the passive heading after them takes only the room left, so it
  truncates before a picker moves into the menu.
- `OverflowToolbar` clips on a frame a focus ring's width (3 px) larger
  than its row on every side, held out of the layout by a negative margin,
  so its units' focus rings show. The row keeps its bounds, which are the
  width it plans with.
- `OverflowToolbar` dismisses an open overflow menu when everything fits
  again, as after a resize. The chevron renders that menu, so before this
  it stayed open unseen.
- `tabs::pane_tab_bar`: the first tab of a bar without a prefix sits one
  hairline to the leading side, so the bar clips its leading line and the
  dock or window edge beside it keeps the only line there.
- Banded list group headers draw only the line under them.
- `workspace::pane_head` keeps its signature: its tab strip is a private
  element that reads `strip_height` when it renders.
- The design guide's rule 3 is now **Strips line up, exactly**, and the
  new rule 4 is **One line per edge**; both are in the reviewers'
  checklist, the board checklist, the component catalogue and the README.
  The anatomy image does not change: it shows the same zones, and its
  strips already line up.
- The Component gallery's `workspace::Toolbar` card shows a strip toolbar.

### Not changed

- Strips lower in a page, such as the Issues decision and attachment
  panels' headers, the GitHub diff view's header and file tree, and the
  Actions graph's toolbar, sit under the pane's toolbar, not beside a dock
  header, so the strip rule does not reach them.
- When `OverflowToolbar` dismisses its menu because everything fits
  again, the anchor returns focus to the chevron's handle, which no longer
  renders. [Review follow-ups](../review-follow-ups/README.md) (#164) moves
  focus to the toolbar's own container instead.
- The Projects toolbar shows Overview and Milestones as action buttons,
  the open one as `Intent::Primary`. With **New milestone** also primary,
  the Milestones view has two primary buttons, against the one-primary
  rule. That is outside this change; the shared `segmented::control`, or
  tabs for the open project, would fix it.

## Delivery

| Defect | Where | Fix | Test |
| --- | --- | --- | --- |
| Projects' toolbar 2 px below the navigator's header | `subspace-desktop/spaces/projects/src/view.rs` | `Toolbar::strip` | `view::tests::the_toolbar_heading_the_pane_ends_level_with_the_navigators_header` |
| Channels' toolbar below both docks' headers | `subspace-desktop/spaces/irc/src/view/frame.rs` | `Toolbar::strip` | `tests::initiatives::header::the_toolbar_leads_with_the_picker_and_trails_with_the_actions` |
| Issues' stand-in screens' toolbar taller than the strip | `subspace-desktop/spaces/issues/src/view/shell.rs` | `Toolbar::strip` | No test renders the stand-in screens; the shell's `workspace::strip_tests::a_toolbar_standing_in_for_the_tab_strip_ends_on_the_same_line` covers `Toolbar::strip` itself |
| Strips part at larger icon-button sizes | `board-kit/dashboard/src/workspace/header.rs`, `pane.rs`, `panel/toolbar.rs`, `tabs/paint.rs` | `strip_height`, exact heights | `workspace::strip_tests::larger_icon_buttons_grow_every_strip_together`, `workspace::strip_tests::the_detail_panel_bar_ends_level_with_the_tab_strip_at_a_large_size`, `workspace::tests::a_strip_grows_only_to_hold_its_tallest_button_with_its_focus_ring` |
| Review P2: a strip toolbar cut off Projects' last leading controls in a narrow pane | `subspace-desktop/spaces/projects/src/view/toolbar.rs` | Leading controls in an `OverflowToolbar` | `view::tests::a_narrow_pane_keeps_every_leading_control_reachable`, `view::tests::the_largest_icon_buttons_keep_every_leading_control_reachable` (pane at its least width, Small and Extra large: each control is wholly inside the toolbar's leading side or in the overflow menu; Back chosen from the menu leaves the milestone, and the initiative entry opens the picker) |
| Review P2/P3: `strip_height` allowed only for toolbar buttons | `board-kit/dashboard/src/workspace.rs`, `icon_button.rs` | The tallest of the toolbar, action and compact roles | `workspace::strip_tests::action_buttons_taller_than_toolbar_buttons_grow_every_strip_together`, `workspace::strip_tests::labelled_buttons_and_inspector_tabs_fit_their_strips_at_a_large_size` |
| After rebasing onto #170: Channels' narrow-pane tests failed, the leading side 6 px wide with its pickers still drawn in it | `board-kit/dashboard/src/overflow_toolbar.rs`, `workspace/pane.rs`; `subspace-desktop/spaces/irc/src/toolbar.rs`, `view/frame.rs`; `subspace-desktop/spaces/projects/src/view/actions.rs` | A measured zero width shows no unit; the strip's leading side keeps room for its menu button and the trailing side yields; Refresh and the page actions move into trailing overflow toolbars | `overflow_toolbar::runtime_tests::a_row_given_no_width_moves_every_unit_to_the_menu` and `workspace::crowded_strip_tests::a_crowded_strip_keeps_each_sides_menu_button_in_view` (each fails without its fix); `tests::initiatives::narrow::every_toolbar_control_stays_reachable_in_a_narrow_pane_at_the_largest_size` checks both sides of Channels' toolbar; `view::tests::the_largest_icon_buttons_keep_edit_project_and_refresh_reachable` checks both sides of Projects'; `overflow_toolbar::runtime_tests::refresh_runs_from_its_button_and_from_the_overflow_menu` (now `an_icon_button_runs_from_its_button_and_from_the_overflow_menu`) |
| Review P2: Channels' pickers could be clipped in a narrow pane | `subspace-desktop/spaces/irc/src/view/frame.rs`, `view/conversations.rs`, `initiatives/picker.rs` | Leading pickers in an `OverflowToolbar` | `tests::initiatives::narrow::every_toolbar_control_stays_reachable_in_a_narrow_pane_at_the_largest_size` (navigator hidden, inspector open, Extra large, pane at its least width: each picker is inside the leading side or in the overflow menu) |
| Review P3: an empty picker row put a gap before Channels' heading | `subspace-desktop/spaces/irc/src/view/frame.rs` | No picker row without a unit; a row that empties is dropped and its overflow menu closed | `tests::initiatives::header::without_pickers_the_heading_starts_at_the_text_edge` (fails without the fix: 4 px off the text edge) |
| Review P2: a chooser opened from the overflow menu stayed open when the initiatives changed | `subspace-desktop/spaces/irc/src/initiatives/picker.rs` | The stale check asks whether the chooser is open, wherever it is anchored, not whether it owns its host | `tests::initiatives::narrow::a_chooser_opened_from_the_overflow_menu_closes_when_the_catalogue_changes` (fails without the fix) |
| Review P2: `OverflowToolbar` clipped its units' focus rings | `board-kit/dashboard/src/overflow_toolbar/view.rs` | A clip frame with a ring's room on every side | `overflow_toolbar::runtime_tests::the_clip_leaves_a_focus_rings_room_round_the_row`; its sizing tests still pass unchanged |
| Review P2: an overflow menu stayed open unseen once everything fitted | `board-kit/dashboard/src/overflow_toolbar/view.rs` | Dismiss it when the chevron stops rendering | `overflow_toolbar::runtime_tests::an_open_overflow_menu_closes_when_everything_fits_again` (fails without the fix) |
| Review P3: focus rings clipped by a strip toolbar's leading items | `board-kit/dashboard/src/workspace/pane.rs` | Ring room in the clip and in `strip_height` | `action_buttons_taller_than_toolbar_buttons_grow_every_strip_together` checks 3 px above the button; the ring itself is not reported headlessly |
| The first pane tab's line beside the navigator's | `board-kit/dashboard/src/tabs/paint.rs`, `bar.rs` | The bar clips the first tab's leading line | `workspace::strip_tests::the_first_pane_tab_draws_no_line_of_its_own_at_the_dock_edge`; in Issues, `tests::workspace::the_strips_line_up_and_the_first_tab_leaves_the_dock_edge_to_the_navigator` |
| Banded group headers' doubled lines | `board-kit/dashboard/src/list/view/rows.rs`, `rows/band.rs` | No top line | `list::view::rows::band::tests::a_banded_group_header_draws_only_the_line_under_it` reads the header's border widths |

`workspace::strip_tests` also checks that the navigator header, the tab
strip and the inspector header end on one line
(`the_dock_headers_and_the_tab_strip_end_on_one_line`), and that a strip
toolbar does (`a_toolbar_standing_in_for_the_tab_strip_ends_on_the_same_line`).
Without the fixes the tests fail with the owner's numbers: a toolbar ending
at 36 px beside a header at 34 px, and the open tab's fill starting 1 px
inside the pane.

Before and after screenshots of Issues and Projects in Twilight Two, from
capture builds of `main` and of this change with the same fixture data and
window, are in the Subspace App's
`subspace-desktop/docs/changes/workspace-hairlines/evidence/`:
`issues-before-twilight-two.png`, `issues-after-twilight-two.png`,
`projects-before-twilight-two.png` and `projects-after-twilight-two.png`.
Checks are recorded in the pull request.

## Tasks

- [x] Find the root causes and survey every Space for both defects.
- [x] Fix them in the shared kit, and switch Projects, Channels and Issues'
      stand-in screens to `Toolbar::strip`.
- [x] Write the rules into the design guide, its checklist, the board
      checklist and the component catalogue.
- [x] Headless layout tests in the shell, Issues, Projects and Channels.
- [x] Component gallery card for `Toolbar::strip`.
- [x] Before and after screenshots of Issues and Projects from capture
      builds.
- [x] Review findings: Projects' leading controls overflow into a menu,
      `strip_height` allows for action and compact buttons and focus
      rings, the strip clip leaves room for rings, and tests for banded
      group headers and the detail panel at a large size.
- [ ] Channels with the IRC test server: covered by its headless test only.
- [x] Second review: Channels' pickers overflow into a menu, the overflow
      row's clip leaves room for focus rings, an unseen overflow menu
      closes, and the Projects tests choose entries from the menu.
- [x] Third review: no empty picker row before Channels' heading, and a
      stale chooser closes wherever it is anchored.
- [x] After rebasing onto #170: both sides of a strip toolbar yield into
      overflow menus, a row with no width shows no unit, and the banded
      group header's look moved to `list/view/rows/band.rs` so `rows.rs`
      stays under the module length limit.
