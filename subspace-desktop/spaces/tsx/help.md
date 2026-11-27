# TSX Spaces guide

## Overview

TSX Spaces is an experimental Space type. A TSX Space is a native Space
written in TSX with [nsh](https://nsh.dev), tested and packaged with the
`nsh` toolchain, and installed as a signed `.nshpkg` package. The package
draws only the content pane; the navigator, toolbar and status bar stay
Subspace's own. TSX Spaces is in builds made with the `tsx-spaces` feature
(`make run-unreleased`) and starts off: turn it on in Settings or from
**Add a Space**.

## Before you start

- Get the package and the public root key that certified its signing key
  from someone you trust.
- Open Settings › TSX Spaces and paste the root key into **Trusted signing
  roots**, then save. No root is trusted by default.

## Main view

The navigator lists the installed packages under **Packages**, each with
its state: **Running**, **Ready**, **Needs grant** or **Error**. Select a
package to run it in the content pane. Only the selected package runs.

## Toolbar and filters

- **Install package** opens a file dialog for a `.nshpkg` file.
- **Reload** reads Settings and the packages again and restarts the
  selected package.
- **Remove package** deletes the selected package, its files and its saved
  data, after you confirm. If you install it again, it asks for its
  capabilities again. In these cases only the selected file is removed,
  and the confirmation says which applies:
  - **An app under another file name.** The app's saved data and grants
    are left alone.
  - **A package whose signing key no trusted root covers**, for example
    because you removed its root from **Trusted roots**. Its saved data,
    install record and grants stay, and apply again if you reinstall a
    build signed by the same key. To delete everything, trust its root
    again first, then remove the package.
  - **A signed package refused for another reason**, such as a revoked
    key, an app ID its certificate does not cover, or a newer runtime than
    this one. The confirmation names the reason; its saved data and grants
    are left alone.
  - **A damaged or unreadable file.** No other data is touched.

## Details

The status bar shows the selected package's name, version and how many of
the capabilities it declares are granted. The content pane shows the
package, or why it is not running:

- **No TSX Spaces installed**: install a package.
- **Checking packages**: signatures are being verified.
- **This package cannot run**: a check failed, named in the message. The
  checks are the trusted root, revocations, the signature, the app ID, the
  minimum runtime, the file hashes and rollback protection.
- **Review what this package asks for**: the package declares capabilities
  and none is granted yet.
- **The package could not start**: the message says why. Use **Reload**.
- **The package stopped**: it had a fatal error. Use **Reload**.

## Actions

### Install a package

1. Trust the package's root key in Settings › TSX Spaces.
2. Select **Install package** and choose the `.nshpkg` file.
3. If the package declares capabilities, open **Configure**, allow the ones
   you want and save.

A package that fails a check is not installed and nothing from it runs.
Installing a newer build of the same app replaces the older one; an older
build is refused. A build signed by another key asks for its capabilities
again and starts with its own saved data. Packages run from their verified
JS source; bytecode in a package is never loaded.

### Grant and revoke capabilities

Settings › TSX Spaces lists each capability an installed package declares,
in plain words, with a switch. Nothing is read before you allow it and
save. A grant is bound to the app, the key that signed it and the set of
capabilities it declares: a build signed by another key, or one that asks
for more, asks again. Turning a switch off and saving stops that data when
Settings closes.

The system capabilities are read-only:

| Capability | What the package receives |
| --- | --- |
| `subspace.system.metrics` | CPU overall and per core, memory, network up and down, every second |
| `subspace.system.apps` | The ten apps using the most memory: name, memory and process count, every two seconds |
| `subspace.system.facts` | OS, kernel, architecture, machine model, CPU, GPU, memory, uptime, displays, shell name and Subspace version |

No capability gives process IDs, command lines, paths, environment
variables, the host name or the user name. Readings stay in memory, and
sampling stops while no granted TSX Space is visible.

### Build a TSX Space

1. Install `nsh`, for example with `mise use nsh`.
2. Run `nsh init my-space --template ts` and write the Space in TSX. Declare
   the host names you need under `capabilities.host` in `nsh.json`.
3. Run `nsh test` to test it headless, stubbing the host calls.
4. Create a root and a release key with `nsh keygen`, certify the release
   key with `nsh certify`, and sign the package with `nsh publish`.

The repository's `tsx-spaces/system-monitor` example and
`mise run tsx-package` do these steps with a local development key, and
put the package in a zip archive with its notice and licence.

## Keyboard shortcuts

The package's view is one Tab stop: Tab moves through its controls and
then back to the Space. The package handles its own keys while it has
focus.

## Settings

Settings › TSX Spaces holds **Trusted signing roots**, a note for each
installed package, and one grant switch per declared capability. Changes
apply when you save.

## Troubleshooting

- **No signing root is trusted**: add the root key in Settings and save.
- **The package was not installed** with a signature or certificate error:
  the package was signed under a root you do not trust, or it was changed.
- **The package stopped**: select **Reload**. If it stops again, report it
  to the package's author.

## Limits

- Experimental, and only in builds with the `tsx-spaces` feature.
- Packages install from a local file. Installing from a URL, unsigned
  packages and developer mode are not available yet.
- A package draws only the content pane: no navigator rows, toolbar actions
  or inspector of its own.
- Checked on macOS. Windows and Linux are not checked.
