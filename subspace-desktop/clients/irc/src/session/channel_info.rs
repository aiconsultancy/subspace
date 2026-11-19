//! Topics, NAMES and WHO replies, MODE prefix changes, and member state
//! from `account-notify`, `away-notify` and `invite-notify`.

use std::collections::BTreeMap;

use super::channels::{self, Channel};
use super::membership::event_line;
use super::{Output, Session};
use crate::model::{Event, Member, Target};
use crate::proto::Message;

/// The `WHOX` token we send, so replies to other `WHO`s are ignored.
const WHOX_TOKEN: &str = "42";

/// TOPIC, 332 (topic) and 331 (no topic) for `channel`.
fn set_topic(
    session: &mut Session,
    channel: &str,
    topic: &str,
    set_by: Option<String>,
    out: &mut Output,
) {
    let key = session.fold(channel);
    let name =
        store_topic(&mut session.channels, &key, topic).unwrap_or_else(|| channel.to_string());
    out.event(Event::Topic {
        buffer: session.buffer(Target::Channel(name)),
        topic: topic.to_string(),
        set_by,
    });
}

fn store_topic(channels: &mut BTreeMap<String, Channel>, key: &str, topic: &str) -> Option<String> {
    let channel = channels.get_mut(key)?;
    channel.topic = topic.to_string();
    Some(channel.name.clone())
}

pub(super) fn topic(session: &mut Session, message: &Message, out: &mut Output) {
    let channel = message.param(0).to_string();
    let set_by = message.nick().map(str::to_string);
    set_topic(session, &channel, message.param(1), set_by, out);
    event_line(session, message, Target::Channel(channel), out);
}

pub(super) fn topic_reply(session: &mut Session, message: &Message, out: &mut Output) {
    let channel = message.param(1).to_string();
    set_topic(session, &channel, message.trailing(), None, out);
}

pub(super) fn no_topic(session: &mut Session, message: &Message, out: &mut Output) {
    let channel = message.param(1).to_string();
    set_topic(session, &channel, "", None, out);
}

/// 333: who set the topic, as `nick` or `nick!user@host`.
pub(super) fn topic_who(session: &mut Session, message: &Message, out: &mut Output) {
    let channel = message.param(1).to_string();
    let setter = message.param(2);
    let setter = setter.split('!').next().unwrap_or(setter).to_string();
    let topic = session
        .channels
        .get(&session.fold(&channel))
        .map(|channel| channel.topic.clone())
        .unwrap_or_default();
    set_topic(session, &channel, &topic, Some(setter), out);
}

/// 353: one batch of `NAMES` entries.
pub(super) fn names(session: &mut Session, message: &Message, _: &mut Output) {
    let key = session.fold(message.param(2));
    let symbols = session.isupport.symbols();
    let entries: Vec<(String, Member)> = message
        .trailing()
        .split_whitespace()
        .map(|entry| channels::names_entry(entry, &symbols))
        .map(|member| (session.fold(&member.nick), member))
        .collect();
    add_names(session.channels.get_mut(&key), entries);
}

fn add_names(channel: Option<&mut Channel>, entries: Vec<(String, Member)>) {
    if let Some(channel) = channel {
        channel.names.get_or_insert_default().extend(entries);
    }
}

/// 366: the `NAMES` list is complete; then ask `WHO` for away and account.
pub(super) fn end_of_names(session: &mut Session, message: &Message, out: &mut Output) {
    let channel = message.param(1).to_string();
    let key = session.fold(&channel);
    let whox = session.isupport.whox;
    let finished = finish_names(&mut session.channels, &key, whox);
    members_event(session, &key, out);
    out.send.extend(finished.then(|| who(&channel)));
}

fn who(channel: &str) -> Message {
    Message::new("WHO", [channel.to_string(), format!("%tnfa,{WHOX_TOKEN}")])
}

/// Replaces the members with the finished `NAMES` reply, keeping known away
/// and account state. True when a `WHO` should follow.
fn finish_names(channels: &mut BTreeMap<String, Channel>, key: &str, whox: bool) -> bool {
    let Some(channel) = channels.get_mut(key) else {
        return false;
    };
    let mut names = channel.names.take().unwrap_or_default();
    names.iter_mut().for_each(|(nick, member)| {
        if let Some(known) = channel.members.get(nick) {
            member.away = known.away;
            member.account = known.account.clone();
        }
    });
    channel.members = names;
    channel.who = whox;
    whox
}

fn members_event(session: &Session, key: &str, out: &mut Output) {
    let symbols = session.isupport.symbols();
    let event = session.channels.get(key).map(|channel| Event::Members {
        buffer: session.buffer(Target::Channel(channel.name.clone())),
        members: channels::sorted(&channel.members, &symbols),
    });
    out.events.extend(event);
}

/// 354 with our token: `<token> <nick> <flags> <account>`.
pub(super) fn whox(session: &mut Session, message: &Message, _: &mut Output) {
    let mut fields = message.params.iter().skip(1).map(String::as_str);
    let ours = fields.next() == Some(WHOX_TOKEN);
    let nick_key = session.fold(fields.next().unwrap_or_default());
    let away = fields.next().is_some_and(|flags| flags.contains('G'));
    let account = fields
        .next()
        .filter(|account| *account != "0" && !account.is_empty());
    let account = account.map(str::to_string);
    session
        .channels
        .values_mut()
        .filter(|channel| ours && channel.who)
        .filter_map(|channel| channel.members.get_mut(&nick_key))
        .for_each(|member| {
            member.away = away;
            member.account = account.clone();
        });
}

/// 315: the `WHO` reply for a channel is complete.
pub(super) fn end_of_who(session: &mut Session, message: &Message, out: &mut Output) {
    let key = session.fold(message.param(1));
    let pending = session
        .channels
        .get_mut(&key)
        .is_some_and(|channel| std::mem::take(&mut channel.who));
    pending
        .then_some(key)
        .iter()
        .for_each(|key| members_event(session, key, out));
}
