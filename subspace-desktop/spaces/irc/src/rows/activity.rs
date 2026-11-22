//! What a navigator row says about who is at work: a ship's one dot, and a
//! person's presence.

use std::collections::HashMap;

use subspace_client_irc::{Member, NetworkId, Target};

use crate::fleet::ChannelKind;
use crate::live::ship::bounded;
use crate::live::{CrewLine, ShipLive};
use crate::state::{IrcState, Statuses};

/// The dot's word while a ship is working.
pub(crate) const WORKING: &str = "working";
/// The dot's word while it is not.
pub(crate) const IDLE: &str = "idle";

/// The most characters of a crew member's task in the dot's tooltip.
const TOOLTIP_TASK_CHARS: usize = 60;

/// A ship's activity, as its row's dot shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Activity {
    /// The captain's nick, which names the dot.
    pub captain: String,
    /// What is at work, one line each: with live fleet data each working
    /// crew member and their task, else the captain's status while the
    /// server vouches for it.
    pub working: Vec<String>,
    /// How many of the ship's crew are working, when the fleet's Fleet API
    /// says; `None` without live data.
    pub crew: Option<usize>,
}

impl Activity {
    /// Whether anything on the ship is at work: with live data, at least
    /// one of its crew; else the captain's vouched status.
    pub(crate) fn is_working(&self) -> bool {
        self.crew.map_or(!self.working.is_empty(), |busy| busy > 0)
    }

    /// The dot's accessible name, such as `aurora-lyra: working` or, with
    /// live data, `aurora-lyra: 3 crew working`.
    pub(crate) fn label(&self) -> String {
        format!("{}: {}", self.captain, self.state())
    }

    /// The dot's state in words.
    fn state(&self) -> String {
        match self.crew {
            Some(0) => "no crew working".to_owned(),
            Some(busy) => format!("{busy} crew working"),
            None if self.working.is_empty() => IDLE.to_owned(),
            None => WORKING.to_owned(),
        }
    }

    /// The dot's tooltip: with live data, how many crew are working, then
    /// each and what on, a line each; else what is at work, or that
    /// nothing is.
    pub(crate) fn tooltip(&self) -> String {
        match (self.crew, self.working.is_empty()) {
            (Some(0), _) => "No crew working".to_owned(),
            (Some(busy), _) => std::iter::once(format!("{busy} crew working"))
                .chain(self.working.iter().cloned())
                .collect::<Vec<_>>()
                .join("\n"),
            (None, true) => format!("{}: {IDLE}", self.captain),
            (None, false) => self.working.join("\n"),
        }
    }
}

/// Whether a ship is working: the one source of its dot. With live fleet
/// data (`live`, the Fleet API's ship matched to the roster captain, when
/// it counts its crew) the dot follows the crew: working while at least one
/// of them has a task, and the tooltip names each. Without it, the
/// captain's nick status, counted only while the server vouches for it
/// (`conversation::vouched_status`, the heading pill's rule), so nothing is
/// invented without one.
pub(crate) fn ship(
    captain: &str,
    statuses: &Statuses,
    members: &[Member],
    live: Option<&ShipLive>,
) -> Activity {
    match live.and_then(crew_at_work) {
        Some((busy, working)) => Activity {
            captain: captain.to_owned(),
            working,
            crew: Some(busy),
        },
        None => captain_status(captain, statuses, members),
    }
}

/// How many of `ship`'s crew are working and a line for each, when the
/// Fleet API counts the crew.
fn crew_at_work(ship: &ShipLive) -> Option<(usize, Vec<String>)> {
    let (busy, _) = ship.crew?;
    let lines = ship.working().map(crew_line).collect();
    Some((busy, lines))
}

/// `Wren · Cancel tile decodes…`: a working crew member and their task.
fn crew_line(member: &CrewLine) -> String {
    match &member.task {
        Some(task) => format!("{} · {}", member.name, bounded(task, TOOLTIP_TASK_CHARS)),
        None => member.name.clone(),
    }
}

/// The ship's activity from its captain's vouched status alone.
fn captain_status(captain: &str, statuses: &Statuses, members: &[Member]) -> Activity {
    let status = crate::conversation::vouched_status(captain, statuses, members);
    Activity {
        captain: captain.to_owned(),
        working: status.into_iter().map(str::to_owned).collect(),
        crew: None,
    }
}

/// A ship channel's captain; `None` for the Starbase and the command
/// channel.
pub(crate) fn captain(kind: &ChannelKind) -> Option<&str> {
    match kind {
        ChannelKind::Ship { captain } => Some(captain),
        ChannelKind::Starbase | ChannelKind::Command => None,
    }
}

/// A person's presence, as their row's mark shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presence {
    /// In a channel we share, and not away.
    Here,
    /// In a channel we share, and away.
    Away,
    /// In no channel we share, so the Space cannot tell.
    Unseen,
}

impl Presence {
    /// How the row's spoken name says it.
    pub(crate) fn spoken(self) -> &'static str {
        match self {
            Self::Here => "here",
            Self::Away => "away",
            Self::Unseen => "not in a shared channel",
        }
    }
}

/// Everyone's presence on `network`, from the members of the channels we
/// are in there, by nick in ASCII lower case: built once per network, so
/// the People rows look it up rather than scan every channel each. A nick
/// in several channels takes its first.
pub(crate) fn presences(state: &IrcState, network: &NetworkId) -> HashMap<String, Presence> {
    let mut found = HashMap::new();
    let members = state
        .buffers()
        .filter(|(key, buffer)| {
            &key.network == network && buffer.joined && matches!(key.target, Target::Channel(_))
        })
        .flat_map(|(_, buffer)| &buffer.members);
    for member in members {
        let presence = if member.away {
            Presence::Away
        } else {
            Presence::Here
        };
        found
            .entry(member.nick.to_ascii_lowercase())
            .or_insert(presence);
    }
    found
}

/// `nick`'s presence in `presences`; unseen when it shares no channel.
pub(crate) fn presence(presences: &HashMap<String, Presence>, nick: &str) -> Presence {
    presences
        .get(&nick.to_ascii_lowercase())
        .copied()
        .unwrap_or(Presence::Unseen)
}

/// A row's spoken name followed by what its mark says, if it has one, such
/// as `lyra, 1 unread, aurora-lyra: working`.
pub(crate) fn spoken(base: &str, mark: Option<&str>) -> String {
    match mark {
        Some(mark) => format!("{base}, {mark}"),
        None => base.to_owned(),
    }
}
