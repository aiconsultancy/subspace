//! `draft/chathistory` requests and their replies, and the anchors that let
//! a connection ask only for what the owner missed.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::{Output, Session, describe, lines, reactions};
use crate::model::{BufferKey, Event, Line, Target};
use crate::proto::Message;

const CAP: &str = "draft/chathistory";
mod anchors;
use anchors::anchor;
pub(super) use anchors::{remember, track};

pub(super) mod targets;
/// Lines requested when a buffer opens with nothing held.
pub const LATEST: u16 = 100;
/// The most missed lines requested for an anchored buffer.
pub const CATCH_UP: u16 = 500;

/// One request waiting for its batch and how its lines will be delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pending {
    limit: u16,
    delivery: Delivery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delivery {
    Older,
    Backfill,
    Around,
    Discovered,
}

/// Requests per folded target, oldest first: servers answer in order.
pub(super) type Requests = BTreeMap<String, Vec<Pending>>;

/// One `CHATHISTORY` request before it is sent.
struct Request {
    subcommand: &'static str,
    selector: String,
    limit: u16,
    delivery: Delivery,
}

/// `CHATHISTORY LATEST <target> * <limit>`, when the server supports it.
pub(super) fn latest(session: &mut Session, target: &str, limit: u16) -> Option<Message> {
    request(session, target, None, limit)
}

/// What a buffer opening asks for: the lines after its anchor when the
/// owner holds some (`LATEST <target> msgid=…`), else the latest page.
pub(super) fn resume(session: &mut Session, target: &str) -> Option<Message> {
    match anchor(&session.anchors, target) {
        Some(anchor) => {
            let request = Request {
                subcommand: "LATEST",
                selector: lines::history_selector(anchor.id.as_deref(), anchor.time),
                limit: CATCH_UP,
                delivery: Delivery::Backfill,
            };
            send(session, target, request)
        }
        None => latest(session, target, LATEST),
    }
}

/// `CHATHISTORY BEFORE <target> timestamp=<time> <limit>`, or `LATEST`
/// without a time.
pub(super) fn request(
    session: &mut Session,
    target: &str,
    before: Option<DateTime<Utc>>,
    limit: u16,
) -> Option<Message> {
    let (subcommand, selector) = match before {
        Some(time) => ("BEFORE", lines::history_selector(None, time)),
        None => ("LATEST", "*".to_string()),
    };
    let request = Request {
        subcommand,
        selector,
        limit,
        delivery: Delivery::Older,
    };
    send(session, target, request)
}

/// `CHATHISTORY AROUND <target> msgid=<id> <limit>`, or by time.
pub(super) fn around(
    session: &mut Session,
    target: &str,
    point: (Option<&str>, DateTime<Utc>),
    limit: u16,
) -> Option<Message> {
    let request = Request {
        subcommand: "AROUND",
        selector: lines::history_selector(point.0, point.1),
        limit,
        delivery: Delivery::Around,
    };
    send(session, target, request)
}

fn send(session: &mut Session, target: &str, request: Request) -> Option<Message> {
    let enabled = session.has_cap(CAP);
    let key = session.fold(target);
    let limit = pending(
        &mut session.history,
        (enabled, session.isupport.chathistory),
        key,
        Pending {
            limit: request.limit,
            delivery: request.delivery,
        },
    )?;
    Some(Message::new(
        "CHATHISTORY",
        [
            request.subcommand.to_string(),
            target.to_string(),
            request.selector,
            limit,
        ],
    ))
}

/// The answer to a request that was not sent, so the owner stops waiting.
/// Once registered without `draft/chathistory`, there is no older history;
/// before that, the request just ends and may be repeated.
pub(super) fn unavailable(buffer: &BufferKey, unsent: bool, registered: bool) -> Option<Event> {
    unsent.then(|| Event::History {
        buffer: buffer.clone(),
        lines: Vec::new(),
        complete: registered,
    })
}

/// Records a pending request; returns its limit, or `None` without the cap.
/// `server` is whether the cap is on and the server's own limit (0: none).
fn pending(
    requests: &mut Requests,
    server: (bool, u16),
    key: String,
    request: Pending,
) -> Option<String> {
    let (enabled, server_limit) = server;
    if !enabled || request.limit == 0 {
        return None;
    }
    let limit = match server_limit {
        0 => request.limit,
        server => request.limit.min(server),
    };
    requests
        .entry(key)
        .or_default()
        .push(Pending { limit, ..request });
    Some(limit.to_string())
}

/// The oldest request waiting for `key`, removed.
fn take(requests: &mut Requests, key: &str) -> Option<Pending> {
    let queue = requests.get_mut(key)?;
    let first = (!queue.is_empty()).then(|| queue.remove(0));
    if queue.is_empty() {
        requests.remove(key);
    }
    first
}

/// A finished `chathistory` batch for `target`, oldest first.
pub(super) fn deliver(
    session: &mut Session,
    target: &str,
    messages: &[Message],
    closed: bool,
    out: &mut Output,
) {
    let key = session.fold(target);
    let requested = take(&mut session.history, &key);
    let buffer_target = buffer_target(session, target);
    let lines: Vec<Line> = messages
        .iter()
        .filter_map(|message| describe::history_line(session, message, &buffer_target))
        .collect();
    let reacted = reactions::history(session, messages, &buffer_target);
    let buffer = session.buffer(buffer_target);
    out.updates.now.extend(reacted);
    match requested.map(|request| request.delivery) {
        Some(Delivery::Discovered) if lines.is_empty() => {}
        Some(Delivery::Discovered) if session.queries.insert(key) => {
            // New conversations use the same unread handling as bouncer replay.
            out.events.extend(lines.into_iter().map(Event::Replayed));
        }
        Some(Delivery::Around) => out.event(Event::Around { buffer, lines }),
        Some(Delivery::Backfill | Delivery::Discovered) => {
            out.event(Event::Backfill { buffer, lines });
        }
        _ => out.event(Event::History {
            buffer,
            complete: closed && complete(messages.len(), requested),
            lines,
        }),
    }
}

/// `FAIL CHATHISTORY <code> [context…] :<description>` ends the pending
/// requests its context names. Servers differ in where the target appears,
/// and some name none, so without a match every pending request ends.
pub(super) fn failed(session: &mut Session, message: &Message, out: &mut Output) {
    if targets::is_failure(session, message) {
        session.discovering_queries = None;
        return;
    }

    let context = message
        .params
        .get(2..message.params.len().saturating_sub(1))
        .unwrap_or_default();
    let named: Vec<String> = context
        .iter()
        .map(|param| session.fold(param))
        .filter(|key| session.history.contains_key(key))
        .collect();
    let targets = match named.is_empty() {
        true => session.history.keys().cloned().collect(),
        false => named,
    };
    for target in targets {
        let requests = session.history.remove(&target).unwrap_or_default();
        out.events.extend(ended(session, &target, &requests));
    }
}

/// Ends every pending request when the connection ends first.
pub(super) fn abandon(session: &mut Session) -> Vec<Event> {
    let pending = std::mem::take(&mut session.history);
    pending
        .into_iter()
        .flat_map(|(target, requests)| ended(session, &target, &requests))
        .collect()
}

/// Answers without lines: older history is not complete and may load
/// later; a merge request just ends.
fn ended(session: &Session, target: &str, requests: &[Pending]) -> Vec<Event> {
    requests
        .iter()
        .filter_map(|request| {
            let buffer = session.buffer(buffer_target(session, target));
            match request.delivery {
                Delivery::Discovered => None,
                Delivery::Backfill => Some(Event::Backfill {
                    buffer,
                    lines: Vec::new(),
                }),
                Delivery::Around => Some(Event::Around {
                    buffer,
                    lines: Vec::new(),
                }),
                Delivery::Older => Some(Event::History {
                    buffer,
                    lines: Vec::new(),
                    complete: false,
                }),
            }
        })
        .collect()
}

fn buffer_target(session: &Session, target: &str) -> Target {
    session.isupport.channel(target).map_or_else(
        || Target::Query(target.to_string()),
        |channel| Target::Channel(channel.to_string()),
    )
}

/// Fewer messages than requested means there is no older history.
fn complete(count: usize, requested: Option<Pending>) -> bool {
    match requested {
        Some(request) => count < usize::from(request.limit),
        None => count == 0,
    }
}

/// After registration: each anchored private conversation opens again and
/// asks for what it missed.
pub(super) fn resume_queries(session: &mut Session) -> Vec<Message> {
    let nicks: Vec<String> = session
        .anchors
        .iter()
        .filter_map(|anchor| match &anchor.target {
            Target::Query(nick) => Some(nick.clone()),
            _ => None,
        })
        .collect();
    nicks
        .iter()
        .filter_map(|nick| {
            let key = session.fold(nick);
            session.queries.insert(key);
            resume(session, nick)
        })
        .collect()
}
