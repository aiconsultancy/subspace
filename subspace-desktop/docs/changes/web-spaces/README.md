# Web Spaces: package folder, packaging script, guide and skill

## Intent

The owner asked to move the web Space packages from `web-boards/` to
`web-spaces/`, with Weather under `web-spaces/examples/weather/`, and for "a
readme and a guide and docs on how to make a web Space and how to package it".
Then: "It should be easy for an agent to bundle up a web Space, put it
somewhere, and we can load that into the app either from a folder, a zip file
or a Git repo. This should be well documented and probably be a skill." New
and updated text says "web Space", not "web board", except for names kept for
compatibility.

## Spec

- `web-boards/weather/` moves to `web-spaces/examples/weather/` with its
  history. Every repository-path reference follows: the
  `weather-space-test` mise task (renamed from `weather-board-test`, still in
  `make check`), AGENTS.md, README.md, the in-app guide, Weather's README, the
  **Install Weather example…** board directory
  (`subspace-desktop/web-spaces/examples/weather`) and its tests.
- Names that are data or format keep the word *board*, because renaming them
  would orphan installed web Spaces or break packages: the manifest
  `subspace-board.json`, the `webBoards` configuration key and manifest field
  names, the private folders `web-board-apps` (installed packages) and
  `web-boards` (WebView2 data, and the Settings extension ID), element IDs
  such as `web-boards-settings`, interface labels such as **Add board…** and
  **Board directory**, and crate and type names. Change records keep their
  old paths.
- Install sources already covered all three that the owner listed, so the app
  is unchanged: **Local folder**, **Local ZIP** (staging, the same validation,
  safe extraction, one enclosing folder, size and entry limits, with tests in
  `spaces/web/src/install/tests.rs`) and Git through **GitHub repository** and
  **GitHub release ZIP**. Git import is GitHub only; this is documented, not
  extended.
- `scripts/package-web-space.py FOLDER [OUT.zip]` (also
  `mise run package-web-space` and `make package-web-space SPACE=…`) applies
  the installer's manifest and package rules with the same messages, leaves
  out tests and development files unless `--include` keeps them, and writes a
  reproducible ZIP: sorted entries, 1980-01-01 timestamps, mode 0644, no
  ZIP64. `--check` validates only. Tests: `scripts/test-package-web-space.py`,
  run by `scripts/test-scripts.sh`.
- Docs: [web-spaces/README.md](../../../web-spaces/README.md), the
  [authoring guide](../../web-spaces-guide.md) and the
  [reference](../../web-spaces-reference.md) (renamed from
  `docs/web-board-apps.md`), linked from the index, the in-app guide and the
  root README.
- Skill: [.agents/skills/web-space](../../../.agents/skills/web-space/SKILL.md)
  with a scaffold template (`assets/template/`) and an error table
  (`references/errors.md`), mentioned in AGENTS.md and the guide.

## Delivery

- The Weather ZIP from the script was installed by the app's own importer
  (`install::prepare` with `ImportRequest::Zip`) in a throwaway test, which
  returned the package name `Weather`, version `1.0.0`. The test was removed.
- Two runs over Weather, one from a copy with a changed modification time,
  give identical bytes (script test).
- No interface change: the only Rust change is the Weather example's board
  directory string in the editor and the tests that use it, so no screenshots
  are needed.
- Gates: `make check`, `mise run quality`, and
  `uvx --from skills-ref agentskills validate .agents/skills/web-space`.

## Tasks

- [x] Move Weather with `git mv`; update path references; keep data and
  format names.
- [x] Rename the mise task; add the packaging task and make target.
- [x] Packaging script and tests, wired into `script-test`.
- [x] README, guide, reference, in-app guide, index and root README.
- [x] Skill with template and error table; validate it.
- [ ] After merge to `main`, check **Install Weather example…** against the
  default branch: until then the new path exists only on this branch.
