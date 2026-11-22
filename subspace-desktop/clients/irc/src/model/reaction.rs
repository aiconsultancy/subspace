//! IRCv3 reactions (`+draft/react` and `+draft/unreact` on a `TAGMSG` that
//! names its message with `+draft/reply`): what owners send and receive.

use chrono::{DateTime, Utc};

use super::{BufferKey, NetworkId};
pub use crate::session::Casemapping;

/// Whether a reaction was added or taken back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReactionChange {
    /// `+draft/react`.
    Added,
    /// `+draft/unreact`.
    Removed,
}

/// One reaction someone added or removed, live, echoed or from history.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Reaction {
    pub buffer: BufferKey,
    /// The `msgid` of the message reacted to (`+draft/reply`).
    pub msgid: String,
    /// The reaction as sent, usually one emoji. Checked with
    /// [`crate::reactions::validate_reaction`] before it reaches an owner.
    pub reaction: String,
    pub change: ReactionChange,
    /// Who reacted.
    pub nick: String,
    /// Their account (`account-tag`), when known.
    pub account: Option<String>,
    /// We reacted (an `echo-message` echo, or a local echo without it).
    pub own: bool,
    /// `server-time`, else the time received.
    pub time: DateTime<Utc>,
    /// The `TAGMSG`'s own `msgid`, when the server sent one.
    pub id: Option<String>,
    /// Replayed from `chathistory` rather than live.
    pub history: bool,
    /// How the network compares nicks and accounts (`CASEMAPPING`).
    pub casemapping: Casemapping,
}

/// Why a reaction was not sent. Nothing is written when one is returned.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReactionError {
    /// The connection has not enabled `message-tags`, or the server's
    /// `CLIENTTAGDENY` blocks reaction tags.
    #[error("This server does not support reactions.")]
    Unsupported,
    /// The network is not connected and registered, or is not configured.
    #[error("Not connected.")]
    NotConnected,
    /// The write failed: the reaction may or may not have reached the
    /// server.
    #[error("The connection failed while sending the reaction.")]
    ConnectionLost,
    /// The buffer's channel or nick is not one valid destination.
    #[error("This conversation cannot take reactions.")]
    InvalidTarget,
    /// The message has no server `msgid`, so nothing can name it.
    #[error("This message cannot take reactions: it has no message ID.")]
    NoMessageId,
    /// The `msgid` is too long or holds spaces or control characters.
    #[error("This message's ID cannot be sent as an IRC tag.")]
    InvalidMessageId,
    /// The reaction is empty, too long, padded with spaces or holds
    /// control characters.
    #[error("A reaction is 1 to 32 characters, without control characters or surrounding spaces.")]
    InvalidReaction,
    /// The server buffer has no conversation to react in.
    #[error("The server buffer has no conversation to react in.")]
    NoConversation,
}

/// The answer the runtime gives besides [`super::Event`]s: reactions and
/// refused reaction commands, in order with the events. Read them with
/// [`crate::Runtime::drain_updates`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Update {
    Event(super::Event),
    Reaction(Reaction),
    /// A [`super::Command::React`] or [`super::Command::Unreact`] that sent
    /// nothing, or ([`ReactionError::ConnectionLost`]) whose write failed.
    ReactionRefused {
        buffer: BufferKey,
        msgid: String,
        error: ReactionError,
    },
    /// Whether `network` can send and receive reactions now: reported with
    /// every capability change and when `CLIENTTAGDENY` changes. A UI hides
    /// reactions while it is false.
    ReactionSupport {
        network: NetworkId,
        supported: bool,
    },
}
