#!/usr/bin/env bash
# Refresh the vendored Flint build that `subspace-charts` compiles charts with.
#
# This is a maintainer tool: the build never runs it, and a normal build needs
# neither Node.js nor the network. It downloads one pinned version with
# `npm pack`, checks the tarball against the integrity the registry publishes,
# and copies the Chart.js bundle and the licence into the crate.
#
#   bash scripts/refresh-flint.sh            # the pinned version
#   bash scripts/refresh-flint.sh 0.6.0      # another version
#
# After it runs: regenerate the parity fixture with the same version
# (services/charts/tests/parity/README.md), update PACKAGE, the vendor
# directory name and the notices in licenses/charts (and the copy the Subspace App ships), then run
# `cargo test -p subspace-charts`.

set -euo pipefail

PACKAGE="flint-chart"
VERSION="${1:-0.5.1}"
REGISTRY="https://registry.npmjs.org"
# The file inside the package: the self-contained ES module of the Chart.js
# backend, which is the only backend Subspace renders.
BUNDLE_IN_PACKAGE="package/dist/chartjs/index.js"
LICENSE_IN_PACKAGE="package/LICENSE"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
vendor="$root/services/charts/vendor/$PACKAGE-$VERSION"
notice="$root/licenses/charts/LICENSE-FLINT-CHART"

for command in npm curl shasum python3; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "refresh-flint: $command is needed but not installed" >&2
        exit 1
    fi
done

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"

echo "Downloading $PACKAGE@$VERSION"
tarball="$(npm pack "$PACKAGE@$VERSION" --silent)"

echo "Checking the tarball against the registry's integrity"
published="$(curl --proto '=https' --tlsv1.2 -fsSL "$REGISTRY/$PACKAGE/$VERSION" |
    python3 -c 'import json,sys; print(json.load(sys.stdin)["dist"]["integrity"])')"
if [[ "$published" != sha512-* ]]; then
    echo "refresh-flint: the registry published no sha512 integrity for $VERSION" >&2
    exit 1
fi
actual="sha512-$(shasum -a 512 "$tarball" | cut -d' ' -f1 | xxd -r -p | base64)"
if [[ "$actual" != "$published" ]]; then
    echo "refresh-flint: $tarball does not match the published integrity" >&2
    echo "  published $published" >&2
    echo "  actual    $actual" >&2
    exit 1
fi

tar -xzf "$tarball" "$BUNDLE_IN_PACKAGE" "$LICENSE_IN_PACKAGE"

mkdir -p "$vendor"
cp "$BUNDLE_IN_PACKAGE" "$vendor/chartjs.js"
cp "$LICENSE_IN_PACKAGE" "$vendor/LICENSE"
cp "$LICENSE_IN_PACKAGE" "$notice"

echo "Vendored $PACKAGE@$VERSION"
echo "  $vendor/chartjs.js ($(wc -c <"$vendor/chartjs.js" | tr -d ' ') bytes)"
echo "  sha256 $(shasum -a 256 "$vendor/chartjs.js" | cut -d' ' -f1)"
echo "Next: regenerate the parity fixture, update PACKAGE and the notices, then"
echo "run cargo test -p subspace-charts."
