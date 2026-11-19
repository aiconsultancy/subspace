# Harbor: a white-label app (archived spike)

**Status: archived proof of concept.** Built on the spike branch
`spike/framework-split` against `840b963`'s layout and kept here for
reference when white-labelling resumes. It is not part of `make check`, the
architecture check or any release, and nothing keeps it building. Expect it to
drift as the Framework changes.

Harbor is a second app built from the Subspace Framework. It proves that an
app with another name can be composed from the Framework's crates and its own
Spaces without changing the Framework or `products/`. It is a proof of
concept from the [framework split research](../../../docs/changes/framework-split/research.md),
not a product.

This folder stands in for a separate repository: it is its own Cargo
workspace with its own lockfile, outside the Subspace workspace.

| Path | What it is |
| --- | --- |
| `Cargo.toml` | The workspace. Its `[workspace.dependencies]` list every Framework crate Harbor uses, by path. |
| `app/` | The `harbor` binary: a product descriptor, two `BoardRegistration`s and one logo, in `src/main.rs`. |
| `spaces/hello/` | Harbor's own Space: the four items `Dashboard` requires, a view and one agent tool. |

Harbor also registers Subspace's Component gallery Space unchanged, beside
its own.

## Build it by hand

From `subspace-desktop/`:

```sh
cargo run --manifest-path .agent/spikes/framework-split-harbor/Cargo.toml -- --version
cargo run --manifest-path .agent/spikes/framework-split-harbor/Cargo.toml -- hello greet --name Ada
cargo run --manifest-path .agent/spikes/framework-split-harbor/Cargo.toml
```

The first two run without a window; the last opens it. On the spike branch a
`mise run white-label` task also ran clippy and the tests and checked that
those commands touch no `subspace` folder. That task is not on `main`. The
window was never checked: chat, caches and other state still use Subspace's
folders there, as the research report's white-labelling findings list.

## What an app must supply today

Cargo applies these only from the workspace being built, so Harbor repeats
them from Subspace's `Cargo.toml`:

- `[patch.crates-io] markdown`, the vendored parser fix. Without it the
  Framework builds against the unpatched crate.
- `panic = "unwind"` in each profile, which that fix needs, and the
  optimisation overrides that make GPUI usable in a debug build.
- The exact `gpui-kit` version the Framework uses, because its types are part
  of the Space interface.

## What would change with published crates

Each `path` in `[workspace.dependencies]` becomes a version requirement or a
pinned git dependency. Nothing in `app/` or `spaces/hello/` changes. The
research report lists what blocks publishing and what still shows Subspace's
name inside a white-label app.
