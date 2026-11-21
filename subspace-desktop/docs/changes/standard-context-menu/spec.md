# Spec: the standard right-click menu

See the [intent](intent.md), [tasks](tasks.md) and [delivery](delivery.md).
Back to [change records](../README.md).

## The groups

Every menu built here has seven groups, always in this order, with a
separator between groups that have commands. A group with nothing in it is
left out, and a Space fills groups in without ordering them.

| Group | Holds |
| --- | --- |
| Open | The default action first, showing its key, then the item's page elsewhere. |
| Act | The item's own verbs, with submenus for choices. |
| Copy | The selected text, then the item's facts. |
| Chat | **Add to chat** and **Add selection to chat**. |
| View | The container's commands, such as folding a group. |
| Remove | Commands that remove or leave something. |
| Place | Settings for the place, such as **Space settings…**. |

## Commands

- Each command has a group, a label, a Lucide glyph and what it runs: a
  click, an action (which also shows its bound key) or a submenu.
- A command that cannot run now stays in its place, disabled, with one line
  under it saying why.
- An enabled Remove command's glyph takes the theme's danger colour; a
  disabled one stays muted like any disabled command. The labels keep
  the menu's text colour, so each stays its item's accessible name in GPUI
  Kit's native menu.

## The Framework

`subspace_dashboard::row_menu` owns the menu:

- `MenuGroup`, `MenuCommand` and `GroupedMenu` describe a menu, and `build`
  turns it into GPUI Kit's `PopupMenu`, shown through
  `anchored_context_menu`. `outline` lists the lines without building them,
  for tests.
- `RowMenu` holds the facts a row already has (title, id, a link and its
  service, whether it opens, whether the chat is offered).
  `row_menu_items` and `panel_menu_items` decide the standard commands from
  them, and `grouped` turns those into commands: **Open** (Enter), **Open
  in &lt;service&gt;**, **Copy**, **Copy link**, **Copy title**, **Copy id**,
  **Add to chat** and **Add selection to chat**.
- The default action is now labelled **Open**; it was **Open details**.
- A shared list row or group header selects its line first, then shows the
  standard commands, the board's own from `ListDashboard::row_commands`,
  and **Fold group** or **Unfold group** on a header. What the older
  `extend_row_menu` and `row_submenus` hooks add lands at the end of Act.
- `workspace::NavRow::menu` gives a navigator row the same menu; a
  right-click runs the row's click first.
- The detail panel's menu uses `panel_menu_items`, which never offers to
  open the panel it is in.

## Where the Spaces put their commands

| Place | Commands |
| --- | --- |
| Space tab and navigation rail destination | **Close Space** (Remove), disabled with "The last open Space can't be closed." for the last one; **Space settings…** (Place) |
| Channels rows | **Open to the right** and **Open below** (Open); **Join** and **Refresh channel list** (Act); **Leave channel**, **Remove from list** or **Close conversation** (Remove) |
| Pull requests rows and repository headers | **Add `owner/name` to favourites** or **Remove … from favourites** (Act) |
| Actions repository headers | **Refresh repository** (Act), disabled until GitHub is signed in |
| Every shared list | The standard Open, Copy and Chat commands its `RowMenu` earns |

## Tests

- The Framework: group order, separators, left-out groups, disabled
  reasons, the danger glyph, and which standard commands a row or the panel
  earns.
- The host app: the Space tab's menu.
- Channels and Pull requests: their own commands land in their groups.
