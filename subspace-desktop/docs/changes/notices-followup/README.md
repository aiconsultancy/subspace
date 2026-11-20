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

The release helper redacts credentials in Cargo configuration diagnostics and
checks the static ONNX archive in Cargo's reported link path against a
target-specific SHA-256. The build lock stays held while the helper stops the
Cargo process group or Windows job and copies the executable. Operators can
set `SUBSPACE_RELEASE_LOCK_TIMEOUT` to allow for cold builds. Publication
fixtures cover interruption, rollback failure and retained recovery copies.
