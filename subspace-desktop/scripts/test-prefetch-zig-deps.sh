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
.bad = "https://example.test/bad.tar.gz",
EOF
tar -czf "$tmp/good.tar.gz" -C "$tmp/package" root

cat >"$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$TEST_DIR/curl.log"
[[ "$*" == *'--connect-timeout 10 --max-time 45 --retry 2 --retry-delay 2 --retry-max-time 100 --retry-all-errors'* ]] || exit 1
while [[ "$1" != '-o' ]]; do shift; done
output="$2"
url="$3"
if [[ "$url" == */missing.tar.gz ]]; then
    printf 'partial' > "$output"
    exit 22
fi
# A proxy's error page, served with HTTP 200.
if [[ "$url" == */bad.tar.gz ]]; then
    printf '<html>blocked</html>' > "$output"
    exit 0
fi
cp "$TEST_DIR/good.tar.gz" "$output"
EOF
cat >"$tmp/bin/zig" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "$1" == fetch && "$2" == --global-cache-dir ]] || exit 1
cache="$3"
archive="$4"
[[ "$archive" != http* && -f "$archive" && "$archive" != */bad.tar.gz && -z "${ZIG_BROKEN:-}" ]] || exit 1
printf '%s\n' "$archive" >> "$TEST_DIR/zig.log"
mkdir -p "$cache/p"
cp "$archive" "$cache/p/good-hash.tar.gz"
EOF
chmod +x "$tmp/bin/curl" "$tmp/bin/zig"
export PATH="$tmp/bin:$PATH" TEST_DIR="$tmp"

bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" "$tmp/out dir/packages" >"$tmp/stdout" 2>"$tmp/stderr"
[[ -f "$tmp/out dir/tarballs/good.tar.gz" ]] || exit 1
[[ -d "$tmp/out dir/packages/good-hash" ]] || exit 1
[[ ! -e "$tmp/out dir/tarballs/missing.tar.gz" ]] || exit 1
[[ -z "$(find "$tmp/out dir/tarballs" -name '.stage-*' -print)" ]] || exit 1
[[ "$(grep -c 'missing.tar.gz' "$tmp/curl.log")" == 2 ]] || exit 1
grep -q 'skipped unavailable https://example.test/missing.tar.gz' "$tmp/stderr"
[[ "$(wc -l < "$tmp/zig.log" | tr -d ' ')" == 2 ]] || exit 1
# A tarball `zig fetch` rejects is removed, and not downloaded again in the
# same run, though it took two rounds.
[[ ! -e "$tmp/out dir/tarballs/bad.tar.gz" ]] || exit 1
[[ "$(grep -c 'bad.tar.gz' "$tmp/curl.log")" == 1 ]] || exit 1
grep -qF "removed $tmp/out dir/tarballs/bad.tar.gz, which is not an archive; the next fill downloads it again" "$tmp/stderr"

# The next fill downloads only the rejected tarball again; once every
# referenced archive is present, a fill needs no network.
sed -i.bak '/missing/d' "$tmp/source dir/build.zig.zon"
: > "$tmp/curl.log"
bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" "$tmp/out dir/packages" >"$tmp/stdout" 2>"$tmp/stderr"
[[ "$(cat "$tmp/curl.log")" == *'https://example.test/bad.tar.gz' && "$(wc -l < "$tmp/curl.log" | tr -d ' ')" == 1 ]] || exit 1
sed -i.bak '/bad/d' "$tmp/source dir/build.zig.zon"
: > "$tmp/curl.log"
bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" "$tmp/out dir/packages" >"$tmp/stdout" 2>"$tmp/stderr"
[[ ! -s "$tmp/curl.log" ]] || exit 1

# An interrupted extraction leaves no package directory, so a retry unpacks
# the package again instead of taking a partial one as complete.
rm -rf "$tmp/out dir/packages/good-hash"
cat >"$tmp/bin/tar" <<'EOF'
#!/usr/bin/env bash
while [[ "$1" != '-C' ]]; do shift; done
touch "$2/partial"
exit 1
EOF
chmod +x "$tmp/bin/tar"
if bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" "$tmp/out dir/packages" >"$tmp/stdout" 2>"$tmp/stderr"; then
    echo 'prefetch-zig-deps succeeded with a failing tar' >&2
    exit 1
fi
[[ ! -e "$tmp/out dir/packages/good-hash" ]] || exit 1
rm "$tmp/bin/tar"
bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" "$tmp/out dir/packages" >"$tmp/stdout" 2>"$tmp/stderr"
[[ -d "$tmp/out dir/packages/good-hash" && ! -e "$tmp/out dir/packages/good-hash/partial" ]] || exit 1
[[ -z "$(find "$tmp/out dir" -name '.stage-*' -print)" ]] || exit 1

# Publishing never replaces or nests in what another writer published first.
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/zig-deps-stage.sh
source "$script_dir/zig-deps-stage.sh"
pub="$tmp/publish"
mkdir -p "$pub/theirs" "$pub/mine"
echo theirs >"$pub/theirs/file"
echo mine >"$pub/mine/file"
publish "$pub/mine" "$pub/theirs"
[[ ! -e "$pub/mine" && "$(find "$pub/theirs" -mindepth 1 -print)" == "$pub/theirs/file" ]] || exit 1
[[ "$(cat "$pub/theirs/file")" == theirs ]] || exit 1
mkdir "$pub/mine" "$pub/empty"
echo mine >"$pub/mine/file"
publish "$pub/mine" "$pub/empty"
[[ ! -e "$pub/mine" && -z "$(find "$pub/empty" -mindepth 1 -print)" ]] || exit 1
mkdir "$pub/mine"
echo mine >"$pub/mine/file"
publish "$pub/mine" "$pub/new"
[[ ! -e "$pub/mine" && "$(cat "$pub/new/file")" == mine ]] || exit 1
echo theirs >"$pub/their-file"
echo mine >"$pub/my-file"
publish "$pub/my-file" "$pub/their-file"
[[ ! -e "$pub/my-file" && "$(cat "$pub/their-file")" == theirs ]] || exit 1
echo mine >"$pub/my-file"
publish "$pub/my-file" "$pub/new-file"
[[ ! -e "$pub/my-file" && "$(cat "$pub/new-file")" == mine ]] || exit 1
echo mine >"$pub/my-file"
if publish "$pub/my-file" "$pub/no such dir/file" 2>/dev/null; then
    echo 'published into a missing directory' >&2
    exit 1
fi

# Only staging whose owner has exited is removed.
sleep 30 &
live="$!"
bash -c 'exit 0' &
dead="$!"
wait "$dead"
mkdir "$pub/.stage-$live-a" "$pub/.stage-$dead-b"
touch "$pub/.stage-$dead-c" "$pub/.stage-x-d"
remove_dead_stages "$pub"
kill "$live"
wait "$live" 2>/dev/null || true
[[ "$(cd "$pub" && find . -name '.stage-*' | sort | tr '\n' ' ')" == "./.stage-$live-a ./.stage-x-d " ]] || exit 1

# Removing a dead writer's staging is best-effort: what cannot be removed,
# such as files an orphaned download still writes, never ends the caller.
# The caller is a fresh `bash -e` whose `rm` always fails.
mkdir -p "$tmp/failing rm" "$pub/.stage-$dead-e"
printf '#!/usr/bin/env bash\necho "rm: cannot remove" >&2\nexit 1\n' >"$tmp/failing rm/rm"
chmod +x "$tmp/failing rm/rm"
status=0
# shellcheck disable=SC2016 # The inner shell expands these.
PATH="$tmp/failing rm:$PATH" bash -e -c 'source "$1"; remove_dead_stages "$2"; echo survived' \
    _ "$script_dir/zig-deps-stage.sh" "$pub" >"$tmp/removed" 2>&1 || status=$?
[[ "$status" == 0 && "$(cat "$tmp/removed")" == survived && -d "$pub/.stage-$dead-e" ]] ||
    { cat "$tmp/removed" >&2; exit 1; }

# A valid archive stays when Zig itself fails for every file, as when it
# cannot start or write its cache.
: >"$tmp/stderr"
ZIG_BROKEN=1 bash "$script_dir/prefetch-zig-deps.sh" "$tmp/source dir" "$tmp/out dir" "$tmp/out dir/packages" \
    >"$tmp/stdout" 2>"$tmp/stderr"
[[ -f "$tmp/out dir/tarballs/good.tar.gz" ]] || exit 1
grep -qF "skipped unreadable $tmp/out dir/tarballs/good.tar.gz" "$tmp/stderr" || exit 1
echo 'prefetch-zig-deps fixture passed'
