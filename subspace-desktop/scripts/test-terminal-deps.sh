#!/usr/bin/env bash
# terminal-deps fills the offline Ghostty cache from a verified archive, stays
# whole with competing writers, and the make targets that build the terminal
# go on without it when it fails. curl and zig are stubs around the real
# package step; nothing uses the network.
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$script_dir/.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/terminal deps.XXXXXX")"
tmp="$(cd "$tmp" && pwd)"
holder=''
trap '[[ -z "$holder" ]] || kill "$holder" 2>/dev/null || true; rm -rf "$tmp"' EXIT

fail() {
    echo "$*" >&2
    exit 1
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum <"$1" | cut -d ' ' -f 1
    else
        shasum -a 256 <"$1" | cut -d ' ' -f 1
    fi
}

commit=1111111111111111111111111111111111111111
real_tar="$(command -v tar)"
real_sleep="$(command -v sleep)"
mkdir -p "$tmp/bin" "$tmp/repo/scripts" "$tmp/archive/ghostty-$commit" "$tmp/tampered/ghostty-$commit" \
    "$tmp/packages/pkg" "$tmp/packages/dep"
echo 'const std = @import("std");' >"$tmp/archive/ghostty-$commit/build.zig"
# The source names a package, which names another; an example names a
# package that is never fetched.
cat >"$tmp/archive/ghostty-$commit/build.zig.zon" <<'EOF'
.pkg = .{ .url = "https://example.test/pkg.tar.gz", .hash = "pkg-hash" },
EOF
mkdir -p "$tmp/archive/ghostty-$commit/example"
echo '.absent = .{ .hash = "absent-hash" },' >"$tmp/archive/ghostty-$commit/example/build.zig.zon"
"$real_tar" -czf "$tmp/good.tar.gz" -C "$tmp/archive" "ghostty-$commit"
echo 'evil' >"$tmp/tampered/ghostty-$commit/build.zig"
"$real_tar" -czf "$tmp/tampered.tar.gz" -C "$tmp/tampered" "ghostty-$commit"
echo '.dep = .{ .url = "https://example.test/dep.tar.gz", .hash = "dep-hash" },' >"$tmp/packages/pkg/build.zig.zon"
echo 'pkg' >"$tmp/packages/pkg/file"
echo 'dep' >"$tmp/packages/dep/file"
"$real_tar" -czf "$tmp/pkg.tar.gz" -C "$tmp/packages" pkg
"$real_tar" -czf "$tmp/dep.tar.gz" -C "$tmp/packages" dep
mkdir -p "$tmp/regenerated/ghostty-$commit"
echo '// regenerated' >"$tmp/regenerated/ghostty-$commit/build.zig"
"$real_tar" -czf "$tmp/regenerated.tar.gz" -C "$tmp/regenerated" "ghostty-$commit"
regenerated="$(sha256_of "$tmp/regenerated.tar.gz")"
good="$(sha256_of "$tmp/good.tar.gz")"
tampered="$(sha256_of "$tmp/tampered.tar.gz")"

repo="$tmp/repo"
cp "$script_dir/terminal-deps.sh" "$script_dir/ghostty-cache.sh" "$script_dir/zig-deps-stage.sh" \
    "$script_dir/ghostty-offline-env.sh" "$repo/scripts/"
# The real package step, behind a wrapper that records when it ends.
cp "$script_dir/prefetch-zig-deps.sh" "$repo/scripts/prefetch-real.sh"
cat >"$repo/scripts/prefetch-zig-deps.sh" <<'EOF'
#!/usr/bin/env bash
status=0
bash "$(dirname "$0")/prefetch-real.sh" "$@" || status=$?
echo "$status" >>"$TEST_DIR/prefetch.log"
exit "$status"
EOF
cat >"$repo/scripts/ghostty-pin" <<EOF
# A fixture pin.
libghostty-rs 2222222222222222222222222222222222222222
ghostty $commit
ghostty-sha256 $good
EOF
cat >"$tmp/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
while [[ "$1" != '-o' ]]; do shift; done
output="$2"
url="$3"
case "$url" in
"https://github.com/ghostty-org/ghostty/archive/$TEST_COMMIT.tar.gz")
    echo "$url" >>"$TEST_DIR/curl.log"
    trap 'touch "$TEST_DIR/curl.done"' EXIT
    sleep "${CURL_SLEEP:-0}"
    if [[ -n "${CURL_GATE:-}" ]]; then
        touch "$CURL_GATE.waiting"
        while [[ ! -e "$CURL_GATE" ]]; do sleep 0.1; done
    fi
    if [[ -n "${CURL_FAIL:-}" ]]; then
        printf 'partial' >"$output"
        exit 22
    fi
    cp "$CURL_SERVE" "$output"
    ;;
https://example.test/*) cp "$TEST_DIR/${url##*/}" "$output" ;;
*) exit 22 ;;
esac
EOF
cat >"$tmp/bin/zig" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
fetch)
    [[ "$2" == --global-cache-dir ]] || exit 1
    name="${4##*/}"
    mkdir -p "$3/p"
    cp "$4" "$3/p/${name%%.*}-hash.tar.gz"
    ;;
build)
    [[ "$2" == --system && -d "$3" && "$4" == --global-cache-dir && -f build.zig ]] || exit 1
    pwd >>"$TEST_DIR/zig.log"
    ;;
*) exit 1 ;;
esac
EOF
chmod +x "$tmp/bin/curl" "$tmp/bin/zig" "$repo/scripts/prefetch-zig-deps.sh"
export PATH="$tmp/bin:$PATH" TEST_DIR="$tmp" TEST_COMMIT="$commit" CURL_SERVE="$tmp/good.tar.gz"
# Every run stands for Linux, where the cache is supported, whatever the
# host; the Windows cases set their own OSTYPE.
export OSTYPE=linux-gnu

deps="$tmp/zig deps"
source_dir="$deps/ghostty-$commit-$good"
archive="$deps/ghostty-$commit-$good.tar.gz"
marker="$deps/.complete-$commit-$good"
store="$deps/zig-packages"
inventory="ghostty-sha256 $good"$'\npackage dep-hash\npackage pkg-hash'

run_deps() {
    SUBSPACE_ZIG_DEPS_DIR="$deps" SUBSPACE_ZIG_DEPS_LOCK_WAIT="${LOCK_WAIT:-30}" \
        bash "$repo/scripts/terminal-deps.sh" >"$tmp/stdout" 2>"$tmp/stderr"
}

# A run that succeeds and writes nothing to standard error.
run_ok() {
    run_deps || { cat "$tmp/stderr" >&2; fail "$1: terminal-deps failed"; }
    [[ ! -s "$tmp/stderr" ]] || { cat "$tmp/stderr" >&2; fail "$1: wrote to standard error"; }
}

calls() {
    [[ -f "$tmp/$1.log" ]] || { echo 0; return; }
    wc -l <"$tmp/$1.log" | tr -d ' '
}

work() {
    echo "$(calls curl):$(calls prefetch):$(calls zig)"
}

reset_logs() {
    rm -f "$tmp/curl.log" "$tmp/zig.log" "$tmp/prefetch.log"
}

# What the environment script exports for this cache.
exported() {
    # shellcheck disable=SC2016 # The inner shell expands these.
    env -i HOME="$tmp/home" OSTYPE="$OSTYPE" SUBSPACE_ZIG_DEPS_DIR="$deps" bash -c '
        set -eu
        source "$0"
        echo "${GHOSTTY_SOURCE_DIR-}"
    ' "$repo/scripts/ghostty-offline-env.sh"
}

# The cache is ready, whole and exported, with nothing else left behind.
expect_ready() {
    local name="$1" left
    [[ "$(cat "$source_dir/build.zig")" == 'const std = @import("std");' ]] || fail "$name: no verified source"
    [[ "$(find "$source_dir" -mindepth 1 ! -path '*/example*' -print | sort)" == \
        "$source_dir/build.zig"$'\n'"$source_dir/build.zig.zon" ]] || fail "$name: the source is not whole"
    [[ "$(cat "$store/pkg-hash/file"):$(cat "$store/dep-hash/file")" == pkg:dep ]] ||
        fail "$name: a package is incomplete"
    [[ "$(find "$store" -mindepth 1 -maxdepth 2 -print | sort)" == "$(printf '%s\n' \
        "$store/dep-hash" "$store/dep-hash/file" "$store/pkg-hash" \
        "$store/pkg-hash/build.zig.zon" "$store/pkg-hash/file")" ]] || fail "$name: a package nested"
    [[ "$(cat "$marker")" == "$inventory" ]] || fail "$name: marker '$(cat "$marker")'"
    [[ "$(exported)" == "$source_dir" ]] || fail "$name: not exported"
    expect_clean "$name"
}

# No lock and no staging is left behind.
expect_clean() {
    [[ ! -e "$deps/.lock" ]] || fail "$1: the lock was not released"
    local left
    left="$(find "$deps" "$deps/tarballs" -mindepth 1 -maxdepth 1 -name '.*' ! -name '.complete-*' -print 2>/dev/null || true)"
    [[ -z "$left" ]] || fail "$1: left $left"
}

expect_fails() {
    local name="$1" message="$2"
    if run_deps; then
        fail "$name: terminal-deps succeeded"
    fi
    [[ -z "$message" ]] || grep -qF "$message" "$tmp/stderr" || { cat "$tmp/stderr" >&2; fail "$name: no '$message'"; }
    [[ ! -e "$source_dir" && ! -e "$marker" ]] || fail "$name: extracted the source"
    [[ -z "$(exported)" ]] || fail "$name: exported the cache"
    expect_clean "$name"
}

# Wait for the predicate named first, up to a minute: an event, never a
# duration, decides each case.
await() {
    local _
    for _ in $(seq 600); do
        ! "$@" || return 0
        sleep 0.1
    done
    fail "timed out waiting for $*"
}

exists() {
    [[ -e "$1" ]]
}

package_steps_ended() {
    [[ "$(calls prefetch)" -ge "$1" ]]
}

fresh() {
    rm -rf "$deps"
    mkdir -p "$deps"
    reset_logs
}

# A cold cache downloads, verifies, extracts and resolves once, and records
# the packages the source's manifests reach.
fresh
run_ok "cold cache"
[[ "$(work)" == 1:1:1 ]] || fail "cold cache: work $(work)"
[[ "$(cat "$tmp/zig.log")" == "$source_dir" ]] || fail "cold cache: built in $(cat "$tmp/zig.log")"
expect_ready "cold cache"

# A ready cache does nothing.
reset_logs
run_ok "ready cache"
grep -q 'offline Ghostty cache ready' "$tmp/stdout" || fail "ready cache: not reported ready"
[[ "$(work)" == 0:0:0 ]] || fail "ready cache: work $(work)"

# A cache missing its source, its store or one package is not ready and is
# rebuilt from the verified archive, without a download.
for gone in "$source_dir" "$store" "$store/dep-hash"; do
    rm -rf "$gone"
    [[ -z "$(exported)" ]] || fail "without $gone: exported"
    reset_logs
    run_ok "without $gone"
    [[ "$(work)" == 0:1:1 ]] || fail "without $gone: work $(work)"
    expect_ready "without $gone"
done

# A marker from before the digest, or naming another digest, is replaced.
for old in '' "ghostty-sha256 $tampered"; do
    printf '%s' "$old" >"$marker"
    [[ -z "$(exported)" ]] || fail "marker '$old': exported"
    reset_logs
    run_ok "marker '$old'"
    [[ "$(work)" == 0:1:1 ]] || fail "marker '$old': work $(work)"
    expect_ready "marker '$old'"
done

# What an earlier version of the task left is never used: its tree and
# archive, unverified; its marker; and its package store, which could hold a
# half-unpacked package.
fresh
mkdir -p "$deps/ghostty-$commit" "$deps/system/pkg-hash"
echo 'evil' >"$deps/ghostty-$commit/build.zig"
cp "$tmp/tampered.tar.gz" "$deps/ghostty-$commit.tar.gz"
cp "$tmp/packages/pkg/build.zig.zon" "$deps/system/pkg-hash/"
touch "$deps/.complete-$commit"
[[ -z "$(exported)" ]] || fail "earlier cache: exported"
run_ok "earlier cache"
[[ "$(work)" == 1:1:1 && "$(cat "$tmp/zig.log")" == "$source_dir" ]] || fail "earlier cache: used it"
rm -rf "$deps/ghostty-$commit" "$deps/ghostty-$commit.tar.gz" "$deps/system"
expect_ready "earlier cache"

# A tree at the digest's name without build.zig is damaged, as a writer
# publishes only whole trees. Another run may be using it, so the task leaves
# it and fails, naming it; once it is removed, the next run replaces it.
rm "$source_dir/build.zig"
reset_logs
if run_deps; then
    fail "damaged tree: terminal-deps succeeded"
fi
grep -qF "$source_dir is damaged (it has no build.zig); remove it and run again" "$tmp/stderr" ||
    { cat "$tmp/stderr" >&2; fail "damaged tree: no message"; }
[[ -f "$source_dir/build.zig.zon" && "$(work)" == 0:0:0 ]] || fail "damaged tree: touched it"
rm -rf "$source_dir"
run_ok "after removing a damaged tree"
expect_ready "after removing a damaged tree"

# The bytes unpacked are the bytes verified: here another process replaces
# the cached archive between the check and the unpacking.
mkdir -p "$tmp/swapping tar"
cat >"$tmp/swapping tar/tar" <<EOF
#!/usr/bin/env bash
if [[ "\$1" == -xzf ]]; then
    rm -f "$archive"
    cp "$tmp/tampered.tar.gz" "$archive"
fi
exec "$real_tar" "\$@"
EOF
chmod +x "$tmp/swapping tar/tar"
rm -rf "$source_dir"
reset_logs
PATH="$tmp/swapping tar:$PATH" run_ok "archive swapped"
[[ "$(work)" == 0:1:1 ]] || fail "archive swapped: work $(work)"
expect_ready "archive swapped"

# A file at the archive's name without its digest is damaged: the task
# removes it and downloads the archive again.
rm -rf "$source_dir"
reset_logs
run_ok "damaged archive"
grep -qF "removed $archive, which is damaged (SHA-256 $tampered)" "$tmp/stdout" ||
    fail "damaged archive: no message"
[[ "$(work)" == 1:1:1 && "$(sha256_of "$archive")" == "$good" ]] || fail "damaged archive: kept it"
expect_ready "damaged archive"

# A download without the pinned digest is refused and extracts nothing.
fresh
CURL_SERVE="$tmp/tampered.tar.gz" expect_fails "tampered download" "expected $good, got $tampered"
[[ ! -e "$archive" ]] || fail "tampered download: kept"

# A failed download fails loudly and leaves no partial file.
CURL_FAIL=1 expect_fails "failed download" "could not download"

# A cache on a file system without hard links fails before any download.
mkdir -p "$tmp/no links"
printf '#!/usr/bin/env bash\nexit 1\n' >"$tmp/no links/ln"
chmod +x "$tmp/no links/ln"
reset_logs
PATH="$tmp/no links:$PATH" expect_fails "no hard links" \
    "the offline Ghostty cache needs hard links, which $deps does not support; set SUBSPACE_ZIG_DEPS_DIR"
[[ "$(calls curl)" == 0 ]] || fail "no hard links: downloaded"

# Windows is not supported yet: the task stops before touching anything.
for ostype in msys cygwin; do
    if OSTYPE="$ostype" SUBSPACE_ZIG_DEPS_DIR="$tmp/windows" bash "$repo/scripts/terminal-deps.sh" \
        >"$tmp/stdout" 2>"$tmp/stderr"; then
        fail "$ostype: terminal-deps succeeded"
    fi
    [[ "$(cat "$tmp/stderr")" == 'the offline Ghostty cache is not supported on Windows yet' ]] ||
        fail "$ostype: printed '$(cat "$tmp/stderr")'"
    [[ ! -e "$tmp/windows" ]] || fail "$ostype: created the cache"
done

# A relative cache is refused before anything is written.
(cd "$tmp" && SUBSPACE_ZIG_DEPS_DIR=relative bash "$repo/scripts/terminal-deps.sh" >/dev/null 2>"$tmp/stderr") &&
    fail "relative cache: accepted"
grep -q 'must be an absolute path, not relative' "$tmp/stderr" || fail "relative cache: no message"
[[ ! -e "$tmp/relative" && ! -e "$repo/relative" ]] || fail "relative cache: created it"

# An interrupted extraction leaves no source tree to be taken as whole.
mkdir -p "$tmp/broken tar"
cat >"$tmp/broken tar/tar" <<'EOF'
#!/usr/bin/env bash
while [[ "$1" != '-C' ]]; do shift; done
echo 'partial' >"$2/build.zig"
exit 1
EOF
chmod +x "$tmp/broken tar/tar"
PATH="$tmp/broken tar:$PATH" expect_fails "interrupted extraction" ""
reset_logs
run_ok "after an interrupted extraction"
expect_ready "after an interrupted extraction"

# An interrupted run releases the lock and removes its staging. Its download
# outlives it and may write the staging file again; the next run removes it.
fresh
rm -f "$tmp/curl.done" "$tmp/curl.gate" "$tmp/curl.gate.waiting"
CURL_GATE="$tmp/curl.gate" SUBSPACE_ZIG_DEPS_DIR="$deps" bash "$repo/scripts/terminal-deps.sh" >/dev/null 2>&1 &
run="$!"
await exists "$tmp/curl.gate.waiting"
kill -TERM "$run"
if wait "$run"; then
    fail "interrupted run: succeeded"
fi
touch "$tmp/curl.gate"
await exists "$tmp/curl.done"
[[ ! -e "$archive" && ! -e "$source_dir" && ! -e "$deps/.lock" ]] ||
    fail "interrupted run: left the archive, source or lock"

# A live holder makes a run wait, up to the limit, then fill the cache
# alongside it, leaving the holder's lock alone.
sleep 60 &
holder="$!"
echo "$holder" >"$deps/.lock"
LOCK_WAIT=1 run_ok "lock held"
grep -qF "terminal-deps (process $holder) is still filling $deps after 1s" "$tmp/stdout" ||
    fail "lock held: no message"
[[ "$(cat "$deps/.lock")" == "$holder" ]] || fail "lock held: taken from a live holder"
kill "$holder"
wait "$holder" 2>/dev/null || true
holder=''
rm "$deps/.lock"
expect_ready "lock held"

# A lock whose holder has gone, or that a killed run left empty, is taken
# over without waiting out the limit.
bash -c 'exit 0' &
holder_gone="$!"
wait "$holder_gone"
for left in "$holder_gone" ''; do
    fresh
    printf '%s' "$left" >"$deps/.lock"
    LOCK_WAIT=30 run_ok "stale lock '$left'"
    ! grep -q 'still filling' "$tmp/stdout" || fail "stale lock '$left': waited out the limit"
    expect_ready "stale lock '$left'"
done

# A lock that is empty for a moment while a new holder writes it is not taken
# from that holder, though an earlier holder was seen. The waiter's own
# sleeps between polls set the lock it reads next: empty, a live holder,
# empty, then the holder again.
fresh
sleep 60 &
holder="$!"
: >"$deps/.lock"
mkdir -p "$tmp/polled sleep"
cat >"$tmp/polled sleep/sleep" <<EOF
#!/usr/bin/env bash
if [[ "\$1" == 1 ]]; then
    polls=\$((\$(cat "$tmp/polls" 2>/dev/null || echo 0) + 1))
    echo "\$polls" >"$tmp/polls"
    case "\$polls" in
    1 | 3) echo "$holder" >"$deps/.lock" ;;
    2) : >"$deps/.lock" ;;
    esac
    exit 0
fi
exec "$real_sleep" "\$@"
EOF
chmod +x "$tmp/polled sleep/sleep"
PATH="$tmp/polled sleep:$PATH" LOCK_WAIT=4 run_ok "lock briefly empty"
[[ "$(cat "$tmp/polls")" == 4 ]] || fail "lock briefly empty: polled $(cat "$tmp/polls") times"
grep -qF "terminal-deps (process $holder) is still filling" "$tmp/stdout" ||
    fail "lock briefly empty: took the lock from its new holder"
kill "$holder"
wait "$holder" 2>/dev/null || true
holder=''
rm "$deps/.lock"
expect_ready "lock briefly empty"

# Checkouts pinning two digests of one commit, as while a regenerated
# archive's digest is recorded, share no path: neither undoes the other.
repo2="$tmp/repo2"
cp -R "$repo" "$repo2"
sed "s/^ghostty-sha256 .*/ghostty-sha256 $regenerated/" "$repo/scripts/ghostty-pin" >"$repo2/scripts/ghostty-pin"
fresh
CURL_SLEEP=1 SUBSPACE_ZIG_DEPS_DIR="$deps" bash "$repo/scripts/terminal-deps.sh" >"$tmp/first" 2>&1 &
one="$!"
CURL_SLEEP=1 CURL_SERVE="$tmp/regenerated.tar.gz" SUBSPACE_ZIG_DEPS_DIR="$deps" \
    bash "$repo2/scripts/terminal-deps.sh" >"$tmp/second" 2>&1 &
two="$!"
wait "$one" || { cat "$tmp/first" >&2; fail "two digests: the first failed"; }
wait "$two" || { cat "$tmp/second" >&2; fail "two digests: the second failed"; }
expect_ready "two digests"
[[ "$(cat "$deps/ghostty-$commit-$regenerated/build.zig")" == '// regenerated' ]] ||
    fail "two digests: the second tree is not its own archive's"
# shellcheck disable=SC2016 # The inner shell expands these.
[[ "$(env -i HOME="$tmp/home" OSTYPE="$OSTYPE" SUBSPACE_ZIG_DEPS_DIR="$deps" bash -c 'source "$0"; echo "$GHOSTTY_SOURCE_DIR"' \
    "$repo2/scripts/ghostty-offline-env.sh")" == "$deps/ghostty-$commit-$regenerated" ]] ||
    fail "two digests: the second checkout's cache is not ready"

# Two runs at once download once and nest nothing.
fresh
CURL_SLEEP=1 SUBSPACE_ZIG_DEPS_DIR="$deps" bash "$repo/scripts/terminal-deps.sh" >"$tmp/first" 2>&1 &
one="$!"
CURL_SLEEP=1 SUBSPACE_ZIG_DEPS_DIR="$deps" bash "$repo/scripts/terminal-deps.sh" >"$tmp/second" 2>&1 &
two="$!"
wait "$one" || { cat "$tmp/first" >&2; fail "concurrent runs: the first failed"; }
wait "$two" || { cat "$tmp/second" >&2; fail "concurrent runs: the second failed"; }
[[ "$(work)" == 1:1:1 ]] || fail "concurrent runs: work $(work)"
expect_ready "concurrent runs"

# A run killed outright while its package step is unpacking: a second run
# takes over and fills the cache, then the first run's package step finishes
# and publishes the same package, which must neither nest nor replace it.
fresh
mkdir -p "$tmp/gated tar"
cat >"$tmp/gated tar/tar" <<EOF
#!/usr/bin/env bash
if [[ "\$*" == *'/cache/p/pkg-hash.tar.gz'* ]]; then
    touch "$tmp/gate.waiting"
    while [[ ! -e "$tmp/gate" ]]; do sleep 0.1; done
fi
exec "$real_tar" "\$@"
EOF
chmod +x "$tmp/gated tar/tar"
PATH="$tmp/gated tar:$PATH" SUBSPACE_ZIG_DEPS_DIR="$deps" \
    bash "$repo/scripts/terminal-deps.sh" >/dev/null 2>&1 &
killed="$!"
await exists "$tmp/gate.waiting"
kill -KILL "$killed"
wait "$killed" 2>/dev/null || true
[[ -n "$(find "$deps" -maxdepth 1 -name '.stage-*' -print)" ]] || fail "killed run: nothing staged"
LOCK_WAIT=30 run_ok "after a killed run"
touch "$tmp/gate"
await package_steps_ended 2
[[ "$(cat "$tmp/prefetch.log")" == $'0\n0' ]] || fail "killed run: package steps ended '$(cat "$tmp/prefetch.log")'"
expect_ready "after a killed run"

# The make targets that build the terminal fill the cache first and build
# without it when that fails; the others never run it. The outer make's
# command line must not reach the inner one.
cat >"$tmp/bin/mise" <<'EOF'
#!/usr/bin/env bash
printf '%s|%s\n' "${MISE_ENV-}" "$*" >>"$TEST_DIR/mise.log"
[[ "$*" != 'run terminal-deps' || -z "${MISE_FAIL_DEPS:-}" ]]
EOF
chmod +x "$tmp/bin/mise"
make_runs() {
    local want="$1" got
    shift
    rm -f "$tmp/mise.log"
    env -u MAKEFLAGS -u MFLAGS -u MAKELEVEL -u MISE_ENV MISE_FAIL_DEPS=1 \
        make -s -C "$root" "$@" >/dev/null 2>"$tmp/make.err" ||
        { cat "$tmp/make.err" >&2; fail "make $*: failed"; }
    got="$(cut -d ' ' -f 1-2 "$tmp/mise.log" | tr '\n' ';')"
    [[ "$got" == "$want" ]] || fail "make $*: ran '$got', want '$want'"
    if [[ "$want" == *terminal-deps* ]]; then
        [[ "$(cat "$tmp/make.err")" == "offline Ghostty cache could not be filled; building without it" ]] ||
            fail "make $*: printed '$(cat "$tmp/make.err")'"
    fi
}
make_runs 'terminal|run terminal-deps;terminal|run run;' run
make_runs '|run run;' run FEATURES=
make_runs 'terminal|run terminal-deps;terminal|run run;' run-unreleased
make_runs 'terminal|run terminal-deps;terminal|run capture;' capture OUT="$tmp/out"
make_runs 'terminal|run terminal-deps;terminal|run check-terminal;' check-terminal
make_runs 'terminal|run terminal-deps;terminal|run install-macos;' install-macos
make_runs '|run install-macos;' install-macos FEATURES=
make_runs '|run build;' build
make_runs '|run package-macos;' package-macos
echo 'terminal-deps fixtures passed'
