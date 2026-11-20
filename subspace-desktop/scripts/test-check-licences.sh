#!/usr/bin/env bash
# Exercise licence policy and copied notices using a tiny offline Cargo graph.
set -euo pipefail
export CARGO_NET_OFFLINE=true
root="$(cd "$(dirname "$0")/.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
desktop="$scratch/desktop"
framework="$scratch/resolved-framework"
product="$desktop/products/subspace"
mkdir -p "$desktop/scripts" "$desktop/spaces/actions/assets" "$product/src" "$framework/src" "$scratch/bin"
cp "$root/scripts/"{check-licences.sh,product.sh,prerelease-common.sh,resolve-licence-notices.py} "$desktop/scripts/"
cp "$root/about.toml" "$desktop/"
cp "$root/../LICENSE" "$scratch/LICENSE"
printf '[]\n' >"$desktop/about-notices.json"
cp -R "$root/licenses" "$desktop/"
cp "$root/spaces/actions/assets/OCTICONS-LICENSE.txt" "$desktop/spaces/actions/assets/"

copy_notice() {
    mkdir -p "$framework/$(dirname "$1")"
    cp "$desktop/$2" "$framework/$1"
}
copy_notice services/charts/vendor/flint-chart-0.5.1/LICENSE licenses/charts/LICENSE-FLINT-CHART
copy_notice host/chat/assets/LICENSE-LOBE-ICONS licenses/icons/LICENSE-LOBE-ICONS
copy_notice board-kit/lucide/LICENSE-LUCIDE licenses/icons/LICENSE-LUCIDE
copy_notice vendor/markdown/license licenses/markdown/LICENSE
copy_notice host/app/themes/gpui-kit/LICENSE-APACHE licenses/themes/gpui-kit/LICENSE-APACHE
copy_notice host/app/themes/gpui-kit/PROVENANCE.md licenses/themes/gpui-kit/PROVENANCE.md
copy_notice host/chat/assets/LICENSE-OCTICONS spaces/actions/assets/OCTICONS-LICENSE.txt

cat >"$desktop/Cargo.toml" <<'EOF'
[workspace]
members = ["products/subspace"]
resolver = "2"
EOF
cat >"$product/Cargo.toml" <<'EOF'
[package]
name = "subspace"
version = "1.0.0"
license = "Apache-2.0"
[dependencies]
subspace-framework = { path = "../../../resolved-framework" }
[features]
terminal = []
EOF
write_framework_manifest() {
    cat >"$framework/Cargo.toml" <<'EOF'
[package]
name = "subspace-framework"
version = "1.0.0"
EOF
    if [[ -n "$1" ]]; then
        printf 'license = "%s"\n' "$1" >>"$framework/Cargo.toml"
    fi
}
write_framework_manifest MIT
touch "$product/src/lib.rs" "$framework/src/lib.rs"
(cd "$desktop"; cargo generate-lockfile --offline)
bash "$desktop/scripts/check-licences.sh"

check_failure() {
    if bash "$desktop/scripts/check-licences.sh" >"$scratch/check.log" 2>&1; then
        echo "Expected licence check failure matching $1" >&2
        exit 1
    fi
    grep -F -- "$1" "$scratch/check.log" || { cat "$scratch/check.log" >&2; return 1; }
}

echo 'fixture drift' >>"$framework/host/chat/assets/LICENSE-OCTICONS"
check_failure 'OCTICONS-LICENSE.txt must match'
# Reuse the real policy output and simulate a registry Framework source.
REAL_CARGO_ABOUT="$(command -v cargo-about)"
export REAL_CARGO_ABOUT
cat >"$scratch/bin/cargo-about" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
"$REAL_CARGO_ABOUT" "$@"
while [[ "$1" != --output-file ]]; do shift; done
jq '(.crates[].package | select(.name == "subspace-framework") | .source) =
    "registry+https://github.com/rust-lang/crates.io-index"' "$2" >"$2.new"
mv "$2.new" "$2"
EOF
chmod +x "$scratch/bin/cargo-about"
PATH="$scratch/bin:$PATH" bash "$desktop/scripts/check-licences.sh" >"$scratch/check.log"
grep -F 'Skipping local notice comparison for the published Subspace Framework crate' "$scratch/check.log"
copy_notice host/chat/assets/LICENSE-OCTICONS spaces/actions/assets/OCTICONS-LICENSE.txt

write_framework_manifest GPL-3.0-only
check_failure 'failed to satisfy license requirements'
write_framework_manifest ''
check_failure 'unable to synthesize license expression'
write_framework_manifest MIT
printf 'fixture notice\n' >"$framework/LICENSE"
cat >>"$desktop/about.toml" <<'EOF'

[subspace-framework.clarify]
license = "MIT"
[[subspace-framework.clarify.files]]
path = "LICENSE"
checksum = "0000000000000000000000000000000000000000000000000000000000000000"
EOF
check_failure 'clarification failed its source validation'
echo 'desktop licence policy and copied-notice fixtures passed'
