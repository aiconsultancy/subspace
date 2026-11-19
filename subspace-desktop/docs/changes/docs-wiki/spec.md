# Documentation index and ways of working

- `docs/README.md` is the documentation index. GitHub shows it when browsing
  `docs/`. The published site keeps its own landing page,
  `docs/subspace/index.md`, and its `toc.yml` files do not change.
- The index groups pages by area: ways of working, architecture, code quality
  and checks, boards, Framework and shared UI, chat and MCP, settings,
  releases, the product site and change records. Each area opens with "if you
  want to know about X, read Y" and gives one line per page.
- `docs/ways-of-working.md` covers conventional commits (types, scopes,
  breaking changes), branches and pull requests, the hook gates, versioning
  and the release steps. A release with any `feat` bumps the minor version;
  fix-only releases bump the patch; breaking changes bump the minor while the
  version is below 1.0. It records that 0.2.1 should have been 0.3.0.
- AGENTS.md tells agents to start at the index and follow Ways of working. It
  remains authoritative for agent rules and links rather than copies.
- `docs/changes/README.md` explains the record convention. Individual records
  are not listed.
- `scripts/check-docs-index.py`, run by `make check` through the `docs-index`
  task, follows local links from the index through `docs/` pages. It fails
  when a `docs/**/*.md` page or a `spaces/*/help.md` guide is not reached, or
  when a local link or heading fragment on a reached page is broken. Change
  records, `docs/subspace/includes/` fragments and site-generated `obj/docs/`
  includes are excluded. `scripts/test-docs-index.py` covers it with fixtures.
