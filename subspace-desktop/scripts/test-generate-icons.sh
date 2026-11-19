#!/usr/bin/env bash
# Verify icon source selection without platform-specific image tools.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/subspace-icons-fixture.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT
mkdir -p "$fixture/bin" "$fixture/assets with spaces"
touch "$fixture/assets with spaces/main.svg" "$fixture/assets with spaces/small.svg"

cat >"$fixture/bin/rsvg-convert" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == --width && "$3" == --height && "$2" == "$4" && "$5" == --output ]]
printf '%s %s %s\n' "$2" "$(basename "$7")" "$(basename "$6")" >>"$ICON_RENDER_LOG"
touch "$6"
SH
cat >"$fixture/bin/magick" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
while (( $# > 1 )); do [[ -f "$1" ]]; shift; done
touch "$1"
SH
cat >"$fixture/bin/iconutil" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == --convert && "$2" == icns && "$3" == --output && -d "$5" ]]
[[ -f "$5/icon_16x16.png" && -f "$5/icon_512x512@2x.png" ]]
touch "$4"
SH
chmod +x "$fixture/bin/rsvg-convert" "$fixture/bin/magick" "$fixture/bin/iconutil"
export PATH="$fixture/bin:$PATH" ICON_RENDER_LOG="$fixture/render.log"

for mode in default small stem; do
    : >"$ICON_RENDER_LOG"
    args=("$fixture/assets with spaces/main.svg")
    if [[ "$mode" == small || "$mode" == stem ]]; then args+=("$fixture/assets with spaces/small.svg"); fi
    name=main
    if [[ "$mode" == stem ]]; then args+=("native-name"); name=native-name; fi
    bash "$root/scripts/generate-icons.sh" "${args[@]}" >/dev/null
    [[ -f "$fixture/assets with spaces/$name.ico" && -f "$fixture/assets with spaces/$name.icns" ]]
    rm "$fixture/assets with spaces/$name.ico" "$fixture/assets with spaces/$name.icns"

    small=main.svg
    if [[ "$mode" == small || "$mode" == stem ]]; then small=small.svg; fi
    cat >"$fixture/expected" <<EOF
16 $small windows-16.png
24 $small windows-24.png
32 main.svg windows-32.png
48 main.svg windows-48.png
64 main.svg windows-64.png
128 main.svg windows-128.png
256 main.svg windows-256.png
16 $small icon_16x16.png
32 main.svg icon_16x16@2x.png
32 main.svg icon_32x32.png
64 main.svg icon_32x32@2x.png
128 main.svg icon_128x128.png
256 main.svg icon_128x128@2x.png
256 main.svg icon_256x256.png
512 main.svg icon_256x256@2x.png
512 main.svg icon_512x512.png
1024 main.svg icon_512x512@2x.png
EOF
    diff -u "$fixture/expected" "$ICON_RENDER_LOG"
done
mkdir -p "$fixture/default-root/scripts" "$fixture/default-root/products/subspace/assets"
cp "$root/scripts/generate-icons.sh" "$fixture/default-root/scripts/generate-icons.sh"
touch "$fixture/default-root/products/subspace/assets/subspace-app.svg"
: >"$ICON_RENDER_LOG"
bash "$fixture/default-root/scripts/generate-icons.sh" >/dev/null
[[ -f "$fixture/default-root/products/subspace/assets/subspace.ico" && -f "$fixture/default-root/products/subspace/assets/subspace.icns" ]]
sed 's/small\.svg/subspace-app.svg/g; s/main\.svg/subspace-app.svg/g' "$fixture/expected" >"$fixture/default-expected"
diff -u "$fixture/default-expected" "$ICON_RENDER_LOG"
if bash "$root/scripts/generate-icons.sh" "$fixture/assets with spaces/main.svg" "$fixture/assets with spaces/small.svg" ../escape >/dev/null 2>&1; then
    echo 'Invalid output stem was accepted' >&2
    exit 1
fi
echo 'Icon generator fixtures passed: default and small-source routing, native output paths'
