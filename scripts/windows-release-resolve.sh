#!/usr/bin/env bash
# Resolve the tag windows-release.yml was asked to build to the one commit the
# run builds and signs, or refuse it. Anyone who can push a tag could otherwise
# get any commit signed, so only a desktop release commit that reached main
# through Release Please's pull request is accepted:
#
# - TAG is a stable subspace-desktop-v<major>.<minor>.<patch> tag, read from
#   refs/tags/ only, so a branch of the same name changes nothing;
# - its commit is EXPECTED_COMMIT when that is set (release.yml passes the
#   release's commit);
# - the commit is in the first-parent history of the checked-out HEAD, which
#   is main: main's own history, not a commit from inside a merged pull
#   request that review may never have seen as it stands;
# - GitHub records it as the merge commit of a merged Release Please pull
#   request for the desktop into main (after a rebase merge, GitHub records
#   only the last commit, so no other pull request's commit passes);
# - it changes only the files Release Please changes;
# - its package-release.sh can skip running the app, which cannot start on
#   the Windows runner.
#
# Any failure, including a failed or unexpected GitHub API answer, refuses the
# tag. Writes tag, version, commit and archive to GITHUB_OUTPUT. Run by
# .github/workflows/windows-release.yml from a full-history checkout of main,
# with GH_TOKEN, TAG, EXPECTED_COMMIT, TARGET and GITHUB_REPOSITORY.
set -euo pipefail

tag="${TAG:-}"
expected="${EXPECTED_COMMIT:-}"
target="${TARGET:?}"
repo="${GITHUB_REPOSITORY:?}"
output="${GITHUB_OUTPUT:?}"
summary="${GITHUB_STEP_SUMMARY:-/dev/null}"
release_branch=release-please--branches--main--components--subspace-desktop

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
# grep reads the whole list: stopping early would kill git with SIGPIPE and
# fail the pipeline under pipefail.
if ! git rev-list --first-parent HEAD | grep -xF "$commit" >/dev/null; then
	echo "$tag points at $commit, which is not in main's first-parent history; only those commits are built and signed" >&2
	exit 1
fi
prs="$(gh api "repos/$repo/commits/$commit/pulls")"
if ! jq -e --arg c "$commit" --arg repo "$repo" --arg branch "$release_branch" '
	[.[] | select(.merge_commit_sha == $c and .merged_at != null
		and .base.ref == "main" and .head.repo.full_name == $repo
		and .head.ref == $branch and .user.login == "github-actions[bot]")] | length == 1' <<<"$prs" >/dev/null; then
	echo "$commit is not the merge commit of a merged Release Please pull request for the desktop" >&2
	exit 1
fi
allowed='^(\.release-please-manifest\.json|subspace-desktop/(CHANGELOG\.md|Cargo\.(toml|lock)|terminal/(panel|runtime)/Cargo\.(toml|lock)))$'
changed="$(git diff --no-ext-diff --no-textconv --no-renames --name-only "$commit^1" "$commit")"
if [[ -z "$changed" ]] || grep -vE "$allowed" <<<"$changed" >/dev/null; then
	printf '%s is not a Release Please release commit; it changes:\n%s\n' "$commit" "$changed" >&2
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
echo "$tag is $commit, a release commit in main's first-parent history." >>"$summary"
