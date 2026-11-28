# The Framework as its own crate: tasks

Back to the [spec](spec.md).

## Round 1: the split (done)

- [x] Move the Framework's crates, the gallery, `clients/github`,
  `clients/test-support` and `vendor/markdown` to `subspace-framework/`
  with `git mv`; the terminal panel, runtime and `ghostty-config` to
  `subspace-desktop/terminal/`.
- [x] Framework workspace, facade crate and lockfile (pruned from the desktop
  lockfile; no third-party version changed).
- [x] Desktop depends only on `subspace-framework`; sources say
  `subspace_framework::<module>::…`.
- [x] The Subspace App's descriptor and metadata helper in
  `products/descriptor`; scripts call `-p subspace-descriptor`.
- [x] Cross-folder reads that broke builds or tests removed (see the
  spec's decisions).
- [x] fmt, clippy and tests pass in both workspaces and both terminal
  workspaces; `cargo build -p subspace --features terminal`,
  `scripts/test-product-interfaces.sh` and `scripts/test-scripts.sh` pass.

## Round 2: tooling and docs (done)

- [x] Architecture checks split: `subspace-framework/scripts/` (layers,
  headless services, source includes, `PATH`, switch, icon-button,
  settings-dispatch, search-feature and artwork rules, copy-details
  inventory) and `subspace-desktop/scripts/` (one Framework crate, host
  features and modules for products only, test-support on dev edges,
  documented Framework path and patch, layer directions, source includes,
  `PATH`, switch and icon-button rules); fixtures split the same way.
- [x] Docs split with an index and index check in each folder; the
  Framework checks its Space guide, the desktop its products' guides; links
  across are external to both checks.
- [x] Framework `Makefile`, `mise.toml`, `prek.toml`, `deny.toml`,
  `rustqual.toml` and baseline, README, AGENTS.md, `framework-checklist` and
  the GPUI Kit skills; `refresh-flint.sh` and `format-workspace.py` there.
- [x] Desktop `Makefile` (quality, audit), `mise.toml`, AGENTS.md, README,
  `developer-checklist` skill, toc, baseline.
- [x] Repository root: `Makefile` runs the shared targets in both and
  forwards the app's; `prek.toml` workspace mode covers both; README.
- [x] Build stamp owned by the product (`ProductBuild::build`,
  `BuildInfo`); the shell runs no git and names no repository.
- [x] Gallery text that named Desktop Spaces phrased as examples.
- [x] Gates in [delivery](delivery.md).

## Round 3: ready to publish (done)

- [x] Manifests ready to publish; internal pins on every edge confirmed.
- [x] `LICENSE` in every crate; `NOTICE` and licence texts where third-party
  material is embedded; `scripts/check-package.py` in `make check`.
- [x] Audit of what a publish exposes (`subspace-framework/docs/publishing-audit.md`);
  a real person's name and handles removed from test fixtures; gitleaks clean.
- [x] `cargo package --workspace` with verification in `make check`;
  `make publish-check` before a release.
- [x] `scripts/publish.sh` (order from `cargo metadata`, refusals, resume,
  rate-limit waits, index waits, dry run, summary) with fixtures in
  `make check`; `make publish`, `make publish-dry-run`, `make bump-version`.
- [x] `subspace-framework/docs/publishing.md`, including the desktop's switch
  to the published crate, proven against the packaged crates.
- [x] Pre-release planning counts the path-built Framework.
- [x] Desktop rustqual findings 45 → 0.

## Left for the owner

- [ ] The decisions in `subspace-framework/docs/publishing-audit.md` (crate
  names, the issue-report destination, the OAuth App, Subspace wording and
  `SUBSPACE_*` names, the owner as a persona, `repository`/`homepage`).
- [ ] Publishing itself, with a token, from a clean pushed commit.
- [ ] After publishing: switch the desktop to the published crate, or keep
  the path while both projects share this repository.
