//! Which networks the account brings: one for each distinct IRC server its
//! fleets use, unless a saved network already connects there. Pure, so it
//! is tested without a runtime or an API.

use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{CredentialSource, NetworkConfig, NetworkId, Sasl};

use super::SignedIn;
use super::device::sasl_username;
use crate::editor::ids;
use crate::initiatives::Endpoint;

/// One IRC server the account's fleets use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Server {
    pub endpoint: Endpoint,
    /// The names of the fleets on it, each once, in the API's order.
    pub fleets: Vec<String>,
}

impl Server {
    /// The name its network shows: its fleets', else its host.
    pub fn name(&self) -> String {
        if self.fleets.is_empty() {
            self.endpoint.host.clone()
        } else {
            self.fleets.join(", ")
        }
    }
}

/// The distinct IRC servers of `initiatives`' fleets, in the API's order.
/// Only a server a sign-in can be sent to safely counts: over TLS, or on
/// this computer. A plaintext server elsewhere is left to a saved network.
pub(crate) fn servers(initiatives: &[MemberInitiative]) -> Vec<Server> {
    let mut servers: Vec<Server> = Vec::new();
    for initiative in initiatives {
        let (Some(fleet), Some(endpoint)) = (
            initiative.fleet.as_ref(),
            Endpoint::from_fleet(initiative)
                .filter(|endpoint| endpoint.tls || endpoint.is_loopback()),
        ) else {
            continue;
        };
        let name = if fleet.name.is_empty() {
            &fleet.slug
        } else {
            &fleet.name
        };
        match servers
            .iter_mut()
            .find(|server| same_endpoint(&server.endpoint, &endpoint))
        {
            Some(server) if !name.is_empty() && !server.fleets.contains(name) => {
                server.fleets.push(name.clone());
            }
            Some(_) => {}
            None => servers.push(Server {
                endpoint,
                fleets: (!name.is_empty())
                    .then(|| name.clone())
                    .into_iter()
                    .collect(),
            }),
        }
    }
    servers
}

fn same_endpoint(one: &Endpoint, other: &Endpoint) -> bool {
    one.host.eq_ignore_ascii_case(&other.host) && one.port == other.port && one.tls == other.tls
}

/// Whether a network on `host` and `port` connects to `server`, whatever
/// its TLS settings: a second connection there would be one too many.
pub(crate) fn same_server(host: &str, port: u16, server: &Server) -> bool {
    host.trim().eq_ignore_ascii_case(&server.endpoint.host) && port == server.endpoint.port
}

/// The saved network that connects to `server`, if any.
pub(crate) fn saved_on<'a>(
    saved: &'a [NetworkConfig],
    server: &Server,
) -> Option<&'a NetworkConfig> {
    saved
        .iter()
        .find(|network| same_server(&network.host, network.port, server))
}

/// The account's networks: one for each of `servers` that no `saved`
/// network connects to, signed in as `person` with logins from `source`.
pub(crate) fn networks(
    servers: &[Server],
    saved: &[NetworkConfig],
    person: &SignedIn,
    login: (&CredentialSource, &str),
) -> Vec<NetworkConfig> {
    let mut taken: Vec<String> = saved.iter().map(|network| network.id.0.clone()).collect();
    servers
        .iter()
        .filter(|server| saved_on(saved, server).is_none())
        .map(|server| {
            let id = network_id(person, &server.endpoint, &taken);
            taken.push(id.clone());
            network(server, NetworkId(id), person, login)
        })
        .collect()
}

/// A stable id from the account and the server, so the network keeps its
/// stored messages across runs, apart from other accounts' and from the
/// saved networks' ids.
fn network_id(person: &SignedIn, endpoint: &Endpoint, taken: &[String]) -> String {
    let base = ids::slug(&format!(
        "{} {} {}",
        person.account, endpoint.host, endpoint.port
    ));
    ids::unique_id(&base, taken)
}

fn network(
    server: &Server,
    id: NetworkId,
    person: &SignedIn,
    (source, device): (&CredentialSource, &str),
) -> NetworkConfig {
    NetworkConfig {
        id,
        name: server.name(),
        host: server.endpoint.host.clone(),
        port: server.endpoint.port,
        tls: server.endpoint.tls,
        accept_invalid_certs: false,
        nickname: person.account.clone(),
        alt_nicks: Vec::new(),
        username: String::new(),
        realname: String::new(),
        password: String::new(),
        sasl: Some(Sasl {
            username: sasl_username(&person.account, device),
            password: String::new(),
            password_file: None,
            external: false,
            credential: Some(source.clone()),
        }),
        channels: Vec::new(),
        auto_connect: true,
        highlights: Vec::new(),
        ca_cert: None,
        client_cert: None,
        client_key: None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_api_client::model::MemberInitiative;
    use subspace_client_irc::{CredentialSource, NetworkConfig};

    use super::{networks, saved_on, servers};
    use crate::fixtures::config;
    use crate::managed::SignedIn;

    fn ada() -> SignedIn {
        SignedIn {
            username: "ada".into(),
            account: "ada".into(),
        }
    }

    fn source() -> CredentialSource {
        CredentialSource::new("fixture-api ada", || Err("not in tests".into()))
    }

    fn initiative(id: &str, fleet: serde_json::Value) -> serde_json::Value {
        json!({"id": id, "name": id, "channels": "", "fleetId": fleet["id"], "fleet": fleet})
    }

    fn fleet(id: &str, name: &str, host: &str, port: u16, tls: bool) -> serde_json::Value {
        json!({"id": id, "slug": id, "name": name, "ircHost": host, "ircPort": port, "ircTls": tls})
    }

    fn catalogue(items: Vec<serde_json::Value>) -> Vec<MemberInitiative> {
        serde_json::from_value(json!(items)).unwrap()
    }

    #[test]
    fn each_distinct_server_of_the_fleets_is_one_network() {
        let initiatives = catalogue(vec![
            initiative(
                "one",
                fleet("aurora", "Aurora", "irc.example.com", 6697, true),
            ),
            initiative(
                "two",
                fleet("aurora", "Aurora", "irc.example.com", 6697, true),
            ),
            initiative(
                "three",
                fleet("boreal", "Boreal", "IRC.example.com", 6697, true),
            ),
            initiative("four", fleet("cygnus", "", "irc.example.org", 6697, true)),
            json!({"id": "local", "name": "Local", "channels": "#local", "fleetId": null, "fleet": null}),
            json!({"id": "unread", "name": "Unread", "channels": "", "fleetId": "gone", "fleet": null}),
        ]);
        let found = servers(&initiatives);
        let named: Vec<(String, String)> = found
            .iter()
            .map(|server| (server.endpoint.label(), server.name()))
            .collect();
        assert_eq!(
            named,
            [
                (
                    "ircs://irc.example.com:6697".into(),
                    "Aurora, Boreal".into()
                ),
                ("ircs://irc.example.org:6697".into(), "cygnus".into()),
            ]
        );
    }

    #[test]
    fn a_sign_in_never_goes_to_a_plaintext_server_elsewhere() {
        let initiatives = catalogue(vec![
            initiative(
                "remote",
                fleet("plain", "Plain", "irc.example.com", 6667, false),
            ),
            initiative("local", fleet("dev", "Dev", "127.0.0.1", 6667, false)),
        ]);
        let found = servers(&initiatives);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].endpoint.label(), "irc://127.0.0.1:6667");
    }

    #[test]
    fn a_managed_network_signs_in_as_the_account_from_this_device() {
        let initiatives = catalogue(vec![initiative(
            "one",
            fleet("aurora", "Aurora", "irc.example.com", 6697, true),
        )]);
        let made = networks(
            &servers(&initiatives),
            &[config()],
            &ada(),
            (&source(), "0123456789abcdef"),
        );
        assert_eq!(made.len(), 1);
        let network = &made[0];
        assert_eq!(network.id.0, "ada-irc-example-com-6697");
        assert_eq!(network.name, "Aurora");
        assert_eq!(
            (network.host.as_str(), network.port, network.tls),
            ("irc.example.com", 6697, true)
        );
        assert!(!network.accept_invalid_certs && network.auto_connect);
        assert_eq!(network.nickname, "ada", "the nick is the plain account");
        assert!(network.channels.is_empty(), "the initiative picks channels");
        let sasl = network.sasl.as_ref().unwrap();
        assert_eq!(sasl.username, "ada@0123456789abcdef");
        assert!(sasl.password.is_empty() && sasl.password_file.is_none());
        assert_eq!(sasl.credential.as_ref(), Some(&source()));
        let saved = serde_json::to_value(network).unwrap();
        assert!(
            saved["sasl"].get("credential").is_none(),
            "nothing of the source can be written"
        );
    }

    #[test]
    fn a_saved_network_on_the_same_server_is_kept_instead() {
        let initiatives = catalogue(vec![
            initiative(
                "one",
                fleet("aurora", "Aurora", "irc.example.com", 6697, true),
            ),
            initiative(
                "two",
                fleet("boreal", "Boreal", "irc.example.org", 6697, true),
            ),
        ]);
        let found = servers(&initiatives);
        let mut manual: NetworkConfig = config();
        manual.host = "IRC.example.com ".trim().into();
        manual.port = 6697;
        manual.tls = false;
        let made = networks(&found, &[manual.clone()], &ada(), (&source(), "d"));
        assert_eq!(made.len(), 1, "no second connection to the same server");
        assert_eq!(made[0].host, "irc.example.org");
        assert_eq!(
            saved_on(&[manual], &found[0]).map(|n| n.id.0.as_str()),
            Some("local")
        );
    }

    #[test]
    fn a_managed_id_never_takes_a_saved_one() {
        let initiatives = catalogue(vec![initiative(
            "one",
            fleet("aurora", "Aurora", "irc.example.com", 6697, true),
        )]);
        let mut clash = config();
        clash.id = subspace_client_irc::NetworkId("ada-irc-example-com-6697".into());
        let made = networks(&servers(&initiatives), &[clash], &ada(), (&source(), "d"));
        assert_eq!(made[0].id.0, "ada-irc-example-com-6697-2");
    }
}
