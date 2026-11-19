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
target="${2:-$(rustc -vV | sed -n 's/^host: //p')}"
case "$target" in
aarch64-apple-darwin) ;;
*)
	echo "unsupported macOS target: $target" >&2
	exit 2
	;;
esac

root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
load_product
export MACOSX_DEPLOYMENT_TARGET="$MACOS_MINIMUM_VERSION"
rustup target add "$target" >&2
build_args=(build --release --locked --target "$target" -p "$binary_name")
if [[ -n "${SUBSPACE_BUILD_FEATURES:-}" ]]; then
	build_args+=(--features "$SUBSPACE_BUILD_FEATURES")
fi
cargo "${build_args[@]}" >&2
build_dir="$(cd "$target_dir/$target/release" && pwd)"

app="$out_dir/$display_name.app"
rm -rf "$app"
create_bundle "$build_dir/$binary_name" "$app" "$bundle_id" "$display_name"
bash "$root/scripts/bundle-mise.sh" "$app" >&2
echo "$app"
