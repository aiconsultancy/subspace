//! The desktop's in-memory initiative membership and selection for its API profile.

use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Task};

use crate::{Client, Error, model::MemberInitiative};

mod picker;

const CLEAR: &str = "local";
/// The first wait before reading memberships again after a fleet lookup
/// failed; each further failure doubles it, up to the maximum.
const FLEET_RETRY_FIRST: Duration = Duration::from_secs(5);
const FLEET_RETRY_MAX: Duration = Duration::from_secs(300);
const FLEET_RETRY_MAX_DOUBLINGS: u32 = 6;

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
    failed: bool,
    fleet_retry: Option<Task<()>>,
    fleet_failures: u32,
    /// The account and organisation scope the items belong to. A NETT
    /// organisation switch keeps the person but changes the scope, which
    /// retires the items, the selection and any read still in flight.
    scope: Option<u64>,
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

    /// Whether the first read of the memberships is still in flight: who
    /// belongs to which fleet is not known yet.
    pub fn checking(&self) -> bool {
        self.fetch.is_some() && self.items.is_empty()
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

    // qual:api
    pub fn hint(&self) -> Option<&'static str> {
        if self.failed {
            return Some("Initiatives unavailable. Reload to retry.");
        }
        (self.fetch.is_some() && self.items.is_empty()).then_some("Checking initiatives…")
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.refresh_using(crate::native::generation(), memberships, cx);
    }

    /// Reads the memberships for `scope` with `read`. A new scope first
    /// retires everything read for the old one, a read in flight included.
    fn refresh_using(
        &mut self,
        scope: Option<u64>,
        read: impl FnOnce() -> (Option<String>, Result<Vec<MemberInitiative>, Error>) + Send + 'static,
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
            let (actor, result) = read.await;
            _ = catalog.update(cx, |catalog, cx| {
                catalog.finish(started_at, actor, result, cx)
            });
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
        self.failed = false;
        self.fleet_retry = None;
        self.fleet_failures = 0;
        self.revision = self.revision.wrapping_add(1);
    }

    fn finish(
        &mut self,
        started_at: Instant,
        actor: Option<String>,
        result: Result<Vec<MemberInitiative>, Error>,
        cx: &mut Context<Self>,
    ) {
        if self
            .fetch
            .as_ref()
            .is_none_or(|fetch| fetch.started_at != started_at || fetch.scope != self.scope)
        {
            return;
        }
        self.fetch = None;
        let refused = matches!(result, Err(Error::SignInNeeded | Error::Forbidden));
        let identity_differs = refused
            || actor
                .as_ref()
                .is_some_and(|actor| Some(actor) != self.actor.as_ref());
        if self.verified_at.is_some_and(|seen| seen > started_at) && identity_differs {
            cx.notify();
            return;
        }
        if actor.is_some() || refused {
            self.verified_at = self.verified_at.max(Some(started_at));
        }
        self.replace(actor, result);
        self.retry_fleet_lookups(cx);
        cx.notify();
    }

    /// While an assigned fleet could not be read, read the memberships again
    /// with backoff; Channels keeps that initiative's autojoin off meanwhile.
    fn retry_fleet_lookups(&mut self, cx: &mut Context<Self>) {
        let Some(delay) = self.next_fleet_retry() else {
            self.fleet_retry = None;
            return;
        };
        self.fleet_retry = Some(cx.spawn(async move |catalog, cx| {
            cx.background_executor().timer(delay).await;
            _ = catalog.update(cx, Self::refresh);
        }));
    }

    /// The wait before the next retry, counting this failure, or `None`
    /// once every assigned fleet is known.
    fn next_fleet_retry(&mut self) -> Option<Duration> {
        if !self.items.iter().any(fleet_unresolved) {
            self.fleet_failures = 0;
            return None;
        }
        let doublings = self.fleet_failures.min(FLEET_RETRY_MAX_DOUBLINGS);
        self.fleet_failures = self.fleet_failures.saturating_add(1);
        Some(
            FLEET_RETRY_FIRST
                .saturating_mul(1 << doublings)
                .min(FLEET_RETRY_MAX),
        )
    }

    fn replace(&mut self, actor: Option<String>, result: Result<Vec<MemberInitiative>, Error>) {
        let verified =
            actor.is_some() || matches!(result, Err(Error::SignInNeeded | Error::Forbidden));
        if verified && self.actor != actor {
            self.reset(actor);
        }
        self.fetch = None;
        self.failed = matches!(
            result,
            Err(error) if !matches!(
                error,
                Error::NotFound | Error::SignInNeeded | Error::Forbidden | Error::OrganisationNeeded
            )
        );
        if !self.failed {
            let items = result.unwrap_or_default();
            if self.items != items {
                self.revision = self.revision.wrapping_add(1);
                self.items = items;
            }
        }
        if self.selected().is_none() {
            self.selected = None;
        }
    }

    // qual:api
    pub fn pick(&mut self, value: &str, cx: &mut Context<Self>) {
        let id = (value != CLEAR).then_some(value);
        if self.select(id) {
            cx.notify();
        }
    }

    fn select(&mut self, id: Option<&str>) -> bool {
        if self.selected.as_deref() == id
            || id.is_some_and(|id| !self.items.iter().any(|item| item.id == id))
        {
            return false;
        }
        self.selected = id.map(str::to_owned);
        self.revision = self.revision.wrapping_add(1);
        true
    }

    #[cfg(feature = "test-support")]
    // qual:test_helper
    pub fn set_memberships_for_test(
        &mut self,
        actor: Option<String>,
        result: Result<Vec<MemberInitiative>, Error>,
        cx: &mut Context<Self>,
    ) {
        self.replace(actor, result);
        self.retry_fleet_lookups(cx);
        cx.notify();
    }

    /// Reads the memberships for `scope` with `read`, as an organisation
    /// switch's refresh does.
    #[cfg(feature = "test-support")]
    // qual:test_helper
    pub fn refresh_for_test(
        &mut self,
        scope: Option<u64>,
        read: impl FnOnce() -> (Option<String>, Result<Vec<MemberInitiative>, Error>) + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.refresh_using(scope, read, cx);
    }

    /// Moves the catalogue to another account or organisation scope, as an
    /// organisation switch does.
    #[cfg(feature = "test-support")]
    // qual:test_helper
    pub fn change_scope_for_test(&mut self, scope: Option<u64>, cx: &mut Context<Self>) {
        if self.fence_scope(scope) {
            cx.notify();
        }
    }

    #[cfg(feature = "test-support")]
    // qual:test_helper
    pub fn fleet_retry_pending_for_test(&self) -> bool {
        self.fleet_retry.is_some()
    }
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

fn memberships() -> (Option<String>, Result<Vec<MemberInitiative>, Error>) {
    let client = match Client::new() {
        Ok(client) => client,
        Err(error) => return (None, Err(error)),
    };
    match client.me() {
        Ok(actor) => (Some(actor.user_id), client.member_initiatives()),
        Err(error) => (None, Err(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::{Catalog, Error, FLEET_RETRY_MAX};
    use std::time::Duration;

    #[test]
    fn a_failed_fleet_lookup_is_retried_with_capped_backoff_until_it_resolves() {
        let mut catalog = Catalog::default();
        let unresolved = serde_json::from_str(
            r##"[{"id":"one","name":"One","channels":"#one","fleetId":"fleet-1","fleet":null}]"##,
        )
        .unwrap();
        catalog.replace(Some("first".into()), Ok(unresolved));
        let delays: Vec<_> = (0..9).map(|_| catalog.next_fleet_retry()).collect();
        assert_eq!(delays[0], Some(Duration::from_secs(5)));
        assert_eq!(delays[1], Some(Duration::from_secs(10)));
        assert_eq!(delays[5], Some(Duration::from_secs(160)));
        assert_eq!(delays[8], Some(FLEET_RETRY_MAX));

        let resolved = serde_json::from_str(
            r##"[{"id":"one","name":"One","channels":"#one","fleetId":"fleet-1",
                 "fleet":{"id":"fleet-1","ircHost":"irc.example.com","ircPort":6697}}]"##,
        )
        .unwrap();
        catalog.replace(Some("first".into()), Ok(resolved));
        assert_eq!(catalog.next_fleet_retry(), None);
        assert_eq!(catalog.fleet_failures, 0, "the next failure starts again");
    }

    #[test]
    fn verified_identity_change_or_refusal_fences_the_loaded_catalogue() {
        let mut catalog = Catalog::default();
        let entries =
            serde_json::from_str(r##"[{"id":"test","name":"Fixture","channels":"#data"}]"##)
                .unwrap();
        catalog.replace(Some("first".into()), Ok(entries));
        assert!(catalog.select(Some("test")));
        assert!(!catalog.fence_identity(&Ok("first".into())));
        assert!(!catalog.fence_identity(&Err(Error::Network)));
        assert!(catalog.selected().is_some());

        assert!(catalog.fence_identity(&Ok("second".into())));
        assert!(catalog.selected().is_none());
        assert!(!catalog.has_memberships());
        assert!(catalog.fence_identity(&Err(Error::Forbidden)));
        assert!(catalog.actor.is_none());
    }
    #[test]
    fn another_organisation_retires_the_catalogue_and_its_read_in_flight() {
        let mut catalog = Catalog::default();
        assert!(catalog.fence_scope(Some(1)));
        let entries = serde_json::from_str(r##"[{"id":"a","name":"A","channels":"#a"}]"##).unwrap();
        catalog.replace(Some("person".into()), Ok(entries));
        assert!(catalog.select(Some("a")));
        assert!(!catalog.fence_scope(Some(1)), "the same scope keeps it");
        // The same person in organisation B: nothing of A survives, even a
        // read for A that was still in flight or failed in transit.
        assert!(catalog.fence_scope(Some(2)));
        assert!(!catalog.has_memberships());
        assert!(catalog.selected().is_none());
        assert_eq!(catalog.actor.as_deref(), Some("person"));
        assert!(catalog.fetch.is_none());
        catalog.replace(Some("person".into()), Err(Error::Network));
        assert!(catalog.selected().is_none() && !catalog.has_memberships());
    }

    #[test]
    fn an_older_identity_check_cannot_reinstate_the_previous_account() {
        let mut catalogue = Catalog::default();
        let initial = std::time::Instant::now();
        let later = initial + std::time::Duration::from_secs(1);
        assert!(catalogue.record_identity(&(initial, Ok("first".into()))));
        assert!(catalogue.record_identity(&(later, Err(Error::Forbidden))));
        assert!(!catalogue.record_identity(&(initial, Ok("first".into()))));
        assert!(catalogue.actor.is_none());
    }
}
