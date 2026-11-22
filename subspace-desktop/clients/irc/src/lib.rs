//! IRC with IRCv3 for Subspace: protocol parsing, connections and per-network
//! session state, run on a background runtime and driven by [`Command`]s.

#[cfg(test)]
mod cleanroom_acceptance;

mod codec;
mod connection;
pub mod format;
pub mod model;
mod network;
mod proto;
pub mod reactions;
pub mod runtime;
mod session;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use model::{
    Anchor, BufferKey, Command, ConnectionStatus, Event, Line, LineKind, ListedChannel, Member,
    NetworkConfig, NetworkId, Reaction, ReactionChange, ReactionError, Sasl, SendResult, Target,
    ThreadRoots, Typing, Update,
};
pub use runtime::Runtime;
pub use session::{
    COMMANDS, CommandHelp, FirstArgument, KEPT_CLIENT_TAGS, MAX_INLINE_BYTES, PROGRESS_TAG,
    check_draft, check_inline_text, connection_batches, draft_line_limit, message_budget,
    private_recipients, status_text,
};
