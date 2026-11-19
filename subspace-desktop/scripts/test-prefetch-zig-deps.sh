#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/zig prefetch.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/source dir" "$tmp/out dir" "$tmp/package/root"
touch "$tmp/source dir/build.zig"
cat >"$tmp/source dir/build.zig.zon" <<'EOF'
.good = "https://example.test/good.tar.gz",
.missing = "https://example.test/missing.tar.gz",
EOF
tar -czf "$tmp/good.tar.gz" -C "$tmp/package" root

cat >"$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$TEST_DIR/curl.log"
[[ "$*" == *'--connect-timeout 10 --max-time 45 --retry 2 --retry-delay 2 --retry-max-time 100 --retry-all-errors'* ]]
while [[ "$1" != '-o' ]]; do shift; done
output="$2"
url="$3"
if [[ "$url" == */missing.tar.gz ]]; then
    printf 'partial' > "$output"
    exit 22
fi
cp "$TEST_DIR/good.tar.gz" "$output"
EOF
cat >"$tmp/bin/zig" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == fetch && "$2" == --global-cache-dir ]]
cache="$3"
archive="$4"
[[ "$archive" != http* && -f "$archive" ]]
printf '%s\n' "$archive" >> "$TEST_DIR/zig.log"
mkdir -p "$cache/p"
cp "$archive" "$cache/p/good-hash.tar.gz"
EOF
chmod +x "$tmp/bin/curl" "$tmp/bin/zig"
export PATH="$tmp/bin:$PATH" TEST_DIR="$tmp"

bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" >"$tmp/stdout" 2>"$tmp/stderr"
[[ -f "$tmp/out dir/tarballs/good.tar.gz" ]]
[[ -d "$tmp/out dir/system/good-hash" ]]
[[ ! -e "$tmp/out dir/tarballs/missing.tar.gz" ]]
[[ -z "$(find "$tmp/out dir/tarballs" -name '.download.*' -print)" ]]
[[ "$(grep -c 'missing.tar.gz' "$tmp/curl.log")" == 2 ]]
grep -q 'skipped unavailable https://example.test/missing.tar.gz' "$tmp/stderr"
[[ "$(wc -l < "$tmp/zig.log" | tr -d ' ')" == 2 ]]

# A warm cache avoids network access when all referenced archives are present.
sed -i.bak '/missing/d' "$tmp/source dir/build.zig.zon"
: > "$tmp/curl.log"
bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" >"$tmp/stdout" 2>"$tmp/stderr"
[[ ! -s "$tmp/curl.log" ]]
echo 'prefetch-zig-deps fixture passed'
