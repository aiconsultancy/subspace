# Logging

Every app built from the Framework writes a structured log. A record has a
time, a level, a target (a board id, `http`, `app`), a message and named
fields. Records go to a rotating file and to an in-memory buffer the log
viewer reads. Nothing in the workspace uses `tracing` or `log`; the module is
`subspace_core::log`.

## The file

| Item | Value |
| --- | --- |
| Directory | `logs/` beside `state.json` in the app's data directory, named by its [data name](glossary.md) (`~/Library/Application Support/subspace/logs` on macOS, `$XDG_STATE_HOME/subspace/logs` on Linux, `%LOCALAPPDATA%\subspace\logs` on Windows for the Subspace App) |
| File | `<binary>.log`: `subspace.log` for the Subspace App |
| Format | JSON Lines, one record per line: `{"seq":1,"at":"…","level":"info","target":"app","message":"…","fields":[["key","value"]]}` |
| Rotation | At 5 MiB the file becomes `<binary>.log.1`; older files shift to `.2` … `.5`, and the sixth is removed |
| Buffer | The newest 5,000 records stay in memory for the viewer |

The GUI and `subspace mcp` open the log at startup. `subspace about` and
`--version` do not.

## The level

The capture level decides which records are kept. The default is `info`.
The viewer's **Capture** menu changes it for the running app and stores it as
`app.log-level` in `state.json`, so the next start uses the same level. The
environment variable `SUBSPACE_LOG` (`error`, `warn`, `info`, `debug`,
`trace`) overrides the stored level for one run.

HTTP requests log at `debug` when they succeed and `warn` when they fail, so
`debug` shows every service call.

## Redaction

Every message and field value passes through `subspace_core::log::redact`
before it is kept. It masks, in this order:

- JSON Web Tokens (`eyJ…` with three segments);
- GitHub tokens (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `github_pat_`);
- `Authorization` header values, keeping the scheme: `Authorization: Bearer ***`;
- bare `Bearer x` and `Basic x` values;
- the query parameters `sig`, `signature`, `token`, `access_token`,
  `refresh_token`, `id_token`, `code`, `client_secret`, `key`, `api_key`,
  `apikey`, `sas`, `password`, `pwd` and `secret`;
- `name: value` and `name=value` pairs whose name ends in `token`, `secret`,
  `password`, `passwd`, `pwd`, `pat`, `api-key`, `apikey`, `private-key` or
  `signature`, including JSON keys and `AZURE_DEVOPS_EXT_PAT=`;
- a value of 32 or more characters after the word `secret`, `key` or `token`.

The HTTP middleware never logs a header, a query string or a body; it logs
the method, the host, the path, the status and the duration.

## Writing records

A board gets its handle from the Framework, scoped by its id:

```rust
let log = subspace_dashboard::logger::<Self>();   // target "github"
log.info("read 3 repositories");
log.record(Level::Warn, "cache miss", &[("key", key), ("age", "61m")]);
let cache = log.scoped("cache");                 // target "github.cache"
```

Code without a dashboard builds one with `Logger::new("target")`. An agent
tool uses the board id with a suffix, such as `github.tools`. Records
below the capture level cost one atomic load.

Every client request through `subspace_core::http::agent()` is logged by the
shared middleware; a client adds nothing.

## The viewer

Open it with **Cmd/Ctrl+Shift+L**, the palette's **Show logs** or, on macOS,
**Help › Show Logs**. It replaces the board, under a **Logs** header, until you close it:
select the close button (×, **Close logs**) at the right of the header, press
Esc anywhere in the view (Esc closes an open menu first), choose **Close logs**
in the palette, or select a board. While the log view is open, no board tab is
shown as current. The Search view closes the same way.

| Control | What it does |
| --- | --- |
| Level tabs | Show records at that level or more important |
| Target | One target, or all |
| Filter | Words that must all appear in the line |
| Follow | Keep the newest record in view; polls the buffer twice a second |
| Copy | Copy the shown lines as text |
| Copy log path | Copy the live log file's path |
| Open log folder | Show the log directory in the file manager |
| Capture | The level the app keeps from now on, remembered as `app.log-level` |
