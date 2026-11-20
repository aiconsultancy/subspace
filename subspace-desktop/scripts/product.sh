#!/usr/bin/env bash
# Shared product metadata and bundle assembly. Source from a build/run script.

# Oldest macOS release that Subspace supports. Builds export it as
# MACOSX_DEPLOYMENT_TARGET and the app bundle declares it.
MACOS_MINIMUM_VERSION=15.0
# The native archive targets also form the licence policy check's graph.
RELEASE_TARGETS=(
    aarch64-apple-darwin
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
    x86_64-pc-windows-msvc
)
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "${BASH_SOURCE[0]}")/prerelease-common.sh"

resolve_target_dir() {
    local root="${root:?}" path="$1"
    case "$path" in
        [A-Za-z]:/* | [A-Za-z]:\\* | \\\\*)
            if ! command -v cygpath >/dev/null 2>&1; then
                echo "cygpath is required for Windows target directory: $path" >&2
                return 1
            fi
            cygpath -u "$path"
            ;;
        /*) printf '%s\n' "$path" ;;
        *) printf '%s\n' "$root/$path" ;;
    esac
}

load_product() {
    local root="${root:?}"
    product=subspace
    metadata="$(cargo metadata --format-version=1 --no-deps --locked)"
    version="$(jq -er --arg name "$product" '.packages[] | select(.name == $name) | .version' <<<"$metadata")"
    product_metadata="$(cargo run --quiet --locked -p subspace-descriptor --bin subspace-product-metadata -- "$product" "$version")"
    binary_name="$(jq -er '.binary_name' <<<"$product_metadata")"
    # Public output consumed by scripts that source this library.
    # shellcheck disable=SC2034
    display_name="$(jq -er '.display_name' <<<"$product_metadata")"
    # Public output consumed by scripts that source this library.
    # shellcheck disable=SC2034
    bundle_id="$(jq -er '.bundle_id' <<<"$product_metadata")"
    # shellcheck disable=SC2034
    asset_stem="$(jq -er '.asset_stem' <<<"$product_metadata")"
    channel="$(jq -er '.channel' <<<"$product_metadata")"
    if [[ "$channel" = stable ]]; then
        if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
            validate_integration_version "$version" || {
                echo 'stable metadata requires a stable or integration version; version and metadata channel do not match' >&2; return 2;
            }
        fi
    elif [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)-(nightly|alpha|beta)\.[1-9][0-9]{11}\.g[0-9a-f]{7}$ ]] || [[ "${BASH_REMATCH[4]}" != "$channel" ]]; then
        echo 'pre-release version and metadata channel do not match' >&2; return 2
    fi
    [[ "$asset_stem" =~ ^[A-Za-z0-9-]+$ ]] || { echo 'unsafe asset stem' >&2; return 2; }
    test "$(jq -er '.version' <<<"$product_metadata")" = "$version"
    # The Framework has its own version; it need not equal the product's.
    jq -er '.subspace_version' <<<"$product_metadata" >/dev/null
    icon_relative="$(jq -er '.icon_paths[] | select(endswith(".icns"))' <<<"$product_metadata")"
    icon="$root/products/$product/$icon_relative"
    # Public output consumed by scripts that source this library.
    # shellcheck disable=SC2034
    target_dir="$(resolve_target_dir "${CARGO_TARGET_DIR:-$(jq -er '.target_directory' <<<"$metadata")}")"
}

load_release_toolchain() {
    local expected compiler_version sysroot notice
    # The Rust tool entry in mise.toml is an inline table with a version pin.
    expected="$(sed -nE 's/^rust[[:space:]]*=[[:space:]]*\{[[:space:]]*version[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/p' "$root/mise.toml")"
    if [[ ! "$expected" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
        echo "could not read the pinned Rust version from $root/mise.toml" >&2
        return 1
    fi
    # Use the checked compiler for Cargo's metadata helper and release build.
    export RUSTC="${RUSTC:-${CARGO_BUILD_RUSTC:-rustc}}"
    compiler_version="$("$RUSTC" --version)" || return
    if [[ "$compiler_version" != "rustc $expected "* ]]; then
        echo "packaging requires Rust $expected from mise.toml; $RUSTC reports $compiler_version" >&2
        return 1
    fi
    sysroot="$("$RUSTC" --print sysroot)" || return
    sysroot="$(resolve_target_dir "$sysroot")" || return
    rust_std_notices=(
        "$sysroot/share/doc/rust/COPYRIGHT-library.html"
        "$sysroot/share/doc/rust/licenses/Apache-2.0.txt"
        "$sysroot/share/doc/rust/licenses/MIT.txt"
    )
    for notice in "${rust_std_notices[@]}"; do
        if [[ ! -f "$notice" || ! -r "$notice" ]]; then
            echo "missing or unreadable Rust standard library notice: $notice" >&2
            return 1
        fi
    done
}

stage_product_licences() {
    local destination="$1" target="$2" notice
    local octicons="$destination/licenses/icons/octicons"
    mkdir -p "$octicons"
    for notice in "$root/licenses/"*; do
        if [[ "${notice##*/}" = webview2 && "$target" != *-windows-* ]]; then continue; fi
        cp -R "$notice" "$destination/licenses/"
    done
    cp "$root/spaces/actions/assets/"{README.md,OCTICONS-LICENSE.txt} "$octicons/"
    cp "$root/../LICENSE" "$root/NOTICE" "$destination/"
}

release_cargo_args() {
    release_args=(--target "$1")
    if [[ -n "${SUBSPACE_BUILD_FEATURES:-}" ]]; then
        release_args+=(--features "$SUBSPACE_BUILD_FEATURES")
    fi
}

release_target_supported() {
    local supported_target
    for supported_target in "${RELEASE_TARGETS[@]}"; do
        if [[ "$supported_target" = "$1" ]]; then return 0; fi
    done
    return 1
}

check_rust_licences() (
    local scratch
    scratch="$(mktemp -d)" || return
    trap 'rm -rf "$scratch"' EXIT
    # The debug diagnostics identify each crate that fell back to SPDX text.
    # JSON can group a fallback and an identical source-backed file together.
    # Run cargo-about directly: a `cargo` wrapper that resolves tools per
    # folder (mise's command wrappers do) can hide the `cargo about` subcommand.
    cargo-about about -L debug generate --locked --fail --manifest-path products/subspace/Cargo.toml \
        --format json "$@" --output-file "$scratch/licences.json" 2>"$scratch/about.log" || {
        grep -v DEBUG "$scratch/about.log" >&2
        return 1
    }
    grep -E 'WARN|ERROR' "$scratch/about.log" >&2 || true
    python3 scripts/resolve-licence-notices.py "$scratch/licences.json" "$scratch/about.log"
)

generate_rust_licences() (
    local output="$1" licence_target="$2" scratch
    local release_args
    unset SUBSPACE_GITHUB_OAUTH_CLIENT_SECRET
    release_cargo_args "$licence_target"
    scratch="$(mktemp -d)" || return
    trap 'rm -rf "$scratch"' EXIT
    cargo tree --locked --color never -p "$product" --edges normal,no-proc-macro \
        --prefix none --format '{p}' "${release_args[@]}" >"$scratch/linked" || return
    check_rust_licences "${release_args[@]}" --config about.toml >"$scratch/licences.json" || return
    jq -er --arg target "$licence_target" --rawfile tree "$scratch/linked" \
        -f about.jq "$scratch/licences.json" >"$output"
)

create_bundle() {
    local root="${root:?}"
    local binary="$1" app="$2" identifier="$3" label="$4"
    local escaped_name escaped_label escaped_id icon_name
    escaped_name="$(jq -r '.display_name | @html' <<<"$product_metadata")"
    escaped_label="$(jq -nr --arg name "$label" '$name | @html')"
    escaped_id="$(jq -nr --arg id "$identifier" '$id | @html')"
    icon_name="$(basename "$icon" .icns)"
    test -x "$binary" || { echo "missing product executable: $binary" >&2; return 1; }
    test -f "$icon" || { echo "missing product icon: $icon" >&2; return 1; }
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
    cp "$binary" "$app/Contents/MacOS/$binary_name"
    chmod 755 "$app/Contents/MacOS/$binary_name"
    cp "$icon" "$app/Contents/Resources/$icon_name.icns"
    printf '%s\n' "$product_metadata" >"$app/Contents/Resources/product.json"
    stage_product_licences "$app/Contents/Resources" aarch64-apple-darwin
    cat >"$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>$binary_name</string>
<key>CFBundleIdentifier</key><string>$escaped_id</string>
<key>CFBundleName</key><string>$escaped_name</string>
<key>CFBundleDisplayName</key><string>$escaped_label</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>$icon_name</string>
<key>CFBundleShortVersionString</key><string>${version%%-*}</string>
<key>CFBundleVersion</key><string>${version%%-*}</string>
<key>LSMinimumSystemVersion</key><string>$MACOS_MINIMUM_VERSION</string>
<key>NSHighResolutionCapable</key><true/>
<key>LSEnvironment</key><dict><key>PATH</key><string>/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string></dict>
</dict></plist>
EOF
    plutil -lint "$app/Contents/Info.plist" >/dev/null
}
