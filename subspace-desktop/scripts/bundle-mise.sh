#!/usr/bin/env bash
# Bundle a pinned, checksum-verified mise CLI into a macOS app's
# Contents/Resources/bin so Board Lab can provision runtimes without relying
# on a host install.
#
# Usage: bash scripts/bundle-mise.sh APP_BUNDLE_PATH
set -euo pipefail

if [[ "$#" -ne 1 ]]; then
	echo "usage: bash scripts/bundle-mise.sh APP_BUNDLE_PATH" >&2
	exit 2
fi
if [[ "$(uname -s)" != Darwin ]]; then
	echo "mise bundling is macOS-only" >&2
	exit 2
fi
if [[ "$(uname -m)" != arm64 ]]; then
	echo "mise bundling only builds the macOS arm64 asset" >&2
	exit 2
fi
app="$1"
if [[ ! -d "$app/Contents/Resources" ]]; then
	echo "missing app bundle Resources directory: $app/Contents/Resources" >&2
	exit 1
fi

# Pinned version. Bump deliberately; the expected checksum below must match.
mise_version="2026.10.17"
asset="mise-v${mise_version}-macos-arm64.tar.gz"
repo="https://github.com/jdx/mise/releases/download/v${mise_version}"
# Overridable only for offline fixtures (test-bundle-mise.sh), which build a
# fake archive and cannot reproduce the real asset's bytes or checksum.
expected_sha256="${SUBSPACE_MISE_SHA256:-d7c50c1683524a764260986ccc9c41c0de5e1fcd491714b5640b657b76781af6}"

root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cache_dir="${SUBSPACE_MISE_CACHE_DIR:-$root/.cache/mise}"
mkdir -p "$cache_dir"
archive="$cache_dir/$asset"

temp_dir="$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/subspace-bundle-mise.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT

if [[ ! -f "$archive" ]]; then
	echo "downloading $asset" >&2
	download="$temp_dir/$asset"
	curl --proto '=https' --tlsv1.2 --fail --silent --show-error --location \
		-o "$download" "$repo/$asset"
	mv "$download" "$archive"
fi

if command -v sha256sum >/dev/null 2>&1; then
	actual_sha256="$(sha256sum "$archive" | cut -d' ' -f1)"
elif command -v shasum >/dev/null 2>&1; then
	actual_sha256="$(shasum -a 256 "$archive" | cut -d' ' -f1)"
else
	echo "neither sha256sum nor shasum is installed; cannot verify the download" >&2
	exit 1
fi
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
	echo "checksum mismatch for $asset" >&2
	echo "  expected $expected_sha256" >&2
	echo "  actual   $actual_sha256" >&2
	rm -f "$archive"
	exit 1
fi

extract_dir="$temp_dir/extract"
mkdir -p "$extract_dir"
tar -xzf "$archive" -C "$extract_dir"
mise_binary="$extract_dir/mise/bin/mise"
mise_license="$extract_dir/mise/LICENSE"
if [[ ! -f "$mise_binary" ]]; then
	echo "mise binary not found in extracted archive: $asset" >&2
	exit 1
fi
if [[ ! -f "$mise_license" ]]; then
	echo "mise license notice is missing from the pinned archive" >&2
	exit 1
fi
chmod 755 "$mise_binary"

actual_version="$("$mise_binary" --version)"
if [[ "$actual_version" != *"$mise_version"* ]]; then
	echo "extracted mise reports an unexpected version: $actual_version" >&2
	exit 1
fi

bin_dir="$app/Contents/Resources/bin"
mkdir -p "$bin_dir"
cp "$mise_binary" "$bin_dir/mise"
chmod 755 "$bin_dir/mise"
license_dir="$app/Contents/Resources/licenses/mise"
mkdir -p "$license_dir"
cp "$mise_license" "$license_dir/LICENSE"
echo "$bin_dir/mise"
