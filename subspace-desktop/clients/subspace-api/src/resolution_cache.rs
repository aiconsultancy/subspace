//! In-memory reference resolution state with caller-supplied time.
//!
//! One cache belongs to a hub session. Call `set_scope` on every scope change,
//! using a fresh generation on connection replacement, failure and stop, even when profile
//! and origin remain the same. Only the caller classifies a post-pack-check
//! 404 as unavailable; every other error uses `ResolutionOutcome::Error`.
//! No card is serialised, logged or sent to I/O by this module. Types containing
//! identifiers or cards deliberately do not implement `Debug` or serialization.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const AVAILABLE_LIFETIME: Duration = Duration::from_secs(5 * 60);
const UNAVAILABLE_LIFETIME: Duration = Duration::from_secs(60);
const PENDING_LIFETIME: Duration = Duration::from_secs(60);
const CAPACITY: usize = 500;

/// The complete identity of the data a cache is allowed to retain.
#[derive(Clone, PartialEq, Eq)]
pub struct ResolutionScope {
    profile: String,
    origin: String,
    generation: u64,
}

impl ResolutionScope {
    pub fn new(profile: &str, origin: &str, generation: u64) -> Self {
        Self {
            profile: profile.into(),
            origin: origin.into(),
            generation,
        }
    }
}

/// An owned read ticket. The caller must finish every started read on the same
/// cache, using `ResolutionOutcome::Error` for a cancelled read. The 60-second
/// abandonment timeout is only a safety net. Each ticket has a private serial
/// so a late completion cannot end or overwrite its replacement.
pub struct ResolutionRead {
    scope: ResolutionScope,
    identifier: String,
    serial: u64,
}

impl ResolutionRead {
    pub fn identifier(&self) -> &str {
        &self.identifier
    }
    pub fn generation(&self) -> u64 {
        self.scope.generation
    }
}

/// A request either borrows a cached card, reports a negative/pending result,
/// or grants the single ticket that starts a read.
pub enum ResolutionLookup<'a, C> {
    Available(&'a C),
    Unavailable,
    Pending,
    Start(ResolutionRead),
}

/// Only `Unavailable` (404 after the pack check) is negatively cached.
pub enum ResolutionOutcome<C> {
    Available(C),
    Unavailable,
    Error,
}

/// Up to 500 resolved entries in LRU order, plus deduplicated pending reads.
pub struct ResolutionCache<C> {
    scope: ResolutionScope,
    entries: Vec<Entry<C>>,
    pending: HashMap<String, PendingRead>,
    next_serial: u64,
}

struct PendingRead {
    started: Instant,
    serial: u64,
}

struct Entry<C> {
    identifier: String,
    card: Option<C>,
    expires: Instant,
}

impl<C> Entry<C> {
    fn from_outcome(
        identifier: String,
        outcome: ResolutionOutcome<C>,
        now: Instant,
    ) -> Option<Self> {
        let (card, lifetime) = match outcome {
            ResolutionOutcome::Available(card) => (Some(card), AVAILABLE_LIFETIME),
            ResolutionOutcome::Unavailable => (None, UNAVAILABLE_LIFETIME),
            ResolutionOutcome::Error => return None,
        };
        Some(Self {
            identifier,
            card,
            expires: now + lifetime,
        })
    }
}

impl<C> ResolutionCache<C> {
    pub fn new(scope: ResolutionScope) -> Self {
        Self {
            scope,
            entries: Vec::new(),
            pending: HashMap::new(),
            next_serial: 0,
        }
    }

    /// Drops all cards and pending reads if any scope component changes.
    pub fn set_scope(&mut self, scope: ResolutionScope) {
        if self.scope != scope {
            self.entries.clear();
            self.pending.clear();
            self.scope = scope;
        }
    }

    /// Touches LRU order without extending the original expiry. Pending reads
    /// are separate from the 500 resolved entries and never evicted by LRU.
    /// At 60 seconds, a pending read is abandoned and a request starts a retry.
    pub fn request(&mut self, identifier: &str, now: Instant) -> ResolutionLookup<'_, C> {
        self.expire(now);
        match self.position(identifier) {
            Some(index) => self.hit(index),
            None => self.begin(identifier, now),
        }
    }

    /// Side-effect-free projection: excludes expired entries and pending reads,
    /// without touching LRU order or starting work. `None` denotes unavailable.
    pub fn cards(&self, now: Instant) -> impl Iterator<Item = (&str, Option<&C>)> {
        self.entries
            .iter()
            .filter(move |entry| entry.expires > now)
            .map(|entry| (entry.identifier.as_str(), entry.card.as_ref()))
    }

    /// Read one live result without touching LRU order or starting work.
    // qual:api - Public read-only accessor, also exercised by integration fixtures.
    pub fn peek(&self, identifier: &str, now: Instant) -> Option<Option<&C>> {
        self.cards(now)
            .find(|(id, _)| *id == identifier)
            .map(|(_, card)| card)
    }

    /// Returns false for a stale ticket, leaving current pending reads intact.
    /// Expiry starts at completion. Errors release the ticket without caching.
    /// The caller must finish every started read, including cancellations with
    /// `ResolutionOutcome::Error`; the abandonment timeout is only a safety net.
    pub fn finish(
        &mut self,
        read: ResolutionRead,
        outcome: ResolutionOutcome<C>,
        now: Instant,
    ) -> bool {
        if !self.take_pending(&read) {
            return false;
        }
        self.store(read.identifier, outcome, now);
        true
    }

    fn expire(&mut self, now: Instant) {
        self.entries.retain(|entry| entry.expires > now);
        self.pending
            .retain(|_, pending| now.saturating_duration_since(pending.started) < PENDING_LIFETIME);
    }

    fn position(&self, identifier: &str) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.identifier == identifier)
    }

    fn hit(&mut self, index: usize) -> ResolutionLookup<'_, C> {
        let entry = self.entries.remove(index);
        self.entries.push(entry);
        match &self.entries[self.entries.len() - 1].card {
            Some(card) => ResolutionLookup::Available(card),
            None => ResolutionLookup::Unavailable,
        }
    }

    fn begin(&mut self, identifier: &str, now: Instant) -> ResolutionLookup<'_, C> {
        if self.pending.contains_key(identifier) {
            return ResolutionLookup::Pending;
        }
        ResolutionLookup::Start(self.ticket(identifier, now))
    }

    /// Starts a fresh read of `identifier` because it changed, such as after
    /// a write: any read already pending is superseded, so its late answer
    /// cannot land, and the card held now stays shown until the new answer
    /// replaces it.
    pub fn refresh(&mut self, identifier: &str, now: Instant) -> ResolutionRead {
        self.expire(now);
        self.ticket(identifier, now)
    }

    fn ticket(&mut self, identifier: &str, now: Instant) -> ResolutionRead {
        self.next_serial = self.next_serial.wrapping_add(1);
        self.pending.insert(
            identifier.into(),
            PendingRead {
                started: now,
                serial: self.next_serial,
            },
        );
        ResolutionRead {
            scope: self.scope.clone(),
            identifier: identifier.into(),
            serial: self.next_serial,
        }
    }

    fn take_pending(&mut self, read: &ResolutionRead) -> bool {
        if !self.is_pending(read) {
            return false;
        }
        self.pending.remove(&read.identifier);
        true
    }

    /// Whether this exact ticket still owns the pending read. Call before
    /// applying session-level errors from a possibly abandoned response.
    pub fn is_pending(&self, read: &ResolutionRead) -> bool {
        self.scope == read.scope
            && self
                .pending
                .get(&read.identifier)
                .is_some_and(|pending| pending.serial == read.serial)
    }

    fn store(&mut self, identifier: String, outcome: ResolutionOutcome<C>, now: Instant) {
        self.expire(now);
        self.insert(Entry::from_outcome(identifier, outcome, now));
    }

    fn insert(&mut self, entry: Option<Entry<C>>) {
        // A failed read stores nothing, so a failed refresh keeps the card
        // shown; an answer replaces the identifier's old card.
        if let Some(entry) = entry {
            self.entries
                .retain(|held| held.identifier != entry.identifier);
            if self.entries.len() == CAPACITY {
                self.entries.remove(0);
            }
            self.entries.push(entry);
        }
    }
}
