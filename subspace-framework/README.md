# Subspace Framework

Build desktop dashboard apps in Rust with [GPUI Kit](https://crates.io/crates/gpui-kit).
An app is a thin binary that composes the Framework's shell with a chosen set
of **Spaces** (the code also calls them boards): dashboard views that
implement `Dashboard`. The Framework gives every Space a switcher tab,
Settings, in-app help, item and full-text search, notifications, agent tools
for chat, MCP and a CLI, updates, issue reports, themes and the shared UI
components.

Apps depend on one crate, `subspace-framework`. It re-exports the
Framework's crates as modules named after them without the `subspace_`
prefix (`subspace_framework::dashboard` is `subspace-dashboard`). The
`subspace-*` crates behind it are published with it; depend on the facade,
not on them.

Status: early (0.1). The API will change between minor versions.

## Features

| Feature | Modules | For |
| --- | --- | --- |
| `space` (default) | `core`, `agent_model`, `agent_tools`, `charts`, `contracts`, `search`, `web`, `github_auth`, `account`, `dashboard`, `lucide`, `client_github` | Spaces and clients |
| `agent` | `agent` | Spaces that run the chat runtime directly |
| `host` | `shell`, `board_command`, `chat`, `mcp`, `search_ui`, `update`, `entra_auth`, `workos_auth` (implies `space` and `agent`) | App binaries only |
| `report-issue` | `report_issue`, and **Report issue** in the shell (implies `host`) | Apps that take issue reports into their GitHub repository |
| `search-onnx` | `search_onnx` | Local ONNX reranking for full-text search |
| `component-gallery` | `component_gallery` | Apps that include the component catalogue Space |
| `test-support` | `client_test_support`, plus each enabled crate's test helpers | Dev-dependencies only |
| `capture` | the shell's screenshot capture build (implies `host`) | Debug capture builds |
| `gpui-kit-text-patch` | Selectable find-in-page matches and Markdown link context actions (implies `space`) | App workspaces using the vendored `gpui-base` 0.9.1 patch |

Enabling `gpui-kit-text-patch` with the crates.io copy of `gpui-base` fails
to compile because it lacks the required text APIs. To enable the feature,
copy this repository's [patched crate](vendor/gpui-base/SUBSPACE-PATCH.md)
and add the override to your workspace root manifest:

```toml
[patch.crates-io]
gpui-base = { path = "vendor/gpui-base" }
```

Leave the feature off in docs.rs builds. Reserve `--all-features` for
workspaces with the override.

`product` (the GPUI-free product descriptor types) is always present, so
`default-features = false` gives a fast build for packaging helpers.

## A minimal Space

A Space is a GPUI view that implements `Dashboard`:

```rust
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement as _, Render,
    Window, div,
};
use subspace_framework::dashboard::{Dashboard, DashboardEvent};

pub struct Hello {
    focus: FocusHandle,
}

impl EventEmitter<DashboardEvent> for Hello {}

impl Focusable for Hello {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Hello {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("Hello from a Space")
    }
}

impl Dashboard for Hello {
    const ID: &'static str = "hello";
    const TITLE: &'static str = "Hello";

    fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self { focus: cx.focus_handle() }
    }

    fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
}
```

## A minimal app

The app's binary enables `host` (here `report-issue`, which adds Report
issue), describes the product, names its GitHub OAuth App and issue
repository, and registers its Spaces:

```rust,no_run
# use gpui_kit::{App, Context, EventEmitter, FocusHandle, Focusable, IntoElement, Render, Window, div};
# use subspace_framework::dashboard::{Dashboard, DashboardEvent};
# struct Hello { focus: FocusHandle }
# impl EventEmitter<DashboardEvent> for Hello {}
# impl Focusable for Hello { fn focus_handle(&self, _: &App) -> FocusHandle { self.focus.clone() } }
# impl Render for Hello { fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement { div() } }
# impl Dashboard for Hello {
#     const ID: &'static str = "hello";
#     const TITLE: &'static str = "Hello";
#     fn new(_: &mut Window, cx: &mut Context<Self>) -> Self { Self { focus: cx.focus_handle() } }
#     fn reload(&mut self, _: &mut Window, _: &mut Context<Self>) {}
# }
use std::process::ExitCode;

use subspace_framework::product::{BoardDescriptor, BuildInfo, BuildKind, ProductDescriptor};
use subspace_framework::github_auth::OAuthClient;
use subspace_framework::shell::{BoardRegistration, ProductBuild, launch};

fn main() -> ExitCode {
    let descriptor = ProductDescriptor {
        id: "hello".into(),
        binary_name: "hello".into(),
        display_name: "Hello".into(),
        bundle_id: "com.example.hello".into(),
        release_repository: "example-org/hello".into(),
        icon_paths: Vec::new(),
        credits: Vec::new(),
        themes: Vec::new(),
        default_theme: None,
        boards: vec![BoardDescriptor {
            id: "hello".into(),
            aliases: Vec::new(),
            default_enabled: true,
        }],
        update: None,
    };
    let build = ProductBuild {
        // A native account from an identity provider; see Accounts below.
        account: None,
        descriptor,
        // Prefixes window titles and About, such as an organisation's name.
        display_name_prefix: None,
        version: env!("CARGO_PKG_VERSION"),
        // Stamp commit, branch and time from your build script; see BuildInfo.
        build: BuildInfo {
            kind: BuildKind::Release,
            ..BuildInfo::UNSTAMPED
        },
        search_reranker: None,
        default_theme_override: None,
        configured_boards: None,
        settings_extensions: &[],
        dock_panel: None,
        // Your OAuth App's client id, and its secret from the build
        // environment (never the source). `None` offers no Sign in with
        // GitHub; tokens then come from the environment, the config file and
        // the gh CLI.
        github_oauth: OAuthClient::new(
            "your-oauth-app-client-id",
            option_env!("HELLO_GITHUB_OAUTH_CLIENT_SECRET"),
        ),
        // Font files to embed, such as
        // `BundledFont::new("Inter", include_bytes!("fonts/Inter-Regular.ttf"))`;
        // see Fonts in docs/appearance.md. `&[]` embeds none.
        fonts: &[],
        // The repository Report issue files into; `None` hides Report issue.
        report_issue_repository: Some("example-org/hello"),
    };
    launch(
        build,
        vec![BoardRegistration::new::<Hello>()],
        &[],
        subspace_framework::board_command::run,
    )
}
```

## Requirements for an app

```toml
[dependencies]
subspace-framework = { version = "0.1.0", features = ["report-issue"] }
gpui-kit = "0.9.1"

# Required: Cargo applies no [patch] from a dependency.
[patch.crates-io]
markdown = { path = "vendor/markdown" }

[profile.dev]
panic = "unwind"

[profile.release]
panic = "unwind"

# GPUI is slow unoptimised.
[profile.dev.package]
markdown = { opt-level = 3 }
resvg = { opt-level = 3 }
rustybuzz = { opt-level = 3 }
taffy = { opt-level = 3 }
ttf-parser = { opt-level = 3 }
gpui-pre = { opt-level = 3 }
gpui-pre-platform = { opt-level = 3 }
```

- **The Markdown patch.** The Framework renders Markdown with `markdown`
  1.0.0, which panics on some input. Until upstream releases a fix, every app
  must patch `markdown` to the fixed copy: take the `vendor/markdown` folder
  from the Framework's source (its `PATCHED.md` describes the change) into
  the app's repository and point `[patch.crates-io]` at it. The Framework
  guards its own calls, but only the patch removes the panics.
- **Panics unwind.** The shell catches panics in views and background work,
  so `panic = "abort"` is not supported.
- **GPUI Kit.** Use `gpui-kit` 0.9.1, the version the Framework is built
  with; GPUI Kit pins its matching GPUI. Enable the tree-sitter grammar
  features your code views need.
- **Optional text patch.** The default Framework build works with registry
  GPUI Kit. An app can enable `gpui-kit-text-patch` and add the
  [vendored `gpui-base` 0.9.1 patch](vendor/gpui-base/SUBSPACE-PATCH.md) to
  its root `[patch.crates-io]` table. This keeps plain text selectable while
  showing find-in-page matches, reveals the matching line, and adds **Copy
  link** to the Markdown text menu using `TextView::on_link_click` on
  secondary clicks. The patch also fixes stale selection after a missed
  mouse release.
- **Rust** 1.99 or newer (edition 2024).
- **Build-time downloads.** With `host` or `agent`, `github-copilot-sdk`
  downloads the Copilot CLI it bundles; set `COPILOT_SKIP_CLI_DOWNLOAD=1` to
  build without it (chat then needs a `copilot` on `PATH`). With
  `search-onnx`, `ort` downloads the ONNX Runtime; set `ORT_LIB_LOCATION` to
  a local build, or `ORT_SKIP_DOWNLOAD=1`, for offline builds.
- **Platforms.** macOS (Apple silicon and Intel), Linux (X11 and Wayland)
  and Windows. On Linux GPUI needs `gcc g++ clang libfontconfig-dev
  libwayland-dev libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev
  libvulkan-dev` (Ubuntu names). `search-onnx` is not built on Windows.
- **Product identity.** Your descriptor names the app, its bundle id, Spaces,
  themes and update source; your build script stamps `BuildInfo`; your
  `ProductBuild` names your GitHub OAuth App (`github_oauth`) and, with
  `report-issue`, the repository Report issue files into
  (`report_issue_repository`). The Framework has neither: read them from
  your build script's environment, not in the Framework. Some text and the
  `SUBSPACE_*` environment variables still say Subspace; see
  [what publishing exposes](docs/publishing-audit.md).
- **Fonts.** The Framework embeds no font files. An app lists the ones it
  embeds in `ProductBuild::fonts`, with their licences in its own notices;
  see [embedded fonts](docs/appearance.md#embedded-fonts).
- **Start-up failures.** An app that checks its own settings before
  `shell::launch` and finds one it cannot use calls
  `shell::report_startup_failure(app_name, title, detail)`: it prints the
  problem and shows one small window with it and **Quit**, so someone who
  opened the app from the Finder, the Dock or the Start menu sees why it did
  not start.

## Accounts

An app can add one native account, from any identity provider, with
`ProductBuild::account`. An `AccountRegistration` names a function that
builds an `Arc<dyn subspace_framework::account::Provider>` (its argument is
the product's bundle id, a stable keychain namespace) and, optionally, a
function that receives an `AccountTokenSource` before Spaces start. The shell
then shows **Settings > Account** with sign-in, switch, retry and sign-out,
restores the session silently at startup and renews it in the background.
Trusted native adapters (an API client, or a Space crate an app adds) read
the current account with `shell::account_session(cx)` and ask it for
resource tokens off the UI thread. Web pages and configuration never see a
token.

An app that cannot be used without its account sets
`AccountRegistration::required`. While there is no session, at start-up and
whenever a session ends for good, the shell then covers the main window with
a sign-in dialog that cannot be dismissed into the app: the provider's
sign-in, its progress, and Quit. While it shows, only Quit acts: no other
shell action, from a key, the menu bar or a toolbar control, reaches the
window behind it; Tab cycles its own controls; it closes Settings, the
palette and any dialog that was open; and a notification clicked meanwhile
opens once signed in. A failed or cancelled sign-in never lets anyone in.
It closes once signed in, and focus returns to the Space. A provider that
signs in by itself (`ProviderInfo::browser` false) never shows it, and a
saved session the shell cannot renew because the service, the network or the
keychain is out of reach still opens the app. Without the flag, or without
an account, nothing changes.

The Framework ships two providers, both public clients with no secret,
using the system browser, PKCE and a loopback redirect:
`entra_auth::SessionManager` (Microsoft Entra; tenant, client, authority and
API scopes from `entra_auth::Config`) and `workos_auth::SessionManager`
(WorkOS AuthKit; client ID and API origin from `workos_auth::Config`). The
WorkOS provider can use device authorisation instead
(`Config::with_device_authorization`): the browser shows a code the person
confirms, Settings › Account shows the same code, and no loopback redirect is
needed. An app can implement `Provider` for anything else, such as a local
development issuer. Nothing in the Framework names a tenant, client or API.

More: the [documentation index](docs/README.md), starting with
[What the Framework gives a board](docs/board-integration.md),
[publishing](docs/publishing.md) and
[what publishing exposes](docs/publishing-audit.md).

## Working on the Framework

This folder is its own Cargo workspace. `mise install` installs the pinned
tools; `make` lists the targets.

## Layout

```text
src/                         subspace-framework       the facade crate
services/core                subspace-core            config file, credentials, HTTP agent, state file, PATH lookup (no GPUI)
services/product             subspace-product         product descriptors, build info, aliases, paths, metadata (no GPUI)
services/agent-model         subspace-agent-model     neutral chat context, session-summary and image-draft records
services/agent-tools         subspace-agent-tools     the agent tool interface boards advertise (no GPUI)
services/charts              subspace-charts          chart model, Chart.js conversion and the embedded Flint compiler
services/contracts           subspace-contracts       board-to-board contracts
services/github-auth         subspace-github-auth     Sign in with GitHub (no GPUI)
services/account             subspace-account         provider-neutral native account sessions and the loopback sign-in callback (no GPUI)
services/auth-return         subspace-auth-return     the browser page a loopback sign-in returns to
services/entra-auth          subspace-entra-auth      Microsoft Entra sign-in for a public client (no GPUI)
services/workos-auth         subspace-workos-auth     WorkOS AuthKit sign-in for a public client (no GPUI)
services/search              subspace-search          scoped persistent SQLite FTS5 search
services/search-onnx         subspace-search-onnx     optional local ONNX reranking
services/web                 subspace-web             bounded web research
board-kit/dashboard          subspace-dashboard       the Dashboard trait, shared widgets and design tokens
board-kit/lucide             subspace-lucide          shared interface icons
runtimes/agent               subspace-agent           Copilot, Pi and ACP chat runtimes
host/app                     subspace-shell           the shared window, toolbar, switcher, palette, help, Settings and themes
host/board-command           subspace-board-command   headless board commands
host/chat                    subspace-chat            the app-wide chat panel
host/mcp                     subspace-mcp             the MCP server for the boards' agent tools
host/report-issue            subspace-report-issue    issue reports with explicit diagnostics consent
host/search-ui               subspace-search-ui       the full-text Search view
host/update                  subspace-update          release checks and installs
clients/github               subspace-client-github   GitHub REST and GraphQL
clients/test-support         subspace-client-test-support offline HTTP fixtures for client tests
spaces/component-gallery     subspace-component-gallery-board the catalogue Space: every component in its states
vendor/markdown              (patch)                  markdown 1.0.0 with the panic fix
```

Dependencies point one way between the layers; [AGENTS.md](AGENTS.md#structure)
has the rules and `make check` enforces them.

## The dashboard interface

`board-kit/dashboard/src/lib.rs` defines `Dashboard`. A dashboard is
a GPUI view that also
implements `EventEmitter<DashboardEvent>` and:

| Member | Purpose |
| --- | --- |
| `ID`, `TITLE` | Identifier and switcher label, such as `github` and `PRs` |
| `new` | Create the view; the app creates each dashboard once |
| `bind_keys` | Bind the dashboard's keys in `view_keys(Self::ID)` |
| `activate`, `deactivate` | The dashboard became visible, or another did |
| `window_title` | The window title, such as the board's current scope |
| `render_toolbar` | Controls after the switcher, such as filters |
| `status`, `busy` | Toolbar text, and the reload button's busy mark |
| `docks`, `toggle_dock` | The workspace docks the Space has and whether each shows, for the status bar's switches, and a switch's click |
| `badge` | An optional mark on the switcher tab, such as a count of running work |
| `subject` | The selected row, shown above the palette's commands |
| `commands` | Palette commands: an action, a callback or a sub-list |
| `search`, `open_search_result` | Item search for `/`, and revealing a result |
| `modal`, `dismiss` | An open menu or dialog, and closing it |
| `keys`, `help_foot` | Rows and a note for the key help |
| `reload` | Load the data again (`r`, Cmd+R) |

A dashboard talks to the shell with `DashboardEvent`: `Toast` (info or
error, with an optional https link), `OpenPalette`, `Opened` (for the
recent items) and `Chat` (a `ChatRequest` the Copilot chat pane answers:
list the saved sessions, or show one in a chat tab). It reads the shell's
choices from globals: `Wrap`, `Density` (through `Tokens::current`),
`Services` (the config and the credential sources) and
`chat_sessions::sessions` (the chat pane's saved sessions).

### Shared widgets

The [UI component catalogue](docs/ui-components.md) maps components, styles and
their owners. Define reusable button, tab, input and dialog treatments in
`board-kit/dashboard`; boards compose them rather than add local style variants.
Use the shared native-tab treatment instead of styling buttons as tabs.
Document control exceptions in the catalogue. Direct GPUI Kit layout and event
handling do not need wrappers.

New tabs and action buttons should have an appropriate glyph by default.
Give every control an accessible label. Glyphs are decorative unless they
communicate status that is not present in the label.

| Module | What it gives a dashboard |
| --- | --- |
| `list` | `ListState` and `list::render`: grouped rows, folding, selection that scrolls into view, row indent levels, the detail panel, optional right-aligned `GroupIndicator` marks; `ListDashboard` hooks `on_selection_changed`, `on_open`, `on_panel_toggled`, `on_activate`, `group_indicators`, and `row_leading`, `row_trailing` and `group_leading` for domain marks and lanes; `list::update` for changes made outside the list's keys |
| `panel` | The detail panel frame: type and id, previous and next, service link, resizable edge |
| `workspace` | The layout every Space uses (see the [design guide](docs/design-guide.md)): `Workspace` with a `Navigator` (`nav_section`, `NavRow`), a `Pane` with a tab strip and a `Toolbar`, and an `Inspector` (`section`, `fact`, a footer for actions); `Workspace::detail` for the shared detail panel, `pane_head` for a list drawn with `list::render_under`, `text_edge` and `READING_WIDTH_PX` |
| `menu` | A popover menu: filter as you type, arrows, Enter, digits 1–9, Escape |
| `composer` | Markdown input with Write and Preview: initial text, submit label, optional text, size limit |
| `chat_input` | Reusable message input with Enter submission, busy/error state and bounded clipboard capture; shared by IRC and the host chat panel. IRC uses its plain-text and recall-key options |
| `chat_sessions` | Published saved-session snapshots with open/busy tab state; read by Copilot Agents without a runtime dependency |
| `action` | Intent-styled native buttons; `ActionState::apply` adds busy, success or failure feedback, and `ActionState::label` selects matching text |
| `row_menu` | The right-click menu of a row, a group header and the detail panel: what a row offers (`RowMenu`), which entries that earns, and the GPUI Kit `PopupMenu` it becomes |
| `context_text` | Labelled plain text for an attached chat reference: `ContextText` lines and blocks, the 16 KB cap with its truncation marker, a shared `Detail` rendered as text, and the reference for selected text |
| `truncated` | One line of text with an ellipsis, and the whole text in a tooltip when the row cuts it; `truncated::styled` keeps a highlighted title's own styling |
| `chart` | `Chart`: a native chart for a `subspace_charts::ChartModel` - bar (stacked, grouped, horizontal), line, area, pie, doughnut and scatter, with axes, thinned category labels, a legend and a hover tooltip. It takes a fixed `height` or `fill`s the height its parent flex container has left. Series colours come from the theme tokens unless the model fixes one |
| `measure` | A probe that reports the width an element was given, for views that lay out by the room they have |
| `rich` and `html` | Rich text: Markdown, and service HTML turned into Markdown, with the link and image rules |
| `code` | Tree-sitter syntax colours (One Dark Pro) for code views |
| `feedback` | Skeleton, empty and selectable error states with glyph-bearing Retry or caller-supplied recovery actions, and the bar button |
| `links` | Open a safe URL in the browser, copy to the clipboard |
| `prefs` | Stored choices under `<dashboard>.<name>`, and cache files beside the state file |
| `dates` | Times as every dashboard shows them: a row's compact age, a panel's relative phrase, a full local stamp |
| `alerts` | The task alert sound and the macOS notification, with their stored settings |
| `tokens`, `density`, `typography` | Theme colours, `Tone`, exact service colours, density values and heading font |
| `logger`, `log_view` | `logger::<Self>()` gives a board its `subspace_core::log::Logger`, scoped by its id; `log_view` is the Framework's log viewer the shell opens with Cmd/Ctrl+Shift+L (see [Logging](docs/logging.md)) |

Dashboards with grouped rows and a detail panel implement `ListDashboard`
and draw with `list::render`. A dashboard with another layout draws its own
view with the same widgets.

## Appearance

The bundled themes and the theme designer share independent heading, body and
code font choices. See [Themes and fonts](docs/appearance.md).

## Checks

`mise install` installs the pinned tools; `make` lists the targets.

| Task | Command |
| --- | --- |
| Build every crate | `make build` |
| Tests | `make test` |
| Everything that must pass | `make check` |
| Format the sources | `make format` |
| Quality score against the baseline | `mise run quality` |
| Dependency advisories | `mise run audit` |
| Verify the packages online before a release | `make publish-check` |
| Publish (see [publishing](docs/publishing.md)) | `make publish-dry-run`, `make publish` |
| Release a new version | Merge the Release Please pull request (see [publishing](docs/publishing.md)) |

`make check` runs the architecture check, the documentation and Space guide
checks, the format check, `cargo shear`, clippy with warnings denied, the
tests and the README's examples, packages every crate and builds each from
its package, checks what each package ships, and tests the publish scripts. In non-interactive shells run Cargo through mise
(`mise exec -- cargo ...`) so builds use the Mr Boxington cache.

## Licence and notices

The Framework is licensed under the Apache License 2.0 ([LICENSE](LICENSE));
every crate ships a copy. Crates that embed third-party material ship a
`NOTICE` naming it and its licence: `subspace-charts` (Flint),
`subspace-shell` (themes), `subspace-chat` (chat engine marks),
`subspace-lucide` and `subspace-component-gallery-board` (Lucide).
`make check` checks that each ships.


`services/charts` embeds the published Chart.js build of `flint-chart` 0.5.1
(MIT) and links QuickJS through `rquickjs` (MIT), so an agent-authored chart
compiles with no Node.js, no install and no network. See
[the notices](licenses/charts/README.md); `scripts/refresh-flint.sh` is the
maintainer tool that replaces the vendored build.

Each board's `assets/` directory holds its service marks. Host artwork such as
the chat toolbar's Copilot mark stays in `host/app/assets/logos`.
Each file names its
source and licence in a comment: the GitHub mark comes from simple-icons
(CC0), and the GitHub Copilot mark from
[primer/octicons](https://github.com/primer/octicons) (MIT), file
`icons/copilot-16.svg`. Board glyphs are unique.
The interface icons are Lucide (ISC), bundled with GPUI Kit and exposed by
`subspace-lucide`.

On Linux, GPUI needs these packages (Ubuntu names):
`gcc g++ clang libfontconfig-dev libwayland-dev libxkbcommon-x11-dev
libx11-xcb-dev libssl-dev libzstd-dev libvulkan-dev`.
