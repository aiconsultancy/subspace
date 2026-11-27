#!/usr/bin/env bash
# Fill the shared offline Ghostty cache for the pin in scripts/ghostty-pin:
# the Ghostty source from GitHub's archive, verified against the pinned
# SHA-256, and every Zig package its build needs. ghostty-offline-env.sh points
# terminal builds at the cache once it is ready.
#
# Checkouts and worktrees share the cache and may fill it at the same time.
# Every write is staged under the writer's pid and published without replacing
# another writer's (zig-deps-stage.sh), so the cache stays whole whatever runs
# at once; the lock only saves a second run the same downloads.
#
# Usage: mise run terminal-deps
#   SUBSPACE_ZIG_DEPS_DIR        the cache, an absolute path
#                                (default ~/.cache/subspace/zig-deps)
#   SUBSPACE_ZIG_DEPS_LOCK_WAIT  seconds to wait for another run before
#                                filling the cache alongside it (default 60)
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/ghostty-cache.sh
source "$script_dir/ghostty-cache.sh"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/zig-deps-stage.sh
source "$script_dir/zig-deps-stage.sh"
if ! ghostty_cache_supported; then
    echo "the offline Ghostty cache is not supported on Windows yet" >&2
    exit 1
fi
ghostty_cache_read_pin "$script_dir/ghostty-pin" ||
    { echo "scripts/ghostty-pin is missing or malformed" >&2; exit 1; }
# Outside target/, which a build cache may sweep, and shared by every checkout.
deps="${SUBSPACE_ZIG_DEPS_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/subspace/zig-deps}"
if ! ghostty_cache_absolute "$deps"; then
    echo "the offline Ghostty cache must be an absolute path, not $deps;" \
        "set SUBSPACE_ZIG_DEPS_DIR or XDG_CACHE_HOME to one" >&2
    exit 1
fi
wait_limit="${SUBSPACE_ZIG_DEPS_LOCK_WAIT:-60}"
source_dir="$deps/$ghostty_source_name"
archive="$deps/$ghostty_archive_name"
marker="$deps/$ghostty_marker_name"
store="$deps/$ghostty_store_name"
url="https://github.com/ghostty-org/ghostty/archive/$ghostty_commit.tar.gz"
lock="$deps/.lock"
staged=()
held=''

# Only this run's own staging and its own lock: everything else may belong to
# a live writer. Bash runs the EXIT trap on an interrupt or SIGTERM too.
cleanup() {
    local holder=''
    if ((${#staged[@]})); then
        rm -rf "${staged[@]}"
    fi
    { read -r holder <"$lock"; } 2>/dev/null || true
    if [[ "$holder" == "$$" ]]; then
        rm -f "$lock"
    fi
}
trap cleanup EXIT

ready() {
    if ghostty_cache_ready "$deps"; then
        echo "offline Ghostty cache ready: $deps"
        return 0
    fi
    return 1
}

# The lock is a file holding its holder's pid. With noclobber, bash creates it
# with O_EXCL and the pid is written at once. A lock whose holder has gone,
# or that is still empty a second later (its holder died between the two),
# is removed. After the wait the run goes on without the lock, which
# costs only duplicate downloads.
take_lock() {
    local waited=0 holder empty=''
    until (set -C && printf '%s\n' "$$" >"$lock") 2>/dev/null; do
        ready && exit 0
        holder=''
        { read -r holder <"$lock"; } 2>/dev/null || true
        if [[ -z "$holder" && -n "$empty" ]] || { [[ -n "$holder" ]] && ! kill -0 "$holder" 2>/dev/null; }; then
            rm -rf "$lock"
            empty=''
            continue
        fi
        if [[ -n "$holder" ]]; then
            empty=''
        else
            empty=1
        fi
        if ((waited >= wait_limit)); then
            echo "terminal-deps (process ${holder:-unknown}) is still filling $deps after ${wait_limit}s;" \
                "filling it alongside"
            return 0
        fi
        sleep 1
        waited=$((waited + 1))
    done
}

sha256_of() {
    local line
    if command -v sha256sum >/dev/null 2>&1; then
        line="$(sha256sum <"$1")"
    elif command -v shasum >/dev/null 2>&1; then
        line="$(shasum -a 256 <"$1")"
    else
        echo "neither sha256sum nor shasum is installed" >&2
        return 1
    fi
    echo "${line%% *}"
}

# Publishing and holding files need hard links: find out before downloading.
check_links() {
    local probe
    probe="$(stage "$deps")"
    staged+=("$probe" "$probe.link")
    if ! ln "$probe" "$probe.link" 2>/dev/null; then
        echo "the offline Ghostty cache needs hard links, which $deps does not support;" \
            "set SUBSPACE_ZIG_DEPS_DIR to a directory on a file system that has them" >&2
        exit 1
    fi
}

# Set `held` to this run's own verified copy of the pinned archive: a hard
# link to the cached one, which no writer changes in place, or a download.
# The bytes checked are the bytes later unpacked, whatever another run does
# to the cached name meanwhile.
hold_archive() {
    local dir digest
    dir="$(stage -d "$deps")"
    staged+=("$dir")
    held="$dir/archive"
    if ln "$archive" "$held" 2>/dev/null; then
        digest="$(sha256_of "$held")"
        [[ "$digest" != "$ghostty_sha256" ]] || return 0
        # Only bytes with this digest are published under this name, so these
        # are damaged. Removing the name leaves any other run's own hold
        # intact; a good copy another run put there meanwhile is kept.
        echo "removed $archive, which is damaged (SHA-256 $digest); downloading it again"
        [[ ! "$archive" -ef "$held" ]] || rm -f "$archive"
        rm -f "$held"
    fi
    curl --fail --silent --show-error --location \
        --connect-timeout 10 --max-time 45 --retry 2 --retry-delay 2 \
        --retry-max-time 100 --retry-all-errors -o "$held" "$url" ||
        { echo "could not download $url" >&2; return 1; }
    digest="$(sha256_of "$held")"
    if [[ "$digest" != "$ghostty_sha256" ]]; then
        echo "the Ghostty archive from $url does not match scripts/ghostty-pin:" \
            "expected $ghostty_sha256, got $digest" >&2
        return 1
    fi
    ln "$held" "$dir/published"
    publish "$dir/published" "$archive"
}

# Unpack this run's verified archive into staging and publish the tree whole
# under the name that records the digest, so a tree from anywhere else is
# never used and an interrupted extraction never looks like a source tree.
extract() {
    local tree
    # A writer publishes only whole trees, with build.zig, so one without it
    # is damaged. Another run may be using whatever is there: leave it.
    if [[ -e "$source_dir" && ! -f "$source_dir/build.zig" ]]; then
        echo "$source_dir is damaged (it has no build.zig); remove it and run again" >&2
        exit 1
    fi
    hold_archive
    tree="$(stage -d "$deps")"
    staged+=("$tree")
    tar -xzf "$held" -C "$tree" --strip-components=1
    publish "$tree" "$source_dir"
}

# The Zig package hashes the source's manifests name, then those of the
# packages they bring, that the store holds.
zon_hashes() {
    find "$@" -name build.zig.zon -not -path '*/zig-cache/*' -not -path '*/.zig-cache/*' \
        -exec grep -hEo '\.hash *= *"[^"/]+"' {} + 2>/dev/null | cut -d '"' -f 2 | sort -u || true
}

inventory() {
    local found='' next name dirs
    next="$(zon_hashes "$source_dir")"
    while [[ -n "$next" ]]; do
        found="$(printf '%s\n%s\n' "$found" "$next" | sed '/^$/d' | sort -u)"
        dirs=()
        for name in $next; do
            [[ ! -d "$store/$name" ]] || dirs+=("$store/$name")
        done
        next=''
        if ((${#dirs[@]})); then
            next="$(comm -13 <(printf '%s\n' "$found") <(zon_hashes "${dirs[@]}"))"
        fi
    done
    for name in $found; do
        [[ ! -d "$store/$name" ]] || printf 'package %s\n' "$name"
    done
}

# The marker goes last. A marker already there, from another run or an
# earlier fill, is kept when it shows the cache ready and replaced otherwise.
publish_marker() {
    local attempt record
    for attempt in 1 2; do
        record="$(stage "$deps")"
        staged+=("$record")
        { printf 'ghostty-sha256 %s\n' "$ghostty_sha256" && inventory; } >"$record"
        publish "$record" "$marker"
        ! ghostty_cache_ready "$deps" || return 0
        ((attempt == 2)) || rm -f "$marker"
    done
}

mkdir -p "$deps"
ready && exit 0
take_lock
ready && exit 0
check_links
remove_dead_stages "$deps" "$deps/tarballs"
if [[ ! -f "$source_dir/build.zig" ]]; then
    extract
fi
bash "$script_dir/prefetch-zig-deps.sh" "$source_dir" "$deps" "$store" >/dev/null
# Resolve the actual library build graph offline before publishing this cache.
# Optional platform packages may be unavailable, but required packages may not.
(cd "$source_dir" && zig build --system "$store" --global-cache-dir "$deps/cache" \
    -Demit-lib-vt=true -Demit-xcframework=false -Dapp-runtime=none --help >/dev/null)
publish_marker
ready || { echo "the offline Ghostty cache is not ready after filling it: $deps" >&2; exit 1; }
