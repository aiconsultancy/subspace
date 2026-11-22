//! Fleet roles in Channels: which nick is a captain or the rear admiral, and
//! which channel is a fleet's Starbase, its command channel or a captain's
//! ship, read from the fleets of the initiatives catalogue
//! (`Fleet::slug`, `rear_admiral_name` and `captain_names`).
//!
//! Fleet IRC servers tie each nick to the account of the same name, and the
//! installer names an agent's account `<slug>-<id>`, where the id is the
//! agent's name in lower case with every other run of characters a hyphen:
//! captain `Lyra` of fleet `aurora` is `aurora-lyra`. A name already written
//! that way, such as `aurora-lyra`, is the nick itself. The installer's
//! channels are `#<slug>-starbase`, `#<slug>-command` and, for each
//! captain's ship, `#<slug>-<id>`.
//!
//! Nothing is guessed. A network that serves no fleet, a fleet that names its
//! agents or channels some other way, and a nick or channel that two roles
//! claim are all unknown, and draw as a plain person or channel.

use subspace_api_client::model::Fleet;
use subspace_framework::dashboard::Tone;

/// A fleet agent's rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rank {
    Captain,
    RearAdmiral,
}

impl Rank {
    /// The quiet note beside a member, such as in the member list.
    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::Captain => "captain",
            Self::RearAdmiral => "rear admiral",
        }
    }

    /// The mark after the agent's name in the transcript.
    pub(crate) fn mark(self) -> &'static str {
        match self {
            Self::Captain => "CAPTAIN",
            Self::RearAdmiral => "REAR ADMIRAL",
        }
    }

    /// The tone of the agent's name: captains blue, the rear admiral the
    /// accent.
    pub(crate) fn tone(self) -> Tone {
        match self {
            Self::Captain => Tone::Blue,
            Self::RearAdmiral => Tone::Accent,
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

/// The fleet roles known on one network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Roster {
    /// Agents by nick in lower case.
    agents: Vec<(String, Rank)>,
    /// Channels by name in lower case.
    channels: Vec<(String, ChannelKind)>,
}

impl Roster {
    /// The roles of `fleets`, the fleets whose IRC server the network is.
    /// Pass none for a network that serves no fleet.
    pub(crate) fn new<'a>(fleets: impl IntoIterator<Item = &'a Fleet>) -> Self {
        let (agents, channels) = fleets.into_iter().filter_map(Names::of).fold(
            (Vec::new(), Vec::new()),
            |(mut agents, mut channels), names| {
                agents.extend(names.agents);
                channels.extend(names.channels);
                (agents, channels)
            },
        );
        Self {
            agents: unclaimed_twice(agents),
            channels: unclaimed_twice(channels),
        }
    }

    /// `nick`'s rank, when exactly one agent of the network's fleets has it.
    pub(crate) fn rank(&self, nick: &str) -> Option<Rank> {
        found(&self.agents, nick).copied()
    }

    /// What `channel` is for, when the network's fleets name it so.
    pub(crate) fn channel(&self, channel: &str) -> Option<&ChannelKind> {
        found(&self.channels, channel)
    }
}

/// One fleet's agents and channels, before clashes between fleets are
/// removed.
struct Names {
    agents: Vec<(String, Rank)>,
    channels: Vec<(String, ChannelKind)>,
}

impl Names {
    /// The default names of `fleet`'s agents and channels; `None` for a
    /// fleet without a usable slug.
    fn of(fleet: &Fleet) -> Option<Self> {
        let slug = slug(&fleet.slug)?;
        let rear_admiral = fleet
            .rear_admiral_name
            .as_deref()
            .and_then(|name| nick(&slug, name))
            .map(|nick| (nick, Rank::RearAdmiral));
        let captains: Vec<String> = fleet
            .captain_names
            .iter()
            .filter_map(|name| nick(&slug, name))
            .collect();
        let ships = captains.iter().map(|captain| {
            let kind = ChannelKind::Ship {
                captain: captain.clone(),
            };
            (format!("#{captain}"), kind)
        });
        let channels = [
            (format!("#{slug}-starbase"), ChannelKind::Starbase),
            (format!("#{slug}-command"), ChannelKind::Command),
        ]
        .into_iter()
        .chain(ships)
        .collect();
        let agents = rear_admiral
            .into_iter()
            .chain(captains.into_iter().map(|nick| (nick, Rank::Captain)))
            .collect();
        Some(Self { agents, channels })
    }
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
    let names: Vec<String> = entries.iter().map(|(name, _)| name.clone()).collect();
    entries
        .into_iter()
        .filter(|(name, _)| names.iter().filter(|other| *other == name).count() == 1)
        .collect()
}

/// A fleet slug as the installer writes it: lower-case letters, digits and
/// hyphens.
fn slug(slug: &str) -> Option<String> {
    let valid = !slug.is_empty()
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    valid.then(|| slug.to_owned())
}

/// An agent's nick from its name: the name itself when it is already
/// written as `<slug>-<id>`, else the installer's account `<slug>-<id>`.
fn nick(slug: &str, name: &str) -> Option<String> {
    let id = ident(name)?;
    let prefixed = id
        .strip_prefix(slug)
        .and_then(|rest| rest.strip_prefix('-'))
        .is_some_and(|rest| !rest.is_empty());
    Some(if prefixed { id } else { format!("{slug}-{id}") })
}

/// The installer's id for a name: lower case, each run of other characters
/// one hyphen, none at either end. It starts with a letter.
fn ident(name: &str) -> Option<String> {
    let lower = name.trim().to_ascii_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit()))
        .filter(|word| !word.is_empty())
        .collect();
    let id = words.join("-");
    id.starts_with(|c: char| c.is_ascii_lowercase())
        .then_some(id)
}

#[cfg(test)]
mod tests {
    use subspace_api_client::model::Fleet;

    use super::{ChannelKind, Rank, Roster};

    fn fleet(slug: &str, rear_admiral: Option<&str>, captains: &[&str]) -> Fleet {
        serde_json::from_value(serde_json::json!({
            "id": format!("fleet-{slug}"),
            "slug": slug,
            "name": slug,
            "ircHost": "irc.example.test",
            "ircPort": 6697,
            "ircTls": true,
            "rearAdmiralName": rear_admiral,
            "captainNames": captains,
        }))
        .unwrap()
    }

    fn ship(captain: &str) -> Option<ChannelKind> {
        Some(ChannelKind::Ship {
            captain: captain.to_owned(),
        })
    }

    #[test]
    fn agents_and_channels_follow_the_installer_s_slug_naming() {
        let aurora = fleet("aurora", Some("Admiral"), &["Lyra", "Castor Pollux"]);
        let roster = Roster::new([&aurora]);
        assert_eq!(roster.rank("aurora-lyra"), Some(Rank::Captain));
        assert_eq!(roster.rank("Aurora-Lyra"), Some(Rank::Captain), "any case");
        assert_eq!(roster.rank("aurora-castor-pollux"), Some(Rank::Captain));
        assert_eq!(roster.rank("aurora-admiral"), Some(Rank::RearAdmiral));
        assert_eq!(
            roster.channel("#aurora-starbase"),
            Some(&ChannelKind::Starbase)
        );
        assert_eq!(
            roster.channel("#aurora-command"),
            Some(&ChannelKind::Command)
        );
        assert_eq!(roster.channel("#Aurora-Lyra").cloned(), ship("aurora-lyra"));
        assert_eq!(
            roster.channel("#aurora-castor-pollux").cloned(),
            ship("aurora-castor-pollux")
        );
    }

    #[test]
    fn a_name_already_written_as_the_nick_is_the_nick() {
        let aurora = fleet("aurora", Some("aurora-admiral"), &["aurora-lyra"]);
        let roster = Roster::new([&aurora]);
        assert_eq!(roster.rank("aurora-lyra"), Some(Rank::Captain));
        assert_eq!(roster.rank("aurora-aurora-lyra"), None);
        assert_eq!(roster.rank("aurora-admiral"), Some(Rank::RearAdmiral));
        assert_eq!(roster.channel("#aurora-lyra").cloned(), ship("aurora-lyra"));
    }

    #[test]
    fn the_rear_admiral_has_no_ship() {
        let aurora = fleet("aurora", Some("Admiral"), &[]);
        let roster = Roster::new([&aurora]);
        assert_eq!(roster.rank("aurora-admiral"), Some(Rank::RearAdmiral));
        assert_eq!(roster.channel("#aurora-admiral"), None);
        assert_eq!(
            roster.channel("#aurora-starbase"),
            Some(&ChannelKind::Starbase)
        );
        let without = fleet("aurora", None, &["Lyra"]);
        assert_eq!(Roster::new([&without]).rank("aurora-admiral"), None);
    }

    #[test]
    fn a_person_named_like_a_captain_and_a_clash_between_roles_are_unknown() {
        let aurora = fleet("aurora", Some("Lyra"), &["Lyra", "Vega"]);
        let roster = Roster::new([&aurora]);
        assert_eq!(
            roster.rank("lyra"),
            None,
            "a person's nick is not the agent's"
        );
        assert_eq!(roster.rank("mira"), None);
        assert_eq!(
            roster.rank("aurora-lyra"),
            None,
            "the rear admiral and a captain claim the same nick"
        );
        assert_eq!(roster.rank("aurora-vega"), Some(Rank::Captain));
        let starbase = fleet("aurora", None, &["Starbase"]);
        assert_eq!(
            Roster::new([&starbase]).channel("#aurora-starbase"),
            None,
            "a ship named like the Starbase is neither"
        );
        let twice = [
            fleet("aurora", None, &["Lyra"]),
            fleet("aurora", None, &["Lyra"]),
        ];
        assert_eq!(Roster::new(&twice).rank("aurora-lyra"), None);
    }

    #[test]
    fn a_network_that_serves_no_fleet_knows_no_roles() {
        let roster = Roster::new([]);
        assert_eq!(roster, Roster::default());
        assert_eq!(roster.rank("aurora-lyra"), None);
        assert_eq!(roster.channel("#aurora-starbase"), None);
    }

    #[test]
    fn channels_and_names_outside_the_defaults_are_unknown() {
        let apollo = fleet("apollo", Some("Admiral"), &["Phoenix", "42"]);
        let roster = Roster::new([&apollo]);
        for channel in [
            "#apollo-general",
            "#apollo-fleet-command",
            "#phoenix",
            "#apollo-42",
            "#aurora-phoenix",
        ] {
            assert_eq!(roster.channel(channel), None, "{channel}");
        }
        assert_eq!(roster.rank("apollo-42"), None, "an id starts with a letter");
        assert_eq!(
            roster.channel("#apollo-phoenix").cloned(),
            ship("apollo-phoenix")
        );
        let unusable = fleet("Not A Slug", Some("Admiral"), &["Lyra"]);
        assert_eq!(Roster::new([&unusable]), Roster::default());
    }
}
