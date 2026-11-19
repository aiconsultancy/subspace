//! Line kinds and descriptive texts for messages, shared by live handling
//! and chathistory playback.

use super::{Session, lines};
use crate::model::{Line, LineKind, Target};
use crate::proto::{Message, ctcp};

/// The kind and text of a membership or channel event, without the actor.
pub(super) fn event(message: &Message) -> Option<(LineKind, String)> {
    let with_reason = |base: String, reason: &str| match reason.is_empty() {
        true => base,
        false => format!("{base} ({reason})"),
    };
    let param = |index| message.param(index);
    let described = match message.command.as_str() {
        "JOIN" => (LineKind::Join, "joined".to_string()),
        "PART" => (LineKind::Part, with_reason("left".into(), param(1))),
        "QUIT" => (LineKind::Quit, with_reason("quit".into(), param(0))),
        "KICK" => (
            LineKind::Kick,
            with_reason(format!("kicked {}", param(1)), param(2)),
        ),
        "NICK" => (LineKind::Nick, format!("is now known as {}", param(0))),
        "TOPIC" => (
            LineKind::Topic,
            format!("changed the topic to: {}", param(1)),
        ),
        "MODE" => (
            LineKind::Mode,
            format!(
                "sets mode {}",
                message.params.get(1..).unwrap_or_default().join(" ")
            ),
        ),
        "INVITE" => (
            LineKind::Invite,
            format!("invited {} to {}", param(0), param(1)),
        ),
        _ => return None,
    };
    Some(described)
}

/// A live MODE line: a change to our own user modes reads as a server line,
/// such as `Your user modes changed: +i`; channel modes as an event.
pub(super) fn mode(message: &Message, own_user_modes: bool) -> (LineKind, String) {
    let modes = message.params.get(1..).unwrap_or_default().join(" ");
    match own_user_modes {
        true => (LineKind::Info, format!("Your user modes changed: {modes}")),
        false => (LineKind::Mode, format!("sets mode {modes}")),
    }
}

/// The kind and text of a PRIVMSG or NOTICE; `None` for CTCP other than
/// `ACTION`.
pub(super) fn message(message: &Message) -> Option<(LineKind, String)> {
    let text = message.trailing();
    let query = ctcp::parse(text);
    message_kind(&message.command, text, query)
}

fn message_kind(
    command: &str,
    text: &str,
    query: Option<ctcp::Ctcp>,
) -> Option<(LineKind, String)> {
    match (command, query) {
        ("PRIVMSG", Some(query)) if query.command == "ACTION" => {
            Some((LineKind::Action, query.params.to_string()))
        }
        ("NOTICE", None) => Some((LineKind::Notice, text.to_string())),
        ("PRIVMSG", None) => Some((LineKind::Privmsg, text.to_string())),
        _ => None,
    }
}

/// A chathistory line for `target`, without changing any state.
pub(super) fn history_line(session: &Session, message: &Message, target: &Target) -> Option<Line> {
    event(message)
        .or_else(|| self::message(message))
        .map(|(kind, text)| lines::line(session, message, target.clone(), kind, text))
}
