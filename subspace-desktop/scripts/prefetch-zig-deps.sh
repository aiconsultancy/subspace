#!/usr/bin/env bash
# Populate an offline Zig package store for a Ghostty checkout.
#
# Usage: prefetch-zig-deps.sh GHOSTTY_SOURCE_DIR OUT_DIR STORE_DIR
#
# Downloads go to OUT_DIR/tarballs, Zig's cache is OUT_DIR/cache, and the
# unpacked packages, for `zig build --system`, go to STORE_DIR.
#
# Other runs may fill the same OUT_DIR at the same time: every download and
# package is staged under this process's pid and published without replacing
# another run's (zig-deps-stage.sh).
set -euo pipefail

# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/zig-deps-stage.sh
source "$(cd "$(dirname "$0")" && pwd)/zig-deps-stage.sh"

usage='usage: prefetch-zig-deps.sh GHOSTTY_SOURCE_DIR OUT_DIR STORE_DIR'
source_dir="${1:?$usage}"
out_dir="${2:?$usage}"
store="${3:?$usage}"
if [[ ! -f "$source_dir/build.zig" ]]; then
    echo "Ghostty source directory has no build.zig: $source_dir" >&2
    exit 1
fi
mkdir -p "$out_dir/tarballs" "$out_dir/cache" "$store"
staged=''
trap '[[ -z "$staged" ]] || rm -rf "$staged"' EXIT

# `zig fetch` hashes local files without using Zig's HTTP client.
fetch() {
    (cd "$source_dir" && zig fetch --global-cache-dir "$out_dir/cache" "$1")
}

# Downloads that are not archives at all, such as an error page a proxy
# served with HTTP 200: removed, so the next fill downloads them again, and
# not downloaded again in this run. `tar -tf` reads every compression the
# manifests use (gzip, xz, zstd) without Zig, so a Zig that cannot start or
# write its cache never costs a good download.
rejected=$'\n'
for round in 1 2 3 4 5; do
    before="$(find "$store" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
    urls="$(find "$source_dir" "$store" -name build.zig.zon \
        -not -path '*/zig-cache/*' -not -path '*/.zig-cache/*' -exec grep -hEo \
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
        if [[ ! -f "$file" && "$rejected" != *$'\n'"$file"$'\n'* ]]; then
            staged="$(stage "$out_dir/tarballs")"
            if curl --fail --silent --show-error --location --connect-timeout 10 \
                --max-time 45 --retry 2 --retry-delay 2 --retry-max-time 100 \
                --retry-all-errors -o "$staged" "$url"; then
                publish "$staged" "$file"
            else
                echo "skipped unavailable $url" >&2
                rm -f "$staged"
            fi
            staged=''
        fi
    done <<EOF
$urls
EOF

    for file in "$out_dir"/tarballs/*; do
        [[ -f "$file" ]] || continue
        fetch "$file" >/dev/null && continue
        if tar -tf "$file" >/dev/null 2>&1; then
            echo "skipped unreadable $file" >&2
        else
            rm -f "$file"
            rejected+="$file"$'\n'
            echo "removed $file, which is not an archive; the next fill downloads it again" >&2
        fi
    done

    # `zig build --system` expects extracted directories named by package hash.
    # Each is unpacked beside the store and published whole, so an
    # interrupted extraction is never taken as a package.
    for archive in "$out_dir"/cache/p/*.tar.gz; do
        [[ -f "$archive" ]] || continue
        name="$(basename "$archive" .tar.gz)"
        if [[ ! -d "$store/$name" ]]; then
            # Beside the store, so the rename stays on one file system.
            staged="$(stage -d "$(dirname "$store")")"
            tar -xf "$archive" -C "$staged" --strip-components=1
            publish "$staged" "$store/$name"
            staged=''
        fi
    done

    after="$(find "$store" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ')"
    echo "round $round: $after packages"
    [[ "$before" = "$after" ]] && break
done

printf 'export GHOSTTY_SOURCE_DIR=%q\n' "$source_dir"
printf 'export GHOSTTY_ZIG_SYSTEM_DIR=%q\n' "$store"
