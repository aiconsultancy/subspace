//! A fleet channel's own commands, which Channels answers itself rather
//! than the IRC client. In this version there is one, `/ask <text>`: the
//! text addressed to the channel's agent, the ship's captain in a ship's
//! channel or the rear admiral in the Starbase, sent as an ordinary message
//! whose first word is `<nick>:`, so the request's status line tracks it.

use subspace_client_irc::{BufferKey, Command, Target};

use crate::fleet::Roster;

/// The command's name, without its slash. It names no IRC client command
/// (`subspace_client_irc::COMMANDS`), which a test checks.
pub(crate) const NAME: &str = "ask";
/// Its argument, as the completion list shows it.
pub(crate) const ARGS: &str = "<text>";

/// What `/ask` in a draft comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Asked {
    /// The message to send.
    Send(Command),
    /// Why nothing was sent; the draft stays.
    Refused(&'static str),
}

/// The agent `/ask` addresses in `buffer`, when it is a ship's channel or
/// a Starbase the roster knows.
pub(crate) fn agent<'a>(roster: &'a Roster, buffer: &BufferKey) -> Option<&'a str> {
    match &buffer.target {
        Target::Channel(name) => roster.asked_in(name),
        Target::Query(_) | Target::Server => None,
    }
}

/// The description beside `/ask` in the completion list.
pub(crate) fn description(agent: &str) -> String {
    format!("Ask {agent} and track the answer")
}

/// `text` as `/ask`, when it is one: the rest of the line addressed to the
/// channel's agent, or why it cannot be sent.
pub(crate) fn ask(text: &str, buffer: &BufferKey, roster: &Roster) -> Option<Asked> {
    let rest = argument(text)?;
    let Some(agent) = agent(roster, buffer) else {
        return Some(Asked::Refused(
            "/ask works in a ship's channel or its fleet's Starbase.",
        ));
    };
    if rest.is_empty() {
        return Some(Asked::Refused("Type what to ask after /ask."));
    }
    Some(Asked::Send(Command::Say {
        buffer: buffer.clone(),
        text: format!("{agent}: {rest}"),
        action: false,
    }))
}

/// What `/ask` refuses when the agent's address makes the message too long.
pub(crate) const TOO_LONG: &str =
    "With the agent's name in front, this is too long to send. Shorten it.";

/// `asked`, refused when its message, with the agent's address added, is
/// over the connection's limits (`batches`: whether it sends multi-line
/// text as one batch): the draft was checked before the address was added.
pub(crate) fn within_limits(asked: Asked, batches: bool) -> Asked {
    match &asked {
        Asked::Send(Command::Say { text, .. })
            if subspace_client_irc::check_draft(text, batches).is_err() =>
        {
            Asked::Refused(TOO_LONG)
        }
        _ => asked,
    }
}

/// The text after `/ask` and a space, trimmed, when `text` starts with the
/// command, in any case.
fn argument(text: &str) -> Option<&str> {
    let command = text.strip_prefix('/')?;
    let (name, rest) = command
        .split_once(char::is_whitespace)
        .unwrap_or((command, ""));
    name.eq_ignore_ascii_case(NAME).then(|| rest.trim())
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::{COMMANDS, Command};

    use super::{Asked, NAME, ask};
    use crate::fixtures::{channel, query};
    use crate::fleet::Roster;
    use crate::fleet::tests::{agent, fleet_with};

    /// The aurora fleet: rear admiral `aurora-admiral` in `#aurora-starbase`
    /// and the captain of the ship Lyra, `aurora-lyra`.
    fn roster() -> Roster {
        let aurora = fleet_with(
            "aurora",
            vec![
                agent("admiral-vance", "rearAdmiral", Some("aurora-admiral"), None),
                agent("aurora-lyra", "captain", Some("aurora-lyra"), Some("Lyra")),
            ],
            serde_json::json!({"starbaseChannel": "#aurora-starbase"}),
        );
        Roster::new([&aurora])
    }

    fn said(text: &str, to: &str) -> Option<Asked> {
        Some(Asked::Send(Command::Say {
            buffer: channel(to),
            text: text.into(),
            action: false,
        }))
    }

    #[test]
    fn ask_names_no_irc_client_command() {
        assert!(COMMANDS.iter().all(|command| !command.answers(NAME)));
    }

    #[test]
    fn ask_builds_a_say_command_addressed_to_the_channel_s_agent() {
        let roster = roster();
        assert_eq!(
            ask("/ask capture a baseline", &channel("#aurora-lyra"), &roster),
            said("aurora-lyra: capture a baseline", "#aurora-lyra"),
            "the ship's captain"
        );
        assert_eq!(
            ask(
                "/ASK   show Priya the pan ",
                &channel("#Aurora-Starbase"),
                &roster
            ),
            said("aurora-admiral: show Priya the pan", "#Aurora-Starbase"),
            "the rear admiral in the Starbase"
        );
    }

    #[test]
    fn ask_is_refused_when_the_address_makes_it_too_long() {
        use super::{TOO_LONG, within_limits};
        let roster = roster();
        let room = channel("#aurora-lyra");
        let fits = ask("/ask capture a baseline", &room, &roster).unwrap();
        assert_eq!(within_limits(fits.clone(), false), fits);
        let limit = subspace_client_irc::MAX_INLINE_BYTES;
        let long = format!("/ask {}", "a".repeat(limit - "/ask ".len()));
        assert!(
            subspace_client_irc::check_draft(&long, false).is_ok(),
            "the draft fits"
        );
        let asked = ask(&long, &room, &roster).unwrap();
        assert_eq!(within_limits(asked, false), Asked::Refused(TOO_LONG));
    }

    #[test]
    fn ask_is_refused_in_a_starbase_that_two_rear_admirals_name() {
        let aurora = fleet_with(
            "aurora",
            vec![
                agent("admiral-vance", "rearAdmiral", Some("aurora-admiral"), None),
                agent("admiral-reyes", "rearAdmiral", Some("aurora-deputy"), None),
            ],
            serde_json::json!({"starbaseChannel": "#aurora-starbase"}),
        );
        let roster = Roster::new([&aurora]);
        assert!(matches!(
            ask("/ask anything", &channel("#aurora-starbase"), &roster),
            Some(Asked::Refused(_))
        ));
    }

    #[test]
    fn ask_is_refused_where_there_is_no_agent_or_nothing_to_ask() {
        let roster = roster();
        for buffer in [
            channel("#aurora-command"),
            channel("#lounge"),
            query("mira"),
        ] {
            assert!(matches!(
                ask("/ask anything", &buffer, &roster),
                Some(Asked::Refused(_))
            ));
        }
        assert_eq!(
            ask("/ask ", &channel("#aurora-lyra"), &roster),
            Some(Asked::Refused("Type what to ask after /ask."))
        );
        assert_eq!(ask("/asking", &channel("#aurora-lyra"), &roster), None);
        assert_eq!(ask("ask me", &channel("#aurora-lyra"), &roster), None);
    }
}
