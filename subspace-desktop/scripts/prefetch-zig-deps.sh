#!/usr/bin/env bash
# Populate an offline Zig package store for a Ghostty checkout.
#
# Usage: prefetch-zig-deps.sh GHOSTTY_SOURCE_DIR OUT_DIR
set -euo pipefail

source_dir="${1:?usage: prefetch-zig-deps.sh GHOSTTY_SOURCE_DIR OUT_DIR}"
out_dir="${2:?usage: prefetch-zig-deps.sh GHOSTTY_SOURCE_DIR OUT_DIR}"
if [[ ! -f "$source_dir/build.zig" ]]; then
    echo "Ghostty source directory has no build.zig: $source_dir" >&2
    exit 1
fi
mkdir -p "$out_dir/tarballs" "$out_dir/cache" "$out_dir/system"

# `zig fetch` hashes local files without using Zig's HTTP client.
fetch() {
    (cd "$source_dir" && zig fetch --global-cache-dir "$out_dir/cache" "$1")
}

for round in 1 2 3 4 5; do
    before="$(find "$out_dir/system" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
    urls="$(find "$source_dir" "$out_dir/system" -name build.zig.zon \
        -not -path '*/zig-cache/*' -exec grep -hEo \
        'https://[^"# ]+(#[^" ]+)?' {} + 2>/dev/null | sort -u || true)"
    while IFS= read -r url; do
        [[ -n "$url" ]] || continue
        case "$url" in
            *.tar.gz|*.tgz|*.tar.zst|*.tar.xz)
                file="$out_dir/tarballs/$(basename "$url")"
                ;;
            *\#*)
                repo="${url%%#*}"
                repo="${repo%.git}"
                commit="${url##*#}"
                file="$out_dir/tarballs/$(basename "$repo")-$commit.tar.gz"
                url="$repo/archive/$commit.tar.gz"
                ;;
            *)
                continue
                ;;
        esac
        # Manifests also mention example URLs (Ghostty's `archive/COMMIT.tar.gz`)
        # and packages for other platforms. Skip what cannot be downloaded or
        # unpacked; `zig build` names any package the build really needs.
        if [[ ! -f "$file" ]]; then
            partial="$(mktemp "$out_dir/tarballs/.download.XXXXXX")"
            if curl --fail --silent --show-error --location --connect-timeout 10 \
                --max-time 45 --retry 2 --retry-delay 2 --retry-max-time 100 \
                --retry-all-errors -o "$partial" "$url"; then
                mv -f "$partial" "$file"
            else
                echo "skipped unavailable $url" >&2
                rm -f "$partial"
            fi
        fi
    done <<EOF
$urls
EOF

    for file in "$out_dir"/tarballs/*; do
        [[ -f "$file" ]] || continue
        fetch "$file" >/dev/null || echo "skipped unreadable $file" >&2
    done

    # `zig build --system` expects extracted directories named by package hash.
    for archive in "$out_dir"/cache/p/*.tar.gz; do
        [[ -f "$archive" ]] || continue
        name="$(basename "$archive" .tar.gz)"
        if [[ ! -d "$out_dir/system/$name" ]]; then
            mkdir -p "$out_dir/system/$name"
            tar -xf "$archive" -C "$out_dir/system/$name" --strip-components=1
        fi
    done

    after="$(find "$out_dir/system" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
    echo "round $round: $after packages"
    [[ "$before" = "$after" ]] && break
done

printf 'export GHOSTTY_SOURCE_DIR=%q\n' "$source_dir"
printf 'export GHOSTTY_ZIG_SYSTEM_DIR=%q\n' "$out_dir/system"
