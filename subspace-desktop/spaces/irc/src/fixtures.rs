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

/// Subspace API's NAV-2 as the tracker returns it, flagged `ready` and
/// assigned to `assignee` (Wren) when given.
pub(crate) fn tracker_issue(ready: bool, assignee: Option<&str>) -> serde_json::Value {
    let mut value: serde_json::Value = serde_json::from_str(r#"{"id":"issue-2","identifier":"NAV-2","projectId":"project-nav","projectKey":"NAV",
        "type":"issue","title":"Course correction preview","summary":"Preview the course.",
        "state":{"id":"state-1","name":"Todo","type":"unstarted"},
        "assignee":null,
        "labels":[],"priority":2,"estimate":null,"dueDate":null,"milestoneId":null,
        "parentId":null,"blocked":false,"archivedAt":null,"createdAt":"2026-11-01T12:00:00Z",
        "updatedAt":"2026-11-01T12:00:00Z","version":4,"progress":null,"commentCount":0,
        "todoCounts":{"total":0,"done":0},"lastActivityAt":"2026-11-16T12:00:00Z","lastActivity":null,
        "cycle":null,"relayedCrew":null,"provenance":null,"readyForAgents":false,
        "description":null,"todos":[],"createdBy":null,"startedAt":null,"completedAt":null,
        "canceledAt":null,"archivedBy":null,"parent":null,"duplicateOf":null,
        "children":{"items":[],"nextCursor":null},"relations":{"items":[],"nextCursor":null},"mentions":[]}"#).unwrap();
    value["readyForAgents"] = serde_json::json!(ready);
    if let Some(id) = assignee {
        value["assignee"] = serde_json::json!({"userId":id,"displayName":"Wren","isFormerMember":false,"kind":"agent","agent":null});
    }
    value
}
