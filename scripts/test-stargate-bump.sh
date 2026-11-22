#!/usr/bin/env bash
# Exercise stargate-bump.sh against a local origin with a fake gh, whose
# compare answers from the origin's history, and a fake stargate-pin.py.
set -euo pipefail
while IFS= read -r name; do
	unset "$name"
done < <(git rev-parse --local-env-vars)
root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-stargate-bump.XXXXXX")"
trap 'rm -rf "${fixture:?}"' EXIT
export GIT_CONFIG_GLOBAL="$fixture/gitconfig" GIT_CONFIG_NOSYSTEM=1
printf '[commit]\n\tgpgsign = false\n[init]\n\tdefaultBranch = main\n' >"$GIT_CONFIG_GLOBAL"
mkdir -p "$fixture/bin"
cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$FIXTURE/gh.log"
case "$1 $2" in
"pr list") printf '%s\n' "${FAKE_PR:-}" ;;
"pr create" | "pr edit") ;;
"api repos/fixture/repository/compare/main..."*)
	# The commits on the branch that are not on main and not the workflow's.
	git --git-dir="$FIXTURE/origin.git" log --format='%H %ae' "main..${2##*...}" |
		grep -v ' 41898282+github-actions\[bot\]@users.noreply.github.com$' | cut -d' ' -f1 || true
	;;
*) echo "unexpected gh call: $*" >&2; exit 3 ;;
esac
SH
cat >"$fixture/pin" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
runtime=subspace-installer/lib/runtime.py
case "$1" in
--update)
	sed "s/v0\.43\.0/$3/" "$runtime" >"$runtime.new"
	mv "$runtime.new" "$runtime"
	if [[ -n "${FAKE_STRAY:-}" ]]; then echo stray >>README.md; fi
	;;
--pr-title) echo "build(installer): move Stargate from v0.43.0 to $(jq -r .latest.tag "$2")" ;;
--pr-body) printf 'Moves the pin.\n\n'; cat "$4" ;;
*) echo "unexpected pin call: $*" >&2; exit 3 ;;
esac
SH
chmod 755 "$fixture/bin/gh" "$fixture/pin"
export PATH="$fixture/bin:$PATH" FIXTURE="$fixture" GITHUB_REPOSITORY=fixture/repository
export GITHUB_STEP_SUMMARY="$fixture/summary" STARGATE_PIN="$fixture/pin"
branch=automation/stargate-pin
bot='41898282+github-actions[bot]@users.noreply.github.com'

git init -q --bare "$fixture/origin.git"
git clone -q "$fixture/origin.git" "$fixture/work" 2>/dev/null
cd "$fixture/work"
mkdir -p subspace-installer/lib scripts
printf 'STARGATE_RELEASE = "v0.43.0"\n' >subspace-installer/lib/runtime.py
printf 'notes\n' >README.md
cp "$root/scripts/stargate-bump.sh" scripts/
git add -A
git -c user.name=fixture -c user.email=fixture@example.invalid commit -qm base
git push -q origin HEAD:main 2>/dev/null
main="$(git rev-parse HEAD)"
printf '{"latest": {"tag": "v0.44.0"}}\n' >"$fixture/status.json"

# The tests run on the moved pin, and a failure is reported, not fatal.
STARGATE_CHECKS=$'grep -q v0.44.0 subspace-installer/lib/runtime.py\necho broken >&2; false' \
	bash scripts/stargate-bump.sh test v0.44.0 "$fixture/results.md"
# shellcheck disable=SC2016 # Backticks are Markdown here.
grep -Fqx '| `grep -q v0.44.0 subspace-installer/lib/runtime.py` | passed |' "$fixture/results.md"
# shellcheck disable=SC2016
grep -Fqx '| `echo broken >&2; false` | **failed** |' "$fixture/results.md"
grep -Fqx 'broken' "$fixture/results.md"
grep -Fq 'tests failed with Stargate v0.44.0' "$GITHUB_STEP_SUMMARY"
git checkout -q -- .

# Moving the pin must change nothing else.
if FAKE_STRAY=1 STARGATE_CHECKS=true bash scripts/stargate-bump.sh test v0.44.0 "$fixture/results.md" 2>"$fixture/error"; then
	echo 'a change outside runtime.py must stop' >&2
	exit 1
fi
grep -Fq 'changed more than subspace-installer/lib/runtime.py' "$fixture/error"
git checkout -q -- .

# No branch and no pull request: one commit by the workflow, then a new pull request.
bash scripts/stargate-bump.sh propose "$fixture/status.json" "$fixture/results.md"
origin() { git --git-dir="$fixture/origin.git" "$@"; }
test "$(origin rev-parse "$branch^")" = "$main"
test "$(origin log -1 --format=%ae "$branch")" = "$bot"
test "$(origin log -1 --format=%s "$branch")" = 'build(installer): move Stargate from v0.43.0 to v0.44.0'
test "$(origin diff --name-only main "$branch")" = subspace-installer/lib/runtime.py
grep -Fq "pr create --repo fixture/repository --base main --head $branch --title build(installer): move Stargate from v0.43.0 to v0.44.0 --body-file" "$fixture/gh.log"
grep -Fq -- '--head automation/stargate-pin' "$fixture/gh.log"
first="$(origin rev-parse "$branch")"

# The same change again, with the pull request open: nothing is pushed and the
# pull request is updated.
git checkout -q --detach "$main"
FAKE_PR=7 bash scripts/stargate-bump.sh propose "$fixture/status.json" "$fixture/results.md"
test "$(origin rev-parse "$branch")" = "$first"
grep -Fq 'pr edit 7 --repo fixture/repository --title' "$fixture/gh.log"
grep -Fq "$branch already holds this change." "$GITHUB_STEP_SUMMARY"

# A newer main moves the branch onto it.
git checkout -q --detach "$main"
printf 'more\n' >>README.md
git -c user.name=fixture -c user.email=fixture@example.invalid commit -qam 'docs: more'
git push -q origin HEAD:main 2>/dev/null
main="$(git rev-parse HEAD)"
FAKE_PR=7 bash scripts/stargate-bump.sh propose "$fixture/status.json" "$fixture/results.md"
test "$(origin rev-parse "$branch^")" = "$main"

# A person's commit on the branch is never overwritten.
git checkout -q --detach "$(origin rev-parse "$branch")"
printf 'fix\n' >>README.md
git -c user.name=maintainer -c user.email=maintainer@example.invalid commit -qam 'fix(installer): follow v0.44.0'
git push -q origin "HEAD:refs/heads/$branch" 2>/dev/null
theirs="$(git rev-parse HEAD)"
git checkout -q --detach "$main"
FAKE_PR=7 bash scripts/stargate-bump.sh propose "$fixture/status.json" "$fixture/results.md"
test "$(origin rev-parse "$branch")" = "$theirs"
grep -Fq "#7 has commits from people on $branch; left as it is." "$GITHUB_STEP_SUMMARY"
git checkout -q --detach "$main"
if bash scripts/stargate-bump.sh propose "$fixture/status.json" "$fixture/results.md" 2>"$fixture/error"; then
	echo 'commits from people with no pull request must stop' >&2
	exit 1
fi
grep -Fq 'has commits from people and no open pull request' "$fixture/error"
test "$(origin rev-parse "$branch")" = "$theirs"

# Nothing is ever merged, and the pull request list does not use the search index.
if grep -Eq '^pr merge|--label|--search' "$fixture/gh.log"; then
	echo 'stargate-bump.sh must not merge or search' >&2
	exit 1
fi
echo "stargate-bump fixtures passed"
