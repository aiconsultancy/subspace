#!/usr/bin/env bash
# Usage: stamp-prerelease-version.sh VERSION
# Stamp the workspace version and the locked versions of the workspace
# packages that inherit it, without network access or other edits.
set -euo pipefail
[[ "$#" = 1 ]] || { echo 'usage: stamp-prerelease-version.sh VERSION' >&2; exit 2; }
version="$1"
# shellcheck source-path=SCRIPTDIR
# shellcheck source=prerelease-common.sh
source "$(dirname "$0")/prerelease-common.sh"
if ! validate_prerelease_version "$version"; then
    echo "invalid pre-release version: $version" >&2; exit 2
fi
cd "$(dirname "$0")/.."
metadata="$(cargo metadata --locked --format-version=1 --no-deps)"
current="$(jq -er '.packages[] | select(.name == "subspace") | .version' <<<"$metadata")"
names="$(jq -er --arg current "$current" '[.packages[] | select(.source == null and .version == $current) | .name] |
    if length > 0 then join("|") else error("no workspace packages") end' <<<"$metadata")"
stage="$(mktemp -d .prerelease-version.XXXXXX)"
trap 'rm -rf "${stage:?}"' EXIT
files=(Cargo.toml Cargo.lock)
for index in "${!files[@]}"; do
    path="${files[$index]}"
    mode=manifest
    if [[ "$path" = Cargo.lock ]]; then mode=lock; fi
    # Cargo's generated lockfile and the manifest use one-line quoted
    # names/versions. Preserve every other byte and emit LF on all platforms.
    if ! awk -v mode="$mode" -v names="$names" -v version="$version" '
        BEGIN {
            count = split(names, list, /\|/)
            for (item = 1; item <= count; item++) wanted[list[item]] = 1
        }
        /^\[/ {
            in_package = ($0 == "[[package]]")
            selected = (mode == "manifest" && $0 == "[workspace.package]")
        }
        mode == "lock" && in_package && /^name = "[^"]+"$/ {
            name = $0
            sub(/^name = "/, "", name); sub(/"$/, "", name)
            selected = (name in wanted)
        }
        selected && /^version = "[^"]+"$/ {
            sub(/"[^"]+"$/, "\"" version "\"")
            changed++; selected = 0
        }
        { print }
        END { if (!changed || (mode == "manifest" && changed != 1)) exit 1 }
    ' "$path" >"$stage/$index"; then
        echo "no matching version entries in $path" >&2; exit 1
    fi
done
# Preflight the entire edit set before replacing any source file. An identical
# repeated stamp leaves files untouched. Staging stays on the same filesystem.
for index in "${!files[@]}"; do
    if ! cmp -s "$stage/$index" "${files[$index]}"; then
        mv -f "$stage/$index" "${files[$index]}"
    fi
done
rm -rf "${stage:?}"
trap - EXIT
# The edits above are offline. Verify with the same locked metadata boundary
# as packaging; a clean runner may first need Cargo's registry metadata.
cargo metadata --locked --format-version=1 --no-deps >/dev/null
