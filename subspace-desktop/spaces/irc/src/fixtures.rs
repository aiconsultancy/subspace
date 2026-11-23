//! Synthetic networks, buffers and lines for tests. No real hosts or people.

use chrono::{DateTime, TimeZone as _, Utc};
use subspace_client_irc::{BufferKey, Line, LineKind, Member, NetworkConfig, NetworkId, Target};

pub(crate) const NETWORK: &str = "local";

pub(crate) fn network_id() -> NetworkId {
    NetworkId(NETWORK.into())
}

pub(crate) fn config() -> NetworkConfig {
    serde_json::from_value(serde_json::json!({
        "id": NETWORK, "name": "Acme IRC", "host": "127.0.0.1", "port": 16667,
        "tls": false, "nickname": "subspace", "channels": ["#subspace"]
    }))
    .unwrap_or_else(|error| panic!("fixture config: {error}"))
}

pub(crate) fn channel(name: &str) -> BufferKey {
    BufferKey {
        network: network_id(),
        target: Target::Channel(name.into()),
    }
}

pub(crate) fn query(nick: &str) -> BufferKey {
    BufferKey {
        network: network_id(),
        target: Target::Query(nick.into()),
    }
}

/// 13 November 2026, `minute` minutes after 09:00 UTC.
pub(crate) fn at(minute: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 13, 9, 0, 0)
        .single()
        .unwrap_or_default()
        + chrono::Duration::minutes(minute)
}

pub(crate) fn line(
    buffer: &BufferKey,
    kind: LineKind,
    from: &str,
    text: &str,
    minute: i64,
) -> Line {
    let mut line = Line::new(buffer.clone(), at(minute), kind, text.into());
    line.from = (!from.is_empty()).then(|| from.to_owned());
    line.own = from == "subspace";
    line
}

pub(crate) fn message(buffer: &BufferKey, from: &str, text: &str, minute: i64) -> Line {
    line(buffer, LineKind::Privmsg, from, text, minute)
}

pub(crate) fn member(nick: &str, prefixes: &str) -> Member {
    Member {
        nick: nick.into(),
        prefixes: prefixes.into(),
        away: false,
        account: None,
    }
}
