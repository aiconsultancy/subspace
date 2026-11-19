# Delivery

Documentation and checks only; Subspace's interface is unchanged, so there are
no screenshots.

## Checks

- `python3 scripts/check-docs-index.py` passes: the index reaches every
  `docs/` page and board guide, and every local link and heading resolves.
- `python3 scripts/test-docs-index.py` passes: fixtures cover a complete
  index, unlinked pages and guides, broken links and headings, links in code,
  skipped change records and a missing index.
- `make check` passes, including the new `docs-index` task and the existing
  board-guide check. The site `toc.yml` files are unchanged.
- The prek commit hooks pass on the changed files.

## Follow-up

The survey found pages that disagree with the developer overview or describe
workflows this repository does not have. They are linked from the index
unchanged:

- `docs/settings.md` lists General, Appearance, Board and GitHub sections;
  Settings now shows app pages and then an icon-led board tree.
- `docs/feature-flags.md` says board enablement needs a restart; the developer
  overview says Save applies it without one.
- `docs/board-lab.md` says debug builds enable Board Lab; the developer
  overview says it is disabled by default in both builds.
- `docs/installation.md` (pre-releases, GitHub release signing, pull request
  CI), `docs/terminal.md` and `docs/web-board-apps.md` describe GitHub
  workflows and CI runners. There is no CI yet. README.md also says the
  architecture check runs in CI.
- `docs/issues.md` describes the Issues board, which this branch does not
  include.
- Before this change, `docs/board-guide-template.md`, `docs/versions.md` and
  `docs/web-research.md` were linked from no page.
- The root `toc.yml` omits several developer pages, such as Agent chat, MCP
  and the CLI, and does not include the documentation index.
- The repository-root `prek.toml` comment names
  `subspace-desktop/subspace/prek.toml`; the file is `subspace-desktop/prek.toml`.
- `CHANGELOG.md` contains only its heading.
