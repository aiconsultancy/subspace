#!/usr/bin/env bash
# Sourced by mise.terminal.toml. When `mise run terminal-deps` has made the
# shared cache ready for the Ghostty commit this checkout pins
# (scripts/ghostty-pin), point the libghostty-vt build at it so it never
# fetches over the network. Zig's own HTTP client fails behind
# TLS-intercepting proxies, which curl in `terminal-deps` handles. The cache
# can hold several commits, so only the pinned one is used: the crate's
# bindings were generated for it. A caller's GHOSTTY_SOURCE_DIR turns the cache
# off, as its packages belong to the pinned source; a caller's
# GHOSTTY_ZIG_SYSTEM_DIR is kept. A relative SUBSPACE_ZIG_DEPS_DIR turns the
# cache off too, as the build script runs in another directory, and so does
# Windows (ghostty-cache.sh). Nothing in the cache is sourced or run.
ghostty_scripts="${BASH_SOURCE[0]%/*}"
# shellcheck source-path=SCRIPTDIR/..
# shellcheck source=scripts/ghostty-cache.sh
source "$ghostty_scripts/ghostty-cache.sh"
ghostty_deps="${SUBSPACE_ZIG_DEPS_DIR:-${XDG_CACHE_HOME:-${HOME-}/.cache}/subspace/zig-deps}"
if [[ -z "${GHOSTTY_SOURCE_DIR+set}" ]] && ghostty_cache_supported &&
    ghostty_cache_read_pin "$ghostty_scripts/ghostty-pin" &&
    ghostty_cache_ready "$ghostty_deps"; then
    export GHOSTTY_SOURCE_DIR="$ghostty_deps/$ghostty_source_name"
    export GHOSTTY_ZIG_SYSTEM_DIR="${GHOSTTY_ZIG_SYSTEM_DIR:-$ghostty_deps/$ghostty_store_name}"
fi
unset ghostty_scripts ghostty_deps ghostty_rev ghostty_commit ghostty_sha256 \
    ghostty_archive_name ghostty_source_name ghostty_marker_name ghostty_store_name
unset -f ghostty_cache_read_pin ghostty_cache_absolute ghostty_cache_supported ghostty_cache_ready
