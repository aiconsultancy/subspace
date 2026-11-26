//! Coalesce roster changes without replacing full detail with the bounded feed.

use std::borrow::Cow;
use std::time::Duration;

use subspace_client_fleet::model::CrewMember;
use subspace_client_fleet::{Error, crew_detail_json_limit};

mod overlay;
mod preview;
mod retry;
mod roster;
use overlay::overlay_facts;
pub(super) use preview::{Field, PartialFields, TaskPreviews};
use retry::ReadWait;
pub(super) use roster::Roster;
use roster::current_in;

type Shown<'a> = (
    Option<Cow<'a, CrewMember>>,
    Option<&'static str>,
    TaskPreviews<'a>,
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrewRead {
    generation: u64,
    pub contexts: usize,
}

pub(super) struct DetailState {
    pub roster: Roster,
    connection: u64,
    generation: u64,
    visible: bool,
    // Demand not yet covered by the active read, independent of its result.
    pending: bool,
    wait: Option<ReadWait>,
    flight: Option<(CrewRead, Box<CrewMember>)>,
    detail: Option<FullDetail>,
    error: Option<Error>,
    retries: u32,
}

impl DetailState {
    pub fn new(roster: Roster, connection: u64) -> Self {
        Self {
            roster,
            connection,
            generation: 0,
            visible: true,
            pending: true,
            wait: None,
            flight: None,
            detail: None,
            error: None,
            retries: 0,
        }
    }

    pub fn update(&mut self, roster: Roster, connection: u64) -> bool {
        let connection_changed = self.connection_changed(connection);
        if !connection_changed && self.roster == roster {
            return false;
        }
        let invalidated = connection_changed || !self.roster.same_assignment(&roster);
        let needs_read = !invalidated
            && self
                .roster
                .needs_read(&roster, self.detail.as_ref().map(|detail| &detail.member));
        let returned = self.roster.member_returned(&roster);
        if invalidated {
            self.detail = None;
            self.generation = self.generation.wrapping_add(1);
        }
        if connection_changed {
            self.flight = None;
            self.error = None;
            self.wait = None;
        }
        self.connection = connection;
        self.roster = roster;
        if returned {
            self.probe();
        } else if invalidated {
            self.rearm();
        } else if needs_read {
            self.queue();
        }
        true
    }

    pub fn connection_changed(&self, connection: u64) -> bool {
        self.connection != connection
    }

    pub fn rearm(&mut self) {
        self.retries = 0;
        self.queue();
    }

    /// Showing the detail, Reload, reconnect or a returning member can
    /// supersede an older read. Ordinary roster updates do not poll a 404.
    pub fn probe(&mut self) {
        self.retries = 0;
        self.flight = None;
        self.pending = true;
        self.wait = None;
    }

    /// Reports a transition so the owner can let go of obsolete work.
    pub fn set_visible(&mut self, visible: bool) -> bool {
        if self.visible == visible {
            return false;
        }
        self.visible = visible;
        if visible {
            self.probe();
        } else {
            self.flight = None;
            self.wait = None;
        }
        true
    }

    pub fn queue(&mut self) {
        if self.error != Some(Error::NotFound) {
            self.pending = true;
        }
    }

    pub fn can_retry(&self) -> bool {
        (!self.pending || self.wait.is_some())
            && self.flight.is_none()
            && self.error.is_some_and(|err| err != Error::NotFound)
    }

    pub fn retry_delay(&self) -> Option<Duration> {
        self.wait.map(ReadWait::delay)
    }

    pub fn retry_elapsed(&mut self) {
        if matches!(self.wait.take(), Some(ReadWait::Retry(_))) {
            self.retries += 1;
            self.queue();
        }
    }

    pub fn begin(&mut self) -> Option<CrewRead> {
        let Roster::Present(roster) = &self.roster else {
            return None;
        };
        if !self.visible || self.flight.is_some() || !self.pending || self.wait.is_some() {
            return None;
        }
        self.pending = false;
        self.generation = self.generation.wrapping_add(1);
        let read = CrewRead {
            generation: self.generation,
            contexts: roster.contexts.len(),
        };
        self.flight = Some((read, roster.clone()));
        Some(read)
    }

    pub fn finish(&mut self, read: CrewRead, result: Result<CrewMember, Error>) {
        let Some((_, roster)) = self.flight.take_if(|(active, _)| *active == read) else {
            return;
        };
        if read.generation != self.generation {
            // An assignment change leaves the same route in flight. A probe
            // or connection change would already have cleared that flight.
            if let Err(Error::Busy(Some(seconds))) = result {
                self.wait = Some(ReadWait::Hold(Duration::from_secs(seconds)));
            }
            return;
        }
        // A read cannot cover a bigger cap that arrived while it was in flight.
        // This is new demand, not an automatic retry of InvalidResponse.
        self.pending |= crew_detail_json_limit(self.roster.context_count())
            > crew_detail_json_limit(read.contexts);
        match result {
            Ok(member) => {
                self.detail = Some(FullDetail { member, roster });
                self.error = None;
                self.retries = 0;
            }
            Err(Error::NotFound) => {
                self.detail = None;
                self.pending = false;
                self.error = Some(Error::NotFound);
            }
            Err(error) => {
                self.error = Some(error);
                self.wait = ReadWait::after(
                    error,
                    self.retries,
                    matches!(self.roster, Roster::Present(_)),
                );
            }
        }
    }

    pub fn shown(&self, ship_available: bool) -> Shown<'_> {
        let roster = if ship_available {
            &self.roster
        } else {
            &Roster::Unavailable
        };
        match roster {
            Roster::Present(member) => {
                let (shown, previews) = self.detail.as_ref().map_or_else(
                    || {
                        (
                            Cow::Borrowed(member.as_ref()),
                            TaskPreviews::from_roster(member),
                        )
                    },
                    |detail| detail.shown(member),
                );
                (Some(shown), self.note(), previews)
            }
            Roster::Gone => (
                None,
                Some("This member is no longer on the ship's roster."),
                TaskPreviews::default(),
            ),
            Roster::Unavailable => (
                None,
                Some("The ship's current crew roster is unavailable."),
                TaskPreviews::default(),
            ),
        }
    }

    pub fn has_full_detail(&self) -> bool {
        self.detail.is_some()
    }

    fn note(&self) -> Option<&'static str> {
        match (self.error, self.detail.is_some()) {
            (None, _) => None,
            (Some(Error::NotFound), false) => Some(
                "Crew detail is unavailable. Showing the ship's roster data. Older Stargate gateways don't serve crew detail.",
            ),
            (Some(_), false) => Some("Could not load crew detail. Showing the ship's roster data."),
            (Some(_), true) => Some(
                "Could not refresh crew detail. Showing the last available detail and current roster tasks.",
            ),
        }
    }
}

struct FullDetail {
    member: CrewMember,
    // Compare the bounded feed to itself, not to the full response.
    roster: Box<CrewMember>,
}

impl FullDetail {
    fn shown<'a>(&'a self, latest: &'a CrewMember) -> (Cow<'a, CrewMember>, TaskPreviews<'a>) {
        let mut shown = Cow::Borrowed(&self.member);
        let mut previews = TaskPreviews::default();
        if self.roster.activity != latest.activity {
            shown.to_mut().activity = latest.activity;
        }
        if self.roster.model != latest.model {
            shown.to_mut().model.clone_from(&latest.model);
        }
        let added = latest
            .contexts
            .iter()
            .filter(|ctx| current_in(&self.roster, ctx.project.as_deref()).is_none());
        for context in self.roster.contexts.iter().chain(added) {
            let key = context.project.as_deref();
            let previous = current_in(&self.roster, key);
            let next = current_in(latest, key);
            match (previous, next) {
                (Some(previous), Some(next)) => previews.insert(
                    next.project.as_deref(),
                    overlay_facts(&mut shown, previous, next),
                ),
                (None, Some(next)) if current_in(&shown, key).is_none() => {
                    shown.to_mut().contexts.push(next.clone());
                    previews.insert(
                        next.project.as_deref(),
                        PartialFields::from_roster(next.current.as_ref()),
                    );
                }
                (Some(_), None) => shown
                    .to_mut()
                    .contexts
                    .retain(|ctx| ctx.project.as_deref() != key),
                (None, _) => (),
            }
        }
        (shown, previews)
    }
}
