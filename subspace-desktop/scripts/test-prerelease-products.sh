#!/usr/bin/env bash
# Exercise channel packaging offline, before the real Rust metadata is built.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
input="$(mktemp -d "${TMPDIR:-/tmp}/subspace-channel-metadata.XXXXXX")"
trap 'rm -rf "${input:?}"' EXIT
write_channel_metadata() {
    local directory="$1" channel="$2" suffix="" title="" label_suffix="" stem_suffix=""
    if [[ "$channel" = integration ]]; then
        channel=stable
        suffix="-feat-subspace-desktop.202611151345.gabc1234"
    elif [[ "$channel" != stable ]]; then
        suffix="-$channel.202611010200.g0123456"
        case "$channel" in nightly) title=Nightly ;; alpha) title=Alpha ;; beta) title=Beta ;; esac
        label_suffix=" ($title)"
        stem_suffix="-$title"
    fi
    jq -n --arg subspace "1.10.2$suffix" '{packages:[{name:"subspace",version:$subspace}]}' >"$directory/workspace.json"
    local icon_stem=subspace svg_stem=subspace-app
    if [[ "$channel" = nightly ]]; then
        icon_stem=subspace-nightly
        svg_stem="$icon_stem"
    fi
    jq -n --arg name "Subspace$label_suffix" --arg stem "Subspace$stem_suffix" \
        --arg channel "$channel" --arg version "1.10.2$suffix" \
        --arg identifier "com.aiconsultancy.subspace${label_suffix:+.$channel}" --arg icon "assets/$icon_stem" --arg svg "assets/$svg_stem.svg" \
        '{binary_name:"subspace",display_name:$name,asset_stem:$stem,channel:$channel,bundle_id:$identifier,version:$version,subspace_version:$version,icon_paths:[($icon + ".icns"),($icon + ".ico"),$svg]}' >"$directory/subspace.json"
}

check_packaged_licences() {
    local destination="$1" target="$2" file
    cmp "$root/../LICENSE" "$destination/LICENSE"
    cmp "$root/NOTICE" "$destination/NOTICE"
    grep -Fq "Subspace binary for $target" "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq -- '- fixture-linked v1.0.0' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq 'Fixture linked licence' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq -- '- fixture-generic v1.0.0 (MIT)' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq -- '- fixture-mpl v3.1.4 <https://static.crates.io/crates/fixture-mpl/fixture-mpl-3.1.4.crate>' "$destination/THIRD_PARTY_LICENSES"
    if grep -Fxq -- '- fixture-linked v1.0.0 (MIT)' "$destination/THIRD_PARTY_LICENSES"; then
        echo 'the source-backed notice was reported as a generic text' >&2; exit 1
    fi
    if grep -Eq 'fixture-macro|Fixture build-only licence' "$destination/THIRD_PARTY_LICENSES"; then
        echo 'the runtime report includes a compile-time proc macro' >&2; exit 1
    fi
    for file in canvas/mermaid-rs-renderer-MIT.txt \
        charts/LICENSE-FLINT-CHART charts/LICENSE-QUICKJS-NG charts/LICENSE-RQUICKJS \
        icons/LICENSE-FEATHER icons/LICENSE-LOBE-ICONS icons/LICENSE-LUCIDE \
        markdown/LICENSE onnxruntime/LICENSE onnxruntime/ThirdPartyNotices.txt \
        terminal/LICENSE-APACHE terminal/LICENSE-GHOSTTY terminal/LICENSE-MIT terminal/LICENSE-PORTABLE-PTY \
        themes/gpui-kit/LICENSE-APACHE themes/gpui-kit/PROVENANCE.md; do
        cmp "$root/licenses/$file" "$destination/licenses/$file"
    done
    for file in README.md OCTICONS-LICENSE.txt; do
        cmp "$root/spaces/actions/assets/$file" "$destination/licenses/icons/octicons/$file"
    done
}

write_channel_metadata "$input" stable
# shellcheck source-path=SCRIPTDIR
# shellcheck source=product-fixture.sh
source "$root/scripts/product-fixture.sh"
setup_product_fixture "$input"
rm -rf "${input:?}"
for channel in stable nightly alpha beta integration; do
    write_channel_metadata "$fixture/metadata" "$channel"
    export SUBSPACE_RELEASE_CHANNEL="$channel"
    metadata="$fixture/metadata/subspace.json"
    version="$(jq -r .version "$metadata")"
    binary="$(jq -r .binary_name "$metadata")"
    name="$(jq -r .display_name "$metadata")"
    stem="$(jq -r .asset_stem "$metadata")"
    touch "$fixture/cargo.log"
    builds_before="$(grep -c '^build ' "$fixture/cargo.log" || true)"
    cp "$fixture/metadata/workspace.json" "$fixture/workspace-current.json"
    jq '.packages[].version = "9.9.9"' "$fixture/workspace-current.json" >"$fixture/metadata/workspace.json"
    if bash "$fixture/scripts/package-release.sh" x86_64-unknown-linux-gnu "$version" >"$fixture/stale-workspace.log" 2>&1; then
        echo 'stale workspace versions must fail before packaging' >&2; exit 1
    fi
    test "$(grep -c '^build ' "$fixture/cargo.log" || true)" = "$builds_before"
    mv "$fixture/workspace-current.json" "$fixture/metadata/workspace.json"
    for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu aarch64-apple-darwin x86_64-pc-windows-msvc; do
        bash "$fixture/scripts/package-release.sh" "$target" "$version"
    done
    svg_relative="$(jq -r '.icon_paths[] | select(endswith(".svg"))' "$metadata")"
    unpacked="$fixture/unpacked-$channel"
    mkdir "$unpacked"
    tar -xzf "$fixture/dist/$binary-$version-x86_64-unknown-linux-gnu.tar.gz" -C "$unpacked"
    test -f "$unpacked/$svg_relative" || { echo "Linux archive is missing $svg_relative" >&2; exit 1; }
    cmp "$root/products/subspace/$svg_relative" "$unpacked/$svg_relative"
    test "$(find "$unpacked/assets" -type f | wc -l | tr -d ' ')" = 1
    if [[ "$channel" = stable ]]; then
        check_packaged_licences "$unpacked" x86_64-unknown-linux-gnu
        mkdir "$fixture/unpacked-arm64-linux"
        tar -xzf "$fixture/dist/$binary-$version-aarch64-unknown-linux-gnu.tar.gz" -C "$fixture/unpacked-arm64-linux"
        check_packaged_licences "$fixture/unpacked-arm64-linux" aarch64-unknown-linux-gnu
        mkdir "$fixture/unpacked-darwin" "$fixture/unpacked-windows"
        tar -xzf "$fixture/dist/$binary-$version-aarch64-apple-darwin.tar.gz" -C "$fixture/unpacked-darwin"
        unzip -q "$fixture/dist/$binary-$version-x86_64-pc-windows-msvc.zip" -d "$fixture/unpacked-windows"
        check_packaged_licences "$fixture/unpacked-darwin" aarch64-apple-darwin
        check_packaged_licences "$fixture/unpacked-windows" x86_64-pc-windows-msvc
    fi
    if [[ "$(/usr/bin/uname -s)" = Darwin ]]; then
        bash "$fixture/scripts/package-macos.sh" 'fixture signing identity' 'fixture notary profile'
        app="$fixture/dist/$name.app"
        test -x "$app/Contents/MacOS/$binary"
        test -f "$fixture/dist/${stem}_${version}_darwin_arm64.zip"
        if [[ "$channel" = stable ]]; then
            ditto -x -k "$fixture/dist/${stem}_${version}_darwin_arm64.zip" "$fixture/unpacked-app"
            check_packaged_licences "$fixture/unpacked-app/$name.app/Contents/Resources" aarch64-apple-darwin
        fi
        cmp "$metadata" "$app/Contents/Resources/product.json"
        icon_relative="$(jq -r '.icon_paths[] | select(endswith(".icns"))' "$metadata")"
        icon_name="$(basename "$icon_relative" .icns)"
        cmp "$root/products/subspace/$icon_relative" "$app/Contents/Resources/$icon_name.icns"
        test "$(plutil -extract CFBundleIconFile raw -o - "$app/Contents/Info.plist")" = "$icon_name"
        test "$(plutil -extract CFBundleShortVersionString raw -o - "$app/Contents/Info.plist")" = "${version%%-*}"
        test "$(plutil -extract CFBundleVersion raw -o - "$app/Contents/Info.plist")" = "${version%%-*}"
        test "$(plutil -extract CFBundleIdentifier raw -o - "$app/Contents/Info.plist")" = "$(jq -r .bundle_id "$metadata")"
        test "$(plutil -extract CFBundleDisplayName raw -o - "$app/Contents/Info.plist")" = "$name"
    fi
    jq '.channel = (if .channel == "nightly" then "beta" else "nightly" end)' "$metadata" >"$metadata.new"
    mv "$metadata.new" "$metadata"
    if bash "$fixture/scripts/package-release.sh" aarch64-apple-darwin "$version" >"$fixture/mismatch.log" 2>&1; then
        echo 'expected channel mismatch to fail before building' >&2; exit 1
    fi
    grep -q 'channel do not match' "$fixture/mismatch.log"
done
echo 'offline channel archive and bundle fixtures passed'
