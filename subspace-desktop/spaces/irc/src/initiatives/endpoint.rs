//! Which IRC server an initiative's channels belong to: its fleet's, from
//! Subspace API, or the app's default Subspace IRC server.

use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::NetworkConfig;

/// Overrides the default Subspace IRC server, as `irc://host:port` (plain)
/// or `ircs://host:port` (TLS).
pub(crate) const ENDPOINT_ENV: &str = "SUBSPACE_IRC_ENDPOINT";
/// The local development server: Ergo's plain listener on loopback.
const DEFAULT_ENDPOINT: &str = "irc://127.0.0.1:6667";
const PLAIN_PORT: u16 = 6667;
const TLS_PORT: u16 = 6697;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) tls: bool,
}

impl Endpoint {
    /// `irc://host[:port]` or `ircs://host[:port]`.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let (tls, rest) = match value.trim().split_once("://")? {
            ("irc", rest) => (false, rest),
            ("ircs", rest) => (true, rest),
            _ => return None,
        };
        let rest = rest.trim_end_matches('/');
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) if !host.ends_with(']') || rest.starts_with('[') => {
                (host, port.parse().ok()?)
            }
            _ => (rest, if tls { TLS_PORT } else { PLAIN_PORT }),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        (!host.is_empty() && !host.contains(['/', '@', ' '])).then(|| Self {
            host: host.to_owned(),
            port,
            tls,
        })
    }

    /// The selected initiative's fleet server, the default one when it has no
    /// fleet, or `None` while its assigned fleet's server is unknown (for
    /// example after a failed fleet lookup), so nothing joins its channels on
    /// another fleet's network.
    pub(crate) fn for_initiative(
        initiative: Option<&MemberInitiative>,
        default: &Self,
    ) -> Option<Self> {
        initiative
            .filter(assigned)
            .map_or_else(|| Some(default.clone()), Self::from_fleet)
    }

    pub(crate) fn from_fleet(initiative: &MemberInitiative) -> Option<Self> {
        let fleet = initiative.fleet.as_ref()?;
        Some(Self {
            host: fleet.irc_host.clone().filter(|host| !host.is_empty())?,
            port: fleet.irc_port?,
            tls: fleet.irc_tls.unwrap_or(false),
        })
    }

    /// The app's default, from `SUBSPACE_IRC_ENDPOINT` or the local server.
    pub(crate) fn configured() -> Self {
        resolve(std::env::var(ENDPOINT_ENV).ok().as_deref())
    }

    /// A configured network on this server. Certificate checks may only be
    /// relaxed for a loopback server, such as a local self-signed one.
    pub(crate) fn matches(&self, network: &NetworkConfig) -> bool {
        network.host.eq_ignore_ascii_case(&self.host)
            && network.port == self.port
            && network.tls == self.tls
            && (!network.accept_invalid_certs || loopback(&network.host))
    }

    /// Whether the server is on this computer.
    pub(crate) fn is_loopback(&self) -> bool {
        loopback(&self.host)
    }

    pub(crate) fn label(&self) -> String {
        let scheme = if self.tls { "ircs" } else { "irc" };
        format!("{scheme}://{}:{}", self.host, self.port)
    }
}

/// Whether the initiative belongs to a fleet, known or not.
fn assigned(initiative: &&MemberInitiative) -> bool {
    initiative.fleet_id.is_some() || initiative.fleet.is_some()
}

fn resolve(value: Option<&str>) -> Endpoint {
    value
        .and_then(Endpoint::parse)
        .or_else(|| Endpoint::parse(DEFAULT_ENDPOINT))
        .unwrap_or(Endpoint {
            host: "127.0.0.1".into(),
            port: PLAIN_PORT,
            tls: false,
        })
}

fn loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::{Endpoint, resolve};
    use crate::fixtures::config;
    use subspace_api_client::model::MemberInitiative;

    fn initiative(fleet: &str) -> MemberInitiative {
        serde_json::from_str(&format!(
            r##"{{"id":"one","name":"One","channels":"#one","fleetId":null,"fleet":{fleet}}}"##
        ))
        .unwrap()
    }

    #[test]
    fn endpoints_parse_with_default_ports() {
        assert_eq!(
            Endpoint::parse("ircs://irc.example.com"),
            Some(Endpoint {
                host: "irc.example.com".into(),
                port: 6697,
                tls: true
            })
        );
        assert_eq!(Endpoint::parse("irc://[::1]:6668").unwrap().host, "::1");
        assert!(Endpoint::parse("https://irc.example.com").is_none());
        assert_eq!(resolve(None).label(), "irc://127.0.0.1:6667");
        assert_eq!(resolve(Some("nonsense")).label(), "irc://127.0.0.1:6667");
    }

    #[test]
    fn a_fleet_server_wins_over_the_default_and_must_match_exactly() {
        let default = resolve(None);
        let fleet = initiative(
            r#"{"id":"f","slug":"blue","name":"Blue","ircHost":"irc.example.com","ircPort":6697,"ircTls":true}"#,
        );
        let endpoint = Endpoint::for_initiative(Some(&fleet), &default).unwrap();
        assert_eq!(endpoint.label(), "ircs://irc.example.com:6697");
        let without = initiative("null");
        assert_eq!(
            Endpoint::for_initiative(Some(&without), &default),
            Some(default.clone())
        );
        assert_eq!(Endpoint::for_initiative(None, &default), Some(default));
        let mut network = config();
        network.host = "IRC.example.com".into();
        network.port = 6697;
        network.tls = true;
        assert!(endpoint.matches(&network));
        network.accept_invalid_certs = true;
        assert!(!endpoint.matches(&network));
        network.tls = false;
        assert!(!endpoint.matches(&network));
    }

    #[test]
    fn an_assigned_fleet_without_a_known_server_never_falls_back_to_the_default() {
        let default = resolve(None);
        let mut lookup_failed = initiative("null");
        lookup_failed.fleet_id = Some("f".into());
        assert_eq!(
            Endpoint::for_initiative(Some(&lookup_failed), &default),
            None
        );
        let no_server = initiative(r#"{"id":"f","slug":"blue","name":"Blue"}"#);
        assert_eq!(Endpoint::for_initiative(Some(&no_server), &default), None);
    }

    #[test]
    fn a_loopback_server_may_use_a_self_signed_certificate() {
        let endpoint = Endpoint::parse("ircs://127.0.0.1:6697").unwrap();
        let mut network = config();
        network.host = "127.0.0.1".into();
        network.port = 6697;
        network.tls = true;
        network.accept_invalid_certs = true;
        assert!(endpoint.matches(&network));
    }
}
