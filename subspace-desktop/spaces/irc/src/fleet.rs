//! Fleet roles in Channels: which nick is the rear admiral, a captain or a
//! crew member, and which channel is a fleet's Starbase, its command channel
//! or a captain's ship, read from the fleets of the initiatives catalogue.
//!
//! An agent's rank comes from the Subspace API, never from its name: the
//! fleet's `agents` give each agent's rank and, when the API knows it, its
//! IRC nick (`FleetAgent::irc_nick`), which fleet IRC servers tie to the
//! account of the same name. An agent without a known nick is not marked.
//!
//! A fleet's channels come from its record (`Fleet::starbase_channel`,
//! `command_channel` and each ship's channel). Without one, the installer's
//! standard names stand in: `#<slug>-starbase`, `#<slug>-command` and, for a
//! captain, `#<nick>` (the installer names a captain's nick and ship channel
//! both `<slug>-<id>`).
//!
//! Nicks and channels compare ignoring ASCII case, as Ergo's `ascii`
//! casemapping and the rest of the Space do. Nothing is guessed: a network
//! that serves no fleet, and a nick or channel that two roles claim, are
//! unknown and draw as a plain person or channel.

use std::collections::HashMap;

use subspace_api_client::model::{AgentRank, Fleet, FleetAgent};
use subspace_framework::dashboard::Tone;
use subspace_framework::dashboard::avatar::AgentRank as AvatarRank;

/// A fleet agent's rank, as the API has it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rank {
    RearAdmiral,
    Captain,
    Crew,
}

impl Rank {
    /// The rank of an API agent; `None` for a rank this build does not know.
    fn of(rank: AgentRank) -> Option<Self> {
        match rank {
            AgentRank::RearAdmiral => Some(Self::RearAdmiral),
            AgentRank::Captain => Some(Self::Captain),
            AgentRank::Crew => Some(Self::Crew),
            _ => None,
        }
    }

    /// The quiet note beside a member, such as in the member list.
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::RearAdmiral => "rear admiral",
            Self::Captain => "captain",
            Self::Crew => "crew",
        }
    }

    /// The mark after the agent's name in the transcript.
    pub(crate) fn mark(self) -> &'static str {
        match self {
            Self::RearAdmiral => "REAR ADMIRAL",
            Self::Captain => "CAPTAIN",
            Self::Crew => "CREW",
        }
    }

    /// The tone of the agent's name and mark: the rear admiral the accent,
    /// captains blue; crew keep a person's tone and a muted mark.
    pub(crate) fn tone(self) -> Option<Tone> {
        self.avatar().map(AvatarRank::tone)
    }

    /// The shared agent avatar's rank: the rear admiral and captains have
    /// one; crew keep a person's avatar.
    pub(crate) fn avatar(self) -> Option<AvatarRank> {
        match self {
            Self::RearAdmiral => Some(AvatarRank::RearAdmiral),
            Self::Captain => Some(AvatarRank::Captain),
            Self::Crew => None,
        }
    }
}

/// What a fleet channel is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChannelKind {
    /// People, the rear admiral and the captains.
    Starbase,
    /// The rear admiral and its captains.
    Command,
    /// A captain's ship, named by the captain's nick.
    Ship { captain: String },
}

/// One agent the roster knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Agent {
    pub rank: Rank,
    /// Where the agent talks: a captain's or crew member's ship channel, or
    /// the rear admiral's Starbase. `None` when the fleet names none.
    pub channel: Option<String>,
}

/// The fleet roles known on one network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Roster {
    /// Agents by nick.
    agents: Vec<(String, Agent)>,
    /// Channels by name.
    channels: Vec<(String, ChannelKind)>,
}

impl Roster {
    /// The roles of `fleets`, the fleets whose IRC server the network is.
    /// Pass none for a network that serves no fleet.
    /// A fleet that several initiatives share counts once.
    pub(crate) fn new<'a>(fleets: impl IntoIterator<Item = &'a Fleet>) -> Self {
        let (agents, channels) = distinct(fleets).into_iter().map(Names::of).fold(
            (Vec::new(), Vec::new()),
            |(mut agents, mut channels), names| {
                agents.extend(names.agents);
                channels.extend(names.channels);
                (agents, channels)
            },
        );
        let agents = unclaimed_twice(agents);
        let channels = crewed(unclaimed_twice(channels), &agents);
        Self { agents, channels }
    }

    /// The agent whose API nick is `nick`, when exactly one agent of the
    /// network's fleets has it.
    pub(crate) fn agent(&self, nick: &str) -> Option<&Agent> {
        found(&self.agents, nick)
    }

    /// `nick`'s rank, when exactly one agent of the network's fleets has it.
    pub(crate) fn rank(&self, nick: &str) -> Option<Rank> {
        self.agent(nick).map(|agent| agent.rank)
    }

    /// `nick`'s rank when the server vouches for it: its `account` is the
    /// nick, ignoring ASCII case, as fleet servers enforce and as the Space's
    /// progress trust requires. A sender or member without an account is
    /// unknown, so a nick alone never earns a rank.
    pub(crate) fn verified(&self, nick: &str, account: Option<&str>) -> Option<Rank> {
        account
            .filter(|account| account.eq_ignore_ascii_case(nick))
            .and_then(|_| self.rank(nick))
    }

    /// What `channel` is for, when the network's fleets name it so.
    pub(crate) fn channel(&self, channel: &str) -> Option<&ChannelKind> {
        found(&self.channels, channel)
    }
}

/// One fleet's agents and channels, before clashes between fleets are
/// removed.
struct Names {
    agents: Vec<(String, Agent)>,
    channels: Vec<(String, ChannelKind)>,
}

impl Names {
    /// The agents with a known nick and the standard channels of `fleet`.
    fn of(fleet: &Fleet) -> Self {
        let slug = slug(&fleet.slug);
        let starbase = recorded_or(fleet.starbase_channel.as_deref(), slug, "starbase");
        let command = recorded_or(fleet.command_channel.as_deref(), slug, "command");
        let agents: Vec<(String, Agent)> = fleet
            .agents
            .iter()
            .filter_map(|agent| known(fleet, agent, starbase.as_deref()))
            .collect();
        let ships = agents
            .iter()
            .filter(|(_, agent)| agent.rank == Rank::Captain)
            .filter_map(|(nick, agent)| {
                let kind = ChannelKind::Ship {
                    captain: nick.clone(),
                };
                agent.channel.clone().map(|channel| (channel, kind))
            });
        let channels = [
            starbase.map(|name| (name, ChannelKind::Starbase)),
            command.map(|name| (name, ChannelKind::Command)),
        ]
        .into_iter()
        .flatten()
        .chain(ships)
        .collect();
        Self { agents, channels }
    }
}

/// `agent` of `fleet` under its API nick, with its rank and channel; `None`
/// without a nick or a known rank.
fn known(fleet: &Fleet, agent: &FleetAgent, starbase: Option<&str>) -> Option<(String, Agent)> {
    let nick = agent.irc_nick.as_deref().map(str::trim)?;
    let rank = Rank::of(agent.rank)?;
    let channel = match rank {
        Rank::RearAdmiral => starbase.map(str::to_owned),
        Rank::Captain => ship_channel(fleet, agent),
        Rank::Crew => captain_of(fleet, agent).and_then(|captain| ship_channel(fleet, captain)),
    };
    (!nick.is_empty()).then(|| (nick.to_owned(), Agent { rank, channel }))
}

/// A captain's ship channel: the one its ship records, else `#<nick>`.
fn ship_channel(fleet: &Fleet, captain: &FleetAgent) -> Option<String> {
    let recorded = captain.ship_channel.clone().or_else(|| {
        let ship = captain.ship_id.as_deref()?;
        fleet
            .ships
            .iter()
            .find(|held| held.id == ship)
            .and_then(|held| held.channel.clone())
    });
    let nick = captain.irc_nick.as_deref().map(str::trim);
    recorded.or_else(|| {
        nick.filter(|nick| !nick.is_empty())
            .map(|nick| format!("#{nick}"))
    })
}

/// The captain of the ship a crew member serves on.
fn captain_of<'a>(fleet: &'a Fleet, crew: &FleetAgent) -> Option<&'a FleetAgent> {
    let ship = crew.ship_id.as_deref()?;
    fleet
        .agents
        .iter()
        .find(|agent| agent.rank == AgentRank::Captain && agent.ship_id.as_deref() == Some(ship))
}

/// The channel a fleet records, else its standard name `#<slug>-<role>`.
fn recorded_or(recorded: Option<&str>, slug: Option<&str>, role: &str) -> Option<String> {
    let recorded = recorded.map(str::trim).filter(|name| !name.is_empty());
    recorded
        .map(str::to_owned)
        .or_else(|| slug.map(|slug| format!("#{slug}-{role}")))
}

/// `fleets` with each fleet once, by its id, in their first order: two
/// initiatives of one fleet bring the same fleet twice, which is no clash.
fn distinct<'a>(fleets: impl IntoIterator<Item = &'a Fleet>) -> Vec<&'a Fleet> {
    let mut seen = std::collections::HashSet::new();
    fleets
        .into_iter()
        .filter(|fleet| seen.insert(fleet.id.as_str()))
        .collect()
}

/// `channels` without a ship whose captain is not one of `agents` as a
/// captain, such as one whose nick two roles claimed.
fn crewed(
    channels: Vec<(String, ChannelKind)>,
    agents: &[(String, Agent)],
) -> Vec<(String, ChannelKind)> {
    channels
        .into_iter()
        .filter(|(_, kind)| match kind {
            ChannelKind::Ship { captain } => {
                found(agents, captain).is_some_and(|agent| agent.rank == Rank::Captain)
            }
            ChannelKind::Starbase | ChannelKind::Command => true,
        })
        .collect()
}

/// The entry named `name`, ignoring ASCII case.
fn found<'a, T>(entries: &'a [(String, T)], name: &str) -> Option<&'a T> {
    entries
        .iter()
        .find(|(held, _)| held.eq_ignore_ascii_case(name))
        .map(|(_, value)| value)
}

/// `entries` without every name that more than one entry claims, so a clash
/// is unknown rather than either role.
fn unclaimed_twice<T>(entries: Vec<(String, T)>) -> Vec<(String, T)> {
    let mut claims: HashMap<String, usize> = HashMap::new();
    for (name, _) in &entries {
        *claims.entry(name.to_ascii_lowercase()).or_default() += 1;
    }
    entries
        .into_iter()
        .filter(|(name, _)| claims.get(&name.to_ascii_lowercase()) == Some(&1))
        .collect()
}

/// A fleet slug as the installer writes it: lower-case letters, digits and
/// hyphens.
fn slug(slug: &str) -> Option<&str> {
    let valid = !slug.is_empty()
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    valid.then_some(slug)
}

#[cfg(test)]
mod tests;
