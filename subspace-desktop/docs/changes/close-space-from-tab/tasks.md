# Tasks: close a Space from its tab

See the [spec](spec.md) and [delivery](delivery.md).

- [x] `SettingsFile::save_board_choices` writes the `boards` key alone,
      through Save's own writer.
- [x] `switched_board_values` derives one Space's saved choice by Save's
      `merge_board_values` rule.
- [x] `settings::board_choice` saves, mirrors and applies one Space's
      Enabled choice; Save's `apply_board_choice` shares its apply step.
- [x] Shell `space_menu`: **Close Space** and **Space settings…** on each
      switcher tab and rail destination; the last-Space guard and its reason;
      the neighbour hand-over; the notice.
- [x] `open_board_settings` opens Settings at a given Space; **Configure**
      uses it.
- [x] Tests: closing a background Space, closing the current Space (focus
      moves to the tab before it, or the next one for the first tab), the
      last-Space guard, the saved config matching the Settings switch and
      Save, **Space settings…**, the neighbour rule, the notice text and the
      core `boards`-only write.
- [x] Docs: the Settings guide, the product Settings and boards pages, the
      Framework's features page and UI component catalogue.
- [x] Capture screenshots.
- [x] `make check` in both projects and `mise run quality`.
