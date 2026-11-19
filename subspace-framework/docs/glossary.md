# Glossary

The Framework's terms. An app keeps its own terms in its own glossary.

| Term | Meaning |
| --- | --- |
| Subspace Framework | The reusable crates in `subspace-framework/`, including the host, board kit, services and runtimes that apps are composed from, re-exported by the `subspace-framework` crate. |
| App server | The Streamable HTTP MCP endpoint that a running App opens when **Settings › MCP › Serve MCP over HTTP** is on. It is separate from a `subspace mcp` process. See [MCP server](mcp.md#settings). |
| Tool group | The unit that the MCP settings turn on or off for MCP clients: one group per active board, plus **Application** (`board_help`, search tools, `web_read`). Chat is not affected. |
| App | A binary built from the Framework with a selected set of boards. Code calls this a *product* (`ProductDescriptor`, `products/`). |
| Space | The preferred name for a board: a dashboard module, one crate (in an app, under `spaces/<id>/`) that implements `Dashboard` and is registered with a `BoardRegistration`. Prefer *Space* in new text, docs and names. |
| Board | The earlier name for a Space, still used by much of the code, the architecture rules and the docs. The terms are interchangeable: a Space is an alias for a board, not a separate concept, so every board rule applies to Spaces, including the layer rules, contracts and the broker, the [board checklist](board-checklist.md) and [what the Framework gives a board](board-integration.md). Existing references to boards need not be renamed. |
| Update source | The release stream an app build updates from, declared in its product descriptor. An app without an update source has no update command, controls or check. |
| Channel | Not the Channels Space. The release line of an app build: `stable`, or one pre-release type. Set at build time with `SUBSPACE_RELEASE_CHANNEL`. The channel selects the display name, bundle id, data locations, installer location and the releases that updates offer. |
| Type | A pre-release channel: `nightly` (scheduled), `alpha` or `beta` (manual). It is the first pre-release identifier in the version, such as `1.10.2-nightly.202611010200.ga43a333`. Stable is not a type. |
| Board cache | The planned opt-in Framework capability for disposable local copies of fetched service records and their search index. Each participating board owns its service fetching and record meaning; the Framework owns storage and cache policies. |
| Cache scope | The identity boundary for cached records and search: app, board, service endpoint, tenant where applicable, and previously verified service account. Equal record IDs in different scopes identify different records. |
| Cache retention | The allowed age since a record's last successful service refresh. A cache read or search does not extend it. Expired records are unavailable before physical reclamation. |
| History coverage | The portions of service history fetched and still retained locally. Searching that history does not search unfetched, expired or evicted records. |

Crate and binary names do not change.
