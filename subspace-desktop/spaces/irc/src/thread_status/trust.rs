//! Which progress notices Channels trusts, and so folds.
//!
//! The tag is the sender's to choose, so a notice is trusted only when the
//! server vouches for who sent it. It must be a recognised progress notice
//! ([`super::marker`]) that replies to a request this buffer holds: a
//! message or action, and in a private conversation our own. The request
//! names the agent it asks: a channel request's first word is `nick:`,
//! `nick,` or `@nick`, and a private conversation's is its partner; a
//! request that asks its own sender is none. The request is bound to an
//! account ([`Requests`]): the one the member lists (`WHOX`,
//! `extended-join` and `account-notify`) report for that nick when the
//! request arrives live, or else the server `account` tag on the earliest
//! progress notice that nick sends on it, never the requester's own. The
//! notice's `account` tag, which only the server sets, must name that
//! account, whoever holds the nick by then. Without a binding nothing
//! folds.

use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, Line, LineKind, Target};

use super::requests::{Arrival, Asked, Notice};
use super::{Phase, marker};
use crate::state::{Buffer, IrcState};

/// What live `line`, to be held as `seq`, brings to its buffer's requests:
/// progress, trusted or not, or a request with the account the member
/// lists report now for the agent it asks.
pub(crate) fn arrival(state: &IrcState, line: &Line, seq: i64) -> Arrival {
    if let Some(notice) = notice(line, seq) {
        let trusted = state
            .buffer(&line.buffer)
            .is_some_and(|held| held.requests.trusts(&notice));
        return Arrival::Progress { notice, trusted };
    }
    request(line).map_or(Arrival::Other, |(id, nick)| {
        let account = reported_account(state, &line.buffer, &nick).map(str::to_owned);
        Arrival::Request(Asked {
            id: id.to_owned(),
            nick,
            requester: line.account.clone(),
            account,
        })
    })
}

/// Every request `held` holds. Only a live arrival reads the member lists,
/// so none carries a reported account.
pub(crate) fn asked(held: &Buffer) -> Vec<Asked> {
    held.entries
        .iter()
        .filter_map(|entry| {
            let (id, nick) = request(&entry.line)?;
            Some(Asked {
                id: id.to_owned(),
                nick,
                requester: entry.line.account.clone(),
                account: None,
            })
        })
        .collect()
}

/// Every progress notice `held` holds, oldest first by server time; of two
/// at the same time, the earlier held.
pub(crate) fn notices(held: &Buffer) -> Vec<Notice> {
    let mut found: Vec<(DateTime<Utc>, Notice)> = held
        .entries
        .iter()
        .filter_map(|entry| Some((entry.line.time, notice(&entry.line, entry.seq)?)))
        .collect();
    found.sort_by_key(|(time, _)| *time);
    found.into_iter().map(|(_, notice)| notice).collect()
}

/// `line` as progress on the request it answers. `None` when it is no
/// progress notice.
fn notice(line: &Line, seq: i64) -> Option<Notice> {
    let marker = marker(line)?;
    Some(Notice {
        seq,
        request: line.reply_to.clone()?,
        ended: marker.phase == Phase::Finished,
        from: line.from.as_deref().map(str::to_ascii_lowercase),
        account: line.account.clone(),
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
