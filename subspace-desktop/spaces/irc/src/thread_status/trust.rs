//! Which progress notices Channels trusts, and so folds.
//!
//! The tag is the sender's to choose, so a notice is trusted only when the
//! server vouches for who sent it. It must be a recognised progress notice
//! ([`super::marker`]) that replies to a request this buffer holds: a
//! message or action, and in a private conversation our own. The request
//! names the agent it asks: a channel request leads with `@nick` or has
//! one distinct `@nick` elsewhere. Leading `nick:` and `nick,` also work.
//! A private conversation's request names its partner. The
//! notice's `account` tag, which only the server sets, must equal that
//! nick, ignoring ASCII case. A request that asks its own sender, by nick
//! or by the sender's account, is none.
//!
//! Owner decision, 21 November 2026: a notice is trusted when its server
//! account equals the addressed nick. Fleet IRC servers are private and
//! tie each nick to the account of the same name, so only the agent's own
//! account can send as its nick there.

use subspace_client_irc::{Line, LineKind, Target};

use super::requests::{Arrival, Asked, Ending, Notice};
use super::{Phase, marker};
use crate::markup::{is_nick, is_nick_char};
use crate::state::{Buffer, IrcState};

/// What live `line`, to be held as `seq`, brings to its buffer's requests:
/// progress, trusted or not, or a request.
pub(crate) fn arrival(state: &IrcState, line: &Line, seq: i64) -> Arrival {
    if let Some(notice) = notice(line, seq) {
        let trusted = state
            .buffer(&line.buffer)
            .is_some_and(|held| held.requests.trusts(&notice));
        return Arrival::Progress { notice, trusted };
    }
    request(line).map_or(Arrival::Other, |(id, nick)| {
        Arrival::Request(Asked {
            id: id.to_owned(),
            nick,
        })
    })
}

/// Every request `held` holds.
pub(crate) fn asked(held: &Buffer) -> Vec<Asked> {
    held.entries
        .iter()
        .filter_map(|entry| {
            let (id, nick) = request(&entry.line)?;
            Some(Asked {
                id: id.to_owned(),
                nick,
            })
        })
        .collect()
}

/// Every progress notice `held` holds.
pub(crate) fn notices(held: &Buffer) -> Vec<Notice> {
    held.entries
        .iter()
        .filter_map(|entry| notice(&entry.line, entry.seq))
        .collect()
}

/// `line` as progress on the request it answers. `None` when it is no
/// progress notice.
fn notice(line: &Line, seq: i64) -> Option<Notice> {
    let marker = marker(line)?;
    Some(Notice {
        seq,
        request: line.reply_to.clone()?,
        ending: match marker.phase {
            Phase::Finished(outcome) => Some(Ending {
                outcome: Some(outcome),
                at: Some(line.time),
            }),
            _ => None,
        },
        account: line.account.clone(),
    })
}

/// `line` as a request: a message or action with a message ID that asks
/// an agent other than its sender, by nick or by the sender's account. Its
/// ID, and the nick it asks in ASCII lower case.
fn request(line: &Line) -> Option<(&str, String)> {
    let eligible = matches!(line.kind, LineKind::Privmsg | LineKind::Action);
    let id = line.id.as_deref().filter(|id| eligible && !id.is_empty())?;
    let nick = addressee(line)?.to_ascii_lowercase();
    let own = [line.from.as_deref(), line.account.as_deref()]
        .into_iter()
        .flatten()
        .any(|sender| sender.eq_ignore_ascii_case(&nick));
    (!own).then_some((id, nick))
}

/// Whom `request` asks: the partner of a private conversation, for our own
/// request there, or the nick addressed in a channel message.
fn addressee(request: &Line) -> Option<String> {
    match &request.buffer.target {
        Target::Query(partner) => request.own.then(|| partner.clone()),
        Target::Channel(_) => {
            let plain = subspace_client_irc::format::strip(&request.text);
            channel_addressee(&plain).map(str::to_owned)
        }
        Target::Server => None,
    }
}

/// A leading @mention or nick followed by a colon addresses the request
/// directly. Otherwise an explicit @mention takes precedence over a comma,
/// as in "Thanks, @pilot".
fn channel_addressee(text: &str) -> Option<&str> {
    let first = text.split_whitespace().next()?;
    first
        .strip_prefix('@')
        .and_then(|rest| {
            rest.split(|character| character != '@' && !is_nick_char(character))
                .next()
        })
        .filter(|nick| is_nick(nick))
        .or_else(|| first.strip_suffix(':').filter(|nick| is_nick(nick)))
        .or_else(|| inline_addressee(text))
        .or_else(|| first.strip_suffix(',').filter(|nick| is_nick(nick)))
}

/// One request timeline cannot describe two different agents' turns.
fn inline_addressee(text: &str) -> Option<&str> {
    let mut mentions = text
        .split(|character| character != '@' && !is_nick_char(character))
        .filter_map(|word| word.strip_prefix('@').filter(|nick| is_nick(nick)));
    let first = mentions.next()?;
    mentions
        .all(|nick| nick.eq_ignore_ascii_case(first))
        .then_some(first)
}
