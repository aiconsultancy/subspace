//! What a navigator row says about who is at work: a ship's one dot, and a
//! person's presence.

use std::collections::HashMap;

use subspace_client_irc::{Member, NetworkId, Target};

use crate::fleet::ChannelKind;
use crate::state::{IrcState, Statuses};

/// The dot's word while a ship is working.
pub(crate) const WORKING: &str = "working";
/// The dot's word while it is not.
pub(crate) const IDLE: &str = "idle";

/// A ship's activity, as its row's dot shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Activity {
    /// The captain's nick, which names the dot.
    pub captain: String,
    /// What is at work, one line each: today the captain's status while the
    /// server vouches for it. Crew activity joins it here.
    pub working: Vec<String>,
}

impl Activity {
    /// Whether anything on the ship is at work.
    pub(crate) fn is_working(&self) -> bool {
        !self.working.is_empty()
    }

    /// The dot's accessible name, such as `aurora-lyra: working`.
    pub(crate) fn label(&self) -> String {
        let state = if self.working.is_empty() {
            IDLE
        } else {
            WORKING
        };
        format!("{}: {state}", self.captain)
    }

    /// The dot's tooltip: what is at work, a line each, else that nothing
    /// is.
    pub(crate) fn tooltip(&self) -> String {
        if self.working.is_empty() {
            format!("{}: {IDLE}", self.captain)
        } else {
            self.working.join("\n")
        }
    }
}

/// Whether a ship is working: the one source of its dot. Today the only
/// data is the captain's nick status, counted only while the server vouches
/// for it (`conversation::vouched_status`, the heading pill's rule), so
/// nothing is invented without one. A later change adds the crew at work to
/// `working` here.
pub(crate) fn ship(captain: &str, statuses: &Statuses, members: &[Member]) -> Activity {
    let status = crate::conversation::vouched_status(captain, statuses, members);
    Activity {
        captain: captain.to_owned(),
        working: status.into_iter().map(str::to_owned).collect(),
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
