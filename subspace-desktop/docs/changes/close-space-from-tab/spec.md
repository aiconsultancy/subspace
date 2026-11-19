# Spec: close a Space from its tab

See the [intent](intent.md), [delivery](delivery.md) and [tasks](tasks.md).

## The menu

Right-clicking a Space's switcher tab opens the native GPUI Kit context menu
(`anchored_context_menu`, the treatment the toolbar's **Show label** menu
uses) with two items:

- **Close Space** closes that Space at once. It takes no ellipsis because it
  needs no further input.
- **Space settings…** opens Settings at that Space's page, as the
  **Configure** recovery action does for the current Space.

The navigation rail's destinations have the same menu, so the command reads
and behaves the same in both navigation modes. The chevron overflow menu
lists tabs as menu rows and has no menu of its own.

For the last open Space, **Close Space** is disabled and the menu adds the
line "The last open Space can't be closed."

## Closing

Closing writes and applies exactly what the Space's **Enabled** switch and
**Save** do:

- The new choice comes from the same rule Save uses
  (`settings::form::switched_board_values`, which calls Save's own
  `merge_board_values`): a choice equal to the product default removes the
  `boards` entry; otherwise it writes `false`.
- `SettingsFile::save_board_choices` writes only the config file's `boards`
  key, through Save's own `write_choices`, with the same validation and
  external-change check. Other settings are left as they are, as the tab
  reorder's `save_board_order` does.
- The saved choices are mirrored into the running config as Save mirrors
  them, and `registry::apply_running_config` rebuilds the board set. Save's
  `apply_board_choice` now calls the same function
  (`settings::board_choice::apply_saved_boards`), so both paths remove the
  tab, stop its background work and drop its agent tools the same way. A
  board that keeps processes (`RETAIN_WHEN_DISABLED`) is parked as before.

When the Space was current, the tab before it becomes current and takes
focus through the normal switch (`switch_to`); the first tab hands over to
the next. While Logs or Search replaces the board, it stays shown and the
neighbour waits behind it.

A notice confirms the close and says where to reopen it:
"Closed Actions. Turn it on in Settings to reopen it." If the board set can't
be rebuilt, the choice is still saved and the notice says "Restart the app to
close Actions." A failed write shows "Couldn't close Actions: <reason>" and
changes nothing.

## Reopening

Reopening is unchanged: open Settings, choose the Space, turn on **Enabled**
and choose **Save**.

## Keyboard

The tabs are Tab stops, but GPUI Kit 0.9.1's `ContextMenu` opens only on a
secondary click or a long press; it has no keyboard trigger (such as
Shift+F10 or the Menu key). The menu itself is keyboard-navigable once open.
**Close Space** is not the only path to the command: Settings' **Enabled**
switch remains, as the Design Guides require for context-menu commands.
