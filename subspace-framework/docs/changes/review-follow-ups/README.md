# Review follow-ups

## Intent

The reviews of #160, #162 and #206 left follow-ups, five of them filed as
issues. Resolve them together: the shared overflow toolbar returns focus to
something that is drawn, scrolls a long menu, offers a general
icon-button unit and one meaning of zero width; both sides
of a strip toolbar keep room for their menu buttons; pane tabs keep to the
strip's height; Channels and Projects share their initiative unit and close
their menus when the toolbar goes; the menu bar tests check every command.

This change closes #164, #165, #167, #168 and #174.

## Spec

### Overflow toolbar (`board-kit/dashboard/src/overflow_toolbar.rs`)

- **Focus after a widen (#164).** When the menu closes because every unit
  fits again and the chevron held focus, focus moves to the toolbar itself,
  so Tab goes on to its first control. The toolbar's clip frame, which holds
  the units and the chevron, has the toolbar role, the name **Toolbar** and
  the toolbar's focus handle. The handle is no tab stop, and a press on
  blank space in the toolbar does not take focus, as GPUI Base's `Toolbar`
  does. A first version put the role and name on a zero-size focus target
  beside the units, which every toolbar exposed as an empty toolbar (review
  of #212).
- **A long menu scrolls (#167).** The overflow menu is GPUI Kit's
  scrollable `PopupMenu`, at most half the window high (and at most
  28.125 rem). Channels' conversation menu, shown while the navigator is
  hidden, is scrollable the same way. The tests scroll each menu to its
  end and choose the last entry, so a menu that only clipped its rows would
  fail them.
- **`OverflowUnit::icon_button(id, icon, label, on_click)` (#174 a).** A quiet
  glyph button inline and a menu entry with the same glyph and label, both
  running `on_click`. It replaces the Refresh-only `OverflowUnit::refresh`.
  Channels and Projects use it for Refresh. Channels' Find stays an
  `OverflowUnit::inline`: its button keeps focus on click
  (`focus_on_click`), which a general unit should not impose.
- **One meaning of zero width (#174 b).** `plan_overflow` takes the row's
  width as an `Option` and handles it all: `None` (not yet measured) shows
  every unit, a measured 0 shows none. `OverflowToolbar::plan` only passes
  its state on.
- **One unmeasured width (#174 d).** `OverflowUnit::UNMEASURED_WIDTH`
  replaces the copies in Channels, Projects, GitHub Copilot agents, the
  Component gallery and the toolbar itself.
- `OverflowToolbar::is_open` says whether its menu, or a chooser opened from
  it, is open.

### Strip toolbar (`board-kit/dashboard/src/workspace/pane.rs`)

- **Trailing room (#174 c).** The trailing side keeps the same least width
  as the leading side while it holds anything: its menu button's width and
  a focus ring each side. Its items end at its trailing edge. A strip too
  narrow for both menu buttons clips the trailing side instead of letting
  it run past the pane. Trailing content must shrink, as an overflow
  toolbar does; content that cannot is clipped from its start, as
  `Toolbar::strip` says.
- **Gaps in the room (review of #162).** Each side's least width adds the
  gaps between its items, so a heading that collapses beside an overflow
  toolbar, as Channels' does after its pickers, no longer takes its gap out
  of the menu button's room.

### Pane tabs (`board-kit/dashboard/src/tabs/paint.rs`)

- **Strip height (review of #162).** A pane tab bar's tabs are exactly the
  strip's height at every bar size. GPUI Kit sets a large tab 36 px high
  after our styles, so the tab is held to the strip with a least and a
  greatest height; before, a `.large()` pane bar's tabs stood 2 px past a
  34 px strip.

### Spaces (`subspace-desktop`)

- **Shared initiative unit (#165).** `Catalog::overflow_unit(key, disabled,
  button, open)` in `subspace-api-client` builds the picker's unit: the
  Space's button inline and an **Initiative: name…** entry in the menu.
  Channels passes `disabled` as false and Projects passes whether editing
  holds the toolbar.
- **Menus close with the toolbar (#168).** When Channels' pane shows a
  stand-in in place of its toolbar (stopped, closed, checking, unreachable
  or unconfigured), it closes both toolbars' overflow menus and the
  initiative chooser if any is open. `Dashboard::dismiss` and `deactivate`
  close the same menus through one `toolbar::close_menus`; before, `dismiss`
  left both toolbars' menus open.
- **Focus stays on something drawn (Claude Opus 5.5 review of #212).**
  Closing a menu returns focus to its button. When the pane swaps its
  toolbar for a stand-in, or the pickers' row empties and is dropped, that
  button is no longer drawn, so focus that was within the board, on a
  toolbar control or in a menu, moves to the board's own focus handle.
- **Projects' overflow priority (review of #206).** The review asked that
  Projects' page action (New milestone or Edit project) overflow after
  Refresh, which comes before it. That cannot arise in any Space: the
  overflow menu's button is wider than Refresh at every icon-button size
  (1.75 rem against 1.5 rem at Small, up to 5.25 rem against 4.5 rem at
  Extra large), so whenever the page action fits beside the button,
  Refresh fits too. Hiding units by a priority distinct from display order
  is not added until a Space needs it; units still overflow from the end.

### Menu bar and docs

- `RichText::scrollable` in `board-kit/dashboard/src/rich/view.rs` loses
  its `qual:allow(test_quality, untested)`. rustqual matches calls by
  method name, so the new `PopupMenu::scrollable` calls, which the tests
  reach, count as covering it, and it flagged the suppression as stale
  (`ORPHAN_SUPPRESSION`).
- The menu bar tests check every action item of each layout against the
  command it runs, by the action's type: each Edit item's GPUI Kit action
  and its `OsAction`, and the Windows and Linux Full Screen item's
  `ToggleFullScreen`. They do not check that each action has a handler on
  the shell, which needs a running shell.
- Space Lab's help names the menu item **Help › Subspace Help**.
- The bundled Markdown editor's README already named GPUI Kit 0.11 and
  GPUI Fast after #207; its last paragraph said to move to the published
  editor once the Framework reached a compatible GPUI Kit, which it has
  (the editor's 0.4 releases target GPUI Kit 0.11). It now says so and
  leaves the swap to a separate change.

### Breaking changes

For a `!` commit of the Framework (`subspace-dashboard`):

- `OverflowUnit::refresh` is removed; use `OverflowUnit::icon_button`.
- `plan_overflow` takes the row's width as `Option<f32>`.

`Catalog::overflow_unit` is a new function in the desktop's
`subspace-api-client` and breaks nothing.

## Delivery

| Item | Where | Test (fails without its fix unless noted) |
| --- | --- | --- |
| #164 focus after a widen | `overflow_toolbar/view.rs` | `overflow_toolbar::runtime_tests::release::focus_moves_to_the_toolbar_when_its_menu_closes_because_everything_fits`, which also checks that the focused element is the toolbar's container, with the toolbar role and name, that it is the only toolbar node, and that Tab then reaches the first unit |
| #165 shared initiative unit | `subspace-desktop/clients/subspace-api/src/initiatives/picker.rs`, `spaces/irc/src/initiatives/picker.rs`, `spaces/projects/src/view/toolbar.rs` | A refactor: `tests::initiatives::narrow::a_chooser_opened_from_the_overflow_menu_closes_when_the_catalogue_changes` and Projects' `view::tests::the_largest_icon_buttons_keep_every_leading_control_reachable` choose the entry from the menu |
| #167 a long menu scrolls | `overflow_toolbar.rs`, `spaces/irc/src/view/conversations.rs` | `overflow_toolbar::runtime_tests::release::a_long_overflow_menu_scrolls_within_the_window` (40 units); Channels' `tests::conversation_menu::a_long_conversation_menu_scrolls_within_the_window` (40 channels in a 480 px window) |
| #168 menus close with the toolbar | `spaces/irc/src/view/frame.rs`, `spaces/irc/src/lib.rs` | `tests::initiatives::stand_in::losing_every_network_closes_the_{pickers,actions}_menu`, `losing_the_last_initiative_closes_the_{pickers,actions}_menu` and `dismissing_the_space_closes_the_{pickers,actions}_menu`; without the fix, `losing_the_last_initiative_closes_the_actions_menu` and `dismissing_the_space_closes_the_pickers_menu` pass too (the initiative chooser's anchors already close the pickers' menu on dismiss), the other four fail |
| Focus after a stand-in | `spaces/irc/src/view/frame.rs` | The same `stand_in` tests for losing every network or the last initiative check that a drawn element within the board holds focus; without the fix, three fail and `losing_the_last_initiative_closes_the_actions_menu` passes. Dropping the pickers' row has no test: no fixture empties it while the toolbar stays |
| #174 a icon-button unit | `overflow_toolbar.rs`; Channels' and Projects' Refresh | `overflow_toolbar::runtime_tests::an_icon_button_runs_from_its_button_and_from_the_overflow_menu` |
| #174 b zero width | `overflow_toolbar.rs` | `overflow_toolbar::tests::an_unmeasured_row_shows_every_unit_and_a_row_with_no_width_none`; `runtime_tests::a_row_given_no_width_moves_every_unit_to_the_menu` |
| #174 c trailing room | `workspace/pane.rs` | `workspace::crowded_strip_tests::a_strip_narrower_than_two_menu_buttons_keeps_each_sides_reserve`; `a_strip_with_room_for_one_menu_button_clips_the_trailing_side_at_its_edge` (the trailing menu button past the strip is not visible) |
| #174 d unmeasured width | `OverflowUnit::UNMEASURED_WIDTH` | A constant: builds |
| Projects' overflow priority | None | Cannot arise in any Space (the overflow button is wider than Refresh); not added until a Space needs it. Projects' `view::tests::the_largest_icon_buttons_keep_new_milestone_and_refresh_reachable` checks both stay reachable at Extra large in a narrow pane |
| Heading gap | `workspace/pane.rs` | `workspace::crowded_strip_tests::a_collapsed_heading_and_its_gap_leave_the_leading_menu_button_its_room`; Channels' `tests::initiatives::narrow::the_pickers_menu_button_keeps_its_room_beside_a_collapsed_heading`, each against the leading overflow toolbar's own row |
| Pane tabs at the strip's height | `tabs/paint.rs` | `workspace::strip_tests::a_large_pane_tab_bar_keeps_its_tabs_to_the_default_strip` |
| Menu bar commands | `host/app/src/app/menu_bar_tests.rs` | `every_mac_item_runs_its_own_command`, `every_windows_and_linux_item_runs_its_own_command`, and with Report issue `report_an_issue_runs_its_command` |
| Space Lab help | `subspace-desktop/spaces/space-lab/help.md` | Documentation |
| Markdown editor README | `board-kit/dashboard/vendor/markdown-editor/README.md` | Documentation |

The Component gallery's `overflow_toolbar::OverflowToolbar` card now leads
with Refresh as an `OverflowUnit::icon_button`. The design guide's rule 3
and the component catalogue's `overflow_toolbar` and `workspace` entries
say the same.

No screenshots: the visible changes are a scrollable menu, a pane tab bar
size no Space uses, and a strip's trailing side in panes narrower than any
Space reaches. The headless tests cover each.

## Tasks

- [x] Overflow toolbar: focus, scrolling menu, icon-button unit,
      `plan_overflow` taking the width's `Option`, `UNMEASURED_WIDTH`.
- [x] Strip toolbar: trailing room and gaps in each side's room.
- [x] Pane tabs at the strip's height.
- [x] Channels: shared initiative unit, menus closed with the toolbar and on
      dismiss, scrollable conversation menu.
- [x] Projects: shared initiative unit, Refresh as an icon-button unit.
- [x] Menu bar tests, Space Lab help, Markdown editor README.
- [x] Design guide, component catalogue, change records and the Component
      gallery card.
