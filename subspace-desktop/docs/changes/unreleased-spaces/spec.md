# Spec: leave Board Lab and the Component gallery out of releases

See the [intent](intent.md), [delivery](delivery.md) and [tasks](tasks.md).

## Features

The app (`products/subspace/Cargo.toml`) gains three features. None is in
`default`, which stays `["report-issue"]`.

| Feature | Turns on | Effect |
| --- | --- | --- |
| `board-lab` | `dep:subspace-board-lab-board`, `subspace-descriptor/board-lab` | Compiles and registers Board Lab on macOS ARM64. The dependency stays target-specific and is now optional. |
| `component-gallery` | `subspace-framework/component-gallery`, `subspace-descriptor/component-gallery` | Compiles and registers the Framework's Component gallery. The Framework dependency no longer names `component-gallery` itself. |
| `unreleased-spaces` | `board-lab`, `component-gallery` | Both, for development and for the workspace lint and tests. |

The descriptor crate (`products/descriptor/Cargo.toml`) gains `board-lab`
and `component-gallery`, both empty. Nothing but the app depends on the
descriptor, and the app turns them on only through its own features of the
same names.

## Registration and descriptor

- `products/subspace/src/main.rs`: Board Lab's `BoardRegistration` is behind
  `#[cfg(all(feature = "board-lab", target_os = "macos", target_arch = "aarch64"))]`
  and the gallery's behind `#[cfg(feature = "component-gallery")]`. The order
  of the others is unchanged.
- `products/descriptor/src/lib.rs`: `board("board-lab", false)` and
  `board("component-gallery", false)` carry the same `cfg` attributes. Both
  were `cfg!(debug_assertions)`; each now starts off in every build profile.
- The shell refuses a descriptor board without a registration ("no GUI
  provider for board …") and never shows a registration the descriptor does
  not list, so the two must agree. One feature turns on both sides, and a
  product test compares them in every configuration it runs in.
- The metadata helper (`subspace-product-metadata`, run by the packaging
  scripts with `-p subspace-descriptor` and no features) therefore writes
  the release board list into `product.json`, which matches the default
  binary. A development bundle made by `make run-unreleased` also gets that
  `product.json`; only its `id`, `version` and `bundle_id` are read, by the
  updater's identity check, so the shorter board list there is harmless.

Everything else follows from the registrations and the descriptor: Settings,
**Add a Space**, the command palette, `board_help`, the MCP and CLI tool
catalogue, and the headless commands.

## Checks

- `mise run lint` (`make lint`, `make check`): `cargo shear`, then clippy
  with warnings denied on the workspace with `--features
  subspace/unreleased-spaces`, then clippy on `-p subspace -p
  subspace-descriptor` with default features.
- `mise run test` (`make test`, `make check`): `cargo test --workspace
  --locked --features subspace/unreleased-spaces`, then `cargo test --locked
  -p subspace -p subspace-descriptor` with default features.
- The second run of each rebuilds what the feature change touches: the
  Framework facade, and the Spaces, clients and app above it, because the
  facade's feature set differs (no `component-gallery`). The Framework's
  other crates and third-party crates are reused unless another workspace
  member had turned on extra features of theirs. It does not repeat a build
  of the whole workspace.
- `mise run quality` reads the sources and is unaffected.
- `make interfaces` builds the default app and expects the board list
  `github, github-copilot-agents, irc, issues, decisions, actions`. With
  `SUBSPACE_FIXTURE_BOARD_LAB=1` (macOS ARM64) it builds with
  `--features board-lab`, expects Board Lab after Decisions and runs its
  optional Board Lab checks.
- `make check-terminal` builds the default app with `terminal`, as before.
- `make capture` builds with `capture` (and `terminal` when Zig is
  available), so captures show neither Space.

## Release and packaging

Release, packaging and install builds go through
`scripts/build-macos-app.sh` or `scripts/package-release.sh`, whose
`release_cargo_args` adds `--features "$SUBSPACE_BUILD_FEATURES"` only when
that variable is set. The `package`, `package-macos`, `release-macos` and
`package-macos-ci` targets do not set it, and `make install-macos` sets it to
`terminal`. Nothing in `scripts/`, the `Makefile`, `mise.toml`,
`mise.terminal.toml` or `.github/workflows/` passes `--all-features` or names
these features, and the workflows build nothing.

A shell can still export `SUBSPACE_BUILD_FEATURES`, and the release targets
pass it on. So `scripts/package-release.sh` and `scripts/package-macos.sh`
(which `release-macos` and `package-macos-ci` also run) call
`refuse_unreleased_features` from `scripts/product.sh` before any build: it
exits with status 2 when the list names `unreleased-spaces`, `board-lab` or
`component-gallery`, with or without a package prefix such as
`subspace/board-lab` or `subspace-framework/component-gallery`, split on
commas and whitespace as Cargo splits it.
`scripts/test-package-products.sh` checks that both refuse, run no Cargo
command and leave the previous archive in place. A local install is not a
release: `make install-macos FEATURES="terminal unreleased-spaces"` still
installs a build with both Spaces.

## Running with them

`make run-unreleased` runs the debug app with `terminal unreleased-spaces`,
in the style of `make run` (`FEATURES=terminal`), so
`make run FEATURES="terminal unreleased-spaces"` is the same. Both Spaces
start off: turn them on in Settings or from **Add a Space**.

## Tests

In the app (`products/subspace/src/main.rs`):

- New: `the_descriptor_lists_exactly_the_registered_boards_in_order` and
  `the_unreleased_spaces_follow_their_features`, in both configurations.
- `the_component_gallery_is_registered_last` runs only with
  `component-gallery`; `board_labs_registered_screenshot_reaches_the_native_help_image_cache`
  only with `board-lab` on macOS ARM64.
- `composed_artwork_keeps_keys_bytes_and_legacy_listing_order`,
  `the_documented_tool_inventory_matches_the_compiled_catalog` and
  `only_boards_that_own_processes_stay_hidden_when_disabled` expect each
  Space's artwork, tools and retention only when it is built in.

In the descriptor (`products/descriptor/src/lib.rs`): the Board Lab and
gallery tests check each is listed only with its feature and starts off; the
order and default tests take the features into account. They run with the
features in the workspace run and without them in the second run.

The Component gallery's and Board Lab's own crates stay workspace members
(the gallery in the Framework's workspace) and keep their own tests.
