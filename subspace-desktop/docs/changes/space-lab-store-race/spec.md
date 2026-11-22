# Spec: create Space Lab project folders without a start-up race

See the [intent](intent.md), [delivery](delivery.md) and [tasks](tasks.md).
Back to [change records](../README.md).

## Cause

When Space Lab opens, two background tasks call
`project_service::shared(&root)` for the same root at about the same time:

- the first catalogue load, in `ProjectBrowser::run`
  (`src/project_browser.rs`);
- the live-update observer, in `ProjectBrowser::observe`
  (`src/project_browser/observation.rs`).

`shared` (`src/project_service/registry.rs`) builds a `ProjectStore` before
it takes the registry lock, so both tasks run `ProjectStore::new` at once.
That creates the store's `projects` and `project-snapshots` folders through
`project::disk::directory`, which checked for the folder and then created
it. On a fresh profile both tasks can find the folder missing; the first
creates it and the second's `create_dir` fails with `AlreadyExists`
("File exists (os error 17)").

Either task can lose. When the observer loses, it shows the error and
returns, so the store's change subscription is never made and live
dashboard updates do not start until Space Lab is opened again.

## Fix

`project::disk::directory` creates first and checks second:

1. `fs::create_dir(path)`. If it succeeds, the folder is new and safe.
2. If it fails with `AlreadyExists`, something is already at the path:
   `fs::symlink_metadata(path)` must show a real directory that is not a
   symlink, or the call fails with "unsafe directory: <path>", as before.
3. Any other error is returned as before.

The loser of a race now finds the winner's folder and accepts it. A
symlink, a dangling symlink or a file at the path makes `create_dir` fail
with `AlreadyExists` and is then rejected, so unsafe paths fail exactly as
they did. The style follows `private_settings::disk::directory`, which
already tolerates `AlreadyExists`.

The registry and its callers are unchanged. Two stores may still be built
for one root during start-up, but only one is kept under the registry lock,
and building a store has no other side effect.

The helper's other caller, `ProjectStore::snapshot`, creates the per-project
folder under `project-snapshots` while it holds the store's write lock. The
new order behaves the same there: an existing real folder is accepted and
anything else is rejected.

## Tests

In `src/project/store_tests.rs`:

- `concurrent_store_opening_on_a_fresh_root_succeeds`: 16 threads released
  together by a `Barrier` open the store on a fresh root, over 20 fresh
  roots. Every opener must succeed and both folders must exist. With the old
  check-then-create, two threads finding a folder missing at once makes one
  of them fail, which is very likely across 640 folder-creation attempts.
- `store_rejects_a_file_in_place_of_its_projects_folder`: a file at
  `projects` still fails with "unsafe directory".
- `store_rejects_a_symlink_in_place_of_its_projects_folder` (Unix): a
  symlink to a folder and a dangling symlink at `projects` both still fail
  with "unsafe directory", and the dangling target is not created.

The existing `source_and_project_directory_symlinks_fail_closed` covers
symlinks below `projects`, not at the folder itself, so these are new.
