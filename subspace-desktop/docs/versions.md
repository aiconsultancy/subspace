# Documentation versions

The site root contains **main / unreleased documentation**. It can describe
changes that are not in a published app. Each page identifies its documentation
version above the title.

Frozen versions contain the complete site built from one stable Subspace
release tag. The snapshot pins downloads to that tag.

[!INCLUDE [Available stable documentation](../../../obj/docs/includes/versions.md)]

History starts with the first stable releases that include the documentation
archive builder. Older releases, including the 1.x releases made before the
version reset to 0.1.0, have no documentation archive and are not backfilled.
Nightly, alpha, beta and integration pre-releases never add documentation versions.

Integration builds of `feat/subspace-desktop` are download-only snapshots,
published separately from stable releases. Their base is the next release
version derived from conventional commits since the newest published stable
tag, following [Versioning](ways-of-working.md#versioning). Any `feat` bumps
the minor version; a breaking change (`!` or a `BREAKING CHANGE` footer) bumps
the minor version below 1.0 and the major version from 1.0 onward. `fix` and
`perf` alone bump the patch version. The highest applicable bump wins; without
app-changing commits the integration planner refuses a build. Only commits
touching `subspace-desktop` count.

The version adds the lower-case branch slug, the HEAD commit's UTC committer
minute and its seven-character SHA. Today's features since stable `0.2.1`
give `0.3.0-feat-subspace-desktop.202611151345.gabc1234`, which sorts above
`0.2.1` and below stable `0.3.0`. This commit-based base applies only to
integration builds. Nightly, alpha and beta retain the existing newest-stable
plus one patch base, such as `0.2.2-nightly.…` after stable `0.2.1`.
Rebuilding the same commit against the same published stable base produces
the same version. These builds keep the Subspace name, bundle id, icons and
data locations. On macOS, an explicit integration install replaces the stable
`~/Applications/Subspace.app` and its CLI symlink (normally
`~/.local/bin/subspace`). The status-bar version distinguishes
the snapshot. Integration builds show no update controls and never offer
updates, including a downgrade to stable. Stable, nightly, alpha and beta
never offer integration builds as updates.

On a Mac, run `make release-macos-prerelease` from the clean, pushed integration
branch. To install a published snapshot, use
`sh scripts/install-release.sh --version 0.3.0-feat-subspace-desktop.202611151345.gabc1234`.
The PowerShell installer (`install-release.ps1`) does not accept integration
versions; use the shell installer for them.
See [Integration builds](installation.md#integration-builds) for signing,
publication and installation details.

Each stable release stores its complete site as `docs-site.tar.gz`. Every
deployment downloads the available stable archives again, so publishing main
docs does not remove older versions. Existing archives are not overwritten.
Each archive records the source tag, commit and download releases used in its
build. It keeps its own shared guides, assets and search index.
