# Interface callers: where the 15 modules are used

Companion to the [functional specification](spec.md). This records the baseline before the rewrite.
For each item the 15 files define, this lists
where it is used **outside** those files (file: line), so the implementer knows what must keep compiling. Paths are
from the repository root. "Private" items are reached by sibling modules or tests through Rust's module privacy
(descendants of `session` or `network` see their private items).

How the list was made: a scan of every `.rs` file in `subspace-desktop/clients/irc` except the 15, joining
method-chain continuation lines, for each item's name (for example `session.isupport`, `.with_tag(`,
`registration::additions`), plus a scan of the workspace for `subspace_client_irc`. Rows for very common names are
exhaustive but may include a few look-alike matches; the per-file counts at the end are approximate.

Calls between the 15 files themselves (for example `network.rs` calling `Session::receive`, or `session/mod.rs`
calling `registration::start`) are not listed: they are rewritten together. Section numbers refer to `spec.md`.

## A. Reached from outside the crate

The 15 modules are all private (`mod codec; mod connection; mod network; mod proto; mod session;` at
`subspace-desktop/clients/irc/src/lib.rs`: 4–11). Their only public exposure is the re-export block at
`subspace-desktop/clients/irc/src/lib.rs`: 20–23, which requires `session/mod.rs` to keep:

```rust
pub use input::{COMMANDS, CommandHelp, FirstArgument};
pub use send::{MAX_INLINE_BYTES, check_draft, check_inline_text, connection_batches, draft_line_limit, message_budget};
```

Users of those re-exports in the `subspace-irc-board` crate:

| Item | Callers (file: lines) |
| --- | --- |
| `COMMANDS`, `CommandHelp`, `FirstArgument` | `subspace-desktop/spaces/irc/src/compose/suggest.rs`: 8, 77, 120, 126, 134, 137, 138, 146 |
| `MAX_INLINE_BYTES`, `check_draft`, `connection_batches`, `draft_line_limit` | `subspace-desktop/spaces/irc/src/compose.rs`: 14, 15, 61, 62, 164, 178, 179 |
| `check_inline_text` | `subspace-desktop/spaces/irc/src/agent_tools/write/validate.rs`: 3, 81 |
| `message_budget` | `subspace-desktop/spaces/irc/src/hub.rs`: 454 |

`subspace-desktop/clients/irc/src/test_support.rs` (feature `test-support`, used by
`subspace-desktop/spaces/irc/src/tests/uploads/rehearse.rs`: 6, 40–42) drives `Session` and reads `Output`; its
lines are in section B. `Runtime` (`runtime.rs`) uses the network task; its lines are under `network`.

Nothing else outside `clients/irc` names an item of the 15 modules.

## B. Reached from inside the crate

### codec (`src/codec.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `BODY_LIMIT: usize = 512` | `subspace-desktop/clients/irc/src/session/split.rs`: 3, 16 |
| `CLIENT_TAGS_LIMIT: usize = 4096` | `subspace-desktop/clients/irc/src/session/send.rs`: 99 |
| `SERVER_TAGS_LIMIT: usize = 8191` | none outside the 15 files (used by the acceptance tests) |
| `encode(&Message) -> Vec<u8>` | `subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 55, 68<br>`subspace-desktop/clients/irc/src/session/tests/replies.rs`: 278 |
| `LineBuffer` (`Default`, `extend`, `next_line`) | none outside the 15 files (used by `network.rs` and the acceptance tests) |

### connection (`src/connection/mod.rs`, `src/connection/tls.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `Timeouts { pub tcp, pub tls }` + `Default` | `subspace-desktop/clients/irc/src/network/tests/metadata.rs`: 8, 38<br>`subspace-desktop/clients/irc/src/network/tests.rs`: 7, 75, 103, 163, 226 |
| `BoxStream`, `Stream`, `connect`, `ConnectError` | not named outside the 15 files; `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 102–107 boxes its own stream type and passes it to `Link::new`, which needs the blanket `Stream` impl and `BoxStream = Box<dyn Stream>`. `connect` and `Timeouts` are also used by the acceptance tests. |

### network (`src/network.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `NetCommand::{Connect, Session, Shutdown}` | `subspace-desktop/clients/irc/src/network/tests/metadata.rs`: 7, 45, 54<br>`subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 12, 116<br>`subspace-desktop/clients/irc/src/network/tests.rs`: 6, 87, 94, 119, 166, 168, 191, 251<br>`subspace-desktop/clients/irc/src/runtime.rs`: 14, 67, 93, 94, 167 |
| `EventSink` (`Default`, `Clone`, `.0`, `extend`) | `subspace-desktop/clients/irc/src/network/tests/metadata.rs`: 7, 11, 14, 33, 60<br>`subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 12, 106, 117<br>`subspace-desktop/clients/irc/src/network/tests.rs`: 6, 36, 42, 70, 102, 162, 221, 248<br>`subspace-desktop/clients/irc/src/runtime.rs`: 14, 24, 30, 45, 53, 72, 83, 129, 156 |
| `run(config, commands, events)` | `subspace-desktop/clients/irc/src/runtime.rs`: 158 |
| `run_with(config, commands, events, timeouts)` (test-facing) | `subspace-desktop/clients/irc/src/network/tests/metadata.rs`: 7, 34<br>`subspace-desktop/clients/irc/src/network/tests.rs`: 6, 71, 107, 159, 222 |
| `backoff(u32) -> u64` (test-facing) | `subspace-desktop/clients/irc/src/network/tests.rs`: 6, 17 |
| `End` (`Debug`, `Lost { .. }`) (test-facing) | `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 12, 125 |
| `Link::new`, `Link::command` (test-facing) | `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 12, 107 |

### proto (`src/proto/*.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `Message::new(command, params)` | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 146<br>`subspace-desktop/clients/irc/src/session/channel_info.rs`: 99<br>`subspace-desktop/clients/irc/src/session/commands.rs`: 139, 155, 163, 171, 180, 188, 197, 203, 273, 284<br>`subspace-desktop/clients/irc/src/session/history.rs`: 116<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 62<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 52, 192<br>`subspace-desktop/clients/irc/src/session/send.rs`: 97, 289, 297, 301, 309<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 257 |
| `Message::with_tag(key, value)` | `subspace-desktop/clients/irc/src/session/commands.rs`: 284<br>`subspace-desktop/clients/irc/src/session/send.rs`: 97, 120, 309, 312<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 50, 51 |
| `Message::param(index)` | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 95<br>`subspace-desktop/clients/irc/src/session/channel_info.rs`: 40, 42, 47, 52, 58, 59, 71, 90, 152<br>`subspace-desktop/clients/irc/src/session/describe.rs`: 14<br>`subspace-desktop/clients/irc/src/session/dispatch.rs`: 21, 29<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 80, 81<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 103, 110<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 19, 37, 111, 151, 155<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 13, 66, 104, 109, 173<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 172, 173<br>`subspace-desktop/clients/irc/src/session/numerics.rs`: 74<br>`subspace-desktop/clients/irc/src/session/send.rs`: 117<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 48 |
| `Message::trailing()` | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 48, 74<br>`subspace-desktop/clients/irc/src/session/describe.rs`: 57<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 140, 145 |
| `Message::tag(key)` | `subspace-desktop/clients/irc/src/session/lines.rs`: 28, 30, 34, 39<br>`subspace-desktop/clients/irc/src/session/send.rs`: 118<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 87, 91, 139, 144, 174, 194, 198<br>`subspace-desktop/clients/irc/src/session/tests/replies.rs`: 280 |
| `Message::nick()` | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 41<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 17, 78<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 129<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 15, 18, 68, 99, 143, 171 |
| `Message.command` (pub field) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 90<br>`subspace-desktop/clients/irc/src/session/describe.rs`: 15, 59, 68<br>`subspace-desktop/clients/irc/src/session/dispatch.rs`: 10<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 31<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 110<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 60, 64, 163<br>`subspace-desktop/clients/irc/src/session/numerics.rs`: 16, 17, 25, 76<br>`subspace-desktop/clients/irc/src/session/send.rs`: 117, 118<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 64<br>`subspace-desktop/clients/irc/src/session/tests/messages/oversized.rs`: 50, 66<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 48, 63, 64, 174, 210 |
| `Message.params` (pub field) | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 131<br>`subspace-desktop/clients/irc/src/session/describe.rs`: 32, 47, 69<br>`subspace-desktop/clients/irc/src/session/history.rs`: 196, 197<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 86<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 23, 119<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 61, 62<br>`subspace-desktop/clients/irc/src/session/numerics.rs`: 19, 20, 44, 46 |
| `Message.source` (pub field) | `subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 45, 47 |
| `Message.tags` | none outside the 15 files |
| `proto::parse(&str)` | `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 93<br>`subspace-desktop/clients/irc/src/session/commands.rs`: 8, 141<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 114<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 3, 40, 59<br>`subspace-desktop/clients/irc/src/session/tests/replies.rs`: 279<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 7, 67<br>`subspace-desktop/clients/irc/src/test_support.rs`: 6, 63 |
| `proto::serialise(&Message)` | `subspace-desktop/clients/irc/src/session/send.rs`: 97<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 3, 53, 69, 126, 127, 128<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 7, 80<br>`subspace-desktop/clients/irc/src/test_support.rs`: 6, 74 |
| `proto::ctcp::{parse, Ctcp}` | `subspace-desktop/clients/irc/src/session/describe.rs`: 58, 65 |

### session (`src/session/mod.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `Carry` (fields `rejoin`, `spellings`, `left`, `anchors`; `forget`) | `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 14, 84<br>`subspace-desktop/clients/irc/src/session/tests/anchors.rs`: 6, 21, 23, 96, 97, 101<br>`subspace-desktop/clients/irc/src/session/tests/autojoin.rs`: 2, 92, 110, 120<br>`subspace-desktop/clients/irc/src/session/tests/forget.rs`: 4, 18, 42, 43, 61, 68, 69, 70, 71, 72, 86, 87<br>`subspace-desktop/clients/irc/src/session/tests/listing.rs`: 4, 111<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 10, 108<br>`subspace-desktop/clients/irc/src/session/tests/messages.rs`: 7, 52<br>`subspace-desktop/clients/irc/src/session/tests/metadata.rs`: 3, 28, 58<br>`subspace-desktop/clients/irc/src/session/tests/registration.rs`: 4, 12, 80, 102, 119, 132, 140, 149, 151, 166<br>`subspace-desktop/clients/irc/src/session/tests/spelling.rs`: 5, 107, 170<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 5, 108<br>`subspace-desktop/clients/irc/src/test_support.rs`: 7, 45 |
| `configure_autojoin(&mut Carry, &NetworkConfig, &Command)` | `subspace-desktop/clients/irc/src/session/tests/autojoin.rs`: 79, 121 |
| `Output::send(message)` (private method) | `subspace-desktop/clients/irc/src/session/commands.rs`: 33, 255<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 52, 192 |
| `Output::event(event)` (private method) | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 26<br>`subspace-desktop/clients/irc/src/session/commands.rs`: 99, 212, 256<br>`subspace-desktop/clients/irc/src/session/history.rs`: 181<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 94<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 39, 47, 140<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 149, 187<br>`subspace-desktop/clients/irc/src/session/numerics.rs`: 30, 59, 77<br>`subspace-desktop/clients/irc/src/session/send_all.rs`: 43 |
| `Output.written` | `subspace-desktop/clients/irc/src/session/send_all.rs`: 36, 37<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 40, 54, 67, 91, 123<br>`subspace-desktop/clients/irc/src/test_support.rs`: 71, 75 |
| `Output.unwritten` | `subspace-desktop/clients/irc/src/session/send_all.rs`: 38<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 54, 69, 97, 123<br>`subspace-desktop/clients/irc/src/test_support.rs`: 76 |
| `Output.quit` | `subspace-desktop/clients/irc/src/session/commands.rs`: 201<br>`subspace-desktop/clients/irc/src/session/tests/messages.rs`: 287<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 70 |
| `Session::new` | `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 84<br>`subspace-desktop/clients/irc/src/session/tests/anchors.rs`: 25<br>`subspace-desktop/clients/irc/src/session/tests/autojoin.rs`: 81, 92, 110, 123<br>`subspace-desktop/clients/irc/src/session/tests/forget.rs`: 18, 29, 44, 61, 73, 88<br>`subspace-desktop/clients/irc/src/session/tests/listing.rs`: 111<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 108<br>`subspace-desktop/clients/irc/src/session/tests/messages.rs`: 52<br>`subspace-desktop/clients/irc/src/session/tests/metadata/lifecycle.rs`: 97<br>`subspace-desktop/clients/irc/src/session/tests/metadata.rs`: 28, 58<br>`subspace-desktop/clients/irc/src/session/tests/registration.rs`: 12, 80, 102, 119, 132, 140, 147, 166<br>`subspace-desktop/clients/irc/src/session/tests/spelling.rs`: 108, 170<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 108<br>`subspace-desktop/clients/irc/src/test_support.rs`: 45 |
| `Session::start` | `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 85<br>`subspace-desktop/clients/irc/src/session/tests/anchors.rs`: 26<br>`subspace-desktop/clients/irc/src/session/tests/forget.rs`: 19, 30, 45, 62, 74, 89<br>`subspace-desktop/clients/irc/src/session/tests/listing.rs`: 112<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 109<br>`subspace-desktop/clients/irc/src/session/tests/messages.rs`: 53<br>`subspace-desktop/clients/irc/src/session/tests/metadata.rs`: 29, 59<br>`subspace-desktop/clients/irc/src/session/tests/registration.rs`: 13, 81, 103, 120, 133, 141, 154, 167<br>`subspace-desktop/clients/irc/src/session/tests/spelling.rs`: 109, 171<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 109<br>`subspace-desktop/clients/irc/src/test_support.rs`: 48 |
| `Session::receive` | `subspace-desktop/clients/irc/src/network/tests/send_all.rs`: 93<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 114<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 26, 113, 120<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 67<br>`subspace-desktop/clients/irc/src/test_support.rs`: 34, 57, 64 |
| `Session::command` | `subspace-desktop/clients/irc/src/session/tests/forget.rs`: 64, 112<br>`subspace-desktop/clients/irc/src/session/tests.rs`: 76<br>`subspace-desktop/clients/irc/src/test_support.rs`: 70 |
| `Session::written` | `subspace-desktop/clients/irc/src/test_support.rs`: 71 |
| `Session::tick` | `subspace-desktop/clients/irc/src/session/tests/metadata.rs`: 201, 207, 218 |
| `Session::abandon` | `subspace-desktop/clients/irc/src/session/tests/metadata/lifecycle.rs`: 90, 96 |
| `Session::abandon_history` | `subspace-desktop/clients/irc/src/session/tests/history.rs`: 217, 223, 226 |
| `Session::into_carry` | `subspace-desktop/clients/irc/src/session/tests/anchors.rs`: 84<br>`subspace-desktop/clients/irc/src/session/tests/autojoin.rs`: 77, 119<br>`subspace-desktop/clients/irc/src/session/tests/forget.rs`: 29, 41, 54, 65<br>`subspace-desktop/clients/irc/src/session/tests/metadata/lifecycle.rs`: 97<br>`subspace-desktop/clients/irc/src/session/tests/spelling.rs`: 106 |
| `Session::channel_names` | `subspace-desktop/clients/irc/src/session/tests/members.rs`: 30, 111 |
| `Session::network` | `subspace-desktop/clients/irc/src/session/metadata.rs`: 94, 109 |
| `Session::registered` | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 142<br>`subspace-desktop/clients/irc/src/session/send_all.rs`: 51<br>`subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs`: 116<br>`subspace-desktop/clients/irc/src/session/tests/registration.rs`: 67 |
| `Session::fold` (private) | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 23, 63, 71, 77, 91, 133, 152<br>`subspace-desktop/clients/irc/src/session/commands.rs`: 222<br>`subspace-desktop/clients/irc/src/session/history.rs`: 106, 173, 201, 335<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 98<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 26, 29, 129<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 17, 18, 24, 44, 113, 114, 145, 175<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 90, 156<br>`subspace-desktop/clients/irc/src/session/numerics.rs`: 74 |
| `Session::is_own` (private) | `subspace-desktop/clients/irc/src/session/lines.rs`: 18, 79<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 37, 151<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 15, 112, 174<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 133 |
| `Session::buffer` (private) | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 27, 123<br>`subspace-desktop/clients/irc/src/session/commands.rs`: 213<br>`subspace-desktop/clients/irc/src/session/history.rs`: 180, 229<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 27, 53<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 48, 141<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 14, 111, 150, 188 |
| `Session::has_cap` (private) | `subspace-desktop/clients/irc/src/session/commands.rs`: 271, 278<br>`subspace-desktop/clients/irc/src/session/history.rs`: 105<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 46, 57, 169<br>`subspace-desktop/clients/irc/src/session/send.rs`: 82, 94, 165, 324 |
| `Session::refold` (private) | `subspace-desktop/clients/irc/src/session/numerics.rs`: 52 |
| `Session.config` (private field) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 129, 137, 141<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 24<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 95<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 181<br>`subspace-desktop/clients/irc/src/session/send.rs`: 155, 157 |
| `Session.reg` (private field) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 123<br>`subspace-desktop/clients/irc/src/session/commands.rs`: 138, 239<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 23, 58, 99<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 67<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 179<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 50, 169<br>`subspace-desktop/clients/irc/src/session/send.rs`: 156, 160 |
| `Session.caps` (private field) | `subspace-desktop/clients/irc/src/session/send.rs`: 161<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 242, 244 |
| `Session.isupport` (private field) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 124<br>`subspace-desktop/clients/irc/src/session/channel_info.rs`: 72, 92, 121<br>`subspace-desktop/clients/irc/src/session/history.rs`: 109, 246<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 80<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 86<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 17, 24, 31<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 67<br>`subspace-desktop/clients/irc/src/session/numerics.rs`: 50<br>`subspace-desktop/clients/irc/src/session/send.rs`: 150 |
| `Session.channels` (private field) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 131<br>`subspace-desktop/clients/irc/src/session/channel_info.rs`: 25, 61, 79, 93, 122, 139, 153<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 32, 130<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 21, 117, 146, 176<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 121, 139<br>`subspace-desktop/clients/irc/src/session/numerics.rs`: 72 |
| `Session.queries` (private field) | `subspace-desktop/clients/irc/src/session/commands.rs`: 223<br>`subspace-desktop/clients/irc/src/session/history.rs`: 336<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 98<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 45, 177 |
| `Session.history` (private field) | `subspace-desktop/clients/irc/src/session/history.rs`: 108, 174, 202, 205, 209, 216 |
| `Session.anchors` (private field) | `subspace-desktop/clients/irc/src/session/commands.rs`: 55<br>`subspace-desktop/clients/irc/src/session/history.rs`: 45, 324 |
| `Session.listing` (private field) | `subspace-desktop/clients/irc/src/session/listing.rs`: 67, 75, 87, 93, 103, 108 |
| `Session.statuses` (private field) | `subspace-desktop/clients/irc/src/session/metadata.rs`: 49, 50, 51, 91, 105, 106, 127, 154, 176, 183, 191<br>`subspace-desktop/clients/irc/src/session/tests/metadata/lifecycle.rs`: 14 |
| `Session.rejoin` (private field) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 138 |
| `Session.autojoin` (private field) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 126, 129, 134, 137, 140<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 22 |
| `Session.left` (private field) | `subspace-desktop/clients/irc/src/session/membership.rs`: 56, 60<br>`subspace-desktop/clients/irc/src/session/tests/forget.rs`: 28 |
| `Session.spellings` (private field) | `subspace-desktop/clients/irc/src/session/numerics.rs`: 51 |
| `Session.sent_batches` (private field) | `subspace-desktop/clients/irc/src/session/send.rs`: 223 |
| `Session.now` (private field) | `subspace-desktop/clients/irc/src/session/lines.rs`: 36, 56<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 174, 182 |
| a `Session` field for open batches (name is yours) | none outside the 15 files |
| `session.reg.nick` | `subspace-desktop/clients/irc/src/session/commands.rs`: 138<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 23, 58<br>`subspace-desktop/clients/irc/src/session/membership.rs`: 179<br>`subspace-desktop/clients/irc/src/session/send.rs`: 156, 160 |
| `session.reg.registered` | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 123<br>`subspace-desktop/clients/irc/src/session/commands.rs`: 239<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 99<br>`subspace-desktop/clients/irc/src/session/listing.rs`: 67<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 50, 169 |
| other `session.reg` fields | none outside the 15 files |

### session::batch (`src/session/batch.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `batch(session, message, out)` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 16 |
| the store of open batches (design is yours) | none outside the 15 files |

### session::caps (`src/session/caps.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `cap(session, message, out)` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 14 |
| `authenticate(session, message, out)` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 15 |
| `sasl_done(session, message, out)` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 58 |
| `sasl_failed(session, message, out)` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 59 |
| `Caps.offered: BTreeMap<String, String>` | `subspace-desktop/clients/irc/src/session/send.rs`: 162<br>`subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 244 |
| `Caps.enabled: BTreeSet<String>` | `subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs`: 242 |
| the list of capabilities to request (design is yours) | none outside the 15 files |

### session::isupport (`src/session/isupport.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `Casemapping` (type, `Ascii`) | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 6, 16, 20, 21, 24, 47 |
| `Casemapping::fold` | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 50, 128 |
| `Isupport` (type) | `subspace-desktop/clients/irc/src/session/member_state.rs`: 7, 57 |
| `Isupport.chantypes` | `subspace-desktop/clients/irc/src/session/listing.rs`: 86 |
| `Isupport.prefix` | `subspace-desktop/clients/irc/src/session/member_state.rs`: 58 |
| `Isupport.network` | none outside the 15 files |
| `Isupport.casemapping` | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 124 |
| `Isupport.chathistory` | `subspace-desktop/clients/irc/src/session/history.rs`: 109 |
| `Isupport.statusmsg` | none outside the 15 files |
| `Isupport.list_modes` | `subspace-desktop/clients/irc/src/session/member_state.rs`: 71 |
| `Isupport.param_modes` | `subspace-desktop/clients/irc/src/session/member_state.rs`: 72 |
| `Isupport.set_param_modes` | `subspace-desktop/clients/irc/src/session/member_state.rs`: 73 |
| `Isupport.whox` | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 92 |
| `Isupport::apply` | `subspace-desktop/clients/irc/src/session/numerics.rs`: 50 |
| `Isupport::fold` | none outside the 15 files |
| `Isupport::channel` | `subspace-desktop/clients/irc/src/session/history.rs`: 246<br>`subspace-desktop/clients/irc/src/session/lines.rs`: 80<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 18<br>`subspace-desktop/clients/irc/src/session/metadata.rs`: 67<br>`subspace-desktop/clients/irc/src/session/send.rs`: 150 |
| `Isupport::symbols` | `subspace-desktop/clients/irc/src/session/channel_info.rs`: 72, 121<br>`subspace-desktop/clients/irc/src/session/member_state.rs`: 31 |

### session::messages (`src/session/messages.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `privmsg` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 17 |
| `notice` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 18 |
| `tagmsg` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 19 |
| `markread` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 20 |
| `standard_reply` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 24 |

### session::registration (`src/session/registration.rs`)

| Item | Callers (file: lines) |
| --- | --- |
| `ping` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 11 |
| `error` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 13 |
| `welcome` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 42 |
| `nick_error` | `subspace-desktop/clients/irc/src/session/dispatch.rs`: 56 |
| `additions(session, before)` | `subspace-desktop/clients/irc/src/session/autojoin.rs`: 149 |
| the registration burst (how `Session::start` gets it is yours) | none outside the 15 files |


### Files that use the widely used types

Approximate counts of lines naming each type, per file (outside the 15 files):

- **`proto::Message` (type and constructors)**: `subspace-desktop/clients/irc/src/session/autojoin.rs` (3); `subspace-desktop/clients/irc/src/session/channel_info.rs` (11); `subspace-desktop/clients/irc/src/session/commands.rs` (22); `subspace-desktop/clients/irc/src/session/describe.rs` (5); `subspace-desktop/clients/irc/src/session/dispatch.rs` (2); `subspace-desktop/clients/irc/src/session/history.rs` (10); `subspace-desktop/clients/irc/src/session/lines.rs` (4); `subspace-desktop/clients/irc/src/session/listing.rs` (9); `subspace-desktop/clients/irc/src/session/member_state.rs` (6); `subspace-desktop/clients/irc/src/session/membership.rs` (9); `subspace-desktop/clients/irc/src/session/metadata.rs` (6); `subspace-desktop/clients/irc/src/session/numerics.rs` (7); `subspace-desktop/clients/irc/src/session/send.rs` (14); `subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs` (3)
- **`session::Session`**: `subspace-desktop/clients/irc/src/network/tests/send_all.rs` (3); `subspace-desktop/clients/irc/src/session/autojoin.rs` (2); `subspace-desktop/clients/irc/src/session/channel_info.rs` (11); `subspace-desktop/clients/irc/src/session/commands.rs` (12); `subspace-desktop/clients/irc/src/session/describe.rs` (2); `subspace-desktop/clients/irc/src/session/dispatch.rs` (2); `subspace-desktop/clients/irc/src/session/history.rs` (12); `subspace-desktop/clients/irc/src/session/lines.rs` (5); `subspace-desktop/clients/irc/src/session/listing.rs` (8); `subspace-desktop/clients/irc/src/session/member_state.rs` (6); `subspace-desktop/clients/irc/src/session/membership.rs` (10); `subspace-desktop/clients/irc/src/session/metadata.rs` (12); `subspace-desktop/clients/irc/src/session/numerics.rs` (6); `subspace-desktop/clients/irc/src/session/send.rs` (7); `subspace-desktop/clients/irc/src/session/send_all.rs` (4); `subspace-desktop/clients/irc/src/session/tests.rs` (7); `subspace-desktop/clients/irc/src/session/tests/anchors.rs` (3); `subspace-desktop/clients/irc/src/session/tests/autojoin.rs` (5); `subspace-desktop/clients/irc/src/session/tests/forget.rs` (7); `subspace-desktop/clients/irc/src/session/tests/listing.rs` (2); `subspace-desktop/clients/irc/src/session/tests/messages.rs` (3); `subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs` (3); `subspace-desktop/clients/irc/src/session/tests/metadata.rs` (3); `subspace-desktop/clients/irc/src/session/tests/metadata/lifecycle.rs` (2); `subspace-desktop/clients/irc/src/session/tests/registration.rs` (9); `subspace-desktop/clients/irc/src/session/tests/spelling.rs` (3); `subspace-desktop/clients/irc/src/test_support.rs` (3)
- **`session::Output`**: `subspace-desktop/clients/irc/src/session/autojoin.rs` (2); `subspace-desktop/clients/irc/src/session/channel_info.rs` (11); `subspace-desktop/clients/irc/src/session/commands.rs` (11); `subspace-desktop/clients/irc/src/session/dispatch.rs` (2); `subspace-desktop/clients/irc/src/session/history.rs` (3); `subspace-desktop/clients/irc/src/session/listing.rs` (6); `subspace-desktop/clients/irc/src/session/member_state.rs` (6); `subspace-desktop/clients/irc/src/session/membership.rs` (8); `subspace-desktop/clients/irc/src/session/metadata.rs` (9); `subspace-desktop/clients/irc/src/session/numerics.rs` (6); `subspace-desktop/clients/irc/src/session/send.rs` (5); `subspace-desktop/clients/irc/src/session/send_all.rs` (4); `subspace-desktop/clients/irc/src/session/tests.rs` (6); `subspace-desktop/clients/irc/src/session/tests/messages.rs` (1); `subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs` (5); `subspace-desktop/clients/irc/src/session/tests/messages/oversized.rs` (2); `subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs` (4); `subspace-desktop/clients/irc/src/session/tests/metadata.rs` (2); `subspace-desktop/clients/irc/src/session/tests/metadata/lifecycle.rs` (3)
- **`Output.send` (field)**: `subspace-desktop/clients/irc/src/session/autojoin.rs` (2); `subspace-desktop/clients/irc/src/session/channel_info.rs` (1); `subspace-desktop/clients/irc/src/session/commands.rs` (10); `subspace-desktop/clients/irc/src/session/listing.rs` (2); `subspace-desktop/clients/irc/src/session/membership.rs` (1); `subspace-desktop/clients/irc/src/session/send_all.rs` (2); `subspace-desktop/clients/irc/src/session/tests.rs` (2); `subspace-desktop/clients/irc/src/session/tests/forget.rs` (2); `subspace-desktop/clients/irc/src/session/tests/messages.rs` (3); `subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs` (6); `subspace-desktop/clients/irc/src/session/tests/messages/oversized.rs` (8); `subspace-desktop/clients/irc/src/session/tests/messages/paragraphs.rs` (18); `subspace-desktop/clients/irc/src/session/tests/metadata.rs` (5); `subspace-desktop/clients/irc/src/session/tests/registration.rs` (2); `subspace-desktop/clients/irc/src/session/tests/replies.rs` (6); `subspace-desktop/clients/irc/src/test_support.rs` (1)
- **`Output.events` (field; the `tests/ergo` hits are a test helper's own `events` field)**: `subspace-desktop/clients/irc/src/session/autojoin.rs` (1); `subspace-desktop/clients/irc/src/session/channel_info.rs` (1); `subspace-desktop/clients/irc/src/session/commands.rs` (1); `subspace-desktop/clients/irc/src/session/history.rs` (1); `subspace-desktop/clients/irc/src/session/membership.rs` (4); `subspace-desktop/clients/irc/src/session/metadata.rs` (2); `subspace-desktop/clients/irc/src/session/send.rs` (3); `subspace-desktop/clients/irc/src/session/send_all.rs` (1); `subspace-desktop/clients/irc/src/session/tests.rs` (2); `subspace-desktop/clients/irc/src/session/tests/anchors.rs` (5); `subspace-desktop/clients/irc/src/session/tests/autojoin.rs` (4); `subspace-desktop/clients/irc/src/session/tests/forget.rs` (1); `subspace-desktop/clients/irc/src/session/tests/history.rs` (8); `subspace-desktop/clients/irc/src/session/tests/listing.rs` (4); `subspace-desktop/clients/irc/src/session/tests/members.rs` (12); `subspace-desktop/clients/irc/src/session/tests/messages.rs` (3); `subspace-desktop/clients/irc/src/session/tests/messages/all_or_nothing.rs` (2); `subspace-desktop/clients/irc/src/session/tests/messages/oversized.rs` (1); `subspace-desktop/clients/irc/src/session/tests/metadata.rs` (6); `subspace-desktop/clients/irc/src/session/tests/metadata/lifecycle.rs` (1); `subspace-desktop/clients/irc/src/session/tests/registration.rs` (7); `subspace-desktop/clients/irc/src/session/tests/replies.rs` (1); `subspace-desktop/clients/irc/src/session/tests/spelling.rs` (9); `subspace-desktop/clients/irc/src/test_support.rs` (1); `subspace-desktop/clients/irc/tests/ergo/anchors.rs` (1); `subspace-desktop/clients/irc/tests/ergo/atomic.rs` (2); `subspace-desktop/clients/irc/tests/ergo/metadata.rs` (2)

## C. Shapes the callers need

A summary of what the call sites above rely on, beyond the names (details in `spec.md`):

| Item | What the callers rely on |
| --- | --- |
| `codec::BODY_LIMIT` | `usize` constant 512 used in arithmetic (`split.rs`) |
| `codec::CLIENT_TAGS_LIMIT` | `usize` constant 4096 compared with a byte count (`send.rs`) |
| `codec::encode` | `fn(&Message) -> Vec<u8>`; tests compare with `format!("{wire}\r\n").as_bytes()` |
| `connection::Timeouts` | struct literal with public `tcp` and `tls` (`Duration`), and `Default` |
| `connection::BoxStream` | `Box<dyn Stream>` built from a test's own `AsyncRead + AsyncWrite + Unpin + Send` type |
| `network::EventSink` | `Default`, `Clone`, public tuple field `.0: Arc<Mutex<VecDeque<Event>>>` (locked and drained by `runtime.rs` and the tests), `extend` |
| `network::NetCommand` | variants `Connect`, `Session(Command)`, `Shutdown` |
| `network::run` | `async fn(NetworkConfig, UnboundedReceiver<NetCommand>, EventSink)` passed to `tokio::spawn` (so the future is `Send + 'static`) |
| `network::{run_with, backoff, End, Link}` | as in `spec.md` 4.1 |
| `proto::Message` | public `command: String`, `params: Vec<String>`, `source` (assignable from another message's `source`); `Debug`, `Clone`, `PartialEq`, `Eq`; the six methods with the exact signatures in `spec.md` 5.1 |
| `proto::parse` | `fn(&str) -> Result<Message, E>` with `E: Debug + Display` (`.unwrap()`, `.to_string()`, `if let Ok`) |
| `proto::serialise` | `fn(&Message) -> String`, usable as `.map(serialise)` over `&Message` items |
| `proto::ctcp` | `parse(&str) -> Option<Ctcp<'_>>`; `Ctcp` with `command: String` and `params: &str`; written as `Option<ctcp::Ctcp>` in a signature |
| `session::Carry` | `Default`, `Clone`, `PartialEq`, `Eq`, `Debug`; struct update syntax with `rejoin` and `anchors`; fields `rejoin`, `spellings`, `left`, `anchors`; `forget(&Command)` |
| `session::Output` | `Default`, `Debug`, `PartialEq`; struct update syntax with `events`; public fields `send`, `events`, `written`, `unwritten`, `quit`; private methods `send(Message)` and `event(Event)` |
| `session::Session` | the methods and private fields in `spec.md` 10.1, with exactly those types (for example `channels: BTreeMap<String, channels::Channel>`, `queries: BTreeSet<String>`, `history: history::Requests`, `statuses: metadata::Statuses`, `sent_batches: u64`, `now: DateTime<Utc>`) |
| `session::configure_autojoin` | `pub(crate) fn(&mut Carry, &NetworkConfig, &Command) -> Vec<Event>` |
| `caps::Caps` | public `offered: BTreeMap<String, String>` (`.get(&str)` then `.cloned()`, `.remove(&str)`) and `enabled: BTreeSet<String>` (`.remove(&str)`) |
| `isupport::Casemapping` | `Copy` (stored in an `Option` and copied out), `Debug`, `Clone`, `PartialEq`, `Eq`; variant `Ascii`; `fold(self, &str) -> String` |
| `isupport::Isupport` | public fields with the types in `spec.md` 13.1; `chantypes` is passed as `&[char]`, `list_modes` and friends use `String::contains(char)`, `chathistory` goes into a `(bool, u16)` tuple |
| handlers in `batch`, `caps`, `messages`, `registration` | `pub(super) fn(&mut Session, &Message, &mut Output)` (dispatch table), and `registration::additions(&Session, &BTreeSet<String>) -> Vec<Message>` |
