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
//! - An owner no longer shown (dropped by the line cap, folded into its
//!   request, or removed with its conversation) lets go of its claim, so
//!   the next new message that names it shows the tile. Only messages not
//!   yet read ever claim, so no message already drawn gains a tile.
//! - A new issue scope can recognise references the messages did not have
//!   before (those messages change anyway, as their text gains chips), so
//!   every held message is read again then, and issues are claimed afresh
//!   within the new scope: an identifier another tracker recognised keeps
//!   no claim. Pull requests keep their owners.
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
            Ref::Channel(_) | Ref::Nick(_) | Ref::Decision(_) => None,
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
/// conversation, with where in that message it was named.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TileOwners(HashMap<TileRef, (i64, usize)>);

impl TileOwners {
    /// The rule: each reference of `messages`, read in order, that has no
    /// owner yet goes to the message that names it first. An owner is
    /// never replaced, but a message read again (as a new recognition reads
    /// every held message) gives its own claims their place among the
    /// references it names now, so a kept pull request and a newly
    /// recognised issue never share a place.
    pub fn claim<'a>(&mut self, messages: impl IntoIterator<Item = (i64, &'a [TileRef])>) {
        for (seq, refs) in messages {
            for (at, found) in refs.iter().enumerate() {
                let claim = self.0.entry(found.clone()).or_insert((seq, at));
                if claim.0 == seq {
                    claim.1 = at;
                }
            }
        }
    }

    /// Whether message `seq` shows the tile for `found`.
    pub fn shows(&self, seq: i64, found: &TileRef) -> bool {
        self.0.get(found).is_some_and(|(owner, _)| *owner == seq)
    }

    /// Lets go of every claim whose message `held` does not hold, so a
    /// later message may show that tile: the owner was dropped by the line
    /// cap, folded into its request, or removed with its conversation. Only
    /// messages not yet read ever claim, so no message already drawn gains
    /// a tile.
    pub fn release(&mut self, held: &HashSet<i64>) {
        self.0.retain(|_, (seq, _)| held.contains(seq));
    }

    /// Lets go of every issue's claim, as a new issue recognition does, so
    /// issues are claimed again within the tracker scope now recognised and
    /// no identifier keeps a claim another scope made.
    pub fn release_issues(&mut self) {
        self.0
            .retain(|found, _| matches!(found, TileRef::PullRequest(_)));
    }

    /// Whether any claim's message is missing from `held`.
    fn outlives(&self, held: &HashSet<i64>) -> bool {
        self.0.values().any(|(seq, _)| !held.contains(seq))
    }

    /// Every reference with a tile, in the order the conversation first
    /// showed them, message by message and within a message as it named
    /// them: what is in play in it, so a cap keeps the first named.
    #[cfg(test)]
    pub fn in_play(&self) -> Vec<TileRef> {
        self.in_play_first(usize::MAX, |_| true)
    }

    /// The first `limit` of [`Self::in_play`], chosen before any is
    /// cloned or the rest are sorted.
    pub fn in_play_first(&self, limit: usize, include: impl Fn(&TileRef) -> bool) -> Vec<TileRef> {
        let mut owned: Vec<(&(i64, usize), &TileRef)> = self
            .0
            .iter()
            .filter(|(found, _)| include(found))
            .map(|(found, claim)| (claim, found))
            .collect();
        if limit < owned.len() {
            owned.select_nth_unstable_by_key(limit, |(claim, _)| **claim);
            owned.truncate(limit);
        }
        owned.sort_unstable_by_key(|(claim, _)| **claim);
        owned.into_iter().map(|(_, found)| found.clone()).collect()
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

    /// The first `limit` of what is in play in the conversation; see
    /// [`TileOwners::in_play_first`].
    pub fn in_play_first(&self, limit: usize, include: impl Fn(&TileRef) -> bool) -> Vec<TileRef> {
        self.owners.in_play_first(limit, include)
    }

    /// Whether any issue is in play in the conversation, without listing
    /// what is.
    pub fn has_issue_in_play(&self) -> bool {
        self.owners
            .0
            .keys()
            .any(|found| matches!(found, TileRef::Issue(_)))
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

    /// Whether reading `found` under `recognition`, with `held` the
    /// messages shown now, would change anything: something new to claim,
    /// a new recognition, or a message read or owning a tile that is no
    /// longer shown. Lets a sync with nothing new skip copying the owners.
    pub fn stale(
        &self,
        recognition: u64,
        held: &HashSet<i64>,
        found: &[(i64, Vec<TileRef>)],
    ) -> bool {
        recognition != self.recognition
            || !found.is_empty()
            || self.read.iter().any(|seq| !held.contains(seq))
            || self.owners.outlives(held)
    }

    /// Records what the messages `found` name, in transcript order, under
    /// `recognition`. `held` is every message the conversation shows now:
    /// those no longer shown are forgotten as read and let go as owners.
    /// A new recognition lets go of every issue's claim, so the messages,
    /// all read again, claim issues within the new tracker scope.
    pub fn read(&mut self, recognition: u64, held: &HashSet<i64>, found: &[(i64, Vec<TileRef>)]) {
        if recognition != self.recognition {
            self.read.clear();
            self.owners.release_issues();
            self.recognition = recognition;
        }
        self.owners.release(held);
        self.owners
            .claim(found.iter().map(|(seq, refs)| (*seq, refs.as_slice())));
        self.read.extend(found.iter().map(|(seq, _)| *seq));
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

    /// Reads `found` into `buffer`'s tiles; see [`Tiles::read`]. With
    /// nothing new it changes nothing and copies nothing.
    pub fn read(
        &mut self,
        buffer: &BufferKey,
        recognition: u64,
        (held, found): (&[i64], &[(i64, Vec<TileRef>)]),
    ) -> Rc<Tiles> {
        let held: HashSet<i64> = held.iter().copied().collect();
        let tiles = self.0.entry(buffer.clone()).or_default();
        if tiles.stale(recognition, &held, found) {
            Rc::make_mut(tiles).read(recognition, &held, found);
        }
        Rc::clone(tiles)
    }

    /// Forgets `buffer`'s tiles, as when the conversation is removed: if it
    /// is joined again, its messages claim afresh.
    pub fn forget(&mut self, buffer: &BufferKey) {
        self.0.remove(buffer);
    }
}
