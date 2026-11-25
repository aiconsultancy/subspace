//! Coalesce roster changes without replacing full detail with the bounded feed.

use std::borrow::Cow;

use subspace_client_fleet::Error;
use subspace_client_fleet::model::{CrewContext, CrewMember, ShipDetail, Task};

#[derive(PartialEq, Eq)]
pub(super) enum Roster {
    Present(Box<CrewMember>),
    Gone,
    Unavailable,
}

impl Roster {
    pub(super) fn find(detail: Option<ShipDetail>, member: &str) -> Self {
        let Some(detail) = detail.filter(|detail| detail.ship.available && !detail.detail_stale)
        else {
            return Self::Unavailable;
        };
        detail
            .crew_members
            .into_iter()
            .find(|entry| entry.id == member)
            .map_or(Self::Gone, |member| Self::Present(Box::new(member)))
    }
}

pub(super) struct DetailState {
    pub roster: Roster,
    directory: u64,
    generation: u64,
    pending: bool,
    flight: Option<u64>,
    detail: Option<FullDetail>,
    error: Option<Error>,
}

impl DetailState {
    pub fn new(roster: Roster, directory: u64) -> Self {
        Self {
            roster,
            directory,
            generation: 0,
            pending: true,
            flight: None,
            detail: None,
            error: None,
        }
    }

    pub fn update(&mut self, roster: Roster, directory: u64) -> bool {
        let directory_changed = self.directory != directory;
        if !directory_changed && self.roster == roster {
            return false;
        }
        if directory_changed || matches!(roster, Roster::Gone) {
            self.detail = None;
        }
        self.directory = directory;
        self.roster = roster;
        self.generation = self.generation.wrapping_add(1);
        self.pending = true;
        self.error = None;
        true
    }

    pub fn begin(&mut self) -> Option<u64> {
        if self.flight.is_some() || !self.pending || !matches!(self.roster, Roster::Present(_)) {
            return None;
        }
        self.pending = false;
        self.flight = Some(self.generation);
        self.flight
    }

    pub fn finish(&mut self, generation: u64, result: Result<CrewMember, Error>) {
        if self.flight != Some(generation) {
            return;
        }
        self.flight = None;
        if generation != self.generation {
            return;
        }
        match result {
            Ok(member) => self.detail = FullDetail::new(member, &self.roster),
            Err(Error::NotFound) => {
                self.detail = None;
                self.error = Some(Error::NotFound);
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn shown(
        &self,
        ship_available: bool,
    ) -> (Option<Cow<'_, CrewMember>>, Option<&'static str>) {
        let roster = if ship_available {
            &self.roster
        } else {
            &Roster::Unavailable
        };
        match roster {
            Roster::Present(member) => (
                Some(
                    self.detail
                        .as_ref()
                        .map_or(Cow::Borrowed(member.as_ref()), |detail| {
                            detail.shown(member)
                        }),
                ),
                self.note(),
            ),
            Roster::Gone => (None, Some("This member is no longer on the ship's roster.")),
            Roster::Unavailable => (None, Some("The ship's current crew roster is unavailable.")),
        }
    }

    pub fn current_task<'a>(&'a self, shown: &'a CrewMember) -> Option<&'a Task> {
        if let (Some(detail), Roster::Present(latest)) = (&self.detail, &self.roster) {
            return detail.current_task(latest);
        }
        shown.current_task()
    }

    pub fn has_full_detail(&self) -> bool {
        self.detail.is_some()
    }

    fn note(&self) -> Option<&'static str> {
        match (self.error, self.detail.is_some()) {
            (None, _) => None,
            (Some(Error::NotFound), false) => {
                Some("Crew detail is unavailable. Showing the ship's roster data.")
            }
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
    roster: CrewMember,
}

impl FullDetail {
    fn new(member: CrewMember, roster: &Roster) -> Option<Self> {
        let Roster::Present(roster) = roster else {
            return None;
        };
        Some(Self {
            member,
            roster: roster.as_ref().clone(),
        })
    }

    fn shown<'a>(&'a self, latest: &CrewMember) -> Cow<'a, CrewMember> {
        let mut shown = Cow::Borrowed(&self.member);
        let added = latest
            .contexts
            .iter()
            .filter(|ctx| current_in(&self.roster, ctx.project.as_deref()).is_none());
        for context in self.roster.contexts.iter().chain(added) {
            let key = context.project.as_deref();
            let previous = current_in(&self.roster, key).and_then(|ctx| ctx.current.as_ref());
            let next = current_in(latest, key);
            if task_text(previous) != task_text(next.and_then(|ctx| ctx.current.as_ref())) {
                replace_task(shown.to_mut(), key, next);
            }
        }
        shown
    }

    fn current_task<'a>(&'a self, latest: &'a CrewMember) -> Option<&'a Task> {
        let next = latest.current_task();
        if task_text(self.roster.current_task()) != task_text(next) {
            return next;
        }
        self.member.current_task()
    }
}

fn current_in<'a>(member: &'a CrewMember, key: Option<&str>) -> Option<&'a CrewContext> {
    member
        .contexts
        .iter()
        .find(|ctx| ctx.project.as_deref() == key)
}

fn replace_task(member: &mut CrewMember, key: Option<&str>, next: Option<&CrewContext>) {
    if let Some(context) = member
        .contexts
        .iter_mut()
        .find(|ctx| ctx.project.as_deref() == key)
    {
        context.current = next.and_then(|ctx| ctx.current.clone());
        if let Some(next) = next {
            context.activity = next.activity;
        }
    } else if let Some(next) = next {
        member.contexts.push(next.clone());
    }
}

fn task_text(task: Option<&Task>) -> Option<(&str, &str, &str, &str)> {
    task.map(|task| {
        (
            task.id.as_str(),
            task.summary.as_str(),
            task.phase.as_str(),
            task.progress.as_str(),
        )
    })
}
