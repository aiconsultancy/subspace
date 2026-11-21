# Workspace shell review findings

## Intent

Pull request #29 brought in the workspace shell and Twilight Two and merged
with its review findings open. This follow-up takes each finding against
the Framework's shared kit (`board-kit`) and host app, checks it against the
current code, and fixes it or records why not. The findings are GPT-6.2
Sol's S1 to S11; Claude Opus 5.5 and the fleet review raised none against
the Framework.

## Spec

### Shared kit (`board-kit/dashboard`)

- **S1, row menus.** A right-click selects its line before the menu is
  built. When the board blocks navigation, as with a dialog open, and the
  selection did not move, **Open**, **Add to chat** and **Fold** are shown
  disabled with "Finish the open dialog first." and the board's own
  commands are left out, so the menu never acts on the previous selection.
- **S2, dock widths.** The navigator, inspector and detail slots shrink
  before the centre pane does. The pane keeps `PANE_MIN_WIDTH_PX` (320 px),
  or the whole width when the workspace is narrower than that, and nothing
  overflows the workspace.
- **S3, navigator keyboard.** Navigator rows are `Role::TreeItem` in a
  `Role::Tree` and say which is selected. The navigator is a Tab stop; `j`,
  `k`, Up and Down move between rows and Enter or Space chooses one. A click
  chooses a row without taking the keyboard focus, and keyboard focus shows
  the accent border.
- **S4, a menu on the selected row.** Choosing an unselected row before its
  menu opens stays the documented contract. A right-click on the row that is
  already selected no longer runs its click again.

### Host app

- **S6, Add a Space.** Each item in the plus menu is a `Role::Label` named
  by the Space's title (`aria_value`), so assistive technology reads the
  item itself.
- **S7, a Space added for after a restart.** When adding a Space needs a
  restart (`Applied::AfterRestart`), its id is saved under `app.dashboard`,
  so it is the current Space after the restart.
- **S8, zoomed Help.** While zoomed Help covers the Space, its docks are
  `NONE`: the dock switches are hidden and their actions ignored, so they
  cannot change the hidden Space.
- **S9, Twilight Two contrast.** Markdown colours are lightened, hue
  unchanged: `link_uri` from `#6F6D66` to `#939189`, `type` and `link_text`
  from `#A86D3B` to `#C1834E`, `title` from `#A76D3B` to `#C0834E`. On the
  darkest background they meet, the current line `#2b2a27`, they reach
  4.55, 4.53 and 4.51 to 1, above the 4.5 needed for normal text.
- **S11, the close notice.** With vertical navigation, closing a Space says
  to add it again "from the plus at the end of the navigation rail"; with
  horizontal navigation it still names the plus in the title bar.

### Not changed

- **S5, separators around an empty Act group (not a defect).** GPUI Kit
  0.9.1's `PopupMenu` skips a separator when the menu is empty or already
  ends with one, and drops a trailing one, so the drawn menu never shows two
  in a row.
- **S10, the design guide's checklist (not a defect).** It is the project's
  own screenshot checklist for people, linked from the agent guidance; it
  gives no instruction to an automated reviewer.

## Delivery

| ID | Severity | Location in #29 | Outcome | Test |
| --- | --- | --- | --- | --- |
| S1 | Medium | `board-kit/dashboard/src/list/menu.rs:27` | Fixed | `list::menu::tests`: `a_menu_selects_its_line_and_offers_the_boards_commands_for_it`, `a_blocked_board_keeps_its_selection_and_the_menu_never_acts_on_it`, `a_blocked_header_menu_shows_fold_disabled` |
| S2 | Medium | `board-kit/dashboard/src/workspace.rs:101` | Fixed | `workspace::layout_tests`: `the_docks_keep_their_widths_while_there_is_room`, `the_docks_yield_so_the_pane_keeps_its_least_width`, `a_workspace_narrower_than_the_pane_gives_the_pane_all_of_it` |
| S3 | Medium | `board-kit/dashboard/src/workspace/navigator.rs:287` | Fixed | `workspace::interaction_tests`: `rows_are_tree_items_that_say_which_is_selected`, `the_list_keys_move_between_rows_and_enter_or_space_chooses_one`, `a_click_chooses_a_row_without_taking_the_keyboard_focus` |
| S4 | Low | `board-kit/dashboard/src/workspace/row_parts.rs:40` | Fixed, narrowed | `workspace::interaction_tests`: `the_menu_of_the_selected_row_does_not_choose_it_again`, `the_menu_of_another_row_chooses_it_first` |
| S5 | Low | `board-kit/dashboard/src/list/menu.rs:62` | Not a defect | See above |
| S6 | Medium | `host/app/src/app/add_space/menu.rs:101` | Fixed | `space_menu_tests/add_space.rs`: `each_space_in_the_plus_menu_is_named_by_its_title`; `choosing_a_space_in_the_plus_menu_adds_it` updated |
| S7 | Medium | `host/app/src/app/add_space.rs:192` | Fixed | `space_menu_tests/add_space.rs`: `a_space_added_for_after_a_restart_is_current_after_the_restart` |
| S8 | Medium | `host/app/src/app/shell.rs:193` | Fixed | `toolbar/tests/status.rs`: `zoomed_help_hides_the_dock_switches_and_keeps_the_hidden_spaces_docks` |
| S9 | Medium | `host/app/themes/twilight-two.json:174` | Fixed | `theme.rs`: `twilight_two_syntax_colours_meet_normal_text_contrast` |
| S10 | Low | `docs/design-guide.md:360` | Not a defect | See above |
| S11 | Low | `host/app/src/app/space_menu.rs:83` | Fixed | `space_menu.rs`: `the_closed_toast_says_where_to_reopen_the_space`; `space_menu_tests.rs`: `the_closed_notice_points_to_the_rail_when_navigation_is_vertical` |

The [design guide](../../design-guide.md) now describes the dock widths,
the navigator's keyboard operation, a right-click on the selected row, the
blocked list menu and a Space added for after a restart.

Checks, screenshots and the findings outside the Framework are recorded in
the Subspace App's change record,
`subspace-desktop/docs/changes/workspace-shell-review-findings/delivery.md`.

## Tasks

- [x] S1 to S4 in the shared kit, each with a headless test.
- [x] S6 to S9 and S11 in the host app, each with a test.
- [x] S5 and S10 checked and recorded as not defects.
- [x] Build, test, `make check` and `mise run quality` in the Framework
      (recorded in the Subspace App's record).
