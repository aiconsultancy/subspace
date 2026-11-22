use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, ConnectionStatus, Event, Line, LineKind, Member};

use super::{PROGRESS_TAG, Shown, statuses};
use crate::fixtures::{at, channel, config, line, member, message, network_id};
use crate::state::IrcState;

mod accounts;
mod endings;
mod lifecycle;
mod markers;
mod retention;
mod trust;

/// The fixture ship's account, as the server reports it: named after its
/// nick, as fleet servers require.
const SHIP: &str = "astra";

fn room() -> BufferKey {
    channel("#ops")
}

/// The ship `astra`, logged in to [`SHIP`].
fn ship() -> Member {
    let mut ship = member("astra", "+");
    ship.account = Some(SHIP.into());
    ship
}

/// Our request `id` to astra in `buffer`.
fn request(buffer: &BufferKey, id: &str, minute: i64) -> Line {
    let mut line = message(
        buffer,
        "subspace",
        &format!("astra: please handle {id}"),
        minute,
    );
    line.id = Some(id.into());
    line
}

/// Our request `id` in the thread of `root`.
fn follow_up(buffer: &BufferKey, id: &str, root: &str, minute: i64) -> Line {
    let mut line = request(buffer, id, minute);
    line.reply_to = Some(root.into());
    line
}

/// A tagged notice from `nick`, logged in to `account`, answering `request`.
fn notice_from(
    (nick, account): (&str, Option<&str>),
    buffer: &BufferKey,
    request: &str,
    value: &str,
    minute: i64,
) -> Line {
    let mut line = line(buffer, LineKind::Notice, nick, "Prose never read.", minute);
    line.id = Some(format!("{nick}-{request}-{minute}"));
    line.account = account.map(str::to_owned);
    line.reply_to = Some(request.into());
    line.client_tags.insert(PROGRESS_TAG.into(), value.into());
    line
}

/// The ship's progress notice on `request`.
fn progress(buffer: &BufferKey, request: &str, value: &str, minute: i64) -> Line {
    notice_from(("astra", Some(SHIP)), buffer, request, value, minute)
}

/// A visible message from astra; a reply to `request` when one is given.
fn said(buffer: &BufferKey, request: Option<&str>, id: &str, minute: i64) -> Line {
    let mut line = message(buffer, "astra", "Visible text.", minute);
    line.id = Some(id.into());
    line.account = Some(SHIP.into());
    line.reply_to = request.map(str::to_owned);
    line
}

fn status_event(status: ConnectionStatus) -> Event {
    Event::Status {
        network: network_id(),
        status,
    }
}

fn connected() -> Event {
    status_event(ConnectionStatus::Connected {
        nick: "subspace".into(),
    })
}

/// Connected and in `#ops` with `members`, holding nothing yet.
fn joined(members: Vec<Member>) -> IrcState {
    let mut state = IrcState::new(&[config()], Vec::new());
    state.apply(connected(), at(0));
    state.apply(Event::Joined { buffer: room() }, at(0));
    state.apply(
        Event::Members {
            buffer: room(),
            members,
        },
        at(0),
    );
    state
}

fn live(state: &mut IrcState, lines: impl IntoIterator<Item = Line>) {
    for line in lines {
        state.apply(Event::Line(line), at(30));
    }
}

/// The status line under `request` in `buffer`, as the transcript shows it.
fn status(state: &IrcState, buffer: &BufferKey, request: &str) -> Option<Shown> {
    let held = state.buffer(buffer)?;
    let connected = state
        .network(&network_id())
        .and_then(|network| network.held_at_connect.clone());
    let seq = held
        .entries
        .iter()
        .find(|entry| entry.line.id.as_deref() == Some(request))?
        .seq;
    statuses(held, connected.as_ref()).get(&seq).cloned()
}

fn label(state: &IrcState, request: &str) -> Option<&'static str> {
    status(state, &room(), request).map(|shown| shown.label)
}

/// Whether the line with message ID `id` in `buffer` is folded.
fn is_folded(state: &IrcState, buffer: &BufferKey, id: &str) -> bool {
    state.buffer(buffer).is_some_and(|held| {
        held.entries
            .iter()
            .any(|entry| entry.line.id.as_deref() == Some(id) && held.folded.contains(&entry.seq))
    })
}

fn shown(label: &'static str, stale: Option<DateTime<Utc>>) -> Shown {
    Shown {
        agent: "astra".into(),
        label,
        stale,
    }
}
