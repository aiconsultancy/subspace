//! A registered session without a connection, for owners' tests that need
//! the client's real answer to a command under given capabilities.
use chrono::{DateTime, Utc};

use crate::model::{BufferKey, Command, Event, NetworkConfig, Reaction, ReactionChange, Update};
use crate::proto::{parse, serialise};
use crate::session::Casemapping;
use crate::session::{Carry, Session};

/// A session registered on a server offering `caps` (a `CAP LS` value,
/// such as `batch draft/multiline=max-bytes=4096,max-lines=1`), joined to
/// `channel`.
// qual:api - used by the IRC board's tests through the test-support feature.
pub struct Rehearsal {
    session: Session,
    now: DateTime<Utc>,
}

/// What the session sent to the server, what it reported to its owner
/// after the write succeeded, and what it reported if the write failed.
pub struct Answer {
    pub wire: Vec<String>,
    pub events: Vec<Event>,
    pub lost: Vec<Event>,
    /// Reaction updates after the write succeeded, and if it failed.
    pub updates: Vec<Update>,
    pub lost_updates: Vec<Update>,
}

/// A live reaction from `nick`, added at the Unix epoch on an ASCII
/// network, for owners' tests; set its public fields to vary it.
pub fn reaction(buffer: BufferKey, msgid: &str, reaction: &str, nick: &str) -> Reaction {
    Reaction {
        buffer,
        msgid: msgid.to_owned(),
        reaction: reaction.to_owned(),
        change: ReactionChange::Added,
        nick: nick.to_owned(),
        account: None,
        own: false,
        time: DateTime::UNIX_EPOCH,
        id: None,
        history: false,
        casemapping: Casemapping::Ascii,
    }
}

impl Rehearsal {
    pub fn new(config: NetworkConfig, caps: &str, channel: &str, now: DateTime<Utc>) -> Self {
        let nick = config.nickname.clone();
        let mut rehearsal = Self::registering(config, caps, now);
        for line in [
            format!(":irc.test 001 {nick} :Welcome"),
            format!(":{nick}!u@h JOIN {channel}"),
        ] {
            rehearsal.receive(&line);
        }
        rehearsal
    }

    /// A session that agreed `caps` but has not been welcomed yet, as
    /// right after a reconnect.
    pub fn registering(mut config: NetworkConfig, caps: &str, now: DateTime<Utc>) -> Self {
        config.sasl = None;
        let nick = config.nickname.clone();
        let mut rehearsal = Self {
            session: Session::new(config, Carry::default()),
            now,
        };
        rehearsal.session.start(now);
        let acked: Vec<&str> = caps
            .split_whitespace()
            .map(|cap| cap.split('=').next().unwrap_or(cap))
            .collect();
        for line in [
            format!(":irc.test CAP * LS :{caps}"),
            format!(":irc.test CAP {nick} ACK :{}", acked.join(" ")),
        ] {
            rehearsal.receive(&line);
        }
        rehearsal
    }

    fn receive(&mut self, line: &str) {
        if let Ok(message) = parse(line) {
            self.session.receive(message, self.now);
        }
    }

    /// The session's answer to `command`.
    pub fn command(&mut self, command: Command) -> Answer {
        let out = self.session.command(command, self.now);
        self.session.written(&out.written);
        let reported = |after: Vec<Event>| out.events.iter().cloned().chain(after).collect();
        let updated = |after: Vec<Update>| out.updates.now.iter().cloned().chain(after).collect();
        Answer {
            wire: out.send.iter().map(serialise).collect(),
            events: reported(out.written.clone()),
            lost: reported(out.unwritten.clone()),
            updates: updated(out.updates.written.clone()),
            lost_updates: updated(out.updates.unwritten.clone()),
        }
    }
}

/// What `source` issues, fetched as the runtime does at a connect, on the
/// caller's thread.
// qual:api - used by the IRC board's tests through the test-support feature.
pub fn fetch(source: &crate::model::CredentialSource) -> Result<crate::model::IssuedLogin, String> {
    source.fetch()
}
