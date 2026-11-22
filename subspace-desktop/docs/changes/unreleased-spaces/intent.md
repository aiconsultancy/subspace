# Intent: leave Board Lab and the Component gallery out of releases

> Board Lab has since been renamed Space Lab; see [Space Lab rename](../space-lab-rename/intent.md).

Board Lab (`spaces/board-lab`, board id `board-lab`) and the Framework's
Component gallery (board id `component-gallery`) are not ready to ship. The
owner's decision: disable them for now, so that they are not built or in the
release version, and so that they default to off.

What that means:

- **Not built by default.** Each Space becomes a Cargo feature of the
  Subspace App (`products/subspace`), and neither feature is in `default`.
  With a feature off, that Space's crate is not compiled into the app, and
  the Space is not registered, not listed in the product descriptor, and
  absent from Settings, the title bar's **Add a Space** menu, the command
  palette, the app's MCP and CLI tools, the headless interface and Help.
  Release and packaging builds use the default features, so every release
  artefact leaves both out.
- **Off by default when built in.** In a build with a feature on, the Space
  exists but starts off in every build profile, debug included. A developer
  turns it on in Settings or from **Add a Space**.
- **Kept alive.** The gated code must not rot: lint and tests cover the app
  with both Spaces built in and in its release configuration, and developers
  have one obvious way to run the app with them.

Board Lab is to be renamed Space Lab in a separate change; this one keeps
every existing name, id, crate and tool name.

See the [spec](spec.md), [delivery](delivery.md) and [tasks](tasks.md).
