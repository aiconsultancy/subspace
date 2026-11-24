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
            password: crate::model::IssuedPassword::new("issued-fixture"),
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

/// A source that issues `ada` and counts fetches and refusals.
fn counted(
    asked: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    refused: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> crate::model::CredentialSource {
    use std::sync::atomic::Ordering;
    let (asked, refused) = (std::sync::Arc::clone(asked), std::sync::Arc::clone(refused));
    crate::model::CredentialSource::new("fixture-api", move || {
        asked.fetch_add(1, Ordering::SeqCst);
        Ok(crate::model::IssuedLogin {
            nickname: "ada".into(),
            username: "ada@desk1".into(),
            password: crate::model::IssuedPassword::new("issued-fixture"),
        })
    })
    .on_refused(move || {
        refused.fetch_add(1, Ordering::SeqCst);
    })
}

#[tokio::test]
async fn a_server_that_registers_without_the_sign_in_is_left_and_retried() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncWriteExt as _;

    let (asked, refused) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (listener, config) = issued_server(counted(&asked, &refused)).await;
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
        .write_all(b":srv CAP * LS :server-time\r\n:srv 001 ada :Welcome\r\n")
        .await
        .unwrap();
    let mut seen = 0;
    let retry = status(&events, &mut seen, |status| {
        matches!(
            status,
            ConnectionStatus::Reconnecting { .. }
                | ConnectionStatus::Failed { .. }
                | ConnectionStatus::Connected { .. }
        )
    })
    .await;
    assert!(
        matches!(&retry, ConnectionStatus::Reconnecting { attempt: 1, error, .. }
            if error.contains("without accepting the sign-in")),
        "{retry:?}"
    );
    let mut received = String::new();
    tokio::time::timeout(WAIT, socket.read_to_string(&mut received))
        .await
        .unwrap()
        .unwrap();
    assert!(
        received.ends_with("QUIT :Sign-in required\r\n"),
        "{received}"
    );
    assert!(!received.contains("JOIN"), "{received}");
    assert!(!received.contains("issued-fixture"), "no SASL was offered");
    assert_eq!(refused.load(Ordering::SeqCst), 1, "the source is told");
    commands.send(NetCommand::Shutdown).unwrap();
    tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
}

#[tokio::test]
async fn a_certificate_the_client_cannot_verify_never_asks_for_a_login() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::AsyncWriteExt as _;

    let (asked, refused) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let (listener, mut config) = issued_server(counted(&asked, &refused)).await;
    config.tls = true;
    config.accept_invalid_certs = false;
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
    // Not a TLS server: the handshake fails before anything is signed in.
    let _ = socket.write_all(b"220 not tls\r\n").await;
    drop(socket);
    let mut seen = 0;
    status(&events, &mut seen, |status| {
        matches!(
            status,
            ConnectionStatus::Reconnecting { .. } | ConnectionStatus::Failed { .. }
        )
    })
    .await;
    assert_eq!(asked.load(Ordering::SeqCst), 0, "no login was fetched");
    commands.send(NetCommand::Shutdown).unwrap();
    tokio::time::timeout(WAIT, task).await.unwrap().unwrap();
}

#[test]
fn a_login_goes_without_tls_only_to_this_computer() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    let (asked, refused) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let mut config: NetworkConfig = serde_json::from_value(serde_json::json!({
        "id": "fleet", "host": "localhost", "port": 6667, "tls": false, "nickname": "ada"
    }))
    .unwrap();
    config.sasl = Some(crate::model::Sasl {
        username: "ada".into(),
        password: String::new(),
        password_file: None,
        external: false,
        credential: Some(counted(&asked, &refused)),
    });
    let exposes = |peer: &str| super::super::exposes_login(&config, peer.parse().unwrap());
    assert!(!exposes("127.0.0.1"));
    assert!(!exposes("::1"));
    assert!(
        exposes("192.0.2.10"),
        "a name that resolved elsewhere gets nothing"
    );
    let mut tls = config.clone();
    tls.tls = true;
    assert!(!super::super::exposes_login(
        &tls,
        "192.0.2.10".parse().unwrap()
    ));
    let mut saved = config.clone();
    if let Some(sasl) = saved.sasl.as_mut() {
        sasl.credential = None;
        sasl.password = "saved".into();
    }
    assert!(
        !super::super::exposes_login(&saved, "192.0.2.10".parse().unwrap()),
        "a saved network keeps its own settings"
    );
}
