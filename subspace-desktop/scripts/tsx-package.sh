#!/usr/bin/env bash
# Build, sign and verify a TSX Space package with a local development key,
# and put it in a zip archive with its NOTICE and licences.
#
#   scripts/tsx-package.sh [PACKAGE_DIR [OUT.nshpkg]]
#
# PACKAGE_DIR defaults to tsx-spaces/system-monitor. The first run creates a
# development root key and a release key under .local/tsx-keys/ (ignored by
# Git) and certifies the release key for `dev.subspace.*` app IDs for 90
# days; delete .local/tsx-keys/release.cert to certify it again. Never
# commit those keys: they are for local trust only. A host that trusts
# .local/tsx-keys/root.pub accepts the package.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

package_dir="${1:-tsx-spaces/system-monitor}"
keys="${SUBSPACE_TSX_KEYS_DIR:-$root/.local/tsx-keys}"
nsh="${NSH:-nsh}"

if ! command -v "$nsh" >/dev/null 2>&1; then
    echo "nsh not found: run this as 'mise run tsx-package' or install nsh" >&2
    exit 1
fi
if [[ ! -f "$package_dir/nsh.json" ]]; then
    echo "$package_dir has no nsh.json" >&2
    exit 1
fi
package_dir="$(cd "$package_dir" && pwd)"
app_id="$(jq -er '.id' "$package_dir/nsh.json")"
build="$(jq -er '.build' "$package_dir/nsh.json")"
out="${2:-$root/dist/tsx-spaces/$app_id-$build.nshpkg}"
mkdir -p "$(dirname "$out")"
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"

mkdir -p "$keys"
chmod 700 "$keys"
for key in root release; do
    if [[ ! -f "$keys/$key.secret" ]]; then
        "$nsh" keygen "$keys/$key"
    fi
done
cert="$keys/release.cert"
# Certify again when the certificate is missing or older than the keys.
if [[ ! -f "$cert" || "$keys/release.pub" -nt "$cert" || "$keys/root.secret" -nt "$cert" ]]; then
    "$nsh" certify --root "$keys/root.secret" --key "$keys/release.pub" \
        --apps 'dev.subspace.*' --days 90 -o "$cert"
fi

(
    cd "$package_dir"
    "$nsh" publish App.tsx --key "$keys/release.secret" --cert "$cert" -o "$out"
)
"$nsh" verify "$out" --root "$keys/root.pub"

# The distributable archive: the package with the package's own NOTICE and
# the files under its licenses/, if it has them, as the package itself
# carries only code. Each package names its own; none is assumed.
archive="${out%.nshpkg}.zip"
rm -f "$archive"
(
    cd "$(dirname "$out")"
    zip -q -j "$archive" "$(basename "$out")"
)
notices=()
while IFS= read -r file; do
    notices+=("$file")
done < <(cd "$package_dir" && { [[ ! -f NOTICE ]] || echo NOTICE; } &&
    { [[ ! -d licenses ]] || find licenses -type f | LC_ALL=C sort; })
if ((${#notices[@]} > 0)); then
    (cd "$package_dir" && zip -q "$archive" "${notices[@]}")
fi
listing="$(unzip -Z1 "$archive")"
for required in "$(basename "$out")" ${notices[@]+"${notices[@]}"}; do
    if ! grep -qxF "$required" <<<"$listing"; then
        echo "$archive lacks $required" >&2
        exit 1
    fi
done
echo "$archive: the package with ${#notices[@]} notice and licence files"
