# System monitor TSX Space

A TSX Space, written with [Native Shell](https://nsh.dev) (nsh), that shows
live CPU (overall and per core), memory and network graphs, the top apps by
memory and a facts panel. App ID: `dev.subspace.system-monitor`.

It reads only the host names in its `nsh.json`:
`subspace.system.metrics`, `subspace.system.apps` and
`subspace.system.facts`. Subspace supplies them; see the
[TSX Spaces specification](../../docs/changes/tsx-spaces/spec.md) §5 and §6.
Each tier the Space is not granted shows an explained empty state.

## Commands

Run these from `subspace-desktop/`. mise installs the pinned `nsh`.

- `mise run tsx-space-test` type checks the package and runs its headless
  tests, with the host calls stubbed.
- `mise run tsx-package` builds, signs and verifies
  `dist/tsx-spaces/dev.subspace.system-monitor-<build>.nshpkg` with a local
  development key in `.local/tsx-keys/`. Never commit those keys.

## Theme

The package uses only theme token classes. The host maps the Subspace
theme to them and sets `text-edge` to the design guide's text edge;
`theme.json` holds the fallback for `nsh run`.

## Licences

See [NOTICE](NOTICE) and [licenses/](licenses/).
