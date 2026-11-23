//! The app-wide fleet service, one per process ([`FleetApi::shared`]): the
//! fleets the person's initiatives belong to, with the address the
//! Subspace API records for each, the token saved for it on this computer,
//! a ready client for one-shot reads and at most one event stream per
//! fleet, however many Spaces follow it.
//!
//! ```ignore
//! let fleets = FleetApi::shared(cx);
//! // Keep the handle while the view shows the fleet; dropping it lets the
//! // stream close when nothing else follows the fleet.
//! self.follow = Some(fleets.update(cx, |fleets, cx| fleets.follow("aurora", cx)));
//! cx.observe(&fleets, |this, fleets, cx| {
//!     let fleets = fleets.read(cx);
//!     this.state = fleets.state("aurora");
//!     this.snapshot = fleets.snapshot("aurora");
//!     cx.notify();
//! })
//! .detach();
//! ```

mod follow;
mod queue;
mod tokens;

pub use follow::FleetFollow;
pub use tokens::TokenError;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use subspace_api_client::initiatives::Catalog;

use crate::directory::{self, FleetRecord, FleetState, SharedFleet, TokenLoad};
use crate::feed::FleetSnapshot;
use crate::tokens::TokenStore;
use crate::{FleetAddress, FleetClient};
use follow::Followed;

struct Session(Entity<FleetApi>);
impl Global for Session {}

/// The least time between two reads of one fleet's token after the secret
/// store could not read it, whoever asks: a locked macOS keychain may ask
/// the person to unlock it on every read.
const REREAD_INTERVAL: Duration = Duration::from_secs(5);

/// A fleet the person's initiatives belong to.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FleetInfo {
    /// The fleet as the Subspace API records it: ID, name and the address
    /// as recorded, usable or not.
    pub record: FleetRecord,
    /// The recorded address, checked, when it is usable.
    pub address: Option<FleetAddress>,
    pub state: FleetState,
}

/// What the service keeps of its token reads.
#[derive(Default)]
struct Reads {
    /// When each fleet's unreadable token was last read again.
    last: BTreeMap<String, Instant>,
    /// Each fleet a Retry asked about too soon, with its generation then:
    /// it is read once its interval ends, unless it changed meanwhile.
    deferred: BTreeMap<String, u64>,
    /// The fleets with a re-read queued and not landed: never two at once.
    rereading: std::collections::BTreeSet<String>,
    /// Each fleet's last store operation queued ([`queue`]): the fleet's
    /// next one runs after it. One queue per fleet, so a keychain call
    /// that waits on one fleet never holds up another.
    tails: BTreeMap<String, Task<()>>,
    /// Each fleet's token generation: a save, a clear or the fleet's
    /// removal starts a new one, and a read or save that began in an older
    /// one is dropped when it lands, so a cleared or replaced token never
    /// comes back.
    generations: BTreeMap<String, u64>,
    /// The last generation handed out, across every fleet.
    generation: u64,
}

/// The service. Reach it through [`FleetApi::shared`].
pub struct FleetApi {
    store: Arc<TokenStore>,
    fleets: Vec<FleetRecord>,
    tokens: BTreeMap<String, TokenLoad>,
    /// The token reads: when each was last read again and which are
    /// current.
    reads: Reads,
    /// Each fleet a handle follows, with the holder every handle clones.
    interest: BTreeMap<String, Arc<()>>,
    feeds: BTreeMap<String, Followed>,
    revision: u64,
    directory_revision: u64,
    /// Whether the timer that checks followed fleets runs.
    ticking: bool,
    _catalog: Option<Subscription>,
}

impl FleetApi {
    /// The process's service, created on first use, following the
    /// initiatives' fleets and the app's token store.
    pub fn shared(cx: &mut App) -> Entity<Self> {
        if let Some(session) = cx.try_global::<Session>() {
            return session.0.clone();
        }
        let catalog = Catalog::shared(cx);
        load_initiatives(&catalog, cx);
        let service = cx.new(|cx| {
            let mut service = Self::new(Arc::new(TokenStore::system()));
            service._catalog = Some(cx.observe(&catalog, |this: &mut Self, catalog, cx| {
                this.set_fleets(directory::fleets_of(catalog.read(cx).memberships()), cx);
            }));
            service.set_fleets(directory::fleets_of(catalog.read(cx).memberships()), cx);
            service
        });
        cx.set_global(Session(service.clone()));
        service
    }

    /// The process's service when something has created it, without
    /// creating one, so a caller that must never open the token store, such
    /// as a Space under test, can still read fleets a test set up.
    pub fn current(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<Session>().map(|session| session.0.clone())
    }

    /// Makes `service` the process's service in place of the one
    /// [`Self::shared`] would create, as a test does with one over
    /// [`TokenStore::memory`]. Only for tests: the `test-support` feature,
    /// which only dev-dependencies enable.
    #[cfg(any(test, feature = "test-support"))]
    pub fn install(service: Entity<Self>, cx: &mut App) {
        cx.set_global(Session(service));
    }

    /// A new service over `store` with `fleets`, installed as the
    /// process's ([`Self::install`]). Only for tests (`test-support`).
    // qual:api
    #[cfg(any(test, feature = "test-support"))]
    pub fn install_new(store: TokenStore, fleets: Vec<FleetRecord>, cx: &mut App) -> Entity<Self> {
        let service = cx.new(|cx| {
            let mut service = Self::new(Arc::new(store));
            service.set_fleets(fleets, cx);
            service
        });
        Self::install(service.clone(), cx);
        service
    }

    /// A service over `store` with no fleets until [`Self::set_fleets`].
    pub fn new(store: Arc<TokenStore>) -> Self {
        Self {
            store,
            fleets: Vec::new(),
            tokens: BTreeMap::new(),
            reads: Reads::default(),
            interest: BTreeMap::new(),
            feeds: BTreeMap::new(),
            revision: 0,
            directory_revision: 0,
            ticking: false,
            _catalog: None,
        }
    }

    /// Replaces the fleets, as the initiatives give them, and reads the
    /// saved token of each new one.
    pub fn set_fleets(&mut self, fleets: Vec<FleetRecord>, cx: &mut Context<Self>) {
        if fleets == self.fleets {
            return;
        }
        for gone in directory::removed(&self.fleets, &fleets) {
            self.supersede(&gone);
        }
        let new = directory::retain_tokens(&mut self.tokens, &fleets);
        self.fleets = fleets;
        for fleet in new {
            self.load_token(fleet, cx);
        }
        self.changed(cx);
    }

    /// Changes whenever anything this service reports changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Changes when a fleet, its recorded address or its saved token
    /// changes, but not with a followed fleet's news.
    pub fn directory_revision(&self) -> u64 {
        self.directory_revision
    }

    /// Every fleet, in the initiatives' order.
    pub fn fleets(&self) -> Vec<FleetInfo> {
        self.fleets.iter().map(|record| self.info(record)).collect()
    }

    pub fn fleet(&self, fleet: &str) -> Option<FleetInfo> {
        self.record(fleet).map(|record| self.info(record))
    }

    /// The fleet's state; [`FleetState::NotConfigured`] for a fleet that
    /// is not one of the initiatives'.
    pub fn state(&self, fleet: &str) -> FleetState {
        self.fleet(fleet)
            .map_or(FleetState::NotConfigured, |info| info.state)
    }

    /// A ready client for one-shot reads, with the token saved for the
    /// fleet's recorded address, or the state that explains why there is
    /// none. Run its calls in the background.
    pub fn client(&self, fleet: &str) -> Result<FleetClient, FleetState> {
        let record = self.record(fleet).ok_or(FleetState::NotConfigured)?;
        directory::ready_client(record, self.token_load(fleet))
    }

    /// What the fleet's stream has delivered, while something follows it.
    pub fn snapshot(&self, fleet: &str) -> Option<FleetSnapshot> {
        self.feeds
            .get(fleet)
            .map(|followed| followed.feed.snapshot())
    }

    /// The fleets the built-in Fleet Console may use, each with its token
    /// only for the address it was saved for.
    pub fn shared_fleets(&self) -> Vec<SharedFleet> {
        self.fleets
            .iter()
            .map(|record| directory::shared_fleet(record, self.token_load(&record.id)))
            .collect()
    }

    fn record(&self, fleet: &str) -> Option<&FleetRecord> {
        self.fleets.iter().find(|record| record.id == fleet)
    }

    fn token_load(&self, fleet: &str) -> &TokenLoad {
        static LOADING: TokenLoad = TokenLoad::Loading;
        self.tokens.get(fleet).unwrap_or(&LOADING)
    }

    fn info(&self, record: &FleetRecord) -> FleetInfo {
        let link = self
            .feeds
            .get(&record.id)
            .map(|followed| followed.feed.link());
        FleetInfo {
            address: directory::address_of(record).ok(),
            state: directory::state_of(record, self.token_load(&record.id), link),
            record: record.clone(),
        }
    }

    /// Reads again every saved token the secret store could not read, as
    /// when the person has unlocked the keychain and chosen Retry. Each
    /// fleet stays **Keychain locked** until a read finds otherwise, and is
    /// read again at most once every few seconds, however often this is
    /// called: one asked sooner is read once that interval ends. Call it
    /// only when the person asks.
    pub fn reload_unreadable(&mut self, cx: &mut Context<Self>) {
        self.reload_unreadable_at(Instant::now(), cx);
    }

    pub(crate) fn reload_unreadable_at(&mut self, now: Instant, cx: &mut Context<Self>) {
        let waiting = directory::waiting(&self.tokens, &self.reads.last, now, REREAD_INTERVAL);
        self.read_due(now, cx);
        for (fleet, at) in waiting {
            self.defer_reread(fleet, now, at, cx);
        }
    }

    /// Reads again each unreadable token not read within the interval.
    fn read_due(&mut self, now: Instant, cx: &mut Context<Self>) {
        directory::due_rereads(&self.tokens, &mut self.reads.last, now, REREAD_INTERVAL)
            .into_iter()
            .for_each(|fleet| self.reread(fleet, cx));
    }

    /// Queues a re-read of `fleet`'s token unless one is queued and not
    /// landed, so one click never reads the keychain twice in a row.
    fn reread(&mut self, fleet: String, cx: &mut Context<Self>) {
        if self.reads.rereading.insert(fleet.clone()) {
            self.load_token(fleet, cx);
        }
    }

    /// A Retry asked within `fleet`'s interval: reads it once at `at`, when
    /// the interval ends, rather than doing nothing. One waiting read per
    /// fleet however often the person asks meanwhile; none once the fleet
    /// was saved, cleared or removed since; and it never waits again, so a
    /// keychain that stays locked is not read on a timer.
    fn defer_reread(&mut self, fleet: String, now: Instant, at: Instant, cx: &mut Context<Self>) {
        if self.reads.deferred.contains_key(&fleet) {
            return;
        }
        let generation = self.generation_of(&fleet);
        self.reads.deferred.insert(fleet.clone(), generation);
        let wait = at.saturating_duration_since(now);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            _ = this.update(cx, |this, cx| this.deferred_due(&fleet, at, cx));
        })
        .detach();
    }

    /// `fleet`'s waiting Retry, at `at`: read when nothing changed it.
    fn deferred_due(&mut self, fleet: &str, at: Instant, cx: &mut Context<Self>) {
        let asked = self.reads.deferred.remove(fleet);
        let unchanged = asked == Some(self.generation_of(fleet));
        // A Retry may have read it since: the deadline is checked again.
        let due = self
            .reads
            .last
            .get(fleet)
            .is_none_or(|last| at.saturating_duration_since(*last) >= REREAD_INTERVAL);
        if unchanged && due && self.tokens.get(fleet) == Some(&TokenLoad::Unavailable) {
            self.reads.last.insert(fleet.to_owned(), at);
            self.reread(fleet.to_owned(), cx);
        }
    }

    /// Queues a read of `fleet`'s token ([`queue`]).
    fn load_token(&mut self, fleet: String, cx: &mut Context<Self>) {
        self.enqueue(&fleet, queue::Op::Read, cx).detach();
    }

    /// The fleet's current token generation.
    fn generation_of(&self, fleet: &str) -> u64 {
        self.reads
            .generations
            .get(fleet)
            .copied()
            .unwrap_or_default()
    }

    /// Starts a new token generation for `fleet`, so every read or save
    /// still running lands in an older one and is dropped.
    fn supersede(&mut self, fleet: &str) -> u64 {
        let reads = &mut self.reads;
        reads.generation = reads.generation.wrapping_add(1);
        reads.generations.insert(fleet.to_owned(), reads.generation);
        reads.generation
    }

    /// Keeps where `fleet`'s token stands, announcing it only when it
    /// differs from what the service had.
    fn set_token(&mut self, fleet: &str, token: TokenLoad, cx: &mut Context<Self>) {
        if self.record(fleet).is_none() || self.tokens.get(fleet) == Some(&token) {
            return;
        }
        self.tokens.insert(fleet.to_owned(), token);
        self.changed(cx);
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        self.revision = self.revision.wrapping_add(1);
        self.directory_revision = self.directory_revision.wrapping_add(1);
        self.reconcile(cx);
        cx.notify();
    }
}

/// Asks the catalogue for the person's initiatives, and so their fleets,
/// unless it has them. Fleet Console can be the first Space shown and ask for
/// the fleets before any other Space has loaded the initiatives.
pub(crate) fn load_initiatives(catalog: &Entity<Catalog>, cx: &mut App) {
    catalog.update(cx, |catalog, cx| {
        if !catalog.has_memberships() {
            catalog.refresh(cx);
        }
    });
}

#[cfg(test)]
mod tests;
