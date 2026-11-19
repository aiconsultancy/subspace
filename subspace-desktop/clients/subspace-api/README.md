# Subspace API client

The blocking client and GPUI-free hub for the desktop Issues board and its
Channels reference responder. The sole profile is `subspace-api`, named
`Subspace API`. The app configures its base URL once at startup with
`configure_api_origin` (HTTPS, or HTTP on loopback such as
`http://127.0.0.1:3000`; `/api` is appended when missing) and installs the
signed-in account's token source with `use_native_account`. Every request
goes to `{base}/v1/...` with `Authorization: Bearer <token>`.

`Hub::start()` reuses the running process-wide hub. `Hub::from_client(client,
name)` creates an isolated hub. Connection establishment reads `/v1/me` and the
paged project catalogue. Workflow states load only for a selected project.
Reads use two priority-ordered workers; reference reads are serialised and
comment POSTs use separate single-attempt workers. Network calls never run in
snapshot accessors.

`Client::decisions` reads `/v1/decisions` in API order using opaque cursors;
`DecisionQuery::for_issue` scopes the list to an issue key or UUID. Blank
filters are omitted, and the client caps the requested page size at 100.
`Client::decision` reads `/v1/decisions/{id}` and never writes decisions.

`Hub::activate_decisions` starts paging only for the open issue; subsequent
requests use `load_more_decisions` and `retry_decisions`. `IssueSnapshot`
exposes the retained page, loading, error and unavailable states. A refresh
re-reads the loaded depth from fresh cursors and keeps the old rows on a
retryable failure. `Hub::open_decision` reads one detail, exposed by
`IssueSnapshot::decision`: Loading, Loaded, Gone (404), or Error. Opening
another decision, closing the panel or leaving the issue retires its read.
Queued and in-flight results cannot cross those boundaries.

Without a signed-in account the client sends no credentials. Debug builds
can pair `SUBSPACE_API_URL` (a numeric loopback origin) with
`SUBSPACE_API_ACCESS_JWT_FILE`, a path to a private token file, for offline
fixtures. The client reads it on creation, trims trailing newlines, and
sends it as a bearer token to that local origin only. Loopback requests bypass proxies. Offline fixtures
can construct `Client::loopback(origin, jwt)` and inject it into the hub.

Comment recovery keeps `CommentState::Unclear { refresh: false }` frozen.
`check_comment(issue, parent)` performs a read of all pages in the root list or
reply thread. Before POST, the hub records all existing comment IDs; if this
read fails, no POST is sent. A matching author, body and parent with a new ID
confirms the post, without relying on desktop or server clocks. A completed check with no match sets `refresh: true`; only a separate
`submit_comment(issue, parent, true)` can then post again. The notice warns
that this may post twice. A failed check keeps retry disabled. `checking()`
reports an in-flight check independently of `refresh`.

Issue activity timestamps and provenance are on `IssueFields` for both list
and detail projections. `IssueListItem` also dereferences to those fields.
Comments retain metadata for existing media, but this crate cannot fetch,
grant, display or upload files. It exposes no issue mutations.

Validate with:

```sh
mise exec -- cargo test -p subspace-api-client
mise exec -- cargo clippy -p subspace-api-client --tests -- -D warnings
mise exec -- cargo fmt -p subspace-api-client
```

Cargo permits lockfile updates by default. Its `--locked` flag takes no value.
