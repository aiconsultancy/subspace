//! Routes conversation traffic and responds to supported CTCP queries.

use chrono::TimeDelta;

use super::{Output, Session, describe, history, lines, listing, reactions};
use crate::model::{Event, LineKind, Target, Typing};
use crate::proto::{Message, ctcp};

const CTCP_SENDER_INTERVAL: TimeDelta = TimeDelta::seconds(3);
const CTCP_WINDOW: TimeDelta = TimeDelta::minutes(1);
const CTCP_REPLIES_PER_WINDOW: usize = 10;

pub(super) fn privmsg(session: &mut Session, message: &Message, out: &mut Output) {
    let target = lines::message_target(session, message);
    if let Some(nick) = lines::query_nick(&target) {
        session.queries.insert(session.fold(nick));
    }
    out.send.extend(ctcp_reply(session, message));
    conversation_line(session, message, target, out);
}

pub(super) fn notice(session: &mut Session, message: &Message, out: &mut Output) {
    conversation_line(
        session,
        message,
        lines::notice_target(session, message),
        out,
    );
}

fn conversation_line(session: &Session, message: &Message, target: Target, out: &mut Output) {
    if let Some((kind, text)) = describe::message(message) {
        out.event(Event::Line(lines::line(
            session, message, target, kind, text,
        )));
    }
}

fn ctcp_reply(session: &mut Session, message: &Message) -> Option<Message> {
    let nick = message.nick().filter(|nick| !session.is_own(nick))?;
    let query = ctcp::parse(message.trailing())?;
    let reply = match query.command.as_str() {
        "PING" => query.params.to_owned(),
        "TIME" => session.now.to_rfc2822(),
        "VERSION" => concat!("Subspace IRC ", env!("CARGO_PKG_VERSION")).to_owned(),
        _ => return None,
    };
    if !allow_ctcp_reply(session, nick) {
        return None;
    }

    let mut text = format!("\x01{}", query.command);
    if !reply.is_empty() {
        text.push(' ');
        text.push_str(&reply);
    }
    text.push('\x01');
    Some(Message::new("NOTICE", [nick, &text]))
}

fn allow_ctcp_reply(session: &mut Session, nick: &str) -> bool {
    let now = session.now;
    session
        .ctcp_replies
        .retain(|(time, _)| now - time < CTCP_WINDOW);
    let folded = session.fold(nick);
    if session.ctcp_replies.len() >= CTCP_REPLIES_PER_WINDOW
        || session.ctcp_replies.iter().any(|(time, sender)| {
            now - time < CTCP_SENDER_INTERVAL && session.fold(sender) == folded
        })
    {
        return false;
    }
    session.ctcp_replies.push_back((now, nick.into()));
    true
}

pub(super) fn tagmsg(session: &mut Session, message: &Message, out: &mut Output) {
    out.updates.now.extend(reactions::live(session, message));
    typing(session, message, out);
}

fn typing(session: &Session, message: &Message, out: &mut Output) {
    let nick = message.nick().unwrap_or_default();
    if message.nick().is_some_and(|nick| session.is_own(nick)) {
        return;
    }
    let state = match message.tag("+typing") {
        Some("active") => Typing::Active,
        Some("paused") => Typing::Paused,
        Some("done") => Typing::Done,
        _ => return,
    };
    out.event(Event::Typing {
        buffer: session.buffer(lines::message_target(session, message)),
        nick: nick.to_owned(),
        state,
    });
}

pub(super) fn markread(session: &mut Session, message: &Message, out: &mut Output) {
    let name = message.param(0);
    let target = match session.isupport.channel(name) {
        Some(channel) => Target::Channel(channel.to_owned()),
        None => Target::Query(name.to_owned()),
    };
    if let Some(time) = message
        .param(1)
        .strip_prefix("timestamp=")
        .and_then(lines::parse_time)
    {
        out.event(Event::ReadMarker {
            buffer: session.buffer(target),
            time,
        });
    }
}

pub(super) fn standard_reply(session: &mut Session, message: &Message, out: &mut Output) {
    listing::failed(session, message);
    let kind = if message.command == "FAIL" {
        if message.param(0) == "CHATHISTORY" {
            history::failed(session, message, out);
        }
        LineKind::Error
    } else {
        LineKind::Info
    };
    let text = format!(
        "{} {}: {}",
        message.param(0),
        message.param(1),
        message.trailing()
    );
    out.event(Event::Line(lines::line(
        session,
        message,
        Target::Server,
        kind,
        text,
    )));
}
