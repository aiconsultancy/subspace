//! The public contract between the IRC runtime and its owners: network
//! configuration, commands in, events out. The runtime owns sockets and
//! protocol state; owners only see these types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod replies;
mod send_result;
pub use replies::ThreadRoots;
pub use send_result::SendResult;

/// A configured network's stable key, such as `libera`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NetworkId(pub String);

/// SASL credentials. Only `PLAIN` and `EXTERNAL` are offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sasl {
    pub username: String,
    #[serde(default)]
    pub password: String,
    /// A file holding the password, read at each connect and never stored;
    /// used when `password` is empty.
    #[serde(default)]
    pub password_file: Option<std::path::PathBuf>,
    #[serde(default)]
    pub external: bool,
}

/// One network from `networks` in the Channels Space's settings file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkConfig {
    pub id: NetworkId,
    /// Display name; defaults to the id.
    #[serde(default)]
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_true")]
    pub tls: bool,
    /// Only for local test servers.
    #[serde(default)]
    pub accept_invalid_certs: bool,
    pub nickname: String,
    #[serde(default)]
    pub alt_nicks: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub realname: String,
    /// Server password (`PASS`), not the account password.
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub sasl: Option<Sasl>,
    /// Channels joined after registration, such as `#subspace`.
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default = "default_true")]
    pub auto_connect: bool,
    /// Words that highlight a line, besides our own nick (case-insensitive,
    /// whole words).
    #[serde(default)]
    pub highlights: Vec<String>,
    /// PEM CA certificate(s) trusted for this server, besides the system roots,
    /// such as a private network's own CA.
    #[serde(default)]
    pub ca_cert: Option<std::path::PathBuf>,
    /// PEM client certificate for TLS (and SASL `EXTERNAL`).
    #[serde(default)]
    pub client_cert: Option<std::path::PathBuf>,
    /// PEM private key for `client_cert`; defaults to the certificate file.
    #[serde(default)]
    pub client_key: Option<std::path::PathBuf>,
}

fn default_port() -> u16 {
    6697
}

fn default_true() -> bool {
    true
}

/// Where a line belongs within one network.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Target {
    /// Server notices, MOTD, numerics and errors.
    Server,
    Channel(String),
    /// A private conversation, keyed by the other nick.
    Query(String),
}

/// A buffer: one target on one network.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BufferKey {
    pub network: NetworkId,
    pub target: Target,
}

/// What a line records. For event kinds, `Line::from` is the actor and
/// `Line::text` describes the event without the actor's nick, such as
/// `joined`, `left (reason)`, `quit (reason)`, `kicked bob (reason)`,
/// `is now known as bob`, `changed the topic to: …`, `sets mode +o bob` or
/// `invited bob to #chan`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineKind {
    Privmsg,
    Notice,
    Action,
    Join,
    Part,
    Quit,
    Kick,
    Nick,
    Topic,
    Mode,
    Invite,
    /// Server text, numerics, MOTD and standard replies (NOTE/WARN).
    Info,
    /// FAIL replies, ERROR and failed commands.
    Error,
}

/// One displayable line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Line {
    pub buffer: BufferKey,
    /// IRCv3 `msgid`, when the server sends one.
    pub id: Option<String>,
    /// The message referenced by `+draft/reply`. Kept as received so a
    /// later history page can resolve its root with [`Line::thread_root`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    /// `server-time`, else the time received.
    pub time: DateTime<Utc>,
    pub kind: LineKind,
    /// The nick that caused the line; `None` for server lines.
    pub from: Option<String>,
    /// The sender's account (`account-tag`), when known.
    pub account: Option<String>,
    /// Text with mIRC formatting codes intact; see [`crate::format`].
    pub text: String,
    /// Mentions our nick or a highlight word.
    pub highlight: bool,
    /// We sent it (including `echo-message` echoes).
    pub own: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    /// Connected; CAP, SASL and registration in progress.
    Registering,
    Connected {
        nick: String,
    },
    /// Waiting before the next attempt.
    Reconnecting {
        attempt: u32,
        delay_secs: u64,
        error: String,
    },
    Failed {
        error: String,
    },
}

/// A channel member with their highest prefix, such as `@` or `+`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Member {
    pub nick: String,
    /// All prefixes, highest first (`multi-prefix`).
    pub prefixes: String,
    pub away: bool,
    pub account: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Typing {
    Active,
    Paused,
    Done,
}

/// The newest line an owner already holds for one buffer, so a connection
/// asks the server only for the lines after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    pub target: Target,
    /// The line's IRCv3 `msgid`, when it has one.
    pub id: Option<String>,
    pub time: DateTime<Utc>,
}

/// One channel from the server's `LIST` reply (`RPL_LIST`, 322).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListedChannel {
    pub name: String,
    /// The visible users the server counted.
    pub users: u32,
    /// The topic as sent, mIRC formatting intact; some servers lead it with
    /// the channel modes, such as `[+nt]`.
    #[serde(default)]
    pub topic: String,
}

/// What the runtime reports. Owners apply events in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A channel autojoined by the old selection left the selected set.
    AutojoinRetired(BufferKey),
    Status {
        network: NetworkId,
        status: ConnectionStatus,
    },
    /// Enabled capabilities after negotiation or `cap-notify`.
    Capabilities {
        network: NetworkId,
        enabled: Vec<String>,
    },
    /// A new live line.
    Line(Line),
    /// A line a bouncer replayed from its buffer on connect, outside
    /// `chathistory` (ZNC's `znc.in/playback` batch). It belongs at the end
    /// like a live line, but it is not news, so it never notifies.
    Replayed(Line),
    /// Older lines from `chathistory` (oldest first), for prepending.
    History {
        buffer: BufferKey,
        lines: Vec<Line>,
        complete: bool,
    },
    /// Lines from `chathistory` that belong among the held lines, not before
    /// them: what was missed after an [`Anchor`], or the lines around one
    /// message ([`Command::LoadAround`]). Owners merge them by time. Empty
    /// when the request could not be answered.
    Backfill {
        buffer: BufferKey,
        lines: Vec<Line>,
    },
    /// We joined; the buffer should exist and may be selected. Also sent for
    /// a query opened with `/query`.
    Joined {
        buffer: BufferKey,
    },
    /// We left or were kicked; the buffer stays but is inactive.
    Left {
        buffer: BufferKey,
    },
    Topic {
        buffer: BufferKey,
        topic: String,
        set_by: Option<String>,
    },
    /// The full member list (after NAMES or WHO).
    Members {
        buffer: BufferKey,
        members: Vec<Member>,
    },
    MemberJoined {
        buffer: BufferKey,
        member: Member,
    },
    MemberLeft {
        buffer: BufferKey,
        nick: String,
    },
    MemberChanged {
        buffer: BufferKey,
        old_nick: String,
        member: Member,
    },
    /// Our nick changed on a network.
    OwnNick {
        network: NetworkId,
        nick: String,
    },
    Typing {
        buffer: BufferKey,
        nick: String,
        state: Typing,
    },
    /// A nickname's `draft/metadata-2` status changed on this network.
    /// Plain text only: IRC formatting and controls removed, trimmed to one
    /// line and at most 80 Unicode characters (including a cut's ellipsis).
    /// Markdown, links and mentions remain literal text. `None` clears it.
    /// Updates and clears use the first spelling seen for the held value.
    NickStatus {
        network: NetworkId,
        nick: String,
        status: Option<String>,
    },
    /// `draft/read-marker`: everything up to `time` was read on some client.
    ReadMarker {
        buffer: BufferKey,
        time: DateTime<Utc>,
    },
    /// The answer to [`Command::SendAll`]: whether its lines were written
    /// to the connection, refused or perhaps partly written.
    Sent {
        buffer: BufferKey,
        id: u64,
        result: SendResult,
    },
    /// The server's public channels, in the order it listed them, when a
    /// `LIST` reply ended (`RPL_LISTEND`). Secret (`+s`) and private (`+p`)
    /// channels we are not in are not listed.
    ChannelList {
        network: NetworkId,
        channels: Vec<ListedChannel>,
    },
}

/// What owners ask the runtime to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Replace the configured networks; connects those with `auto_connect`.
    Configure(Vec<NetworkConfig>),
    Connect(NetworkId),
    /// Use these initiative channels for autojoin, or restore local channels.
    SetAutojoin {
        network: NetworkId,
        channels: Option<Vec<String>>,
    },
    Disconnect {
        network: NetworkId,
        reason: String,
    },
    /// Composer input for a buffer: plain text or a `/command`. Multi-line
    /// text is sent as one `draft/multiline` batch when available.
    Input {
        buffer: BufferKey,
        text: String,
    },
    /// Text for the buffer's channel or nick, never read as a `/command`.
    /// `action` sends it as a CTCP `ACTION`, as `/me` does.
    Say {
        buffer: BufferKey,
        text: String,
        action: bool,
    },
    /// Literal lines for the buffer's channel or nick, checked together:
    /// the connection must be registered and every line must fit its
    /// negotiated limits before any is written, so a refused line sends
    /// none. A line starting with `//` is literal text, as in the composer;
    /// any other command is refused. Answered with [`Event::Sent`] carrying
    /// `id` once the write ended; local echoes follow only a full write.
    SendAll {
        buffer: BufferKey,
        id: u64,
        lines: Vec<String>,
    },
    /// Literal text replying to a server-provided `msgid`. Owners use
    /// [`Line::reply`] to resolve the root from their loaded history.
    /// Requires `message-tags`; unavailable replies send no message.
    Reply {
        buffer: BufferKey,
        msgid: String,
        text: String,
    },
    Join {
        network: NetworkId,
        channel: String,
        key: Option<String>,
    },
    Part {
        buffer: BufferKey,
        reason: String,
    },
    /// The user removed this buffer from the list. Sends nothing.
    ///
    /// - A channel: connections skip it in the autojoin until we join it
    ///   again, whether or not the network is connected now.
    /// - A private conversation: the session forgets it, so the other side's
    ///   notices go to the server buffer again, as before it was opened.
    Forget {
        buffer: BufferKey,
    },
    /// Opens (or focuses) a query buffer without sending anything.
    Query {
        network: NetworkId,
        nick: String,
    },
    /// Older history before the oldest line we hold.
    LoadHistory {
        buffer: BufferKey,
        before: Option<DateTime<Utc>>,
        limit: u16,
    },
    /// Lines around one message, by `msgid` or else by time, answered as
    /// [`Event::Backfill`].
    LoadAround {
        buffer: BufferKey,
        id: Option<String>,
        time: DateTime<Utc>,
        limit: u16,
    },
    /// The newest line the owner holds per buffer of `network`. Joining a
    /// channel then asks only for the lines after its anchor, and after
    /// registration each anchored private conversation does too. Sends
    /// nothing by itself.
    Anchors {
        network: NetworkId,
        anchors: Vec<Anchor>,
    },
    MarkRead {
        buffer: BufferKey,
        time: DateTime<Utc>,
    },
    Typing {
        buffer: BufferKey,
        state: Typing,
    },
    /// Asks the server for its public channels (`LIST`), answered as
    /// [`Event::ChannelList`]. Each connection asks once by itself after
    /// the message of the day. Ignored while a `LIST` is running or before
    /// registration.
    ListChannels {
        network: NetworkId,
    },
}

#[cfg(test)]
mod tests {
    use super::{NetworkConfig, default_port, default_true};

    #[test]
    fn defaults_are_tls_and_auto_connect() {
        assert_eq!(default_port(), 6697);
        assert!(default_true());
    }

    #[test]
    fn minimal_network_config_uses_defaults() {
        let config: NetworkConfig =
            serde_json::from_str(r#"{"id":"local","host":"127.0.0.1","nickname":"me"}"#).unwrap();
        assert_eq!(config.port, 6697);
        assert!(config.tls);
        assert!(config.auto_connect);
        assert!(config.highlights.is_empty());
        assert!(config.sasl.is_none());
    }
}
