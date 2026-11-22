//! List groups (networks) and rows (buffers) built from the state.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui_kit::SharedString;
use subspace_client_irc::{BufferKey, ConnectionStatus, ListedChannel, NetworkId, Target};
use subspace_framework::dashboard::{Glyph, Group, ItemSearch, Row, SearchDoc, Tone};

use crate::fleet::{NO_ROSTER, Roster};
use crate::live::Lives;
use crate::state::{Buffer, IrcState};

pub(crate) mod activity;
mod arrange;
mod groups;
mod sections;

pub(crate) use activity::{Activity, Presence};

/// Separates the parts of a row key; it cannot appear in a nick or channel.
const SEPARATOR: char = '\u{1f}';
/// Counts above this show as `99+`.
const MAX_COUNT: usize = 99;
/// The kind part of a listed channel's row key.
const LISTED: &str = "l";
/// The mark of a row whose unread lines mention us.
pub(crate) const MENTION: &str = "@";

/// Each network's fleet roles, as the hub caches them.
pub(crate) type Rosters = HashMap<NetworkId, Rc<Roster>>;

/// The rows the list shows, the buffer each row key names and, apart, the
/// channels from the server's list that we are not in.
#[derive(Debug, Default)]
pub(crate) struct Layout {
    pub groups: Vec<Group>,
    pub buffers: HashMap<SharedString, BufferKey>,
    pub listed: HashMap<SharedString, BufferKey>,
    /// The network each group header stands for.
    pub networks: HashMap<SharedString, NetworkId>,
    /// The fleet groups, with their fleet's id.
    pub fleets: HashMap<SharedString, String>,
    /// Each ship row's activity, for its dot.
    pub ships: HashMap<SharedString, Activity>,
    /// Each row in `People`, with that person's presence.
    pub people: HashMap<SharedString, Presence>,
    /// The quiet captions inside each fleet group, by group key: each the
    /// row it stands above and its words.
    pub captions: HashMap<SharedString, Vec<(SharedString, &'static str)>>,
    /// Each network's name, for search.
    pub network_names: HashMap<NetworkId, SharedString>,
}

/// The navigator: on each network, one group per fleet holding its
/// channels, then `People` with the private conversations with people,
/// then the network's own group with its server, the channels of no fleet,
/// any older private conversation with an agent and the listed channels.
/// A network that serves no fleet keeps one group, as before.
/// A ship's dot follows its crew where `lives`, the Fleet API's word on
/// each fleet the board shows, has them.
pub(crate) fn layout(state: &IrcState, rosters: &Rosters, lives: &Lives) -> Layout {
    let mut layout = Layout::default();
    for network in state.networks() {
        let roster = roster_of(rosters, &network.id);
        groups::network(state, network, (roster, lives), &mut layout);
    }
    layout
}

/// `network`'s roster, or the empty one when it serves no fleet.
fn roster_of<'a>(rosters: &'a Rosters, network: &NetworkId) -> &'a Roster {
    rosters.get(network).map_or(&*NO_ROSTER, Rc::as_ref)
}

/// The channels from `network`'s list that are not in the list already,
/// greyed, by name, after its buffers.
pub(super) fn listed_rows(
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

/// What `/` and the palette search: every buffer by its full name, under
/// its group and its network, with its kind, so `#rust`, `libera carol` or
/// `aurora lyra` finds it, also once it moved into a fleet's group or
/// `People`.
pub(crate) fn search(layout: &Layout) -> ItemSearch {
    let networks = &layout.network_names;
    ItemSearch::new(layout.groups.iter().flat_map(move |group| {
        let parent = search_parent(
            &group.name,
            layout
                .networks
                .get(&group.key)
                .and_then(|id| networks.get(id)),
        );
        group.rows.iter().map(move |row| {
            let name = layout
                .buffers
                .get(&row.key)
                .map_or_else(|| row.title.to_string(), |key| title(key).to_owned());
            let doc = SearchDoc::new(row.key.to_string(), name)
                .parent(parent.clone())
                .meta(kind_name(&row.key));
            (doc, row.clone())
        })
    }))
}

/// A search document's parent: its group, and its network when the group
/// is a fleet's or `People`.
fn search_parent(group: &str, network: Option<&SharedString>) -> String {
    match network {
        Some(network) if network.as_ref() != group => format!("{group} · {network}"),
        _ => group.to_owned(),
    }
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
pub(super) fn order(key: &BufferKey) -> (u8, String) {
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

/// A fleet's group on `network`, apart from the network's own.
pub(crate) fn fleet_group_key(network: &NetworkId, fleet: &str) -> SharedString {
    format!("irc{SEPARATOR}{}{SEPARATOR}f{SEPARATOR}{fleet}", network.0).into()
}

/// The `People` group on `network`.
pub(crate) fn people_group_key(network: &NetworkId) -> SharedString {
    format!("irc{SEPARATOR}{}{SEPARATOR}p", network.0).into()
}

/// The buffer's display name: the channel or nick, or `Server`.
pub(crate) fn title(key: &BufferKey) -> &str {
    match &key.target {
        Target::Server => "Server",
        Target::Channel(name) => name,
        Target::Query(nick) => nick,
    }
}

pub(super) fn row(key: &BufferKey, buffer: &Buffer) -> Row {
    Row {
        key: row_key(key),
        icon: Glyph::new(kind_glyph(&key.target), Tone::Muted),
        id: None,
        title: title(key).to_owned().into(),
        trailing: count(buffer),
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

/// The unread count, in the accent tone when it includes a mention, after
/// a distinct `@` mark for the mention.
fn count(buffer: &Buffer) -> Vec<Glyph> {
    if buffer.unread == 0 {
        return Vec::new();
    }
    let text = if buffer.unread > MAX_COUNT {
        format!("{MAX_COUNT}+")
    } else {
        buffer.unread.to_string()
    };
    if buffer.highlights > 0 {
        return vec![
            Glyph::new(MENTION, Tone::Red),
            Glyph::new(text, Tone::Accent),
        ];
    }
    vec![Glyph::new(text, Tone::Muted)]
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

pub(super) fn status_tone(status: &ConnectionStatus) -> Tone {
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
