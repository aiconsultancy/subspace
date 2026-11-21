# Delivery: rename Board Lab to Space Lab

See the [spec](spec.md) and [tasks](tasks.md).

## State on 21 November 2026

The rename is edited on the working tree. What was renamed:

- The folder `spaces/space-lab`, its crate `subspace-space-lab-board` and
  the registered type `SpaceLab`, with the support crate
  `spaces/space-lab/crates/mxc-worker` moved with it.
- The app's and the descriptor's `space-lab` Cargo features, and
  `unreleased-spaces`.
- The board id `space-lab`, the display name, the navigator preference, the
  saved choice and the data folder.
- The six agent, MCP and CLI tools, `space_lab_*`, and their CLI commands.
- The `space-lab-examples` mise task, the interfaces fixture variable
  `SUBSPACE_FIXTURE_SPACE_LAB`, scripts, tests and fixtures.
- The Component gallery's sample for the Space's row actions and its
  evidence images.
- The documentation: the Space's guide, `HELP.txt` and help-image notes, the
  developer page `docs/space-lab.md`, the design evidence under
  `docs/agents/space-lab/`, the README, `AGENTS.md`, the documentation index,
  the tables of contents, the boards page, the Spaces audit, the glossary,
  the installation page, and the Framework's UI components, MCP and board
  integration pages.
- `CAPTURE_FEATURES` in `scripts/capture.sh`, documented in
  [Desktop capture](../../capture.md).

Historical change records are unchanged apart from a one-line pointer here
in the [unreleased Spaces](../unreleased-spaces/intent.md) and
[workspace shell](../workspace-shell/intent.md) intents.

## Validation

Run on 21 November 2026 on macOS ARM64, on the uncommitted working tree
based on `main` at `40b9ebd`:

- `make check` in the desktop: passed. Clippy and the tests ran on the
  workspace with `subspace/unreleased-spaces` (Space Lab built in, its crate
  202 tests passing) and then on `subspace` and `subspace-descriptor` alone
  in the release configuration. The run rebuilt the Component gallery's
  `dist/` from source; only the renamed strings and the hashed chunk names
  changed.
- `make check` in the Framework: passed.
- `mise run quality` in the desktop and in the Framework: passed with no
  regression. The desktop reports one finding against its baseline, a stale
  `qual:allow` in `products/subspace/build.rs`; the same finding is reported
  on `main` without this change, so it is not caused by the rename.
- `make interfaces`: passed, without Space Lab.
- `SUBSPACE_FIXTURE_SPACE_LAB=1 make interfaces`: passed, with Space Lab
  built in and turned on.

Not run: `make run-unreleased` by hand (the captures stand in for Settings;
**Add a Space** was not checked), `MISE_ENV=terminal make check-terminal`
(no terminal crate changed), and the model reviews.

The captures show a red "File exists (os error 17)" message in Space Lab's
welcome page on a fresh profile. It comes from the project store creating
its `projects` folder at start-up (`project::disk::directory` checks for the
folder, then creates it without tolerating one made in the meantime), code
this change does not alter. It was not reproduced on `main`, which has no way
to capture with the Space built in; it is left for a separate fix.

## Evidence

The captures, in Twilight Two from a capture build with
`CAPTURE_FEATURES=space-lab` and Space Lab turned on through
`CAPTURE_CONFIG` on macOS ARM64, with
`CAPTURE_STATE='{"app.dashboard":"space-lab"}'`:

- [Space Lab in the title bar](evidence/space-lab-twilight-two.png): the
  Space Lab tab and navigator header (`STEPS='wait:6000;shot:main'`).
- [Space Lab in Settings](evidence/space-lab-settings-twilight-two.png):
  **Settings / Space / Space Lab** and the Spaces list
  (`STEPS='wait:6000;action:subspace_app::OpenSettings;wait:1500;click:185,536;wait:1000;shot:settings'`).
