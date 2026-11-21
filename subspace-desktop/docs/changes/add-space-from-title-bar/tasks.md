# Tasks: add a Space from the title bar

See the [spec](spec.md) and [delivery](delivery.md).

- [x] Pure logic in `app/add_space.rs`: the Spaces offered (off, in product
      order), the menu's rows and the every-Space-on state, the three
      notices, the Manage Spaces target and the palette list, with unit
      tests.
- [x] `open_space` and `manage_spaces` on the shell, through
      `board_choice::save_board_enabled`.
- [x] The plus and its menu (`app/add_space/menu.rs`): the tab strip's last
      overflow unit with its chevron entry, and the end of the navigation
      rail.
- [x] The palette command **Add a Space…**.
- [x] The closed notice points to the plus.
- [x] Runtime tests (`app/space_menu_tests/add_space.rs`): adding brings the
      tab back and switches to it, the saved config matches the Settings
      switch and Save, a Space already on is left alone, the every-Space-on
      line, choosing from the menu, and **Manage Spaces…**.
- [x] Docs: the UI component catalogue, the design guide, the Framework's
      features page, the Settings guide, the product Settings and boards
      pages, and the close-from-tab record.
- [x] Component gallery: `shell::toolbar` and `shell::navigation_rail` link
      `anchored::dropdown_menu` (and the rail `icon_button::IconButton`).
- [x] Build, and the unit and headless tests with
      `cargo test --workspace --locked --no-fail-fast` in both projects.
- [x] `make check` in both projects and `mise run quality` in both, on the
      final tree (21 November 2026): all passed.
- [ ] Capture screenshots in Twilight Two (see [delivery](delivery.md)).
      Not yet taken. Update the shell anatomy image if the title bar's
      appearance needs it.
- [ ] Both model reviews clear the final head. Not run.
