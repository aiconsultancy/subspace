//! Numerics and other server text: ISUPPORT, MOTD, errors and the rest.

use super::{Output, Session, lines, listing};
use crate::model::{Event, LineKind, Target};
use crate::proto::Message;

/// Error numerics about a channel, shown in that channel when joined.
const CHANNEL_ERRORS: &[&str] = &[
    "403", "404", "405", "442", "467", "471", "473", "474", "475", "477", "482", "489",
];
const ERROR_NUMERICS: std::ops::RangeInclusive<u16> = 400..=599;

/// The readable text of a server message: numerics without our nick, other
/// commands with their name.
pub(super) fn text(message: &Message) -> String {
    let numeric = message.command.chars().all(|c| c.is_ascii_digit());
    let known = message.command == "ERROR";
    let params = match numeric {
        true => message.params.get(1..).unwrap_or_default(),
        false => &message.params,
    };
    let text = params.join(" ");
    match numeric || known {
        true => text,
        false => format!("{} {text}", message.command),
    }
}

pub(super) fn server_line(session: &Session, message: &Message, kind: LineKind, out: &mut Output) {
    out.event(Event::Line(lines::line(
        session,
        message,
        Target::Server,
        kind,
        text(message),
    )));
}

/// 900 (`RPL_LOGGEDIN`): shown, and our account remembered.
pub(super) fn logged_in(session: &mut Session, message: &Message, out: &mut Output) {
    server_line(session, message, LineKind::Info, out);
    session.reg.account = Some(message.param(2).to_owned()).filter(|account| !account.is_empty());
    out.send.extend(super::history::targets::request(session));
}

/// 901 (`RPL_LOGGEDOUT`): shown, and our account forgotten.
pub(super) fn logged_out(session: &mut Session, message: &Message, out: &mut Output) {
    server_line(session, message, LineKind::Info, out);
    session.reg.account = None;
    session.discovering_queries = None;
}

pub(super) fn isupport(session: &mut Session, message: &Message, out: &mut Output) {
    let allowed = super::reactions::supported(session);
    let end = message.params.len().saturating_sub(1);
    message
        .params
        .get(1..end)
        .unwrap_or_default()
        .iter()
        .for_each(|token| session.isupport.apply(token));
    let names = session.spellings.names();
    session.refold(names);
    super::metadata::refold(session);
    if super::reactions::supported(session) != allowed {
        out.updates.now.push(super::reactions::support(session));
    }
}

/// 221 (`RPL_UMODEIS`): our user modes, such as `+Zi`.
pub(super) fn user_modes(session: &mut Session, message: &Message, out: &mut Output) {
    let text = format!("Your user modes are {}", text(message));
    out.event(Event::Line(lines::line(
        session,
        message,
        Target::Server,
        LineKind::Info,
        text,
    )));
}

/// Any other message: an info or error line in the server buffer, or in a
/// joined channel for channel errors.
pub(super) fn other(session: &mut Session, message: &Message, out: &mut Output) {
    listing::refused(session, message);
    let channel = session
        .channels
        .get(&session.fold(message.param(1)))
        .map(|channel| channel.name.clone());
    let (target, kind) = placement(&message.command, channel);
    out.event(Event::Line(lines::line(
        session,
        message,
        target,
        kind,
        text(message),
    )));
}

fn placement(command: &str, channel: Option<String>) -> (Target, LineKind) {
    let error = command
        .parse::<u16>()
        .is_ok_and(|code| ERROR_NUMERICS.contains(&code));
    let kind = match error {
        true => LineKind::Error,
        false => LineKind::Info,
    };
    let target = match (CHANNEL_ERRORS.contains(&command), channel) {
        (true, Some(channel)) => Target::Channel(channel),
        _ => Target::Server,
    };
    (target, kind)
}
