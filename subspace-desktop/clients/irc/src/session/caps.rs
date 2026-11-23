//! Negotiates IRCv3 capabilities and SASL during registration.

use std::collections::{BTreeMap, BTreeSet};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{DateTime, TimeDelta, Utc};

use super::{Output, Session, lines, metadata, numerics};
use crate::model::{Event, IssuedPassword, LineKind, Sasl, Target};
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
    pub(super) issued: Issued,
}

/// A login a credential source issued, which the server must accept.
#[derive(Debug, Default)]
pub(super) struct Issued {
    /// The password, for this connection's one PLAIN exchange; taken when
    /// it is sent.
    password: Option<IssuedPassword>,
    /// Whether registration needs the server to accept the SASL exchange.
    required: bool,
    /// Whether the server accepted the SASL exchange (903).
    accepted: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum Authentication {
    #[default]
    Idle,
    InProgress(DateTime<Utc>),
    Finished,
    TimedOut,
}

impl Issued {
    /// Signs in with `password`, which the server must accept.
    pub(super) fn issue(&mut self, password: IssuedPassword) {
        *self = Self {
            password: Some(password),
            required: true,
            accepted: false,
        };
    }

    /// Whether a sign-in that had to be accepted was not.
    pub(super) fn unaccepted(&self) -> bool {
        self.required && !self.accepted
    }
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
    if session.registered() {
        out.updates.now.push(super::reactions::support(session));
    }
}

fn mechanism(credentials: &Sasl) -> &'static str {
    if credentials.external {
        "EXTERNAL"
    } else {
        "PLAIN"
    }
}

pub(super) fn authenticate(session: &mut Session, message: &Message, out: &mut Output) {
    if session.caps.sasl == Authentication::TimedOut || message.param(0) != "+" {
        return;
    }
    // An issued password goes out once, then the session drops it, and
    // only while it may still be sent.
    let issued = session.caps.issued.password.take();
    if issued.as_ref().is_some_and(|issued| !issued.sendable()) {
        refuse(EXPIRED, out);
        return;
    }
    let Some(credentials) = session.config.sasl.as_ref() else {
        return;
    };
    if credentials.external {
        out.send(Message::new("AUTHENTICATE", ["+"]));
        return;
    }
    let password = issued
        .as_ref()
        .map_or(credentials.password.as_str(), IssuedPassword::expose);
    let payload = format!("{0}\0{0}\0{1}", credentials.username, password);
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

/// Why an issued login was not sent.
const EXPIRED: &str = "The sign-in expired or its account signed out before it was sent";

/// Ends the link because an issued login cannot be used: leave, and let
/// the network tell the source and retry.
fn refuse(reason: &str, out: &mut Output) {
    out.ending = Some(super::Ending::Refused(reason.to_owned()));
    out.send(Message::new("QUIT", ["Sign-in required"]));
}

/// A SASL outcome in fixed words, with its numeric.
fn sign_in_outcome(numeric: &str) -> String {
    match numeric {
        "903" => format!("The server accepted the sign-in ({numeric})"),
        "907" => format!("The connection is already signed in ({numeric})"),
        _ => format!("The server did not accept the sign-in ({numeric})"),
    }
}

pub(super) fn sasl_done(session: &mut Session, message: &Message, out: &mut Output) {
    if message.command == "903" {
        session.caps.issued.accepted = true;
    }
    sasl_result(session, message, LineKind::Info, out);
}

pub(super) fn sasl_failed(session: &mut Session, message: &Message, out: &mut Output) {
    sasl_result(session, message, LineKind::Error, out);
}

fn sasl_result(session: &mut Session, message: &Message, kind: LineKind, out: &mut Output) {
    if session.caps.issued.required {
        // The server's words could echo the issued login: say it in ours.
        let text = sign_in_outcome(&message.command);
        let line = lines::line(session, message, Target::Server, kind, text.clone());
        out.event(Event::Line(line));
        if kind == LineKind::Error {
            // Refused: leave now, so the login is dropped and retried later.
            session.caps.sasl = Authentication::Finished;
            refuse(&text, out);
            return;
        }
    } else {
        numerics::server_line(session, message, kind, out);
    }
    if session.caps.sasl != Authentication::TimedOut {
        session.caps.sasl = Authentication::Finished;
    }
    progress(session, out);
}
