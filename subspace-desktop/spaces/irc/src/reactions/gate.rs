//! The one rule every reaction Channels sends goes through
//! ([`crate::conversation::reacting::toggle`]): the pills, the `+N` menu,
//! **Add reaction**, the `r` key's menu, and anything added later.
//!
//! - **No reaction to a fleet agent in private.** Everything said to an
//!   agent is said in a channel, and a reaction, a stop above all, is
//!   saying something. A private conversation with a roster agent sends no
//!   reaction, as its composer sends no message
//!   ([`crate::compose::AgentNotice::refusing`]). Until the initiatives
//!   first load, no one is known not to be an agent, so a private
//!   conversation sends no reaction then either. Reactions between people
//!   in private are ordinary.
//! - **A stop is sent only from its labelled item.** A new ✋ or 🛑 of ours
//!   (Stargate's stop keys, skin tones and the variation selector folded)
//!   goes out only from **Stop \<ship\>** in a channel's menu. Anywhere
//!   else, whatever the app knows of the message, it is explained instead:
//!   Stargate stops a queued turn before any status line exists, so not
//!   recognising a request yet is no reason to send one. Taking back our
//!   own stop reaction stays possible.

use subspace_client_irc::Target;
use subspace_client_irc::reactions::agent_stop::is_agent_stop_reaction;

use crate::compose::AgentNotice;
use crate::fleet::Roster;

/// What happens to a reaction asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Gate {
    /// Send it (or take ours back).
    Send,
    /// A stop reaction not chosen from its labelled item: say where the
    /// deliberate stop is, and send nothing.
    Explain,
    /// Nothing is sent here, for the reason given.
    Refuse(String),
}

/// How a reaction was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Via {
    /// A pill, the `+N` menu or a common choice.
    Ordinary,
    /// The labelled **Stop \<ship\>** item.
    LabelledStop,
}

/// Why nothing is sent in the server buffer.
pub(crate) const SERVER: &str = "Reactions are sent in conversations, not to the server.";

/// Why nothing is sent in private before the initiatives first load.
pub(crate) const ROSTER_LOADING: &str =
    "Reactions in private wait until Subspace knows who the fleet agents are.";

/// Whether `reaction`, added (`add`) or taken back, may be sent in the
/// conversation `target`, asked for `via`.
pub(crate) fn gate(
    (target, roster): (&Target, &Roster),
    reaction: &str,
    add: bool,
    via: Via,
) -> Gate {
    if let Some(words) = refusal(target, roster) {
        return Gate::Refuse(words);
    }
    let labelled = via == Via::LabelledStop && matches!(target, Target::Channel(_));
    if add && is_agent_stop_reaction(reaction) && !labelled {
        Gate::Explain
    } else {
        Gate::Send
    }
}

/// Why `target` takes no reaction at all: the server buffer, a private
/// conversation with a fleet agent, or any private conversation before
/// the initiatives first load.
pub(crate) fn refusal(target: &Target, roster: &Roster) -> Option<String> {
    match target {
        Target::Server => Some(SERVER.to_owned()),
        Target::Query(nick) if roster.not_known_yet(nick) => Some(ROSTER_LOADING.to_owned()),
        Target::Query(nick) => AgentNotice::refusing(roster, nick, |_| false).map(|n| n.words()),
        Target::Channel(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::Target;

    use super::{Gate, ROSTER_LOADING, SERVER, Via, gate, refusal};
    use crate::fleet::Roster;
    use crate::fleet::tests::{agent, fleet};

    fn roster() -> Roster {
        let aurora = fleet(
            "aurora",
            vec![agent(
                "aurora-lyra",
                "captain",
                Some("aurora-lyra"),
                Some("Lyra"),
            )],
        );
        Roster::new([&aurora])
    }

    fn room() -> Target {
        Target::Channel("#aurora-lyra".into())
    }

    #[test]
    fn a_new_stop_goes_only_from_its_labelled_item_in_a_channel() {
        let roster = roster();
        for stop in [
            "\u{270B}",
            "\u{270B}\u{1F3FD}",
            "\u{1F6D1}",
            "\u{1F6D1}\u{FE0F}",
        ] {
            assert_eq!(
                gate((&room(), &roster), stop, true, Via::Ordinary),
                Gate::Explain,
                "{stop} from a pill or the +N menu"
            );
            assert_eq!(
                gate((&room(), &roster), stop, true, Via::LabelledStop),
                Gate::Send
            );
            assert_eq!(
                gate((&room(), &roster), stop, false, Via::Ordinary),
                Gate::Send,
                "taking ours back stays possible"
            );
            let person = Target::Query("mira".into());
            assert_eq!(
                gate((&person, &roster), stop, true, Via::LabelledStop),
                Gate::Explain,
                "the labelled item counts only in a channel"
            );
        }
        assert_eq!(
            gate((&room(), &roster), "\u{1F44D}", true, Via::Ordinary),
            Gate::Send
        );
    }

    #[test]
    fn nothing_goes_to_an_agent_in_private_and_people_may_react() {
        let roster = roster();
        let agent = Target::Query("Aurora-Lyra".into());
        for (reaction, add) in [
            ("\u{1F44D}", true),
            ("\u{1F44D}", false),
            ("\u{270B}", false),
        ] {
            assert!(matches!(
                gate((&agent, &roster), reaction, add, Via::Ordinary),
                Gate::Refuse(words) if words.contains("fleet agent")
            ));
        }
        let person = Target::Query("mira".into());
        assert_eq!(
            gate((&person, &roster), "\u{1F44D}", true, Via::Ordinary),
            Gate::Send
        );
        assert_eq!(refusal(&Target::Server, &roster).as_deref(), Some(SERVER));
    }

    #[test]
    fn nothing_goes_in_private_before_the_roster_loads() {
        let loading = Roster::default().with_loading(true);
        let unknown = Target::Query("aurora-lyra".into());
        assert_eq!(
            gate((&unknown, &loading), "\u{1F44D}", true, Via::Ordinary),
            Gate::Refuse(ROSTER_LOADING.to_owned()),
            "not yet known to be a person"
        );
        assert_eq!(
            gate((&room(), &loading), "\u{1F44D}", true, Via::Ordinary),
            Gate::Send,
            "a channel is unaffected"
        );
        let loaded = Roster::default();
        assert_eq!(
            gate((&unknown, &loaded), "\u{1F44D}", true, Via::Ordinary),
            Gate::Send,
            "a loaded roster that does not name it: a person"
        );
    }
}
