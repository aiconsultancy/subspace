//! Coalesce roster changes without replacing full detail with the bounded feed.

use std::borrow::Cow;
use std::time::Duration;

use subspace_client_fleet::Error;
use subspace_client_fleet::model::{CrewContext, CrewMember};

mod retry;
mod roster;
use retry::retry_delay;
pub(super) use roster::Roster;
use roster::{current_in, same_task_facts};

pub(super) struct DetailState {
    pub roster: Roster,
    connection: u64,
    generation: u64,
    visible: bool,
    // Demand not yet covered by the active read, independent of its result.
    pending: bool,
    // A transient failure gates pending demand behind the retry policy.
    waiting_to_retry: bool,
    flight: Option<(u64, Box<CrewMember>)>,
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
            waiting_to_retry: false,
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
        let task_changed = !self.roster.same_task_revision(&roster);
        let returned = self.roster.member_returned(&roster);
        if invalidated {
            self.detail = None;
            self.generation = self.generation.wrapping_add(1);
        }
        if connection_changed {
            self.flight = None;
            self.error = None;
            self.waiting_to_retry = false;
        }
        self.connection = connection;
        self.roster = roster;
        if returned {
            self.probe();
        } else if invalidated {
            self.rearm();
        } else if task_changed {
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
        self.waiting_to_retry = false;
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
        }
        true
    }

    pub fn queue(&mut self) {
        if self.error != Some(Error::NotFound) {
            self.pending = true;
        }
    }

    pub fn can_retry(&self) -> bool {
        (!self.pending || self.waiting_to_retry)
            && self.flight.is_none()
            && self.error.is_some_and(|err| err != Error::NotFound)
    }

    pub fn retry_delay(&self) -> Option<Duration> {
        if !self.can_retry() {
            return None;
        }
        self.automatic_retry_delay(self.error?)
    }

    fn automatic_retry_delay(&self, error: Error) -> Option<Duration> {
        if !matches!(self.roster, Roster::Present(_)) {
            return None;
        }
        retry_delay(error, self.retries)
    }

    pub fn retry_elapsed(&mut self) {
        self.retries += 1;
        self.waiting_to_retry = false;
        self.queue();
    }

    pub fn begin(&mut self) -> Option<u64> {
        let Roster::Present(roster) = &self.roster else {
            return None;
        };
        if !self.visible || self.flight.is_some() || !self.pending || self.waiting_to_retry {
            return None;
        }
        self.pending = false;
        self.generation = self.generation.wrapping_add(1);
        self.flight = Some((self.generation, roster.clone()));
        Some(self.generation)
    }

    pub fn finish(&mut self, generation: u64, result: Result<CrewMember, Error>) {
        let Some((_, roster)) = self.flight.take_if(|(active, _)| *active == generation) else {
            return;
        };
        if generation != self.generation {
            return;
        }
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
                self.waiting_to_retry = self.automatic_retry_delay(error).is_some();
            }
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
    fn shown<'a>(&'a self, latest: &CrewMember) -> Cow<'a, CrewMember> {
        let mut shown = Cow::Borrowed(&self.member);
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
                (Some(previous), Some(next)) => overlay_facts(&mut shown, previous, next),
                (None, Some(next)) if current_in(&shown, key).is_none() => {
                    shown.to_mut().contexts.push(next.clone());
                }
                (Some(_), None) => shown
                    .to_mut()
                    .contexts
                    .retain(|ctx| ctx.project.as_deref() != key),
                (None, _) => (),
            }
        }
        shown
    }
}

macro_rules! overlay {
    ($shown:ident, $old:ident, $new:ident; $($field:ident),+ $(,)?) => {{$(
        if $old.$field != $new.$field {
            $shown.$field.clone_from(&$new.$field);
        }
    )+}};
}

fn overlay_facts(shown: &mut Cow<'_, CrewMember>, previous: &CrewContext, next: &CrewContext) {
    let git_changed = match (&previous.git, &next.git) {
        (Some(old), Some(new)) => {
            let mut old = old.clone();
            old.checked_at = new.checked_at;
            &old != new
        }
        (old, new) => old != new,
    };
    let task_changed = !same_task_facts(previous.current.as_ref(), next.current.as_ref());
    if !needs_overlay(previous, next, git_changed || task_changed) {
        return;
    }
    let Some(context) = shown
        .to_mut()
        .contexts
        .iter_mut()
        .find(|ctx| ctx.project == next.project)
    else {
        shown.to_mut().contexts.push(next.clone());
        return;
    };
    if git_changed {
        match (&mut context.git, &previous.git, &next.git) {
            (Some(shown), Some(old), Some(new)) => {
                overlay!(shown, old, new; repository, head, base, branch, ahead, behind, changed_files, error)
            }
            (shown, _, new) => shown.clone_from(new),
        }
    }
    if previous.pull_request != next.pull_request {
        match (
            &mut context.pull_request,
            &previous.pull_request,
            &next.pull_request,
        ) {
            (Some(shown), Some(old), Some(new)) if old.number == new.number => {
                overlay!(shown, old, new; title, url, state, checks)
            }
            (shown, _, new) => shown.clone_from(new),
        }
    }
    if task_changed {
        match (&mut context.current, &previous.current, &next.current) {
            (Some(shown), Some(old), Some(new)) if shown.id == new.id => {
                overlay!(shown, old, new; status, updated_at, summary, phase, progress)
            }
            (shown, _, new) => shown.clone_from(new),
        }
    }
    overlay!(context, previous, next; activity, provisioned, pull_request_known);
}

fn needs_overlay(previous: &CrewContext, next: &CrewContext, details_changed: bool) -> bool {
    details_changed
        || previous.activity != next.activity
        || previous.provisioned != next.provisioned
        || previous.pull_request != next.pull_request
        || previous.pull_request_known != next.pull_request_known
}
