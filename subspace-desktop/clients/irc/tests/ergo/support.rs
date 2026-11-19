//! Local server selection and isolated client configuration.

use std::time::{SystemTime, UNIX_EPOCH};
use subspace_client_irc::{NetworkConfig, NetworkId};

pub(super) fn suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_millis() % 1_000_000)
        .unwrap_or_default()
}

pub(super) fn server() -> Option<(String, u16)> {
    let value = std::env::var("IRC_TEST_SERVER").ok()?;
    let (host, port) = value.rsplit_once(':')?;
    Some((host.to_string(), port.parse().ok()?))
}

pub(super) fn config(id: &str, nick: &str, (host, port): &(String, u16)) -> NetworkConfig {
    NetworkConfig {
        id: NetworkId(id.into()),
        name: id.into(),
        host: host.clone(),
        port: *port,
        tls: false,
        accept_invalid_certs: false,
        nickname: nick.into(),
        alt_nicks: Vec::new(),
        username: String::new(),
        realname: String::new(),
        password: String::new(),
        sasl: None,
        channels: Vec::new(),
        auto_connect: true,
        highlights: Vec::new(),
        ca_cert: None,
        client_cert: None,
        client_key: None,
    }
}
