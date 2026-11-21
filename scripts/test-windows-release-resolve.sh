#!/usr/bin/env bash
# Exercise windows-release-resolve.sh against a local history with a fake gh:
# only a Release Please merge commit in main's first-parent history passes.
set -euo pipefail
while IFS= read -r name; do
	unset "$name"
done < <(git rev-parse --local-env-vars)
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-windows-release-resolve.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT
export GIT_CONFIG_GLOBAL="$fixture/gitconfig" GIT_CONFIG_NOSYSTEM=1
printf '[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n[user]\n\tname = fixture\n\temail = fixture@example.invalid\n' >"$GIT_CONFIG_GLOBAL"
mkdir -p "$fixture/bin"
cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE/gh.log"
if [[ $# -ne 2 || "$1" != api || ! "$2" =~ ^repos/fixture/repository/commits/([0-9a-f]{40})/pulls$ ]]; then
	echo "unexpected gh call: $*" >&2
	exit 3
fi
sha="${BASH_REMATCH[1]}"
pr() {
	jq -n --arg sha "${5:-$sha}" --arg ref "$1" --arg user "$2" --arg merged "$3" --arg repo "$4" --arg base "${6:-main}" '{
		merge_commit_sha: $sha, merged_at: (if $merged == "" then null else $merged end),
		base: {ref: $base}, head: {ref: $ref, repo: {full_name: $repo}}, user: {login: $user}}'
}
release=release-please--branches--main--components--subspace-desktop
bot='github-actions[bot]'
at=2026-11-21T00:00:00Z
case "$FAKE_GH" in
release) pr "$release" "$bot" "$at" fixture/repository | jq -s . ;;
twice) { pr "$release" "$bot" "$at" fixture/repository; pr "$release" "$bot" "$at" fixture/repository; } | jq -s . ;;
feature) pr feature someone "$at" fixture/repository | jq -s . ;;
framework) pr release-please--branches--main--components--subspace-framework "$bot" "$at" fixture/repository | jq -s . ;;
person) pr "$release" someone "$at" fixture/repository | jq -s . ;;
fork) pr "$release" "$bot" "$at" someone/repository | jq -s . ;;
open) pr "$release" "$bot" "" fixture/repository | jq -s . ;;
other-merge) pr "$release" "$bot" "$at" fixture/repository 0000000000000000000000000000000000000000 | jq -s . ;;
other-base) pr "$release" "$bot" "$at" fixture/repository "" integration | jq -s . ;;
none) echo '[]' ;;
object) echo '{"message":"Not Found"}' ;;
garbage) echo '<html>' ;;
fail) echo 'gh: HTTP 502' >&2; exit 1 ;;
*) echo "unknown FAKE_GH: $FAKE_GH" >&2; exit 4 ;;
esac
SH
chmod 755 "$fixture/bin/gh"
export PATH="$fixture/bin:$PATH" FIXTURE="$fixture" GITHUB_REPOSITORY=fixture/repository
export TARGET=x86_64-pc-windows-msvc GITHUB_OUTPUT="$fixture/output" GITHUB_STEP_SUMMARY="$fixture/summary"

git init -q "$fixture/work"
cd "$fixture/work"
commit() { git add -A && git commit -qm "$1"; }
release_branch=release-please--branches--main--components--subspace-desktop
# A Release Please pull request off main: the given files, merged into main.
release() {
	git switch -qc "$release_branch"
	for file in "$@"; do printf '%s\n' "$file $RANDOM" >>"$file"; done
	commit "chore(main): release"
	git switch -q main
	git merge -q --no-ff -m "chore(main): release (#1)" "$release_branch"
	git branch -qD "$release_branch"
}
mkdir -p subspace-desktop/scripts subspace-desktop/src
printf '{}\n' >.release-please-manifest.json
printf '[package]\n' >subspace-desktop/Cargo.toml
printf '# Changelog\n' >subspace-desktop/CHANGELOG.md
printf 'bash\n' >subspace-desktop/scripts/package-release.sh
printf 'fn main() {}\n' >subspace-desktop/src/main.rs
commit base
# A release from before Windows builds: packaging cannot skip running the app.
release .release-please-manifest.json subspace-desktop/CHANGELOG.md
git tag subspace-desktop-v0.9.0
printf 'SUBSPACE_PACKAGE_SKIP_RUN_CHECK\n' >>subspace-desktop/scripts/package-release.sh
commit "build: skip the run check"
# A feature pull request, merged: its own commit is reachable from main, but
# not in main's first-parent history.
git switch -qc feature
printf '// reviewed?\n' >>subspace-desktop/src/main.rs
commit "feat: a feature"
inside="$(git rev-parse HEAD)"
git switch -q main
git merge -q --no-ff -m "feat: a feature (#2)" feature
feature_merge="$(git rev-parse HEAD)"
# The release: annotated, as a tag may be.
release .release-please-manifest.json subspace-desktop/CHANGELOG.md subspace-desktop/Cargo.toml subspace-desktop/Cargo.lock
git tag -a -m "release 1.0.0" subspace-desktop-v1.0.0
good="$(git rev-parse HEAD)"
# A release pull request that also changed the app.
release subspace-desktop/CHANGELOG.md subspace-desktop/src/main.rs
git tag subspace-desktop-v1.1.0
# A commit that never reached main.
git switch -qc unreviewed
printf '// unreviewed\n' >>subspace-desktop/src/main.rs
commit "feat: unreviewed"
outside="$(git rev-parse HEAD)"
git switch -q main
git tag subspace-desktop-v1.0.1 "$outside"
git tag subspace-desktop-v1.0.2 "$inside"
git tag subspace-desktop-v1.0.3 "$feature_merge"
# Branches named like the tags, pointing elsewhere, change nothing.
git branch subspace-desktop-v1.0.0 "$outside"
git branch subspace-desktop-v1.0.1 "$good"

resolve() {
	: >"$GITHUB_OUTPUT"
	: >"$fixture/gh.log"
	TAG="$1" EXPECTED_COMMIT="${2:-}" bash "$root/scripts/windows-release-resolve.sh"
}
expect_refusal() {
	local message="$1"
	shift
	if resolve "$@" >"$fixture/out" 2>&1; then
		echo "expected a refusal for tag '$1' (FAKE_GH=$FAKE_GH)" >&2
		exit 1
	fi
	grep -Fq -- "$message" "$fixture/out" || { cat "$fixture/out" >&2; exit 1; }
	if [[ -s "$GITHUB_OUTPUT" ]]; then
		echo "a refused tag must write no outputs" >&2
		exit 1
	fi
}

# The release resolves to its commit, read from the tag, not the branch.
export FAKE_GH=release
resolve subspace-desktop-v1.0.0 2>/dev/null
printf 'tag=subspace-desktop-v1.0.0\nversion=1.0.0\ncommit=%s\narchive=subspace-1.0.0-x86_64-pc-windows-msvc.zip\n' "$good" |
	cmp -s - "$GITHUB_OUTPUT" || { cat "$GITHUB_OUTPUT" >&2; exit 1; }
grep -Fqx "api repos/fixture/repository/commits/$good/pulls" "$fixture/gh.log"
grep -Fq "subspace-desktop-v1.0.0 is $good" "$GITHUB_STEP_SUMMARY"
resolve subspace-desktop-v1.0.0 "$good" 2>/dev/null
grep -Fqx "commit=$good" "$GITHUB_OUTPUT"
expect_refusal "not $outside, the release's commit" subspace-desktop-v1.0.0 "$outside"

# Only a stable tag in the expected form, without anything after it.
for tag in subspace-desktop-v1.0 subspace-desktop-v01.0.0 subspace-desktop-v1.0.0-rc.1 \
	"subspace-desktop-v1.0.0 " $'subspace-desktop-v1.0.0\nx' subspace-framework-v1.0.0 \
	refs/tags/subspace-desktop-v1.0.0 "subspace-desktop-v1.0.0^{}" ""; do
	expect_refusal "not a stable Subspace desktop release tag" "$tag"
done
expect_refusal "no tag subspace-desktop-v2.0.0" subspace-desktop-v2.0.0

# A commit off main, inside a merged pull request, or a feature's merge.
expect_refusal "$outside, which is not in main's first-parent history" subspace-desktop-v1.0.1
expect_refusal "$inside, which is not in main's first-parent history" subspace-desktop-v1.0.2
test ! -s "$fixture/gh.log"
FAKE_GH=feature expect_refusal "not the merge commit of a merged Release Please pull request" subspace-desktop-v1.0.3

# GitHub must record the commit as the merge of exactly one merged Release
# Please pull request for the desktop into main; anything else, or a failed
# or unexpected answer, refuses it.
for answer in twice feature framework person fork open other-merge other-base none object garbage; do
	FAKE_GH="$answer" expect_refusal "not the merge commit of a merged Release Please pull request" subspace-desktop-v1.0.0
done
FAKE_GH=fail expect_refusal "HTTP 502" subspace-desktop-v1.0.0

# A release commit must change only Release Please's files, and its packaging
# must be able to skip running the app.
expect_refusal "subspace-desktop/src/main.rs" subspace-desktop-v1.1.0
expect_refusal "predates Windows release builds" subspace-desktop-v0.9.0
echo "windows-release-resolve fixtures passed"
