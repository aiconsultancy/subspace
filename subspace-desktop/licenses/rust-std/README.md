# Rust standard library notices

Release packaging copies the following files unchanged into this directory
from `share/doc/rust/` in the release compiler's sysroot.

- `COPYRIGHT-library.html`
- `licenses/Apache-2.0.txt`
- `licenses/MIT.txt`

The rustup `rustc` component supplies all three files. The packager checks
that the compiler matches the version pinned in `subspace-desktop/mise.toml`
and that the files are readable before compiling.
