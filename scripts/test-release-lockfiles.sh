#!/usr/bin/env bash
# Exercise release-lockfiles.sh against a local origin with a fake gh, whose
# pull request list applies the script's own --jq filter with jq.
set -euo pipefail
while IFS= read -r name; do
	unset "$name"
done < <(git rev-parse --local-env-vars)
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-release-lockfiles.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT
export GIT_CONFIG_GLOBAL="$fixture/gitconfig" GIT_CONFIG_NOSYSTEM=1
printf '[commit]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n' >"$GIT_CONFIG_GLOBAL"
mkdir -p "$fixture/bin"
cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE/gh.log"
[[ "$1 $2" = "workflow run" ]] && exit 0
[[ "$1 $2" = "pr list" ]] || { echo "unexpected gh call: $*" >&2; exit 3; }
filter=
while (($#)); do
	[[ "$1" = --jq ]] && filter="$2"
	shift
done
# FAKE_BRANCHES carry the pending label; FAKE_UNLABELLED carry none.
{
	for branch in $FAKE_BRANCHES; do
		printf '{"headRefName":"%s","labels":[{"name":"autorelease: pending"}]}\n' "$branch"
	done
	for branch in ${FAKE_UNLABELLED:-}; do
		printf '{"headRefName":"%s","labels":[]}\n' "$branch"
	done
} | jq -rs "$filter"
SH
chmod 755 "$fixture/bin/gh"
export PATH="$fixture/bin:$PATH" FIXTURE="$fixture" GITHUB_REPOSITORY=fixture/repository
export GITHUB_STEP_SUMMARY="$fixture/summary"

framework=release-please--branches--main--components--subspace-framework
desktop=release-please--branches--main--components--subspace-desktop
git init -q --bare "$fixture/origin.git"
git clone -q "$fixture/origin.git" "$fixture/work" 2>/dev/null
cd "$fixture/work"
git -c user.name=fixture -c user.email=fixture@example.invalid commit -q --allow-empty -m base
mkdir -p subspace-framework scripts
printf 'version = "0.1.0"\n' >subspace-framework/Cargo.lock
printf 'notes\n' >README.md
git add -A
git -c user.name=fixture -c user.email=fixture@example.invalid commit -qm lockfile
git push -q origin HEAD:main HEAD:"$framework" HEAD:"$desktop" HEAD:feature 2>/dev/null
cp "$root/scripts/release-lockfiles.sh" scripts/

# The sync bumps the Framework's lockfile, once; Cargo's check is recorded.
export RELEASE_VERIFY="touch $fixture/verified"
cat >"$fixture/sync" <<'SH'
#!/usr/bin/env bash
sed s/0.1.0/0.2.0/ subspace-framework/Cargo.lock >subspace-framework/Cargo.lock.new
mv subspace-framework/Cargo.lock.new subspace-framework/Cargo.lock
SH
export RELEASE_SYNC="bash $fixture/sync"
FAKE_BRANCHES="$framework feature" FAKE_UNLABELLED="$desktop" bash scripts/release-lockfiles.sh
test "$(git --git-dir="$fixture/origin.git" show "$framework:subspace-framework/Cargo.lock")" = 'version = "0.2.0"'
test "$(git --git-dir="$fixture/origin.git" log -1 --format=%s "$framework")" = 'chore: refresh the lockfiles for the release'
test "$(git --git-dir="$fixture/origin.git" log -1 --format=%an "$framework")" = 'github-actions[bot]'
test "$(git --git-dir="$fixture/origin.git" show feature:subspace-framework/Cargo.lock)" = 'version = "0.1.0"'
test -f "$fixture/verified"
grep -Fq "$framework: refreshed 1 lockfiles." "$GITHUB_STEP_SUMMARY"
# Not --label, which makes gh query the lagging search index.
if grep -Fq -- "--label" "$fixture/gh.log"; then
	echo 'the pull request list must not use --label' >&2
	exit 1
fi
test "$(git --git-dir="$fixture/origin.git" rev-parse "$desktop")" = "$(git --git-dir="$fixture/origin.git" rev-parse main)"
# The lockfiles and Stargate pin checks are started on the refreshed release
# branch only.
test "$(grep -c '^workflow run' "$fixture/gh.log")" = 2
grep -Fqx "workflow run lockfiles.yml --repo fixture/repository --ref $framework" "$fixture/gh.log"
grep -Fqx "workflow run stargate-pin.yml --repo fixture/repository --ref $framework" "$fixture/gh.log"
test "$(git rev-parse HEAD)" = "$(git --git-dir="$fixture/origin.git" rev-parse main)"

# A second run finds nothing to do and pushes nothing.
rm "$fixture/verified"
before="$(git --git-dir="$fixture/origin.git" rev-parse "$framework")"
FAKE_BRANCHES="$framework" bash scripts/release-lockfiles.sh
test "$(git --git-dir="$fixture/origin.git" rev-parse "$framework")" = "$before"
test ! -e "$fixture/verified"
grep -Fq "$framework: the lockfiles are up to date." "$GITHUB_STEP_SUMMARY"
# An up-to-date release branch is checked too.
test "$(grep -c "^workflow run lockfiles.yml --repo fixture/repository --ref $framework\$" "$fixture/gh.log")" = 2
test "$(grep -c "^workflow run stargate-pin.yml --repo fixture/repository --ref $framework\$" "$fixture/gh.log")" = 2

# A sync that touches anything but lockfiles stops before committing.
printf '#!/usr/bin/env bash\nprintf "changed\\n" >>README.md\n' >"$fixture/sync"
if FAKE_BRANCHES="$desktop" bash scripts/release-lockfiles.sh 2>"$fixture/error"; then
	echo 'a sync outside the lockfiles must stop' >&2
	exit 1
fi
grep -Fq 'changed files other than lockfiles' "$fixture/error"
if grep -Fq -- "--ref $desktop" "$fixture/gh.log"; then
	echo 'a branch that was not refreshed must not be checked' >&2
	exit 1
fi
test "$(git --git-dir="$fixture/origin.git" rev-parse "$desktop")" = "$(git --git-dir="$fixture/origin.git" rev-parse main)"

# The app's release branch, which carries the installer, gets both checks.
printf '#!/usr/bin/env bash\n' >"$fixture/sync"
FAKE_BRANCHES="$desktop" bash scripts/release-lockfiles.sh
grep -Fqx "workflow run lockfiles.yml --repo fixture/repository --ref $desktop" "$fixture/gh.log"
grep -Fqx "workflow run stargate-pin.yml --repo fixture/repository --ref $desktop" "$fixture/gh.log"
echo "release-lockfiles fixtures passed"
