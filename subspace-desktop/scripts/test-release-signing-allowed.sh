#!/usr/bin/env bash
# Exercise signing eligibility with synthetic GitHub metadata and a fixture config.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-signing-trigger.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
mkdir "$fixture/bin"
cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" = api ]] || exit 90
[[ "${SIGNING_API_FAIL:-0}" = 0 ]] || exit 1
cat "$SIGNING_PULLS_JSON"
SH
chmod +x "$fixture/bin/gh"
export PATH="$fixture/bin:$PATH"
export GITHUB_EVENT_NAME=push GITHUB_REF=refs/heads/main
export GITHUB_REPOSITORY=fixture/repo GITHUB_SHA=fixture-merge
export SIGNING_PULLS_JSON="$fixture/pulls.json"
config="$fixture/release-please-config.json"
cat >"$config" <<'JSON'
{"label":"autorelease: pending","release-label":"autorelease: tagged",
"packages":{"subspace-desktop":{"component":"subspace-desktop"}}}
JSON
cat >"$fixture/base.json" <<'JSON'
[{"merged_at":"2026-10-21T00:00:00Z","merge_commit_sha":"fixture-merge",
"base":{"ref":"main","repo":{"full_name":"fixture/repo"}},
"head":{"ref":"release-please--branches--main--components--subspace-desktop","repo":{"full_name":"fixture/repo"}},
"user":{"type":"Bot"},"labels":[{"name":"autorelease: tagged"}]}]
JSON

expect() {
	local expected="$1" actual
	actual="$(bash "$root/scripts/release-signing-allowed.sh" "$config")"
	if [[ "$actual" != "$expected" ]]; then
		echo "expected signing=$expected, got $actual" >&2
		exit 1
	fi
}

cp "$fixture/base.json" "$SIGNING_PULLS_JSON"
expect true
GITHUB_EVENT_NAME=workflow_dispatch expect false
GITHUB_EVENT_NAME=pull_request SIGNING_API_FAIL=1 expect false
GITHUB_REF=refs/heads/feature SIGNING_API_FAIL=1 expect false

for mutation in \
	'[]' \
	'.[0].merged_at = null' \
	'.[0].merge_commit_sha = "previous-merge"' \
	'.[0].base.ref = "other"' \
	'.[0].base.repo.full_name = "other/repo"' \
	'.[0].head.repo.full_name = "fork/repo"' \
	'.[0].head.ref = "feature"' \
	'.[0].head.ref = "release-please--branches--main--components--other"' \
	'.[0].user.type = "User"' \
	'.[0].labels = []'; do
	jq "$mutation" "$fixture/base.json" >"$SIGNING_PULLS_JSON"
	expect false
done

jq '.[0].labels[0].name = "autorelease: pending"' \
	"$fixture/base.json" >"$SIGNING_PULLS_JSON"
expect true

if SIGNING_API_FAIL=1 bash "$root/scripts/release-signing-allowed.sh" "$config" >/dev/null 2>&1; then
	echo 'GitHub API failure must block signing eligibility' >&2
	exit 1
fi
printf '%s\n' '{"label":"a","release-label":"b","packages":{}}' >"$fixture/other-config.json"
if bash "$root/scripts/release-signing-allowed.sh" "$fixture/other-config.json" >/dev/null 2>&1; then
	echo 'a config without the Subspace package must block signing eligibility' >&2
	exit 1
fi
echo 'Release Please merge-only signing fixtures passed'
