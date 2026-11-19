# Publishing the Framework

Back to the [documentation index](README.md). Read
[what publishing exposes](publishing-audit.md) first.

The Framework is 24 crates: the facade `subspace-framework` and the 23
`subspace-*` crates behind it. They share one version and pin each other to
it (`version = "=0.1.0"`), so they are always published together.

**Status.** 0.1.0 was published to crates.io by hand on 17 November 2026, as
crates.io requires for a crate's first release, and tagged
`subspace-framework-v0.1.0`. Later releases come from Release Please and are
published by the `publish-framework` GitHub workflow; see
[The next release](#the-next-release) and
[Publishing from CI](#publishing-from-ci).

## What publishing means

- The source of every crate becomes public on crates.io, under its
  Apache-2.0 licence, for good. A published version cannot be deleted or
  replaced, only *yanked* (hidden from new lockfiles, still downloadable).
- A crate name, once taken, stays taken.
- The first publish of 24 new crates takes about **3 hours 10 minutes**:
  crates.io allows a burst of 5 new crates, then one every 10 minutes. Later
  releases publish new versions of existing crates (a burst of 30, then one
  a minute), so they take a few minutes.

## Prerequisites

- A crates.io account (sign in with GitHub) with a **verified e-mail
  address**.
- An API token from <https://crates.io/settings/tokens> with the
  `publish-new` and `publish-update` scopes, limited to `subspace-*` crates
  if you like. Keep it out of shell history and files; export it only in
  the shell that publishes, as `CARGO_REGISTRY_TOKEN`. Do not run
  `cargo login`; the script reads the variable and never prints it.

## Before the first publish: the owner's checklist

- [x] Apache-2.0 is the licence for every crate (confirmed by the owner on
  17 November 2026).
- [x] The issue-report destination and the GitHub OAuth App are app build
  configuration, not Framework constants (`ProductBuild::github_oauth`,
  `ProductBuild::report_issue_repository` behind the `report-issue`
  feature).
- [x] The audit's open items were decided, the crate names were free, and
  0.1.0 was published (17 November 2026).
- [ ] `repository` and `homepage` stay unset while the repository is
  private (crates.io then shows no source link); set them if it becomes
  public.

## The commands

From `subspace-framework/`, on a clean commit that is pushed (fetch first, so
`git branch -r --contains HEAD` sees it):

```bash
make check            # every check, verified packaging included; stamps this commit
make publish-check    # packaging again, online against the live index; stamps it
make publish-dry-run  # everything except the upload; reads crates.io's API
export CARGO_REGISTRY_TOKEN=…   # from your password manager
make publish
```

`make publish` runs `scripts/publish.sh`, which:

1. refuses unless the tree is clean, `HEAD` is on a remote branch, `make
   check` and `make publish-check` passed for `HEAD` (their stamps in
   `.local/checks/`) and `CARGO_REGISTRY_TOKEN` is set;
2. derives the order from `cargo metadata`: each crate after every crate it
   depends on, dev-dependencies included;
3. asks crates.io which versions exist, skips crates already published (so
   an interrupted run can simply be run again), and prints an estimate;
4. publishes each crate with `cargo publish -p <crate> --no-verify`
   (`make publish-check` verified this commit), waits out HTTP 429 rate
   limits with a progress line and retries, and waits until each version is
   in the index before its dependents;
5. ends with a summary of what was published, skipped or not reached.

`make publish-dry-run` does steps 2 and 3, runs `cargo publish --workspace
--dry-run`, and lists what a real run would refuse instead of stopping.

## The next release

Release Please makes every release; nobody bumps a version or pushes a tag
by hand. The configuration is `release-please-config.json` and
`.release-please-manifest.json` at the repository root, and the workflow
is `.github/workflows/release.yml`.

1. Each push to `main` with releasable commits under `subspace-framework/`
   (`feat`, `fix`, `perf`, `revert` or a breaking change) opens or updates
   the pull request `chore(framework): release X.Y.Z`. Below 1.0 a breaking
   change or a `feat` bumps the minor version, anything else the patch.
2. The pull request sets `[workspace.package] version` and every internal
   pin, the lines between `# x-release-please-start-version` and
   `# x-release-please-end` in `Cargo.toml` (`=0.1.0` → `=0.2.0`). Because
   the app builds the Framework by path, it also moves the app's
   requirement on it in `subspace-desktop/Cargo.toml` and the two terminal
   manifests, and adds the entry to `CHANGELOG.md`. The workflow then
   commits the four lockfiles, refreshed by `scripts/sync-lockfiles.py` at
   the repository root.
3. Every night the workflow merges the pull request when one is open, tags
   the merge commit `subspace-framework-vX.Y.Z`, creates the GitHub release
   and starts `publish-framework.yml` on the tag with `publish` set
   (below). To hold a release back, close the pull request; Release Please
   opens a new one on the next releasable commit.

Keep only exact internal pins inside the markers: Release Please replaces
every version on those lines. `scripts/check-release-config.py`, in the
root `make release-check`, checks that and that every version Release
Please edits matches the manifest.

## Publishing from CI

`.github/workflows/publish-framework.yml` (at the repository root) runs only
when started: `release.yml` starts it on each new release tag with
`publish` set (`gh workflow run publish-framework.yml --ref
subspace-framework-vX.Y.Z -f publish=true`, because a tag created by a
workflow's token starts no workflow by itself).

- **No stored token.** Each crate on crates.io names this workflow file in
  `aiconsultancy/subspace` as a trusted publisher. The publish job exchanges
  its GitHub identity for a crates.io token that lasts 30 minutes and is
  revoked when the job ends.
- **verify** (Linux): with `publish` set, refuses unless the run is on a
  `subspace-framework-v*` tag whose commit is on `main`; installs the
  packages GPUI needs on Linux, packages every crate and builds each from
  its packaged copy (`scripts/package-workspace.sh`). It runs no tests:
  every push to `main` passed them in the pre-push hook, and macOS runner
  minutes cost ten times Linux ones.
- **publish** (Linux): runs `scripts/publish.sh --ci`. In CI mode the
  workflow's verify job is the gate, so the script does not ask for the
  local check stamps; it requires GitHub Actions, a clean tree and a run on
  the tag that names the workspace version. Uploads skip versions already
  published, so a failed or timed-out job can be re-run.
- **A run without `publish` uploads nothing.** `gh workflow run
  publish-framework.yml` verifies, exchanges the identity and does the dry
  run. Use it to check the workflow after changing it.
- **Do not rename the workflow file** without updating every crate's trusted
  publisher on crates.io first (crate › Settings › Trusted Publishing, or
  `POST /api/v1/trusted_publishing/github_configs` with a token that has the
  trusted-publishing scope).
- **A new crate** cannot be created by the workflow. Publish its first
  version by hand with `make publish` (it skips the crates already
  published), then add its trusted publisher.

`make publish` still works for a release from a developer's machine with a
token.

## Subspace Desktop and the published crate

While both projects share this repository, the desktop builds the Framework
from `../subspace-framework` by path. After a publish it can use the
registry instead.

### Switch to the published crate

1. In `subspace-desktop/Cargo.toml`, `[workspace.dependencies]`:

   ```toml
   subspace-framework = { version = "0.1.0" }
   ```

   (delete `path = "../subspace-framework", `).
2. In `terminal/panel/Cargo.toml` and `terminal/runtime/Cargo.toml`, the
   same edit to their `subspace-framework` line.
3. If the chosen release predates the `gpui-kit-text-patch` feature, remove
   that feature from `products/subspace/Cargo.toml`.
4. Refresh the three lockfiles. Cargo makes the smallest change a manifest
   edit needs: the `subspace-*` entries move from path to registry, with
   checksums, and nothing else moves. It must download the crates once, so
   run these online:

   ```bash
   cd subspace-desktop
   mise exec -- cargo metadata --format-version 1 > /dev/null
   MISE_ENV=terminal mise exec -- cargo metadata --manifest-path terminal/runtime/Cargo.toml --format-version 1 > /dev/null
   MISE_ENV=terminal mise exec -- cargo metadata --manifest-path terminal/panel/Cargo.toml --format-version 1 > /dev/null
   git diff --stat Cargo.lock terminal/*/Cargo.lock
   ```

   Check that only `subspace-*` entries changed. Do not run `cargo update`.
5. `make check` and `MISE_ENV=terminal make check-terminal`. After this,
   `--locked` builds work offline from Cargo's cache.

The desktop architecture check accepts the Framework either at
`../subspace-framework` or without a path (crates.io), and any other path is
an error. It still forbids every Framework crate but the facade, the `host`
and `capture` features outside products and host modules in Space source.
The optional `gpui-kit-text-patch` feature is also selected by the product.

### Switch back for Framework work

Restore `path = "../subspace-framework"` in the three manifests and run the
same three `cargo metadata` commands. Do this when a Space change needs an
unreleased Framework change: with the published crate, a Framework edit
reaches the desktop only through a release and a version bump, which is
slower but means the app builds exactly what was released. Integration
pre-releases (`scripts/prerelease-plan.sh`) count Framework commits as app
changes only while the path is in place.

### What stays cross-folder

`[patch.crates-io] markdown` points at
`../subspace-framework/vendor/markdown` (and the two terminal manifests at
`../../../subspace-framework/vendor/markdown`). Cargo never applies a
dependency's `[patch]`, so every app must carry it; the crates do not ship
`vendor/markdown`.

The desktop also uses the optional [GPUI Kit text patch](../vendor/gpui-base/SUBSPACE-PATCH.md).
All three desktop manifests patch `gpui-base` 0.9.1 from the same vendor
directory. The published Framework's default features compile against the
registry's GPUI Kit.

Upstream has merged selectable text reveal and shared linked-image click
handling. The Framework uses `TextView::on_link_click` to supply the URL
to the text menu on secondary click. See the
[patch status and retirement checks](../vendor/gpui-base/SUBSPACE-PATCH.md#upstream-status)
when choosing a replacement GPUI Kit release.

If the two folders become separate repositories, copy `vendor/markdown` and
`vendor/gpui-base` from the Framework's repository into
`subspace-desktop/vendor/`, add
the copies to the workspace's `exclude`, and point the patches at the
corresponding directories under `vendor/` in `Cargo.toml` and `../../vendor/`
in the terminal manifests. The architecture check accepts those locations.
Refresh the copies when the Framework's copies change. Remove each when its
upstream release provides the corresponding fixes.

## Evidence that the switch works

Before any publish, the desktop was built against the packaged crates
exactly as crates.io would serve them: `cargo package --workspace` wrote the
24 `.crate` files, a throwaway copy of `subspace-desktop` dropped the
`path` from its three `subspace-framework` lines and patched each
`subspace-*` crate to its unpacked package (registry-style manifests, no
workspace), and in that copy `cargo check -p subspace`, `cargo check
--workspace --all-targets`, `cargo build -p subspace` (whose `--version`
ran) and `cargo test --workspace` (1,561 tests) passed with no third-party
version changed in its lockfile. The Subspace App's change record for the
split keeps the details.
