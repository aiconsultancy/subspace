//! IRCv3 reactions for owners: whether a connection supports them, checks a
//! UI can run before offering or sending one, a bounded per-message
//! aggregation ([`ReactionBook`]) and the reactions that stop an agent
//! ([`agent_stop`]).
//!
//! On the wire a reaction is a `TAGMSG` to the conversation with
//! `+draft/react=<reaction>` (or `+draft/unreact=<reaction>`) and
//! `+draft/reply=<msgid>` naming the message. Servers relay it only between
//! clients that enabled `message-tags`.

pub mod agent_stop;
mod book;

pub use book::{
    MAX_MESSAGES, MAX_REACTIONS_PER_MESSAGE, MAX_REACTORS_PER_REACTION, ReactionBook,
    ReactionSummary, Reactor,
};

use crate::model::ReactionError;

/// The client-only tag that adds a reaction.
pub const REACT_TAG: &str = "+draft/react";
/// The client-only tag that takes a reaction back.
pub const UNREACT_TAG: &str = "+draft/unreact";
/// The capability reactions need.
pub const CAPABILITY: &str = "message-tags";
/// The longest reaction, in Unicode scalar values: room for any emoji
/// sequence, and Stargate's own limit for a stop reaction.
pub const MAX_REACTION_CHARS: usize = 32;
/// The longest `msgid` sent or accepted, in bytes. Ergo's are 26.
pub const MAX_MSGID_BYTES: usize = 128;

/// Whether a connection with these enabled capabilities (from
/// [`crate::Event::Capabilities`]) can send and receive reactions. A UI
/// hides reactions when this is false.
pub fn reactions_supported(enabled: &[String]) -> bool {
    enabled.iter().any(|cap| cap == CAPABILITY)
}

/// Checks a reaction before sending it: the connection supports reactions,
/// the buffer is a conversation (`conversation` is false for the server
/// buffer), the message has a usable `msgid` and the reaction is valid.
pub fn check_reaction(
    enabled: &[String],
    conversation: bool,
    msgid: Option<&str>,
    reaction: &str,
) -> Result<(), ReactionError> {
    require(reactions_supported(enabled), ReactionError::Unsupported)?;
    require(conversation, ReactionError::NoConversation)?;
    validate_msgid(msgid)?;
    validate_reaction(reaction).map(|_| ())
}

fn require(condition: bool, error: ReactionError) -> Result<(), ReactionError> {
    if condition { Ok(()) } else { Err(error) }
}

/// A reaction value from a person or the network: 1 to
/// [`MAX_REACTION_CHARS`] characters, no control characters and no leading
/// or trailing whitespace. Returns it unchanged when valid.
pub fn validate_reaction(reaction: &str) -> Result<&str, ReactionError> {
    let count = reaction.chars().count();
    let padded =
        reaction.starts_with(char::is_whitespace) || reaction.ends_with(char::is_whitespace);
    let valid = (1..=MAX_REACTION_CHARS).contains(&count)
        && !padded
        && !reaction.chars().any(char::is_control);
    if valid {
        Ok(reaction)
    } else {
        Err(ReactionError::InvalidReaction)
    }
}

/// A message ID to name in `+draft/reply`: present, at most
/// [`MAX_MSGID_BYTES`] and free of whitespace and control characters.
pub fn validate_msgid(msgid: Option<&str>) -> Result<&str, ReactionError> {
    match msgid {
        None | Some("") => Err(ReactionError::NoMessageId),
        Some(id)
            if id.len() > MAX_MSGID_BYTES
                || id.chars().any(|c| c.is_whitespace() || c.is_control()) =>
        {
            Err(ReactionError::InvalidMessageId)
        }
        Some(id) => Ok(id),
    }
}

#[cfg(test)]
mod tests;
