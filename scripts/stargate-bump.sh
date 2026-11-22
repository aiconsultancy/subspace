#!/usr/bin/env bash
# Propose moving the fleet installer's Stargate pin to Stargate's latest
# release. Run by .github/workflows/stargate-bump.yml from the repository root,
# on a checkout of main, after scripts/stargate-pin.py --json has found the pin
# behind (and not held).
#
#   stargate-bump.sh test TAG RESULTS
#     Move the pin to TAG in the working tree and run the installer's offline
#     tests, writing a Markdown table of what passed to RESULTS. The tests
#     failing does not fail this; the pull request says so.
#   stargate-bump.sh propose STATUS RESULTS
#     Move the pin to STATUS's latest release, commit it as the workflow, push
#     it to BRANCH and open the pull request, or update the open one. Never
#     overwrites a commit someone else pushed to BRANCH, and never merges.
#     Needs GH_TOKEN, a checkout that can push, contents: write and
#     pull-requests: write.
#
# Environment for tests: STARGATE_PIN replaces scripts/stargate-pin.py and
# STARGATE_CHECKS (one command a line) the installer's tests.
set -euo pipefail

pin="${STARGATE_PIN:-python3 scripts/stargate-pin.py}"
branch="${STARGATE_BRANCH:-automation/stargate-pin}"
summary="${GITHUB_STEP_SUMMARY:-/dev/null}"
runtime=subspace-installer/lib/runtime.py
bot_name='github-actions[bot]'
bot_email='41898282+github-actions[bot]@users.noreply.github.com'
default_checks='python3 -B subspace-installer/tests/policy_test.py
python3 -B subspace-installer/tests/sessions_test.py
python3 -B subspace-installer/tests/setup_test.py
python3 -B subspace-installer/tests/medical_test.py
bash -n install.sh subspace-installer/*.sh subspace-installer/tests/*.sh subspace-installer/vm/*.sh
shellcheck install.sh subspace-installer/*.sh subspace-installer/tests/*.sh subspace-installer/vm/*.sh'
checks="${STARGATE_CHECKS:-$default_checks}"

# The pin moves and nothing else does.
move_pin() {
	$pin --update --expect "$1"
	local changed
	changed="$(git status --porcelain --untracked-files=no)"
	if [[ -z "$changed" ]]; then
		echo "the pin is already $1; there is nothing to propose" >&2
		exit 1
	fi
	if [[ "$changed" != " M $runtime" ]]; then
		printf 'moving the pin changed more than %s:\n%s\n' "$runtime" "$changed" >&2
		exit 1
	fi
}

run_tests() {
	local tag="$1" results="$2" check log failed=0
	move_pin "$tag"
	log="$(mktemp)"
	{
		echo "On main at \`$(git rev-parse --short=12 HEAD)\`, with the pin moved to $tag:"
		echo
		echo '| Check | Result |'
		echo '| --- | --- |'
	} >"$results"
	local -a failures=()
	while IFS= read -r check; do
		[[ -n "$check" ]] || continue
		if bash -c "$check" >"$log" 2>&1; then
			echo "| \`$check\` | passed |" >>"$results"
		else
			echo "| \`$check\` | **failed** |" >>"$results"
			failures+=("$check"$'\n'"$(tail -n 40 "$log")")
			failed=1
		fi
	done <<<"$checks"
	for failure in ${failures[@]+"${failures[@]}"}; do
		{
			echo
			echo "<details><summary><code>${failure%%$'\n'*}</code>: the last lines of its output</summary>"
			echo
			echo '```text'
			echo "${failure#*$'\n'}"
			echo '```'
			echo
			echo '</details>'
		} >>"$results"
	done
	rm -f "$log"
	if ((failed)); then
		echo "The installer's tests failed with Stargate $tag; the pull request says which." >>"$summary"
	else
		echo "The installer's tests passed with Stargate $tag." >>"$summary"
	fi
}

propose() {
	local status="$1" results="$2" repo="${GITHUB_REPOSITORY:?}"
	local tag title body pr lease others
	tag="$(jq -r .latest.tag "$status")"
	title="$($pin --pr-title "$status")"
	body="$(mktemp)"
	move_pin "$tag"
	$pin --pr-body "$status" --tests "$results" >"$body"
	git config user.name "$bot_name"
	git config user.email "$bot_email"
	git commit --quiet --message "$title" -- "$runtime"

	# Not --label or --search, which go through the lagging search index.
	pr="$(gh pr list --repo "$repo" --base main --state open --head "$branch" \
		--json number --jq '.[0].number // empty')"
	lease=
	if git fetch --quiet --depth=1 origin "refs/heads/$branch" 2>/dev/null; then
		lease="$(git rev-parse FETCH_HEAD)"
		others="$(gh api "repos/$repo/compare/main...$lease" \
			--jq ".commits[] | select(.commit.author.email != \"$bot_email\") | .sha")"
		if [[ -n "$others" ]]; then
			if [[ -n "$pr" ]]; then
				echo "#$pr has commits from people on $branch; left as it is." >>"$summary"
				return 0
			fi
			echo "$branch has commits from people and no open pull request; open one or delete the branch." >&2
			exit 1
		fi
	fi
	if [[ -n "$lease" && "$(git rev-parse "$lease^{tree}")" == "$(git rev-parse 'HEAD^{tree}')" ]]; then
		echo "$branch already holds this change." >>"$summary"
	else
		# Only over the commit just looked at, or onto no branch at all.
		git push --quiet --force-with-lease="refs/heads/$branch:$lease" origin "HEAD:refs/heads/$branch"
	fi
	if [[ -n "$pr" ]]; then
		gh pr edit "$pr" --repo "$repo" --title "$title" --body-file "$body"
		echo "Updated #$pr: $title." >>"$summary"
	else
		gh pr create --repo "$repo" --base main --head "$branch" --title "$title" --body-file "$body"
		echo "Opened a pull request: $title." >>"$summary"
	fi
	rm -f "$body"
}

case "${1:-}" in
test) run_tests "${2:?tag}" "${3:?results file}" ;;
propose) propose "${2:?status file}" "${3:?results file}" ;;
*)
	echo "usage: $0 test TAG RESULTS | propose STATUS RESULTS" >&2
	exit 2
	;;
esac
