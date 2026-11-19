# The Framework as its own crate: spec

Back to the [intent](intent.md).

## Rules of the split

1. **Two folders, two Cargo workspaces.** `subspace-framework/` and
   `subspace-desktop/` each have their own `Cargo.toml` workspace,
   `Cargo.lock`, `Makefile`, `mise.toml`, checks, quality baseline, README
   and AGENTS.md. Each builds, lints and tests alone.
2. **Everything in `subspace-framework/` is published; nothing unpublished
   lives there**, apart from the vendored `markdown` patch (see below),
   examples and tests.
3. **One import.** Subspace Desktop depends on exactly one Framework crate,
   `subspace-framework`, and names nothing else in the Framework's folder.
   Spaces, products and desktop-owned clients write
   `subspace_framework::<module>::…`.
4. **Two cross-folder paths, in `subspace-desktop/Cargo.toml`:** the
   `subspace-framework` dependency (`path` plus `version`; deleting `path`
   switches to crates.io) and the `[patch.crates-io] markdown` entry. The
   two excluded terminal manifests (`terminal/panel/Cargo.toml`,
   `terminal/runtime/Cargo.toml`) repeat the same two paths, because an
   excluded workspace cannot inherit them. No script, test, build script,
   doc check, `include_str!` or `#[path]` in either folder reads the other
   folder. The desktop architecture check names `../subspace-framework` only
   to verify that those paths point there; it reads nothing in it. Docs may
   link across with plain relative Markdown links, which each folder's doc
   checks treat as external and never open.
5. **Dependencies point one way:** desktop → framework. Nothing in the
   Framework names a Subspace Desktop Space, client or product.

## What goes where

**`subspace-framework/`**

- The root package is the facade crate `subspace-framework` (`src/lib.rs`),
  as `bevy` is over `bevy_*`: it re-exports the Framework's crates as
  modules named after them (`subspace_framework::dashboard`, `::core`,
  `::lucide`, `::agent_tools`, `::contracts`, `::charts`, `::search`,
  `::product`, `::client_github`, …). The crates behind it keep their names
  and are published with it, because Cargo requires every dependency of a
  published crate to be published.
- Today's layers, unchanged: `services/`, `board-kit/`, `runtimes/agent`,
  `host/` (app, board-command, chat, mcp, report-issue, search-ui, update).
- `clients/github` and `clients/test-support`, which Framework crates use.
- The Component gallery Space (`spaces/component-gallery`): the Framework's
  component catalogue, published with it and exposed through the facade.
- `vendor/markdown`, the patched parser both workspaces need.
- The Framework's docs (Space author guide, component catalogue, checklist,
  glossary), scripts and architecture check.
- Facade features: the default is what a Space author needs (services,
  board-kit, the GitHub client). `host` adds the shell and the other host
  crates, for app builders only. Others as needed: `agent`, `search-onnx`,
  `component-gallery`, `test-support`, `capture`.

**`subspace-desktop/`**

- `spaces/` (every Space except the gallery), `products/subspace`,
  `clients/irc` and `clients/nett`.
- The terminal panel and its runtime, with `ghostty-config`: `libghostty-vt`
  is a git dependency, which crates.io refuses, so they cannot be published
  yet. They consume the Framework through the facade like any other
  desktop crate and stay excluded Zig workspaces. The shell still docks the
  panel through board-kit's `dock_panel` contract; behaviour does not change.
  They move into the Framework when `libghostty-vt` has a usable release.
- Subspace's own product descriptor (`known.rs`: its Spaces, themes list,
  bundle id, update repository) and the product-metadata helper the
  packaging scripts use. The Framework keeps the descriptor types.
- Packaging, release, capture and install scripts; the product docs; the
  change records.

## Layer rules

- Framework: today's rules between services, board-kit, runtimes, clients
  and host, checked from Cargo metadata by the Framework's own architecture
  check. The gallery may use what any Space may use.
- Desktop: products → Spaces → `subspace-framework`. A Space or a
  desktop client depends on `subspace-framework` without the `host` feature,
  on its own crates and on desktop clients, never on another Space. Because
  Cargo unifies features across a workspace build, the desktop architecture
  check also rejects `subspace_framework::<host module>` in Space and client
  source.

## Versions

The Framework has its own version, starting at `0.1.0`; Subspace Desktop
keeps its own (`0.8.0` today). Framework crates share one version and pin
each other to it. `product.json`'s `subspace_version` is the Framework's
version; nothing requires the two to be equal.

## Ready to publish

- Every Framework crate has `publish = true`, a description, the licence,
  `version` on each internal dependency, and a readme where it is the entry
  point. `LICENSE` (Apache-2.0, as every manifest already declares) and
  third-party notices ship inside the crates that need them.
- `cargo package --workspace` with verification passes in
  `subspace-framework/`, and `make check` there runs it.
- `scripts/publish.sh` publishes the crates in dependency order from
  `CARGO_REGISTRY_TOKEN`, with a dry-run mode, and waits out crates.io's
  limit on new crates (five at once, then one every ten minutes). It is not
  run in this change.
- `docs/publishing.md` in the Framework says how to publish and how Subspace
  Desktop switches to the published crate.

## Gates

`make check` and `mise run quality` (Findings 0 → 0) pass in both folders;
the repository root's `make check` runs both; the pre-push hook runs both.
`make check-terminal` and `make interfaces` pass in `subspace-desktop/`. The
macOS packaging script tests pass, and a debug build of the app starts.

## Not in this change

- Publishing, and switching Subspace Desktop to the registry.
- Renaming `Dashboard` to `Space`, a builder API, or removing the remaining
  Subspace names from Framework text (the white-label work in the research).
- Splitting the repository.

## Decisions made while building it

Round 1 (the split itself) settled these. Where a decision departs from a
rule above, the rule was impossible or worse; the reason is given.

- **Facade features.** `default = ["space"]`. `space` is the services,
  board-kit (`dashboard`, `lucide`) and `client_github`. `product` is always
  present, so `default-features = false` gives only the GPUI-free product
  types; the Subspace App's packaging helper uses that to build quickly.
  `agent` adds `agent` (Board Lab needs it; it is not a host crate). `host`
  adds `shell`, `board_command`, `chat`, `mcp`, `report_issue`, `search_ui`
  and `update`, and implies `agent`. `search-onnx`, `component-gallery`
  (`component_gallery`), `test-support` (forwards each enabled crate's own
  `test-support` and exposes `client_test_support`) and `capture` (implies
  `host`). Module names drop the `subspace_` prefix; the gallery's module is
  `component_gallery`, without `_board`.
- **Internal pins.** Framework crates depend on each other with
  `version = "=0.1.0"`, so a published set only ever resolves with itself.
  Subspace Desktop asks for `version = "0.1.0"`.
- **Terminal manifests carry their own Framework path.** The terminal panel
  and runtime are excluded workspaces with their own lockfiles; an excluded
  package cannot inherit `workspace.dependencies` or `[patch]`. Each names
  `../../../subspace-framework` and the Markdown patch itself, as it already
  did for the patch. That makes four cross-folder paths, not the two rule 4
  allows; they go when the terminal crates can join a workspace.
- **No shared target directory.** Each workspace keeps its own `target/`
  (Mr Boxington's cache already deduplicates the compilation work), so no
  configuration in one folder names a path outside it.
- **The Subspace App's descriptor.** `known.rs` became the desktop crate
  `subspace-descriptor` (`products/descriptor`) with the
  `subspace-product-metadata` helper binary, which now takes
  `<product> <version>` and reports the Framework's own version as
  `subspace_version`. The descriptor decides on self-updates from the app's
  version, not the Framework's. The Framework keeps the descriptor types,
  validation, `DEFAULT_THEME` and `DEFAULT_THEME_DOCUMENT`, and gains
  `subspace_product::fixture` (behind `test-support`) for its own tests.
  `ProductContext::known` was unused outside tests and is gone.
- **Capture feature.** The product declares `capture =
  ["subspace-framework/capture"]`; scripts build `--features capture`
  instead of naming `subspace-shell/capture`.
- **Build stamp (round 2).** The product stamps its own build:
  `products/subspace/build.rs` prints the directives that
  `products/subspace/build/stamp.rs` computes (commit, dirty flag, branch,
  build time, target, profile, kind and host; it re-runs when this
  workspace's sources or the checked-out commit change), and the app's tests
  include `stamp.rs` to test it. `main.rs` builds a
  `subspace_framework::product::BuildInfo` from those variables, with its
  repository URL, and passes it in `ProductBuild::build`. The Framework's
  shell has no build script, runs no git and hard-codes no repository: About
  shows a Repository line only when `BuildInfo::repository` is set.
  `ProductContext` carries the build; a context nobody stamped
  (`BuildInfo::UNSTAMPED`, as in the shell's tests) is a test build of host
  `unknown`. About, `--version`, diagnostics and the dev, test and capture
  version markers behave as before.
- **Component gallery's Kit usage.** "Used" now means used by the
  Framework's own crates; only those are scanned. `MessageScroller`, used
  only by the IRC Space, is now marked unused.
- **Source-boundary tests.** The anchored-menu boundary test scans the
  Framework's crates; a copy in `products/subspace/tests` scans the desktop's
  Spaces, clients, products and terminal crates.
- **Example themes.** The Actions Space's `flow-graph` example has its own
  copy of the two example themes instead of reading the Framework's.

Round 2 (tooling, checks and docs) settled these:

- **Skills.** Each folder keeps its own copy of the upstream `gpui-kit` and
  `gpui-kit-design-guides` skills under `.agents/skills/`, so either works
  alone. The desktop keeps `developer-checklist` and `web-space`; the
  Framework has its own `framework-checklist`.
- **Docs split.** Framework docs: the Space author material
  (`board-integration`, `board-checklist`, `board-guide-template`,
  `ui-components`, `feature-flags`) and what every app gets (`appearance`,
  `developer-tools`, `logging`, `search`, `report-issue`, `cli`,
  `web-research`, `agent-chat`, `pi-chat`, `acp-chat`, `mcp`) plus the
  Framework terms of the glossary. Desktop docs keep the app, its Spaces,
  installation, releases, ways of working, settings, the terminal and the
  change records. The Subspace App's inventories that lived in Framework
  pages (the MCP tool table, the declared and candidate contracts, the board
  audit and gaps) moved to the desktop's `docs/spaces-audit.md`, which the
  product tests read; the Framework pages describe the mechanism. The
  application icons section moved to the desktop's `docs/app-icons.md`. The
  product site (`toc.yml`) no longer lists Framework pages; its two product
  pages that pointed at Framework pages use GitHub URLs, because the site
  cannot publish files outside the folder.
- **Board-guide template.** It is the Framework's. The desktop's guide check
  cannot read it, so it holds the ten H2 headings as a constant with a
  comment naming the template; the Framework's check still reads its
  template.
- **Third-party notices.** The Framework keeps `licenses/` for its own
  vendored and adapted code (Flint charts, themes, the Mermaid renderer); the
  desktop keeps the full set the app bundle ships, as plain text without
  links into the Framework.
- **Quality baselines.** See [delivery](delivery.md#quality-baselines): the
  Framework is at 0 findings; the desktop's baseline records 45 findings that
  the single-tree analysis had hidden.
- **The shared product build helper is gone.** With one product,
  `products/build.rs` became `products/subspace/build.rs`; the architecture
  check no longer allows a shared build-script include.
- **Gallery text.** The Inconsistencies section names its Subspace App
  sources as examples ("Subspace App example: spaces/github/…"); "used in"
  notes keep naming Spaces as examples. Kit usage counts the Framework only.
- **CHANGELOG.** The desktop's `CHANGELOG.md` is a one-line stub, not
  maintained, so the Framework has none yet.

Round 3 (ready to publish) settled these:

- **Manifests.** Every crate: `publish = true`, a description (internal
  crates say apps depend on `subspace-framework`), Apache-2.0,
  `rust-version = "1.99"`, keywords, the `gui` category and a docs.rs
  `documentation` link. No `repository` or `homepage` while the repository
  is private (an owner decision in the Framework's `docs/publishing.md`).
  The facade ships only `src/`, `README.md` and `LICENSE`; its README is the
  crate's front page and its two examples compile as doctests with `host`.
- **Licence and notices.** Each crate folder holds a copy of the Framework's
  `LICENSE`; `scripts/check-package.py` (in `make check`) checks that each
  package ships it unchanged, that every embedded third-party file ships its
  licence text and is named in the crate's `NOTICE`, and that no packaged
  file credits other work without being listed.
- **The packaging gate lives in `make check`.** Full verification (each
  crate built from its packaged copy against the others') takes about 30 s
  warm and about 6 min from a cold target, so it runs offline in every
  `make check`. `make publish-check` repeats it online against the live
  index before a release and stamps the commit for `scripts/publish.sh`.
- **Personal data removed.** Test fixtures that used a real person's name and
  handles now use fictional ones. Nothing was renamed for white-labelling;
  the owner's decisions are in the Framework's `docs/publishing-audit.md`.
- **Pre-release planning follows the Framework while it is path-built.**
  `scripts/prerelease-common.sh` reads the desktop's own `Cargo.toml`; while
  `subspace-framework` has a `path` inside the same repository, the plan, the
  notes and the version bump consider that folder too (a git pathspec, not a
  file read). With the published crate, only the desktop folder counts.
- **The desktop's Markdown patch may be its own copy.** The desktop
  architecture check accepts `markdown` patched to the Framework's
  `vendor/markdown` or to `subspace-desktop/vendor/markdown`, the copy a
  repository split needs.
- **Desktop quality at 0.** The 45 hidden findings were fixed: tests for the
  untested functions, per-enum text in one match each, IOSP splits, a split
  test module and a removed test-only accessor. Suppressions, each with a
  reason, only where the codebase's conventions apply: nine
  `qual:allow(dry, repeated_matches)` on single-match enums rustqual groups
  by shape (the convention in the Framework's `product/src/channel.rs`), and
  eleven `qual:allow(test_quality, untested)` on the terminal's paint
  functions, which run only inside GPUI's `Element::paint`.
