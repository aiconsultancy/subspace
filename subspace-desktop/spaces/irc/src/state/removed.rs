//! What the user removed from the list: private conversations they closed
//! and channels they left. The board keeps it in the state file, so a
//! removed item stays out of the list after a restart, a reconnect or a
//! history replay. Names match ignoring ASCII case, because the runtime may
//! spell a nick or channel differently after a restart. They do not use RFC
//! 1459 folding or Unicode case: on a network with ASCII case mapping,
//! `#a[1]` and `#a{1}` are two channels, and removing one must not hide the
//! other.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use subspace_client_irc::{BufferKey, Event, Line, LineKind, NetworkId, Target};

/// The state-file key.
pub(crate) const KEY: &str = "irc.removed";

/// One removed channel or private conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Removal {
    pub buffer: BufferKey,
    /// The server time of the newest line it held when it was removed. Only
    /// a newer message brings a private conversation back, so replayed
    /// history does not. `None` when it held no line: then any message
    /// brings it back, as the local clock cannot be compared with the
    /// server's.
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
}

/// Every removal, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Removed(Vec<Removal>);

impl Removed {
    /// The stored removals; none when there are none or they cannot be read.
    pub fn decode(text: Option<&str>) -> Self {
        text.and_then(|text| serde_json::from_str(text).ok())
            .map(Self)
            .unwrap_or_default()
    }

    pub fn encode(&self) -> String {
        serde_json::to_string(&self.0).unwrap_or_default()
    }

    /// Remembers `buffer` as removed after its newest line at `since`. The
    /// server buffer stays.
    pub fn insert(&mut self, buffer: &BufferKey, since: Option<DateTime<Utc>>) {
        if buffer.target == Target::Server {
            return;
        }
        self.0.retain(|removal| !same(&removal.buffer, buffer));
        self.0.push(Removal {
            buffer: buffer.clone(),
            since,
        });
    }

    /// Forgets `buffer`'s removal. True when it was removed.
    pub fn forget(&mut self, buffer: &BufferKey) -> bool {
        let before = self.0.len();
        self.0.retain(|removal| !same(&removal.buffer, buffer));
        self.0.len() != before
    }

    /// The removal of `buffer`, by any spelling.
    pub fn get(&self, buffer: &BufferKey) -> Option<&Removal> {
        self.0.iter().find(|removal| same(&removal.buffer, buffer))
    }

    /// Whether `event` reaches the state. Events for a removed buffer are
    /// dropped, except those that bring it back, which forget the removal:
    /// our own join of the channel or opening of the private conversation,
    /// our own join line, and a message in a private conversation newer than
    /// its removal.
    pub fn admit(&mut self, event: &Event) -> bool {
        let Some(buffer) = buffer_of(event) else {
            return true;
        };
        let Some(removal) = self.get(buffer) else {
            return true;
        };
        let returns = match event {
            Event::Joined { .. } => true,
            Event::Line(line) => brings_back(line, removal.since),
            _ => false,
        };
        if returns {
            self.forget(buffer);
        }
        returns
    }

    /// Whether `channel`, an entry of a network's `channels` setting such as
    /// `#ops` or `#ops key`, names a channel the user left on `network`.
    pub fn left(&self, network: &NetworkId, channel: &str) -> bool {
        let name = channel.split_whitespace().next().unwrap_or_default();
        self.0.iter().any(|removal| {
            removal.buffer.network == *network
                && matches!(&removal.buffer.target, Target::Channel(left) if equal(left, name))
        })
    }
}

/// Our own join brings a channel back; a message newer than `since` brings
/// a private conversation back. Replayed history, membership and status
/// lines do not.
fn brings_back(line: &Line, since: Option<DateTime<Utc>>) -> bool {
    match line.buffer.target {
        Target::Channel(_) => line.kind == LineKind::Join && line.own,
        Target::Query(_) => {
            super::is_message(line.kind) && since.is_none_or(|since| line.time > since)
        }
        Target::Server => true,
    }
}

/// The buffer an event changes, if it changes one.
fn buffer_of(event: &Event) -> Option<&BufferKey> {
    match event {
        Event::Line(line) | Event::Replayed(line) => Some(&line.buffer),
        Event::History { buffer, .. }
        | Event::Backfill { buffer, .. }
        | Event::Joined { buffer }
        | Event::Left { buffer }
        | Event::Topic { buffer, .. }
        | Event::Members { buffer, .. }
        | Event::MemberJoined { buffer, .. }
        | Event::MemberLeft { buffer, .. }
        | Event::MemberChanged { buffer, .. }
        | Event::Typing { buffer, .. }
        | Event::ReadMarker { buffer, .. } => Some(buffer),
        Event::Status { .. }
        | Event::AutojoinRetired(_)
        | Event::NickStatus { .. }
        | Event::Capabilities { .. }
        | Event::OwnNick { .. }
        | Event::Sent { .. }
        | Event::ChannelList { .. } => None,
    }
}

/// The same network and kind, with names equal ignoring case.
pub(super) fn same(one: &BufferKey, other: &BufferKey) -> bool {
    one.network == other.network
        && match (&one.target, &other.target) {
            (Target::Channel(one), Target::Channel(other))
            | (Target::Query(one), Target::Query(other)) => equal(one, other),
            (Target::Server, Target::Server) => true,
            _ => false,
        }
}

/// Equal ignoring ASCII case, as every IRC case mapping agrees on that
/// much; Unicode case and RFC 1459 brackets may name different buffers.
fn equal(one: &str, other: &str) -> bool {
    one.eq_ignore_ascii_case(other)
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::{NetworkId, Target};

    use super::Removed;
    use crate::fixtures::{at, channel, network_id, query};

    #[test]
    fn removals_match_any_case_and_round_trip() {
        let mut removed = Removed::default();
        removed.insert(&query("Mira[1]"), Some(at(3)));
        removed.insert(&channel("#Ops"), None);
        assert_eq!(
            removed.get(&query("MIRA[1]")).map(|found| found.since),
            Some(Some(at(3))),
            "case is ignored"
        );
        assert!(
            removed.get(&query("mira{1}")).is_none(),
            "brackets are not folded: another nick on an ASCII network"
        );
        assert!(removed.get(&channel("mira[1]")).is_none(), "kinds differ");
        assert!(removed.left(&network_id(), "#ops secret"));
        assert!(!removed.left(&NetworkId("other".into()), "#ops"));
        assert_eq!(Removed::decode(Some(&removed.encode())), removed);
        assert_eq!(Removed::decode(Some("not json")), Removed::default());

        removed.insert(&query("mira[1]"), Some(at(9)));
        assert_eq!(removed.get(&query("Mira[1]")).unwrap().since, Some(at(9)));
        assert!(removed.forget(&query("MIRA[1]")));
        assert!(!removed.forget(&query("mira[1]")), "forgotten once");
    }

    #[test]
    fn the_server_buffer_is_never_removed() {
        let mut removed = Removed::default();
        let server = crate::state::server_key(&network_id());
        removed.insert(&server, None);
        assert!(removed.get(&server).is_none());
        assert_eq!(server.target, Target::Server);
    }
}
