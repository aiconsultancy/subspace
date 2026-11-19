# Change records

Record each change under `docs/changes/<slug>/`, as described in
[AGENTS.md](../../AGENTS.md#conventions). Back to the
[documentation index](../README.md).

A record has these files:

- `intent.md`: what the owner asked for and why.
- `spec.md`: the behaviour the change delivers.
- `delivery.md`: how it was checked, with screenshots for UI changes.
- `tasks.md`: the checklist of work.
- `evidence/`: PNG screenshots embedded in `delivery.md` and the pull request.

Small changes may use one `README.md` instead. A record with all four files may
add a `README.md` that summarises it and links the others.

Records are not listed here. Browse this folder, or find the record for an
area with `ls docs/changes` or `git log --format=%s -- docs/changes`. The
check that keeps the [documentation index](../README.md) complete does not
cover change records.
