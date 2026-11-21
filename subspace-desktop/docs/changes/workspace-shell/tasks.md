# Tasks: workspace shell

See the [intent](intent.md) and [spec](spec.md). Section numbers refer to the
spec. A box is ticked only when the work has been built and its tests have
passed; [delivery](delivery.md) says how each was checked.

## Framework

- [x] `dashboard::workspace`: `Workspace`, `Navigator`, `NavRow`,
      `nav_section`, `Pane`, `Toolbar`, `Inspector`, `section`, `fact`,
      `text_edge` and the dock width rules (sections 1 to 3).
- [x] `NavRow::menu`, `Workspace::detail` and `pane_head` (section 2).
- [x] Density tokens `--strip-h`, `--nav-w` and `--inspector-w`, in the theme
      designer's Window group.
- [x] Component gallery cards and the catalogue rows.
- [x] Layout guidance in the Framework's board integration page and board
      checklist, and in the desktop's `AGENTS.md`; the
      [design guide](../../../../subspace-framework/docs/design-guide.md)
      with the shell anatomy image.

## Spaces

- [x] Issues: navigator, tab strip, toolbar, peek, issue page inspector, and
      the click, double click and Open behaviour (sections 4 and 5).
- [x] Issues page restyle: property rows, workflow bar, path toolbar,
      hover-quiet pencils, one-line comment box (section 5).
- [x] Channels, and its content restyle (sections 4 and 5b).
- [x] Pull requests: inspector, opened tab and repository filter (sections 4
      and 5a).
- [x] Actions: inspector and opened tab (sections 4 and 5a).
- [x] Copilot Agents: inspector and opened tab (sections 4 and 5a).
- [x] Board Lab (section 4).
- [x] Web Spaces (section 4).
- [x] The restyle to the mock-ups (section 2a): accent Space name, section
      labels in capitals, full-height pane tabs with the item's key in mono,
      plain filter buttons, the borderless toolbar search, one primary
      action, and the Issues toolbar's contents with the hidden sub-issue
      count in the status bar.
- [x] The dock switches in the status bar: `Dashboard::docks` and
      `Dashboard::toggle_dock`, the status bar's switches, and each Space
      wired to them (section 3).
- [x] The Twilight Two theme, the mock-ups' colours and sizes (section 2a).
- [x] Inter and JetBrains Mono embedded for Twilight Two; see the
      [bundled fonts record](../bundled-fonts/README.md).
- [ ] Redraw the shell anatomy image from the owner's artboard, with the
      switches in the status bar.

## Gates

- [x] Unit and headless tests in both projects, with plain
      `cargo test --workspace --locked --no-fail-fast` on the working tree;
      the counts are in [delivery](delivery.md#tests).
- [x] `make check` in the Framework, then in the desktop, on the final tree
      (21 November 2026, exit 0 in both).
- [x] `mise run quality` in both, on the final tree (21 November 2026): the
      Framework with no new findings against the baseline, the desktop
      exit 0.
- [x] `make interfaces` in the desktop (21 November 2026, exit 0): the
      default board list, without Board Lab or the Component gallery.
- [ ] `SUBSPACE_FIXTURE_BOARD_LAB=1 make interfaces`. Not run.
- [ ] `MISE_ENV=terminal make check-terminal`. Not run.
- [ ] `mise run audit`. Run on 21 November 2026 and fails on nine
      advisories against `heapless` 0.9.3 (RUSTSEC-2026-0574 to
      RUSTSEC-2026-0582, all "unsound"); `main` depends on the same version
      and this branch adds no third-party crate, so `main` fails it too.
- [x] Captures in Twilight Two (`CAPTURE_THEME="Twilight Two"`) of the
      Issues list, the issue page, Pull requests and Decisions, and of the
      Issues list in Cupertino Dark, under `evidence/`, retaken from the
      final build on 21 November 2026.
- [ ] Before captures from a build of `main`, and captures of Channels,
      Actions, Copilot Agents, Board Lab, a web Space, the right-click menus
      and the Add a Space menu. Not yet taken.
- [x] The captures compared with the owner's mock-ups and the design
      guide's checklist; the differences are in
      [delivery](delivery.md#known-gaps).
- [ ] The captures compared with the shell anatomy image, once it is
      redrawn.
- [ ] Both model reviews (Claude Opus 5.5 and GPT-6.2 Sol, high effort)
      clear the final head. Not run.
- [ ] The known gaps in [delivery](delivery.md#known-gaps) fixed, or each
      recorded with the reason it stays.
