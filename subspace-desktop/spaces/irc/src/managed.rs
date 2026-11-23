//! Networks the signed-in account brings: one for each IRC server its
//! fleets use, signed in with a short-lived login the Subspace API issues
//! at every connect, so nobody types IRC settings. They are derived state:
//! rebuilt when the account, its fleets or the saved networks change, never
//! written to `irc/config.json`, and gone on sign-out. A saved network on
//! the same server is kept instead, and Settings offers to switch.

mod account;
mod device;
mod plan;
mod source;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::{App, AppContext as _, Context};
use subspace_api_client::Client;
use subspace_api_client::initiatives::Catalog;
use subspace_client_irc::NetworkConfig;

pub(crate) use account::{Account, SignedIn};
pub(crate) use plan::{Server, same_server};

use crate::hub::Hub;

/// The account's side of the hub: who is signed in, this install's
/// device, and the networks last derived.
pub(crate) struct Managed {
    account: Account,
    /// The client the account was checked with, bound to that account.
    client: Option<Arc<Client>>,
    device: String,
    schedule: account::Schedule,
    networks: Vec<NetworkConfig>,
}

/// What Settings shows of the account's networks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccountNetworks {
    /// The signed-in username, while the account brings networks.
    pub username: Option<String>,
    /// Every server of the account's fleets a login can go to, including
    /// those a saved network connects to instead.
    pub servers: Vec<Server>,
}

impl Default for Managed {
    /// With a new device ID, kept for the run only, until the hub has the
    /// stored one.
    fn default() -> Self {
        Self::new(device::generate())
    }
}

impl Managed {
    pub(crate) fn new(device: String) -> Self {
        Self {
            account: Account::Unknown,
            client: None,
            device,
            schedule: account::Schedule::default(),
            networks: Vec::new(),
        }
    }

    /// The networks last derived.
    pub(crate) fn networks(&self) -> &[NetworkConfig] {
        &self.networks
    }

    /// Derives the account's networks again among `saved` ones, for the
    /// fleets in `initiatives`.
    pub(crate) fn rederive(
        &mut self,
        saved: &[NetworkConfig],
        initiatives: &[subspace_api_client::model::MemberInitiative],
    ) {
        self.networks = self.derive(saved, initiatives);
    }

    /// What Settings shows for the fleets in `initiatives`.
    pub(crate) fn shown(
        &self,
        initiatives: &[subspace_api_client::model::MemberInitiative],
    ) -> AccountNetworks {
        let Account::SignedIn(person) = &self.account else {
            return AccountNetworks::default();
        };
        let servers = plan::servers(initiatives);
        AccountNetworks {
            username: (!servers.is_empty()).then(|| person.username.clone()),
            servers,
        }
    }

    /// Sets the account as a check would, for tests: `client` stands for
    /// the API the account's networks ask for logins.
    #[cfg(test)]
    pub(crate) fn settle_for_test(&mut self, account: Account, client: Option<Arc<Client>>) {
        self.client = client;
        self.account = account;
        self.schedule.settle();
    }

    fn derive(
        &self,
        saved: &[NetworkConfig],
        initiatives: &[subspace_api_client::model::MemberInitiative],
    ) -> Vec<NetworkConfig> {
        let (Account::SignedIn(person), Some(client)) = (&self.account, &self.client) else {
            return Vec::new();
        };
        let login = source::source(Arc::clone(client), &person.account, &self.device);
        plan::networks(
            &plan::servers(initiatives),
            saved,
            person,
            (&login, &self.device),
        )
    }

    /// Whether the signed-in account is no longer the one checked, looked
    /// at no more than once a second.
    fn account_changed(&mut self, now: Instant) -> bool {
        self.schedule.look(now)
            && self
                .client
                .as_ref()
                .is_some_and(|client| client.account_changed())
    }

    /// Forgets the account after it changed, so the next tick checks again.
    fn forget(&mut self) {
        self.account = Account::Unknown;
        self.client = None;
        self.schedule.restart();
    }

    /// Takes a finished check: the account it showed, or a later retry.
    fn finish(&mut self, checked: account::Checked, now: Instant) -> bool {
        if account::stale(&checked) {
            // Another account signed in or out meanwhile: check again.
            self.schedule.running = false;
            return false;
        }
        match account::outcome(checked.result) {
            Some(account) => {
                self.schedule.settle();
                self.client = checked.client;
                let changed = self.account != account;
                self.account = account;
                changed
            }
            None => {
                self.schedule.failed(now);
                false
            }
        }
    }
}

/// This install's device ID, made and stored on first use.
pub(crate) fn device(cx: &mut App) -> String {
    device::load(cx)
}

/// On the hub's timer: forget the account once another signs in or out,
/// and check it when due.
pub(crate) fn tick(hub: &mut Hub, cx: &mut Context<Hub>) {
    let now = Instant::now();
    if hub.managed.account_changed(now) {
        hub.managed.forget();
        hub.reconcile(cx);
    }
    if hub.managed.schedule.due(now) {
        begin(hub, cx);
    }
}

/// Checks the account on a background task.
fn begin(hub: &mut Hub, cx: &mut Context<Hub>) {
    hub.managed.schedule.running = true;
    cx.spawn(async move |hub, cx| {
        let checked = cx.background_spawn(async move { account::check() }).await;
        let _ = hub.update(cx, |hub, cx| finish(hub, checked, cx));
    })
    .detach();
}

/// Takes a check's answer. Once signed in, the fleets are read if they are
/// not known yet, and the networks follow when either arrives.
fn finish(hub: &mut Hub, checked: account::Checked, cx: &mut Context<Hub>) {
    if !hub.managed.finish(checked, Instant::now()) {
        return;
    }
    let catalog = hub.initiatives.catalog.clone();
    if matches!(hub.managed.account, Account::SignedIn(_)) && !catalog.read(cx).memberships_known()
    {
        catalog.update(cx, Catalog::refresh);
    }
    hub.reconcile(cx);
    cx.notify();
}
