#!/usr/bin/env bash
# Build the unsigned Subspace macOS app and print its bundle path.
# Usage: bash scripts/build-macos-app.sh OUTPUT_DIR [RUST_TARGET]
set -euo pipefail

if [[ "$#" -lt 1 ]] || [[ "$#" -gt 2 ]]; then
	echo "usage: bash scripts/build-macos-app.sh OUTPUT_DIR [RUST_TARGET]" >&2
	exit 2
fi
if [[ "$(uname -s)" != Darwin ]]; then
	echo "the macOS app must be built on macOS" >&2
	exit 2
fi
out_dir="$1"
if [[ -z "$out_dir" ]]; then
	echo "an output directory is required" >&2
	exit 2
fi
root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
load_release_toolchain
require_product_licences
target="${2:-$("$RUSTC" -vV | sed -n 's/^host: //p')}"
case "$target" in
aarch64-apple-darwin) ;;
*)
	echo "unsupported macOS target: $target" >&2
	exit 2
	;;
esac

load_product
export MACOSX_DEPLOYMENT_TARGET="$MACOS_MINIMUM_VERSION"
rustup target add "$target" >&2
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
release_cargo_args "$target"
generate_rust_licences "$scratch/THIRD_PARTY_LICENSES" "$target"
python3 scripts/release-files.py build "$target_dir" "$binary_name" "$scratch" "${release_args[@]}"
unset SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET

app="$out_dir/$display_name.app"
rm -rf "$app"
create_bundle "$scratch/$binary_name" "$app" "$bundle_id" "$display_name"
cp "${rust_std_notices[@]}" "$app/Contents/Resources/licenses/rust-std/"
cp "$scratch/THIRD_PARTY_LICENSES" "$app/Contents/Resources/"
bash "$root/scripts/bundle-mise.sh" "$app" >&2
echo "$app"
