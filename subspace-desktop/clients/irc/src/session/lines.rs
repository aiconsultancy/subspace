//! Building displayable lines: buffers, timestamps, ownership and highlights.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::{Session, send::REPLY};
use crate::model::{Line, LineKind, Target};
use crate::proto::Message;

/// A line caused by `message`, in `target`'s buffer.
pub(super) fn line(
    session: &Session,
    message: &Message,
    target: Target,
    kind: LineKind,
    text: String,
) -> Line {
    let from = message.nick().map(str::to_string);
    let own = from.as_deref().is_some_and(|nick| session.is_own(nick));
    let highlight = highlight(
        own,
        kind,
        &text,
        &session.reg.nick,
        &session.config.highlights,
    );
    Line {
        buffer: session.buffer(target),
        id: message.tag("msgid").map(str::to_string),
        reply_to: message
            .tag(REPLY)
            .filter(|id| !id.is_empty() && matches!(message.command.as_str(), "PRIVMSG" | "NOTICE"))
            .map(str::to_string),
        client_tags: client_tags(message),
        time: message
            .tag("time")
            .and_then(parse_time)
            .unwrap_or(session.now),
        kind,
        from,
        account: message.tag("account").map(str::to_string),
        text,
        highlight,
        own,
    }
}

/// A line we produced ourselves, such as a local echo or an error.
pub(super) fn local_line(session: &Session, target: Target, kind: LineKind, text: String) -> Line {
    let own = matches!(
        kind,
        LineKind::Privmsg | LineKind::Notice | LineKind::Action
    );
    Line {
        buffer: session.buffer(target),
        id: None,
        reply_to: None,
        client_tags: BTreeMap::new(),
        time: session.now,
        kind,
        from: own.then(|| session.reg.nick.clone()),
        account: None,
        text,
        highlight: false,
        own,
    }
}

/// The client-only tag by which an agent gateway (Stargate) marks request
/// progress on a `NOTICE`, read by Channels.
pub const PROGRESS_TAG: &str = "+stargate/progress";

/// The client-only tags a line keeps, besides the reply reference that
/// [`Line::reply_to`] holds: only those a Space reads. Any sender can add
/// client-only tags, so the rest are dropped rather than held and stored.
pub const KEPT_CLIENT_TAGS: &[&str] = &[PROGRESS_TAG];

/// The longest value a kept client-only tag may have, in bytes; a longer
/// one is dropped. Progress values are a stage name and a short suffix.
const CLIENT_TAG_VALUE_MAX: usize = 64;

/// The kept client-only tags ([`KEPT_CLIENT_TAGS`]) of a `PRIVMSG` or
/// `NOTICE` with short values. Other commands keep none.
fn client_tags(message: &Message) -> BTreeMap<String, String> {
    let message_or_notice = matches!(message.command.as_str(), "PRIVMSG" | "NOTICE");
    message
        .tags
        .iter()
        .filter(|(key, value)| {
            message_or_notice
                && KEPT_CLIENT_TAGS.contains(&key.as_str())
                && value.len() <= CLIENT_TAG_VALUE_MAX
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

/// `msgid=<id>` when there is one, else `timestamp=<time>`.
pub(super) fn history_selector(id: Option<&str>, time: DateTime<Utc>) -> String {
    match id {
        Some(id) => format!("msgid={id}"),
        None => format!("timestamp={}", format_time(time)),
    }
}

pub(super) fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

pub(super) fn format_time(time: DateTime<Utc>) -> String {
    time.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// The buffer for a PRIVMSG, NOTICE or TAGMSG.
pub(super) fn message_target(session: &Session, message: &Message) -> Target {
    let sender = message.nick();
    let own = sender.is_some_and(|nick| session.is_own(nick));
    let channel = session.isupport.channel(message.param(0));
    route(message.param(0), sender, own, channel)
}

fn route(target: &str, sender: Option<&str>, own: bool, channel: Option<&str>) -> Target {
    match (channel, sender, own) {
        (Some(channel), _, _) => Target::Channel(channel.to_string()),
        (None, Some(_), true) => Target::Query(target.to_string()),
        (None, Some(sender), false) => Target::Query(sender.to_string()),
        (None, None, _) => Target::Server,
    }
}

/// Notices outside channels go to the server buffer unless a query with the
/// other side is open.
pub(super) fn notice_target(session: &Session, message: &Message) -> Target {
    let target = message_target(session, message);
    let open =
        query_nick(&target).is_some_and(|nick| session.queries.contains(&session.fold(nick)));
    notice_route(target, session.reg.registered, open)
}

pub(super) fn query_nick(target: &Target) -> Option<&str> {
    match target {
        Target::Query(nick) => Some(nick),
        _ => None,
    }
}

fn notice_route(target: Target, registered: bool, query_open: bool) -> Target {
    match target {
        Target::Channel(_) => target,
        Target::Query(_) if registered && query_open => target,
        _ => Target::Server,
    }
}

/// Whether a message line mentions our nick or a highlight word as a whole
/// word, ignoring case. Our own lines never highlight.
fn highlight(own: bool, kind: LineKind, text: &str, nick: &str, words: &[String]) -> bool {
    let message = matches!(
        kind,
        LineKind::Privmsg | LineKind::Notice | LineKind::Action
    );
    let text = text.to_lowercase();
    let is_word = |c: char| c.is_alphanumeric() || "_-[]\\`^{}|".contains(c);
    let mentions = |word: &str| {
        let word = word.to_lowercase();
        !word.is_empty()
            && text.match_indices(&word).any(|(start, _)| {
                let before = text[..start].chars().next_back();
                let after = text[start + word.len()..].chars().next();
                !before.is_some_and(is_word) && !after.is_some_and(is_word)
            })
    };
    !own && message && (mentions(nick) || words.iter().any(|word| mentions(word)))
}
