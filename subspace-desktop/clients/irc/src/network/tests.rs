use std::time::Duration;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::{EventSink, NetCommand, backoff, run_with};
use crate::connection::Timeouts;
use crate::model::{Command, ConnectionStatus, Event, NetworkConfig, NetworkId};

pub(super) const WAIT: Duration = Duration::from_secs(5);

mod credential;
mod metadata;
mod send_all;

#[tokio::test]
async fn a_nickname_exceeding_nicklen_ends_registration_as_failed() {
    let (listener, mut config) = silent_server().await;
    config.tls = false;
    let (commands, receiver) = mpsc::unbounded_channel();
    let events = EventSink::default();
    let task = tokio::spawn(run_with(
        config,
        receiver,
        events.clone(),
        Timeouts::default(),
    ));
    let (mut socket, _) = tokio::time::timeout(WAIT, listener.accept())
        .await
        .unwrap()
        .unwrap();
    socket
        .write_all(
            b":srv 005 * NICKLEN=8 :are supported\r\n:srv 432 * subspace :Invalid nickname\r\n",
        )
        .await
        .unwrap();
    let mut received = String::new();
    tokio::time::timeout(WAIT, socket.read_to_string(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        received,
        "CAP LS 302\r\nNICK subspace\r\nUSER subspace 0 * subspace\r\n"
    );
    let failed = status(&events, &mut 0, |status| {
        matches!(status, ConnectionStatus::Failed { .. })
    })
    .await;
    assert!(matches!(failed, ConnectionStatus::Failed { error } if error.contains("8-byte limit")));
    commands.send(NetCommand::Shutdown).unwrap();
    tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
}

#[test]
fn backoff_doubles_up_to_a_minute() {
    let delays: Vec<u64> = (1..=8).map(backoff).collect();
    assert_eq!(delays, [1, 2, 4, 8, 16, 32, 60, 60]);
}

/// A TLS network on a local listener that accepts and never answers, so
/// the handshake hangs.
pub(super) async fn silent_server() -> (TcpListener, NetworkConfig) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = serde_json::from_value(serde_json::json!({
        "id": "local", "host": "127.0.0.1", "port": port, "tls": true,
        "acceptInvalidCerts": true, "nickname": "subspace"
    }))
    .unwrap();
    (listener, config)
}

/// The first status reported after `seen` events that matches.
pub(super) async fn status(
    events: &EventSink,
    seen: &mut usize,
    matches: impl Fn(&ConnectionStatus) -> bool,
) -> ConnectionStatus {
    let found = tokio::time::timeout(WAIT, async {
        loop {
            let next =
                events.events().iter().skip(*seen).enumerate().find_map(
                    |(index, event)| match event {
                        Event::Status { status, .. } if matches(status) => {
                            Some((index, status.clone()))
                        }
                        _ => None,
                    },
                );
            if let Some((index, status)) = next {
                *seen += index + 1;
                return status;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    found.unwrap()
}

#[tokio::test]
async fn disconnect_abandons_a_hanging_tls_handshake() {
    let (listener, config) = silent_server().await;
    let (commands, receiver) = mpsc::unbounded_channel();
    let events = EventSink::default();
    let task = tokio::spawn(run_with(
        config,
        receiver,
        events.clone(),
        Timeouts::default(),
    ));
    let (_socket, _) = tokio::time::timeout(WAIT, listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut seen = 0;
    status(&events, &mut seen, |status| {
        *status == ConnectionStatus::Connecting
    })
    .await;
    commands
        .send(NetCommand::Session(Command::Disconnect {
            network: NetworkId("local".into()),
            reason: String::new(),
        }))
        .unwrap();
    let after = status(&events, &mut seen, |_| true).await;
    assert_eq!(after, ConnectionStatus::Disconnected);
    commands.send(NetCommand::Shutdown).unwrap();
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
}

#[tokio::test]
async fn a_stalled_handshake_times_out_and_retries() {
    let (listener, config) = silent_server().await;
    let (commands, receiver) = mpsc::unbounded_channel();
    let events = EventSink::default();
    let timeouts = Timeouts {
        tcp: Duration::from_secs(5),
        tls: Duration::from_millis(100),
    };
    let task = tokio::spawn(run_with(config, receiver, events.clone(), timeouts));
    let _socket = tokio::time::timeout(WAIT, listener.accept()).await.unwrap();
    let mut seen = 0;
    let retry = status(&events, &mut seen, |status| {
        matches!(status, ConnectionStatus::Reconnecting { .. })
    })
    .await;
    let ConnectionStatus::Reconnecting { attempt, error, .. } = retry else {
        panic!("expected a reconnect");
    };
    assert_eq!(attempt, 1);
    assert!(error.contains("timed out"), "{error}");
    commands.send(NetCommand::Shutdown).unwrap();
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
}

/// Registers the client with `draft/chathistory`, joins `#c` and waits for
/// the history request the join sends.
async fn register_and_await_history(socket: &mut tokio::net::TcpStream) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let burst = ":srv CAP * LS :batch draft/chathistory\r\n\
        :srv CAP subspace ACK :batch draft/chathistory\r\n\
        :srv 001 subspace :Welcome\r\n\
        :srv 005 subspace CHANTYPES=# :are supported\r\n\
        :subspace!u@h JOIN #c\r\n";
    socket.write_all(burst.as_bytes()).await.unwrap();
    let mut received = Vec::new();
    tokio::time::timeout(WAIT, async {
        while !String::from_utf8_lossy(&received).contains("CHATHISTORY LATEST #c") {
            let mut chunk = [0; 512];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0, "the client closed the connection");
            received.extend_from_slice(&chunk[..count]);
        }
    })
    .await
    .unwrap();
}

/// What the client sends on a network that does not connect at startup and
/// configures `#c`, after `before` arrives while idle, then `Connect` and a
/// welcome.
async fn sent_after_welcome(before: Option<Command>) -> String {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config: NetworkConfig = serde_json::from_value(serde_json::json!({
        "id": "local", "host": "127.0.0.1", "port": port, "tls": false,
        "nickname": "subspace", "channels": ["#c"], "autoConnect": false
    }))
    .unwrap();
    let (commands, receiver) = mpsc::unbounded_channel();
    let task = tokio::spawn(run_with(
        config,
        receiver,
        EventSink::default(),
        Timeouts::default(),
    ));
    if let Some(command) = before {
        commands.send(NetCommand::Session(command)).unwrap();
    }
    commands.send(NetCommand::Connect).unwrap();
    let (mut socket, _) = tokio::time::timeout(WAIT, listener.accept())
        .await
        .unwrap()
        .unwrap();
    // The session handles lines in order, so the PONG for a PING sent after
    // the welcome follows everything the welcome made it send, autojoin
    // included. Reading to the PONG needs no fixed wait, even under load.
    socket
        .write_all(b":srv 001 subspace :Welcome\r\nPING :after-welcome\r\n")
        .await
        .unwrap();
    let mut received = Vec::new();
    tokio::time::timeout(WAIT, async {
        while !String::from_utf8_lossy(&received).contains("after-welcome") {
            let mut chunk = [0; 512];
            let count = socket.read(&mut chunk).await.unwrap();
            assert!(count > 0, "the client closed the connection");
            received.extend_from_slice(&chunk[..count]);
        }
    })
    .await
    .unwrap();
    commands.send(NetCommand::Shutdown).unwrap();
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
    String::from_utf8_lossy(&received).into_owned()
}

#[tokio::test]
async fn a_channel_forgotten_while_disconnected_is_not_joined_at_connect() {
    assert!(
        sent_after_welcome(None).await.contains("JOIN #c"),
        "the configured channel is joined"
    );
    let forget = Command::Forget {
        buffer: crate::model::BufferKey {
            network: NetworkId("local".into()),
            target: crate::model::Target::Channel("#C".into()),
        },
    };
    let sent = sent_after_welcome(Some(forget)).await;
    assert!(!sent.contains("JOIN"), "{sent}");
}

#[tokio::test]
async fn a_dropped_connection_ends_its_pending_history_request() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let config: NetworkConfig = serde_json::from_value(serde_json::json!({
        "id": "local", "host": "127.0.0.1", "port": port, "tls": false, "nickname": "subspace"
    }))
    .unwrap();
    let (commands, receiver) = mpsc::unbounded_channel();
    let events = EventSink::default();
    let task = tokio::spawn(run_with(
        config,
        receiver,
        events.clone(),
        Timeouts::default(),
    ));
    let (mut socket, _) = tokio::time::timeout(WAIT, listener.accept())
        .await
        .unwrap()
        .unwrap();
    register_and_await_history(&mut socket).await;
    drop(socket);
    let mut seen = 0;
    status(&events, &mut seen, |status| {
        matches!(status, ConnectionStatus::Reconnecting { .. })
    })
    .await;
    let ended = Event::History {
        buffer: crate::model::BufferKey {
            network: NetworkId("local".into()),
            target: crate::model::Target::Channel("#c".into()),
        },
        lines: Vec::new(),
        complete: false,
    };
    assert!(
        events.events().contains(&ended),
        "the pending request was not answered"
    );
    commands.send(NetCommand::Shutdown).unwrap();
    assert!(tokio::time::timeout(WAIT, task).await.is_ok());
}
