## Summary

<!-- What changed and why. Link the issue (NET-…) and the change record. -->

## Checklist

Run through this before opening the pull request and again before merging.
See [Ways of working](https://github.com/aiconsultancy/subspace/blob/main/subspace-desktop/docs/ways-of-working.md#before-opening-or-merging-a-pull-request).

- [ ] Commits are conventional (`type(scope)!: summary`), one logical change each, rebased with no merge noise
- [ ] `make check` and `mise run quality` pass (the prek pre-push hook runs both; no `--no-verify`)
- [ ] Change record under `subspace-desktop/docs/changes/<slug>/` (intent, spec, delivery, tasks)
- [ ] New or moved pages under `docs/` are linked from `docs/README.md`; guides, `docs/mcp.md` and `docs/cli.md` match the behaviour
- [ ] Visual changes: before and after screenshots from the capture build with fixture data, embedded under **Screenshots** with commit-pinned URLs (see [UI change evidence](https://github.com/aiconsultancy/subspace/blob/main/subspace-desktop/AGENTS.md#ui-change-evidence)), or "no UI change". Integration merges into `main` embed each included change's key images, or link the pull requests that carry them. A pull request with visual changes and no images is not ready to merge.
- [ ] UI follows the [component rules](https://github.com/aiconsultancy/subspace/blob/main/subspace-framework/docs/ui-components.md#rules): GPUI Kit component first, then a shared component built from GPUI Kit, custom last with its reason recorded; every new or changed component has its Component gallery card and catalogue name
- [ ] Breaking changes (settings, config keys, CLI commands, MCP tools, stored state) are marked with `!` and a `BREAKING CHANGE:` footer
- [ ] Dependency changes: `mise run audit` passes
- [ ] No version, changelog entry or release tag changed by hand: Release Please's pull requests do that
- [ ] **Before merging: Claude Opus 5.5 (high) review clear** for the final head, recorded under **Reviews**. See [Pull request review clearance](https://github.com/aiconsultancy/subspace/blob/main/AGENTS.md#pull-request-review-clearance) for exceptions
- [ ] **Before merging: GPT-6.2 Sol (high) review clear** for the final head, recorded under **Reviews**
- [ ] If this adds or changes a board: [board checklist](https://github.com/aiconsultancy/subspace/blob/main/subspace-framework/docs/board-checklist.md) is completed in the change record, with each item implemented or "not applicable because …"

## Reviews

Both independent reviews must clear the final head before merging. Read-only
subagents and restricted CLI reviewers count. Record the method actually used.

| Reviewer | Method | Target and merge base | Reviewed range and final head | Verdict | Findings and resolution |
| --- | --- | --- | --- | --- | --- |
| Claude Opus 5.5 (high) | | | | | |
| GPT-6.2 Sol (high) | | | | | |

## Screenshots

<!-- ![caption](https://github.com/aiconsultancy/subspace/blob/<commit-sha>/subspace-desktop/docs/changes/<slug>/evidence/<file>.png?raw=true) -->
