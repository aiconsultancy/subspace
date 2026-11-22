#!/usr/bin/env bash
# Refresh the lockfiles on each open Release Please branch. Release Please
# moves the versions in the manifests but cannot run Cargo; this commits
# what scripts/sync-lockfiles.py changes, after Cargo accepts the result with
# --locked, and pushes it to the branch. Then it starts
# .github/workflows/lockfiles.yml and .github/workflows/stargate-pin.yml on the
# branch: a pull request opened or pushed with the workflow's token starts no
# pull_request run, so this is how a release pull request gets its lockfiles
# and Stargate pin checks. Run by
# .github/workflows/release.yml from the repository root, with GH_TOKEN, a
# checkout that can push and actions: write.
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

# Checks the branch's head as it is when the run starts: refreshed, or
# rewritten since by Release Please, whose next run refreshes and checks it.
# The Stargate pin check skips the Framework's branch itself; starting it there
# too gives every release pull request the same checks. A branch Release
# Please built before the check existed cannot start it; its rebuild on main
# can.
check() {
	gh workflow run lockfiles.yml --repo "$repo" --ref "$1"
	if [[ -f .github/workflows/stargate-pin.yml ]]; then
		gh workflow run stargate-pin.yml --repo "$repo" --ref "$1"
	else
		echo "$1: has no stargate-pin workflow yet, so its Stargate pin was not checked." >>"$summary"
	fi
}

git config user.name 'github-actions[bot]'
git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
start="$(git rev-parse HEAD)"
# The label is filtered here, not with --label, which makes gh query the search
# index. That lags, so it missed the pull request Release Please had opened and
# labelled a second earlier in the same run, which then merged unrefreshed.
branches="$(gh pr list --repo "$repo" --base main --state open --limit 100 \
	--json headRefName,labels \
	--jq '.[] | select(any(.labels[]; .name == "autorelease: pending")) | .headRefName')"
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
		check "$branch"
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
	check "$branch"
done
git switch --quiet --force --detach "$start"
