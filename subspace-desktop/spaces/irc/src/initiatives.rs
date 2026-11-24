//! Applies the desktop initiative selection to the verified IRC runtime.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::{Context, Entity, Subscription};
use subspace_api_client::initiatives::{Catalog, IdentityObservation, not_a_member};
use subspace_api_client::model::{Fleet, MemberInitiative};
use subspace_client_irc::{BufferKey, Command, NetworkConfig, NetworkId, Target};

use crate::Irc;
use crate::fleet::Roster;
use crate::hub::Hub;

mod channels;
mod endpoint;
mod picker;
pub(crate) use channels::{channel_names, focus_channel, initiative_channels};
pub(crate) use endpoint::{Endpoint, comparable_host};
pub(crate) use picker::{Picker, button, command, dismiss_stale_picker, hint};

/// The longest the networks wait at start for the first read of the
/// memberships: long enough for an API that is waking up, as Issues allows.
pub(crate) const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

pub(crate) struct InitiativeIrc {
    pub(crate) catalog: Entity<Catalog>,
    selection: Option<MemberInitiative>,
    /// The network on which an initiative replaces local autojoin.
    network: Option<NetworkId>,
    /// Whether the person switched to the selected initiative and its first
    /// channel is still to be shown, once a network carries its channels.
    focus_pending: bool,
    /// The catalogue's count of the person's own picks when last followed.
    picks: u64,
    /// The user the selection was followed for.
    actor: Option<String>,
    /// Whether Channels is closed: the API serves initiative channels and
    /// found that the person belongs to none, so no network runs until an
    /// administrator adds them to one.
    closed: bool,
    /// Whether the networks wait for the first read of the memberships to
    /// settle.
    awaiting_read: bool,
    /// The server initiatives use when their fleet names none.
    default: Endpoint,
    loopback_fixture: bool,
    /// Each network's fleet roles, built once for the catalogue and the
    /// networks as they are, and forgotten when either changes.
    rosters: RefCell<HashMap<NetworkId, Rc<Roster>>>,
    _subscription: Option<Subscription>,
}

impl InitiativeIrc {
    pub(crate) fn new(catalog: Entity<Catalog>, loopback_fixture: bool) -> Self {
        Self {
            catalog,
            selection: None,
            network: None,
            focus_pending: false,
            picks: 0,
            actor: None,
            closed: false,
            awaiting_read: false,
            default: Endpoint::configured(),
            loopback_fixture,
            rosters: RefCell::default(),
            _subscription: None,
        }
    }

    /// The selected initiative's server: its fleet's, or the default. `None`
    /// while its assigned fleet's server is unknown.
    fn endpoint(&self) -> Option<Endpoint> {
        Endpoint::for_initiative(self.selection.as_ref(), &self.default)
    }

    /// The one configured network on the selected initiative's server.
    /// Switching to an initiative of another fleet switches network; the
    /// same server keeps the current one.
    /// While the assigned fleet's server is unknown there is none, so the
    /// initiative's autojoin stays off until the fleet lookup succeeds.
    pub(crate) fn network(&self, networks: &[NetworkConfig]) -> Option<NetworkId> {
        let endpoint = self.endpoint()?;
        match eligible(networks, &endpoint, self.loopback_fixture)[..] {
            [network] => Some(network.id.clone()),
            _ => None,
        }
    }

    /// Whether any of `initiatives` (or the default server) has exactly one
    /// configured network, so the picker can offer a working choice.
    pub(crate) fn serves_any(
        &self,
        initiatives: &[MemberInitiative],
        networks: &[NetworkConfig],
    ) -> bool {
        let unique =
            |endpoint: &Endpoint| eligible(networks, endpoint, self.loopback_fixture).len() == 1;
        unique(&self.default)
            || initiatives
                .iter()
                .filter_map(|item| Endpoint::for_initiative(Some(item), &self.default))
                .any(|endpoint| unique(&endpoint))
    }

    /// Names the server to add when no configured network serves the
    /// selected initiative.
    pub(crate) fn missing_network(&self, networks: &[NetworkConfig]) -> Option<String> {
        self.network(networks).is_none().then(|| {
            let fleet = self.selection.as_ref().and_then(|item| item.fleet.as_ref());
            let Some(endpoint) = self.endpoint() else {
                return unresolved_fleet(fleet);
            };
            match fleet {
                Some(fleet) if !fleet.name.is_empty() => {
                    format!(
                        "Add a connection to {} for fleet {}",
                        endpoint.label(),
                        fleet.name
                    )
                }
                _ => format!("Add one connection to {} for initiatives", endpoint.label()),
            }
        })
    }

    /// The selected initiative's fleet id: `None` with no initiative
    /// selected, `Some(None)` for one without a fleet.
    pub(crate) fn selected_fleet(&self) -> Option<Option<&str>> {
        self.selection
            .as_ref()
            .map(|initiative| initiative.fleet_id.as_deref())
    }

    #[cfg(test)]
    pub(crate) fn selected_for_runtime(&self) -> bool {
        self.network.is_some()
    }

    /// Whether Channels is closed until the person belongs to an initiative.
    /// Only with an API that serves initiative channels, and only once their
    /// memberships were read: never while they are read, after a read
    /// failed, or in a build without an API, where the configured networks
    /// run as before.
    pub(crate) fn closed(&self) -> bool {
        self.closed
    }

    /// Whether the networks wait for the first read of the memberships.
    pub(crate) fn awaiting_read(&self) -> bool {
        self.awaiting_read
    }

    /// Follows the catalogue's word on whether Channels is closed; true when
    /// that changed. While another account's or organisation's memberships
    /// are read it stays as it is, so nobody connects only to stop again.
    fn sync_closed(&mut self, cx: &gpui_kit::App) -> bool {
        let catalog = self.catalog.read(cx);
        let closed = match unsettled(catalog) {
            true => self.closed,
            false => not_a_member(subspace_api_client::capabilities(), catalog),
        };
        std::mem::replace(&mut self.closed, closed) != closed
    }

    /// Ends the wait for the first read once it settled; true when it ended.
    /// A read refused for want of a session does not settle it: at start a
    /// saved session may still be restored, and a person who is signed out
    /// is asked to sign in first.
    fn settle_first_read(&mut self, cx: &gpui_kit::App) -> bool {
        let catalog = self.catalog.read(cx);
        let settled = self.awaiting_read && !catalog.reading() && !catalog.sign_in_needed();
        if settled {
            self.awaiting_read = false;
        }
        settled
    }

    pub(crate) fn serves_route(
        &self,
        network: &NetworkConfig,
        endpoint: Option<&Endpoint>,
    ) -> bool {
        endpoint.unwrap_or(&self.default).matches(network)
    }

    /// The fleet roles on `network`: those of the catalogue's fleets whose
    /// IRC server it is, as an initiative of that fleet would pick it among
    /// `networks`. None for a network that serves no fleet.
    /// Built once until the catalogue or the networks change.
    pub(crate) fn roster(
        &self,
        networks: &[NetworkConfig],
        network: &NetworkId,
        cx: &gpui_kit::App,
    ) -> Rc<Roster> {
        if let Some(held) = self.rosters.borrow().get(network) {
            return Rc::clone(held);
        }
        let catalog = self.catalog.read(cx);
        let roster = Rc::new(Roster::for_catalogue(
            &initiatives_on(
                catalog.memberships(),
                networks,
                network,
                self.loopback_fixture,
            ),
            catalog.memberships_known(),
        ));
        self.rosters
            .borrow_mut()
            .insert(network.clone(), Rc::clone(&roster));
        roster
    }

    /// Forgets the built rosters, after the catalogue or the networks
    /// changed.
    pub(crate) fn forget_rosters(&self) {
        self.rosters.borrow_mut().clear();
    }
}

/// Whether a hub holds its networks for the first read of the memberships:
/// a `running` one, where the API `may_serve` initiative channels, before
/// they are `known`.
pub(crate) fn holds_for_first_read(running: bool, may_serve: bool, known: bool) -> bool {
    running && may_serve && !known
}

/// Whether `catalog` is reading memberships it knows nothing of yet, as for
/// a new account or organisation.
fn unsettled(catalog: &Catalog) -> bool {
    catalog.reading() && !catalog.memberships_known() && !catalog.no_memberships()
}

/// The initiatives with a fleet whose server is `network` among `networks`.
fn initiatives_on<'a>(
    initiatives: &'a [MemberInitiative],
    networks: &[NetworkConfig],
    network: &NetworkId,
    loopback_fixture: bool,
) -> Vec<&'a MemberInitiative> {
    initiatives
        .iter()
        .filter(|initiative| {
            Endpoint::from_fleet(initiative).is_some_and(|endpoint| {
                eligible(networks, &endpoint, loopback_fixture)
                    .iter()
                    .any(|eligible| eligible.id == *network)
            })
        })
        .filter(|initiative| initiative.fleet.is_some())
        .collect()
}

pub(crate) fn loopback_fixture() -> bool {
    let api = std::env::var("SUBSPACE_API_URL").ok();
    std::env::var_os("SUBSPACE_CAPTURE_DIR").is_some()
        && subspace_api_client::fixture_from_env(cfg!(debug_assertions), api.as_deref()).is_some()
}

impl Hub {
    pub(crate) fn follow_initiatives(&mut self, cx: &mut Context<Self>) {
        self.initiatives.sync_selection(cx);
        self.initiatives.sync_closed(cx);
        self.await_first_read(self.is_running(), Catalog::refresh, cx);
        self.initiatives._subscription =
            Some(cx.observe(&self.initiatives.catalog, |hub, _, cx| {
                hub.initiatives.forget_rosters();
                let selection_changed = hub.initiatives.sync_selection(cx);
                let reopened_or_closed = hub.initiatives.sync_closed(cx);
                let first_read = hub.initiatives.settle_first_read(cx);
                // The account's networks follow its fleets.
                hub.reconcile(cx);
                if reopened_or_closed || first_read {
                    hub.rerun();
                }
                // Any change is applied, so a selection that went with its
                // scope also takes its autojoin with it. Without initiative
                // channels (a NETT organisation that does not serve them),
                // nothing new switches network or autojoins.
                if selection_changed {
                    hub.apply_initiative();
                }
                cx.notify();
            }));
    }

    /// For a `running` hub whose API may serve initiative channels, before
    /// the memberships are known: reads them with `read` and holds the
    /// networks until that read settles, or [`READ_TIMEOUT`] passes, so a
    /// member's configured channels are not joined only to be left for
    /// their initiative's, and someone in no initiative never connects.
    /// NETT's capability list may not be read yet; the read decides.
    pub(crate) fn await_first_read(
        &mut self,
        running: bool,
        read: impl FnOnce(&mut Catalog, &mut Context<Catalog>),
        cx: &mut Context<Self>,
    ) {
        let catalog = self.initiatives.catalog.clone();
        let known = {
            let catalog = catalog.read(cx);
            catalog.memberships_known() || catalog.no_memberships()
        };
        if !holds_for_first_read(
            running,
            subspace_api_client::may_serve_initiative_channels(),
            known,
        ) {
            return;
        }
        catalog.update(cx, read);
        self.hold_for_first_read(cx);
    }

    /// Holds the networks until the first read settles or [`READ_TIMEOUT`]
    /// passes. When the wait times out, the pane is redrawn, so it stops
    /// saying it is checking even when no network event follows.
    pub(crate) fn hold_for_first_read(&mut self, cx: &mut Context<Self>) {
        self.initiatives.awaiting_read = true;
        cx.spawn(async move |hub, cx| {
            cx.background_executor().timer(READ_TIMEOUT).await;
            _ = hub.update(cx, |hub, cx| {
                if std::mem::take(&mut hub.initiatives.awaiting_read) {
                    hub.rerun();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn observe_initiative_identity(
        &mut self,
        identity: &IdentityObservation,
        cx: &mut Context<Self>,
    ) {
        self.initiatives
            .catalog
            .update(cx, |catalog, cx| catalog.observe_identity(identity, cx));
    }

    fn focus_initiative(&mut self, buffer: Option<BufferKey>) {
        let joined = buffer
            .as_ref()
            .is_some_and(|key| self.state.buffer(key).is_some_and(|buffer| buffer.joined));
        self.state
            .joins
            .focus(if joined { None } else { buffer.clone() });
        if joined {
            self.state.opened = buffer;
        }
    }

    /// Synchronise the selected initiative with the configured Subspace
    /// IRC connection. Also called after run-time settings replace a connection.
    /// Leaving a network takes the focus with it; arriving on one shows the
    /// initiative's first channel only after the person switched to it, so
    /// the selection made for them at start never takes the conversation
    /// they are in.
    pub(crate) fn apply_initiative(&mut self) {
        let selection = self
            .initiatives
            .selection
            .as_ref()
            .filter(|_| subspace_api_client::capabilities().initiative_channels)
            .zip(self.configured_initiative_network());
        let intent = selection.map(|(initiative, network)| {
            let focus = focus_channel(initiative);
            (network, initiative_channels(initiative), focus)
        });
        let new_network = intent.as_ref().map(|(network, ..)| network.clone());
        let old_network = std::mem::replace(&mut self.initiatives.network, new_network.clone());
        let changed_network = old_network != new_network;
        let left = changed_network && old_network.is_some();
        if let Some(network) = old_network.filter(|_| changed_network) {
            self.send(Command::SetAutojoin {
                network,
                channels: None,
            });
        }
        let show = self.initiatives.focus_pending && intent.is_some();
        if show || left {
            self.focus_initiative(intent.as_ref().and_then(first_channel));
        }
        if show {
            self.initiatives.focus_pending = false;
        }
        if let Some((network, channels, _)) = intent {
            self.send(Command::SetAutojoin {
                network,
                channels: Some(channels),
            });
        }
    }
}

impl InitiativeIrc {
    /// Follows the catalogue's selection; true when the runtime needs to
    /// update. Only a switch from one initiative to another leaves its first
    /// channel to show: the first selection, made for the person once their
    /// memberships are read, joins its channels without taking the
    /// conversation they are in.
    fn sync_selection(&mut self, cx: &gpui_kit::App) -> bool {
        let catalog = self.catalog.read(cx);
        let selected = catalog.selected().cloned();
        if self.selection == selected || self.keeps_through_scope_read(catalog, &selected) {
            return false;
        }
        let picked = catalog.picked_since(&mut self.picks);
        let id = |item: &Option<MemberInitiative>| item.as_ref().map(|item| item.id.clone());
        if id(&self.selection) != id(&selected) {
            self.focus_pending = picked && selected.is_some();
        }
        self.actor = catalog.actor().map(str::to_owned);
        self.selection = selected;
        true
    }

    /// Whether to keep following the previous initiative while the same
    /// person's memberships in another organisation are read, rather than
    /// the configured channels in between. Another person's never are.
    fn keeps_through_scope_read(
        &self,
        catalog: &Catalog,
        selected: &Option<MemberInitiative>,
    ) -> bool {
        selected.is_none()
            && self.selection.is_some()
            && unsettled(catalog)
            && catalog.actor().is_some()
            && catalog.actor() == self.actor.as_deref()
    }
}

/// Why an initiative with an assigned fleet has no server yet. A failed
/// lookup is retried by the catalogue; a fleet without a server is not.
fn unresolved_fleet(fleet: Option<&Fleet>) -> String {
    match fleet {
        None => "Fleet server unavailable. Retrying…".into(),
        Some(fleet) if !fleet.name.is_empty() => {
            format!("Fleet {} names no IRC server", fleet.name)
        }
        Some(_) => "The initiative's fleet names no IRC server".into(),
    }
}

/// The networks that can carry `endpoint`'s initiative channels. Capture with
/// a loopback fixture API uses only numeric loopback networks: the one on the
/// exact endpoint when there is one, so a local stack with a network per fleet
/// still switches between them, else any of them.
fn eligible<'a>(
    networks: &'a [NetworkConfig],
    endpoint: &Endpoint,
    loopback_fixture: bool,
) -> Vec<&'a NetworkConfig> {
    if !loopback_fixture {
        return networks
            .iter()
            .filter(|network| endpoint.matches(network))
            .collect();
    }
    let loopback: Vec<_> = networks
        .iter()
        .filter(|network| {
            network
                .host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
        })
        .collect();
    let exact: Vec<_> = loopback
        .iter()
        .copied()
        .filter(|network| endpoint.matches(network))
        .collect();
    if exact.is_empty() { loopback } else { exact }
}

/// The channel shown when an initiative is chosen, on its network.
fn first_channel(
    (network, _, focus): &(NetworkId, Vec<String>, Option<String>),
) -> Option<BufferKey> {
    Some(BufferKey {
        network: network.clone(),
        target: Target::Channel(focus.clone()?),
    })
}

pub(crate) fn refresh(board: &Irc, cx: &mut Context<Irc>) {
    if let Some(hub) = &board.hub {
        let hub = hub.read(cx);
        if hub.is_running() {
            hub.initiatives.catalog.clone().update(cx, Catalog::refresh);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Endpoint, channel_names, eligible};
    use crate::fixtures::config;
    use subspace_client_irc::NetworkConfig;

    fn eligible_one(network: &NetworkConfig, endpoint: &Endpoint, loopback_fixture: bool) -> bool {
        !eligible(std::slice::from_ref(network), endpoint, loopback_fixture).is_empty()
    }

    #[test]
    fn membership_channel_names_are_sent_only_to_the_initiative_server_or_capture_loopback() {
        let service = Endpoint::parse("ircs://irc.example.com:6697").unwrap();
        let mut network = config();
        assert!(!eligible_one(&network, &service, false));
        assert!(eligible_one(&network, &service, true));
        network.host = "irc.example.org".into();
        network.port = 6697;
        network.tls = true;
        assert!(!eligible_one(&network, &service, false));
        assert!(!eligible_one(&network, &service, true));

        network.host = "irc.example.com".into();
        assert!(eligible_one(&network, &service, false));
        assert!(!eligible_one(&network, &service, true));
        network.accept_invalid_certs = true;
        assert!(!eligible_one(&network, &service, false));
    }

    #[test]
    fn capture_loopback_prefers_the_network_on_the_fleet_endpoint() {
        let plain = config();
        let mut tls = config();
        tls.id = subspace_client_irc::NetworkId("tls".into());
        tls.port = 6697;
        tls.tls = true;
        tls.accept_invalid_certs = true;
        let networks = [plain, tls];
        let ids = |endpoint: &str| {
            let endpoint = Endpoint::parse(endpoint).unwrap();
            eligible(&networks, &endpoint, true)
                .iter()
                .map(|network| network.id.0.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(ids("ircs://127.0.0.1:6697"), ["tls"]);
        assert_eq!(ids("irc://127.0.0.1:16667"), [networks[0].id.0.clone()]);
        // No exact match: any loopback network, as the fixture API's fleets
        // need not name the local test server.
        assert_eq!(ids("ircs://irc.example.com:6697").len(), 2);
    }

    #[test]
    fn fleet_roles_belong_only_to_the_network_on_the_fleet_s_server() {
        let initiatives: Vec<subspace_api_client::model::MemberInitiative> =
            serde_json::from_value(serde_json::json!([
                {"id": "one", "name": "One", "channels": "#aurora-starbase", "fleetId": "aurora",
                 "fleet": {"id": "aurora", "slug": "aurora", "ircHost": "irc.example.com",
                           "ircPort": 6697, "ircTls": true, "captainNames": ["Lyra"]}},
                {"id": "two", "name": "Two", "channels": "#local", "fleetId": null, "fleet": null}
            ]))
            .unwrap();
        let mut fleet = config();
        fleet.host = "irc.example.com".into();
        fleet.port = 6697;
        fleet.tls = true;
        let mut local = config();
        local.id = subspace_client_irc::NetworkId("home".into());
        local.host = "irc.example.org".into();
        let networks = [fleet.clone(), local.clone()];
        let slugs = |network: &NetworkConfig| {
            super::initiatives_on(&initiatives, &networks, &network.id, false)
                .iter()
                .filter_map(|initiative| initiative.fleet.as_ref())
                .map(|fleet| fleet.slug.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(slugs(&fleet), ["aurora"]);
        assert!(
            slugs(&local).is_empty(),
            "a local-only network serves no fleet"
        );
    }

    #[test]
    fn two_initiatives_of_one_fleet_give_its_roles_once() {
        let fleet = serde_json::json!({
            "id": "aurora", "slug": "aurora", "ircHost": "irc.example.com", "ircPort": 6697,
            "ircTls": true,
            "agents": [{"name": "Lyra", "rank": "captain", "ircNick": "aurora-lyra",
                        "available": true}]
        });
        let initiatives: Vec<subspace_api_client::model::MemberInitiative> =
            serde_json::from_value(serde_json::json!([
                {"id": "one", "name": "One", "channels": "", "fleetId": "aurora", "fleet": fleet},
                {"id": "two", "name": "Two", "channels": "", "fleetId": "aurora", "fleet": fleet}
            ]))
            .unwrap();
        let mut network = config();
        network.host = "irc.example.com".into();
        network.port = 6697;
        network.tls = true;
        let networks = [network.clone()];
        let held = super::initiatives_on(&initiatives, &networks, &network.id, false);
        assert_eq!(held.len(), 2, "one per initiative");
        let roster = crate::fleet::Roster::for_initiatives(&held);
        assert_eq!(
            roster.rank("aurora-lyra"),
            Some(crate::fleet::Rank::Captain),
            "the shared fleet is no clash with itself"
        );
    }

    #[test]
    fn an_initiative_joins_its_fleet_s_recorded_channels_then_its_own() {
        let initiative: subspace_api_client::model::MemberInitiative =
            serde_json::from_value(serde_json::json!({
                "id": "one", "name": "One", "channels": "#aurora-nav, #Aurora-Starbase",
                "fleetId": "aurora",
                "fleet": {"id": "aurora", "slug": "aurora",
                          "starbaseChannel": "#aurora-starbase", "commandChannel": "#aurora-command",
                          "agents": [{"name": "Lyra", "rank": "captain", "ircNick": "aurora-lyra",
                                      "shipChannel": "#aurora-lyra"}],
                          "ships": [{"id": "s1", "name": "Lattice", "channel": "#aurora-lyra"},
                                    {"id": "s2", "name": "Plover", "channel": null}]}
            }))
            .unwrap();
        assert_eq!(
            super::initiative_channels(&initiative),
            [
                "#aurora-starbase",
                "#aurora-command",
                "#aurora-lyra",
                "#aurora-nav"
            ],
            "the fleet's first, each once ignoring case, then the initiative's own"
        );
        let unrecorded: subspace_api_client::model::MemberInitiative =
            serde_json::from_value(serde_json::json!({
                "id": "two", "name": "Two", "channels": "#local", "fleetId": "aurora",
                "fleet": {"id": "aurora", "slug": "aurora",
                          "agents": [{"name": "Lyra", "rank": "captain", "ircNick": "aurora-lyra"}]}
            }))
            .unwrap();
        assert_eq!(
            super::initiative_channels(&unrecorded),
            ["#local"],
            "no guessed standard names are joined"
        );
    }

    #[test]
    fn only_channel_tokens_are_used_in_their_original_order() {
        assert_eq!(
            channel_names(
                " #Design &Shared ,#design,#releases\t#ops,ordinary,#, #bad key, #bad\r\nline, #bad:mask,,,"
            ),
            ["#Design", "&Shared", "#releases", "#ops"],
        );
    }
}
