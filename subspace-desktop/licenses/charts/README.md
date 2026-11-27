# Chart licence notices

`subspace-charts` embeds one third-party JavaScript file and links one
JavaScript engine.

- **flint-chart 0.5.1** (MIT, Microsoft Corporation). The published
  `dist/chartjs/index.js` is vendored unchanged at
  the Subspace Framework's `services/charts/vendor/flint-chart-0.5.1/chartjs.js`
  and compiled into the binary with `include_str!`. Its notice is the
  copy beside it and in [`LICENSE-FLINT-CHART`](LICENSE-FLINT-CHART).
  The Framework's `scripts/refresh-flint.sh` replaces both files from npm; it checks the
  tarball against the registry's `dist.integrity` and is run by hand, not
  by the build.
- **QuickJS** is built from the source bundled in
  [`quickjs-jit-sys` 0.14.2](https://crates.io/crates/quickjs-jit-sys/0.14.2),
  nsh's fork of `rquickjs`, which TSX Spaces also links: one QuickJS in
  the binary. [LICENSE-QUICKJS](LICENSE-QUICKJS) comes from
  `quickjs/LICENSE` in that crate. [LICENSE-QUICKJS-JIT](LICENSE-QUICKJS-JIT)
  comes from [`quickjs-jit` 0.14.2](https://crates.io/crates/quickjs-jit/0.14.2).

The Flint guide in the Subspace Framework's `services/charts/src/guide.md` is adapted from the same
project's MIT-licensed `flint-chart-author` agent skill and names its source
at the top.
