//! Which progress notices Channels trusts, and so folds.
//!
//! The tag is the sender's to choose, so a notice is trusted only when the
//! server vouches for who sent it. It must be a recognised progress notice
//! ([`super::marker`]) that replies to a request this buffer holds: a
//! message or action, and in a private conversation our own. The request
//! names the agent it asks: a channel request's first word is `nick:`,
//! `nick,` or `@nick`, and a private conversation's is its partner; a
//! request that asks its own sender is none. The server reports that nick's
//! account in the member lists (`WHOX`, `extended-join` and
//! `account-notify`): the channel's own, or, for a private conversation,
//! those of the network's channels. The request is pinned to the account
//! reported when Channels first sees it with one ([`Requests`]), and the
//! notice's `account` tag, which only the server sets, must name that
//! account, whoever holds the nick by then. Without any of these the notice
//! is not trusted.

use std::collections::HashMap;

use subspace_client_irc::{BufferKey, Line, LineKind, Target};

use super::requests::{Arrival, Asked, Notice, Requests};
use super::{Phase, marker};
use crate::state::{Buffer, IrcState};

/// What live `line`, to be held as `seq`, brings to its buffer's requests:
/// progress, trusted or not, or a request with the account its agent holds
/// now.
pub(crate) fn arrival(state: &IrcState, line: &Line, seq: i64) -> Arrival {
    let requests = state.buffer(&line.buffer).map(|held| &held.requests);
    if let Some(notice) = notice(line, seq, requests) {
        return Arrival::Progress(notice);
    }
    request(line).map_or(Arrival::Other, |(id, nick)| {
        let account = reported_account(state, &line.buffer, &nick).map(str::to_owned);
        Arrival::Request(Asked {
            id: id.to_owned(),
            nick,
            account,
        })
    })
}

/// Every request buffer `key` holds, with the account the server reports
/// now for each one not yet pinned.
pub(crate) fn asked(state: &IrcState, key: &BufferKey) -> Vec<Asked> {
    let Some(held) = state.buffer(key) else {
        return Vec::new();
    };
    let mut accounts: HashMap<String, Option<String>> = HashMap::new();
    held.entries
        .iter()
        .filter_map(|entry| request(&entry.line))
        .map(|(id, nick)| {
            let account = match held.requests.pin(id) {
                Some(_) => None,
                None => accounts
                    .entry(nick.clone())
                    .or_insert_with(|| reported_account(state, key, &nick).map(str::to_owned))
                    .clone(),
            };
            Asked {
                id: id.to_owned(),
                nick,
                account,
            }
        })
        .collect()
}

/// Every progress notice `held` holds, trusted against its requests.
pub(crate) fn notices(held: &Buffer) -> Vec<Notice> {
    held.entries
        .iter()
        .filter_map(|entry| notice(&entry.line, entry.seq, Some(&held.requests)))
        .collect()
}

/// After a member list change in buffer `changed`, the waiting requests'
/// agents the server now reports an account for: in each buffer whose
/// people that list speaks for, each waiting nick (only `nick` when given)
/// with its account.
pub(crate) fn reported(
    state: &IrcState,
    changed: &BufferKey,
    nick: Option<&str>,
) -> Vec<(BufferKey, String, String)> {
    let mut found = Vec::new();
    for (key, held) in state.buffers() {
        if !shares_members(changed, key) {
            continue;
        }
        for waiting in held.requests.waiting() {
            let account = nick
                .is_none_or(|nick| nick == waiting)
                .then(|| reported_account(state, key, waiting))
                .flatten();
            if let Some(account) = account {
                found.push((key.clone(), waiting.to_owned(), account.to_owned()));
            }
        }
    }
    found
}

/// `line` as progress on the request it answers: trusted when `requests`
/// holds that request, pinned, and the notice comes from the pinned
/// account. `None` when it is no progress notice.
fn notice(line: &Line, seq: i64, requests: Option<&Requests>) -> Option<Notice> {
    let marker = marker(line)?;
    let request = line.reply_to.clone()?;
    let pinned = requests
        .filter(|requests| requests.held(&request))
        .and_then(|requests| requests.pin(&request));
    Some(Notice {
        seq,
        trusted: sent_by(line, pinned),
        ended: marker.phase == Phase::Finished,
        request,
    })
}

/// `line` as a request: a message or action with a message ID that asks
/// an agent other than its sender. Its ID, and the nick it asks in ASCII
/// lower case.
fn request(line: &Line) -> Option<(&str, String)> {
    let eligible = matches!(line.kind, LineKind::Privmsg | LineKind::Action);
    let id = line.id.as_deref().filter(|id| eligible && !id.is_empty())?;
    let nick = addressee(line)?.to_ascii_lowercase();
    let own = line
        .from
        .as_deref()
        .is_some_and(|from| from.eq_ignore_ascii_case(&nick));
    (!own).then_some((id, nick))
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
