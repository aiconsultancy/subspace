//! Tiles once: within one conversation, the first message to show a tile
//! for an issue or a pull request keeps it, and every later mention is only
//! the inline reference chip in the text.
//!
//! "First" is decided once and is sticky. Whenever the conversation syncs
//! with the hub, before any new row is laid out, the messages it has not
//! read yet are read in transcript order and each reference nobody owns yet
//! goes to the earliest of them ([`TileOwners::claim`]). An owner never
//! changes after that, so a message never gains or loses a tile once drawn
//! and never changes height:
//!
//! - History loaded above, and lines merged in from the store, are read
//!   after the messages already held: an older mention found later stays a
//!   chip, and the message that showed the tile first in this session
//!   keeps it.
//! - New messages only ever see references already owned or new ones.
//! - Re-rendering reads nothing again.
//! - An owner that leaves the held window (the line cap) keeps its claim:
//!   no other message takes the tile, which would make it grow.
//! - A new issue scope can recognise references the messages did not have
//!   before (those messages change anyway, as their text gains chips), so
//!   every held message is read again then; references already owned stay
//!   with their owner.
//!
//! Owners live in the hub per conversation (`BufferKey`) for the session,
//! in memory only: closing and reopening a conversation keeps them, and a
//! restart decides again from the messages then held. Conversations never
//! share owners.

mod frame;
mod row;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use subspace_client_irc::BufferKey;

use crate::markup::Ref;

pub(crate) use frame::{Lead, Slot, loading, meter, slot, tile, unresolved};
pub(crate) use row::row;

/// A reference a tile can show, by its stable identity: an issue's
/// identifier or a pull request's canonical `owner/repo#n`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TileRef {
    Issue(String),
    PullRequest(String),
}

impl TileRef {
    /// The tile identity of `found`; `None` for channels and nicks.
    pub fn of(found: &Ref) -> Option<Self> {
        match found {
            Ref::Issue(identifier) => Some(Self::Issue(identifier.clone())),
            Ref::PullRequest(found) => Some(Self::PullRequest(found.key.clone())),
            Ref::Channel(_) | Ref::Nick(_) => None,
        }
    }
}

/// The references of a message that could show a tile: its issues and pull
/// requests, each once, in reading order.
pub(crate) fn candidates(found: impl IntoIterator<Item = Ref>) -> Vec<Ref> {
    let mut seen = HashSet::new();
    found
        .into_iter()
        .filter(|found| TileRef::of(found).is_some_and(|key| seen.insert(key)))
        .collect()
}

/// Which message, by local sequence, keeps each reference's tile in one
/// conversation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TileOwners(HashMap<TileRef, i64>);

impl TileOwners {
    /// The rule: each reference of `messages`, read in order, that has no
    /// owner yet goes to the message that names it first. An owner is
    /// never replaced.
    pub fn claim<'a>(&mut self, messages: impl IntoIterator<Item = (i64, &'a [TileRef])>) {
        for (seq, refs) in messages {
            for found in refs {
                self.0.entry(found.clone()).or_insert(seq);
            }
        }
    }

    /// Whether message `seq` shows the tile for `found`.
    pub fn shows(&self, seq: i64, found: &TileRef) -> bool {
        self.0.get(found) == Some(&seq)
    }
}

/// One conversation's owners and the messages already read for them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tiles {
    owners: TileOwners,
    /// Messages read under `recognition`.
    read: HashSet<i64>,
    /// The issue recognition the messages were read with.
    recognition: u64,
}

impl Tiles {
    /// Whether message `seq` shows the tile for `found`.
    pub fn shows(&self, seq: i64, found: &Ref) -> bool {
        TileRef::of(found).is_some_and(|found| self.owners.shows(seq, &found))
    }

    /// Of `messages`, those not yet read under `recognition`.
    pub fn unread(&self, messages: &[i64], recognition: u64) -> HashSet<i64> {
        let fresh = recognition != self.recognition;
        messages
            .iter()
            .copied()
            .filter(|seq| fresh || !self.read.contains(seq))
            .collect()
    }

    /// Records what the messages `found` name, in transcript order, under
    /// `recognition`. `held` is every message the conversation holds now,
    /// so those no longer held are forgotten as read (never as owners).
    pub fn read(&mut self, recognition: u64, held: &[i64], found: &[(i64, Vec<TileRef>)]) {
        if recognition != self.recognition {
            self.read.clear();
            self.recognition = recognition;
        }
        self.owners
            .claim(found.iter().map(|(seq, refs)| (*seq, refs.as_slice())));
        self.read.extend(found.iter().map(|(seq, _)| *seq));
        let held: HashSet<i64> = held.iter().copied().collect();
        self.read.retain(|seq| held.contains(seq));
    }
}

/// Every conversation's tiles for this session.
#[derive(Default)]
pub(crate) struct Sessions(HashMap<BufferKey, Rc<Tiles>>);

impl Sessions {
    /// `buffer`'s tiles as they stand.
    pub fn get(&self, buffer: &BufferKey) -> Rc<Tiles> {
        self.0.get(buffer).cloned().unwrap_or_default()
    }

    /// Reads `found` into `buffer`'s tiles; see [`Tiles::read`].
    pub fn read(
        &mut self,
        buffer: &BufferKey,
        recognition: u64,
        (held, found): (&[i64], &[(i64, Vec<TileRef>)]),
    ) -> Rc<Tiles> {
        let tiles = self.0.entry(buffer.clone()).or_default();
        Rc::make_mut(tiles).read(recognition, held, found);
        Rc::clone(tiles)
    }
}
