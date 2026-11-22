# Tasks: create Space Lab project folders without a start-up race

See the [spec](spec.md) and [delivery](delivery.md).

- [x] Make `project::disk::directory` create first and accept an existing
      real folder, keeping the "unsafe directory" refusal.
- [x] Confirm the helper's other caller, `ProjectStore::snapshot`, behaves
      the same with the new order.
- [x] Add the concurrent-opening regression test.
- [x] Add tests that a file or a symlink at `projects` is still rejected.
- [x] This record, linked from the documentation index.
- [x] The concurrent-opening test fails against the old helper and passes
      with the fix.
- [x] `make check` in the desktop, with Space Lab built in.
- [x] `mise run quality` in the desktop, with no new findings in touched
      code.
- [x] Capture in Twilight Two under `evidence/`, as listed in
      [delivery](delivery.md#evidence).
- [ ] Both model reviews (Claude Opus 5.5 and GPT-6.2 Sol, high effort)
      clear the final head.
