#!/usr/bin/env bash
# Exercise release-lockfiles.sh against a local origin with a fake gh.
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
[[ "$1 $2" = "pr list" ]] || { echo "unexpected gh call: $*" >&2; exit 3; }
printf '%s\n' $FAKE_BRANCHES
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
FAKE_BRANCHES="$framework feature" bash scripts/release-lockfiles.sh
test "$(git --git-dir="$fixture/origin.git" show "$framework:subspace-framework/Cargo.lock")" = 'version = "0.2.0"'
test "$(git --git-dir="$fixture/origin.git" log -1 --format=%s "$framework")" = 'chore: refresh the lockfiles for the release'
test "$(git --git-dir="$fixture/origin.git" log -1 --format=%an "$framework")" = 'github-actions[bot]'
test "$(git --git-dir="$fixture/origin.git" show feature:subspace-framework/Cargo.lock)" = 'version = "0.1.0"'
test -f "$fixture/verified"
grep -Fq "$framework: refreshed 1 lockfiles." "$GITHUB_STEP_SUMMARY"
grep -Fq -- "--label autorelease: pending" "$fixture/gh.log"
test "$(git rev-parse HEAD)" = "$(git --git-dir="$fixture/origin.git" rev-parse main)"

# A second run finds nothing to do and pushes nothing.
rm "$fixture/verified"
before="$(git --git-dir="$fixture/origin.git" rev-parse "$framework")"
FAKE_BRANCHES="$framework" bash scripts/release-lockfiles.sh
test "$(git --git-dir="$fixture/origin.git" rev-parse "$framework")" = "$before"
test ! -e "$fixture/verified"
grep -Fq "$framework: the lockfiles are up to date." "$GITHUB_STEP_SUMMARY"

# A sync that touches anything but lockfiles stops before committing.
printf '#!/usr/bin/env bash\nprintf "changed\\n" >>README.md\n' >"$fixture/sync"
if FAKE_BRANCHES="$desktop" bash scripts/release-lockfiles.sh 2>"$fixture/error"; then
	echo 'a sync outside the lockfiles must stop' >&2
	exit 1
fi
grep -Fq 'changed files other than lockfiles' "$fixture/error"
test "$(git --git-dir="$fixture/origin.git" rev-parse "$desktop")" = "$(git --git-dir="$fixture/origin.git" rev-parse main)"
echo "release-lockfiles fixtures passed"
