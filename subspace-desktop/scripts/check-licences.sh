#!/usr/bin/env bash
# Check the native desktop licence policy without compiling crates.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=product.sh
source "$root/scripts/product.sh"

if [[ "$(uname -s)" = Darwin && "$(uname -m)" = x86_64 ]] &&
   ! command -v cargo-about >/dev/null 2>&1; then
    echo 'Skipping desktop release licence policy on Intel macOS: cargo-about has no upstream binary for this platform'
    exit 0
fi

check_copy() {
    if ! cmp -s "$1" "$2"; then
        echo "$2 must match $1 byte for byte" >&2
        return 1
    fi
}

# Check the union in one metadata pass: the default features plus the opt-in
# terminal and Copilot features, so a build with either is covered. Each
# archive also checks its exact target and feature flags.
target_args=()
for target in "${RELEASE_TARGETS[@]}"; do
    target_args+=(--target "$target")
done
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
# Policy needs the manifest expressions and top-level licence files. Archive
# generation also scans nested sources for bundled third-party notices.
{ printf 'max-depth = 1\n'; cat about.toml; } >"$scratch/about.toml"
check_rust_licences "${target_args[@]}" --features terminal,copilot --config "$scratch/about.toml" \
    >"$scratch/licences.json"
framework_manifest="$(jq -er '
    [.crates[].package | select(.name == "subspace-framework")] |
    if length != 1 then error("expected one resolved subspace-framework crate")
    elif .[0].source == null then .[0].manifest_path else "" end
' "$scratch/licences.json")"
if [[ -z "$framework_manifest" ]]; then
    echo 'Skipping local notice comparison for the published Subspace Framework crate'
else
    framework_root="$(dirname "$(resolve_target_dir "$framework_manifest")")"
    check_copy "$framework_root/services/charts/vendor/flint-chart-0.5.1/LICENSE" licenses/charts/LICENSE-FLINT-CHART
    check_copy "$framework_root/host/chat/assets/LICENSE-LOBE-ICONS" licenses/icons/LICENSE-LOBE-ICONS
    check_copy "$framework_root/board-kit/lucide/LICENSE-LUCIDE" licenses/icons/LICENSE-LUCIDE
    check_copy "$framework_root/host/app/themes/gpui-kit/LICENSE-APACHE" licenses/themes/gpui-kit/LICENSE-APACHE
    check_copy "$framework_root/host/app/themes/gpui-kit/PROVENANCE.md" licenses/themes/gpui-kit/PROVENANCE.md
    check_copy "$framework_root/host/chat/assets/LICENSE-OCTICONS" spaces/actions/assets/OCTICONS-LICENSE.txt
fi
markdown_manifest="$(jq -er '
    [.crates[].package | select(.name == "markdown")] |
    if length != 1 then error("expected one resolved markdown crate")
    else .[0].manifest_path end
' "$scratch/licences.json")"
markdown_manifest="$(resolve_target_dir "$markdown_manifest")"
check_copy "$(dirname "$markdown_manifest")/license" licenses/markdown/LICENSE
# Every crate an about-notices.json entry names must be in one of the
# workspace's lockfiles, so an upgrade cannot leave a pinned notice silently unused.
lockfiles=()
for lockfile in Cargo.lock terminal/panel/Cargo.lock terminal/runtime/Cargo.lock; do
    [[ -f "$lockfile" ]] && lockfiles+=("$lockfile")
done
locked="$(cat "${lockfiles[@]}" |
    awk -F'"' '/^name = /{name=$2} /^version = /{print name " v" $2}' | sort -u)"
stale="$(jq -r '.[].crates[]' about-notices.json | sort -u | comm -23 - <(printf '%s\n' "$locked"))"
if [[ -n "$stale" ]]; then
    echo 'about-notices.json names crates no lockfile has:' >&2
    echo "$stale" >&2
    exit 1
fi
check_copy licenses/sysmon/LICENSE spaces/tsx/crates/system/LICENSE-SYSMON
check_copy licenses/sysmon/LICENSE tsx-spaces/system-monitor/licenses/LICENSE-SYSMON
echo 'Desktop release licence policy passed'
