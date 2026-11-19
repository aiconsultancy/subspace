#!/usr/bin/env bash
# Packages every crate and builds each from its packaged copy against the
# others' packaged copies. Usage: scripts/package-workspace.sh [cargo args]
#
# Cargo verifies against a local registry of the packaged crates and treats a
# registry crate of a known version as unchanged: it reuses the source it
# extracted under CARGO_HOME and the build it compiled. Every crate keeps one
# version between releases, so a reused copy is stale after any change. The
# verification therefore builds in its own folder, target/package-build, and
# this script clears this workspace's crates from it and from the extracted
# sources first; dependencies stay built.
#
# Every checkout that uses this CARGO_HOME extracts into it, so one run at a
# time holds a lock there: a second run would delete the sources the first is
# compiling.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
build="$root/target/package-build"
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
exec 9>"$cargo_home/.subspace-package.lock"
python3 -c '
import fcntl, sys
try:
    fcntl.flock(9, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    print("Waiting for another checkout to finish packaging", file=sys.stderr, flush=True)
    fcntl.flock(9, fcntl.LOCK_EX)
'
rm -rf "$cargo_home"/registry/src/-*/subspace-*
# The build cache (mbx) makes `target` a link into its own storage and prunes
# that storage when disk is short. Cargo cannot create folders through the
# dangling link, so restore the folder it points at.
if [ -L "$root/target" ] && [ ! -e "$root/target" ]; then
    mkdir -p "$(readlink "$root/target")"
fi
if [ -d "$build" ]; then
    find "$build" -name '*subspace[-_]*' -prune -exec rm -rf {} +
fi
cd "$root"
# The lock stays with this script: a process Cargo leaves running must not
# keep it.
cargo package --workspace --allow-dirty --locked --config "build.build-dir=\"$build\"" "$@" 9>&-
