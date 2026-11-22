# Intent: create Space Lab project folders without a start-up race

Back to [change records](../README.md).

On a fresh profile, Space Lab's welcome page shows a red
"File exists (os error 17)" message. The [Space Lab rename](../space-lab-rename/delivery.md#validation)
captures showed it and left it for a separate fix, because the code that
causes it is not part of the rename.

The message is more than cosmetic. Two background tasks open the project
store for the same root at start-up, and both try to create its folders.
When the live-update observer is the one that fails, it reports the error
and stops, so live dashboard updates never start in that session.

## Owner direction (21 November 2026)

- **Fix the race where it happens.** Make the project store's folder
  creation safe when two openers race, and keep its refusal of symlinks and
  other unsafe paths.
- **Keep it minimal.** Change only the folder helper and its tests; leave the
  service registry and its callers as they are.
- **A separate commit** stacked on the Space Lab rename, for example
  `fix(desktop): create Space Lab project folders without a start-up race`.

## Out of scope

- Any other change to how the project service starts, is shared or reports
  errors.
- The rename itself, which has its own [record](../space-lab-rename/intent.md).

See the [spec](spec.md), [delivery](delivery.md) and [tasks](tasks.md).
