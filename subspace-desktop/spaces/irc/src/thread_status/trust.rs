//! Which progress notices Channels trusts, and so folds.
//!
//! The tag is the sender's to choose, so a notice is trusted only when the
//! server vouches for who sent it. It must be a recognised progress notice
//! ([`super::marker`]) that replies to a request this buffer holds: a
//! message or action, and in a private conversation our own. The request
//! names the agent it asks: a channel request's first word is `nick:`,
//! `nick,` or `@nick`, and a private conversation's is its partner. The
//! server reports that nick's account in the member lists (`WHOX`,
//! `extended-join` and `account-notify`): the channel's own, or, for a
//! private conversation, those of the network's channels. The notice's
//! `account` tag, which only the server sets, must name that account.
//! Without any of these the notice is not trusted.

use std::collections::{BTreeMap, HashSet};

use subspace_client_irc::{BufferKey, Line, LineKind, Target};

use super::marker;
use crate::state::{Buffer, IrcState};

/// The sequences of the lines held in buffer `key` that are trusted
/// progress in `state`.
pub(crate) fn trusted(state: &IrcState, key: &BufferKey) -> HashSet<i64> {
    let Some(held) = state.buffer(key) else {
        return HashSet::new();
    };
    let requests = requests(held);
    held.entries
        .iter()
        .filter(|entry| vouched(state, &entry.line, &requests))
        .map(|entry| entry.seq)
        .collect()
}

/// Whether `line`, before it is held, is trusted progress in `state`.
pub(crate) fn folds(state: &IrcState, line: &Line) -> bool {
    let requests = state.buffer(&line.buffer).map(requests).unwrap_or_default();
    vouched(state, line, &requests)
}

/// The held lines a progress notice may answer, by message ID: messages
/// and actions.
fn requests(held: &Buffer) -> BTreeMap<&str, &Line> {
    held.entries
        .iter()
        .map(|entry| &entry.line)
        .filter(|line| matches!(line.kind, LineKind::Privmsg | LineKind::Action))
        .filter_map(|line| Some((line.id.as_deref()?, line)))
        .collect()
}

/// A recognised progress notice that answers a held request and comes from
/// the account the server reports for the agent that request asks.
fn vouched(state: &IrcState, line: &Line, requests: &BTreeMap<&str, &Line>) -> bool {
    let expected = marker(line)
        .and_then(|_| requests.get(line.reply_to.as_deref()?).copied())
        .and_then(|request| expected_account(state, request));
    sent_by(line, expected)
}

/// The account the server reports for the agent `request` asks.
fn expected_account<'s>(state: &'s IrcState, request: &Line) -> Option<&'s str> {
    let nick = addressee(request)?;
    reported_account(state, &request.buffer, &nick)
}

/// Whom `request` asks: the partner of a private conversation, for our own
/// request there, or the nick a channel request's first word addresses,
/// written `nick:`, `nick,` or `@nick`.
fn addressee(request: &Line) -> Option<String> {
    match &request.buffer.target {
        Target::Query(partner) => request.own.then(|| partner.clone()),
        Target::Channel(_) => {
            let plain = subspace_client_irc::format::strip(&request.text);
            let first = plain.split_whitespace().next()?;
            let nick = first
                .strip_prefix('@')
                .or_else(|| first.strip_suffix([':', ',']))?
                .trim_end_matches([':', ',']);
            (!nick.is_empty()).then(|| nick.to_owned())
        }
        Target::Server => None,
    }
}

/// The account the server reports for `nick` in the member lists buffer
/// `buffer` may use: its own channel's, or, for a private conversation,
/// every channel's on its network.
fn reported_account<'s>(state: &'s IrcState, buffer: &BufferKey, nick: &str) -> Option<&'s str> {
    state
        .buffers()
        .filter(|(key, _)| shares_members(key, buffer))
        .flat_map(|(_, held)| held.members.iter())
        .filter(|member| member.nick.eq_ignore_ascii_case(nick))
        .find_map(|member| member.account.as_deref())
}

/// Whether buffer `key`'s member list speaks for `buffer`'s people.
fn shares_members(key: &BufferKey, buffer: &BufferKey) -> bool {
    match &buffer.target {
        Target::Channel(_) => key == buffer,
        _ => key.network == buffer.network && matches!(key.target, Target::Channel(_)),
    }
}

/// The server's `account` tag on `line` names `expected`.
fn sent_by(line: &Line, expected: Option<&str>) -> bool {
    matches!(
        (line.account.as_deref(), expected),
        (Some(sent), Some(expected)) if sent.eq_ignore_ascii_case(expected)
    )
}
