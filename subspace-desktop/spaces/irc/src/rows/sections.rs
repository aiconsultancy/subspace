//! Which navigator group each of a network's rows belongs to: one of its
//! fleets', `People`, or the network's own.

use std::collections::HashSet;

use subspace_client_irc::{BufferKey, Target};

use crate::fleet::{ChannelKind, FleetPlace, Roster, short_name};
use crate::state::{Buffer, IrcState, Network};

/// A buffer or an unjoined channel from the server's list.
pub(super) enum Entry<'a> {
    Buffer(&'a BufferKey, &'a Buffer),
    Listed(BufferKey, u32),
}

impl Entry<'_> {
    pub(super) fn key(&self) -> &BufferKey {
        match self {
            Self::Buffer(key, _) => key,
            Self::Listed(key, _) => key,
        }
    }
}

/// A network's rows by navigator group.
pub(super) struct Sections<'a> {
    /// Each fleet's channels, in the roster's fleet order: the Starbase, the
    /// command channel, Fleet Audit, the ships, then the rest, each by name.
    pub fleets: Vec<Vec<Entry<'a>>>,
    /// Private conversations with people, on a network that serves a fleet.
    pub people: Vec<Entry<'a>>,
    /// The server, the channels of no fleet and, on a network that serves
    /// no fleet, every private conversation; on one that does, those with
    /// an agent, which stay where they were.
    pub rest: Vec<Entry<'a>>,
}

impl<'a> Sections<'a> {
    /// `network`'s buffers and listed channels, grouped by `roster`.
    pub(super) fn of(state: &'a IrcState, network: &Network, roster: &Roster) -> Self {
        let mut held: Vec<_> = state
            .buffers()
            .filter(|(key, _)| key.network == network.id)
            .collect();
        held.sort_by_cached_key(|(key, _)| super::order(key));
        let mut sections = Self {
            fleets: roster.fleets().iter().map(|_| Vec::new()).collect(),
            people: Vec::new(),
            rest: Vec::new(),
        };
        let listed = super::unjoined(state, &network.id);
        let available: HashSet<_> = held
            .iter()
            .filter_map(|(key, _)| match &key.target {
                Target::Channel(name) => Some(name.to_ascii_lowercase()),
                _ => None,
            })
            .chain(
                listed
                    .iter()
                    .map(|channel| channel.name.to_ascii_lowercase()),
            )
            .collect();
        for (key, buffer) in held {
            sections.place(Entry::Buffer(key, buffer), roster, &available);
        }
        for channel in listed {
            let key = BufferKey {
                network: network.id.clone(),
                target: Target::Channel(channel.name.clone()),
            };
            sections.place(Entry::Listed(key, channel.users), roster, &available);
        }
        for (fleet, rows) in roster.fleets().iter().zip(&mut sections.fleets) {
            rows.sort_by_cached_key(|entry| fleet_order(entry.key(), fleet, roster));
        }
        sections
    }

    fn place(&mut self, entry: Entry<'a>, roster: &Roster, available: &HashSet<String>) {
        let fleets = roster.fleets();
        let section = match &entry.key().target {
            Target::Channel(name) => {
                let owner = fleets.iter().position(|fleet| fleet.holds(name));
                let legacy = || {
                    let mut matches = fleets.iter().enumerate().filter_map(|(index, fleet)| {
                        let canonical = fleet.canonical(name)?;
                        fleet.holds(canonical).then_some((index, canonical))
                    });
                    let owner = matches.next()?;
                    matches.next().is_none().then_some(owner)
                };
                let owner = match (owner, legacy()) {
                    (None, Some((_, canonical))) if available.contains(canonical) => return,
                    (None, Some((index, _))) => Some(index),
                    (owner, _) => owner,
                };
                owner.and_then(|index| self.fleets.get_mut(index))
            }
            Target::Query(nick) if !fleets.is_empty() && roster.agent(nick).is_none() => {
                Some(&mut self.people)
            }
            _ => None,
        };
        section.unwrap_or(&mut self.rest).push(entry);
    }
}

/// Where a fleet channel sorts in its group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Starbase,
    Command,
    Audit,
    Ship,
    Other,
}

/// A fleet channel's place in its group: the Starbase, the command
/// channel, Fleet Audit, the ships, then the rest, each by the name the row shows.
fn fleet_order(key: &BufferKey, fleet: &FleetPlace, roster: &Roster) -> (Place, String) {
    let name = super::title(key);
    let place = match roster.navigator_kind(fleet, name) {
        Some(ChannelKind::Starbase) => Place::Starbase,
        Some(ChannelKind::Command) => Place::Command,
        Some(ChannelKind::Audit) => Place::Audit,
        Some(ChannelKind::Ship { .. }) => Place::Ship,
        None => Place::Other,
    };
    (
        place,
        short_name(fleet.slug.as_deref(), name).to_ascii_lowercase(),
    )
}
