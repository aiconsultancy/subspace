# Third-party notices

Vite collects licences from the gallery's JavaScript chunks. The build
adds the Tailwind CSS and Vite preload helper licences from the same
lockfile-pinned installation. The package tests compare the shipped texts
and version headings against the installed files and lockfile, check that
rerunning the notice script preserves its output, and verify the copied
Nerd Font glyph notices.

Native releases stage the project licence and notice, the desktop licence
tree and the existing Actions Octicons attribution. The desktop tree includes
the texts for the embedded Feather, Lucide and Lobe Icons assets, the GPUI Kit
themes, QuickJS NG and the Markdown parser. The root NOTICE also records the
pinned GPUI Kit version for the vendored skills and guides.

`bash scripts/test-prerelease-products.sh` compares the notices against
their source files after extracting the Linux and macOS tarballs, Windows
ZIP and macOS app ZIP. It uses fixture binaries and signing commands.
Verification also covers the gallery build, its tests, Web Space package
validation, the desktop documentation checks and the public repository guard.
