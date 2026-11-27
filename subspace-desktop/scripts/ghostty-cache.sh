#!/usr/bin/env bash
# Sourced by ghostty-offline-env.sh and terminal-deps.sh: the pinned Ghostty
# build, the names it has in the shared offline cache, and when the cache
# holds it. Shell builtins only, and safe under `set -eu`, because mise
# sources the environment script before every command run with
# MISE_ENV=terminal.

# Read scripts/ghostty-pin into ghostty_rev, ghostty_commit and ghostty_sha256,
# and set the cache's names for that pin. Fails, without ending a `set -e`
# shell, when the file is missing or a value is absent or malformed.
ghostty_cache_read_pin() {
    local key value
    ghostty_rev='' ghostty_commit='' ghostty_sha256=''
    ghostty_archive_name='' ghostty_source_name='' ghostty_marker_name='' ghostty_store_name=''
    [[ -f "$1" && -r "$1" ]] || return 1
    while read -r key value || [[ -n "${key-}" ]]; do
        case "${key-}" in
        '' | '#'*) ;;
        libghostty-rs) ghostty_rev="${value-}" ;;
        ghostty) ghostty_commit="${value-}" ;;
        ghostty-sha256) ghostty_sha256="${value-}" ;;
        *) return 1 ;;
        esac
    done <"$1"
    [[ "$ghostty_rev" =~ ^[0-9a-f]{40}$ && "$ghostty_commit" =~ ^[0-9a-f]{40}$ &&
        "$ghostty_sha256" =~ ^[0-9a-f]{64}$ ]] || return 1
    # Everything a pin produces is named by its commit and archive digest, so
    # checkouts pinning different digests of one commit never share a path.
    # Only terminal-deps publishes under these names: the archive and the
    # tree only from bytes with this digest, so the names are their
    # provenance. The package store is shared by every pin, as each package
    # is named by its Zig hash, and only terminal-deps has written it.
    # shellcheck disable=SC2034 # terminal-deps.sh reads it.
    ghostty_archive_name="ghostty-$ghostty_commit-$ghostty_sha256.tar.gz"
    ghostty_source_name="ghostty-$ghostty_commit-$ghostty_sha256"
    ghostty_marker_name=".complete-$ghostty_commit-$ghostty_sha256"
    ghostty_store_name="zig-packages"
}

# Whether PATH is absolute. A relative cache would name another directory in
# the build script, which runs elsewhere.
ghostty_cache_absolute() {
    [[ "$1" == /* ]]
}

# The cache is off on Windows: Git Bash's /c/... paths would reach the
# native build script unconverted, and nothing here is tested there.
ghostty_cache_supported() {
    [[ "${OSTYPE-}" != msys* && "${OSTYPE-}" != cygwin* ]]
}

# Whether the cache DEPS, an absolute path, is ready for the pin read above:
# terminal-deps wrote the marker last, after extracting the source from the
# verified archive and filling the package store. The marker names the
# archive digest on its first line, then one package directory per line;
# every package must still exist. Any other line means not ready.
ghostty_cache_ready() {
    local key value header='' store="$1/$ghostty_store_name"
    ghostty_cache_absolute "$1" && [[ -f "$1/$ghostty_marker_name" &&
        -f "$1/$ghostty_source_name/build.zig" && -d "$store" ]] || return 1
    while read -r key value || [[ -n "${key-}" ]]; do
        case "${key-}" in
        ghostty-sha256)
            [[ -z "$header" && "${value-}" == "$ghostty_sha256" ]] || return 1
            header=1
            ;;
        package)
            [[ -n "$header" && "${value-}" =~ ^[A-Za-z0-9._-]+$ && "$value" != . &&
                "$value" != .. && -d "$store/$value" ]] || return 1
            ;;
        *) return 1 ;;
        esac
    done <"$1/$ghostty_marker_name"
    [[ -n "$header" ]]
}
