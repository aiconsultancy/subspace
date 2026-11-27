#!/usr/bin/env bash
# The offline Ghostty environment uses only a ready cache of the commit this
# checkout pins, and the pin agrees with the terminal crates' libghostty-rs rev.
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$script_dir/.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/ghostty env.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

fail() {
    echo "$*" >&2
    exit 1
}

pin_value() {
    awk -v key="$1" '$1 == key { print $2 }' "$script_dir/ghostty-pin"
}
rev="$(pin_value libghostty-rs)"
pin="$(pin_value ghostty)"
digest="$(pin_value ghostty-sha256)"
[[ "$rev" =~ ^[0-9a-f]{40}$ ]] || fail "scripts/ghostty-pin has no libghostty-rs rev: $rev"
[[ "$pin" =~ ^[0-9a-f]{40}$ ]] || fail "scripts/ghostty-pin has no Ghostty commit: $pin"
[[ "$digest" =~ ^[0-9a-f]{64}$ ]] || fail "scripts/ghostty-pin has no archive digest: $digest"

# The variables the script exports, one per line, from an otherwise empty
# environment on Linux (OSTYPE linux-gnu, whatever the host) with `deps` as
# the cache and the given variables, OSTYPE included, already set.
# The shell has `set -eu` on, as a caller's may, and must survive the source
# without a message, as mise sources it before every command.
exported_by() {
    local env_script="$1" deps="$2"
    shift 2
    # shellcheck disable=SC2016 # The inner shell expands these.
    env -i HOME="$tmp/home" OSTYPE=linux-gnu SUBSPACE_ZIG_DEPS_DIR="$deps" "$@" bash -c '
        set -euo pipefail
        source "$0"
        printf "%s\n" "source=${GHOSTTY_SOURCE_DIR-}" "system=${GHOSTTY_ZIG_SYSTEM_DIR-}"
        [[ -z "$(compgen -v ghostty_ || true)$(compgen -A function ghostty_ || true)" ]] ||
            echo "leaked $(compgen -v ghostty_) $(compgen -A function ghostty_)"
    ' "$env_script" 2>&1
}

exported() {
    exported_by "$script_dir/ghostty-offline-env.sh" "$@"
}

expect() {
    local name="$1" want="$2" got="$3"
    [[ "$got" == "$want" ]] || { printf '%s\nwant:\n%s\ngot:\n%s\n' "$name" "$want" "$got" >&2; exit 1; }
}

# A cache that `terminal-deps` has filled for `commit` from the archive with
# `sha`, holding the named packages.
fill() {
    local deps="$1" commit="$2" sha="$3" package
    shift 3
    mkdir -p "$deps/ghostty-$commit-$sha" "$deps/zig-packages"
    touch "$deps/ghostty-$commit-$sha/build.zig"
    {
        echo "ghostty-sha256 $sha"
        for package in "$@"; do
            mkdir -p "$deps/zig-packages/$package"
            echo "package $package"
        done
    } >"$deps/.complete-$commit-$sha"
}

none=$'source=\nsystem='

deps="$tmp/zig deps"
mkdir -p "$deps"
expect "no cache" "$none" "$(exported "$deps")"

# Sorts before the pin, as a later Ghostty bump may.
other=0000000000000000000000000000000000000000
[[ "$(printf '%s\n' "$pin" "$other" | sort | head -n 1)" == "$other" ]] || exit 1
fill "$deps" "$other" "$digest" a-hash
expect "only another commit is cached" "$none" "$(exported "$deps")"

fill "$deps" "$pin" "$digest" a-hash b.0-hash_X
source_dir="$deps/ghostty-$pin-$digest"
ready="source=$source_dir"$'\n'"system=$deps/zig-packages"
expect "the pinned commit among two" "$ready" "$(exported "$deps")"

marker="$deps/.complete-$pin-$digest"
cp "$marker" "$tmp/marker"
not_ready() {
    local name="$1"
    expect "$name" "$none" "$(exported "$deps")"
    cp "$tmp/marker" "$marker"
    expect "after $name" "$ready" "$(exported "$deps")"
}
rm "$marker"
not_ready "no marker"
: >"$marker"
not_ready "a marker from before the digest"
sed -i.bak "s/^ghostty-sha256 .*/ghostty-sha256 $(printf '%064d' 0)/" "$marker"
not_ready "a marker for another archive"
grep -v '^ghostty-sha256' "$tmp/marker" >"$marker"
not_ready "a marker without its digest"
{ cat "$tmp/marker" && echo "ghostty-sha256 $digest"; } >"$marker"
not_ready "a marker with two digests"
for line in 'package' 'package ../escape' 'package a/b' 'package a-hash/' 'package ./a-hash' 'package .' 'package ..' 'package a b' \
    'extra line' ''; do
    { cat "$tmp/marker" && echo "$line"; } >"$marker"
    not_ready "a marker line '$line'"
done
{ echo 'package a-hash' && cat "$tmp/marker"; } >"$marker"
not_ready "a package before the digest"
# Each of these alone makes the cache not ready.
gone() {
    local name="$1" path="$2"
    mv "$path" "$tmp/gone"
    expect "$name" "$none" "$(exported "$deps")"
    mv "$tmp/gone" "$path"
    expect "after $name" "$ready" "$(exported "$deps")"
}
gone "a missing package" "$deps/zig-packages/b.0-hash_X"
gone "no package store" "$deps/zig-packages"
gone "no source" "$source_dir"

# A tree where an earlier version extracted it, without the digest in its
# name, is not one terminal-deps verified.
mv "$source_dir" "$deps/ghostty-$pin"
expect "an unverified tree" "$none" "$(exported "$deps")"
mv "$deps/ghostty-$pin" "$source_dir"

# A relative cache would name another directory in the build script.
(cd "$tmp" && expect "a relative cache" "$none" "$(exported "zig deps")")

# Absolute paths start at the root; nothing else is.
(
    # shellcheck source-path=SCRIPTDIR/..
    # shellcheck source=scripts/ghostty-cache.sh
    source "$script_dir/ghostty-cache.sh"
    for path in / /cache '/c/Users/me/cache'; do
        ghostty_cache_absolute "$path" || fail "not absolute: $path"
    done
    # shellcheck disable=SC2088 # A literal tilde is relative.
    for path in '' cache ./cache ../cache 'C:/Cache' 'C:\Cache' '\\server\share' '~/cache'; do
        ! ghostty_cache_absolute "$path" || fail "absolute: $path"
    done
)

# Windows is not supported yet: nothing is exported there.
for ostype in msys cygwin; do
    expect "OSTYPE $ostype" "$none" "$(exported "$deps" OSTYPE="$ostype")"
done
expect "OSTYPE darwin" "$ready" "$(exported "$deps" OSTYPE=darwin25)"

# Only the cache an earlier version of the task filled: never used.
earlier="$tmp/earlier"
mkdir -p "$earlier/ghostty-$pin" "$earlier/system/a-hash"
touch "$earlier/ghostty-$pin/build.zig" "$earlier/.complete-$pin"
expect "an earlier version's cache" "$none" "$(exported "$earlier")"

expect "the caller's source directory turns the cache off" \
    $'source=/caller/source\nsystem=' \
    "$(exported "$deps" GHOSTTY_SOURCE_DIR=/caller/source)"
expect "the caller's source directory with its own store" \
    $'source=/caller/source\nsystem=/caller/system' \
    "$(exported "$deps" GHOSTTY_SOURCE_DIR=/caller/source GHOSTTY_ZIG_SYSTEM_DIR=/caller/system)"
expect "the caller's package directory wins" \
    "source=$source_dir"$'\n'"system=/caller/system" \
    "$(exported "$deps" GHOSTTY_ZIG_SYSTEM_DIR=/caller/system)"

# A copy of the scripts with a broken pin exports nothing and does not end
# the caller's shell, even though the pinned commit is ready.
copy="$tmp/copy"
mkdir -p "$copy"
cp "$script_dir/ghostty-offline-env.sh" "$script_dir/ghostty-cache.sh" "$copy/"
expect "a missing pin" "$none" "$(exported_by "$copy/ghostty-offline-env.sh" "$deps")"
sed "s/^ghostty $pin/ghostty ${pin:0:8}/" "$script_dir/ghostty-pin" >"$copy/ghostty-pin"
expect "a short commit in the pin" "$none" "$(exported_by "$copy/ghostty-offline-env.sh" "$deps")"
grep -v '^ghostty-sha256 ' "$script_dir/ghostty-pin" >"$copy/ghostty-pin"
expect "a pin without its digest" "$none" "$(exported_by "$copy/ghostty-offline-env.sh" "$deps")"
cp "$script_dir/ghostty-pin" "$copy/ghostty-pin"
expect "the copy with the real pin" "$ready" "$(exported_by "$copy/ghostty-offline-env.sh" "$deps")"

# The pin is the libghostty-rs rev the terminal crates build.
manifests=0
for file in terminal/runtime/Cargo.toml terminal/panel/Cargo.toml; do
    grep -q 'libghostty-rs' "$root/$file" || continue
    manifests=$((manifests + 1))
    while IFS= read -r named; do
        [[ "$named" == "$rev" ]] || fail "$file names libghostty-rs rev $named, scripts/ghostty-pin $rev"
    done < <(grep 'libghostty-rs' "$root/$file" | grep -Eo 'rev = "[^"]*"' | cut -d '"' -f 2)
    grep -q "rev = \"$rev\"" "$root/$file" || fail "$file names no libghostty-rs rev"
done
((manifests > 0)) || fail "no terminal Cargo.toml names libghostty-rs"
locks=0
for file in Cargo.lock terminal/runtime/Cargo.lock terminal/panel/Cargo.lock; do
    grep -q 'libghostty-rs' "$root/$file" || continue
    locks=$((locks + 1))
    while IFS= read -r named; do
        [[ "$named" == "rev=$rev#$rev" ]] || fail "$file locks libghostty-rs at $named, scripts/ghostty-pin $rev"
    done < <(grep -Eo 'libghostty-rs\?rev=[^"]*' "$root/$file" | cut -d '?' -f 2)
done
((locks > 0)) || fail "no lockfile names libghostty-rs"
short="$(grep -Eo 'This rev pins Ghostty [0-9a-f]+' "$root/terminal/runtime/Cargo.toml" | awk '{ print $5 }')"
[[ -n "$short" && "$pin" == "$short"* ]] ||
    fail "terminal/runtime/Cargo.toml says the rev pins Ghostty '$short', scripts/ghostty-pin $pin"
# When Cargo has checked the rev out, its build script names the same commit.
for build in "${CARGO_HOME:-$HOME/.cargo}"/git/checkouts/libghostty-rs-*/"${rev:0:7}"/crates/libghostty-vt-sys/build.rs; do
    [[ -f "$build" ]] || continue
    grep -q "GHOSTTY_COMMIT: &str = \"$pin\"" "$build" ||
        fail "$build does not build Ghostty $pin"
done

# The pin is written out in these places too; they must not drift from it.
for file in docs/terminal.md licenses/terminal/README.md; do
    grep -q "$pin" "$root/$file" || fail "$file does not name the pinned Ghostty commit $pin"
done
grep -q "$rev" "$root/licenses/terminal/README.md" ||
    fail "licenses/terminal/README.md does not name the pinned libghostty-rs rev $rev"
for file in scripts/terminal-deps.sh scripts/ghostty-offline-env.sh scripts/ghostty-cache.sh; do
    [[ -f "$root/$file" ]] || fail "$file is missing"
    ! grep -Eq '[0-9a-f]{40}' "$root/$file" || fail "$file repeats a pin from scripts/ghostty-pin"
done
grep -q 'scripts/terminal-deps.sh' <(sed -n '/^\[tasks.terminal-deps\]/,/^\[tasks\./p' "$root/mise.toml") ||
    fail "mise.toml terminal-deps does not run scripts/terminal-deps.sh"
echo "ghostty offline environment fixtures passed"
