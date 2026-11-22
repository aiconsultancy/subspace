//! Whom composer input would message privately, read from what it sends.

use super::{Input, parse_input};
use crate::model::Target;
use crate::proto::parse;

/// The nicks composer `text` in `target` would send a private PRIVMSG or
/// NOTICE to, from the same parse the session sends it with. Text and `/me`
/// in a private conversation go to its other side; `/msg`, `/notice` and
/// `/query` to their target; `/raw`, `/quote` and an unknown command to the
/// target of the PRIVMSG or NOTICE line they carry. A comma-separated target
/// list names each of its nicks. Channels are never named, and input that
/// does not parse names no one, as it sends nothing.
pub fn private_recipients(text: &str, target: &Target) -> Vec<String> {
    parse_input(text).map_or_else(|_| Vec::new(), |input| recipients(&input, target))
}

fn recipients(input: &Input, target: &Target) -> Vec<String> {
    match input {
        Input::Say(_) | Input::Me(_) => partner(target).into_iter().collect(),
        Input::Msg { target, .. } | Input::Notice { target, .. } => nicks(target),
        Input::Query { nick, .. } => nicks(nick),
        Input::Raw(line) => raw_recipients(line),
        _ => Vec::new(),
    }
}

/// The nicks a raw PRIVMSG or NOTICE line addresses.
fn raw_recipients(line: &str) -> Vec<String> {
    parse(line)
        .ok()
        .filter(|message| matches!(message.command.as_str(), "PRIVMSG" | "NOTICE"))
        .and_then(|message| message.params.first().map(|first| nicks(first)))
        .unwrap_or_default()
}

/// The other side of a private conversation.
fn partner(target: &Target) -> Option<String> {
    match target {
        Target::Query(nick) => Some(nick.clone()),
        Target::Channel(_) | Target::Server => None,
    }
}

/// The nicks of a comma-separated target list, without its channels.
fn nicks(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.starts_with(['#', '&', '!', '+']))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::private_recipients;
    use crate::model::Target;

    fn room() -> Target {
        Target::Channel("#ops".into())
    }

    #[test]
    fn msg_notice_and_query_name_each_nick_of_a_target_list() {
        assert_eq!(
            private_recipients("/msg mira,#ops,Lyra hi", &room()),
            ["mira", "Lyra"]
        );
        assert_eq!(private_recipients("/notice lyra hi", &room()), ["lyra"]);
        assert_eq!(private_recipients("/query Lyra", &room()), ["Lyra"]);
        assert_eq!(
            private_recipients("/msg #ops,&local hi", &room()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn raw_and_unknown_commands_name_the_nicks_of_their_privmsg_or_notice() {
        assert_eq!(
            private_recipients("/raw PRIVMSG lyra :hi", &room()),
            ["lyra"]
        );
        assert_eq!(
            private_recipients("/quote notice #ops,lyra :hi", &room()),
            ["lyra"]
        );
        assert_eq!(
            private_recipients("/privmsg mira,lyra hi", &room()),
            ["mira", "lyra"]
        );
        assert_eq!(
            private_recipients("/raw WHOIS lyra", &room()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn text_and_actions_go_to_a_private_conversation_s_other_side_only() {
        let query = Target::Query("lyra".into());
        assert_eq!(private_recipients("hello", &query), ["lyra"]);
        assert_eq!(private_recipients("/me waves", &query), ["lyra"]);
        assert_eq!(private_recipients("hello", &room()), Vec::<String>::new());
        assert_eq!(
            private_recipients("/join #ops", &query),
            Vec::<String>::new()
        );
        assert_eq!(
            private_recipients("//msg lyra", &room()),
            Vec::<String>::new()
        );
        assert_eq!(
            private_recipients("/msg lyra", &room()),
            Vec::<String>::new()
        );
    }
}
