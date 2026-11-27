#!/usr/bin/env bash
# Exercise the real CI wrapper with fake Apple tools and fixture credentials.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product-fixture.sh
source "$root/scripts/product-fixture.sh"
setup_product_fixture
mkdir -p "$fixture/runner" "$fixture/state"
export RUNNER_TEMP="$fixture/runner" FIXTURE_STATE="$fixture/state"
export ASSERT_SIGNING_SECRETS_UNSET=1 FAIL_AT=''
unset SUBSPACE_SIGNING_KEY_VAULT

cat >"$fixture/bin/security" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'security %s\n' "$1" >>"$FIXTURE_STATE/log"
case "$1" in
list-keychains)
    test "$2" = -d && test "$3" = user
    if [[ "$#" -eq 3 ]]; then
        while IFS= read -r keychain; do
            printf '    "%s"\n' "$keychain"
        done <"$FIXTURE_STATE/search-list"
    else
        test "$4" = -s
        shift 4
        : >"$FIXTURE_STATE/search-list"
        for keychain in "$@"; do
            printf '%s\n' "$keychain" >>"$FIXTURE_STATE/search-list"
        done
        if [[ "$FAIL_AT" = search-list && "${1:-}" = "$(cat "$FIXTURE_STATE/keychain")" ]]; then
            exit 18
        fi
    fi ;;
create-keychain)
    touch "$4"
    printf '%s\n' "$4" >"$FIXTURE_STATE/keychain" ;;
set-keychain-settings | unlock-keychain | set-key-partition-list) ;;
import)
    test -f "$2"
    test "$(cat "$2")" = fixture-certificate
    test "$3" = -k && test "$4" = "$(cat "$FIXTURE_STATE/keychain")"
    test "$5" = -P && test "$6" = fixture-password
    [[ "$FAIL_AT" != import ]] || exit 17 ;;
find-identity)
    test "$2" = -v && test "$3" = -p && test "$4" = codesigning
    test "$5" = "$(cat "$FIXTURE_STATE/keychain")"
    case "$FAIL_AT" in
        no-identity) printf '     0 valid identities found\n' ;;
        wrong-kind)
            printf '  1) AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "Apple Development: Fixture (TESTTEAM)"\n'
            printf '     1 valid identities found\n' ;;
        *)
            printf '  1) AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "Developer ID Application: Fixture (TESTTEAM)"\n'
            printf '     1 valid identities found\n' ;;
    esac ;;
delete-keychain) rm "$2" ;;
*) exit 90 ;;
esac
SH
cat >"$fixture/bin/xcrun" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'xcrun %s %s\n' "$1" "${2:-}" >>"$FIXTURE_STATE/log"
case "$1 ${2:-}" in
'--find notarytool' | '--find stapler') ;;
'notarytool store-credentials')
    test "$3" = subspace-ci
    test "$4" = --keychain && test "$5" = "$(cat "$FIXTURE_STATE/keychain")"
    test "$6" = --key && test "$(cat "$7")" = fixture-api-key
    test "$8" = --key-id && test "$9" = fixture-key-id
    test "${10}" = --issuer && test "${11}" = fixture-issuer ;;
'notarytool submit')
    test "$4" = --keychain-profile && test "$5" = subspace-ci
    test "$6" = --wait && test "$7" = --keychain
    test "$8" = "$(cat "$FIXTURE_STATE/keychain")"
    [[ "$FAIL_AT" != notarize ]] || exit 19 ;;
'stapler staple' | 'stapler validate') ;;
*) exit 91 ;;
esac
SH
cat >"$fixture/bin/codesign" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'codesign %s\n' "$*" >>"$FIXTURE_STATE/log"
if [[ "$1" = --dryrun || "$1" = --force ]]; then
    if ! grep -Fxq "$(cat "$FIXTURE_STATE/keychain")" "$FIXTURE_STATE/search-list"; then
        echo 'fixture codesign: no identity found in the user search list' >&2
        exit 22
    fi
fi
if [[ "$1" = --dryrun ]]; then
    test "$2" = --force && test "$3" = --options && test "$4" = runtime
    test "$5" = --timestamp && test "$6" = --sign
    test "$8" = --keychain && test "$9" = "$(cat "$FIXTURE_STATE/keychain")"
    test -x "${10}"
    case "$7" in
        'Developer ID Application: Fixture (TESTTEAM)' | AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA) ;;
        *) echo 'fixture codesign: no identity found' >&2; exit 22 ;;
    esac
    [[ "$FAIL_AT" != preflight ]] || exit 22
elif [[ "$1" = --force ]]; then
    printf '%s\n' "$*" | grep -F -- "--keychain $(cat "$FIXTURE_STATE/keychain")" >/dev/null
    [[ "$FAIL_AT" != sign ]] || exit 21
fi
SH
cat >"$fixture/bin/az" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$1 $2 $3" = 'keyvault secret show'
test "$4" = --vault-name && test "$6" = --name
test "$8" = --query && test "$9" = value && test "${10}" = --output && test "${11}" = tsv
printf 'az %s %s\n' "$5" "$7" >>"$FIXTURE_STATE/log"
if [[ ! -f "$FIXTURE_STATE/vault/$5/$7" ]]; then
    echo "fixture az: SecretNotFound $7" >&2
    exit 3
fi
cat "$FIXTURE_STATE/vault/$5/$7"
SH
cat >"$fixture/bin/ditto" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
test "$1" = -c && test "$2" = -k && test "$3" = --keepParent
printf 'fixture stapled app archive\n' >"$5"
SH
cat >"$fixture/bin/plutil" <<'SH'
#!/usr/bin/env bash
exit 0
SH
cat >"$fixture/bin/spctl" <<'SH'
#!/usr/bin/env bash
printf 'spctl\n' >>"$FIXTURE_STATE/log"
SH
cat >"$fixture/bin/openssl" <<'SH'
#!/usr/bin/env bash
printf 'fixture-keychain-password\n'
SH
chmod 755 "$fixture/bin/"*
version="$(jq -r .version "$fixture/metadata/subspace.json")"

reset_case() {
    rm -rf "$FIXTURE_STATE" "$fixture/dist"
    rm -f "$fixture/cargo.log"
    mkdir -p "$FIXTURE_STATE"
    printf '%s\n' '/fixture/login.keychain-db' '/fixture/Company Signing Keys.keychain-db' \
        >"$FIXTURE_STATE/search-list"
    cp "$FIXTURE_STATE/search-list" "$FIXTURE_STATE/expected-search-list"
    export MACOS_CERTIFICATE_BASE64 MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY
    export APPLE_API_KEY_BASE64 APPLE_API_KEY_ID APPLE_API_ISSUER_ID
    MACOS_CERTIFICATE_BASE64="$(printf fixture-certificate | base64)"
    MACOS_CERTIFICATE_PASSWORD='fixture-password'
    MACOS_SIGNING_IDENTITY='Developer ID Application: Fixture (TESTTEAM)'
    APPLE_API_KEY_BASE64="$(printf fixture-api-key | base64)"
    APPLE_API_KEY_ID='fixture-key-id'
    APPLE_API_ISSUER_ID='fixture-issuer'
    FAIL_AT=''
}

run_case() {
    local expected="$1" version="$2" status=0
    "$BASH" "$fixture/scripts/package-macos-ci.sh" \
        aarch64-apple-darwin "$version" >"$FIXTURE_STATE/output" 2>&1 || status=$?
    if [[ "$status" != "$expected" ]]; then
        cat "$FIXTURE_STATE/output" >&2
        echo "expected exit $expected, got $status" >&2
        exit 1
    fi
    if find "$RUNNER_TEMP" -name 'subspace-signing.*' | grep -q .; then
        echo 'credential directory survived' >&2
        exit 1
    fi
    if [[ -f "$FIXTURE_STATE/keychain" ]]; then
        grep -Fx 'security delete-keychain' "$FIXTURE_STATE/log" >/dev/null
    fi
    if ! cmp -s "$FIXTURE_STATE/expected-search-list" "$FIXTURE_STATE/search-list"; then
        echo 'the original Keychain search list was not restored' >&2
        exit 1
    fi
    if [[ "$expected" != 0 ]] && find "$fixture/dist" -type f \( -name '*.zip' -o -name '*.sha256' \) 2>/dev/null | grep -q .; then
        echo 'failed package produced a publishable artifact' >&2
        exit 1
    fi
}

for failure in no-identity wrong-kind preflight; do
    reset_case
    FAIL_AT="$failure"
    expected=1
    if [[ "$failure" = preflight ]]; then expected=22; fi
    run_case "$expected" "$version"
    if grep -qE '^(build|run) ' "$fixture/cargo.log"; then
        echo 'signing preflight failure reached compilation' >&2
        exit 1
    fi
    if grep -Fq 'xcrun notarytool store-credentials' "$FIXTURE_STATE/log"; then
        echo 'signing preflight failure reached notarization credential setup' >&2
        exit 1
    fi
    if [[ "$failure" = preflight ]]; then
        grep -Fq AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA "$FIXTURE_STATE/output"
    fi
    echo "PASS: $failure blocks compilation and deletes the keychain"
done

reset_case
MACOS_SIGNING_IDENTITY='Developer ID Application: Missing (OTHERTEAM)'
run_case 22 "$version"
if grep -qE '^(build|run) ' "$fixture/cargo.log"; then
    echo 'an unmatched signing selector reached compilation' >&2
    exit 1
fi
echo 'PASS: an unmatched selector fails before compilation without selecting another identity'

reset_case
run_case 0 "$version"
archive="$fixture/dist/$(jq -r .asset_stem "$fixture/metadata/subspace.json")_${version}_darwin_arm64.zip"
test -s "$archive"
(cd "$fixture/dist" && shasum -a 256 -c "$(basename "$archive").sha256")
grep -Fx spctl "$FIXTURE_STATE/log" >/dev/null
grep -Fq 'codesign --dryrun' "$FIXTURE_STATE/log"
echo "PASS: signed app fixture and credential cleanup"

reset_case
MACOS_SIGNING_IDENTITY=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA
run_case 0 "$version"
echo 'PASS: a certificate SHA-1 selector passes the signing preflight'

reset_case
: >"$FIXTURE_STATE/search-list"
: >"$FIXTURE_STATE/expected-search-list"
run_case 0 "$version"
echo 'PASS: an initially empty Keychain search list is restored'

if grep -q '^az ' "$FIXTURE_STATE/log"; then
    echo 'environment secrets still read Key Vault' >&2
    exit 1
fi
echo 'PASS: environment secrets do not read Key Vault'

store_vault_secrets() {
    local vault="$1/$2"
    mkdir -p "$vault"
    printf '%s\n' "$MACOS_CERTIFICATE_BASE64" >"$vault/Subspace--MacosCertificateBase64"
    printf '%s\n' "$MACOS_CERTIFICATE_PASSWORD" >"$vault/Subspace--MacosCertificatePassword"
    printf '%s\n' "$MACOS_SIGNING_IDENTITY" >"$vault/Subspace--MacosSigningIdentity"
    printf '%s\n' "$APPLE_API_KEY_BASE64" >"$vault/Subspace--AppleApiKeyBase64"
    printf '%s\n' "$APPLE_API_KEY_ID" >"$vault/Subspace--AppleApiKeyId"
    printf '%s\n' "$APPLE_API_ISSUER_ID" >"$vault/Subspace--AppleApiIssuerId"
}

reset_case
MACOS_CERTIFICATE_PASSWORD=''
run_case 1 "$version"
test ! -f "$FIXTURE_STATE/keychain"
if grep -q '^az ' "$FIXTURE_STATE/log" 2>/dev/null; then
    echo 'a missing vault name still ran the Azure CLI' >&2
    exit 1
fi
grep -Fq 'missing MACOS_CERTIFICATE_PASSWORD: set it, or set SUBSPACE_SIGNING_KEY_VAULT' "$FIXTURE_STATE/output"
echo 'PASS: a missing secret without SUBSPACE_SIGNING_KEY_VAULT fails before keychain creation'

reset_case
export SUBSPACE_SIGNING_KEY_VAULT='fixture kv'
MACOS_CERTIFICATE_PASSWORD=''
run_case 1 "$version"
unset SUBSPACE_SIGNING_KEY_VAULT
grep -Fq 'SUBSPACE_SIGNING_KEY_VAULT must be a Key Vault name' "$FIXTURE_STATE/output"
echo 'PASS: an invalid vault name is rejected before the Azure CLI runs'

reset_case
store_vault_secrets "$FIXTURE_STATE/vault" example-signing-kv
export SUBSPACE_SIGNING_KEY_VAULT=example-signing-kv
unset MACOS_CERTIFICATE_BASE64 MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY
unset APPLE_API_KEY_BASE64 APPLE_API_KEY_ID APPLE_API_ISSUER_ID
run_case 0 "$version"
unset SUBSPACE_SIGNING_KEY_VAULT
test "$(grep -c '^az example-signing-kv Subspace--' "$FIXTURE_STATE/log")" = 6
echo 'PASS: missing secrets are read from the configured Key Vault'

reset_case
store_vault_secrets "$FIXTURE_STATE/vault" fixture-kv
export SUBSPACE_SIGNING_KEY_VAULT=fixture-kv
MACOS_CERTIFICATE_PASSWORD=''
run_case 0 "$version"
unset SUBSPACE_SIGNING_KEY_VAULT
test "$(grep '^az ' "$FIXTURE_STATE/log")" = 'az fixture-kv Subspace--MacosCertificatePassword'
echo 'PASS: only the missing secret is read, from the chosen vault'

reset_case
export SUBSPACE_SIGNING_KEY_VAULT=example-signing-kv
MACOS_CERTIFICATE_PASSWORD=''
run_case 1 "$version"
unset SUBSPACE_SIGNING_KEY_VAULT
test ! -f "$FIXTURE_STATE/keychain"
grep -Fq 'could not read Subspace--MacosCertificatePassword from Key Vault example-signing-kv' "$FIXTURE_STATE/output"
echo 'PASS: a secret missing from Key Vault is rejected before keychain creation'

for failure in search-list import sign notarize; do
    reset_case
    FAIL_AT="$failure"
    case "$failure" in search-list) expected=18 ;; import) expected=17 ;; sign) expected=21 ;; notarize) expected=19 ;; esac
    run_case "$expected" "$version"
    echo "PASS: $failure failure blocks artifacts and deletes the keychain"
done

# --run hands the temporary keychain, the identity and the notary profile to
# a command, such as `make release-macos`, in the caller's directory and with
# the caller's umask; the raw credentials stay behind.
mkdir -p "$fixture/caller"
cat >"$fixture/signing-step.sh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf 'step %s\n' "$*" >>"$FIXTURE_STATE/log"
test "$PWD" = "$FIXTURE_CALLER"
test "$(umask)" = 0022
test "${SUBSPACE_MACOS_KEYCHAIN:-}" = "$(cat "$FIXTURE_STATE/keychain")"
grep -Fxq "$SUBSPACE_MACOS_KEYCHAIN" "$FIXTURE_STATE/search-list"
test "${SIGNING_IDENTITY:-}" = 'Developer ID Application: Fixture (TESTTEAM)'
test "${KEYCHAIN_PROFILE:-}" = subspace-ci
for name in MACOS_CERTIFICATE_BASE64 MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY \
    APPLE_API_KEY_BASE64 APPLE_API_KEY_ID APPLE_API_ISSUER_ID; do
    if [[ -n "${!name:-}" ]]; then echo "the signing step saw $name" >&2; exit 5; fi
done
[[ "$FAIL_AT" != step ]] || exit 23
SH
chmod 755 "$fixture/signing-step.sh"
export FIXTURE_CALLER
FIXTURE_CALLER="$(cd "$fixture/caller" && pwd -P)"

run_step() {
    local expected="$1" status=0
    shift
    (cd "$FIXTURE_CALLER" && umask 022 && "$BASH" "$fixture/scripts/package-macos-ci.sh" "$@") \
        >"$FIXTURE_STATE/output" 2>&1 || status=$?
    if [[ "$status" != "$expected" ]]; then
        cat "$FIXTURE_STATE/output" >&2
        echo "expected exit $expected, got $status" >&2
        exit 1
    fi
    if find "$RUNNER_TEMP" -name 'subspace-signing.*' | grep -q .; then
        echo 'credential directory survived' >&2
        exit 1
    fi
    if ! cmp -s "$FIXTURE_STATE/expected-search-list" "$FIXTURE_STATE/search-list"; then
        echo 'the original Keychain search list was not restored' >&2
        exit 1
    fi
}

reset_case
run_step 0 --run "$fixture/signing-step.sh" release 'two words'
grep -Fxq 'step release two words' "$FIXTURE_STATE/log"
grep -Fx 'security delete-keychain' "$FIXTURE_STATE/log" >/dev/null
if grep -qE '^(build|run|metadata) ' "$fixture/cargo.log" 2>/dev/null; then
    echo '--run also packaged an app of its own' >&2
    exit 1
fi
echo 'PASS: --run gives a command the temporary keychain without the raw credentials'

reset_case
FAIL_AT=step
run_step 23 --run "$fixture/signing-step.sh"
grep -Fx 'security delete-keychain' "$FIXTURE_STATE/log" >/dev/null
echo 'PASS: a failing signing step keeps its status and deletes the keychain'

reset_case
run_step 2 --run
test ! -f "$FIXTURE_STATE/keychain"
echo 'PASS: --run without a command is refused before keychain creation'

echo 'macOS CI signing contracts passed (fake Apple tools only)'
