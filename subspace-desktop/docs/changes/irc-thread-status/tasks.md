# Tasks: IRC thread status

- [x] Read Stargate's progress, typing, question, status and reply messages
      upstream (stargate#90 to #92) and record their exact shapes.
- [x] Record how Channels threads, counts and notifies them today.
- [x] Find that progress could not be identified reliably before a marker,
      and propose one.
- [x] Adopt the agreed contract: `+stargate/progress` on a `NOTICE` with
      `+draft/reply` naming the individual request.
- [x] Keep client-only tags on lines in the IRC client, live, in history and
      from the opening multiline batch.
- [x] Trust only the addressed ship's server-authenticated account, bound
      once per request, on a request Channels holds; record the rule and
      its sources in `spec.md`.
- [x] Keep one status per request message ID: every stage, endings that stay
      final, staleness on a disconnect, reopening.
- [x] Fold trusted notices out of rows, reply counts, unread counts and
      notifications, live and from history; keep everything else a message.
- [x] Draw one line per request with GPUI Kit's `Marker` and the shared
      `Pill`, and remeasure its row when it changes.
- [x] Add pure, client and headless render tests.
- [x] Extend the disposable fixture: three requests, two in one thread, a
      question and its answer, a lookalike from another account, a lost
      stream, a main-room final reply and one request ending while others
      work.
- [x] Update `docs/irc.md` and the guide.
- [x] Re-apply the change onto `main` after pull request #29 (the
      workspace shell), by hand, file by file; record what changed in
      `delivery.md`.
- [x] Bound the agent's nick on the status line to one line of plain text.
- [x] Keep the nick-wide `draft/metadata-2` status apart: not changed
      here; Channels shows it through `channels-agent-status`, now on
      `main`.
- [x] Run `make check` and `mise run quality` after the re-application and
      fix what they find (one clippy lint; no baseline refresh needed).
- [x] Run the fixture against a live local Ergo server (fixing its login
      wait) and capture the before and after evidence under `evidence/`.
- [x] Base the branch again on `main` at `19242ec`, after the agent status
      feature merged (#30, #33); merge `state.rs` and `docs/irc.md` by hand.
- [x] Address both model reviews of the previous head (see Review findings
      in `delivery.md`): pin each request's trusted account; refuse a
      request to its own sender; fold in proportion to each event; keep
      endings apart from their notices; order progress by server time;
      unfold progress whose request is dropped; refuse an empty marker
      suffix; keep only the allow-listed client-only tags; harden the
      fixture's config file. Leave case mapping and history age open, with
      reasons.
- [x] Add pinning, retention, marker and client tests for those fixes.
- [x] Build and test the round-1 fixes: `cargo test` and `make check` pass;
      commit them (`06cf1a6`).
- [x] Address both model reviews of `06cf1a6` (see Review findings, round
      2, in `delivery.md`): bind each request only as it arrives live or by
      its agent's first answer, never from later member lists; store
      bindings and endings beside the buffer and load them before the
      lines fold; refuse the requester's own account; count notices
      unfolded by eviction at once; drop format characters from the
      agent's name; replace the fixture's config atomically. Leave case
      mapping open, with the reason.
- [x] Add binding, storage, eviction-counting and agent-name tests.
- [ ] Build and test the round-2 fixes: `cargo test` and `make check`.
- [ ] Run the fixture against a fresh local Ergo server again.
- [ ] Owner: commit (signed), push and open the pull request, with the
      captures embedded by commit-pinned URLs.
- [ ] Both model reviews (round 3) clear the final head.

### Board checklist: irc

Identity and registration
- [x] ID, title, description, icon, descriptor and artwork: unchanged.

Layout and UI
- [x] Layout: the workspace shell from `main` is unchanged; the line sits
      in the conversation's transcript under the request, like the reply
      count. Nothing in the navigator, tab strip, toolbar, inspector or
      status bar.
- [x] Toolbar, palette commands, keys, menus and dialogs: none added.
- [x] Empty, loading and error states: no line without trusted progress;
      stale while the connection is down.

Settings and configuration
- [x] No new setting or feature flag.

Data caching and refresh
- [x] Stored lines keep their client tags; request bindings and endings
      are stored beside them and load before the lines fold; folding and
      statuses rebuild from held lines, with no I/O in render.

Search
- [x] Folded progress stays searchable; opening it shows its request.

Agent tools, MCP and CLI
- [x] Unchanged: `irc_read` still returns every line.

Notifications and attention
- [x] Trusted progress never counts as unread or notifies; everything else
      as before.

Help and docs
- [x] Guide and developer page updated; this record linked from
      `docs/irc.md`.

Communicating with other boards
- [x] No contract or cross-board dependency.

Testing and evidence
- [x] Pure logic and headless rendering covered.
- [x] Capture-build screenshots, before and after (see `delivery.md`).
