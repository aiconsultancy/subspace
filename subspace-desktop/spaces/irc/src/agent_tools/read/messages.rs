//! `messages`: one buffer's lines, paged back by `seq`, and the server's
//! chat history past the oldest line held.

use chrono::SecondsFormat;
use serde_json::{Value, json};
use subspace_client_irc::{BufferKey, Command, LineKind, Target, format};

use super::super::lookup;
use super::{CHATHISTORY, buffer_key};
use crate::state::{Entry, HistoryState, IrcState};
use crate::view::HISTORY_PAGE;

#[derive(Debug, Clone, Copy)]
pub(super) struct Page {
    pub limit: usize,
    /// Only lines with a smaller `seq`, from a previous `next_before`.
    pub before: Option<i64>,
}

pub(super) fn messages(
    state: &mut IrcState,
    network: &str,
    target: Option<&str>,
    page: Page,
) -> (Result<Value, String>, Vec<Command>) {
    let key = match buffer_key(state, network, target) {
        Ok(key) => key,
        Err(error) => return (Err(error), Vec::new()),
    };
    let lines = page_lines(state, &key, page);
    let older_held = lines.first().is_some_and(|first| {
        state
            .buffer(&key)
            .and_then(|buffer| buffer.entries.front())
            .is_some_and(|oldest| oldest.seq != first.seq)
    });
    let (history, commands) = server_history(state, &key, older_held);
    let next_before = lines
        .first()
        .map(|first| first.seq)
        .filter(|_| older_held || matches!(history, "requested" | "loading"));
    let value = json!({
        "network": key.network.0,
        "target": lookup::target_name(&key.target),
        "lines": lines.iter().map(line).collect::<Vec<_>>(),
        "next_before": next_before,
        "server_history": history,
    });
    (Ok(value), commands)
}

/// The `limit` lines before the line `before` names, by position: lines
/// merged in later take new numbers, so numbers do not follow the order.
/// A `before` the buffer no longer holds was dropped as the oldest, so
/// nothing older is held.
fn page_lines(state: &IrcState, key: &BufferKey, page: Page) -> Vec<Entry> {
    let Some(buffer) = state.buffer(key) else {
        return Vec::new();
    };
    let end = match page.before {
        Some(before) => buffer
            .entries
            .iter()
            .position(|entry| entry.seq == before)
            .unwrap_or_default(),
        None => buffer.entries.len(),
    };
    let start = end.saturating_sub(page.limit);
    buffer.entries.range(start..end).cloned().collect()
}

/// Whether the server can give lines older than the board holds; at the
/// start of what it holds, an idle buffer asks for the next page, as
/// scrolling up does.
fn server_history(
    state: &mut IrcState,
    key: &BufferKey,
    older_held: bool,
) -> (&'static str, Vec<Command>) {
    let supported = key.target != Target::Server
        && state.network(&key.network).is_some_and(|network| {
            lookup::is_connected(network) && network.caps.iter().any(|cap| cap == CHATHISTORY)
        });
    let history = state.buffer(key).map(|buffer| buffer.history.state);
    match history {
        Some(HistoryState::Complete) => ("complete", Vec::new()),
        Some(HistoryState::Loading) => ("loading", Vec::new()),
        _ if !supported => ("unavailable", Vec::new()),
        _ if older_held => ("available", Vec::new()),
        _ => match state.older_history(key, HISTORY_PAGE) {
            Some(command) => ("requested", vec![command]),
            None => ("unavailable", Vec::new()),
        },
    }
}

pub(crate) fn line(entry: &Entry) -> Value {
    json!({
        "seq": entry.seq,
        "time": entry.line.time.to_rfc3339_opts(SecondsFormat::Secs, true),
        "kind": line_kind(entry.line.kind),
        "from": entry.line.from,
        "text": format::strip(&entry.line.text),
        "own": entry.line.own,
        "mention": entry.highlight,
    })
}

fn line_kind(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Privmsg => "message",
        LineKind::Notice => "notice",
        LineKind::Action => "action",
        LineKind::Join => "join",
        LineKind::Part => "part",
        LineKind::Quit => "quit",
        LineKind::Kick => "kick",
        LineKind::Nick => "nick",
        LineKind::Topic => "topic",
        LineKind::Mode => "mode",
        LineKind::Invite => "invite",
        LineKind::Info => "info",
        LineKind::Error => "error",
    }
}
