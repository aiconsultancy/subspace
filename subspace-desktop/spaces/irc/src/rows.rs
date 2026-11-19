//! List groups (networks) and rows (buffers) built from the state.

use std::collections::{HashMap, HashSet};

use gpui_kit::SharedString;
use subspace_client_irc::{BufferKey, ConnectionStatus, ListedChannel, NetworkId, Target};
use subspace_framework::dashboard::{Glyph, Group, ItemSearch, Row, SearchDoc, Tone};

use crate::state::{Buffer, IrcState, Network};

/// Separates the parts of a row key; it cannot appear in a nick or channel.
const SEPARATOR: char = '\u{1f}';
/// Counts above this show as `99+`.
const MAX_COUNT: usize = 99;
/// The kind part of a listed channel's row key.
const LISTED: &str = "l";

/// The rows the list shows, the buffer each row key names and, apart, the
/// channels from the server's list that we are not in.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    pub groups: Vec<Group>,
    pub buffers: HashMap<SharedString, BufferKey>,
    pub listed: HashMap<SharedString, BufferKey>,
}

pub(crate) fn layout(state: &IrcState) -> Layout {
    let mut layout = Layout::default();
    for network in state.networks() {
        let mut rows = buffer_rows(state, &network.id, &mut layout.buffers);
        rows.extend(listed_rows(state, &network.id, &mut layout.listed));
        layout.groups.push(group(network, rows));
    }
    layout
}

/// The buffers of `network`: server, channels, then private conversations.
fn buffer_rows(
    state: &IrcState,
    network: &NetworkId,
    keys: &mut HashMap<SharedString, BufferKey>,
) -> Vec<Row> {
    let mut held: Vec<(&BufferKey, &Buffer)> = state
        .buffers()
        .filter(|(key, _)| &key.network == network)
        .collect();
    held.sort_by_cached_key(|(key, _)| order(key));
    held.into_iter()
        .map(|(key, buffer)| {
            let row = row(key, buffer);
            keys.insert(row.key.clone(), key.clone());
            row
        })
        .collect()
}

/// The channels from `network`'s list that are not in the list already,
/// greyed, by name, after its buffers.
fn listed_rows(
    state: &IrcState,
    network: &NetworkId,
    keys: &mut HashMap<SharedString, BufferKey>,
) -> Vec<Row> {
    unjoined(state, network)
        .into_iter()
        .map(|channel| {
            let key = BufferKey {
                network: network.clone(),
                target: Target::Channel(channel.name.clone()),
            };
            let row = listed_row(&key, channel.users);
            keys.insert(row.key.clone(), key);
            row
        })
        .collect()
}

/// The listed channels of `network` that have no buffer, matched ignoring
/// ASCII case as the runtime may spell a buffer differently.
pub(crate) fn unjoined<'a>(state: &'a IrcState, network: &NetworkId) -> Vec<&'a ListedChannel> {
    let held: HashSet<String> = state
        .buffers()
        .filter_map(|(key, _)| match &key.target {
            Target::Channel(name) if &key.network == network => Some(name.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    state
        .directories
        .get(network)
        .map(|directory| {
            directory
                .channels
                .iter()
                .filter(|channel| !held.contains(&channel.name.to_ascii_lowercase()))
                .collect()
        })
        .unwrap_or_default()
}

/// What `/` and the palette search: every buffer by name, under its network,
/// with its kind, so `#rust` or `libera carol` finds it.
pub(crate) fn search(groups: &[Group]) -> ItemSearch {
    ItemSearch::new(groups.iter().flat_map(|group| {
        group.rows.iter().map(move |row| {
            let doc = SearchDoc::new(row.key.to_string(), row.title.to_string())
                .parent(group.name.to_string())
                .meta(kind_name(&row.key));
            (doc, row.clone())
        })
    }))
}

/// `channel`, `conversation` or `server`, from a row key.
fn kind_name(key: &str) -> &'static str {
    let target = key.split(SEPARATOR).nth(2).unwrap_or_default();
    match target {
        "c" => "channel",
        LISTED => "channel not joined",
        "q" => "private conversation",
        _ => "server",
    }
}

/// Server first, then channels, then queries, each by name ignoring case.
fn order(key: &BufferKey) -> (u8, String) {
    match &key.target {
        Target::Server => (0, String::new()),
        Target::Channel(name) => (1, name.to_lowercase()),
        Target::Query(nick) => (2, nick.to_lowercase()),
    }
}

pub(crate) fn row_key(key: &BufferKey) -> SharedString {
    let target = match &key.target {
        Target::Server => "s".to_owned(),
        Target::Channel(name) => format!("c{SEPARATOR}{name}"),
        Target::Query(nick) => format!("q{SEPARATOR}{nick}"),
    };
    format!("irc{SEPARATOR}{}{SEPARATOR}{target}", key.network.0).into()
}

/// The row key of a listed channel we are not in, apart from its buffer's.
pub(crate) fn listed_key(key: &BufferKey) -> SharedString {
    format!(
        "irc{SEPARATOR}{}{SEPARATOR}{LISTED}{SEPARATOR}{}",
        key.network.0,
        title(key)
    )
    .into()
}

pub(crate) fn group_key(network: &NetworkId) -> SharedString {
    format!("irc{SEPARATOR}{}", network.0).into()
}

/// The buffer's display name: the channel or nick, or `Server`.
pub(crate) fn title(key: &BufferKey) -> &str {
    match &key.target {
        Target::Server => "Server",
        Target::Channel(name) => name,
        Target::Query(nick) => nick,
    }
}

fn row(key: &BufferKey, buffer: &Buffer) -> Row {
    Row {
        key: row_key(key),
        icon: Glyph::new(kind_glyph(&key.target), Tone::Muted),
        id: None,
        title: title(key).to_owned().into(),
        trailing: count(buffer).into_iter().collect(),
        done: matches!(key.target, Target::Channel(_)) && !buffer.joined,
        indent: 0,
    }
}

/// A channel we are not in: greyed, with its user count and no unread count.
fn listed_row(key: &BufferKey, users: u32) -> Row {
    Row {
        key: listed_key(key),
        icon: Glyph::new(kind_glyph(&key.target), Tone::Muted),
        id: None,
        title: title(key).to_owned().into(),
        trailing: vec![Glyph::new(users_label(users), Tone::Muted)],
        done: true,
        indent: 0,
    }
}

/// `1 user` or `12 users`.
pub(crate) fn users_label(users: u32) -> String {
    match users {
        1 => "1 user".to_owned(),
        users => format!("{users} users"),
    }
}

/// The text form of a buffer's kind, used where a glyph must be text.
fn kind_glyph(target: &Target) -> &'static str {
    match target {
        Target::Server => "≡",
        Target::Channel(_) => "#",
        Target::Query(_) => "@",
    }
}

/// The unread count, in the accent tone when it includes a mention.
fn count(buffer: &Buffer) -> Option<Glyph> {
    if buffer.unread == 0 {
        return None;
    }
    let text = if buffer.unread > MAX_COUNT {
        format!("{MAX_COUNT}+")
    } else {
        buffer.unread.to_string()
    };
    let tone = if buffer.highlights > 0 {
        Tone::Accent
    } else {
        Tone::Muted
    };
    Some(Glyph::new(text, tone))
}

fn group(network: &Network, rows: Vec<Row>) -> Group {
    Group {
        key: group_key(&network.id),
        icon: Glyph::new("●", status_tone(&network.status)),
        name: network.name.clone().into(),
        count: Some(status_label(&network.status).into()),
        rows,
    }
}

/// A short connection state for the group header and the status bar.
pub(crate) fn status_label(status: &ConnectionStatus) -> String {
    match status {
        ConnectionStatus::Disconnected => "Disconnected".into(),
        ConnectionStatus::Connecting => "Connecting…".into(),
        ConnectionStatus::Registering => "Signing in…".into(),
        ConnectionStatus::Connected { .. } => "Connected".into(),
        ConnectionStatus::Reconnecting { delay_secs, .. } => {
            format!("Reconnecting in {delay_secs} s")
        }
        ConnectionStatus::Failed { .. } => "Failed".into(),
    }
}

fn status_tone(status: &ConnectionStatus) -> Tone {
    match status {
        ConnectionStatus::Connected { .. } => Tone::Green,
        ConnectionStatus::Connecting
        | ConnectionStatus::Registering
        | ConnectionStatus::Reconnecting { .. } => Tone::Yellow,
        ConnectionStatus::Failed { .. } => Tone::Red,
        ConnectionStatus::Disconnected => Tone::Muted,
    }
}

/// A listed channel's spoken name: not joined, with its user count.
pub(crate) fn listed_accessible_label(row: &Row) -> SharedString {
    let users = row.trailing.first().map_or("", |glyph| &glyph.text);
    format!("{}, not joined, {users}", row.title).into()
}

/// The row's spoken name, with its unread state.
pub(crate) fn accessible_label(title: &str, buffer: &Buffer) -> SharedString {
    match (buffer.unread, buffer.highlights) {
        (0, _) => title.to_owned().into(),
        (unread, 0) => format!("{title}, {unread} unread").into(),
        (unread, mentions) => format!("{title}, {unread} unread, {mentions} mentioning you").into(),
    }
}

#[cfg(test)]
mod tests;
