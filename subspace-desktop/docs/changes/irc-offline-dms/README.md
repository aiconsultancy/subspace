# Offline IRC direct messages

## Intent and specification

Keep registered IRC accounts reachable and show them as away when their last
connection closes. The Ergo defaults permit account-level overrides.

After an authenticated connection negotiates chat history and batches, ask for
recent targets across retained history, up to 100 on each page. Joined channels
and existing DMs keep their per-conversation catch-up. Fetch recent messages for newly found
DMs, including ones older than the newest locally held message, and add them to
the existing unread list without a desktop notification. A rejected discovery
request leaves other history requests alone.

## Delivery and evidence

The IRC client transcript tests cover new DM discovery, already tracked targets
and a server that refuses discovery. The Channels reducer test checks that the
first replayed DM opens an unread conversation and prevents duplicate lines.
The server history path was exercised with two registered accounts on the
pinned Ergo image and PostgreSQL. One was away when the other sent its first
DM, then found and fetched it after reconnecting.

## Tasks

- [x] Enable opt-out defaults for always-on and auto-away.
- [x] Discover and recover first-time offline DMs.
- [x] Cover replay, duplicate targets and a rejected request.
