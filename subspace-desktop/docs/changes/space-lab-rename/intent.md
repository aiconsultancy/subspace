# Intent: rename Board Lab to Space Lab

Back to [change records](../README.md).

Board Lab is the alpha Space for native dashboards backed by managed Python,
Node or Bun projects. Its name comes from the time when a Space was called a
board. The product's vocabulary is now Spaces: the title bar's
**Add a Space** menu, the Settings group and the guides all say Space, so a
Space whose name says board reads as a leftover.

## Owner direction (21 November 2026)

- **Rename Board Lab to Space Lab.** The display name changes everywhere it
  shows: the title bar, the navigator, Settings, **Add a Space**, Help and
  the documentation.
- **A full rename, not a display-only one.** The board id, folder, crate,
  Cargo features, agent and CLI tools, task names, stored keys and
  documentation paths follow the new name too, so the code and the product
  do not use two names for one Space. A display-only rename was considered
  and not chosen: it would leave `board-lab` in every identifier a
  developer or an agent types.
- **No migration.** Subspace has no users or installs yet, and Board Lab has
  never been in a release build, so no stored setting, saved choice or data
  folder under the old name needs carrying across. None is written.

This follows the [workspace shell](../workspace-shell/intent.md) change,
whose pull request has merged, and the
[unreleased Spaces](../unreleased-spaces/intent.md) change, which left Board
Lab out of releases and said the rename would come separately.

## Out of scope

- Any change to what the Space does, how it is gated or where it runs.
- Historical change records. They keep the name they were written with; the
  two that define the Space's current gating and layout point here.
- The general Space-for-board terminology in code. Other identifiers that say
  board keep their names.

See the [spec](spec.md), [delivery](delivery.md) and [tasks](tasks.md).
