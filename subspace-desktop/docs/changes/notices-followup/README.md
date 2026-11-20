# Release notice follow-up

The reviews of the third-party notices and Rust licence reporting changes
identified gaps in the packaging checks and fixtures. The gallery test now
recreates the appended notices and checks that a second run preserves them.

Release archives use the desktop NOTICE, whose paths are relative to the
archive's licence files. Packaging checks its licence inputs before staging.
The ONNX Runtime distribution table, the WebView2 static loader and their
notices are checked against pins for the linked crates.

Cargo package IDs distinguish crates with the same name and version. A
generic licence text requires evidence for each linked crate that uses it.
Local Framework crates use the resolved Framework project's LICENSE for
Apache-2.0 fallbacks. The release helper stages Cargo's executable with its
licence report under a lock. Ordinary builds reuse a target and feature
cache. Builds with an OAuth client secret use temporary Cargo directories.

The packaging fixtures cover copied notices, missing inputs, native
dependency drift, distinct Cargo sources, incomplete fallback diagnostics,
Framework licence substitution and overlapping builds with different
features. Follow-up fixtures cover surviving Cargo descendants, timed lock
contention, build-stamp refresh, OAuth artefact cleanup, Cargo configuration
errors, build-only ONNX overrides and rollback after a failed checksum rename.

The release helper suppresses Cargo configuration diagnostics and gives a
command to reproduce them locally. Preflight rejects ONNX linkage overrides
from the environment and Cargo configuration, and Cargo's build output must
confirm the pinned static download. The notices still pin ort-sys
`2.0.0-rc.15`, ONNX Runtime `1.31.0` and the bytes of the distribution table.
In that crate, `build/download/dist.tsv` pins each download's SHA-256.
`build/main.rs` lines 149–161 compare the downloaded archive with that pin
before publishing the extracted files to the digest-keyed cache and
emitting static link instructions at lines 182–183. The crate's verification
is the archive integrity check, so packaging no longer hashes an extracted
file after linking.

The build lock stays held while the helper stops the Cargo process group or
Windows job and copies the executable. A detached compiler-cache daemon such
as sccache writes to its own cache. The build's outputs are copied under the
lock after the build group exits, so no extra process tracking is needed for
those daemons. Operators can set `SUBSPACE_RELEASE_LOCK_TIMEOUT` to allow
for cold builds.

Publication rolls back on SIGTERM and SIGHUP where available. The fixtures
cover both signals, rollback failure and retained recovery copies. The
release helper and its fixtures run on Python 3.9.
