//! Registers the connection and restores joins after the server welcomes it.

use std::collections::BTreeSet;

use super::{Output, Session, autojoin, caps, history, metadata, numerics};
use crate::model::{ConnectionStatus, Event, LineKind, NetworkConfig};
use crate::proto::Message;

const MAX_UNDERSCORES: usize = 3;

pub(super) struct Registration {
    pub(super) nick: String,
    pub(super) registered: bool,
    /// Our account, from `RPL_LOGGEDIN` (900).
    pub(super) account: Option<String>,
    next_alternative: usize,
    underscores: usize,
}

impl Registration {
    pub(super) fn new(nick: String) -> Self {
        Self {
            nick,
            registered: false,
            account: None,
            next_alternative: 0,
            underscores: 0,
        }
    }
}

pub(super) fn start(config: &NetworkConfig) -> Vec<Message> {
    let nick = config.nickname.as_str();
    let username = nonempty_or(&config.username, nick);
    let realname = nonempty_or(&config.realname, nick);
    let mut messages = vec![Message::new("CAP", ["LS", "302"])];
    if !config.password.is_empty() {
        messages.push(Message::new("PASS", [&config.password]));
    }
    messages.push(Message::new("NICK", [nick]));
    messages.push(Message::new("USER", [username, "0", "*", realname]));
    messages
}

fn nonempty_or<'a>(value: &'a str, default: &'a str) -> &'a str {
    if value.is_empty() { default } else { value }
}

pub(super) fn ping(_: &mut Session, message: &Message, out: &mut Output) {
    out.send(Message::new("PONG", &message.params));
}

pub(super) fn error(session: &mut Session, message: &Message, out: &mut Output) {
    numerics::server_line(session, message, LineKind::Error, out);
}

pub(super) fn nick_error(session: &mut Session, message: &Message, out: &mut Output) {
    let reg = &mut session.reg;
    if reg.registered {
        numerics::server_line(session, message, LineKind::Error, out);
        return;
    }
    let nick = if let Some(nick) = session.config.alt_nicks.get(reg.next_alternative) {
        reg.next_alternative += 1;
        nick.clone()
    } else if reg.underscores < MAX_UNDERSCORES {
        reg.underscores += 1;
        format!("{}_", reg.nick)
    } else {
        out.fatal = Some(format!(
            "No IRC nickname was accepted. {}",
            message.trailing()
        ));
        return;
    };
    if let Some(limit) = session.isupport.nicklen
        && nick.len() > limit
    {
        out.fatal = Some(format!(
            "The next IRC nickname exceeds the server’s {limit}-byte limit"
        ));
        return;
    }
    reg.nick = nick;
    out.send(Message::new("NICK", [&reg.nick]));
}

pub(super) fn welcome(session: &mut Session, message: &Message, out: &mut Output) {
    session.reg.nick = message.param(0).to_owned();
    session.reg.registered = true;
    caps::close(session, out);
    let nick = session.reg.nick.clone();
    out.event(Event::OwnNick {
        network: session.network(),
        nick: nick.clone(),
    });
    out.event(Event::Status {
        network: session.network(),
        status: ConnectionStatus::Connected { nick },
    });
    numerics::server_line(session, message, LineKind::Info, out);
    metadata::negotiate(session, out);

    let configured = session
        .autojoin
        .entries(&session.config.channels, &str::to_ascii_lowercase);
    out.send.extend(joins(
        configured.into_iter().chain(&session.rejoin),
        &session.left,
    ));
    out.send.extend(history::resume_queries(session));
}

pub(super) fn additions(session: &Session, before: &BTreeSet<String>) -> Vec<Message> {
    let fold = |name: &str| session.fold(name);
    let configured = session.autojoin.entries(&session.config.channels, &fold);
    joins(
        configured
            .into_iter()
            .filter(|entry| !before.contains(&fold(autojoin::name(entry)))),
        &session.left,
    )
    .collect()
}

fn joins<'a>(
    entries: impl Iterator<Item = &'a String> + 'a,
    left: &'a [String],
) -> impl Iterator<Item = Message> + 'a {
    let mut seen = BTreeSet::new();
    entries.filter_map(move |entry| {
        let mut words = entry.split_whitespace();
        let channel = words.next()?;
        if left.iter().any(|name| channel.eq_ignore_ascii_case(name))
            || !seen.insert(channel.to_ascii_lowercase())
        {
            return None;
        }
        let params = std::iter::once(channel).chain(words.next());
        Some(Message::new("JOIN", params))
    })
}
