#!/usr/bin/env bash
# Test the run script with stub builds and no real window.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product-fixture.sh
source "$root/scripts/product-fixture.sh"
setup_product_fixture

metadata="$fixture/metadata/subspace.json"
binary="$(jq -r .binary_name "$metadata")"
name="$(jq -r .display_name "$metadata")"
identifier="$(jq -r .bundle_id "$metadata")"
SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET=fixture-build-secret ASSERT_OAUTH_SECRET_UNSET=1 \
    bash "$fixture/scripts/run-app.sh"
grep -Fq 'oauth secret' "$fixture/cargo.log"
app="$fixture/dist/dev/$name.app"
test -x "$app/Contents/MacOS/$binary"
test -f "$app/Contents/Resources/product.json"
test "$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")" = "$identifier.dev"
test "$(plutil -extract CFBundleExecutable raw -o - "$app/Contents/Info.plist")" = "$binary"
grep -Fq -- "open -a $app" "$fixture/actions.log"
: >"$fixture/actions.log"
(umask 077; printf '%s\n' fixture-access >"$fixture/access-jwt")
SUBSPACE_API_URL=http://127.0.0.1:18161/api SUBSPACE_API_ACCESS_JWT_FILE="$fixture/access-jwt" \
    bash "$fixture/scripts/run-app.sh"
grep -Fq -- '--env SUBSPACE_API_URL=http://127.0.0.1:18161/api' "$fixture/actions.log"
grep -Fq -- '--env SUBSPACE_API_ACCESS_JWT_FILE=' "$fixture/actions.log"
if grep -Fq -- fixture-access "$fixture/actions.log"; then
    echo "the access assertion itself reached open" >&2
    exit 1
fi
: >"$fixture/actions.log"
SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET=fixture-build-secret ASSERT_OAUTH_SECRET_UNSET=1 \
    bash "$fixture/scripts/run-app.sh" --help "value with spaces"
test ! -s "$fixture/actions.log"
grep -Fq -- '--help value with spaces' "$fixture/binary.log"
: >"$fixture/cargo.log"
SUBSPACE_BUILD_FEATURES=capture bash "$fixture/scripts/run-app.sh" --help
if ! grep -Fq -- "build --locked -p $binary --features capture" "$fixture/cargo.log"; then
    echo "expected the requested capture feature to reach cargo build" >&2
    exit 1
fi
test ! -s "$fixture/actions.log"
export FAKE_BUILD_FAIL=1
if bash "$fixture/scripts/run-app.sh"; then echo "expected build failure" >&2; exit 1; fi
test ! -s "$fixture/actions.log"
test -x "$app/Contents/MacOS/$binary"
unset FAKE_BUILD_FAIL
export FAKE_UNAME=Linux
SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET=fixture-build-secret ASSERT_OAUTH_SECRET_UNSET=1 \
    bash "$fixture/scripts/run-app.sh" one "two words"
grep -Fq 'one two words' "$fixture/binary.log"
echo 'run-app checks passed'
