# Subspace Framework documentation

Start here. This page links every page under `docs/`, grouped by area, with
one line on what each covers. Agents: read [AGENTS.md](../AGENTS.md) first.
`make check` fails when a page under `docs/` or a Space guide is not linked
from here or from a page it links.

## Using the Framework

- [Developer overview](../README.md): what the Framework is, the facade crate
  and its features, how an app depends on it and the Markdown patch every
  consumer copies.
- [Glossary](glossary.md): Framework, App, Space, board, channel, update source
  and the cache terms.
- [AGENTS.md](../AGENTS.md): the layer rules, component rules, conventions and
  code quality rules for Framework changes.

## Publishing

- [Publishing the Framework](publishing.md): prerequisites, the owner's
  checklist, the commands, the next release and how an app switches to the
  published crate.
- [What publishing exposes](publishing-audit.md): the audit of the packaged
  files, package sizes, licences and notices, and what the owner decides
  before the first publish.

## Building a Space

If you want to build a Space or know what the Framework offers one, read
[What the Framework gives a board](board-integration.md), then fill in the
[board checklist](board-checklist.md).

- [What the Framework gives a board](board-integration.md): every hook and
  service a board can use, with examples, and board-to-board contracts.
- [Board checklist](board-checklist.md): the integration points to complete or
  rule out in a board's change record and pull request.
- [Board guide template](board-guide-template.md): the sections of every
  Space's in-app guide.
- [Features and experiments](feature-flags.md): board enablement, experiment
  registration, graduation and removal.

## Shared UI

- [UI component catalogue](ui-components.md): every component and style, which
  crate owns it and the component rules.
- [Component gallery](../spaces/component-gallery/help.md): the in-app guide to
  the catalogue Space.
- [Appearance](appearance.md): bundled themes, the theme designer and fonts.
- [Developer tools](developer-tools.md): the debug-build element inspector,
  component names, the Developer menu and Developer mode.

## What every app gets

- [Logging](logging.md): `subspace_core::log`, the log file and the log viewer.
- [Full-text search](search.md): the Search view, its sources and the index.
- [Report an issue](report-issue.md): the report dialog, diagnostics and
  privacy controls, and the `report-issue` feature and repository an app
  supplies.
- [Command-line interface](cli.md): headless board commands.
- [Web research](web-research.md): `web_read` and its allowed hosts.

## Chat, agents and MCP

- [Agent chat](agent-chat.md): the chat pane, engines, sessions, approvals and
  canvases.
- [Pi chat engine](pi-chat.md): setup, tool permissions and limits.
- [ACP agent chat engine](acp-chat.md): OpenCode, Claude Code, Codex, Gemini CLI
  and custom agents over ACP.
- [MCP server](mcp.md): the `mcp` command, the app server and tool groups.

## Change records

- [Restart after an update](changes/restart-after-update/README.md): About and Settings restart routing and regression coverage.
