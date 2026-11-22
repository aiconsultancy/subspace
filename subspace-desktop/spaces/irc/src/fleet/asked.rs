//! Whom `/ask` addresses in a fleet's channel, and whom an assignment
//! can name.

use super::{Agent, ChannelKind, Rank, Roster};

impl Roster {
    /// The agents of the network's fleets, by case-folded nick, in no
    /// particular order: whom a tracker assignment can name.
    pub(crate) fn agents(&self) -> impl Iterator<Item = (&str, &Agent)> {
        self.agents
            .iter()
            .map(|(nick, agent)| (nick.as_str(), agent))
    }

    /// Whom `/ask` addresses in `channel`: a ship's captain, or the rear
    /// admiral whose Starbase it is. `None` elsewhere, the command channel
    /// included, where no one agent answers for the channel; `None` too in a
    /// channel two fleets claim, which the navigator shows in neither fleet,
    /// and in a Starbase whose fleet names two rear admirals, even when one of
    /// them clashes with another fleet's nick, as nothing is guessed. The
    /// completion list and the command both ask this.
    pub(crate) fn asked_in(&self, channel: &str) -> Option<&str> {
        if !self.owned_once(channel) {
            return None;
        }
        match self.channel(channel)? {
            ChannelKind::Ship { captain } => Some(captain),
            ChannelKind::Starbase if self.admirals_of(channel) != 1 => None,
            ChannelKind::Starbase => only(self.agents.iter().filter(|(_, agent)| {
                agent.rank == Rank::RearAdmiral
                    && agent
                        .channel
                        .as_deref()
                        .is_some_and(|held| held.eq_ignore_ascii_case(channel))
            }))
            .map(|(nick, _)| nick.as_str()),
            ChannelKind::Command => None,
        }
    }

    /// How many rear admirals the fleets name for Starbase `channel`,
    /// before clashing nicks were dropped.
    fn admirals_of(&self, channel: &str) -> usize {
        self.admirals
            .get(&channel.to_ascii_lowercase())
            .copied()
            .unwrap_or_default()
    }

    /// Whether exactly one fleet holds `channel` as the navigator groups
    /// it; true when the roster was built without the fleets' places.
    fn owned_once(&self, channel: &str) -> bool {
        self.fleets.is_empty()
            || self
                .fleets
                .iter()
                .filter(|fleet| fleet.holds(channel))
                .count()
                == 1
    }
}

/// The one item of `items`; `None` for none or several.
fn only<T>(mut items: impl Iterator<Item = T>) -> Option<T> {
    let first = items.next()?;
    items.next().is_none().then_some(first)
}
