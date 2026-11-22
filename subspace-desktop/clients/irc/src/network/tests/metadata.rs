use std::time::Duration;

use tokio::io::AsyncWriteExt as _;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::super::{EventSink, NetCommand, run_with};
use crate::connection::Timeouts;
use crate::model::{Command, Event, NetworkConfig, NetworkId};

async fn wait_status(events: &EventSink, cleared: bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let found = events.events().iter().any(|event| {
                matches!(event, Event::NickStatus { nick, status, .. } if nick == "agent" && status.is_none() == cleared)
            });
            if found { return; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
}

#[tokio::test]
async fn socket_loss_and_owner_disconnect_emit_clears() {
    for disconnect in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let config: NetworkConfig = serde_json::from_value(serde_json::json!({
            "id": "local", "host": "127.0.0.1", "port": listener.local_addr().unwrap().port(),
            "tls": false, "nickname": "subspace"
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
        let (mut socket, _) = listener.accept().await.unwrap();
        socket.write_all(b":s CAP * LS :batch draft/metadata-2\r\n:s CAP subspace ACK :batch draft/metadata-2\r\n:s 001 subspace :Welcome\r\n:s 761 * agent status * :Thinking\r\n").await.unwrap();
        wait_status(&events, false).await;
        if disconnect {
            commands
                .send(NetCommand::Session(Command::Disconnect {
                    network: NetworkId("local".into()),
                    reason: "Done".into(),
                }))
                .unwrap();
            wait_status(&events, true).await;
        }
        drop(socket);
        wait_status(&events, true).await;
        commands.send(NetCommand::Shutdown).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(5), task)
                .await
                .is_ok()
        );
        let cleared = events
            .events()
            .iter()
            .filter(|event| matches!(event, Event::NickStatus { status: None, .. }))
            .count();
        assert_eq!(cleared, 1);
    }
}
