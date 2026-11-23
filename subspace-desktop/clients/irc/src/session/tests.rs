//! Session tests driven by recorded server transcripts.

use chrono::{DateTime, TimeZone, Utc};

use super::{Carry, Output, Session};
use crate::model::{BufferKey, Command, Event, Line, NetworkConfig, NetworkId, Sasl, Target};
use crate::proto::{parse, serialise};

mod anchors;
mod autojoin;
mod client_tags;
mod forget;
mod history;
mod listing;
mod members;
mod messages;
mod metadata;
mod reactions;
mod registration;
mod replies;
mod sign_in;
mod spelling;

/// Everything Ergo 2.19 offers, as two `CAP LS 302` lines.
pub(super) const ERGO_LS: [&str; 2] = [
    ":ergo.test CAP * LS * :account-notify account-tag away-notify batch cap-notify chghost draft/account-registration=before-connect draft/channel-rename draft/chathistory draft/event-playback draft/extended-isupport draft/multiline=max-bytes=4096,max-lines=100 draft/persistence draft/pre-away",
    ":ergo.test CAP * LS :draft/read-marker draft/relaymsg=/ echo-message extended-join extended-monitor invite-notify labeled-response message-tags multi-prefix no-implicit-names sasl=PLAIN,EXTERNAL,SCRAM-SHA-256 server-time setname standard-replies userhost-in-names znc.in/playback",
];

pub(super) const ERGO_ISUPPORT: &str = ":ergo.test 005 subspace AWAYLEN=390 CASEMAPPING=ascii CHANMODES=Ibe,k,fl,CEMRUimnstu CHANTYPES=# CHATHISTORY=1000 NETWORK=ErgoTest PREFIX=(qaohv)~&@%+ STATUSMSG=~&@%+ WHOX :are supported by this server";

pub(super) fn config() -> NetworkConfig {
    NetworkConfig {
        id: NetworkId("local".into()),
        name: "Local".into(),
        host: "127.0.0.1".into(),
        port: 16667,
        tls: false,
        accept_invalid_certs: false,
        nickname: "subspace".into(),
        alt_nicks: vec!["subspace2".into()],
        username: String::new(),
        realname: String::new(),
        password: String::new(),
        sasl: Some(Sasl {
            username: "subspace".into(),
            password: "secret".into(),
            password_file: None,
            external: false,
            credential: None,
        }),
        channels: vec!["#subspace".into()],
        auto_connect: true,
        highlights: vec!["deploy".into()],
        ca_cert: None,
        client_cert: None,
        client_key: None,
    }
}

pub(super) fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
        .single()
        .unwrap_or_default()
}

/// Feeds server lines; returns everything they produced.
pub(super) fn feed(session: &mut Session, lines: &[&str]) -> Output {
    let mut all = Output::default();
    for line in lines {
        let out = session.receive(parse(line).unwrap(), now());
        all.send.extend(out.send);
        all.events.extend(out.events);
        all.updates.now.extend(out.updates.now);
        all.quit |= out.quit;
        all.ending = all.ending.or(out.ending);
    }
    all
}

pub(super) fn command(session: &mut Session, command: Command) -> Output {
    session.command(command, now())
}

pub(super) fn sent(out: &Output) -> Vec<String> {
    out.send.iter().map(serialise).collect()
}

pub(super) fn lines(out: &Output) -> Vec<&Line> {
    out.events
        .iter()
        .filter_map(|event| match event {
            Event::Line(line) => Some(line),
            _ => None,
        })
        .collect()
}

pub(super) fn buffer(target: Target) -> BufferKey {
    BufferKey {
        network: NetworkId("local".into()),
        target,
    }
}

pub(super) fn channel(name: &str) -> BufferKey {
    buffer(Target::Channel(name.into()))
}

/// A registered session with every Ergo cap, before the autojoin.
pub(super) fn registered(caps: &str) -> Session {
    let mut config = config();
    config.sasl = None;
    let mut session = Session::new(config, Carry::default());
    session.start(now());
    let acked: Vec<&str> = caps
        .split_whitespace()
        .map(|cap| cap.split('=').next().unwrap_or(cap))
        .collect();
    feed(
        &mut session,
        &[
            &format!(":ergo.test CAP * LS :{caps}"),
            &format!(":ergo.test CAP subspace ACK :{}", acked.join(" ")),
            ":ergo.test 001 subspace :Welcome",
            ERGO_ISUPPORT,
        ],
    );
    session
}

pub(super) const ALL_CAPS: &str = "account-notify account-tag away-notify batch cap-notify chghost draft/chathistory draft/event-playback draft/multiline=max-bytes=4096,max-lines=100 draft/read-marker echo-message extended-join invite-notify labeled-response message-tags multi-prefix server-time setname standard-replies userhost-in-names";

/// `ALL_CAPS` without `echo-message` or `draft/multiline`.
pub(super) const PLAIN_CAPS: &str = "batch message-tags server-time draft/chathistory multi-prefix";

/// A registered session in `#c` with alice (op), bob (voice) and us.
pub(super) fn in_channel(caps: &str) -> Session {
    let mut session = registered(caps);
    feed(
        &mut session,
        &[
            ":subspace!u@h JOIN #c * :Subspace",
            ":ergo.test 353 subspace = #c :@alice!a@h +bob!b@h subspace!u@h",
            ":ergo.test 366 subspace #c :End of NAMES list",
        ],
    );
    session
}
