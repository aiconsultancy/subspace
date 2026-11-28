//! The reducer: one function per runtime event.

use std::collections::{HashSet, VecDeque};

use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, Event, Line, LineKind, Member, NetworkId, Target, Typing};

use super::{
    Attention, AttentionKind, Entry, HISTORY_MAX, HistoryState, IrcState, LINE_CAP, Topic, counts,
    folding, typing_expiry,
};

mod status;

/// Applies `event`, then folds any request progress it made trustworthy.
pub(super) fn event(state: &mut IrcState, event: Event, now: DateTime<Utc>) -> Option<Attention> {
    let scope = folding::scope(&event);
    let attention = dispatch(state, event, now);
    scope.iter().for_each(|scope| folding::refold(state, scope));
    attention
}

fn dispatch(state: &mut IrcState, event: Event, now: DateTime<Utc>) -> Option<Attention> {
    match event {
        Event::NickStatus {
            network,
            nick,
            status,
        } => nick_status(state, &network, &nick, status.as_deref()),
        Event::Line(line) => return append(state, line),
        // A bouncer's replay counts for the badges but is not news.
        Event::Replayed(line) => {
            append(state, line);
        }
        Event::Status { network, status } => status::apply(state, &network, status, now),
        Event::History {
            buffer,
            lines,
            complete,
        } => history(state, &buffer, lines, complete),
        Event::Backfill { buffer, lines } | Event::Around { buffer, lines }
            if !lines.is_empty() =>
        {
            super::merge::merge(state, &buffer, lines, true);
        }
        Event::Typing {
            buffer,
            nick,
            state: typing,
        } => self::typing(state, buffer, nick, typing, now),
        Event::ChannelList { network, channels } => listed(state, &network, channels, now),
        Event::Sent { id, result, .. } => {
            state.sends.answers.insert(id, result);
        }
        other => membership(state, other),
    }
    None
}

/// Events that change who is where, without lines.
fn membership(state: &mut IrcState, event: Event) {
    match event {
        Event::Capabilities { network, enabled } => {
            if let Some(network) = network_mut(state, &network) {
                network.caps = enabled;
            }
        }
        Event::OwnNick { network, nick } => {
            if let Some(network) = network_mut(state, &network) {
                network.nick = nick;
            }
        }
        Event::ReadMarker { buffer, time } => read_marker(state, &buffer, time),
        other => buffer_membership(state, other),
    }
}

/// Joins, departures, topics and member lists of one buffer.
fn buffer_membership(state: &mut IrcState, event: Event) {
    match event {
        Event::Joined { buffer } => joined(state, buffer),
        Event::Left { buffer } => {
            if let Some(buffer) = state.buffers.get_mut(&buffer) {
                buffer.joined = false;
                buffer.members.clear();
            }
        }
        Event::Topic {
            buffer,
            topic,
            set_by,
        } => {
            state.buffers.entry(buffer).or_default().topic = Some(Topic {
                text: topic,
                set_by,
            })
        }
        Event::Members { buffer, members } => {
            state.buffers.entry(buffer).or_default().members = members;
        }
        Event::MemberJoined { buffer, member } => member_joined(state, buffer, member),
        Event::MemberLeft { buffer, nick } => {
            member_left(state, &buffer, &nick);
            status_if_gone(state, &buffer.network, &nick);
        }
        Event::MemberChanged {
            buffer,
            old_nick,
            member,
        } => {
            let network = buffer.network.clone();
            member_left(state, &buffer, &old_nick);
            member_joined(state, buffer, member);
            status_if_gone(state, &network, &old_nick);
        }
        _ => {}
    }
}

/// We joined `buffer`. A private conversation from `/query`, or a channel
/// joined from the list, is shown.
fn joined(state: &mut IrcState, buffer: BufferKey) {
    if state.joins.show(&buffer) || matches!(buffer.target, Target::Query(_)) {
        state.opened = Some(buffer.clone());
    }
    state.buffers.entry(buffer).or_default().joined = true;
}

/// A network's new channel list, kept while the network is configured.
fn listed(
    state: &mut IrcState,
    network: &NetworkId,
    channels: Vec<subspace_client_irc::ListedChannel>,
    now: DateTime<Utc>,
) {
    if state.network(network).is_some() {
        state.directories.replace(network, channels, now);
    }
}

fn network_mut<'a>(state: &'a mut IrcState, id: &NetworkId) -> Option<&'a mut super::Network> {
    state.networks.iter_mut().find(|network| &network.id == id)
}

/// Member events carry the spelling the runtime stored for each member, so
/// nicks compare exactly; only the runtime knows the network's case mapping.
fn member_joined(state: &mut IrcState, buffer: BufferKey, member: Member) {
    let members = &mut state.buffers.entry(buffer).or_default().members;
    members.retain(|known| known.nick != member.nick);
    members.push(member);
}

fn member_left(state: &mut IrcState, buffer: &BufferKey, nick: &str) {
    if let Some(buffer) = state.buffers.get_mut(buffer) {
        buffer.members.retain(|known| known.nick != nick);
    }
}

/// A nick's status set, changed or cleared on `network`. It is held for the
/// whole nick, never for a buffer or a message, and cleaned again on the way
/// in: the Space does not rely on the runtime having done it.
fn nick_status(state: &mut IrcState, network: &NetworkId, nick: &str, status: Option<&str>) {
    if let Some(network) = network_mut(state, network) {
        network.statuses.set(nick, status);
    }
}

/// After `nick` left a channel of `network`: when no channel of that
/// network we hold still lists it, its status goes. The runtime clears a
/// status when the last channel we share with the nick ends, so this only
/// keeps the Space right should that clear come late. A private
/// conversation's peer keeps a status it set while sharing no channel,
/// because no channel departure concerns it.
fn status_if_gone(state: &mut IrcState, network: &NetworkId, nick: &str) {
    // Most departures concern a nick without a status: skip the scan.
    let held = state
        .network(network)
        .is_some_and(|held| held.statuses.get(nick).is_some());
    if held
        && !in_a_channel(state, network, nick)
        && let Some(network) = network_mut(state, network)
    {
        network.statuses.remove(nick);
    }
}

/// Whether a channel of `network` we hold lists `nick`, ignoring ASCII case.
fn in_a_channel(state: &IrcState, network: &NetworkId, nick: &str) -> bool {
    state.buffers.iter().any(|(key, held)| {
        &key.network == network
            && held
                .members
                .iter()
                .any(|member| member.nick.eq_ignore_ascii_case(nick))
    })
}

fn read_marker(state: &mut IrcState, key: &BufferKey, time: DateTime<Utc>) {
    let Some(buffer) = state.buffers.get_mut(key) else {
        return;
    };
    if buffer.read_marker.is_some_and(|marker| marker >= time) {
        return;
    }
    buffer.read_marker = Some(time);
    counts::recount(buffer);
}

fn typing(
    state: &mut IrcState,
    buffer: BufferKey,
    nick: String,
    typing: Typing,
    now: DateTime<Utc>,
) {
    let notices = &mut state.buffers.entry(buffer).or_default().typing;
    match typing_expiry(typing, now) {
        Some(expires) => notices.insert(nick, expires),
        None => notices.remove(&nick),
    };
}

/// A live line: appended, counted, and reported when it may need attention.
/// Trusted progress folds into its request's status line and never counts.
fn append(state: &mut IrcState, line: Line) -> Option<Attention> {
    if super::merge::known(state, &line) {
        return None;
    }
    let highlight = super::is_highlight(&line, &state.highlights);
    let key = line.buffer.clone();
    folding::make_room(state, &key);
    let seq = state.next_seq;
    state.next_seq += 1;
    let arrival = crate::thread_status::arrival(state, &line, seq);
    let buffer = state.buffers.entry(key.clone()).or_default();
    // A line ends its sender's typing notice but not their status, which
    // the gateway clears itself when the turn ends.
    if let Some(from) = &line.from {
        buffer.typing.remove(from);
    }
    if matches!(line.kind, LineKind::Join) && line.own {
        buffer.joined = true;
    }
    let entry = Entry {
        seq,
        line,
        highlight,
    };
    let unread = counts::count(buffer, &key, &entry, arrival.folds());
    let attention = unread.then(|| attention(&entry)).flatten();
    buffer.entries.push_back(entry);
    folding::arrived(state, &key, &arrival);
    attention
}

/// What an unread line asks for: a mention anywhere, or any message in a
/// private conversation. Being unread already rules out our own lines,
/// membership noise, the server buffer and lines the read marker covers.
fn attention(entry: &Entry) -> Option<Attention> {
    let private = matches!(entry.line.buffer.target, Target::Query(_));
    let kind = match (entry.highlight, private) {
        (true, _) => AttentionKind::Highlight,
        (false, true) => AttentionKind::Query,
        (false, false) => return None,
    };
    Some(Attention {
        buffer: entry.line.buffer.clone(),
        kind,
        from: entry.line.from.clone(),
    })
}

/// Older lines, oldest first, placed before what the buffer holds. Lines
/// the buffer already has (by `msgid`, or not older than its oldest line)
/// and repeats within the page are skipped.
///
/// Live lines keep a buffer at `LINE_CAP`, dropping the oldest. History the
/// user loads raises that buffer's cap by the lines it added, so the next
/// live line does not drop them again, up to `HISTORY_MAX` in total. A page
/// that would pass `HISTORY_MAX` keeps only its newest lines that fit, and
/// the buffer offers no older history after that. Nothing already held is
/// dropped here, so counts and scroller rows stay consistent.
fn history(state: &mut IrcState, key: &BufferKey, lines: Vec<Line>, complete: bool) {
    let words = state.highlights.clone();
    let buffer = state.buffers.entry(key.clone()).or_default();
    let fresh = unseen(&buffer.entries, lines);
    let room = HISTORY_MAX.saturating_sub(buffer.entries.len());
    let skip = fresh.len().saturating_sub(room);
    if complete || skip > 0 {
        buffer.history.state = HistoryState::Complete;
    } else if buffer.history.state == HistoryState::Loading {
        buffer.history.state = HistoryState::Idle;
    }
    for line in fresh.into_iter().skip(skip).rev() {
        state.first_seq -= 1;
        let highlight = super::is_highlight(&line, &words);
        let entry = Entry {
            seq: state.first_seq,
            line,
            highlight,
        };
        let buffer = state.buffers.entry(key.clone()).or_default();
        counts::count_history(buffer, key, &entry);
        buffer.entries.push_front(entry);
    }
    let buffer = state.buffers.entry(key.clone()).or_default();
    buffer.history.extra = buffer.entries.len().saturating_sub(LINE_CAP);
}

/// The history lines that are new: not held (by `msgid`, or for lines
/// without one, not at or after the oldest held line) and not repeated
/// earlier in the page.
fn unseen(entries: &VecDeque<Entry>, lines: Vec<Line>) -> Vec<Line> {
    let oldest = entries.front().map(|entry| entry.line.time);
    let mut known: HashSet<String> = entries
        .iter()
        .filter_map(|entry| entry.line.id.clone())
        .collect();
    lines
        .into_iter()
        .filter(|line| match &line.id {
            Some(id) => known.insert(id.clone()),
            None => oldest.is_none_or(|oldest| line.time < oldest),
        })
        .collect()
}
