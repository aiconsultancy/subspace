# Tasks: the standard right-click menu

See the [intent](intent.md), [spec](spec.md) and [delivery](delivery.md).
A box is ticked only when the work has been built and its tests have passed.

- [x] `row_menu`: `MenuGroup`, `MenuCommand`, `GroupedMenu`, `build` and
      `outline`, with the standard commands in their groups and **Open** in
      place of **Open details**, with unit tests.
- [x] Shared list rows and group headers: select first, then the standard
      groups, `ListDashboard::row_commands` and Fold or Unfold.
- [x] The detail panel's menu in the standard groups.
- [x] `workspace::NavRow::menu`.
- [x] The Space tab and navigation rail menu: **Close Space** in Remove,
      **Space settings…** in Place.
- [x] Channels, Pull requests and Actions commands moved into their groups.
- [x] Component gallery sample and the catalogue row in
      [UI components](../../../../subspace-framework/docs/ui-components.md);
      the design guide's
      [Context menus](../../../../subspace-framework/docs/design-guide.md#context-menus).
- [x] Unit and headless tests, with
      `cargo test --workspace --locked --no-fail-fast` in both projects.
- [x] `make check` in both projects and `mise run quality` in both, on the
      final tree (21 November 2026): all passed.
- [ ] Captures in Twilight Two of a list row's menu, a group header's menu,
      a Remove command and the Space tab's menu, under `evidence/`. Not yet
      taken.
- [ ] Both model reviews clear the final head. Not run.
- [ ] The gaps in [delivery](delivery.md#known-gaps) fixed, or each recorded
      with the reason it stays.
