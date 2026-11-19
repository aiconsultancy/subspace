//! MODE prefix changes and member state from `account-notify`,
//! `away-notify` and `invite-notify`.

use std::collections::BTreeMap;

use super::channels::{self, Channel};
use super::isupport::Isupport;
use super::membership::event_line;
use super::{Output, Session, describe, lines};
use crate::model::{Event, Member, Target};
use crate::proto::Message;

/// One mode letter set (`true`) or unset, with its parameter.
type ModeChange = (bool, char, Option<String>);

pub(super) fn mode(session: &mut Session, message: &Message, out: &mut Output) {
    let channel = session
        .isupport
        .channel(message.param(0))
        .map(str::to_string);
    let target = channel.clone().map_or(Target::Server, Target::Channel);
    let changes = mode_changes(
        message.params.get(1..).unwrap_or_default(),
        &session.isupport,
    );
    let key = session.fold(channel.as_deref().unwrap_or_default());
    let folded: Vec<Option<String>> = changes
        .iter()
        .map(|(_, _, param)| param.as_deref().map(|nick| session.fold(nick)))
        .collect();
    let symbols = session.isupport.symbols();
    let changed = session
        .channels
        .get_mut(&key)
        .map(|channel| apply_prefixes(channel, &changes, &folded, &symbols))
        .unwrap_or_default();
    let own = session.is_own(message.param(0));
    let (kind, text) = describe::mode(message, channel.is_none() && own);
    out.event(Event::Line(lines::line(
        session,
        message,
        target.clone(),
        kind,
        text,
    )));
    changed.into_iter().for_each(|member| {
        out.event(Event::MemberChanged {
            buffer: session.buffer(target.clone()),
            old_nick: member.nick.clone(),
            member,
        });
    });
}

/// The changes in `+o-v nick nick`: prefix modes carry the prefix symbol in
/// place of the letter.
fn mode_changes(params: &[String], isupport: &Isupport) -> Vec<ModeChange> {
    let prefix = &isupport.prefix;
    let Some((modes, args)) = params.split_first() else {
        return Vec::new();
    };
    let mut args = args.iter();
    let mut adding = true;
    let mut changes = Vec::new();
    for mode in modes.chars() {
        match mode {
            '+' => adding = true,
            '-' => adding = false,
            _ => {
                let takes = prefix.iter().any(|(m, _)| *m == mode)
                    || isupport.list_modes.contains(mode)
                    || isupport.param_modes.contains(mode)
                    || (adding && isupport.set_param_modes.contains(mode));
                let param = takes.then(|| args.next().cloned()).flatten();
                let letter = prefix
                    .iter()
                    .find(|(m, _)| *m == mode)
                    .map_or(mode, |(_, symbol)| *symbol);
                changes.push((adding, letter, param));
            }
        }
    }
    changes
}

/// Applies prefix changes (`folded` holds each change's folded parameter);
/// returns the changed members.
fn apply_prefixes(
    channel: &mut Channel,
    changes: &[ModeChange],
    folded: &[Option<String>],
    symbols: &[char],
) -> Vec<Member> {
    let mut changed: BTreeMap<String, Member> = BTreeMap::new();
    changes
        .iter()
        .zip(folded)
        .filter(|((_, symbol, _), _)| symbols.contains(symbol))
        .for_each(|((adding, symbol, _), key)| {
            let member = key.as_ref().and_then(|key| channel.members.get_mut(key));
            if let (Some(member), Some(key)) = (member, key) {
                member.prefixes =
                    channels::change_prefix(&member.prefixes, *symbol, *adding, symbols);
                changed.insert(key.clone(), member.clone());
            }
        });
    changed.into_values().collect()
}

pub(super) fn account(session: &mut Session, message: &Message, out: &mut Output) {
    let account = message.param(0);
    let account = (account != "*" && !account.is_empty()).then(|| account.to_string());
    update_member(session, message, out, |member| {
        member.account = account.clone()
    });
}

pub(super) fn away(session: &mut Session, message: &Message, out: &mut Output) {
    let away = !message.params.is_empty();
    update_member(session, message, out, |member| member.away = away);
}

fn update_member(
    session: &mut Session,
    message: &Message,
    out: &mut Output,
    update: impl Fn(&mut Member),
) {
    let key = session.fold(message.nick().unwrap_or_default());
    let changed: Vec<(String, Member)> = session
        .channels
        .values_mut()
        .filter_map(|channel| {
            let member = channel.members.get_mut(&key)?;
            update(member);
            Some((channel.name.clone(), member.clone()))
        })
        .collect();
    changed.into_iter().for_each(|(channel, member)| {
        out.event(Event::MemberChanged {
            buffer: session.buffer(Target::Channel(channel)),
            old_nick: member.nick.clone(),
            member,
        });
    });
}

/// INVITE: to us in the server buffer, otherwise (`invite-notify`) in the
/// channel.
pub(super) fn invite(session: &mut Session, message: &Message, out: &mut Output) {
    let to_us = session.is_own(message.param(0));
    event_line(
        session,
        message,
        invite_target(to_us, message.param(1)),
        out,
    );
}

fn invite_target(to_us: bool, channel: &str) -> Target {
    if to_us {
        Target::Server
    } else {
        Target::Channel(channel.to_string())
    }
}
