//! Session acceptance tests from the functional specification.

mod capabilities;
mod features;
mod registration;
mod state;

use chrono::{DateTime, TimeZone, Utc};

use super::{Carry, Output, Session};
use crate::model::{BufferKey, Event, Line, NetworkConfig, NetworkId, Target};
use crate::proto::{parse, serialise};

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2030, 6, 7, 8, 9, 10).single().unwrap()
}

fn net() -> NetworkId {
    NetworkId("acc".into())
}

fn key(target: Target) -> BufferKey {
    BufferKey {
        network: net(),
        target,
    }
}

fn base() -> NetworkConfig {
    serde_json::from_value(serde_json::json!({
        "id": "acc", "name": "Acceptance", "host": "irc.acceptance.test", "port": 6667,
        "tls": false, "nickname": "kim"
    }))
    .unwrap()
}

fn feed(session: &mut Session, lines: &[&str]) -> Output {
    let mut all = Output::default();
    for line in lines {
        let out = session.receive(parse(line).unwrap(), at());
        all.send.extend(out.send);
        all.events.extend(out.events);
        all.quit |= out.quit;
    }
    all
}

fn wire(out: &Output) -> Vec<String> {
    out.send.iter().map(serialise).collect()
}

fn lines(out: &Output) -> Vec<&Line> {
    out.events
        .iter()
        .filter_map(|event| match event {
            Event::Line(line) => Some(line),
            _ => None,
        })
        .collect()
}

/// Registered with `caps` acknowledged, before any channel.
fn welcomed(config: NetworkConfig, caps: &str) -> Session {
    let mut session = Session::new(config, Carry::default());
    session.start(at());
    let names: Vec<&str> = caps
        .split_whitespace()
        .map(|cap| cap.split('=').next().unwrap_or(cap))
        .collect();
    feed(
        &mut session,
        &[
            &format!(":irc.acceptance.test CAP * LS :{caps}"),
            &format!(":irc.acceptance.test CAP kim ACK :{}", names.join(" ")),
            ":irc.acceptance.test 001 kim :Hello",
        ],
    );
    session
}

fn capabilities(out: &Output) -> Vec<Vec<String>> {
    out.events
        .iter()
        .filter_map(|event| match event {
            Event::Capabilities { enabled, .. } => Some(enabled.clone()),
            _ => None,
        })
        .collect()
}
