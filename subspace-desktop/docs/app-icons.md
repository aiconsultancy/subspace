# Application icons

Run `mise install` from this folder before `make check-icons` or
`make generate-icons`. `mise.toml` pins ImageMagick 7.1.2_31 through
[`conda:imagemagick`](https://mise.jdx.dev/dev-tools/backends/conda.html), the
backend listed by `mise registry imagemagick`. Mise installs the package and
its dependencies without a separate conda installation. The icon targets
run through `mise exec` so they find the pinned `magick` in non-interactive
shells too.

`make check-icons` validates the committed icons' signatures and dimensions.
`make generate-icons` rebuilds them from the canonical SVG and additionally
requires `rsvg-convert` (for example, `brew install librsvg`) and macOS's
`iconutil`; run generation on macOS. Neither target requires a global
ImageMagick installation.
