# IRC reply tags (PR 1 of 3)

## Intent

Deliver the approved thread model and send/receive protocol foundation after
subspace#21, keeping messages inline until PRs 2 and 3. The prototype branch
The IRC threads feature branch at `0ca7f75` remains local and untouched.

## Specification

- `Line::reply_to` preserves the nonempty `+draft/reply` reference on message,
  notice and action lines, including private conversations and history.
- `Line::thread_root` resolves within the same buffer's loaded history, in
  any delivery order, with a bounded traversal that rejects cycles. Unknown
  parents keep their ID; later history can extend the known chain. Raw links
  remain in stored payloads; old payloads without the field still load.
- `Line::reply` requires `message-tags` and a selected message with a server
  ID, and targets the earliest known root. `Command::Reply` rechecks the
  capability and ID, treats text literally, and uses the existing tag escaping.
- A multiline reply takes its reference from and sends it on the opening
  `BATCH` only. Split-message fallback tags each standalone message, and local
  echoes retain the reference.
- No UI change. No count, server lookup, reply action, hiding, pane, keyboard
  or focus changes. Existing unread/notification behaviour and agent tools
  remain as before. The reply-search follow-up's future lookup is separate from raw relationships.

## Delivery and evidence

Client tests were written first. The initial run failed with 17 missing-API
errors. After introducing the model and send path but before receive parsing,
the executable suite had five failures (live, history, multiline, root
resolution and cycle handling); three new send/gate tests passed. The receive
implementation made all eight original reply tests pass. Additional tests
cover ID limits, older payloads, private root resolution and store restoration.
The independent read-only review reported no P0, P1 or P2 findings. Its two
coverage suggestions are now tests: withdrawing `message-tags` after preparing
a reply, and accepting the exact maximum tag size without truncation.

`make check` and `mise run quality` pass. The IRC client and board suites
were also run with `IRC_TEST_SERVER=127.0.0.1:16930`, exercising the two
runtime integration tests and the board's agent-tool integration against
local Ergo. Before syncing subspace#41 the board suite passed all 265 tests.
All 12 reply tests pass,
including the two review follow-ups. Repository push hooks are run again
on the rebased, reviewed commit before signed publication. GitHub currently
reports zero Actions workflows; the PR's latest-head check state is inspected
after publication. Defiant's PR review and the captain's deployed proof follow.

Local Ergo 2.19.1 was started in this checkout on `127.0.0.1:16930`:

```sh
IRC_TEST_LISTEN=127.0.0.1:16930 scripts/irc-test-server.sh
IRC_PROOF_SERVER=127.0.0.1 IRC_PROOF_PORT=16930 IRC_PROOF_NICK=BookerProof python3 scripts/irc-reply-proof.py --local-plaintext --yes '#reply-proof' 'PR 1 final rebase'
```

16 November 2026, 00:51 UTC, run `reply-proof-56dae45549b0`: PASS for live
relay, fresh-connection `CHATHISTORY LATEST`, and `BEFORE msgid` replay.
Parent ID `wqyyyggu7gds5r3ctbk3jhgr7i`, reply ID
`rd869pa2m5xbhc86eh4za3jcbi`; both replays preserved the reply's ID and
`+draft/reply` value. This proves reconnect replay from in-memory local
history, not server-restart persistence.

Run the deployed proof from the worktree root with `IRC_PROOF_CLIENT_PEM` set
to the operator's certificate and key file. Substitute the configured IRC
host, account and registered channel for the example values. The command
clears any inherited PLAIN password.

```sh
env -u IRC_PROOF_PASSWORD IRC_PROOF_SERVER=irc.nett.example.com IRC_PROOF_PORT=6697 IRC_PROOF_NICK=reply-proof python3 subspace-desktop/scripts/irc-reply-proof.py --yes '#reply-proof' 'reply-tag replay'
```

This writes the three previewed, prefixed messages to the already registered
channel and reconnects.

The deployed proof passed on 16 November 2026 at 00:59 UTC. The captain ran
the command from this worktree at
`cafb185bfdd2d358102e3f39e94ef6a9980e24f8`, against
the configured TLS listener on port 6697 with SASL EXTERNAL. The run marker was `reply-proof-5cf11cca261b`.

- PASS: A's parent got a `msgid`, B's tagged reply echoed, and A received the
  matching reply.
- PASS: a fresh-connection `CHATHISTORY LATEST` kept the reply and its `msgid`.
- PASS: a fresh-connection `CHATHISTORY BEFORE` msgid anchor kept the reply.
- PASS: the reply-tag history proof as a whole.

The result is also recorded in the
[captain's PR comment](https://github.com/aiconsultancy/subspace/pull/43#issuecomment-6904407243).

## Tasks

- [x] Preserve the prototype and cherry-pick proof commits `0572a06`, `6cbf9e3`.
- [x] Test first; implement model, receive and send without UI changes.
- [x] Run the local Ergo proof.
- [x] Full checks and independent pre-PR review; no valid findings remain.

### Board checklist: irc

Identity and registration
- [x] ID, title, description and icon: not applicable because these stay unchanged.
- [x] Product descriptor, registration and artwork: not applicable because no board is added.
- [x] Retain when disabled: not applicable because process lifecycle is unchanged.

Layout and UI
- [x] List hooks/custom layout: not applicable because layout is unchanged.
- [x] Toolbar, status, badge and busy: not applicable because there are no new controls.
- [x] Palette commands: not applicable because the reply action comes later.
- [x] Keys and help foot: not applicable because no keyboard handling is added.
- [x] Menus, dialogs, modal and dismiss: not applicable because none are changed.
- [x] Empty/loading/error states: existing session errors report unavailable reply sends.
- [x] Quit warnings: not applicable because no new work survives the app.

Settings and configuration
- [x] Settings/notification opt-ins: not applicable because none change.
- [x] Service connections/config: not applicable because existing connections are used.
- [x] Feature flags: not applicable because this exposes no new UI experiment.

Data caching and refresh
- [x] Stored choices: not applicable because no preferences are added.
- [x] Framework store: existing message payloads preserve the optional reply reference.
- [x] Store-first loading: existing restore path, tested with a late parent.
- [x] Automatic refresh/UI blocking: not applicable because existing runtime/history I/O stays unchanged.

Search
- [x] Item search/recent items: not applicable because no search behaviour changes.
- [x] Full-text index: existing payload mapping round-trips replies; search text stays unchanged.
- [x] Cross-board references: not applicable because no contracts or references change.

Agent tools, MCP and CLI
- [x] Read/write tools: not applicable because agent thread support is deferred.
- [x] App-only tool hosting: not applicable because no tool is added.
- [x] Tool documentation: existing tools remain unchanged.

Chat context
- [x] Agent context: not applicable because no context changes.

Notifications and attention
- [x] Notification request/defaults/open: not applicable because this slice preserves existing behaviour.
- [x] Process notices/badge: not applicable because no notification path is added.

Help and docs
- [x] In-app help/images: not applicable because no visible interaction changes.
- [x] Developer page/index: reply foundation documented in the existing indexed `docs/irc.md`.

Communicating with other boards
- [x] Contracts/receivers/responders: not applicable because none change.
- [x] Framework globals: not applicable because none are introduced.

Links, logging and diagnostics
- [x] URL opening/citations: not applicable because no links change.
- [x] Writes/failures: existing session error lines; proof never logs credentials.

Testing and evidence
- [x] Pure logic/protocol tested; existing board interaction suite is required before publication.
- [x] Screenshots: not applicable because this PR has no UI change.
