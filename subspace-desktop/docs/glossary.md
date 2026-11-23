# Glossary

The Subspace App's terms. The Framework's terms, such as App, Space,
board, channel and update source, are in the
[Framework glossary](../../subspace-framework/docs/glossary.md).

| Term | Meaning |
| --- | --- |
| Subspace App | The app. Binary `subspace`. Bundle `Subspace.app`. |
| Space Lab | The alpha Space for native dashboards backed by managed Python, Node or Bun projects, on macOS ARM64 only. Called Board Lab until November 2026; its board id, crate, Cargo feature and tools now say `space-lab` or `space_lab`. Release builds leave it out. See [Space Lab](space-lab.md). |
| Channels | The Space for IRC networks, channels and private messages, called IRC until 0.4.0. Its board id, crates, settings file and stored data keep the `irc` name. Not a release channel (see Channel). |
| Run | In Actions, one GitHub workflow execution. Active runs are queued, in progress, waiting, requested or pending. |
| Pending deployment | In Actions, an environment deployment waiting for review on a workflow run. |
| Job graph | In Actions, one node per API job and directed edges from workflow `needs` dependencies. Matrix variants remain separate nodes. |
| Degraded graph | An Actions job graph without edges, with an explanation that dependencies could not be read or matched. |
| Dispatch | An explicitly confirmed `workflow_dispatch` request with a ref and declared or unvalidated string inputs. |


## Proposed managed-board terms

These terms describe the accepted managed board environment intent. Space Lab exposes an experimental subset; the general Framework capability remains a proposal.

| Term | Proposed meaning |
| --- | --- |
| Managed board environment | A board-local project and dependency environment created from configuration and bound to configured app-owned runtime and package-manager versions, with explicit network, credential and output policies. Runtime downloads can be shared read-only; project dependencies and writable state remain isolated. Updates activate a new immutable revision. |
| Private toolchain | An app-owned toolchain CLI with private configuration, data, cache and state. Board environments do not use the host user's global toolchain or shell activation. The prototype copies a mise executable into a private tree as developer bootstrap, not as a release installer contract. |
| uv Python project | A board-local Python project with `pyproject.toml` and `uv.lock`, using uv as the project tool. |
| pnpm Node project | A board-local Node project with `package.json` and `pnpm-lock.yaml` when pnpm is the selected manager. Proposed default for new Node projects in the draft intent. Not an approved shipping policy. |
| Bun project | A board-local TypeScript project that uses Bun as the runtime and its native package manager, with `bun.lock`. |
| Approved extra CLI tool | An extra CLI installed through the private toolchain with an exact version pin. It is not a replacement of the board's primary runtime or package manager. |
| Configuration-created environment | A board environment created from declared configuration rather than from ad-hoc host installs. |
| Environment revision | An immutable prepared project tree for one environment identity. Activation points at a ready revision. It does not rewrite that revision in place. A failed update leaves the previous revision active. |
| Custom recipe reference | An immutable identity for versioned custom recipe content. A saved visualization stores the reference. Refresh loads that content, not a later file of the same name. |
| Execution sandbox | The OS-enforced boundary around an executing worker or job. It is separate from a Python virtualenv or Node dependency environment, which does not itself restrict host file, process or network access. |
| Visualization preset | A saved visualization definition with its controls, parameter bindings and defaults, plus a refresh recipe when one is available. It is independent of its originating chat. |
| Dashboard preset | A named arrangement of saved visualizations in a board. Its persistence does not confer authority to run their recipes. |
| Visualization snapshot | A retained result from one evaluation. It records that evaluation's data age and source scope; it is not itself a recipe for fetching new data. |
| Dashboard project | Editable dashboard source and a declarative native dashboard definition associated with a managed board environment. It is distinct from the prepared environment revision and from an immutable saved recipe. No general project contract is implemented yet. |
| Dashboard definition | A serializable description of supported native components, their layout, typed properties, bindings and named actions. It describes data for the native renderer, not executable GPUI code or a web page. |
| Control binding | A declared connection between a native control and a typed dashboard value or runtime action. User changes can reach runtime code, and validated runtime updates can change the control without creating another user event. |
| Live dashboard state | Accepted control values, options, results and operation status for an active dashboard definition. It is distinct from editable definition source and a persisted result snapshot. |
| Runtime session | Persistent managed execution associated with a dashboard project and selected source/environment revisions. It can retain data and connections across interactions; its memory is not saved dashboard content or a permission grant. |
