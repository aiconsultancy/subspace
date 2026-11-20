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
- **QuickJS NG** is built from the source bundled in
  [`rquickjs-sys` 0.14.0](https://crates.io/crates/rquickjs-sys/0.14.0).
  [LICENSE-QUICKJS-NG](LICENSE-QUICKJS-NG) comes from `quickjs/LICENSE` in
  that crate. [LICENSE-RQUICKJS](LICENSE-RQUICKJS) comes from
  [`rquickjs` 0.14.0](https://crates.io/crates/rquickjs/0.14.0).

The Flint guide in the Subspace Framework's `services/charts/src/guide.md` is adapted from the same
project's MIT-licensed `flint-chart-author` agent skill and names its source
at the top.
