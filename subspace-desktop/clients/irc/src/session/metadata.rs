//! Nickname-wide `draft/metadata-2` status values. Ergo 2.19.1 uses
//! 761/766 for notifications; the draft also defines METADATA messages.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};

use super::{Output, Session};
use crate::model::Event;
use crate::proto::Message;

mod text;

pub use text::clean as status_text;

pub(super) const CAP: &str = "draft/metadata-2";

#[derive(Default)]
pub(super) struct Statuses {
    /// Folded nickname -> first spelling and display-safe value.
    pub(super) values: BTreeMap<String, (String, String)>,
    subscribed: bool,
    syncs: BTreeMap<String, DateTime<Utc>>,
}

impl Statuses {
    fn set(
        &mut self,
        key: String,
        nick: &str,
        status: Option<String>,
    ) -> Option<(String, Option<String>)> {
        if self.values.get(&key).map(|(_, text)| text) == status.as_ref() {
            return None;
        }
        let nick = self
            .values
            .remove(&key)
            .map_or_else(|| nick.to_string(), |(nick, _)| nick);
        if let Some(text) = &status {
            self.values.insert(key, (nick.clone(), text.clone()));
        }
        Some((nick, status))
    }
}

pub(super) fn negotiate(session: &mut Session, out: &mut Output) {
    let enabled = session.has_cap(CAP) && session.has_cap("batch");
    if !enabled {
        out.events.extend(clear_all(session));
        session.statuses.subscribed = false;
    } else if session.reg.registered && !session.statuses.subscribed {
        session.statuses.subscribed = true;
        out.send(Message::new("METADATA", ["*", "SUB", "status"]));
    }
}

pub(super) fn receive(session: &mut Session, message: &Message, out: &mut Output) {
    if !session.has_cap(CAP) || !session.has_cap("batch") {
        return;
    }
    let params = match message.command.as_str() {
        "METADATA" => message.params.as_slice(),
        _ => message.params.get(1..).unwrap_or_default(),
    };
    let Some((nick, status)) = value(&message.command, params) else {
        return;
    };
    if nick.starts_with('*') || session.isupport.channel(nick).is_some() {
        return;
    }
    set(session, nick, status.and_then(text::clean), out);
}

fn value<'a>(command: &str, params: &'a [String]) -> Option<(&'a str, Option<&'a str>)> {
    match (command, params) {
        ("761" | "METADATA", [nick, key, visibility, text])
            if key == "status" && !visibility.is_empty() =>
        {
            Some((nick.as_str(), Some(text.as_str())))
        }
        ("METADATA", [nick, key, visibility]) if key == "status" && !visibility.is_empty() => {
            Some((nick.as_str(), None))
        }
        ("766", [nick, key, ..]) if key == "status" => Some((nick.as_str(), None)),
        _ => None,
    }
    .filter(|(nick, _)| !nick.is_empty())
}

fn set(session: &mut Session, nick: &str, status: Option<String>, out: &mut Output) {
    let key = session.fold(nick);
    let changed = session.statuses.set(key, nick, status);
    out.events
        .extend(changed.map(|(nick, status)| Event::NickStatus {
            network: session.network(),
            nick,
            status,
        }));
}

pub(super) fn clear(session: &mut Session, nick: &str, out: &mut Output) {
    set(session, nick, None, out);
}

pub(super) fn clear_all(session: &mut Session) -> Vec<Event> {
    session.statuses.syncs.clear();
    std::mem::take(&mut session.statuses.values)
        .into_values()
        .map(|(nick, _)| Event::NickStatus {
            network: session.network(),
            nick,
            status: None,
        })
        .collect()
}

/// Existing member maps suffice to tell when the final shared channel ends.
pub(super) fn unshared(session: &mut Session, candidates: Vec<String>, out: &mut Output) {
    let gone: Vec<String> = candidates
        .into_iter()
        .filter(|key| {
            !session
                .channels
                .values()
                .any(|channel| channel.members.contains_key(key))
        })
        .filter_map(|key| {
            session
                .statuses
                .values
                .get(&key)
                .map(|(nick, _)| nick.clone())
        })
        .filter(|nick| !session.is_own(nick))
        .collect();
    gone.iter().for_each(|nick| clear(session, nick, out));
}

pub(super) fn departing(session: &Session, channel: &str, nick: &str, own: bool) -> Vec<String> {
    session
        .channels
        .get(channel)
        .map(|channel| {
            channel
                .members
                .keys()
                .filter(|key| own || *key == nick)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn refold(session: &mut Session) {
    session.statuses.values = std::mem::take(&mut session.statuses.values)
        .into_values()
        .map(|(nick, text)| (session.fold(&nick), (nick, text)))
        .collect();
}

/// Bookkeeping and failures must never become conversation lines. Log only
/// the command/code, without echoing untrusted metadata or control bytes.
pub(super) fn reply(_: &mut Session, message: &Message, _: &mut Output) {
    eprintln!("IRC metadata reply: {}", message.command);
}

/// 774: recipient, target, retry-after seconds, optional human explanation.
pub(super) fn sync_later(session: &mut Session, message: &Message, out: &mut Output) {
    reply(session, message, out);
    if !session.reg.registered || !session.has_cap(CAP) || !session.has_cap("batch") {
        return;
    }
    let target = message.param(1);
    let delay = message.param(2).parse::<u32>().ok().map(i64::from);
    let due = delay.and_then(|seconds| session.now.checked_add_signed(Duration::seconds(seconds)));
    if let Some(due) = due.filter(|_| !target.is_empty()) {
        session.statuses.syncs.insert(target.to_string(), due);
    }
    sync_due(session, out);
}

pub(super) fn sync_due(session: &mut Session, out: &mut Output) {
    let now = session.now;
    let due: Vec<String> = session
        .statuses
        .syncs
        .iter()
        .filter(|(_, due)| **due <= now)
        .map(|(target, _)| target.clone())
        .collect();
    due.into_iter().for_each(|target| {
        session.statuses.syncs.remove(&target);
        out.send(Message::new("METADATA", [target, "SYNC".into()]));
    });
}
