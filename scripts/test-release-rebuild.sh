#!/usr/bin/env bash
# Exercise release-rebuild.sh with a fake gh.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-release-rebuild.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT
mkdir -p "$fixture/bin"
cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE/gh.log"
case "$1 $2" in
"pr list") printf '%s\n' "$FAKE_PULLS" ;;
"api repos/fixture/repository/compare/main..."*)
	case "$2" in
	*subspace-framework) echo "$FAKE_FRAMEWORK_BEHIND" ;;
	*) echo 0 ;;
	esac
	;;
"api repos/fixture/repository/pulls/"*) echo "Release notes" ;;
"api --method") ;;
*) echo "unexpected gh call: $*" >&2; exit 3 ;;
esac
SH
chmod 755 "$fixture/bin/gh"
export PATH="$fixture/bin:$PATH" FIXTURE="$fixture" GITHUB_REPOSITORY=fixture/repository
export GITHUB_STEP_SUMMARY="$fixture/summary"
: >"$GITHUB_STEP_SUMMARY"

framework=release-please--branches--main--components--subspace-framework
desktop=release-please--branches--main--components--subspace-desktop
export FAKE_PULLS="74 $framework
73 $desktop
9 feature"

# Only the release pull request that is behind main gets a new description,
# which keeps the notes.
FAKE_FRAMEWORK_BEHIND=2 bash "$root/scripts/release-rebuild.sh"
test "$(grep -c -- '--method PATCH' "$fixture/gh.log")" = 1
grep -Fq -- "--method PATCH repos/fixture/repository/pulls/74 --silent -f body=Release notes" "$fixture/gh.log"
grep -Fq "<!-- behind main by 2: rebuild -->" "$fixture/gh.log"
grep -Fq "#74 is behind main by 2" "$GITHUB_STEP_SUMMARY"
grep -Fq -- "--label autorelease: pending" "$fixture/gh.log"
if grep -q 'feature\|pulls/9' <(grep -v '^pr list' "$fixture/gh.log"); then
	echo 'a pull request from another branch must be left alone' >&2
	exit 1
fi

# Nothing is behind: nothing changes. No open release pull request: no calls.
: >"$fixture/gh.log"
FAKE_FRAMEWORK_BEHIND=0 bash "$root/scripts/release-rebuild.sh"
test "$(grep -c -- '--method PATCH' "$fixture/gh.log" || true)" = 0
: >"$fixture/gh.log"
FAKE_PULLS="" FAKE_FRAMEWORK_BEHIND=2 bash "$root/scripts/release-rebuild.sh"
test "$(wc -l <"$fixture/gh.log" | tr -d ' ')" = 1
echo "release-rebuild fixtures passed"
