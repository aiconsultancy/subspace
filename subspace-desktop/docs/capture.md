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

`CAPTURE_THEME` selects a theme. The fixture always writes both files, so
a capture is an existing install: without `CAPTURE_THEME` it has no saved
theme and starts in Cupertino Dark, the app's default. Screenshots for pull
requests and the README are taken in Twilight Two
(`CAPTURE_THEME="Twilight Two"`), as
[UI change evidence](../AGENTS.md#ui-change-evidence) says.
`CAPTURE_FIRST_RUN=1` writes neither file, as at a new install's first
start, which opens in Twilight Two; it refuses `CAPTURE_THEME`,
`CAPTURE_STATE`, `CAPTURE_CONFIG`, `CAPTURE_IRC_CONFIG` and
`CAPTURE_FLEET_API`. With no saved window bounds the window opens at its
default size, so add a `resize:<width>,<height>` step, such as
`resize:1440,900`, before the shot to capture it at another size.
`CAPTURE_STATE` and `CAPTURE_CONFIG` merge JSON keys into the fixture files. `CAPTURE_IRC_CONFIG` copies a configuration for a
local IRC fixture. `CAPTURE_FLEET_API=1` starts the Stargate Fleet API fixture
(see [Fleet fixture](#fleet-fixture)) and points the built-in Fleet Console at
it. `CAPTURE_FLEET=1` adds the fleet's issues to the Subspace API fixture. `CAPTURE_NO_UPDATES_STREAM=1` starts the Subspace API fixture
without its live updates stream. `CAPTURE_CHANGE_AFTER=<seconds>` retitles
SUB-1 that long after the first detail read to simulate a change made
elsewhere. `CAPTURE_ITEM_DELAY=<seconds>` holds each GitHub
issue or pull request detail read that long, so a Channels pull request
tile can be captured while it loads. `CAPTURE_KEEP_LOG=1` keeps the app and API fixture logs in
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

## Fleet fixture

`CAPTURE_FLEET_API=1` runs `scripts/fleet_api_test_server.py` on loopback: a
fictional Stargate Fleet API v1 serving the demo world's `aurora` fleet (host
`aurora-vm-01`, Stargate 0.43.0) with the routes, fields and event stream of
Stargate's `src/api/v1/types.ts`. The rear admiral `aurora-admiral` runs four
ships, Lyra, Orion, Vega and Castor, with 16 crew: nine busy, five tasks
queued, one uncertain, idle and not yet provisioned crew, branches, pull
requests, subagents and 24 hours of outcomes. Every few seconds a crew member
moves to the next phase, opens a pull request, finishes or picks up queued
work, so the stream stays live. The capture writes its address and the
fictional token `fixture-fleet` into `webBoards.fleet-console`, and the app
adds its built-in Fleet Console to that entry at start, with its permissions
allowed, so Fleet Console shows the fleet live, not in sample mode.

Crew work names issues of the demo world's projects `NAV`, `MAP`, `WARP` and
`RELAY`. `CAPTURE_FLEET=1` adds those projects to the Subspace API fixture as
a busy team's tracker: 58 issues in every workflow state, trackers with their
sub-issues, labels, priorities, estimates, due dates, people and captains as
assignees, linked pull requests and comments, and 13 decisions in every status
with supersession chains, made by people and by captains. The issues the
crew works on have the same numbers, titles and states, so Fleet Console's
issue tiles resolve through the Issues Space. It also adds the neutral `FLT`
project. The GitHub fixture's pull requests and Actions runs include the same
work. The repository README's desktop screenshots use both options:

```bash
CAPTURE_THEME="Twilight Two" CAPTURE_FLEET_API=1 CAPTURE_FLEET=1 \
  CAPTURE_STATE='{"app.dashboard":"issues","app.window":"[40,40,1440,900]"}' \
  make capture OUT=dist/capture STEPS='wait:6000;shot:issues'
```

The README's Fleet Console image is the fleet tier, live:

```bash
CAPTURE_THEME="Twilight Two" CAPTURE_FLEET_API=1 CAPTURE_FLEET=1 \
  CAPTURE_STATE='{"app.dashboard":"fleet-console","app.window":"[40,40,1440,900]"}' \
  make capture OUT=dist/capture STEPS='wait:9000;shot:fleet-console'
```

For the Channels image, start `scripts/irc-test-server.sh`, then
`scripts/irc-history-fixture.py`, which posts a fictional `#aurora-lyra`
conversation between people and captains that names fixture issues and links
the GitHub fixture's pull requests, and stays connected so the server keeps
it as chat history. Pass `CAPTURE_IRC_CONFIG` a config for that server with
`#aurora-lyra` in its channels, with `CAPTURE_FLEET=1` so the issue tiles
resolve. `CAPTURE_FLEET=1` also lists the aurora fleet's initiative,
never selected, so Channels marks its captains and rear admiral on any
loopback connection. `--status` gives `aurora-lyra` a nick status, and
`--late-after <s>` posts three more lines that many seconds after the
capture build joins, so a capture that shows another buffer meanwhile finds
them new; restart the server and the fixture before each capture.

Run `python3 scripts/fleet_api_test_server.py --port 18484` to serve the same
fleet for an installed copy of the app, and `--snapshot` to print it as one
JSON document. Like Stargate, the fixture answers only the app's own web
Space origins, so a Fleet Console opened in a browser cannot read it.

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
