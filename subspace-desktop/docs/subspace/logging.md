# Logs and retention

The Subspace App writes an app log. It contains JSON Lines, one complete JSON
record per line.

## Find the app log

Open the app's Log view. **Open log folder** opens its directory. **Copy log
path** copies the current live file path, not the text shown in the viewer.
The existing **Copy** action copies the visible records. Copy log path is
disabled if file logging could not start.

The app log sits beside the product's `state.json`, under `logs/`:

| Platform | Stable app log directory |
| --- | --- |
| Linux | `$XDG_STATE_HOME/<binary>/logs`, or `~/.local/state/<binary>/logs` |
| macOS | `~/Library/Application Support/<binary>/logs` |
| Windows | `%LOCALAPPDATA%\<binary>\logs` |

`<binary>` is `subspace`. The live file is `<binary>.log`. Pre-release
channels use their existing channel-specific product directory.

## What is kept

- Keep the five most recent local dates with writes, including the current
  date. Days without writes do not count. Weekly use therefore keeps five
  weekly files. Several runs on one date append to the same dated file.
- Append `.YYYY-MM-DD` to the live filename for the dated file. For example,
  `subspace.log.2026-11-09`.
- On the first write after midnight, select the new local date. Opening an
  unused writer does not create an empty dated file.
- Each dated file holds at most 5 MiB. Before a complete record would exceed
  that limit, start a new chunk in that same file. Discard the earlier chunk
  for that date. A very busy day therefore keeps its latest chunk, not every
  record from the day. Drop individual records larger than 5 MiB.
- The stable live file mirrors the active dated chunk. Five dated files plus
  live use at most 30 MiB per app log, plus a small `.run-day-v1` metadata
  file and an empty `.run-day-v1.lock` file. Separate writers share the lock
  so their writes cannot bypass the size cap.

Retention removes only exact dated filenames for that log. It does not remove
the live path, the active dated file, foreign files, malformed date names,
directories, or symbolic links. If the system prevents required cleanup or
rollover, logging drops the persistent write rather than grow without limit.
Existing excess files that cannot be removed remain on disk.

## Existing numbered archives

The first use of this policy attempts to import the legacy live file and exact
`.1` through `.5` archives. It uses their local modification dates and complete
JSON records, with the same date count and size limits. Successful imports
remove their numbered source archives. Migration does not prevent startup.

Validated numbered archives older than the retained dates are removed. If a
canonical dated file already exists, migration preserves any same-date legacy
source rather than overwrite or guess how to merge it. Those sources, and
unsafe, unreadable or undeletable legacy files, are outside the 30 MiB
managed-data bound. Migration runs once when its marker can be saved.
Keep the `.run-day-v1` metadata and lock files, including after clearing logs, so legacy
data does not return on the next run.

Before sharing a log, inspect it and remove credentials, private keys, secret
values, and sensitive resource identifiers. Automatic redaction is not a
substitute for reviewing the file you share.
