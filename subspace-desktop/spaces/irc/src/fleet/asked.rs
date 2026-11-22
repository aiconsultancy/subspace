//! Whom `/ask` addresses in a fleet's channel.

use super::{ChannelKind, Rank, Roster};

impl Roster {
    /// Whom `/ask` addresses in `channel`: a ship's captain, or the rear
    /// admiral whose Starbase it is. `None` elsewhere, the command channel
    /// included, where no one agent answers for the channel; `None` too in a
    /// channel two fleets claim, which the navigator shows in neither fleet,
    /// and in a Starbase two rear admirals name, as nothing is guessed. The
    /// completion list and the command both ask this.
    pub(crate) fn asked_in(&self, channel: &str) -> Option<&str> {
        if !self.owned_once(channel) {
            return None;
        }
        match self.channel(channel)? {
            ChannelKind::Ship { captain } => Some(captain),
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
