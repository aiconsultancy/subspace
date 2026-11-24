# Subspace documentation

Start here. This page links every page under `docs/`, grouped by area, with
one line on what each covers. Read the area that matches your task, then only
the pages you need. Agents: read [AGENTS.md](../AGENTS.md) and
[Ways of working](ways-of-working.md) first, then choose from the areas below.

The published product guide starts at [Subspace](subspace/index.md); this page
is the developer and agent entry point. `make check` fails when a page under
`docs/` is not linked from here.

## Ways of working

If you want to know how to commit, branch, version and release, read
[Ways of working](ways-of-working.md).

- [Ways of working](ways-of-working.md): conventional commits, branches and
  pull requests, hooks, versioning and the release steps.
- [Before opening or merging a pull request](ways-of-working.md#before-opening-or-merging-a-pull-request):
  the pull request checklist, also in the repository's pull request template.
- [AGENTS.md](../AGENTS.md): rules for agents, including terminology, commands,
  structure, conventions and UI evidence.
- [Change records](changes/README.md): the intent, spec, delivery and tasks
  recorded for each change.
- [Glossary](glossary.md): the Subspace App's terms; the Framework's terms
  (App, Space, channel, update source) are in the
  [Framework glossary](../../subspace-framework/docs/glossary.md).

## Architecture and code structure

If you want to know how the crates are laid out and which layer may depend on
which, read the [layout](../README.md#layout) and the
[structure rules](../AGENTS.md#structure).

- [Developer overview](../README.md): the Framework dependency, layout, adding
  a Space, settings, keys and tasks.
- [The Framework](../README.md#the-framework): the one crate this folder
  imports, the two documented paths into `../subspace-framework`, working on
  a Framework change and a Space change together, and switching to the
  published crate.
- [Structure rules](../AGENTS.md#structure): dependency direction, crate
  ownership and what the architecture check enforces.
- [Canonical design and coding guides](../AGENTS.md#canonical-design-and-coding-guides):
  the GPUI Kit Design and Coding Guides every UI and architecture change follows.
- [Identity providers and the API URL](../README.md#identity-providers-and-the-api-url):
  the build config and run-time variables that select `dev`, WorkOS or Entra
  sign-in and the Subspace API.
- [Issues and Subspace API](issues.md): the API connection, projects, the
  shared hub, live updates, comments and offline fixtures.
- [Fleets and the Fleet API](fleet-api.md): the Stargate Fleet API client,
  the app-wide fleet service a Space follows a fleet through, one saved
  token per fleet bound to its address, Settings › Fleets and what the
  built-in Fleet Console receives.

## Code quality and checks

If you want to know what must pass before work is done, read
[Commands](../AGENTS.md#commands), [Code quality rules](../AGENTS.md#code-quality-rules)
and the [gates](ways-of-working.md#gates).

- [Commands](../AGENTS.md#commands): the make targets and what `make check`
  runs.
- [Desktop capture](capture.md): isolated macOS and Linux screenshots,
  display prerequisites and fixture configuration.
- [Code quality rules](../AGENTS.md#code-quality-rules): rustqual, IOSP,
  function size and test assertions.
- [Dependency audit](../AGENTS.md#dependency-audit): `mise run audit` and
  accepted advisories in `deny.toml`.
- [Gates](ways-of-working.md#gates): the commit and push hooks; there is no
  CI yet.

## Boards

If you want to know how a board behaves, read its in-app guide. Its developer
page, where there is one, covers configuration and internals. Guides follow
the Framework's [board guide template](../../subspace-framework/docs/board-guide-template.md).

If you want to build a Space, read [Add a Space](../README.md#add-a-space) and
the Framework's [What the Framework gives a board](../../subspace-framework/docs/board-integration.md),
then fill in its [board checklist](../../subspace-framework/docs/board-checklist.md).

- [Design guide](../../subspace-framework/docs/design-guide.md): the
  workspace shell every Space is built on, its zones and rules, and the
  anatomy image that pull request screenshots are compared with.
- [The Subspace App's Spaces](spaces-audit.md): the agent tools the app serves,
  the contracts its Spaces declare and exchange, and the audit of which
  Framework integration points each Space uses, with the gaps.
- [Build and package a web Space](web-spaces-guide.md): make a web app into a
  web Space, use the bridge, test it with Node, package it with
  `scripts/package-web-space.py` and install it from a folder, ZIP or GitHub.
  The [web Space reference](web-spaces-reference.md) lists every manifest
  field, package rule and error, and how a product signs a web Space in with
  the app's account; the packages live in
  [web-spaces/](../web-spaces/README.md).

| Board | In-app guide | Developer page |
| --- | --- | --- |
| PRs | [GitHub pull requests](../spaces/github/help.md) | |
| Copilot Agents | [Copilot agent tasks](../spaces/github-copilot-agents/help.md) | [Copilot Agents dashboard](github-copilot-agents.md) |
| Channels | [IRC networks, channels and private messages](../spaces/irc/help.md) | [Channels](irc.md): configuration, protocol support, initiatives and fleets, agent tools |
| Issues | [Subspace API issues and comments](../spaces/issues/help.md) | [Issues and Subspace API](issues.md): connection, shared hub and offline fixtures |
| Decisions | [Subspace API decisions](../spaces/decisions/help.md) | [Decisions](decisions.md): reads, stored choices and opening linked issues |
| Space Lab | [Native dashboards and managed environments](../spaces/space-lab/help.md) | [Space Lab](space-lab.md): enablement and safety; [design evidence](agents/space-lab/README.md): retired harness history |
| Actions | [GitHub Actions](../spaces/actions/help.md) | [Actions board](actions.md) |
| Component gallery | [Shared UI components](../../subspace-framework/spaces/component-gallery/help.md) | |
| Web Spaces | [Websites and installed web Spaces](../spaces/web/help.md) | [Web Space reference](web-spaces-reference.md): manifest, install sources, package rules, bridge, native sessions; [authoring guide](web-spaces-guide.md) |

- [Desktop UI components](ui-components.md): shared catalogue and component ownership.

## The Framework

The shared UI, the shell and everything every app gets are documented in the
Framework's own [documentation index](../../subspace-framework/docs/README.md). The pages most
often needed for work here:

- [UI component catalogue](../../subspace-framework/docs/ui-components.md): every component and
  style, which crate owns it and the component rules.
- [Agent chat](../../subspace-framework/docs/agent-chat.md), [MCP server](../../subspace-framework/docs/mcp.md) and
  [Command-line interface](../../subspace-framework/docs/cli.md): the chat pane and engines, the
  `subspace mcp` server and headless board commands.
- [Developer tools](../../subspace-framework/docs/developer-tools.md): the debug-build element
  inspector and the build version markers.
- [Appearance](../../subspace-framework/docs/appearance.md), [Logging](../../subspace-framework/docs/logging.md),
  [Full-text search](../../subspace-framework/docs/search.md),
  [Report an issue](../../subspace-framework/docs/report-issue.md),
  [Features and experiments](../../subspace-framework/docs/feature-flags.md) and
  [Web research](../../subspace-framework/docs/web-research.md).

## App-owned features

- [Terminal](terminal.md): the docked terminal panel, its layouts, build
  feature, platforms and configuration; the in-app
  [terminal guide](../terminal/panel/help.md).
- [Application icons](app-icons.md): checking and regenerating the native
  icons.

## Settings, configuration and secrets

If you want to know where settings, credentials and state are stored, read
[Settings and stored state](../README.md#settings-and-stored-state).

- [Settings and stored state](../README.md#settings-and-stored-state):
  `config.json`, credential priority, `state.json` and caches.
- [Settings and appearance](settings.md): the Settings dialog, updates and
  service connections.
- [Fleet API tokens](fleet-api.md#where-tokens-are-stored): where each
  fleet's token is kept (keychain, or `fleet-api-tokens.json` in debug
  builds) and the rule that sends it only to the address it was saved for.

## Releases, packaging and installation

If you want to know how to version or publish a release, read
[Versioning](ways-of-working.md#versioning) and
[Releases](ways-of-working.md#releases).

- [Install Subspace](installation.md): source builds, packaging, signing,
  publishing, release installers, pre-releases and updates.
- [Make your own build](build-your-own.md): the build configuration file, the
  build environment, the code constants a fork changes, and signing and
  publishing, with WorkOS and Entra examples.
- [Releases](../AGENTS.md#releases): the release rules for agents.

## Product documentation site

If you want to know how the published guide is organised, read
[Product documentation](../AGENTS.md#product-documentation). The site's
navigation is `toc.yml` in the project root and `subspace/toc.yml`; the
shared quick-start fragments live in `subspace/includes/`.

- [Subspace](subspace/index.md): the product landing page.
- [Install and update Subspace](subspace/installation.md): the product
  installation page.
- [Boards and board guides](subspace/boards.md): links to every board guide.
- [Subspace settings](subspace/settings.md): the product settings page.
- [The connection file](subspace/connection-file.md): pointing a downloaded
  build at a hosted API and sign-in with `connection.json`.
- [Logs and retention](subspace/logging.md): finding and keeping the app log.
- [Documentation versions](versions.md): main and frozen release snapshots of
  the site.

## Change records

- [Selectable find results and link menus](changes/gpui-text-patch/README.md):
  before-and-after evidence from the Component gallery fixture.
- [Fleet API client and shared fleet tokens](changes/fleet-api-client/README.md):
  native reads and the event stream of each initiative's fleet, one token
  per fleet in Settings › Fleets, and the built-in Fleet Console's share.
- [Subspace API, accounts and fleets](changes/subspace-api-accounts/README.md):
  the Issues client, provider-neutral sign-in, initiative and fleet switching
  in Channels, and what came across from a downstream fork.
- [NETT backend design](changes/nett-backend/design.md): a second desktop
  build with NETT as its tracker, device-flow sign-in, its own organisation
  scope, NETT's update stream and features gated on what NETT serves.
  NETT-side prerequisites are tracked in the Indra repository.
- [A build with no API backend](changes/desktop-no-backend/README.md):
  `api_backend: "none"`, what it leaves out and where each part is decided.
- [Sign-in at start-up](changes/desktop-sign-in-gate/README.md): the sign-in
  dialog over the main window in a build with an API backend and no session.
If you want to know why something was built the way it was, read its change
record. [Change records](changes/README.md) explains the convention.

- [Public repository examples](changes/public-repository-examples/README.md):
  fictional deployment settings, tracker references and fixture identifiers.
- [Issue Activity data](changes/issue-activity-data/README.md) and
  [Issue Activity pane](changes/issue-activity-pane/README.md): data retrieval,
  refresh and the Issues Space view.
- [Integration pre-releases](changes/integration-prereleases/README.md):
  download-only snapshots, commit-derived versions and disabled updates.
- [Framework split research](changes/framework-split/research.md): separating
  the Framework from the Spaces, what blocks publishing and white-labelling,
  the proposed Space author API and the migration plan.
- [Workspace shell](changes/workspace-shell/intent.md): one layout for every
  Space, the restyle to the owner's mock-ups, the dock switches in the status
  bar and the Twilight Two theme, with the captures and the known gaps.
- [Standard right-click menu](changes/standard-context-menu/intent.md): the
  seven groups every menu is built from, in one order.
- [Add a Space from the title bar](changes/add-space-from-title-bar/intent.md):
  the plus after the last Space tab and its menu of the Spaces that are off.
- [Decisions Space](changes/decisions-space/intent.md): a read-only Space
  listing every decision, built on the workspace shell.
- [Decision authoring](changes/decision-authoring/README.md): create and review
  proposals, handle version conflicts and read immutable revisions.
- [Fleet roles in Channels](changes/channels-fleet-roles/README.md): rank
  marks for captains and the rear admiral, a ship channel's live status,
  connection commands in **More**, state dots on references and a counted
  unread divider.
- [Agent identity in Channels](changes/channels-agent-identity/README.md):
  ranks from the API's agent nicks and the fleet's recorded channels, agent
  avatars and coloured marks, no private conversation with an agent, and
  the base pull request's review fixes.
- [Connection guidance](changes/connection-guidance/README.md) explains what to check when a service is unreachable.
- [Channels navigator, request lines and composer hints](changes/channels-navigator-and-requests/README.md):
  the navigator grouped by fleet with a People section, request status lines
  that settle into `Done in …`, and the composer's hint row with `/ask`.
- [Channels fleet live](changes/channels-fleet-live/README.md): each
  fleet's live state from its Fleet API in Channels: a ship's dot, the
  header pill and the Ship, Starbase and People inspector tabs.
- [Channels Thread tab](changes/channels-thread-tab/README.md): the
  inspector's Thread tab with a request's status timeline and its replies,
  reply counts and status lines that open it, and reply mode.
- [Channels reactions](changes/channels-reactions/README.md): message
  reactions as pills in Channels, toggling yours, the stop reactions on a
  running request, and the client layer's review findings closed.
- [Channels tiles once](changes/channels-tiles-once/README.md): an issue or
  pull request tile only under the first message that names it, and fuller
  two-line tiles of a fixed size for both.
- [Bundled Inter and JetBrains Mono](changes/bundled-fonts/README.md): the
  fonts the Subspace App embeds for Twilight Two, and why themes that name no
  font keep the platform's fonts.
- [Unreleased Spaces](changes/unreleased-spaces/intent.md): Space Lab (then
  Board Lab) and the Component gallery as Cargo features left out of release
  builds.
- [Fleet Console built in](changes/fleet-console-built-in/README.md): Fleet
  Console in every macOS and Windows build without an install, and the
  fictional Stargate Fleet API that captures show it live against.
- [Workspace shell review findings](changes/workspace-shell-review-findings/intent.md):
  the follow-up that takes every open review finding of the workspace shell,
  starting with Decisions dropping what it read when the account changes.
- [Space Lab rename](changes/space-lab-rename/intent.md): Board Lab renamed
  Space Lab, in its display name and every identifier, with no migration.
- [Space Lab store race](changes/space-lab-store-race/intent.md): Space Lab's
  project folders created without a start-up race.
- [Twilight Two for new installs](changes/default-twilight-two/README.md): a
  new install opens in Twilight Two, recorded at its first start, while an
  existing install keeps the theme it shows.
