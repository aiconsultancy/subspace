//! Following one officer's turns without a gap: the snapshot route joined
//! with the `agent-activity` events of the fleet's one stream, as
//! Stargate's *Live activity events* describes.
//!
//! 1. Subscribe ([`crate::FleetApi::watch_activity`]) and keep the events.
//! 2. Once the stream that carries the officer has answered, and not
//!    before, fetch the snapshot: Stargate watches the officer only once
//!    the events response has started, so a snapshot fetched alongside it
//!    could miss changes with no `seq` to tell. Events that arrive
//!    meanwhile wait. Without a stream (the fleet is polled, or the stream
//!    refused the officer) the view fetches on a timer instead
//!    ([`ActivityJoin::set_polling`]).
//! 3. Apply each waiting or later event whose `seq` is above the
//!    snapshot's `cursor.seq`, as an upsert by ID; drop the rest.
//! 4. Fetch again, one fetch at a time, when the stream reconnects or
//!    closes, when an event's `generation` is not the cursor's, when `seq`
//!    fails to rise, when an item names a turn the snapshot lacks, or when
//!    the reader fell so far behind that the feed dropped events for it.
//!
//! `seq` skips values for coalesced changes, so a skipped value is not a
//! gap: only a `seq` that does not rise is. `seq` counts per officer, so
//! each join follows one officer, with its own cursor and last `seq`.

use super::{ActivityChange, ActivityEntry, ActivityTurn, AgentActivity, AgentActivityEvent};
use super::{TurnStatus, TurnSummary};
use crate::Error;

/// The most turns Stargate keeps for an officer, and so the most kept here.
const MAX_TURNS: usize = 16;

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
    /// The snapshot is current, or the last fetch failed.
    Idle,
    /// A fetch should start ([`ActivityJoin::start_fetch`]).
    Wanted,
    /// A fetch runs for the stream connection it started under; `again`
    /// when a reason to fetch arrived while it ran.
    InFlight {
        connection: Option<u64>,
        again: bool,
    },
}

/// One officer's turns, kept current from the snapshot and the events.
pub struct ActivityJoin {
    snapshot: Option<AgentActivity>,
    error: Option<Error>,
    connection: Option<u64>,
    refused: bool,
    /// No stream carries the officer: fetch without waiting for one.
    polling: bool,
    next: u64,
    /// The last event's `seq` on this connection, which the next must pass.
    stream_seq: Option<u64>,
    /// Events that wait for the fetch in flight.
    pending: Vec<AgentActivityEvent>,
    fetch: Fetch,
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

    /// Whether the fleet is followed without a stream (polled), so a fetch
    /// need not wait for one to answer.
    // qual:allow(dry, boilerplate) reason: "Reads or sets one field without opening the join's state to writes that skip its rules."
    pub fn set_polling(&mut self, polling: bool) {
        self.polling = polling;
    }
}

impl ActivityJoin {
    /// A join that wants its first snapshot.
    pub fn new() -> Self {
        Self {
            snapshot: None,
            error: None,
            connection: None,
            refused: false,
            polling: false,
            next: 0,
            stream_seq: None,
            pending: Vec::new(),
            fetch: Fetch::Wanted,
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

    /// Asks for a fetch, as a refresh without a live stream does.
    pub fn refresh(&mut self) {
        self.want();
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
            self.pending.clear();
            self.want();
        }
        if news.missed {
            self.pending.clear();
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
    pub fn finish_fetch(&mut self, result: Result<AgentActivity, Error>) {
        let Fetch::InFlight { connection, again } = self.fetch else {
            return;
        };
        match result {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.error = None;
                self.fetch = if again || connection != self.connection {
                    Fetch::Wanted
                } else {
                    Fetch::Idle
                };
                for event in std::mem::take(&mut self.pending) {
                    self.take_settled(event);
                }
            }
            Err(error) => {
                self.error = Some(error);
                self.pending.clear();
                self.fetch = Fetch::Idle;
            }
        }
    }

    fn want(&mut self) {
        self.fetch = match self.fetch {
            Fetch::InFlight { connection, .. } => Fetch::InFlight {
                connection,
                again: true,
            },
            _ => Fetch::Wanted,
        };
    }

    /// One event off the stream: checks that `seq` rises, then waits for
    /// the fetch or applies it.
    fn take(&mut self, event: AgentActivityEvent) -> bool {
        if self.stream_seq.is_some_and(|last| event.seq <= last) {
            self.pending.clear();
            self.want();
        }
        self.stream_seq = Some(event.seq);
        self.take_settled(event)
    }

    /// Waits for the fetch while one is wanted or runs; else applies.
    fn take_settled(&mut self, event: AgentActivityEvent) -> bool {
        if self.fetch != Fetch::Idle {
            self.pending.push(event);
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
