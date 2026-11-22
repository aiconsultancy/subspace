//! Applies the desktop initiative selection to the verified IRC runtime.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::{Context, Entity, Subscription};
use subspace_api_client::initiatives::{Catalog, IdentityObservation};
use subspace_api_client::model::{Fleet, MemberInitiative};
use subspace_client_irc::{BufferKey, Command, NetworkConfig, NetworkId, Target};

use crate::Irc;
use crate::fleet::Roster;
use crate::hub::Hub;

const MAX_CHANNEL_BYTES: usize = 200;

mod endpoint;
mod picker;
pub(crate) use endpoint::Endpoint;
pub(crate) use picker::{Picker, button, command, dismiss_stale_picker, hint};

pub(crate) struct InitiativeIrc {
    pub(crate) catalog: Entity<Catalog>,
    selection: Option<MemberInitiative>,
    /// The network on which an initiative replaces local autojoin.
    network: Option<NetworkId>,
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

    pub(crate) fn selected_for_runtime(&self) -> bool {
        self.network.is_some()
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
        let memberships = self.catalog.read(cx).memberships();
        let roster = Rc::new(Roster::new(fleets_on(
            memberships,
            networks,
            network,
            self.loopback_fixture,
        )));
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

/// The fleets of `initiatives` whose server is `network` among `networks`.
fn fleets_on<'a>(
    initiatives: &'a [MemberInitiative],
    networks: &[NetworkConfig],
    network: &NetworkId,
    loopback_fixture: bool,
) -> Vec<&'a Fleet> {
    initiatives
        .iter()
        .filter(|initiative| {
            Endpoint::from_fleet(initiative).is_some_and(|endpoint| {
                eligible(networks, &endpoint, loopback_fixture)
                    .iter()
                    .any(|eligible| eligible.id == *network)
            })
        })
        .filter_map(|initiative| initiative.fleet.as_ref())
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
        self.initiatives._subscription =
            Some(cx.observe(&self.initiatives.catalog, |hub, _, cx| {
                hub.initiatives.forget_rosters();
                // Any change is applied, so a selection that went with its
                // scope also takes its autojoin with it. Without initiative
                // channels (a NETT organisation that does not serve them),
                // nothing new switches network or autojoins.
                if let Some(focus) = hub.initiatives.sync_selection(cx) {
                    hub.apply_initiative(focus);
                }
                cx.notify();
            }));
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
    pub(crate) fn apply_initiative(&mut self, focus: bool) {
        let selection = self
            .initiatives
            .selection
            .as_ref()
            .filter(|_| subspace_api_client::capabilities().initiative_channels)
            .zip(self.configured_initiative_network());
        let intent =
            selection.map(|(initiative, network)| (network, channel_names(&initiative.channels)));
        let new_network = intent.as_ref().map(|(network, _)| network.clone());
        let old_network = std::mem::replace(&mut self.initiatives.network, new_network.clone());
        let changed_network = old_network != new_network;
        if let Some(network) = old_network.filter(|_| changed_network) {
            self.send(Command::SetAutojoin {
                network,
                channels: None,
            });
        }
        if focus || changed_network {
            self.focus_initiative(intent.as_ref().and_then(first_channel));
        }
        if let Some((network, channels)) = intent {
            self.send(Command::SetAutojoin {
                network,
                channels: Some(channels),
            });
        }
    }
}

impl InitiativeIrc {
    /// Reports whether the runtime needs to update and whether to focus a new room.
    fn sync_selection(&mut self, cx: &gpui_kit::App) -> Option<bool> {
        let selected = self.catalog.read(cx).selected().cloned();
        if self.selection == selected {
            return None;
        }
        let focus = selected
            .as_ref()
            .is_some_and(|new| self.selection.as_ref().is_none_or(|old| old.id != new.id));
        self.selection = selected;
        Some(focus)
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

fn first_channel((network, channels): &(NetworkId, Vec<String>)) -> Option<BufferKey> {
    Some(BufferKey {
        network: network.clone(),
        target: Target::Channel(channels.first()?.clone()),
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

/// A JOIN accepts a single channel token. Store and show the first spelling,
/// with the same ASCII case-insensitive comparison as registration.
pub(crate) fn channel_names(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.split(',')
        .flat_map(|entry| {
            let names = entry
                .trim()
                .split([' ', '\t'])
                .filter(|name| !name.is_empty());
            names
                .clone()
                .all(valid_channel)
                .then_some(names)
                .into_iter()
                .flatten()
        })
        .filter(|name| seen.insert(name.to_ascii_lowercase()))
        .map(str::to_owned)
        .collect()
}

fn valid_channel(name: &str) -> bool {
    // Leave room on the 512-byte IRC line for the JOIN command.
    (2..=MAX_CHANNEL_BYTES).contains(&name.len())
        && name.starts_with(['#', '&', '+', '!'])
        && !name.contains(|c: char| c.is_whitespace() || c.is_control() || c == ':')
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
            super::fleets_on(&initiatives, &networks, &network.id, false)
                .iter()
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
            "agents": [{"name": "Lyra", "rank": "captain", "ircNick": "aurora-lyra"}]
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
        let fleets = super::fleets_on(&initiatives, &networks, &network.id, false);
        assert_eq!(fleets.len(), 2, "one per initiative");
        let roster = crate::fleet::Roster::new(fleets);
        assert_eq!(
            roster.rank("aurora-lyra"),
            Some(crate::fleet::Rank::Captain),
            "the shared fleet is no clash with itself"
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
