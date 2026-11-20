#!/usr/bin/env bash
# Check the native desktop licence policy without compiling crates.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=product.sh
source "$root/scripts/product.sh"

check_copy() {
    if ! cmp -s "../subspace-framework/$1" "licenses/$2"; then
        echo "licenses/$2 must match subspace-framework/$1 byte for byte" >&2
        return 1
    fi
}

check_copy services/charts/vendor/flint-chart-0.5.1/LICENSE charts/LICENSE-FLINT-CHART
check_copy host/chat/assets/LICENSE-LOBE-ICONS icons/LICENSE-LOBE-ICONS
check_copy board-kit/lucide/LICENSE-LUCIDE icons/LICENSE-LUCIDE
check_copy vendor/markdown/license markdown/LICENSE
check_copy host/app/themes/gpui-kit/LICENSE-APACHE themes/gpui-kit/LICENSE-APACHE
check_copy host/app/themes/gpui-kit/PROVENANCE.md themes/gpui-kit/PROVENANCE.md

# Check the union in one metadata pass. Terminal retains the default
# features, and each archive also checks its exact target and feature flags.
target_args=()
for target in "${RELEASE_TARGETS[@]}"; do
    target_args+=(--target "$target")
done
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
# Policy needs the manifest expressions and top-level licence files. Archive
# generation also scans nested sources for bundled third-party notices.
{ printf 'max-depth = 1\n'; cat about.toml; } >"$scratch/about.toml"
check_rust_licences "${target_args[@]}" --features terminal --config "$scratch/about.toml" \
    --output-file "$scratch/licences.json"
echo 'Desktop release licence policy and copied notices passed'
