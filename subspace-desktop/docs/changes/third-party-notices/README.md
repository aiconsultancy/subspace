# Third-party notices

Vite collects licences from the gallery's JavaScript chunks. The build
adds the Tailwind CSS and Vite preload helper licences from the same
lockfile-pinned installation. The package tests compare the shipped texts
and version headings against the installed files and lockfile, check that
rerunning the notice script preserves its output, and verify the copied
Nerd Font glyph notices. The pinned Codicons 0.0.45 licence credits
Jason Long's Git logo under CC BY 3.0. The Font Awesome 6.5.1 notice
identifies SIL OFL 1.1 for the bundled font glyphs and credits Fonticons
with the 2023 copyright.

Native releases stage the project licence and notice, the desktop licence
tree, the combined Rust crate report and the existing Actions Octicons
attribution. The desktop tree includes the texts for the embedded Feather,
Lucide and Lobe Icons assets, the GPUI Kit themes, QuickJS NG and the Markdown
parser. The root NOTICE also records the pinned GPUI Kit version for the
vendored skills and guides.

Native archives include Microsoft ONNX Runtime 1.31.0's licence and
third-party notices. Packaging copies the Rust standard library licences
and copyright notice from the release compiler's sysroot and checks the
files and pinned compiler version before compiling. The Windows ZIP also
includes the licence and notice from the WebView2 SDK 1.0.3650.58 NuGet
archive. The Rust crate report uses
the versioned sources in `about-notices.json` for licence files missed by
cargo-about, highlights remaining generic texts and links to each MPL-2.0
crate's source archive.
An invalid cargo-about clarification, a changed pinned source file or a
linked generic text without a matching fallback diagnostic stops packaging
before compiling the release binary.

The fallback audit checked the locked releases across macOS, Linux and
Windows. The versioned entries identify upstream revisions for omitted
files and checksums for files read directly from published crates. Where
the pinned sources provide attribution only, the report retains it
alongside the generic text. This covers recent objc2 releases, Pathfinder
Geometry and the Zune crates. The pinned mac and SeaHash sources have
no standalone licence text. The policy graph also contains dwrote, whose
pinned source has an MPL header. The linked release graphs carry the
MPL-2.0 source link for option-ext.

`bash scripts/test-prerelease-products.sh` compares the notices against
their source files, including a fixture sysroot, and checks the generated
Rust report after extracting the Linux and macOS tarballs, Windows ZIP and
macOS app ZIP. It uses fixture binaries and signing commands.
`bash scripts/test-package-products.sh` checks that a missing sysroot
notice or a compiler version mismatch stops packaging before Cargo runs.
`make licence-check` checks the allowed SPDX expressions and byte-compares
the copied notices against the Framework checkout Cargo actually selected.
Packaging uses Cargo's normal dependency tree to limit the report to the
selected target and features and omit compile-time proc macros.
Packaging also checks each linked crate in a merged canonical SPDX text group
for its own source text or a recognised fallback diagnostic. For ONNX Runtime,
the preflight checks Cargo's configuration from the build's working directory,
with its environment and configuration arguments. Packaging also checks that
the ort-sys build script selected the pinned static download.

Release builds reuse `release-pkg/` within Cargo's target directory, keyed by
target and feature set. Each run refreshes the build stamp. Builds with a
GitHub OAuth client secret use a private Cargo directory removed after staging.
A lock covers the build and the copy of Cargo's reported executable into the
staging directory with its licence report. A separate lock serialises writers
of each archive and checksum in `dist/`. Lock waits time out after 30 minutes.
The checksum is briefly withdrawn during replacement, and a failed rename
restores the previous pair.
The packaging fixtures exercise cache reuse, concurrent builds and writers,
Cargo configuration overrides and merged licence provenance.
Verification also covers the gallery build, its tests, Web Space package
validation, the desktop documentation checks and the public repository guard.
