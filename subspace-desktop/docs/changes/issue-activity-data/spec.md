# Specification

The [hub API and contract](../../issues.md#activity-reads) are authoritative.
`ActivityQuery` selects the team and offset-bearing inclusive since window,
defaulting to seven days before an injectable current instant. Recent and
started-tracker reads are flat, ordered by activity and paged by independent
cursors, 50 rows at a time. Trackers have no since restriction.

`activate_activity` reads only on demand when the query changed or the last
first-page attempt completed more than 60 seconds ago. `reload_activity` always
reads both first pages, deferring a section's head while its continuation is
in flight. Each head supplies authoritative copies and re-sorts by time/ID.
Complete heads replace the whole result. A partial head retains the older tail
and cursor only with an anchor: a loaded ID with unchanged `lastActivityAt`.
A moved shared row is not an anchor; without an anchor the head replaces rows
and cursor so overflow remains reachable. Anchored partial heads remove missing
rows inside their range. Activation skips deferred heads as well as loading
heads. Panicking reads clear loading and deferred state so Reload can recover.
`retry_activity(section)` retries one section; `load_more_activity(section)`
continues or retries that section's cursor. Both sections expose independent
first-page and continuation loading/errors through `HubSnapshot::activity()`.
The section argument extends the suggested API so every tracker can be paged,
and either section can retry independently.

Requests use whole-second RFC 3339 instants with explicit offsets; the fixture
matches NETT's instant validation. The board calls `activate_activity` again
after organisation/team changes; clearing never refreshes activity by itself.

Activity and grouped lists use independent revisions; changing grouped tabs or
filters preserves activity and vice versa. Query changes discard the corresponding
activity data. Team and organisation changes, sign-out, expiry and Stop retire
late activity reads and clear scoped data. First pages use Visible priority and
later pages use LaterPage. No refresh timer or new IRC path exists.

One shared kind enum and actor DTO support future per-issue Activity reads.
Unknown kinds and actor types retain rows. Agent actors and all 11 named-list
kinds decode; optional `data.completedItemCount` is retained for batch completion.
The field name is verified against the Indra schema at `135f4796120d60e64546bafa036d930b368b8f9e`.
Old servers may omit the new row fields. Current servers never lower
`lastActivityAt` or date it before issue creation. Null last activity means
created and can cover odd imports whose entries all predate issue creation.
When activity is present, its `occurredAt` is the entry's original time and
equals `lastActivityAt`. Decoding still tolerates unequal values, age comes
from `lastActivityAt`, and head copies remain authoritative.

All content remains in memory. Synthetic loopback fixtures cover creation
fallbacks, matching entry/list times, known/unknown kinds and actors, and moving
activity across a keyset cursor between reads.
