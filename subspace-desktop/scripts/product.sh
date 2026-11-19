#!/usr/bin/env bash
# Shared product metadata and bundle assembly. Source from a build/run script.

# Oldest macOS release that Subspace supports. Builds export it as
# MACOSX_DEPLOYMENT_TARGET and the app bundle declares it.
MACOS_MINIMUM_VERSION=15.0
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
    cp -R "$root/licenses" "$app/Contents/Resources/licenses"
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
