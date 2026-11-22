# Web Spaces

A web Space is a web app packaged with a `subspace-board.json` manifest beside
its `index.html`. Subspace's Web Space shows it in a native web view, serves
its files without a web server, and lets it read its own settings, show a tab
badge, share context with chat and send notifications through a
permission-controlled bridge. The manifest keeps the older name
`subspace-board.json` for compatibility.

This folder holds web Space packages. They are not Rust crates. Fleet Console
is built into the Subspace App: the build embeds its package files, and the app
adds it to the tab strip at start (see
[its README](fleet-console/README.md#in-subspace)). The examples are installed
by users at run time.

| Path | What it is |
| --- | --- |
| [`fleet-console/`](fleet-console/README.md) | Fleet Console, built into the app: your Stargate fleets drawn with WebGL2, drilling from fleets to fleet, ship and crew, fed by Stargate's Fleet API; sample fleets without setup |
| [`examples/weather/`](examples/weather/README.md) | Weather: a keyless UK-postcode forecast, plain HTML, CSS and JavaScript with no build step and no permissions |

## Try Weather

In Subspace, open **Settings > Application > Web Spaces**, then either:

- **From GitHub.** Choose **Install Weather example…**. It fills in the
  **GitHub repository** form: repository `aiconsultancy/subspace`, board
  directory `subspace-desktop/web-spaces/examples/weather` and a blank ref
  (the default branch). Choose **Validate source**, then **Install now**. This
  needs a GitHub account in Settings that belongs to the `aiconsultancy`
  organization.
- **From a local folder.** Choose **Add board…**, set **Source type** to
  **Local folder** and choose `subspace-desktop/web-spaces/examples/weather`
  in a checkout of this repository.
- **From a ZIP.** Package it (below), then choose **Add board…** with
  **Source type** **Local ZIP** and the ZIP's path.

## Test and package

From `subspace-desktop/`:

```sh
mise run weather-space-test                              # Weather's offline Node tests (part of make check)
mise run fleet-console-space-test                        # Fleet Console's offline Node tests (part of make check)
make package-web-space SPACE=web-spaces/examples/weather # writes dist/web-spaces/weather-1.0.0.zip
make package-web-space SPACE=web-spaces/fleet-console    # writes dist/web-spaces/fleet-console-1.0.0.zip
```

`scripts/package-web-space.py` validates a web Space folder with the same
rules as the app's installer and writes a reproducible ZIP without tests and
development files. Its tests are `scripts/test-package-web-space.py`, run by
`mise run script-test`.

## Make your own

- [Build and package a web Space](../docs/web-spaces-guide.md): the
  step-by-step guide, from an empty folder to a published package.
- [Web Space manifest, packages and bridge reference](../docs/web-spaces-reference.md):
  every manifest field, install source, package rule, bridge method and error.
- [Web Spaces in-app guide](../spaces/web/help.md): what users see.
- The `web-space` agent skill:
  [.agents/skills/web-space](../.agents/skills/web-space/SKILL.md).

Put a new example in `examples/<name>/` with its own `README.md`, tests and,
if it has tests, a mise task like `weather-space-test` added to `make check`.
