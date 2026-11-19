//! JOIN, PART, KICK, QUIT and NICK: who is in which channel.

use std::collections::BTreeMap;

use super::channels::Channel;
use super::{Output, Session, describe, history, lines};
use crate::model::{BufferKey, Event, Line, Member, Target};
use crate::proto::Message;

type Channels = BTreeMap<String, Channel>;

pub(super) fn join(session: &mut Session, message: &Message, out: &mut Output) {
    let channel = message.param(0).to_string();
    let buffer = session.buffer(Target::Channel(channel.clone()));
    let own = message.nick().is_some_and(|nick| session.is_own(nick));
    let keys = (
        session.fold(&channel),
        session.fold(message.nick().unwrap_or_default()),
    );
    let member = joined_member(message);
    let joined = record_join(&mut session.channels, keys, member, own, buffer);
    if session
        .autojoin
        .is_retired(&channel, &|name| session.fold(name))
    {
        return;
    }
    out.events.extend(joined);
    event_line(session, message, Target::Channel(channel.clone()), out);
    if own {
        remember_left(session, &channel, false);
    }
    out.send
        .extend(own.then(|| history::resume(session, &channel)).flatten());
}

/// The user removed `target` from the list: later connections skip a
/// channel in the autojoin until we join it again, and a private
/// conversation's notices go to the server buffer again.
pub(super) fn forget(session: &mut Session, target: &Target) {
    match target {
        Target::Channel(name) => remember_left(session, name, true),
        Target::Query(nick) => {
            let key = session.fold(nick);
            session.queries.remove(&key);
        }
        Target::Server => {}
    }
}

/// Records that the user removed `channel` (`left`) or that we joined it
/// again, so a reconnect does not join a channel the user removed. Names
/// match ignoring ASCII case, as the autojoin compares them before the
/// server announces its case mapping.
fn remember_left(session: &mut Session, channel: &str, left: bool) {
    session
        .left
        .retain(|name| !name.eq_ignore_ascii_case(channel));
    if left {
        session.left.push(channel.to_string());
    }
}

/// The joining member, with its account from `extended-join`.
fn joined_member(message: &Message) -> Member {
    let account = message.param(1);
    Member {
        nick: message.nick().unwrap_or_default().to_string(),
        prefixes: String::new(),
        away: false,
        account: (!account.is_empty() && account != "*").then(|| account.to_string()),
    }
}

fn record_join(
    channels: &mut Channels,
    (key, nick_key): (String, String),
    member: Member,
    own: bool,
    buffer: BufferKey,
) -> Option<Event> {
    let Target::Channel(name) = &buffer.target else {
        return None;
    };
    if own {
        let channel = Channel {
            name: name.clone(),
            ..Channel::default()
        };
        channels.insert(key, channel);
        return Some(Event::Joined { buffer });
    }
    let channel = channels.get_mut(&key)?;
    channel.members.insert(nick_key, member.clone());
    Some(Event::MemberJoined { buffer, member })
}

pub(super) fn part(session: &mut Session, message: &Message, out: &mut Output) {
    let nick = message.nick().unwrap_or_default().to_string();
    leave(session, message, &nick, out);
}

pub(super) fn kick(session: &mut Session, message: &Message, out: &mut Output) {
    leave(session, message, message.param(1), out);
}

/// `nick` left `param(0)` by PART or KICK.
fn leave(session: &mut Session, message: &Message, nick: &str, out: &mut Output) {
    let channel = message.param(0).to_string();
    event_line(session, message, Target::Channel(channel.clone()), out);
    let buffer = session.buffer(Target::Channel(channel.clone()));
    let own = session.is_own(nick);
    let key = session.fold(&channel);
    let nick_key = session.fold(nick);
    let candidates = super::metadata::departing(session, &key, &nick_key, own);
    out.events.extend(record_leave(
        &mut session.channels,
        (&key, &nick_key),
        own,
        buffer,
    ));
    super::metadata::unshared(session, candidates, out);
}

fn record_leave(
    channels: &mut Channels,
    (key, nick_key): (&str, &str),
    own: bool,
    buffer: BufferKey,
) -> Option<Event> {
    if own {
        channels.remove(key);
        return Some(Event::Left { buffer });
    }
    let member = channels.get_mut(key)?.members.remove(nick_key)?;
    Some(Event::MemberLeft {
        buffer,
        nick: member.nick,
    })
}

pub(super) fn quit(session: &mut Session, message: &Message, out: &mut Output) {
    let nick = message.nick().unwrap_or_default().to_string();
    super::metadata::clear(session, &nick, out);
    let nick_key = session.fold(&nick);
    let left = remove_everywhere(&mut session.channels, &nick_key);
    left.into_iter().for_each(|(channel, nick)| {
        event_line(session, message, Target::Channel(channel.clone()), out);
        out.event(Event::MemberLeft {
            buffer: session.buffer(Target::Channel(channel)),
            nick,
        });
    });
}

/// Removes a member from every channel; returns the channels' names and
/// the member's spelling in each.
fn remove_everywhere(channels: &mut Channels, nick_key: &str) -> Vec<(String, String)> {
    channels
        .values_mut()
        .filter_map(|channel| {
            channel
                .members
                .remove(nick_key)
                .map(|member| (channel.name.clone(), member.nick))
        })
        .collect()
}

pub(super) fn nick(session: &mut Session, message: &Message, out: &mut Output) {
    let old = message.nick().unwrap_or_default().to_string();
    super::metadata::clear(session, &old, out);
    let new = message.param(0).to_string();
    let own = session.is_own(&old);
    let keys = (session.fold(&old), session.fold(&new));
    let renamed = rename_everywhere(&mut session.channels, &keys, &new);
    rename_query(&mut session.queries, &keys);
    out.events.extend(own.then(|| {
        session.reg.nick = new.clone();
        Event::OwnNick {
            network: session.config.id.clone(),
            nick: new.clone(),
        }
    }));
    renamed.into_iter().for_each(|(channel, old_nick, member)| {
        event_line(session, message, Target::Channel(channel.clone()), out);
        out.event(Event::MemberChanged {
            buffer: session.buffer(Target::Channel(channel)),
            old_nick,
            member,
        });
    });
}

/// Renames a member in every channel; returns each channel's name, the
/// member's previous spelling there and the renamed member.
fn rename_everywhere(
    channels: &mut Channels,
    (old, new): &(String, String),
    nick: &str,
) -> Vec<(String, String, Member)> {
    channels
        .values_mut()
        .filter_map(|channel| {
            let mut member = channel.members.remove(old)?;
            let old_nick = std::mem::replace(&mut member.nick, nick.to_string());
            channel.members.insert(new.clone(), member.clone());
            Some((channel.name.clone(), old_nick, member))
        })
        .collect()
}

fn rename_query(queries: &mut std::collections::BTreeSet<String>, (old, new): &(String, String)) {
    if queries.remove(old) {
        queries.insert(new.clone());
    }
}

/// The event's line in `target`, when the message describes one.
pub(super) fn event_line(session: &Session, message: &Message, target: Target, out: &mut Output) {
    let line: Option<Line> = describe::event(message)
        .map(|(kind, text)| lines::line(session, message, target, kind, text));
    out.events.extend(line.map(Event::Line));
}
