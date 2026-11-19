#!/usr/bin/env bash
# Generate native application icons; no arguments generate the stable Subspace icon.
# Requires rsvg-convert, iconutil (macOS), and ImageMagick (Windows ICO).
# Optional arguments: main SVG, 16/24 px SVG, and output stem. Outputs are beside the main SVG.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
source="${1:-$root/products/subspace/assets/subspace-app.svg}"
small_source="${2:-$source}"
assets="$(dirname "$source")"
if (( $# == 0 )); then
    name=subspace
else
    name="${3:-$(basename "$source" .svg)}"
fi
if [[ -n "${3:-}" ]]; then
    case "$name" in
        .|..|*[!a-zA-Z0-9._-]*) echo "Invalid output stem: $name" >&2; exit 1 ;;
    esac
fi
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

command -v rsvg-convert >/dev/null || { echo "rsvg-convert is required" >&2; exit 1; }
command -v magick >/dev/null || { echo "ImageMagick (magick) is required" >&2; exit 1; }

render() {
    local size="$1" output="$2"
    local image="$source"
    if [[ "$size" == 16 || "$size" == 24 ]]; then image="$small_source"; fi
    rsvg-convert --width "$size" --height "$size" --output "$output" "$image"
}

# Windows accepts a multi-image ICO. Keep alpha from the SVG and include the
# standard shell sizes, including 256px for high-DPI displays.
windows_sizes=(16 24 32 48 64 128 256)
windows_pngs=()
for size in "${windows_sizes[@]}"; do
    output="$work/windows-$size.png"
    render "$size" "$output"
    windows_pngs+=("$output")
done
magick "${windows_pngs[@]}" "$assets/$name.ico"

# iconutil requires this exact iconset naming scheme. @2x files provide the
# 2x representation for each logical macOS size without a white backing.
if command -v iconutil >/dev/null 2>&1; then
    iconset="$work/$name.iconset"
    mkdir "$iconset"
    for size in 16 32 128 256 512; do
        render "$size" "$iconset/icon_${size}x${size}.png"
        render "$((size * 2))" "$iconset/icon_${size}x${size}@2x.png"
    done
    iconutil --convert icns --output "$assets/$name.icns" "$iconset"
else
    echo "iconutil is required to generate $name.icns on macOS" >&2
    exit 1
fi

printf 'Generated %s and %s\n' "$assets/$name.icns" "$assets/$name.ico"
