//! A fleet's Fleet API address, checked and normalised once. A token is
//! bound to the normalised text, so two spellings of one address compare
//! equal and any other address does not.

use std::fmt;
use url::{Host, Url};

use crate::Error;

/// The longest address accepted, as the Fleet Console accepts.
const MAX_ADDRESS_CHARS: usize = 2000;

/// An absolute `https` address, or `http` on this computer, without
/// credentials, query or fragment, kept as `origin` plus path with no
/// trailing slash and no trailing `/api/v1`: the Fleet Console's
/// `normalizeEndpoint` gives the same text for the same address.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FleetAddress {
    text: String,
}

impl FleetAddress {
    /// Checks and normalises `raw`, as the Subspace API records it or a
    /// person types it.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let raw = raw.trim();
        if raw.is_empty() || raw.chars().count() > MAX_ADDRESS_CHARS {
            return Err(Error::InvalidAddress);
        }
        let url = Url::parse(raw).map_err(|_| Error::InvalidAddress)?;
        if !acceptable(&url) {
            return Err(Error::InvalidAddress);
        }
        Ok(Self {
            text: normalised(&url),
        })
    }

    /// The normalised address, such as `https://fleet.example.com`.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The URL of `path`, such as `/api/v1/fleet`, under this address.
    pub(crate) fn url(&self, segments: &[&str]) -> Result<Url, Error> {
        let mut url = Url::parse(&self.text).map_err(|_| Error::InvalidAddress)?;
        url.path_segments_mut()
            .map_err(|_| Error::InvalidAddress)?
            .pop_if_empty()
            .extend(["api", "v1"])
            .extend(segments);
        Ok(url)
    }
}

/// `https` anywhere, `http` only to loopback; no credentials, query or
/// fragment.
fn acceptable(url: &Url) -> bool {
    let secure = match url.scheme() {
        "https" => true,
        "http" => loopback(url.host()),
        _ => false,
    };
    secure
        && url.host().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
}

fn loopback(host: Option<Host<&str>>) -> bool {
    match host {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        Some(Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

fn normalised(url: &Url) -> String {
    let path = url.path().trim_end_matches('/');
    let path = path.strip_suffix("/api/v1").unwrap_or(path);
    format!("{}{path}", url.origin().ascii_serialization())
}

impl fmt::Debug for FleetAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FleetAddress({})", self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::FleetAddress;
    use crate::Error;

    #[test]
    fn addresses_are_normalised_like_the_fleet_console() {
        let raw = [
            "https://Fleet.Example.com/",
            "https://fleet.example.com:443",
            "https://fleet.example.com:8443/",
            "  https://fleet.example.com/stargate//  ",
            "https://fleet.example.com/api/v1/",
            "http://127.0.0.1:8484",
            "http://localhost:8484/",
            "http://[::1]:8484",
        ];
        let normalised: Vec<String> = raw
            .iter()
            .filter_map(|raw| FleetAddress::parse(raw).ok())
            .map(|address| address.as_str().to_owned())
            .collect();
        assert_eq!(
            normalised,
            [
                "https://fleet.example.com",
                "https://fleet.example.com",
                "https://fleet.example.com:8443",
                "https://fleet.example.com/stargate",
                "https://fleet.example.com",
                "http://127.0.0.1:8484",
                "http://localhost:8484",
                "http://[::1]:8484",
            ]
        );
    }

    #[test]
    fn unsafe_or_ambiguous_addresses_are_refused() {
        for raw in [
            "",
            "fleet.example.com",
            "http://fleet.example.com",
            "http://10.0.0.4:8484",
            "ftp://fleet.example.com",
            "https://user:pass@fleet.example.com",
            "https://user@fleet.example.com",
            "https://fleet.example.com/?token=x",
            "https://fleet.example.com/#token",
            "file:///tmp/api.sock",
        ] {
            assert_eq!(
                FleetAddress::parse(raw),
                Err(Error::InvalidAddress),
                "{raw}"
            );
        }
        assert_eq!(
            FleetAddress::parse(&format!("https://example.com/{}", "a".repeat(2000))),
            Err(Error::InvalidAddress)
        );
    }

    #[test]
    fn routes_are_under_the_api_version() {
        let address = FleetAddress::parse("https://fleet.example.com/stargate").unwrap();
        assert_eq!(
            address.url(&["ships", "a b/c"]).unwrap().as_str(),
            "https://fleet.example.com/stargate/api/v1/ships/a%20b%2Fc"
        );
        let root = FleetAddress::parse("http://127.0.0.1:8484").unwrap();
        assert_eq!(
            root.url(&["fleet"]).unwrap().as_str(),
            "http://127.0.0.1:8484/api/v1/fleet"
        );
    }
}
