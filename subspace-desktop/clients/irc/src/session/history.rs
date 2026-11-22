//! `draft/chathistory` requests and their replies, and the anchors that let
//! a connection ask only for what the owner missed.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::{Output, Session, describe, lines, reactions};
use crate::model::{Anchor, BufferKey, Event, Line, LineKind, Target};
use crate::proto::Message;

const CAP: &str = "draft/chathistory";
/// Lines requested when a buffer opens with nothing held.
pub const LATEST: u16 = 100;
/// The most missed lines requested for an anchored buffer.
pub const CATCH_UP: u16 = 500;

/// One request waiting for its batch: its limit, and whether its lines merge
/// among the held ones ([`Event::Backfill`]) or go before them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pending {
    limit: u16,
    merge: bool,
}

/// Requests per folded target, oldest first: servers answer in order.
pub(super) type Requests = BTreeMap<String, Vec<Pending>>;

/// One `CHATHISTORY` request before it is sent.
struct Request {
    subcommand: &'static str,
    selector: String,
    limit: u16,
    merge: bool,
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
                selector: selector(anchor.id.as_deref(), anchor.time),
                limit: CATCH_UP,
                merge: true,
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
        Some(time) => ("BEFORE", selector(None, time)),
        None => ("LATEST", "*".to_string()),
    };
    let request = Request {
        subcommand,
        selector,
        limit,
        merge: false,
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
        selector: selector(point.0, point.1),
        limit,
        merge: true,
    };
    send(session, target, request)
}

/// `msgid=<id>` when there is one, else `timestamp=<time>`.
fn selector(id: Option<&str>, time: DateTime<Utc>) -> String {
    match id {
        Some(id) => format!("msgid={id}"),
        None => format!("timestamp={}", lines::format_time(time)),
    }
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
            merge: request.merge,
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
    out.event(match requested {
        Some(Pending { merge: true, .. }) => Event::Backfill { buffer, lines },
        _ => Event::History {
            buffer,
            complete: closed && complete(messages.len(), requested),
            lines,
        },
    });
}

/// `FAIL CHATHISTORY <code> [context…] :<description>` ends the pending
/// requests its context names. Servers differ in where the target appears,
/// and some name none, so without a match every pending request ends.
pub(super) fn failed(session: &mut Session, message: &Message, out: &mut Output) {
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
        .map(|request| {
            let buffer = session.buffer(buffer_target(session, target));
            match request.merge {
                true => Event::Backfill {
                    buffer,
                    lines: Vec::new(),
                },
                false => Event::History {
                    buffer,
                    lines: Vec::new(),
                    complete: false,
                },
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

/// The anchor of `target`, by any spelling: anchors may arrive before the
/// server announces its case mapping.
fn anchor<'a>(anchors: &'a [Anchor], target: &str) -> Option<&'a Anchor> {
    anchors
        .iter()
        .find(|anchor| name(&anchor.target).is_some_and(|name| name.eq_ignore_ascii_case(target)))
}

fn name(target: &Target) -> Option<&str> {
    match target {
        Target::Channel(name) | Target::Query(name) => Some(name),
        Target::Server => None,
    }
}

/// Keeps `next` when it is newer than the anchor held for its buffer.
pub(super) fn remember(anchors: &mut Vec<Anchor>, next: Anchor) {
    let Some(target) = name(&next.target) else {
        return;
    };
    match anchors
        .iter()
        .position(|held| name(&held.target).is_some_and(|held| held.eq_ignore_ascii_case(target)))
    {
        Some(at) if anchors[at].time < next.time => anchors[at] = next,
        Some(_) => {}
        None => anchors.push(next),
    }
}

/// Moves each buffer's anchor to the newest message among `events`, so a
/// reconnect asks only for what came after.
pub(super) fn track(anchors: &mut Vec<Anchor>, events: &[Event]) {
    let found: Vec<Anchor> = events
        .iter()
        .flat_map(event_lines)
        .filter(|line| {
            matches!(
                line.kind,
                LineKind::Privmsg | LineKind::Action | LineKind::Notice
            )
        })
        .map(|line| Anchor {
            target: line.buffer.target.clone(),
            id: line.id.clone(),
            time: line.time,
        })
        .collect();
    found
        .into_iter()
        .for_each(|anchor| remember(anchors, anchor));
}

fn event_lines(event: &Event) -> &[Line] {
    match event {
        Event::Line(line) | Event::Replayed(line) => std::slice::from_ref(line),
        Event::History { lines, .. } | Event::Backfill { lines, .. } => lines,
        _ => &[],
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
