//! Board-to-board contracts. A contract is an interface that no board owns:
//! a stable name, a version and the typed messages boards exchange through
//! it. A board that publishes or answers a contract and a board that listens
//! to or asks it both depend on this crate, never on each other, so either
//! can be absent or disabled.
//!
//! Two kinds of message exist. An [`Event`] announces that something happened
//! and expects no answer. A [`Request`] asks the one board that responds to
//! the contract for an immediate answer from the state it holds. Messages
//! carry identities and small summaries, never credentials or bulk content.
//!
//! The broker that routes messages, and the declarations boards make at
//! registration, live in board-kit (`subspace_dashboard::contracts`).

pub mod decisions;
pub mod github;
pub mod issues;

/// A named, versioned interface between boards.
pub trait Contract: 'static {
    /// Stable lower-case name, such as `issues`: ASCII letters, digits and
    /// hyphens, starting with a letter.
    const NAME: &'static str;
    /// Starts at 1. A breaking change is a new version with its own types.
    const VERSION: u32;
}

/// A notification that something happened. Delivered at most once to every
/// running board that subscribes to it, with no answer and no replay.
pub trait Event: 'static {
    type Contract: Contract;
    /// The event's name within its contract, such as `changed`.
    const NAME: &'static str;
}

/// A question with a typed answer, routed to the contract's one responder.
pub trait Request: 'static {
    type Contract: Contract;
    /// The request's name within its contract, such as `resolve`.
    const NAME: &'static str;
    type Response: 'static;
}

/// Whether a message is an [`Event`] or a [`Request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageKind {
    Event,
    Request,
}

/// The identity of one message type: contract, version, name and kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct MessageKey {
    pub contract: &'static str,
    pub version: u32,
    pub message: &'static str,
    pub kind: MessageKind,
}

impl MessageKey {
    pub const fn event<E: Event>() -> Self {
        Self {
            contract: <E::Contract as Contract>::NAME,
            version: <E::Contract as Contract>::VERSION,
            message: E::NAME,
            kind: MessageKind::Event,
        }
    }

    pub const fn request<R: Request>() -> Self {
        Self {
            contract: <R::Contract as Contract>::NAME,
            version: <R::Contract as Contract>::VERSION,
            message: R::NAME,
            kind: MessageKind::Request,
        }
    }

    /// `issues@1/resolve`: the contract, its version and the message, as
    /// logs, errors and the documented inventory show it.
    pub fn label(&self) -> String {
        format!("{}@{}/{}", self.contract, self.version, self.message)
    }

    /// Whether the contract and message names are well formed and the
    /// version is at least 1.
    pub fn is_valid(&self) -> bool {
        self.version >= 1 && valid_name(self.contract) && valid_name(self.message)
    }
}

fn valid_name(name: &str) -> bool {
    name.starts_with(|first: char| first.is_ascii_lowercase())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests;
