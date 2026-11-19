#!/usr/bin/env bash
# Refresh the lockfiles on each open Release Please branch. Release Please
# moves the versions in the manifests but cannot run Cargo; this commits
# what scripts/sync-lockfiles.py changes, after Cargo accepts the result with
# --locked, and pushes it to the branch. Run by .github/workflows/release.yml
# from the repository root, with GH_TOKEN and a checkout that can push.
# Environment for tests: RELEASE_SYNC and RELEASE_VERIFY replace the sync and
# the Cargo check.
set -euo pipefail

repo="${GITHUB_REPOSITORY:?}"
summary="${GITHUB_STEP_SUMMARY:-/dev/null}"
sync="${RELEASE_SYNC:-python3 scripts/sync-lockfiles.py}"
verify="${RELEASE_VERIFY:-verify_locked}"
workspaces=(subspace-framework subspace-desktop subspace-desktop/terminal/panel subspace-desktop/terminal/runtime)

verify_locked() {
	local workspace
	for workspace in "${workspaces[@]}"; do
		cargo metadata --locked --format-version 1 --manifest-path "$workspace/Cargo.toml" >/dev/null
	done
}

git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
start="$(git rev-parse HEAD)"
branches="$(gh pr list --repo "$repo" --base main --state open --label 'autorelease: pending' \
	--json headRefName --jq '.[].headRefName')"
for branch in $branches; do
	case "$branch" in
	release-please--branches--main--components--*) ;;
	*) continue ;;
	esac
	git fetch --quiet --depth=1 origin "refs/heads/$branch"
	git switch --quiet --force --detach FETCH_HEAD
	$sync
	changed="$(git diff --name-only)"
	if [[ -z "$changed" ]]; then
		echo "$branch: the lockfiles are up to date." >>"$summary"
		continue
	fi
	if grep -v '/Cargo\.lock$' <<<"$changed" >&2; then
		echo "$branch: the sync changed files other than lockfiles" >&2
		exit 1
	fi
	$verify
	git commit --quiet --all --message 'chore: refresh the lockfiles for the release'
	# Not forced: if Release Please rewrote the branch meanwhile, the next run
	# refreshes it.
	git push --quiet origin "HEAD:refs/heads/$branch"
	echo "$branch: refreshed $(wc -l <<<"$changed" | tr -d ' ') lockfiles." >>"$summary"
done
git switch --quiet --force --detach "$start"
