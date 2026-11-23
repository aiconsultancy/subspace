//! A network whose SASL login comes from a credential source: asked at
//! every connect, and retried, never failed, when it cannot issue one.

use tokio::io::AsyncReadExt as _;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::super::{EventSink, NetCommand, run_with};
use super::{WAIT, silent_server, status};
use crate::connection::Timeouts;
use crate::model::{ConnectionStatus, NetworkConfig};

/// A plaintext network on a local listener whose SASL login comes from
/// `source`, registering first as `placeholder`.
async fn issued_server(source: crate::model::CredentialSource) -> (TcpListener, NetworkConfig) {
    let (listener, mut config) = silent_server().await;
    config.tls = false;
    config.nickname = "placeholder".into();
    config.sasl = Some(crate::model::Sasl {
        username: "placeholder".into(),
        password: String::new(),
        password_file: None,
        external: false,
        credential: Some(source),
    });
    (listener, config)
}

#[tokio::test]
async fn a_credential_source_is_asked_again_at_each_reconnect() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let asked = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&asked);
    let source = crate::model::CredentialSource::new("fixture-api", move || {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(crate::model::IssuedLogin {
            nickname: "ada".into(),
            username: "ada@desk1".into(),
            password: "issued-fixture".into(),
        })
    });
    let (listener, config) = issued_server(source).await;
    let (commands, receiver) = mpsc::unbounded_channel();
    let events = EventSink::default();
    let task = tokio::spawn(run_with(
        config,
        receiver,
        events.clone(),
        Timeouts::default(),
    ));
    for connect in 1..=2 {
        let (mut socket, _) = tokio::time::timeout(WAIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut received = vec![0; 256];
        let count = tokio::time::timeout(WAIT, socket.read(&mut received))
            .await
            .unwrap()
            .unwrap();
        let burst = String::from_utf8_lossy(&received[..count]).into_owned();
        assert!(burst.contains("NICK ada\r\n"), "{burst}");
        assert_eq!(asked.load(Ordering::SeqCst), connect);
        drop(socket);
    }
    commands.send(NetCommand::Shutdown).unwrap();
    tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
}

#[tokio::test]
async fn a_login_the_source_cannot_issue_is_retried_never_failed() {
    let source = crate::model::CredentialSource::new("fixture-api", || {
        Err("Subspace API could not be reached for an IRC sign-in".into())
    });
    let (listener, config) = issued_server(source).await;
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
    let mut seen = 0;
    let retry = status(&events, &mut seen, |status| {
        matches!(
            status,
            ConnectionStatus::Reconnecting { .. } | ConnectionStatus::Failed { .. }
        )
    })
    .await;
    assert!(
        matches!(&retry, ConnectionStatus::Reconnecting { attempt: 1, error, .. }
            if error.contains("could not be reached")),
        "{retry:?}"
    );
    let mut received = String::new();
    tokio::time::timeout(WAIT, socket.read_to_string(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(received, "", "nothing is sent without a login");
    commands.send(NetCommand::Shutdown).unwrap();
    tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
}
