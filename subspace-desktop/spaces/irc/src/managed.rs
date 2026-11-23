//! Networks the signed-in account brings: one for each IRC server its
//! fleets use that the Subspace API vouches for, each signed in with a
//! short-lived login issued for that server alone, so nobody types IRC
//! settings. They are derived state: rebuilt when the account, its fleets
//! or the saved networks change, never written to `irc/config.json`, and
//! gone on sign-out. A saved network on the same server is kept instead,
//! and Settings offers to switch.

mod account;
mod device;
mod env;
mod ids;
mod login;
mod plan;
mod servers;
mod source;
#[cfg(test)]
mod tests;

use std::sync::{Arc, RwLock};
use std::time::Instant;

use gpui_kit::{App, AppContext as _, Context};
use subspace_api_client::initiatives::Catalog;
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::NetworkConfig;
use subspace_framework::dashboard::full_text::Document;

pub(crate) use account::Account;
pub(crate) use ids::reserved;
pub(crate) use plan::{Server, same_server};

use crate::hub::Hub;
use crate::initiatives::Endpoint;
use env::Env;
use source::Binding;

/// The account's side of the hub: who is signed in, this install's
/// device, the account's servers and the networks last derived.
pub(crate) struct Managed {
    env: Arc<dyn Env>,
    account: Account,
    /// The verified account the networks sign in as, while signed in.
    binding: Option<Arc<Binding>>,
    /// The account generation the last check began in, once one ran.
    checked: Option<Option<u64>>,
    device: String,
    schedule: account::Schedule,
    servers: servers::Servers,
    /// Changes when the account is forgotten, so late answers are dropped.
    epoch: u64,
    /// When to read the memberships again while they are not known for
    /// the signed-in account.
    catalogue: account::Retry,
    /// The person whose account networks' stored lines search shows: the
    /// verified account's, `None` while nobody is verified.
    searchable: Arc<RwLock<Option<String>>>,
    networks: Vec<NetworkConfig>,
}

/// What Settings shows of the account's networks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AccountNetworks {
    /// The account name the API issued logins for, while the account
    /// brings networks.
    pub account: Option<String>,
    /// Every server the API vouches for, including those a saved network
    /// connects to instead.
    pub servers: Vec<Server>,
    /// The verified person's API user ID, while one is signed in.
    pub person: Option<String>,
    /// Whether the memberships were read for that person, so the servers
    /// are theirs.
    pub confirmed: bool,
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
            env: Arc::new(env::Native),
            account: Account::Unknown,
            binding: None,
            checked: None,
            device,
            schedule: account::Schedule::default(),
            servers: servers::Servers::default(),
            epoch: 0,
            catalogue: account::Retry::default(),
            searchable: Arc::default(),
            networks: Vec::new(),
        }
    }

    /// The rule search asks of each stored line: hidden when it belongs
    /// to an account network of anyone but the verified person.
    pub(crate) fn search_rule(&self) -> impl Fn(&Document) -> bool + Send + Sync + 'static {
        let current = Arc::clone(&self.searchable);
        move |document| {
            current
                .read()
                .map(|person| ids::hidden(document, person.as_deref()))
                .unwrap_or(true)
        }
    }

    /// Tells search whose account networks it shows now.
    fn publish_searchable(&self) {
        let person = self
            .binding
            .as_ref()
            .map(|binding| ids::digest(&binding.person_id));
        if let Ok(mut searchable) = self.searchable.write() {
            *searchable = person;
        }
    }

    /// The networks last derived.
    pub(crate) fn networks(&self) -> &[NetworkConfig] {
        &self.networks
    }

    /// Follows the fleets in `initiatives`, read for the account `owner`
    /// names, and derives the account's networks again among `saved` ones.
    /// Memberships read for any other account plan nothing.
    pub(crate) fn rederive(
        &mut self,
        saved: &[NetworkConfig],
        (initiatives, owner): (&[MemberInitiative], Option<(Option<u64>, &str)>),
    ) {
        let Some(binding) = self.binding.clone().filter(|binding| binding.owns(owner)) else {
            if self.binding.is_none() {
                self.servers.clear();
            }
            self.networks.clear();
            return;
        };
        self.servers.follow(plan::servers(initiatives));
        let vouched = self
            .servers
            .vouched()
            .map(|(entry, account)| plan::Vouched {
                server: &entry.server,
                account,
                source: source::source(&binding, &entry.login, &self.device),
            })
            .collect();
        self.networks = plan::networks(vouched, saved, (&binding.person_id, &self.device));
    }

    /// What Settings shows.
    /// What Settings shows, while the memberships read for `owner` are the
    /// verified person's.
    pub(crate) fn shown(&self, owner: Option<(Option<u64>, &str)>) -> AccountNetworks {
        let confirmed = self
            .binding
            .as_ref()
            .is_some_and(|binding| binding.owns(owner));
        let vouched = || self.servers.vouched().filter(|_| confirmed);
        AccountNetworks {
            account: vouched().map(|(_, account)| account.to_owned()).next(),
            servers: vouched().map(|(entry, _)| entry.server.clone()).collect(),
            person: self
                .binding
                .as_ref()
                .map(|binding| binding.person_id.clone()),
            confirmed,
        }
    }

    /// Whether the signed-in account is no longer the one checked, looked
    /// at no more than once a second.
    fn account_changed(&mut self, now: Instant) -> bool {
        self.schedule.look(now)
            && self
                .checked
                .is_some_and(|generation| generation != self.env.generation())
    }

    /// Forgets the account after it changed, so the next tick checks again
    /// and nothing obtained for it is offered any more.
    fn forget(&mut self) {
        self.account = Account::Unknown;
        self.binding = None;
        self.checked = None;
        self.servers.clear();
        self.epoch = self.epoch.wrapping_add(1);
        self.schedule.restart();
        self.publish_searchable();
    }

    /// Takes a finished check: the account it showed, or a later retry.
    /// True when the account changed.
    fn finish(&mut self, checked: account::Checked, now: Instant) -> bool {
        if checked.generation != self.env.generation() {
            // Another account signed in or out meanwhile: check again.
            self.schedule.running = false;
            return false;
        }
        self.checked = Some(checked.generation);
        match account::outcome(checked.result) {
            account::Outcome::Settled(account) => {
                self.schedule.settle();
                self.binding = match (&account, checked.client) {
                    (Account::SignedIn(person), Some(client)) => Some(Arc::new(Binding {
                        env: Arc::clone(&self.env),
                        client,
                        generation: checked.generation,
                        person_id: person.id.clone(),
                    })),
                    _ => None,
                };
                let changed = self.account != account;
                self.account = account;
                self.publish_searchable();
                changed
            }
            account::Outcome::Retry { cap } => {
                self.schedule.failed(now, cap);
                false
            }
        }
    }

    /// Compares the signed-in account again at the next tick, for tests.
    #[cfg(test)]
    pub(crate) fn look_again_for_test(&mut self) {
        self.schedule.look_again();
    }

    /// Membership reads asked for since they were last known for the
    /// account, for tests.
    #[cfg(test)]
    pub(crate) fn catalogue_reads_for_test(&self) -> u32 {
        self.catalogue.reads()
    }

    /// Uses `env` in place of the app's account and API, for tests.
    #[cfg(test)]
    pub(crate) fn use_env_for_test(&mut self, env: Arc<dyn Env>) {
        self.env = env;
    }
}

/// This install's device ID, made and stored on first use.
pub(crate) fn device(cx: &mut App) -> String {
    device::load(cx)
}

/// On the hub's timer: forget the account once another signs in or out,
/// check it when due, and ask about each server when its probe is due.
pub(crate) fn tick(hub: &mut Hub, cx: &mut Context<Hub>) {
    let now = Instant::now();
    if hub.managed.account_changed(now) {
        hub.managed.forget();
        hub.reconcile(cx);
    }
    if hub.managed.schedule.due(now) {
        begin(hub, cx);
    }
    reread(hub, now, cx);
    probe(hub, now, cx);
}

/// While signed in and the memberships are not known for this account,
/// as after a failed or refused read, reads them again with backoff.
fn reread(hub: &mut Hub, now: Instant, cx: &mut Context<Hub>) {
    let catalog = hub.initiatives.catalog.clone();
    let owned = hub
        .managed
        .binding
        .as_ref()
        .map(|binding| binding.owns(catalog.read(cx).verified_for()));
    if hub.managed.catalogue.due(owned, now) {
        catalog.update(cx, Catalog::refresh);
    }
}

/// Checks the account on a background task.
fn begin(hub: &mut Hub, cx: &mut Context<Hub>) {
    hub.managed.schedule.running = true;
    let env = Arc::clone(&hub.managed.env);
    cx.spawn(async move |hub, cx| {
        let checked = cx
            .background_spawn(async move { account::check(env.as_ref()) })
            .await;
        let _ = hub.update(cx, |hub, cx| finish(hub, checked, cx));
    })
    .detach();
}

/// Takes a check's answer. Once signed in, the fleets are read if they are
/// not known yet, and the networks follow when either arrives.
fn finish(hub: &mut Hub, checked: account::Checked, cx: &mut Context<Hub>) {
    let now = Instant::now();
    if !hub.managed.finish(checked, now) {
        return;
    }
    reread(hub, now, cx);
    hub.reconcile(cx);
    cx.notify();
}

/// Asks, on background tasks, for the login of each server whose probe is
/// due: whether the API vouches for it, and the login its connection uses.
fn probe(hub: &mut Hub, now: Instant, cx: &mut Context<Hub>) {
    let Some(binding) = hub.managed.binding.clone() else {
        return;
    };
    let epoch = hub.managed.epoch;
    for (endpoint, login) in hub.managed.servers.due(now) {
        let client = Arc::clone(&binding.client);
        cx.spawn(async move |hub, cx| {
            let answer = cx
                .background_spawn(async move { login.login(&client).map(|login| login.account) })
                .await;
            let _ = hub.update(cx, |hub, cx| answered(hub, (epoch, &endpoint), answer, cx));
        })
        .detach();
    }
}

/// Takes a probe's answer for `endpoint`, unless the account changed since.
fn answered(
    hub: &mut Hub,
    (epoch, endpoint): (u64, &Endpoint),
    answer: Result<String, login::Unavailable>,
    cx: &mut Context<Hub>,
) {
    let current = hub.managed.epoch == epoch;
    if current && hub.managed.servers.answer(endpoint, answer, Instant::now()) {
        hub.reconcile(cx);
        cx.notify();
    }
}
