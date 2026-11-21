---
name: developer-checklist
description: Pre-commit, pre-push and pull request checklist for the Subspace desktop app. Use before committing, pushing, opening or merging a pull request, or reporting work as done in subspace-desktop; when asked "am I ready to commit", "is this done", "check my change", "ship this" or "open a PR"; and whenever a change touches UI, a board, the framework, board-to-board contracts, settings, dependencies, docs or a release. Routes each kind of change to the rules it must follow.
compatibility: Subspace desktop repository (aiconsultancy/subspace, subspace-desktop/) with mise, prek and the gh CLI.
metadata:
  owner: aiconsultancy/subspace
  last-reviewed: "2026-11-17"
---

# Developer checklist

Run this before every commit and again before pushing or opening a pull
request. Paths are relative to `subspace-desktop/`. The rules live in the
linked documents; this skill only routes to them. Start from the
[documentation index](../../../docs/README.md) when unsure where a rule is.

## 1. Before you edit

- [ ] Synced: fetch `main` and `feat/subspace-desktop`. If `main` has content
  your branch lacks, bring it in first; rebase onto the remote branch when
  your tree is clean. Never check out another branch in a shared checkout.
- [ ] Hooks installed: `make hooks` (prek). `.git/hooks/pre-commit` and
  `pre-push` must exist. Commits run hygiene, the format check and gitleaks;
  pushes run the [checks for changed projects](../../../docs/ways-of-working.md#gates).

## 2. Classify the change and read its rules

Tick every kind that applies, then read the linked rules before finishing.

- [ ] **UI change** (anything Subspace shows: layout, styling, controls,
  icons, themes, text). Read the `gpui-kit-design-guides` and `gpui-kit`
  skills and the guide files themselves; follow the Framework's component
  rules ([ui-components.md](../../../../subspace-framework/docs/ui-components.md#rules)):
  GPUI Kit component first, then a shared component composed from GPUI Kit
  in the Framework's board kit, custom last with a recorded reason;
  everything is a component with an interactive example on the Component
  gallery board. Capture screenshots as
  [UI change evidence](../../../AGENTS.md#ui-change-evidence) requires
  (Cupertino Dark, before and after, fixture data, at most 1600 px), only
  with `make capture`, never a capture launched by hand.
- [ ] **New or changed Space** (a board; the same rules): complete the
  Framework's [board checklist](../../../../subspace-framework/docs/board-checklist.md) in the change
  record, using [What the Framework gives a board](../../../../subspace-framework/docs/board-integration.md);
  one crate depending on `subspace-framework` only, one dependency line, one
  `BoardRegistration`, a `BoardDescriptor` in `products/descriptor`; a
  `help.md` guide; its tools and contract roles in
  [docs/spaces-audit.md](../../../docs/spaces-audit.md).
- [ ] **Framework change** (anything in `../subspace-framework`): follow the
  Framework's [AGENTS.md](../../../../subspace-framework/AGENTS.md) and its
  `framework-checklist` skill. Run the desktop code checks if the desktop also
  has code or build input changes. Commit the Framework change separately, first
  ([AGENTS.md › Structure](../../../AGENTS.md#structure)).
- [ ] **Desktop crate or dependency change** (`spaces/**`, `clients/**`,
  `products/**`, `terminal/**`): the one-import and layer rules in
  [AGENTS.md › Structure](../../../AGENTS.md#structure); `mise run
  architecture`.
- [ ] **Board-to-board communication**: only through contracts and the broker
  ([Board-to-board contracts](../../../../subspace-framework/docs/board-integration.md#board-to-board-contracts));
  update the declared contracts in [docs/spaces-audit.md](../../../docs/spaces-audit.md).
- [ ] **Settings, configuration or credentials**: the private config-file
  editor, Save boundaries and credential priority in
  [AGENTS.md › Structure](../../../AGENTS.md#structure); never copy secrets
  into UI state or logs.
- [ ] **Dependencies**: `mise run audit`; keep `deny.toml` entries current
  ([Dependency audit](../../../AGENTS.md#dependency-audit)).
- [ ] **Docs**: every new or moved page under `docs/` is linked from the
  index (`make check` enforces it); the tool table in `docs/spaces-audit.md`
  and the Framework's `mcp.md` and `cli.md` match the code.
- [ ] **Release**: never bump a version by hand. Release Please's pull
  requests carry every version and changelog change
  ([Releases](../../../docs/ways-of-working.md#releases)); after the app's
  merges, `make release-macos` on its tag publishes the draft.

## 3. Record and commit

- [ ] Change record under `docs/changes/<slug>/` (intent, spec, delivery,
  tasks, evidence).
- [ ] Commit only when the owner asks. Never commit to `main`.
- [ ] Conventional commits, `type(scope)!: summary`, one logical change each,
  no merge noise ([Commits](../../../docs/ways-of-working.md#commits)).

## 4. Gates before pushing

- [ ] Run `make check` for desktop code or build input changes. For a
  Markdown-only desktop change, run `mise run --skip-tools board-docs ::: docs-index`.
- [ ] `MISE_ENV=terminal make check-terminal` passes when the terminal crates
  or a Framework API they use changed.
- [ ] `make interfaces` passes when the product interface, tools, MCP or CLI
  changed.
- [ ] For Rust changes, `mise run quality` passes and `rustqual . --findings`
  shows no new findings in code you touched (IOSP, functions under 60 lines, at most 5
  parameters, no `unwrap` outside tests).
- [ ] Push with the hook; never `--no-verify`.

## 5. Pull request

- [ ] Use the template's sections (`gh pr create --body` skips it).
- [ ] Visual changes embed commit-pinned screenshots; integration merges into
  `main` embed each included change's key images or link the PRs that carry
  them ([Before opening or merging a pull request](../../../docs/ways-of-working.md#before-opening-or-merging-a-pull-request)).
- [ ] Merge `feat/subspace-desktop` into `main` through a temporary
  `merge/feat-to-main` branch, never the integration branch itself
  ([Integration merges](../../../docs/ways-of-working.md#integration-merges-into-main)).

Read [references/gotchas.md](references/gotchas.md) when a hook, push or
build fails unexpectedly.
