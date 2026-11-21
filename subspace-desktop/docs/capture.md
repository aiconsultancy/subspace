# Isolated desktop capture

Use `make capture` from either repository directory to capture a debug build
with fictional accounts and loopback fixture APIs. The app captures its own
windows to PNG and then exits. See [UI change evidence](../AGENTS.md#ui-change-evidence)
for required states, themes and screenshot sizes.

```bash
make capture OUT=dist/capture STEPS='wait:5000;shot:main'
```

macOS builds a temporary `.app` bundle and launches it with `open -g -n -W`,
so capture does not take the foreground. Linux builds and launches the native
executable in a private D-Bus session and a private authenticated Xvfb display.
This also works from a Wayland desktop: capture uses its own X server and
never reads the desktop session's windows or input. Each app window is raised
and repainted on that private display before its pixels are read, so overlapping
app windows are captured independently.

## Linux prerequisites

Install the repository's pinned mise toolchain and the native build
prerequisites before capturing. The runtime needs `dbus-run-session`,
`dbus-daemon`, system fonts, `xvfb-run`, `Xvfb`, `xauth` and a Vulkan driver. Mesa lavapipe supplies software
Vulkan when no GPU is available.
On Ubuntu 24.04 these runtime packages are `dbus-daemon`, `fonts-dejavu-core`,
`libvulkan1`, `mesa-vulkan-drivers`, `xvfb` and `xauth`.

An X server alone does not provide the GPU API. If GPUI reports no suitable
adapter or cannot initialise its renderer, check that the Vulkan loader and
a usable driver are installed.

## Isolation and options

The app starts under `env -i` with a fresh HOME, separate XDG directories,
fictional `octo-dev` accounts and a fixture `gh`. Its GitHub and Subspace API
URLs point to temporary loopback servers. A private file contains the fixture
bearer token. Caller tokens, proxy settings, tool paths and D-Bus session
addresses are not inherited. Linux also discards the caller's X11 and Wayland
connection variables and creates its own runtime directory and X authentication
file.

Linux writes `state.json` under the fixture's `.local/state/subspace/` and
`config.json` under `.config/subspace/`. macOS keeps both under the fixture's
`Library/Application Support/subspace/`. The fixture directories, servers and
private display are removed when capture exits.

`CAPTURE_THEME` selects a theme; without it the build starts in Cupertino
Dark, the app's default. Screenshots for pull requests and the README are
taken in Twilight Two (`CAPTURE_THEME="Twilight Two"`), as
[UI change evidence](../AGENTS.md#ui-change-evidence) says.
`CAPTURE_STATE` and `CAPTURE_CONFIG` merge JSON keys into the fixture files. `CAPTURE_IRC_CONFIG` copies a configuration for a
local IRC fixture. `CAPTURE_NO_UPDATES_STREAM=1` starts the Subspace API fixture
without its live updates stream. `CAPTURE_CHANGE_AFTER=<seconds>` retitles
SUB-1 that long after the first detail read to simulate a change made
elsewhere. `CAPTURE_KEEP_LOG=1` keeps the app and API fixture logs in
the output directory for diagnosis. A failed launch or render fails the
command; inspect retained logs before using the evidence.

`CAPTURE_FEATURES=<features>` adds Cargo features of the app to the capture
build, for evidence of a Space that the default build leaves out. Without it
the capture build is unchanged and shows neither unreleased Space. A Space
built in this way still starts off, so turn it on with `CAPTURE_CONFIG`, for
example `CAPTURE_FEATURES=space-lab
CAPTURE_CONFIG='{"boards":{"space-lab":true}}'` on macOS ARM64.

The step syntax is listed in [AGENTS.md](../AGENTS.md#ui-change-evidence).
Capture uses the same steps and fixture data on both platforms. Window
decorations and text rasterisation can differ between operating systems.
macOS reads the GPUI-rendered image. The pinned GPUI version does not implement
native Linux image readback, so Linux reads pixels from the app's exact native
X11 window after presenting its frame. It does not capture the entire display.

The app embeds Inter and JetBrains Mono and registers them before it applies
the theme, so screenshots in Twilight Two use those fonts on both platforms
whether or not the machine has them installed, and start without a font
notice. Themes that name no font still use the machine's own fonts. See
[Embedded fonts](../../subspace-framework/docs/appearance.md#embedded-fonts).

## Decisions fixture

Open **SUB-1** to capture three fictional decisions and their entries in the
**All** activity timeline. They have different decided-on dates and list
newest decided first, as Subspace API orders them. **Use one review queue**
links to its superseded decision, **Keep separate review queues**, and to
issue **SUB-4**. Open **SUB-2** for an issue with no decisions.

Set `CAPTURE_DECISION_SCENARIO=missing` to leave the superseded decision
listed while its detail returns 404. Use `forbidden` for 403 or
`unavailable` for 503 on decision reads. The default is `normal`. These
modes use the same local fixture and leave other issue reads available.

## Move fixture

Moving **SUB-1** to **OPS** moves it with its sub-issues **SUB-2** and
**SUB-3**. Subspace API confirms every move it commits, so the fixture never
gives what a NETT move can: a committed move with sub-issues it could not
confirm in the destination. Set `CAPTURE_MOVE_UNCONFIRMED=1` to report a
committed move's sub-issues that way, so the move panel lists them before
the moved issue opens. The move itself happens on the fixture; the reasons
are invented: the first sub-issue was moved again since and the others
could not be read back. Only a debug capture build against the loopback
fixture reads the setting.

## Upload fixture

Subspace API keeps no upload transfers, and adding a file needs the system
file picker, so a standalone upload that NETT left unresolved cannot be made
in a capture. Set `CAPTURE_UPLOAD_SCENARIO` to show one, invented, as
`fixture-checklist.txt` in the **Attachments** panel of the first issue
opened, such as **SUB-1**:

- `unknown`: its bytes went to a reserved transfer and the reply was lost,
  so it waits for **Check**. Checks against the fixture cannot tell whether
  it was stored either, so after three, the panel offers to send it again.
- `absent`: kept while its issue was closed, its bytes were let go, and a
  check then found it was not stored.

Everything after the seeded upload is the app's own handling. Only a debug
capture build against the loopback fixture reads the setting.

## File-drop evidence

Use `drop:<x>,<y>,<fixture-path>` in `STEPS` to dispatch a native file-enter/submit pair at a window position. Supply only a synthetic fixture file; it is read by the isolated capture build and uploaded only to the loopback fixture API. This supports file previews without using the system clipboard or foreground pointer.

The debug capture step `account:<state>` seeds a fictional account UI.
States are `unavailable`, `signed-out`, `waiting`, `switching`, `completing`,
`signed-in`, `reconnecting`, `offline`, `expired`, `keychain` and `denied`. It performs no sign-in or keychain
lookup. All capture builds disable real account restoration, whatever identity
provider the build config selects. The one exception is the local development
provider (`dev`), which holds no real account: it signs in to a loopback API
with an in-memory token, so a capture can show the local stack's signed-in
state. Fixture captures set `SUBSPACE_IDENTITY_PROVIDER=` and use the fixture
bearer token instead.
