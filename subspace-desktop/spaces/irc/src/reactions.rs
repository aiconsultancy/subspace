//! Message reactions as the hub keeps them: the client's
//! [`ReactionBook`], whether each network can react now, and the latest
//! refusal per conversation. Pure data, fed in the runtime's order.
//!
//! A conversation reads what its messages show with [`Reactions::shown`],
//! and asks [`stop::target`] whether a message's reaction choices carry
//! the stop group.

use std::collections::{BTreeMap, HashSet, VecDeque};

use subspace_client_irc::reactions::{ReactionBook, ReactionSummary};
use subspace_client_irc::{
    BufferKey, Event, NetworkId, Reaction, ReactionChange, ReactionError, Update,
};

use crate::state::{Buffer, IrcState};

pub(crate) mod choices;
pub(crate) mod gate;
pub(crate) mod stop;
#[cfg(test)]
mod tests;

/// What each held message shows, by its line's sequence: only messages
/// with reactions.
pub(crate) type Shown = BTreeMap<i64, Vec<ReactionSummary>>;

/// A reaction the client refused or could not write, for a brief notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    /// Counts up with every refusal, so a conversation shows each once.
    pub serial: u64,
    pub msgid: String,
    pub error: ReactionError,
}

/// How many of our latest live additions [`Reactions::echoed_since`]
/// remembers.
const MAX_ECHOES: usize = 64;

/// One of our additions the client reported live, numbered.
#[derive(Debug, Clone)]
struct Echo {
    serial: u64,
    buffer: BufferKey,
    msgid: String,
    reaction: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Reactions {
    book: ReactionBook,
    support: BTreeMap<NetworkId, bool>,
    refusals: BTreeMap<BufferKey, Refusal>,
    serial: u64,
    /// Our latest live additions, as the client reported them, before the
    /// book's limits could drop any: a stop's acknowledgement waits for
    /// its echo, not for room in the display.
    echoes: VecDeque<Echo>,
    echo_serial: u64,
}

impl Reactions {
    /// Follows an event the hub is about to apply: nick changes, and the
    /// messages lines bring, which the book then counts as loaded.
    pub fn observe(&mut self, event: &Event) {
        self.book.observe(event);
    }

    /// Takes one update that is not an event. A reaction to a message the
    /// state already `held`, such as one restored from the store, counts
    /// that message as loaded first. True when what is shown may change, or
    /// our own addition was reported live: a reaction the book discarded,
    /// as a repeat or an older operation from history, changes nothing.
    pub fn update(&mut self, update: Update, held: bool) -> bool {
        match update {
            Update::Reaction(reaction) => {
                let echoed = self.note_echo(&reaction);
                let made_room = held && self.book.mark_seen(&reaction.buffer, &reaction.msgid);
                self.book.apply(&reaction) || made_room || echoed
            }
            Update::ReactionRefused {
                buffer,
                msgid,
                error,
            } => {
                self.serial += 1;
                let serial = self.serial;
                self.refusals.insert(
                    buffer,
                    Refusal {
                        serial,
                        msgid,
                        error,
                    },
                );
                true
            }
            Update::ReactionSupport { network, supported } => {
                self.support.insert(network, supported) != Some(supported)
            }
            _ => false,
        }
    }

    /// Whether `network` can send and receive reactions now. Reactions
    /// already known stay shown while it cannot.
    pub fn supported(&self, network: &NetworkId) -> bool {
        self.support.get(network).copied().unwrap_or(false)
    }

    /// Remembers `reaction` when it is our own addition reported live;
    /// whether it was, as a stop sent may wait for it.
    fn note_echo(&mut self, reaction: &Reaction) -> bool {
        if !reaction.own || reaction.history || reaction.change != ReactionChange::Added {
            return false;
        }
        self.echo_serial += 1;
        self.echoes.push_back(Echo {
            serial: self.echo_serial,
            buffer: reaction.buffer.clone(),
            msgid: reaction.msgid.clone(),
            reaction: reaction.reaction.clone(),
        });
        if self.echoes.len() > MAX_ECHOES {
            self.echoes.pop_front();
        }
        true
    }

    /// The number of our latest live addition, for [`Self::echoed_since`].
    pub fn echo_serial(&self) -> u64 {
        self.echo_serial
    }

    /// Whether the client reported our `reaction`, exactly that value, on
    /// the message `msgid` in `buffer`, live and after addition number
    /// `after`, whether or not the book had room to show it.
    pub fn echoed_since(
        &self,
        after: u64,
        buffer: &BufferKey,
        (msgid, reaction): (&str, &str),
    ) -> bool {
        self.echoes.iter().any(|echo| {
            echo.serial > after
                && echo.buffer == *buffer
                && echo.msgid == msgid
                && echo.reaction == reaction
        })
    }

    /// When what `buffer` shows from the book last changed: what was read
    /// with [`Self::shown`] at the same revision, over the same held
    /// lines, is still current.
    pub fn revision(&self, buffer: &BufferKey) -> u64 {
        self.book.revision(buffer)
    }

    /// The latest refusal in `buffer`, if any.
    pub fn refusal(&self, buffer: &BufferKey) -> Option<&Refusal> {
        self.refusals.get(buffer)
    }

    /// Drops everything kept for a removed conversation.
    pub fn forget(&mut self, buffer: &BufferKey) {
        self.book.forget(buffer);
        self.refusals.remove(buffer);
    }

    /// The reactions on each of `held`'s messages, in the book's order.
    pub fn shown(&self, buffer: &BufferKey, held: &Buffer) -> Shown {
        let reacted: HashSet<&str> = self.book.reacted_messages(buffer).into_iter().collect();
        if reacted.is_empty() {
            return Shown::new();
        }
        held.entries
            .iter()
            .filter_map(|entry| {
                let id = entry.line.id.as_deref().filter(|id| reacted.contains(id))?;
                Some((entry.seq, self.book.reactions(buffer, id)))
            })
            .collect()
    }
}

/// Takes one reaction update into `reactions`, knowing what `state`
/// holds; see [`Reactions::update`].
pub(crate) fn react(reactions: &mut Reactions, state: &IrcState, update: Update) -> bool {
    // A message already known loaded needs no look through the buffer.
    let held = match &update {
        Update::Reaction(reaction) => {
            !reactions.book.is_seen(&reaction.buffer, &reaction.msgid)
                && holds(state.buffer(&reaction.buffer), &reaction.msgid)
        }
        _ => false,
    };
    reactions.update(update, held)
}

/// Whether `held` holds a line with message ID `msgid`.
fn holds(held: Option<&Buffer>, msgid: &str) -> bool {
    held.is_some_and(|held| {
        held.entries
            .iter()
            .any(|entry| entry.line.id.as_deref() == Some(msgid))
    })
}

/// The brief, plain words for a refusal.
pub(crate) fn refusal_text(error: &ReactionError) -> String {
    match error {
        ReactionError::NotConnected => "Reaction not sent: not connected.".to_owned(),
        ReactionError::ConnectionLost => {
            "The connection failed while sending the reaction; it may not have arrived.".to_owned()
        }
        ReactionError::Unsupported => {
            "Reaction not sent: this server does not support reactions.".to_owned()
        }
        other => format!("Reaction not sent. {other}"),
    }
}
