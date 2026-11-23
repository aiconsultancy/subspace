#!/usr/bin/env bash
# Resolve the tag windows-release.yml was asked to build to the one commit the
# run builds and signs, or refuse it. The repository's writers are trusted, so
# this guards against mistakes, not against them:
#
# - TAG is a stable subspace-desktop-v<major>.<minor>.<patch> tag, read from
#   refs/tags/ only, so a branch of the same name changes nothing;
# - its commit is EXPECTED_COMMIT when that is set (release.yml passes the
#   release's commit);
# - the commit is on main (the checked-out HEAD), so nothing that never
#   reached main is signed;
# - its package-release.sh can skip running the app, which cannot start on
#   the Windows runner.
#
# A tag ruleset stops release tags being moved or deleted. Writes tag,
# version, commit and archive to GITHUB_OUTPUT. Run by
# .github/workflows/windows-release.yml from a full-history checkout of main,
# with TAG, EXPECTED_COMMIT and TARGET.
set -euo pipefail

tag="${TAG:-}"
expected="${EXPECTED_COMMIT:-}"
target="${TARGET:?}"
output="${GITHUB_OUTPUT:?}"
summary="${GITHUB_STEP_SUMMARY:-/dev/null}"

if [[ ! "$tag" =~ ^subspace-desktop-v((0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*))$ ]]; then
	echo "not a stable Subspace desktop release tag: $tag" >&2
	exit 1
fi
version="${BASH_REMATCH[1]}"
if ! commit="$(git rev-parse --verify --quiet "refs/tags/$tag^{commit}")"; then
	echo "no tag $tag" >&2
	exit 1
fi
if [[ -n "$expected" && "$commit" != "$expected" ]]; then
	echo "$tag points at $commit, not $expected, the release's commit" >&2
	exit 1
fi
if ! git merge-base --is-ancestor "$commit" HEAD; then
	echo "$tag points at $commit, which is not on main; only commits on main are built and signed" >&2
	exit 1
fi
if ! git grep -qF SUBSPACE_PACKAGE_SKIP_RUN_CHECK "$commit" -- subspace-desktop/scripts/package-release.sh; then
	echo "$tag predates Windows release builds: its package-release.sh cannot skip running the app" >&2
	exit 1
fi
{
	echo "tag=$tag"
	echo "version=$version"
	echo "commit=$commit"
	echo "archive=subspace-$version-$target.zip"
} >>"$output"
echo "$tag is $commit, on main." >>"$summary"
