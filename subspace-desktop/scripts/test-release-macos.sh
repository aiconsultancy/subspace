#!/usr/bin/env bash
# Exercise release-macos.sh with a fake gh and packager in a fixture checkout.
set -euo pipefail
# Git exports worktree-local variables to hooks. Clear them before creating
# a repository or running the release script in the fixture.
while IFS= read -r name; do
	unset "$name"
done < <(git rev-parse --local-env-vars)
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product-fixture.sh
source "$root/scripts/product-fixture.sh"
# shellcheck disable=SC2119 # The fixture reads this workspace, not saved metadata.
setup_product_fixture
cp "$root/scripts/release-macos.sh" "$fixture/scripts/"
mkdir -p "$fixture/products/subspace"
cp "$root/products/subspace/"{build-config.json,build-config.no-api.json} "$fixture/products/subspace/"
stem="$(jq -r .asset_stem "$fixture/metadata/subspace.json")"
archive="${stem}_${version}_darwin_arm64.zip"
no_api_archive="${stem}-NoAPI_${version}_darwin_arm64.zip"
tag="subspace-desktop-v$version"

cat >"$fixture/bin/gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'gh %s\n' "$*" >>"$FIXTURE/gh.log"
# Only the packaging may see the GitHub OAuth secret.
[[ -z "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}" ]] || { echo 'gh saw the OAuth secret' >&2; exit 4; }
case "$1 $2" in
"auth status") exit 0 ;;
"api repos/"*/commits/subspace-desktop-v*)
	[[ "${FAKE_TAG_MISSING:-0}" = 1 ]] && exit 1
	printf '%s\n' "${FAKE_TAG_SHA:-$FIXTURE_SHA}"
	exit 0 ;;
"api repos/"*) [[ "${FAKE_COMMIT_MISSING:-0}" = 1 ]] && exit 1; exit 0 ;;
"release view")
	# Release Please's draft, unless a test says otherwise.
	case "${FAKE_RELEASE:-draft}" in
	draft) echo true ;;
	published) echo false ;;
	*) exit 1 ;;
	esac
	exit 0 ;;
"release upload")
	shift 3
	while [[ "$1" != --repo ]]; do test -f "$1"; shift; done
	exit 0 ;;
"release edit") exit 0 ;;
esac
echo "unexpected gh call: $*" >&2
exit 3
SH
cat >"$fixture/package.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'package %s\n' "$*" >>"$FIXTURE/gh.log"
printf 'config %s\n' "${SUBSPACE_BUILD_CONFIG:-}" >>"$FIXTURE/gh.log"
test "${SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET:-}" = "$EXPECTED_OAUTH_SECRET"
mkdir -p "$FIXTURE/dist"
# Each variant's archive, named as package-macos.sh names it.
if [[ "$(jq -r '.api_backend // ""' "$SUBSPACE_BUILD_CONFIG")" = none ]]; then
	printf 'no-api app archive\n' >"$FIXTURE/dist/$FIXTURE_NO_API_ARCHIVE"
else
	printf 'app archive\n' >"$FIXTURE/dist/$FIXTURE_ARCHIVE"
fi
SH
chmod 755 "$fixture/bin/gh" "$fixture/package.sh"
export SUBSPACE_RELEASE_PACKAGE="$fixture/package.sh" FIXTURE_ARCHIVE="$archive" FIXTURE_NO_API_ARCHIVE="$no_api_archive"
# The login keychain holds the OAuth secret (see product-fixture.sh).
export FAKE_OAUTH_SECRET=keychain-oauth-secret EXPECTED_OAUTH_SECRET=keychain-oauth-secret
printf '/dist\n/metadata\n/bin\n/package.sh\n/gh.log\n/*.log\n/*.out\n/notes.md\n/scripts/desktop-session.sh\n' >"$fixture/.gitignore"
git -C "$fixture" init -q
git -C "$fixture" add -A
git -C "$fixture" -c commit.gpgsign=false -c user.name=fixture -c user.email=fixture@example.invalid commit -qm fixture
sha="$(git -C "$fixture" rev-parse HEAD)"
export FIXTURE_SHA="$sha"

release() {
	: >"$fixture/gh.log"
	: >"$fixture/security.log"
	bash "$fixture/scripts/release-macos.sh" "Developer ID Application: Fixture (TEAMID)" fixture-profile
}

release >"$fixture/release.out" 2>&1
if grep -Fq keychain-oauth-secret "$fixture/release.out" "$fixture/gh.log"; then echo 'the OAuth secret was printed' >&2; exit 1; fi
grep -Fxq 'security find-generic-password' "$fixture/security.log"
test "$(grep -c "^package Developer ID Application: Fixture (TEAMID) fixture-profile aarch64-apple-darwin$" "$fixture/gh.log")" = 2
# Both variants, from the repository's own build configs, the standard app last.
test "$(grep '^config ' "$fixture/gh.log" | tr '\n' ' ')" = "config $fixture/products/subspace/build-config.no-api.json config $fixture/products/subspace/build-config.json "
# Release Please's draft gets the files first, then is published.
grep -Fxq "gh release view $tag --repo aiconsultancy/subspace --json isDraft --jq .isDraft" "$fixture/gh.log"
grep -Fxq "gh api repos/aiconsultancy/subspace/commits/$tag --jq .sha" "$fixture/gh.log"
grep -Fxq "gh release upload $tag dist/$no_api_archive dist/$no_api_archive.sha256 dist/$archive dist/$archive.sha256 --repo aiconsultancy/subspace --clobber" "$fixture/gh.log"
grep -Fxq "gh release edit $tag --repo aiconsultancy/subspace --title Subspace $version --draft=false --latest" "$fixture/gh.log"
test "$(grep -n 'release upload' "$fixture/gh.log" | cut -d: -f1)" -lt "$(grep -n 'release edit' "$fixture/gh.log" | cut -d: -f1)"
if grep -Fq 'release create' "$fixture/gh.log"; then echo 'a stable release must use the draft' >&2; exit 1; fi
for file in "$archive" "$no_api_archive"; do
	expected="$(cd "$fixture/dist" && shasum -a 256 "$file")"
	test "$(cat "$fixture/dist/$file.sha256")" = "$expected"
done

# A build config left in the shell never reaches either variant.
SUBSPACE_BUILD_CONFIG=/elsewhere/private-build-config.json release
if grep -Fq private-build-config "$fixture/gh.log"; then echo 'a stray SUBSPACE_BUILD_CONFIG reached the packaging' >&2; exit 1; fi
test "$(grep -c '^config ' "$fixture/gh.log")" = 2

# DRAFT=1 leaves it a draft; a notes file replaces the changelog's notes.
printf 'Fixture notes\n' >"$fixture/notes.md"
RELEASE_DRAFT=1 RELEASE_NOTES_FILE="$fixture/notes.md" release
grep -Fxq "gh release edit $tag --repo aiconsultancy/subspace --title Subspace $version --notes-file $fixture/notes.md" "$fixture/gh.log"

# Started outside the desktop session, it runs itself there.
cat >"$fixture/scripts/desktop-session.sh" <<'SH'
#!/usr/bin/env bash
printf 'session %s\n' "$*" >>"$FIXTURE/gh.log"
FAKE_LAUNCHD_MANAGER=Aqua exec "$@"
SH
FAKE_LAUNCHD_MANAGER=Background release
grep -Fxq "session bash $(cd "$fixture/scripts" && pwd)/release-macos.sh Developer ID Application: Fixture (TEAMID) fixture-profile" "$fixture/gh.log"
test "$(grep -c '^session ' "$fixture/gh.log")" = 1
grep -Fq "release edit $tag" "$fixture/gh.log"

for failure in FAKE_RELEASE=published FAKE_RELEASE=missing FAKE_TAG_MISSING=1 FAKE_TAG_SHA=0123456789abcdef FAKE_COMMIT_MISSING=1; do
	if env "$failure" bash -c "$(declare -f release); fixture='$fixture' release" 2>"$fixture/failure.out"; then
		echo "$failure must stop the release" >&2
		exit 1
	fi
	if grep -Eq '^(package|gh release (create|upload|edit))' "$fixture/gh.log"; then
		echo "$failure must stop before packaging" >&2
		exit 1
	fi
done
FAKE_RELEASE=published bash -c "$(declare -f release); fixture='$fixture' release" 2>"$fixture/failure.out" || true
grep -Fq 'already published; a published release is never changed' "$fixture/failure.out"
FAKE_RELEASE=missing bash -c "$(declare -f release); fixture='$fixture' release" 2>"$fixture/failure.out" || true
grep -Fq "merge the app's release pull request first" "$fixture/failure.out"

# The variable wins over the keychain.
SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET=environment-oauth-secret EXPECTED_OAUTH_SECRET=environment-oauth-secret release
test ! -s "$fixture/security.log"

# A stable release without the secret stops before packaging and says how to add it.
if FAKE_OAUTH_SECRET='' release 2>"$fixture/missing.out"; then
	echo 'a stable release without the OAuth secret must stop' >&2
	exit 1
fi
grep -Fq 'security add-generic-password -s subspace-github-oauth -a client-secret -w' "$fixture/missing.out"
if grep -Eq '^(package|gh release (create|upload|edit))' "$fixture/gh.log"; then echo 'a missing OAuth secret must stop before packaging' >&2; exit 1; fi

printf 'local change\n' >>"$fixture/README.md"
if release 2>/dev/null; then
	echo "local changes must stop the release" >&2
	exit 1
fi
if grep -q '^package' "$fixture/gh.log"; then echo "local changes must stop before packaging" >&2; exit 1; fi
echo "release-macos fixtures passed"
