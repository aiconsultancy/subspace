//! `Command::SendAll`: the connection must be registered and every line
//! is checked against the negotiated limits before any goes out. The answer
//! and the local echoes wait for the write.
use super::super::Output;
use super::{
    ALL_CAPS, LineKind, PLAIN_CAPS, Target, buffer, command, config, in_channel, lines, now,
};
use crate::model::{Command, Event, SendResult};
use crate::session::send_all::{NOT_REGISTERED, WRITE_FAILED};
use crate::session::{Carry, Session};

const REFERENCE: &str = "UPL-AAECAwQFBgcICQoLDA0ODw report.pdf (12 KB)";

fn send_all(caps: &str, texts: &[String]) -> Output {
    let mut session = in_channel(caps);
    send_all_on(&mut session, texts)
}

fn send_all_on(session: &mut Session, texts: &[String]) -> Output {
    command(
        session,
        Command::SendAll {
            buffer: buffer(Target::Channel("#c".into())),
            id: 7,
            lines: texts.to_vec(),
        },
    )
}

/// The answer to send 7 among `events`.
fn answer_in(events: &[Event]) -> Option<&SendResult> {
    events.iter().find_map(|event| match event {
        Event::Sent { id: 7, result, .. } => Some(result),
        _ => None,
    })
}

/// The answer as the owner sees it after a full write.
fn answer(out: &Output) -> Option<&SendResult> {
    answer_in(&out.events).or_else(|| answer_in(&out.written))
}

fn refused(out: &Output) -> bool {
    matches!(answer(out), Some(SendResult::Refused(_)))
}

#[test]
fn a_draft_over_the_negotiated_lines_sends_no_line_and_no_reference() {
    let caps = ALL_CAPS.replace("max-lines=100", "max-lines=1");
    let draft = "line\n".repeat(24) + "last";
    let out = send_all(&caps, &[draft, REFERENCE.to_string()]);
    assert!(out.send.is_empty(), "neither the text nor the reference");
    assert!(refused(&out));
    assert!(out.written.is_empty() && out.unwritten.is_empty());
    let shown = lines(&out);
    assert_eq!(shown.len(), 1, "only the local error, no echo");
    assert_eq!(shown[0].kind, LineKind::Error);
}

#[test]
fn lines_within_the_limits_all_go_and_are_accepted() {
    let draft = "line\n".repeat(24) + "last";
    let out = send_all(ALL_CAPS, &[draft, REFERENCE.to_string()]);
    let privmsgs = out.send.iter().filter(|m| m.command == "PRIVMSG").count();
    assert_eq!(privmsgs, 26, "25 text lines and the reference");
    assert_eq!(answer_in(&out.events), None, "nothing before the write");
    assert_eq!(answer_in(&out.written), Some(&SendResult::Written));
    assert_eq!(
        answer_in(&out.unwritten),
        Some(&SendResult::Uncertain(WRITE_FAILED.into())),
        "a failed write may have sent some"
    );
}

#[test]
fn a_command_line_is_refused_and_double_slash_is_text() {
    let out = send_all(ALL_CAPS, &["/part #c".into(), REFERENCE.to_string()]);
    assert!(out.send.is_empty());
    assert!(refused(&out));
    let out = send_all(ALL_CAPS, &["//part".into()]);
    assert_eq!(out.send.len(), 1);
    assert_eq!(answer(&out), Some(&SendResult::Written));
}

#[test]
fn local_echoes_wait_for_the_write() {
    let out = send_all(PLAIN_CAPS, &["hello".into(), REFERENCE.to_string()]);
    assert_eq!(out.send.len(), 2);
    assert!(lines(&out).is_empty(), "no echo before the write");
    let echoes = out
        .written
        .iter()
        .filter(|event| matches!(event, Event::Line(line) if line.own))
        .count();
    assert_eq!(echoes, 2, "both echo once written");
    assert!(
        out.unwritten
            .iter()
            .all(|event| matches!(event, Event::Sent { .. })),
        "a failed write echoes nothing"
    );
}

#[test]
fn before_the_welcome_nothing_is_prepared_and_the_send_is_refused() {
    let mut config = config();
    config.sasl = None;
    let mut session = Session::new(config, Carry::default());
    session.start(now());
    for line in [
        ":ergo.test CAP * LS :batch draft/multiline=max-bytes=4096,max-lines=100",
        ":ergo.test CAP subspace ACK :batch draft/multiline",
    ] {
        session.receive(crate::proto::parse(line).unwrap(), now());
    }
    assert!(!session.registered());
    let out = send_all_on(&mut session, &["hello".into(), REFERENCE.to_string()]);
    assert!(out.send.is_empty(), "no PRIVMSG before 001");
    assert_eq!(
        answer(&out),
        Some(&SendResult::Refused(NOT_REGISTERED.into()))
    );
    assert!(out.written.is_empty() && out.unwritten.is_empty());
    assert!(lines(&out).iter().all(|line| !line.own), "no echo");
}
