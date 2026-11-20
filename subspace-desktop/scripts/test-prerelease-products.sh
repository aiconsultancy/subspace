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
    # The repository index and the distributable NOTICE have different paths.
    if cmp -s "$root/../NOTICE" "$destination/NOTICE"; then
        echo 'native archive contains the source-tree NOTICE' >&2
        return 1
    fi
    grep -Fq "Subspace binary for $target" "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq -- '- fixture-linked v1.0.0' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq 'Fixture linked licence' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq 'Fixture Git fork licence' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq 'Fixture Framework project licence' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq -- '- fixture-generic v1.0.0 (MIT)' "$destination/THIRD_PARTY_LICENSES"
    grep -Fxq -- '- fixture-mpl v3.1.4 <https://static.crates.io/crates/fixture-mpl/fixture-mpl-3.1.4.crate>' "$destination/THIRD_PARTY_LICENSES"
    if grep -Eq -- 'fixture-linked v1.0.0 \(MIT\)|subspace-update v1.0.0 \(Apache-2.0\)' "$destination/THIRD_PARTY_LICENSES"; then
        echo 'a source-backed notice was reported as a generic text' >&2; exit 1
    fi
    if grep -Eq 'fixture-macro|Fixture build-only licence' "$destination/THIRD_PARTY_LICENSES"; then
        echo 'the runtime report includes a compile-time proc macro' >&2; exit 1
    fi
    for file in canvas/mermaid-rs-renderer-MIT.txt \
        charts/LICENSE-FLINT-CHART charts/LICENSE-QUICKJS-NG charts/LICENSE-RQUICKJS \
        icons/LICENSE-FEATHER icons/LICENSE-LOBE-ICONS icons/LICENSE-LUCIDE \
        markdown/LICENSE onnxruntime/LICENSE onnxruntime/ThirdPartyNotices.txt \
        rust-std/README.md \
        terminal/LICENSE-APACHE terminal/LICENSE-GHOSTTY terminal/LICENSE-MIT terminal/LICENSE-PORTABLE-PTY \
        themes/gpui-kit/LICENSE-APACHE themes/gpui-kit/PROVENANCE.md; do
        cmp "$root/licenses/$file" "$destination/licenses/$file"
    done
    for file in COPYRIGHT-library.html licenses/Apache-2.0.txt licenses/MIT.txt; do
        cmp "$fixture/fake sysroot/share/doc/rust/$file" "$destination/licenses/rust-std/${file##*/}"
    done
    if [[ "$target" = *-windows-* ]]; then
        for file in LICENSE.txt NOTICE.txt README.md; do
            cmp "$root/licenses/webview2/$file" "$destination/licenses/webview2/$file"
        done
    else
        test ! -e "$destination/licenses/webview2"
    fi
    for file in README.md OCTICONS-LICENSE.txt; do
        cmp "$root/spaces/actions/assets/$file" "$destination/licenses/icons/octicons/$file"
    done
}

expect_packaging_failure() {
    local diagnostic="$1" target="$2" version="$3" log="$fixture/packaging-failure.log"
    if bash "$fixture/scripts/package-release.sh" "$target" "$version" >"$log" 2>&1; then
        echo "expected packaging failure matching $diagnostic" >&2
        return 1
    fi
    grep -F -- "$diagnostic" "$log" || { cat "$log" >&2; return 1; }
}

check_packaging_failures() {
    local version="$1" archive="$2" backup="$fixture/prior-archive"
    local linux=x86_64-unknown-linux-gnu windows=x86_64-pc-windows-msvc notice
    cp "$archive" "$backup"
    cp "$archive.sha256" "$backup.sha256"
    local builds_before
    builds_before="$(grep -c '^build ' "$fixture/cargo.log")"

    for notice in "$fixture/../LICENSE" "$fixture/NOTICE"; do
        mv "$notice" "$notice.saved"
        expect_packaging_failure "missing or unreadable product licence input: $notice" "$linux" "$version"
        mv "$notice.saved" "$notice"
    done
    FAKE_PARTIAL_FALLBACK=1 expect_packaging_failure \
        'missing cargo-about fallback diagnostics for fixture-generic v1.0.0 (MIT,' "$linux" "$version"
    # The SPDX text below is the MIT fallback from spdx 0.13.6. The first
    # crate has those exact bytes on disk and cargo-about retains its path.
    cat >"$fixture/fixture-canonical-mit.txt" <<'EOF'
MIT License

Copyright (c) <year> <copyright holders>

Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
associated documentation files (the "Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the
following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial
portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT
LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO
EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
USE OR OTHER DEALINGS IN THE SOFTWARE.
EOF
    cp "$fixture/fixture-canonical-mit.txt" "$fixture/native/ort/LICENSE"
    FAKE_SOURCE_BACKED_FIRST=1 FAKE_FALLBACK_LOG_CHANGED=1 expect_packaging_failure \
        'missing cargo-about fallback diagnostics for fixture-generic v1.0.0 (MIT,' "$linux" "$version"
    FAKE_MISSING_FORK=1 expect_packaging_failure \
        'missing licence records for fixture-linked v1.0.0 (https://example.com/fixture/fork#12345678)' "$linux" "$version"
    FAKE_NATIVE_VERSION=9.9.9 expect_packaging_failure 'onnxruntime 1.31.0 notices require ort-sys' "$linux" "$version"
    FAKE_NATIVE_VERSION=9.9.9 expect_packaging_failure 'webview2 1.0.3650.58 notices require webview2-com-sys' "$windows" "$version"
    ORT_LIB_PATH=/fixture/runtime expect_packaging_failure 'ONNX Runtime release notices require the pinned download' "$linux" "$version"
    ORT_CACHE_DIR=/fixture/cache CXXSTDLIB=fixture expect_packaging_failure 'CXXSTDLIB, ORT_CACHE_DIR' "$linux" "$version"
    if [[ "$(/usr/bin/uname -s)" = Linux ]]; then
        XDG_CACHE_HOME=/fixture/cache expect_packaging_failure 'linkage overrides: XDG_CACHE_HOME' "$linux" "$version"
    fi
    mkdir -p "$fixture/.cargo" "$fixture/cargo-home"
    printf '[env]\nORT_LIB_LOCATION = { value = "fixture-runtime", relative = true, force = true }\n' >"$fixture/.cargo/config.toml"
    printf '[env]\nORT_PREFER_DYNAMIC_LINK = "1"\n' >"$fixture/cargo-home/config.toml"
    CARGO_HOME="$fixture/cargo-home" expect_packaging_failure 'ORT_LIB_LOCATION, ORT_PREFER_DYNAMIC_LINK' "$linux" "$version"
    rm "$fixture/.cargo/config.toml"
    FIXTURE_CARGO_CONFIG='env.ORT_CUDA_VERSION="12"' expect_packaging_failure 'linkage overrides: ORT_CUDA_VERSION' "$linux" "$version"
    FIXTURE_CARGO_CONFIG="target.$linux.onnxruntime.rustc-link-lib=['onnxruntime']" \
        expect_packaging_failure "linkage overrides: target.$linux.onnxruntime" "$linux" "$version"
    local ort="$fixture/native/ort/build/download/dist.tsv"
    local loader="$fixture/native/webview2/x64/WebView2LoaderStatic.lib"
    cp "$ort" "$ort.saved"
    cp "$loader" "$loader.saved"
    printf 'distribution drift\n' >"$ort"
    printf 'loader drift\n' >"$loader"
    expect_packaging_failure 'onnxruntime 1.31.0 artefact mismatch: licence checksum mismatch' "$linux" "$version"
    expect_packaging_failure 'webview2 1.0.3650.58 artefact mismatch: licence checksum mismatch' "$windows" "$version"
    mv "$ort.saved" "$ort"
    mv "$loader.saved" "$loader"
    test "$(grep -c '^build ' "$fixture/cargo.log")" = "$builds_before"
    cmp "$archive" "$backup"
    cmp "$archive.sha256" "$backup.sha256"
    # Recognised diagnostics and the source-backed first crate can coexist.
    FAKE_SOURCE_BACKED_FIRST=1 bash "$fixture/scripts/package-release.sh" "$linux" "$version"
}

assert_lock_held() {
    python3 - "$1" <<'PY'
import fcntl
import sys

with open(sys.argv[1], "rb") as lock:
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        pass
    else:
        sys.exit("expected another process to hold the lock")
PY
}

check_concurrent_archives() {
    local version="$1" binary="$2" gate="$fixture/concurrent" target=aarch64-apple-darwin
    local first waited terminal_cache default_cache archive="$fixture/dist/$binary-$version-$target.tar.gz"
    mkdir -p "$gate/with-terminal" "$gate/without-terminal"
    : >"$fixture/release-cache.log"
    FIXTURE_BUILD_GATE="$gate" SUBSPACE_BUILD_FEATURES=terminal \
        bash "$fixture/scripts/package-release.sh" "$target" "$version" >"$gate/with.log" 2>&1 &
    first=$!
    for ((waited = 0; waited < 300; waited++)); do
        [[ ! -f "$gate/built" ]] || break
        kill -0 "$first" 2>/dev/null || break
        sleep 0.1
    done
    terminal_cache="$(sed -n '1p' "$fixture/release-cache.log")"
    if [[ -f "$gate/built" ]] &&
       assert_lock_held "$terminal_cache/.package.lock" &&
       SUBSPACE_BUILD_FEATURES='' bash "$fixture/scripts/package-release.sh" "$target" "$version" &&
       tar -xzf "$archive" -C "$gate/without-terminal"; then
        touch "$gate/resume"
        wait "$first" || { cat "$gate/with.log" >&2; return 1; }
    else
        touch "$gate/resume"
        wait "$first" || true
        cat "$gate/with.log" >&2
        return 1
    fi
    tar -xzf "$archive" -C "$gate/with-terminal"
    grep -Fxq '# features: terminal' "$gate/with-terminal/$binary"
    grep -Fxq -- '- fixture-terminal v1.0.0' "$gate/with-terminal/THIRD_PARTY_LICENSES"
    grep -Fxq '# features: ' "$gate/without-terminal/$binary"
    if grep -Fq 'fixture-terminal' "$gate/without-terminal/THIRD_PARTY_LICENSES"; then
        echo 'archive without Terminal contains Terminal dependency licences' >&2
        return 1
    fi
    default_cache="$(sed -n '2p' "$fixture/release-cache.log")"
    test "$terminal_cache" != "$default_cache"
    test -x "$terminal_cache/$target/release/from-cargo/$binary"
    SUBSPACE_BUILD_FEATURES='terminal,terminal' bash "$fixture/scripts/package-release.sh" "$target" "$version"
    test "$(tail -n 1 "$fixture/release-cache.log")" = "$terminal_cache"
}

check_concurrent_publication() {
    local version="$1" binary="$2" target=x86_64-unknown-linux-gnu gate="$fixture/publish-gate"
    local archive="$binary-$version-$target.tar.gz" first second waited
    mkdir -p "$gate/second"
    cat >"$fixture/bin/tar" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" = -czf && -n "${PUBLISH_GATE:-}" ]]; then
    touch "$PUBLISH_GATE/tar-started"
    for ((waited = 0; waited < 300; waited++)); do
        if [[ -f "$PUBLISH_GATE/resume" ]]; then exec /usr/bin/tar "$@"; fi
        sleep 0.1
    done
    exit 28
fi
exec /usr/bin/tar "$@"
EOF
    chmod 755 "$fixture/bin/tar"
    PUBLISH_GATE="$gate" SUBSPACE_BUILD_FEATURES=terminal \
        bash "$fixture/scripts/package-release.sh" "$target" "$version" >"$gate/first.log" 2>&1 &
    first=$!
    for ((waited = 0; waited < 300; waited++)); do
        [[ ! -f "$gate/tar-started" ]] || break
        kill -0 "$first" 2>/dev/null || break
        sleep 0.1
    done
    # The archive writer must hold the same lock throughout compression and
    # publication. Confirm that an independent process can't acquire it.
    if [[ ! -f "$gate/tar-started" ]] || ! assert_lock_held "$fixture/dist/.locks/$archive.lock"; then
        touch "$gate/resume"
        wait "$first" || cat "$gate/first.log" >&2
        return 1
    fi
    (cd "$fixture/dist"; shasum -a 256 -c "$archive.sha256")
    FIXTURE_BUILD_GATE="$gate/second" SUBSPACE_BUILD_FEATURES='' \
        bash "$fixture/scripts/package-release.sh" "$target" "$version" >"$gate/second.log" 2>&1 &
    second=$!
    for ((waited = 0; waited < 300; waited++)); do
        [[ ! -f "$gate/second/built" ]] || break
        kill -0 "$second" 2>/dev/null || break
        sleep 0.1
    done
    touch "$gate/second/resume" "$gate/resume"
    test -f "$gate/second/built"
    wait "$first" || { cat "$gate/first.log" >&2; return 1; }
    wait "$second" || { cat "$gate/second.log" >&2; return 1; }
    (cd "$fixture/dist"; shasum -a 256 -c "$archive.sha256")
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
        check_packaging_failures "$version" "$fixture/dist/$binary-$version-x86_64-unknown-linux-gnu.tar.gz"
        check_concurrent_archives "$version" "$binary"
        check_concurrent_publication "$version" "$binary"
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
