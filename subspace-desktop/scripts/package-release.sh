#!/usr/bin/env bash
# Build the Subspace release archive.
# When this machine can run the target, verify the built binary reports the
# product metadata version before replacing an archive.
# Usage: scripts/package-release.sh [RUST_TARGET [VERSION]]
set -euo pipefail

root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
host="$(rustc -vV | sed -n 's/^host: //p')"
target="${1:-$host}"
expected_version="${2:-}"
if ! release_target_supported "$target"; then
    echo "unsupported release target: $target" >&2
    exit 2
fi
load_product
if [[ -n "$expected_version" ]] && [[ "$version" != "${expected_version#v}" ]]; then
    echo "Cargo.toml has version $version, but the release is $expected_version" >&2
    exit 1
fi
if ! rustup target list --installed | grep -qx "$target"; then rustup target add "$target"; fi
exe=""
case "$target" in
    *-windows-*)
        exe=".exe"
        target_env="$(echo "$target" | tr 'a-z-' 'A-Z_')"
        export "CARGO_TARGET_${target_env}_RUSTFLAGS=-C target-feature=+crt-static"
        ;;
    *-apple-darwin) export MACOSX_DEPLOYMENT_TARGET="$MACOS_MINIMUM_VERSION" ;;
esac
mkdir -p "$root/dist"
stage="$(mktemp -d "$root/dist/.$product-release.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
release_cargo_args "$target"
generate_rust_licences "$stage/THIRD_PARTY_LICENSES" "$target"
cargo build --release --locked -p "$binary_name" "${release_args[@]}"
build_dir="$target_dir/$target/release"
if [[ "$host" = "$target" ]]; then
    reported_metadata="$("$build_dir/$binary_name$exe" __product_metadata | tr -d '\r')"
    reported="$(jq -er .version <<<"$reported_metadata")"
    if [[ "$reported" != "$version" ]]; then
        echo "$binary_name reports '$reported', expected '$version'" >&2
        exit 1
    fi
    if [[ "$(jq -r '.channel' <<<"$reported_metadata")" != "$channel" ]]; then
        echo 'built binary channel does not match product metadata' >&2
        exit 1
    fi
    echo "verified metadata: $binary_name $reported"
else
    echo "skipped the metadata version check: $host cannot run $target binaries"
fi
cp "$build_dir/$binary_name$exe" "$root/README.md" "$stage/"
stage_product_licences "$stage"
chmod 755 "$stage/$binary_name$exe"
printf '%s\n' "$product_metadata" >"$stage/product.json"
name="$binary_name-$version-$target"
files=("$binary_name$exe" README.md product.json LICENSE NOTICE THIRD_PARTY_LICENSES licenses)
case "$target" in
    *-linux-*)
        svg_relative="$(jq -er '.icon_paths[] | select(endswith(".svg"))' <<<"$product_metadata")"
        mkdir -p "$stage/$(dirname "$svg_relative")"
        cp "$root/products/$product/$svg_relative" "$stage/$svg_relative"
        files+=("$svg_relative")
        ;;
esac
case "$target" in
    *-windows-*)
        archive="$name.zip"
        (cd "$stage"; if command -v 7z >/dev/null 2>&1; then 7z a -tzip -bso0 "$archive" "${files[@]}"; else zip -qr "$archive" "${files[@]}"; fi)
        ;;
    *) archive="$name.tar.gz"; (cd "$stage"; tar -czf "$archive" "${files[@]}") ;;
esac
mv -f "$stage/$archive" "$root/dist/$archive"
(cd "$root/dist"; if command -v sha256sum >/dev/null 2>&1; then sha256sum "$archive" >"$archive.sha256"; else shasum -a 256 "$archive" >"$archive.sha256"; fi)
echo "$root/dist/$archive"
echo "$root/dist/$archive.sha256"
