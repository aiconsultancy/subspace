# Delivery: leave Board Lab and the Component gallery out of releases

See the [spec](spec.md) and [tasks](tasks.md).

## State on 21 November 2026

The manifests, registrations, descriptor, tests, check tasks, `make
run-unreleased`, the interface script and the documentation are edited on
the working tree. On the final working tree, on 21 November 2026, `make check`
passed in the desktop and in the Framework, `mise run quality` passed in the
desktop, `make interfaces` passed with the default board list, and
`make capture` from the final build showed neither unreleased Space in the
captures. Not yet run: Board Lab's optional interfaces checks, the terminal
check and `make run-unreleased` by hand.

## Checks

Passed on 21 November 2026, in `subspace-desktop/` unless stated:

In `subspace-desktop/`:

- [x] `make check` (exit 0). Its lint and test steps ran in both
      configurations: the workspace with
      `--features subspace/unreleased-spaces`, and the default product
      (`-p subspace -p subspace-descriptor`) without Board Lab or the
      Component gallery.
- [x] `mise run quality` (exit 0).
- [x] `make interfaces` (exit 0): the default board list without either
      Space.
- [x] `make capture OUT=<dir>`, from the final build: the title bar shows
      only Pull requests, Channels, Issues and Decisions, so neither Space
      appears in the captures.
- [x] In the Framework, `make check` (exit 0): the gallery crate and the
      facade are unchanged in code; only the gallery's guide changed.

Not run:

- [ ] On macOS ARM64, `SUBSPACE_FIXTURE_BOARD_LAB=1 make interfaces`: Board
      Lab's optional checks with `--features board-lab`.
- [ ] `MISE_ENV=terminal make check-terminal`.
- [ ] `make run-unreleased` checked by hand: both Spaces in Settings and in
      **Add a Space**, both off until turned on; `make run`: neither appears
      anywhere.

Run and not passing, and not caused by this change: `mise run audit` fails
on nine advisories published against `heapless` 0.9.3 (RUSTSEC-2026-0574 to
RUSTSEC-2026-0582, all "unsound"). `main` depends on the same version, and
the branch adds no third-party crate, so `main` fails the audit too.

No UI changes in a Space's own drawing, so no new screenshots are needed
beyond confirming that neither Space appears in a default build.
