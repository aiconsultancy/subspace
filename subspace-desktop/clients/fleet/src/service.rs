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
mod tokens;

pub use follow::FleetFollow;
pub use tokens::TokenError;

use std::collections::BTreeMap;
use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription};
use subspace_api_client::initiatives::Catalog;

use crate::directory::{self, FleetRecord, FleetState, SharedFleet, TokenLoad};
use crate::feed::FleetSnapshot;
use crate::tokens::TokenStore;
use crate::{FleetAddress, FleetClient};
use follow::Followed;

struct Session(Entity<FleetApi>);
impl Global for Session {}

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

/// The service. Reach it through [`FleetApi::shared`].
pub struct FleetApi {
    store: Arc<TokenStore>,
    fleets: Vec<FleetRecord>,
    tokens: BTreeMap<String, TokenLoad>,
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

    /// A service over `store` with no fleets until [`Self::set_fleets`].
    pub fn new(store: Arc<TokenStore>) -> Self {
        Self {
            store,
            fleets: Vec::new(),
            tokens: BTreeMap::new(),
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

    fn load_token(&self, fleet: String, cx: &mut Context<Self>) {
        let store = Arc::clone(&self.store);
        let id = fleet.clone();
        let load = cx.background_spawn(async move { TokenLoad::read(&store, &id) });
        cx.spawn(async move |this, cx| {
            let token = load.await;
            _ = this.update(cx, |this, cx| this.set_token(&fleet, token, cx));
        })
        .detach();
    }

    fn set_token(&mut self, fleet: &str, token: TokenLoad, cx: &mut Context<Self>) {
        if self.record(fleet).is_none() {
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
