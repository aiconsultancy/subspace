# Desktop defect repairs: tasks

- [x] Read developer-checklist and GPUI Kit's coding and design guides.
- [x] Fetch both integration and main; both match the starting base.
- [x] Install configured pre-commit and pre-push hooks with `prek install`.
  `make hooks` is documented but absent in this revision.
- [x] Attempt base captures before editing in dark and light themes.
- [x] Use sized SVG masks for themed marks; retain the compatibility image.
- [x] Keep the shell catalogue and existing gallery card accurate.
- [x] Add and verify Actions preference precedence coverage.
- [x] Update Actions Help and check all `actions.account` accesses.
- [x] Move lanes tests without content changes; verify five before and after.
- [x] Check mise's registry and pin ImageMagick; document both icon targets.
- [x] Add chosen-account selection and preserve credential-viewer verification.
- [x] Add four pure client regression tests; all pass with synthetic inputs.
- [x] Update Copilot Agents Help and account documentation; record tool-client gap.
- [x] Complete four-fix preparation gates; repeat all gates on the final five-commit head.
- [x] jt captured before/after dark/light images on macOS with `make capture`;
  crop and inspect all four native 2× title bars, keeping the originals intact.
- [x] Recheck developer-checklist before each commit and publication; the
  captain explicitly authorises publishing with captures pending and no PR.
- [x] Fetch integration again and rebase onto `a0a8f25`; the original
  `c9ecbf6` remains the before-capture revision. Fetch once more before publication.
- [x] Prepare Mac capture commands and a PR body for the captain to complete
  with published-head results; image placeholders stay pending.
- [x] Rebase the five fixes onto `c58a31de1985` without conflicts; preserve
  the original before (`c9ecbf6`) and after (`63bf985`) capture provenance.
- [x] Replace the PR's image placeholders with inspected, commit-pinned crops
  as part of evidence publication; the captain opens the PR.

Publication completion (signed heads, matching trees, clean checkout and
final gate results) is reported by the worker and in the external PR body.

## Board checklist: Actions

Each unchanged integration point is not applicable to this account-read fix:

- [x] ID, title, description and icon: not applicable; unchanged.
- [x] Product descriptor, registration and artwork: not applicable; unchanged.
- [x] Retain when disabled: not applicable; no process-lifecycle change.
- [x] List hooks/custom layout: not applicable; existing layout retained.
- [x] Components and gallery: no Actions control changes; the shell's existing
  `shell::toolbar` card and catalogue describe the logo repair.
- [x] Toolbar, status, badge and busy: not applicable; unchanged.
- [x] Palette commands: not applicable; no action added.
- [x] Keys and help footer: not applicable; unchanged.
- [x] Menus, dialogs, modal and dismiss: not applicable; unchanged.
- [x] Empty, loading and error states: not applicable; existing handling retained.
- [x] Quit warnings: not applicable; no new operation.
- [x] Settings description, notifications and background opt-ins: unchanged.
- [x] Service connections: read shared GitHub account after a board override;
  retain existing private credential handling and Save/restart boundary.
- [x] Feature flags: not applicable; no new feature.
- [x] Stored choices: existing board preference takes priority over shared state.
- [x] Framework store: not applicable; no record-persistence change.
- [x] Store-first loading: not applicable; no record-loading change.
- [x] Refresh interval, enabled and background I/O: unchanged.
- [x] Item search and recents: not applicable; unchanged.
- [x] Full-text index and opening results: not applicable; unchanged.
- [x] Cross-board reference metadata: not applicable; unchanged.
- [x] Read/write tools and approvals: not applicable; no tools added.
- [x] App-only tool host: not applicable; unchanged.
- [x] MCP/action inventory: not applicable; public interfaces unchanged.
- [x] Chat context: not applicable; unchanged.
- [x] Notification requests and defaults: not applicable; unchanged.
- [x] Process-owned notices and badges: not applicable; unchanged.
- [x] Help guide: document shared-account fallback and board-override priority.
- [x] Guide/developer index links: existing links retained.
- [x] Published, received and answered contracts: not applicable; none changed.
- [x] Framework globals: read shared GitHub preference, with no other board's
  private state or crate dependency.
- [x] Links/citations: not applicable; unchanged.
- [x] Logging and diagnostics: not applicable; no credential logging or paths.
- [x] Pure logic/headless tests: context test covers the preference chain.
- [x] UI screenshot evidence: jt's macOS dark/light title-bar crops, see delivery.

## Other affected boards

Issues only moves a test module without changing tests or integration; its
registration, UI, settings, storage, refresh, search, tools, chat, notifications,
contracts, links, diagnostics and guide are not applicable to this relocation.
The five lane tests are verified both before and after.

Component gallery already has the `shell::toolbar` card for this surface;
its descriptions and composition remain accurate. No new control or card is
needed. The catalogue documents the mark's rendering path and asset tones.

## Board checklist: Copilot Agents and PRs

The same unchanged-integration checklist above applies to both boards: IDs,
registration, artwork, retained processes, list hooks, components, controls,
palette, keys, modal handling, empty/error/loading presentation, quit warnings,
settings declarations, feature flags, storage, refresh, search, tool declarations,
chat context, notifications, contracts, links and diagnostic handling are not
applicable because this repair changes only client construction and account
selection. No new control or component is introduced.

- [x] Service connections/shared state: both UI clients receive
  `GITHUB_ACCOUNT_KEY`; no other board's private preferences are read.
- [x] Credential handling: viewer verification and token-source priority are
  retained; no credentials are copied into UI state or logs.
- [x] Stored choice: shared Settings preference survives restart as before;
  account changes still need a restart.
- [x] Help/docs: explain chosen-account priority and EMU fallback. Existing
  guide/index links remain intact; MCP/CLI interfaces do not change.
- [x] Agent-tool environment: explicitly outside scope; clients without saved
  preferences still use EMU detection, documented as follow-up work.
- [x] Pure tests: four offline regressions verify selection and viewer fencing.
- [x] UI evidence: jt's canonical macOS captures inspected and cropped; the
  captain retains responsibility for opening the PR.

Back to [intent](intent.md); see [specification](spec.md) and [delivery](delivery.md).
