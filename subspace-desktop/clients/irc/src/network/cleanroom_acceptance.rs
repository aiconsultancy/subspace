//! Clean-room acceptance tests for the network task. Written from the
//! functional specification.

use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::JoinHandle;

use super::{EventSink, NetCommand, backoff, run_with};
use crate::connection::Timeouts;
use crate::model::{
    BufferKey, Command, ConnectionStatus, Event, LineKind, NetworkConfig, NetworkId, Target,
};

const WAIT: Duration = Duration::from_secs(10);

fn statuses(events: &EventSink) -> Vec<ConnectionStatus> {
    events
        .events()
        .iter()
        .filter_map(|event| match event {
            Event::Status { status, .. } => Some(status.clone()),
            _ => None,
        })
        .collect()
}

async fn until(events: &EventSink, what: &str, done: impl Fn(&[ConnectionStatus]) -> bool) {
    let waited = tokio::time::timeout(WAIT, async {
        while !done(&statuses(events)) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    assert!(waited.is_ok(), "{what}: {:?}", statuses(events));
}

async fn read_until(socket: &mut TcpStream, needle: &str) -> String {
    let mut received = Vec::new();
    let found = tokio::time::timeout(WAIT, async {
        while !String::from_utf8_lossy(&received).contains(needle) {
            let mut chunk = [0; 1024];
            let count = socket.read(&mut chunk).await.unwrap();
            if count == 0 {
                break;
            }
            received.extend_from_slice(&chunk[..count]);
        }
    })
    .await;
    assert!(found.is_ok(), "waiting for {needle:?}");
    String::from_utf8_lossy(&received).into_owned()
}

async fn listener(extra: serde_json::Value) -> (TcpListener, crate::model::NetworkConfig) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut json = serde_json::json!({
        "id": "acc", "name": "Acceptance", "host": "127.0.0.1",
        "port": listener.local_addr().unwrap().port(), "tls": false, "nickname": "kim"
    });
    if let (Some(base), Some(extra)) = (json.as_object_mut(), extra.as_object()) {
        base.extend(extra.clone());
    }
    (listener, serde_json::from_value(json).unwrap())
}

fn start(config: NetworkConfig) -> (UnboundedSender<NetCommand>, EventSink, JoinHandle<()>) {
    let (commands, receiver) = mpsc::unbounded_channel();
    let events = EventSink::default();
    let task = tokio::spawn(run_with(
        config,
        receiver,
        events.clone(),
        Timeouts::default(),
    ));
    (commands, events, task)
}

async fn accept(listener: &TcpListener) -> TcpStream {
    tokio::time::timeout(WAIT, listener.accept())
        .await
        .unwrap()
        .unwrap()
        .0
}

async fn accept_and_welcome(listener: &TcpListener, events: &EventSink) -> TcpStream {
    let mut socket = accept(listener).await;
    socket.write_all(b":s 001 kim :Hi\r\n").await.unwrap();
    until(events, "connected", |seen| {
        seen.contains(&ConnectionStatus::Connected { nick: "kim".into() })
    })
    .await;
    socket
}

#[test]
fn w01_backoff_starts_at_one_second() {
    assert_eq!(backoff(0), 1);
    assert_eq!(backoff(1), 1);
    assert_eq!(backoff(6), 32);
    assert_eq!(backoff(7), 60);
    assert_eq!(backoff(u32::MAX), 60);
}

#[tokio::test]
async fn w02_an_idle_network_answers_offline_and_connects_on_request() {
    let (listener, config) = listener(serde_json::json!({ "autoConnect": false })).await;
    let (commands, events, task) = start(config);
    until(&events, "idle", |seen| {
        seen == [ConnectionStatus::Disconnected]
    })
    .await;
    commands
        .send(NetCommand::Session(Command::Join {
            network: NetworkId("acc".into()),
            channel: "#x".into(),
            key: None,
        }))
        .unwrap();
    commands.send(NetCommand::Connect).unwrap();
    let mut socket = accept(&listener).await;
    let burst = read_until(&mut socket, "USER kim 0 * kim\r\n").await;
    assert_eq!(burst, "CAP LS 302\r\nNICK kim\r\nUSER kim 0 * kim\r\n");
    until(&events, "registering", |seen| {
        seen.ends_with(&[ConnectionStatus::Connecting, ConnectionStatus::Registering])
    })
    .await;
    let offline = events.events().iter().any(|event| {
        matches!(event, Event::Line(line) if line.kind == LineKind::Error
            && line.text == "Not connected to Acceptance: cannot join #x"
            && line.buffer == BufferKey { network: NetworkId("acc".into()), target: Target::Server })
    });
    assert!(offline);
    commands.send(NetCommand::Shutdown).unwrap();
    let quit = read_until(&mut socket, "\r\n").await;
    assert_eq!(quit, "QUIT :Subspace closed\r\n");
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
}

#[tokio::test]
async fn w03_an_unreadable_password_file_fails_without_retrying() {
    let (listener, config) = listener(serde_json::json!({
        "sasl": { "username": "kim", "passwordFile": "/no/such/cleanroom.secret" }
    }))
    .await;
    let (commands, events, task) = start(config);
    let _socket = tokio::time::timeout(WAIT, listener.accept())
        .await
        .unwrap()
        .unwrap();
    until(&events, "failed", |seen| {
        matches!(seen.last(), Some(ConnectionStatus::Failed { .. }))
    })
    .await;
    let seen = statuses(&events);
    assert_eq!(
        seen[..2],
        [ConnectionStatus::Connecting, ConnectionStatus::Registering]
    );
    let Some(ConnectionStatus::Failed { error }) = seen.last() else {
        unreachable!()
    };
    assert!(error.contains("/no/such/cleanroom.secret"), "{error}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(statuses(&events).len(), 3, "no retry after a failure");
    commands.send(NetCommand::Shutdown).unwrap();
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
}

#[tokio::test]
async fn w04_losses_back_off_and_registration_resets_the_count() {
    let (listener, config) = listener(serde_json::json!({})).await;
    let (commands, events, task) = start(config);
    let socket = accept(&listener).await;
    drop(socket);
    until(&events, "first loss", |seen| {
        seen.iter().any(|status| {
            matches!(
                status,
                ConnectionStatus::Reconnecting {
                    attempt: 1,
                    delay_secs: 1,
                    ..
                }
            )
        })
    })
    .await;
    let socket = accept(&listener).await;
    drop(socket);
    until(&events, "second loss", |seen| {
        seen.iter().any(|status| {
            matches!(
                status,
                ConnectionStatus::Reconnecting {
                    attempt: 2,
                    delay_secs: 2,
                    ..
                }
            )
        })
    })
    .await;
    let socket = accept_and_welcome(&listener, &events).await;
    drop(socket);
    until(&events, "loss after registration", |seen| {
        matches!(
            seen.last(),
            Some(ConnectionStatus::Reconnecting {
                attempt: 1,
                delay_secs: 1,
                ..
            })
        )
    })
    .await;
    commands.send(NetCommand::Shutdown).unwrap();
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
}

#[tokio::test]
async fn w05_disconnect_quits_with_the_reason_and_does_not_reconnect() {
    let (listener, config) = listener(serde_json::json!({})).await;
    let (commands, events, task) = start(config);
    let mut socket = accept_and_welcome(&listener, &events).await;
    commands
        .send(NetCommand::Session(Command::Disconnect {
            network: NetworkId("acc".into()),
            reason: "later".into(),
        }))
        .unwrap();
    let sent = read_until(&mut socket, "QUIT").await;
    assert!(sent.ends_with("QUIT later\r\n"), "{sent:?}");
    until(&events, "disconnected", |seen| {
        seen.last() == Some(&ConnectionStatus::Disconnected)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        !statuses(&events)
            .iter()
            .any(|status| matches!(status, ConnectionStatus::Reconnecting { .. }))
    );
    commands.send(NetCommand::Connect).unwrap();
    let again = tokio::time::timeout(WAIT, listener.accept()).await;
    assert!(again.is_ok(), "Connect reconnects");
    commands.send(NetCommand::Shutdown).unwrap();
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
}
