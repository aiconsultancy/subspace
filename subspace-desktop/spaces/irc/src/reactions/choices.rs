//! The short, fixed set of reactions a message offers, and the stop group
//! a running request offers apart from them. No full emoji picker.

use subspace_client_irc::reactions::ReactionSummary;
use subspace_client_irc::reactions::agent_stop::{AGENT_STOP_REACTIONS, reaction_key};

/// The common reactions, each with a plain name for its menu item.
pub(crate) const COMMON: &[(&str, &str)] = &[
    ("\u{1F44D}", "Thumbs up"),
    ("\u{2764}\u{FE0F}", "Heart"),
    ("\u{1F604}", "Smile"),
    ("\u{1F389}", "Celebrate"),
    ("\u{1F440}", "Looking"),
    ("\u{2705}", "Done"),
];

/// Plain names for Stargate's default stop reactions.
const STOP_NAMES: &[&str] = &["Raised hand", "Stop sign"];

/// One reaction to choose, and whether ours is already on the message, so
/// choosing it again takes ours back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Choice {
    pub reaction: String,
    pub name: &'static str,
    pub mine: bool,
    /// Our reactions on the message that this choice stands for, exactly as
    /// the client holds them (`❤` for the menu's `❤️`, a skin tone, the
    /// variation selector): taking ours back removes these values, not the
    /// menu's spelling.
    pub own: Vec<String>,
}

impl Choice {
    /// What choosing it sends: our exact values taken back when it is ours,
    /// else the choice added.
    pub fn sends(&self) -> Vec<(String, bool)> {
        if self.own.is_empty() {
            vec![(self.reaction.clone(), true)]
        } else {
            self.own.iter().map(|own| (own.clone(), false)).collect()
        }
    }

    /// The menu item's words: the reaction and its name.
    pub fn label(&self) -> String {
        format!("{}  {}", self.reaction, self.name)
    }
}

/// The stop reactions offered apart, under `Stop <ship>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StopGroup {
    pub ship: String,
    pub choices: Vec<Choice>,
}

impl StopGroup {
    /// The group's plain label.
    pub fn label(&self) -> String {
        format!("Stop {}", self.ship)
    }
}

/// What a message's reaction menu offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Choices {
    pub common: Vec<Choice>,
    /// Only on a running request to a verified agent, or that agent's
    /// reply to it ([`super::stop::target`]).
    pub stop: Option<StopGroup>,
}

/// The choices for a message showing `summaries`; `ship` names the agent
/// a stop would ask to stop, when the stop group belongs.
pub(crate) fn choices(summaries: &[ReactionSummary], ship: Option<&str>) -> Choices {
    let own = |reaction: &str| {
        let key = reaction_key(reaction);
        summaries
            .iter()
            .filter(|summary| summary.mine() && reaction_key(&summary.reaction) == key)
            .map(|summary| summary.reaction.clone())
            .collect::<Vec<_>>()
    };
    let choice = |(reaction, name): (&str, &'static str)| {
        let own = own(reaction);
        Choice {
            reaction: reaction.to_owned(),
            name,
            mine: !own.is_empty(),
            own,
        }
    };
    Choices {
        common: COMMON.iter().copied().map(choice).collect(),
        stop: ship.map(|ship| StopGroup {
            ship: ship.to_owned(),
            choices: AGENT_STOP_REACTIONS
                .iter()
                .copied()
                .zip(STOP_NAMES.iter().copied())
                .map(choice)
                .collect(),
        }),
    }
}
