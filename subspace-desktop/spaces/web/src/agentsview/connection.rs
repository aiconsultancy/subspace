//! The address and its credential are saved and replaced together.

use serde_json::{Map, Value};
use subspace_framework::core::{Secret, config::web_boards::web_board_url_allowed};
use url::Url;

pub(super) const ADDRESS: &str = "address";
pub(super) const KEY: &str = "apiKey";
const MAX_KEY_BYTES: usize = 4096;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Connection {
    pub(crate) address: Url,
    pub(crate) key: Option<Secret>,
}

impl Connection {
    pub(super) fn from_saved(saved: &Map<String, Value>) -> Result<Option<Self>, &'static str> {
        Self::parse(
            saved.get(ADDRESS).and_then(Value::as_str).unwrap_or(""),
            saved.get(KEY).and_then(Value::as_str).unwrap_or(""),
        )
    }

    pub(super) fn parse(address: &str, key: &str) -> Result<Option<Self>, &'static str> {
        let address = address.trim();
        let key = key.trim();
        if address.is_empty() {
            return if key.is_empty() {
                Ok(None)
            } else {
                Err("Enter the fleet’s AgentsView address.")
            };
        }
        let mut address = Url::parse(address)
            .ok()
            .filter(|url| {
                web_board_url_allowed(url) && url.query().is_none() && url.fragment().is_none()
            })
            .ok_or("Enter an HTTPS address or a local HTTP address without a query or fragment.")?;
        if !address.path().bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'~' | b'/' | b'-')
        }) {
            return Err(
                "Use a simple AgentsView URL path with letters, numbers, dots, underscores, tildes or hyphens.",
            );
        }
        if !address.path().ends_with('/') {
            let path = format!("{}/", address.path());
            address.set_path(&path);
        }
        if key.len() > MAX_KEY_BYTES || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err("Enter a single AgentsView API key without spaces.");
        }
        Ok(Some(Self {
            address,
            key: (!key.is_empty()).then(|| Secret::new(key)),
        }))
    }

    pub(crate) fn admits(home: &Url, target: &str) -> bool {
        Url::parse(target).is_ok_and(|url| {
            web_board_url_allowed(&url)
                && url.origin() == home.origin()
                && url.path().starts_with(home.path())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Connection;

    #[test]
    fn validates_the_address_and_keeps_a_proxy_prefix() {
        let connection =
            Connection::parse(" https://fleet.example.com:8443/view ", " example-key ")
                .unwrap()
                .unwrap();
        assert_eq!(
            connection.address.as_str(),
            "https://fleet.example.com:8443/view/"
        );
        assert_eq!(connection.key.unwrap().expose(), "example-key");
        for address in [
            "http://fleet.example.com/",
            "https://user@fleet.example.com/",
            "https://fleet.example.com/?key=example",
            "https://fleet.example.com/#sessions",
            "https://fleet.example.com/view{team}/",
        ] {
            assert!(Connection::parse(address, "").is_err());
        }
        assert!(
            Connection::parse("http://127.0.0.1:8080", "")
                .unwrap()
                .is_some()
        );
        assert!(Connection::parse("", "").unwrap().is_none());
    }

    #[test]
    fn navigation_stays_at_the_configured_service() {
        let home = url::Url::parse("https://fleet.example.com:8443/view/").unwrap();
        assert!(Connection::admits(
            &home,
            "https://fleet.example.com:8443/view/sessions/1"
        ));
        for outside in [
            "https://fleet.example.com:8443/views/",
            "https://fleet.example.com:8443/view/../elsewhere",
            "https://fleet.example.com/view/",
            "https://other.example.com:8443/view/",
        ] {
            assert!(!Connection::admits(&home, outside));
        }
    }
}
