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
}

impl Choice {
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
    let mine = |reaction: &str| {
        let key = reaction_key(reaction);
        summaries
            .iter()
            .any(|summary| summary.mine() && reaction_key(&summary.reaction) == key)
    };
    let choice = |(reaction, name): (&str, &'static str)| Choice {
        reaction: reaction.to_owned(),
        name,
        mine: mine(reaction),
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
