# The Framework as its own crate: delivery

Back to the [spec](spec.md) and [tasks](tasks.md).

## Round 3: ready to publish

### Gates

Run on 2026-11-17 on `feat/subspace-desktop` with uncommitted round 3
changes.

| Where | Command | Result |
| --- | --- | --- |
| `subspace-framework/` | `make check` | pass: "Framework architecture check passed for 24 packages", "Package check passed for 24 crates", docs and Space guide checks, format, cargo-shear, clippy, 2151 tests (2 README doctests) passed, 4 ignored, verified packaging of all 24 crates, publish and bump fixtures (6 and 3 tests) |
| `subspace-framework/` | `make publish-check` | pass in 5 min 44 s from a cleared target; verified packaging alone takes about 25 s warm |
| `subspace-framework/` | `mise run quality` | `Findings: 0 → 0 (+0)` |
| `subspace-framework/` | `mise run audit` | `advisories ok` |
| `subspace-framework/` | `make publish-dry-run` | "24 crates; 24 new, 0 new versions, 0 already on crates.io", "Estimated time: about 190 min", "Dry run passed; nothing was uploaded." It listed what a real run would refuse (uncommitted tree, HEAD not on a fetched remote branch, no stamps, no token) |
| `subspace-desktop/` | `make check` (through the root) | pass: "Desktop architecture check passed for 15 packages", 1568 tests passed, 4 ignored, script fixtures |
| `subspace-desktop/` | `mise run quality` | `Findings: 0 → 0 (+0)` (from 45) |
| `subspace-desktop/` | `MISE_ENV=terminal make check-terminal` | pass: 279 terminal tests, `cargo build -p subspace --features terminal`, rustqual `0 → 0` |
| `subspace-desktop/` | `make interfaces` | `real product headless interface checks passed` |
| repository root | `make check` | pass: 3719 tests, 0 failed |
| repository root | `prek run --all-files`, `prek run --stage pre-push --all-files` | every hook passed in both projects and at the root |

### The desktop against the packaged crates

`cargo package --workspace` wrote the 24 `.crate` files. A throwaway copy of
`subspace-desktop` (outside the repository) took `subspace-framework =
"0.1.0"` without a path in its `Cargo.toml` and both terminal manifests and
patched each `subspace-*` crate to its unpacked package, which has the
registry-style manifest crates.io serves. In that copy, offline: `cargo check
-p subspace` and `cargo check --workspace --all-targets` passed, compiling
the 24 crates from the unpacked packages; `cargo build -p subspace` passed and
`subspace --version` printed `Subspace v0.8.0-dev-…`; `cargo test --workspace`
passed 1,561 tests; its lockfile changed no third-party version. Its
architecture check rejected only the extra `[patch]` entries, as it should.
The copy was deleted afterwards.

### Cross-folder scan

```bash
git ls-files -co --exclude-standard subspace-desktop | grep -v '^subspace-desktop/docs/changes/' \
  | xargs grep -InE '\.\./subspace-framework|ROOT\.parent|subspace-framework/' | awk -F: '{print $1}' | sort | uniq -c
git ls-files -co --exclude-standard subspace-framework | xargs grep -InE '\.\./subspace-desktop|subspace-desktop/'
```

Desktop hits: the two documented paths in `Cargo.toml` and their repeats in
the terminal manifests; the facade feature name `subspace-framework/capture`
in `products/subspace/Cargo.toml`; the architecture check's expected
location and its fixtures; prerelease and board-guide test fixtures and
comments; and Markdown prose and links in docs, guides' evidence comments,
README, AGENTS.md and the checklist skill. The Framework's hits are prose in
`docs/publishing.md`, AGENTS.md and its checklist skill naming where the app
lives. No script, test or build reads the other folder.

## Round 2: tooling, checks and docs

### Gates

Run on 2026-11-17 on `feat/subspace-desktop` with uncommitted round 2
changes, through `MISE_AUTO_INSTALL=0 COPILOT_SKIP_CLI_DOWNLOAD=1 mise exec`.

| Where | Command | Result |
| --- | --- | --- |
| `subspace-framework/` | `make check` | pass: architecture (61 fixture tests, 5 icon-button, 8 copy-details; "Framework architecture check passed for 24 packages"), docs index and Space guides, format, cargo-shear, clippy `-D warnings`, 2149 tests passed, 0 failed, 4 ignored |
| `subspace-framework/` | `mise run quality` | pass: `Findings: 0 → 0 (+0)`, `Violations: 0 → 0` |
| `subspace-framework/` | `mise run audit` | `advisories ok` |
| `subspace-desktop/` | `make check` | pass: architecture (40 fixture tests, 5 icon-button; "Desktop architecture check passed for 15 packages including excluded terminals"), docs index and board guides, format, cargo-shear, clippy `-D warnings`, 1561 tests passed, 0 failed, 4 ignored, script, web bridge and Weather fixtures |
| `subspace-desktop/` | `mise run quality` | pass: `Findings: 45 → 45 (+0)`, `Violations: 3 → 3` (see below) |
| `subspace-desktop/` | `MISE_ENV=terminal make check-terminal` | pass: fmt, clippy and 278 tests in both terminal workspaces, `cargo build -p subspace --features terminal`, rustqual `45 → 45` |
| `subspace-desktop/` | `make interfaces` | `real product headless interface checks passed` |
| `subspace-desktop/` | `mise run audit` | `advisories ok` for `Cargo.toml`, `terminal/runtime` and `terminal/panel` |
| `subspace-desktop/` | `bash scripts/test-release-macos.sh` and the other packaging fixtures | `release-macos fixtures passed`; package-products, package-macos-ci, run-app, capture, bundle-mise and install-releases fixtures passed |
| `subspace-desktop/` | `make capture OUT=… STEPS='wait:5000;shot:main'` | wrote `main.png` (PRs Space, status bar `v0.8.0-test-capture-…`) |
| repository root | `make check` | pass: both projects, 3710 tests |
| repository root | `prek run --all-files` and `prek run --stage pre-push --all-files` | every hook passed in both projects and at the root |

### Quality baselines

The old single baseline scored 0 findings over the whole tree (17,248
functions when re-run on the parent of the split commit). The two new
baselines cover 9,401 (Framework) and 7,885 (desktop) functions.

- **Framework: 0 findings.** Splitting first showed 94: 86 public functions
  and types only the app used (dead code from the Framework's point of view)
  and 7 private GitHub client functions only the app's tests reached. The
  public API is now marked `// qual:api`, the established marker for API
  entry points; the client functions have new offline tests
  (`clients/github/src/tests/copilot_and_assignable.rs`). One IOSP finding in
  `chart/marks.rs` (`scatter`) carries a reasoned `qual:allow(iosp)`.
- **Desktop: 45 findings**, in code the split did not change. rustqual's
  call graph is name-based. In the single tree a test anywhere that named a
  function (for example a `main()` inside a test macro, or another crate's
  `paint`) counted as testing every function of that name, which hid these:
  21 untested functions (terminal paint code, IRC listing and details
  helpers, Issues pickers and lanes, the panel registration, two example
  helpers and the IRC smoke example), 18 repeated matches, 3 IOSP
  violations, 1 long test module, 1 test without a system under test and 1
  test-only accessor. The 17 findings the build stamp brought into the app
  are fixed: the stamp moved to a tested module (`build/stamp.rs`), the
  build script keeps two reasoned `qual:allow(test_quality, untested)`
  markers, and a dead `known_products` became a test helper. The 45 are
  recorded honestly in the baseline as round 3 follow-ups, not suppressed.

### Cross-folder scan

```bash
cd /Users/jack/Code/aic/subspace
git ls-files -co --exclude-standard subspace-desktop | grep -v '^subspace-desktop/docs/changes/' \
  | xargs grep -InE '\.\./subspace-framework|ROOT\.parent' | awk -F: '{print $1}' | sort | uniq -c
git ls-files -co --exclude-standard subspace-framework \
  | xargs grep -InE '\.\./subspace-desktop|subspace-desktop/'
```

Every desktop hit is one of the documented kinds: the two paths in
`Cargo.toml` and their repeats in `terminal/panel/Cargo.toml` and
`terminal/runtime/Cargo.toml`; the architecture check's expected Framework
location (`FRAMEWORK_ROOT`, never read) and its fixtures; and Markdown links
in docs, AGENTS.md, README.md and the developer-checklist skill, which the
doc checks treat as external. The Framework's two hits are prose in its
AGENTS.md and checklist skill telling agents where the app lives in this
repository.

## Round 1: the split

fmt, clippy and tests passed in both workspaces and both terminal
workspaces; see the round 1 report in the [tasks](tasks.md).
