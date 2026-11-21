# Spec: add a Space from the title bar

See the [intent](intent.md), [delivery](delivery.md) and [tasks](tasks.md).
It mirrors [closing a Space from its tab](../close-space-from-tab/spec.md)
and follows the same rules.

## The plus

The plus is the shared ghost `IconButton` with the Plus glyph, the tooltip
and accessible name **Add a Space**, as the collapsed toolbar menu's button
is built. It opens a native menu through `anchored::dropdown_menu`, so the
shared anchored host owns placement, Escape, outside dismissal and focus.
While the menu is open it covers the board area, as a tab's menu does, so a
web Space's native view does not draw over it.

- **Title bar.** The plus is the tab strip's last `OverflowToolbar` unit, so
  it sits right after the last tab. When the strip runs out of room it is
  the first unit to move into the chevron menu, as that menu's last entry,
  **Add a Space…**; choosing it replaces the chevron menu with the add menu
  on the same anchor.
- **Navigation rail.** The plus ends the rail's list of destinations and
  scrolls with it.

## The menu

The menu is built when it opens, from the board set then:

1. The label "Add a Space".
2. Each Space the product includes that has no tab, in switcher order: its
   glyph, its title and its board's `DESCRIPTION` on a second, muted line.
   The row carries the title as its accessible name. With no Space off, the
   quiet line "Every Space is on." instead.
3. A separator, then **Manage Spaces…**.

The product has no preview mark for a Space (Settings and the tabs show
none), so the menu shows none.

**Manage Spaces…** opens Settings in its Spaces group, at the first Space
that is off, or at the first Space when every one is on. Settings has no
page of its own for the Spaces group; each Space's page holds its
**Enabled** switch.

## Adding

Choosing a Space calls `settings::board_choice::save_board_enabled` with
`true`, the function **Close Space** uses with `false`. It writes and
applies exactly what the Space's **Enabled** switch and **Save** do: the
choice from Save's own rule (a choice equal to the product default removes
the `boards` entry, otherwise it writes `true`), only the `boards` key, the
same validation and external-change check, the same mirror into the running
config and the same `registry::apply_running_config` rebuild.

When the rebuild succeeds the shell reconciles its tabs at once and switches
to the new Space through the normal switch (`switch_to`), which takes it out
from behind Logs or Search. The notices:

- "Opened Actions."
- "Restart the app to open Actions." when the board set could not be
  rebuilt; the choice is saved.
- "Couldn't open Actions: <reason>" when the write failed; nothing changes.

Turning a Space on can never leave no Space on, so the close path's
last-Space guard has no counterpart. Adding a Space that already has a tab
does nothing.

## Closing points here

The notice after **Close Space** now reads "Closed Actions. Add it again
from the plus in the title bar."

## Palette

**Add a Space…** in the command palette opens a palette list with the same
choices: each Space that is off, with its description as the hint, then
**Manage Spaces…**. Its placeholder says "Every Space is on." when there is
nothing to add. The palette cannot open the native menu itself, because the
plus may be hidden in the chevron menu or the rail; the list runs the same
functions.
