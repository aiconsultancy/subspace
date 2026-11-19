#!/usr/bin/env bash
# Build Subspace, sign it ad hoc and install it into ~/Applications.
# Usage: bash scripts/install-macos.sh [RUST_TARGET]
set -euo pipefail

if [ "$#" -gt 1 ]; then
	echo "usage: bash scripts/install-macos.sh [RUST_TARGET]" >&2
	exit 2
fi
root="${SUBSPACE_PACKAGE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
cd "$root"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/product.sh
source "$root/scripts/product.sh"
load_product
dest="$HOME/Applications"

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
app="$(bash "$root/scripts/build-macos-app.sh" "$stage" "$@")"
codesign --force --sign - "$app"
codesign --verify --strict "$app"

# Match the installed bundle only: debug builds from worktrees share the name.
if pgrep -f "^$dest/$display_name.app/Contents/MacOS/$binary_name" >/dev/null; then
	echo "$display_name is running. Quit it before you install." >&2
	exit 1
fi
mkdir -p "$dest"
# Replace the old app only after the new one builds and verifies.
rm -rf "$dest/$display_name.app"
ditto "$app" "$dest/$display_name.app"
echo "$dest/$display_name.app"
