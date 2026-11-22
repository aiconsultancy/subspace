<picture>
  <source media="(prefers-color-scheme: dark)" srcset="products/subspace/assets/product-dark.svg">
  <img src="products/subspace/assets/product-light.svg" alt="Subspace logo" width="112">
</picture>

# Subspace

## Quick start

For the desktop app, start with the
[Subspace download-and-copy guide](docs/subspace/index.md#quick-start).
The app checks for updates at launch by default. To check manually, open
**About > Check for updates**. Checks do not install updates automatically.

[Other ways to install](docs/installation.md#other-ways-to-install)

## Developer overview

A desktop dashboard app built with Rust and
[GPUI Kit](https://github.com/longbridge/gpui-kit). It opens in the Cupertino Dark
theme, with Space tabs in the title bar and a ⌘K command palette. Every Space
uses the same workspace shell: a navigator on the left, a tab strip and
toolbar above the content, an inspector on the right and the status bar
(see the [design guide](../subspace-framework/docs/design-guide.md)).

The reusable crates are the **Subspace Framework**. An **App** is a binary
built from the Framework with a selected set of boards. The **Subspace App**
(`subspace`, bundle `Subspace.app`) is the app.
See [the glossary](docs/glossary.md). The
[documentation index](docs/README.md) lists every developer and product page
by area.

Status: prototype. This workspace is maintained under `subspace-desktop`
in `aiconsultancy/subspace`, beside the Framework in `subspace-framework`
([The Framework](#the-framework)). The Subspace App includes GitHub, the default-off Copilot Agents board, a
prototype [Channels Space](docs/irc.md) (IRC), the [Issues](docs/issues.md)
and [Decisions](docs/decisions.md) Spaces for Subspace API, and the default-off
[Actions board](docs/actions.md). Two Spaces are not ready to ship and are
left out of the default and release builds: the
[Space Lab alpha](docs/space-lab.md) (macOS ARM64 only) and the developer
[Component gallery](../subspace-framework/spaces/component-gallery/help.md), which shows the shared UI
controls in their states, with fixture data. Build them in with the
product's `space-lab` and `component-gallery` features, or both with
`unreleased-spaces` (`make run-unreleased`); they start off there.
The standalone Space Lab developer harnesses are retired; the integrated board
and its [historical design evidence](docs/agents/space-lab/README.md) remain.
Debug builds dock a [terminal](docs/terminal.md) beside every Space, as they
do the chat, through the optional `terminal` feature, on macOS, Linux and, as
an experiment, Windows (ConPTY). Each dashboard reads its service
directly: GitHub reads and writes pull requests and issues. The Issues board
uses [Subspace API](docs/issues.md): lists, the full issue view, editing,
creation, comments, images, attachments, linked pull requests and live updates
over server-sent events. Channels follows the selected initiative's channels
and, when the initiative belongs to another fleet, that fleet's IRC server.

## Board guides

Each guide is the same file shown in the app's Help dock. Open a board, then
choose **Help** in **Commands**, or press `?` while board navigation has focus.
Guides cover prerequisites, controls, actions, keyboard shortcuts, Settings,
troubleshooting and limits. They identify untested live behavior and omitted
screenshots.

| Board | Guide |
| --- | --- |
| PRs | [GitHub pull requests](spaces/github/help.md) |
| Copilot Agents | [Copilot agent tasks](spaces/github-copilot-agents/help.md) |
| Channels | [IRC networks, channels and private messages](spaces/irc/help.md) |
| Issues | [Subspace API issues and comments](spaces/issues/help.md) |
| Decisions | [Subspace API decisions](spaces/decisions/help.md) |
| Space Lab | [Native dashboards and managed environments](spaces/space-lab/help.md) |
| Actions | [GitHub Actions](spaces/actions/help.md) |
| Component gallery | [Shared UI components for developers](../subspace-framework/spaces/component-gallery/help.md) |
| Fleet Console | [Fleet Console web Space](web-spaces/fleet-console/README.md) |
| Web Spaces | [Websites and installed web Spaces](spaces/web/help.md) |

Availability depends on the platform, build features and enabled boards. Configured web Spaces share the Web Spaces guide.

## Identity providers and the API URL

The Issues board, the Channels initiative picker and captain notifications
talk to [Subspace API](docs/issues.md). The app signs in to it with one
account from the identity provider its build selects; **Settings > Account**
signs in, switches account, retries and signs out. The provider, the API URL
and optional branding come from `products/subspace/build-config.json`, or
from the JSON file `SUBSPACE_BUILD_CONFIG` names when you build. The
same-named environment variables override the provider settings when the app
starts, which is handy against a local stack.

| Build config field | Run-time variable | Meaning |
| --- | --- | --- |
| `identity_provider` | `SUBSPACE_IDENTITY_PROVIDER` | `dev`, `workos`, `entra`, or empty for no sign-in |
| `api_url` | `SUBSPACE_API_URL` | The API, such as `https://api.example.com`. HTTP only on loopback; `/api` is added when missing |
| `dev_email`, `dev_name`, `dev_organisation` | `SUBSPACE_DEV_EMAIL`, `SUBSPACE_DEV_NAME`, `SUBSPACE_DEV_ORGANISATION` | Who `dev` signs in as |
| `workos_client_id` | `SUBSPACE_WORKOS_CLIENT_ID` | The WorkOS AuthKit client ID, such as `client_0000000000` |
| `workos_api_url` | `SUBSPACE_WORKOS_API_URL` | Optional; defaults to `https://api.workos.com` |
| `workos_redirect_port` | `SUBSPACE_WORKOS_REDIRECT_PORT` | Optional fixed loopback port for the redirect URI |
| `entra_tenant_id`, `entra_client_id` | `SUBSPACE_ENTRA_TENANT_ID`, `SUBSPACE_ENTRA_CLIENT_ID` | The tenant and its public-client registration, such as `00000000-0000-0000-0000-000000000000` |
| `entra_authority` | `SUBSPACE_ENTRA_AUTHORITY` | Optional; defaults to `https://login.microsoftonline.com` |
| `entra_api_scope` | `SUBSPACE_ENTRA_API_SCOPE` | The API's delegated scope, such as `api://your-api-client-id/API.Access` |
| `organization_name`, `title_prefix`, `about_logo` | | Optional account label, window title prefix and About logo (SVG) |

- **`dev`** (the checked-in default) signs in to a local API's development
  issuer (`POST /api/v1/auth/dev/token`) with no external identity provider,
  keeps the token in memory and signs in again when the app starts. It only
  accepts a loopback API, such as the default `http://127.0.0.1:3000`.
- **`workos`** opens WorkOS AuthKit in the system browser with PKCE and a
  loopback redirect. Register `http://127.0.0.1:*/callback` in WorkOS for
  a dynamically assigned port, or register the exact port and set
  `workos_redirect_port`. The refresh token stays in the system keychain.
- **`entra`** opens Microsoft sign-in in the system browser with PKCE and a
  loopback redirect, `http://localhost/auth/callback` (Entra ignores the
  port). Register a public client on the Mobile and desktop platform, with
  no secret, and grant it the API scope.

Example build config for a hosted API with WorkOS:

```json
{
  "identity_provider": "workos",
  "api_url": "https://api.example.com",
  "workos_client_id": "client_0000000000"
}
```

### A NETT build

Set `api_backend` to `nett` to build the app against NETT instead of Subspace
API. NETT is then the only tracker: Issues uses NETT's issue teams inside a
NETT organisation. Features NETT adds release by release, such as initiative
channels, chat uploads, standalone attachments, issue moves and live
updates, turn on when NETT lists them for the selected organisation, and
stay hidden until then. The build needs `nett_api_url` (run-time
`SUBSPACE_NETT_API_URL`), `identity_provider: "workos"` and the NETT
environment's `workos_client_id`. It signs in with WorkOS device
authorisation: the browser shows a code, and **Settings > Account** shows the
same code to check. A NETT build refuses `api_url`, `workos_redirect_port`
and the `dev_` and `entra_` settings, and will not start while any of
`SUBSPACE_API_URL`, `SUBSPACE_API_ACCESS_JWT_FILE`,
`SUBSPACE_WORKOS_REDIRECT_PORT`, `SUBSPACE_DEV_*` or `SUBSPACE_ENTRA_*` is
set. A Subspace build likewise refuses `SUBSPACE_NETT_API_URL`. Keep the file
outside the repository:

```json
{
  "api_backend": "nett",
  "nett_api_url": "https://api.nett.example",
  "identity_provider": "workos",
  "workos_client_id": "client_00000000000000000000000000"
}
```

`scripts/nett-api-test-server.sh` runs a loopback NETT mock for local work.
See the [NETT backend design](docs/changes/nett-backend/design.md) for what a
NETT build gates on NETT's capability list and what it leaves out.

Channels uses the IRC connections in **Settings > Channels**. An initiative's
channels go to the connection on its fleet's IRC server; initiatives without a
fleet use `SUBSPACE_IRC_ENDPOINT` (`irc://host:port` or `ircs://host:port`),
by default the local server, `irc://127.0.0.1:6667`. IRC sign-in is the
connection's SASL account, not the API account.

## Report a problem

Use **Copy details** beside an error or **Copy diagnostics** in About to share
local debugging context. **Report issue** previews the exact GitHub submission.
Its optional **Include paths** switch starts off for every report. See
[Report an issue](../subspace-framework/docs/report-issue.md) for the fields and privacy controls.
Reports go to `aiconsultancy/subspace`, which the product's build
configuration names; see
[Build configuration](docs/installation.md#build-configuration).

## Build and installation reference

Run `mise install` from this directory to install the pinned development
tools. This includes ImageMagick (`conda:imagemagick`, 7.1.2_31), used by
`make check-icons` to verify native icons and `make generate-icons` to rebuild
them from the canonical SVG. These targets run through mise; no global
ImageMagick installation is required. See [Developer tools](../subspace-framework/docs/developer-tools.md#application-icons)
for platform prerequisites.

Build and run from source with `make run`. On macOS this builds a fresh debug
`dist/dev/Subspace.app` with the approved icon and opens that exact bundle;
it does not sign or replace `dist/Subspace.app`. Pass app arguments with
`make run ARGS='...'`. Arguments run headless, including on macOS.
`make run-unreleased` does the same with the Spaces that are not ready to
ship, Space Lab (macOS ARM64) and the Component gallery, built in through the
product's `unreleased-spaces` feature. Both start off: turn them on in
Settings or from **Add a Space**. Release and packaging builds never include
them; capture builds include them only when `CAPTURE_FEATURES` names them
([Desktop capture](docs/capture.md)).
Use `make capture OUT=<dir>` for isolated fixture screenshots on macOS or
Linux. Linux uses the native executable on a private Xvfb display; see
[Desktop capture](docs/capture.md) for prerequisites and options.
`make package` writes a release
archive and its SHA-256 file to `dist/`, and `scripts/install-release.sh` and
`scripts/install-release.ps1` install a release for the current user. On macOS,
the release installers put the signed app in `~/Applications` and link its CLI
from `~/.local/bin` or the selected bin directory.
For a signed, notarized macOS app, use `make package-macos` with a Developer ID
Application identity and a notarization Keychain profile; `make release-macos`
also publishes it, on the GitHub release Release Please drafted. To install a local
build, use `make install-macos`. It builds `Subspace.app`, signs it ad hoc and
copies it to `~/Applications`. Quit Subspace first. The release source
is `aiconsultancy/subspace`, using `subspace-desktop-vX.Y.Z` tags.
The release-download scripts default to that repository; use
`SUBSPACE_RELEASE_REPO=OWNER/REPO` to override it. See [docs/installation.md](docs/installation.md).

No CI builds or tests the app: `make check` and `mise run quality` are the
gate, and the prek pre-push hook runs both. Release Please opens the release
pull requests ([Releases](docs/ways-of-working.md#releases)); after the app's
merges, `make release-macos` attaches the Developer ID signed, notarized and
stapled Apple silicon app, `Subspace_<version>_darwin_arm64.zip`, and its
`.sha256` file to the draft release and publishes it. See the installation
guide for local signing setup.

Debug builds, such as `make run`, include the element inspector: press ⌘F12
(Ctrl+F12 elsewhere) or choose **Developer > Inspect elements**, point at an
element to see its bounds and component name, and click to pin it. See
[Developer tools](../subspace-framework/docs/developer-tools.md). Release builds have no inspector
or Developer menu. The status bar shows the version; debug builds
also show their Git branch and UTC build timestamp. Use **About**
in the application menu or command palette to view the app and Subspace
Framework versions, the repository link, commit, branch, UTC build time, Rust
target and build profile. The CLI exposes the same
details without configuration or GUI initialization:

```bash
subspace about
```

After a download-and-copy or release-script installation, update from
**About > Check for updates**, or use `subspace upgrade`. See
[Update an installed app](docs/installation.md#update-an-installed-app).

Board selection is defined by each product's package dependencies, descriptor
and entrypoint registrations. A different board combination uses another thin
product crate; disabling boards in Settings does not remove them from a binary.

## The Framework

The app is built on the [Subspace Framework](../subspace-framework/README.md),
a separate project in `../subspace-framework` with its own workspace, checks
and docs. Every crate here depends on exactly one Framework crate, the facade
`subspace-framework`, and writes `subspace_framework::<module>::…`
(`dashboard`, `core`, `shell`, …). Spaces and clients use its default
features (or `agent`); only `products/subspace` enables `host`,
`search-onnx`, `gpui-kit-text-patch` and, for capture builds, `capture`, and
`component-gallery` only through its own `component-gallery` feature.

```toml
# Cargo.toml
subspace-framework = { path = "../subspace-framework", version = "0.2.0" }

[patch.crates-io]
gpui-base = { path = "../subspace-framework/vendor/gpui-base" }
markdown = { path = "../subspace-framework/vendor/markdown" }
```

The excluded terminal manifests use the same Framework dependency and
vendored patches. The Framework's
[publishing guide](../subspace-framework/docs/publishing.md#subspace-desktop-and-the-published-crate)
shows how to switch to a published crate, refresh the three lockfiles and
return to the source checkout. The Markdown patch stays until upstream fixes the
parser (see the
[Framework README](../subspace-framework/README.md#requirements-for-an-app)).
Integration pre-releases count Framework commits as app changes while the
path is in place.

**Changing the Framework and a Space together.** Both live in this
repository and the desktop builds the Framework by path, so a Framework edit
is in the next desktop build. Make and check the Framework change in
`../subspace-framework` (`make check` there), then the Space change here
(`make check` here); `make check` at the repository root runs both. Commit
the Framework change first, on its own.

## Layout

```text
products/subspace           subspace                       the Subspace App binary, composition, icons and build stamp
products/descriptor         subspace-descriptor            the app's product descriptor and the packaging metadata helper
spaces/github               subspace-github-board          GitHub pull requests, issues, diffs
spaces/github-copilot-agents subspace-github-copilot-agents-board GitHub Copilot cloud agent tasks and chats
spaces/actions              subspace-actions-board         experimental GitHub Actions runs, deployments, releases and job graphs
spaces/space-lab            subspace-space-lab-board       alpha native dashboards and managed runtimes (macOS ARM64; `space-lab` feature)
spaces/space-lab/crates/mxc-worker subspace-mxc-worker     MXC Seatbelt worker adapter; unsupported platforms fail closed
spaces/irc                  subspace-irc-board             IRC networks, channels and private messages ([docs](docs/irc.md))
spaces/issues               subspace-issues-board          Subspace issues, activity, comments and attachments
spaces/decisions            subspace-decisions-board       Subspace decisions, read only
spaces/web                  subspace-web-board             web pages and installed web Spaces in a native web view
clients/irc                 subspace-client-irc            IRC and IRCv3 protocol, connections and runtime
clients/subspace-api        subspace-api-client            Subspace API reads, writes, live updates and development sign-in
terminal/panel              subspace-terminal-panel        the docked terminal panel (excluded Zig workspace, feature `terminal`; [docs](docs/terminal.md))
terminal/runtime            subspace-terminal              PTY/ConPTY sessions with libghostty VT emulation (excluded Zig workspace)
terminal/ghostty-config     subspace-ghostty-config        Zig-free Ghostty configuration and theme parser
web-spaces/fleet-console    (not a crate)                  Fleet Console web Space, built into the app
web-spaces/examples/weather (not a crate)                  installable Weather web Space package
```

The terminal stays in this project, not the Framework, because
`libghostty-vt` is a git dependency, which crates.io refuses. The panel and
its runtime need Zig, so they are excluded workspaces with their own
lockfiles; the product reaches them through its `terminal` feature, and the
shell docks the panel through the Framework's `dashboard::dock_panel`
contract.

The [dependency rules](AGENTS.md#structure) apply to every internal normal,
build and dev dependency. `mise run architecture`, included in `make check`,
checks Cargo metadata for this workspace and both terminal workspaces: products
compose Spaces, Spaces reach only their own crates, desktop clients and
`subspace-framework`, nothing enables the facade's host features outside
products, and no dependency or source include leaves this folder except the
two documented Framework paths.

A dashboard crate uses its client crates. Each client crate has `model.rs`
(service types, serde names equal to the JSON names), a `Client` that holds
the credential sources, tests with recorded responses, and an
`examples/*_smoke.rs` that calls the live service. Client calls are blocking:
a dashboard runs them in `cx.background_spawn` and applies the result with
`update`.

## Settings and stored state

The Subspace App ships one web Space, [Fleet Console](web-spaces/fleet-console/README.md),
in the tab strip on macOS and Windows from the first start: the build embeds
its package files and the app adds its `webBoards.fleet-console` entry at
start. It can add other [web Spaces](spaces/web/help.md) immediately from
**Settings > Application > Web Spaces**, using URLs, built folders, ZIPs or
organization-authorized GitHub sources. It includes an installable UK-postcode
Weather example. Local apps can declare native settings and request a
[permission-controlled JavaScript bridge](docs/web-spaces-reference.md).
[Build and package a web Space](docs/web-spaces-guide.md) shows how to make one;
packages live in [web-spaces/](web-spaces/README.md).

The [full-text Search view](../subspace-framework/docs/search.md) searches opted-in retained board
content. Open it with **Cmd/Ctrl+Shift+F**. It shares its read-only query service
with Copilot Chat and MCP. BM25 works without the optional local ONNX model.

Product GUI settings and preferences are separate. Existing native Subspace
locations remain supported. A product also checks `~/.config/<binary>/config.json`
on macOS and Windows before its native location.

Board commands run headless; see [the command-line interface](../subspace-framework/docs/cli.md).
Configure optional board-command aliases in the product's `config.json`:

```json
{"commands":{"aliases":{"github":["gh"]}}}
```

Aliases must not collide with board names or other aliases. They do not add
excluded boards. The Subspace App uses prefixed commands.

Enable or disable boards on their pages in Settings. Each included board has an
Enabled checkbox, including boards that default to off. Save applies the
choice without a restart: an enabled board gets its tab, background work and
agent tools, and a disabled board loses them. Space Lab only hides its tab
while disabled, so its project workers keep running until you quit or enable
the board again. Full-text
search adds a newly enabled board after a restart. Settings cannot add boards
excluded from the product.
Enabling a default-off board writes an explicit `true`. App pages appear first,
followed by boards in an icon-led tree. Boards register nested sections and
features through `Dashboard::settings`; new included boards get an Enabled page
without a settings case in the shell.
The Settings dialog sizes itself from the window. The page body and the
navigation tree scroll independently. The title and the Save and Cancel footer
stay fixed.

Boards that read services automatically have **Background updates** controls on
their main Settings page. Choose an interval or **Default** to keep the board's
existing timings. Save applies the choice without a restart; Cancel discards
the draft. Off pauses periodic reads and that board's notification and task
monitoring, even while the board is open. Manual refresh and loads when opening
a board remain available. Existing requests may finish. The General page's automatic-refresh control remains the
master switch and does not replace individual board choices.

Use **Configure** on a board's empty or error screen to open its Settings page.
Enter service details there and select **Save**. Subspace creates or updates its
private configuration file; no file needs to be created by hand. Repository
settings are shared by GitHub, Copilot Agents and Actions, and can be edited on any of
those pages.
Save and restart Subspace to apply service changes. Blank token fields keep
saved tokens.

**Experiments** lists app and board experiments. A board's experiments also
appear below its ordinary settings, sharing the same saved toggles. Experiments
are off by default. [Features and experiments](../subspace-framework/docs/feature-flags.md) describes
registration, graduation and removal. Pi chat is the first app experiment.

You can also set board enablement in the product's `config.json` with Boolean
values keyed by ID: `{"boards":{"github":false,"irc":true}}`. An omitted
board uses that product's default for the current build. In a Subspace release
build, Channels and Issues default to disabled. They remain compiled and
you can enable them in Settings. Debug builds enable Channels and Issues
by default. GitHub and Actions default to enabled in both builds; Copilot Agents defaults to disabled.
An explicit `true` or `false`
overrides the product default. Restart to apply changes made in the file;
Settings applies them on Save. Disabled boards have
no UI or agent tools. Config cannot enable a board excluded from a product.
Actions defaults to enabled in both builds. Its Flow view needs no separate
experiment flag.
Space Lab and the Component gallery are in the binary only with the
`space-lab` and `component-gallery` features, which release builds leave off.
Built in, each defaults to disabled in both builds: enable it in Settings and
save, or choose it from **Add a Space**. Opening Space Lab does not install
runtimes or packages; provisioning requires an explicit action. Linux and
Windows do not include Space Lab, even with its feature.
Each board bundles Markdown help for the app and the
read-only `board_help` tool in agent chat/MCP.

The conversation prototype is described in [Agent chat](../subspace-framework/docs/agent-chat.md),
with [Pi setup and limits](../subspace-framework/docs/pi-chat.md) for the experimental local engine,
and the Copilot task list in [Copilot Agents dashboard](docs/github-copilot-agents.md).
Chat engine, the remembered model for new chats, and the Approve all default
live on **Settings > Chat**. A confirmed model choice is stored per engine in
`state.json` as `chat.default-model.copilot` and `chat.default-model.pi`. New
chats, including after a restart, use that model. Resumed and replay chats keep
the model on the conversation. A Chat engine change still needs a restart.
**Approve all tool requests for new chats** is off unless `chat.approve-all-default`
is exactly `1`. Only Copilot uses that default. Missing or other values stay in
the asking mode. The setting applies to new chats. It does not change an open
chat, managed policy, or service-write approval. Pi does not support Approve
all; the saved Copilot choice is kept.

Each board advertises agent tools through `Dashboard::agent_tools`. Local chat
registers them, and `subspace mcp` serves the same
tools to other harnesses over stdio or Streamable HTTP. **Settings › MCP** can
also serve them from the running app, turn tool groups off for MCP clients and
copy a setup prompt for an agent. See [MCP server](../subspace-framework/docs/mcp.md).

Subspace reads `subspace/config.json` in the platform config directory
(`~/.config` on Linux, `~/Library/Application Support` on macOS,
`%APPDATA%` on Windows). The file is optional:

```json
{
  "repos": ["owner/name"]
}
```

Credentials come from the environment first, then the config file, then
an account signed in through Subspace, then the CLI:

| Service | 1. Environment | 2. Config file | 3. Signed in | 4. CLI |
| --- | --- | --- | --- | --- |
| GitHub | `GH_TOKEN`, `GITHUB_TOKEN` | `github.token` | **Sign in with GitHub** in Settings, per account | `gh auth token` per account (`gh auth login`) |

Signed-in accounts are listed in `github-accounts.json` beside the state
file (logins only). Their tokens are in the system keychain under
`<data directory>.github`, such as `subspace.github`; debug builds keep them in
the owner-only `github-tokens.json` beside the state file instead, unless
`SUBSPACE_DEVELOPMENT_USE_KEYCHAIN` is set. The account picker lists signed-in
and `gh` accounts once each. Sign in with GitHub uses
Subspace's OAuth App, which the product's build configuration names
(`products/subspace/build/config.rs`); a build without one offers no
in-app sign-in. See
[Sign in with GitHub](docs/settings.md#sign-in-with-github).

If the config file holds a token, make it private (`chmod 600`); Subspace
ignores tokens in a file that other users can read or write. UI choices
go to `subspace/state.json` in the platform state directory; large caches
(`github-cache.json`) are files beside it. A missing
credential's message names the variable, the config key with the config
file's path, Sign in with GitHub, or the CLI command that provides it.

## Add a Space

The Framework's [developer overview](../subspace-framework/README.md#the-dashboard-interface)
describes the `Dashboard` interface and the shared widgets, and
[What the Framework gives a board](../subspace-framework/docs/board-integration.md)
every hook. Complete the
[board checklist](../subspace-framework/docs/board-checklist.md) in the
Space's change record.

1. Create `spaces/<id>` with a package named `subspace-<id>-board` that
   depends on `subspace-framework`, like `spaces/github/Cargo.toml`. If it
   uses a service that has no client yet, put its JSON types and requests in
   a new crate under `clients/`, like `clients/subspace-api`.
2. Implement `Dashboard` (and `ListDashboard`, `Focusable` and `Render`).
   `spaces/github/src/lib.rs` is a `ListDashboard` to start from.
3. Add the crate to `products/subspace/Cargo.toml`, a `BoardDescriptor` to
   `products/descriptor/src/lib.rs` and one registration to
   `products/subspace/src/main.rs`:

   ```rust
   BoardRegistration::new::<subspace_name_board::Name>(),
   ```

4. Document its tools and contract roles in the
   [Spaces audit](docs/spaces-audit.md); product tests compare both with the
   registrations.

## Keys

`Cmd` on macOS is `Ctrl` on Linux and Windows. Keys without Cmd, Ctrl or
Alt do not act while a text field, a menu or a dialog has focus.

Board tabs default to a horizontal bar. The command palette action
**Toggle vertical navigation** switches to a left rail, or back. The rail can
collapse to icons. The choice is stored in `state.json` as `app.board-navigation`
(`horizontal` or `vertical`) and `app.board-navigation-collapsed` (`1` or `0`).
Board order, badges, function keys and history stay the same. Each tab keeps its
descriptive tooltip and accessible label. A short window scrolls the rail and
keeps the collapse control.

| Key | Action |
| --- | --- |
| Cmd+K (Ctrl+Shift+K on Linux) | Command palette: commands and items |
| / | Search the dashboard's items |
| r, Cmd+R | Reload the dashboard |
| w | Wrap or truncate long titles |
| \` | Theme designer |
| ? | Show or hide the Help dock panel |
| Cmd+Shift+/ (Ctrl+Shift+/ on Windows and Linux) | Show or hide Keyboard Shortcuts. Type to filter by key or action; Esc closes it and restores focus |
| F1 to F12 | Dashboard switch keys with Secondary+1 to Secondary+9. Enabled product order. With every Subspace board on: F1 GitHub, F2 Copilot Agents, F3 Channels, F4 Issues, F5 Decisions, F6 Actions. A build with the unreleased Spaces adds the Component gallery after Actions and, on macOS ARM64, Space Lab after Decisions. Disabled boards take no slot. Unused positions do nothing. |
| Secondary+1 to Secondary+9 | Product dashboard order; unused positions do nothing. |
| Cmd+Shift+L (Ctrl+Shift+L elsewhere) | Show the log viewer |
| Cmd+Shift+C (Super+Shift+C on Linux, Ctrl+Shift+C on Windows) | Show or hide chat |
| Cmd+Shift+A (Ctrl+Shift+A elsewhere) | Add the selected row and its open details to the chat |
| Cmd+Q (Ctrl+Shift+Q on Linux) | Quit |

The command palette's **Keyboard Shortcuts** command opens the same searchable
view of the active board's shortcuts and the global shortcuts. The toolbar and
status-bar **Help** commands still open the Help dock panel.

The detail panel's title bar has the same **Add to chat** button.

A right click opens a menu, on every row, on a group header that stands for
an item, and anywhere in the detail panel. It selects the line it was opened over first, so the menu acts on what
was clicked. A row offers **Add to chat** (with its key), **Open
details**, **Open in** its service, **Copy link**, **Copy title** and **Copy
id**, for whichever of those it has. The detail panel offers **Copy** and
**Add selection to chat** when text is selected, then the same item
entries. Property values in the shared panel are selectable text, so a
value such as `State: Open` can be selected and sent.

Either way the chat pane opens if it was closed and the text arrives as a
pinned reference, which rides with the next message.

Chat tab keys, while the chat pane has focus:

| Key | Action |
| --- | --- |
| Cmd+T | New chat tab |
| Cmd+W | Close the chat tab |
| Cmd+Shift+] , Cmd+Shift+[ | Next or previous chat tab |
| Cmd+Alt+G (Ctrl+Alt+G elsewhere) | Open or close the canvas gallery; Esc closes it, ← and → step while the canvas has focus. See [Canvases](../subspace-framework/docs/agent-chat.md#canvases) |

In the Sessions tab:

| Key | Action |
| --- | --- |
| Up, Down | Move the highlight |
| Enter | Open the chat |
| F2 | Rename the chat |
| Delete | Delete the chat, after confirmation |

Each dashboard's own keys (? lists those of the active dashboard):

| Dashboard | Keys |
| --- | --- |
| GitHub | `j`/`k`, Enter, `←`/`→`, `1`–`6` sections, `s` pull requests or issues, `d` diff, `u` account, `A` approve, `R` request changes, `c` comment, `a` assignees, `i` assign me, `o` open in GitHub, `y` copy URL; in the diff: `n`/`p` files, `v` viewed, `s` split or unified |
| Copilot Agents | `j`/`k`, Enter, `1`–`3` filters (kind, state, repository), `0` clear filters, `o` open session, `p` open pull request, `y` copy link, `m` monitor, `u` show the subject, `c` open in chat |
| Terminal (docked, not a Space) | Ctrl+\` show or hide, new/close tab, next/previous tab, copy and paste; see [Terminal](docs/terminal.md) for platform-specific keys and configuration |

## Tasks

`mise` pins Rust 1.99.0 with the Mr Boxington build cache. `make` with no
target lists the targets.

| Task | Command |
| --- | --- |
| Build every crate | `make build` |
| Update compatible dependencies | `make update-deps` |
| Run the desktop app | `make run` |
| Run it with Space Lab and the Component gallery | `make run-unreleased` |
| Tests | `make test` |
| Format check, lint and tests | `make check` |
| Format the sources | `make format` |

Non-interactive shells must use `mise exec -- cargo ...` or mise shims so
Cargo goes through Mr Boxington. `prek install` installs the format and
clippy hooks from `prek.toml`. `make check`'s lint step also runs
`cargo shear` to catch unused dependencies before clippy. Clippy and the
tests run on the workspace with the unreleased Spaces built in
(`--features subspace/unreleased-spaces`), then again on the app and its
descriptor alone in the release configuration, so neither rots.

`make update-deps` refreshes `Cargo.lock` within the existing version ranges.
The Copilot SDK is pinned to `1.0.34` to keep the experimental canvas API and
its bundled CLI together and remove its dependency on vulnerable `zip` 7.
Direct `zip` dependencies require at least `9.5.1`. Keep the lockfiles for
reproducible builds and run `make check build` after an update.
GPUI Kit's matching asset pin remains deliberate.

## Asset notices

`licenses/` holds the notices the app ships, including those of the
Framework's third-party code (Flint charts, QuickJS NG, the Markdown parser,
themes and the Mermaid renderer) and the terminal's. The Framework keeps its
own copies with its crates. Native releases also carry the project `LICENSE`
and `NOTICE`.

Each Space's `assets/` directory holds its service marks. The GitHub mark
comes from Simple Icons (CC0). The Copilot Agents, Actions, Channels and Issues
icons have [licences and attribution](licenses/icons/README.md).

On Linux, GPUI needs these packages (Ubuntu names):
`gcc g++ clang libfontconfig-dev libwayland-dev libxkbcommon-x11-dev
libx11-xcb-dev libssl-dev libzstd-dev libvulkan-dev`.
