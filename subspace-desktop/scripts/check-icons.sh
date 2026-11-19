#!/usr/bin/env bash
# Check generated native icon signatures, sizes, and alpha channels.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"

command -v magick >/dev/null || { echo "ImageMagick (magick) is required" >&2; exit 1; }
command -v file >/dev/null || { echo "file is required" >&2; exit 1; }

assets="$root/products/subspace/assets"
for stem in subspace subspace-nightly; do
    grep -q 'Mac OS X icon' < <(file "$assets/$stem.icns")
    grep -q 'MS Windows icon resource' < <(file "$assets/$stem.ico")

    expected='16 24 32 48 64 128 256'
    actual="$(magick identify -format '%w ' "$assets/$stem.ico" | sed 's/ $//')"
    [ "$actual" = "$expected" ] || {
        echo "unexpected $stem ICO widths: $actual" >&2
        exit 1
    }

    channels="$(magick identify -format '%[channels] ' "$assets/$stem.ico" | sed 's/ $//')"
    case "$channels" in
        *a*) ;;
        *) echo "$stem ICO images have no alpha channel: $channels" >&2; exit 1 ;;
    esac
done

echo "icon assets have expected signatures, sizes, and alpha"
