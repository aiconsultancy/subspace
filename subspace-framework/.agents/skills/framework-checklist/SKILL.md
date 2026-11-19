---
name: framework-checklist
description: Pre-commit, pre-push and pull request checklist for the Subspace Framework. Use before committing, pushing or opening a pull request in subspace-framework, when asked "am I ready to commit" or "is this done", and whenever a change touches a Framework crate, the facade, shared UI, contracts, dependencies or docs. Routes each kind of change to the rules it must follow.
compatibility: The subspace-framework folder with mise and prek.
metadata:
  owner: aiconsultancy/subspace
  last-reviewed: "2026-11-17"
---

# Framework checklist

Paths are relative to `subspace-framework/`. The rules live in
[AGENTS.md](../../../AGENTS.md) and the [documentation index](../../../docs/README.md);
this skill only routes to them.

## 1. Classify the change and read its rules

- [ ] **Shared UI** (a component, the shell, themes): read the
  `gpui-kit-design-guides` and `gpui-kit` skills and the guide files; follow
  the [component rules](../../../docs/ui-components.md#rules); a new or changed
  component has its Component gallery card and catalogue name. Screenshots
  come from an app's capture build ([UI change evidence](../../../AGENTS.md#ui-change-evidence)).
- [ ] **A crate or its dependencies**: the layer rules in
  [AGENTS.md › Structure](../../../AGENTS.md#structure); a new crate gets a
  facade module and feature; `mise run architecture`.
- [ ] **What a board gets** (a `Dashboard` hook, a service, a contract): update
  [What the Framework gives a board](../../../docs/board-integration.md) and the
  [board checklist](../../../docs/board-checklist.md).
- [ ] **Public API**: apps depend on it; note breaking changes for the next
  version ([Versions and publishing](../../../AGENTS.md#versions-and-publishing)).
- [ ] **Dependencies**: `mise run audit`; keep `deny.toml` current.
- [ ] **Docs**: every page under `docs/` and every Space guide is linked from
  the index (`make check` enforces it).

## 2. Gates

- [ ] `make check` passes.
- [ ] `mise run quality` passes and `rustqual . --findings` shows no new
  findings in code you touched.
- [ ] Apps built on the Framework in the same repository still pass their own
  `make check` (in this repository: `../subspace-desktop`).
- [ ] Commit only when the owner asks; push with the hook, never
  `--no-verify`.
