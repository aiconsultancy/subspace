#!/usr/bin/env bash
# Build, sign and notarize Subspace without changing other archives.
# Usage: mise run package-macos -- SIGNING_IDENTITY KEYCHAIN_PROFILE [RUST_TARGET]
set -euo pipefail

# Signing with the login keychain needs the desktop login session; from any
# other shell, run there. A keychain of its own (package-macos-ci.sh) does not.
if [[ "$(uname -s)" = Darwin && -z "${SUBSPACE_MACOS_KEYCHAIN:-}" && "$(launchctl managername 2>/dev/null || true)" != Aqua ]]; then
	scripts="$(cd "$(dirname "$0")" && pwd)"
	exec bash "$scripts/desktop-session.sh" bash "$scripts/$(basename "$0")" "$@"
fi

if [[ "$#" -lt 2 || "$#" -gt 3 ]]; then
	echo "usage: mise run package-macos -- SIGNING_IDENTITY KEYCHAIN_PROFILE [RUST_TARGET]" >&2
	exit 2
fi
if [[ "$(uname -s)" != Darwin ]]; then
	echo "the macOS app must be packaged on macOS" >&2
	exit 2
fi
identity="$1"
profile="$2"
if [[ -z "$identity" || -z "$profile" ]]; then
	echo "a Developer ID Application signing identity and Keychain profile are required" >&2
	exit 2
fi
xcrun --find notarytool >/dev/null
xcrun --find stapler >/dev/null

root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
load_release_toolchain
target="${3:-$("$RUSTC" -vV | sed -n 's/^host: //p')}"
case "$target" in
aarch64-apple-darwin) arch=arm64 ;;
*)
	echo "unsupported macOS target: $target" >&2
	exit 2
	;;
esac
load_product

# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/github-oauth-secret.sh
source "$root/scripts/github-oauth-secret.sh"
oauth_secret="$(github_oauth_secret)"
unset SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET
if [[ -z "$oauth_secret" ]]; then
	echo 'warning: no GitHub OAuth client secret, so this app signs in to GitHub with a code instead of the browser' >&2
fi

mkdir -p "$root/dist"
stage="$(mktemp -d "$root/dist/.$product-macos.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
# Only the build sees the secret; it is compiled into the executable.
app="$(SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET="$oauth_secret" bash "$root/scripts/build-macos-app.sh" "$stage" "$target")"
# A build cache that ignored the variable would ship without it.
if [[ -n "$oauth_secret" ]] && ! LC_ALL=C grep -qF -f <(printf '%s\n' "$oauth_secret") "$app/Contents/MacOS/$binary_name"; then
	echo 'the built app does not contain the GitHub OAuth client secret; run cargo clean -p subspace-github-auth, then package again' >&2
	exit 1
fi
unset oauth_secret
# GPUI's assets are embedded. No extra libraries or permissive entitlements
# are needed; sign the native executable as part of the enclosing bundle.
sign_args=(--force --options runtime --timestamp --sign "$identity")
notary_args=(--keychain-profile "$profile" --wait)
if [[ -n "${SUBSPACE_MACOS_KEYCHAIN:-}" ]]; then
	sign_args+=(--keychain "$SUBSPACE_MACOS_KEYCHAIN")
	notary_args+=(--keychain "$SUBSPACE_MACOS_KEYCHAIN")
fi
# The Subspace app bundles a nested mise executable (see bundle-mise.sh). Sign
# it before the enclosing bundle; codesign rejects an outer signature over an
# unsigned nested executable.
bundled_mise="$app/Contents/Resources/bin/mise"
if [[ -f "$bundled_mise" ]]; then
	codesign "${sign_args[@]}" "$bundled_mise"
fi
codesign "${sign_args[@]}" "$app"
codesign --verify --deep --strict --verbose=2 "$app"

archive="${asset_stem}_${version}_darwin_${arch}.zip"
ditto -c -k --keepParent "$app" "$stage/$archive"
xcrun notarytool submit "$stage/$archive" "${notary_args[@]}"
# Stapling fails unless Apple accepted the app, even if notarytool exited 0
# for a completed but rejected submission. Never publish an unchecked bundle.
xcrun stapler staple "$app"
xcrun stapler validate "$app"
codesign --verify --deep --strict --verbose=2 "$app"
spctl --assess --type execute --verbose=2 "$app"
# Recreate the archive so the distributed app includes the stapled ticket.
rm "$stage/$archive"
ditto -c -k --keepParent "$app" "$stage/$archive"

# Leave the last verified app untouched if building or notarization fails.
final="$root/dist/$display_name.app"
if [[ -L "$final" ]]; then echo "refusing to replace symbolic link: $final" >&2; exit 1; fi
rm -rf "$final"
mv "$app" "$final"
mv -f "$stage/$archive" "$root/dist/$archive"
echo "$final"
echo "$root/dist/$archive"
