# Tasks: rename Board Lab to Space Lab

See the [spec](spec.md) and [delivery](delivery.md).

- [x] Move `spaces/board-lab` to `spaces/space-lab` and rename the crate to
      `subspace-space-lab-board` and its registered type to `SpaceLab`.
- [x] Rename the app's and descriptor's `board-lab` features to
      `space-lab`, and `unreleased-spaces` to
      `["space-lab", "component-gallery"]`.
- [x] Rename the board id, display name, navigator preference, saved choice
      and data folder.
- [x] Rename the agent, MCP and CLI tools to `space_lab_*`, and the
      Framework's naming tests that list them.
- [x] Rename the `space-lab-examples` mise task, the interfaces fixture
      variable and the names in scripts, tests and fixtures.
- [x] Rename the Component gallery's row-actions sample and its evidence
      images.
- [x] Add the opt-in `CAPTURE_FEATURES` to `scripts/capture.sh`.
- [x] Docs: the Space's guide, `HELP.txt` and help-image notes, the
      developer page and design evidence (moved), the README, `AGENTS.md`,
      the documentation index, both tables of contents, the boards page, the
      Spaces audit, the glossary, the installation and capture pages, and
      the Framework's UI components, MCP and board integration pages.
- [x] Pointers to this record from the unreleased Spaces and workspace shell
      intents; other historical records left as they are.
- [x] `make check` in the desktop and in the Framework.
- [x] `mise run quality` in the desktop and in the Framework.
- [x] `make interfaces`, and on macOS ARM64
      `SUBSPACE_FIXTURE_SPACE_LAB=1 make interfaces`.
- [ ] `make run-unreleased` checked by hand: Space Lab in Settings and in
      **Add a Space**, off until turned on.
- [x] Captures in Twilight Two under `evidence/`, as listed in
      [delivery](delivery.md#evidence).
- [ ] Both model reviews (Claude Opus 5.5 and GPT-6.2 Sol, high effort)
      clear the final head.
