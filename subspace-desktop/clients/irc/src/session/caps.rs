//! Negotiates IRCv3 capabilities and SASL during registration.

use std::collections::{BTreeMap, BTreeSet};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, TimeDelta, Utc};

use super::{Output, Session, metadata, numerics};
use crate::model::{Event, LineKind, Sasl};
use crate::proto::Message;

const WANTED: &[&str] = &[
    "account-notify",
    "account-tag",
    "away-notify",
    "batch",
    "cap-notify",
    "chghost",
    "draft/chathistory",
    "draft/event-playback",
    metadata::CAP,
    "draft/multiline",
    "draft/read-marker",
    "echo-message",
    "extended-join",
    "invite-notify",
    "labeled-response",
    "message-tags",
    "multi-prefix",
    "server-time",
    "setname",
    "standard-replies",
    "userhost-in-names",
];
const CHUNK_SIZE: usize = 400;
const SASL_LIMIT: TimeDelta = TimeDelta::seconds(30);

#[derive(Debug, Default)]
pub struct Caps {
    pub offered: BTreeMap<String, String>,
    pub enabled: BTreeSet<String>,
    open: bool,
    list_complete: bool,
    awaiting: usize,
    sasl: Authentication,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Authentication {
    #[default]
    Idle,
    InProgress(DateTime<Utc>),
    Finished,
    TimedOut,
}

impl Caps {
    pub(super) fn open(&mut self) {
        self.open = true;
    }

    fn requested(&self, credentials: Option<&Sasl>) -> Vec<&'static str> {
        let available = |name: &str| self.offered.contains_key(name);
        let metadata_supported = available("batch") || self.enabled.contains("batch");
        let mut wanted: Vec<_> = WANTED
            .iter()
            .copied()
            .filter(|&name| available(name) && !self.enabled.contains(name))
            .filter(|&name| name != metadata::CAP || metadata_supported)
            .collect();
        if self.open
            && !self.enabled.contains("sasl")
            && let Some(credentials) = credentials
            && let Some(value) = self.offered.get("sasl")
            && (value.is_empty()
                || value
                    .split(',')
                    .any(|name| name.eq_ignore_ascii_case(mechanism(credentials))))
        {
            wanted.push("sasl");
        }
        wanted
    }
}

pub(super) fn cap(session: &mut Session, message: &Message, out: &mut Output) {
    match message.param(1).to_ascii_uppercase().as_str() {
        "LS" | "NEW" => offer(session, message, out),
        "ACK" => acknowledge(session, message, out),
        "NAK" => {
            count_answer(&mut session.caps, message);
            progress(session, out);
        }
        "DEL" => {
            for name in message.trailing().split_whitespace() {
                session.caps.enabled.remove(name);
                session.caps.offered.remove(name);
            }
            if !session.caps.open {
                report(session, out);
            }
            metadata::negotiate(session, out);
        }
        _ => {}
    }
}

fn offer(session: &mut Session, message: &Message, out: &mut Output) {
    let caps = &mut session.caps;
    for token in message.trailing().split_whitespace() {
        let (name, value) = token.split_once('=').unwrap_or((token, ""));
        caps.offered.insert(name.into(), value.into());
    }
    if !matches!(message.params.as_slice(), [_, _, continuation, _, ..] if continuation == "*") {
        caps.list_complete = true;
        let wanted = caps.requested(session.config.sasl.as_ref());
        if !wanted.is_empty() {
            out.send(Message::new("CAP", ["REQ", &wanted.join(" ")]));
            caps.awaiting += 1;
        }
    }
    progress(session, out);
}

fn acknowledge(session: &mut Session, message: &Message, out: &mut Output) {
    let caps = &mut session.caps;
    for token in message.trailing().split_whitespace() {
        if let Some(name) = token.strip_prefix('-') {
            caps.enabled.remove(name);
        } else {
            caps.enabled.insert(token.into());
        }
    }
    count_answer(caps, message);
    if !caps.open {
        report(session, out);
    }
    progress(session, out);
    metadata::negotiate(session, out);
}

fn count_answer(caps: &mut Caps, message: &Message) {
    if message.param(2) != "*" {
        caps.awaiting = caps.awaiting.saturating_sub(1);
    }
}

fn progress(session: &mut Session, out: &mut Output) {
    let caps = &mut session.caps;
    if !caps.open {
        return;
    }
    if caps.sasl == Authentication::Idle
        && caps.enabled.contains("sasl")
        && let Some(credentials) = &session.config.sasl
    {
        out.send(Message::new("AUTHENTICATE", [mechanism(credentials)]));
        caps.sasl = Authentication::InProgress(session.now);
    }
    if caps.list_complete
        && caps.awaiting == 0
        && !matches!(caps.sasl, Authentication::InProgress(_))
    {
        out.send(Message::new("CAP", ["END"]));
        close(session, out);
    }
}

pub(super) fn expire_sasl(session: &mut Session, out: &mut Output) {
    if session.caps.open
        && let Authentication::InProgress(started) = session.caps.sasl
        && session.now - started >= SASL_LIMIT
    {
        session.caps.sasl = Authentication::TimedOut;
        out.send(Message::new("AUTHENTICATE", ["*"]));
        progress(session, out);
    }
}

pub(super) fn close(session: &mut Session, out: &mut Output) {
    if session.caps.open {
        session.caps.open = false;
        report(session, out);
    }
}

fn report(session: &Session, out: &mut Output) {
    out.event(Event::Capabilities {
        network: session.network(),
        enabled: session.caps.enabled.iter().cloned().collect(),
    });
}

fn mechanism(credentials: &Sasl) -> &'static str {
    if credentials.external {
        "EXTERNAL"
    } else {
        "PLAIN"
    }
}

pub(super) fn authenticate(session: &mut Session, message: &Message, out: &mut Output) {
    if session.caps.sasl == Authentication::TimedOut {
        return;
    }
    let Some(credentials) = session
        .config
        .sasl
        .as_ref()
        .filter(|_| message.param(0) == "+")
    else {
        return;
    };
    if credentials.external {
        out.send(Message::new("AUTHENTICATE", ["+"]));
        return;
    }
    let payload = format!("{0}\0{0}\0{1}", credentials.username, credentials.password,);
    let encoded = STANDARD.encode(payload);
    // Base64 is ASCII, so these byte offsets are character boundaries.
    for start in (0..encoded.len()).step_by(CHUNK_SIZE) {
        let end = (start + CHUNK_SIZE).min(encoded.len());
        out.send(Message::new("AUTHENTICATE", [&encoded[start..end]]));
    }
    if encoded.len().is_multiple_of(CHUNK_SIZE) {
        out.send(Message::new("AUTHENTICATE", ["+"]));
    }
}

pub(super) fn sasl_done(session: &mut Session, message: &Message, out: &mut Output) {
    sasl_result(session, message, LineKind::Info, out);
}

pub(super) fn sasl_failed(session: &mut Session, message: &Message, out: &mut Output) {
    sasl_result(session, message, LineKind::Error, out);
}

fn sasl_result(session: &mut Session, message: &Message, kind: LineKind, out: &mut Output) {
    numerics::server_line(session, message, kind, out);
    if session.caps.sasl != Authentication::TimedOut {
        session.caps.sasl = Authentication::Finished;
    }
    progress(session, out);
}
