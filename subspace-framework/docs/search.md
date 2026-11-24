# Full-text search prototype

Select **Search** in the toolbar, or press **Cmd/Ctrl+Shift+F**. This opens a
Framework view, not another board. **Commands** and **Cmd/Ctrl+K** still open
the command palette.

## Enable a source

Open the Search view's source controls and enable caching for a participating
board. Refresh that board and open the items or conversations you want to retain.
Search does not fetch service history or migrate older JSON caches. A board can
declare its source on by default (`BoardSettings::store_by_default`); the
person's choice in the Search view always wins.

Current adapters cover GitHub issue/pull-request details, comments and
activity, Copilot Agents session logs, and IRC channel and private messages.
Each source verifies its account before indexing. IRC's source is on by default
and has one local scope: its store is also where IRC keeps message history, so
turning it off stops keeping messages and **Clear** deletes them. (IRC is
the Subspace App's Channels Space, an example of a board-owned store.)

Each board starts with 60 days of retention and a 2,000,000,000-byte budget across
its accounts. The budget includes the content database and its full-text index.
Model assets have a separate fixed installation size. Clear removes that board's
indexed content, not its credentials, bookmarks, aliases or drafts. Disable also
stops cache reads and future writes.

## Search and open a result

Choose the current board or all participating boards, enter words and press
**Enter**. All words must match eligible indexed text. Search is case-insensitive
and supports Unicode words. The final word also matches a prefix, as in `/`
item search. Earlier words match complete tokens. SQL, wildcard and Boolean
query syntax are not exposed.

Use the arrow keys to select a result and **Enter** to open it. Results identify
their source and refresh time; the excerpt comes from the result's body. Opening a retained message does not fetch more
history or mark it read. A result that has expired, been cleared or left the
selected account is unavailable.

Each board has exactly one active scope (verified account or subscription).
Selecting another scope hides the previous scope's retained records. All-board
search combines those active scopes; it does not search every saved account.

Within its scope, a board can also hide some records for a while without
deleting them: `Service::set_hidden(board, rule)` takes a rule over a
`Document`. Every search asks it while candidates are selected, so hidden
records never take a visible one's place; `related_items` leaves them out
before its limit; `read` and `read_fenced` answer nothing for them, so a
citation cannot reach one; and `Service::shows(&hit)` asks it again for a
result kept from an earlier search. The Search view drops results the rule
hides before it shows them, so results that arrived before a change are hidden
too. Only the board's own `records` and `groups` still return hidden records.
Channels uses it to show the lines of networks that come from a signed-in
account only while that account is signed in.

Search covers only fetched, retained content. A miss does not prove that an item
is absent from the service. Attachment contents and unfetched history are not
indexed. Remote edits and access changes cannot be detected while offline.

## Local reranking

BM25 works without a model, network connection or Copilot sign-in. On macOS and
Linux, Subspace also offers an explicit model installation action for local ONNX
reranking. Windows uses BM25 only in this prototype to preserve its static C
runtime and existing package format. Installation downloads pinned public assets.
It does not upload queries or indexed content. Search never downloads a model
automatically.

The model is `cross-encoder/ms-marco-MiniLM-L6-v2`, licensed under Apache-2.0,
at revision `233902d25c440f23af6f7d6e94d2946bac0bee0a`. Its six assets total
91,728,555 bytes. Installation verifies fixed sizes and SHA-256 checksums.
Allow about 184 MB during installation or repair for the installed assets and
bounded staging files. This is separate from each board's content budget.
On macOS, assets live under
`~/Library/Application Support/subspace/search/models/` in private files.

Enable reranking after installation to reorder the lexical shortlist. It cannot
find a semantic match that BM25 did not retrieve. If the model is missing or fails,
the response reports the limitation and keeps BM25 results available.

Local CPU inference has been exercised on macOS ARM64. Linux still needs native
inference verification. The model can also be verified or
repaired explicitly from the source controls.

## Copilot Chat and MCP

`search_content` queries the same index as the UI. It accepts exact `kind`,
`account`, `project` and `parent_id` facets and inclusive `refreshed_after` and
`refreshed_before` Unix timestamps. Values are bound parameters; callers cannot
supply SQL or FTS syntax. Unknown argument names are rejected with the name.

`read_search_result` reads a retained result from its `open_in_subspace`
citation. `related_items` takes one canonical reference plus optional relation
types, boards and a limit. It returns only retained records linked to that exact
identity. A heuristic relation stays labelled with its evidence and confidence
and is never reported as exact. All three tools are read-only and use this
product's selected verified accounts. Callers cannot choose database paths,
account scopes or SQL expressions.

The query limit is 2,048 UTF-8 bytes, the board list limit is 32 and the result
limit is 50. Responses are bounded. Cached text is untrusted evidence, not an
instruction or authority to write to a service.

### Board adapter index API

Adapters publish the retained `Document` first, then call this API with the same
verified `Scope` and document id:

```rust
service.index_metadata(
    &scope,
    &document.id,
    &IndexMetadata {
        project: Some("owner/repository".into()),
        references: vec![Reference {
            identity: ReferenceIdentity::github("owner/repository", 42)?,
            relation_type: "pull-request".into(),
            matched: ReferenceMatch::Exact,
        }],
    },
)?;
```

`index_metadata` replaces the project facet and all references in one
transaction. It refuses a missing document, disabled cache or inactive scope.
Omit identities the service did not supply; never infer them from title text.
The constructors normalize case and whitespace and reject incomplete or
oversized values:

- `ReferenceIdentity::repository(repository)`
- `ReferenceIdentity::branch(repository, branch)`
- `ReferenceIdentity::commit(repository, commit)`
- `ReferenceIdentity::github(repository, issue_or_pull_request)`
- `ReferenceIdentity::github_copilot_agents(task_id)`

Use `ReferenceMatch::Exact` only for a service-supplied link. A heuristic uses
`ReferenceMatch::Heuristic { evidence, confidence }`, with non-empty evidence
and confidence from 0 to 1. `kind`, `account`, `parent_id` and `refreshed_at`
come from the published `Document` and verified `Scope`; the adapter supplies
only the optional project facet and relation records.

### Board storage API

A board's search store is also its persistent store: a board keeps its data as
retained `Document`s and reads them back, rather than in a second file. Besides
`upsert` and `index_metadata`, the service offers, all within the active scope,
policy and retention window:

- `records(&scope, &RecordQuery { filters, before, limit })`: the board's own
  documents matching `Filters`, newest first by `refreshed_at` then id, with
  payloads, up to 1,000 per page; `RecordPage::next` is the cursor for the
  next page.
- `groups(&scope, &filters, limit)`: each distinct `parent_id` with its record
  count and newest time, such as a board's conversations.
- `remove_matching(&scope, &filters)`: deletes every matching document and its
  references; at least one facet is required (`clear` wipes a board).
- `index_metadata_batch(&scope, &[(id, metadata)])`: `index_metadata` for up
  to 100 documents in one transaction.
- `offer_default(board, policy)`: applies a board's declared default policy
  until the person sets one (`settings.chosen`, schema version 3).

## Prototype status

Existing offline-history requirements remain the target. History restoration,
platform support and native proof are not yet complete.

Writes retry quota exhaustion by evicting the oldest records, up to 1,000 per
batch. Eviction and the new write commit together; a batch that still cannot fit
leaves existing records intact. Incoming document identities are not evicted.
Write admission counts filesystem allocation beyond SQLite's logical pages
once, including sidecars. The page cap also leaves room for the rollback
journal. A quota reduction that leaves too little room can be rejected.
Full-history clear uses a single delete/VACUUM operation. It runs off the UI thread, but is not yet bounded
as incremental maintenance. Native fenced-result navigation currently supports
Unix; Windows fails closed for this operation.

## Framework cache contracts

The search store records `PRAGMA user_version` (3). Older search files from 1.4.0
upgrade locally. Version 3 adds `settings.chosen`, set for stores whose policy
already differs from the default. Search rebuilds the FTS prefix index and indexed facets from
stored JSON in one transaction, retaining document identities, ages, scope and
fencing records. The `references` table starts empty because old files did not
record service relation identities.
Later opens do not rebuild it. Newer unsupported versions are rejected.

JSON atomic writes and index files use 0600 Unix permissions.

`subspace_dashboard::prefetch` is the shared detail runner for board adapters:

1. In a background task, call `Source::bind(service, board, bind)`; the closure
   must obtain a credential-bound `Scope`. A disabled source returns `None`
   without binding. Boards retain their service-specific authentication checks.
2. After a loaded-list update or `on_open`, call
   `prefetch::start(loaded, source, config, fetch, cx)`. Pass only loaded items.
   Keep the returned handle on the board and cancel/drop it on deactivation,
   scope change or list replacement. Never invoke the runner from render.
3. `fetch(item, &Work)` runs on the background executor. The helper obtains a
   fresh lease before each fetch, which checks current policy, active scope
   and the generation captured when the run was bound. Clear/re-enable or an
   away/back scope transition cannot revive an old run.
   Map fetched details to bounded documents and call `Work::publish(&documents)`.
   The existing service mutex serializes writes; lease ordering still applies.
4. Map client errors with `Failure::from(&http_error)` or equivalent typed
   classification. Authentication stops the run until a new verified binding.
   Throttling/network failures use `PollBackoff`, retry at most twice and pause
   the whole queue. A concurrent success cannot remove a throttle delay.

Defaults are two concurrent requests, 400 loaded items and 80 ms debounce.
Hard limits are four workers and 1,000 loaded items. Cancellation prevents new
requests and publishing at the next checkpoint; an already blocking HTTP call
finishes normally. Boards must use their existing transition/invalidation path
on identity changes so old leases cannot publish after a switch.

The helper alone does not expand existing board fetch behaviour. Copilot Agents logs
are approved opt-in adapter content. Raw secrets, certificates
and Terminal scrollback are permanently excluded.
