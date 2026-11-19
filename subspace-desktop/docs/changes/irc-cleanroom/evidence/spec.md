# Clean-room functional specification: the IRC client protocol layer

Crate `subspace-client-irc` (`subspace-desktop/clients/irc`), 15 modules to be rewritten.
This document describes the baseline before the rewrite.
Written 19 November 2026 by the specification author, who had read the code being replaced.
The implementer must not read that code (see the [exclusion list](excluded.md)).

## 0. How to use this document

### 0.1 Purpose

Fifteen files under `subspace-desktop/clients/irc/src/` are to be replaced by new code written only from this
document, the published IRC/IRCv3 specifications and the rest of the repository. The new code must:

1. keep every caller compiling unchanged (the rest of the crate, the `spaces/irc` crate and the tests);
2. keep every existing test outside the 15 files passing (section 17);
3. pass the new acceptance tests listed in each module section (also supplied as ready-made Rust test modules in
   `acceptance/`, see 17.3);
4. carry no adaptation header and no text from the replaced files. New files carry no "Adapted from" header; the
   crate stays `Apache-2.0` (inherited from `subspace-desktop/Cargo.toml`).

### 0.2 The files

| # | File (under `clients/irc/src/`) | Section |
| ---: | --- | --- |
| 1 | `codec.rs` | 1 |
| 2 | `connection/mod.rs` | 2 |
| 3 | `connection/tls.rs` | 3 |
| 4 | `network.rs` | 4 |
| 5 | `proto/mod.rs` | 5 |
| 6 | `proto/ctcp.rs` | 6 |
| 7 | `proto/parse.rs` | 7 |
| 8 | `proto/serialise.rs` | 8 |
| 9 | `proto/tests.rs` | 9 (write fresh tests; do not try to recreate the old ones) |
| 10 | `session/mod.rs` | 10 |
| 11 | `session/batch.rs` | 11 |
| 12 | `session/caps.rs` | 12 |
| 13 | `session/isupport.rs` | 13 |
| 14 | `session/messages.rs` | 14 |
| 15 | `session/registration.rs` | 15 |

You may read, call and rely on every other file in the repository, including the sibling modules of `session`
(`autojoin`, `channel_info`, `channels`, `commands`, `describe`, `dispatch`, `history`, `input`, `lines`, `listing`,
`member_state`, `membership`, `metadata`, `numerics`, `send`, `send_all`, `spelling`, `split`), `network/offline.rs`,
`network/secrets.rs`, `model.rs`, `runtime.rs`, `test_support.rs`, `format.rs` and all tests. Those files define much
of the behaviour; section 16 lists the helpers in them that produce behaviour this document requires.

### 0.3 Conventions

- "MUST", "SHOULD" and "MAY" are used as in RFC 2119.
- **Interface** items are given as exact Rust signatures where callers depend on them. Anything not listed is yours
  to design, including private helpers, private field names (unless listed) and internal representations.
- Where a behaviour follows a specification, the specification is cited and only the choices this crate makes on
  top of it are stated. Read the cited section.
- "Folding" a name means mapping it with the server's case mapping (section 13) so that equivalent nicks or
  channels compare equal.
- Rust edition 2024. Workspace lints apply (`clippy` with warnings denied in `make check`). Prose and comments in
  British English.
- Dependencies already declared for the crate: `base64`, `chrono`, `rustls` (features `aws_lc_rs`, `std`,
  `tls12`; no default features), `rustls-platform-verifier`, `serde`, `thiserror`, `tokio` (`full`),
  `tokio-rustls` (no default features); dev: `serde_json`, `tempfile`. Do not add dependencies unless needed; if you
  do, record why.

### 0.4 Where the modules sit

```text
spaces/irc (UI) --Command--> runtime::Runtime --NetCommand--> network task (one per network, tokio)
                                                                 |  connection::connect (TCP, TLS)
                                                                 |  codec (framing, encoding)
                                                                 |  proto (parse, serialise)
                                                                 v
                                                           session::Session (IO-free state machine)
spaces/irc <--Vec<Event> via Runtime::drain-- EventSink <--------+
```

`lib.rs` (not replaced) declares `mod codec; mod connection; mod network; mod proto; mod session;` (all private) and
re-exports from `session` (section 10.1). Only `network` performs IO; `session` is a pure state machine driven by
`network` (and directly by tests), which makes it testable without sockets.

### 0.5 Normative references

| Short name | Document |
| --- | --- |
| RFC 1459 | <https://www.rfc-editor.org/rfc/rfc1459>: §2.2 (character codes, case equivalence), §2.3 and §2.3.1 (message format, 512-byte limit, BNF), §4.1 (registration), §4.6.2–4.6.3 (PING/PONG), §6 (numeric replies) |
| RFC 2812 | <https://www.rfc-editor.org/rfc/rfc2812>: §2.3 and §2.3.1 (messages, prefix BNF), §3.1.1–3.1.3 (PASS, NICK, USER), §3.7.2–3.7.4 (PING, PONG, ERROR), §5.2 (error replies 432, 433, 436, 437) |
| Modern | Modern IRC Client Protocol, <https://modern.ircdocs.horse/>: "Messages" (tags, source, command, parameters), "Connection Registration", "RPL_ISUPPORT Parameters" (CASEMAPPING, CHANMODES, CHANTYPES, NETWORK, PREFIX, STATUSMSG), "Numerics" (001, 005, 432, 433, 436, 900–908) |
| CTCP | <https://modern.ircdocs.horse/ctcp>: "Message syntax" (the closing delimiter may be omitted), ACTION, PING, TIME, VERSION |
| CAP | IRCv3 Capability Negotiation, <https://ircv3.net/specs/extensions/capability-negotiation>: CAP LS 302, multi-line LS replies, capability values, REQ/ACK/NAK/END, cap-notify (NEW/DEL) |
| SASL | <https://ircv3.net/specs/extensions/sasl-3.1> and <https://ircv3.net/specs/extensions/sasl-3.2>: AUTHENTICATE, 400-byte chunks, `+` for empty, numerics 900–908, mechanism lists in the `sasl` value |
| PLAIN | RFC 4616, <https://www.rfc-editor.org/rfc/rfc4616> §2 (authzid NUL authcid NUL passwd) |
| EXTERNAL | RFC 4422 Appendix A, <https://www.rfc-editor.org/rfc/rfc4422#appendix-A> |
| Base64 | RFC 4648 §4, <https://www.rfc-editor.org/rfc/rfc4648#section-4> (standard alphabet with padding) |
| Tags | IRCv3 Message Tags, <https://ircv3.net/specs/extensions/message-tags>: format, "Escaping values", "Size limit", client-only tags |
| Batch | <https://ircv3.net/specs/extensions/batch> (references, nesting) |
| Chathistory | <https://ircv3.net/specs/extensions/chathistory> (batch type `chathistory`, ISUPPORT `CHATHISTORY`) |
| Multiline | <https://ircv3.net/specs/extensions/multiline> (`draft/multiline` batch, `draft/multiline-concat`, `max-bytes`, `max-lines`) |
| Read marker | <https://ircv3.net/specs/extensions/read-marking> (`MARKREAD <target> timestamp=…`) |
| Typing | <https://ircv3.net/specs/client-tags/typing> (`+typing` = active, paused, done; TAGMSG) |
| Reply | <https://ircv3.net/specs/client-tags/reply> (`+draft/reply`) |
| Metadata | <https://ircv3.net/specs/extensions/metadata> (`draft/metadata-2`) |
| Others | <https://ircv3.net/specs/extensions/> `server-time`, `echo-message`, `labeled-response`, `standard-replies`, `account-notify`, `account-tag`, `away-notify`, `chghost`, `extended-join`, `invite-notify`, `multi-prefix`, `setname`, `userhost-in-names` |
| ZNC playback | <https://wiki.znc.in/Playback> (batch type `znc.in/playback`) |
| TLS | rustls 0.23 <https://docs.rs/rustls/0.23>, rustls-platform-verifier 0.7 <https://docs.rs/rustls-platform-verifier/0.7>, tokio-rustls 0.26 <https://docs.rs/tokio-rustls/0.26>; SNI: RFC 6066 §3 |

## 1. `codec` (`src/codec.rs`)

Turns received bytes into lines, and messages into bytes to write, within the protocol's size limits.

### 1.1 Interface

Module visibility: `mod codec;` in `lib.rs` (crate-private).

```rust
pub const BODY_LIMIT: usize = 512;
pub const SERVER_TAGS_LIMIT: usize = 8191;
pub const CLIENT_TAGS_LIMIT: usize = 4096;

#[derive(Debug, Default)]
pub struct LineBuffer { /* private */ }

impl LineBuffer {
    pub fn extend(&mut self, bytes: &[u8]);
    pub fn next_line(&mut self) -> Result<Option<String>, E>; // E: your error type
}

pub fn encode(message: &crate::proto::Message) -> Vec<u8>;
```

| Constant | Meaning |
| --- | --- |
| `BODY_LIMIT` | longest line the protocol allows without tags, CR-LF included (RFC 1459 §2.3) |
| `SERVER_TAGS_LIMIT` | largest tag section a client must accept from a server, counting the `@` and the separating space (Tags "Size limit") |
| `CLIENT_TAGS_LIMIT` | largest tag section a client may send, counted the same way (Tags "Size limit") |

- `E` MUST implement `std::fmt::Debug`, `std::fmt::Display` and `std::error::Error` (the network task reports its
  `Display` text as the reason for dropping the connection). Its name is yours to choose; nothing outside the module
  names it.
- `BODY_LIMIT` is used by `session/split.rs` and `CLIENT_TAGS_LIMIT` by `session/send.rs`; `encode` is used by
  `network.rs` and by session tests. `SERVER_TAGS_LIMIT` is used by the acceptance tests.

### 1.2 Behaviour

Limits follow Tags "Size limit" and RFC 1459 §2.3.

`LineBuffer` accumulates bytes; `next_line` removes and returns the first complete line:

| Buffer state | `next_line` result |
| --- | --- |
| Contains a LF (0x0A) | `Ok(Some(line))`: the bytes up to and including the first LF are removed; the line is those bytes decoded as UTF-8 with invalid sequences replaced by U+FFFD (as `String::from_utf8_lossy` does), with every trailing CR and LF removed. An empty line yields `Ok(Some(""))`. |
| No LF, length ≤ 8703 (`SERVER_TAGS_LIMIT + BODY_LIMIT`) | `Ok(None)`; bytes are kept for the next call. |
| No LF, length > 8703 | `Err(E)`: the peer sent more than a full line without a terminator. |

- Bare LF terminates a line as CR-LF does. A CR-LF split across two `extend` calls is one terminator.
- A line that is terminated is returned whole even when it exceeds the limits (no truncation on input).
- The error case is checked only when no LF is buffered.

`encode(message)`:

1. Serialise the message (section 8), giving `[@tags ][:source ]command[ params]` without CR-LF.
2. If the text starts with `@`, its tag section is everything before the first space and its body everything after
   that space. Otherwise there is no tag section and the whole text is the body.
3. Truncate the body to at most `BODY_LIMIT - 2` (510) bytes and the tag section to at most `CLIENT_TAGS_LIMIT - 1`
   (4095) bytes. Truncation MUST cut at a UTF-8 character boundary, keeping the longest prefix that fits.
4. Return the bytes of `tag section + " " + body + "\r\n"`, or `body + "\r\n"` without tags.

So the encoded line never exceeds 512 bytes of body (including CR-LF) and 4096 bytes of tags (including `@` and the
space). A tag section of exactly 4095 bytes is sent unchanged.

### 1.3 Acceptance tests

| ID | Input | Expected |
| --- | --- | --- |
| C01 | the constants | 512, 4096, 8191 |
| C02 | extend `NOTICE * :one\r\nNOTICE * :two\nNOT`; call `next_line` three times; extend `ICE * :three\r\n`; call twice | `Some("NOTICE * :one")`, `Some("NOTICE * :two")`, `None`, `Some("NOTICE * :three")`, `None` |
| C03 | extend `PING :x\r`, call; extend `\n`, call | `None`, then `Some("PING :x")` |
| C04 | bytes `:a!b@c PRIVMSG #x :na`, 0xEF, `ve\r\n` | `Some(":a!b@c PRIVMSG #x :na\u{FFFD}ve")` |
| C05 | `\r\nPING :y\r\r\n` | `Some("")`, then `Some("PING :y")` |
| C06 | 8703 bytes of `z`, call; one more `z`, call | `Ok(None)`, then `Err(_)` |
| C07 | 9000 bytes of `q` then `\n` in one extend | a line of length 9000 |
| C08 | `encode` of PRIVMSG `#room` `good morning` with tag `label=a1`; `encode` of PING `t0` | `@label=a1 PRIVMSG #room :good morning\r\n`; `PING t0\r\n` |
| C09 | `encode` of PRIVMSG `#x` with text `ü` × 300 | `PRIVMSG #x ` + `ü` × 249 + `\r\n` (511 bytes; the cut falls inside a character and backs off) |
| C10 | TAGMSG `#x` with tag `k` = `v` × 4092 | tag section of 4095 bytes plus the space (4096), body `TAGMSG #x\r\n` |
| C11 | TAGMSG `#x` with tag `k` = `w` × 4196 | tag section truncated to 4095 bytes (starts `@k=www`), body `TAGMSG #x\r\n` |

## 2. `connection` (`src/connection/mod.rs`)

Establishes the byte stream to a server: a TCP socket, optionally wrapped in TLS.

### 2.1 Interface

Module visibility: `mod connection;` in `lib.rs`; this module declares `mod tls;` (private).

```rust
pub trait Stream: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> Stream for T {}
pub type BoxStream = Box<dyn Stream>;

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub tcp: std::time::Duration,
    pub tls: std::time::Duration,
}
impl Default for Timeouts { /* 15 s each */ }

#[derive(Debug, thiserror::Error)]   // thiserror is a suggestion; Display and Error are required
pub enum ConnectError { /* variants are yours */ }
impl ConnectError {
    pub fn retryable(&self) -> bool;
}

pub async fn connect(
    config: &crate::model::NetworkConfig,
    timeouts: Timeouts,
) -> Result<BoxStream, ConnectError>;
```

- `retryable()` decides what the network task does after a failed attempt: true means wait and try again, false
  means stop and report `Failed` (table in 2.2).
- The blanket `Stream` impl is required: `network/tests/send_all.rs` boxes its own fake stream type into a
  `BoxStream`.
- `Timeouts` (with public `tcp` and `tls` and `Default`) is constructed by `network/tests.rs` and
  `network/tests/metadata.rs`.

### 2.2 Behaviour

1. Resolve and connect TCP to `(config.host, config.port)`. If this takes longer than `timeouts.tcp`, fail with a
   timeout error for the step named `Connecting`.
2. Set `TCP_NODELAY` on the socket (an IO error here is an IO failure).
3. If `config.tls` is false, return the TCP stream.
4. Otherwise perform the TLS handshake (section 3). If this takes longer than `timeouts.tls`, fail with a timeout
   error for the step named `The TLS handshake`.

Error classification and `Display` text:

| Failure | `retryable()` | `Display` |
| --- | --- | --- |
| IO error (resolution, refusal, reset, NODELAY) | true | the IO error's own text |
| Step timeout | true | `{step} timed out after {secs} seconds`, where `secs` is the limit in whole seconds rounded down (a 100 ms limit reads `0 seconds`). Example: `The TLS handshake timed out after 0 seconds`. |
| TLS handshake failure (including a certificate the verifier rejects) | true | `TLS: ` followed by the TLS error's text |
| TLS configuration failure (unreadable or invalid PEM file, invalid server name, rustls configuration error) | false | `TLS: ` followed by the TLS error's text |

The network task shows these texts to the user in the connection status, so they must stay readable. A test asserts
that the timeout text contains `timed out`.

### 2.3 Acceptance tests

| ID | Input | Expected |
| --- | --- | --- |
| N01 | `Timeouts::default()` | 15 s and 15 s |
| N02 | plain config to a listening local port; then the same after the listener is dropped | `Ok`; then `Err` with `retryable() == true` |
| N03 | TLS config (`acceptInvalidCerts: true`) to a local listener that accepts and never speaks; `tcp` 5 s, `tls` 100 ms | `Err`, retryable, `Display` exactly `The TLS handshake timed out after 0 seconds` |
| N04 | TLS config to a local listener with `caCert` (and separately `clientCert`) naming a missing file | `Err`, `retryable() == false`, `Display` starts with `TLS: ` |

## 3. `connection::tls` (`src/connection/tls.rs`)

### 3.1 Interface

Private to `connection`. Nothing outside `connection` names its items. Suggested shape (yours to change):

```rust
pub async fn connect(tcp: tokio::net::TcpStream, config: &crate::model::NetworkConfig)
    -> Result<tokio_rustls::client::TlsStream<tokio::net::TcpStream>, TlsError>;
```

`TlsError` (any name) MUST distinguish handshake failures (retryable) from configuration failures (not retryable),
and implement `Display` and `Error`.

### 3.2 Behaviour

Build a rustls client configuration with the `aws-lc-rs` crypto provider and rustls's safe default protocol
versions (TLS 1.2 and 1.3). Then:

| `config.accept_invalid_certs` | `config.ca_cert` | Server certificate verification |
| --- | --- | --- |
| true | any | Accept any certificate chain for any server name (a development setting; see `model.rs`). Use rustls's "dangerous" custom-verifier API (`ClientConfig::dangerous`, `ServerCertVerifier`). For the handshake signature methods, the RECOMMENDED choice is to verify with the provider's algorithms (`rustls::crypto::verify_tls12_signature` / `verify_tls13_signature` with `signature_verification_algorithms`), which still lets any certificate through; accepting every signature is also acceptable. Report the provider's supported schemes. |
| false | `Some(path)` | The platform verifier (rustls-platform-verifier) with extra trusted roots: every certificate in the PEM file at `path`. |
| false | `None` | The platform verifier alone. |

Client authentication (used for SASL EXTERNAL and certificate-based server access):

- If `config.client_cert` is `Some(cert_path)`: the certificate chain is every certificate in the PEM file at
  `cert_path`; the private key is the first private key (PKCS#1, PKCS#8 or SEC1) in the PEM file at
  `config.client_key`, or in `cert_path` itself when `client_key` is `None`.
- Otherwise: no client authentication.

The server name for SNI and verification is `config.host`, which may be a DNS name or an IP address literal
(RFC 6066 §3). Then run the handshake over the TCP stream.

Errors: an unreadable or malformed PEM file, an invalid server name or a rustls configuration error is a
configuration failure; an error during the handshake itself (including the verifier rejecting the certificate and
the peer closing) is a handshake failure. The wording of the texts is yours; they appear after `TLS: `.

Write the "accept anything" verifier from the rustls documentation. Every implementation of that trait looks alike
because the trait dictates it; do not look for an existing implementation to copy.

### 3.3 Acceptance tests

N03 and N04 above cover the timeout and configuration-error paths. Recommended manual checks (not automated, since no
certificate generator is a dependency):

| ID | Setup | Expected |
| --- | --- | --- |
| T-M1 | Ergo with a self-signed certificate on 6697; network with `tls: true`, `acceptInvalidCerts: true` | registers |
| T-M2 | same, `acceptInvalidCerts: false`, no `caCert` | `Reconnecting` with a `TLS: …` error (handshake failure, retried) |
| T-M3 | same, `caCert` pointing at the self-signed certificate (as a CA) | registers |
| T-M4 | client certificate in one PEM file with its key, `clientCert` only; SASL `external: true` | SASL EXTERNAL succeeds on a server that maps the certificate fingerprint |

## 4. `network` (`src/network.rs`)

Each configured network runs as its own tokio task. The task owns the socket: it feeds received lines to the
session, writes what the session produces, detects a dead link, retries after failures with growing delays, and still
answers commands when there is no connection.

### 4.1 Interface

Module visibility: `mod network;` in `lib.rs`. This file MUST declare these submodules, which are not replaced:

```rust
mod offline;      // network/offline.rs: offline::reply(config, command) -> Option<Event>
mod secrets;      // network/secrets.rs: secrets::resolve(config) -> Result<NetworkConfig, String>
#[cfg(test)]
mod tests;        // network/tests.rs (+ tests/metadata.rs, tests/send_all.rs)
```

Crate interface (used by `runtime.rs`):

```rust
#[derive(Debug)]
pub(crate) enum NetCommand {
    Connect,
    Session(crate::model::Command),
    Shutdown,
}

#[derive(Clone, Default)]
pub(crate) struct EventSink(pub std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<crate::model::Event>>>);
impl EventSink {
    pub fn extend(&self, events: impl IntoIterator<Item = crate::model::Event>);
}

pub(crate) async fn run(
    config: crate::model::NetworkConfig,
    commands: tokio::sync::mpsc::UnboundedReceiver<NetCommand>,
    events: EventSink,
);
```

Test interface (private items that `network/tests.rs`, `network/tests/metadata.rs` and
`network/tests/send_all.rs` import with `super::` / `super::super::`; keep these names and shapes):

```rust
async fn run_with(
    config: crate::model::NetworkConfig,
    commands: tokio::sync::mpsc::UnboundedReceiver<NetCommand>,
    events: EventSink,
    timeouts: crate::connection::Timeouts,
);

fn backoff(attempt: u32) -> u64;

#[derive(Debug)]
enum End {
    Quit,
    Shutdown,
    Lost { error: String, registered: bool },
    Fatal(String),
}

struct Link { /* private */ }
impl Link {
    fn new(stream: crate::connection::BoxStream, session: crate::session::Session, events: EventSink) -> Self;
    async fn command(&mut self, command: Option<NetCommand>) -> Option<End>;
}
```

| `End` variant | When a connection attempt or connection ends this way |
| --- | --- |
| `Quit` | ended at the owner's request: a `Disconnect` command, or a QUIT the owner sent (`Output::quit`) |
| `Shutdown` | the runtime is stopping or the network was reconfigured or removed |
| `Lost { error, registered }` | the attempt failed or the link dropped in a way another attempt may fix; `registered` says whether `001` had arrived |
| `Fatal(error)` | something only a configuration change can fix (for example an unreadable SASL password file or certificate) |

The tests only match `End::Lost { .. }` and format `End` with `{:?}`; the other variants are a suggestion. They only
call `Link::new` and `Link::command`.

`EventSink` wraps the queue that `Runtime::drain` empties; every network task holds a clone. `EventSink::extend` appends in order. If the mutex is poisoned it MUST drop the events silently (no panic).

### 4.2 Behaviour

#### 4.2.1 Status reporting

Every status change is reported as `Event::Status { network: config.id.clone(), status }` through the sink, in order.
`ConnectionStatus` is defined in `model.rs`.

#### 4.2.2 States of a network task

The task is always in one of three situations: **waiting** (no connection), **connecting** (TCP and TLS in
progress) or **connected** (a `Session` is running). It also keeps, across connections, a `Carry` (initially
`Carry::default()`) and a reconnect counter `attempt` (initially 0).

| Situation | On entry | Leaves when |
| --- | --- | --- |
| task start | nothing | immediately: to *connecting* if `config.auto_connect`, else to *waiting without a deadline* |
| *waiting without a deadline* | report `Disconnected` | a `Connect` command (→ *connecting*), or a stop (task ends) |
| *waiting with a deadline* `d` | nothing | `d` elapses or `Connect` arrives (→ *connecting*), or a stop (task ends) |
| *waiting forever* (after a fatal error) | nothing (the last status, `Failed`, stays) | `Connect` (→ *connecting*), or a stop (task ends) |
| *connecting* | report `Connecting` | the stream opens (→ *connected*), the attempt fails, or a command abandons it (4.2.3) |
| *connected* | report `Registering` | the connection ends (4.2.4) |

While waiting in any form, an owner `Disconnect` command resets `attempt` to 0 and moves to *waiting without a
deadline* (so it reports `Disconnected` again). A *stop* is the command queue closing or a `Shutdown` command. Every
other command received while waiting is answered offline (4.2.3).

How a connection or attempt ends decides what comes next:

| End | Next |
| --- | --- |
| `Shutdown` | the task ends |
| `Quit` | *waiting without a deadline* |
| `Fatal(error)` | report `Failed { error }`, then *waiting forever* |
| `Lost { error, registered }` | set `attempt` to 1 if `registered`, otherwise increase it by 1; report `Reconnecting { attempt, delay_secs: backoff(attempt), error }`; then *waiting with a deadline* of that many seconds |

`attempt` changes only as stated (it is not reset after `Quit`).

`backoff(attempt)` is `2^(attempt − 1)` seconds capped at 60, where `attempt − 1` saturates at 0 and the power
saturates: attempts 0, 1, 2, …, 7, 8 give 1, 1, 2, 4, 8, 16, 32, 60, 60.

On reaching *connected*: resolve secrets with `secrets::resolve(&config)`. If that fails with `Err(text)`, the
connection ends `Fatal(text)` at once (the stream is dropped). Otherwise create
`Session::new(resolved_config, carry.clone())` and run it (4.2.4). However the connection ends, report the events of
`session.abandon()` and then replace the kept carry with `session.into_carry()`.

#### 4.2.3 Commands while there is no session

While *connecting*, commands keep being read alongside the connection attempt:

| Command | Effect on the attempt |
| --- | --- |
| queue closed, `Shutdown` | abandoned; ends `Shutdown` |
| `Session(Command::Disconnect { .. })` | abandoned; ends `Quit` |
| `Connect` | none |
| any other `Session(command)` | none; the command is answered offline |

A failed attempt ends `Lost { error: err.to_string(), registered: false }` when `err.retryable()`, otherwise
`Fatal(err.to_string())`.

**Answering a command offline** (while waiting or connecting) does three things, in this order:

1. report the events of `crate::session::configure_autojoin(&mut carry, &config, &command)`;
2. call `carry.forget(&command)`;
3. report `offline::reply(&config, &command)` when it is `Some`.

#### 4.2.4 A running connection

The stream is split into a read half and a write half. The task remembers when input last arrived and whether (and
when) it sent a keep-alive PING.

**Delivering an `Output`** to the connection gives these guarantees:

- the events in `out.events` are reported before any byte of `out.send` is written;
- all of `out.send` is encoded with `codec::encode` and written as one buffer (`write_all`, then `flush`); nothing is
  written when there are no bytes;
- if writing or flushing fails, `out.unwritten` is reported (and `out.written` is not), and the connection ends
  `Lost { error: io_error.to_string(), registered: session.registered() }`;
- otherwise `session.written(&out.written)` is called, `out.written` is reported, and the connection ends `Quit` if
  `out.quit` is set.

An all-or-nothing send (`Command::SendAll`) relies on this: its local echoes and its `Sent` answer are in `written`,
its uncertain answer in `unwritten` (see `network/tests/send_all.rs`).

The connection first delivers `session.start(Utc::now())`. Then it reacts to whichever of these comes first, until
the connection ends:

| Input | Reaction |
| --- | --- |
| socket read returns 0 bytes | ends `Lost` with error `The server closed the connection` |
| socket read fails | ends `Lost` with the IO error's text |
| socket read returns bytes (read buffer 4096 bytes) | note the time of input; forget any pending PING; add the bytes to the line buffer; then, for each complete line in arrival order, parse it with `proto::parse` (silently skip lines that do not parse) and deliver `session.receive(message, Utc::now())` before looking at the next line. A line-buffer error ends `Lost` with the error's text. |
| command queue closed, or `Shutdown` | deliver `session.command(Command::Disconnect { network: session.network(), reason: "Subspace closed".into() }, Utc::now())`, ignoring a write failure; then end `Shutdown` |
| `Connect` | nothing |
| `Session(command)` | deliver `session.command(command, Utc::now())` |
| timer, every 10 s (the first may fire at once) | deliver `session.tick(Utc::now())`; then, if a PING is pending and was sent more than 60 s ago, end `Lost` with error `The server stopped responding (ping timeout)`; otherwise, if no PING is pending and no input arrived for at least 120 s, send `PING subspace-<unix seconds>` (one parameter) and note it as pending (a write failure ends `Lost`) |

Every `Lost` built here carries `registered: session.registered()`.

Because each line is delivered before the next is parsed, anything the server causes with a later line is written
after everything an earlier line caused (the network tests depend on this).

#### 4.2.5 Timing summary

| Constant | Value |
| --- | --- |
| TCP connect limit | 15 s (`Timeouts::default`) |
| TLS handshake limit | 15 s (`Timeouts::default`) |
| Read buffer | 4096 bytes |
| Keep-alive timer | every 10 s |
| Silence before a keep-alive PING | 120 s |
| PING answer limit | 60 s (checked by the timer, so the drop happens 60–70 s after the PING) |
| Backoff | 1, 2, 4 … 60 s |
| QUIT reason when the task stops | `Subspace closed` |

The user-visible texts above (`The server closed the connection`, `The server stopped responding (ping timeout)`,
`Subspace closed`, and the timeout and TLS texts of section 2) are product wording, not protocol. Keep them for
continuity unless the owner agrees new wording; only the `timed out` part is asserted by an existing test.
### 4.3 Acceptance tests

Existing tests in `network/tests.rs` (backoff sequence; Disconnect abandons a hanging TLS handshake; a stalled
handshake times out and retries with attempt 1; a channel forgotten while disconnected is not joined at connect; a
dropped connection ends the pending history request), `network/tests/metadata.rs` (socket loss and owner disconnect
each clear nick statuses exactly once) and `network/tests/send_all.rs` (write failures give one uncertain answer and
no echo; a full write answers after the echoes) must pass. New:

| ID | Input | Expected |
| --- | --- | --- |
| W01 | `backoff(0)`, `backoff(1)`, `backoff(6)`, `backoff(7)`, `backoff(u32::MAX)` | 1, 1, 32, 60, 60 |
| W02 | `autoConnect: false`; send `Session(Join #x)`, then `Connect`; local server reads the registration messages; then `Shutdown` | statuses `Disconnected`, `Connecting`, `Registering`; an Error line `Not connected to Acceptance: cannot join #x` in the server buffer; bytes exactly `CAP LS 302\r\nNICK kim\r\nUSER kim 0 * kim\r\n`; on shutdown the server reads `QUIT :Subspace closed\r\n` and the task ends |
| W03 | SASL `passwordFile` pointing at a missing file; server accepts | statuses `Connecting`, `Registering`, `Failed { error }` with the file path in `error`; no further status for 1.5 s (no retry) |
| W04 | server drops the first two connections at once, accepts the third, sends `:s 001 kim :Hi`, then drops it | `Reconnecting { attempt: 1, delay_secs: 1 }`, then `{ attempt: 2, delay_secs: 2 }`, then `Connected { nick: "kim" }`, then `Reconnecting { attempt: 1, delay_secs: 1 }` |
| W05 | registered; owner sends `Disconnect { reason: "later" }` | server reads `QUIT later\r\n`; last status `Disconnected`; no `Reconnecting` within 1.5 s; a later `Connect` opens a new connection |
| W06 (optional; needs tokio's `test-util` feature as a dev-dependency) | registered, then silent for 120 s of paused time | the server reads `PING subspace-<digits>`; with no answer, `Reconnecting` with error `The server stopped responding (ping timeout)` follows |

## 5. `proto` (`src/proto/mod.rs`)

The message type shared by the codec, the session and the tests.

### 5.1 Interface

Module visibility: `mod proto;` in `lib.rs`. Within it:

```rust
pub mod ctcp;              // section 6
mod parse;                 // section 7 (or any private layout)
mod serialise;             // section 8 (or any private layout)
pub use parse::parse;      // crate::proto::parse
pub use serialise::serialise; // crate::proto::serialise
#[cfg(test)]
mod tests;                 // section 9
```

The paths `crate::proto::Message`, `crate::proto::parse`, `crate::proto::serialise`, `crate::proto::ctcp` and
`crate::proto::ctcp::{parse, Ctcp}` MUST exist.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub source: Option<Source>,
    pub command: String,
    pub params: Vec<String>,
    // tags: representation is yours (see below)
}

impl Message {
    pub fn new<S: Into<String>>(command: &str, params: impl IntoIterator<Item = S>) -> Self;
    pub fn with_tag(self, key: &str, value: &str) -> Self;
    pub fn param(&self, index: usize) -> &str;
    pub fn trailing(&self) -> &str;
    pub fn tag(&self, key: &str) -> Option<&str>;
    pub fn nick(&self) -> Option<&str>;
}
```

| Method | Returns |
| --- | --- |
| `new(command, params)` | a message with no tags and no source, the command ASCII upper-cased, the parameters converted to `String` |
| `with_tag(key, value)` | the message with that tag set (an existing value under `key` is replaced) |
| `param(i)` | parameter `i`, or `""` if there are fewer parameters |
| `trailing()` | the final parameter (usually the text), or `""` if there are no parameters |
| `tag(key)` | the tag's unescaped value, `Some("")` for a tag present without a value, `None` when absent |
| `nick()` | the nick of a user source; `None` for a server source or no source |

- `Message::new` is called with arrays of `&str`, `&String` or `String`, `Vec<String>`, `Option<String>` and iterator
  chains, so the generic signature above is required.
- `command` and `params` are read directly by many callers. `source` is read and assigned by a session test, which
  moves the `source` of one parsed message onto another, so it MUST be a public, cloneable field. Nothing outside the
  module names the type `Source` (or a user type), so their design is yours, provided they can represent a server
  name and a user with nick, optional user and optional host.
- No caller outside the module touches the tags field. Keep tags in any representation that gives: lookup by key,
  replacement on insert, and serialisation in ascending byte order of key (section 8). A public `tags` field is
  allowed but not required.

### 5.2 Behaviour

The message model follows Modern "Messages" and Tags. `Message::new` produces no tags and no source. `with_tag`
inserts or replaces. `nick()` is `Some` only for a user source.

### 5.3 Acceptance tests

| ID | Input | Expected |
| --- | --- | --- |
| M01 | `Message::new("part", ["#a", "x"]).with_tag("k", "1").with_tag("k", "2")` | command `PART`, no source, tag `k` = `2` |
| M02 | `Message::new("JOIN", vec!["#a".to_string()])`; `Message::new("LIST", Vec::<String>::new())` | params `["#a"]`; params empty |
| M03 | parsed `QUIT` | `trailing() == ""`, `param(0) == ""`, `nick() == None` |

## 6. `proto::ctcp` (`src/proto/ctcp.rs`)

### 6.1 Interface

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ctcp<'a> {
    pub command: String,
    pub params: &'a str,
}

pub fn parse(text: &str) -> Option<Ctcp<'_>>;
```

Used by `session/describe.rs` (`query.command == "ACTION"`, `query.params`) and by `session::messages`.

### 6.2 Behaviour

Follows CTCP "Message syntax":

1. The text MUST start with U+0001; otherwise `None`.
2. Remove that leading delimiter, and one trailing U+0001 if present (the closing delimiter is optional).
3. Split the remainder at the first space: the part before is the command, the part after (possibly empty, with any
   further spaces kept as they are) is `params`. Without a space, the whole remainder is the command and `params`
   is `""`.
4. An empty command gives `None`. Otherwise the command is ASCII upper-cased.

### 6.3 Acceptance tests

| ID | Text | Expected |
| --- | --- | --- |
| T01 | `\x01VERSION\x01` | `VERSION`, `""` |
| T02 | `\x01ping 1700000000\x01` | `PING`, `1700000000` |
| T03 | `\x01ACTION stretches` (no closing delimiter) | `ACTION`, `stretches` |
| T04 | `\x01ACTION  looks around \x01` | `ACTION`, ` looks around ` |
| T05 | `plain text`, empty, `\x01\x01`, `\x01`, `\x01 x\x01`, `x\x01VERSION\x01` | `None` each |

## 7. `proto::parse` (`src/proto/parse.rs`)

### 7.1 Interface

```rust
pub fn parse(line: &str) -> Result<Message, ParseError>; // re-exported as crate::proto::parse
```

`ParseError` (any name) MUST implement `Debug`, `Display` and `std::error::Error` (`Clone`, `PartialEq` and `Eq`
are recommended). Its
`Display` text MUST include the offending line (after line-ending removal); it reaches the user as an error line when
`/raw` or `/quote` input does not parse (`session/commands.rs` calls `.to_string()`). Suggested wording: "cannot parse
IRC line: " followed by the line in Rust debug quoting.

### 7.2 Behaviour

Grammar per Modern "Messages", RFC 1459 §2.3.1 and RFC 2812 §2.3.1, with these choices:

1. Remove every trailing CR and LF.
2. **Tags**: if the line starts with `@`, the tag section runs to the first space (exclusive). The rest of the line
   follows after that space with any further leading spaces removed. Tag section syntax: items separated by `;`;
   empty items are skipped; each item is `key` or `key=value` (split at the first `=`); a missing value and an empty
   value both mean `""`. Values are unescaped per Tags "Escaping values": `\:` → `;`, `\s` → space, `\\` → `\`,
   `\r` → CR, `\n` → LF; a backslash before any other character yields that character; a lone trailing backslash is
   dropped. When a key repeats, the last value wins.
3. **Source**: if what remains starts with `:`, the source runs to the next space (exclusive); the rest follows with
   leading spaces removed. Classification:
   - contains `!`: a user. The nick is the text before the first `!`; the text after it is split at its first `@`
     into user and host; without `@` it is all user and the host is absent. (So a nick may contain `@` or `:` when a
     `!` follows, as bridged nicks do.)
   - otherwise, contains `.` or `:`: a server name (host names, `*.example` masks and IPv6 literals).
   - otherwise: a user with only a nick (no user, no host). A bare `name@host` without `!` is therefore a nick when
     it has no dot, and a server name when it has one; it is not split.
4. **Command**: the next word, up to the first space or the end. It MUST be either one or more ASCII letters, or
   exactly three ASCII digits. Anything else (empty, other lengths of digits, mixed letters and digits, punctuation,
   non-ASCII, a line that starts with a space) is a `ParseError`. The command is ASCII upper-cased.
5. **Parameters**: repeat: skip spaces; stop at the end; if the next character is `:`, everything after it (spaces
   and colons included) is the last parameter and parsing stops; otherwise take the word up to the next space.
   Several spaces between parameters count as one separator. `TOPIC #a :` gives the parameters `#a` and `""`.

### 7.3 Acceptance tests

| ID | Line | Expected |
| --- | --- | --- |
| P01 | `@label=x7;+example.org/flag :carol!c@host.example PRIVMSG #ops :status check` | tags `label`=`x7`, `+example.org/flag`=`""`; nick `carol`; command `PRIVMSG`; params `["#ops", "status check"]`; `param(7) == ""` |
| P02 | `:srv.example 372 me :-  welcome :) to  the server ` | params `["me", "-  welcome :) to  the server "]` |
| P03 | `MODE   #ops   +v    dave` | params `["#ops", "+v", "dave"]` |
| P04 | `@a=1   :x.example   PING   :tok` | command `PING`, params `["tok"]` |
| P05 | `notice #a :hi` | command `NOTICE` |
| P06 | `:srv.example 352 me #ops ~u h.example srv.example dave H :0 Dave` | command `352`, params `["me", "#ops", "~u", "h.example", "srv.example", "dave", "H", "0 Dave"]` |
| P07 | each of: empty, three spaces, `@tag=1`, `:src.example`, `@tag=1 :src.example`, `9 y`, `0042 y`, `12345 y`, `NOT_A_CMD y`, `CMD2 y`, ` PING x`, `ü x` | `Err`; for `0042 y` the error text contains `0042 y` |
| P08 | `PING :abc\r\n`; `PING :abc\n`; `TOPIC #a :`; `QUIT` | `["abc"]`; `["abc"]`; `["#a", ""]`; no params, no source |
| P09 | `@a=one\stwo;b=semi\:colon;c=back\\slash;d=cr\rlf\n;e=\q;f=trail\;g=;h :s.example TAGMSG #x` (backslashes literal) | `a`=`one two`, `b`=`semi;colon`, `c`=`back\slash`, `d`=CR/LF as described, `e`=`q`, `f`=`trail`, `g`=`""`, `h`=`""` |
| P10 | `@k=1;;k=2; PING x` | `k`=`2`; serialises to `@k=2 PING x` |
| P11 | `:services.example.org NOTICE me :hi` | server source; `nick() == None`; serialises to `:services.example.org NOTICE me hi` |
| P12 | `:2001:db8::7 NOTICE me :hi` | server source |
| P13 | `:Helper NOTICE me :hi` | nick `Helper` |
| P14 | `:ann!~ann@203.0.113.9 JOIN #x` | nick `ann`; serialises back unchanged |
| P15 | `:bridge[m]@chat.example!relay@gw.example PRIVMSG #x :y` | nick `bridge[m]@chat.example` |
| P16 | `:ann!ident JOIN #x` | nick `ann`; serialises back unchanged (user without host) |
| P17 | `:ann@hostname NOTICE me :hi`; `:ann@host.example NOTICE me :hi` | nick `ann@hostname`; server source (`nick() == None`) |

## 8. `proto::serialise` (`src/proto/serialise.rs`)

### 8.1 Interface

```rust
pub fn serialise(message: &Message) -> String; // re-exported as crate::proto::serialise
```

Used as a function value (`.map(serialise)` over `&Message`), by `session/send.rs` to measure a tag, by
`test_support.rs` and by tests.

### 8.2 Behaviour

The output is the space-joined sequence of the parts that are present:

1. **Tags**, when there is at least one: `@` followed by the tags in ascending byte order of key, joined with `;`.
   A tag whose value is empty is written as its bare key; otherwise `key=escaped value`, escaping per Tags "Escaping
   values" (`\` → `\\`, `;` → `\:`, space → `\s`, CR → `\r`, LF → `\n`).
2. **Source**, when present: `:` then the server name, or the nick followed by `!user` when there is a user and by
   `@host` when there is a host.
3. **Command**.
4. **Parameters**, when there is at least one: all parameters joined by single spaces, where the last one is
   prefixed with `:` if and only if it is empty, contains a space, or starts with `:` (RFC 1459 §2.3.1 "trailing").
   Earlier parameters are written as they are (no validation).

Parsing the output gives back an equal message for any message that parsing could have produced.

### 8.3 Acceptance tests

| ID | Message | Expected text |
| --- | --- | --- |
| S01 | PRIVMSG `#a` `evening` | `PRIVMSG #a evening` |
| S02 | PRIVMSG `#a` `good evening` | `PRIVMSG #a :good evening` |
| S03 | PRIVMSG `#a` `""` | `PRIVMSG #a :` |
| S04 | PRIVMSG `#a` `:)` | `PRIVMSG #a ::)` |
| S05 | PRIVMSG without parameters | `PRIVMSG` |
| S06 | CHATHISTORY `LATEST` `bob` `*` `50` | `CHATHISTORY LATEST bob * 50` |
| S07 | TAGMSG `#a` with tags `zeta`=`1`, `+alpha`=`""`, `mid`=`x y` (added in that order) | `@+alpha;mid=x\sy;zeta=1 TAGMSG #a` |
| S08 | TAGMSG `#a` with `k` = `a;b c\d` CR LF `e` | `@k=a\:b\sc\\d\r\ne TAGMSG #a` |
| S09 | round trip of each line: `@+reply=a\sb;msgid=Zx1;time=2025-03-04T05:06:07.890Z :dave!d@gw.example PRIVMSG #ops :see above`; `:services.example.org 353 me = #ops :@dave +erin frank`; `@+typing=active :erin!e@h.example TAGMSG #ops`; `:frank!f@h.example PART #ops` | `serialise(parse(line)) == line` and parsing the result gives an equal message |

## 9. `proto` tests (`src/proto/tests.rs`)

The old test file is excluded. Write a new `proto/tests.rs` (declared `#[cfg(test)] mod tests;` in `proto/mod.rs`)
from sections 5–8. The acceptance tests C01–C11, P01–P17, M01–M03, S01–S09 and T01–T05 are supplied ready-made in
`acceptance/crate_root.rs` (crate-root module); you may move the proto and CTCP ones into `proto/tests.rs`. Do not
try to reconstruct the old file's inputs. Keep the codec tests in `codec.rs` (in a `#[cfg(test)] mod tests`) or in
the crate-root module, as you prefer.

## 10. `session` (`src/session/mod.rs`)

The per-network state machine. It never touches a socket: each call gives it one input (a server message, an owner
command, a clock tick, or the start of a connection) and the current time, and it returns an `Output`.

### 10.1 Interface

Module visibility: `mod session;` in `lib.rs`. This file MUST declare the existing private submodules and keep the
public re-exports that `lib.rs` uses:

```rust
mod autojoin; mod batch; mod caps; mod channel_info; mod channels; mod commands; mod describe;
mod dispatch; mod history; mod input; mod isupport; mod lines; mod listing; mod member_state;
mod membership; mod messages; mod metadata; mod numerics; mod registration; mod send; mod send_all;
mod spelling; mod split;
#[cfg(test)]
mod tests;

pub use input::{COMMANDS, CommandHelp, FirstArgument};
pub use send::{
    MAX_INLINE_BYTES, check_draft, check_inline_text, connection_batches, draft_line_limit, message_budget,
};
```

(`lib.rs` re-exports these to `spaces/irc`; see `interface-callers.md`.)

#### `Carry`: state kept between connections of one network

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carry {
    pub rejoin: Vec<String>,
    pub spellings: Vec<String>,
    pub left: Vec<String>,
    pub anchors: Vec<crate::model::Anchor>,
    // PLUS one private field holding the autojoin state, of type `autojoin::Autojoin`
    // (that type derives Debug, Clone, Default, PartialEq, Eq). The field name is yours.
}

impl Carry {
    pub fn forget(&mut self, command: &crate::model::Command);
}

pub(crate) fn configure_autojoin(
    carry: &mut Carry,
    config: &crate::model::NetworkConfig,
    command: &crate::model::Command,
) -> Vec<crate::model::Event>;
```

| `Carry` field | Content |
| --- | --- |
| `rejoin` | channel names to send JOIN for once the next connection is welcomed |
| `spellings` | the display spelling of every buffer name seen, so the next connection reports the same `BufferKey`s |
| `left` | channels the user removed from the sidebar (`Command::Forget`); the autojoin skips them until they are joined again; compared ignoring ASCII case |
| `anchors` | per buffer, the newest line the owner already has (`Command::Anchors` and tracked lines), so history requests ask only for newer lines |
| (private) | the initiative autojoin state (`autojoin::Autojoin`) |

Tests build `Carry { rejoin: …, ..Carry::default() }` and `Carry { anchors, ..Carry::default() }`, and read
`carry.rejoin`, `carry.left`, `carry.anchors`.

#### `Output`: the result of one session step

```rust
#[derive(Debug, Default, PartialEq)]
pub struct Output {
    pub send: Vec<crate::proto::Message>,
    pub events: Vec<crate::model::Event>,
    pub written: Vec<crate::model::Event>,
    pub unwritten: Vec<crate::model::Event>,
    pub quit: bool,
}

impl Output {
    fn send(&mut self, message: crate::proto::Message); // appends to `send`
    fn event(&mut self, event: crate::model::Event);    // appends to `events`
}
```

| `Output` field | Content |
| --- | --- |
| `send` | messages to write, in order |
| `events` | events to report before writing |
| `written` | events to report only after all of `send` was written successfully |
| `unwritten` | events to report, in place of `written`, if writing `send` failed |
| `quit` | set when the step asked to close the connection for good (QUIT); the network task then does not reconnect |

The two private methods are called by sibling modules (`out.send(…)`, `out.event(…)`) and MUST exist with these
names alongside the public fields.

#### `Session`

```rust
pub struct Session { /* see the field list below */ }

impl Session {
    pub fn new(config: crate::model::NetworkConfig, carry: Carry) -> Self;
    pub fn into_carry(self) -> Carry;
    pub fn abandon_history(&mut self) -> Vec<crate::model::Event>;
    pub fn abandon(&mut self) -> Vec<crate::model::Event>;
    pub fn tick(&mut self, now: chrono::DateTime<chrono::Utc>) -> Output;
    pub fn start(&mut self, now: chrono::DateTime<chrono::Utc>) -> Output;
    pub fn receive(&mut self, message: crate::proto::Message, now: chrono::DateTime<chrono::Utc>) -> Output;
    pub fn command(&mut self, command: crate::model::Command, now: chrono::DateTime<chrono::Utc>) -> Output;
    pub fn written(&mut self, events: &[crate::model::Event]);
    pub fn channel_names(&self) -> Vec<String>;
    pub fn network(&self) -> crate::model::NetworkId;
    pub fn registered(&self) -> bool;

    // Private, called by sibling modules:
    fn fold(&self, name: &str) -> String;           // folds with the current ISUPPORT case mapping
    fn is_own(&self, nick: &str) -> bool;           // fold(nick) == fold(our current nick)
    fn buffer(&self, target: crate::model::Target) -> crate::model::BufferKey; // this network + target
    fn has_cap(&self, cap: &str) -> bool;           // the capability is enabled
    fn refold(&mut self, names: Vec<String>);       // rebuilds the spelling table from `names` with the current fold
}
```

`Session` must be `Send` (it lives in a spawned tokio task). It needs no derives.

**Private fields that sibling modules read or write by name.** Their names and types are fixed by those modules:

| Field | Type | Meaning | Used by (see `interface-callers.md`) |
| --- | --- | --- | --- |
| `config` | `NetworkConfig` | the network's (secret-resolved) configuration | lines, listing, membership, autojoin, send |
| `reg` | a private struct with at least `nick: String` and `registered: bool` | the current nick (during registration, the candidate being tried) and the registered flag | lines, listing, metadata, membership (writes `reg.nick`), commands, send, autojoin |
| `caps` | `caps::Caps` | capability state (section 12) | send (`caps.offered`), tests (`caps.enabled`, `caps.offered`) |
| `isupport` | `isupport::Isupport` | ISUPPORT state (section 13) | many |
| `channels` | `BTreeMap<String, channels::Channel>` | channels we are in; key = folded channel name | channel_info, member_state, membership, metadata, numerics, autojoin |
| `queries` | `BTreeSet<String>` | private conversations the owner has open; entries are folded nicks | lines, membership, commands, history |
| `history` | `history::Requests` | CHATHISTORY requests sent and not yet answered, per folded target | history |
| `anchors` | `Vec<Anchor>` | per buffer, the latest line known to the owner | history, commands |
| `listing` | `listing::Listing` | state of LIST requests on this connection | listing |
| `statuses` | `metadata::Statuses` | nick status values | metadata, tests |
| `rejoin` | `Vec<String>` | channels to rejoin after registration | autojoin |
| `autojoin` | `autojoin::Autojoin` | initiative autojoin state | autojoin, membership |
| `left` | `Vec<String>` | channels the user removed | membership, tests |
| `spellings` | `spelling::Spellings` | display spelling per folded name | numerics |
| `sent_batches` | `u64` | number of outgoing batches so far (`send.rs` derives batch references from it) | send |
| `now` | `chrono::DateTime<chrono::Utc>` | the time given to the current step | lines, metadata |

Add any further private fields you need (for example open batches, SASL state lives in `caps`).

### 10.2 Behaviour

**`new(config, carry)`**: our nick is `config.nickname`, not registered; default capability and ISUPPORT state; no
channels, queries, open batches or pending history; `anchors`, `rejoin`, the autojoin state and `left` come from the
carry; spellings are the carry's spellings re-recorded under the default case mapping (`refold`); `sent_batches` is 0;
`now` is the current time.

**`start(now)`**: set `now`; open capability negotiation (section 12); return an `Output` whose `send` holds the
registration messages (section 15).

Every call that takes `now` stores it in `self.now` first, so lines built during the call default to that time.

**`receive(message, now)`** guarantees:

- a message that belongs to an open buffered batch (section 11) produces nothing until its batch ends; every other
  message is handled by `dispatch::handle(self, &message, &mut out)`;
- the returned events use canonical spellings (below);
- afterwards the anchors include the newest message lines among the returned events
  (`history::track(&mut self.anchors, &out.events)`).

**`command(command, now)`** guarantees:

- the command is handled by `commands::handle(self, command, &mut out)`;
- every message the call sends, except when the command is `Command::SetAutojoin`, is recorded in the autojoin state
  with `self.autojoin.record_manual(message, fold)` (`fold` being the session's current fold), so JOINs and PARTs the
  owner causes count as manual;
- `out.events`, `out.written` and `out.unwritten` use canonical spellings;
- afterwards the anchors include the newest message lines among `out.events` (lines in `out.written` count only once
  the network task calls `written`).

**Canonical spellings**: in every event the session returns, each channel or query name inside a `BufferKey`
(found with `spelling::buffers_mut` and `spelling::name_mut`) is the spelling `self.spellings.canonical(self.fold(name),
name)` gives: the first spelling recorded for that folded name, the given name being recorded when there is none yet.
So `#Room` and `#room` (or `[x]` and `{x}` under RFC 1459) name one buffer, spelled as first seen.

**`written(events)`**: `history::track(&mut self.anchors, events)`.

**`tick(now)`**: set `now`; `metadata::sync_due(self, &mut out)`; return `out`.

**`abandon_history()`**: the events of `history::abandon(self)`, with canonical spellings.

**`abandon()`**: `abandon_history()` followed by `metadata::clear_all(self)`. A second call returns nothing new.

**`into_carry(self)`**: if registered, first record the server's case mapping in the autojoin state
(`autojoin.remember_mapping(isupport.casemapping)`). Then:

| Carry field | Value |
| --- | --- |
| `rejoin` | `channel_names()` minus channels for which `autojoin.is_retired(name, fold)` holds |
| `spellings` | `spellings.names()` |
| `left`, `anchors`, autojoin state | moved from the session |

Note: `rejoin` is built from the channels joined on this connection only. A connection that never registered hands
on an empty `rejoin` (existing behaviour; see 18.2).

**`channel_names()`**: the stored names (as the server spelled them on JOIN) of joined channels, in ascending order of
folded name. **`network()`**: `config.id`. **`registered()`**: whether `001` arrived.

**`Carry::forget(command)`**:

| Command | Effect |
| --- | --- |
| `Anchors { anchors, .. }` | each anchor is merged with `history::remember(&mut self.anchors, anchor)` (keeps the newer per target, names compared ignoring ASCII case) |
| `Forget { buffer }` with a `Channel(name)` target | `name` is appended to `left` unless an ASCII-case-insensitive equal entry is already there |
| anything else, including `Forget` of a query or the server | nothing |

**`configure_autojoin(carry, config, command)`**: for any command other than `SetAutojoin { channels, .. }`, it
returns an empty list and changes nothing. For `SetAutojoin` it applies the new selection to the carried autojoin
state with `Autojoin::set(config, channels, mapping)`, where `mapping` is the case mapping that state remembers
(`Autojoin::mapping()`, ASCII when none), and then:

- `carry.rejoin` no longer contains any channel the autojoin state now treats as retired (`is_retired` under
  `mapping`'s fold);
- the result is `autojoin::retire_events(config, names)` for the channels `set` reported as dropped, each name
  replaced by the carried display spelling that folds (under `mapping`) to the same name, when there is one.

### 10.3 Acceptance tests

| ID | Scenario | Expected |
| --- | --- | --- |
| X01 | carry `spellings = ["#Room"]`; registered; server `:kim!k@h JOIN #ROOM` | `Event::Joined` for `#Room`; `channel_names() == ["#ROOM"]`; `into_carry()` gives `rejoin ["#ROOM"]`, `spellings ["#Room"]` |
| X02 | `Carry::forget` of Forget `#Ops`, Forget `#OPS`, Forget query `lee`; then Anchors for `#ops` at +5 s, +1 s, +9 s, +3 s; then `configure_autojoin` with a Forget | `left == ["#Ops"]`; `anchors` holds only the +9 s anchor; `configure_autojoin` returns nothing and leaves the carry unchanged |
| X03 | registered; `Command::Disconnect { reason: "see you" }` | sends `QUIT :see you`; `quit == true`; `network()` is the configured id |
| X04 | with `batch draft/chathistory draft/metadata-2`; own JOIN `#room` (history requested); `:s 761 kim lee status * :away for lunch`; `abandon()` twice | first call contains `History { #room, [], complete: false }` and `NickStatus { lee, None }`; second call empty |
| X05 | without `echo-message`; Say `hi` in `#room` gives a local echo; `written` with that echo moved to query `lee` 30 s later | sends `PRIVMSG #room hi`; carried anchors are for `#room` then `lee` |

The session tests in `session/tests.rs` and `session/tests/*` (section 17) are the main regression suite for this
module and its siblings.

## 11. `session::batch` (`src/session/batch.rs`)

Handles `BATCH` (Batch). Four batch types are buffered and processed when they end; every other batch is transparent
(its messages are handled immediately).

### 11.1 Interface

```rust
pub(super) fn batch(session: &mut Session, message: &Message, out: &mut Output);
```

Everything else is yours: in particular how the session stores open batches (a private `Session` field) and how
`Session::receive` sets aside messages of buffered batches before dispatching.

### 11.2 Behaviour

**Buffered types**: `chathistory`, `draft/chathistory`, `draft/multiline`, `znc.in/playback`. A batch is
*buffered* if its type is one of these, or if its parent batch is open and buffered.

**Setting messages aside** (in `Session::receive`): a message is set aside when it is not itself a `BATCH` command,
carries a `batch` tag, and that tag names an open buffered batch. It is appended to that batch. Every other message,
including messages tagged with an open batch that is not buffered and messages tagged with an unknown reference, is
dispatched at once.

**Opening** (`BATCH` whose first parameter does not start with `-`): the reference is the first parameter without its
leading `+`; the type is the second parameter; the batch's parameters are the third and later parameters; its parent
is the value of the opening message's own `batch` tag, if any; the opening message itself is kept (its tags matter
for multiline). Opening a reference that is already open replaces it.

**Closing** (`BATCH -ref`): remove the open batch `ref`; if there is none, do nothing. Otherwise:

1. Take the messages set aside for it, in arrival order; for `draft/multiline`, replace them by their combination
   (below).
2. If the batch has a parent that is open and buffered, append those messages to the parent's set-aside messages;
   they are then not acted on here.
3. Act by type (on whatever step 2 left):
   - `chathistory` or `draft/chathistory`: `history::deliver(session, target, &messages, out)`, where `target` is the
     batch's first parameter (or `""`);
   - `znc.in/playback`: dispatch each message in order with `dispatch::handle`, then turn every `Event::Line` those
     dispatches produced into `Event::Replayed` with the same line (other events are unchanged and keep their
     order);
   - any other type: dispatch each message in order with `dispatch::handle`.

Batches nest (Batch "Nested batches"): a multiline batch inside a chathistory batch becomes one combined message among
the history lines; any batch inside a buffered batch is itself buffered, and its messages join the parent's.

**Multiline combination** (Multiline): no messages combine to none. Otherwise the result is a single message equal to
the first set-aside message, except:

- its parameters are exactly two: the first message's first parameter (the target), and the text built by
  concatenating each message's last parameter in order, inserting `\n` before every message except the first,
  unless that message carries the `draft/multiline-concat` tag (then nothing is inserted);
- its tags: start from the first message's tags; remove `+draft/reply` (components' reply tags are ignored, see
  `session/tests/replies.rs`); then add every tag of the opening `BATCH` message, overriding equal keys (so the
  opening's `msgid`, `time` and `+draft/reply` win); finally remove `batch` and `draft/multiline-concat`.

The command and source are the first message's (PRIVMSG or NOTICE).

### 11.3 Acceptance tests

| ID | Server lines | Expected |
| --- | --- | --- |
| B01 | `@label=q1 :s BATCH +lr labeled-response`; `@batch=lr :s NOTICE kim :inside`; `:s BATCH -lr`; `@batch=nope :s NOTICE kim :unknown batch`; `:s BATCH -never-opened` | no event for the opening or closings; lines `inside` and `unknown batch` arrive as each message arrives |
| B02 | `@msgid=open1;time=2030-02-03T04:05:06.000Z :lee!l@h.example BATCH +ml draft/multiline #room`, then components `@batch=ml;msgid=part1;time=2001-01-01T00:00:00.000Z … :alpha`, `@batch=ml;draft/multiline-concat … :beta`, `@batch=ml … :` (empty), `@batch=ml … :gamma`; then `BATCH -ml` | nothing until the close; then one line with text `alphabeta\n\ngamma`, id `open1`, time 2030-02-03T04:05:06Z, in `#room`. An opened-and-closed empty multiline batch produces nothing |
| B03 | `:s BATCH +h chathistory #room`; `@batch=h :s BATCH +x example.org/other`; `@batch=x;msgid=n1 … PRIVMSG #room :nested`; `:s BATCH -x`; `@batch=h;msgid=n2 … PRIVMSG #room :direct`; `:s BATCH -h` (no request pending) | one `History` with lines `nested`, `direct` and `complete: false` (unrequested history is complete only when empty); no live lines |
| B04 | in `#room`; `:bnc.example BATCH +p znc.in/playback #room`; `@batch=p :lee!l@h.example JOIN #room`; `@batch=p … PRIVMSG #room :back`; `:bnc.example BATCH -p` | `Replayed` lines `joined` and `back`; no `Line`; the `MemberJoined` event is still reported |

Also covered by `session/tests/history.rs` (chathistory with nested multiline, playback, pass-through, older pages)
and `session/tests/replies.rs` (reply tags in batches).

## 12. `session::caps` (`src/session/caps.rs`)

Everything about capabilities: the initial negotiation, later NEW/DEL notifications, and SASL authentication with the
PLAIN or EXTERNAL mechanism (CAP, SASL).

### 12.1 Interface

```rust
#[derive(Debug, Default)]
pub struct Caps {
    pub offered: std::collections::BTreeMap<String, String>,
    pub enabled: std::collections::BTreeSet<String>,
    // private negotiation and SASL state
}

pub(super) fn cap(session: &mut Session, message: &Message, out: &mut Output);          // CAP
pub(super) fn authenticate(session: &mut Session, message: &Message, out: &mut Output); // AUTHENTICATE
pub(super) fn sasl_done(session: &mut Session, message: &Message, out: &mut Output);    // 903, 907
pub(super) fn sasl_failed(session: &mut Session, message: &Message, out: &mut Output);  // 902, 904, 905, 906
```

`offered` maps each capability the server listed to its value (`""` when it has none); `enabled` holds the
capabilities the server acknowledged. `offered.get(…)` is read by `session/send.rs` (multiline limits); tests remove entries from `enabled` and `offered`.
Keep the BTree types: the order of `Event::Capabilities { enabled }` is ascending. Add whatever methods
`session::registration` and `Session` need to open negotiation at start and to close it at `001`.

### 12.2 Capabilities requested

The client asks for each of these that the server lists, in exactly this order (the existing tests compare the whole
`CAP REQ` line), and appends `sasl` when eligible (12.3).

| # | Capability | Why the client wants it |
| ---: | --- | --- |
| 1 | `account-notify` | ACCOUNT messages update members' accounts |
| 2 | `account-tag` | `account` tag on messages fills `Line::account` |
| 3 | `away-notify` | AWAY messages update members' away state |
| 4 | `batch` | needed for chathistory, multiline, metadata and playback |
| 5 | `cap-notify` | NEW/DEL after registration (implicit with 302, requested for older servers) |
| 6 | `chghost` | host changes arrive as CHGHOST (ignored) instead of fake QUIT/JOIN |
| 7 | `draft/chathistory` | history on join, older pages, catch-up after anchors |
| 8 | `draft/event-playback` | JOIN/PART/etc. included in history (an Ergo draft extension) |
| 9 | `draft/metadata-2` | nickname `status` values; requested only when `batch` is offered or already enabled |
| 10 | `draft/multiline` | multi-line messages as one batch, sent and received |
| 11 | `draft/read-marker` | MARKREAD in both directions |
| 12 | `echo-message` | own messages come back with server ids and times (no local echo) |
| 13 | `extended-join` | account in JOIN |
| 14 | `invite-notify` | invites to others shown in the channel |
| 15 | `labeled-response` | requested for completeness; the client sends no labels |
| 16 | `message-tags` | client tags (`+typing`, `+draft/reply`) and `msgid` |
| 17 | `multi-prefix` | all prefixes in NAMES |
| 18 | `server-time` | `time` tag for line times |
| 19 | `setname` | real-name changes arrive as SETNAME (ignored) |
| 20 | `standard-replies` | FAIL/WARN/NOTE |
| 21 | `userhost-in-names` | NAMES entries as `nick!user@host` |

### 12.3 Behaviour

The rules below follow the CAP flow (list, request, wait for every answer, authenticate, end). To apply them the
module has to know what was offered and enabled, whether negotiation is still open, whether the last LS line has
arrived, whether any REQ is still unanswered, and whether SASL has started and whether it has finished.

**Opening** (from `Session::start`): negotiation is open.

**`cap`**: the subcommand is the second parameter, compared ignoring ASCII case.

| Subcommand | Effect |
| --- | --- |
| `LS`, `NEW` | Record every space-separated token of the last parameter as offered (`name=value`, split at the first `=`; value `""` without `=`), replacing earlier values. The line is a continuation when its third parameter is `*` and it has at least four parameters; otherwise it is the last line and the LS list is complete. On the last line only: build the list to request (12.2 order, then `sasl` if eligible), keeping capabilities that are offered and not enabled, and dropping `draft/metadata-2` unless `batch` is offered or enabled. If the list is not empty, send one `CAP REQ` whose single parameter is the space-joined list, and count one REQ awaiting an answer. On every LS/NEW line, finish by applying the progress rules. |
| `ACK` | For each token of the last parameter: `-name` removes `name` from enabled; otherwise add the token. If the third parameter is not `*`, one REQ is answered (count down, never below zero). If negotiation is closed, report `Event::Capabilities { network, enabled }`. Then apply the progress rules. Then `metadata::negotiate(session, out)`. |
| `NAK` | Enable nothing. If the third parameter is not `*`, one REQ is answered. Then apply the progress rules. |
| `DEL` | Remove each token of the last parameter from enabled and from offered. If negotiation is closed, report `Capabilities`. Then `metadata::negotiate(session, out)`. |
| anything else (`LIST`, …) | ignored |

`sasl` is eligible when negotiation is open, the configuration has SASL credentials, and `sasl` is offered with an empty
value or with a comma-separated mechanism list containing the configured mechanism (ASCII case-insensitive). The
configured mechanism is `EXTERNAL` when `sasl.external`, else `PLAIN`. So `sasl` is never requested after
registration.

**Progress rules**, applied in this order:

1. If negotiation is open, SASL has not started, `sasl` is enabled and credentials are configured: send
   `AUTHENTICATE <mechanism>`; SASL has now started.
2. Then, if negotiation is open, the last LS line has arrived, no REQ is unanswered, and SASL has either not started
   or finished: close negotiation, send `CAP END` and report `Capabilities` with the enabled list.

**`authenticate`**: only when the server's first parameter is exactly `+` and credentials are configured:

- EXTERNAL: send `AUTHENTICATE +` (empty authorisation identity).
- PLAIN: encode `username NUL username NUL password` (authzid = authcid = the configured username, PLAIN §2) in
  standard Base64 with padding; send it as `AUTHENTICATE <chunk>` messages of at most 400 bytes each, in order; if the
  final chunk is exactly 400 bytes long (or the encoding is empty), send one more `AUTHENTICATE +` (SASL 3.1
  "AUTHENTICATE" chunking).

Any other challenge sends nothing.

**`sasl_done`** (903 RPL_SASLSUCCESS, 907 ERR_SASLALREADY): add an Info line in the server buffer
(`numerics::server_line(session, message, LineKind::Info, out)`); SASL has finished; apply the progress rules.
**`sasl_failed`** (902, 904, 905, 906): the same with `LineKind::Error`. Registration continues without an account.
(900 RPL_LOGGEDIN is an Info line via `numerics::info`; 908 RPL_SASLMECHS is ignored; both are routed by `dispatch`.)

**Closing at `001`** (section 15): if negotiation is still open (the server ignored CAP, or completed registration
before `CAP END`), close it without sending `CAP END` and report `Capabilities` with the enabled list (possibly
empty).

`Event::Capabilities` always carries `network: config.id` and the enabled names in ascending order.

### 12.4 Acceptance tests

Configurations: nick `kim`; "PLAIN creds" = SASL `kim`/`pw`; "EXTERNAL" = SASL with `external: true`.

| ID | Scenario | Expected |
| --- | --- | --- |
| K01 | PLAIN creds; `:s CAP * LS * :server-time sasl=PLAIN unknown/x`, `:s CAP * LS * :away-notify message-tags`, `:s CAP * LS :batch account-tag` | nothing after the first two; then `CAP REQ :account-tag away-notify batch message-tags server-time sasl` |
| K02 | single LS line; (no creds, `sasl server-time`), (PLAIN creds, `sasl`), (PLAIN creds, `sasl=plain`), (EXTERNAL, `sasl=PLAIN server-time`), (EXTERNAL, `sasl=EXTERNAL,PLAIN`) | `CAP REQ server-time`, `CAP REQ sasl`, `CAP REQ sasl`, `CAP REQ server-time`, `CAP REQ sasl` |
| K03 | LS `draft/metadata-2` alone; LS `draft/metadata-2 batch` | `CAP END`; `CAP REQ :batch draft/metadata-2` |
| K04 | LS `batch echo-message server-time`; `:s CAP kim ACK * :batch echo-message`; `:s CAP kim ACK :server-time` | nothing (no `Capabilities`) after the first ACK; then `CAP END` and `Capabilities [batch, echo-message, server-time]` |
| K05 | LS `batch server-time`; NAK of both | `CAP END`; `Capabilities []` |
| K06 | PLAIN, LS+ACK `sasl`, `AUTHENTICATE +`: creds `kim`/`hunter2`; `a`×100/`b`×398; `a`×100/`b`×399; empty/empty | one message with the Base64 of `kim\0kim\0hunter2`; three messages (400, 400 bytes of payload, then `AUTHENTICATE +`); three messages (400, 400, 4); `AUTHENTICATE AAA=` |
| K07 | EXTERNAL; LS `sasl=EXTERNAL`, ACK; `AUTHENTICATE abc`; `AUTHENTICATE +` | nothing; `AUTHENTICATE +` |
| K08 | PLAIN, ACK `sasl`, `AUTHENTICATE +`, then each of 903, 907, 902, 904, 905, 906 as `:s NNN kim :outcome text` | no `CAP END` while waiting; then `CAP END` and one line `outcome text`: Info for 903/907, Error for the others |
| K09 | registered with `cap-notify` (PLAIN creds); `NEW :sasl invite-notify`; `ACK :invite-notify`; `DEL :invite-notify`; `NEW :invite-notify`; `LIST :cap-notify` | `CAP REQ invite-notify` (no sasl); then `Capabilities [cap-notify, invite-notify]`, no message; `Capabilities [cap-notify]`; `CAP REQ invite-notify` again; no event for LIST |

Existing coverage: `session/tests/registration.rs` (full Ergo LS, SASL PLAIN/EXTERNAL, chunking, NAK, no-CAP
servers, cap-notify) and `session/tests/metadata.rs` (metadata needs batch; late ACK subscribes once).

## 13. `session::isupport` (`src/session/isupport.rs`)

Server feature advertisement from numeric 005 (Modern "RPL_ISUPPORT Parameters"), reduced to the values the session
needs, and case mapping.

### 13.1 Interface

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Casemapping {
    Ascii,
    Rfc1459,
    StrictRfc1459,
}

impl Casemapping {
    pub fn fold(self, name: &str) -> String;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isupport {
    pub chantypes: Vec<char>,
    pub prefix: Vec<(char, char)>,
    pub network: Option<String>,
    pub casemapping: Casemapping,
    pub chathistory: u16,
    pub statusmsg: Vec<char>,
    pub list_modes: String,
    pub param_modes: String,
    pub set_param_modes: String,
    pub whox: bool,
}

impl Default for Isupport { /* table below */ }

impl Isupport {
    pub fn apply(&mut self, token: &str);
    pub fn fold(&self, name: &str) -> String;
    pub fn channel<'a>(&self, target: &'a str) -> Option<&'a str>;
    pub fn symbols(&self) -> Vec<char>;
}
```

| `Isupport` field | Holds |
| --- | --- |
| `chantypes` | characters that start a channel name (CHANTYPES) |
| `prefix` | (mode letter, prefix symbol) pairs from PREFIX, highest rank first |
| `network` | the NETWORK name, if given |
| `casemapping` | the CASEMAPPING in force |
| `chathistory` | the CHATHISTORY limit on messages per request; 0 when not given |
| `statusmsg` | STATUSMSG prefix characters (as in `@#channel`) |
| `list_modes`, `param_modes`, `set_param_modes` | CHANMODES types A, B and C: modes that take a parameter (always, always, only when set) |
| `whox` | whether WHOX is supported |

`Casemapping` is stored in `autojoin::Autojoin` (which derives `Debug, Clone, Default, PartialEq, Eq` and copies it
out of an `Option`), hence `Copy`. `Casemapping::Ascii` is named by `autojoin.rs`. `network` and `statusmsg` are not
read outside this module but keep them public.

### 13.2 Behaviour

**Folding** (RFC 1459 §2.2; Modern "CASEMAPPING"): every character is mapped individually; non-ASCII characters are
left unchanged.

| Mapping | ASCII `A`–`Z` | `[` `]` `\` | `~` |
| --- | --- | --- | --- |
| `Ascii` | lower-case | unchanged | unchanged |
| `Rfc1459` | lower-case | `{` `}` `\|` | `^` |
| `StrictRfc1459` | lower-case | `{` `}` `\|` | unchanged |

**Defaults** (before any 005):

| Field | Default |
| --- | --- |
| `chantypes` | `#`, `&` |
| `prefix` | (`o`, `@`), (`v`, `+`) |
| `network` | `None` |
| `casemapping` | `Rfc1459` |
| `chathistory` | 0 |
| `statusmsg` | empty |
| `list_modes`, `param_modes`, `set_param_modes` | `beI`, `k`, `l` |
| `whox` | false |

**`apply(token)`**: split at the first `=` into key and value (value `""` without `=`). In the value, replace the
escapes `\x20` with a space and `\x5C` with a backslash (Modern: ISUPPORT values may use `\xHH`). Match the key
ignoring ASCII case:

| Key | Effect |
| --- | --- |
| `CHANTYPES` | `chantypes` = the value's characters (an empty value means no channel types) |
| `PREFIX` | the value must be `(modes)symbols`; `prefix` = the modes zipped pairwise with the symbols (extra characters on either side dropped). A value not starting with `(` or without `)` gives an empty list. |
| `NETWORK` | `network` = `Some(value)` |
| `CASEMAPPING` | value compared ignoring ASCII case: `ascii` → `Ascii`, `strict-rfc1459` → `StrictRfc1459`, anything else (including `rfc1459`, `rfc7613`) → `Rfc1459` |
| `CHATHISTORY`, `DRAFT/CHATHISTORY` | `chathistory` = the value as `u16`, or 0 when it does not parse (including values over 65535) |
| `STATUSMSG` | `statusmsg` = the value's characters |
| `CHANMODES` | the value split at `,`: the first three parts become `list_modes`, `param_modes`, `set_param_modes`; a missing part becomes `""`; further parts are ignored |
| `WHOX` | `whox` = true |
| `-WHOX` | `whox` = false |
| anything else | ignored (including other `-KEY` negations; see 18.2) |

**`channel(target)`**: strip all leading characters that are in `statusmsg` (so `@+#ops` → `#ops` when STATUSMSG has
`@` and `+`); return the remainder if it starts with one of `chantypes`, else `None`.

**`symbols()`**: the second element of each `prefix` pair, in order.

The 005 handler (`numerics::isupport`, not replaced) applies each token from the second parameter up to but excluding
the last, then calls `Session::refold` with the current spellings and `metadata::refold`.

### 13.3 Acceptance tests

| ID | Scenario | Expected |
| --- | --- | --- |
| I01 | `Isupport::default()` | the defaults table; `symbols() == ['@', '+']` |
| I02 | apply `PREFIX=(qaohv)~&@%+`, `CHANTYPES=#!`, `NETWORK=Example\x20Net\x5Cwork` (backslashes literal), `chathistory=250`, `STATUSMSG=@+`, `CHANMODES=beI,k,lj,imnpst`, `whox`, `UNKNOWN=1` | symbols `~&@%+`, first prefix (`q`, `~`); chantypes `#!`; network `Example Net\work`; chathistory 250; statusmsg `@+`; modes `beI`, `k`, `lj`; whox true |
| I03 | then `-WHOX`; `DRAFT/CHATHISTORY=50`; `CHATHISTORY=lots`; `CHATHISTORY=70000`; `CHANMODES=b,k` | whox false; 50; 0; 0; `set_param_modes` empty |
| I04 | `PREFIX=`, `PREFIX`, `PREFIX=ov@+` on fresh defaults | empty prefix each |
| I05 | `CASEMAPPING=` `ascii`, `ASCII`, `strict-rfc1459`, `rfc1459`, `rfc7613` | Ascii, Ascii, StrictRfc1459, Rfc1459, Rfc1459 |
| I06 | fold `Nick[A]\~ÉCOLE` (backslash literal) | Ascii `nick[a]\~École`; Rfc1459 `nick{a}\|^École`; StrictRfc1459 `nick{a}\|~École`; with ASCII mapping `fold("#Ops[1]") == "#ops[1]"` |
| I07 | `channel`: `#ops`, `&local`, `@#ops` (no STATUSMSG), `bob`; after `STATUSMSG=@+`: `@+#ops`, `+bob`; after `CHANTYPES=`: `#ops` | `#ops`, `&local`, None, None; `#ops`, None; None |

## 14. `session::messages` (`src/session/messages.rs`)

Handlers for conversation traffic: private and channel messages and notices, automatic CTCP answers, typing
notifications, read markers, and FAIL/WARN/NOTE.

### 14.1 Interface

All called by `dispatch.rs`:

```rust
pub(super) fn privmsg(session: &mut Session, message: &Message, out: &mut Output);
pub(super) fn notice(session: &mut Session, message: &Message, out: &mut Output);
pub(super) fn tagmsg(session: &mut Session, message: &Message, out: &mut Output);
pub(super) fn markread(session: &mut Session, message: &Message, out: &mut Output);
pub(super) fn standard_reply(session: &mut Session, message: &Message, out: &mut Output); // FAIL/WARN/NOTE, except METADATA
```

### 14.2 Behaviour

The helpers named here live in visible modules (section 16).

**PRIVMSG**:

1. Target buffer: `lines::message_target(session, message)` (a channel when the first parameter is a channel; a query
   with the recipient when we sent it; a query with the sender otherwise; the server buffer without a sender).
2. If the target is a query, add the folded nick to `session.queries` (an open query takes that nick's notices).
3. CTCP reply: if the text (last parameter) is a CTCP query (`ctcp::parse`), the sender is a user and is not us
   (`session.is_own`), reply with `NOTICE <sender nick> :\x01<COMMAND>[ <reply>]\x01` (the space and reply are
   omitted when the reply is empty):

   | Query | Reply text |
   | --- | --- |
   | `VERSION` | `Subspace IRC ` followed by the crate's version (`env!("CARGO_PKG_VERSION")`) |
   | `PING` | the query's parameters, unchanged (may be empty) |
   | `TIME` | `session.now` as an RFC 2822 date-time in UTC as chrono's `to_rfc2822` writes it, for example `Fri, 7 Jun 2030 08:09:10 +0000` |
   | anything else (including `ACTION`, `CLIENTINFO`) | no reply |

   Replies are sent for CTCP in channels too (to the sender's nick). (CTCP: "VERSION", "PING", "TIME".)
4. Line: if `describe::message(message)` gives a kind and text (PRIVMSG text, or ACTION for CTCP ACTION; nothing for
   other CTCP), add `Event::Line(lines::line(session, message, target, kind, text))`.

**NOTICE**: target `lines::notice_target(session, message)` (channels stay; a query only when registered and that
query is open; otherwise the server buffer); then the line as in PRIVMSG step 4 (a CTCP reply inside a NOTICE makes no
line). No CTCP replies to notices.

**TAGMSG**: target as for PRIVMSG; if the sender is not us and the `+typing` tag is exactly `active`, `paused` or
`done`, add `Event::Typing { buffer: session.buffer(target), nick: <sender nick, or "">, state }`. Anything else adds
nothing (Typing).

**MARKREAD** (Read marker; server to client: `MARKREAD <target> timestamp=<time>`): the buffer is
`Target::Channel(c)` when `session.isupport.channel(first parameter)` gives `c`, else `Target::Query(first
parameter)`. If the second parameter starts with `timestamp=` and the rest parses with `lines::parse_time`
(RFC 3339), add `Event::ReadMarker { buffer, time }`; otherwise (for example `*`) nothing.

**Standard replies** (`FAIL`, `WARN`, `NOTE` whose first parameter is not `METADATA`; Standard replies):

1. `listing::failed(session, message)` (a `FAIL LIST …` ends a running LIST);
2. if the command is `FAIL` and the first parameter is `CHATHISTORY`: `history::failed(session, message, out)` (ends
   the pending requests it names);
3. add a line in the server buffer, kind `Error` for FAIL and `Info` for WARN and NOTE, with text
   `<param 0> <param 1>: <last parameter>`, built with `lines::line`.

### 14.3 Acceptance tests

| ID | Scenario | Expected |
| --- | --- | --- |
| G01 | registered with `echo-message`; from `lee`: `PRIVMSG #room :\x01VERSION\x01`, `PRIVMSG kim :\x01PING\x01`, `PRIVMSG kim :\x01TIME\x01`, `PRIVMSG kim :\x01CLIENTINFO\x01`; from `kim`: `PRIVMSG lee :\x01VERSION\x01`; from `lee`: `NOTICE kim :\x01VERSION other 1.0\x01`; session time 2030-06-07T08:09:10Z | exactly three messages: `NOTICE lee :\x01VERSION Subspace IRC <version>\x01`, `NOTICE lee \x01PING\x01`, `NOTICE lee :\x01TIME Fri, 7 Jun 2030 08:09:10 +0000\x01`; no lines |
| G02 | `@+typing=paused :lee… TAGMSG #room`; `@+typing=active :lee… TAGMSG kim`; `@+typing=bogus … #room`; untagged TAGMSG; `@+typing=active :kim… TAGMSG #room` | exactly two events: Typing Paused in `#room` from `lee`; Typing Active in query `lee` |
| G03 | `:s MARKREAD lee timestamp=2030-01-01T00:00:00.000Z`; `:s MARKREAD #room timestamp=not-a-time`; `:s MARKREAD #room` | exactly one `ReadMarker` for query `lee` at 2030-01-01T00:00:00Z |
| G04 | `:s NOTE * OPER_MESSAGE :The server restarts at noon`; `:s FAIL JOIN CHANNEL_FULL #room :Channel is full` | server-buffer lines `* OPER_MESSAGE: The server restarts at noon` (Info) and `JOIN CHANNEL_FULL: Channel is full` (Error) |
| G05 | registered; `NOTICE kim :before` from `lee`; then `PRIVMSG kim :hello` from `lee`; `NOTICE kim :after` from `LEE` | `before` in the server buffer; `hello` and `after` in query `lee` (the notice is a Notice line) |

Existing coverage: `session/tests/messages.rs` (time, ids, accounts, highlights, queries, echo, CTCP, typing,
MARKREAD, standard replies), `session/tests/replies.rs`, `session/tests/history.rs` (failed chathistory).

## 15. `session::registration` (`src/session/registration.rs`)

Connection registration: the opening messages, retrying with other nicks, what happens on numeric 001, and the PING and
ERROR handlers.

### 15.1 Interface

Called by `dispatch.rs`, and `additions` by `autojoin.rs`:

```rust
pub(super) fn ping(session: &mut Session, message: &Message, out: &mut Output);       // PING
pub(super) fn error(session: &mut Session, message: &Message, out: &mut Output);      // ERROR
pub(super) fn welcome(session: &mut Session, message: &Message, out: &mut Output);    // 001
pub(super) fn nick_error(session: &mut Session, message: &Message, out: &mut Output); // 432, 433, 436, 437
pub(super) fn additions(session: &Session, before: &std::collections::BTreeSet<String>) -> Vec<Message>;
```

How `Session::start` obtains the registration messages is yours.

### 15.2 Behaviour

**Registration messages** (Modern "Connection Registration"; RFC 2812 §3.1), in this order:

1. `CAP LS 302`
2. `PASS <config.password>`, only when the password is not empty (the server password, not the account password)
3. `NICK <config.nickname>`
4. `USER <username> 0 * <realname>`, where `username` is `config.username` or, when empty, the nickname, and
   `realname` is `config.realname` or, when empty, the nickname (serialisation adds the `:` when the real name has a
   space)

**PING**: send `PONG` with the same parameters (RFC 2812 §3.7.2–3.7.3). PONG from the server is ignored by dispatch.

**ERROR**: an Error line in the server buffer via `numerics::server_line` (its text is the parameters joined with
spaces).

**`nick_error`** (432, 433, 436, 437):

- Before registration: the next nick is `config.alt_nicks[n]` where `n` is the number of fallbacks already tried, or,
  once the alternatives are used up, the nick last tried with `_` appended. Record it as our nick, count the attempt,
  and send `NICK <next>`. No line is shown.
- After registration: send nothing; add an Error line in the server buffer with `numerics::text(message)` (the
  parameters after our nick, joined with spaces), built with `lines::line`.

**`welcome`** (001, RPL_WELCOME), in this order:

1. our nick becomes the first parameter (the server may have changed it); registered becomes true;
2. if negotiation is still open, close it and report `Capabilities` (section 12.3, "Closing at 001"); no `CAP END`;
3. report `Event::OwnNick { network, nick }`;
4. report `Event::Status { network, status: ConnectionStatus::Connected { nick } }`;
5. add an Info line in the server buffer (`numerics::server_line`);
6. `metadata::negotiate(session, out)` (may send `METADATA * SUB status`);
7. send the autojoin JOINs:
   - *configured*: `session.autojoin.entries(&config.channels, fold)` with `fold` = ASCII lower-casing (ASCII, not
     RFC 1459, at this point: see `session/tests/forget.rs`), then
   - *rejoin*: `session.rejoin`,

   each list first filtered to drop entries whose channel (the first word of the entry) matches a `session.left`
   entry ignoring ASCII case. An entry is `#channel` or `#channel key`. Walk configured then rejoin entries in order,
   skipping an entry whose channel was already seen (compared by ASCII lower case) and sending `JOIN <channel>` or
   `JOIN <channel> <key>` for the rest;
8. send `history::resume_queries(session)` (catch-up requests for anchored private conversations).

**`additions(session, before)`**: the entries of `session.autojoin.entries(&config.channels, fold)` (with `fold` the
session's current fold) whose folded channel name (`autojoin::name(entry)`, folded) is not in `before`, filtered
against `left` and turned into JOINs exactly as in welcome step 7 (with an empty rejoin list).

### 15.3 Acceptance tests

| ID | Scenario | Expected |
| --- | --- | --- |
| R01 | `start` with password `pw`, username `uid`, real name `Kim Example`; and with none of them | `CAP LS 302`, `PASS pw`, `NICK kim`, `USER uid 0 * :Kim Example`; `CAP LS 302`, `NICK kim`, `USER kim 0 * kim`; not registered |
| R02 | registered; `PING irc.acceptance.test :t 1` | `PONG irc.acceptance.test :t 1` |
| R03 | alt nick `kim2`; 433 for `kim`, 432 for `kim2`, 436 for `kim2_`, 437 for `kim2__`; then `001 kim2___`; then `:s 433 kim2___ kim :Nickname is already in use` | `NICK kim2`, `NICK kim2_`, `NICK kim2__`, `NICK kim2___`, no lines; `OwnNick kim2___`; no message and an Error line `kim Nickname is already in use` in the server buffer |
| R04 | channels `#alpha key1`, `#beta`; carry rejoin `#BETA`, `#gamma` and an anchor for query `lee` (id `m9`); LS+ACK `batch draft/metadata-2 draft/chathistory`; `:s 001 kim :Welcome kim` | sends `METADATA * SUB status`, `JOIN #alpha key1`, `JOIN #beta`, `JOIN #gamma`, `CHATHISTORY LATEST lee msgid=m9 500`; events in order OwnNick, Status Connected `kim`, Info line `Welcome kim` |
| R05 | no CAP reply; `:legacy.example 001 kim :Hi`; then `ERROR :Closing Link: kim (Bye)` | `Capabilities []` and no `CAP END`; Error line `Closing Link: kim (Bye)` |
| R06 | `:s 001 kim_truncated :Hi` | `OwnNick kim_truncated` |

Existing coverage: `session/tests/registration.rs`, `session/tests/autojoin.rs`, `session/tests/forget.rs`,
`session/tests/anchors.rs`, `session/tests/metadata.rs`.

## 16. Helpers in visible modules

These functions live in files you may read. They already produce behaviour this document requires, so the simplest
route to identical behaviour is to call them. Signatures at the baseline before the rewrite:

| Module | Item | Use in the rewritten modules |
| --- | --- | --- |
| `session::dispatch` | `handle(session: &mut Session, message: &Message, out: &mut Output)` | routes every live message (the routing table names the handlers in sections 11–15) |
| `session::commands` | `handle(session: &mut Session, command: Command, out: &mut Output)` | all owner commands (`Session::command`) |
| `session::lines` | `line(session, message, target, kind, text) -> Line`; `local_line(…)`; `message_target(session, message) -> Target`; `notice_target(…) -> Target`; `query_nick(&Target) -> Option<&str>`; `parse_time(&str) -> Option<DateTime<Utc>>`; `format_time(DateTime<Utc>) -> String` | building lines (time from `server-time` or `session.now`, `msgid`, `account`, `+draft/reply`, highlight, own), routing PRIVMSG/NOTICE/TAGMSG, parsing MARKREAD times |
| `session::describe` | `message(&Message) -> Option<(LineKind, String)>`; `event(…)`; `history_line(…)` | PRIVMSG/NOTICE/ACTION kind and text |
| `session::numerics` | `server_line(session, message, kind, out)`; `text(&Message) -> String`; `info(…)`; `isupport(…)` (the 005 handler) | server-buffer lines for 001, ERROR, SASL outcomes and nick errors |
| `session::history` | `deliver(session, target, &[Message], out)`; `failed(session, message, out)`; `abandon(session) -> Vec<Event>`; `track(&mut Vec<Anchor>, &[Event])`; `remember(&mut Vec<Anchor>, Anchor)`; `resume_queries(session) -> Vec<Message>`; type `Requests` | chathistory delivery, failures, abandonment, anchors |
| `session::metadata` | `CAP` (= `"draft/metadata-2"`); `negotiate(session, out)`; `clear_all(session) -> Vec<Event>`; `sync_due(session, out)`; type `Statuses` | capability filter, subscription after ACK/DEL/001, abandonment, ticks |
| `session::listing` | `failed(session, message)`; type `Listing` | FAIL LIST in standard replies |
| `session::autojoin` | type `Autojoin` with `mapping()`, `remember_mapping(m)`, `entries(local, fold)`, `set(config, channels, mapping)`, `is_retired(name, fold)`, `record_manual(message, fold)`; `name(entry) -> &str`; `retire_events(config, &[String]) -> Vec<Event>` | carry, welcome JOINs, `additions`, `configure_autojoin`, `Session::command` |
| `session::spelling` | type `Spellings` with `canonical(key, name)`, `names()`, `refold(names, fold)`; `buffers_mut(&mut Event)`; `name_mut(&mut Target)` | respelling and carry |
| `session::send` | `REPLY` (= `"+draft/reply"`) | multiline combination |
| `session::channels` | type `Channel { pub name, pub topic, pub members, pub names, pub who }` | the `channels` field |
| `network::offline` | `reply(&NetworkConfig, &Command) -> Option<Event>` | commands while no session runs |
| `network::secrets` | `resolve(&NetworkConfig) -> Result<NetworkConfig, String>` | the SASL password file, read at each connect |

`model.rs` defines every public type (`NetworkConfig`, `Command`, `Event`, `Line`, `ConnectionStatus`, …) and
documents their meaning.

## 17. Verification

### 17.1 Existing tests that must keep passing

Test run at the baseline before the rewrite (`mise exec -- cargo test -p subspace-client-irc --locked`): 175 unit tests, 5 Ergo
integration tests (they skip unless `IRC_TEST_SERVER` is set) and 1 example test, all passing.

| Test module (under `clients/irc/src/` unless noted) | Tests | Exercises |
| --- | ---: | --- |
| `codec.rs` (inside an excluded file) | 4 | replaced by C01–C11 |
| `proto/tests.rs` (excluded) | 10 | replaced by P/M/S/T tests |
| `network/tests.rs` | 5 | network task, timeouts, backoff, carry, history abandonment |
| `network/tests/metadata.rs` | 1 | abandonment clears statuses once |
| `network/tests/send_all.rs` | 4 | `Link::command`, delivery ordering, write failures |
| `network/offline.rs` (tests module) | 4 | offline replies |
| `network/secrets.rs` (tests module) | 3 | password file |
| `session/tests.rs` + `session/tests/*.rs` | 113 | the whole session through `Session`: anchors 5, autojoin 5, forget 4, history 8, listing 4, members 8, messages 12 (+ all_or_nothing 5, oversized 9, paragraphs 9), metadata 9 (+ lifecycle 7), registration 9, replies 13, spelling 6 |
| `session/channels.rs`, `session/input/tests.rs`, `session/metadata/text.rs`, `session/send/limit_tests.rs`, `session/split.rs` | 19 | helpers (indirectly use `Message`) |
| `model.rs`, `model/replies/tests.rs`, `format/tests.rs` | 12 | public types and formatting |
| `tests/ergo.rs` (+ `tests/ergo/*.rs`) | 5 | end to end against Ergo: two clients talk, anchors, atomic sends, metadata |
| `examples/irc_smoke.rs` | 1 | argument parsing |

The `subspace-irc-board` crate (`spaces/irc`) uses the public API and `test_support::Rehearsal` (which drives
`Session::new/start/receive/command/written` and reads `Output`), including `spaces/irc/src/tests/uploads/rehearse.rs`
and the Ergo agent-tool tests in `spaces/irc/src/agent_tools/tests/ergo.rs`.

### 17.2 Commands

From `subspace-desktop/` (run Cargo through mise in non-interactive shells):

```sh
mise exec -- cargo test -p subspace-client-irc --locked
mise exec -- cargo test -p subspace-irc-board --locked
scripts/irc-test-server.sh &        # Ergo 2.19.1 on 127.0.0.1:16667
IRC_TEST_SERVER=127.0.0.1:16667 mise exec -- cargo test -p subspace-client-irc --test ergo
IRC_TEST_SERVER=127.0.0.1:16667 mise exec -- cargo test -p subspace-irc-board agent_tools
make check                          # architecture, docs, fmt, cargo-shear, clippy -D warnings, tests
mise run quality                    # rustqual must not drop below rustqual-baseline.json
```

Also run the app against a local Ergo (Channels Space: connect, join, send, receive, history, typing, reconnect after
killing the server) as a smoke test.

### 17.3 Ready-made acceptance tests

`acceptance/` holds the acceptance tests of this document as Rust test modules, written fresh for this document from
the specifications. All 67 pass against the baseline implementation (242 tests in total with the existing 175), and
they are `clippy -D warnings` and `rustfmt` clean:

| File | Tests | Place it at | Register with |
| --- | ---: | --- | --- |
| `acceptance/crate_root.rs` | 29 (C, P, M, S, T, N) | `clients/irc/src/cleanroom_acceptance.rs` (or split into `codec.rs`, `proto/tests.rs`, `connection/mod.rs` test modules) | `#[cfg(test)] mod cleanroom_acceptance;` in `lib.rs` |
| `acceptance/session.rs` | 33 (R, K, I, G, B, X) | `clients/irc/src/session/cleanroom_acceptance.rs` | `#[cfg(test)] mod cleanroom_acceptance;` in `session/mod.rs` |
| `acceptance/network.rs` | 5 (W01–W05) | `clients/irc/src/network/cleanroom_acceptance.rs` | `#[cfg(test)] mod cleanroom_acceptance;` in `network.rs` |

They use only the interfaces in this document (plus `Casemapping`, `Isupport`, `configure_autojoin`, `run_with`,
`backoff`, `LineBuffer`, `encode`, `connect`, `Timeouts` and the three `codec` limits). Some IDs in the tables bundle
two Rust tests or vice versa; the Rust files are authoritative for exact inputs.

## 18. Choices left to the implementer, and known oddities

### 18.1 Free choices

- All private representations and helper structure: how tags are stored, the `Source`/user types, the batch store,
  capability negotiation state, the registration state beyond `nick` and `registered`, error type names and their
  variants, and the internal split of `proto` into files.
- The wording of TLS error texts (after the fixed `TLS: ` prefix) and of the parse error (which must contain the
  line).

### 18.2 Existing behaviour that looks odd

This document describes the baseline. These points are deliberate parts of the baseline unless noted; an
"improvement" needs a new test and a note in the change.

1. `into_carry` builds `rejoin` from this connection's joined channels only, so a connection that drops before
   `001` hands on an empty `rejoin` and forgets earlier manual joins. Keeping the carried list when the session
   never registered would be an improvement.
2. A chathistory batch nested inside another buffered batch hands its lines to the parent, and the baseline also
   delivers an empty `History` for its own target (consuming a pending request). No server nests history like this;
   skipping the empty delivery is acceptable.
3. ISUPPORT negations other than `-WHOX` are ignored, and only the `\x20` and `\x5C` value escapes are decoded.
   Resetting a negated parameter to its default, and decoding other `\xHH` escapes, are spec-conformant improvements.
4. `FAIL CHATHISTORY` is matched case-sensitively on the first parameter.
5. The reconnect `attempt` count is not reset by an owner `Quit` (only by a `Disconnect` while waiting, or by a loss
   after registration).
6. Messages inside a `znc.in/playback` batch are dispatched as live messages, so a replayed CTCP query from someone
   else is answered.
7. `AUTHENTICATE +` from the server is answered whenever credentials are configured, whatever the SASL phase.
8. A pending keep-alive PING is cleared by any input, not only by the matching PONG.
9. A certificate the verifier rejects is a handshake failure and is retried with backoff, indefinitely.

### 18.3 Not pinned down by automated tests

- Real TLS handshakes (accept-invalid, extra CA roots, client certificates): manual checks T-M1–T-M4.
- Keep-alive timing (120 s idle, 60 s answer, 10 s tick): W06 needs tokio's `test-util` feature.
- Exact `Display` texts for IO and rustls errors (they come from the libraries).

## 19. Clean-room procedure

1. The implementer works from: this document, `interface-callers.md`, `excluded.md`, the `acceptance/` tests, the
   normative references in 0.5, crate documentation for the dependencies, and the visible repository files.
2. The implementer MUST NOT open any file or text listed in `excluded.md`, the repository's history of those files,
   or the source of other IRC clients and libraries (in particular Halloy, <https://github.com/squidowl/halloy>).
3. Questions go to the owner in writing; answers that describe behaviour are added to this document (with a version
   note), never as code.
4. Record provenance in each commit message body: "Clean-room implementation from the functional specification
   (version of 19 November 2026) and the IRC/IRCv3 specifications; the author has not read the replaced code."
5. After the rewrite, the owner (not the implementer) updates the texts that mention the adaptation:
   `subspace-desktop/AGENTS.md` (Structure: "adapted from Halloy (GPL-3.0)") and
   `subspace-desktop/docs/changes/irc-board/spec.md` (Intent and Crates paragraphs). The pre-rewrite history in the source repository
   contained the earlier GPL-3.0-or-later files. Those versions were excluded from the isolated repository.
