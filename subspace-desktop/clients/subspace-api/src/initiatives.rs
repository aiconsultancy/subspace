//! The desktop's initiative memberships and selection for its API profile.
//! Once the memberships are known, a person who belongs to an initiative
//! always has one selected: the one they last chose for this account and
//! organisation while they still belong to it, else the first in the API's
//! order. Only someone who belongs to none has no selection.

use std::time::Instant;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Task};

use crate::{Error, model::MemberInitiative};

mod fleet_retry;
mod outcome;
mod picker;
mod read;
mod remembered;
#[cfg(any(test, feature = "test-support"))]
mod testing;

use fleet_retry::FleetRetry;
use outcome::Outcome;
use read::{Read, memberships};

const UNAVAILABLE: &str = "Initiatives unavailable. Reload to retry.";
/// What the Spaces say while the person belongs to no initiative.
pub const NO_MEMBERSHIPS: &str = "Not a member of any initiative yet";
/// What the Spaces say while the memberships are first read.
pub const CHECKING: &str = "Checking initiatives…";

/// The time the request began and the API's independent identity result.
pub type IdentityObservation = (Instant, Result<String, Error>);

struct Session(Entity<Catalog>);
impl Global for Session {}

struct Refresh {
    started_at: Instant,
    /// The account and organisation scope the read was made for.
    scope: Option<u64>,
    _task: Task<()>,
}

#[derive(Default)]
pub struct Catalog {
    items: Vec<MemberInitiative>,
    selected: Option<String>,
    actor: Option<String>,
    verified_at: Option<Instant>,
    revision: u64,
    fetch: Option<Refresh>,
    outcome: Outcome,
    fleet_retry: FleetRetry,
    /// The account and organisation scope the items belong to. A NETT
    /// organisation switch keeps the person but changes the scope, which
    /// retires the items, the selection and any read still in flight.
    scope: Option<u64>,
    /// Where the person's choice is remembered: the API, verified user and
    /// organisation the latest successful read was made for.
    place: Option<String>,
    /// How many times the person's own pick moved the selection.
    picks: u64,
}

impl Catalog {
    /// Both spaces observe the same service session, including across windows.
    pub fn shared(cx: &mut App) -> Entity<Self> {
        if let Some(session) = cx.try_global::<Session>() {
            return session.0.clone();
        }
        let catalog = cx.new(|_| Self::default());
        cx.set_global(Session(catalog.clone()));
        catalog
    }

    pub fn has_memberships(&self) -> bool {
        !self.items.is_empty()
    }

    /// Whether the latest read of the memberships succeeded for the current
    /// account and organisation, so who belongs to which fleet is known.
    /// False before the first read lands, after a read failed or was
    /// refused, also after earlier ones succeeded, and once the account or
    /// organisation changes.
    pub fn memberships_known(&self) -> bool {
        self.outcome.known == Some(self.scope)
    }

    /// Whether the latest settled read for this account and organisation
    /// found that the person belongs to no initiative. A read that then
    /// fails in transit leaves it so. Never before a read settled, after a
    /// refusal, or against an API without memberships.
    pub fn no_memberships(&self) -> bool {
        self.items.is_empty() && self.outcome.empty == Some(self.scope)
    }

    /// Whether a read of the memberships is in flight.
    pub fn reading(&self) -> bool {
        self.fetch.is_some()
    }

    /// Whether the latest read was refused for want of a session, which at
    /// start can mean a saved one is still being restored.
    pub fn sign_in_needed(&self) -> bool {
        self.outcome.sign_in_needed
    }

    /// Whether the person's own pick moved the selection since `seen`, the
    /// count a reader last saw, which this brings up to date: so it can tell
    /// their switch from a fallback or a default.
    pub fn picked_since(&self, seen: &mut u64) -> bool {
        std::mem::replace(seen, self.picks) != self.picks
    }

    /// The user the catalogue belongs to, verified or not yet.
    pub fn actor(&self) -> Option<&str> {
        self.actor.as_deref()
    }

    /// The account scope and verified user the memberships were read for,
    /// while the latest read succeeded for the current scope; so a reader
    /// can tell they belong to the account it serves.
    pub fn verified_for(&self) -> Option<(Option<u64>, &str)> {
        if !self.memberships_known() {
            return None;
        }
        Some((self.scope, self.actor.as_deref()?))
    }

    /// The caller's initiatives, in the API's order.
    // qual:api
    pub fn memberships(&self) -> &[MemberInitiative] {
        &self.items
    }

    // qual:api
    pub fn revision(&self) -> u64 {
        self.revision
    }

    // qual:api
    pub fn selected(&self) -> Option<&MemberInitiative> {
        let id = self.selected.as_deref()?;
        self.items.iter().find(|item| item.id == id)
    }

    /// Whether the latest read of the memberships failed because the API
    /// could not be reached at all.
    pub fn unreachable(&self) -> bool {
        self.outcome.unreachable
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_using(crate::native::generation(), memberships, cx);
    }

    /// Reads the memberships for `scope` with `read`. A new scope first
    /// retires everything read for the old one, a read in flight included.
    fn refresh_using(
        &mut self,
        scope: Option<u64>,
        read: impl FnOnce() -> Read + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.fence_scope(scope) {
            cx.notify();
        }
        if self.fetch.is_some() {
            return;
        }
        let started_at = Instant::now();
        let scope = self.scope;
        let read = cx.background_spawn(async move { read() });
        let task = cx.spawn(async move |catalog, cx| {
            let read = read.await;
            _ = catalog.update(cx, |catalog, cx| catalog.finish(started_at, read, cx));
        });
        self.fetch = Some(Refresh {
            started_at,
            scope,
            _task: task,
        });
        cx.notify();
    }

    /// Retire the old membership and cancel an in-flight refresh on a proven
    /// change of actor or an explicit API access refusal.
    pub fn observe_identity(&mut self, observation: &IdentityObservation, cx: &mut Context<Self>) {
        if self.record_identity(observation) {
            if observation.1.is_ok() {
                self.refresh(cx);
            } else {
                cx.notify();
            }
        }
    }

    /// Retires everything read for another account or organisation scope:
    /// the items, the selection, fleet retries and a read in flight, whose
    /// answer is then dropped. True when something was retired.
    fn fence_scope(&mut self, scope: Option<u64>) -> bool {
        if self.scope == scope {
            return false;
        }
        self.scope = scope;
        let actor = self.actor.clone();
        self.reset(actor);
        true
    }

    fn record_identity(&mut self, (started_at, identity): &IdentityObservation) -> bool {
        if self.verified_at.is_some_and(|seen| seen >= *started_at) || !authoritative(identity) {
            return false;
        }
        self.verified_at = Some(*started_at);
        self.fence_identity(identity)
    }

    fn fence_identity(&mut self, identity: &Result<String, Error>) -> bool {
        let actor = match identity {
            Ok(actor) => Some(actor.as_str()),
            Err(Error::SignInNeeded | Error::Forbidden) => None,
            Err(_) => return false,
        };
        if self.actor.as_deref() == actor && (actor.is_some() || self.fetch.is_none()) {
            return false;
        }
        self.reset(actor.map(str::to_owned));
        true
    }

    fn reset(&mut self, actor: Option<String>) {
        self.fetch = None;
        self.actor = actor;
        self.items.clear();
        self.selected = None;
        self.place = None;
        self.outcome = Outcome::default();
        self.fleet_retry = FleetRetry::default();
        self.revision = self.revision.wrapping_add(1);
    }

    fn finish(&mut self, started_at: Instant, read: Read, cx: &mut Context<Self>) {
        if self
            .fetch
            .as_ref()
            .is_none_or(|fetch| fetch.started_at != started_at || fetch.scope != self.scope)
        {
            return;
        }
        self.fetch = None;
        let refused = matches!(read.result, Err(Error::SignInNeeded | Error::Forbidden));
        let identity_differs = refused
            || read
                .actor
                .as_ref()
                .is_some_and(|actor| Some(actor) != self.actor.as_ref());
        if self.verified_at.is_some_and(|seen| seen > started_at) && identity_differs {
            cx.notify();
            return;
        }
        if read.actor.is_some() || refused {
            self.verified_at = self.verified_at.max(Some(started_at));
        }
        self.apply(read, cx);
    }

    /// Takes in a read for the current scope, with the choice remembered
    /// where it was made.
    fn apply(&mut self, read: Read, cx: &mut Context<Self>) {
        let remembered = read
            .place
            .as_deref()
            .and_then(|place| remembered::get(place, cx));
        self.replace(read, remembered.as_deref());
        self.retry_fleet_lookups(cx);
        cx.notify();
    }

    /// While an assigned fleet could not be read, read the memberships again
    /// with backoff; Channels keeps that initiative's autojoin off meanwhile.
    fn retry_fleet_lookups(&mut self, cx: &mut Context<Self>) {
        let unresolved = self.items.iter().any(fleet_unresolved);
        let Some(delay) = self.fleet_retry.next(unresolved) else {
            self.fleet_retry.task = None;
            return;
        };
        self.fleet_retry.task = Some(cx.spawn(async move |catalog, cx| {
            cx.background_executor().timer(delay).await;
            _ = catalog.update(cx, Self::refresh);
        }));
    }

    fn replace(&mut self, read: Read, remembered: Option<&str>) {
        let Read {
            actor,
            place,
            result,
        } = read;
        let verified =
            actor.is_some() || matches!(result, Err(Error::SignInNeeded | Error::Forbidden));
        if verified && self.actor != actor {
            self.reset(actor);
        }
        self.fetch = None;
        self.outcome.record(self.scope, &result);
        if result.is_ok() {
            self.place = place;
        }
        if !self.outcome.failed {
            let items = result.unwrap_or_default();
            if self.items != items {
                self.revision = self.revision.wrapping_add(1);
                self.items = items;
            }
        }
        self.settle_selection(remembered);
    }

    /// Keeps the selection while the person still belongs to it. Otherwise,
    /// once the memberships are known, selects `remembered` while they still
    /// belong to it, else their first initiative; with none, nothing.
    fn settle_selection(&mut self, remembered: Option<&str>) {
        let choice = settled_choice(
            &self.items,
            self.selected.as_deref(),
            self.memberships_known(),
            remembered,
        );
        self.set_selected(choice);
    }

    /// Selects `choice`, counting a change as a new revision.
    fn set_selected(&mut self, choice: Option<String>) {
        if self.selected != choice {
            self.selected = choice;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    /// Selects the initiative the person chose and remembers it for this
    /// account and organisation. A value that is not one of their
    /// initiatives changes nothing: there is no choice of none.
    // qual:api
    pub fn pick(&mut self, value: &str, cx: &mut Context<Self>) {
        if !self.items.iter().any(|item| item.id == value) {
            return;
        }
        if let Some(place) = &self.place {
            remembered::set(place, value, cx);
        }
        let before = self.revision;
        self.set_selected(Some(value.to_owned()));
        if self.revision != before {
            self.picks = self.picks.wrapping_add(1);
            cx.notify();
        }
    }
}

/// Whether the person is shut out of what initiatives scope, in a build
/// that offers `capabilities`: only where initiatives carry channels, once a
/// settled read found that they belong to none. A build without an API
/// offers none, so it never shuts anyone out.
pub fn not_a_member(capabilities: crate::Capabilities, catalog: &Catalog) -> bool {
    capabilities.initiative_channels && catalog.no_memberships()
}

/// The initiative to select among `items`: `selected` while it is one of
/// them; otherwise, once they are `known`, `remembered` while it is one of
/// them, else the first in the API's order; before then, none.
fn settled_choice(
    items: &[MemberInitiative],
    selected: Option<&str>,
    known: bool,
    remembered: Option<&str>,
) -> Option<String> {
    let member = |id: &str| items.iter().find(|item| item.id == id);
    let fallback = || {
        known
            .then(|| remembered.and_then(member).or_else(|| items.first()))
            .flatten()
    };
    selected
        .and_then(member)
        .or_else(fallback)
        .map(|item| item.id.clone())
}

/// An initiative assigned to a fleet whose details could not be read.
fn fleet_unresolved(item: &MemberInitiative) -> bool {
    item.fleet_id.is_some() && item.fleet.is_none()
}

fn authoritative(identity: &Result<String, Error>) -> bool {
    matches!(
        identity,
        Ok(_) | Err(Error::SignInNeeded | Error::Forbidden)
    )
}

#[cfg(test)]
mod tests;
