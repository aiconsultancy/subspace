# A build with no API backend

## Intent

The Issues Space is the Subspace API's issues, and it is not being
abstracted over other trackers. What keeps the app usable without the API is
a build option, not an interface. The owner wants a third `api_backend`
value, `none`, beside `subspace` (the default) and `nett`: a build that
starts with nothing to sign in to, connects to IRC networks configured by
hand, shows Fleet Console only for a fleet whose address and token were
saved by hand, keeps its updates, and shows no surface that would call the API. The other two
builds behave exactly as before.

## Spec

### Build and launch settings

- `api_backend: "none"` is accepted by the build script and the app
  (`build/settings.rs`, `ApiBackend::None`).
- The build refuses `identity_provider`, `api_url`, `nett_api_url` and every
  `workos_`, `entra_` and `dev_` setting with one message
  (`build/branding.rs`, `validate_without_backend`). Branding
  (`organization_name`, `title_prefix`, `about_logo`) still applies.
- The launch refuses, as a NETT build refuses Subspace API's settings, any
  non-empty `SUBSPACE_API_URL`, `SUBSPACE_API_ACCESS_JWT_FILE`,
  `SUBSPACE_NETT_API_URL`, `SUBSPACE_IDENTITY_PROVIDER`, `SUBSPACE_WORKOS_*`,
  `SUBSPACE_DEV_*` or `SUBSPACE_ENTRA_*`, naming each, before anything is
  installed (`src/launch.rs`, `check_without_backend`). Empty values are not
  settings.
- Installing the launch calls `subspace_api_client::configure_no_backend()`
  (`src/launch.rs`, `install`). After it no API origin can be configured, so
  the choice cannot be reversed in the process.

### What the build leaves out, and where

| Surface | Why it needs the API | Where it is left out |
| --- | --- | --- |
| Settings › Account, the sign-in prompt and token renewal | The account signs in to the API's identity provider | No provider: `identity::registration_for` returns none, and the Framework then shows no Account page (`src/identity.rs`) |
| Issues Space | Reads the API's issues | `surfaces::spaces` and `surfaces::adjust_descriptor` (`src/surfaces.rs`) |
| Decisions Space | Reads the API's decisions | The same |
| Settings › Fleets | Lists the fleets the API assigns to initiatives | `surfaces::settings_extensions` (`src/surfaces.rs`) |
| Channels initiative picker and its status-bar hints | The API's initiatives | `Capabilities::NONE` (`clients/subspace-api/src/backend.rs`) |
| Channels fleet context (fleet groups, ship and starbase inspector, roster) | The initiatives' fleets | No initiatives, so nothing to follow |
| Channels captain notices | Route by the API's initiatives and fleets | `Capabilities::NONE`; the **Notify ship captains** switch in Settings › Channels is hidden with `has_backend()` (`spaces/irc/src/editor/view.rs`) |
| Channels chat uploads (attach, paste upload, upload tiles) | The API's upload store | `Capabilities::NONE`; `chat_uploads::references` finds none, so an `UPL-` key stays the text that was sent and never shows **Sign in to open** (`clients/subspace-api/src/chat_uploads.rs`) |
| Channels tracker commands and message actions | Write to the API through the Issues Space | Without the Issues Space they are not offered; `/assign`, `/ready` and `/decision` are ordinary IRC input instead of an error that names the API (`spaces/irc/src/tracker/flow.rs`) |
| Channels issue tiles | Resolved by the Issues Space | Without it, references stay plain, as they do with Issues turned off |
| Notifications fed by the API | Captain notices; Issues sends none | As above |
| Any API client | | `Client::new()` refuses in a build without a backend, debug fixture variables included (`clients/subspace-api/src/lib.rs`) |

Not present, so nothing to leave out:

- **The API's agent tools and MCP surface.** The compiled tool catalogue has
  no tool that calls the API: the Issues Space serves none, the Decisions
  Space none, and the Channels tools act on IRC only
  ([Spaces audit](../../spaces-audit.md#tools)). The Framework's MCP server
  serves the registered Spaces' tools and help, so leaving out Issues and
  Decisions is all it needs.
- **Search, the command palette and About.** Search and the palette list the
  registered Spaces' content and commands, so they follow the Space set.
  Neither the palette nor About has an account or API entry; the Framework's
  Settings navigation drops Account without a provider
  (`app_pages_omit_account_without_a_provider`).
- **Account networks in Channels.** Pull request 83 adds them; `main` does
  not have them yet.

### What it keeps

- Channels as plain IRC: the networks in Settings › Channels, exactly as
  against an API without fleet details.
- The GitHub, Copilot Agents and Actions Spaces, Sign in with GitHub when the
  product supplies its OAuth App, web Spaces, the terminal, Space Lab and the
  Component gallery where the build has them.
- **Fleet Console.** Today it merges three sources of fleets
  (`web-spaces/fleet-console/src/fleet-sources.js`): its own settings'
  first fleet (`endpoint` and the `token` secret in its `webBoards` entry,
  which `install.sh` writes), the fleets the API assigns to initiatives
  (`subspace.fleets.get`, from `FleetApi` over the initiative catalogue) and
  fleets added inside the page. With no API the second source is always
  empty, so the build registers the built-in Fleet Console only once its
  entry holds both an address and a token (`surfaces::shows_web_space`);
  the config file is watched, so it appears when they are saved. No new
  settings surface: the entry's settings are the ones the installer and the
  Space's own Settings page already use.
- **Updates.** The updater reads the product's release source on GitHub,
  with the GitHub credential when there is one, and never the Subspace API
  or its account, so a `none` build updates as every build does.

### No Framework change

The Framework already lets a product leave out an account
(`ProductBuild::account`), the Settings extensions and the Spaces. Every change is in the desktop: the product crate, the API client
and two lines in Channels.

### Not changed

- Channels' Help guide still describes the initiative picker, fleet groups,
  uploads and tracker commands, each under the condition that the API is
  there. It is one guide for every build.

## Delivery

- Tests:
  - `build/settings.rs`: `none` parses, and every API and sign-in variable is
    refused.
  - `build/branding.rs`: a `none` build config refuses each API and identity
    setting and keeps branding.
  - `src/launch.rs`: a `none` launch resolves no URL and no provider, and
    refuses each override.
  - `src/identity.rs`: a launch with no provider offers no account; the
    stamped build's descriptor is adjusted for its backend.
  - `src/main.rs`: each backend's Space and Settings page set, and the
    adjusted descriptor lists exactly the Spaces a `none` build registers
    and keeps the update source.
  - `src/surfaces.rs`: Fleet Console shows in a `none` build only with an
    address and a token.
  - `tests/no_backend_build.rs`: in a child process, with the Subspace API
    debug fixture variables set, the launch refuses them; a clean launch
    installs no backend, after which no origin can be configured, no client
    is built, every capability is off, upload references are plain text,
    and the shared catalogue and fleet service find nothing. A loopback API
    receives no request.
  - `scripts/test-capture.sh`: `CAPTURE_NO_API=1` gives the app no Subspace
    API variables.
- Capture: `CAPTURE_NO_API=1` leaves the Subspace API fixture variables out
  of the app's environment, so `make capture` can start a `none` build
  ([Desktop capture](../../capture.md#isolation-and-options)).
- Screenshots of a `none` build, in Twilight Two at 1440 x 900, built with
  `SUBSPACE_BUILD_CONFIG` naming `{"api_backend": "none"}` and captured with
  `CAPTURE_NO_API=1`, Channels connected to `scripts/irc-test-server.sh`
  with `IRC_TEST_CHANNELS="#design #random #releases"`. The app was given
  no Subspace API address. There is no before image: the
  build variant is new, and the Subspace build is unchanged.

| Image | Shows | Options |
| --- | --- | --- |
| [Shell](evidence/none-shell.png) | PRs, Channels, Actions and Fleet Console; no Issues or Decisions. Channels is plain IRC on the hand-configured network, with no initiative picker | `CAPTURE_FLEET_API=1` (Fleet Console's address and token saved) |
| [Shell without a fleet](evidence/none-shell-no-fleet.png) | The same without Fleet Console, whose address and token are not saved | |
| [Settings](evidence/none-settings.png) | Application has no Account and no Fleets; Spaces lists no Issues or Decisions | `CAPTURE_FLEET_API=1`, `OpenSettings` |
| [Settings › Channels](evidence/none-settings-channels.png) | The hand-configured network | `CAPTURE_FLEET_API=1`, then Channels |
| [Settings › Channels, end](evidence/none-settings-channels-end.png) | History, then agent setup: no **Notify ship captains** | Scrolled to the end |

## Tasks

- [x] `api_backend: "none"` in the build and launch settings.
- [x] Leave out the Spaces, Settings pages and Channels surfaces that need
      the API.
- [x] Fleet Console from its own saved settings.
- [x] Tests, README section and this record.
- [ ] When pull request 83 lands, check that account networks stay off in
      a `none` build (no client and no capability) and that Settings ›
      Channels shows nothing of them.
