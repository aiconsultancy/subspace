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
release_build_dir=""
trap 'rm -rf "$scratch" "${release_build_dir:+$release_build_dir}"' EXIT
release_cargo_args "$target"
generate_rust_licences "$scratch/THIRD_PARTY_LICENSES" "$target"
release_build_dir="$(new_release_build_dir)"
CARGO_TARGET_DIR="$release_build_dir" cargo build --release --locked -p "$binary_name" "${release_args[@]}" >&2
unset SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET
build_dir="$release_build_dir/$target/release"

app="$out_dir/$display_name.app"
rm -rf "$app"
create_bundle "$build_dir/$binary_name" "$app" "$bundle_id" "$display_name"
cp "${rust_std_notices[@]}" "$app/Contents/Resources/licenses/rust-std/"
cp "$scratch/THIRD_PARTY_LICENSES" "$app/Contents/Resources/"
bash "$root/scripts/bundle-mise.sh" "$app" >&2
echo "$app"
