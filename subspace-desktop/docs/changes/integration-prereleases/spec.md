# Contract

`make release-macos-prerelease` requires an attached, clean, pushed commit.
The base follows the [versioning rule](../../ways-of-working.md#versioning),
using conventional commits that touch `subspace-desktop` since the newest
published stable tag. Any `feat` bumps the minor version; `!` or a
`BREAKING CHANGE` footer bumps the minor below 1.0 and the major from 1.0.
`fix`/`perf` alone bump the patch. The highest applicable bump wins. With no
app-changing commits, the planner refuses a build. Missing stable tags must
be fetched before planning.

The version combines that base, a normalized branch slug, HEAD's UTC committer
minute and its seven-character SHA, for example
`0.3.0-feat-subspace-desktop.202611151345.gabc1234` after the features since
stable `0.2.1`. Reserved channel slugs are rejected. Shared planning and
stamping code serves the nightly, alpha and beta paths with their existing
stable-plus-one-patch base unchanged.

The existing Mac signing/notarization path publishes the archive and checksum
as a GitHub pre-release with `--latest=false`. The temporary manifest and
lockfile stamps are restored on success or failure.

Integration versions keep stable product identity and have no update source
or updater. No supported release channel accepts their versions as updates.
The existing no-update-source path shows no update controls. No popover
wording or other GUI change is required, so there is no screenshot work.
Explicit installer versions support snapshots without broadening latest
selection. Integration snapshots do not add documentation versions.
