#!/usr/bin/env bash
# Sign with secrets from the environment, reading any missing ones from Key
# Vault, on an approved macOS release runner or a signer's Mac.
set -euo pipefail
# Never trace credential arguments, even if the caller enabled shell tracing.
set +x

if [[ "$#" -ne 2 ]]; then
	echo 'usage: mise run package-macos-ci -- RUST_TARGET VERSION' >&2
	exit 2
fi
if [[ "$(uname -s)" != Darwin ]]; then
	echo 'signed application packaging requires macOS' >&2
	exit 2
fi
target="$1"
version="$2"
case "$target" in
aarch64-apple-darwin) ;;
*)
	echo "unsupported macOS target: $target" >&2
	exit 2
	;;
esac

root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
# shellcheck source=product.sh
source "$root/scripts/product.sh"
cd "$root"
# Secrets missing from the environment are read from the Azure Key Vault
# named by SUBSPACE_SIGNING_KEY_VAULT. There is no default vault. Key Vault
# names allow only letters, digits and dashes.
vault="${SUBSPACE_SIGNING_KEY_VAULT:-}"
for entry in MACOS_CERTIFICATE_BASE64:Subspace--MacosCertificateBase64 \
	MACOS_CERTIFICATE_PASSWORD:Subspace--MacosCertificatePassword \
	MACOS_SIGNING_IDENTITY:Subspace--MacosSigningIdentity \
	APPLE_API_KEY_BASE64:Subspace--AppleApiKeyBase64 \
	APPLE_API_KEY_ID:Subspace--AppleApiKeyId \
	APPLE_API_ISSUER_ID:Subspace--AppleApiIssuerId; do
	name="${entry%%:*}"
	secret="${entry#*:}"
	if [[ -n "${!name:-}" ]]; then
		continue
	fi
	if [[ -z "$vault" ]]; then
		echo "missing $name: set it, or set SUBSPACE_SIGNING_KEY_VAULT to the Key Vault that holds $secret" >&2
		exit 1
	fi
	if [[ ! "$vault" =~ ^[A-Za-z0-9-]+$ ]]; then
		echo 'SUBSPACE_SIGNING_KEY_VAULT must be a Key Vault name of letters, digits and dashes' >&2
		exit 1
	fi
	if ! command -v az >/dev/null; then
		echo "missing $name, and the Azure CLI (az) is not installed to read $secret from Key Vault $vault" >&2
		exit 1
	fi
	if ! value="$(az keyvault secret show --vault-name "$vault" --name "$secret" \
		--query value --output tsv)" || [[ -z "$value" ]]; then
		echo "could not read $secret from Key Vault $vault for $name. Run az login, and check that you have Key Vault Secrets User on the secret" >&2
		exit 1
	fi
	printf -v "$name" '%s' "$value"
done
unset value
certificate_base64="$MACOS_CERTIFICATE_BASE64"
certificate_password="$MACOS_CERTIFICATE_PASSWORD"
identity="$MACOS_SIGNING_IDENTITY"
api_key_base64="$APPLE_API_KEY_BASE64"
api_key_id="$APPLE_API_KEY_ID"
api_issuer_id="$APPLE_API_ISSUER_ID"
# Cargo and packaging subprocesses do not need the raw credential variables.
unset MACOS_CERTIFICATE_BASE64 MACOS_CERTIFICATE_PASSWORD MACOS_SIGNING_IDENTITY
unset APPLE_API_KEY_BASE64 APPLE_API_KEY_ID APPLE_API_ISSUER_ID

actual_version="$(cargo metadata --format-version=1 --no-deps --locked \
	| jq -er '.packages[] | select(.name == "subspace") | .version')"
if [[ "$version" != "$actual_version" ]]; then
	echo "expected Subspace version $version, found $actual_version" >&2
	exit 1
fi

umask 077
stage="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/subspace-signing.XXXXXX")"
keychain="$stage/signing.keychain-db"
original_keychains=()
search_list_changed=false
cleanup() {
	status=$?
	trap - EXIT
	if [[ "$search_list_changed" = true ]]; then
		security list-keychains -d user -s ${original_keychains[@]+"${original_keychains[@]}"} || status=1
	fi
	if [[ -f "$keychain" ]]; then
		security delete-keychain "$keychain" || status=1
	fi
	rm -rf "$stage"
	exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Preserve the runner's search list, including paths containing spaces.
original_search_list="$(security list-keychains -d user)"
while IFS= read -r original_path; do
	original_path="${original_path#*\"}"
	original_path="${original_path%\"*}"
	if [[ -n "$original_path" ]]; then
		original_keychains+=("$original_path")
	fi
done <<<"$original_search_list"

printf '%s' "$certificate_base64" | base64 --decode >"$stage/certificate.p12"
printf '%s' "$api_key_base64" | base64 --decode >"$stage/AuthKey.p8"
password="$(openssl rand -hex 32)"
security create-keychain -p "$password" "$keychain"
security set-keychain-settings -lut 21600 "$keychain"
security unlock-keychain -p "$password" "$keychain"
security import "$stage/certificate.p12" -k "$keychain" \
	-P "$certificate_password" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null

# --keychain restricts codesign's lookup; it does not add the Keychain to the
# user's search list. Set the cleanup flag first in case registration fails.
search_list_changed=true
security list-keychains -d user -s "$keychain" ${original_keychains[@]+"${original_keychains[@]}"}

# Import success does not prove that an identity is valid or usable for signing.
# Keep certificate names private; only public fingerprints are diagnostic output.
valid_identities="$(security find-identity -v -p codesigning "$keychain")"
fingerprints="$(awk 'length($2) == 40 && $2 !~ /[^[:xdigit:]]/ && /"Developer ID Application: / { print $2 }' <<<"$valid_identities")"
if [[ -z "$fingerprints" ]]; then
	echo 'No valid Developer ID Application identity found in the imported keychain. Check certificate validity, trust chain and its private key.' >&2
	exit 1
fi

# --dryrun exercises native identity selection and key access without writing a
# signature. Use a private copy of a system Mach-O binary, never the host file.
probe="$stage/signing-probe"
cp /usr/bin/true "$probe"
if codesign --dryrun --force --options runtime --timestamp --sign "$identity" --keychain "$keychain" "$probe"; then
	echo 'Signing identity preflight passed.'
else
	status=$?
	echo 'Signing preflight failed. Valid Developer ID Application SHA-1 fingerprints:' >&2
	printf '%s\n' "$fingerprints" >&2
	exit "$status"
fi
rm "$probe"

xcrun notarytool store-credentials subspace-ci --keychain "$keychain" \
	--key "$stage/AuthKey.p8" --key-id "$api_key_id" --issuer "$api_issuer_id"
unset certificate_base64 certificate_password api_key_base64 api_key_id api_issuer_id password
rm "$stage/certificate.p12" "$stage/AuthKey.p8"

SUBSPACE_MACOS_KEYCHAIN="$keychain" bash scripts/package-macos.sh "$identity" subspace-ci "$target"
app_name="$(no_api_suffix "$(cargo run --quiet --locked -p subspace-descriptor --bin subspace-product-metadata -- subspace "$version" | jq -er '.asset_stem')" -NoAPI)"
archive="$root/dist/${app_name}_${version}_darwin_arm64.zip"
test -s "$archive"
(
	cd "$root/dist"
	shasum -a 256 "$(basename "$archive")" >"$(basename "$archive").sha256"
)
