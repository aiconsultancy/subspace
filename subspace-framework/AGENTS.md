# Agent instructions for the Subspace Framework

The Subspace Framework is the set of crates desktop dashboard apps are built
from, with Rust and GPUI Kit, behind one facade crate, `subspace-framework`.
Read [README.md](README.md) for the facade, its features, the layout and the
dashboard interface. This folder is a project of its own: nothing in it
reads, names or depends on an app built from it.

## Documentation

Start each session at the [documentation index](docs/README.md). It lists
every page by area with a one-line summary; read the pages relevant to the
task rather than all of them. When you add a page under `docs/` or a Space
guide, link it from the index or a page it links; `make check` fails
otherwise.

Use conventional commit messages (`type(scope)!: summary`), one logical
change per commit. Do not commit, push or add a remote unless the owner asks.

## Before every commit and push

Load the `framework-checklist` skill
([.agents/skills/framework-checklist](.agents/skills/framework-checklist/SKILL.md))
and work through it before every commit, every push and every pull request.

## Canonical design and coding guides

Load and use both `gpui-kit-design-guides` and `gpui-kit`. If either skill is
missing, install that skill from
[`longbridge/gpui-kit`](https://github.com/longbridge/gpui-kit.git).

Before changing UI, interaction, interface language, layout, styling,
components, or application architecture, read and follow the canonical guides:

- [Design Guides](https://gpui-kit.com/docs/design-guides.md)
- [Coding Guides](https://gpui-kit.com/docs/coding-guides.md)

Read the guide files themselves, not only the skill summaries. These guides are
requirements, not optional inspiration. Do not copy generic web conventions,
infer a design system from one existing screen, or add a control merely because
the underlying feature exists. Preserve the documented task hierarchy,
interaction promise, desktop conventions, spacing, alignment, theme tokens,
component boundaries, naming, and crate architecture.

Review the finished work against both guides before considering it complete.

## Terminology

The **Subspace Framework** is this folder's crates. An **App** is a binary
built from the Framework with a selected set of boards (code: *product*). See
the [glossary](docs/glossary.md).

**Space** is the current name for a **board**; the terms are
interchangeable. A Space is an alias for a board, so every rule here about
boards applies to Spaces. Prefer Space in new text, docs and names. Existing
references to boards are fine and need not be renamed.

## Commands

Use the make targets. Plain `make` lists them.

- `make test` for the tests, `make check` before reporting work as done
  (architecture, Space-guide and documentation-index checks, format check,
  unused-dependency check with cargo-shear, clippy with warnings denied,
  tests, the README's examples, packaging every crate and building each from
  its packaged copy, the package check, and the publish-script fixtures).
  Packaging builds in `target/package-build` and rebuilds the Framework's own
  crates each run (`scripts/package-workspace.sh` says why): about 80 s warm,
  about 5 min cold.
- `make publish-check` repeats the packaging online, against the live index,
  as the `publish-framework` workflow does before each upload.
  `make publish-dry-run` and `make publish` are in
  [publishing](docs/publishing.md).
- `make build`, `make format`, `make quality`, `make audit`.
- In non-interactive shells, run Cargo through mise (`mise exec -- cargo ...`)
  so builds use the Mr Boxington cache.
- prek runs the hooks (`make hooks` installs them). Commits run the format
  check. The pre-push hook selects checks by project and file type. See
  [Gates](../subspace-desktop/docs/ways-of-working.md#gates). Vendored files
  and `.agents/skills` keep their upstream bytes.
- There is no app in this folder. To see a Framework change in an app, build
  the app that depends on this folder (in this repository, the Subspace App
  in `../subspace-desktop`, which depends on it by path).

## Structure

- The root package is the facade crate (`src/lib.rs`). It re-exports each
  Framework crate as a module named after it without the `subspace_` prefix,
  behind the features the README lists. A new crate gets a facade module and
  a feature line; Spaces never need `host`.
- Layers: `services/`, `board-kit/`, `runtimes/`, `host/`, `clients/` and
  `spaces/` (the component gallery, the catalogue Space). Internal normal,
  build and dev dependencies follow these rules:
  - Host may depend on host, board-kit, services, runtimes and clients.
  - Spaces and their owned crates (`spaces/<id>/crates/*`) may depend on
    board-kit, services, runtimes, clients and their own crates, but not host
    or other Spaces' crates.
  - Board-kit may depend on board-kit and services, but not clients, host,
    runtimes or Spaces.
  - Runtimes may depend on runtimes, board-kit, services and clients.
  - Clients may depend on clients and services. `clients/test-support`
    (offline HTTP fixtures) is a dev-dependency only, except as the facade's
    optional `test-support` feature.
  - Services may depend only on services. `services/agent-tools` and
    `services/agent-model` have no GPUI or Copilot SDK dependency of any kind.
  - No path dependency leaves this folder.
- `subspace-agent` lives in `runtimes/agent`; board-kit must not import or
  re-export it or the host chat panel (`host/chat`, `ChatPanel`).
  `host/search-ui` owns `SearchView`; board-kit must not define, import or
  re-export it.
- Source includes (`include!`, `include_str!`, `include_bytes!`, `#[path]`),
  including tests and examples, follow the same direction rules and never
  leave their package, except data under `docs/*.md`. Lower layers must not
  read host theme files; host artwork holds no copied Space artwork.
- Tool and `PATH` resolution lives in `subspace_core::path`; use `find_tool`.
  The architecture check rejects `split_paths(` and `var_os("PATH")`
  elsewhere, except the exemptions in `scripts/architecture_source.py`.
- Boards communicate only through contracts (`services/contracts`) routed by
  board-kit's broker, declared in `Dashboard::contracts`.
- Framework features every app gets (updates, About, MCP) belong in
  Framework crates. A product declares its identity, update source and build
  (`ProductBuild`, `ProductDescriptor`, `BuildInfo`); the Framework assumes
  no repository, product or Space of a particular app, and runs no git.
- The app supplies its GitHub OAuth App (`ProductBuild::github_oauth`) and
  the repository Report issue files into
  (`ProductBuild::report_issue_repository`). The Framework holds no client
  id, issue repository or `option_env!` for them: a published crate is not
  rebuilt when an app's environment changes. Without an OAuth App nothing
  offers or names Sign in with GitHub. Report issue is the optional
  `report-issue` feature (not part of `host`); code for it is behind
  `#[cfg(feature = "report-issue")]`, and `make check` tests and lints the
  shell with and without it. Tests use fictional values
  (`example-org/example-app`).
- Client calls block: run them in `cx.background_spawn`, never in render.
  Start panel loads from `ListDashboard::on_open`, not from `render`.
- Keep large caches out of `state.json`: write them with
  `prefs::write_cache` to a file beside it. Keep serde names equal to the
  service's JSON names.
- Colours come from `subspace_dashboard::tokens`, which maps the active GPUI
  Kit theme. `host/app/themes/one-dark.json` must agree with the default
  tokens; a test checks this.
- Service settings use the private config-file editor in `subspace-core`.
  Never copy saved tokens into UI inputs or write them to UI state.

`mise run architecture`, part of `make check`, enforces the dependency,
include, `PATH`, switch, icon-button, settings-dispatch and artwork rules
from Cargo metadata and the sources.

## Components

The canonical rules are in [docs/ui-components.md](docs/ui-components.md#rules),
with what `make check` enforces. In short:

1. Use a GPUI Kit component as it ships, themed by tokens, through its shared
   treatment where the catalogue lists one (`action`, `toggle::switch`,
   `anchored`, `tabs`).
2. If none fits, build a shared component in `board-kit/dashboard`
   by composing GPUI Kit components; extend an existing one first.
3. Draw a custom element only when no GPUI Kit composition can do it, and
   record the reason in the catalogue.
4. Everything visible is a component: no one-off styled controls in boards or
   the shell. Layout `div`s are not components.
5. Every shared component and shell surface has an interactive card on the
   Component gallery board with its copyable name. A new or changed component
   is not done until its card and its catalogue name are.

Existing direct controls are migration candidates, not permission to copy
them.

Every Space is laid out on the workspace shell (`dashboard::workspace`). The [design guide](docs/design-guide.md)
says what goes in each zone, the layout rules, and how to build a new Space
on it. A change to the shell updates that guide and its anatomy image.

## Conventions

- Use edition 2024, the workspace dependencies and lints, and `gpui-kit` as
  the one GPUI dependency. The workspace manifests are the source of truth.
- Import GPUI through `gpui_kit`. In test modules, import names explicitly
  rather than `use super::*`, because the glob brings GPUI's `test` macro,
  which shadows `#[test]`.
- No `unwrap` or `expect` outside tests and startup.
- New tabs and action buttons should always have an appropriate glyph by
  default. Every control still needs an accessible label. Treat the glyph as
  decorative unless it communicates status that the label does not.
- Use shared `IconButton::latched(bool)` for independent on/off toolbar modes.
  Use `toggle::switch` for booleans in forms, settings and dialogs, including
  settings panels opened from toolbars. In forms, use `toggle::toggle_row` with
  the label on the left and switch on the right. Preserve the owner's existing
  Save or immediate-apply boundary; Settings switches edit drafts until Save.
  Keep exclusive selection, task completion, disclosure and checked menu items
  in their native semantic controls. Do not restyle switches at call sites.
- Keep pure logic (grouping, selection, filters) out of render code and test
  it. Test observable interaction and critical layout invariants, not incidental
  rendering details.
- Record changes under `docs/changes/<slug>/` (intent, spec, delivery, tasks).
- Do not commit, push or add a remote unless the owner asks.

## UI change evidence

A change that alters what an app shows needs screenshots from an app's
isolated capture build with fixture data, as that app's AGENTS.md
describes, plus headless render tests here. A new or changed shared
component needs its Component gallery card.

## Versions and publishing

Every crate is published under Apache-2.0 (confirmed by the owner on 17
November 2026). Each ships `LICENSE` (a copy of this folder's Apache-2.0
text) and, when it embeds third-party material, a `NOTICE`
naming each file and its licence text; `scripts/check-package.py` (in
`make check`) enforces both and lists the third-party files. A new
third-party file needs its licence text in the crate, a `NOTICE` line and a
`THIRD_PARTY` entry. Never read files outside a crate's folder in non-test
code: the packaged crate would not build. Publishing and the version bump
are in [docs/publishing.md](docs/publishing.md).

0.1.0 is on crates.io. Releases come from Release Please (the repository
root's `release-please-config.json`): its `chore(framework): release X.Y.Z`
pull request moves `[workspace.package] version`, every internal pin (the
block between the `x-release-please` markers in `Cargo.toml`), the app's
requirement on the Framework, `CHANGELOG.md` and the lockfiles. The
repository's `release` workflow merges it nightly, tags
`subspace-framework-vX.Y.Z` and starts the `publish-framework` workflow on
the tag, which publishes through crates.io Trusted Publishing, with no
stored token. Never bump the version or tag by hand, never publish from a
branch, and never rename that workflow file without updating each crate's
trusted publisher. A published version is permanent, so a mistake is fixed
by a new version.

The Framework has its own version, `[workspace.package] version` in
`Cargo.toml` (`0.1.0`); every crate shares it and the crates pin each other
to it with `=`. Apps choose their own versions. Use semantic versioning: any
breaking change to a public API bumps the minor version while below 1.0.

## Dependency audit

- Run `mise run audit` after a dependency change. It runs `cargo deny check
  advisories` on this workspace's lockfile. It needs the network, so it is
  not part of `make check`.
- `deny.toml` lists each accepted advisory with its blocking dependency
  chain, its reason and a review date. Do not add an entry for an advisory
  that a compatible update fixes.

## Code Quality Rules

- Run `mise run quality` after changing Rust code. It compares the workspace with
  `rustqual-baseline.json` and fails only when the quality score drops below
  the baseline. The prek pre-push hook runs the same task.
- The check compares the overall score only. A new finding that leaves the
  rounded score unchanged can still pass, so run `rustqual . --findings` and
  do not add findings in the code you touch.
- After you fix findings, run `mise run quality-baseline` and commit the
  updated `rustqual-baseline.json`. Never refresh the baseline to hide a
  regression.
- rustqual runs with its built-in configuration. Do not add a `rustqual.toml`
  (`rustqual --init` writes one with stricter magic-number rules).
- `mise install` builds rustqual from source. The first install takes about a
  minute.
- Follow IOSP: every function is either an Integration (calls other functions, no own logic) or an Operation (contains logic, no calls to project functions). Never mix both.
- Keep functions under 60 lines and cognitive complexity under 15.
- Don't duplicate logic — extract shared patterns into reusable Operations.
- Don't introduce functions with more than 5 parameters.
- Every test function must contain at least one assertion (`assert!`, `assert_eq!`, etc.).
- For public-API functions that are intentionally untested in this crate, mark with `// qual:api` instead of writing a stub test.
