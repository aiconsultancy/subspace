//! Following one officer's turns without a gap: the snapshot route joined
//! with the `agent-activity` events of the fleet's one stream, as
//! Stargate's *Live activity events* describes.
//!
//! 1. Subscribe ([`crate::FleetApi::watch_activity`]) and keep the events.
//! 2. Once the stream that carries the officer has answered, and not
//!    before, fetch the snapshot: Stargate watches the officer only once
//!    the events response has started, so a snapshot fetched alongside it
//!    could miss changes with no `seq` to tell. Events that arrive
//!    meanwhile wait. A stream still connecting or reconnecting is waited
//!    for; only without one (the fleet is polled, or the stream refused
//!    the officer) does the view fetch on a timer instead
//!    ([`ActivityJoin::set_fleet_state`]).
//! 3. Apply each waiting or later event whose `seq` is above the
//!    snapshot's `cursor.seq`, as an upsert by ID; drop the rest. An event
//!    at or below the cursor as fetched is covered: it is dropped and
//!    counts for nothing else. Events that wait are checked only once the
//!    snapshot they wait for has come, after the ones it covers are
//!    dropped, so an event covered by a snapshot still in flight never
//!    fetches again.
//! 4. Fetch again, one fetch at a time, when the stream reconnects or
//!    closes, when an event's `generation` is not the cursor's, when the
//!    `seq` of the live events (those the snapshot does not cover) fails
//!    to rise, when an item names a turn the snapshot lacks, or when
//!    the reader fell so far behind that the feed dropped events for it.
//!
//! 5. When a fetch fails, keep showing the last snapshot with the error
//!    ([`Fetch::Failed`]) and apply no event to it: events wait, up to
//!    [`MAX_HELD_EVENTS`] or [`MAX_HELD_BYTES`], and are dropped past
//!    that, since the next snapshot covers them. Fetch again after a
//!    backoff of 1, 2, 4 … seconds up to 30, or Stargate's `Retry-After`
//!    up to the same cap ([`ActivityJoin::retry_in`]), at once on the
//!    next stream connection, or when the person asks
//!    ([`ActivityJoin::retry`]). `NotFound` is not retried on a timer.
//!
//! `seq` skips values for coalesced changes, so a skipped value is not a
//! gap: only a `seq` that does not rise is. `seq` counts per officer, so
//! each join follows one officer, with its own cursor and last `seq`.

use std::time::Duration;

use super::{
    ActivityChange, ActivityCursor, ActivityEntry, ActivityTurn, AgentActivity, AgentActivityEvent,
};
use super::{TurnStatus, TurnSummary};
use crate::{Error, FleetState, Link};

/// The most turns Stargate keeps for an officer, and so the most kept here.
const MAX_TURNS: usize = 16;
/// The most events held for one officer, by the feed for its reader and
/// by the join while it waits for a snapshot.
pub(crate) const MAX_HELD_EVENTS: usize = 512;
/// The most bytes of event JSON held for one officer, as for
/// [`MAX_HELD_EVENTS`].
pub(crate) const MAX_HELD_BYTES: usize = 8 * 1024 * 1024;
/// The wait before the first retry of a failed fetch, doubled each time.
const FIRST_RETRY: Duration = Duration::from_secs(1);
/// The longest wait before a retry, `Retry-After` included.
const MAX_RETRY: Duration = Duration::from_secs(30);
/// The most times the first wait doubles: 32 seconds, capped at
/// [`MAX_RETRY`].
const MAX_DOUBLINGS: u32 = 5;

/// What the feed holds for a reader of one officer's events since it last
/// read ([`crate::FleetApi::activity_news`]).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct ActivityNews {
    /// The stream connection the officer's events now come from; `None`
    /// while no open stream carries them.
    pub connection: Option<u64>,
    /// The events after the reader's position, oldest first.
    pub events: Vec<AgentActivityEvent>,
    /// The position to read from next.
    pub next: u64,
    /// Events after the reader's position were dropped before it read them.
    pub missed: bool,
    /// The stream refused the subscription (an unknown agent, or a gateway
    /// that checks the parameter and refuses it): no events will come.
    pub refused: bool,
}

/// Where the snapshot fetch stands. One runs at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fetch {
    /// The snapshot is current.
    Idle,
    /// A fetch should start ([`ActivityJoin::start_fetch`]).
    Wanted,
    /// A fetch runs for the stream connection it started under; `again`
    /// when a reason to fetch arrived while it ran.
    InFlight {
        connection: Option<u64>,
        again: bool,
    },
    /// The last fetch failed: the snapshot is stale, events wait, and the
    /// next fetch waits for [`ActivityJoin::retry`] or a new connection.
    Failed,
}

/// Events that wait for a fetch, and the bytes of JSON they were read from.
#[derive(Default)]
struct Pending {
    events: Vec<AgentActivityEvent>,
    bytes: usize,
}

/// One officer's turns, kept current from the snapshot and the events.
pub struct ActivityJoin {
    snapshot: Option<AgentActivity>,
    /// The cursor of the last snapshot fetched, which events applied to it
    /// never move: an event at or below it is covered by what was fetched.
    fetched: Option<ActivityCursor>,
    error: Option<Error>,
    connection: Option<u64>,
    refused: bool,
    /// No stream carries the officer: fetch without waiting for one.
    polling: bool,
    next: u64,
    /// The last event's `seq` on this connection, which the next must pass.
    stream_seq: Option<u64>,
    /// Events that wait for the fetch in flight, or for one that succeeds.
    pending: Pending,
    fetch: Fetch,
    /// Fetches that failed since the last that succeeded.
    failures: u32,
}

impl Default for ActivityJoin {
    fn default() -> Self {
        Self::new()
    }
}

// qual:allow(dry, boilerplate) reason: "The join's fields change only together, through its join rules; these read or set one each without exposing the rest to writes."
impl ActivityJoin {
    /// Why the last fetch failed, until one succeeds.
    pub fn error(&self) -> Option<Error> {
        self.error
    }

    /// Where the snapshot fetch stands.
    // qual:allow(dry, boilerplate) reason: "Reads or sets one field without opening the join's state to writes that skip its rules."
    pub fn fetch(&self) -> Fetch {
        self.fetch
    }

    /// Whether the stream refused the subscription.
    // qual:allow(dry, boilerplate) reason: "Reads or sets one field without opening the join's state to writes that skip its rules."
    pub fn refused(&self) -> bool {
        self.refused
    }

    /// The feed position to read from next.
    // qual:allow(dry, boilerplate) reason: "Reads or sets one field without opening the join's state to writes that skip its rules."
    pub fn next(&self) -> u64 {
        self.next
    }

    /// The fleet's state. Only a fleet polled without a stream
    /// ([`Link::Polling`]) lets a fetch start without one: a stream that is
    /// connecting or reconnecting is waited for, so the snapshot never
    /// starts before the events it must join with.
    // qual:allow(dry, boilerplate) reason: "Reads or sets one field without opening the join's state to writes that skip its rules."
    pub fn set_fleet_state(&mut self, state: &FleetState) {
        self.polling = *state == FleetState::Followed(Link::Polling);
    }
}

impl ActivityJoin {
    /// A join that wants its first snapshot.
    pub fn new() -> Self {
        Self {
            snapshot: None,
            fetched: None,
            error: None,
            connection: None,
            refused: false,
            polling: false,
            next: 0,
            stream_seq: None,
            pending: Pending::default(),
            fetch: Fetch::Wanted,
            failures: 0,
        }
    }

    /// The officer's turns as last joined.
    pub fn activity(&self) -> Option<&AgentActivity> {
        self.snapshot.as_ref()
    }

    /// Whether events arrive live: a stream carries the subscription.
    pub fn live(&self) -> bool {
        self.connection.is_some()
    }

    /// Asks for a fetch, as a refresh without a live stream does. A failed
    /// fetch waits for its retry instead.
    pub fn refresh(&mut self) {
        self.want();
    }

    /// Fetches again after a failure, when the retry is due or the person
    /// asks; whether a fetch is now wanted.
    pub fn retry(&mut self) -> bool {
        if self.fetch != Fetch::Failed {
            return false;
        }
        self.fetch = Fetch::Wanted;
        true
    }

    /// How long to wait before retrying the failed fetch: 1, 2, 4 …
    /// seconds up to 30 by the failures so far, or longer when Stargate
    /// asked to wait, up to the same cap. `None` when no fetch failed, or
    /// when the gateway has no such route or officer (`NotFound`).
    pub fn retry_in(&self) -> Option<Duration> {
        if self.fetch != Fetch::Failed {
            return None;
        }
        let doublings = self.failures.saturating_sub(1).min(MAX_DOUBLINGS);
        let backoff = FIRST_RETRY.saturating_mul(1 << doublings).min(MAX_RETRY);
        match self.error? {
            Error::NotFound => None,
            Error::Busy(Some(seconds)) => {
                Some(Duration::from_secs(seconds).max(backoff).min(MAX_RETRY))
            }
            _ => Some(backoff),
        }
    }

    /// Whether a fetch may start now: a stream carries the officer and has
    /// answered, or none will.
    fn may_fetch(&self) -> bool {
        self.connection.is_some() || self.polling || self.refused
    }

    /// Takes what the feed holds since the last read; whether anything
    /// shown changed or a fetch is now wanted.
    pub fn absorb(&mut self, news: ActivityNews) -> bool {
        let before = (self.fetch, self.next, self.refused);
        self.refused = news.refused;
        self.next = news.next;
        if news.connection != self.connection {
            self.connection = news.connection;
            self.stream_seq = None;
            self.drop_held();
            self.want_now();
        }
        if news.missed {
            self.drop_held();
            self.want();
        }
        let mut changed = false;
        for event in news.events {
            changed |= self.take(event);
        }
        changed || before != (self.fetch, self.next, self.refused)
    }

    /// Starts the wanted fetch, if one is wanted and none runs; whether the
    /// caller should now fetch the snapshot.
    pub fn start_fetch(&mut self) -> bool {
        if self.fetch != Fetch::Wanted || !self.may_fetch() {
            return false;
        }
        self.fetch = Fetch::InFlight {
            connection: self.connection,
            again: false,
        };
        true
    }

    /// The fetch's answer. A snapshot taken under an older connection, or
    /// while another reason to fetch arrived, is shown but fetched again.
    /// A failure keeps the last snapshot and the waiting events, and waits
    /// for a retry, unless the connection changed meanwhile.
    pub fn finish_fetch(&mut self, result: Result<AgentActivity, Error>) {
        let Fetch::InFlight { connection, again } = self.fetch else {
            return;
        };
        let moved = connection != self.connection;
        match result {
            Ok(snapshot) => {
                self.fetched = Some(snapshot.cursor.clone());
                self.snapshot = Some(snapshot);
                self.error = None;
                self.failures = 0;
                self.fetch = if again || moved {
                    Fetch::Wanted
                } else {
                    Fetch::Idle
                };
                // Covered events go first: only those left are checked
                // for a `seq` that fails to rise.
                self.pending.bytes = 0;
                for event in std::mem::take(&mut self.pending.events) {
                    self.take(event);
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.failures = self.failures.saturating_add(1);
                self.fetch = if moved { Fetch::Wanted } else { Fetch::Failed };
            }
        }
    }

    /// Wants a fetch, except after a failure, whose retry covers it.
    fn want(&mut self) {
        if self.fetch != Fetch::Failed {
            self.want_now();
        }
    }

    /// Wants a fetch, a failed one too.
    fn want_now(&mut self) {
        self.fetch = match self.fetch {
            Fetch::InFlight { connection, .. } => Fetch::InFlight {
                connection,
                again: true,
            },
            _ => Fetch::Wanted,
        };
    }

    /// One event off the stream, or one that waited for a snapshot: drops
    /// it when the fetched snapshot covers it; else, while a fetch is
    /// wanted, runs or failed, keeps it for the snapshot, unchecked, since
    /// that snapshot may cover it; else checks that `seq` rises among the
    /// live events and applies it. A covered event is no live event, so it
    /// neither fetches again nor moves the `seq` the next must pass.
    fn take(&mut self, event: AgentActivityEvent) -> bool {
        if covered(self.fetched.as_ref(), &event) {
            return false;
        }
        if self.fetch != Fetch::Idle {
            self.hold(event);
            return false;
        }
        if self.stream_seq.is_some_and(|last| event.seq <= last) {
            self.drop_held();
            self.want();
        }
        self.stream_seq = Some(event.seq);
        self.take_settled(event)
    }

    /// Waits for the fetch while one is wanted, runs or failed; else
    /// applies.
    fn take_settled(&mut self, event: AgentActivityEvent) -> bool {
        if self.fetch != Fetch::Idle {
            self.hold(event);
            return false;
        }
        match self
            .snapshot
            .as_mut()
            .map(|snapshot| apply(snapshot, event))
        {
            Some(Applied::Gap) => {
                self.want();
                true
            }
            Some(Applied::Changed) => true,
            Some(Applied::Covered) | None => false,
        }
    }

    /// Keeps `event` for the next snapshot. Past the bounds every waiting
    /// event goes: a snapshot fetched after them covers them all.
    fn hold(&mut self, event: AgentActivityEvent) {
        self.pending.bytes = self.pending.bytes.saturating_add(event.bytes);
        self.pending.events.push(event);
        if self.pending.events.len() > MAX_HELD_EVENTS || self.pending.bytes > MAX_HELD_BYTES {
            self.drop_held();
            self.want();
        }
    }

    fn drop_held(&mut self) {
        self.pending.events.clear();
        self.pending.bytes = 0;
    }
}

#[cfg(test)]
impl ActivityJoin {
    /// How many events wait for a snapshot.
    pub(crate) fn held(&self) -> usize {
        self.pending.events.len()
    }
}

/// What one event did to a snapshot.
#[derive(Debug, PartialEq, Eq)]
enum Applied {
    /// The snapshot already held it.
    Covered,
    Changed,
    /// It cannot apply: fetch again.
    Gap,
}

/// Applies `event` to `snapshot` as an upsert by ID.
fn apply(snapshot: &mut AgentActivity, event: AgentActivityEvent) -> Applied {
    if event.generation != snapshot.cursor.generation {
        return Applied::Gap;
    }
    if event.seq <= snapshot.cursor.seq {
        return Applied::Covered;
    }
    let applied = match event.change {
        ActivityChange::TurnStarted { turn }
        | ActivityChange::TurnUpdated { turn }
        | ActivityChange::TurnEnded { turn } => upsert_turn(&mut snapshot.turns, turn),
        ActivityChange::Item { item } => upsert_item(&mut snapshot.turns, &event.turn_id, *item),
    };
    if applied {
        snapshot.cursor.seq = event.seq;
        Applied::Changed
    } else {
        Applied::Gap
    }
}

/// Replaces the turn's summary, keeping its items, or adds the turn as the
/// newest; then lets go of the oldest ended turns beyond [`MAX_TURNS`].
fn upsert_turn(turns: &mut Vec<ActivityTurn>, summary: TurnSummary) -> bool {
    match turns.iter_mut().find(|turn| turn.summary.id == summary.id) {
        Some(turn) if summary.seq >= turn.summary.seq => turn.summary = summary,
        Some(_) => {}
        None => turns.push(ActivityTurn {
            summary,
            items: Vec::new(),
        }),
    }
    while turns.len() > MAX_TURNS {
        let Some(oldest) = turns
            .iter()
            .position(|turn| turn.summary.status != TurnStatus::Running)
        else {
            break;
        };
        turns.remove(oldest);
    }
    true
}

/// Replaces the item with a newer one of the same ID, or adds it; `false`
/// when its turn is not in the snapshot.
fn upsert_item(turns: &mut [ActivityTurn], turn: &str, item: ActivityEntry) -> bool {
    let Some(turn) = turns.iter_mut().find(|held| held.summary.id == turn) else {
        return false;
    };
    let id = &item.base().id;
    match turn.items.iter_mut().find(|held| held.base().id == *id) {
        Some(held) if item.base().seq >= held.base().seq => *held = item,
        Some(_) => {}
        None => turn.items.push(item),
    }
    true
}

/// Whether the last snapshot fetched, at `fetched`, already holds
/// `event`: of its generation, at or below its cursor as fetched,
/// whatever events moved the cursor since.
fn covered(fetched: Option<&ActivityCursor>, event: &AgentActivityEvent) -> bool {
    fetched.is_some_and(|cursor| cursor.generation == event.generation && event.seq <= cursor.seq)
}
