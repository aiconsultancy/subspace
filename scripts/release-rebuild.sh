#!/usr/bin/env bash
# Have Release Please rebuild each open release pull request that is behind
# main. Release Please leaves a release pull request alone while its
# description would not change, even after another release has changed the
# files both edit (the manifest, the lockfiles) and it no longer merges. A
# changed description makes the next Release Please run rebuild the branch on
# main and write the description again. Run by .github/workflows/release.yml
# before each Release Please run, with GH_TOKEN.
set -euo pipefail

repo="${GITHUB_REPOSITORY:?}"
summary="${GITHUB_STEP_SUMMARY:-/dev/null}"
pulls="$(gh pr list --repo "$repo" --base main --state open --label 'autorelease: pending' \
	--json number,headRefName --jq '.[] | "\(.number) \(.headRefName)"')"
while read -r number branch; do
	case "$branch" in
	release-please--branches--main--components--*) ;;
	*) continue ;;
	esac
	behind="$(gh api "repos/$repo/compare/main...$branch" --jq .behind_by)"
	if [[ "$behind" -eq 0 ]]; then
		continue
	fi
	body="$(gh api "repos/$repo/pulls/$number" --jq .body)"
	gh api --method PATCH "repos/$repo/pulls/$number" --silent \
		-f body="$body"$'\n\n'"<!-- behind main by $behind: rebuild -->"
	echo "#$number is behind main by $behind: asked Release Please to rebuild it." >>"$summary"
done <<<"$pulls"
