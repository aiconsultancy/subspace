//! Which navigator group each of a network's buffers belongs to: one of its
//! fleets', `People`, or the network's own.

use subspace_client_irc::{BufferKey, Target};

use crate::fleet::{ChannelKind, FleetPlace, Roster, short_name};
use crate::state::{Buffer, IrcState, Network};

/// A buffer and its state, as the state holds them.
pub(super) type Held<'a> = (&'a BufferKey, &'a Buffer);

/// A network's buffers by navigator group.
pub(super) struct Sections<'a> {
    /// Each fleet's channels, in the roster's fleet order: the Starbase, the
    /// command channel, the ships, then the rest, each by name.
    pub fleets: Vec<Vec<Held<'a>>>,
    /// Private conversations with people, on a network that serves a fleet.
    pub people: Vec<Held<'a>>,
    /// The server, the channels of no fleet and, on a network that serves
    /// no fleet, every private conversation; on one that does, those with
    /// an agent, which stay where they were.
    pub rest: Vec<Held<'a>>,
}

impl<'a> Sections<'a> {
    /// `network`'s buffers in `state`, sorted into groups by `roster`.
    pub(super) fn of(state: &'a IrcState, network: &Network, roster: &Roster) -> Self {
        let mut held: Vec<Held<'a>> = state
            .buffers()
            .filter(|(key, _)| key.network == network.id)
            .collect();
        held.sort_by_cached_key(|(key, _)| super::order(key));
        let mut sections = Self {
            fleets: vec![Vec::new(); roster.fleets().len()],
            people: Vec::new(),
            rest: Vec::new(),
        };
        for entry in held {
            sections.place(entry, roster);
        }
        for (fleet, rows) in roster.fleets().iter().zip(&mut sections.fleets) {
            rows.sort_by_cached_key(|(key, _)| fleet_order(key, fleet, roster));
        }
        sections
    }

    fn place(&mut self, entry: Held<'a>, roster: &Roster) {
        let fleets = roster.fleets();
        let section = match &entry.0.target {
            Target::Channel(name) => fleets
                .iter()
                .position(|fleet| fleet.holds(name))
                .and_then(|index| self.fleets.get_mut(index)),
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
    Ship,
    Other,
}

/// A fleet channel's place in its group: the Starbase, the command
/// channel, the ships, then the rest, each by the name the row shows.
fn fleet_order(key: &BufferKey, fleet: &FleetPlace, roster: &Roster) -> (Place, String) {
    let name = super::title(key);
    let place = match roster.channel(name) {
        Some(ChannelKind::Starbase) => Place::Starbase,
        Some(ChannelKind::Command) => Place::Command,
        Some(ChannelKind::Ship { .. }) => Place::Ship,
        None => Place::Other,
    };
    (
        place,
        short_name(fleet.slug.as_deref(), name).to_ascii_lowercase(),
    )
}
