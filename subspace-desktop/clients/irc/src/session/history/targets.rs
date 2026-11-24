//! Discover missed private conversations at registration.

use chrono::{DateTime, TimeDelta, Utc};

use super::{CAP, CATCH_UP, Delivery, Request, send};
use crate::proto::Message;
use crate::session::{Output, Session, lines};

pub(in crate::session) const BATCH: &str = "draft/chathistory-targets";
const LABEL: &str = "subspace-history-targets";
const TARGET_LIMIT: u16 = 100;
const CLOCK_SKEW: TimeDelta = TimeDelta::seconds(5);

pub(in crate::session) struct Discovery {
    before: DateTime<Utc>,
    limit: u16,
}

/// Finds conversations initiated while the account was disconnected.
pub(in crate::session) fn request(session: &mut Session) -> Option<Message> {
    if !session.registered()
        || session.reg.account.is_none()
        || session.discovering_queries.is_some()
        || !session.has_cap(CAP)
        || !session.has_cap("batch")
    {
        return None;
    }

    // A partial reconnect may save newer messages from a known conversation
    // before a first DM arrives. Use retained history for discovery so that
    // the newer message cannot hide the first DM on the next connection.
    Some(page(session, session.now + CLOCK_SKEW))
}

fn page(session: &mut Session, before: DateTime<Utc>) -> Message {
    let limit = match session.isupport.chathistory {
        0 => TARGET_LIMIT,
        server => server.min(TARGET_LIMIT),
    };
    session.discovering_queries = Some(Discovery { before, limit });
    let command = Message::new(
        "CHATHISTORY",
        [
            "TARGETS".to_owned(),
            lines::history_selector(None, before),
            lines::history_selector(None, DateTime::UNIX_EPOCH),
            limit.to_string(),
        ],
    );
    if session.has_cap("labeled-response") {
        command.with_tag("label", LABEL)
    } else {
        command
    }
}

pub(super) fn is_failure(session: &Session, message: &Message) -> bool {
    message.tag("label") == Some(LABEL)
        || (session.discovering_queries.is_some()
            && message.param(2).eq_ignore_ascii_case("TARGETS")
            && !session
                .history
                .contains_key(&session.fold(message.param(2))))
}

/// TARGETS also lists channels and known DMs. Use all targets for the
/// next page cursor and fetch only the previously unknown conversations.
pub(in crate::session) fn deliver(
    session: &mut Session,
    opening: &Message,
    messages: &[Message],
    closed: bool,
    out: &mut Output,
) {
    let Some(discovery) = session.discovering_queries.take().filter(|_| closed) else {
        return;
    };
    let limit = usize::from(discovery.limit);
    let mut oldest = discovery.before;
    for (nick, time) in messages.iter().take(limit).filter_map(target) {
        oldest = oldest.min(time);
        if let Some(command) = fetch(session, nick) {
            out.send.push(command);
        }
    }
    if messages.len() >= limit
        && opening.tag("draft/chathistory-end").is_none()
        && oldest < discovery.before
        && oldest > DateTime::UNIX_EPOCH
    {
        out.send.push(page(session, oldest));
    }
}

fn target(message: &Message) -> Option<(&str, DateTime<Utc>)> {
    if message.command != "CHATHISTORY" || !message.param(0).eq_ignore_ascii_case("TARGETS") {
        return None;
    }
    Some((message.param(1), lines::parse_time(message.param(2))?))
}

fn fetch(session: &mut Session, nick: &str) -> Option<Message> {
    if !private_nick(session, nick) {
        return None;
    }
    let key = session.fold(nick);
    if session.queries.contains(&key) || session.history.contains_key(&key) {
        return None;
    }
    let request = Request {
        subcommand: "LATEST",
        selector: "*".to_owned(),
        limit: CATCH_UP,
        delivery: Delivery::Discovered,
    };
    send(session, nick, request)
}

fn private_nick(session: &Session, nick: &str) -> bool {
    !nick.is_empty()
        && !nick.starts_with([':', '*'])
        && !nick.contains(|character: char| character.is_whitespace() || character == ',')
        && session.isupport.channel(nick).is_none()
        && !session.isupport.casemapping.same(nick, &session.reg.nick)
}
