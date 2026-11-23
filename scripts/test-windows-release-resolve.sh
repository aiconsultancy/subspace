#!/usr/bin/env bash
# Exercise windows-release-resolve.sh against a local history: only a stable
# release tag on a commit on main, from a release that can skip running the
# app, passes.
set -euo pipefail
while IFS= read -r name; do
	unset "$name"
done < <(git rev-parse --local-env-vars)
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-windows-release-resolve.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT
export GIT_CONFIG_GLOBAL="$fixture/gitconfig" GIT_CONFIG_NOSYSTEM=1
printf '[commit]\n\tgpgsign = false\n[tag]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n[user]\n\tname = fixture\n\temail = fixture@example.invalid\n' >"$GIT_CONFIG_GLOBAL"
export TARGET=x86_64-pc-windows-msvc GITHUB_OUTPUT="$fixture/output" GITHUB_STEP_SUMMARY="$fixture/summary"

git init -q "$fixture/work"
cd "$fixture/work"
commit() { git add -A && git commit -qm "$1"; }
mkdir -p subspace-desktop/scripts
printf 'bash\n' >subspace-desktop/scripts/package-release.sh
printf '# Changelog\n' >subspace-desktop/CHANGELOG.md
commit base
# A release from before Windows builds: packaging cannot skip running the app.
git tag subspace-desktop-v0.9.0
printf 'SUBSPACE_PACKAGE_SKIP_RUN_CHECK\n' >>subspace-desktop/scripts/package-release.sh
commit "build: skip the run check"
# The release: annotated, as a tag may be.
printf '## 1.0.0\n' >>subspace-desktop/CHANGELOG.md
commit "chore(subspace): release 1.0.0"
git tag -a -m "release 1.0.0" subspace-desktop-v1.0.0
good="$(git rev-parse HEAD)"
# A commit that never reached main.
git switch -qc unmerged
printf '## 1.0.1\n' >>subspace-desktop/CHANGELOG.md
commit "chore(subspace): release 1.0.1"
outside="$(git rev-parse HEAD)"
git switch -q main
git tag subspace-desktop-v1.0.1 "$outside"
# Branches named like the tags, pointing elsewhere, change nothing.
git branch subspace-desktop-v1.0.0 "$outside"
git branch subspace-desktop-v1.0.1 "$good"

resolve() {
	: >"$GITHUB_OUTPUT"
	TAG="$1" EXPECTED_COMMIT="${2:-}" bash "$root/scripts/windows-release-resolve.sh"
}
expect_refusal() {
	local message="$1"
	shift
	if resolve "$@" >"$fixture/out" 2>&1; then
		echo "expected a refusal for tag '$1'" >&2
		exit 1
	fi
	grep -Fq -- "$message" "$fixture/out" || { cat "$fixture/out" >&2; exit 1; }
	if [[ -s "$GITHUB_OUTPUT" ]]; then
		echo "a refused tag must write no outputs" >&2
		exit 1
	fi
}

# The release resolves to its commit, read from the tag, not the branch.
resolve subspace-desktop-v1.0.0
printf 'tag=subspace-desktop-v1.0.0\nversion=1.0.0\ncommit=%s\narchive=subspace-1.0.0-x86_64-pc-windows-msvc.zip\n' "$good" |
	cmp -s - "$GITHUB_OUTPUT" || { cat "$GITHUB_OUTPUT" >&2; exit 1; }
grep -Fq "subspace-desktop-v1.0.0 is $good, on main." "$GITHUB_STEP_SUMMARY"
resolve subspace-desktop-v1.0.0 "$good"
grep -Fqx "commit=$good" "$GITHUB_OUTPUT"
expect_refusal "not $outside, the release's commit" subspace-desktop-v1.0.0 "$outside"

# Only a stable tag in the expected form, without anything after it.
for tag in subspace-desktop-v1.0 subspace-desktop-v01.0.0 subspace-desktop-v1.0.0-rc.1 \
	"subspace-desktop-v1.0.0 " $'subspace-desktop-v1.0.0\nx' subspace-framework-v1.0.0 \
	refs/tags/subspace-desktop-v1.0.0 "subspace-desktop-v1.0.0^{}" ""; do
	expect_refusal "not a stable Subspace desktop release tag" "$tag"
done
expect_refusal "no tag subspace-desktop-v2.0.0" subspace-desktop-v2.0.0

# A commit that never reached main, and a release from before Windows builds.
expect_refusal "$outside, which is not on main" subspace-desktop-v1.0.1
expect_refusal "predates Windows release builds" subspace-desktop-v0.9.0
echo "windows-release-resolve fixtures passed"
