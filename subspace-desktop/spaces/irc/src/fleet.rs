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
//! Only active agents count: one the API reports as not available, such as
//! a retired captain, has no rank and is no longer a fleet agent here.
//!
//! Nicks and channels compare ignoring ASCII case, as Ergo's `ascii`
//! casemapping and the rest of the Space do. Nothing is guessed: a network
//! that serves no fleet, and a nick or channel that two roles claim, are
//! unknown and draw as a plain person or channel. A crew member's channel is
//! its ship's recorded one, else that of its ship's only active captain.

use std::collections::HashMap;
use std::sync::LazyLock;

use subspace_api_client::model::{AgentRank, Fleet, FleetAgent, MemberInitiative};
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
    /// Whether the API records `channel`, rather than a standard name
    /// standing in for it.
    pub recorded: bool,
}

/// The fleet roles known on one network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Roster {
    /// Agents by case-folded nick.
    agents: HashMap<String, Agent>,
    /// Channels by case-folded name.
    channels: HashMap<String, ChannelKind>,
    /// The fleets on the network and the channels each claims alone, for
    /// the navigator's fleet groups.
    fleets: Vec<FleetPlace>,
    /// How many active rear admirals each Starbase's fleet names, by
    /// case-folded channel, counted before nicks two roles claim are
    /// dropped, so a clash cannot hide that a fleet has two.
    admirals: HashMap<String, usize>,
}

/// The roster of a network that serves no fleet.
pub(crate) static NO_ROSTER: LazyLock<Roster> = LazyLock::new(Roster::default);

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
        let admirals = admirals(&agents);
        let agents = unclaimed_twice(agents);
        let channels = crewed(unclaimed_twice(channels), &agents);
        Self {
            agents,
            channels,
            fleets: Vec::new(),
            admirals,
        }
    }

    /// The roles of the fleets of `initiatives`, the initiatives whose IRC
    /// server the network is, with the channels each fleet and its
    /// initiatives name, for the navigator.
    pub(crate) fn for_initiatives(initiatives: &[&MemberInitiative]) -> Self {
        let fleets = initiatives
            .iter()
            .filter_map(|initiative| initiative.fleet.as_ref());
        Self {
            fleets: places::places(initiatives),
            ..Self::new(fleets)
        }
    }

    /// The fleets on the network, in the catalogue's order.
    pub(crate) fn fleets(&self) -> &[FleetPlace] {
        &self.fleets
    }

    /// The agent whose API nick is `nick`, when exactly one agent of the
    /// network's fleets has it.
    pub(crate) fn agent(&self, nick: &str) -> Option<&Agent> {
        self.agents.get(&nick.to_ascii_lowercase())
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
        self.channels.get(&channel.to_ascii_lowercase())
    }
}

/// One fleet's agents and channels, before clashes between fleets are
/// removed.
struct Names {
    agents: Vec<(String, Agent)>,
    channels: Vec<(String, ChannelKind)>,
}

impl Names {
    /// The active agents with a known nick and the standard channels of
    /// `fleet`.
    fn of(fleet: &Fleet) -> Self {
        let slug = slug(&fleet.slug);
        let starbase = recorded_or(fleet.starbase_channel.as_deref(), slug, "starbase");
        let command = recorded_or(fleet.command_channel.as_deref(), slug, "command");
        let ships = Ships::of(fleet);
        let agents: Vec<(String, Agent)> = fleet
            .agents
            .iter()
            .filter_map(|agent| known(&ships, agent, starbase.clone()))
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
            starbase.map(|(name, _)| (name, ChannelKind::Starbase)),
            command.map(|(name, _)| (name, ChannelKind::Command)),
        ]
        .into_iter()
        .flatten()
        .chain(ships)
        .collect();
        Self { agents, channels }
    }
}

/// A fleet's ships, indexed once: each ship's recorded channel and its
/// active captains, by ship id.
struct Ships<'a> {
    channels: HashMap<&'a str, &'a str>,
    captains: HashMap<&'a str, Vec<&'a FleetAgent>>,
}

impl<'a> Ships<'a> {
    fn of(fleet: &'a Fleet) -> Self {
        let channels = fleet
            .ships
            .iter()
            .filter_map(|ship| Some((ship.id.as_str(), ship.channel.as_deref()?)))
            .collect();
        let mut captains: HashMap<&str, Vec<&FleetAgent>> = HashMap::new();
        for captain in fleet
            .agents
            .iter()
            .filter(|agent| agent.available && agent.rank == AgentRank::Captain)
        {
            if let Some(ship) = captain.ship_id.as_deref() {
                captains.entry(ship).or_default().push(captain);
            }
        }
        Self { channels, captains }
    }

    /// The channel `agent`'s ship records, from the agent's own copy or the
    /// fleet's ships.
    fn recorded(&self, agent: &FleetAgent) -> Option<String> {
        let held = || {
            let ship = agent.ship_id.as_deref()?;
            self.channels.get(ship).map(|channel| (*channel).to_owned())
        };
        nonempty(agent.ship_channel.as_deref()).or_else(held)
    }

    /// The one active captain of `agent`'s ship; `None` when it has none or
    /// several.
    fn captain(&self, agent: &FleetAgent) -> Option<&'a FleetAgent> {
        match self.captains.get(agent.ship_id.as_deref()?)?.as_slice() {
            [captain] => Some(captain),
            _ => None,
        }
    }
}

/// `agent` under its API nick, with its rank and channel; `None` without a
/// nick or a known rank, or once the agent is no longer active.
fn known(ships: &Ships, agent: &FleetAgent, starbase: Option<Place>) -> Option<(String, Agent)> {
    let nick = agent
        .available
        .then_some(agent.irc_nick.as_deref())
        .flatten()
        .map(str::trim)?;
    let rank = Rank::of(agent.rank)?;
    let place = match rank {
        Rank::RearAdmiral => starbase,
        Rank::Captain => ship_channel(ships, agent),
        Rank::Crew => crew_channel(ships, agent),
    };
    let (channel, recorded) =
        place.map_or((None, false), |(name, recorded)| (Some(name), recorded));
    let agent = Agent {
        rank,
        channel,
        recorded,
    };
    (!nick.is_empty()).then(|| (nick.to_owned(), agent))
}

/// The ship channel of `fleet`'s agent `user_id`, by the roster's rule: its
/// ship's recorded channel, else the standard `#<captain nick>` of its
/// ship's one active captain. `None` for a rear admiral or an agent that is
/// not active.
pub(crate) fn ship_channel_of(fleet: &Fleet, user_id: &str) -> Option<String> {
    let agent = fleet
        .agents
        .iter()
        .find(|agent| agent.available && agent.user_id == user_id)?;
    let ships = Ships::of(fleet);
    let place = match Rank::of(agent.rank)? {
        Rank::Captain => ship_channel(&ships, agent),
        Rank::Crew => crew_channel(&ships, agent),
        Rank::RearAdmiral => None,
    };
    place.map(|(name, _)| name)
}

/// A channel, and whether the API records it rather than a standard name
/// standing in.
type Place = (String, bool);

/// A captain's ship channel: the one its ship records, else `#<nick>`.
fn ship_channel(ships: &Ships, captain: &FleetAgent) -> Option<Place> {
    let standard = || nonempty(captain.irc_nick.as_deref()).map(|nick| (format!("#{nick}"), false));
    ships
        .recorded(captain)
        .map(|name| (name, true))
        .or_else(standard)
}

/// A crew member's channel: the one its ship records, else its ship's one
/// captain's.
fn crew_channel(ships: &Ships, crew: &FleetAgent) -> Option<Place> {
    ships
        .recorded(crew)
        .map(|name| (name, true))
        .or_else(|| ship_channel(ships, ships.captain(crew)?))
}

/// `text` trimmed, when anything is left.
fn nonempty(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// The channel a fleet records, else its standard name `#<slug>-<role>`.
fn recorded_or(recorded: Option<&str>, slug: Option<&str>, role: &str) -> Option<Place> {
    let standard = || slug.map(|slug| (format!("#{slug}-{role}"), false));
    nonempty(recorded)
        .map(|name| (name, true))
        .or_else(standard)
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

/// How many rear admirals of `agents` talk in each channel, by
/// case-folded name.
fn admirals(agents: &[(String, Agent)]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    let starbases = agents
        .iter()
        .filter(|(_, agent)| agent.rank == Rank::RearAdmiral)
        .filter_map(|(_, agent)| agent.channel.as_deref());
    for channel in starbases {
        *counts.entry(channel.to_ascii_lowercase()).or_default() += 1;
    }
    counts
}

/// `channels` without a ship whose captain is not one of `agents` as a
/// captain, such as one whose nick two roles claimed.
fn crewed(
    channels: HashMap<String, ChannelKind>,
    agents: &HashMap<String, Agent>,
) -> HashMap<String, ChannelKind> {
    channels
        .into_iter()
        .filter(|(_, kind)| match kind {
            ChannelKind::Ship { captain } => agents
                .get(&captain.to_ascii_lowercase())
                .is_some_and(|agent| agent.rank == Rank::Captain),
            ChannelKind::Starbase | ChannelKind::Command => true,
        })
        .collect()
}

/// `entries` by case-folded name, without every name that more than one
/// entry claims, so a clash is unknown rather than either role.
fn unclaimed_twice<T>(entries: Vec<(String, T)>) -> HashMap<String, T> {
    let mut claims: HashMap<String, usize> = HashMap::new();
    for (name, _) in &entries {
        *claims.entry(name.to_ascii_lowercase()).or_default() += 1;
    }
    entries
        .into_iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .filter(|(name, _)| claims.get(name) == Some(&1))
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

mod asked;
mod places;

pub(crate) use places::{FleetPlace, short_name};

#[cfg(test)]
pub(crate) mod tests;
