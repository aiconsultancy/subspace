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
command to reproduce them locally. Preflight rejects caller-supplied ONNX
linkage overrides from the environment and Cargo configuration. Packaging
verifies the archive against the pinned ort-sys `2.0.0-rc.15` distribution
table, extracts it into a fresh private directory under the build lock and
checks that Cargo links from that directory. The fixtures reject tampered
archives and check that a tampered extracted cache entry cannot affect the
private copy. The shipped ONNX Runtime notice records the exact Eigen source.

On POSIX, the helper retains the group leader until it sees Cargo's result.
It signals a failed or interrupted build group before reaping its leader.
Compiler-cache daemons can survive a successful build. Operators can set
`SUBSPACE_RELEASE_LOCK_TIMEOUT` to allow for cold builds.

Both packagers require the asset notices before building. Staging follows
the native notice target list, so the Windows archive carries WebView2
notices and the macOS and Linux archives carry ONNX Runtime notices. The
licence check compares the copy of the Markdown notice with the parser
that Cargo resolves. Its fixtures cover a drifted desktop patch, the Intel
macOS skip when cargo-about is unavailable and packaging's fatal error when
the tool is missing.

Publication rolls back on SIGTERM and SIGHUP where available. The fixtures
cover both signals, rollback failure and retained recovery copies. The
release helper and its fixtures run on Python 3.9.
