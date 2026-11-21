# Delivery: create Space Lab project folders without a start-up race

See the [spec](spec.md) and [tasks](tasks.md).

## State on 21 November 2026

The fix is edited on the working tree, on top of the uncommitted
[Space Lab rename](../space-lab-rename/intent.md). It is a separate commit
stacked on the rename:

```text
fix(desktop): create Space Lab project folders without a start-up race
```

What changed:

- `spaces/space-lab/src/project/disk.rs`: `directory` creates the folder
  first and checks what is there only when it already exists.
- `spaces/space-lab/src/project/store_tests.rs`: the concurrent-opening
  regression test and the file and symlink rejection tests.

## Validation

Run on 21 November 2026 on macOS ARM64, on the working tree with the
rename and this fix:

- `concurrent_store_opening_on_a_fresh_root_succeeds` against the old
  `directory`: failed with `Err(File exists (os error 17))`, the error the
  captures showed. With the fix: passed, with the rest of
  `project::store_tests` (12 tests).
- `make check` in the desktop: passed, with Space Lab built in for the
  workspace tests and lint, then the release configuration.
- `mise run quality` in the desktop: passed with no regression and no
  finding in the touched code.
- After rebasing onto `main` at `bef3699`: `make check` and `mise run
  quality` in both projects passed again, with no findings.

The model reviews are recorded in the pull request.

## Evidence

- Before: the rename's
  [Space Lab capture](../space-lab-rename/evidence/space-lab-twilight-two.png)
  shows "File exists (os error 17)" at the foot of the welcome page.
- [Space Lab after start-up](evidence/space-lab-start-up-twilight-two.png):
  a capture in Twilight Two after the fix, from a capture build with
  `CAPTURE_FEATURES=space-lab` and Space Lab turned on, showing the welcome
  page with no error.
