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
Apache-2.0 fallbacks. Each release build uses a private Cargo target directory
until its executable and licence report are staged together.

The packaging fixtures cover copied notices, missing inputs, native
dependency drift, distinct Cargo sources, incomplete fallback diagnostics,
Framework licence substitution and overlapping builds with different
features.
