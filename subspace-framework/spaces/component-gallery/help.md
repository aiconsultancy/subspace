# Component gallery guide

<!--
Evidence: spaces/component-gallery/src (catalog.rs, view.rs, browse.rs,
samples/), docs/ui-components.md and board-kit/dashboard/src.
The gallery uses fixture data only.
-->

## Overview

The Component gallery is a developer board. It shows every shared Subspace
component, every shell surface and every GPUI Kit component, each in the
states it can show. Use it to choose a component before you build one, to
compare sizes, variants and states, and to name a component in a request,
such as "use `action::icon_button`".

The rules for choosing and building components are in
`docs/ui-components.md`: a GPUI Kit component first, then a shared component
built from GPUI Kit components, and custom drawing last. Every shared
component and shell surface has a card here.

The board is in the Subspace App only. Dev builds enable it by default. Release
builds disable it by default: turn it on in **Settings > Boards** if you need
it. It uses fixture data only. It reads no configuration or credentials and
starts no network requests.

To open this guide, press `?` while board navigation has focus, or run **Help**
in the command palette.

## Before you start

The gallery needs no account, credential or configuration. In a release build,
turn it on in **Settings > Boards** first.

## Main view

The navigation on the left groups the sections by library, with the number
of cards in each:

- **Subspace components**: the shared components in
  `board-kit/dashboard`, by kind: buttons, choices, inputs, menus,
  dialogs, feedback, content, messaging, navigation and chrome, lists and
  tables. **Inconsistencies** lists hand-rolled controls in boards that do not
  use a shared component, beside the closest shared one.
- **Shell**: the surfaces the shell builds from shared components, such as
  the toolbar, status bar, Settings and the chat panel.
- **GPUI Kit**: every GPUI Kit component, grouped as GPUI Kit's documentation
  groups them: basic, form, layout and advanced components. The group label
  says how many of them Subspace uses.

Choose a section to show its cards. The gallery remembers the section.

### Read a card

Each card has these parts:

- The **name** in monospace, with a copy button beside it. The button copies
  the exact name, such as `icon_button::IconButton` or
  `gpui_kit::component::button::Button`, and shows a check mark. Shared
  names are relative to `subspace_dashboard`.
- For a GPUI Kit component, **Used in Subspace** or **Not used in Subspace**.
  A test keeps this mark in step with the code outside the gallery.
- The **source** file that owns the component.
- A **note** that says where Subspace uses it.
- For a GPUI Kit component that Subspace wraps, **In Subspace, use** with a
  button that shows the shared component's card. Use that instead.
- One **sample** for each state, with the state under it. The samples are
  real controls: hover, focus, clicks, typing and menus work, and choices you
  make in a sample stay while the app runs.

### Shell surfaces

Boards cannot draw the shell's own surfaces, so a shell card lists the cards
of the components the surface is built from; click one to show it. Where the
shell has a command for the surface, **Open** opens the real one.

## Toolbar and filters

- **Hide unused** hides the GPUI Kit components that Subspace does not use.
  It is off by default, so every component shows, and the gallery remembers
  it. The status bar shows how many GPUI Kit components Subspace uses.
- **Filter by name**, at the top of the navigation, shows the cards from every
  section whose name, source or GPUI Kit type contains every word you type,
  such as `button` or `ButtonGroup`. It shows at most 40 cards; add words to
  narrow it. Clear the field to return to the chosen section.

## Details

The gallery has no detail panel, and it does not show `panel::frame`: its
resize handle saves the app-wide panel width. Open a detail panel in a list
board to see it. The `list::render` sample opens no details for the same
reason.

## Actions

- Copy a name with the copy button beside it.
- Press `/` and type a component name to search the cards. Choosing one shows
  its section and scrolls to its card, even if **Hide unused** would hide it;
  the gallery then shows unused components again.
- Clicking a sample does nothing outside the gallery, with these exceptions:
  a toast and GPUI Kit's notifications show in the window; dialogs and sheets
  open over the window; the native menu sample shows a system menu; link
  samples open `https://example.com/component-gallery` in the browser; a
  shell card's **Open** opens that surface; and **Choose…** in the path input
  opens the file picker without reading the file.

### Add a card

1. Add the item to its section's `ITEMS` in
   `spaces/component-gallery/src/samples/`. Use the name from code. For a
   GPUI Kit component, add it under `samples/kit/` with its paths, whether
   Subspace uses it and the shared treatment to prefer.
2. Add its samples. Call the public constructor that boards call. Do not
   restyle it. Keep a sample's entities in the section's retained state and
   its controlled values in the gallery's `Values`.
3. Add the name to the gallery names table in `docs/ui-components.md`.

The tests check that each name is unique, that every card renders, that the
catalogue and the gallery list the same names, and that each GPUI Kit card's
used mark matches the source.

The opt-in [Linux offscreen proof](tests/README.md) captures actual catalogue
cards through GPUI's test platform and WGPU renderer. It uses isolated theme
fixtures and does not launch an installed application.

## Keyboard shortcuts

`/` opens the item search for the gallery. The shared app keys and the native
keys of each sample control apply.

## Settings

The board has no settings of its own. **Settings > Boards** turns it on or off.

## Troubleshooting

If the board is not in the switcher, turn it on in **Settings > Boards**.

If a GPUI Kit card is missing, check whether **Hide unused** is on.

## Limits

The gallery shows fixture data only. A state that needs live data, such as an
open popup, shows when you use the sample. GPUI Kit's speech button shows as
unsupported, because Subspace builds GPUI Kit without speech. The title bar
sample is covered so that dragging it cannot move the window.
