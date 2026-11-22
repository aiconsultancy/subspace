//! A `SendAll` whose write fails: no success answer and no local
//! echo, but an uncertain answer, whether the connection failed before any
//! byte, after a prefix or on flush.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use chrono::Utc;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

use super::super::{End, EventSink, Link, NetCommand, WRITE_LIMIT};
use crate::model::{BufferKey, Command, Event, NetworkConfig, NetworkId, SendResult, Target};
use crate::session::{Carry, Session};

/// Where the fake connection fails.
#[derive(Clone, Copy)]
enum Failure {
    BeforeAnyByte,
    AfterBytes(usize),
    OnFlush,
    StalledWrite,
}

/// A connection that never answers and fails its writes as `failure` says,
/// keeping what it accepted.
struct Failing {
    failure: Failure,
    wire: Arc<Mutex<Vec<u8>>>,
}

fn broken() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::BrokenPipe, "broken pipe")
}

impl AsyncRead for Failing {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        _: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Pending
    }
}

impl AsyncWrite for Failing {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut wire = self.wire.lock().unwrap();
        let room = match self.failure {
            Failure::BeforeAnyByte => 0,
            Failure::AfterBytes(limit) => limit.saturating_sub(wire.len()),
            Failure::OnFlush => bytes.len(),
            Failure::StalledWrite => return Poll::Pending,
        };
        if room == 0 {
            return Poll::Ready(Err(broken()));
        }
        let taken = room.min(bytes.len());
        wire.extend_from_slice(&bytes[..taken]);
        Poll::Ready(Ok(taken))
    }

    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.failure {
            Failure::OnFlush => Poll::Ready(Err(broken())),
            _ => Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// A session welcomed by a server without `echo-message`, so a full write
/// would echo our lines locally.
fn welcomed() -> Session {
    let config: NetworkConfig = serde_json::from_value(serde_json::json!({
        "id": "local", "host": "127.0.0.1", "port": 6667, "tls": false, "nickname": "subspace"
    }))
    .unwrap();
    let mut session = Session::new(config, Carry::default());
    session.start(Utc::now());
    for line in [
        ":srv CAP * LS :batch message-tags",
        ":srv CAP subspace ACK :batch message-tags",
        ":srv 001 subspace :Welcome",
        ":srv 005 subspace CHANTYPES=# :are supported",
        ":subspace!u@h JOIN #c",
    ] {
        session.receive(crate::proto::parse(line).unwrap(), Utc::now());
    }
    session
}

/// Sends `lines` over a connection failing as `failure` says. Returns how
/// the connection ended, what was reported and what reached the wire.
async fn send_over(failure: Failure, lines: &[&str]) -> (Option<End>, Vec<Event>, String) {
    let wire = Arc::new(Mutex::new(Vec::new()));
    let stream = Box::new(Failing {
        failure,
        wire: wire.clone(),
    });
    let events = EventSink::default();
    let mut link = Link::new(stream, welcomed(), events.clone());
    let send = Command::SendAll {
        buffer: BufferKey {
            network: NetworkId("local".into()),
            target: Target::Channel("#c".into()),
        },
        id: 9,
        lines: lines.iter().map(|line| line.to_string()).collect(),
    };
    let end = link.command(Some(NetCommand::Session(send))).await;
    let reported = events.events();
    let written = String::from_utf8_lossy(&wire.lock().unwrap()).into_owned();
    (end, reported, written)
}

async fn uncertain_without_echo(failure: Failure) -> String {
    let lines = ["hello", "UPL-AAECAwQFBgcICQoLDA0ODw a.txt (4 bytes)"];
    let (end, events, wire) = send_over(failure, &lines).await;
    assert!(matches!(end, Some(End::Lost { .. })), "{end:?}");
    let answers: Vec<&SendResult> = events
        .iter()
        .filter_map(|event| match event {
            Event::Sent { id: 9, result, .. } => Some(result),
            _ => None,
        })
        .collect();
    assert!(
        matches!(answers.as_slice(), [SendResult::Uncertain(_)]),
        "one uncertain answer: {answers:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, Event::Line(line) if line.own)),
        "no local echo: {events:?}"
    );
    wire
}

#[tokio::test]
async fn a_write_failing_before_any_byte_is_uncertain_and_echoes_nothing() {
    assert_eq!(uncertain_without_echo(Failure::BeforeAnyByte).await, "");
}

#[tokio::test]
async fn a_write_failing_after_a_prefix_is_uncertain_and_echoes_nothing() {
    let wire = uncertain_without_echo(Failure::AfterBytes(10)).await;
    assert_eq!(wire, "PRIVMSG #c", "part of the first line went");
}

#[tokio::test]
async fn a_failed_flush_is_uncertain_and_echoes_nothing() {
    let wire = uncertain_without_echo(Failure::OnFlush).await;
    assert!(wire.contains("hello"), "the lines were written: {wire}");
}

#[tokio::test(start_paused = true)]
async fn a_stalled_write_times_out_without_reporting_success() {
    let start = tokio::time::Instant::now();
    let lines = ["hello", "UPL-AAECAwQFBgcICQoLDA0ODw a.txt (4 bytes)"];
    let (end, events, wire) =
        tokio::time::timeout(WRITE_LIMIT * 2, send_over(Failure::StalledWrite, &lines))
            .await
            .expect("the stalled write should end the connection");
    assert_eq!(start.elapsed(), WRITE_LIMIT);
    assert!(
        matches!(end, Some(End::Lost { error, registered: true }) if error.contains("timed out"))
    );
    assert_eq!(wire, "");
    assert!(matches!(
        events.as_slice(),
        [Event::Sent {
            id: 9,
            result: SendResult::Uncertain(_),
            ..
        }]
    ));
}

#[tokio::test]
async fn a_full_write_answers_written_after_the_echoes() {
    let (end, events, wire) = send_over(Failure::AfterBytes(usize::MAX), &["hello"]).await;
    assert!(end.is_none(), "{end:?}");
    assert!(wire.contains("hello"), "{wire}");
    let echo = events
        .iter()
        .position(|event| matches!(event, Event::Line(line) if line.own));
    let answer = events.iter().position(|event| {
        matches!(
            event,
            Event::Sent {
                id: 9,
                result: SendResult::Written,
                ..
            }
        )
    });
    assert!(
        echo.is_some() && answer.is_some() && echo < answer,
        "{events:?}"
    );
}
