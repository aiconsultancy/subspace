# Spec: rename Board Lab to Space Lab

See the [intent](intent.md), [delivery](delivery.md) and [tasks](tasks.md).
Back to [change records](../README.md).

## Names

Every name below changes, with no alias for the old one.

| What | Before | After |
| --- | --- | --- |
| Display name | Board Lab | Space Lab |
| Space (board) id | `board-lab` | `space-lab` |
| Folder | `spaces/board-lab` | `spaces/space-lab` |
| Crate | `subspace-board-lab-board` | `subspace-space-lab-board` |
| Registered type | `subspace_board_lab_board::BoardLab` | `subspace_space_lab_board::SpaceLab` |
| App Cargo feature (`products/subspace`) | `board-lab` | `space-lab` |
| Descriptor Cargo feature (`products/descriptor`) | `board-lab` | `space-lab` |
| Agent and MCP tools | `board_lab_project_inspect`, `board_lab_project_edit`, `board_lab_inspect`, `board_lab_edit`, `board_lab_run`, `board_lab_environment` | `space_lab_project_inspect`, `space_lab_project_edit`, `space_lab_inspect`, `space_lab_edit`, `space_lab_run`, `space_lab_environment` |
| CLI commands | `subspace board-lab <action>`, `subspace board-lab-inspect` and the other single-tool commands | `subspace space-lab <action>`, `subspace space-lab-inspect` and so on |
| mise task | `board-lab-examples` | `space-lab-examples` |
| Saved choice in `config.json` | `boards."board-lab"` | `boards."space-lab"` |
| Navigator preference | `board-lab.navigator` | `space-lab.navigator` |
| Data folder | `…/subspace/boards/board-lab/` | `…/subspace/boards/space-lab/` |
| Interfaces fixture variable | `SUBSPACE_FIXTURE_BOARD_LAB` | `SUBSPACE_FIXTURE_SPACE_LAB` |
| Developer page | `docs/board-lab.md` | `docs/space-lab.md` |
| Design evidence | `docs/agents/board-lab/` | `docs/agents/space-lab/` |
| Component gallery sample | `board-lab::row_actions`, `gallery-sample:board-lab-button-row` | `space-lab::row_actions`, `gallery-sample:space-lab-button-row` |
| Gallery evidence images | `*-inconsistencies-board-lab-row-actions.png` | `*-inconsistencies-space-lab-row-actions.png` |

The app's `unreleased-spaces` feature becomes
`["space-lab", "component-gallery"]`. Other names derived from the id, such
as element ids, constants and test names, follow it: `board_lab` becomes
`space_lab`, `BoardLab` becomes `SpaceLab` and `BOARD_LAB` becomes
`SPACE_LAB`.

Nothing reads the old names. A `boards."board-lab"` entry or a
`board-lab.navigator` preference left from a development build is ignored,
and a `boards/board-lab/` data folder is left where it is. There are no
users, so none is migrated.

## Behaviour that does not change

- **Gating.** Space Lab is in the app only with its `space-lab` feature, or
  `unreleased-spaces`. Neither is in `default`, so release and packaging
  builds leave it out.
- **Off by default.** Built in, it starts off in every build profile, debug
  included. A developer turns it on in Settings or from **Add a Space**.
- **Platform.** It is registered only on macOS ARM64. Linux and Windows do
  not include it, even with its feature.
- **Everything else.** Its views, tools' actions and approval rules,
  settings, layout and help images are unchanged; only the names above
  differ.

## Capture

`scripts/capture.sh` gains an opt-in `CAPTURE_FEATURES=<features>` variable,
which adds Cargo features of the app to the capture build, so that a Space
left out of the default build can be captured as evidence. Without it the
capture build is the same as before. It is a small addition, separate from
the rename, made so that this change's screenshots can be taken.

## Historical records

Change records under `docs/changes/` keep the name they were written with.
None of them links to a file that moved, so their text is unchanged. The [unreleased Spaces](../unreleased-spaces/intent.md)
and [workspace shell](../workspace-shell/intent.md) records, which define
the Space's current gating and layout, carry a one-line pointer to this
record.

## Breaking change

The agent, MCP and CLI tool names change, so a caller using the old names
stops working, and so does a build that names the old Cargo feature. The
commit is marked with `!` and carries a `BREAKING CHANGE:` footer, for
example:

```text
refactor(desktop)!: rename Board Lab to Space Lab

BREAKING CHANGE: the Space's agent, MCP and CLI tools are renamed from
board_lab_* to space_lab_*, and the app's and descriptor's Cargo feature
from board-lab to space-lab.
```
